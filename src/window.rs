// Window management of the compositor, driven from the window panel of the header (copied from Facteur). Only niri is supported: its
// actions are run on the focused window (the one the user just clicked in) through `niri msg action`. The names are
// checked against a fixed list, so nothing but a window action can be run from the interface.
use std::process::{Command, Stdio};

// Every action the window panel offers (qml/WindowPanel.qml), in niri's own spelling
pub const ACTIONS: &[&str] = &[
    "fullscreen-window",
    "toggle-windowed-fullscreen",
    "maximize-column",
    "maximize-window-to-edges",
    "expand-column-to-available-width",
    "switch-preset-column-width",
    "switch-preset-window-height",
    "reset-window-height",
    "center-column",
    "center-window",
    "toggle-window-floating",
    "move-column-left",
    "move-column-right",
    "move-window-up",
    "move-window-down",
    "consume-or-expel-window-left",
    "consume-or-expel-window-right",
    "toggle-column-tabbed-display",
    "move-window-to-workspace-up",
    "move-window-to-workspace-down",
    "move-window-to-monitor-left",
    "move-window-to-monitor-right",
    "screenshot-window",
    "toggle-overview",
];

// Whether the session runs niri (its socket is announced in the environment) and the `niri` program is there
pub fn available() -> bool {
    std::env::var_os("NIRI_SOCKET").is_some_and(|s| !s.is_empty()) && on_path("niri")
}

fn on_path(program: &str) -> bool {
    std::env::var_os("PATH").is_some_and(|p| std::env::split_paths(&p).any(|d| d.join(program).is_file()))
}

// Runs one action of the list without waiting for it; false when the name is not on the list or niri is absent
pub fn run(name: &str) -> bool {
    if !ACTIONS.contains(&name) || !available() {
        return false;
    }
    Command::new("niri").args(["msg", "action", name]).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn().is_ok()
}

// First numbers of the array that follows `"key":[` in niri's JSON (compact), for example `"window_size":[1431,567]`
fn numbers(json: &str, key: &str, count: usize) -> Option<Vec<f64>> {
    let at = json.find(&format!("\"{key}\":["))? + key.len() + 4;
    let list: Vec<f64> = json[at..].split(']').next()?.split(',').filter_map(|n| n.trim().parse().ok()).collect();
    (list.len() >= count).then_some(list)
}

fn number(json: &str, key: &str) -> Option<f64> {
    let at = json.find(&format!("\"{key}\":"))? + key.len() + 3;
    json[at..].split([',', '}']).next()?.trim().parse().ok()
}

// Which panel actions describe the focused window as it is now (their names, comma separated): floating, filling the
// width of the screen (maximize column, fill the width), covering the whole screen (maximize to edges). niri does not
// report the rest (centered, tabbed, presets), so the panel cannot show it.
pub fn state() -> String {
    if !available() {
        return String::new();
    }
    let run = |args: &[&str]| Command::new("niri").args(args).stdin(Stdio::null()).stderr(Stdio::null()).output().map(|o| String::from_utf8_lossy(&o.stdout).into_owned()).unwrap_or_default();
    let window = run(&["msg", "-j", "focused-window"]);
    let mut on = state_of(&window, &run(&["msg", "-j", "focused-output"]));
    if stacked(&window, &run(&["msg", "-j", "windows"])) {
        on.push("stacked");
    }
    on.join(",")
}

// Whether the focused window shares its column with other tiled windows: the "join" actions then take it out of the
// column instead of merging it into the neighbour, and the column actions (tabs, move up/down) have something to act on
fn stacked(window: &str, windows: &str) -> bool {
    let key = |w: &str| Some((number(w, "workspace_id")?, numbers(w, "pos_in_scrolling_layout", 2)?[0]));
    let Some(mine) = key(window) else { return false };
    windows.split("{\"id\":").skip(1).filter(|w| !w.contains("\"is_floating\":true") && key(w) == Some(mine)).count() > 1
}

fn state_of(window: &str, output: &str) -> Vec<&'static str> {
    let mut on = Vec::new();
    let floating = window.contains("\"is_floating\":true");
    if floating {
        on.push("toggle-window-floating");
    }
    // The size that counts is the logical one (the "modes" of the output also have a width and a height)
    let logical = output.find("\"logical\"").map_or("", |i| &output[i..]);
    let (Some(size), Some(out_w), Some(out_h)) = (numbers(window, "window_size", 2), number(logical, "width"), number(logical, "height")) else { return on };
    if !floating {
        if size[0] >= out_w * 0.95 {
            on.push("maximize-column");
            on.push("expand-column-to-available-width");
        }
        if size[0] >= out_w - 1.0 && size[1] >= out_h - 1.0 {
            on.push("maximize-window-to-edges");
        }
    }
    on
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_of_a_window() {
        let out = r#"{"name":"eDP-1","modes":[{"width":2560,"height":1600}],"logical":{"x":0,"y":0,"width":1462,"height":914,"scale":1.75}}"#;
        let tiled = r#"{"id":1,"is_floating":false,"layout":{"window_size":[1431,567],"tile_size":[1430.8,566.8]}}"#;
        let full = r#"{"id":1,"is_floating":false,"layout":{"window_size":[1462,914]}}"#;
        let small = r#"{"id":1,"is_floating":true,"layout":{"window_size":[700,500]}}"#;
        assert_eq!(state_of(tiled, out), vec!["maximize-column", "expand-column-to-available-width"]);
        assert_eq!(state_of(full, out), vec!["maximize-column", "expand-column-to-available-width", "maximize-window-to-edges"]);
        assert_eq!(state_of(small, out), vec!["toggle-window-floating"]);
        assert!(state_of("", "").is_empty());
    }

    #[test]
    fn stacked_columns() {
        let me = r#"{"id":1,"workspace_id":3,"is_floating":false,"layout":{"pos_in_scrolling_layout":[2,1]}}"#;
        let one = r#"[{"id":1,"workspace_id":3,"is_floating":false,"layout":{"pos_in_scrolling_layout":[2,1]}},{"id":2,"workspace_id":3,"is_floating":false,"layout":{"pos_in_scrolling_layout":[1,1]}}]"#;
        let two = r#"[{"id":1,"workspace_id":3,"is_floating":false,"layout":{"pos_in_scrolling_layout":[2,1]}},{"id":2,"workspace_id":3,"is_floating":false,"layout":{"pos_in_scrolling_layout":[2,2]}}]"#;
        assert!(!stacked(me, one));
        assert!(stacked(me, two));
        assert!(!stacked("", two));
    }

    #[test]
    fn only_listed_actions_run() {
        assert!(!run("quit"));
        assert!(!run("spawn-sh"));
        assert!(!run(""));
    }

    // Every action the panel shows is on the list, and every listed action is in the panel
    #[test]
    fn the_panel_and_the_list_agree() {
        let qml = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/qml/WindowPanel.qml")).unwrap();
        let used: Vec<&str> = qml.split("action: \"").skip(1).filter_map(|t| t.split('"').next()).collect();
        for u in &used {
            assert!(ACTIONS.contains(u), "{u} is shown but not allowed");
        }
        for a in ACTIONS {
            assert!(used.contains(a), "{a} is allowed but not shown");
        }
    }
}
