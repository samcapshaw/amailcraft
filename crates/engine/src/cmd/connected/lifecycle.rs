//! Appointment lifecycle proposals. No local placeholder is confused with a confirmed server event.
use super::*;
use mailcraft_model::FolderId;

pub fn can_create(s: &Session, folder: FolderId) -> bool {
    let Some(f) = s.mailbox.folder(folder) else { return false };
    let o = crate::accounts::account_options(s, f.account);
    f.kind == FolderKind::Calendar
        && f.graph_source.as_ref().is_some_and(|source| !source.remote_id.is_empty())
        && s.mailbox.account(f.account).is_some_and(|a| a.kind == AccountKind::Graph)
        && o.graph_calendars
        && o.graph_editing
        && o.oauth.as_ref().is_none_or(|b| {
            b.scopes.iter().any(|v| v == "https://graph.microsoft.com/Calendars.ReadWrite.Shared")
        })
}
pub fn can_delete(s: &Session, id: ItemId) -> bool {
    capabilities(s, id).0
        && item(s, id).is_ok_and(|(_, _, _, meta)| meta.event_type == "singleInstance")
        && s.mailbox.appointments.get(&id).is_some_and(|a| !a.is_meeting && a.attendees.is_empty())
}
pub fn write_allowed(s: &Session, op: &PendingWrite) -> bool {
    if !s.mailbox.account(op.account).is_some_and(|a| {
        a.address == op.mailbox && a.incoming.as_ref().is_some_and(|i| i.host == op.endpoint)
    }) {
        return false;
    }
    if op.action != "create"
        && !item(s, op.item).is_ok_and(|(account, remote, kind, _)| {
            account == op.account
                && remote == op.remote_id
                && op.kind == if kind == FolderKind::Calendar { "event" } else { "contact" }
        })
    {
        return false;
    }
    match op.action.as_str() {
        "create" => op.folder.is_some_and(|id| {
            can_create(s, id)
                && s.mailbox.folder(id).is_some_and(|f| {
                    f.account == op.account
                        && f.graph_source.as_ref().is_some_and(|source| source.remote_id == op.remote_id)
                })
        }),
        "delete" => can_delete(s, op.item),
        "patch" => capabilities(s, op.item).0,
        "accept" | "tentativelyAccept" | "decline" => capabilities(s, op.item).1,
        _ => false,
    }
}
pub(super) fn create(s: &mut Session, p: &Value) -> CmdResult {
    create_event(s, p, false)
}
pub(super) fn create_meeting(s: &mut Session, p: &Value) -> CmdResult {
    create_event(s, p, true)
}
fn create_event(s: &mut Session, p: &Value, meeting: bool) -> CmdResult {
    crate::accounts::load_prefs(s);
    let folder = FolderId(target(p, "folder")?);
    if !can_create(s, folder) {
        return Err(CmdError::Disabled(
            "Choose a connected calendar with editing enabled and sign in with the editing grant.".into(),
        ));
    }
    if p.get("fields").is_none() {
        s.notify(Notice::OpenDialog(
            if meeting { "graphCreateMeeting" } else { "graphCreateAppointment" }.into(),
            json!({"folder":folder.0}),
        ));
        return Ok(json!({"dialog":if meeting {"graphCreateMeeting"} else {"graphCreateAppointment"}}));
    }
    let fields = p
        .get("fields")
        .filter(|v| v.is_object())
        .ok_or_else(|| CmdError::Params("`fields` must be an object".into()))?;
    if fields.as_object().is_some_and(|m| {
        m.keys().any(|k| {
            !["subject", "location", "start", "end", "notes"].contains(&k.as_str())
                && !(meeting && k == "attendees")
        })
    }) {
        return Err(CmdError::Params(
            if meeting {
                "Only a single timed meeting with required or optional attendees is supported here."
            } else {
                "Only a timed appointment without attendees or recurrence is supported here."
            }
            .into(),
        ));
    }
    let attendees = if meeting {
        if p.get("confirm").and_then(Value::as_bool) != Some(true) {
            return Err(CmdError::Params("Confirm sending invitations before creating this meeting.".into()));
        }
        Some(meeting_attendees(fields)?)
    } else {
        None
    };
    let subject = text(fields, "subject", 2048)?
        .filter(|v| !v.trim().is_empty())
        .ok_or_else(|| CmdError::Params("Enter a subject.".into()))?;
    let parse = |key| -> Result<DateTime, CmdError> {
        let raw = text(fields, key, 80)?.ok_or_else(|| CmdError::Params(format!("`{key}` is required")))?;
        if raw.len() < 20
            || !raw.contains('T')
            || !(raw.ends_with('Z') || raw.get(19..).is_some_and(|v| v.contains('+') || v.contains('-')))
        {
            return Err(CmdError::Params("Include seconds and UTC Z or a numeric offset.".into()));
        }
        DateTime::parse_rfc5322(&raw)
            .map_err(|_| CmdError::Params("Use an ISO time with UTC Z or a numeric offset.".into()))
    };
    let (start_time, end_time) = (parse("start")?, parse("end")?);
    if end_time.unix <= start_time.unix
        || start_time.unix < s.now().unix.saturating_sub(90 * 86400)
        || end_time.unix > s.now().unix.saturating_add(366 * 86400)
    {
        return Err(CmdError::Params(
            "End must follow start, within the connected calendar's past 90 days and coming year.".into(),
        ));
    }
    if s.sync.prefs.data.graph_writes.len() >= 128 {
        return Err(CmdError::Disabled("Review pending changes first.".into()));
    }
    let f = s.mailbox.folder(folder).ok_or_else(|| CmdError::Params("No calendar".into()))?;
    let account = f.account;
    let remote_id = f.graph_source.as_ref().map(|v| v.remote_id.clone()).unwrap_or_default();
    let source = s.mailbox.account(account).ok_or_else(|| CmdError::Params("No account".into()))?;
    let (mailbox, endpoint) =
        (source.address.clone(), source.incoming.as_ref().map(|v| v.host.clone()).unwrap_or_default());
    let transaction = transaction_id()?;
    let time = |t: DateTime| json!({"dateTime":DateTime::from_unix(t.unix,0).to_rfc3339().trim_end_matches('Z'),"timeZone":"UTC"});
    let mut payload = json!({"subject":subject,"location":{"displayName":text(fields,"location",2048)?.unwrap_or_default()},"body":{"contentType":"text","content":text(fields,"notes",16384)?.unwrap_or_default()},"start":time(start_time),"end":time(end_time),"transactionId":transaction});
    if let Some(attendees) = attendees {
        let object = payload
            .as_object_mut()
            .ok_or_else(|| CmdError::Failed("Could not prepare meeting fields.".into()))?;
        object.insert("attendees".into(), Value::Array(attendees));
        object.insert("responseRequested".into(), Value::Bool(true));
    }
    if serde_json::to_vec(&payload).map_err(|e| CmdError::Failed(e.to_string()))?.len() > 32 * 1024 {
        return Err(CmdError::Params("Appointment is too large.".into()));
    }
    let before = s.sync.prefs.data.clone();
    let write = before
        .next_graph_write
        .checked_add(1)
        .ok_or_else(|| CmdError::Failed("Pending identifier limit".into()))?;
    s.sync.prefs.data.next_graph_write = write;
    s.sync.prefs.data.graph_writes.push(PendingWrite {
        id: write,
        account,
        mailbox,
        endpoint,
        item: ItemId(0),
        remote_id,
        folder: Some(folder),
        kind: "event".into(),
        action: "create".into(),
        etag: String::new(),
        payload,
        status: "queued".into(),
        error: String::new(),
    });
    persist(s, before)?;
    start(s, account);
    Ok(json!({"queued":true,"write":write,"offline":s.view.work_offline}))
}
fn meeting_attendees(fields: &Value) -> Result<Vec<Value>, CmdError> {
    let values = fields
        .get("attendees")
        .and_then(Value::as_array)
        .filter(|v| !v.is_empty() && v.len() <= 100)
        .ok_or_else(|| CmdError::Params("Add between 1 and 100 required or optional attendees.".into()))?;
    let mut seen = std::collections::BTreeSet::new();
    values
        .iter()
        .map(|v| {
            let email = v.get("email").and_then(Value::as_str).unwrap_or_default().trim();
            let role = v.get("type").and_then(Value::as_str).unwrap_or("required");
            let valid = email.len() <= 254
                && email.is_ascii()
                && email.split_once('@').is_some_and(|(local, domain)| {
                    !local.is_empty()
                        && local.len() <= 64
                        && !local.starts_with('.')
                        && !local.ends_with('.')
                        && !local.contains("..")
                        && local
                            .bytes()
                            .all(|c| c.is_ascii_alphanumeric() || b".!#$%&'*+-/=?^_`{|}~".contains(&c))
                        && domain.contains('.')
                        && domain.split('.').all(|l| {
                            !l.is_empty()
                                && l.len() <= 63
                                && !l.starts_with('-')
                                && !l.ends_with('-')
                                && l.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-')
                        })
                });
            if !valid
                || !["required", "optional"].contains(&role)
                || v.as_object().is_none_or(|o| o.keys().any(|k| k != "email" && k != "type"))
            {
                return Err(CmdError::Params(
                    "Use plain email addresses and required or optional attendee types.".into(),
                ));
            }
            if !seen.insert(email.to_ascii_lowercase()) {
                return Err(CmdError::Params("Each attendee should appear only once.".into()));
            }
            Ok(json!({"emailAddress":{"address":email},"type":role}))
        })
        .collect()
}

pub(super) fn delete(s: &mut Session, p: &Value) -> CmdResult {
    let id = ItemId(target(p, "id")?);
    if !can_delete(s, id) {
        return Err(CmdError::Disabled("Delete here supports only a non-recurring appointment without attendees. Meeting cancellations and series deletion require Outlook.".into()));
    }
    if p.get("confirm").and_then(Value::as_bool) != Some(true) {
        let (_, _, _, meta) = item(s, id)?;
        s.notify(Notice::OpenDialog("graphDeleteAppointment".into(), json!({"id":id.0,"version":meta.etag})));
        return Ok(json!({"dialog":"graphDeleteAppointment"}));
    }
    enqueue(s, p, "delete")
}
pub(super) fn retry_create(s: &mut Session, op: &PendingWrite) -> CmdResult {
    if !write_allowed(s, op) {
        return Err(CmdError::Disabled(
            "The source calendar or permissions changed. Discard this proposal and start again.".into(),
        ));
    }
    if op.status != "queued" {
        let before = s.sync.prefs.data.clone();
        if let Some(write) = s.sync.prefs.data.graph_writes.iter_mut().find(|v| v.id == op.id) {
            write.status = "queued".into();
            write.error.clear();
        }
        persist(s, before)?;
    }
    // Preserve exactly the same transactionId and fields, including after uncertain outcomes.
    start(s, op.account);
    Ok(json!({"queued":true,"write":op.id}))
}

#[cfg(not(target_arch = "wasm32"))]
fn transaction_id() -> Result<String, CmdError> {
    mailcraft_net::oauth::random_state().map_err(|e| CmdError::Failed(e.to_string()))
}
#[cfg(target_arch = "wasm32")]
fn transaction_id() -> Result<String, CmdError> {
    Err(CmdError::Disabled("Connected creation requires the desktop app.".into()))
}
