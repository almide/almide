#!/usr/bin/env bash
# A2 byte-binding GROUNDING gate — the mechanized triangle (#932).
#
# The Coq proof `WasmEncode.rc_inc_bytes_encode_the_instruction_tree` (and
# WasmExec's rc_dec theorems) are about byte LISTS defined in the .v files.
# Those theorems mean something only if the lists are (a) the bytes the real
# assembler produces and (b) the bytes OUR RENDERER actually emits. This gate
# closes the triangle with ZERO hand-copied constants:
#
#   proofs/*.v byte lists  ←→  wat2wasm(assembled WAT)  ←→  the renderer's
#   (parsed from source)        (the real assembler)         OWN $rc_inc/$rc_dec
#                                                            (extracted from a
#                                                             rendered module)
#
# The previous edition assembled a WAT heredoc WRITTEN IN THIS SCRIPT — textually
# equivalent to the renderer's, but nothing enforced it: edit the renderer and
# every "the renderer's real bytes" theorem silently became a statement about a
# shell-script literal.
#
# Tool policy: locally a missing tool is an honest skip; in CI it is a FAILURE
# (#921) — a gate that exits 0 without its instrument is not a gate.
set -euo pipefail
cd "$(dirname "$0")"
ROOT=".."

require_or_skip() {
  command -v "$1" >/dev/null 2>&1 && return 0
  if [ "${CI:-}" = "true" ]; then
    echo "check-wasm-bytes: $1 not found — FAIL (CI must install it)"
    exit 1
  fi
  echo "check-wasm-bytes: $1 not found — SKIP (grounding not re-checked here)"
  exit 0
}
require_or_skip wat2wasm
require_or_skip cargo
require_or_skip python3

tmp="$(mktemp -d)"; trap 'rm -rf "$tmp"' EXIT
hex() { od -An -v -tx1 "$1" | tr -d ' \n'; }

# ── 1. Render a probe through THE renderer and extract its $rc_inc/$rc_dec ──
# The probe exercises a mutable-global Bytes read+write, which keeps both rc
# primitives reachable in the pruned output (a Dup renders `call $rc_inc`; the
# scope-end release renders `call $rc_dec`).
cat > "$tmp/probe.almd" <<'EOF'
var g = bytes.new(8)

fn main() -> Unit = {
  bytes.set_at(g, 0, 7)
  println(int.to_string(bytes.get_or(g, 0, 0 - 1)))
}
EOF
( cd "$ROOT" && cargo run --release -q -p almide-mir --example render_program -- "$tmp/probe.almd" ) \
  > "$tmp/probe.wat" 2>/dev/null
for f in rc_inc rc_dec; do
  python3 - "$tmp/probe.wat" "$f" > "$tmp/$f.func.wat" <<'PY'
import sys
w = open(sys.argv[1]).read()
name = sys.argv[2]
i = w.find(f"(func ${name} ")
if i < 0:
    i = w.find(f"(func ${name}\n")
if i < 0:
    sys.exit(f"extract: (func ${name} ...) not found in the rendered module")
d = 0
for j in range(i, len(w)):
    if w[j] == "(": d += 1
    elif w[j] == ")":
        d -= 1
        if d == 0:
            print(w[i:j+1]); sys.exit(0)
sys.exit(f"extract: unbalanced parens for ${name}")
PY
  echo "ok   extracted the renderer's real \$$f"
done

# ── 2. Parse the PROVEN byte lists out of the .v sources ───────────────────
# WasmEncode.rc_inc_bytes and WasmExec.rc_dec_bytes are `[a;b;…]` lists of
# decimal opcode/immediate bytes; hex-encode them here so the script carries
# no byte constant of its own.
vlist_hex() { # file defname
  python3 - "$1" "$2" <<'PY'
import re, sys
src = open(sys.argv[1]).read()
m = re.search(r"Definition\s+" + re.escape(sys.argv[2]) + r"\s*:\s*list Z\s*:=\s*\[(.*?)\]\.", src, re.S)
if not m:
    sys.exit(f"parse: Definition {sys.argv[2]} not found")
print("".join(f"{int(t):02x}" for t in re.split(r"[;\s]+", m.group(1).strip()) if t))
PY
}
RC_INC_BODY="$(vlist_hex WasmEncode.v rc_inc_bytes)"
RC_DEC_BODY="$(vlist_hex WasmExec.v rc_dec_bytes)"

# ── 3. Assemble the RENDERER's functions and compare against the proofs ────
{ printf '(module (memory 1)\n'; cat "$tmp/rc_inc.func.wat"; printf ')\n'; } > "$tmp/rc_inc.wat"
wat2wasm "$tmp/rc_inc.wat" -o "$tmp/rc_inc.wasm"
if [[ "$(hex "$tmp/rc_inc.wasm")" == *"$RC_INC_BODY"* ]]; then
  echo "ok   the renderer's \$rc_inc assembles to WasmEncode.rc_inc_bytes ($RC_INC_BODY)"
else
  echo "FAIL rc_inc: the RENDERER'S bytes do not match WasmEncode.rc_inc_bytes — the"
  echo "     renderer and the proof have drifted (exactly what this gate exists to catch)"
  exit 1
fi

{ printf '(module (memory 1) (global $freelist (mut i32) (i32.const 0))\n'; cat "$tmp/rc_dec.func.wat"; printf ')\n'; } > "$tmp/rc_dec.wat"
wat2wasm "$tmp/rc_dec.wat" -o "$tmp/rc_dec.wasm"
DEC_HEX="$(hex "$tmp/rc_dec.wasm")"
if [[ "$DEC_HEX" == *"$RC_DEC_BODY"* ]]; then
  echo "ok   the renderer's \$rc_dec assembles to WasmExec.rc_dec_bytes (incl. trap + free-list push)"
else
  echo "FAIL rc_dec: the RENDERER'S bytes do not match WasmExec.rc_dec_bytes — drift"
  exit 1
fi

# Opcode grounding for the constants WasmEncode defines, checked on the REAL
# rc_dec's assembled hex (not a synthetic snippet).
check_op() { # subsequence label
  if [[ "$DEC_HEX" == *"$1"* ]]; then echo "ok   opcode grounded: $2 ($1)";
  else echo "FAIL opcode not grounded: $2 ($1)"; exit 1; fi
}
check_op "2101"     "local.set (0x21)"
check_op "45044000" "i32.eqz; if void; unreachable (0x45 0x04 0x40 0x00)"
check_op "41016b"   "i32.const 1; i32.sub (0x41 .. 0x6b)"

echo
echo "WASM-BYTES OK: the RENDERER'S OWN \$rc_inc/\$rc_dec, assembled by wat2wasm,"
echo "match the byte lists the Rocq proofs are about — the byte-binding is"
echo "non-circular AND anchored to the shipping renderer, with no hand-copied"
echo "constants in between."
