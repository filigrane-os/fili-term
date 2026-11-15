// A tab of a window: a tree of splits whose leaves are terminals, each with its own session of the daemon. A node is
// a leaf { view } or a split { dir: "right" | "down", ratio, a, b } (a: left or top, b: right or bottom). The tab lays
// the leaves out itself, draws the dividers between them (dragged to resize) and dims the panes without the focus.
import QtQuick
import Fili.Term

Item {
    id: tab

    // Makes one terminal (Main.qml's component), given its properties
    property Component maker
    property var root: null
    // The pane with the keyboard; the one zoomed to the whole tab (null: none)
    property var active: null
    property var zoomed: null
    // Leaves in reading order, and the dividers, both recomputed by layout()
    property var leaves: []
    property var dividers: []
    property int dividerCount: 0
    property bool shown: false
    // Result of a command that ended while this tab was not shown, until the tab is selected
    property bool unseen: false
    property int lastExit: 0
    // A tab of herdr: its tab id, its label and the status of its agents (its panes come from herdr's layout, each
    // terminal attached to one pane: modePane / modeTerminal of the view)
    // Workspace of the tab (a project folder: the git root of its first pane); "" for the tabs of herdr
    property string workspace: ""
    // Typing in every pane: what is typed in one pane goes to all the others of the tab
    property bool broadcast: false
    property string modeTab: ""
    property string modeLabel: ""
    property string modeStatus: ""
    // A divider is being dragged: the ratios herdr sends meanwhile do not move it back
    property bool dragging: false
    // The user gave the keyboard to another pane (click, keys): herdr's focus follows in a tab of herdr
    signal userFocused(var view)
    // The pane the user chose last and when: a state of herdr older than that choice does not take the keyboard back
    property var chosen: null
    property real chosenAt: 0

    // Thickness of a divider line, and the smallest pane (pixels) a drag or a resize leaves
    readonly property int line: FiliTheme.borderWidth
    readonly property int smallest: 80

    onWidthChanged: layout()
    onHeightChanged: layout()

    function newView(props) {
        const v = maker.createObject(tab, Object.assign({ paneTab: tab, visible: false }, props));
        return v;
    }
    // The first terminal of the tab
    function start(props) {
        const v = newView(props);
        root = { view: v };
        active = v;
        layout();
        return v;
    }
    // The layout to keep (windows.json): leaves {session, cwd, name, active}, splits {dir, ratio, a, b}
    function describe(n) {
        if (n.view)
            return { session: n.view.sessionId, cwd: n.view.cwd, name: n.view.paneName || "", active: n.view === active };
        return { dir: n.dir, ratio: n.ratio, a: describe(n.a), b: describe(n.b) };
    }
    // Bring a saved layout back: a pane attaches to its session when the daemon still has it (`live`: ids), else it is a
    // fresh shell in the saved folder
    function restoreTree(saved, live) {
        const build = n => {
            if (n.dir)
                return { dir: n.dir === "down" ? "down" : "right", ratio: Math.max(0.1, Math.min(0.9, n.ratio || 0.5)), a: build(n.a), b: build(n.b) };
            const v = newView({ sessionId: live.indexOf(n.session) >= 0 ? n.session : 0, cwd: n.cwd || "", command: [], paneName: n.name || "" });
            if (n.active)
                active = v;
            return { view: v };
        };
        root = build(saved);
        if (!active)
            active = firstLeaf(root);
        layout();
    }
    function closeAll() {
        for (const v of leaves)
            v.closeSession();
    }
    function focusActive() {
        if (active)
            active.forceActiveFocus();
    }
    function setShown(on) {
        shown = on;
        layout();
    }
    // A pane took the keyboard (click, focus move)
    function setActive(v) {
        if (v === active || leaves.indexOf(v) < 0)
            return;
        active = v;
        chosen = v;
        chosenAt = Date.now();
        layout();
        userFocused(v);
    }

    // The node holding a terminal and its parent split
    function find(v, n, parent) {
        n = n || root;
        if (!n)
            return null;
        if (n.view)
            return n.view === v ? { node: n, parent: parent || null } : null;
        return find(v, n.a, n) || find(v, n.b, n);
    }
    function firstLeaf(n) {
        return n.view ? n.view : firstLeaf(n.a);
    }

    // Split the active pane: the new terminal goes right of it or below it and takes the keyboard
    function split(dir, props) {
        return active ? insert(newView(props), dir) : null;
    }
    // Put a terminal beside the active pane (a new one, or one moved from another tab), with the keyboard
    function insert(v, dir) {
        if (v.parent !== tab) {
            v.parent = tab;
            v.paneTab = tab;
        }
        if (!root) {
            root = { view: v };
        } else {
            const f = active ? find(active) : null;
            const n = f ? f.node : root;
            const old = Object.assign({}, n);
            for (const k of ["view", "dir", "ratio", "a", "b", "path"])
                delete n[k];
            n.dir = dir;
            n.ratio = 0.5;
            n.a = old;
            n.b = { view: v };
        }
        zoomed = null;
        active = v;
        layout();
        if (shown)
            v.forceActiveFocus();
        return v;
    }
    // Take a terminal out of the tree, alive (its sibling takes the place of the split); false when it is the last one
    function take(v) {
        const f = find(v);
        if (!f || !f.parent)
            return false;
        const p = f.parent;
        const other = p.a === f.node ? p.b : p.a;
        for (const k of ["view", "dir", "ratio", "a", "b", "path"])
            delete p[k];
        Object.assign(p, other);
        if (zoomed === v)
            zoomed = null;
        if (chosen === v)
            chosen = null;
        if (active === v)
            active = firstLeaf(p);
        layout();
        return true;
    }
    // The same, and the terminal ends
    function remove(v) {
        if (!take(v))
            return false;
        v.destroy();
        return true;
    }

    // The pane next to a terminal in a direction (left, right, up, down): among those across the edge and overlapping
    // it on the other axis, the closest one, then the one whose middle is nearest
    function neighbour(v, dir, within) {
        if (dir === "")
            return within ? firstLeaf(within) : null;
        const r = { x: v.x, y: v.y, w: v.width, h: v.height };
        let best = null, bestScore = 1e9;
        for (const o of leaves) {
            if (o === v)
                continue;
            const horizontal = dir === "left" || dir === "right";
            const gap = dir === "left" ? r.x - (o.x + o.width) : dir === "right" ? o.x - (r.x + r.w)
                      : dir === "up" ? r.y - (o.y + o.height) : o.y - (r.y + r.h);
            if (gap < -2)
                continue;
            const overlap = horizontal ? Math.min(r.y + r.h, o.y + o.height) - Math.max(r.y, o.y)
                                       : Math.min(r.x + r.w, o.x + o.width) - Math.max(r.x, o.x);
            if (overlap <= 0)
                continue;
            const middle = horizontal ? Math.abs((o.y + o.height / 2) - (r.y + r.h / 2)) : Math.abs((o.x + o.width / 2) - (r.x + r.w / 2));
            const score = gap * 10000 + middle;
            if (score < bestScore) {
                bestScore = score;
                best = o;
            }
        }
        return best;
    }
    function focusTowards(dir) {
        if (!active)
            return;
        zoomed = null;
        layout();
        const o = neighbour(active, dir);
        if (o)
            o.forceActiveFocus();
    }
    // The next or previous pane in reading order
    function cycle(delta) {
        const i = leaves.indexOf(active);
        if (i >= 0 && leaves.length > 1)
            leaves[(i + delta + leaves.length) % leaves.length].forceActiveFocus();
    }

    // The panes of a tab of herdr from herdr's layout: a tree of {dir, ratio, path, a, b} and panes {id, terminal, cwd}.
    // `pool` holds the terminals already attached in the window's herdr tabs, by terminal id: a pane takes its own (even
    // from another tab, when it moved: its history goes with it), a new pane gets one (props(pane) gives its
    // properties). What stays in the pool is the caller's to end. focusedPane takes the keyboard, zoomedTab shows it alone.
    function syncTree(tree, focusedPane, zoomedTab, props, pool) {
        const old = pool;
        const ratios = {};
        const keep = n => {
            if (n && !n.view) {
                ratios[JSON.stringify(n.path || [])] = n.ratio;
                keep(n.a);
                keep(n.b);
            }
        };
        if (dragging)
            keep(root);
        const used = [];
        const build = n => {
            if (!n)
                return null;
            if (n.dir) {
                const a = build(n.a), b = build(n.b);
                if (!a || !b)
                    return a || b;
                const k = JSON.stringify(n.path || []);
                return { dir: n.dir, ratio: k in ratios ? ratios[k] : n.ratio, path: n.path || [], a: a, b: b };
            }
            if (!n.terminal)
                return null;
            let v = old[n.terminal];
            if (v) {
                delete old[n.terminal];
                if (v.paneTab !== tab) {
                    v.parent = tab;
                    v.paneTab = tab;
                }
            } else {
                v = newView(Object.assign(props(n), { modeTerminal: n.terminal }));
            }
            v.modePane = n.id;
            used.push(v);
            return { view: v };
        };
        const next = build(tree);
        if (!next)
            return false;
        const hadFocus = active !== null && active.activeFocus;
        root = next;
        // herdr confirmed the choice: its own focus moves are followed again
        if (chosen !== null && chosen.modePane === focusedPane)
            chosen = null;
        const recent = chosen !== null && used.indexOf(chosen) >= 0 && Date.now() - chosenAt < 1500;
        active = recent ? chosen : used.find(v => v.modePane === focusedPane) || used[0];
        zoomed = zoomedTab && used.length > 1 ? active : null;
        layout();
        if (hadFocus)
            focusActive();
        return true;
    }
    // A new terminal in the place of one that ended (a pane of herdr whose client left: attached again), same focus
    function renew(v, props) {
        const f = find(v);
        if (!f)
            return null;
        const n = newView(props);
        f.node.view = n;
        if (active === v)
            active = n;
        if (zoomed === v)
            zoomed = n;
        if (chosen === v)
            chosen = n;
        const had = v.activeFocus;
        v.destroy();
        layout();
        if (had || (shown && active === n))
            n.forceActiveFocus();
        return n;
    }
    // Exchange the active pane with its neighbour in a direction; the keyboard stays with the same terminal
    function swap(dir) {
        const o = active ? neighbour(active, dir) : null;
        if (!o)
            return;
        const a = find(active).node, b = find(o).node;
        const t = a.view;
        a.view = b.view;
        b.view = t;
        layout();
        active.forceActiveFocus();
    }
    function toggleZoom() {
        zoomed = zoomed === null && leaves.length > 1 ? active : null;
        layout();
        focusActive();
    }
    // The splits from the root down to a terminal
    function pathTo(v) {
        const walk = (n, acc) => {
            if (n.view)
                return n.view === v ? acc : null;
            return walk(n.a, acc.concat([n])) || walk(n.b, acc.concat([n]));
        };
        return root ? walk(root, []) || [] : [];
    }
    // Move the nearest divider of the active pane on that axis, as tmux's resize-pane: left and up move it towards the
    // start, right and down towards the end
    function resize(dir, step) {
        if (!active)
            return;
        const want = dir === "left" || dir === "right" ? "right" : "down";
        const split = pathTo(active).reverse().find(p => p.dir === want);
        const d = split ? dividers.find(x => x.node === split) : null;
        if (d)
            setRatio(d, d.at + (dir === "left" || dir === "up" ? -step : step));
    }
    // A divider dragged to a position (pixels from the tab's left or top)
    function setRatio(d, pos) {
        const span = d.length - line;
        if (span <= 2 * smallest)
            return;
        const at = Math.max(smallest, Math.min(span - smallest, pos - d.origin));
        d.node.ratio = at / span;
        layout();
    }

    // Place every leaf: the zoomed one alone, else the tree in the tab's rectangle
    function layout() {
        if (!root)
            return;
        const ls = [], ds = [];
        const place = (n, x, y, w, h) => {
            if (n.view) {
                ls.push(n.view);
                n.view.x = x;
                n.view.y = y;
                n.view.width = Math.max(1, w);
                n.view.height = Math.max(1, h);
                return;
            }
            const across = n.dir === "right";
            const length = across ? w : h;
            const first = Math.round((length - line) * n.ratio);
            if (across) {
                ds.push({ node: n, dir: n.dir, x: x + first, y: y, w: line, h: h, origin: x, length: w, at: x + first });
                place(n.a, x, y, first, h);
                place(n.b, x + first + line, y, w - first - line, h);
            } else {
                ds.push({ node: n, dir: n.dir, x: x, y: y + first, w: w, h: line, origin: y, length: h, at: y + first });
                place(n.a, x, y, w, first);
                place(n.b, x, y + first + line, w, h - first - line);
            }
        };
        place(root, 0, 0, width, height);
        const z = zoomed !== null && ls.indexOf(zoomed) >= 0 ? zoomed : null;
        if (z) {
            z.x = 0;
            z.y = 0;
            z.width = width;
            z.height = height;
        }
        for (const v of ls) {
            const on = shown && (z === null || v === z);
            v.visible = on;
            v.setShown(on);
            // The panes without the keyboard are dimmed, so the eye finds the one that types
            v.opacity = ls.length > 1 && z === null && v !== active ? 0.6 : 1;
        }
        leaves = ls;
        dividers = z ? [] : ds;
        dividerCount = dividers.length;
    }
}
