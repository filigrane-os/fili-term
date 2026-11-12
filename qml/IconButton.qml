// Toolbar icon button: hover surfaceHover, pressed surfaceActive, disabled muted. No tooltip: Qt Quick Controls would
// cost the window process ~3 MB; `tip` is the accessible name
import QtQuick
import Fili.Term

Rectangle {
    id: b
    // Line icon (Icon.qml)
    property string name
    property bool filled: false
    property string tip
    property bool enabledState: true
    // Toggle state (filter on): drawn with the active surface
    property bool toggled: false
    signal clicked()

    width: 28
    height: 24
    radius: FiliTheme.radiusControl
    color: !enabledState ? "transparent" : area.pressed || toggled ? FiliTheme.surfaceActive : area.containsMouse ? FiliTheme.surfaceHover : "transparent"
    activeFocusOnTab: false

    Icon {
        anchors.centerIn: parent
        name: b.name
        filled: b.filled
        size: 16
        color: b.enabledState ? FiliTheme.text : FiliTheme.muted
    }
    Accessible.role: Accessible.Button
    Accessible.name: b.tip
    MouseArea {
        id: area
        anchors.fill: parent
        hoverEnabled: true
        enabled: b.enabledState
        onClicked: b.clicked()
    }
}
