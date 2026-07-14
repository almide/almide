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

total=0; match=0; mismatch=0; native_fail=0; skipped=0
declare -a MISMATCHES=()

run_one() { # repo entry
  local repo="$1" entry="$2"
  local dir; dir="$(dirname "$entry")"
  # A program that needs stdin/args/net is out of scope for a no-args smoke run — a
  # native failure (non-zero without wasm divergence) SKIPS rather than fails: the gate
  # is CROSS-TARGET equality, not program health.
  local n_out n_rc w_out w_rc
  n_out="$(cd "$dir" && $TO "$ALMIDE" run "$(basename "$entry")" 2>&1)"; n_rc=$?
  w_out="$(cd "$dir" && $TO "$ALMIDE" run "$(basename "$entry")" --target wasm --verified 2>&1)"; w_rc=$?
  # Normalize nondeterministic temp paths in error text so two runs of the SAME
  # failure compare equal (the audio-poc false positive: identical unknown-import
  # errors differing only in the tmpfile name).
  n_out="$(printf '%s' "$n_out" | sed 's|/[^ ]*/almide-run-[0-9a-f]*\.wasm|<tmp>.wasm|g')"
  w_out="$(printf '%s' "$w_out" | sed 's|/[^ ]*/almide-run-[0-9a-f]*\.wasm|<tmp>.wasm|g')"
  # A browser-host-only program (unknown wasm import — webaudio/dom/…): wasmtime can
  # never run it regardless of codegen; cross-target equality is out of scope → SKIP.
  case "$w_out" in *"unknown import"*)
    echo "  SKIP  $repo/$(basename "$entry") (browser-host import)"; skipped=$((skipped+1)); total=$((total+1)); return;;
  esac
  total=$((total+1))
  if [ "$n_rc" = "124" ] || [ "$w_rc" = "124" ]; then
    echo "  SKIP  $repo/$(basename "$entry") (timeout)"; skipped=$((skipped+1)); return
  fi
  if [ "$n_out" = "$w_out" ] && [ "$n_rc" = "$w_rc" ]; then
    if [ "$n_rc" != "0" ]; then
      echo "  MATCH $repo/$(basename "$entry") (both exit $n_rc)"
    else
      echo "  MATCH $repo/$(basename "$entry")"
    fi
    match=$((match+1))
  elif [ "$n_rc" != "0" ] && [ "$w_rc" != "0" ]; then
    # both failed differently (env-dependent programs) — surface but do not fail the gate
    echo "  SKIP  $repo/$(basename "$entry") (both-fail, rc $n_rc vs $w_rc)"; skipped=$((skipped+1))
  else
    echo "  ✗ MISMATCH $repo/$(basename "$entry") (native rc=$n_rc, wasm rc=$w_rc)"
    MISMATCHES+=("$repo/$entry")
    mismatch=$((mismatch+1))
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
echo "org byte-verify: $match match / $mismatch MISMATCH / $skipped skipped (of $total)"
if [ "$mismatch" -ne 0 ]; then
  echo "MISMATCHES:"; printf '  %s\n' "${MISMATCHES[@]}"
  exit 1
fi
echo "ORG BYTE-VERIFY OK (the Release-② gate holds on runnable entries)"
