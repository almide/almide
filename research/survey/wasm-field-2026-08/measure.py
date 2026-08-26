#!/usr/bin/env python3
"""wasm field expansion measurement driver (2026-08).

Measures, for every (lane, kernel):
  1. run time     — stock wasmtime CLI, 1 warmup + 5 timed runs; best AND median,
                    raw and empty-baseline-deducted (same lane, same runner config)
  2. compile time — CLI end-to-end, warm caches, leaf source touched before every
                    rep to defeat no-op caching; 1 warmup + 5 timed reps
  3. size         — standalone .wasm bytes
  4. peak RSS     — /usr/bin/time -l around the run, "maximum resident set size"
  5. portability  — runs on the stock wasmtime CLI with default flags: yes/no

Output correctness is asserted on EVERY timed run (integer kernels byte-compare;
float_math compares as parsed f64). A lane/kernel that traps at stock defaults is
recorded as-is; if a documented non-default runner config lets it run (bigger
wasm stack), a reference measurement under that config is recorded with the
config named in the row.

Writes out/results.json and out/results.md. Re-runnable; run setup.sh first.
"""

import json
import os
import statistics
import subprocess
import sys
import time
from pathlib import Path

SURVEY = Path(__file__).resolve().parent
OUT = SURVEY / "out"
HOME = Path.home()
BREW = "/opt/homebrew/bin"
WASMTIME = f"{BREW}/wasmtime"
BIGSTACK = ["-W", "max-wasm-stack=1073741824"]  # 1 GiB, for stack-exhaustion reference rows

KERNELS = ["int_loop", "float_math", "str_build", "recursion",
           "list_sort", "sort_by", "list_pipeline"]
EMPTY = "empty"

EXPECTED = {
    "int_loop": "908565",
    "float_math": "30.250749144754316",   # compared as parsed f64
    "str_build": "11700000",
    "recursion": "4590",
    "list_sort": "3001800",
    "sort_by": "3001800",
    "list_pipeline": "102034",
    EMPTY: "",
}

ENV_BASE = dict(os.environ, PATH=f"{BREW}:{os.environ['PATH']}")
ENV_TINYGO = dict(ENV_BASE,
                  PATH=f"/opt/homebrew/opt/go@1.26/bin:{ENV_BASE['PATH']}",
                  GOROOT="/opt/homebrew/opt/go@1.26/libexec")
ENV_GO = dict(ENV_BASE, GOOS="wasip1", GOARCH="wasm")
ENV_MOON = dict(ENV_BASE, PATH=f"{HOME}/.moon/bin:{ENV_BASE['PATH']}")

RUST_SYSROOT = OUT / "rust-sysroot-overlay"


def lane_defs():
    """lane -> dict(compile=fn(k)->(cmd, cwd, env, touch_path), wasm=fn(k)->path)"""
    def rust(k):
        return (["rustc", "--sysroot", str(RUST_SYSROOT), "--target", "wasm32-wasip1",
                 "-C", "opt-level=3", f"src/rust/{k}.rs", "-o", f"out/rust/{k}.wasm"],
                SURVEY, ENV_BASE, SURVEY / f"src/rust/{k}.rs")

    def go(k):
        return (["go", "build", "-o", f"{OUT}/go/{k}.wasm", f"{k}.go"],
                SURVEY / "src/go", ENV_GO, SURVEY / f"src/go/{k}.go")

    def tinygo(k):
        return (["tinygo", "build", "-target=wasip1", "-opt=2",
                 "-o", f"{OUT}/tinygo/{k}.wasm", f"{k}.go"],
                SURVEY / "src/tinygo", ENV_TINYGO, SURVEY / f"src/tinygo/{k}.go")

    def asc(k):
        return (["./node_modules/.bin/asc", f"{k}.ts",
                 "--config", "node_modules/@assemblyscript/wasi-shim/asconfig.json",
                 "-O3", "-o", f"{OUT}/assemblyscript/{k}.wasm"],
                SURVEY / "src/assemblyscript", ENV_BASE, SURVEY / f"src/assemblyscript/{k}.ts")

    def grain(k):
        return (["grain", "compile", "--release",
                 "-o", f"{OUT}/grain/{k}.wasm", f"{k}.gr"],
                SURVEY / "src/grain", ENV_BASE, SURVEY / f"src/grain/{k}.gr")

    def moon(k):
        return (["moon", "build", "--target", "wasm", "--release"],
                SURVEY / "src/moonbit", ENV_MOON, SURVEY / f"src/moonbit/{k}/main.mbt")

    def kotlin(k):
        return (["gradle", "--daemon", f":{k}:build", "-x", "check"],
                SURVEY / "src/kotlin", ENV_BASE,
                SURVEY / f"src/kotlin/{k}/src/wasmWasiMain/kotlin/Main.kt")

    def almide(k):
        return ([str(SURVEY / "tools/emit-only/target/release/almide-emit-only"),
                 f"src/almide/{k}.almd", f"out/almide/{k}.wasm"],
                SURVEY, ENV_BASE, SURVEY / f"src/almide/{k}.almd")

    return {
        "almide": dict(compile=almide, wasm=lambda k: OUT / f"almide/{k}.wasm"),
        "rust": dict(compile=rust, wasm=lambda k: OUT / f"rust/{k}.wasm"),
        "moonbit": dict(compile=moon,
                        wasm=lambda k: SURVEY / f"src/moonbit/_build/wasm/release/build/{k}/{k}.wasm"),
        "grain": dict(compile=grain, wasm=lambda k: OUT / f"grain/{k}.wasm"),
        "assemblyscript": dict(compile=asc, wasm=lambda k: OUT / f"assemblyscript/{k}.wasm"),
        "tinygo": dict(compile=tinygo, wasm=lambda k: OUT / f"tinygo/{k}.wasm"),
        "go": dict(compile=go, wasm=lambda k: OUT / f"go/{k}.wasm"),
        "kotlin": dict(compile=kotlin,
                       wasm=lambda k: SURVEY / f"src/kotlin/{k}/build/compileSync/wasmWasi/main/productionExecutable/optimized/wasm-field-kotlin-{k}.wasm"),
    }


def check_output(kernel, stdout):
    want = EXPECTED[kernel]
    got = stdout.strip()
    if kernel == "float_math":
        try:
            return float(got) == float(want), got
        except ValueError:
            return False, got
    return got == want, got


def timed(cmd, cwd, env, timeout=300):
    t0 = time.perf_counter()
    p = subprocess.run(cmd, cwd=cwd, env=env, capture_output=True, text=True, timeout=timeout)
    return time.perf_counter() - t0, p


def run_lane_kernel_runtime(wasm, kernel, extra_flags):
    """1 warmup + 5 timed runs; returns dict or trap info."""
    cmd = [WASMTIME, "run", *extra_flags, str(wasm)]
    t, p = timed(cmd, SURVEY, ENV_BASE)  # warmup / probe
    if p.returncode != 0:
        return dict(ok=False, error=(p.stderr.strip().splitlines() or ["?"])[-1])
    times = []
    for _ in range(5):
        t, p = timed(cmd, SURVEY, ENV_BASE)
        ok, got = check_output(kernel, p.stdout)
        if p.returncode != 0 or not ok:
            return dict(ok=False, error=f"wrong output or rc={p.returncode}: {got!r}")
        times.append(t)
    # peak RSS via /usr/bin/time -l (separate run)
    p = subprocess.run(["/usr/bin/time", "-l", *cmd], cwd=SURVEY, env=ENV_BASE,
                       capture_output=True, text=True, timeout=300)
    rss = None
    for line in p.stderr.splitlines():
        if "maximum resident set size" in line:
            rss = int(line.split()[0])
    return dict(ok=True, best=min(times), median=statistics.median(times), rss=rss)


def measure_compile(lane, defs, kernel):
    """1 warmup + 5 timed reps. The leaf source is invalidated by CONTENT
    (unique trailing comment), not mtime: go/gradle/moon build caches are
    content-hashed, so an mtime touch would hand them a no-op cache hit."""
    cmd, cwd, env, touch = defs["compile"](kernel)
    orig = touch.read_bytes()
    times = []
    try:
        for i in range(6):  # rep 0 = warmup
            touch.write_bytes(orig + f"// invalidate {time.time_ns()}\n".encode())
            t, p = timed(cmd, cwd, env, timeout=600)
            if p.returncode != 0:
                return dict(ok=False, error=(p.stderr.strip().splitlines() or ["?"])[-1])
            if i > 0:
                times.append(t)
    finally:
        touch.write_bytes(orig)
    return dict(ok=True, best=min(times), median=statistics.median(times))


def versions():
    def v(cmd, env=ENV_BASE, cwd=SURVEY):
        try:
            p = subprocess.run(cmd, capture_output=True, text=True, env=env, cwd=cwd, timeout=120)
            return (p.stdout + p.stderr).strip().splitlines()[0]
        except Exception as e:  # noqa: BLE001
            return f"unavailable: {e}"
    return {
        "wasmtime": v([WASMTIME, "--version"]),
        "almide": "greenfield 46e689518 (survey/wasm-field-2026-08 base)",
        "rustc": v(["rustc", "--version"]),
        "go": v(["go", "version"]),
        "tinygo": v(["tinygo", "version"], env=ENV_TINYGO),
        "assemblyscript": v(["./node_modules/.bin/asc", "--version"], cwd=SURVEY / "src/assemblyscript"),
        "node": v(["node", "--version"]),
        "grain": "grain " + v(["grain", "--version"]) + " (official mac-x64 binary under Rosetta 2)",
        "moon": v(["moon", "version"], env=ENV_MOON),
        "gradle": next((l.strip() for l in subprocess.run(
            ["gradle", "--version"], capture_output=True, text=True, env=ENV_BASE,
            timeout=300).stdout.splitlines() if l.strip().startswith("Gradle")), "?"),
        "kotlin": "2.3.21 (kotlin-multiplatform gradle plugin, wasmWasi target)",
        "wasm-opt": v([f"{BREW}/wasm-opt", "--version"]),
    }


def main():
    only_lanes = sys.argv[1:] or None
    defs_all = lane_defs()
    lanes = {k: v for k, v in defs_all.items() if not only_lanes or k in only_lanes}
    results = {"versions": versions(), "lanes": {}}
    prior = OUT / "results.json"
    if only_lanes and prior.exists():  # partial re-run keeps other lanes
        results = json.loads(prior.read_text())
        results["versions"] = versions()

    for lane, defs in lanes.items():
        print(f"=== lane: {lane}", flush=True)
        lr = {}
        # empty baseline first, per runner config
        base = {}
        for cfg_name, flags in [("stock", []), ("bigstack", BIGSTACK)]:
            c = measure_compile(lane, defs, EMPTY) if cfg_name == "stock" else None
            r = run_lane_kernel_runtime(defs["wasm"](EMPTY), EMPTY, flags)
            base[cfg_name] = dict(run=r, compile=c)
        lr["empty"] = dict(
            compile=base["stock"]["compile"],
            run_stock=base["stock"]["run"],
            run_bigstack=base["bigstack"]["run"],
            size=os.path.getsize(defs["wasm"](EMPTY)),
        )
        print(f"  empty: run={lr['empty']['run_stock']}", flush=True)

        for k in KERNELS:
            wasm = defs["wasm"](k)
            row = dict(size=os.path.getsize(wasm))
            row["compile"] = measure_compile(lane, defs, k)
            r = run_lane_kernel_runtime(wasm, k, [])
            row["runner"] = "wasmtime(stock)"
            row["portable_stock_wasmtime"] = r["ok"]
            if not r["ok"]:
                row["stock_error"] = r["error"]
                r2 = run_lane_kernel_runtime(wasm, k, BIGSTACK)
                if r2["ok"]:
                    row["runner"] = "wasmtime -W max-wasm-stack=1GiB (non-default)"
                    b = base["bigstack"]["run"]
                    r2["best_deducted"] = max(0.0, r2["best"] - b["best"])
                    r2["median_deducted"] = max(0.0, r2["median"] - b["median"])
                    row["run"] = r2
                else:
                    row["run"] = r2
            else:
                b = base["stock"]["run"]
                r["best_deducted"] = max(0.0, r["best"] - b["best"])
                r["median_deducted"] = max(0.0, r["median"] - b["median"])
                row["run"] = r
            lr[k] = row
            print(f"  {k}: {json.dumps(row)}", flush=True)
        results["lanes"][lane] = lr
        OUT.mkdir(exist_ok=True)
        prior.write_text(json.dumps(results, indent=1))

    render(results)
    print("wrote out/results.json, out/results.md")


def render(results):
    lines = ["# raw measurement tables (generated by measure.py)", ""]
    lines.append("versions:")
    for k, v in results["versions"].items():
        lines.append(f"- {k}: {v}")
    lines.append("")
    for metric, title in [("run", "run time (s, baseline-deducted, best / median)"),
                          ("compile", "compile time (s, warm, best / median)"),
                          ("size", "standalone .wasm size (bytes)"),
                          ("rss", "peak RSS of run (bytes)")]:
        lines.append(f"## {title}")
        lanes = list(results["lanes"])
        lines.append("| kernel | " + " | ".join(lanes) + " |")
        lines.append("|" + "---|" * (len(lanes) + 1))
        for k in KERNELS:
            cells = []
            for lane in lanes:
                row = results["lanes"][lane].get(k)
                if row is None:
                    cells.append("n/a")
                    continue
                if metric == "size":
                    cells.append(str(row["size"]))
                elif metric == "compile":
                    c = row["compile"]
                    cells.append(f"{c['best']:.3f} / {c['median']:.3f}" if c["ok"] else "FAIL")
                elif metric == "run":
                    r = row.get("run", {})
                    if r.get("ok"):
                        mark = "" if row["portable_stock_wasmtime"] else " (bigstack)"
                        cells.append(f"{r['best_deducted']:.3f} / {r['median_deducted']:.3f}{mark}")
                    else:
                        cells.append("TRAP")
                elif metric == "rss":
                    r = row.get("run", {})
                    cells.append(str(r.get("rss", "n/a")))
            lines.append(f"| {k} | " + " | ".join(cells) + " |")
        lines.append("")
    lines.append("## empty baselines (raw, stock runner)")
    lanes = list(results["lanes"])
    lines.append("| lane | run best/median (s) | compile best/median (s) | size (bytes) | rss (bytes) |")
    lines.append("|---|---|---|---|---|")
    for lane in lanes:
        e = results["lanes"][lane]["empty"]
        r, c = e["run_stock"], e["compile"]
        lines.append(f"| {lane} | {r['best']:.3f}/{r['median']:.3f} | {c['best']:.3f}/{c['median']:.3f} | {e['size']} | {r.get('rss')} |")
    (OUT / "results.md").write_text("\n".join(lines) + "\n")


if __name__ == "__main__":
    main()
