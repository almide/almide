#!/usr/bin/env bash
# Negative controls for the runtime unsafe ratchet (#4007). Each case copies
# runtime/rs/src and the allowlist into a throwaway tree, changes one thing,
# and checks the gate's verdict. The real tree is never touched.
set -euo pipefail
export LC_ALL=C
cd "$(dirname "$0")/.."
root="$PWD"
GATE="$root/scripts/check-runtime-unsafe-ratchet.sh"

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

fresh() {
  rm -rf "$tmp/t"
  mkdir -p "$tmp/t/runtime/rs" "$tmp/t/scripts"
  cp -R "$root/runtime/rs/src" "$tmp/t/runtime/rs/src"
  cp "$root/scripts/runtime-unsafe-allowlist.txt" "$tmp/t/scripts/"
}
run() { RUNTIME_UNSAFE_ROOT="$tmp/t" bash "$GATE" >"$tmp/out" 2>&1; }
green() { if run; then echo "ok   $1"; else echo "FAIL: $1 — expected green" >&2; cat "$tmp/out" >&2; exit 1; fi; }
red() { if run; then echo "FAIL: $1 — expected red" >&2; cat "$tmp/out" >&2; exit 1; else echo "ok   $1 (red)"; fi; }

fresh
green "the tree as committed"

fresh
printf '\nfn sneak(p: *const u8) -> u8 { unsafe { *p } }\n' >> "$tmp/t/runtime/rs/src/matrix.rs"
red "one new unsafe line in a listed file"

fresh
printf '\nfn sneak(p: *const u8) -> u8 { unsafe { *p } }\n' >> "$tmp/t/runtime/rs/src/int.rs"
red "an unsafe line in a file the allowlist does not name"

fresh
printf '\n    // unsafe is only mentioned in this comment\n' >> "$tmp/t/runtime/rs/src/int.rs"
green "the word unsafe inside a comment is not counted"

fresh
awk '$2 == "runtime/rs/src/matrix.rs" { $1 = $1 + 1 } { print }' "$root/scripts/runtime-unsafe-allowlist.txt" > "$tmp/t/scripts/runtime-unsafe-allowlist.txt"
red "an allowlist count above the tree (stale)"

fresh
printf '1 runtime/rs/src/gone.rs os-ffi — removed file\n' >> "$tmp/t/scripts/runtime-unsafe-allowlist.txt"
red "an allowlist line for a file that does not exist"

echo "runtime-unsafe-ratchet negative controls: all verdicts as expected"
