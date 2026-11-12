# Changelog

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
