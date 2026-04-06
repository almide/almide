# almide-codegen

IR → source code (Rust) or binary (WASM). The final pipeline stage.

## Three-Layer Architecture

### 1. Nanopass Pipeline (`pass_*.rs`)

20+ semantic transformation passes, each doing one thing. Passes are composed per target:

- **Rust:** ListPatternLowering → BoxDeref → TCO → LICM → TypeConcretization → StreamFusion → BorrowInsertion → CaptureClone → CloneInsertion → MatchSubject → EffectInference → StdlibLowering → AutoParallel → ResultPropagation → BuiltinLowering → ClosureConversion (WASM only) → FanLowering
- Each pass: `impl NanoPass { fn run(&self, program, target) -> PassResult }`

### 2. TOML Templates (`templates/*.toml`)

Syntax patterns for each target. Walker substitutes `{var}` placeholders. Target differences live here, not in the walker.

### 3. Walker (`walker/`)

Target-agnostic IR renderer. **Zero `if target == Rust` checks.** All target decisions made in passes + templates.

### 4. WASM Direct Emit (`emit_wasm/`)

Bypasses templates/walker entirely. IR → WASM binary via `wasm-encoder`. Has its own runtime (string ops, list ops, equality, alloc) compiled inline.

## Rules

- **Walker must stay target-agnostic.** If you need target-specific behavior, add a nanopass or a template guard.
- **Nanopass passes are independent.** Each pass reads and rewrites the IrProgram. Passes must not assume ordering except through declared `Postcondition`s.
- **WASM emit is self-contained.** It doesn't share code with the Rust walker. Type resolution, runtime functions, memory layout — all in `emit_wasm/`.

## WASM Emitter (`emit_wasm/`)

```
mod.rs            Orchestrator: register types → register functions → compile
functions.rs      IrFunction → WASM function body
closures.rs       Lambda pre-scan + ClosureCreate compilation
statements.rs     Statement emission + local variable pre-scanning
expressions.rs    Expression emission (literals, binops, calls, match, etc.)
control.rs        Match arm emission (patterns, guards, destructuring)
equality.rs       Deep equality, comparison, record field extraction
collections.rs    Record/list/tuple/spread construction
values.rs         Ty → ValType mapping, byte sizes, field offsets
calls*.rs         Stdlib dispatch (string, list, map, option, result, etc.)
rt_*.rs           Inline WASM runtime (string ops, regex, numeric, value)
runtime.rs        Core runtime (alloc, print, concat, equality)
runtime_eq.rs     mem_eq, list_eq, list comparison
scratch.rs        ScratchAllocator (bump/reuse typed temp locals)
dce.rs            WASM-level dead function elimination
```

### Key WASM Invariants

- **Memory layout:** `[len:i32][data...]` for strings and lists. Records: `[field0][field1]...`. Variants: `[tag:i32][payload...]`.
- **`string.len` returns char count** (UTF-8 code points), not byte count. Byte count is `i32_load(0)` on the string pointer.
- **`Ty::Unknown` → `ValType::I32` (default).** When type inference fails, WASM falls back to i32 (pointer). This is often wrong for Int (i64) — the closure conversion + function registration pipeline applies fallback resolution.
- **Closure convention:** `(env_ptr: i32, params...) → ret`. All closures are two-word `[table_idx: i32, env_ptr: i32]`. `call_indirect` dispatches via the function table.
- **Anonymous records** must be registered in `record_fields` via `register_anonymous_records()` so that `emit_member` can resolve field offsets even when the Lambda param type is unresolved.
- **`sort_by` on empty lists:** The outer loop guard checks `len < 2` to prevent unsigned underflow of `len - 1`.

### Adding Stdlib Functions (WASM)

1. Register signature in `rt_*.rs` (`register` function).
2. Implement body in `rt_*.rs` (`compile_*` function).
3. Dispatch in `calls_*.rs` (method match arm).
4. Add Almide test in `spec/stdlib/`.
