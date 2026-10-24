#!/usr/bin/env bash
# RUNTIME UNSAFE LEDGER (#4007). The native leg's memory safety is "Rust's safe code";
# every `unsafe` in code that ends up inside a native program is outside it. This gate
# counts, per file, the lines that USE the keyword `unsafe` (a block, `fn`, `impl`,
# `extern` or `trait`; a comment or the bare word in prose is not a use) and refuses any
# file whose count differs from its row in proofs/runtime-unsafe-allowlist.toml (a new
# `unsafe` needs a row, a class and a reason; a removed one lowers the row in the same
# change). Every row's class must be one of the ledger's closed set.
#
# Scope — everything the compiler pastes into a native program:
#   runtime/rs/src                         the runtime template
#   crates/almide-rt-core/src              the cores runtime/rs `include!`s
#   crates/almide-codegen/src              the emitted preludes (Rust held in string
#                                          templates; src/generated mirrors runtime/rs
#                                          and is left out)
#   crates/almide-mir/src                  the v1 native render
#   crates/almide-kernel/src               inlined beside matrix.rs (ALMIDE_KERNEL_INLINE)
#   crates/almide-base/src/native_stack_guard.rs   the C-196 stack guard both renders emit
#
#   --self-test  forge an extra `unsafe` in a runtime file and in an emitted prelude of a
#                scratch copy and expect a refusal each time
set -uo pipefail
export LC_ALL=C
ROOT="${RUNTIME_UNSAFE_ROOT:-$(cd "$(dirname "$0")/.." && pwd)}"
LEDGER="$ROOT/proofs/runtime-unsafe-allowlist.toml"
TREES=(runtime/rs/src crates/almide-rt-core/src crates/almide-codegen/src crates/almide-mir/src crates/almide-kernel/src crates/almide-base/src/native_stack_guard.rs)
EXCLUDE='^crates/almide-codegen/src/generated/'
CLASSES="os-ffi blas-ffi region-arena fan-sync ffi-surface simd-intrinsic inline-asm user-ffi"

if [ "${1:-}" = "--self-test" ]; then
  tmp=$(mktemp -d); trap 'rm -rf "$tmp"' EXIT
  for t in "${TREES[@]}"; do [ -e "$ROOT/$t" ] && mkdir -p "$tmp/$(dirname "$t")" && cp -R "$ROOT/$t" "$tmp/$t"; done
  mkdir -p "$tmp/proofs" "$tmp/scripts"; cp "$LEDGER" "$tmp/proofs/"; cp "$0" "$tmp/scripts/"
  run() { RUNTIME_UNSAFE_ROOT="$tmp" bash "$tmp/scripts/check-runtime-unsafe.sh" >/dev/null; }
  run || { echo "self-test: the unforged copy failed"; exit 1; }
  printf '\nfn forged() { unsafe { std::hint::unreachable_unchecked() } }\n' >> "$tmp/runtime/rs/src/int.rs"
  if run; then echo "self-test: an unlisted unsafe in int.rs was accepted"; exit 1; fi
  sed -i.bak '$d' "$tmp/runtime/rs/src/int.rs"; sed -i.bak '$d' "$tmp/runtime/rs/src/int.rs"
  printf '// a comment that says unsafe\n' >> "$tmp/runtime/rs/src/matrix.rs"
  run || { echo "self-test: a comment mentioning unsafe was counted"; exit 1; }
  # An emitted prelude: an `unsafe` block inside a string template is code the program runs.
  printf '\nconst FORGED: &str = "fn f(p: *const u8) -> u8 { unsafe { *p } }";\n' >> "$tmp/crates/almide-codegen/src/walker/mod.rs"
  if run; then echo "self-test: a forged unsafe in an emitted prelude (walker/mod.rs) was accepted"; exit 1; fi
  sed -i.bak '$d' "$tmp/crates/almide-codegen/src/walker/mod.rs"
  # A row whose class is outside the closed set.
  sed -i.bak 's/^class = "os-ffi"/class = "c-call"/' "$tmp/proofs/runtime-unsafe-allowlist.toml"
  if run; then echo "self-test: a row of an unknown class was accepted"; exit 1; fi
  echo "self-test: unforged copy passes, an unlisted unsafe fails (runtime and emitted prelude), a comment is not counted, an unknown class fails"
  exit 0
fi

[ -f "$LEDGER" ] || { echo "::error::$LEDGER missing"; exit 1; }
actual=$(cd "$ROOT" && for t in "${TREES[@]}"; do [ -e "$t" ] && /usr/bin/grep -rnwH unsafe "$t" --include='*.rs'; done \
  | /usr/bin/grep -vE "$EXCLUDE" \
  | /usr/bin/grep -vE '^[^:]+:[0-9]+:[[:space:]]*(//|\*)' \
  | /usr/bin/grep -E '\bunsafe[[:space:]]*(\{|fn\b|impl\b|extern\b|trait\b)' \
  | cut -d: -f1 | sort | uniq -c | awk '{print $2" "$1}')
listed=$(awk -F'"' '/^path = /{p=$2} /^count = /{split($0,a," = "); print p" "a[2]}' "$LEDGER" | sort)
fail=0
while read -r f c; do [ -z "$f" ] && continue
  case " $CLASSES " in *" $c "*) ;; *) echo "::error::$f: class \"$c\" is not one of: $CLASSES"; fail=1;; esac
done <<< "$(awk -F'"' '/^path = /{p=$2} /^class = /{print p" "$2}' "$LEDGER")"
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
if [ $fail = 0 ]; then
  echo "runtime + emitted unsafe: $total line(s) in $(echo "$actual" | wc -l | tr -d ' ') file(s), every one listed with a class and a reason"
  awk -F'"' '/^count = /{split($0,a," = "); n=a[2]} /^class = /{s[$2]+=n} END{for (k in s) printf "  %-15s %s\n", k, s[k]}' "$LEDGER" | sort
fi
exit $fail
