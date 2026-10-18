//! Shared Notes writing tools. Formatting is stored as character ranges, not markup in the text.
use super::notes_workspace::BLOCK_TYPES;
use egui::{FontId, RichText, TextEdit, Ui};
use mailcraft_model::note::{NoteBlock, NoteBlockKind, NoteInline, remap_inline};

fn layout(
    ui: &Ui,
    text: &str,
    spans: &[NoteInline],
    font: &FontId,
    width: f32,
    ink: egui::Color32,
    accent: egui::Color32,
) -> egui::text::LayoutJob {
    let offsets: Vec<_> = text.char_indices().map(|(at, _)| at).chain(std::iter::once(text.len())).collect();
    let length = offsets.len().saturating_sub(1);
    let mut cuts = vec![0, length];
    for span in spans.iter().take(256) {
        cuts.extend([span.start.min(length), span.end.min(length)]);
    }
    cuts.sort_unstable();
    cuts.dedup();
    let mut job = egui::text::LayoutJob::default();
    job.wrap.max_width = width;
    for part in cuts.windows(2) {
        let (Some(&start), Some(&end)) = (part.first(), part.get(1)) else { continue };
        let mut format = egui::TextFormat {
            font_id: font.clone(),
            color: ink,
            line_height: Some(font.size * 1.45),
            ..Default::default()
        };
        for span in spans.iter().take(256).filter(|r| r.start <= start && r.end >= end) {
            if span.bold {
                let name =
                    egui::FontFamily::Name(crate::face_key(mailcraft_fonts::UI_FAMILY, 700, false).into());
                if ui.fonts_mut(|fonts| fonts.families().contains(&name)) {
                    format.font_id.family = name;
                } else {
                    format.extra_letter_spacing = 0.35;
                }
            }
            format.italics |= span.italic;
            if span.highlight {
                format.background = accent.gamma_multiply(0.25);
            }
            if span.link.is_some() {
                format.color = accent;
                format.underline = egui::Stroke::new(1.0, accent);
            }
        }
        if let Some(slice) = offsets.get(start).zip(offsets.get(end)).and_then(|(a, b)| text.get(*a..*b)) {
            job.append(slice, 0.0, format);
        }
    }
    job
}

pub(super) fn text(
    ui: &mut Ui,
    app: &crate::App,
    page: u64,
    block: &mut NoteBlock,
    font: FontId,
) -> egui::Response {
    let id = egui::Id::new(("note-block", page, block.id));
    let before = block.text.clone();
    let dismissed_key = id.with("slash-dismissed");
    let slash_dismissed =
        ui.ctx().data(|d| d.get_temp::<String>(dismissed_key)).as_deref() == Some(before.as_str());
    let mut slash_key = None;
    if !slash_dismissed
        && block.kind != NoteBlockKind::Code
        && before.starts_with('/')
        && !before.contains('\n')
        && ui.memory(|m| m.has_focus(id))
    {
        for key in [egui::Key::Enter, egui::Key::Escape, egui::Key::ArrowDown, egui::Key::ArrowUp] {
            if ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, key)) {
                slash_key = Some(key);
            }
        }
    }
    let spans = block.inline.clone();
    let ink = app.theme.text;
    let accent = app.theme.accent;
    let mut layouter = |ui: &Ui, buffer: &dyn egui::TextBuffer, width: f32| {
        let job = layout(ui, buffer.as_str(), &spans, &font, width, ink, accent);
        ui.fonts_mut(|fonts| fonts.layout_job(job))
    };
    let output = TextEdit::multiline(&mut block.text)
        .id(id)
        .frame(egui::Frame::NONE)
        .font(font.clone())
        .desired_width(ui.available_width())
        .desired_rows(1)
        .hint_text("Write, or type / for blocks…")
        .layouter(&mut layouter)
        .show(ui);
    let mut response = output.response.response;
    if response.changed() {
        remap_inline(&mut block.inline, &before, &block.text);
    }
    let selection_key = id.with("selection");
    let active_key = egui::Id::new("notes-inline-active");
    if response.has_focus() {
        ui.ctx().data_mut(|d| {
            d.insert_temp(active_key, id);
            if let Some(range) = output
                .cursor_range
                .map(|r| {
                    let range = r.as_sorted_char_range();
                    range.start.0..range.end.0
                })
                .filter(|r| !r.is_empty())
            {
                d.insert_temp(selection_key, range);
            } else {
                d.remove::<std::ops::Range<usize>>(selection_key);
            }
        });
    }
    let selection = ui.ctx().data(|d| {
        (d.get_temp::<egui::Id>(active_key) == Some(id))
            .then(|| d.get_temp::<std::ops::Range<usize>>(selection_key))
            .flatten()
    });
    if let Some(range) = selection.filter(|r| r.end <= block.text.chars().count()) {
        let mut chosen = None;
        egui::Frame::new()
            .fill(app.theme.card_surface())
            .stroke(egui::Stroke::new(0.5, app.theme.separator))
            .corner_radius(if app.theme.terminal_style { 2 } else { 6 })
            .inner_margin(6.0)
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    ui.label(RichText::new("Selection").small().color(app.theme.text_muted));
                    for (label, kind) in [("Bold", 0), ("Italic", 1), ("Highlight", 2)] {
                        let on = block.inline.iter().any(|span| {
                            span.start <= range.start
                                && span.end >= range.end
                                && match kind {
                                    0 => span.bold,
                                    1 => span.italic,
                                    _ => span.highlight,
                                }
                        });
                        if ui.selectable_label(on, label).clicked() {
                            chosen = Some(kind);
                        }
                    }
                    egui::containers::menu::SubMenuButton::new("Link…")
                        .config(
                            egui::containers::menu::MenuConfig::new()
                                .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside),
                        )
                        .ui(ui, |ui| {
                            let key = id.with("link-url");
                            let mut url = ui.ctx().data(|d| d.get_temp::<String>(key)).unwrap_or_default();
                            let input =
                                ui.add(TextEdit::singleline(&mut url).hint_text("https://example.net"));
                            super::common::a11y(&input, egui::WidgetType::TextEdit, "Link address");
                            let valid = url.len() <= 2048
                                && !url.chars().any(char::is_control)
                                && ["https://", "http://", "mailto:"].iter().any(|s| url.starts_with(s));
                            if ui.add_enabled(valid, egui::Button::new("Apply link")).clicked()
                                && block.inline.len() < 256
                            {
                                block.inline.push(NoteInline {
                                    start: range.start,
                                    end: range.end,
                                    link: Some(url.clone()),
                                    ..Default::default()
                                });
                                response.mark_changed();
                                ui.close();
                            }
                            ui.ctx().data_mut(|d| d.insert_temp(key, url));
                        });
                    if ui.button("Clear formatting").clicked() {
                        chosen = Some(3);
                    }
                });
            });
        if let Some(kind) = chosen {
            let was_on = block.inline.iter().any(|span| {
                span.start <= range.start
                    && span.end >= range.end
                    && match kind {
                        0 => span.bold,
                        1 => span.italic,
                        2 => span.highlight,
                        _ => false,
                    }
            });
            clear_attribute(&mut block.inline, range.clone(), kind);
            if kind != 3 && !was_on && block.inline.len() < 256 {
                block.inline.push(NoteInline {
                    start: range.start,
                    end: range.end,
                    bold: kind == 0,
                    italic: kind == 1,
                    highlight: kind == 2,
                    ..Default::default()
                });
            }
            response.mark_changed();
            response.request_focus();
            if let Some(mut state) = TextEdit::load_state(ui.ctx(), id) {
                state.cursor.set_char_range(output.cursor_range);
                state.store(ui.ctx(), id);
            }
        }
    }
    // Link opening is explicit; typing or selecting a link never launches a browser.
    if !response.has_focus() {
        let mut urls = std::collections::BTreeSet::new();
        for url in block.inline.iter().filter_map(|r| r.link.as_deref()) {
            urls.insert(url);
        }
        if !urls.is_empty() {
            ui.horizontal_wrapped(|ui| {
                for url in urls {
                    ui.hyperlink_to("Open link ↗", url);
                }
            });
        }
    }
    let query =
        block.text.strip_prefix('/').filter(|s| s.len() <= 64 && !s.contains('\n')).map(str::to_lowercase);
    if response.has_focus()
        && !slash_dismissed
        && block.kind != NoteBlockKind::Code
        && let Some(query) = query
    {
        let choices: Vec<_> = BLOCK_TYPES
            .iter()
            .filter(|(_, label, desc)| format!("{label} {desc}").to_lowercase().contains(&query))
            .collect();
        let choice_key = id.with("slash-choice");
        let mut at = ui
            .ctx()
            .data(|d| d.get_temp::<usize>(choice_key))
            .unwrap_or(0)
            .min(choices.len().saturating_sub(1));
        if slash_key == Some(egui::Key::ArrowDown) {
            at = (at + 1).min(choices.len().saturating_sub(1));
        }
        if slash_key == Some(egui::Key::ArrowUp) {
            at = at.saturating_sub(1);
        }
        let mut choose = None;
        if slash_key == Some(egui::Key::Enter) {
            choose = choices.get(at).map(|c| c.0.clone());
        }
        let dismissed = slash_key == Some(egui::Key::Escape);
        if !dismissed {
            egui::Frame::new()
                .fill(app.theme.card_surface())
                .stroke(egui::Stroke::new(1.0, app.theme.separator))
                .corner_radius(if app.theme.terminal_style { 2 } else { 8 })
                .inner_margin(12.0)
                .show(ui, |ui| {
                    ui.set_min_width(ui.available_width().min(300.0));
                    ui.label(RichText::new("Insert block").strong());
                    ui.label(RichText::new("↑ ↓ to choose · Enter to insert · Esc to cancel").small().weak());
                    egui::ScrollArea::vertical().id_salt(id.with("slash-scroll")).max_height(240.0).show(
                        ui,
                        |ui| {
                            for (index, (kind, label, description)) in choices.iter().enumerate() {
                                if ui.selectable_label(index == at, *label).clicked() {
                                    choose = Some(kind.clone());
                                }
                                ui.label(RichText::new(*description).small().color(app.theme.text_muted));
                                ui.add_space(5.0);
                            }
                            if choices.is_empty() {
                                ui.label("No matching blocks");
                            }
                        },
                    );
                });
        }
        if dismissed {
            ui.ctx().data_mut(|d| d.insert_temp(dismissed_key, block.text.clone()));
        }
        if let Some(kind) = choose {
            block.kind = kind;
            block.text.clear();
            block.inline.clear();
            if block.kind == NoteBlockKind::Table {
                block.text = "Column 1\tColumn 2\n\t".into();
            }
            response.mark_changed();
            response.request_focus();
            if let Some(mut state) = TextEdit::load_state(ui.ctx(), id) {
                state.cursor.set_char_range(Some(egui::text::CCursorRange::one(egui::text::CCursor::new(0))));
                state.store(ui.ctx(), id);
            }
        }
        ui.ctx().data_mut(|d| d.insert_temp(choice_key, at));
    }
    response
}

fn clear_attribute(spans: &mut Vec<NoteInline>, selection: std::ops::Range<usize>, kind: u8) {
    let original = spans.clone();
    let mut next = Vec::new();
    for span in spans.drain(..) {
        if span.end <= selection.start || span.start >= selection.end {
            next.push(span);
            continue;
        }
        if span.start < selection.start {
            next.push(NoteInline { end: selection.start, ..span.clone() });
        }
        if span.end > selection.end {
            next.push(NoteInline { start: selection.end, ..span.clone() });
        }
        let mut middle =
            NoteInline { start: span.start.max(selection.start), end: span.end.min(selection.end), ..span };
        if kind == 0 || kind == 3 {
            middle.bold = false;
        }
        if kind == 1 || kind == 3 {
            middle.italic = false;
        }
        if kind == 2 || kind == 3 {
            middle.highlight = false;
        }
        if kind == 3 {
            middle.link = None;
        }
        if middle.bold || middle.italic || middle.highlight || middle.link.is_some() {
            next.push(middle);
        }
    }
    *spans = if next.len() <= 256 { next } else { original };
}

/// Use a bundled bold face when installed, with a safe fallback for bare test hosts.
pub(super) fn heading_font(ui: &Ui, size: f32) -> FontId {
    let family = egui::FontFamily::Name(crate::face_key(mailcraft_fonts::UI_FAMILY, 700, false).into());
    FontId::new(
        size,
        if ui.fonts_mut(|f| f.families().contains(&family)) {
            family
        } else {
            egui::FontFamily::Proportional
        },
    )
}

/// Original tonal cover, tessellated inside its rounded outline rather than square-clipped.
pub(super) fn cover(ui: &Ui, rect: egui::Rect, app: &crate::App, tint: egui::Color32) {
    let radius = if app.theme.terminal_style { 2.0 } else { 12.0 };
    let start = egui::Rgba::from(tint.gamma_multiply(0.60));
    let end = egui::Rgba::from(app.theme.card_surface());
    let color = |point: egui::Pos2| {
        egui::Color32::from(egui::lerp(
            start..=end,
            (((point.x - rect.left()) / rect.width() + (point.y - rect.top()) / rect.height()) * 0.5)
                .clamp(0.0, 1.0),
        ))
    };
    let mut mesh = egui::Mesh::default();
    mesh.colored_vertex(rect.center(), color(rect.center()));
    for (center, angle) in [
        (egui::pos2(rect.right() - radius, rect.top() + radius), -std::f32::consts::FRAC_PI_2),
        (egui::pos2(rect.right() - radius, rect.bottom() - radius), 0.0),
        (egui::pos2(rect.left() + radius, rect.bottom() - radius), std::f32::consts::FRAC_PI_2),
        (egui::pos2(rect.left() + radius, rect.top() + radius), std::f32::consts::PI),
    ] {
        for step in 0..=8 {
            let angle = angle + (step as f32 / 8.0) * std::f32::consts::FRAC_PI_2;
            let point = center + egui::vec2(angle.cos(), angle.sin()) * radius;
            mesh.colored_vertex(point, color(point));
        }
    }
    for vertex in 1..=36 {
        mesh.add_triangle(0, vertex, if vertex == 36 { 1 } else { vertex + 1 });
    }
    ui.painter().add(egui::Shape::mesh(mesh));
}
