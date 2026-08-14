> Last updated: 2026-08-15

# Edit Locality

The invariant behind Almide's mission metric. MSR (modification survival
rate) measures, empirically, whether an LLM's edit survives; this document
states the semantic property that makes edits survivable, maps every
existing language rule to the role it plays in enforcing it, and records —
with `file:line` evidence — where the current implementation still violates
it. The research program built on top of this statement (Lean kernel,
backend refinement, MSR prediction loop) lives in
`docs/roadmap/active/edit-locality-theory.md`; this file contains only the
present-tense truth.

The observable set is fixed once, by the contract ledger
(`docs/contracts/contracts.toml`): **stdout bytes, stderr bytes, exit
code**. Nothing below ever means anything else by "observable".

## 1. The invariant

**L1 — Edit Frame.** Let `P` be a well-typed program and `e` an edit to the
body of one definition `f` that preserves `f`'s signature: name, parameter
types, return type, the `effect` marker, and visibility. Then every
execution of `P[e]` whose trace never enters `f` has observables identical
to the same execution of `P`.

**L2 — Cross-target agreement.** Observables are a function of the source
alone, not the target: native and wasm agree byte-for-byte. This is the
contract ledger's standing promise (280 contracts, `spec/wasm_cross/`
fixtures, the differential fuzz); L1 quantifies over "the observables" only
because L2 makes that phrase well-defined.

**L3 — Diagnostic locality.** A signature-preserving edit that makes the
program ill-typed produces diagnostics anchored inside the edited
definition, not in unedited code. (E006 fires inside the pure `fn` that
tried to call an effect; E041/E042 fire at the offending statement in the
edited body.)

L1 is the load-bearing one. Almost no implemented language satisfies it:
overload resolution, implicit instances, macros, whole-program inference,
and dynamically scoped effect handlers each let a distant edit change the
meaning of unedited code. Almide's rules, adopted one-by-one for MSR
reasons, are exactly L1's preconditions — the table in §2 is that claim
made checkable.

## 2. What enforces it today

| Rule | Locality role | Evidence |
|---|---|---|
| Return types on `fn`/`effect fn` are syntactically mandatory (hard parse error, no `Option<TypeExpr>` in the AST) | The signature callers see is text, not an inference result — a body edit cannot change it | `crates/almide-syntax/src/parser/fn_decls.rs:69-82`, `crates/almide-syntax/src/ast.rs:401` |
| `effect` is part of the signature; the checker reads `sig.is_effect`, never the body | Effects are bounded by "signature-preserving"; a pure `fn` cannot produce observables | `crates/almide-frontend/src/check/calls.rs:381-408` — spec: `effect-system.md` |
| Explicit propagation only (ADR-0008): E041/E042 are driven by the callee's **declared** `Result` return type | A callee gaining or losing error paths cannot silently rewire caller control flow; every propagation point is visible in the caller's own text (`!`) | `crates/almide-frontend/src/check/post_solve_validation.rs:399-434` — tests: `tests/expr_stmt_diag_test.rs`, `spec/wasm_cross/expr_stmt_comment.almd` (C-252) |
| No glob imports; the auto-import set is a compile-time constant | Adding a definition to a module cannot capture an existing unqualified reference elsewhere | `crates/almide-syntax/src/parser/declarations.rs:75-125`, `AUTO_IMPORT_BUNDLED` in `crates/almide-types/src/stdlib_info.rs:39-56` — spec: `module-system.md` |
| Cross-module access is qualified-only, no transitive access | A reference names its module in place; resolution cannot drift with distant edits | `crates/almide-frontend/src/import_table.rs:23-25` — tests: `spec/integration/modules/` |
| No overloading, no user-defined operators, no implicit conversions, no instance declarations; protocol conformance is declared (`deriving`), lookup is a fixed convention | There is no dispatch whose winner depends on non-local information | `docs/design/DESIGN.md:27`, `crates/almide-frontend/src/canonicalize/registration_validate.rs:237-265`, E012 (`docs/diagnostics/E012.md`) |
| Fixed UFCS ladder, stdlib-first | A new user `fn len` cannot steal `xs.len()` | `crates/almide-frontend/src/check/calls_ufcs.rs:36-67` — test: `spec/lang/user_fn_namespace_collision_test.almd` |
| Type inference is module-isolated; call return types come from declared signatures; monomorphization is keyed by call-site argument types only | No constraint crosses a module boundary; callee bodies are never consulted for callers' types | `crates/almide-frontend/src/check/module_inference.rs:16-19`, `check/calls.rs:276-310`, `crates/almide-optimize/src/mono/discovery.rs:28-42` |
| Stdlib purity is a hardcoded, machine-gated registry — never inferred from bodies | Optimization consumers see a fixed answer regardless of stdlib body edits | `crates/almide-mir/src/purity.rs:51,303-306`, gate `proofs/check-stdlib-purity-registry.sh` |

## 3. Where it breaks today (hunt of 2026-08-15)

Every finding below is a fact about the current compiler, with the
mechanism cited. Triage: **V** = language/compiler bug, L1 must hold, fix
it; **R** = backend refinement obligation (Stage 3 of the roadmap); **S** =
side condition, declared in §4 instead of fixed.

### V1 — LICM speculatively executes body-inferred-"pure" partial ops (live cross-target divergence)

`pass_licm_purity.rs` infers purity **from bodies** by whole-program
fixpoint (`crates/almide-codegen/src/pass_licm_purity.rs:243-276`) and
classifies partial operations — `xs[0]`, division, ranges — as pure
(`:289-290`, `:307-310`). LICM then hoists such calls above the loop
unconditionally (`crates/almide-codegen/src/pass_licm.rs:116-121`),
including out of zero-trip loops. Shipped regression almide#846 was this
shape (`pass_licm.rs:63-76`).

Reproduced on `almide 0.57.0` (2026-08-15). This program:

```almide
fn risky(xs: List[Int]) -> Int = xs[0] * 2

effect fn main() -> Unit = {
  let empty: List[Int] = []
  var acc = 0
  for _x in empty {
    acc = acc + risky(empty)
  }
  println("ok: ${acc}")
}
```

must print `ok: 0` and exit 0 — the loop is zero-trip, `risky` is never
called. Observed: **native prints `Error: index out of bounds` and exits
1; wasm prints `ok: 0` and exits 0.** One source, two behaviors — this is
simultaneously an L1 violation (a body edit that flips `risky`'s inferred
purity toggles the hoist for callers that never execute it) and a
cross-target contract violation that `spec/wasm_cross/` and the
differential fuzz had not caught. The fix must either guard hoists of
possibly-trapping ops or restrict the pure set to total operations; the
repro above then lands as a `spec/wasm_cross/` fixture with its own
contract.

### V2 — Checker and lowering disagree on local `fn` vs selective import

The checker resolves a bare call against the local `fn` first
(`crates/almide-frontend/src/check/calls.rs:290-310`); lowering resolves
the selective import first
(`crates/almide-frontend/src/lower/calls_target.rs:43-47`); and
registration has no collision diagnostic between the two
(`crates/almide-frontend/src/canonicalize/registration.rs:551-553`). In a
file with `import json.{parse}` (pattern:
`spec/lang/selective_import_test.almd`), adding `fn parse(...)` makes
existing `parse(x)` calls type-check against the new local function while
still executing `json.parse`. Adding a definition silently changed the
meaning of an existing reference — with a checker/codegen split. Fix: a
hard collision error (the E012 family).

### V3 — Constructor resolution is bare-name, first-registered-wins

`lookup_ctor` returns `cands.first()` in registration order
(`crates/almide-frontend/src/type_env.rs:412-414`), and the E019 ambiguity
guard is skipped when the current module owns a candidate
(`crates/almide-frontend/src/check/calls.rs:64-66`). Adding a same-named
variant re-binds existing constructor references instead of erroring.

### R — Backend refinement obligations

These live below the surface language; "backends as refinements" (roadmap
Stage 3) must discharge them. All are observable today in the build-failure
or wrong-code sense:

| Finding | Mechanism | Evidence |
|---|---|---|
| Self-host splice links whole source files, transitively, to fixpoint — editing stdlib fn `A` pulls new files into programs that only call its file-mate `B` | `link_self_host_runtime_to_fixpoint` | `crates/almide-mir/src/pipeline_link.rs:229-262` |
| Splice link collisions are a hard compile error — a stdlib body edit can break programs that never call the edited fn (real instance: `__hex_fill`, almide#1068) | `dedup_linked_by_name` | `crates/almide-mir/src/pipeline_link.rs:184-222` |
| Global impl-name→call-name rewrite captures user identifiers (a user `fn math_abs` gets redirected to `math.abs`) | `rewrite_impl_names_to_call_names` over **all** MIR functions | `crates/almide-mir/src/pipeline_link.rs:272-286` |
| Borrow inference is a whole-program fixpoint over bodies, capped at 6 rounds with no convergence check; a body edit (e.g. making a fn tail-recursive → `tco_owned_params`) rewrites every call site's `Borrow` wrapping, and past the cap results are iteration-order-dependent | `infer_borrow_signatures` | `crates/almide-codegen/src/pass_borrow_inference.rs:135-146,390-416`, `pass_borrow_inference_call_sites.rs:3-18` |
| `inline_pure_call_globals` decides by transitive body purity whether a top-level `let X = f()` initializer is substituted to use sites — moving a potential trap from "always at startup" to "only if read" | `expr_is_pure` recursing into callee bodies | `crates/almide-mir/src/lower/newtype_erase.rs:385-462,517-521,643-656` |
| Adding/removing a `test` block flips `library_mode`, changing wasm DCE roots program-wide | `reachable_fn_names` | `crates/almide-codegen/src/reachability.rs:168-183` |
| `Method`/`Computed` call targets contribute no reachability name — a body edit that removes the last `Named` reference to a UFCS-reached fn falsely prunes it (wasm trap, per the pass's own soundness note) | reachability collection | `crates/almide-codegen/src/reachability.rs:15-17,66-72` |
| Latent: whole-program effect inference from bodies exists (analysis-only today); the moment a diagnostic or permission consumes it, body edits change caller-visible output | `pass_effect_inference` | `crates/almide-codegen/src/pass_effect_inference.rs:12,47-70` |

## 4. Side conditions

L1 as stated in §1 holds **modulo** the following declared exceptions. An
exception is written here or it does not exist; an unwritten exception is a
bug.

- **S1 — un-annotated top-level `let`/`var`.** `TopLet` types are inferred
  from initializers and flow cross-module
  (`crates/almide-syntax/src/ast.rs:406`,
  `crates/almide-frontend/src/check/module_inference.rs:65-90`). Editing a
  top-level initializer is therefore a *signature-changing* edit even
  though no signature is written. Same for lambdas stored in such
  bindings. (`fn`/`effect fn` are exempt — their signatures are mandatory
  text, §2 row 1.)
- **S2 — `test` blocks are outside the frame.** File-scoped
  `mod test where { path = expr }` overrides deliberately rebind references
  inside every test in the file
  (`crates/almide-syntax/src/ast.rs:409-432`,
  `crates/almide-frontend/src/lower/mod.rs:420-423,489-495`) — a bounded,
  declared form of the dynamic scoping L1 forbids in program code. Test
  blocks also act as DCE roots (§3-R). L1 quantifies over program
  executions, not test executions.
- **S3 — the ADR-0009 quadrant hole.** A pure `fn` can currently launder
  effects through a higher-order parameter, which ADR-0009 itself measures
  as breaking the purity premise
  (`docs/adr/0009-fn-type-quadrant-transparency.md:29-36`). Until that ADR
  closes the hole, "pure `fn` cannot produce observables" (§2 row 2) is a
  per-quadrant statement, not a theorem.

## 5. The gate

Every language or compiler change answers one question before landing:
**does it preserve L1?** Preserved — land it. Needs a side condition —
write the side condition into §4 in the same PR. Violates it — that is the
design telling you no. (Dynamically scoped handlers, implicit instances,
glob re-exports, and body-inferred anything all fail this gate; that is
the gate working.)
