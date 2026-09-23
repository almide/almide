import AlmideEditBelt.Kernel
import AlmideEditBelt.Typing
import AlmideEditBelt.Purity

/-!
# Above L1: declared-frame preservation (L4) and composition (L5)

`docs/specs/edit-locality.md` §1a, the two obligations #2009 asked to be
written as statements. They are numbered L4 and L5, not L2 and L3, because
that file already spends L2 on cross-target agreement and L3 on diagnostic
locality (the ladder note added in #2525).

L1 (`edit_frame`) is about executions that do NOT enter the edited
definition. The interesting case is the one L1 leaves alone: an execution
that DOES pass through it. What survives there is not "the same value" — a
replacement may legitimately compute something else — but the FRAME the
declared contract draws around the definition.

What this file proves, and what it does not:

* **L4, effect frame — proven here.** `l4_pure_replacement_silent`: replace
  a definition declared pure (`isEffect = false`) by ANY body that checks
  against the unchanged signature; the replacement produces no observables,
  in any environment, in the edited program. The declared effect column is a
  fence a replacement cannot cross. It is `typing_modular` (the edited
  program stays well-typed) composed with `pure_silent` (pure code has an
  empty trace) — no new induction, because both halves were already proven
  in the form this needs. That the statement falls out of two existing
  theorems is the point: the `effect` flag was designed as a frame.
* **L5, composition — proven here.** `l5_edits_compose` and
  `l5_pure_silent_after_edits`: a SEQUENCE of signature-preserving
  replacements keeps the program well-typed, and after all of them every
  definition declared pure is still silent. By induction on the edit list.
* **L4, resource frame (#1997's `scoped` regions) — NOT stated here.**
  λ_almd has no heap and no regions, so there is nothing for a region
  contract to quantify over. The obligation is written in the spec in prose;
  mechanizing it needs a region component in the kernel first. Writing a Lean
  statement against a calculus that cannot express it would be a theorem
  about nothing.
* **L5, realization on both targets — trusted, not proven.** That the
  checked lowering (#1995's ExitPlan, #1996's E-OWN-LOWERING) preserves these
  observations on native and wasm is a refinement obligation between this
  kernel and the two backends. Like every backend claim in this belt, it is
  gated by `spec/wasm_cross` fixtures and the contract ledger, and named as
  trusted in `docs/contracts/proven-vs-trusted.md`.

Nothing here claims business behaviour. A pure replacement may return a
different value, and that value may change what the CALLER prints next; L4
says only that the replacement itself adds nothing to the trace.
-/

namespace LambdaAlmd

/-- **L4, effect frame.** Replace `f` by any definition `d'` that checks
against `f`'s existing signature and is declared pure. Every evaluation of
the replacement's body, in the edited program and in any environment, has an
empty trace: a replacement that satisfies a pure declared contract cannot
produce observables. -/
theorem l4_pure_replacement_silent {S : Sigs} {D : Defs} {f : Name}
    {d' : Defn} (hwt : WT S D) (hd' : DefWT S f d')
    (hpure : d'.isEffect = false)
    {ρ : Env} {r : Res} {t : Trace} {c : Calls}
    (h : Ev (upd D f d') ρ d'.body r t c) : t = [] := by
  have hwt' : WT S (upd D f d') := typing_modular hwt hd'
  obtain ⟨_, _, _, hty⟩ := hd'
  rw [hpure] at hty
  exact pure_silent hwt' h hty

/-- A sequence of body replacements, applied left to right. Signatures do
not change: an edit replaces a body, and the declared signature is what
every replacement is checked against. -/
def applyEdits (D : Defs) : List (Name × Defn) → Defs
  | [] => D
  | (f, d') :: es => applyEdits (upd D f d') es

/-- Every replacement in the sequence checks against the (unchanging)
signature table. -/
def EditsWT (S : Sigs) : List (Name × Defn) → Prop
  | [] => True
  | (f, d') :: es => DefWT S f d' ∧ EditsWT S es

/-- **L5, composition.** Any sequence of signature-preserving replacements
keeps a well-typed program well-typed. `typing_modular` is one step; this is
its induction. -/
theorem l5_edits_compose {S : Sigs} (es : List (Name × Defn)) :
    ∀ {D : Defs}, WT S D → EditsWT S es → WT S (applyEdits D es) := by
  induction es with
  | nil => intro _ hwt _; exact hwt
  | cons e es ih =>
      obtain ⟨f, d'⟩ := e
      intro D hwt hes
      obtain ⟨hd, hes'⟩ := hes
      exact ih (typing_modular hwt hd) hes'

/-- **L5, the frame survives composition.** After any sequence of
signature-preserving replacements, every definition declared pure is still
silent: no chain of contract-preserving edits can open the effect fence,
however long. -/
theorem l5_pure_silent_after_edits {S : Sigs} {D : Defs}
    {es : List (Name × Defn)} (hwt : WT S D) (hes : EditsWT S es)
    {f : Name} {d : Defn} (hfd : applyEdits D es f = some d)
    (hpure : d.isEffect = false)
    {ρ : Env} {r : Res} {t : Trace} {c : Calls}
    (h : Ev (applyEdits D es) ρ d.body r t c) : t = [] := by
  have hwt' : WT S (applyEdits D es) := l5_edits_compose es hwt hes
  obtain ⟨_, _, _, hty⟩ := hwt' f d hfd
  rw [hpure] at hty
  exact pure_silent hwt' h hty

/-!
## Why L4 is not a theorem about nothing

A theorem whose hypotheses cannot all hold at once is true and says nothing.
The two witnesses below are checked by `lake build` on every run, so that
cannot quietly become the case.

* **The hypotheses are satisfiable.** `l4_witness` meets all four at once
  with a concrete pure replacement, so L4 is not vacuously true.
* **`hpure` is load-bearing.** `l4_loud_replacement_prints` is a replacement
  that checks against its OWN signature — declared effectful — and prints.
  Drop the purity hypothesis and the conclusion `t = []` is false. The fence
  L4 names is the declared effect flag, and this is the case it keeps out.
-/

private def witnessSigs : Sigs :=
  fun n => if n = "f" then some ⟨.int, .int, false⟩ else none
private def loudSigs : Sigs :=
  fun n => if n = "f" then some ⟨.int, .unit, true⟩ else none
private def emptyDefs : Defs := fun _ => none
private def pureReplacement : Defn := ⟨"x", .intLit 7, false⟩
private def loudReplacement : Defn := ⟨"x", .print (.strLit "hi"), true⟩

/-- All four hypotheses of `l4_pure_replacement_silent` hold together. -/
theorem l4_witness : ([] : Trace) = [] :=
  l4_pure_replacement_silent (S := witnessSigs) (D := emptyDefs) (f := "f")
    (fun _ _ h => by simp [emptyDefs] at h)
    ⟨⟨.int, .int, false⟩, by simp [witnessSigs], rfl, HasTy.intLit⟩
    rfl
    (Ev.intLit : Ev (upd emptyDefs "f" pureReplacement) emptyEnv
      pureReplacement.body (.norm (.vInt 7)) [] [])

/-- Without `hpure`, L4's conclusion fails: this replacement checks against
its own effectful signature and its body's trace is not empty. -/
theorem l4_loud_replacement_prints :
    DefWT loudSigs "f" loudReplacement ∧
    Ev (upd emptyDefs "f" loudReplacement) emptyEnv loudReplacement.body
      (.norm .vUnit) ["hi"] [] ∧
    (["hi"] : Trace) ≠ [] :=
  ⟨⟨⟨.int, .unit, true⟩, by simp [loudSigs], rfl, HasTy.print HasTy.strLit⟩,
   Ev.printNorm Ev.strLit,
   by simp⟩

end LambdaAlmd
