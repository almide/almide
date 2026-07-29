#!/usr/bin/env bash
# Determinism/Purity Belt — construction layer, tier T1.
#
# The compiler runs compiled to wasm32-unknown-unknown in the browser playground,
# where std::time / threads are unsupported (they PANIC), and where any output
# that depends on a clock/RNG/thread-schedule is also non-deterministic. These
# sources can NEVER legitimately reach the codegen path. Forbid them by grep so a
# violation fails CI instead of crashing the in-browser compiler — the bug that
# broke the playground (an unconditional std::time::Instant::now in codegen).
#
# Timing for ALMIDE_PROFILE must go through the one sanctioned, wasm-safe shim:
# almide_base::profile::ProfileTimer (cfg-gated to no-op on wasm32).
set -uo pipefail
cd "$(dirname "$0")/.."

# Crates on the in-browser compile path (parse → check → lower → mono →
# codegen → the v1 MIR render), plus the types/lang layers everything shares.
# almide-base is deliberately OUT: it is where the one sanctioned wasm-safe
# shim (almide_base::profile::ProfileTimer) lives, so it legitimately names
# the forbidden APIs. The scope previously covered 4 of these 8 crates while
# the header claimed the whole path (#986).
SCOPE="crates/almide-syntax/src crates/almide-frontend/src crates/almide-types/src crates/almide-lang/src crates/almide-ir/src crates/almide-optimize/src crates/almide-codegen/src crates/almide-mir/src"

# A vanished scope directory is a broken scan, not a clean result (#986/#976
# class — the old `2>/dev/null` on the grep would have hidden a reorganized
# path forever). Assert existence and a scanned-file floor first.
for d in $SCOPE; do
  [ -d "$d" ] || { echo "::error::forbidden-impurities: scope dir $d does not exist — the compile path moved; update SCOPE (#986)"; exit 1; }
done
scanned=$(find $SCOPE -name '*.rs' | wc -l | tr -d ' ')
[ "$scanned" -ge 100 ] || { echo "::error::forbidden-impurities: only $scanned .rs files in scope — the scan went blind (#986)"; exit 1; }
# Forbidden source tokens: clocks, threads, RNG, and never-reset atomic counters
# (the egg fresh-var counter class) — all non-deterministic or wasm32-unsupported.
PATTERN='std::time|Instant::now|SystemTime|thread::spawn|::spawn_blocking|thread_rng|fastrand|rand::random|AtomicU64|AtomicUsize|fetch_add'

# Strip line comments before matching so doc/comments that NAME the forbidden API
# (like this file's own references) don't trip it; ignore #[cfg(test)] modules is
# left to reviewers (tests don't reach the wasm build).
hits=$(grep -rnE "$PATTERN" $SCOPE \
  | grep -vE '/generated/|/runtime/|rust_runtime\.rs|rt_datetime' \
  | grep -vE '^\s*[^:]+:[0-9]+:\s*//' \
  | grep -vE '//.*(forbidden|sanctioned|shim|ProfileTimer|wasm-safe|playground)' )

if [ -n "$hits" ]; then
  echo "::error::forbidden impurity in the compile path — std::time/threads panic on wasm32-unknown-unknown (the playground) and break determinism. Route timing through almide_base::profile::ProfileTimer."
  echo "$hits"
  exit 1
fi
echo "forbidden-impurities: clean (no raw clock/thread sources in the compile-path crates)"
