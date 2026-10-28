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

STDLIB_INFO = os.path.join(REPO, "crates/almide-types/src/stdlib_info.rs")


def rust_str_list(const):
    """The names of a `pub const <const>: &[&str]` in stdlib_info.rs, re-read
    so the sweep holds no copy that can drift."""
    src = open(STDLIB_INFO, encoding="utf-8").read()
    m = re.search(rf"pub const {const}[^=]*= &\[(.*?)\];", src, re.S)
    body = re.sub(r"//[^\n]*", "", m.group(1))
    return re.findall(r'"([a-z0-9_]+)"', body)


_AUTO_IMPORTED = None


def needs_import(module):
    """Whether a file must write `import <module>`: everything outside the
    auto-import surface (TIER1_ALWAYS_ACCESSIBLE + AUTO_IMPORT_BUNDLED), the
    one list the resolver reads. A hand-kept set here missed every module
    added after it (`secret`, #4107)."""
    global _AUTO_IMPORTED
    if _AUTO_IMPORTED is None:
        _AUTO_IMPORTED = set(rust_str_list("TIER1_ALWAYS_ACCESSIBLE")) | set(rust_str_list("AUTO_IMPORT_BUNDLED"))
    return module not in _AUTO_IMPORTED

_MEMORY_UNSAFE = None
MEMORY_UNSAFE_RX = re.compile(r"^@memory_unsafe\s*\n(?:@[^\n]*\n)*(?:effect )?fn (\w+)", re.M)


def memory_unsafe_fns():
    """(module, fn) for every `@memory_unsafe` declaration in stdlib/<module>.almd,
    read from the declaration the checker reads."""
    global _MEMORY_UNSAFE
    if _MEMORY_UNSAFE is None:
        _MEMORY_UNSAFE = set()
        d = os.path.join(REPO, "stdlib")
        for name in os.listdir(d):
            if name.endswith(".almd"):
                with open(os.path.join(d, name), encoding="utf-8") as fh:
                    for fn in MEMORY_UNSAFE_RX.findall(fh.read()):
                        _MEMORY_UNSAFE.add((name[:-5], fn))
    return _MEMORY_UNSAFE

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
    return rust_str_list("STDLIB_MODULES")


def surface(module_filter=None, modules=None):
    """Every PUBLIC stdlib fn, from the module-interface JSON — not the
    @intrinsic regex: the old enumeration missed every self-host-only fn
    (list.group_by, map.upsert, result.partition, ...), which is exactly the
    class most likely to lack a wasm body. `__`-prefixed internal carriers
    (E043) are not surface."""
    out = []
    for module in (modules if modules is not None else stdlib_modules()):
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
    "Int8": ["int8.from(1)"], "Int16": ["int16.from(1)"],
    "Int32": ["int32.from(1)"], "Int64": ["int64.from(1)"],
    "UInt8": ["uint8.from(1)"], "UInt16": ["uint16.from(1)"],
    "UInt32": ["uint32.from(1)"], "UInt64": ["uint64.from(1)"],
    "Float32": ["float32.from(1.0)"], "Float64": ["float64.from(1.0)"],
    "Deque": ["deque.from_list([1, 2])"], "Heap": ["heap.from_list([1, 2])"],
    "BigInt": ['match bigint.parse("12") { ok(b) => b, err(e) => panic(e) }'],
    "Regex": ['regex.new("a")'],
}


# The instantiations a type variable is probed at, in order: (literal, type).
# The interface JSON does not carry a bound, so a protocol-bounded generic
# (`list.join[T: Text]`) that does not check at Int gets its turn at String.
TYPE_VAR_INSTANTIATIONS = (("1", "Int"), ('"a"', "String"))


def has_type_var(ty):
    """Whether a JSON-shaped type mentions a type variable anywhere."""
    if isinstance(ty, dict):
        if ty.get("kind") == "type_var" or (ty.get("kind") == "named"
                                            and len(ty.get("name", "")) == 1
                                            and ty.get("name", "").isupper()):
            return True
        return any(has_type_var(v) for v in ty.values())
    if isinstance(ty, list):
        return any(has_type_var(v) for v in ty)
    return False


def render_ty(ty, tv="Int"):
    """A type annotation for a JSON-shaped type with every type variable at
    `tv`, or None. A generic constructor that takes no element (`heap.new()`,
    `list.new()`) leaves its element type open, so its probe only types with
    the binding annotated."""
    k = ty.get("kind")
    simple = {"int": "Int", "float": "Float", "string": "String", "bool": "Bool",
              "unit": "Unit", "bytes": "Bytes", "matrix": "Matrix", "type_var": tv}
    if k in simple:
        return simple[k]
    def inner(key):
        return render_ty(ty.get(key, {}), tv)
    if k == "named":
        args = [render_ty(a, tv) for a in ty.get("args", []) or []]
        if any(a is None for a in args):
            return None
        return ty.get("name", "") + (f"[{', '.join(args)}]" if args else "")
    if k in ("list", "set"):
        x = inner("inner")
        return f"{k.capitalize()}[{x}]" if x else None
    if k == "option":
        x = inner("inner")
        return f"{x}?" if x else None
    if k == "map":
        a, b = inner("key"), inner("value")
        return f"Map[{a}, {b}]" if a and b else None
    if k == "tuple":
        parts = [render_ty(e, tv) for e in ty.get("elements", []) or []]
        return "(" + ", ".join(parts) + ")" if parts and all(parts) else None
    return None


def synth_ty(ty, depth=0, tv="1"):
    """Candidate spellings for a value of a JSON-shaped type, every type
    variable spelled by the literal `tv`."""
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
        if len(name) == 1 and name.isupper(): return [tv]
        return constructed(name, depth, tv, ty.get("args") or [])
    if k == "list":
        return [f"[{c}]" for c in synth_ty(ty.get("inner", {}), depth + 1, tv)[:1]]
    if k == "set":
        return [f"set.from_list([{c}])" for c in synth_ty(ty.get("inner", {}), depth + 1, tv)[:1]]
    if k == "option":
        return [f"some({c})" for c in synth_ty(ty.get("inner", {}), depth + 1, tv)[:1]]
    if k == "result":
        return [f"ok({c})" for c in synth_ty(ty.get("ok", ty.get("inner", {})), depth + 1, tv)[:1]]
    if k == "map":
        ks = synth_ty(ty.get("key", {}), depth + 1, tv)
        vs = synth_ty(ty.get("value", {}), depth + 1, tv)
        return [f"[{ks[0]}: {vs[0]}]"] if ks and vs else []
    if k == "tuple":
        parts = [synth_ty(e, depth + 1, tv) for e in ty.get("elements", []) or []]
        return ["(" + ", ".join(x[0] for x in parts) + ")"] if parts and all(parts) else []
    if k == "fn":
        rets = synth_ty(ty.get("return", {}), depth + 1, tv)
        if not rets:
            return []
        names = ", ".join(f"p{i}" for i in range(len(ty.get("params", []))))
        return [f"({names}) => {rets[0]}"]
    return []


# A module reference in an argument spelling (`secret.new(1)`): the module
# the probe has to import.
MODULE_REF_RX = re.compile(r"(?<![\w).])([a-z][a-z0-9_]*)\.[a-z_]")

# Named type -> the public fns that return it. A stdlib type with no literal
# (an opaque `mod type`, `Secret[A]`) is built the way user code builds it:
# through a fn of the surface that returns it. Filled from the surface itself
# in `main`, so a new opaque type is probeable the day its module lands.
CONSTRUCTORS = {}


def mentions_named(ty, name):
    """Whether a JSON-shaped type mentions the named type `name` anywhere."""
    if isinstance(ty, dict):
        if ty.get("kind") == "named" and ty.get("name") == name:
            return True
        return any(mentions_named(v, name) for v in ty.values())
    if isinstance(ty, list):
        return any(mentions_named(v, name) for v in ty)
    return False


def index_constructors(fns):
    """Every pure, memory-safe surface fn whose result is a named type and
    whose params do not need that type already: a constructor a probe can
    call without an `effect` main or an `import ffi` of its own."""
    for f in fns:
        ret = f.get("ret_json", {})
        if ret.get("kind") != "named" or f["effect"] or (f["module"], f["name"]) in memory_unsafe_fns():
            continue
        if not mentions_named(f["params"], ret.get("name")):
            CONSTRUCTORS.setdefault(ret["name"], []).append(f)


def constructed(name, depth, tv, args=()):
    """`module.ctor(args)` for each constructor of `name` whose own params
    synthesize, every type variable at `tv` — or, for an applied type
    (`Secret[Bytes]`), at a value of its first type argument, so the generic
    constructor (`secret.new[A](x: A)`) builds the type the param asks for."""
    if args:
        inner = synth_ty(args[0], depth + 1, tv)
        if inner:
            tv = inner[0]
    out = []
    for c in CONSTRUCTORS.get(name, []):
        parts = [synth_ty(p["type"], depth + 1, tv) for p in c["params"]]
        if all(parts):
            out.append(f"{c['module']}.{c['name']}({', '.join(x[0] for x in parts)})")
    return out


def synth_args(params, tv="1"):
    """One argument spelling per param (first candidate), or None."""
    out = []
    for prm in params:
        c = synth_ty(prm["type"], tv=tv)
        if not c:
            return None
        out.append((c[0], prm["type"].get("kind")))
    return out


def programs(f, args, tv="Int"):
    """Candidate probe shapes. `var`-hoisted receivers (a `let` receiver cannot
    be passed to an in-place mutator, which alone accounted for most of the old
    UNPROBEABLE set), the mixed shape (scalar literals inline — an all-hoisted
    scalar `var` binding hits a v1 subset wall the call itself never does), and
    both statement positions. The verdict must describe the FUNCTION, not the
    wrapper, so every shape gets a turn (see classify)."""
    # The fn's own module and every module an argument spelling reaches (a
    # constructor-built argument, `secret.new(1)`), each imported when the
    # auto-import surface does not already reach it.
    reached = [f["module"]] + [m for a, _ in args for m in MODULE_REF_RX.findall(a)]
    imp = "".join(f"import {m}\n" for m in dict.fromkeys(reached) if needs_import(m))
    imp = imp + "\n" if imp else ""
    # A `@memory_unsafe` fn is outside the memory-safety guarantee; a user file
    # calls it only after the `import ffi` opt-in (E100, #4043).
    if (f["module"], f["name"]) in memory_unsafe_fns() and f["module"] != "ffi":
        imp = "import ffi\n\n" + imp
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
        else:
            annot = render_ty(f.get("ret_json", {}), tv)
            if annot:
                bodies.append(f"  let _: {annot} = {call}{bang}")
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
    """The verdict over the type-variable instantiations in turn: the first
    one whose probe TYPES decides (Int first, so a fn that checks at Int is
    probed exactly as before)."""
    generic = has_type_var(f["params"]) or has_type_var(f.get("ret_json", {}))
    insts = TYPE_VAR_INSTANTIATIONS if generic else TYPE_VAR_INSTANTIATIONS[:1]
    verdict = None
    for lit, ty in insts:
        args = synth_args(f["params"], lit)
        if args is None:
            verdict = verdict or ("UNPROBEABLE", "parameter type outside the synthesizer")
            continue
        verdict, checked = classify_at(f, args, ty)
        if checked:
            return verdict
    return verdict


def classify_at(f, args, tv):
    """(verdict, whether any shape type-checked) at one instantiation."""
    first_gap = None
    any_native = False
    any_checked = False
    for src in programs(f, args, tv):
        tf = tempfile.NamedTemporaryFile("w", suffix=".almd", delete=False, dir="/tmp")
        tf.write(src); tf.close()
        try:
            # The CONTROL: this shape has to type and build natively, or it says
            # nothing about the function and the next shape gets a turn.
            if run([ALMIDE, "check", tf.name])[0] != 0:
                continue
            any_checked = True
            if run([ALMIDE, "build", tf.name, "-o", tf.name + ".bin"])[0] != 0:
                continue
            any_native = True
            rc, out = run([ALMIDE, "build", tf.name, "--target", "wasm", "-o", tf.name + ".wasm"])
            if rc == 0:
                return ("PARITY", ""), True
            if first_gap is None:
                m = re.search(r"wall: (.+)", out)
                first_gap = (m.group(1)[:160] if m else
                             "wasm renderer wall" if WALL_RX.search(out) else "wasm build failed")
        finally:
            for ext in ("", ".bin", ".wasm"):
                try: os.unlink(tf.name + ext)
                except OSError: pass
    if any_native:
        return ("GAP", first_gap or "wasm build failed"), True
    return ("UNPROBEABLE", "no candidate shape builds natively"), any_checked


def ci_slice(fns):
    """The CI slice (scripts/lib/ci-slice.sh): under ALMIDE_CI_SLICE=k > 1 keep
    the commit-rotated 1-in-k fns plus every fn of a module file the change
    touches. Key: `stdlib/<module>.almd#<fn>`, so a change to
    stdlib/<module>.almd touches each of its fns. Full run: every fn."""
    if int(os.environ.get("ALMIDE_CI_SLICE", "1") or "1") <= 1:
        return fns
    keys = [f"stdlib/{f['module']}.almd#{f['name']}" for f in fns]
    rc = subprocess.run(["bash", "-c", ". scripts/lib/ci-slice.sh && ci_slice_filter wasm-reachability"],
                        input="".join(k + "\n" for k in keys), capture_output=True, text=True, cwd=REPO)
    sys.stderr.write(rc.stderr)
    if rc.returncode != 0:
        sys.exit("wasm-reachability: the CI slice selection failed")
    keep = set(rc.stdout.split())
    return [f for f, k in zip(fns, keys) if k in keep]


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--modules", nargs="*", help="limit to these modules")
    ap.add_argument("--json", help="write the raw result rows here")
    a = ap.parse_args()
    every = surface()
    # A constructor may live in a bundled module outside the probed surface
    # (`html.escape` builds the `SafeHtml` that `http.html` takes): index those
    # too, read from the same declaration the resolver reads (BUNDLED_MODULES).
    bundled = [m for m in rust_str_list("BUNDLED_MODULES") if m not in stdlib_modules()]
    index_constructors(every + surface(modules=bundled))
    wanted = set(a.modules) if a.modules else None
    fns = ci_slice([f for f in every if wanted is None or f["module"] in wanted])
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
