// A window of Aéroport: header (title, new tab, window buttons), tab row from two tabs, the terminals, footer (where
// the shell is, size of the grid or place in the history). App.qml creates one per request, all in one process.
import QtQuick
import QtQuick.Window
import Fili.Term

Window {
    id: win

    property int attachId: 0
    property string cwd: ""
    property var command: []
    property var tabs: []
    property int current: -1
    readonly property var view: current >= 0 && current < tabs.length ? tabs[current] : null

    // Settings of fili-term.toml, read when the window opens
    readonly property var settings: JSON.parse(Aeroport.settingsJson())
    property int fontSize: settings.font_size
    // Close and window-actions buttons at the right of the header (bar menu: Window > Show Window Buttons); the choice
    // made from the menu is kept in the state file and wins over the setting
    property bool windowButtons: {
        const kept = Aeroport.uiGet("window_buttons");
        return kept === "" ? settings.window_buttons : kept !== "0";
    }
    function toggleWindowButtons() {
        windowButtons = !windowButtons;
        Aeroport.uiSet("window_buttons", windowButtons ? "1" : "0");
    }

    width: 860
    height: 560
    minimumWidth: 360
    minimumHeight: 200
    visible: true
    color: FiliTheme.background
    title: view && view.title ? view.title : "Aéroport"

    function tabTitle(t) {
        return !t ? "" : t.title ? t.title : Aeroport.shortPath(t.cwd);
    }
    function addTab(attach, dir, argv) {
        const t = termComponent.createObject(stack, { sessionId: attach, cwd: dir, command: argv || [] });
        tabs = tabs.concat([t]);
        select(tabs.length - 1);
    }
    function select(i) {
        if (i < 0 || i >= tabs.length)
            return;
        current = i;
        for (let k = 0; k < tabs.length; ++k) {
            tabs[k].visible = k === i;
            tabs[k].setShown(k === i);
        }
        tabs[i].forceActiveFocus();
    }
    function step(delta) {
        if (tabs.length > 1)
            select((current + delta + tabs.length) % tabs.length);
    }
    function zoom(delta) {
        fontSize = delta === 0 ? settings.font_size : Math.max(6, Math.min(48, fontSize + delta));
    }
    function closeTab(t, hangUp) {
        const i = tabs.indexOf(t);
        if (i < 0)
            return;
        if (hangUp)
            t.closeSession();
        tabs = tabs.filter(x => x !== t);
        t.destroy();
        if (tabs.length === 0) {
            win.close();
            return;
        }
        select(Math.min(i, tabs.length - 1));
    }
    function refocus() {
        if (view && !aboutCard.visible && !windowPanel.visible)
            view.forceActiveFocus();
    }

    // Commands of the fili-bar menu (fili-term ctl action NAME), for the active window
    function run(name) {
        const v = win.view;
        if (name === "about") aboutCard.show();
        else if (name === "new-tab") win.addTab(0, v ? v.cwd : win.cwd, []);
        else if (name === "new-window") Instance.newWindow(v ? v.cwd : win.cwd);
        else if (name === "close-tab") { if (v) win.closeTab(v, true) }
        else if (name === "close-window") win.close();
        else if (name === "next-tab") win.step(1);
        else if (name === "previous-tab") win.step(-1);
        else if (name === "zoom-in") win.zoom(1);
        else if (name === "zoom-out") win.zoom(-1);
        else if (name === "zoom-reset") win.zoom(0);
        else if (name === "window-actions") windowPanel.visible ? windowPanel.close() : windowPanel.open();
        else if (name === "toggle-window-buttons") win.toggleWindowButtons();
        else if (v) {
            if (name === "copy") v.copy();
            else if (name === "paste") v.paste();
            else if (name === "page-up") v.scrollPage(-1);
            else if (name === "page-down") v.scrollPage(1);
            else if (name === "scroll-top") v.scrollPage(-2);
            else if (name === "scroll-bottom") v.scrollPage(2);
        }
    }

    Component.onCompleted: addTab(attachId, cwd, command)
    signal closed()
    onClosing: {
        for (const t of tabs)
            t.closeSession();
        win.closed();
        win.destroy();
    }

    Component {
        id: termComponent
        TerminalView {
            id: term
            anchors.fill: parent
            fontFamily: FiliTheme.mono
            fontPixelSize: win.fontSize
            padding: win.settings.padding
            colors: FiliTheme.terminalColors
            onExited: win.closeTab(term, false)
            onBell: win.alert(0)
        }
    }

    // Header: title of the shell (what it runs), new tab, window buttons
    Rectangle {
        id: toolbar
        width: parent.width
        height: FiliTheme.toolbarHeight
        color: FiliTheme.background
        Text {
            anchors { left: parent.left; leftMargin: FiliTheme.margin; right: tools.left; rightMargin: FiliTheme.space3; verticalCenter: parent.verticalCenter }
            text: win.tabTitle(win.view)
            elide: Text.ElideRight
            font.family: FiliTheme.font
            font.pixelSize: FiliTheme.size
            font.weight: FiliTheme.weightAccent
            color: FiliTheme.text
        }
        Row {
            id: tools
            anchors { right: win.windowButtons ? windowButton.left : parent.right; rightMargin: win.windowButtons ? FiliTheme.space3 : FiliTheme.margin; verticalCenter: parent.verticalCenter }
            spacing: FiliTheme.space1
            IconButton { name: "plus"; tip: Aeroport.tr("New Tab"); onClicked: win.run("new-tab") }
        }
        // Window buttons at the far right: close, and the panel of window actions (niri)
        Rectangle {
            id: closeButton
            visible: win.windowButtons
            anchors { right: parent.right; rightMargin: (toolbar.height - height) / 2; verticalCenter: parent.verticalCenter }
            width: 28
            height: 24
            radius: FiliTheme.radiusControl
            color: closeArea.pressed ? FiliTheme.danger : closeArea.containsMouse ? FiliTheme.alpha(FiliTheme.danger, 0.75) : "transparent"
            WindowGlyph {
                anchors.centerIn: parent
                kind: "close"
                width: 16
                height: 16
                color: closeArea.containsMouse ? "#ffffff" : FiliTheme.text
            }
            MouseArea { id: closeArea; anchors.fill: parent; hoverEnabled: true; onClicked: win.close() }
            Accessible.role: Accessible.Button
            Accessible.name: Aeroport.tr("Close window")
        }
        Rectangle {
            id: windowButton
            visible: win.windowButtons
            anchors { right: closeButton.left; rightMargin: FiliTheme.space1; verticalCenter: parent.verticalCenter }
            width: 28
            height: 24
            radius: FiliTheme.radiusControl
            color: windowArea.pressed ? FiliTheme.surfaceActive : windowArea.containsMouse || windowPanel.visible ? FiliTheme.surfaceHover : "transparent"
            WindowGlyph { anchors.centerIn: parent; kind: "window"; width: 16; height: 16 }
            MouseArea { id: windowArea; anchors.fill: parent; hoverEnabled: true; onClicked: windowPanel.visible ? windowPanel.close() : windowPanel.open() }
            Accessible.role: Accessible.Button
            Accessible.name: Aeroport.tr("Window")
        }
    }
    Rectangle {
        id: toolbarLine
        anchors.top: toolbar.bottom
        width: parent.width
        height: FiliTheme.borderWidth
        color: FiliTheme.border
    }

    // Tabs, from two: one per shell of the window (Facteur's look); middle click or × closes one
    Rectangle {
        id: tabRow
        visible: win.tabs.length > 1
        anchors { left: parent.left; right: parent.right; top: toolbarLine.bottom }
        height: visible ? FiliTheme.tabBarHeight : 0
        color: FiliTheme.background
        Row {
            anchors { left: parent.left; leftMargin: FiliTheme.margin; right: parent.right; rightMargin: FiliTheme.margin; verticalCenter: parent.verticalCenter }
            spacing: FiliTheme.space3
            Repeater {
                model: win.tabs.length
                Rectangle {
                    id: tab
                    required property int index
                    readonly property bool on: index === win.current
                    readonly property bool hovered: tabArea.containsMouse || closeTabArea.containsMouse
                    // Readable width (80 to 180), shared out when the row is full
                    width: Math.max(80, Math.min(180, label.implicitWidth + 2 * FiliTheme.space4 + 16,
                                                 (tabRow.width - 2 * FiliTheme.margin) / win.tabs.length - FiliTheme.space3))
                    height: FiliTheme.tabHeight
                    radius: FiliTheme.radiusControl
                    color: on ? FiliTheme.surfaceActive : hovered ? FiliTheme.surfaceHover : "transparent"
                    Text {
                        id: label
                        anchors { left: parent.left; leftMargin: FiliTheme.space4; right: parent.right; rightMargin: FiliTheme.space4; verticalCenter: parent.verticalCenter }
                        text: win.tabTitle(win.tabs[tab.index])
                        elide: Text.ElideRight
                        font.family: FiliTheme.font
                        font.pixelSize: FiliTheme.sizeList
                        font.weight: tab.on ? FiliTheme.weightAccent : Font.Normal
                        color: tab.on ? FiliTheme.text : FiliTheme.textSecondary
                    }
                    MouseArea {
                        id: tabArea
                        anchors.fill: parent
                        hoverEnabled: true
                        acceptedButtons: Qt.LeftButton | Qt.MiddleButton
                        onClicked: e => e.button === Qt.MiddleButton ? win.closeTab(win.tabs[tab.index], true) : win.select(tab.index)
                    }
                    Rectangle {
                        visible: tab.hovered
                        anchors { right: parent.right; rightMargin: FiliTheme.space1; verticalCenter: parent.verticalCenter }
                        width: 20
                        height: 20
                        radius: FiliTheme.radiusControl
                        color: closeTabArea.containsMouse ? FiliTheme.alpha(FiliTheme.text, 0.12) : tab.color
                        Icon { anchors.centerIn: parent; name: "x"; size: 12; color: FiliTheme.textSecondary }
                        MouseArea { id: closeTabArea; anchors.fill: parent; hoverEnabled: true; onClicked: win.closeTab(win.tabs[tab.index], true) }
                        Accessible.role: Accessible.Button
                        Accessible.name: Aeroport.tr("Close Tab")
                    }
                }
            }
        }
        Rectangle { anchors { left: parent.left; right: parent.right; bottom: parent.bottom } height: FiliTheme.borderWidth; color: FiliTheme.border }
    }

    // The terminals of the window, one shown
    Item {
        id: stack
        anchors { left: parent.left; right: parent.right; top: tabRow.visible ? tabRow.bottom : toolbarLine.bottom; bottom: footer.top }
    }

    // Footer: where the shell started, size of the grid or place in the history
    Rectangle {
        id: footer
        anchors { left: parent.left; right: parent.right; bottom: parent.bottom }
        height: FiliTheme.statusHeight
        color: FiliTheme.backgroundDeep
        Rectangle { anchors { left: parent.left; right: parent.right; top: parent.top } height: FiliTheme.borderWidth; color: FiliTheme.border }
        Text {
            id: where
            anchors { left: parent.left; leftMargin: FiliTheme.margin; right: size.left; rightMargin: FiliTheme.space4; verticalCenter: parent.verticalCenter }
            text: win.view ? Aeroport.shortPath(win.view.cwd) : ""
            elide: Text.ElideMiddle
            font.family: FiliTheme.font
            font.pixelSize: FiliTheme.sizeSmall
            color: FiliTheme.textSecondary
        }
        Text {
            id: size
            anchors { right: parent.right; rightMargin: FiliTheme.margin; verticalCenter: parent.verticalCenter }
            readonly property bool back: win.view !== null && win.view.scrollOffset > 0
            text: !win.view ? ""
                : back ? Aeroport.tr("%1 of %2 lines back").replace("%1", win.view.scrollOffset).replace("%2", win.view.historySize)
                : Aeroport.tr("%1 × %2").replace("%1", win.view.columns).replace("%2", win.view.lines)
            font.family: FiliTheme.font
            font.pixelSize: FiliTheme.sizeSmall
            color: back ? FiliTheme.accentColor : FiliTheme.textSecondary
            MouseArea {
                anchors.fill: parent
                enabled: size.back
                cursorShape: enabled ? Qt.PointingHandCursor : Qt.ArrowCursor
                onClicked: win.run("scroll-bottom")
            }
        }
    }

    WindowPanel {
        id: windowPanel
        anchorRight: width - (toolbar.height - closeButton.height) / 2
        anchorTop: toolbar.height + 1
        onVisibleChanged: if (!visible) win.refocus()
    }
    AboutCard {
        id: aboutCard
        anchors.fill: parent
        onVisibleChanged: if (!visible) win.refocus()
    }

    // Keys of the menu (appmenu/fili-term.json shows the same); Ctrl+Shift so that Ctrl+letter stays the shell's
    Shortcut { sequence: "Ctrl+Shift+T"; onActivated: win.run("new-tab") }
    Shortcut { sequence: "Ctrl+Shift+N"; onActivated: win.run("new-window") }
    Shortcut { sequence: "Ctrl+Shift+W"; onActivated: win.run("close-tab") }
    Shortcut { sequence: "Ctrl+Shift+Q"; onActivated: win.run("close-window") }
    Shortcut { sequences: ["Ctrl+Shift+C", "Ctrl+Ins", "Copy"]; onActivated: win.run("copy") }
    Shortcut { sequences: ["Ctrl+Shift+V", "Shift+Ins", "Paste"]; onActivated: win.run("paste") }
    Shortcut { sequences: ["Ctrl+PgDown", "Ctrl+Tab"]; onActivated: win.run("next-tab") }
    Shortcut { sequences: ["Ctrl+PgUp", "Ctrl+Shift+Tab"]; onActivated: win.run("previous-tab") }
    Shortcut { sequences: ["Ctrl++", "Ctrl+="]; onActivated: win.run("zoom-in") }
    Shortcut { sequence: "Ctrl+-"; onActivated: win.run("zoom-out") }
    Shortcut { sequence: "Ctrl+0"; onActivated: win.run("zoom-reset") }
    // Shift+PgUp / Shift+PgDown (history) are handled by the TerminalView itself
}
