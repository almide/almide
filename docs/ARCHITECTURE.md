# Architecture

Almide is a ~170,000-line pure-Rust compiler organized as a workspace of a CLI
binary + 13 library crates (plus the cargo-excluded `almide-kernel` SIMD crate
and the Lean 4 proof project `almide-perceus-belt`). Key dependencies: `serde` +
`serde_json` (AST serialization), `toml` (template loading), `clap` (CLI),
`lasso` (string interning), `wat` + `wasmparser` + `wasm-encoder` (WASM
assembly/validation), `lsp-server` + `lsp-types` (language server).

## Pipeline

```
  .almd source
       │
       ▼
  ┌─────────┐   ┌──────────┐   ┌─────────┐   ┌──────────────┐
  │  Lexer   │──▶│  Parser  │──▶│   AST   │──▶│ Type Checker │   almide-syntax / almide-frontend
  └─────────┘   └──────────┘   └─────────┘   └──────┬───────┘
                                                     │  expr_types + env
                                                     ▼
                               ┌───────────────────────────────┐
                               │   Lowering (AST → Typed IR)   │   almide-frontend
                               └──────────────┬────────────────┘
                                              ▼
                               ┌───────────────────────────────┐
                               │ optimize → verify → mono →    │   almide-optimize
                               │ ir_link                       │
                               └──────────────┬────────────────┘
                          ┌───────────────────┼──────────────────────┐
                          ▼                   ▼                      ▼
                 Rust target          WASM target             WGSL target
                 codegen v3           v1 MIR trust-spine      emit_wgsl
                 (nanopass + TOML     (almide-mir → WAT →     (almide-codegen)
                 templates + walker)  wat assemble)
                          │                   │
                          ▼                   ▼
                 native trust-spine      .wasm (verified;
                 (v1 Rust render,        optional --wasm-opt
                 v0 fallback on wall)    with parity gate)
                          │
                          ▼
                 rustc/cargo → binary
```

`almide-interp` sits beside the backends as a third executor: a tree-walking
interpreter over the linked IR (after `lower → optimize → mono → ir_link`,
before any target-lowering pass). It shares no codegen pass with either backend
and serves as the cross-target oracle / executable spec.

### Targets

- **native (default)** — codegen v3 emits Rust source; the v1 native
  trust-spine renderer (`almide-mir`) replaces it where it can lower (v0
  codegen source is the fallback on a wall); `rustc`/`cargo` produces the
  binary.
- **`--target wasm`** — two legs
  (`src/cli/build.rs::render_wasm_module_routed`): cheap PROJECT-SHAPE
  routes pick the leg up front, and a structural wall reroutes (below):
  - the **commissioned structural leg** (default): `almide::wasm_leg`
    (parse→check→lower→self-host link→`link_ir`) feeds
    `almide-wasm::emit_program`, which emits wasm bytes structurally
    (wasm-encoder — no WAT text). Measured 610/610 byte-identical to native
    on the full wasm_cross corpus. `almide run` executes on the embedded
    `almide-wasm-run` host (fs/env/stdin included); `almide build` ships the
    `to_wasi` form, which runs on STOCK runtimes (`wasmtime run mod.wasm` —
    the 578-fixture stock-runtime gate is the witness).
  - the **incumbent WAT trust-spine**: `almide-mir` renders WAT, the CLI
    assembles it with `wat` and strips local names. Routed for main-less
    library modules (#881), dependency-package and `import self` projects
    (#1596), host-variant BUILD artifacts, and `ALMIDE_FUEL_PROBE`
    instrumentation; `ALMIDE_WASM_INCUMBENT=1` forces it (the reversible
    switch, kept for one release).
  A structural WALL reroutes to the incumbent renderer (both legs are
  VERIFIED — this is not #782's sin, which was falling into unverified v0
  codegen; `ALMIDE_VERIFIED_DEBUG=1` names the wall that rerouted). A shape
  NEITHER leg lowers is a hard, diagnosed error with the incumbent's rich
  wall rendering. `--wasm-opt` is opt-in and
  guarded by a differential parity gate against the verified module
  (`tests/wasm_runtime_opt_parity.rs::wasm_opt_parity_spec`).
- **`--target wasm32` / `wasi`** — the generated Rust source compiled by bare
  `rustc --target wasm32-wasip1` (SIMD128 enabled). A different beast from
  `--target wasm`.
- **`--target wgsl`** — GPU compute shaders via `emit_wgsl` in almide-codegen.

`--verified` is the default (`--no-verified` is deprecated and warns).
`almide run --target wasm` and `almide build --target wasm` share
`compile_to_wasm_bytes`, so run and build observe one emission. On the
structural leg the BUILD artifact additionally passes `to_wasi` (stock-runtime
form) — same observable behavior, different bytes; on the incumbent leg the
two stay byte-identical.

## Crate Structure

```
almide/                    Workspace root
├── src/                   CLI binary (almide)
│   ├── main.rs            Subcommands: init, run, build, test, check, lsp,
│   │                      explain, fmt, compile, clean, add, deps, dep-path,
│   │                      install, self-update, ide, fix, docs-gen
│   │                      (no subcommand → REPL)
│   ├── compile_driver.rs  Shared front half: parse → check → lower → optimize
│   ├── resolve.rs         Module resolution (filesystem + git deps)
│   ├── project.rs         almide.toml parsing, PkgId
│   ├── project_fetch.rs   Git dependency fetching
│   └── cli/               Subcommand implementations; lsp.rs is the LSP server
│                          (lsp-server over stdio); repl.rs; ide.rs = semantic
│                          queries for agents (outline, doc, stdlib-snapshot)
│
├── crates/
│   ├── almide-base/       Foundation: Sym interning (lasso), Span, Diagnostic
│   ├── almide-syntax/     Lexer, recursive-descent parser, untyped AST
│   ├── almide-types/      Resolved Ty, unification, protocol defs,
│   │                      stdlib_info.rs (module registry, auto-import lists,
│   │                      bundled stdlib sources via include_str!)
│   ├── almide-lang/       Facade crate: re-exports syntax + types (no logic)
│   ├── almide-frontend/   Type checker (Infer → Solve → Resolve), AST→IR
│   │                      lowering, canonicalize, ir_link
│   ├── almide-ir/         Typed IR: IrProgram/IrExpr (every node carries ty),
│   │                      VarId/VarTable, visitors
│   ├── almide-mir/        v1 Middle IR — single source of truth for ownership
│   │                      and layout (Perceus). #![forbid(unsafe_code)].
│   │                      Renders the wasm (WAT) and native trust-spines.
│   ├── almide-optimize/   Monomorphization, DCE, constant propagation,
│   │                      stream fusion
│   ├── almide-codegen/    Codegen v3 for Rust (+ WGSL): nanopass pipeline,
│   │                      TOML template renderer, target-agnostic walker
│   ├── almide-interp/     Pre-codegen IR interpreter — 3rd cross-target oracle
│   ├── almide-tools/      Formatter, module interface (almide compile), ALMDI
│   ├── almide-dialect/    Pure-Rust MLIR dialect schema (no FFI)
│   ├── almide-egg-lab/    Equality-saturation (egg) PoC, isolated
│   ├── almide-kernel/     [cargo-excluded] target-specific SIMD numeric kernels
│   └── almide-perceus-belt/  [Lean 4, not a cargo crate] Perceus RC/ownership
│                             proof belt
│
├── grammar/               Git submodule → almide/almide-grammar: descriptive
│                          keyword/precedence data consumed by the tree-sitter
│                          and TextMate generators (not by the compiler build)
├── codegen/templates/rust.toml   Rust syntax templates (~330 rules)
├── stdlib/                Self-hosted stdlib: ~280 .almd files (see below)
└── runtime/rs/src/        Native Rust runtime for @intrinsic functions
```

## Codegen v3 (Rust target): Three-Layer Architecture

All semantic decisions are made in the IR before any text is emitted. The
walker sees only typed IR nodes — it never checks what target it renders for.

1. **Nanopass pipeline** — each `pass_*.rs` receives `&mut IrProgram` and does
   one semantic rewrite (StdlibLowering, ResultPropagation, CloneInsertion,
   BuiltinLowering, FanLowering, ...).
2. **Template renderer** — TOML files define syntax patterns; the walker calls
   `templates.render_with("if_expr", ...)`. All string rendering happens here.
3. **Walker** — target-agnostic IR tree renderer; zero `if target == Rust`
   checks.

## WASM Trust-Spine (almide-mir)

The wasm backend is the v1 MIR pipeline in `almide-mir`:

- `pipeline::try_render_wasm_source` lowers linked IR through MIR to WAT text;
  the CLI assembles it (`wat::parse_str`) and keeps only function names in the
  name section.
- Ownership/RC follows Perceus; the crate forbids `unsafe`.
- Stdlib calls are *self-hosted*: pure-Almide implementations from
  `stdlib/*.almd` are registered in `almide-types/src/self_host_registry.rs`
  and compiled along with user code. An unlinked stdlib call is a wall (hard
  error).
- `wasmparser::validate` guards the test harness; `almide test --target wasm`
  falls back to native execution on a wall.

## Stdlib

The stdlib is self-hosted: every function lives in `stdlib/<module>[_part].almd`
(the old `stdlib/defs/*.toml` pipeline is gone). Two consumption paths:

- **Native / type signatures** — module sources are embedded via `include_str!`
  in `almide-types/src/stdlib_info.rs`; the frontend extracts signatures, the
  codegen extracts `@inline_rust` templates. `@intrinsic("almide_rt_*")`
  declarations dispatch to hand-written Rust in `runtime/rs/src/<module>.rs`.
- **WASM** — pure-Almide implementations registered in
  `almide-types/src/self_host_registry.rs` (shared with the interp oracle)
  are compiled to WAT with user code.

Auto-import is the union of the seed list in
`almide-frontend/src/import_table.rs` and `AUTO_IMPORT_BUNDLED` in
`stdlib_info.rs`. Modules like `json`, `fs`, `http`, `env`, `io`, `random`,
`regex`, `testing` require an explicit `import`.

## Build System

The root `build.rs` is empty; codegen lives in crate-specific build scripts:

- `almide-codegen/build.rs` — scans `runtime/rs/src/*.rs` → generates
  `arg_transforms.rs` (per-function argument decoration) and `rust_runtime.rs`
  (embedded runtime via `include_str!`).
- `almide-frontend` — generates `stdlib_sigs.rs` (signatures for checking).

## Type System

Constraint-based inference with eager unification:

1. **Infer** — walk AST, assign fresh type variables, collect constraints
2. **Solve** — unify constraints, propagate solutions
3. **Resolve** — replace inference variables with concrete types in `expr_types`

UFCS resolution: `xs.map(f)` → checker finds `builtin_module_for_type(List) =
"list"` → dispatches to `list.map(xs, f)`. Pipes, UFCS, and string
interpolation are desugared once, in lowering — codegen never sees them.

## Module System

1. **Resolve** (`src/resolve.rs`) — walk `import` declarations, find `.almd`
   files (local, git deps; stdlib handled via bundled sources)
2. **Register** — module signatures added to TypeEnv with prefixes
3. **Check** → **Lower** → per-module IR with separate namespaces
4. **Codegen** — module functions emitted with `almide_rt_{module}_{func}`
   prefixes

## Diagnostics

Every diagnostic includes an error code (E001–E030, E420 — see
[diagnostics/](./diagnostics/)), file:line:col, source context with underline,
and an actionable hint. Output: human-readable or `--json`.

```
error[E005]: argument 'xs' expects List[Int] but got String
  at line 5
  in call to list.sort()
  hint: Fix the argument type
  |
5 | let sorted = list.sort("hello")
  |                        ^^^^^^^
```

## Verification Spine

- **Contracts** — every observable cross-target promise is a named
  `[[contract]]` in [contracts/contracts.toml](./contracts/contracts.toml) with
  executable evidence (fixture / fuzz / Σ-probe / theorem).
- **3-way oracle** — native, wasm, and `almide-interp` must agree on
  stdout/stderr/exit code.
- **Lean belt** — `almide-perceus-belt` proves the Perceus RC discipline that
  `almide-mir` implements.
