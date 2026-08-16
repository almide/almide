#!/usr/bin/env python3
"""Integer-domain edge matrix — every public stdlib fn x every Int parameter x every edge.

WHY THIS EXISTS
---------------
A room test written `pos + N <= len`, or a chunk count written `(total + n - 1) / n`, is
defeated by its own arithmetic: the sum wraps, the comparison passes, the store or the
allocation goes ahead. The shape has recurred in this repo five times, each fixed
point-wise, returning at whichever END nobody measured (#1408 at the negative end, the
SAME line at the positive end, `list.chunk`, `bytes.chunks`).

The differential fuzzer cannot close it, and the reason is written down in its own source.
`tools/xtarget-fuzz/src/generator/term.rs:363-365`:

    // Count/size/index Int parameter => a small non-pathological value
    // (avoids the `u32::MAX`/negative allocation-bomb noise).
    SigType::Int if COUNT_LIKE_PARAM_NAMES.contains(&param_name) =>
        Some(format!("{}", b.rng.pick(pools::SMALL_COUNT_POOL)))   // {0,1,2,3,4,5}

That decision is CORRECT — feeding `u32::MAX` to `repeat` manufactures an out-of-memory
"hang" that is noise, not a finding. But it draws the blind spot exactly over the
parameters the room guards read. It predicts which siblings are reachable and which are
not: `pos` is not in the name list, so `bytes.set_f32_le` was found; `size` is, so
`bytes.chunks` never could be.

So this is a SEPARATE instrument, not a fuzzer change. The fuzzer keeps its small-count
rule. Here the extremes are fed deliberately, exhaustively, and the expectation is
cross-leg AGREEMENT rather than speed — an allocation bomb is a declared outcome, not a
finding.

WHAT THE SURVEY SAID TO BUILD (../almide-references/RESEARCH-integer-domain-guards.md)
--------------------------------------------------------------------------------------
No compiler of the nine enforces how a guard is PHRASED — clippy's lints for it are
allow-by-default and not enabled on rustc's own source, and Zig has no lint layer. Three
things were worth copying, and they are what this file implements:

  * Swift `utils/SwiftIntTypes.py` — ONE declared table, consumed by every cell, so adding
    a width regenerates the matrix instead of needing a new hand-written case.
  * Zig `lib/std/mem.zig:4963` — the far edge is COMPUTED from the buffer
    (`offset_at_end = @bitSizeOf(Backing) - @bitSizeOf(Packed)`), never written as a
    literal, so a new width brings its own edge case along.
  * Rust `src/tools/tidy/src/target_policy.rs:27-62` — discover the family by WALKING the
    implementation, subtract what the tests cover, fail on the remainder, and name every
    exception so each hole is attributed rather than silent.

Usage:
    python3 tools/domain_edge_matrix.py                 # measure, print the matrix
    python3 tools/domain_edge_matrix.py --json out.json # machine-readable
    python3 tools/domain_edge_matrix.py --only bytes    # one module
"""

import argparse
import json
import os
import re
import subprocess
import sys
import tempfile
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent

# ── The declared table (Swift's SwiftIntTypes.py role) ───────────────────────
#
# Every cell in the matrix is generated from this. A value added here appears
# against every function at once; there is no per-function edge list to keep in
# sync, which is the drift Rust's hand-rolled ~200 `test_impl_try_from_*!`
# invocations still carry.
I64_MAX = 9223372036854775807
I64_MIN = -9223372036854775808

FIXED_EDGES = [
    ("zero", 0),
    ("one", 1),
    ("neg_one", -1),
    ("i32_max", 2147483647),
    ("i32_min", -2147483648),
    ("u32_max", 4294967295),
    ("two_pow_32", 4294967296),
    ("i64_max_m1", I64_MAX - 1),
    ("i64_max", I64_MAX),
    ("i64_min_p1", I64_MIN + 1),
    ("i64_min", I64_MIN),
]

# The far edges are DERIVED from the synthesized receiver, not written down —
# Zig's `offset_at_end` rule. `len` is the length of whatever collection the
# call's other arguments were built from, so a function that grows a new
# receiver shape gets its boundary cells for free.
def derived_edges(recv_len):
    if recv_len is None:
        return []
    return [
        ("len_m1", recv_len - 1),
        ("len", recv_len),
        ("len_p1", recv_len + 1),
    ]

# ── Value table: how to build a non-target argument of each type ─────────────
#
# Deliberately small and concrete. A type that is not here makes the whole cell
# SKIPPED and counted — an honest hole, in the ledger, rather than a silent
# absence.
RECV_LEN = 5

VALUES = {
    "Int": "3",
    "Float": "1.5",
    "Bool": "true",
    "String": '"abcde"',
    "Bytes": "bytes.from_list([1, 2, 3, 4, 5])",
    "List[Int]": "[1, 2, 3, 4, 5]",
    "List[String]": '["a", "b", "c", "d", "e"]',
    "List[Float]": "[1.0, 2.0, 3.0, 4.0, 5.0]",
    "List[Bool]": "[true, false, true, false, true]",
}

# Length of the collection each receiver type carries, for the derived edges.
RECV_LENS = {
    "String": RECV_LEN,
    "Bytes": RECV_LEN,
    "List[Int]": RECV_LEN,
    "List[String]": RECV_LEN,
    "List[Float]": RECV_LEN,
    "List[Bool]": RECV_LEN,
}

# How to render a result of each return type as a printable digest. A return
# type absent here also makes the cell SKIPPED — the matrix reports what it
# could not measure rather than pretending.
RENDER = {
    "Int": 'int.to_string({e})',
    "Float": 'float.to_string({e})',
    "Bool": '(if {e} then "t" else "f")',
    "String": '{e}',
    "Bytes": 'int.to_string(bytes.len({e}))',
    "List[Int]": 'int.to_string(list.len({e}))',
    "List[String]": 'int.to_string(list.len({e}))',
    "List[Float]": 'int.to_string(list.len({e}))',
    "List[Bool]": 'int.to_string(list.len({e}))',
    "List[Bytes]": 'int.to_string(list.len({e}))',
    "List[List[Int]]": 'int.to_string(list.len({e}))',
    "List[List[String]]": 'int.to_string(list.len({e}))',
    "Unit": None,  # call for effect only
}

# Modules that need an explicit import in the generated program.
NEEDS_IMPORT = {"bytes", "json", "fs", "http", "env", "io", "random", "regex",
                "process", "testing", "matrix"}

# The `COUNT_LIKE_PARAM_NAMES` list the fuzzer uses to shrink a parameter to
# {0..5}. Kept in sync by the gate below, not by hand: a name added there and
# not here would silently narrow the blind spot without widening this matrix.
FUZZER_COUNT_LIKE = {
    "n", "count", "len", "times", "size", "decimals", "width", "k", "i", "j",
    "index", "start", "end", "lo", "hi",
}

SIG_RE = re.compile(
    r'^(?:@intrinsic\([^)]*\)\s*\n)?(?:effect\s+)?fn\s+(\w+)\(([^)]*)\)\s*->\s*([^=\n]+?)\s*=',
    re.M,
)


def parse_type(t):
    return t.strip().rstrip("?").strip()


def parse_stdlib(only=None):
    """Walk the implementation and discover the family (tidy's rule 1)."""
    out = []
    for path in sorted((REPO / "stdlib").glob("*.almd")):
        module = path.stem.split("_")[0]
        if only and module != only:
            continue
        src = path.read_text()
        for m in SIG_RE.finditer(src):
            fn, raw_params, ret = m.group(1), m.group(2), parse_type(m.group(3))
            if fn.startswith("__") or fn.startswith("impl_"):
                continue
            params = []
            ok = True
            for p in [x for x in raw_params.split(",") if x.strip()]:
                if ":" not in p:
                    ok = False
                    break
                name, ty = p.split(":", 1)
                params.append((name.strip(), parse_type(ty)))
            if not ok or not params:
                continue
            if not any(ty == "Int" for _, ty in params):
                continue
            out.append({"module": module, "fn": fn, "params": params, "ret": ret})
    return out


def build_program(sig, target_idx, value):
    """Emit the smallest program that puts `value` in one Int slot."""
    args, recv_len = [], None
    for i, (_, ty) in enumerate(sig["params"]):
        if i == target_idx:
            args.append(str(value))
            continue
        if ty not in VALUES:
            return None, None
        args.append(VALUES[ty])
        if recv_len is None:
            recv_len = RECV_LENS.get(ty)

    call = f'{sig["module"]}.{sig["fn"]}({", ".join(args)})'
    tmpl = RENDER.get(sig["ret"], "MISSING")
    if tmpl == "MISSING":
        return None, None

    imports = f"import {sig['module']}\n" if sig["module"] in NEEDS_IMPORT else ""
    if tmpl is None:
        body = f"  let _ = {call}\n  println(\"unit\")"
    else:
        body = f'  println({tmpl.format(e=call)})'
    return f"{imports}fn main() -> Unit = {{\n{body}\n}}\n", recv_len


def run_leg(almide, src_path, wasm):
    cmd = [almide, "run", str(src_path)] + (["--target", "wasm"] if wasm else [])
    env = dict(os.environ, PATH="/opt/homebrew/bin:" + os.environ.get("PATH", ""))
    try:
        # `errors="replace"`: a byte-level stdlib fn can print raw non-UTF-8 bytes,
        # and a decode crash in the harness would be indistinguishable from "no
        # divergence here" — the instrument must never lose a cell to its own I/O.
        p = subprocess.run(cmd, capture_output=True, text=True, errors="replace",
                           timeout=45, env=env)
        return p.returncode, p.stdout, p.stderr
    except subprocess.TimeoutExpired:
        return "timeout", "", ""


# The v1 renderer DECLINES a shape outside its subset with this line, on purpose.
# A wall is the repo's honest-error contract, not a divergence — counting it as one
# is how a first run of this matrix reported 812 "findings" over `bytes`, nearly all
# of them shapes the renderer had correctly refused. An instrument that cannot tell a
# refusal from a wrong answer is worse than no instrument.
WALL_MARK = "not yet supported by the verified wasm renderer"


def classify(nat, wasm):
    """Compare stdout + exit code ONLY; stderr is read for the wall mark alone.

    Folding stderr into the comparison looks harmless and is not: native prints
    warnings the wasm leg does not, and the two legs word some diagnostics
    differently, so every such cell became a "divergence". A first run with
    stderr in the comparison reported 2157 of 3614 cells divergent, including
    `bytes.get(b, 0)` on a five-byte buffer — which is simply correct on both.
    """
    (nrc, nout, _nerr), (wrc, wout, werr) = nat, wasm
    if nrc == "timeout" or wrc == "timeout":
        return "BOMB"       # a declared outcome, not a finding — see the header
    if wrc != 0 and WALL_MARK in werr:
        return "WALL"
    if nrc == wrc and nout == wout:
        return "AGREE"
    return "DIVERGE"


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--json")
    ap.add_argument("--only")
    ap.add_argument("--almide", default=str(REPO / "target/release/almide"))
    args = ap.parse_args()

    sigs = parse_stdlib(args.only)
    cells, skipped = [], []
    tmp = Path(tempfile.mkdtemp(prefix="domain-edges-"))

    for sig in sigs:
        for idx, (pname, ty) in enumerate(sig["params"]):
            if ty != "Int":
                continue
            probe, recv_len = build_program(sig, idx, 0)
            if probe is None:
                skipped.append(f'{sig["module"]}.{sig["fn"]}({pname}) — unbuildable arg/ret')
                continue
            edges = FIXED_EDGES + derived_edges(recv_len)
            for ename, value in edges:
                src, _ = build_program(sig, idx, value)
                f = tmp / "probe.almd"
                f.write_text(src)
                verdict = classify(run_leg(args.almide, f, False),
                                   run_leg(args.almide, f, True))
                cells.append({
                    "module": sig["module"], "fn": sig["fn"], "param": pname,
                    "edge": ename, "value": value, "verdict": verdict,
                    "fuzzer_reachable": pname not in FUZZER_COUNT_LIKE,
                })
                if verdict == "DIVERGE":
                    print(f'  DIVERGE  {sig["module"]}.{sig["fn"]}  {pname}={ename}'
                          f'  (fuzzer-reachable: {pname not in FUZZER_COUNT_LIKE})',
                          flush=True)

    tally = {}
    for c in cells:
        tally[c["verdict"]] = tally.get(c["verdict"], 0) + 1
    blind = sum(1 for c in cells if not c["fuzzer_reachable"])

    print(f"\ndomain-edge matrix: {len(cells)} cells over {len(sigs)} signatures")
    for k in sorted(tally):
        print(f"  {k:8} {tally[k]}")
    print(f"  cells the fuzzer can never synthesize: {blind}")
    print(f"  skipped signatures (unbuildable): {len(skipped)}")

    if args.json:
        Path(args.json).write_text(json.dumps(
            {"cells": cells, "skipped": skipped, "tally": tally}, indent=2))
    return 1 if tally.get("DIVERGE") else 0


if __name__ == "__main__":
    sys.exit(main())
