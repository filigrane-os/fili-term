// Qt Quick window of fili-term (module Fili.Term, qrc:/qt/qml/Fili/Term/qml/); the terminal grid is a QQuickPaintedItem
// written in Rust (src/gui/terminal.rs) that talks to the session daemon over its Unix socket.
use cxx_qt_build::{CxxQtBuilder, QmlFile, QmlModule};

fn main() {
    // Version of the About card: tag or commit of the repository, else the version of Cargo.toml
    let git = std::process::Command::new("git")
        .args(["describe", "--tags", "--always", "--dirty"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());
    let version = git.unwrap_or_else(|| std::env::var("CARGO_PKG_VERSION").unwrap_or_default());
    println!("cargo:rustc-env=FILI_TERM_VERSION={version}");
    // HEAD moves on checkout, the refs on commit and tag
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-changed=.git/refs");
    println!("cargo:rerun-if-changed=.git/packed-refs");

    CxxQtBuilder::new_qml_module(
        QmlModule::new("Fili.Term")
            .qml_files([
                "qml/App.qml",
                "qml/Main.qml",
                "qml/AboutCard.qml",
                "qml/FiliScrollBar.qml",
                "qml/KineticScroll.qml",
                "qml/WindowGlyph.qml",
                "qml/WindowPanel.qml",
                "qml/FocusRing.qml",
                "qml/Icon.qml",
                "qml/IconButton.qml",
            ])
            .qml_file(QmlFile::from("qml/FiliTheme.qml").singleton(true))
            .depend("QtQuick"),
    )
    .qrc_resources([
        "fonts/JetBrainsMono-Variable.ttf",
        "fonts/Jost-Variable.ttf",
        // The icon of the application, shown by the About card
        "data/icons/hicolor/scalable/apps/fili-term.svg",
    ])
    .qt_module("Quick")
    .files(["src/gui/terminal.rs", "src/gui/instance.rs", "src/gui/app.rs"])
    .build();
    // cxx-qt names the Qt libraries before its generated archive; with --as-needed, ld drops Qt Quick before it sees
    // the item that needs it (Facteur and Casier escape this through their library crate)
    println!("cargo:rustc-link-arg-bins=-Wl,--no-as-needed");
    for lib in ["Qt6Quick", "Qt6Qml", "Qt6Gui", "Qt6Core"] {
        println!("cargo:rustc-link-arg-bins=-l{lib}");
    }
}
