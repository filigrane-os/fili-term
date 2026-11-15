//! Keyboard shortcuts of Aéroport: a preset (`keymap = "default" | "mac" | "tmux" | "none"` at the top of
//! `~/.config/filigrane/fili-term.toml`), then the `[keys]` section of the same file, action by action:
//!
//! ```toml
//! [keys]
//! new-tab = "Ctrl+Shift+T | Meta+T"   # alternatives are separated by |, a chord by a comma: "Ctrl+B, C"
//! find = ""                           # no shortcut
//! ```
//!
//! The result is a JSON list [{"action": "new-tab", "keys": ["Ctrl+Shift+T"]}] for Main.qml (Qt key sequence names;
//! Meta is the Super key).
use super::theme::{Val, parse_sections};

pub const PRESETS: &[&str] = &["default", "mac", "tmux", "none"];

/// The actions with a shortcut, in the order of the menu, and the keys of the default preset
const DEFAULT: &[(&str, &str)] = &[
    ("new-tab", "Ctrl+Shift+T"),
    ("new-window", "Ctrl+Shift+N"),
    ("close-tab", "Ctrl+Alt+Shift+W"),
    ("close-window", "Ctrl+Shift+Q"),
    ("copy", "Ctrl+Shift+C | Ctrl+Ins | Copy"),
    ("paste", "Ctrl+Shift+V | Shift+Ins | Paste"),
    ("next-tab", "Ctrl+PgDown | Ctrl+Tab | Ctrl+Shift+Right"),
    ("previous-tab", "Ctrl+PgUp | Ctrl+Shift+Tab | Ctrl+Shift+Left"),
    ("tab-1", "Alt+1"),
    ("tab-2", "Alt+2"),
    ("tab-3", "Alt+3"),
    ("tab-4", "Alt+4"),
    ("tab-5", "Alt+5"),
    ("tab-6", "Alt+6"),
    ("tab-7", "Alt+7"),
    ("tab-8", "Alt+8"),
    ("tab-9", "Alt+9"),
    ("split-right", "Ctrl+Shift+D"),
    ("split-down", "Ctrl+Shift+E"),
    ("close-pane", "Ctrl+Shift+W"),
    ("zoom-pane", "Ctrl+Shift+Return | Ctrl+Shift+Enter"),
    ("focus-left", "Ctrl+Alt+Left"),
    ("focus-right", "Ctrl+Alt+Right"),
    ("focus-up", "Ctrl+Alt+Up"),
    ("focus-down", "Ctrl+Alt+Down"),
    ("resize-left", "Ctrl+Alt+Shift+Left"),
    ("resize-right", "Ctrl+Alt+Shift+Right"),
    ("resize-up", "Ctrl+Alt+Shift+Up"),
    ("resize-down", "Ctrl+Alt+Shift+Down"),
    ("resize-mode", "Ctrl+Alt+R"),
    ("rename-pane", ""),
    ("edit-scrollback", "Ctrl+Alt+Shift+E"),
    ("next-agent", "Ctrl+Alt+A"),
    ("next-workspace", "Ctrl+Alt+PgDown"),
    ("previous-workspace", "Ctrl+Alt+PgUp"),
    ("new-workspace", ""),
    ("commands", "Ctrl+Alt+P"),
    ("new-worktree", ""),
    ("broadcast", "Ctrl+Alt+I"),
    ("toggle-journal", "Ctrl+Alt+J"),
    ("zoom-in", "Ctrl++ | Ctrl+="),
    ("zoom-out", "Ctrl+-"),
    ("zoom-reset", "Ctrl+0"),
    ("previous-command", "Ctrl+Shift+Up"),
    ("next-command", "Ctrl+Shift+Down"),
    ("copy-last-output", "Ctrl+Shift+O"),
    ("find", "Ctrl+Shift+F"),
    ("find-next", "Ctrl+Shift+G | F3"),
    ("find-previous", "Ctrl+Shift+H | Shift+F3"),
    ("page-up", "Shift+PgUp"),
    ("page-down", "Shift+PgDown"),
    ("scroll-top", "Ctrl+Shift+Home"),
    ("scroll-bottom", "Ctrl+Shift+End"),
    ("toggle-sidebar", "Ctrl+Shift+B"),
    ("context-menu", "Menu | Shift+F10"),
];

/// Keys a preset changes (the rest stays as in the default)
fn overlay(preset: &str) -> Vec<(String, String)> {
    let mut v: Vec<(String, String)> = Vec::new();
    let mut set = |a: &str, k: &str| v.push((a.to_string(), k.to_string()));
    match preset {
        // Cmd of a Mac keyboard is the Super key here (Meta in Qt): the compositor keeps the combinations it binds itself
        "mac" => {
            set("new-tab", "Meta+T");
            set("new-window", "Meta+N");
            set("close-pane", "Meta+W");
            set("close-tab", "Meta+Alt+W");
            set("close-window", "Meta+Shift+W");
            set("copy", "Meta+C | Ctrl+Ins | Copy");
            set("paste", "Meta+V | Shift+Ins | Paste");
            set("next-tab", "Meta+Shift+] | Ctrl+Tab");
            set("previous-tab", "Meta+Shift+[ | Ctrl+Shift+Tab");
            // Panes as in iTerm2: Cmd+D right, Cmd+Shift+D below, Cmd+Option+arrows to move
            set("split-right", "Meta+D");
            set("split-down", "Meta+Shift+D");
            set("zoom-pane", "Meta+Shift+Return | Meta+Shift+Enter");
            for d in ["Left", "Right", "Up", "Down"] {
                set(&format!("focus-{}", d.to_lowercase()), &format!("Meta+Alt+{d}"));
                set(&format!("resize-{}", d.to_lowercase()), &format!("Meta+Alt+Shift+{d}"));
            }
            for n in 1..=9 {
                set(&format!("tab-{n}"), &format!("Meta+{n}"));
            }
            set("zoom-in", "Meta++ | Meta+=");
            set("zoom-out", "Meta+-");
            set("zoom-reset", "Meta+0");
            set("previous-command", "Meta+Up");
            set("next-command", "Meta+Down");
            set("copy-last-output", "Meta+Shift+O");
            set("find", "Meta+F");
            set("find-next", "Meta+G | F3");
            set("find-previous", "Meta+Shift+G | Shift+F3");
            set("scroll-top", "Meta+Home");
            set("scroll-bottom", "Meta+End");
            set("resize-mode", "Meta+Alt+R");
            set("edit-scrollback", "Meta+Alt+Shift+E");
            set("next-agent", "Meta+Alt+A");
            set("commands", "Meta+Alt+P");
            set("broadcast", "Meta+Alt+I");
            set("next-workspace", "Meta+Alt+PgDown");
            set("previous-workspace", "Meta+Alt+PgUp");
        }
        // Prefix Ctrl+B, then a key, as in tmux (a tmux running inside the terminal then needs another prefix)
        "tmux" => {
            set("new-tab", "Ctrl+B, C");
            set("new-window", "Ctrl+B, Shift+C");
            set("close-tab", "Ctrl+B, &");
            set("split-right", "Ctrl+B, %");
            set("split-down", "Ctrl+B, \"");
            set("close-pane", "Ctrl+B, X");
            set("zoom-pane", "Ctrl+B, Z");
            for d in ["Left", "Right", "Up", "Down"] {
                set(&format!("focus-{}", d.to_lowercase()), &format!("Ctrl+B, {d}"));
                set(&format!("resize-{}", d.to_lowercase()), &format!("Ctrl+B, Ctrl+{d}"));
            }
            set("resize-mode", "Ctrl+B, R");
            set("edit-scrollback", "Ctrl+B, E");
            set("next-agent", "Ctrl+B, A");
            set("commands", "Ctrl+B, :");
            set("broadcast", "Ctrl+B, Shift+S");
            set("next-workspace", "Ctrl+B, )");
            set("previous-workspace", "Ctrl+B, (");
            set("close-window", "Ctrl+B, Shift+X");
            set("next-tab", "Ctrl+B, N");
            set("previous-tab", "Ctrl+B, P");
            for n in 1..=9 {
                set(&format!("tab-{n}"), &format!("Ctrl+B, {n}"));
            }
            set("find", "Ctrl+B, /");
            set("page-up", "Ctrl+B, PgUp | Shift+PgUp");
            set("page-down", "Ctrl+B, PgDown | Shift+PgDown");
            set("scroll-top", "Ctrl+B, Home");
            set("scroll-bottom", "Ctrl+B, End");
        }
        _ => {}
    }
    v
}

/// The final key list: preset, then `[keys]`. `none` starts from nothing.
pub fn resolve(text: &str) -> Vec<(String, Vec<String>)> {
    let sections = parse_sections(text);
    let preset = match sections.get("").and_then(|t| t.get("keymap")) {
        Some(Val::Str(p)) if PRESETS.contains(&p.as_str()) => p.clone(),
        _ => "default".to_string(),
    };
    let over = overlay(&preset);
    let user = sections.get("keys");
    let mut out = Vec::new();
    for (action, keys) in DEFAULT {
        let mut value = if preset == "none" { "" } else { keys }.to_string();
        if let Some((_, k)) = over.iter().find(|(a, _)| a == action) {
            value = k.clone();
        }
        if let Some(v) = user.and_then(|u| u.get(*action)) {
            value = match v {
                Val::Str(s) => s.clone(),
                Val::List(l) => l.join(" | "),
            };
        }
        let list: Vec<String> = value.split('|').map(|k| k.trim().to_string()).filter(|k| !k.is_empty() && k != "none").collect();
        out.push((action.to_string(), list));
    }
    out
}

pub fn json_from(text: &str) -> String {
    let items: Vec<String> = resolve(text)
        .into_iter()
        .map(|(a, keys)| {
            let k: Vec<String> = keys.iter().map(|k| super::terminal::json_string(k)).collect();
            format!("{{\"action\":\"{a}\",\"keys\":[{}]}}", k.join(","))
        })
        .collect();
    format!("[{}]", items.join(","))
}

pub fn json() -> String {
    json_from(&std::fs::read_to_string(super::theme::path()).unwrap_or_default())
}

/// The preset named in the file ("default" when absent or unknown)
pub fn current(text: &str) -> String {
    match parse_sections(text).get("").and_then(|t| t.get("keymap")) {
        Some(Val::Str(p)) if PRESETS.contains(&p.as_str()) => p.clone(),
        _ => "default".to_string(),
    }
}

/// The text of the file with `keymap = "NAME"` set (replaced in place, else added before the first section)
pub fn with_keymap(text: &str, name: &str) -> String {
    let line = format!("keymap = \"{name}\"");
    let mut out: Vec<String> = Vec::new();
    let mut done = false;
    let mut in_top = true;
    for l in text.lines() {
        let t = l.trim_start();
        if t.starts_with('[') {
            if in_top && !done {
                out.push(line.clone());
                out.push(String::new());
                done = true;
            }
            in_top = false;
        }
        if in_top && !done && t.split('=').next().is_some_and(|k| k.trim() == "keymap") && t.contains('=') {
            out.push(line.clone());
            done = true;
            continue;
        }
        out.push(l.to_string());
    }
    if !done {
        out.push(line);
    }
    out.join("\n") + "\n"
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(text: &str, action: &str) -> Vec<String> {
        resolve(text).into_iter().find(|(a, _)| a == action).unwrap().1
    }

    #[test]
    fn default_and_presets() {
        assert_eq!(keys("", "new-tab"), ["Ctrl+Shift+T"]);
        assert_eq!(keys("keymap = \"mac\"", "new-tab"), ["Meta+T"]);
        assert_eq!(keys("keymap = \"tmux\"", "new-tab"), ["Ctrl+B, C"]);
        // What a preset does not change stays
        assert_eq!(keys("keymap = \"tmux\"", "copy"), ["Ctrl+Shift+C", "Ctrl+Ins", "Copy"]);
        assert_eq!(keys("keymap = \"none\"", "copy"), Vec::<String>::new());
        assert_eq!(keys("keymap = \"nonsense\"", "new-tab"), ["Ctrl+Shift+T"]);
    }

    #[test]
    fn keys_section_overrides_one_action() {
        let t = "keymap = \"mac\"\n[keys]\nnew-tab = \"Ctrl+T | Alt+T\"\nfind = \"\"\nzoom-in = [\"Ctrl+Up\"]\n";
        assert_eq!(keys(t, "new-tab"), ["Ctrl+T", "Alt+T"]);
        assert!(keys(t, "find").is_empty());
        assert_eq!(keys(t, "zoom-in"), ["Ctrl+Up"]);
        assert_eq!(keys(t, "close-pane"), ["Meta+W"]);
        assert_eq!(keys(t, "focus-left"), ["Meta+Alt+Left"]);
    }

    #[test]
    fn every_listed_action_is_known_to_the_window() {
        for (a, _) in DEFAULT {
            assert!(crate::ctl::ACTIONS.contains(a), "{a} is not a ctl action");
        }
    }

    #[test]
    fn writing_the_preset() {
        assert_eq!(with_keymap("", "mac"), "keymap = \"mac\"\n");
        assert_eq!(with_keymap("font_size = 13\n[theme]\nx = 1\n", "tmux"), "font_size = 13\nkeymap = \"tmux\"\n\n[theme]\nx = 1\n");
        assert_eq!(with_keymap("# c\nkeymap = \"mac\" # old\nfont_size = 1\n", "default"), "# c\nkeymap = \"default\"\nfont_size = 1\n");
        assert_eq!(current(&with_keymap("[keys]\nfind = \"\"\n", "mac")), "mac");
    }
}
