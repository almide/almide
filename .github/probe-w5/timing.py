# Corpus parse/stringify timing per compiler ref, native and wasm AOT, alternating.
import os, re, statistics, subprocess, sys, shutil
out, corpus, rounds = sys.argv[1], sys.argv[2], int(sys.argv[3])
tags = sys.argv[4:]
impls = []
for t in tags:
    impls.append((t, "native", [os.path.join(out, "bin", "c-" + t)]))
    impls.append((t, "wasm-aot", ["wasmtime", "run", "--dir", ".", "--allow-precompiled", os.path.join(out, "bin", "c-" + t + ".cwasm")]))
md = ["# corpus rows (ms per operation, median of %d alternating rounds)" % rounds, ""]
for f in ["canada", "twitter", "citm_catalog"]:
    wd = os.path.join(out, "wd-" + f); os.makedirs(wd, exist_ok=True)
    shutil.copy(os.path.join(corpus, f + ".json"), os.path.join(wd, "corpus.json"))
    samples = {(n, k): {"parse": [], "stringify": []} for n, k, _ in impls}
    first = {}
    for r in range(rounds + 1):
        order = impls if r % 2 == 0 else list(reversed(impls))
        for n, k, cmd in order:
            p = subprocess.run(cmd, cwd=wd, capture_output=True, text=True)
            m = re.search(r"CORPUS parse=(\d+) stringify=(\d+)", p.stdout)
            if p.returncode != 0 or not m:
                first[(n, k)] = "FAIL rc=%d %s" % (p.returncode, (p.stderr or p.stdout)[-200:].replace("\n", " "))
                continue
            first.setdefault((n, k), p.stdout.splitlines()[0])
            if r > 0:
                samples[(n, k)]["parse"].append(int(m.group(1))); samples[(n, k)]["stringify"].append(int(m.group(2)))
    md += ["## %s" % f, "", "| ref | leg | parse ms | stringify ms | first line | parse samples |", "|---|---|---:|---:|---|---|"]
    for n, k, _ in impls:
        s = samples[(n, k)]
        if s["parse"]:
            md.append("| %s | %s | %.3f | %.3f | %s | %s |" % (n, k, statistics.median(s["parse"]) / 1e6, statistics.median(s["stringify"]) / 1e6,
                      first.get((n, k), ""), " ".join("%.1f" % (x / 1e6) for x in sorted(s["parse"]))))
        else:
            md.append("| %s | %s | — | — | %s | |" % (n, k, first.get((n, k), "")))
    md.append("")
text = "\n".join(md)
print(text)
s = os.environ.get("GITHUB_STEP_SUMMARY")
if s: open(s, "a").write(text + "\n")
