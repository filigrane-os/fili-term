// Pictogram of a window action, drawn on a 16x16 grid in the current text color (no icon font needed, so it follows
// the theme). `kind` names the picture; unknown kinds draw nothing.
import QtQuick
import Fili.Term

Canvas {
    id: g
    property string kind
    property color color: FiliTheme.text
    property real stroke: 1.4

    width: 18
    height: 18
    onKindChanged: requestPaint()
    onColorChanged: requestPaint()
    onWidthChanged: requestPaint()

    onPaint: {
        const c = getContext("2d");
        c.reset();
        c.scale(width / 16, height / 16);
        c.lineWidth = stroke;
        c.lineCap = "round";
        c.lineJoin = "round";
        c.strokeStyle = g.color;
        c.fillStyle = g.color;
        const box = (x, y, w, h) => { c.beginPath(); c.rect(x, y, w, h); c.stroke(); };
        const fill = (x, y, w, h, a) => { c.globalAlpha = a === undefined ? 1 : a; c.fillRect(x, y, w, h); c.globalAlpha = 1; };
        const line = (...p) => { c.beginPath(); c.moveTo(p[0], p[1]); for (let i = 2; i < p.length; i += 2) c.lineTo(p[i], p[i + 1]); c.stroke(); };
        // Arrow from (x1, y1) to (x2, y2) with a small head
        const arrow = (x1, y1, x2, y2) => {
            const a = Math.atan2(y2 - y1, x2 - x1), h = 2.6;
            line(x1, y1, x2, y2);
            line(x2 - h * Math.cos(a - 0.6), y2 - h * Math.sin(a - 0.6), x2, y2, x2 - h * Math.cos(a + 0.6), y2 - h * Math.sin(a + 0.6));
        };
        switch (kind) {
        case "close":
            line(3.5, 3.5, 12.5, 12.5);
            line(12.5, 3.5, 3.5, 12.5);
            break;
        case "window":
            box(2.5, 2.5, 11, 11);
            break;
        case "minimize":
            box(1.5, 2.5, 13, 11);
            line(4.5, 10.5, 11.5, 10.5);
            break;
        case "fullscreen":
            line(2, 6, 2, 2, 6, 2);
            line(10, 2, 14, 2, 14, 6);
            line(14, 10, 14, 14, 10, 14);
            line(6, 14, 2, 14, 2, 10);
            break;
        case "windowed-fullscreen":
            box(1.5, 2.5, 13, 11);
            box(4.5, 5, 7, 6);
            break;
        case "maximize-column":
            box(1.5, 2.5, 13, 11);
            fill(5, 2.5, 6, 11, 0.5);
            line(5, 2.5, 5, 13.5);
            line(11, 2.5, 11, 13.5);
            break;
        case "maximize-edges":
            box(1.5, 2.5, 13, 11);
            fill(1.5, 2.5, 13, 11, 0.5);
            break;
        case "expand":
            box(1.5, 2.5, 13, 11);
            arrow(8, 8, 12.5, 8);
            arrow(8, 8, 3.5, 8);
            break;
        case "preset-width":
            box(1.5, 2.5, 13, 11);
            line(8, 4, 8, 12);
            arrow(8, 8, 13, 8);
            break;
        case "preset-height":
            box(1.5, 2.5, 13, 11);
            line(4, 8, 12, 8);
            arrow(8, 8, 8, 12.5);
            break;
        case "reset-height":
            box(1.5, 2.5, 13, 11);
            line(8, 4.5, 8, 11.5);
            line(5.5, 4.5, 10.5, 4.5);
            line(5.5, 11.5, 10.5, 11.5);
            break;
        case "center-column":
            box(1.5, 2.5, 13, 11);
            fill(5.5, 4.5, 5, 7, 0.55);
            break;
        case "center-window":
            box(1.5, 2.5, 13, 11);
            box(5, 5, 6, 6);
            fill(7.2, 7.2, 1.6, 1.6);
            break;
        case "float":
            box(1.5, 2.5, 9, 7);
            fill(1.6, 2.6, 8.8, 6.8, 0.25);
            box(5.5, 6.5, 9, 7);
            break;
        case "column-left":
            arrow(14, 8, 3.5, 8);
            line(1.8, 3, 1.8, 13);
            break;
        case "column-right":
            arrow(2, 8, 12.5, 8);
            line(14.2, 3, 14.2, 13);
            break;
        case "window-up":
            arrow(8, 14, 8, 3.5);
            line(3, 1.8, 13, 1.8);
            break;
        case "window-down":
            arrow(8, 2, 8, 12.5);
            line(3, 14.2, 13, 14.2);
            break;
        case "join-left":
            box(1.5, 2.5, 5, 11);
            box(9.5, 2.5, 5, 5);
            arrow(14.5, 11, 9, 11);
            break;
        case "join-right":
            box(9.5, 2.5, 5, 11);
            box(1.5, 2.5, 5, 5);
            arrow(1.5, 11, 7, 11);
            break;
        case "leave-left":
            box(7, 2.5, 7.5, 11);
            arrow(10, 8, 1.5, 8);
            break;
        case "leave-right":
            box(1.5, 2.5, 7.5, 11);
            arrow(6, 8, 14.5, 8);
            break;
        case "tabs":
            box(1.5, 5.5, 13, 8);
            line(1.5, 5.5, 1.5, 3, 6, 3, 6, 5.5);
            line(7.5, 5.5, 7.5, 3, 12, 3, 12, 5.5);
            break;
        case "workspace-up":
            box(3.5, 7.5, 9, 6.5);
            arrow(8, 6, 8, 1.8);
            break;
        case "workspace-down":
            box(3.5, 2, 9, 6.5);
            arrow(8, 10, 8, 14.2);
            break;
        case "monitor-left":
            box(5, 2.5, 9.5, 8);
            line(8, 13.5, 11.5, 13.5);
            arrow(4, 6.5, 1.2, 6.5);
            break;
        case "monitor-right":
            box(1.5, 2.5, 9.5, 8);
            line(4.5, 13.5, 8, 13.5);
            arrow(12, 6.5, 14.8, 6.5);
            break;
        case "screenshot":
            line(2, 5.5, 2, 2, 5.5, 2);
            line(10.5, 2, 14, 2, 14, 5.5);
            line(14, 10.5, 14, 14, 10.5, 14);
            line(5.5, 14, 2, 14, 2, 10.5);
            fill(6.2, 6.2, 3.6, 3.6);
            break;
        case "overview":
            box(1.5, 1.5, 5.5, 5.5);
            box(9, 1.5, 5.5, 5.5);
            box(1.5, 9, 5.5, 5.5);
            box(9, 9, 5.5, 5.5);
            break;
        }
    }
}
