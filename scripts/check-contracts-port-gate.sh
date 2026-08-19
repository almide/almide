#!/usr/bin/env bash
# PORT GATE for the contract ledger during the greenfield port (ARCHITECTURE.md §4/§5).
# ==============================================================================
#
# Runs the UNMODIFIED incumbent gate (scripts/check-contracts.sh, ported verbatim
# from almide@a877d2138) and holds it to aviation-style deviation discipline:
#
#   - Every ::error:: it emits must be a FORWARD REFERENCE: a missing evidence /
#     cited path that is enumerated, one per line, in the deviation register
#     scripts/lib/port-deviations.txt (path <TAB> resolving-unit from §5).
#     Wildcards are not accepted; the register is the closed list.
#   - A registered path that HAS come to exist is a stale deviation — this gate
#     fails until the line is removed. The register may only SHRINK (ratchet).
#   - Any error that is not a registered forward reference — schema violation,
#     broken fixture<->contract symmetry, stale generated doc, coverage gap —
#     fails this gate exactly as it would fail upstream.
#   - gen-claims (--check) inside the incumbent gate needs proofs/*.v (unit 5).
#     Until then that single failure signature is deviation D-GENCLAIMS; any
#     OTHER gen-claims failure fails this gate.
#
# When the register reaches zero lines, this wrapper is deleted and CI calls
# scripts/check-contracts.sh directly.
set -uo pipefail
export LC_ALL=C
cd "$(dirname "$0")/.." || { echo "::error::cannot cd to repo root"; exit 2; }

REGISTER="scripts/lib/port-deviations.txt"
[ -f "$REGISTER" ] || { echo "::error::$REGISTER not found"; exit 2; }

# Ratchet ceiling: LOWER only, never raise. Set at unit #0 landing (96 paths).
MAX_DEVIATIONS=67
n_reg="$(grep -cvE '^[[:space:]]*(#|$)' "$REGISTER")"
fail=0
err() { echo "::error::$1"; fail=1; }

[ "$n_reg" -le "$MAX_DEVIATIONS" ] || \
  err "deviation register has $n_reg entries, ceiling is $MAX_DEVIATIONS — the register may only shrink"

# ── stale deviations: a registered path that exists must be de-registered ────
while IFS=$'\t' read -r path unit; do
  [ -z "$path" ] && continue
  case "$path" in \#*) continue ;; esac
  [ -e "$path" ] && err "stale deviation: '$path' exists now — remove it from $REGISTER (shrink ratchet)"
done < "$REGISTER"

# ── run the incumbent gate verbatim ─────────────────────────────────────────
out="$(bash scripts/check-contracts.sh 2>&1)"
rc=$?

fired=0
unexplained=0
while IFS= read -r line; do
  case "$line" in
    *"::error::"*) ;;
    *) continue ;;
  esac
  msg="${line#*::error::}"
  # The generic trailer that accompanies any failure — not a finding itself.
  [ "$msg" = "contract-ledger gate FAILED — see messages above." ] && continue
  # Extract the missing path from the two forward-reference shapes.
  p=""
  case "$msg" in
    *"evidence path does not exist: "*) p="${msg##*evidence path does not exist: }" ;;
    *"cited path '"*) p="${msg#*cited path \'}"; p="${p%%\'*}" ;;
  esac
  if [ -n "$p" ] && awk -F'\t' -v p="$p" '$1==p{found=1} END{exit !found}' "$REGISTER"; then
    fired=$((fired + 1))
  else
    err "unexplained gate error (not a registered forward reference): $msg"
    unexplained=$((unexplained + 1))
  fi
done <<< "$out"

# ── D-GENCLAIMS: the one tolerated non-path failure, pinned to its signature ─
if ! genout="$(bash scripts/gen-claims.sh --check 2>&1)"; then
  if ! printf '%s' "$genout" | grep -q "proofs/\*\.v: No such file"; then
    err "gen-claims failed for a reason other than deviation D-GENCLAIMS (missing proofs/*.v, unit 5): $genout"
  fi
fi

if [ "$rc" -eq 0 ] && [ "$n_reg" -gt 0 ]; then
  err "incumbent gate is fully green but $n_reg deviations remain registered — empty the register and delete this wrapper"
fi

echo "----"
echo "port-gate: incumbent gate rc=$rc; $fired forward-reference finding(s) matched the register ($n_reg registered), $unexplained unexplained."
if [ "$fail" -eq 0 ]; then
  echo "port-gate: GREEN — every failure is a registered forward reference to an unported unit."
  exit 0
fi
echo "::error::port-gate FAILED — see messages above."
exit 1
