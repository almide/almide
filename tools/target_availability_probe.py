#!/usr/bin/env python3
"""#1423 stage 1 — measure the single-leg stdlib surface WITH THE RENDERER'S
EYES: for every public stdlib fn (the docs/stdlib signature indexes, the same
source check-interface-diff.sh trusts), synthesize a minimal well-typed call
and attempt the STRUCTURAL wasm lowering (ALMIDE_WASM_STRUCTURAL=1 forces the
leg and turns the reroute into a hard error). Record ok | wall | unsynth.

Name-diffs over self_host_registry.rs over-report (~199 false rows — linkage
is multi-mechanism); this probe cannot: it asks the one authority, the
renderer itself.

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
  ok        lowered and emitted
  wall      the structural leg refused (detail = first error line)
  unsynth   the probe could not synthesize a well-typed minimal call
            (detail = the unhandled type) — an honesty bucket, not a wall
"""
import os
import re
import subprocess
import sys
import tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
ALMIDE = os.environ.get("ALMIDE", os.path.join(ROOT, "target/release/almide"))

SIG_RX = re.compile(r"^### `([a-z0-9_]+)\.([a-z0-9_]+)\((.*)\)(?: -> (.+?))?`\s*$")

# Modules that need an explicit import (the rest are auto-imported).
EXPLICIT_IMPORT = {
    "json", "fs", "http", "env", "io", "random", "regex", "process",
    "testing", "url", "args", "base64", "compute", "hash", "hex",
    "path", "html", "duration",
}


def dummy(ty: str):
    """A minimal well-typed expression for a param type, or None."""
    t = ty.strip()
    # strip trailing default annotations like `= 0` if any
    t = re.sub(r"\s*=.*$", "", t)
    if t in ("Int", "I64"): return "0"
    if t in ("U8", "U16", "U32", "I8", "I16", "I32", "U64"): return f"{t.lower()}(0)" if False else "0"
    if t == "Float": return "0.0"
    if t == "Bool": return "true"
    if t == "String": return '"x"'
    if t == "Bytes": return 'bytes.from_list([0])'
    if t == "Unit": return "()"
    m = re.match(r"^List\[(.+)\]$", t)
    if m:
        inner = dummy(m.group(1))
        return f"[{inner}]" if inner is not None else None
    m = re.match(r"^Option\[(.+)\]$", t)
    if m:
        inner = dummy(m.group(1))
        return f"some({inner})" if inner is not None else None
    m = re.match(r"^Result\[(.+),\s*(.+)\]$", t)
    if m:
        inner = dummy(m.group(1))
        return f"ok({inner})" if inner is not None else None
    m = re.match(r"^\((.+)\)$", t)  # tuple
    if m:
        parts = split_top(m.group(1))
        ds = [dummy(p) for p in parts]
        if all(d is not None for d in ds):
            return "(" + ", ".join(ds) + ")"
        return None
    m = re.match(r"^Map\[(.+)\]$", t)
    if m:
        parts = split_top(m.group(1))
        if len(parts) == 2:
            k, v = dummy(parts[0]), dummy(parts[1])
            if k is not None and v is not None:
                return f"[{k}: {v}]"
        return None
    m = re.match(r"^Set\[(.+)\]$", t)
    if m:
        inner = dummy(m.group(1))
        return f"set.from_list([{inner}])" if inner is not None else None
    if t == "Value":
        return "value.int(0)"
    m = re.match(r"^fn\((.*)\)\s*->\s*(.+)$", t) or re.match(r"^Fn\[(.*)\]\s*->\s*(.+)$", t)
    if m is None:
        # bare arrow form: `(A, String) -> A`
        m = re.match(r"^\((.*)\)\s*->\s*(.+)$", t)
        if m and "->" in m.group(1):
            m = None
    if m:
        params = [p for p in split_top(m.group(1)) if p.strip()]
        ret = dummy(m.group(2))
        if ret is None:
            return None
        names = [f"_p{i}" for i in range(len(params))]
        return f"({', '.join(names)}) => {ret}"
    # single generic letters resolve to Int
    if re.fullmatch(r"[A-Z]", t):
        return "0"
    return None


def split_top(s: str):
    """Split on top-level commas (bracket/paren aware)."""
    out, depth, cur = [], 0, []
    for ch in s:
        if ch in "[(":
            depth += 1
        elif ch in "])":
            depth -= 1
        if ch == "," and depth == 0:
            out.append("".join(cur))
            cur = []
        else:
            cur.append(ch)
    if cur:
        out.append("".join(cur))
    return out


def parse_sigs():
    sigs = []
    docdir = os.path.join(ROOT, "docs/stdlib")
    for name in sorted(os.listdir(docdir)):
        if not name.endswith(".md"):
            continue
        for line in open(os.path.join(docdir, name)):
            m = SIG_RX.match(line)
            if m:
                mod, fn, params, ret = m.groups()
                sigs.append((mod, fn, params, ret or "Unit"))
    return sigs


def synth(mod, fn, params, ret, variant):
    """A minimal program calling module.fn once, or None.

    variant 0: plain args, discarded bind.
    variant 1: FIRST arg bound as a `var` (mut-param fns, E032).
    variant 2: the bind annotated `Map[Int, Int]` (generic-empty
               producers, E018) — only sensible for Map returns.
    variant 3: EVERY arg hoisted into a `var` and passed by name — the
               reachability sweep's repertoire (a var-bound callback
               lowers where the inline lambda walls: the fs.for_each_line
               lesson, where the weaker shape produced a false
               wasm-unavailable row and E081 blocked a working fn).
    variant 4: hoisted args AND the call as the FINAL statement (no
               trailing print) — the same lesson's second half: the
               postlude itself walls some shapes.
    """
    args, tys = [], []
    for p in split_top(params):
        p = p.strip()
        if not p:
            continue
        # `name: Type`
        m = re.match(r"^[a-z0-9_]+:\s*(.+)$", p)
        ty = m.group(1) if m else p
        d = dummy(ty)
        if d is None:
            return None, ty
        args.append(d)
        tys.append(ty)
    prelude = ""
    if variant == 1:
        if not args:
            return None, "no-first-arg"
        prelude = f"  var subj = {args[0]}\n"
        args = ["subj"] + args[1:]
    elif variant in (3, 4):
        if not args:
            return None, "no-args"
        prelude = "".join(f"  var a{i} = {a}\n" for i, a in enumerate(args))
        args = [f"a{i}" for i in range(len(args))]
    call = f"{mod}.{fn}({', '.join(args)})"
    # Result-returning fns propagate; Unit-returning fns sit in STATEMENT
    # position (a bare Unit call is legal and matches real usage);
    # everything else binds discarded. The probe body opens with a print
    # so the call sits MID-BODY — a single-call main measures the
    # renderer's minimal-program shape support, not the fn (the
    # process.exit lesson: the fn was served, the one-statement main was
    # not, and the conflated wall row broke real programs through E081).
    is_result = ret.strip().startswith("Result[")
    is_unit = ret.strip() == "Unit"
    if variant == 2:
        if not ret.strip().startswith("Map["):
            return None, "no-map-ret"
        stmt = f"let _r: Map[Int, Int] = {call}"
    elif is_result:
        stmt = f"let _ = {call}!"
    elif is_unit:
        stmt = call
    else:
        stmt = f"let _ = {call}"
    if variant == 4:
        # The final-statement form: a Result-returning effect call rides
        # bare `call!` propagation (the sweep's shape-0 spelling).
        if is_result:
            stmt = f"{call}!"
        body = f"{prelude}  {stmt}"
    else:
        body = f"  println(\"pre\")\n{prelude}  {stmt}\n  println(\"p\")"
    imp = f"import {mod}\n\n" if mod in EXPLICIT_IMPORT else ""
    return f"{imp}effect fn main() -> Unit = {{\n{body}\n}}\n", None


def main():
    sigs = parse_sigs()
    tmp = tempfile.mkdtemp(prefix="almide-avail-")
    leg = "structural"
    if "--default-routing" in sys.argv[1:]:
        leg = "stock-p1"
    for i, a in enumerate(sys.argv[1:]):
        if a == "--leg":
            leg = sys.argv[1:][i + 1]
    if leg == "structural":
        env = dict(os.environ, ALMIDE_WASM_STRUCTURAL="1")
    else:
        env = dict(os.environ)
        env.pop("ALMIDE_WASM_STRUCTURAL", None)
    # Measure the ground truth, not our own declaration (see
    # check_wasm_availability's escape).
    env["ALMIDE_NO_AVAIL_CHECK"] = "1"
    for mod, fn, params, ret in sigs:
        verdict = None
        for variant in (0, 1, 2, 3, 4):
            prog, missing = synth(mod, fn, params, ret, variant)
            if prog is None:
                if variant == 0:
                    verdict = ("unsynth", missing)
                    break
                continue
            src = os.path.join(tmp, "probe.almd")
            with open(src, "w") as f:
                f.write(prog)
            if leg == "embedded":
                try:
                    r = subprocess.run(
                        [ALMIDE, "run", src, "--target", "wasm"],
                        capture_output=True, text=True, env=env, cwd=tmp,
                        stdin=subprocess.DEVNULL, timeout=120,
                    )
                except subprocess.TimeoutExpired:
                    # A hang is not a service verdict either way — record
                    # it honestly; the row needs human eyes, not a guess.
                    if verdict is None or verdict[0] != "wall":
                        verdict = ("wall", "probe-timeout (120s)")
                    continue
            else:
                r = subprocess.run(
                    [ALMIDE, "build", src, "--target", "wasm", "-o", os.devnull],
                    capture_output=True, text=True, env=env, cwd=tmp,
                )
            combined = r.stderr + r.stdout
            if leg == "embedded" and "unknown" in combined and " op " in combined:
                # The host answered "unknown ... op N": the run REACHED the
                # host and the service is absent — an embedded wall.
                first = next(
                    (l for l in combined.splitlines() if "unknown" in l), "?"
                )
                if verdict is None or verdict[0] != "wall":
                    verdict = ("wall", first[:120])
                continue
            if r.returncode == 0 or (
                leg == "embedded"
                and r.returncode != 0
                and "wall" not in combined
                and "error[E0" not in combined
                and "Expected" not in combined
            ):
                # Embedded service includes an answered runtime err (a
                # probe arg like a missing file) — the host served the op.
                verdict = ("ok", "")
                break
            first = next(
                (l for l in (r.stderr + r.stdout).splitlines() if l.strip()),
                "?",
            )
            # A type/synthesis error is the probe's fault — try the next
            # variant. A wall is only the VERDICT once every variant
            # walled: the ladder exists because verdicts are shape-
            # sensitive (the fs.for_each_line lesson — variant 0 walls,
            # variant 4 builds), so a first-shape wall must not stop it.
            if "error[E0" in first or "Expected" in first or "type error" in first.lower():
                # Never let a later shape's TYPE error demote an earlier
                # shape's renderer wall — verdict precedence is
                # ok > wall > unsynth.
                if verdict is None or verdict[0] != "wall":
                    verdict = ("unsynth", f"probe-ill-typed: {first[:80]}")
                continue
            if verdict is None or verdict[0] != "wall":
                verdict = ("wall", first[:120])
            continue
        status, detail = verdict
        print(f"{status}\t{mod}.{fn}\t{detail}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
