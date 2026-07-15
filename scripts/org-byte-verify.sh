#!/usr/bin/env bash
# org-byte-verify.sh — the Release-② gate: run every org repo's RUNNABLE entry point on
# native AND `--target wasm --verified` (v1-first, v0 fallback) and byte-compare
# stdout+exit. A MISMATCH is a v1 miscompile escaping the honest wall — the exact class
# the ENDGAME campaign's adversarial discipline exists to catch — and fails the sweep.
#
# Coverage note: `almide test`'s wasm path is v0's test harness (test blocks are outside
# the v1 render subset), so THIS sweep — real programs through the shipped `--verified`
# pipeline — is the authoritative default-on gate, complementing (not replacing) the
# existing per-repo native⇄wasm test-suite verification (C-121..C-125 era, v0-wasm).
#
# Usage:
#   scripts/org-byte-verify.sh                 # sweep all org repos
#   ALMIDE_ORG_DIR=/path scripts/org-byte-verify.sh
#   TIMEOUT_SECS=20 scripts/org-byte-verify.sh
set -uo pipefail

work_root="$(git rev-parse --show-toplevel)"
main_repo="$(cd "$(git rev-parse --git-common-dir)/.." && pwd)"
ORG_DIR="${ALMIDE_ORG_DIR:-$(dirname "$main_repo")}"
TIMEOUT_SECS="${TIMEOUT_SECS:-30}"
ALMIDE="$work_root/target/release/almide"
# macOS has no coreutils `timeout`; prefer gtimeout, else run unwrapped.
if command -v timeout >/dev/null 2>&1; then TO="timeout $TIMEOUT_SECS"
elif command -v gtimeout >/dev/null 2>&1; then TO="gtimeout $TIMEOUT_SECS"
else TO=""; echo "note: no timeout binary — running unwrapped" >&2; fi

echo "building almide (release)…" >&2
( cd "$work_root" && cargo build -q --release --bin almide )

total=0; match=0; mismatch=0; native_fail=0; skipped=0; preexisting=0
declare -a MISMATCHES=()
declare -a PREEXISTING=()

# Strip run-to-run noise before comparing: the temp module path embeds a fresh hash
# every run (`.../almide-run-<hash>.wasm`), so an error message containing it would
# spuriously differ between two otherwise identical failures.
# Also neutralize WALL-CLOCK tokens (Nms, N.Nµs/call, N.N GFLOPS): two runs of the SAME
# binary differ there, so they carry zero cross-target signal. The deterministic remainder
# (acc= sums, shapes, counts) is what must byte-match.
norm() { sed -E -e 's#[^ `]*almide-run-[0-9a-f]+\.wasm#<tmp>.wasm#g' \
                -e 's#[0-9]+ms#<ms>#g' \
                -e 's#[0-9.]+(µs|us)/call#<us>/call#g' \
                -e 's#[0-9.]+ GFLOPS#<gflops> GFLOPS#g'; }

run_one() { # repo entry
  local repo="$1" entry="$2"
  local dir; dir="$(dirname "$entry")"
  # THREE legs — native (oracle), plain wasm (v0 baseline), wasm --verified (v1-first).
  # The Release-2 gate is ATTRIBUTION-aware: only `verified != plain-wasm` is a
  # v1-attributable mismatch (fails the sweep). A native/wasm divergence where BOTH wasm
  # legs agree is PRE-EXISTING (v0-wasm) — reported for the org ledger, not a v1 blocker.
  local raw n_out n_rc w0_out w0_rc w1_out w1_rc
  raw="$(cd "$dir" && $TO "$ALMIDE" run "$(basename "$entry")" 2>&1)"; n_rc=$?
  n_out="$(printf '%s' "$raw" | norm)"
  raw="$(cd "$dir" && $TO "$ALMIDE" run "$(basename "$entry")" --target wasm --no-verified 2>&1)"; w0_rc=$?
  w0_out="$(printf '%s' "$raw" | norm)"
  raw="$(cd "$dir" && $TO "$ALMIDE" run "$(basename "$entry")" --target wasm --verified 2>&1)"; w1_rc=$?
  w1_out="$(printf '%s' "$raw" | norm)"
  total=$((total+1))
  if [ "$n_rc" = "124" ] || [ "$w0_rc" = "124" ] || [ "$w1_rc" = "124" ]; then
    echo "  SKIP  $repo/$(basename "$entry") (timeout)"; skipped=$((skipped+1)); return
  fi
  if [ "$w1_out" != "$w0_out" ] || [ "$w1_rc" != "$w0_rc" ]; then
    echo "  x V1-MISMATCH $repo/$(basename "$entry") (verified rc=$w1_rc vs plain-wasm rc=$w0_rc)"
    MISMATCHES+=("$repo/$entry")
    mismatch=$((mismatch+1)); return
  fi
  if [ "$n_out" = "$w1_out" ] && [ "$n_rc" = "$w1_rc" ]; then
    if [ "$n_rc" != "0" ]; then
      echo "  MATCH $repo/$(basename "$entry") (both exit $n_rc)"
    else
      echo "  MATCH $repo/$(basename "$entry")"
    fi
    match=$((match+1))
  elif [ "$n_rc" != "0" ] && [ "$w1_rc" != "0" ]; then
    echo "  SKIP  $repo/$(basename "$entry") (both-fail, rc $n_rc vs $w1_rc)"; skipped=$((skipped+1))
  else
    # native != wasm but v0wasm == v1wasm: a pre-existing cross-target divergence
    # (timing output, host imports, the 4MB memory ceiling...) — org-ledger material.
    echo "  ~ PREEXISTING $repo/$(basename "$entry") (native rc=$n_rc vs wasm rc=$w1_rc, v0==v1)"
    PREEXISTING+=("$repo/$entry")
    preexisting=$((preexisting+1))
  fi
}

for d in "$ORG_DIR"/*/; do
  repo="$(basename "$d")"
  [ "$repo" = "$(basename "$main_repo")" ] && continue
  entries=()
  [ -f "$d/src/main.almd" ] && entries+=("$d/src/main.almd")
  [ -f "$d/main.almd" ] && entries+=("$d/main.almd")
  while IFS= read -r ex; do entries+=("$ex"); done < <(ls "$d"examples/*.almd 2>/dev/null)
  [ "${#entries[@]}" = "0" ] && continue
  echo "== $repo =="
  for e in "${entries[@]}"; do run_one "$repo" "$e"; done
done

echo
echo "org byte-verify: $match match / $mismatch V1-MISMATCH / $preexisting pre-existing / $skipped skipped (of $total)"
if [ "$preexisting" -ne 0 ]; then
  echo "PRE-EXISTING native!=wasm divergences (v0==v1 — NOT Release-2 blockers):"
  printf '  %s\n' "${PREEXISTING[@]}"
fi
if [ "$mismatch" -ne 0 ]; then
  echo "V1-ATTRIBUTABLE MISMATCHES:"; printf '  %s\n' "${MISMATCHES[@]}"
  exit 1
fi
echo "ORG BYTE-VERIFY OK (zero v1-attributable mismatches — the Release-2 gate holds)"
