use serde::{Deserialize, Serialize};

use crate::{DateTime, FolderId, ItemId};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum NoteColor {
    Blue,
    Green,
    Pink,
    #[default]
    Yellow,
    White,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NotePaper {
    #[default]
    Plain,
    Ruled,
    Dotted,
}
fn default_icon() -> String {
    "note".into()
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Note {
    /// Zero uses an adaptive reading width; other values are validated preset widths.
    #[serde(default)]
    pub content_width: u16,
    #[serde(default)]
    pub cover: bool,
    #[serde(default)]
    pub is_template: bool,
    #[serde(default)]
    pub paper: NotePaper,
    #[serde(default = "default_icon")]
    pub icon: String,
    #[serde(default)]
    pub image_blobs: Vec<String>,
    #[serde(default)]
    pub parent: Option<ItemId>,
    #[serde(default)]
    pub blocks: Vec<NoteBlock>,
    #[serde(default)]
    pub history: Vec<NoteRevision>,
    #[serde(default)]
    pub markdown: bool,
    #[serde(default)]
    pub pinned: bool,
    /// Local source message; note content remains available if the message is removed.
    #[serde(default)]
    pub source_mail: Option<ItemId>,
    pub id: ItemId,
    pub folder: FolderId,
    /// First line is the title.
    pub text: String,
    pub color: NoteColor,
    pub created: DateTime,
    pub modified: DateTime,
    pub categories: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NoteBlockKind {
    #[default]
    Paragraph,
    Heading,
    Checklist,
    Quote,
    Callout,
    Code,
    Table,
    Image,
    Agenda,
    Tasks,
    Habits,
    References,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct NoteBlock {
    /// Inline styles use Unicode character offsets, never byte offsets.
    #[serde(default)]
    pub inline: Vec<NoteInline>,
    /// Zero is full width; consecutive blocks sharing a nonzero section form columns.
    #[serde(default)]
    pub section: u64,
    #[serde(default)]
    pub column: u8,
    /// Agenda offset from today; task window extends this many days ahead.
    #[serde(default)]
    pub day_offset: i16,
    #[serde(default)]
    pub habits: Vec<NoteHabit>,
    #[serde(default)]
    pub references: Vec<NoteReference>,
    #[serde(default)]
    pub blob: Option<String>,
    pub id: u64,
    pub kind: NoteBlockKind,
    pub text: String,
    #[serde(default)]
    pub checked: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct NoteRevision {
    pub modified: DateTime,
    pub text: String,
    pub blocks: Vec<NoteBlock>,
}

/// Completion is stored by absolute local calendar day, independent of the displayed week.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct NoteHabit {
    pub name: String,
    #[serde(default)]
    pub days: std::collections::BTreeSet<i64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "camelCase")]
pub enum NoteReference {
    Page(ItemId),
    Person(ItemId),
    Date(i64),
}

/// A bounded inline style range. Overlapping ranges combine their attributes.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct NoteInline {
    pub start: usize,
    pub end: usize,
    #[serde(default)]
    pub bold: bool,
    #[serde(default)]
    pub italic: bool,
    #[serde(default)]
    pub highlight: bool,
    #[serde(default)]
    pub link: Option<String>,
}

/// Keep formatting anchored through a text replacement, including multibyte text.
pub fn remap_inline(ranges: &mut Vec<NoteInline>, before: &str, after: &str) {
    let old: Vec<_> = before.chars().collect();
    let new: Vec<_> = after.chars().collect();
    let prefix = old.iter().zip(&new).take_while(|(a, b)| a == b).count();
    let suffix =
        old.iter().skip(prefix).rev().zip(new.iter().skip(prefix).rev()).take_while(|(a, b)| a == b).count();
    let old_end = old.len().saturating_sub(suffix);
    let new_end = new.len().saturating_sub(suffix);
    let shift = |at: usize| new_end.saturating_add(at.saturating_sub(old_end));
    // Formatting wholly inside replaced text disappears; an exact styled replacement inherits it.
    ranges.retain(|r| {
        old_end == prefix || r.start < prefix || r.end > old_end || (r.start == prefix && r.end == old_end)
    });
    for range in ranges.iter_mut() {
        range.start = if range.start >= old_end {
            shift(range.start)
        } else if range.start > prefix {
            new_end
        } else {
            range.start
        };
        range.end = if range.end > old_end {
            shift(range.end)
        } else if range.end > prefix {
            new_end
        } else {
            range.end
        };
        range.start = range.start.min(new.len());
        range.end = range.end.min(new.len());
    }
    ranges.retain(|r| r.start < r.end);
}

#[cfg(test)]
mod inline_tests {
    use super::*;
    #[test]
    fn formatting_tracks_unicode_replacements_without_touching_unrelated_text() {
        let mut spans = vec![NoteInline { start: 1, end: 5, bold: true, ..Default::default() }];
        remap_inline(&mut spans, " Café later", " Café! later");
        assert_eq!((spans[0].start, spans[0].end), (1, 5));
        remap_inline(&mut spans, " Café! later", " Tea! later");
        assert_eq!((spans[0].start, spans[0].end), (1, 4));
        remap_inline(&mut spans, " Tea! later", " ! later");
        assert!(spans.is_empty());
        let mut spans = vec![NoteInline { start: 1, end: 4, italic: true, ..Default::default() }];
        remap_inline(&mut spans, "abcdefgh", "Replacement");
        assert!(spans.is_empty(), "Replacing an entire paragraph must not spread partial styles");
    }
}
