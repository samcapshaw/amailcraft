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
    result: Option<Value>,
    error: String,
}
pub(super) fn show(ctx: &Context, app: &mut App, d: &Dlg<'_>) -> Out {
    with_state(
        ctx,
        app,
        d.uid,
        |_| State::default(),
        |st, app| {
            if let Some(job) = st.job {
                match app.s.run("caldav.discover", &json!({"job":job})) {
                    Ok(v) if v.get("status").and_then(Value::as_str) == Some("done") => {
                        st.error = v.get("error").and_then(Value::as_str).unwrap_or_default().to_owned();
                        st.result = Some(v);
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
            window(ctx, app, d, "CalDAV calendars", 580.0, |ui, app| {
                ui.label(RichText::new("Find your calendars").size(24.0));
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
                muted(ui, app, "Discovery only · No events downloaded or changed · Password is not saved");
                if st.job.is_some() {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label("Looking for calendars…");
                    });
                }
                if !st.error.is_empty() {
                    ui.colored_label(app.theme.danger, &st.error);
                }
                if let Some(calendars) =
                    st.result.as_ref().and_then(|v| v.get("calendars")).and_then(Value::as_array)
                {
                    ui.add_space(12.0);
                    ui.label(RichText::new(format!("{} calendars found", calendars.len())).strong());
                    egui::ScrollArea::vertical().max_height(210.0).show(ui, |ui| {
                        for calendar in calendars {
                            ui.separator();
                            ui.label(calendar.get("name").and_then(Value::as_str).unwrap_or("Calendar"));
                            let url = calendar.get("url").and_then(Value::as_str).unwrap_or_default();
                            ui.add(
                                egui::Label::new(RichText::new(url).color(app.theme.text_muted).size(12.0))
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
                        "Connection checked. Adding these calendars and syncing events is a future step.",
                    );
                }
                let mut out = Out::Keep;
                footer(ui, |ui| {
                    if ui.button("Close").clicked() {
                        st.password.clear();
                        out = Out::Close;
                    }
                    let ready = st.job.is_none()
                        && !st.endpoint.trim().is_empty()
                        && !st.username.trim().is_empty()
                        && !st.password.is_empty();
                    if primary(ui, app, ready, "Find calendars") {
                        st.error.clear();
                        st.result = None;
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
