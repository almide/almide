#!/usr/bin/env python3
"""Time every implementation build.sh produced and write the report.

Three tables, all on this one runner: native, wasm AOT (wasmtime compile +
run --allow-precompiled) and wasm normal (wasmtime run -C cache=n: Cranelift
compiles the module at load, inside the process time).

Every run must print the expected first line (computed here), else the
implementation is dropped and the failure reported. Rounds alternate: each
round runs every implementation once, in an order rotated (and reversed on
odd rounds) so no implementation always runs first or after the same one.

Phases come from the PHASES line each program prints (ns, timed in-process);
total is the process wall time measured here (spawn + phases + teardown, and
for wasm the runtime start and, in normal mode, the compile).

Allocations come from the counting builds in out/count (run once each).
"""
import json
import os
import platform
import re
import statistics
import subprocess
import sys
import time

OUT = os.environ.get("OUT", os.path.join(os.getcwd(), "out"))
ROUNDS = int(os.environ.get("JW_ROUNDS", "15"))
HERE = os.path.dirname(os.path.abspath(__file__))
N = 50000


def expected_line():
    recs = [{"id": i, "name": f"user_{i}", "tags": [f"t{i % 7}", f"t{i % 11}"],
             "meta": {"score": (i * 37) % 1000, "active": i % 2 == 0}} for i in range(N)]
    text = json.dumps(recs, separators=(",", ":"))
    return f"{sum((i * 37) % 1000 for i in range(N))} {len(text)}"


EXPECTED = expected_line()


def sh(cmd):
    try:
        return subprocess.run(cmd, shell=True, capture_output=True, text=True).stdout.strip()
    except Exception as e:  # noqa: BLE001
        return f"? ({e})"


def run_once(cmd, env=None):
    e = dict(os.environ)
    e.update(env or {})
    t0 = time.perf_counter()
    p = subprocess.run(cmd, capture_output=True, text=True, env=e)
    t1 = time.perf_counter()
    return p, (t1 - t0) * 1e3


def parse_phases(stdout):
    m = re.search(r"^PHASES (.*)$", stdout, re.M)
    if not m:
        return None
    return {k: int(v) / 1e6 for k, v in (kv.split("=") for kv in m.group(1).split())}


def discover():
    tables = {"native": [], "wasm-aot": [], "wasm-normal": []}
    nd = os.path.join(OUT, "native")
    for f in sorted(os.listdir(nd)) if os.path.isdir(nd) else []:
        path = os.path.join(nd, f)
        tables["native"].append((f, [path], {}))
        if f.startswith("simdjson-") and f.endswith("__portable"):
            tables["native"].append((f + "-westmere", [path], {"SIMDJSON_FORCE_IMPLEMENTATION": "westmere"}))
    wd = os.path.join(OUT, "wasm")
    for f in sorted(os.listdir(wd)) if os.path.isdir(wd) else []:
        path = os.path.join(wd, f)
        if f.endswith(".cwasm"):
            tables["wasm-aot"].append((f[:-6], ["wasmtime", "run", "--allow-precompiled", path], {}))
        elif f.endswith(".wasm"):
            tables["wasm-normal"].append((f[:-5], ["wasmtime", "run", "-C", "cache=n", path], {}))
    return tables


def measure(table, configs, failures):
    ok = []
    extra = {}
    for name, cmd, env in configs:
        p, _ = run_once(cmd, env)
        first = p.stdout.splitlines()[0] if p.stdout else ""
        if p.returncode != 0 or first != EXPECTED:
            failures.append({"table": table, "impl": name, "rc": p.returncode, "first": first,
                             "stderr": p.stderr[-2000:]})
            print(f"[{table}] DROP {name}: rc={p.returncode} first={first!r}\n{p.stderr[-600:]}", flush=True)
            continue
        m = re.search(r"^IMPL (.*)$", p.stdout, re.M)
        if m:
            extra[name] = m.group(1)
        ok.append((name, cmd, env))
    samples = {name: {"total": [], "phases": []} for name, _, _ in ok}
    for r in range(ROUNDS):
        k = r % max(1, len(ok))
        order = ok[k:] + ok[:k]
        if r % 2:
            order = order[::-1]
        for name, cmd, env in order:
            p, ms = run_once(cmd, env)
            if p.returncode != 0 or not p.stdout.startswith(EXPECTED):
                failures.append({"table": table, "impl": name, "round": r, "rc": p.returncode,
                                 "stderr": p.stderr[-500:]})
                continue
            samples[name]["total"].append(ms)
            ph = parse_phases(p.stdout)
            if ph:
                samples[name]["phases"].append(ph)
        print(f"[{table}] round {r + 1}/{ROUNDS} done", flush=True)
    return samples, extra


def med(xs):
    return statistics.median(xs) if xs else None


def summarize(samples):
    rows = {}
    for name, s in samples.items():
        row = {"n": len(s["total"]), "total": med(s["total"]), "total_min": min(s["total"]) if s["total"] else None}
        for ph in ("build", "stringify", "parse", "sum"):
            row[ph] = med([p[ph] for p in s["phases"] if ph in p])
        if s["phases"]:
            row["phases"] = med([sum(p.values()) for p in s["phases"]])
        rows[name] = row
    return rows


# ---- allocations ----------------------------------------------------------
def parse_allocs_line(stdout):
    m = re.search(r"^ALLOCS (.*)$", stdout, re.M)
    if not m:
        return None
    out = {}
    for kv in m.group(1).split():
        k, v = kv.split("=")
        a, r = v.split("/")
        out[k] = (int(a), int(r))
    return out


def allocations():
    res = {"native": {}, "wasm": {}}
    for kind in ("native", "wasm"):
        d = os.path.join(OUT, "count", kind)
        for f in sorted(os.listdir(d)) if os.path.isdir(d) else []:
            path = os.path.join(d, f)
            cmd = [path] if kind == "native" else ["wasmtime", "run", "-C", "cache=n", path]
            p, _ = run_once(cmd)
            name = f[:-5] if f.endswith(".wasm") else f
            entry = {"rc": p.returncode, "first": (p.stdout.splitlines() or [""])[0]}
            per = parse_allocs_line(p.stdout)
            if per:
                entry["phases"] = per
            m = re.search(r"__ALMD_ALLOC allocs=(\d+) deallocs=(\d+) reallocs=(\d+) peak=(\d+)", p.stderr)
            if m:
                entry["almide"] = dict(zip(("allocs", "deallocs", "reallocs", "peak"), map(int, m.groups())))
            if not per and not m:
                entry["stderr"] = p.stderr[-400:]
            res[kind][name] = entry
    # Almide on wasm: the embedded host's counters (almide run --target wasm).
    for tag in ("head", "reborn"):
        almide = os.environ.get(f"ALMIDE_{tag.upper()}")
        if not almide or not os.path.exists(almide):
            continue
        for prog in ("jsonround_phased", "jr_build", "jr_stringify"):
            src = os.path.join(HERE, "almide", prog + ".almd")
            p, _ = run_once([almide, "run", src, "--target", "wasm"], {"ALMIDE_WASM_ALLOC_COUNT": "1"})
            m = re.search(r"__ALMD_WASM_ALLOC (\S+) heap_end=(\S+)", p.stderr)
            res["wasm"][f"almide-{tag}-{prog}"] = {
                "rc": p.returncode, "first": (p.stdout.splitlines() or [""])[0],
                "almide_wasm": {"count": m.group(1), "heap_end": m.group(2)} if m else None,
                **({} if m else {"stderr": p.stderr[-400:]}),
            }
    return res


def alloc_table(kind, res):
    """Rows: impl -> (build, stringify, parse, sum, total) as 'allocs(+reallocs)' strings."""
    rows = []
    for name, e in sorted(res.items()):
        if "phases" in e:
            ph = e["phases"]
            cells = [ph.get(k, (0, 0)) for k in ("build", "stringify", "parse", "sum")]
            tot = (sum(c[0] for c in cells), sum(c[1] for c in cells))
            rows.append((name, *[f"{a:,} (+{r:,})" for a, r in cells], f"{tot[0]:,} (+{tot[1]:,})"))
    # Almide: per phase by difference of the three programs.
    for tag in ("head", "reborn"):
        key = (lambda p: f"almide-{tag}-{p}")
        if kind == "native":
            g = {p: res.get(key(p), {}).get("almide") for p in ("jr_build", "jr_stringify", "jsonround_phased")}
            if all(g.values()):
                b, s, f = g["jr_build"], g["jr_stringify"], g["jsonround_phased"]
                cell = lambda x, y=None: f"{x['allocs'] - (y['allocs'] if y else 0):,} (+{x['reallocs'] - (y['reallocs'] if y else 0):,})"
                rows.append((f"almide-{tag} (by program difference; parse+sum together)", cell(b), cell(s, b), cell(f, s), "—", cell(f)))
        else:
            g = {p: (res.get(key(p), {}).get("almide_wasm") or {}).get("count") for p in ("jr_build", "jr_stringify", "jsonround_phased")}
            if all(v is not None and v.isdigit() for v in g.values()):
                b, s, f = (int(g[p]) for p in ("jr_build", "jr_stringify", "jsonround_phased"))
                rows.append((f"almide-{tag} (embedded host counter, by program difference; parse+sum together)", f"{b:,}", f"{s - b:,}", f"{f - s:,}", "—", f"{f:,}"))
    return rows


def fmt(x, nd=1):
    return "—" if x is None else f"{x:,.{nd}f}"


def main():
    info = {
        "cpu": sh("lscpu | sed -n 's/^Model name:[[:space:]]*//p'"),
        "nproc": sh("nproc"),
        "kernel": platform.release(),
        "wasmtime": sh("wasmtime --version"),
        "rustc": sh("rustc -V"),
        "gcc": sh("gcc --version | head -1"),
        "deps": sh(f"cat {os.environ.get('JW_DEPS', 'deps')}/versions.txt"),
        "almide_head": sh(f"{os.environ.get('ALMIDE_HEAD', 'false')} --version"),
        "almide_reborn": sh(f"{os.environ.get('ALMIDE_REBORN', 'false')} --version"),
        "flags": sh("grep -o -w -E 'avx2|avx512f|bmi2|sse4_2' /proc/cpuinfo | sort -u | tr '\\n' ' '"),
        "rounds": ROUNDS,
        "expected": EXPECTED,
        "run_id": os.environ.get("GITHUB_RUN_ID", "local"),
    }
    print(json.dumps(info, indent=1), flush=True)
    failures = []
    results = {}
    impl_notes = {}
    for table, configs in discover().items():
        if not configs:
            continue
        samples, extra = measure(table, configs, failures)
        results[table] = {"rows": summarize(samples), "samples": samples}
        impl_notes[table] = extra
    allocs = allocations()

    md = [f"# JSON world reference — run {info['run_id']}", "",
          f"Runner: {info['cpu']} ({info['nproc']} vCPU; flags: {info['flags']}), kernel {info['kernel']}.",
          f"{info['wasmtime']}; {info['rustc']}; {info['gcc']}; {info['deps']}.",
          f"almide head: {info['almide_head']}; almide json-reborn: {info['almide_reborn']}.",
          f"Median of {ROUNDS} alternating rounds; every run printed `{EXPECTED}`.",
          "Phases in ms (in-process monotonic clock); total = process wall time in ms.", ""]
    for table, data in results.items():
        rows = data["rows"]
        ref_name = next((n for n in ("almide-head__v3", "almide-head__plain") if n in rows), None)
        ref = rows.get(ref_name, {}) if ref_name else {}
        md += [f"## {table}", "", f"Ratio = {ref_name} ÷ row (above 1: Almide head is slower).", "",
               "| impl | variant | build | stringify | parse | sum | Σ phases | total | total min | ratio Σ | ratio total | note |",
               "|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---|"]
        for name, r in sorted(rows.items(), key=lambda kv: (kv[1]["total"] or 1e18)):
            impl, _, variant = name.partition("__")
            rs = ref.get("phases") / r["phases"] if ref.get("phases") and r.get("phases") else None
            rt = ref.get("total") / r["total"] if ref.get("total") and r.get("total") else None
            md.append(f"| {impl} | {variant} | {fmt(r['build'])} | {fmt(r['stringify'])} | {fmt(r['parse'])} | {fmt(r['sum'])} | "
                      f"{fmt(r.get('phases'))} | {fmt(r['total'])} | {fmt(r['total_min'])} | {fmt(rs, 2)} | {fmt(rt, 2)} | {impl_notes[table].get(name, '')} |")
        md.append("")
    for kind in ("native", "wasm"):
        md += [f"## allocations ({kind})", "", "allocs (+reallocs) per phase, counting builds run once.", "",
               "| impl | build | stringify | parse | sum | total |", "|---|---:|---:|---:|---:|---:|"]
        for row in alloc_table(kind, allocs[kind]):
            md.append("| " + " | ".join(row) + " |")
        md.append("")
    if failures:
        md += ["## failures", ""] + [f"- {f['table']} {f['impl']}: rc={f.get('rc')} {f.get('first', '')!r} {f.get('stderr', '')[-300:]!r}" for f in failures[:60]] + [""]
    bf = os.path.join(OUT, "build-failed.txt")
    if os.path.exists(bf) and open(bf).read().strip():
        md += ["## build failures", "", "- " + "\n- ".join(open(bf).read().split()), ""]
    text = "\n".join(md)
    open(os.path.join(OUT, "results.md"), "w").write(text)
    json.dump({"info": info, "results": results, "allocs": allocs, "failures": failures},
              open(os.path.join(OUT, "results.json"), "w"), indent=1)
    print(text)
    summary = os.environ.get("GITHUB_STEP_SUMMARY")
    if summary:
        open(summary, "a").write(text + "\n")


if __name__ == "__main__":
    sys.exit(main())
