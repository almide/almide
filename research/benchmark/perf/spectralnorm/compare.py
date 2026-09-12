#!/usr/bin/env python3
"""Compare #2098 spellings; fail on output divergence or an explicit ratio limit."""
import argparse
import json
import os
from pathlib import Path
import statistics
import subprocess
import tempfile
import time


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--almide", default="target/release/almide")
    parser.add_argument("--n", type=int, default=1200)
    parser.add_argument("--runs", type=int, default=9)
    parser.add_argument("--max-ratio", type=float)
    args = parser.parse_args()
    if args.n < 1 or args.runs < 1:
        parser.error("--n and --runs must be positive")
    compiler = str(Path(args.almide).resolve())
    sources = Path(__file__).resolve().parent
    env = os.environ.copy()
    for name in ("ALMIDE_WASM_STRUCTURAL", "ALMIDE_WASM_INCUMBENT",
                 "ALMIDE_FUEL_PROBE", "ALMIDE_COMPONENT_P3", "ALMIDE_COMPONENT_ADAPTER"):
        env.pop(name, None)
    result = {"n": args.n, "runs": args.runs, "targets": {}}
    expected = None
    failed = False
    with tempfile.TemporaryDirectory(prefix="almide-spectralnorm-") as temporary:
        for target in ("rust", "wasm"):
            programs = {}
            for spelling in ("imperative", "indexed", "enumerate"):
                suffix = "" if spelling == "imperative" else "_" + spelling
                artifact = Path(temporary) / (spelling + (".wasm" if target == "wasm" else ".bin"))
                subprocess.run([compiler, "build", str(sources / f"spectralnorm{suffix}.almd"),
                                "--target", target, "-o", str(artifact)], env=env, check=True,
                               stdout=subprocess.PIPE, stderr=subprocess.PIPE)
                command = (["wasmtime", "run"] if target == "wasm" else []) + [str(artifact), str(args.n)]
                programs[spelling] = (command, artifact.stat().st_size)
            samples = {name: [] for name in programs}
            # One warmup, then round-robin sampling to reduce order/thermal bias.
            for iteration in range(args.runs + 1):
                for spelling, (command, _) in programs.items():
                    start = time.perf_counter()
                    output = subprocess.run(command, env=env, check=True, capture_output=True)
                    elapsed = time.perf_counter() - start
                    observable = (output.stdout, output.stderr)
                    if expected is None:
                        expected = observable
                    if observable != expected:
                        raise RuntimeError(f"output divergence: {target}/{spelling}: {observable!r} != {expected!r}")
                    if iteration:
                        samples[spelling].append(elapsed)
            baseline = statistics.median(samples["imperative"])
            rows = {}
            for spelling, times in samples.items():
                median = statistics.median(times)
                ratio = median / baseline
                rows[spelling] = {"median_ms": median * 1000, "ratio": ratio,
                                  "bytes": programs[spelling][1], "samples_ms": [t * 1000 for t in times]}
                failed |= args.max_ratio is not None and ratio > args.max_ratio
            result["targets"][target] = rows
    result["stdout"] = expected[0].decode()
    print(json.dumps(result, indent=2))
    return 1 if failed else 0


if __name__ == "__main__":
    raise SystemExit(main())
