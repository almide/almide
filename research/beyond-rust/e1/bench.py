#!/usr/bin/env python3
"""E1 timing driver: interleaved columns, median of N, instructions:u, output hashes."""
import argparse, hashlib, json, os, platform, shutil, statistics, subprocess, tempfile, time

COLS = ["R0", "A0", "A1", "Rstar"]

def run_once(binary, args, perf):
    cmd = [binary] + args
    stat = None
    if perf:
        stat = tempfile.NamedTemporaryFile(delete=False, suffix=".perf").name
        cmd = ["perf", "stat", "-x,", "-e", "instructions:u", "-o", stat] + cmd
    t0 = time.perf_counter()
    p = subprocess.run(cmd, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    dt = time.perf_counter() - t0
    if p.returncode != 0:
        raise SystemExit(f"{binary} exited {p.returncode}: {p.stderr.decode()[:400]}")
    ins = None
    if stat:
        for line in open(stat):
            f = line.strip().split(",")
            if len(f) > 2 and f[2].startswith("instructions"):
                ins = int(f[0]) if f[0].isdigit() else None
        os.unlink(stat)
    return dt, ins, hashlib.sha256(p.stdout).hexdigest(), p.stdout.decode()

def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--dir", required=True)
    ap.add_argument("--almide-dir")
    ap.add_argument("--runs", type=int, default=9)
    ap.add_argument("--kernel", action="append", required=True, help="name=arg1,arg2")
    ap.add_argument("--out", required=True)
    a = ap.parse_args()
    perf = shutil.which("perf") is not None and subprocess.run(
        ["perf", "stat", "-e", "instructions:u", "true"], capture_output=True).returncode == 0
    kernels = []
    for kv in a.kernel:
        k, args = kv.split("=", 1)
        kernels.append((k, args.split(",")))
    res = {"flags": open(os.path.join(a.dir, "flags.txt")).read().strip(),
           "perf_instructions": perf, "runs": a.runs, "kernels": {}}
    cols = list(COLS)
    if a.almide_dir and os.path.basename(a.dir) == "primary":
        cols.append("almide")  # the almide-built (prebuilt-runtime) A0 binary, sanity check
    def path(k, c):
        return os.path.join(a.almide_dir, f"{k}.almide") if c == "almide" else os.path.join(a.dir, f"{k}.{c}")
    for k, args in kernels:
        res["kernels"][k] = {"args": args, "columns": {c: {"times_s": [], "instructions_u": [], "sha256": set()} for c in cols}}
    for k, args in kernels:  # one warm-up per column
        for c in cols:
            _, _, h, out = run_once(path(k, c), args, False)
            res["kernels"][k]["columns"][c]["stdout"] = out
    for r in range(a.runs):
        for k, args in kernels:
            order = cols[r % len(cols):] + cols[:r % len(cols)]  # rotate the start column each round
            for c in order:
                dt, ins, h, _ = run_once(path(k, c), args, perf)
                e = res["kernels"][k]["columns"][c]
                e["times_s"].append(round(dt, 5)); e["sha256"].add(h)
                if ins is not None: e["instructions_u"].append(ins)
    for k in res["kernels"]:
        cs = res["kernels"][k]["columns"]
        for c, e in cs.items():
            e["median_s"] = round(statistics.median(e["times_s"]), 5)
            if e["instructions_u"]: e["median_instructions_u"] = int(statistics.median(e["instructions_u"]))
            e["sha256"] = sorted(e["sha256"])
        hashes = {h for e in cs.values() for h in e["sha256"]}
        res["kernels"][k]["outputs_identical"] = len(hashes) == 1
        m = {c: cs[c]["median_s"] for c in cs}
        res["kernels"][k]["ratios"] = {"A0/A1": round(m["A0"] / m["A1"], 4), "A1/Rstar": round(m["A1"] / m["Rstar"], 4),
                                       "A1/R0": round(m["A1"] / m["R0"], 4), "A0/R0": round(m["A0"] / m["R0"], 4)}
        print(k, {c: m[c] for c in cs}, res["kernels"][k]["ratios"], "identical" if res["kernels"][k]["outputs_identical"] else "OUTPUT MISMATCH")
    json.dump(res, open(a.out, "w"), indent=1)

if __name__ == "__main__":
    main()
