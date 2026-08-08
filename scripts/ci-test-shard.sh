#!/usr/bin/env bash
# Partition `cargo test --workspace` across N CI jobs.
#
# WHY: the suite's wall clock is TEST EXECUTION, not compilation — 29 of the
# step's 30 minutes are `finished in` time. It cannot be fixed by caching, and
# it does not shrink with cores: almost every test shells out to `almide`, and
# each `almide` compile takes a GLOBAL exclusive flock on the shared build dir
# (`src/cli/run.rs::BuildDirLock`), so the whole suite serializes inside one
# machine no matter how wide it is. Separate JOBS each get their own build dir,
# and therefore their own lock — which is where the parallelism has to come
# from.
#
# The load is extremely skewed (measured: the top 4 targets are half the total,
# the top 9 are 80%), so a modulo split would leave one shard carrying the two
# giants. Targets are packed heaviest-first into the least-loaded shard, using
# measured weights from `scripts/ci-test-weights.txt`; an unlisted target gets
# DEFAULT_WEIGHT, which costs balance but never coverage.
#
# VALIDATED 2026-08-08: `cargo test --workspace` reported 1979 passed / 0
# failed, and the four shards together reported 1979 passed / 0 failed — the
# partition runs the same tests, not merely the same target NAMES. The CI gate
# below re-checks the name set on every commit; this count equality was the
# one-time proof that the name set is the right thing to gate on.
#
# COVERAGE IS GATED, NOT ASSUMED: `--list` prints this shard's targets and
# `--list-all` prints every target, so CI can assert that the shards' union is
# the whole set. A partition that silently drops a target would read as a
# faster green build — the exact failure this script must not enable.
set -euo pipefail
cd "$(dirname "$0")/.."

DEFAULT_WEIGHT=5     # seconds; the median target is a few seconds
WEIGHTS="scripts/ci-test-weights.txt"

enumerate() {
  cargo test --workspace --no-run --message-format=json 2>/dev/null | python3 -c '
import sys, json
out = []
for line in sys.stdin:
    try:
        m = json.loads(line)
    except ValueError:
        continue
    if m.get("reason") != "compiler-artifact":
        continue
    if not m.get("profile", {}).get("test"):
        continue
    t = m["target"]
    pkg = m["package_id"].split("#")[-1].split("@")[0]
    # package_id shapes differ across cargo versions; recover the NAME.
    if pkg[0].isdigit():
        pkg = m["package_id"].split("#")[0].rstrip("/").split("/")[-1].split("@")[0]
    kind = t["kind"][0]
    out.append("%s\t%s\t%s" % (pkg, kind, t["name"]))
for line in sorted(set(out)):
    print(line)
'
}

partition() { # shard total  (empty shard = print every target)
  local shard="${1:-}" total="${2:-}"
  enumerate | python3 -c '
import sys, os
shard = os.environ.get("SHARD", "")
total = os.environ.get("TOTAL", "")
default = int(os.environ["DEFAULT_WEIGHT"])
weights = {}
path = os.environ["WEIGHTS"]
if os.path.exists(path):
    for line in open(path):
        line = line.split("#")[0].strip()
        if not line:
            continue
        name, w = line.rsplit(None, 1)
        weights[name.strip()] = int(w)
rows = [l.rstrip("\n").split("\t") for l in sys.stdin if l.strip()]
if not shard:
    for pkg, kind, name in rows:
        print("%s\t%s\t%s" % (pkg, kind, name))
    sys.exit(0)
shard, total = int(shard), int(total)
# Longest-processing-time-first: pack the giants before the gravel.
rows.sort(key=lambda r: (-weights.get(r[2], default), r[2]))
load = [0] * total
bins = [[] for _ in range(total)]
for r in rows:
    i = load.index(min(load))
    bins[i].append(r)
    load[i] += weights.get(r[2], default)
for pkg, kind, name in bins[shard]:
    print("%s\t%s\t%s" % (pkg, kind, name))
'
}

case "${1:-}" in
  --list-all) SHARD="" TOTAL="" DEFAULT_WEIGHT=$DEFAULT_WEIGHT WEIGHTS=$WEIGHTS partition ;;
  --list)     SHARD="$2" TOTAL="$3" DEFAULT_WEIGHT=$DEFAULT_WEIGHT WEIGHTS=$WEIGHTS partition ;;
  --run)
    shard="$2"; total="$3"
    mapfile -t rows < <(SHARD="$shard" TOTAL="$total" DEFAULT_WEIGHT=$DEFAULT_WEIGHT WEIGHTS=$WEIGHTS partition)
    if [ "${#rows[@]}" -eq 0 ]; then
      echo "shard $shard/$total: no targets — a partition that runs nothing is a bug, not a fast build" >&2
      exit 1
    fi
    echo "== shard $shard/$total: ${#rows[@]} target(s) =="
    # ONE cargo invocation PER PACKAGE, not one for the whole shard: `--lib`
    # cannot be repeated ("the argument '--lib' cannot be used multiple times"),
    # and a single flat arg list silently means something else anyway — cargo
    # takes the UNION of -p and the UNION of --test rather than pairing them, so
    # one bad list can run the wrong set while still exiting 0. Grouping by
    # package keeps each invocation's selection unambiguous.
    declare -A pkg_args=()
    for row in "${rows[@]}"; do
      IFS=$'\t' read -r pkg kind name <<<"$row"
      case "$kind" in
        lib)  pkg_args["$pkg"]+=" --lib" ;;
        bin)  pkg_args["$pkg"]+=" --bin $name" ;;
        *)    pkg_args["$pkg"]+=" --test $name" ;;
      esac
    done
    rc=0
    for pkg in "${!pkg_args[@]}"; do
      echo "-- $pkg:${pkg_args[$pkg]}"
      # shellcheck disable=SC2086
      cargo test -p "$pkg" ${pkg_args[$pkg]} || rc=1
    done
    exit $rc
    ;;
  *)
    echo "usage: $0 --run <shard> <total> | --list <shard> <total> | --list-all" >&2
    exit 2
    ;;
esac
