// "Ask Rob" button of the header: opens the chat session of Rob scoped to this application (`fili-ai here`).
// Same size and states as the window buttons (28 x 24, hover surfaceHover); the speech bubble is drawn, so it follows
// the theme (or the accent colour of an application that needs the user: set `ink`). Shared file: identical in every
// application (reference: Casier).
import QtQuick
import Fili.Term

Rectangle {
    id: b
    property color ink: FiliTheme.text

    width: 28
    height: 24
    radius: FiliTheme.radiusControl
    color: area.pressed ? FiliTheme.surfaceActive : area.containsMouse ? FiliTheme.surfaceHover : "transparent"
    activeFocusOnTab: false

    Canvas {
        anchors.centerIn: parent
        width: 16
        height: 16
        property color ink: b.ink
        onInkChanged: requestPaint()
        onPaint: {
            const c = getContext("2d");
            c.reset();
            c.lineWidth = 1.4;
            c.lineCap = "round";
            c.lineJoin = "round";
            c.strokeStyle = ink;
            // Rounded bubble with its tail at the bottom left
            c.beginPath();
            c.moveTo(4.5, 2.5);
            c.lineTo(11.5, 2.5);
            c.quadraticCurveTo(14, 2.5, 14, 5);
            c.lineTo(14, 8);
            c.quadraticCurveTo(14, 10.5, 11.5, 10.5);
            c.lineTo(8, 10.5);
            c.lineTo(5, 13.5);
            c.lineTo(5, 10.5);
            c.lineTo(4.5, 10.5);
            c.quadraticCurveTo(2, 10.5, 2, 8);
            c.lineTo(2, 5);
            c.quadraticCurveTo(2, 2.5, 4.5, 2.5);
            c.closePath();
            c.stroke();
        }
    }
    MouseArea {
        id: area
        anchors.fill: parent
        hoverEnabled: true
        onClicked: Aeroport.askRob()
    }
    Accessible.role: Accessible.Button
    Accessible.name: Aeroport.tr("Ask Rob")
}
