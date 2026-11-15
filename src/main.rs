//! fili-term: the Filigrane terminal.
//!
//! `fili-term-daemon` (daemon/) keeps the sessions (ptys + terminal state); `fili-term` opens a window on a new session, in the
//! window process already running if there is one (one Qt process for every window).
mod ctl;
mod gui;
mod modes;
mod proto;
mod window;

use std::io::Write;
use std::os::unix::net::UnixStream;

fn usage() -> i32 {
    eprintln!(
        "usage: fili-term [--new] [--app-id ID] [-T TITLE] [--hold] [--tab] [--no-header] [--no-footer] [-e COMMAND [ARGS…]] [--attach ID]\n       fili-term list\n       \
         fili-term blocks [--session ID]                 commands of a session (JSON)\n       \
         fili-term output [--session ID] [--all] [BLOCK] output of a command (default: the last one)\n       \
         fili-term send [--session ID] TEXT              types TEXT at the prompt, without Enter\n       \
         fili-term keymap [default|mac|tmux|none]         show or choose the keyboard shortcuts\n       \
         fili-term mode herdr [--remote TARGET [--session NAME]]   herdr's tabs as tabs of the window, its agents in the sidebar\n       \
         fili-term ctl [--pid PID] action NAME | actions\n       \
         fili-term report [--session ID] working|blocked|done|idle [--agent NAME]   an agent's state (hooks)\n\n\
         Opens a terminal window (Aéroport); sessions live in the daemon and survive their window.\n\
         --app-id ID: the window gets that Wayland app-id (for compositor rules), in a process of its own.\n\
         --tab: open a tab in the window that has the focus (a new window when none is open).\n\
         --new: open a new window without bringing back the windows of the last run (restore in fili-term.toml).\n\
         --hold: keep the tab open after the command exits, to read its output.\n\
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
        Some("keymap") => keymap(&args[1..]),
        Some("mode") => mode(&args[1..]),
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
        // An agent's hook: its state on its pane (nothing outside Aéroport)
        Some("report") => match ctl::report(&args[1..]) {
            Ok(()) => 0,
            Err(e) => {
                eprintln!("fili-term report: {e}");
                2
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

/// `fili-term keymap [NAME]`: the preset of keyboard shortcuts (written as `keymap = "NAME"` in fili-term.toml)
fn keymap(args: &[String]) -> i32 {
    let path = gui::theme::path();
    let text = std::fs::read_to_string(&path).unwrap_or_default();
    match args.first().map(String::as_str) {
        None => {
            println!("{}", gui::keys::current(&text));
            0
        }
        Some("list") => {
            println!("{}", gui::keys::PRESETS.join("\n"));
            0
        }
        Some(name) if gui::keys::PRESETS.contains(&name) => {
            if let Some(dir) = path.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            match std::fs::write(&path, gui::keys::with_keymap(&text, name)) {
                Ok(()) => 0,
                Err(e) => {
                    eprintln!("fili-term keymap: {}: {e}", path.display());
                    1
                }
            }
        }
        Some(other) => {
            eprintln!("fili-term keymap: unknown preset {other} (default, mac, tmux, none)");
            2
        }
    }
}

/// `fili-term mode KIND [--remote TARGET [--session NAME]]`: the integrated mode of the program in the window being used
/// (herdr: its tabs as tabs of the window, its workspaces and agents in the sidebar; its server is started when it does
/// not run). With --remote, the herdr server of that machine through SSH; the window gets "KIND|TARGET|SESSION".
fn mode(args: &[String]) -> i32 {
    let usage = || {
        eprintln!("usage: fili-term mode herdr [--remote SSH_TARGET [--session NAME]]");
        2
    };
    let (mut kind, mut remote, mut session) = (None, None, None);
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--remote" => remote = it.next().cloned(),
            "--session" => session = it.next().cloned(),
            k if kind.is_none() && !k.starts_with('-') => kind = Some(k.to_string()),
            _ => return usage(),
        }
    }
    let Some(kind) = kind else { return usage() };
    if session.is_some() && remote.is_none() {
        eprintln!("fili-term mode: --session needs --remote");
        return 2;
    }
    if let Some(r) = &remote {
        if !modes::herdr::valid_ssh_target(r) || r.contains(['|', ',']) {
            eprintln!("fili-term mode: not an SSH target: {r}");
            return 2;
        }
    }
    if session.as_ref().is_some_and(|s| s.is_empty() || s.contains(['|', ',']) || s.chars().any(|c| c.is_whitespace() || c.is_control())) {
        eprintln!("fili-term mode: not a session name");
        return 2;
    }
    let kind = &kind;
    if !gui::settings::MODES.contains(&kind.as_str()) {
        eprintln!("fili-term mode: no integration for {kind} (known: {})", gui::settings::MODES.join(", "));
        return 2;
    }
    let found = std::env::var_os("PATH").is_some_and(|p| std::env::split_paths(&p).any(|d| d.join(kind).is_file()));
    if !found {
        eprintln!("fili-term mode: {kind} is not installed (not in PATH)");
        return 1;
    }
    let spec = match remote {
        Some(r) => format!("{kind}|{r}|{}", session.unwrap_or_default()),
        None => kind.clone(),
    };
    open(&["--tab".into(), "--mode".into(), spec])
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
    let mut app_id: Option<String> = None;
    let mut title = String::new();
    let mut hold = false;
    let mut tab = false;
    let mut mode = String::new();
    // Nothing asked: the windows of the last run come back (`--new` skips that once)
    let plain = args.is_empty();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--new" => {}
            "-e" | "--" => {
                command = it.by_ref().cloned().collect();
            }
            "--attach" => attach = it.next().and_then(|v| v.parse().ok()).unwrap_or(0),
            "--hold" => hold = true,
            "--header" | "--no-header" | "--footer" | "--no-footer" => {}
            "--tab" => tab = true,
            "--mode" => mode = it.next().cloned().unwrap_or_default().replace([',', '\t'], ""),
            "-T" | "--title" => title = it.next().cloned().unwrap_or_default(),
            "--app-id" => app_id = it.next().cloned(),
            // foot's spelling, so a script can switch terminal by changing the program name only
            a if a.starts_with("--app-id=") => app_id = Some(a["--app-id=".len()..].to_string()),
            a if a.starts_with("--title=") => title = a["--title=".len()..].to_string(),
            _ => return usage(),
        }
    }
    if !ensure_daemon() {
        eprintln!("fili-term: cannot start the session daemon");
        return 1;
    }
    let cwd = std::env::current_dir().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
    // Request line for the window process: "open\t<attach>\t<cwd>\t<flags>\t<title>\t<argv…>" (tabs never appear in a cwd we accept)
    let clean = |t: &str| t.replace(['\t', '\n'], " ");
    let mode = (!mode.is_empty()).then(|| format!("mode={mode}"));
    let flags: Vec<&str> = [hold.then_some("hold"), tab.then_some("tab"), plain.then_some("plain"), mode.as_deref()].into_iter().flatten().collect();
    let mut request = format!("open\t{attach}\t{cwd}\t{}\t{}", flags.join(","), clean(&title));
    for a in &command {
        request.push('\t');
        request.push_str(&clean(a));
    }
    // A window process already running opens the window itself; a window with its own app-id needs a process of its own
    // (Qt sets one app-id per process)
    if app_id.is_none() {
        if let Ok(mut s) = UnixStream::connect(proto::runtime_dir().join("ui.sock")) {
            if s.write_all(format!("{request}\n").as_bytes()).is_ok() {
                return 0;
            }
        }
    }
    gui::run(&request, app_id.as_deref())
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
