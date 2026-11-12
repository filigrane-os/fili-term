// Root of the window process: no window of its own, opens one Main.qml window per request (Instance.openRequested)
// and hands the commands of the fili-bar menu to the active window.
import QtQuick
import QtQuick.Window
import Fili.Term

QtObject {
    id: root

    property var windows: []
    // Fonts embedded in the binary (fonts/, licenses/): same look on every distribution
    property FontLoader ui: FontLoader { source: "qrc:/qt/qml/Fili/Term/fonts/Jost-Variable.ttf" }
    property FontLoader mono: FontLoader { source: "qrc:/qt/qml/Fili/Term/fonts/JetBrainsMono-Variable.ttf" }
    property Component windowComponent: Component { Main {} }
    property Connections requests: Connections {
        target: Instance
        function onOpenRequested(attach, cwd, command) {
            const w = root.windowComponent.createObject(root, { attachId: attach, cwd: cwd, command: command });
            if (!w)
                return;
            root.windows = root.windows.concat([w]);
            w.closed.connect(() => { root.windows = root.windows.filter(x => x !== w) });
        }
    }
    // The bar acts on the focused window; the last one created when none has the focus (a command run from a script)
    property Connections commands: Connections {
        target: Aeroport
        function onCommand(name) {
            const w = root.windows.find(x => x.active) || root.windows[root.windows.length - 1];
            if (w)
                w.run(name);
        }
    }

    Component.onCompleted: Instance.start()
}
