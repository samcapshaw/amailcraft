//! The Options window (plan/reference/options.md): 14 categories in a left list, each page a list of sections
//! and settings. Every setting is an engine setting (`mailcraft_engine::settings::DEFS`, read
//! with `Session::option`, written with `options.set` on OK and saved with the profile); the page layout is
//! generated from that schema, with the buttons and notes of each section added here. Buttons open the
//! matching dialogs or run the matching commands.

mod appearance;

use egui::{Context, RichText};
use serde_json::{Map, Value, json};

use super::common::{self, Dlg, Out, btn, footer, muted, primary, take_replies, window, with_state};
use crate::App;
use mailcraft_engine::settings::{self, Def, Kind, WEEKDAYS};

#[derive(Clone, Copy)]
enum Act {
    /// Open a dialog by id (params `{}`).
    Dialog(&'static str),
    /// Run a command with `{}`.
    Cmd(&'static str),
    /// Pick a folder into the given setting key (its path).
    PickFolder(&'static str),
    /// Not available yet: show this note.
    Info(&'static str),
}

use Act::*;

/// Page descriptions, in window order (ids and titles come from the engine).
const DESCRIPTIONS: [(&str, &str); 14] = [
    ("general", "General options for working with MailCraft."),
    ("mail", "Change the settings for messages you create and receive."),
    ("calendar", "Change the settings for calendars, meetings and time zones."),
    ("groups", "Options for working with groups."),
    ("people", "Change how you work with your contacts."),
    ("tasks", "Change the settings that track your tasks and to-do items."),
    ("search", "Change how items are searched."),
    ("language", "Set the display, editing and proofing languages."),
    ("accessibility", "Make MailCraft easier to use."),
    ("advanced", "More options for working with MailCraft."),
    ("customizeRibbon", "Choose how the ribbon looks."),
    ("quickAccessToolbar", "Choose the commands on the Quick Access Toolbar."),
    ("addIns", "View and manage add-ins."),
    ("trustCenter", "Keep your messages and computer safe."),
];

const NEEDS_SERVER: &str = "Needs a mail server; not available for local data files.";

/// Sections with no settings of their own, shown after another section: (page, after section, heading).
const EXTRA_SECTIONS: [(&str, &str, &str); 3] = [
    ("mail", "Compose messages", "MailCraft panes"),
    ("advanced", "Reminders", "Export"),
    ("addIns", "", "Add-ins"),
];

/// Buttons at the end of a section: (page, section, label, action).
const BUTTONS: &[(&str, &str, &str, Act)] = &[
    ("mail", "Compose messages", "Signatures...", Dialog("signatures")),
    ("mail", "Compose messages", "Stationery and Fonts...", Dialog("stationery")),
    ("mail", "MailCraft panes", "Reading Pane...", Dialog("readingPaneOptions")),
    ("mail", "Conversation Clean Up", "Browse...", PickFolder("mail.cleanup.folder")),
    ("mail", "Send messages", "Empty Auto-Complete List", Cmd("compose.emptyAutoComplete")),
    ("calendar", "Calendar options", "Add Holidays...", Info("Holiday calendars are not bundled yet.")),
    ("calendar", "Calendar options", "Free/Busy Options...", Info(NEEDS_SERVER)),
    ("tasks", "Task options", "Quick Click...", Cmd("mail.setQuickClickFlag")),
    ("search", "Results", "Indexing Options...", Cmd("search.searchTools")),
    ("accessibility", "Application display options", "Stationery and Fonts...", Dialog("stationery")),
    ("advanced", "MailCraft panes", "Reading Pane...", Dialog("readingPaneOptions")),
    ("advanced", "Start and exit", "Browse...", PickFolder("advanced.startFolder")),
    ("advanced", "AutoArchive", "AutoArchive Settings...", Cmd("folder.autoArchiveSettings")),
    ("advanced", "Export", "Export", Cmd("file.openExport")),
    ("advanced", "Send and receive", "Send/Receive...", Cmd("sendReceive.showProgress")),
    ("advanced", "Other", "Quick Click...", Cmd("categories.setQuickClick")),
    ("customizeRibbon", "Ribbon", "Ribbon Display Options...", Cmd("app.toggleRibbonMode")),
    (
        "quickAccessToolbar",
        "Quick Access Toolbar",
        "Customize Quick Access Toolbar...",
        Cmd("app.quickAccess"),
    ),
];

/// Notes under a section: (page, section, text).
const NOTES: &[(&str, &str, &str)] = &[
    (
        "groups",
        "User Interface options",
        "Groups are a service of hosted mail systems; MailCraft has none to show yet.",
    ),
    ("customizeRibbon", "Ribbon", "Adding tabs, groups and commands to the ribbon is not available yet."),
    ("addIns", "Add-ins", "No add-ins are installed. MailCraft does not load add-ins."),
    (
        "trustCenter",
        "Security & more",
        "MailCraft never loads remote content in messages unless you allow it.",
    ),
];

/// Choice settings shown as radio buttons (the rest are drop-downs).
const RADIO_KEYS: [&str; 7] = [
    "general.multipleDisplays",
    "general.cloudAttach",
    "groups.attach",
    "mail.tracking.readReceiptResponse",
    "search.scope",
    "language.translate",
    "accessibility.warnings",
];

/// Settings greyed out until the check box they belong to is ticked: (setting, check box).
const DEPENDS: [(&str, &str); 4] = [
    ("mail.reply.prefaceName", "mail.reply.prefaceComments"),
    ("mail.save.autoSaveMinutes", "mail.save.autoSave"),
    ("mail.send.expireDays", "mail.send.expire"),
    ("mail.tracking.moveReceiptsTo", "mail.tracking.moveReceipts"),
];

/// Whole-number settings holding a UTC offset in minutes (shown as a time zone list).
const TZ_KEYS: [&str; 3] =
    ["calendar.timeZone", "calendar.secondTimeZoneOffset", "calendar.thirdTimeZoneOffset"];

/// UTC offsets offered for the time zone (label, minutes).
const TIME_ZONES: [(&str, i64); 23] = [
    ("(UTC-12:00)", -720),
    ("(UTC-11:00)", -660),
    ("(UTC-10:00) Hawaii", -600),
    ("(UTC-09:00) Alaska", -540),
    ("(UTC-08:00) Pacific Time", -480),
    ("(UTC-07:00) Mountain Time", -420),
    ("(UTC-06:00) Central Time", -360),
    ("(UTC-05:00) Eastern Time", -300),
    ("(UTC-04:00) Atlantic Time", -240),
    ("(UTC-03:30) Newfoundland", -210),
    ("(UTC-03:00) Brasilia", -180),
    ("(UTC-01:00) Azores", -60),
    ("(UTC) Coordinated Universal Time", 0),
    ("(UTC+01:00) Central Europe", 60),
    ("(UTC+02:00) Eastern Europe", 120),
    ("(UTC+03:00) Moscow", 180),
    ("(UTC+04:00) Gulf", 240),
    ("(UTC+05:30) India", 330),
    ("(UTC+07:00) Bangkok", 420),
    ("(UTC+08:00) Beijing, Perth", 480),
    ("(UTC+09:00) Tokyo", 540),
    ("(UTC+10:00) Sydney", 600),
    ("(UTC+12:00) Auckland", 720),
];

/// Label of a UTC offset (one of [`TIME_ZONES`], else "(UTC±hh:mm)").
fn tz_label(min: i64) -> String {
    if let Some((l, _)) = TIME_ZONES.iter().find(|(_, m)| *m == min) {
        return (*l).to_owned();
    }
    let sign = if min < 0 { '-' } else { '+' };
    let a = min.unsigned_abs();
    format!("(UTC{sign}{:02}:{:02})", a / 60, a % 60)
}

/// One section of a page: heading, its settings, buttons and notes.
struct Section {
    heading: &'static str,
    defs: Vec<&'static Def>,
}

/// The sections of a page in order.
fn sections(page: &str) -> Vec<Section> {
    let mut out: Vec<Section> = Vec::new();
    for (heading, d) in settings::page_defs(page) {
        match out.iter_mut().find(|s| s.heading == heading) {
            Some(s) => s.defs.push(d),
            None => out.push(Section { heading, defs: vec![d] }),
        }
    }
    for (p, after, heading) in EXTRA_SECTIONS {
        if p != page || out.iter().any(|s| s.heading == heading) {
            continue;
        }
        let at = out.iter().position(|s| s.heading == after).map_or(out.len(), |i| i + 1);
        out.insert(at, Section { heading, defs: Vec::new() });
    }
    out
}

#[derive(Clone, Default)]
struct OptState {
    page: usize,
    /// Section to scroll to when the window opens (`options.open` with an alias such as `editorAdvanced`).
    scroll_to: Option<String>,
    /// Edited values (key → value); untouched keys show their current value.
    edits: Map<String, Value>,
    note: String,
    /// Settings refused on OK (key: reason).
    errors: Vec<String>,
}

fn current(app: &App, st: &OptState, d: &Def) -> Value {
    if let Some(v) = st.edits.get(d.key) {
        return v.clone();
    }
    app.s.option(d.key).unwrap_or_else(|| d.default_value())
}

/// Current (edited or stored) value of a setting by key.
fn current_key(app: &App, st: &OptState, key: &str) -> Value {
    settings::def(key).map_or(Value::Null, |d| current(app, st, d))
}

/// Whether a setting can be edited now: the ones in [`DEPENDS`] follow their check box, the plain-text
/// prefix is used only when replies or forwards prefix each line.
fn enabled(app: &App, st: &OptState, key: &str) -> bool {
    if let Some((_, parent)) = DEPENDS.iter().find(|(k, _)| *k == key) {
        return current_key(app, st, parent).as_bool().unwrap_or(false);
    }
    if key == "mail.reply.plainPrefix" {
        return ["mail.reply.replyText", "mail.reply.forwardText"]
            .iter()
            .any(|k| current_key(app, st, k).as_str() == Some("prefix"));
    }
    true
}

pub(crate) fn show(ctx: &Context, app: &mut App, d: &Dlg<'_>) -> Out {
    let uid = d.uid;
    let want = d.str("page");
    let section = d.str("section");
    let init = |_: &App| OptState {
        page: settings::PAGES.iter().position(|(id, _)| *id == want).unwrap_or(0),
        scroll_to: (!section.is_empty()).then(|| section.clone()),
        ..Default::default()
    };
    with_state(ctx, app, uid, init, |st, app| {
        for (k, v) in take_replies(ctx, uid) {
            if let Some(key) = k.strip_prefix("folder:")
                && let Some(f) = v.as_u64()
            {
                st.edits
                    .insert(key.to_owned(), json!(app.s.mailbox.folder_path(mailcraft_model::FolderId(f))));
            }
        }
        if app.mac_style || app.theme_chooser_enabled {
            return mac_options(ctx, app, d, uid, st);
        }
        window(ctx, app, d, "MailCraft Options", 820.0, |ui, app| {
            let mut out = Out::Keep;
            let h = (ctx.content_rect().height() - 170.0).clamp(240.0, 560.0);
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    ui.set_width(170.0);
                    for (i, (_, title)) in settings::PAGES.iter().enumerate() {
                        if ui
                            .add(
                                egui::Button::selectable(st.page == i, *title)
                                    .min_size(egui::vec2(166.0, 24.0)),
                            )
                            .clicked()
                        {
                            st.page = i;
                            st.note.clear();
                        }
                    }
                });
                ui.separator();
                ui.vertical(|ui| {
                    let Some((pid, _)) = settings::PAGES.get(st.page).copied() else { return };
                    let desc = DESCRIPTIONS.iter().find(|(p, _)| *p == pid).map_or("", |(_, t)| *t);
                    ui.label(RichText::new(desc).strong().size(15.0));
                    egui::ScrollArea::vertical()
                        .id_salt(("opts", uid, pid))
                        .max_height(h)
                        .min_scrolled_height(h)
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            if let Some(o) = page_ui(ui, app, uid, st, pid) {
                                out = o;
                            }
                        });
                });
            });
            if !st.note.is_empty() {
                muted(ui, app, &st.note);
            }
            for e in &st.errors {
                ui.label(RichText::new(e).color(app.theme.danger));
            }
            if !matches!(out, Out::Keep) {
                return out;
            }
            footer(ui, |ui| {
                if btn(ui, "Cancel") {
                    return Out::Close;
                }
                if primary(ui, app, true, "OK") {
                    st.errors = apply(ctx, app, st);
                    if st.errors.is_empty() {
                        return Out::Close;
                    }
                }
                Out::Keep
            })
        })
    })
}

/// Mac settings keep navigation separate from grouped, consistently aligned forms.
fn mac_options(ctx: &Context, app: &mut App, d: &Dlg<'_>, uid: u64, st: &mut OptState) -> Out {
    window(ctx, app, d, "Settings", 880.0, |ui, app| {
        let mut out = Out::Keep;
        let card = app.theme.card_surface();
        let edge = app.theme.control_edge();
        let height = (ctx.content_rect().height() - 230.0).clamp(240.0, 490.0);
        ui.add_space(8.0);
        ui.horizontal_top(|ui| {
            egui::Frame::new().fill(app.theme.chrome).corner_radius(10).inner_margin(10).show(ui, |ui| {
                ui.vertical(|ui| {
                    ui.set_width(170.0);
                    ui.set_min_height(height + 60.0);
                    ui.label(RichText::new("MAILCRAFT").size(10.0).color(app.theme.text_muted));
                    ui.add_space(9.0);
                    for (i, (_, title)) in settings::PAGES.iter().enumerate() {
                        if i == 7 || i == 10 {
                            ui.add_space(12.0);
                        }
                        let selected = st.page == i;
                        let (rect, r) = ui.allocate_exact_size(egui::vec2(170.0, 28.0), egui::Sense::click());
                        if selected || r.hovered() {
                            if selected {
                                ui.painter().rect_filled(
                                    rect.translate(egui::vec2(0.0, 1.0)),
                                    6,
                                    egui::Color32::from_black_alpha(if app.theme.dark { 25 } else { 10 }),
                                );
                            }
                            ui.painter().rect_filled(
                                rect,
                                6,
                                if selected {
                                    if app.theme.custom_surfaces { app.theme.selected } else { card }
                                } else {
                                    app.theme.hover
                                },
                            );
                        }
                        let icon = match i {
                            1 => "mail",
                            2 => "calendar",
                            3 => "people",
                            4 => "person",
                            5 => "tasks",
                            6 => "search",
                            9 => "ellipsis",
                            10 => "ribbon-display",
                            11 => "quick-steps",
                            12 => "plus",
                            13 => "private",
                            _ => "settings",
                        };
                        app.icons.paint(
                            ui,
                            egui::Rect::from_center_size(
                                rect.left_center() + egui::vec2(18.0, 0.0),
                                egui::vec2(14.0, 14.0),
                            ),
                            icon,
                            if selected { app.theme.text } else { app.theme.text_muted },
                        );
                        ui.painter().text(
                            rect.left_center() + egui::vec2(35.0, 0.0),
                            egui::Align2::LEFT_CENTER,
                            title,
                            egui::FontId::proportional(12.0),
                            if selected { app.theme.text } else { app.theme.text_muted },
                        );
                        r.widget_info(|| {
                            egui::WidgetInfo::selected(
                                egui::WidgetType::SelectableLabel,
                                true,
                                selected,
                                *title,
                            )
                        });
                        if r.clicked() {
                            st.page = i;
                            st.scroll_to = None;
                            st.note.clear();
                        }
                    }
                    ui.add_space(12.0);
                    if ui.add_sized([170.0, 28.0], egui::Button::new("Accounts…")).clicked() {
                        out = Out::open("accountSettings", json!({}));
                    }
                });
            });
            ui.add_space(16.0);
            ui.vertical(|ui| {
                ui.set_width((ui.available_width() - 8.0).max(250.0));
                let Some((pid, title)) = settings::PAGES.get(st.page).copied() else {
                    return;
                };
                ui.add_space(3.0);
                ui.label(RichText::new(title).size(25.0));
                let desc = DESCRIPTIONS.iter().find(|(p, _)| *p == pid).map_or("", |(_, t)| *t);
                ui.label(RichText::new(desc).size(12.0).color(app.theme.text_muted));
                ui.add_space(16.0);
                egui::ScrollArea::vertical()
                    .id_salt(("mac-settings", uid, pid))
                    .max_height(height)
                    .min_scrolled_height(height)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        if pid == "general" {
                            appearance::chooser(ui, app, st);
                        }
                        let mut groups = sections(pid);
                        groups.retain(|section| section.heading != "Appearance");
                        if pid == "general" {
                            groups.sort_by_key(|sec| match sec.heading {
                                "Personalize your copy of MailCraft" => 0,
                                "User Interface options" => 1,
                                "Start up options" => 2,
                                _ => 3,
                            });
                        }
                        for sec in groups {
                            if st.scroll_to.as_deref() == Some(sec.heading) {
                                ui.scroll_to_cursor(Some(egui::Align::TOP));
                                st.scroll_to = None;
                            }
                            ui.add_space(4.0);
                            ui.label(
                                RichText::new(match sec.heading {
                                    "Personalize your copy of MailCraft" => "Profile & appearance",
                                    "User Interface options" => "Interface",
                                    "Start up options" => "Startup",
                                    other => other,
                                })
                                .size(12.0)
                                .strong()
                                .color(app.theme.text_muted),
                            );
                            ui.add_space(7.0);
                            egui::Frame::new()
                                .fill(card)
                                .stroke(egui::Stroke::new(0.6, edge))
                                .corner_radius(10)
                                .shadow(egui::Shadow {
                                    offset: [0, 2],
                                    blur: 7,
                                    spread: 0,
                                    color: egui::Color32::from_black_alpha(if app.theme.dark {
                                        24
                                    } else {
                                        9
                                    }),
                                })
                                .inner_margin(16)
                                .show(ui, |ui| {
                                    ui.set_width((ui.available_width() - 4.0).max(200.0));
                                    ui.spacing_mut().item_spacing.y = 5.0;
                                    for (i, def) in
                                        sec.defs.iter().filter(|d| d.key != "general.theme").enumerate()
                                    {
                                        if i > 0 {
                                            let (r, _) = ui.allocate_exact_size(
                                                egui::vec2(ui.available_width(), 1.0),
                                                egui::Sense::hover(),
                                            );
                                            ui.painter().hline(
                                                r.x_range(),
                                                r.center().y,
                                                egui::Stroke::new(0.5, edge.gamma_multiply(0.7)),
                                            );
                                        }
                                        let on = enabled(app, st, def.key);
                                        ui.add_enabled_ui(on, |ui| setting_ui(ui, app, uid, st, def));
                                    }
                                    for (_, _, text) in
                                        NOTES.iter().filter(|(p, s, _)| *p == pid && *s == sec.heading)
                                    {
                                        muted(ui, app, text);
                                    }
                                    if BUTTONS.iter().any(|(p, s, _, _)| *p == pid && *s == sec.heading) {
                                        ui.horizontal_wrapped(|ui| {
                                            for (_, _, label, action) in BUTTONS
                                                .iter()
                                                .filter(|(p, s, _, _)| *p == pid && *s == sec.heading)
                                            {
                                                if ui.button(*label).clicked() {
                                                    match action {
                                                        Dialog(name) => out = Out::open(name, json!({})),
                                                        Cmd(id) => {
                                                            app.run(id, json!({}));
                                                        }
                                                        PickFolder(key) => {
                                                            out = Out::open(
                                                                "pickFolder",
                                                                common::child(
                                                                    uid,
                                                                    &format!("folder:{key}"),
                                                                    json!({"pick": "any"}),
                                                                ),
                                                            )
                                                        }
                                                        Info(note) => st.note = (*note).into(),
                                                    }
                                                }
                                            }
                                        });
                                    }
                                });
                            ui.add_space(17.0);
                        }
                    });
            });
        });
        if !st.note.is_empty() {
            muted(ui, app, &st.note);
        }
        for error in &st.errors {
            ui.colored_label(app.theme.danger, error);
        }
        if !matches!(out, Out::Keep) {
            return out;
        }
        ui.add_space(16.0);
        ui.separator();
        ui.add_space(5.0);
        footer(ui, |ui| {
            if btn(ui, "Cancel") {
                return Out::Close;
            }
            if primary(ui, app, true, "Save Changes") {
                st.errors = apply(ctx, app, st);
                if st.errors.is_empty() {
                    return Out::Close;
                }
            }
            Out::Keep
        })
    })
}

fn mac_setting(ui: &mut egui::Ui, app: &App, uid: u64, st: &mut OptState, d: &'static Def) {
    let cur = current(app, st, d);
    let mut edit = None;
    let width = ui.available_width();
    ui.horizontal(|ui| {
        let label = ui
            .allocate_ui_with_layout(
                egui::vec2(width * 0.54, 34.0),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    ui.set_min_width(width * 0.54);
                    ui.set_max_width(width * 0.54);
                    ui.set_min_height(34.0);
                    ui.add(egui::Label::new(d.label.trim_end_matches(':')).wrap())
                },
            )
            .inner;
        let control_w = ui.available_width().max(60.0);
        ui.add_enabled_ui(true, |ui| {
            ui.set_width(control_w);
            match d.kind {
                Kind::Bool(default) => {
                    let value = cur.as_bool().unwrap_or(default);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let (rect, response) =
                            ui.allocate_exact_size(egui::vec2(32.0, 19.0), egui::Sense::click());
                        let toggled = if response.clicked() {
                            edit = Some(json!(!value));
                            !value
                        } else {
                            value
                        };
                        let track = if toggled {
                            app.theme.accent
                        } else if app.theme.dark {
                            egui::Color32::from_rgb(75, 76, 82)
                        } else {
                            egui::Color32::from_rgb(204, 208, 215)
                        };
                        ui.painter().rect_filled(rect, 10, track);
                        let x = if toggled { rect.right() - 9.5 } else { rect.left() + 9.5 };
                        ui.painter().circle_filled(
                            egui::pos2(x, rect.center().y + 0.5),
                            7.5,
                            egui::Color32::from_black_alpha(18),
                        );
                        ui.painter().circle_filled(egui::pos2(x, rect.center().y), 7.0, egui::Color32::WHITE);
                        if response.has_focus() {
                            ui.painter().rect_stroke(
                                rect.expand(2.0),
                                11,
                                egui::Stroke::new(1.0, app.theme.accent),
                                egui::StrokeKind::Outside,
                            );
                        }
                        response.widget_info(|| {
                            egui::WidgetInfo::selected(
                                egui::WidgetType::Checkbox,
                                ui.is_enabled(),
                                toggled,
                                d.label,
                            )
                        });
                    });
                }
                Kind::Choice(default, _) if d.key == "general.theme" => {
                    let value = cur.as_str().unwrap_or(default);
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 3.0;
                        let w = ((control_w - 6.0) / 3.0).max(36.0);
                        for (id, name) in [("light", "Light"), ("dark", "Dark"), ("system", "Auto")] {
                            let selected = value == id;
                            let response = ui.add_sized([w, 29.0], egui::Button::selectable(selected, name));
                            if response.clicked() {
                                edit = Some(json!(id));
                            }
                        }
                    });
                }
                Kind::Choice(default, choices) => {
                    let value = cur.as_str().unwrap_or(default);
                    let mut index = choices.iter().position(|(id, _)| *id == value).unwrap_or(0);
                    let labels: Vec<_> = choices.iter().map(|(_, label)| *label).collect();
                    if common::combo_idx(
                        ui,
                        ("mac-option", uid, d.key),
                        &mut index,
                        &labels,
                        control_w.min(245.0),
                    ) && let Some((value, _)) = choices.get(index)
                    {
                        edit = Some(json!(value));
                    }
                }
                Kind::Int(default, min, max) => {
                    let mut value = cur.as_i64().unwrap_or(default);
                    if ui
                        .add(egui::DragValue::new(&mut value).range(min..=max))
                        .labelled_by(label.id)
                        .changed()
                    {
                        edit = Some(json!(value));
                    }
                }
                Kind::Text(default, max) => {
                    let mut value = cur.as_str().unwrap_or(default).to_owned();
                    if ui
                        .add(
                            egui::TextEdit::singleline(&mut value)
                                .desired_width(control_w)
                                .margin(egui::Margin::symmetric(8, 6))
                                .char_limit(max),
                        )
                        .labelled_by(label.id)
                        .changed()
                    {
                        edit = Some(json!(value));
                    }
                }
                Kind::Time(default) | Kind::TimeRange(default) => {
                    let mut value = cur.as_str().unwrap_or(default).to_owned();
                    if ui
                        .add(
                            egui::TextEdit::singleline(&mut value)
                                .desired_width(control_w)
                                .margin(egui::Margin::symmetric(8, 6))
                                .char_limit(16),
                        )
                        .labelled_by(label.id)
                        .changed()
                    {
                        edit = Some(json!(value));
                    }
                }
                _ => {}
            }
        });
    });
    if let Some(value) = edit {
        st.edits.insert(d.key.into(), value);
    }
}

/// Applies the edits; returns the settings that were refused.
fn apply(ctx: &Context, app: &mut App, st: &OptState) -> Vec<String> {
    let mut errors = Vec::new();
    for key in settings::PALETTE_KEYS {
        if let Some(value) = st.edits.get(*key)
            && let Some(def) = settings::def(key)
            && let Err(error) = settings::normalize(def, value)
        {
            errors.push(error.to_string());
        }
    }
    if !errors.is_empty() {
        return errors;
    }
    for (k, v) in &st.edits {
        if app.s.option(k).as_ref() == Some(v) {
            continue;
        }
        if let Err(e) = app.s.run("options.set", &json!({"key": k, "value": v})) {
            let label = settings::def(k).map_or(k.as_str(), |d| d.label).trim_end_matches(':').to_owned();
            errors.push(format!("{label}: {e}"));
        }
    }
    common::sync_mirror(ctx, app);
    errors
}

fn page_ui(ui: &mut egui::Ui, app: &mut App, uid: u64, st: &mut OptState, pid: &str) -> Option<Out> {
    let mut out = None;
    for sec in sections(pid) {
        if st.scroll_to.as_deref() == Some(sec.heading) {
            // Opened at this section (e.g. Set Default Paste): bring its heading to the top once.
            ui.scroll_to_cursor(Some(egui::Align::TOP));
            st.scroll_to = None;
        }
        common::heading(ui, app, sec.heading);
        for d in &sec.defs {
            let on = enabled(app, st, d.key);
            ui.add_enabled_ui(on, |ui| setting_ui(ui, app, uid, st, d));
        }
        for (_, _, text) in NOTES.iter().filter(|(p, s, _)| *p == pid && *s == sec.heading) {
            muted(ui, app, text);
        }
        for (_, _, label, act) in BUTTONS.iter().filter(|(p, s, _, _)| *p == pid && *s == sec.heading) {
            if ui.button(*label).clicked() {
                match act {
                    Dialog(name) => out = Some(Out::open(name, json!({}))),
                    Cmd(id) => {
                        app.run(id, json!({}));
                    }
                    PickFolder(key) => {
                        out = Some(Out::open(
                            "pickFolder",
                            common::child(uid, &format!("folder:{key}"), json!({"pick": "any"})),
                        ));
                    }
                    Info(t) => st.note = (*t).to_owned(),
                }
            }
        }
    }
    out
}

fn setting_ui(ui: &mut egui::Ui, app: &App, uid: u64, st: &mut OptState, d: &'static Def) {
    if app.mac_style
        && !TZ_KEYS.contains(&d.key)
        && !matches!(d.kind, Kind::Days(_) | Kind::List(_) | Kind::Object(_))
    {
        mac_setting(ui, app, uid, st, d);
        return;
    }
    let cur = current(app, st, d);
    let mut edit: Option<Value> = None;
    match d.kind {
        Kind::Bool(def) => {
            let mut b = cur.as_bool().unwrap_or(def);
            if ui.checkbox(&mut b, d.label).changed() {
                edit = Some(json!(b));
            }
        }
        Kind::Int(def, min, max) if TZ_KEYS.contains(&d.key) => {
            let n = cur.as_i64().unwrap_or(def).clamp(min, max);
            let mut label = tz_label(n);
            let mut choices: Vec<String> = TIME_ZONES.iter().map(|(l, _)| (*l).to_owned()).collect();
            if !choices.contains(&label) {
                choices.insert(0, label.clone());
            }
            let refs: Vec<&str> = choices.iter().map(String::as_str).collect();
            ui.horizontal(|ui| {
                ui.label(d.label);
                if common::combo(ui, ("otz", uid, d.key), &mut label, &refs, 260.0)
                    && let Some((_, m)) = TIME_ZONES.iter().find(|(l, _)| *l == label)
                {
                    edit = Some(json!(m));
                }
            });
        }
        Kind::Int(def, min, max) => {
            let mut n = cur.as_i64().unwrap_or(def);
            ui.horizontal(|ui| {
                ui.label(d.label);
                if ui.add(egui::DragValue::new(&mut n).range(min..=max)).changed() {
                    edit = Some(json!(n));
                }
            });
        }
        Kind::Choice(def, choices) => {
            let id = cur.as_str().unwrap_or(def);
            if RADIO_KEYS.contains(&d.key) {
                ui.label(d.label);
                ui.indent((uid, d.key), |ui| {
                    for (cid, clabel) in choices {
                        if ui.radio(id == *cid, *clabel).clicked() {
                            edit = Some(json!(cid));
                        }
                    }
                });
            } else {
                let labels: Vec<&str> = choices.iter().map(|(_, l)| *l).collect();
                let mut i = choices.iter().position(|(c, _)| *c == id).unwrap_or(0);
                ui.horizontal(|ui| {
                    ui.label(d.label);
                    if common::combo_idx(ui, ("oc", uid, d.key), &mut i, &labels, 260.0)
                        && let Some((c, _)) = choices.get(i)
                    {
                        edit = Some(json!(c));
                    }
                });
            }
        }
        Kind::Text(def, max) => text_row(ui, d, &cur, def, 220.0, max, &mut edit),
        Kind::Time(def) | Kind::TimeRange(def) => text_row(ui, d, &cur, def, 110.0, 16, &mut edit),
        Kind::Days(def) => {
            let days: Vec<String> = cur
                .as_array()
                .map(|a| a.iter().filter_map(Value::as_str).map(str::to_owned).collect())
                .unwrap_or_else(|| def.iter().map(|s| (*s).to_owned()).collect());
            ui.horizontal_wrapped(|ui| {
                ui.label(d.label);
                for (id, name) in WEEKDAYS {
                    let mut on = days.iter().any(|x| x == id);
                    let short: String = name.chars().take(3).collect();
                    if ui.checkbox(&mut on, short).changed() {
                        let next: Vec<&str> = WEEKDAYS
                            .iter()
                            .map(|(w, _)| *w)
                            .filter(|w| if *w == id { on } else { days.iter().any(|x| x == w) })
                            .collect();
                        edit = Some(json!(next));
                    }
                }
            });
        }
        Kind::List(_) | Kind::Object(_) => {}
    }
    if let Some(v) = edit {
        st.edits.insert(d.key.to_owned(), v);
    }
}

/// A labelled one-line text field (text, time, time range); values are checked by the engine on OK.
fn text_row(
    ui: &mut egui::Ui,
    d: &Def,
    cur: &Value,
    def: &str,
    width: f32,
    max: usize,
    edit: &mut Option<Value>,
) {
    let mut t = cur.as_str().unwrap_or(def).to_owned();
    ui.horizontal(|ui| {
        ui.label(d.label);
        let te = egui::TextEdit::singleline(&mut t).desired_width(width).char_limit(max);
        if ui.add(te).changed() {
            *edit = Some(json!(t));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::Harness;
    use egui_kittest::kittest::Queryable;
    use mailcraft_engine::sample::sample_session;

    fn harness(app: App) -> Harness<'static, App> {
        Harness::builder().with_size(egui::Vec2::new(1200.0, 900.0)).build_ui_state(
            |ui, app: &mut App| {
                let ctx = ui.ctx().clone();
                crate::dialogs::dialogs(&ctx, app);
            },
            app,
        )
    }

    #[test]
    fn every_page_renders_from_the_engine_schema() {
        for (pid, title) in settings::PAGES {
            let mut app = App::new(sample_session());
            app.run("options.open", json!({"page": pid})).unwrap();
            let mut h = harness(app);
            h.run_steps(2);
            assert!(h.query_all_by_label(title).next().is_some(), "{pid}");
            assert!(!sections(pid).is_empty(), "{pid} has no sections");
        }
    }

    #[test]
    fn ok_writes_engine_settings_and_cancel_does_not() {
        let mut app = App::new(sample_session());
        app.run("options.open", json!({"page": "mail"})).unwrap();
        let mut h = harness(app);
        h.run_steps(2);
        h.get_by_label("Always show the Bcc box").click();
        h.run_steps(2);
        assert!(!h.state().s.settings.bool("mail.compose.showBcc"), "applied only on OK");
        h.get_by_label("OK").click();
        h.run_steps(2);
        assert!(h.state().s.settings.bool("mail.compose.showBcc"));
        let mut app = h.into_state();
        app.run("options.open", json!({"page": "search"})).unwrap();
        let mut h = harness(app);
        h.run_steps(2);
        h.get_by_label("All mailboxes").click();
        h.run_steps(2);
        h.get_by_label("Cancel").click();
        h.run_steps(2);
        assert_eq!(h.state().s.settings.text("search.scope"), "currentFolder");
    }

    #[test]
    fn set_default_paste_scrolls_to_the_paste_defaults() {
        // The paste defaults sit at the end of the long Mail page: below the window unless it scrolls there.
        let y = |page: &str| {
            let mut app = App::new(sample_session());
            app.run("options.open", json!({"page": page})).unwrap();
            let mut h = harness(app);
            h.run_steps(4);
            h.query_by_label("Pasting from other programs:").map(|n| n.rect().top())
        };
        let (scrolled, plain) = (y("editorAdvanced"), y("mail"));
        let scrolled = scrolled.unwrap_or(f32::MAX);
        assert!(scrolled < 900.0, "{scrolled}");
        // Without a section the page opens at its top: the row is far down (or not laid out at all).
        assert!(plain.is_none_or(|p| p > scrolled + 100.0), "{plain:?} vs {scrolled}");
    }

    #[test]
    fn dependent_settings_follow_their_check_box() {
        let app = App::new(sample_session());
        let st = OptState::default();
        assert!(!enabled(&app, &st, "mail.reply.prefaceName"));
        assert!(!enabled(&app, &st, "mail.reply.plainPrefix"), "only for Prefix each line");
        assert!(enabled(&app, &st, "mail.save.autoSaveMinutes"));
        let mut st = OptState::default();
        st.edits.insert("mail.reply.prefaceComments".into(), json!(true));
        st.edits.insert("mail.reply.forwardText".into(), json!("prefix"));
        st.edits.insert("mail.save.autoSave".into(), json!(false));
        assert!(enabled(&app, &st, "mail.reply.prefaceName"));
        assert!(enabled(&app, &st, "mail.reply.plainPrefix"));
        assert!(!enabled(&app, &st, "mail.save.autoSaveMinutes"));
        assert!(enabled(&app, &st, "mail.compose.showBcc"));
    }

    #[test]
    fn sections_and_time_zone_labels() {
        let mail = sections("mail");
        let panes = mail.iter().position(|s| s.heading == "MailCraft panes").unwrap();
        assert_eq!(mail.get(panes - 1).map(|s| s.heading), Some("Compose messages"));
        assert!(
            sections("accessibility").iter().any(|s| s.defs.iter().any(|d| d.key == "general.screenTips"))
        );
        assert_eq!(tz_label(-300), "(UTC-05:00) Eastern Time");
        assert_eq!(tz_label(345), "(UTC+05:45)");
        assert_eq!(tz_label(-30), "(UTC-00:30)");
    }
    #[test]
    fn invalid_palette_keeps_all_pending_edits_unapplied() {
        let mut app = App::new(sample_session());
        let mut state = OptState::default();
        state.edits.insert("general.interface".into(), json!("omarchy"));
        state.edits.insert("general.accentColor".into(), json!("#oops"));
        assert!(!apply(&Context::default(), &mut app, &state).is_empty());
        assert_eq!(app.s.settings.text("general.interface"), "apple");
    }

    #[test]
    fn appearance_chooser_saves_and_cancel_discards_profile() {
        let mut app = App::new(sample_session());
        app.enable_mac_style();
        app.run("options.open", json!({"page":"general"})).unwrap();
        let mut h = harness(app);
        h.get_by_label("Omarchy").click();
        h.run_steps(2);
        assert_eq!(h.state().s.settings.text("general.interface"), "apple");
        h.get_by_label("Cancel").click();
        h.run_steps(2);
        assert_eq!(h.state().s.settings.text("general.interface"), "apple");
        let mut app = h.into_state();
        app.run("options.open", json!({"page":"general"})).unwrap();
        let mut h = harness(app);
        h.get_by_label("Outlook layout").click();
        h.run_steps(2);
        h.get_by_label("Save Changes").click();
        h.run_steps(2);
        assert_eq!(h.state().s.settings.text("general.interface"), "outlook");
        // The same chooser remains reachable with the classic layout selected.
        let mut app = h.into_state();
        app.mac_style = false;
        app.run("options.open", json!({"page":"general"})).unwrap();
        let mut h = harness(app);
        h.get_by_label("Apple").click();
        h.run_steps(2);
        h.get_by_label("Save Changes").click();
        h.run_steps(2);
        assert_eq!(h.state().s.settings.text("general.interface"), "apple");
    }

    #[test]
    fn mac_settings_save_and_cancel_preserve_engine_behavior() {
        let mut app = App::new(sample_session());
        app.enable_mac_style();
        app.run("options.open", json!({"page": "mail"})).unwrap();
        let mut h = harness(app);
        h.run_steps(2);
        h.get_by_role_and_label(egui::accesskit::Role::CheckBox, "Always show the Bcc box").click();
        h.run_steps(2);
        assert!(!h.state().s.settings.bool("mail.compose.showBcc"));
        h.get_by_label("Save Changes").click();
        h.run_steps(2);
        assert!(h.state().s.settings.bool("mail.compose.showBcc"));
        let mut app = h.into_state();
        app.run("options.open", json!({"page": "mail"})).unwrap();
        let mut h = harness(app);
        h.run_steps(2);
        h.get_by_role_and_label(egui::accesskit::Role::CheckBox, "Always show the Bcc box").click();
        h.run_steps(2);
        h.get_by_label("Cancel").click();
        h.run_steps(2);
        assert!(h.state().s.settings.bool("mail.compose.showBcc"));
    }
}
