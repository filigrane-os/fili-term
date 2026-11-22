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
Shift+PgUp/PgDn history, Ctrl+plus/minus/0 font size. Shift+mouse selects even when a program grabs the mouse. Ctrl+Shift+F searches the history (Enter / Shift+Enter next and
previous match, Escape closes), Ctrl+click opens an address.

Panes: Ctrl+Shift+D / E split right / down, Ctrl+Alt+arrows move between panes, Ctrl+Alt+Shift+arrows move a divider,
Ctrl+Alt+R resize mode (arrows or H J K L, Esc when done), Ctrl+Shift+Return zoom. Rename Pane… (right click) puts a
name at the pane's top right. Ctrl+Alt+Shift+E opens the pane's whole history in $VISUAL / $EDITOR (else nvim), in a
zoomed pane that closes with the editor.

`fili-term [--app-id ID] [-T TITLE] [--hold] [--tab] [-e COMMAND…]` is the command line for scripts: `--app-id` gives the
window its own Wayland app-id (in a process of its own), `--hold` keeps the tab after the command ended, `--tab` adds
a tab to the window in use. A notification tells when a command of 10 s or more ends in a terminal nobody watches
(`notify_after`), and the footer offers Undo for the latest `fili-tx` transaction.

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
(`$XDG_RUNTIME_DIR/fili/context/PID.json`: session, folder, last commands) and runs `fili-ai ask`; the assistant
reads the output itself with `fili-term output`.

Colour queries (OSC 10/11/4) are answered with the theme's palette.

## Agents

A pane knows when an AI agent runs in it (the process in its foreground: claude, codex, opencode, gemini, aider, goose,
amp, cursor-agent, copilot, crush, qwen, kimi, droid, pi, also under node, bun or python; `[agents] programs = [...]` in
fili-term.toml replaces the list) and what it does: working, waiting for you, done (finished while you looked
elsewhere), idle. Without any setup, as herdr does, the state comes from what the agent shows: the title it sets (Claude
Code's spinner), the bottom of its screen ("esc to interrupt", "Do you want to proceed?", "Allow command?", "[y/n]"…), a
bell, output that keeps coming. The tab shows it (spinner, bell, check), the pane's name chip too when the tab has
several panes, and a notification comes when an agent waits or finished in a pane you do not see (a click on it shows
the pane). Ctrl+Alt+A (action next-agent, tmux prefix+a) goes to the next agent waiting for you, else to one that
finished.

For exact states, an agent's hooks can tell them: `fili-term report working|blocked|done|idle [--agent NAME]` (the
session is `$FILI_TERM_SESSION`; outside Aéroport it does nothing, so hooks set for every terminal are harmless). A
reported state wins for ten minutes, until you answer (a key in the pane drops a "blocked" or "done"). For Claude Code,
in `~/.claude/settings.json`:

```json
{
  "hooks": {
    "UserPromptSubmit": [{ "hooks": [{ "type": "command", "command": "fili-term report working" }] }],
    "PreToolUse": [{ "matcher": "", "hooks": [{ "type": "command", "command": "fili-term report working" }] }],
    "Notification": [{ "hooks": [{ "type": "command", "command": "fili-term report blocked" }] }],
    "Stop": [{ "hooks": [{ "type": "command", "command": "fili-term report done" }] }]
  }
}
```

## Custom commands

Your own commands in fili-term.toml, run for the active pane (its session in `$FILI_TERM_SESSION`, its folder in
`$FILI_TERM_CWD`):

```toml
[[commands]]
key = "Ctrl+Alt+G"        # alternatives: "Ctrl+Alt+G | Ctrl+B, G"
name = "Lazygit"          # in the Commands menu (default: the program's name)
type = "popup"            # popup (over the pane, closes when it ends) | pane (split below) | tab | shell (background)
command = "lazygit"       # run by sh
width = "90%"             # popup only: a percentage or a count of cells
height = "90%"
```

The pane menu has a Commands submenu, Ctrl+Alt+P (action `commands`) opens the same list, and
`fili-term ctl command Lazygit` runs one by name from a script or a launcher.

## Command journal

Ctrl+Alt+J (View > Show Command Journal, or `journal = true` in fili-term.toml) puts a margin at the left of each pane,
about a fifth of its width. Next to each command block on screen it shows the command typed, a check or a cross with the
exit code, and how long it took (Running while it runs); a block that began above the view keeps its command at the
top. What you type and what comes out stay separate without squeezing the output: the grid only loses the margin's
width. Each entry starts with the time the command was launched (the date for an older day; commands run before the
daemon was last restarted have none). A program on the alternate screen (nvim, htop) takes the whole pane, as does a
pane that is too narrow.

The header has buttons, left of the search field, for the sidebar, the journal, splitting, typing in every pane, the
custom commands, the next agent, workspaces, worktrees, machines and the history in the editor, then New Tab. A narrow
window shows fewer of them and puts the rest in the More (…) menu; resting the pointer on one shows its name and key.
Each is also a key and an entry of the bar menu, so the header can stay hidden.

## herdr on another machine

`fili-term mode herdr --remote me@box [--session NAME]` shows the herdr of another machine in the window, as a local one
(its tabs are tabs of the window, its workspaces and agents fill the sidebar) through an SSH link kept up by Aéroport,
which needs your key in ssh-agent (no question is asked). When there are machines to choose from, the sidebar has a
Machines section: this computer, the machines of `herdr machine list` and the ones you list in fili-term.toml:

```toml
[[machines]]
label = "Build box"
ssh = "me@build.lan"
session = "work"        # optional: the remote herdr session
```

A click moves the mode there; Shell > Machines… (action `machines`, no key by default) and
`fili-term ctl machine "Build box"` (or `local`) do the same.

## Typing in every pane

Ctrl+Alt+I (Type in Every Pane in the pane menu, tmux prefix+S) sends what you type or paste in a pane of the tab to
all its other panes too, for the same command on several machines (several SSH sessions in several panes). The panes get
a coloured border and the footer says so with Stop; it is off by default and per tab, and a pane that gets a paste
wraps it as its own program asked. Mouse, scrolling and the shell's own answers are not repeated.

## Worktrees

New Tab in Worktree… (right click on a pane or a tab, or `fili-term ctl worktree BRANCH`) adds a git worktree for a
branch beside the repository (`<repo>-<branch>`; the branch when it exists, else a new one) and opens a tab there, in
the same workspace. The workspace menu (right click on a workspace in the sidebar) lists the project's worktrees to
open one in a tab, and Remove Worktree… removes a linked one after a confirmation; git never forces it, so one with
changes stays and git's message says why.

## Workspaces

Tabs are grouped by project: the workspace of a tab is the git root of its first pane's folder, else the folder. The tab
row shows the tabs of the workspace shown, a new tab joins it, and a tab whose folder leaves its project stays (right
click on a tab: Move to Workspace). The sidebar appears when the window has two workspaces or an agent runs (View >
Show Sidebar, Ctrl+Shift+B, hides it): Workspaces with their tab count and the worst state of their agents, Agents
grouped by workspace or by priority. Ctrl+Alt+PgDown / PgUp change workspace, **+** (or Tabs > New Workspace…) asks for
a folder, right click renames or closes a workspace.

The layout is kept in `~/.local/state/filigrane/term/windows.json` and comes back at the next bare `fili-term` when no
window is open (after a logout, a crash, a restart of the window process): the same workspaces, tabs, splits and pane
names, each pane on its old session when the daemon still has it, else a new shell in its old folder. Closing a window
on purpose forgets it. `restore = "never"` in fili-term.toml turns this off, `fili-term --new` skips it once.

## Integrated modes

When the program runs in a tab: `herdr` typed in a terminal is integrated at once (the daemon reads the foreground
process of each terminal from `/proc` once a second; `[modes] herdr = "auto" | "ask" | "off"`, "ask" offers it in the
footer), or `fili-term mode herdr`. The footer then offers Use herdr's Interface: the mode stops and herdr's own client
opens in a tab. Nothing runs otherwise. The adapter (`src/modes/`) is a thread of the window process while the mode is on; it
sends the window a generic state (sections, items with a status, tabs and their panes) as JSON and takes its actions.

- herdr (herdr.dev): each tab of herdr's focused workspace is a tab of the window, attached to the terminal of one pane
  (`herdr terminal attach TERMINAL`): no sidebar, tab row or border of herdr. Its workspaces, its agents (status:
  working, waiting for you, done, idle) and the panes of the tab shown (a tab split in several panes shows one, chosen
  there) fill the sidebar. New tab, close tab, a click on a workspace or an agent go through herdr's socket API
  (newline-delimited JSON, one connection per request, one kept for the events). Integrate on a herdr started by hand
  ends that client (its server and panes go on) and opens its tabs beside. Its server is started when it does not run.
  A notification when an agent waits or finished while herdr's tabs are not watched. Use herdr's Interface closes the
  tabs and opens herdr's client in one; herdr goes on.
- A remote herdr (`herdr --remote TARGET`): one `ssh -N` forwards its API socket and its clients' socket to a private
  directory of this machine (`$XDG_RUNTIME_DIR/fili-term/`), then everything goes as for a local one; the link comes
  back when SSH drops. SSH runs without a prompt: the key must be in ssh-agent.
- Next: nvim (msgpack-RPC, its tabs and a file tree), tmux (windows and sessions).

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

## Weight per release

Measured in use on the same machine (i5-1038NG7, niri, scale 1.75): PSS from `/proc/PID/smaps_rollup`, CPU over 10 s of
an idle window (a shell at its prompt, command journal on). The window is the Qt process (all windows), the daemon holds
the sessions.

| version | window (PSS) | daemon (PSS) | CPU idle (window / daemon) | notes |
|---|---|---|---|---|
| v2.0.0 (V0 figures above) | 41 MB | 7 MB | not measured | 1 terminal |
| v3.0.0 | 43 MB | 16 MB | 0.1 % / 0.1 % | 1 session open for 20 minutes, the daemon's blocks and history included |
| v3.1.0 | not measured | not measured | not measured | Debug panel closed by default; the log adds a 2000-line buffer and a file |

The window is within 2 MB of v2.0.0 with panes, workspaces, agents, the journal and the header added. The daemon grew
by 9 MB (the blocks and their times are kept for each session, and this one had lived for 20 minutes): to watch.

## Not done yet

Box-drawing characters drawn by hand, a sidebar of the daemon's sessions, the nvim and tmux modes.
