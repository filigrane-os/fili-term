// Root of the window process: no window of its own, opens one TermWindow per request (Instance.openRequested).
import QtQuick
import Fili.Term

QtObject {
    id: root

    property var windows: []
    property FontLoader font: FontLoader { source: "qrc:/qt/qml/Fili/Term/fonts/JetBrainsMono-Variable.ttf" }
    property Component windowComponent: Component { TermWindow {} }
    property Connections requests: Connections {
        target: Instance
        function onOpenRequested(attach, cwd, command) {
            const w = root.windowComponent.createObject(root, { attachId: attach, cwd: cwd, command: command })
            if (!w) return
            root.windows = root.windows.concat([w])
            w.closed.connect(() => { root.windows = root.windows.filter(x => x !== w) })
        }
    }

    Component.onCompleted: Instance.start()
}
