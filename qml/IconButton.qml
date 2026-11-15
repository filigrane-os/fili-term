// Toolbar icon button: hover surfaceHover, pressed surfaceActive, disabled muted. `tip` is its name: the accessible name
// and a small label under the button after the pointer rested on it (drawn by the window, `showTip` / `hideTip`: Qt
// Quick Controls would cost the window process ~3 MB), with `hint` (its key) after it
import QtQuick
import QtQuick.Window
import Fili.Term

Rectangle {
    id: b
    // Line icon (Icon.qml)
    property string name
    property bool filled: false
    property string tip
    property string hint
    property bool enabledState: true
    // Toggle state (filter on): drawn with the active surface
    property bool toggled: false
    readonly property bool containsMouse: area.containsMouse
    signal clicked()

    width: 28
    height: 24
    radius: FiliTheme.radiusControl
    color: !enabledState ? "transparent" : area.pressed || toggled ? FiliTheme.surfaceActive : area.containsMouse ? FiliTheme.surfaceHover : "transparent"
    activeFocusOnTab: false

    Icon {
        anchors.centerIn: parent
        name: b.name
        filled: b.filled
        size: 16
        color: b.enabledState ? FiliTheme.text : FiliTheme.muted
    }
    Accessible.role: Accessible.Button
    Accessible.name: b.tip
    // A button out of use still says what it is
    Timer {
        id: tipTimer
        interval: 450
        onTriggered: {
            const w = Window.window;
            if (w && w.showTip && b.tip !== "")
                w.showTip(b, b.hint !== "" ? b.tip + "   " + b.hint : b.tip);
        }
    }
    function hideTip() {
        tipTimer.stop();
        const w = Window.window;
        if (w && w.hideTip)
            w.hideTip(b);
    }
    onVisibleChanged: if (!visible) hideTip()
    MouseArea {
        id: area
        anchors.fill: parent
        hoverEnabled: true
        onContainsMouseChanged: containsMouse ? tipTimer.restart() : b.hideTip()
        onPressed: b.hideTip()
        onClicked: if (b.enabledState) b.clicked()
    }
}
