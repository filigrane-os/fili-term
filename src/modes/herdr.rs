//! herdr (terminal workspaces for coding agents, herdr.dev): its socket API is newline-delimited JSON,
//! `{"id", "method", "params"}` answered by `{"id", "result"}` or `{"id", "error": {"code", "message"}}`.
//!
//! The adapter keeps two connections: one for requests (`session.snapshot`, focus, create, close), one subscribed to the
//! events. Events are only a sign that something changed: the state comes from a fresh snapshot (as herdr advises),
//! taken once the events of a burst are in. Agents' status changes are subscribed pane by pane (herdr wants a pane id),
//! so the subscription is renewed when the panes change.
//!
//! Window: the workspaces and the agents in the sidebar, the tabs of the focused workspace in the second tab row; a
//! notice when an agent waits for an answer (blocked) or ends its work (done).

use std::collections::{BTreeSet, HashMap};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use super::{Ctx, In, quote};

/// Events of herdr that change what the window shows (those without a pane id; agents' status come per pane)
const EVENTS: &[&str] = &[
    "workspace.created",
    "workspace.updated",
    "workspace.renamed",
    "workspace.moved",
    "workspace.reordered",
    "workspace.closed",
    "workspace.focused",
    "tab.created",
    "tab.closed",
    "tab.focused",
    "tab.renamed",
    "tab.moved",
    "pane.created",
    "pane.closed",
    "pane.focused",
    "pane.exited",
    "pane.agent_detected",
];

/// Events of a burst arriving within this delay make one snapshot
const SETTLE: Duration = Duration::from_millis(60);
const TIMEOUT: Duration = Duration::from_secs(3);

pub fn run(ctx: Ctx, rx: Receiver<In>) {
    let mut adapter = Adapter { ctx, generation: 0, sub: None, panes: BTreeSet::new(), statuses: None, shown: String::new() };
    adapter.main(&rx);
}

struct Adapter {
    ctx: Ctx,
    /// Generation of the event connection: the end of an older one is not a loss
    generation: u32,
    sub: Option<UnixStream>,
    /// Panes the subscription covers
    panes: BTreeSet<String>,
    /// Status of each agent (pane id) at the last snapshot; None before the first one (no notice for what was there)
    statuses: Option<HashMap<String, String>>,
    /// Last state sent (the periodic look sends nothing when nothing changed)
    shown: String,
}

/// Requests to herdr: one connection each (herdr closes a connection once it answered, unless it subscribed)
struct Requests {
    path: PathBuf,
    next: u64,
}

impl Requests {
    fn connect(path: &PathBuf) -> std::io::Result<Requests> {
        UnixStream::connect(path)?;
        Ok(Requests { path: path.clone(), next: 1 })
    }

    fn call(&mut self, method: &str, params: Value) -> Result<Value, String> {
        let id = format!("fili-term:{}", self.next);
        self.next += 1;
        let mut stream = UnixStream::connect(&self.path).map_err(|e| e.to_string())?;
        stream.set_read_timeout(Some(TIMEOUT)).map_err(|e| e.to_string())?;
        let line = json!({"id": id, "method": method, "params": params}).to_string() + "\n";
        stream.write_all(line.as_bytes()).map_err(|e| e.to_string())?;
        let mut reader = BufReader::new(stream);
        loop {
            let mut text = String::new();
            match reader.read_line(&mut text) {
                Ok(0) => return Err("herdr closed the connection".into()),
                Ok(_) => {}
                Err(e) => return Err(e.to_string()),
            }
            let Ok(v) = serde_json::from_str::<Value>(&text) else { continue };
            if v["id"] != id.as_str() {
                continue;
            }
            if let Some(e) = v.get("error") {
                return Err(e["message"].as_str().unwrap_or("error").to_string());
            }
            return Ok(v["result"].clone());
        }
    }
}

impl Adapter {
    fn main(&mut self, rx: &Receiver<In>) {
        loop {
            let path = self.socket();
            match path.as_ref().map(Requests::connect) {
                Some(Ok(mut req)) => {
                    if !self.connected(&mut req, rx) {
                        return;
                    }
                }
                other => {
                    let message = match other {
                        None => "herdr runs on another machine: no integration".to_string(),
                        _ => "Waiting for herdr…".to_string(),
                    };
                    self.shown.clear();
                    self.ctx.publish(format!(
                        "{{\"kind\":\"herdr\",\"title\":\"herdr\",\"connected\":false,\"message\":{},\"tabs\":[],\"sections\":[]}}",
                        quote(&message)
                    ));
                }
            }
            self.close_events();
            // Again in a moment (the server starting with its client, or restarting)
            let until = Instant::now() + Duration::from_millis(700);
            loop {
                match rx.recv_timeout(until.saturating_duration_since(Instant::now())) {
                    Ok(In::Stop) | Err(RecvTimeoutError::Disconnected) => return,
                    Err(RecvTimeoutError::Timeout) => break,
                    Ok(_) => {}
                }
            }
        }
    }

    /// Serve one connection to herdr; false when the mode stops
    fn connected(&mut self, req: &mut Requests, rx: &Receiver<In>) -> bool {
        // Subscribe first, then the snapshot: nothing falls between the two
        let snapshot = match req.call("session.snapshot", json!({})) {
            Ok(s) => s,
            Err(e) => return self.failed(&e),
        };
        if let Err(e) = self.subscribe(&panes_of(&snapshot["snapshot"])) {
            return self.failed(&e.to_string());
        }
        match req.call("session.snapshot", json!({})) {
            Ok(s) => self.show(&s["snapshot"]),
            Err(e) => return self.failed(&e),
        }
        let mut dirty: Option<Instant> = None;
        loop {
            let wait = dirty.map_or(Duration::from_secs(5), |at| at.saturating_duration_since(Instant::now()));
            match rx.recv_timeout(wait) {
                Ok(In::Stop) | Err(RecvTimeoutError::Disconnected) => return false,
                Ok(In::Changed(g)) if g == self.generation => {
                    dirty.get_or_insert_with(|| Instant::now() + SETTLE);
                }
                Ok(In::Lost(g)) if g == self.generation => return true,
                Ok(In::Action(a)) => {
                    if let Err(e) = self.act(req, &a) {
                        self.ctx.reply(json!({"op": "error", "message": e}).to_string());
                    }
                    dirty.get_or_insert_with(|| Instant::now() + SETTLE);
                }
                Ok(_) => {}
                Err(RecvTimeoutError::Timeout) => {
                    // A periodic look as well: a lost event must not leave a stale sidebar
                    dirty = None;
                    let Ok(snapshot) = req.call("session.snapshot", json!({})) else { return true };
                    let snapshot = &snapshot["snapshot"];
                    let panes = panes_of(snapshot);
                    if panes != self.panes && self.subscribe(&panes).is_err() {
                        return true;
                    }
                    self.show(snapshot);
                }
            }
        }
    }

    /// A connection that broke: said on the daemon's error output, tried again
    fn failed(&self, e: &str) -> bool {
        eprintln!("fili-term-daemon: herdr: {e}");
        true
    }

    fn socket(&self) -> Option<PathBuf> {
        Some(self.ctx.socket.clone())
    }

    fn subscribe(&mut self, panes: &BTreeSet<String>) -> std::io::Result<()> {
        self.close_events();
        self.generation += 1;
        let path = self.socket().ok_or_else(|| std::io::Error::other("remote"))?;
        let mut stream = UnixStream::connect(path)?;
        let mut subs: Vec<Value> = EVENTS.iter().map(|t| json!({"type": t})).collect();
        for p in panes {
            subs.push(json!({"type": "pane.agent_status_changed", "pane_id": p}));
        }
        let line = json!({"id": "fili-term:events", "method": "events.subscribe", "params": {"subscriptions": subs}});
        stream.write_all((line.to_string() + "\n").as_bytes())?;
        let mut reader = BufReader::new(stream.try_clone()?);
        let mut first = String::new();
        stream.set_read_timeout(Some(TIMEOUT))?;
        reader.read_line(&mut first)?;
        if !first.contains("subscription_started") {
            return Err(std::io::Error::other(first));
        }
        stream.set_read_timeout(None)?;
        let (tx, generation) = (self.ctx.tx.clone(), self.generation);
        std::thread::Builder::new().name("herdr-events".into()).spawn(move || {
            let mut line = String::new();
            loop {
                line.clear();
                match reader.read_line(&mut line) {
                    // events_lost (an error with our id) ends the subscription too: reconnect
                    Ok(n) if n > 0 && !line.contains("\"error\"") => {
                        if tx.send(In::Changed(generation)).is_err() {
                            return;
                        }
                    }
                    _ => {
                        let _ = tx.send(In::Lost(generation));
                        return;
                    }
                }
            }
        })?;
        self.sub = Some(stream);
        self.panes = panes.clone();
        Ok(())
    }

    fn close_events(&mut self) {
        if let Some(s) = self.sub.take() {
            let _ = s.shutdown(std::net::Shutdown::Both);
        }
    }

    /// An action of the window (see the actions of mod.rs and the context menus of Main.qml)
    fn act(&mut self, req: &mut Requests, a: &Value) -> Result<(), String> {
        let id = a["id"].as_str().unwrap_or("");
        let label = || a["label"].as_str().map(str::trim).filter(|l| !l.is_empty());
        let (method, params) = match (a["op"].as_str().unwrap_or(""), a["section"].as_str().unwrap_or("")) {
            ("tab", _) => ("tab.focus", json!({"tab_id": id})),
            ("close-tab", _) => ("tab.close", json!({"tab_id": id})),
            ("rename-tab", _) => ("tab.rename", json!({"tab_id": id, "label": label().ok_or("empty name")?})),
            ("new-tab", _) => {
                let snapshot = req.call("session.snapshot", json!({}))?;
                let ws = snapshot["snapshot"]["focused_workspace_id"].clone();
                ("tab.create", json!({"workspace_id": ws, "focus": true}))
            }
            ("select", "workspaces") => ("workspace.focus", json!({"workspace_id": id})),
            ("select", "agents") => ("agent.focus", json!({"target": id})),
            ("new", "workspaces") => ("workspace.create", json!({"focus": true})),
            ("rename", "workspaces") => ("workspace.rename", json!({"workspace_id": id, "label": label().ok_or("empty name")?})),
            ("close", "workspaces") => ("workspace.close", json!({"workspace_id": id, "close_group": a["group"].as_bool().unwrap_or(false)})),
            // Worktrees of a workspace's repository: listed for the menu, a new one on a branch, one opened, one removed
            ("worktrees", _) => {
                let r = req.call("worktree.list", json!({"workspace_id": id}))?;
                self.ctx.reply(json!({"op": "worktrees", "id": id, "items": r["worktrees"]}).to_string());
                return Ok(());
            }
            ("new-worktree", _) => (
                "worktree.create",
                json!({"workspace_id": id, "branch": label().ok_or("empty branch name")?, "focus": true}),
            ),
            ("open-worktree", _) => ("worktree.open", json!({"workspace_id": id, "path": a["path"], "focus": true})),
            ("remove-worktree", _) => ("worktree.remove", json!({"workspace_id": id, "force": a["force"].as_bool().unwrap_or(false)})),
            ("rename", "panes") => ("pane.rename", json!({"pane_id": id, "label": label()})),
            ("close", "panes") => ("pane.close", json!({"pane_id": id})),
            ("split", _) => (
                "pane.split",
                json!({"target_pane_id": id, "direction": a["direction"].as_str().unwrap_or("right"), "focus": true}),
            ),
            ("move-tab", _) => ("tab.move", json!({"tab_id": id, "insert_index": a["index"].as_u64().unwrap_or(0)})),
            ("reload-config", _) => ("server.reload_config", json!({})),
            ("swap", _) => ("pane.swap", json!({"source_pane_id": id, "target_pane_id": a["with"]})),
            // The panes of a tab, as Aéroport lays them out: the keyboard, zoom, a divider moved by keys or dragged
            ("focus-pane", _) => ("pane.focus", json!({"pane_id": id})),
            ("zoom", _) => ("pane.zoom", json!({"pane_id": id})),
            ("resize", _) => ("pane.resize", json!({"pane_id": id, "direction": a["direction"].as_str().unwrap_or("right")})),
            ("ratio", _) => (
                "layout.set_split_ratio",
                json!({"tab_id": id, "path": a["path"], "ratio": a["ratio"].as_f64().unwrap_or(0.5).clamp(0.05, 0.95)}),
            ),
            ("rename", "agents") => ("agent.rename", json!({"target": id, "name": label()})),
            (op, section) => return Err(format!("unknown action {op} {section}")),
        };
        req.call(method, params).map(|_| ())
    }

    /// State for the window, and notices for the agents that need the user
    fn show(&mut self, snap: &Value) {
        let (state, statuses, notices) = state_of(snap, self.statuses.as_ref());
        self.statuses = Some(statuses);
        if state != self.shown {
            self.shown = state.clone();
            self.ctx.publish(state);
        }
        for n in notices {
            self.ctx.notice(n);
        }
    }
}

/// herdr's key bindings (its `[keys]` defaults, 0.9.5): Aéroport shows herdr's panes without herdr's client, so it
/// answers them itself in herdr's tabs. Actions without an equivalent here (help, zoom, resize, edit_scrollback…) are left
/// out.
const KEYS: &[(&str, &str)] = &[
    ("settings", "prefix+s"),
    ("detach", "prefix+q"),
    ("reload_config", "prefix+shift+r"),
    ("open_notification_target", "prefix+o"),
    ("workspace_picker", "prefix+w"),
    ("goto", "prefix+g"),
    ("new_workspace", "prefix+shift+n"),
    ("new_worktree", "prefix+shift+g"),
    ("open_worktree", ""),
    ("remove_worktree", ""),
    ("rename_workspace", "prefix+shift+w"),
    ("close_workspace", "prefix+shift+d"),
    ("previous_workspace", ""),
    ("next_workspace", ""),
    ("previous_agent", ""),
    ("next_agent", ""),
    ("focus_agent", ""),
    ("new_tab", "prefix+c"),
    ("rename_tab", "prefix+shift+t"),
    ("previous_tab", "prefix+p"),
    ("next_tab", "prefix+n"),
    ("move_tab_previous", ""),
    ("move_tab_next", ""),
    ("switch_tab", "prefix+1..9"),
    ("switch_workspace", ""),
    ("close_tab", "prefix+shift+x"),
    ("rename_pane", "prefix+shift+p"),
    ("focus_pane_left", "prefix+h"),
    ("focus_pane_down", "prefix+j"),
    ("focus_pane_up", "prefix+k"),
    ("focus_pane_right", "prefix+l"),
    ("cycle_pane_next", "prefix+tab"),
    ("cycle_pane_previous", "prefix+shift+tab"),
    ("last_pane", ""),
    ("split_vertical", "prefix+v"),
    ("split_horizontal", "prefix+minus"),
    ("close_pane", "prefix+x"),
    ("zoom", "prefix+z"),
    ("resize_pane_left", ""),
    ("resize_pane_down", ""),
    ("resize_pane_up", ""),
    ("resize_pane_right", ""),
    ("toggle_sidebar", "prefix+b"),
];

/// `[ui] right_click_passthrough_modifier` of herdr's config as a Qt modifier mask ("ctrl+alt" → Ctrl|Alt; empty, "off"
/// or unknown: 0, the menu always; Shift is refused, as herdr does)
pub fn right_click_pass(config: &str) -> i32 {
    let mut section = String::new();
    for line in config.lines() {
        let line = line.split(" #").next().unwrap_or("").trim();
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        if line.starts_with('[') {
            section = line.trim_matches(|c| c == '[' || c == ']').trim().to_string();
            continue;
        }
        let Some((k, v)) = line.split_once('=') else { continue };
        if section != "ui" || k.trim() != "right_click_passthrough_modifier" {
            continue;
        }
        let mut mask = 0;
        for m in v.trim().trim_matches('"').split('+').map(|m| m.trim().to_ascii_lowercase()) {
            mask |= match m.as_str() {
                "ctrl" | "control" => 0x0400_0000,
                "alt" | "option" => 0x0800_0000,
                "super" | "cmd" | "meta" => 0x1000_0000,
                _ => return 0,
            };
        }
        return mask;
    }
    0
}

/// herdr's config file: HERDR_CONFIG_PATH, else config.toml in its folder of the configuration
pub fn config_path() -> PathBuf {
    if let Some(p) = std::env::var_os("HERDR_CONFIG_PATH").filter(|p| !p.is_empty()) {
        return PathBuf::from(p);
    }
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|c| !c.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(std::env::var("HOME").unwrap_or_default()).join(".config"));
    base.join("herdr").join("config.toml")
}

/// One key of herdr's syntax ("ctrl+b", "shift+x", "minus", "?") as Qt writes it ("Ctrl+B", "Shift+X", "-", "?")
fn qt_key(spec: &str) -> Option<String> {
    let mut parts: Vec<String> = Vec::new();
    let tokens: Vec<&str> = spec.split('+').collect();
    let (mods, key) = tokens.split_at(tokens.len().checked_sub(1)?);
    for m in mods {
        parts.push(
            match m.to_ascii_lowercase().as_str() {
                "ctrl" | "control" => "Ctrl",
                "shift" => "Shift",
                "alt" | "option" => "Alt",
                "cmd" | "super" | "meta" => "Meta",
                _ => return None,
            }
            .to_string(),
        );
    }
    let k = key[0].to_ascii_lowercase();
    let named = match k.as_str() {
        "" => return None,
        "minus" => "-",
        "plus" => "+",
        "comma" => ",",
        "period" | "dot" => ".",
        "slash" => "/",
        "backslash" => "\\",
        "backtick" | "grave" => "`",
        "ampersand" => "&",
        "question" => "?",
        "tab" => "Tab",
        "enter" | "return" => "Return",
        "esc" | "escape" => "Esc",
        "space" => "Space",
        "left" => "Left",
        "right" => "Right",
        "up" => "Up",
        "down" => "Down",
        "home" => "Home",
        "end" => "End",
        "pageup" => "PgUp",
        "pagedown" => "PgDown",
        "backspace" => "Backspace",
        "delete" => "Del",
        _ => "",
    };
    let name = if !named.is_empty() {
        named.to_string()
    } else if k.len() > 1 && k.starts_with('f') && k[1..].parse::<u8>().is_ok() {
        k.to_ascii_uppercase()
    } else if k.chars().count() == 1 {
        k.to_ascii_uppercase()
    } else {
        return None;
    };
    parts.push(name);
    Some(parts.join("+"))
}

/// A binding of herdr ("prefix+shift+x", "ctrl+alt+n", "prefix+1..9") as Qt key sequences, with the index for a range
fn qt_binding(binding: &str, prefix: &str) -> Vec<(Option<u32>, String)> {
    let binding = binding.trim();
    let (lead, rest) = match binding.strip_prefix("prefix+") {
        Some(rest) => (qt_key(prefix).map(|p| format!("{p}, ")), rest),
        None => (Some(String::new()), binding),
    };
    let Some(lead) = lead else { return Vec::new() };
    // An indexed range: the last part is "1..9"
    if let Some((head, range)) = rest.rsplit_once('+').map(|(h, r)| (Some(h), r)).or(Some((None, rest))).filter(|(_, r)| r.contains("..")) {
        let (a, b) = range.split_once("..").unwrap_or(("", ""));
        let (Ok(a), Ok(b)) = (a.parse::<u32>(), b.parse::<u32>()) else { return Vec::new() };
        return (a..=b.min(9))
            .filter_map(|n| qt_key(&head.map_or(n.to_string(), |h| format!("{h}+{n}"))).map(|k| (Some(n), format!("{lead}{k}"))))
            .collect();
    }
    qt_key(rest).map(|k| vec![(None, format!("{lead}{k}"))]).unwrap_or_default()
}

/// herdr's bindings for the window, JSON [{"action": "new_tab", "keys": ["Ctrl+B, C"]}, {"action": "switch_tab:1", …}]:
/// the defaults, then the `[keys]` section of herdr's config (an empty value unbinds)
pub fn keys_json(config: &str) -> String {
    let mut section = String::new();
    let mut set: HashMap<String, String> = HashMap::new();
    for line in config.lines() {
        let line = line.split(" #").next().unwrap_or("").trim();
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        if line.starts_with('[') {
            section = line.trim_matches(|c| c == '[' || c == ']').trim().to_string();
            continue;
        }
        if section == "keys" {
            if let Some((k, v)) = line.split_once('=') {
                set.insert(k.trim().to_string(), v.trim().trim_matches('"').to_string());
            }
        }
    }
    let prefix = set.get("prefix").cloned().unwrap_or_else(|| "ctrl+b".into());
    // `fullscreen` is herdr's older name of `zoom`
    if let Some(f) = set.get("fullscreen").cloned() {
        set.entry("zoom".into()).or_insert(f);
    }
    let mut out: Vec<Value> = Vec::new();
    for (action, default) in KEYS {
        let binding = set.get(*action).cloned().unwrap_or_else(|| default.to_string());
        let mut indexed: std::collections::BTreeMap<u32, Vec<String>> = std::collections::BTreeMap::new();
        let mut plain: Vec<String> = Vec::new();
        // Alternatives: herdr accepts a list in some bindings; a comma never appears in a single one here
        for b in binding.split('|').filter(|b| !b.trim().is_empty()) {
            for (n, k) in qt_binding(b, &prefix) {
                match n {
                    Some(n) => indexed.entry(n).or_default().push(k),
                    None => plain.push(k),
                }
            }
        }
        if !plain.is_empty() {
            out.push(json!({"action": action, "keys": plain}));
        }
        for (n, keys) in indexed {
            out.push(json!({"action": format!("{action}:{n}"), "keys": keys}));
        }
    }
    Value::Array(out).to_string()
}

/// Socket of a herdr client running as `pid` (its own options and environment), else the default one of this process'
/// environment. None for a client of a remote herdr.
pub fn socket_for(pid: Option<u32>) -> Option<PathBuf> {
    let args: Vec<String> = pid
        .and_then(|p| std::fs::read(format!("/proc/{p}/cmdline")).ok())
        .map(|b| b.split(|c| *c == 0).map(|a| String::from_utf8_lossy(a).into_owned()).collect())
        .unwrap_or_default();
    let env: HashMap<String, String> = pid
        .and_then(|p| std::fs::read(format!("/proc/{p}/environ")).ok())
        .map(|b| {
            b.split(|c| *c == 0)
                .filter_map(|kv| {
                    let kv = String::from_utf8_lossy(kv);
                    kv.split_once('=').map(|(k, v)| (k.to_string(), v.to_string()))
                })
                .collect()
        })
        .unwrap_or_else(|| std::env::vars().collect());
    socket_path(&args, &env)
}

/// Where a herdr client connects, from its arguments and environment (herdr's order: `--session NAME`,
/// HERDR_SOCKET_PATH, HERDR_SESSION, then the default session). None for `--remote`.
fn socket_path(args: &[String], env: &HashMap<String, String>) -> Option<PathBuf> {
    let mut session: Option<String> = None;
    let mut it = args.iter().skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
            "--remote" | "--machine" => return None,
            "--session" => session = it.next().cloned(),
            "attach" if session.is_none() => session = it.next().cloned(),
            a if a.starts_with("--session=") => session = Some(a["--session=".len()..].to_string()),
            a if a.starts_with("--remote=") => return None,
            _ => {}
        }
    }
    if session.is_none() {
        if let Some(p) = env.get("HERDR_SOCKET_PATH").filter(|p| !p.is_empty()) {
            return Some(PathBuf::from(p));
        }
        session = env.get("HERDR_SESSION").filter(|s| !s.is_empty()).cloned();
    }
    let config = env
        .get("XDG_CONFIG_HOME")
        .filter(|c| !c.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env.get("HOME").cloned().unwrap_or_default()).join(".config"))
        .join("herdr");
    Some(match session {
        Some(name) => config.join("sessions").join(name).join("herdr.sock"),
        None => config.join("herdr.sock"),
    })
}

/// A tab's layout as a tree for the window: a split {dir, ratio, path, a, b} (path: the booleans herdr's
/// layout.set_split_ratio wants, true for the second child) or a pane (one of `items`, with its terminal). herdr gives
/// the splits flat, named split_<n>_<path> ("root" for the top one, "0"/"1" per level), and the panes by rectangle:
/// a pane goes where the middle of its rectangle falls.
fn layout_tree(layout: &Value, items: &[Value]) -> Value {
    let empty = Vec::new();
    let mut splits: HashMap<String, &Value> = HashMap::new();
    for sp in layout["splits"].as_array().unwrap_or(&empty) {
        let id = sp["id"].as_str().unwrap_or("");
        let path = id.rsplit('_').next().unwrap_or("");
        splits.insert(if path == "root" { String::new() } else { path.to_string() }, sp);
    }
    let rects: Vec<(&str, f64, f64)> = layout["panes"]
        .as_array()
        .unwrap_or(&empty)
        .iter()
        .filter_map(|p| {
            let r = &p["rect"];
            let mid = |a: &str, b: &str| r[a].as_f64().unwrap_or(0.0) + r[b].as_f64().unwrap_or(0.0) / 2.0;
            Some((p["pane_id"].as_str()?, mid("x", "width"), mid("y", "height")))
        })
        .collect();
    let item = |id: &str| items.iter().find(|i| i["id"] == id).cloned().unwrap_or_else(|| json!({"id": id}));
    // The region of a path: its rectangle cut by the splits above it
    fn build(
        path: &str, x: f64, y: f64, w: f64, h: f64, splits: &HashMap<String, &Value>, rects: &[(&str, f64, f64)],
        item: &dyn Fn(&str) -> Value,
    ) -> Value {
        if let Some(sp) = splits.get(path) {
            let ratio = sp["ratio"].as_f64().unwrap_or(0.5);
            let dir = sp["direction"].as_str().unwrap_or("right");
            let bools: Vec<bool> = path.chars().map(|c| c == '1').collect();
            let (a, b) = if dir == "right" {
                let cut = w * ratio;
                (build(&format!("{path}0"), x, y, cut, h, splits, rects, item), build(&format!("{path}1"), x + cut, y, w - cut, h, splits, rects, item))
            } else {
                let cut = h * ratio;
                (build(&format!("{path}0"), x, y, w, cut, splits, rects, item), build(&format!("{path}1"), x, y + cut, w, h - cut, splits, rects, item))
            };
            return json!({"dir": dir, "ratio": ratio, "path": bools, "a": a, "b": b});
        }
        let inside = |(_, mx, my): &&(&str, f64, f64)| *mx >= x && *mx < x + w && *my >= y && *my < y + h;
        match rects.iter().find(inside) {
            Some((id, _, _)) => item(id),
            None => Value::Null,
        }
    }
    let area = &layout["area"];
    let f = |k: &str| area[k].as_f64().unwrap_or(0.0);
    let tree = build("", f("x"), f("y"), f("width").max(1.0), f("height").max(1.0), &splits, &rects, &item);
    if tree.is_null() { items.first().cloned().unwrap_or(Value::Null) } else { tree }
}

fn panes_of(snap: &Value) -> BTreeSet<String> {
    let mut panes = BTreeSet::new();
    for p in snap["panes"].as_array().into_iter().flatten() {
        if let Some(id) = p["pane_id"].as_str() {
            panes.insert(id.to_string());
        }
    }
    panes
}

fn text(v: &Value) -> Option<&str> {
    v.as_str().filter(|s| !s.is_empty())
}

fn by_number(items: &mut [&Value]) {
    items.sort_by_key(|v| v["number"].as_u64().unwrap_or(u64::MAX));
}

/// Name of an agent: what herdr shows (display_agent), else the agent's program, its name, its title
fn agent_label(a: &Value) -> String {
    text(&a["display_agent"])
        .or_else(|| text(&a["agent"]))
        .or_else(|| text(&a["name"]))
        .or_else(|| text(&a["title"]))
        .unwrap_or("agent")
        .to_string()
}

/// (state JSON, status per agent, notices) from a `session.snapshot`
fn state_of(snap: &Value, before: Option<&HashMap<String, String>>) -> (String, HashMap<String, String>, Vec<String>) {
    let empty = Vec::new();
    let mut workspaces: Vec<&Value> = snap["workspaces"].as_array().unwrap_or(&empty).iter().collect();
    by_number(&mut workspaces);
    let focused_ws = snap["focused_workspace_id"].as_str().unwrap_or("");
    let ws_label: HashMap<&str, &str> =
        workspaces.iter().filter_map(|w| Some((w["workspace_id"].as_str()?, w["label"].as_str().unwrap_or("")))).collect();

    let mut tabs: Vec<&Value> = snap["tabs"].as_array().unwrap_or(&empty).iter().filter(|t| t["workspace_id"] == focused_ws).collect();
    by_number(&mut tabs);
    let active_tab = workspaces
        .iter()
        .find(|w| w["workspace_id"] == focused_ws)
        .and_then(|w| w["active_tab_id"].as_str())
        .or_else(|| snap["focused_tab_id"].as_str())
        .unwrap_or("");
    // Panes of each tab (in the order of its layout) and the one focused in it: the tab of Aéroport shows that one
    let panes: HashMap<&str, &Value> = snap["panes"].as_array().unwrap_or(&empty).iter().filter_map(|p| Some((p["pane_id"].as_str()?, p))).collect();
    let layouts: HashMap<&str, &Value> =
        snap["layouts"].as_array().unwrap_or(&empty).iter().filter_map(|l| Some((l["tab_id"].as_str()?, l))).collect();
    let agent_names: HashMap<&str, String> = snap["agents"]
        .as_array()
        .unwrap_or(&empty)
        .iter()
        .filter_map(|a| Some((a["pane_id"].as_str()?, agent_label(a))))
        .collect();
    let tab_items: Vec<Value> = tabs
        .iter()
        .map(|t| {
            let id = t["tab_id"].as_str().unwrap_or("");
            let layout = layouts.get(id);
            let mut ids: Vec<&str> = layout
                .and_then(|l| l["panes"].as_array())
                .map(|ps| ps.iter().filter_map(|p| p["pane_id"].as_str()).collect())
                .unwrap_or_default();
            if ids.is_empty() {
                ids = snap["panes"].as_array().unwrap_or(&empty).iter().filter(|p| p["tab_id"] == id).filter_map(|p| p["pane_id"].as_str()).collect();
            }
            let focused = layout.and_then(|l| l["focused_pane_id"].as_str()).or_else(|| ids.first().copied()).unwrap_or("");
            let pane_items: Vec<Value> = ids
                .iter()
                .enumerate()
                .filter_map(|(i, pid)| {
                    let p = panes.get(pid)?;
                    let label = text(&p["label"])
                        .map(str::to_string)
                        .or_else(|| agent_names.get(pid).cloned())
                        .or_else(|| text(&p["terminal_title_stripped"]).map(str::to_string))
                        .unwrap_or_else(|| format!("{}", i + 1));
                    Some(json!({
                        "id": pid, "terminal": p["terminal_id"], "label": label, "status": p["agent_status"],
                        "focused": *pid == focused, "cwd": p["foreground_cwd"].as_str().or_else(|| p["cwd"].as_str()).unwrap_or(""),
                    }))
                })
                .collect();
            let main = panes.get(focused);
            let tree = layout.map_or(Value::Null, |l| layout_tree(l, &pane_items));
            json!({
                "id": id, "label": text(&t["label"]).map_or_else(|| t["number"].to_string(), str::to_string),
                "active": id == active_tab, "status": t["agent_status"],
                "pane": focused, "terminal": main.map_or(Value::Null, |p| p["terminal_id"].clone()),
                "cwd": main.and_then(|p| p["foreground_cwd"].as_str().or_else(|| p["cwd"].as_str())).unwrap_or(""),
                "zoomed": layout.and_then(|l| l["zoomed"].as_bool()).unwrap_or(false),
                "layout": tree,
                "panes": pane_items,
            })
        })
        .collect();

    // Worktrees of one repository stay together, the linked ones under the main checkout (as in herdr's sidebar)
    let repo = |w: &Value| w["worktree"]["repo_key"].as_str().map(str::to_string);
    let linked = |w: &Value| w["worktree"]["is_linked_worktree"].as_bool().unwrap_or(false);
    let mut ordered: Vec<&Value> = Vec::new();
    for w in &workspaces {
        if ordered.iter().any(|o| std::ptr::eq(*o, *w)) || (linked(w) && workspaces.iter().any(|m| !linked(m) && repo(m) == repo(w))) {
            continue;
        }
        ordered.push(w);
        if let Some(r) = repo(w).filter(|_| !linked(w)) {
            ordered.extend(workspaces.iter().filter(|o| linked(o) && repo(o).as_deref() == Some(r.as_str())));
        }
    }
    let ws_items: Vec<Value> = ordered
        .iter()
        .map(|w| {
            let group = repo(w).is_some_and(|r| workspaces.iter().filter(|o| repo(o).as_deref() == Some(r.as_str())).count() > 1);
            json!({
                "id": w["workspace_id"], "label": text(&w["label"]).map_or_else(|| w["number"].to_string(), str::to_string),
                "detail": w["tab_count"].as_u64().filter(|n| *n > 1).map(|n| n.to_string()).unwrap_or_default(),
                "status": w["agent_status"], "active": w["workspace_id"] == focused_ws,
                "indent": if linked(w) && ordered.iter().any(|m| !linked(m) && repo(m) == repo(w)) { 1 } else { 0 },
                "repo": w["worktree"].is_object(), "linked": linked(w), "group": group,
            })
        })
        .collect();

    let mut agents: Vec<&Value> = snap["agents"].as_array().unwrap_or(&empty).iter().collect();
    agents.sort_by_key(|a| (a["workspace_id"].as_str().unwrap_or("").to_string(), a["pane_id"].as_str().unwrap_or("").to_string()));
    let focused_pane = snap["focused_pane_id"].as_str().unwrap_or("");
    let mut statuses = HashMap::new();
    let mut notices = Vec::new();
    let agent_items: Vec<Value> = agents
        .iter()
        .map(|a| {
            let pane = a["pane_id"].as_str().unwrap_or("");
            let status = a["agent_status"].as_str().unwrap_or("unknown");
            let label = agent_label(a);
            let place = ws_label.get(a["workspace_id"].as_str().unwrap_or("")).copied().unwrap_or("");
            if let Some(before) = before {
                let was = before.get(pane).map(String::as_str).unwrap_or("");
                let wants = (status == "blocked" && was != "blocked") || (status == "done" && was == "working");
                if wants {
                    notices.push(json!({"kind": status, "agent": label, "place": place}).to_string());
                }
            }
            statuses.insert(pane.to_string(), status.to_string());
            json!({"id": pane, "label": label, "detail": place, "status": status, "active": pane == focused_pane, "workspace": a["workspace_id"]})
        })
        .collect();

    let state = json!({
        "kind": "herdr", "title": "herdr", "connected": true, "message": "",
        "tabs": tab_items, "addTab": true,
        "sections": [
            {"key": "workspaces", "title": "Workspaces", "add": true, "items": ws_items},
            {"key": "agents", "title": "Agents", "add": false, "items": agent_items},
        ],
    });
    (state.to_string(), statuses, notices)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }
    fn args(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn socket_resolution() {
        let home = env(&[("HOME", "/home/u")]);
        assert_eq!(socket_path(&args(&["herdr"]), &home), Some("/home/u/.config/herdr/herdr.sock".into()));
        assert_eq!(socket_path(&args(&["herdr", "--session", "work"]), &home), Some("/home/u/.config/herdr/sessions/work/herdr.sock".into()));
        assert_eq!(socket_path(&args(&["herdr", "session", "attach", "x"]), &home), Some("/home/u/.config/herdr/sessions/x/herdr.sock".into()));
        assert_eq!(socket_path(&args(&["herdr", "--remote", "host"]), &home), None);
        let e = env(&[("HOME", "/home/u"), ("XDG_CONFIG_HOME", "/c"), ("HERDR_SESSION", "s")]);
        assert_eq!(socket_path(&args(&["herdr"]), &e), Some("/c/herdr/sessions/s/herdr.sock".into()));
        let e = env(&[("HOME", "/home/u"), ("HERDR_SOCKET_PATH", "/run/h.sock")]);
        assert_eq!(socket_path(&args(&["herdr"]), &e), Some("/run/h.sock".into()));
    }

    /// A snapshot as herdr 0.9.5 returns it (session.snapshot, trimmed)
    const SNAPSHOT: &str = r#"{"agents":[{"agent":"claude","agent_status":"working","pane_id":"w1:p2","tab_id":"w1:t2","workspace_id":"w1","focused":false,"revision":0,"terminal_id":"t"}],
        "focused_pane_id":"w1:p1","focused_tab_id":"w1:t1","focused_workspace_id":"w1",
        "layouts":[{"tab_id":"w1:t1","focused_pane_id":"w1:p3","panes":[{"pane_id":"w1:p1"},{"pane_id":"w1:p3"}]}],
        "panes":[{"pane_id":"w1:p1","tab_id":"w1:t1","terminal_id":"term_a","cwd":"/a"},{"pane_id":"w1:p2","tab_id":"w1:t2","terminal_id":"term_b"},
                 {"pane_id":"w1:p3","tab_id":"w1:t1","terminal_id":"term_c","label":"logs","foreground_cwd":"/c"}],"protocol":22,
        "tabs":[{"agent_status":"working","focused":false,"label":"logs","number":2,"pane_count":1,"tab_id":"w1:t2","workspace_id":"w1"},
                {"agent_status":"unknown","focused":true,"label":"1","number":1,"pane_count":1,"tab_id":"w1:t1","workspace_id":"w1"},
                {"agent_status":"idle","focused":false,"label":"x","number":1,"pane_count":1,"tab_id":"w2:t1","workspace_id":"w2"}],
        "version":"0.9.5",
        "workspaces":[{"active_tab_id":"w2:t1","agent_status":"idle","focused":false,"label":"web","number":2,"pane_count":1,"tab_count":1,"workspace_id":"w2"},
                      {"active_tab_id":"w1:t1","agent_status":"working","focused":true,"label":"demo","number":1,"pane_count":2,"tab_count":2,"workspace_id":"w1"}]}"#;

    #[test]
    fn key_bindings() {
        let v: Value = serde_json::from_str(&keys_json("")).unwrap();
        let find = |a: &str| v.as_array().unwrap().iter().find(|x| x["action"] == a).map(|x| x["keys"].clone());
        assert_eq!(find("new_tab"), Some(json!(["Ctrl+B, C"])));
        assert_eq!(find("close_tab"), Some(json!(["Ctrl+B, Shift+X"])));
        assert_eq!(find("split_horizontal"), Some(json!(["Ctrl+B, -"])));
        assert_eq!(find("cycle_pane_previous"), Some(json!(["Ctrl+B, Shift+Tab"])));
        assert_eq!(find("switch_tab:3"), Some(json!(["Ctrl+B, 3"])));
        assert_eq!(find("open_worktree"), None);
        let user = "[ui]\nnew_tab = \"x\"\n[keys]\nprefix = \"ctrl+a\"\nnew_tab = \"ctrl+alt+t\" # direct\nclose_tab = \"\"\nswitch_workspace = \"prefix+shift+1..9\"\n";
        let v: Value = serde_json::from_str(&keys_json(user)).unwrap();
        let find = |a: &str| v.as_array().unwrap().iter().find(|x| x["action"] == a).map(|x| x["keys"].clone());
        assert_eq!(find("new_tab"), Some(json!(["Ctrl+Alt+T"])));
        assert_eq!(find("close_tab"), None);
        assert_eq!(find("previous_tab"), Some(json!(["Ctrl+A, P"])));
        assert_eq!(find("switch_workspace:2"), Some(json!(["Ctrl+A, Shift+2"])));
    }

    #[test]
    fn right_click_passthrough() {
        assert_eq!(right_click_pass(""), 0);
        assert_eq!(right_click_pass("[ui]\nright_click_passthrough_modifier = \"\"\n"), 0);
        assert_eq!(right_click_pass("[ui]\nright_click_passthrough_modifier = \"ctrl+alt\"\n"), 0x0c00_0000);
        assert_eq!(right_click_pass("[ui]\nright_click_passthrough_modifier = \"shift\"\n"), 0);
        assert_eq!(right_click_pass("[keys]\nright_click_passthrough_modifier = \"alt\"\n"), 0);
    }

    #[test]
    fn layout_as_a_tree() {
        // As herdr 0.9.5 gives it: p1 | (p2 / (p3 | p4))
        let layout: Value = serde_json::from_str(r#"{"area":{"x":0,"y":0,"width":120,"height":40},"zoomed":false,"focused_pane_id":"p3",
            "panes":[{"pane_id":"p1","rect":{"x":0,"y":0,"width":60,"height":40}},{"pane_id":"p2","rect":{"x":60,"y":0,"width":60,"height":20}},
                     {"pane_id":"p3","rect":{"x":60,"y":20,"width":30,"height":20}},{"pane_id":"p4","rect":{"x":90,"y":20,"width":30,"height":20}}],
            "splits":[{"id":"split_0_root","direction":"right","ratio":0.5,"rect":{}},{"id":"split_1_1","direction":"down","ratio":0.5,"rect":{}},
                      {"id":"split_2_11","direction":"right","ratio":0.5,"rect":{}}]}"#).unwrap();
        let items: Vec<Value> = ["p1", "p2", "p3", "p4"].iter().map(|p| json!({"id": p, "terminal": format!("t{p}")})).collect();
        let t = layout_tree(&layout, &items);
        assert_eq!(t["dir"], "right");
        assert_eq!(t["path"], json!([]));
        assert_eq!(t["a"]["id"], "p1");
        assert_eq!(t["b"]["dir"], "down");
        assert_eq!(t["b"]["path"], json!([true]));
        assert_eq!(t["b"]["a"]["id"], "p2");
        assert_eq!(t["b"]["b"]["path"], json!([true, true]));
        assert_eq!(t["b"]["b"]["a"]["terminal"], "tp3");
        assert_eq!(t["b"]["b"]["b"]["id"], "p4");
        // One pane, no split
        let one: Value = serde_json::from_str(r#"{"area":{"x":0,"y":0,"width":80,"height":24},"panes":[{"pane_id":"p1","rect":{"x":0,"y":0,"width":80,"height":24}}],"splits":[]}"#).unwrap();
        assert_eq!(layout_tree(&one, &items[..1])["id"], "p1");
    }

    #[test]
    fn worktrees_stay_with_their_repository() {
        let snap: Value = serde_json::from_str(r#"{"agents":[],"panes":[],"tabs":[],"layouts":[],"focused_workspace_id":"w1","workspaces":[
            {"workspace_id":"w1","number":1,"label":"api","worktree":{"repo_key":"r","is_linked_worktree":false}},
            {"workspace_id":"w2","number":2,"label":"web"},
            {"workspace_id":"w3","number":3,"label":"api-fix","worktree":{"repo_key":"r","is_linked_worktree":true}}]}"#).unwrap();
        let (state, _, _) = state_of(&snap, None);
        let s: Value = serde_json::from_str(&state).unwrap();
        let items = s["sections"][0]["items"].as_array().unwrap();
        let ids: Vec<&str> = items.iter().map(|i| i["id"].as_str().unwrap()).collect();
        assert_eq!(ids, ["w1", "w3", "w2"]);
        assert_eq!(items[1]["indent"], 1);
        assert_eq!(items[0]["group"], true);
        assert_eq!(items[2]["group"], false);
    }

    #[test]
    fn state_from_snapshot() {
        let snap: Value = serde_json::from_str(SNAPSHOT).unwrap();
        let (state, statuses, notices) = state_of(&snap, None);
        let s: Value = serde_json::from_str(&state).unwrap();
        // Tabs of the focused workspace only, by number
        assert_eq!(s["tabs"].as_array().unwrap().len(), 2);
        assert_eq!(s["tabs"][0]["id"], "w1:t1");
        assert_eq!(s["tabs"][0]["active"], true);
        assert_eq!(s["tabs"][1]["label"], "logs");
        // A tab shows its focused pane; its panes in the order of its layout
        assert_eq!(s["tabs"][0]["terminal"], "term_c");
        assert_eq!(s["tabs"][0]["cwd"], "/c");
        assert_eq!(s["tabs"][0]["panes"].as_array().unwrap().len(), 2);
        assert_eq!(s["tabs"][0]["panes"][1]["label"], "logs");
        assert_eq!(s["tabs"][0]["panes"][1]["focused"], true);
        // Without a layout: the panes of the tab, the first one; an agent's pane is named after it
        assert_eq!(s["tabs"][1]["terminal"], "term_b");
        assert_eq!(s["tabs"][1]["panes"][0]["label"], "claude");
        // Workspaces by number, the focused one active
        assert_eq!(s["sections"][0]["items"][0]["label"], "demo");
        assert_eq!(s["sections"][0]["items"][0]["detail"], "2");
        assert_eq!(s["sections"][0]["items"][0]["active"], true);
        assert_eq!(s["sections"][1]["items"][0]["label"], "claude");
        assert_eq!(s["sections"][1]["items"][0]["detail"], "demo");
        assert!(notices.is_empty());
        assert_eq!(statuses["w1:p2"], "working");

        // The agent stops and waits for the user: one notice; the same state again: none
        let blocked = SNAPSHOT.replace(r#""agent_status":"working","pane_id""#, r#""agent_status":"blocked","pane_id""#);
        let snap: Value = serde_json::from_str(&blocked).unwrap();
        let (_, statuses, notices) = state_of(&snap, Some(&statuses));
        assert_eq!(notices.len(), 1);
        assert!(notices[0].contains("\"blocked\"") && notices[0].contains("claude"));
        let (_, _, notices) = state_of(&snap, Some(&statuses));
        assert!(notices.is_empty());
    }
}
