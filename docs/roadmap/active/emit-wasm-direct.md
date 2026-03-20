# Direct WASM Emission [ACTIVE]

## Status: 15/66 lang tests pass (23%)

## Current Architecture

```
src/codegen/emit_wasm/
├── mod.rs          WasmEmitter, assembly, lambda pre-scan, closures (956 lines)
├── values.rs       Ty → ValType mapping, byte_size, field_offset (55 lines)
├── strings.rs      String literal interning (35 lines)
├── runtime.rs      12 runtime functions (700 lines)
├── expressions.rs  emit_expr + operators + helpers (720 lines)
├── calls.rs        emit_call + stdlib + closures + list.map/get (580 lines)
├── collections.rs  Record/Tuple/List/Map construction (245 lines)
├── control.rs      for-in/match/do-block (520 lines)
├── statements.rs   emit_stmt + local scan + scratch depth (400 lines)
└── functions.rs    IrFunction → WASM function (80 lines)
```

## Remaining Work: 51 files to pass

### Tier 1: Validation Errors (14 files)

Must fix WASM validation errors — the entire module is rejected if any function is invalid.

| Category | Files | Root Cause | Fix |
|----------|-------|-----------|-----|
| Codec/JSON (7) | codec_*, value_utils | Auto-generated codec functions use unimplemented stdlib | Implement json.stringify/parse or skip codec functions |
| Lambda type inference (3) | edge_cases, function_test, scope_test | Lambda params have TypeVar/Unknown type when passed to HOFs | Improve lambda param type resolution in pre_scan |
| Generic TypeVar in body (2) | generics, type_system | Monomorphized functions still have TypeVar in body expressions | Fix mono.rs type substitution for body expressions |
| Record default fields (1) | default_fields | RecordPattern with default field values | Handle default fields in match |
| Fan in effect fn (1) | fan_test | Fan block inside effect fn with auto-unwrap | ResultPropagation + Fan interaction |

### Tier 2: stdlib Implementation (20+ files)

Tests that trap on unimplemented stdlib calls. Each stdlib function unblocks 1-3 files.

| Priority | Function | Files Unblocked | Complexity |
|----------|----------|-----------------|------------|
| **High** | `list.filter` | data_types, lambda_test, operator_test | Medium (dynamic output size) |
| **High** | `string.trim` | string_test, pipe_test | Medium (whitespace strip runtime) |
| **High** | `map.get` | map_edge, map_literal | Medium (key lookup) |
| **High** | `list.fold` | eq_protocol, operator_test | Easy (accumulator loop) |
| **Medium** | `math.pi` (constant) | import_test | Easy |
| **Medium** | `PowFloat/PowInt` | expr_test | Medium (loop or intrinsic) |
| **Medium** | `for over Map` | for_test, for_tuple_test | Medium (Map iteration) |
| **Medium** | `list.zip` | control_flow_test | Medium |
| **Low** | `fan.any/map/race` | fan_ext/map/race | Hard (needs sequential simulation) |
| **Low** | `json.stringify/encode/decode` | codec_*, prelude | Very Hard (JSON parser) |
| **Low** | `hash` protocol | hash_protocol_test | Medium |

### Tier 3: Language Feature Gaps (10+ files)

| Feature | Files | Description |
|---------|-------|-------------|
| Match guard expressions | match_edge_test | `match x { n if n > 0 => ... }` |
| IndexAssign (`xs[i] = v`) | variable_test | Mutable list element assignment |
| Result deep equality | effect_fn, error_test, equality_test | Compare ok/err with deep inner comparison |
| Variant deep equality | equality_test, eq_protocol | Compare variant constructors recursively |
| Nested open record | open_record_test | Open record with nested fields |
| Guard in braceless body | do_block_pure_test | `fn f(x) = do { guard ... }` without braces |
| Convention methods (UFCS) | trait_impl, derive_conventions | `x.method()` → convention method dispatch |
| Nested variant record | variant_record_test | Variant with record payload, nested matching |

### Tier 4: Optimization (post-correctness)

- Dead code elimination (remove unused runtime functions)
- Constant folding in WASM
- Function inlining
- Memory compaction

## Binary Size

| Program | Almide | MoonBit | Ratio |
|---------|-------:|--------:|------:|
| Hello World | 1,195 B | 1,717 B | 1.4x |
| FizzBuzz | 1,296 B | 13,215 B | 10.2x |
| Fibonacci | 1,253 B | 13,186 B | 10.5x |
| Closure | 1,297 B | 13,146 B | 10.1x |
| Variant | 1,392 B | 13,423 B | 9.6x |

After DCE, Hello World should return to ~600B.
