//! Headless screenshot of the main window with the sample mailbox (egui_kittest + wgpu, no window).
//!
//! ```text
//! cargo run -p mailcraft-ui-egui --example ui_shot -- <out.png> [WIDTHxHEIGHT] [--dark] [--module NAME]
//!     [--reading right|bottom|off] [--select] [--classic] [--mac] [--sidebar-hidden] [--shared] [--compose] [--compose-preview] [--settings] [--accounts] [--interface apple|outlook|omarchy] [--custom-colors] [--connected-preview] [--graph-setup] [--graph-edit event|contact|response|create|delete] [--graph-pending] [--scale F]
//! ```
//!
//! Defaults: 1920x1040, light theme, Mail module, nothing selected, scale 1. `--module` takes mail, calendar,
//! people, tasks, notes. `--select` selects the first message so the reading pane shows it.
//!
//! wgpu needs a GPU adapter (Vulkan/Metal/DX12/GL, or a software one such as lavapipe/llvmpipe). When none can
//! be created the program prints why and exits with status 3 without writing a file.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::process::ExitCode;

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use mailcraft_engine::queries::{self, ListRow};
use mailcraft_engine::sample::sample_session;
use mailcraft_ui_egui::App;
use serde_json::json;

struct Opts {
    out: String,
    w: f32,
    h: f32,
    dark: bool,
    classic: bool,
    mac: bool,
    interface: Option<String>,
    custom_colors: bool,
    shared: bool,
    sidebar_hidden: bool,
    compose: bool,
    compose_preview: bool,
    settings: bool,
    settings_page: Option<String>,
    more_menu: bool,
    dialog: Option<String>,
    accounts: bool,
    graph_setup: bool,
    account_setup: Option<String>,
    connected_preview: bool,
    graph_edit: Option<String>,
    graph_pending: bool,
    unified_sent: bool,
    module: Option<String>,
    reading: Option<String>,
    select: bool,
    scale: f32,
}

fn parse() -> Result<Opts, String> {
    let mut args = std::env::args().skip(1);
    let mut o = Opts {
        out: String::new(),
        w: 1920.0,
        h: 1040.0,
        dark: false,
        classic: false,
        mac: false,
        interface: None,
        custom_colors: false,
        shared: false,
        sidebar_hidden: false,
        compose: false,
        compose_preview: false,
        settings: false,
        settings_page: None,
        more_menu: false,
        dialog: None,
        accounts: false,
        graph_setup: false,
        account_setup: None,
        connected_preview: false,
        graph_edit: None,
        graph_pending: false,
        unified_sent: false,
        module: None,
        reading: None,
        select: false,
        scale: 1.0,
    };
    while let Some(a) = args.next() {
        match a.as_str() {
            "--sidebar-hidden" => {
                o.sidebar_hidden = true;
                o.mac = true;
            }
            "--shared" => {
                o.shared = true;
                o.mac = true;
            }
            "--compose-preview" => {
                o.compose_preview = true;
                o.compose = true;
                o.mac = true;
            }
            "--compose" => {
                o.compose = true;
                o.mac = true;
            }
            "--unified-sent" => {
                o.unified_sent = true;
                o.mac = true;
            }
            "--settings-page" => {
                o.settings_page = Some(args.next().ok_or("--settings-page needs accounts or appearance")?);
                o.mac = true;
            }
            "--more-menu" => {
                o.more_menu = true;
                o.mac = true;
            }
            "--account-setup" => {
                o.account_setup = Some(args.next().ok_or("--account-setup needs entry, shared or imap")?);
                o.mac = true;
            }
            "--graph-edit" => {
                o.connected_preview = true;
                o.graph_edit = Some(args.next().ok_or("--graph-edit needs event, contact or response")?);
            }
            "--graph-pending" => {
                o.connected_preview = true;
                o.graph_pending = true;
            }
            "--connected-preview" => {
                o.connected_preview = true;
            }
            "--graph-setup" => {
                o.graph_setup = true;
                o.mac = true;
            }
            "--accounts" => {
                o.accounts = true;
                o.mac = true;
            }
            "--settings" => {
                o.settings = true;
                o.mac = true;
            }
            "--dialog" => {
                o.dialog = Some(args.next().ok_or("--dialog needs a command id")?);
                o.mac = true;
            }
            "--custom-colors" => {
                o.custom_colors = true;
                o.mac = true;
                o.settings = true;
            }
            "--interface" => {
                o.interface = Some(args.next().ok_or("--interface needs outlook, apple or omarchy")?);
                o.mac = true;
            }
            "--mac" => o.mac = true,
            "--dark" => o.dark = true,
            "--classic" => o.classic = true,
            "--select" => o.select = true,
            "--module" => o.module = Some(args.next().ok_or("--module needs a name")?),
            "--reading" => o.reading = Some(args.next().ok_or("--reading needs right, bottom or off")?),
            "--scale" => {
                let v = args.next().ok_or("--scale needs a number")?;
                o.scale = v
                    .parse::<f32>()
                    .ok()
                    .filter(|s| s.is_finite() && *s > 0.2 && *s <= 4.0)
                    .ok_or("bad --scale")?;
            }
            s if s.contains('x') && s.chars().next().is_some_and(|c| c.is_ascii_digit()) => {
                let (w, h) = s.split_once('x').ok_or("size must be WIDTHxHEIGHT")?;
                let w = w.parse::<f32>().map_err(|_| "bad width")?;
                let h = h.parse::<f32>().map_err(|_| "bad height")?;
                if !(100.0..=8000.0).contains(&w) || !(100.0..=8000.0).contains(&h) {
                    return Err("size out of range (100..8000)".into());
                }
                (o.w, o.h) = (w, h);
            }
            s if s.starts_with('-') => return Err(format!("unknown option `{s}`")),
            s => o.out = s.to_owned(),
        }
    }
    if o.out.is_empty() {
        return Err("usage: ui_shot <out.png> [WIDTHxHEIGHT] [--dark] [--module NAME] [--reading POS] [--select] [--classic] [--mac] [--sidebar-hidden] [--shared] [--compose] [--compose-preview] [--settings] [--accounts] [--interface apple|outlook|omarchy] [--custom-colors] [--connected-preview] [--graph-setup] [--graph-edit event|contact|response|create|delete] [--graph-pending] [--scale F]".into());
    }
    Ok(o)
}

fn shoot(o: &Opts) -> Result<(), String> {
    let mut session = sample_session();
    if o.mac {
        session.mailbox = mailcraft_engine::sample::mac_sample_mailbox(session.now());
    }
    let mut app = App::new(session);
    if let Some(interface) = &o.interface {
        app.run("options.set", json!({"key":"general.interface", "value":interface}));
    }
    if o.mac {
        app.enable_mac_style();
    }
    if o.custom_colors {
        app.run("options.set", json!({"key":"general.customColors", "value":true}));
    }
    if o.sidebar_hidden {
        app.run("view.folderPane", json!({"mode":"off"}));
    }
    if o.dark {
        app.run("options.set", json!({ "key": "view.darkMode", "value": true }));
    }
    if o.classic {
        app.run("app.toggleRibbonMode", json!({ "mode": "classic" }));
    }
    if let Some(pos) = &o.reading {
        app.run("view.readingPane", json!({ "position": pos }));
    }
    if o.shared
        && let Some(inbox) = app
            .s
            .mailbox
            .accounts
            .iter()
            .find(|a| a.address == "design@example.net")
            .and_then(|a| app.s.mailbox.role_folder(a.id, mailcraft_engine::model::FolderRole::Inbox))
    {
        app.run("nav.selectFolder", json!({"folder": inbox.0}));
    }
    if o.unified_sent {
        app.run("nav.unifiedSent", json!({}));
    }
    if let Some(mode) = &o.account_setup {
        app.s.view.work_offline = true;
        if mode == "imap" {
            let account = app.s.run("accounts.add", &json!({"kind":"imap", "address":"person@example.net", "displayName":"Personal", "incoming":{"host":"imap.example.net", "port":993,"security":"tls"}, "outgoing":{"host":"smtp.example.net", "port":465,"security":"tls"}})).map_err(|e| e.to_string())?;
            app.s.notify(mailcraft_engine::Notice::OpenDialog(
                "addAccount".into(),
                json!({"change":account.get("account")}),
            ));
        } else {
            app.s.notify(mailcraft_engine::Notice::OpenDialog(
                if mode == "shared" { "addSharedMailbox" } else { "addAccount" }.into(),
                json!({}),
            ));
        }
    }
    if o.graph_setup
        && let Ok(account) = app.s.run("accounts.add", &json!({"kind":"graph", "graphCalendars":true, "graphContacts":true, "address":"graph@example.net", "default":false, "incoming":{"host":"https://graph.microsoft.com/v1.0"}})) {
            app.s.notify(mailcraft_engine::Notice::OpenDialog("addAccount".into(), json!({"change":account.get("account")})));
        }
    if o.accounts {
        app.run("file.accountSettings", json!({}));
    }
    if let Some(command) = &o.dialog {
        app.run(command, json!({}));
    }
    if o.settings {
        app.run("options.open", json!({"page": "general"}));
    }
    if let Some(page) = &o.settings_page {
        app.s.notify(mailcraft_engine::Notice::OpenDialog(
            "options".into(),
            json!({"page":"general","workspace":page}),
        ));
    }
    if o.compose {
        app.run("mail.newEmail", json!({}));
        if o.compose_preview
            && let Some(cid) = app.s.composes.keys().next().copied()
        {
            app.run(
                "compose.signatureCreate",
                json!({"name":"Work","text":"Jordan Lee\nDesign team · Studio North"}),
            );
            app.run("compose.setTo", json!({"compose":cid,"value":"Alex Morgan <alex@example.net>"}));
            app.run("compose.setSubject", json!({"compose":cid,"value":"A few thoughts for Thursday"}));
            app.run("compose.insertText",json!({"compose":cid,"text":"Hi Alex,\n\nI’ve attached a few thoughts ahead of our review. The direction feels good—let’s keep the details simple and give the work a little room to breathe.\n\nLooking forward to hearing what you think.\n\n"}));
            app.run("compose.signature", json!({"compose":cid,"name":"Work"}));
        }
    }
    if o.select {
        let first = queries::list_rows(&app.s).into_iter().find_map(|r| match r {
            ListRow::Item(i) => Some(i.id.0),
            ListRow::Group { .. } => None,
        });
        if let Some(id) = first {
            app.run("nav.selectItem", json!({ "id": id }));
        }
    }
    if let Some(m) = &o.module {
        app.run(&format!("nav.{m}"), json!({}));
    }
    if o.connected_preview {
        let now = app.s.now();
        let account = app.s.mailbox.default_account().map(|a| a.id);
        // Synthetic offline fixture only: no server endpoint, token or automatic network activity.
        if let Some(id) = account {
            if let Some(a) = app.s.mailbox.accounts.iter_mut().find(|a| a.id == id) {
                a.kind = mailcraft_model::AccountKind::Graph;
            }
            app.run("accounts.options",json!({"account":id.0,"graphCalendars":true,"graphContacts":true,"graphEditing":(o.graph_edit.is_some() || o.graph_pending),"intervalMinutes":0,"push":false}));
        }

        let items: Vec<_> = app
            .s
            .mailbox
            .appointments
            .values()
            .map(|a| (a.folder, a.id, true))
            .chain(app.s.mailbox.contacts.values().map(|c| (c.folder, c.id, false)))
            .collect();
        for folder in app.s.mailbox.folders.values_mut().filter(|f| {
            matches!(f.kind, mailcraft_model::FolderKind::Calendar | mailcraft_model::FolderKind::Contacts)
                && Some(f.account) == account
        }) {
            folder.graph_source = Some(mailcraft_model::folder::GraphFolderSource {
                remote_id: "synthetic-preview".into(),
                items: items
                    .iter()
                    .filter(|(f, _, _)| *f == folder.id)
                    .map(|(_, id, _)| (format!("synthetic-{}", id.0), *id))
                    .collect(),
                metadata: items
                    .iter()
                    .filter(|(f, _, _)| *f == folder.id)
                    .map(|(_, id, event)| {
                        (
                            format!("synthetic-{}", id.0),
                            mailcraft_model::folder::GraphItemMeta {
                                etag: "W/\"synthetic\"".into(),
                                can_edit: !(o.graph_edit.as_deref() == Some("response") && *event),
                                can_respond: o.graph_edit.as_deref() == Some("response") && *event,
                                event_type: if o.graph_edit.as_deref() == Some("delete") {
                                    "singleInstance"
                                } else {
                                    "occurrence"
                                }
                                .into(),
                            },
                        )
                    })
                    .collect(),
                last_sync: Some(now),
                window_start: (folder.kind == mailcraft_model::FolderKind::Calendar)
                    .then(|| mailcraft_model::DateTime::from_unix(now.unix - 90 * 86400, 0)),
                window_end: (folder.kind == mailcraft_model::FolderKind::Calendar)
                    .then(|| mailcraft_model::DateTime::from_unix(now.unix + 366 * 86400, 0)),
            });
        }
    }
    if o.graph_edit.is_some() || o.graph_pending {
        app.s.view.work_offline = true;
        let event = o.graph_edit.as_deref().is_some_and(|kind| kind != "contact");
        let id = if event {
            app.s
                .mailbox
                .appointments
                .values()
                .find(|a| app.s.mailbox.folder(a.folder).is_some_and(|f| f.graph_source.is_some()))
                .map(|a| a.id)
        } else {
            app.s
                .mailbox
                .contacts
                .values()
                .find(|c| app.s.mailbox.folder(c.folder).is_some_and(|f| f.graph_source.is_some()))
                .map(|c| c.id)
        };
        if let Some(id) = id {
            if o.graph_pending {
                app.run("graph.edit", json!({"id":id.0,"fields":{"company":"Studio North"}}));
                app.run("graph.pending", json!({}));
            } else if o.graph_edit.as_deref() == Some("create") {
                if let Some(a) = app.s.mailbox.appointments.get(&id) {
                    app.run("graph.createAppointment", json!({"folder":a.folder.0}));
                }
            } else if o.graph_edit.as_deref() == Some("delete") {
                if let Some(a) = app.s.mailbox.appointments.get_mut(&id) {
                    a.is_meeting = false;
                    a.attendees.clear();
                    a.recurrence = None;
                    a.recurrence_id = None;
                    a.subject = "Design review".into();
                }
                app.run("graph.deleteAppointment", json!({"id":id.0}));
            } else {
                app.run("graph.edit", json!({"id":id.0}));
            }
        }
    }
    let mut fonts_done = false;
    let mut harness = Harness::builder()
        .with_size(egui::vec2(o.w, o.h))
        .with_pixels_per_point(o.scale)
        .wgpu()
        .build_ui_state(
            move |ui, app: &mut App| {
                if !fonts_done {
                    App::install_fonts(ui.ctx());
                    fonts_done = true;
                    ui.ctx().request_repaint();
                    return;
                }
                app.ui_root(ui);
            },
            app,
        );
    // Fonts and icon textures settle over a few frames.
    harness.run_steps(6);
    if o.more_menu {
        harness.get_by_label("More").click();
        harness.run_steps(3);
    }
    let image = harness.render()?;
    image.save(&o.out).map_err(|e| format!("cannot write {}: {e}", o.out))
}

fn main() -> ExitCode {
    let o = match parse() {
        Ok(o) => o,
        Err(e) => {
            eprintln!("ui_shot: {e}");
            return ExitCode::from(2);
        }
    };
    // kittest panics when no wgpu adapter can be created; report that instead of crashing.
    match catch_unwind(AssertUnwindSafe(|| shoot(&o))) {
        Ok(Ok(())) => {
            println!("wrote {} ({}x{} @{}x)", o.out, o.w, o.h, o.scale);
            ExitCode::SUCCESS
        }
        Ok(Err(e)) => {
            eprintln!("ui_shot: {e}");
            ExitCode::FAILURE
        }
        Err(payload) => {
            let msg = payload
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| payload.downcast_ref::<&str>().map(|s| (*s).to_owned()))
                .unwrap_or_else(|| "unknown error".into());
            eprintln!("ui_shot: wgpu could not render on this machine ({msg}); no screenshot written");
            ExitCode::from(3)
        }
    }
}
