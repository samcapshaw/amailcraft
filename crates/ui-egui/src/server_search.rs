//! Server search uses the same desktop styles and keeps remote results separate from local mail.
use crate::App;
use egui::{RichText, Ui};
use mailcraft_model::AccountKind;
use serde_json::json;

pub fn entry(ui: &mut Ui, app: &mut App) {
    if app.s.view.search.trim().is_empty()
        || !app.s.mailbox.accounts.iter().any(|a| a.kind == AccountKind::Graph)
    {
        return;
    }
    ui.horizontal(|ui| {
        ui.label(RichText::new("Searching downloaded mail").small().color(app.theme.text_muted));
        if ui.button("Search Microsoft 365…").clicked() {
            app.run("search.server", json!({"action":"show"}));
        }
    });
    ui.add_space(5.0);
}

pub fn window(ctx: &egui::Context, app: &mut App) {
    if !app.s.server_search.visible {
        return;
    }
    let mut visible = true;
    let mut command = None;
    let mut open_result = None;
    let accounts: Vec<_> = app
        .s
        .mailbox
        .accounts
        .iter()
        .filter(|a| a.kind == AccountKind::Graph)
        .map(|a| (a.id, a.display_name.clone(), a.address.clone()))
        .collect();
    let frame = egui::Frame::window(&ctx.style_of(if app.theme.dark {
        egui::Theme::Dark
    } else {
        egui::Theme::Light
    }))
    .fill(app.theme.pane)
    .stroke(egui::Stroke::new(1.0, app.theme.separator))
    .corner_radius(if app.theme.terminal_style { 4 } else { 14 })
    .inner_margin(24);
    egui::Window::new("Search Microsoft 365")
        .id(egui::Id::new("mailcraft.server.search"))
        .title_bar(false)
        .collapsible(false)
        .frame(frame)
        .default_width(640.0)
        .default_pos((ctx.content_rect().center() - egui::vec2(320.0, 210.0)).max(egui::pos2(12.0,12.0)))
        .resizable(true)
        .show(ctx,|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(12.0,14.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new("Find messages beyond this computer").size(22.0).strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Close").clicked() { visible = false; }
            });
        });
        ui.label(RichText::new("This sends your query to Microsoft 365. Local search results stay unchanged.").color(app.theme.text_muted));
        let busy = app.s.server_search.busy;
        ui.add_enabled_ui(!busy,|ui| {
            ui.add_space(4.0);
            ui.add_sized(egui::vec2(ui.available_width(),36.0),egui::TextEdit::singleline(&mut app.s.server_search.query).hint_text("Words, phrases, from: or subject:").desired_width(f32::INFINITY));
            let selected = if app.s.server_search.only_folder { "Current folder".into() } else {
                app.s.server_search.target.and_then(|id|accounts.iter().find(|a|a.0==id).map(|a|a.1.clone())).unwrap_or_else(||"All Microsoft 365 mailboxes".into())
            };
            ui.horizontal(|ui| {
            ui.label(RichText::new("Search in").color(app.theme.text_muted));
            egui::ComboBox::from_id_salt("server-search-scope").width(360.0).selected_text(selected).show_ui(ui,|ui| {
                if ui.selectable_label(app.s.server_search.target.is_none(),"All Microsoft 365 mailboxes").clicked() { app.s.server_search.target=None; app.s.server_search.only_folder=false; }
                for (id,name,address) in &accounts {
                    if ui.selectable_label(app.s.server_search.target == Some(*id) && !app.s.server_search.only_folder,format!("{name} · {address}")).clicked() { app.s.server_search.target=Some(*id); app.s.server_search.only_folder=false; }
                }
                if !app.s.view.unified_inbox && let Some(account) = app.s.current_account().filter(|id|accounts.iter().any(|a|a.0==*id))
                    && ui.selectable_label(app.s.server_search.only_folder,"Current folder").clicked() {
                    app.s.server_search.target=Some(account);app.s.server_search.only_folder=true;
                }
            });
            });
            ui.label(RichText::new("Uses Microsoft 365 search syntax. Local-only filters are not applied here.").small().color(app.theme.text_muted));
            ui.horizontal(|ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center),|ui| {
            let ink = if app.theme.dark { egui::Color32::from_rgb(16,30,52) } else { egui::Color32::WHITE };
            if ui.add_enabled(!app.s.server_search.query.trim().is_empty(),egui::Button::new(RichText::new("Search server").color(ink)).min_size(egui::vec2(124.0,36.0)).fill(app.theme.accent)).clicked() {
                command=Some(json!({"query":app.s.server_search.query,"scope":if app.s.server_search.only_folder {"folder"} else if app.s.server_search.target.is_some() {"mailbox"} else {"all"},"account":app.s.server_search.target.map(|id|id.0)}));
            }
            });
            });
        });
        ui.add_space(4.0);
        ui.separator();
        if busy { ui.horizontal(|ui| {ui.spinner();ui.label("Searching Microsoft 365…");}); }
        if let Some(error) = &app.s.server_search.error { ui.colored_label(app.theme.accent,error); }
        if app.s.server_search.complete && app.s.server_search.rows.is_empty() { ui.label("No additional messages found. Messages already on this computer are omitted; use local search to find them."); }
        if !busy && !app.s.server_search.complete && app.s.server_search.rows.is_empty() {
            ui.add_space(14.0);
            ui.label(RichText::new("Choose a scope and press Search server to find additional messages.").color(app.theme.text_muted));
            ui.add_space(20.0);
        }
        if !app.s.server_search.rows.is_empty() { ui.label(RichText::new(format!("{} additional messages",app.s.server_search.rows.len())).color(app.theme.text_muted)); }
        egui::ScrollArea::vertical().id_salt("server-search-results").max_height(360.0).show(ui,|ui| {
            for (index,row) in app.s.server_search.rows.iter().enumerate() {
                ui.push_id(index,|ui| {
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            ui.label(RichText::new(if row.subject.is_empty(){"(No subject)"}else{&row.subject}).strong());
                            let account = accounts.iter().find(|a|a.0==row.account).map(|a|a.1.as_str()).unwrap_or("Mailbox");
                            ui.label(RichText::new(format!("{} · {account}",row.sender)).small().color(app.theme.text_muted));
                        });
                        if ui.button("Open").clicked() { open_result=Some(index); }
                    });
                    ui.label(RichText::new(&row.preview).color(app.theme.text_muted));
                    ui.add_space(6.0);ui.separator();
                });
            }
        });
        ui.horizontal(|ui| {
            if ui.add_enabled(!busy && app.s.server_search.has_more(),egui::Button::new("More results")).clicked() { command=Some(json!({"action":"more"})); }
            ui.label(RichText::new("Up to 1,000 additional results · open to download").small().color(app.theme.text_muted));
        });
    });
    if !visible {
        app.run("search.server", json!({"action":"cancel"}));
    } else if let Some(index) = open_result {
        app.run("search.openServerResult", json!({"index":index}));
    } else if let Some(params) = command {
        app.run("search.server", params);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::{Harness, kittest::Queryable};

    #[test]
    fn server_search_is_discoverable_in_all_desktop_profiles() {
        for profile in ["apple", "outlook", "omarchy"] {
            let mut app = App::new(mailcraft_engine::sample::sample_session());
            app.enable_desktop_themes();
            app.s.view.work_offline = true;
            app.run("options.set", json!({"key":"general.interface","value":profile}));
            app.run("accounts.add",json!({"kind":"graph","address":"team@example.net","incoming":{"host":"https://graph.microsoft.com/v1.0"}}));
            app.run("search.query", json!({"text":"budget"}));
            let mut h = Harness::builder()
                .with_size(egui::vec2(1280.0, 900.0))
                .build_ui_state(|ui, app: &mut App| app.ui_root(ui), app);
            h.run();
            h.get_by_label("Search Microsoft 365…").click();
            h.run();
            assert!(h.state().s.server_search.visible, "{profile}");
            assert!(h.query_by_label("Search server").is_some(), "{profile}");
            assert!(h.query_by_label("More results").is_some(), "{profile}");
            assert!(h.query_by_label("Find messages beyond this computer").is_some(), "{profile}");
            let footer = h.get_by_label("More results").rect();
            assert!(
                footer.bottom() < 876.0 && footer.top() > 24.0,
                "Search controls must fit the window: {profile}, {footer:?}"
            );
            h.get_by_label("Close").click();
            h.run();
            assert!(!h.state().s.server_search.visible, "{profile}");
        }
    }
}
