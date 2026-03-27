<!-- description: Candidate new codegen targets (Go, Python, C, Swift, Kotlin) -->
<!-- done: 2026-03-18 -->
# New Codegen Targets

With the IR redesign complete, the cost of adding new targets has dropped significantly. A new backend only needs to accept `&IrProgram` and return a string.

## Why

Almide is "the language LLMs can write most accurately." More targets mean LLM-written Almide code can run in more environments. Since the IR is normalized, each target does not need to understand the complexity of the AST.

## Candidate targets

### Priority 1: High impact

| Target | Output | Use case |
|--------|--------|----------|
| **Go** | `.go` | Cloud-native CLI, server-side. GC, goroutine |
| **Python** | `.py` | ML/data science, scripting. Largest ecosystem |

### Priority 2: Strategic

| Target | Output | Use case |
|--------|--------|----------|
| **C** | `.c` | Embedded systems, maximum portability, FFI bridge |
| **Swift** | `.swift` | iOS/macOS native apps |
| **Kotlin** | `.kt` | Android, JVM server-side |

### Priority 3: Experimental

| Target | Output | Use case |
|--------|--------|----------|
| **Zig** | `.zig` | Rust alternative, WASM, C interop |
| **Lua** | `.lua` | Game engine embedding (Roblox, Neovim) |

## Implementation pattern

Each target is implemented using the same pattern:

```rust
// src/emit_go/mod.rs
pub fn emit(ir: &IrProgram) -> String {
    let mut emitter = GoEmitter::new();
    emitter.emit_program(ir);
    emitter.out
}
```

Transformations for the main IR nodes:

| IR Node | Each Target's Responsibility |
|---------|------------------------------|
| `IrExprKind::BinOp { op: AddInt }` | `+` (common across all languages) |
| `IrExprKind::Call { target: Module }` | stdlib mapping (language-specific) |
| `IrTypeDeclKind::Variant` | tagged union / sealed class / enum (language-specific) |
| `IrExprKind::Match` | switch / match / when (language-specific) |
| `IrFunction { is_effect: true }` | Result / Exception / error return (language-specific) |

Estimated size: 500-1000 lines per target (Rust emitter is ~1200 lines, TS is ~800 lines).

## Unlocked by

IR Redesign Phase 5 complete. Codegen input is unified to `&IrProgram`, so new targets do not need to understand the AST.
