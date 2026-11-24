// Sidebar of an integrated mode (herdr: workspaces, agents, panes), drawn from the mode's state (src/modes): foldable
// sections, a row per item with the status of its agents (indented under its group, or a group heading), a choice on the
// right of a section heading (herdr's agents: grouped or by priority), a context menu on right click, the action to leave
// the mode at the bottom.
import QtQuick
import QtQuick.Window
import Fili.Term
import Fili.UI

Rectangle {
    id: side
    // State of the mode (JSON parsed), null without one
    property var mode: null
    // The way back to herdr's own interface is offered only in a mode (not for the workspaces of the window)
    property bool canLeave: true
    signal activated(string section, string id)
    signal added(string section)
    signal leave()
    // Right click on an item: its section, the item, where (window coordinates)
    signal contextRequested(string section, var item, real x, real y)
    // Click on the choice at the right of a section heading
    signal choiceClicked(string section)

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
        anchors { fill: parent; topMargin: FiliTheme.space3; bottomMargin: (leaveButton.visible ? leaveButton.height : 0) + FiliTheme.space3; rightMargin: FiliTheme.borderWidth }
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
                text: side.mode ? (side.mode.messageArgs || []).reduce((t, a, i) => t.replace("%" + (i + 1), a), Aeroport.tr(side.mode.message)) : ""
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
                            anchors { left: arrow.right; leftMargin: FiliTheme.space2; right: choice.visible ? choice.left : add.visible ? add.left : parent.right; rightMargin: FiliTheme.space2; bottom: parent.bottom; bottomMargin: (FiliTheme.sideRowHeight - height) / 2 }
                            text: Aeroport.tr(section.modelData.title)
                            elide: Text.ElideRight
                            font.family: FiliTheme.font
                            font.pixelSize: FiliTheme.sizeSmall
                            font.weight: FiliTheme.weightAccent
                            color: FiliTheme.muted
                        }
                        MouseArea { id: head; anchors.fill: parent; hoverEnabled: true; onClicked: side.toggle(section.modelData.key) }
                        // A choice of the section (herdr's agents: "Grouped" or "Priority"), a click changes it
                        Text {
                            id: choice
                            visible: (section.modelData.choice || "") !== ""
                            anchors { right: add.visible ? add.left : parent.right; rightMargin: FiliTheme.space3; bottom: parent.bottom; bottomMargin: (FiliTheme.sideRowHeight - height) / 2 }
                            text: section.modelData.choice || ""
                            font.family: FiliTheme.font
                            font.pixelSize: FiliTheme.sizeSmall
                            color: choiceArea.containsMouse ? FiliTheme.text : FiliTheme.muted
                            MouseArea { id: choiceArea; anchors { fill: parent; margins: -4 } hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: side.choiceClicked(section.modelData.key) }
                            Accessible.role: Accessible.Button
                            Accessible.name: text
                        }
                        FiliIconButton {
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
                            // A heading inside the section (a workspace above its agents): small, muted, not clickable
                            readonly property bool heading: modelData.heading === true
                            readonly property int indent: (modelData.indent || 0) * FiliTheme.space4
                            width: section.width
                            height: heading ? FiliTheme.sideRowHeight - FiliTheme.space2 : FiliTheme.sideRowHeight
                            color: heading ? "transparent" : modelData.active ? FiliTheme.surfaceHover : rowArea.containsMouse ? FiliTheme.alpha(FiliTheme.surfaceHover, 0.5) : "transparent"
                            // Status of the agents: a dot (hollow when idle); none when unknown
                            Rectangle {
                                id: dot
                                readonly property string status: row.modelData.status || ""
                                visible: !row.heading && status !== "" && status !== "unknown"
                                x: FiliTheme.margin / 2 + row.indent
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
                                visible: !row.heading
                                anchors { right: parent.right; rightMargin: FiliTheme.space3; verticalCenter: parent.verticalCenter }
                                width: Math.min(implicitWidth, 72)
                                text: row.modelData.detail || ""
                                elide: Text.ElideRight
                                font.family: FiliTheme.font
                                font.pixelSize: FiliTheme.sizeSmall
                                color: FiliTheme.muted
                            }
                            Text {
                                anchors { left: parent.left; leftMargin: FiliTheme.margin / 2 + row.indent + (row.heading ? 0 : 8 + FiliTheme.space2); right: detail.left; rightMargin: FiliTheme.space2; verticalCenter: parent.verticalCenter }
                                text: row.modelData.label
                                elide: Text.ElideRight
                                font.family: FiliTheme.font
                                font.pixelSize: row.heading ? FiliTheme.sizeSmall : FiliTheme.size
                                font.weight: row.modelData.active && !row.heading ? FiliTheme.weightAccent : Font.Normal
                                color: row.heading ? FiliTheme.muted : FiliTheme.text
                            }
                            MouseArea {
                                id: rowArea
                                anchors.fill: parent
                                enabled: !row.heading
                                hoverEnabled: true
                                acceptedButtons: Qt.LeftButton | Qt.RightButton
                                onClicked: mouse => {
                                    if (mouse.button === Qt.RightButton) {
                                        const p = rowArea.mapToItem(null, mouse.x, mouse.y);
                                        side.contextRequested(section.modelData.key, row.modelData, p.x, p.y);
                                    } else {
                                        side.activated(section.modelData.key, row.modelData.id);
                                    }
                                }
                            }
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
    // Leave the mode: herdr's own interface opens in a tab, for the same server
    Text {
        id: leaveButton
        visible: side.canLeave
        anchors { left: parent.left; bottom: parent.bottom; leftMargin: FiliTheme.margin / 2; bottomMargin: FiliTheme.space2 }
        height: FiliTheme.sideRowHeight
        verticalAlignment: Text.AlignVCenter
        text: Aeroport.tr("Use herdr's Interface")
        font.family: FiliTheme.font
        font.pixelSize: FiliTheme.sizeSmall
        color: leaveArea.containsMouse ? FiliTheme.text : FiliTheme.textSecondary
        MouseArea { id: leaveArea; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: side.leave() }
        Accessible.role: Accessible.Button
        Accessible.name: text
    }
}
