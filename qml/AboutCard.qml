// "About Aéroport" card: Aéroport is the display name of fili-term (an alias); fili-term stays the program name.
import QtQuick
import Fili.Term

Rectangle {
    id: card
    property var rows: []
    z: 210
    visible: false
    color: FiliTheme.alpha(FiliTheme.backgroundDeep, 0.7)
    focus: visible

    function show() {
        rows = JSON.parse(Aeroport.aboutRows());
        visible = true;
    }
    onVisibleChanged: if (visible) forceActiveFocus()
    Keys.onPressed: e => {
        if (e.key === Qt.Key_Return || e.key === Qt.Key_Enter || e.key === Qt.Key_Escape) {
            visible = false;
            e.accepted = true;
        }
    }
    MouseArea { anchors.fill: parent; onClicked: card.visible = false }

    Rectangle {
        anchors.centerIn: parent
        width: 380
        height: column.implicitHeight + 2 * FiliTheme.space5
        radius: FiliTheme.radiusWindow
        color: FiliTheme.surface
        border.width: FiliTheme.borderWidth
        border.color: FiliTheme.border
        MouseArea { anchors.fill: parent }
        Column {
            id: column
            anchors { left: parent.left; right: parent.right; top: parent.top; margins: FiliTheme.space5 }
            spacing: FiliTheme.space3
            Image {
                anchors.horizontalCenter: parent.horizontalCenter
                width: 96
                height: 96
                source: "qrc:/qt/qml/Fili/Term/data/icons/hicolor/scalable/apps/fili-term.svg"
                sourceSize.width: 192
                sourceSize.height: 192
                fillMode: Image.PreserveAspectFit
            }
            Text {
                anchors.horizontalCenter: parent.horizontalCenter
                text: "Aéroport"
                font.family: FiliTheme.font
                font.pixelSize: FiliTheme.sizeTitle
                font.weight: FiliTheme.weightAccent
                color: FiliTheme.text
            }
            Text {
                anchors.horizontalCenter: parent.horizontalCenter
                text: Aeroport.tr("The terminal of Filigrane")
                font.family: FiliTheme.font
                font.pixelSize: FiliTheme.size
                font.weight: Font.Light
                color: FiliTheme.textSecondary
            }
            Rectangle { width: parent.width; height: 1; color: FiliTheme.surfaceActive }
            Grid {
                id: grid
                width: parent.width
                columns: 2
                columnSpacing: FiliTheme.space3
                rowSpacing: FiliTheme.space2
                Repeater {
                    model: card.rows.length * 2
                    Text {
                        readonly property bool label: index % 2 === 0
                        width: label ? 110 : grid.width - 110 - FiliTheme.space3
                        horizontalAlignment: label ? Text.AlignRight : Text.AlignLeft
                        wrapMode: Text.WrapAnywhere
                        text: card.rows[Math.floor(index / 2)][label ? 0 : 1]
                        textFormat: Text.PlainText
                        font.family: FiliTheme.font
                        font.pixelSize: FiliTheme.sizeList
                        font.weight: label ? FiliTheme.weightAccent : Font.Normal
                        color: label ? FiliTheme.textSecondary : FiliTheme.text
                    }
                }
            }
        }
    }
}
