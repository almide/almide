<!-- description: Complete redesign of the hand-written WASM emitter into a type-aware, layout-safe, stack-verified compiler (WASM 3.0/Component Model ready) -->
# Almide WASM Engine — Complete Redesign

> **Status**: Design phase — foundation (LayoutRegistry + WasmIR + WasmBuilder) exists, full recreate planned
> **Motivation**: Hand-written WASM emission is the largest correctness gap. Also: WASM 3.0, WASI Preview 2, and Component Model all require a proper compilation pipeline, not raw instruction assembly.
> **Goal**: Replace the 40-file hand-written assembler with a type-aware, layout-safe, stack-verified WASM compiler that outputs core modules or Component Model components.

## Problem Statement

Current `emit_wasm/` is an assembler: AlmideIR → raw `wasm-encoder` instructions. Every memory access manually computes offsets. Stack balance is verified by humans.

**Consequences**:
- Layout change → grep 40 files → miss sites → silent memory corruption
- No static stack-effect verification → stack bugs at runtime
- Swiss Table map iteration is 30+ lines of boilerplate per method
- WASI Preview 1 raw imports can't interop with Component Model ecosystem
- No WIT generation → can't participate in WASM component ecosystem

## Architecture

```
AlmideIR (after nanopass pipeline)
  ↓
┌──────────────────────────────────────────────────┐
│  Almide WASM Engine v2                           │
│                                                  │
│  LayoutRegistry          ← memory layout SoT     │
│  Lowering (AlmideIR → WasmIR)                    │
│  Stack-Effect Verifier   ← static balance proof   │
│  Optimize (RC fusion, local coloring, peephole)   │
│  Core Emit (WasmIR → wasm-encoder)               │
│  Component Wrap (core → component + WIT)          │
│  WASI Adapter (canonical ABI for system calls)    │
│                                                  │
└──────────────────────────────────────────────────┘
  ↓                        ↓
Core WASM module     Component (.wasm + WIT)
```

### Key Design Decisions

1. **Internal layout stays Almide-native** (LayoutRegistry). No canonical ABI overhead for internal data. Canonical ABI is only used at component boundaries (imports/exports).

2. **Stack effects are structural, not annotated**. Each WasmIR Op declares `(pops: u8, pushes: u8)`. The verifier computes net stack effect per block/function and rejects mismatches before emission.

3. **Component Model is a wrapper, not a rewrite**. The core module is compiled normally. A separate Component Wrap phase lifts it to a component by generating:
   - WIT declarations from Almide's `pub fn` / `pub type` exports
   - Canonical ABI adapter functions (Almide layout ↔ canonical layout)
   - WASI Preview 2 imports via `wit-component`

4. **WASI Preview 2 replaces Preview 1**. Current raw imports (`fd_write`, `clock_time_get`, etc.) are replaced with Component Model imports (`wasi:filesystem/types`, `wasi:http/outgoing-handler`, etc.).

## Core Components

### 1. LayoutRegistry (exists)

All memory layouts defined declaratively. No hardcoded offsets anywhere else.

Built-in layouts: String, List, SwissMap, Set, AllocHeader, ClosurePair, Variant, Option, Result.

### 2. WasmIR with Stack Effects

```rust
pub struct WasmOp {
    pub kind: WasmOpKind,
    pub pops: u8,    // values consumed from stack
    pub pushes: u8,  // values produced onto stack
}

pub enum WasmOpKind {
    // ── Typed memory access (layout-safe) ──
    FieldLoad { layout: LayoutId, field: FieldId },   // pops: 1 (base), pushes: 1
    FieldStore { layout: LayoutId, field: FieldId },   // pops: 2 (base, val), pushes: 0
    FieldAddr { layout: LayoutId, field: FieldId },    // pops: 1 (base), pushes: 1 (addr)

    // ── Collection iteration (single IR node) ──
    ListForEach { body: Vec<WasmOp> },     // pops: 1 (list), pushes: 0
    MapForEach { body: Vec<WasmOp> },      // pops: 1 (map), pushes: 0

    // ── Allocation ──
    Alloc { layout: LayoutId, size: SizeExpr },
    AllocCollection { layout: LayoutId, len_local: Local, stride: u32 },

    // ── Reference counting ──
    RcInc,                                  // pops: 1 (ptr), pushes: 0
    RcDec { layout: LayoutId },            // pops: 1 (ptr), pushes: 0
    CowCheck { clone_body: Vec<WasmOp> },  // pops: 1 (ptr), pushes: 1 (ptr or clone)

    // ── Structured control flow ──
    Block { body: Vec<WasmOp>, result_ty: Option<ValType> },
    Loop { body: Vec<WasmOp> },
    If { then: Vec<WasmOp>, else_: Vec<WasmOp>, result_ty: Option<ValType> },
    Br(u32),
    BrIf(u32),
    Return,
    ReturnCall(FuncId),          // WASM 3.0 tail call

    // ── Stack primitives ──
    LocalGet(Local),
    LocalSet(Local),
    LocalTee(Local),
    Const(WasmConst),
    Drop,
    BinOp(WasmBinOp),
    Call(FuncId),
    CallIndirect { sig: SigId },

    // ── High-level ops (lowered to primitives in emit) ──
    StringConcat,                           // pops: 2, pushes: 1
    StringInterp { part_count: u32 },       // pops: N, pushes: 1
    DeepEq { ty: AlmideTy },               // pops: 2, pushes: 1 (i32 bool)

    // ── Component Model canonical ABI ──
    CanonLift { wit_func: WitFuncId },      // core → component boundary
    CanonLower { wit_func: WitFuncId },     // component → core boundary
    CanonStringToMemory,                    // canonical string → Almide string
    CanonStringFromMemory,                  // Almide string → canonical string
    CanonListToMemory { elem_ty: ValType }, // canonical list → Almide list
    CanonListFromMemory { elem_ty: ValType },

    // ── Memory ──
    MemoryCopy { dst_mem: u32, src_mem: u32 },
    MemoryGrow(u32),
    MemorySize(u32),

    Seq(Vec<WasmOp>),
    Unreachable,
}
```

### 3. Stack-Effect Verifier

```rust
fn verify_stack_balance(ops: &[WasmOp], expected_result: Option<ValType>) -> Result<(), StackError> {
    let mut depth: i32 = 0;
    for op in ops {
        depth -= op.pops as i32;
        if depth < 0 { return Err(StackError::Underflow { op, depth }); }
        depth += op.pushes as i32;
    }
    let expected = if expected_result.is_some() { 1 } else { 0 };
    if depth != expected {
        return Err(StackError::Imbalance { expected, actual: depth });
    }
    Ok(())
}
```

Runs recursively on every Block/If/Loop body. Nested scopes are verified independently.

### 4. Lowering (AlmideIR → WasmIR)

Semantic translation. Each AlmideIR node maps to WasmIR ops.

### 5. Core Emit (WasmIR → wasm-encoder)

Mechanical translation. The **only** place that touches raw WASM instructions. Verified-correct WasmIR goes in, valid WASM bytes come out.

### 6. Component Wrap

Generates a Component Model component from the core module:

```
Core WASM module
  + WIT declarations (from pub fn/type exports)
  + Canonical ABI adapter functions
  + WASI Preview 2 import declarations
  = Component (.wasm)
```

Uses `wit-component` crate for the heavy lifting. Almide generates:
- **WIT from types**: `pub type User = { name: String, age: Int }` → `record user { name: string, age: s64 }`
- **WIT from functions**: `pub fn greet(name: String) -> String` → `greet: func(name: string) -> string`
- **Canonical ABI adapters**: Convert between Almide's internal layout and canonical ABI at export boundaries

### 7. WASI Preview 2 Adapter

Replaces raw WASI Preview 1 imports with Component Model imports:

| Current (Preview 1) | New (Preview 2 via CM) |
|---------------------|----------------------|
| `fd_write(fd, iovs, ...)` | `wasi:cli/stdout.get-stdout` + `output-stream.write` |
| `fd_read(fd, iovs, ...)` | `wasi:cli/stdin.get-stdin` + `input-stream.read` |
| `path_open(...)` | `wasi:filesystem/types.open-at` |
| `clock_time_get(...)` | `wasi:clocks/monotonic-clock.now` |
| (none) | `wasi:http/outgoing-handler.handle` |

## Implementation Phases

### Phase 0: Stack-Effect Verifier

Add `(pops, pushes)` to existing WasmIR Op enum. Implement verifier. Run on WasmBuilder output. This alone closes the correctness gap for the engine layer.

- [ ] Add stack-effect fields to WasmIR Op
- [ ] Implement `verify_stack_balance()` recursive checker
- [ ] Wire into WasmBuilder — verify after each function is built
- [ ] Gate: zero stack-effect violations on `almide test spec/ --target wasm`

### Phase 1: New Lowering

Replace hand-written emission with AlmideIR → WasmIR lowering.

- [ ] `lower_expr()` for all IrExprKind variants
- [ ] `lower_stmt()` for all IrStmtKind variants
- [ ] `lower_function()` with local allocation and structured control flow
- [ ] Core emit: WasmIR → wasm-encoder (mechanical)
- [ ] Gate: all `spec/lang/` + `spec/stdlib/` pass through new pipeline (240/240)

### Phase 2: Component Model

- [ ] WIT generation from Almide pub exports (`almide build --target wasm-component`)
- [ ] Canonical ABI adapter generation (string, list, record, variant, option, result)
- [ ] `wit-component` integration for component wrapping
- [ ] Gate: compiled component validates with `wasm-tools validate --features component-model`

### Phase 3: WASI Preview 2

- [ ] Replace fd_write/fd_read with `wasi:cli/std{in,out,err}`
- [ ] Replace path_open/fd_seek etc. with `wasi:filesystem/types`
- [ ] Replace clock_time_get with `wasi:clocks/monotonic-clock`
- [ ] Add `wasi:http/outgoing-handler` for HTTP client
- [ ] Gate: `almide run app.almd --target wasm` works on wasmtime with WASI Preview 2

### Phase 4: Exception Handling (WASM 3.0 v2)

Waiting for wasmtime to enable by default.

- [ ] `try_table`/`throw` for effect fn `?` propagation
- [ ] Zero-cost error path (no Result heap allocation)
- [ ] Gate: effect fn benchmarks show measurable improvement

### Phase 5: Component Model Async (WASM 3.0 v3)

- [ ] `fan` → `future<T>` + `waitable-set` multiplex
- [ ] `stream<T>` for streaming data
- [ ] Gate: fan benchmarks work on wasmtime with WASI 0.3

## What Dies

All of current `emit_wasm/` (40 files, ~15K LOC):

| Current | Replacement |
|---------|------------|
| `expressions.rs`, `statements.rs`, `control.rs` | `lower_expr()`, `lower_stmt()` |
| `calls_*.rs` (12 files) | stdlib lowering rules |
| `rt_*.rs` (6 files) | runtime as WasmIR sequences |
| `runtime.rs`, `runtime_eq.rs` | emit phase primitives |
| `closures.rs`, `calls_lambda.rs` | closure lowering |
| `equality.rs`, `collections.rs` | `DeepEq` op, collection lowering |
| `scratch.rs` | local allocator in emit phase |
| `values.rs` | LayoutRegistry |
| `dce.rs` | WasmIR-level DCE pass |

Kept: `engine/` (LayoutRegistry, WasmIR, WasmBuilder, emit) — this IS the new engine.

## Existing Foundation

Already implemented in `emit_wasm/engine/`:
- **LayoutRegistry**: All layouts defined (String, List, SwissMap, Set, AllocHeader, ClosurePair, Variant, Option, Result)
- **WasmIR**: 40+ op variants with typed memory access
- **WasmBuilder**: Chainable API with layout-safe field access, collection iteration, RC ops
- **Emit**: WasmIR → wasm-encoder translation

23 files already migrated to LayoutRegistry. WasmBuilder used in list_layout.rs and Perceus core.

## Metrics

- **Correctness**: `almide test spec/ --target wasm` — 240/240
- **Stack safety**: zero stack-effect violations (structurally enforced)
- **Layout safety**: zero hardcoded offsets (enforced by LayoutRegistry)
- **Binary size**: ≤ current
- **Performance**: ≥ current
- **Code volume**: emit_wasm/ LOC reduced by ≥50%
- **Component Model**: validates with `wasm-tools validate --features component-model`
- **WASI Preview 2**: runs on wasmtime without Preview 1 adapter
