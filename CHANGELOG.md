# Changelog

## Unreleased (V3)

### V3, part A

- `fili-term [--app-id ID] [-T TITLE] [--hold] [--tab] [-e COMMAND…]`: the options scripts need from a terminal.
  `--app-id` opens the window in a process of its own with that Wayland app-id (compositor rules), `--hold` keeps
  the tab open once the command ended, `--tab` opens a tab in the window being used.
- Search in the history: Ctrl+Shift+F, Enter / Shift+Enter (or Ctrl+Shift+G / H, F3) for the next and previous
  match, Escape closes; smart case, every match lit, the current one stronger. Menu actions find, find-next,
  find-previous.
- Ctrl+click opens an address (http, https, ftp, file, www.) with xdg-open; Ctrl+hover underlines it.
- The selection follows the history: it stays on its text while scrolling, dragging past an edge scrolls, and a
  selection reaching into the history is copied by the daemon.
- A notification when a command that ran 10 s or more ends while its terminal is not the one being watched
  (`notify_after` in fili-term.toml, 0 turns it off).
- Files dropped on the terminal (from Casier) type their quoted paths at the prompt; menu action show-in-files.
- Keyboard shortcuts you can choose and edit: `keymap = "default" | "mac" | "tmux" | "none"` (also `fili-term keymap NAME`,
  Settings > Aéroport) and a `[keys]` section of fili-term.toml, action by action (`new-tab = "Ctrl+Shift+T | Meta+T"`,
  chords like "Ctrl+B, C", empty = none); open windows follow the file at once. New actions: tab-1 to tab-9 (Alt+1…9),
  Ctrl+Shift+Left/Right for the previous and next tab, scroll-top and scroll-bottom keys.
- Hover strip of a block: Edit Output (opens it in $VISUAL / $EDITOR in a new tab) and Show in Casier (the block's folder).
- Header and footer follow `fili-theme header|footer` live; search field in the header like Casier, with a clear
  button and previous / next buttons; history scroll bar; dashed line between blocks; `line_spacing` (default 0);
  tabs show the running command with a spinner, and a check or cross for a result not yet seen.
- The footer shows the latest fili-tx transaction for two minutes, with Undo (Redo once undone).

### V3, part B

- Integrated modes, only when the user calls the program. First one: herdr (`fili-term mode herdr`, or Integrate in the
  footer when `herdr` runs in a tab; `[modes] herdr = "ask" | "auto" | "off"`, Shell > Integrate Program). herdr's tabs
  become tabs of the window, each attached to a pane (`herdr terminal attach`), without herdr's own interface; its
  workspaces, agents (with their status) and the panes of the tab shown fill the sidebar; new and closed tabs, workspaces
  and agents go through its socket API; a notification when an agent waits for you or finished while you look
  elsewhere. Stop Integration closes those tabs, herdr goes on.
- The daemon tells the window the process in the foreground of each terminal (from /proc, once a second).
- herdr's right-click menus: on a tab (rename, new, close), a workspace (rename, new worktree, open worktree, delete a
  worktree checkout, close the group, close), a pane or an agent (rename, clear the name, swap with the focused pane,
  split right or down, close); names typed in place, closes confirmed in the footer, herdr's errors shown there. Linked
  worktrees under their repository; agents grouped by workspace or by priority (the choice on the Agents heading).
- herdr's key bindings in herdr's tabs (prefix Ctrl+B and the `[keys]` of herdr's config): tabs, workspaces,
  worktrees, agents, panes (the directions go through the panes of the tab in turn), sidebar, detach (stops the
  integration), settings, reload. A question in the footer answers to Enter and Escape.
- View > Show Sidebar (Ctrl+Shift+B).
- Fix: a session hung up by its window (closed tab) left its terminal state in the daemon.

- Panes: every tab is a tree of splits, each pane a terminal with its own session. Split Right (Ctrl+Shift+D),
  Split Down (Ctrl+Shift+E), Close Pane (Ctrl+Shift+W; Close Tab moves to Ctrl+Alt+Shift+W), Zoom Pane
  (Ctrl+Shift+Return), move between panes with Ctrl+Alt+arrows, move a divider with Ctrl+Alt+Shift+arrows or by
  dragging it (double click: even again). The panes without the keyboard are dimmed. A new pane starts in the
  folder of the active one. mac preset: Cmd+D, Cmd+Shift+D, Cmd+W, Cmd+Option+arrows; tmux preset: prefix %, ",
  x, z, arrows.
- Right click in a terminal opens a menu (Copy, Paste, Split Right / Down, Swap Pane, Zoom Pane, Close Pane; the
  entries of herdr's pane menu in a herdr tab). When the program takes the mouse, Shift+right click opens it.
  The Menu key or Shift+F10 opens it from the keyboard (action context-menu).
  In a herdr tab, right click works as in herdr's client: the pane menu always, even when the program takes the
  mouse; herdr's `[ui] right_click_passthrough_modifier` (e.g. "ctrl+alt") sends it to the program instead.
- Close Workspace (herdr) closes at once, without a confirmation.
- Each pane has its own scroll bar, block hover strip and drop target; search follows the active pane.

- herdr tabs show all their panes, laid out as herdr has them (splits and ratios from its layout), each pane attached
  to its own terminal: split, close, swap, zoom, move between panes and resize act on herdr's real panes, a dragged
  divider sets herdr's ratio, the pane clicked becomes herdr's focused pane. herdr's keys prefix+h/j/k/l, Tab, v, -,
  x, z and resize_pane_* follow. The "Panes" section of the sidebar is gone.

## v2.0.0 (2026-11-12)

- Command blocks: shell integration loaded by the daemon (zsh, bash), OSC 133 marks kept in the grid; hairline,
  failure bar, exit code and duration per command; hover strip with Copy output, Copy command, Run again, Ask Rob.
- Ctrl+Shift+Up/Down jumps between commands, Ctrl+Shift+O copies the last output; menu actions previous-command,
  next-command, copy-last-output, copy-last-command, ask-rob.
- `fili-term blocks`, `fili-term output [BLOCK]`, `fili-term send TEXT` for the assistant; context file for Rob.
- Current folder of the shell (OSC 7) in the title and the footer.
- Colour queries (OSC 10/11/4) answered with the theme's palette.

## v1.0.0 (2026-11-12)

- Display name Aéroport (About card: "Also known as fili-term").
- Window anatomy of Filigrane: header with title, new tab, Window panel (niri actions) and Close; tab row from two
  tabs; footer with the folder and the size of the grid, or the place in the history.
- Menu of the fili-bar through `fili-term ctl [--pid PID] action NAME` (fallback appmenu/fili-term.json in Filigrane).
- Settings and theme in `~/.config/filigrane/fili-term.toml` (font size, padding, window buttons; theme keys and the
  terminal `palette`), live light/dark mode and accent of fili-theme.
- Copy on select: a selection goes to the primary selection and to the clipboard.
- French translation, desktop entry, icon (a prompt and an airport), Makefile, package.env (xbps package of Filigrane).

## V0 (spike)

- Session daemon (alacritty_terminal) and a Qt Quick window painting the damaged lines, all in Rust.
