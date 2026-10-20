#!/bin/bash
# E1 (lever L1) — rebuild and time the four columns R0 / A0 / A1 / R*.
#
#   ALMIDE=/path/to/almide-0.68.0-rc2 bash run.sh [RUNS]
#
# Needs: the almide release binary (A0 is whatever it emits), rustc, python3,
# and optionally `perf` for instructions:u. Run from anywhere; it builds into
# ./build next to this script and writes build/result-<set>.json per flag set.
#
# A0 is what `almide build --release` compiles: /tmp/almide-run/almide_gen_main.rs
# (the program, with the self-hosted stdlib it uses) linked against the native
# runtime crate almide writes to $TMPDIR/almide-rtlib-*/almide_rt.rs. This script
# rebuilds both with the column flags (the runtime as an rlib, the program with
# fat LTO across the two), the same two-crate shape almide uses. The one-crate
# form (ALMIDE_NO_RTLIB=1, runtime inlined into src/main.rs) was measured and
# rejected: on c4a it ran spectralnorm_enumerate 1.31 s vs the almide binary's
# 1.12 s, so it is not the program almide ships.
# A1 is A0 plus a1/<kernel>.patch (the hand-applied L1 facts).
# Every column is compiled by the same rustc with the same flags.
set -euo pipefail
HERE=$(cd "$(dirname "$0")" && pwd)
REPO=$(cd "$HERE/../../.." && pwd)
PERF="$REPO/research/benchmark/perf"
RUNS=${1:-9}
: "${ALMIDE:?set ALMIDE to the almide release binary}"
B="$HERE/build"; mkdir -p "$B"

# Kernel table: name, almide source, R0 source, R* source, run-time args.
KERNELS=(
  "spectralnorm|$PERF/spectralnorm/spectralnorm.almd|$PERF/rust-ref/spectralnorm.rs|$HERE/rstar/spectralnorm.rs|5500"
  "spectralnorm_enumerate|$PERF/spectralnorm/spectralnorm_enumerate.almd|$PERF/rust-ref/spectralnorm.rs|$HERE/rstar/spectralnorm.rs|5500"
  "spectralnorm_indexed|$PERF/spectralnorm/spectralnorm_indexed.almd|$PERF/rust-ref/spectralnorm.rs|$HERE/rstar/spectralnorm.rs|5500"
  # Probe: the indexed spelling with only fact F1 (the row index), not F2 (the fold counter).
  "spectralnorm_indexed_f1only|$PERF/spectralnorm/spectralnorm_indexed.almd|$PERF/rust-ref/spectralnorm.rs|$HERE/rstar/spectralnorm.rs|5500"
  "nbody|$PERF/nbody/nbody.almd|$PERF/rust-ref/nbody.rs|$HERE/rstar/nbody.rs|50000000"
  "idxloop|$HERE/idxloop.almd|$HERE/r0/idxloop.rs|$HERE/rstar/idxloop.rs|100000 5000"
)

# Almide's --release target CPU (src/cli/native_target.rs release_target_cpu):
# x86-64-v3 on x86_64, the generic CPU elsewhere. Almide's --release also passes
# -C llvm-args=-force-ordered-reductions=false; it is given to all columns.
case $(uname -m) in x86_64) PRIMARY_CPU=x86-64-v3; SECONDARY=(nocpu);; *) PRIMARY_CPU=; SECONDARY=(native);; esac
BASE=(-C opt-level=3 -C codegen-units=1 -C lto=fat -C panic=abort -C llvm-args=-force-ordered-reductions=false --edition 2021 --crate-name e1 -A warnings)

# 1. A0 (and the almide-built binary for the sanity check), A1 = A0 + patch.
for row in "${KERNELS[@]}"; do
  IFS='|' read -r k src r0 rs args <<<"$row"
  rm -rf /tmp/almide-run
  "$ALMIDE" build "$src" --release -o "$B/$k.almide" >/dev/null 2>&1
  cp /tmp/almide-run/almide_gen_main.rs "$B/$k.a0.rs"
  cp "$B/$k.a0.rs" "$B/$k.a1.rs"
  if [ -s "$HERE/a1/$k.patch" ]; then patch -s "$B/$k.a1.rs" < "$HERE/a1/$k.patch"; fi
done
RT=$(ls -t "${TMPDIR:-/tmp}"/almide-rtlib-*/almide_rt.rs | head -1)
cp "$RT" "$B/almide_rt.rs"

# 2. Compile every column under each flag set.
build_set() { # $1 = set name, $2 = target cpu ("" = none)
  local set=$1 cpu=$2 F=("${BASE[@]}")
  [ -n "$cpu" ] && F+=(-C "target-cpu=$cpu")
  mkdir -p "$B/$set/rt"; echo "${F[*]}" > "$B/$set/flags.txt"
  # the runtime crate: same flags (lto applies at the final link)
  local RTF=() skip=0
  for x in "${F[@]}"; do
    if [ $skip = 1 ]; then skip=0; continue; fi
    case $x in --crate-name) skip=1; continue;; lto=fat) unset 'RTF[${#RTF[@]}-1]'; continue;; esac
    RTF+=("$x")
  done
  rustc "${RTF[@]}" --crate-type lib --crate-name almide_rt "$B/almide_rt.rs" --out-dir "$B/$set/rt"
  local EXT=(--extern "almide_rt=$B/$set/rt/libalmide_rt.rlib" -L "$B/$set/rt")
  for row in "${KERNELS[@]}"; do
    IFS='|' read -r k src r0 rs args <<<"$row"
    rustc "${F[@]}" "$r0" -o "$B/$set/$k.R0"
    rustc "${F[@]}" "${EXT[@]}" "$B/$k.a0.rs" -o "$B/$set/$k.A0"
    rustc "${F[@]}" "${EXT[@]}" "$B/$k.a1.rs" -o "$B/$set/$k.A1"
    rustc "${F[@]}" "$rs" -o "$B/$set/$k.Rstar"
  done
}
build_set primary "$PRIMARY_CPU"
for s in "${SECONDARY[@]}"; do
  case $s in nocpu) build_set nocpu "";; native) build_set native native;; esac
done

# 3. Time: columns interleaved, RUNS rounds, median; output hash per column.
for set in primary "${SECONDARY[@]}"; do
  python3 "$HERE/bench.py" --dir "$B/$set" --runs "$RUNS" --almide-dir "$B" \
    $(for row in "${KERNELS[@]}"; do IFS='|' read -r k src r0 rs args <<<"$row"; printf -- '--kernel %s=%s ' "$k" "${args// /,}"; done) \
    --out "$B/result-$set.json"
done
