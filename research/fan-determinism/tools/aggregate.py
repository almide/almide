#!/usr/bin/env python3
"""Fold every probe's result.json (downloaded artifacts) into one table:
probe / case / config -> runs, distinct outcomes, runs diverging from the
sequential twin's modal outcome, and the median wall time."""
import glob, json, sys

root = sys.argv[1]
rows = []
for p in sorted(glob.glob(f"{root}/**/result.json", recursive=True)):
    r = json.load(open(p))
    for case, cr in r["cases"].items():
        for cname, c in cr["configs"].items():
            if "skipped" in c:
                rows.append((r["probe"], case, cname, "-", "-", c["skipped"], "-"))
                continue
            rows.append((r["probe"], case, cname, c["n"], len(c["outcomes"]), c.get("diverge", "?"), c["wall_median"]))
print("| probe | case | config | runs | outcomes | diverge | median s |")
print("|---|---|---|---|---|---|---|")
for row in rows:
    print("| " + " | ".join(str(x) for x in row) + " |")
