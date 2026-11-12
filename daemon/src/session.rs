//! Session daemon: owns the ptys and the terminal state (alacritty_terminal), outlives the windows.
//!
//! One thread accepts connections on `$XDG_RUNTIME_DIR/fili-term/daemon.sock`; each connection is attached to one
//! session and gets a reader thread (input, resize, scroll) and a writer thread (frames). A frame carries the damaged
//! lines of the visible screen; the writer sends the next one only after the window acknowledged the previous, so a
//! flood of output (`cat` of a big file) costs the window one frame per paint, never a queue.

use std::collections::HashMap;
use std::io::{self, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::time::{Duration, Instant};

use alacritty_terminal::event::{Event, EventListener, WindowSize};
use alacritty_terminal::grid::{Dimensions, Scroll};
use alacritty_terminal::index::{Column, Line};
use alacritty_terminal::sync::FairMutex;
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::color::Colors;
use alacritty_terminal::term::{Config, Term, TermDamage};
use alacritty_terminal::tty;
use alacritty_terminal::vte::ansi::{Color, CursorShape, NamedColor, Rgb};

use crate::blocks::{self, Blocks, State};
use crate::proto::{self, Reader};
use crate::pty_loop::{Msg, PtyLoop, PtySender};

/// Lines of history kept per session.
const SCROLLBACK: usize = 10_000;
/// Shortest time between two frames of one session (≈ 120 Hz).
const FRAME_INTERVAL: Duration = Duration::from_millis(8);

struct Size {
    cols: usize,
    rows: usize,
}

impl Dimensions for Size {
    fn total_lines(&self) -> usize {
        self.rows
    }
    fn screen_lines(&self) -> usize {
        self.rows
    }
    fn columns(&self) -> usize {
        self.cols
    }
}

/// What happened in a session since the writer last looked.
#[derive(Default)]
struct Pending {
    dirty: bool,
    title: Option<String>,
    exited: Option<i32>,
    bell: bool,
    clipboard: Option<String>,
    /// Set when the attached connection changes: its writer stops.
    generation: u64,
}

struct Shared {
    pending: Mutex<Pending>,
    cv: Condvar,
    sender: OnceLock<PtySender>,
    window: Mutex<WindowSize>,
    /// Colours of the window (PALETTE), answered to OSC 4/10/11 queries.
    palette: Mutex<Option<[u32; 18]>>,
}

impl Shared {
    fn notify(&self, f: impl FnOnce(&mut Pending)) {
        let mut p = self.pending.lock().unwrap();
        f(&mut p);
        self.cv.notify_all();
    }
    fn send(&self, msg: Msg) {
        if let Some(s) = self.sender.get() {
            let _ = s.send(msg);
        }
    }
}

#[derive(Clone)]
struct Listener(Arc<Shared>);

impl EventListener for Listener {
    fn send_event(&self, event: Event) {
        let s = &self.0;
        match event {
            Event::Wakeup => s.notify(|p| p.dirty = true),
            Event::Title(t) => s.notify(|p| p.title = Some(t)),
            Event::ResetTitle => s.notify(|p| p.title = Some(String::new())),
            Event::Bell => s.notify(|p| p.bell = true),
            Event::ClipboardStore(_, text) => s.notify(|p| p.clipboard = Some(text)),
            Event::PtyWrite(text) => s.send(Msg::Input(text.into_bytes().into())),
            Event::ColorRequest(index, format) => {
                let reply = format(palette_rgb(*s.palette.lock().unwrap(), index));
                s.send(Msg::Input(reply.into_bytes().into()));
            }
            Event::TextAreaSizeRequest(format) => {
                let size = *s.window.lock().unwrap();
                s.send(Msg::Input(format(size).into_bytes().into()));
            }
            Event::ChildExit(status) => s.notify(|p| p.exited = Some(status.code().unwrap_or(1))),
            Event::Exit => s.notify(|p| p.exited = p.exited.or(Some(0))),
            _ => {}
        }
    }
}

/// Colours answered to OSC 4/10/11 queries (vim, bat… ask for the background): those of the window (PALETTE), the
/// dark palette of Filigrane until it sent them.
fn palette_rgb(window: Option<[u32; 18]>, index: usize) -> Rgb {
    const DARK: [u32; 18] = [
        0x2d3744, 0xc98f93, 0xa3bfa8, 0xd2bf92, 0x9dbbd6, 0xb3a8c9, 0x95c0c6, 0xa9b8c8, 0x5f7894, 0xd9a5a9, 0xb8d0bc, 0xe0d0a8,
        0xb9d0e4, 0xc6bdd8, 0xadd2d7, 0xdbe7f4, 0xdbe7f4, 0x1c222b,
    ];
    let p = window.unwrap_or(DARK);
    let v = match index {
        i if i < 16 => p[i],
        i if i == NamedColor::Background as usize => p[17],
        _ => p[16],
    };
    Rgb { r: (v >> 16) as u8, g: (v >> 8) as u8, b: v as u8 }
}

struct Session {
    id: u32,
    term: Arc<FairMutex<Term<Listener>>>,
    shared: Arc<Shared>,
    blocks: Arc<Mutex<Blocks>>,
    command: String,
    started: Instant,
}

#[derive(Default)]
struct Daemon {
    sessions: Mutex<HashMap<u32, Arc<Session>>>,
    next_id: AtomicU32,
    next_generation: AtomicU64,
}

pub fn run() -> i32 {
    let dir = proto::runtime_dir();
    let _ = std::fs::create_dir_all(&dir);
    let path = proto::daemon_socket();
    // A live daemon answers; a stale socket file (crash) is replaced
    if UnixStream::connect(&path).is_ok() {
        eprintln!("fili-term: the daemon is already running");
        return 0;
    }
    let _ = std::fs::remove_file(&path);
    let listener = match UnixListener::bind(&path) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("fili-term: {}: {e}", path.display());
            return 1;
        }
    };
    tty::setup_env();
    write_shell_integration();
    // The shells must not inherit the variables of whoever started the daemon's window
    for var in ["QT_QPA_PLATFORMTHEME", "QT_QUICK_BACKEND", "FILI_TERM_TRACE"] {
        // SAFETY: single thread at this point
        unsafe { std::env::remove_var(var) };
    }
    let daemon = Arc::new(Daemon { next_id: AtomicU32::new(1), ..Default::default() });
    for stream in listener.incoming().flatten() {
        let daemon = daemon.clone();
        std::thread::spawn(move || {
            let _ = serve(&daemon, stream);
        });
    }
    0
}

fn serve(daemon: &Arc<Daemon>, mut stream: UnixStream) -> io::Result<()> {
    let (kind, payload) = proto::read_msg(&mut stream)?;
    let mut r = Reader::new(&payload);
    let session = match kind {
        proto::NEW => {
            let size = read_size(&mut r)?;
            let cwd = r.string()?;
            let argc = r.u8()?;
            let mut argv = Vec::new();
            for _ in 0..argc {
                argv.push(r.string()?);
            }
            spawn(daemon, size, cwd, argv)?
        }
        proto::ATTACH => {
            let id = r.u32()?;
            let size = read_size(&mut r)?;
            let session = daemon.sessions.lock().unwrap().get(&id).cloned();
            let session = session.ok_or_else(|| io::Error::other("no such session"))?;
            resize(&session, size);
            session
        }
        proto::SEND => {
            let id = r.u32()?;
            let text = r.rest();
            let session = daemon.sessions.lock().unwrap().get(&id).cloned();
            let Some(session) = session else {
                let mut b = Vec::new();
                proto::put_string(&mut b, &format!("no session {id}"));
                return proto::write_msg(&mut stream, proto::ERROR, &b);
            };
            // Typed, not run: the user reads it and presses Enter; several lines arrive as one paste
            let bracketed = session.term.lock().mode().contains(alacritty_terminal::term::TermMode::BRACKETED_PASTE);
            let text = String::from_utf8_lossy(text).replace("\x1b", "");
            let bytes = if bracketed { format!("\x1b[200~{text}\x1b[201~") } else { text.replace('\n', "\r") };
            session.shared.send(Msg::Input(bytes.into_bytes().into()));
            return proto::write_msg(&mut stream, proto::TEXT, b"");
        }
        proto::QUERY => {
            let id = r.u32()?;
            let what = r.u8()?;
            let block = r.u32()?;
            let session = daemon.sessions.lock().unwrap().get(&id).cloned();
            let Some(session) = session else {
                let mut b = Vec::new();
                proto::put_string(&mut b, &format!("no session {id}"));
                return proto::write_msg(&mut stream, proto::ERROR, &b);
            };
            let term = session.term.lock();
            let blocks = session.blocks.lock().unwrap();
            let answer = match what {
                0 => Ok(blocks.json()),
                _ => {
                    let block = if block == 0 { blocks.last_command().map_or(0, |b| b.id) } else { block };
                    match blocks.lines_of(&term, block) {
                        Some((start, output, end)) => Ok(blocks::text(&term, if what == 1 { output } else { start }, end)),
                        None => Err(format!("no block {block} on the screen or in the history")),
                    }
                }
            };
            drop((term, blocks));
            return match answer {
                Ok(text) => proto::write_msg(&mut stream, proto::TEXT, text.as_bytes()),
                Err(e) => {
                    let mut b = Vec::new();
                    proto::put_string(&mut b, &e);
                    proto::write_msg(&mut stream, proto::ERROR, &b)
                }
            };
        }
        proto::LIST => {
            let mut text = String::new();
            let sessions = daemon.sessions.lock().unwrap();
            let mut ids: Vec<_> = sessions.keys().copied().collect();
            ids.sort();
            for id in ids {
                let s = &sessions[&id];
                text.push_str(&format!("{id}\t{}s\t{}\n", s.started.elapsed().as_secs(), s.command));
            }
            return proto::write_msg(&mut stream, proto::TEXT, text.as_bytes());
        }
        _ => return Ok(()),
    };

    let mut id = Vec::new();
    proto::put_u32(&mut id, session.id);
    proto::write_msg(&mut stream, proto::SESSION, &id)?;

    // This connection takes the session over: an older writer stops
    let generation = daemon.next_generation.fetch_add(1, Ordering::Relaxed) + 1;
    session.shared.notify(|p| {
        p.generation = generation;
        p.dirty = true;
    });
    let acked = Arc::new((Mutex::new(true), Condvar::new()));
    let closed = Arc::new(AtomicBool::new(false));
    // Frames (writer thread) and answers (reader thread) go out whole, one after the other
    let out = Arc::new(Mutex::new(stream.try_clone()?));
    let writer = {
        let (session, out, acked, closed, daemon) = (session.clone(), out.clone(), acked.clone(), closed.clone(), daemon.clone());
        std::thread::spawn(move || write_frames(&daemon, &session, &out, generation, &acked, &closed))
    };

    let result = read_input(&session, &mut stream, &out, &acked);
    closed.store(true, Ordering::Relaxed);
    session.shared.notify(|_| {});
    acked.1.notify_all();
    let _ = writer.join();
    result
}

fn read_size(r: &mut Reader<'_>) -> io::Result<WindowSize> {
    Ok(WindowSize {
        num_cols: r.u16()?.max(2),
        num_lines: r.u16()?.max(1),
        cell_width: r.u16()?.max(1),
        cell_height: r.u16()?.max(1),
    })
}

fn resize(session: &Session, size: WindowSize) {
    *session.shared.window.lock().unwrap() = size;
    session.term.lock().resize(Size { cols: size.num_cols as usize, rows: size.num_lines as usize });
    session.shared.send(Msg::Resize(size));
    session.shared.notify(|p| p.dirty = true);
}

fn spawn(daemon: &Daemon, size: WindowSize, cwd: String, argv: Vec<String>) -> io::Result<Arc<Session>> {
    let id = daemon.next_id.fetch_add(1, Ordering::Relaxed);
    let command = if argv.is_empty() { std::env::var("SHELL").unwrap_or_else(|_| "sh".into()) } else { argv.join(" ") };
    let mut env = HashMap::new();
    env.insert("FILI_TERM_SESSION".to_string(), id.to_string());
    // The user's shell gets the integration (marks of prompts and commands); a command given on the line does not
    let argv = if argv.is_empty() { with_integration(&command, &mut env) } else { argv };
    let shell = argv.split_first().map(|(p, a)| tty::Shell::new(p.clone(), a.to_vec()));
    let options = tty::Options {
        shell,
        working_directory: Some(PathBuf::from(cwd)).filter(|p| p.is_dir()),
        drain_on_exit: true,
        env,
        ..Default::default()
    };
    let pty = tty::new(&options, size, id as u64)?;
    let shared = Arc::new(Shared {
        pending: Mutex::new(Pending::default()),
        cv: Condvar::new(),
        sender: OnceLock::new(),
        window: Mutex::new(size),
        palette: Mutex::new(None),
    });
    let config = Config { scrolling_history: SCROLLBACK, ..Default::default() };
    let term = Term::new(config, &Size { cols: size.num_cols as usize, rows: size.num_lines as usize }, Listener(shared.clone()));
    let term = Arc::new(FairMutex::new(term));
    let blocks = Arc::new(Mutex::new(Blocks::default()));
    let pty_loop = PtyLoop::new(term.clone(), blocks.clone(), Listener(shared.clone()), pty)?;
    let _ = shared.sender.set(pty_loop.channel());
    pty_loop.spawn();
    let session = Arc::new(Session { id, term, shared, blocks, command, started: Instant::now() });
    daemon.sessions.lock().unwrap().insert(id, session.clone());
    Ok(session)
}

fn read_input(
    session: &Session,
    stream: &mut UnixStream,
    out: &Mutex<UnixStream>,
    acked: &(Mutex<bool>, Condvar),
) -> io::Result<()> {
    loop {
        let (kind, payload) = proto::read_msg(stream)?;
        let mut r = Reader::new(&payload);
        match kind {
            proto::INPUT => {
                {
                    let mut term = session.term.lock();
                    if term.grid().display_offset() != 0 {
                        term.scroll_display(Scroll::Bottom);
                        session.shared.notify(|p| p.dirty = true);
                    }
                }
                session.shared.send(Msg::Input(payload.into()));
            }
            proto::RESIZE => resize(session, read_size(&mut r)?),
            proto::SCROLL => {
                let kind = r.u8()?;
                let lines = r.i32()?;
                let mut term = session.term.lock();
                let scroll = match kind {
                    1 => Scroll::PageUp,
                    2 => Scroll::PageDown,
                    3 => Scroll::Top,
                    4 => Scroll::Bottom,
                    // To the start of the command before or after the one at the top of the view
                    5 | 6 => {
                        let offset = term.grid().display_offset() as i32;
                        match Blocks::neighbour(&term, -offset, if kind == 5 { -1 } else { 1 }) {
                            Some(line) => Scroll::Delta(-line - offset),
                            None if kind == 6 => Scroll::Bottom,
                            None => Scroll::Delta(0),
                        }
                    }
                    _ => Scroll::Delta(lines),
                };
                term.scroll_display(scroll);
                drop(term);
                session.shared.notify(|p| p.dirty = true);
            }
            proto::TEXT_OF => {
                let request = r.u32()?;
                let id = r.u32()?;
                let part = r.u8()?;
                let text = {
                    let term = session.term.lock();
                    let blocks = session.blocks.lock().unwrap();
                    match blocks.lines_of(&term, id) {
                        Some((start, output, end)) => blocks::text(&term, if part == 0 { output } else { start }, end),
                        None => String::new(),
                    }
                };
                let mut b = Vec::with_capacity(text.len() + 4);
                proto::put_u32(&mut b, request);
                b.extend_from_slice(text.as_bytes());
                proto::write_msg(&mut *out.lock().unwrap(), proto::BLOCK_TEXT, &b)?;
            }
            proto::PALETTE => {
                let mut p = [0u32; 18];
                for v in &mut p {
                    *v = r.u32()?;
                }
                *session.shared.palette.lock().unwrap() = Some(p);
            }
            proto::ACK => {
                *acked.0.lock().unwrap() = true;
                acked.1.notify_all();
            }
            proto::CLOSE => {
                session.shared.send(Msg::Shutdown);
                // SIGHUP to the shell: the pty is dropped with the event loop
                return Ok(());
            }
            _ => {}
        }
    }
}

fn write_frames(
    daemon: &Daemon,
    session: &Session,
    stream: &Mutex<UnixStream>,
    generation: u64,
    acked: &(Mutex<bool>, Condvar),
    closed: &AtomicBool,
) -> io::Result<()> {
    let mut full = true;
    let mut last = Instant::now() - FRAME_INTERVAL;
    // Version of the block table the window has seen (0: all of it is sent first)
    let mut seen = 0u64;
    let mut seen_cwd = 0u64;
    loop {
        // Wait for the window to show the previous frame
        {
            let mut a = acked.0.lock().unwrap();
            while !*a && !closed.load(Ordering::Relaxed) {
                a = acked.1.wait_timeout(a, Duration::from_millis(500)).unwrap().0;
            }
        }
        // Then for something to send
        let (title, exited, bell, clipboard) = {
            let mut p = session.shared.pending.lock().unwrap();
            loop {
                if closed.load(Ordering::Relaxed) || p.generation != generation {
                    return Ok(());
                }
                if p.dirty || p.title.is_some() || p.exited.is_some() || p.bell || p.clipboard.is_some() {
                    break;
                }
                p = session.shared.cv.wait(p).unwrap();
            }
            p.dirty = false;
            (p.title.take(), p.exited, std::mem::take(&mut p.bell), p.clipboard.take())
        };
        // Coalesce: everything that arrives within one frame interval goes in one frame
        let since = last.elapsed();
        if since < FRAME_INTERVAL {
            std::thread::sleep(FRAME_INTERVAL - since);
        }
        session.shared.pending.lock().unwrap().dirty = false;
        let mut out = Vec::new();
        let frame = {
            let mut term = session.term.lock();
            let blocks = session.blocks.lock().unwrap();
            // Blocks first: the frame that shows them finds them known
            for b in blocks.changed_since(seen) {
                write_into(&mut out, proto::BLOCK, &encode_block(b));
            }
            seen = blocks.version;
            if blocks.cwd_version > seen_cwd {
                seen_cwd = blocks.cwd_version;
                let mut b = Vec::new();
                proto::put_string(&mut b, &blocks.cwd);
                write_into(&mut out, proto::CWD, &b);
            }
            encode_frame(&mut term, &blocks, full)
        };
        full = false;
        last = Instant::now();
        *acked.0.lock().unwrap() = false;
        write_into(&mut out, proto::FRAME, &frame);
        if let Some(t) = title {
            let mut b = Vec::new();
            proto::put_string(&mut b, &t);
            write_into(&mut out, proto::TITLE, &b);
        }
        if let Some(text) = clipboard {
            let mut b = Vec::new();
            proto::put_string(&mut b, &text);
            write_into(&mut out, proto::CLIPBOARD, &b);
        }
        if bell {
            write_into(&mut out, proto::BELL, &[]);
        }
        if let Some(code) = exited {
            write_into(&mut out, proto::EXIT, &code.to_le_bytes());
            stream.lock().unwrap().write_all(&out)?;
            daemon.sessions.lock().unwrap().remove(&session.id);
            return Ok(());
        }
        stream.lock().unwrap().write_all(&out)?;
    }
}

fn write_into(out: &mut Vec<u8>, kind: u8, payload: &[u8]) {
    out.extend_from_slice(&(payload.len() as u32 + 1).to_le_bytes());
    out.push(kind);
    out.extend_from_slice(payload);
}

/// Colour as sent to the window: `0x00RRGGBB` for a true colour, `0x01000000 | n` for a named colour (n as in
/// alacritty's `NamedColor`: 0-15 ANSI, 256 foreground, 257 background, 258 cursor, 259-266 dim, 267 bright fg, 268
/// dim fg), `0x02000000 | n` for an index of the 256-colour palette. Colours redefined by the program (OSC 4) are
/// sent resolved.
fn color(c: Color, colors: &Colors) -> u32 {
    let rgb = |v: &Rgb| (v.r as u32) << 16 | (v.g as u32) << 8 | v.b as u32;
    match c {
        Color::Spec(v) => rgb(&v),
        Color::Indexed(i) => colors[i as usize].as_ref().map(rgb).unwrap_or(0x0200_0000 | i as u32),
        Color::Named(n) => colors[n].as_ref().map(rgb).unwrap_or(0x0100_0000 | n as u32),
    }
}

/// FRAME payload:
/// `u16 cols, u16 rows, u32 mode, u16 cursor col, i16 cursor line (-1: not on screen), u8 cursor shape (0 hidden,
/// 1 block, 2 underline, 3 beam, 4 hollow block), u32 display offset, u32 history size, u8 full, u16 line count`,
/// then per line `u16 line` and `cols` cells of `u32 char (bit 31: zero-width characters follow), u32 fg, u32 bg,
/// u16 flags` (+ `u8 n, n × u32` when bit 31 is set), then the command blocks on screen: `u16 count` and per block
/// `i32 first row, i32 first row of the output (i32::MIN: none), i32 last row, u32 id` (rows of the view, the first
/// may be above it).
fn encode_frame(term: &mut Term<Listener>, blocks: &Blocks, full: bool) -> Vec<u8> {
    let cols = term.columns();
    let rows = term.screen_lines();
    let lines: Vec<usize> = match term.damage() {
        TermDamage::Full => (0..rows).collect(),
        TermDamage::Partial(_) if full => (0..rows).collect(),
        TermDamage::Partial(it) => {
            let mut v: Vec<usize> = it.map(|d| d.line).collect();
            v.sort_unstable();
            v.dedup();
            v
        }
    };
    term.reset_damage();
    let content = term.renderable_content();
    let offset = content.display_offset;
    let cursor = content.cursor;
    let mode = content.mode.bits();
    let cursor_line = cursor.point.line.0 + offset as i32;
    let shape = match cursor.shape {
        _ if cursor_line < 0 || cursor_line >= rows as i32 => 0u8,
        CursorShape::Hidden => 0,
        CursorShape::Block => 1,
        CursorShape::Underline => 2,
        CursorShape::Beam => 3,
        CursorShape::HollowBlock => 4,
    };
    let colors = content.colors;
    let grid = term.grid();
    let mut out = Vec::with_capacity(32 + lines.len() * (2 + cols * 14));
    proto::put_u16(&mut out, cols as u16);
    proto::put_u16(&mut out, rows as u16);
    proto::put_u32(&mut out, mode);
    proto::put_u16(&mut out, cursor.point.column.0 as u16);
    proto::put_u16(&mut out, (if shape == 0 { -1 } else { cursor_line } as i16) as u16);
    out.push(shape);
    proto::put_u32(&mut out, offset as u32);
    proto::put_u32(&mut out, grid.history_size() as u32);
    out.push((lines.len() == rows) as u8);
    proto::put_u16(&mut out, lines.len() as u16);
    for &line in &lines {
        proto::put_u16(&mut out, line as u16);
        let row = &grid[Line(line as i32 - offset as i32)];
        for col in 0..cols {
            let cell = &row[Column(col)];
            let extra = cell.zerowidth().filter(|z| !z.is_empty());
            let mut c = cell.c as u32;
            if extra.is_some() {
                c |= 1 << 31;
            }
            proto::put_u32(&mut out, c);
            proto::put_u32(&mut out, color(cell.fg, colors));
            proto::put_u32(&mut out, color(cell.bg, colors));
            proto::put_u16(&mut out, (cell.flags & Flags::all()).bits());
            if let Some(z) = extra {
                let z = &z[..z.len().min(255)];
                out.push(z.len() as u8);
                for ch in z {
                    proto::put_u32(&mut out, *ch as u32);
                }
            }
        }
    }
    let visible = blocks.visible(term, offset as i32);
    proto::put_u16(&mut out, visible.len() as u16);
    for v in visible {
        proto::put_i32(&mut out, v.start);
        proto::put_i32(&mut out, v.output);
        proto::put_i32(&mut out, v.end);
        proto::put_u32(&mut out, v.id);
    }
    out
}

fn encode_block(b: &blocks::Block) -> Vec<u8> {
    let mut out = Vec::with_capacity(32 + b.command.len() + b.cwd.len());
    proto::put_u32(&mut out, b.id);
    out.push(match b.state {
        State::Prompt => 0,
        State::Running => 1,
        State::Done => 2,
    });
    proto::put_i32(&mut out, b.exit.unwrap_or(i32::MIN));
    proto::put_u32(&mut out, b.duration.map_or(u32::MAX, |d| d.as_millis().min(u32::MAX as u128 - 1) as u32));
    proto::put_string(&mut out, &b.command);
    proto::put_string(&mut out, &b.cwd);
    out
}

// ---------------------------------------------------------------- shell integration

const ZSHENV: &str = include_str!("../../shell/zsh/.zshenv");
const ZSH: &str = include_str!("../../shell/fili-term.zsh");
const BASH: &str = include_str!("../../shell/fili-term.bash");

fn shell_dir() -> PathBuf {
    proto::runtime_dir().join("shell")
}

/// The scripts of the integration, in the runtime directory (they ship inside the daemon: nothing to install).
fn write_shell_integration() {
    let dir = shell_dir();
    let _ = std::fs::create_dir_all(dir.join("zsh"));
    let _ = std::fs::write(dir.join("zsh/.zshenv"), ZSHENV);
    let _ = std::fs::write(dir.join("fili-term.zsh"), ZSH);
    let _ = std::fs::write(dir.join("fili-term.bash"), BASH);
}

/// Command line of the user's shell with the integration loaded: zsh reads our `.zshenv` first (ZDOTDIR, put back
/// before the user's files), bash our rc file, which reads `~/.bashrc`. Other shells start as they are. Turned off by
/// `FILI_TERM_SHELL_INTEGRATION=0` in the environment of the daemon.
fn with_integration(shell: &str, env: &mut HashMap<String, String>) -> Vec<String> {
    if std::env::var("FILI_TERM_SHELL_INTEGRATION").is_ok_and(|v| v == "0") {
        return Vec::new();
    }
    let dir = shell_dir();
    match shell.rsplit('/').next().unwrap_or("") {
        "zsh" => {
            if let Ok(old) = std::env::var("ZDOTDIR") {
                env.insert("FILI_TERM_ZDOTDIR".into(), old);
            }
            env.insert("ZDOTDIR".into(), dir.join("zsh").to_string_lossy().into_owned());
            Vec::new()
        }
        "bash" => vec![shell.into(), "--rcfile".into(), dir.join("fili-term.bash").to_string_lossy().into_owned()],
        _ => Vec::new(),
    }
}
