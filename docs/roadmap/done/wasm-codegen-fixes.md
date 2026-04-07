<!-- description: WASM codegen issues were porta interpreter bugs, not codegen bugs -->
<!-- done: 2026-04-07 -->
# WASM Codegen Fixes (Resolved)

## Original Report

Three issues were reported as WASM codegen bugs during porta agent testing:

1. String interpolation causes unreachable trap
2. String concatenation argument order reversed
3. f32 opcodes not handled (opcode 0xB0)

## Root Cause Analysis

All three issues are bugs in **porta's WASM interpreter**, not in Almide's codegen. The generated WASM is spec-compliant and runs correctly on wasmtime.

| Issue | Porta Bug | File |
|-------|-----------|------|
| String interp trap | `read_memarg` ignores multi-memory flag (bit 6 of align), leaves mem_idx byte unconsumed → cascading parse errors | `binary.almd:233` |
| Concat order reversal | `pop_n_acc` appends in LIFO order (`acc + [val]` instead of `[val] + acc`), reversing function arguments | `interp.almd:91` |
| Unknown opcode 0xB0 | Opcode 0xAE mapped to `I64TruncF64S` but 0xAE is `i64.trunc_f32_s`; correct opcode 0xB0 not handled | `binary.almd:374` |

## Resolution

- Hotfixes applied to porta's `binary.almd` and `interp.almd`
- Long-term: porta will migrate to wasmtime via extern FFI (see porta roadmap: `wasmtime-migration.md`), eliminating the hand-rolled interpreter entirely
