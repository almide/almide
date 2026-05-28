# Almide WASM Engine — Complete Redesign

> **Status**: Design phase
> **Motivation**: String layout migration (4→8 byte header) broke 35+ sites silently. Map Swiss Table migration broke all map closure ops. Root cause: raw WASM instruction emission with no type/layout abstraction.
> **Goal**: Replace the 40-file hand-written assembler with a type-aware, layout-safe WASM compilation engine.

## Problem Statement

Current `emit_wasm/` is an assembler: AlmideIR → raw `wasm-encoder` instructions. Every memory access manually computes offsets:

```rust
// "load string data at index i" — 6 instructions, 3 magic numbers
wasm!(self.func, {
    local_get(str_ptr); i32_const(8); i32_add;
    local_get(i); i32_add; i32_load8_u(0);
});
```

**Consequences**:
- Layout change → grep 40 files → miss sites → silent memory corruption
- Swiss Table map iteration is 30+ lines of boilerplate per method
- No way to verify layout consistency at compile time
- Code is unreadable — intent buried in instruction soup

## Architecture

```
AlmideIR (after nanopass)
  ↓
┌─────────────────────────────────┐
│  Almide WASM Engine             │
│                                 │
│  LayoutRegistry                 │  ← single source of truth for memory layouts
│  WasmIR (typed mid-level IR)    │  ← stack-machine IR with typed memory ops
│  Lowering (AlmideIR → WasmIR)  │  ← semantic translation
│  Optimize (WasmIR → WasmIR)    │  ← RC fusion, dead field elim, slot coloring
│  Emit (WasmIR → wasm-encoder)  │  ← mechanical translation
│                                 │
└─────────────────────────────────┘
  ↓
WASM binary
```

## Core Components

### 1. LayoutRegistry

All memory layouts defined declaratively. **No hardcoded offsets anywhere else.**

```rust
pub struct LayoutRegistry {
    layouts: Vec<MemLayout>,
}

pub struct MemLayout {
    pub name: &'static str,
    pub fields: Vec<MemField>,
}

pub struct MemField {
    pub name: &'static str,
    pub offset: FieldOffset,  // Fixed(n) or Dynamic(expr)
    pub ty: MemType,
}

pub enum FieldOffset {
    Fixed(u32),
    /// Offset depends on runtime value (e.g., map entries after tag array)
    AfterField { field: &'static str },
}

pub enum MemType {
    I32, I64, F32, F64, U8,
    ByteArray,          // [u8; field.len]
    Array(Box<MemType>), // [T; field.len]
}
```

Definitions:

```rust
registry.define("String", fields![
    len  : I32 @ 0,
    cap  : I32 @ 4,
    data : ByteArray @ 8,
]);

registry.define("List", fields![
    len  : I32 @ 0,
    cap  : I32 @ 4,
    data : Array(elem_type) @ 8,
]);

registry.define("SwissMap", fields![
    len     : I32 @ 0,
    cap     : I32 @ 4,
    tags    : ByteArray @ 8,          // size = cap
    entries : Array(entry_type) @ after("tags"),  // offset = 8 + cap
]);
```

### 2. WasmIR

Stack-machine IR that preserves Almide type information. Matches WASM's execution model but with typed operations.

```rust
pub enum WasmOp {
    // ── Typed memory access ──
    FieldLoad { base: Local, layout: LayoutId, field: FieldId },
    FieldStore { base: Local, layout: LayoutId, field: FieldId },
    ElemLoad { base: Local, layout: LayoutId, field: FieldId, index: Local, elem_ty: WasmType },
    ElemStore { base: Local, layout: LayoutId, field: FieldId, index: Local, elem_ty: WasmType },

    // ── Collection iteration ──
    ListForEach { list: Local, elem_ty: WasmType, body: Vec<WasmOp> },
    MapForEach { map: Local, key_ty: WasmType, val_ty: WasmType, body: Vec<WasmOp> },

    // ── Allocation ──
    Alloc { layout: LayoutId, size_expr: SizeExpr },
    AllocList { elem_ty: WasmType, len: Local },
    AllocMap { key_ty: WasmType, val_ty: WasmType, cap: u32 },

    // ── Reference counting ──
    RcInc { ptr: Local },
    RcDec { ptr: Local, layout: LayoutId },
    CowCheck { ptr: Local, clone_body: Vec<WasmOp> },

    // ── Control flow (WASM-native structured) ──
    Block { body: Vec<WasmOp> },
    Loop { body: Vec<WasmOp> },
    If { then: Vec<WasmOp>, else_: Vec<WasmOp> },
    Br(u32),
    BrIf(u32),
    Return,

    // ── Stack operations ──
    LocalGet(Local),
    LocalSet(Local),
    Const(WasmConst),
    BinOp(WasmBinOp),
    Call(FuncId),
    CallIndirect { sig: SigId, table_idx: Local },

    // ── String operations (first-class) ──
    StringConcat { left: Local, right: Local },
    StringInterp { parts: Vec<StringPart> },

    // ── Comparison ──
    DeepEq { ty: AlmideTy, left: Local, right: Local },
}
```

### 3. Lowering (AlmideIR → WasmIR)

Each AlmideIR node maps to WasmIR ops. This is where semantic decisions happen.

```rust
fn lower_expr(expr: &IrExpr, ctx: &mut LowerCtx) -> Vec<WasmOp> {
    match &expr.kind {
        IrExprKind::StringInterp { parts } => {
            vec![WasmOp::StringInterp {
                parts: parts.iter().map(|p| lower_string_part(p, ctx)).collect(),
            }]
        }
        IrExprKind::ForIn { var, iterable, body } if is_map(&iterable.ty) => {
            vec![WasmOp::MapForEach {
                map: ctx.lower_to_local(iterable),
                key_ty: map_key_wasm_ty(&iterable.ty),
                val_ty: map_val_wasm_ty(&iterable.ty),
                body: lower_stmts(body, ctx),
            }]
        }
        // ...
    }
}
```

### 4. Emit (WasmIR → wasm-encoder)

Mechanical translation. **This is the only place that knows raw WASM instructions.**

```rust
fn emit_op(op: &WasmOp, f: &mut Function, reg: &LayoutRegistry) {
    match op {
        WasmOp::FieldLoad { base, layout, field } => {
            let offset = reg.resolve_offset(*layout, *field);
            f.instruction(&LocalGet(*base));
            f.instruction(&I32Load(mem_arg(offset)));
        }
        WasmOp::MapForEach { map, key_ty, val_ty, body } => {
            let layout = reg.get("SwissMap");
            // Emit Swiss Table iteration — ONCE, CORRECTLY
            // cap, entry_base, tag check, skip empty — all here
            emit_swiss_table_iter(f, reg, *map, *key_ty, *val_ty, body);
        }
        WasmOp::StringInterp { parts } => {
            emit_string_interp(f, reg, parts);  // uses LayoutRegistry for offsets
        }
        // ...
    }
}
```

### 5. Optimization Passes (WasmIR → WasmIR)

Run between lowering and emission:

| Pass | Effect | LLVM equivalent |
|------|--------|-----------------|
| **RC Fusion** | Cancel adjacent inc+dec | None (LLVM doesn't know RC) |
| **Dead Field Elim** | Skip unused record fields in alloc | Partial (SROA) |
| **Iterator Fusion** | Merge chained MapForEach/ListForEach | None |
| **Const Folding** | Evaluate constant FieldLoad at compile time | Yes, but layout-unaware |
| **Local Coloring** | Reuse WASM locals across non-overlapping lifetimes | Register allocation |
| **Inline Expansion** | Inline small WasmIR function bodies | Yes |
| **Peephole** | Pattern-match and simplify instruction sequences | Yes |

## What Dies

All of current `emit_wasm/`:

| File | Replacement |
|------|------------|
| `list_layout.rs` | `LayoutRegistry` definitions |
| `calls_string.rs` | `lower_string_call()` + StringInterp/StringConcat ops |
| `calls_map.rs` | `lower_map_call()` + MapForEach/FieldLoad ops |
| `calls_map_closure.rs` | same — 500 lines of boilerplate → ~50 lines of lowering |
| `calls_list*.rs` | `lower_list_call()` + ListForEach ops |
| `runtime.rs` | Emit phase: runtime fns emitted from WasmIR |
| `rt_string*.rs` | same |
| `expressions.rs` | `lower_expr()` |
| `statements.rs` | `lower_stmt()` |
| `control.rs` | Lowering for loops/match + Block/Loop/If WasmIR ops |
| `equality.rs` | `DeepEq` WasmIR op |
| `scratch.rs` | Local allocator moves into emit phase |

## Implementation Plan

### Phase 1: Foundation (LayoutRegistry + WasmIR types)
- [ ] `LayoutRegistry` with String, List, Map, Set, Variant, Record, Tuple, Option, Result
- [ ] `WasmIR` enum with core ops
- [ ] `emit_op()` for each WasmIR variant (the only wasm-encoder touchpoint)
- [ ] Proof of concept: compile `"hello ${name}" |> println` through new pipeline

### Phase 2: Lowering (AlmideIR → WasmIR)
- [ ] `lower_expr()` for all IrExprKind variants
- [ ] `lower_stmt()` for all IrStmtKind variants
- [ ] `lower_function()` including local allocation and control flow
- [ ] Gate: all `spec/lang/` tests pass through new pipeline

### Phase 3: Stdlib & Runtime
- [ ] String runtime (concat, interp, slice, trim, eq, cmp, etc.)
- [ ] List runtime (push, map, filter, fold, sort, etc.)
- [ ] Map runtime (get, set, delete, keys, values, fold, etc.)
- [ ] Other stdlib modules
- [ ] Gate: all `spec/stdlib/` tests pass

### Phase 4: Optimization Passes
- [ ] RC Fusion
- [ ] Local Coloring
- [ ] Peephole
- [ ] Gate: all benchmarks match or beat current emit_wasm performance

### Phase 5: Delete Old Code
- [ ] Remove all `emit_wasm/*.rs` files
- [ ] Single entry point: `wasm_engine::compile(ir_program) -> Vec<u8>`

## Findings from v0.23.11 Patch Session (2026-05-27)

Session fixed 18 test files (175→193/240) by patching emit_wasm directly. Key findings that inform the redesign:

### 1. Stack Discipline is the Hardest Problem

**The blocking CI bug**: Perceus/ANF inserts a trailing `Ret(var_ref)` in void functions. This becomes a Block tail expression that pushes a value onto the WASM stack. The IR type annotation says Unit (Perceus doesn't update it), but codegen actually emits `local.get` which pushes i32. Result: "values remaining on stack at end of block" — rejected by V8 and newer wasmtime.

**Why WasmIR fixes this**: The emit phase can statically verify stack balance per block/function. Each `Op` has a known stack effect (+1, -1, 0). A void function whose ops sum to non-zero gets an automatic `Op::Drop`. No IR type annotation trust needed.

### 2. Layout Offset Bugs are Systemic

String layout change (4→8 byte header) broke 3 sites. Map Swiss Table migration broke 35+ sites. HTTP module had 15+ hardcoded offsets. All fixed by replacing `i32_const(4)` with `list_layout::DATA_OFFSET` — but this is still manual and fragile.

**Why WasmIR fixes this**: `Op::FieldLoad { layout: STRING, field: DATA }` resolves offset via `LayoutRegistry`. Zero hardcoded offsets in lowering.

### 3. rc_dec on Data Section Pointers Corrupts Memory

`rc_dec(ptr)` reads `ptr-4` as refcount. Interned strings in the data section have no alloc header → memory corruption. Fixed by adding `ptr < heap_start` guard to rc_dec/rc_inc/cow_check.

**Why WasmIR fixes this**: `Op::RcDec { ptr, layout }` in the emit phase can check pointer origin. Or better: the lowering phase never emits RcDec for compile-time-known interned values.

### 4. wasmparser Validation+Stub Hides Bugs

The old emit_wasm validated with wasmparser 0.225 and replaced broken functions with `unreachable` stubs. This silently degraded correctness. Worse: wasmparser 0.225 had false positives on valid WASM that wasm-tools 0.244 accepts. Removed entirely — codegen must produce valid WASM.

**Why WasmIR fixes this**: Stack balance is verified structurally at the IR level before emission. If WasmIR is well-formed, the emitted WASM is guaranteed valid.

### 5. Swiss Table Iteration is a Pattern, Not a Primitive

Every map closure method (fold/each/any/all/count/find/update/map) reimplemented the Swiss Table iteration loop. Extracted `MapIter` helpers — but still 8 call sites.

**Why WasmIR fixes this**: `Op::MapForEach { map, entry_stride, body }` is a single IR node. The emit phase generates the Swiss Table loop once.

### 6. Perceus/ANF Transforms IR Types Unreliably

Perceus changes block structure (inserts RcDec statements, Ret tail expressions) but doesn't always update `.ty` annotations. The WASM emitter can't trust `expr.ty` for stack management decisions.

**Why WasmIR fixes this**: WasmIR doesn't carry type annotations — it carries stack effects. Each Op explicitly declares what it pushes/pops. No "trust the annotation" problem.

### Current Blockers (require WasmEngine to fix properly)

| Bug | Symptom | Root Cause |
|-----|---------|-----------|
| `filter \|> map` pipe | WASM stack mismatch in void function | Perceus tail expr in void block |
| `mut` param | list.push via mut param traps | Pass-by-reference not implemented in WASM |
| `bytes.from_list` + assert | Heap corruption in test function | Perceus drop ordering |
| `group_by` + assert | Same as above | Same |
| `opaque type` | ConcretizeTypes fails on Member | Upstream type checker issue |
| `intra-module fn ref` | Wrong function table index | Closure conversion for module fns |

## Metrics

- **Correctness**: `almide test spec/ --target wasm` — 240/240 (currently 193/240)
- **Binary size**: ≤ current (336B hello world baseline)
- **Performance**: ≥ current (5 wins vs Rust+LLVM baseline)
- **Code volume**: emit_wasm/ LOC reduced by ≥50%
- **Layout bugs**: structurally impossible (enforced by LayoutRegistry)
- **Stack bugs**: structurally impossible (verified at WasmIR level)
