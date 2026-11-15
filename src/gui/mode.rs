//! `ModeClient`: the integrated mode of a window (src/modes), for Main.qml. One per window; its adapter thread runs only
//! while the mode is on.
#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
        include!("cxx-qt-lib/qstringlist.h");
        type QStringList = cxx_qt_lib::QStringList;
    }

    extern "RustQt" {
        #[qobject]
        #[qml_element]
        /// State of the mode (JSON of src/modes), "" while it is off
        #[qproperty(QString, state)]
        #[qproperty(QString, kind)]
        type ModeClient = super::ModeClientRust;

        /// The program asks for attention (JSON {kind, agent, place})
        #[qsignal]
        fn notice(self: Pin<&mut ModeClient>, notice: &QString);
        /// Answer to an action (JSON {op, …}; op "error": {message})
        #[qsignal]
        fn reply(self: Pin<&mut ModeClient>, reply: &QString);

        /// Start the mode of `kind` ("herdr"). `pid`: a client of the program started by hand in a tab (its socket is
        /// the one to use; it is then ended, its tab goes back to the shell), 0 for none. False when the program cannot
        /// be integrated (a client of another machine).
        #[qinvokable]
        fn start(self: Pin<&mut ModeClient>, kind: &QString, pid: i32) -> bool;
        #[qinvokable]
        fn stop(self: Pin<&mut ModeClient>);
        /// An action (JSON of src/modes)
        #[qinvokable]
        fn act(self: Pin<&mut ModeClient>, action: &QString);
        /// Command of a tab showing a terminal of the program (herdr: `herdr terminal attach TERMINAL`)
        /// herdr's key bindings, JSON [{action, keys}] (its config's [keys] over its defaults)
        #[qinvokable]
        #[cxx_name = "keysJson"]
        fn keys_json(self: &ModeClient) -> QString;
        /// herdr's config file (Settings of herdr)
        #[qinvokable]
        #[cxx_name = "configPath"]
        fn config_path(self: &ModeClient) -> QString;
        /// Modifiers (Qt mask) that send a right click to the program instead of the menu, from herdr's
        /// `[ui] right_click_passthrough_modifier` (0: none, herdr's default)
        /// herdr's custom commands ([[keys.command]]), JSON [{key, type, command, width, height}]
        #[qinvokable]
        #[cxx_name = "commandsJson"]
        fn commands_json(self: &ModeClient) -> QString;
        /// A command of herdr's environment (its socket, the pane and workspace it acts on), run by sh: for a popup tab
        #[qinvokable]
        #[cxx_name = "commandLine"]
        fn command_line(self: &ModeClient, command: &QString, pane: &QString, workspace: &QString) -> QStringList;
        /// The same, detached in the background (custom command of type "shell")
        #[qinvokable]
        #[cxx_name = "runShell"]
        fn run_shell(self: &ModeClient, command: &QString, cwd: &QString, pane: &QString, workspace: &QString);
        #[qinvokable]
        #[cxx_name = "rightClickPass"]
        fn right_click_pass(self: &ModeClient) -> i32;
        #[qinvokable]
        #[cxx_name = "attachCommand"]
        fn attach_command(self: &ModeClient, terminal: &QString, pane: &QString) -> QStringList;
    }

    impl cxx_qt::Threading for ModeClient {}
}

use std::path::PathBuf;
use std::pin::Pin;

use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::{QString, QStringList};

use crate::modes::{self, Mode, Out, herdr};

#[derive(Default)]
pub struct ModeClientRust {
    state: QString,
    kind: QString,
    mode: Option<Mode>,
    socket: PathBuf,
    /// The program's executable (that of the client replaced, else "herdr" from PATH)
    program: String,
}

impl qobject::ModeClient {
    fn start(mut self: Pin<&mut Self>, kind: &QString, pid: i32) -> bool {
        let kind = kind.to_string();
        if modes::KINDS.iter().all(|k| *k != kind) {
            return false;
        }
        let pid = (pid > 0).then_some(pid as u32);
        let Some(socket) = herdr::socket_for(pid) else { return false };
        // The client's own binary (a release binary keeps its name, herdr-linux-x86_64); not a wrapper script's shell
        let program = pid
            .and_then(|p| std::fs::read_link(format!("/proc/{p}/exe")).ok())
            .filter(|p| p.file_name().is_some_and(|n| n.to_string_lossy().starts_with(kind.as_str())))
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|| kind.clone());
        if let Some(p) = pid {
            // The full client leaves (its server and panes go on): the window draws herdr's tabs and sidebar instead
            // SAFETY: plain signal to a process of the user
            unsafe { libc::kill(p as i32, libc::SIGTERM) };
        } else if std::os::unix::net::UnixStream::connect(&socket).is_err() {
            start_server(&program, &socket);
        }
        let thread = self.qt_thread();
        let out = Box::new(move |o: Out| {
            let _ = thread.queue(move |mut this: Pin<&mut qobject::ModeClient>| match o {
                Out::State(s) => {
                    // A late state of a mode just stopped must not bring it back
                    if this.mode.is_some() {
                        this.as_mut().set_state(QString::from(&s));
                    }
                }
                Out::Notice(n) => this.notice(&QString::from(&n)),
                Out::Reply(r) => this.reply(&QString::from(&r)),
            });
        });
        let mode = Mode::start(&kind, socket.clone(), out);
        let ok = mode.is_some();
        {
            let mut r = self.as_mut().rust_mut();
            r.mode = mode;
            r.socket = socket;
            r.program = program;
        }
        if ok {
            self.as_mut().set_kind(QString::from(&kind));
        }
        ok
    }

    fn stop(mut self: Pin<&mut Self>) {
        self.as_mut().rust_mut().mode = None;
        self.as_mut().set_state(QString::default());
        self.as_mut().set_kind(QString::default());
    }

    fn act(self: Pin<&mut Self>, action: &QString) {
        if let (Some(mode), Ok(v)) = (self.mode.as_ref(), serde_json::from_str(&action.to_string())) {
            mode.action(v);
        }
    }

    fn keys_json(&self) -> QString {
        QString::from(&herdr::keys_json(&std::fs::read_to_string(herdr::config_path()).unwrap_or_default()))
    }

    fn commands_json(&self) -> QString {
        let cfg = std::fs::read_to_string(herdr::config_path()).unwrap_or_default();
        QString::from(&serde_json::Value::Array(herdr::commands(&cfg)).to_string())
    }

    fn command_line(&self, command: &QString, pane: &QString, workspace: &QString) -> QStringList {
        let mut l = QStringList::default();
        for a in [
            "env".to_string(),
            format!("HERDR_SOCKET_PATH={}", self.socket.display()),
            format!("HERDR_PANE_ID={pane}"),
            format!("HERDR_WORKSPACE_ID={workspace}"),
            "sh".into(),
            "-c".into(),
            command.to_string(),
        ] {
            l.append(QString::from(&a));
        }
        l
    }

    fn run_shell(&self, command: &QString, cwd: &QString, pane: &QString, workspace: &QString) {
        use std::os::unix::process::CommandExt;
        let mut cmd = std::process::Command::new("sh");
        cmd.arg("-c")
            .arg(command.to_string())
            .env("HERDR_SOCKET_PATH", &self.socket)
            .env("HERDR_PANE_ID", pane.to_string())
            .env("HERDR_WORKSPACE_ID", workspace.to_string())
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        let dir = cwd.to_string();
        if !dir.is_empty() && std::path::Path::new(&dir).is_dir() {
            cmd.current_dir(dir);
        }
        // SAFETY: setsid is async-signal-safe; the command outlives the window
        unsafe {
            cmd.pre_exec(|| {
                libc::setsid();
                Ok(())
            });
        }
        if let Ok(mut child) = cmd.spawn() {
            // Reaped in a thread: no zombie while the window lives
            std::thread::spawn(move || child.wait());
        }
    }

    fn right_click_pass(&self) -> i32 {
        herdr::right_click_pass(&std::fs::read_to_string(herdr::config_path()).unwrap_or_default())
    }

    fn config_path(&self) -> QString {
        QString::from(&herdr::config_path().to_string_lossy().into_owned())
    }

    /// `herdr terminal attach` gives the screen only: the pane's recent output is printed first, so that its older
    /// lines land in the history of the terminal, then the attachment draws the screen over the last ones
    fn attach_command(&self, terminal: &QString, pane: &QString) -> QStringList {
        let q = |s: &str| format!("'{}'", s.replace('\'', "'\\''"));
        let program = q(&self.program);
        let script = format!(
            "rows=$(stty size 2>/dev/null | cut -d' ' -f1); {program} pane read {} --source recent --lines {HISTORY_LINES} --ansi 2>/dev/null | head -n -\"${{rows:-0}}\"; exec {program} terminal attach {}",
            q(&pane.to_string()),
            q(&terminal.to_string())
        );
        let mut l = QStringList::default();
        for a in [
            "env".to_string(),
            format!("HERDR_SOCKET_PATH={}", self.socket.display()),
            "sh".into(),
            "-c".into(),
            script,
        ] {
            l.append(QString::from(&a));
        }
        l
    }
}

/// Lines of a herdr pane's history brought into Aéroport when it attaches (herdr keeps its own limit too)
const HISTORY_LINES: u32 = 10_000;

/// `herdr server` in the background, detached from the window (it outlives it, like when herdr starts it itself)
fn start_server(program: &str, socket: &std::path::Path) {
    use std::os::unix::process::CommandExt;
    let mut cmd = std::process::Command::new(program);
    cmd.arg("server")
        .env("HERDR_SOCKET_PATH", socket)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    // SAFETY: setsid is async-signal-safe
    unsafe {
        cmd.pre_exec(|| {
            libc::setsid();
            Ok(())
        });
    }
    if let Err(e) = cmd.spawn() {
        eprintln!("fili-term: {program} server: {e}");
    }
}
