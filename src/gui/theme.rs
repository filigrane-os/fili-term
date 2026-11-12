//! Theme of Aéroport: the defaults of `theme.toml` (embedded), overridden by the shared
//! `~/.config/filigrane/theme.toml`, then by `~/.config/filigrane/fili-term.toml`, key by key
//! (same layers and same keys as Casier and Facteur, plus the terminal `palette`). Merged as JSON for `FiliTheme.qml`.
use std::{collections::BTreeMap, path::PathBuf};

fn strip_comment(line: &str) -> &str {
    let mut quoted = false;
    for (i, c) in line.char_indices() {
        match c {
            '"' => quoted = !quoted,
            '#' if !quoted => return &line[..i],
            _ => {}
        }
    }
    line
}

// ---- Theme -------------------------------------------------------------------------------------------------
// The defaults are in theme.toml (embedded); ~/.config/filigrane/fili-term.toml overrides them key by key.
// Lookup order of a key: the user's [theme.<mode>], their [theme], the defaults' [theme.<mode>], their [theme].

#[derive(Clone, Debug, PartialEq)]
pub(super) enum Val {
    Str(String),
    List(Vec<String>),
}

type Sections = BTreeMap<String, BTreeMap<String, Val>>;

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Color,
    Int,
    Str,
    // 16 colours (the ANSI palette of the terminal)
    Palette,
}

// (TOML key, type): all of them apply in [theme] and in each mode
const KEYS: &[(&str, Kind)] = &[
    ("background", Kind::Color),
    ("background_deep", Kind::Color),
    ("surface", Kind::Color),
    ("surface_hover", Kind::Color),
    ("surface_active", Kind::Color),
    ("border", Kind::Color),
    ("muted", Kind::Color),
    ("text_secondary", Kind::Color),
    ("text", Kind::Color),
    ("warning", Kind::Color),
    ("danger", Kind::Color),
    ("success", Kind::Color),
    ("accent", Kind::Color), // optional: without a value, the accent follows fili-theme
    ("accent_bright", Kind::Color), // optional
    ("font", Kind::Str),
    ("mono", Kind::Str),
    ("icons", Kind::Str),
    ("size", Kind::Int),
    ("size_list", Kind::Int),
    ("size_small", Kind::Int),
    ("size_label", Kind::Int),
    ("size_title", Kind::Int),
    ("weight_accent", Kind::Int),
    ("radius_mark", Kind::Int),
    ("radius_control", Kind::Int),
    ("radius_window", Kind::Int),
    ("border_width", Kind::Int),
    ("space_1", Kind::Int),
    ("space_2", Kind::Int),
    ("space_3", Kind::Int),
    ("space_4", Kind::Int),
    ("space_5", Kind::Int),
    ("space_6", Kind::Int),
    ("row_height", Kind::Int),
    ("side_row_height", Kind::Int),
    ("icon_size", Kind::Int),
    ("margin", Kind::Int),
    ("toolbar_height", Kind::Int),
    ("status_height", Kind::Int),
    ("sidebar_width", Kind::Int),
    ("tab_bar_height", Kind::Int),
    ("tab_height", Kind::Int),
    ("palette", Kind::Palette),
];

const DEFAULT_THEME: &str = include_str!("../../theme.toml");

fn parse_value(v: &str) -> Val {
    let v = v.trim();
    if let Some(inner) = v.strip_prefix('[').and_then(|r| r.strip_suffix(']')) {
        Val::List(
            inner
                .split(',')
                .map(|x| x.trim().trim_matches('"').trim().to_string())
                .filter(|x| !x.is_empty())
                .collect(),
        )
    } else {
        Val::Str(v.trim_matches('"').trim().to_string())
    }
}

pub(super) fn parse_sections(text: &str) -> Sections {
    let mut out = Sections::new();
    let mut section = String::new();
    for line in text.lines() {
        let line = strip_comment(line).trim();
        if let Some(name) = line.strip_prefix('[').and_then(|r| r.strip_suffix(']')) {
            section = name.trim().to_string();
        } else if let Some((k, v)) = line.split_once('=') {
            out.entry(section.clone())
                .or_default()
                .insert(k.trim().to_string(), parse_value(v));
        }
    }
    out
}

fn valid_color(s: &str) -> bool {
    s.strip_prefix('#')
        .is_some_and(|h| matches!(h.len(), 3 | 6 | 8) && h.chars().all(|c| c.is_ascii_hexdigit()))
}

fn valid(kind: Kind, s: &str) -> bool {
    match kind {
        Kind::Color => valid_color(s),
        Kind::Int => s.parse::<u32>().is_ok(),
        Kind::Str => !s.is_empty(),
        Kind::Palette => false,
    }
}

fn json_str(s: &str) -> String {
    let mut o = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            c if (c as u32) < 0x20 => o.push(' '),
            c => o.push(c),
        }
    }
    o.push('"');
    o
}

// Accent tones: the user's (4 valid colors) replace the defaults', by name
fn accents_from(layers: &[&Sections]) -> BTreeMap<String, Vec<String>> {
    let mut accents = BTreeMap::new();
    for sections in layers {
        for (name, v) in sections.get("theme.accents").into_iter().flatten() {
            if let Val::List(l) = v {
                if l.len() == 4 && l.iter().all(|c| valid_color(c)) {
                    accents.insert(name.clone(), l.clone());
                }
            }
        }
    }
    accents
}

// Merged theme as JSON: {"dark": {key: value}, "light": {...}, "accents": {name: [4 colors]}}
#[cfg(test)]
pub fn theme_json_from(user: &str) -> String {
    theme_json_layers("", user)
}

// `global`: ~/.config/filigrane/theme.toml (shared by Filigrane); `user`: fili-term.toml, which takes precedence
pub fn theme_json_layers(global: &str, user: &str) -> String {
    let defaults = parse_sections(DEFAULT_THEME);
    let global = parse_sections(global);
    let user = parse_sections(user);
    // A value as JSON when it is valid for its kind
    let get = |sections: &Sections, name: &str, key: &str, kind: Kind| match sections
        .get(name)
        .and_then(|s| s.get(key))
    {
        Some(Val::Str(v)) if valid(kind, v) => Some(if kind == Kind::Int { v.clone() } else { json_str(v) }),
        Some(Val::List(l)) if kind == Kind::Palette && l.len() == 16 && l.iter().all(|c| valid_color(c)) => {
            Some(format!("[{}]", l.iter().map(|c| json_str(c)).collect::<Vec<_>>().join(",")))
        }
        _ => None,
    };
    let mut out = String::from("{");
    for (n, mode) in ["dark", "light"].iter().enumerate() {
        if n > 0 {
            out.push(',');
        }
        out.push_str(&format!("{}:{{", json_str(mode)));
        let section = format!("theme.{mode}");
        let mut first = true;
        for (key, kind) in KEYS {
            let found = [
                (&user, section.as_str()),
                (&user, "theme"),
                (&global, section.as_str()),
                (&global, "theme"),
                (&defaults, section.as_str()),
                (&defaults, "theme"),
            ]
            .iter()
            .find_map(|(s, name)| get(s, name, key, *kind));
            let Some(v) = found else { continue };
            if !first {
                out.push(',');
            }
            first = false;
            out.push_str(&json_str(key));
            out.push(':');
            out.push_str(&v);
        }
        out.push('}');
    }
    let accents = accents_from(&[&defaults, &global, &user]);
    let list: Vec<String> = accents
        .iter()
        .map(|(n, l)| {
            format!(
                "{}:[{}]",
                json_str(n),
                l.iter().map(|c| json_str(c)).collect::<Vec<_>>().join(",")
            )
        })
        .collect();
    out.push_str(&format!(",\"accents\":{{{}}}}}", list.join(",")));
    out
}

fn config_dir() -> PathBuf {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".config")
        });
    base.join("filigrane")
}

pub fn path() -> PathBuf {
    config_dir().join("fili-term.toml")
}

pub fn global_path() -> PathBuf {
    config_dir().join("theme.toml")
}

pub fn theme_json() -> String {
    let read = |p: PathBuf| std::fs::read_to_string(p).unwrap_or_default();
    theme_json_layers(&read(global_path()), &read(path()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_have_both_palettes() {
        let json = theme_json_from("");
        assert_eq!(json.matches("\"palette\":[").count(), 2);
        assert!(json.contains("\"toolbar_height\":44"));
    }

    #[test]
    fn user_palette_needs_sixteen_colours() {
        let short = "[theme.dark]\npalette = [\"#000000\", \"#111111\"]\n";
        assert!(theme_json_from(short).contains("\"#2d3744\""));
        let mut full = String::from("[theme.dark]\npalette = [");
        full.push_str(&vec!["\"#010203\""; 16].join(", "));
        full.push_str("]\n");
        let json = theme_json_from(&full);
        assert!(json.contains("\"#010203\""));
        // The light mode keeps its own default
        assert!(json.contains("\"#9a4f55\""));
    }

    #[test]
    fn user_overrides_a_colour_key_by_key() {
        let json = theme_json_from("[theme]\nbackground = \"#000000\"\nsize = \"x\"\n");
        assert!(json.contains("\"background\":\"#000000\""));
        assert!(json.contains("\"size\":14"));
    }
}
