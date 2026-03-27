<!-- description: Emit LLVM IR directly to eliminate rustc dependency for end users -->
# Self-Contained Compiler: Remove rustc Dependency

**Goal**: `almide build` does not require rustc. A self-contained compiler like Go.

```
Current:  almide build → Rust source generation → rustc → LLVM → binary (rustc required)
Goal:     almide build → LLVM IR generation → LLVM → binary (rustc not required)
```

---

## Stage 1: Users Don't Need rustc

The Almide compiler itself remains written in Rust. At build time, the Rust runtime is baked into LLVM bitcode, and rustc is not called during user compilation.

### Architecture

```
Almide build time (cargo build):
  runtime/rs/src/*.rs → rustc --emit=llvm-bc → runtime.bc → embed in almide binary

User compile time (almide build):
  .almd → Almide IR → (nanopass pipeline) → LLVM IR
  LLVM IR + embedded runtime.bc → llvm-link → opt → llc → binary
```

### Implementation Steps

| Step | Content | Dependency |
|------|---------|------------|
| 1. LLVM IR emitter | Almide IR → LLVM IR text (basic types: Int, Float, Bool, if/for/call) | inkwell crate |
| 2. Runtime bitcode | `build.rs` runs `rustc --emit=llvm-bc` to compile runtime to bitcode | Existing runtime/ |
| 3. Bitcode embedding | Embed runtime bitcode in Almide binary via `include_bytes!` | Step 2 |
| 4. LLVM link + optimization | Combine bitcode with inkwell → opt → generate executable binary | Steps 1-3 |
| 5. Stdlib dispatch | TOML template → LLVM IR call generation (calls to `almide_rt_*` functions) | Step 1 |
| 6. Coexistence with Rust target | Selectable via `--backend llvm` / `--backend rust` | Step 4 |

### Technical Challenges

- **Type ABI**: LLVM-level representation of String (`{ ptr, len, cap }`), Vec, HashMap. Since the runtime is written in Rust, the ABI is Rust ABI → included in bitcode
- **Generics**: `Vec<i64>` and `Vec<String>` need separate monomorphized functions. Either pre-generate instances for major types on the runtime side, or generate on the user code side
- **Drop / destructors**: Need to call drop at appropriate times at the LLVM IR level
- **LLVM version**: Compatibility between the LLVM version inkwell depends on and the user's environment

### Estimates

- Prototype (Int/Float + arithmetic + if/for): 2-3 weeks
- Basic operation (String/List/Map + major stdlib functions): 1-2 months
- Production quality (full stdlib + error handling + tests): 3-4 months

### What We Get

| Benefit | Effect |
|---------|--------|
| **User experience** | Just `cargo install almide` is enough. No rustc installation needed |
| **Compile speed** | Skip rustc frontend (50-70% of compile time) |
| **LLVM annotations** | Directly attach pure → `readonly`/`willreturn`, immutable → `noalias` |
| **Distribution size** | Almide distributable standalone (no rustc + cargo needed) |

---

## Stage 2: Write Almide Itself in Almide (Self-Hosting)

Rewrite the compiler itself in Almide. Completely remove Rust dependency.

### Prerequisites

- Stage 1 complete (LLVM direct output works)
- Almide language features sufficiently mature (generics, trait/protocol, file I/O, string processing)
- Sufficient test coverage (guaranteeing compiler correctness)

### Incremental Migration

1. **lexer.rs → lexer.almd**: String processing focused, few dependencies
2. **parser/ → parser/**: Recursive descent, data structure manipulation
3. **check/ → check/**: Type inference, most complex
4. **lower/ → lower/**: IR transformation
5. **codegen/ → codegen/**: LLVM IR generation
6. **Bootstrap**: Compile new Almide compiler with old Almide compiler

### Estimates

- 1-2 years (after Stage 1 completion)
- Language stabilization comes first

---

## Priority

Stage 1 >> Stage 2

Stage 1 directly impacts user experience and compile speed. Stage 2 is a technical achievement but lower practical priority.
