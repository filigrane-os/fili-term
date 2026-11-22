#!/usr/bin/env python3
"""The ctl catalog of the application (shell/ctl/<app_id>.json, docs/api.md of the Filigrane repository) stays in agreement
with the program: valid schema, its verbs are exactly the ones of the usage line of `<program> ctl` in the source, and its
`actions` (the names `action NAME` accepts, with the label and shortcut of the menu when it has one) are exactly the ones of
the window: the ACTIONS table of the Rust source, else the table of qml/Main.qml.
Usage: scripts/check-ctl-catalog.py (from anywhere). `--write` rewrites the `actions` of the catalog from the source."""
import glob, json, pathlib, re, sys

WRITE = "--write" in sys.argv


def source_actions(root):
    """Names of the actions of the window, in the order of the source: the ACTIONS table of src/ctl.rs, else the
    menuActions table of qml/Main.qml (Casier), else its `"name": { on: ...` table (Baladeur, Cassette)."""
    ctl = root / "src/ctl.rs"
    if ctl.is_file():
        m = re.search(r"pub const ACTIONS: &\[&str\] = &\[(.*?)\];", ctl.read_text(), re.S)
        if m:
            return re.findall(r'"([^"]+)"', m.group(1))
    main = root / "qml/Main.qml"
    if main.is_file():
        text = main.read_text()
        m = re.search(r"menuActions: \(\{(.*?)\}\)", text, re.S)
        if m:
            return re.findall(r'"([a-z0-9-]+)":\s*\w+', m.group(1))
        return re.findall(r'^\s+"([a-z0-9-]+)": \{ on:', text, re.M)
    return None


def menu_entries(root, app_id):
    """name -> (label, keys) of the entries of the fallback menu that run `action NAME`."""
    path = root / f"shell/appmenu/{app_id}.json"
    found = {}
    if not path.is_file():
        return found

    def walk(items):
        for e in items:
            if "children" in e:
                walk(e["children"])
            m = re.search(r" action ([a-z0-9-]+)$", e.get("exec", ""))
            if m and m.group(1) not in found:
                found[m.group(1)] = (e.get("label", ""), e.get("keys", ""))

    menu = json.loads(path.read_text())
    walk(menu.get("app", []) + menu.get("window", []))
    for top in menu.get("menus", []):
        walk(top["children"])
    return found

root = pathlib.Path(__file__).resolve().parents[1]
files = sorted(glob.glob(str(root / "shell/ctl/*.json")))
errors = []
if not files:
    errors.append("no shell/ctl/<app_id>.json")
for f in files:
    name = pathlib.Path(f).name
    c = json.load(open(f))
    if WRITE:
        c = json.load(open(f))
        names, menu = source_actions(root), menu_entries(root, c["app_id"])
        if names is None:
            errors.append(f"{name}: no table of actions in the source")
            continue
        c["actions"] = [{"name": n, **({"label": menu[n][0]} if menu.get(n, ("",))[0] else {}), **({"keys": menu[n][1]} if menu.get(n, ("", ""))[1] else {})} for n in names]
        json.dump(c, open(f, "w"), indent=2, ensure_ascii=False)
        open(f, "a").write("\n")
    c = json.load(open(f))
    for key, kind in (("v", int), ("app_id", str), ("name", str), ("program", str), ("ctl", str), ("commands", list)):
        if not isinstance(c.get(key), kind):
            errors.append(f"{name}: {key} is missing or has the wrong type")
    if errors:
        continue
    if c["v"] > 1:
        errors.append(f"{name}: v {c['v']} is newer than the tools understand (1)")
    if pathlib.Path(f).stem != c["app_id"]:
        errors.append(f"{name}: the file is named after the app id ({c['app_id']})")
    if "--pid" in c["ctl"] and "%pid" not in c["ctl"]:
        errors.append(f"{name}: ctl: --pid needs %pid")
    verbs = []
    for cmd in c["commands"]:
        if not cmd.get("verb") or not cmd.get("summary"):
            errors.append(f"{name}: a command needs a verb and a summary: {cmd}")
        else:
            verbs.append(cmd["verb"])
    if len(set(verbs)) != len(verbs):
        errors.append(f"{name}: a verb is listed twice")
    actions = c.get("actions")
    if not isinstance(actions, list) or not all(isinstance(a, dict) and a.get("name") for a in actions):
        errors.append(f"{name}: actions must be a list of {{name, label?, keys?}}")
    else:
        declared_actions = [a["name"] for a in actions]
        if len(set(declared_actions)) != len(declared_actions):
            errors.append(f"{name}: an action is listed twice")
        real = source_actions(root)
        if real is None:
            errors.append(f"{name}: no table of actions in the source")
        else:
            for a in sorted(set(real) - set(declared_actions)):
                errors.append(f"{name}: the window has the action {a}, the catalog does not (scripts/check-ctl-catalog.py --write)")
            for a in sorted(set(declared_actions) - set(real)):
                errors.append(f"{name}: the catalog has the action {a}, the window does not (scripts/check-ctl-catalog.py --write)")
    # Alternatives are separated by " | " outside the brackets ("select [--add | --none]" is one)
    # The usage line of the program: "usage: <program> ctl [--pid PID] verb ... | verb ..." (possibly cut by a backslash)
    usage = None
    for src in sorted(root.glob("src/**/*.rs")):
        m = re.search(r'usage: ' + re.escape(c["program"]) + r' ctl \[--pid PID\] ((?:[^"\\]|\\\n\s*)+)"', src.read_text())
        if m:
            usage = re.sub(r"\\\n\s*", "", m.group(1))
            break
    if usage is None:
        errors.append(f"{name}: no usage line of `{c['program']} ctl` in src/")
        continue
    declared = {alt.strip().split()[0] for alt in re.split(r"\s\|\s(?![^\[]*\])", usage) if alt.strip()}
    for v in sorted(declared - set(verbs)):
        errors.append(f"{name}: the program has the verb {v}, the catalog does not")
    for v in sorted(set(verbs) - declared):
        errors.append(f"{name}: the catalog has the verb {v}, the program does not")
for e in errors:
    print(e)
sys.exit(1 if errors else 0)
