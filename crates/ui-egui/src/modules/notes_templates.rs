//! Visual template library; previews are drawn from local page blocks, with original project icons.
use crate::{App, widgets};
use egui::{Color32, RichText, ScrollArea, Stroke, TextEdit, vec2};
use mailcraft_engine::cmd::notes;
use mailcraft_model::{
    FolderId, ItemId, Note, NoteColor,
    note::{NoteBlock, NoteBlockKind, NotePaper},
};
use serde_json::json;

pub(super) fn tint(color: NoteColor, dark: bool) -> Color32 {
    match (color, dark) {
        (NoteColor::Blue, true) => Color32::from_rgb(122, 162, 247),
        (NoteColor::Green, true) => Color32::from_rgb(158, 206, 106),
        (NoteColor::Pink, true) => Color32::from_rgb(187, 154, 247),
        (NoteColor::Yellow, true) => Color32::from_rgb(224, 175, 104),
        (NoteColor::White, true) => Color32::from_rgb(192, 202, 245),
        (NoteColor::Blue, false) => Color32::from_rgb(55, 106, 188),
        (NoteColor::Green, false) => Color32::from_rgb(63, 120, 83),
        (NoteColor::Pink, false) => Color32::from_rgb(145, 89, 160),
        (NoteColor::Yellow, false) => Color32::from_rgb(162, 113, 32),
        (NoteColor::White, false) => Color32::from_rgb(105, 114, 132),
    }
}

pub(super) fn field_frame(app: &App) -> egui::Frame {
    egui::Frame::new()
        .fill(app.theme.control_surface())
        .stroke(Stroke::new(1.0, app.theme.control_edge()))
        .corner_radius(if app.theme.terminal_style { 2.0 } else { 6.0 })
        .inner_margin(8.0)
}

struct Card {
    title: String,
    description: String,
    group: String,
    icon: String,
    color: NoteColor,
    paper: NotePaper,
    blocks: Vec<NoteBlock>,
    key: Option<&'static str>,
    source: Option<ItemId>,
}

fn preview(ui: &mut egui::Ui, app: &mut App, card: &Card) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 72.0), egui::Sense::hover());
    let color = tint(card.color, app.theme.dark);
    ui.painter().rect_filled(rect, 6.0, app.theme.reading);
    paper(ui, rect, card.paper, color);
    ui.painter().rect_filled(egui::Rect::from_min_size(rect.min, vec2(4.0, rect.height())), 2.0, color);
    widgets::paint_icon(ui, app, rect.min + vec2(24.0, 24.0), 20.0, &card.icon, color);
    let mut y = rect.top() + 18.0;
    let x = rect.left() + 46.0;
    let width = (rect.width() - 64.0).max(10.0);
    for block in card.blocks.iter().filter(|b| !b.text.trim().is_empty()).take(5) {
        let heading = block.kind == NoteBlockKind::Heading;
        if block.kind == NoteBlockKind::Table {
            for row in 0..3 {
                for col in 0..3 {
                    let cell = egui::Rect::from_min_size(
                        egui::pos2(x + col as f32 * width / 3.0, y + row as f32 * 6.0),
                        vec2(width / 3.0 - 2.0, 4.0),
                    );
                    ui.painter().rect_filled(cell, 1.0, app.theme.text_muted.gamma_multiply(0.3));
                }
            }
            y += 23.0;
        } else {
            let inset = if block.kind == NoteBlockKind::Checklist { 10.0 } else { 0.0 };
            if inset > 0.0 {
                ui.painter().rect_stroke(
                    egui::Rect::from_min_size(egui::pos2(x, y), vec2(6.0, 6.0)),
                    1.0,
                    Stroke::new(1.0, color),
                    egui::StrokeKind::Inside,
                );
            }
            let length = if heading { 0.65 } else { 0.9 };
            ui.painter().rect_filled(
                egui::Rect::from_min_size(
                    egui::pos2(x + inset, y),
                    vec2((width - inset) * length, if heading { 4.0 } else { 3.0 }),
                ),
                1.0,
                if heading { color.gamma_multiply(0.7) } else { app.theme.text_muted.gamma_multiply(0.35) },
            );
            y += 12.0;
        }
        if y > rect.bottom() - 12.0 {
            break;
        }
    }
}

pub(super) fn show(ui: &mut egui::Ui, app: &mut App, folder: FolderId, selected: &mut Option<ItemId>) {
    ui.add_space(12.0);
    ui.label(RichText::new("Make room for your next idea").size(30.0).strong());
    ui.label(
        RichText::new("Start with a thoughtfully structured page, then make it your own.")
            .color(app.theme.text_muted),
    );
    ui.add_space(12.0);
    let key = egui::Id::new("note-template-search");
    let mut search = ui.ctx().data(|data| data.get_temp::<String>(key)).unwrap_or_default();
    let response = ui.add_sized(
        [ui.available_width(), 36.0],
        TextEdit::singleline(&mut search)
            .char_limit(256)
            .frame(field_frame(app))
            .hint_text("Find a template…"),
    );
    super::common::a11y(&response, egui::WidgetType::TextEdit, "Find a template");
    ui.ctx().data_mut(|data| data.insert_temp(key, search.clone()));
    let query = search.to_lowercase();
    let mut cards: Vec<Card> = notes::TEMPLATES
        .iter()
        .map(|t| {
            let note = Note {
                text: notes::template_text(t.key).unwrap_or_default().into(),
                markdown: true,
                ..Default::default()
            };
            let mut blocks =
                if t.key == "weekly" { notes::weekly_blocks() } else { notes::page_blocks(&note) };
            for block in &mut blocks {
                if block.text.contains('\t') {
                    block.kind = NoteBlockKind::Table;
                }
            }
            Card {
                title: t.title.into(),
                description: t.description.into(),
                group: t.group.into(),
                icon: t.icon.into(),
                color: t.color,
                paper: t.paper,
                blocks,
                key: Some(t.key),
                source: None,
            }
        })
        .collect();
    cards.extend(app.s.mailbox.notes.values().filter(|n| notes::template_available(&app.s, n.id)).map(|n| {
        Card {
            title: notes::title(n),
            description: "Your reusable layout, including its content and page styling.".into(),
            group: "Yours".into(),
            icon: if notes::PAGE_ICONS.contains(&n.icon.as_str()) { n.icon.clone() } else { "note".into() },
            color: n.color,
            paper: n.paper,
            blocks: notes::page_blocks(n),
            key: None,
            source: Some(n.id),
        }
    }));
    cards.retain(|card| {
        format!("{} {} {}", card.title, card.description, card.group).to_lowercase().contains(&query)
    });
    let mut create = None;
    let mut remove = None;
    ScrollArea::vertical().id_salt("note-template-cards").show(ui, |ui| {
        if cards.is_empty() {
            ui.label("No templates match. Try a different word.");
        }
        for group in ["Yours", "Planning", "Work", "Personal"] {
            let entries: Vec<_> = cards.iter().filter(|card| card.group == group).collect();
            if entries.is_empty() {
                continue;
            }
            ui.add_space(18.0);
            ui.label(RichText::new(group).strong().color(app.theme.text_muted));
            let columns = if ui.available_width() >= 840.0 {
                3
            } else if ui.available_width() >= 520.0 {
                2
            } else {
                1
            };
            let card_width =
                ((ui.available_width() - 16.0 * (columns - 1) as f32) / columns as f32).max(200.0);
            egui::Grid::new(("note-template-group", group)).spacing(vec2(16.0, 16.0)).show(ui, |ui| {
                for (index, card) in entries.iter().enumerate() {
                    ui.push_id((group, index), |ui| {
                        egui::Frame::new()
                            .fill(app.theme.card_surface())
                            .stroke(Stroke::new(1.0, app.theme.control_edge()))
                            .corner_radius(if app.theme.terminal_style { 2.0 } else { 10.0 })
                            .inner_margin(14.0)
                            .show(ui, |ui| {
                                ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                                    ui.spacing_mut().item_spacing.y = 6.0;
                                    ui.set_width((card_width - 28.0).max(160.0));
                                    preview(ui, app, card);
                                    ui.add_space(6.0);
                                    ui.label(RichText::new(&card.title).size(17.0).strong());
                                    ui.add_sized(
                                        [ui.available_width(), 42.0],
                                        egui::Label::new(
                                            RichText::new(&card.description)
                                                .size(13.0)
                                                .color(app.theme.text_muted),
                                        )
                                        .wrap()
                                        .halign(egui::Align::Min),
                                    );
                                    if ui
                                        .add_sized(
                                            [ui.available_width(), 32.0],
                                            egui::Button::new(format!("Use {}", card.title))
                                                .fill(app.theme.selected),
                                        )
                                        .clicked()
                                    {
                                        create = Some(if let Some(key) = card.key {
                                            json!({"folder":folder.0,"template":key})
                                        } else {
                                            json!({"folder":folder.0,"templateId":card.source.map(|id|id.0)})
                                        });
                                    }
                                    if let Some(id) = card.source
                                        && ui.small_button("Remove template").clicked()
                                    {
                                        remove = Some(id);
                                    }
                                });
                            });
                    });
                    if index.saturating_add(1) % columns == 0 {
                        ui.end_row();
                    }
                }
                ui.end_row();
            });
        }
        ui.add_space(20.0);
        ui.label(
            RichText::new("Make your own: open a page and choose Page options → Save as template.")
                .small()
                .color(app.theme.text_muted),
        );
    });
    if let Some(id) = remove {
        app.run("notes.delete", json!({"id":id.0}));
    }
    if let Some(params) = create
        && let Some(value) = app.run("notes.new", params)
        && let Some(id) = value.get("id").and_then(|v| v.as_u64())
    {
        *selected = Some(ItemId(id));
        ui.ctx().data_mut(|data| data.insert_temp(egui::Id::new("notes-template-gallery"), false));
    }
}

pub(super) fn paper(ui: &egui::Ui, rect: egui::Rect, style: NotePaper, color: Color32) {
    let rect = rect.intersect(ui.clip_rect());
    let step = 24.0;
    match style {
        NotePaper::Plain => {}
        NotePaper::Ruled => {
            for row in 0..60 {
                let y = rect.top() + row as f32 * step;
                if y > rect.bottom() {
                    break;
                }
                ui.painter().hline(rect.x_range(), y, Stroke::new(1.0, color.gamma_multiply(0.08)));
            }
        }
        NotePaper::Dotted => {
            for row in 0..60 {
                for col in 0..60 {
                    let p = rect.min + vec2(col as f32 * step, row as f32 * step);
                    if rect.contains(p) {
                        ui.painter().circle_filled(p, 1.0, color.gamma_multiply(0.15));
                    }
                }
            }
        }
    }
}
