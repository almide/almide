#!/usr/bin/env bash
# A2 byte-EXECUTION grounding gate — the executor counterpart of check-wasm-bytes.sh.
#
# WasmExec.v models a bespoke wasm byte interpreter (`run_g`) and PROVES the rc bytes'
# memory effects: rc_inc takes a cell 4 -> 5 (`rc_inc_bytes_execute_to_rt_inc`); rc_dec on a
# uniquely-owned cell (rc=1) frees it to 0 (`rc_dec_bytes_frees_when_one`); rc_dec on an
# already-0 cell TRAPS — the double-free sentinel (`rc_dec_bytes_trap_on_zero`). Those proofs
# are only meaningful if `run_g` is FAITHFUL to a REAL wasm engine. check-wasm-bytes.sh grounds
# the ENCODER (our bytes == wat2wasm's); this grounds the EXECUTOR: it runs the SAME rc bytes on
# wasmtime and confirms the observed memory effect equals WasmExec.v's proven prediction — so the
# byte-EXECUTION binding is non-circular, re-checked every build.
#
# Skips (exit 0) if wasmtime / wat2wasm are unavailable, so the proof gate never blocks on them;
# CI installs both so the grounding actually runs.
set -euo pipefail

command -v wasmtime >/dev/null 2>&1 || { echo "check-wasm-exec: wasmtime not found — SKIP (executor grounding not re-checked here)"; exit 0; }
command -v wat2wasm >/dev/null 2>&1 || { echo "check-wasm-exec: wat2wasm not found — SKIP"; exit 0; }

tmp="$(mktemp -d)"; trap 'rm -rf "$tmp"' EXIT

# The rc_inc / rc_dec bodies are EXACTLY check-wasm-bytes.sh's grounded forms (== the renderer's
# emitted bytes == WasmEncode/WasmExec's modeled bytes). The exported wrappers set up a cell,
# run the rc op, and return the observed cell — so wasmtime executes the real bytes and we read
# the resulting memory. (`16` is an arbitrary live heap address; `$freelist` mirrors WasmExec.)
cat > "$tmp/rc.wat" <<'EOF'
(module
  (memory 1)
  (global $freelist (mut i32) (i32.const 0))
  (func $rc_inc (param $p i32)
    (i32.store (i32.add (local.get $p) (i32.const 0))
               (i32.add (i32.load (i32.add (local.get $p) (i32.const 0))) (i32.const 1))))
  (func $rc_dec (param $p i32) (local $rc i32)
    (local.set $rc (i32.load (i32.add (local.get $p) (i32.const 0))))
    (if (i32.eqz (local.get $rc)) (then (unreachable)))
    (local.set $rc (i32.sub (local.get $rc) (i32.const 1)))
    (i32.store (i32.add (local.get $p) (i32.const 0)) (local.get $rc))
    (if (i32.eqz (local.get $rc))
      (then (i32.store (i32.add (local.get $p) (i32.const 4)) (global.get $freelist))
            (global.set $freelist (local.get $p)))))
  ;; rc_inc on a cell holding 4 must leave it holding 5 (rt_inc).
  (func (export "inc_4_to_5") (result i32)
    (i32.store (i32.const 16) (i32.const 4))
    (call $rc_inc (i32.const 16))
    (i32.load (i32.const 16)))
  ;; rc_dec on a uniquely-owned cell (rc=1) must free it (rc -> 0).
  (func (export "dec_free_1_to_0") (result i32)
    (i32.store (i32.const 16) (i32.const 1))
    (call $rc_dec (i32.const 16))
    (i32.load (i32.const 16)))
  ;; freeing the cell must also RECLAIM it: $freelist points at the freed block (16) for reuse —
  ;; the leak-freedom mechanism (a freed block is returned to the allocator, not lost).
  (func (export "dec_free_sets_freelist") (result i32)
    (i32.store (i32.const 16) (i32.const 1))
    (call $rc_dec (i32.const 16))
    (global.get $freelist))
  ;; rc_dec on an already-0 cell must TRAP (the double-free sentinel).
  (func (export "dec_trap_on_0")
    (i32.store (i32.const 16) (i32.const 0))
    (call $rc_dec (i32.const 16))))
EOF
wat2wasm "$tmp/rc.wat" -o "$tmp/rc.wasm"

# Anti-drift: the assembled rc_inc body must still be WasmExec/WasmEncode's modeled bytes, so we
# are grounding the EXECUTION of exactly the proven bytes (not some other shape wat2wasm produced).
hex() { od -An -v -tx1 "$1" | tr -d ' \n'; }
RC_INC_BODY="200041006a200041006a28020041016a3602000b"
if [[ "$(hex "$tmp/rc.wasm")" != *"$RC_INC_BODY"* ]]; then
  echo "FAIL rc_inc bytes drift — the grounded module no longer contains WasmEncode.rc_inc_bytes"; exit 1
fi

check() { # invoke expected label
  local got
  got="$(wasmtime run --invoke "$1" "$tmp/rc.wasm" 2>/dev/null | tr -d '\r\n ')"
  if [ "$got" = "$2" ]; then echo "ok   exec grounded: $3 (wasmtime = $got)";
  else echo "FAIL exec: $3 — wasmtime got '$got' want '$2'"; exit 1; fi
}
check inc_4_to_5             5  "rc_inc cell 4 -> 5            (WasmExec.rc_inc_bytes_execute_to_rt_inc)"
check dec_free_1_to_0        0  "rc_dec rc=1 frees to 0        (WasmExec.rc_dec_bytes_frees_when_one)"
check dec_free_sets_freelist 16 "rc_dec rc=1 reclaims to \$freelist (the leak-freedom reclamation)"

# TRAP direction: rc_dec on rc=0 must trap (wasmtime exits non-zero with 'unreachable').
if wasmtime run --invoke dec_trap_on_0 "$tmp/rc.wasm" >/dev/null 2>&1; then
  echo "FAIL exec: rc_dec on rc=0 did NOT trap (the double-free sentinel did not fire)"; exit 1
else
  echo "ok   exec grounded: rc_dec rc=0 TRAPS          (WasmExec.rc_dec_bytes_trap_on_zero)"
fi

echo
echo "WASM-EXEC OK: wasmtime executes the rc bytes to EXACTLY WasmExec.v's proven memory"
echo "effects (inc 4->5, free 1->0, trap on 0) — the byte-EXECUTION binding is grounded in a"
echo "real engine, the executor counterpart of check-wasm-bytes.sh's encoder grounding."
