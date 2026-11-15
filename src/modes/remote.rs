//! A herdr server on another machine (`herdr --remote TARGET`), reached through SSH. herdr listens on two sockets there:
//! its API (herdr.sock) and the one of its clients (herdr-client.sock, which `herdr terminal attach` derives from the
//! API's name). One `ssh -N` forwards both to a private directory of this machine, under the same names, so the adapter,
//! `herdr pane read` and `herdr terminal attach` work as for a local server (HERDR_SOCKET_PATH). The link comes back on
//! its own when SSH drops, while the mode runs; it ends with the mode.
//!
//! SSH runs without a terminal (BatchMode): the key must be loaded in ssh-agent, as `herdr --remote` asks too.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// How the link goes: None while it connects or works, else a message to translate (%1 the machine, %2 the detail) and
/// its detail
pub type Link = Arc<Mutex<Option<(String, String)>>>;

pub struct Tunnel {
    dir: PathBuf,
    stop: Arc<AtomicBool>,
    child: Arc<Mutex<Option<Child>>>,
    pub link: Link,
}

impl Tunnel {
    pub fn start(ssh: &str, session: Option<&str>) -> std::io::Result<Tunnel> {
        let dir = private_dir()?;
        let tunnel = Tunnel { dir, stop: Arc::default(), child: Arc::default(), link: Arc::default() };
        let (ssh, session) = (ssh.to_string(), session.map(str::to_string));
        let (dir, stop, child, link) = (tunnel.dir.clone(), tunnel.stop.clone(), tunnel.child.clone(), tunnel.link.clone());
        std::thread::Builder::new()
            .name("herdr-remote".into())
            .spawn(move || run(&ssh, session.as_deref(), &dir, &stop, &child, &link))?;
        Ok(tunnel)
    }

    /// The API socket as seen from this machine
    pub fn socket(&self) -> PathBuf {
        self.dir.join("herdr.sock")
    }
}

impl Drop for Tunnel {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(mut c) = self.child.lock().ok().and_then(|mut c| c.take()) {
            let _ = c.kill();
            let _ = c.wait();
        }
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// $XDG_RUNTIME_DIR/fili-term/remote-PID-N, only for the user
fn private_dir() -> std::io::Result<PathBuf> {
    use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
    static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let base = std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from).unwrap_or_else(std::env::temp_dir).join("fili-term");
    std::fs::DirBuilder::new().recursive(true).mode(0o700).create(&base)?;
    let dir = base.join(format!("remote-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::SeqCst)));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::DirBuilder::new().mode(0o700).create(&dir)?;
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700))?;
    Ok(dir)
}

fn run(ssh: &str, session: Option<&str>, dir: &Path, stop: &AtomicBool, child: &Mutex<Option<Child>>, link: &Link) {
    let set = |v: Option<(&str, String)>| {
        if let Ok(mut l) = link.lock() {
            *l = v.map(|(m, d)| (m.to_string(), d));
        }
    };
    let pause = |secs: u64| {
        for _ in 0..secs * 10 {
            if stop.load(Ordering::SeqCst) {
                return;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    };
    while !stop.load(Ordering::SeqCst) {
        let remote = match remote_socket(ssh, session) {
            Ok(Some(p)) => p,
            Ok(None) => {
                // No server there (the client that started it left with it): start one, as `herdr --remote` does
                set(Some(("Starting herdr on %1…", String::new())));
                let _ = remote_command(ssh, &format!("{} server </dev/null >/dev/null 2>&1 &", herdr_call(session)));
                pause(2);
                continue;
            }
            Err(e) => {
                set(Some(("Cannot reach %1: %2", e)));
                pause(5);
                continue;
            }
        };
        let client = client_socket(&remote);
        let (local, local_client) = (dir.join("herdr.sock"), dir.join("herdr-client.sock"));
        let _ = std::fs::remove_file(&local);
        let _ = std::fs::remove_file(&local_client);
        let spawned = Command::new("ssh")
            .args(OPTIONS)
            .args(["-N", "-o", "ExitOnForwardFailure=yes", "-o", "StreamLocalBindUnlink=yes", "-o", "StreamLocalBindMask=0177"])
            .arg("-L")
            .arg(format!("{}:{}", local.display(), remote))
            .arg("-L")
            .arg(format!("{}:{}", local_client.display(), client))
            .arg("--")
            .arg(ssh)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn();
        let mut c = match spawned {
            Ok(c) => c,
            Err(e) => {
                set(Some(("Cannot reach %1: %2", e.to_string())));
                pause(5);
                continue;
            }
        };
        let err = c.stderr.take();
        if let Ok(mut slot) = child.lock() {
            *slot = Some(c);
        }
        set(None);
        // Until SSH ends (the link dropped, or the mode stopped and killed it)
        loop {
            if stop.load(Ordering::SeqCst) {
                return;
            }
            let ended = child.lock().ok().map(|mut s| s.as_mut().map(|c| c.try_wait().map(|r| r.is_some()).unwrap_or(true)).unwrap_or(true));
            if ended != Some(false) {
                break;
            }
            std::thread::sleep(Duration::from_millis(300));
        }
        if let Ok(mut slot) = child.lock() {
            slot.take();
        }
        let mut text = String::new();
        if let Some(mut e) = err {
            let _ = e.read_to_string(&mut text);
        }
        let detail = last_line(&text);
        eprintln!("fili-term: herdr on {ssh}: SSH ended {detail}");
        set(Some(("Lost the link to %1, connecting again…", detail)));
        pause(2);
    }
}

/// SSH without a prompt, quick to give up, watching the link
const OPTIONS: &[&str] = &["-o", "BatchMode=yes", "-o", "ConnectTimeout=10", "-o", "ServerAliveInterval=15", "-o", "ServerAliveCountMax=3"];

/// herdr on the other machine: the one its shell finds, else where `herdr --remote` installs it
fn herdr_call(session: Option<&str>) -> String {
    let session = session.map(|s| format!(" --session {}", super::herdr::shell_quote(s))).unwrap_or_default();
    format!("h=$(command -v herdr 2>/dev/null || echo \"$HOME/.local/bin/herdr\"); \"$h\"{session}")
}

fn remote_command(ssh: &str, script: &str) -> Result<String, String> {
    let out = Command::new("ssh")
        .args(OPTIONS)
        .arg("--")
        .arg(ssh)
        .arg(script)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| e.to_string())?;
    if !out.status.success() {
        let text = String::from_utf8_lossy(&out.stderr).into_owned();
        return Err(last_line(&text));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// The API socket of the server there (`herdr status server --json`); None when no server runs
fn remote_socket(ssh: &str, session: Option<&str>) -> Result<Option<String>, String> {
    let out = remote_command(ssh, &format!("{} status server --json", herdr_call(session)))?;
    let v: serde_json::Value = serde_json::from_str(out.trim()).map_err(|_| "herdr is not installed there".to_string())?;
    if v["running"] != true {
        return Ok(None);
    }
    v["socket"].as_str().map(|s| Some(s.to_string())).ok_or_else(|| "herdr gave no socket".to_string())
}

/// The clients' socket beside an API socket: herdr.sock → herdr-client.sock
fn client_socket(api: &str) -> String {
    match api.strip_suffix(".sock") {
        Some(stem) => format!("{stem}-client.sock"),
        None => format!("{api}-client"),
    }
}

fn last_line(text: &str) -> String {
    text.lines().map(str::trim).filter(|l| !l.is_empty()).last().unwrap_or("").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_socket_name() {
        assert_eq!(client_socket("/home/u/.config/herdr/herdr.sock"), "/home/u/.config/herdr/herdr-client.sock");
        assert_eq!(client_socket("/run/x"), "/run/x-client");
    }

    #[test]
    fn remote_call_quotes_the_session() {
        assert!(herdr_call(Some("my work")).ends_with("\"$h\" --session 'my work'"));
        assert!(herdr_call(None).ends_with("\"$h\""));
    }
}
