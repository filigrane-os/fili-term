//! fili-term-daemon: keeps the terminal sessions of fili-term (ptys + screen state) alive between windows.
//! Started on demand by the window process; tiny and Qt-free on purpose.
#[path = "../../src/proto.rs"]
mod proto;
mod session;

fn main() {
    std::process::exit(session::run());
}
