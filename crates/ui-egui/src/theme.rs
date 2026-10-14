//! Colour and size tokens. Our own palette (light default, dark variant); geometry from plan/reference/layout.md.

use egui::Color32;

#[derive(Clone, Debug, PartialEq)]
pub struct Theme {
    pub dark: bool,
    /// Use the shared Mac window and form treatment.
    pub mac_style: bool,
    pub custom_surfaces: bool,
    /// Title bar, tab row and module rail.
    pub chrome: Color32,
    pub chrome_text: Color32,
    /// Background behind the rounded panes (the gaps).
    pub backdrop: Color32,
    pub ribbon: Color32,
    pub pane: Color32,
    pub reading: Color32,
    pub text: Color32,
    pub text_muted: Color32,
    pub accent: Color32,
    /// Unread bar, unread subject/time.
    pub unread: Color32,
    pub selected: Color32,
    pub selected_folder: Color32,
    pub hover: Color32,
    pub group_header: Color32,
    pub separator: Color32,
    pub flagged_row: Color32,
    pub flag: Color32,
    pub complete: Color32,
    pub importance_high: Color32,
    pub importance_low: Color32,
    pub draft: Color32,
    pub link: Color32,
    pub danger: Color32,
    pub info_bar: Color32,
    /// Background behind search words in the message list and reading pane.
    pub search_hit: Color32,
}

/// Fixed geometry (logical px) from the observations.
pub mod size {
    pub const TITLE_BAR: f32 = 48.0;
    pub const TAB_ROW: f32 = 30.0;
    pub const RIBBON_SIMPLIFIED: f32 = 49.0;
    pub const RIBBON_CLASSIC: f32 = 100.0;
    pub const STATUS_BAR: f32 = 22.0;
    pub const RAIL: f32 = 48.0;
    pub const PANE_GAP: f32 = 14.0;
    pub const FOLDER_PANE: f32 = 230.0;
    pub const FOLDER_PANE_MIN: f32 = 26.0;
    pub const FOLDER_ROW: f32 = 24.0;
    pub const ACCOUNT_ROW: f32 = 34.0;
    pub const LIST: f32 = 300.0;
    /// Narrowest mail area kept beside the To-Do Bar (below it the bar hides).
    pub const TODO_BAR_MIN_MAIL: f32 = 560.0;
    pub const LIST_HEADER: f32 = 97.0;
    pub const ROW_COMPACT: f32 = 72.0;
    pub const GROUP_HEADER: f32 = 24.0;
    pub const UNREAD_BAR: f32 = 3.0;
    pub const SEARCH_W: f32 = 305.0;
    pub const SEARCH_H: f32 = 34.0;
    pub const MIN_WINDOW: [f32; 2] = [400.0, 400.0];
    /// Folder pane auto-minimises at or below this window width.
    pub const AUTO_MINIMIZE_W: f32 = 800.0;
    /// Single-pane mode at or below this width (reading pane Right).
    pub const SINGLE_PANE_W: f32 = 700.0;
    /// Gap between neighbouring panes (holds the splitter).
    pub const PANE_SPLIT: f32 = 8.0;
    /// Corner radius of the content panes.
    pub const PANE_RADIUS: u8 = 8;
    /// Tab row: File button width.
    pub const FILE_BUTTON_W: f32 = 54.0;
    /// Module rail item (square) and its icon.
    pub const RAIL_ITEM: f32 = 48.0;
    pub const RAIL_ICON: f32 = 20.0;
    /// Title bar account button.
    pub const ACCOUNT_BUTTON_W: f32 = 50.0;
    /// Smallest widths the splitters allow.
    pub const FOLDER_PANE_DRAG_MIN: f32 = 140.0;
    pub const LIST_MIN: f32 = 220.0;
    pub const READING_MIN: f32 = 260.0;
    /// Default list share of the content height when the reading pane is at the bottom (≈395 of 891 observed).
    pub const BOTTOM_LIST_FRACTION: f32 = 0.44;
    /// Toast lifetime, seconds.
    pub const TOAST_SECS: f64 = 5.0;
}

impl Theme {
    /// The light or dark palette.
    pub fn for_dark(dark: bool) -> Theme {
        if dark { Theme::dark() } else { Theme::light() }
    }

    /// egui visuals derived from the palette (widgets, selection, windows, menus).
    pub fn visuals(&self) -> egui::Visuals {
        use egui::{CornerRadius, Stroke};
        let mut v = if self.dark { egui::Visuals::dark() } else { egui::Visuals::light() };
        v.override_text_color = Some(self.text);
        v.hyperlink_color = self.link;
        v.panel_fill = self.pane;
        v.window_fill = if self.mac_style { self.dialog_surface() } else { self.pane };
        v.extreme_bg_color = if self.dark { self.backdrop } else { Color32::WHITE };
        v.faint_bg_color = self.group_header;
        v.code_bg_color = self.info_bar;
        v.window_stroke = Stroke::new(1.0, self.separator);
        v.window_corner_radius = CornerRadius::same(if self.mac_style { 13 } else { 8 });
        v.menu_corner_radius = CornerRadius::same(if self.mac_style { 8 } else { 6 });
        if self.mac_style {
            v.window_shadow =
                egui::Shadow { offset: [0, 12], blur: 40, spread: 0, color: Color32::from_black_alpha(80) };
        }
        v.selection.bg_fill = self.selected;
        v.selection.stroke = Stroke::new(1.0, self.text);
        v.error_fg_color = self.danger;
        v.warn_fg_color = self.flag;
        let radius = CornerRadius::same(4);
        let w = &mut v.widgets;
        w.noninteractive.bg_fill = self.pane;
        w.noninteractive.weak_bg_fill = self.pane;
        w.noninteractive.bg_stroke = Stroke::new(1.0, self.separator);
        w.noninteractive.fg_stroke = Stroke::new(1.0, self.text);
        w.noninteractive.corner_radius = radius;
        w.inactive.bg_fill = self.ribbon;
        w.inactive.weak_bg_fill = Color32::TRANSPARENT;
        w.inactive.bg_stroke = Stroke::NONE;
        w.inactive.fg_stroke = Stroke::new(1.0, self.text);
        w.inactive.corner_radius = radius;
        w.hovered.bg_fill = self.hover;
        w.hovered.weak_bg_fill = self.hover;
        w.hovered.bg_stroke = Stroke::new(1.0, self.separator);
        w.hovered.fg_stroke = Stroke::new(1.0, self.text);
        w.hovered.corner_radius = radius;
        w.active.bg_fill = self.selected;
        w.active.weak_bg_fill = self.selected;
        w.active.bg_stroke = Stroke::new(1.0, self.accent);
        w.active.fg_stroke = Stroke::new(1.0, self.text);
        w.active.corner_radius = radius;
        w.open.bg_fill = self.selected;
        w.open.weak_bg_fill = self.selected;
        w.open.bg_stroke = Stroke::new(1.0, self.separator);
        w.open.fg_stroke = Stroke::new(1.0, self.text);
        w.open.corner_radius = radius;
        v
    }

    /// Installs the visuals plus our text sizes and spacing into `ctx`.
    pub fn apply(&self, ctx: &egui::Context) {
        use egui::{FontFamily, FontId, TextStyle};
        let visuals = self.visuals();
        ctx.all_styles_mut(|style| {
            style.visuals = visuals.clone();
            style.text_styles = [
                (TextStyle::Small, FontId::new(11.0, FontFamily::Proportional)),
                (TextStyle::Body, FontId::new(13.0, FontFamily::Proportional)),
                (TextStyle::Button, FontId::new(13.0, FontFamily::Proportional)),
                (TextStyle::Heading, FontId::new(20.0, FontFamily::Proportional)),
                (TextStyle::Monospace, FontId::new(12.5, FontFamily::Monospace)),
            ]
            .into();
            style.spacing.item_spacing = egui::vec2(6.0, 4.0);
            style.spacing.button_padding = egui::vec2(6.0, 3.0);
            style.spacing.interact_size.y = 22.0;
            style.spacing.menu_margin = egui::Margin::same(6);
            style.interaction.selectable_labels = false;
        });
    }

    /// Shared surfaces for settings, account tools and item editors.
    pub fn dialog_surface(&self) -> Color32 {
        if self.custom_surfaces {
            self.backdrop
        } else if self.dark {
            Color32::from_rgb(33, 34, 37)
        } else {
            Color32::from_rgb(246, 247, 249)
        }
    }

    pub fn card_surface(&self) -> Color32 {
        if self.custom_surfaces {
            self.chrome
        } else if self.dark {
            Color32::from_rgb(43, 44, 48)
        } else {
            Color32::WHITE
        }
    }

    pub fn control_surface(&self) -> Color32 {
        if self.custom_surfaces {
            self.pane
        } else if self.dark {
            Color32::from_rgb(35, 36, 39)
        } else {
            Color32::from_rgb(249, 250, 252)
        }
    }

    pub fn control_edge(&self) -> Color32 {
        if self.custom_surfaces {
            self.separator
        } else if self.dark {
            Color32::from_rgb(62, 63, 68)
        } else {
            Color32::from_rgb(224, 227, 233)
        }
    }

    /// One saved appearance profile, shared by every desktop surface.
    pub fn for_settings(settings: &mailcraft_engine::settings::Settings, dark: bool) -> Theme {
        let mut t = match settings.text("general.interface").as_str() {
            "outlook" => Self::for_dark(dark),
            "omarchy" => Self::omarchy(dark),
            _ => Self::mac(dark),
        };
        if settings.bool("general.customColors") {
            let color = |key: &str, fallback| parse_hex(&settings.text(key)).unwrap_or(fallback);
            t.accent = color("general.accentColor", t.accent);
            t.pane = color("general.backgroundColor", t.pane);
            t.chrome = color("general.surfaceColor", t.chrome);
            t.text = color("general.textColor", t.text);
            t.backdrop = t.pane;
            t.reading = t.pane;
            t.ribbon = t.chrome;
            t.chrome_text = t.text;
            t.text_muted = mix(t.text, t.pane, 0.60);
            t.separator = mix(t.text, t.pane, 0.18);
            t.selected = mix(t.accent, t.pane, 0.30);
            t.selected_folder = t.selected;
            t.hover = mix(t.text, t.pane, 0.08);
            t.group_header = t.chrome;
            t.unread = t.accent;
            t.link = t.accent;
            t.info_bar = mix(t.accent, t.pane, 0.12);
            t.custom_surfaces = true;
        }
        t
    }

    /// Tokyo Night flourishes over the compact layout, in both appearances.
    pub fn omarchy(dark: bool) -> Theme {
        let mut t = Self::mac(dark);
        let rgb = Color32::from_rgb;
        if dark {
            t.backdrop = rgb(26, 27, 38);
            t.chrome = rgb(36, 40, 59);
            t.text = rgb(192, 202, 245);
            t.text_muted = rgb(142, 155, 191);
            t.accent = rgb(122, 162, 247);
            t.separator = rgb(56, 65, 95);
            t.selected = rgb(47, 64, 99);
            t.hover = rgb(41, 46, 66);
            t.draft = rgb(187, 154, 247);
            t.importance_low = rgb(125, 207, 255);
            t.flag = rgb(224, 175, 104);
            t.complete = rgb(158, 206, 106);
            t.danger = rgb(247, 118, 142);
        } else {
            t.backdrop = rgb(245, 246, 250);
            t.chrome = rgb(228, 231, 241);
            t.text = rgb(52, 59, 88);
            t.text_muted = rgb(100, 111, 145);
            t.accent = rgb(52, 84, 166);
            t.separator = rgb(204, 210, 228);
            t.selected = rgb(211, 224, 249);
            t.hover = rgb(230, 234, 245);
            t.draft = rgb(116, 74, 169);
            t.importance_low = rgb(27, 112, 138);
            t.flag = rgb(143, 94, 21);
            t.complete = rgb(65, 111, 48);
            t.danger = rgb(166, 54, 79);
        }
        t.pane = t.backdrop;
        t.reading = t.backdrop;
        t.ribbon = t.chrome;
        t.chrome_text = t.text;
        t.group_header = t.chrome;
        t.selected_folder = t.selected;
        t.unread = t.accent;
        t.link = t.accent;
        t.info_bar = mix(t.accent, t.pane, 0.12);
        t.custom_surfaces = true;
        t
    }

    /// Quiet neutral surfaces, system blue, and restrained separators.
    pub fn mac(dark: bool) -> Theme {
        let mut t = Self::for_dark(dark);
        t.mac_style = true;
        t.accent = if dark { Color32::from_rgb(105, 159, 220) } else { Color32::from_rgb(57, 124, 204) };
        t.unread = t.accent;
        t.link = t.accent;
        if dark {
            t.chrome = Color32::from_rgb(39, 40, 42);
            t.chrome_text = t.text;
            t.backdrop = Color32::from_rgb(31, 33, 34);
            t.pane = t.backdrop;
            t.reading = t.backdrop;
            t.ribbon = t.chrome;
            t.separator = Color32::from_rgb(57, 59, 62);
            t.selected = Color32::from_rgb(37, 63, 88);
            t.group_header = t.chrome;
            t.hover = Color32::from_rgb(47, 49, 52);
            t.text = Color32::from_rgb(229, 230, 232);
            t.text_muted = Color32::from_rgb(149, 153, 160);
            t.selected_folder = t.selected;
        } else {
            t.chrome = Color32::from_rgb(242, 242, 244);
            t.chrome_text = t.text;
            t.backdrop = Color32::WHITE;
            t.ribbon = t.chrome;
            t.text = Color32::from_rgb(36, 36, 38);
            t.text_muted = Color32::from_rgb(128, 128, 134);
            t.separator = Color32::from_rgb(226, 226, 230);
            t.selected = Color32::from_rgb(226, 238, 252);
            t.selected_folder = Color32::from_rgb(215, 227, 243);
            t.hover = Color32::from_rgb(235, 235, 239);
            t.group_header = Color32::from_rgb(250, 250, 251);
        }
        t
    }

    pub fn light() -> Theme {
        Theme {
            dark: false,
            mac_style: false,
            custom_surfaces: false,
            chrome: Color32::from_rgb(0x1f, 0x4e, 0x5f),
            chrome_text: Color32::WHITE,
            backdrop: Color32::from_rgb(0xec, 0xee, 0xf1),
            ribbon: Color32::from_rgb(0xf7, 0xf8, 0xfa),
            pane: Color32::WHITE,
            reading: Color32::WHITE,
            text: Color32::from_rgb(0x1d, 0x1f, 0x23),
            text_muted: Color32::from_rgb(0x5f, 0x66, 0x70),
            accent: Color32::from_rgb(0x0e, 0x7c, 0x86),
            unread: Color32::from_rgb(0x0b, 0x6a, 0xb8),
            selected: Color32::from_rgb(0xd5, 0xe6, 0xf2),
            selected_folder: Color32::from_rgb(0xdf, 0xe6, 0xec),
            hover: Color32::from_rgb(0xee, 0xf2, 0xf5),
            group_header: Color32::from_rgb(0xf3, 0xf4, 0xf6),
            separator: Color32::from_rgb(0xdc, 0xdf, 0xe4),
            flagged_row: Color32::from_rgb(0xfb, 0xec, 0xec),
            flag: Color32::from_rgb(0xc4, 0x31, 0x2f),
            complete: Color32::from_rgb(0x2e, 0x8b, 0x46),
            importance_high: Color32::from_rgb(0xc4, 0x31, 0x2f),
            importance_low: Color32::from_rgb(0x2f, 0x6f, 0xc4),
            draft: Color32::from_rgb(0xc4, 0x31, 0x2f),
            link: Color32::from_rgb(0x0b, 0x6a, 0xb8),
            danger: Color32::from_rgb(0xb0, 0x22, 0x22),
            info_bar: Color32::from_rgb(0xf1, 0xf3, 0xf6),
            search_hit: Color32::from_rgb(0xff, 0xf1, 0x9e),
        }
    }
    pub fn dark() -> Theme {
        Theme {
            dark: true,
            mac_style: false,
            custom_surfaces: false,
            chrome: Color32::from_rgb(0x0c, 0x0e, 0x10),
            chrome_text: Color32::from_rgb(0xe8, 0xea, 0xed),
            backdrop: Color32::from_rgb(0x15, 0x17, 0x1a),
            ribbon: Color32::from_rgb(0x26, 0x29, 0x2d),
            pane: Color32::from_rgb(0x1d, 0x20, 0x23),
            reading: Color32::from_rgb(0x22, 0x25, 0x29),
            text: Color32::from_rgb(0xe8, 0xea, 0xed),
            text_muted: Color32::from_rgb(0x9a, 0xa1, 0xab),
            accent: Color32::from_rgb(0x3f, 0xb8, 0xc2),
            unread: Color32::from_rgb(0x6c, 0xb8, 0xf6),
            selected: Color32::from_rgb(0x34, 0x45, 0x52),
            selected_folder: Color32::from_rgb(0x33, 0x38, 0x3e),
            hover: Color32::from_rgb(0x2a, 0x2e, 0x33),
            group_header: Color32::from_rgb(0x26, 0x29, 0x2d),
            separator: Color32::from_rgb(0x33, 0x37, 0x3c),
            flagged_row: Color32::from_rgb(0x3a, 0x22, 0x24),
            flag: Color32::from_rgb(0xe5, 0x5b, 0x58),
            complete: Color32::from_rgb(0x5c, 0xc0, 0x76),
            importance_high: Color32::from_rgb(0xe5, 0x5b, 0x58),
            importance_low: Color32::from_rgb(0x6c, 0xa0, 0xf0),
            draft: Color32::from_rgb(0xe5, 0x5b, 0x58),
            link: Color32::from_rgb(0x6c, 0xb8, 0xf6),
            danger: Color32::from_rgb(0xe5, 0x5b, 0x58),
            info_bar: Color32::from_rgb(0x2a, 0x2e, 0x33),
            search_hit: Color32::from_rgb(0x6b, 0x5a, 0x12),
        }
    }
}

/// Strict RGB hex parsing. Persisted values are validated by the engine as well.
pub fn parse_hex(value: &str) -> Option<Color32> {
    let hex = value.strip_prefix('#')?;
    if hex.len() != 6 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let rgb = u32::from_str_radix(hex, 16).ok()?;
    Some(Color32::from_rgb((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8))
}

fn mix(front: Color32, back: Color32, amount: f32) -> Color32 {
    let blend = |a: u8, b: u8| (f32::from(a) * amount + f32::from(b) * (1.0 - amount)).round() as u8;
    Color32::from_rgb(blend(front.r(), back.r()), blend(front.g(), back.g()), blend(front.b(), back.b()))
}

#[cfg(test)]
mod appearance_tests {
    use super::*;
    use mailcraft_engine::settings::Settings;
    use serde_json::json;

    #[test]
    fn profiles_and_custom_palette_survive_settings_roundtrip() {
        for id in ["outlook", "apple", "omarchy"] {
            let mut settings = Settings::default();
            settings.set("general.interface", &json!(id)).unwrap();
            let (loaded, warnings) =
                Settings::from_json(&serde_json::to_value(settings.overrides()).unwrap());
            assert!(warnings.is_empty());
            for dark in [false, true] {
                let t = Theme::for_settings(&loaded, dark);
                assert_eq!(t.mac_style, id != "outlook");
                assert_eq!(t.dark, dark);
                if id == "omarchy" {
                    assert_eq!(t.card_surface(), t.chrome);
                }
            }
            settings.set("general.customColors", &json!(true)).unwrap();
            settings.set("general.accentColor", &json!("#AABBCC")).unwrap();
            settings.set("general.backgroundColor", &json!("#112233")).unwrap();
            let t = Theme::for_settings(&settings, true);
            assert_eq!(t.accent, Color32::from_rgb(170, 187, 204));
            assert_eq!(t.reading, Color32::from_rgb(17, 34, 51));
            assert_eq!(t.mac_style, id != "outlook");
        }
    }

    #[test]
    fn malformed_custom_colors_are_rejected_before_persistence() {
        let mut settings = Settings::default();
        for value in ["#123", "#12345Z", "1234567", "#ééé", "#ABCDEF\n", ""] {
            assert!(settings.set("general.accentColor", &json!(value)).is_err());
            assert!(parse_hex(value).is_none());
        }
        assert_eq!(settings.set("general.accentColor", &json!("#abcdef")).unwrap(), json!("#ABCDEF"));
    }
}
