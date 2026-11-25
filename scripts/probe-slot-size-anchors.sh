#!/usr/bin/env bash
# Probe only, not a gate: for five contract programs, the module, the witness
# bundle, the anchor file and the byte-level check's own account, under the
# unmutated compiler and under one mutant of the emitter (every aggregate slot
# one byte wide). The question the files answer offline: is an event anchored
# at the accesses that the mutant moves past the end of their block?
#
#   scripts/probe-slot-size-anchors.sh <out-dir>
set -uo pipefail
export LC_ALL=C
cd "$(dirname "$0")/.."
ROOT="$PWD"
OUT="${1:?usage: scripts/probe-slot-size-anchors.sh <out-dir>}"
mkdir -p "$OUT"
OUT="$(cd "$OUT" && pwd)"
TD="${CARGO_TARGET_DIR:-$ROOT/target}/release"
ALMIDE="$TD/almide"
PRODUCER="$TD/examples/emit_structural_bundle"

PROGRAMS="
spec/wasm_cross/bytes_writer_mut_from_declaration.almd
spec/wasm_cross/closure_capture_escapes_frame.almd
spec/wasm_cross/effect_slot_unit_block_tail.almd
spec/wasm_cross/grain_functions.almd
spec/wasm_cross/top_let_fn_value_closure_param.almd
"

cargo build -q --release --locked -p almide-anchor-check || exit 1
mkdir -p "$OUT/bin"
cp "$TD/almide-anchor-check" "$OUT/bin/almide-anchor-check"
CHECK="$OUT/bin/almide-anchor-check"

build() {
  cargo build -q --release --locked --bin almide && cargo build -q --release --locked -p almide-wasm --example emit_structural_bundle
  # The driver must not find a verifier beside itself: it would refuse to
  # write a module whose witness the verifier rejects.
  rm -f "$TD/almide-verify"
}

# one <label> <program> <dir>
one() {
  local label="$1" f="$2" d="$3" rc=0
  mkdir -p "$d"
  if ! "$ALMIDE" build "$f" --target wasm -o "$d/m.wasm" > "$d/build.err" 2>&1 < /dev/null; then
    echo "   [$label] the build refuses: $(head -n 2 "$d/build.err" | tr '\n' ' ' | cut -c1-200)"; return
  fi
  if ! "$PRODUCER" "$f" --artifact "$d/m.wasm" --anchors "$d/m.anchors" > "$d/m.bundle" 2> "$d/producer.err" < /dev/null; then
    echo "   [$label] the producer refuses: $(head -n 2 "$d/producer.err" | tr '\n' ' ' | cut -c1-200)"; return
  fi
  "$CHECK" "$d/m.wasm" "$d/m.bundle" "$d/m.anchors" > "$d/check.verdict" 2>&1 || rc=$?
  "$CHECK" --explain "$d/m.wasm" "$d/m.bundle" "$d/m.anchors" > "$d/check.out" 2>&1
  echo "   [$label] check rc $rc; module $(sha256sum < "$d/m.wasm" | cut -c1-12), bundle $(sha256sum < "$d/m.bundle" | cut -c1-12) ($(wc -l < "$d/m.bundle" | tr -d ' ') lines), anchors $(sha256sum < "$d/m.anchors" | cut -c1-12) ($(wc -l < "$d/m.anchors" | tr -d ' ') lines)"
  grep -E '^(RC |ANCHORS |.*REJECT)' "$d/check.verdict" | cut -c1-400 | head -n 4 | sed 's/^/      /'
}

name_of() { printf '%s' "$1" | sed 's|.*/||; s|\.almd$||'; }

build || { echo "the unmutated compiler does not build"; exit 1; }
echo "=================== unmutated"
echo "$PROGRAMS" | while read -r f; do
  [ -n "$f" ] || continue
  echo "== $f"
  one base "$f" "$OUT/base/$(name_of "$f")"
done

# The mutant, as the mutation tool wrote it: SliceTy::slot_size returns 1.
python3 -I - <<'PY' || { echo "the mutant does not apply"; exit 1; }
p = "crates/almide-wasm/src/lib.rs"
s = open(p, encoding="utf-8").read()
a = """    fn slot_size(self) -> u32 {
        match self.val_type() {
            ValType::I64 | ValType::F64 => 8,
            _ => 4,
        }
    }
"""
assert s.count(a) == 1, s.count(a)
open(p, "w", encoding="utf-8").write(s.replace(a, "    fn slot_size(self) -> u32 {\n        1\n    }\n"))
PY
git diff --stat | tail -n 1

if build; then
  echo "=================== mutant: every aggregate slot one byte wide"
  echo "$PROGRAMS" | while read -r f; do
    [ -n "$f" ] || continue
    n="$(name_of "$f")"
    echo "== $f"
    one mutant "$f" "$OUT/mutant/$n"
    for x in wasm bundle anchors; do
      if [ -f "$OUT/base/$n/m.$x" ] && [ -f "$OUT/mutant/$n/m.$x" ]; then
        if cmp -s "$OUT/base/$n/m.$x" "$OUT/mutant/$n/m.$x"; then echo "      $x: same as unmutated"; else echo "      $x: differs from unmutated"; fi
      fi
    done
  done
else
  echo "the mutated compiler does not build"
fi
git checkout -- crates/almide-wasm/src/lib.rs
