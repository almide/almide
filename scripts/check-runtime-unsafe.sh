#!/usr/bin/env bash
# RUNTIME UNSAFE LEDGER (#4007). The native leg's memory safety is "Rust's safe code";
# every `unsafe` in the native runtime is outside it. This gate counts, per file, the
# non-comment lines holding the word `unsafe` under the native runtime trees, and in the
# preludes the compiler emits into every generated program (crates/almide-codegen/src/
# prelude_*.rs, #4023: the region arena's handle deref is `unsafe` in generated code), and
# refuses any file whose count differs from its row in proofs/runtime-unsafe-allowlist.toml
# (a new `unsafe` needs a row and a reason; a removed one lowers its row in the same change).
# Also emitted (#4007 step 7): every other crates/almide-codegen/src file (the walker's main
# SIGPIPE prelude, the @extern(c) glue; src/generated mirrors runtime/rs and is left out),
# the C-196 stack guard crates/almide-base/src/native_stack_guard.rs, and the
# crates/almide-kernel/src modules build_kernel_inline pastes beside matrix.rs.
# A line counts when it USES the keyword (`unsafe` before a block, fn, impl, extern or
# trait): a template that holds `// note\n unsafe {` on one source line is counted, and
# the bare word in prose or a keyword list is not.
#   --self-test  forge one extra `unsafe` line in a scratch copy and expect a refusal
set -uo pipefail
export LC_ALL=C
ROOT="${RUNTIME_UNSAFE_ROOT:-$(cd "$(dirname "$0")/.." && pwd)}"
LEDGER="$ROOT/proofs/runtime-unsafe-allowlist.toml"
TREES=(runtime/rs/src crates/almide-rt-core/src)
# The emitted preludes are Rust TEXT the compiler pastes into the generated crate (string
# literals or include_str!), so their `unsafe` is counted per source line like the runtime's.
PRELUDE_DIR=crates/almide-codegen/src
preludes() { (cd "$ROOT" && ls "$PRELUDE_DIR"/prelude_*.rs 2>/dev/null); }
# #4007: the rest of the emitted code. The kernel modules are lib.rs's `pub mod`s but
# `bridge`, the set build_kernel_inline (crates/almide-codegen/buildscript) embeds.
emitted() {
  (cd "$ROOT" && {
    find "$PRELUDE_DIR" -name '*.rs' ! -name 'prelude_*.rs' ! -path "$PRELUDE_DIR/generated/*" 2>/dev/null
    [ -f crates/almide-base/src/native_stack_guard.rs ] && echo crates/almide-base/src/native_stack_guard.rs
    [ -f crates/almide-kernel/src/lib.rs ] && sed -n 's/^pub mod \([a-z0-9_]*\);$/\1/p' crates/almide-kernel/src/lib.rs \
      | grep -vx bridge | sed 's|^|crates/almide-kernel/src/|; s|$|.rs|'
  } | sort)
}

if [ "${1:-}" = "--self-test" ]; then
  tmp=$(mktemp -d); trap 'rm -rf "$tmp"' EXIT
  for t in "${TREES[@]}"; do [ -d "$ROOT/$t" ] && mkdir -p "$tmp/$(dirname "$t")" && cp -R "$ROOT/$t" "$tmp/$t"; done
  mkdir -p "$tmp/$PRELUDE_DIR"; for f in $(preludes); do cp "$ROOT/$f" "$tmp/$f"; done
  for f in $(emitted) crates/almide-kernel/src/lib.rs; do [ -f "$ROOT/$f" ] && mkdir -p "$tmp/$(dirname "$f")" && cp "$ROOT/$f" "$tmp/$f"; done
  mkdir -p "$tmp/proofs" "$tmp/scripts"; cp "$LEDGER" "$tmp/proofs/"; cp "$0" "$tmp/scripts/"
  RUNTIME_UNSAFE_ROOT="$tmp" bash "$tmp/scripts/check-runtime-unsafe.sh" >/dev/null || { echo "self-test: the unforged copy failed"; exit 1; }
  printf '\nfn forged() { unsafe { std::hint::unreachable_unchecked() } }\n' >> "$tmp/runtime/rs/src/int.rs"
  if RUNTIME_UNSAFE_ROOT="$tmp" bash "$tmp/scripts/check-runtime-unsafe.sh" >/dev/null; then echo "self-test: an unlisted unsafe in int.rs was accepted"; exit 1; fi
  printf '// a comment that says unsafe\n' >> "$tmp/runtime/rs/src/matrix.rs"
  sed -i.bak '$d' "$tmp/runtime/rs/src/int.rs"; sed -i.bak '$d' "$tmp/runtime/rs/src/int.rs"
  RUNTIME_UNSAFE_ROOT="$tmp" bash "$tmp/scripts/check-runtime-unsafe.sh" >/dev/null || { echo "self-test: a comment mentioning unsafe was counted"; exit 1; }
  # #4023: an `unsafe` inside an emitted prelude's string literal is in scope too.
  printf '    s.push_str("fn forged(p: *const u8) -> u8 { unsafe { *p } }\\n");\n' >> "$tmp/$PRELUDE_DIR/prelude_region.rs"
  if RUNTIME_UNSAFE_ROOT="$tmp" bash "$tmp/scripts/check-runtime-unsafe.sh" >/dev/null; then echo "self-test: an extra emitted unsafe in prelude_region.rs was accepted"; exit 1; fi
  sed -i.bak '$d' "$tmp/$PRELUDE_DIR/prelude_region.rs"
  # #4007: an emitted template outside prelude_*.rs (the walker's main prelude), written
  # the way MAIN_SIGPIPE_PRELUDE is: a comment and the block on one source line.
  printf 'const FORGED: &str = "    // forged\\n    unsafe { *(0 as *const u8) };\\n";\n' >> "$tmp/$PRELUDE_DIR/walker/mod.rs"
  if RUNTIME_UNSAFE_ROOT="$tmp" bash "$tmp/scripts/check-runtime-unsafe.sh" >/dev/null; then echo "self-test: an extra emitted unsafe in walker/mod.rs was accepted"; exit 1; fi
  echo "self-test: unforged copy passes, an unlisted unsafe fails, a comment is not counted, an extra emitted unsafe fails (prelude_region.rs, walker/mod.rs)"
  exit 0
fi

[ -f "$LEDGER" ] || { echo "::error::$LEDGER missing"; exit 1; }
actual=$(cd "$ROOT" && { for t in "${TREES[@]}"; do [ -d "$t" ] && /usr/bin/grep -rnw unsafe "$t" --include='*.rs'; done
  for f in $(preludes) $(emitted); do /usr/bin/grep -Hnw unsafe "$f"; done; } \
  | /usr/bin/grep -vE '^[^:]+:[0-9]+:[[:space:]]*(//|\*)' \
  | /usr/bin/grep -E '\bunsafe[[:space:]]*(\{|fn\b|impl\b|extern\b|trait\b)' | cut -d: -f1 | sort | uniq -c | awk '{print $2" "$1}')
listed=$(awk -F'"' '/^path = /{p=$2} /^count = /{split($0,a," = "); print p" "a[2]}' "$LEDGER" | sort)
fail=0
while read -r f n; do [ -z "$f" ] && continue
  want=$(echo "$listed" | awk -v f="$f" '$1==f{print $2}')
  if [ -z "$want" ]; then echo "::error::$f has $n unsafe line(s) and no row in proofs/runtime-unsafe-allowlist.toml"; fail=1
  elif [ "$want" != "$n" ]; then echo "::error::$f has $n unsafe line(s); its row says $want — update the row (and its reason)"; fail=1; fi
done <<< "$(echo "$actual" | sort)"
while read -r f n; do [ -z "$f" ] && continue
  echo "$actual" | awk -v f="$f" '$1==f{found=1} END{exit !found}' || { echo "::error::$f is listed with $n but has no unsafe left — delete its row"; fail=1; }
done <<< "$listed"
total=$(echo "$actual" | awk '{s+=$2} END{print s+0}')
[ "$total" -gt 0 ] || { echo "::error::read no unsafe at all — the scan is broken, not the runtime clean"; exit 2; }
[ $fail = 0 ] && echo "runtime unsafe: $total line(s) in $(echo "$actual" | wc -l | tr -d ' ') file(s), every one listed with a reason"
exit $fail
