#!/usr/bin/env bash
# RELEASE-ONLY TEST COVERAGE (#2122).
#
# A test marked `#[cfg_attr(debug_assertions, ignore = …)]` runs only in a
# RELEASE-shaped job. The shards run the `ci-test` profile, which inherits
# `dev` — so every such test is SKIPPED there, silently, and a release-only
# test that no release job names runs nowhere at all.
#
# That is what happened to `tests/embedded_cross_test.rs`: the native⇄embedded
# cross net was skipped by every shard, covered by no release job, and failed
# for months on a nondeterministic fixture while CI read green.
#
# The six under `crates/almide-wasm/tests/` are covered by the commissioned
# wasm gates job (`cargo test --release -p almide-wasm …`). The ROOT package's
# are covered by one step, and this gate is what keeps that list honest: a new
# release-only root test that nobody added to the step fails here instead of
# disappearing.
set -euo pipefail
cd "$(dirname "$0")/.."

WORKFLOW=".github/workflows/ci.yml"
missing=0

for f in tests/*.rs; do
  grep -q 'cfg_attr(debug_assertions, ignore' "$f" || continue
  target="$(basename "$f" .rs)"
  if ! grep -q -- "--test $target" "$WORKFLOW"; then
    echo "::error::$f is release-only but no CI step names \`--test $target\` — it would run in NO job (the shards skip it under debug_assertions). Add it to the release-only step in $WORKFLOW."
    missing=$((missing + 1))
  fi
done

# The crate-level release-only tests ride their own package job; assert that
# job still exists rather than trusting the comment above.
grep -q 'cargo test --release --locked -p almide-wasm' "$WORKFLOW" || {
  echo "::error::the commissioned wasm gates no longer run almide-wasm in release — the six release-only tests there now run nowhere"
  missing=$((missing + 1))
}

[ "$missing" -eq 0 ] || exit 1
echo "release-only tests: OK — every release-only root test is named by a release CI step"
