//! Compose windows (viewports) and inline responses in the reading pane, with the body editor (compose area).
//!
//! Layout follows plan/reference/compose.md, reading-pane.md (inline reply) and responsive.md (compose window):
//!
//! - Each non-inline [`ComposeState`] is an immediate viewport (default 1440×780, title from
//!   [`ComposeState::title`], OS title bar). Under it: a Quick Access row (Save, Undo, Redo, Previous Item, Next
//!   Item), the tab row (File + the compose tabs) and the ribbon from [`crate::ribbon::ribbon`].
//! - Header: a big Send button (60×76) spanning the first two address rows; To / Cc buttons (80×32) that open the
//!   address book; Subject label; optional From and Bcc rows; the attachments well; a 1-px separator; the body.
//! - At 300×200 (anything below ~360×260, `responsive::inspector_tiny`, shared with read windows) the Quick
//!   Access row, tab row and ribbon disappear; the header stays.
//! - While any compose has unsaved changes, `compose.autoSaveTick` runs every [`AUTO_SAVE_CHECK_SECS`]
//!   seconds ([`auto_save_timer`]); the engine saves the drafts whose changes are older than the Options ›
//!   Mail auto-save minutes (nothing when auto-save is off).
//! - Closing a window with unsaved changes asks "Do you want to save changes?" (Yes / No / Cancel) →
//!   `compose.close {"save": true|false}`.
//! - Inline responses: the same header plus Discard and Pop Out at the top right, the body editor below.
//!
//! Every change goes through `App::run` (`compose.*`, `format.*`, `nav.*`); header fields run their `compose.set*`
//! command once per frame when the text changed.
//!
//! Submodules: `editor` (the body editor), `recipients` (To / Cc / Bcc: Auto-Complete, resolved names, Ctrl+K
//! Check Names), `attach` (attachments well, Browse This PC…, recent files, drag and drop, file hooks),
//! `spelling` (the F7 Spelling and Grammar dialog), `clipboard` (the system clipboard's HTML flavour for rich
//! paste and copy, and a picture reader for picture paste), `voting` (reading-pane voting, read-receipt and
//! tracking bars) and `panes` (the Clipboard, Styles and Selection panes beside the body).

mod attach;
mod clipboard;
mod editor;
mod panes;
mod recipients;
mod spelling;
#[cfg(test)]
mod tests;
mod voting;

use egui::{
    Align, Align2, CornerRadius, Id, Layout, Rect, Sense, Stroke, StrokeKind, TextEdit, Ui, Vec2,
    ViewportBuilder, ViewportClass, ViewportCommand, ViewportId, WidgetInfo, WidgetType, pos2, vec2,
};
use serde_json::json;

use mailcraft_engine::compose::{ComposeKind, ComposeState};

use crate::responsive::Frame;
use crate::{App, Surface};

pub use attach::{
    DROP_HINT, FileHooks, MAX_RECENT, SAVE_TITLE, file_hooks, file_name as recent_label, recent_files,
    remember_file, set_file_hooks, size_text,
};
pub use clipboard::{
    ClipboardHooks, clipboard_hooks, clipboard_html, clipboard_image, clipboard_text, copy_html,
    html_for_paste, set_clipboard_hooks, set_clipboard_image_reader,
};
pub use editor::{BODY_LABEL, NO_SUGGESTIONS, PASTE_MODES, body_focused, editor_id, request_paste};
pub(crate) use editor::{EditorHost, body_editor_in};
pub use panes::{
    CLIPBOARD_EMPTY, CLIPBOARD_TITLE, NO_OBJECTS, PANE_W, SELECTION_TITLE, STYLES_TITLE, clip_label,
    hidden_objects,
};
pub use recipients::{
    Entry as RecipientEntry, entries as recipient_entries, split_entries as split_recipients,
};
pub use spelling::{COMPLETE as SPELLING_COMPLETE, TITLE as SPELLING_TITLE, is_open as spelling_open};
pub use voting::{READ_RECEIPT_REQUEST, ReceiptPolicy, tracking_bars};

/// Default compose window size (compose.md).
pub const DEFAULT_SIZE: [f32; 2] = [1440.0, 780.0];
/// Smallest size accepted (responsive.md: 300×200 accepted).
pub const MIN_SIZE: [f32; 2] = [300.0, 200.0];
/// Seconds between auto-save checks while a compose has unsaved changes (`compose.autoSaveTick`; the engine
/// saves only once File › Options › Mail › "Automatically save items…" minutes have passed).
pub const AUTO_SAVE_CHECK_SECS: f64 = 15.0;

const SEND_SIZE: Vec2 = Vec2::new(60.0, 76.0);
const NAME_BTN: Vec2 = Vec2::new(80.0, 32.0);
const ROW_H: f32 = 38.0;
const MARGIN: f32 = 16.0;
const GAP: f32 = 12.0;
const QAT_H: f32 = 30.0;
const TAB_H: f32 = 30.0;
/// Left inset of the attachments well (lines the chips up with the address buttons).
const WELL_INDENT: f32 = MARGIN + SEND_SIZE.x + MARGIN;

/// Compose tabs used when the ribbon area does not list any (compose.md).
const FALLBACK_TABS: [&str; 6] = ["Message", "Insert", "Options", "Format Text", "Review", "Help"];

/// Save-changes prompt text (our wording).
pub const SAVE_PROMPT: &str = "Do you want to save changes?";

/// The viewport of compose window `cid`.
pub fn viewport_id(cid: u64) -> ViewportId {
    ViewportId::from_hash_of(("mailcraft-compose", cid))
}

/// Header fields.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Field {
    From,
    To,
    Cc,
    Bcc,
    Subject,
}

impl Field {
    fn command(self) -> &'static str {
        match self {
            Field::From => "compose.setFrom",
            Field::To => "compose.setTo",
            Field::Cc => "compose.setCc",
            Field::Bcc => "compose.setBcc",
            Field::Subject => "compose.setSubject",
        }
    }
    fn label(self) -> &'static str {
        match self {
            Field::From => "From",
            Field::To => "To",
            Field::Cc => "Cc",
            Field::Bcc => "Bcc",
            Field::Subject => "Subject",
        }
    }
    fn book_field(self) -> &'static str {
        match self {
            Field::Cc => "cc",
            Field::Bcc => "bcc",
            _ => "to",
        }
    }
    fn value(self, c: &ComposeState) -> &str {
        match self {
            Field::From => &c.from,
            Field::To => &c.to,
            Field::Cc => &c.cc,
            Field::Bcc => &c.bcc,
            Field::Subject => &c.subject,
        }
    }
}

/// egui id of a header field of compose `cid`.
fn field_id(cid: u64, f: Field) -> Id {
    Id::new(("mailcraft-compose-field", cid, f))
}

/// The compose whose header field or body has keyboard focus, if any (for the keyboard handler: compose
/// shortcuts such as Ctrl+Enter apply to it, and list shortcuts must not fire while typing).
pub fn focused_compose(ctx: &egui::Context, app: &App) -> Option<u64> {
    let focus = ctx.memory(|m| m.focused())?;
    app.s.composes.keys().copied().find(|&cid| {
        focus == editor_id(cid)
            || [Field::From, Field::To, Field::Cc, Field::Bcc, Field::Subject]
                .iter()
                .any(|f| focus == field_id(cid, *f))
    })
}

/// Per-compose window state in egui temp memory.
#[derive(Clone, Copy, Debug, Default)]
struct WinState {
    /// Initial focus placed.
    started: bool,
    /// Widget to focus once it is drawn for real.
    pending_focus: Option<Id>,
    /// The save-changes prompt is open.
    ask_save: bool,
}

fn win_state_id(cid: u64) -> Id {
    Id::new(("mailcraft-compose-win", cid))
}

fn win_state(ctx: &egui::Context, cid: u64) -> WinState {
    ctx.data(|d| d.get_temp::<WinState>(win_state_id(cid))).unwrap_or_default()
}

fn set_win_state(ctx: &egui::Context, cid: u64, st: WinState) {
    ctx.data_mut(|d| d.insert_temp(win_state_id(cid), st));
}

/// Opens the "Do you want to save changes?" prompt in compose window `cid` (e.g. for File › Close or a
/// `saveChanges` dialog notice from `compose.close`).
pub fn ask_save_changes(ctx: &egui::Context, cid: u64) {
    let mut st = win_state(ctx, cid);
    st.ask_save = true;
    set_win_state(ctx, cid, st);
}

/// Close request for a window: prompt when there are unsaved changes, else close right away.
fn request_close(ctx: &egui::Context, app: &mut App, cid: u64) {
    let dirty = app.s.composes.get(&cid).is_some_and(|c| c.dirty);
    if dirty {
        ask_save_changes(ctx, cid);
    } else {
        app.run("compose.close", json!({"compose": cid, "save": false}));
    }
}

// =======================================================================================================
// Entry points
// =======================================================================================================

/// All open compose windows (non-inline composes in `app.s.composes`).
pub fn compose_windows(ctx: &egui::Context, app: &mut App) {
    let now = ctx.input(|i| i.time);
    auto_save_timer(ctx, app, now);
    let ids: Vec<u64> = app.s.composes.values().filter(|c| !c.inline).map(|c| c.id).collect();
    for cid in ids {
        let Some(c) = app.s.composes.get(&cid) else {
            continue;
        };
        let builder = ViewportBuilder::default()
            .with_title(if app.mac_style {
                if c.subject.trim().is_empty() { "New Message".to_owned() } else { c.subject.clone() }
            } else {
                c.title()
            })
            .with_inner_size(if app.mac_style { [860.0, 680.0] } else { DEFAULT_SIZE })
            .with_min_inner_size(MIN_SIZE);
        let builder = if cfg!(target_os = "macos") && app.mac_style {
            builder.with_fullsize_content_view(true).with_titlebar_shown(false).with_title_shown(false)
        } else {
            builder
        };
        ctx.show_viewport_immediate(viewport_id(cid), builder, |ui, class| {
            compose_viewport(ui, app, cid, class)
        });
    }
}

fn auto_save_id() -> Id {
    Id::new("mailcraft-compose-autosave")
}

/// The auto-save timer: while a compose (window or inline) has unsaved changes, runs `compose.autoSaveTick`
/// every [`AUTO_SAVE_CHECK_SECS`] seconds of `now` (egui time) and asks for a repaint then, so a message
/// nobody is typing in is still saved. The first check comes one period after the changes are first seen.
/// Returns true when it ran the tick.
pub fn auto_save_timer(ctx: &egui::Context, app: &mut App, now: f64) -> bool {
    let dirty = app.s.composes.values().any(|c| c.dirty);
    let last: Option<f64> = ctx.data(|d| d.get_temp(auto_save_id()));
    if !dirty {
        if last.is_some() {
            ctx.data_mut(|d| d.remove::<f64>(auto_save_id()));
        }
        return false;
    }
    // A clock that went backwards (a new context) restarts the period.
    let since = match last {
        Some(t) if t <= now => t,
        _ => {
            ctx.data_mut(|d| d.insert_temp(auto_save_id(), now));
            ctx.request_repaint_after(std::time::Duration::from_secs_f64(AUTO_SAVE_CHECK_SECS));
            return false;
        }
    };
    let waited = now - since;
    if waited < AUTO_SAVE_CHECK_SECS {
        let left = (AUTO_SAVE_CHECK_SECS - waited).clamp(0.1, AUTO_SAVE_CHECK_SECS);
        ctx.request_repaint_after(std::time::Duration::from_secs_f64(left));
        return false;
    }
    ctx.data_mut(|d| d.insert_temp(auto_save_id(), now));
    ctx.request_repaint_after(std::time::Duration::from_secs_f64(AUTO_SAVE_CHECK_SECS));
    app.run("compose.autoSaveTick", json!({}));
    true
}

/// Inline response inside the reading pane.
pub fn inline_compose(ui: &mut Ui, app: &mut App, id: u64) {
    if !app.s.composes.contains_key(&id) {
        return;
    }
    let ctx = ui.ctx().clone();
    initial_focus(&ctx, app, id);
    attach::take_dropped_files(&ctx, app, id, Some(ui.max_rect()));
    ui.push_id(("mailcraft-inline-compose", id), |ui| {
        // Discard / Pop Out at the top right.
        ui.allocate_ui_with_layout(
            vec2(ui.available_width(), 28.0),
            Layout::right_to_left(Align::Center),
            |ui| {
                ui.add_space(MARGIN);
                if crate::widgets::icon_button(ui, app, "close", "Discard", true).clicked() {
                    app.run("compose.discard", json!({"compose": id}));
                }
                if crate::widgets::icon_button(ui, app, "restore", "Pop Out", true).clicked() {
                    app.run("compose.popOut", json!({"compose": id}));
                }
            },
        );
        if !app.s.composes.contains_key(&id) {
            return;
        }
        if app.mac_style {
            mac_toolbar(ui, app, id);
        }
        header(ui, app, id);
        attach::well(ui, app, id, if app.mac_style { 28.0 } else { WELL_INDENT });
        separator(ui, app);
        if app.mac_style {
            mac_formatting(ui, app, id);
        }
        let h = ui.available_height();
        let min_h = if h.is_finite() && h < 4000.0 { h.max(160.0) } else { 240.0 };
        panes::body_with_panes(
            ui,
            app,
            id,
            if app.mac_style { (min_h - 48.0).max(100.0) } else { min_h },
            false,
        );
        if app.mac_style {
            mac_signature(ui, app, id);
        }
    });
    spelling::dialog(&ctx, app, id);
    attach::save_as_dialog(&ctx, app, id);
}

// =======================================================================================================
// Window
// =======================================================================================================

fn compose_viewport(ui: &mut Ui, app: &mut App, cid: u64, class: ViewportClass) {
    let ctx = ui.ctx().clone();
    if !app.s.composes.contains_key(&cid) {
        return;
    }
    let embedded = matches!(class, ViewportClass::EmbeddedWindow);
    if !embedded && ctx.input(|i| i.viewport().close_requested()) {
        let dirty = app.s.composes.get(&cid).is_some_and(|c| c.dirty);
        if dirty {
            ctx.send_viewport_cmd(ViewportCommand::CancelClose);
        }
        request_close(&ctx, app, cid);
        if !app.s.composes.contains_key(&cid) {
            return;
        }
    }
    // Files dropped from the OS become attachments (an embedded window shares the main window's input: only
    // drops over the window count).
    attach::take_dropped_files(&ctx, app, cid, embedded.then(|| ui.max_rect()));
    initial_focus(&ctx, app, cid);

    let size = if embedded { ui.available_size() } else { ctx.content_rect().size() };
    let frame = Frame::compute(size.x, size.y);
    let tiny = frame.inspector_tiny;

    let backstage_here = matches!(&app.ui.backstage, Some((Surface::Compose(s), _)) if *s == cid);
    egui::CentralPanel::default().frame(egui::Frame::new().fill(app.theme.pane)).show(ui, |ui| {
        if backstage_here {
            crate::backstage::compose_backstage(ui, app, cid);
            return;
        }
        if app.mac_style {
            mac_toolbar(ui, app, cid);
        }
        if !app.mac_style && (!tiny || embedded) {
            quick_access(ui, app, cid, embedded, tiny);
        }
        if !tiny && !app.mac_style {
            tab_row(ui, app, cid);
            crate::ribbon::ribbon(ui, app, Surface::Compose(cid), &frame);
        }
        if !app.s.composes.contains_key(&cid) {
            return;
        }
        ui.add_space(if app.mac_style { 4.0 } else { 8.0 });
        header(ui, app, cid);
        attach::well(ui, app, cid, if app.mac_style { 28.0 } else { WELL_INDENT });
        separator(ui, app);
        if app.mac_style {
            mac_formatting(ui, app, cid);
            let available = ui.available_rect_before_wrap();
            let body_rect = Rect::from_min_max(
                available.min,
                pos2(available.right(), (available.bottom() - 48.0).max(available.top() + 60.0)),
            );
            let mut body_ui =
                ui.new_child(egui::UiBuilder::new().max_rect(body_rect).layout(Layout::top_down(Align::Min)));
            egui::Frame::new().inner_margin(egui::Margin::symmetric(28, 18)).show(&mut body_ui, |ui| {
                panes::body_with_panes(ui, app, cid, 0.0, true);
            });
            ui.advance_cursor_after_rect(body_rect);
            mac_signature(ui, app, cid);
        } else {
            panes::body_with_panes(ui, app, cid, 0.0, true);
        }
    });

    save_prompt(&ctx, app, cid);
    spelling::dialog(&ctx, app, cid);
    attach::save_as_dialog(&ctx, app, cid);

    if app.mac_style && !embedded {
        crate::dialogs::dialogs(&ctx, app);
    }

    // F7 anywhere in the window (the body editor already took it when it has focus).
    let f7 = ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::F7));
    if f7 && app.s.composes.contains_key(&cid) {
        spelling::open(&ctx, app, cid);
    }

    // Compose-surface shortcuts (Ctrl+S, Ctrl+Enter, Alt keytips…) on this window's own input. The body editor
    // already took the keys it handles out of it. An embedded window shares the main viewport's input, which
    // the main handler routes to the focused compose itself (`focused_compose`).
    if !embedded {
        crate::keys::handle(&ctx, app, Surface::Compose(cid));
    }
}

/// Focus starts in To for new messages and forwards, in the body for replies and drafts (compose.md,
/// reply-forward.md).
fn initial_focus(ctx: &egui::Context, app: &App, cid: u64) {
    let mut st = win_state(ctx, cid);
    if st.started {
        return;
    }
    st.started = true;
    let to_first =
        app.s.composes.get(&cid).is_some_and(|c| matches!(c.kind, ComposeKind::New | ComposeKind::Forward));
    st.pending_focus = Some(if to_first { field_id(cid, Field::To) } else { editor_id(cid) });
    set_win_state(ctx, cid, st);
}

/// Gives `resp` the initial focus of compose `cid` when it is the pending target and really on screen (a new
/// window's first frame is a sizing pass, where a focus request would be dropped).
pub(crate) fn claim_initial_focus(ui: &Ui, cid: u64, resp: &egui::Response) {
    let ctx = ui.ctx();
    let mut st = win_state(ctx, cid);
    if st.pending_focus != Some(resp.id) || ui.is_sizing_pass() || !ui.is_visible() {
        return;
    }
    resp.request_focus();
    st.pending_focus = None;
    set_win_state(ctx, cid, st);
}

/// Quick Access row: Save, Undo, Redo, Previous Item, Next Item (plus Close when embedded).
fn quick_access(ui: &mut Ui, app: &mut App, cid: u64, embedded: bool, tiny: bool) {
    let Some(c) = app.s.composes.get(&cid) else {
        return;
    };
    let can_undo = !c.edit_undo.is_empty();
    let can_redo = !c.edit_redo.is_empty();
    let has_source = c.source.is_some();
    let ctx = ui.ctx().clone();
    ui.allocate_ui_with_layout(
        vec2(ui.available_width(), QAT_H),
        Layout::left_to_right(Align::Center),
        |ui| {
            ui.add_space(8.0);
            if !tiny {
                if crate::widgets::icon_only(ui, app, "save", "Save", true).clicked() {
                    app.run("compose.saveDraft", json!({"compose": cid}));
                }
                if crate::widgets::icon_only(ui, app, "undo", "Undo", can_undo).clicked() {
                    app.run("compose.editUndo", json!({"compose": cid}));
                }
                if crate::widgets::icon_only(ui, app, "redo", "Redo", can_redo).clicked() {
                    app.run("compose.editRedo", json!({"compose": cid}));
                }
                if crate::widgets::icon_only(ui, app, "chevron-up", "Previous Item", has_source).clicked() {
                    app.run("nav.prevItem", json!({}));
                }
                if crate::widgets::icon_only(ui, app, "chevron-down", "Next Item", has_source).clicked() {
                    app.run("nav.nextItem", json!({}));
                }
            }
            if embedded {
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.add_space(8.0);
                    // The embedding egui window shows the title; it has no close button of its own.
                    if crate::widgets::icon_only(ui, app, "close", "Close", true).clicked() {
                        request_close(&ctx, app, cid);
                    }
                });
            }
        },
    );
}

/// File + the compose tabs; the selected tab is `app.ui.tab(Surface::Compose(cid))`.
fn tab_row(ui: &mut Ui, app: &mut App, cid: u64) {
    let surface = Surface::Compose(cid);
    let mut tabs: Vec<&str> = crate::ribbon::tabs(app, surface);
    if tabs.is_empty() {
        tabs = FALLBACK_TABS.to_vec();
    }
    let selected = app
        .ui
        .tab(surface)
        .filter(|t| tabs.contains(t))
        .map(str::to_owned)
        .unwrap_or_else(|| tabs.first().map(|t| (*t).to_owned()).unwrap_or_default());
    let accent = app.theme.accent;
    let text = app.theme.text;
    ui.allocate_ui_with_layout(
        vec2(ui.available_width(), TAB_H),
        Layout::left_to_right(Align::Center),
        |ui| {
            ui.add_space(8.0);
            // Tabs sit apart like the main window's tab row, not packed together.
            ui.spacing_mut().item_spacing.x = 10.0;
            ui.spacing_mut().button_padding.x = 6.0;
            if ui.add(egui::Button::new(egui::RichText::new("File").color(text)).frame(false)).clicked() {
                app.ui.backstage = Some((surface, "info".to_owned()));
            }
            for t in tabs {
                let on = t == selected;
                let r = ui
                    .add(egui::Button::new(egui::RichText::new(t).color(text)).frame(false).selected(false));
                if on {
                    let y = r.rect.bottom() + 1.0;
                    ui.painter().line_segment(
                        [pos2(r.rect.left() + 4.0, y), pos2(r.rect.right() - 4.0, y)],
                        Stroke::new(3.0, accent),
                    );
                }
                if r.clicked() {
                    app.ui.set_tab(surface, t);
                }
            }
        },
    );
}

/// The save-changes prompt (Yes saves to Drafts and closes, No closes, Cancel keeps editing).
fn save_prompt(ctx: &egui::Context, app: &mut App, cid: u64) {
    let mut st = win_state(ctx, cid);
    if !st.ask_save {
        return;
    }
    let mut choice: Option<Option<bool>> = None;
    let theme = app.theme.clone();
    let modal = egui::Modal::new(Id::new(("mailcraft-compose-save", cid))).show(ctx, |ui| {
        crate::dialogs::form_visuals(ui, &theme);
        ui.set_min_width(300.0);
        ui.label(egui::RichText::new(SAVE_PROMPT).size(15.0));
        ui.add_space(12.0);
        ui.horizontal(|ui| {
            let size = vec2(80.0, 26.0);
            if ui.add(egui::Button::new("Yes").min_size(size)).clicked() {
                choice = Some(Some(true));
            }
            if ui.add(egui::Button::new("No").min_size(size)).clicked() {
                choice = Some(Some(false));
            }
            if ui.add(egui::Button::new("Cancel").min_size(size)).clicked() {
                choice = Some(None);
            }
        });
    });
    if choice.is_none() && modal.should_close() {
        choice = Some(None);
    }
    if let Some(c) = choice {
        st.ask_save = false;
        set_win_state(ctx, cid, st);
        if let Some(save) = c {
            app.run("compose.close", json!({"compose": cid, "save": save}));
        }
    }
}

// =======================================================================================================
// Header
// =======================================================================================================

fn separator(ui: &mut Ui, app: &App) {
    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
    ui.painter().rect_filled(r, CornerRadius::ZERO, app.theme.separator);
}

/// The big Send button (icon over label).
fn send_button(ui: &mut Ui, app: &mut App, rect: Rect) -> egui::Response {
    let r = ui.allocate_rect(rect, Sense::click());
    r.widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, "Send"));
    let fill = if r.is_pointer_button_down_on() {
        app.theme.selected
    } else if r.hovered() {
        app.theme.hover
    } else {
        app.theme.pane
    };
    let p = ui.painter();
    p.rect_filled(rect, CornerRadius::same(4), fill);
    p.rect_stroke(rect, CornerRadius::same(4), Stroke::new(1.0, app.theme.separator), StrokeKind::Inside);
    let icon = Rect::from_center_size(pos2(rect.center().x, rect.top() + 26.0), vec2(24.0, 24.0));
    app.icons.paint(ui, icon, "send", app.theme.accent);
    ui.painter().text(
        pos2(rect.center().x, rect.bottom() - 18.0),
        Align2::CENTER_CENTER,
        "Send",
        egui::FontId::proportional(13.0),
        app.theme.text,
    );
    r
}

/// To / Cc / Bcc / From buttons (80×32, outlined like the Send button: they are buttons, not labels).
fn name_button(ui: &mut Ui, app: &App, rect: Rect, label: &str) -> egui::Response {
    ui.put(
        rect,
        egui::Button::new(egui::RichText::new(label).color(app.theme.text))
            .fill(app.theme.pane)
            .stroke(Stroke::new(1.0, app.theme.separator))
            .min_size(NAME_BTN),
    )
}

/// An address / subject field filling `rect`; runs the field's command when the text changes.
fn field_edit(ui: &mut Ui, app: &mut App, cid: u64, f: Field, rect: Rect, label_id: Option<Id>) {
    let Some(c) = app.s.composes.get(&cid) else {
        return;
    };
    let mut text = f.value(c).to_owned();
    let before = text.clone();
    let edit_rect =
        Rect::from_min_max(pos2(rect.left(), rect.top() + 3.0), pos2(rect.right(), rect.bottom() - 3.0));
    let te = TextEdit::singleline(&mut text)
        .id(field_id(cid, f))
        .frame(egui::Frame::NONE)
        .text_color(app.theme.text)
        .font(egui::FontId::proportional(if app.mac_style { 17.0 } else { 13.0 }))
        .hint_text(if app.mac_style && f == Field::Subject { "Add a subject" } else { "" })
        .vertical_align(Align::Center)
        .desired_width(edit_rect.width());
    let mut r = ui.put(edit_rect, te);
    if let Some(l) = label_id {
        r = r.labelled_by(l);
    }
    claim_initial_focus(ui, cid, &r);
    let line_col = if r.has_focus() {
        app.theme.accent.gamma_multiply(0.6)
    } else if app.mac_style {
        egui::Color32::TRANSPARENT
    } else {
        app.theme.separator
    };
    ui.painter().line_segment(
        [pos2(rect.left(), rect.bottom() - 1.0), pos2(rect.right(), rect.bottom() - 1.0)],
        Stroke::new(1.0, line_col),
    );
    if text != before {
        app.run(f.command(), json!({"compose": cid, "value": text}));
    }
}

/// F7 on a compose surface: opens the Spelling and Grammar dialog (for the keyboard handler; compose windows
/// and the body editor catch F7 themselves).
pub fn open_spelling_dialog(ctx: &egui::Context, app: &mut App, cid: u64) {
    spelling::open(ctx, app, cid);
}

/// The compose whose To, Cc or Bcc field has keyboard focus (Ctrl+K there is Check Names, not Insert Link).
pub fn focused_recipient_field(ctx: &egui::Context, app: &App) -> Option<u64> {
    let focus = ctx.memory(|m| m.focused())?;
    app.s
        .composes
        .keys()
        .copied()
        .find(|&cid| [Field::To, Field::Cc, Field::Bcc].iter().any(|f| focus == field_id(cid, *f)))
}

/// Ribbon commands the compose window handles itself (called by the ribbon before running a command on a
/// compose surface). Returns true when handled:
/// - `compose.attachFile` without a file (Browse This PC…) runs the platform's file picker when installed;
///   with a `path` it attaches and remembers the file for the recent list;
/// - `options.open {"page": "signatures"}` (Signature ▾ › Signatures…) opens Signatures and Stationery itself
///   rather than the Options dialog;
/// - `review.spelling` opens the Spelling and Grammar dialog;
/// - `format.paste` with a `mode` (Paste ▾ menu) pastes the system clipboard with that mode.
pub(crate) fn ribbon_command(
    ctx: &egui::Context,
    app: &mut App,
    cid: u64,
    cmd: &str,
    params: &serde_json::Value,
) -> bool {
    match cmd {
        "compose.attachFile" if attach::names_no_file(params) => attach::browse(app, cid),
        "compose.attachFile" => match params.get("path").and_then(serde_json::Value::as_str) {
            Some(p) => {
                attach::attach_path(app, cid, p);
                true
            }
            None => false,
        },
        "options.open" if params.get("page").and_then(serde_json::Value::as_str) == Some("signatures") => {
            crate::dialogs::open(app, "signatures", json!({"compose": cid}));
            true
        }
        "review.spelling" => {
            let grammar = params.get("grammar").and_then(serde_json::Value::as_bool).unwrap_or(true);
            spelling::open_with(ctx, app, cid, grammar);
            true
        }
        // Paste ▾ › Keep Source Formatting / Merge Formatting / Keep Text Only: paste the system clipboard
        // (HTML when it has some) with that mode, like Ctrl+V.
        "format.paste" if params.get("text").is_none() && params.get("html").is_none() => {
            let wanted = params.get("mode").and_then(serde_json::Value::as_str);
            match PASTE_MODES.iter().map(|(m, _)| *m).find(|m| Some(*m) == wanted) {
                Some(mode) => {
                    request_paste(ctx, cid, mode);
                    true
                }
                None => false,
            }
        }
        _ => false,
    }
}

/// Send + address rows + Subject (shared by windows and inline responses).
fn header(ui: &mut Ui, app: &mut App, cid: u64) {
    if app.mac_style {
        mac_header(ui, app, cid);
        return;
    }
    let Some(c) = app.s.composes.get(&cid) else {
        return;
    };
    let mut rows: Vec<Field> = Vec::new();
    if c.show_from || app.mac_style {
        rows.push(Field::From);
    }
    rows.extend([Field::To, Field::Cc]);
    if c.show_bcc {
        rows.push(Field::Bcc);
    }
    rows.push(Field::Subject);
    let accounts: Vec<(u32, String)> = app
        .s
        .mailbox
        .accounts
        .iter()
        .filter(|a| !a.address.trim().is_empty())
        .map(|a| (a.id.0, a.address.clone()))
        .collect();

    let width = ui.available_width();
    let rows_h = ROW_H * rows.len() as f32;
    let height = rows_h.max(if app.mac_style { 0.0 } else { SEND_SIZE.y }) + 4.0;
    let (rect, _) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
    let left = rect.left() + MARGIN;
    let send_rect =
        Rect::from_min_size(pos2(left, rect.top()), if app.mac_style { vec2(0.0, 0.0) } else { SEND_SIZE });
    if !app.mac_style && send_button(ui, app, send_rect).clicked() {
        app.run("compose.send", json!({"compose": cid}));
        if !app.s.composes.contains_key(&cid) {
            return;
        }
    }
    let btn_x = if app.mac_style { left } else { send_rect.right() + MARGIN };
    let field_x = btn_x + NAME_BTN.x + GAP;
    let field_r = (rect.right() - MARGIN).max(field_x + 20.0);
    for (i, f) in rows.into_iter().enumerate() {
        let top = rect.top() + ROW_H * i as f32;
        let btn = Rect::from_min_size(pos2(btn_x, top + (ROW_H - NAME_BTN.y) / 2.0), NAME_BTN);
        let field = Rect::from_min_max(pos2(field_x, top), pos2(field_r, top + ROW_H));
        let label_id = match f {
            Field::Subject => {
                let r = ui.put(btn, egui::Label::new(egui::RichText::new("Subject").color(app.theme.text)));
                Some(r.id)
            }
            Field::From => {
                let r = name_button(ui, app, btn, "From");
                egui::Popup::menu(&r).show(|ui| {
                    for (id, addr) in &accounts {
                        if ui.button(addr).clicked() {
                            app.run("compose.setFrom", json!({"compose": cid, "value": id.to_string()}));
                        }
                    }
                    if accounts.is_empty() {
                        ui.label("No accounts");
                    }
                });
                Some(r.id)
            }
            _ => {
                let r = name_button(ui, app, btn, f.label());
                if r.clicked() {
                    app.run("compose.addressBook", json!({"compose": cid, "field": f.book_field()}));
                }
                Some(r.id)
            }
        };
        match f {
            Field::To | Field::Cc | Field::Bcc => recipients::address_field(ui, app, cid, f, field, label_id),
            Field::From | Field::Subject => field_edit(ui, app, cid, f, field, label_id),
        }
    }
}

/// Mac header: quiet labels, continuous row dividers, and a consistent writing inset.
fn mac_header(ui: &mut Ui, app: &mut App, cid: u64) {
    let Some(c) = app.s.composes.get(&cid) else {
        return;
    };
    let cc_open = !c.cc.is_empty()
        || ui.ctx().data(|d| d.get_temp::<bool>(Id::new(("mac-compose-cc", cid)))).unwrap_or(false);
    let mut rows = vec![Field::From, Field::To];
    if cc_open {
        rows.push(Field::Cc);
    }
    if c.show_bcc {
        rows.push(Field::Bcc);
    }
    rows.push(Field::Subject);
    let sender_badge = mailcraft_engine::compose::compose_account(&app.s.mailbox, &c.from)
        .and_then(|id| app.s.mailbox.account(id))
        .and_then(|a| a.linked_mailbox.as_ref())
        .map(|link| {
            format!(
                "{} · {}",
                if link.delegated { "Delegated" } else { "Shared" },
                if link.send_as { "Send As" } else { "Receive only" }
            )
        });
    let accounts: Vec<(u32, String)> = app
        .s
        .mailbox
        .accounts
        .iter()
        .filter(|a| !a.address.trim().is_empty())
        .map(|a| (a.id.0, format!("{} <{}>", a.display_name, a.address)))
        .collect();
    let (rect, _) =
        ui.allocate_exact_size(vec2(ui.available_width(), rows.len() as f32 * 54.0 + 8.0), Sense::hover());
    for (i, f) in rows.into_iter().enumerate() {
        let y = rect.top() + i as f32 * 54.0;
        let label_rect = Rect::from_min_size(pos2(rect.left() + 24.0, y + 13.0), vec2(64.0, 28.0));
        let badge_visible = f == Field::From && sender_badge.is_some() && rect.width() > 620.0;
        let trailing = if badge_visible {
            185.0
        } else if f == Field::To {
            84.0
        } else {
            28.0
        };
        if badge_visible && let Some(text) = &sender_badge {
            let badge = Rect::from_min_size(pos2(rect.right() - 175.0, y + 14.0), vec2(150.0, 26.0));
            ui.painter().rect_filled(badge, 5, app.theme.selected_folder);
            ui.painter().text(
                badge.center(),
                egui::Align2::CENTER_CENTER,
                text,
                egui::FontId::proportional(12.0),
                app.theme.accent,
            );
        }
        let field = Rect::from_min_max(pos2(rect.left() + 100.0, y), pos2(rect.right() - trailing, y + 54.0));
        if f == Field::To {
            let r = ui.put(
                Rect::from_min_size(pos2(rect.right() - 80.0, y + 13.0), vec2(56.0, 28.0)),
                egui::Button::new(egui::RichText::new("Cc/Bcc").size(13.0).color(app.theme.text_muted))
                    .frame(false),
            );
            egui::Popup::menu(&r).show(|ui| {
                if ui.selectable_label(cc_open, "Show Cc").clicked() {
                    ui.ctx().data_mut(|d| d.insert_temp(Id::new(("mac-compose-cc", cid)), !cc_open));
                    ui.close();
                }
                if ui.button("Show Bcc").clicked() {
                    app.run("compose.showBcc", json!({"compose":cid}));
                    ui.close();
                }
            });
        }
        let label = egui::RichText::new(f.label()).size(15.0).color(app.theme.text_muted);
        let response = if f == Field::Subject {
            ui.put(label_rect, egui::Label::new(label))
        } else {
            ui.put(label_rect, egui::Button::new(label).frame(false))
        };
        if f == Field::From {
            let arrow_rect =
                Rect::from_center_size(pos2(field.right() - 10.0, field.center().y), vec2(20.0, 28.0));
            let arrow = ui.interact(arrow_rect, ui.id().with(("sender-picker", cid)), Sense::click());
            let response = response.union(arrow);
            egui::Popup::menu(&response).show(|ui| {
                for (id, label) in &accounts {
                    if ui.button(label).clicked() {
                        app.run("compose.setFrom", json!({"compose": cid, "value": id.to_string()}));
                    }
                }
                if accounts.is_empty() {
                    ui.label("No accounts");
                }
            });
            app.icons.paint(
                ui,
                Rect::from_center_size(pos2(field.right() - 10.0, field.center().y), vec2(10.0, 10.0)),
                "chevron-down",
                app.theme.text_muted,
            );
        } else if f != Field::Subject && response.clicked() {
            app.run("compose.addressBook", json!({"compose":cid, "field":f.book_field()}));
        }
        match f {
            Field::To | Field::Cc | Field::Bcc => {
                recipients::address_field(ui, app, cid, f, field, Some(response.id))
            }
            _ => field_edit(
                ui,
                app,
                cid,
                f,
                if f == Field::From {
                    Rect::from_min_max(field.min, pos2(field.right() - 24.0, field.bottom()))
                } else {
                    field
                },
                Some(response.id),
            ),
        }
        {
            ui.painter().hline(
                rect.left() + 28.0..=rect.right() - 28.0,
                y + 54.0,
                Stroke::new(0.5, app.theme.separator.gamma_multiply(0.65)),
            );
        }
    }
}

/// Compact toolbar with an explicit primary action and quiet secondary tools.
fn mac_action(ui: &mut Ui, app: &mut App, icon: &str, label: &str, primary: bool) -> egui::Response {
    let w = if primary { if ui.available_width() < 240.0 { 38.0 } else { 104.0 } } else { 38.0 };
    let (rect, r) = ui.allocate_exact_size(vec2(w, 40.0), Sense::click());
    r.widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, label));
    let fill = if primary {
        app.theme.accent
    } else if r.hovered() {
        app.theme.hover
    } else {
        egui::Color32::TRANSPARENT
    };
    ui.painter().rect_filled(
        rect,
        if primary { egui::CornerRadius { nw: 7, sw: 7, ne: 0, se: 0 } } else { egui::CornerRadius::same(7) },
        fill,
    );
    if r.has_focus() {
        ui.painter().rect_stroke(
            rect.expand(2.0),
            8,
            Stroke::new(1.0, app.theme.accent),
            StrokeKind::Outside,
        );
    }
    let ink = if primary {
        if app.theme.dark { egui::Color32::from_rgb(24, 31, 40) } else { egui::Color32::WHITE }
    } else {
        app.theme.text
    };
    app.icons.paint(
        ui,
        Rect::from_center_size(
            if primary { rect.left_center() + vec2(22.0, 0.0) } else { rect.center() },
            vec2(20.0, 20.0),
        ),
        icon,
        ink,
    );
    if primary && w >= 60.0 {
        ui.painter().text(
            rect.left_center() + vec2(40.0, 0.0),
            Align2::LEFT_CENTER,
            label,
            egui::FontId::proportional(16.0),
            ink,
        );
    }
    r
}

fn mac_toolbar(ui: &mut Ui, app: &mut App, cid: u64) {
    let params = json!({"compose":cid});
    let width = ui.available_width();
    let native = cfg!(target_os = "macos")
        && !ui.ctx().embed_viewports()
        && ui.ctx().viewport_id() == viewport_id(cid);
    let left = 24;
    let title = app
        .s
        .composes
        .get(&cid)
        .map(|c| match c.kind {
            mailcraft_engine::compose::ComposeKind::Reply
            | mailcraft_engine::compose::ComposeKind::ReplyAll => "Reply",
            mailcraft_engine::compose::ComposeKind::Forward => "Forward",
            mailcraft_engine::compose::ComposeKind::Draft => "Draft",
            _ => "New message",
        })
        .unwrap_or("New message");
    let separate = app.s.composes.get(&cid).is_some_and(|c| !c.inline);
    if native {
        let rect = Rect::from_min_size(ui.cursor().min, vec2(width, 56.0));
        if ui.interact(rect, ui.id().with(("compose-drag", cid)), Sense::drag()).drag_started() {
            ui.ctx().send_viewport_cmd(ViewportCommand::StartDrag);
        }
    }
    egui::Frame::new()
        .fill(app.theme.chrome)
        .inner_margin(egui::Margin { left, right: 20, top: 16, bottom: 16 })
        .show(ui, |ui| {
            ui.set_width((width - f32::from(left) - 20.0).max(120.0));
            ui.horizontal(|ui| {
                if separate && crate::widgets::icon_only(ui, app, "close", "Close draft", true).clicked() {
                    let ctx = ui.ctx().clone();
                    request_close(&ctx, app, cid);
                }

                ui.label(
                    egui::RichText::new(title).font(app.theme.detail_font(19.0)).color(
                        if app.theme.terminal_style { app.theme.importance_low } else { app.theme.text },
                    ),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;
                    let send_options = egui::containers::menu::MenuButton::from_button(
                        egui::Button::new("")
                            .fill(app.theme.accent)
                            .corner_radius(egui::CornerRadius { nw: 0, sw: 0, ne: 7, se: 7 })
                            .min_size(vec2(32.0, 40.0)),
                    )
                    .ui(ui, |ui| {
                        ui.menu_button("All tools", |ui| crate::ribbon::compose_tools_menu(ui, app, cid));
                        ui.separator();
                        for (label, cmd) in [
                            ("Save draft", "compose.saveDraft"),
                            ("Show Bcc", "compose.showBcc"),
                            ("Check spelling", "review.spelling"),
                            ("Send later…", "compose.delayDelivery"),
                        ] {
                            if ui.button(label).clicked() {
                                let ctx = ui.ctx().clone();
                                if !ribbon_command(&ctx, app, cid, cmd, &params) {
                                    app.run(cmd, params.clone());
                                }
                                ui.close();
                            }
                        }
                    })
                    .0;
                    send_options.widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, "More"));
                    app.icons.paint(
                        ui,
                        Rect::from_center_size(send_options.rect.center(), vec2(16.0, 16.0)),
                        "chevron-down",
                        if app.theme.dark {
                            egui::Color32::from_rgb(24, 31, 40)
                        } else {
                            egui::Color32::WHITE
                        },
                    );
                    if mac_action(ui, app, "send", "Send", true)
                        .on_hover_text("Send message · ⌘Return")
                        .clicked()
                    {
                        app.run("compose.send", params.clone());
                    }

                    ui.spacing_mut().item_spacing.x = 10.0;
                    let id = Id::new(("mac-compose-format", cid));
                    let show = ui.ctx().data(|d| d.get_temp::<bool>(id)).unwrap_or(false);
                    if crate::widgets::icon_toggle(ui, app, "font-color", "Formatting", show, 38.0).clicked()
                    {
                        ui.ctx().data_mut(|d| d.insert_temp(id, !show));
                    }
                    if mac_action(ui, app, "attach", "Attach", false).clicked() {
                        let ctx = ui.ctx().clone();
                        if !ribbon_command(&ctx, app, cid, "compose.attachFile", &params) {
                            app.run("compose.attachFile", params.clone());
                        }
                    }
                });
            });
        });
    separator(ui, app);
}

/// A persistent signature choice, independent of whether formatting tools are shown.
fn mac_signature(ui: &mut Ui, app: &mut App, cid: u64) {
    let current = app.s.composes.get(&cid).and_then(|c| c.signature.as_ref()).map(|s| s.name.clone());
    let names: Vec<String> = app.s.mailbox.signatures.iter().map(|s| s.name.clone()).collect();
    let width = ui.available_width();
    let status = app
        .s
        .composes
        .get(&cid)
        .map(|c| {
            if c.dirty {
                "Unsaved changes"
            } else if c.draft.is_some() {
                "Saved locally"
            } else {
                "Draft"
            }
        })
        .unwrap_or("Draft");
    separator(ui, app);
    egui::Frame::new().fill(app.theme.chrome).inner_margin(egui::Margin::symmetric(24, 7)).show(ui, |ui| {
        ui.set_width((width - 48.0).max(180.0));
        ui.allocate_ui_with_layout(
            vec2((width - 48.0).max(180.0), 26.0),
            Layout::right_to_left(Align::Center),
            |ui| {
                let response = egui::ComboBox::from_id_salt(("mac-signature", cid))
                    .selected_text(current.as_deref().unwrap_or("None"))
                    .width(142.0)
                    .show_ui(ui, |ui| {
                        if ui.selectable_label(current.is_none(), "None").clicked() {
                            app.run("compose.signature", json!({"compose":cid,"name":"none"}));
                        }
                        for name in &names {
                            if ui.selectable_label(current.as_deref() == Some(name.as_str()), name).clicked()
                            {
                                app.run("compose.signature", json!({"compose":cid,"name":name}));
                            }
                        }
                        ui.separator();
                        if ui.button("Edit signatures…").clicked() {
                            let ctx = ui.ctx().clone();
                            ribbon_command(
                                &ctx,
                                app,
                                cid,
                                "options.open",
                                &json!({"compose":cid,"page":"signatures"}),
                            );
                        }
                    })
                    .response;
                response.widget_info(|| WidgetInfo::labeled(WidgetType::ComboBox, true, "Signature"));
                ui.label(egui::RichText::new("Signature").size(12.0).color(app.theme.text_muted));
                ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                    ui.label(egui::RichText::new(status).font(app.theme.detail_font(12.0)).color(
                        if app.theme.terminal_style && status == "Saved locally" {
                            app.theme.complete
                        } else {
                            app.theme.text_muted
                        },
                    ));
                });
            },
        );
    });
}

/// Everyday rich-text tools, in one slim row that wraps on smaller windows.
fn mac_formatting(ui: &mut Ui, app: &mut App, cid: u64) {
    if !ui.ctx().data(|d| d.get_temp::<bool>(Id::new(("mac-compose-format", cid)))).unwrap_or(false) {
        return;
    }
    let Some(c) = app.s.composes.get(&cid) else {
        return;
    };
    if c.format == mailcraft_model::BodyFormat::PlainText {
        ui.horizontal(|ui| {
            ui.add_space(24.0);
            ui.label(egui::RichText::new("Plain text").size(12.0).color(app.theme.text_muted));
            if ui.button("Use rich text").clicked() {
                app.run("compose.format", json!({"compose":cid,"kind":"html"}));
            }
        });
        separator(ui, app);
        return;
    }
    let style = c.style_at_caret();
    let family = style
        .font_family
        .clone()
        .or_else(|| c.body.base.font_family.clone())
        .unwrap_or_else(|| "Inter".to_owned());
    let pt = style.size_pt.or(c.body.base.size_pt).unwrap_or(11.0);
    let width = ui.available_width();
    egui::Frame::new()
        .fill(app.theme.chrome.gamma_multiply(0.65))
        .inner_margin(egui::Margin::symmetric(24, 7))
        .show(ui, |ui| {
            ui.set_width((width - 48.0).max(180.0));
            ui.spacing_mut().item_spacing.x = 5.0;
            ui.spacing_mut().interact_size.y = 26.0;
            ui.visuals_mut().widgets.inactive.bg_stroke = Stroke::NONE;
            ui.horizontal_wrapped(|ui| {
                egui::ComboBox::from_id_salt(("mac-font", cid))
                    .selected_text(&family)
                    .width(112.0)
                    .show_ui(ui, |ui| {
                        for font in
                            ["Inter", "Carlito", "Liberation Sans", "Liberation Serif", "Liberation Mono"]
                        {
                            if ui.selectable_label(font == family, font).clicked() {
                                app.run("format.font", json!({"compose":cid,"family":font}));
                            }
                        }
                    })
                    .response
                    .on_hover_text("Font family");
                egui::ComboBox::from_id_salt(("mac-font-size", cid))
                    .selected_text(format!("{pt:.0}"))
                    .width(42.0)
                    .show_ui(ui, |ui| {
                        for size in [9, 10, 11, 12, 14, 16, 18, 24, 36] {
                            if ui.selectable_label(pt == size as f32, size.to_string()).clicked() {
                                app.run("format.fontSize", json!({"compose":cid,"pt":size}));
                            }
                        }
                    })
                    .response
                    .on_hover_text("Font size");
                ui.add_space(7.0);
                for (icon, label, command, on) in [
                    ("bold", "Bold", "format.bold", style.bold == Some(true)),
                    ("italic", "Italic", "format.italic", style.italic == Some(true)),
                    ("underline", "Underline", "format.underline", style.underline == Some(true)),
                ] {
                    if crate::widgets::icon_toggle(ui, app, icon, label, on, 27.0).clicked() {
                        app.run(command, json!({"compose":cid}));
                    }
                }
                ui.add_space(7.0);
                ui.menu_button("Paragraph", |ui| {
                    for (label, command, extra) in [
                        ("Align left", "format.align", json!({"kind":"left"})),
                        ("Center", "format.align", json!({"kind":"center"})),
                        ("Align right", "format.align", json!({"kind":"right"})),
                        ("Bulleted list", "format.bullets", json!({})),
                        ("Numbered list", "format.numbering", json!({})),
                        ("Increase indent", "format.indent", json!({})),
                        ("Decrease indent", "format.outdent", json!({})),
                    ] {
                        if ui.button(label).clicked() {
                            let mut params = extra;
                            if let Some(object) = params.as_object_mut() {
                                object.insert("compose".to_owned(), json!(cid));
                            }
                            app.run(command, params);
                            ui.close();
                        }
                    }
                });
                ui.menu_button("Insert", |ui| {
                    for (label, command) in [
                        ("Link…", "insert.link"),
                        ("Table…", "insert.table"),
                        ("Picture…", "insert.picture"),
                        ("Horizontal line", "insert.horizontalLine"),
                    ] {
                        if ui.button(label).clicked() {
                            app.run(command, json!({"compose":cid}));
                            ui.close();
                        }
                    }
                });
                ui.menu_button("Text", |ui| {
                    for (label, command) in [
                        ("Font and color…", "format.fontDialog"),
                        ("Clear formatting", "format.clearFormatting"),
                    ] {
                        if ui.button(label).clicked() {
                            app.run(command, json!({"compose":cid}));
                            ui.close();
                        }
                    }
                });
            });
        });
    separator(ui, app);
}
