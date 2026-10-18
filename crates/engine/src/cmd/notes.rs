//! Notes commands (`notes.*`).
//!
//! Notes live in notes folders (`Mailbox.notes`); the first line of a note is its title. `notes.new` creates
//! the note straight away (like a paper sticky note) and asks the UI to open it for editing.
//!
//! **View state.** The notes folder's module view (`cmd::view::modules`) keeps the Icon view arrangement
//! (Large Icons, Small Icons, List) and whether the icons are lined up: `notes.iconView` and
//! `notes.lineUpIcons` update it (switching the folder to the Icon view) and return it as
//! `{"view": NotesViewState}`. A `view` param overrides the stored state as the starting point.
//! `notes.lineUpIcons` also returns the grid order of the notes in the current folder.
//!
//! **Icon positions.** Icons dragged off the grid keep their place: `notes.setPosition` stores the note's
//! offset from the grid's top-left corner (points) with the folder's view state, which the profile saves,
//! and [`positions`] reads them back. Line Up Icons drops them (the icons snap back to the grid), and so
//! does Reset View.
//!
//! `notes.forward` sends the note's text in a new message. [`view_rows`] is the list of the Notes views
//! (Icon, Notes List, Last 7 Days).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::cmd::calendar::{pim, pim::Kind};
use crate::cmd::util::opt_str;
use crate::cmd::view::modules::{self, ModuleKind, RowKind, ViewRow};
use crate::{CmdError, CmdResult, CommandSpec, Session, always};
use mailcraft_model::note::{NoteBlock, NoteBlockKind};
use mailcraft_model::{FolderId, ItemId, Note, NoteColor};

/// Icon view arrangement.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum IconMode {
    #[default]
    Large,
    Small,
    List,
}

/// The Notes Icon view state the UI holds.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct NotesViewState {
    pub icon_mode: IconMode,
    /// Icons snapped to the grid (set by Line Up Icons; cleared when the UI lets the user drag icons).
    pub lined_up: bool,
}

/// The starting view state: the `view` param, else the current notes folder's stored state.
fn view_param(s: &Session, p: &Value) -> Result<NotesViewState, CmdError> {
    match p.get("view") {
        None | Some(Value::Null) => Ok(stored_view(s)),
        Some(v) => serde_json::from_value(v.clone()).map_err(|e| CmdError::Params(format!("`view`: {e}"))),
    }
}

/// Icon arrangement stored for the current notes folder.
pub fn stored_view(s: &Session) -> NotesViewState {
    pim::current_folder(s, Kind::Note)
        .map(|f| {
            let st = modules::state(s, f, ModuleKind::Notes);
            NotesViewState { icon_mode: st.icon_mode, lined_up: st.lined_up }
        })
        .unwrap_or_default()
}

/// Stores `v` for the current notes folder (which switches to the Icon view).
fn store_view(s: &mut Session, v: &NotesViewState) {
    if let Some(f) = pim::current_folder(s, Kind::Note) {
        modules::set_icons(s, f, v.icon_mode, v.lined_up);
    }
}

/// Reserved entry of the folder's view state holding the icon positions (JSON after the prefix). The
/// control character keeps it apart from group labels (`cmd::view::is_group_label`).
const NOTE_POS: &str = "\u{1}notes.pos=";
/// Most icon positions kept per folder.
pub const MAX_POSITIONS: usize = 2_000;
/// Largest offset accepted (points).
pub const MAX_OFFSET: f32 = 100_000.0;

fn valid_offset(v: f32) -> bool {
    v.is_finite() && (0.0..=MAX_OFFSET).contains(&v)
}

/// Icon positions of the notes of `folder` that were dragged off the grid: note id → (x, y) offset from the
/// grid's top-left corner. Positions of notes no longer in the folder are left out.
pub fn positions(s: &Session, folder: FolderId) -> BTreeMap<u64, (f32, f32)> {
    s.view
        .collapsed_groups
        .get(&folder)
        .and_then(|list| list.iter().find_map(|e| e.strip_prefix(NOTE_POS)))
        .and_then(|json| serde_json::from_str::<BTreeMap<u64, (f32, f32)>>(json).ok())
        .unwrap_or_default()
        .into_iter()
        .filter(|(id, (x, y))| {
            valid_offset(*x)
                && valid_offset(*y)
                && s.mailbox.notes.get(&ItemId(*id)).is_some_and(|n| n.folder == folder)
        })
        .take(MAX_POSITIONS)
        .collect()
}

fn store_positions(s: &mut Session, folder: FolderId, map: &BTreeMap<u64, (f32, f32)>) {
    let list = s.view.collapsed_groups.entry(folder).or_default();
    list.retain(|e| !e.starts_with(NOTE_POS));
    if !map.is_empty()
        && let Ok(json) = serde_json::to_string(map)
    {
        list.push(format!("{NOTE_POS}{json}"));
    }
    if list.is_empty() {
        s.view.collapsed_groups.remove(&folder);
    }
}

fn offset_param(p: &Value, key: &str) -> Result<f32, CmdError> {
    p.get(key)
        .and_then(Value::as_f64)
        .map(|v| v as f32)
        .filter(|v| valid_offset(*v))
        .ok_or_else(|| CmdError::Params(format!("`{key}` must be a number from 0 to {MAX_OFFSET}")))
}

/// `notes.setPosition {id, x, y}` keeps a dragged icon where it was dropped; `{id, position: null}` puts it
/// back on the grid. Dragging means the icons are no longer lined up.
fn run_set_position(s: &mut Session, p: &Value) -> CmdResult {
    let id = pim::require_targets(s, p, Kind::Note)?
        .first()
        .copied()
        .ok_or_else(|| CmdError::Params("no note given".into()))?;
    let folder =
        s.mailbox.notes.get(&id).map(|n| n.folder).ok_or_else(|| CmdError::Params("no note".into()))?;
    let mut map = positions(s, folder);
    if p.get("position").is_some_and(Value::is_null) {
        map.remove(&id.0);
    } else {
        let (x, y) = (offset_param(p, "x")?, offset_param(p, "y")?);
        if !map.contains_key(&id.0) && map.len() >= MAX_POSITIONS {
            return Err(CmdError::Params(format!("at most {MAX_POSITIONS} icons keep a position")));
        }
        map.insert(id.0, (x, y));
        let st = modules::state(s, folder, ModuleKind::Notes);
        if st.lined_up {
            modules::set_icons(s, folder, st.icon_mode, false);
        }
    }
    store_positions(s, folder, &map);
    Ok(json!({ "id": id.0, "folder": folder.0, "positions": map.len() }))
}

fn parse_color(c: &str) -> Option<NoteColor> {
    match c {
        "blue" => Some(NoteColor::Blue),
        "green" => Some(NoteColor::Green),
        "pink" => Some(NoteColor::Pink),
        "yellow" => Some(NoteColor::Yellow),
        "white" => Some(NoteColor::White),
        _ => None,
    }
}

fn color_name(c: NoteColor) -> &'static str {
    match c {
        NoteColor::Blue => "blue",
        NoteColor::Green => "green",
        NoteColor::Pink => "pink",
        NoteColor::Yellow => "yellow",
        NoteColor::White => "white",
    }
}

/// Title of a note: its first non-empty line.
pub fn title(n: &Note) -> String {
    let line = n.text.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("");
    if n.markdown { line.trim_start_matches('#').trim().to_owned() } else { line.to_owned() }
}

pub fn note_json(n: &Note, tz: i32) -> Value {
    json!({
        "id": n.id.0,
        "folder": n.folder.0,
        "title": title(n),
        "text": n.text,
        "color": color_name(n.color),
        "created": pim::datetime_string(n.created, tz),
        "modified": pim::datetime_string(n.modified, tz),
        "categories": n.categories,
        "markdown": n.markdown, "pinned": n.pinned, "sourceMail": n.source_mail.map(|id|id.0),
        "isTemplate":n.is_template, "paper":n.paper,"icon":n.icon,"contentWidth":n.content_width,"cover":n.cover,
        "parent":n.parent.map(|id|id.0), "blocks":n.blocks,
        "history":n.history.iter().map(|r|json!({"modified":r.modified.to_rfc5322()})).collect::<Vec<_>>(),
    })
}

/// Reject oversized structured input before cloning/deserializing it.
struct BlockSizeCap(usize);
impl std::io::Write for BlockSizeCap {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0 = self.0.checked_sub(bytes.len()).ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::InvalidInput, "Page blocks exceed 1 MiB")
        })?;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
fn apply_note_fields(n: &mut Note, p: &Value) -> Result<bool, CmdError> {
    let mut changed = false;
    if let Some(width) = p.get("contentWidth") {
        n.content_width = width
            .as_u64()
            .filter(|v| matches!(v, 0 | 720 | 960 | 1200))
            .and_then(|v| u16::try_from(v).ok())
            .ok_or_else(|| CmdError::Params("Choose a supported page width.".into()))?;
        changed = true;
    }
    if let Some(cover) = p.get("cover") {
        n.cover = cover.as_bool().ok_or_else(|| CmdError::Params("Cover must be true or false.".into()))?;
        changed = true;
    }
    if let Some(paper) = p.get("paper") {
        n.paper = serde_json::from_value(paper.clone())
            .map_err(|_| CmdError::Params("Paper must be plain, ruled or dotted.".into()))?;
        changed = true;
    }
    if let Some(icon) = p.get("icon") {
        let icon = icon
            .as_str()
            .filter(|icon| PAGE_ICONS.contains(icon))
            .ok_or_else(|| CmdError::Params("Unknown page icon.".into()))?;
        n.icon = icon.into();
        changed = true;
    }
    if let Some(value) = p.get("blocks") {
        if value.as_array().is_none_or(|blocks| blocks.len() > 500) {
            return Err(CmdError::Params("A page supports at most 500 blocks.".into()));
        }
        serde_json::to_writer(BlockSizeCap(pim::MAX_TEXT), value)
            .map_err(|_| CmdError::Params("Page blocks exceed 1 MiB.".into()))?;
        let blocks: Vec<mailcraft_model::note::NoteBlock> = serde_json::from_value(value.clone())
            .map_err(|_| CmdError::Params("Invalid note blocks".into()))?;
        let mut ids = std::collections::BTreeSet::new();
        if blocks.len() > 500
            || blocks.iter().any(|b| b.id == 0 || !ids.insert(b.id))
            || blocks.iter().fold(0usize, |size, b| size.saturating_add(b.text.len())) > pim::MAX_TEXT
            || blocks.iter().any(|b| {
                b.kind == mailcraft_model::note::NoteBlockKind::Table
                    && (b.text.lines().count() > 50
                        || b.text.lines().any(|line| line.split('\t').count() > 12))
            })
        {
            return Err(CmdError::Params("A page supports 500 unique blocks and 1 MiB of text.".into()));
        }
        for block in &blocks {
            let length = block.text.chars().count();
            if block.inline.len() > 256
                || block.inline.iter().any(|r| {
                    r.start >= r.end
                        || r.end > length
                        || r.link.as_ref().is_some_and(|link| {
                            link.len() > 2048
                                || link.chars().any(char::is_control)
                                || !(link.starts_with("https://")
                                    || link.starts_with("http://")
                                    || link.starts_with("mailto:"))
                        })
                })
            {
                return Err(CmdError::Params("Invalid inline formatting range or link.".into()));
            }
        }
        let mut sections = std::collections::BTreeSet::new();
        let mut previous = 0;
        for block in &blocks {
            if block.column > 2 || (block.section == 0 && block.column != 0)
                || block.day_offset.unsigned_abs() > 366
                || block.habits.len() > 30 || block.references.len() > 100
                || block.habits.iter().any(|h| h.name.len() > 240 || h.days.len() > 3660
                    || h.days.iter().any(|d| d.unsigned_abs() > 3_000_000))
                || block.references.iter().any(|r| matches!(r, mailcraft_model::note::NoteReference::Date(d) if d.unsigned_abs() > 3_000_000))
            {
                return Err(CmdError::Params("Invalid page layout, date, habit or reference limits.".into()));
            }
            if block.section != previous && block.section != 0 && !sections.insert(block.section) {
                return Err(CmdError::Params("Column sections must be consecutive.".into()));
            }
            previous = block.section;
        }
        if serde_json::to_vec(&blocks).map_err(|e| CmdError::Params(e.to_string()))?.len() > pim::MAX_TEXT {
            return Err(CmdError::Params("Page blocks exceed 1 MiB.".into()));
        }
        n.blocks = blocks;
        if n.blocks.iter().any(|b| {
            (b.kind == NoteBlockKind::Image && b.blob.is_none())
                || b.blob.as_ref().is_some_and(|key| !n.image_blobs.contains(key))
        }) {
            return Err(CmdError::Params("Image blocks must use images attached to this page.".into()));
        }
        changed = true;
    }
    for (key, target) in [("markdown", &mut n.markdown), ("pinned", &mut n.pinned)] {
        if let Some(value) = p.get(key) {
            *target =
                value.as_bool().ok_or_else(|| CmdError::Params(format!("`{key}` must be true or false")))?;
            changed = true;
        }
    }
    if let Some(t) = pim::text(p, "text")? {
        n.text = t;
        if p.get("blocks").is_none() {
            n.blocks.clear();
        }
        changed = true;
    }
    if let Some(c) = p.get("color").filter(|v| !v.is_null()) {
        n.color = c
            .as_str()
            .and_then(parse_color)
            .ok_or_else(|| CmdError::Params("`color` must be blue|green|pink|yellow|white".into()))?;
        changed = true;
    }
    if let Some(v) = pim::list(p, "categories")? {
        n.categories = v;
        changed = true;
    }
    if p.get("blocks").is_some() {
        n.text = block_text(&title(n), &n.blocks);
        if n.text.len() > pim::MAX_TEXT {
            return Err(CmdError::Params("Page text is too long.".into()));
        }
    }
    Ok(changed)
}

pub fn page_blocks(note: &mailcraft_model::Note) -> Vec<NoteBlock> {
    if !note.blocks.is_empty() {
        return note.blocks.clone();
    }
    let mut lines = note.text.lines();
    lines.next();
    let text = lines.collect::<Vec<_>>().join("\n");
    if !note.markdown || text.lines().count() > 500 {
        return vec![NoteBlock { id: 1, text, ..Default::default() }];
    }
    let mut out: Vec<NoteBlock> = Vec::new();
    let mut code = false;
    for line in text.lines() {
        if line.starts_with("```") {
            code = !code;
            continue;
        }
        let (kind, value, checked) = if code {
            (NoteBlockKind::Code, line, false)
        } else if let Some(value) = line.strip_prefix("## ").or_else(|| line.strip_prefix("### ")) {
            (NoteBlockKind::Heading, value, false)
        } else if let Some(value) = line.strip_prefix("- [ ] ") {
            (NoteBlockKind::Checklist, value, false)
        } else if let Some(value) = line.strip_prefix("- [x] ") {
            (NoteBlockKind::Checklist, value, true)
        } else if let Some(value) = line.strip_prefix("> ") {
            (NoteBlockKind::Quote, value, false)
        } else {
            (NoteBlockKind::Paragraph, line, false)
        };
        if matches!(kind, NoteBlockKind::Paragraph | NoteBlockKind::Code)
            && let Some(last) = out.last_mut().filter(|last| last.kind == kind)
        {
            last.text.push('\n');
            last.text.push_str(value);
            continue;
        }
        out.push(NoteBlock {
            id: u64::try_from(out.len().saturating_add(1)).unwrap_or(u64::MAX),
            kind,
            text: value.into(),
            checked,
            ..Default::default()
        });
    }
    out
}

pub fn block_text(title: &str, blocks: &[mailcraft_model::note::NoteBlock]) -> String {
    use mailcraft_model::note::NoteBlockKind as K;
    let body = blocks
        .iter()
        .map(|b| match b.kind {
            K::Heading => format!("## {}", b.text),
            K::Checklist => format!("- [{}] {}", if b.checked { "x" } else { " " }, b.text),
            K::Quote => format!("> {}", b.text),
            K::Code => format!("```\n{}\n```", b.text),
            K::Image => format!("[Image: {}]", b.text),
            K::Agenda => format!("## {}\n[Live calendar agenda]", b.text),
            K::Tasks => format!("## {}\n[Live due tasks]", b.text),
            K::Habits => format!(
                "## {}\n{}",
                b.text,
                b.habits.iter().map(|h| format!("- {}", h.name)).collect::<Vec<_>>().join("\n")
            ),
            _ => b.text.clone(),
        })
        .collect::<Vec<_>>()
        .join("\n\n");
    format!("# {title}\n\n{body}")
}

fn trim_history(note: &mut Note) {
    while !note.history.is_empty()
        && (note.history.len() > 20
            || note.history.iter().fold(0usize, |size, r| {
                size.saturating_add(r.text.len())
                    .saturating_add(serde_json::to_vec(&r.blocks).map(|v| v.len()).unwrap_or(usize::MAX))
            }) > 2 * 1024 * 1024)
    {
        note.history.drain(..1);
    }
}

pub fn decode_page_image(bytes: &[u8]) -> Result<image::RgbaImage, CmdError> {
    if bytes.len() > 5 * 1024 * 1024 {
        return Err(CmdError::Params("Images are limited to 5 MiB.".into()));
    }
    let mut reader = image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| CmdError::Params(e.to_string()))?;
    if !matches!(reader.format(), Some(image::ImageFormat::Png | image::ImageFormat::Jpeg)) {
        return Err(CmdError::Params("Choose a PNG or JPEG image.".into()));
    }
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(4096);
    limits.max_image_height = Some(4096);
    limits.max_alloc = Some(64 * 1024 * 1024);
    reader.limits(limits);
    reader
        .decode()
        .map(|image| image.to_rgba8())
        .map_err(|_| CmdError::Params("Image is invalid or exceeds the 4096-pixel limit.".into()))
}

fn attach_image(s: &mut Session, p: &Value) -> CmdResult {
    use std::io::Read;
    let id = pim::require_targets(s, p, Kind::Note)?
        .first()
        .copied()
        .ok_or_else(|| CmdError::Params("No page".into()))?;
    let note = s.mailbox.notes.get(&id).ok_or_else(|| CmdError::Params("No page".into()))?;
    let existing = page_blocks(note);
    let target = p
        .get("block")
        .map(|value| {
            value
                .as_u64()
                .filter(|id| {
                    existing
                        .iter()
                        .any(|b| b.id == *id && b.kind == mailcraft_model::note::NoteBlockKind::Image)
                })
                .ok_or_else(|| CmdError::Params("Choose an image block on this page.".into()))
        })
        .transpose()?;
    if note.image_blobs.len() >= 20 || (existing.len() >= 500 && target.is_none()) {
        return Err(CmdError::Params("A page supports 20 image imports and 500 blocks.".into()));
    }
    let next = existing
        .iter()
        .map(|b| b.id)
        .max()
        .unwrap_or(0)
        .checked_add(1)
        .ok_or_else(|| CmdError::Params("Block IDs exhausted".into()))?;
    let path = pim::text(p, "path")?.ok_or_else(|| CmdError::Params("Image path required".into()))?;
    let file = std::fs::File::open(&path).map_err(|e| CmdError::Failed(e.to_string()))?;
    if !file.metadata().map_err(|e| CmdError::Failed(e.to_string()))?.is_file() {
        return Err(CmdError::Params("Choose an image file.".into()));
    }
    let mut bytes = Vec::new();
    file.take(5 * 1024 * 1024 + 1).read_to_end(&mut bytes).map_err(|e| CmdError::Failed(e.to_string()))?;
    decode_page_image(&bytes)?;
    let store = s
        .store
        .as_mut()
        .ok_or_else(|| CmdError::Disabled("Save the profile to disk before adding images.".into()))?;
    let key = store.put_blob(&bytes).map_err(|e| CmdError::Failed(e.to_string()))?;
    let mut note = s.mailbox.notes.get(&id).cloned().ok_or_else(|| CmdError::Params("No page".into()))?;
    note.image_blobs.push(key.clone());
    note.history.push(mailcraft_model::note::NoteRevision {
        modified: note.modified,
        text: note.text.clone(),
        blocks: note.blocks.clone(),
    });
    trim_history(&mut note);
    note.blocks = existing;
    if let Some(block) = target.and_then(|target| note.blocks.iter_mut().find(|b| b.id == target)) {
        block.blob = Some(key);
    } else {
        note.blocks.push(mailcraft_model::note::NoteBlock {
            id: next,
            kind: mailcraft_model::note::NoteBlockKind::Image,
            text: "Image".into(),
            blob: Some(key),
            checked: false,
            ..Default::default()
        });
    }
    note.text = block_text(&title(&note), &note.blocks);
    if note.text.len() > 1024 * 1024 {
        return Err(CmdError::Params("A page supports 1 MiB of text.".into()));
    }
    note.modified = s.now();
    s.mailbox.notes.insert(id, note);
    Ok(json!({"id":id.0}))
}

fn run_new(s: &mut Session, p: &Value) -> CmdResult {
    let folder = pim::new_item_folder(s, p, Kind::Note)?;
    let now = s.now();
    if p.get("template").is_some() && p.get("templateId").is_some() {
        return Err(CmdError::Params("Choose one template.".into()));
    }
    let mut n = if let Some(value) = p.get("templateId") {
        let id = value.as_u64().ok_or_else(|| CmdError::Params("Template id must be a number.".into()))?;
        let mut n = s
            .mailbox
            .notes
            .get(&ItemId(id))
            .filter(|n| template_available(s, n.id))
            .cloned()
            .ok_or_else(|| CmdError::Params("Template unavailable.".into()))?;
        n.folder = folder;
        n.created = now;
        n.modified = now;
        n.history.clear();
        n.parent = None;
        n.source_mail = None;
        n.pinned = false;
        n.is_template = false;
        for block in &mut n.blocks {
            for habit in &mut block.habits {
                habit.days.clear();
            }
        }
        n
    } else {
        Note { folder, created: now, modified: now, icon: "note".into(), ..Default::default() }
    };
    if let Some(template) = p.get("template") {
        let name = template.as_str().ok_or_else(|| CmdError::Params("Template name must be text.".into()))?;
        let info = TEMPLATES
            .iter()
            .find(|t| t.key == name)
            .ok_or_else(|| CmdError::Params("Unknown note template".into()))?;
        n.text = template_text(name).ok_or_else(|| CmdError::Params("Unknown note template".into()))?.into();
        n.markdown = true;
        n.icon = info.icon.into();
        n.color = info.color;
        n.paper = info.paper;
        n.content_width = if name == "weekly" {
            1200
        } else if name == "project" {
            960
        } else {
            720
        };
        n.cover = matches!(name, "daily" | "weekly" | "project");
        n.blocks = page_blocks(&n);
        n.blocks.retain(|b| b.kind != NoteBlockKind::Paragraph || !b.text.trim().is_empty());
        for b in &mut n.blocks {
            if b.kind != NoteBlockKind::Code {
                b.text = b.text.trim_matches('\n').into();
            }
            if b.text.contains('\t') {
                b.kind = NoteBlockKind::Table;
            }
        }
        let original = std::mem::take(&mut n.blocks);
        for (index, block) in original.iter().enumerate() {
            let mut block = block.clone();
            if index == 1
                && ["daily", "weekly", "project"].contains(&name)
                && block.kind == NoteBlockKind::Paragraph
            {
                block.kind = NoteBlockKind::Callout;
            }
            n.blocks.push(block.clone());
            if block.kind == NoteBlockKind::Heading
                && original
                    .get(index.saturating_add(1))
                    .is_none_or(|next| next.kind == NoteBlockKind::Heading)
            {
                n.blocks.push(NoteBlock::default());
            }
        }
        for (index, block) in n.blocks.iter_mut().enumerate() {
            block.id = u64::try_from(index.saturating_add(1)).unwrap_or(u64::MAX);
        }
    }
    if p.get("template").and_then(Value::as_str) == Some("weekly") {
        n.blocks = weekly_blocks();
        n.text = block_text("This week", &n.blocks);
    }
    if p.get("template").and_then(Value::as_str) == Some("habits") {
        n.blocks = vec![NoteBlock {
            id: 1,
            kind: NoteBlockKind::Habits,
            text: "Small steps, repeated".into(),
            habits: default_habits(),
            ..Default::default()
        }];
        n.text = block_text("Habit tracker", &n.blocks);
    }
    apply_note_fields(&mut n, p)?;
    let id = ItemId(s.mailbox.alloc_id());
    n.id = id;
    let mut out = note_json(&n, s.tz_offset_min);
    s.mailbox.notes.insert(id, n);
    pim::dialog(s, "note", json!({ "id": id.0 }))?;
    if let Some(o) = out.as_object_mut() {
        o.insert("dialog".into(), json!("note"));
    }
    Ok(out)
}

pub const PAGE_ICONS: [&str; 8] =
    ["note", "calendar", "task", "appointment", "folder", "person", "flag", "attach"];

pub struct NoteTemplate {
    pub key: &'static str,
    pub title: &'static str,
    pub description: &'static str,
    pub group: &'static str,
    pub icon: &'static str,
    pub color: NoteColor,
    pub paper: mailcraft_model::note::NotePaper,
}
use mailcraft_model::note::NotePaper;
pub const TEMPLATES: [NoteTemplate; 10] = [
    NoteTemplate {
        key: "daily",
        title: "Daily planner",
        description: "Choose your priorities, plan the day and close with a reflection.",
        group: "Planning",
        icon: "calendar",
        color: NoteColor::Blue,
        paper: NotePaper::Plain,
    },
    NoteTemplate {
        key: "weekly",
        title: "This week",
        description: "Live agendas, priorities, dated habits and tasks in two columns.",
        group: "Planning",
        icon: "calendar",
        color: NoteColor::Green,
        paper: NotePaper::Plain,
    },
    NoteTemplate {
        key: "monthly",
        title: "Monthly goals",
        description: "Turn a bigger goal into milestones, dates and a monthly review.",
        group: "Planning",
        icon: "flag",
        color: NoteColor::Pink,
        paper: NotePaper::Plain,
    },
    NoteTemplate {
        key: "habits",
        title: "Habit tracker",
        description: "A seven-day checkbox table for the routines you want to keep.",
        group: "Planning",
        icon: "task",
        color: NoteColor::Green,
        paper: NotePaper::Plain,
    },
    NoteTemplate {
        key: "meeting",
        title: "Meeting notes",
        description: "Bring the agenda, decisions and owners into one place.",
        group: "Work",
        icon: "appointment",
        color: NoteColor::Blue,
        paper: NotePaper::Plain,
    },
    NoteTemplate {
        key: "project",
        title: "Project brief",
        description: "Define the outcome, milestones, risks and the next concrete steps.",
        group: "Work",
        icon: "folder",
        color: NoteColor::Green,
        paper: NotePaper::Plain,
    },
    NoteTemplate {
        key: "decision",
        title: "Decision log",
        description: "Compare options and record the reason behind a decision.",
        group: "Work",
        icon: "flag",
        color: NoteColor::Yellow,
        paper: NotePaper::Plain,
    },
    NoteTemplate {
        key: "journal",
        title: "Journal",
        description: "A quiet page for a highlight, a thought and something to remember.",
        group: "Personal",
        icon: "note",
        color: NoteColor::Pink,
        paper: NotePaper::Ruled,
    },
    NoteTemplate {
        key: "reading",
        title: "Reading notes",
        description: "Keep the ideas, quotes and actions worth returning to.",
        group: "Personal",
        icon: "note",
        color: NoteColor::Yellow,
        paper: NotePaper::Plain,
    },
    NoteTemplate {
        key: "travel",
        title: "Trip planner",
        description: "An itinerary, booking notes and a packing checklist for your next trip.",
        group: "Personal",
        icon: "attach",
        color: NoteColor::Blue,
        paper: NotePaper::Dotted,
    },
];

pub fn template_text(name: &str) -> Option<&'static str> {
    Some(match name {
        "daily" => {
            "# Daily planner\n\n## Today's focus\nWhat would make today a good day?\n\n## Top three\n- [ ] First priority\n- [ ] Second priority\n- [ ] Third priority\n\n## Schedule\nTime\tPlan\nMorning\t\nAfternoon\t\nEvening\t\n\n## Notes & ideas\n\n## End-of-day reflection\nWhat worked? What can wait until tomorrow?"
        }
        "weekly" => {
            "# Weekly plan\n\n## This week's focus\nThe outcome that matters most.\n\n## Week at a glance\nDay\tFocus\nMonday\t\nTuesday\t\nWednesday\t\nThursday\t\nFriday\t\n\n## Priorities\n- [ ] One meaningful outcome\n- [ ] One thing to finish\n\n## Weekly review\nWhat moved forward? What needs a different approach?"
        }
        "monthly" => {
            "# Monthly goals\n\n## Theme of the month\nChoose a direction, not just a longer list.\n\n## Outcomes\n- [ ] Outcome one\n- [ ] Outcome two\n- [ ] Outcome three\n\n## Milestones\nMilestone\tTarget date\tProgress\nFirst milestone\t\tNot started\n\n## Important dates\n\n## Looking back\nWhat changed? What will you carry into next month?"
        }
        "habits" => {
            "# Habit tracker\n\n## Small steps, repeated\nChoose a few routines. Mark each day with a check when you complete it.\n\n## This week\nHabit\tMon\tTue\tWed\tThu\tFri\tSat\tSun\nMove your body\t\t\t\t\t\t\t\nRead a little\t\t\t\t\t\t\t\nTake a pause\t\t\t\t\t\t\t\n\n## Reflection\nWhat made it easier to show up?"
        }
        "meeting" => {
            "# Meeting notes\n\n## Details\nDate: \nAttendees: \n\n## Agenda\nWhat do we need to decide?\n\n## Discussion\n\n## Decisions\nWhat did we agree, and why?\n\n## Action items\n- [ ] Owner · Action · Due date\n"
        }
        "project" => {
            "# Project brief\n\n## Goal\nWhat are we making, and who is it for?\n\n## Success looks like\nHow will we know this worked?\n\n## Context\n\n## Milestones\n- [ ] Define the scope\n- [ ] Review the first draft\n- [ ] Deliver the result\n\n## Risks & open questions\n\n## Next steps\n- [ ] Choose the first concrete action\n"
        }
        "decision" => {
            "# Decision log\n\n## The question\nWhat needs to be decided?\n\n## Options\nOption\tBenefits\tTrade-offs\nOption A\t\t\nOption B\t\t\n\n## Decision\nWhat did we choose, and why?\n\n## Follow-up\n- [ ] Communicate the decision\n- [ ] Check the result later"
        }
        "journal" => {
            "# Journal\n\n## A moment worth keeping\n\n## On my mind\n\n## Something I appreciated\n\n## A small intention for tomorrow\n"
        }
        "reading" => {
            "# Reading notes\n\n## The book or article\nTitle: \nAuthor: \n\n## The central idea\nExplain it in your own words.\n\n## Ideas worth keeping\n\n## A passage to return to\n> Add a short quote and its page or source.\n\n## Put it into practice\n- [ ] One thing to try"
        }
        "travel" => {
            "# Trip planner\n\n## The destination\nWhere, when and who is coming?\n\n## Itinerary\nDay\tPlan\tBooking or address\nDay one\t\t\nDay two\t\t\n\n## Before you go\n- [ ] Confirm travel and accommodation\n- [ ] Save the essential addresses\n\n## Packing\n- [ ] Documents\n- [ ] Chargers\n- [ ] Something for the journey\n\n## Places to explore\n"
        }
        _ => return None,
    })
}

fn save_template(s: &mut Session, p: &Value) -> CmdResult {
    let id = pim::require_targets(s, p, Kind::Note)?
        .first()
        .copied()
        .ok_or_else(|| CmdError::Params("Choose a page.".into()))?;
    if s.mailbox.notes.values().filter(|n| template_available(s, n.id)).count() >= 100 {
        return Err(CmdError::Params("Keep at most 100 custom templates.".into()));
    }
    let mut n =
        s.mailbox.notes.get(&id).cloned().ok_or_else(|| CmdError::Params("Page unavailable.".into()))?;
    let name = pim::text(p, "name")?.unwrap_or_else(|| title(&n));
    let name = name.trim();
    if name.is_empty() || name.chars().count() > 120 || name.chars().any(char::is_control) {
        return Err(CmdError::Params("Use a template name of 1–120 characters.".into()));
    }
    n.text = if n.blocks.is_empty() {
        format!(
            "{}{}\n{}",
            if n.markdown { "# " } else { "" },
            name,
            n.text.lines().skip(1).collect::<Vec<_>>().join("\n")
        )
    } else {
        block_text(name, &n.blocks)
    };
    if n.text.len() > pim::MAX_TEXT {
        return Err(CmdError::Params("Template text is too long.".into()));
    }
    n.id = ItemId(s.mailbox.alloc_id());
    n.is_template = true;
    n.pinned = false;
    n.parent = None;
    n.source_mail = None;
    n.history.clear();
    n.created = s.now();
    n.modified = s.now();
    let id = n.id;
    s.mailbox.notes.insert(id, n);
    Ok(json!({"id":id.0}))
}

fn run_from_mail(s: &mut Session, p: &Value) -> CmdResult {
    let id = super::mail::primary_item(s, p)
        .ok_or_else(|| CmdError::Disabled("Select a message first.".into()))?;
    if crate::sync::cmds::is_header_only(s, id) {
        return Err(CmdError::Disabled("Download the full message before creating a note.".into()));
    }
    let mail = s.mailbox.mail.get(&id).ok_or_else(|| CmdError::Params("No source message".into()))?;
    let subject: String = mail.subject.chars().filter(|c| !c.is_control()).take(4096).collect();
    let sender = mail.from.as_ref().map(|from| from.to_display_string()).unwrap_or_default();
    let body = mail
        .body
        .text
        .clone()
        .or_else(|| mail.body.html.as_ref().map(|html| mailcraft_html::html_to_text(html)))
        .unwrap_or_else(|| mail.preview.clone());
    let truncated = body.chars().count() > 100_000;
    let body: String = body.chars().take(100_000).collect();
    let text = format!(
        "# {subject}\n\nFrom: {sender}\nDate: {}\n\n## Notes\n\n## Action items\n- [ ] \n\n## Original message\n{}",
        mail.received.to_rfc5322(),
        body.lines().map(|line| format!("> {line}\n")).collect::<String>()
    );
    let text = if truncated {
        format!(
            "{text}\n\nExcerpt limited to 100,000 characters. Open the source email for the complete message."
        )
    } else {
        text
    };
    let folder = p.get("folder").cloned().unwrap_or_else(|| {
        s.mailbox
            .folder(mail.folder)
            .and_then(|f| s.mailbox.role_folder(f.account, mailcraft_model::FolderRole::Notes))
            .map_or(Value::Null, |id| json!(id.0))
    });
    let mut result = run_new(s, &json!({"folder":folder,"text":text,"markdown":true}))?;
    if let Some(note_id) = result.get("id").and_then(Value::as_u64)
        && let Some(note) = s.mailbox.notes.get_mut(&ItemId(note_id))
    {
        note.source_mail = Some(id);
    }
    if let Some(object) = result.as_object_mut() {
        object.insert("sourceMail".into(), json!(id.0));
    }
    Ok(result)
}

fn open_source(s: &mut Session, p: &Value) -> CmdResult {
    let ids = pim::require_targets(s, p, Kind::Note)?;
    let source = ids
        .first()
        .and_then(|id| s.mailbox.notes.get(id))
        .and_then(|note| note.source_mail)
        .filter(|id| s.mailbox.mail.contains_key(id))
        .ok_or_else(|| CmdError::Disabled("The source email is no longer available.".into()))?;
    super::mail::open(s, &json!({"ids":[source.0]}))
}

fn run_update(s: &mut Session, p: &Value) -> CmdResult {
    let ids = pim::require_targets(s, p, Kind::Note)?;
    let now = s.now();
    let tz = s.tz_offset_min;
    let mut updated = Vec::new();
    for id in &ids {
        let Some(mut n) = s.mailbox.notes.get(id).cloned() else { continue };
        let old = n.clone();
        if apply_note_fields(&mut n, p)? {
            n.modified = now;
        }
        if n.text != old.text || n.blocks != old.blocks {
            n.history.push(mailcraft_model::note::NoteRevision {
                modified: old.modified,
                text: old.text,
                blocks: old.blocks,
            });
            trim_history(&mut n);
        }
        updated.push(n);
    }
    let out: Vec<Value> = updated.iter().map(|n| note_json(n, tz)).collect();
    for n in updated {
        s.mailbox.notes.insert(n.id, n);
    }
    Ok(json!({ "updated": out }))
}

fn reparent(s: &mut Session, p: &Value) -> CmdResult {
    let id = pim::require_targets(s, p, Kind::Note)?
        .first()
        .copied()
        .ok_or_else(|| CmdError::Params("No page".into()))?;
    let note = s
        .mailbox
        .notes
        .get(&id)
        .filter(|n| !n.is_template)
        .ok_or_else(|| CmdError::Params("No page".into()))?;
    let parent = match p.get("parent") {
        Some(Value::Null) => None,
        Some(value) => Some(ItemId(
            value.as_u64().ok_or_else(|| CmdError::Params("`parent` must be a page id or null".into()))?,
        )),
        None => return Err(CmdError::Params("`parent` required".into())),
    };
    let mut current = parent;
    let mut seen = std::collections::BTreeSet::new();
    while let Some(next) = current {
        if next == id || !seen.insert(next) || seen.len() > 12 {
            return Err(CmdError::Params("Page nesting must be acyclic and at most 12 levels.".into()));
        }
        let candidate = s
            .mailbox
            .notes
            .get(&next)
            .filter(|n| n.folder == note.folder && !n.is_template)
            .ok_or_else(|| CmdError::Params("Choose a parent in this notebook.".into()))?;
        current = candidate.parent;
    }
    for child in s.mailbox.notes.values().filter(|n| n.folder == note.folder && n.id != id) {
        let mut cursor = child.parent;
        let mut depth = 1usize;
        let mut visited = std::collections::BTreeSet::new();
        while let Some(next) = cursor {
            if next == id {
                if depth.saturating_add(seen.len()) > 12 {
                    return Err(CmdError::Params("Moving this subtree would exceed 12 levels.".into()));
                }
                break;
            }
            if !visited.insert(next) || depth > 12 {
                break;
            }
            cursor = s.mailbox.notes.get(&next).and_then(|n| n.parent);
            depth = depth.saturating_add(1);
        }
    }
    if let Some(note) = s.mailbox.notes.get_mut(&id) {
        note.parent = parent;
    }
    Ok(json!({"id":id.0,"parent":parent.map(|id|id.0)}))
}

fn restore_revision(s: &mut Session, p: &Value) -> CmdResult {
    let id = pim::require_targets(s, p, Kind::Note)?
        .first()
        .copied()
        .ok_or_else(|| CmdError::Params("No page".into()))?;
    let index = p
        .get("revision")
        .and_then(Value::as_u64)
        .and_then(|v| usize::try_from(v).ok())
        .ok_or_else(|| CmdError::Params("`revision` required".into()))?;
    let revision = s
        .mailbox
        .notes
        .get(&id)
        .and_then(|n| n.history.get(index))
        .cloned()
        .ok_or_else(|| CmdError::Params("Revision unavailable".into()))?;
    if revision.blocks.is_empty() {
        run_update(s, &json!({"id":id.0,"text":revision.text}))
    } else {
        run_update(s, &json!({"id":id.0,"text":revision.text,"blocks":revision.blocks}))
    }
}

fn run_delete(s: &mut Session, p: &Value) -> CmdResult {
    pim::run_delete(s, p, Kind::Note)
}
fn run_copy(s: &mut Session, p: &Value) -> CmdResult {
    if p.get("folder").is_none_or(Value::is_null) {
        return crate::cmd::tasks::move_dialog(s, p, Kind::Note, true);
    }
    pim::run_copy(s, p, Kind::Note)
}
fn run_move(s: &mut Session, p: &Value) -> CmdResult {
    if p.get("folder").is_none_or(Value::is_null) {
        return crate::cmd::tasks::move_dialog(s, p, Kind::Note, false);
    }
    pim::run_move(s, p, Kind::Note)
}
fn run_all_categories(s: &mut Session, p: &Value) -> CmdResult {
    crate::cmd::tasks::categories_dialog(s, p, Kind::Note)
}

/// Forward: a new message with the note's title as the subject and its text as the body (one message for
/// all the selected notes).
fn run_forward(s: &mut Session, p: &Value) -> CmdResult {
    let ids = pim::require_targets(s, p, Kind::Note)?;
    let notes: Vec<Note> = ids.iter().filter_map(|i| s.mailbox.notes.get(i)).cloned().collect();
    let subject = match notes.as_slice() {
        [one] => title(one),
        [first, rest @ ..] => format!("{} and {} more", title(first), rest.len()),
        [] => String::new(),
    };
    let body = notes.iter().map(|n| n.text.trim_end().to_owned()).collect::<Vec<_>>().join("\n\n");
    let mut compose = crate::compose::ComposeState {
        subject: mailcraft_model::subject::forward_subject(&subject),
        body: mailcraft_model::Doc::from_plain_text(&body),
        dirty: true,
        ..Default::default()
    };
    crate::compose::insert_default_signature(&s.mailbox, &mut compose);
    let cid = s.open_compose(compose);
    Ok(json!({ "compose": cid, "notes": notes.len() }))
}

fn note_selected(s: &Session) -> Option<&'static str> {
    if pim::selected(s, Kind::Note).is_empty() { Some("select a note first") } else { None }
}

/// Rows of the selected Notes view of `folder`: Icon in grid order (oldest first), Notes List newest
/// first, Last 7 Days the notes changed in the last seven days, newest first. Reverse Sort reverses.
pub fn view_rows(s: &Session, folder: FolderId) -> Vec<ViewRow> {
    let st = modules::state(s, folder, ModuleKind::Notes);
    let since = s.now().unix.saturating_sub(7 * pim::DAY_SECS);
    let mut v: Vec<(i64, ItemId)> = s
        .mailbox
        .notes
        .values()
        .filter(|n| n.folder == folder && !n.is_template)
        .filter(|n| st.view != "Last 7 Days" || n.modified.unix >= since)
        .map(|n| (n.created.unix, n.id))
        .collect();
    v.sort();
    if st.view != "Icon" {
        v.reverse();
    }
    let mut out: Vec<ViewRow> =
        v.into_iter().map(|(_, id)| ViewRow { id, kind: RowKind::Note, group: None }).collect();
    if modules::reversed(s, folder) {
        out.reverse();
    }
    out.sort_by_key(|row| !s.mailbox.notes.get(&row.id).is_some_and(|note| note.pinned));
    out
}
fn run_categorize(s: &mut Session, p: &Value) -> CmdResult {
    pim::run_categorize(s, p, Kind::Note)
}

fn run_icon_view(s: &mut Session, p: &Value) -> CmdResult {
    let mode = match opt_str(p, "mode") {
        Some("large") => IconMode::Large,
        Some("small") => IconMode::Small,
        Some("list") => IconMode::List,
        _ => return Err(CmdError::Params("`mode` must be large|small|list".into())),
    };
    let mut v = view_param(s, p)?;
    v.icon_mode = mode;
    store_view(s, &v);
    Ok(json!({ "view": v }))
}

/// Notes of the current notes folder in grid order (oldest first, then id).
pub fn grid_order(s: &Session) -> Vec<ItemId> {
    let Some(folder) = pim::current_folder(s, Kind::Note) else {
        return Vec::new();
    };
    let mut v: Vec<(i64, ItemId)> = s
        .mailbox
        .notes
        .values()
        .filter(|n| n.folder == folder && !n.is_template)
        .map(|n| (n.created.unix, n.id))
        .collect();
    v.sort();
    v.into_iter().map(|(_, id)| id).collect()
}

fn notes_present(s: &Session) -> Option<&'static str> {
    if grid_order(s).is_empty() { Some("no notes to line up") } else { None }
}

fn run_line_up(s: &mut Session, p: &Value) -> CmdResult {
    let mut v = view_param(s, p)?;
    if v.icon_mode == IconMode::List {
        return Err(CmdError::Disabled("not available in the list arrangement".into()));
    }
    v.lined_up = true;
    store_view(s, &v);
    if let Some(f) = pim::current_folder(s, Kind::Note) {
        store_positions(s, f, &BTreeMap::new());
    }
    Ok(json!({ "view": v, "order": pim::ids_json(&grid_order(s)) }))
}

pub fn specs() -> Vec<CommandSpec> {
    let spec = |id, label, location, params, mutates, enabled, run| CommandSpec {
        id,
        label,
        location,
        shortcut: "",
        params,
        mutates,
        enabled,
        run,
    };
    vec![
        spec(
            "notes.openReference",
            "Open page reference",
            "Notes › References",
            r#"{"kind":"page|person|appointment|task","id":number}"#,
            false,
            always,
            open_reference,
        ),
        spec(
            "notes.workspace",
            "Read live page data",
            "Notes › Workspace",
            r#"{"id"?:number,"day":number}"#,
            false,
            always,
            workspace_query,
        ),
        spec(
            "notes.saveTemplate",
            "Save page as template",
            "Notes › Templates",
            r#"{"id":number,"name"?:string}"#,
            true,
            always,
            save_template,
        ),
        spec(
            "notes.attachImage",
            "Add page image",
            "Notes › Page",
            r#"{"id":number,"path":string}"#,
            true,
            always,
            attach_image,
        ),
        spec(
            "notes.reparent",
            "Move page under page",
            "Notes › Page",
            r#"{"id":number,"parent":number|null}"#,
            true,
            always,
            reparent,
        ),
        spec(
            "notes.restoreRevision",
            "Restore page revision",
            "Notes › History",
            r#"{"id":number,"revision":number}"#,
            true,
            always,
            restore_revision,
        ),
        spec(
            "notes.fromMail",
            "Create note from email",
            "Message context menu",
            r#"{"ids"?:[number],"folder"?:number}"#,
            true,
            always,
            run_from_mail,
        ),
        spec(
            "notes.openSource",
            "Open source email",
            "Notes › Item",
            r#"{"id":number}"#,
            false,
            always,
            open_source,
        ),
        spec(
            "notes.iconView",
            "Icon Arrangement",
            "Notes › View › Arrangement",
            r#"{"mode": "large|small|list", "view"?: NotesViewState}"#,
            true,
            always,
            run_icon_view,
        ),
        spec(
            "notes.lineUpIcons",
            "Line Up Icons",
            "Notes › View › Arrangement",
            r#"{"view"?: NotesViewState}"#,
            true,
            notes_present,
            run_line_up,
        ),
        spec(
            "notes.setPosition",
            "Move Icon",
            "Notes › Icon view",
            r#"{"id": number, "x": number, "y": number} | {"id": number, "position": null} (back to the grid)"#,
            true,
            always,
            run_set_position,
        ),
        spec(
            "notes.new",
            "New Note",
            "Notes › Home › New",
            r#"{"text"?: string, "template"?: string, "templateId"?:number, "markdown"?:bool, "pinned"?:bool, "color"?: "blue|green|pink|yellow|white", "categories"?: [string], "folder"?: number}"#,
            true,
            always,
            run_new,
        ),
        spec(
            "notes.update",
            "Edit Note",
            "Notes › Item",
            r#"{"id": number | "ids": [number], "text"?, "paper"?:"plain|ruled|dotted", "icon"?:string, "blocks"?:[object], "autosave"?:bool, "markdown"?:bool, "pinned"?:bool, "color"?, "categories"?}"#,
            true,
            always,
            run_update,
        ),
        spec(
            "notes.delete",
            "Delete",
            "Notes › Home › Delete",
            r#"{"id"?: number, "ids"?: [number]}"#,
            true,
            always,
            run_delete,
        ),
        spec(
            "notes.copy",
            "Copy to Folder",
            "Notes › Item",
            r#"{"folder": number, "id"?: number, "ids"?: [number]}"#,
            true,
            always,
            run_copy,
        ),
        spec(
            "notes.move",
            "Move to Folder",
            "Notes › Home › Actions › Move",
            r#"{"folder": number, "id"?: number, "ids"?: [number]}"#,
            true,
            always,
            run_move,
        ),
        spec(
            "notes.categorize",
            "Categorize",
            "Notes › Home › Tags",
            r#"{"name": string, "on"?: bool, "clear"?: bool, "id"?: number, "ids"?: [number]}"#,
            true,
            always,
            run_categorize,
        ),
        spec(
            "notes.allCategories",
            "All Categories",
            "Notes › Home › Tags › Categorize",
            r#"{"ids"?: [number]}"#,
            false,
            note_selected,
            run_all_categories,
        ),
        spec(
            "notes.forward",
            "Forward",
            "Notes › Home › Actions",
            r#"{"ids"?: [number]}"#,
            false,
            note_selected,
            run_forward,
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cmd::calendar::tests::{role, session};
    use crate::{Notice, Selection};
    use mailcraft_model::FolderRole;

    #[test]
    fn inline_styles_layout_and_covers_survive_templates_undo_and_validation() {
        let mut s = crate::sample::sample_session();
        let id =
            s.run("notes.new", &json!({"text":"Writing","markdown":true})).unwrap()["id"].as_u64().unwrap();
        let styled = json!([{"id":1,"kind":"paragraph","text":"Café launch","inline":[{"start":0,"end":4,"bold":true},{"start":5,"end":11,"link":"https://example.net"}]}]);
        s.run("notes.update", &json!({"id":id,"blocks":styled,"cover":true,"contentWidth":960})).unwrap();
        let expected = s.mailbox.notes[&ItemId(id)].clone();
        assert_eq!(expected.blocks[0].inline.len(), 2);
        let decoded: Note = serde_json::from_slice(&serde_json::to_vec(&expected).unwrap()).unwrap();
        assert_eq!(decoded, expected);
        let template = s.run("notes.saveTemplate", &json!({"id":id,"name":"Styled writing"})).unwrap()["id"]
            .as_u64()
            .unwrap();
        let reused = s.run("notes.new", &json!({"templateId":template})).unwrap()["id"].as_u64().unwrap();
        assert_eq!(s.mailbox.notes[&ItemId(reused)].blocks, expected.blocks);
        assert!(s.mailbox.notes[&ItemId(reused)].cover);
        assert_eq!(s.mailbox.notes[&ItemId(reused)].content_width, 960);
        for patch in [
            json!({"contentWidth":999}),
            json!({"cover":"yes"}),
            json!({"blocks":[{"id":1,"kind":"paragraph","text":"é","inline":[{"start":0,"end":2,"bold":true}]}]}),
            json!({"blocks":[{"id":1,"kind":"paragraph","text":"Link","inline":[{"start":0,"end":4,"link":"javascript:alert(1)"}]}]}),
        ] {
            let mut patch = patch;
            patch["id"] = json!(id);
            assert!(s.run("notes.update", &patch).is_err());
            assert_eq!(s.mailbox.notes[&ItemId(id)], expected);
        }
        s.run("notes.update", &json!({"id":id,"blocks":[{"id":1,"kind":"paragraph","text":"Revised"}]}))
            .unwrap();
        s.run("app.undo", &json!({})).unwrap();
        assert_eq!(s.mailbox.notes[&ItemId(id)], expected);
    }

    #[test]
    fn template_catalog_custom_reuse_and_page_appearance() {
        let mut s = crate::sample::sample_session();
        for template in TEMPLATES {
            let result = s.run("notes.new", &json!({"template":template.key})).unwrap();
            let id = ItemId(result["id"].as_u64().unwrap());
            let n = &s.mailbox.notes[&id];
            assert_eq!(title(n), template.title);
            assert_eq!(n.icon, template.icon);
            assert_eq!(n.paper, template.paper);
            assert!(!n.blocks.is_empty());
        }
        let result = s.run("notes.new", &json!({"template":"habits"})).unwrap();
        let id = ItemId(result["id"].as_u64().unwrap());
        assert!(
            s.mailbox.notes[&id]
                .blocks
                .iter()
                .any(|b| b.kind == NoteBlockKind::Habits && b.habits.len() == 3)
        );
        s.run("notes.update", &json!({"id":id.0,"paper":"dotted","icon":"calendar"})).unwrap();
        let before = s.mailbox.notes[&id].clone();
        for invalid in [json!({"id":id.0,"paper":"invalid"}), json!({"id":id.0,"icon":"../private"})] {
            assert!(s.run("notes.update", &invalid).is_err());
            assert_eq!(s.mailbox.notes[&id], before);
        }
        let saved = s.run("notes.saveTemplate", &json!({"id":id.0,"name":"My weekly habits"})).unwrap();
        let template_id = ItemId(saved["id"].as_u64().unwrap());
        assert_eq!(s.mailbox.notes[&id], before);
        assert!(s.mailbox.notes[&template_id].is_template);
        assert!(s.run("notes.reparent", &json!({"id":id.0,"parent":template_id.0})).is_err());
        assert!(!view_rows(&s, before.folder).iter().any(|row| row.id == template_id));
        let reused = s.run("notes.new", &json!({"templateId":template_id.0})).unwrap();
        let reused = ItemId(reused["id"].as_u64().unwrap());
        let n = &s.mailbox.notes[&reused];
        assert!(!n.is_template && n.history.is_empty() && n.parent.is_none());
        assert_eq!(n.paper, NotePaper::Dotted);
        assert_eq!(n.blocks, before.blocks);
        assert_eq!(title(n), "My weekly habits");
        let roundtrip: Note = serde_json::from_slice(&serde_json::to_vec(n).unwrap()).unwrap();
        assert_eq!(&roundtrip, n);
        assert!(s.run("notes.new", &json!({"templateId":id.0})).is_err());
        assert!(s.run("notes.saveTemplate", &json!({"id":id.0,"name":"bad\nname"})).is_err());
        s.run("notes.delete", &json!({"id":template_id.0})).unwrap();
        assert!(!template_available(&s, template_id));
        assert!(s.run("notes.new", &json!({"templateId":template_id.0})).is_err());
        s.run("app.undo", &json!({})).unwrap();
        assert!(template_available(&s, template_id));
    }

    #[test]
    fn workspace_blocks_history_and_parent_cycles() {
        let mut s = crate::sample::sample_session();
        let a = s.run("notes.new", &json!({"text":"Page","markdown":true})).unwrap()["id"].as_u64().unwrap();
        let folder = s.mailbox.notes[&ItemId(a)].folder;
        let b =
            s.run("notes.new", &json!({"folder":folder.0,"text":"Child"})).unwrap()["id"].as_u64().unwrap();
        s.run("notes.reparent", &json!({"id":b,"parent":a})).unwrap();
        assert!(s.run("notes.reparent", &json!({"id":a,"parent":b})).is_err());
        assert!(s.run("notes.reparent", &json!({"id":a,"parent":a})).is_err());
        let blocks = vec![NoteBlock {
            id: 1,
            kind: NoteBlockKind::Checklist,
            text: "Review changes".into(),
            checked: true,
            ..Default::default()
        }];
        s.run("notes.update", &json!({"id":a,"blocks":blocks})).unwrap();
        assert!(s.mailbox.notes[&ItemId(a)].text.contains("[x] Review changes"));
        assert_eq!(s.mailbox.notes[&ItemId(a)].history.len(), 1);
        let before = s.mailbox.notes[&ItemId(a)].clone();
        assert!(
            s.run("notes.update", &json!({"id":a,"blocks":[blocks[0].clone(),blocks[0].clone()]})).is_err()
        );
        assert_eq!(s.mailbox.notes[&ItemId(a)], before);
        s.run("notes.restoreRevision", &json!({"id":a,"revision":0})).unwrap();
        assert_eq!(s.mailbox.notes[&ItemId(a)].text, "Page");
        let restored: Note =
            serde_json::from_slice(&serde_json::to_vec(&s.mailbox.notes[&ItemId(b)]).unwrap()).unwrap();
        assert_eq!(restored.parent, Some(ItemId(a)));
        assert!(decode_page_image(b"invalid image").is_err());
    }

    #[test]
    fn adjacent_autosaves_undo_as_one_edit() {
        let mut s = crate::sample::sample_session();
        let id = s.run("notes.new", &json!({"text":"Original"})).unwrap()["id"].as_u64().unwrap();
        s.run("notes.update", &json!({"id":id,"text":"First","autosave":true})).unwrap();
        s.run("notes.update", &json!({"id":id,"text":"Second","autosave":true})).unwrap();
        s.run("app.undo", &json!({})).unwrap();
        assert_eq!(s.mailbox.notes[&ItemId(id)].text, "Original");
        s.run("app.redo", &json!({})).unwrap();
        assert_eq!(s.mailbox.notes[&ItemId(id)].text, "Second");
    }

    #[test]
    fn page_images_preserve_legacy_text_and_survive_reload_and_undo() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = crate::sample::sample_session();
        let mut store = mailcraft_store::Store::open(dir.path()).unwrap();
        store.checkpoint(&s.mailbox).unwrap();
        s.store = Some(store);
        let id = s.run("notes.new", &json!({"text":"Title\nKeep this paragraph"})).unwrap()["id"]
            .as_u64()
            .unwrap();
        let path = dir.path().join("image.png");
        image::RgbaImage::from_pixel(2, 2, image::Rgba([20, 30, 40, 255])).save(&path).unwrap();
        s.run("notes.attachImage", &json!({"id":id,"path":path})).unwrap();
        let note = s.mailbox.notes[&ItemId(id)].clone();
        assert!(note.text.contains("Keep this paragraph"));
        assert_eq!(note.blocks.last().unwrap().kind, NoteBlockKind::Image);
        let key = note.image_blobs.first().unwrap().clone();
        assert!(s.store.as_ref().unwrap().get_blob(&key).is_ok());
        s.run("app.undo", &json!({})).unwrap();
        assert!(s.mailbox.notes[&ItemId(id)].image_blobs.is_empty());
        s.run("app.redo", &json!({})).unwrap();
        assert_eq!(s.mailbox.notes[&ItemId(id)], note);
        let mailbox = s.mailbox.clone();
        s.store.as_mut().unwrap().gc_blobs(&mailbox).unwrap();
        assert!(s.store.as_ref().unwrap().get_blob(&key).is_ok());
        assert_eq!(s.store.as_mut().unwrap().load().unwrap().notes[&ItemId(id)], note);
        let saved = s.run("notes.saveTemplate", &json!({"id":id,"name":"Image layout"})).unwrap()["id"]
            .as_u64()
            .unwrap();
        s.run("notes.delete", &json!({"id":id})).unwrap();
        let mailbox = s.mailbox.clone();
        s.store.as_mut().unwrap().gc_blobs(&mailbox).unwrap();
        assert!(s.store.as_ref().unwrap().get_blob(&key).is_ok());
        let reused = s.run("notes.new", &json!({"templateId":saved})).unwrap()["id"].as_u64().unwrap();
        assert_eq!(s.mailbox.notes[&ItemId(reused)].image_blobs, vec![key]);
        assert!(s.mailbox.notes[&ItemId(reused)].blocks.iter().any(|b| b.kind == NoteBlockKind::Image));
        let block = s.mailbox.notes[&ItemId(reused)]
            .blocks
            .iter()
            .find(|b| b.kind == NoteBlockKind::Image)
            .unwrap()
            .id;
        let count = s.mailbox.notes[&ItemId(reused)].blocks.len();
        s.run("notes.attachImage", &json!({"id":reused,"block":block,"path":path})).unwrap();
        assert_eq!(s.mailbox.notes[&ItemId(reused)].blocks.len(), count);
        assert!(
            s.mailbox.notes[&ItemId(reused)].blocks.iter().find(|b| b.id == block).unwrap().blob.is_some()
        );
        assert!(s.run("notes.attachImage", &json!({"id":reused,"block":u64::MAX,"path":path})).is_err());
    }

    #[test]
    fn templates_metadata_legacy_notes_and_pinned_order() {
        let mut s = crate::sample::sample_session();
        let result = s.run("notes.new", &json!({"template":"meeting","pinned":true})).unwrap();
        let id = ItemId(result["id"].as_u64().unwrap());
        let note = &s.mailbox.notes[&id];
        assert!(note.markdown && note.pinned);
        assert_eq!(title(note), "Meeting notes");
        assert!(note.text.contains("## Decisions") && note.text.contains("- [ ]"));
        let folder = note.folder;
        assert_eq!(view_rows(&s, folder).first().unwrap().id, id);
        let mut legacy = serde_json::to_value(note).unwrap();
        for field in ["markdown", "pinned", "source_mail"] {
            legacy.as_object_mut().unwrap().remove(field);
        }
        let restored: Note = serde_json::from_value(legacy).unwrap();
        assert!(!restored.markdown && !restored.pinned && restored.source_mail.is_none());
        let before = s.mailbox.notes.clone();
        for bad in [json!({"template":"missing"}), json!({"pinned":"yes"}), json!({"markdown":12})] {
            assert!(s.run("notes.new", &bad).is_err());
            assert_eq!(s.mailbox.notes, before);
        }
    }

    #[test]
    fn email_note_copies_content_and_keeps_source_across_undo_and_serialization() {
        let mut s = crate::sample::sample_session();
        let mail =
            s.mailbox.mail.values().find(|mail| !mail.is_draft && mail.body.text.is_some()).unwrap().clone();
        let result = s.run("notes.fromMail", &json!({"ids":[mail.id.0]})).unwrap();
        let id = ItemId(result["id"].as_u64().unwrap());
        let note = s.mailbox.notes[&id].clone();
        assert_eq!(note.source_mail, Some(mail.id));
        assert_eq!(result["sourceMail"], mail.id.0);
        assert!(note.markdown && note.text.contains(&mail.subject));
        assert!(note.text.contains("## Original message") && note.text.contains("## Action items"));
        let restored: Note = serde_json::from_slice(&serde_json::to_vec(&note).unwrap()).unwrap();
        assert_eq!(restored, note);
        assert!(s.run("notes.openSource", &json!({"id":id.0})).is_ok());
        s.run("app.undo", &json!({})).unwrap();
        assert!(!s.mailbox.notes.contains_key(&id));
        s.run("app.redo", &json!({})).unwrap();
        assert_eq!(s.mailbox.notes[&id], note);
        s.mailbox.mail.remove(&mail.id);
        assert_eq!(s.mailbox.notes[&id].text, note.text);
        assert!(s.run("notes.openSource", &json!({"id":id.0})).is_err());
    }

    #[test]
    fn new_note_creates_and_opens() {
        let mut s = session();
        let r = s.run("notes.new", &json!({"text": "Groceries\nmilk", "color": "blue"})).unwrap();
        let id = ItemId(r["id"].as_u64().unwrap());
        let n = &s.mailbox.notes[&id];
        assert_eq!(n.folder, role(&s, FolderRole::Notes));
        assert_eq!(n.color, NoteColor::Blue);
        assert_eq!(r["title"], "Groceries");
        assert_eq!(r["dialog"], "note");
        assert!(
            matches!(s.notices.last(), Some(Notice::OpenDialog(name, p)) if name == "note" && p["id"] == id.0)
        );
        let r = s.run("notes.new", &json!({})).unwrap();
        assert_eq!(s.mailbox.notes[&ItemId(r["id"].as_u64().unwrap())].color, NoteColor::Yellow);
        assert!(matches!(s.run("notes.new", &json!({"color": "red"})), Err(CmdError::Params(_))));
        assert!(s.mailbox.validate().is_empty());
    }

    #[test]
    fn update_sets_modified() {
        let mut s = session();
        let id = s.run("notes.new", &json!({"text": "a"})).unwrap()["id"].as_u64().unwrap();
        s.fixed_now = s.fixed_now.map(|t| pim::add_secs(t, 3600));
        let r = s.run("notes.update", &json!({"id": id, "text": "b"})).unwrap();
        assert_eq!(r["updated"][0]["modified"], "2026-10-09T11:07");
        assert_eq!(r["updated"][0]["created"], "2026-10-09T10:07");
    }

    #[test]
    fn icon_view_and_line_up() {
        let mut s = session();
        let r = s.run("notes.iconView", &json!({"mode": "small"})).unwrap();
        assert_eq!(r["view"]["iconMode"], "small");
        assert!(matches!(s.run("notes.iconView", &json!({"mode": "huge"})), Err(CmdError::Params(_))));
        s.selection = Selection { folder: role(&s, FolderRole::Notes), items: vec![], focus: None };
        assert!(matches!(s.run("notes.lineUpIcons", &json!({})), Err(CmdError::Disabled(_))));
        let a = s.run("notes.new", &json!({"text": "a"})).unwrap()["id"].as_u64().unwrap();
        let r = s.run("notes.lineUpIcons", &json!({"view": {"iconMode": "large"}})).unwrap();
        assert_eq!(r["order"], json!([a]));
        assert_eq!(r["view"]["linedUp"], true);
        assert!(matches!(
            s.run("notes.lineUpIcons", &json!({"view": {"iconMode": "list"}})),
            Err(CmdError::Disabled(_))
        ));
    }

    #[test]
    fn forward_note_and_dialogs() {
        let mut s = session();
        let id =
            s.run("notes.new", &json!({"text": "Groceries\nmilk\neggs"})).unwrap()["id"].as_u64().unwrap();
        s.selection = Selection {
            folder: role(&s, FolderRole::Notes),
            items: vec![ItemId(id)],
            focus: Some(ItemId(id)),
        };
        let r = s.run("notes.forward", &json!({})).unwrap();
        let c = &s.composes[&r["compose"].as_u64().unwrap()];
        assert_eq!(c.subject, "FW: Groceries");
        assert!(c.body.to_plain_text().contains("eggs"));
        assert_eq!(s.run("notes.move", &json!({})).unwrap()["dialog"], "pimMove");
        assert_eq!(s.run("notes.allCategories", &json!({})).unwrap()["dialog"], "pimCategories");
        s.selection.items.clear();
        s.selection.focus = None;
        assert!(matches!(s.run("notes.forward", &json!({})), Err(CmdError::Disabled(_))));
    }

    #[test]
    fn delete_copy_move_categorize_notes() {
        let mut s = session();
        let id = s.run("notes.new", &json!({"text": "a"})).unwrap()["id"].as_u64().unwrap();
        let root = role(&s, FolderRole::Notes);
        let sub = s.mailbox.create_folder(root, "Ideas").unwrap();
        s.run("notes.copy", &json!({"id": id, "folder": sub.0})).unwrap();
        s.run("notes.move", &json!({"id": id, "folder": sub.0})).unwrap();
        assert_eq!(s.mailbox.notes[&ItemId(id)].folder, sub);
        s.run("notes.categorize", &json!({"id": id, "name": "Yellow category"})).unwrap();
        s.run("notes.categorize", &json!({"id": id, "clear": true})).unwrap();
        assert!(s.mailbox.notes[&ItemId(id)].categories.is_empty());
        s.run("notes.delete", &json!({"id": id})).unwrap();
        assert_eq!(s.mailbox.notes[&ItemId(id)].folder, role(&s, FolderRole::DeletedItems));
        assert!(matches!(s.run("notes.delete", &json!({"id": 424_242})), Err(CmdError::Params(_))));
    }

    #[test]
    fn dragged_icon_positions_are_kept() {
        let mut s = session();
        let folder = role(&s, FolderRole::Notes);
        s.selection = Selection { folder, items: vec![], focus: None };
        let a = s.run("notes.new", &json!({"text": "A"})).unwrap()["id"].as_u64().unwrap();
        let b = s.run("notes.new", &json!({"text": "B"})).unwrap()["id"].as_u64().unwrap();
        s.run("notes.lineUpIcons", &json!({})).unwrap();
        assert!(stored_view(&s).lined_up);
        s.run("notes.setPosition", &json!({"id": a, "x": 420.5, "y": 30})).unwrap();
        s.run("notes.setPosition", &json!({"id": b, "x": 0, "y": 900})).unwrap();
        assert_eq!(positions(&s, folder).get(&a), Some(&(420.5, 30.0)));
        assert!(!stored_view(&s).lined_up, "dragging breaks the lined-up grid");
        // The positions are not group labels and survive a settings round trip of the view state.
        assert!(s.view.collapsed_groups[&folder].iter().all(|e| !crate::cmd::view::is_group_label(e)));
        let view: crate::ViewState = serde_json::from_value(serde_json::to_value(&s.view).unwrap()).unwrap();
        s.view = view;
        assert_eq!(positions(&s, folder).len(), 2);
        // Back to the grid one at a time, or all with Line Up Icons.
        s.run("notes.setPosition", &json!({"id": b, "position": null})).unwrap();
        assert_eq!(positions(&s, folder).keys().copied().collect::<Vec<_>>(), vec![a]);
        s.run("notes.lineUpIcons", &json!({})).unwrap();
        assert!(positions(&s, folder).is_empty());
        // Hostile values.
        for bad in [
            json!({"id": a, "x": -1, "y": 0}),
            json!({"id": a, "x": 1e30, "y": 0}),
            json!({"id": a, "x": "1", "y": 0}),
            json!({"id": a}),
            json!({"id": u64::MAX, "x": 1, "y": 1}),
        ] {
            assert!(s.run("notes.setPosition", &bad).is_err(), "{bad}");
        }
        // A deleted note's position is dropped.
        s.run("notes.setPosition", &json!({"id": a, "x": 5, "y": 5})).unwrap();
        s.run("notes.delete", &json!({"id": a})).unwrap();
        assert!(positions(&s, folder).is_empty());
        // Damaged stored text reads as no positions.
        s.view.collapsed_groups.insert(folder, vec![format!("{NOTE_POS}{{not json")]);
        assert!(positions(&s, folder).is_empty());
    }
}

/// The weekly benchmark uses live organizer data rather than embedded sample rows.
pub fn default_habits() -> Vec<mailcraft_model::note::NoteHabit> {
    ["Move your body", "Read a little", "Take a pause"]
        .into_iter()
        .map(|name| mailcraft_model::note::NoteHabit { name: name.into(), ..Default::default() })
        .collect()
}
pub fn weekly_blocks() -> Vec<NoteBlock> {
    let definitions = [
        (NoteBlockKind::Agenda, "Today", 1, 0, 0),
        (NoteBlockKind::Agenda, "Tomorrow", 1, 0, 1),
        (NoteBlockKind::Heading, "Top three", 1, 1, 0),
        (NoteBlockKind::Checklist, "First priority", 1, 1, 0),
        (NoteBlockKind::Checklist, "Second priority", 1, 1, 0),
        (NoteBlockKind::Checklist, "Third priority", 1, 1, 0),
        (NoteBlockKind::Habits, "Habits", 1, 1, 0),
        (NoteBlockKind::Tasks, "Due soon", 0, 0, 7),
        (NoteBlockKind::Paragraph, "", 0, 0, 0),
    ];
    definitions
        .into_iter()
        .enumerate()
        .map(|(i, (kind, text, section, column, day_offset))| {
            let habits = if kind == NoteBlockKind::Habits { default_habits() } else { Vec::new() };
            NoteBlock {
                id: u64::try_from(i).unwrap_or(0).saturating_add(1),
                kind,
                text: text.into(),
                section,
                column,
                day_offset,
                habits,
                ..Default::default()
            }
        })
        .collect()
}

/// Backlinks follow stable page IDs and survive renaming the destination.
pub fn backlinks(s: &Session, target: ItemId) -> Vec<(ItemId, String)> {
    s.mailbox
        .notes
        .values()
        .filter(|n| {
            reference_available(s, "page", n.id)
                && n.id != target
                && n.blocks
                    .iter()
                    .any(|b| b.references.contains(&mailcraft_model::note::NoteReference::Page(target)))
        })
        .map(|n| (n.id, title(n)))
        .collect()
}
/// Tags are searchable across explicit categories and inline #tags.
pub fn page_tags(n: &Note) -> std::collections::BTreeSet<String> {
    let mut tags: std::collections::BTreeSet<String> =
        n.categories.iter().map(|c| c.to_lowercase()).collect();
    for word in n.text.split_whitespace() {
        if let Some(tag) = word.strip_prefix('#') {
            let tag: String = tag
                .chars()
                .take_while(|c| c.is_alphanumeric() || matches!(c, '_' | '-' | '/'))
                .take(80)
                .collect();
            if !tag.is_empty() {
                tags.insert(tag.to_lowercase());
            }
        }
    }
    tags
}

fn open_reference(s: &mut Session, p: &Value) -> CmdResult {
    let id = ItemId(
        p.get("id")
            .and_then(Value::as_u64)
            .ok_or_else(|| CmdError::Params("Reference id required.".into()))?,
    );
    let kind = p.get("kind").and_then(Value::as_str).unwrap_or("");
    if !reference_available(s, kind, id) {
        return Err(CmdError::Params("Referenced item is unavailable.".into()));
    }
    let dialog = match kind {
        "page" => "note",
        "person" => "contact",
        "appointment" => "appointment",
        "task" => "task",
        _ => return Err(CmdError::Params("Unknown reference kind.".into())),
    };
    pim::dialog(s, dialog, json!({"id":id.0}))
}
/// Templates in Deleted Items are hidden and cannot create new pages.
pub fn template_available(s: &Session, id: ItemId) -> bool {
    s.mailbox.notes.get(&id).is_some_and(|n| n.is_template && active_folder(s, n.folder))
}
/// Deleted and missing items remain as references but cannot be opened.
pub fn reference_available(s: &Session, kind: &str, id: ItemId) -> bool {
    let folder = match kind {
        "page" => s.mailbox.notes.get(&id).filter(|n| !n.is_template).map(|n| n.folder),
        "person" => s.mailbox.contacts.get(&id).map(|n| n.folder),
        "appointment" => s.mailbox.appointments.get(&id).map(|n| n.folder),
        "task" => s.mailbox.tasks.get(&id).map(|n| n.folder),
        _ => None,
    };
    folder.is_some_and(|f| active_folder(s, f))
}
fn active_folder(s: &Session, f: FolderId) -> bool {
    s.mailbox.folder(f).is_some()
        && !s
            .mailbox
            .folder_account(f)
            .and_then(|a| s.mailbox.role_folder(a, mailcraft_model::FolderRole::DeletedItems))
            .is_some_and(|d| s.mailbox.is_in_subtree(f, d))
}
/// Local day agenda, including recurrence and overlapping all-day appointments.
pub fn agenda(s: &Session, day: i64) -> Vec<(ItemId, String, mailcraft_model::DateTime, bool)> {
    if day.unsigned_abs() > 3_000_000 {
        return Vec::new();
    }
    let start = day.saturating_mul(86400).saturating_sub(i64::from(s.tz_offset_min).saturating_mul(60));
    let from = mailcraft_model::DateTime { unix: start, offset_min: s.tz_offset_min };
    let to = mailcraft_model::DateTime { unix: start.saturating_add(86400), ..from };
    let mut rows = Vec::new();
    for a in s.mailbox.appointments.values().filter(|a| active_folder(s, a.folder)) {
        for (start, _) in super::calendar::occurrences(a, from, to, 100) {
            rows.push((a.id, a.subject.clone(), start, a.all_day));
            if rows.len() >= 500 {
                break;
            }
        }
        if rows.len() >= 500 {
            break;
        }
    }
    rows.sort_by_key(|r| r.2.unix);
    rows
}
/// Open dated tasks through the chosen day, including overdue work.
pub fn due_tasks(s: &Session, through: i64) -> Vec<(ItemId, String, i64)> {
    let mut rows: Vec<_> = s
        .mailbox
        .tasks
        .values()
        .filter(|t| !t.is_complete() && active_folder(s, t.folder))
        .filter_map(|t| {
            t.due.map(|d| {
                (
                    t.id,
                    t.subject.clone(),
                    d.unix.saturating_add(i64::from(s.tz_offset_min) * 60).div_euclid(86400),
                )
            })
        })
        .filter(|r| r.2 <= through)
        .collect();
    rows.sort_by_key(|r| (r.2, r.0));
    rows.truncate(500);
    rows
}
fn workspace_query(s: &mut Session, p: &Value) -> CmdResult {
    let day = p
        .get("day")
        .and_then(Value::as_i64)
        .filter(|d| d.unsigned_abs() <= 3_000_000)
        .ok_or_else(|| CmdError::Params("A bounded local calendar day is required.".into()))?;
    let links = p.get("id").and_then(Value::as_u64).map(|id| backlinks(s, ItemId(id))).unwrap_or_default();
    Ok(json!({"agenda":agenda(s,day),"tasks":due_tasks(s,day.saturating_add(7)),"backlinks":links}))
}

#[cfg(test)]
mod workspace_tests {
    use super::*;
    use mailcraft_model::note::{NoteHabit, NoteReference};
    #[test]
    fn live_weekly_data_updates_with_organizer_and_undo() {
        let mut s = crate::sample::sample_session();
        let day = s.now().unix.saturating_add(i64::from(s.tz_offset_min) * 60).div_euclid(86400);
        let appointments = agenda(&s, day);
        assert!(!appointments.is_empty());
        let appointment = appointments[0].0;
        s.mailbox.appointments.get_mut(&appointment).unwrap().subject = "Renamed live event".into();
        assert!(agenda(&s, day).iter().any(|r| r.1 == "Renamed live event"));
        let task =
            s.run("tasks.new", &json!({"subject":"Weekly benchmark task","due":"2026-10-09"})).unwrap();
        let id = ItemId(task["id"].as_u64().unwrap());
        assert!(due_tasks(&s, day + 7).iter().any(|r| r.0 == id));
        s.run("tasks.markComplete", &json!({"ids":[id.0]})).unwrap();
        assert!(!due_tasks(&s, day + 7).iter().any(|r| r.0 == id));
        s.run("app.undo", &json!({})).unwrap();
        assert!(due_tasks(&s, day + 7).iter().any(|r| r.0 == id));
        let query = s.run("notes.workspace", &json!({"day":day})).unwrap();
        assert!(!query["agenda"].as_array().unwrap().is_empty());
        assert!(s.run("notes.workspace", &json!({"day":i64::MAX})).is_err());
        assert!(agenda(&s, i64::MAX).is_empty());
    }
    #[test]
    fn links_habits_columns_roundtrip_and_restore() {
        let mut s = crate::sample::sample_session();
        let target = s.run("notes.new", &json!({"text":"Destination"})).unwrap()["id"].as_u64().unwrap();
        let source = s.run("notes.new", &json!({"template":"weekly"})).unwrap()["id"].as_u64().unwrap();
        let mut blocks = s.mailbox.notes[&ItemId(source)].blocks.clone();
        assert!(blocks.iter().any(|b| b.column == 1));
        blocks[0].references.push(NoteReference::Page(ItemId(target)));
        let habit = blocks.iter_mut().find(|b| b.kind == NoteBlockKind::Habits).unwrap();
        habit.habits[0].days.extend([20000, 20007]);
        s.run("notes.update", &json!({"id":source,"blocks":blocks,"categories":["Launch"]})).unwrap();
        assert_eq!(backlinks(&s, ItemId(target)), vec![(ItemId(source), "This week".into())]);
        s.run("notes.update", &json!({"id":target,"text":"Renamed destination"})).unwrap();
        assert_eq!(backlinks(&s, ItemId(target)).len(), 1);
        let note = &s.mailbox.notes[&ItemId(source)];
        assert!(page_tags(note).contains("launch"));
        let roundtrip: Note = serde_json::from_slice(&serde_json::to_vec(note).unwrap()).unwrap();
        assert_eq!(&roundtrip, note);
        let dir = tempfile::tempdir().unwrap();
        let mut store = mailcraft_store::Store::open(dir.path()).unwrap();
        store.checkpoint(&s.mailbox).unwrap();
        assert_eq!(store.load().unwrap().notes[&ItemId(source)], *note);
        let template = s.run("notes.saveTemplate", &json!({"id":source})).unwrap()["id"].as_u64().unwrap();
        let fresh = s.run("notes.new", &json!({"templateId":template})).unwrap()["id"].as_u64().unwrap();
        assert!(
            s.mailbox.notes[&ItemId(fresh)].blocks.iter().flat_map(|b| &b.habits).all(|h| h.days.is_empty())
        );
        assert_eq!(
            s.mailbox.notes[&ItemId(source)].blocks.iter().flat_map(|b| &b.habits).next().unwrap().days.len(),
            2
        );
        s.run("notes.delete", &json!({"id":target})).unwrap();
        assert!(!reference_available(&s, "page", ItemId(target)));
        assert!(s.run("notes.openReference", &json!({"kind":"page","id":target})).is_err());
        s.run("notes.restoreRevision", &json!({"id":source,"revision":0})).unwrap();
        assert!(
            s.mailbox.notes[&ItemId(source)].blocks.iter().flat_map(|b| &b.habits).all(|h| h.days.is_empty())
        );
    }
    #[test]
    fn invalid_workspace_metadata_is_atomic_and_legacy_blocks_load() {
        let mut s = crate::sample::sample_session();
        let id = s.run("notes.new", &json!({"template":"weekly"})).unwrap()["id"].as_u64().unwrap();
        let before = s.mailbox.notes[&ItemId(id)].clone();
        let invalid = [
            vec![NoteBlock { id: 1, column: 3, ..Default::default() }],
            vec![NoteBlock { id: 1, column: 1, ..Default::default() }],
            vec![NoteBlock { id: 1, day_offset: 367, ..Default::default() }],
            vec![NoteBlock {
                id: 1,
                habits: vec![NoteHabit { name: "x".repeat(241), ..Default::default() }],
                ..Default::default()
            }],
            vec![NoteBlock { id: 1, references: vec![NoteReference::Date(i64::MAX)], ..Default::default() }],
            vec![
                NoteBlock { id: 1, section: 1, ..Default::default() },
                NoteBlock { id: 2, ..Default::default() },
                NoteBlock { id: 3, section: 1, ..Default::default() },
            ],
        ];
        for blocks in invalid {
            assert!(s.run("notes.update", &json!({"id":id,"blocks":blocks})).is_err());
            assert_eq!(s.mailbox.notes[&ItemId(id)], before);
        }
        assert!(
            s.run(
                "notes.update",
                &json!({"id":id,"blocks":[{"id":1,"kind":"paragraph","text":"x".repeat(pim::MAX_TEXT+1)}]})
            )
            .is_err()
        );
        assert_eq!(s.mailbox.notes[&ItemId(id)], before);
        let old: NoteBlock =
            serde_json::from_value(json!({"id":1,"kind":"paragraph","text":"Old content"})).unwrap();
        assert_eq!(old.column, 0);
        assert!(old.habits.is_empty());
        assert!(s.run("notes.openReference", &json!({"kind":"page","id":u64::MAX})).is_err());
    }
}
