// Outline of the item chosen with the keyboard, visible only once the keyboard has been used (Aeroport.keyNav), like the
// FocusRing of the fili-shell. negative gap: outline drawn inside (views stuck to their neighbors).
import QtQuick
import Fili.Term

Rectangle {
    property Item target: parent
    property bool shown: target.activeFocus
    property real gap: 2
    anchors { fill: target; margins: -gap }
    radius: Math.max(0, (target.radius ?? FiliTheme.radiusControl) + gap)
    color: "transparent"
    border.width: FiliTheme.borderWidth
    border.color: FiliTheme.accentColor
    visible: shown && Aeroport.keyNav
}
