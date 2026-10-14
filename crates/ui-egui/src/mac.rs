//! Integrated desktop shell. All mail operations dispatch through the existing command registry.
use crate::{App, responsive::Frame, widgets, with_shell};
use egui::{Align, Layout, Rect, RichText, Sense, Stroke, Ui, UiBuilder, pos2, vec2};
use mailcraft_engine::queries::{self, FolderRowKind};
use serde_json::json;

impl App {
    pub fn enable_mac_style(&mut self) {
        self.mac_style = true;
        self.run("nav.unifiedInbox", json!({}));
        self.theme = crate::theme::Theme::mac(self.s.view.dark);
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
    let toolbar = Rect::from_min_max(rect.min, pos2(rect.right(), (rect.top() + 58.0).min(rect.bottom())));
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
        toolbar.min + vec2(if ui.ctx().os() == egui::os::OperatingSystem::Mac { 82.0 } else { 14.0 }, 0.0),
        pos2(
            (toolbar.left() + if frame.width < 700.0 { 180.0 } else { 230.0 }).min(toolbar.right()),
            toolbar.bottom(),
        ),
    );
    let mut c = child(ui, left, "mac-title", Layout::left_to_right(Align::Center));
    action(&mut c, app, "sync", "Get new mail", "sendReceive.all", true);
    c.add_space(8.0);
    c.label(RichText::new("MailCraft").size(16.0).strong());
    let tools = Rect::from_min_max(pos2(left.right(), toolbar.top()), toolbar.max);
    let mut c = child(ui, tools.shrink2(vec2(12.0, 0.0)), "mac-tools", Layout::left_to_right(Align::Center));
    action(&mut c, app, "new-mail", "New message", "mail.newEmail", true);
    c.separator();
    let selected = !app.s.selection.items.is_empty() || app.s.selection.focus.is_some();
    action(&mut c, app, "archive", "Archive", "mail.archive", selected);
    action(&mut c, app, "deleted", "Move to Trash", "mail.delete", selected);
    if frame.width >= 900.0 {
        c.separator();
        action(&mut c, app, "reply", "Reply", "mail.reply", selected);
        action(&mut c, app, "reply-all", "Reply to all", "mail.replyAll", selected);
        action(&mut c, app, "forward", "Forward", "mail.forward", selected);
        action(&mut c, app, "flag", "Flag", "mail.flag", selected);
    }
    c.menu_button("More", |ui| {
        for (label, cmd) in [
            ("Reply", "mail.reply"),
            ("Reply all", "mail.replyAll"),
            ("Forward", "mail.forward"),
            ("Flag", "mail.flag"),
        ] {
            if ui.add_enabled(selected, egui::Button::new(label)).clicked() {
                app.run(cmd, json!({}));
                ui.close();
            }
        }
        ui.separator();
        for (label, cmd) in [
            ("Accounts…", "file.accountSettings"),
            ("Mail settings…", "file.options"),
            ("Rules…", "rules.manage"),
        ] {
            if ui.button(label).clicked() {
                app.run(cmd, json!({}));
                ui.close();
            }
        }
        if ui.button("Add shared or delegated mailbox…").clicked() {
            app.ui.dialogs.push(("addSharedMailbox".into(), json!({})));
            ui.close();
        }
        ui.separator();
        for (label, cmd) in [
            ("Mail", "nav.mail"),
            ("Calendar", "nav.calendar"),
            ("People", "nav.people"),
            ("Tasks", "nav.tasks"),
            ("Notes", "nav.notes"),
        ] {
            if ui.button(label).clicked() {
                app.run(cmd, json!({}));
                ui.close();
            }
        }
    });
    if c.available_width() > 125.0 {
        let w = c.available_width().min(240.0);
        let r =
            Rect::from_center_size(pos2(c.max_rect().right() - w / 2.0, toolbar.center().y), vec2(w, 29.0));
        search_field(&mut c, app, r);
    }

    let content = Rect::from_min_max(pos2(rect.left(), toolbar.bottom()), rect.max);
    if content.is_positive() {
        let mut c = child(ui, content, "mac-content", Layout::top_down(Align::Min));
        crate::chrome::content_area(&mut c, app, frame);
    }
}

fn search_field(ui: &mut Ui, app: &mut App, rect: Rect) {
    let id = egui::Id::new("mailcraft_search_box");
    let focused = ui.ctx().memory(|m| m.has_focus(id));
    if !focused {
        app.ui.search_text.clone_from(&app.s.view.search);
    }
    let th = app.theme.clone();
    let fill =
        if th.dark { egui::Color32::from_rgb(48, 48, 51) } else { egui::Color32::from_rgb(232, 232, 235) };
    ui.painter().rect(
        rect,
        7,
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
            .hint_text(RichText::new("Search").size(12.0).color(th.text_muted))
            .font(egui::FontId::proportional(12.0))
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

pub fn sidebar(ui: &mut Ui, app: &mut App) {
    let rows = queries::folder_rows(&app.s);
    let mut sections: Vec<Vec<queries::FolderRow>> = Vec::new();
    for row in rows {
        if matches!(row.kind, FolderRowKind::FavoritesHeader | FolderRowKind::AccountRoot) {
            sections.push(Vec::new());
        }
        if let Some(section) = sections.last_mut() {
            section.push(row);
        }
    }
    sections.sort_by_key(|section| match section.first() {
        Some(row) if row.kind == FolderRowKind::FavoritesHeader => 0,
        Some(row)
            if app.s.mailbox.accounts.iter().any(|a| a.address == row.name && a.linked_mailbox.is_some()) =>
        {
            1
        }
        _ => 2,
    });
    let mut skip = false;
    let mut shown_unified = false;
    egui::ScrollArea::vertical().id_salt("mac-sidebar").auto_shrink([false, false]).show(ui, |ui| {
        ui.add_space(12.0);
        ui.spacing_mut().item_spacing.y = 3.0;
        for mut row in sections.into_iter().flatten() {
            if matches!(row.kind, FolderRowKind::FavoritesHeader | FolderRowKind::AccountRoot) {
                let linked_account = app
                    .s
                    .mailbox
                    .accounts
                    .iter()
                    .find(|a| a.address == row.name && a.linked_mailbox.is_some())
                    .cloned();
                skip = linked_account
                    .as_ref()
                    .is_some_and(|a| app.s.view.unified_inbox || app.s.current_account() != Some(a.id));
                if let Some(a) = &linked_account {
                    ui.add_space(8.0);
                    let (rect, response) =
                        ui.allocate_exact_size(vec2(ui.available_width(), 44.0), Sense::click());
                    if !skip || response.hovered() {
                        ui.painter().rect_filled(
                            rect.shrink2(vec2(8.0, 0.0)),
                            6,
                            if skip { app.theme.hover } else { app.theme.selected_folder },
                        );
                    }
                    app.icons.paint(
                        ui,
                        Rect::from_center_size(pos2(rect.left() + 28.0, rect.center().y), vec2(20.0, 20.0)),
                        "people",
                        app.theme.text_muted,
                    );
                    let kind = if a.linked_mailbox.as_ref().is_some_and(|l| l.delegated) {
                        "Delegated"
                    } else {
                        "Shared"
                    };
                    ui.painter().text(
                        pos2(rect.left() + 48.0, rect.top() + 13.0),
                        egui::Align2::LEFT_CENTER,
                        &a.display_name,
                        egui::FontId::proportional(13.0),
                        app.theme.text,
                    );
                    ui.painter().text(
                        pos2(rect.left() + 48.0, rect.top() + 30.0),
                        egui::Align2::LEFT_CENTER,
                        format!("{kind} mailbox"),
                        egui::FontId::proportional(11.0),
                        app.theme.text_muted,
                    );
                    response.widget_info(|| {
                        egui::WidgetInfo::labeled(
                            egui::WidgetType::Button,
                            true,
                            format!("{} · {kind}", a.display_name),
                        )
                    });
                    if response.clicked()
                        && let Some(inbox) =
                            app.s.mailbox.role_folder(a.id, mailcraft_model::FolderRole::Inbox)
                    {
                        app.run("nav.selectFolder", json!({"folder": inbox.0}));
                    }
                } else {
                    ui.add_space(12.0);
                    ui.horizontal(|ui| {
                        ui.add_space(14.0);
                        ui.label(RichText::new(&row.name).size(11.0).strong().color(app.theme.text_muted));
                    });
                }
                continue;
            }
            if skip {
                continue;
            }
            let Some(id) = row.id else {
                continue;
            };
            let unified_row = row.kind == FolderRowKind::Favorite
                && app
                    .s
                    .mailbox
                    .folder(id)
                    .is_some_and(|f| f.role == Some(mailcraft_model::FolderRole::Inbox));
            if unified_row {
                if shown_unified {
                    continue;
                }
                shown_unified = true;
                row.name = "All Inboxes".into();
                let unread = queries::inbox_items(&app.s).iter().filter(|m| !m.read).count();
                row.badge = if unread == 0 { String::new() } else { unread.to_string() };
                row.accessible = format!("All Inboxes: {unread} unread messages");
                row.selected = app.s.view.unified_inbox;
            } else {
                row.selected &= !app.s.view.unified_inbox && row.kind != FolderRowKind::Favorite;
            }
            let (rect, r) = ui.allocate_exact_size(vec2(ui.available_width(), 31.0), Sense::click());
            let paint = rect.shrink2(vec2(8.0, 0.0));
            if row.selected || r.hovered() {
                ui.painter().rect_filled(
                    paint,
                    6,
                    if row.selected { app.theme.selected_folder } else { app.theme.hover },
                );
            }
            let icon = match row.name.as_str() {
                "Inbox" | "All Inboxes" => "inbox",
                "Drafts" => "drafts",
                "Sent Items" => "sent",
                "Deleted Items" => "deleted",
                "Archive" => "archive-folder",
                "Junk Email" => "junk-folder",
                _ => "folder",
            };
            let x = paint.left() + 12.0 + row.depth.saturating_sub(1).min(6) as f32 * 12.0;
            app.icons.paint(
                ui,
                Rect::from_center_size(pos2(x + 8.0, paint.center().y), vec2(17.0, 17.0)),
                icon,
                if row.selected { app.theme.text } else { app.theme.text_muted },
            );
            let name = if row.name == "Deleted Items" {
                "Trash"
            } else if row.name == "Sent Items" {
                "Sent"
            } else {
                &row.name
            };
            ui.painter().text(
                pos2(x + 25.0, paint.center().y),
                egui::Align2::LEFT_CENTER,
                name,
                egui::FontId::proportional(13.0),
                app.theme.text,
            );
            ui.painter().text(
                pos2(paint.right() - 10.0, paint.center().y),
                egui::Align2::RIGHT_CENTER,
                &row.badge,
                egui::FontId::proportional(11.0),
                app.theme.text_muted,
            );
            r.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, &row.accessible));
            if r.clicked() {
                if unified_row {
                    app.run("nav.unifiedInbox", json!({}));
                } else {
                    app.run("nav.selectFolder", json!({"folder": id.0}));
                }
            }
            if unified_row {
                r.context_menu(|ui| {
                    if ui.button("Mark all inboxes as read").clicked() {
                        app.run("mail.markAllRead", json!({}));
                        ui.close();
                    }
                });
            } else {
                r.context_menu(|ui| crate::menus::folder_menu(ui, app, id));
            }
        }
        ui.add_space(20.0);
        ui.horizontal(|ui| {
            ui.add_space(12.0);
            if ui.small_button("Manage accounts…").clicked() {
                app.run("file.accountSettings", json!({}));
            }
        });
    });
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
        h.get_by_label("Design team · Shared").click();
        h.run();
        assert_eq!(h.state().s.current_account(), Some(expected));
    }
}
