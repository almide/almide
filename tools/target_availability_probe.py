#!/usr/bin/env python3
"""#1423 stage 1 / #1827 — measure the single-leg stdlib surface WITH THE
RENDERER'S EYES, over the WHOLE public surface: for every public stdlib fn
(the compiler's own module interface, `almide compile <mod> --json` — the
source the docs/stdlib signature indexes are generated from), synthesize a
minimal well-typed call and attempt the leg's build (or run). Record
ok | wall | unprobed | error.

Name-diffs over self_host_registry.rs over-report (~199 false rows — linkage
is multi-mechanism); this probe cannot: it asks the one authority, the
renderer itself.

Enumeration (#1827): the `### \\`mod.fn(...)\\`` prose headings of
docs/stdlib/*.md covered 391 of the ~970 public fns — everything without its
own heading (the bytes width family, matrix, the sized-numeric modules,
`string.split_once`, `result.filter`, …) was never probed, never compared,
and the "declared == measured" agreement was over 43 % of the surface. The
interface JSON is complete by construction (it IS the surface), carries the
tuple element types the markdown index flattens to `()`, and flags effect
and deprecated fns. Every fn of every registered module is enumerated; a fn
the synthesizer cannot build a probe program for is an ERROR line and a
non-zero exit — never a silent skip.

The sweep is per-LEG (--leg, #1710 increment 2's vocabulary):
  structural   ALMIDE_WASM_STRUCTURAL=1 build — the emitter frontier.
  stock-p1     default `build --target wasm` (reroute included) — walls
               mean no build path serves the fn (the E081 set).
               (--default-routing is the legacy alias.)
  embedded     `run --target wasm` through the embedded host — service
               measured by EXECUTION: a run that reaches the host and is
               answered (even with a runtime err like a missing file) is
               service; a build wall or an `unknown ... op` host reply is
               not.

Output (stdout): one line per fn — `status<TAB>module.fn<TAB>detail`.
  ok        lowered and emitted (embedded: executed and answered)
  wall      the leg refused (detail = the leg's own first error line)
  unprobed  the leg walled on an ARGUMENT CONSTRUCTOR the probe injected
            (detail names it), so the fn under probe was never reached —
            the honesty bucket for values that only exist through another
            fn (an HttpRequest inside an http.serve handler, a SafeHtml
            from html.empty). Claimed neither way; the gate holds it under
            a per-leg ceiling so it can never grow silently.
  error     the probe could not synthesize a well-typed minimal call
            (detail = why). Fails the probe (exit 1) and the gate: the
            synthesizer is the thing to fix, never the enumeration.
"""
import json
import os
import re
import subprocess
import sys
import tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
ALMIDE = os.environ.get("ALMIDE", os.path.join(ROOT, "target/release/almide"))
DOCDIR = os.path.join(ROOT, "docs/stdlib")
REGISTRY = os.path.join(ROOT, "crates/almide-types/src/stdlib_info.rs")
LEGS = ("structural", "stock-p1", "embedded")

# Modules that need an explicit import (the rest are auto-imported).
EXPLICIT_IMPORT = {
    "json", "fs", "http", "env", "io", "random", "regex", "process",
    "testing", "url", "args", "base64", "compute", "hash", "hex",
    "path", "html", "duration", "mem", "net", "zlib",
}
# docs/stdlib pages that document CHECKER surface (clock constructors), not
# a module interface — tools/gen-stdlib-doc-index.py's own exclusion.
DOC_EXCLUDED = {"compute", "duration"}
# Registered modules with no public page by design: the v1 primitive floor.
INTERNAL_MODULES = {"prim"}

SIZED_INTS = {"Int8", "Int16", "Int32", "Int64", "UInt8", "UInt16", "UInt32", "UInt64"}
SIZED_FLOATS = {"Float32", "Float64"}
# Error-slot type variables resolve to String so `!` propagation types;
# every other type variable resolves to Int.
STRING_TVARS = {"E", "F"}

# Cells the interface JSON renders as `{"kind": "unknown"}` — types its
# emitter does not model (the raw-pointer FFI surface, the f32 matrix, the
# Never return). Keyed "mod.fn" -> {param name | "return": type node}. A NEW
# unknown cell outside this table is an ERROR line, never a guess.
RAWPTR = {"kind": "named", "name": "RawPtr"}
MAT_F32 = {"kind": "named", "name": "Matrix", "args": [{"kind": "named", "name": "Float32"}]}
NEVER = {"kind": "never"}
UNMODELED = {
    "bytes.as_mut_ptr": {"return": RAWPTR},
    "bytes.as_ptr": {"return": RAWPTR},
    "bytes.copy_to_ptr": {"ptr": RAWPTR},
    "bytes.from_raw_ptr": {"ptr": RAWPTR},
    "matrix.zeros_f32": {"return": MAT_F32},
    "matrix.ones_f32": {"return": MAT_F32},
    "matrix.mul_f32": {"a": MAT_F32, "b": MAT_F32, "return": MAT_F32},
    "matrix.mul_f32_scaled": {"a": MAT_F32, "b": MAT_F32, "return": MAT_F32},
    "matrix.mul_f32_t": {"a": MAT_F32, "b": MAT_F32, "return": MAT_F32},
    "matrix.mul_f32_t_scaled": {"a": MAT_F32, "b": MAT_F32, "return": MAT_F32},
    "process.exit": {"return": NEVER},
}

# Nominal runtime types with no `types` entry in the interface JSON: the
# constructor expression, the stdlib fn it calls (recorded as an INJECTED
# constructor — a wall naming it is `unprobed`, not the probed fn's wall),
# and the module to import.
NOMINAL = {
    "Value": ("value.int(0)", "value.int", None),
    "JsonPath": ("json.root()", "json.root", "json"),
    "SafeHtml": ("html.empty()", "html.empty", "html"),
    "SafePath": ('path.trusted("x")', "path.trusted", "path"),
    "HttpResponse": ('http.response(200, "x")', "http.response", "http"),
}
BYTES8 = "bytes.from_list([0, 0, 0, 0, 0, 0, 0, 0])"


class Unsynth(Exception):
    """The synthesizer has no expression for this type."""


class Ctx:
    """Per-program synthesis state: imports, hoisted typed leaves, the
    stdlib constructors the probe itself injected, and whether the call
    needs an HttpRequest (reachable only inside an http.serve handler)."""

    def __init__(self):
        self.imports = set()
        self.hoists = []
        self.ctors = set()
        self.needs_req = False
        self.n = 0

    def hoist(self, ty: str, expr: str) -> str:
        name = f"h{self.n}"
        self.n += 1
        self.hoists.append(f"let {name}: {ty} = {expr}")
        return name


def tvar(name: str) -> dict:
    return {"kind": "string"} if name in STRING_TVARS else {"kind": "int"}


def render(t: dict) -> str:
    """Almide type syntax for a type node, type variables resolved."""
    k = t.get("kind")
    if k == "int": return "Int"
    if k == "float": return "Float"
    if k == "string": return "String"
    if k == "bool": return "Bool"
    if k == "unit": return "Unit"
    if k == "bytes": return "Bytes"
    if k == "matrix": return "Matrix"
    if k == "value": return "Value"
    if k == "type_var": return render(tvar(t["name"]))
    if k == "list": return f"List[{render(t['inner'])}]"
    if k == "option": return f"Option[{render(t['inner'])}]"
    if k == "set": return f"Set[{render(t['inner'])}]"
    if k == "result": return f"Result[{render(t['ok'])}, {render(t['err'])}]"
    if k == "map": return f"Map[{render(t['key'])}, {render(t['value'])}]"
    if k == "tuple": return "(" + ", ".join(render(e) for e in t["elements"]) + ")"
    if k == "fn":
        return "(" + ", ".join(render(p) for p in t.get("params", [])) + f") -> {render(t['return'])}"
    if k == "named":
        args = t.get("args", [])
        return t["name"] + (f"[{', '.join(render(a) for a in args)}]" if args else "")
    raise Unsynth(f"no type syntax for kind={k}")


def has_tvar(t: dict) -> bool:
    if t.get("kind") == "type_var":
        return True
    for key in ("inner", "ok", "err", "key", "value", "return"):
        if key in t and has_tvar(t[key]):
            return True
    return any(has_tvar(x) for x in t.get("params", []) + t.get("elements", []) + t.get("args", []))


def dummy(t: dict, ctx: Ctx, types: dict) -> str:
    """A minimal well-typed expression for a type node, or raise Unsynth."""
    k = t.get("kind")
    if k == "int": return "0"
    if k == "float": return "0.0"
    if k == "string": return '"x"'
    if k == "bool": return "true"
    if k == "unit": return "()"
    if k == "value": return NOMINAL["Value"][0]
    if k == "bytes":
        ctx.ctors.add("bytes.from_list")
        return BYTES8
    if k == "matrix":
        ctx.ctors.add("matrix.zeros")
        return "matrix.zeros(1, 1)"
    if k == "type_var":
        return dummy(tvar(t["name"]), ctx, types)
    if k == "list":
        return f"[{dummy(t['inner'], ctx, types)}]"
    if k == "option":
        return f"some({dummy(t['inner'], ctx, types)})"
    if k == "set":
        ctx.ctors.add("set.from_list")
        return f"set.from_list([{dummy(t['inner'], ctx, types)}])"
    if k == "result":
        # Hoisted with its full annotation so the error slot is pinned
        # (a bare `ok(0)` leaves E free and the call ill-typed).
        return ctx.hoist(render(t), f"ok({dummy(t['ok'], ctx, types)})")
    if k == "map":
        return f"[{dummy(t['key'], ctx, types)}: {dummy(t['value'], ctx, types)}]"
    if k == "tuple":
        return "(" + ", ".join(dummy(e, ctx, types) for e in t["elements"]) + ")"
    if k == "fn":
        names = [f"_p{i}" for i in range(len(t.get("params", [])))]
        return f"({', '.join(names)}) => {dummy(t['return'], ctx, types)}"
    if k == "named":
        name = t["name"]
        if name in SIZED_INTS:
            return ctx.hoist(name, "0")
        if name in SIZED_FLOATS:
            return ctx.hoist(name, "0.0")
        if name == "Matrix" and [render(a) for a in t.get("args", [])] == ["Float32"]:
            ctx.ctors.add("matrix.zeros_f32")
            return "matrix.zeros_f32(1, 1)"
        if name == "RawPtr":
            ctx.ctors.add("bytes.as_ptr")
            return f"bytes.as_ptr({dummy({'kind': 'bytes'}, ctx, types)})"
        if name == "HttpRequest":
            # No constructor exists: the value lives only inside an
            # http.serve handler, so the probe body is wrapped in one.
            ctx.needs_req = True
            ctx.imports.add("http")
            ctx.ctors.update({"http.serve", "http.response"})
            return "req"
        if name in NOMINAL:
            expr, ctor, imp = NOMINAL[name]
            ctx.ctors.add(ctor)
            if imp:
                ctx.imports.add(imp)
            return expr
        if name in types:
            mod, kind = types[name]
            if mod in EXPLICIT_IMPORT:
                ctx.imports.add(mod)
            if kind.get("kind") == "record":
                fields = ", ".join(
                    f"{f['name']}: {dummy(f['type'], ctx, types)}" for f in kind["fields"]
                )
                return f"{name} {{ {fields} }}"
            if kind.get("kind") == "variant":
                for case in kind.get("cases", []):
                    if not case.get("fields") and not case.get("payload") and not case.get("args"):
                        return case["name"]
                raise Unsynth(f"variant {name} has no payload-less case")
            raise Unsynth(f"type {name} of kind {kind.get('kind')}")
        raise Unsynth(f"nominal type {name} has no known constructor")
    raise Unsynth(f"type kind={k}")


def parse_registry():
    """The module names the compiler registers (STDLIB_MODULES ∪ BUNDLED_MODULES)."""
    src = open(REGISTRY).read()
    names = set()
    for const in ("STDLIB_MODULES", "BUNDLED_MODULES"):
        m = re.search(rf"pub const {const}: &\[&str\] = &\[(.*?)\];", src, re.S)
        if not m:
            raise SystemExit(f"::error::{REGISTRY}: cannot find {const}")
        body = re.sub(r"//[^\n]*", "", m.group(1))
        names.update(re.findall(r'"([a-z0-9_]+)"', body))
    return names - INTERNAL_MODULES


def enumerate_surface():
    """Every public fn of every module: [(mod, fn-record)], plus the
    cross-module nominal type table {name: (module, kind)}."""
    docs = {n[:-3] for n in os.listdir(DOCDIR) if n.endswith(".md")} - DOC_EXCLUDED
    registry = parse_registry()
    missing = sorted(registry - docs)
    if missing:
        # A registered module without a page would otherwise vanish from
        # the sweep — the #1827 shape, one level up.
        raise SystemExit(f"::error::registered stdlib modules without a docs/stdlib page: {', '.join(missing)}")
    sigs, types = [], {}
    for mod in sorted(docs):
        r = subprocess.run([ALMIDE, "compile", mod, "--json"], capture_output=True, text=True, cwd=ROOT)
        if r.returncode != 0 or not r.stdout.strip():
            raise SystemExit(f"::error::`almide compile {mod} --json` failed:\n{r.stderr.strip()}")
        iface = json.loads(r.stdout)
        for t in iface.get("types", []):
            types[t["name"]] = (mod, t["kind"])
        for f in iface.get("functions", []):
            # __-prefixed fns are INTERNAL carriers (ADR-0006 D3) — not surface.
            if not f["name"].startswith("__"):
                sigs.append((mod, f))
    return sigs, types


def resolve_unknowns(mod, f):
    """Substitute the UNMODELED table into a fn record; raise on a new cell."""
    key = f"{mod}.{f['name']}"
    table = UNMODELED.get(key, {})
    params = []
    for p in f.get("params", []):
        t = p["type"]
        if t.get("kind") == "unknown":
            if p["name"] not in table:
                raise Unsynth(f"param {p['name']}: interface JSON kind=unknown, not in UNMODELED")
            t = table[p["name"]]
        params.append((p["name"], t))
    ret = f.get("return", {"kind": "unit"})
    if ret.get("kind") == "unknown":
        if "return" not in table:
            raise Unsynth("return: interface JSON kind=unknown, not in UNMODELED")
        ret = table["return"]
    # The interface JSON names `Never` since #1834 (it was kind=unknown,
    # resolved through UNMODELED); a Never return is the statement-position
    # shape (`process.exit(0)!`), never a bound value.
    if ret.get("kind") == "named" and ret.get("name") == "Never":
        ret = NEVER
    return params, ret


def synth(mod, f, params, ret, types, variant, shape=0):
    """A minimal program calling module.fn once, or (None, why).

    variant 0: plain args, discarded bind.
    variant 1: FIRST arg bound as a `var` (mut-param fns, E032).
    variant 2: the bind annotated with the resolved return type (generic-
               empty producers, E018: `map.new`, `set.new`,
               `list.with_capacity`) — only for returns carrying a type
               variable.
    variant 3: EVERY arg hoisted into a `var` and passed by name — the
               reachability sweep's repertoire (a var-bound callback
               lowers where the inline lambda walls: the fs.for_each_line
               lesson, where the weaker shape produced a false
               wasm-unavailable row and E081 blocked a working fn).
    variant 4: hoisted args AND the call as the FINAL statement (no
               trailing print) — the same lesson's second half: the
               postlude itself walls some shapes.

    Tuple-shaped returns are CONSUMED the way real code consumes them,
    through a `shape` ladder of their own (every variant tries each):
    a tuple — `let (a, b) = …`, then the discard; an Option of a tuple
    (the string.split_once shape of #1827) — `match … { some((a, b)) =>
    … }`, then `let (a, b) = … ?? (…)`, then the discard. The ladder
    exists because the incumbent walls the call-bearing match ARMS while
    serving the fn (the `??` and discard forms build and run) — a
    consumption-shape wall must not become the fn's row (the process.exit
    lesson again). Shapes beyond the return's ladder return (None, why).
    """
    ctx = Ctx()
    args = [dummy(t, ctx, types) for _, t in params]
    # A hoisted `var` carries the param's type: an un-annotated rebinding
    # of a Result value is E041 (ADR-0008), and the annotation is what
    # keeps a nested `Result[Result[A, E], E]` argument at its full depth.
    tys = [render(t) for _, t in params]
    prelude = ""
    if variant == 1:
        if not args:
            return None, "no-first-arg"
        prelude = f"  var subj: {tys[0]} = {args[0]}\n"
        args = ["subj"] + args[1:]
    elif variant in (3, 4):
        if not args:
            return None, "no-args"
        prelude = "".join(f"  var a{i}: {tys[i]} = {a}\n" for i, a in enumerate(args))
        args = [f"a{i}" for i in range(len(args))]
    call = f"{mod}.{f['name']}({', '.join(args)})"
    # An effect fn (and a Result-returning one) propagates; Unit- and
    # Never-returning fns sit in STATEMENT position (a bare Unit call is
    # legal and matches real usage); everything else binds discarded. The
    # probe body opens with a print so the call sits MID-BODY — a
    # single-call main measures the renderer's minimal-program shape
    # support, not the fn (the process.exit lesson: the fn was served,
    # the one-statement main was not, and the conflated wall row broke
    # real programs through E081).
    rk = ret.get("kind")
    bang = "!" if (f.get("effect") or rk == "result") else ""
    is_unit = rk in ("unit", "never")
    # What the bind receives: `!` strips one layer — the Result's ok
    # payload, or (an effect fn declared `-> Option[A]`) the Option's
    # inner, a none propagating as the error.
    payload = ret
    if bang and rk == "result":
        payload = ret["ok"]
    elif bang and rk == "option":
        payload = ret["inner"]
    pk = payload.get("kind")
    opt_tuple = pk == "option" and payload["inner"].get("kind") == "tuple"
    shapes = ["default"]
    if variant == 2 or is_unit:
        pass
    elif pk == "tuple":
        shapes = ["destructure", "discard"]
    elif opt_tuple:
        shapes = ["match", "coalesce", "discard"]
    if shape >= len(shapes):
        return None, "no-such-shape"
    how = shapes[shape]
    if variant == 2:
        if not has_tvar(ret):
            return None, "no-generic-ret"
        stmt = f"let _r: {render(payload)} = {call}{bang}"
    elif is_unit:
        stmt = f"{call}{bang}"
    elif how == "destructure":
        names = ", ".join(f"_t{i}" for i in range(len(payload["elements"])))
        stmt = f"let ({names}) = {call}{bang}"
    elif how == "match":
        names = ", ".join(f"_t{i}" for i in range(len(payload["inner"]["elements"])))
        stmt = (f"match {call}{bang} {{\n    some(({names})) => println(\"s\"),\n"
                f"    none => println(\"n\"),\n  }}")
    elif how == "coalesce":
        names = ", ".join(f"_t{i}" for i in range(len(payload["inner"]["elements"])))
        stmt = f"let ({names}) = {call}{bang} ?? {dummy(payload['inner'], ctx, types)}"
    else:
        stmt = f"let _ = {call}{bang}"
    hoists = "".join(f"  {h}\n" for h in ctx.hoists)
    # Every program opens with a print: the call sits mid-body, and on the
    # embedded leg the printed line is the proof the build succeeded and
    # execution began (variant 4 keeps the call FINAL — no trailing print).
    body = f"  println(\"pre\")\n{hoists}{prelude}  {stmt}"
    if variant != 4:
        body = f"{body}\n  println(\"p\")"
    if mod in EXPLICIT_IMPORT:
        ctx.imports.add(mod)
    imp = "".join(f"import {m}\n" for m in sorted(ctx.imports))
    imp = imp + "\n" if imp else ""
    if ctx.needs_req:
        # The HttpRequest lives only inside a handler: the probe body is a
        # handler-side fn, main installs it through http.serve.
        return (
            f"{imp}effect fn __probe(req: HttpRequest) -> Unit = {{\n{body}\n}}\n\n"
            f"effect fn main() -> Unit = {{\n  println(\"pre\")\n"
            f"  http.serve(0, (req) => {{ __probe(req)!\n    ok(http.response(200, \"x\")) }})!\n"
            f"  println(\"p\")\n}}\n"
        ), ctx
    return f"{imp}effect fn main() -> Unit = {{\n{body}\n}}\n", ctx


GENERIC_WALL = "this program shape is not yet supported"


def first_error_line(text: str) -> str:
    """The leg's own verdict line: the first `error`/`wall` line, else the
    first non-empty one (a deprecation WARNING must never pose as the
    wall). The incumbent's generic wall headline carries its reason on
    the NEXT line (`main is outside the MIR-lowering subset: …`) — that
    line is the verdict, so it is joined in."""
    lines = [l for l in text.splitlines() if l.strip()]
    for i, l in enumerate(lines):
        if l.startswith("error") or l.startswith("wall"):
            if GENERIC_WALL in l and i + 1 < len(lines):
                return f"{l} — {lines[i + 1].strip()}"
            return l
    return lines[0] if lines else "?"


def names_injected_ctor(detail: str, ctors) -> str:
    """The injected constructor a wall detail names, if any."""
    for c in sorted(ctors):
        if re.search(rf"(?<![A-Za-z0-9_.]){re.escape(c)}(?![A-Za-z0-9_])", detail):
            return c
    return ""


def run_probe(prog: str, leg: str, tmp: str, env: dict):
    src = os.path.join(tmp, "probe.almd")
    with open(src, "w") as fh:
        fh.write(prog)
    if leg == "embedded":
        return subprocess.run(
            [ALMIDE, "run", src, "--target", "wasm"],
            capture_output=True, text=True, env=env, cwd=tmp,
            stdin=subprocess.DEVNULL, timeout=120,
        )
    return subprocess.run(
        [ALMIDE, "build", src, "--target", "wasm", "-o", os.devnull],
        capture_output=True, text=True, env=env, cwd=tmp,
    )


def measure(mod, f, types, leg, tmp, env):
    """The verdict for one fn on one leg: (status, detail)."""
    try:
        params, ret = resolve_unknowns(mod, f)
    except Unsynth as e:
        return "error", str(e)
    verdict, ctors = None, set()
    for variant, shape in [(v, sh) for v in (0, 1, 2, 3, 4) for sh in (0, 1, 2)]:
        try:
            prog, ctx = synth(mod, f, params, ret, types, variant, shape)
        except Unsynth as e:
            return "error", str(e)
        if prog is None:
            continue
        ctors |= ctx.ctors
        try:
            r = run_probe(prog, leg, tmp, env)
        except subprocess.TimeoutExpired:
            # A hang is not a service verdict either way — record it
            # honestly; the row needs human eyes, not a guess.
            if verdict is None or verdict[0] != "wall":
                verdict = ("wall", "probe-timeout (120s)")
            continue
        combined = r.stderr + r.stdout
        if leg == "embedded" and "unknown" in combined and " op " in combined:
            # The host answered "unknown ... op N": the run REACHED the
            # host and the service is absent — an embedded wall.
            first = next((l for l in combined.splitlines() if "unknown" in l), "?")
            if verdict is None or verdict[0] != "wall":
                verdict = ("wall", first[:200])
            continue
        if r.returncode == 0 or (leg == "embedded" and "pre" in r.stdout.splitlines()):
            # Embedded service includes an answered runtime err (a probe
            # arg like a missing file): the program's opening print on
            # stdout proves the build succeeded and execution began, so
            # the non-zero exit is the host's ANSWER. A refusal before
            # that line (a build wall, the native-only matrix guard) is
            # not service — the text heuristic this replaces took
            # `matrix.qwen3_block_q1_0_kv is native-only` for a runtime
            # err (#1827).
            return "ok", ""
        first = first_error_line(combined)
        # A type/synthesis error is the probe's fault — try the next
        # variant. A wall is only the VERDICT once every variant walled:
        # the ladder exists because verdicts are shape-sensitive (the
        # fs.for_each_line lesson — variant 0 walls, variant 4 builds),
        # so a first-shape wall must not stop it.
        if "error[E0" in first or "Expected" in first or "type error" in first.lower():
            # Never let a later shape's TYPE error demote an earlier
            # shape's renderer wall — verdict precedence is ok > wall > error.
            if verdict is None or verdict[0] != "wall":
                verdict = ("error", f"probe-ill-typed: {first[:100]}")
            continue
        if verdict is None or verdict[0] != "wall":
            verdict = ("wall", first[:200])
    if verdict is None:
        return "error", "no variant produced a program"
    status, detail = verdict
    if status == "wall":
        c = names_injected_ctor(detail, ctors - {f"{mod}.{f['name']}"})
        if c:
            # The leg refused the probe's OWN argument constructor before
            # the fn under probe was reached: not this fn's wall.
            return "unprobed", f"arg-constructor-walls: {c} ({detail[:80]})"
    return status, detail


def main():
    argv = sys.argv[1:]
    leg = "structural"
    if "--default-routing" in argv:
        leg = "stock-p1"
    for i, a in enumerate(argv):
        if a == "--leg":
            leg = argv[i + 1]
    if leg not in LEGS:
        print(f"::error::unknown leg {leg!r}; swept legs are {', '.join(LEGS)}", file=sys.stderr)
        return 2
    if leg == "structural":
        env = dict(os.environ, ALMIDE_WASM_STRUCTURAL="1")
    else:
        env = dict(os.environ)
        env.pop("ALMIDE_WASM_STRUCTURAL", None)
    # Measure the ground truth, not our own declaration (see
    # check_wasm_availability's escape).
    env["ALMIDE_NO_AVAIL_CHECK"] = "1"
    sigs, types = enumerate_surface()
    tmp = tempfile.mkdtemp(prefix="almide-avail-")
    counts = {"ok": 0, "wall": 0, "unprobed": 0, "error": 0}
    for mod, f in sigs:
        status, detail = measure(mod, f, types, leg, tmp, env)
        counts[status] += 1
        print(f"{status}\t{mod}.{f['name']}\t{detail}", flush=True)
        if status == "error":
            print(f"::error::[{leg}] cannot probe {mod}.{f['name']}: {detail}", file=sys.stderr)
    mods = len({m for m, _ in sigs})
    print(
        f"# [{leg}] {len(sigs)} public fns over {mods} modules: "
        + ", ".join(f"{k} {v}" for k, v in counts.items()),
        file=sys.stderr,
    )
    return 1 if counts["error"] else 0


if __name__ == "__main__":
    sys.exit(main())
