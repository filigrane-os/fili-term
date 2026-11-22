#!/usr/bin/env python3
"""The ctl catalog of the application (shell/ctl/<app_id>.json, docs/api.md of the Filigrane repository) stays in agreement
with the program: valid schema, and its verbs are exactly the ones of the usage line of `<program> ctl` in the source.
Usage: scripts/check-ctl-catalog.py (from anywhere)."""
import glob, json, pathlib, re, sys

root = pathlib.Path(__file__).resolve().parents[1]
files = sorted(glob.glob(str(root / "shell/ctl/*.json")))
errors = []
if not files:
    errors.append("no shell/ctl/<app_id>.json")
for f in files:
    name = pathlib.Path(f).name
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
