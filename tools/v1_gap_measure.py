#!/usr/bin/env python3
"""Precisely measure the v1 (render_wasm proven subset) coverage gap over the real .almd corpus.

For every real program/library, run render_program and parse the per-function "outside the lowering
subset" reasons, bucket them by gap CATEGORY, and quantify the program-level impact: how many programs
fully render now, and — greedily, in impact order — how many MORE become full-pass as each category is
closed. This turns "where to bet" into a measured ranking."""
import subprocess, re, glob, os, sys, collections

RP = "target/debug/examples/render_program"

# Gap categories — a stable bucket per recurring render_program Unsupported reason.
CATS = [
    ("heap-if/match (let/var/tail)", re.compile(r"heap-result `?if|heap-result if|variant.*match in tail|Option/Result.*match", re.I)),
    ("capability (env/fs/effectful)", re.compile(r"capabilit|effectful/impure|declared capability|env\.|fs\.|\bio\b", re.I)),
    ("List[heap] literal (nested)",   re.compile(r"List\[heap\] literal|non-empty List\[heap\]|nested-ownership", re.I)),
    ("heap module-level global",      re.compile(r"module-level (heap )?global|heap module-level", re.I)),
    ("call-arg not in brick",         re.compile(r"call argument .* not in this brick", re.I)),
    ("unlinked stdlib/runtime call",  re.compile(r"unlinked stdlib|no wasm definition", re.I)),
    ("closure/funcref",               re.compile(r"closure|funcref|lambda|first-class fn", re.I)),
    ("string-interp/format",          re.compile(r"interp|format|to_string total", re.I)),
]
def categorize(reason):
    for name, rx in CATS:
        if rx.search(reason):
            return name
    return "OTHER: " + reason[:60]

def corpus():
    pats = ["examples/*.almd", "research/benchmark/exercises/**/*.almd",
            "tools/**/*.almd", "stdlib/*.almd", "research/benchmark/perf/native/*.almd"]
    files = []
    for p in pats:
        files += glob.glob(p, recursive=True)
    return sorted(f for f in files if not f.endswith("_test.almd"))

OUTSIDE_RX = re.compile(r"(\d+) of (\d+) function\(s\) outside the lowering subset")
FUNC_RX = re.compile(r"^\s+([A-Za-z0-9_]+): Unsupported\(\"(.+)\"\)\s*$")
TYPEERR_RX = re.compile(r"type errors:")
UNLINK_RX = re.compile(r"unlinked stdlib/runtime call")

def run_one(f):
    try:
        r = subprocess.run([RP, f], capture_output=True, text=True, timeout=60)
    except subprocess.TimeoutExpired:
        return {"file": f, "verdict": "TIMEOUT", "cats": set(), "n": 0, "out": 0}
    out = r.stdout + r.stderr
    m = OUTSIDE_RX.search(out)
    cats, reasons = set(), []
    for line in out.splitlines():
        fm = FUNC_RX.match(line)
        if fm:
            c = categorize(fm.group(2)); cats.add(c); reasons.append((fm.group(1), c, fm.group(2)))
    if m:
        n_out, n_tot = int(m.group(1)), int(m.group(2))
        verdict = "PARTIAL"
    elif TYPEERR_RX.search(out):
        n_out, n_tot, verdict = 0, 0, "TYPE-ERR"
        te = re.search(r"type errors: \[(.+?)\]", out)
        if te:
            for cap in re.findall(r"undefined variable '(\w+)'", te.group(1)):
                cats.add("capability (env/fs/effectful)")
            if not cats: cats.add("OTHER: type-error")
    elif UNLINK_RX.search(out) and not m:
        n_out, n_tot, verdict = 0, 0, "LINK-FAIL"; cats.add("unlinked stdlib/runtime call")
    elif "(module" in out or "(func" in out:
        n_out, n_tot, verdict = 0, 0, "FULL-PASS"
    else:
        n_out, n_tot, verdict = 0, 0, "OTHER-FAIL"
    return {"file": f, "verdict": verdict, "cats": cats, "n": n_tot, "out": n_out, "reasons": reasons}

def group_of(f):
    return "stdlib" if f.startswith("stdlib/") else "apps"

def report(results, files, label):
    print(f"\n########## {label} ({len(files)} files) ##########")
    by_verdict = collections.Counter(r["verdict"] for r in results)
    for v, c in by_verdict.most_common():
        print(f"  {v:12} {c}")
    # FULL distinct reasons (untruncated), by program count
    reasonprogs = collections.defaultdict(set); reasonfns = collections.Counter()
    for r in results:
        for (fn, c, reason) in r.get("reasons", []):
            key = re.sub(r"`[^`]*`", "`X`", reason)[:95]   # normalize backticked names
            reasonprogs[key].add(r["file"]); reasonfns[key] += 1
    print(f"  --- top distinct gap reasons (progs | fns) ---")
    for k, progs in sorted(reasonprogs.items(), key=lambda x: -len(x[1]))[:12]:
        print(f"    {len(progs):3} | {reasonfns[k]:4} | {k}")
    # greedy unblock WITHIN this group: programs that become FULL-PASS as reason-keys close
    full = sum(1 for r in results if r["verdict"] == "FULL-PASS")
    blocked = [r for r in results if r["verdict"] in ("PARTIAL","TYPE-ERR","LINK-FAIL")]
    # map each blocked program to its set of normalized reason-keys
    def keys_of(r):
        ks = set(re.sub(r"`[^`]*`", "`X`", reason)[:95] for (_, _, reason) in r.get("reasons", []))
        if r["verdict"] == "TYPE-ERR": ks.add("__type_err__")
        if r["verdict"] == "LINK-FAIL": ks.add("__link_fail__")
        return ks
    order = sorted(reasonprogs.keys(), key=lambda k: -len(reasonprogs[k]))
    closed = set(); print(f"  --- greedy unblock (start {full}/{len(files)} full-pass) ---")
    for k in order[:8]:
        closed.add(k)
        now = sum(1 for r in blocked if keys_of(r) and keys_of(r).issubset(closed))
        print(f"    +close [{k[:55]}] -> {full+now}/{len(files)}")

def main():
    files = corpus()
    results = [run_one(f) for f in files]
    for grp in ("apps", "stdlib"):
        gr = [r for r in results if group_of(r["file"]) == grp]
        gf = [f for f in files if group_of(f) == grp]
        report(gr, gf, grp.upper())
    # program-level
    by_verdict = collections.Counter(r["verdict"] for r in results)
    full = [r for r in results if r["verdict"] == "FULL-PASS"]
    blocked = [r for r in results if r["verdict"] not in ("FULL-PASS",)]
    print(f"=== PROGRAM-LEVEL ({len(files)} real .almd) ===")
    for v, c in by_verdict.most_common():
        print(f"  {v:12} {c}")
    # function-level gap tally
    catfns = collections.Counter()
    catprogs = collections.defaultdict(set)
    for r in results:
        for (fn, c, reason) in r.get("reasons", []):
            catfns[c] += 1; catprogs[c].add(r["file"])
        for c in r["cats"]:
            catprogs[c].add(r["file"])
    print(f"\n=== GAP CATEGORIES (by # programs affected) ===")
    for c, progs in sorted(catprogs.items(), key=lambda x: -len(x[1])):
        print(f"  {len(progs):3} programs | {catfns.get(c,0):4} fns | {c}")
    # greedy unblock: which programs become full-pass as we close categories in impact order
    print(f"\n=== GREEDY UNBLOCK (programs that become FULL-PASS as categories close) ===")
    remaining = [r for r in blocked if r["verdict"] in ("PARTIAL","TYPE-ERR","LINK-FAIL")]
    closed = set()
    # rank categories by how many *currently-blocked* programs they'd help solo-close
    order = sorted(catprogs.keys(), key=lambda c: -len(catprogs[c]))
    cum = len(full)
    print(f"  start: {len(full)}/{len(files)} full-pass")
    for c in order:
        closed.add(c)
        now = [r for r in remaining if r["cats"] and r["cats"].issubset(closed)]
        newly = len(now)
        print(f"  + close [{c}] -> {len(full)+newly}/{len(files)} full-pass (cumulative)")
    # the blockers per still-blocked program (top offenders)
    print(f"\n=== STILL-BLOCKED programs + their gap categories ===")
    for r in sorted(blocked, key=lambda r: -r["out"])[:18]:
        print(f"  {r['out']:2}/{r['n']:<2} {os.path.basename(r['file']):28} {r['verdict']:9} {'; '.join(sorted(r['cats']))[:70]}")

main()
