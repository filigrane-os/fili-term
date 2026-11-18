// Text button (trash toolbar, status bar, confirmation): hover surfaceHover, pressed surfaceActive.
// primary: text in accentColor (default action).
import QtQuick
import Fili.Term

Rectangle {
    id: b
    property string label
    property bool primary: false
    signal clicked()

    width: text.implicitWidth + 2 * FiliTheme.space3
    height: 22
    radius: FiliTheme.radiusControl
    color: area.pressed ? FiliTheme.surfaceActive : area.containsMouse ? FiliTheme.surfaceHover : "transparent"
    border.width: primary ? 1 : 0
    border.color: FiliTheme.accentColor

    Text {
        id: text
        anchors.centerIn: parent
        text: b.label
        font.family: FiliTheme.font
        font.pixelSize: FiliTheme.sizeList
        color: b.primary ? FiliTheme.accentColor : FiliTheme.text
    }
    MouseArea {
        id: area
        anchors.fill: parent
        hoverEnabled: true
        onClicked: b.clicked()
    }
}
