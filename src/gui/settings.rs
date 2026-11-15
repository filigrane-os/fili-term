//! Settings of Aéroport: the plain `key = value` lines at the top of `~/.config/filigrane/fili-term.toml`, before any
//! section (the sections are the theme, see theme.rs). Every key is optional; the defaults live here. An invalid value
//! is ignored.
use super::theme::{Val, parse_sections};

/// (key, default, smallest, largest)
const NUMBERS: &[(&str, i64, i64, i64)] = &[("font_size", 12, 6, 48), ("padding", 10, 0, 64), ("line_spacing", 0, 0, 24), ("notify_after", 10, 0, 86400)];
const SWITCHES: &[(&str, bool)] = &[("window_buttons", true)];
/// What a plain `fili-term` does when no window is open: bring back the windows of the last run
pub const RESTORE: &[&str] = &["always", "never"];
/// Integrated modes (`[modes]`): what to do when such a program is seen in a tab
pub const MODES: &[&str] = &["herdr"];
pub const MODE_POLICIES: &[&str] = &["ask", "auto", "off"];

/// The settings as JSON for the QML: {"font_size": 12, "padding": 10, "line_spacing": 0, "notify_after": 10,
/// "window_buttons": true, "restore": "always", "modes": {"herdr": "auto"}}
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
    let restore = match value("restore") {
        Some(v) if RESTORE.contains(&v.as_str()) => v,
        _ => "always".to_string(),
    };
    parts.push(format!("\"restore\":\"{restore}\""));
    let modes = sections.get("modes");
    let policies: Vec<String> = MODES
        .iter()
        .map(|kind| {
            let v = match modes.and_then(|m| m.get(*kind)) {
                Some(Val::Str(v)) if MODE_POLICIES.contains(&v.as_str()) => v.as_str(),
                _ => "auto",
            };
            format!("\"{kind}\":\"{v}\"")
        })
        .collect();
    parts.push(format!("\"modes\":{{{}}}", policies.join(",")));
    format!("{{{}}}", parts.join(","))
}

/// The file with `key = "value"` set in `[section]` (the line replaced, or added at the end of the section, or the
/// section added at the end of the file); comments and other lines stay as they are
pub fn with_value(text: &str, section: &str, key: &str, value: &str) -> String {
    let line = format!("{key} = \"{value}\"");
    let header = format!("[{section}]");
    let mut out: Vec<String> = Vec::new();
    let (mut inside, mut done) = (false, false);
    for l in text.lines() {
        let t = l.trim();
        if t.starts_with('[') {
            if inside && !done {
                // End of our section: the value goes before the blank lines that close it
                let at = out.iter().rposition(|x| !x.trim().is_empty()).map_or(out.len(), |i| i + 1);
                out.insert(at, line.clone());
                done = true;
            }
            inside = t == header;
        } else if inside && !done && !t.starts_with('#') && t.split_once('=').is_some_and(|(k, _)| k.trim() == key) {
            out.push(line.clone());
            done = true;
            continue;
        }
        out.push(l.to_string());
    }
    if !done {
        if !inside {
            if out.last().is_some_and(|l| !l.trim().is_empty()) {
                out.push(String::new());
            }
            out.push(header);
        }
        out.push(line);
    }
    out.join("\n") + "\n"
}

/// `[modes] KIND = "POLICY"` in fili-term.toml (the footer's "Don't Ask Again", the menu)
pub fn set_mode_policy(kind: &str, policy: &str) -> std::io::Result<()> {
    if !MODES.contains(&kind) || !MODE_POLICIES.contains(&policy) {
        return Err(std::io::Error::other("unknown mode or policy"));
    }
    let path = super::theme::path();
    let text = std::fs::read_to_string(&path).unwrap_or_default();
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(&path, with_value(&text, "modes", kind, policy))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults() {
        assert_eq!(json_from(""), "{\"font_size\":12,\"padding\":10,\"line_spacing\":0,\"notify_after\":10,\"window_buttons\":true,\"restore\":\"always\",\"modes\":{\"herdr\":\"auto\"}}");
    }

    #[test]
    fn restore_setting() {
        assert!(json_from("restore = \"never\"\n").contains("\"restore\":\"never\""));
        assert!(json_from("restore = \"sometimes\"\n").contains("\"restore\":\"always\""));
        assert!(json_from("[window]\nrestore = \"never\"\n").contains("\"restore\":\"always\""));
    }

    #[test]
    fn top_keys_only_and_valid_values() {
        let text = "font_size = 14 # bigger\npadding = 999\nwindow_buttons = false\n[theme]\nfont_size = 30\n";
        assert_eq!(json_from(text), "{\"font_size\":14,\"padding\":10,\"line_spacing\":0,\"notify_after\":10,\"window_buttons\":false,\"restore\":\"always\",\"modes\":{\"herdr\":\"auto\"}}");
    }

    #[test]
    fn modes() {
        assert!(json_from("[modes]\nherdr = \"auto\"\n").contains("\"modes\":{\"herdr\":\"auto\"}"));
        assert!(json_from("[modes]\nherdr = \"never\"\n").contains("\"modes\":{\"herdr\":\"auto\"}"));
    }

    #[test]
    fn set_a_value() {
        assert_eq!(with_value("", "modes", "herdr", "off"), "[modes]\nherdr = \"off\"\n");
        assert_eq!(with_value("font_size = 13\n", "modes", "herdr", "off"), "font_size = 13\n\n[modes]\nherdr = \"off\"\n");
        assert_eq!(
            with_value("[modes]\n# herdr = \"ask\"\nherdr = \"auto\"\n\n[keys]\nfind = \"\"\n", "modes", "herdr", "off"),
            "[modes]\n# herdr = \"ask\"\nherdr = \"off\"\n\n[keys]\nfind = \"\"\n"
        );
        assert_eq!(
            with_value("[modes]\nnvim = \"ask\"\n\n[keys]\n", "modes", "herdr", "off"),
            "[modes]\nnvim = \"ask\"\nherdr = \"off\"\n\n[keys]\n"
        );
    }
}
