# Aéroport (`fili-term`)

Aéroport is the terminal. Sessions live in a daemon and survive their window.
- `fili-term list` (sessions), `fili-term blocks [--session ID]` (commands run, JSON), `fili-term output [--session ID] [--all] [BLOCK]` (output of a command, by default the last one), `fili-term ctl [--pid PID] action NAME` / `actions`, `fili-term keymap`, `fili-term mode herdr`.
- To understand a failed command: `fili-term blocks` then `fili-term output`, not a screenshot.
- Fast path: `fili-term output` alone gives the last command's output; use `blocks` only to pick an older one.
- `fili-term send [--session ID] TEXT` types TEXT at the prompt **without Enter**: the user reads it and presses Enter. Never add a newline, never run a command in the user's session on your own; run what you need in your own shell.
- Never read the whole output of a session (`--all`) unless asked: it may hold secrets (tokens, passwords typed or printed). Do not copy such output into a message.
- Settings: `~/.config/filigrane/fili-term.toml`; keymaps `default|mac|tmux|none`.
