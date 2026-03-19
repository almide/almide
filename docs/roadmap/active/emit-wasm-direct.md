# Direct WASM Emission [ACTIVE]

## Motivation

Current WASM path: `.almd → IR → Rust source → rustc → WASM` (433KB hello world)
Goal: `.almd → IR → WASM bytecode` (1-5KB hello world, 100-1000x improvement)

MoonBit achieves tens of bytes to single-digit KB with direct WASM emit. Almide could adopt the same approach for Playground speed and Edge deployment.

## Architecture (Codegen v3)

Almide's codegen v3 uses a three-layer architecture:

```
IR → Nanopass Pipeline → Walker/Emitter → Output
```

For text targets (Rust, TS, JS), the walker renders IR nodes via TOML templates. For WASM, a new **binary emitter** would walk the IR directly and produce `wasm_encoder` instructions — no template system needed.

```
src/codegen/
├── target.rs           ← Add Target::WasmDirect + pipeline config
├── emit_wasm/          ← NEW: IR → WASM binary
│   ├── mod.rs              Entry: IrProgram → Vec<u8>
│   ├── runtime.rs          Linear memory management (bump allocator)
│   ├── strings.rs          UTF-8 string on linear memory
│   ├── values.rs           Almide types → WASM value types
│   ├── expressions.rs      IrExpr → WASM instructions
│   ├── statements.rs       IrStmt → locals + instructions
│   ├── functions.rs        IrFunction → WASM function section
│   ├── collections.rs      List (dynamic array), Map (hash table)
│   └── patterns.rs         IrPattern → br_table / if-else chain
```

### Integration with Target System

```rust
// src/codegen/target.rs
Target::WasmDirect => Pipeline::new()
    .add(TypeConcretizationPass)
    // No CloneInsertion — WASM uses ref counting
    // No BorrowInsertion — WASM has no borrow checker
    .add(StdlibLoweringPass)  // resolve module calls
    .add(FanLoweringPass),    // fan → sequential (WASM has no threads yet)
```

The nanopass pipeline still applies for IR optimization. But instead of the template walker, a binary emitter walks the rewritten IR and produces WASM instructions via `wasm_encoder`.

### Crate Dependency

```toml
[dependencies]
wasm-encoder = "0.245"  # bytecodealliance low-level WASM encoder
```

## Almide Type → WASM Type Mapping

| Almide type | WASM type | Memory layout |
|-------------|-----------|---------------|
| Int | `i64` | local variable or stack |
| Float | `f64` | local variable or stack |
| Bool | `i32` (0/1) | local variable or stack |
| Unit | (omitted) | — |
| String | `i32` (pointer) | `[len: i32][data: u8...]` |
| List[T] | `i32` (pointer) | `[rc: i32][len: i32][cap: i32][data...]` |
| Record | `i32` (pointer) | fields laid out sequentially |
| Variant | `i32` (pointer) | `[tag: i32][payload...]` |
| Map | `i32` (pointer) | hash table structure |
| Option[T] | `i32` (pointer or 0) | None = 0, Some = pointer |
| fn(A)->B | `i32` (pointer) | `[funcidx: i32][env_ptr: i32]` |

**Value types** (Int, Float, Bool): WASM locals, zero-cost copy.
**Heap types** (String, List, Record, Variant): Linear memory with i32 pointers, ref counted.

## Implementation Phases

### Phase 0: PoC — Hello World (COMPLETE)

Generated minimal WASM binaries with `wasm_encoder`:
- WASI mode: 143 bytes (fd_write Hello World)
- Embed mode: 111 bytes (putchar import)
- wasm-gc mode: 77 bytes (struct + field access)

### Phase 1: Minimal Language Subset (COMPLETE)

IR-driven codegen replacing hardcoded PoC. 6-file module structure:

```
src/codegen/emit_wasm/
├── mod.rs          WasmEmitter, assembly, entry point
├── values.rs       Ty → ValType mapping
├── strings.rs      String literal interning ([len:i32][data:u8...])
├── runtime.rs      __alloc, __println_str, __int_to_string, __println_int
├── expressions.rs  IrExpr → WASM instructions
├── statements.rs   IrStmt → WASM instructions + local pre-scanning
└── functions.rs    IrFunction → compiled WASM function
```

Implemented:
- Int/Float literals, all arithmetic (+, -, *, /, %)
- Comparison (<, >, <=, >=, ==, !=) for Int and Float
- Bool, and/or/not
- let/var bindings → WASM locals
- if/then/else → WASM structured blocks
- fn definition and calls (including recursion)
- while loops with break/continue
- println (polymorphic: String, Int, Bool)
- String literals in data section
- int.to_string runtime function
- Bump allocator for heap allocation
- CLI: `almide build app.almd --target wasm`

Results:
- Hello World: **444 bytes** (target was <500B ✓)
- FizzBuzz: **538 bytes** (target was 1-3KB ✓, beat estimate by 5x)
- Fibonacci: **495 bytes**

### Phase 2: Collections + Closures (2-3 weeks)

- List, Map, Record, Variant, Tuple
- Closures: funcref + captured environment
- Reference counting
- for...in, while, do/guard

Goal: Most exercises pass.

### Phase 3: Full Feature Parity (3-4 weeks)

- match → br_table / if-else chain
- String interpolation
- effect fn: Result as tag + payload
- fan: sequential fallback
- Stdlib core modules (string, list, map, int, float, math)

Goal: 142/142 test files pass on WASM target.

### Phase 4: Optimization (ongoing)

- Dead code elimination
- Constant folding
- Function inlining
- List operation fusion

## Expected Binary Size

| Program | Rust-via (current) | Direct emit (est.) | MoonBit (ref.) |
|---------|-------------------|-------------------|----------------|
| Hello World | 433KB | 100-500B | ~30B |
| FizzBuzz | 433KB | 1-3KB | ~500B |
| Fibonacci | 433KB | 1-3KB | ~1KB |
| Quicksort | ~500KB | 10-30KB | ~5KB |
| Real app | ~1MB | 50-200KB | 30-100KB |

## Playground Impact

| | Current (JS emit) | Direct WASM emit |
|---|---|---|
| Compile | WASM compiler → JS string → eval | WASM compiler → WASM binary → instantiate |
| Runtime | JS runtime (`__almd_list` etc.) | Stdlib in WASM binary |
| Speed | ~10-50ms compile + eval overhead | ~5ms compile + instant start |
| Hacks | `patchRuntimeForBrowser` | None needed |

## Decision Criteria

**Do it if:**
- Playground speed is a priority
- Edge/WASM deployment is a real use case
- "Almide compiles fast" is a selling point

**Don't do it if:**
- LLM-first mission doesn't benefit directly
- Rust/TS/JS targets are sufficient
- Time is better spent on LSP, trait system, or cookbook

**Compromise:**
- Implement Phase 0-1 only to measure real size/speed
- Decide Phase 2+ based on measurements
- Keep Playground on JS emit until WASM direct is proven

## References

- [wasm-encoder](https://docs.rs/wasm-encoder/) — bytecodealliance WASM binary generator
- [WASM spec](https://webassembly.github.io/spec/)
- [WASI preview1](https://github.com/WebAssembly/WASI/blob/main/legacy/preview1/docs.md)
- [MoonBit](https://www.moonbitlang.com/) — direct WASM emit reference
