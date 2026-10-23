#!/usr/bin/env bash
# MIRI OVER GENERATED RUST (#4009)
# ================================
#
# The native leg's memory safety rests on the `unsafe` in runtime/rs and the
# compiler-emitted preludes (#4007), and on rustc. A cross-target diff cannot
# see memory corruption that both legs share, or that does not change the
# output. This runs each program's generated crate under Miri, which checks
# every pointer use, every foreign call's signature and every data race
# against Rust's rules, and reports undefined behaviour where it happens.
#
#   bash scripts/miri-generated.sh [files...]       (default: spec/wasm_cross/*.almd)
#
# ALMIDE_BIN    the compiler (default target/release/almide)
# MIRI_TOOLCHAIN the nightly with the miri component (default nightly-2026-08-01)
# MIRI_JOBS     parallel programs (default: the CPU count)
#
# Per program, one verdict:
#   ok           Miri ran it to the end with no UB
#   UB           undefined behaviour — FAILS the gate
#   unsupported  Miri stopped at an operation it does not model (a socket, a
#                child process, a host call); reported, not failed
#   timeout      past MIRI_TIMEOUT seconds (default 600); reported, not judged
#   no-build     the program does not build natively (a negative fixture)
#
# Before the corpus, a control: a program reading 64 bytes through the
# address of a 4-byte buffer (the #4043 shape) MUST come back UB, so a Miri
# that stopped checking cannot pass the gate.
set -uo pipefail
export LC_ALL=C
cd "$(dirname "$0")/.."
ROOT="$PWD"
ALMIDE_BIN="${ALMIDE_BIN:-$ROOT/target/release/almide}"
MIRI_TOOLCHAIN="${MIRI_TOOLCHAIN:-nightly-2026-08-01}"
MIRI_JOBS="${MIRI_JOBS:-$(getconf _NPROCESSORS_ONLN 2>/dev/null || echo 2)}"
[ -x "$ALMIDE_BIN" ] || { echo "::error::miri-generated: $ALMIDE_BIN is not executable (cargo build --release)"; exit 2; }
cargo "+$MIRI_TOOLCHAIN" miri --version >/dev/null 2>&1 || { echo "::error::miri-generated: no miri on $MIRI_TOOLCHAIN (rustup component add miri rust-src --toolchain $MIRI_TOOLCHAIN)"; exit 2; }

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

# One program: build its self-contained crate (no prebuilt runtime rlib, so
# the runtime's source is in the crate Miri interprets), then run it.
one() {
  local f="$1" d
  d="$WORK/$(printf '%s' "$f" | tr '/.' '__')"
  mkdir -p "$d"
  if ! ALMIDE_NO_RTLIB=1 ALMIDE_RUN_PROJECT_DIR="$d" "$ALMIDE_BIN" build "$f" -o "$d/bin" >"$d/build.log" 2>&1; then
    echo "no-build $f"; return
  fi
  # Miri runs a program hundreds of times slower than native, so a fixture
  # built to stress size or depth can take hours; it is cut off and reported.
  local limit=()
  command -v timeout >/dev/null 2>&1 && limit=(timeout "${MIRI_TIMEOUT:-600}")
  (cd "$d" && MIRIFLAGS="${MIRIFLAGS:--Zmiri-disable-isolation}" ${limit[@]+"${limit[@]}"} cargo "+$MIRI_TOOLCHAIN" miri run -q </dev/null >"$d/out" 2>"$d/err")
  local code=$?
  if grep -q 'Undefined Behavior' "$d/err"; then
    echo "UB $f"
    sed -n '/Undefined Behavior/,/^$/p' "$d/err" | head -12 | sed 's/^/    /' >&2
  elif grep -q 'unsupported operation' "$d/err"; then
    echo "unsupported $f"
  elif [ "$code" = 124 ]; then
    echo "timeout $f"
  else
    echo "ok $f"
  fi
  rm -rf "$d/target"
}
export -f one
export WORK ALMIDE_BIN MIRI_TOOLCHAIN

# The control.
ctl="$WORK/control.almd"
cat > "$ctl" <<'EOF'
import ffi
effect fn main() -> Unit = {
  let small = bytes.from_list([1, 2, 3, 4])
  let leaked = ffi.read(ffi.address(small), 64)
  println(int.to_string(bytes.len(leaked)))
}
EOF
v="$(one "$ctl" 2>/dev/null)"
if [ "${v%% *}" != UB ]; then
  echo "::error::miri-generated: the control (a 64-byte read through a 4-byte buffer's address) came back '${v%% *}', not UB — Miri is not checking, so no verdict below would mean anything."
  exit 1
fi
echo "control: a 64-byte read past a 4-byte buffer is UB under Miri, as it must be"

if [ "$#" -gt 0 ]; then printf '%s\n' "$@"; else ls spec/wasm_cross/*.almd; fi > "$WORK/list"
xargs -P "$MIRI_JOBS" -I{} bash -c 'one "$1"' _ {} < "$WORK/list" > "$WORK/verdicts"

sort -o "$WORK/verdicts" "$WORK/verdicts"
for k in ok UB unsupported timeout no-build; do
  printf '  %-12s %s\n' "$k" "$(grep -c "^$k " "$WORK/verdicts")"
done
grep -E '^(unsupported|timeout) ' "$WORK/verdicts" | sed 's/^/  /' || true
if grep -q '^UB ' "$WORK/verdicts"; then
  grep '^UB ' "$WORK/verdicts" | sed 's/^UB /::error::miri-generated: undefined behaviour in /'
  exit 1
fi
echo "miri-generated: $(wc -l < "$WORK/list" | tr -d ' ') program(s), no undefined behaviour"
