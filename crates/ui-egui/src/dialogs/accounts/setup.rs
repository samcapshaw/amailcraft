//! Shared account setup presentation using the application's own theme tokens.
use super::*;
use egui::{Align2, FontId, Sense, Stroke, StrokeKind, pos2, vec2};

pub(super) fn polished(app: &App) -> bool {
    app.mac_style || app.theme_chooser_enabled
}

pub(super) fn header(ui: &mut Ui, app: &mut App, title: &str, subtitle: &str) {
    ui.add_space(10.0);
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(vec2(42.0, 42.0), Sense::hover());
        ui.painter().rect_filled(
            rect,
            if app.theme.terminal_style { 4 } else { 11 },
            app.theme.selected_folder,
        );
        app.icons.paint(ui, rect.shrink(11.0), "mail", app.theme.accent);
        ui.add_space(8.0);
        ui.vertical(|ui| {
            ui.label(RichText::new(title).size(25.0));
            muted(ui, app, subtitle);
        });
    });
    ui.add_space(20.0);
}

pub(super) fn card(ui: &mut Ui, app: &App, title: &str, body: impl FnOnce(&mut Ui)) {
    if !polished(app) {
        heading(ui, app, title);
        body(ui);
        return;
    }
    egui::Frame::new()
        .fill(app.theme.card_surface())
        .stroke(Stroke::new(0.7, app.theme.separator))
        .corner_radius(if app.theme.terminal_style { 4 } else { 10 })
        .inner_margin(18)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 9.0;
            if !title.is_empty() {
                ui.label(
                    RichText::new(title)
                        .font(app.theme.detail_font(13.0))
                        .color(if app.theme.terminal_style { app.theme.draft } else { app.theme.text_muted }),
                );
                ui.add_space(3.0);
            }
            body(ui);
        });
    ui.add_space(14.0);
}

pub(super) fn field(
    ui: &mut Ui,
    label: &str,
    value: &mut String,
    hint: &str,
    password: bool,
) -> egui::Response {
    let l = ui.label(RichText::new(label).size(12.0));
    let r = ui
        .add_sized(
            [ui.available_width(), 36.0],
            egui::TextEdit::singleline(value)
                .font(FontId::proportional(15.0))
                .margin(vec2(10.0, 9.0))
                .hint_text(hint)
                .password(password),
        )
        .labelled_by(l.id);
    ui.add_space(7.0);
    r
}

pub(super) fn choice(
    ui: &mut Ui,
    app: &App,
    title: &str,
    subtitle: &str,
    selected: bool,
    width: f32,
) -> bool {
    let (r, response) = ui.allocate_exact_size(vec2(width, 78.0), Sense::click());
    response
        .widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::SelectableLabel, true, selected, title));
    let p = ui.painter();
    p.rect_filled(
        r,
        if app.theme.terminal_style { 4 } else { 9 },
        if selected { app.theme.selected_folder } else { app.theme.control_surface() },
    );
    p.rect_stroke(
        r,
        if app.theme.terminal_style { 4 } else { 9 },
        Stroke::new(0.8, if selected { app.theme.accent } else { app.theme.separator }),
        StrokeKind::Inside,
    );
    p.text(
        pos2(r.left() + 14.0, r.top() + 17.0),
        Align2::LEFT_TOP,
        title,
        FontId::proportional(15.0),
        app.theme.text,
    );
    p.text(
        pos2(r.left() + 14.0, r.top() + 44.0),
        Align2::LEFT_TOP,
        subtitle,
        FontId::proportional(12.0),
        app.theme.text_muted,
    );
    response.clicked()
}

pub(super) fn service(
    ui: &mut Ui,
    app: &App,
    label: &str,
    description: &str,
    value: &mut bool,
    enabled: bool,
) -> egui::Response {
    ui.add_enabled_ui(enabled, |ui| {
        let (row, mut response) = ui.allocate_exact_size(vec2(ui.available_width(), 42.0), Sense::click());
        if response.clicked() {
            *value = !*value;
            response.mark_changed();
        }
        response
            .widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Checkbox, enabled, *value, label));
        let p = ui.painter();
        p.text(
            pos2(row.left(), row.top() + 5.0),
            Align2::LEFT_TOP,
            label,
            FontId::proportional(15.0),
            app.theme.text,
        );
        p.text(
            pos2(row.left(), row.top() + 26.0),
            Align2::LEFT_TOP,
            description,
            FontId::proportional(12.0),
            app.theme.text_muted,
        );
        let r = egui::Rect::from_center_size(pos2(row.right() - 18.0, row.center().y), vec2(36.0, 22.0));
        p.rect_filled(r, 11, if *value { app.theme.accent } else { app.theme.separator });
        p.circle_filled(
            pos2(if *value { r.right() - 11.0 } else { r.left() + 11.0 }, r.center().y),
            8.0,
            app.theme.pane,
        );
        response
    })
    .inner
}
