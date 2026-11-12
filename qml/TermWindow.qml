// V0 window: tabs of terminals, no Filigrane chrome yet (V1 brings the app-window anatomy and the bar menu).
import QtQuick
import QtQuick.Window
import Fili.Term

Window {
    id: win

    property int attachId: 0
    property string cwd: ""
    property var command: []
    property var tabs: []
    property int current: -1
    readonly property var view: current >= 0 && current < tabs.length ? tabs[current] : null

    width: 860
    height: 540
    visible: true
    color: "#1c222b"
    title: view && view.title ? view.title : "fili-term"

    function addTab(attach, dir, argv) {
        const t = termComponent.createObject(stack, { sessionId: attach, cwd: dir, command: argv || [] })
        tabs = tabs.concat([t])
        select(tabs.length - 1)
    }
    function select(i) {
        if (i < 0 || i >= tabs.length) return
        current = i
        for (let k = 0; k < tabs.length; ++k) {
            tabs[k].visible = k === i
            tabs[k].setShown(k === i)
        }
        tabs[i].forceActiveFocus()
    }
    function zoom(step) {
        for (const t of tabs) t.fontPixelSize = step === 0 ? 12 : t.fontPixelSize + step
    }
    function closeTab(t, hangUp) {
        const i = tabs.indexOf(t)
        if (i < 0) return
        if (hangUp) t.closeSession()
        tabs = tabs.filter(x => x !== t)
        t.destroy()
        if (tabs.length === 0) { win.close(); return }
        select(Math.min(i, tabs.length - 1))
    }

    Component.onCompleted: addTab(attachId, cwd, command)
    signal closed()
    onClosing: {
        for (const t of tabs) t.closeSession()
        win.closed()
        win.destroy()
    }

    Component {
        id: termComponent
        TerminalView {
            id: term
            anchors.fill: parent
            fontFamily: "JetBrains Mono"
            fontPixelSize: 12
            padding: 10
            onExited: win.closeTab(term, false)
            onBell: win.alert(0)
        }
    }

    Rectangle {
        id: tabBar
        visible: win.tabs.length > 1
        height: visible ? 28 : 0
        anchors { left: parent.left; right: parent.right; top: parent.top }
        color: "#161b22"
        Row {
            anchors.fill: parent
            Repeater {
                model: win.tabs.length
                Rectangle {
                    required property int index
                    width: Math.min(220, tabBar.width / win.tabs.length)
                    height: tabBar.height
                    color: index === win.current ? "#1c222b" : "transparent"
                    Text {
                        anchors.fill: parent
                        anchors.leftMargin: 10
                        anchors.rightMargin: 10
                        verticalAlignment: Text.AlignVCenter
                        elide: Text.ElideRight
                        color: index === win.current ? "#dbe7f4" : "#8a94a0"
                        font.pixelSize: 12
                        text: win.tabs[index] ? (win.tabs[index].title || "shell") : ""
                    }
                    MouseArea { anchors.fill: parent; onClicked: win.select(parent.index) }
                }
            }
        }
    }

    Item {
        id: stack
        anchors { left: parent.left; right: parent.right; top: tabBar.bottom; bottom: parent.bottom }
    }

    // Scrolled back in the history: where we are
    Rectangle {
        visible: win.view !== null && win.view.scrollOffset > 0
        anchors { right: parent.right; top: stack.top; margins: 8 }
        width: offsetLabel.implicitWidth + 16
        height: 22
        radius: 4
        color: "#2d3744"
        Text {
            id: offsetLabel
            anchors.centerIn: parent
            color: "#dbe7f4"
            font.pixelSize: 11
            text: win.view ? win.view.scrollOffset + " / " + win.view.historySize : ""
        }
    }

    Shortcut { sequence: "Ctrl+Shift+T"; onActivated: win.addTab(0, win.cwd, []) }
    Shortcut { sequence: "Ctrl+Shift+W"; onActivated: if (win.view) win.closeTab(win.view, true) }
    Shortcut { sequence: "Ctrl+Shift+N"; onActivated: Instance.newWindow(win.cwd) }
    Shortcut { sequence: "Ctrl+Shift+C"; onActivated: if (win.view) win.view.copy() }
    Shortcut { sequence: "Ctrl+Shift+V"; onActivated: if (win.view) win.view.paste() }
    Shortcut { sequence: "Ctrl+PgDown"; onActivated: win.select((win.current + 1) % win.tabs.length) }
    Shortcut { sequence: "Ctrl+PgUp"; onActivated: win.select((win.current - 1 + win.tabs.length) % win.tabs.length) }
    Shortcut { sequence: "Ctrl++"; onActivated: win.zoom(1) }
    Shortcut { sequence: "Ctrl+="; onActivated: win.zoom(1) }
    Shortcut { sequence: "Ctrl+-"; onActivated: win.zoom(-1) }
    Shortcut { sequence: "Ctrl+0"; onActivated: win.zoom(0) }
}
