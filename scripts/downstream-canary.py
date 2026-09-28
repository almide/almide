#!/usr/bin/env python3
"""Downstream canary (#2839): roster -> matrix, leg results -> report.

  downstream-canary.py matrix [roster]                 JSON matrix for the workflow
  downstream-canary.py report <results-dir> [roster]   Markdown table on stdout;
                                                       exit 1 on any REGRESSION or
                                                       on any project without a verdict

<results-dir> holds one sub-directory per leg, named by the matrix `slug`,
each with the result.tsv written by scripts/downstream-canary.sh. A roster
project whose directory or result is missing is reported as infra (the leg
never delivered) — never as a pass, and never silently dropped: the report
counts every roster entry.
"""
import json
import os
import sys
import tomllib

HERE = os.path.dirname(os.path.abspath(__file__))
DEFAULT_ROSTER = os.path.join(HERE, "downstream-canary.toml")
DEFAULT_TIMEOUT = 900


def load(roster):
    with open(roster, "rb") as f:
        projects = tomllib.load(f)["project"]
    seen = set()
    for p in projects:
        p["slug"] = p["repo"].replace("/", "__")
        if p["slug"] in seen:
            sys.exit(f"duplicate roster entry: {p['repo']}")
        seen.add(p["slug"])
        if not p.get("commands"):
            sys.exit(f"{p['repo']}: no commands")
    return projects


def matrix(roster):
    out = []
    for p in load(roster):
        out.append({
            "slug": p["slug"],
            "repo": p["repo"],
            "ref": p.get("ref", ""),
            "commands": "\n".join(p["commands"]),
            "timeout": p.get("timeout", DEFAULT_TIMEOUT),
        })
    print(json.dumps({"include": out}))


# Project verdict = the worst of its commands, in this order.
RANK = ["ok/FAIL", "infra", "ok/flaky", "FAIL/ok", "fail/fail", "ok/ok"]
LABEL = {
    "ok/FAIL": "**REGRESSION**",
    "infra": "infra (no verdict)",
    "ok/flaky": "flaky",
    "FAIL/ok": "fixed",
    "fail/fail": "pre-existing failure",
    "ok/ok": "ok",
}


def read_leg(path):
    rows = []
    if not os.path.exists(path):
        return rows
    with open(path) as f:
        for line in f:
            parts = line.rstrip("\n").split("\t")
            parts += [""] * (5 - len(parts))
            rows.append(parts[:5])
    return rows


def first_diagnostics(leg_dir, n, limit=40):
    log = os.path.join(leg_dir, f"cmd{n}.candidate.log")
    try:
        with open(log, errors="replace") as f:
            lines = f.read().splitlines()
    except OSError:
        return "(candidate log missing)"
    # Lead with the first error line when there is one.
    start = next((i for i, l in enumerate(lines) if "error" in l.lower()), 0)
    return "\n".join(lines[max(0, start - 2):start - 2 + limit])


def report(results, roster):
    projects = load(roster)
    rows, regressions = [], []
    tally = {k: 0 for k in RANK}
    for p in projects:
        leg_dir = os.path.join(results, p["slug"])
        leg = read_leg(os.path.join(leg_dir, "result.tsv"))
        sha, cmds, infra_reason = "-", [], ""
        for kind, a, b, cls, detail in leg:
            if kind == "project":
                sha = b if b != "-" else sha
                if cls == "infra":
                    infra_reason = detail
            elif kind == "deps" and cls == "infra":
                infra_reason = detail
            elif kind == "cmd":
                cmds.append((a, b, cls, detail))
        ran = {c[0] for c in cmds}
        for c in p["commands"]:
            if c not in ran:
                cmds.append((c, "", "infra", infra_reason or "no result from the leg (job failed, timed out, or was cancelled)"))
        verdict = min((c[2] for c in cmds), key=RANK.index)
        tally[verdict] += 1
        cells = ", ".join(f"`{c[0]}` {c[2]}" for c in cmds)
        notes = "; ".join(f"{c[0]}: {c[3]}" for c in cmds if c[2] not in ("ok/ok", "fail/fail") and c[3])
        if p.get("note"):
            notes = (notes + "; " if notes else "") + p["note"]
        short = sha[:9] if sha != "-" else "-"
        rows.append(f"| {p['repo']} | `{short}` | {LABEL[verdict]} | {cells} | {notes} |")
        for c in cmds:
            if c[2] == "ok/FAIL":
                regressions.append((p["repo"], short, c, first_diagnostics(leg_dir, c[1])))

    out = []
    head = ", ".join(f"{tally[k]} {LABEL[k].strip('*')}" for k in RANK if tally[k])
    out.append(f"**{len(projects)} projects:** {head}\n")
    out.append("| project | HEAD | verdict | commands (baseline/candidate) | notes |")
    out.append("|---|---|---|---|---|")
    out.extend(rows)
    if regressions:
        out.append("\n### Regressions — each blocks the tag until triaged\n")
        for repo, sha, (cmd, _, _, detail), diag in regressions:
            out.append(f"<details><summary>{repo}@{sha} — <code>almide {cmd}</code> ({detail})</summary>\n")
            out.append("```\n" + diag + "\n```\n</details>\n")
    if tally["infra"]:
        out.append(f"\n**{tally['infra']} project(s) produced no verdict (infra).** The run is "
                   "incomplete — re-run the failed legs before reading it as clean.")
    print("\n".join(out))
    return 1 if regressions or tally["infra"] else 0


def main():
    if len(sys.argv) < 2:
        sys.exit(__doc__)
    if sys.argv[1] == "matrix":
        matrix(sys.argv[2] if len(sys.argv) > 2 else DEFAULT_ROSTER)
    elif sys.argv[1] == "report":
        sys.exit(report(sys.argv[2], sys.argv[3] if len(sys.argv) > 3 else DEFAULT_ROSTER))
    else:
        sys.exit(__doc__)


if __name__ == "__main__":
    main()
