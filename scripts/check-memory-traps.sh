#!/usr/bin/env bash
# #4023: the memory traps, held to the corpus.
#
# Two traps turn a silent memory error into a defined abort:
#   - the wasm Perceus runtime's double-free trap ($dec_flat and the drop
#     bodies: a release of a block already at rc 0 hits `unreachable`). It
#     is in EVERY wasm build since #4023:
#     measured +0.0% instructions / +0.3% cycles (median of the perf suite's
#     wasm rows) and +0.8% bytes over spec/wasm_cross. The stock-WASI build
#     puts proc_exit(1) in front of every `unreachable`, so it exits 1.
#   - the native region arena's stale-handle trap, ALMIDE_REGION_TRAP_STALE
#     (read at BUILD time: a handle read after its window rewound panics,
#     exit 101, instead of returning the old bytes). It stays OFF in shipped
#     builds — measured +30% on binarytrees and +52% on treealloc, the rows the
#     region window exists for — so this gate is what runs it: the corpus must
#     pass with it armed, so a wrong region verdict fails here before release.
#
#   scripts/check-memory-traps.sh spec      almide test spec/ on native with ALMIDE_REGION_TRAP_STALE=1
#                                           (and on wasm, where the double-free trap is always on)
#   scripts/check-memory-traps.sh fuzz      proofs/diff-fuzz.sh and one xtarget-fuzz campaign, fixed
#                                           seed, region trap armed: a trap that fires on one leg is
#                                           an exit-code divergence, i.e. a finding
#   scripts/check-memory-traps.sh controls  re-inject one known bug per trap and require the trap to
#                                           turn it into an abort with its exit code
#   env: ALMIDE (default target/release/almide), DIFF_FUZZ_N (150), FUZZ_COUNT (300),
#        FUZZ_SEED (4023), FUZZ_JOBS (2)
set -uo pipefail
export LC_ALL=C
cd "$(dirname "$0")/.."
ROOT="$PWD"
ALMIDE="${ALMIDE:-$ROOT/target/release/almide}"
mode="${1:-spec}"
# The CI slice (scripts/lib/ci-slice.sh): under ALMIDE_CI_SLICE=k > 1 (a PR into
# develop) `spec` runs the commit-rotated 1-in-k slice of spec/'s test files
# (plus the ones the change touches), one `almide test <file>` each, and
# `fuzz` runs 1 in k of the programs from the same seeds (diff-fuzz at least
# 20, xtarget-fuzz at least 30). The full run (develop, main, PRs into main,
# the nightly) runs all of spec/ and every program.
. scripts/lib/ci-slice.sh
. scripts/lib/wasm-skip.sh

arm() { ALMIDE_REGION_TRAP_STALE=1 "$@"; }
unarm() { env -u ALMIDE_REGION_TRAP_STALE "$@"; }
need_wasmtime() { command -v wasmtime >/dev/null || { echo "memory-traps: wasmtime not found" >&2; exit 2; }; }
need_almide() { [ -x "$ALMIDE" ] || { echo "memory-traps: $ALMIDE is not built" >&2; exit 2; }; }

case "$mode" in
spec)
  need_almide; need_wasmtime
  fail=0
  if ci_slice_active; then
    # The files `almide test spec/` would collect (a `test` block at a line
    # start, no hidden directory), then the slice.
    sel=$(mktemp)
    find spec -name '*.almd' -not -path '*/.*' | LC_ALL=C sort | xargs grep -l '^test ' \
      | ci_slice_filter memory-traps-spec > "$sel" || exit 2
    n=$(wc -l < "$sel" | tr -d ' ')
    [ "$n" -gt 0 ] || { echo "memory-traps: the CI slice selected 0 test files — a vacuous run is not a pass" >&2; exit 1; }
    one() { # $1 = target, $2 = file
      # A file that DECLARES `// wasm:skip` has no wasm leg to arm: run on
      # its own it runs no test, which `almide test` fails (#4325). Not run,
      # and named with its reason so the skip stays visible. A file the leg
      # merely WALLS carries no declaration and still runs here — and fails.
      local reason
      if [ "$1" = wasm ] && reason=$(wasm_skip_reason "$2"); then
        printf 'SKIP %s (--target wasm: declared // wasm:skip: %s)\n' "$2" "$reason"; return 0
      fi
      local o; o=$(ALMIDE_REGION_TRAP_STALE=1 "$ALMIDE" test "$2" --target "$1" 2>&1) && return 0
      printf 'FAIL %s (--target %s):\n%s\n' "$2" "$1" "$o" | head -60; return 1
    }
    export -f one wasm_skip_reason; export ALMIDE
    for leg in rust wasm; do
      echo "== spec/ CI slice ($n test files) on $leg, ALMIDE_REGION_TRAP_STALE=1"
      xargs -P "$(getconf _NPROCESSORS_ONLN 2>/dev/null || echo 4)" -I{} bash -c 'one "$@"' _ "$leg" {} < "$sel" || fail=1
    done
    rm -f "$sel"
  else
  echo "== spec/ on native, ALMIDE_REGION_TRAP_STALE=1"
  arm "$ALMIDE" test spec/ --target rust || fail=1
  echo "== spec/ on wasm (the double-free trap is in every build)"
  arm "$ALMIDE" test spec/ --target wasm || fail=1
  fi
  [ $fail = 0 ] && echo "memory-traps: spec/ passes on both legs with both traps armed"
  exit $fail
  ;;
fuzz)
  need_almide; need_wasmtime
  fail=0
  FUZZ_COUNT="$(ci_slice_count "${FUZZ_COUNT:-300}" 30 xtarget-fuzz)" || exit 2
  echo "== proofs/diff-fuzz.sh, seed ${FUZZ_SEED:-4023}, ${DIFF_FUZZ_N:-150} programs, region trap armed"
  # diff-fuzz.sh stamps the PATH `almide` against the workspace build: put the
  # binary under test first so a stale installed one cannot stand in for it.
  PATH="$(dirname "$ALMIDE"):$PATH" arm bash proofs/diff-fuzz.sh "${DIFF_FUZZ_N:-150}" "${FUZZ_SEED:-4023}" || fail=1
  (cd tools/xtarget-fuzz && cargo build --release -q) || exit 2
  out=$(mktemp -d)
  echo "== xtarget-fuzz, seed ${FUZZ_SEED:-4023}, ${FUZZ_COUNT:-300} programs, region trap armed"
  arm tools/xtarget-fuzz/target/release/xtarget-fuzz run --repo "$ROOT" --almide "$ALMIDE" \
    --seed "${FUZZ_SEED:-4023}" --count "${FUZZ_COUNT:-300}" --jobs "${FUZZ_JOBS:-2}" --out "$out" \
    || { echo "memory-traps: xtarget-fuzz findings under $out"; fail=1; }
  [ $fail = 0 ] && echo "memory-traps: no differential finding with both traps armed"
  exit $fail
  ;;
controls)
  need_wasmtime
  TARGET="${REINJECT_TARGET_DIR:-$ROOT/target/memory-traps-reinject}"
  wt=$(mktemp -d)
  trap 'git -C "$ROOT" worktree remove --force "$wt" >/dev/null 2>&1; rm -rf "$wt"' EXIT
  git -C "$ROOT" worktree add --detach -q "$wt" HEAD || exit 2
  fail=0
  t=$(mktemp -d)
  # reinject <patch>: apply it to the scratch worktree and build; prints the binary
  reinject() {
    git -C "$wt" checkout -q -- .
    git -C "$wt" apply "$1" || { echo "FAIL $(basename "$1" .patch): the patch no longer applies — regenerate it" >&2; return 1; }
    (cd "$wt" && CARGO_TARGET_DIR="$TARGET" cargo build -q --release --bin almide) \
      || { echo "FAIL $(basename "$1" .patch): the re-injected compiler does not build" >&2; return 1; }
    echo "$TARGET/release/almide"
  }
  # verdict <name> <rc> <stderr file> <want rc> <want stderr fragment, or ""> <what>
  verdict() {
    if [ "$2" != "$4" ]; then echo "FAIL $1: $6 exited $2, expected $4"; sed 's/^/    /' "$3" | head -5; fail=1
    elif [ -n "$5" ] && ! grep -qF "$5" "$3"; then echo "FAIL $1: $6's stderr lacks '$5'"; sed 's/^/    /' "$3" | head -5; fail=1
    else echo "ok   $1: $6 exits $2${5:+ — $(grep -m1 -F "$5" "$3" | cut -c1-140)}"; fi
  }

  # wasm — #3538 re-injected: option.zip copies each payload into the tuple
  # without its share, so the tuple's release frees both strings and the
  # let-bound inputs release them again, still at rc 0. On the compiler
  # before #4023 the same patch and program exit 0 with the right output
  # (measured 2026-10-24 on b19d88caa); the trap ends it with exit 1.
  name=rc-3538-zip-let-bound-double-release
  if bin=$(reinject "$ROOT/proofs/fixtures/heap-check/reinject/3538-payload-copy-without-credit.patch"); then
    if "$bin" build "$ROOT/proofs/fixtures/memory-traps/$name.almd" --target wasm -o "$t/w.wasm" >/dev/null 2>&1; then
      (cd "$t" && wasmtime run w.wasm >w.out 2>w.err; echo $? >w.rc)
      verdict "$name (wasm)" "$(cat "$t/w.rc")" "$t/w.err" 1 "" "the shipped wasm build"
    else echo "FAIL $name: the reproduction does not build"; fail=1; fi
  else fail=1; fi

  # native — a wrong region verdict: the producer gets a window of its own,
  # which rewinds before the consumer reads the tree. Unarmed, the rewound
  # slots still hold the old bytes and the answer is right (silent); armed,
  # the first read of a rewound handle panics.
  name=region-producer-window-rewinds-before-consume
  src="$ROOT/proofs/fixtures/memory-traps/$name.almd"
  if bin=$(reinject "$ROOT/proofs/fixtures/memory-traps/$name.patch"); then
    if unarm "$bin" build "$src" --release -o "$t/off" >/dev/null 2>&1 && arm "$bin" build "$src" --release -o "$t/on" >/dev/null 2>&1; then
      (cd "$t" && ./off >off.out 2>off.err; echo $? >off.rc; ./on >on.out 2>on.err; echo $? >on.rc)
      if [ "$(cat "$t/off.rc")" != 0 ] || [ "$(cat "$t/off.out")" != 2047 ]; then
        echo "FAIL $name: the unarmed build exited $(cat "$t/off.rc") printing '$(cat "$t/off.out")' — the control expects the stale read to be SILENT there (exit 0, 2047)"; fail=1
      else
        echo "ok   $name (native, unarmed): exits 0 printing 2047 — the stale read is silent"
      fi
      verdict "$name (native, ALMIDE_REGION_TRAP_STALE=1)" "$(cat "$t/on.rc")" "$t/on.err" 101 \
        "region window trap (ALMIDE_REGION_TRAP_STALE)" "the armed build"
    else echo "FAIL $name: the reproduction does not build"; fail=1; fi
  else fail=1; fi

  git -C "$wt" checkout -q -- .
  rm -rf "$t"
  [ $fail = 0 ] && echo "memory-traps: each re-injected bug ends in its trap's abort"
  exit $fail
  ;;
*) echo "usage: $0 spec|fuzz|controls" >&2; exit 2 ;;
esac
