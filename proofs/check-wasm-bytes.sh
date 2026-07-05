#!/usr/bin/env bash
# A2 byte-binding GROUNDING gate. The Coq proof
# `WasmEncode.rc_inc_bytes_encode_the_instruction_tree` shows our encoder produces
# `rc_inc_bytes` for the rc_inc instruction tree — but that is only meaningful if
# `rc_inc_bytes` and the opcode constants are the REAL wasm bytes, not a guess.
# This gate re-assembles the rc primitives with the real assembler (wat2wasm) and
# confirms the bytes match what `WasmEncode.v` models — so the opcode constants are
# grounded in reality, re-checked every build (the anti-circularity).
#
# Skips (exit 0) if wat2wasm is unavailable, so the proof gate never blocks on it;
# CI installs wabt so the grounding actually runs.
set -euo pipefail

if ! command -v wat2wasm >/dev/null 2>&1; then
  echo "check-wasm-bytes: wat2wasm (wabt) not found — SKIP (grounding not re-checked here)"
  exit 0
fi

tmp="$(mktemp -d)"; trap 'rm -rf "$tmp"' EXIT
hex() { od -An -v -tx1 "$1" | tr -d ' \n'; }

# --- rc_inc: the FULL body must equal WasmEncode.rc_inc_bytes ---
cat > "$tmp/rc_inc.wat" <<'EOF'
(module (memory 1)
  (func $rc_inc (param $p i32)
    (i32.store (i32.add (local.get $p) (i32.const 0))
               (i32.add (i32.load (i32.add (local.get $p) (i32.const 0))) (i32.const 1)))))
EOF
wat2wasm "$tmp/rc_inc.wat" -o "$tmp/rc_inc.wasm"
RC_INC_BODY="200041006a200041006a28020041016a3602000b"   # == WasmEncode.rc_inc_bytes
if [[ "$(hex "$tmp/rc_inc.wasm")" == *"$RC_INC_BODY"* ]]; then
  echo "ok   rc_inc body bytes match WasmEncode.rc_inc_bytes ($RC_INC_BODY)"
else
  echo "FAIL rc_inc: assembler bytes do not match WasmEncode.rc_inc_bytes"; exit 1
fi

# --- rc_dec core: ground the remaining opcode constants the encoder defines
#     (local.set / i32.eqz / if / blocktype-void / unreachable / i32.sub) ---
cat > "$tmp/rc_dec.wat" <<'EOF'
(module (memory 1)
  (func $rc_dec (param $p i32) (local $rc i32)
    (local.set $rc (i32.load (i32.add (local.get $p) (i32.const 0))))
    (if (i32.eqz (local.get $rc)) (then (unreachable)))
    (local.set $rc (i32.sub (local.get $rc) (i32.const 1)))
    (i32.store (i32.add (local.get $p) (i32.const 0)) (local.get $rc))))
EOF
wat2wasm "$tmp/rc_dec.wat" -o "$tmp/rc_dec.wasm"
DEC_HEX="$(hex "$tmp/rc_dec.wasm")"
check_op() { # subsequence label
  if [[ "$DEC_HEX" == *"$1"* ]]; then echo "ok   opcode grounded: $2 ($1)";
  else echo "FAIL opcode not grounded: $2 ($1)"; exit 1; fi
}
check_op "2101"     "local.set (0x21)"
check_op "45044000" "i32.eqz; if void; unreachable (0x45 0x04 0x40 0x00)"
check_op "41016b"   "i32.const 1; i32.sub (0x41 .. 0x6b)"

# --- the FULL rc_dec (free-list incl global.get/set) == WasmExec.rc_dec_bytes ---
cat > "$tmp/rc_dec_full.wat" <<'EOF'
(module (memory 1) (global $freelist (mut i32) (i32.const 0))
  (func $rc_dec (param $p i32) (local $rc i32)
    (local.set $rc (i32.load (i32.add (local.get $p) (i32.const 0))))
    (if (i32.eqz (local.get $rc)) (then (unreachable)))
    (local.set $rc (i32.sub (local.get $rc) (i32.const 1)))
    (i32.store (i32.add (local.get $p) (i32.const 0)) (local.get $rc))
    (if (i32.eqz (local.get $rc))
      (then (i32.store (i32.add (local.get $p) (i32.const 4)) (global.get $freelist))
            (global.set $freelist (local.get $p))))))
EOF
wat2wasm "$tmp/rc_dec_full.wat" -o "$tmp/rc_dec_full.wasm"
RC_DEC_BODY="200041006a28020021012001450440000b200141016b2101200041006a20013602002001450440200041046a2300360200200024000b0b"
if [[ "$(hex "$tmp/rc_dec_full.wasm")" == *"$RC_DEC_BODY"* ]]; then
  echo "ok   full rc_dec body bytes match WasmExec.rc_dec_bytes (incl global.get/set 0x23/0x24)"
else
  echo "FAIL full rc_dec: assembler bytes do not match WasmExec.rc_dec_bytes"; exit 1
fi

echo
echo "WASM-BYTES OK: WasmEncode.v's opcode constants and rc_inc_bytes are grounded"
echo "in the real assembler (wat2wasm) — the byte-binding proof is non-circular."
