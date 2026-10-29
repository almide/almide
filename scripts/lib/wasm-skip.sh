# shellcheck shell=bash
# The `// wasm:skip` declaration, read the way `almide test` reads it
# (src/cli/test_wasm.rs, `wasm_test_preflight_outcome`): the literal
# `// wasm:skip` on one of the file's first three lines. Shared by every
# per-file wasm loop in CI so none of them keeps its own spelling of the
# rule — a looser `grep wasm:skip` matched prose that only MENTIONED the
# marker (spec/stdlib/random_test.almd, zlib_test.almd) and skipped files
# that run on wasm.
#
# wasm_skip_reason FILE — exit 0 and print the declared reason when FILE
# declares the skip, exit 1 otherwise.
wasm_skip_reason() {
  local line
  line=$(head -3 "$1" | grep -m1 -F '// wasm:skip') || return 1
  line=${line#*// wasm:skip}
  # Drop the separator authors put before the reason (` — `, `: `, ` - `).
  printf '%s\n' "$line" | sed -E 's/^[[:space:]]*(—|:|-)?[[:space:]]*//'
}
