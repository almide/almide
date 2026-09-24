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
    python3 tools/domain_edge_matrix.py --only bytes --edge i32_max  # one edge (local loop)

The wasm leg runs under a DECLARED linear-memory budget (WASM_BUDGET_BYTES below,
#2387) on the wasmtime CLI, never on the embedded host whose ceiling is the
machine's free memory. The budget is recorded in the JSON and checked by
tools/domain_edge_ledger.py before any reading can reach the ledger.
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
# Deliberately small and concrete. A type that is not here makes the whole slot
# SKIPPED and NAMED — an honest hole, in the ledger, rather than a silent
# absence. The naming is the part that was missing (#2402): the skip count was
# printed and never read back, and for `json` it hid 12 of 13 Int slots,
# including `json.index(path: JsonPath, i: Int)` — the signature that carried
# #2396, a wasm-only i32 truncation of the path index. The skipped population
# is now printed by name, written to the ledger, and a skip that APPEARS fails
# the gate exactly as a divergence that appears does.
RECV_LEN = 5

# The known document every JSON cell is measured against. Three elements, so
# an index edge at `len`, `len_p1`, or 2^32+1 has something to be wrong about.
JSON_DOC = 'json.parse("[10, 20, 30]") ?? value.null()'

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
    "List[List[String]]": '[["a"], ["b"], ["c"], ["d"], ["e"]]',
    # A JsonPath is opaque: the only value a program can hold is one built
    # from `json.root()` by the path constructors, so the receiver is the root.
    "JsonPath": "json.root()",
    "Value": JSON_DOC,
    "List[Value]": "[value.int(1), value.int(2), value.int(3), value.int(4), value.int(5)]",
}

# Length of the collection each receiver type carries, for the derived edges.
RECV_LENS = {
    "String": RECV_LEN,
    "Bytes": RECV_LEN,
    "List[Int]": RECV_LEN,
    "List[String]": RECV_LEN,
    "List[Float]": RECV_LEN,
    "List[Bool]": RECV_LEN,
    "List[List[String]]": RECV_LEN,
    "List[Value]": RECV_LEN,
    "Value": 3,       # JSON_DOC has three elements
    "JsonPath": 3,    # a path is applied to JSON_DOC, so its far edge is the same
}

# How to render a result of each return type as a printable digest. A return
# type absent here also makes the slot SKIPPED and named — the matrix reports
# what it could not measure rather than pretending. `T?` and `Result[T, E]`
# are rendered structurally by `render_for` (a `match` that prints which arm
# was taken and the digest of the payload), so only the base types live here.
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
    "List[Value]": 'int.to_string(list.len({e}))',
    "Value": 'json.stringify({e})',
    # A JsonPath is opaque, so it is rendered by APPLYING it to the known
    # document — removing at the path and printing what is left is the only
    # way an out-of-range step is observable at all. #2396 was exactly a step
    # that landed on a real element after wrapping; `[10,30]` where `[10,20,30]`
    # was owed is the digest that catches it.
    "JsonPath": f'json.stringify(json.remove_path({JSON_DOC}, {{e}}))',
    "Unit": None,  # call for effect only
}


def value_for(ty):
    """The argument expression for a non-target parameter, or None (unbuildable)."""
    if ty in VALUES:
        return VALUES[ty]
    if ty.endswith("?") and ty[:-1] in VALUES:
        return f"some({VALUES[ty[:-1]]})"
    return None


def split_top(s):
    """Split on commas at bracket depth zero: `Map[String, Int], n: Int` is two."""
    out, depth, cur = [], 0, ""
    for ch in s:
        if ch in "[(":
            depth += 1
        elif ch in "])":
            depth -= 1
        if ch == "," and depth == 0:
            out.append(cur)
            cur = ""
        else:
            cur += ch
    if cur.strip():
        out.append(cur)
    return out


def render_for(ty, e):
    """The printable digest of `e : ty`, "" for Unit, or None (unrenderable)."""
    if ty in RENDER:
        return "" if RENDER[ty] is None else RENDER[ty].format(e=e)
    if ty.endswith("?"):
        inner = render_for(ty[:-1], "x")
        if inner:
            return f'(match {e} {{ some(x) => "some:" + {inner}, none => "none" }})'
        return None
    if ty.startswith("Result[") and ty.endswith("]"):
        parts = [p.strip() for p in split_top(ty[len("Result["):-1])]
        if len(parts) == 2:
            ok, err = render_for(parts[0], "x"), render_for(parts[1], "m")
            if ok and err:
                return f'(match {e} {{ ok(x) => "ok:" + {ok}, err(m) => "err:" + {err} }})'
    return None

# Modules that need an explicit import in the generated program.
NEEDS_IMPORT = {"bytes", "json", "fs", "http", "env", "io", "random", "regex",
                "process", "testing", "matrix", "net", "zlib", "args", "mem"}

# The `COUNT_LIKE_PARAM_NAMES` list the fuzzer uses to shrink a parameter to
# {0..5}. Kept in sync by the gate below, not by hand: a name added there and
# not here would silently narrow the blind spot without widening this matrix.
FUZZER_COUNT_LIKE = {
    "n", "count", "len", "times", "size", "decimals", "width", "k", "i", "j",
    "index", "start", "end", "lo", "hi",
}

# `fn name[A, B](` — the generic list is captured so the type variables can be
# instantiated (at Int) rather than the whole signature being invisible: the
# old `fn\s+(\w+)\(` form could not see `list.get[A](xs: List[A], i: Int)` at
# all, and 25 generic fns with an Int parameter were outside the family with
# nothing counting them.
SIG_HEAD_RE = re.compile(
    r'^(?:@intrinsic\([^)]*\)\s*\n)?(effect\s+)?fn\s+(\w+)(?:\[([^\]]*)\])?\(',
    re.M,
)


def parse_type(t):
    # `?` is KEPT: an `Int?` return rendered as a bare `Int` is a program that
    # does not type-check, and a probe that fails to compile on both legs used
    # to be counted as AGREE (both exit 1, both print nothing). See `probe`.
    return re.sub(r"\s+", " ", t.strip())


def instantiate(ty, generics):
    for g in generics:
        ty = re.sub(rf"\b{re.escape(g)}\b", "Int", ty)
    return ty


def parse_signature(src, m):
    """Read `(params) -> ret =` from a signature head, bracket-depth aware.

    Returns (params, ret) or None when the head is not a signature we can read
    (a callback parameter's `->` inside the parens, an `=` before the arrow).
    """
    i, depth = m.end(), 1
    while i < len(src) and depth:
        depth += {"(": 1, "[": 1, ")": -1, "]": -1}.get(src[i], 0)
        i += 1
    if depth:
        return None
    raw_params = src[m.end():i - 1]
    tail = re.match(r"\s*->\s*([^=\n]+?)\s*=", src[i:])
    if not tail:
        return None
    return raw_params, parse_type(tail.group(1))


def parse_stdlib(only=None, stdlib_dir=None):
    """Walk the implementation and discover the family (tidy's rule 1).

    A signature the walker can SEE but cannot READ (a parameter without a
    `:`, a callback type) is returned with `unreadable` set, so it lands in
    the skipped population by name instead of vanishing.
    """
    out = []
    for path in sorted((stdlib_dir or REPO / "stdlib").glob("*.almd")):
        module = path.stem.split("_")[0]
        if only and module != only:
            continue
        src = path.read_text()
        for m in SIG_HEAD_RE.finditer(src):
            fn = m.group(2)
            if fn.startswith("__") or fn.startswith("impl_"):
                continue
            generics = [g.split(":")[0].strip() for g in (m.group(3) or "").split(",") if g.strip()]
            parsed = parse_signature(src, m)
            if parsed is None:
                continue
            raw_params, ret = parsed
            params, unreadable = [], None
            for p in split_top(raw_params):
                if ":" not in p:
                    unreadable = f"parameter without a type annotation: `{p.strip()}`"
                    break
                name, ty = p.split(":", 1)
                params.append((name.strip(), instantiate(parse_type(ty), generics)))
            if not params or not any(ty == "Int" for _, ty in params):
                continue
            out.append({"module": module, "fn": fn, "params": params,
                        "ret": instantiate(ret, generics), "generics": generics,
                        "effect": bool(m.group(1)), "unreadable": unreadable})
    return out


def slot_name(sig, pname):
    return f'{sig["module"]}.{sig["fn"]}({pname})'


def build_program(sig, target_idx, value):
    """Emit the smallest program that puts `value` in one Int slot.

    Returns (source, recv_len) or (None, reason) — the reason is the NAME of
    the hole, so a skip is attributed the moment it is decided.
    """
    if sig["unreadable"]:
        return None, sig["unreadable"]
    args, recv_len, lets = [], None, []
    for i, (pname, ty) in enumerate(sig["params"]):
        if i == target_idx:
            args.append(str(value))
            continue
        arg = value_for(ty)
        if arg is None:
            return None, f"unbuildable: no VALUES entry for parameter type `{ty}`"
        # A `mut` parameter cannot take a temporary (E032): bind it first.
        if pname.startswith("mut "):
            lets.append(f"  var v{i} = {arg}")
            arg = f"v{i}"
        args.append(arg)
        if recv_len is None:
            recv_len = RECV_LENS.get(ty)

    # An effect fn is called with `!` from an effect main (ADR-0008).
    call = f'{sig["module"]}.{sig["fn"]}({", ".join(args)}){"!" if sig["effect"] else ""}'
    rendered = render_for(sig["ret"], call)
    if rendered is None:
        return None, f"unrenderable: no RENDER entry for return type `{sig['ret']}`"

    if rendered == "":
        body = f"  let _ = {call}\n  println(\"unit\")"
    else:
        body = f'  println({rendered})'
    body = "\n".join(lets + [body])
    # Every module the program MENTIONS is imported, not just the one under
    # test: a `value` cell renders through `json.stringify`.
    imports = "".join(f"import {mod}\n" for mod in sorted(NEEDS_IMPORT) if f"{mod}." in body)
    head = "effect fn main" if sig["effect"] else "fn main"
    return f"{imports}{head}() -> Unit = {{\n{body}\n}}\n", recv_len


# The family is "every PUBLIC stdlib fn". The walk over stdlib/*.almd also sees
# the self-host implementation helpers (`json_path_rm_index`, `list_swap_value`,
# …) that live beside the public fns and are spliced in by name — a program
# cannot call them, so a probe against one is E002 on both legs and was
# counted as AGREE. The public surface is what `almide compile <module> --json`
# lists; helpers are NAMED in the summary but are not slots. If the interface
# cannot be read the walk is trusted whole and any helper surfaces as an E002
# skip, which is the louder failure.
def public_surface(almide, module):
    env = dict(os.environ, PATH="/opt/homebrew/bin:" + os.environ.get("PATH", ""))
    try:
        p = subprocess.run([almide, "compile", module, "--json"], capture_output=True,
                           text=True, errors="replace", timeout=60, env=env)
        return {f["name"] for f in json.loads(p.stdout)["functions"]} if p.returncode == 0 else None
    except (subprocess.TimeoutExpired, ValueError, KeyError, OSError):
        return None


# A probe that does not TYPE-CHECK is not a measurement. Both legs exit 1 and
# print nothing, so `classify` reads AGREE — the first run of this matrix
# counted `list.get(xs, i)` (returns `Int?`, rendered as `Int`) as 14 agreeing
# cells that were 14 compile errors. The check is one `almide check` per slot
# at value 0 (~20 ms), and a failure is a NAMED skip carrying the diagnostic.
def probe(almide, src_path):
    env = dict(os.environ, PATH="/opt/homebrew/bin:" + os.environ.get("PATH", ""))
    p = subprocess.run([almide, "check", str(src_path)], capture_output=True,
                       text=True, errors="replace", timeout=60, env=env)
    if p.returncode == 0:
        return None
    first = next((ln for ln in (p.stderr + p.stdout).splitlines() if ln.startswith("error[")), "")
    return f"probe does not type-check: {first or 'exit ' + str(p.returncode)}"


# ── The wasm leg's DECLARED linear-memory budget (#2387) ─────────────────────
#
# `almide run --target wasm` executes on the embedded host with
# `StoreLimits::default()` (crates/almide-wasm-run/src/host.rs:663): its ceiling
# is whatever the machine has free at that instant. Plain allocations carry NO
# chosen cap by ratification (scripts/check-alloc-ceilings.sh, A 2026-08-17), so
# for a cell that asks the wasm leg for 2 GiB — every 1-byte-per-element fn at
# `i32_max` — the SAME binary answered on a quiet machine and took the C-197
# abort under load, and the classifier read those as AGREE and DIVERGE of one
# cell minutes apart. A verdict that tracks free memory is not a measurement of
# the compiler, and a shrink-only ledger freezes whichever reading landed first.
#
# So the wasm leg is run under a FIXED budget, structurally: the module is built
# once and executed on the wasmtime CLI with `-W max-memory-size=<budget>`.
# Growth past the budget fails, which the emitted allocator turns into the
# DEFINED "Error: out of memory" + exit 1 (C-197) — the same line the leg takes
# past the 4 GiB wasm32 bound, now at a boundary that does not depend on RAM.
# Any request >= the budget aborts on every machine; the budget is recorded in
# the measurement (`wasm_budget_bytes`, run-level and per cell as `pinned`) and
# declared in the ledger header, and tools/domain_edge_ledger.py refuses a
# measurement that is not pinned to the ledger's declared budget. A row then
# means "diverges at the declared budget", not "diverged on whatever machine
# last ran this".
#
# 1 GiB: far above anything a 5-element receiver needs, below every edge that
# sizes an allocation (`i32_max` = 2 GiB for a 1-byte element). The native leg
# is NOT capped: RLIMIT_AS is unenforced on Darwin (setrlimit -> EINVAL, measured
# 2026-09-21), so a native cap would split the local and CI verdicts; its bound
# stays host availability at 2 GiB exactly as it already is at 4 GiB for every
# declared `u32_max` row (#2387 item 2, open). The direction that flipped in
# every recorded reading was the wasm leg, and that one is now pinned.
WASM_BUDGET_BYTES = 1 << 30

# The C-197 line, byte-exact: the ONLY abort a budget-pinned leg may take past
# the budget. Anything else past it is a finding.
OOM_LINE = "Error: out of memory"


def _tool_env():
    return dict(os.environ, PATH="/opt/homebrew/bin:" + os.environ.get("PATH", ""))


def _run(cmd, timeout=45):
    try:
        # `errors="replace"`: a byte-level stdlib fn can print raw non-UTF-8 bytes,
        # and a decode crash in the harness would be indistinguishable from "no
        # divergence here" — the instrument must never lose a cell to its own I/O.
        p = subprocess.run(cmd, capture_output=True, text=True, errors="replace",
                           timeout=timeout, env=_tool_env())
        return p.returncode, p.stdout, p.stderr
    except subprocess.TimeoutExpired:
        return "timeout", "", ""


def run_leg(almide, src_path, wasm, wasm_budget=WASM_BUDGET_BYTES):
    """One leg's (returncode, stdout, stderr).

    Native: `almide run`. Wasm: `almide build --target wasm` (a wall — the
    renderer's honest refusal — surfaces here with its mark on stderr and a
    non-zero code, exactly as `almide run --target wasm` reported it), then the
    module on the wasmtime CLI under the declared budget.
    """
    src_path = Path(src_path)
    if not wasm:
        return _run([almide, "run", str(src_path)])
    module = src_path.with_suffix(".wasm")
    rc, out, err = _run([almide, "build", str(src_path), "--target", "wasm", "-o", str(module)])
    if rc != 0:
        return rc, out, err
    return _run(["wasmtime", "run", "-W", f"max-memory-size={wasm_budget}", str(module)])


def wasmtime_available():
    """The pinned leg needs the wasmtime CLI; without it the instrument must
    refuse to measure rather than fall back to an unpinned leg."""
    try:
        return _run(["wasmtime", "--version"], timeout=10)[0] == 0
    except (OSError, FileNotFoundError):
        return False


# The v1 renderer DECLINES a shape outside its subset with this line, on purpose.
# A wall is the repo's honest-error contract, not a divergence — counting it as one
# is how a first run of this matrix reported 812 "findings" over `bytes`, nearly all
# of them shapes the renderer had correctly refused. An instrument that cannot tell a
# refusal from a wrong answer is worse than no instrument.
WALL_MARK = "not yet supported by the verified wasm renderer"

# Modules whose surface is RAW MEMORY, where cross-target agreement is not
# promised and never was: `prim.load32(garbage)` reads two genuinely different
# memory models. Their cells are still MEASURED and still counted — hiding them
# would make the headline number a lie by omission — but they are reported as
# their own verdict so they cannot be mistaken for defects in a contracted API.
# 273 of the first full run's 491 divergences were `prim`, and calling those bugs
# would have buried the 218 that are real.
RAW_MODULES = {"prim"}


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


def verdict_for(module, nat, wasm):
    v = classify(nat, wasm)
    return "RAW" if v == "DIVERGE" and module in RAW_MODULES else v


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--json")
    ap.add_argument("--only")
    ap.add_argument("--almide", default=str(REPO / "target/release/almide"))
    # The population and its skips WITHOUT running any edge: the type-check
    # probe still runs, so a slot that stopped compiling is still seen. This is
    # what the gate's skip ledger is regenerated from, and what the negative
    # test forges a signature into.
    ap.add_argument("--skips-only", action="store_true")
    ap.add_argument("--stdlib", help="walk this directory instead of stdlib/ (tests)")
    ap.add_argument("--edge", action="append", default=None, metavar="NAME",
                    help="restrict to these edge names (repeatable) — a fast local loop; "
                         "the ledger gate never scopes by edge")
    args = ap.parse_args()

    # An edge run measures the wasm leg on the wasmtime CLI under the declared
    # budget; without the CLI the only alternative is an unpinned leg that
    # answers with the machine's free memory (#2387), so refuse instead. A
    # skips-only run executes nothing and needs no host.
    if not args.skips_only and not wasmtime_available():
        print("domain-edges: the wasmtime CLI is not on PATH (/opt/homebrew/bin is "
              "prepended) — the wasm leg is measured under a declared budget on it, "
              "and an unpinned leg would answer with the machine's free memory (#2387). "
              "Refusing to measure.", file=sys.stderr)
        return 2
    if not args.skips_only:
        print(f"domain-edges: wasm leg pinned at wasm_budget_bytes={WASM_BUDGET_BYTES} "
              f"(wasmtime CLI, -W max-memory-size); native leg uncapped", flush=True)

    sigs = parse_stdlib(args.only, Path(args.stdlib) if args.stdlib else None)
    cells, skipped, retried_away, exercised, helpers = [], [], [], [], []
    tmp = Path(tempfile.mkdtemp(prefix="domain-edges-"))
    surface = {}

    for sig in sigs:
        mod = sig["module"]
        if mod not in surface:
            surface[mod] = public_surface(args.almide, mod)
        if surface[mod] is not None and sig["fn"] not in surface[mod]:
            helpers.append(f'{mod}.{sig["fn"]}')
            continue
        for idx, (pname, ty) in enumerate(sig["params"]):
            if ty != "Int":
                continue
            slot = slot_name(sig, pname)
            src, recv_len = build_program(sig, idx, 0)
            if src is None:
                skipped.append({"slot": slot, "reason": recv_len})
                continue
            f = tmp / "probe.almd"
            f.write_text(src)
            reason = probe(args.almide, f)
            if reason is not None:
                skipped.append({"slot": slot, "reason": reason})
                continue
            exercised.append(slot)
            if args.skips_only:
                continue
            edges = FIXED_EDGES + derived_edges(recv_len)
            if args.edge:
                edges = [e for e in edges if e[0] in args.edge]
            for ename, value in edges:
                src, _ = build_program(sig, idx, value)
                f = tmp / "probe.almd"
                f.write_text(src)
                verdict = verdict_for(sig["module"],
                                      run_leg(args.almide, f, False),
                                      run_leg(args.almide, f, True))
                # SOLO RETRY, the `output-parity.sh` discipline: a DIVERGE is
                # re-measured once, alone, before it is believed.
                #
                # The sweep runs thousands of programs back to back and some of
                # them are DELIBERATELY enormous — the edge set includes u32::MAX
                # and i64::MAX against parameters that size an allocation, which
                # is the whole point of this instrument. A cell sitting just
                # inside the 2 GiB ceiling therefore asks BOTH legs for 2 GiB, and
                # under memory pressure one of them can lose that race while the
                # other wins. That is a machine artifact wearing a divergence's
                # clothes, and it is indistinguishable from the real thing in the
                # verdict alone.
                #
                # Measured: a full sweep run CONCURRENTLY with another heavy job
                # reported `string.pad_start:n:i32_max` and `pad_end:n:i32_max` as
                # divergent; re-run on a quiet machine both agree, and so does the
                # ceiling value itself (2147483648). Two phantom rows out of 7767
                # is enough to make a shrink-only ledger untrustworthy, because a
                # phantom that lands once is then declared forever.
                #
                # The retry is cheap precisely because DIVERGE is rare, and it can
                # only ever REMOVE a finding: a cell that diverges twice is kept.
                if verdict == "DIVERGE":
                    verdict = verdict_for(sig["module"],
                                          run_leg(args.almide, f, False),
                                          run_leg(args.almide, f, True))
                    if verdict != "DIVERGE":
                        retried_away.append(
                            f'{sig["module"]}.{sig["fn"]}:{pname}:{ename} '
                            f'(first pass DIVERGE, solo re-run {verdict})')
                cells.append({
                    "module": sig["module"], "fn": sig["fn"], "param": pname,
                    "edge": ename, "value": value, "verdict": verdict,
                    "fuzzer_reachable": pname not in FUZZER_COUNT_LIKE,
                    # The wasm reading was taken under WASM_BUDGET_BYTES. The
                    # ledger tool refuses a DIVERGE whose cell says otherwise.
                    "pinned": True,
                })
                if verdict == "DIVERGE":
                    print(f'  DIVERGE  {sig["module"]}.{sig["fn"]}  {pname}={ename}'
                          f'  (fuzzer-reachable: {pname not in FUZZER_COUNT_LIKE})',
                          flush=True)

    if retried_away:
        print(f"\n  {len(retried_away)} cell(s) diverged once and AGREED on the solo re-run —")
        print("  recorded as load artifacts, not findings:")
        for r in retried_away:
            print(f"    {r}")

    tally = {}
    for c in cells:
        tally[c["verdict"]] = tally.get(c["verdict"], 0) + 1
    blind = sum(1 for c in cells if not c["fuzzer_reachable"])

    # A "signature" here is one Int-parameter SLOT, `module.fn(param)` — the
    # unit the matrix iterates and the unit a skip is attributed to. The line
    # names every skipped slot so that a silent skip cannot recur (#2402): the
    # count alone was printed for a month and never read back.
    total = len(exercised) + len(skipped)
    print(f"\ndomain-edge matrix: {len(cells)} cells over {len(exercised)} signatures"
          f"{' (skips-only: no edge was run)' if args.skips_only else ''}")
    for k in sorted(tally):
        print(f"  {k:8} {tally[k]}")
    print(f"  cells the fuzzer can never synthesize: {blind}")
    print(f"  coverage: {len(exercised)} of {total} signatures exercised, skipped: ["
          + ", ".join(f'{s["slot"]}: {s["reason"]}' for s in skipped) + "]")
    print(f"  outside the public surface (self-host helpers, reached only through their "
          f"public callers; not slots): {len(helpers)} [" + ", ".join(helpers) + "]")

    if args.json:
        Path(args.json).write_text(json.dumps(
            {"cells": cells, "skipped": skipped, "exercised": exercised, "helpers": helpers,
             "skips_only": bool(args.skips_only), "tally": tally,
             # The budget every wasm reading above was taken under. The ledger
             # tool compares it with the ledger's declared one and refuses a
             # mismatch or an absence — an unpinned reading is an availability
             # reading (#2387), and it must not reach the ledger as a verdict.
             "wasm_budget_bytes": WASM_BUDGET_BYTES,
             "native_budget": "uncapped",
             "edges": sorted(args.edge) if args.edge else "all"}, indent=2))
    return 1 if tally.get("DIVERGE") else 0


if __name__ == "__main__":
    sys.exit(main())


# ── Triage: give every divergent cell a MEASURED class ───────────────────────
#
# "218 divergent cells" is not a work estimate until each one says HOW it
# diverges. Three classes need three different responses, and only the first is
# unambiguously a defect:
#
#   SILENT_WRONG  both legs exit 0 and print different answers. The worst class,
#                 and the one this language exists to remove.
#   ABORT_FORM    both legs fail, in different forms. ALS-T6 says a domain error
#                 is one unified abort; a raw panic (exit 101) or a wasm trap
#                 (134) is the forbidden form, so this is a defect too, but the
#                 fix is the FORM, not the value.
#   ONE_SIDED     one leg answers, the other fails. Usually means the domain rule
#                 itself was never decided — someone has to rule on what
#                 `string.pad_start(s, 2^32)` MEANS before either leg can be
#                 called wrong.
#
# Run: python3 tools/domain_edge_matrix.py --triage measured.json
def triage(json_path, almide):
    import collections
    cells = json.load(open(json_path))["cells"]
    div = [c for c in cells if c["verdict"] == "DIVERGE" and c["module"] not in RAW_MODULES]
    sigs = {}
    for mod in {c["module"] for c in div}:
        for s in parse_stdlib(mod):
            sigs[(s["module"], s["fn"])] = s
    tmp = Path(tempfile.mkdtemp(prefix="triage-"))
    out, tally = [], collections.Counter()
    for c in div:
        sig = sigs.get((c["module"], c["fn"]))
        if sig is None:
            continue
        idx = next((i for i, (n, _t) in enumerate(sig["params"]) if n == c["param"]), None)
        if idx is None:
            continue
        src, _ = build_program(sig, idx, c["value"])
        if src is None:
            continue
        f = tmp / "p.almd"
        f.write_text(src)
        nrc, nout, _ = run_leg(almide, f, False)
        wrc, wout, _ = run_leg(almide, f, True)
        if nrc == 0 and wrc == 0:
            cls = "SILENT_WRONG"
        elif nrc != 0 and wrc != 0:
            cls = "ABORT_FORM"
        else:
            cls = "ONE_SIDED"
        tally[cls] += 1
        out.append({**c, "klass": cls, "native": [nrc, nout.strip()[:60]],
                    "wasm": [wrc, wout.strip()[:60]]})
    return out, tally
