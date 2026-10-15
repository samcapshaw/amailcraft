//! CalDAV discovery and saved read-only calendar snapshots. Passwords use the secret store.
use crate::{CmdError, CmdResult, CommandSpec, Notice, Session, always};
use serde_json::{Value, json};

#[derive(Default)]
pub(crate) struct State {
    #[cfg(not(target_arch = "wasm32"))]
    job: Option<Job>,
    #[cfg(not(target_arch = "wasm32"))]
    last_schedule: Option<std::time::Instant>,
    #[cfg(not(target_arch = "wasm32"))]
    refresh_revision: u64,
    #[cfg(not(target_arch = "wasm32"))]
    next: u64,
    #[cfg(not(target_arch = "wasm32"))]
    completed: Option<(u64, Value)>,
    #[cfg(not(target_arch = "wasm32"))]
    refresh: Option<(mailcraft_model::FolderId, String, String, Job)>,
}
#[cfg(not(target_arch = "wasm32"))]
struct Job {
    id: u64,
    rx: std::sync::mpsc::Receiver<Value>,
    result: Option<Value>,
}
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec {
            id: "caldav.configure",
            label: "Calendar connection settings",
            location: "Calendar › Connections",
            shortcut: "",
            params: r#"{folder,password?,automatic?,refreshMin?}: update credentials or refresh schedule"#,
            mutates: false,
            enabled: always,
            run: configure,
        },
        CommandSpec {
            id: "caldav.disconnect",
            label: "Disconnect calendar",
            location: "Calendar › Connections",
            shortcut: "",
            params: r#"{folder}: remove saved password and connection; keep cached events as a local calendar"#,
            mutates: false,
            enabled: always,
            run: disconnect,
        },
        CommandSpec {
            id: "caldav.refreshDue",
            label: "Refresh due calendars",
            location: "Calendar › Connections",
            shortcut: "",
            params: "{}: start the oldest due automatic source if idle",
            mutates: false,
            enabled: always,
            run: refresh_due,
        },
        CommandSpec {
            id: "caldav.add",
            label: "Save CalDAV calendar",
            location: "Calendar › Connections",
            shortcut: "",
            params: r#"{url,username,password,name?}: save a read-only calendar and its credential"#,
            mutates: true,
            enabled: always,
            run: add,
        },
        CommandSpec {
            id: "caldav.refresh",
            label: "Refresh CalDAV calendar",
            location: "Calendar › Connections",
            shortcut: "",
            params: r#"{folder}: start read-only event download; {job}: poll and apply snapshot"#,
            mutates: false,
            enabled: always,
            run: refresh,
        },
        CommandSpec {
            id: "caldav.sources",
            label: "CalDAV sources",
            location: "Calendar › Connections",
            shortcut: "",
            params: "{}: list saved sources and refresh status",
            mutates: false,
            enabled: always,
            run: sources,
        },
        CommandSpec {
            id: "caldav.discover",
            label: "Discover CalDAV calendars…",
            location: "Calendar › Connections",
            shortcut: "",
            params: r#"{} opens setup; {"endpoint":string,"username":string,"password":string} starts read-only discovery; {"job":number} polls it. Credentials are not saved. Discovery does not save credentials or download events."#,
            mutates: false,
            enabled: always,
            run: discover,
        },
    ]
}
fn discover(s: &mut Session, p: &Value) -> CmdResult {
    if p.as_object().is_some_and(|o| o.is_empty()) {
        s.notices.push(Notice::OpenDialog("caldavDiscovery".into(), json!({})));
        return Ok(Value::Null);
    }
    #[cfg(target_arch = "wasm32")]
    {
        let _ = &s.caldav;
        Err(CmdError::Disabled("CalDAV discovery is available in the desktop app.".into()))
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        if let Some(id) = p.get("job").and_then(Value::as_u64) {
            let job = s
                .caldav
                .job
                .as_mut()
                .filter(|j| j.id == id)
                .ok_or_else(|| CmdError::Params("This CalDAV lookup is no longer available.".into()))?;
            return Ok(poll(job));
        }
        if let Some(job) = s.caldav.job.as_mut() {
            poll(job);
            if job.result.is_none() {
                return Err(CmdError::Disabled(
                    "A CalDAV lookup is already running. Wait for it to finish before starting another."
                        .into(),
                ));
            }
        }
        let field = |key| {
            p.get(key)
                .and_then(Value::as_str)
                .ok_or_else(|| CmdError::Params(format!("Missing CalDAV {key}.")))
        };
        let endpoint = field("endpoint")?.trim();
        let username = field("username")?.trim();
        let password = field("password")?;
        mailcraft_net::caldav::validate(endpoint, username, password)
            .map_err(|e| CmdError::Params(e.to_string()))?;
        let (endpoint, username, password) = (endpoint.to_owned(), username.to_owned(), password.to_owned());
        let (tx, rx) = std::sync::mpsc::channel();
        let id = s
            .caldav
            .next
            .checked_add(1)
            .ok_or_else(|| CmdError::Failed("CalDAV lookup limit reached. Reopen the app.".into()))?;
        std::thread::Builder::new()
            .name("mailcraft-caldav".into())
            .spawn(move || {
                let r = std::panic::catch_unwind(|| {
                    mailcraft_net::caldav::discover(&endpoint, &username, &password)
                });
                let value = match r {
                    Ok(Ok(calendars)) => json!({"status":"done", "calendars":calendars}),
                    Ok(Err(error)) => json!({"status":"done", "error":error.to_string()}),
                    Err(_) => json!({"status":"done", "error":"The CalDAV lookup stopped unexpectedly."}),
                };
                let _ = tx.send(value);
            })
            .map_err(|_| CmdError::Failed("Could not start CalDAV discovery.".into()))?;
        s.caldav.next = id;
        s.caldav.job = Some(Job { id, rx, result: None });
        Ok(json!({"status":"running", "job":id}))
    }
}
#[cfg(not(target_arch = "wasm32"))]
fn poll(job: &mut Job) -> Value {
    if job.result.is_none() {
        match job.rx.try_recv() {
            Ok(v) => job.result = Some(v),
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                job.result = Some(json!({"status":"done", "error":"CalDAV discovery stopped."}))
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {}
        }
    }
    job.result.clone().unwrap_or_else(|| json!({"status":"running", "job":job.id}))
}
/// Keeps refreshes progressing even after setup is closed; sync never enters undo history.
pub fn poll_background(s: &mut Session) -> bool {
    #[cfg(not(target_arch = "wasm32"))]
    {
        if let Some((_, _, _, job)) = s.caldav.refresh.as_ref() {
            let id = job.id;
            let _ = s.run("caldav.refresh", &json!({"job":id}));
        }
        if s.caldav.last_schedule.is_none_or(|t| t.elapsed() >= std::time::Duration::from_secs(30)) {
            s.caldav.last_schedule = Some(std::time::Instant::now());
            let _ = s.run("caldav.refreshDue", &json!({}));
        }
        s.caldav.refresh.is_some()
    }
    #[cfg(target_arch = "wasm32")]
    {
        let _ = s;
        false
    }
}
/// Whether the desktop should wake periodically even without mail accounts.
pub fn scheduled(s: &Session) -> bool {
    s.mailbox.folders.keys().any(|&f| {
        let p = super::calendar::props::read(&s.mailbox, f);
        p.caldav_auto && p.caldav_username.is_some()
    })
}

#[cfg(not(target_arch = "wasm32"))]
fn source(
    s: &Session,
    p: &Value,
) -> Result<(mailcraft_model::FolderId, mailcraft_model::calendar::CalendarProps), CmdError> {
    let folder = mailcraft_model::FolderId(
        p.get("folder")
            .and_then(Value::as_u64)
            .ok_or_else(|| CmdError::Params("Choose a calendar connection.".into()))?,
    );
    let props = super::calendar::props::read(&s.mailbox, folder);
    if props.caldav_username.is_none() || props.subscription.is_none() {
        return Err(CmdError::Params("Not a CalDAV connection.".into()));
    }
    Ok((folder, props))
}
#[cfg(not(target_arch = "wasm32"))]
fn persist(s: &mut Session, before: &mailcraft_model::Mailbox) {
    s.journal(crate::session::diff_ops(before, &s.mailbox));
    for entry in s.undo.undo.iter_mut().chain(s.undo.redo.iter_mut()) {
        carry_over(&s.mailbox, &mut entry.before);
    }
}
fn configure(s: &mut Session, p: &Value) -> CmdResult {
    #[cfg(target_arch = "wasm32")]
    {
        let _ = (s, p);
        Err(CmdError::Disabled("Calendar connections require the desktop app.".into()))
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let (folder, mut props) = source(s, p)?;
        let automatic = match p.get("automatic") {
            None => None,
            Some(v) => {
                Some(v.as_bool().ok_or_else(|| CmdError::Params("automatic must be true or false.".into()))?)
            }
        };
        let minutes = match p.get("refreshMin") {
            None => None,
            Some(v) => Some(
                v.as_u64()
                    .filter(|v| (15..=10080).contains(v))
                    .ok_or_else(|| CmdError::Params("Refresh interval must be 15–10080 minutes.".into()))?
                    as u32,
            ),
        };
        let password = match p.get("password") {
            None => None,
            Some(v) => Some(v.as_str().ok_or_else(|| CmdError::Params("Enter an app password.".into()))?),
        };
        if let Some(password) = password {
            let sub = props
                .subscription
                .as_ref()
                .ok_or_else(|| CmdError::Params("Missing calendar address.".into()))?;
            mailcraft_net::caldav::validate(
                &sub.url,
                props.caldav_username.as_deref().unwrap_or_default(),
                password,
            )
            .map_err(|e| CmdError::Params(e.to_string()))?;
            props.caldav_revision = props
                .caldav_revision
                .checked_add(1)
                .ok_or_else(|| CmdError::Failed("Connection revision limit reached.".into()))?;
            s.secrets.set(&secret_key(folder), password).map_err(CmdError::Failed)?;
            if let Some(sub) = props.subscription.as_mut() {
                sub.last_fetch = None;
                sub.last_error = None;
            }
        }
        if let Some(automatic) = automatic {
            props.caldav_auto = automatic;
        }
        if let Some(minutes) = minutes
            && let Some(sub) = props.subscription.as_mut()
        {
            sub.refresh_min = minutes;
        }
        let before = s.mailbox.clone();
        super::calendar::props::write(&mut s.mailbox, folder, &props);
        persist(s, &before);
        Ok(json!({"folder":folder.0,"saved":true,"passwordUpdated":password.is_some()}))
    }
}
fn disconnect(s: &mut Session, p: &Value) -> CmdResult {
    #[cfg(target_arch = "wasm32")]
    {
        let _ = (s, p);
        Err(CmdError::Disabled("Calendar connections require the desktop app.".into()))
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let (folder, _) = source(s, p)?;
        s.secrets.remove(&secret_key(folder)).map_err(CmdError::Failed)?;
        let before = s.mailbox.clone();
        super::calendar::props::update(&mut s.mailbox, folder, |x| {
            x.caldav_username = None;
            x.caldav_auto = false;
            x.caldav_revision = 0;
            x.subscription = None;
            x.source = mailcraft_model::calendar::CalendarSource::Local;
            x.read_only = false;
        });
        persist(s, &before);
        Ok(json!({"folder":folder.0,"disconnected":true,"eventsKept":true}))
    }
}
#[cfg(not(target_arch = "wasm32"))]
fn due(s: &Session) -> Option<mailcraft_model::FolderId> {
    if s.view.work_offline {
        return None;
    }
    let now = s.now().unix;
    s.mailbox
        .folders
        .keys()
        .filter_map(|&folder| {
            let p = super::calendar::props::read(&s.mailbox, folder);
            if !p.caldav_auto || p.caldav_username.is_none() {
                return None;
            }
            let sub = p.subscription?;
            let every = i64::from(sub.refresh_min.clamp(15, 10080)) * 60;
            sub.last_fetch
                .is_none_or(|t| t > now || now.saturating_sub(t) >= every)
                .then_some((sub.last_fetch.unwrap_or(i64::MIN), folder))
        })
        .min()
        .map(|(_, f)| f)
}
fn refresh_due(s: &mut Session, _: &Value) -> CmdResult {
    #[cfg(target_arch = "wasm32")]
    {
        let _ = s;
        Ok(Value::Null)
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        if s.caldav.refresh.is_some() {
            return Ok(json!({"busy":true}));
        }
        let Some(folder) = due(s) else {
            return Ok(json!({"due":false}));
        };
        match s.run("caldav.refresh", &json!({"folder":folder.0})) {
            Ok(v) => Ok(v),
            Err(e) => {
                let result = super::calendar::subscribe::apply_background(s, folder, Err(e.to_string()));
                Ok(json!({"status":"done","result":result}))
            }
        }
    }
}
/// Connection management is not an editing step; undo must not resurrect credentials or old schedules.
pub(crate) fn carry_over(live: &mailcraft_model::Mailbox, restored: &mut mailcraft_model::Mailbox) {
    let folders: Vec<_> = restored.folders.keys().copied().collect();
    for f in folders {
        let old = super::calendar::props::read(restored, f);
        if old.caldav_username.is_none() || live.folder(f).is_none() {
            continue;
        }
        let current = super::calendar::props::read(live, f);
        if current.caldav_username.is_none() {
            restored.next_id = restored.next_id.max(live.next_id);
            let cached: Vec<_> = live
                .appointments
                .values()
                .filter(|a| {
                    a.folder == f && restored.appointments.get(&a.id).is_none_or(|old| old.folder == f)
                })
                .cloned()
                .collect();
            restored.appointments.retain(|_, a| a.folder != f);
            for appointment in cached {
                restored.appointments.insert(appointment.id, appointment);
            }
        }

        super::calendar::props::update(restored, f, |x| {
            x.caldav_username = current.caldav_username.clone();
            x.caldav_auto = current.caldav_auto;
            x.caldav_revision = current.caldav_revision;
            x.subscription = current.subscription.clone();
            x.source = current.source;
            x.read_only = current.read_only;
        });
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn secret_key(folder: mailcraft_model::FolderId) -> String {
    format!("caldav.{}.password", folder.0)
}
fn add(s: &mut Session, p: &Value) -> CmdResult {
    #[cfg(target_arch = "wasm32")]
    {
        let _ = (s, p);
        Err(CmdError::Disabled("CalDAV connections require the desktop app.".into()))
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        use super::calendar::{self, props};
        use mailcraft_model::calendar::{CalendarSource, Subscription};
        let field = |key| {
            p.get(key).and_then(Value::as_str).ok_or_else(|| CmdError::Params(format!("Missing {key}.")))
        };
        let url = field("url")?.trim();
        let username = field("username")?.trim();
        let password = field("password")?;
        mailcraft_net::caldav::validate(url, username, password)
            .map_err(|e| CmdError::Params(e.to_string()))?;
        if s.mailbox.folders.keys().any(|&f| {
            let x = props::read(&s.mailbox, f);
            x.caldav_username.as_deref() == Some(username) && x.subscription.is_some_and(|v| v.url == url)
        }) {
            return Err(CmdError::Params("This calendar is already connected.".into()));
        }
        let parent = calendar::calendar_parent(s, p)?;
        let before = s.mailbox.clone();
        let folder = calendar::create_calendar(
            s,
            parent,
            p.get("name").and_then(Value::as_str).unwrap_or("CalDAV calendar"),
        )?;
        if let Err(e) = s.secrets.set(&secret_key(folder), password) {
            s.mailbox = before;
            return Err(CmdError::Failed(e));
        }
        props::update(&mut s.mailbox, folder, |x| {
            x.source = CalendarSource::CalDav;
            x.read_only = true;
            x.caldav_username = Some(username.into());
            x.subscription = Some(Subscription { url: url.into(), refresh_min: 60, ..Default::default() });
        });
        Ok(json!({"folder":folder.0}))
    }
}
fn sources(s: &mut Session, _: &Value) -> CmdResult {
    let sources: Vec<Value> = s.mailbox.folders.values().filter_map(|f| {
        let p=super::calendar::props::read(&s.mailbox,f.id);
        let username=p.caldav_username?; let sub=p.subscription?;
        #[cfg(not(target_arch="wasm32"))]
        let syncing=s.caldav.refresh.as_ref().is_some_and(|(id,_,_,_)|*id==f.id);
        #[cfg(target_arch="wasm32")]
        let syncing=false;
        Some(json!({"folder":f.id.0,"name":f.name,"username":username,"url":sub.url,"lastUpdate":sub.last_update,"lastAttempt":sub.last_fetch,"lastError":sub.last_error,"automatic":p.caldav_auto,"refreshMin":sub.refresh_min,"syncing":syncing}))
    }).collect();
    Ok(json!({"sources":sources,"credentialStore":s.secrets.location(),"encrypted":s.secrets.encrypted()}))
}
#[cfg(not(target_arch = "wasm32"))]
fn validate_events(data: &str, offset: i32) -> Result<(), String> {
    use mailcraft_formats::ics;
    let top = ics::parse::parse(data);
    let events: Vec<_> =
        top.iter().filter(|c| c.name == "VCALENDAR").flat_map(|c| c.children_named("VEVENT")).collect();
    if events.len() > 2000
        || events
            .iter()
            .any(|e| e.prop("RRULE").is_some() || e.prop("RDATE").is_some() || e.prop("UID").is_none())
    {
        return Err("Server returned unsupported or unexpanded events. Cached calendar was kept.".into());
    }
    let parsed = ics::read(data.as_bytes(), ics::ImportOptions { local_offset_min: offset })
        .map_err(|e| e.to_string())?;
    if events
        .iter()
        .flat_map(|e| &e.props)
        .filter_map(|p| p.param("TZID"))
        .any(|z| parsed.zones.get(z).is_none())
    {
        return Err("Calendar names an unsupported time zone. Cached calendar was kept.".into());
    }
    if parsed.events.len() != events.len() {
        return Err("Some calendar events could not be read. Cached calendar was kept.".into());
    }
    Ok(())
}

fn refresh(s: &mut Session, p: &Value) -> CmdResult {
    #[cfg(target_arch = "wasm32")]
    {
        let _ = (s, p);
        Err(CmdError::Disabled("CalDAV refresh requires the desktop app.".into()))
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        use super::calendar::{props, subscribe};
        if let Some(id) = p.get("job").and_then(Value::as_u64) {
            if let Some((_, v)) = s.caldav.completed.as_ref().filter(|(j, _)| *j == id) {
                return Ok(v.clone());
            }
            let (folder, url, username, job) = s
                .caldav
                .refresh
                .as_mut()
                .filter(|(_, _, _, j)| j.id == id)
                .ok_or_else(|| CmdError::Params("This refresh is no longer available.".into()))?;
            let value = poll(job);
            if value.get("status").and_then(Value::as_str) != Some("done") {
                return Ok(value);
            }
            let (folder, url, username) = (*folder, url.clone(), username.clone());
            s.caldav.refresh = None;
            let current = props::read(&s.mailbox, folder);
            if current.caldav_revision != s.caldav.refresh_revision
                || current.caldav_username.as_deref() != Some(&username)
                || !current.subscription.is_some_and(|x| x.url == url)
            {
                return Err(CmdError::Disabled("Calendar connection changed; refresh discarded.".into()));
            }
            let outcome = match value.get("data").and_then(Value::as_str) {
                Some(data) => validate_events(data, s.tz_offset_min).map(|()| subscribe::Fetched::Data {
                    body: data.as_bytes().to_vec(),
                    etag: None,
                    last_modified: None,
                }),
                None => Err(value
                    .get("error")
                    .and_then(Value::as_str)
                    .unwrap_or("Calendar download failed.")
                    .to_owned()),
            };
            let applied = subscribe::apply_background(s, folder, outcome);
            let done = json!({"status":"done","result":applied});
            s.caldav.completed = Some((id, done.clone()));
            return Ok(done);
        }
        if s.view.work_offline {
            return Err(CmdError::Disabled(
                "You are working offline. Go online to refresh this calendar.".into(),
            ));
        }
        if s.caldav.refresh.is_some() {
            return Err(CmdError::Disabled(
                "A calendar refresh is already running. Finish it before starting another.".into(),
            ));
        }
        let folder = mailcraft_model::FolderId(
            p.get("folder")
                .and_then(Value::as_u64)
                .ok_or_else(|| CmdError::Params("Choose a saved calendar.".into()))?,
        );
        let source = props::read(&s.mailbox, folder);
        s.caldav.refresh_revision = source.caldav_revision;
        let username =
            source.caldav_username.ok_or_else(|| CmdError::Params("Not a CalDAV calendar.".into()))?;
        let url =
            source.subscription.ok_or_else(|| CmdError::Params("Missing calendar address.".into()))?.url;
        let password = s.secrets.get(&secret_key(folder)).ok_or_else(|| {
            CmdError::Failed("Calendar password is unavailable in this profile's credential store.".into())
        })?;
        let stamp = |unix| {
            let d =
                mailcraft_model::dates::LocalTime::of(mailcraft_model::DateTime { unix, offset_min: 0 }, 0);
            format!("{:04}{:02}{:02}T{:02}{:02}{:02}Z", d.year, d.month, d.day, d.hour, d.minute, d.second)
        };
        let now = s.now().unix;
        let start = stamp(now.saturating_sub(90 * 86400));
        let end = stamp(now.saturating_add(365 * 86400));
        let id = s
            .caldav
            .next
            .checked_add(1)
            .ok_or_else(|| CmdError::Failed("Refresh limit reached. Reopen the app.".into()))?;
        let (tx, rx) = std::sync::mpsc::channel();
        let worker_url = url.clone();
        let worker_user = username.clone();
        std::thread::Builder::new()
            .name("mailcraft-caldav-events".into())
            .spawn(move || {
                let r = std::panic::catch_unwind(|| {
                    mailcraft_net::caldav::events(&worker_url, &worker_user, &password, &start, &end)
                });
                let value = match r {
                    Ok(Ok(data)) => match String::from_utf8(data) {
                        Ok(data) => json!({"status":"done","data":data}),
                        Err(_) => json!({"status":"done","error":"Invalid calendar text."}),
                    },
                    Ok(Err(e)) => json!({"status":"done","error":e.to_string()}),
                    Err(_) => json!({"status":"done","error":"Calendar download stopped unexpectedly."}),
                };
                let _ = tx.send(value);
            })
            .map_err(|_| CmdError::Failed("Could not start calendar refresh.".into()))?;
        s.caldav.next = id;
        s.caldav.refresh = Some((folder, url, username, Job { id, rx, result: None }));
        Ok(json!({"status":"running","job":id}))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(not(target_arch = "wasm32"))]
    fn connected(s: &mut Session) -> mailcraft_model::FolderId {
        let v=s.run("caldav.add",&json!({"url":"https://calendar.example.net/team/","username":"person@example.net","password":"synthetic-secret","name":"Team calendar"})).unwrap();
        mailcraft_model::FolderId(v["folder"].as_u64().unwrap())
    }
    #[cfg(not(target_arch = "wasm32"))]
    fn deliver(s: &mut Session, f: mailcraft_model::FolderId, value: Value) -> Value {
        let source = super::super::calendar::props::read(&s.mailbox, f);
        let (tx, rx) = std::sync::mpsc::channel();
        tx.send(value).unwrap();
        s.caldav.refresh = Some((
            f,
            source.subscription.unwrap().url,
            source.caldav_username.unwrap(),
            Job { id: 99, rx, result: None },
        ));
        s.run("caldav.refresh", &json!({"job":99})).unwrap()
    }
    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn saved_connection_is_read_only_and_secret_is_outside_mailbox() {
        let mut s = crate::sample::sample_session();
        let f = connected(&mut s);
        assert!(super::super::calendar::props::is_read_only(&s.mailbox, f));
        assert_eq!(s.secrets.get(&secret_key(f)).as_deref(), Some("synthetic-secret"));
        assert!(!serde_json::to_string(&s.mailbox).unwrap().contains("synthetic-secret"));
        assert!(super::super::calendar::subscribe::subscription(&s, f).is_none());
        assert!(s.run("calendar.refreshCalendar", &json!({"folder":f.0})).is_err());
        assert!(s.run("caldav.add",&json!({"url":"https://calendar.example.net/team/","username":"person@example.net","password":"synthetic-secret"})).is_err());
    }
    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn refresh_keeps_ids_and_preserves_cache_on_failure_or_unexpanded_series() {
        let mut s = crate::sample::sample_session();
        let f = connected(&mut s);
        let data = "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nBEGIN:VEVENT\r\nUID:fixture@example.net\r\nDTSTART:20261015T120000Z\r\nDTEND:20261015T130000Z\r\nSUMMARY:Review\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";
        s.run("caldav.configure", &json!({"folder":f.0,"refreshMin":30})).unwrap();
        let with_publisher_interval = data.replace("VERSION:2.0", "VERSION:2.0\r\nREFRESH-INTERVAL:P1D");
        deliver(&mut s, f, json!({"status":"done","data":with_publisher_interval}));
        assert_eq!(super::super::calendar::props::read(&s.mailbox, f).subscription.unwrap().refresh_min, 30);
        s.run("calendar.newAppointment", &json!({"subject":"Mine","start":"2026-10-20T09:00"})).unwrap();
        let undo_before = s.undo.undo.len();
        let old: Vec<_> = s.mailbox.appointments.values().filter(|a| a.folder == f).cloned().collect();
        assert_eq!(old.len(), 1);
        s.caldav.completed = None;
        deliver(&mut s, f, json!({"status":"done","data":data}));
        assert_eq!(s.mailbox.appointments.get(&old[0].id), old.first());
        assert_eq!(s.undo.undo.len(), undo_before);
        s.undo_last().unwrap();
        assert_eq!(s.mailbox.appointments.get(&old[0].id), old.first());
        s.redo_last().unwrap();
        s.caldav.completed = None;
        deliver(&mut s, f, json!({"status":"done","error":"offline"}));
        assert_eq!(s.mailbox.appointments.get(&old[0].id), old.first());
        s.caldav.completed = None;
        let bad = data.replace("SUMMARY:Review", "RRULE:FREQ=DAILY\r\nSUMMARY:Review");
        let result = deliver(&mut s, f, json!({"status":"done","data":bad}));
        assert!(result["result"]["error"].is_string());
        assert_eq!(s.mailbox.appointments.get(&old[0].id), old.first());
        assert!(
            s.run(
                "calendar.newAppointment",
                &json!({"folder":f.0,"subject":"No write","start":"2026-10-15T12:00:00Z"})
            )
            .is_err()
        );
    }
    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn changed_binding_discards_inflight_result() {
        let mut s = crate::sample::sample_session();
        let f = connected(&mut s);
        let (tx, rx) = std::sync::mpsc::channel();
        tx.send(json!({"status":"done","data":"BEGIN:VCALENDAR\r\nEND:VCALENDAR\r\n"})).unwrap();
        s.caldav.refresh = Some((
            f,
            "https://calendar.example.net/old/".into(),
            "person@example.net".into(),
            Job { id: 7, rx, result: None },
        ));
        let before = s.mailbox.clone();
        assert!(s.run("caldav.refresh", &json!({"job":7})).is_err());
        assert_eq!(s.mailbox, before);
        assert!(s.caldav.refresh.is_none());
    }

    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn credential_changes_invalidate_old_download_and_disconnect_survives_undo() {
        let mut s = crate::sample::sample_session();
        let f = connected(&mut s);
        s.run("calendar.newAppointment", &json!({"subject":"Local work","start":"2026-10-20T09:00"}))
            .unwrap();
        let source = super::super::calendar::props::read(&s.mailbox, f);
        let (tx, rx) = std::sync::mpsc::channel();
        s.caldav.refresh = Some((
            f,
            source.subscription.unwrap().url,
            source.caldav_username.unwrap(),
            Job { id: 8, rx, result: None },
        ));
        s.run(
            "caldav.configure",
            &json!({"folder":f.0,"password":"replacement-secret","automatic":true,"refreshMin":30}),
        )
        .unwrap();
        assert_eq!(s.secrets.get(&secret_key(f)).as_deref(), Some("replacement-secret"));
        tx.send(json!({"status":"done","data":"BEGIN:VCALENDAR\r\nEND:VCALENDAR\r\n"})).unwrap();
        assert!(s.run("caldav.refresh", &json!({"job":8})).is_err());
        s.undo_last().unwrap();
        let props = super::super::calendar::props::read(&s.mailbox, f);
        assert!(props.caldav_auto);
        assert_eq!(props.caldav_revision, 1);
        // Leave the older appointment snapshot in Redo while disconnecting.
        s.run("caldav.disconnect", &json!({"folder":f.0})).unwrap();
        assert!(s.secrets.get(&secret_key(f)).is_none());
        s.undo_last().unwrap();
        assert!(super::super::calendar::props::read(&s.mailbox, f).caldav_username.is_none());
        assert!(!super::super::calendar::props::is_read_only(&s.mailbox, f));
        assert_eq!(s.run("caldav.sources", &json!({})).unwrap()["sources"], json!([]));
        s.redo_last().unwrap(); // Restore the locally retained calendar.
        s.redo_last().unwrap(); // An older redo snapshot must remain disconnected too.
        assert!(super::super::calendar::props::read(&s.mailbox, f).caldav_username.is_none());
        assert!(s.secrets.get(&secret_key(f)).is_none());
    }
    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn scheduler_is_opt_in_and_missing_credentials_back_off_without_network() {
        let mut s = crate::sample::sample_session();
        let f = connected(&mut s);
        assert_eq!(due(&s), None);
        s.run("caldav.configure", &json!({"folder":f.0,"automatic":true,"refreshMin":15})).unwrap();
        assert_eq!(due(&s), Some(f));
        s.view.work_offline = true;
        assert_eq!(due(&s), None);
        assert!(s.run("caldav.refresh", &json!({"folder":f.0})).is_err());
        s.view.work_offline = false;
        s.secrets.remove(&secret_key(f)).unwrap();
        let result = s.run("caldav.refreshDue", &json!({})).unwrap();
        assert!(result["result"]["error"].is_string());
        assert_eq!(due(&s), None);
        super::super::calendar::props::update(&mut s.mailbox, f, |p| {
            if let Some(sub) = p.subscription.as_mut() {
                sub.last_fetch = Some(i64::MIN);
            }
        });
        assert_eq!(due(&s), Some(f));
        s.run("caldav.configure", &json!({"folder":f.0,"automatic":false})).unwrap();
        assert_eq!(due(&s), None);
    }
    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn invalid_configuration_never_changes_credentials_or_calendar() {
        let mut s = crate::sample::sample_session();
        let f = connected(&mut s);
        let before = s.mailbox.clone();
        for p in [
            json!({"folder":f.0,"password":"replacement","refreshMin":0}),
            json!({"folder":f.0,"password":""}),
            json!({"folder":f.0,"automatic":"yes"}),
        ] {
            assert!(s.run("caldav.configure", &p).is_err());
            assert_eq!(s.mailbox, before);
            assert_eq!(s.secrets.get(&secret_key(f)).as_deref(), Some("synthetic-secret"));
        }
    }
    #[test]
    fn opening_and_invalid_input_do_not_change_mail_or_persist_secrets() {
        let mut s = crate::sample::sample_session();
        let before = s.mailbox.clone();
        s.run("caldav.discover", &json!({})).unwrap();
        assert!(
            s.notices.iter().any(|n| matches!(n, Notice::OpenDialog(name, _) if name == "caldavDiscovery"))
        );
        assert!(s.run("caldav.discover", &json!({"endpoint":"http://calendar.example.net", "username":"person", "password":"synthetic"})).is_err());
        assert_eq!(s.mailbox, before);
        assert!(s.secrets.keys().is_empty());
    }
}
