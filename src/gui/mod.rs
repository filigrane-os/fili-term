//! The window process: one Qt Quick application for every fili-term window.
pub mod agents;
pub mod app;
pub mod i18n;
pub mod instance;
pub mod keys;
pub mod mode;
pub mod settings;
pub mod terminal;
pub mod theme;
pub mod worktree;

use cxx_qt_lib::{QGuiApplication, QQmlApplicationEngine, QString, QUrl};

/// `app_id`: a window process of its own with that app-id; it does not take the shared socket of the window process
pub fn run(request: &str, app_id: Option<&str>) -> i32 {
    instance::STANDALONE.store(app_id.is_some(), std::sync::atomic::Ordering::Relaxed);
    // Static initializers of the Qt side; named here so the linker keeps them
    cxx_qt::init_crate!(cxx_qt_lib);
    cxx_qt::init_crate!(fili_term);
    cxx_qt::init_qml_module!("Fili.Term");
    // Software rendering like the rest of the shell: no GPU context, smaller memory
    if std::env::var_os("QT_QUICK_BACKEND").is_none() {
        // SAFETY: nothing else runs yet (single thread, before Qt starts)
        unsafe { std::env::set_var("QT_QUICK_BACKEND", "software") };
    }
    *instance::FIRST_REQUEST.lock().unwrap() = Some(request.to_string());
    let mut app = QGuiApplication::new();
    // Wayland app-id (niri rules, fili-term.desktop, menu of the fili-bar)
    QGuiApplication::set_desktop_file_name(&QString::from(app_id.unwrap_or("fili-term")));
    let mut engine = QQmlApplicationEngine::new();
    if let Some(engine) = engine.as_mut() {
        engine.load(&QUrl::from("qrc:/qt/qml/Fili/Term/qml/App.qml"));
    }
    let code = match app.as_mut() {
        Some(app) => app.exec(),
        None => 1,
    };
    if let Some(f) = app::context_file() {
        let _ = std::fs::remove_file(f);
    }
    code
}
