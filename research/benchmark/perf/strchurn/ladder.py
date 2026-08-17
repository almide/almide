#!/usr/bin/env python3
"""The #1004 attribution ladder: remove ONE cost per rung and re-measure.

`bench.py` reports the strchurn ratio; this script explains it. It takes the
compiler's own `--target rust` output for `strchurn.almd` and replaces the
body of `__almide_main` per rung, leaving every runtime function
byte-identical, then compiles each rung with the exact rustc flags the
handwritten references use. Everything outside the replaced body is the same
in every rung, so the delta between two adjacent rungs IS the one cost the
later rung removed — no absolute attribution, no profiler, no counters.

The ladder is closed at both ends on purpose: rung `v3_fused_sum` should land
on `rust-ref/strchurn.rs` and rung `v5_borrowed_split` on
`rust-ref/strchurn_idiomatic.rs`. If it does not, the deltas do not account
for the gap and the table is a story rather than a measurement.

Usage:
  python3 research/benchmark/perf/strchurn/ladder.py
  ALMIDE_BIN=target/release/almide python3 .../ladder.py --n 4000000 --runs 21

Timing is interleaved and the reported statistic is the MIN of the runs: on a
shared box the median measures the load, not the binary (the same reasoning
`scripts/check-perf-ratio.sh` gives for its IDIOM_CEILING). Run it three
times and compare — the noise floor is a few ms.

Findings and the resulting work list: ../string-gap-1004.md
"""

import argparse
import os
import re
import shutil
import subprocess
import sys
import tempfile
import time

HERE = os.path.dirname(os.path.abspath(__file__))
PERF = os.path.dirname(HERE)
FLAGS = ["-C", "opt-level=3", "-C", "lto=yes", "-C", "codegen-units=1",
         "-C", "overflow-checks=no", "--edition", "2021"]

# A monomorphic twin of `almide_rt_list_map` — same body, `impl Fn` instead of
# `Rc<dyn Fn>`. Isolating the boxing is the whole reason it exists.
HELPER = """
fn __map_static<A, B>(xs: Vec<A>, f: impl Fn(A) -> B) -> Vec<B> {
    xs.into_iter().map(f).collect()
}
"""

PRELUDE = """    let mut args: Vec<String> = almide_rt_env_args();
    let n: i64 = match almide_rt_list_get(&args, 0i64) {
        Some(s) => match almide_rt_int_parse(&*s) { Ok(v) => v, Err(_) => 1000000i64 },
        None => 1000000i64,
    };
"""

BUILD_DYN = ("    let mut parts: Vec<String> = almide_rt_list_map(almide_rt_list_range(0i64, n), "
             "(std::rc::Rc::new(move |i: i64| almide_rt_int_to_string(i)) "
             "as std::rc::Rc<dyn Fn(i64) -> String>));\n")
BUILD_STATIC = ("    let mut parts: Vec<String> = __map_static(almide_rt_list_range(0i64, n), "
                "move |i: i64| almide_rt_int_to_string(i));\n")
BUILD_LAZY = "    let mut parts: Vec<String> = (0i64..n).map(|i| almide_rt_int_to_string(i)).collect();\n"

JOIN = '    let joined: String = almide_rt_string_join(&parts, ",");\n'

SPLIT = '    let mut back: Vec<String> = almide_rt_string_split(&*joined, ",");\n'
FOLD = ".fold(0i64, |a, b| a.wrapping_add(b));\n"  # == almide_rt_list_sum's body

SUM_DYN = SPLIT + ("    let total: i64 = almide_rt_list_sum(&almide_rt_list_map(back, "
                   "(std::rc::Rc::new(move |s: String| almide_rt_string_len(&*s)) "
                   "as std::rc::Rc<dyn Fn(String) -> i64>)));\n")
SUM_STATIC = SPLIT + ("    let total: i64 = almide_rt_list_sum(&__map_static(back, "
                      "move |s: String| almide_rt_string_len(&*s)));\n")
SUM_FUSED = SPLIT + ("    let total: i64 = back.into_iter()"
                     ".map(|s| almide_rt_string_len(&*s))" + FOLD)
SUM_BYTELEN = SPLIT + "    let total: i64 = back.into_iter().map(|s| s.len() as i64)" + FOLD
SUM_LENVEC = ("    let lens: Vec<i64> = joined.split(',').map(|s| s.len() as i64).collect();\n"
              "    let total: i64 = lens.into_iter()" + FOLD)
SUM_BORROW = "    let total: i64 = joined.split(',').map(|s| s.len() as i64)" + FOLD

TAIL_CHARS = ('    Ok::<(), String>(println!("{}", format!("n: {} chars: {} sum: {}", n, '
              'almide_rt_string_len(&*joined), total)))\n')
TAIL_BYTES = ('    Ok::<(), String>(println!("{}", format!("n: {} chars: {} sum: {}", n, '
              'joined.len() as i64, total)))\n')

RUNGS = [
    ("v0_emitted",        BUILD_DYN,    SUM_DYN,     TAIL_CHARS, "(compiler output verbatim)"),
    ("v1_static_closure", BUILD_STATIC, SUM_STATIC,  TAIL_CHARS, "Rc<dyn Fn> boxed closure per element"),
    ("v2_lazy_range",     BUILD_LAZY,   SUM_STATIC,  TAIL_CHARS, "list.range's materialized Vec<i64>"),
    ("v3_fused_sum",      BUILD_LAZY,   SUM_FUSED,   TAIL_CHARS, "map->sum Vec<i64> intermediate"),
    ("v4_byte_len",       BUILD_LAZY,   SUM_BYTELEN, TAIL_BYTES, "string.len = chars().count() scan"),
    ("v4c_len_vec",       BUILD_LAZY,   SUM_LENVEC,  TAIL_BYTES, "split's N owned String allocations"),
    ("v5_borrowed_split", BUILD_LAZY,   SUM_BORROW,  TAIL_BYTES, "the remaining Vec materialization"),
]

REFS = [("ref_same_shape", "strchurn.rs"), ("ref_idiomatic", "strchurn_idiomatic.rs")]


def run(cmd, **kw):
    r = subprocess.run(cmd, capture_output=True, **kw)
    if r.returncode != 0:
        sys.exit(f"FAILED: {' '.join(map(str, cmd))}\n{r.stderr.decode()[:3000]}")
    return r


def build(work, almide, rustc):
    """Emit, patch, compile. Returns [(label, binary, removes)] in ladder order."""
    emitted = run([almide, os.path.join(HERE, "strchurn.almd"), "--target", "rust"]).stdout.decode()
    m = re.search(r"pub fn __almide_main\(\) -> Result<\(\), String> \{\n.*?\n\}\n", emitted, re.S)
    if not m:
        sys.exit("could not locate __almide_main in the emitted source — did codegen change shape?")
    out = []
    for name, parts, total, tail, removes in RUNGS:
        body = ("pub fn __almide_main() -> Result<(), String> {\n"
                + PRELUDE + parts + JOIN + total + tail + "}\n")
        path = os.path.join(work, f"{name}.rs")
        with open(path, "w") as f:
            f.write(emitted[:m.start()] + HELPER + body + emitted[m.end():])
        binp = os.path.join(work, name)
        run([rustc, *FLAGS, path, "-o", binp])
        out.append((name, binp, removes))
    for label, src in REFS:
        binp = os.path.join(work, label)
        run([rustc, *FLAGS, os.path.join(PERF, "rust-ref", src), "-o", binp])
        out.append((label, binp, "— handwritten reference"))
    return out


def verify(rungs, arg="200000"):
    outs = {label: run([binp, arg]).stdout for label, binp, _ in rungs}
    ref = outs[rungs[0][0]]
    bad = [k for k, v in outs.items() if v != ref]
    if bad:
        sys.exit(f"OUTPUT MISMATCH on {bad} — a rung changed the program, not its cost")
    print(f"verify: {len(outs)} variants agree ({ref.decode().strip()})")


def measure(rungs, n, runs):
    times = {label: [] for label, _, _ in rungs}
    for _, binp, _ in rungs:
        subprocess.run([binp, n], stdout=subprocess.DEVNULL)
    for _ in range(runs):
        for label, binp, _ in rungs:
            t0 = time.perf_counter()
            subprocess.run([binp, n], stdout=subprocess.DEVNULL)
            times[label].append(time.perf_counter() - t0)
    return {label: min(ts) * 1000 for label, ts in times.items()}


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--n", default="4000000")
    ap.add_argument("--runs", type=int, default=21)
    args = ap.parse_args()

    almide = os.environ.get("ALMIDE_BIN") or shutil.which("almide") or sys.exit("almide not on PATH")
    rustc = shutil.which("rustc") or sys.exit("rustc not on PATH")

    work = tempfile.mkdtemp(prefix="almide-1004-")
    try:
        rungs = build(work, almide, rustc)
        verify(rungs)
        mins = measure(rungs, args.n, args.runs)

        ladder = [r for r in rungs if r[0].startswith("v")]
        total = mins[ladder[0][0]] - mins["ref_idiomatic"]
        print(f"\nN={args.n} runs={args.runs} (min of runs, interleaved)\n")
        print(f"{'rung':22s} {'min ms':>8s} {'delta':>8s} {'share':>7s}  cost removed")
        prev = None
        for label, _, removes in ladder:
            ms = mins[label]
            if prev is None:
                print(f"{label:22s} {ms:8.1f} {'':>8s} {'':>7s}  {removes}")
            else:
                d = prev - ms
                print(f"{label:22s} {ms:8.1f} {d:8.1f} {d / total * 100:6.1f}%  {removes}")
            prev = ms
        for label in ("ref_same_shape", "ref_idiomatic"):
            print(f"{label:22s} {mins[label]:8.1f}")
        print(f"\ntotal gap (v0 - idiomatic ref): {total:.1f} ms "
              f"({mins[ladder[0][0]] / mins['ref_idiomatic']:.2f}x)")
        print(f"v3 vs same-shape ref: {mins['v3_fused_sum'] - mins['ref_same_shape']:+.1f} ms; "
              f"v5 vs idiomatic ref: {mins['v5_borrowed_split'] - mins['ref_idiomatic']:+.1f} ms "
              "(both should be within the noise floor, or the deltas do not add up to the gap)")
    finally:
        shutil.rmtree(work, ignore_errors=True)


if __name__ == "__main__":
    main()
