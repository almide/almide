<!-- description: Rewrite stdlib in Almide with a 3-layer architecture -->
# Stdlib in Almide: Unified Library Architecture

**Goal**: Rewrite stdlib in Almide and make it use the same mechanism as userlib. All libraries operate on a 3-layer architecture.
**Current**: 381 functions × 2 targets (Rust/TS) maintained by hand. Cost of adding new targets is extreme.
**Benefit**: When adding a new target, only 20-30 primitives need to be written. The distinction between stdlib and userlib disappears.

---

## Why

### Current Problems

```
stdlib/defs/list.toml     → Function signature definitions
runtime/rust/list.rs      → Rust implementation (hand-written)
runtime/ts/list.ts        → TS implementation (hand-written)
```

- Hand-maintaining 381 functions × N targets
- Risk of subtle divergence between Rust and TS implementations
- Adding a new target (Go, Python, etc.) requires rewriting all 381 functions
- stdlib and userlib run on completely different mechanisms

### Desired State

```
stdlib/list.almd          → Implemented in Almide (single source)
  ↓ compile
Included in IR (on equal footing with user code)
  ↓ codegen
Target-specific code is auto-generated
```

## 3-Layer Architecture

### Layer 1: Primitives (per-target, required)

The minimal operations that cannot be written in Almide. 20-30 per target.

```
println, eprintln          — output
math.sin, math.cos, ...   — math functions (hardware instructions)
list.alloc, list.get_raw   — memory operations
string.len_bytes           — byte length
fs.read_raw, fs.write_raw  — file I/O
random.int_raw             — random number generation
```

codegen provides these per target:
- Rust: `println!()`, `f64::sin()`, `Vec::new()`, ...
- TS: `console.log()`, `Math.sin()`, `[]`, ...
- WASM: `fd_write`, WASI imports, linear memory ops, ...

### Layer 2: Almide Implementation (shared, default)

The majority of stdlib. Written in Almide, automatically works across all targets.

```almide
fn map(xs: List[A], f: fn(A) -> B) -> List[B] = {
  var result: List[B] = []
  for x in xs {
    result = result + [f(x)]
  }
  result
}

fn filter(xs: List[A], f: fn(A) -> Bool) -> List[A] = {
  var result: List[A] = []
  for x in xs {
    if f(x) then { result = result + [x] } else ()
  }
  result
}

fn join(xs: List[String], sep: String) -> String = {
  var result = ""
  for (i, x) in list.enumerate(xs) {
    if i > 0 then { result = result + sep } else ()
    result = result + x
  }
  result
}
```

### Layer 3: Target Optimizations (optional, overrides)

Only performance-critical functions are overridden with target-native implementations.

```almide
// Default implementation (Layer 2)
fn sort(xs: List[Int]) -> List[Int] = {
  // merge sort, etc.
}

// Target optimization (Layer 3)
@native(rust, "vec_sort")     // Rust: Vec::sort() (pdqsort)
@native(ts, "array_sort")     // TS: Array.sort() (TimSort)
// WASM: no override → Layer 2 Almide implementation
```

## stdlib = userlib

This mechanism is not stdlib-specific. userlib works exactly the same way.

```almide
// User-written library
fn hash(data: String) -> String = {
  // Fallback implementation in Almide
}

@native(rust, "ring_digest")   // Rust: ring crate
@native(ts, "node_crypto")     // TS: Node crypto
// WASM: Almide implementation
```

**stdlib is simply "userlib that comes pre-installed."**

## Impact on IR

### Current

```json
{
  "kind": "call",
  "target": { "kind": "module", "module": "list", "func": "sort" }
}
```

stdlib calls are unresolved references. codegen injects the runtime.

### After Migration

```json
{
  "functions": [
    { "name": "list.sort", "body": { "..." }, "native_override": { "rust": "vec_sort", "ts": "array_sort" } },
    { "name": "list.map", "body": { "..." } },
    { "name": "main", "body": { "..." } }
  ],
  "primitives": ["println", "math.sin", "list.alloc"]
}
```

- stdlib functions appear in `functions` on equal footing with user code
- `native_override` swaps in target-specific implementations when present
- Only `primitives` are codegen's responsibility

## Relationship to codegen Separation

This design minimizes what an external codegen tool needs:

```
IR (JSON)                    → All function implementations (including stdlib)
+ primitives (20-30/target)  → Minimal target-specific implementations
= Complete output
```

When adding a new target:
- **Now**: Write all 381 functions
- **After migration**: Only primitives 20-30 + @native for a few dozen performance-critical functions

## Phases

### Phase 1: @native Mechanism

- [ ] Parser/checker support for `@native(target, impl)` attribute
- [ ] Add `native_override` field to IR
- [ ] codegen selects native implementation when override exists

### Phase 2: Primitive Definitions

- [ ] Identify Layer 1 primitives (target: 30 or fewer per target)
- [ ] Explicitly treat primitives as `primitives` in IR
- [ ] Primitive implementation in codegen (Rust / TS / WASM)

### Phase 3: stdlib Migration (incremental)

Rewrite in Almide by priority:

| Priority | Module | Functions | Reason |
|----------|--------|-----------|--------|
| 1 | option | 12 | Pure logic, no primitives needed |
| 2 | result | 15 | Same as above |
| 3 | list | 45 | map/filter/fold etc. can be written in Almide. sort uses @native |
| 4 | string | 35 | Many reduce to list operations. len/chars are primitives |
| 5 | map | 20 | Internal data structure design needed |
| 6 | set | 20 | Depends on map |
| 7 | math | 25 | Almost all primitives (sin, cos, ...) |
| 8 | int, float | 20 | parse is a primitive, arithmetic is built into the language |
| 9 | json | 23 | Write the parser in Almide |
| 10 | io, fs | 15 | Almost all primitives |
| 11 | http | 20 | Large differences across targets |
| 12 | Others | ~150 | datetime, regex, crypto, ... |

### Phase 4: userlib Integration

- [ ] Confirm `@native` works in user-defined modules
- [ ] `almide pack --target ts` generates npm package structure
- [ ] `almide pack --target rust` generates crate structure

## Success Criteria

- 80% or more of stdlib is written in Almide
- When adding a new target, all stdlib works with only primitives + @native
- IR is self-contained (no external dependencies beyond primitives)
- userlib operates on the same mechanism as stdlib
- `almide pack` outputs packages for target ecosystems

## Lessons from Other Languages

| Language | stdlib Strategy | What Almide adopts |
|----------|----------------|-------------------|
| Gleam | Self-language + @external FFI | Layer 1-2 separation, @external declarative FFI |
| Kotlin | expect/actual | Layer 3 @native overrides |
| ReScript | Minimal stdlib + external | Minimal runtime philosophy |
| Haxe | Written in self-language | All targets from single source |
