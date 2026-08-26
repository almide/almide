# Almide Compiler Crates

> **Codegen 理想形（完了済みの設計記録）**: [docs/roadmap/done/codegen-ideal-form.md](../docs/roadmap/done/codegen-ideal-form.md) — 新しいパスや emit 修正の設計判断はこの記録に整合させる。

## Pipeline

```
Source (.almd)
  → almide-syntax    Lex + parse → AST
  → almide-frontend  Type check + lower → IR
  → almide-optimize  Monomorphize + DCE → IR
  → almide-codegen   Nanopass + emit → Rust
  → almide-mir       v1 trust-spine → WASM (WAT) / native Rust render
```

## Dependency Graph

```
almide-base           Interned strings (Sym), spans, diagnostics
  ↕
almide-types          Ty enum, unification, stdlib info
almide-syntax         Lexer, parser, AST nodes
  ↕
almide-lang           Re-export facade (types + syntax)
  ↕
almide-ir             Typed IR, VarTable, visitors
  ↕
almide-frontend       Type checker, constraint solver, AST→IR lowering
almide-optimize       Monomorphization, DCE, constant propagation
almide-codegen        Nanopass pipeline, TOML templates, walker（WGSL は on-hold — attribute パースのみ）
almide-mir            v1 Middle IR: ownership/layout SoT, WASM (WAT) + native renderers
almide-interp         Pre-codegen IR tree-walker — 3rd cross-target oracle / executable spec
almide-tools          Formatter, module interface (.almdi). NOT the LSP — that
                      lives in src/cli/lsp*.rs at the workspace root, and
                      lsp-types is a root Cargo.toml dependency, so a
                      crates/-scoped survey will not find it
almide-layout         THE single source for heap block layout — every consumer
                      (almide-wasm, almide-interp's arena) derives from it
almide-wasm           Commissioned structural wasm emitter: typed IR → wasm
                      bytes via wasm-encoder (no WAT text). The default
                      `--target wasm` leg (routing: src/cli/build.rs)
almide-wasm-run       The embedded almide.* host (wasmtime) + the `to_wasi`
                      transform that makes build artifacts stock-runtime
almide-spine          Salsa-cached front queries + the parity gates; its s5
                      driver re-exports `almide::wasm_leg` (root lib), so the
                      product leg and the gates judge ONE implementation
almide-corpus         Corpus path resolution (in-tree, or the als/ judge mount
                      in the greenfield form)
```

`almide-interp` is a *sibling consumer* of the linked IR, not part of the
compile pipeline: it runs the IR at the cut point **after** `lower → optimize →
mono → ir_link` but **before** any of `almide-codegen`'s target-lowering passes,
so it shares no codegen pass with either backend. See
[almide-interp/CLAUDE.md](./almide-interp/CLAUDE.md).

## Core Design Principles

1. **Type checker is source of truth.** All expression types come from `TypeMap` (populated by almide-frontend). Lowering and codegen trust it — they do NOT re-infer types.

2. **IR carries full type info.** Every `IrExpr` has a `ty: Ty` field. Codegen must never need to query the type checker at emit time.

3. **VarId eliminates shadowing.** All variables are assigned unique `VarId(u32)` during lowering. No string-based variable lookup in IR or codegen.

4. **Desugar once in lowering.** Pipes (`|>`), UFCS (`x.method()`), string interpolation (`"${expr}"`) are desugared in almide-frontend's lowering pass. Codegen never sees these forms.

5. **Nanopass isolation.** Each codegen pass does one semantic transformation. The walker is target-agnostic — it never checks `if target == Rust`. Target differences are encoded in pass selection and TOML templates.

6. **String interning everywhere.** All identifiers, type names, field names are `Sym` (interned, `Copy`). Compare with `==`, not string matching. Use `almide_base::intern::sym()` to intern, `.as_str()` to read.

## Layout Discipline (new long-lived structures)

**Rule (#1316).** Every **new** long-lived data structure in the compile pipeline — a
module-cache format, the MIR→CLIF structures, any artifact that outlives one compile
step — is built as **flat arrays + `u32` ids + interned atoms**. Not `Box`/`Rc`-rich
trees, not `String` fields, not maps that must be rebuilt entry-by-entry on load. The
existing AST/IR are explicitly OUT of scope: this is a zero-retrofit policy that binds
code that does not exist yet.

Gated by `scripts/check-layout-discipline.sh` against `proofs/layout-ledger.toml`. A
type in a cache/CLIF module, or any type deriving `Serialize`/`Deserialize` outside the
frozen pre-existing files, needs a ledger row: `FLAT` (re-derived from source every run,
so the row cannot lie) or `DEVIATION` + a one-paragraph note saying what it costs and
what would remove it. The `DEVIATION` count is a shrink-only ceiling. **What the gate
cannot judge — SoA vs AoS, function bodies, type aliases — is listed in the script
header; that part is review, and the deviation note is where the reasoning is written
down.** Measurements below: 2026-08-13, `--release`, best-of-5, this tree.

1. **Interned atoms, not owned text.** Principle 6 above, held inside long-lived *rows*.
   `Option<Sym>` is 4 B (niche-packed) against 24 B for `Option<String>` — that single
   field is all of `VarInfo`'s 120 B → 96 B (−20%). `VarInfo.module_origin:
   Option<String>` holds a module *name* that is a `Sym` everywhere else in the IR; it
   got in because nothing asked. Cloning a 100k-row table with that field populated
   costs 1.95 ms owned against 0.57 ms interned (3.4×).

2. **Integer ids, not embedded trees** — so equality is an integer compare. `Ty`
   equality measures 4.33 ns/cmp for `Applied(UserDefined(String), [Int])` (the
   `TypeConstructorId::UserDefined` payload is an owned `String`, so equal user types
   are compared by `strcmp`) and 7.28 ns for a 3-level nested `Ty`, against 0.57 ns for
   a `u32`: 7.6× and 13×. `DefInfo` is four interned fields plus one `ty: Ty`, and that
   one field is why `DefTable` can never be loaded as bytes.

3. **Flat arrays, so loading is a read.** The IR of `spec/stdlib/stdlib-test.almd` is
   1.93 MB of JSON: `serde_json::from_str::<IrProgram>` against `fs::read` of the same
   bytes measured 4.269 ms vs 0.059 ms in one run and 6.648 ms vs 0.088 ms in another —
   72× and 76×. A pointer-rich cache pays that per module on every build; a flat one
   pays the read.

4. **SoA when a pass walks one column.** Summing `use_count` over 200k rows costs
   0.234 ms at `VarInfo`'s 120 B stride against 0.090 ms over a `Vec<u32>` column
   (2.6×). This is the clause the gate *cannot* enforce — `Vec<Row>` of flat rows passes
   and is array-of-structs. Decide it in review, and record the decision.

The shapes to copy already exist: `VarId`/`VarTable` and `DefId`/`DefTable` in
`almide-ir` (`u32` newtype + `Vec<Row>` + `get(id)`), and `Sym` in `almide-base`.

## When Adding a New Feature

- **New syntax** → almide-syntax (parser) → almide-frontend (checker + lowering) → almide-codegen (passes + templates)
- **New stdlib function** → pure-Almide impl in `stdlib/<module>[_<part>].almd`; register for WASM + interp in `almide-types/src/self_host_registry.rs` (`self_host_runtime()`); native intrinsics (only when needed) in `runtime/rs/src/<module>.rs` + `@intrinsic` declaration. To keep the 3-way oracle covering it (instead of skipping it), also add the glue to almide-interp's bridge — it is hand-maintained, NOT auto-generated. See [almide-interp/CLAUDE.md](./almide-interp/CLAUDE.md#coverage-model--does-a-new-stdlib-fn-get-covered-automatically).
- **New type** → almide-types (Ty variant) → almide-frontend (inference rules) → almide-ir (IR nodes) → almide-codegen (emission)
- **New codegen target** → almide-codegen (pass pipeline + TOML template + target entry)
