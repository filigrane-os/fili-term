# Changelog

## Unreleased (V1)

- Display name Aéroport (About card: "Also known as fili-term").
- Window anatomy of Filigrane: header with title, new tab, Window panel (niri actions) and Close; tab row from two
  tabs; footer with the folder and the size of the grid, or the place in the history.
- Menu of the fili-bar through `fili-term ctl [--pid PID] action NAME` (fallback appmenu/fili-term.json in Filigrane).
- Settings and theme in `~/.config/filigrane/fili-term.toml` (font size, padding, window buttons; theme keys and the
  terminal `palette`), live light/dark mode and accent of fili-theme.
- French translation, desktop entry, icon (placeholder), Makefile.

## V0 (spike)

- Session daemon (alacritty_terminal) and a Qt Quick window painting the damaged lines, all in Rust.
