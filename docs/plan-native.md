# Plan: herdr's good ideas, native in Aéroport (V3, part C)

Written 2026-11-15 after V3 part B (herdr mode, native panes, remote herdr: commits up to 84d4002). Each step below is
self-contained: build, verify headless, commit, then the next. Nothing is tagged before Thomas tested the whole live.

## Ground rules (read first)

- Code, comments, commits, docs and UI strings in English; French only in `i18n/fr.json` (every new UI string gets its
  translation there) and in answers to Thomas.
- Commits: no AI attribution, no `Co-Authored-By`, no `Claude-Session` line. Scan the staged diff for secrets before each
  commit. Never push `main`.
- Build: `QMAKE=/usr/lib/qt6/bin/qmake6 cargo build --release --workspace`; tests: `QMAKE=… make test` (slow, run in the
  background). QML is compiled into the binary: rebuild after any QML change. A cxx-qt bridge (`#[cxx_qt::bridge]`) only
  holds declarations; bodies go in `impl qobject::X` below it (else "Unsupported verbatim input in ForeignItem").
- `~/.local/bin/fili-term` and `fili-term-daemon` are symlinks to `target/release`: a build changes what Thomas runs at
  his next window. Daemon protocol changes need his daemon restarted, which kills his sessions: ask him, never do it.
- Testing never touches Thomas's windows: fresh `XDG_RUNTIME_DIR` per launch (`mktemp -d /tmp/claude-1000/ftXXXX`, short
  path, sockets limited to 108 bytes), `WAYLAND_DISPLAY=/run/user/1000/wayland-1 QT_QUICK_BACKEND=software`, drive it with
  `fili-term ctl --pid PID action NAME`, look with `niri msg action screenshot-window --id W -d true --path FILE`.
  The helper `scratchpad/ft.sh` (see the session notes in memory `fili-term-project.md`) does both. Kill only the exact
  pids whose `/proc/PID/environ` has that `XDG_RUNTIME_DIR`. Never `pkill -x fili-term`, never `pkill -f` with a pattern
  that matches your own shell, never `wtype`/simulated keys on Thomas's screen. A command for a test tab that must put a
  program in the foreground of the terminal needs job control: `-e sh -c 'set -m; PROGRAM; exec sh'`.
- herdr tests: a private server `XDG_CONFIG_HOME=/tmp/claude-1000/hr setsid herdr server`, its socket
  `/tmp/claude-1000/hr/herdr/herdr.sock`, called with `/tmp/claude-1000/hcall.py` (`HS=socket python3 hcall.py METHOD
  'JSON'`). Thomas's own `herdr server` must stay untouched.
- Every new action: name in `src/ctl.rs` `ACTIONS`, default keys in `src/gui/keys.rs` (default, mac and tmux presets; the
  test checks every key action is a ctl action), handled in `Main.qml` `onAction`, listed in the bar menu fallback
  `~/Projects/filigrane/dotfiles/config/filigrane/appmenu/fili-term.json` (filigrane repo, own commit) and documented in
  the `[keys]` comment of `~/Projects/filigrane/dotfiles/config/filigrane/fili-term.toml`.
- QML follows the filigrane-ui skill (`~/Projects/filigrane/agents/skills/filigrane-ui/`, tokens in `FiliTheme`: spacing
  4/8/12, `sizeSmall`, `textSecondary`, accent only for the one main action). Menus = `ContextMenu.qml`; footer notes =
  the `modeNote` / `txNotice` rows of `Main.qml`; text prompts = `TextPrompt.qml`.
- Each step ends with: CHANGELOG.md ("Unreleased (V3)", new "V3, part C" heading), README.md when behaviour changes, a
  line in memory `fili-term-project.md`, a short French report to Thomas (done / verified / not tested).

## Where things are

- `qml/Main.qml` window: `tabs` (Tab.qml items), `tab`/`view` current, `newTab`, `addTab`, `select`, `closePane`,
  `paneMenu` (right click), `onAction` (ctl + keys), `syncMode` (herdr), footer rows, `Sidebar` (fed by `sidebarModel`,
  today only from herdr's state: sections with items {id, label, detail, status, active}).
- `qml/Tab.qml` split tree: leaf {view} | split {dir, ratio, path, a, b}; `split`, `insert`, `take`, `remove`, `renew`,
  `neighbour`, `swap`, `toggleZoom`, `resize`, `layout()`.
- `termComponent` in Main.qml: the TerminalView of a pane (props `paneTab`, `modeTerminal`, `modePane`, `declined`);
  signals `exited`, `bell`, `commandFinished(command, exit, duration)`; property `foreground` (name of the foreground
  process, polled by the daemon once a second), `foregroundPid()`, `cwd`, `title`, `contextJson()` (blocks).
- `src/gui/terminal.rs` TerminalView (Rust side, talks to the daemon, `src/proto.rs`); `daemon/` sessions, pty loop,
  OSC 133 blocks (`daemon/src/blocks.rs`), OSC 7 cwd.
- `src/gui/app.rs` `Aeroport` singleton: `settingsJson`, `uiGet/uiSet` (state in `~/.local/state/filigrane/term/ui`),
  `notify`, `tr`, `setModePolicy`. `src/gui/settings.rs` parses `fili-term.toml`.
- `src/modes/` herdr adapter (`herdr.rs`), SSH tunnel (`remote.rs`); `src/gui/mode.rs` ModeClient for QML.

## Step 1: small gestures (pane names, resize mode, scrollback in the editor)

1. **Rename Pane** natively: `property string paneName` on the view; pane menu entry "Rename Pane…" / "Clear Pane Name"
   for native panes too (today herdr only), prompt via `TextPrompt`; the name chip already drawn for herdr panes (look
   for the chip in Main.qml) shows `paneName` for native panes when the tab has more than one pane. Action
   `rename-pane` (no default key in default preset; tmux preset `Ctrl+B, .`? keep tmux's `select-pane -T` habit: none).
   The tab label stays the title of its active pane unless the tab was renamed.
2. **Resize mode** natively: the herdr mode already has a "resizing" state with Shortcuts (arrows, HJKL, Esc) and a
   footer note; make it work in native tabs too (call `tab.resize(dir, step)` instead of herdr's op). Action
   `resize-mode` (default `Ctrl+Alt+R`, tmux `Ctrl+B, r`).
3. **Scrollback in the editor**: action `edit-scrollback` (default `Ctrl+Shift+Alt+E`? pick a free one, tmux
   `Ctrl+B, e`): dump the active pane's history + screen to a temp file (`$XDG_RUNTIME_DIR/fili-term/scrollback-N.txt`,
   0600; the daemon already serves `fili-term output --all`; add a TerminalView invokable `historyText()` if simpler),
   then open `${VISUAL:-${EDITOR:-nvim}} FILE` in a new pane split below, zoomed, closed when the editor exits (command
   pane, not hold). herdr tabs keep herdr's own edit_scrollback.
Verify: ctl actions on a test window, screenshots. Commit "Native panes: rename, resize mode, scrollback in the editor".

## Step 2: agent status on native tabs and panes

Goal: herdr's best feature without herdr. Each pane knows whether an AI agent runs in it and its state: `working`,
`blocked` (waits for you), `done` (finished, not seen yet), `idle`.

1. **Detection** (`src/gui/agents.rs`, pure functions + unit tests): an agent runs when the foreground process name is in
   a known list (claude, codex, opencode, gemini, aider, goose, amp, cursor-agent, copilot; also node/bun/python with
   argv containing one of these: read `/proc/PID/cmdline` from `foregroundPid()`). Configurable list
   `[agents] programs = [...]` in fili-term.toml (default list in code).
2. **State** without hooks: `working` while the terminal received output in the last ~2 s (add a TerminalView property
   `lastOutputMs` or a signal `activity` from the Rust side, throttled to once per second); `blocked` on BEL or on
   OSC 9 / OSC 777 notifications (Claude Code and Codex ring the bell / send OSC 9 when they wait: check by running them
   in a test pane and reading the raw bytes, `script -q` log) or when idle >2 s right after output ends with a question
   prompt; `done` when it went from working to idle while the pane was not watched; `idle` else. Keep the heuristic in
   Rust, tested with recorded sequences.
3. **State with hooks** (precise, optional): `fili-term report --session ID STATE [--agent NAME]` (new CLI verb; the
   session id is `$FILI_TERM_SESSION` in every Aéroport shell; goes to the daemon, which forwards it to the window as a
   new proto message, or simpler: to the window's ui socket like `ctl`). Document hooks for Claude Code
   (`~/.claude/settings.json` hooks `Notification` → blocked, `Stop` → done, `UserPromptSubmit` → working) in README;
   look at what `herdr integration install claude` writes (run it with a scratch `HOME`) and mirror it. A reported state
   wins over the heuristic for 10 minutes.
4. **Display**: a status dot on the tab (same dots/colours as herdr's tabs in mode: reuse the code of `modeStatus`), on
   the pane chip, and in the sidebar (step 3). Notification "X is waiting for you" / "X finished" when the pane is not
   watched (reuse herdr's `onNotice` text and `Aeroport.notify`).
5. **Go to the next agent that waits** — action `next-agent` (default `Ctrl+Alt+A`, tmux `Ctrl+B, a`): selects the tab
   and focuses the pane of the first blocked, else done, agent; also used when a notification is clicked if notify
   supports actions (check `Aeroport.notify`).
Verify: a fake agent script named `claude` (sleep, print, ring bell, wait for input) in test panes; screenshots of the
dots; unit tests of the heuristic. Commit "Agent status on every pane: working, waiting, done; next agent".

## Step 3: native workspaces in the sidebar

1. A workspace = a group of tabs, by default one per project: the git root of the tab's first pane cwd, else its cwd.
   Window property `workspaces` [{id, label, tabs, collapsed}]; a tab carries `workspace` (id). New tab joins the current
   workspace; a tab whose cwd leaves its project stays where it is (no jumping), "Move Tab to Workspace" in the tab menu.
2. Sidebar for native windows (today shown only in herdr mode): sections "Workspaces" (items: label, detail = tab count,
   status = worst agent status of its panes, active) and "Agents" (every pane with an agent, grouped by workspace or by
   priority, same as herdr's: reuse `sortedAgents`). Feed `sidebarModel` from native data when no mode runs, from herdr
   when it runs. Selecting a workspace shows only its tabs in the tab row (others hidden, not closed); "+" makes a new
   workspace (prompt for a folder via a path prompt or the current cwd), right click: rename, close (closes its tabs,
   confirm only when a command runs).
3. Off by default? No: shown when the window has more than one workspace or an agent runs; View > Show Sidebar
   (Ctrl+Shift+B) still forces it on/off (`sidebarWanted`).
4. Actions `next-workspace` / `previous-workspace` (default `Ctrl+Alt+PgDown/PgUp`, tmux `Ctrl+B, )` / `(`).
Verify: two projects in test tabs, screenshots of sidebar and tab row filtering. Commit.

## Step 4: layout restored when a window reopens

1. Save on every change (debounced 1 s) and on close: `~/.local/state/filigrane/term/windows.json` = list of windows,
   each {workspaces, tabs: [{workspace, name, tree}]}, tree leaves {session (daemon id), cwd, name, command if not a
   shell}. Herdr tabs are not saved (herdr restores itself; save only "herdr mode was on" + its target, and restart the
   mode on restore).
2. `fili-term` with no argument and no window open: if windows.json lists windows whose sessions still exist in the
   daemon (`fili-term list` / the daemon's list request), reopen them attached (`sessionId`), same splits and ratios.
   After a daemon restart (sessions gone): reopen the layout with fresh shells in the saved cwds. Setting
   `restore = "always" | "never"` (default "always") at the top of fili-term.toml; `--new` flag to skip once.
3. A window closed by the user on purpose (Close Window) is removed from the file; the last window closed by quitting
   (logout, daemon kept) stays.
Verify: open a test window with splits, close it, relaunch in the same XDG_RUNTIME_DIR... careful: same runtime dir means
the old ui socket; delete `ui.sock` between runs or use the daemon kept alive. Commit.

## Step 5: custom commands and popups

1. `[[commands]]` in fili-term.toml, like herdr's `[[keys.command]]`: `key`, `name` (for the menu), `type` = "popup" |
   "pane" | "tab" | "shell", `command`, `width`/`height` (popup, fraction or percent). Parse in settings.rs (tests).
2. Reuse the herdr popup overlay (`popup` Loader + `popupFrame.popupSize` in Main.qml) for native tabs: a terminal
   floating over the tab, closed when its command exits or with Esc held? (herdr closes on exit; keep that). "pane":
   split below, closed on exit. "tab": new tab. "shell": detached in the background (reuse `runShell` pattern, setsid).
   Environment: `FILI_TERM_SESSION`, `FILI_TERM_CWD` (active pane cwd).
3. Menu: a "Commands" submenu in the pane menu and in the bar menu (generated: the bar fallback JSON is static, so add
   one entry "Commands…" opening a picker via ctl action `commands`).
Default example in the toml comment: lazygit popup on `Ctrl+Alt+G`. Verify with a popup running `sh -c 'echo hi; sleep 3'`.
Commit.

## Step 6: tab in a git worktree

Pane/tab menu "New Tab in Worktree…" (when the cwd is in a git repo): prompt for a branch name; `git worktree add
../<repo>-<branch> -b <branch>` (or existing branch: `git worktree add PATH BRANCH`), new tab there, in the same
workspace. Sidebar workspace menu lists the repo's worktrees (`git worktree list --porcelain`) to open one in a tab, and
"Remove Worktree…" (confirm in the footer; refuse when dirty, show git's error). Run git with `Command` from Rust
(invokable on Aeroport returning JSON / error), never through a shell string. Verify in a scratch repo under
/tmp/claude-1000. Commit.

## Step 7: type in every pane of a tab

Action `broadcast` (default `Ctrl+Alt+I`, tmux `Ctrl+B, Ctrl+I`?): toggles for the current tab; while on, keys typed in
the active pane are also sent to every other pane of the tab (TerminalView gets an invokable `sendInput(bytes)`;
the key handler of the active view forwards what it writes to the pty to its tab when `tab.broadcast`). Footer note
"Typing in every pane · Stop" and a coloured border on the panes. Not for herdr tabs (herdr has no such API besides
pane.send_text: allowed, but later). Verify with two panes and `ctl` + `fili-term send`. Commit.

## Step 8: remote herdr from anywhere

1. `fili-term mode herdr --remote TARGET [--session NAME]`: pass the target through `open … mode=herdr` flags (main.rs
   `mode()`, instance.rs flags, `startMode` with a target instead of a pid: new ModeClient invokable
   `startTarget(kind, ssh, session)` sharing `start`'s code).
2. Sidebar section "Machines" when in herdr mode or when `herdr machine list --json` (check the flag) returns some:
   click = stop the current mode, start it on that machine; this computer is listed first ("This Computer").
Verify with the sshd test rig (memory `fili-term-project.md`, 2026-11-15 line): user sshd on 127.0.0.1:2222, ssh wrapper
first in PATH. Commit.

## At the end

Thomas tests everything live (he restarts the daemon himself if the protocol changed), then one tag v3.0.0 for
parts A, B and C, pin `apps/fili-term/upstream` in filigrane, package; herdr installed declaratively (users/thomas layer).
