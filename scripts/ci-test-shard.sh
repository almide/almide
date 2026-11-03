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
# Two measured profiles: a full run (develop pushes, PRs into main) and a CI
# slice run (ALMIDE_CI_SLICE > 1: PRs into develop and their merge queue),
# where every sliced corpus test runs a fraction of its cells while the
# unsliced ones run whole — the same targets weigh differently. Packing a
# slice run with full-run weights left PR shards at 25/24/19/13 min
# (run 41552197154). Every job of one run reads the same env, so all of them
# pick the same file.
if [ "${ALMIDE_CI_SLICE:-1}" -gt 1 ] && [ -f scripts/ci-test-weights-slice.txt ]; then
  WEIGHTS="scripts/ci-test-weights-slice.txt"
else
  WEIGHTS="scripts/ci-test-weights.txt"
fi

# SOLO targets (#2381): the five corpus sweeps that are each ONE test walking
# spec/wasm_cross serially for 18–35 minutes. They are not partitioned; each
# runs as its own `Test Rust (solo …)` job from the same archive, and the
# interp ledger's two gates get a job each. Measured on the first run of the
# family-aware packer (PR #2440, run 35581361256): with a giant and three
# other suites sharing a 4-vCPU runner every giant ran 40–95 % slower than
# alone (opt_parity 1075 s → 2110 s, interp_oracle 1223 → 2040, cross_target
# 1377 → 1981, run_parity 1310 → 1907) and the shard walls were 32–44 min for
# 4.2–5.0 ks sums — the runner, not the packing, was the floor. A giant on
# its own runner costs what it costs alone; the remaining ~6.5 ks of gravel
# packs across the four shards at ~1.6 ks each. `--list-solo-archive` prints
# these rows so the coverage gate can prove shards ∪ solos == everything.
# Rows are `pkg<TAB>name`; each is one solo leg (the ledger binary's two
# gates share one sweep in one test since #2446, so it is one leg too).
SOLO_TARGETS="almide	wasm_runtime_cross_target
almide	wasm_runtime_interp_oracle
almide	wasm_runtime_opt_parity
almide	wasm_runtime_interp_ledger
almide-spine	run_parity"

# leg name -> nextest filterset. The coverage gate asserts the ledger leg's
# filterset selects the binary's whole test list.
solo_filter() {
  case "$1" in
    wasm_runtime_cross_target) echo 'binary_id(=almide::wasm_runtime_cross_target)' ;;
    wasm_runtime_interp_oracle) echo 'binary_id(=almide::wasm_runtime_interp_oracle)' ;;
    wasm_runtime_opt_parity)   echo 'binary_id(=almide::wasm_runtime_opt_parity)' ;;
    run_parity)                echo 'binary_id(=almide-spine::run_parity)' ;;
    interp_ledger)             echo 'binary_id(=almide::wasm_runtime_interp_ledger)' ;;
    *) echo "unknown solo leg: $1" >&2; return 2 ;;
  esac
}
SOLO_LEGS="wasm_runtime_cross_target wasm_runtime_interp_oracle wasm_runtime_opt_parity run_parity interp_ledger"
export SOLO_TARGETS

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
  { case "${ENUM:-}" in
      archive) enumerate_archive ;;
      built)   enumerate_built ;;
      file)    cat "$ROWS_FILE" ;;
      *)       enumerate ;;
    esac; } | python3 -c '
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
solo = set(tuple(l.split("\t")) for l in os.environ.get("SOLO_TARGETS", "").splitlines() if l.strip())
if os.environ.get("LIST_SOLO"):
    # The solo rows, from the SAME enumeration the shards partition — a solo
    # target that stopped existing prints nothing here and the coverage
    # gate sees the difference.
    for pkg, kind, name in rows:
        if (pkg, name) in solo:
            print("%s\t%s\t%s" % (pkg, kind, name))
    sys.exit(0)
if not shard:
    for pkg, kind, name in rows:
        print("%s\t%s\t%s" % (pkg, kind, name))
    sys.exit(0)
shard, total = int(shard), int(total)
# Solo targets run in their own jobs (see SOLO_TARGETS above); the shards
# partition everything else.
rows = [r for r in rows if (r[0], r[2]) not in solo]
# Longest-processing-time-first: pack the giants before the gravel.
rows.sort(key=lambda r: (-weights.get(r[2], default), r[2]))
load = [0] * total
bins = [[] for _ in range(total)]
# The shared-fixture family first, spread one per shard (#2381). The
# `wasm_runtime_*` corpus gates run ONE AT A TIME on a shard (.config/
# nextest.toml, test-group shared-fixture-serial), so two of them on one
# shard cost their SUM in wall clock while everything else on the shard
# overlaps. Summed seconds — the only thing the weights measure — cannot
# see that: on develop run 35569545602 the packer put interp_oracle
# (1223 s) and opt_parity (1075 s) together and that shard ran 2391 s
# for a 2771 s sum, the second-longest job in CI. So the family is placed
# before the gravel, each member on the shard with the fewest family
# members (then the least load); the interp ledger is not in the family
# (its tests are backend-free and run in parallel — see nextest.toml).
def serial_family(name):
    return name.startswith("wasm_runtime_") and name != "wasm_runtime_interp_ledger"
family_count = [0] * total
for r in [r for r in rows if serial_family(r[2])]:
    i = min(range(total), key=lambda j: (family_count[j], load[j], j))
    bins[i].append(r)
    load[i] += weights.get(r[2], default)
    family_count[i] += 1
for r in [r for r in rows if not serial_family(r[2])]:
    i = load.index(min(load))
    bins[i].append(r)
    load[i] += weights.get(r[2], default)
for pkg, kind, name in bins[shard]:
    print("%s\t%s\t%s" % (pkg, kind, name))
'
}

# Archive-sourced enumerate (#1732): the same pkg/kind/name rows, read
# from a cargo-nextest archive instead of a fresh workspace compile. The
# row format is IDENTICAL (diffed 213 == 213 at the refit), so the
# partitioner, the weights file and the shard-coverage gate keep their
# exact contract.
suite_rows() {
  python3 -c '
import json, sys
d = json.load(sys.stdin)
rows = []
for v in d["rust-suites"].values():
    rows.append("%s\t%s\t%s" % (v["package-name"], v["kind"], v["binary-name"]))
for line in sorted(set(rows)):
    print(line)
'
}
enumerate_archive() {
  cargo nextest list --archive-file "$ARCHIVE" --message-format json | suite_rows
}
# The same rows from the build job's own ci-test tree, before anything is
# archived: no unpack. The full list (not `--list-type binaries-only`, whose
# JSON is the binary-list schema without `rust-suites`/`package-name`), so
# the rows come out of the very JSON shape the archive enumeration reads.
enumerate_built() {
  cargo nextest list --cargo-profile ci-test --workspace --message-format json | suite_rows
}

# The nextest binary id of a `pkg<TAB>kind<TAB>name` row.
row_bid() {
  case "$2" in
    lib) echo "$1" ;;
    bin) echo "$1::bin/$3" ;;
    *)   echo "$1::$3" ;;
  esac
}
# The union filterset over a rows file.
rows_expr() {
  local expr="" pkg kind name bid
  while IFS=$'\t' read -r pkg kind name; do
    [ -n "$pkg" ] || continue
    bid="$(row_bid "$pkg" "$kind" "$name")"
    if [ -z "$expr" ]; then expr="binary_id(=$bid)"; else expr="$expr | binary_id(=$bid)"; fi
  done < "$1"
  echo "$expr"
}
solo_expr() {
  # The tool tripwire rides along (#983): a solo runner whose wasmtime or
  # wasm-opt install broke must fail HERE, not let a tool-gated giant
  # self-skip as a pass. It also runs in its shard; ~1 s, not a dup the
  # coverage gate counts (it is not a solo target).
  echo "($(solo_filter "$1")) | binary_id(=almide::tool_arming_tripwire_test)"
}

# Run one shard's rows from an archive (the shard's own, or the full one).
run_rows() { # rows-file archive label
  local rows="$1" archive="$2" label="$3" expr config
  if [ ! -s "$rows" ]; then
    echo "$label: no targets — a partition that runs nothing is a bug, not a fast build" >&2
    exit 1
  fi
  echo "== $label: $(grep -c . "$rows") target(s) (archive) =="
  expr="$(rows_expr "$rows")"
  # The archive must carry every binary the rows name: a filterset over a
  # binary the archive lacks matches nothing and runs green. Read from the
  # archive's own binary list (decompressing the stream, writing nothing).
  # awk reads the whole listing (no early exit: under pipefail a SIGPIPE'd
  # tar would fail the step).
  local member
  member="$(tar --zstd -tf "$archive" | awk '/binaries-metadata\.json$/ && !f { print; f = 1 }')"
  [ -n "$member" ] || { echo "SHARD FAIL: $archive has no binaries-metadata.json" >&2; exit 1; }
  tar --zstd -xOf "$archive" "$member" | python3 -c '
import json, sys
meta = json.load(sys.stdin)
have = set(meta["rust-binaries"])
want = [l.split("\t") for l in open(sys.argv[1]).read().splitlines() if l.strip()]
def bid(p, k, n):
    return p if k == "lib" else (p + "::bin/" + n if k == "bin" else p + "::" + n)
missing = [bid(*r) for r in want if bid(*r) not in have]
if missing:
    print("SHARD FAIL: the archive lacks %d binary(ies) this shard must run: %s" % (len(missing), ", ".join(missing)), file=sys.stderr)
    sys.exit(1)
print("archive carries all %d binaries of the shard (%d in the archive)" % (len(want), len(have)))
' "$rows"
  # Heaviest first INSIDE the shard too. The packer balances summed
  # seconds, but nextest starts tests in name order, so a giant whose name
  # sorts late starts late and becomes the shard's tail (shape_matrix began
  # 11 minutes into a 20-minute shard and ran 7 more). The priorities come
  # from the same measured weights the packer reads — no hand list.
  config="$(mktemp)"
  cat .config/nextest.toml > "$config"
  WEIGHTS=$WEIGHTS python3 -c '
import os, sys
weights = {}
for line in open(os.environ["WEIGHTS"]):
    line = line.strip()
    if line and not line.startswith("#"):
        name, w = line.rsplit(None, 1)
        weights[name.strip()] = int(w)
heavy = []
for row in open(sys.argv[1]):
    if not row.strip():
        continue
    pkg, kind, name = row.rstrip("\n").split("\t")
    bid = pkg if kind == "lib" else (pkg + "::bin/" + name if kind == "bin" else pkg + "::" + name)
    w = weights.get(name, 0)
    if w >= 60:
        heavy.append((-w, bid))
for rank, (_, bid) in enumerate(sorted(heavy)[:100]):
    print("\n[[profile.default.overrides]]\nfilter = %r\npriority = %d" % ("binary_id(=%s)" % bid, 100 - rank))
' "$rows" >> "$config"
  echo "== start order: $(grep -c '^priority' "$config") heavy target(s) prioritised by weight ($WEIGHTS) =="
  # --extract-to .: the archive recreates ./target/debug/* in the
  # workspace, so the CARGO_BIN_EXE_* paths the test helpers baked at
  # compile time resolve on a checkout that never ran cargo (the
  # binding_diag "No such file or directory" class, soak round 2).
  exec cargo nextest run --config-file "$config" --archive-file "$archive" --extract-to . --workspace-remap . --no-fail-fast -E "$expr"
}

case "${1:-}" in
  --list-all) SHARD="" TOTAL="" DEFAULT_WEIGHT=$DEFAULT_WEIGHT WEIGHTS=$WEIGHTS partition ;;
  --list-all-archive)
    ARCHIVE="$2"
    SHARD="" TOTAL="" DEFAULT_WEIGHT=$DEFAULT_WEIGHT WEIGHTS=$WEIGHTS ENUM=archive partition ;;
  --list-archive)
    ARCHIVE="$4"
    SHARD="$2" TOTAL="$3" DEFAULT_WEIGHT=$DEFAULT_WEIGHT WEIGHTS=$WEIGHTS ENUM=archive partition ;;
  --list-solo-archive)
    # The solo rows (pkg/kind/name), for the coverage gate's union.
    ARCHIVE="$2"
    SHARD="" TOTAL="" DEFAULT_WEIGHT=$DEFAULT_WEIGHT WEIGHTS=$WEIGHTS ENUM=archive LIST_SOLO=1 partition ;;
  --solo-legs) echo "$SOLO_LEGS" ;;
  --list-solo-tests-archive)
    # `<binary_id>\t<test>` for one solo leg's filterset — the coverage gate
    # proves the two ledger legs partition their binary with nothing lost.
    leg="$2"; ARCHIVE="$3"
    expr="$(solo_filter "$leg")" || exit 2
    cargo nextest list --archive-file "$ARCHIVE" --workspace-remap . --message-format json -E "$expr" | python3 -c '
import json, sys
d = json.load(sys.stdin)
# `list -E` still prints every testcase of a matched binary; the filter verdict
# is per testcase under filter-match.status.
for bid, v in sorted(d["rust-suites"].items()):
    for t, tc in sorted(v["testcases"].items()):
        if tc.get("filter-match", {}).get("status") == "matches":
            print("%s\t%s" % (bid, t))
'
    ;;
  --run-solo-archive)
    # One solo leg on its own runner (#2381): the same archive replay as a
    # shard, over the leg's filterset instead of a partition.
    leg="$2"; ARCHIVE="$3"
    solo_filter "$leg" >/dev/null || exit 2
    expr="$(solo_expr "$leg")"
    echo "== solo $leg: $expr =="
    exec cargo nextest run --archive-file "$ARCHIVE" --extract-to . --workspace-remap . --no-fail-fast -E "$expr"
    ;;
  --run-archive)
    # The no-compile shard run (#1732): partition from the archive, then
    # ONE nextest invocation over the union filterset of this shard's
    # binary ids (lib = the bare package id, test = pkg::name). Keeps
    # per-test process isolation and --no-fail-fast parity with the
    # per-package cargo loop below.
    shard="$2"; total="$3"; ARCHIVE="$4"
    rows="$(mktemp)"
    SHARD="$shard" TOTAL="$total" DEFAULT_WEIGHT=$DEFAULT_WEIGHT WEIGHTS=$WEIGHTS ENUM=archive partition > "$rows"
    run_rows "$rows" "$ARCHIVE" "shard $shard/$total"
    ;;
  --run-rows)
    # A shard partitioned ONCE by the build job (`--pack-archives`): its rows
    # file and its own archive, which carries only those binaries. No
    # enumeration here, so no full-archive unpack; the coverage gate judges
    # these same rows files.
    run_rows "$2" "$3" "${SHARD_LABEL:-shard rows $2}"
    ;;
  --pack-archives)
    # The build job, after the full archive: enumerate the built tree once,
    # partition it, and write per job one directory holding
    # `almide-tests.tar.zst` (only that job's binaries) and, for a shard,
    # `rows.tsv`. A 4-vCPU shard used to download the 3.4 GB full archive
    # (2.3 min) and unpack it twice (52 s to enumerate, 59 s to run).
    out="$2"; total="$3"
    mkdir -p "$out"
    ENUM=built SHARD="" TOTAL="" DEFAULT_WEIGHT=$DEFAULT_WEIGHT WEIGHTS=$WEIGHTS partition > "$out/all-rows.tsv"
    [ -s "$out/all-rows.tsv" ] || { echo "pack: the built tree enumerated no targets" >&2; exit 1; }
    mkdir -p "$out/rows"
    for k in $(seq 1 "$total"); do
      d="$out/shard-$k"; mkdir -p "$d"
      ENUM=file ROWS_FILE="$out/all-rows.tsv" SHARD=$((k - 1)) TOTAL="$total" DEFAULT_WEIGHT=$DEFAULT_WEIGHT WEIGHTS=$WEIGHTS partition > "$d/rows.tsv"
      cp "$d/rows.tsv" "$out/rows/shard-$k.tsv"
      echo "== pack shard $k/$total: $(grep -c . "$d/rows.tsv") target(s) =="
      cargo nextest archive --workspace --cargo-profile ci-test -E "$(rows_expr "$d/rows.tsv")" --archive-file "$d/almide-tests.tar.zst"
    done
    for leg in $SOLO_LEGS; do
      d="$out/solo-$leg"; mkdir -p "$d"
      echo "== pack solo $leg =="
      cargo nextest archive --workspace --cargo-profile ci-test -E "$(solo_expr "$leg")" --archive-file "$d/almide-tests.tar.zst"
    done
    # The coverage job's merge step runs the two corpus gates' merge form; it
    # gets just those binaries instead of the full archive.
    d="$out/merge"; mkdir -p "$d"
    echo "== pack merge =="
    cargo nextest archive --workspace --cargo-profile ci-test -E "$(bash scripts/ci-corpus-shards.sh --merge-binaries)" --archive-file "$d/almide-tests.tar.zst"
    ls -la "$out"/*/almide-tests.tar.zst
    ;;
  --list-solo-rows)
    # The solo rows of a rows file (the coverage gate's enumeration).
    ENUM=file ROWS_FILE="$2" SHARD="" TOTAL="" DEFAULT_WEIGHT=$DEFAULT_WEIGHT WEIGHTS=$WEIGHTS LIST_SOLO=1 partition ;;
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
    echo "       $0 --run-archive <shard> <total> <archive> | --list-archive <shard> <total> <archive> | --list-all-archive <archive>" >&2
    echo "       $0 --run-solo-archive <leg> <archive> | --list-solo-archive <archive> | --list-solo-tests-archive <leg> <archive> | --solo-legs" >&2
    echo "       $0 --pack-archives <outdir> <total> | --run-rows <rows.tsv> <archive> | --list-solo-rows <rows.tsv>" >&2
    exit 2
    ;;
esac
