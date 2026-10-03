#!/usr/bin/env python3
"""Enumerate how many implementations each stdlib HOLE has.

A hole is a stdlib declaration whose body is `_` (`fn name[...](...) -> T = _`):
the language declares the operation and hands its body to a backend. Tonight's
release-blocker stream (#2395 / #2397 / #2398) was three instances of one
shape — the same operation implemented more than once, a rule landing in one
copy and not reaching the others — so this enumerator counts the copies.

Holes are read from EVERY `stdlib/*.almd` — a module's top-level file and its
sub-files alike (#3254: http_call / http_framed / http_serve / process_wasm
declared 23 holes the top-level-only walk never saw). Each file's module comes
from the compiler's own tables (`bundled_source()` arms, self_host_registry.rs
rows), never from its filename.

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
import tempfile

root = sys.argv[1]

# A hole is declared by `fn` OR `effect fn` (#3242: `effect fn exec(..) = _`
# and the rest of the effectful families were invisible to `^fn`).
HOLE = re.compile(r"^(?:effect\s+)?fn\s+([a-z_][a-z0-9_]*)\b.*=\s*_\s*$")
INTRINSIC = re.compile(r"^@intrinsic\(")

# Negative control, run on every invocation: each declaration spelling that
# can carry a hole must be counted, and a bodied fn must not. A pattern that
# goes blind to one spelling fails here, loudly, instead of quietly dropping
# that spelling's rows from the count.
for line, want in [
    ("fn pid() -> Int = _", "pid"),
    ('effect fn exec(cmd: String, args: List[String]) -> Result[String, String] = _', "exec"),
    ("effect  fn exit(code: Int) -> Never = _", "exit"),
    ("fn len(s: String) -> Int = s.len()", None),
    ("effect fn run() -> Unit = _ignored()", None),
    ("// effect fn exec(..) = _", None),
]:
    m = HOLE.match(line)
    got = m.group(1) if m else None
    if got != want:
        sys.exit(f"impl-count-enumerate: HOLE pattern self-check failed on {line!r}: got {got!r}, want {want!r}")
LOWER_FN = re.compile(r"^\s*(?:pub(?:\(crate\))?\s+)?fn\s+lower_([a-z0-9_]+)\s*[<(]")
REG_CALL = re.compile(r'\(\s*"[a-z0-9_]+"\s*,\s*"([a-z0-9]+)\.([a-z0-9_]+)"\s*\)')

BUNDLED_ARM = re.compile(r'"([a-z0-9_]+)"\s*=>\s*Some\(\s*crate::embedded::SRC_([A-Z0-9_]+)\s*\)')
EMBEDDED_SRC = re.compile(r"crate::embedded::SRC_([A-Z0-9_]+)")


def module_map(stdlib_info_text, registry_text):
    """stem -> owning module, from the compiler's own tables, never from a
    filename guess (#3254). A top-level module file is the arm of
    `bundled_source()` in stdlib_info.rs (`"http" => Some(SRC_HTTP)`); a
    sub-file is owned by the module its self_host_registry.rs rows register
    calls under (`(SRC_HTTP_CALL, &[(.., "http.poll"), ..])`). A stem whose rows
    name two modules maps to the set, and the caller refuses to guess."""
    owners = {}
    for m in BUNDLED_ARM.finditer(stdlib_info_text):
        owners.setdefault(m.group(2).lower(), set()).add(m.group(1))
    spans = list(EMBEDDED_SRC.finditer(registry_text))
    for k, m in enumerate(spans):
        end = spans[k + 1].start() if k + 1 < len(spans) else len(registry_text)
        mods = {c.group(1) for c in REG_CALL.finditer(registry_text[m.end():end])}
        if mods:
            owners.setdefault(m.group(1).lower(), set()).update(mods)
    return owners


def enumerate_holes(stdlib_dir, owners):
    """Every hole in every `stdlib/*.almd` — top-level module files AND their
    sub-files (http_call.almd, process_wasm.almd, ...) — attributed to its
    module. A file that declares a hole but has no single owning module is an
    error: a hole nobody can attribute is a hole nobody counts."""
    out = []  # (module, name, native)
    for fname in sorted(os.listdir(stdlib_dir)):
        if not fname.endswith(".almd"):
            continue
        stem = fname[:-5]
        with open(os.path.join(stdlib_dir, fname), encoding="utf-8") as f:
            lines = f.read().split("\n")
        found = []
        for i, line in enumerate(lines):
            m = HOLE.match(line)
            if not m:
                continue
            window = lines[max(0, i - 3):i]
            native = 1 if any(INTRINSIC.match(w) for w in window) else 0
            found.append((m.group(1), native))
        if not found:
            continue
        mods = owners.get(stem, set())
        if len(mods) != 1:
            sys.exit(f"impl-count-enumerate: stdlib/{fname} declares {len(found)} hole(s) but its owning module "
                     f"is {sorted(mods) or 'unknown'} — wire it in bundled_source() or self_host_registry.rs")
        module = next(iter(mods))
        out.extend((module, name, native) for name, native in found)
    return out


# Negative control for the file walk (#3254): a hole in a sub-file must be
# counted under its module. Narrowing the walk back to top-level
# `<module>.almd` files drops `sub_leaf` and fails here.
with tempfile.TemporaryDirectory() as tmp:
    with open(os.path.join(tmp, "demo.almd"), "w", encoding="utf-8") as f:
        f.write("fn top() -> Int = _\n")
    with open(os.path.join(tmp, "demo_sub.almd"), "w", encoding="utf-8") as f:
        f.write("@intrinsic(\"x\")\neffect fn __sub_leaf(a: String) -> Result[String, String] = _\n")
    demo_owners = module_map(
        '"demo" => Some(crate::embedded::SRC_DEMO),',
        '(crate::embedded::SRC_DEMO_SUB, &[("demo_sub_impl", "demo.sub")]),',
    )
    got = enumerate_holes(tmp, demo_owners)
    want = [("demo", "top", 0), ("demo", "__sub_leaf", 1)]
    if got != want:
        sys.exit(f"impl-count-enumerate: sub-file self-check failed: got {got!r}, want {want!r}")

# 1. holes, in every stdlib source file, attributed to the owning module.
with open(os.path.join(root, "crates", "almide-types", "src", "stdlib_info.rs"), encoding="utf-8") as f:
    _stdlib_info = f.read()
with open(os.path.join(root, "crates", "almide-types", "src", "self_host_registry.rs"), encoding="utf-8") as f:
    _registry = f.read()
holes = enumerate_holes(os.path.join(root, "stdlib"), module_map(_stdlib_info, _registry))

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
reg_calls = [(m.group(1), m.group(2)) for m in REG_CALL.finditer(_registry)]


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
