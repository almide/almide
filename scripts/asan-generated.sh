#!/usr/bin/env bash
# ADDRESSSANITIZER OVER GENERATED RUST (#4009 item 1)
# ===================================================
#
# scripts/miri-generated.sh checks each program's generated crate under Miri,
# but Miri stops at what it does not model — a socket, a child process, a host
# call — and a stress fixture can outrun its timeout. This runs the same
# self-contained crates compiled natively with AddressSanitizer
# (`-Zsanitizer=address`), which instruments every load and store of the
# program and its runtime, threads and foreign calls included, and reports an
# out-of-bounds access, a use after free or a double free where it happens.
#
#   bash scripts/asan-generated.sh [files...]       (default: spec/wasm_cross/*.almd)
#
# ALMIDE_BIN      the compiler (default target/release/almide)
# ASAN_TOOLCHAIN  the nightly that accepts -Zsanitizer (default nightly-2026-08-01, Miri's)
# ASAN_JOBS       parallel programs (default: the CPU count)
# ASAN_TIMEOUT    seconds per run (default 300)
#
# Per program, one verdict:
#   ok         ran to the end with no AddressSanitizer report
#   ASAN       AddressSanitizer reported an error — FAILS the gate
#   timeout    past ASAN_TIMEOUT seconds; reported, not judged
#   no-build   the program does not build natively (a negative fixture)
#   no-asan    the crate builds ordinarily but not under the sanitizer
#              (reported: the program was not checked)
#
# Leak detection is off (detect_leaks=0): a leak is not a memory-safety
# violation, and LeakSanitizer does not run on every host. An allocation too
# large to satisfy returns null (allocator_may_return_null=1), as the system
# allocator does: the runtime's try_reserve turns that into the defined
# "Error: out of memory" abort, which the OOM fixtures pin, instead of the
# sanitizer stopping the program with a report that is not a memory error.
#
# Before the corpus, a control: a program reading 64 bytes of a 4-byte malloc block
# through a Region whose `@region(len = 64)` clause lies (ADR-0031: the
# length clause is the trusted line, so a wrong one is the one way left) MUST come back ASAN, so a sanitizer
# that stopped instrumenting cannot pass the gate.
set -uo pipefail
export LC_ALL=C
cd "$(dirname "$0")/.."
ROOT="$PWD"
ALMIDE_BIN="${ALMIDE_BIN:-$ROOT/target/release/almide}"
ASAN_TOOLCHAIN="${ASAN_TOOLCHAIN:-nightly-2026-08-01}"
ASAN_JOBS="${ASAN_JOBS:-$(getconf _NPROCESSORS_ONLN 2>/dev/null || echo 2)}"
[ -x "$ALMIDE_BIN" ] || { echo "::error::asan-generated: $ALMIDE_BIN is not executable (cargo build --release)"; exit 2; }
HOST="$(rustc "+$ASAN_TOOLCHAIN" -vV 2>/dev/null | sed -n 's/^host: //p')"
[ -n "$HOST" ] || { echo "::error::asan-generated: no $ASAN_TOOLCHAIN toolchain (rustup toolchain install $ASAN_TOOLCHAIN)"; exit 2; }

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

# One program: build its self-contained crate (no prebuilt runtime rlib, so
# the runtime is compiled — and instrumented — with the program), then run it.
# The explicit --target keeps build scripts and proc macros uninstrumented,
# as the sanitizer requires.
one() {
  local f="$1" d
  d="$WORK/$(printf '%s' "$f" | tr '/.' '__')"
  mkdir -p "$d"
  if ! ALMIDE_NO_RTLIB=1 ALMIDE_RUN_PROJECT_DIR="$d" "$ALMIDE_BIN" build "$f" -o "$d/bin" >"$d/build.log" 2>&1; then
    echo "no-build $f"; return
  fi
  if ! (cd "$d" && RUSTFLAGS="-Zsanitizer=address -Awarnings" cargo "+$ASAN_TOOLCHAIN" build -q --target "$HOST" >"$d/asan-build.log" 2>&1); then
    echo "no-asan $f"; rm -rf "$d/target"; return
  fi
  local exe
  exe="$(find "$d/target/$HOST/debug" -maxdepth 1 -type f -perm -u+x | head -1)"
  local limit=()
  command -v timeout >/dev/null 2>&1 && limit=(timeout "${ASAN_TIMEOUT:-300}")
  (cd "$d" && ASAN_OPTIONS="detect_leaks=0:allocator_may_return_null=1:${ASAN_OPTIONS:-}" ${limit[@]+"${limit[@]}"} "$exe" </dev/null >"$d/out" 2>"$d/err")
  local code=$?
  if grep -q 'ERROR: AddressSanitizer' "$d/err"; then
    echo "ASAN $f"
    grep -m1 -A8 'ERROR: AddressSanitizer' "$d/err" | sed 's/^/    /' >&2
  elif [ "$code" = 124 ]; then
    echo "timeout $f"
  else
    echo "ok $f"
  fi
  rm -rf "$d/target"
}
export -f one
export WORK ALMIDE_BIN ASAN_TOOLCHAIN HOST

# The control.
ctl="$WORK/control.almd"
cat > "$ctl" <<'EOF'
import ffi
@extern(c, "c", "malloc")
@region(len = 64, release = "free")
fn c_malloc(n: UInt64) -> Region
effect fn main() -> Unit = {
  let small = c_malloc(4)
  let leaked = ffi.read(small, 0, 64)!
  println(int.to_string(bytes.len(leaked)))
}
EOF
v="$(one "$ctl" 2>/dev/null)"
if [ "${v%% *}" != ASAN ]; then
  echo "::error::asan-generated: the control (a 64-byte read of a 4-byte block through a lying @region length) came back '${v%% *}', not ASAN — the sanitizer is not instrumenting, so no verdict below would mean anything."
  exit 1
fi
echo "control: a 64-byte read past a 4-byte block through a lying @region length is a heap-buffer-overflow under AddressSanitizer, as it must be"

if [ "$#" -gt 0 ]; then printf '%s\n' "$@"; else ls spec/wasm_cross/*.almd; fi > "$WORK/list"
xargs -P "$ASAN_JOBS" -I{} bash -c 'one "$1"' _ {} < "$WORK/list" > "$WORK/verdicts"

sort -o "$WORK/verdicts" "$WORK/verdicts"
for k in ok ASAN timeout no-build no-asan; do
  printf '  %-9s %s\n' "$k" "$(grep -c "^$k " "$WORK/verdicts")"
done
grep -E '^(timeout|no-asan) ' "$WORK/verdicts" | sed 's/^/  /' || true
if grep -q '^ASAN ' "$WORK/verdicts"; then
  grep '^ASAN ' "$WORK/verdicts" | sed 's/^ASAN /::error::asan-generated: AddressSanitizer error in /'
  exit 1
fi
echo "asan-generated: $(wc -l < "$WORK/list" | tr -d ' ') program(s), no AddressSanitizer error"
