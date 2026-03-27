<!-- description: Redesign WASM function local allocation and scratch layout -->
<!-- done: 2026-03-23 -->
# WASM Local Allocation Redesign

## Status: Design Phase

## Current Problems

### Architecture

```
WASM function local layout:
[params...][bind locals...][i64 scratch × N][i32 scratch × N]
                            ^match_i64_base  ^match_i32_base

N = count_scratch_depth(body) — statically scans IR to estimate maximum scratch usage
```

Stdlib function implementations use contiguous scratch locals starting from `match_i32_base + match_depth`:

```rust
let s = self.match_i32_base + self.match_depth;
// s+0: list_ptr, s+1: idx, s+2: result, ...
wasm!(self.func, { local_get(s); ... local_get(s + 3); ... });
```

Nested calls advance the offset with `match_depth += N` to avoid collisions.

### Problems

1. **count_scratch_depth doesn't know function names at IR stage**
   - Returns a flat 4 for every `IrExprKind::Call`
   - In practice, list.unique_by uses s+0 through s+5 (6 slots), list.sort_by uses s+0 through s+7 (7 slots)
   - 4 overflows, but 8 inflates total locals for all functions and breaks other things

2. **Two separate systems needed for i32/i64**
   - f64 values can't be stored in i64 locals (type mismatch)
   - Currently float.round evacuates f64 to mem[0..8] (ugly)

3. **match_depth management is scattered**
   - 30+ places in stdlib functions do `match_depth += N`, a source of omission and double-counting bugs
   - OptionSome/ResultOk also require `match_depth += 1`, easy to overlook

4. **Mixed use with mem[] scratch**
   - Some functions also store temporary values in mem[0], mem[4], mem[8]
   - Local scratch and mem[] scratch are mixed with no consistency
   - Source of bugs where nested calls overwrite mem[]

## Ideal: Scratch Allocator

### Design Approach

**Introduce an allocator that manages scratch locals as "named slots". Dynamically allocate during emit as needed, release when no longer needed. After function compilation, generate local declarations from the maximum simultaneous usage count.**

### Architecture

```rust
struct ScratchAllocator {
    // 型ごとの使用中スロット
    i32_slots: Vec<bool>,  // true = in use
    i64_slots: Vec<bool>,
    f64_slots: Vec<bool>,

    // 確保された最大数（local宣言用）
    i32_max: u32,
    i64_max: u32,
    f64_max: u32,

    // base indices（関数コンパイル後に確定）
    i32_base: u32,
    i64_base: u32,
    f64_base: u32,
}

impl ScratchAllocator {
    /// Allocate a slot. Reuses an unused slot if available, otherwise adds a new one
    fn alloc_i32(&mut self) -> u32 { ... }
    fn alloc_i64(&mut self) -> u32 { ... }
    fn alloc_f64(&mut self) -> u32 { ... }

    /// Free a slot (makes it reusable)
    fn free_i32(&mut self, idx: u32) { ... }
    fn free_i64(&mut self, idx: u32) { ... }
    fn free_f64(&mut self, idx: u32) { ... }

    /// RAII guard: automatically freed at scope exit
    fn scoped_i32(&mut self) -> ScratchGuard<'_> { ... }
}
```

### Usage Example

```rust
// Before (current)
let s = self.match_i32_base + self.match_depth;
wasm!(self.func, { local_get(s); ... local_get(s + 3); });

// After (ideal)
let list_ptr = self.scratch.alloc_i32();
let idx = self.scratch.alloc_i32();
let result = self.scratch.alloc_i32();
wasm!(self.func, { local_get(list_ptr); ... local_get(result); });
self.scratch.free_i32(list_ptr);
self.scratch.free_i32(idx);
self.scratch.free_i32(result);
```

### Benefits

1. **count_scratch_depth becomes unnecessary** — the allocator automatically tracks maximum simultaneous usage
2. **Independent per type** — i32/i64/f64 can be safely mixed
3. **No nesting management** — manual match_depth management eliminated
4. **Eliminates mem[] scratch** — everything unified under local scratch
5. **Bug detection** — can detect forgotten frees and double frees

### Migration Strategy

Full migration is too large, so incremental:

#### Phase 0: Migrate to Two-Pass Approach (Prerequisite)

Current: count_scratch_depth (IR scan) → local declarations → emit (1 pass)

Ideal: emit (1 pass) → allocator records maximum count → local declarations added retroactively

WASM binary format requires local declarations at the function start. Two approaches:
- **A. Placeholder + Patch**: emit with provisional local declarations → rewrite binary with actual usage count
- **B. Two-pass compilation**: first pass collects scratch usage count, second pass does actual emit
- **C. wasm-encoder retroactive locals**: investigate if locals can be added after `Function::new()`

In practice, wasm-encoder's `Function::new(locals)` must be called first. **Option B** is realistic: first pass is a dry-run recording only scratch allocation, second pass does actual emit. However, compile time doubles.

**Compromise**: improve count_scratch_depth to return a "per-function-name usage table". Since function names can be extracted from IR Call nodes, table lookup yields accurate counts.

#### Phase 1: Function-Name-Based count_scratch_depth

```rust
fn stdlib_scratch_depth(module: &str, func: &str) -> u32 {
    match (module, func) {
        ("list", "unique_by") => 6,
        ("list", "sort_by") => 7,
        ("list", "sort") => 5,
        ("list", "map") => 5,
        ("list", "fold") => 3,
        ("list", "filter") => 2,
        _ => 4,  // default
    }
}
```

Extract function names from CallTarget::Module and CallTarget::Method, and return accurate depth via table lookup.

**Effort**: ~50 lines of changes. Just fix the Call case in count_scratch_depth.
**Effect**: Eliminates local out of bounds errors.

#### Phase 2: Eliminate mem[] scratch

Replace all temporary value storage in mem[0], mem[4], mem[8] with local scratch.

Target: places using `i32_const(0); ... i32_store(0)` patterns in calls_list_closure.rs, calls_list_closure2.rs, calls_map.rs, calls_option.rs, etc.

**Effort**: rewrite each function individually. ~20 functions, 10-30 lines of changes each.
**Effect**: Eliminates mem[] overwrite bugs in nested calls.

#### Phase 3: Introduce ScratchAllocator

Add ScratchAllocator to FuncCompiler. Incrementally replace the existing `match_i32_base + match_depth` pattern.

**Effort**: allocator core ~100 lines + stdlib function rewrites ~500 lines.
**Effect**: Complete elimination of match_depth management.

#### Phase 4: Add f64 scratch

Add f64 slots to ScratchAllocator. Replace mem[] f64 evacuation used in float.round, etc.

**Effort**: ~30 lines.
**Effect**: Eliminates mem[] dependency for float operations.

## Recommendation: Execute Phase 1 Immediately

Phase 1 (function-name-based count_scratch_depth) has minimum effort for maximum impact. Immediately resolves the remaining 2 local out of bounds errors.

Phase 2 and beyond are medium-to-long-term improvements, to be started as needed.

## Related Files

| File | Role |
|------|------|
| `src/codegen/emit_wasm/mod.rs:310-320` | FuncCompiler struct (match_i32_base, match_depth) |
| `src/codegen/emit_wasm/functions.rs:42-50` | Local allocation during function compilation |
| `src/codegen/emit_wasm/closures.rs:200-215` | Local allocation during lambda compilation |
| `src/codegen/emit_wasm/statements.rs:240-280` | count_scratch_depth |
| `src/codegen/emit_wasm/calls_list_closure*.rs` | Stdlib functions (scratch consumers) |
