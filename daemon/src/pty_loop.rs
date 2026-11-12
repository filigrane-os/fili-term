//! I/O loop of a session's pty: reads the shell's output into the terminal state, writes the keys to the shell.
//!
//! Adapted from alacritty_terminal's `event_loop.rs` (Apache-2.0, The Alacritty Project), whose parser cannot be told
//! about the marks of the shell: here the bytes are scanned for them first (blocks::Scanner) and parsed up to each
//! mark, which is then applied to the terminal (blocks::Blocks::apply) with the screen as it stands at that point.

use std::borrow::Cow;
use std::collections::VecDeque;
use std::io::{self, ErrorKind, Read, Write};
use std::num::NonZeroUsize;
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use alacritty_terminal::event::{Event, EventListener, OnResize, WindowSize};
use alacritty_terminal::sync::FairMutex;
use alacritty_terminal::term::Term;
use alacritty_terminal::tty::{self, EventedPty, EventedReadWrite};
use alacritty_terminal::vte::ansi;
use polling::{Event as PollingEvent, Events, PollMode, Poller};

use crate::blocks::{Blocks, Mark, Scanner};

/// Max bytes read from the pty before the terminal is forcibly locked.
const READ_BUFFER_SIZE: usize = 0x10_0000;
/// Max bytes read while the terminal is locked.
const MAX_LOCKED_READ: usize = u16::MAX as usize;
/// Poller keys of alacritty_terminal's unix pty (`tty::unix`, crate-private there).
const PTY_READ_WRITE_TOKEN: usize = 0;
const PTY_CHILD_EVENT_TOKEN: usize = 1;

pub enum Msg {
    Input(Cow<'static, [u8]>),
    Shutdown,
    Resize(WindowSize),
}

#[derive(Clone)]
pub struct PtySender {
    tx: Sender<Msg>,
    poller: Arc<Poller>,
}

impl PtySender {
    pub fn send(&self, msg: Msg) {
        if self.tx.send(msg).is_ok() {
            let _ = self.poller.notify();
        }
    }
}

pub struct PtyLoop<U: EventListener> {
    poll: Arc<Poller>,
    pty: tty::Pty,
    rx: Receiver<Msg>,
    peeked: Option<Msg>,
    tx: Sender<Msg>,
    terminal: Arc<FairMutex<Term<U>>>,
    blocks: Arc<Mutex<Blocks>>,
    events: U,
}

#[derive(Default)]
struct State {
    write_list: VecDeque<Cow<'static, [u8]>>,
    writing: Option<(Cow<'static, [u8]>, usize)>,
    parser: ansi::Processor,
    scanner: Scanner,
    marks: Vec<(usize, Mark)>,
}

impl<U: EventListener + Send + 'static> PtyLoop<U> {
    pub fn new(terminal: Arc<FairMutex<Term<U>>>, blocks: Arc<Mutex<Blocks>>, events: U, pty: tty::Pty) -> io::Result<Self> {
        let (tx, rx) = mpsc::channel();
        Ok(Self { poll: Poller::new()?.into(), pty, rx, peeked: None, tx, terminal, blocks, events })
    }

    pub fn channel(&self) -> PtySender {
        PtySender { tx: self.tx.clone(), poller: self.poll.clone() }
    }

    fn peek(&mut self) -> bool {
        if self.peeked.is_none() {
            self.peeked = self.rx.try_recv().ok();
        }
        self.peeked.is_some()
    }

    /// Returns false on shutdown.
    fn drain_channel(&mut self, state: &mut State) -> bool {
        loop {
            let msg = match self.peeked.take() {
                Some(m) => m,
                None => match self.rx.try_recv() {
                    Ok(m) => m,
                    Err(TryRecvError::Empty) => return true,
                    Err(TryRecvError::Disconnected) => return false,
                },
            };
            match msg {
                Msg::Input(input) => state.write_list.push_back(input),
                Msg::Resize(size) => self.pty.on_resize(size),
                Msg::Shutdown => return false,
            }
        }
    }

    fn pty_read(&mut self, state: &mut State, buf: &mut [u8]) -> io::Result<()> {
        let mut unprocessed = 0;
        let mut processed = 0;
        let _lease = self.terminal.lease();
        let mut terminal = None;
        loop {
            match self.pty.reader().read(&mut buf[unprocessed..]) {
                Ok(0) if unprocessed == 0 => break,
                Ok(got) => unprocessed += got,
                Err(err) => match err.kind() {
                    ErrorKind::Interrupted | ErrorKind::WouldBlock => {
                        if unprocessed == 0 {
                            break;
                        }
                    }
                    _ => return Err(err),
                },
            }
            let terminal = match &mut terminal {
                Some(t) => t,
                None => terminal.insert(match self.terminal.try_lock_unfair() {
                    None if unprocessed >= READ_BUFFER_SIZE => self.terminal.lock_unfair(),
                    None => continue,
                    Some(t) => t,
                }),
            };
            let bytes = &buf[..unprocessed];
            let marks = &mut state.marks;
            state.scanner.feed(bytes, |end, mark| marks.push((end, mark)));
            if state.marks.is_empty() {
                state.parser.advance(&mut **terminal, bytes);
            } else {
                // Parsed up to each mark, which then applies to the screen as it is there
                let mut from = 0;
                let mut blocks = self.blocks.lock().unwrap();
                for (end, mark) in state.marks.drain(..) {
                    state.parser.advance(&mut **terminal, &bytes[from..end]);
                    from = end;
                    blocks.apply(&mut **terminal, mark);
                }
                state.parser.advance(&mut **terminal, &bytes[from..]);
            }
            processed += unprocessed;
            unprocessed = 0;
            if processed >= MAX_LOCKED_READ {
                break;
            }
        }
        if state.parser.sync_bytes_count() < processed && processed > 0 {
            self.events.send_event(Event::Wakeup);
        }
        Ok(())
    }

    fn pty_write(&mut self, state: &mut State) -> io::Result<()> {
        if state.writing.is_none() {
            state.writing = state.write_list.pop_front().map(|b| (b, 0));
        }
        while let Some((bytes, mut written)) = state.writing.take() {
            loop {
                match self.pty.writer().write(&bytes[written..]) {
                    Ok(0) => {
                        state.writing = Some((bytes, written));
                        return Ok(());
                    }
                    Ok(n) => {
                        written += n;
                        if written >= bytes.len() {
                            state.writing = state.write_list.pop_front().map(|b| (b, 0));
                            break;
                        }
                    }
                    Err(err) => {
                        state.writing = Some((bytes, written));
                        return match err.kind() {
                            ErrorKind::Interrupted | ErrorKind::WouldBlock => Ok(()),
                            _ => Err(err),
                        };
                    }
                }
            }
        }
        Ok(())
    }

    pub fn spawn(mut self) {
        std::thread::Builder::new()
            .name("pty".into())
            .spawn(move || {
                let mut state = State::default();
                let mut buf = vec![0u8; READ_BUFFER_SIZE];
                let mut interest = PollingEvent::readable(0);
                // SAFETY: the pty outlives its registration, removed below
                if unsafe { self.pty.register(&self.poll, interest, PollMode::Level) }.is_err() {
                    return;
                }
                let mut events = Events::with_capacity(NonZeroUsize::new(1024).unwrap());
                'event_loop: loop {
                    // Wake up when a synchronized update (DECSET 2026) times out
                    let timeout = state.parser.sync_timeout().sync_timeout().map(|t| t.saturating_duration_since(Instant::now()));
                    events.clear();
                    if let Err(err) = self.poll.wait(&mut events, timeout) {
                        if err.kind() == ErrorKind::Interrupted {
                            continue;
                        }
                        break;
                    }
                    if events.is_empty() && !self.peek() {
                        state.parser.stop_sync(&mut *self.terminal.lock());
                        self.events.send_event(Event::Wakeup);
                        continue;
                    }
                    if !self.drain_channel(&mut state) {
                        break;
                    }
                    for event in events.iter() {
                        match event.key {
                            PTY_CHILD_EVENT_TOKEN => {
                                if let Some(tty::ChildEvent::Exited(status)) = self.pty.next_child_event() {
                                    if let Some(status) = status {
                                        self.events.send_event(Event::ChildExit(status));
                                    }
                                    let _ = self.pty_read(&mut state, &mut buf);
                                    self.terminal.lock().exit();
                                    self.events.send_event(Event::Wakeup);
                                    break 'event_loop;
                                }
                            }
                            PTY_READ_WRITE_TOKEN => {
                                if event.is_interrupt() {
                                    continue;
                                }
                                if event.readable {
                                    if let Err(err) = self.pty_read(&mut state, &mut buf) {
                                        // EIO when the shell hung up: the child event follows
                                        if err.raw_os_error() == Some(libc::EIO) {
                                            continue;
                                        }
                                        break 'event_loop;
                                    }
                                }
                                if event.writable && self.pty_write(&mut state).is_err() {
                                    break 'event_loop;
                                }
                            }
                            _ => {}
                        }
                    }
                    let needs_write = state.writing.is_some() || !state.write_list.is_empty();
                    if needs_write != interest.writable {
                        interest.writable = needs_write;
                        let _ = self.pty.reregister(&self.poll, interest, PollMode::Level);
                    }
                }
                let _ = self.pty.deregister(&self.poll);
            })
            .expect("pty thread");
    }
}
