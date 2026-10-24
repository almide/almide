#!/usr/bin/env bash
# RUNTIME UNSAFE LEDGER (#4007). The native leg's memory safety is "Rust's safe code";
# every `unsafe` in the native runtime is outside it. This gate counts, per file, the
# non-comment lines holding the word `unsafe` under the native runtime trees, and in the
# preludes the compiler emits into every generated program (crates/almide-codegen/src/
# prelude_*.rs, #4023: the region arena's handle deref is `unsafe` in generated code), and
# refuses any file whose count differs from its row in proofs/runtime-unsafe-allowlist.toml
# (a new `unsafe` needs a row and a reason; a removed one lowers its row in the same change).
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

if [ "${1:-}" = "--self-test" ]; then
  tmp=$(mktemp -d); trap 'rm -rf "$tmp"' EXIT
  for t in "${TREES[@]}"; do [ -d "$ROOT/$t" ] && mkdir -p "$tmp/$(dirname "$t")" && cp -R "$ROOT/$t" "$tmp/$t"; done
  mkdir -p "$tmp/$PRELUDE_DIR"; for f in $(preludes); do cp "$ROOT/$f" "$tmp/$f"; done
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
  echo "self-test: unforged copy passes, an unlisted unsafe fails, a comment is not counted, an extra emitted-prelude unsafe fails"
  exit 0
fi

[ -f "$LEDGER" ] || { echo "::error::$LEDGER missing"; exit 1; }
actual=$(cd "$ROOT" && { for t in "${TREES[@]}"; do [ -d "$t" ] && /usr/bin/grep -rnw unsafe "$t" --include='*.rs'; done
  for f in $(preludes); do /usr/bin/grep -Hnw unsafe "$f"; done; } \
  | /usr/bin/grep -vE '^[^:]+:[0-9]+:[[:space:]]*(//|\*)' | /usr/bin/grep -vE '//.*\bunsafe\b' | cut -d: -f1 | sort | uniq -c | awk '{print $2" "$1}')
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
