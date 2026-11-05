#!/usr/bin/env bash
# RELEASE BUILD-TIME RATCHET (#4459).
#
# `almide build --release` links with fat LTO. Three quarters of what that
# LTO module held after optimization was std's backtrace printer (gimli,
# addr2line, the demangler), re-optimized and re-code-generated on every
# build; the backtrace split (src/cli/std_split.rs) links it precompiled
# instead. This gate holds the win: per program, the median release build
# time with the split against the same build with the plain fat-LTO profile
# (`ALMIDE_RELEASE_STD_UNSPLIT=1`), interleaved, every build with a fresh
# constant so the IR-keyed build cache never answers. A ratio, not seconds:
# the runner's speed cancels.
#
# Measured 2026-11-05, GCE c4-standard-8, rustc 1.99.1, median of 7:
#   fib (the cargo route: no runtime)      2.16 s -> 0.80 s  (0.37)
#   jsonround (the runtime-rlib route)     3.01 s -> 1.62 s  (0.54)
# The binaries run the same instructions (cachegrind: equal to 0.003%); what
# the split moves is the code layout, measured in #4459 against 16 shuffled
# layouts of each profile.
#
# Negative control: with ALMIDE_RELEASE_STD_UNSPLIT=1 in the environment
# both legs are the plain profile, the ratio reads ~1.0 and this gate must
# fail (CI runs it that way too).
#
# Needs the `llvm-tools` rustup component (the split's one-time transform);
# Linux x86_64 only, elsewhere it says so and exits 0.
set -euo pipefail
export LC_ALL=C
cd "$(git rev-parse --show-toplevel)"

ALMIDE="${ALMIDE_BIN:-almide}"
ROUNDS="${RELEASE_BUILD_ROUNDS:-5}"
# Ceilings sit well above the measured ratios (0.37 / 0.54) and well below the
# ~1.0 a build that lost the split reads.
declare -A CEILING=([fib]=0.60 [jsonround]=0.75)

if [ "$(uname -s)-$(uname -m)" != "Linux-x86_64" ]; then
  echo "release-build-time: the split is Linux x86_64 only; skipped on $(uname -s)-$(uname -m)"
  exit 0
fi
sysroot=$(rustc --print sysroot)
host=$(rustc -vV | sed -n 's/^host: //p')
for t in opt llc llvm-dis llvm-as llvm-objcopy llvm-ar; do
  if [ ! -x "$sysroot/lib/rustlib/$host/bin/$t" ]; then
    echo "::error::release-build-time: $t missing — rustup component add llvm-tools"
    exit 1
  fi
done

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
# Fresh caches: the first build of each leg pays the runtime rlib and the
# split's one-time transform, and is the untimed warm-up below.
mkdir -p "$work/tmp"
export TMPDIR="$work/tmp"

cat > "$work/fib.src" <<'EOF'
fn fib(n: Int) -> Int = if n < 2 then n else fib(n - 1) + fib(n - 2)
fn main() -> Unit = { let v = fib(25) + NONCE; println("${v}") }
EOF
sed 's/50000/NONCE/g' research/benchmark/perf/jsonround/jsonround.almd > "$work/jsonround.src"
grep -q NONCE "$work/jsonround.src" || { echo "::error::release-build-time: jsonround.almd lost its 50000 default"; exit 1; }

nonce=$(( $(date +%s) % 100000 + 100 ))
# build <program> <split|plain> [nonce] -> prints the wall seconds of one release build
# (a fresh nonce unless one is given)
build() {
  local p=$1 leg=$2
  if [ -n "${3:-}" ]; then local n=$3; else nonce=$((nonce + 1)); local n=$nonce; fi
  local src="$work/$p-$n.almd"
  sed "s/NONCE/$n/g" "$work/$p.src" > "$src"
  local -a env=()
  [ "$leg" = plain ] && env=(ALMIDE_RELEASE_STD_UNSPLIT=1)
  local t0 t1
  t0=$(date +%s.%N)
  if ! env "${env[@]}" "$ALMIDE" build --release "$src" -o "$work/$p-$leg" > "$work/build.log" 2>&1; then
    cat "$work/build.log"
    echo "::error::release-build-time: almide build --release $p ($leg) failed"
    exit 1
  fi
  t1=$(date +%s.%N)
  echo "$t1 - $t0" | bc
}

median() { sort -g | awk '{ a[NR] = $1 } END { print (NR % 2) ? a[(NR + 1) / 2] : (a[NR / 2] + a[NR / 2 + 1]) / 2 }'; }

fail=0
for p in fib jsonround; do
  build "$p" split > /dev/null
  build "$p" plain > /dev/null
  : > "$work/$p.split"; : > "$work/$p.plain"
  for r in $(seq 1 "$ROUNDS"); do
    if [ $((r % 2)) = 1 ]; then order="split plain"; else order="plain split"; fi
    for leg in $order; do build "$p" "$leg" >> "$work/$p.$leg"; done
  done
  # One program built both ways prints the same (the build cache is keyed on
  # ALMIDE_RELEASE_STD_UNSPLIT, so the second leg is a build, not a hit).
  nonce=$((nonce + 1))
  build "$p" split "$nonce" > /dev/null
  build "$p" plain "$nonce" > /dev/null
  if ! cmp -s <("$work/$p-split" 2>&1) <("$work/$p-plain" 2>&1); then
    echo "::error::release-build-time: $p prints differently built with and without the split"
    fail=1
  fi
  s=$(median < "$work/$p.split"); b=$(median < "$work/$p.plain")
  ratio=$(echo "scale=3; $s / $b" | bc)
  printf 'release-build-time: %-10s split %6.3f s  plain %6.3f s  ratio %s (ceiling %s)\n' "$p" "$s" "$b" "$ratio" "${CEILING[$p]}"
  if [ "$(echo "$ratio > ${CEILING[$p]}" | bc)" = 1 ]; then
    echo "::error::release-build-time: $p builds at $ratio of the plain fat-LTO time, over the ${CEILING[$p]} ceiling."
    echo "Run with ALMIDE_DEBUG_STD_SPLIT=1 to see why the backtrace split was not used (src/cli/std_split.rs)."
    fail=1
  fi
done
exit "$fail"
