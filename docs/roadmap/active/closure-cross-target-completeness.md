<!-- description: Eliminate native/WASM closure-codegen divergence classes by construction, backed by a permanent CI differential gate -->
# Closure cross-target completeness

**Goal:** make native (Rust) and WASM agree on *every* closure program — not by passing more
adversarial sweeps (sampling can't prove absence) but by **eliminating the bug classes by
construction** and adding a **permanent CI differential gate** so the surface stays closed.

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

The **6th sweep** (authoritative — run against the merged, *stable* binary) still found **12 real
divergences**, which is the point: per-position patching never converges. They cluster into **5 bug
classes** (below). Two of them want a *structural* fix that makes the whole class impossible; three
are targeted. A CI gate makes it permanent.

> **Sweep-binary trap (learned):** the 4th sweep was wasted because agents ran the stale `~/.local/bin`
> binary, not the freshly-built one. Always `cp target/release/almide ~/.local/bin/almide` AND have the
> sweep runbook assert `--version` before generating. Never rebuild *during* a sweep (churn → muddy results).

---

## The 5 classes + the gate

### Class 1 — boxing / type-unification (E0308 "two closures can't unify", E0562 "impl Trait in field")
**Symptom:** a stored closure stays a concrete anonymous type, so a second closure (a `??` fallback,
a different `map.set`, a variant payload, an `if`/`match` branch) can't share its type. Also the
**a6 regression** (#5 over-boxed a Computed-call arg → `Rc<Rc<dyn Fn>>`) and the open **sc4** edge
(let-bound closure whose `Ty::Fn` is unresolved at the current box-pass timing).

**Fix — Refactor 1 (by construction): `FnBoxingPass` + one invariant.**
> **INV-BOX.** A closure value (`Ty::Fn`) is `Rc<dyn Fn>` in *every storage/unification slot*, and
> raw `move ||` *only* in a closed, enumerated set of three: (a) directly called here, (b) passed
> by value to an `impl Fn` parameter, (c) a `fan.*` Send+Sync position.

- One classifier `classify_fn_site(parent, slot) -> {Boxed, RawCall, RawImplFn, RawSendSync}`. `Boxed`
  is the default; the three `Raw` cases are the *only* exceptions. This collapses the ~14 per-position
  IR rules + 5 walker rules in `pass_rust_lowering.rs`/`walker/` into one decision.
- **New pass `FnBoxingPass`**, re-anchored: **after** `ConcretizeTypes` (so every `Ty::Fn` is final →
  kills sc4's stale-type failure) and **before** `CloneInsertion` (so a boxed closure's clone lowers
  to cheap `Rc::clone`). This re-ordering is the substantive change.
- Type renderer: collapse `render_type` (impl Fn) / `render_type_field_fn` / `render_type_rc_fn` so
  "storage ⇒ `Rc<dyn Fn>`" is one rule; a `let`-bound closure renders the concrete `Rc<dyn Fn>` (not
  `_`) so call sites are unambiguous (fixes sc4 + a6 by construction: box-if-not-already is implicit
  in INV-BOX because a value already `Rc` is at a `Boxed` site, not re-wrapped).
- **Top risk:** the `Raw` set must be *exhaustive* — a false `Boxed` at a direct-call/HOF/fan site
  gives `Rc<dyn Fn>` where Rust wants `impl Fn` (E0277/E0631); a false `Raw` re-introduces E0308.
  Tested by: all #337–344 regression tests + spec 240×2 targets + the curated sweep corpus (gate).
- **Subsumes:** every `box_node`/`box_fn_in_value` rule, sc4, a6. **Effort: L.**

### Class 2 — WASM mutable-capture cell: residual LEAK *(unsoundness already fixed)*
**Status (code-verified, corrected):** the **unsoundness/trap is already fixed** — closure-v2 **P6**.
`RcDec{var}` for a mutable-capture cell (`emit_wasm/statements.rs:342-358`) now does a **plain
`rc_dec` on the cell** (line 352), explicitly *not* `emit_typed_rc_dec(inner_ty, cell_ptr)`; the code
comment documents exactly why (the typed dec walked the cell ptr as the object — `cell[0]` read as the
element count → element-drop loop decref'd garbage → `List[String]` trapped, `List[Int]` survived).
**What remains is a LEAK only:** a plain `rc_dec` frees the *cell's own* allocation but never drops the
**inner heap referent** at `cell[0]`, so a captured `List[String]`/`Map`/record leaks when the cell
dies. WASM-only (Rust uses `RcCow`/`thread_local RefCell<Rc<T>>` → RAII).

**Fix — Refactor 2 (by construction): typed cell destructor (closes the leak).**
- Replace the plain `rc_dec` (line 352) with `emit_typed_cell_rc_dec(inner_ty, cell_ptr)`: decrement
  the cell's RC; **when it reaches 0**, if `is_heap_type(inner_ty)` load `inner = cell[0]` and call the
  *existing* `emit_typed_rc_dec(inner_ty, inner)` — **reuse, don't reimplement** (same recursive
  child-dropper, now correctly rooted at the inner ptr) — then free the cell. Copy inner types need no
  inner free (today's plain `rc_dec` is already correct for them).
- Correct by construction: capture does a *plain* `rc_inc` on the **cell** (inner RC untouched, owned
  once by the cell), so the inner is freed exactly once under the `rc(cell)==0` gate.
- **Lean note (MAP-DRIFT A):** `ClosureRc.lean` does **not** exist; the belt is `Basic/FnBody/Heap.lean`.
  The proof that "cell drop frees the inner exactly once (inc==dec, no leak)" is **net-new**, not an
  extension. Optional but is the machine-checked completeness for the memory model.
- **Effort: M** (the fix is small + reuses tested code); **+L if the Lean proof is in scope.**

### Class 3 — in-place rewrite on a non-owned lvalue (SILENT-WRONG) 🔴  *(re-diagnosed)*
**NOT a closure bug.** `var g: List[Int] = []; g = g + [n]` prints `len = 0` on **both** Rust and
WASM, compiles and runs clean — and reproduces **without any closure** (`g = g + [1]` at top level of
`main`). Only `g = g + [single_literal]` is hit; `g = [1,2,3]` and `g = g + other_list` are fine. The
closure framing from the sweep was a red herring (it just made the global mutation conspicuous).

**Root cause:** `try_rewrite_push` (`pass_rust_lowering.rs:497`) rewrites `xs = xs + [v]` →
`xs.push(v)` (a Method-call) **based only on lvalue *aliasing*, not storage class**. It *already*
bails for shared closure cells — line 500: `if shared.contains(&var) { return None; }`, comment:
"rewriting to `xs.push(v)` would push onto a discarded clone" — **but a module-level global is not in
the `shared` set**, so the peephole fires for it. The method object then renders (walker
`expressions.rs`, which only special-cases `is_rc_cow`, not `ModuleRc`) as a *read-only clone*
`G.with(|c| (**c.borrow()).clone()).push(v)` on a discarded temporary → the write-back is lost. WASM
has an **independent sibling** bug in its `Assign`-to-heap-global path (`emit_wasm/statements.rs`) plus
a silent `drop` fallthrough that swallows it. *(Code-verified root cause; the "both targets, no
closure needed" reproduction breadth is per design workflow `wf23aws8j`'s empirical run.)*

**Fix — the guard already exists; it just doesn't cover globals:**
- **Primary (S):** **extend the existing `shared.contains(&var)` bail** at line 500 to also return
  `None` when `var` is a module-global / `ModuleRc` lvalue (thread the module-origin set in alongside
  `shared`), so the original `Assign` takes the correct `ModuleRc` write-back path. This is a few
  lines, not a new mechanism — the bail pattern is right there.
- **Defense (M):** extend the walker's mutating-method write-back (`expressions.rs:961-980`) to handle
  `VarStorage::ModuleRc` (mirror the `is_rc_cow` arm at 1206) — closes the class for *any* method-call
  source. Fix the WASM heap-global `global_set` write-back, and **replace both silent `drop`
  fallthroughs with a hard `unreachable` + emit-time diagnostic** (silent-discard is the amplifier).
- **By construction:** add an `LvalueClass {OwnedLocal, ModuleCell, ModuleRc, RcCow}` query usable by
  *every* in-place rewrite (push/index-assign/swap/reverse) so no future peephole can silently target
  a non-owned lvalue. **Effort: M.** Highest priority of the targeted items — silent-wrong, and it's a
  **general correctness bug**, not closure-specific.

### Class 4 — WASM closure-env width  *(mostly collapses — NOT a separate bug)*
The "captured Int stored as i32 / env_ptr garbage" hypothesis was **falsified**: a closure value is a
single i32 pointer to a heap `[table_idx:i32][env_ptr:i32]` pair; env store (`equality.rs:561`) and
load (`expressions.rs:141-157`) are symmetric i32 — correct. The vp3 sweep failure is the
**curried-closure non-Clone** issue, i.e. **Class 1** (Refactor 1) territory, not an env-width bug.
Only a *latent* inconsistency remains: `byte_size(Ty::Fn)=4` (`values.rs:49`) vs the hard-coded 8-byte
env stride — currently correct-by-accident. **Fix (S, hardening):** give `Ty::Fn` an explicit
`byte_size`/`ty_to_valtype` arm + a `debug_assert` that env stride ≥ `byte_size(Fn)`, so any future
closure-representation change trips a test instead of corrupting layout. *(One open item: reconcile
vp3 — the sweep saw native-OK/WASM-validator-fail, the design says it's the Class-1 boxing path; verify
after Refactor 1 lands whether vp3 is fully absorbed.)*

### Class 5 — enum-derive through closure-bearing types (vp5)  *(confirmed)*
**Symptom:** `type Tree = Leaf | Node(Lazy[Int])` where `Lazy` carries a `() -> T` field → `Tree`
gets `#[derive(Clone, Debug, PartialEq)]` but `Lazy<i64>` is neither `Debug` nor `PartialEq` →
E0277 + E0369.
**Root cause:** derive selection is **asymmetric** — `render_type_decl` computes `has_fn_fields`/
`has_non_eq_fields` for **records** (`declarations.rs:30,34`) but the **enum branch (66-124) computes
neither** (only `has_hash`), and `enum_decl` (`rust.toml:317-326`) has no Clone-only / no-PartialEq
variants. The transitive `eq_blocked_types` fixed-point (`compute_eq_blocked_types`, declarations.rs:373)
**already handles enums correctly** — the consumer just never reads it.
**Fix (S/M):** in the enum branch compute `has_fn_fields`/`has_non_eq_fields` mirroring the record
branch + push to `enum_attrs`; add `when_attr="has_fn_fields"` (Clone-only) and `"has_non_eq_fields"`
(Clone+Debug) variants to `enum_decl`. **By construction:** extract one `select_derives(field_tys,
eq_blocked, has_hash, repr_c) -> attrs` shared by struct and enum so they can't drift again. No
regression risk (an enum transitively containing `Fn` that relied on `PartialEq` is unsound today —
it doesn't compile).

### The gate — extend the EXISTING cross-target CI  *(already partly there)*
`.github/workflows/ci-cross-target.yml` **already exists** but only checks pass/fail divergence, **not
output correctness**, and the `wasm_cross_target_spec` test diffs native-vs-wasm for `spec/wasm_cross/*`.
**Fix (S/M):** commit the full sweep corpus (`/tmp/sweep4_*` + `/tmp/cbox/*`) under `spec/wasm_cross/`
(or `spec/closure_xt/`) with **golden expected output** (Class 3 showed cross-target *equality* isn't
enough — both targets were wrong at the same value), and extend `ci-cross-target.yml` to fail on any
`native != wasm` OR `!= golden`. Converts "we patched it" into "it can't regress".

---

> **Design-phase refinement (what the two design workflows changed):** the 6th sweep's "5 classes"
> got *smaller* under analysis, not bigger. **Class 3 is a general correctness bug**, not closure-
> specific (a peephole ignoring lvalue storage class — reproduces with no closure, on both targets).
> **Class 4 mostly collapses into Class 1** (the closure-env representation is correct; only a latent
> layout assertion remains). And the **CI gate already exists** — it only needs an output-correctness
> corpus. So the real work is **2 refactors + 2 small targeted fixes + 1 gate extension**, not 6
> independent unknowns.

## Sequencing

1. **Class 3** (S primary / M full, silent-wrong) — **first**: it's a *general* correctness bug
   (in-place rewrite on a non-owned lvalue), independent of all closure work, and silent-wrong is the
   most dangerous mode. The S "peephole storage guard" lands immediately; the M `LvalueClass` +
   remove-silent-`drop` hardening can follow.
2. **Class 5** (S/M, independent) — quick win, unblocks variant/enum closure shapes (shared
   `select_derives` is the by-construction form).
3. **Refactor 1 / Class 1** (L, by construction) — the headline; absorbs sc4 + a6 + all boxing edges
   **and the vp3/Class-4 curried-closure failure**. Must land with the curated corpus gate so the
   14-rule deletion is safe.
4. **Refactor 2 / Class 2** (M, by construction) — WASM cell unsoundness+leak; independent of 1.
5. **Class 4 residue** (S, hardening) — after Refactor 1, confirm vp3 is absorbed; add the
   `byte_size(Ty::Fn)` arm + env-stride `debug_assert`. No live patch expected.
6. **CI gate** (S/M) — *extend the existing* `ci-cross-target.yml`: add the sweep corpus with golden
   output, fail on `native != wasm` OR `!= golden`. Land corpus incrementally as each class closes.
7. **(optional) Lean proof** for the cell destructor — machine-checked memory completeness.

Each step: verify spec 240 ×2 targets + full `cargo test` + the closure regression suite + the
curated corpus diff. One PR per step (the established loop: root-cause → fix → verify → PR → CI → merge).

## Definition of DONE (completeness)
The closure native↔WASM surface is **complete** when: (a) Classes 1 and 2 are eliminated *by
construction* (INV-BOX + typed cell destructor — no per-position rule, no leak, no unsoundness);
(b) Classes 3 and 5 are fixed and covered, and Class 4 reduced to a passing layout assertion;
(c) the extended `ci-cross-target.yml` runs the full sweep corpus with golden output on every build
and fails on any `native != wasm` OR `!= golden`; (d) a fresh adversarial sweep returns **0 real
divergences**. Effort is **far** (2 big refactors + 2 small fixes + gate → multiple focused sessions)
but the **uncertainty is gone** — this is now a known, finite list, not a fractal of new edges.

> Live design detail for Refactor 1 + 2: workflow `wj7yrlb5h`. For Classes 3/4/5 + the gate: `wf23aws8j`.
> See memory `project_closure_parametered_dimension.md` for the per-form boxing notes and the sc4 edge.
