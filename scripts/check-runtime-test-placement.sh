#!/usr/bin/env bash
# RUNTIME TEST-PLACEMENT GATE (#2507).
#
# A Rust unit test written under `runtime/rs` runs NOWHERE, and looks like
# coverage to the next reader. Two independent reasons, both measured:
#
#   * No cargo invocation ever compiles this directory. `runtime/rs` is not a
#     workspace member and cannot become one: its files are a source TEMPLATE
#     that the compiler concatenates into ONE flat module together with a
#     prelude it synthesises at emit time (rust_runtime_prelude, a string
#     literal in crates/almide-codegen/src/lib.rs). Added to `members` the
#     package fails with 220 rustc errors — see runtime/rs/README.md.
#   * Even where the source IS compiled — the emitted `almide_rt` rlib and
#     every generated project — `strip_test_blocks` deletes each
#     `#[cfg(test)]` block before rustc sees it.
#
# Seven such modules (env, int, string, list, base64, process, regex) sat here
# for months and were read as evidence. The behavioural oracle for runtime/rs
# is the executable `spec/` suite, which runs on every leg.
set -euo pipefail
export LC_ALL=C
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

hits="$(grep -rnE '^[[:space:]]*(#\[cfg\(test\)\]|#\[test\]|mod tests[[:space:]]*\{)' runtime/rs || true)"
n="$(printf '%s' "$hits" | grep -c . || true)"

if [ "$n" != "0" ]; then
  echo "$hits"
  echo
  echo "ERROR: $n Rust unit test marker(s) under runtime/rs — they never run (#2507)."
  echo
  echo "runtime/rs is not a workspace member (it cannot compile as a crate), and"
  echo "the compiler strips #[cfg(test)] out of every emitted crate. Put the test"
  echo "in spec/stdlib/ — or spec/wasm_cross/ for a cross-target promise — where"
  echo "it runs on every leg. See runtime/rs/README.md."
  exit 1
fi

echo "OK: 0 Rust unit test markers under runtime/rs — spec/ is its oracle (#2507)"
