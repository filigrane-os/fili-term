pragma Singleton
// Tokens of the fili-shell (dotfiles/config/quickshell/FiliTheme.qml), without Quickshell: same names; default values
// identical to theme.toml of the other fili-* applications, customizable. Light mode followed live (fili-theme state).
import QtQuick
import Fili.Term

QtObject {
    readonly property bool light: Aeroport.light
    readonly property string accent: Aeroport.accent

    // Merged theme: defaults of theme.toml overridden by ~/.config/filigrane/fili-term.toml (sections [theme],
    // [theme.dark], [theme.light], [theme.accents]); the keys are in English, the token names are the shell's
    readonly property var theme: JSON.parse(Aeroport.themeJson())
    readonly property var m: light ? theme.light : theme.dark
    readonly property color background: m.background
    readonly property color backgroundDeep: m.background_deep
    readonly property color surface: m.surface
    readonly property color surfaceHover: m.surface_hover
    readonly property color surfaceActive: m.surface_active
    readonly property color border: m.border
    readonly property color muted: m.muted
    readonly property color textSecondary: m.text_secondary
    // Accent color (fili-theme accent): [dark, dark bright, light, light bright]; default = text color in dark mode, black (background) in light mode.
    // An `accent` set in the mode takes precedence.
    readonly property var accentTones: theme.accents
    readonly property var tone: accentTones[accent] || accentTones.default
    readonly property color accentColor: m.accent || (light ? tone[2] : tone[0])
    readonly property color accentBright: m.accent_bright || (light ? tone[3] : tone[1])
    readonly property color text: m.text
    readonly property color warning: m.warning
    readonly property color danger: m.danger
    function alpha(c, a) {
        return Qt.rgba(c.r, c.g, c.b, a);
    }

    readonly property string font: m.font
    readonly property string mono: m.mono
    readonly property string icons: m.icons
    readonly property int size: m.size
    readonly property int sizeList: m.size_list
    readonly property int sizeSmall: m.size_small
    readonly property int sizeLabel: m.size_label
    readonly property int sizeTitle: m.size_title
    readonly property int weightAccent: m.weight_accent
    readonly property int radiusMark: m.radius_mark
    readonly property int radiusControl: m.radius_control
    readonly property int radiusWindow: m.radius_window
    readonly property int borderWidth: m.border_width
    readonly property int space1: m.space_1
    readonly property int space2: m.space_2
    readonly property int space3: m.space_3
    readonly property int space4: m.space_4
    readonly property int space5: m.space_5
    readonly property int space6: m.space_6

    // Layout: list and sidebar row, list icon, bars
    readonly property int rowHeight: m.row_height
    readonly property int sideRowHeight: m.side_row_height
    readonly property int iconSize: m.icon_size
    // Outer margin of the window: navigation capsule, glyphs and headings of the sidebar, breadcrumb,
    // search and selection info align on it
    readonly property int margin: m.margin
    readonly property int toolbarHeight: m.toolbar_height
    readonly property int statusHeight: m.status_height
    readonly property int sidebarWidth: m.sidebar_width
    // Tab row and tab (shared by every fili-* application)
    readonly property int tabBarHeight: m.tab_bar_height
    readonly property int tabHeight: m.tab_height

    // Terminal: "text background selection cursor c0 … c15" for TerminalView.colors
    readonly property string terminalColors: [m.text, m.background, m.surface_active, accentColor.toString()].concat(m.palette).join(" ")
}
