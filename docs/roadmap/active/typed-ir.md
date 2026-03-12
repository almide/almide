# Typed IR [ACTIVE]

## The Problem

Almide's codegen goes directly from AST to target language strings. This causes:

1. **Duplicated logic** — Rust and TS emitters independently implement the same semantic decisions (while, IndexAccess, match, operator dispatch, UFCS, effect fn, do-block, guard, pipe)
2. **Ad-hoc optimizations on AST** — `expr_has_float`, `single_use_vars`, borrow analysis all operate on untyped string-based AST
3. **Fragile type checks at codegen** — emitters inspect `resolved_type` at runtime to decide codegen, e.g. "is this expression Float?"
4. **New targets require full reimplementation** — adding WASM direct emit would mean implementing every construct a third time

## Design

Insert a typed IR between the checker and emitters:

```
AST → Checker → IR (fully typed) → Emit Rust
                                  → Emit TS
                                  → Emit WASM (direct)
```

### Core Principles

- **Every node carries a concrete `Ty`** — no runtime type queries during codegen
- **VarId** — variables identified by unique ID, not string name. Eliminates shadowing bugs in analysis
- **CallTarget** — all calls (UFCS, pipe, module, stdlib, constructor) resolved during lowering
- **Desugared** — pipe → call, interpolated string → format parts, do-block with guard → loop+break
- **No SSA** — tree-based IR with scoping is sufficient for Almide's structured control flow
- **TOML stdlib dispatch preserved** — `CallTarget::StdlibFn` delegates to existing generated codegen

### What moves to IR lowering (out of emitters)

- UFCS resolution
- Pipe desugaring
- InterpolatedString parsing (currently re-parses at codegen time)
- Effect fn auto-? propagation
- Do-block guard-to-loop lowering
- Ok/Err context-dependent behavior
- Binary operator type dispatch (float vs int vs bigint)
- Variant constructor qualification + Box wrapping
- Guard action resolution

### What stays in codegen (target-specific)

- Type syntax (`i64` vs `number`, `Vec<T>` vs `Array`)
- Clone/borrow insertion (Rust only)
- Result erasure (TS only)
- Runtime preamble
- `unsafe { get_unchecked }` (Rust --unchecked-index only)

### What becomes IR optimization passes

- Single-use variable analysis (on VarId, not string names)
- Borrow inference (on VarId with type info)
- Concat-to-push optimization

## Phases

- [x] Phase 1: IR type definitions (`src/ir.rs`) + expression lowering (literals, binop, if, block)
- [x] Phase 2: Call/UFCS/pipe lowering, InterpolatedString resolution
- [x] Phase 3: Control flow (match, for, while, do-block, guard) + statements + declarations
- [x] Phase 4: Analysis passes on IR (borrow, single-use, concat→push)
- [x] Phase 5: Rust emitter from IR
- [x] Phase 6: TS emitter from IR
- [x] Phase 6b: User module IR lowering (multi-segment paths, aliases, bundled stdlib)
- [ ] Phase 7: WASM emitter from IR (see emit-wasm-direct.md)
- [ ] Phase 8: Remove old AST-direct codegen, remove ResolvedType
