# Direct WASM Emission [ACTIVE]

## Status: Phase 1 complete, WASM test runner needed

## Current Architecture

```
src/codegen/emit_wasm/
├── mod.rs          WasmEmitter, assembly, lambda pre-scan, closures (980 lines)
├── wasm_macro.rs   wasm! DSL macro for instruction emission (372 lines)
├── values.rs       Ty → ValType mapping, byte_size, field_offset (61 lines)
├── strings.rs      String literal interning (35 lines)
├── runtime.rs      14 runtime functions via wasm! macro (1135 lines)
├── expressions.rs  emit_expr + operators + helpers (780 lines)
├── calls.rs        emit_call + stdlib + closures + list.map/filter/fold (1665 lines)
├── collections.rs  Record/Tuple/List/Map construction (266 lines)
├── control.rs      for-in/match/do-block (617 lines)
├── statements.rs   emit_stmt + local scan + scratch depth (442 lines)
└── functions.rs    IrFunction → WASM function (85 lines)
```

Total: 6,438 lines

## Implemented

- Literals (Int, Float, Bool, String, Unit)
- Variables (local, global, top-level let)
- Binary/Unary operators (arithmetic, comparison, logical, string concat, list concat)
- Control flow (if/else, while, for..in Range, for..in List, break, continue)
- Match expressions (wildcard, bind, literal, constructor, record pattern, tuple, Some/None, Ok/Err, guards)
- Functions (user-defined, recursion, closures, FnRef, call_indirect)
- Records (construction, spread, field access, variant tag)
- Tuples (construction, index, destructure)
- Lists (construction, index access, list.map/filter/fold/reverse/sort/get/enumerate)
- Option/Result (construction, pattern matching, deep equality)
- String interpolation
- Effect fn (auto-unwrap via ResultPropagation pass + Try node)
- Do blocks (with guard → loop structure)
- Fan blocks (sequential fallback)
- wasm! macro DSL (96% of instruction emission migrated)

## Remaining: WASM stdlib

Functions that need WASM-native implementation (currently stub or missing):

### String

| Function | Priority | Notes |
|----------|----------|-------|
| `string.split` | High | Needs delimiter search + list construction |
| `string.join` | High | List of strings → single string |
| `string.slice` | Medium | Substring extraction |
| `string.replace` | Medium | Search and replace |
| `string.repeat` | Medium | String repetition |
| `string.reverse` | Medium | Byte reversal (ASCII) |
| `string.index_of` | Low | First occurrence search |
| `string.pad_start/pad_end` | Low | Padding |
| `string.trim_start/trim_end` | Low | One-sided trim |
| `string.get` | Low | Character at index |
| `string.count` | Low | Substring count |

### Float

| Function | Priority | Notes |
|----------|----------|-------|
| `float.to_string` (proper) | High | Currently truncates to int; needs decimal output |

### List

| Function | Priority | Notes |
|----------|----------|-------|
| `list.find` | Medium | First matching element |
| `list.any/all` | Medium | Predicate checks |
| `list.take/drop` | Medium | Sublist operations |
| `list.zip` | Medium | Pair two lists |
| `list.flat_map` | Medium | Map + flatten |
| `list.contains` | Low | Element search |
| `list.sort_by` | Low | Custom comparator sort |
| `list.count` | Low | Predicate count |
| `list.filter_map` | Low | Filter + map combined |

### Map

| Function | Priority | Notes |
|----------|----------|-------|
| `map.get` | High | Key lookup → Option |
| `map.set` | High | Key insert/update |
| `map.keys/values` | Medium | Iteration |
| `map.contains_key` | Medium | Key existence check |

### JSON/Codec

| Function | Priority | Notes |
|----------|----------|-------|
| `json.stringify` | Low | Value → JSON string |
| `json.parse` | Low | JSON string → Value |

### Math

| Function | Priority | Notes |
|----------|----------|-------|
| `math.pow` (Int/Float) | Medium | Exponentiation loop |

## Binary Size

| Program | Almide | MoonBit | Ratio |
|---------|-------:|--------:|------:|
| Hello World | 1,195 B | 1,717 B | 1.4x |
| FizzBuzz | 1,296 B | 13,215 B | 10.2x |
| Fibonacci | 1,253 B | 13,186 B | 10.5x |
| Closure | 1,297 B | 13,146 B | 10.1x |
| Variant | 1,392 B | 13,423 B | 9.6x |

After DCE, Hello World should return to ~600B.

## Next Steps

1. WASM test runner (`almide test --target wasm`)
2. High-priority stdlib (string.split/join, float.to_string, map.get/set)
3. Dead code elimination for unused runtime functions
