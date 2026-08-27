#!/usr/bin/env bash
# Refresh scripts/ci-test-weights.txt from CI's OWN shard logs (#1615).
#
# The weights file is the shard packer's only input, and its header demands
# every line be measured. The original capture was a laptop session; this
# makes the refresh a command: given a ci.yml run id (default: the latest
# completed develop run), pull the four `Test Rust (shard N/4)` job logs,
# pair each `Running tests/<name>.rs` / `Running unittests … (deps/<name>-…)`
# line with its `test result: … finished in Ns` line (cargo executes targets
# sequentially, so the two streams pair FIFO), sum per target across shards,
# and rewrite the weights file with every target at or above the 5 s default
# (below it, the default's noise floor covers them — balance, never
# coverage, exactly as the file header says).
#
# Also the drift report the header mentions: prints each shard's measured
# `Cargo tests` total so the max/min skew is visible at refresh time.
#
# Usage: bash scripts/refresh-ci-test-weights.sh [run-id]

set -euo pipefail
export LC_ALL=C
cd "$(dirname "$0")/.."

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

python3 - "$TMP" <<'EOF'
import os, re, sys
from collections import defaultdict

tmp = sys.argv[1]
weights = defaultdict(float)
ansi = re.compile(r"\x1b\[[0-9;]*m")
running_test = re.compile(r"Running\s+tests/([A-Za-z0-9_]+)\.rs\s+\(")
running_unit = re.compile(r"Running\s+unittests\s+\S+\s+\(.*/deps/([A-Za-z0-9_]+)-[0-9a-f]{16}\)")
result = re.compile(r"test result: \w+\. .* finished in ([0-9.]+)s")

for logf in sorted(os.listdir(tmp)):
    queue, total = [], 0.0
    with open(os.path.join(tmp, logf), errors="replace") as fh:
        for line in fh:
            line = ansi.sub("", line)
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
head.append("# shard logs (Running/finished-in pairs, summed per target across shards).\n")

with open(path, "w") as fh:
    fh.writelines(head)
    for name, secs in rows:
        fh.write(f"{name:<32} {secs:.0f}\n")
print(f"wrote {len(rows)} measured target(s) to {path}")
EOF
