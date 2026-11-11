// Qt Quick window of fili-term (module Fili.Term, qrc:/qt/qml/Fili/Term/qml/); the terminal grid is a C++ item
// (src/gui/terminalview.*) that talks to the session daemon over its Unix socket.
use cxx_qt_build::{CxxQtBuilder, QmlModule};

fn main() {
    let builder = CxxQtBuilder::new_qml_module(QmlModule::new("Fili.Term").qml_files(["qml/TermWindow.qml"]).depend("QtQuick"))
        .qrc_resources(["fonts/JetBrainsMono-Variable.ttf"])
        .qt_module("Quick")
        .qt_module("Network")
        .files(["src/gui/bridge.rs"])
        .cpp_files(["src/gui/terminalview.h", "src/gui/terminalview.cpp", "src/gui/instance.h", "src/gui/instance.cpp"]);
    // SAFETY: only flags are added to cxx-qt's own C++ build
    let builder = unsafe {
        builder.cc_builder(|cc| {
            cc.flag_if_supported("-std=c++17").flag_if_supported("-O2");
        })
    };
    builder.build();
    // The C++ archive is linked after the Qt libraries cxx-qt names; without a library crate in between (Facteur and
    // Casier have one), ld drops Qt Quick before it sees the item that needs it
    println!("cargo:rustc-link-arg-bins=-Wl,--no-as-needed");
    for lib in ["Qt6Quick", "Qt6Qml", "Qt6Network", "Qt6Gui", "Qt6Core"] {
        println!("cargo:rustc-link-arg-bins=-l{lib}");
    }
}
