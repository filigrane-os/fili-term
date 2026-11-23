#!/usr/bin/env python3
"""Checks the translation catalogs of Aéroport (fili-term) against the English texts of the code.

    scripts/check-i18n.py          (from the source tree of Aéroport (fili-term))

Texts are written in English in the code: tr("...") / trf("...") in src/*.rs and Aeroport.tr("...") / Aeroport.trf("...") in
qml/*.qml (and FiliHost.tr("...") in the QML of the fili-ui kit). Each catalog (i18n/<language>.json) needs an entry per text with the same %1, %2... placeholders; an entry
that nothing uses any more is reported too. "@decimal" is the decimal separator of the language. Exit code 1 on any problem.
"""
import glob
import json
import os
import re
import subprocess
import sys

root = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..")
texts = set()
for path in glob.glob(f"{root}/src/*.rs"):
    if path.endswith("i18n.rs"):
        continue
    for m in re.finditer(r'\btrf?\(\s*"((?:[^"\\]|\\.)*)"', open(path).read()):
        texts.add(json.loads('"' + m.group(1) + '"'))
for path in glob.glob(f"{root}/qml/*.qml"):
    for m in re.finditer(r'(?:Fili|Aeroport)\.trf?\(\s*"((?:[^"\\]|\\.)*)"', open(path).read()):
        texts.add(json.loads('"' + m.group(1) + '"'))


def kit_qml_dir():
    """The QML of the fili-ui kit this build is pinned to (found through cargo): its components translate their texts with
    FiliHost.tr("...") and the catalogs of the application hold those translations."""
    out = subprocess.run(["cargo", "metadata", "--format-version", "1", "--locked"], cwd=root, capture_output=True, text=True)
    if out.returncode != 0:
        sys.exit("check-i18n: cargo metadata failed, cannot find the fili-ui kit:\n" + out.stderr[-400:])
    manifest = next(p["manifest_path"] for p in json.loads(out.stdout)["packages"] if p["name"] == "fili-ui")
    return os.path.join(os.path.dirname(manifest), "qml/Fili/UI")


# Only the components the application uses, and those they use in turn: a kit component it never shows needs no translation here
kit = kit_qml_dir()
kit_files = {os.path.basename(p)[:-4]: open(p).read() for p in glob.glob(kit + "/*.qml")}
used = set()
queue = [m for path in glob.glob(f"{root}/qml/*.qml") for m in re.findall(r"\b(Fili\w+)\b", open(path).read()) if m in kit_files]
while queue:
    name = queue.pop()
    if name in used:
        continue
    used.add(name)
    queue += [m for m in re.findall(r"\b(Fili\w+)\b", kit_files[name]) if m in kit_files and m not in used]
for name in used:
    for m in re.finditer(r'FiliHost\.tr\(\s*"((?:[^"\\]|\\.)*)"', kit_files[name]):
        texts.add(json.loads('"' + m.group(1) + '"'))


def placeholders(s):
    return sorted(set(re.findall(r"%[1-9]", s)))


problems = 0
catalogs = sorted(glob.glob(f"{root}/i18n/*.json"))
for catalog in catalogs:
    lang = os.path.basename(catalog)[:-5]
    entries = json.load(open(catalog))
    for t in sorted(texts):
        if t not in entries:
            print(f"{lang}: missing: {t}")
            problems += 1
        elif placeholders(t) != placeholders(entries[t]):
            print(f"{lang}: placeholders differ: {t}")
            problems += 1
    for k in sorted(set(entries) - texts - {"@decimal", "@name"}):
        # the catalog keeps texts that the code translates without a literal tr("...") (names of states, of herdr): a note
        print(f"{lang}: note, no literal use of: {k}")
print(f"{len(texts)} texts, {len(catalogs)} catalog(s), {problems} problem(s)")
sys.exit(1 if problems else 0)
