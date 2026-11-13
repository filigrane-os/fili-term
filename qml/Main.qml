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
    // --hold: a tab whose command ended stays open (its last screen is still there) until it is closed
    property bool hold: false
    property string initialTitle: ""
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
    // Header and footer follow Settings > Appearance (fili-theme header|footer on|off) live. A hidden footer still
    // appears while it has something to say (the latest file transaction); nothing is reachable only from the header
    property bool showHeader: Aeroport.windowPart("header")
    property bool showFooter: Aeroport.windowPart("footer")
    readonly property bool headerShown: showHeader
    readonly property bool footerShown: showFooter || txNotice.wanted || modeOffer.wanted
    Connections {
        target: Aeroport
        function onWindowChanged() {
            win.showHeader = Aeroport.windowPart("header");
            win.showFooter = Aeroport.windowPart("footer");
        }
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
    title: view && view.title ? view.title : initialTitle !== "" ? initialTitle : "Aéroport"

    function tabTitle(t) {
        return !t ? "" : t.modeTab !== "" ? t.modeLabel : t.title ? t.title : win.initialTitle !== "" ? win.initialTitle : Aeroport.shortPath(t.cwd);
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
        // A tab of herdr: herdr's focus follows (its sidebar, the agents' place)
        const t = tabs[i];
        if (t.modeTab !== "" && modeState && !modeState.tabs.some(x => x.id === t.modeTab && x.active))
            modeClient.act(JSON.stringify({ op: "tab", id: t.modeTab }));
        for (let k = 0; k < tabs.length; ++k) {
            tabs[k].visible = k === i;
            tabs[k].setShown(k === i);
        }
        tabs[i].unseen = false;
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
        // Closing a tab of herdr closes it in herdr; the tab goes away with the next state
        if (hangUp && t.modeTab !== "" && !syncing && modeState) {
            modeClient.act(JSON.stringify({ op: "close-tab", id: t.modeTab }));
            return;
        }
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
    // The chat drawer (Rob) about a command block: the context file names it, Rob reads its output himself
    function askRob(id) {
        const v = win.view;
        if (!v || id <= 0)
            return;
        const all = JSON.parse(v.contextJson()).blocks;
        const b = all.find(x => x.id === id);
        if (b)
            Aeroport.askRob(v.contextJson(), id, b.command, b.exit !== null && b.exit !== 0);
    }
    // Integrated mode (src/modes), only when the user calls the program: `fili-term mode herdr`, or the footer's offer
    // when herdr runs in a tab (or [modes] herdr = "auto"). herdr's tabs become tabs of this window, each attached to the
    // terminal of a pane (`herdr terminal attach`), without herdr's own interface; its workspaces and agents fill the
    // sidebar. Stopping the mode closes those tabs; herdr goes on.
    ModeClient {
        id: modeClient
        onStateChanged: win.syncMode()
        // An agent waits for an answer or finished, while nobody watches herdr's tabs
        onNotice: notice => {
            if (win.active && win.view && win.view.modeTab !== "")
                return;
            const n = JSON.parse(notice);
            const who = n.agent || Aeroport.tr("An agent");
            Aeroport.notify(n.kind === "blocked" ? Aeroport.tr("%1 is waiting for you").replace("%1", who) : Aeroport.tr("%1 finished").replace("%1", who),
                            n.place ? "herdr · " + n.place : "herdr");
        }
    }
    readonly property var modeState: modeClient.state !== "" ? JSON.parse(modeClient.state) : null
    // Where herdr's tabs go in the row (they stay together)
    property int modeAt: 0
    property bool syncing: false
    property string modeActive: ""
    // The sidebar of the mode, beside herdr's tabs (View > Show Sidebar hides it, for the window)
    property bool sidebarWanted: Aeroport.uiGet("sidebar") !== "0"
    readonly property bool sidebarShown: modeState !== null && sidebarWanted && (!view || view.modeTab !== "" || !tabs.some(t => t.modeTab !== ""))
    function toggleSidebar() {
        sidebarWanted = !sidebarWanted;
        Aeroport.uiSet("sidebar", sidebarWanted ? "1" : "0");
    }
    // Policy per program (fili-term.toml [modes]); "Don't Ask Again" changes it at once for this window
    property var modePolicies: settings.modes
    function modeKind(process) {
        for (const k of Object.keys(modePolicies))
            if (process === k || process.startsWith(k + "-"))
                return k;
        return "";
    }
    // Program of a plain tab that could be integrated ("" when none, or a mode already runs in the window)
    function integrable(t) {
        return t && t.modeTab === "" && modeClient.state === "" ? modeKind(t.foreground) : "";
    }
    // Start the mode: from a tab where the program runs (its client leaves, the tab goes back to its shell), or alone
    function startMode(kind, fromTab) {
        if (modeClient.state !== "")
            return;
        modeAt = fromTab ? tabs.indexOf(fromTab) + 1 : tabs.length;
        modeActive = "";
        if (!modeClient.start(kind, fromTab ? fromTab.foregroundPid() : 0))
            Aeroport.notify(Aeroport.tr("Cannot integrate %1").replace("%1", kind), Aeroport.tr("It runs on another machine."));
    }
    // Menu "Integrate Program": the program of the tab, or leave the mode that runs
    function integrate() {
        if (modeClient.state !== "")
            modeClient.stop();
        else if (integrable(view) !== "")
            startMode(integrable(view), view);
    }
    function modeAct(action) {
        modeClient.act(JSON.stringify(action));
    }
    // herdr's tabs as tabs of the window: one per tab of herdr's focused workspace, attached to the pane chosen in the
    // sidebar or else to the focused one; together at modeAt, in herdr's order
    function syncMode() {
        // The state itself, not modeState: its binding may not have followed yet
        const st = modeClient.state !== "" ? JSON.parse(modeClient.state) : null;
        if (st && !st.connected)
            return;
        syncing = true;
        const want = st ? st.tabs : [];
        let list = tabs.slice();
        const made = [];
        for (const w of want) {
            let t = list.find(x => x.modeTab === w.id);
            const chosen = t && t.modePane !== "" ? w.panes.find(p => p.id === t.modePane) : null;
            const pane = chosen || w.panes.find(p => p.focused) || w.panes[0];
            const terminal = pane ? pane.terminal : w.terminal;
            if (!terminal)
                continue;
            if (t && t.modeTerminal !== terminal) {
                // Another pane: a new attachment in the same place
                const old = t;
                t = termComponent.createObject(stack, { cwd: pane ? pane.cwd : w.cwd, command: modeClient.attachCommand(terminal), modeTab: w.id, modeTerminal: terminal, modePane: old.modePane, visible: false });
                list[list.indexOf(old)] = t;
                old.closeSession();
                old.destroy();
            } else if (!t) {
                t = termComponent.createObject(stack, { cwd: pane ? pane.cwd : w.cwd, command: modeClient.attachCommand(terminal), modeTab: w.id, modeTerminal: terminal, visible: false });
                list.push(t);
            }
            t.modeLabel = w.label;
            t.modeStatus = w.status || "";
            made.push(t);
        }
        // Tabs of herdr that went away (closed, another workspace, the mode stopped)
        for (const t of list.filter(x => x.modeTab !== "" && made.indexOf(x) < 0)) {
            t.closeSession();
            t.destroy();
        }
        const shown = view;
        const others = list.filter(x => x.modeTab === "");
        const at = Math.min(modeAt, others.length);
        tabs = others.slice(0, at).concat(made).concat(others.slice(at));
        syncing = false;
        // Show the tab herdr has active when it changed there (a click on an agent, another workspace), or when the tab
        // shown is gone
        const active = want.find(w => w.active);
        const activeTab = active ? made.find(t => t.modeTab === active.id) : null;
        if (activeTab && (active.id !== modeActive || tabs.indexOf(shown) < 0) && (shown === null || shown.modeTab !== "" || tabs.indexOf(shown) < 0 || modeActive === ""))
            select(tabs.indexOf(activeTab));
        else if (tabs.indexOf(shown) >= 0)
            current = tabs.indexOf(shown);
        else if (tabs.length > 0)
            select(Math.min(current, tabs.length - 1));
        modeActive = active ? active.id : "";
        if (tabs.length === 0 && !st)
            win.close();
    }
    // Sidebar: herdr's sections, then the panes of the tab shown when it has several
    readonly property var sidebarModel: {
        if (!modeState)
            return null;
        const v = view;
        const w = v && v.modeTab !== "" ? modeState.tabs.find(x => x.id === v.modeTab) : null;
        if (!w || w.panes.length < 2)
            return modeState;
        const panes = { key: "panes", title: "Panes", add: false, items: w.panes.map(p => ({ id: p.id, label: p.label, detail: "", status: p.status, active: p.terminal === v.modeTerminal })) };
        return Object.assign({}, modeState, { sections: modeState.sections.concat([panes]) });
    }

    // Context for the assistant (Mod+Y): the terminal of the active window, a moment after each change
    Timer {
        id: contextTimer
        interval: 150
        onTriggered: if (win.active && win.view) Aeroport.publishContext(win.view.contextJson(), 0)
    }
    onActiveChanged: contextTimer.restart()
    onViewChanged: contextTimer.restart()
    Connections {
        target: win.view
        function onBlocksRevisionChanged() { contextTimer.restart() }
        function onCwdChanged() { contextTimer.restart() }
    }

    // Search in the history (Ctrl+Shift+F): the bar at the top right of the terminal, the matches are lit in the grid
    property bool searching: false
    function openSearch() {
        searching = true;
        searchField.forceActiveFocus();
        searchField.selectAll();
        if (view && searchField.text !== "")
            view.search(searchField.text);
    }
    function closeSearch() {
        searching = false;
        searchField.text = "";
        if (view)
            view.searchClose();
        refocus();
    }
    onCurrentChanged: if (searching) closeSearch()

    function refocus() {
        if (view && !aboutCard.visible && !windowPanel.visible)
            view.forceActiveFocus();
    }

    // Commands of the fili-bar menu (fili-term ctl action NAME), for the active window
    function run(name) {
        const v = win.view;
        if (name === "about") aboutCard.show();
        // In herdr's tabs, a new tab of herdr
        else if (name === "new-tab") { if (v && v.modeTab !== "" && modeState) modeAct({ op: "new-tab" }); else win.addTab(0, v ? v.cwd : win.cwd, []) }
        else if (name === "new-window") Instance.newWindow(v ? v.cwd : win.cwd);
        else if (name === "close-tab") { if (v) win.closeTab(v, true) }
        else if (name === "close-window") win.close();
        else if (name.startsWith("tab-")) { const n = parseInt(name.slice(4)); win.select(n === 9 ? win.tabs.length - 1 : n - 1) }
        else if (name === "next-tab") win.step(1);
        else if (name === "previous-tab") win.step(-1);
        else if (name === "zoom-in") win.zoom(1);
        else if (name === "zoom-out") win.zoom(-1);
        else if (name === "zoom-reset") win.zoom(0);
        else if (name === "show-in-files") Aeroport.showInFiles(v ? v.cwd : win.cwd);
        else if (name === "window-actions") windowPanel.visible ? windowPanel.close() : windowPanel.open();
        else if (name === "toggle-window-buttons") win.toggleWindowButtons();
        else if (name === "toggle-sidebar") win.toggleSidebar();
        else if (v) {
            if (name === "copy") v.copy();
            else if (name === "paste") v.paste();
            else if (name === "page-up") v.scrollPage(-1);
            else if (name === "page-down") v.scrollPage(1);
            else if (name === "scroll-top") v.scrollPage(-2);
            else if (name === "scroll-bottom") v.scrollPage(2);
            else if (name === "previous-command") v.jumpBlock(-1);
            else if (name === "next-command") v.jumpBlock(1);
            else if (name === "copy-last-output") v.copyBlock(v.lastBlock(), 0);
            else if (name === "copy-last-command") v.copyCommand(v.lastBlock());
            else if (name === "ask-rob") win.askRob(v.lastBlock());
            else if (name === "edit-output") v.editBlock(v.lastBlock());
            else if (name === "find") win.openSearch();
            else if (name === "integrate") win.integrate();
            else if (name === "find-next") { if (v.searchActive) v.searchStep(1); else win.openSearch() }
            else if (name === "find-previous") { if (v.searchActive) v.searchStep(-1); else win.openSearch() }
        }
    }

    // `fili-term mode herdr`: the window opens on herdr's tabs only
    property string initialMode: ""
    Component.onCompleted: initialMode !== "" ? startMode(initialMode, null) : addTab(attachId, cwd, command)
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
            // Result of a command that ended while this tab was not shown, until the tab is selected
            property bool unseen: false
            property int lastExit: 0
            // A tab of herdr: its tab id, the terminal attached, the pane chosen in the sidebar ("" the focused one),
            // its label and the status of its agents
            property string modeTab: ""
            property string modeTerminal: ""
            property string modePane: ""
            property string modeLabel: ""
            property string modeStatus: ""
            // The user said "Not Now" to the offer: not asked again while the program stays in the foreground
            property string declined: ""
            onForegroundChanged: {
                const kind = win.modeKind(foreground);
                if (kind === "")
                    declined = "";
                else if (win.integrable(term) !== "" && win.modePolicies[kind] === "auto" && declined !== kind)
                    win.startMode(kind, term);
            }
            anchors.fill: parent
            fontFamily: FiliTheme.mono
            fontPixelSize: win.fontSize
            padding: win.settings.padding
            lineSpacing: win.settings.line_spacing
            colors: FiliTheme.terminalColors
            onExited: if (!(win.hold && term.command.length > 0)) win.closeTab(term, false)
            onBell: win.alert(0)
            // A long command ended while nobody watches this terminal: a notification with its result
            onCommandFinished: (command, exit, duration) => {
                const after = win.settings.notify_after;
                if (term !== win.view) { term.unseen = true; term.lastExit = exit }
                if (after <= 0 || duration < after * 1000 || (win.active && term === win.view))
                    return;
                const text = command.trim().replace(/\s+/g, " ");
                Aeroport.notify(exit === 0 ? Aeroport.tr("Command finished") : Aeroport.tr("Command failed (status %1)").replace("%1", exit),
                                (text.length > 100 ? text.slice(0, 100) + "…" : text) + " · " + Aeroport.duration(duration));
            }
        }
    }

    // Header: title of the shell (what it runs), new tab, window buttons
    Rectangle {
        id: toolbar
        width: parent.width
        visible: win.headerShown
        height: win.headerShown ? FiliTheme.toolbarHeight : 0
        color: FiliTheme.background
        Text {
            anchors { left: parent.left; leftMargin: FiliTheme.margin; right: searchBar.inHeader ? searchBar.left : tools.left; rightMargin: FiliTheme.space3; verticalCenter: parent.verticalCenter }
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
        visible: win.headerShown
        height: win.headerShown ? FiliTheme.borderWidth : 0
        color: FiliTheme.border
    }

    // Tabs, from two: one per shell of the window (Facteur's look); middle click or × closes one
    Rectangle {
        id: tabRow
        visible: win.tabs.length > 1 || win.modeState !== null
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
                    width: Math.max(80, Math.min(180, label.implicitWidth + 2 * FiliTheme.space4 + 16 + (tab.state !== "" ? 18 : 0),
                                                 (tabRow.width - 2 * FiliTheme.margin) / win.tabs.length - FiliTheme.space3))
                    height: FiliTheme.tabHeight
                    radius: FiliTheme.radiusControl
                    color: on ? FiliTheme.surfaceActive : hovered ? FiliTheme.surfaceHover : "transparent"
                    // What the tab does: the command being run (spinner), else the folder; a check or a cross when a
                    // command ended while the tab was not shown
                    readonly property var term: win.tabs[index]
                    readonly property string running: { term.blocksRevision; return term.runningCommand() }
                    readonly property string state: term.modeTab !== "" ? (term.modeStatus === "working" ? "running" : term.modeStatus === "blocked" ? "waiting" : term.modeStatus === "done" ? "ok" : "")
                        : running !== "" ? "running" : term.unseen ? (term.lastExit === 0 ? "ok" : "failed") : ""
                    Icon {
                        id: stateIcon
                        visible: tab.state !== ""
                        anchors { left: parent.left; leftMargin: FiliTheme.space3; verticalCenter: parent.verticalCenter }
                        name: tab.state === "running" ? "refresh" : tab.state === "ok" ? "check" : tab.state === "waiting" ? "bell" : "x"
                        size: 12
                        color: tab.state === "failed" ? FiliTheme.danger : tab.state === "waiting" ? FiliTheme.warning : tab.state === "running" ? FiliTheme.accentColor : FiliTheme.textSecondary
                        RotationAnimator on rotation { running: tab.state === "running" && win.active; from: 0; to: 360; duration: 1400; loops: Animation.Infinite }
                    }
                    Text {
                        id: label
                        anchors { left: stateIcon.visible ? stateIcon.right : parent.left; leftMargin: stateIcon.visible ? FiliTheme.space2 : FiliTheme.space4; right: parent.right; rightMargin: FiliTheme.space4; verticalCenter: parent.verticalCenter }
                        text: tab.running !== "" && tab.term.modeTab === "" ? tab.running : win.tabTitle(win.tabs[tab.index])
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

    // Sidebar of herdr: workspaces, agents, the panes of the tab shown
    Sidebar {
        id: sidebar
        visible: win.sidebarShown
        anchors { left: parent.left; top: tabRow.visible ? tabRow.bottom : toolbarLine.bottom; bottom: footer.top }
        width: visible ? FiliTheme.sidebarWidth : 0
        mode: win.sidebarModel
        onActivated: (section, id) => {
            if (section === "panes") {
                if (win.view) { win.view.modePane = id; win.syncMode() }
            } else {
                win.modeAct({ op: "select", section: section, id: id });
            }
            win.refocus();
        }
        onAdded: section => { win.modeAct({ op: "new", section: section }); win.refocus() }
        onLeave: { modeClient.stop(); win.refocus() }
    }

    // The terminals of the window, one shown
    Item {
        id: stack
        anchors { left: sidebar.right; right: parent.right; top: tabRow.visible ? tabRow.bottom : toolbarLine.bottom; bottom: footer.top }
    }

    // Search in the history, as in Casier: a field in the header (surface, border, accent when focused); without a
    // header it appears over the top right of the terminal while it is used. Counter inside, Enter / Shift+Enter move
    Rectangle {
        id: searchBar
        readonly property bool inHeader: win.headerShown
        visible: inHeader || win.searching
        x: inHeader ? toolbar.x + tools.x - width - searchNav.width - FiliTheme.space3 - FiliTheme.space1 : stack.x + stack.width - width - searchNav.width - FiliTheme.space4 - 12 - FiliTheme.space1
        y: inHeader ? (toolbar.height - height) / 2 : stack.y + FiliTheme.space3
        z: 60
        width: inHeader ? 180 : 240
        height: 26
        radius: FiliTheme.radiusControl
        color: FiliTheme.surface
        border.width: FiliTheme.borderWidth
        border.color: searchField.activeFocus ? FiliTheme.accentColor : FiliTheme.border
        readonly property bool none: win.view !== null && win.view.searchActive && win.view.searchCount === 0
        Text {
            anchors { left: parent.left; leftMargin: FiliTheme.space2; verticalCenter: parent.verticalCenter }
            visible: searchField.text === ""
            text: Aeroport.tr("Search")
            font.family: FiliTheme.font
            font.pixelSize: FiliTheme.size
            color: FiliTheme.muted
        }
        TextInput {
            id: searchField
            anchors { left: parent.left; leftMargin: FiliTheme.space2; right: counter.visible ? counter.left : clear.visible ? clear.left : parent.right; rightMargin: FiliTheme.space2; verticalCenter: parent.verticalCenter }
            clip: true
            selectByMouse: true
            font.family: FiliTheme.font
            font.pixelSize: FiliTheme.size
            color: FiliTheme.text
            selectionColor: FiliTheme.surfaceActive
            selectedTextColor: FiliTheme.text
            Accessible.name: Aeroport.tr("Find in History")
            onTextChanged: if (win.view) win.view.search(text)
            onActiveFocusChanged: {
                if (activeFocus)
                    win.searching = true;
                else if (text === "")
                    win.searching = false;
            }
            Keys.onPressed: e => {
                if (e.key === Qt.Key_Return || e.key === Qt.Key_Enter) {
                    if (win.view) win.view.searchStep(e.modifiers & Qt.ShiftModifier ? -1 : 1);
                    e.accepted = true;
                } else if (e.key === Qt.Key_Escape) {
                    win.closeSearch();
                    e.accepted = true;
                }
            }
        }
        Text {
            id: counter
            anchors { right: clear.visible ? clear.left : parent.right; rightMargin: FiliTheme.space2; verticalCenter: parent.verticalCenter }
            visible: searchField.text !== "" && win.view !== null && win.view.searchActive
            text: searchBar.none ? Aeroport.tr("No results") : Aeroport.tr("%1 of %2").replace("%1", win.view ? win.view.searchPosition : 0).replace("%2", win.view ? win.view.searchCount : 0)
            font.family: FiliTheme.font
            font.pixelSize: FiliTheme.sizeSmall
            color: searchBar.none ? FiliTheme.danger : FiliTheme.textSecondary
        }
        // Clears the search (without a header it also closes the field)
        Rectangle {
            id: clear
            visible: searchField.text !== ""
            anchors { right: parent.right; rightMargin: FiliTheme.space1; verticalCenter: parent.verticalCenter }
            width: 20
            height: 20
            radius: FiliTheme.radiusControl
            color: clearArea.containsMouse ? FiliTheme.alpha(FiliTheme.text, 0.12) : "transparent"
            Icon { anchors.centerIn: parent; name: "x"; size: 12; color: FiliTheme.textSecondary }
            MouseArea {
                id: clearArea
                anchors.fill: parent
                hoverEnabled: true
                onClicked: { if (searchBar.inHeader) { searchField.text = ""; searchField.forceActiveFocus() } else win.closeSearch() }
            }
            Accessible.role: Accessible.Button
            Accessible.name: Aeroport.tr("Clear")
        }
    }

    // Previous and next match, at the right of the field
    Row {
        id: searchNav
        visible: searchBar.visible
        x: searchBar.x + searchBar.width + FiliTheme.space1
        y: searchBar.y + (searchBar.height - 24) / 2
        z: 60
        IconButton { name: "chevron-left"; tip: Aeroport.tr("Previous Match"); enabledState: win.view !== null && win.view.searchCount > 0; onClicked: win.run("find-previous") }
        IconButton { name: "chevron-right"; tip: Aeroport.tr("Next Match"); enabledState: win.view !== null && win.view.searchCount > 0; onClicked: win.run("find-next") }
    }

    // Scroll bar of the history: a thin handle at the right edge, shown while scrolling, on hover and when dragged
    Item {
        id: scrollBar
        readonly property var v: win.view
        readonly property int history: v ? v.historySize : 0
        readonly property int rows: v ? Math.max(1, v.lines) : 1
        readonly property real thumbHeight: Math.max(32, Math.min(height, height * rows / (rows + history)))
        // 0 at the top of the history, 1 at the prompt
        readonly property real fraction: history > 0 ? 1 - v.scrollOffset / history : 1
        anchors { right: stack.right; top: stack.top; bottom: stack.bottom }
        width: 14
        z: 50
        visible: history > 0
        Rectangle {
            id: thumb
            x: parent.width - width - 3
            width: grip.containsMouse || grip.drag.active ? 8 : 6
            height: scrollBar.thumbHeight
            radius: width / 2
            color: FiliTheme.alpha(FiliTheme.text, grip.drag.active ? 0.55 : grip.containsMouse ? 0.45 : 0.3)
            opacity: grip.containsMouse || grip.drag.active || hold.running ? 1 : 0
            Behavior on opacity { NumberAnimation { duration: 180 } }
            Behavior on width { NumberAnimation { duration: 100 } }
        }
        Binding { target: thumb; property: "y"; value: scrollBar.fraction * (scrollBar.height - thumb.height); when: !grip.drag.active }
        Timer { id: hold; interval: 700 }
        Connections {
            target: scrollBar.v
            function onScrollOffsetChanged() { hold.restart() }
            function onHistorySizeChanged() { hold.restart() }
        }
        MouseArea {
            id: grip
            anchors.fill: parent
            hoverEnabled: true
            drag.target: thumb
            drag.axis: Drag.YAxis
            drag.minimumY: 0
            drag.maximumY: scrollBar.height - thumb.height
            function follow(y) {
                const span = scrollBar.height - thumb.height;
                if (span > 0 && scrollBar.v)
                    scrollBar.v.scrollToOffset(Math.round((1 - Math.max(0, Math.min(1, y / span))) * scrollBar.history));
            }
            // A click beside the handle jumps there; the handle then follows the cursor
            onPressed: e => { if (e.y < thumb.y || e.y > thumb.y + thumb.height) follow(e.y - thumb.height / 2) }
            onPositionChanged: if (grip.drag.active) follow(thumb.y)
        }
    }

    // Files dragged from Casier (or any file manager): their paths are typed at the prompt
    DropArea {
        anchors.fill: stack
        keys: ["text/uri-list"]
        onDropped: drop => {
            if (drop.hasUrls && win.view) {
                win.view.dropPaths(drop.urls.join("\n"));
                drop.accept();
                win.view.forceActiveFocus();
            }
        }
    }

    // Command blocks: actions on the block under the pointer, at the right of its first line (pinned to the top of
    // the view while that line is scrolled out). Takes no button: clicks and the wheel go on to the terminal.
    MouseArea {
        id: blockHover
        anchors.fill: stack
        hoverEnabled: true
        acceptedButtons: Qt.NoButton
        property var block: null
        // A link under the pointer with Ctrl held: underlined, hand cursor
        property bool link: false
        cursorShape: link ? Qt.PointingHandCursor : Qt.ArrowCursor
        function look() {
            const v = win.view;
            const j = v && containsMouse ? v.blockAt(mouseY) : "";
            block = j ? JSON.parse(j) : null;
        }
        onPositionChanged: mouse => { look(); link = win.view !== null && win.view.linkAt(mouseX, mouseY, (mouse.modifiers & Qt.ControlModifier) !== 0) }
        onContainsMouseChanged: { look(); if (!containsMouse) { link = false; if (win.view) win.view.linkAt(0, 0, false) } }
        Connections {
            target: win.view
            function onBlocksRevisionChanged() { blockHover.look() }
            function onScrollOffsetChanged() { blockHover.look() }
        }

        Rectangle {
            id: blockBar
            readonly property var b: blockHover.block
            visible: b !== null && b.state !== "prompt"
            x: parent.width - width - FiliTheme.space4
            y: b ? Math.max(b.top - (height - win.fontSize * 1.2) / 2, FiliTheme.space1) : 0
            width: barRow.width + 2 * FiliTheme.space1
            height: 28
            radius: FiliTheme.radiusControl
            color: FiliTheme.background
            border.width: FiliTheme.borderWidth
            border.color: FiliTheme.border
            Row {
                id: barRow
                anchors.centerIn: parent
                spacing: FiliTheme.space1
                Text {
                    anchors.verticalCenter: parent.verticalCenter
                    readonly property var hovered: [copyOut, copyCmd, rerun, edit, files, rob].find(x => x.hovered)
                    text: hovered ? hovered.tip : blockBar.status()
                    leftPadding: FiliTheme.space3
                    rightPadding: FiliTheme.space2
                    font.family: FiliTheme.font
                    font.pixelSize: FiliTheme.sizeSmall
                    color: !hovered && blockBar.b && blockBar.b.exit ? FiliTheme.danger : FiliTheme.textSecondary
                }
                IconButton {
                    id: copyOut
                    readonly property bool hovered: containsMouse
                    name: "copy"
                    tip: Aeroport.tr("Copy Output")
                    enabledState: blockBar.b !== null && blockBar.b.outputTop >= 0
                    onClicked: win.view.copyBlock(blockBar.b.id, 0)
                }
                IconButton {
                    id: copyCmd
                    readonly property bool hovered: containsMouse
                    name: "terminal"
                    tip: Aeroport.tr("Copy Command")
                    onClicked: win.view.copyCommand(blockBar.b.id)
                }
                IconButton {
                    id: rerun
                    readonly property bool hovered: containsMouse
                    name: "refresh"
                    tip: Aeroport.tr("Run Again")
                    enabledState: win.view !== null && blockBar.b !== null && win.view.blocksRevision >= 0 && win.view.canRerun()
                    onClicked: win.view.rerunBlock(blockBar.b.id)
                }
                IconButton {
                    id: edit
                    readonly property bool hovered: containsMouse
                    name: "compose"
                    tip: Aeroport.tr("Edit Output")
                    enabledState: blockBar.b !== null && blockBar.b.outputTop >= 0
                    onClicked: win.view.editBlock(blockBar.b.id)
                }
                IconButton {
                    id: files
                    readonly property bool hovered: containsMouse
                    name: "folder"
                    tip: Aeroport.tr("Show in Casier")
                    onClicked: Aeroport.showInFiles(blockBar.b.cwd)
                }
                IconButton {
                    id: rob
                    readonly property bool hovered: containsMouse
                    name: "chat"
                    tip: blockBar.b && blockBar.b.exit ? Aeroport.tr("Ask Rob Why It Failed") : Aeroport.tr("Ask Rob About This Command")
                    onClicked: win.askRob(blockBar.b.id)
                }
            }
            // "Running", "Failed (2) · 3.4 s", "Done · 850 ms"
            function status() {
                if (!b)
                    return "";
                if (b.state === "running")
                    return Aeroport.tr("Running");
                const parts = [b.exit ? Aeroport.tr("Failed (%1)").replace("%1", b.exit) : Aeroport.tr("Done")];
                if (b.duration !== null)
                    parts.push(Aeroport.duration(b.duration));
                return parts.join(" · ");
            }
        }
    }

    // Footer: where the shell started, size of the grid or place in the history
    Rectangle {
        id: footer
        anchors { left: parent.left; right: parent.right; bottom: parent.bottom }
        visible: win.footerShown
        height: win.footerShown ? FiliTheme.statusHeight : 0
        color: FiliTheme.backgroundDeep
        Rectangle { anchors { left: parent.left; right: parent.right; top: parent.top } height: FiliTheme.borderWidth; color: FiliTheme.border }
        Text {
            id: where
            anchors { left: parent.left; leftMargin: FiliTheme.margin; right: modeOffer.wanted ? modeOffer.left : txNotice.wanted ? txNotice.left : size.left; rightMargin: FiliTheme.space4; verticalCenter: parent.verticalCenter }
            text: win.view ? Aeroport.shortPath(win.view.cwd) : ""
            elide: Text.ElideMiddle
            font.family: FiliTheme.font
            font.pixelSize: FiliTheme.sizeSmall
            color: FiliTheme.textSecondary
        }
        // A program with a mode runs in the tab (herdr): offer to integrate it, never on its own unless [modes] says "auto"
        Row {
            id: modeOffer
            readonly property string kind: win.integrable(win.view)
            readonly property bool wanted: kind !== "" && win.modePolicies[kind] === "ask" && win.view.declined !== kind
            visible: wanted
            anchors { right: txNotice.wanted ? txNotice.left : size.left; rightMargin: FiliTheme.space5; verticalCenter: parent.verticalCenter }
            spacing: FiliTheme.space3
            Text {
                anchors.verticalCenter: parent.verticalCenter
                text: Aeroport.tr("%1 is running").replace("%1", modeOffer.kind)
                font.family: FiliTheme.font
                font.pixelSize: FiliTheme.sizeSmall
                color: FiliTheme.textSecondary
            }
            Repeater {
                model: [
                    { label: Aeroport.tr("Integrate"), act: "integrate", accent: true },
                    { label: Aeroport.tr("Not Now"), act: "later", accent: false },
                    { label: Aeroport.tr("Don't Ask Again"), act: "never", accent: false }
                ]
                Text {
                    required property var modelData
                    anchors.verticalCenter: parent.verticalCenter
                    text: modelData.label
                    font.family: FiliTheme.font
                    font.pixelSize: FiliTheme.sizeSmall
                    color: modelData.accent ? FiliTheme.accentColor : offerArea.containsMouse ? FiliTheme.text : FiliTheme.textSecondary
                    MouseArea {
                        id: offerArea
                        anchors { fill: parent; margins: -4 }
                        hoverEnabled: true
                        cursorShape: Qt.PointingHandCursor
                        onClicked: {
                            const v = win.view, kind = modeOffer.kind;
                            if (parent.modelData.act === "integrate") win.startMode(kind, v);
                            else if (parent.modelData.act === "later") v.declined = kind;
                            else {
                                Aeroport.setModePolicy(kind, "off");
                                const p = Object.assign({}, win.modePolicies);
                                p[kind] = "off";
                                win.modePolicies = p;
                            }
                            win.refocus();
                        }
                    }
                    Accessible.role: Accessible.Button
                    Accessible.name: text
                }
            }
        }
        // The latest file transaction (fili-tx: what Rob or a tool just changed), for a couple of minutes, with Undo
        Row {
            id: txNotice
            property var tx: null
            readonly property bool undone: tx !== null && tx.state === "undone"
            readonly property bool wanted: tx !== null && tx.changes > 0 && tx.age < 120 && (tx.state === "open" || tx.state === "committed" || undone)
            visible: wanted
            anchors { right: size.left; rightMargin: FiliTheme.space5; verticalCenter: parent.verticalCenter }
            spacing: FiliTheme.space3
            function refresh() { const j = Aeroport.txState(); tx = j ? JSON.parse(j) : null }
            Timer { interval: 2000; repeat: true; running: win.active; triggeredOnStart: true; onTriggered: txNotice.refresh() }
            Text {
                anchors.verticalCenter: parent.verticalCenter
                readonly property string what: !txNotice.tx ? ""
                    : txNotice.tx.label && txNotice.tx.label !== "auto" ? txNotice.tx.label
                    : Aeroport.tr("Files changed: %1").replace("%1", txNotice.tx.changes)
                text: txNotice.undone ? Aeroport.tr("%1 undone").replace("%1", what) : what
                elide: Text.ElideRight
                width: Math.min(implicitWidth, 260)
                font.family: FiliTheme.font
                font.pixelSize: FiliTheme.sizeSmall
                color: FiliTheme.textSecondary
            }
            Text {
                anchors.verticalCenter: parent.verticalCenter
                text: txNotice.undone ? Aeroport.tr("Redo") : Aeroport.tr("Undo")
                font.family: FiliTheme.font
                font.pixelSize: FiliTheme.sizeSmall
                color: FiliTheme.accentColor
                MouseArea {
                    anchors { fill: parent; margins: -4 }
                    cursorShape: Qt.PointingHandCursor
                    onClicked: { Aeroport.txRun(txNotice.undone ? "redo" : "undo"); txNotice.refresh() }
                }
                Accessible.role: Accessible.Button
                Accessible.name: text
            }
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
        anchorRight: win.headerShown ? width - (toolbar.height - closeButton.height) / 2 : width - FiliTheme.margin
        anchorTop: toolbar.height + 1
        onVisibleChanged: if (!visible) win.refocus()
    }
    AboutCard {
        id: aboutCard
        anchors.fill: parent
        onVisibleChanged: if (!visible) win.refocus()
    }

    // Keys of the menu (appmenu/fili-term.json shows those of the default preset); Ctrl+Shift so that Ctrl+letter stays the shell's
    // The keys come from the preset and the [keys] section of fili-term.toml (Settings > Aéroport), reread live
    property var keymap: JSON.parse(Aeroport.keysJson())
    Connections {
        target: Aeroport
        function onWindowChanged() { win.keymap = JSON.parse(Aeroport.keysJson()) }
    }
    Instantiator {
        model: win.keymap
        delegate: Shortcut {
            required property var modelData
            sequences: modelData.keys
            onActivated: win.run(modelData.action)
        }
    }
}
