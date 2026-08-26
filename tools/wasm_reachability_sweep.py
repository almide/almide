#!/usr/bin/env python3
"""Per-FUNCTION native <-> wasm reachability sweep over the PUBLIC stdlib surface.

WHY THIS EXISTS
---------------
`proofs/check-wasm-fallback.sh` measures parity at the TEST-FILE level: which
spec files the wasm leg cannot run. That makes a stdlib function nobody tests
INVISIBLE — `datetime.monotonic_ns` had no wasm body at all and no spec test,
so no file fell back, and the ratchet read a clean 18/18 while a stdlib
function simply did not exist on one of the two first-class targets.

This sweep closes that blind spot by asking the question per FUNCTION: build one
minimal program per intrinsic-backed stdlib fn and compare the two legs.

THE THREE-STATE CLASSIFICATION (the native build is the CONTROL)
----------------------------------------------------------------
  UNPROBEABLE  `almide check` rejects the synthesized call -> the ARGUMENT
               SYNTHESIS failed, not the compiler. Never counted as parity and
               never counted as a gap; reported so the sweep's own coverage is
               visible. A sweep that silently drops what it cannot build reads
               as green while measuring nothing.
  PARITY       checks, builds native, builds wasm.
  GAP          checks and builds NATIVE, but the wasm renderer walls. This is
               the signal: the function exists on one target and not the other.

A type error must never be reported as a wall — that is the trap this control
exists to defeat (`net.tcp_is_open(0)` fails to CHECK because the handle is not
an Int literal, which looks identical to a wasm wall if you only build wasm).
"""
import argparse, json, os, re, subprocess, sys, tempfile

REPO = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
ALMIDE = os.environ.get("ALMIDE_BIN", os.path.join(REPO, "target/release/almide"))

# Modules requiring an explicit `import` (CLAUDE.md); everything else is auto-imported.
NEEDS_IMPORT = {"json", "fs", "http", "env", "io", "random", "regex", "process",
                "testing", "net", "zlib", "base64", "hash", "hex", "html", "path", "compute",
                "url"}

# Type -> a literal that inhabits it. Reachability only needs the call to RENDER,
# so an argument that would panic at runtime (an out-of-range index) is fine; an
# argument that does not TYPE is not, and lands in UNPROBEABLE.
LITERALS = {
    "Int": "0", "Float": "0.0", "Bool": "true", "String": '"a"',
    "Bytes": "bytes.from_list([1, 2, 3, 4, 5, 6, 7, 8])",
    "List[Int]": "[1, 2]", "List[Float]": "[1.0, 2.0]", "List[String]": '["a", "b"]',
    "List[Bytes]": "[bytes.from_list([1])]",
    "Int8": "(1).to_int8()", "Int16": "(1).to_int16()", "Int32": "(1).to_int32()",
    "Int64": "(1).to_int64()", "UInt8": "(1).to_uint8()", "UInt16": "(1).to_uint16()",
    "UInt32": "(1).to_uint32()", "UInt64": "(1).to_uint64()",
    "Float32": "(1.0).to_float32()", "Float64": "(1.0).to_float64()",
}

DECL_RX = re.compile(
    r'@intrinsic\("([^"]+)"\)\s*\n\s*(pub )?(effect )?fn (\w+)\(([^)]*)\)\s*->\s*([^=\n]+?)\s*=',
    re.M)


def stdlib_modules():
    """STDLIB_MODULES re-read from stdlib_info.rs so the list cannot drift."""
    src = open(os.path.join(REPO, "crates/almide-types/src/stdlib_info.rs")).read()
    m = re.search(r"pub const STDLIB_MODULES[^=]*= &\[(.*?)\];", src, re.S)
    return re.findall(r'"([a-z0-9_]+)"', m.group(1))


def surface(module_filter=None):
    """Every PUBLIC stdlib fn, from the module-interface JSON — not the
    @intrinsic regex: the old enumeration missed every self-host-only fn
    (list.group_by, map.upsert, result.partition, ...), which is exactly the
    class most likely to lack a wasm body. `__`-prefixed internal carriers
    (E043) are not surface."""
    out = []
    for module in stdlib_modules():
        if module_filter and module not in module_filter:
            continue
        rc = subprocess.run([ALMIDE, "compile", module, "--json"],
                            capture_output=True, text=True, cwd=REPO)
        if rc.returncode != 0:
            continue
        for f in json.loads(rc.stdout)["functions"]:
            if f["name"].startswith("__"):
                continue
            out.append(dict(module=module, name=f["name"],
                            effect=bool(f.get("effect")),
                            params=f["params"],
                            ret="Unit" if f["return"].get("kind") == "unit" else "T",
                            ret_json=f["return"]))
    return out


NAMED_CANDIDATES = {
    "Value": ['json.parse("1")!', 'json.parse("1")'],
    "Int8": ["(1).to_int8()"], "Int16": ["(1).to_int16()"],
    "Int32": ["(1).to_int32()"], "Int64": ["(1).to_int64()"],
    "UInt8": ["(1).to_uint8()"], "UInt16": ["(1).to_uint16()"],
    "UInt32": ["(1).to_uint32()"], "UInt64": ["(1).to_uint64()"],
    "Float32": ["(1.0).to_float32()"], "Float64": ["(1.0).to_float64()"],
    "Endian": ["LittleEndian"],
}


def synth_ty(ty, depth=0):
    """Candidate spellings for a value of a JSON-shaped type."""
    if depth > 4:
        return []
    k = ty.get("kind")
    if k == "int": return ["1"]
    if k == "float": return ["1.5"]
    if k == "string": return ['"a"']
    if k == "bool": return ["true"]
    if k == "unit": return ["()"]
    if k == "bytes": return ["bytes.from_list([1, 2, 3, 4, 5, 6, 7, 8])"]
    if k == "matrix": return ["matrix.ones(1, 1)"]
    if k in ("named", "type_var"):
        name = ty.get("name", "")
        if name in NAMED_CANDIDATES: return NAMED_CANDIDATES[name]
        if len(name) == 1 and name.isupper(): return ["1"]
        return []
    if k == "list":
        return [f"[{c}]" for c in synth_ty(ty.get("inner", {}), depth + 1)[:1]]
    if k == "set":
        return [f"set.from_list([{c}])" for c in synth_ty(ty.get("inner", {}), depth + 1)[:1]]
    if k == "option":
        return [f"some({c})" for c in synth_ty(ty.get("inner", {}), depth + 1)[:1]]
    if k == "result":
        return [f"ok({c})" for c in synth_ty(ty.get("ok", ty.get("inner", {})), depth + 1)[:1]]
    if k == "map":
        ks = synth_ty(ty.get("key", {}), depth + 1)
        vs = synth_ty(ty.get("value", {}), depth + 1)
        return [f"[{ks[0]}: {vs[0]}]"] if ks and vs else []
    if k == "tuple":
        parts = [synth_ty(e, depth + 1) for e in ty.get("elements", []) or []]
        return ["(" + ", ".join(x[0] for x in parts) + ")"] if parts and all(parts) else []
    if k == "fn":
        rets = synth_ty(ty.get("return", {}), depth + 1)
        if not rets:
            return []
        names = ", ".join(f"p{i}" for i in range(len(ty.get("params", []))))
        return [f"({names}) => {rets[0]}"]
    return []


def synth_args(params):
    """One argument spelling per param (first candidate), or None."""
    out = []
    for prm in params:
        c = synth_ty(prm["type"])
        if not c:
            return None
        out.append((c[0], prm["type"].get("kind")))
    return out


def programs(f, args):
    """Candidate probe shapes. `var`-hoisted receivers (a `let` receiver cannot
    be passed to an in-place mutator, which alone accounted for most of the old
    UNPROBEABLE set), the mixed shape (scalar literals inline — an all-hoisted
    scalar `var` binding hits a v1 subset wall the call itself never does), and
    both statement positions. The verdict must describe the FUNCTION, not the
    wrapper, so every shape gets a turn (see classify)."""
    imp = f"import {f['module']}\n\n" if f["module"] in NEEDS_IMPORT else ""
    scalar = {"int", "float", "string", "bool", "unit"}
    all_binds = "".join(f"  var a{i} = {a}\n" for i, (a, _) in enumerate(args))
    all_names = ", ".join(f"a{i}" for i in range(len(args)))
    mixed_binds, mixed_names = "", []
    for i, (a, kind) in enumerate(args):
        if kind in scalar:
            mixed_names.append(a)
        else:
            mixed_binds += f"  var a{i} = {a}\n"
            mixed_names.append(f"a{i}")
    bang = "!" if f["effect"] else ""
    head = "effect fn" if f["effect"] else "fn"
    shapes = []
    for binds, names in ((all_binds, all_names), (mixed_binds, ", ".join(mixed_names))):
        call = f"{f['module']}.{f['name']}({names})"
        bodies = [f"  let _ = {call}{bang}", f"  {call}{bang}"]
        if f["ret"] == "Unit":               # a Unit result binds to nothing useful
            bodies.reverse()
        for b in bodies:
            prog = f"{imp}{head} main() -> Unit = {{\n{binds}{b}\n}}\n"
            if prog not in shapes:
                shapes.append(prog)
    return shapes


def run(cmd):
    p = subprocess.run(cmd, capture_output=True, text=True, cwd=REPO)
    return p.returncode, (p.stdout or "") + (p.stderr or "")


WALL_RX = re.compile(r"not yet supported by the verified wasm renderer|^wall:", re.M)


def classify(f):
    args = synth_args(f["params"])
    if args is None:
        return "UNPROBEABLE", "parameter type outside the synthesizer"
    first_gap = None
    any_native = False
    for src in programs(f, args):
        tf = tempfile.NamedTemporaryFile("w", suffix=".almd", delete=False, dir="/tmp")
        tf.write(src); tf.close()
        try:
            # The CONTROL: this shape has to type and build natively, or it says
            # nothing about the function and the next shape gets a turn.
            if run([ALMIDE, "check", tf.name])[0] != 0:
                continue
            if run([ALMIDE, "build", tf.name, "-o", tf.name + ".bin"])[0] != 0:
                continue
            any_native = True
            rc, out = run([ALMIDE, "build", tf.name, "--target", "wasm", "-o", tf.name + ".wasm"])
            if rc == 0:
                return "PARITY", ""
            if first_gap is None:
                m = re.search(r"wall: (.+)", out)
                first_gap = (m.group(1)[:160] if m else
                             "wasm renderer wall" if WALL_RX.search(out) else "wasm build failed")
        finally:
            for ext in ("", ".bin", ".wasm"):
                try: os.unlink(tf.name + ext)
                except OSError: pass
    if any_native:
        return "GAP", first_gap or "wasm build failed"
    return "UNPROBEABLE", "no candidate shape builds natively"


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--modules", nargs="*", help="limit to these modules")
    ap.add_argument("--json", help="write the raw result rows here")
    a = ap.parse_args()
    fns = surface(set(a.modules) if a.modules else None)
    rows, counts = [], {"PARITY": 0, "GAP": 0, "UNPROBEABLE": 0}
    for i, f in enumerate(fns, 1):
        verdict, why = classify(f)
        counts[verdict] += 1
        rows.append(dict(**f, verdict=verdict, why=why))
        print(f"\r  {i}/{len(fns)}  parity={counts['PARITY']} "
              f"gap={counts['GAP']} unprobeable={counts['UNPROBEABLE']}", end="", file=sys.stderr)
    print(file=sys.stderr)
    if a.json:
        json.dump(rows, open(a.json, "w"), indent=1)
    for r in rows:
        if r["verdict"] == "GAP":
            print(f"GAP  {r['module']}.{r['name']}  :: {r['why']}")
    n = len(fns)
    probed = counts["PARITY"] + counts["GAP"]
    print(f"\n{n} public fns: {counts['PARITY']} parity, {counts['GAP']} gap, "
          f"{counts['UNPROBEABLE']} unprobeable "
          f"({100*probed//n if n else 0}% of the surface actually measured)")


if __name__ == "__main__":
    main()
