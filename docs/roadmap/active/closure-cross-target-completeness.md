<!-- description: Eliminate native/WASM closure-codegen divergence classes by construction, backed by a permanent CI differential gate -->
# Closure cross-target completeness

**Goal:** make native (Rust) and WASM agree on *every* closure program — not by passing more
adversarial sweeps (sampling can't prove absence) but by **eliminating the bug classes by
construction** and adding a **permanent CI differential gate** so the surface stays closed.

**Status (2026-07-19 audit):** of the original 5 classes, **3 are SHIPPED** (Class 1, Class 3,
Class 5 — cited below with landing commits) and **Class 4 collapsed into Class 1** at design time
(never its own fix). **Active scope is down to two items:** Class 2 (a WASM-only RC *leak*, not
unsoundness — confirmed still open by code inspection) and the CI-gate item (a *golden-output*
closure corpus is still missing from the cross-target gates).

## How we got here

Six adversarial native-vs-WASM closure sweeps (generate diverse closure programs, diff `almide run`
vs `wasmtime`). Sweeps 1–3 closed the `() -> Unit` surface (11 fixes, PRs #337–340). Sweeps 4–6
opened the **parametered-closure** dimension `(T) -> U`:

| PR | fixed |
|----|-------|
| #341 | fold ICE (HOF-lambda-param call result type) + WASM build-gate |
| #342 | field-type nested-Fn → `Rc<dyn Fn>` (recursive) |
| #343 | closure-returning-closure (`Box`→`Rc` + trait-object cast) |
| #344 | closure-valued `Map` (map.set / `??` / from_list / HOF / variant), 5/6 |

The **6th sweep** (authoritative — run against the merged, *stable* binary) found **12 real
divergences**, clustering into **5 bug classes** (below). Per-position patching never converges,
so the shape of the fix mattered as much as the fix itself — the classes below note which ones got
a *structural* by-construction fix vs. a targeted patch.

> **Sweep-binary trap (learned):** the 4th sweep was wasted because agents ran the stale `~/.local/bin`
> binary, not the freshly-built one. Always `cp target/release/almide ~/.local/bin/almide` AND have the
> sweep runbook assert `--version` before generating. Never rebuild *during* a sweep (churn → muddy results).

---

## The 5 classes — 3 shipped, 1 collapsed, 1 open

### Class 1 — boxing / type-unification — ✅ SHIPPED
**Was:** a stored closure stayed a concrete anonymous type, so a second closure (a `??` fallback, a
different `map.set`, a variant payload, an `if`/`match` branch) couldn't share its type (E0308 "two
closures can't unify", E0562 "impl Trait in field"), plus the a6 over-boxing regression and the sc4
stale-type edge.

**Shipped as the by-construction fix this doc called for:** `c685d4b4` ("Unify closure
representation to Rc\<dyn Fn\> via box-by-default") makes every closure value `Rc<dyn Fn>` in every
storage/unification slot by default, replacing the ~14 per-position boxing rules with one invariant
— exactly the `FnBoxingPass` / INV-BOX design this doc proposed. `619f8ccf` ("Make runtime HOFs
take `Rc<dyn Fn>` to drop the consumed-vs-value allow-list") completes it on the runtime side: data-
plane stdlib HOFs (list/map/set/option/result/bytes/matrix) now take `Rc<dyn Fn>` instead of
`impl Fn`, deleting the `keeps_closure_boxed` name-list and the `RuntimeCall` un-box arm. `fan`
correctly stays `impl Fn + Send + Sync` (a real exception, not a leftover).

### Class 2 — WASM mutable-capture cell: residual LEAK — still OPEN (confirmed)
**Status:** the **unsoundness/trap was already fixed** by closure-v2 P6 before this doc's last
revision; **what remains is a plain memory LEAK**, and it is still unfixed. Confirmed by code
inspection (2026-07-19): `emit_typed_cell_rc_dec` does not exist anywhere in the tree, and
`crates/almide-codegen/src/emit_wasm/statements.rs` (`IrStmtKind::RcDec` arm, ~line 411-421) still
does exactly what this doc described — a **plain** `rc_dec` on the cell itself, with the original
comment explaining why the *typed* dec was rejected (it walks the cell pointer as the object, so
`cell[0]` — the real inner object pointer — gets misread as an element count, corrupting/crashing
on `List[String]` while `List[Int]` happens to survive by accident). A plain `rc_dec` frees the
cell's own allocation but never drops the **inner heap referent** at `cell[0]`, so a captured
`List[String]`/`Map`/record leaks when the cell dies. WASM-only (Rust uses `Rc<Cell>`/
`Rc<RefCell>` → RAII, see [Closure Architecture v2](closure-architecture-v2.md) P3/P6).

**Fix — still as designed (by construction): typed cell destructor.**
- Replace the plain `rc_dec` with `emit_typed_cell_rc_dec(inner_ty, cell_ptr)`: decrement the
  cell's RC; **when it reaches 0**, if `is_heap_type(inner_ty)` load `inner = cell[0]` and call the
  *existing* `emit_typed_rc_dec(inner_ty, inner)` (reuse, don't reimplement — same recursive
  child-dropper, correctly rooted at the inner ptr this time) — then free the cell. Copy inner
  types need no inner free (today's plain `rc_dec` is already correct for them).
- Capture already does a *plain* `rc_inc` on the **cell** (inner RC untouched, owned once by the
  cell), so this closes the leak under the `rc(cell)==0` gate without touching the inc side.
- **Effort: M** (small, reuses tested code). Optional Lean proof of "cell drop frees the inner
  exactly once" remains net-new (`ClosureRc.lean` covers the P5 env-capture contract but not this
  cell-destructor path) — **+L if the Lean proof is in scope.**

### Class 3 — in-place rewrite on a non-owned lvalue (SILENT-WRONG) — ✅ SHIPPED
**Was never actually a closure bug** — `var g: List[Int] = []; g = g + [n]` printed `len = 0` on
**both** Rust and WASM, silently, and reproduced with no closure at all. Root cause:
`try_rewrite_push` rewrote `xs = xs + [v]` → `xs.push(v)` based only on lvalue *aliasing*, and its
existing `shared.contains(&var)` bail (added for closure cells) didn't cover module-level globals,
so the peephole fired for them and the write-back silently vanished.

**Shipped:** `155c947e` ("Fix module-var self-append write loss and cross-module spread base
typing") — the guard now covers module-global / cross-module lvalues, restoring the correct
`Assign`/write-back path. Confirmed the fix targets exactly this file/mechanism
(`pass_concretize_types.rs`, `pass_rust_lowering.rs`, `canonicalize/registration.rs`) plus a
regression (`spec/lang/module_var_test.almd`, `cross_module_toplet_byvalue_test.almd`).

### Class 4 — WASM closure-env width — COLLAPSED into Class 1, never a separate fix
The "captured Int stored as i32 / env_ptr garbage" hypothesis was falsified at design time: a
closure value is a single i32 pointer to a heap `[table_idx:i32][env_ptr:i32]` pair, and store/load
were already symmetric i32. The vp3 sweep failure this class chased was the curried-closure
non-Clone issue — i.e. Class 1 territory — and Class 1 shipping (`c685d4b4`/`619f8ccf`) absorbed
it. The only residual is hardening, not a bug: give `Ty::Fn` an explicit `byte_size`/`ty_to_valtype`
arm + a `debug_assert` that env stride ≥ `byte_size(Fn)`, so a future closure-representation change
trips a test instead of silently corrupting layout. Not verified as landed; low priority (no known
live failure), track opportunistically rather than as an open item.

### Class 5 — enum-derive through closure-bearing types (vp5) — ✅ SHIPPED
**Was:** `type Tree = Leaf | Node(Lazy[Int])` where `Lazy` carries a `() -> T` field made `Tree`
`#[derive(Clone, Debug, PartialEq)]`, but `Lazy<i64>` is neither `Debug` nor `PartialEq` → E0277 +
E0369. Root cause: derive selection was asymmetric — the record branch of `render_type_decl`
computed `has_fn_fields`/`has_non_eq_fields`, but the enum branch computed neither.

**Shipped:** `446f7bb8` ("Box closures in variant payloads and function-typed closure parameters")
adds the missing `has_fn_fields` computation to the enum branch
(`crates/almide-codegen/src/walker/declarations.rs`) and selects a Clone-only derive
(`#[derive(Clone)]`) when a variant carries an `Fn` payload — mirroring the record branch. Landed
as a direct fix rather than the fully generalized `select_derives(...)` shared-helper this doc
originally proposed; functionally equivalent for the symptom, with less structural insurance
against the two branches drifting apart again in the future.

---

## The gate — still missing a golden-output closure corpus

**Confirmed still absent (2026-07-19):** `.github/workflows/ci-cross-target.yml` runs `almide test`
over every `spec/**/*_test.almd` on both targets and fails on any pass/fail *divergence* — but it
does not touch `spec/wasm_cross/`, and it never compares output *content* (Class 3's whole point
was that native==wasm equality isn't enough — both targets were silently wrong at the same value).

Separately, `tests/wasm_runtime_test.rs::wasm_cross_target_spec` (a `cargo test`, not part of
`ci-cross-target.yml`) *does* run every `spec/wasm_cross/*.almd` fixture on both targets and asserts
byte-identical stdout — and 5 closure-shaped fixtures already live there (`closure_accumulator`,
`closures_and_variants`, `closures_hof`, `call_closure_lambda_param`, `hof_closure_string_tail`).
But every one of them asserts native==wasm *equality* only; none carries an independently-known
**golden** expected value in the fixture itself. So the exact gap this doc originally called out is
still real, just narrower: there is a cross-target equality gate for closures, but no golden-output
one, and the sweep corpus (`/tmp/sweep4_*` / `/tmp/cbox/*`) that motivated this section was never
committed.

**Fix (S/M, unchanged from the original plan):** commit the sweep corpus under `spec/wasm_cross/`
(or a `spec/closure_xt/` subset) with hand-verified **golden** expected output baked into each
fixture (a `test` block assertion, not just a `println`), so a future bug that manages to be wrong
identically on both targets still fails CI.

---

## Sequencing (remaining work only)

1. **Class 2 fix** (M, by construction) — the typed cell destructor. Self-contained, reuses
   `emit_typed_rc_dec`, no dependency on anything else in this doc.
2. **Golden-output gate** (S/M) — commit the sweep corpus with golden assertions; extend either
   `ci-cross-target.yml` or the existing `wasm_cross_target_spec` cargo test to fail on
   `!= golden` in addition to `native != wasm`.
3. **(optional) Class 4 hardening** — the `byte_size(Ty::Fn)` + env-stride `debug_assert`; no known
   live bug motivates urgency.
4. **(optional) Lean proof** for the Class 2 cell destructor once it lands — machine-checked memory
   completeness, mirroring `ClosureRc.lean`'s P5 env-capture proof.

Each step: verify spec 240×2 targets + full `cargo test` + the closure regression suite + the
curated corpus diff (once committed).

## Definition of DONE (completeness)

The closure native↔WASM surface is **complete** when: (a) Class 2 is eliminated by construction (no
leak on cell death, verified via `emit_typed_cell_rc_dec` reusing the existing typed dropper); (b)
the golden-output closure corpus is committed and gates CI, failing on any `native != wasm` OR
`!= golden`; (c) Class 4's layout assertion lands (hardening, non-blocking); (d) a fresh adversarial
sweep against current `develop` returns **0 real divergences**. Classes 1, 3, and 5 are already
closed and don't need re-verification beyond their existing regression tests.

> Live design detail for the original Refactor 1/2 proposals: workflow `wj7yrlb5h`. For Classes
> 3/4/5 + the gate: `wf23aws8j`. See memory `project_closure_parametered_dimension.md` for the
> per-form boxing notes and the sc4 edge (now resolved by Class 1's shipped fix).
