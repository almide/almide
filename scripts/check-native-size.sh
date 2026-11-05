#!/usr/bin/env bash
# NATIVE BINARY SIZE RATCHET (#2092).
#
# `almide build --release` of a program that touches the heap links the
# precompiled runtime rlib with a bare rustc (the fast path in
# src/cli/cargo_build.rs). Cargo strips debug info from a release binary by
# default; a bare rustc does not, so std's DWARF rode along and every such
# binary shipped at ~2.0 MB where the code was ~0.4 MB — while hello (built
# through cargo) stayed at 0.37 MB, so nothing that measured hello saw it.
#
# What it checks, per program in PROGRAMS (all heap-using):
#   1. the release binary carries no `.debug_*` section,
#   2. its size stays under scripts/native-size-baseline.txt + BUDGET_PCT, and
#   3. it defines no function twice (#4497). One fat-LTO module never does;
#      the `--release` backtrace split (src/cli/std_split.rs) links a second
#      object, and a std function both it and the LTO module carry a private
#      copy of is linked twice — 20-32 KB per binary when the split shipped
#      (#4459). The only names allowed twice are the allocator shim's
#      (`__rustc::__rust_alloc` and kin): rustc generates that shim into every
#      link, so the object cannot reference the program's.
#      Negative control: a build that duplicates (the split before #4497, or
#      a split whose sharing step is skipped) lists ~84 names here and fails.
# A size below 80% of the baseline also fails: a shrink that large is a build
# that lost something until proven otherwise. A deliberate change (a runtime
# that grew or shrank on purpose) rewrites the baseline in the SAME change:
#   NATIVE_SIZE_WRITE_BASELINE=1 scripts/check-native-size.sh
#
# Linux x86_64 only: the baseline is bytes of an ELF built by the toolchain
# CI's perf job installs. Elsewhere the script says so and exits 0.
set -euo pipefail
export LC_ALL=C
cd "$(git rev-parse --show-toplevel)"

ALMIDE="${ALMIDE_BIN:-almide}"
BASELINE_FILE="scripts/native-size-baseline.txt"
BUDGET_PCT="${NATIVE_SIZE_BUDGET_PCT:-10}"
PROGRAMS=(binarytrees jsonround strchurn wordfreq)

if [ "$(uname -s)-$(uname -m)" != "Linux-x86_64" ]; then
  echo "native-size: baseline is Linux x86_64 bytes; skipped on $(uname -s)-$(uname -m)"
  exit 0
fi
command -v readelf >/dev/null || { echo "::error::native-size: readelf not found"; exit 1; }
command -v nm >/dev/null || { echo "::error::native-size: nm not found"; exit 1; }

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

declare -A size
fail=0
for p in "${PROGRAMS[@]}"; do
  src="research/benchmark/perf/$p/$p.almd"
  [ -f "$src" ] || { echo "::error::native-size: $src is missing"; exit 1; }
  if ! "$ALMIDE" build --release "$src" -o "$work/$p" >"$work/$p.log" 2>&1; then
    cat "$work/$p.log"
    echo "::error::native-size: almide build --release $src failed"
    exit 1
  fi
  size[$p]=$(stat -c %s "$work/$p")
  if readelf -S -W "$work/$p" | grep -q ' \.debug_'; then
    echo "::error::native-size: $p ships debug sections:"
    readelf -S -W "$work/$p" | grep ' \.debug_' | awk '{print "  " $2 "  0x" $6}'
    echo "A release binary carries no debug info (the cargo release profile strips it;"
    echo "the rlib fast path passes \`-C strip=debuginfo\` — FAST_PATH_RELEASE_ARGS)."
    fail=1
  fi
  twice=$(nm --defined-only "$work/$p" | awk '$2 ~ /^[tTwW]$/ { print $3 }' | sort | uniq -d \
    | grep -Ev '7___rustc[0-9]+___rust_(alloc|dealloc|realloc|alloc_zeroed)$' || true)
  if [ -n "$twice" ]; then
    echo "::error::native-size: $p defines $(echo "$twice" | wc -l) function(s) twice:"
    echo "$twice" | head -20 | sed 's/^/  /'
    echo "A std function the backtrace split's object and the LTO module both carry is linked"
    echo "twice (#4497): src/cli/std_split.rs shares it (moved into the object, or referenced"
    echo "in the program) — find why this one was left out (two definitions in std's bitcode,"
    echo "an alias, or a seed that does not reach it)."
    fail=1
  fi
done

if [ "${NATIVE_SIZE_WRITE_BASELINE:-}" = "1" ]; then
  for p in "${PROGRAMS[@]}"; do printf '%s %s\n' "$p" "${size[$p]}"; done > "$BASELINE_FILE"
  echo "native-size: wrote $BASELINE_FILE"
  cat "$BASELINE_FILE"
  exit "$fail"
fi

for p in "${PROGRAMS[@]}"; do
  base=$(awk -v p="$p" '$1 == p { print $2 }' "$BASELINE_FILE")
  if [ -z "$base" ]; then
    echo "::error::native-size: $p has no row in $BASELINE_FILE"
    fail=1
    continue
  fi
  ceiling=$(( base + base * BUDGET_PCT / 100 ))
  floor=$(( base * 80 / 100 ))
  printf 'native-size: %-12s %9s bytes (baseline %s, ceiling %s)\n' "$p" "${size[$p]}" "$base" "$ceiling"
  if [ "${size[$p]}" -gt "$ceiling" ]; then
    echo "::error::native-size: $p release binary ${size[$p]} bytes exceeds the $ceiling ceiling (+$BUDGET_PCT% over $base)."
    echo "Find what grew (\`nm -S --size-sort -C\`, \`readelf -S -W\`). If the growth is on purpose,"
    echo "rewrite the baseline in the same change: NATIVE_SIZE_WRITE_BASELINE=1 $0"
    fail=1
  elif [ "${size[$p]}" -lt "$floor" ]; then
    echo "::error::native-size: $p release binary ${size[$p]} bytes is below $floor (80% of $base)."
    echo "A drop this size is a build that lost something until proven otherwise. If the"
    echo "shrink is real, rewrite the baseline in the same change: NATIVE_SIZE_WRITE_BASELINE=1 $0"
    fail=1
  fi
done
exit "$fail"
