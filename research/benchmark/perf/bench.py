#!/usr/bin/env python3
"""Native/wasm runtime scoreboard for the perf suite (#917).

Builds every benchmark on the requested legs, verifies output equivalence
across all variants (small workload), then wall-clock times the canonical
workload with interleaved runs and writes a dated JSON results file.

Legs:
  native — `almide build --release` (cargo release profile: opt-level=3, LTO)
  wasm   — `almide build --target wasm`, executed by the wasmtime CLI
  rust   — handwritten references in rust-ref/, compiled with the same
           rustc flags the native leg uses (opt-level=3, lto, 1 CGU)

Usage:
  python3 bench.py                    # full suite, all legs, 5 runs
  python3 bench.py --runs 7 --label m4pro
  python3 bench.py --legs native,rust --quick   # ratchet workloads
"""

import argparse
import datetime
import json
import os
import platform
import shutil
import subprocess
import sys
import tempfile
import time

HERE = os.path.dirname(os.path.abspath(__file__))

# (name, almd source, rust refs, timing arg, verify arg, verify mode, legs)
# verify mode: "bytes" = full stdout must match across every variant;
# "line1" = only the first line is deterministic (fft self-times on line 2).
# legs: None = every requested leg; otherwise restrict this row.
# fft is split into two rows: the wasm leg currently degrades so hard on the
# hot `data[i] = x` list writes (~3,000x at 2^18) that the canonical 2^22
# workload would never finish — the wasm row runs 2^18, the native-vs-rust
# comparison runs 2^22.
SUITE = [
    ("nbody",        "nbody/nbody.almd",                 ["nbody.rs", "nbody_unrolled.rs"], "50000000", "1000", "bytes", None),
    ("spectralnorm", "spectralnorm/spectralnorm.almd",   ["spectralnorm.rs"],               "5500",     "100",  "bytes", None),
    ("fasta",        "fasta/fasta.almd",                 ["fasta.rs"],                      "25000000", "1000", "bytes", None),
    ("fft",          "fft/fft.almd",                     ["fft.rs"],                        "22",       "10",   "line1", ["native", "rust"]),
    ("fft-wasm",     "fft/fft.almd",                     ["fft.rs"],                        "18",       "10",   "line1", ["native", "wasm", "rust"]),
    ("fannkuchredux","fannkuchredux/fannkuchredux.almd", [],                                "11",       "7",    "bytes", None),
    # onebrc writes/reads a measurements file; the wasm leg has no preopened
    # dir under `wasmtime run` so the row is native/rust only.
    ("onebrc",       "onebrc/onebrc.almd",               ["onebrc.rs"],                     "10000000", "50000", "bytes", ["native", "rust"]),
    ("binarytrees",  "binarytrees/binarytrees.almd",     [],                                "18",       "10",   "bytes", None),
    ("mandelbrot",   "mandelbrot/mandelbrot.almd",       [],                                "4000",     "200",  "bytes", None),
]

QUICK_ARGS = {  # small workloads for the CI ratchet: seconds, not minutes.
    # Gated rows must stay above ~0.08s median or process-spawn noise
    # dominates the ratio — check-perf-ratio.sh enforces that floor.
    "nbody": "10000000",
    "spectralnorm": "2500",
    "fasta": "2500000",
    "fft": "22",
    "fft-wasm": "16",
    "fannkuchredux": "9",
    "onebrc": "1000000",
    "binarytrees": "14",
    "mandelbrot": "1000",
}

RUSTC_FLAGS = ["-C", "opt-level=3", "-C", "lto=yes", "-C", "codegen-units=1",
               "-C", "overflow-checks=no", "--edition", "2021"]


def find_tool(name, extra_dirs=("/opt/homebrew/bin", "/usr/local/bin")):
    path = shutil.which(name)
    if path:
        return path
    for d in extra_dirs:
        cand = os.path.join(d, name)
        if os.path.exists(cand):
            return cand
    return None


def run(cmd, **kw):
    r = subprocess.run(cmd, capture_output=True, **kw)
    if r.returncode != 0:
        sys.exit(f"FAILED ({r.returncode}): {' '.join(map(str, cmd))}\n{r.stderr.decode()[:2000]}")
    return r


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--runs", type=int, default=5)
    ap.add_argument("--legs", default="native,wasm,rust")
    ap.add_argument("--label", default=platform.machine())
    ap.add_argument("--quick", action="store_true", help="small workloads (CI ratchet)")
    ap.add_argument("--bench", default=None, help="comma-separated subset of bench names")
    ap.add_argument("--out", default=None, help="results JSON path (default: results/<date>-<label>.json)")
    args = ap.parse_args()
    legs = args.legs.split(",")

    almide = os.environ.get("ALMIDE_BIN") or find_tool("almide") or sys.exit("almide not on PATH")
    rustc = find_tool("rustc") or sys.exit("rustc not on PATH")
    wasmtime = find_tool("wasmtime")
    if "wasm" in legs and not wasmtime:
        sys.exit("wasmtime not found (needed for the wasm leg); install it or drop --legs wasm")

    suite = SUITE
    if args.bench:
        keep = set(args.bench.split(","))
        suite = [b for b in SUITE if b[0] in keep]

    work = tempfile.mkdtemp(prefix="almide-perf-")
    variants = {}  # bench -> [(variant_name, argv_prefix)]

    print(f"== build ({work})")
    for name, almd, refs, _, _, _, row_legs in suite:
        src = os.path.join(HERE, almd)
        row = [l for l in legs if row_legs is None or l in row_legs]
        vs = []
        if "native" in row:
            out = os.path.join(work, f"{name}_native")
            run([almide, "build", src, "--release", "-o", out])
            vs.append((f"{name}/native", [out]))
        if "wasm" in row:
            out = os.path.join(work, f"{name}.wasm")
            run([almide, "build", src, "--target", "wasm", "-o", out])
            vs.append((f"{name}/wasm", [wasmtime, "run", out]))
        if "rust" in row:
            for ref in refs:
                stem = ref[:-3]
                out = os.path.join(work, f"{name}_{stem}_ref")
                run([rustc, *RUSTC_FLAGS, os.path.join(HERE, "rust-ref", ref), "-o", out])
                vs.append((f"{name}/rust:{stem}", [out]))
        variants[name] = vs
        print(f"  {name}: {len(vs)} variant(s)")

    print("== verify (small workload, output equivalence across variants)")
    for name, _, _, _, verify_arg, mode, _ in suite:
        outs = {}
        for vname, argv in variants[name]:
            r = run(argv + [verify_arg])
            out = r.stdout
            if mode == "line1":
                out = out.split(b"\n", 1)[0]
            outs[vname] = out
        ref = None
        for vname, out in outs.items():
            if ref is None:
                ref = out
            elif out != ref:
                sys.exit(f"OUTPUT MISMATCH: {vname} differs on {name} (arg {verify_arg})")
        print(f"  {name}: {len(outs)} variant(s) agree ({mode})")

    print(f"== time (interleaved, warmup + {args.runs} runs)")
    results = {}
    for name, _, _, timing_arg, _, _, _ in suite:
        arg = QUICK_ARGS[name] if args.quick else timing_arg
        times = {vname: [] for vname, _ in variants[name]}
        for vname, argv in variants[name]:  # warmup
            subprocess.run(argv + [arg], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        for _ in range(args.runs):
            for vname, argv in variants[name]:
                t0 = time.perf_counter()
                r = subprocess.run(argv + [arg], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
                dt = time.perf_counter() - t0
                if r.returncode != 0:
                    sys.exit(f"RUN FAILED: {vname} {arg}")
                times[vname].append(dt)
        results[name] = {
            "arg": arg,
            "variants": {
                vname: {
                    "min": round(min(ts), 4),
                    "median": round(sorted(ts)[len(ts) // 2], 4),
                    "mean": round(sum(ts) / len(ts), 4),
                    "runs": [round(t, 4) for t in ts],
                }
                for vname, ts in times.items()
            },
        }
        line = "  ".join(f"{v}={st['median']}s" for v, st in results[name]["variants"].items())
        print(f"  {name}({arg}): {line}")

    meta = {
        "date": datetime.date.today().isoformat(),
        "label": args.label,
        "host": platform.platform(),
        "cpu": subprocess.run(["sysctl", "-n", "machdep.cpu.brand_string"],
                              capture_output=True, text=True).stdout.strip()
               if sys.platform == "darwin" else platform.processor(),
        "rustc": subprocess.run([rustc, "--version"], capture_output=True, text=True).stdout.strip(),
        "almide": subprocess.run([almide, "--version"], capture_output=True, text=True).stdout.strip(),
        "wasmtime": subprocess.run([wasmtime, "--version"], capture_output=True, text=True).stdout.strip()
                    if wasmtime else None,
        "runs": args.runs,
        "quick": args.quick,
    }
    out_path = args.out or os.path.join(
        HERE, "results", f"{meta['date']}-{args.label}{'-quick' if args.quick else ''}.json")
    os.makedirs(os.path.dirname(out_path), exist_ok=True)
    with open(out_path, "w") as f:
        json.dump({"meta": meta, "results": results}, f, indent=2)
    print(f"\nwritten: {out_path}")

    # Markdown table (median seconds) for pasting into docs
    cols = sorted({v.split("/", 1)[1] for r in results.values() for v in r["variants"]})
    print("\n| Benchmark (arg) | " + " | ".join(cols) + " |")
    print("|---|" + "---:|" * len(cols))
    for name, r in results.items():
        by_col = {v.split("/", 1)[1]: st["median"] for v, st in r["variants"].items()}
        cells = [f"{by_col[c]}s" if c in by_col else "—" for c in cols]
        print(f"| {name} ({r['arg']}) | " + " | ".join(cells) + " |")
    shutil.rmtree(work, ignore_errors=True)


if __name__ == "__main__":
    main()
