//! fili-term: the Filigrane terminal.
//!
//! `fili-term-daemon` (daemon/) keeps the sessions (ptys + terminal state); `fili-term` opens a window on a new session, in the
//! window process already running if there is one (one Qt process for every window).
mod ctl;
mod gui;
mod proto;
mod window;

use std::io::Write;
use std::os::unix::net::UnixStream;

fn usage() -> i32 {
    eprintln!(
        "usage: fili-term [-e COMMAND [ARGS…]] [--attach ID]\n       fili-term list\n       \
         fili-term blocks [--session ID]                 commands of a session (JSON)\n       \
         fili-term output [--session ID] [--all] [BLOCK] output of a command (default: the last one)\n       \
         fili-term send [--session ID] TEXT              types TEXT at the prompt, without Enter\n       \
         fili-term ctl [--pid PID] action NAME | actions\n\n\
         Opens a terminal window (Aéroport); sessions live in the daemon and survive their window.\n\
         The session defaults to $FILI_TERM_SESSION (set in each Aéroport shell)."
    );
    2
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let code = match args.first().map(String::as_str) {
        Some("daemon") => {
            eprintln!("fili-term: the daemon is fili-term-daemon (started on demand)");
            2
        }
        Some("list") => list(),
        // Command an open window (menu of the fili-bar); never starts Qt
        Some("ctl") => match ctl::run(&args[1..]) {
            Ok(answer) => {
                println!("{answer}");
                0
            }
            Err(e) => {
                eprintln!("fili-term ctl: {e}");
                1
            }
        },
        Some(verb @ ("blocks" | "output" | "send")) => match session_command(verb, &args[1..]) {
            Ok(text) => {
                if !text.is_empty() {
                    println!("{}", text.trim_end_matches('\n'));
                }
                0
            }
            Err(e) => {
                eprintln!("fili-term {verb}: {e}");
                1
            }
        },
        Some("-h" | "--help") => usage(),
        _ => open(&args),
    };
    std::process::exit(code);
}

fn list() -> i32 {
    let Ok(mut s) = UnixStream::connect(proto::daemon_socket()) else {
        eprintln!("fili-term: no daemon");
        return 1;
    };
    if proto::write_msg(&mut s, proto::LIST, &[]).is_err() {
        return 1;
    }
    match proto::read_msg(&mut s) {
        Ok((_, text)) => {
            print!("{}", String::from_utf8_lossy(&text));
            0
        }
        Err(_) => 1,
    }
}

/// `blocks`, `output`, `send`: one request to the daemon about a session
fn session_command(verb: &str, args: &[String]) -> Result<String, String> {
    let mut session: Option<u32> = std::env::var("FILI_TERM_SESSION").ok().and_then(|v| v.parse().ok());
    let mut all = false;
    let mut rest: Vec<String> = Vec::new();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--session" => session = Some(it.next().and_then(|v| v.parse().ok()).ok_or("--session needs a number")?),
            "--all" if verb == "output" => all = true,
            _ => rest.push(a.clone()),
        }
    }
    let session = session.ok_or("no session: --session ID (see `fili-term list` or the context of Aéroport)")?;
    let mut s = UnixStream::connect(proto::daemon_socket()).map_err(|_| "no daemon")?;
    let mut b = Vec::new();
    proto::put_u32(&mut b, session);
    let kind = match verb {
        "send" => {
            if rest.is_empty() {
                return Err("nothing to send".into());
            }
            b.extend_from_slice(rest.join(" ").as_bytes());
            proto::SEND
        }
        _ => {
            let block = match rest.as_slice() {
                [] => 0,
                [n] => n.parse::<u32>().map_err(|_| format!("not a block number: {n}"))?,
                _ => return Err("one block at a time".into()),
            };
            b.push(if verb == "blocks" { 0 } else if all { 2 } else { 1 });
            proto::put_u32(&mut b, block);
            proto::QUERY
        }
    };
    proto::write_msg(&mut s, kind, &b).map_err(|e| e.to_string())?;
    let (kind, payload) = proto::read_msg(&mut s).map_err(|e| e.to_string())?;
    if kind == proto::ERROR {
        return Err(proto::Reader::new(&payload).string().unwrap_or_default());
    }
    Ok(String::from_utf8_lossy(&payload).into_owned())
}

fn open(args: &[String]) -> i32 {
    let mut command: Vec<String> = Vec::new();
    let mut attach = 0u32;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "-e" | "--" => {
                command = it.by_ref().cloned().collect();
            }
            "--attach" => attach = it.next().and_then(|v| v.parse().ok()).unwrap_or(0),
            _ => return usage(),
        }
    }
    if !ensure_daemon() {
        eprintln!("fili-term: cannot start the session daemon");
        return 1;
    }
    let cwd = std::env::current_dir().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
    // Request line for the window process: "open\t<attach>\t<cwd>\t<argv…>" (tabs never appear in a cwd we accept)
    let mut request = format!("open\t{attach}\t{cwd}");
    for a in &command {
        request.push('\t');
        request.push_str(&a.replace('\t', " "));
    }
    // A window process already running opens the window itself
    if let Ok(mut s) = UnixStream::connect(proto::runtime_dir().join("ui.sock")) {
        if s.write_all(format!("{request}\n").as_bytes()).is_ok() {
            return 0;
        }
    }
    gui::run(&request)
}

/// Make sure a daemon is listening; start one in the background otherwise. Returns false if none could be reached.
fn ensure_daemon() -> bool {
    let path = proto::daemon_socket();
    use std::os::unix::net::UnixStream;
    use std::time::Duration;
    if UnixStream::connect(&path).is_ok() {
        return true;
    }
    // Next to this executable (installed or target/), else in PATH
    let exe = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("fili-term-daemon")))
        .filter(|p| p.exists())
        .unwrap_or_else(|| "fili-term-daemon".into());
    // Detached: its own session, no terminal, survives the window
    let mut cmd = std::process::Command::new(exe);
    cmd.stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null());
    use std::os::unix::process::CommandExt;
    // SAFETY: setsid is async-signal-safe
    unsafe {
        cmd.pre_exec(|| {
            libc::setsid();
            Ok(())
        });
    }
    if cmd.spawn().is_err() {
        return false;
    }
    for _ in 0..100 {
        if UnixStream::connect(&path).is_ok() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    false
}
