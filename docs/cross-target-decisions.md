# Cross-target decisions: which ones read one source

A **decision** is one semantic question the compiler answers about a program. Examples: "does this call write its argument?", "may this expression run later?", "may these pipeline stages run element by element?".

Both targets must answer each question the same way. When the native leg and the wasm leg answer it from **different sources of truth**, the answers can disagree. The two legs then run the same program differently. Usually neither leg's own tests notice, because each leg is self-consistent.

The class has a common shape: a table that exists on one leg and not the other. Three examples:

- **The lowered module set.** Native lowers every resolved bundled stdlib module into the IR. The wasm leg (`src/wasm_leg.rs`) skips every bundled module that has a bodyless `= _` surface: `list`, `map`, `string`, `bytes`, …
- **The implementation behind a stdlib call.** Native calls a Rust runtime body. Wasm links a self-hosted implementation from the registry.
- **A hand-maintained list** that duplicates what a declaration already says.

This page is the inventory from the 2026-09-29 audit. The rule it records is that a question gets **one** answer function, and every leg calls it.

## The inventory

| Question | Native read | Wasm read | One source now | Gate |
|---|---|---|---|---|
| Which args does a call write? (branch lift declines to outline a branch that writes an outer var) | `IrFunction::mutated_params` of the lowered modules, keyed by **bare** name, so a module fn overwrote a same-named user fn (#2948) | the same table, but without the bundled modules (#2931), later with a declaration fallback | `almide_ir::mut_args`: a stdlib fn answers from its declaration on every leg; a user fn answers from its own scope | `branch_lift::tests::every_stdlib_mut_fn_declines_the_lift_whether_or_not_its_module_is_lowered`, enumerated from the declarations, with the lowered copy's table deliberately wrong |
| Which args does a registry-linked stdlib call write? (copy-on-write at the call site) | runtime `&mut` table (`generated/runtime_fn_modes.rs`), then the declaration | the **linked implementation's** params, which carry no `mut` (#2949) | the surface declaration, `almide_ir::mut_args` | `declared_mut_agrees_with_runtime` (native table == declaration, every `@intrinsic`) |
| Does a captured var need shared storage? (C-319) | `Mutability::Var`, or the closure body's `&mut` borrows | a hand list of six bytes writers plus `Assign` (#2951) | `almide_ir::mut_args` for every stdlib call | `cells::tests::every_stdlib_mut_write_to_a_captured_var_makes_a_cell` |
| Is this `var` ever written? (`var`→`let` demotion) | shared lowering: `Assign`-family statements only, so a var written only through `list.push(xs, ..)` became `let` and native snapshotted it into a closure (#2952) | the same, but wasm's cell scan counted the call | a var handed to a declared `mut` param counts as written | `use_count::mut_arg_write_tests`, enumerated |
| May this expression run earlier, later, twice, or not at all? | LICM: its own `binop_may_trap` | the small-scalar-fn inliner: "call-free" only, which drops a trapping argument (#2947) | `almide_ir::speculation` | unit tests; C-001 fixture |
| When does a top-level `let`'s initializer run? (C-007) | lazy, forced at startup only for a **direct** `/`/`%` (#2954) | always at startup | forced iff not speculation-safe (`almide_ir::speculation`) | C-007 fixture |
| May a pipeline's stages run element by element? (stream fusion) | IR body purity plus `EFFECT_MODULES`/`TOTAL_MODULES` plus "at most one aborting stage" | its own 7-module denylist and no abort rule, so a user-module callback's output interleaved (#2953) | `almide_ir::fusion`: module vocabulary, trap rule, stage-sequencing rule | unit tests; C-001 fixture |

## Checked and consistent

Each of these was probed or read, and found to answer from the same source on both legs, or to be consistent for another reason.

- **The shared optimizer:** fold, DCE, propagate, the unsigned re-fold, optional-chain desugar. These are syntactic and read no per-leg table. DCE's `is_pure` treats a trapping `BinOp` as pure. That is the same on every leg, so an unused `let q = a / 0` is dropped everywhere. It is a language question, not a divergence.
- **`mono` / `ir_link` / `top_let_storage::global_init_order`.** They read `program.modules`, but no answer they give depends on a bundled module being present.
- **`mutual_tco` / `accum_tre`.** They refuse a fn with a `mut` param from `IrParam::is_mut`, which is the same IR on both legs.
- **`fan`.** Native threads a `fan { a; b }` block; wasm runs it sequentially. This is the documented C-004 interleaving exception.
- **LICM hoisting a non-terminating call above a zero-trip loop.** Probed: both legs agree.
- **The MIR incumbent's linearized `if`** (`almide-mir/src/lower/control.rs`, call-free arms run both ways). Probed with trapping arms: both legs agree. The incumbent is being retired (#2930 / #2935).

## Known, tracked elsewhere

- **#2503.** An effect callee is excluded from wasm's call-site copy-on-write, and so is a mut argument that is a record field (`param_mut` requires `!is_effect`; `lower_mut_param_arg` takes only a `Var`). Native clones at bind in both cases.
- **`almide-mir/src/purity.rs`.** Its `PURE_MODULES` is the capability-witness admit list that the MIR lowering and the inliner read. It is a third module vocabulary: `zlib` is pure there but an effect for fusion. Today it only admits scalar-returning stdlib calls into inlined bodies, which stay calls, so no divergence was found. It should converge on `almide_ir::fusion`'s vocabulary.

## When you add a decision

If a new pass asks one of the questions above, call the answer function; do not re-derive the answer. If it asks a new question, answer it from what the **program says**: the declaration, the checked signature, or the IR node. Do not answer it from what a leg happened to lower or link. Then add a row here, with a gate that enumerates the registry rather than a hand list.
