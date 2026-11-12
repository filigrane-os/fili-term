//! The window process: one Qt Quick application for every fili-term window.
pub mod app;
pub mod i18n;
pub mod instance;
pub mod settings;
pub mod terminal;
pub mod theme;

use cxx_qt_lib::{QGuiApplication, QQmlApplicationEngine, QString, QUrl};

pub fn run(request: &str) -> i32 {
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
    QGuiApplication::set_desktop_file_name(&QString::from("fili-term"));
    let mut engine = QQmlApplicationEngine::new();
    if let Some(engine) = engine.as_mut() {
        engine.load(&QUrl::from("qrc:/qt/qml/Fili/Term/qml/App.qml"));
    }
    match app.as_mut() {
        Some(app) => app.exec(),
        None => 1,
    }
}
