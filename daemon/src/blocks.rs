//! Command blocks: the shell marks its prompts and commands (OSC 133, shell/fili-term.zsh and .bash), the daemon
//! records each command as a block (command line, working directory, exit status, duration).
//!
//! alacritty_terminal ignores OSC 133, so the pty loop (pty_loop.rs) scans the bytes for it before parsing them and
//! applies each mark at its exact place in the stream, with the screen as the shell left it.
//!
//! Where a block starts is written in the grid itself: the first cell of the block's first line (the prompt) carries
//! a flag bit alacritty does not use (`MARK`) and the block id in its underline colour (drawn only for underlined
//! text, never sent to the window). The mark moves with its line: history, reflow on resize, clear of the screen or
//! of the history; positions kept on the side would drift. The table holds the rest, by id.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use alacritty_terminal::event::EventListener;
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line};
use alacritty_terminal::term::cell::{Cell, Flags};
use alacritty_terminal::term::{Term, TermMode};
use alacritty_terminal::vte::ansi::{Color, Rgb};

/// Flag bit of the first cell of a block (the last bit of alacritty's cell flags is free).
const MARK: Flags = Flags::from_bits_retain(0x8000);
/// Blocks kept in the table (their marks scroll out of the history long before).
const KEEP: usize = 2000;
/// How far up a mark of the current prompt is looked for (prompt and command lines).
const NEAR: i32 = 2000;

/// A mark the shell put in its output.
#[derive(Debug, PartialEq)]
pub enum Mark {
    /// OSC 133;A: the prompt starts.
    Prompt,
    /// OSC 133;B: the prompt ends, the command line starts.
    Input,
    /// OSC 133;C: the command runs, its output starts (`cmdline_url=` gives the command line).
    Output(Option<String>),
    /// OSC 133;D: the command ended (exit status).
    Done(Option<i32>),
    /// OSC 7: working directory.
    Cwd(String),
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum State {
    /// The prompt is shown, the user types the command.
    Prompt = 0,
    Running = 1,
    Done = 2,
}

#[derive(Clone, Debug)]
pub struct Block {
    pub id: u32,
    pub state: State,
    pub exit: Option<i32>,
    pub started: Option<Instant>,
    pub duration: Option<Duration>,
    pub command: String,
    pub cwd: String,
    /// Logical lines (wrapped lines count once) of prompt and command: the output starts after them.
    pub head_lines: u16,
    /// Table version of the last change (the window gets what changed since it last looked).
    pub version: u64,
}

#[derive(Default)]
pub struct Blocks {
    list: VecDeque<Block>,
    next_id: u32,
    /// Absolute line (history + screen) of the prompt start, until the prompt is drawn and marked.
    prompt_line: Option<i64>,
    /// Absolute line and column where the command line starts (OSC 133;B).
    input_at: Option<(i64, usize)>,
    pub version: u64,
    pub cwd: String,
    pub cwd_version: u64,
}

/// A block on the screen of a frame, in rows of the view (the start may be above it).
#[derive(Debug, PartialEq)]
pub struct Visible {
    pub id: u32,
    pub start: i32,
    /// First row of the output, `i32::MIN` when the block has none yet.
    pub output: i32,
    pub end: i32,
}

fn abs<T: EventListener>(term: &Term<T>, line: Line) -> i64 {
    term.grid().history_size() as i64 + line.0 as i64
}

fn line_of<T: EventListener>(term: &Term<T>, abs: i64) -> Line {
    let top = -(term.grid().history_size() as i64);
    let bottom = term.screen_lines() as i64 - 1;
    Line((abs - term.grid().history_size() as i64).clamp(top, bottom) as i32)
}

fn id_color(id: u32) -> Color {
    Color::Spec(Rgb { r: (id >> 16) as u8, g: (id >> 8) as u8, b: id as u8 })
}

/// Block id of a marked cell.
fn mark_of(cell: &Cell) -> Option<u32> {
    if !cell.flags.contains(MARK) {
        return None;
    }
    match cell.underline_color() {
        Some(Color::Spec(c)) => Some((c.r as u32) << 16 | (c.g as u32) << 8 | c.b as u32),
        _ => None,
    }
}

fn mark_at<T: EventListener>(term: &Term<T>, line: Line) -> Option<u32> {
    mark_of(&term.grid()[line][Column(0)])
}

fn set_mark<T: EventListener>(term: &mut Term<T>, line: Line, id: Option<u32>) {
    let cell = &mut term.grid_mut()[line][Column(0)];
    match id {
        Some(id) => {
            cell.flags.insert(MARK);
            cell.set_underline_color(Some(id_color(id)));
        }
        None => {
            cell.flags.remove(MARK);
            cell.set_underline_color(None);
        }
    }
}

fn wraps<T: EventListener>(term: &Term<T>, line: Line) -> bool {
    let cols = term.columns();
    term.grid()[line][Column(cols - 1)].flags.contains(Flags::WRAPLINE)
}

fn topmost<T: EventListener>(term: &Term<T>) -> i32 {
    -(term.grid().history_size() as i32)
}

/// Nearest marked line at or above `from` (down to `limit`): line and block id.
fn mark_above<T: EventListener>(term: &Term<T>, from: i32, limit: i32) -> Option<(i32, u32)> {
    let mut l = from.min(term.screen_lines() as i32 - 1);
    while l >= limit.max(topmost(term)) {
        if let Some(id) = mark_at(term, Line(l)) {
            return Some((l, id));
        }
        l -= 1;
    }
    None
}

/// Line where the output of a block that starts at `start` begins: after `head` logical lines.
fn output_line<T: EventListener>(term: &Term<T>, start: i32, head: u16) -> i32 {
    let bottom = term.screen_lines() as i32 - 1;
    let mut l = start;
    let mut left = head.max(1);
    while l <= bottom {
        let w = wraps(term, Line(l));
        l += 1;
        if !w {
            left -= 1;
            if left == 0 {
                break;
            }
        }
    }
    l
}

/// Text of the lines `from..=to` (grid lines), wrapped lines joined, trailing blanks and blank lines dropped.
pub fn text<T: EventListener>(term: &Term<T>, from: i32, to: i32) -> String {
    let cols = term.columns();
    let mut out = String::new();
    let mut l = from.max(topmost(term));
    let to = to.min(term.screen_lines() as i32 - 1);
    while l <= to {
        let row = &term.grid()[Line(l)];
        let mut s = String::new();
        for col in 0..cols {
            let cell = &row[Column(col)];
            if cell.flags.intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER) {
                continue;
            }
            s.push(if cell.c == '\0' { ' ' } else { cell.c });
            if let Some(z) = cell.zerowidth() {
                s.extend(z);
            }
        }
        if wraps(term, Line(l)) && l != to {
            out.push_str(&s);
        } else {
            out.push_str(s.trim_end_matches(' '));
            out.push('\n');
        }
        l += 1;
    }
    let trimmed = out.trim_end_matches(['\n', ' ']).len();
    out.truncate(trimmed);
    out
}

impl Blocks {
    pub fn get(&self, id: u32) -> Option<&Block> {
        self.list.iter().rev().find(|b| b.id == id)
    }

    /// Latest block that ran a command.
    pub fn last_command(&self) -> Option<&Block> {
        self.list.iter().rev().find(|b| b.state != State::Prompt)
    }

    /// The blocks that ran a command (newest last, at most 200), as JSON for `fili-term blocks`.
    pub fn json(&self) -> String {
        let all: Vec<&Block> = self.list.iter().filter(|b| b.state != State::Prompt).collect();
        let items: Vec<String> = all[all.len().saturating_sub(200)..]
            .iter()
            .map(|b| {
                format!(
                    "{{\"id\":{},\"command\":{},\"cwd\":{},\"state\":\"{}\",\"exit\":{},\"duration_ms\":{}}}",
                    b.id,
                    json_string(&b.command),
                    json_string(&b.cwd),
                    if b.state == State::Running { "running" } else { "done" },
                    b.exit.map_or("null".into(), |e| e.to_string()),
                    b.duration.map_or("null".into(), |d| d.as_millis().to_string()),
                )
            })
            .collect();
        format!("[{}]", items.join(","))
    }

    /// Blocks changed since `version`.
    pub fn changed_since(&self, version: u64) -> impl Iterator<Item = &Block> {
        self.list.iter().filter(move |b| b.version > version)
    }

    fn touch(&mut self) -> u64 {
        self.version += 1;
        self.version
    }

    fn current_mut(&mut self, state: State) -> Option<&mut Block> {
        self.list.back_mut().filter(|b| b.state == state)
    }

    pub fn apply<T: EventListener>(&mut self, term: &mut Term<T>, mark: Mark) {
        if let Mark::Cwd(dir) = mark {
            if dir == self.cwd {
                return;
            }
            self.cwd = dir;
            self.cwd_version = self.touch();
            return;
        }
        // Full-screen programs draw on the other screen, which has no history: nothing to mark there
        if term.mode().contains(TermMode::ALT_SCREEN) {
            return;
        }
        let cursor = term.grid().cursor.point;
        match mark {
            Mark::Prompt => {
                // A prompt drawn again (resize, Ctrl+C, empty line) stays the same block
                if self.current_mut(State::Prompt).is_none() {
                    let id = self.next_id.max(1);
                    self.next_id = id + 1;
                    let version = self.touch();
                    let cwd = self.cwd.clone();
                    self.list.push_back(Block {
                        id,
                        state: State::Prompt,
                        exit: None,
                        started: None,
                        duration: None,
                        command: String::new(),
                        cwd,
                        head_lines: 1,
                        version,
                    });
                    while self.list.len() > KEEP {
                        self.list.pop_front();
                    }
                }
                self.prompt_line = Some(abs(term, cursor.line));
            }
            Mark::Input => {
                let Some(id) = self.current_mut(State::Prompt).map(|b| b.id) else { return };
                let start = self.prompt_line.map(|a| line_of(term, a)).unwrap_or(cursor.line).min(cursor.line);
                // One mark per block: the one of an earlier drawing of the prompt goes
                let mut l = cursor.line.0;
                while l >= (cursor.line.0 - NEAR).max(topmost(term)) {
                    if l != start.0 && mark_at(term, Line(l)) == Some(id) {
                        set_mark(term, Line(l), None);
                    }
                    l -= 1;
                }
                set_mark(term, start, Some(id));
                self.input_at = Some((abs(term, cursor.line), cursor.column.0));
                let version = self.touch();
                if let Some(b) = self.current_mut(State::Prompt) {
                    b.version = version;
                }
            }
            Mark::Output(cmdline) => {
                let Some(id) = self.current_mut(State::Prompt).map(|b| b.id) else { return };
                // Lines of prompt and command: from the mark to the cursor (at the start of the first output line)
                let end = if cursor.column.0 == 0 { cursor.line.0 - 1 } else { cursor.line.0 };
                let start = mark_above(term, end, end - NEAR).filter(|m| m.1 == id).map(|m| m.0);
                let mut head = 0u16;
                if let Some(start) = start {
                    for l in start..=end {
                        if !wraps(term, Line(l)) || l == end {
                            head = head.saturating_add(1);
                        }
                    }
                }
                let command = match cmdline {
                    Some(c) => c,
                    None => match self.input_at {
                        Some((a, col)) => {
                            let from = line_of(term, a).0;
                            let t = text(term, from, end);
                            t.chars().skip(col).collect::<String>().trim().to_string()
                        }
                        None => String::new(),
                    },
                };
                let version = self.touch();
                let cwd = self.cwd.clone();
                if let Some(b) = self.current_mut(State::Prompt) {
                    b.state = State::Running;
                    b.started = Some(Instant::now());
                    b.command = command;
                    if !cwd.is_empty() {
                        b.cwd = cwd;
                    }
                    b.head_lines = head.max(1);
                    b.version = version;
                }
                self.prompt_line = None;
                self.input_at = None;
            }
            Mark::Done(exit) => {
                let version = self.touch();
                if let Some(b) = self.current_mut(State::Running) {
                    b.state = State::Done;
                    b.exit = exit;
                    b.duration = b.started.map(|s| s.elapsed());
                    b.version = version;
                }
            }
            Mark::Cwd(_) => unreachable!(),
        }
    }

    /// The blocks a view shows (`offset`: lines scrolled back into the history), from top to bottom.
    pub fn visible<T: EventListener>(&self, term: &Term<T>, offset: i32) -> Vec<Visible> {
        if term.mode().contains(TermMode::ALT_SCREEN) {
            return Vec::new();
        }
        let rows = term.screen_lines() as i32;
        let top = -offset;
        let mut starts: Vec<(i32, u32)> = Vec::new();
        // The block that holds the first row starts on it or above
        if let Some(m) = mark_above(term, top, topmost(term)) {
            starts.push(m);
        }
        for l in top + 1..top + rows {
            if let Some(id) = mark_at(term, Line(l)) {
                starts.push((l, id));
            }
        }
        let cursor = term.grid().cursor.point.line.0;
        let mut out = Vec::with_capacity(starts.len());
        for (i, &(line, id)) in starts.iter().enumerate() {
            let next = starts.get(i + 1).map(|s| s.0 - 1);
            let end = next.unwrap_or(cursor);
            let output = match self.get(id) {
                Some(b) if b.state != State::Prompt => {
                    let o = output_line(term, line, b.head_lines);
                    if o <= end || b.state == State::Running { o + offset } else { i32::MIN }
                }
                _ => i32::MIN,
            };
            out.push(Visible { id, start: line + offset, output, end: end + offset });
        }
        out
    }

    /// Grid lines of a block: start, first output line, last line.
    pub fn lines_of<T: EventListener>(&self, term: &Term<T>, id: u32) -> Option<(i32, i32, i32)> {
        let b = self.get(id)?;
        let cursor = term.grid().cursor.point;
        let bottom = term.screen_lines() as i32 - 1;
        let mut l = bottom;
        let mut next: Option<i32> = None;
        while l >= topmost(term) {
            match mark_at(term, Line(l)) {
                Some(m) if m == id => {
                    let end = match next {
                        Some(n) => n - 1,
                        None if cursor.column.0 == 0 => cursor.line.0 - 1,
                        None => cursor.line.0,
                    };
                    return Some((l, output_line(term, l, b.head_lines), end));
                }
                Some(_) => next = Some(l),
                None => {}
            }
            l -= 1;
        }
        None
    }

    /// First line of the block before (`dir` < 0) or after the one that starts at or holds `line`.
    pub fn neighbour<T: EventListener>(term: &Term<T>, line: i32, dir: i32) -> Option<i32> {
        if dir < 0 {
            mark_above(term, line - 1, topmost(term)).map(|m| m.0)
        } else {
            (line + 1..term.screen_lines() as i32).find(|&l| mark_at(term, Line(l)).is_some())
        }
    }
}

/// Finds the shell's marks in the pty output. Keeps its state between reads: a mark may be cut in two.
#[derive(Default)]
pub struct Scanner {
    state: Scan,
    body: Vec<u8>,
}

#[derive(Default, Clone, Copy, PartialEq)]
enum Scan {
    #[default]
    Ground,
    Esc,
    /// Inside `ESC ]`: the body until BEL or `ESC \`.
    Osc,
    OscEsc,
    /// An OSC of no interest, or too long: skipped to its end.
    Skip,
    SkipEsc,
}

/// Longest OSC kept (a long command line in `cmdline_url`).
const MAX_OSC: usize = 64 * 1024;

impl Scanner {
    /// Scans `bytes`; `found(end, mark)` gets each mark with the offset just after it.
    pub fn feed(&mut self, bytes: &[u8], mut found: impl FnMut(usize, Mark)) {
        for (i, &b) in bytes.iter().enumerate() {
            self.state = match (self.state, b) {
                (Scan::Ground, 0x1b) => Scan::Esc,
                (Scan::Ground, _) => Scan::Ground,
                (Scan::Esc, b']') => {
                    self.body.clear();
                    Scan::Osc
                }
                (Scan::Esc, 0x1b) => Scan::Esc,
                (Scan::Esc, _) => Scan::Ground,
                (Scan::Osc, 0x07) | (Scan::OscEsc, b'\\') => {
                    if let Some(m) = parse(&self.body) {
                        found(i + 1, m);
                    }
                    Scan::Ground
                }
                (Scan::Osc, 0x1b) => Scan::OscEsc,
                (Scan::Osc, 0x18 | 0x1a) => Scan::Ground,
                (Scan::Osc, _) => {
                    // Only OSC 7 and 133 are kept whole
                    self.body.push(b);
                    let n = self.body.len();
                    let body = &self.body;
                    let wanted = |p: &[u8]| if n <= p.len() { p.starts_with(body) } else { body.starts_with(p) };
                    if n <= MAX_OSC && (wanted(b"133;") || wanted(b"7;")) { Scan::Osc } else { Scan::Skip }
                }
                // ESC then something else inside an OSC: the OSC is cut short, the ESC starts a new sequence
                (Scan::OscEsc, b']') => {
                    self.body.clear();
                    Scan::Osc
                }
                (Scan::OscEsc | Scan::SkipEsc, 0x1b) => Scan::Esc,
                (Scan::OscEsc | Scan::SkipEsc, _) => Scan::Ground,
                (Scan::Skip, 0x07 | 0x18 | 0x1a) => Scan::Ground,
                (Scan::Skip, 0x1b) => Scan::SkipEsc,
                (Scan::Skip, _) => Scan::Skip,
            };
        }
    }
}

fn parse(body: &[u8]) -> Option<Mark> {
    let text = std::str::from_utf8(body).ok()?;
    if let Some(url) = text.strip_prefix("7;") {
        // file://host/path
        let rest = url.strip_prefix("file://")?;
        let path = &rest[rest.find('/')?..];
        return Some(Mark::Cwd(unescape(path)));
    }
    let mut parts = text.strip_prefix("133;")?.split(';');
    Some(match parts.next()? {
        "A" => Mark::Prompt,
        "B" => Mark::Input,
        "C" => Mark::Output(parts.find_map(|p| p.strip_prefix("cmdline_url=")).map(unescape)),
        "D" => Mark::Done(parts.next().and_then(|s| s.parse().ok())),
        _ => return None,
    })
}

fn json_string(s: &str) -> String {
    let mut o = String::with_capacity(s.len() + 2);
    o.push('"');
    for c in s.chars() {
        match c {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            c if (c as u32) < 0x20 => o.push_str(&format!("\\u{:04x}", c as u32)),
            c => o.push(c),
        }
    }
    o.push('"');
    o
}

/// Percent-decoding (invalid UTF-8 replaced).
fn unescape(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() && b[i + 1].is_ascii_hexdigit() && b[i + 2].is_ascii_hexdigit() {
            let hex = |c: u8| (c as char).to_digit(16).unwrap_or(0) as u8;
            out.push(hex(b[i + 1]) << 4 | hex(b[i + 2]));
            i += 3;
            continue;
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn marks(chunks: &[&[u8]]) -> Vec<Mark> {
        let mut s = Scanner::default();
        let mut out = Vec::new();
        for c in chunks {
            s.feed(c, |_, m| out.push(m));
        }
        out
    }

    #[test]
    fn marks_are_found_even_when_cut() {
        let m = marks(&[b"x\x1b]133;A\x07$ \x1b]13", b"3;B\x1b\\ls\r\n\x1b]133;C;cmdline_url=ls%20-l\x07", b"\x1b]133;D;2\x07"]);
        assert_eq!(m, vec![Mark::Prompt, Mark::Input, Mark::Output(Some("ls -l".into())), Mark::Done(Some(2))]);
    }

    #[test]
    fn other_sequences_are_ignored() {
        let m = marks(&[b"\x1b]0;title\x07\x1b]8;;http://a\x1b\\link\x1b]8;;\x1b\\\x1b[1m\x1b]7;file://host/home/a%20b\x07"]);
        assert_eq!(m, vec![Mark::Cwd("/home/a b".into())]);
    }

    #[test]
    fn offsets_point_after_the_mark() {
        let mut s = Scanner::default();
        let mut ends = Vec::new();
        s.feed(b"ab\x1b]133;A\x07cd", |e, _| ends.push(e));
        assert_eq!(ends, vec![10]);
    }

    #[test]
    fn utf8_is_decoded() {
        assert_eq!(unescape("caf%C3%A9%20%"), "café %");
    }
}
