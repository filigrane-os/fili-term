// A name typed in place (rename a tab, a workspace, a pane; a branch for a new worktree): a field over the item, like
// Casier's inline rename. Enter keeps the text, Escape or a click elsewhere cancels.
import QtQuick
import Fili.Term

Item {
    id: prompt
    // Called with the text when Enter is pressed
    property var accept: null
    visible: false
    anchors.fill: parent
    z: 110

    function open(x, y, width, title, text, done) {
        accept = done;
        caption.text = title;
        box.x = Math.max(FiliTheme.space1, Math.min(x, prompt.width - width - FiliTheme.space1));
        box.y = Math.max(FiliTheme.space1, Math.min(y, prompt.height - box.height - FiliTheme.space1));
        box.width = width;
        field.text = text;
        visible = true;
        field.forceActiveFocus();
        field.selectAll();
    }
    function close() {
        visible = false;
        accept = null;
    }

    MouseArea {
        anchors.fill: parent
        acceptedButtons: Qt.AllButtons
        onPressed: prompt.close()
    }
    Rectangle {
        id: box
        height: column.height + 2 * FiliTheme.space2
        radius: FiliTheme.radiusControl + 2
        color: FiliTheme.surface
        border.width: FiliTheme.borderWidth
        border.color: FiliTheme.border
        Column {
            id: column
            x: FiliTheme.space2
            y: FiliTheme.space2
            width: box.width - 2 * FiliTheme.space2
            spacing: FiliTheme.space1
            Text {
                id: caption
                width: parent.width
                elide: Text.ElideRight
                font.family: FiliTheme.font
                font.pixelSize: FiliTheme.sizeSmall
                color: FiliTheme.textSecondary
            }
            Rectangle {
                width: parent.width
                height: 26
                radius: FiliTheme.radiusControl
                color: FiliTheme.background
                border.width: FiliTheme.borderWidth
                border.color: FiliTheme.accentColor
                TextInput {
                    id: field
                    anchors { left: parent.left; right: parent.right; leftMargin: FiliTheme.space2; rightMargin: FiliTheme.space2; verticalCenter: parent.verticalCenter }
                    clip: true
                    selectByMouse: true
                    font.family: FiliTheme.font
                    font.pixelSize: FiliTheme.size
                    color: FiliTheme.text
                    selectionColor: FiliTheme.surfaceActive
                    selectedTextColor: FiliTheme.text
                    Accessible.name: caption.text
                    Keys.onPressed: e => {
                        if (e.key === Qt.Key_Return || e.key === Qt.Key_Enter) {
                            const done = prompt.accept;
                            const value = field.text;
                            prompt.close();
                            if (done)
                                done(value);
                            e.accepted = true;
                        } else if (e.key === Qt.Key_Escape) {
                            prompt.close();
                            e.accepted = true;
                        }
                    }
                }
            }
        }
    }
}
