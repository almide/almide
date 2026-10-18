#!/usr/bin/env python3
"""Compare #2098 spellings; fail on output divergence or an explicit ratio limit."""
import argparse
import json
import os
from pathlib import Path
import statistics
import subprocess
import sys
import tempfile
import time


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--almide", default="target/release/almide")
    parser.add_argument("--n", type=int, default=1200)
    parser.add_argument("--runs", type=int, default=9)
    parser.add_argument("--max-ratio", type=float)
    parser.add_argument("--profile-pair", action="store_true",
                        help="native leg only: build every spelling at BOTH profiles and time "
                             "the six artifacts round-robin; each spelling's ratio is its "
                             "release median over its default median (#3604)")
    parser.add_argument("--max-release-ratio", type=float,
                        help="with --profile-pair: fail when a spelling's --release build is "
                             "slower than this multiple of its default build")
    parser.add_argument("--release", action="store_true",
                        help="build with --release (opt-level 3 + LTO) instead of the "
                             "default profile `almide build` uses (opt-level 1)")
    args = parser.parse_args()
    if args.n < 1 or args.runs < 1:
        parser.error("--n and --runs must be positive")
    compiler = str(Path(args.almide).resolve())
    sources = Path(__file__).resolve().parent
    env = os.environ.copy()
    for name in ("ALMIDE_WASM_SKIP_STOCK_AUDIT",
                 "ALMIDE_FUEL_PROBE", "ALMIDE_COMPONENT_P3", "ALMIDE_COMPONENT_ADAPTER"):
        env.pop(name, None)
    if args.profile_pair:
        return profile_pair(args, compiler, sources, env)
    result = {"n": args.n, "runs": args.runs,
              "profile": "release" if args.release else "default", "targets": {}}
    expected = None
    failed = False
    with tempfile.TemporaryDirectory(prefix="almide-spectralnorm-") as temporary:
        for target in ("rust", "wasm"):
            programs = {}
            for spelling in ("imperative", "indexed", "enumerate"):
                suffix = "" if spelling == "imperative" else "_" + spelling
                artifact = Path(temporary) / (spelling + (".wasm" if target == "wasm" else ".bin"))
                build = [compiler, "build", str(sources / f"spectralnorm{suffix}.almd"),
                         "--target", target, "-o", str(artifact)]
                if args.release:
                    build.append("--release")
                subprocess.run(build, env=env, check=True,
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
                        # A spelling that gets faster by computing something
                        # else must not buy a green: say what diverged, on
                        # stderr, without a traceback the reader has to parse.
                        print(f"output divergence: {target}/{spelling} answered {observable!r}, "
                              f"the imperative spelling answered {expected!r}", file=sys.stderr)
                        return 1
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


def profile_pair(args, compiler, sources, env):
    """#3604: opt-level 3 + LTO slower than opt-level 1 on the SAME source is a
    pessimization whatever the spelling. A ratio between spellings cannot see
    one that slows every spelling alike (aarch64 2026-10-12: all three at
    ~77 ms under --release against 54 ms at the default profile), so this
    compares each spelling with itself across the two profiles, in one
    round-robin run that cancels the runner the same way."""
    spellings = ("imperative", "indexed", "enumerate")
    expected = None
    with tempfile.TemporaryDirectory(prefix="almide-spectralnorm-pair-") as temporary:
        programs = {}
        for spelling in spellings:
            suffix = "" if spelling == "imperative" else "_" + spelling
            for profile in ("default", "release"):
                artifact = Path(temporary) / f"{spelling}-{profile}.bin"
                build = [compiler, "build", str(sources / f"spectralnorm{suffix}.almd"),
                         "--target", "rust", "-o", str(artifact)]
                if profile == "release":
                    build.append("--release")
                subprocess.run(build, env=env, check=True,
                               stdout=subprocess.PIPE, stderr=subprocess.PIPE)
                programs[(spelling, profile)] = [str(artifact), str(args.n)]
        samples = {key: [] for key in programs}
        for iteration in range(args.runs + 1):
            for key, command in programs.items():
                start = time.perf_counter()
                output = subprocess.run(command, env=env, check=True, capture_output=True)
                elapsed = time.perf_counter() - start
                observable = (output.stdout, output.stderr)
                if expected is None:
                    expected = observable
                if observable != expected:
                    print(f"output divergence: rust/{key[0]} at {key[1]} answered {observable!r}, "
                          f"the first artifact answered {expected!r}", file=sys.stderr)
                    return 1
                if iteration:
                    samples[key].append(elapsed)
    rows = {}
    failed = False
    for spelling in spellings:
        default = statistics.median(samples[(spelling, "default")])
        release = statistics.median(samples[(spelling, "release")])
        ratio = release / default
        rows[spelling] = {"default_ms": default * 1000, "release_ms": release * 1000, "ratio": ratio}
        failed |= args.max_release_ratio is not None and ratio > args.max_release_ratio
    print(json.dumps({"n": args.n, "runs": args.runs, "profile": "pair",
                      "spellings": rows, "stdout": expected[0].decode()}, indent=2))
    return 1 if failed else 0


if __name__ == "__main__":
    raise SystemExit(main())
