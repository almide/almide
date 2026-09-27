#!/usr/bin/env bash
# Native-deps musl gate (#2777). `almide build --target <triple>` (#2772) is
# checked per PR on a dependency-free program, which links against rustc's
# self-contained musl crt and compiles no C. The programs people ship on musl
# (porta, comide) pull `[native-deps]` crates that DO compile C — ring under
# rustls needs musl-gcc — or are simply large (wasmtime). This builds one
# fixture project under tests/native_musl/<name>/ for the musl target and
# asserts:
#   1. the build succeeds;
#   2. `file` reports the binary statically linked (a glibc/host binary handed
#      back in its place is the #2772 shape);
#   3. running it prints exactly <fixture>/expected.txt.
#
#   check-native-musl.sh <fixture-dir>              the gate
#   check-native-musl.sh --selftest <fixture-dir>   the gate refuses forged runs
#
# --selftest is the gate's own negative evidence, re-measured every time it
# runs: (a) the C compiler for the musl target replaced by an absent tool (the
# runner without musl-tools) must fail the build; (b) a glibc target, whose
# binary is dynamic, must fail the static check; (c) a forged expected line
# must fail the output check; and the genuine run must pass. Each case builds
# in a fresh project dir so no cached artifact can answer for it.
#
# Env: ALMIDE_BIN (default target/release/almide); NATIVE_MUSL_TARGET (default
# x86_64-unknown-linux-musl); ALMIDE_RUN_PROJECT_DIR (the generated cargo
# project — CI points it at a cached dir; default a fresh temp dir).
set -uo pipefail
export LC_ALL=C
cd "$(dirname "$0")/.."

BIN="${ALMIDE_BIN:-target/release/almide}"
case "$BIN" in /*) ;; */*) BIN="$PWD/$BIN" ;; *) BIN="$(command -v "$BIN" || echo "$PWD/$BIN")" ;; esac
TARGET="${NATIVE_MUSL_TARGET:-x86_64-unknown-linux-musl}"

selftest=0
if [ "${1:-}" = "--selftest" ]; then selftest=1; shift; fi
FIXTURE="${1:?usage: check-native-musl.sh [--selftest] <fixture-dir>}"
FIXTURE="${FIXTURE%/}"
for f in almide.toml src/main.almd expected.txt; do
  [ -f "$FIXTURE/$f" ] || { echo "::error::native-musl: $FIXTURE/$f missing"; exit 2; }
done

if [ "$selftest" = 1 ]; then
  self="$PWD/scripts/check-native-musl.sh"
  fails=0
  # expect <want-exit: pass|fail> <label> [env assignments...]
  expect() {
    local want="$1" label="$2"; shift 2
    local proj; proj="$(mktemp -d)"
    if env "$@" ALMIDE_RUN_PROJECT_DIR="$proj" bash "$self" "$FIXTURE" >"$proj.log" 2>&1; then got=pass; else got=fail; fi
    if [ "$got" = "$want" ]; then
      echo "ok   $label: $got ($(grep -m1 -E '^(PASS|FAIL)' "$proj.log" || tail -1 "$proj.log"))"
    else
      echo "::error::native-musl selftest: $label expected $want, got $got"; sed 's/^/    /' "$proj.log" | tail -30
      fails=$((fails + 1))
    fi
    rm -rf "$proj" "$proj.log"
  }
  cc_var="CC_$(echo "$TARGET" | tr '-' '_')"
  gnu_target="${TARGET%-musl}-gnu"
  forged="$(mktemp -d)"; cp -R "$FIXTURE/." "$forged/"; echo "forged line" > "$forged/expected.txt"
  expect pass "genuine $TARGET build"
  expect fail "no C compiler for $TARGET ($cc_var=musl-gcc-absent)" "$cc_var=musl-gcc-absent"
  expect fail "glibc target $gnu_target (dynamic binary)" "NATIVE_MUSL_TARGET=$gnu_target"
  FIXTURE_SAVE="$FIXTURE"; FIXTURE="$forged"
  expect fail "forged expected.txt"
  FIXTURE="$FIXTURE_SAVE"; rm -rf "$forged"
  [ "$fails" = 0 ] && { echo "PASS native-musl selftest: the gate refused 3/3 forged runs and accepted the genuine one"; exit 0; }
  echo "FAIL native-musl selftest: $fails case(s) gave the wrong verdict"; exit 1
fi

outdir="$(mktemp -d)"; out="$outdir/bin"
proj="${ALMIDE_RUN_PROJECT_DIR:-}"
if [ -n "$proj" ]; then trap 'rm -rf "$outdir"' EXIT
else proj="$(mktemp -d)"; trap 'rm -rf "$outdir" "$proj"' EXIT; fi
name="$(basename "$FIXTURE")"

start=$(date +%s)
if ! (cd "$FIXTURE" && ALMIDE_RUN_PROJECT_DIR="$proj" "$BIN" build src/main.almd --target "$TARGET" -o "$out"); then
  echo "FAIL native-musl $name: almide build --target $TARGET failed"
  echo "::error::native-musl: $FIXTURE did not build for $TARGET (a [native-deps] crate that compiles C needs the target's C compiler — musl-tools)"
  exit 1
fi
secs=$(( $(date +%s) - start ))

desc="$(file -b "$out")"
if ! printf '%s' "$desc" | grep -Eq 'statically linked|static-pie linked'; then
  echo "FAIL native-musl $name: not a static binary: $desc"
  echo "::error::native-musl: --target $TARGET produced a non-static binary: $desc"
  exit 1
fi

actual="$("$out" 2>&1)"; rc=$?
expected="$(cat "$FIXTURE/expected.txt")"
if [ "$rc" != 0 ] || [ "$actual" != "$expected" ]; then
  echo "FAIL native-musl $name: exit $rc, printed [$actual], expected [$expected]"
  echo "::error::native-musl: the static $TARGET binary of $FIXTURE printed [$actual] (exit $rc), expected [$expected]"
  exit 1
fi
echo "PASS native-musl $name: $TARGET static binary built in ${secs}s ($(wc -c <"$out" | tr -d ' ') bytes) and printed the expected line"
