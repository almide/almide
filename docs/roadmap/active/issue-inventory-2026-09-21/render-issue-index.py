#!/usr/bin/env python3
"""Render the open-issue index from the published headers.

Reads every open issue's body, parses the 状態 line the reformatting pass
prepended, and prints (1) coverage, (2) a table grouped by 状態, (3) a table
grouped by area label. Anything whose body has NO header is listed separately —
that list is the honest measure of how far the pass got, so it is printed first.
"""
import json
import re
import subprocess
import sys
from collections import defaultdict

AREAS = ["A-frontend", "A-ir", "A-codegen", "A-wasm", "A-runtime", "A-interp",
         "A-stdlib", "A-driver", "A-perf", "A-ci", "A-modules"]
STATES = ["BLOCKER", "実装", "計測器・ゲート", "設計判断", "追跡", "調査"]
BLOCKER_LABELS = {"I-unsound", "I-miscompile", "I-divergence", "regression"}

out = subprocess.run(
    ["gh", "issue", "list", "--state", "open", "--limit", "300",
     "--json", "number,title,labels,body"],
    capture_output=True, text=True, check=True).stdout
issues = json.loads(out)

rows, missing = [], []
for it in issues:
    body = it.get("body") or ""
    labels = [l["name"] for l in it["labels"]]
    m = re.match(r"> \*\*状態\*\* — (.+)", body)
    if not m:
        missing.append((it["number"], it["title"], labels))
        continue
    state_line = re.sub(r"[*`]", "", m.group(1)).strip()
    # `計測器・ゲート` itself contains ・, so splitting on the first one truncates
    # the state and then reports it as outside the closed set — the parser's
    # fault read as the data's. Match the known states longest-first instead,
    # and only fall back to the split when none of them is the prefix.
    state, rest = None, ""
    for cand in sorted(STATES, key=len, reverse=True):
        if state_line.startswith(cand):
            state = cand
            rest = state_line[len(cand):].lstrip("・").strip()
            break
    if state is None:
        state = state_line.split("・")[0].strip()
        rest = "・".join(state_line.split("・")[1:]).strip()
    area = [l for l in labels if l in AREAS]
    rows.append({
        "n": it["number"], "title": it["title"], "state": state,
        "rest": rest, "area": area, "labels": labels,
    })

print(f"open issues: {len(issues)}   with header: {len(rows)}   without: {len(missing)}")
if missing:
    print("\n## NO HEADER YET (the pass did not reach these)")
    for n, t, labels in sorted(missing):
        print(f"  #{n}  [{','.join(labels) or '-'}]  {t[:70]}")

# Consistency checks that can fail, printed whether or not they do.
print("\n## checks")
noarea = [r["n"] for r in rows if not r["area"]]
print(f"  header but no A-* label: {len(noarea)}" + (f" — {noarea}" if noarea else ""))
twoarea = [r["n"] for r in rows if len(r["area"]) > 1]
print(f"  two A-* labels: {len(twoarea)}" + (f" — {twoarea}" if twoarea else ""))
badstate = [(r["n"], r["state"]) for r in rows if r["state"] not in STATES]
print(f"  状態 outside the closed set: {len(badstate)}" + (f" — {badstate}" if badstate else ""))
# A BLOCKER 状態 must agree with the blocker labels, in both directions.
mism = [(r["n"], r["state"], sorted(set(r["labels"]) & BLOCKER_LABELS))
        for r in rows
        if (r["state"] == "BLOCKER") != bool(set(r["labels"]) & BLOCKER_LABELS)]
print(f"  BLOCKER 状態 vs I-*/regression label disagreement: {len(mism)}"
      + (f" — {mism}" if mism else ""))

by_state = defaultdict(list)
for r in rows:
    by_state[r["state"]].append(r)
print("\n## by 状態")
for s in STATES + sorted(k for k in by_state if k not in STATES):
    if s not in by_state:
        continue
    print(f"\n### {s}  ({len(by_state[s])})")
    for r in sorted(by_state[s], key=lambda r: r["n"]):
        pr = [l for l in r["labels"] if l.startswith("P-")]
        tag = " ".join(r["area"] + pr)
        print(f"  #{r['n']:<5} {tag:<22} {r['title'][:66]}")
        if r["rest"]:
            print(f"         └ {r['rest'][:96]}")

by_area = defaultdict(list)
for r in rows:
    for a in (r["area"] or ["(none)"]):
        by_area[a].append(r)
print("\n## by crate")
for a in AREAS + ["(none)"]:
    if a not in by_area:
        continue
    nums = " ".join(f"#{r['n']}" for r in sorted(by_area[a], key=lambda r: r["n"]))
    print(f"  {a:<12} {len(by_area[a]):>3}   {nums}")
