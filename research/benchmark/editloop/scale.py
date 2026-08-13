#!/usr/bin/env python3
"""EDIT-LOOP SCALE LADDER — `almide check` latency as a function of project size (#1334).

The README publishes `almide check` at 22.7ms on a 268-line file. That number is
real, but it does not establish the property the roadmap's axis A promises ("the
edit loop is scale-independent"): a fast check on a small file is also exactly what
a QUADRATIC compiler produces. The distinguishing measurement is the same command
run over a ladder of sizes, and the distinguishing statistic is the SLOPE.

THE CORPUS is this repository's own stdlib: 300 hand-written `.almd` modules,
~30k lines, byte-sorted, copied into a scratch project whose `main.almd` imports
them all. Nothing is synthesized — a generated 10k-line file has uniform shape and
would exercise one code path deeply instead of the spread real code produces. The
stdlib is the largest coherent Almide codebase that exists, it is written by hand
for real use, and (unlike `spec/`, whose independent test files declare colliding
top-level type and constructor names when composed into one program) it composes
into a single project that type-checks clean.

RUNGS are nested prefixes of that byte-sorted list at cumulative-line targets, so
rung k's sources are a subset of rung k+1's and the only variable is size. Rung 0
is the entry alone: the constant floor (process spawn + the auto-imported bundled
stdlib every check pays regardless of project size). Marginal time is measured
against that floor, which is what makes the slope a statement about the project's
lines rather than about the compiler's startup.

WHICH STATISTIC. Every rung is timed with the runs INTERLEAVED (round robin, not
rung-at-a-time) so a burst of machine load lands on every rung instead of on one.
Beyond that:

  * `min` is the least-contaminated observation and the one the ratchet uses.
  * `p50`/`p95` are what a human or an agent actually waits on, so they are what
    gets reported and published — but they are NOT gateable on a shared runner.
    Measured here on 2026-08-13 on one M4 Pro, same commit, same corpus: at load
    average 42 the top rung read p50 752ms / p95 1231ms against min 292ms, and
    the slope computed from p50 read 1.53 against 1.13 from min. The tail tracks
    the machine, the slope from `min` does not. See scripts/check-edit-loop-scale.sh.

Usage:
  python3 scale.py                       # default ladder, 40 interleaved runs
  python3 scale.py --runs 60 --out r.json
  python3 scale.py --targets 0,2000,10000  # custom cumulative-line rungs
"""

import argparse
import datetime
import glob
import json
import math
import os
import platform
import shutil
import subprocess
import sys
import tempfile
import time

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, "..", "..", ".."))

# Stdlib sources that do not stand alone as a user module: they are re-exported
# or intrinsic-only shims whose names the compiler resolves specially, so a
# renamed copy of them fails to check on its own. Excluded by name (not by
# "whatever fails today") so that a NEW failure is a gate failure rather than a
# silently shrinking corpus.
SKIP = {"args.almd", "html.almd", "path.almd"}

# Cumulative source-line targets for the rungs. 0 is the floor (entry only);
# 10000 is the roadmap's headline size; the last rung is the whole corpus.
DEFAULT_TARGETS = [0, 2000, 5000, 10000, 20000, 10 ** 9]


def corpus_files():
    """The byte-sorted stdlib sources that make up the ladder, with line counts."""
    paths = sorted(glob.glob(os.path.join(ROOT, "stdlib", "*.almd")))
    out = []
    for p in paths:
        if os.path.basename(p) in SKIP:
            continue
        with open(p, encoding="utf-8") as f:
            out.append((os.path.relpath(p, ROOT), f.read().count("\n")))
    return out


def build_rung(files, out_dir):
    """Materialize a project of `files` as modules m000..mNNN plus an entry."""
    src = os.path.join(out_dir, "src")
    shutil.rmtree(out_dir, ignore_errors=True)
    os.makedirs(src)
    total, names = 0, []
    for i, (rel, _) in enumerate(files):
        name = "m%03d" % i
        with open(os.path.join(ROOT, rel), encoding="utf-8") as f:
            text = f.read()
        total += text.count("\n")
        with open(os.path.join(src, name + ".almd"), "w", encoding="utf-8") as f:
            f.write(text)
        names.append(name)
    entry_text = "".join("import %s\n" % n for n in names)
    entry_text += '\nfn main() -> Unit = {\n  println("editloop-scale corpus")\n}\n'
    entry = os.path.join(src, "main.almd")
    with open(entry, "w", encoding="utf-8") as f:
        f.write(entry_text)
    return entry, total + entry_text.count("\n")


def pct(values, p):
    v = sorted(values)
    k = (len(v) - 1) * p / 100.0
    lo = int(k)
    hi = min(lo + 1, len(v) - 1)
    return v[lo] + (v[hi] - v[lo]) * (k - lo)


def loglog_slope(points):
    """Least-squares slope of log(marginal ms) against log(lines).

    1.0 is linear: doubling the project doubles the marginal check time. 2.0 is
    quadratic. Below 1.0 means the per-line cost falls with size (fixed work
    amortizing). This is a RATCHET INDEX, not a proof of complexity class — the
    rungs are real code, so later rungs also differ in what they contain, and
    that confound is held constant only because the corpus and its order are.
    """
    n = len(points)
    sx = sum(math.log(x) for x, _ in points)
    sy = sum(math.log(y) for _, y in points)
    sxx = sum(math.log(x) ** 2 for x, _ in points)
    sxy = sum(math.log(x) * math.log(y) for x, y in points)
    return (n * sxy - sx * sy) / (n * sxx - sx * sx)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--runs", type=int, default=40,
                    help="interleaved repetitions per rung (p95 of N=40 is the 38th sample)")
    ap.add_argument("--targets", default=None, help="comma-separated cumulative-line rung targets")
    ap.add_argument("--almide", default=os.path.join(ROOT, "target/release/almide"))
    ap.add_argument("--out", default=None, help="write JSON results here")
    ap.add_argument("--keep", action="store_true", help="keep the scratch corpus")
    args = ap.parse_args()

    if not os.path.exists(args.almide):
        sys.exit("no almide binary at %s — run `cargo build --release` first" % args.almide)

    targets = DEFAULT_TARGETS
    if args.targets:
        targets = [int(t) for t in args.targets.split(",")]

    files = corpus_files()
    workdir = tempfile.mkdtemp(prefix="almide-editloop-")
    rungs = []
    try:
        acc, idx = 0, 0
        for t in targets:
            while idx < len(files) and acc < t:
                acc += files[idx][1]
                idx += 1
            entry, lines = build_rung(files[:idx], os.path.join(workdir, "r%03d" % idx))
            probe = subprocess.run([args.almide, "check", entry], capture_output=True, text=True)
            if probe.returncode != 0:
                sys.stderr.write(probe.stdout[-4000:] + probe.stderr[-2000:])
                sys.exit("::error::editloop-scale: rung of %d modules / %d lines does NOT check "
                         "clean. The ladder measures a clean check; a corpus that errors measures "
                         "the diagnostic renderer. Fix the corpus (see SKIP in scale.py) rather "
                         "than timing a red build." % (idx, lines))
            rungs.append({"modules": idx, "lines": lines, "entry": entry, "samples": []})

        started = time.time()
        for _ in range(args.runs):
            for r in rungs:
                t0 = time.perf_counter()
                subprocess.run([args.almide, "check", r["entry"]], capture_output=True)
                r["samples"].append((time.perf_counter() - t0) * 1000.0)
        wall = time.time() - started
    finally:
        if not args.keep:
            shutil.rmtree(workdir, ignore_errors=True)

    floor = rungs[0]
    for r in rungs:
        s = r["samples"]
        r.update(min=min(s), p50=pct(s, 50), p95=pct(s, 95), max=max(s))
    for r in rungs:
        r["marginal_min"] = r["min"] - floor["min"]
        r["marginal_p50"] = r["p50"] - floor["p50"]
        r["us_per_line"] = (r["marginal_min"] * 1000.0 / r["lines"]) if r["lines"] else 0.0

    scaling = [(r["lines"], r["marginal_min"]) for r in rungs[1:] if r["marginal_min"] > 0]
    result = {
        "date": datetime.date.today().isoformat(),
        "machine": "%s %s" % (platform.machine(), platform.system()),
        "loadavg": list(os.getloadavg()),
        "runs": args.runs,
        "wall_seconds": round(wall, 1),
        "corpus": {"source": "stdlib/*.almd", "files": len(files),
                   "lines": sum(n for _, n in files), "skipped": sorted(SKIP)},
        "rungs": [{k: v for k, v in r.items() if k not in ("entry", "samples")} for r in rungs],
        "slope_min": loglog_slope(scaling) if len(scaling) >= 2 else None,
        "slope_p95": loglog_slope([(r["lines"], r["p95"] - floor["p95"])
                                   for r in rungs[1:] if r["p95"] - floor["p95"] > 0])
        if len(rungs) > 2 else None,
    }

    print("editloop-scale: corpus %s — %d files, %d lines (%d used, %s skipped)"
          % (result["corpus"]["source"], len(files) + len(SKIP), result["corpus"]["lines"],
             len(files), ",".join(sorted(SKIP))))
    print("editloop-scale: %s, load %.1f, %d interleaved runs per rung, %.1fs wall"
          % (result["machine"], result["loadavg"][0], args.runs, wall))
    print("%9s %8s %9s %9s %9s %9s %10s" % ("modules", "lines", "min", "p50", "p95", "max", "us/line"))
    for r in rungs:
        print("%9d %8d %9.1f %9.1f %9.1f %9.1f %10.2f"
              % (r["modules"], r["lines"], r["min"], r["p50"], r["p95"], r["max"], r["us_per_line"]))
    print("editloop-scale: log-log slope over the ladder — min %.3f, p95 %.3f (1.0 = linear)"
          % (result["slope_min"], result["slope_p95"]))

    if args.out:
        with open(args.out, "w", encoding="utf-8") as f:
            json.dump(result, f, indent=2)
        print("editloop-scale: wrote %s" % args.out)
    return 0


if __name__ == "__main__":
    sys.exit(main())
