// Discreet vertical scroll indicator: thin handle on the right edge of a list or grid, visible while scrolling,
// when hovering the edge and when dragging (fade), absent when everything fits. The handle can be dragged; a click in the
// track jumps one page. Place it as a sibling of the view, anchored to it (anchors.top/bottom/right of the view).
import QtQuick
import Fili.Term

Item {
    id: bar
    required property Flickable flick
    readonly property bool needed: flick.contentHeight > flick.height + 1
    readonly property real ratio: Math.min(1, flick.height / Math.max(1, flick.contentHeight))
    readonly property real thumbHeight: Math.max(32, ratio * height)
    // Fraction (0 to 1) of the possible scrolling that has been covered
    readonly property real fraction: {
        const room = flick.contentHeight - flick.height;
        return room > 0 ? Math.max(0, Math.min(1, (flick.contentY - flick.originY) / room)) : 0;
    }
    visible: needed
    width: 14
    z: 50

    // Handle
    Rectangle {
        id: thumb
        x: parent.width - width - 3
        width: grip.containsMouse || grip.drag.active ? 8 : 6
        height: bar.thumbHeight
        radius: width / 2
        color: FiliTheme.alpha(FiliTheme.text, grip.drag.active ? 0.55 : grip.containsMouse ? 0.45 : 0.3)
        opacity: flick.moving || grip.containsMouse || grip.drag.active || hold.running ? 1 : 0
        Behavior on opacity { NumberAnimation { duration: 180 } }
        Behavior on width { NumberAnimation { duration: 100 } }
    }
    // Position from the scrolling, except while it is dragged (then it drives the scrolling)
    Binding {
        target: thumb
        property: "y"
        value: bar.fraction * (bar.height - thumb.height)
        when: !grip.drag.active
    }
    // Stays visible for a moment after the scrolling ends
    Timer { id: hold; interval: 700 }
    Connections {
        target: bar.flick
        function onContentYChanged() { hold.restart(); }
    }

    MouseArea {
        id: grip
        anchors.fill: parent
        hoverEnabled: true
        // The handle follows the cursor; the scrolling follows from it
        drag.target: thumb
        drag.axis: Drag.YAxis
        drag.minimumY: 0
        drag.maximumY: bar.height - thumb.height
        onPressed: e => {
            // Click outside the handle: one page towards the click
            if (e.y < thumb.y || e.y > thumb.y + thumb.height) {
                const page = bar.flick.height * 0.9;
                bar.flick.contentY = Math.max(bar.flick.originY, Math.min(bar.flick.originY + bar.flick.contentHeight - bar.flick.height, bar.flick.contentY + (e.y < thumb.y ? -page : page)));
            }
        }
        onPositionChanged: {
            if (grip.drag.active) {
                const span = bar.height - thumb.height;
                const room = bar.flick.contentHeight - bar.flick.height;
                if (span > 0 && room > 0)
                    bar.flick.contentY = bar.flick.originY + thumb.y / span * room;
            }
        }
    }
}
