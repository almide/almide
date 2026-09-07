#!/usr/bin/env bash
# Refresh scripts/ci-test-weights.txt from CI's OWN shard logs (#1615).
#
# The weights file is the shard packer's only input, and its header demands
# every line be measured. The original capture was a laptop session; this
# makes the refresh a command: given a ci.yml run id (default: the latest
# completed develop run), pull the four `Test Rust (shard N/4)` job logs,
# sum every nextest per-test line (`PASS [ Ns] (i/n) pkg::target test`) per
# target across shards — or, for a pre-nextest log, pair each `Running
# tests/<name>.rs` line with its `test result: … finished in Ns` line —
# and rewrite the weights file with every target at or above the 5 s default
# (below it, the default's noise floor covers them — balance, never
# coverage, exactly as the file header says).
#
# Also the drift report the header mentions: prints each shard's measured
# `Cargo tests` total so the max/min skew is visible at refresh time.
#
# Usage: bash scripts/refresh-ci-test-weights.sh [run-id]
#        bash scripts/refresh-ci-test-weights.sh --check [run-id]
#
# --check (#1615 item 4, the balance ratchet): measure the latest run's
# per-shard `Cargo tests` totals and FAIL when max/min exceeds
# WEIGHTS_SKEW_RATIO (default 1.4) — the signal that the weights went
# stale again. Run weekly by .github/workflows/shard-balance.yml; the fix
# it demands is exactly this script without --check.

set -euo pipefail
export LC_ALL=C
cd "$(dirname "$0")/.."

CHECK=0
if [ "${1:-}" = "--check" ]; then
  CHECK=1
  shift
fi

RUN_ID="${1:-}"
if [ -z "$RUN_ID" ]; then
  RUN_ID=$(gh run list --workflow ci.yml --branch develop --status success --limit 1 --json databaseId --jq '.[0].databaseId')
  echo "using latest green develop ci.yml run: $RUN_ID"
fi

JOB_IDS=$(gh run view "$RUN_ID" --json jobs --jq '.jobs[] | select(.name | test("Test Rust \\(shard")) | .databaseId')
if [ "$(echo "$JOB_IDS" | grep -c .)" -eq 0 ]; then
  echo "FAIL: run $RUN_ID has no 'Test Rust (shard N/4)' jobs" >&2
  exit 1
fi

TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

for job in $JOB_IDS; do
  gh api "repos/{owner}/{repo}/actions/jobs/$job/logs" > "$TMP/$job.log"
done

python3 - "$TMP" "$CHECK" "${WEIGHTS_SKEW_RATIO:-1.4}" <<'EOF'
import os, re, sys
from collections import defaultdict

tmp = sys.argv[1]
check, skew_limit = sys.argv[2] == "1", float(sys.argv[3])
weights = defaultdict(float)
shard_totals = []
ansi = re.compile(r"\x1b\[[0-9;]*m")
running_test = re.compile(r"Running\s+tests/([A-Za-z0-9_]+)\.rs\s+\(")
running_unit = re.compile(r"Running\s+unittests\s+\S+\s+\(.*/deps/([A-Za-z0-9_]+)-[0-9a-f]{16}\)")
result = re.compile(r"test result: \w+\. .* finished in ([0-9.]+)s")
# cargo-nextest (the shard jobs replay a nextest archive since #1615's
# follow-up): one line per test, `PASS [ 12.345s] ( 3/559) pkg::target name`.
# The binary id is `pkg::target` for tests/ targets, `pkg` alone for a lib's
# unit tests and `pkg::bin/name` for a bin's — reduced to the cargo target
# name the packer keys on.
nextest = re.compile(r"^\s*(?:PASS|SLOW|FAIL|LEAK|TIMEOUT)\s+\[\s*([0-9.]+)s\]\s+\(\s*\d+/\d+\)\s+(\S+)\s+\S")
def nextest_target(binary_id):
    if "::" in binary_id:
        pkg, name = binary_id.split("::", 1)
        return name.split("/", 1)[1] if name.startswith("bin/") else name
    return binary_id.replace("-", "_")

for logf in sorted(os.listdir(tmp)):
    queue, total = [], 0.0
    with open(os.path.join(tmp, logf), errors="replace") as fh:
        for line in fh:
            line = ansi.sub("", line)
            line = re.sub(r"^\S+Z ", "", line)  # the API log prefixes a timestamp
            m = nextest.search(line)
            if m:
                secs = float(m.group(1))
                weights[nextest_target(m.group(2))] += secs
                total += secs
                continue
            m = running_test.search(line) or running_unit.search(line)
            if m:
                queue.append(m.group(1))
                continue
            m = result.search(line)
            if m and queue:
                secs = float(m.group(1))
                weights[queue.pop(0)] += secs
                total += secs
    if queue:
        print(f"WARN: {logf}: {len(queue)} Running line(s) without a result "
              f"(a crashed target?) — dropped: {', '.join(queue)}", file=sys.stderr)
    print(f"  shard {logf.split('.')[0]}: measured test time {total:.0f}s")
    shard_totals.append(total)

if check:
    # A partially-fetched run must not pass by omission: the worst shard
    # could be the missing one.
    if len(shard_totals) < 4:
        print(f"::error::only {len(shard_totals)} shard log(s) parsed (expected 4) — "
              "cannot judge balance from a partial run", file=sys.stderr)
        sys.exit(1)
    lo, hi = min(shard_totals), max(shard_totals)
    ratio = hi / lo if lo > 0 else float("inf")
    print(f"shard balance: max {hi:.0f}s / min {lo:.0f}s = {ratio:.2f} (limit {skew_limit})")
    if ratio > skew_limit:
        print(f"::error::shard skew {ratio:.2f} exceeds {skew_limit} — the weights went "
              "stale; run scripts/refresh-ci-test-weights.sh and commit the refreshed "
              "scripts/ci-test-weights.txt", file=sys.stderr)
        sys.exit(1)
    sys.exit(0)

rows = sorted(((n, s) for n, s in weights.items() if s >= 5.0),
              key=lambda kv: -kv[1])
if not rows:
    print("FAIL: no measured targets at or above 5s — wrong logs?", file=sys.stderr)
    sys.exit(1)

path = "scripts/ci-test-weights.txt"
with open(path) as fh:
    head = []
    for line in fh:
        if line.startswith("#"):
            head.append(line)
        else:
            break

import datetime
stamp = os.environ.get("WEIGHTS_DATE") or datetime.date.today().isoformat()
head = [l for l in head if not l.startswith("# Captured ") and not l.startswith("# Refreshed ")]
head.append(f"# Refreshed {stamp} by scripts/refresh-ci-test-weights.sh from a CI run's own\n")
head.append("# shard logs (nextest per-test lines, or Running/finished-in pairs, summed per target across shards).\n")

with open(path, "w") as fh:
    fh.writelines(head)
    for name, secs in rows:
        fh.write(f"{name:<32} {secs:.0f}\n")
print(f"wrote {len(rows)} measured target(s) to {path}")
EOF
