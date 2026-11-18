//! fili-term-daemon: keeps the terminal sessions of fili-term (ptys + screen state) alive between windows.
//! Started on demand by the window process; tiny and Qt-free on purpose.
#[path = "../../src/proto.rs"]
mod proto;
// The window uses the rest of it (the panel reads the buffer)
#[allow(dead_code)]
#[path = "../../src/logs.rs"]
mod logs;
mod blocks;
mod foreground;
mod pty_loop;
mod search;
mod session;

fn main() {
    std::process::exit(session::run());
}
