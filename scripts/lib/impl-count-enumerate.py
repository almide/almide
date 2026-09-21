#!/usr/bin/env python3
"""Enumerate how many implementations each stdlib HOLE has.

A hole is a stdlib declaration whose body is `_` (`fn name[...](...) -> T = _`):
the language declares the operation and hands its body to a backend. Tonight's
release-blocker stream (#2395 / #2397 / #2398) was three instances of one
shape — the same operation implemented more than once, a rule landing in one
copy and not reaching the others — so this enumerator counts the copies.

Three columns per hole, each re-derived from source on every run:

  native    1 if the declaration carries `@intrinsic("...")` within the three
            lines above it (the native target's Rust runtime body), else 0.
  emitter   the number of `fn lower_<module>_<name>(` and
            `fn lower_<module>_<name>_<suffix>(` definitions in
            crates/almide-wasm/src — the structural wasm leg's hand-written
            lowerings for this operation. A dispatcher counts as one, on
            purpose: it is a place the rule has to be remembered.
  selfhost  the number of registry rows in self_host_registry.rs whose call
            name is `<module>.<name>` or `<module>.<name>_<shape>` — the
            prim-floor Almide copies the incumbent leg and the interp run.

`total` is their sum. This script only COUNTS and prints one TSV row per hole;
the gate that compares against proofs/impl-count-ledger.toml — and the only
writer of that ledger — is `tools/almide-gates impl-count` (the ledger is a
structured artifact, so its reader is Almide, #2128).

Usage: impl-count-enumerate.py <repo-root>
"""
import os
import re
import sys

root = sys.argv[1]

HOLE = re.compile(r"^fn\s+([a-z_][a-z0-9_]*)\b.*=\s*_\s*$")
INTRINSIC = re.compile(r"^@intrinsic\(")
LOWER_FN = re.compile(r"^\s*(?:pub(?:\(crate\))?\s+)?fn\s+lower_([a-z0-9_]+)\s*[<(]")
REG_CALL = re.compile(r'\(\s*"[a-z0-9_]+"\s*,\s*"([a-z0-9]+)\.([a-z0-9_]+)"\s*\)')

# 1. holes, per stdlib module file (module = file stem before the first `_`
#    part is NOT assumed: only top-level <module>.almd files declare holes).
holes = []  # (module, name, native)
stdlib = os.path.join(root, "stdlib")
for fname in sorted(os.listdir(stdlib)):
    if not fname.endswith(".almd") or "_" in fname[:-5]:
        continue
    module = fname[:-5]
    with open(os.path.join(stdlib, fname), encoding="utf-8") as f:
        lines = f.read().split("\n")
    for i, line in enumerate(lines):
        m = HOLE.match(line)
        if not m:
            continue
        window = lines[max(0, i - 3):i]
        native = 1 if any(INTRINSIC.match(w) for w in window) else 0
        holes.append((module, m.group(1), native))

# 2. emitter lowerings: every `fn lower_*` name in the wasm crate.
lower_names = []
wasm_src = os.path.join(root, "crates", "almide-wasm", "src")
for fname in sorted(os.listdir(wasm_src)):
    if not fname.endswith(".rs"):
        continue
    with open(os.path.join(wasm_src, fname), encoding="utf-8") as f:
        for line in f:
            m = LOWER_FN.match(line)
            if m:
                lower_names.append(m.group(1))

# 3. self-host registry call names.
reg_calls = []
with open(os.path.join(root, "crates", "almide-types", "src", "self_host_registry.rs"), encoding="utf-8") as f:
    for m in REG_CALL.finditer(f.read()):
        reg_calls.append((m.group(1), m.group(2)))


def count_emitter(module, name):
    exact = f"{module}_{name}"
    prefix = exact + "_"
    return sum(1 for n in lower_names if n == exact or n.startswith(prefix))


def count_selfhost(module, name):
    return sum(1 for (m, c) in reg_calls if m == module and (c == name or c.startswith(name + "_")))


rows = []
for module, name, native in holes:
    e = count_emitter(module, name)
    s = count_selfhost(module, name)
    rows.append((module, name, native, e, s, native + e + s))

for module, name, native, e, s, t in rows:
    print(f"{module}\t{name}\t{native}\t{e}\t{s}\t{t}")
