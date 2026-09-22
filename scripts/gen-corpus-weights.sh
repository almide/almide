#!/usr/bin/env bash
# The corpus weight table generator (#2457, #2502): the measured wall of every
# spec/wasm_cross fixture in each corpus gate, rendered into
# proofs/corpus-weights.txt — the table `ALMIDE_CORPUS_SHARD=k/N` balances on
# (crates/almide-corpus/src/lib.rs, `partition_by_weight`).
#
# Why a table: the modulo slice halved each giant's COST but not its WALL
# (run_parity 6 vs 16 min at N=2 on develop run 35645124173) because the
# per-fixture cost of the interpreter legs spans four orders of magnitude and
# the heavy tail fell on one residue class. A slice is now the LPT partition
# over these measured weights. A fixture with no row gets the column's median,
# so a stale table cannot exclude a new fixture — it lands in some shard, and
# the coverage step (scripts/ci-corpus-shards.sh --coverage) still proves the
# union is the corpus.
#
# WHERE THE TABLE IS MEASURED IS PART OF THE TABLE (#2502). Absolute ms differ
# per machine and only the RATIOS matter to the partition — but the ratios
# differ per machine too. The first table was measured on a Mac: it predicted
# run_parity 93 / 93 s per half and the ubuntu-latest runner then ran the same
# slices in 479 / 1096 s, because the heavy fixtures there are allocation-bound
# on a 7 GB runner (#2387) while the Mac's are CPU-bound. So the committed
# table is rendered from CI's OWN recordings and its `# measured-on:` header
# says from which run. `--check` (weekly, .github/workflows/shard-balance.yml)
# fails when the legs drift apart again, and names this script as the fix.
#
#   bash scripts/gen-corpus-weights.sh --from-ci [run-id]   # THE REFRESH
#   bash scripts/gen-corpus-weights.sh --check   [run-id]   # the weekly ratchet
#   ALMIDE_BIN=target/release/almide bash scripts/gen-corpus-weights.sh   # local (experiments)
#   bash scripts/gen-corpus-weights.sh --render <dir> [measured-on]       # from a measurement dir
#
# The three columns and what measures them (each gate records under
# ALMIDE_CORPUS_WEIGHTS_DIR, a `weights/<column>.<gate>[.<k>-of-<N>].txt` of
# `stem<TAB>ms`; a render folds every file of a column, sorted by name, first
# row per stem wins — so the ledger's uncontended interp measurement beats the
# oracle's, and an unsharded local file beats a shard's):
#   run_parity  crates/almide-spine/tests/run_parity.rs — the serial interpreter walk
#   interp      the interp sweep (tests/wasm_runtime_test_parts/interp_leg.rs),
#               measured by the ledger binary; the oracle balances on it too
#   build       the native + wasm (+ wasm-opt) subprocess builds per fixture
#               (tests/wasm_runtime_test_parts/corpus.rs), measured by cross_target
# The `weights/` subdirectory matters: on CI this switch points at the
# shard-partials dir, where a flat `run_parity.*.txt` also matches the
# run_parity gate's `fixtures` and `counts` partials (see `weights_file`).
# Requires wasmtime for the LOCAL measurement (memory: /opt/homebrew/bin off the sandbox PATH).
set -uo pipefail
export LC_ALL=C
export PATH="/opt/homebrew/bin:$PATH"
cd "$(dirname "$0")/.." || exit 2

OUT="proofs/corpus-weights.txt"
MANIFEST="crates/almide-spine/tests/golden/spec-run-manifest.txt"
COLUMNS="run_parity interp build"
# The ten `Test Rust (solo <leg> k/2)` jobs are where the slices run; their
# artifacts carry the recordings and their logs carry the walls --check judges.
ARTIFACT_PATTERN="corpus-shard-*"

# A newer `gh` refuses to print a response containing ANSI escapes unless told
# to (it turned the weekly test-shard ratchet red twice, #2207); an older one
# has neither the refusal nor the flag. Same probe as
# scripts/refresh-ci-test-weights.sh.
gh_escape_flag() {
  # shellcheck disable=SC2086
  if gh $1 --help 2>&1 | grep -q -- '--allow-escape-sequences'; then
    echo "--allow-escape-sequences"
  fi
}

latest_green_develop() {
  gh run list --workflow ci.yml --branch develop --status success --limit 1 --json databaseId --jq '.[0].databaseId'
}

# render <dir> [measured-on]
render() {
  local dir="$1" measured="${2:-local — $(uname -s)/$(uname -m), $(nproc 2>/dev/null || sysctl -n hw.ncpu) cpus}"
  local col stems tmp
  tmp="$(mktemp -d)"
  for col in $COLUMNS; do
    # Every file of the column, in name order. ONLY under a `weights/`
    # directory: a CI download carries the shard partials beside the
    # recordings, and `run_parity.fixtures.*` / `run_parity.counts.*` match a
    # flat `run_parity.*` glob — which would fold `identical 355`, `rows 367`
    # and 735 fixture PATHS into the table as fixture weights.
    find "$dir" -path '*/weights/*' -name "$col.*.txt" | sort | xargs cat > "$tmp/$col.txt" 2>/dev/null
    if [ ! -s "$tmp/$col.txt" ]; then
      echo "::error::no weights/$col.*.txt under $dir — the gate that records that column did not run, or the recordings predate #2502's weights/ layout" >&2
      return 1
    fi
  done
  dir="$tmp"
  # Every stem any column measured, plus the corpus and the manifest (a stem
  # nothing measured gets an empty row: the reader takes the median).
  stems="$(
    { for f in spec/wasm_cross/*.almd; do basename "$f" .almd; done
      grep -v '^[[:space:]]*#' "$MANIFEST" | grep -v '^[[:space:]]*$' | cut -f3 | xargs -n1 basename | sed 's/\.almd$//'
      for col in $COLUMNS; do cut -f1 "$dir/$col.txt"; done
    } | sort -u
  )"
  {
    echo "# corpus-weights — measured wall (ms) of every spec/wasm_cross fixture per"
    echo "# corpus gate (#2457). ALMIDE_CORPUS_SHARD=k/N reads ONE column (which one,"
    echo "# per gate: crates/almide-corpus/src/lib.rs, weight_column) and slices the"
    echo "# sorted corpus by LPT over it, so the N shards' walls are balanced instead"
    echo "# of the heavy tail landing on one residue class. A stem with an empty cell"
    echo "# takes the column's MEDIAN — a stale table cannot exclude a fixture, and"
    echo "# scripts/ci-corpus-shards.sh --coverage proves the union regardless."
    echo "#"
    echo "# The absolute values are one machine's and only the ratios are used — but"
    echo "# the RATIOS are one machine's too (#2502): a Mac-measured table predicted"
    echo "# run_parity 93 / 93 s per half and split the runner 479 / 1096 s. This"
    echo "# table must therefore be a CI measurement; \`# measured-on:\` says which run"
    echo "# it came from, and the weekly ratchet only judges a table that says \`ci\`."
    echo "# Refresh: bash scripts/gen-corpus-weights.sh --from-ci [run-id]"
    echo "# measured-on: $measured"
    printf '# columns:\tstem'
    for col in $COLUMNS; do printf '\t%s' "$col"; done
    echo
    while IFS= read -r stem; do
      printf '%s' "$stem"
      for col in $COLUMNS; do
        printf '\t%s' "$(awk -F'\t' -v s="$stem" '$1 == s { print $2; exit }' "$dir/$col.txt")"
      done
      echo
    done <<<"$stems"
  } > "$OUT"
  echo "wrote $OUT: $(grep -vc '^#' "$OUT") row(s), measured-on: $measured"
  # Summarize the TABLE, not the concatenation it was folded from: a CI render
  # reads two shards × up to three gates per column, so counting raw lines
  # would report 1466 "measured" for 733 fixtures and sum each stem twice.
  local i=1
  for col in $COLUMNS; do
    i=$((i + 1))
    printf '  %-10s %5s measured, %8s ms summed, top: %s\n' "$col" \
      "$(awk -F'\t' -v c="$i" '!/^#/ && $c != "" { n++ } END { print n + 0 }' "$OUT")" \
      "$(awk -F'\t' -v c="$i" '!/^#/ && $c != "" { s += $c } END { print s + 0 }' "$OUT")" \
      "$(awk -F'\t' -v c="$i" '!/^#/ && $c != "" { print $1 "\t" $c }' "$OUT" | sort -t$'\t' -k2,2nr | head -3 | awk -F'\t' '{ printf "%s=%s ", $1, $2 }')"
  done
}

case "${1:-}" in
  --render)
    [ $# -ge 2 ] || { echo "usage: $0 --render <dir> [measured-on]" >&2; exit 2; }
    render "$2" "${3:-}"
    ;;
  --from-ci)
    # The refresh the ratchet demands: take a CI run's `corpus-shard-*`
    # artifacts — every solo job uploaded the walls it measured — and render
    # the table from them. The artifacts are kept for a few days only
    # (retention-days on the upload step in ci.yml), so this runs against a
    # RECENT run; --check names the one it judged.
    RUN_ID="${2:-}"
    if [ -z "$RUN_ID" ]; then
      RUN_ID="$(latest_green_develop)" || exit 2
      echo "using the latest green develop ci.yml run: $RUN_ID"
    fi
    VIEW_ESC=$(gh_escape_flag "run view")
    # shellcheck disable=SC2086
    meta="$(gh run view $VIEW_ESC "$RUN_ID" --json headSha,headBranch,createdAt --jq '[.headSha[0:9], .headBranch, (.createdAt|split("T")[0])] | @tsv')" || exit 2
    IFS=$'\t' read -r sha branch created <<<"$meta"
    dir="$(mktemp -d)"
    if ! gh run download "$RUN_ID" -p "$ARTIFACT_PATTERN" -D "$dir"; then
      echo "::error::no $ARTIFACT_PATTERN artifacts on run $RUN_ID — they expire (retention-days in .github/workflows/ci.yml); pick a newer run, or re-run the solo jobs" >&2
      exit 1
    fi
    n_legs="$(find "$dir" -path '*/weights/*' -name '*.txt' | wc -l | tr -d ' ')"
    echo "downloaded $n_legs weight file(s) from run $RUN_ID ($sha on $branch, $created)"
    render "$dir" "ci — run $RUN_ID ($sha on $branch, $created), ubuntu-latest 4 cpus" || exit 1
    echo "commit $OUT; the next develop run's solo jobs are sliced by it"
    ;;
  --check)
    # The weekly balance ratchet (#2502), the corpus half of
    # .github/workflows/shard-balance.yml. It measures the `Test Rust (solo
    # <leg> k/2)` jobs of a run from their OWN logs — the artifacts are long
    # gone by Monday, the logs are not.
    RUN_ID="${2:-}"
    if [ -z "$RUN_ID" ]; then
      RUN_ID="$(latest_green_develop)" || exit 2
      echo "using the latest green develop ci.yml run: $RUN_ID"
    fi
    API_ESC=$(gh_escape_flag api)
    VIEW_ESC=$(gh_escape_flag "run view")
    TMP="$(mktemp -d)"
    trap 'rm -rf "$TMP"' EXIT
    # shellcheck disable=SC2086
    gh run view $VIEW_ESC "$RUN_ID" --json jobs \
      --jq '.jobs[] | select(.name | test("Test Rust \\(solo ")) | [.databaseId, .name, .conclusion, .startedAt, .completedAt] | @tsv' > "$TMP/jobs.tsv" || exit 2
    if [ ! -s "$TMP/jobs.tsv" ]; then
      echo "::error::run $RUN_ID has no 'Test Rust (solo <leg> k/2)' jobs" >&2
      exit 1
    fi
    while IFS=$'\t' read -r job name conclusion started completed; do
      # shellcheck disable=SC2086
      gh api $API_ESC "repos/{owner}/{repo}/actions/jobs/$job/logs" > "$TMP/$job.log" 2>/dev/null || : > "$TMP/$job.log"
      printf '%s\t%s\t%s\t%s\t%s\n' "$job" "$name" "$conclusion" "$started" "$completed" >> "$TMP/index.tsv"
    done < "$TMP/jobs.tsv"
    python3 - "$TMP" "$OUT" "$RUN_ID" "${CORPUS_SKEW_RATIO:-1.4}" "${CORPUS_POOL_FLOOR_RATIO:-1.6}" <<'EOF'
import datetime, os, re, sys

tmp, table_path, run_id = sys.argv[1], sys.argv[2], sys.argv[3]
skew_limit, floor_limit = float(sys.argv[4]), float(sys.argv[5])

# Which legs are a SERIAL walk (an even column split is an even wall split)
# and which run the interp sweep on a thread pool (the wall is the makespan,
# floored by the single heaviest fixture, which no partition can move —
# crates/almide-corpus/src/lib.rs, `weight_column`).
SERIAL = {"run_parity", "wasm_runtime_cross_target", "wasm_runtime_opt_parity"}
POOL = {"interp_ledger", "wasm_runtime_interp_oracle"}
N = 2

ansi = re.compile(r"\x1b\[[0-9;]*m")
# nextest prints one line per test; the gate's own test is the long one (the
# tool tripwire that rides along is milliseconds).
nextest = re.compile(r"^\s*(?:PASS|FAIL|LEAK|TIMEOUT)\s+\[\s*([0-9.]+)s\]\s+\(")
job_name = re.compile(r"Test Rust \(solo (\S+) (\d+)/(\d+)\)")

def seconds(a, b):
    p = lambda t: datetime.datetime.fromisoformat(t.replace("Z", "+00:00"))
    try:
        return (p(b) - p(a)).total_seconds()
    except ValueError:
        return 0.0

legs, bad, seen_jobs = {}, [], 0
for line in open(os.path.join(tmp, "index.tsv")):
    job, name, conclusion, started, completed = line.rstrip("\n").split("\t")
    m = job_name.search(name)
    if not m:
        continue
    seen_jobs += 1
    leg, k, n = m.group(1), int(m.group(2)), int(m.group(3))
    if conclusion != "success":
        bad.append(f"{name} concluded {conclusion!r}")
    walls = []
    with open(os.path.join(tmp, f"{job}.log"), errors="replace") as fh:
        for l in fh:
            l = ansi.sub("", l)
            l = re.sub(r"^\S+Z ", "", l)  # the API log prefixes a timestamp
            m2 = nextest.search(l)
            if m2:
                walls.append(float(m2.group(1)))
    if not walls:
        bad.append(f"{name}: no nextest PASS/FAIL line in its log")
        continue
    legs.setdefault(leg, {})[k] = (max(walls), seconds(started, completed), n)

# A docs-only run skips every step of these jobs and still concludes success:
# no leg ran, so there is no balance to judge and a red here would be a
# phantom. Say which run it was, so the reader can name another.
if seen_jobs and not legs:
    print(f"::warning::run {run_id}'s solo legs ran no tests (a docs-only run skips them) — "
          f"no balance to judge; pass a run id that built: bash scripts/gen-corpus-weights.sh --check <run-id>")
    sys.exit(0)

if bad:
    print("::error::cannot judge corpus-leg balance from run %s:" % run_id, file=sys.stderr)
    for b in bad:
        print(f"  {b}", file=sys.stderr)
    sys.exit(1)

# A partially-fetched run must not pass by omission: the worst half could be
# the missing one (the doctrine of refresh-ci-test-weights.sh --check).
missing = [f"{leg} {k}/{N}" for leg in sorted(SERIAL | POOL) for k in range(1, N + 1)
           if k not in legs.get(leg, {})]
if missing:
    print(f"::error::run {run_id} is missing solo leg(s): {', '.join(missing)} — "
          "a balance verdict over a partial run is not a verdict", file=sys.stderr)
    sys.exit(1)

# The committed table's provenance and, for the pool legs, the makespan floor
# it records (the heaviest fixture in the interp column).
measured_on, header, rows = "", [], {}
for l in open(table_path):
    if l.startswith("# measured-on:"):
        measured_on = l.split(":", 1)[1].strip()
    elif l.startswith("# columns:"):
        header = [c.strip() for c in l.split("\t")[1:] if c.strip()]
    elif not l.startswith("#") and l.strip():
        cells = l.rstrip("\n").split("\t")
        rows[cells[0]] = cells[1:]
is_ci = measured_on.startswith("ci")
interp_i = header.index("interp") - 1 if "interp" in header else None
floor_stem, floor_s = "?", 0.0
if interp_i is not None:
    for stem, cells in rows.items():
        v = cells[interp_i].strip() if len(cells) > interp_i else ""
        if v and float(v) / 1000 > floor_s:
            floor_stem, floor_s = stem, float(v) / 1000

print(f"corpus-weights measured-on: {measured_on or '(no header line)'}")
print(f"corpus legs of run {run_id} — the wall of each leg's own test (the part the table")
print("slices; the job wall is that plus a constant setup, so a test-wall verdict implies")
print("the same verdict on the job wall):")
failures = []
for leg in sorted(legs):
    walls = {k: v[0] for k, v in legs[leg].items()}
    jobs = {k: v[1] for k, v in legs[leg].items()}
    hi, lo = max(walls.values()), min(walls.values())
    ratio = hi / lo if lo > 0 else float("inf")
    halves = " · ".join(f"{walls[k]:.0f}s" for k in sorted(walls))
    job_halves = " · ".join(f"{jobs[k]:.0f}s" for k in sorted(jobs))
    print(f"  {leg:28s} test {halves:>16s}   job {job_halves:>16s}")
    if leg in SERIAL:
        verdict = "OK" if ratio <= skew_limit else "SKEW"
        print(f"  {'':28s} max/min {ratio:.2f} (limit {skew_limit}, serial walk) {verdict}")
        if verdict == "SKEW":
            failures.append(f"{leg}: halves {halves}, max/min {ratio:.2f} > {skew_limit}")
    else:
        # The pool legs cannot be balanced by ratio: one fixture is a third of
        # the leg, so the makespan floor is that fixture and the halves stay
        # ≈1.8x apart at BEST. What a stale table does show up as is the max
        # wall drifting away from that floor.
        rel = hi / floor_s if floor_s > 0 else float("inf")
        verdict = "OK" if rel <= floor_limit else "DRIFT"
        print(f"  {'':28s} max/min {ratio:.2f} (not a target: thread-pool leg); "
              f"max/floor {rel:.2f} (limit {floor_limit}, floor {floor_s:.0f}s = {floor_stem}) {verdict}")
        if verdict == "DRIFT":
            failures.append(f"{leg}: max half {hi:.0f}s is {rel:.2f}x the {floor_s:.0f}s makespan floor ({floor_stem}), limit {floor_limit}")

if not failures:
    print("corpus shard balance OK")
    sys.exit(0)

sys.stdout.flush()  # the report explains the ::error:: lines; print it first
for f in failures:
    print(f"::error::{f}", file=sys.stderr)
if not is_ci:
    print(f"::warning::the committed table is not a CI measurement (measured-on: {measured_on!r}), "
          "so this skew is expected and nobody can act on a red here. Refresh it first: "
          "bash scripts/gen-corpus-weights.sh --from-ci", file=sys.stderr)
    print("not judging a table that was not measured on CI — refresh it and this ratchet arms itself")
    sys.exit(0)
print("::error::the corpus weights went stale: run "
      "`bash scripts/gen-corpus-weights.sh --from-ci <a recent green develop run>` "
      "and commit proofs/corpus-weights.txt. A pool leg's drift instead means its heaviest "
      "fixture grew — make that fixture cheaper; re-slicing cannot move a makespan floor.",
      file=sys.stderr)
sys.exit(1)
EOF
    ;;
  "")
    # The LOCAL measurement: for experiments and for a machine's own ratios.
    # It does NOT produce a committable table — --from-ci does (#2502).
    BIN="${ALMIDE_BIN:-target/release/almide}"
    case "$BIN" in /*) ;; *) BIN="$PWD/$BIN" ;; esac
    [ -x "$BIN" ] || { echo "::error::ALMIDE_BIN not executable: $BIN (cargo build --release)" >&2; exit 2; }
    command -v wasmtime >/dev/null || { echo "::error::wasmtime not on PATH — the build column needs it" >&2; exit 2; }
    dir="$(mktemp -d)"
    export ALMIDE_BIN="$BIN" ALMIDE_EXPECT_TOOLS=1 ALMIDE_CORPUS_WEIGHTS_DIR="$dir"
    unset ALMIDE_CORPUS_SHARD ALMIDE_CORPUS_FILTER
    # Serially: the interp sweep takes every core, and a contended
    # measurement is a wrong ratio. The gates' verdicts are not the point
    # here, but a red one still exits non-zero — a weight measured on a
    # panicking fixture is not a weight.
    echo "== run_parity (serial walk) =="
    cargo test --release -p almide-spine --test run_parity || exit 1
    echo "== interp sweep (ledger binary) =="
    cargo test --release --test wasm_runtime_interp_ledger || exit 1
    echo "== builds (cross_target binary) =="
    cargo test --release --test wasm_runtime_cross_target || exit 1
    render "$dir"
    echo "NOTE: this table is a LOCAL measurement — its ratios are this machine's, not the"
    echo "runner's, and the weekly ratchet will not judge it. Commit one from --from-ci."
    ;;
  *)
    echo "usage: $0 [--from-ci [run-id] | --check [run-id] | --render <dir> [measured-on]]" >&2
    exit 2
    ;;
esac
