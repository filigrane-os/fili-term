//! `Aeroport`, the QML singleton of the environment: theme (live light/dark mode and accent of fili-theme), settings,
//! translations, interface state, window actions of niri, About card, and the commands of the fili-bar menu.
#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        #[qproperty(bool, light, READ, NOTIFY = theme_changed)]
        #[qproperty(QString, accent, READ, NOTIFY = theme_changed)]
        type Aeroport = super::AeroportRust;

        #[qsignal]
        #[cxx_name = "themeChanged"]
        fn theme_changed(self: Pin<&mut Aeroport>);

        /// A command of the fili-bar menu (`fili-term ctl action NAME`), for the active window
        #[qsignal]
        fn command(self: Pin<&mut Aeroport>, name: &QString);

        /// Merged theme (defaults of theme.toml, theme.toml of Filigrane, fili-term.toml), as JSON
        #[qinvokable]
        #[cxx_name = "themeJson"]
        fn theme_json(self: &Aeroport) -> QString;
        /// Settings of fili-term.toml, as JSON (settings.rs)
        #[qinvokable]
        #[cxx_name = "settingsJson"]
        fn settings_json(self: &Aeroport) -> QString;
        /// Interface state kept between runs (~/.local/state/filigrane/term/ui, lines key=value)
        #[qinvokable]
        #[cxx_name = "uiGet"]
        fn ui_get(self: &Aeroport, key: &QString) -> QString;
        #[qinvokable]
        #[cxx_name = "uiSet"]
        fn ui_set(self: &Aeroport, key: &QString, value: &QString);
        /// Translation of a text written in English
        #[qinvokable]
        fn tr(self: &Aeroport, text: &QString) -> QString;
        /// Window management under niri (the window panel of the header)
        #[qinvokable]
        #[cxx_name = "windowControl"]
        fn window_control(self: &Aeroport) -> bool;
        #[qinvokable]
        #[cxx_name = "windowState"]
        fn window_state(self: &Aeroport) -> QString;
        #[qinvokable]
        #[cxx_name = "windowAction"]
        fn window_action(self: &Aeroport, name: &QString) -> bool;
        /// Rows of the About card: JSON [[label, value], ...]
        #[qinvokable]
        #[cxx_name = "aboutRows"]
        fn about_rows(self: &Aeroport) -> QString;
        /// `~` for the home directory at the start of a path
        #[qinvokable]
        #[cxx_name = "shortPath"]
        fn short_path(self: &Aeroport, path: &QString) -> QString;
    }

    impl cxx_qt::Initialize for Aeroport {}
    impl cxx_qt::Threading for Aeroport {}
}

use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::QString;
use std::{env, path::PathBuf, pin::Pin};

#[derive(Default)]
pub struct AeroportRust {
    light: bool,
    accent: QString,
}

fn home() -> String {
    env::var("HOME").unwrap_or_default()
}

fn state_dir() -> PathBuf {
    let state = env::var("XDG_STATE_HOME").unwrap_or_else(|_| format!("{}/.local/state", home()));
    PathBuf::from(state).join("filigrane")
}

fn ui_file() -> PathBuf {
    state_dir().join("term/ui")
}

/// State of fili-theme ("light" or "dark"), like FiliTheme.qml of the fili-shell; dark without it
fn read_light() -> bool {
    std::fs::read_to_string(state_dir().join("theme")).is_ok_and(|t| t.trim() == "light")
}

/// Accent colour (name), "default" by default
fn read_accent() -> String {
    std::fs::read_to_string(state_dir().join("accent")).map(|t| t.trim().to_string()).unwrap_or_else(|_| "default".into())
}

impl cxx_qt::Initialize for qobject::Aeroport {
    fn initialize(mut self: Pin<&mut Self>) {
        self.as_mut().rust_mut().light = read_light();
        self.as_mut().rust_mut().accent = QString::from(&read_accent());
        // Commands of the fili-bar menu
        let qt = self.qt_thread();
        crate::ctl::serve(move |name| {
            let _ = qt.queue(move |f| f.command(&QString::from(&name)));
        });
        // Follow fili-theme's state directory (inotify, thread asleep on read while nothing changes)
        let qt = self.qt_thread();
        std::thread::spawn(move || {
            let dir = state_dir();
            let _ = std::fs::create_dir_all(&dir);
            let Ok(c) = std::ffi::CString::new(dir.as_os_str().as_encoded_bytes()) else { return };
            // SAFETY: plain inotify calls on a descriptor owned by this thread
            let fd = unsafe { libc::inotify_init1(libc::IN_CLOEXEC) };
            if fd < 0 || unsafe { libc::inotify_add_watch(fd, c.as_ptr(), libc::IN_CLOSE_WRITE | libc::IN_MOVED_TO | libc::IN_CREATE) } < 0 {
                return;
            }
            let mut buf = [0u8; 4096];
            loop {
                // SAFETY: reads into a buffer of the stated length
                if unsafe { libc::read(fd, buf.as_mut_ptr().cast(), buf.len()) } <= 0 {
                    return;
                }
                let (light, accent) = (read_light(), read_accent());
                let sent = qt.queue(move |mut f| {
                    if f.light != light || f.accent.to_string() != accent {
                        f.as_mut().rust_mut().light = light;
                        f.as_mut().rust_mut().accent = QString::from(&accent);
                        f.as_mut().theme_changed();
                    }
                });
                if sent.is_err() {
                    return;
                }
            }
        });
    }
}

impl qobject::Aeroport {
    fn theme_json(&self) -> QString {
        QString::from(&super::theme::theme_json())
    }

    fn settings_json(&self) -> QString {
        QString::from(&super::settings::json())
    }

    fn ui_get(&self, key: &QString) -> QString {
        let key = format!("{key}=");
        let text = std::fs::read_to_string(ui_file()).unwrap_or_default();
        QString::from(text.lines().find_map(|l| l.strip_prefix(&key)).unwrap_or(""))
    }

    fn ui_set(&self, key: &QString, value: &QString) {
        let prefix = format!("{key}=");
        let path = ui_file();
        let old = std::fs::read_to_string(&path).unwrap_or_default();
        let mut lines: Vec<String> = old.lines().filter(|l| !l.starts_with(&prefix)).map(str::to_owned).collect();
        lines.push(format!("{prefix}{value}"));
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(&path, lines.join("\n") + "\n");
    }

    fn tr(&self, text: &QString) -> QString {
        QString::from(&super::i18n::tr(&text.to_string()))
    }

    fn window_control(&self) -> bool {
        crate::window::available()
    }

    fn window_state(&self) -> QString {
        QString::from(&crate::window::state())
    }

    fn window_action(&self, name: &QString) -> bool {
        crate::window::run(&name.to_string())
    }

    fn about_rows(&self) -> QString {
        let t = super::i18n::tr;
        let sessions = std::os::unix::net::UnixStream::connect(crate::proto::daemon_socket()).is_ok();
        let rows = [
            [t("Version"), env!("FILI_TERM_VERSION").to_string()],
            [t("Also known as"), "fili-term".into()],
            [t("Part of"), "Filigrane".into()],
            [t("Built with"), "Rust, Qt Quick, alacritty_terminal".into()],
            [t("Sessions"), if sessions { t("kept by fili-term-daemon") } else { t("no daemon") }],
            [t("Settings"), "~/.config/filigrane/fili-term.toml".into()],
        ];
        let json = |s: &str| serde_json::Value::from(s).to_string();
        let list: Vec<String> = rows.iter().map(|[a, b]| format!("[{},{}]", json(a), json(b))).collect();
        QString::from(&format!("[{}]", list.join(",")))
    }

    fn short_path(&self, path: &QString) -> QString {
        let (path, home) = (path.to_string(), home());
        match path.strip_prefix(&home) {
            Some(rest) if !home.is_empty() && (rest.is_empty() || rest.starts_with('/')) => QString::from(&format!("~{rest}")),
            _ => QString::from(&path),
        }
    }
}
