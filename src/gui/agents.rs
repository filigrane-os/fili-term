//! AI agents in panes, without hooks: which agent runs in a terminal (the process in its foreground) and what it does
//! (`working`, `blocked` waiting for you, `done` finished while nobody watched, `idle`). The state comes from what the
//! agent shows, as herdr reads it: the window title it sets (Claude Code draws a spinner there while it works), the
//! bottom of the screen ("esc to interrupt", "Do you want to proceed?"), and output that keeps coming. A state reported
//! by the agent's hooks (`fili-term report`) wins for a while.
//!
//! The programs come from `[agents] programs = [...]` in fili-term.toml, else the list below.
use std::{
    sync::RwLock,
    time::{Duration, Instant},
};

use super::theme::{Val, parse_sections};

pub const DEFAULT_PROGRAMS: &[&str] = &[
    "claude", "codex", "opencode", "gemini", "aider", "goose", "amp", "cursor-agent", "copilot", "crush", "qwen", "kimi",
    "droid", "pi",
];
/// Interpreters an agent may run in (the agent is then a word of their command line)
const RUNTIMES: &[&str] = &["node", "nodejs", "bun", "deno", "python", "python3", "uv", "uvx", "npx", "pnpm"];

/// Output that went on this long makes a known agent `working` when its screen says nothing
const ACTIVE: Duration = Duration::from_secs(2);
/// Output right after a key, a paste or a resize is the echo of it, not the agent working
const ECHO: Duration = Duration::from_millis(700);
/// How long a state reported by a hook holds without news
const REPORTED: Duration = Duration::from_secs(600);

static PROGRAMS: RwLock<Vec<String>> = RwLock::new(Vec::new());

/// `[agents] programs` of the settings file (the default list when absent)
pub fn programs_from(text: &str) -> Vec<String> {
    match parse_sections(text).get("agents").and_then(|s| s.get("programs")) {
        Some(Val::List(l)) => l.iter().map(|p| p.trim().to_string()).filter(|p| !p.is_empty()).collect(),
        Some(Val::Str(s)) => s.split([',', ' ']).map(str::trim).filter(|p| !p.is_empty()).map(str::to_string).collect(),
        None => DEFAULT_PROGRAMS.iter().map(|p| p.to_string()).collect(),
    }
}

/// Read the settings again (the window calls this when the file changes)
pub fn reload(text: &str) {
    *PROGRAMS.write().unwrap() = programs_from(text);
}

fn programs() -> Vec<String> {
    let known = PROGRAMS.read().unwrap();
    if known.is_empty() { DEFAULT_PROGRAMS.iter().map(|p| p.to_string()).collect() } else { known.clone() }
}

/// The name of a path or command word without its folder and extension ("/usr/lib/node_modules/x/codex.js" → "codex")
fn stem(word: &str) -> &str {
    let name = word.rsplit('/').next().unwrap_or(word);
    match name.rsplit_once('.') {
        Some((s, ext)) if ["js", "mjs", "cjs", "ts", "py", "exe"].contains(&ext) && !s.is_empty() => s,
        _ => name,
    }
}

/// The agent a foreground process is: its name, or for an interpreter a word of its command line
pub fn agent_of(name: &str, argv: &[String], programs: &[String]) -> Option<String> {
    let known = |w: &str| programs.iter().find(|p| p.as_str() == w).cloned();
    if let Some(a) = known(stem(name)) {
        return Some(a);
    }
    if !RUNTIMES.contains(&name) && !name.starts_with("python") {
        return None;
    }
    // node /usr/lib/node_modules/@openai/codex/bin/codex.js: the script, or a package folder on its path
    for word in argv.iter().skip(1).filter(|w| !w.starts_with('-')) {
        for part in word.split('/').rev() {
            if let Some(a) = known(stem(part)) {
                return Some(a);
            }
        }
    }
    None
}

/// The agent in the foreground of a terminal (reads /proc/PID/cmdline for interpreters)
pub fn detect(pid: u32, name: &str) -> Option<String> {
    let programs = programs();
    if pid == 0 || name.is_empty() {
        return None;
    }
    let argv: Vec<String> = std::fs::read(format!("/proc/{pid}/cmdline"))
        .map(|b| b.split(|c| *c == 0).filter(|w| !w.is_empty()).map(|w| String::from_utf8_lossy(w).into_owned()).collect())
        .unwrap_or_default();
    agent_of(name, &argv, &programs)
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum State {
    #[default]
    Idle,
    Working,
    Blocked,
    Done,
}

impl State {
    pub fn as_str(self) -> &'static str {
        match self {
            State::Idle => "idle",
            State::Working => "working",
            State::Blocked => "blocked",
            State::Done => "done",
        }
    }
    pub fn parse(s: &str) -> Option<State> {
        match s.trim().to_ascii_lowercase().as_str() {
            "idle" => Some(State::Idle),
            "working" | "busy" | "running" => Some(State::Working),
            "blocked" | "waiting" | "waiting_for_input" | "attention" => Some(State::Blocked),
            "done" | "finished" | "stopped" => Some(State::Done),
            _ => None,
        }
    }
}

/// Phrases of a question that waits for an answer (permission, confirmation), at the bottom of the screen
const BLOCKERS: &[&[&str]] = &[
    &["do you want to proceed?"],
    &["do you want to", "yes"],
    &["would you like to", "yes"],
    &["waiting for permission"],
    &["waiting for approval"],
    &["waiting for user confirmation"],
    &["allow command?"],
    &["permission required"],
    &["approve once"],
    &["approve for this session"],
    &["press enter to confirm or esc to cancel"],
    &["enter to submit answer"],
    &["run this command?"],
    &["[y/n]"],
    &["(y/n)"],
    &["yes (y)"],
];
/// Hints an agent shows only while it works
const WORKING: &[&str] = &["esc to interrupt", "ctrl+c to interrupt", "to interrupt)"];

/// What the screen says: `title` the window title the program set, `bottom` its last non-empty lines. None when it
/// says nothing (then the output decides).
pub fn screen_state(title: &str, bottom: &str) -> Option<State> {
    let text = bottom.to_lowercase();
    let title_l = title.to_lowercase();
    if title_l.contains("action required") || BLOCKERS.iter().any(|all| all.iter().all(|p| text.contains(p))) {
        return Some(State::Blocked);
    }
    let mut chars = title.chars();
    let first = chars.next();
    let spaced = chars.next() == Some(' ');
    // Claude Code: a braille spinner (or half circles) while it works, ✳ at its prompt
    if spaced && first.is_some_and(|c| ('\u{2800}'..='\u{28ff}').contains(&c) || ('\u{25d0}'..='\u{25d3}').contains(&c)) {
        return Some(State::Working);
    }
    if WORKING.iter().any(|p| text.contains(p)) {
        return Some(State::Working);
    }
    if spaced && first == Some('\u{2733}') {
        return Some(State::Idle);
    }
    None
}

/// The state of the agent of one terminal over time
#[derive(Default)]
pub struct Tracker {
    state: State,
    last_output: Option<Instant>,
    /// Since when the output runs (reset by a quiet second)
    output_since: Option<Instant>,
    last_input: Option<Instant>,
    last_bell: Option<Instant>,
    reported: Option<(State, Instant)>,
}

impl Tracker {
    /// The program wrote to the terminal
    pub fn output(&mut self, now: Instant) {
        if self.last_input.is_some_and(|t| now.duration_since(t) < ECHO) {
            return;
        }
        if self.last_output.is_none_or(|t| now.duration_since(t) > Duration::from_secs(1)) {
            self.output_since = Some(now);
        }
        self.last_output = Some(now);
    }
    /// The user typed, pasted, resized or scrolled
    pub fn input(&mut self, now: Instant) {
        self.last_input = Some(now);
        // An answer ends what a hook said about waiting or being done
        if matches!(self.reported, Some((State::Blocked | State::Done, _))) {
            self.reported = None;
        }
    }
    pub fn bell(&mut self, now: Instant) {
        self.last_bell = Some(now);
    }
    pub fn report(&mut self, state: State, now: Instant) {
        self.reported = Some((state, now));
    }
    pub fn reset(&mut self) {
        *self = Tracker::default();
    }

    /// Once a second: the new state from what the screen says and whether someone looks at the terminal
    pub fn tick(&mut self, now: Instant, screen: Option<State>, watched: bool) -> State {
        let reported = self.reported.filter(|(_, at)| now.duration_since(*at) < REPORTED).map(|(s, _)| s);
        let base = if let Some(s) = reported {
            s
        } else if let Some(s) = screen {
            s
        } else {
            let output = self.last_output.is_some_and(|t| now.duration_since(t) < ACTIVE)
                && self.output_since.is_some_and(|t| self.last_output.is_some_and(|l| l.duration_since(t) >= Duration::from_secs(1)));
            let bell = self.last_bell.is_some_and(|b| self.last_output.is_none_or(|o| b >= o) && self.last_input.is_none_or(|i| b > i));
            if bell {
                State::Blocked
            } else if output {
                State::Working
            } else {
                State::Idle
            }
        };
        let was = self.state;
        self.state = match base {
            // Finished while nobody looked: "done" until someone does
            State::Idle if !watched && (was == State::Working || was == State::Done) => State::Done,
            State::Done if watched => State::Idle,
            s => s,
        };
        self.state
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(words: &[&str]) -> Vec<String> {
        words.iter().map(|w| w.to_string()).collect()
    }

    #[test]
    fn agents_by_name_or_in_an_interpreter() {
        let p = v(DEFAULT_PROGRAMS);
        assert_eq!(agent_of("claude", &v(&["claude"]), &p).as_deref(), Some("claude"));
        assert_eq!(agent_of("node", &v(&["node", "/usr/lib/node_modules/@openai/codex/bin/codex.js", "--yolo"]), &p).as_deref(), Some("codex"));
        assert_eq!(agent_of("python3", &v(&["python3", "-m", "x", "/home/u/.local/bin/aider"]), &p).as_deref(), Some("aider"));
        assert_eq!(agent_of("node", &v(&["node", "server.js"]), &p), None);
        assert_eq!(agent_of("zsh", &v(&["zsh"]), &p), None);
        assert_eq!(agent_of("nvim", &v(&["nvim", "claude.md"]), &p), None);
        assert_eq!(agent_of("myagent", &[], &v(&["myagent"])).as_deref(), Some("myagent"));
    }

    #[test]
    fn programs_setting() {
        assert_eq!(programs_from(""), v(DEFAULT_PROGRAMS));
        assert_eq!(programs_from("[agents]\nprograms = [\"claude\", \"mine\"]\n"), v(&["claude", "mine"]));
    }

    #[test]
    fn what_screens_say() {
        // Claude Code
        assert_eq!(screen_state("⠐ Fixing the build", ""), Some(State::Working));
        assert_eq!(screen_state("✳ Fixing the build", "> \n? for shortcuts"), Some(State::Idle));
        let permission = "Bash command\n  cargo build\nDo you want to proceed?\n❯ 1. Yes\n  2. No\nEsc to cancel";
        assert_eq!(screen_state("✳ Claude Code", permission), Some(State::Blocked));
        assert_eq!(screen_state("Claude Code", "✻ Thinking… (12s · esc to interrupt)"), Some(State::Working));
        // Codex
        assert_eq!(screen_state("codex", "• Working (3s • esc to interrupt)"), Some(State::Working));
        assert_eq!(screen_state("codex", "Allow command?\n  ls -la\n› Yes   No"), Some(State::Blocked));
        // A shell or an agent saying nothing
        assert_eq!(screen_state("zsh", "$ ls\nREADME.md"), None);
    }

    #[test]
    fn output_makes_working_then_done_when_unwatched() {
        let t0 = Instant::now();
        let s = |n: u64| t0 + Duration::from_millis(n);
        let mut t = Tracker::default();
        assert_eq!(t.tick(s(0), None, false), State::Idle);
        for ms in (0..1500).step_by(100) {
            t.output(s(ms));
        }
        assert_eq!(t.tick(s(1500), None, false), State::Working);
        // Quiet: done, since nobody watched
        assert_eq!(t.tick(s(4000), None, false), State::Done);
        assert_eq!(t.tick(s(5000), None, false), State::Done);
        assert_eq!(t.tick(s(6000), None, true), State::Idle);
        // Watched while it ends: idle
        for ms in (7000..8500).step_by(100) {
            t.output(s(ms));
        }
        assert_eq!(t.tick(s(8500), None, true), State::Working);
        assert_eq!(t.tick(s(11000), None, true), State::Idle);
    }

    #[test]
    fn echo_of_typing_is_not_work() {
        let t0 = Instant::now();
        let s = |n: u64| t0 + Duration::from_millis(n);
        let mut t = Tracker::default();
        for ms in (0..3000).step_by(150) {
            t.input(s(ms));
            t.output(s(ms + 20));
        }
        assert_eq!(t.tick(s(3000), None, false), State::Idle);
    }

    #[test]
    fn bell_waits_until_an_answer() {
        let t0 = Instant::now();
        let s = |n: u64| t0 + Duration::from_millis(n);
        let mut t = Tracker::default();
        t.output(s(0));
        t.bell(s(10));
        assert_eq!(t.tick(s(1000), None, false), State::Blocked);
        t.input(s(2000));
        assert_eq!(t.tick(s(2100), None, true), State::Idle);
    }

    #[test]
    fn screen_wins_over_output_and_reports_over_both() {
        let t0 = Instant::now();
        let s = |n: u64| t0 + Duration::from_millis(n);
        let mut t = Tracker::default();
        for ms in (0..1500).step_by(100) {
            t.output(s(ms));
        }
        assert_eq!(t.tick(s(1500), Some(State::Blocked), false), State::Blocked);
        t.report(State::Working, s(1600));
        assert_eq!(t.tick(s(1700), Some(State::Idle), false), State::Working);
        t.report(State::Done, s(1800));
        assert_eq!(t.tick(s(1900), Some(State::Working), false), State::Done);
        // An answer drops a "done" or "blocked" report; the screen decides again
        t.input(s(2000));
        assert_eq!(t.tick(s(2100), Some(State::Working), true), State::Working);
        // A report gets old
        t.report(State::Blocked, s(3000));
        assert_eq!(t.tick(s(3000) + REPORTED + Duration::from_secs(1), None, true), State::Idle);
    }
}
