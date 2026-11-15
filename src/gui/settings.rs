//! Settings of Aéroport: the plain `key = value` lines at the top of `~/.config/filigrane/fili-term.toml`, before any
//! section (the sections are the theme, see theme.rs). Every key is optional; the defaults live here. An invalid value
//! is ignored. (`journal = true` shows the command journal in each pane from the start.)
use super::theme::{Val, parse_sections};

/// (key, default, smallest, largest)
const NUMBERS: &[(&str, i64, i64, i64)] = &[("font_size", 12, 6, 48), ("padding", 10, 0, 64), ("line_spacing", 0, 0, 24), ("notify_after", 10, 0, 86400)];
const SWITCHES: &[(&str, bool)] = &[("window_buttons", true), ("journal", false)];
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

/// A custom command (`[[commands]]` of fili-term.toml): run by a key or from the Commands menu
#[derive(Debug, PartialEq)]
pub struct Command {
    pub keys: Vec<String>,
    pub name: String,
    /// "popup" (a terminal over the pane), "pane" (split below), "tab" (a new tab), "shell" (in the background)
    pub kind: String,
    pub command: String,
    pub width: String,
    pub height: String,
}

pub const COMMAND_KINDS: &[&str] = &["popup", "pane", "tab", "shell"];

/// One TOML value of a command line: "basic string" (\" and \\ escapes), 'literal string', or a bare word up to a comment
fn command_value(v: &str) -> String {
    let v = v.trim();
    if let Some(rest) = v.strip_prefix('"') {
        let mut out = String::new();
        let mut chars = rest.chars();
        while let Some(c) = chars.next() {
            match c {
                '"' => break,
                '\\' => match chars.next() {
                    Some('n') => out.push(' '),
                    Some('t') => out.push(' '),
                    Some(o) => out.push(o),
                    None => break,
                },
                c => out.push(c),
            }
        }
        out
    } else if let Some(rest) = v.strip_prefix('\'') {
        rest.split('\'').next().unwrap_or("").to_string()
    } else {
        v.split('#').next().unwrap_or("").trim().to_string()
    }
}

/// The `[[name]]` tables of the file as key → value maps (values as `command_value` reads them)
pub fn tables_from(text: &str, name: &str) -> Vec<std::collections::BTreeMap<String, String>> {
    let header = format!("[[{name}]]");
    let mut tables: Vec<std::collections::BTreeMap<String, String>> = Vec::new();
    let mut inside = false;
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') {
            inside = line.split('#').next().unwrap_or("").replace(' ', "") == header;
            if inside {
                tables.push(Default::default());
            }
        } else if inside {
            if let (Some((k, v)), Some(t)) = (line.split_once('='), tables.last_mut()) {
                t.insert(k.trim().to_string(), command_value(v));
            }
        }
    }
    tables
}

/// The `[[commands]]` tables of the file; an entry without a command or with an unknown type is left out
pub fn commands_from(text: &str) -> Vec<Command> {
    tables_from(text, "commands")
        .into_iter()
        .filter_map(|t| {
            let command = t.get("command")?.trim().to_string();
            let kind = t.get("type").cloned().filter(|k| !k.is_empty()).unwrap_or_else(|| "popup".into());
            if command.is_empty() || !COMMAND_KINDS.contains(&kind.as_str()) {
                return None;
            }
            let name = t.get("name").map(|n| n.trim().to_string()).filter(|n| !n.is_empty()).unwrap_or_else(|| {
                command.split_whitespace().next().unwrap_or("").rsplit('/').next().unwrap_or("").to_string()
            });
            let keys = t.get("key").map(|k| k.split('|').map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).collect()).unwrap_or_default();
            let size = |k: &str| t.get(k).map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).unwrap_or_else(|| "80%".into());
            Some(Command { keys, name, kind, command, width: size("width"), height: size("height") })
        })
        .collect()
}

/// The commands as JSON for the QML: [{keys: [...], name, type, command, width, height}]
pub fn commands_json_from(text: &str) -> String {
    let q = |s: &str| super::terminal::json_string(s);
    let items: Vec<String> = commands_from(text)
        .iter()
        .map(|c| {
            let keys: Vec<String> = c.keys.iter().map(|k| q(k)).collect();
            format!(
                "{{\"keys\":[{}],\"name\":{},\"type\":{},\"command\":{},\"width\":{},\"height\":{}}}",
                keys.join(","),
                q(&c.name),
                q(&c.kind),
                q(&c.command),
                q(&c.width),
                q(&c.height)
            )
        })
        .collect();
    format!("[{}]", items.join(","))
}

pub fn commands_json() -> String {
    commands_json_from(&std::fs::read_to_string(super::theme::path()).unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn custom_commands() {
        let text = "font_size = 13\n\n[[commands]]\nkey = \"Ctrl+Alt+G | Ctrl+Alt+L\"\nname = \"Lazygit\"\ntype = \"popup\"\ncommand = \"lazygit\"  # git\nwidth = \"90%\"\nheight = 40\n\n[[commands]]\ncommand = 'cargo test -- --nocapture'\ntype = \"pane\"\n\n[[commands]]\ncommand = \"echo \\\"hi\\\" # not a comment\"\ntype = \"shell\"\n\n[[commands]]\ncommand = \"x\"\ntype = \"window\"\n\n[[commands]]\nname = \"No command\"\n\n[keys]\nfind = \"\"\n";
        let c = commands_from(text);
        assert_eq!(c.len(), 3);
        assert_eq!(c[0], Command { keys: vec!["Ctrl+Alt+G".into(), "Ctrl+Alt+L".into()], name: "Lazygit".into(), kind: "popup".into(), command: "lazygit".into(), width: "90%".into(), height: "40".into() });
        // Name from the command, popup size and type by default
        assert_eq!((c[1].name.as_str(), c[1].kind.as_str(), c[1].width.as_str(), c[1].command.as_str()), ("cargo", "pane", "80%", "cargo test -- --nocapture"));
        assert_eq!(c[2].command, "echo \"hi\" # not a comment");
        assert!(commands_json_from(text).starts_with("[{\"keys\":[\"Ctrl+Alt+G\",\"Ctrl+Alt+L\"],\"name\":\"Lazygit\""));
        assert_eq!(commands_json_from(""), "[]");
        // The tables do not leak into the settings
        assert!(json_from(text).contains("\"font_size\":13"));
    }

    #[test]
    fn defaults() {
        assert_eq!(json_from(""), "{\"font_size\":12,\"padding\":10,\"line_spacing\":0,\"notify_after\":10,\"window_buttons\":true,\"journal\":false,\"restore\":\"always\",\"modes\":{\"herdr\":\"auto\"}}");
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
        assert_eq!(json_from(text), "{\"font_size\":14,\"padding\":10,\"line_spacing\":0,\"notify_after\":10,\"window_buttons\":false,\"journal\":false,\"restore\":\"always\",\"modes\":{\"herdr\":\"auto\"}}");
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
