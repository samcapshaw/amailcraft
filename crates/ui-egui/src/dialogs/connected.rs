//! Focused Microsoft 365 edit/response sheets. Changes go through the engine's durable write queue.
mod lifecycle;
use super::common::{self, Dlg, Out, footer, muted, primary, window, with_state};
use crate::App;
use egui::{Context, RichText, TextEdit};
use mailcraft_engine::cmd::connected;
use mailcraft_model::{DateTime, ItemId};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

pub(crate) fn show(ctx: &Context, app: &mut App, d: &Dlg<'_>) -> Option<Out> {
    Some(match d.name {
        "graphCreateAppointment" | "graphCreateMeeting" => lifecycle::create(ctx, app, d),
        "graphDeleteAppointment" => lifecycle::delete(ctx, app, d),
        "graphItem" => edit(ctx, app, d),
        "graphPending" => review(ctx, app, d),
        _ => return None,
    })
}
#[derive(Clone, Default)]
struct Edit {
    fields: BTreeMap<String, String>,
    original: BTreeMap<String, String>,
    response: usize,
    comment: String,
    error: String,
}
fn label(key: &str) -> &str {
    match key {
        "subject" => "Subject",
        "location" => "Location",
        "start" => "Starts (UTC)",
        "end" => "Ends (UTC)",
        "firstName" => "First name",
        "lastName" => "Last name",
        "company" => "Company",
        "jobTitle" => "Job title",
        "email" => "Email",
        "mobile" => "Mobile",
        "notes" => "Notes",
        _ => key,
    }
}
fn edit(ctx: &Context, app: &mut App, d: &Dlg<'_>) -> Out {
    let id = ItemId(d.u64("id").unwrap_or(0));
    let init = |app: &App| {
        let mut fields = BTreeMap::new();
        if let Some(a) = app.s.mailbox.appointments.get(&id) {
            fields.extend([
                ("subject".into(), a.subject.clone()),
                ("location".into(), a.location.clone()),
                ("start".into(), DateTime::from_unix(a.start.unix, 0).to_rfc3339()),
                ("end".into(), DateTime::from_unix(a.end.unix, 0).to_rfc3339()),
            ]);
        }
        if let Some(c) = app.s.mailbox.contacts.get(&id) {
            fields.extend([
                ("firstName".into(), c.first_name.clone()),
                ("lastName".into(), c.last_name.clone()),
                ("company".into(), c.company.clone()),
                ("jobTitle".into(), c.job_title.clone()),
                ("email".into(), c.emails.first().cloned().unwrap_or_default()),
                (
                    "mobile".into(),
                    c.phones.iter().find(|(k, _)| k == "Mobile").map(|(_, p)| p.clone()).unwrap_or_default(),
                ),
                ("notes".into(), c.notes.clone()),
            ]);
        }
        Edit { original: fields.clone(), fields, ..Default::default() }
    };
    with_state(ctx, app, d.uid, init, |st, app| {
        window(ctx, app, d, "Microsoft 365", 560.0, |ui, app| {
            let (can_edit, can_respond) = connected::capabilities(&app.s, id);
            let event = app.s.mailbox.appointments.get(&id).cloned();
            let account = event
                .as_ref()
                .map(|a| a.folder)
                .or_else(|| app.s.mailbox.contacts.get(&id).map(|c| c.folder))
                .and_then(|f| app.s.mailbox.folder(f))
                .and_then(|f| app.s.mailbox.account(f.account))
                .map(|a| a.address.clone())
                .unwrap_or_default();
            common::heading(
                ui,
                app,
                if can_respond {
                    "Respond to meeting"
                } else if event.is_some() {
                    "Edit event"
                } else {
                    "Edit contact"
                },
            );
            muted(ui, app, &account);
            ui.add_space(8.0);
            if can_respond {
                if let Some(a) = event.as_ref() {
                    ui.label(RichText::new(&a.subject).strong());
                }
                ui.horizontal(|ui| {
                    ui.radio_value(&mut st.response, 0, "Accept");
                    ui.radio_value(&mut st.response, 1, "Tentative");
                    ui.radio_value(&mut st.response, 2, "Decline");
                });
                ui.label("Message (optional)");
                ui.add(TextEdit::multiline(&mut st.comment).desired_rows(3).desired_width(f32::INFINITY));
                muted(
                    ui,
                    app,
                    "Your response will be sent to the organizer. Declining may remove the event from your calendar.",
                );
            } else {
                let keys = if event.is_some() {
                    vec!["subject", "location", "start", "end"]
                } else {
                    vec!["firstName", "lastName", "company", "jobTitle", "email", "mobile", "notes"]
                };
                egui::Grid::new(("graph-fields", d.uid)).num_columns(2).spacing([14.0, 10.0]).show(
                    ui,
                    |ui| {
                        for key in keys {
                            let field = st.fields.entry(key.into()).or_default();
                            let enabled = can_edit
                                && !(matches!(key, "start" | "end")
                                    && event.as_ref().is_some_and(|a| a.all_day))
                                && !(key == "email"
                                    && app.s.mailbox.contacts.get(&id).is_some_and(|c| c.emails.len() > 1));
                            ui.label(label(key));
                            ui.add_enabled_ui(enabled, |ui| {
                                if matches!(key, "start" | "end") {
                                    time_field(ui, field, (d.uid, key));
                                } else if key == "notes" {
                                    ui.add(TextEdit::multiline(field).desired_rows(4).desired_width(360.0));
                                } else {
                                    ui.add(TextEdit::singleline(field).desired_width(360.0));
                                }
                            });
                            ui.end_row();
                        }
                    },
                );
                muted(
                    ui,
                    app,
                    if event.is_some() {
                        "Times are shown in UTC. Recurring events change only this occurrence. Meeting edits may notify attendees."
                    } else {
                        "Other contact details stay unchanged. Contacts with multiple email addresses keep their existing addresses."
                    },
                );
            }
            if !can_edit && !can_respond {
                muted(
                    ui,
                    app,
                    "Editing is unavailable. Check the editing grant in account settings, then refresh this folder.",
                );
            }
            if !st.error.is_empty() {
                ui.colored_label(app.theme.danger, &st.error);
            }
            footer(ui, |ui| {
                let mut out = Out::Keep;
                if ui.button("Cancel").clicked() {
                    out = Out::Close;
                }
                if primary(
                    ui,
                    app,
                    can_edit || can_respond,
                    if can_respond { "Send response" } else { "Save to Microsoft 365" },
                ) {
                    let result = if can_respond {
                        app.s.run("graph.respond",&json!({"id":id.0,"version":d.str("version"),"response":(["accept","tentativelyAccept","decline"].get(st.response).copied().unwrap_or("accept")),"comment":st.comment}))
                    } else {
                        let fields: serde_json::Map<String, Value> = st
                            .fields
                            .iter()
                            .filter(|(k, v)| st.original.get(*k) != Some(*v))
                            .map(|(k, v)| (k.clone(), json!(v)))
                            .collect();
                        app.s
                            .run("graph.edit", &json!({"id":id.0,"version":d.str("version"),"fields":fields}))
                    };
                    match result {
                        Ok(_) => {
                            app.s.notify(mailcraft_engine::Notice::Info("Change queued. The cached item updates after Microsoft confirms it; Pending changes keeps any failures.".into()));
                            out = Out::Close;
                        }
                        Err(e) => st.error = e.to_string(),
                    }
                }
                out
            })
        })
    })
}
fn time_field(ui: &mut egui::Ui, value: &mut String, id: (u64, &str)) {
    let (date, time) = value.split_once('T').unwrap_or((value.as_str(), "09:00:00Z"));
    let mut date = date.to_owned();
    let hour = time.get(..2).and_then(|v| v.parse::<u32>().ok()).filter(|h| *h < 24).unwrap_or(9);
    let min = time.get(3..5).and_then(|v| v.parse::<u32>().ok()).filter(|m| *m < 60).unwrap_or(0);
    let sec = time.get(6..8).and_then(|v| v.parse::<u32>().ok()).filter(|m| *m < 60).unwrap_or(0);
    let mut minute = hour * 60 + min;
    let original = minute;
    let clock = |minute: u32| {
        format!(
            "{}:{:02} {}",
            if (minute / 60).is_multiple_of(12) { 12 } else { minute / 60 % 12 },
            minute % 60,
            if minute < 720 { "AM" } else { "PM" }
        )
    };
    let mut changed = false;
    ui.horizontal(|ui| {
        changed =
            ui.add(TextEdit::singleline(&mut date).hint_text("YYYY-MM-DD").desired_width(125.0)).changed();
        egui::ComboBox::from_id_salt(id).selected_text(clock(minute)).width(110.0).show_ui(ui, |ui| {
            if !minute.is_multiple_of(15) {
                let current = minute;
                ui.selectable_value(&mut minute, current, clock(current));
            }
            for quarter in 0..96 {
                let candidate = quarter * 15;
                ui.selectable_value(&mut minute, candidate, clock(candidate));
            }
        });
    });
    if changed || minute != original {
        *value = format!(
            "{date}T{:02}:{:02}:{:02}Z",
            minute / 60,
            minute % 60,
            if minute == original { sec } else { 0 }
        );
    }
}

fn proposal_text(value: &Value) -> String {
    if let Some(s) = value.as_str() {
        return s.into();
    }
    if let Some(s) = value.get("displayName").and_then(Value::as_str) {
        return s.into();
    }
    if let Some(s) = value.get("dateTime").and_then(Value::as_str) {
        return format!("{s} UTC");
    }
    if let Some(content) = value.get("content").and_then(Value::as_str) {
        return content.into();
    }
    if let Some(flag) = value.as_bool() {
        return if flag { "Yes" } else { "No" }.into();
    }
    if let Some(addresses) = value.as_array() {
        return addresses
            .iter()
            .filter_map(|v| {
                if let Some(email) = v.pointer("/emailAddress/address").and_then(Value::as_str) {
                    Some(format!(
                        "{} ({})",
                        email,
                        v.get("type").and_then(Value::as_str).unwrap_or("required")
                    ))
                } else {
                    v.get("address").and_then(Value::as_str).map(str::to_owned)
                }
            })
            .collect::<Vec<_>>()
            .join(", ");
    }
    String::new()
}

#[derive(Clone, Default)]
struct Review {
    checked: BTreeSet<u64>,
    error: String,
}
fn review(ctx: &Context, app: &mut App, d: &Dlg<'_>) -> Out {
    with_state(
        ctx,
        app,
        d.uid,
        |_| Review::default(),
        |st, app| {
            window(ctx, app, d, "Pending Microsoft 365 changes", 640.0, |ui, app| {
                let rows = connected::pending(&app.s);
                muted(
                    ui,
                    app,
                    "Cached items change after server confirmation. Discard removes this local proposal; it does not undo a change that Microsoft already accepted.",
                );
                if rows.is_empty() {
                    ui.add_space(12.0);
                    ui.label("Everything is up to date.");
                }
                egui::ScrollArea::vertical().max_height(440.0).show(ui, |ui| {
                    for op in rows {
                        ui.push_id(op.id, |ui| {
                            ui.add_space(10.0);
                            let title = app
                                .s
                                .mailbox
                                .appointments
                                .get(&op.item)
                                .map(|a| a.subject.clone())
                                .or_else(|| app.s.mailbox.contacts.get(&op.item).map(|c| c.full_name()))
                                .unwrap_or_else(|| {
                                    op.payload
                                        .get("subject")
                                        .and_then(Value::as_str)
                                        .unwrap_or("Unavailable item")
                                        .into()
                                });
                            ui.label(RichText::new(title).strong());
                            muted(
                                ui,
                                app,
                                &format!(
                                    "{} · {} · {}",
                                    op.mailbox,
                                    match op.action.as_str() {
                                        "patch" => "Edit",
                                        "create"
                                            if op
                                                .payload
                                                .get("attendees")
                                                .and_then(Value::as_array)
                                                .is_some_and(|v| !v.is_empty()) =>
                                            "Meeting invitations",
                                        "create" => "New appointment",
                                        "delete" => "Delete appointment",
                                        _ => &op.action,
                                    },
                                    match op.status.as_str() {
                                        "inFlight" => "Sending",
                                        "queued" => "Queued",
                                        "uncertain" => "Outcome uncertain",
                                        _ => "Needs attention",
                                    }
                                ),
                            );
                            if !op.error.is_empty() {
                                ui.colored_label(app.theme.danger, &op.error);
                            }
                            ui.collapsing("Proposed changes", |ui| {
                                if let Some(fields) = op.payload.as_object() {
                                    for (key, value) in fields {
                                        if key == "sendResponse" || key == "transactionId" {
                                            continue;
                                        }
                                        let value = proposal_text(value);
                                        ui.label(format!(
                                            "{}: {value}",
                                            match key.as_str() {
                                                "givenName" => "First name",
                                                "surname" => "Last name",
                                                "companyName" => "Company",
                                                "personalNotes" => "Notes",
                                                "mobilePhone" => "Mobile",
                                                "emailAddresses" => "Email",
                                                "attendees" => "Attendees",
                                                "responseRequested" => "Request responses",
                                                "comment" => "Message",
                                                _ => label(key),
                                            }
                                        ));
                                    }
                                }
                            });
                            let mut checked = st.checked.contains(&op.id);
                            if op.status == "queued" {
                                muted(
                                    ui,
                                    app,
                                    if app.s.view.work_offline {
                                        "Waiting until you are online. This proposal has not been submitted."
                                    } else {
                                        "Waiting for the next account sync."
                                    },
                                );
                                ui.horizontal(|ui| {
                                    if ui
                                        .add_enabled(!app.s.view.work_offline, egui::Button::new("Send now"))
                                        .clicked()
                                        && let Err(e) = app.s.run("graph.retry", &json!({"write":op.id}))
                                    {
                                        st.error = e.to_string();
                                    }
                                    if ui.button("Discard proposal").clicked()
                                        && let Err(e) = app.s.run("graph.discard", &json!({"write":op.id}))
                                    {
                                        st.error = e.to_string();
                                    }
                                });
                            } else if op.status != "inFlight" {
                                if ui
                                    .checkbox(
                                        &mut checked,
                                        "I reviewed the current item and previous outcome on Microsoft 365",
                                    )
                                    .changed()
                                {
                                    if checked {
                                        st.checked.insert(op.id);
                                    } else {
                                        st.checked.remove(&op.id);
                                    }
                                }
                                ui.horizontal(|ui| {
                                    if ui.add_enabled(checked, egui::Button::new("Retry")).clicked()
                                        && let Err(e) =
                                            app.s.run("graph.retry", &json!({"write":op.id,"reviewed":true}))
                                    {
                                        st.error = e.to_string();
                                    }
                                    if ui.button("Discard proposal").clicked()
                                        && let Err(e) = app.s.run("graph.discard", &json!({"write":op.id}))
                                    {
                                        st.error = e.to_string();
                                    }
                                });
                            }
                            ui.separator();
                        });
                    }
                });
                if !st.error.is_empty() {
                    ui.colored_label(app.theme.danger, &st.error);
                }
                footer(ui, |ui| if ui.button("Close").clicked() { Out::Close } else { Out::Keep })
            })
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn meeting_review_exposes_recipients_roles_and_notes() {
        let value = json!([{"emailAddress":{"address":"maya@example.net"},"type":"required"},{"emailAddress":{"address":"alex@example.net"},"type":"optional"}]);
        assert_eq!(proposal_text(&value), "maya@example.net (required), alex@example.net (optional)");
        assert_eq!(
            proposal_text(&json!({"contentType":"text","content":"Review layouts"})),
            "Review layouts"
        );
    }
}
