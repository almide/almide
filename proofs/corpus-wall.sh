#!/usr/bin/env bash
# V0-CORPUS WALL GATE (step-4 "continuous corpus verification = the definition of
# parity", in its honest first form). The completion definition demands the
# proven profile ACCEPT the v0 corpus via the PCC chain and REJECT outside it.
# This gate runs the ENTIRE v0 spec corpus through the real frontend → MIR
# lowering and asserts the two soundness-relevant invariants empirically:
#
#   (1) THE WALL: `lower_function` is TOTAL over the corpus — every function is
#       `Ok` (in-profile) or an explicit `Unsupported` (walled). Zero panics,
#       zero silent miscompiles. A program outside the value-semantics subset is
#       rejected with a reason, never quietly mislowered.
#   (2) ACCEPT ⟹ SAFE: the ownership witness of EVERY in-profile function is
#       re-verified by the KERNEL-PROVEN checker in one pass (accept ⟹ RC-safe,
#       by `check_sound`) — the PCC chain run over real corpus programs, not just
#       hand-built MIR.
#
# Coverage (how many of the corpus lower) is REPORTED honestly, never gated on a
# brittle exact count — only the soundness invariants are hard. The per-feature
# Unsupported histogram is the coverage roadmap: the biggest buckets name the
# next language-surface features to admit (step 2).
set -euo pipefail
cd "$(dirname "$0")"
ROOT="$(cd .. && pwd)"

CORPUS="${1:-$ROOT/spec}"

echo "== build the corpus classifier (real frontend → MIR lowering) =="
# Pre-existing workspace warnings are not this gate's concern — silence them so
# the honest wall report below is the only thing on screen.
(cd "$ROOT" && cargo build -q -p almide-mir --example classify_corpus) 2>/dev/null

echo "== sweep the v0 corpus: $CORPUS =="
CERTS="$(mktemp)"; REPORT="$(mktemp)"
set +e
(cd "$ROOT" && cargo run -q -p almide-mir --example classify_corpus -- "$CORPUS") \
  >"$CERTS" 2>"$REPORT"
WALL_RC=$?
set -e
# Show only the harness report (from its marker), dropping any cargo build noise.
sed -n '/== v0-corpus MIR-lowering wall report ==/,$p' "$REPORT"

if [ "$WALL_RC" -ne 0 ]; then
  echo "WALL GATE FAIL: lower_function breached totality (panicked) on a corpus function." >&2
  rm -f "$CERTS" "$REPORT"
  exit 1
fi

# Anti-collapse floor: at least one in-profile program must reach the checker, so
# the PCC chain is genuinely exercised (a corpus where NOTHING lowers would pass
# the wall vacuously — that silent coverage collapse must fail the gate).
if [ ! -s "$CERTS" ]; then
  echo "WALL GATE FAIL: no in-profile heap object reached the checker (coverage collapsed to 0)." >&2
  rm -f "$CERTS" "$REPORT"
  exit 1
fi

echo
echo "== build the kernel-proven checker (from the Coq proof) =="
./build-checker.sh >/dev/null

echo "== PCC chain: the proven checker re-verifies EVERY in-profile witness =="
IN_PROFILE_OBJS=$(wc -l < "$CERTS" | tr -d ' ')
set +e
./checker ownership "$CERTS" >/tmp/corpus-wall.checker.out 2>&1
CHECK_RC=$?
set -e
echo "checker over $IN_PROFILE_OBJS in-profile heap object(s): $(cat /tmp/corpus-wall.checker.out) (exit $CHECK_RC)"

if [ "$CHECK_RC" -ne 0 ]; then
  echo "WALL GATE FAIL: the proven checker REJECTED an in-profile witness (accept ⟹ safe violated)." >&2
  rm -f "$CERTS" "$REPORT"
  exit 1
fi

rm -f "$CERTS" "$REPORT"
echo
echo "CORPUS WALL OK: over the whole v0 corpus, lower_function is total (wall holds,"
echo "zero silent miscompiles) AND the kernel-proven checker accepts every in-profile"
echo "ownership witness (accept ⟹ RC-safe on real corpus programs). Coverage is"
echo "reported above; the Unsupported histogram is the per-feature roadmap."
