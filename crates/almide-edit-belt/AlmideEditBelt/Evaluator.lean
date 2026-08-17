import AlmideEditBelt.Kernel
import AlmideEditBelt.EditFrame

/-!
# The kernel as an executable specification

Stage 3 of `docs/roadmap/active/edit-locality-theory.md` begins by making
λ_almd runnable: a fuel-indexed evaluator `evalE`, plus the adequacy
direction that matters for conformance —

* `eval_sound` — whatever `evalE` returns, the relational semantics `Ev`
  derives. Together with `ev_det` (determinism, `EditFrame.lean`) this
  makes every kernel-reduced `evalE`-output theorem (`:= by rfl`, in
  `Conformance.lean`/`Corpus.lean`) THE kernel-semantic observables of
  that program: `evalE` says so, `eval_sound` lifts it to a derivation,
  `ev_det` says no other derivation exists.

The other direction (completeness: every derivation is reached by some
fuel) is deliberately NOT proven: the conformance gate only ever consumes
`some`-outputs, where soundness alone carries the claim. The assumption
is an enumerable object — `EvalCompleteness` states it,
`trustEvalCompleteness` marks it — not a doc comment; the boundary is
recorded in `docs/contracts/proven-vs-trusted.md`.
-/

namespace LambdaAlmd

/-- One evaluation outcome: result, trace, entered-definition ledger. -/
abbrev Out := Res × Trace × Calls

/-- Fuel-indexed evaluator. Fuel is spent one unit per expression node
entered; `none` means "out of fuel" or "stuck" (an ill-typed program —
e.g. `print` of a non-string). Every recursive call strictly decreases
fuel, so this is structural recursion on `Nat`. -/
def evalE (D : Defs) : Nat → Env → Expr → Option Out
  | 0, _, _ => none
  | _ + 1, _, .intLit m => some (.norm (.vInt m), [], [])
  | _ + 1, _, .strLit s => some (.norm (.vStr s), [], [])
  | _ + 1, ρ, .var x =>
      match ρ x with
      | some v => some (.norm v, [], [])
      | none => none
  | n + 1, ρ, .letE x e₁ e₂ =>
      match evalE D n ρ e₁ with
      | some (.norm v, t₁, c₁) =>
          match evalE D n (upd ρ x v) e₂ with
          | some (r, t₂, c₂) => some (r, t₁ ++ t₂, c₁ ++ c₂)
          | none => none
      | some (.abrupt v, t, c) => some (.abrupt v, t, c)
      | none => none
  | n + 1, ρ, .call f a =>
      match evalE D n ρ a with
      | some (.norm va, t₁, c₁) =>
          match D f with
          | some d =>
              match evalE D n (upd emptyEnv d.param va) d.body with
              | some (.norm v, t₂, c₂) =>
                  some (.norm v, t₁ ++ t₂, c₁ ++ f :: c₂)
              | some (.abrupt v, t₂, c₂) =>
                  some (.norm (.vErr v), t₁ ++ t₂, c₁ ++ f :: c₂)
              | none => none
          | none => none
      | some (.abrupt v, t, c) => some (.abrupt v, t, c)
      | none => none
  | n + 1, ρ, .ok e =>
      match evalE D n ρ e with
      | some (.norm v, t, c) => some (.norm (.vOk v), t, c)
      | some (.abrupt v, t, c) => some (.abrupt v, t, c)
      | none => none
  | n + 1, ρ, .err e =>
      match evalE D n ρ e with
      | some (.norm v, t, c) => some (.norm (.vErr v), t, c)
      | some (.abrupt v, t, c) => some (.abrupt v, t, c)
      | none => none
  | n + 1, ρ, .matchR s x e₁ y e₂ =>
      match evalE D n ρ s with
      | some (.norm (.vOk v), t₁, c₁) =>
          match evalE D n (upd ρ x v) e₁ with
          | some (r, t₂, c₂) => some (r, t₁ ++ t₂, c₁ ++ c₂)
          | none => none
      | some (.norm (.vErr v), t₁, c₁) =>
          match evalE D n (upd ρ y v) e₂ with
          | some (r, t₂, c₂) => some (r, t₁ ++ t₂, c₁ ++ c₂)
          | none => none
      | some (.norm _, _, _) => none
      | some (.abrupt v, t, c) => some (.abrupt v, t, c)
      | none => none
  | n + 1, ρ, .prop e =>
      match evalE D n ρ e with
      | some (.norm (.vOk v), t, c) => some (.norm v, t, c)
      | some (.norm (.vErr v), t, c) => some (.abrupt v, t, c)
      | some (.norm _, _, _) => none
      | some (.abrupt v, t, c) => some (.abrupt v, t, c)
      | none => none
  | n + 1, ρ, .orElse e d' =>
      match evalE D n ρ e with
      | some (.norm (.vOk v), t, c) => some (.norm v, t, c)
      | some (.norm (.vErr _), t₁, c₁) =>
          match evalE D n ρ d' with
          | some (r, t₂, c₂) => some (r, t₁ ++ t₂, c₁ ++ c₂)
          | none => none
      | some (.norm _, _, _) => none
      | some (.abrupt v, t, c) => some (.abrupt v, t, c)
      | none => none
  | n + 1, ρ, .print e =>
      match evalE D n ρ e with
      | some (.norm (.vStr s), t, c) => some (.norm .vUnit, t ++ [s], c)
      | some (.norm _, _, _) => none
      | some (.abrupt v, t, c) => some (.abrupt v, t, c)
      | none => none
  | n + 1, ρ, .seq e₁ e₂ =>
      match evalE D n ρ e₁ with
      | some (.norm _, t₁, c₁) =>
          match evalE D n ρ e₂ with
          | some (r, t₂, c₂) => some (r, t₁ ++ t₂, c₁ ++ c₂)
          | none => none
      | some (.abrupt v, t, c) => some (.abrupt v, t, c)
      | none => none

/-- **Soundness of the evaluator.** Whatever `evalE` computes, the
relational semantics derives. With `ev_det`, a `some`-output is therefore
THE meaning of the program — the fact the conformance gate stands on. -/
theorem eval_sound {D : Defs} :
    ∀ (n : Nat) (ρ : Env) (e : Expr) (r : Res) (t : Trace) (c : Calls),
      evalE D n ρ e = some (r, t, c) → Ev D ρ e r t c := by
  intro n
  induction n with
  | zero => intro ρ e r t c h; simp [evalE] at h
  | succ n ih =>
      intro ρ e r t c h
      cases e <;> simp only [evalE] at h
      case intLit m =>
        cases h; exact .intLit
      case strLit s =>
        cases h; exact .strLit
      case var x =>
        split at h
        · cases h; exact .var (by assumption)
        · cases h
      case letE x e₁ e₂ =>
        split at h
        · split at h
          · cases h
            exact .letNorm (ih _ _ _ _ _ (by assumption)) (ih _ _ _ _ _ (by assumption))
          · cases h
        · cases h; exact .letAbrupt (ih _ _ _ _ _ (by assumption))
        · cases h
      case call f a =>
        split at h
        · split at h
          · split at h
            · cases h
              exact .callNorm (ih _ _ _ _ _ (by assumption)) (by assumption)
                (ih _ _ _ _ _ (by assumption))
            · cases h
              exact .callReify (ih _ _ _ _ _ (by assumption)) (by assumption)
                (ih _ _ _ _ _ (by assumption))
            · cases h
          · cases h
        · cases h; exact .callArgAbrupt (ih _ _ _ _ _ (by assumption))
        · cases h
      case ok e =>
        split at h
        · cases h; exact .okNorm (ih _ _ _ _ _ (by assumption))
        · cases h; exact .okAbrupt (ih _ _ _ _ _ (by assumption))
        · cases h
      case err e =>
        split at h
        · cases h; exact .errNorm (ih _ _ _ _ _ (by assumption))
        · cases h; exact .errAbrupt (ih _ _ _ _ _ (by assumption))
        · cases h
      case matchR s x e₁ y e₂ =>
        split at h
        · split at h
          · cases h
            exact .matchOk (ih _ _ _ _ _ (by assumption)) (ih _ _ _ _ _ (by assumption))
          · cases h
        · split at h
          · cases h
            exact .matchErr (ih _ _ _ _ _ (by assumption)) (ih _ _ _ _ _ (by assumption))
          · cases h
        · cases h
        · cases h; exact .matchAbrupt (ih _ _ _ _ _ (by assumption))
        · cases h
      case prop e =>
        split at h
        · cases h; exact .propOk (ih _ _ _ _ _ (by assumption))
        · cases h; exact .propErr (ih _ _ _ _ _ (by assumption))
        · cases h
        · cases h; exact .propAbrupt (ih _ _ _ _ _ (by assumption))
        · cases h
      case orElse e d' =>
        split at h
        · cases h; exact .orElseOk (ih _ _ _ _ _ (by assumption))
        · split at h
          · cases h
            exact .orElseErr (ih _ _ _ _ _ (by assumption)) (ih _ _ _ _ _ (by assumption))
          · cases h
        · cases h
        · cases h; exact .orElseAbrupt (ih _ _ _ _ _ (by assumption))
        · cases h
      case print e =>
        split at h
        · cases h; exact .printNorm (ih _ _ _ _ _ (by assumption))
        · cases h
        · cases h; exact .printAbrupt (ih _ _ _ _ _ (by assumption))
        · cases h
      case seq e₁ e₂ =>
        split at h
        · split at h
          · cases h
            exact .seqNorm (ih _ _ _ _ _ (by assumption)) (ih _ _ _ _ _ (by assumption))
          · cases h
        · cases h; exact .seqAbrupt (ih _ _ _ _ _ (by assumption))
        · cases h

/-- The completeness direction as a first-class statement: every
derivation is reached at some fuel. DELIBERATELY UNPROVEN — the
conformance gate only ever consumes `some`-outputs, where `eval_sound`
alone carries the claim. -/
def EvalCompleteness : Prop :=
  ∀ (D : Defs) (ρ : Env) (e : Expr) (r : Res) (t : Trace) (c : Calls),
    Ev D ρ e r t c → ∃ n, evalE D n ρ e = some (r, t, c)

/-- The trusted assumption as an enumerable object (lean4's
`trustCompiler` pattern, `src/Init/Core.lean`): an argument that needs
`EvalCompleteness` — "`evalE` returning `none` at every fuel means no
derivation exists" — must cite this axiom, so the seam shows up in the
axiom audits (`#print axioms`, the CI ratchet) instead of hiding in a
doc comment. Kept at `True` so it can prove nothing by accident. -/
axiom trustEvalCompleteness : True

end LambdaAlmd
