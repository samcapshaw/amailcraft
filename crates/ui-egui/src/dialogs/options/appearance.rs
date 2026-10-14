//! Appearance previews use our own UI geometry and palette; no platform artwork.
use super::{OptState, current_key};
use crate::{App, theme::Theme};
use egui::{Color32, Rect, RichText, Sense, Stroke, Ui, pos2, vec2};
use serde_json::json;

fn preset(id: &str, dark: bool) -> Theme {
    match id {
        "outlook" => Theme::for_dark(dark),
        "omarchy" => Theme::omarchy(dark),
        _ => Theme::mac(dark),
    }
}

pub(super) fn chooser(ui: &mut Ui, app: &App, st: &mut OptState) {
    let selected = current_key(app, st, "general.interface").as_str().unwrap_or("apple").to_owned();
    let dark = match current_key(app, st, "general.theme").as_str() {
        Some("dark") => true,
        Some("light") => false,
        _ => app.s.view.dark,
    };
    ui.label(RichText::new("Make it yours").size(18.0).strong());
    ui.label(
        RichText::new("Choose a layout and palette. Your accounts and mail stay the same.")
            .color(app.theme.text_muted),
    );
    ui.add_space(8.0);
    let width = ((ui.available_width() - 16.0) / 3.0).max(85.0);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        for (id, title, caption) in [
            ("outlook", "Outlook layout", "Ribbon & classic panes"),
            ("apple", "Apple", "Quiet & spacious"),
            ("omarchy", "Omarchy", "Tokyo Night flourishes"),
        ] {
            let t = preset(id, dark);
            let (r, response) = ui.allocate_exact_size(vec2(width, 132.0), Sense::click());
            response.widget_info(|| {
                egui::WidgetInfo::selected(egui::WidgetType::SelectableLabel, true, selected == id, title)
            });
            let active = selected == id;
            let p = ui.painter();
            p.rect_filled(r, 9, app.theme.card_surface());
            p.rect_stroke(
                r,
                9,
                Stroke::new(
                    if active { 1.5 } else { 0.7 },
                    if active { app.theme.accent } else { app.theme.control_edge() },
                ),
                egui::StrokeKind::Inside,
            );
            let preview = Rect::from_min_max(r.min + vec2(9.0, 9.0), pos2(r.right() - 9.0, r.top() + 79.0));
            miniature(ui, preview, &t, id == "outlook");
            p.text(
                pos2(r.left() + 11.0, r.top() + 91.0),
                egui::Align2::LEFT_TOP,
                title,
                egui::FontId::proportional(13.0),
                app.theme.text,
            );
            p.text(
                pos2(r.left() + 11.0, r.top() + 111.0),
                egui::Align2::LEFT_TOP,
                caption,
                egui::FontId::proportional(10.0),
                app.theme.text_muted,
            );
            if response.on_hover_text(caption).clicked() {
                st.edits.insert("general.interface".into(), json!(id));
                st.edits.insert("general.customColors".into(), json!(false));
            }
        }
    });
    ui.add_space(10.0);
    if let Some(def) = mailcraft_engine::settings::def("general.theme") {
        super::mac_setting(ui, app, 0, st, def);
    }
    ui.add_space(8.0);
    egui::Frame::new()
        .fill(app.theme.card_surface())
        .stroke(Stroke::new(0.7, app.theme.control_edge()))
        .corner_radius(10)
        .inner_margin(14)
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            let mut custom = current_key(app, st, "general.customColors").as_bool().unwrap_or(false);
            if ui.checkbox(&mut custom, "Customize this palette").changed() {
                let has_saved_colors = mailcraft_engine::settings::PALETTE_KEYS
                    .iter()
                    .any(|key| app.s.settings.overrides().contains_key(*key) || st.edits.contains_key(*key));
                if custom && !has_saved_colors {
                    // Begin with the selected preset, rather than unrelated default colors.
                    let t = preset(&selected, dark);
                    for (key, value) in [
                        ("general.accentColor", t.accent),
                        ("general.backgroundColor", t.pane),
                        ("general.surfaceColor", t.chrome),
                        ("general.textColor", t.text),
                    ] {
                        st.edits.insert(key.into(), json!(hex(value)));
                    }
                }
                st.edits.insert("general.customColors".into(), json!(custom));
            }
            if custom {
                ui.label(
                    RichText::new("Custom colors override light and dark palettes until turned off.")
                        .small()
                        .color(app.theme.text_muted),
                );
                for (key, label) in [
                    ("general.accentColor", "Accent"),
                    ("general.backgroundColor", "Background"),
                    ("general.surfaceColor", "Surface"),
                    ("general.textColor", "Text"),
                ] {
                    let mut value = current_key(app, st, key).as_str().unwrap_or_default().to_owned();
                    ui.horizontal(|ui| {
                        ui.add_sized([100.0, 28.0], egui::Label::new(label));
                        if let Some(mut color) = crate::theme::parse_hex(&value)
                            && ui
                                .color_edit_button_srgba(&mut color)
                                .on_hover_text(format!("Choose {label} color"))
                                .changed()
                        {
                            value = hex(color);
                            st.edits.insert(key.into(), json!(value));
                            value = current_key(app, st, key).as_str().unwrap_or_default().to_owned();
                        }
                        let response = ui
                            .add(egui::TextEdit::singleline(&mut value).desired_width(110.0).char_limit(7))
                            .on_hover_text("RGB color, for example #7AA2F7");
                        response.widget_info(|| {
                            egui::WidgetInfo::labeled(
                                egui::WidgetType::TextEdit,
                                true,
                                format!("{label} color"),
                            )
                        });
                        if response.changed() {
                            st.edits.insert(key.into(), json!(value.clone()));
                        }
                        if crate::theme::parse_hex(&value).is_none() {
                            ui.colored_label(app.theme.danger, "Use #RRGGBB");
                        }
                    });
                }
                let mut settings = app.s.settings.clone();
                for (key, value) in &st.edits {
                    let _ = settings.set(key, value);
                }
                ui.add_space(8.0);
                let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 100.0), Sense::hover());
                miniature(ui, r, &Theme::for_settings(&settings, dark), selected == "outlook");
            }
        });
    ui.add_space(20.0);
}

fn hex(c: Color32) -> String {
    format!("#{:02X}{:02X}{:02X}", c.r(), c.g(), c.b())
}

fn miniature(ui: &Ui, r: Rect, t: &Theme, ribbon: bool) {
    let p = ui.painter();
    p.rect_filled(r, 5, t.pane);
    let top = Rect::from_min_size(r.min, vec2(r.width(), if ribbon { 22.0 } else { 12.0 }));
    p.rect_filled(top, 5, t.chrome);
    let sidebar =
        Rect::from_min_max(pos2(r.left(), top.bottom()), pos2(r.left() + r.width() * 0.20, r.bottom()));
    p.rect_filled(sidebar, 0, t.chrome);
    let list_x = sidebar.right() + 4.0;
    let list_w = r.width() * 0.30;
    for i in 0..3 {
        let y = top.bottom() + 5.0 + i as f32 * 14.0;
        if y + 10.0 > r.bottom() {
            break;
        }
        let row = Rect::from_min_size(pos2(list_x, y), vec2(list_w, 11.0));
        if i == 0 {
            p.rect_filled(row, 2, t.selected);
        }
        p.line_segment(
            [pos2(list_x + 4.0, y + 4.0), pos2(list_x + list_w - 5.0, y + 4.0)],
            Stroke::new(1.5, t.text_muted),
        );
        p.circle_filled(pos2(sidebar.left() + 6.0, y + 4.0), 1.5, t.accent);
    }
    let reader_x = list_x + list_w + 8.0;
    p.vline(reader_x - 4.0, top.bottom()..=r.bottom(), Stroke::new(1.0, t.separator));
    for i in 0..4 {
        let y = top.bottom() + 7.0 + i as f32 * 7.0;
        if y > r.bottom() - 3.0 {
            break;
        }
        p.line_segment(
            [pos2(reader_x, y), pos2(r.right() - 8.0, y)],
            Stroke::new(if i == 0 { 2.0 } else { 1.0 }, if i == 0 { t.text } else { t.text_muted }),
        );
    }
    p.rect_stroke(r, 5, Stroke::new(0.7, t.separator), egui::StrokeKind::Inside);
}
