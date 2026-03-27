<!-- description: Fix 44 WASM runtime traps (protocols, maps, closures, strings) -->
<!-- done: 2026-03-23 -->
# WASM Runtime Traps

## Status: 21 pass, 44 runtime traps, 8 skipped, 0 compile failures

## Trap Categories (44 files)

### Protocol/Convention dispatch [8 files]
basic protocol method, basic protocol satisfaction, builder pattern via protocol,
convention method resolution, convention method via UFCS, diamond protocol,
generic function with protocol bound, protocol methods chained via pipe

**Root**: Protocol method calls fall through to `unreachable`.
Convention/protocol dispatch is not implemented in WASM codegen.
Rust codegen transforms to direct calls via `TypeName.method(self)`, but WASM requires either table indirect calls or name resolution.

**Fix**: Resolve protocol methods to Named calls in emit_call.

### Map operations [4 files]
map basic operations, map creation and get, empty map operations, empty map with type annotation

**Root**: Map type WASM runtime is not implemented.
List is implemented with linear memory + length header, but Map requires a hash table.

**Fix**: Implement Map runtime (hash + bucket array). Major effort.

### Map iteration [3 files]
for over map, for with map tuple destructure, for with zip

**Root**: WASM codegen for `for k, v in map` is not implemented.
List iteration is implemented. Map iteration depends on the Map runtime.

**Fix**: Implement after Map runtime.

### Fan (sequential fallback) [4 files]
fan basic, fan.map, fan.race, fan.any

**Root**: WASM codegen for fan (concurrent execution) is not implemented.
WASM is single-threaded, but fan's semantics is "execute multiple expressions and collect results".
Sequential execution produces the same results.

**Fix**: Implement fan with sequential fallback.
- `fan { a, b, c }` → `(a(), b(), c())`
- `fan.map(list, fn)` → `list.map(list, fn)`
- `fan.race(list, fn)` → return the first element

### Deep equality [3 files]
nested list equality, deep equality on nested structures, recursive variant types

**Root**: `assert_eq` で nested containers (List[List[T]], variant with payloads) の
deep equality が incomplete。`option_eq_i64` / `list_eq` が shallow comparison のみ。

**Fix**: runtime の eq 関数を recursive にする。型情報に基づいた dispatch が必要。

### Record/Variant features [3 files]
let record destructure, nested open record, basic variant record construction

**Root**: WASM codegen for record destructure (`let { name, age } = person`) and variant record
construction is incomplete.

**Fix**: Implement BindDestructure in emit_stmt and the variant record case in emit_record.

### Type features [5 files]
match constructor with payload, match nested option, default fields - omit all defaults,
comparison on type variable, multi type param generic

**Root**: Individual pattern matching and generic function codegen edge cases.
Identify the first trap point in each test and fix individually.

**Fix**: Investigate and fix each case.

### String operations [2 files]
string split and join, json stringify

**Root**: WASM runtime for `string.split`, `string.join` is not implemented (stub).

**Fix**: Implement string split/join in runtime.

### Import/Codec [4 files]
encode/decode roundtrip, result.map, naming strategy, unit variant encode

**Root**: Codec-related + module import WASM support.

**Fix**: Most should be skipped. Add result.map to stdlib runtime.

### Misc codegen [4 files]
UFCS basic, pipe into list function, nested closure, do guard

**Root**: Individual codegen patterns. UFCS transformation, pipe desugaring, nested closure capture, etc.

**Fix**: Investigate and fix each case.

### Other [4 files]
structured error, string keys, tco deep recursion, variant roundtrip

## Priority Order

| Priority | Category | Files | Impact | Effort |
|----------|----------|-------|--------|--------|
| 1 | Fan sequential fallback | 4 | -4 traps | Low |
| 2 | Protocol dispatch | 8 | -8 traps, unblocks many tests | Medium |
| 3 | Record/Variant features | 3 | -3 traps | Medium |
| 4 | Type features | 5 | -5 traps | Medium (individual) |
| 5 | Misc codegen | 4 | -4 traps | Medium (individual) |
| 6 | Deep equality | 3 | -3 traps | Medium |
| 7 | String operations | 2 | -2 traps | Low |
| 8 | Import/Codec skip | 4 | -4 traps | 5 min |
| 9 | Map runtime | 7 | -7 traps | High (hash table) |
| 10 | TCO | 1 | -1 trap | High |

## Expected Progress

| After step | Pass | Traps |
|------------|------|-------|
| Current | 21 | 44 |
| 1 (fan) | 25 | 40 |
| 2 (protocol) | 29+ | 32 |
| 3-5 (features) | 41+ | 20 |
| 6-8 (equality/string/codec) | 50+ | 11 |
| 9 (map) | 57+ | 4 |
