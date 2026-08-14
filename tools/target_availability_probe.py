#!/usr/bin/env python3
"""#1423 Stage 1 — measure the per-fn wasm availability matrix with the
renderer's own eyes.

For every public stdlib fn (module interface JSON), synthesize a minimal
type-correct call, `almide check` it, and attempt the WASM build. Record:

  ok           — the wasm leg lowers and links the call
  wall(reason) — the wasm renderer refuses, with its own named reason
  skip(reason) — the probe could not synthesize a checkable call
                 (reported, never silently dropped — a skip is a hole in
                 the measurement, and the ledger only means something if
                 the holes are enumerated next to the data)

A name-diff over self_host_registry.rs over-reports by ~199 rows because
linkage is multi-mechanism (registry splice / WAT prelude / prim direct) —
which is why this probe asks the renderer instead (the corpus-wall recipe).

Synthesis is arbitrated by the checker, not assumed: each parameter type maps
to a list of CANDIDATE spellings and the first program `almide check` accepts
wins; generic-return producers get a typed-discard candidate so E018-class
undecidables still measure.

Usage: python3 tools/target_availability_probe.py [--almide BIN] [--out FILE]
Exit code is always 0 — Stage 1 is measurement; the gate is Stage 2.
"""

import argparse
import json
import re
import subprocess
import sys
import tempfile
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent

# The documented public surface (stdlib_info.rs STDLIB_MODULES) — re-read from
# source so this list cannot drift.
def stdlib_modules():
    src = (REPO / "crates/almide-types/src/stdlib_info.rs").read_text()
    m = re.search(r"pub const STDLIB_MODULES[^=]*= &\[(.*?)\];", src, re.S)
    if not m:
        sys.exit("cannot find STDLIB_MODULES in stdlib_info.rs")
    return re.findall(r'"([a-z0-9_]+)"', m.group(1))

# Modules that need an explicit `import` (CLAUDE.md; everything else in
# STDLIB_MODULES is auto-imported).
EXPLICIT_IMPORT = {"json", "fs", "http", "env", "io", "random", "regex", "process", "testing", "net", "zlib"}

# Concrete named types the probe knows how to build, as candidate spellings
# (first one the checker accepts wins). An empty list = knowingly
# unsynthesizable -> skip with the type named in the reason.
NAMED_CANDIDATES = {
    "Value": ['json.parse("1")!', 'json.parse("1")', "value.from_int(1)", "value.of_int(1)"],
    "Int8": ["int.to_int8(1)", "1.to_int8()"],
    "Int16": ["int.to_int16(1)", "1.to_int16()"],
    "Int32": ["int.to_int32(1)", "1.to_int32()"],
    "Int64": ["int.to_int64(1)", "1.to_int64()"],
    "UInt8": ["int.to_uint8(1)", "1.to_uint8()"],
    "UInt16": ["int.to_uint16(1)", "1.to_uint16()"],
    "UInt32": ["int.to_uint32(1)", "1.to_uint32()"],
    "UInt64": ["int.to_uint64(1)", "1.to_uint64()"],
    "Float32": ["float.to_float32(1.0)", "1.0.to_float32()"],
    "Float64": ["float.to_float64(1.0)", "1.0.to_float64()"],
    "Endian": ["LittleEndian"],
}

def synth(ty, depth=0):
    """Candidate spellings for a value of `ty` (checker arbitrates)."""
    if depth > 4:
        return []
    k = ty.get("kind")
    if k == "int":
        return ["1"]
    if k == "float":
        return ["1.5"]
    if k == "string":
        return ['"x"']
    if k == "bool":
        return ["true"]
    if k == "unit":
        return ["()"]
    if k == "bytes":
        return ["bytes.new(1)"]
    if k == "matrix":
        return ["matrix.ones(1, 1)"]
    if k in ("named", "type_var"):
        name = ty.get("name", "")
        if name in NAMED_CANDIDATES:
            return NAMED_CANDIDATES[name]
        if len(name) == 1 and name.isupper():  # a generic — instantiate at Int
            return ["1"]
        return []
    if k == "list":
        return [f"[{c}]" for c in synth(ty.get("inner", {}), depth + 1)[:1]]
    if k == "set":
        return [f"set.from_list([{c}])" for c in synth(ty.get("inner", {}), depth + 1)[:1]]
    if k == "option":
        return [f"some({c})" for c in synth(ty.get("inner", {}), depth + 1)[:1]]
    if k == "result":
        ok = synth(ty.get("ok", ty.get("inner", {})), depth + 1)
        return [f"ok({c})" for c in ok[:1]]
    if k == "map":
        ks = synth(ty.get("key", {}), depth + 1)
        vs = synth(ty.get("value", {}), depth + 1)
        if ks and vs:
            return [f"[{ks[0]}: {vs[0]}]"]
        return []
    if k == "tuple":
        parts = [synth(e, depth + 1) for e in ty.get("elements", ty.get("inner", []) or [])]
        if all(parts):
            return ["(" + ", ".join(p[0] for p in parts) + ")"]
        return []
    if k == "fn":
        params = ty.get("params", [])
        rets = synth(ty.get("return", {}), depth + 1)
        if not rets:
            return []
        names = [f"p{i}" for i in range(len(params))]
        return ["(" + ", ".join(names) + ") => " + rets[0]]
    return []

def render_ty(ty):
    """Spell `ty` as Almide type syntax with generics instantiated at Int
    (for the typed-discard candidate). None when unspellable."""
    k = ty.get("kind")
    simple = {"int": "Int", "float": "Float", "string": "String", "bool": "Bool",
              "unit": "Unit", "bytes": "Bytes", "matrix": "Matrix"}
    if k in simple:
        return simple[k]
    if k in ("named", "type_var"):
        name = ty.get("name", "")
        if len(name) == 1 and name.isupper():
            return "Int"
        return name or None
    if k == "list":
        i = render_ty(ty.get("inner", {}))
        return f"List[{i}]" if i else None
    if k == "set":
        i = render_ty(ty.get("inner", {}))
        return f"Set[{i}]" if i else None
    if k == "option":
        i = render_ty(ty.get("inner", {}))
        return f"Option[{i}]" if i else None
    if k == "result":
        o = render_ty(ty.get("ok", ty.get("inner", {})))
        e = render_ty(ty.get("err", {"kind": "string"}))
        return f"Result[{o}, {e}]" if o and e else None
    if k == "map":
        a = render_ty(ty.get("key", {}))
        b = render_ty(ty.get("value", {}))
        return f"Map[{a}, {b}]" if a and b else None
    if k == "tuple":
        es = [render_ty(e) for e in ty.get("elements", []) or []]
        return "(" + ", ".join(es) + ")" if es and all(es) else None
    if k == "fn":
        ps = [render_ty(p) for p in ty.get("params", [])]
        r = render_ty(ty.get("return", {}))
        return "(" + ", ".join(ps) + ") -> " + r if r and all(ps) else None
    return None

def programs_for(mod, fn):
    """Candidate probe programs for one fn, in preference order."""
    arg_cands = [synth(p["type"]) for p in fn["params"]]
    if not all(arg_cands):
        missing = [render_ty(p["type"]) or json.dumps(p["type"]) for p, c in
                   zip(fn["params"], arg_cands) if not c]
        return [], f"unsynthesizable param type(s): {', '.join(missing)}"
    call = f"{mod}.{fn['name']}(" + ", ".join(c[0] for c in arg_cands) + ")"
    if not fn["params"]:
        call = f"{mod}.{fn['name']}()"
    # A var-hoisted shape fixes E032 (a `mut` receiver cannot be a temporary):
    # every arg binds to a var first, then passes by name.
    binds = "\n".join(f"  var a{i} = {c[0]}" for i, c in enumerate(arg_cands))
    hoisted_call = f"{mod}.{fn['name']}(" + ", ".join(f"a{i}" for i in range(len(arg_cands))) + ")"
    bodies = [f"  let _ = {call}\n  ()"]
    ret_spelled = render_ty(fn["return"])
    if ret_spelled and ret_spelled != "Unit":
        bodies.append(f"  let _x: {ret_spelled} = {call}\n  ()")
    if fn["params"]:
        bodies.append(f"{binds}\n  let _ = {hoisted_call}\n  ()")
        if ret_spelled and ret_spelled != "Unit":
            bodies.append(f"{binds}\n  let _x: {ret_spelled} = {hoisted_call}\n  ()")
    progs = []
    for body in bodies:
        prog_body = f"effect fn main() -> Unit = {{\n{body}\n}}\n"
        used = {m for m in EXPLICIT_IMPORT if f"{m}." in prog_body} | ({mod} & EXPLICIT_IMPORT if False else set())
        if mod in EXPLICIT_IMPORT:
            used.add(mod)
        imp = "".join(f"import {m}\n" for m in sorted(used))
        progs.append((imp + "\n" if imp else "") + prog_body)
    return progs, None

WALL_PATTERNS = [
    re.compile(r"unlinked stdlib/runtime call\(s\)[^\n]*"),
    re.compile(r"outside the MIR-lowering subset[^\n]*"),
    re.compile(r"wall(?:ed)?[^\n]*", re.I),
    re.compile(r"error\[[^\n]*"),
    re.compile(r"Error:[^\n]*"),
]

def wall_reason(stderr):
    for pat in WALL_PATTERNS:
        m = pat.search(stderr)
        if m:
            return m.group(0)[:220]
    tail = [l for l in stderr.strip().splitlines() if l.strip()]
    return (tail[-1][:220] if tail else "no diagnostic captured")

def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--almide", default=str(REPO / "target/release/almide"))
    ap.add_argument("--out", default=str(REPO / "proofs/target-availability-measured.toml"))
    ap.add_argument("--modules", nargs="*", help="probe only these modules")
    args = ap.parse_args()

    mods = args.modules or stdlib_modules()
    rows = []           # (module, fn, status, detail)
    with tempfile.TemporaryDirectory() as td:
        td = Path(td)
        for mod in mods:
            r = subprocess.run([args.almide, "compile", mod, "--json"],
                               capture_output=True, text=True)
            if r.returncode != 0:
                rows.append((mod, "*", "skip", f"module interface failed: {wall_reason(r.stderr)}"))
                continue
            iface = json.loads(r.stdout)
            for fn in iface["functions"]:
                # `__`-prefixed names are internal carriers (E043: source may
                # not name them) — not public surface, excluded outright.
                if fn["name"].startswith("__"):
                    continue
                progs, why = programs_for(mod, fn)
                if not progs:
                    rows.append((mod, fn["name"], "skip", why))
                    continue
                src = td / "probe.almd"
                checked = None
                last_check_err = ""
                for prog in progs:
                    src.write_text(prog)
                    c = subprocess.run([args.almide, "check", str(src)],
                                       capture_output=True, text=True)
                    if c.returncode == 0:
                        checked = prog
                        break
                    last_check_err = wall_reason(c.stderr + c.stdout)
                if checked is None:
                    rows.append((mod, fn["name"], "skip", f"no candidate checks: {last_check_err}"))
                    continue
                src.write_text(checked)
                b = subprocess.run([args.almide, "build", str(src), "--target", "wasm",
                                    "-o", str(td / "probe.wasm")],
                                   capture_output=True, text=True)
                if b.returncode == 0:
                    rows.append((mod, fn["name"], "ok", ""))
                else:
                    rows.append((mod, fn["name"], "wall", wall_reason(b.stderr + b.stdout)))
                print(f"  {mod}.{fn['name']}: {rows[-1][2]}", file=sys.stderr)

    ok = [r for r in rows if r[2] == "ok"]
    wall = [r for r in rows if r[2] == "wall"]
    skip = [r for r in rows if r[2] == "skip"]
    out = []
    out.append("# MEASURED wasm availability of the public stdlib surface (#1423 Stage 1).")
    out.append("# Generated by tools/target_availability_probe.py — the renderer's own")
    out.append("# verdict on a minimal checked call per fn. Regenerate; do not hand-edit.")
    out.append(f"# totals: ok={len(ok)} wall={len(wall)} skip={len(skip)} of {len(rows)}")
    out.append("")
    for status, group in (("wall", wall), ("skip", skip)):
        for mod, name, _, detail in group:
            out.append("[[fn]]")
            out.append(f'name = "{mod}.{name}"')
            out.append(f'status = "{status}"')
            safe = detail.replace("\\", "\\\\").replace('"', '\\"')
            out.append(f'reason = "{safe}"')
            out.append("")
    out.append("# both-leg ok:")
    for mod, name, _, _ in ok:
        out.append(f'# ok: {mod}.{name}')
    Path(args.out).write_text("\n".join(out) + "\n")
    print(f"\nok={len(ok)} wall={len(wall)} skip={len(skip)} (of {len(rows)}) -> {args.out}")

if __name__ == "__main__":
    main()
