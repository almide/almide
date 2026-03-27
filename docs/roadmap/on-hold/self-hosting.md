<!-- description: Rewrite the compiler in Almide for a self-contained 350KB WASM toolchain -->
# Self-Hosting: Autonomous Bootstrap Compiler

**Status**: On Hold (Phase 3+ prerequisite)
**Priority**: Strategic — Directly aligned with mission
**Prerequisite**: Language spec stable, WASM direct emit complete, Protocol/Generics mature

## Why

Almide's mission is **modification survival rate** — being the language LLMs can write most accurately.

Self-hosting is the logical consequence of this mission:

1. **Compiler source is written in Almide** → Becomes the compiler LLMs can read and write most accurately
2. **WASM direct emit can produce a 350KB all-in-one binary** → compiler + formatter + test runner + type checker fit in a single binary
3. **Combining 1 + 2** → LLMs modify the compiler itself → recompile → modify further with the modified compiler, a self-contained loop

In other words: **Place a single 350KB WASM binary in a dev container, and the loop where LLMs sharpen their own tools begins.**

This doesn't work with ordinary languages:
- Source is too complex and LLMs break it → low modification survival rate
- Toolchain dependencies cause environment setup failures → high setup cost
- Binary is too large for easy distribution → high startup cost

Almide is positioned to solve all three.

## Goal State

```
Single 350KB WASM binary:
  almide compile  — Self-compilable
  almide fmt      — Formatter built-in
  almide test     — Test runner built-in
  almide check    — Type checker built-in

Execution environment:
  wasmtime / wasmer / browser / edge — Same binary runs anywhere

LLM loop:
  LLM reads source → modifies → almide test → almide compile → new compiler
  ↑ This loop runs with zero external dependencies
```

## Incremental Migration

| Phase | Target | Reason |
|-------|--------|--------|
| 0 | Formatter | String processing focused, limited damage if broken |
| 1 | Test runner | Few dependencies on compiler core |
| 2 | Lexer | String processing focused, few dependencies |
| 3 | Parser | Recursive descent, data structure manipulation |
| 4 | Type checker | Most complex, uses language features to their fullest |
| 5 | Lowering + codegen | IR transformation, WASM emit |
| 6 | Bootstrap | Compile new Almide compiler with old Almide compiler |

Phase 0-1 doubles as a language feature maturity test. Missing features discovered here feed back into the language.

## Prerequisites

- [ ] WASM direct emit is complete including stdlib
- [ ] Protocol / Generics are stable (needed for compiler internal data structures)
- [ ] File I/O works via WASI
- [ ] Language spec is nearly frozen (minimize bootstrap breakage)
- [ ] Test coverage sufficiently guarantees compiler correctness

### Missing Language Features

**Data Structures / Type System**

| Feature | Status | Notes |
|---------|--------|-------|
| Efficient HashMap/BTreeMap | ❌ | Current `Map` is limited. Essential for symbol tables, type environments |
| Trait / typeclass | ❌ | Abstraction for common interfaces (Display, Eq, Hash) |
| Recursive algebraic data types | ⚠️ | Essential for AST/IR representation. Need to verify recursive variant behavior |
| Generics maturity | ⚠️ | Needed for container types, visitor pattern |

**String / Binary Operations**

| Feature | Status | Notes |
|---------|--------|-------|
| Character-level operations | ❌ | Essential for lexer (peek, advance, char category checks) |
| Byte sequence operations | ❌ | Essential for WASM binary generation (LEB128 encoding, etc.) |
| StringBuilder equivalent | ❌ | Efficient string assembly for code generation |

**Runtime / Control**

| Feature | Status | Notes |
|---------|--------|-------|
| File system (directory traversal) | ❌ | Needed for multi-file compilation |
| Process arguments / exit codes | ⚠️ | Needed for CLI operation |
| Panic / unrecoverable errors | ❌ | ICE (Internal Compiler Error) handling |

## Technical Challenges

- **Bootstrap trust chain**: The first build must be done from the Rust version
- **File I/O on WASM**: Solvable with WASI, but alignment with compiler file access patterns is needed
- **Compiler complexity**: Type inference, pattern matching, and IR transformation push Almide's language features to their limits. If insufficient, the language needs to be extended
- **Binary size**: Design must be mindful of compiler code size to maintain the 350KB target

## What Happens When This Succeeds

**Almide evolves from "the language LLMs can write most accurately" to "the toolchain LLMs can autonomously evolve."**

With a single 350KB WASM binary in a dev container, with no external dependencies:
- LLMs can fix bugs in the compiler itself
- LLMs can add new optimization passes
- LLMs can implement new targets
- All of the above can be verified with tests

When the compiler is small, the source is LLM-readable, and the tools are self-contained — when these three come together, the compiler becomes **not software, but part of the agent**.
