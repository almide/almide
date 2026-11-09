#!/usr/bin/env python3
"""Run one fan-determinism probe many times under several substrates and
compare every run's observation (stdout, stderr, exit code, files left in
the cwd) byte-for-byte against the sequential twin's.

usage: drive.py <probe-dir> <out-dir> [--almide PATH] [--runs N]

The probe dir holds fan.almd (the program under test) and seq.almd (the
same arms written one after another: the reading ADR-0024 D1 promises).
Per-probe knobs live in PROBES below.
"""
import argparse, collections, hashlib, json, os, shutil, subprocess, sys, tempfile, threading, time

PROBES = {
    # name: dict(runs, timeout, stdin, cases=[(case-name, env, ulimit_kb)], timing)
    "p05a_map_large": dict(runs=100),
    "p05b_map_trap": dict(runs=100),
    "p05c_map_parallel": dict(runs=50),
    "p10_sockets": dict(runs=20, timeout=8),
    "p11_timing": dict(runs=30, timing=True),
    "p08_stdin": dict(stdin=b"l1\nl2\nl3\nl4\n"),
    "p12_effect_callbacks": dict(runs=60),
    "p13_shared_strings": dict(runs=100),
    "p14_deep_recursion": dict(runs=10, cases=[
        (f"{m}-{d}", {"DEPTH": str(d), "MODE": m}, None)
        for m in ("block", "map") for d in (20000, 50000, 100000, 200000, 400000)]),
    "p15_memory": dict(runs=10, cases=[
        ("n5M-nolimit", {"N": "5000000"}, None),
        ("n20M-nolimit", {"N": "20000000"}, None),
        ("n20M-as1200M", {"N": "20000000"}, 1200 * 1024),
        ("n20M-as800M", {"N": "20000000"}, 800 * 1024),
    ]),
}

CONFIGS = [
    # name, binary (fan|seq|wasm|emb), cpu list, load, extra env
    ("seq", "seq", None, None, {}),
    ("fan", "fan", None, None, {}),
    ("fan_1cpu", "fan", "0", None, {}),
    ("fan_2cpu", "fan", "0,1", None, {}),
    ("fan_load", "fan", None, "all", {}),
    ("fan_1cpu_load", "fan", "0", "cpu0", {}),
    ("fan_seqenv", "fan", None, None, {"ALMIDE_FAN_SEQUENTIAL": "1"}),
    ("fan_threads1", "fan", None, None, {"ALMIDE_FAN_THREADS": "1"}),
    ("wasm", "wasm", None, None, {}),
    ("wasm_emb", "emb", None, None, {}),
]


def sha(b):
    return hashlib.sha256(b).hexdigest()[:16]


def start_load(kind):
    procs = []
    if kind == "all":
        for _ in range(2 * (os.cpu_count() or 2)):
            procs.append(subprocess.Popen([sys.executable, "-c", "while True: pass"]))
        # memory churn too
        procs.append(subprocess.Popen([sys.executable, "-c",
            "import time\nwhile True:\n b=bytearray(200*1024*1024)\n for i in range(0,len(b),4096): b[i]=1\n del b"]))
    elif kind == "cpu0":
        for _ in range(3):
            procs.append(subprocess.Popen(["taskset", "-c", "0", sys.executable, "-c", "while True: pass"]))
    time.sleep(0.3)
    return procs


def stop_load(procs):
    for p in procs:
        p.kill()
    for p in procs:
        p.wait()


def snapshot_files(d):
    out = {}
    for root, _, files in os.walk(d):
        for f in files:
            p = os.path.join(root, f)
            rel = os.path.relpath(p, d)
            with open(p, "rb") as fh:
                out[rel] = sha(fh.read())
    return out


def run_once(cmd, env, stdin, timeout, ulimit_kb, cwd, timing):
    if ulimit_kb:
        cmd = ["bash", "-c", f'ulimit -v {ulimit_kb}; exec "$@"', "x"] + cmd
    t0 = time.monotonic()
    p = subprocess.Popen(cmd, cwd=cwd, env=env, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                         stderr=subprocess.PIPE, start_new_session=True)
    out_chunks, err_chunks, line_times = [], [], []

    def pump(stream, sink, stamps):
        while True:
            line = stream.readline()
            if not line:
                break
            if stamps is not None:
                stamps.append((round((time.monotonic() - t0) * 1000), line.decode(errors="replace").rstrip("\n")))
            sink.append(line)

    th = [threading.Thread(target=pump, args=(p.stdout, out_chunks, line_times if timing else None)),
          threading.Thread(target=pump, args=(p.stderr, err_chunks, None))]
    for t in th:
        t.start()
    try:
        if stdin:
            p.stdin.write(stdin)
        p.stdin.close()
    except BrokenPipeError:
        pass
    timed_out = False
    try:
        p.wait(timeout=timeout)
    except subprocess.TimeoutExpired:
        timed_out = True
        try:
            os.killpg(p.pid, 9)
        except ProcessLookupError:
            pass
        p.wait()
    for t in th:
        t.join(5)
    wall = time.monotonic() - t0
    code = "TIMEOUT" if timed_out else p.returncode
    return b"".join(out_chunks), b"".join(err_chunks), code, wall, line_times


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("probe")
    ap.add_argument("out")
    ap.add_argument("--almide", default="almide")
    ap.add_argument("--runs", type=int, default=200)
    ap.add_argument("--wasmtime", default="wasmtime")
    a = ap.parse_args()
    probe = os.path.abspath(a.probe)
    name = os.path.basename(probe.rstrip("/"))
    knobs = PROBES.get(name, {})
    runs = knobs.get("runs", a.runs)
    timeout = knobs.get("timeout", 60)
    stdin = knobs.get("stdin", b"")
    cases = knobs.get("cases", [("default", {}, None)])
    timing = knobs.get("timing", False)
    out = os.path.abspath(os.path.join(a.out, name))
    os.makedirs(out, exist_ok=True)
    bins = os.path.join(out, "bin")
    os.makedirs(bins, exist_ok=True)

    build = {}
    for kind, src, args in [("fan", "fan.almd", []), ("seq", "seq.almd", []),
                            ("wasm", "fan.almd", ["--target", "wasm"])]:
        dst = os.path.join(bins, kind + (".wasm" if kind == "wasm" else ""))
        r = subprocess.run([a.almide, "build", os.path.join(probe, src), "-o", dst] + args,
                           capture_output=True, timeout=1800)
        build[kind] = dict(code=r.returncode, stderr=r.stderr.decode(errors="replace")[-3000:])
        print(f"[{name}] build {kind}: {r.returncode}", flush=True)
    json.dump(build, open(os.path.join(out, "build.json"), "w"), indent=1)

    def command(kind, env):
        if kind in ("fan", "seq"):
            return [os.path.join(bins, kind)] if build[kind]["code"] == 0 else None
        if kind == "wasm":
            if build["wasm"]["code"] != 0:
                return None
            c = [a.wasmtime, "run", "--dir=.", "-S", "inherit-network=y", "-S", "tcp=y"]
            for k, v in env.items():
                c += ["--env", f"{k}={v}"]
            return c + [os.path.join(bins, "wasm.wasm")]
        if kind == "emb":
            return [a.almide, "run", os.path.join(probe, "fan.almd"), "--target", "wasm"]

    result = dict(probe=name, runs=runs, cases={})
    for case, case_env, ulimit_kb in cases:
        cres = {}
        samples = {}
        for cname, kind, cpus, load, extra in CONFIGS:
            n = runs if kind != "emb" else min(runs, 30)
            env = dict(os.environ)
            env.update(case_env)
            env.update(extra)
            cmd = command(kind, dict(case_env, **extra))
            if cmd is None:
                cres[cname] = dict(skipped="build failed")
                continue
            if cpus:
                cmd = ["taskset", "-c", cpus] + cmd
            procs = start_load(load) if load else []
            tally = collections.Counter()
            walls, timings = [], []
            try:
                for i in range(n):
                    cwd = tempfile.mkdtemp(prefix="fanrun-")
                    so, se, code, wall, lt = run_once(cmd, env, stdin, timeout, ulimit_kb, cwd, timing)
                    files = snapshot_files(cwd)
                    shutil.rmtree(cwd, ignore_errors=True)
                    key = sha(so) + "/" + sha(se) + "/" + str(code) + "/" + sha(json.dumps(files, sort_keys=True).encode())
                    tally[key] += 1
                    walls.append(wall)
                    if timing:
                        timings.append(lt)
                    if key not in samples:
                        samples[key] = dict(stdout=so.decode(errors="replace")[-4000:],
                                            stderr=se.decode(errors="replace")[-4000:],
                                            code=code, files=files, first_config=cname)
            finally:
                stop_load(procs)
            walls.sort()
            cres[cname] = dict(n=n, outcomes=dict(tally.most_common()),
                               wall_median=round(walls[len(walls) // 2], 3), wall_max=round(walls[-1], 3))
            if timing:
                cres[cname]["line_times_first3"] = timings[:3]
            print(f"[{name}/{case}] {cname}: {len(tally)} outcome(s) in {n} runs, median {cres[cname]['wall_median']}s", flush=True)
        oracle = None
        if "outcomes" in cres.get("seq", {}):
            oracle = max(cres["seq"]["outcomes"].items(), key=lambda kv: kv[1])[0]
        for cname, c in cres.items():
            if "outcomes" in c and oracle:
                c["diverge"] = sum(v for k, v in c["outcomes"].items() if k != oracle)
        result["cases"][case] = dict(oracle=oracle, configs=cres, samples=samples)
    json.dump(result, open(os.path.join(out, "result.json"), "w"), indent=1)

    # a readable table
    lines = [f"## {name}", ""]
    for case, cr in result["cases"].items():
        lines.append(f"### case `{case}` (oracle = seq modal outcome `{cr['oracle']}`)")
        lines.append("")
        lines.append("| config | runs | outcomes | diverge from seq | median s |")
        lines.append("|---|---|---|---|---|")
        for cname, c in cr["configs"].items():
            if "skipped" in c:
                lines.append(f"| {cname} | - | {c['skipped']} | - | - |")
                continue
            lines.append(f"| {cname} | {c['n']} | {len(c['outcomes'])} | {c.get('diverge', '?')} | {c['wall_median']} |")
        lines.append("")
        for key, s in cr["samples"].items():
            lines.append(f"<details><summary>outcome {key} (first seen in {s['first_config']}, exit {s['code']})</summary>")
            lines.append("")
            lines.append("stdout (tail):\n```\n" + s["stdout"][-1500:] + "\n```")
            lines.append("stderr (tail):\n```\n" + s["stderr"][-1500:] + "\n```")
            if s["files"]:
                lines.append("files: `" + json.dumps(s["files"]) + "`")
            lines.append("</details>")
            lines.append("")
    open(os.path.join(out, "summary.md"), "w").write("\n".join(lines))


if __name__ == "__main__":
    main()
