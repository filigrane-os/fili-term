//! Settings of Aéroport: the plain `key = value` lines at the top of `~/.config/filigrane/fili-term.toml`, before any
//! section (the sections are the theme, see theme.rs). Every key is optional; the defaults live here. An invalid value
//! is ignored.
use super::theme::{Val, parse_sections};

/// (key, default, smallest, largest)
const NUMBERS: &[(&str, i64, i64, i64)] = &[("font_size", 12, 6, 48), ("padding", 10, 0, 64)];
const SWITCHES: &[(&str, bool)] = &[("window_buttons", true)];

/// The settings as JSON for the QML: {"font_size": 12, "padding": 10, "window_buttons": true}
pub fn json_from(text: &str) -> String {
    let sections = parse_sections(text);
    let top = sections.get("");
    let value = |key: &str| match top.and_then(|t| t.get(key)) {
        Some(Val::Str(v)) => Some(v.clone()),
        _ => None,
    };
    let mut parts = Vec::new();
    for (key, default, low, high) in NUMBERS {
        let v = value(key).and_then(|v| v.parse::<i64>().ok()).filter(|v| (*low..=*high).contains(v)).unwrap_or(*default);
        parts.push(format!("\"{key}\":{v}"));
    }
    for (key, default) in SWITCHES {
        let v = value(key).and_then(|v| v.parse::<bool>().ok()).unwrap_or(*default);
        parts.push(format!("\"{key}\":{v}"));
    }
    format!("{{{}}}", parts.join(","))
}

pub fn json() -> String {
    json_from(&std::fs::read_to_string(super::theme::path()).unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults() {
        assert_eq!(json_from(""), "{\"font_size\":12,\"padding\":10,\"window_buttons\":true}");
    }

    #[test]
    fn top_keys_only_and_valid_values() {
        let text = "font_size = 14 # bigger\npadding = 999\nwindow_buttons = false\n[theme]\nfont_size = 30\n";
        assert_eq!(json_from(text), "{\"font_size\":14,\"padding\":10,\"window_buttons\":false}");
    }
}
