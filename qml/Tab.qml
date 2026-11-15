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
    // A tab of herdr: its tab id, the terminal attached, the pane chosen in the sidebar ("" the focused one), its label
    // and the status of its agents
    property string modeTab: ""
    property string modeTerminal: ""
    property string modePane: ""
    property string modeLabel: ""
    property string modeStatus: ""

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
    // Another program in place of every pane (a tab of herdr attached to another terminal)
    function resetTo(props) {
        const focused = active !== null && active.activeFocus;
        for (const v of leaves) {
            v.closeSession();
            v.destroy();
        }
        zoomed = null;
        start(props);
        if (focused)
            focusActive();
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
        layout();
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
        const f = active ? find(active) : null;
        if (!f)
            return null;
        const v = newView(props);
        const n = f.node;
        const old = { view: n.view };
        delete n.view;
        n.dir = dir;
        n.ratio = 0.5;
        n.a = old;
        n.b = { view: v };
        zoomed = null;
        active = v;
        layout();
        v.forceActiveFocus();
        return v;
    }
    // Take a terminal out of the tree (its sibling takes the place of the split); false when it is the last one
    function remove(v) {
        const f = find(v);
        if (!f || !f.parent)
            return false;
        const p = f.parent;
        const other = p.a === f.node ? p.b : p.a;
        for (const k of ["view", "dir", "ratio", "a", "b"])
            delete p[k];
        Object.assign(p, other);
        if (zoomed === v)
            zoomed = null;
        if (active === v)
            active = neighbour(v, "", p) || firstLeaf(p);
        v.destroy();
        layout();
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
        if (o) {
            active = o;
            layout();
            o.forceActiveFocus();
        }
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
