#!/usr/bin/env bash
# Developer entry to the generative differential fuzzer (#1490 item 4, ruling
# 2026-09-04: the fuzzer stays a repo tool — it needs the checkout's stdlib/
# and its own excluded crate, so it is not an `almide test --fuzz` an
# installed binary could serve). Builds tools/xtarget-fuzz and the compiler
# under test if needed, then runs one campaign with the nightly's flags.
#
#   bash scripts/fuzz.sh                 # 5 minutes, all families
#   bash scripts/fuzz.sh --minutes 20    # any xtarget-fuzz `run` flag passes through
#   bash scripts/fuzz.sh --count 200 --family identity
#   bash scripts/fuzz.sh replay --seed N --index I   # a non-run subcommand passes through as is
#
# Findings land under tools/xtarget-fuzz/findings/ (override with --out DIR).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
[ -x target/release/almide ] || cargo build --release --bin almide
(cd tools/xtarget-fuzz && cargo build --release -q)
FUZZ=tools/xtarget-fuzz/target/release/xtarget-fuzz
case "${1:-}" in
  run|replay|ladder|gen|stats|-h|--help|help) exec "$FUZZ" "$@" ;;
esac
args=("$@")
printf '%s\n' "${args[@]}" | grep -qE '^--(minutes|count)$' || args=(--minutes 5 "${args[@]}")
exec "$FUZZ" run --repo "$ROOT" --almide "$ROOT/target/release/almide" "${args[@]}"
