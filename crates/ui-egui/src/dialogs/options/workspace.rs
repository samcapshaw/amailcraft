//! Settings workspace: grouped navigation and account overview, using the existing account commands.
use super::*;
use egui::{Align, Layout, Sense, Stroke, pos2, vec2};

fn navigation_row(ui: &mut egui::Ui, app: &mut App, st: &mut OptState, page: usize, title: &str, icon: &str) {
    let query = st.filter.trim().to_lowercase();
    let schema = settings::PAGES.get(page).map(|p| p.0).unwrap_or("");
    let aliases = match page {
        101 => "appearance theme colors palette layout dark light",
        100 => "accounts mailboxes sender sending identity delegated shared sidebar",
        _ => "",
    };
    if !query.is_empty()
        && !aliases.contains(&query)
        && !title.to_lowercase().contains(&query)
        && !settings::DEFS.iter().any(|d| d.page == schema && d.label.to_lowercase().contains(&query))
    {
        return;
    }
    let (rect, response) = ui.allocate_exact_size(vec2(196.0, 36.0), Sense::click());
    let selected = st.page == page;
    if selected || response.hovered() {
        ui.painter().rect_filled(
            rect,
            if app.theme.terminal_style { 3 } else { 7 },
            if selected { app.theme.selected_folder } else { app.theme.hover },
        );
    }
    let ink = if selected { app.theme.accent } else { app.theme.text };
    app.icons.paint(
        ui,
        egui::Rect::from_center_size(pos2(rect.left() + 19.0, rect.center().y), vec2(17.0, 17.0)),
        icon,
        ink,
    );
    ui.painter().text(
        pos2(rect.left() + 39.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        title,
        egui::FontId::proportional(14.0),
        ink,
    );
    response
        .widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::SelectableLabel, true, selected, title));
    if response.clicked() {
        st.page = page;
        st.scroll_to = None;
        st.note.clear();
    }
}
pub(super) fn navigation(ui: &mut egui::Ui, app: &mut App, st: &mut OptState, height: f32) {
    egui::Frame::new()
        .fill(if app.theme.terminal_style { app.theme.backdrop } else { app.theme.chrome })
        .inner_margin(12)
        .show(ui, |ui| {
            ui.vertical(|ui| {
                ui.set_width(196.0);
                ui.set_min_height(height + 64.0);
                egui::ScrollArea::vertical().max_height(height + 64.0).show(ui, |ui| {
                    for (heading, rows) in [
                        (
                            "APPLICATION",
                            vec![
                                (0, "General", "settings"),
                                (101, "Appearance", "categorize"),
                                (100, "Accounts", "people"),
                            ],
                        ),
                        (
                            "MAIL & ORGANIZER",
                            vec![
                                (1, "Mail", "mail"),
                                (2, "Calendar", "calendar"),
                                (4, "Contacts", "people"),
                                (5, "Tasks", "tasks"),
                                (6, "Search", "search"),
                            ],
                        ),
                    ] {
                        ui.label(
                            RichText::new(heading)
                                .font(app.theme.detail_font(11.0))
                                .color(app.theme.text_muted),
                        );
                        ui.add_space(8.0);
                        for (page, title, icon) in rows {
                            navigation_row(ui, app, st, page, title, icon);
                        }
                        ui.add_space(20.0);
                    }
                    egui::CollapsingHeader::new("More settings")
                        .default_open(!st.filter.is_empty() || st.page > 6 && st.page < 100)
                        .show(ui, |ui| {
                            for (i, (_, title)) in
                                settings::PAGES.iter().enumerate().filter(|(i, _)| *i > 6 || *i == 3)
                            {
                                navigation_row(ui, app, st, i, title, "settings");
                            }
                        });
                });
            });
        });
}
pub(super) fn accounts(ui: &mut egui::Ui, app: &mut App, st: &mut OptState, height: f32) -> Out {
    let mut out = Out::Keep;
    ui.label(RichText::new("Accounts").size(28.0));
    muted(ui, app, "Your mailboxes, identities, and sending preferences.");
    ui.add_space(22.0);
    egui::ScrollArea::vertical()
        .max_height(height)
        .min_scrolled_height(height)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            section(ui, app, "Mailboxes");
            let accounts: Vec<_> = app.s.mailbox.accounts.to_vec();
            egui::Frame::new()
                .fill(app.theme.card_surface())
                .stroke(Stroke::new(1.0, app.theme.separator))
                .corner_radius(if app.theme.terminal_style { 4 } else { 9 })
                .inner_margin(14)
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    for (i, a) in accounts.iter().enumerate() {
                        if i > 0 {
                            ui.add_space(12.0);
                            ui.separator();
                            ui.add_space(12.0);
                        }
                        let name = if a.display_name.is_empty() { &a.address } else { &a.display_name };
                        let initials: String = name
                            .split_whitespace()
                            .filter_map(|s| s.chars().next())
                            .take(2)
                            .collect::<String>()
                            .to_uppercase();
                        let kind = a
                            .linked_mailbox
                            .as_ref()
                            .map(|l| if l.delegated { "Delegated" } else { "Shared" })
                            .unwrap_or("Personal");
                        let status = if a.is_default {
                            "Default sender"
                        } else if a.linked_mailbox.as_ref().is_some_and(|l| !l.send_as) {
                            "Receive only"
                        } else if a.linked_mailbox.is_some() {
                            "Sending enabled in app"
                        } else {
                            ""
                        };
                        let row = ui.horizontal(|ui| {
                            let (avatar, _) = ui.allocate_exact_size(vec2(38.0, 38.0), Sense::hover());
                            ui.painter().rect_filled(avatar, 6, app.theme.selected_folder);
                            ui.painter().text(
                                avatar.center(),
                                egui::Align2::CENTER_CENTER,
                                initials,
                                app.theme.detail_font(14.0),
                                app.theme.accent,
                            );
                            ui.add_space(10.0);
                            ui.vertical(|ui| {
                                ui.set_max_width((ui.available_width() - 150.0).max(130.0));
                                ui.add(egui::Label::new(RichText::new(name).size(16.0)).truncate());
                                ui.add(
                                    egui::Label::new(
                                        RichText::new(format!("{} · {kind}", a.address))
                                            .size(12.0)
                                            .color(app.theme.text_muted),
                                    )
                                    .truncate(),
                                );
                            });
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                muted(ui, app, status);
                            });
                        });
                        let r = ui
                            .interact(
                                row.response.rect,
                                ui.id().with(("settings-account", a.id.0)),
                                Sense::click(),
                            )
                            .on_hover_text("Open account details");
                        r.widget_info(|| {
                            egui::WidgetInfo::labeled(
                                egui::WidgetType::Button,
                                true,
                                format!("Account details: {name}"),
                            )
                        });
                        if r.clicked() {
                            out = Out::open("accountSettings", json!({"account":a.id.0}));
                        }
                    }
                    if accounts.is_empty() {
                        muted(ui, app, "Add a mailbox to get started.");
                    }
                });
            ui.add_space(14.0);
            ui.menu_button(RichText::new("+ Add mailbox").color(app.theme.accent).size(15.0), |ui| {
                if ui.button("Personal account…").clicked() {
                    out = Out::open("addAccount", json!({}));
                    ui.close();
                }
                if ui.button("Shared or delegated mailbox…").clicked() {
                    out = Out::open("addSharedMailbox", json!({}));
                    ui.close();
                }
            });
            ui.add_space(22.0);
            section(ui, app, "Defaults");
            egui::Frame::new()
                .fill(app.theme.card_surface())
                .stroke(Stroke::new(1.0, app.theme.separator))
                .corner_radius(if app.theme.terminal_style { 4 } else { 9 })
                .inner_margin(16)
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    let old = st.default_account.or_else(|| app.s.mailbox.default_account().map(|a| a.id.0));
                    let mut selected = old;
                    ui.horizontal(|ui| {
                        ui.label("Default sending account");
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            let name = accounts
                                .iter()
                                .find(|a| Some(a.id.0) == selected)
                                .map(|a| a.display_name.as_str())
                                .unwrap_or("Choose account");
                            egui::ComboBox::from_id_salt("settings-default-sender")
                                .selected_text(name)
                                .width(180.0)
                                .show_ui(ui, |ui| {
                                    for a in accounts.iter().filter(|a| a.can_send()) {
                                        ui.selectable_value(&mut selected, Some(a.id.0), &a.display_name);
                                    }
                                });
                        });
                    });
                    if selected != old {
                        st.default_account = selected;
                    }
                    ui.add_space(14.0);
                    ui.separator();
                    ui.add_space(14.0);
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            ui.label("Show account sidebar");
                            muted(ui, app, "Keep your mailboxes visible in the main window.");
                        });
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            let value = st
                                .show_sidebar
                                .unwrap_or(app.s.view.folder_pane != mailcraft_model::FolderPaneState::Off);
                            let (rect, r) = ui.allocate_exact_size(vec2(36.0, 21.0), Sense::click());
                            ui.painter().rect_filled(
                                rect,
                                11,
                                if value { app.theme.accent } else { app.theme.separator },
                            );
                            ui.painter().circle_filled(
                                pos2(
                                    if value { rect.right() - 10.5 } else { rect.left() + 10.5 },
                                    rect.center().y,
                                ),
                                8.0,
                                app.theme.backdrop,
                            );
                            r.widget_info(|| {
                                egui::WidgetInfo::selected(
                                    egui::WidgetType::Checkbox,
                                    true,
                                    value,
                                    "Show account sidebar",
                                )
                            });
                            if r.clicked() {
                                st.show_sidebar = Some(!value);
                            }
                        });
                    });
                });
        });
    out
}
fn section(ui: &mut egui::Ui, app: &mut App, title: &str) {
    ui.label(RichText::new(title).font(app.theme.detail_font(15.0)).color(if app.theme.terminal_style {
        app.theme.draft
    } else {
        app.theme.text
    }));
    ui.add_space(10.0);
}
