#!/usr/bin/env python3
"""Per-FUNCTION native <-> wasm reachability sweep over the @intrinsic stdlib surface.

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
                "testing", "net", "zlib", "base64", "hex", "html", "path", "compute"}

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


def surface(module_filter=None):
    """Every @intrinsic-backed fn on a module's DECLARATION surface (the bare
    stdlib/<module>.almd; the <module>_*.almd parts are implementations)."""
    out = []
    for fn in sorted(os.listdir(os.path.join(REPO, "stdlib"))):
        if not fn.endswith(".almd"):
            continue
        module = fn[:-5]
        if "_" in module or module == "prim":   # parts are not the surface; prim IS the wasm floor
            continue
        if module_filter and module not in module_filter:
            continue
        src = open(os.path.join(REPO, "stdlib", fn)).read()
        for m in DECL_RX.finditer(src):
            out.append(dict(module=module, name=m.group(4), effect=bool(m.group(3)),
                            params=m.group(5).strip(), ret=m.group(6).strip()))
    return out


def synth_args(params):
    """Literals for a parameter list, or None when a type is outside the table."""
    if not params.strip():
        return ""
    args = []
    depth, cur = 0, ""
    for ch in params + ",":          # split on top-level commas only
        if ch == "," and depth == 0:
            args.append(cur.strip()); cur = ""
            continue
        if ch in "[(": depth += 1
        if ch in "])": depth -= 1
        cur += ch
    lits = []
    for a in args:
        if not a:
            continue
        ty = a.split(":", 1)[1].strip() if ":" in a else None
        if ty is None:
            return None
        ty = ty.removesuffix("?")     # an Option param still accepts the bare value
        if ty not in LITERALS:
            return None
        lits.append(LITERALS[ty])
    return ", ".join(lits)


def program(f, args):
    imp = f"import {f['module']}\n\n" if f["module"] in NEEDS_IMPORT else ""
    call = f"{f['module']}.{f['name']}({args})"
    if f["effect"]:
        return f"{imp}effect fn main() -> Unit = {{\n  let _ = {call}!\n}}\n"
    return f"{imp}fn main() -> Unit = {{\n  let _ = {call}\n}}\n"


def run(cmd):
    p = subprocess.run(cmd, capture_output=True, text=True, cwd=REPO)
    return p.returncode, (p.stdout or "") + (p.stderr or "")


WALL_RX = re.compile(r"not yet supported by the verified wasm renderer|^wall:", re.M)


def classify(f):
    args = synth_args(f["params"])
    if args is None:
        return "UNPROBEABLE", "parameter type outside the literal table"
    src = program(f, args)
    tf = tempfile.NamedTemporaryFile("w", suffix=".almd", delete=False, dir="/tmp")
    tf.write(src); tf.close()
    try:
        rc, out = run([ALMIDE, "check", tf.name])
        if rc != 0:
            return "UNPROBEABLE", "synthesized call does not type"
        rc, out = run([ALMIDE, "build", tf.name, "-o", tf.name + ".bin"])
        if rc != 0:
            return "UNPROBEABLE", "native build failed (no control)"
        rc, out = run([ALMIDE, "build", tf.name, "--target", "wasm", "-o", tf.name + ".wasm"])
        if rc == 0:
            return "PARITY", ""
        reason = "wasm renderer wall" if WALL_RX.search(out) else "wasm build failed"
        m = re.search(r"wall: (.+)", out)
        return "GAP", (m.group(1)[:160] if m else reason)
    finally:
        for ext in ("", ".bin", ".wasm"):
            try: os.unlink(tf.name + ext)
            except OSError: pass


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
    print(f"\n{n} intrinsic fns: {counts['PARITY']} parity, {counts['GAP']} gap, "
          f"{counts['UNPROBEABLE']} unprobeable "
          f"({100*probed//n if n else 0}% of the surface actually measured)")


if __name__ == "__main__":
    main()
