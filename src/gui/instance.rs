//! The window process is one per user: it listens on `$XDG_RUNTIME_DIR/fili-term/ui.sock`, so a second `fili-term`
//! opens its window here instead of starting another Qt process. Main.qml creates the windows on `openRequested`.

use std::io::{BufRead, BufReader};
use std::os::unix::net::UnixListener;
use std::pin::Pin;
use std::sync::Mutex;

use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::{QString, QStringList};

/// Request of the command line that started this process, opened once Main.qml is ready
pub static FIRST_REQUEST: Mutex<Option<String>> = Mutex::new(None);

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
        #[qml_singleton]
        type Instance = super::InstanceRust;

        /// Open a window on a session (attach > 0) or on a new one (cwd, command)
        #[qsignal]
        #[cxx_name = "openRequested"]
        fn open_requested(self: Pin<&mut Instance>, attach: i32, cwd: QString, command: QStringList);

        /// Called by Main.qml once loaded: opens the first window and starts listening
        #[qinvokable]
        fn start(self: Pin<&mut Instance>);

        #[qinvokable]
        #[cxx_name = "newWindow"]
        fn new_window(self: Pin<&mut Instance>, cwd: &QString);
    }

    impl cxx_qt::Threading for Instance {}
}

#[derive(Default)]
pub struct InstanceRust {
    started: bool,
}

impl qobject::Instance {
    /// "open\t<attach id>\t<cwd>\t<argv…>"
    fn handle(self: Pin<&mut Self>, request: &str) {
        let mut parts = request.split('\t');
        if parts.next() != Some("open") {
            return;
        }
        let attach = parts.next().and_then(|v| v.parse().ok()).unwrap_or(0);
        let cwd = QString::from(parts.next().unwrap_or(""));
        let mut command = QStringList::default();
        for a in parts {
            command.append(QString::from(a));
        }
        self.open_requested(attach, cwd, command);
    }

    fn start(mut self: Pin<&mut Self>) {
        if self.started {
            return;
        }
        self.as_mut().rust_mut().started = true;
        let path = crate::proto::runtime_dir().join("ui.sock");
        let _ = std::fs::remove_file(&path);
        match UnixListener::bind(&path) {
            Ok(listener) => {
                let thread = self.qt_thread();
                std::thread::spawn(move || {
                    for stream in listener.incoming().flatten() {
                        for line in BufReader::new(stream).lines().map_while(Result::ok) {
                            if thread.queue(move |instance| instance.handle(&line)).is_err() {
                                return;
                            }
                        }
                    }
                });
            }
            Err(e) => eprintln!("fili-term: {}: {e}", path.display()),
        }
        let first = FIRST_REQUEST.lock().unwrap().take();
        if let Some(request) = first {
            self.handle(&request);
        }
    }

    fn new_window(self: Pin<&mut Self>, cwd: &QString) {
        let cwd = if cwd.is_empty() { std::env::var("HOME").unwrap_or_default() } else { cwd.to_string() };
        self.open_requested(0, QString::from(&cwd), QStringList::default());
    }
}
