//! Integrated modes: a program (herdr today; nvim and tmux next) drives part of a window of Aéroport. Opt-in, only when
//! the user calls the program: `fili-term mode herdr`, or the footer's offer when it runs in a tab (the daemon tells the
//! window the foreground process of each terminal). Stopping a mode never stops the program.
//!
//! An adapter runs in a thread of the window process while its mode is on (nothing runs otherwise). It talks to the
//! program (socket, RPC), publishes the state the window draws as JSON, and takes the window's actions:
//!
//! ```json
//! {"kind": "herdr", "title": "herdr", "connected": true, "message": "",
//!  "tabs": [{"id": "w1:t1", "label": "1", "active": true, "status": "working", "terminal": "term_…", "cwd": "/…",
//!            "panes": [{"id": "w1:p1", "terminal": "term_…", "label": "zsh", "status": "idle", "focused": true}]}],
//!  "addTab": true,
//!  "sections": [{"key": "workspaces", "title": "Workspaces", "add": true,
//!                "items": [{"id": "w1", "label": "api", "detail": "2", "status": "idle", "active": true}]}]}
//! ```
//!
//! herdr: each of its tabs is a tab of Aéroport attached to the terminal of a pane (`herdr terminal attach TERMINAL`),
//! without herdr's own interface. Actions (JSON from the window): `{"op": "tab", "id"}`, `{"op": "close-tab", "id"}`,
//! `{"op": "new-tab"}`,
//! `{"op": "select", "section", "id"}`, `{"op": "new", "section"}`. Notices (a program asks for attention) go to the
//! window, which shows a notification when nobody watches the terminal: `{"kind": "blocked" | "done", "agent", "place"}`.

pub mod herdr;

use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};

/// The programs with a mode
pub const KINDS: &[&str] = &["herdr"];

pub enum In {
    /// An action of the window (JSON)
    Action(serde_json::Value),
    /// Something changed in the program (adapter's own wake-up), with the generation of the connection that saw it
    Changed(u32),
    /// The program's event connection ended (generation)
    Lost(u32),
    Stop,
}

/// What an adapter sends out: the state to draw, or a notice
pub enum Out {
    State(String),
    Notice(String),
}

pub struct Ctx {
    /// Where the program listens (herdr: its socket)
    pub socket: PathBuf,
    pub out: Box<dyn Fn(Out) + Send + Sync>,
    /// For the adapter's own threads (events of the program)
    pub tx: Sender<In>,
}

impl Ctx {
    pub fn publish(&self, state: String) {
        (self.out)(Out::State(state));
    }
    pub fn notice(&self, notice: String) {
        (self.out)(Out::Notice(notice));
    }
}

/// A running mode, held by its window
pub struct Mode {
    tx: Sender<In>,
}

impl Mode {
    pub fn start(kind: &str, socket: PathBuf, out: Box<dyn Fn(Out) + Send + Sync>) -> Option<Mode> {
        let kind = KINDS.iter().copied().find(|k| *k == kind)?;
        let (tx, rx) = mpsc::channel();
        let ctx = Ctx { socket, out, tx: tx.clone() };
        let run: fn(Ctx, Receiver<In>) = match kind {
            "herdr" => herdr::run,
            _ => return None,
        };
        std::thread::Builder::new().name(format!("mode-{kind}")).spawn(move || run(ctx, rx)).ok()?;
        Some(Mode { tx })
    }

    pub fn action(&self, action: serde_json::Value) {
        let _ = self.tx.send(In::Action(action));
    }
}

impl Drop for Mode {
    fn drop(&mut self) {
        let _ = self.tx.send(In::Stop);
    }
}

/// JSON string literal
pub fn quote(s: &str) -> String {
    serde_json::Value::String(s.to_string()).to_string()
}
