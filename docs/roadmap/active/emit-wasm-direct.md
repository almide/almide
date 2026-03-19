# Direct WASM Emission via wasm-gc [ACTIVE]

Branch: `feature/wasm-direct`

## Motivation

Current WASM path: `.almd → IR → Rust source → rustc → WASM` (433KB hello world)
Goal: `.almd → IR → WASM bytecode` via wasm-gc (MoonBit-equivalent architecture)

Rust via WASM has structural overhead: allocator (~16KB), fmt (~40KB), panic handler (~10KB). Direct emit bypasses all of this.

## Phase 0: PoC — DONE

Three modes tested, all generating valid WASM:

| Mode | Size | How | Validated |
|------|------|-----|-----------|
| WASI | 143 bytes | linear memory + fd_write | wasmtime run |
| Embed | 111 bytes | putchar import | wasm-tools validate |
| **wasm-gc** | **77 bytes** | struct + host GC | **wasm-tools validate** |

**Decision: wasm-gc is the path forward.** No linear memory, no allocator, host GC manages all heap objects. Same strategy as MoonBit.

wasmtime 42.0.1 confirmed wasm-gc support (struct, array, struct.new, struct.get, array.new_fixed).

## Architecture: wasm-gc

```
Almide IR  →  wasm-gc binary
                ├── struct    (Record, Variant payload)
                ├── array i8  (String)
                ├── array     (List[T])
                ├── i64       (Int)
                ├── f64       (Float)
                ├── i32       (Bool: 0/1)
                └── ref null  (Option[T])
```

**No linear memory. No allocator. No ref counting.** The WASM runtime (browser/wasmtime) handles GC.

### Almide Type → wasm-gc Mapping

| Almide | wasm-gc | Notes |
|--------|---------|-------|
| Int | `i64` | value type, stack/local |
| Float | `f64` | value type |
| Bool | `i32` | 0/1 |
| Unit | (none) | omitted |
| String | `(ref $str)` — `(array (mut i8))` | UTF-8 bytes |
| List[T] | `(ref $list_T)` — `(array (mut (ref $T)))` | GC array |
| Record | `(ref $RecordName)` — `(struct ...)` | GC struct |
| Variant | `(ref $VariantName)` — tagged union via subtyping | GC struct hierarchy |
| Option[T] | `(ref null $T)` | null = none, non-null = some |
| fn(A)->B | `(ref $closure)` — `(struct (ref $func) (ref $env))` | funcref + env |

### Integration with Codegen v3

```rust
// src/codegen/target.rs
Target::WasmGc => Pipeline::new()
    .add(TypeConcretizationPass)  // resolve generics
    .add(StdlibLoweringPass)      // resolve module calls
    .add(FanLoweringPass),        // fan → sequential
    // No CloneInsertion (GC handles it)
    // No BorrowInsertion (no ownership model)
    // No BoxDeref (no Box, GC handles recursion)
```

## Implementation Phases

### Phase 1: Minimal IR → wasm-gc (next)

IR nodes to support:
- `LitInt`, `LitFloat`, `LitBool` → i64/f64/i32 constants
- `BinOp` → WASM arithmetic/comparison
- `If { cond, then, else }` → `if` instruction
- `Block { stmts, expr }` → instruction sequence
- `Bind { var, value }` → local.set
- `Var { id }` → local.get
- `Call { target: Named, args }` → call
- `IrFunction` → WASM function

Goal: FizzBuzz compiles to wasm-gc. Target: <500 bytes.

### Phase 2: Strings + Collections

- String as `(array (mut i8))`
- List[T] as GC arrays
- Record as GC structs
- for...in → loop over GC array

### Phase 3: Variant + Pattern Matching

- Variant → struct hierarchy with tag
- match → tag check + br_on_cast
- Option[T] → nullable ref
- Result[T, E] → tagged struct

### Phase 4: Closures + stdlib

- Lambda → closure struct (funcref + env)
- Port core stdlib to wasm-gc or host imports

## Size Targets

| Program | Target | MoonBit (ref.) |
|---------|--------|----------------|
| return 42 | ~30 bytes | ~30 bytes |
| FizzBuzz | <500 bytes | ~500 bytes |
| Quicksort | <5KB | ~5KB |
| Real app | <50KB | 30-100KB |

## References

- [wasm-gc proposal](https://github.com/nickmain/nickmain.github.io/wiki/WebAssembly-GC-Proposal)
- [MoonBit](https://www.moonbitlang.com/)
- [wasm-encoder](https://docs.rs/wasm-encoder/)
- [V8 wasm-gc blog](https://v8.dev/blog/wasm-gc)
