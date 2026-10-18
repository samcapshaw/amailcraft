//! Page workspace shared by the Apple, PC and Omarchy desktop profiles.
use super::common::*;
use crate::App;
use egui::{RichText, ScrollArea, TextEdit};
use mailcraft_engine::cmd::{calendar::pim::Kind, notes};
use mailcraft_model::{
    FolderId, ItemId,
    note::{NoteBlock, NoteBlockKind},
};
use serde_json::json;

fn blocks(note: &mailcraft_model::Note) -> Vec<NoteBlock> {
    let mut blocks = notes::page_blocks(note);
    if note.blocks.is_empty() && note.markdown {
        blocks.retain(|block| block.kind != NoteBlockKind::Paragraph || !block.text.trim().is_empty());
        for block in &mut blocks {
            if block.kind != NoteBlockKind::Code {
                block.text = block.text.trim_matches('\n').into();
            }
        }
    }
    if blocks.is_empty() {
        blocks.push(NoteBlock { id: 1, ..Default::default() });
    }
    blocks
}

fn page_image(ui: &mut egui::Ui, app: &App, block: &NoteBlock) {
    let Some(key) = &block.blob else {
        ui.label("Image unavailable");
        return;
    };
    let cache = egui::Id::new(("note-image", key));
    let mut texture = ui.ctx().data(|data| data.get_temp::<egui::TextureHandle>(cache));
    if texture.is_none() {
        let decoded = app
            .s
            .store
            .as_ref()
            .and_then(|store| store.get_blob(key).ok())
            .and_then(|bytes| notes::decode_page_image(&bytes).ok());
        if let Some(image) = decoded {
            let image = image::DynamicImage::ImageRgba8(image).thumbnail(1200, 1200).to_rgba8();
            let size =
                [usize::try_from(image.width()).unwrap_or(0), usize::try_from(image.height()).unwrap_or(0)];
            let handle = ui.ctx().load_texture(
                key,
                egui::ColorImage::from_rgba_unmultiplied(size, image.as_raw()),
                egui::TextureOptions::LINEAR,
            );
            ui.ctx().data_mut(|data| data.insert_temp(cache, handle.clone()));
            texture = Some(handle);
        }
    }
    if let Some(texture) = texture {
        ui.add(egui::Image::new(&texture).max_width(ui.available_width()));
    } else {
        ui.label("Image unavailable or invalid. The page text is retained.");
    }
}

fn table(ui: &mut egui::Ui, block: &mut NoteBlock) -> bool {
    if block.text.lines().count() > 50 || block.text.lines().any(|line| line.split('\t').count() > 12) {
        ui.label("Convert this oversized table to Text to edit it.");
        return false;
    }
    let mut rows: Vec<Vec<String>> =
        block.text.lines().map(|line| line.split('\t').map(str::to_owned).collect()).collect();
    if rows.is_empty() {
        rows.push(vec![String::new(), String::new()]);
    }
    let columns = rows.iter().map(Vec::len).max().unwrap_or(2).max(2);
    let habit_grid = rows.first().is_some_and(|row| {
        row.iter().skip(1).map(String::as_str).eq(["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"])
    });
    let mut changed = false;
    ScrollArea::horizontal().id_salt(("note-table-scroll", block.id)).show(ui, |ui| {
        egui::Grid::new(("note-table", block.id)).striped(true).show(ui, |ui| {
            for (row_index, row) in rows.iter_mut().enumerate() {
                row.resize(columns, String::new());
                let row_title: String =
                    row.first().map(String::as_str).unwrap_or("Habit").chars().take(120).collect();
                for (column, cell) in row.iter_mut().enumerate() {
                    if habit_grid && row_index == 0 {
                        ui.label(RichText::new(cell.as_str()).strong());
                    } else if habit_grid && column > 0 {
                        let mut checked = cell == "[x]";
                        let response = ui.checkbox(&mut checked, "");
                        let day = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"]
                            .get(column.saturating_sub(1))
                            .copied()
                            .unwrap_or("Day");
                        let label = format!("{row_title}, {day}");
                        response.widget_info(|| {
                            egui::WidgetInfo::selected(egui::WidgetType::Checkbox, true, checked, &label)
                        });
                        if response.changed() {
                            *cell = if checked { "[x]" } else { "" }.into();
                            changed = true;
                        }
                    } else {
                        changed |= ui
                            .add(TextEdit::singleline(cell).desired_width(120.0).hint_text("Cell"))
                            .changed();
                    }
                }
                ui.end_row();
            }
        });
    });
    ui.horizontal(|ui| {
        if rows.len() < 50 && ui.small_button("+ Row").clicked() {
            rows.push(vec![String::new(); columns]);
            changed = true;
        }
        if !habit_grid && columns < 12 && ui.small_button("+ Column").clicked() {
            for row in &mut rows {
                row.push(String::new());
            }
            changed = true;
        }
    });
    if changed {
        block.text = rows.into_iter().map(|row| row.join("\t")).collect::<Vec<_>>().join("\n");
    }
    changed
}

fn page_tree(
    ui: &mut egui::Ui,
    app: &mut App,
    folder: FolderId,
    parent: Option<ItemId>,
    depth: usize,
    selected: &mut Option<ItemId>,
    gallery: &mut bool,
) {
    if depth > 12 {
        return;
    }
    let mut pages: Vec<_> = app
        .s
        .mailbox
        .notes
        .values()
        .filter(|n| {
            n.folder == folder
                && !n.is_template
                && (n.parent == parent
                    || (parent.is_none()
                        && n.parent.is_some_and(|id| {
                            app.s.mailbox.notes.get(&id).is_none_or(|p| p.folder != folder || p.is_template)
                        })))
        })
        .cloned()
        .collect();
    pages.sort_by_key(|n| (!n.pinned, notes::title(n).to_lowercase(), n.id));
    for page in pages {
        ui.push_id(page.id.0, |ui| {
            let branch = egui::Id::new(("note-page-expanded", page.id.0));
            let mut expanded = ui.ctx().data(|data| data.get_temp::<bool>(branch)).unwrap_or(true);
            let has_children = app
                .s
                .mailbox
                .notes
                .values()
                .any(|child| child.folder == folder && child.parent == Some(page.id));
            ui.horizontal(|ui| {
                ui.add_space(depth as f32 * 12.0);
                if has_children {
                    if crate::widgets::icon_only(
                        ui,
                        app,
                        if expanded { "chevron-down" } else { "chevron-right" },
                        "Show or hide subpages",
                        true,
                    )
                    .clicked()
                    {
                        expanded = !expanded;
                    }
                } else {
                    ui.add_space(20.0);
                }
                let (badge, _) = ui.allocate_exact_size(egui::vec2(18.0, 24.0), egui::Sense::hover());
                let icon =
                    if notes::PAGE_ICONS.contains(&page.icon.as_str()) { page.icon.as_str() } else { "note" };
                let tint = super::notes_templates::tint(page.color, app.theme.dark);
                crate::widgets::paint_icon(ui, app, badge.center(), 16.0, icon, tint);
                page_entry(ui, app, &page, selected, gallery);
            });
            ui.ctx().data_mut(|data| data.insert_temp(branch, expanded));
            if expanded {
                page_tree(ui, app, folder, Some(page.id), depth.saturating_add(1), selected, gallery);
            }
        });
    }
}

/// A trailing, always-visible action button makes the context menu discoverable.
fn page_entry(
    ui: &mut egui::Ui,
    app: &mut App,
    page: &mailcraft_model::Note,
    selected: &mut Option<ItemId>,
    gallery: &mut bool,
) {
    let title = notes::title(page);
    let label = if title.is_empty() { "Untitled page" } else { &title };
    let response = ui.add_sized(
        [(ui.available_width() - 28.0 - ui.spacing().item_spacing.x).max(24.0), 34.0],
        egui::Button::new(format!("{}{}", if page.pinned { "★ " } else { "" }, label))
            .truncate()
            .selected(*selected == Some(page.id)),
    );
    a11y(&response, egui::WidgetType::Button, label);
    response.context_menu(|ui| page_actions(ui, app, page, selected, gallery));
    if response.clicked() {
        *selected = Some(page.id);
        *gallery = false;
    }
    let more = egui::containers::menu::MenuButton::from_button(
        egui::Button::new("").frame(false).min_size(egui::vec2(28.0, 34.0)),
    )
    .ui(ui, |ui| page_actions(ui, app, page, selected, gallery))
    .0;
    a11y(&more, egui::WidgetType::Button, &format!("More options for {label}"));
    let tint = if more.hovered() { app.theme.text } else { app.theme.text_muted };
    for y in [-5.0, 0.0, 5.0] {
        ui.painter().circle_filled(more.rect.center() + egui::vec2(0.0, y), 1.4, tint);
    }
    more.on_hover_text("Page actions");
}

fn new_page(app: &mut App, folder: FolderId, parent: Option<ItemId>, selected: &mut Option<ItemId>) {
    if let Some(value) = app.run("notes.new", json!({"folder":folder.0,"text":"","markdown":true}))
        && let Some(id) = value.get("id").and_then(|v| v.as_u64())
    {
        let id = ItemId(id);
        if let Some(parent) = parent {
            app.run("notes.reparent", json!({"id":id.0,"parent":parent.0}));
        }
        *selected = Some(id);
    }
}

pub(super) const BLOCK_TYPES: [(NoteBlockKind, &str, &str); 12] = [
    (NoteBlockKind::Paragraph, "Text", "Write a paragraph"),
    (NoteBlockKind::Heading, "Heading", "Give a section a title"),
    (NoteBlockKind::Checklist, "To-do list", "Track an action with a checkbox"),
    (NoteBlockKind::Quote, "Quote", "Highlight someone’s words"),
    (NoteBlockKind::Callout, "Callout", "Make an important detail stand out"),
    (NoteBlockKind::Code, "Code", "Keep code in a monospace block"),
    (NoteBlockKind::Table, "Table", "Organize information into rows and columns"),
    (NoteBlockKind::Image, "Image", "Add a local PNG or JPEG image"),
    (NoteBlockKind::Agenda, "Calendar agenda", "Live appointments for a day"),
    (NoteBlockKind::Tasks, "Due tasks", "Complete actual tasks from this page"),
    (NoteBlockKind::Habits, "Habit tracker", "Keep a dated history of your routines"),
    (NoteBlockKind::References, "Links & references", "Connect pages, people and dates"),
];

fn choose_block(ui: &mut egui::Ui) -> Option<NoteBlockKind> {
    let mut choice = None;
    ui.set_min_width(270.0);
    ui.label(RichText::new("ADD TO PAGE").small().weak());
    ScrollArea::vertical().max_height(450.0).show(ui, |ui| {
        for (kind, label, description) in BLOCK_TYPES {
            if ui.add_sized([ui.available_width(), 30.0], egui::Button::new(label)).clicked() {
                choice = Some(kind);
                ui.close();
            }
            ui.label(RichText::new(description).small().weak());
            ui.add_space(5.0);
        }
    });
    choice
}

fn appearance(ui: &mut egui::Ui, app: &mut App, note: &mailcraft_model::Note) {
    use mailcraft_model::{NoteColor, note::NotePaper};
    ui.set_min_width(280.0);
    ui.label(RichText::new("Page layout").strong());
    for (width, label) in [(0, "Adaptive"), (720, "Reading"), (960, "Wide"), (1200, "Full page")] {
        if ui.selectable_label(note.content_width == width, label).clicked() {
            app.run("notes.update", json!({"id":note.id.0,"contentWidth":width}));
        }
    }
    let mut cover = note.cover;
    if ui.checkbox(&mut cover, "Show colour cover").changed() {
        app.run("notes.update", json!({"id":note.id.0,"cover":cover}));
    }
    ui.label(RichText::new("The cover follows your page accent colour.").small().weak());
    ui.separator();
    ui.label(RichText::new("Paper").strong());
    ui.horizontal(|ui| {
        for (paper, label, value) in [
            ("plain", "Plain", NotePaper::Plain),
            ("ruled", "Ruled", NotePaper::Ruled),
            ("dotted", "Dotted", NotePaper::Dotted),
        ] {
            if ui.selectable_label(note.paper == value, label).clicked() {
                app.run("notes.update", json!({"id":note.id.0,"paper":paper}));
            }
        }
    });
    ui.separator();
    ui.label(RichText::new("Accent colour").strong());
    ui.horizontal_wrapped(|ui| {
        for (key, label, color) in [
            ("blue", "Blue", NoteColor::Blue),
            ("green", "Green", NoteColor::Green),
            ("pink", "Lavender", NoteColor::Pink),
            ("yellow", "Amber", NoteColor::Yellow),
            ("white", "Slate", NoteColor::White),
        ] {
            let tint = super::notes_templates::tint(color, app.theme.dark);
            if ui.selectable_label(note.color == color, RichText::new(label).color(tint)).clicked() {
                app.run("notes.update", json!({"id":note.id.0,"color":key}));
            }
        }
    });
    ui.separator();
    ui.label(RichText::new("Page icon").strong());
    egui::Grid::new("note-style-icons").num_columns(4).spacing(egui::vec2(14.0, 10.0)).show(ui, |ui| {
        for (index, (icon, label)) in [
            ("note", "Note"),
            ("calendar", "Calendar"),
            ("task", "Checklist"),
            ("appointment", "Meeting"),
            ("folder", "Project"),
            ("person", "People"),
            ("flag", "Goal"),
            ("attach", "Attachment"),
        ]
        .iter()
        .enumerate()
        {
            if crate::widgets::icon_only(ui, app, icon, label, true).clicked() {
                app.run("notes.update", json!({"id":note.id.0,"icon":icon}));
            }
            if index.saturating_add(1) % 4 == 0 {
                ui.end_row();
            }
        }
    });
}

pub(super) fn show(ui: &mut egui::Ui, app: &mut App) {
    ensure_folder(app, Kind::Note);
    // Workspace pages edit in the main pane; command-created notes select their page here.
    let ctx = ui.ctx().clone();
    let mut state = load(&ctx);
    if let Some(editor) = state.editors.note.take() {
        if let Some(folder) = app.s.mailbox.notes.get(&ItemId(editor.id)).map(|note| note.folder) {
            select_folder(app, folder);
        }
        app.ui.module_selection = Some(editor.id);
    }
    store(&ctx, state);
    let Some(folder) = current_folder(app, Kind::Note) else {
        ui.label("Create a Notes folder to begin.");
        return;
    };
    let mut selected = app
        .ui
        .module_selection
        .map(ItemId)
        .filter(|id| app.s.mailbox.notes.get(id).is_some_and(|n| n.folder == folder && !n.is_template));
    for (style, size) in
        [(egui::TextStyle::Body, 15.0), (egui::TextStyle::Button, 14.0), (egui::TextStyle::Small, 12.0)]
    {
        ui.style_mut().text_styles.insert(style, egui::FontId::proportional(size));
    }
    ui.spacing_mut().button_padding = egui::vec2(10.0, 7.0);
    let initial_selected = selected;
    let gallery_key = egui::Id::new("notes-template-gallery");
    let mut gallery = ctx.data(|data| data.get_temp::<bool>(gallery_key)).unwrap_or(false);
    let history_key = egui::Id::new("notes-navigation-history");
    let mut history = ctx.data(|data| data.get_temp::<PageHistory>(history_key)).unwrap_or_default();
    if !gallery {
        history.observe(selected);
    }
    let width = ui.available_width();
    ui.horizontal_top(|ui| {
        ui.allocate_ui_with_layout(
            egui::vec2(270.0, ui.available_height()),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                egui::Frame::new().fill(app.theme.pane).inner_margin(20.0).show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 10.0;
                    ui.add_space(4.0);
                    ui.label(RichText::new("Notebooks").size(20.0).strong());
                    let folders: Vec<_> = app
                        .s
                        .mailbox
                        .folders
                        .values()
                        .filter(|f| f.kind == mailcraft_model::FolderKind::Notes)
                        .map(|f| {
                            let owner = app
                                .s
                                .mailbox
                                .accounts
                                .iter()
                                .find(|a| a.id == f.account)
                                .map(|a| a.display_name.as_str())
                                .unwrap_or("Local");
                            (f.id, format!("{} · {}", f.name, owner))
                        })
                        .collect();
                    let name = folders
                        .iter()
                        .find(|(id, _)| *id == folder)
                        .map(|(_, name)| name.as_str())
                        .unwrap_or("Notebook");
                    egui::ComboBox::from_id_salt("notes-notebook")
                        .selected_text(name)
                        .width(ui.available_width())
                        .show_ui(ui, |ui| {
                            for (id, name) in &folders {
                                if ui.selectable_label(*id == folder, name).clicked() {
                                    select_folder(app, *id);
                                    selected = None;
                                }
                            }
                            ui.separator();
                            if ui.button("Create notebook…").clicked() {
                                app.run("folder.new", json!({"parent":folder.0}));
                            }
                        });
                    ui.add_space(8.0);
                    if ui
                        .add_sized(
                            [ui.available_width(), 38.0],
                            egui::Button::new(RichText::new("+ New page").strong()).fill(app.theme.selected),
                        )
                        .clicked()
                    {
                        new_page(app, folder, None, &mut selected);
                        gallery = false;
                    }
                    if ui.add_sized([ui.available_width(),34.0],egui::Button::new("Template library").right_text("10+ layouts")).clicked() { gallery = true; }
                    ui.add_space(10.0);
                    let search_id = egui::Id::new("notes-page-search");
                    let mut search = ctx.data(|data| data.get_temp::<String>(search_id)).unwrap_or_default();
                    let response = ui.add(TextEdit::singleline(&mut search).char_limit(256).frame(super::notes_templates::field_frame(app)).desired_width(f32::INFINITY).hint_text("Search pages"));
                    a11y(&response, egui::WidgetType::TextEdit, "Search pages");
                    ctx.data_mut(|data| data.insert_temp(search_id, search.clone()));
                    ui.add_space(8.0);
                    let tags:std::collections::BTreeSet<_>=app.s.mailbox.notes.values().filter(|n|n.folder==folder && !n.is_template).flat_map(notes::page_tags).collect();
                    ui.horizontal_wrapped(|ui| { for tag in tags.iter().take(30) { if ui.small_button(format!("#{tag}")).clicked() { ctx.data_mut(|d|d.insert_temp(search_id,format!("#{tag}"))); } } });
                    ui.label(RichText::new("PAGES").small().color(app.theme.text_muted));
                    ScrollArea::vertical().id_salt("notes-page-tree").show(ui, |ui| {
                        if search.trim().is_empty() {
                            page_tree(ui, app, folder, None, 0, &mut selected, &mut gallery);
                        } else {
                            let query = search.to_lowercase();
                            let pages:Vec<_>=app.s.mailbox.notes.values().filter(|page| {
                                page.folder == folder && !page.is_template && (page.text.to_lowercase().contains(&query) || notes::page_tags(page).contains(query.trim_start_matches('#')))
                            }).cloned().collect();
                            for page in pages {
                                ui.push_id(page.id.0,|ui|ui.horizontal(|ui|page_entry(ui,app,&page,&mut selected,&mut gallery)));
                            }
                        }
                    });
                });
            },
        );
        if selected != initial_selected { gallery = false; }
        ctx.data_mut(|data|data.insert_temp(gallery_key,gallery));
        ui.separator();
        ui.allocate_ui_with_layout(
            egui::vec2((width - 290.0).max(240.0), ui.available_height()),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                egui::Frame::new().inner_margin(24.0).show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 12.0;
                    if gallery || selected.is_none() {
                        super::notes_templates::show(ui,app,folder,&mut selected);
                        return;
                    }
                    let Some(id) = selected else {return;};
                    let Some(note) = app.s.mailbox.notes.get(&id).cloned() else {
                        return;
                    };
                    ui.horizontal(|ui| {
                        ui.allocate_ui_with_layout(egui::vec2((ui.available_width()-100.0).max(180.0),34.0),egui::Layout::left_to_right(egui::Align::Center),|ui|page_navigation(ui,app,&note,&mut selected,&mut gallery,&mut history));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let status = if app.s.store.is_none() {
                                "Session only"
                            } else if !app.s.pending_ops.is_empty() {
                                "Save pending"
                            } else {
                                "Saved locally"
                            };
                            ui.label(RichText::new(status).small().color(app.theme.text_muted));
                        });
                    });
                    if selected!=Some(id) {ui.ctx().request_repaint();return;}
                    ui.separator();
                    let canvas_width = ui.available_width().min(if note.content_width!=0 {f32::from(note.content_width)} else if note.blocks.iter().any(|b|b.section!=0) {1080.0} else {800.0});
                    let canvas_rect = egui::Rect::from_min_size(ui.cursor().min + egui::vec2(((ui.available_width()-canvas_width)/2.0).max(0.0),0.0),egui::vec2(canvas_width,ui.available_height()));
                    ui.scope_builder(egui::UiBuilder::new().max_rect(canvas_rect), |ui| {
                    ui.set_max_width(canvas_width);
                    ui.add_space(24.0);
                    let page_tint = super::notes_templates::tint(note.color,app.theme.dark);
                    if note.cover && ui.available_height()>360.0 {
                        let (cover,_) = ui.allocate_exact_size(egui::vec2(ui.available_width(),112.0),egui::Sense::hover());
                        super::notes_editing::cover(ui,cover,app,page_tint);
                        ui.add_space(16.0);
                    }
                    ui.horizontal(|ui| {
                        let (badge,_) = ui.allocate_exact_size(egui::vec2(52.0,52.0),egui::Sense::hover());
                        ui.painter().rect_filled(badge,if app.theme.terminal_style {3.0}else{12.0},page_tint.gamma_multiply(0.14));
                        let icon = if notes::PAGE_ICONS.contains(&note.icon.as_str()) {note.icon.as_str()} else {"note"};
                        crate::widgets::paint_icon(ui,app,badge.center(),28.0,icon,page_tint);

                    });
                    let mut title = notes::title(&note);
                    let response = ui.add(
                        TextEdit::singleline(&mut title)
                            .id(egui::Id::new(("note-title", id.0)))
                            .font(super::notes_editing::heading_font(ui,34.0))
                            .frame(egui::Frame::NONE)
                            .hint_text(RichText::new("Untitled page").size(32.0))
                            .desired_width(f32::INFINITY),
                    );
                    a11y(&response, egui::WidgetType::TextEdit, "Page title");
                    let focus_key=egui::Id::new("note-focus-title");
                    if ui.ctx().data(|data|data.get_temp::<u64>(focus_key))==Some(id.0) {
                        response.request_focus();
                        if let Some(mut state)=TextEdit::load_state(ui.ctx(),response.id) {
                            state.cursor.set_char_range(Some(egui::text::CCursorRange::two(egui::text::CCursor::new(0),egui::text::CCursor::new(title.chars().count()))));
                            state.store(ui.ctx(),response.id);
                        }
                        ui.ctx().data_mut(|data|data.remove::<u64>(focus_key));
                    }
                    let mut changed = response.changed();
                    if note.source_mail.is_some() && ui.button("Open source email").clicked() {
                        app.run("notes.openSource", json!({"id":id.0}));
                    }
                    if !note.categories.is_empty() {
                        ui.label(
                            RichText::new(note.categories.join(" · ")).small().color(app.theme.text_muted),
                        );
                    }
                    let backlinks=notes::backlinks(&app.s,id);
                    if !backlinks.is_empty() {
                        ui.horizontal_wrapped(|ui| {
                            ui.label(RichText::new("Linked from").small().weak());
                            for (source,label) in backlinks {
                                if ui.link(label).clicked() { selected=Some(source); }
                            }
                        });
                    }
                    ui.add_space(8.0);
                    let mut content = blocks(&note);
                    let mut actions = BlockActions::default();
                    let count = content.len();
                    let paper_rect = egui::Rect::from_min_size(ui.cursor().min,egui::vec2(ui.available_width(),(ui.available_height()-85.0).max(150.0)));
                    super::notes_templates::paper(ui,paper_rect,note.paper,page_tint);
                    ScrollArea::vertical()
                        .id_salt(("notes-page", id.0))
                        .max_height((ui.available_height() - 120.0).max(150.0))
                        .show(ui, |ui| {
                            let mut begin=0;
                            while begin < content.len() {
                                let section=content.get(begin).map(|b|b.section).unwrap_or(0);
                                let end=if section==0 { begin.saturating_add(1) } else {
                                    content.iter().enumerate().skip(begin).find(|(_,b)|b.section!=section).map(|(i,_)|i).unwrap_or(content.len())
                                };
                                let lanes=content.get(begin..end).unwrap_or_default().iter().map(|b|usize::from(b.column).min(2)+1).max().unwrap_or(1);
                                if lanes>1 && ui.available_width()>=620.0 {
                                    ui.columns(lanes,|columns| {
                                        for (lane,column_ui) in columns.iter_mut().enumerate() {
                                            for (index,block) in content.iter_mut().enumerate().take(end).skip(begin).filter(|(_,b)|usize::from(b.column).min(2)==lane) {
                                                render_block(column_ui,block,index,BlockContext{app,id,count,changed:&mut changed,actions:&mut actions,selected:&mut selected});
                                            }
                                        }
                                    });
                                } else {
                                    for lane in 0..lanes {
                                        for (index,block) in content.iter_mut().enumerate().take(end).skip(begin).filter(|(_,b)|usize::from(b.column).min(2)==lane) {
                                            render_block(ui,block,index,BlockContext{app,id,count,changed:&mut changed,actions:&mut actions,selected:&mut selected});
                                        }
                                    }
                                }
                                begin=end;
                            }
                        });
                    if let Some((index, kind)) = actions.insert_after
                        && let Some(next) = content.iter().map(|b| b.id).max().unwrap_or(0).checked_add(1)
                        && index < content.len()
                    {
                        let (section,column)=content.get(index).map(|b|(b.section,b.column)).unwrap_or_default();
                        content.insert(index + 1, NoteBlock { id: next, kind, section,column,..Default::default() });
                        ui.ctx().data_mut(|data| {
                            data.insert_temp(egui::Id::new("note-focus-next"), (id.0, next))
                        });
                        changed = true;
                    }
                    if let Some(index) = actions.remove
                        && index < content.len()
                    {
                        content.remove(index);
                        changed = true;
                    }
                    if let Some(index) = actions.move_up
                        && index > 0
                        && index < content.len()
                    {
                        adopt_position(&mut content,index,index-1);
                        content.swap(index, index - 1);
                        changed = true;
                    }
                    if let Some(index) = actions.move_down
                        && index.saturating_add(1) < content.len()
                    {
                        adopt_position(&mut content,index,index+1);
                        content.swap(index, index + 1);
                        changed = true;
                    }
                    if let Some((source,target))=actions.drag {
                        changed |= move_block(&mut content,source,target);
                    }
                    ui.add_space(12.0);
                    ui.horizontal(|ui| {
                        if content.len() <= 498 && ui.button("+ Two columns").clicked() {
                            let section=content.iter().map(|b|b.section).max().unwrap_or(0).saturating_add(1);
                            let next=content.iter().map(|b|b.id).max().unwrap_or(0);
                            if let Some(last)=next.checked_add(2) {
                                content.push(NoteBlock{id:last-1,section,column:0,..Default::default()});
                                content.push(NoteBlock{id:last,section,column:1,..Default::default()});
                                changed=true;
                            }
                        }
                        ui.add_enabled_ui(content.len() < 500, |ui| {
                            egui::containers::menu::MenuButton::from_button(egui::Button::new("+ Add to page").frame(true).fill(app.theme.control_surface()).stroke(egui::Stroke::new(1.0,app.theme.control_edge()))).ui(ui, |ui| {
                                if let Some(kind) = choose_block(ui)
                                    && let Some(next) =
                                        content.iter().map(|b| b.id).max().unwrap_or(0).checked_add(1)
                                {
                                    content.push(NoteBlock { id: next, kind, ..Default::default() });
                                    ui.ctx().data_mut(|data| {
                                        data.insert_temp(egui::Id::new("note-focus-next"), (id.0, next))
                                    });
                                    changed = true;
                                }
                            });
                        });
                        if ui
                            .add_enabled(
                                app.files.is_some() && app.s.store.is_some(),
                                egui::Button::new("Add image…"),
                            )
                            .clicked()
                            && let Some(paths) = app.files.as_ref().and_then(|files| {
                                files.open_files(
                                    "Add page image",
                                    &[crate::FileFilter::new("Images", &["png", "jpg", "jpeg"])],
                                    false,
                                    None,
                                )
                            })
                            && let Some(path) = paths.first()
                        {
                            app.run("notes.attachImage", json!({"id":id.0,"path":path.to_string_lossy()}));
                        }
                    });
                    if changed {
                        let text = notes::block_text(&title, &content);
                        app.run(
                            "notes.update",
                            json!({"id":id.0,"text":text,"blocks":content,"markdown":true,"autosave":true}),
                        );
                    }
                    ui.label(
                        RichText::new(
                            "Type / to insert a block. Select text to format it. Hover beside a block to move it or open its menu.",
                        )
                        .small()
                        .color(app.theme.text_muted),
                    );
                    });
                });
            },
        );
    });
    if !gallery {
        history.observe(selected);
    }
    ctx.data_mut(|data| {
        data.insert_temp(history_key, history);
        data.insert_temp(gallery_key, gallery);
    });
    select_item(app, selected.map(|id| id.0));
}

#[derive(Default)]
struct BlockActions {
    insert_after: Option<(usize, NoteBlockKind)>,
    move_up: Option<usize>,
    move_down: Option<usize>,
    remove: Option<usize>,
    drag: Option<(u64, u64)>,
}
struct BlockContext<'a> {
    app: &'a mut App,
    id: ItemId,
    count: usize,
    changed: &'a mut bool,
    actions: &'a mut BlockActions,
    selected: &'a mut Option<ItemId>,
}
fn render_block(ui: &mut egui::Ui, block: &mut NoteBlock, index: usize, context: BlockContext<'_>) {
    let BlockContext { app, id, count, changed, actions, selected } = context;
    let bounds_key = egui::Id::new(("notes-block-bounds", id.0, block.id));
    let previous = ui.ctx().data(|d| d.get_temp::<egui::Rect>(bounds_key));
    let active = ui.memory(|m| m.has_focus(egui::Id::new(("note-block", id.0, block.id))))
        || ui.input(|i| {
            i.pointer.hover_pos().is_some_and(|p| previous.is_some_and(|r| r.expand(10.0).contains(p)))
        })
        || egui::Popup::is_any_open(ui.ctx());
    let response = ui
        .push_id(block.id, |ui| {
            ui.add_space(8.0);
            ui.horizontal_top(|ui| {
                let kind_label = BLOCK_TYPES
                    .iter()
                    .find(|(kind, _, _)| *kind == block.kind)
                    .map(|(_, label, _)| *label)
                    .unwrap_or("Image");
                let kind_label =
                    if block.kind == NoteBlockKind::Checklist { "Checklist" } else { kind_label };
                ui.scope(|ui| {
                    if !active {
                        ui.set_opacity(0.0);
                    }
                    ui.menu_button(RichText::new("...").color(app.theme.text_muted), |ui| {
                        ui.set_opacity(1.0);
                        ui.set_min_width(240.0);
                        ui.spacing_mut().item_spacing.y = 6.0;
                        ui.spacing_mut().interact_size.y = 30.0;
                        ui.label(RichText::new(format!("{kind_label} block")).strong());
                        ui.menu_button("Change type", |ui| {
                            if let Some(kind) = choose_block(ui) {
                                block.kind = kind;
                                *changed = true;
                            }
                        });
                        ui.separator();
                        if block.section != 0 {
                            ui.label("COLUMN");
                            for (column, label) in [(0, "Left"), (1, "Right"), (2, "Third")] {
                                if ui.selectable_label(block.column == column, label).clicked() {
                                    block.column = column;
                                    *changed = true;
                                    ui.close();
                                }
                            }
                        }
                        egui::containers::menu::SubMenuButton::new("References")
                            .config(
                                egui::containers::menu::MenuConfig::new()
                                    .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside),
                            )
                            .ui(ui, |ui| {
                                *changed |= reference_controls(ui, app, block, selected, true);
                            });
                        ui.add_enabled_ui(count < 500, |ui| {
                            ui.menu_button("Insert below", |ui| {
                                if let Some(kind) = choose_block(ui) {
                                    actions.insert_after = Some((index, kind));
                                }
                            });
                        });
                        if index > 0 && ui.button("Move up").clicked() {
                            actions.move_up = Some(index);
                            ui.close();
                        }
                        if index.saturating_add(1) < count && ui.button("Move down").clicked() {
                            actions.move_down = Some(index);
                            ui.close();
                        }
                        if ui.button("Delete block").clicked() {
                            actions.remove = Some(index);
                            ui.close();
                        }
                    })
                    .response
                    .on_hover_text("Block options: change type, insert below, move or delete");
                });
                let (grip_rect, grip) = ui.allocate_exact_size(egui::vec2(12.0, 22.0), egui::Sense::drag());
                for x in [-2.0, 2.0] {
                    for y in [-4.0, 0.0, 4.0] {
                        ui.painter().circle_filled(
                            grip_rect.center() + egui::vec2(x, y),
                            1.0,
                            if active {
                                app.theme.text_muted.gamma_multiply(0.6)
                            } else {
                                egui::Color32::TRANSPARENT
                            },
                        );
                    }
                }
                let grip = grip.on_hover_text("Drag to move this block");
                a11y(&grip, egui::WidgetType::Label, &format!("Move block {}", block.id));
                grip.dnd_set_drag_payload((id.0, block.id));
                if block.kind == NoteBlockKind::Checklist {
                    *changed |= ui.checkbox(&mut block.checked, "").changed();
                }
                let font = if block.kind == NoteBlockKind::Heading {
                    super::notes_editing::heading_font(ui, 25.0)
                } else if block.kind == NoteBlockKind::Code {
                    egui::FontId::monospace(14.0)
                } else {
                    egui::FontId::proportional(16.0)
                };
                egui::Frame::new()
                    .corner_radius(if app.theme.terminal_style { 2 } else { 8 })
                    .fill(
                        if matches!(
                            block.kind,
                            NoteBlockKind::Callout
                                | NoteBlockKind::Code
                                | NoteBlockKind::Quote
                                | NoteBlockKind::Table
                        ) {
                            app.theme.card_surface()
                        } else {
                            egui::Color32::TRANSPARENT
                        },
                    )
                    .inner_margin(
                        if matches!(
                            block.kind,
                            NoteBlockKind::Callout
                                | NoteBlockKind::Code
                                | NoteBlockKind::Quote
                                | NoteBlockKind::Table
                        ) {
                            10.0
                        } else {
                            4.0
                        },
                    )
                    .show(ui, |ui| {
                        ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                            if live_block(ui, app, id, block, selected) {
                                *changed = true;
                            }
                            if matches!(
                                block.kind,
                                NoteBlockKind::Agenda
                                    | NoteBlockKind::Tasks
                                    | NoteBlockKind::Habits
                                    | NoteBlockKind::References
                            ) {
                                return;
                            }
                            if block.kind == NoteBlockKind::Image {
                                if block.blob.is_none() {
                                    ui.label(RichText::new("Add an image to this page").strong());
                                    if ui
                                        .add_enabled(
                                            app.files.is_some() && app.s.store.is_some(),
                                            egui::Button::new("Choose image…"),
                                        )
                                        .clicked()
                                        && let Some(paths) = app.files.as_ref().and_then(|files| {
                                            files.open_files(
                                                "Add page image",
                                                &[crate::FileFilter::new("Images", &["png", "jpg", "jpeg"])],
                                                false,
                                                None,
                                            )
                                        })
                                        && let Some(path) = paths.first()
                                    {
                                        app.run(
                                            "notes.attachImage",
                                            json!({"id":id.0,"block":block.id,"path":path.to_string_lossy()}),
                                        );
                                        if let Some(saved) = app
                                            .s
                                            .mailbox
                                            .notes
                                            .get(&id)
                                            .and_then(|n| n.blocks.iter().find(|b| b.id == block.id))
                                        {
                                            *block = saved.clone();
                                        }
                                    }
                                    if app.s.store.is_none() {
                                        ui.label(
                                            RichText::new("Save the profile to disk to add images.")
                                                .small()
                                                .weak(),
                                        );
                                    }
                                } else {
                                    page_image(ui, app, block);
                                }
                                *changed |= ui
                                    .add(TextEdit::singleline(&mut block.text).hint_text("Image caption"))
                                    .changed();
                                return;
                            }
                            if block.kind == NoteBlockKind::Table {
                                *changed |= table(ui, block);
                                return;
                            }
                            let response = super::notes_editing::text(ui, app, id.0, block, font);
                            a11y(&response, egui::WidgetType::TextEdit, "Block text");
                            if !block.references.is_empty() {
                                *changed |= reference_controls(ui, app, block, selected, false);
                            }
                            *changed |= response.changed();
                            let focus_key = egui::Id::new("note-focus-next");
                            if ui.ctx().data(|data| data.get_temp::<(u64, u64)>(focus_key))
                                == Some((id.0, block.id))
                            {
                                response.request_focus();
                                ui.ctx().data_mut(|data| data.remove::<(u64, u64)>(focus_key));
                            }
                        });
                    });
            });
        })
        .response;
    ui.ctx().data_mut(|d| d.insert_temp(bounds_key, response.rect));
    if let Some(payload) = response.dnd_hover_payload::<(u64, u64)>()
        && payload.0 == id.0
        && payload.1 != block.id
    {
        ui.painter().line_segment(
            [response.rect.left_top() + egui::vec2(48.0, 0.0), response.rect.right_top()],
            egui::Stroke::new(2.0, app.theme.accent),
        );
    }
    if let Some(payload) = response.dnd_release_payload::<(u64, u64)>()
        && payload.0 == id.0
        && payload.1 != block.id
    {
        actions.drag = Some((payload.1, block.id));
    }
}

fn adopt_position(content: &mut [NoteBlock], source: usize, target: usize) {
    if let Some((section, column)) = content.get(target).map(|b| (b.section, b.column))
        && let Some(block) = content.get_mut(source)
    {
        block.section = section;
        block.column = column;
    }
}
fn move_block(content: &mut Vec<NoteBlock>, source: u64, target: u64) -> bool {
    let Some(from) = content.iter().position(|b| b.id == source) else { return false };
    let Some(to) = content.iter().position(|b| b.id == target) else { return false };
    if from == to {
        return false;
    }
    adopt_position(content, from, to);
    let block = content.remove(from);
    content.insert(if from < to { to.saturating_sub(1) } else { to }, block);
    true
}

fn live_block(
    ui: &mut egui::Ui,
    app: &mut App,
    page: ItemId,
    block: &mut NoteBlock,
    selected: &mut Option<ItemId>,
) -> bool {
    use mailcraft_model::note::NoteHabit;
    if !matches!(
        block.kind,
        NoteBlockKind::Agenda | NoteBlockKind::Tasks | NoteBlockKind::Habits | NoteBlockKind::References
    ) {
        return false;
    }
    let mut changed = false;
    ui.set_min_width(ui.available_width().min(240.0));
    let heading = ui.add(
        TextEdit::singleline(&mut block.text)
            .font(egui::FontId::proportional(22.0))
            .frame(egui::Frame::NONE)
            .hint_text("Section title")
            .desired_width(ui.available_width()),
    );
    a11y(&heading, egui::WidgetType::TextEdit, "Section title");
    changed |= heading.changed();
    let current = today(app);
    match block.kind {
        NoteBlockKind::Agenda => {
            ui.horizontal(|ui| {
                if ui.small_button("‹").on_hover_text("Previous day").clicked() {
                    block.day_offset = block.day_offset.saturating_sub(1).max(-366);
                    changed = true;
                }
                if ui.small_button("›").on_hover_text("Next day").clicked() {
                    block.day_offset = block.day_offset.saturating_add(1).min(366);
                    changed = true;
                }
                ui.label(
                    RichText::new(day_string(current.saturating_add(i64::from(block.day_offset))))
                        .small()
                        .weak(),
                );
            });
            let day = current.saturating_add(i64::from(block.day_offset));
            let rows = notes::agenda(&app.s, day);
            egui::Frame::new()
                .fill(app.theme.card_surface())
                .corner_radius(if app.theme.terminal_style { 3.0 } else { 8.0 })
                .inner_margin(12.0)
                .show(ui, |ui| {
                    ui.set_min_width((ui.available_width() - 4.0).max(120.0));
                    if rows.is_empty() {
                        ui.label(RichText::new("No appointments this day").weak());
                    }
                    for (id, title, start, all_day) in rows {
                        ui.horizontal_wrapped(|ui| {
                            let seconds = local_secs(start, tz(app)).rem_euclid(DAY);
                            let time = if all_day {
                                "All day".into()
                            } else {
                                format!("{:02}:{:02}", seconds / 3600, (seconds % 3600) / 60)
                            };
                            ui.label(RichText::new(time).small().weak());
                            if ui.link(title).clicked() {
                                app.run("notes.openReference", json!({"kind":"appointment","id":id.0}));
                            }
                        });
                    }
                    if ui.small_button("Open day in Calendar").clicked() {
                        super::calendar_command(
                            ui.ctx(),
                            app,
                            "calendar.goToDate",
                            json!({"date":day_string(day),"mode":"day"}),
                        );
                    }
                });
        }
        NoteBlockKind::Tasks => {
            ui.horizontal(|ui| {
                ui.label(RichText::new("Overdue + next").small().weak());
                changed |= ui
                    .add(egui::DragValue::new(&mut block.day_offset).range(0..=366).suffix(" days"))
                    .changed();
            });
            let rows = notes::due_tasks(&app.s, current.saturating_add(i64::from(block.day_offset)));
            if rows.is_empty() {
                ui.label(RichText::new("No dated tasks due in this window").weak());
            }
            for (id, title, due) in rows {
                ui.horizontal_wrapped(|ui| {
                    let mut complete = false;
                    if ui.checkbox(&mut complete, &title).changed() {
                        app.run("tasks.markComplete", json!({"ids":[id.0]}));
                    }
                    if ui.small_button(day_string(due)).on_hover_text("Open task").clicked() {
                        app.run("notes.openReference", json!({"kind":"task","id":id.0}));
                    }
                });
            }
            if ui.small_button("+ New task").clicked() {
                app.run("tasks.new", json!({}));
            }
        }
        NoteBlockKind::Habits => {
            let key = egui::Id::new(("habit-week", page.0, block.id));
            let mut offset = ui.ctx().data(|d| d.get_temp::<i64>(key)).unwrap_or(0);
            ui.horizontal(|ui| {
                if ui.small_button("‹").on_hover_text("Previous week").clicked() {
                    offset = offset.saturating_sub(7).max(-3654);
                }
                if ui.small_button("This week").clicked() {
                    offset = 0;
                }
                if ui.small_button("›").on_hover_text("Next week").clicked() {
                    offset = offset.saturating_add(7).min(3654);
                }
            });
            ui.ctx().data_mut(|d| d.insert_temp(key, offset));
            let monday =
                current.saturating_sub((current.saturating_add(3)).rem_euclid(7)).saturating_add(offset);
            ui.label(RichText::new(format!("Week of {}", day_string(monday))).small().weak());
            let mut remove = None;
            for (index, habit) in block.habits.iter_mut().enumerate() {
                ui.push_id(index, |ui| {
                    let wide = ui.available_width() >= 420.0;
                    ui.allocate_ui_with_layout(
                        egui::vec2(ui.available_width(), if wide { 32.0 } else { 64.0 }),
                        if wide {
                            egui::Layout::left_to_right(egui::Align::Center)
                        } else {
                            egui::Layout::top_down(egui::Align::Min)
                        },
                        |ui| {
                            ui.horizontal(|ui| {
                                changed |= ui
                                    .add(
                                        TextEdit::singleline(&mut habit.name)
                                            .frame(egui::Frame::NONE)
                                            .char_limit(60)
                                            .hint_text("Habit name")
                                            .desired_width(if wide {
                                                120.0
                                            } else {
                                                (ui.available_width() - 28.0).max(80.0)
                                            }),
                                    )
                                    .changed();
                                if ui.small_button("×").on_hover_text("Remove habit").clicked() {
                                    remove = Some(index);
                                }
                            });
                            ui.horizontal(|ui| {
                                for (i, label) in ["M", "T", "W", "T", "F", "S", "S"].into_iter().enumerate()
                                {
                                    let day = monday.saturating_add(i64::try_from(i).unwrap_or(0));
                                    let mut checked = habit.days.contains(&day);
                                    let color =
                                        if checked { app.theme.complete } else { app.theme.text_muted };
                                    let response = ui
                                        .add(
                                            egui::Button::new(RichText::new(label).color(color))
                                                .small()
                                                .min_size(egui::vec2(24.0, 24.0))
                                                .corner_radius(12.0)
                                                .fill(if checked {
                                                    app.theme.complete.gamma_multiply(0.12)
                                                } else {
                                                    egui::Color32::TRANSPARENT
                                                })
                                                .stroke(egui::Stroke::new(
                                                    1.0,
                                                    if checked {
                                                        app.theme.complete
                                                    } else {
                                                        app.theme.separator
                                                    },
                                                )),
                                        )
                                        .on_hover_text(format!("{} · {}", habit.name, day_string(day)));
                                    let day_name = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"]
                                        .get(i)
                                        .copied()
                                        .unwrap_or("Day");
                                    response.widget_info(|| {
                                        egui::WidgetInfo::selected(
                                            egui::WidgetType::SelectableLabel,
                                            true,
                                            checked,
                                            format!("{}, {day_name}", habit.name),
                                        )
                                    });
                                    if response.clicked() {
                                        checked = !checked;
                                        if checked && habit.days.len() < 3660 {
                                            habit.days.insert(day);
                                        } else if !checked {
                                            habit.days.remove(&day);
                                        }
                                        changed = true;
                                    }
                                }
                                let done = habit.days.range(monday..=monday.saturating_add(6)).count();
                                ui.label(RichText::new(format!("{done}/7")).small().weak());
                            });
                        },
                    );
                });
            }
            if let Some(index) = remove {
                block.habits.remove(index);
                changed = true;
            }
            if block.habits.len() < 30 && ui.small_button("+ Add habit").clicked() {
                block.habits.push(NoteHabit::default());
                changed = true;
            }
        }
        NoteBlockKind::References => {}
        _ => {}
    }
    // Every block can carry references; this dedicated block makes the controls discoverable.
    changed |= reference_controls(ui, app, block, selected, block.kind == NoteBlockKind::References);
    changed
}
fn reference_controls(
    ui: &mut egui::Ui,
    app: &mut App,
    block: &mut NoteBlock,
    selected: &mut Option<ItemId>,
    show_add: bool,
) -> bool {
    use mailcraft_model::note::NoteReference;
    let mut changed = false;
    let mut remove = None;
    for (index, reference) in block.references.iter().enumerate() {
        ui.horizontal_wrapped(|ui| {
            match reference {
                NoteReference::Page(id) => {
                    let title = app
                        .s
                        .mailbox
                        .notes
                        .get(id)
                        .filter(|n| notes::reference_available(&app.s, "page", n.id))
                        .map(notes::title);
                    if ui
                        .add_enabled(
                            title.is_some(),
                            egui::Button::new(format!("↗ {}", title.unwrap_or("Page unavailable".into()))),
                        )
                        .clicked()
                    {
                        if let Some(folder) = app.s.mailbox.notes.get(id).map(|n| n.folder) {
                            select_folder(app, folder);
                        }
                        *selected = Some(*id);
                        app.run("notes.openReference", json!({"kind":"page","id":id.0}));
                    }
                }
                NoteReference::Person(id) => {
                    let name = app
                        .s
                        .mailbox
                        .contacts
                        .get(id)
                        .filter(|p| notes::reference_available(&app.s, "person", p.id))
                        .map(|p| p.file_as.clone());
                    if ui
                        .add_enabled(
                            name.is_some(),
                            egui::Button::new(format!("@ {}", name.unwrap_or("Person unavailable".into()))),
                        )
                        .clicked()
                    {
                        app.run("notes.openReference", json!({"kind":"person","id":id.0}));
                    }
                }
                NoteReference::Date(day) => {
                    if ui.link(format!("@ {}", day_string(*day))).clicked() {
                        super::calendar_command(
                            ui.ctx(),
                            app,
                            "calendar.goToDate",
                            json!({"date":day_string(*day),"mode":"day"}),
                        );
                    }
                }
            }
            if ui.small_button("×").on_hover_text("Remove reference").clicked() {
                remove = Some(index);
            }
        });
    }
    if let Some(index) = remove {
        block.references.remove(index);
        changed = true;
    }
    if show_add && block.references.len() < 100 {
        egui::containers::menu::MenuButton::new("+ Link page, person or date")
            .config(
                egui::containers::menu::MenuConfig::new()
                    .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside),
            )
            .ui(ui, |ui| {
                ui.set_min_width(260.0);
                ui.menu_button("Page", |ui| {
                    ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
                        for note in app
                            .s
                            .mailbox
                            .notes
                            .values()
                            .filter(|n| notes::reference_available(&app.s, "page", n.id))
                        {
                            if ui.button(notes::title(note)).clicked() {
                                block.references.push(NoteReference::Page(note.id));
                                changed = true;
                                ui.close();
                            }
                        }
                    });
                });
                ui.menu_button("Person", |ui| {
                    ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
                        for person in app
                            .s
                            .mailbox
                            .contacts
                            .values()
                            .filter(|p| notes::reference_available(&app.s, "person", p.id))
                        {
                            if ui.button(&person.file_as).clicked() {
                                block.references.push(NoteReference::Person(person.id));
                                changed = true;
                                ui.close();
                            }
                        }
                    });
                });
                let key = egui::Id::new(("note-reference-day", block.id));
                let mut offset = ui.ctx().data(|d| d.get_temp::<i16>(key)).unwrap_or(0);
                ui.horizontal(|ui| {
                    ui.label("Days from today");
                    ui.add(egui::DragValue::new(&mut offset).range(-366..=366));
                });
                ui.ctx().data_mut(|d| d.insert_temp(key, offset));
                let day = today(app).saturating_add(i64::from(offset));
                if ui.button(format!("Link {}", day_string(day))).clicked() {
                    block.references.push(NoteReference::Date(day));
                    changed = true;
                    ui.close();
                }
            });
    }
    changed
}

/// Actions always target the clicked page, independently of the page currently being read.
fn page_actions(
    ui: &mut egui::Ui,
    app: &mut App,
    note: &mailcraft_model::Note,
    selected: &mut Option<ItemId>,
    gallery: &mut bool,
) {
    let id = note.id;
    let folder = note.folder;
    ui.set_min_width(264.0);
    ui.spacing_mut().item_spacing = egui::vec2(8.0, 4.0);
    ui.spacing_mut().interact_size.y = 32.0;
    ui.spacing_mut().button_padding = egui::vec2(12.0, 7.0);
    ui.add(
        egui::Label::new(RichText::new(notes::title(note)).small().color(app.theme.text_muted)).truncate(),
    );
    ui.add_space(5.0);
    if ui.button("Rename").clicked() {
        *selected = Some(id);
        *gallery = false;
        ui.ctx().data_mut(|data| data.insert_temp(egui::Id::new("note-focus-title"), id.0));
        ui.close();
    }

    if ui.button(if note.pinned { "Remove from favourites" } else { "Add to favourites" }).clicked() {
        app.run("notes.update", json!({"id":id.0,"pinned":!note.pinned}));
        ui.close();
    }
    ui.add_space(3.0);
    ui.separator();
    ui.add_space(3.0);
    if ui.button("Add subpage").clicked() {
        new_page(app, folder, Some(id), selected);
        *gallery = false;
        ui.close();
    }
    ui.menu_button("Move under page", |ui| {
        if ui.button("Notebook root").clicked() {
            app.run("notes.reparent", json!({"id":id.0,"parent":null}));
            ui.close();
        }
        let parents: Vec<_> = app
            .s
            .mailbox
            .notes
            .values()
            .filter(|n| n.folder == folder && n.id != id && !n.is_template)
            .map(|n| (n.id, notes::title(n)))
            .collect();
        for (parent, title) in parents {
            if ui.button(title).clicked() {
                app.run("notes.reparent", json!({"id":id.0,"parent":parent.0}));
                ui.close();
            }
        }
    });
    ui.add_space(3.0);
    ui.separator();
    ui.add_space(3.0);
    ui.menu_button("Page style", |ui| appearance(ui, app, note));
    ui.menu_button("Categories", |ui| {
        let mut categories = note.categories.clone();
        super::editors::categories_menu(ui, app, &mut categories);
        if categories != note.categories {
            app.run("notes.update", json!({"id":id.0,"categories":categories}));
        }
    });
    ui.add_space(3.0);
    ui.separator();
    ui.add_space(3.0);
    ui.menu_button("Version history", |ui| {
        if note.history.is_empty() {
            ui.label("Previous versions appear here as you write.");
        }
        for (index, revision) in note.history.iter().enumerate().rev() {
            if ui
                .button(format!(
                    "Restore version {} · {}",
                    index.saturating_add(1),
                    revision.modified.to_rfc5322()
                ))
                .clicked()
            {
                app.run("notes.restoreRevision", json!({"id":id.0,"revision":index}));
                ui.close();
            }
        }
    });
    egui::containers::menu::SubMenuButton::new("Save as template")
        .config(
            egui::containers::menu::MenuConfig::new()
                .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside),
        )
        .ui(ui, |ui| {
            ui.label("Reuse this page's content and styling.");
            let key = egui::Id::new(("note-template-name", id.0));
            let mut name =
                ui.ctx().data(|data| data.get_temp::<String>(key)).unwrap_or_else(|| notes::title(note));
            let response = ui.add(
                TextEdit::singleline(&mut name)
                    .char_limit(120)
                    .frame(super::notes_templates::field_frame(app))
                    .hint_text("Template name"),
            );
            a11y(&response, egui::WidgetType::TextEdit, "Template name");
            ui.ctx().data_mut(|data| data.insert_temp(key, name.clone()));
            if ui.button("Save template").clicked()
                && app.run("notes.saveTemplate", json!({"id":id.0,"name":name})).is_some()
            {
                *gallery = true;
                ui.close();
            }
        });
    ui.separator();
    ui.add_space(3.0);
    if ui.button(RichText::new("Delete page").color(app.theme.danger)).clicked() {
        if app.run("notes.delete", json!({"id":id.0})).is_some() && *selected == Some(id) {
            *selected = None;
        }
        ui.close();
    }
}

/// Session-local page history; navigation never changes the page content.
#[derive(Clone, Default)]
struct PageHistory {
    back: Vec<ItemId>,
    current: Option<ItemId>,
    forward: Vec<ItemId>,
}
impl PageHistory {
    fn observe(&mut self, id: Option<ItemId>) {
        if id.is_none() || id == self.current {
            return;
        }
        if let Some(previous) = self.current {
            self.back.push(previous);
        }
        if self.back.len() > 100 {
            self.back.remove(0);
        }
        self.current = id;
        self.forward.clear();
    }
    fn navigate(&mut self, app: &App, back: bool) -> Option<ItemId> {
        let (source, destination) =
            if back { (&mut self.back, &mut self.forward) } else { (&mut self.forward, &mut self.back) };
        while let Some(id) = source.pop() {
            if notes::reference_available(&app.s, "page", id) {
                if let Some(current) = self.current {
                    destination.push(current);
                }
                if destination.len() > 100 {
                    destination.remove(0);
                }
                self.current = Some(id);
                return Some(id);
            }
        }
        None
    }
}
fn navigate_page(app: &mut App, id: ItemId, selected: &mut Option<ItemId>, gallery: &mut bool) {
    if let Some(folder) = app.s.mailbox.notes.get(&id).map(|n| n.folder) {
        app.run("notes.openReference", json!({"kind":"page","id":id.0}));
        select_folder(app, folder);
        *selected = Some(id);
        *gallery = false;
    }
}
fn page_navigation(
    ui: &mut egui::Ui,
    app: &mut App,
    note: &mailcraft_model::Note,
    selected: &mut Option<ItemId>,
    gallery: &mut bool,
    history: &mut PageHistory,
) {
    for (back, icon, label, enabled) in [
        (
            true,
            "chevron-left",
            "Back to previous page",
            history.back.iter().any(|id| notes::reference_available(&app.s, "page", *id)),
        ),
        (
            false,
            "chevron-right",
            "Forward to next page",
            history.forward.iter().any(|id| notes::reference_available(&app.s, "page", *id)),
        ),
    ] {
        if crate::widgets::icon_only(ui, app, icon, label, enabled).clicked()
            && let Some(id) = history.navigate(app, back)
        {
            navigate_page(app, id, selected, gallery);
        }
    }
    let notebook =
        app.s.mailbox.folders.get(&note.folder).map(|f| f.name.clone()).unwrap_or_else(|| "Notebook".into());
    ui.menu_button(crumb_title(&notebook), |ui| {
        ui.set_min_width(220.0);
        let pages: Vec<_> = app
            .s
            .mailbox
            .notes
            .values()
            .filter(|n| {
                n.folder == note.folder
                    && n.parent.is_none()
                    && notes::reference_available(&app.s, "page", n.id)
            })
            .map(|n| (n.id, notes::title(n)))
            .collect();
        ScrollArea::vertical().max_height(280.0).show(ui, |ui| {
            for (id, title) in pages {
                if ui.button(if title.is_empty() { "Untitled page" } else { &title }).clicked() {
                    navigate_page(app, id, selected, gallery);
                    ui.close();
                }
            }
        });
    });
    let mut parents = Vec::new();
    let mut next = note.parent;
    for _ in 0..12 {
        let Some(id) = next else { break };
        let Some(parent) = app
            .s
            .mailbox
            .notes
            .get(&id)
            .filter(|p| p.folder == note.folder && notes::reference_available(&app.s, "page", p.id))
        else {
            break;
        };
        if id == note.id || parents.iter().any(|(seen, _)| *seen == id) {
            break;
        }
        parents.push((id, notes::title(parent)));
        next = parent.parent;
    }
    parents.reverse();
    if parents.len() > 2 || (!parents.is_empty() && ui.available_width() < 360.0) {
        ui.label(RichText::new("/").weak());
        ui.menu_button("…", |ui| {
            for (id, title) in &parents {
                if ui.button(title).clicked() {
                    navigate_page(app, *id, selected, gallery);
                    ui.close();
                }
            }
        });
        parents = if ui.available_width() >= 240.0 {
            parents.into_iter().rev().take(1).collect()
        } else {
            Vec::new()
        };
    }
    for (id, title) in parents {
        ui.label(RichText::new("/").weak());
        let short = crumb_title(&title);
        if ui.link(short).on_hover_text(title).clicked() {
            navigate_page(app, id, selected, gallery);
        }
    }
    ui.label(RichText::new("/").weak());
    let title = notes::title(note);
    let title = if title.is_empty() { "Untitled page".into() } else { title };
    let response = ui.add(egui::Label::new(RichText::new(&title).color(app.theme.text_muted)).truncate());
    a11y(&response, egui::WidgetType::Label, &format!("Current page: {title}"));
}

fn crumb_title(title: &str) -> String {
    let mut chars = title.chars();
    let short: String = chars.by_ref().take(24).collect();
    if chars.next().is_some() { format!("{short}…") } else { short }
}
