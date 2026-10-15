//! Graph mail jobs use the existing worker and normalized string-ID mail events. Each account has its own
//! cursors/shadow; IMAP and JMAP credentials and synchronization state are never reused.
use super::{
    Plan,
    jmap::{JEmail, JmapEvent},
    native::{Event, Events, Outgoing},
};
use crate::{Session, accounts::JmapShadow};
use mailcraft_model::{Account, AccountId, FolderKind};
use mailcraft_net::jmap::JmapMailbox;
use mailcraft_net::{NetError, graph::GraphConn};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Default)]
pub(super) struct GraphJob {
    receive: bool,
    calendars: bool,
    contacts: bool,
    now: mailcraft_model::DateTime,
    pub(super) writes: Vec<crate::cmd::connected::PendingWrite>,
    cursors: BTreeMap<String, String>,
    shadow: BTreeMap<String, JmapShadow>,
    updates: Vec<(String, JmapShadow, JmapShadow)>,
    destroy: Vec<String>,
}

pub(super) fn build_job(s: &Session, a: &Account, plan: &Plan) -> Result<Option<GraphJob>, String> {
    let state = s.sync.prefs.data.graph.get(&a.id.0).cloned().unwrap_or_default();
    let mut job = GraphJob {
        writes: s
            .sync
            .prefs
            .data
            .graph_writes
            .iter()
            .filter(|op| {
                op.account == a.id
                    && op.status == "queued"
                    && op.mailbox == a.address
                    && a.incoming.as_ref().is_some_and(|i| i.host == op.endpoint)
                    && crate::cmd::connected::write_allowed(s, op)
                    && crate::accounts::account_options(s, a.id).graph_editing
            })
            .cloned()
            .collect(),
        receive: plan.receive,
        calendars: crate::accounts::account_options(s, a.id).graph_calendars,
        contacts: crate::accounts::account_options(s, a.id).graph_contacts,
        now: s.now(),
        cursors: serde_json::from_str(&state.email_state).unwrap_or_default(),
        shadow: state.shadow,
        ..Default::default()
    };
    let mut present = BTreeSet::new();
    for m in s.mailbox.mail.values() {
        let Some(f) = s.mailbox.folder(m.folder).filter(|f| f.account == a.id && f.kind == FolderKind::Mail)
        else {
            continue;
        };
        let Some(id) = m.remote_id.as_deref() else { continue };
        present.insert(id.to_owned());
        let Some(previous) = job.shadow.get(id) else { continue };
        let Some(folder) = &f.remote_name else {
            return Err(
                "Graph cannot move a message into a folder that has not been created on the server.".into()
            );
        };
        let mut keywords = Vec::new();
        if m.read {
            keywords.push("$seen".to_owned());
        }
        if m.is_flagged() {
            keywords.push("$flagged".to_owned());
        }
        if matches!(m.flag, mailcraft_model::FlagState::Complete { .. }) {
            keywords.push("$complete".to_owned());
        }
        if m.is_draft {
            keywords.push("$draft".to_owned());
        }
        keywords.sort();
        let next = JmapShadow { mailbox: folder.clone(), keywords };
        if &next != previous {
            job.updates.push((id.to_owned(), previous.clone(), next));
        }
    }
    job.destroy = job.shadow.keys().filter(|id| !present.contains(*id)).cloned().collect();
    Ok((job.receive || !job.writes.is_empty() || !job.updates.is_empty() || !job.destroy.is_empty())
        .then_some(job))
}

pub(super) fn run(
    account: AccountId,
    conn: &GraphConn,
    job: Option<&GraphJob>,
    outgoing: &[Outgoing],
    events: &Events,
    cancel: &AtomicBool,
) -> Option<String> {
    let emit = |ev| events.send(Event::Jmap { account, ev });
    let stopped = || cancel.load(Ordering::SeqCst);
    let result = (|| -> Result<(), NetError> {
        let client = conn.connect()?;
        let mut write_error = None;
        if let Some(job) = job {
            for op in &job.writes {
                if stopped() {
                    return Ok(());
                }
                let result =
                    client.write_connected(&op.kind, &op.remote_id, &op.action, &op.etag, &op.payload);
                let error = result.as_ref().err().map(ToString::to_string);
                if !events.send(Event::GraphWrite {
                    account,
                    id: op.id,
                    result: result.map_err(|e| e.to_string()),
                }) {
                    return Ok(());
                }
                if let Some(error) = error {
                    write_error = Some(error);
                    break;
                }
            }
        }
        if let Some(job) = job.filter(|j| !j.receive && !j.writes.is_empty()) {
            fetch_services(account, &client, job, events, cancel)?;
            return match write_error {
                Some(error) => Err(NetError::Refused(error)),
                None => Ok(()),
            };
        }

        if let Some(job) = job {
            for (id, old, new) in &job.updates {
                if stopped() {
                    return Ok(());
                }
                if old.mailbox != new.mailbox {
                    client.move_message(id, &new.mailbox)?;
                }
                let has = |kw: &str| new.keywords.iter().any(|k| k == kw);
                client.update(
                    id,
                    has("$seen"),
                    if has("$complete") {
                        "complete"
                    } else if has("$flagged") {
                        "flagged"
                    } else {
                        "notFlagged"
                    },
                )?;
                if !emit(JmapEvent::Updated { id: id.clone(), shadow: new.clone() }) {
                    return Ok(());
                }
            }
            for id in &job.destroy {
                if stopped() {
                    return Ok(());
                }
                match client.delete(id) {
                    Ok(()) | Err(NetError::Http { status: 404, .. }) => {}
                    Err(error) => return Err(error),
                }
                if !emit(JmapEvent::Destroyed { ids: vec![id.clone()] }) {
                    return Ok(());
                }
            }
        }
        for message in outgoing {
            if stopped() {
                return Ok(());
            }
            let result = client.send_mime(&message.raw);
            let failed = result.clone().err();
            if !events.send(Event::Sent {
                account,
                item: message.item,
                result: result.map_err(|e| e.to_string()),
            }) {
                return Ok(());
            }
            if let Some(error) = failed {
                return Err(error);
            }
        }
        let Some(job) = job.filter(|j| j.receive) else { return Ok(()) };
        events.send(Event::Progress { account, text: "Synchronizing Microsoft 365 mail…".into() });
        let folders = client.folders()?;
        let mut roles = BTreeMap::new();
        for (name, role) in [
            ("inbox", "inbox"),
            ("sentitems", "sent"),
            ("drafts", "drafts"),
            ("deleteditems", "trash"),
            ("junkemail", "junk"),
            ("archive", "archive"),
        ] {
            if stopped() {
                return Ok(());
            }
            match client.well_known_folder(name) {
                Ok(value) => {
                    if let Some(id) = value.get("id").and_then(Value::as_str) {
                        roles.insert(id.to_owned(), role.to_owned());
                    }
                }
                Err(NetError::Http { status: 404, .. }) => {}
                Err(error) => return Err(error),
            }
        }
        let mut normalized = Vec::new();
        for f in &folders {
            let id = text(f, "id")?;
            normalized.push(JmapMailbox {
                id: id.to_owned(),
                name: text(f, "displayName")?.into(),
                parent_id: f.get("parentFolderId").and_then(Value::as_str).map(str::to_owned),
                role: roles.get(id).cloned(),
                is_subscribed: true,
                ..Default::default()
            });
        }
        if !emit(JmapEvent::Mailboxes { list: normalized, state: String::new() }) {
            return Ok(());
        }
        let mut cursors = job.cursors.clone();
        let mut removed = BTreeSet::new();
        let mut changed: BTreeMap<String, (String, Value)> = BTreeMap::new();
        for folder in &folders {
            if stopped() {
                return Ok(());
            }
            let folder_id = text(folder, "id")?;
            let cursor = cursors.get(folder_id).map(String::as_str);
            let (page, reset) = match client.messages_delta(folder_id, cursor) {
                Ok(page) => (page, cursor.is_none()),
                Err(NetError::Http { status: 410, .. }) => (client.messages_delta(folder_id, None)?, true),
                Err(error) => return Err(error),
            };
            let mut present = BTreeSet::new();
            if changed.len().saturating_add(removed.len()).saturating_add(page.values.len()) > 100_000 {
                return Err(NetError::TooLarge("Graph synchronization batch".into()));
            }
            for value in page.values {
                let id = text(&value, "id")?.to_owned();
                if value.get("@removed").is_some() {
                    present.remove(&id);
                    if changed.get(&id).is_some_and(|(source, _)| source == folder_id) {
                        changed.remove(&id);
                    }
                    removed.insert(id);
                    continue;
                }
                present.insert(id.clone());
                changed.insert(id, (folder_id.to_owned(), value));
            }
            if reset {
                removed.extend(
                    job.shadow
                        .iter()
                        .filter(|(_, sh)| sh.mailbox == folder_id)
                        .filter(|(id, _)| !present.contains(*id))
                        .map(|(id, _)| id.clone()),
                );
            }
            let delta = page
                .delta
                .ok_or_else(|| NetError::Protocol("Graph delta response has no continuation state".into()))?;
            cursors.insert(folder_id.to_owned(), delta);
        }
        for (id, (folder, value)) in &changed {
            if stopped() {
                return Ok(());
            }
            let raw = if job.shadow.contains_key(id) { None } else { Some(client.mime(id)?) };
            let mut keywords = Vec::new();
            if value.get("isRead").and_then(Value::as_bool).unwrap_or(false) {
                keywords.push("$seen".into());
            }
            if value.get("isDraft").and_then(Value::as_bool).unwrap_or(false) {
                keywords.push("$draft".into());
            }
            if value.pointer("/flag/flagStatus").and_then(Value::as_str) == Some("flagged") {
                keywords.push("$flagged".into());
            }
            if value.pointer("/flag/flagStatus").and_then(Value::as_str) == Some("complete") {
                keywords.push("$complete".into());
            }
            keywords.sort();
            if !emit(JmapEvent::Emails {
                list: vec![JEmail {
                    id: id.clone(),
                    mailboxes: vec![folder.clone()],
                    keywords,
                    size: raw.as_ref().map(|b| b.len() as u64),
                    raw,
                }],
            }) {
                return Ok(());
            }
        }
        let gone = removed.into_iter().filter(|id| !changed.contains_key(id)).collect();
        if !emit(JmapEvent::Gone { ids: gone }) {
            return Ok(());
        }
        let active: BTreeSet<_> =
            folders.iter().filter_map(|f| f.get("id").and_then(Value::as_str)).collect();
        cursors.retain(|id, _| active.contains(id.as_str()));
        let state = serde_json::to_string(&cursors).map_err(|e| NetError::Protocol(e.to_string()))?;
        emit(JmapEvent::EmailState { state });
        fetch_services(account, &client, job, events, cancel)?;
        if let Some(error) = write_error {
            return Err(NetError::Refused(error));
        }
        Ok(())
    })();
    if stopped() { Some("Send/Receive was cancelled.".into()) } else { result.err().map(|e| e.to_string()) }
}
fn fetch_services(
    account: AccountId,
    client: &mailcraft_net::graph::GraphClient,
    job: &GraphJob,
    events: &Events,
    cancel: &AtomicBool,
) -> Result<(), NetError> {
    let stopped = || cancel.load(Ordering::SeqCst);
    let mut errors = Vec::new();
    for (enabled, kind, label) in
        [(job.calendars, FolderKind::Calendar, "calendars"), (job.contacts, FolderKind::Contacts, "contacts")]
    {
        if !enabled || stopped() {
            continue;
        }
        events.send(Event::Progress { account, text: format!("Synchronizing Microsoft 365 {label}…") });
        match super::graph_pim::fetch(client, kind, job.now, cancel) {
                Ok(snapshot) => { if !events.send(Event::GraphPim { account, snapshot }) { return Ok(()); } }
                Err(error) => errors.push(format!("Microsoft 365 {label}: {error}. Check the connected-service authorization; cached data has been kept.")),
            }
    }
    if !errors.is_empty() {
        return Err(NetError::Refused(errors.join(" ")));
    }
    Ok(())
}

fn text<'a>(value: &'a Value, key: &str) -> Result<&'a str, NetError> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| NetError::Protocol(format!("Graph object has no {key}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::accounts::prefs;
    use mailcraft_model::{FolderRole, ItemId};
    use serde_json::json;
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::TcpListener;
    use std::time::Duration;

    fn fixture() -> (String, std::thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let root = format!("{base}/v1.0/users/owner%40example.net");
        let thread = std::thread::spawn(move || {
            for _ in 0..18 {
                let (mut stream, _) = listener.accept().unwrap();
                stream.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut first = String::new();
                reader.read_line(&mut first).unwrap();
                let words: Vec<_> = first.split_whitespace().collect();
                let method = words[0];
                let path = words[1];
                let mut length = 0;
                let mut authorized = false;
                loop {
                    let mut line = String::new();
                    reader.read_line(&mut line).unwrap();
                    if line == "\r\n" || line.is_empty() {
                        break;
                    }
                    if let Some((name, value)) = line.split_once(':') {
                        if name.eq_ignore_ascii_case("content-length") {
                            length = value.trim().parse().unwrap();
                        }
                        if name.eq_ignore_ascii_case("authorization") {
                            authorized = value.trim() == "Bearer fixture-token";
                        }
                    }
                }
                assert!(authorized);
                let mut body = vec![0; length];
                reader.read_exact(&mut body).unwrap();
                let (status,body)=match (method,path) {
                    ("GET",p) if p.ends_with("calendars?$top=100") => (200,json!({"value":[{"id":"cal-id","name":"Team calendar"}]}).to_string()),
                    ("GET",p) if p.contains("/calendarView?") => (200,json!({"value":[{"id":"event-id","subject":"Design review","start":{"dateTime":"2026-10-14T09:00:00.0000000","timeZone":"UTC"},"end":{"dateTime":"2026-10-14T10:00:00","timeZone":"UTC"}}]}).to_string()),
                    ("GET",p) if p.ends_with("contacts?$top=100") => (200,json!({"value":[{"id":"contact-id","givenName":"Maya","surname":"Chen","emailAddresses":[{"address":"maya@example.net"}]}]}).to_string()),
                    ("GET",p) if p.ends_with("mailFolders?$top=100") => (200,json!({"value":[{"id":"inbox-id","displayName":"Inbox","childFolderCount":0},{"id":"sent-id","displayName":"Sent","childFolderCount":0}]}).to_string()),
                    ("GET",p) if p.contains("mailFolders/inbox?") => (200,json!({"id":"inbox-id"}).to_string()),
                    ("GET",p) if p.contains("mailFolders/sentitems?") => (200,json!({"id":"sent-id"}).to_string()),
                    ("GET",p) if p.contains("messages/delta") => {
                        let values=if p.contains("inbox-id") { json!([{"id":"m1","isRead":false,"flag":{"flagStatus":"notFlagged"}},{"id":"m2","isRead":false,"flag":{"flagStatus":"notFlagged"}}]) } else { json!([]) };
                        (200,json!({"value":values,"@odata.deltaLink":format!("{root}/mailFolders/{}/messages/delta?delta=next",if p.contains("inbox-id") {"inbox-id"}else{"sent-id"})}).to_string())
                    },
                    ("GET",p) if p.ends_with("/$value") => (200,format!("From: Sender <sender@example.net>\r\nTo: owner@example.net\r\nSubject: Synthetic {}\r\nMessage-ID: <{}@example.net>\r\n\r\nLocal Graph fixture",if p.contains("m1") {"one"}else{"two"},if p.contains("m1") {"m1"}else{"m2"})),
                    ("GET",_) => (404,"{}".into()),
                    ("PATCH",_) => { let value:Value=serde_json::from_slice(&body).unwrap(); assert_eq!(value["isRead"],true); assert_eq!(value["flag"]["flagStatus"],"flagged"); (200,"{}".into()) },
                    ("POST",p) if p.ends_with("/move") => { let value:Value=serde_json::from_slice(&body).unwrap(); assert_eq!(value["destinationId"],"sent-id"); (201,"{}".into()) },
                    ("POST",p) if p.ends_with("/sendMail") => { assert!(!body.is_empty()); (202,String::new()) },
                    ("DELETE",p) => { assert!(p.ends_with("/messages/m2")); (204,String::new()) },
                    _ => (400,"{}".into()),
                };
                write!(stream,"HTTP/1.1 {status} Fixture\r\nContent-Length: {}\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{body}",body.len()).unwrap();
            }
        });
        (base, thread)
    }

    #[test]
    fn background_graph_mail_roundtrip_keeps_local_ids_and_independent_state() {
        let (base, thread) = fixture();
        let mut s = crate::sample::sample_session();
        let value=s.run("accounts.add",&json!({"kind":"graph","graphCalendars":true,"graphContacts":true,"address":"owner@example.net","incoming":{"host":format!("{base}/v1.0"),"allowPlaintextAuth":true},"oauthToken":"fixture-token"})).unwrap();
        let account = AccountId(value["account"].as_u64().unwrap() as u32);
        assert_eq!(s.send_receive(&[account], Plan::all()).unwrap(), 1);
        assert!(s.wait_for_sync(Duration::from_secs(10)));
        let cal = s
            .mailbox
            .folders
            .values()
            .find(|f| f.account == account && f.kind == FolderKind::Calendar && f.graph_source.is_some())
            .unwrap();
        assert_eq!(cal.name, "Team calendar");
        assert_eq!(
            s.mailbox.appointments[&cal.graph_source.as_ref().unwrap().items["event-id"]].subject,
            "Design review"
        );
        let contacts = s
            .mailbox
            .folders
            .values()
            .find(|f| f.account == account && f.kind == FolderKind::Contacts && f.graph_source.is_some())
            .unwrap();
        assert_eq!(
            s.mailbox.contacts[&contacts.graph_source.as_ref().unwrap().items["contact-id"]].emails,
            ["maya@example.net"]
        );
        let inbox = s.mailbox.role_folder(account, FolderRole::Inbox).unwrap();
        let sent = s.mailbox.role_folder(account, FolderRole::SentItems).unwrap();
        let ids: Vec<_> = s.mailbox.items_in(inbox).iter().map(|m| m.id).collect();
        assert_eq!(ids.len(), 2, "{:?}", s.sync_status());
        let m1 = *ids
            .iter()
            .find(|id| s.mailbox.mail.get(id).unwrap().remote_id.as_deref() == Some("m1"))
            .unwrap();
        let m2 = *ids
            .iter()
            .find(|id| s.mailbox.mail.get(id).unwrap().remote_id.as_deref() == Some("m2"))
            .unwrap();
        s.run("nav.selectFolder", &json!({"folder":inbox.0})).unwrap();
        s.run("nav.selectItem", &json!({"id":m1.0})).unwrap();
        s.run("mail.markRead", &json!({"ids":[m1.0]})).unwrap();
        s.run("mail.flag", &json!({"ids":[m1.0],"kind":"today"})).unwrap();
        s.run("mail.move", &json!({"ids":[m1.0],"folder":sent.0})).unwrap();
        s.run("mail.permanentDelete", &json!({"ids":[m2.0]})).unwrap();
        let mut outgoing = s.mailbox.mail.get(&m1).unwrap().clone();
        outgoing.id = ItemId(s.mailbox.alloc_id());
        outgoing.folder = s.mailbox.role_folder(account, FolderRole::Outbox).unwrap();
        outgoing.remote_id = None;
        outgoing.sent = None;
        let outgoing_id = outgoing.id;
        s.mailbox.mail.insert(outgoing_id, outgoing);
        assert_eq!(
            s.send_receive(&[account], Plan { send: true, receive: false, user: true, folders: None })
                .unwrap(),
            1
        );
        assert!(s.wait_for_sync(Duration::from_secs(10)));
        thread.join().unwrap();
        assert_eq!(s.mailbox.mail.get(&m1).unwrap().remote_id.as_deref(), Some("m1"));
        assert_eq!(s.mailbox.mail.get(&m1).unwrap().folder, sent);
        assert_eq!(s.mailbox.mail.get(&outgoing_id).unwrap().folder, sent);
        assert!(!s.mailbox.mail.contains_key(&m2));
        assert!(!prefs(&mut s).jmap.contains_key(&account.0));
        let state = prefs(&mut s).graph.get(&account.0).unwrap();
        assert_eq!(state.shadow.len(), 1);
        assert_eq!(state.shadow["m1"].mailbox, "sent-id");
        assert!(state.email_state.contains("delta=next"));
    }

    #[test]
    fn graph_grants_and_shared_accounts_do_not_reuse_imap_authorization() {
        let mut s = crate::sample::sample_session();
        let account=s.run("accounts.add",&json!({"kind":"graph","address":"owner@example.net","incoming":{"host":"https://graph.microsoft.com/v1.0"}})).unwrap()["account"].as_u64().unwrap();
        assert!(
            s.run(
                "accounts.oauthSignIn",
                &json!({"account":account,"provider":"microsoft","clientId":"fixture-client"})
            )
            .is_err()
        );
        let linked = s
            .run("accounts.addShared", &json!({"account":account,"address":"team@example.net"}))
            .unwrap()["account"]
            .as_u64()
            .unwrap();
        let linked = s.mailbox.account(AccountId(linked as u32)).unwrap();
        assert_eq!(linked.kind, mailcraft_model::AccountKind::Graph);
        assert!(!linked.can_send());
        assert!(s.secrets.get(&crate::accounts::secret_key(linked.id, false)).is_none());
    }
}

#[cfg(test)]
mod pending_tests {
    use super::*;
    use mailcraft_model::{FolderRole, ItemId};
    use serde_json::json;
    #[test]
    fn graph_delta_keeps_edits_made_after_dispatch_and_refuses_unsupported_folder_writes() {
        let mut s = crate::sample::sample_session();
        let value=s.run("accounts.add",&json!({"kind":"graph","address":"owner@example.net","incoming":{"host":"https://graph.microsoft.com/v1.0"}})).unwrap();
        let account = AccountId(value["account"].as_u64().unwrap() as u32);
        let inbox = s.mailbox.role_folder(account, FolderRole::Inbox).unwrap();
        let sent = s.mailbox.role_folder(account, FolderRole::SentItems).unwrap();
        s.mailbox.folders.get_mut(&inbox).unwrap().remote_name = Some("inbox-id".into());
        s.mailbox.folders.get_mut(&sent).unwrap().remote_name = Some("sent-id".into());
        let event = |raw| JmapEvent::Emails {
            list: vec![JEmail {
                id: "m1".into(),
                mailboxes: vec!["inbox-id".into()],
                keywords: vec![],
                raw,
                size: None,
            }],
        };
        super::super::jmap::apply(
            &mut s,
            account,
            event(Some(b"From: sender@example.net\r\nSubject: Fixture\r\n\r\nBody".to_vec())),
            &mut Vec::new(),
        );
        let id: ItemId = s.mailbox.items_in(inbox)[0].id;
        let m = s.mailbox.mail.get_mut(&id).unwrap();
        m.read = true;
        m.folder = sent;
        super::super::jmap::apply(&mut s, account, event(None), &mut Vec::new());
        assert!(s.mailbox.mail[&id].read);
        assert_eq!(s.mailbox.mail[&id].folder, sent);
        let job = build_job(&s, s.mailbox.account(account).unwrap(), &Plan::all()).unwrap().unwrap();
        assert_eq!(job.updates.len(), 1);
        assert_eq!(job.updates[0].2.mailbox, "sent-id");
        assert!(
            s.run("folder.new", &json!({"parent":inbox.0,"name":"Unsupported"}))
                .unwrap_err()
                .to_string()
                .contains("Graph")
        );
        assert!(s.run("mail.copy", &json!({"ids":[id.0],"folder":inbox.0})).is_err());
    }
}
