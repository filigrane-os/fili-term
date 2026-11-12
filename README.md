# Aéroport (fili-term)

The terminal of [Filigrane](https://github.com/filigrane-os/filigrane). Aéroport is its display name; the program, the commands and this repository
keep the name `fili-term` (an `aeroport` link is installed too). Two processes:

- `fili-term-daemon` (`daemon/`, Rust, no Qt): owns the ptys and the terminal state through
  [`alacritty_terminal`](https://crates.io/crates/alacritty_terminal). Started on demand, it outlives the windows: a
  session survives a crash of the window and can be reattached (`fili-term list`, `fili-term --attach ID`).
- `fili-term` (Rust + Qt Quick through cxx-qt, software rendering): one process for every window. The grid is a
  `QQuickPaintedItem` subclass written in Rust (`src/gui/terminal.rs`) that paints only the damaged lines it receives
  over the daemon's socket (`src/proto.rs`). No hand-written C++: the Qt classes cxx-qt-lib lacks (key, mouse and
  input-method events, font metrics, clipboard) are bound straight to Qt's own members in the bridge. A frame is sent only after the window acknowledged the previous one, so a flood of output
  costs the window one frame per paint, never a queue.

## Build

```sh
export QMAKE=/usr/lib/qt6/bin/qmake6
cargo build --release --workspace
./target/release/fili-term            # FILI_TERM_TRACE=1 prints frame sizes, paint times and key→paint latency
```

`make` builds both binaries, `make test` runs the unit tests, `make install` installs them with the desktop entry and
the icon.

## Window

The window follows the anatomy of every Filigrane application (header 44 with the Window and Close buttons, a tab row
from two tabs, footer 28 with the folder and the size of the grid or the place in the history). Its menu lives in the
fili-bar of Filigrane: `fili-term ctl [--pid PID] action NAME` runs a menu command in the active window
(`fili-term ctl actions` lists them). The About card is the `about` action.

Keys: Ctrl+Shift+T new tab, Ctrl+Shift+W close tab, Ctrl+PgUp/PgDn (or Ctrl+Tab) switch tab, Ctrl+Shift+N new
window, Ctrl+Shift+Q close window, Ctrl+Shift+C/V (Ctrl+Insert, Shift+Insert, Copy/Paste keys) copy/paste,
Shift+PgUp/PgDn history, Ctrl+plus/minus/0 font size. Shift+mouse selects even when a program grabs the mouse.

## Command blocks

The daemon loads its shell integration by itself (zsh through `ZDOTDIR`, bash through `--rcfile`; nothing to add to
`~/.zshrc`; `FILI_TERM_SHELL_INTEGRATION=0` turns it off). The shell marks its prompt, command and exit code
(OSC 133) and its folder (OSC 7); the daemon keeps the marks in the grid, so they survive reflow and history. Each
command is a block: a hairline above it, a red bar in the margin when it failed (accent while it runs), the exit code
and duration at the right of its command line. Hovering a block shows Copy output, Copy command, Run again (only at an
idle prompt) and Ask Rob. Ctrl+Shift+Up/Down jumps between commands, Ctrl+Shift+O copies the last output.

From the command line (session `$FILI_TERM_SESSION` by default, or `--session ID`):
`fili-term blocks` (JSON list), `fili-term output [BLOCK]` (output of a block, the last command by default),
`fili-term send TEXT` (types text at the prompt without running it). Ask Rob writes the window's context file
(`$XDG_RUNTIME_DIR/fili/context/PID.json`: session, folder, last commands) and runs `fili-chat ask`; the assistant
reads the output itself with `fili-term output`.

Colour queries (OSC 10/11/4) are answered with the theme's palette.

## Settings and theme

`~/.config/filigrane/fili-term.toml`, every key optional: `font_size` (12), `padding` (10), `window_buttons` (true) at
the top, then the theme in the sections of `theme.toml` (`[theme]`, `[theme.dark]`, `[theme.light]`,
`[theme.accents]`), above the shared `~/.config/filigrane/theme.toml` and the defaults of `theme.toml` here. The
terminal font is `mono`, its 16 colours `palette` (one list per mode); text, background and selection follow `text`,
`background` and `surface_active`, the cursor the accent colour. Light and dark mode follow `fili-theme` live.

## V0 measurements (2026-11-12, i5-1038NG7, niri, scale 1.75, window 707×858, JetBrains Mono 12 px)

| | foot | fili-term |
|---|---|---|
| `cat` 50 MB of base64 (idle machine) | 0.78–0.79 s | 0.78–1.04 s |
| key → painted (zsh echo, through the daemon) | n/a | median 1.7 ms, max 4.9 ms |
| memory, 1 terminal (PSS) | ≈ 10 MB | 41 MB window + 7 MB daemon |
| memory, 4 terminals (PSS) | ≈ 40 MB (4 windows) | 50 MB window (4 tabs) + 11 MB daemon |

Full-screen repaint: ≈ 4.5 ms (software). Hidden tabs drop their image.

## Not done yet

Box-drawing characters drawn by hand, selection in the history, search, URL detection, session sidebar,
`-e`/`--app-id`/`--hold` options.
