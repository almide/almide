#!/usr/bin/env bash
# DOC-FENCE GATE (#1490 item 1 — the doctest class)
# =================================================
#
# The stdlib docs are hand-written prose, and a documented behavior that
# drifts from the implementation is a training-corpus bug models learn
# from (the `regex.captures` drift, almide#1432, broke a downstream tool).
# This gate makes documented examples EXECUTABLE:
#
#   ```almd check     — the fence is a complete program; `almide check`
#                       must accept it (the LLM-surface gate's contract,
#                       extended to docs/stdlib/).
#   ```almd run       — the fence is a complete program AND the next
#                       ```output fence pins its stdout BYTE-EXACTLY.
#                       Runs on the embedded wasm host (`almide run
#                       --target wasm` — rustc-free, deterministic).
#
# Plain ```almd stays a highlighting label for fragments, deliberate
# ✗-examples and idiom halves — out of scope on purpose; the gate checks
# the promise, not the prose. Growing coverage means completing an
# example and promoting its marker, one fence at a time.
#
# Usage: check-doc-fences.sh    (uses target/release/almide or PATH)
set -euo pipefail
export LC_ALL=C

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BIN="${ALMIDE_BIN:-}"
if [ -z "$BIN" ]; then
  if [ -x "$ROOT/target/release/almide" ]; then BIN="$ROOT/target/release/almide"; else BIN="almide"; fi
fi

TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

fail=0
checked=0
ran=0
for f in "$ROOT"/docs/stdlib/*.md; do
  base=$(basename "$f")
  # Split the file into numbered fences with their kind:
  #   check.N.almd / run.N.almd / run.N.out
  awk -v out="$TMP/$base" '
    /^```almd check[ \t]*$/  { kind="check"; n++; on=1; next }
    /^```almd run[ \t]*$/    { kind="run"; n++; on=1; next }
    /^```output[ \t]*$/      { if (lastrun) { kind="out"; on=1; next } }
    /^```/                   { if (on && kind=="run") lastrun=n; else if (on) lastrun=0; on=0; next }
    on && kind=="check"      { print > (out ".check." n ".almd") }
    on && kind=="run"        { print > (out ".run." n ".almd") }
    on && kind=="out"        { print > (out ".run." lastrun ".out") }
  ' "$f"

  for snip in "$TMP/$base".check.*.almd; do
    [ -e "$snip" ] || continue
    checked=$((checked + 1))
    if ! "$BIN" check "$snip" > "$TMP/out.txt" 2>&1; then
      echo "FAIL: $base check-fence $(basename "$snip") does not check:"
      sed 's/^/  /' "$TMP/out.txt" | head -12
      fail=1
    fi
  done

  for snip in "$TMP/$base".run.*.almd; do
    [ -e "$snip" ] || continue
    ran=$((ran + 1))
    want="${snip%.almd}.out"
    if [ ! -e "$want" ]; then
      echo "FAIL: $base run-fence $(basename "$snip") has no output fence after it"
      fail=1
      continue
    fi
    if ! "$BIN" run "$snip" --target wasm > "$TMP/got.txt" 2> "$TMP/err.txt"; then
      echo "FAIL: $base run-fence $(basename "$snip") did not run:"
      sed 's/^/  /' "$TMP/err.txt" | head -12
      fail=1
      continue
    fi
    if ! cmp -s "$TMP/got.txt" "$want"; then
      echo "FAIL: $base run-fence $(basename "$snip") output drifted from its output fence:"
      diff "$want" "$TMP/got.txt" | sed 's/^/  /' | head -12
      fail=1
    fi
  done
done

if [ "$checked" -eq 0 ] && [ "$ran" -eq 0 ]; then
  echo "FAIL: no marked fences found under docs/stdlib/ — the gate went blind (#976 class)" >&2
  exit 1
fi
echo "doc-fences: $checked check fence(s), $ran run fence(s) verified across docs/stdlib/"
exit $fail
