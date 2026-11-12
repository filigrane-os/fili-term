// Qt Quick window of fili-term (module Fili.Term, qrc:/qt/qml/Fili/Term/qml/); the terminal grid is a QQuickPaintedItem
// written in Rust (src/gui/terminal.rs) that talks to the session daemon over its Unix socket.
use cxx_qt_build::{CxxQtBuilder, QmlModule};

fn main() {
    CxxQtBuilder::new_qml_module(QmlModule::new("Fili.Term").qml_files(["qml/Main.qml", "qml/TermWindow.qml"]).depend("QtQuick"))
        .qrc_resources(["fonts/JetBrainsMono-Variable.ttf"])
        .qt_module("Quick")
        .files(["src/gui/terminal.rs", "src/gui/instance.rs"])
        .build();
    // cxx-qt names the Qt libraries before its generated archive; with --as-needed, ld drops Qt Quick before it sees
    // the item that needs it (Facteur and Casier escape this through their library crate)
    println!("cargo:rustc-link-arg-bins=-Wl,--no-as-needed");
    for lib in ["Qt6Quick", "Qt6Qml", "Qt6Gui", "Qt6Core"] {
        println!("cargo:rustc-link-arg-bins=-l{lib}");
    }
}
