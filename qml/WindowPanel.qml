// Window panel (toolbar button "Window"): every window action niri offers, one clickable tile with its pictogram.
// It opens under the button, a click elsewhere or Esc closes it, and it closes after an action. The actions run on
// this window (the focused one) through Aeroport.windowAction (src/window.rs lists the names it accepts). Outside niri
// the panel offers what Qt does on any desktop: fullscreen, maximize, minimize (items with `local`).
// The tiles that describe the window as it is now (fullscreen, floating, full width...) are highlighted. The layout
// adapts to the space the window leaves: more columns when it is short, a scrollable card when even that is too tall.
import QtQuick
import QtQuick.Window
import Fili.Term

Item {
    id: panel
    property real anchorRight: width
    property real anchorTop: 0
    readonly property int tileW: 92
    readonly property int tileH: 62
    readonly property var localGroups: [
        { title: Aeroport.tr("Window"), items: [
            { local: "fullscreen", glyph: "fullscreen", label: Aeroport.tr("Fullscreen") },
            { local: "maximize", glyph: "maximize-edges", label: Aeroport.tr("Maximize") },
            { local: "minimize", glyph: "minimize", label: Aeroport.tr("Minimize") }
        ] }
    ]
    readonly property var groups: Aeroport.windowControl() ? niriGroups : localGroups
    readonly property var niriGroups: [
        { title: Aeroport.tr("Size"), items: [
            { action: "fullscreen-window", glyph: "fullscreen", label: Aeroport.tr("Fullscreen") },
            { action: "toggle-windowed-fullscreen", glyph: "windowed-fullscreen", label: Aeroport.tr("Windowed fullscreen") },
            { action: "maximize-column", glyph: "maximize-column", label: Aeroport.tr("Maximize column") },
            { action: "maximize-window-to-edges", glyph: "maximize-edges", label: Aeroport.tr("Maximize to edges") },
            { action: "expand-column-to-available-width", glyph: "expand", label: Aeroport.tr("Fill the width") },
            { action: "switch-preset-column-width", glyph: "preset-width", label: Aeroport.tr("Next width") },
            { action: "switch-preset-window-height", glyph: "preset-height", label: Aeroport.tr("Next height") },
            { action: "reset-window-height", glyph: "reset-height", label: Aeroport.tr("Reset height") }
        ] },
        { title: Aeroport.tr("Position"), items: [
            { action: "center-column", glyph: "center-column", label: Aeroport.tr("Center column") },
            { action: "center-window", glyph: "center-window", label: Aeroport.tr("Center window") },
            { action: "toggle-window-floating", glyph: "float", label: Aeroport.tr("Floating") },
            { action: "move-column-left", glyph: "column-left", label: Aeroport.tr("Column left") },
            { action: "move-column-right", glyph: "column-right", label: Aeroport.tr("Column right") },
            { action: "move-window-up", glyph: "window-up", label: Aeroport.tr("Move up"), needsStack: true },
            { action: "move-window-down", glyph: "window-down", label: Aeroport.tr("Move down"), needsStack: true }
        ] },
        { title: Aeroport.tr("Arrange"), items: [
            // Alone in its column the window joins the neighbour; with others it leaves the column (what niri does)
            { action: "consume-or-expel-window-left", glyph: "join-left", label: Aeroport.tr("Join left"), whenStacked: { glyph: "leave-left", label: Aeroport.tr("Leave column left") } },
            { action: "consume-or-expel-window-right", glyph: "join-right", label: Aeroport.tr("Join right"), whenStacked: { glyph: "leave-right", label: Aeroport.tr("Leave column right") } },
            { action: "toggle-column-tabbed-display", glyph: "tabs", label: Aeroport.tr("Tabbed column"), needsStack: true },
            { action: "toggle-overview", glyph: "overview", label: Aeroport.tr("Overview") }
        ] },
        { title: Aeroport.tr("Send to"), items: [
            { action: "move-window-to-workspace-up", glyph: "workspace-up", label: Aeroport.tr("Workspace above") },
            { action: "move-window-to-workspace-down", glyph: "workspace-down", label: Aeroport.tr("Workspace below") },
            { action: "move-window-to-monitor-left", glyph: "monitor-left", label: Aeroport.tr("Monitor left") },
            { action: "move-window-to-monitor-right", glyph: "monitor-right", label: Aeroport.tr("Monitor right") },
            { action: "screenshot-window", glyph: "screenshot", label: Aeroport.tr("Screenshot") }
        ] }
    ]

    // Tiles that match the state of the window now (names of `action` items, or `local` kinds), read when it opens
    property var active: []
    property int cols: 3
    readonly property int headingH: 16
    readonly property real availH: height - anchorTop - FiliTheme.space2
    readonly property real availW: width - 2 * FiliTheme.space1
    function heightFor(c) {
        let h = 2 * FiliTheme.space2 + 2 + (groups.length - 1) * FiliTheme.space2;
        for (const g of groups) {
            const r = Math.ceil(g.items.length / c);
            h += headingH + FiliTheme.space1 + r * tileH + (r - 1) * FiliTheme.space1;
        }
        return h;
    }
    function widthFor(c) {
        return c * tileW + (c - 1) * FiliTheme.space1 + 2 * FiliTheme.space2 + 2;
    }
    // Fewest columns (3 or more) for which the card fits in the height; as many as fit in the width otherwise
    function fit() {
        let most = 3;
        while (widthFor(most + 1) <= availW && most < 8)
            most++;
        let c = 3;
        while (c < most && heightFor(c) > availH)
            c++;
        cols = Math.min(c, Math.max(1, most));
    }
    onAvailHChanged: if (visible) fit()
    onAvailWChanged: if (visible) fit()
    // The window shares its column with others (niri's "join" then leaves the column)
    readonly property bool stacked: active.includes("stacked")
    // What a tile shows and does now: its stacked variant when there is one
    function shown(item) {
        return stacked && item.whenStacked ? Object.assign({}, item, item.whenStacked) : item;
    }
    // A column action with nothing to act on (tabs, move up/down in a column of one window) is dimmed
    function enabledItem(item) {
        return !item.needsStack || stacked;
    }
    function isActive(item) {
        if (item.action)
            return active.includes(item.action);
        const w = Window.window;
        return !!w && w.visibility === { fullscreen: Window.FullScreen, maximize: Window.Maximized, minimize: Window.Minimized }[item.local];
    }
    function refreshState() {
        const w = Window.window;
        const full = !!w && w.visibility === Window.FullScreen;
        let list = Aeroport.windowControl() ? Aeroport.windowState().split(",").filter(x => x !== "") : [];
        if (full)
            list = list.filter(x => x !== "maximize-window-to-edges").concat(["fullscreen-window"]);
        active = list;
    }
    // Keyboard selection: index in the flat list of tiles; rows[] gives the tiles of each visual row
    property int current: 0
    // Last pointer position seen: the pointer resting under the panel when it opens must not take the selection
    property real px: -1
    property real py: -1
    readonly property var rows: {
        const r = [];
        let n = 0;
        for (const g of groups) {
            for (let i = 0; i < g.items.length; i += cols) {
                const row = [];
                for (let k = i; k < Math.min(i + cols, g.items.length); k++)
                    row.push(n + k);
                r.push(row);
            }
            n += g.items.length;
        }
        return r;
    }
    readonly property var flat: groups.reduce((a, g) => a.concat(g.items), [])
    function rowOf(i) {
        return rows.findIndex(r => r.includes(i));
    }
    function moveRow(d) {
        const r = rowOf(current), col = rows[r].indexOf(current);
        const t = rows[(r + d + rows.length) % rows.length];
        current = t[Math.min(col, t.length - 1)];
    }

    visible: false
    anchors.fill: parent
    focus: visible
    z: 100

    function open() {
        current = 0;
        px = -1;
        refreshState();
        visible = true;
        fit();
        forceActiveFocus();
    }
    function close() {
        visible = false;
    }
    function run(item) {
        if (!enabledItem(item))
            return;
        close();
        if (item.action) {
            Aeroport.windowAction(item.action);
            return;
        }
        const w = Window.window;
        if (!w)
            return;
        const target = { fullscreen: Window.FullScreen, maximize: Window.Maximized, minimize: Window.Minimized }[item.local];
        w.visibility = w.visibility === target ? Window.Windowed : target;
    }
    Keys.onPressed: e => {
        const n = flat.length;
        switch (e.key) {
        case Qt.Key_Escape: close(); break;
        case Qt.Key_Right: current = (current + 1) % n; break;
        case Qt.Key_Left: current = (current - 1 + n) % n; break;
        case Qt.Key_Tab: current = (current + 1) % n; break;
        case Qt.Key_Backtab: current = (current - 1 + n) % n; break;
        case Qt.Key_Down: moveRow(1); break;
        case Qt.Key_Up: moveRow(-1); break;
        case Qt.Key_Home: current = 0; break;
        case Qt.Key_End: current = n - 1; break;
        case Qt.Key_Return:
        case Qt.Key_Enter:
        case Qt.Key_Space: run(flat[current]); break;
        default: return;
        }
        e.accepted = true;
    }

    MouseArea {
        anchors.fill: parent
        acceptedButtons: Qt.AllButtons
        onPressed: panel.close()
    }

    Rectangle {
        id: card
        x: Math.max(FiliTheme.space1, panel.anchorRight - width)
        y: panel.anchorTop
        width: panel.widthFor(panel.cols)
        height: Math.min(body.height + 2 * FiliTheme.space2 + 2, panel.availH)
        radius: 8
        color: FiliTheme.surface
        border.width: FiliTheme.borderWidth
        border.color: FiliTheme.border

        // Keeps clicks inside the card from closing it
        MouseArea {
            anchors.fill: parent
            acceptedButtons: Qt.AllButtons
        }

        Flickable {
            id: scroller
            anchors { fill: parent; margins: 1 }
            clip: true
            contentWidth: width
            contentHeight: body.height + 2 * FiliTheme.space2
            boundsBehavior: Flickable.StopAtBounds
            interactive: contentHeight > height
            KineticScroll {}
        // Brings a tile into view (keyboard selection)
        function reveal(item) {
            const top = item.mapToItem(body, 0, 0).y + FiliTheme.space2;
            if (top < scroller.contentY)
                scroller.contentY = top - FiliTheme.space1;
            else if (top + item.height > scroller.contentY + scroller.height)
                scroller.contentY = top + item.height - scroller.height + FiliTheme.space1;
        }
        Column {
            id: body
            x: FiliTheme.space2
            y: FiliTheme.space2
            spacing: FiliTheme.space2
            Repeater {
                model: panel.groups
                delegate: Column {
                    id: grp
                    required property var modelData
                    required property int index
                    readonly property int base: panel.groups.slice(0, index).reduce((a, g) => a + g.items.length, 0)
                    spacing: FiliTheme.space1
                    Text {
                        text: grp.modelData.title
                        font.family: FiliTheme.font
                        font.pixelSize: FiliTheme.sizeSmall
                        font.weight: FiliTheme.weightAccent
                        color: FiliTheme.muted
                    }
                    Grid {
                        columns: panel.cols
                        spacing: FiliTheme.space1
                        Repeater {
                            model: grp.modelData.items
                            delegate: Rectangle {
                                id: tile
                                required property var modelData
                                required property int index
                                readonly property bool selected: panel.current === grp.base + index
                                readonly property bool on: panel.active.length >= 0 && panel.isActive(modelData)
                                onSelectedChanged: if (selected) scroller.reveal(tile)
                                width: panel.tileW
                                height: panel.tileH
                                opacity: panel.active.length >= 0 && panel.enabledItem(modelData) ? 1 : 0.4
                                radius: FiliTheme.radiusControl
                                color: tileArea.pressed ? FiliTheme.surfaceActive : tile.selected ? FiliTheme.surfaceHover : tile.on ? FiliTheme.alpha(FiliTheme.accentColor, 0.22) : "transparent"
                                border.width: tile.selected ? 1 : 0
                                border.color: FiliTheme.accentColor
                                Column {
                                    anchors.centerIn: parent
                                    spacing: 4
                                    WindowGlyph {
                                        anchors.horizontalCenter: parent.horizontalCenter
                                        kind: panel.shown(tile.modelData).glyph
                                        color: tile.on ? FiliTheme.accentBright : FiliTheme.text
                                        width: 20
                                        height: 20
                                    }
                                    Text {
                                        width: panel.tileW - 8
                                        horizontalAlignment: Text.AlignHCenter
                                        text: panel.shown(tile.modelData).label
                                        wrapMode: Text.WordWrap
                                        maximumLineCount: 2
                                        elide: Text.ElideRight
                                        font.family: FiliTheme.font
                                        font.pixelSize: FiliTheme.sizeSmall
                                        color: tile.on ? FiliTheme.accentBright : FiliTheme.text
                                    }
                                }
                                MouseArea {
                                    id: tileArea
                                    anchors.fill: parent
                                    hoverEnabled: true
                                    onPositionChanged: mouse => {
                                        const p = tileArea.mapToItem(panel, mouse.x, mouse.y);
                                        const moved = panel.px >= 0 && Math.abs(p.x - panel.px) + Math.abs(p.y - panel.py) >= 3;
                                        if (moved)
                                            panel.current = grp.base + tile.index;
                                        if (panel.px < 0 || moved) {
                                            panel.px = p.x;
                                            panel.py = p.y;
                                        }
                                    }
                                    onClicked: panel.run(tile.modelData)
                                }
                            }
                        }
                    }
                }
            }
        }
        }
        FiliScrollBar {
            flick: scroller
            anchors { top: scroller.top; bottom: scroller.bottom; right: scroller.right }
        }
    }
}
