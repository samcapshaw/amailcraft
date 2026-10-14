//! Complete, bounded read-only Graph calendar/contact snapshots. A failed fetch never prunes cached
//! data. Identity maps live with folders, so items and their remote identities persist atomically.
#![cfg_attr(target_arch = "wasm32", allow(dead_code))]
use crate::Session;
use mailcraft_model::calendar::{Attendee, AttendeeRole, BusyStatus, ResponseStatus};
use mailcraft_model::contact::PostalAddress;
use mailcraft_model::folder::{GraphFolderSource, GraphItemMeta};
use mailcraft_model::{
    AccountId, AccountKind, Appointment, Contact, DateTime, Folder, FolderId, FolderKind, ItemId, Mailbox,
};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug)]
pub(super) struct SnapshotFolder {
    pub remote_id: String,
    pub name: String,
    pub appointments: BTreeMap<String, Appointment>,
    pub contacts: BTreeMap<String, Contact>,
    pub metadata: BTreeMap<String, GraphItemMeta>,
}
#[derive(Clone, Debug)]
pub(super) struct Snapshot {
    pub kind: FolderKind,
    pub folders: Vec<SnapshotFolder>,
    pub at: DateTime,
    pub window: Option<(DateTime, DateTime)>,
}

#[cfg(not(target_arch = "wasm32"))]
pub(super) fn fetch(
    client: &mailcraft_net::graph::GraphClient,
    kind: FolderKind,
    now: DateTime,
    cancel: &std::sync::atomic::AtomicBool,
) -> Result<Snapshot, mailcraft_net::NetError> {
    use mailcraft_net::NetError;
    let check = || {
        if cancel.load(std::sync::atomic::Ordering::SeqCst) {
            Err(NetError::Refused("Calendar/contact sync cancelled".into()))
        } else {
            Ok(())
        }
    };
    check()?;
    let mut folders = Vec::new();
    let mut window = None;
    if kind == FolderKind::Calendar {
        let day = now.unix.div_euclid(86_400).saturating_mul(86_400);
        let start = DateTime::from_unix(day.saturating_sub(90 * 86_400), 0);
        let end = DateTime::from_unix(day.saturating_add(366 * 86_400), 0);
        window = Some((start, end));
        let page = client.calendars()?;
        let mut bytes = page.bytes_received;
        let calendars = page.values;
        if calendars.len() > 256 {
            return Err(NetError::TooLarge("Graph calendars".into()));
        }
        let mut seen = BTreeSet::new();
        let mut total = 0usize;
        for calendar in calendars {
            check()?;
            let id = required(&calendar, "id").map_err(NetError::Protocol)?;
            if !seen.insert(id.to_owned()) {
                return Err(NetError::Protocol("Graph repeated a calendar".into()));
            }
            let mut f = SnapshotFolder {
                remote_id: id.into(),
                name: required(&calendar, "name").map_err(NetError::Protocol)?.into(),
                appointments: BTreeMap::new(),
                contacts: BTreeMap::new(),
                metadata: BTreeMap::new(),
            };
            let page = client.calendar_view(id, &start.to_rfc3339(), &end.to_rfc3339())?;
            bytes = bytes.saturating_add(page.bytes_received);
            if bytes > 64 * 1024 * 1024 {
                return Err(NetError::TooLarge("Graph calendar snapshot bytes".into()));
            }
            let values = page.values;
            total = total.saturating_add(values.len());
            if total > 100_000 {
                return Err(NetError::TooLarge("Graph calendar snapshot".into()));
            }
            for value in values {
                check()?;
                if value.get("isCancelled").and_then(Value::as_bool) == Some(true) {
                    continue;
                }
                let id = required(&value, "id").map_err(NetError::Protocol)?.to_owned();
                f.metadata.insert(
                    id.clone(),
                    metadata(&value, calendar.get("canEdit").and_then(Value::as_bool) == Some(true), true),
                );
                f.appointments.insert(id, appointment(&value).map_err(NetError::Protocol)?);
            }
            folders.push(f);
        }
    } else if kind == FolderKind::Contacts {
        let mut f = SnapshotFolder {
            remote_id: "default-contacts".into(),
            name: "Contacts".into(),
            appointments: BTreeMap::new(),
            contacts: BTreeMap::new(),
            metadata: BTreeMap::new(),
        };
        for value in client.contacts(None)?.values {
            check()?;
            let id = required(&value, "id").map_err(NetError::Protocol)?.to_owned();
            f.metadata.insert(id.clone(), metadata(&value, true, false));
            f.contacts.insert(id, contact(&value));
        }
        folders.push(f);
    } else {
        return Err(NetError::Invalid("Unsupported Graph connected folder kind".into()));
    }
    check()?;
    Ok(Snapshot { kind, folders, at: now, window })
}

fn metadata(v: &Value, can_edit: bool, event: bool) -> GraphItemMeta {
    let organizer = v.get("isOrganizer").and_then(Value::as_bool).unwrap_or(false);
    let meeting = v.get("attendees").and_then(Value::as_array).is_some_and(|a| !a.is_empty());
    GraphItemMeta {
        etag: text(v, "@odata.etag"),
        can_edit: can_edit && (!event || organizer || !meeting),
        can_respond: event && can_edit && !organizer && meeting,
        event_type: text(v, "type"),
    }
}

fn text(v: &Value, key: &str) -> String {
    v.get(key).and_then(Value::as_str).unwrap_or_default().into()
}
fn required<'a>(v: &'a Value, key: &str) -> Result<&'a str, String> {
    v.get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| format!("Graph object has no {key}"))
}
fn strings(v: &Value, key: &str) -> Vec<String> {
    v.get(key)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect()
}
/// Graph returns dateTimeTimeZone with UTC (requested in Prefer). Reject a non-UTC response rather
/// than silently interpreting a Windows zone as the machine's zone. Fractional seconds are ignored.
fn utc(v: &Value) -> Result<DateTime, String> {
    if !matches!(v.get("timeZone").and_then(Value::as_str), Some("UTC" | "Etc/UTC")) {
        return Err("Graph event did not return the requested UTC time zone".into());
    }
    let s = required(v, "dateTime")?;
    let n = |a, b| s.get(a..b).and_then(|t| t.parse::<u32>().ok());
    let suffix = s.get(19..).unwrap_or_default().trim_end_matches('Z');
    if s.get(4..5) != Some("-")
        || s.get(7..8) != Some("-")
        || s.get(10..11) != Some("T")
        || s.get(13..14) != Some(":")
        || s.get(16..17) != Some(":")
        || !(suffix.is_empty()
            || suffix
                .strip_prefix('.')
                .is_some_and(|s| !s.is_empty() && s.bytes().all(|c| c.is_ascii_digit())))
    {
        return Err("Invalid Graph event date/time".into());
    }
    DateTime::from_civil(
        n(0, 4).and_then(|n| i32::try_from(n).ok()).ok_or("Invalid Graph year")?,
        n(5, 7).and_then(|n| u8::try_from(n).ok()).ok_or("Invalid Graph month")?,
        n(8, 10).and_then(|n| u8::try_from(n).ok()).ok_or("Invalid Graph day")?,
        n(11, 13).and_then(|n| u8::try_from(n).ok()).ok_or("Invalid Graph hour")?,
        n(14, 16).and_then(|n| u8::try_from(n).ok()).ok_or("Invalid Graph minute")?,
        n(17, 19).and_then(|n| u8::try_from(n).ok()).ok_or("Invalid Graph second")?,
        0,
    )
    .ok_or_else(|| "Invalid Graph event date/time".into())
}
fn address(v: &Value) -> Option<mailcraft_mime::Mailbox> {
    let email = v.get("emailAddress")?;
    let address = required(email, "address").ok()?;
    Some(mailcraft_mime::Mailbox {
        name: email.get("name").and_then(Value::as_str).filter(|s| !s.is_empty()).map(str::to_owned),
        addr: address.into(),
    })
}
fn appointment(v: &Value) -> Result<Appointment, String> {
    let start = utc(v.get("start").ok_or("Graph event has no start")?)?;
    let end = utc(v.get("end").ok_or("Graph event has no end")?)?;
    if end.unix < start.unix {
        return Err("Graph event ends before its start".into());
    }
    let body = v.get("body").unwrap_or(&Value::Null);
    let content = text(body, "content");
    let attendees: Vec<_> = v
        .get("attendees")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|a| {
            Some(Attendee {
                mailbox: address(a)?,
                role: match a.get("type").and_then(Value::as_str) {
                    Some("optional") => AttendeeRole::Optional,
                    Some("resource") => AttendeeRole::Resource,
                    _ => AttendeeRole::Required,
                },
                response: match a.pointer("/status/response").and_then(Value::as_str) {
                    Some("accepted") => ResponseStatus::Accepted,
                    Some("tentativelyAccepted") => ResponseStatus::Tentative,
                    Some("declined") => ResponseStatus::Declined,
                    _ => ResponseStatus::None,
                },
            })
        })
        .collect();
    Ok(Appointment {
        uid: text(v, "iCalUId"),
        subject: text(v, "subject"),
        location: v.pointer("/location/displayName").and_then(Value::as_str).unwrap_or_default().into(),
        start,
        end,
        all_day: v.get("isAllDay").and_then(Value::as_bool).unwrap_or(false),
        show_as: match v.get("showAs").and_then(Value::as_str) {
            Some("free") => BusyStatus::Free,
            Some("tentative") => BusyStatus::Tentative,
            Some("oof") => BusyStatus::OutOfOffice,
            Some("workingElsewhere") => BusyStatus::WorkingElsewhere,
            _ => BusyStatus::Busy,
        },
        // Read-only remote snapshots must not generate duplicate local reminder notifications.
        reminder_min: None,
        organizer: v.get("organizer").and_then(address),
        is_meeting: !attendees.is_empty(),
        attendees,
        categories: strings(v, "categories"),
        private: matches!(v.get("sensitivity").and_then(Value::as_str), Some("private" | "confidential")),
        body: if body
            .get("contentType")
            .and_then(Value::as_str)
            .is_some_and(|s| s.eq_ignore_ascii_case("html"))
        {
            mailcraft_html::html_to_text(&content)
        } else {
            content
        },
        // calendarView already expands series and exceptions. No local recurrence is set.
        ..Default::default()
    })
}
fn contact(v: &Value) -> Contact {
    // Notes from a remote contact are never an internal distribution-group marker.
    let notes = text(v, "personalNotes");
    let notes =
        if notes.lines().next().is_some_and(|line| line.trim() == crate::cmd::people::CONTACT_GROUP_MARKER) {
            format!("\n{notes}")
        } else {
            notes
        };

    let mut phones = Vec::new();
    for (key, label) in [("businessPhones", "Business"), ("homePhones", "Home")] {
        for phone in strings(v, key) {
            phones.push((label.into(), phone));
        }
    }
    let mobile = text(v, "mobilePhone");
    if !mobile.is_empty() {
        phones.push(("Mobile".into(), mobile));
    }
    let mut addresses = Vec::new();
    for (key, label) in [("businessAddress", "Business"), ("homeAddress", "Home"), ("otherAddress", "Other")]
    {
        if let Some(a) = v.get(key).filter(|a| a.is_object()) {
            let address = PostalAddress {
                kind: label.into(),
                street: text(a, "street"),
                city: text(a, "city"),
                region: text(a, "state"),
                postal_code: text(a, "postalCode"),
                country: text(a, "countryOrRegion"),
            };
            if [&address.street, &address.city, &address.region, &address.postal_code, &address.country]
                .iter()
                .any(|s| !s.is_empty())
            {
                addresses.push(address);
            }
        }
    }
    Contact {
        first_name: text(v, "givenName"),
        middle_name: text(v, "middleName"),
        last_name: text(v, "surname"),
        title: text(v, "title"),
        suffix: text(v, "generation"),
        file_as: v
            .get("fileAs")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .unwrap_or_else(|| text(v, "displayName")),
        company: text(v, "companyName"),
        job_title: text(v, "jobTitle"),
        department: text(v, "department"),
        emails: v
            .get("emailAddresses")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|a| a.get("address").and_then(Value::as_str))
            .map(str::to_owned)
            .collect(),
        phones,
        addresses,
        web_page: text(v, "businessHomePage"),
        im_address: strings(v, "imAddresses").into_iter().next().unwrap_or_default(),
        birthday: v
            .get("birthday")
            .and_then(Value::as_str)
            .and_then(|s| s.get(..10))
            .filter(|s| crate::cmd::people::valid_birthday(s))
            .map(str::to_owned),
        notes,
        categories: strings(v, "categories"),
        office: text(v, "officeLocation"),
        profession: text(v, "profession"),
        manager: text(v, "manager"),
        assistant: text(v, "assistantName"),
        nickname: text(v, "nickName"),
        spouse: text(v, "spouseName"),
        ..Default::default()
    }
}

pub(super) fn apply(s: &mut Session, account: AccountId, snapshot: Snapshot) {
    if !s.mailbox.account(account).is_some_and(|a| a.kind == AccountKind::Graph) {
        return;
    }
    let options = crate::accounts::account_options(s, account);
    if !(match snapshot.kind {
        FolderKind::Calendar => options.graph_calendars,
        FolderKind::Contacts => options.graph_contacts,
        _ => false,
    }) {
        return;
    }
    let active: BTreeSet<_> = snapshot.folders.iter().map(|f| f.remote_id.clone()).collect();
    let gone: Vec<_> = s
        .mailbox
        .folders
        .values()
        .filter(|f| {
            f.account == account
                && f.kind == snapshot.kind
                && f.graph_source.as_ref().is_some_and(|source| !active.contains(&source.remote_id))
        })
        .map(|f| f.id)
        .collect();
    for id in gone {
        remove_folder(&mut s.mailbox, id);
    }
    for remote in snapshot.folders {
        let folder = s
            .mailbox
            .folders
            .values()
            .find(|f| {
                f.account == account
                    && f.kind == snapshot.kind
                    && f.graph_source.as_ref().is_some_and(|source| source.remote_id == remote.remote_id)
            })
            .map(|f| f.id)
            .unwrap_or_else(|| {
                let id = FolderId(s.mailbox.alloc_id());
                s.mailbox.folders.insert(
                    id,
                    Folder {
                        id,
                        account,
                        kind: snapshot.kind,
                        name: remote.name.clone(),
                        graph_source: Some(GraphFolderSource {
                            remote_id: remote.remote_id.clone(),
                            ..Default::default()
                        }),
                        ..Default::default()
                    },
                );
                id
            });
        let old = s
            .mailbox
            .folder(folder)
            .and_then(|f| f.graph_source.as_ref())
            .map(|s| s.items.clone())
            .unwrap_or_default();
        let mut items = BTreeMap::new();
        s.mailbox.appointments.retain(|_, a| a.folder != folder);
        s.mailbox.contacts.retain(|_, c| c.folder != folder);
        for (remote_id, mut a) in remote.appointments {
            let id = old.get(&remote_id).copied().unwrap_or_else(|| ItemId(s.mailbox.alloc_id()));
            a.id = id;
            a.folder = folder;
            items.insert(remote_id, id);
            s.mailbox.appointments.insert(id, a);
        }
        for (remote_id, mut c) in remote.contacts {
            let id = old.get(&remote_id).copied().unwrap_or_else(|| ItemId(s.mailbox.alloc_id()));
            c.id = id;
            c.folder = folder;
            items.insert(remote_id, id);
            s.mailbox.contacts.insert(id, c);
        }
        if let Some(f) = s.mailbox.folders.get_mut(&folder) {
            f.name = remote.name;
            f.graph_source = Some(GraphFolderSource {
                remote_id: remote.remote_id,
                metadata: remote.metadata,
                items,
                last_sync: Some(snapshot.at),
                window_start: snapshot.window.map(|w| w.0),
                window_end: snapshot.window.map(|w| w.1),
            });
        }
    }
}
fn remove_folder(mb: &mut Mailbox, folder: FolderId) {
    mb.appointments.retain(|_, a| a.folder != folder);
    mb.contacts.retain(|_, c| c.folder != folder);
    mb.folders.remove(&folder);
    mb.views.remove(&folder);
}

/// Reject every route that could modify connected contents, including import, cross-folder move,
/// generic item commands and changes to folder identity. View/colour/favourite changes remain local.
pub(crate) fn changed(before: &Mailbox, after: &Mailbox) -> bool {
    for f in before.folders.values().filter(|f| f.graph_source.is_some()) {
        if after.account(f.account).is_none() {
            continue;
        } // Removing an account is intentional.
        let Some(next) = after.folder(f.id) else {
            return true;
        };
        if f.account != next.account
            || f.kind != next.kind
            || f.name != next.name
            || f.parent != next.parent
            || f.graph_source != next.graph_source
        {
            return true;
        }
        if before
            .appointments
            .iter()
            .filter(|(_, a)| a.folder == f.id)
            .ne(after.appointments.iter().filter(|(_, a)| a.folder == f.id))
            || before
                .contacts
                .iter()
                .filter(|(_, c)| c.folder == f.id)
                .ne(after.contacts.iter().filter(|(_, c)| c.folder == f.id))
        {
            return true;
        }
    }
    false
}
/// Server refreshes are not undo steps; preserve the current connected data across undo/redo.
pub(crate) fn carry_over(live: &Mailbox, restored: &mut Mailbox) {
    let connected: BTreeSet<_> = live
        .folders
        .values()
        .chain(restored.folders.values())
        .filter(|f| f.graph_source.is_some())
        .map(|f| f.id)
        .collect();
    if connected.is_empty() {
        return;
    }
    for id in connected {
        remove_folder(restored, id);
        if let Some(f) = live.folder(id).filter(|f| restored.account(f.account).is_some()) {
            restored.folders.insert(id, f.clone());
            if let Some(view) = live.views.get(&id) {
                restored.views.insert(id, view.clone());
            }
            restored.appointments.extend(
                live.appointments.iter().filter(|(_, a)| a.folder == id).map(|(id, a)| (*id, a.clone())),
            );
            restored
                .contacts
                .extend(live.contacts.iter().filter(|(_, c)| c.folder == id).map(|(id, c)| (*id, c.clone())));
        }
    }
    restored.next_id = restored.next_id.max(live.next_id);
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn account(s: &mut Session) -> AccountId {
        AccountId(s.run("accounts.add", &json!({"kind":"graph","address":"owner@example.net","graphCalendars":true,"graphContacts":true,"incoming":{"host":"https://graph.microsoft.com/v1.0"}})).unwrap()["account"].as_u64().unwrap() as u32)
    }
    fn snapshot(kind: FolderKind, name: &str, present: bool) -> Snapshot {
        let appointments = if present && kind == FolderKind::Calendar {
            BTreeMap::from([("a".into(), Appointment { subject: name.into(), ..Default::default() })])
        } else {
            BTreeMap::new()
        };
        let contacts = if present && kind == FolderKind::Contacts {
            BTreeMap::from([("c".into(), Contact { first_name: name.into(), ..Default::default() })])
        } else {
            BTreeMap::new()
        };
        Snapshot {
            kind,
            folders: vec![SnapshotFolder {
                remote_id: "remote".into(),
                name: name.into(),
                appointments,
                contacts,
                metadata: BTreeMap::new(),
            }],
            at: DateTime::from_unix(1, 0),
            window: None,
        }
    }
    #[test]
    fn refresh_preserves_ids_prunes_only_connected_data_and_survives_undo_and_reload() {
        let mut s = crate::sample::sample_session();
        let account = account(&mut s);
        let local_contacts = s.mailbox.contacts.clone();
        assert!(!s.mailbox.folders.values().any(|f| f.graph_source.is_some()));
        apply(&mut s, account, snapshot(FolderKind::Contacts, "Maya", true));
        let folder = s.mailbox.folders.values().find(|f| f.graph_source.is_some()).unwrap().id;
        let id = s.mailbox.folder(folder).unwrap().graph_source.as_ref().unwrap().items["c"];
        let old = s.mailbox.clone();
        apply(&mut s, account, snapshot(FolderKind::Contacts, "Maya Chen", true));
        assert_eq!(s.mailbox.folder(folder).unwrap().graph_source.as_ref().unwrap().items["c"], id);
        assert_eq!(s.mailbox.contacts[&id].first_name, "Maya Chen");
        let mut restored = old;
        carry_over(&s.mailbox, &mut restored);
        assert_eq!(restored.contacts[&id].first_name, "Maya Chen");
        let encoded = serde_json::to_vec(&s.mailbox).unwrap();
        let decoded: Mailbox = serde_json::from_slice(&encoded).unwrap();
        assert_eq!(
            decoded.folder(folder).unwrap().graph_source,
            s.mailbox.folder(folder).unwrap().graph_source
        );
        apply(&mut s, account, snapshot(FolderKind::Contacts, "Contacts", false));
        assert!(!s.mailbox.contacts.contains_key(&id));
        for (id, c) in local_contacts {
            assert_eq!(s.mailbox.contacts[&id], c);
        }
        // Disabled or removed accounts ignore late worker results; retained copies stay offline.
        s.run("accounts.edit", &json!({"account":account.0,"graphContacts":false})).unwrap();
        apply(&mut s, account, snapshot(FolderKind::Contacts, "Late", true));
        assert!(!s.mailbox.contacts.contains_key(&id));
    }
    #[test]
    fn connected_writes_are_refused_through_commands_including_new_move_import_and_delete() {
        let mut s = crate::sample::sample_session();
        let account = account(&mut s);
        apply(&mut s, account, snapshot(FolderKind::Contacts, "Maya", true));
        apply(&mut s, account, snapshot(FolderKind::Calendar, "Review", true));
        let contacts = s
            .mailbox
            .folders
            .values()
            .find(|f| f.kind == FolderKind::Contacts && f.graph_source.is_some())
            .unwrap()
            .id;
        let calendar = s
            .mailbox
            .folders
            .values()
            .find(|f| f.kind == FolderKind::Calendar && f.graph_source.is_some())
            .unwrap()
            .id;
        let contact = s.mailbox.folder(contacts).unwrap().graph_source.as_ref().unwrap().items["c"];
        let appointment = s.mailbox.folder(calendar).unwrap().graph_source.as_ref().unwrap().items["a"];
        let before = s.mailbox.clone();
        for (cmd, params) in [
            ("people.update", json!({"id":contact.0,"firstName":"Changed"})),
            ("people.delete", json!({"ids":[contact.0]})),
            ("people.new", json!({"folder":contacts.0,"fullName":"New"})),
            ("calendar.update", json!({"id":appointment.0,"subject":"Changed"})),
            ("calendar.newAppointment", json!({"folder":calendar.0,"subject":"New"})),
            ("calendar.delete", json!({"ids":[appointment.0]})),
            ("folder.rename", json!({"folder":contacts.0,"name":"Changed"})),
        ] {
            assert!(s.registry.get(cmd).is_some(), "unknown {cmd}");
            assert!(s.run(cmd, &params).is_err(), "{cmd}");
            assert_eq!(s.mailbox.contacts, before.contacts, "{cmd}");
            assert_eq!(s.mailbox.appointments, before.appointments, "{cmd}");
        }
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("synthetic.vcf");
        std::fs::write(&path, "BEGIN:VCARD\r\nVERSION:3.0\r\nFN:Imported\r\nN:Imported;;;;\r\nEND:VCARD\r\n")
            .unwrap();
        assert!(
            s.run(
                "file.openExport",
                &json!({"action":"importVcf","path":path.to_string_lossy(),"folder":contacts.0})
            )
            .is_err()
        );
        assert_eq!(s.mailbox.contacts, before.contacts);
        assert!(
            s.run(
                "accounts.oauthSignIn",
                &json!({"account":account.0,"provider":"microsoft-graph","clientId":"fixture-client"})
            )
            .unwrap_err()
            .to_string()
            .contains("calendars and contacts")
        );
        let local = *s.mailbox.contacts.keys().find(|id| **id != contact).unwrap();
        assert!(s.run("people.move", &json!({"id":local.0,"folder":contacts.0})).is_err());
        assert_eq!(s.mailbox.contacts, before.contacts);
        assert!(s.run("people.new", &json!({"folder":contacts.0})).is_err());
        assert!(s.run("calendar.newAppointment", &json!({"folder":calendar.0})).is_err());
        let local_folder = before.contacts.values().find(|c| c.id != contact).unwrap().folder;
        let copied = s.run("people.copy", &json!({"id":contact.0,"folder":local_folder.0})).unwrap();
        let copied = copied
            .get("copies")
            .and_then(Value::as_array)
            .and_then(|ids| ids.first())
            .and_then(Value::as_u64)
            .unwrap();
        s.run("people.update", &json!({"id":copied,"firstName":"Local copy"})).unwrap();
        assert_eq!(s.mailbox.contacts[&contact], before.contacts[&contact]);
        // Calendar colour is a local presentation choice, not a server edit.
        let mut props = crate::cmd::calendar::props::read(&s.mailbox, calendar);
        props.color = Some("Blue".into());
        crate::cmd::calendar::props::write(&mut s.mailbox, calendar, &props);
        assert!(!changed(&before, &s.mailbox));
    }
    #[test]
    fn contact_fields_preserve_phone_address_birthday_and_plain_notes() {
        let c = contact(
            &json!({"givenName":"Maya","surname":"Chen","displayName":"Maya Chen","birthday":"1991-05-12T00:00:00Z","businessPhones":["+1 555 0100"],"mobilePhone":"+1 555 0101","emailAddresses":[{"address":"maya@example.net"}],"businessAddress":{"street":"123 Example St","state":"CA","countryOrRegion":"US"},"personalNotes":"Contact Group\nOrdinary notes","categories":["Clients"]}),
        );
        assert_eq!(c.birthday.as_deref(), Some("1991-05-12"));
        assert_eq!(c.phones.len(), 2);
        assert_eq!(c.addresses[0].region, "CA");
        assert_eq!(c.emails, ["maya@example.net"]);
        assert!(!crate::cmd::people::is_group(&c));
        assert!(c.notes.contains("Ordinary notes"));
    }

    #[test]
    fn expanded_events_keep_utc_instants_and_do_not_expand_or_fire_reminders_again() {
        let v = json!({"id":"occurrence","subject":"Review","type":"occurrence","isReminderOn":true,"reminderMinutesBeforeStart":15,"recurrence":{"pattern":{"type":"daily"}},"start":{"dateTime":"2026-10-14T09:00:00.0000000","timeZone":"UTC"},"end":{"dateTime":"2026-10-14T10:00:00","timeZone":"UTC"},"body":{"contentType":"HTML","content":"<p>Hello <b>team</b></p>"},"attendees":[{"type":"optional","emailAddress":{"name":"Maya","address":"maya@example.net"},"status":{"response":"accepted"}}]});
        let a = appointment(&v).unwrap();
        assert_eq!(a.start.to_rfc3339(), "2026-10-14T09:00:00Z");
        assert_eq!(a.attendees[0].role, AttendeeRole::Optional);
        assert!(a.body.contains("Hello team"));
        assert!(a.recurrence.is_none());
        assert!(a.reminder_min.is_none());
        let mut bad = v.clone();
        bad["start"]["timeZone"] = json!("Pacific Standard Time");
        assert!(appointment(&bad).is_err());
        for date in ["🦀", "2026-99-99T09:00:00", "2026-10-14T09:00:00garbage"] {
            bad["start"] = json!({"timeZone":"UTC","dateTime":date});
            assert!(appointment(&bad).is_err());
        }
        let a=appointment(&json!({"start":{"dateTime":"2026-10-14T00:00:00","timeZone":"UTC"},"end":{"dateTime":"2026-10-15T00:00:00","timeZone":"UTC"},"isAllDay":true})).unwrap();
        assert!(a.all_day);
    }
}
