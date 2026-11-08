#!/usr/bin/env python3
"""Time wasm modules on one runner, alternating, median of N after warmup.

usage: timeit.py ROUNDS NAME=WASM [NAME=WASM ...]
The first module is the reference every ratio is taken against. Each module
is AOT-compiled once (`wasmtime compile`), then each round runs every module
once, the order rotated per round. Every run's stdout must equal the
reference's.
"""
import os, statistics, subprocess, sys, time

WT = os.environ.get("WASMTIME", "wasmtime")


def main():
    rounds = int(sys.argv[1])
    mods = [a.split("=", 1) for a in sys.argv[2:]]
    cw = {}
    for name, path in mods:
        out = path + ".cwasm"
        subprocess.run([WT, "compile", path, "-o", out], check=True)
        cw[name] = out
    names = [n for n, _ in mods]
    ref_out = None
    times = {n: [] for n in names}
    for r in range(rounds + 2):
        k = r % len(names)
        order = names[k:] + names[:k]
        for n in order:
            t0 = time.perf_counter()
            p = subprocess.run([WT, "run", "--allow-precompiled", cw[n]], capture_output=True)
            dt = time.perf_counter() - t0
            if p.returncode != 0:
                sys.exit("%s exited %d: %s" % (n, p.returncode, p.stderr[:400]))
            if ref_out is None:
                ref_out = p.stdout
            elif p.stdout != ref_out:
                sys.exit("%s printed %r, reference printed %r" % (n, p.stdout, ref_out))
            if r >= 2:
                times[n].append(dt * 1000)
    ref = statistics.median(times[names[0]])
    print("stdout: %r" % ref_out)
    print("%-14s %10s %10s %10s %8s" % ("module", "median ms", "min ms", "max ms", "ratio"))
    for n in names:
        m = statistics.median(times[n])
        print("%-14s %10.2f %10.2f %10.2f %8.4f" % (n, m, min(times[n]), max(times[n]), m / ref))
    print("raw:")
    for n in names:
        print(n, " ".join("%.1f" % x for x in times[n]))


if __name__ == "__main__":
    main()
