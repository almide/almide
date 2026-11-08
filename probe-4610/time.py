"""Alternating wall-clock A/B of wasm modules under wasmtime (one runner).
usage: time.py MODE ROUNDS label=path ...   (MODE: jit | aot)"""
import statistics, subprocess, sys, time

mode, rounds = sys.argv[1], int(sys.argv[2])
variants = dict(a.split("=", 1) for a in sys.argv[3:])


def cmd(p):
    return ["wasmtime", "run", "--allow-precompiled", p] if mode == "aot" else ["wasmtime", "run", p]


outs = {}
for k, p in variants.items():
    for _ in range(2):  # warmup
        outs[k] = subprocess.run(cmd(p), capture_output=True, text=True).stdout.strip()
times = {k: [] for k in variants}
for i in range(rounds):
    order = list(variants) if i % 2 == 0 else list(reversed(list(variants)))
    for k in order:
        t = time.perf_counter()
        subprocess.run(cmd(variants[k]), capture_output=True)
        times[k].append((time.perf_counter() - t) * 1000)
for k in variants:
    ts = times[k]
    print(f"{mode}\t{k}\tmedian_ms={statistics.median(ts):.2f}\tmin_ms={min(ts):.2f}\tmax_ms={max(ts):.2f}\tout={outs[k]}")
