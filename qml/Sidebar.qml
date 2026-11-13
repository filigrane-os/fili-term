// Sidebar of an integrated mode (herdr: workspaces and agents), drawn from the mode's state (daemon/src/modes): foldable
// sections, a row per item with the status of its agents, the action to leave the mode at the bottom.
import QtQuick
import QtQuick.Window
import Fili.Term

Rectangle {
    id: side
    // State of the mode (JSON parsed), null without one
    property var mode: null
    signal activated(string section, string id)
    signal added(string section)
    signal leave()

    color: FiliTheme.backgroundDeep
    clip: true

    // Sections folded by the user, by key, for the life of the window
    property var folded: ({})
    function toggle(key) {
        const next = Object.assign({}, folded);
        next[key] = !folded[key];
        folded = next;
    }
    // Colour of an agent's status (herdr: idle, working, blocked, done, unknown)
    function statusColor(s) {
        return s === "blocked" ? FiliTheme.warning : s === "working" ? FiliTheme.accentColor : s === "done" ? FiliTheme.text : FiliTheme.muted;
    }
    function statusName(s) {
        return s === "blocked" ? Aeroport.tr("Waiting for you") : s === "working" ? Aeroport.tr("Working")
             : s === "done" ? Aeroport.tr("Done") : s === "idle" ? Aeroport.tr("Idle") : "";
    }

    Rectangle { anchors { top: parent.top; bottom: parent.bottom; right: parent.right } width: FiliTheme.borderWidth; color: FiliTheme.border }

    Flickable {
        id: flick
        anchors { fill: parent; topMargin: FiliTheme.space3; bottomMargin: leaveButton.height + FiliTheme.space3; rightMargin: FiliTheme.borderWidth }
        contentHeight: column.height
        clip: true
        boundsBehavior: Flickable.StopAtBounds
        KineticScroll {}
        Column {
            id: column
            width: flick.width
            // Not connected yet (the program starts, or its server is away)
            Text {
                visible: side.mode !== null && !side.mode.connected
                width: parent.width
                leftPadding: FiliTheme.margin / 2
                rightPadding: FiliTheme.space3
                topPadding: FiliTheme.space4
                text: side.mode ? Aeroport.tr(side.mode.message) : ""
                wrapMode: Text.Wrap
                font.family: FiliTheme.font
                font.pixelSize: FiliTheme.sizeSmall
                color: FiliTheme.muted
            }
            Repeater {
                model: side.mode ? side.mode.sections : []
                delegate: Column {
                    id: section
                    required property var modelData
                    readonly property bool open: !side.folded[modelData.key]
                    width: column.width
                    // Header: room above, fold arrow, title, optional + on the right
                    Rectangle {
                        width: parent.width
                        height: FiliTheme.sideRowHeight + FiliTheme.space4
                        color: head.containsMouse ? FiliTheme.alpha(FiliTheme.surfaceHover, 0.5) : "transparent"
                        Text {
                            id: arrow
                            x: FiliTheme.margin / 2
                            anchors { bottom: parent.bottom; bottomMargin: (FiliTheme.sideRowHeight - height) / 2 }
                            text: section.open ? "▾" : "▸"
                            font.pixelSize: FiliTheme.sizeSmall
                            color: FiliTheme.muted
                        }
                        Text {
                            anchors { left: arrow.right; leftMargin: FiliTheme.space2; right: add.visible ? add.left : parent.right; rightMargin: FiliTheme.space2; bottom: parent.bottom; bottomMargin: (FiliTheme.sideRowHeight - height) / 2 }
                            text: Aeroport.tr(section.modelData.title)
                            elide: Text.ElideRight
                            font.family: FiliTheme.font
                            font.pixelSize: FiliTheme.sizeSmall
                            font.weight: FiliTheme.weightAccent
                            color: FiliTheme.muted
                        }
                        MouseArea { id: head; anchors.fill: parent; hoverEnabled: true; onClicked: side.toggle(section.modelData.key) }
                        IconButton {
                            id: add
                            visible: section.modelData.add
                            anchors { right: parent.right; rightMargin: FiliTheme.space2; bottom: parent.bottom; bottomMargin: (FiliTheme.sideRowHeight - height) / 2 }
                            name: "plus"
                            tip: section.modelData.key === "workspaces" ? Aeroport.tr("New Workspace") : Aeroport.tr("New")
                            onClicked: side.added(section.modelData.key)
                        }
                    }
                    // An empty section says so (no agent found yet)
                    Text {
                        visible: section.open && section.modelData.items.length === 0
                        width: parent.width
                        height: FiliTheme.sideRowHeight
                        leftPadding: FiliTheme.margin / 2 + FiliTheme.space2
                        verticalAlignment: Text.AlignVCenter
                        text: section.modelData.key === "agents" ? Aeroport.tr("No agent running") : Aeroport.tr("None")
                        font.family: FiliTheme.font
                        font.pixelSize: FiliTheme.sizeSmall
                        color: FiliTheme.muted
                    }
                    Repeater {
                        model: section.open ? section.modelData.items : []
                        delegate: Rectangle {
                            id: row
                            required property var modelData
                            width: section.width
                            height: FiliTheme.sideRowHeight
                            color: modelData.active ? FiliTheme.surfaceHover : rowArea.containsMouse ? FiliTheme.alpha(FiliTheme.surfaceHover, 0.5) : "transparent"
                            // Status of the agents: a dot (hollow when idle); none when unknown
                            Rectangle {
                                id: dot
                                readonly property string status: row.modelData.status || ""
                                visible: status !== "" && status !== "unknown"
                                x: FiliTheme.margin / 2
                                anchors.verticalCenter: parent.verticalCenter
                                width: 8
                                height: 8
                                radius: 4
                                color: status === "idle" ? "transparent" : side.statusColor(status)
                                border.width: status === "idle" ? FiliTheme.borderWidth : 0
                                border.color: FiliTheme.muted
                                SequentialAnimation on opacity {
                                    running: dot.status === "working" && Window.window !== null && Window.window.active
                                    loops: Animation.Infinite
                                    onRunningChanged: if (!running) dot.opacity = 1
                                    NumberAnimation { to: 0.35; duration: 700; easing.type: Easing.InOutQuad }
                                    NumberAnimation { to: 1; duration: 700; easing.type: Easing.InOutQuad }
                                }
                            }
                            Text {
                                id: detail
                                anchors { right: parent.right; rightMargin: FiliTheme.space3; verticalCenter: parent.verticalCenter }
                                width: Math.min(implicitWidth, 72)
                                text: row.modelData.detail || ""
                                elide: Text.ElideRight
                                font.family: FiliTheme.font
                                font.pixelSize: FiliTheme.sizeSmall
                                color: FiliTheme.muted
                            }
                            Text {
                                anchors { left: parent.left; leftMargin: FiliTheme.margin / 2 + 8 + FiliTheme.space2; right: detail.left; rightMargin: FiliTheme.space2; verticalCenter: parent.verticalCenter }
                                text: row.modelData.label
                                elide: Text.ElideRight
                                font.family: FiliTheme.font
                                font.pixelSize: FiliTheme.size
                                font.weight: row.modelData.active ? FiliTheme.weightAccent : Font.Normal
                                color: FiliTheme.text
                            }
                            MouseArea { id: rowArea; anchors.fill: parent; hoverEnabled: true; onClicked: side.activated(section.modelData.key, row.modelData.id) }
                            Accessible.role: Accessible.ListItem
                            Accessible.name: row.modelData.label + (side.statusName(dot.status) ? ", " + side.statusName(dot.status) : "")
                        }
                    }
                }
            }
        }
    }
    FiliScrollBar {
        flick: flick
        anchors { top: flick.top; bottom: flick.bottom; right: flick.right }
    }
    // Leave the mode: the program goes on in the terminal, the window goes back to a plain terminal
    Text {
        id: leaveButton
        anchors { left: parent.left; bottom: parent.bottom; leftMargin: FiliTheme.margin / 2; bottomMargin: FiliTheme.space2 }
        height: FiliTheme.sideRowHeight
        verticalAlignment: Text.AlignVCenter
        text: Aeroport.tr("Stop Integration")
        font.family: FiliTheme.font
        font.pixelSize: FiliTheme.sizeSmall
        color: leaveArea.containsMouse ? FiliTheme.text : FiliTheme.textSecondary
        MouseArea { id: leaveArea; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: side.leave() }
        Accessible.role: Accessible.Button
        Accessible.name: text
    }
}
