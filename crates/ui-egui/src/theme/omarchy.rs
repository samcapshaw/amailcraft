//! Read-only Omarchy palette integration. No external code is imported.
use super::{Theme, mix, parse_hex};
use egui::Color32;
use std::collections::BTreeMap;

const LIMIT: u64 = 16 * 1024;

fn parse(source: &str) -> Option<Theme> {
    if source.len() > LIMIT as usize {
        return None;
    }
    let mut colors = BTreeMap::new();
    let mut dark = true;
    for line in source.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (key, value) = line.split_once('=')?;
        let key = key.trim();
        let value = value.trim();
        let quote = value.chars().next()?;
        if quote != '\"' && quote != '\'' {
            return None;
        }
        let rest = value.get(1..)?;
        let end = rest.find(quote)?;
        let tail = rest.get(end.saturating_add(1)..)?.trim();
        if !tail.is_empty() && !tail.starts_with('#') {
            return None;
        }
        let value = rest.get(..end)?;
        if key == "mode" {
            dark = match value {
                "dark" => true,
                "light" => false,
                _ => return None,
            };
        } else if let Some(color) = parse_hex(value) {
            colors.insert(key, color);
        }
    }
    let mut t = Theme::omarchy(dark);
    t.pane = *colors.get("background")?;
    t.text = *colors.get("foreground")?;
    t.accent = *colors.get("accent")?;
    let color = |key, fallback: Color32| colors.get(key).copied().unwrap_or(fallback);
    t.chrome = color("lighter_background", mix(t.text, t.pane, 0.06));
    t.backdrop = t.pane;
    t.reading = t.pane;
    t.ribbon = t.chrome;
    t.chrome_text = t.text;
    t.text_muted = mix(t.text, t.pane, 0.65);
    t.separator = color("muted", mix(t.text, t.pane, 0.18));
    t.selected = color("selection", mix(t.accent, t.pane, 0.25));
    t.selected_folder = t.selected;
    t.hover = mix(t.text, t.pane, 0.08);
    t.group_header = t.chrome;
    t.unread = t.accent;
    t.link = color("cyan", t.accent);
    t.draft = color("magenta", t.accent);
    t.importance_low = t.link;
    t.flag = color("yellow", t.flag);
    t.complete = color("green", t.complete);
    t.danger = color("red", t.danger);
    t.info_bar = mix(t.accent, t.pane, 0.12);
    Some(t)
}

#[cfg(not(target_arch = "wasm32"))]
fn read() -> Option<Theme> {
    use std::io::Read;
    let root = std::env::var_os("XDG_STATE_HOME")
        .filter(|v| !v.is_empty())
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|v| std::path::PathBuf::from(v).join(".local/state")))?;
    let file = std::fs::File::open(root.join("omarchy/current/theme/colors.toml")).ok()?;
    if !file.metadata().ok()?.is_file() {
        return None;
    }
    let mut source = String::new();
    file.take(LIMIT + 1).read_to_string(&mut source).ok()?;
    parse(&source)
}

pub(super) fn active() -> Option<Theme> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        use std::{
            cell::RefCell,
            time::{Duration, Instant},
        };
        thread_local! {
            static CACHE: RefCell<Option<(Instant, Option<Theme>)>> = const { RefCell::new(None) };
        }
        CACHE.with(|cache| {
            let mut cache = cache.borrow_mut();
            if cache.as_ref().is_none_or(|(at, _)| at.elapsed() >= Duration::from_secs(2)) {
                *cache = Some((Instant::now(), read()));
            }
            cache.as_ref().and_then(|(_, theme)| theme.clone())
        })
    }
    #[cfg(target_arch = "wasm32")]
    {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn valid_palette_maps_light_mode_and_optional_accents() {
        let t = parse("mode = 'light'\nbackground = '#ffffff'\nforeground = '#222222'\naccent = '#123456' # comment\ngreen = '#abcdef'").unwrap();
        assert!(!t.dark);
        assert!(t.terminal_style);
        assert_eq!(t.accent, Color32::from_rgb(0x12, 0x34, 0x56));
        assert_eq!(t.complete, Color32::from_rgb(0xab, 0xcd, 0xef));
    }
    #[test]
    fn incomplete_malformed_or_oversized_palette_falls_back() {
        for input in
            ["", "background = '#123456'", "accent = '\"", "mode = 'unknown'", "accent = '#ffffff' junk"]
        {
            assert!(parse(input).is_none());
        }
        assert!(parse(&"x".repeat(LIMIT as usize + 1)).is_none());
    }
}
