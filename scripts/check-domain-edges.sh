#!/usr/bin/env bash
# Integer-domain edge gate — the measured matrix, diffed BIDIRECTIONALLY against
# the declared ledger.
#
# THE LAW THIS ENFORCES (../almide-references/RESEARCH-integer-domain-guards.md)
# -----------------------------------------------------------------------------
# A guard must not be defeatable by its own arithmetic. No compiler of the nine
# surveyed enforces how a guard is PHRASED — clippy's lints for it are
# allow-by-default and not enabled on rustc's own source, and Zig has no lint
# layer at all — so this repo cannot copy a lint. What it can copy is Rust
# `tidy`'s shape (src/tools/tidy/src/target_policy.rs:27-62): discover the family
# by WALKING the implementation, subtract what is covered, fail on the remainder,
# and name every exception so each hole is attributed rather than silent.
#
# BOTH DIRECTIONS, because one direction is how the class kept coming back:
#
#   measured-but-undeclared  -> a NEW divergence. Fail. This is the bug catcher.
#   declared-but-not-measured -> a row that no longer diverges. Fail, and delete
#                                the row. Without this half the ledger becomes a
#                                list of things that used to be true, and the
#                                count stops meaning anything.
#
# The DIVERGE count is a shrink-only ceiling: it may go down, never up.
#
# THE SAME TWO DIRECTIONS FOR THE SKIPPED POPULATION (#2402)
# ---------------------------------------------------------
# A slot the matrix cannot build is coverage the matrix does not have, and the
# count of those was printed for a month and never read back — it held
# `json.index(path: JsonPath, i: Int)`, the signature that carried #2396. So a
# skip is now a ledger row with a name and a reason, and the diff runs on it:
#
#   skipped-but-undeclared   -> coverage LOST (a type left the tables, a probe
#                                stopped compiling). Fail.
#   declared-but-now-built   -> coverage regained. Fail, and delete the row, for
#                                the same reason a stale divergence row fails.
#
# WHY A SEPARATE INSTRUMENT FROM THE FUZZER
# -----------------------------------------
# The fuzzer already owns the extreme pool (generator/pools.rs:76 has i64::MAX,
# i64::MIN, both i32 rails, u32::MAX) and already derives the function catalogue
# by parsing every bundled module (generator/catalogue.rs:165). It does not cross
# them, on purpose: generator/term.rs:363-365 feeds any parameter whose NAME is
# count-like a value from {0,1,2,3,4,5}, because a `repeat` of u32::MAX
# manufactures an out-of-memory "hang" that is noise rather than a finding.
#
# That decision is right and stays. It also draws the blind spot exactly over the
# parameters the room guards read — `pos` is not in the name list, so
# `bytes.set_f32_le` was found by fuzzing; `size` is, so `bytes.chunks` never
# could be. This gate covers what the lottery structurally cannot.
#
# Usage:
#   scripts/check-domain-edges.sh                 # gate against proofs/domain-edges.toml
#   scripts/check-domain-edges.sh --update        # re-measure and rewrite the ledger
#   scripts/check-domain-edges.sh --only bytes    # one module (fast local loop)
#   scripts/check-domain-edges.sh --skips-only    # the population and its skips, no edge run
#   scripts/check-domain-edges.sh --measured F    # diff an existing tool JSON (tests)
#   scripts/check-domain-edges.sh --selftest      # the ledger's admission rules, forged inputs
#
# THE BUDGET PIN (#2387): the wasm leg is measured under a DECLARED linear-memory
# budget (the wasmtime CLI's `-W max-memory-size`, tools/domain_edge_matrix.py
# WASM_BUDGET_BYTES), recorded in the ledger header as `wasm_budget_bytes`.
# Unpinned, the leg's ceiling was the machine's free memory, and an `i32_max`
# cell — 2 GiB for a 1-byte element — read AGREE and DIVERGE from the SAME
# binary minutes apart; two such phantoms sat declared for a month. The ledger
# step refuses a measurement that is not pinned to the declared budget before
# diffing it, so an availability reading can neither fail the gate nor be
# written as a row.
#
# `--update --only M` rewrites only M's rows; the other modules' rows and the
# hand-written header are kept.

set -euo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
LEDGER="$REPO/proofs/domain-edges.toml"
ALMIDE="${ALMIDE_BIN:-$REPO/target/release/almide}"
MEASURED=""
OWN_MEASURED=""

UPDATE=0
ONLY=""
SKIPS_ONLY=""
while [ $# -gt 0 ]; do
  case "$1" in
    --update)     UPDATE=1; shift ;;
    --only)       ONLY="$2"; shift 2 ;;
    --skips-only) SKIPS_ONLY=1; shift ;;
    --measured)   MEASURED="$2"; shift 2 ;;
    --selftest)   exec python3 "$REPO/tools/domain_edge_ledger.py" --selftest ;;
    *) echo "unknown flag: $1" >&2; exit 2 ;;
  esac
done

if [ -z "$MEASURED" ]; then
  if [ ! -x "$ALMIDE" ]; then
    echo "::error::$ALMIDE not built — run 'cargo build --release' first"
    exit 1
  fi
  OWN_MEASURED="$(mktemp -t domain-edges-XXXXXX.json)"
  MEASURED="$OWN_MEASURED"
  trap 'rm -f "$OWN_MEASURED"' EXIT
  echo "domain-edges: measuring${ONLY:+ (module $ONLY)}${SKIPS_ONLY:+ (skips only)} …"
  # The tool exits 1 when any cell diverges (the ledger decides whether that is
  # declared) and 2 when it cannot pin the wasm leg (#2387) — that one is a
  # hard failure, because the alternative is an unpinned measurement
  # pretending to be a verdict.
  set +e
  python3 "$REPO/tools/domain_edge_matrix.py" \
    --json "$MEASURED" --almide "$ALMIDE" ${ONLY:+--only "$ONLY"} ${SKIPS_ONLY:+--skips-only} \
    | grep -E "^  (coverage|outside the public surface):|^domain-edges: wasm leg pinned"
  tool_rc=${PIPESTATUS[0]}
  set -e
  if [ "$tool_rc" -eq 2 ]; then
    echo "::error::domain-edges: could not pin the wasm leg (see above) — not measuring"
    exit 1
  fi
fi

# The ledger step lives in tools/domain_edge_ledger.py so its admission rules
# (both diffs, and the #2387 budget pin that refuses an unpinned measurement)
# are testable with forged inputs: `--selftest` above, and
# tests/domain_edge_skip_ledger_test.rs through `--measured`.
LEDGER_ARGS=("$MEASURED" "$LEDGER")
[ "$UPDATE" -eq 1 ] && LEDGER_ARGS+=(--update)
[ -n "$ONLY" ] && LEDGER_ARGS+=(--only "$ONLY")
python3 "$REPO/tools/domain_edge_ledger.py" "${LEDGER_ARGS[@]}"
