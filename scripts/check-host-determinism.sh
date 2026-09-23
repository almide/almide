#!/usr/bin/env bash
# Host-architecture WASM codegen determinism gate.
#
# The compiler runs as wasm32 in the browser playground but as x86-64/aarch64 in
# the test suite. A codegen path whose output depends on host pointer width
# (usize) or HashMap iteration order produces a DIFFERENT — but individually
# stack-/RC-valid — WASM module on a 32-bit host, which can trap at runtime
# (`RuntimeError: unreachable`). The stack-effect verifier and Perceus belt check
# a single module's well-formedness, not reproducibility ACROSS hosts, so they
# are blind to this class. This gate closes that gap: it compiles each fixture
# with the compiler built BOTH natively and to wasm32-wasip1, and asserts the
# emitted WASM is byte-identical.
#
# Usage: scripts/check-host-determinism.sh [fixture-dir]   (default: spec/wasm_cross)
set -uo pipefail

# Byte-order collation, pinned: `sort`'s last-resort comparison follows the ambient
# locale, so an unpinned sort produces different output on differently-configured
# machines. #1031 caught docs/roadmap/README.md changing row order with no content change.
export LC_ALL=C
cd "$(dirname "$0")/.."

FIXTURE_DIR="${1:-spec/wasm_cross}"
HARNESS="tools/wasmgen-harness"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

WASMTIME="$(command -v wasmtime || echo "$HOME/.wasmtime/bin/wasmtime")"
[ -x "$WASMTIME" ] || { echo "::error::wasmtime not found"; exit 2; }

echo "==> Building harness (native)"
cargo build --release --manifest-path "$HARNESS/Cargo.toml" -q || { echo "::error::native harness build failed"; exit 2; }
echo "==> Building harness (wasm32-wasip1)"
cargo build --release --target wasm32-wasip1 --manifest-path "$HARNESS/Cargo.toml" -q || { echo "::error::wasm32 harness build failed"; exit 2; }

NATIVE_BIN="$HARNESS/target/release/wasmgen-harness"
WASM_BIN="$HARNESS/target/wasm32-wasip1/release/wasmgen-harness.wasm"

fail=0; n=0
# WALL exit code from the harness: the fixture is not host-nondeterministic, it
# is simply not renderable by v1 yet (#782 — the v0 emitter that used to render
# it is retired). A wall on BOTH hosts is a TRACKED SKIP; a wall on only one host
# is a real host-dependent divergence and still FAILS.
WALL_RC=3
walled=0
for fix in "$FIXTURE_DIR"/*.almd; do
  [ -e "$fix" ] || continue
  name="$(basename "$fix")"
  cp "$fix" "$WORK/in.almd"
  # x86-64/aarch64 host
  "$NATIVE_BIN" "$WORK/in.almd" "$WORK/native.wasm" 2>/dev/null; nrc=$?
  # wasm32 host (compiler running as 32-bit, under wasmtime)
  "$WASMTIME" run --dir "$WORK::/w" "$WASM_BIN" /w/in.almd /w/wasm32.wasm >/dev/null 2>&1; wrc=$?
  if [ "$nrc" -eq "$WALL_RC" ] && [ "$wrc" -eq "$WALL_RC" ]; then
    echo "skip  $name (v1 wall on both hosts — tracked #782)"
    walled=$((walled+1)); continue
  fi
  if [ "$nrc" -ne "$wrc" ]; then
    echo "FAIL  $name — HOST-DEPENDENT wall (native rc=$nrc, wasm32 rc=$wrc)"
    fail=1; continue
  fi
  if [ "$nrc" -ne 0 ]; then echo "FAIL  $name (harness errored rc=$nrc)"; fail=1; continue; fi
  if cmp -s "$WORK/native.wasm" "$WORK/wasm32.wasm"; then
    echo "ok    $name ($(wc -c < "$WORK/native.wasm" | tr -d ' ') bytes, identical)"
  else
    echo "FAIL  $name — host-arch codegen DIVERGENCE (native $(wc -c < "$WORK/native.wasm" | tr -d ' ')B vs wasm32 $(wc -c < "$WORK/wasm32.wasm" | tr -d ' ')B)"
    fail=1
  fi
  n=$((n+1))
done

echo "----"
if [ "$fail" -ne 0 ]; then
  echo "::error::host-architecture codegen determinism FAILED — the compiler emits different WASM on 32-bit vs 64-bit hosts (the playground runs wasm32). Sort any HashMap/HashSet whose iteration order reaches emitted bytes."
  exit 1
fi

# The gate must not pass VACUOUSLY (#985): `n` counts only fixtures that
# reached the byte-compare, so a renderer regression that walled everything
# printed "0/0 byte-identical" and exited 0. On a green run every corpus file
# is either compared or walled — enforce that identity, and ratchet `walled`
# at its real value: 0 as of 2026-07-30 (324 fixtures); 16 as of 2026-08-26 —
# the commissioning switchover GRADUATED the entire legacy wall corpus into
# spec/wasm_cross (the structural leg renders them; the incumbent still
# walls them, so they are tracked skips of the INCUMBENT'S determinism
# domain, locally re-measured at exactly 16 over 634 fixtures). A NEW wall
# is a conscious ceiling bump, never silent shrinkage of coverage.
# 18 as of 2026-08-30 (second bump): env_sleep_pause.almd (C-327) walls on
# the incumbent brick (env.sleep_ms has no capability seat there) — same
# structural-only division, prunes with #1696 steps 4-5.
# 17 as of 2026-08-30: gzip_inflate_members.almd (C-326) is STRUCTURAL-ONLY
# — the incumbent's brick walls its loop-level tuple write-back
# (WhileHeapAccumulator), which is the intended division of labor until
# #1696 steps 4-5 move the certificate and retire the incumbent.
# 19 as of 2026-09-01: env_set_overlay.almd (C-329) — the determinism
# harnesses build the almide.* module without the env host surface, so the
# env.set/get fixture walls here; it executes on the embedded + stock-p1
# sweeps (#1716), which is where its promise lives.
# 20 as of 2026-09-01: zlib_selfhost.almd (C-331/#1700) — the incumbent
# walls the promoted C-326 decoder's tuple write-back loops, the same
# division the gzip_inflate_members rows record; prunes with #1696 4-5.
# 22 as of 2026-09-02: list_rest_pattern.almd (C-332) and
# as_pattern.almd (C-333) — the incumbent brick walls both #1461 forms
# honestly (the structural leg is the default route and lowers them);
# prunes with #1696 4-5.
# 23 as of 2026-09-02: generic_record_fn_field.almd (C-092/#1676) — the
# incumbent brick refuses the return-position computed call on the
# instantiated closure field (the structural leg, the default route,
# lowers it); prunes with #1696 4-5.
# 24 as of 2026-09-02: list_unique_by_nonscalar_key.almd (C-053/#1797) —
# the incumbent routes a non-scalar unique_by key to its unlinked `_x`
# render wall (C-147; a render-phase refusal the walled-real ratchet
# already classes "(b) acceptable", so no baseline row); the structural
# leg lowers every equatable key. Prunes with #1696 4-5.
# 25 as of 2026-09-03: bytes_temp_receiver.almd (C-213/#1849) — a TEMPORARY
# receiver of a Unit-returning bytes mutator; the incumbent brick walls it
# honestly (the receiver discipline names the call-result receiver, and the
# `let _ =` Unit binding is outside its value subset) where the structural
# leg, the default route, releases the mutated block. Prunes with #1696 4-5.
# 26 as of 2026-09-03: mut_param_effect_can_err.almd (C-132/#1576) — the
# can-err effect fn with a `mut` param takes the move-mode tuple rewrite
# (`(T, Buf)` on the ok payload); the incumbent brick walls the synthesized
# `let (r, b) = call!` destructure-unwrap honestly (`unwrap `!` in a
# call-argument position`) where the structural leg, the default route,
# lowers every cell byte-identical to native. Prunes with #1696 4-5.
# 28 as of 2026-09-09: record_option_none_cells.almd (C-255/#2057) is
# a NEW structural fixture, covering absent and present Option record cells.
# Host CI run 34321896641 confirms it walls on BOTH incumbent hosts; every
# emitted fixture still compares identical. Native and the structural leg
# execute the complete field matrix (none/some), with size and allocation
# ledgers pinning its current emission. This adds one unsupported incumbent
# input, not a loss of coverage for a previously emitted fixture.
# 29 as of 2026-09-13: sort_by_compound_key.almd (C-053/#2154) is a NEW
# structural fixture, and its wall here is the FIX rather than a gap. The
# incumbent rendered a compound sort key by comparing the cached key as a raw
# i64 while the key closure returned an i32 handle — `indirect call type
# mismatch` at run time, out of an artifact it reported `verified`. It now
# REFUSES the shape (`list.sort_by_x`, an unlinked render wall), the same
# refusal it already makes for a non-scalar `unique_by` key (C-147). There was
# never a correct incumbent emission of this fixture to lose; the ceiling rises
# by the one input the incumbent stopped mis-rendering. The structural leg, the
# default route, orders it byte-identical to native across the whole key
# lattice, and tests/sort_by_compound_key_test.rs pins the refusal itself.
# 30 as of 2026-09-13: ord_record_variant.almd (C-053/#2167) — same shape of
# bump as the row above, one type lattice further in. A record or variant that
# derives Ord sorted on native and BOTH wasm legs refused it; the structural
# leg now orders it (field declaration order / case order then payload, native's
# derive) and the incumbent keeps its standing refusal of every non-scalar
# element (C-147). Nothing that emitted before stopped emitting.
# 31 as of 2026-09-14: ord_recursive.almd (C-053/#2172) — the third bump in
# this family and the same shape as the two above. A RECURSIVE type that
# derives Ord built natively and neither wasm leg could emit it; the structural
# leg now does, because a `Named` comparator is emitted once out of line and
# CALLED instead of inlined at the use site, and the incumbent keeps its
# standing refusal of every non-scalar element (C-147). Measured directly on
# this tree: 676 emitted, 31 walled, 707 total — the emitted count is unchanged,
# so this is one more input the incumbent never rendered, not coverage lost.
# 33 as of 2026-09-22: ref_grain_or_pattern_alias_match.almd and
# ref_rust_or_pattern_heap_subject.almd (C-323, #2435) — reference or-pattern
# suites whose subjects are heap values (a String, a `some(String)`, a
# `List[String]`) matched by literal and wildcard alternatives and reused after
# the match. The structural leg emits both (native == wasm byte-identical, and
# the judge agrees); the incumbent refuses the non-scalar subject the same way
# it refuses every non-scalar element (C-147). Two new inputs the incumbent
# never rendered, not coverage lost: every previously emitted fixture still is.
# 34 as of 2026-09-22: or_pattern_guarded_nullary.almd (C-323, #2463) — the
# fixture pinning the native fixpoint fix (a guarded or-pattern arm over a
# nullary constructor). The incumbent refuses its lifted guard arms as a
# heap-result match outside its subset (proofs/walled-real-baseline.txt has the
# five rows, owned by #2473); the structural leg lowers it byte-identical to
# native. One more input the incumbent never rendered, not coverage lost.
# 33 as of 2026-09-22 (fourth A-ci batch, measured on the assembled tree with
# the native wasmgen harness: 712 emitted + 33 walled of 745): the ceiling goes
# DOWN by one although the batch adds seven fixtures. #2466 made the bytes
# writers take `mut` receivers, and bytes_temp_receiver.almd — walled since
# 2026-09-03 as a temporary receiver outside the incumbent's value subset — now
# drives its writers through a `var`, so the incumbent renders it. The matrix,
# bytes-domain and record-field fixtures this batch adds all emit on both legs.
# 29 as of 2026-09-23 (#2473/#2520, measured with the native wasmgen harness
# on the tree rebased over develop: 742 emitted + 29 walled of 771): DOWN from 33
# (this branch lifts three; the ceiling is set to the measured count). The
# incumbent now specializes
# guarded / literal-payload Option, Result and custom-variant matches and
# String-literal list patterns per constructor, so ref_grain_or_pattern_alias_match,
# ref_rust_or_pattern_heap_subject and or_pattern_guarded_nullary render and
# byte-match native; the two fixtures the change adds
# (or_pattern_heap_subject_matrix, list_heap_tuple_return) emit on both legs.
# 30 as of 2026-09-23 (#2553): record_variant_field_literal.almd (C-036/C-070)
# pins a record pattern with refutable fields (`Circle { r: 0, .. }`,
# `P { x: 0, y }`), which only the structural leg lowers. The incumbent's
# variant-arm specializer admits a refutable payload only on a single-field
# positional constructor, so it walls these four functions
# (proofs/walled-real-baseline.txt), identically on both hosts. One more input
# the incumbent never rendered, not coverage lost.
MAX_WALLED=30
corpus=$(ls "$FIXTURE_DIR"/*.almd 2>/dev/null | wc -l | tr -d ' ')
if [ "$corpus" -eq 0 ] || [ $((n + walled)) -ne "$corpus" ]; then
  echo "::error::host-determinism: compared $n + walled $walled != corpus $corpus in $FIXTURE_DIR — the scan went blind (#985)"
  exit 1
fi
if [ "$walled" -gt "$MAX_WALLED" ]; then
  echo "::error::host-determinism: $walled fixtures walled (ceiling $MAX_WALLED) — coverage shrank; fix the wall or raise MAX_WALLED consciously in the same change (#985)"
  exit 1
fi
echo "host-architecture codegen determinism: $n/$corpus emitted fixtures byte-identical across x86-64 and wasm32 ($walled walled, ceiling $MAX_WALLED)"
