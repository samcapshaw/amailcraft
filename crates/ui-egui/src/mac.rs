//! Integrated desktop shell. All mail operations dispatch through the existing command registry.
use crate::{App, responsive::Frame, widgets, with_shell};
use egui::{Align, Layout, Rect, RichText, Sense, Stroke, Ui, UiBuilder, pos2, vec2};
use mailcraft_engine::Module;
use mailcraft_engine::queries::{self, FolderRowKind};
use serde_json::json;

impl App {
    /// Compatibility entry point for hosts that previously opted into the Mac shell.
    pub fn enable_mac_style(&mut self) {
        self.enable_desktop_themes();
    }

    /// Enable saved desktop layouts on any platform, with Apple as the default.
    pub fn enable_desktop_themes(&mut self) {
        self.theme_chooser_enabled = true;
        self.theme = crate::theme::Theme::for_settings(&self.s.settings, self.s.view.dark);
        self.mac_style = self.theme.mac_style;
        if self.mac_style {
            self.run("nav.unifiedInbox", json!({}));
            self.run("view.readingPane", json!({"position":"right"}));
            if self.s.mailbox.views.get(&self.s.selection.folder).cloned().unwrap_or_default().focused_inbox {
                self.run("view.focusedInbox", json!({"on":false}));
            }
        }
    }
}

fn child(ui: &mut Ui, rect: Rect, salt: &str, layout: Layout) -> Ui {
    let mut c = ui.new_child(UiBuilder::new().id_salt(salt).max_rect(rect).layout(layout));
    c.set_clip_rect(rect.intersect(ui.clip_rect()));
    c
}

fn action(ui: &mut Ui, app: &mut App, icon: &str, label: &str, command: &str, enabled: bool) {
    let r = widgets::icon_button(ui, app, icon, "", enabled).on_hover_text(label);
    r.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, label));
    if r.clicked() {
        app.run(command, json!({}));
    }
}

pub fn main_window(ui: &mut Ui, app: &mut App, frame: &Frame) {
    let rect = ui.max_rect();
    let toolbar = Rect::from_min_max(
        rect.min,
        pos2(
            rect.right(),
            (rect.top() + if app.mail_like_module() && frame.width < 680.0 { 104.0 } else { 58.0 })
                .min(rect.bottom()),
        ),
    );
    ui.painter().rect_filled(toolbar, 0, app.theme.chrome);
    ui.painter().hline(toolbar.x_range(), toolbar.bottom() - 0.5, Stroke::new(1.0, app.theme.separator));
    let drag = ui.interact(toolbar, ui.id().with("mac-toolbar-drag"), Sense::drag());
    if drag.drag_started() {
        ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
    }
    with_shell(ui.ctx(), |s| {
        s.layout.tabs_shown = false;
        s.layout.ribbon_shown = false;
    });
    let left = Rect::from_min_max(
        toolbar.min + vec2(14.0, 0.0),
        pos2(
            (toolbar.left() + if frame.width < 700.0 { 180.0 } else { 230.0 }).min(toolbar.right()),
            toolbar.top() + 58.0,
        ),
    );
    let mut c = child(ui, left, "mac-title", Layout::left_to_right(Align::Center));
    let shown = app.s.view.folder_pane != mailcraft_model::FolderPaneState::Off;
    if widgets::icon_toggle(
        &mut c,
        app,
        "folder-pane",
        if shown { "Hide mailboxes" } else { "Show mailboxes" },
        shown,
        28.0,
    )
    .clicked()
    {
        app.run("view.folderPane", json!({"mode":if shown {"off"}else{"normal"}}));
    }
    c.add_space(8.0);
    let title = RichText::new(if app.theme.terminal_style { "◇ MailCraft" } else { "MailCraft" })
        .font(app.theme.detail_font(16.0))
        .strong()
        .color(if app.theme.terminal_style { app.theme.importance_low } else { app.theme.text });
    c.label(title);
    let tools = Rect::from_min_max(
        pos2((toolbar.right() - 650.0).max(left.right()), toolbar.top()),
        pos2(toolbar.right(), toolbar.top() + 58.0),
    );
    let mut c = child(ui, tools.shrink2(vec2(12.0, 0.0)), "mac-tools", Layout::left_to_right(Align::Center));
    let mail = app.mail_like_module();
    let selected = !app.s.selection.items.is_empty() || app.s.selection.focus.is_some();
    if mail {
        action(&mut c, app, "new-mail", "New message", "mail.newEmail", true);
        c.separator();
        action(&mut c, app, "archive", "Archive", "mail.archive", selected);
        action(&mut c, app, "deleted", "Move to Trash", "mail.delete", selected);
        if frame.width >= 900.0 {
            c.separator();
            action(&mut c, app, "reply", "Reply", "mail.reply", selected);
        }
    } else {
        module_toolbar(&mut c, app, frame);
    }
    c.menu_button("More", |ui| {
        ui.set_min_width(248.0);
        ui.spacing_mut().item_spacing.y = 5.0;
        ui.label(
            RichText::new(if mail { "Mail tools" } else { "Tools" }).size(12.0).color(app.theme.text_muted),
        );
        ui.add_space(4.0);
        if mail {
            more_item(ui, app, "quick-steps", "Quick steps…", "", "quicksteps.manage", json!({}));
            more_item(ui, app, "filter", "Rules…", "", "rules.manage", json!({}));
        } else {
            ui.menu_button("All tools", |ui| crate::ribbon::module_tools_menu(ui, app));
        }
        ui.separator();
        more_item(ui, app, "folder-open", "Import…", "", "file.openExport", json!({}));
        more_item(ui, app, "save", "Export…", "", "file.openExport", json!({"export":true}));
        ui.separator();
        ui.menu_button("View & connection", |ui| {
            more_item(ui, app, "sync", "Get new mail", "", "sendReceive.all", json!({}));
            if mail {
                ui.menu_button("Reading layout", |ui| {
                    for (label, position) in
                        [("Below messages", "bottom"), ("Reader on right", "right"), ("Hidden", "off")]
                    {
                        if ui.button(label).clicked() {
                            app.run("view.readingPane", json!({"position":position}));
                            ui.close();
                        }
                    }
                });
                if ui.button("Toggle Focused inbox").clicked() {
                    app.run("view.focusedInbox", json!({}));
                    ui.close();
                }
            }
            more_item(ui, app, "calendar", "Find CalDAV calendars…", "", "caldav.discover", json!({}));
            if ui.button("Accounts…").clicked() {
                app.run("file.accountSettings", json!({}));
                ui.close();
            }
            if ui.button("Add shared or delegated mailbox…").clicked() {
                app.ui.dialogs.push(("addSharedMailbox".into(), json!({})));
                ui.close();
            }
        });
        ui.separator();
        more_item(
            ui,
            app,
            "settings",
            "Settings…",
            if cfg!(target_os = "macos") { "⌘ ," } else { "Ctrl ," },
            "file.options",
            json!({}),
        );
    });
    if mail {
        if frame.width < 680.0 {
            let search = Rect::from_min_max(
                pos2(toolbar.left() + 16.0, toolbar.top() + 62.0),
                pos2(toolbar.right() - 16.0, toolbar.bottom() - 10.0),
            );
            search_field(ui, app, search);
        } else if c.available_width() > 140.0 {
            let width = c.available_width().min(300.0);
            let search = Rect::from_center_size(
                pos2(toolbar.right() - width / 2.0 - 16.0, toolbar.top() + 29.0),
                vec2(width, 34.0),
            );
            search_field(ui, app, search);
        }
    }

    let navigation =
        Rect::from_min_max(pos2(rect.left(), (rect.bottom() - 40.0).max(toolbar.bottom())), rect.max);
    module_navigation(ui, app, navigation);
    let content =
        Rect::from_min_max(pos2(rect.left(), toolbar.bottom()), pos2(rect.right(), navigation.top()));
    if content.is_positive() {
        let mut c = child(ui, content, "mac-content", Layout::top_down(Align::Min));
        crate::chrome::content_area(&mut c, app, frame);
    }
    if app.theme.terminal_style {
        ui.painter().rect_stroke(
            rect.shrink(0.5),
            2,
            Stroke::new(1.0, app.theme.accent),
            egui::StrokeKind::Inside,
        );
    }
}

fn more_item(
    ui: &mut Ui,
    app: &mut App,
    icon: &str,
    label: &str,
    shortcut: &str,
    command: &str,
    params: serde_json::Value,
) {
    let (rect, response) = ui.allocate_exact_size(vec2(248.0, 36.0), Sense::click());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, label));
    if response.hovered() || response.has_focus() {
        ui.painter().rect_filled(rect, 4, app.theme.hover);
    }
    app.icons.paint(
        ui,
        Rect::from_center_size(pos2(rect.left() + 17.0, rect.center().y), vec2(18.0, 18.0)),
        icon,
        app.theme.text,
    );
    ui.painter().text(
        pos2(rect.left() + 39.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        label,
        egui::FontId::proportional(15.0),
        app.theme.text,
    );
    ui.painter().text(
        pos2(rect.right() - 10.0, rect.center().y),
        egui::Align2::RIGHT_CENTER,
        shortcut,
        app.theme.detail_font(12.0),
        app.theme.text_muted,
    );
    if response.clicked() {
        app.run(command, params);
        ui.close();
    }
}

fn module_toolbar(ui: &mut Ui, app: &mut App, frame: &Frame) {
    let (icon, label, cmd) = match app.s.view.module {
        Module::Calendar => ("appointment", "New event", "calendar.newAppointment"),
        Module::People => ("person", "New contact", "people.new"),
        Module::Tasks => ("task", "New task", "tasks.new"),
        Module::Notes => ("note", "New note", "notes.new"),
        _ => return,
    };
    if widgets::icon_button(ui, app, icon, label, true).clicked() {
        let folder = app.s.selection.folder;
        if app.s.view.module == Module::Calendar
            && app.s.mailbox.folder(folder).is_some_and(|f| f.graph_source.is_some())
        {
            app.run("graph.createAppointment", json!({"folder":folder.0}));
        } else {
            app.run(cmd, json!({"form":true}));
        }
    }
    if app.s.view.module == Module::Calendar
        && frame.width >= 700.0
        && widgets::icon_button(ui, app, "meeting", "New meeting", true).clicked()
    {
        app.run("calendar.newMeeting", json!({"form":true}));
    }
    ui.separator();
}

/// Persistent, compact navigation keeps the full application visible from every module.
fn module_navigation(ui: &mut Ui, app: &mut App, rect: Rect) {
    ui.painter().rect_filled(
        rect,
        0,
        if app.theme.terminal_style { app.theme.backdrop.gamma_multiply(0.82) } else { app.theme.chrome },
    );
    ui.painter().hline(rect.x_range(), rect.top(), Stroke::new(0.5, app.theme.separator));
    let labels = rect.width() >= 560.0;
    let width = if labels { 106.0 } else { ((rect.width() - 24.0) / 5.0).clamp(28.0, 64.0) };
    for (i, (module, icon, label, cmd)) in [
        (Module::Mail, "mail", "Mail", "nav.mail"),
        (Module::Calendar, "calendar", "Calendar", "nav.calendar"),
        (Module::People, "people", "Contacts", "nav.people"),
        (Module::Tasks, "tasks", "Tasks", "nav.tasks"),
        (Module::Notes, "note", "Notes", "nav.notes"),
    ]
    .into_iter()
    .enumerate()
    {
        let row = Rect::from_min_size(
            pos2(rect.left() + 12.0 + i as f32 * width, rect.top() + 5.0),
            vec2(width - 4.0, 30.0),
        );
        let selected = app.s.view.module == module || module == Module::Mail && app.mail_like_module();
        let r = ui.interact(row, ui.id().with(("mac-module", cmd)), Sense::click());
        r.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, true, selected, label));
        if selected || r.hovered() {
            ui.painter().rect_filled(
                row,
                if app.theme.terminal_style { 3 } else { 6 },
                if selected { app.theme.selected_folder } else { app.theme.hover },
            );
        }
        if r.has_focus() {
            ui.painter().rect_stroke(row, 6, Stroke::new(1.0, app.theme.accent), egui::StrokeKind::Inside);
        }
        let ink = if selected && app.theme.terminal_style {
            app.theme.importance_low
        } else if selected {
            app.theme.text
        } else {
            app.theme.text_muted
        };
        let x = if labels { row.left() + 17.0 } else { row.center().x };
        app.icons.paint(ui, Rect::from_center_size(pos2(x, row.center().y), vec2(15.0, 15.0)), icon, ink);
        if labels {
            ui.painter().text(
                pos2(row.left() + 31.0, row.center().y),
                egui::Align2::LEFT_CENTER,
                label,
                app.theme.detail_font(12.0),
                ink,
            );
        }
        if r.on_hover_text(label).clicked() {
            app.run(cmd, json!({}));
        }
    }
    if app.theme.terminal_style && rect.width() > 860.0 {
        let status = app.s.sync_status();
        let (label, ink) = if app.s.view.work_offline {
            ("● Offline", app.theme.text_muted)
        } else if status.busy {
            ("● Syncing", app.theme.importance_low)
        } else if status.accounts.iter().any(|s| s.error.is_some()) {
            ("● Needs attention", app.theme.flag)
        } else if status.accounts.iter().any(|s| s.last_sync.is_some()) {
            ("● Synced", app.theme.complete)
        } else {
            ("● Ready", app.theme.complete)
        };
        ui.painter().text(
            pos2(rect.right() - 18.0, rect.center().y),
            egui::Align2::RIGHT_CENTER,
            label,
            app.theme.detail_font(12.0),
            ink,
        );
    }
}

fn search_field(ui: &mut Ui, app: &mut App, rect: Rect) {
    let id = egui::Id::new("mailcraft_search_box");
    let focused = ui.ctx().memory(|m| m.has_focus(id));
    if !focused {
        app.ui.search_text.clone_from(&app.s.view.search);
    }
    let th = app.theme.clone();
    let fill = if th.terminal_style {
        if th.dark { egui::Color32::from_rgb(22, 22, 32) } else { th.reading }
    } else if th.dark {
        th.chrome
    } else {
        egui::Color32::from_rgb(246, 246, 248)
    };
    ui.painter().rect(
        rect,
        if th.terminal_style { 3 } else { 8 },
        fill,
        Stroke::new(
            if focused { 1.0 } else { 0.5 },
            if focused { th.accent.gamma_multiply(0.65) } else { th.separator },
        ),
        egui::StrokeKind::Inside,
    );
    let mut c = child(ui, rect.shrink2(vec2(8.0, 3.0)), "mac-search", Layout::left_to_right(Align::Center));
    let (icon, _) = c.allocate_exact_size(vec2(14.0, 14.0), Sense::hover());
    app.icons.paint(&c, icon, "search", th.text_muted);
    let clear = !app.ui.search_text.is_empty();
    let edit = c.add(
        egui::TextEdit::singleline(&mut app.ui.search_text)
            .id(id)
            .font(th.detail_font(14.0))
            .hint_text(RichText::new("Search mail").font(th.detail_font(14.0)).color(th.text_muted))
            .font(th.detail_font(14.0))
            .frame(egui::Frame::NONE)
            .desired_width((c.available_width() - if clear { 28.0 } else { 0.0 }).max(20.0))
            .vertical_align(Align::Center),
    );
    if with_shell(ui.ctx(), |s| std::mem::take(&mut s.focus_search)) {
        edit.request_focus();
    }
    app.ui.search_active = edit.has_focus();
    if edit.changed() {
        app.run("search.query", json!({"text": app.ui.search_text.clone()}));
    }
    let escape = edit.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Escape));
    let cleared =
        clear && widgets::icon_button(&mut c, app, "close", "", true).on_hover_text("Clear search").clicked();
    if escape || cleared {
        app.ui.search_text.clear();
        app.run("search.query", json!({"text": ""}));
        if escape {
            edit.surrender_focus();
        }
    }
}

fn sidebar_heading(ui: &mut Ui, app: &App, title: &str) {
    ui.add_space(if ui.clip_rect().height() < 650.0 { 10.0 } else { 18.0 });
    ui.horizontal(|ui| {
        ui.add_space(20.0);
        ui.label(
            RichText::new(if app.theme.terminal_style { title.to_uppercase() } else { title.to_owned() })
                .font(app.theme.detail_font(12.0))
                .color(app.theme.text_muted),
        );
    });
    ui.add_space(8.0);
}

fn sidebar_entry(
    ui: &mut Ui,
    app: &mut App,
    key: &str,
    label: &str,
    icon: &str,
    badge: &str,
    selected: bool,
) -> egui::Response {
    let compact = ui.clip_rect().height() < 650.0;
    let (rect, r) =
        ui.allocate_exact_size(vec2(ui.available_width(), if compact { 34.0 } else { 42.0 }), Sense::click());
    let paint = rect.shrink2(vec2(12.0, 0.0));
    if selected || r.hovered() {
        ui.painter().rect_filled(
            paint,
            7,
            if selected { app.theme.selected_folder } else { app.theme.hover },
        );
    }
    if selected && app.theme.terminal_style {
        ui.painter().rect_filled(
            Rect::from_min_size(paint.min, vec2(2.0, paint.height())),
            0,
            app.theme.importance_low,
        );
    }
    app.icons.paint(
        ui,
        Rect::from_center_size(pos2(rect.left() + 30.0, rect.center().y), vec2(18.0, 18.0)),
        icon,
        app.theme.accent,
    );
    let text = ui.painter().layout_no_wrap(
        label.to_owned(),
        egui::FontId::proportional(if compact { 14.0 } else { 16.0 }),
        app.theme.text,
    );
    ui.painter()
        .with_clip_rect(Rect::from_min_max(
            pos2(rect.left() + 48.0, rect.top()),
            pos2(rect.right() - 42.0, rect.bottom()),
        ))
        .galley(pos2(rect.left() + 48.0, rect.center().y - text.size().y / 2.0), text, app.theme.text);
    ui.painter().text(
        pos2(rect.right() - 24.0, rect.center().y),
        egui::Align2::RIGHT_CENTER,
        badge,
        egui::FontId::proportional(12.0),
        app.theme.text_muted,
    );
    r.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, true, selected, key));
    r
}

pub fn sidebar(ui: &mut Ui, app: &mut App) {
    let full = ui.available_rect_before_wrap();
    let body = Rect::from_min_max(full.min, pos2(full.right(), (full.bottom() - 52.0).max(full.top())));
    let mut content = child(ui, body, "mac-mailboxes", Layout::top_down(Align::Min));
    let accounts = app.s.mailbox.accounts.clone();
    let rows = queries::folder_rows(&app.s);
    egui::ScrollArea::vertical()
        .id_salt("mac-sidebar")
        .auto_shrink([false, false])
        .max_height(body.height())
        .show(&mut content, |ui| {
            ui.spacing_mut().item_spacing.y = 3.0;
            sidebar_heading(ui, app, "Favorites");
            let inboxes = queries::inbox_items(&app.s);
            let unread = inboxes.iter().filter(|m| !m.read).count();
            let flagged = inboxes
                .iter()
                .filter(|m| matches!(m.flag, mailcraft_model::FlagState::Flagged { .. }))
                .count();
            let filter = app.s.mailbox.views.get(&app.s.selection.folder).cloned().unwrap_or_default().filter;
            let key = format!("All Inboxes: {unread} unread messages");
            if sidebar_entry(
                ui,
                app,
                &key,
                "All Inboxes",
                "inbox",
                &unread.to_string(),
                app.s.view.unified_inbox
                    && !app.s.view.unified_sent
                    && filter != mailcraft_model::ListFilter::Flagged
                    && !app.s.view.search.contains("hasattachments:"),
            )
            .clicked()
            {
                app.run("nav.unifiedInbox", json!({}));
                app.run("view.filter", json!({"kind":"all"}));
            }
            if sidebar_entry(
                ui,
                app,
                "Flagged",
                "Flagged",
                "flag",
                &flagged.to_string(),
                app.s.view.unified_inbox
                    && !app.s.view.unified_sent
                    && filter == mailcraft_model::ListFilter::Flagged,
            )
            .clicked()
            {
                app.run("nav.unifiedInbox", json!({}));
                if filter != mailcraft_model::ListFilter::Flagged {
                    app.run("view.filter", json!({"kind":"flagged"}));
                }
            }
            if sidebar_entry(
                ui,
                app,
                "All Sent",
                "All Sent",
                "sent",
                "",
                app.s.view.unified_inbox && app.s.view.unified_sent,
            )
            .clicked()
            {
                app.run("nav.unifiedSent", json!({}));
                app.run("view.filter", json!({"kind":"all"}));
            }
            if let Some(account) = app.s.mailbox.default_account().map(|a| a.id)
                && let Some(folder) = app.s.mailbox.role_folder(account, mailcraft_model::FolderRole::Drafts)
            {
                let count = app.s.mailbox.mail.values().filter(|m| m.folder == folder).count();
                let badge = if count > 0 { count.to_string() } else { String::new() };
                if sidebar_entry(
                    ui,
                    app,
                    "Drafts",
                    "Drafts",
                    "drafts",
                    &badge,
                    !app.s.view.unified_inbox && app.s.selection.folder == folder,
                )
                .clicked()
                {
                    app.run("nav.selectFolder", json!({"folder":folder.0}));
                }
            }
            sidebar_heading(ui, app, "Mailboxes");
            for a in &accounts {
                let linked = a.linked_mailbox.as_ref();
                let delegated = linked.is_some_and(|l| l.delegated);
                let state = egui::Id::new(("mac-account-expanded", a.id.0));
                let compact = body.height() < 650.0;
                let expanded = ui.ctx().data(|d| d.get_temp::<bool>(state)).unwrap_or(!compact && !delegated);
                let (rect, _) = ui.allocate_exact_size(
                    vec2(ui.available_width(), if compact { 44.0 } else { 55.0 }),
                    Sense::hover(),
                );
                let chevron =
                    Rect::from_min_size(pos2(rect.left() + 18.0, rect.top() + 8.0), vec2(18.0, 22.0));
                let toggle = ui.interact(chevron, ui.id().with(("account-expand", a.id.0)), Sense::click());
                app.icons.paint(
                    ui,
                    Rect::from_center_size(chevron.center(), vec2(12.0, 12.0)),
                    if expanded { "chevron-down" } else { "chevron-right" },
                    app.theme.text_muted,
                );
                toggle.widget_info(|| {
                    egui::WidgetInfo::labeled(
                        egui::WidgetType::Button,
                        true,
                        format!(
                            "{} {} mailbox",
                            if expanded { "Collapse" } else { "Expand" },
                            a.display_name
                        ),
                    )
                });
                if toggle.clicked() {
                    ui.ctx().data_mut(|d| d.insert_temp(state, !expanded));
                }
                let title =
                    if linked.is_none() && a.is_default { "Personal" } else { a.display_name.as_str() };
                let subtitle = linked
                    .map(|l| {
                        if l.delegated {
                            if l.send_as { "Delegated mailbox" } else { "Delegated · Receive only" }
                        } else {
                            "Shared mailbox"
                        }
                    })
                    .unwrap_or(&a.address);
                ui.painter().text(
                    pos2(rect.left() + 38.0, rect.top() + 9.0),
                    egui::Align2::LEFT_TOP,
                    title,
                    egui::FontId::proportional(16.0),
                    app.theme.text,
                );
                ui.painter().text(
                    pos2(rect.left() + 26.0, rect.top() + if compact { 27.0 } else { 32.0 }),
                    egui::Align2::LEFT_TOP,
                    subtitle,
                    egui::FontId::proportional(12.0),
                    app.theme.text_muted,
                );
                let select = ui.interact(
                    Rect::from_min_max(pos2(chevron.right(), rect.top()), rect.max),
                    ui.id().with(("account-select", a.id.0)),
                    Sense::click(),
                );
                let kind = if delegated { "Delegated" } else { "Shared" };
                select.widget_info(|| {
                    egui::WidgetInfo::labeled(
                        egui::WidgetType::Button,
                        true,
                        if linked.is_some() {
                            format!("{} · {kind}", a.display_name)
                        } else {
                            format!("{} mailbox", a.address)
                        },
                    )
                });
                if select.clicked()
                    && let Some(inbox) = app.s.mailbox.role_folder(a.id, mailcraft_model::FolderRole::Inbox)
                {
                    app.run("nav.selectFolder", json!({"folder":inbox.0}));
                    ui.ctx().data_mut(|d| d.insert_temp(state, true));
                }
                if expanded {
                    if let Some(inbox) = app.s.mailbox.role_folder(a.id, mailcraft_model::FolderRole::Inbox) {
                        let count =
                            app.s.mailbox.mail.values().filter(|m| m.folder == inbox && !m.read).count();
                        if sidebar_entry(
                            ui,
                            app,
                            &format!("{} Inbox", a.display_name),
                            "Inbox",
                            "inbox",
                            &count.to_string(),
                            !app.s.view.unified_inbox && app.s.selection.folder == inbox,
                        )
                        .clicked()
                        {
                            app.run("nav.selectFolder", json!({"folder":inbox.0}));
                        }
                    }
                    ui.horizontal(|ui| {
                        ui.add_space(38.0);
                        ui.menu_button(
                            RichText::new("More folders").size(11.0).color(app.theme.text_muted),
                            |ui| {
                                for row in &rows {
                                    if row.kind == FolderRowKind::Folder
                                        && let Some(folder) = row.id
                                        && app.s.mailbox.folder_account(folder) == Some(a.id)
                                    {
                                        let r = ui.button(&row.name);
                                        if r.clicked() {
                                            app.run("nav.selectFolder", json!({"folder":folder.0}));
                                            ui.close();
                                        }
                                        r.context_menu(|ui| crate::menus::folder_menu(ui, app, folder));
                                    }
                                }
                                ui.separator();
                                if ui.button("All folders…").clicked() {
                                    app.run("nav.goToFolder", json!({}));
                                    ui.close();
                                }
                            },
                        );
                    });
                }
                ui.add_space(8.0);
            }
            sidebar_heading(ui, app, "Smart folders");
            if sidebar_entry(
                ui,
                app,
                "Attachments",
                "Attachments",
                "attach",
                "",
                app.s.view.search.contains("hasattachments:"),
            )
            .clicked()
            {
                app.run("nav.unifiedInbox", json!({}));
                app.run("view.filter", json!({"kind":"all"}));
                app.run("view.filter", json!({"kind":"hasAttachments"}));
            }
            let smart: Vec<_> = rows
                .iter()
                .filter(|r| {
                    r.kind == FolderRowKind::Favorite
                        && r.id.is_some_and(|id| app.s.mailbox.folder(id).is_some_and(|f| f.role.is_none()))
                })
                .cloned()
                .collect();
            for row in &smart {
                if let Some(folder) = row.id
                    && sidebar_entry(
                        ui,
                        app,
                        &row.accessible,
                        &row.name,
                        "folder",
                        &row.badge,
                        row.selected && !app.s.view.unified_inbox,
                    )
                    .clicked()
                {
                    app.run("nav.selectFolder", json!({"folder":folder.0}));
                }
            }
        });
    let footer = Rect::from_min_max(
        pos2(full.left() + 14.0, (full.bottom() - 42.0).max(full.top())),
        pos2(full.right() - 12.0, full.bottom() - 8.0),
    );
    let mut footer = child(ui, footer, "mac-manage-accounts", Layout::left_to_right(Align::Center));
    if widgets::icon_button(&mut footer, app, "settings", "Manage accounts…", true).clicked() {
        app.run("file.accountSettings", json!({}));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::{Harness, kittest::Queryable};
    #[test]
    fn mac_shell_keeps_mail_commands_and_hides_ribbon() {
        let mut app = App::new(mailcraft_engine::sample::sample_session());
        app.enable_mac_style();
        let mut h = Harness::builder()
            .with_size(vec2(1280.0, 800.0))
            .build_ui_state(|ui, app: &mut App| app.ui_root(ui), app);
        h.run();
        assert!(h.query_by_label("New message").is_some());
        assert!(h.query_by_label("Manage accounts…").is_some());
        assert!(!with_shell(&h.ctx, |s| s.layout.ribbon_shown));
        assert!(with_shell(&h.ctx, |s| s.layout.rect("reading").is_some()));
    }
    #[test]
    fn reference_style_keeps_reader_on_right_and_mailboxes_collapsible() {
        let mut app = App::new(mailcraft_engine::sample::sample_session());
        app.enable_mac_style();
        let mut h = Harness::builder()
            .with_size(vec2(1280.0, 1000.0))
            .build_ui_state(|ui, app: &mut App| app.ui_root(ui), app);
        h.run();
        let (reader, folders, list) = with_shell(&h.ctx, |s| {
            (s.layout.rect("reading"), s.layout.rect("folders"), s.layout.rect("list"))
        });
        let reader = reader.unwrap();
        let folders = folders.unwrap();
        let list = list.unwrap();
        assert!(list.left() >= folders.right());
        assert!(list.right() <= reader.left());
        assert_eq!(reader.top(), list.top());
        assert_eq!(reader.bottom(), list.bottom());
        let width = folders.width();
        h.get_by_label("Hide mailboxes").click();
        h.run();
        assert_eq!(h.state().s.view.folder_pane, mailcraft_model::FolderPaneState::Off);
        let (list, reader) =
            with_shell(&h.ctx, |s| (s.layout.rect("list").unwrap(), s.layout.rect("reading").unwrap()));
        assert!(list.left() < 1.0);
        assert!(list.right() <= reader.left());
        h.get_by_label("Show mailboxes").click();
        h.run();
        assert_eq!(h.state().s.view.folder_pane, mailcraft_model::FolderPaneState::Normal);
        assert_eq!(with_shell(&h.ctx, |s| s.layout.rect("folders").unwrap().width()), width);

        h.get_by_label("Flagged").click();
        h.run();
        assert!(h.state().s.view.unified_inbox);
        let filter =
            h.state().s.mailbox.views.get(&h.state().s.selection.folder).cloned().unwrap_or_default().filter;
        assert_eq!(filter, mailcraft_model::ListFilter::Flagged);
        h.get_by_label("Attachments").scroll_to_me();
        h.run();
        h.get_by_label("Attachments").click();
        h.run();
        assert!(h.state().s.view.search.contains("hasattachments:yes"));
        h.get_by_label("More").click();
        h.run();
        h.get_by_label_contains("View & connection").click();
        h.run();
        h.get_by_label_contains("Reading layout").click();
        h.run();
        h.get_by_label("Below messages").click();
        h.run();
        assert_eq!(h.state().reading_pane_pos(), mailcraft_model::ReadingPanePos::Bottom);
    }

    #[test]
    fn modules_remain_visible_and_calendar_creation_opens_an_unsaved_form() {
        for (width, dark) in [(1280.0, false), (400.0, true)] {
            let mut app = App::new(mailcraft_engine::sample::sample_session());
            app.enable_mac_style();
            if dark {
                app.run("options.set", json!({"key":"view.darkMode","value":true}));
            }
            let mut h = Harness::builder()
                .with_size(vec2(width, 900.0))
                .build_ui_state(|ui, app: &mut App| app.ui_root(ui), app);
            h.run();
            for (label, module, create) in [
                ("Calendar", Module::Calendar, "New event"),
                ("Contacts", Module::People, "New contact"),
                ("Tasks", Module::Tasks, "New task"),
                ("Notes", Module::Notes, "New note"),
            ] {
                h.get_by_role_and_label(egui::accesskit::Role::Button, label).click();
                h.run();
                assert_eq!(h.state().s.view.module, module);
                assert!(h.query_by_label(create).is_some());
                assert!(h.query_by_label("New message").is_none());
                if module == Module::Calendar && width > 700.0 {
                    assert!(h.query_by_label("Today").is_some());
                    let count = h.state().s.mailbox.appointments.len();
                    h.get_by_label("New event").click();
                    h.run();
                    assert!(h.query_by_label("Save & Close").is_some());
                    assert_eq!(h.state().s.mailbox.appointments.len(), count);
                    h.get_by_label("Cancel").click();
                    h.run();
                    assert_eq!(h.state().s.mailbox.appointments.len(), count);
                }
            }
            h.get_by_role_and_label(egui::accesskit::Role::Button, "Mail").click();
            h.run();
            assert_eq!(h.state().s.view.module, Module::Mail);
            assert!(h.query_by_label("New message").is_some());
        }
    }

    #[test]
    fn shared_mailbox_navigation_uses_its_own_folder() {
        let now = mailcraft_engine::sample::sample_now();
        let mut app =
            App::new(mailcraft_engine::Session::new(mailcraft_engine::sample::mac_sample_mailbox(now)));
        app.enable_mac_style();
        let expected =
            app.s.mailbox.accounts.iter().find(|a| a.address == "design@example.net").map(|a| a.id).unwrap();
        let mut h = Harness::builder()
            .with_size(vec2(1280.0, 900.0))
            .build_ui_state(|ui, app: &mut App| app.ui_root(ui), app);
        h.run();
        h.get_by_label("Design team · Shared").scroll_to_me();
        h.run();
        h.get_by_label("Design team · Shared").click();
        h.run();
        assert_eq!(h.state().s.current_account(), Some(expected));
    }
}
