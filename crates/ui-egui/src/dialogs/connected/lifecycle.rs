use super::*;
use mailcraft_model::FolderId;

#[derive(Clone, Default)]
struct AppointmentForm {
    subject: String,
    location: String,
    start: String,
    end: String,
    notes: String,
    error: String,
}
pub(super) fn create(ctx: &Context, app: &mut App, d: &Dlg<'_>) -> Out {
    let folder = FolderId(d.u64("folder").unwrap_or(0));
    with_state(
        ctx,
        app,
        d.uid,
        |app| {
            let now = app.s.now().unix;
            let start = now.div_euclid(900).saturating_add(1).saturating_mul(900);
            AppointmentForm {
                start: DateTime::from_unix(start, 0).to_rfc3339(),
                end: DateTime::from_unix(start.saturating_add(3600), 0).to_rfc3339(),
                ..Default::default()
            }
        },
        |st, app| {
            window(ctx, app, d, "New Microsoft 365 appointment", 560.0, |ui, app| {
                let name = app
                    .s
                    .mailbox
                    .folder(folder)
                    .map(|f| f.name.clone())
                    .unwrap_or_else(|| "Unavailable calendar".into());
                ui.label(RichText::new(name).strong().size(18.0));
                if let Some(account) =
                    app.s.mailbox.folder(folder).and_then(|f| app.s.mailbox.account(f.account))
                {
                    muted(ui, app, &account.address);
                }
                ui.add_space(14.0);
                let field_width = (ui.available_width() - 110.0).clamp(180.0, 360.0);
                egui::Grid::new(("connected-create", d.uid)).num_columns(2).spacing([16.0, 12.0]).show(
                    ui,
                    |ui| {
                        let label = ui.label("Subject");
                        ui.add(TextEdit::singleline(&mut st.subject).desired_width(field_width))
                            .labelled_by(label.id);
                        ui.end_row();
                        let label = ui.label("Location");
                        ui.add(TextEdit::singleline(&mut st.location).desired_width(field_width))
                            .labelled_by(label.id);
                        ui.end_row();
                        ui.label("Starts (UTC)");
                        time_field(ui, &mut st.start, (d.uid, "create-start"));
                        ui.end_row();
                        ui.label("Ends (UTC)");
                        time_field(ui, &mut st.end, (d.uid, "create-end"));
                        ui.end_row();
                        ui.label("Notes");
                        ui.add(TextEdit::multiline(&mut st.notes).desired_rows(4).desired_width(field_width));
                        ui.end_row();
                    },
                );
                ui.add_space(12.0);
                muted(
                    ui,
                    app,
                    "A single timed appointment. Times are UTC; attendees, all-day events and recurrence will be added separately.",
                );
                muted(
                    ui,
                    app,
                    if app.s.view.work_offline {
                        "You're offline. The proposal will wait in Pending changes until you reconnect."
                    } else {
                        "Appears in your calendar after Microsoft accepts it and the calendar refreshes."
                    },
                );
                if !st.error.is_empty() {
                    ui.colored_label(app.theme.danger, &st.error);
                }
                footer(ui, |ui| {
                    if ui.button("Cancel").clicked() {
                        return Out::Close;
                    }
                    if primary(
                        ui,
                        app,
                        connected::can_create(&app.s, folder) && !st.subject.trim().is_empty(),
                        if app.s.view.work_offline { "Queue appointment" } else { "Create appointment" },
                    ) {
                        match app.s.run("graph.createAppointment",&json!({"folder":folder.0,"fields":{"subject":st.subject,"location":st.location,"start":st.start,"end":st.end,"notes":st.notes}})) {
                        Ok(_)=>return Out::Close,
                        Err(e)=>st.error=e.to_string(),
                    }
                    }
                    Out::Keep
                })
            })
        },
    )
}
pub(super) fn delete(ctx: &Context, app: &mut App, d: &Dlg<'_>) -> Out {
    let id = ItemId(d.u64("id").unwrap_or(0));
    with_state(
        ctx,
        app,
        d.uid,
        |_| String::new(),
        |error, app| {
            window(ctx, app, d, "Delete Microsoft 365 appointment?", 480.0, |ui, app| {
                let title = app
                    .s
                    .mailbox
                    .appointments
                    .get(&id)
                    .map(|a| a.subject.clone())
                    .unwrap_or_else(|| "Unavailable appointment".into());
                ui.label(RichText::new(title).size(18.0).strong());
                ui.add_space(12.0);
                muted(
                    ui,
                    app,
                    "This removes the appointment from Microsoft 365 after confirmation. It cannot be undone here. Meetings and recurring events cannot be deleted from this sheet.",
                );
                if app.s.view.work_offline {
                    muted(ui, app, "You're offline. Deletion will wait in Pending changes.");
                }
                if !error.is_empty() {
                    ui.colored_label(app.theme.danger, error.as_str());
                }
                footer(ui, |ui| {
                    if ui.button("Keep appointment").clicked() {
                        return Out::Close;
                    }
                    if primary(ui, app, connected::can_delete(&app.s, id), "Delete appointment") {
                        match app.s.run(
                            "graph.deleteAppointment",
                            &json!({"id":id.0,"confirm":true,"version":d.str("version")}),
                        ) {
                            Ok(_) => return Out::Close,
                            Err(e) => *error = e.to_string(),
                        }
                    }
                    Out::Keep
                })
            })
        },
    )
}
