//! Read-only discovery sheet; every request runs through the command registry.
use super::common::{Dlg, Out, footer, muted, primary, window, with_state};
use crate::App;
use egui::{Context, RichText, TextEdit};
use serde_json::{Value, json};

#[derive(Clone, Default)]
struct State {
    endpoint: String,
    username: String,
    password: String,
    job: Option<u64>,
    refreshing: bool,
    selected: Option<String>,
    message: String,
    result: Option<Value>,
    error: String,
    managing: Option<u64>,
    replacement_password: String,
    disconnecting: Option<u64>,
}
pub(super) fn show(ctx: &Context, app: &mut App, d: &Dlg<'_>) -> Out {
    with_state(
        ctx,
        app,
        d.uid,
        |_| State::default(),
        |st, app| {
            if let Some(job) = st.job {
                match app.s.run(
                    if st.refreshing { "caldav.refresh" } else { "caldav.discover" },
                    &json!({"job":job}),
                ) {
                    Ok(v) if v.get("status").and_then(Value::as_str) == Some("done") => {
                        st.error = v.get("error").and_then(Value::as_str).unwrap_or_default().to_owned();
                        if st.refreshing {
                            st.error = v
                                .pointer("/result/error")
                                .and_then(Value::as_str)
                                .unwrap_or_default()
                                .to_owned();
                            st.message = if st.error.is_empty() {
                                "Calendar updated. Events are available offline.".into()
                            } else {
                                String::new()
                            };
                        } else {
                            st.result = Some(v);
                        }
                        st.job = None;
                    }
                    Err(e) => {
                        st.error = e.to_string();
                        st.job = None;
                    }
                    _ => {
                        ctx.request_repaint_after(std::time::Duration::from_millis(200));
                    }
                }
            }
            window(ctx, app, d, "CalDAV calendars", 620.0, |ui, app| {
                ui.label(RichText::new("Your CalDAV calendars").size(24.0));
                if let Ok(saved) = app.s.run("caldav.sources", &json!({})) {
                    if let Some(sources) =
                        saved.get("sources").and_then(Value::as_array).filter(|v| !v.is_empty())
                    {
                        egui::ScrollArea::vertical().id_salt("saved-caldav").max_height(if st.managing.is_some() {420.0} else {220.0}).show(
                            ui,
                            |ui| {
                                for source in sources {
                                    let folder=source.get("folder").and_then(Value::as_u64).unwrap_or_default();
                                    let syncing=source.get("syncing").and_then(Value::as_bool)==Some(true);
                                    ui.push_id(folder,|ui| {
                                        ui.horizontal(|ui| {
                                            ui.label(RichText::new(source.get("name").and_then(Value::as_str).unwrap_or("Calendar")).strong());
                                            if ui.add_enabled(st.job.is_none() && !syncing,egui::Button::new("Refresh")).clicked() { start_refresh(app,st,folder); }
                                            if ui.button("Manage").clicked() {
                                                st.managing=if st.managing==Some(folder) {None} else {Some(folder)};
                                                st.replacement_password.clear(); st.disconnecting=None;
                                            }
                                        });
                                        let last=source.get("lastUpdate").and_then(Value::as_i64);
                                        let status=if syncing { "Updating calendar…".into() } else if let Some(error)=source.get("lastError").and_then(Value::as_str) {
                                            format!("Needs attention · {error}")
                                        } else if let Some(time)=last { format!("{} · Last synced {}",if app.s.view.work_offline {"Offline"} else {"Up to date"},stamp(app,time)) } else { "Read-only · Not downloaded yet".into() };
                                        muted(ui,app,&status);
                                        if source.get("lastError").is_some_and(|v|!v.is_null()) && let Some(time)=last {
                                            muted(ui,app,&format!("Offline events kept · Last synced {}",stamp(app,time)));
                                        }
                                        if let Some(attempt)=source.get("lastAttempt").and_then(Value::as_i64) { muted(ui,app,&format!("Last checked {}",stamp(app,attempt))); }
                                        if st.managing==Some(folder) {
                                            ui.add_space(8.0);
                                            muted(ui,app,source.get("username").and_then(Value::as_str).unwrap_or_default());
                                            muted(ui,app,source.get("url").and_then(Value::as_str).unwrap_or_default());
                                            let mut automatic=source.get("automatic").and_then(Value::as_bool).unwrap_or(false);
                                            if ui.checkbox(&mut automatic,"Refresh automatically").changed()
                                                && let Err(e)=app.s.run("caldav.configure",&json!({"folder":folder,"automatic":automatic})) { st.error=e.to_string(); }
                                            let mut minutes=source.get("refreshMin").and_then(Value::as_u64).unwrap_or(60);
                                            let old=minutes;
                                            ui.horizontal(|ui| {
                                                ui.label("Refresh every");
                                                egui::ComboBox::from_id_salt("interval").selected_text(format!("{minutes} minutes")).show_ui(ui,|ui| {
                                                    for (value,label) in [(15,"15 minutes"),(30,"30 minutes"),(60,"1 hour"),(360,"6 hours"),(1440,"1 day")] { ui.selectable_value(&mut minutes,value,label); }
                                                });
                                            });
                                            if minutes!=old && let Err(e)=app.s.run("caldav.configure",&json!({"folder":folder,"refreshMin":minutes})) { st.error=e.to_string(); }
                                            ui.add_space(8.0);
                                            let label=ui.label("New app password");
                                            ui.add(TextEdit::singleline(&mut st.replacement_password).password(true).desired_width(f32::INFINITY)).labelled_by(label.id);
                                            if primary(ui,app,!syncing && st.job.is_none() && !st.replacement_password.is_empty(),"Save password and reconnect") {
                                                match app.s.run("caldav.configure",&json!({"folder":folder,"password":st.replacement_password})) {
                                                    Ok(_)=> { st.replacement_password.clear(); start_refresh(app,st,folder); },
                                                    Err(e)=>st.error=e.to_string(),
                                                }
                                            }
                                            ui.add_space(8.0);
                                            if st.disconnecting==Some(folder) {
                                                muted(ui,app,"Remove this connection and saved password? Downloaded events will remain as an editable local calendar.");
                                                ui.horizontal(|ui| {
                                                    if ui.button("Keep connection").clicked() { st.disconnecting=None; }
                                                    if ui.button("Disconnect and keep events").clicked() {
                                                        match app.s.run("caldav.disconnect",&json!({"folder":folder})) {
                                                            Ok(_)=> { if syncing && st.refreshing {st.job=None;} st.managing=None; st.disconnecting=None; st.replacement_password.clear(); st.message="Disconnected. Events kept as a local calendar.".into(); st.error.clear(); },
                                                            Err(e)=>st.error=e.to_string(),
                                                        }
                                                    }
                                                });
                                            } else if ui.button("Disconnect…").clicked() { st.disconnecting=Some(folder); }
                                        }
                                        ui.add_space(8.0); ui.separator();
                                    });
                                }
                            },
                        );
                    }
                    muted(
                        ui,
                        app,
                        &format!(
                            "Credential storage · {}",
                            saved
                                .get("credentialStore")
                                .and_then(Value::as_str)
                                .unwrap_or("Profile credential store")
                        ),
                    );
                }
                if !st.message.is_empty() {
                    ui.label(&st.message);
                }
                if !st.error.is_empty() {
                    ui.colored_label(app.theme.danger, &st.error);
                }
                if st.managing.is_none() {
                    muted(ui, app, "Check a CalDAV account using your provider’s endpoint and app password.");
                    ui.add_space(14.0);
                    ui.add_enabled_ui(st.job.is_none(), |ui| {
                        for (label, value, hint, secret) in [
                            ("CalDAV endpoint", &mut st.endpoint, "https://calendar.example.net/", false),
                            ("Username", &mut st.username, "person@example.net", false),
                            ("App password", &mut st.password, "", true),
                        ] {
                            let l = ui.label(label);
                            ui.add(
                                TextEdit::singleline(value)
                                    .hint_text(hint)
                                    .password(secret)
                                    .desired_width(f32::INFINITY),
                            )
                            .labelled_by(l.id);
                            ui.add_space(8.0);
                        }
                    });
                    muted(ui, app, "Read-only · Refresh includes 90 days back and one year ahead");
                    if st.job.is_some() {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label(if st.refreshing {
                                "Updating calendar…"
                            } else {
                                "Looking for calendars…"
                            });
                        });
                    }
                    if let Some(calendars) =
                        st.result.as_ref().and_then(|v| v.get("calendars")).and_then(Value::as_array)
                    {
                        ui.add_space(12.0);
                        ui.label(RichText::new(format!("{} calendars found", calendars.len())).strong());
                        egui::ScrollArea::vertical().max_height(210.0).show(ui, |ui| {
                            for calendar in calendars {
                                ui.separator();

                                let url = calendar.get("url").and_then(Value::as_str).unwrap_or_default();
                                let supported =
                                    calendar.get("components").and_then(Value::as_array).is_none_or(|v| {
                                        v.is_empty() || v.iter().any(|v| v.as_str() == Some("VEVENT"))
                                    });
                                if ui
                                    .add_enabled(
                                        supported,
                                        egui::RadioButton::new(
                                            st.selected.as_deref() == Some(url),
                                            calendar
                                                .get("name")
                                                .and_then(Value::as_str)
                                                .unwrap_or("Calendar"),
                                        ),
                                    )
                                    .clicked()
                                {
                                    st.selected = Some(url.into());
                                }
                                ui.add(
                                    egui::Label::new(
                                        RichText::new(url).color(app.theme.text_muted).size(12.0),
                                    )
                                    .wrap(),
                                );
                                if ui.small_button("Copy calendar address").clicked() {
                                    ctx.copy_text(url.to_owned());
                                }
                            }
                        });
                        muted(
                            ui,
                            app,
                            "Select a calendar and re-enter its app password to save the connection.",
                        );
                    }
                    if let Some(url) = st.selected.clone()
                        && primary(
                            ui,
                            app,
                            st.job.is_none() && !st.password.is_empty(),
                            "Add read-only calendar",
                        )
                    {
                        let name = st
                            .result
                            .as_ref()
                            .and_then(|v| v.get("calendars"))
                            .and_then(Value::as_array)
                            .and_then(|v| {
                                v.iter().find(|c| c.get("url").and_then(Value::as_str) == Some(&url))
                            })
                            .and_then(|c| c.get("name"))
                            .and_then(Value::as_str)
                            .unwrap_or("Calendar");
                        match app.s.run(
                        "caldav.add",
                        &json!({"url":url,"username":st.username.trim(),"password":st.password,"name":name}),
                    ) {
                        Ok(v) => {
                            st.password.clear();
                            st.selected = None;
                            st.result = None;
                            st.message = "Calendar saved.".into();
                            st.error.clear();
                            match app.s.run("caldav.refresh", &json!({"folder":v.get("folder")})) {
                                Ok(v) => {
                                    st.job = v.get("job").and_then(Value::as_u64);
                                    st.refreshing = true;
                                }
                                Err(e) => st.error = e.to_string(),
                            }
                        }
                        Err(e) => st.error = e.to_string(),
                    }
                    }
                } else if ui.button("Add another calendar").clicked() {
                    st.managing = None;
                    st.replacement_password.clear();
                    st.disconnecting = None;
                }
                let mut out = Out::Keep;
                footer(ui, |ui| {
                    if ui.button("Close").clicked() {
                        st.password.clear();
                        st.replacement_password.clear();
                        out = Out::Close;
                    }
                    let ready = st.job.is_none()
                        && !st.endpoint.trim().is_empty()
                        && !st.username.trim().is_empty()
                        && !st.password.is_empty();
                    if st.managing.is_none() && primary(ui, app, ready, "Find calendars") {
                        st.error.clear();
                        st.result = None;
                        st.selected = None;
                        st.refreshing = false;
                        st.message.clear();
                        let result = app.s.run("caldav.discover", &json!({"endpoint":st.endpoint.trim(), "username":st.username.trim(), "password":st.password}));
                        match result {
                            Ok(v) => {
                                st.job = v.get("job").and_then(Value::as_u64);
                                st.password.clear();
                            }
                            Err(e) => st.error = e.to_string(),
                        }
                    }
                });
                out
            })
        },
    )
}

fn start_refresh(app: &mut App, st: &mut State, folder: u64) {
    match app.s.run("caldav.refresh", &json!({"folder":folder})) {
        Ok(v) => {
            st.job = v.get("job").and_then(Value::as_u64);
            st.refreshing = true;
            st.error.clear();
            st.message.clear();
        }
        Err(e) => st.error = e.to_string(),
    }
}
fn stamp(app: &App, unix: i64) -> String {
    let d = mailcraft_model::dates::LocalTime::of(
        mailcraft_model::DateTime { unix, offset_min: 0 },
        app.s.tz_offset_min,
    );
    format!("{:04}-{:02}-{:02} {:02}:{:02}", d.year, d.month, d.day, d.hour, d.minute)
}
