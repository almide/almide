#!/usr/bin/env bash
# MC/DC VECTOR MUTATION CHECK (#566 rung 2, hole 1): a `vectors` row in
# proofs/mcdc-ledger.toml CLAIMS its named tests demonstrate each condition's
# independent effect. This gate makes the claim mechanical: for every resolved
# site it applies the operator-swap mutant (&& <-> ||) at the site's exact byte
# offset and requires the named tests to be GREEN unmutated and RED under the
# mutant. A mutant that survives means the vectors do not actually sense the
# operator — the row is fiction and the gate is red.
#
#   bash proofs/mcdc-mutation.sh              # all vectors-resolved sites
#   bash proofs/mcdc-mutation.sh --site <id>  # one site
#
# v1 mutant class is the operator swap only: condition-level stuck-at mutants
# need operand extents (an AST), recorded as the ledger's next ratchet. Do not
# run cargo concurrently with this gate — it edits sources in place (backed up
# and restored on every exit path, verified byte-identical afterwards).
set -uo pipefail
export LC_ALL=C
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
python3 - "${1:-}" "${2:-}" <<'PY'
import hashlib, json, re, shutil, subprocess, sys

LEDGER = "proofs/mcdc-ledger.toml"
only = sys.argv[2] if sys.argv[1] == "--site" else None

sites = {}
p = subprocess.run(["bash", "proofs/mcdc-ledger.sh", "--emit-sites"], capture_output=True, text=True)
if p.returncode != 0:
    print("::error::mcdc-mutation: --emit-sites failed"); sys.exit(2)
for line in p.stdout.splitlines():
    s = json.loads(line); sites[s["id"]] = s

rows = {}
src = open(LEDGER, encoding="utf-8").read()
for block in re.split(r'^\[\[site\]\]', src, flags=re.M)[1:]:
    f = dict(re.findall(r'^(\w+)\s*=\s*"((?:[^"\\]|\\.)*)"', block, re.M))
    if f.get("resolution") == "vectors":
        rows[f["id"]] = f

targets = {i: r for i, r in rows.items() if (only is None or i == only)}
if not targets:
    print("::error::mcdc-mutation: no vectors-resolved site matched"); sys.exit(1)

def cargo_for(test_path):
    m = re.match(r'crates/([^/]+)/tests/([^/]+)\.rs$', test_path)
    if m: return ["cargo", "test", "-p", m.group(1), "--test", m.group(2), "--release"]
    m = re.match(r'tests/([^/]+)\.rs$', test_path)
    if m: return ["cargo", "test", "--test", m.group(1), "--release"]
    return None

def run_tests(tests):
    """tests: list of 'file::fn'. Returns (exit, out) running each file's fns exactly."""
    by_file = {}
    for t in tests:
        tf, fn = t.rsplit("::", 1); by_file.setdefault(tf, []).append(fn)
    worst, log = 0, []
    for tf, fns in by_file.items():
        cmd = cargo_for(tf)
        if cmd is None: return (2, [f"unmappable test path {tf}"])
        r = subprocess.run(cmd + ["--"] + fns + ["--exact"], capture_output=True, text=True)
        log.append(r.stdout[-400:] + r.stderr[-400:])
        worst = max(worst, r.returncode)
    return (worst, log)

errs, killed = [], 0
SWAP = {"&&": "||", "||": "&&"}
for sid, row in sorted(targets.items()):
    site = sites.get(sid)
    if site is None:
        errs.append(f"{sid}: vectors row but the site no longer scans"); continue
    tests = [t.strip() for t in row.get("tests", "").split(",") if t.strip()]
    path, off, op = site["file"], site["offset"], site["op"]
    # CHARACTER offset (the scanner regex walks str, and the sources carry
    # multi-byte comments) — read/write as text, never bytes.
    blob = open(path, encoding="utf-8").read()
    if blob[off:off+2] != op:
        errs.append(f"{sid}: offset drifted — {path}@{off} is not {op!r} (rescan)"); continue
    # 1. unmutated: the vectors must be green (a red baseline would fake kills)
    code, log = run_tests(tests)
    if code != 0:
        errs.append(f"{sid}: vectors FAIL UNMUTATED — fix the tests first\n  " + (log[-1][-200:] if log else "")); continue
    # 2. mutant: swap the operator, the vectors must go red
    backup = blob
    try:
        open(path, "w", encoding="utf-8").write(blob[:off] + SWAP[op] + blob[off+2:])
        code, log = run_tests(tests)
    finally:
        open(path, "w", encoding="utf-8").write(backup)
        if hashlib.sha256(open(path, "rb").read()).hexdigest() != hashlib.sha256(backup.encode()).hexdigest():
            print(f"::error::{sid}: RESTORE FAILED for {path} — tree is dirty"); sys.exit(2)
    if code == 0:
        errs.append(f"{sid}: MUTANT SURVIVED — {path}:{site['line']} {op}->{SWAP[op]} and every named vector stayed green: {', '.join(tests)}")
    else:
        killed += 1
        print(f"  {sid} {path}:{site['line']} {op}->{SWAP[op]} killed by its vectors")
for e in errs:
    print(f"::error::{e}")
if errs: sys.exit(1)
print(f"mcdc-mutation OK: {killed}/{len(targets)} operator-swap mutant(s) killed by their named vectors (green unmutated, red mutated).")
PY
