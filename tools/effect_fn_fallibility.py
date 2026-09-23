#!/usr/bin/env python3
"""ADR-0002 Falsifier-2 measurement: classify every `effect fn` as fallible / total / unclassifiable.

WHY THIS EXISTS
---------------
ADR-0002 Phase 3 redefines `effect fn f() -> T` as TOTAL (no Result lift). Its Falsifier 2
says the flip collapses if "many effect fns cannot be statically classified as fallible /
total". This script is that measurement, re-runnable (#2556): it walks corpora of `.almd`
files, asks the compiler for the AST (`almide <file> --emit-ast`), and classifies every
top-level `effect fn` (the language has no nested fns and no impl blocks, so top-level
decls are the whole population).

TWO CLASSIFIERS, REPORTED SIDE BY SIDE
--------------------------------------
`source` -- the issue's table, read off the source, in this precedence:
    unclassifiable  intrinsic body (`= _` hole, or `@intrinsic` with no body):
                    fallibility lives in the runtime, not in the source
    fallible        the return type is an explicit `-> T!` / `-> Result[..]` (the
                    explicit Result form is the bundled-stdlib spelling of the same
                    declared fallibility; it is counted under `fallible` and shown
                    separately as `declared -> Result`), or the body contains a
                    propagation `!` (any subject) or an `err(...)`
    total           none of the above

`codegen` -- a mirror of the never-err classifier codegen already runs
(`compute_can_err` / `has_result_err` / `unwrap_named_callees` in
crates/almide-mir/src/lower/mod_p2.rs, consumed by `strip_never_err_unwraps` and
`unwrap_never_err_call_types`). Same `unclassifiable` set; a fn is `fallible` when
    - its return type is an explicit `-> T!` / `-> Result[..]`
      (codegen keeps those OUT of the lifted set -- they are declared fallible)
    - its body contains `err(...)`                                    (ResultErr)
    - its body contains `!` over anything that is NOT a bare-name call
      (`fs.read_text(p)!`, `r!` on a local, a field, an Option `!`)   (the non-Named rule)
    - it calls `prim.read_text_file` / `prim.read_bytes_file`          (the prim rule)
    - FIXPOINT: it `!`-propagates a bare-name call to a fn (same file) that is fallible
      (`unwrap_named_callees` + the `compute_can_err` loop). A bare-name callee the file
      does not define (an imported name, a fn-typed parameter) is conservatively fallible.
and `total` otherwise. The one rule NOT mirrored is `returns_foreign_result` (a tail
call to a module fn whose TYPE is Result): the AST carries no types, so a tail
`fs.read_text(p)` without `!` is invisible here (it is E042 at check time anyway,
so it does not occur in a checked corpus).

The difference between the two columns is exactly "`!` spelled over a callee that never
errs" -- e.g. the fern walker's self-call `walk(..)!`: `source` says fallible (the `!` is
there), `codegen` says total (the callee is never-err). That delta is the count of effect
fns whose `!` carries no information today, which is what Phase 3 removes.

THE UNCLASSIFIABLE SET, REFINED
-------------------------------
An intrinsic is opaque to the BODY rules, but two static facts still classify most of them:
    declared        the hole's own signature says `-> Result[..]` / `-> T!`
    self-host       the hole's `module.fn` has a pure-Almide body registered in
                    crates/almide-types/src/self_host_registry.rs (`SRC_<STEM>` is
                    `stdlib/<stem>.almd`); the impl body is classified with the
                    `source` rules and the verdict is inherited
    runtime-opaque  neither: a `-> T` intrinsic with no Almide body. THIS is the
                    Falsifier residue -- the fns that need a per-intrinsic declaration.
Precedence is declared > self-host > runtime-opaque.

USAGE
-----
    python3 tools/effect_fn_fallibility.py [--almide BIN] [--json] [--list CLASS]
        NAME=DIR [NAME=DIR ...]
    e.g.  python3 tools/effect_fn_fallibility.py stdlib=stdlib spec=spec \
              playground=../playground/web/examples

A file that fails `--emit-ast` with "no almide.toml" (a playground example using
`import self.x`) is retried inside a temporary package mirroring the playground's own
export layout (`almide.toml` + `src/*.almd`). Other failures (deliberately unparseable
negative fixtures, packages with unfetched deps) are reported per file, with the number
of `effect fn` headers grep finds in each so the uncounted population is visible.

`--list unclassifiable` prints every intrinsic effect fn as `module.fn` with its
refinement. `--list fallible|total` lists the fns of that class under the `source`
classifier, with the `codegen` verdict beside each. Exit 0 always; the numbers are the
product, not a gate.
"""
import argparse, collections, json, os, re, shutil, subprocess, sys, tempfile

CLASSES = ("fallible", "total", "unclassifiable")
HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.dirname(HERE)
REGISTRY = os.path.join(REPO, "crates", "almide-types", "src", "self_host_registry.rs")
STDLIB = os.path.join(REPO, "stdlib")


def run_emit_ast(almide, path):
    p = subprocess.run([almide, path, "--emit-ast"], capture_output=True, text=True)
    if p.returncode != 0 or not p.stdout.strip():
        return None, (p.stderr.strip().splitlines() or ["(no stderr)"])[0]
    try:
        return json.loads(p.stdout), None
    except json.JSONDecodeError as e:
        return None, f"emit-ast JSON: {e}"


def emit_ast(almide, path):
    ast, err = run_emit_ast(almide, path)
    if ast is not None or "no almide.toml" not in (err or ""):
        return ast, err
    # Mirror the playground's export: almide.toml + src/<the example's files>.
    d = os.path.dirname(os.path.abspath(path))
    with tempfile.TemporaryDirectory() as tmp:
        with open(os.path.join(tmp, "almide.toml"), "w") as f:
            f.write('[package]\nname = "playground_export"\nversion = "0.1.0"\n')
        src = os.path.join(tmp, "src")
        os.mkdir(src)
        for name in os.listdir(d):
            if name.endswith(".almd"):
                shutil.copy(os.path.join(d, name), src)
        return run_emit_ast(almide, os.path.join(src, os.path.basename(path)))


def walk(node):
    """Yield every dict node with a `kind` in a body, depth-first."""
    if isinstance(node, dict):
        if "kind" in node:
            yield node
        for v in node.values():
            yield from walk(v)
    elif isinstance(node, list):
        for v in node:
            yield from walk(v)


def is_intrinsic(fn):
    if fn.get("body") is None:
        return True
    return fn["body"].get("kind") == "hole"


def declared_fallible(fn):
    rt = fn.get("returnType") or {}
    return rt.get("kind") == "generic" and rt.get("name") in ("!", "Result")


def named_callee(call):
    """The bare name if `call` is `f(..)` with an ident callee (codegen's CallTarget::Named)."""
    c = call.get("callee") or {}
    if call.get("kind") == "call" and c.get("kind") == "ident":
        return c.get("name")
    return None


def module_callee(call):
    c = call.get("callee") or {}
    if call.get("kind") == "call" and c.get("kind") == "member":
        obj = c.get("object") or {}
        if obj.get("kind") == "ident":
            return obj.get("name"), c.get("field")
    return None


def source_class(fn):
    if is_intrinsic(fn):
        return "unclassifiable"
    if declared_fallible(fn):
        return "fallible"
    for n in walk(fn["body"]):
        if n["kind"] in ("unwrap", "err"):
            return "fallible"
    return "total"


def codegen_seed(fn):
    """has_result_err mirror (minus the type-based returns_foreign_result); returns
    (seed_fallible: bool, named_unwrap_callees: set[str])."""
    if declared_fallible(fn):
        return True, set()
    seed = False
    callees = set()
    for n in walk(fn["body"]):
        k = n["kind"]
        if k == "err":
            seed = True
        elif k == "unwrap":
            name = named_callee(n.get("expr") or {})
            if name is None:
                seed = True
            else:
                callees.add(name)
        elif k == "call":
            mc = module_callee(n)
            if mc and mc[0] == "prim" and mc[1] in ("read_text_file", "read_bytes_file"):
                seed = True
    return seed, callees


def classify_file(fns):
    """Per-file codegen fixpoint over ALL fns in the file (pure fns take part in the
    closure exactly as in compute_can_err, which runs over the whole program)."""
    defined = {f["name"] for f in fns}
    seeds, callees = {}, {}
    for f in fns:
        if is_intrinsic(f):
            continue
        s, cs = codegen_seed(f)
        seeds[f["name"]] = s
        callees[f["name"]] = cs
    can_err = {n for n, s in seeds.items() if s}
    changed = True
    while changed:
        changed = False
        for n, cs in callees.items():
            if n in can_err:
                continue
            if any((g not in defined) or (g in can_err) for g in cs):
                can_err.add(n)
                changed = True
    return can_err


REG_ENTRY = re.compile(r"SRC_([A-Z0-9_]+)\s*,\s*&\[(.*?)\]\s*\)", re.S)
REG_PAIR = re.compile(r'\(\s*"([^"]+)"\s*,\s*"([^"]+)"\s*\)')


def load_registry(path):
    """{ 'module.fn': (stem, impl_fn) } from self_host_registry.rs."""
    out = {}
    try:
        text = open(path).read()
    except OSError:
        return out
    for m in REG_ENTRY.finditer(text):
        stem = m.group(1).lower()
        for impl_fn, call in REG_PAIR.findall(m.group(2)):
            out[call] = (stem, impl_fn)
    return out


class SelfHost:
    """Classifies a registered intrinsic by its self-host impl body (parsed on demand)."""

    def __init__(self, almide, registry, stdlib):
        self.almide, self.registry, self.stdlib = almide, registry, stdlib
        self.cache = {}

    def impl_class(self, call):
        hit = self.registry.get(call)
        if not hit:
            return None
        stem, impl_fn = hit
        if stem not in self.cache:
            ast, _ = run_emit_ast(self.almide, os.path.join(self.stdlib, stem + ".almd"))
            fns = [x for x in (ast or {}).get("decls", []) if x.get("kind") == "fn"]
            self.cache[stem] = {f["name"]: f for f in fns}
        f = self.cache[stem].get(impl_fn)
        if f is None:
            return None
        return source_class(f)


def refine_intrinsic(fn, call, selfhost):
    if declared_fallible(fn):
        return "declared", "fallible"
    c = selfhost.impl_class(call)
    if c in ("fallible", "total"):
        return "self-host", c
    return "runtime-opaque", None


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--almide", default=os.environ.get("ALMIDE_BIN", "almide"))
    ap.add_argument("--registry", default=REGISTRY, help="self_host_registry.rs to refine intrinsics with")
    ap.add_argument("--stdlib", default=STDLIB, help="stdlib dir holding the self-host bodies")
    ap.add_argument("--json", action="store_true", help="machine-readable output")
    ap.add_argument("--list", choices=CLASSES, help="list the fns of one class (source classifier)")
    ap.add_argument("corpora", nargs="+", metavar="NAME=DIR")
    args = ap.parse_args()

    corpora = []
    for c in args.corpora:
        if "=" in c:
            name, d = c.split("=", 1)
        else:
            name, d = os.path.basename(os.path.normpath(c)), c
        corpora.append((name, d))
    selfhost = SelfHost(args.almide, load_registry(args.registry), args.stdlib)

    report = {}
    listing = []
    for name, d in corpora:
        files = []
        for root, _, fs in os.walk(d):
            files += [os.path.join(root, f) for f in fs if f.endswith(".almd")]
        files.sort()
        counts = {"source": collections.Counter(), "codegen": collections.Counter()}
        refined = collections.Counter()
        declared_result = 0
        spelled_only = []      # source=fallible, codegen=total
        unclassifiable = {}    # module.fn -> (refinement, inherited class)
        parse_failures = []
        n_fns = 0
        for path in files:
            ast, err = emit_ast(args.almide, path)
            if ast is None:
                with open(path, errors="replace") as fh:
                    missed = sum(1 for line in fh if re.match(r"\s*(pub\s+)?effect fn\b", line))
                parse_failures.append((path, err, missed))
                continue
            fns = [x for x in ast.get("decls", []) if x.get("kind") == "fn"]
            effect = [f for f in fns if f.get("effect")]
            if not effect:
                continue
            modname = ast.get("module") or os.path.splitext(os.path.basename(path))[0]
            can_err = classify_file(fns)
            for f in effect:
                n_fns += 1
                sc = source_class(f)
                cg = "unclassifiable" if sc == "unclassifiable" else (
                    "fallible" if f["name"] in can_err else "total")
                counts["source"][sc] += 1
                counts["codegen"][cg] += 1
                rt = f.get("returnType") or {}
                if rt.get("kind") == "generic" and rt.get("name") == "Result":
                    declared_result += 1
                if sc == "fallible" and cg == "total":
                    spelled_only.append(f"{path}:{f['name']}")
                if sc == "unclassifiable":
                    call = f"{modname}.{f['name']}"
                    how, inherited = refine_intrinsic(f, call, selfhost)
                    refined[how if inherited is None else f"{how}:{inherited}"] += 1
                    unclassifiable[call] = (how, inherited)
                if args.list == sc:
                    listing.append((name, path, f["name"], sc, cg,
                                    unclassifiable.get(f"{modname}.{f['name']}")))
        report[name] = {
            "dir": d,
            "files": len(files),
            "effect_fns": n_fns,
            "source": {k: counts["source"][k] for k in CLASSES},
            "codegen": {k: counts["codegen"][k] for k in CLASSES},
            "declared_result": declared_result,
            "spelled_only": spelled_only,
            "unclassifiable": {k: {"refinement": v[0], "class": v[1]}
                               for k, v in sorted(unclassifiable.items())},
            "unclassifiable_refined": dict(refined),
            "parse_failures": [{"path": p, "error": e, "effect_fn_headers": m}
                               for p, e, m in parse_failures],
        }

    if args.json:
        print(json.dumps(report, indent=2))
        return
    if args.list:
        for name, path, fn, sc, cg, ref in listing:
            extra = f"\t{ref[0]}" + (f":{ref[1]}" if ref[1] else "") if ref else ""
            print(f"{name}\t{path}\t{fn}\tsource={sc}\tcodegen={cg}{extra}")
        return

    pad = " " * 23
    print(f"{'corpus':<12}{'effect fns':>11}  {'classifier':<9}{'fallible':>9}{'total':>7}{'unclassifiable':>16}")
    for name, r in report.items():
        for cls in ("source", "codegen"):
            c = r[cls]
            head = f"{name:<12}{r['effect_fns']:>11}" if cls == "source" else pad
            print(f"{head}  {cls:<9}{c['fallible']:>9}{c['total']:>7}{c['unclassifiable']:>16}")
        print(f"{pad}  declared -> Result[..]: {r['declared_result']}; "
              f"`!` spelled over a never-err callee (source=fallible, codegen=total): {len(r['spelled_only'])}")
        if r["unclassifiable"]:
            rf = r["unclassifiable_refined"]
            print(f"{pad}  unclassifiable refined: " + ", ".join(f"{k}={v}" for k, v in sorted(rf.items())))
        if r["parse_failures"]:
            missed = sum(p["effect_fn_headers"] for p in r["parse_failures"])
            print(f"{pad}  emit-ast failed on {len(r['parse_failures'])} file(s) "
                  f"holding {missed} `effect fn` header(s) NOT counted above:")
            for p in r["parse_failures"]:
                print(f"{pad}    {p['path']} [{p['effect_fn_headers']}]: {p['error']}")
    tot = collections.Counter()
    for r in report.values():
        for k in CLASSES:
            tot[k] += r["source"][k]
    print(f"{'ALL (source)':<23}  {'':<9}{tot['fallible']:>9}{tot['total']:>7}{tot['unclassifiable']:>16}")
    uncls = {}
    for r in report.values():
        uncls.update(r["unclassifiable"])
    opaque = [k for k, v in uncls.items() if v["refinement"] == "runtime-opaque"]
    print(f"\nunclassifiable intrinsics: {len(uncls)} distinct module.fn, of which "
          f"runtime-opaque (`-> T`, no self-host body): {len(opaque)}")
    for k in sorted(uncls):
        v = uncls[k]
        tag = v["refinement"] + (f" -> {v['class']}" if v["class"] else "")
        print(f"  {k:<40} {tag}")


if __name__ == "__main__":
    main()
