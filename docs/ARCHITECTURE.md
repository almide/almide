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

## Optimization pass roster per target

Every optimization or rewrite pass, where it runs, and for which target
(#929). Gated: `scripts/check-pass-roster.sh` enumerates the `pass_*.rs`
files and `NanoPass::name()` strings in `almide-codegen`, and the pass modules
and `ALMIDE_ONLY_PASS` axis names in `almide-optimize`, and fails when one is
missing from this section — the roster cannot drift from the code. The
measured cost of the divergence is the wasm/native runtime ratio per program in
[docs/benchmarks/wasm-runtime.txt](./benchmarks/wasm-runtime.txt), ratcheted by
`scripts/check-wasm-runtime-ratio.sh`.

Which leg runs what:

| Leg | Selected when | Pre-split (shared) | Target-side passes |
|---|---|---|---|
| native, v1 MIR render | `almide run`/`build` default (`--verified`), where `almide-mir` lowers | `link_ir` | almide-mir native rung (table D, the native rows) |
| native, codegen v3 | `almide test` (native), and the fallback on a v1 native wall; `--target rust` emit; `--target wasm32`/`wasi` | `link_ir` | almide-codegen nanopass pipeline, Rust arm (table B) |
| `--target wasm`, structural (default) | `src/wasm_leg.rs` → `almide-wasm::emit_program` | `link_ir` | almide-wasm emitter rewrites (table C) |
| `--target wasm`, incumbent (retiring, #1696) | library modules, dependency projects, `ALMIDE_WASM_INCUMBENT=1`, structural wall reroute | `link_ir` | almide-mir wasm rung + WAT-level passes (table D) |
| `--target wgsl` | `almide-codegen::emit_wgsl` | `link_ir` | almide-codegen nanopass pipeline, Wgsl arm (table B, 4 passes) |
| interp (oracle) | `almide-interp` | `link_ir` | none — by design |

### A. Shared, pre-split — `almide-driver::link_ir` (`crates/almide-optimize`)

The ONE stage order every leg and the interp oracle consume (`tests/one_driver_test.rs`
fails on a second spelling). `optimize_half` = rows 1–7; `link_half` = rows 8–9.
`ALMIDE_DISABLE_OPT=1` skips the three perf rows (the ablation leg of the perf
ratchet); `ALMIDE_ONLY_PASS=fold|dce|propagate` runs exactly one of them
(`scripts/check-pass-isolated.sh`, `spec/pass_isolated/`).

| # | Pass | Module | Kind | Does |
|---|---|---|---|---|
| 1 | `fold` | `optimize/mod.rs` (`constant_fold`) | opt (perf) | evaluate literal arithmetic bottom-up, wrapping; substitutes earlier immutable scalar top-lets (#809) |
| 2 | `dce` | `optimize/dce.rs` | opt (perf) | drop unused bindings whose value is pure; runs again after propagation |
| 3 | `propagate` | `optimize/propagate.rs` | opt (perf) | replace vars bound to literals with the literal |
| 4 | unsigned re-fold | `optimize/mod.rs` (`refold_unsigned_lane`) | correctness | re-fold the `UInt64` `/`/`%` literal pairs propagation exposes, unsigned (#872) — never ablated |
| 5 | optional-chain desugar | `optimize/optional_chain.rs` | lowering enabler | `p?.f` → call to a synthesized tail helper both backends prove — never ablated |
| 6 | branch-lift | `optimize/branch_lift.rs` | lowering enabler | heap-typed `let`-bound `if`/`match` → tail helper fn (the shape the trust-spine renderer lowers) — never ablated |
| 7 | top-let reclassify | `almide_ir::reclassify_top_lets` | analysis | const-vs-runtime top-let classification after folding |
| 8 | monomorphize | `mono/` (`monomorphize`) | specialization | clone structurally-bounded generic fns per call-site type; runs `mutual_tco` (`mutual_tco.rs`: mutually-recursive tail calls → one loop) as its last step |
| 9 | ir_link | `almide-frontend::ir_link` | link | link dependency modules into the root after the monomorphized call graph exists |

### B. `almide-codegen` nanopass pipeline (native codegen v3 / WGSL)

Run order is `crates/almide-codegen/src/target.rs::build_pipeline`. The pass
`Target` enum is `{Rust, Wgsl}` — **the wasm legs never enter this pipeline**;
"all" in the Targets column means Rust + Wgsl as declared by `targets()`, which
is what makes a pass *eligible* to run pre-split, not evidence that it should.
The Wgsl arm is the four rows marked W. Class:

- **Rust by design** — exists because Rust has ownership, borrows, `Result`,
  macros or `LazyLock`; porting is meaningless for wasm.
- **enabler** — a rewrite/analysis the TOML walker needs; the structural leg
  has its own equivalent (named) and needs no port.
- **optimizer** — a performance rewrite; the "structural leg" column says
  whether an equivalent exists, and the pre-split decision follows in E.

| # | Pass (`name()`) | File(s) | Targets | Class | Does | Structural wasm leg |
|---|---|---|---|---|---|---|
| 1 | `UnifyVarTables` | `pass_unify_var_tables.rs` | all, W | enabler | merge every `IrModule.var_table` into the program table | reads per-module tables (`build_globals`) |
| 2 | `ListPatternLowering` | `pass_list_pattern.rs` | all | enabler | list patterns → length checks + indexing | lowers `IrPattern::List` natively (`patterns.rs`) |
| 3 | `LambdaTypeResolve` | `pass_lambda_type_resolve.rs`, `pass_lambda_type_lookup.rs` | all, W | enabler | closure param types from the stdlib callee signature (top-down) | `TypeTable` (`types_table.rs`) |
| 4 | `ConcretizeTypes` | `pass_concretize_types.rs`, `pass_concretize_types_call_ret.rs`, `pass_concretize_types_signatures.rs`, `pass_concretize_types_unresolved.rs`, `pass_concretize_types_walker.rs` | all, W | enabler | sync every `IrExpr.ty` with its authoritative concrete type | `TypeTable` |
| 5 | `PatternLiteralGuard` | `pass_pattern_literal_guard.rs` | Rust | Rust by design | hoist payload-nested string literals into guards (the `as_deref` subject form) | n/a |
| 6 | `ResolveCalls` | `pass_resolve_calls.rs` | all | enabler | verify-and-rewrite every `CallTarget::Module` to a known fn | `FnTable` resolution (`emit.rs`) |
| 7 | `BoxDeref` | `pass_box_deref.rs` | Rust | Rust by design | `*deref` for pattern vars bound from `Box`'d fields | n/a |
| 8 | `LICM` | `pass_licm.rs`, `pass_licm_hoist.rs`, `pass_licm_purity.rs` | all | optimizer | hoist loop-invariant pure expressions to `let`s before the loop | **no equivalent** — see E |
| 9 | `EggSaturation` | `pass_egg_saturation.rs` | all | optimizer | equality-saturation fusion of matrix and list combinator chains (`almide-egg-lab`, rules from stdlib `@rewrite`) | list half: `list_fuse.rs` (map/filter → fold); matrix half: **no equivalent** — see E |
| 10 | `MatrixShapeSpec` | `pass_matrix_shape_spec.rs` | Rust | Rust by design | small-shape matmul → fully unrolled `InlineRust` | n/a (hand-written kernels, `matrix_kernels.rs`) |
| 11 | `ConstFold` | `pass_const_fold.rs` | all | optimizer | fold literal arithmetic left by rows 9–10 | superseded by A.1 (`fold`) — see E |
| 12 | `IntrinsicLowering` | `pass_intrinsic_lowering.rs` | all | enabler | `@intrinsic` stdlib calls → `RuntimeCall { symbol }` | self-host registry link (`src/wasm_leg.rs`); intrinsics are walls |
| 13 | `BorrowInsertion` | `pass_borrow_inference.rs`, `pass_borrow_inference_call_sites.rs`, `pass_borrow_inference_ownership.rs` (wrapper in `pass.rs`) | Rust | Rust by design | Roc-style borrow-by-default signatures, `Borrow` nodes at call sites | RC-3 borrow/fresh classifier (`rc_ownership.rs`) |
| 14 | `TailCallOpt` | `pass_tco.rs`, `pass_tco_loop_rewrite.rs`, `pass_tco_owned_reads.rs` | all | optimizer | self-recursive tail calls → loop | equivalent: `tco.rs` (`loop_convert` over the encoded body; `return_call` otherwise) |
| 15 | `CaptureClone` | `pass_capture_clone.rs` | Rust | Rust by design | pre-clone variables captured by `move` closures | n/a |
| 16 | `CloneInsertion` | `pass_clone.rs`, `pass_clone_interp.rs`, `pass_clone_loops.rs` | Rust | Rust by design | `Clone` nodes for heap-typed reuse (loops, interpolation, E0505 guards) | RC inc/share guards (`rc_ownership.rs`) |
| 17 | `MatchSubject` | `pass_match_subject.rs` | Rust | Rust by design | `.as_str()` / `.as_deref()` on match subjects | n/a |
| 18 | `EffectInference` | `pass_effect_inference.rs` | all | analysis | infer capability categories from transitive stdlib use | shared by another route: `cli::check_permissions` runs this pass standalone on the pre-mono IR for every leg |
| 19 | `StdlibLowering` | `pass_stdlib_lowering.rs`, `pass_stdlib_lowering_ufcs.rs` | Rust | Rust by design | `Module` calls → `Named` runtime calls with arg decoration | self-hosted stdlib bodies are emitted as wasm fns |
| 20 | `AutoParallel` | `pass_auto_parallel.rs` | Rust | Rust by design | pure `list.map/filter/any/all` → `std::thread::scope` variants | n/a (single-threaded wasm) |
| 21 | `ResultPropagation` | `pass_result_propagation.rs` | all | Rust by design | effect fn `T → Result[T, String]`, `Try` at call sites | own effect lowering (`effect_raw`, `emit.rs`) |
| 22 | `BuiltinLowering` | `pass_builtin_lowering.rs` | Rust | Rust by design | `assert_eq`/`println`/… → `RustMacro` | n/a |
| 23 | `Peephole` | `pass_peephole.rs` | all | optimizer | idiomatic list loops → `ListSwap`/`ListReverse`/`ListRotateLeft`/`ListCopySlice` nodes | **no lowering for those nodes** — see E |
| 24 | `RustLowering` | `pass_rust_lowering.rs` | Rust | Rust by design | push optimization, borrow index lift | n/a |
| 25 | `FanLowering` | `pass_fan_lowering.rs` (wrapper in `pass.rs`) | all, W | enabler | strip auto-try from fan spawn closures | own fan lowering (`fan.rs`) |
| 26 | `NormalizeRuntimeCalls` | `pass_normalize_runtime_calls.rs` | Rust | Rust by design | legacy `Named { almide_rt_* }` → `RuntimeCall` | n/a |
| 27 | `IrLinkFlatten` | `pass_ir_link_flatten.rs` | Rust | Rust by design | flatten modules into the root for the walker | keeps modules, qualified names |
| 28 | `SharedCellBorrow` | `pass_shared_cell_borrow.rs` | Rust | Rust by design | borrow a captured cell in place for statement-proven-safe reads (#1143) | n/a |
| 29 | `RangeCountingVars` | `pass_range_counting.rs` | Rust | optimizer | a `let`-bound range read ONLY as `for-in` heads stays a bare `Range<i64>` instead of a materialized `Vec<i64>` (#1857); mirrors MIR's #1400 `range_counting_vars` admission rule and runs last so the set names the final IR | `ranges.rs` counting loop (#1400) — already has it |
| 30 | `TopLetStorage` | `pass_top_let_storage.rs` | all | analysis | the unified top-let storage attribute for the walker (§4 Stage 1) | own globals plan (`build_globals`) |

### C. Structural wasm leg — `crates/almide-wasm` (default `--target wasm`)

Rewrites the emitter applies to the linked IR or the encoded body. None is an
IR→IR pass in the nanopass sense; each is a route inside `emit_program`.

| Pass | File | Does |
|---|---|---|
| transparent-newtype erasure | `newtype.rs` | erase transparent aliases so both emission passes read one tree (#1423 stage 4) |
| reachability DCE | `emit.rs` (two-pass) | pass 1 records what `main` reaches; pass 2 re-emits with only the reachable set |
| deterministic meter plan | `fuel.rs` | which fns charge, whose entry is exempt (ALS-DT2, mirrors the interp) |
| self-tail-call loop conversion | `tco.rs` | `return_call $self` → param `local.set` + `br` to a wrapping `loop` |
| map/filter → fold fusion | `list_fuse.rs` | deforestation over observation-free callbacks (the list half of B.9) |
| counted-while partial unroll | `unroll.rs` | the loop-control headroom LLVM takes on the native leg |
| RC-3 ownership guards | `rc_ownership.rs` | borrow/fresh classification, droppable set, inc/share/arg guards (the wasm twin of B.13/B.16) |
| heap cap | `heap_cap.rs` | harness-set linear-memory ceiling (#1729), not an optimization |

No SIMD (`v128`) is emitted anywhere on this leg; the matrix routines are
scalar kernels (`matrix_kernels.rs`). Out of scope for #929 by decision.

### D. Incumbent trust-spine — `crates/almide-mir` (retiring under #1696)

Listed for completeness; the roster gate does not enumerate this crate because
its rows leave with the retirement.

| Pass | File | Rung | Does |
|---|---|---|---|
| charge probe | `charge_probe.rs` | wasm + native (`ALMIDE_FUEL_PROBE` builds) | `Charge` at fn entry and after every `LoopStart` |
| self-append rewrite | `concat_to_append.rs` | wasm | `x = x + [e]` → in-place append |
| scalar wrapper inline | `scalar_call_inline.rs` | wasm | inline single-prim scalar stdlib wrappers (#826) |
| dead `MakeUnique` elision | `alias_safety.rs` | wasm + native | drop unaliased copy-on-write guards (#824) |
| region allocation / compaction | `region_alloc.rs`, `region_compact.rs` | wasm | `consume(produce(scalars))` windows become bump regions (#838) |
| native Result carrier rewrite | `native_result_rewrite.rs` | native | T1-3 Result producer/consumer rewrite for the Rust render |
| operand fusion | `render_wasm_fuse.rs` | wasm (WAT) | fold a value's defining op into its single consumer |
| constant fold through extend/wrap | `render_wasm_peephole.rs` | wasm (WAT) | narrow text peephole over each rendered body |
| preamble DCE | `render_wasm_dce.rs` | wasm (WAT) | dead WASI import / helper / data elimination |
| bounds-check elision | `render_wasm_bce.rs` | wasm (WAT) | loop versioning for hot `v[i]` (#806 step 4) |
| `br_table` dispatch | `render_wasm_switch.rs` | wasm (WAT) | dense integer `match` → `br_table` (#882) |
| local-slot reuse | `render_wasm_local_reuse.rs` | wasm (WAT) | SSA locals shared in oversized fns (#1554) |

### E. Pre-split decision (closure of #929)

The shared subset **already runs pre-split**: table A is the whole of
`almide-optimize` and every leg in the first table consumes it through the one
`link_ir`. The question is only whether a target-neutral row of table B should
join it. Verdict per candidate:

| Pass | Move pre-split today? | Blocker / reason |
|---|---|---|
| `ConstFold` (B.11) | no move needed | A.1 folds the same literal arithmetic before the split; on the Rust arm the pass only cleans artifacts of B.9–B.10, which exist on no other leg |
| `TailCallOpt` (B.14) | no move needed | the structural leg converts self tail calls at the encoded-body level (`tco.rs`) and keeps `return_call` for the rest — the same constant-stack guarantee |
| `LICM` (B.8) | **not today — follow-up** | (1) layering: `almide-driver` (owner of the cut point) depends on `almide-optimize`, not `almide-codegen`, and the pass is written against `NanoPass`/`Target` there — relocating three files, not a one-line pipeline change; (2) the cut point is also the interp oracle's and the incumbent's input, so a pre-split hoist needs an `ALMIDE_ONLY_PASS=licm` axis and `spec/pass_isolated/` rows before it lands; (3) the win on the structural leg is unmeasured — measure on the loop-bound ledger rows (`nbody`, `spectralnorm`) first |
| `EggSaturation` (B.9) | **not today — follow-up** | pulls `almide-egg-lab` below the driver; the matrix half rewrites into fused `matrix.*` forms only the Rust stdlib lowering consumes, and the list half already has a structural twin (`list_fuse.rs`) |
| `Peephole` (B.23) | **not today — follow-up** | its output nodes (`ListSwap` …) have no lowering in `almide-wasm`; pre-split it would wall every fixture that hits a pattern (the incumbent's `lower/mod_p5.rs` and the interp are the only consumers) |

`--target wasm32`/`wasi` is a different beast: it compiles the codegen v3 Rust
source with `rustc`, so it gets the full Rust arm of table B plus LLVM.

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
