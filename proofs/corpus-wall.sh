#!/usr/bin/env bash
# V0-CORPUS WALL GATE (step-4 "continuous corpus verification = the definition of
# parity", in its honest first form). The completion definition demands the
# proven profile ACCEPT the v0 corpus via the PCC chain and REJECT outside it.
# This gate runs the ENTIRE v0 spec corpus through the real frontend → MIR
# lowering and asserts the two soundness-relevant invariants empirically:
#
#   (1) THE WALL: `lower_function` is TOTAL over the corpus — every function is
#       `Ok` (in-profile) or an explicit `Unsupported` (walled). Zero panics,
#       zero UNDETECTED lowering refusals. NOTE (claim precision, F2-4): totality
#       + certificate acceptance is NOT an output-correctness claim — an Ok
#       function can still lower to WRONG OUTPUT within a sound cert (the
#       2026-07-03 match-linearization ran both println arms under a green wall).
#       Output correctness is the SEPARATE output-parity gate's claim, and only
#       for its baseline set.
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
# F6-2: identity of the evidence — stamp + verify the toolchain (see proofs/lib/stamp.sh).
source "$ROOT/proofs/lib/stamp.sh"
stamp_toolchain "$ROOT" || exit 1


CORPUS="${1:-$ROOT/spec}"

# The stdlib purity drift gate (brick #47): a stdlib `Module` call lowers into
# the subset only when its callee is provably PURE; this gate fails if a pure
# module ever gains a host effect (the accept-but-unsafe class). Run it FIRST —
# the corpus sweep below trusts `purity::is_pure`, so its registry must be sound.
echo "== stdlib purity drift gate (admit-set ⊆ keyword-pure, all modules classified) =="
bash "$ROOT/proofs/check-stdlib-purity-registry.sh"
echo

echo "== build the corpus classifier (real frontend → MIR lowering) =="
# Pre-existing workspace warnings are not this gate's concern — silence them so
# the honest wall report below is the only thing on screen.
(cd "$ROOT" && cargo build -q -p almide-mir --example classify_corpus) 2>/dev/null

echo "== sweep the v0 corpus: $CORPUS =="
OUTDIR="$(mktemp -d)"; REPORT="$(mktemp)"
cleanup() { rm -rf "$OUTDIR" "$REPORT"; }
set +e
(cd "$ROOT" && cargo run -q -p almide-mir --example classify_corpus -- --out "$OUTDIR" "$CORPUS") \
  2>"$REPORT"
WALL_RC=$?
set -e
# Show only the harness report (from its marker), dropping any cargo build noise.
sed -n '/== v0-corpus MIR-lowering wall report ==/,$p' "$REPORT"

if [ "$WALL_RC" -ne 0 ]; then
  echo "WALL GATE FAIL: lower_function breached totality (panicked) on a corpus function." >&2
  cleanup; exit 1
fi

# Anti-collapse floor: at least one in-profile program must reach the checker, so
# the PCC chain is genuinely exercised (a corpus where NOTHING lowers would pass
# the wall vacuously — that silent coverage collapse must fail the gate).
if [ ! -s "$OUTDIR/ownership.cert" ]; then
  echo "WALL GATE FAIL: no in-profile heap object reached the checker (coverage collapsed to 0)." >&2
  cleanup; exit 1
fi

# WALLED-REAL RATCHET (ENDGAME baseline, 2026-07-14): the wall=0 metric reached
# ZERO — every real corpus function lowers, witnesses, and kernel-ACCEPTs
# (docs/roadmap: v1-wall-histogram-goal, 112 -> 0). It may never regress: a change
# that re-walls ANY real corpus function must ship its lowering (or move the
# fixture out of the corpus with justification) in the SAME change.
WALLED_REAL="$(sed -n 's/^.*walled real (lowering)[[:space:]]*: *\([0-9][0-9]*\).*$/\1/p' "$REPORT" | head -1)"
if [ -z "$WALLED_REAL" ]; then
  echo "WALL GATE FAIL: could not read the walled-real count from the classify report (format drift?)." >&2
  cleanup; exit 1
fi
if [ "$WALLED_REAL" -ne 0 ]; then
  echo "WALLED-REAL RATCHET FAIL: $WALLED_REAL corpus function(s) walled — the ENDGAME baseline is 0." >&2
  echo "  List them:  WALL_NAMES=1 cargo run -q --release -p almide-mir --example classify_corpus -- --out /tmp/cw spec 2>&1 | grep '^WALLED REAL'" >&2
  cleanup; exit 1
fi
echo "WALLED-REAL RATCHET OK: 0 walled real corpus functions (the ENDGAME floor holds)."

echo
echo "== build the kernel-proven checker (from the Coq proof) =="
./build-checker.sh >/dev/null

# accept ⟹ safe for the FULL proven property set over real corpus programs: the
# kernel-proven checker re-verifies EVERY in-profile witness in each of its three
# modes (ownership = no double-free/leak; names = no dangling MIR ref; caps = no
# undeclared host effect).
#
# WITNESS GRANULARITY (a structural fact this 3-property extension surfaced): the
# ownership checker (`check_cert`) FOLDS over heap objects — one per line — so the
# whole ownership.cert is verified in ONE pass. But the name/capability checkers
# (`check_names_cert` / `check_caps_cert`) parse the WHOLE input as a SINGLE
# `<superset>|<subset>` witness (no line split) — each is a per-FUNCTION property.
# So names/caps are verified one function (one line) at a time; a batched file
# would wrongly fold every function's ids into one superset. accept on every line
# ⟹ the property holds of every in-profile function.
echo "== PCC chain: the proven checker re-verifies EVERY in-profile witness (3 properties) =="

# Ownership: a single fold over all heap objects (the checker splits by line).
OWN_N=$(wc -l < "$OUTDIR/ownership.cert" | tr -d ' ')
set +e
./checker ownership "$OUTDIR/ownership.cert" >/tmp/corpus-wall.checker.out 2>&1
OWN_RC=$?
set -e
echo "  [ownership] $OWN_N heap object(s) (no double-free / no leak): $(cat /tmp/corpus-wall.checker.out) (exit $OWN_RC)"
if [ "$OWN_RC" -ne 0 ]; then
  echo "WALL GATE FAIL: proven checker REJECTED an in-profile [ownership] witness (accept ⟹ safe violated)." >&2
  cleanup; exit 1
fi

# Names / caps: one proven-checker invocation per function (per witness line).
check_per_function() { # property cert-file human-meaning
  local prop="$1" cert="$2" meaning="$3"
  local n=0 line one
  one="$(mktemp)"
  while IFS= read -r line; do
    n=$((n + 1))
    printf '%s\n' "$line" > "$one"
    set +e
    ./checker "$prop" "$one" >/dev/null 2>&1
    local rc=$?
    set -e
    if [ "$rc" -ne 0 ]; then
      echo "  [$prop] FUNCTION $n REJECTS: witness '$line'"
      echo "WALL GATE FAIL: proven checker REJECTED an in-profile [$prop] witness (accept ⟹ safe violated)." >&2
      rm -f "$one"; cleanup; exit 1
    fi
  done < "$cert"
  rm -f "$one"
  echo "  [$prop] $n function witness(es) ($meaning): ACCEPT (exit 0)"
}
check_per_function names "$OUTDIR/names.cert" "no dangling MIR reference"
# Caps: only the witnesses of functions PROVABLY Stdout-free TRANSITIVELY are
# emitted (the classifier's conservative `reaches_capability_or_unknown` fold —
# a function calling an unanalyzable callee is reported caps-unverified, never
# claimed safe). The modeled capability is Stdout only, so the honest property is
# "no undeclared STDOUT effect" (stderr / abort / fs / net are real host effects
# not yet named — a wider Capability set is a later brick).
check_per_function caps  "$OUTDIR/caps.cert"  "no undeclared Stdout effect, transitive"
# Caps (TRANSITIVE, fold IN-PROOF): for fully-analyzable files the classifier emits the call
# GRAPH (one line per file: `declared|direct|callee-indices` per function) and the proven
# `check_prog_cert` (CapabilityReach) COMPUTES the transitive reach itself — the reachability
# fold is no longer trusted Rust. A program whose function reaches an undeclared capability,
# even via a callee, is REJECTED. This is the stronger sibling of the per-function caps gate.
check_per_function caps-transitive "$OUTDIR/caps_graph.cert" "transitive reach ⊆ declared, fold in-proof (per program)"

echo
echo "== KERNEL ORACLE (brick 6b): the Rocq KERNEL re-verifies the ENTIRE corpus witness set =="
# One generated assertion file over ALL witnesses (ownership = one multi-line
# cert fold; names/caps/tcaps = forallb over the per-function witnesses), coqc'd
# with vm_compute: the KERNEL itself certifies every verdict the extracted binary
# just produced. Divergence fails the build — so the extraction pipeline (OCaml
# codegen + ocamlopt) is a FAST PATH, no longer a trust root for the corpus gate
# (TRUSTED_BASE §2, brick 6). Measured ~4 min on the full 27k-object corpus.
KGEN="$(mktemp /tmp/KernelCorpus_XXXXXX).v"
python3 - "$OUTDIR" > "$KGEN" <<'PYEOF'
import sys, os
out = sys.argv[1]
def lit(s):
    return '"' + s.replace('"', '""') + '"'
def lines(name):
    return [l for l in open(os.path.join(out, name)).read().splitlines() if l.strip()]
own = open(os.path.join(out, 'ownership.cert')).read()
print("From AlmideTrust Require Import OwnershipChecker NameTotality CapabilityBound CapabilityReach.")
print("From Stdlib Require Import String List.")
print("Import ListNotations.")
print("Open Scope string_scope.")
print("Goal check_bc %s = true." % lit(own))
print("Proof. vm_compute. reflexivity. Qed.")
print("Goal forallb check_names_cert [%s] = true." % ";".join(lit(w) for w in lines('names.cert')))
print("Proof. vm_compute. reflexivity. Qed.")
print("Goal forallb check_caps_cert [%s] = true." % ";".join(lit(w) for w in lines('caps.cert')))
print("Proof. vm_compute. reflexivity. Qed.")
print("Goal forallb check_prog_cert [%s] = true." % ";".join(lit(w) for w in lines('caps_graph.cert')))
print("Proof. vm_compute. reflexivity. Qed.")
PYEOF
KSTART=$(date +%s)
if (cd "$ROOT/proofs" && "${COQC:-$(command -v coqc)}" -Q . AlmideTrust "$KGEN" >/dev/null 2>&1); then
  echo "  KERNEL OK: all four corpus witness sets certified by the Rocq kernel in $(( $(date +%s) - KSTART ))s (vm_compute)"
  rm -f "$KGEN" "${KGEN%.v}.vo" "${KGEN%.v}.vos" "${KGEN%.v}.vok" "${KGEN%.v}.glob"
else
  echo "WALL GATE FAIL: the KERNEL rejected a corpus witness the binary accepted (extraction divergence)." >&2
  rm -f "$KGEN"; exit 1
fi

cleanup
echo
echo "CORPUS WALL OK: over the whole v0 corpus, lower_function is total (wall holds,"
echo "zero panics, zero undetected refusals) AND the kernel-proven checker accepts every in-profile"
echo "witness on ALL THREE proven properties (accept ⟹ ownership ∧ name-totality ∧"
echo "capability-bound, on real corpus programs) — every verdict ALSO certified by the"
echo "Rocq KERNEL itself (the kernel oracle; extraction is a fast path, not a trust"
echo "root). Coverage is reported above; the Unsupported histogram is the roadmap."
