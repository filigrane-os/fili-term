//! Control of the open windows: `fili-term ctl [--pid PID] action NAME` runs a menu command in the active window of
//! that process (one process holds every window). The fili-bar menu (appmenu/fili-term.json) calls this instead of
//! simulating key presses. One socket per window process: $XDG_RUNTIME_DIR/fili-term/ctl/<pid>.sock, one command per
//! connection ("action<TAB>NAME"), answered by a JSON line {"ok":true} or {"ok":false,"error":"..."}.
use std::{
    fs,
    io::{Read, Write},
    os::unix::net::{UnixListener, UnixStream},
    path::{Path, PathBuf},
    time::Duration,
};

/// The commands of the window (menu entries); the interface runs them by name.
pub const ACTIONS: &[&str] = &[
    "about", "new-tab", "new-window", "close-tab", "close-window", "copy", "paste", "next-tab", "previous-tab", "zoom-in",
    "zoom-out", "zoom-reset", "page-up", "page-down", "scroll-top", "scroll-bottom", "window-actions",
    "toggle-window-buttons", "previous-command", "next-command", "copy-last-output", "copy-last-command",
    "find", "find-next", "find-previous", "show-in-files",
    "tab-1", "tab-2", "tab-3", "tab-4", "tab-5", "tab-6", "tab-7", "tab-8", "tab-9", "edit-output",
    "ask-rob", "integrate", "toggle-sidebar", "split-right", "split-down", "close-pane", "zoom-pane", "focus-left",
    "focus-right", "focus-up", "focus-down", "resize-left", "resize-right", "resize-up", "resize-down", "context-menu",
    "rename-pane", "resize-mode", "edit-scrollback", "next-agent",
];

fn dir() -> Option<PathBuf> {
    // Its own directory: the sweep below removes every socket not named after a living pid
    let dir = crate::proto::runtime_dir().join("ctl");
    fs::create_dir_all(&dir).ok()?;
    Some(dir)
}

fn answer(request: &str) -> (String, Option<String>) {
    let (verb, name) = request.trim().split_once('\t').unwrap_or((request.trim(), ""));
    if verb == "report" {
        // report<TAB>SESSION<TAB>STATE<TAB>AGENT: for the window holding that session (agents' hooks)
        let parts: Vec<&str> = name.split('\t').collect();
        return match parts.as_slice() {
            [session, state, rest @ ..]
                if session.parse::<u32>().is_ok() && crate::gui::agents::State::parse(state).is_some() && rest.len() <= 1 =>
            {
                let agent: String = rest.first().unwrap_or(&"").chars().filter(|c| c.is_ascii_alphanumeric() || "._-".contains(*c)).take(40).collect();
                ("{\"ok\":true}".into(), Some(format!("report\t{session}\t{}\t{agent}", state.trim().to_ascii_lowercase())))
            }
            _ => ("{\"ok\":false,\"error\":\"usage: report SESSION STATE [AGENT]\"}".into(), None),
        };
    }
    if verb == "action" && ACTIONS.contains(&name) {
        ("{\"ok\":true}".into(), Some(name.to_owned()))
    } else if verb == "action" {
        (format!("{{\"ok\":false,\"error\":\"unknown action: {}\"}}", name.replace(['"', '\\'], "")), None)
    } else {
        ("{\"ok\":false,\"error\":\"unknown command\"}".into(), None)
    }
}

/// Listen for commands in a thread (asleep on accept); `on_action` gets each valid action name.
pub fn serve(on_action: impl Fn(String) + Send + 'static) {
    let Some(dir) = dir() else { return };
    // Sockets of windows that are gone are never removed by their owner: sweep them (a reused pid must not match one)
    for e in fs::read_dir(&dir).into_iter().flatten().flatten() {
        let path = e.path();
        if path.extension().is_some_and(|x| x == "sock") && !alive(&path.file_stem().unwrap_or_default().to_string_lossy()) {
            let _ = fs::remove_file(path);
        }
    }
    let sock = dir.join(format!("{}.sock", std::process::id()));
    let _ = fs::remove_file(&sock);
    let Ok(listener) = UnixListener::bind(&sock) else { return };
    let _ = fs::set_permissions(&sock, std::os::unix::fs::PermissionsExt::from_mode(0o600));
    std::thread::spawn(move || {
        for mut c in listener.incoming().flatten() {
            let _ = c.set_read_timeout(Some(Duration::from_secs(2)));
            let mut text = String::new();
            let _ = (&mut c).take(4096).read_to_string(&mut text);
            let (reply, action) = answer(&text);
            if let Some(name) = action {
                on_action(name);
            }
            let _ = c.write_all(format!("{reply}\n").as_bytes());
        }
    });
}

fn alive(pid: &str) -> bool {
    Path::new(&format!("/proc/{pid}")).is_dir()
}

/// The socket of the requested pid, otherwise of the most recently started window.
fn target(pid: Option<&str>) -> Option<PathBuf> {
    let dir = dir()?;
    if let Some(pid) = pid.filter(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit())) {
        let s = dir.join(format!("{pid}.sock"));
        return (alive(pid) && s.exists()).then_some(s);
    }
    let mut socks: Vec<_> = fs::read_dir(&dir)
        .ok()?
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "sock"))
        .filter(|e| alive(&e.path().file_stem().unwrap_or_default().to_string_lossy()))
        .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path())))
        .collect();
    socks.sort();
    socks.pop().map(|(_, p)| p)
}

/// `fili-term report [--session ID] STATE [--agent NAME]` (an agent's hook): tell every window process; the one that
/// holds the session shows the state. Silent and successful outside Aéroport, so a hook set for every terminal is harmless.
pub fn report(args: &[String]) -> Result<(), String> {
    let mut session: Option<u32> = std::env::var("FILI_TERM_SESSION").ok().and_then(|v| v.parse().ok());
    let (mut state, mut agent) = (None, String::new());
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--session" => session = Some(it.next().and_then(|v| v.parse().ok()).ok_or("--session needs a number")?),
            "--agent" => agent = it.next().cloned().ok_or("--agent needs a name")?,
            s if state.is_none() && crate::gui::agents::State::parse(s).is_some() => state = Some(s.to_string()),
            other => return Err(format!("unknown state or option: {other} (working, blocked, done, idle)")),
        }
    }
    let state = state.ok_or("usage: fili-term report [--session ID] working|blocked|done|idle [--agent NAME]")?;
    let Some(session) = session else { return Ok(()) };
    let Some(dir) = dir() else { return Ok(()) };
    for e in fs::read_dir(&dir).into_iter().flatten().flatten() {
        let path = e.path();
        if path.extension().is_none_or(|x| x != "sock") || !alive(&path.file_stem().unwrap_or_default().to_string_lossy()) {
            continue;
        }
        if let Ok(mut c) = UnixStream::connect(&path) {
            c.set_read_timeout(Some(Duration::from_millis(500))).ok();
            let _ = c.write_all(format!("report\t{session}\t{state}\t{agent}\n").as_bytes());
            c.shutdown(std::net::Shutdown::Write).ok();
            let _ = c.read_to_string(&mut String::new());
        }
    }
    Ok(())
}

const USAGE: &str = "usage: fili-term ctl [--pid PID] action NAME | actions";

/// `fili-term ctl ...` (arguments after `ctl`): prints the JSON answer; Err carries the reason.
pub fn run(args: &[String]) -> Result<String, String> {
    let (pid, rest) = match args {
        [flag, pid, rest @ ..] if flag == "--pid" => (Some(pid.as_str()), rest),
        _ => (None, args),
    };
    match rest {
        [verb] if verb == "actions" => Ok(format!("[{}]", ACTIONS.iter().map(|a| format!("\"{a}\"")).collect::<Vec<_>>().join(","))),
        [verb, name] if verb == "action" => {
            let sock = target(pid).ok_or("no fili-term window")?;
            let mut c = UnixStream::connect(sock).map_err(|e| e.to_string())?;
            c.set_read_timeout(Some(Duration::from_secs(5))).ok();
            c.write_all(format!("action\t{name}\n").as_bytes()).map_err(|e| e.to_string())?;
            c.shutdown(std::net::Shutdown::Write).ok();
            let mut reply = String::new();
            let _ = c.read_to_string(&mut reply);
            let reply = reply.trim().to_owned();
            if reply.starts_with("{\"ok\":true") { Ok(reply) } else { Err(reply) }
        }
        _ => Err(USAGE.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_action_is_forwarded() {
        let (reply, action) = answer("action\tnew-tab\n");
        assert_eq!(reply, "{\"ok\":true}");
        assert_eq!(action.as_deref(), Some("new-tab"));
    }

    #[test]
    fn unknown_action_is_refused() {
        let (reply, action) = answer("action\trm -rf\n");
        assert!(reply.contains("\"ok\":false"));
        assert!(action.is_none());
        assert!(answer("nope").0.contains("unknown command"));
    }

    #[test]
    fn reports_of_agents() {
        let (reply, action) = answer("report\t12\tBlocked\tclaude\n");
        assert_eq!(reply, "{\"ok\":true}");
        assert_eq!(action.as_deref(), Some("report\t12\tblocked\tclaude"));
        assert_eq!(answer("report\t12\tdone\t\n").1.as_deref(), Some("report\t12\tdone\t"));
        assert_eq!(answer("report\t12\tdone\tx;rm -rf\n").1.as_deref(), Some("report\t12\tdone\txrm-rf"));
        assert!(answer("report\tx\tdone\n").1.is_none());
        assert!(answer("report\t12\tnonsense\n").1.is_none());
    }

    #[test]
    fn usage_without_a_command() {
        assert!(run(&[]).is_err());
        assert!(run(&["actions".into()]).unwrap().contains("close-tab"));
    }
}
