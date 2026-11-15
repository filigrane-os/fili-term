// Root of the window process: no window of its own, opens one Main.qml window per request (Instance.openRequested)
// and hands the commands of the fili-bar menu to the active window.
import QtQuick
import QtQuick.Window
import Fili.Term

QtObject {
    id: root

    property var windows: []
    // Fonts embedded in the binary (fonts/, licenses/): same look on every distribution
    property FontLoader ui: FontLoader { source: "qrc:/qt/qml/Fili/Term/fonts/Jost-Variable.ttf" }
    property FontLoader mono: FontLoader { source: "qrc:/qt/qml/Fili/Term/fonts/JetBrainsMono-Variable.ttf" }
    property Component windowComponent: Component { Main {} }
    property Connections requests: Connections {
        target: Instance
        function onOpenRequested(attach, cwd, hold, tab, title, command, mode, plain) {
            // A bare `fili-term` in a process with no window yet: the windows of the last run come back
            if (plain && root.windows.length === 0 && !root.restored) {
                root.restored = true;
                if (root.restoreWindows())
                    return;
            }
            // --tab: a tab in the window being used, when there is one
            const open = tab ? (root.windows.find(x => x.active) || root.windows[root.windows.length - 1]) : null;
            if (open) {
                if (mode !== "")
                    open.startMode(mode, null);
                else
                    // From another program (a script, a shell): the workspace of its folder's project
                    open.addTab(attach, cwd, command, cwd !== "" ? Aeroport.projectRoot(cwd) : "");
                open.requestActivate();
                return;
            }
            root.addWindow({ attachId: attach, cwd: cwd, command: command, hold: hold, initialTitle: title, initialMode: mode });
        }
    }
    property bool restored: false
    function addWindow(props) {
        const w = root.windowComponent.createObject(root, props);
        if (!w)
            return null;
        root.windows = root.windows.concat([w]);
        // A window closed on purpose is forgotten at once; the others stay for the next run
        w.closed.connect(() => { root.windows = root.windows.filter(x => x !== w); root.save() });
        return w;
    }
    // Bring back the windows saved by the last run (windows.json): panes attach to the sessions the daemon still has,
    // else they are fresh shells in the saved folders. False when there is nothing to bring back.
    function restoreWindows() {
        if (JSON.parse(Aeroport.settingsJson()).restore === "never")
            return false;
        let saved = [];
        try { saved = JSON.parse(Aeroport.savedWindows()) } catch (e) { saved = [] }
        if (!Array.isArray(saved) || saved.length === 0)
            return false;
        const live = JSON.parse(Aeroport.liveSessions());
        for (const s of saved)
            root.addWindow({ restore: JSON.stringify(s), restoreLive: live });
        return root.windows.length > 0;
    }
    // The layout of every open window, written when it changed (looked at every two seconds, so a burst of changes
    // makes one write)
    property string savedLayout: ""
    function save() {
        if (Instance.standalone())
            return;
        const parts = root.windows.map(w => w.layoutJson()).filter(j => j !== "");
        const text = "[" + parts.join(",") + "]";
        if (text !== root.savedLayout) {
            root.savedLayout = text;
            Aeroport.saveWindows(text);
        }
    }
    property Timer saver: Timer {
        interval: 2000
        repeat: true
        running: root.windows.length > 0
        onTriggered: root.save()
    }
    // The bar acts on the focused window; the last one created when none has the focus (a command run from a script)
    property Connections commands: Connections {
        target: Aeroport
        function onCommand(name) {
            // An agent's hook (fili-term report): the window holding that session; a click on an agent's notification
            const parts = name.split("\t");
            if (parts[0] === "report") {
                root.windows.some(x => x.agentReport(parseInt(parts[1]), parts[2], parts[3] || ""));
                return;
            }
            if (parts[0] === "show-session") {
                root.windows.some(x => x.showSession(parseInt(parts[1])));
                return;
            }
            const w = root.windows.find(x => x.active) || root.windows[root.windows.length - 1];
            if (w)
                w.run(name);
        }
    }

    Component.onCompleted: Instance.start()
}
