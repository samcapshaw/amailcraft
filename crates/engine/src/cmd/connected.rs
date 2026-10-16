//! Server-confirmed edits and meeting responses. Cached connected items remain immutable; explicit
//! writes are persisted separately and claimed before I/O. Uncertain operations never retry themselves.
use crate::{CmdError, CmdResult, CommandSpec, Notice, Plan, Session, always};
use mailcraft_model::folder::GraphItemMeta;
use mailcraft_model::{AccountId, AccountKind, DateTime, FolderKind, ItemId};
mod lifecycle;
pub use lifecycle::{can_create, can_delete, write_allowed};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingWrite {
    pub id: u64,
    pub account: AccountId,
    pub mailbox: String,
    pub endpoint: String,
    pub item: ItemId,
    pub remote_id: String,
    #[serde(default)]
    pub folder: Option<mailcraft_model::FolderId>,
    pub kind: String,
    pub action: String,
    pub etag: String,
    pub payload: Value,
    pub status: String,
    pub error: String,
}

pub fn specs() -> Vec<CommandSpec> {
    [("graph.createMeeting","New Microsoft 365 meeting",r#"{"folder":number,"confirm":true,"fields"?:{"subject":string,"start":string,"end":string,"location"?:string,"notes"?:string,"attendees":[{"email":string,"type":"required"|"optional"}]}} (no fields opens form; creation sends invitations)"#),
     ("graph.createAppointment","New Microsoft 365 appointment",r#"{"folder":number,"fields"?:{"subject":string,"location"?:string,"start":string,"end":string,"notes"?:string}}"#),
     ("graph.deleteAppointment","Delete Microsoft 365 appointment",r#"{"id":number,"confirm"?:true,"version"?:string}"#),
     ("graph.edit","Edit on Microsoft 365",r#"{"id": number, "fields"?: object} (no fields opens editor; existing event: subject/location/start/end; contact: firstName/lastName/company/jobTitle/email/mobile/notes)"#),
     ("graph.respond","Respond to Microsoft 365 meeting",r#"{"id": number, "response": "accept"|"tentativelyAccept"|"decline", "comment"?: string}"#),
     ("graph.pending","Microsoft 365 pending changes",r#"{} (opens review dialog); {"list":true} returns pending writes"#),
     ("graph.retry","Retry Microsoft 365 change",r#"{"write":number,"reviewed"?:true} (review required for held operations; queued proposals keep their original version)"#),
     ("graph.discard","Discard Microsoft 365 change",r#"{"write":number} (queued/failed/uncertain only)"#)]
    .into_iter().map(|(id,label,params)|CommandSpec {id,label,location:"Microsoft 365 › Connected items",shortcut:"",params,mutates:false,enabled:always,run:match id {"graph.createMeeting"=>lifecycle::create_meeting,"graph.createAppointment"=>lifecycle::create,"graph.deleteAppointment"=>lifecycle::delete,"graph.edit"=>edit,"graph.respond"=>respond,"graph.pending"=>pending_cmd,"graph.retry"=>retry,_=>discard}}).collect()
}
fn item(s: &Session, id: ItemId) -> Result<(AccountId, String, FolderKind, GraphItemMeta), CmdError> {
    let folder = s
        .mailbox
        .appointments
        .get(&id)
        .map(|a| a.folder)
        .or_else(|| s.mailbox.contacts.get(&id).map(|c| c.folder))
        .ok_or_else(|| CmdError::Params("Select a connected event or contact.".into()))?;
    let f = s.mailbox.folder(folder).ok_or_else(|| CmdError::Params("No such folder".into()))?;
    if !s.mailbox.account(f.account).is_some_and(|a| a.kind == AccountKind::Graph) {
        return Err(CmdError::Disabled("This item is not connected to Microsoft 365.".into()));
    }
    let source =
        f.graph_source.as_ref().ok_or_else(|| CmdError::Disabled("This is a local folder.".into()))?;
    let remote = source
        .items
        .iter()
        .find(|(_, local)| **local == id)
        .map(|(remote, _)| remote.clone())
        .ok_or_else(|| CmdError::Disabled("Refresh this connected folder first.".into()))?;
    Ok((f.account, remote.clone(), f.kind, source.metadata.get(&remote).cloned().unwrap_or_default()))
}
pub fn capabilities(s: &Session, id: ItemId) -> (bool, bool) {
    let Ok((account, _, kind, meta)) = item(s, id) else { return (false, false) };
    let opts = crate::accounts::account_options(s, account);
    let grant = opts.oauth.as_ref().is_none_or(|binding| {
        binding.scopes.iter().any(|scope| {
            scope
                == match kind {
                    FolderKind::Calendar => "https://graph.microsoft.com/Calendars.ReadWrite.Shared",
                    FolderKind::Contacts => "https://graph.microsoft.com/Contacts.ReadWrite.Shared",
                    _ => "",
                }
        })
    });
    let enabled = grant
        && meta.event_type != "seriesMaster"
        && opts.graph_editing
        && match kind {
            FolderKind::Calendar => opts.graph_calendars,
            FolderKind::Contacts => opts.graph_contacts,
            _ => false,
        };
    (enabled && meta.can_edit && !meta.etag.is_empty(), enabled && meta.can_respond && !meta.etag.is_empty())
}
pub fn pending_count(s: &Session, account: AccountId) -> usize {
    s.sync.prefs.data.graph_writes.iter().filter(|op| op.account == account).count()
}
pub fn pending(s: &Session) -> Vec<PendingWrite> {
    s.sync.prefs.data.graph_writes.clone()
}
fn target(p: &Value, key: &str) -> Result<u64, CmdError> {
    p.get(key).and_then(Value::as_u64).ok_or_else(|| CmdError::Params(format!("`{key}` must be a number")))
}
fn text(p: &Value, key: &str, max: usize) -> Result<Option<String>, CmdError> {
    match p.get(key) {
        None => Ok(None),
        Some(Value::String(s)) if s.len() <= max && !s.contains('\0') => Ok(Some(s.clone())),
        _ => Err(CmdError::Params(format!("`{key}` must be text of at most {max} bytes"))),
    }
}
fn patch(s: &Session, id: ItemId, kind: FolderKind, fields: &Value) -> Result<Value, CmdError> {
    let fields = fields.as_object().ok_or_else(|| CmdError::Params("`fields` must be an object".into()))?;
    let mut out = serde_json::Map::new();
    for (key, value) in fields {
        let p = json!({(key):value});
        let v = text(&p, key, if key == "notes" { 16 * 1024 } else { 2048 })?
            .ok_or_else(|| CmdError::Params("Missing field".into()))?;
        let (remote, new, old) = match (kind, key.as_str()) {
            (FolderKind::Calendar, "subject") => (
                "subject",
                json!(v),
                json!(s.mailbox.appointments.get(&id).map(|a| a.subject.as_str()).unwrap_or_default()),
            ),
            (FolderKind::Calendar, "location") => (
                "location",
                json!({"displayName":v}),
                json!({"displayName":s.mailbox.appointments.get(&id).map(|a|a.location.as_str()).unwrap_or_default()}),
            ),
            (FolderKind::Calendar, "start" | "end") => {
                let a = s
                    .mailbox
                    .appointments
                    .get(&id)
                    .ok_or_else(|| CmdError::Params("No such event".into()))?;
                if a.all_day {
                    return Err(CmdError::Disabled(
                        "All-day rescheduling is not supported yet. Edit the dates in Outlook.".into(),
                    ));
                }
                let t = DateTime::parse_rfc5322(&v).map_err(|_| {
                    CmdError::Params("Use an ISO date/time including UTC Z or a numeric offset.".into())
                })?;
                if v.len() < 20
                    || !v.contains('T')
                    || !(v.ends_with('Z') || v.get(19..).is_some_and(|s| s.contains('+') || s.contains('-')))
                {
                    return Err(CmdError::Params(
                        "Include the date, seconds and UTC Z or a numeric offset.".into(),
                    ));
                }
                let iso = DateTime::from_unix(t.unix, 0).to_rfc3339();
                let old = if key == "start" { a.start } else { a.end };
                (
                    if key == "start" { "start" } else { "end" },
                    json!({"dateTime":iso.trim_end_matches('Z'),"timeZone":"UTC"}),
                    json!({"dateTime":DateTime::from_unix(old.unix,0).to_rfc3339().trim_end_matches('Z'),"timeZone":"UTC"}),
                )
            }
            (FolderKind::Contacts, key) => {
                let c =
                    s.mailbox.contacts.get(&id).ok_or_else(|| CmdError::Params("No such contact".into()))?;
                match key {
                    "firstName" => ("givenName", json!(v), json!(c.first_name)),
                    "lastName" => ("surname", json!(v), json!(c.last_name)),
                    "company" => ("companyName", json!(v), json!(c.company)),
                    "jobTitle" => ("jobTitle", json!(v), json!(c.job_title)),
                    "mobile" => (
                        "mobilePhone",
                        json!(v),
                        json!(
                            c.phones
                                .iter()
                                .find(|(kind, _)| kind == "Mobile")
                                .map(|(_, n)| n.as_str())
                                .unwrap_or_default()
                        ),
                    ),
                    "notes" => ("personalNotes", json!(v), json!(c.notes)),
                    "email" => {
                        if c.emails.len() > 1 {
                            return Err(CmdError::Disabled("This contact has multiple email addresses; edit them in Outlook to preserve every address.".into()));
                        }
                        if !v.is_empty() && (!v.contains('@') || v.chars().any(char::is_whitespace)) {
                            return Err(CmdError::Params("Enter a valid email address.".into()));
                        }
                        (
                            "emailAddresses",
                            if v.is_empty() { json!([]) } else { json!([{"address":v}]) },
                            json!(c.emails.iter().map(|a| json!({"address":a})).collect::<Vec<_>>()),
                        )
                    }
                    _ => return Err(CmdError::Params(format!("Unsupported contact field `{key}`"))),
                }
            }
            _ => return Err(CmdError::Params(format!("Unsupported event field `{key}`"))),
        };
        if new != old {
            out.insert(remote.into(), new);
        }
    }
    if kind == FolderKind::Calendar {
        let a = s.mailbox.appointments.get(&id).ok_or_else(|| CmdError::Params("No such event".into()))?;
        let instant = |key: &str, default: DateTime| {
            out.get(key)
                .and_then(|v| v.get("dateTime"))
                .and_then(Value::as_str)
                .and_then(|v| DateTime::parse_rfc5322(v).ok())
                .unwrap_or(default)
                .unix
        };
        if instant("end", a.end) < instant("start", a.start) {
            return Err(CmdError::Params("The event ends before it starts.".into()));
        }
    }
    Ok(Value::Object(out))
}
fn persist(s: &mut Session, before: crate::accounts::AccountPrefs) -> Result<(), CmdError> {
    if let Err(error) = crate::accounts::save_prefs_checked(s) {
        s.sync.prefs.data = before;
        return Err(CmdError::Failed(format!("The pending change could not be saved: {error}")));
    }
    Ok(())
}
fn start(s: &mut Session, account: AccountId) {
    s.sync.jmap_dirty.insert(account);
    if !s.view.work_offline {
        let _ = s.send_receive(&[account], Plan { send: false, receive: false, folders: None, user: true });
    }
}
fn enqueue(s: &mut Session, p: &Value, action: &str) -> CmdResult {
    crate::accounts::load_prefs(s);
    let id = ItemId(target(p, "id")?);
    let (account, remote_id, kind, meta) = item(s, id)?;
    let (can_edit, can_respond) = capabilities(s, id);
    if if action == "patch" || action == "delete" { !can_edit } else { !can_respond } {
        return Err(CmdError::Disabled("Enable Microsoft 365 editing, sign in with the editing provider and refresh this folder. The server must grant permission for this operation.".into()));
    }
    if s.sync.prefs.data.graph_writes.iter().any(|op| op.item == id) {
        return Err(CmdError::Disabled(
            "This item already has a pending change. Review or discard it first.".into(),
        ));
    }
    if s.sync.prefs.data.graph_writes.len() >= 128 {
        return Err(CmdError::Disabled("Review pending Microsoft 365 changes before adding more.".into()));
    }
    let payload = if action == "patch" {
        patch(s, id, kind, p.get("fields").unwrap_or(&Value::Null))?
    } else if action == "delete" {
        json!({"subject":s.mailbox.appointments.get(&id).map(|a|a.subject.as_str()).unwrap_or_default()})
    } else {
        json!({"comment":text(p,"comment",4096)?.unwrap_or_default(),"sendResponse":true})
    };
    if payload.as_object().is_some_and(|v| v.is_empty()) {
        return Ok(json!({"id":id.0,"changed":false}));
    }
    if serde_json::to_vec(&payload).map_err(|e| CmdError::Failed(e.to_string()))?.len() > 32 * 1024 {
        return Err(CmdError::Params("This change is too large.".into()));
    }
    let source = s.mailbox.account(account).ok_or_else(|| CmdError::Params("No such account".into()))?;
    let mailbox = source.address.clone();
    let endpoint = source.incoming.as_ref().map(|i| i.host.clone()).unwrap_or_default();
    let version = text(p, "version", 2048)?.unwrap_or(meta.etag);
    if version.is_empty() || version.chars().any(char::is_control) {
        return Err(CmdError::Params("Refresh the item to obtain a valid server version.".into()));
    }
    let before = s.sync.prefs.data.clone();
    let write = s
        .sync
        .prefs
        .data
        .next_graph_write
        .checked_add(1)
        .ok_or_else(|| CmdError::Failed("Pending change identifier limit reached.".into()))?;
    s.sync.prefs.data.next_graph_write = write;
    s.sync.prefs.data.graph_writes.push(PendingWrite {
        id: write,
        account,
        mailbox,
        endpoint,
        item: id,
        remote_id,
        folder: None,
        kind: if kind == FolderKind::Calendar { "event" } else { "contact" }.into(),
        action: action.into(),
        etag: version,
        payload,
        status: "queued".into(),
        error: String::new(),
    });
    persist(s, before)?;
    start(s, account);
    Ok(json!({"id":id.0,"write":write,"queued":true,"offline":s.view.work_offline}))
}
fn edit(s: &mut Session, p: &Value) -> CmdResult {
    if p.get("fields").is_some() {
        return enqueue(s, p, "patch");
    }
    let id = ItemId(target(p, "id")?);
    let (_, _, _, meta) = item(s, id)?;
    if !(capabilities(s, id).0 || capabilities(s, id).1) {
        return Err(CmdError::Disabled("Editing requires the Microsoft 365 editing grant and a fresh server version. Check account settings and refresh.".into()));
    }
    s.notify(Notice::OpenDialog("graphItem".into(), json!({"id":id.0,"version":meta.etag})));
    Ok(json!({"dialog":"graphItem"}))
}
fn respond(s: &mut Session, p: &Value) -> CmdResult {
    if p.get("response").is_none() {
        let id = ItemId(target(p, "id")?);
        if !capabilities(s, id).1 {
            return Err(CmdError::Disabled(
                "This event cannot be answered with the current server permissions.".into(),
            ));
        }
        let (_, _, _, meta) = item(s, id)?;
        s.notify(Notice::OpenDialog("graphItem".into(), json!({"id":id.0,"version":meta.etag})));
        return Ok(json!({"dialog":"graphItem"}));
    }

    let response = p
        .get("response")
        .and_then(Value::as_str)
        .ok_or_else(|| CmdError::Params("Choose accept, tentativelyAccept or decline.".into()))?;
    if !["accept", "tentativelyAccept", "decline"].contains(&response) {
        return Err(CmdError::Params("Choose accept, tentativelyAccept or decline.".into()));
    }
    enqueue(s, p, response)
}
fn pending_cmd(s: &mut Session, p: &Value) -> CmdResult {
    crate::accounts::load_prefs(s);
    if p.get("list").and_then(Value::as_bool) == Some(true) {
        return Ok(json!({"writes":pending(s)}));
    }
    s.notify(Notice::OpenDialog("graphPending".into(), json!({})));
    Ok(json!({"dialog":"graphPending"}))
}
fn retry(s: &mut Session, p: &Value) -> CmdResult {
    crate::accounts::load_prefs(s);
    let write = target(p, "write")?;
    let op = s
        .sync
        .prefs
        .data
        .graph_writes
        .iter()
        .find(|op| op.id == write)
        .cloned()
        .ok_or_else(|| CmdError::Params("No such pending change.".into()))?;
    if op.status != "queued" && p.get("reviewed").and_then(Value::as_bool) != Some(true) {
        return Err(CmdError::Params(
            "Review the current item and previous result in Outlook before retrying; pass reviewed:true."
                .into(),
        ));
    }
    if op.status == "inFlight" || s.sync.busy(op.account) {
        return Err(CmdError::Disabled("Wait for this account's current operation to finish.".into()));
    }
    if op.action == "create" {
        return lifecycle::retry_create(s, &op);
    }
    let (account, remote, _, meta) = item(s, op.item)?;
    if !s.mailbox.account(account).is_some_and(|a| {
        a.address == op.mailbox && a.incoming.as_ref().is_some_and(|i| i.host == op.endpoint)
    }) {
        return Err(CmdError::Disabled(
            "The mailbox connection changed. Discard this proposal and edit the current item.".into(),
        ));
    }

    if op.action == "delete" && !can_delete(s, op.item) {
        return Err(CmdError::Disabled(
            "Only a non-recurring appointment without attendees can be deleted here. Refresh first.".into(),
        ));
    }
    let allowed = capabilities(s, op.item);
    if !(if op.action == "patch" || op.action == "delete" { allowed.0 } else { allowed.1 }) {
        return Err(CmdError::Disabled(
            "Refresh this item and check its editing permissions before retrying.".into(),
        ));
    }
    if account != op.account || remote != op.remote_id {
        return Err(CmdError::Disabled(
            "The source mailbox changed. Discard this proposal and edit the current item.".into(),
        ));
    }
    if op.status == "queued" {
        start(s, account);
        return Ok(json!({"queued":true,"write":write}));
    }
    let before = s.sync.prefs.data.clone();
    if let Some(pending) = s.sync.prefs.data.graph_writes.iter_mut().find(|p| p.id == write) {
        pending.status = "queued".into();
        pending.error.clear();
        pending.etag = meta.etag;
    }
    persist(s, before)?;
    start(s, account);
    Ok(json!({"queued":true,"write":write}))
}
fn discard(s: &mut Session, p: &Value) -> CmdResult {
    crate::accounts::load_prefs(s);
    let id = target(p, "write")?;
    let op = s
        .sync
        .prefs
        .data
        .graph_writes
        .iter()
        .find(|op| op.id == id)
        .ok_or_else(|| CmdError::Params("No such pending change.".into()))?;
    if op.status == "inFlight" {
        return Err(CmdError::Disabled("Wait for the server operation to finish.".into()));
    }
    let before = s.sync.prefs.data.clone();
    s.sync.prefs.data.graph_writes.retain(|op| op.id != id);
    persist(s, before)?;
    Ok(json!({"discarded":id}))
}

pub(crate) fn claim(s: &mut Session, account: AccountId, writes: &[PendingWrite]) -> Result<(), String> {
    if writes.is_empty() {
        return Ok(());
    }
    let before = s.sync.prefs.data.clone();
    for op in &mut s.sync.prefs.data.graph_writes {
        if op.account == account && writes.iter().any(|w| w.id == op.id) {
            op.status = "inFlight".into();
        }
    }
    if let Err(error) = crate::accounts::save_prefs_checked(s) {
        s.sync.prefs.data = before;
        return Err(format!("Cannot record pending-operation state: {error}"));
    }
    Ok(())
}
pub(crate) fn finish(s: &mut Session, account: AccountId, id: u64, result: Result<(), String>) {
    match result {
        Ok(()) => {
            s.sync.prefs.data.graph_writes.retain(|op| !(op.account == account && op.id == id));
            s.notify(Notice::Info(
                "Microsoft 365 accepted the change. Refreshing the connected folder…".into(),
            ));
        }
        Err(error) => {
            if let Some(op) =
                s.sync.prefs.data.graph_writes.iter_mut().find(|op| op.account == account && op.id == id)
            {
                op.status = if error.contains("Outcome uncertain") { "uncertain" } else { "failed" }.into();
                op.error = error.clone();
            }
            s.notify(Notice::Error(format!("Microsoft 365 change was not confirmed: {error}. Review Pending changes; the proposal has been kept.")));
        }
    }
    crate::accounts::save_prefs(s);
}
pub(crate) fn invalidate(s: &mut Session, account: AccountId) {
    settle(s, account);
    for op in &mut s.sync.prefs.data.graph_writes {
        if op.account == account && op.status == "queued" {
            op.status = "failed".into();
            op.error="The mailbox connection changed before this proposal was submitted. Discard it and edit the current item.".into();
        }
    }
}
pub(crate) fn settle(s: &mut Session, account: AccountId) {
    for op in &mut s.sync.prefs.data.graph_writes {
        if op.account == account && op.status == "inFlight" {
            op.status = "uncertain".into();
            op.error =
                "The worker stopped before recording an outcome. Check Outlook before retrying.".into();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mailcraft_model::folder::GraphFolderSource;
    use mailcraft_model::{Appointment, Contact, FolderId, FolderRole};
    use std::collections::BTreeMap;
    fn fixture(s: &mut Session, calendar: bool) -> (AccountId, ItemId, FolderId) {
        s.view.work_offline = true;
        let account=AccountId(s.run("accounts.add",&json!({"kind":"graph","address":"owner@example.net","graphCalendars":true,"graphContacts":true,"graphEditing":true,"incoming":{"host":"https://graph.microsoft.com/v1.0"}})).unwrap()["account"].as_u64().unwrap() as u32);
        let folder = s
            .mailbox
            .role_folder(account, if calendar { FolderRole::Calendar } else { FolderRole::Contacts })
            .unwrap();
        let id = ItemId(s.mailbox.alloc_id());
        if calendar {
            s.mailbox.appointments.insert(
                id,
                Appointment {
                    id,
                    folder,
                    subject: "Review".into(),
                    start: DateTime::from_civil(2026, 10, 14, 9, 0, 0, 0).unwrap(),
                    end: DateTime::from_civil(2026, 10, 14, 10, 0, 0, 0).unwrap(),
                    ..Default::default()
                },
            );
        } else {
            s.mailbox.contacts.insert(
                id,
                Contact {
                    id,
                    folder,
                    first_name: "Maya".into(),
                    emails: vec!["maya@example.net".into()],
                    ..Default::default()
                },
            );
        }
        s.mailbox.folders.get_mut(&folder).unwrap().graph_source = Some(GraphFolderSource {
            remote_id: if calendar { "calendar" } else { "default-contacts" }.into(),
            items: BTreeMap::from([("remote-item".into(), id)]),
            metadata: BTreeMap::from([(
                "remote-item".into(),
                GraphItemMeta {
                    etag: "W/\"one\"".into(),
                    can_edit: true,
                    can_respond: calendar,
                    event_type: "occurrence".into(),
                },
            )]),
            ..Default::default()
        });
        if let Some(store) = s.store.as_mut() {
            store.checkpoint(&s.mailbox).unwrap();
        }
        (account, id, folder)
    }
    #[test]
    fn creation_is_durable_and_reviewed_retry_preserves_its_transaction() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = Session::open(dir.path()).unwrap();
        let (account, _, folder) = fixture(&mut s, true);
        let start = DateTime::from_unix(s.now().unix + 3600, 0).to_rfc3339();
        let end = DateTime::from_unix(s.now().unix + 7200, 0).to_rfc3339();
        let before = s.mailbox.appointments.clone();
        s.run(
            "graph.createAppointment",
            &json!({"folder":folder.0,"fields":{"subject":"Planning","start":start,"end":end}}),
        )
        .unwrap();
        let op = pending(&s)[0].clone();
        assert_eq!(s.mailbox.appointments, before);
        assert!(write_allowed(&s, &op));
        assert!(op.payload["transactionId"].as_str().is_some_and(|v| !v.is_empty()));
        claim(&mut s, account, std::slice::from_ref(&op)).unwrap();
        drop(s);
        let mut s = Session::open(dir.path()).unwrap();
        s.view.work_offline = true;
        assert_eq!(pending(&s)[0].status, "uncertain");
        assert!(s.run("graph.retry", &json!({"write":op.id})).is_err());
        s.run("graph.retry", &json!({"write":op.id,"reviewed":true})).unwrap();
        assert_eq!(pending(&s)[0].payload, op.payload);
        assert_eq!(pending(&s)[0].remote_id, "calendar");
        s.mailbox.accounts.iter_mut().find(|a| a.id == account).unwrap().address =
            "changed@example.net".into();
        assert!(!write_allowed(&s, &pending(&s)[0]));
        assert!(s.run("graph.retry", &json!({"write":op.id})).is_err());
    }

    #[test]
    fn meeting_invitations_are_durable_and_keep_recipients_on_reviewed_retry() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = Session::open(dir.path()).unwrap();
        let (account, _, folder) = fixture(&mut s, true);
        let start = DateTime::from_unix(s.now().unix + 3600, 0).to_rfc3339();
        let end = DateTime::from_unix(s.now().unix + 7200, 0).to_rfc3339();
        let fields = json!({"subject":"Design review","start":start,"end":end,"attendees":[{"email":"maya@example.net","type":"required"},{"email":"alex@example.net","type":"optional"}]});
        assert!(s.run("graph.createMeeting", &json!({"folder":folder.0,"fields":fields})).is_err());
        let cached = s.mailbox.appointments.clone();
        s.run("graph.createMeeting", &json!({"folder":folder.0,"fields":fields,"confirm":true})).unwrap();
        assert_eq!(s.mailbox.appointments, cached);
        let op = pending(&s)[0].clone();
        assert_eq!(op.payload["attendees"][1]["type"], "optional");
        assert_eq!(op.payload["attendees"][0]["emailAddress"]["address"], "maya@example.net");
        assert!(write_allowed(&s, &op));
        claim(&mut s, account, std::slice::from_ref(&op)).unwrap();
        drop(s);
        let mut s = Session::open(dir.path()).unwrap();
        s.view.work_offline = true;
        assert_eq!(pending(&s)[0].status, "uncertain");
        assert!(s.run("graph.retry", &json!({"write":op.id})).is_err());
        s.run("graph.retry", &json!({"write":op.id,"reviewed":true})).unwrap();
        assert_eq!(pending(&s)[0].payload, op.payload);
    }
    #[test]
    fn meeting_creation_rejects_bad_duplicate_and_unsupported_attendees() {
        let mut s = crate::sample::sample_session();
        let (_, _, folder) = fixture(&mut s, true);
        let start = DateTime::from_unix(s.now().unix + 3600, 0).to_rfc3339();
        let end = DateTime::from_unix(s.now().unix + 7200, 0).to_rfc3339();
        for attendees in [
            json!([]),
            json!([{"email":"bad\r\nBcc: hidden@example.net"}]),
            json!([{"email":"maya@example.net","type":"resource"}]),
            json!([{"email":"maya@example.net"},{"email":"MAYA@example.net"}]),
            json!(vec![json!({"email":"a@example.net"}); 101]),
        ] {
            assert!(s.run("graph.createMeeting",&json!({"folder":folder.0,"confirm":true,"fields":{"subject":"Review","start":start,"end":end,"attendees":attendees}})).is_err());
        }
        assert!(pending(&s).is_empty());
    }
    #[test]
    fn creation_rejects_meetings_missing_offsets_and_invalid_times() {
        let mut s = crate::sample::sample_session();
        let (_, _, folder) = fixture(&mut s, true);
        let start = DateTime::from_unix(s.now().unix + 3600, 0).to_rfc3339();
        let end = DateTime::from_unix(s.now().unix + 7200, 0).to_rfc3339();
        for fields in [
            json!({"subject":"A","start":start,"end":end,"attendees":["a@example.net"]}),
            json!({"subject":"A","start":start,"end":start}),
            json!({"subject":"A","start":start.trim_end_matches('Z'),"end":end}),
            json!({"subject":"","start":start,"end":end}),
        ] {
            assert!(s.run("graph.createAppointment", &json!({"folder":folder.0,"fields":fields})).is_err());
        }
        assert!(pending(&s).is_empty());
    }
    #[test]
    fn deletion_requires_confirmation_and_refuses_meetings_or_series() {
        let mut s = crate::sample::sample_session();
        let (_, id, folder) = fixture(&mut s, true);
        assert!(!can_delete(&s, id));
        s.mailbox
            .folders
            .get_mut(&folder)
            .unwrap()
            .graph_source
            .as_mut()
            .unwrap()
            .metadata
            .get_mut("remote-item")
            .unwrap()
            .event_type = "singleInstance".into();
        assert!(can_delete(&s, id));
        s.run("graph.deleteAppointment", &json!({"id":id.0})).unwrap();
        assert!(pending(&s).is_empty());
        s.run("graph.deleteAppointment", &json!({"id":id.0,"confirm":true})).unwrap();
        assert_eq!(pending(&s)[0].action, "delete");
        assert!(s.mailbox.appointments.contains_key(&id));
        s.mailbox.appointments.get_mut(&id).unwrap().is_meeting = true;
        assert!(!write_allowed(&s, &pending(&s)[0]));
    }
    #[test]
    fn proposals_are_partial_durable_and_do_not_change_the_cache_or_allow_generic_writes() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = Session::open(dir.path()).unwrap();
        let (account, id, _) = fixture(&mut s, false);
        let before = s.mailbox.contacts[&id].clone();
        let result = s
            .run("graph.edit", &json!({"id":id.0,"fields":{"firstName":"Maya","company":"Studio North"}}))
            .unwrap();
        assert!(result["queued"].as_bool().unwrap());
        assert_eq!(s.mailbox.contacts[&id], before);
        let op = pending(&s).pop().unwrap();
        assert_eq!(op.payload, json!({"companyName":"Studio North"}));
        assert_eq!(op.mailbox, "owner@example.net");
        assert!(s.run("people.update", &json!({"id":id.0,"company":"Changed"})).is_err());
        assert!(s.run("graph.edit", &json!({"id":id.0,"fields":{"company":"Another"}})).is_err());
        claim(&mut s, account, std::slice::from_ref(&op)).unwrap();
        drop(s);
        let reopened = Session::open(dir.path()).unwrap();
        let op = pending(&reopened).pop().unwrap();
        assert_eq!(op.status, "uncertain");
        assert!(op.error.contains("stopped"));
    }
    #[test]
    fn uncertainty_never_retries_itself_and_manual_retry_uses_the_reviewed_version() {
        let mut s = crate::sample::sample_session();
        let (account, id, folder) = fixture(&mut s, true);
        s.run("graph.respond", &json!({"id":id.0,"response":"accept","comment":"See you there"})).unwrap();
        let op = pending(&s).pop().unwrap();
        assert_eq!(op.payload, json!({"comment":"See you there","sendResponse":true}));
        claim(&mut s, account, std::slice::from_ref(&op)).unwrap();
        finish(&mut s, account, op.id, Err("Outcome uncertain. Check Outlook.".into()));
        assert_eq!(pending(&s)[0].status, "uncertain");
        assert!(pending(&s).iter().all(|op| op.status != "queued"));
        assert!(s.run("graph.retry", &json!({"write":op.id})).is_err());
        s.mailbox
            .folders
            .get_mut(&folder)
            .unwrap()
            .graph_source
            .as_mut()
            .unwrap()
            .metadata
            .get_mut("remote-item")
            .unwrap()
            .etag = "W/\"two\"".into();
        s.run("graph.retry", &json!({"write":op.id,"reviewed":true})).unwrap();
        assert_eq!(pending(&s)[0].etag, "W/\"two\"");
        let writes = pending(&s);
        claim(&mut s, account, &writes).unwrap();
        settle(&mut s, account);
        assert_eq!(pending(&s)[0].status, "uncertain");
        finish(&mut s, account, op.id, Ok(()));
        assert!(pending(&s).is_empty());
        assert_eq!(s.mailbox.appointments[&id].subject, "Review");
    }
    #[test]
    fn permissions_versions_dates_and_unsupported_fields_are_checked_before_queuing() {
        let mut s = crate::sample::sample_session();
        let (account, id, folder) = fixture(&mut s, true);
        for fields in [
            json!({"body":"Do not overwrite a meeting link"}),
            json!({"start":"2026-10-14T11:00:00Z"}),
            json!({"start":"2026-10-14T09:00"}),
            json!({"subject":42}),
        ] {
            assert!(s.run("graph.edit", &json!({"id":id.0,"fields":fields})).is_err());
        }
        s.run("accounts.options", &json!({"account":account.0,"graphEditing":false})).unwrap();
        assert!(s.run("graph.edit", &json!({"id":id.0,"fields":{"subject":"New"}})).is_err());
        s.run("accounts.options", &json!({"account":account.0,"graphEditing":true})).unwrap();
        assert!(
            s.run(
                "accounts.oauthSignIn",
                &json!({"account":account.0,"provider":"microsoft-graph-pim","clientId":"fixture"})
            )
            .unwrap_err()
            .to_string()
            .contains("editing provider")
        );
        crate::accounts::prefs(&mut s).accounts.get_mut(&account.0).unwrap().oauth =
            Some(crate::accounts::OAuthBinding {
                scopes: vec!["https://graph.microsoft.com/Calendars.Read.Shared".into()],
                ..Default::default()
            });
        assert_eq!(capabilities(&s, id), (false, false));
        crate::accounts::prefs(&mut s).accounts.get_mut(&account.0).unwrap().oauth = None;
        s.mailbox
            .folders
            .get_mut(&folder)
            .unwrap()
            .graph_source
            .as_mut()
            .unwrap()
            .metadata
            .get_mut("remote-item")
            .unwrap()
            .etag
            .clear();
        assert!(s.run("graph.edit", &json!({"id":id.0,"fields":{"subject":"New"}})).is_err());
        assert!(pending(&s).is_empty());
    }
    #[test]
    fn failed_changes_survive_permission_errors_and_cannot_cross_mailbox_connections() {
        let mut s = crate::sample::sample_session();
        let (account, id, _) = fixture(&mut s, false);
        s.run("graph.edit", &json!({"id":id.0,"fields":{"company":"Studio"}})).unwrap();
        let op = pending(&s).pop().unwrap();
        claim(&mut s, account, std::slice::from_ref(&op)).unwrap();
        assert!(s.run("graph.discard", &json!({"write":op.id})).is_err());
        finish(&mut s, account, op.id, Err("HTTP 403: missing permission".into()));
        assert_eq!(pending(&s)[0].status, "failed");
        s.mailbox.accounts.iter_mut().find(|a| a.id == account).unwrap().address = "other@example.net".into();
        assert!(s.run("graph.retry", &json!({"write":op.id,"reviewed":true})).is_err());
        s.run("graph.discard", &json!({"write":op.id})).unwrap();
        assert!(pending(&s).is_empty());
    }
    #[test]
    fn background_writes_confirm_cache_refresh_and_keep_conflicted_proposals_without_replaying() {
        use std::io::{BufRead, BufReader, Read, Write};
        use std::net::TcpListener;
        use std::time::{Duration, Instant};
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let base = format!("http://{}/v1.0", listener.local_addr().unwrap());
        let thread = std::thread::spawn(move || {
            let mut patches = 0;
            let mut requests = 0;
            let deadline = Instant::now() + Duration::from_secs(15);
            while requests < 4 {
                let (mut stream, _) = match listener.accept() {
                    Ok(s) => s,
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        assert!(Instant::now() < deadline, "worker did not send expected request");
                        std::thread::sleep(Duration::from_millis(5));
                        continue;
                    }
                    Err(e) => panic!("{e}"),
                };
                stream.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                let first = line.clone();
                let mut length = 0;
                let mut etag = String::new();
                let mut token = false;
                loop {
                    line.clear();
                    reader.read_line(&mut line).unwrap();
                    if line == "\r\n" {
                        break;
                    }
                    if let Some((k, v)) = line.split_once(':') {
                        match k.to_ascii_lowercase().as_str() {
                            "content-length" => length = v.trim().parse().unwrap(),
                            "if-match" => etag = v.trim().into(),
                            "authorization" => token = v.trim() == "Bearer fixture-token",
                            _ => {}
                        }
                    }
                }
                assert!(token);
                let mut body = vec![0; length];
                reader.read_exact(&mut body).unwrap();
                let (status, body) = if first.starts_with("PATCH ") {
                    patches += 1;
                    let p: Value = serde_json::from_slice(&body).unwrap();
                    if patches == 1 {
                        assert_eq!(etag, "W/\"one\"");
                        assert_eq!(p, json!({"companyName":"Studio"}));
                        (200, "{}".into())
                    } else {
                        assert_eq!(etag, "W/\"two\"");
                        assert_eq!(p, json!({"mobilePhone":"+1 555 0100"}));
                        (412, json!({"error":{"code":"ErrorIrresolvableConflict"}}).to_string())
                    }
                } else {
                    assert!(first.contains("/contacts?$top=100"), "{first}");
                    (200,json!({"value":[{"id":"remote-item","@odata.etag":if patches==1 {"W/\"two\""}else{"W/\"three\""},"givenName":"Maya","companyName":if patches==1 {"Studio"}else{"Server changed"},"emailAddresses":[{"address":"maya@example.net"}]}]}).to_string())
                };
                write!(stream,"HTTP/1.1 {status} Fixture\r\nContent-Length: {}\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{body}",body.len()).unwrap();
                requests += 1;
            }
        });
        let mut s = crate::sample::sample_session();
        let (account, id, _) = fixture(&mut s, false);
        let a = s.mailbox.accounts.iter_mut().find(|a| a.id == account).unwrap();
        a.incoming.as_mut().unwrap().host = base;
        a.incoming.as_mut().unwrap().allow_plaintext_auth = true;
        s.run("accounts.options", &json!({"account":account.0,"graphCalendars":false})).unwrap();
        s.secrets.set(&crate::accounts::secret_key(account, false), "fixture-token").unwrap();
        s.run("graph.edit", &json!({"id":id.0,"fields":{"company":"Studio"}})).unwrap();
        assert_eq!(s.mailbox.contacts[&id].company, "");
        s.view.work_offline = false;
        s.send_receive(&[account], Plan { send: false, receive: false, user: true, folders: None }).unwrap();
        assert!(s.wait_for_sync(Duration::from_secs(10)));
        assert!(pending(&s).is_empty());
        assert_eq!(s.mailbox.contacts[&id].company, "Studio");
        s.run("graph.edit", &json!({"id":id.0,"fields":{"mobile":"+1 555 0100"}})).unwrap();
        assert!(s.wait_for_sync(Duration::from_secs(10)));
        assert_eq!(pending(&s)[0].status, "failed");
        assert_eq!(s.mailbox.contacts[&id].company, "Server changed");
        assert_eq!(
            s.send_receive(&[account], Plan { send: false, receive: false, user: true, folders: None })
                .unwrap(),
            0
        );
        thread.join().unwrap();
    }
    #[test]
    fn persistence_failure_prevents_queuing_or_claiming_a_server_write() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = Session::open(dir.path()).unwrap();
        let (account, id, _) = fixture(&mut s, false);
        let path = dir.path().join(crate::accounts::ACCOUNT_OPTIONS_FILE);
        std::fs::remove_file(&path).unwrap();
        std::fs::create_dir(&path).unwrap();
        assert!(s.run("graph.edit", &json!({"id":id.0,"fields":{"company":"Unsaved"}})).is_err());
        assert!(pending(&s).is_empty());
        std::fs::remove_dir(&path).unwrap();
        crate::accounts::save_prefs_checked(&mut s).unwrap();
        s.run("graph.edit", &json!({"id":id.0,"fields":{"company":"Saved proposal"}})).unwrap();
        let op = pending(&s).pop().unwrap();
        std::fs::remove_file(&path).unwrap();
        std::fs::create_dir(&path).unwrap();
        assert!(claim(&mut s, account, std::slice::from_ref(&op)).is_err());
        assert_eq!(pending(&s)[0].status, "queued");
    }
    #[test]
    fn a_sheet_keeps_its_original_version_when_a_background_refresh_changes_the_cache() {
        let mut s = crate::sample::sample_session();
        let (_, id, folder) = fixture(&mut s, false);
        s.run("graph.edit", &json!({"id":id.0})).unwrap();
        let version = s
            .notices
            .iter()
            .find_map(|n| match n {
                Notice::OpenDialog(name, p) if name == "graphItem" => {
                    p.get("version").and_then(Value::as_str)
                }
                _ => None,
            })
            .unwrap()
            .to_owned();
        s.mailbox
            .folders
            .get_mut(&folder)
            .unwrap()
            .graph_source
            .as_mut()
            .unwrap()
            .metadata
            .get_mut("remote-item")
            .unwrap()
            .etag = "W/\"two\"".into();
        s.run(
            "graph.edit",
            &json!({"id":id.0,"version":version,"fields":{"company":"Proposal from older sheet"}}),
        )
        .unwrap();
        assert_eq!(pending(&s)[0].etag, "W/\"one\"");
        let queued = pending(&s)[0].clone();
        s.run("graph.retry", &json!({"write":queued.id})).unwrap();
        assert_eq!(pending(&s)[0].etag, "W/\"one\"");
        let account = pending(&s)[0].account;
        invalidate(&mut s, account);
        assert_eq!(pending(&s)[0].status, "failed");
    }
}
