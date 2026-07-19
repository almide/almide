# Almide Workspace Crates

The Almide compiler is split into a Cargo workspace with focused crates for build parallelism, clear API boundaries, and independent development.

## Architecture

```mermaid
graph TD
    BASE["almide-base<br/><i>~0.6k lines</i><br/>Sym, Span, Diagnostic"]
    SYNTAX["almide-syntax<br/><i>~6.4k lines</i><br/>AST, lexer, parser"]
    TYPES["almide-types<br/><i>~1.5k lines</i><br/>Ty, unify, constructor,<br/>stdlib_info"]
    LANG["almide-lang<br/><i>re-export shim</i><br/>syntax + types"]
    IR["almide-ir<br/><i>~5.1k lines</i><br/>IR nodes, visit, verify,<br/>effect, annotations"]
    CODEGEN["almide-codegen<br/><i>~89k lines</i><br/>nanopass pipeline, walker,<br/>emit_wasm (v0), template"]
    FRONTEND["almide-frontend<br/><i>~14.8k lines</i><br/>check, canonicalize, lower,<br/>stdlib, import_table"]
    OPTIMIZE["almide-optimize<br/><i>~3.2k lines</i><br/>DCE, propagation,<br/>monomorphization"]
    MIR["almide-mir<br/><i>~68k lines</i><br/>v1 trust-spine: MIR, Perceus,<br/>PCC certificates, wasm/native render"]
    INTERP["almide-interp<br/><i>~4.0k lines</i><br/>pre-codegen IR interpreter<br/>(3rd cross-target oracle)"]
    DIALECT["almide-dialect<br/>MLIR dialect schema<br/>pure-Rust, FFI-free"]
    EGG["almide-egg-lab<br/><i>~1.6k lines</i><br/>equality-saturation PoC"]
    TOOLS["almide-tools<br/><i>~1.9k lines</i><br/>fmt, interface, almdi"]
    CLI["almide (CLI)<br/><i>~8.5k lines</i><br/>main, cli/, resolve,<br/>project, project_fetch"]

    BASE --> SYNTAX
    BASE --> TYPES
    SYNTAX --> LANG
    TYPES --> LANG
    LANG --> IR
    LANG --> CODEGEN
    IR --> CODEGEN
    LANG --> FRONTEND
    IR --> FRONTEND
    LANG --> OPTIMIZE
    IR --> OPTIMIZE
    FRONTEND --> MIR
    OPTIMIZE --> MIR
    IR --> MIR
    FRONTEND --> INTERP
    OPTIMIZE --> INTERP
    IR --> INTERP
    IR --> DIALECT
    IR --> EGG
    LANG --> TOOLS
    IR --> TOOLS
    CODEGEN --> CLI
    FRONTEND --> CLI
    OPTIMIZE --> CLI
    MIR --> CLI
    INTERP --> CLI
    DIALECT --> CLI
    TOOLS --> CLI

    style BASE fill:#e8f5e9,stroke:#388e3c
    style SYNTAX fill:#e8eaf6,stroke:#3f51b5
    style TYPES fill:#e8eaf6,stroke:#3f51b5
    style LANG fill:#e3f2fd,stroke:#1976d2
    style IR fill:#fff3e0,stroke:#f57c00
    style CODEGEN fill:#fce4ec,stroke:#c62828
    style MIR fill:#fce4ec,stroke:#880e4f
    style INTERP fill:#e0f2f1,stroke:#00695c
    style DIALECT fill:#ede7f6,stroke:#4527a0
    style EGG fill:#f1f8e9,stroke:#558b2f
    style FRONTEND fill:#f3e5f5,stroke:#7b1fa2
    style OPTIMIZE fill:#e0f7fa,stroke:#00838f
    style TOOLS fill:#fff8e1,stroke:#f9a825
    style CLI fill:#f5f5f5,stroke:#616161
```

**Arrows point from dependency to dependent** (A → B means B depends on A). `almide-base` edges beyond the first tier are elided for readability — every crate depends on it.

## Crate Summary

| Crate | Role | Key Modules |
|-------|------|-------------|
| **almide-base** | Shared primitives | `Sym` (interned strings), `Span` (source locations), `Diagnostic` (error reporting) |
| **almide-syntax** | Syntax layer | AST node definitions, lexer (tokenizer), parser |
| **almide-types** | Type system | `Ty`, `unify`, `constructor` (type constructors), stdlib module registry |
| **almide-lang** | Re-export shim | Combines almide-syntax + almide-types for backward compatibility |
| **almide-ir** | Intermediate representation | Typed IR nodes (`IrExpr`, `IrStmt`, `IrProgram`), visitor pattern, verification, effect system |
| **almide-frontend** | Analysis pipeline | Type checker, name canonicalization, IR lowering, stdlib signatures (build.rs generated) |
| **almide-optimize** | IR optimization | Dead code elimination, constant propagation, generic monomorphization |
| **almide-codegen** | v0 code generation | 20+ nanopass passes, TOML-driven template walker (Rust), direct WASM binary emit (`emit_wasm/`) |
| **almide-mir** | v1 trust-spine (the DEFAULT renderer) | Middle IR with ownership as the single source of truth: Perceus RC insertion, per-function PCC certificates re-verified by the Rocq-kernel-extracted checker, self-hosted stdlib registry, wasm + native render. Falls back to v0 where it walls — a v1-rendered program is never wrong |
| **almide-interp** | Executable spec / 3rd oracle | Tree-walks the pre-codegen `IrProgram` — shares no target-lowering pass with either backend, so the 3-way gate catches both-backends-wrong-the-same-way bugs. Abstentions are ledgered (`interp-abstain-ledger.txt`) |
| **almide-dialect** | MLIR dialect schema | Models MLIR's Region/Block/Operation hierarchy as pure-Rust types (FFI-free) |
| **almide-egg-lab** | Experiment | Equality-saturation (egg) feasibility PoC on a minimal IR subset |
| **almide-tools** | Developer tools | Source formatter, module interface serialization, `.almdi` binary format |
| **almide** (CLI) | Entry point | Command dispatch, project resolution, dependency fetching, content-addressed native build cache. Re-exports all crates. |

Outside the workspace: **almide-kernel** (verified SIMD numeric kernels, its own workspace) and **AlmidePerceusBelt** (`crates/almide-perceus-belt/`, the Lean proofs for the Perceus discipline).

## Compilation Pipeline

```mermaid
flowchart TD
    SRC["Source (.almd)"]
    PARSE["Parse<br/><i>almide-syntax</i><br/>lexer → parser → AST"]
    FRONT["Canonicalize / Check / Lower<br/><i>almide-frontend + almide-types</i><br/>name resolution → TypeMap (ExprId→Ty) → typed IR"]
    OPT["Optimize / Mono<br/><i>almide-optimize</i><br/>DCE, constant propagation, monomorphization"]
    V1["v1 trust-spine — the verified DEFAULT, both targets<br/><i>almide-mir</i><br/>MIR lower → Perceus → PCC certificate<br/>→ kernel-checked → wasm / native render"]
    V0["v0 fallback<br/><i>almide-codegen</i><br/>20+ nanopass rewrites → Rust (template)<br/>or WASM (direct binary)"]
    ORACLE["3-way oracle (test harness)<br/><i>almide-interp</i><br/>pre-codegen tree-walk"]
    OUT["native binary / .wasm"]

    SRC --> PARSE --> FRONT --> OPT
    OPT --> V1
    V1 -->|"walls (honest decline)"| V0
    OPT -.-> ORACLE
    V1 --> OUT
    V0 --> OUT

    style V1 fill:#fce4ec,stroke:#880e4f
    style V0 fill:#fce4ec,stroke:#c62828
    style ORACLE fill:#e0f2f1,stroke:#00695c
```

## Build Parallelism

Once `almide-base` is built, `almide-syntax` and `almide-types` compile **in parallel** (no dependency between them). After those complete, the downstream crates fan out in parallel too:

```mermaid
flowchart LR
    BASE[almide-base] --> SYNTAX[almide-syntax] & TYPES[almide-types]
    SYNTAX --> LANG[almide-lang]
    TYPES --> LANG
    LANG --> IR[almide-ir]
    IR --> CODEGEN[almide-codegen] & FRONTEND[almide-frontend] & OPTIMIZE[almide-optimize] & DIALECT[almide-dialect] & EGG[almide-egg-lab] & TOOLS[almide-tools]
    FRONTEND --> MIR[almide-mir] & INTERP[almide-interp]
    OPTIMIZE --> MIR & INTERP
```

Changing a file in `check/` does **not** recompile codegen (~89k lines), and vice versa. Changing a type definition does **not** recompile the parser.

## Build Scripts

Two crates have `build.rs` for code generation from `stdlib/defs/*.toml`:

| Crate | Generates | From |
|-------|-----------|------|
| **almide-codegen** | `arg_transforms.rs`, `rust_runtime.rs` | `stdlib/defs/*.toml`, `runtime/rs/src/*.rs` |
| **almide-frontend** | `stdlib_sigs.rs` | `stdlib/defs/*.toml` |

## Re-export Pattern

The main `almide` crate re-exports all sub-crates via `pub use` in `lib.rs`, so all existing `almide::module::*` paths continue to work. Similarly, `almide-lang` re-exports `almide-syntax` and `almide-types` for backward compatibility.
