#!/usr/bin/env python3
"""The fili-bar menu of Aeroport (shell/appmenu/fili-term.json) stays in agreement with the window (qml/Main.qml): every
`fili-term ctl --pid %pid action NAME` of the menu is an action of the window (`run(name)`), every action of the window is in
the menu (the few that have none are listed below), and the brief of Lia (shell/lia/fili-term.md) exists. The shortcuts are
not checked here: they are configurable (keymap in Rust), so the menu shows the defaults and the keymap owns them.
Usage: scripts/check-shell-menu.py (from anywhere)."""
import json, pathlib, re, sys

root = pathlib.Path(__file__).resolve().parents[1]
menu = json.loads((root / "shell/appmenu/fili-term.json").read_text())
qml = (root / "qml/Main.qml").read_text()
start = qml.index("function run(name)")
body = qml[start:qml.index("function layoutJson()", start)]
actions = set(re.findall(r'name === "([a-z-]+)"', body))
# Families of actions: focus-left, resize-up (a direction), tab-1 to tab-9 (by number)
prefixes = tuple(re.findall(r'name\.startsWith\("([a-z]+-)"\)', body))
# Test hooks for the attention colours (ctl only), the context menu of the pane (right click) and the editing of a command's
# output (button of the command block)
without_menu = {"attention", "attention-clear", "context-menu", "edit-output"}
errors = []
if not (root / "shell/lia/fili-term.md").is_file():
    errors.append("brief of Lia missing: shell/lia/fili-term.md")

def entries(items):
    for e in items:
        if "children" in e:
            yield from entries(e["children"])
        elif not e.get("sep"):
            yield e

everything = list(entries(menu["app"] + menu["window"] + [m for top in menu["menus"] for m in top["children"]]))
used = set()
for e in everything:
    cmd = e.get("exec", "")
    if not cmd.startswith("fili-term ctl"):
        continue  # a command that opens a file (Settings…)
    m = re.fullmatch(r"fili-term ctl --pid %pid action ([a-z0-9-]+)", cmd)
    if not m or not (m.group(1) in actions or m.group(1).startswith(prefixes)):
        errors.append(f"command without an action in Main.qml: {cmd}")
    else:
        used.add(m.group(1))
for a in sorted(actions - used - without_menu):
    errors.append(f"action of Main.qml missing from the menu: {a}")
if errors:
    print("not ok - menu of Aeroport:")
    print("\n".join("#   " + e for e in errors))
    sys.exit(1)
print("ok - menu of Aeroport agrees with Main.qml (%d actions)" % len(actions))
