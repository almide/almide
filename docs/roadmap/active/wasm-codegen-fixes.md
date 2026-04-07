<!-- description: Fix WASM codegen issues found during porta agent development -->

# WASM Codegen Fixes

Issues discovered while building and testing WASM agents with porta's built-in interpreter. All confirmed by running Almide-compiled `.wasm` binaries through porta's WASM interpreter.

## 1. String interpolation causes unreachable trap

**Severity**: Critical — blocks most non-trivial agents

```almide
let x = 42
println("value=${int.to_string(x)}")  // WASM trap: unreachable
```

Works in native target. Crashes in WASM target. The string interpolation codegen produces WASM code that hits `unreachable`.

**Workaround**: Manual concatenation works for some cases (`"value=" + int.to_string(x)`) but has its own issue (see below).

## 2. String concatenation argument order reversed in WASM

**Severity**: High — produces wrong results silently

```almide
let s = "value=" + int.to_string(42)
println(s)
// Expected: value=42
// Actual:   42value=
```

When the right operand of `+` is a function call result, the WASM code swaps the operands. Literal-only concatenation (`"a" + "b"`) is correct.

**Root cause hypothesis**: Evaluation order in WASM codegen. The function call result is pushed to the stack before the left operand, but the concat function pops them as (first, second).

## 3. f32 opcodes not emitted / handled

**Severity**: Medium — blocks agents that use json module

```
error: unknown opcode: 0xb0 at pos 1227
```

Agents that `import json` produce WASM binaries with f32 instructions (0xb0 = f32.convert_i64_s). These are either:
- Not needed (Almide doesn't use f32) and shouldn't be emitted, or
- Needed and porta's interpreter should handle them

**Workaround**: Avoid `import json` in WASM agents (severely limiting).

## Impact

These three issues together mean WASM agents are currently limited to:
- Simple `println` with string literals
- Basic `int.to_string` (standalone, not in interpolation)
- No JSON parsing, no complex string operations

Fixing issues 1 and 2 would unblock most practical agents. Issue 3 blocks agents that need JSON.
