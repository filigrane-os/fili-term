// Context menu (right click or Menu key): list with radius 6, rows of 24, hover surfaceHover, disabled entry in
// muted, separators, shortcut on the right. An entry can carry a submenu (children) that opens on the right.
// Arrows up/down and Enter, right opens the submenu, left closes it, Esc or a click elsewhere closes.
// items: [{ id, label, hint, enabled, separator, checked, children: [{ id, label }] }]; if one entry carries "checked",
// every row gets a gutter on the left for the check mark
import QtQuick
import Fili.Term

Item {
    id: menu
    property var items: []
    property int current: -1
    property int subCurrent: -1
    property bool subOpen: false
    signal activated(string id)
    visible: false
    anchors.fill: parent
    focus: visible
    z: 100

    readonly property int itemHeight: 24
    readonly property int menuWidth: 220

    function open(x, y, list) {
        items = list;
        current = -1;
        subOpen = false;
        panel.x = Math.max(FiliTheme.space1, Math.min(x, menu.width - menuWidth - FiliTheme.space1));
        // Height known in advance (the Repeater has not created its rows yet); too low: the menu opens upwards
        const h = list.reduce((a, i) => a + (i.separator ? 9 : menu.itemHeight), 0) + 2 * FiliTheme.space1;
        panel.y = Math.max(FiliTheme.space1, y + h > menu.height ? y - h : y);
        visible = true;
        forceActiveFocus();
    }
    function close() {
        visible = false;
    }
    function pick(item) {
        if (item.enabled === false || item.separator)
            return;
        if (item.children) {
            subOpen = true;
            subCurrent = 0;
            return;
        }
        close();
        activated(item.id);
    }
    function step(delta) {
        if (subOpen) {
            const n = items[current].children.length;
            subCurrent = (subCurrent + delta + n) % n;
            return;
        }
        let i = current;
        for (let k = 0; k < items.length; k++) {
            i = (i + delta + items.length) % items.length;
            if (!items[i].separator && items[i].enabled !== false)
                break;
        }
        current = i;
    }

    Keys.onPressed: e => {
        if (e.key === Qt.Key_Escape)
            close();
        else if (e.key === Qt.Key_Down)
            step(1);
        else if (e.key === Qt.Key_Up)
            step(-1);
        else if (e.key === Qt.Key_Right && current >= 0 && items[current].children)
            pick(items[current]);
        else if (e.key === Qt.Key_Left)
            subOpen = false;
        else if (e.key === Qt.Key_Return || e.key === Qt.Key_Enter) {
            if (subOpen)
                pick(items[current].children[subCurrent]);
            else if (current >= 0)
                pick(items[current]);
        } else
            return;
        e.accepted = true;
    }

    MouseArea {
        anchors.fill: parent
        acceptedButtons: Qt.AllButtons
        onPressed: menu.close()
    }

    component Panel: Rectangle {
        id: p
        property var model: []
        property int cur: -1
        signal hovered(int index)
        signal chosen(int index)
        width: menu.menuWidth
        height: col.height + 2 * FiliTheme.space1
        radius: FiliTheme.radiusControl + 2
        color: FiliTheme.surface
        border.width: FiliTheme.borderWidth
        border.color: FiliTheme.border
        // Gutter of the check marks (columns shown...)
        readonly property bool gutter: model.some(i => i.checked !== undefined)
        Column {
            id: col
            x: FiliTheme.space1
            y: FiliTheme.space1
            width: parent.width - 2 * FiliTheme.space1
            Repeater {
                model: p.model
                delegate: Item {
                    id: it
                    required property var modelData
                    required property int index
                    readonly property bool on: modelData.enabled !== false
                    width: col.width
                    height: modelData.separator ? 9 : menu.itemHeight
                    Rectangle {
                        visible: it.modelData.separator === true
                        anchors { left: parent.left; right: parent.right; verticalCenter: parent.verticalCenter; leftMargin: FiliTheme.space2; rightMargin: FiliTheme.space2 }
                        height: 1
                        color: FiliTheme.border
                    }
                    Rectangle {
                        visible: !it.modelData.separator
                        anchors.fill: parent
                        radius: FiliTheme.radiusControl
                        color: it.on && (p.cur === it.index) ? FiliTheme.surfaceHover : "transparent"
                        Text {
                            anchors { left: parent.left; leftMargin: FiliTheme.space2 + (p.gutter ? 18 : 0); right: hint.left; rightMargin: FiliTheme.space2; verticalCenter: parent.verticalCenter }
                            text: it.modelData.label || ""
                            elide: Text.ElideRight
                            font.family: FiliTheme.font
                            font.pixelSize: FiliTheme.size
                            color: it.on ? FiliTheme.text : FiliTheme.muted
                        }
                        Text {
                            visible: it.modelData.checked === true
                            anchors { left: parent.left; leftMargin: FiliTheme.space2; verticalCenter: parent.verticalCenter }
                            text: "✓"
                            font.family: FiliTheme.font
                            font.pixelSize: FiliTheme.size
                            color: it.on ? FiliTheme.accentColor : FiliTheme.muted
                        }
                        Text {
                            id: hint
                            anchors { right: parent.right; rightMargin: FiliTheme.space2; verticalCenter: parent.verticalCenter }
                            text: it.modelData.children ? "›" : (it.modelData.hint || "")
                            font.family: FiliTheme.font
                            font.pixelSize: FiliTheme.sizeSmall
                            color: FiliTheme.muted
                        }
                        MouseArea {
                            anchors.fill: parent
                            hoverEnabled: true
                            onEntered: p.hovered(it.index)
                            onClicked: p.chosen(it.index)
                        }
                    }
                }
            }
        }
    }

    Panel {
        id: panel
        model: menu.items
        cur: menu.current
        onHovered: i => {
            if (menu.items[i].enabled === false)
                return;
            menu.current = i;
            menu.subOpen = !!menu.items[i].children;
            menu.subCurrent = -1;
        }
        onChosen: i => menu.pick(menu.items[i])
    }
    Panel {
        id: sub
        visible: menu.subOpen && menu.current >= 0
        model: menu.subOpen && menu.current >= 0 ? menu.items[menu.current].children : []
        cur: menu.subCurrent
        x: Math.min(panel.x + panel.width - 2, menu.width - width - FiliTheme.space1)
        y: Math.min(panel.y + FiliTheme.space1 + menu.current * menu.itemHeight, menu.height - height - FiliTheme.space1)
        onHovered: i => menu.subCurrent = i
        onChosen: i => menu.pick(model[i])
    }
}
