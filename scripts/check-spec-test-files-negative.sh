#!/usr/bin/env bash
# Negative control for scripts/check-spec-test-files.sh (#3065): the gate must
# pass a well-formed file and refuse each broken shape it exists for.
set -uo pipefail
export LC_ALL=C
cd "$(dirname "$0")/.." || exit 2
BIN="${ALMIDE_BIN:-target/release/almide}"
[ -x "$BIN" ] || { echo "::error::ALMIDE_BIN not executable: $BIN"; exit 2; }
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

write() { printf '%s\n' "$2" > "$tmp/$1"; }
write ok_test.almd 'test "one" { assert_eq(1 + 1, 2) }'
write main_test.almd 'fn main() -> Unit = println("x")'
write neg_ok_test.almd '// wasm:skip — expected compile error test (no test blocks)
fn main() -> Unit = println(undefined_name)'
# The #3065 shape: a 3-argument assert_eq (only rustc refused it).
write arity_test.almd 'test "one" { assert_eq(1, 1, "msg") }'
# Run by nothing: a main behind wasm:skip, and no test block.
write orphan_test.almd '// wasm:skip — native only
fn main() -> Unit = println("x")'
# A negative fixture the checker accepts tests nothing.
write neg_bad_test.almd '// wasm:skip — expected compile error test (no test blocks)
fn main() -> Unit = println("fine")'

run() { SPEC_FILES="$*" ALMIDE_BIN="$BIN" bash scripts/check-spec-test-files.sh >/dev/null 2>&1; }
bad=0
run "$tmp/ok_test.almd" "$tmp/main_test.almd" "$tmp/neg_ok_test.almd" \
  || { echo "::error::spec-test-files negative control: the well-formed files were refused"; bad=1; }
for f in arity orphan neg_bad; do
  if run "$tmp/ok_test.almd" "$tmp/${f}_test.almd"; then
    echo "::error::spec-test-files negative control: ${f}_test.almd was accepted"
    bad=1
  fi
done
[ "$bad" = 0 ] && echo "spec-test-files negative controls: 1 positive + 3 negatives all behaved"
exit $bad
