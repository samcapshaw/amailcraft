//! Session-only CalDAV connection discovery. No secrets enter persisted state.
use crate::{CmdError, CmdResult, CommandSpec, Notice, Session, always};
use serde_json::{Value, json};

#[derive(Default)]
pub(crate) struct State {
    #[cfg(not(target_arch = "wasm32"))]
    job: Option<Job>,
    #[cfg(not(target_arch = "wasm32"))]
    next: u64,
}
#[cfg(not(target_arch = "wasm32"))]
struct Job {
    id: u64,
    rx: std::sync::mpsc::Receiver<Value>,
    result: Option<Value>,
}
pub fn specs() -> Vec<CommandSpec> {
    vec![CommandSpec {
        id: "caldav.discover",
        label: "Discover CalDAV calendars…",
        location: "Calendar › Connections",
        shortcut: "",
        params: r#"{} opens setup; {"endpoint":string,"username":string,"password":string} starts read-only discovery; {"job":number} polls it. Credentials are not saved. No event sync or writes."#,
        mutates: false,
        enabled: always,
        run: discover,
    }]
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
#[cfg(test)]
mod tests {
    use super::*;
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
