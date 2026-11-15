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
    // Tabs of the window (Tab.qml: a tree of panes), the one shown, and its pane with the keyboard
    property var tabs: []
    property int current: -1
    readonly property var tab: current >= 0 && current < tabs.length ? tabs[current] : null
    readonly property var view: tab ? tab.active : null

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
    readonly property bool footerShown: showFooter || txNotice.wanted || modeOffer.wanted || asking !== null || error !== "" || resizing
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
        const v = t ? t.active : null;
        return !v ? "" : t.modeTab !== "" ? t.modeLabel : v.title ? v.title : win.initialTitle !== "" ? win.initialTitle : Aeroport.shortPath(v.cwd);
    }
    function newTab(props) {
        const t = tabComponent.createObject(stack, Object.assign({ visible: false }, props || {}));
        return t;
    }
    function addTab(attach, dir, argv) {
        const t = newTab();
        t.start({ sessionId: attach, cwd: dir, command: argv || [] });
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
        tabs[i].focusActive();
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
            t.closeAll();
        tabs = tabs.filter(x => x !== t);
        t.destroy();
        if (tabs.length === 0) {
            win.close();
            return;
        }
        select(Math.min(i, tabs.length - 1));
    }
    // Close one pane (the tab with its last pane); a pane of herdr closes in herdr
    function closePane(v, hangUp) {
        const t = v ? v.paneTab : null;
        if (!t)
            return;
        // A pane of herdr closes in herdr; the next state takes it away
        if (hangUp && t.modeTab !== "" && v.modePane !== "" && modeState) {
            modeAct({ op: "close", section: "panes", id: v.modePane });
            return;
        }
        if (t.leaves.length <= 1) {
            closeTab(t, hangUp);
            return;
        }
        if (hangUp)
            v.closeSession();
        t.remove(v);
        if (t === tab)
            t.focusActive();
    }
    // The pane of herdr shown in a tab of herdr (null outside)
    function herdrPane(t) {
        const info = t && t.modeTab !== "" && modeState ? modeState.tabs.find(x => x.id === t.modeTab) : null;
        const id = t && t.active ? t.active.modePane : "";
        return info ? info.panes.find(p => p.id === id) || null : null;
    }
    // herdr's pane of a terminal ({id, label, named, status…}), null outside herdr's tabs
    function herdrPaneOf(v) {
        const t = v ? v.paneTab : null;
        const info = t && t.modeTab !== "" && modeState ? modeState.tabs.find(x => x.id === t.modeTab) : null;
        return info ? info.panes.find(p => p.id === v.modePane) || null : null;
    }
    // A pane took the keyboard in a tab of herdr: herdr's focus follows (its agents, its keys act there)
    function paneFocused(v) {
        const t = v ? v.paneTab : null;
        if (!t || t.modeTab === "" || syncing || v.modePane === "" || !modeState)
            return;
        const info = modeState.tabs.find(x => x.id === t.modeTab);
        if (info && info.pane !== v.modePane)
            modeAct({ op: "focus-pane", id: v.modePane });
    }
    // Exchange the active pane with its neighbour in a direction (in herdr for a tab of herdr)
    function swapPane(dir) {
        const t = tab;
        const o = t && t.active ? t.neighbour(t.active, dir) : null;
        if (!o)
            return;
        if (t.modeTab !== "")
            modeAct({ op: "swap", id: t.active.modePane, with: o.modePane });
        else
            t.swap(dir);
    }
    // Pane actions of the menu and the keys: herdr does them in its tabs, the tab itself elsewhere
    function paneAction(name) {
        const t = tab, v = view;
        if (!t || !v)
            return;
        const herdr = t.modeTab !== "" && v.modePane !== "" && modeState !== null;
        if (name === "zoom") {
            if (herdr) modeAct({ op: "zoom", id: v.modePane }); else t.toggleZoom();
        } else if (name.startsWith("focus-")) {
            // Moving out of a zoomed pane shows them all again, as in herdr
            if (herdr && t.zoomed) modeAct({ op: "zoom", id: v.modePane });
            t.focusTowards(name.slice(6));
        } else if (name.startsWith("resize-")) {
            if (herdr) modeAct({ op: "resize", id: v.modePane, direction: name.slice(7) }); else t.resize(name.slice(7), 3 * win.fontSize);
        }
    }
    // A divider let go (or reset by a double click): herdr keeps the new ratio of its tab
    function dividerMoved(d) {
        const t = tab;
        if (t && t.modeTab !== "" && d && d.node.path !== undefined)
            modeAct({ op: "ratio", id: t.modeTab, path: d.node.path, ratio: d.node.ratio });
    }
    // Split the pane with the keyboard: a shell in the same folder, right of it or below; in a tab of herdr, herdr splits
    function split(dir) {
        const t = tab, v = view;
        if (!t || !v)
            return;
        if (t.modeTab !== "") {
            const p = herdrPane(t);
            if (p)
                modeAct({ op: "split", id: p.id, direction: dir });
            return;
        }
        t.split(dir, { cwd: v.cwd, command: [] });
    }
    // Menu of a pane (right click, or Shift+right click when the program takes the mouse): copy and paste, the panes,
    // and in a tab of herdr the entries of herdr's pane menu
    function paneMenu(v, x, y) {
        const t = v.paneTab;
        // Window coordinates (the window itself is no Item: it has no mapFromItem)
        const p = v.mapToItem(null, x, y);
        let entries = [{ id: "pane:copy", label: Aeroport.tr("Copy"), hint: keyHint("copy") },
                       { id: "pane:paste", label: Aeroport.tr("Paste"), hint: keyHint("paste") }, { separator: true }];
        // In a tab of herdr, herdr's names of panes first
        const hp = herdrPane(t);
        if (hp)
            entries = entries.concat([{ id: "rename-pane", label: Aeroport.tr("Rename Pane…") }, { id: "clear-pane-name", label: Aeroport.tr("Clear Pane Name") }, { separator: true }]);
        {
            const several = t.leaves.length > 1;
            const swaps = ["left", "right", "up", "down"].map(d => ({ id: "pane:swap-" + d, label: Aeroport.tr(swapLabels[d]), enabled: t.neighbour(v, d) !== null }));
            entries = entries.concat([
                { id: "pane:split-right", label: Aeroport.tr("Split Right"), hint: keyHint("split-right") },
                { id: "pane:split-down", label: Aeroport.tr("Split Down"), hint: keyHint("split-down") },
                { separator: true },
                { id: "pane:swap", label: Aeroport.tr("Swap Pane"), enabled: several, children: swaps.filter(x => x.enabled) },
                { id: "pane:move", label: Aeroport.tr("Move Pane To"), enabled: moveTargets(t).length > 0, children: moveTargets(t) },
                { id: "pane:zoom", label: t.zoomed ? Aeroport.tr("Show All Panes") : Aeroport.tr("Zoom Pane"), hint: keyHint("zoom-pane"), enabled: several },
                { separator: true },
                { id: "pane:close", label: Aeroport.tr("Close Pane"), hint: keyHint("close-pane") }
            ]);
            menuTarget = { section: hp ? "panes" : "pane", item: hp, x: p.x, y: p.y };
        }
        contextMenu.open(p.x, p.y, entries);
    }
    // Where a pane can go: another tab of the same kind, a new tab (and in herdr a new workspace)
    function moveTargets(t) {
        if (t.modeTab !== "") {
            return tabs.filter(x => x.modeTab !== "" && x !== t).map(x => ({ id: "pane:move-tab:" + x.modeTab, label: x.modeLabel }))
                .concat([{ id: "pane:move-new-tab", label: Aeroport.tr("New Tab") }, { id: "pane:move-new-workspace", label: Aeroport.tr("New Workspace") }]);
        }
        const out = tabs.map((x, i) => ({ x: x, i: i })).filter(o => o.x.modeTab === "" && o.x !== t)
            .map(o => ({ id: "pane:move-tab:" + o.i, label: tabTitle(o.x) }));
        if (t.leaves.length > 1)
            out.push({ id: "pane:move-new-tab", label: Aeroport.tr("New Tab") });
        return out;
    }
    // Move the active pane: in herdr through herdr; here, the terminal itself goes (its session goes on)
    function movePane(name) {
        const t = tab, v = view;
        if (!t || !v)
            return;
        if (t.modeTab !== "") {
            const ws = modeSection("workspaces").find(w => w.active);
            const dest = name.startsWith("move-tab:") ? { type: "tab", tab_id: name.slice(9), split: "right" }
                : name === "move-new-tab" ? { type: "new_tab", workspace_id: ws ? ws.id : null }
                : name === "move-new-workspace" ? { type: "new_workspace" } : null;
            if (dest && v.modePane !== "")
                modeAct({ op: "move-pane", id: v.modePane, destination: dest });
            return;
        }
        let to = name.startsWith("move-tab:") ? tabs[parseInt(name.slice(9))] : null;
        const single = t.leaves.length <= 1;
        if ((!to && single) || to === t)
            return;
        if (!single)
            t.take(v);
        if (!to) {
            to = newTab();
            tabs = tabs.concat([to]);
        }
        to.insert(v, "right");
        // A tab whose only pane left goes away, without hanging the pane up
        if (single) {
            tabs = tabs.filter(x => x !== t);
            t.destroy();
        }
        select(tabs.indexOf(to));
    }
    // A tab dragged in the row and dropped at x (row coordinates): its new place. herdr's tabs move in herdr.
    function dropTab(from, x) {
        let to = 0;
        for (let k = 0; k < tabs.length; ++k) {
            const it = tabRepeater.itemAt(k);
            if (k !== from && it && it.x + it.width / 2 < x)
                ++to;
        }
        const t = tabs[from];
        if (!t || to === from)
            return;
        if (t.modeTab !== "") {
            const before = tabs.filter((x, i) => i !== from).slice(0, to).filter(x => x.modeTab !== "").length;
            modeAct({ op: "move-tab", id: t.modeTab, index: before });
            return;
        }
        const shown = tab;
        const list = tabs.filter(x => x !== t);
        list.splice(to, 0, t);
        tabs = list;
        const firstMode = list.findIndex(x => x.modeTab !== "");
        if (firstMode >= 0)
            modeAt = firstMode;
        current = tabs.indexOf(shown);
    }
    readonly property var swapLabels: ({ left: "With the Pane on the Left", right: "With the Pane on the Right", up: "With the Pane Above", down: "With the Pane Below" })
    function menuAction(id) {
        if (!id.startsWith("pane:")) {
            herdrMenuAction(id);
            return;
        }
        const name = id.slice(5);
        if (name.startsWith("move-")) {
            movePane(name);
        } else if (name.startsWith("swap-")) {
            swapPane(name.slice(5));
        } else if (name === "zoom") {
            run("zoom-pane");
        } else if (name === "close") {
            run("close-pane");
        } else {
            run(name);
        }
    }
    // First key of an action, for a menu
    function keyHint(action) {
        const k = keymap.find(x => x.action === action);
        return k && k.keys.length ? k.keys[0] : "";
    }
    // The chat drawer (Rob) about a command block: the context file names it, Rob reads its output himself
    function askRob(id, on) {
        const v = on || win.view;
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
            if (win.active && win.tab && win.tab.modeTab !== "")
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
    readonly property bool sidebarShown: modeState !== null && sidebarWanted && (!tab || tab.modeTab !== "" || !tabs.some(t => t.modeTab !== ""))
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
    function integrable(v) {
        return v && v.paneTab && v.paneTab.modeTab === "" && modeClient.state === "" ? modeKind(v.foreground) : "";
    }
    // Start the mode: from a tab where the program runs (its client leaves, the tab goes back to its shell), or alone
    function startMode(kind, fromTab) {
        if (modeClient.state !== "")
            return;
        modeAt = fromTab ? tabs.indexOf(fromTab.paneTab) + 1 : tabs.length;
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
        // Any other action may move herdr's focus itself (a split, a new pane): followed at once
        if (action.op !== "focus-pane" && tab)
            tab.chosen = null;
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
        // Every terminal attached in herdr's tabs, by terminal: a pane moved to another tab keeps its own
        const pool = {};
        for (const t of list.filter(x => x.modeTab !== ""))
            for (const v of t.leaves)
                pool[v.modeTerminal] = v;
        for (const w of want) {
            // The panes of herdr's tab laid out as herdr has them, each attached to its terminal
            let t = list.find(x => x.modeTab === w.id);
            const fresh = !t;
            if (fresh)
                t = newTab({ modeTab: w.id });
            const tree = w.layout || w.panes.find(p => p.focused) || w.panes[0] || null;
            // Right click as in herdr's client: its pane menu, or the program with the modifier of its config
            const props = n => ({ cwd: n.cwd || w.cwd, command: modeClient.attachCommand(n.terminal, n.id || ""), menuFirst: true, passMods: modeClient.rightClickPass(), primaryScreen: true });
            if (!t.syncTree(tree, w.pane, w.zoomed === true, props, pool)) {
                if (fresh)
                    t.destroy();
                continue;
            }
            if (fresh)
                list.push(t);
            t.modeLabel = w.label;
            t.modeStatus = w.status || "";
            made.push(t);
        }
        // Panes of herdr gone (closed, another workspace, the mode stopped), then the tabs gone
        for (const k of Object.keys(pool)) {
            pool[k].closeSession();
            pool[k].destroy();
        }
        for (const t of list.filter(x => x.modeTab !== "" && made.indexOf(x) < 0))
            t.destroy();
        const shown = tab;
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
    // herdr's agents in the sidebar: grouped by workspace (a heading per workspace) or by priority (waiting for you, then
    // done, working, idle), as herdr's "grouped" / "priority"; kept between runs
    property string agentsSort: Aeroport.uiGet("herdr_agents") === "priority" ? "priority" : "grouped"
    function toggleAgentsSort() {
        agentsSort = agentsSort === "priority" ? "grouped" : "priority";
        Aeroport.uiSet("herdr_agents", agentsSort);
    }
    function sortedAgents(items) {
        if (agentsSort === "priority") {
            const rank = { blocked: 0, done: 1, working: 2, idle: 3 };
            return items.slice().sort((a, b) => (rank[a.status] ?? 4) - (rank[b.status] ?? 4)).map(a => Object.assign({}, a));
        }
        const out = [];
        let place = null;
        for (const a of items) {
            if (a.workspace !== place) {
                place = a.workspace;
                out.push({ id: "", heading: true, label: a.detail || "" });
            }
            out.push(Object.assign({}, a, { detail: "", indent: 0 }));
        }
        return out;
    }
    // Sidebar: herdr's sections, agents sorted (the panes are in the tab itself)
    readonly property var sidebarModel: {
        if (!modeState)
            return null;
        const v = tab;
        const sections = modeState.sections.map(sec => sec.key !== "agents" ? sec
            : Object.assign({}, sec, { items: sortedAgents(sec.items), choice: agentsSort === "priority" ? Aeroport.tr("Priority") : Aeroport.tr("Grouped") }));
        return Object.assign({}, modeState, { sections: sections });
    }

    // Context menus of herdr's tabs, workspaces, panes and agents (the entries of herdr's own right-click menus)
    property var menuTarget: null
    function herdrMenu(section, item, x, y) {
        menuTarget = { section: section, item: item, x: x, y: y };
        contextMenu.open(x, y, herdrEntries(section, item));
    }
    function herdrEntries(section, item) {
        let entries = [];
        if (section === "tabs")
            entries = [{ id: "rename-tab", label: Aeroport.tr("Rename Tab…") }, { id: "new-tab", label: Aeroport.tr("New Tab") },
                       { separator: true }, { id: "close-tab", label: Aeroport.tr("Close Tab") }];
        else if (section === "workspaces") {
            entries = [{ id: "rename-workspace", label: Aeroport.tr("Rename Workspace…") }, { separator: true },
                       { id: "new-worktree", label: Aeroport.tr("New Worktree…"), enabled: item.repo === true },
                       { id: "open-worktree", label: Aeroport.tr("Open Worktree…"), enabled: item.repo === true }];
            if (item.linked)
                entries.push({ id: "remove-worktree", label: Aeroport.tr("Delete Worktree Checkout…") });
            entries.push({ separator: true });
            if (item.group && !item.linked)
                entries.push({ id: "close-group", label: Aeroport.tr("Close Group…") });
            entries.push({ id: "close-workspace", label: Aeroport.tr("Close Workspace") });
        } else if (section === "panes" || section === "agents") {
            if (section === "agents")
                entries = [{ id: "rename-agent", label: Aeroport.tr("Rename Agent…") }, { id: "show-agent", label: Aeroport.tr("Show Agent") }, { separator: true }];
            entries = entries.concat([{ id: "rename-pane", label: Aeroport.tr("Rename Pane…") }, { id: "clear-pane-name", label: Aeroport.tr("Clear Pane Name") },
                       { separator: true }, { id: "swap-pane", label: Aeroport.tr("Swap with Focused Pane"), enabled: section === "panes" && !item.focused },
                       { id: "split-right", label: Aeroport.tr("Split Right") }, { id: "split-down", label: Aeroport.tr("Split Down") },
                       { separator: true }, { id: "close-pane", label: Aeroport.tr("Close Pane") }]);
        }
        return entries;
    }
    // The focused pane of the tab of a pane (swap)
    function focusedPaneOf(paneId) {
        for (const t of modeState ? modeState.tabs : [])
            if (t.panes.some(p => p.id === paneId))
                return t.pane;
        return "";
    }
    function herdrMenuAction(id) {
        const m = menuTarget;
        if (!m)
            return;
        const it = m.item || {};
        const ask = (title, text, done) => namePrompt.open(m.x, m.y, 240, title, text, done);
        if (id === "rename-tab") ask(Aeroport.tr("Name of the tab"), it.label, v => modeAct({ op: "rename-tab", id: it.id, label: v }));
        else if (id === "new-tab") modeAct({ op: "new-tab" });
        else if (id === "close-tab") modeAct({ op: "close-tab", id: it.id });
        else if (id === "rename-workspace") ask(Aeroport.tr("Name of the workspace"), it.label, v => modeAct({ op: "rename", section: "workspaces", id: it.id, label: v }));
        else if (id === "new-worktree") ask(Aeroport.tr("Branch of the new worktree"), "", v => modeAct({ op: "new-worktree", id: it.id, label: v }));
        else if (id === "open-worktree") modeAct({ op: "worktrees", id: it.id });
        else if (id === "remove-worktree") confirm(Aeroport.tr("Delete the checkout of %1?").replace("%1", it.label), Aeroport.tr("Delete"), () => modeAct({ op: "remove-worktree", id: it.id }));
        else if (id === "close-group") confirm(Aeroport.tr("Close %1 and its worktrees?").replace("%1", it.label), Aeroport.tr("Close"), () => modeAct({ op: "close", section: "workspaces", id: it.id, group: true }));
        else if (id === "close-workspace") modeAct({ op: "close", section: "workspaces", id: it.id });
        else if (id === "rename-agent") ask(Aeroport.tr("Name of the agent"), it.label, v => modeAct({ op: "rename", section: "agents", id: it.id, label: v }));
        else if (id === "show-agent") modeAct({ op: "select", section: "agents", id: it.id });
        else if (id === "rename-pane") ask(Aeroport.tr("Name of the pane"), it.label, v => modeAct({ op: "rename", section: "panes", id: it.id, label: v }));
        else if (id === "clear-pane-name") modeAct({ op: "rename", section: "panes", id: it.id });
        else if (id === "swap-pane") modeAct({ op: "swap", id: it.id, with: focusedPaneOf(it.id) });
        else if (id === "split-right" || id === "split-down") modeAct({ op: "split", id: it.id, direction: id === "split-right" ? "right" : "down" });
        else if (id === "close-pane") modeAct({ op: "close", section: "panes", id: it.id });
        else if (id.startsWith("worktree:")) modeAct({ op: "open-worktree", id: m.item.id, path: id.slice(9) });
        else if (id.startsWith("herdrkey:")) herdrKey(id.slice(9));
        else if (id.startsWith("space:")) modeAct({ op: "select", section: "workspaces", id: id.slice(6) });
    }
    Connections {
        target: modeClient
        function onReply(reply) {
            const r = JSON.parse(reply);
            if (r.op === "error")
                win.showError(r.message);
            else if (r.op === "worktrees" && win.menuTarget) {
                const items = (r.items || []).map(w => ({ id: "worktree:" + w.path, label: (w.branch || w.label) + (w.open_workspace_id ? " ·" : ""), enabled: true }));
                contextMenu.open(win.menuTarget.x, win.menuTarget.y, items.length ? items : [{ id: "", label: Aeroport.tr("No other worktree"), enabled: false }]);
            }
        }
    }
    // herdr's key bindings (its config's [keys], prefix Ctrl+B by default) in herdr's tabs: Aéroport shows herdr's panes
    // without herdr's client, so it answers them itself. Read when the mode starts and on reload_config.
    property var herdrKeys: []
    Connections {
        target: modeClient
        function onKindChanged() {
            win.herdrKeys = modeClient.kind === "herdr" ? JSON.parse(modeClient.keysJson()) : [];
            win.herdrCommands = modeClient.kind === "herdr" ? JSON.parse(modeClient.commandsJson()) : [];
        }
    }
    Instantiator {
        model: win.herdrKeys
        delegate: Shortcut {
            required property var modelData
            sequences: modelData.keys
            enabled: win.tab !== null && win.tab.modeTab !== "" && !contextMenu.visible && !namePrompt.visible && win.popup === null && !win.resizing
            onActivated: win.herdrKey(modelData.action)
        }
    }
    function modeSection(key) {
        const sec = modeState ? modeState.sections.find(x => x.key === key) : null;
        return sec ? sec.items : [];
    }
    function herdrKey(name) {
        const v = tab;
        if (!v || v.modeTab === "" || !modeState)
            return;
        const parts = name.split(":");
        const action = parts[0], n = parseInt(parts[1] || "0");
        const p = stack.mapToItem(null, FiliTheme.margin, FiliTheme.space4);
        const herdrTabs = tabs.filter(t => t.modeTab !== "");
        const here = herdrTabs.indexOf(v);
        const spaces = modeSection("workspaces"), agents = modeSection("agents");
        const space = spaces.find(w => w.active) || null;
        const pane = herdrPane(v);
        // An entry of a context menu, on an item, at the top left of the terminal
        const menuOn = (section, item, id) => { if (item) { menuTarget = { section: section, item: item, x: p.x, y: p.y }; herdrMenuAction(id) } };
        const showTab = t => { if (t) select(tabs.indexOf(t)) };
        const showSpace = w => { if (w) modeAct({ op: "select", section: "workspaces", id: w.id }) };
        const showAgent = a => { if (a) modeAct({ op: "select", section: "agents", id: a.id }) };
        const stepIn = (list, i, d) => list.length ? list[(i + d + list.length) % list.length] : null;
        if (action === "settings") Qt.openUrlExternally("file://" + modeClient.configPath());
        else if (action === "detach") modeClient.stop();
        else if (action === "reload_config") { modeAct({ op: "reload-config" }); herdrKeys = JSON.parse(modeClient.keysJson()); herdrCommands = JSON.parse(modeClient.commandsJson()) }
        else if (action === "open_notification_target") showAgent(agents.find(a => a.status === "blocked") || agents.find(a => a.status === "done"));
        else if (action === "workspace_picker" || action === "goto") {
            menuTarget = { section: "picker", item: null, x: p.x, y: p.y };
            contextMenu.open(p.x, p.y, spaces.map(w => ({ id: "space:" + w.id, label: (w.indent ? "    " : "") + w.label, checked: w.active })));
        }
        else if (action === "new_workspace") modeAct({ op: "new", section: "workspaces" });
        else if (action === "new_worktree") menuOn("workspaces", space, "new-worktree");
        else if (action === "open_worktree") menuOn("workspaces", space, "open-worktree");
        else if (action === "remove_worktree") menuOn("workspaces", space && space.linked ? space : null, "remove-worktree");
        else if (action === "rename_workspace") menuOn("workspaces", space, "rename-workspace");
        else if (action === "close_workspace") menuOn("workspaces", space, "close-workspace");
        else if (action === "previous_workspace" || action === "next_workspace") showSpace(stepIn(spaces, spaces.indexOf(space), action === "next_workspace" ? 1 : -1));
        else if (action === "switch_workspace") showSpace(spaces[n - 1]);
        else if (action === "previous_agent" || action === "next_agent") showAgent(stepIn(agents, agents.findIndex(a => a.active), action === "next_agent" ? 1 : -1));
        else if (action === "focus_agent") showAgent(agents[n - 1]);
        else if (action === "new_tab") modeAct({ op: "new-tab" });
        else if (action === "rename_tab") menuOn("tabs", { id: v.modeTab, label: v.modeLabel }, "rename-tab");
        else if (action === "previous_tab" || action === "next_tab") showTab(stepIn(herdrTabs, here, action === "next_tab" ? 1 : -1));
        else if (action === "move_tab_previous" || action === "move_tab_next") {
            const to = here + (action === "move_tab_next" ? 1 : -1);
            if (to >= 0 && to < herdrTabs.length) modeAct({ op: "move-tab", id: v.modeTab, index: to });
        }
        else if (action === "switch_tab") showTab(herdrTabs[n - 1]);
        else if (action === "close_tab") modeAct({ op: "close-tab", id: v.modeTab });
        else if (action === "rename_pane") menuOn("panes", pane, "rename-pane");
        else if (action === "cycle_pane_next") v.cycle(1);
        else if (action === "cycle_pane_previous" || action === "last_pane") v.cycle(-1);
        else if (action.startsWith("focus_pane_")) paneAction("focus-" + action.slice(11));
        else if (action.startsWith("resize_pane_")) paneAction("resize-" + action.slice(12));
        else if (action === "zoom") paneAction("zoom");
        else if (action === "split_vertical") split("right");
        else if (action === "split_horizontal") split("down");
        else if (action === "close_pane") closePane(view, true);
        else if (action.startsWith("swap_pane_")) swapPane(action.slice(10));
        else if (action === "resize_mode") resizing = true;
        else if (action === "edit_scrollback") { if (pane) modeAct({ op: "edit-scrollback", id: pane.id }) }
        else if (action === "clear_pane") { if (pane) modeAct({ op: "clear", id: pane.id }) }
        else if (action === "help") herdrHelp(p);
        else if (action === "command") runCommand(herdrCommands[n], pane, space);
        else if (action === "toggle_sidebar") toggleSidebar();
    }
    // herdr's custom commands ([[keys.command]] of its config): shell in the background, pane beside (in herdr), popup over
    // the tab (here)
    property var herdrCommands: []
    function runCommand(c, pane, space) {
        if (!c)
            return;
        const cwd = view ? view.cwd : "";
        const paneId = pane ? pane.id : "", ws = space ? space.id : "";
        const type = c.type || "popup";
        if (type === "shell")
            modeClient.runShell(c.command, cwd, paneId, ws);
        else if (type === "pane") {
            if (pane) modeAct({ op: "run-pane", id: pane.id, command: c.command });
        } else
            popup = { command: modeClient.commandLine(c.command, paneId, ws), cwd: cwd, width: c.width || "80%", height: c.height || "80%" };
    }
    // A popup of herdr: a terminal over the tab until its command ends
    property var popup: null
    function closePopup() {
        popup = null;
        refocus();
    }
    // Resize mode of herdr (prefix+r): arrows or h j k l move the active pane's dividers until Esc or Enter
    property bool resizing: false
    onCurrentChanged: resizing = false
    // Help of herdr (prefix+?): its bindings, by family, each runnable from the menu
    function herdrHelp(p) {
        const groups = [["Workspaces", /workspace|worktree|goto/], ["Tabs", /tab/], ["Panes", /pane|split|zoom|resize|scrollback/], ["Agents", /agent|notification/], ["Commands", /^command:/]];
        const label = k => {
            if (k.action.startsWith("command:")) {
                const c = herdrCommands[parseInt(k.action.slice(8))];
                return c ? (c.description || c.command) : k.action;
            }
            const t = k.action.replace(/:(\d)$/, " $1").replace(/_/g, " ");
            return t.charAt(0).toUpperCase() + t.slice(1);
        };
        const used = new Set();
        const entries = [];
        for (const [title, re] of groups.concat([["Other", /./]])) {
            const items = herdrKeys.filter(k => !used.has(k.action) && re.test(k.action) && k.action !== "help");
            items.forEach(k => used.add(k.action));
            if (items.length)
                entries.push({ id: "", label: Aeroport.tr(title), children: items.map(k => ({ id: "herdrkey:" + k.action, label: label(k), hint: k.keys[0] })) });
        }
        menuTarget = { section: "help", item: null, x: p.x, y: p.y };
        contextMenu.open(p.x, p.y, entries);
    }

    // A question in the footer before a close or a delete (Cancel, then the action), as the HIG's inline confirmation
    property var asking: null
    function confirm(text, action, done) {
        asking = { text: text, action: action, done: done };
    }
    // The question answered from the keyboard: Enter for its action (the default button), Escape cancels
    function answer(go) {
        const a = asking;
        asking = null;
        if (go && a)
            a.done();
        refocus();
    }
    Shortcut { sequences: ["Return", "Enter"]; enabled: win.asking !== null; onActivated: win.answer(true) }
    Shortcut { sequence: "Esc"; enabled: !win.resizing && (win.asking !== null || win.error !== ""); onActivated: { win.error = ""; win.answer(false) } }
    // Resize mode: the keys of herdr's resize mode, the footer says how to leave it
    Shortcut { sequences: ["Left", "H"]; enabled: win.resizing; onActivated: win.paneAction("resize-left") }
    Shortcut { sequences: ["Right", "L"]; enabled: win.resizing; onActivated: win.paneAction("resize-right") }
    Shortcut { sequences: ["Up", "K"]; enabled: win.resizing; onActivated: win.paneAction("resize-up") }
    Shortcut { sequences: ["Down", "J"]; enabled: win.resizing; onActivated: win.paneAction("resize-down") }
    Shortcut { sequences: ["Esc", "Return", "Enter", "Q"]; enabled: win.resizing; onActivated: win.resizing = false }
    // An error of herdr, in the footer for a few seconds
    property string error: ""
    function showError(text) {
        error = text;
        errorTimer.restart();
    }
    Timer { id: errorTimer; interval: 6000; onTriggered: win.error = "" }

    // Context for the assistant (Mod+Y): the terminal of the active window, a moment after each change
    Timer {
        id: contextTimer
        interval: 150
        onTriggered: if (win.active && win.view) Aeroport.publishContext(win.view.contextJson(), 0)
    }
    onActiveChanged: contextTimer.restart()
    onViewChanged: { contextTimer.restart(); if (searching && view !== searchView) closeSearch() }
    Connections {
        target: win.view
        function onBlocksRevisionChanged() { contextTimer.restart() }
        function onCwdChanged() { contextTimer.restart() }
    }

    // Search in the history (Ctrl+Shift+F): the bar at the top right of the terminal, the matches are lit in the grid
    property bool searching: false
    // The terminal searched (the bar acts on it; another pane or tab ends the search)
    property var searchView: null
    function openSearch() {
        searching = true;
        searchView = view;
        searchField.forceActiveFocus();
        searchField.selectAll();
        if (view && searchField.text !== "")
            view.search(searchField.text);
    }
    function closeSearch() {
        searching = false;
        searchField.text = "";
        if (searchView)
            searchView.searchClose();
        searchView = null;
        refocus();
    }

    function refocus() {
        if (popupLoader.item)
            popupLoader.item.forceActiveFocus();
        else if (view && !aboutCard.visible && !windowPanel.visible && !contextMenu.visible && !namePrompt.visible)
            view.forceActiveFocus();
    }

    // Commands of the fili-bar menu (fili-term ctl action NAME), for the active window
    function run(name) {
        const v = win.view;
        if (name === "about") aboutCard.show();
        // In herdr's tabs, a new tab of herdr
        else if (name === "new-tab") { if (win.tab && win.tab.modeTab !== "" && modeState) modeAct({ op: "new-tab" }); else win.addTab(0, v ? v.cwd : win.cwd, []) }
        else if (name === "new-window") Instance.newWindow(v ? v.cwd : win.cwd);
        else if (name === "close-tab") { if (win.tab) win.closeTab(win.tab, true) }
        else if (name === "close-pane") { if (v) win.closePane(v, true) }
        else if (name === "split-right") win.split("right");
        else if (name === "split-down") win.split("down");
        else if (name === "zoom-pane") win.paneAction("zoom");
        else if (name === "context-menu") { if (v) win.paneMenu(v, FiliTheme.space4, FiliTheme.space4) }
        else if (name.startsWith("focus-") || name.startsWith("resize-")) win.paneAction(name);
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
            t.closeAll();
        win.closed();
        win.destroy();
    }

    Component {
        id: termComponent
        TerminalView {
            id: term
            // The tab (Tab.qml) whose tree holds this pane
            property var paneTab: null
            // In a tab of herdr: the terminal of herdr attached here and its pane
            property string modeTerminal: ""
            property string modePane: ""
            // The user said "Not Now" to the offer: not asked again while the program stays in the foreground
            property string declined: ""
            onForegroundChanged: {
                const kind = win.modeKind(foreground);
                if (kind === "")
                    declined = "";
                else if (win.integrable(term) !== "" && win.modePolicies[kind] === "auto" && declined !== kind)
                    win.startMode(kind, term);
            }
            onActiveFocusChanged: if (activeFocus && paneTab) paneTab.setActive(term)
            onContextRequested: (x, y) => win.paneMenu(term, x, y)
            fontFamily: FiliTheme.mono
            fontPixelSize: win.fontSize
            padding: win.settings.padding
            lineSpacing: win.settings.line_spacing
            colors: FiliTheme.terminalColors
            onExited: if (!(win.hold && term.command.length > 0)) win.closePane(term, false)
            onBell: win.alert(0)
            // A long command ended while nobody watches this terminal: a notification with its result
            onCommandFinished: (command, exit, duration) => {
                const after = win.settings.notify_after;
                const watched = term.paneTab === win.tab && term.visible;
                if (term.paneTab !== win.tab) { term.paneTab.unseen = true; term.paneTab.lastExit = exit }
                if (after <= 0 || duration < after * 1000 || (win.active && watched))
                    return;
                const text = command.trim().replace(/\s+/g, " ");
                Aeroport.notify(exit === 0 ? Aeroport.tr("Command finished") : Aeroport.tr("Command failed (status %1)").replace("%1", exit),
                                (text.length > 100 ? text.slice(0, 100) + "…" : text) + " · " + Aeroport.duration(duration));
            }
        }
    }

    Component {
        id: tabComponent
        Tab {
            anchors.fill: parent
            maker: termComponent
            onUserFocused: v => win.paneFocused(v)
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
            anchors { left: parent.left; leftMargin: FiliTheme.margin; right: tools.left; rightMargin: searchBar.inHeader ? searchBar.width + searchNav.width + 2 * FiliTheme.space3 + FiliTheme.space1 : FiliTheme.space3; verticalCenter: parent.verticalCenter }
            text: win.tabTitle(win.tab)
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
                id: tabRepeater
                model: win.tabs.length
                Rectangle {
                    id: tab
                    required property int index
                    // Dragged along the row: follows the pointer, dropped in its new place
                    property real dragDx: 0
                    transform: Translate { x: tab.dragDx }
                    z: dragDx !== 0 ? 10 : 0
                    opacity: dragDx !== 0 ? 0.85 : 1
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
                    readonly property var pane: term ? term.active : null
                    readonly property string running: { if (!pane) return ""; pane.blocksRevision; return pane.runningCommand() }
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
                        acceptedButtons: Qt.LeftButton | Qt.MiddleButton | Qt.RightButton
                        property real pressX: 0
                        property bool moved: false
                        onPressed: e => { pressX = e.x; moved = false }
                        onPositionChanged: e => {
                            if (pressed && (e.buttons & Qt.LeftButton) && (moved || Math.abs(e.x - pressX) > 8)) {
                                moved = true;
                                tab.dragDx = e.x - pressX;
                            }
                        }
                        onReleased: e => {
                            if (moved)
                                win.dropTab(tab.index, tab.x + tab.width / 2 + tab.dragDx);
                            tab.dragDx = 0;
                        }
                        onClicked: e => {
                            if (moved)
                                return;
                            const t = win.tabs[tab.index];
                            if (e.button === Qt.RightButton) {
                                // herdr's tab menu on a tab of herdr
                                if (t.modeTab !== "") {
                                    const p = tabArea.mapToItem(null, e.x, e.y);
                                    win.herdrMenu("tabs", { id: t.modeTab, label: t.modeLabel }, p.x, p.y);
                                }
                            } else if (e.button === Qt.MiddleButton) {
                                win.closeTab(t, true);
                            } else {
                                win.select(tab.index);
                            }
                        }
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
            win.modeAct({ op: "select", section: section, id: id });
            win.refocus();
        }
        onAdded: section => { win.modeAct({ op: "new", section: section }); win.refocus() }
        onContextRequested: (section, item, x, y) => win.herdrMenu(section, item, x, y)
        onChoiceClicked: section => { if (section === "agents") win.toggleAgentsSort() }
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

    // Dividers of the tab shown: a line, and a wider band around it that takes the drag (the cursor says which way)
    Item {
        anchors.fill: stack
        z: 40
        Repeater {
            model: win.tab ? win.tab.dividerCount : 0
            Item {
                id: divider
                required property int index
                readonly property var d: win.tab ? win.tab.dividers[index] || null : null
                readonly property bool across: d !== null && d.dir === "right"
                readonly property int grab: 7
                x: d ? (across ? d.x - (grab - FiliTheme.borderWidth) / 2 : d.x) : 0
                y: d ? (across ? d.y : d.y - (grab - FiliTheme.borderWidth) / 2) : 0
                width: d ? (across ? grab : d.w) : 0
                height: d ? (across ? d.h : grab) : 0
                z: 20
                Rectangle {
                    x: divider.across ? (divider.grab - FiliTheme.borderWidth) / 2 : 0
                    y: divider.across ? 0 : (divider.grab - FiliTheme.borderWidth) / 2
                    width: divider.across ? FiliTheme.borderWidth : parent.width
                    height: divider.across ? parent.height : FiliTheme.borderWidth
                    color: drag.pressed || drag.containsMouse ? FiliTheme.accentColor : FiliTheme.border
                }
                MouseArea {
                    id: drag
                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: divider.across ? Qt.SplitHCursor : Qt.SplitVCursor
                    onPositionChanged: m => {
                        if (!pressed || !divider.d)
                            return;
                        const p = mapToItem(stack, m.x, m.y);
                        win.tab.setRatio(divider.d, divider.across ? p.x : p.y);
                    }
                    // A double click shares the space evenly again
                    onDoubleClicked: if (divider.d) { divider.d.node.ratio = 0.5; win.tab.layout(); win.dividerMoved(divider.d) }
                    onPressed: win.tab.dragging = true
                    onReleased: { win.tab.dragging = false; win.dividerMoved(divider.d); win.tab.focusActive() }
                }
            }
        }
    }

    // Over each pane of the tab shown: its scroll bar, files dropped on it, the actions of the command block under the
    // pointer
    Item {
        anchors.fill: stack
        z: 30
        Repeater {
            model: win.tab ? win.tab.leaves : []
            Item {
                id: pane
                required property var modelData
                readonly property var v: modelData
                x: v.x
                y: v.y
                width: v.width
                height: v.height
                visible: v.visible
                // The pane of herdr shown here (its name, its agent)
                readonly property var info: { win.modeState; return win.herdrPaneOf(v) }

                // Name of a pane of herdr (given in herdr, or its agent's) at its top right, when the tab has several:
                // it steps aside while the pointer is over it
                Rectangle {
                    id: nameChip
                    visible: pane.info !== null && pane.info.named === true && win.tab !== null && win.tab.leaves.length > 1 && !win.tab.zoomed
                    anchors { top: parent.top; right: parent.right; topMargin: FiliTheme.space1; rightMargin: FiliTheme.space3 }
                    z: 2
                    width: Math.min(nameText.implicitWidth + 2 * FiliTheme.space2, parent.width / 2)
                    height: 20
                    radius: FiliTheme.radiusControl
                    color: FiliTheme.alpha(FiliTheme.backgroundDeep, 0.85)
                    border.width: FiliTheme.borderWidth
                    border.color: pane.v === win.view ? FiliTheme.accentColor : FiliTheme.border
                    opacity: chipHover.containsMouse ? 0.15 : 1
                    Behavior on opacity { NumberAnimation { duration: 120 } }
                    Text {
                        id: nameText
                        anchors { fill: parent; leftMargin: FiliTheme.space2; rightMargin: FiliTheme.space2 }
                        verticalAlignment: Text.AlignVCenter
                        text: pane.info ? pane.info.label : ""
                        elide: Text.ElideRight
                        font.family: FiliTheme.font
                        font.pixelSize: FiliTheme.sizeSmall
                        color: pane.v === win.view ? FiliTheme.text : FiliTheme.textSecondary
                    }
                    MouseArea { id: chipHover; anchors.fill: parent; hoverEnabled: true; acceptedButtons: Qt.NoButton }
                }

                // Scroll bar of the history: a thin handle at the right edge, shown while scrolling, on hover and when dragged
                Item {
                    id: scrollBar
                    readonly property var v: pane.v
                    readonly property int history: v ? v.historySize : 0
                    readonly property int rows: v ? Math.max(1, v.lines) : 1
                    readonly property real thumbHeight: Math.max(32, Math.min(height, height * rows / (rows + history)))
                    // 0 at the top of the history, 1 at the prompt
                    readonly property real fraction: history > 0 ? 1 - v.scrollOffset / history : 1
                    anchors { right: parent.right; top: parent.top; bottom: parent.bottom }
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
                    anchors.fill: parent
                    keys: ["text/uri-list"]
                    onDropped: drop => {
                        if (drop.hasUrls && pane.v) {
                            pane.v.dropPaths(drop.urls.join("\n"));
                            drop.accept();
                            pane.v.forceActiveFocus();
                        }
                    }
                }

                // Command blocks: actions on the block under the pointer, at the right of its first line (pinned to the top of
                // the view while that line is scrolled out). Takes no button: clicks and the wheel go on to the terminal.
                MouseArea {
                    id: blockHover
                    anchors.fill: parent
                    hoverEnabled: true
                    acceptedButtons: Qt.NoButton
                    property var block: null
                    // A link under the pointer with Ctrl held: underlined, hand cursor
                    property bool link: false
                    cursorShape: link ? Qt.PointingHandCursor : Qt.ArrowCursor
                    function look() {
                        const v = pane.v;
                        const j = v && containsMouse ? v.blockAt(mouseY) : "";
                        block = j ? JSON.parse(j) : null;
                    }
                    onPositionChanged: mouse => { look(); link = pane.v !== null && pane.v.linkAt(mouseX, mouseY, (mouse.modifiers & Qt.ControlModifier) !== 0) }
                    onContainsMouseChanged: { look(); if (!containsMouse) { link = false; if (pane.v) pane.v.linkAt(0, 0, false) } }
                    Connections {
                        target: pane.v
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
                                onClicked: pane.v.copyBlock(blockBar.b.id, 0)
                            }
                            IconButton {
                                id: copyCmd
                                readonly property bool hovered: containsMouse
                                name: "terminal"
                                tip: Aeroport.tr("Copy Command")
                                onClicked: pane.v.copyCommand(blockBar.b.id)
                            }
                            IconButton {
                                id: rerun
                                readonly property bool hovered: containsMouse
                                name: "refresh"
                                tip: Aeroport.tr("Run Again")
                                enabledState: pane.v !== null && blockBar.b !== null && pane.v.blocksRevision >= 0 && pane.v.canRerun()
                                onClicked: pane.v.rerunBlock(blockBar.b.id)
                            }
                            IconButton {
                                id: edit
                                readonly property bool hovered: containsMouse
                                name: "compose"
                                tip: Aeroport.tr("Edit Output")
                                enabledState: blockBar.b !== null && blockBar.b.outputTop >= 0
                                onClicked: pane.v.editBlock(blockBar.b.id)
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
                                onClicked: win.askRob(blockBar.b.id, pane.v)
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
            }
        }
    }

    // Footer: where the shell started, size of the grid or place in the history; a confirmation or an error takes it whole
    Rectangle {
        id: footer
        anchors { left: parent.left; right: parent.right; bottom: parent.bottom }
        visible: win.footerShown
        height: win.footerShown ? FiliTheme.statusHeight : 0
        color: FiliTheme.backgroundDeep
        Rectangle { anchors { left: parent.left; right: parent.right; top: parent.top } height: FiliTheme.borderWidth; color: FiliTheme.border }
        Rectangle {
            id: question
            visible: win.asking !== null || win.error !== "" || win.resizing
            anchors { fill: parent; topMargin: FiliTheme.borderWidth }
            z: 5
            color: FiliTheme.backgroundDeep
            Text {
                anchors { left: parent.left; leftMargin: FiliTheme.margin; right: answers.left; rightMargin: FiliTheme.space4; verticalCenter: parent.verticalCenter }
                text: win.asking ? win.asking.text : win.resizing ? Aeroport.tr("Resizing the pane: arrows or H J K L, Esc when done") : win.error
                elide: Text.ElideRight
                font.family: FiliTheme.font
                font.pixelSize: FiliTheme.sizeSmall
                color: win.asking || win.resizing ? FiliTheme.text : FiliTheme.danger
            }
            Row {
                id: answers
                anchors { right: parent.right; rightMargin: FiliTheme.margin; verticalCenter: parent.verticalCenter }
                spacing: FiliTheme.space4
                Repeater {
                    model: win.asking ? [{ label: Aeroport.tr("Cancel"), go: false }, { label: win.asking.action, go: true }] : win.resizing ? [{ label: Aeroport.tr("Done"), go: false }] : [{ label: Aeroport.tr("OK"), go: false }]
                    Text {
                        required property var modelData
                        anchors.verticalCenter: parent.verticalCenter
                        text: modelData.label
                        font.family: FiliTheme.font
                        font.pixelSize: FiliTheme.sizeSmall
                        font.weight: modelData.go ? FiliTheme.weightAccent : Font.Normal
                        color: modelData.go ? FiliTheme.danger : answerArea.containsMouse ? FiliTheme.text : FiliTheme.textSecondary
                        MouseArea {
                            id: answerArea
                            anchors { fill: parent; margins: -4 }
                            hoverEnabled: true
                            cursorShape: Qt.PointingHandCursor
                            onClicked: {
                                // Read before the question goes: clearing it destroys this button
                                const a = win.asking, go = parent.modelData.go;
                                win.resizing = false;
                                win.asking = null;
                                win.error = "";
                                if (go && a)
                                    a.done();
                                win.refocus();
                            }
                        }
                        Accessible.role: Accessible.Button
                        Accessible.name: text
                    }
                }
            }
        }
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

    // Popup of herdr over the tab: the rest dimmed and out of reach until the command ends (a cell count or a percentage)
    Rectangle {
        anchors.fill: stack
        visible: win.popup !== null
        z: 70
        color: FiliTheme.alpha(FiliTheme.backgroundDeep, 0.55)
        MouseArea { anchors.fill: parent; acceptedButtons: Qt.AllButtons; onWheel: w => w.accepted = true }
        Rectangle {
            id: popupFrame
            readonly property real cellW: win.view && win.view.columns > 0 ? win.view.width / win.view.columns : win.fontSize * 0.6
            readonly property real cellH: win.view && win.view.lines > 0 ? win.view.height / win.view.lines : win.fontSize * 1.2
            function popupSize(spec, cell, whole) {
                const t = String(spec || "80%");
                const v = t.endsWith("%") ? whole * parseInt(t) / 100 : parseInt(t) * cell + 2 * win.settings.padding;
                return Math.max(120, Math.min(whole - 2 * FiliTheme.space4, isNaN(v) ? whole * 0.8 : v));
            }
            width: win.popup ? popupFrame.popupSize(win.popup.width, popupFrame.cellW, parent.width) : 0
            height: win.popup ? popupFrame.popupSize(win.popup.height, popupFrame.cellH, parent.height) : 0
            anchors.centerIn: parent
            radius: FiliTheme.radiusControl
            color: FiliTheme.background
            border.width: FiliTheme.borderWidth
            border.color: FiliTheme.border
            Loader {
                id: popupLoader
                anchors { fill: parent; margins: FiliTheme.borderWidth }
                active: win.popup !== null
                sourceComponent: TerminalView {
                    cwd: win.popup ? win.popup.cwd : ""
                    command: win.popup ? win.popup.command : []
                    fontFamily: FiliTheme.mono
                    fontPixelSize: win.fontSize
                    padding: win.settings.padding
                    lineSpacing: win.settings.line_spacing
                    colors: FiliTheme.terminalColors
                    onExited: win.closePopup()
                    Component.onCompleted: forceActiveFocus()
                }
            }
        }
    }

    ContextMenu {
        id: contextMenu
        onActivated: id => win.menuAction(id)
        onVisibleChanged: if (!visible && !namePrompt.visible) win.refocus()
    }
    TextPrompt {
        id: namePrompt
        onVisibleChanged: if (!visible) win.refocus()
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
