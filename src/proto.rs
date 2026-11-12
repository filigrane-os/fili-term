#![allow(dead_code)] // shared by the window and the daemon, each uses its half
//! Wire protocol between the session daemon and the window (src/gui/terminal.rs reads and writes it).
//!
//! Every message is `u32 length (LE) | u8 kind | payload`, the length counting the kind byte and the payload.
//! Integers are little-endian. Strings are `u16 length | UTF-8 bytes`.

use std::io::{self, Read, Write};

// Window → daemon
/// New session: u16 cols, u16 rows, u16 cell width, u16 cell height, string cwd, u8 argc, argc × string.
pub const NEW: u8 = 1;
/// Attach to a living session: u32 id, u16 cols, u16 rows, u16 cell width, u16 cell height.
pub const ATTACH: u8 = 2;
/// Bytes for the pty (keys, paste, mouse reports): raw payload.
pub const INPUT: u8 = 3;
/// u16 cols, u16 rows, u16 cell width, u16 cell height.
pub const RESIZE: u8 = 4;
/// u8 kind (0 delta, 1 page up, 2 page down, 3 top, 4 bottom), i32 lines (delta, positive = back in history).
pub const SCROLL: u8 = 5;
/// End the session (hang up its shell).
pub const CLOSE: u8 = 6;
/// List the sessions: answered by `TEXT`.
pub const LIST: u8 = 7;
/// The last frame is on screen: the daemon sends the next one only after this (frames never pile up in the window).
pub const ACK: u8 = 8;

// Daemon → window
/// u32 id of the session this connection is attached to.
pub const SESSION: u8 = 10;
/// Screen update, see `daemon::encode_frame`.
pub const FRAME: u8 = 11;
/// Window title (string).
pub const TITLE: u8 = 12;
/// The shell exited: i32 status.
pub const EXIT: u8 = 13;
pub const BELL: u8 = 14;
/// Text for the clipboard (OSC 52): string.
pub const CLIPBOARD: u8 = 15;
/// Plain text answer (LIST): raw UTF-8.
pub const TEXT: u8 = 16;

pub fn read_msg(r: &mut impl Read) -> io::Result<(u8, Vec<u8>)> {
    let mut len = [0u8; 4];
    r.read_exact(&mut len)?;
    let len = u32::from_le_bytes(len) as usize;
    if len == 0 || len > 64 << 20 {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "bad message length"));
    }
    let mut buf = vec![0u8; len];
    r.read_exact(&mut buf)?;
    let kind = buf.remove(0);
    Ok((kind, buf))
}

pub fn write_msg(w: &mut impl Write, kind: u8, payload: &[u8]) -> io::Result<()> {
    let mut out = Vec::with_capacity(payload.len() + 5);
    out.extend_from_slice(&(payload.len() as u32 + 1).to_le_bytes());
    out.push(kind);
    out.extend_from_slice(payload);
    w.write_all(&out)
}

/// Little cursor over a payload.
pub struct Reader<'a> {
    buf: &'a [u8],
}

impl<'a> Reader<'a> {
    pub fn new(buf: &'a [u8]) -> Self {
        Self { buf }
    }
    fn take(&mut self, n: usize) -> io::Result<&'a [u8]> {
        if self.buf.len() < n {
            return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "short payload"));
        }
        let (a, b) = self.buf.split_at(n);
        self.buf = b;
        Ok(a)
    }
    pub fn u8(&mut self) -> io::Result<u8> {
        Ok(self.take(1)?[0])
    }
    pub fn u16(&mut self) -> io::Result<u16> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into().unwrap()))
    }
    pub fn u32(&mut self) -> io::Result<u32> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    pub fn i32(&mut self) -> io::Result<i32> {
        Ok(i32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    pub fn string(&mut self) -> io::Result<String> {
        let n = self.u16()? as usize;
        Ok(String::from_utf8_lossy(self.take(n)?).into_owned())
    }
}

pub fn put_u16(out: &mut Vec<u8>, v: u16) {
    out.extend_from_slice(&v.to_le_bytes());
}
pub fn put_u32(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_le_bytes());
}
pub fn put_string(out: &mut Vec<u8>, s: &str) {
    let b = &s.as_bytes()[..s.len().min(u16::MAX as usize)];
    put_u16(out, b.len() as u16);
    out.extend_from_slice(b);
}

/// Path of the daemon's socket: `$XDG_RUNTIME_DIR/fili-term/daemon.sock`.
pub fn runtime_dir() -> std::path::PathBuf {
    let base = std::env::var_os("XDG_RUNTIME_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join(format!("fili-term-{}", unsafe { libc::getuid() })));
    base.join("fili-term")
}

pub fn daemon_socket() -> std::path::PathBuf {
    runtime_dir().join("daemon.sock")
}
