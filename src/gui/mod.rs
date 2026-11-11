//! The window process: one Qt Quick application for every fili-term window.
pub mod bridge;

use cxx_qt_lib::{QGuiApplication, QQmlApplicationEngine, QString};

unsafe extern "C" {
    // src/gui/terminalview.cpp: TerminalView as `Fili.TermView 1.0`
    fn fili_term_register_types();
    // src/gui/instance.cpp: listens on ui.sock, opens the first window (request line, UTF-8, NUL-terminated)
    fn fili_term_start(engine: *mut std::ffi::c_void, request: *const std::ffi::c_char);
}

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
    let mut app = QGuiApplication::new();
    QGuiApplication::set_desktop_file_name(&QString::from("fili-term"));
    // SAFETY: registers QML types; the application object exists and no engine has loaded yet
    unsafe { fili_term_register_types() };
    let engine = QQmlApplicationEngine::new();
    if engine.is_null() {
        return 1;
    }
    let request = std::ffi::CString::new(request.replace('\0', "")).unwrap_or_default();
    // SAFETY: the engine lives until the end of this function, after the event loop
    unsafe { fili_term_start(engine.as_mut_ptr() as *mut std::ffi::c_void, request.as_ptr()) };
    match app.as_mut() {
        Some(app) => app.exec(),
        None => 1,
    }
}
