import AlmideEditBelt.Kernel
import AlmideEditBelt.Typing

/-!
# Pure code is silent

`docs/specs/edit-locality.md` §2, row "`effect` is part of the signature":
the semantic content of the claim "a pure `fn` cannot produce observables".
If an expression types at `eff = false` (outside any `effect fn`) in a
well-typed program, every evaluation of it has an EMPTY trace — no matter
what the runtime environment holds.

The proof needs no type soundness and no value agreement: it is a trace
argument alone. `print` and `!` do not type at `eff = false` at all, and a
call from pure context can only reach a definition whose declared flag is
`false` (E006), whose body — by `WT` — types at `eff = false` again. The
effect column of the signature is a fence the trace cannot cross. -/

namespace LambdaAlmd

/-- From "if the callee is effectful then so are we", at `eff = false`,
conclude the callee's declared flag is `false`. -/
private theorem effect_flag_false {b : Bool} (hpe : b = true → false = true) :
    b = false := by
  cases b
  · rfl
  · exact absurd (hpe rfl) (by simp)

/-- **Pure code is silent.** A well-typed-at-`false` expression evaluates
with an empty trace, in any environment, in any well-typed program. -/
theorem pure_silent {S : Sigs} {D : Defs} (hwt : WT S D)
    {ρ : Env} {e : Expr} {r : Res} {t : Trace} {c : Calls}
    (h : Ev D ρ e r t c) :
    ∀ {Γ : TyEnv} {τ : Ty}, HasTy S false Γ e τ → t = [] := by
  induction h with
  | intLit => intro _ _ _; rfl
  | strLit => intro _ _ _; rfl
  | var _ => intro _ _ _; rfl
  | letNorm _ _ ih₁ ih₂ =>
      intro _ _ ht
      cases ht with
      | letE h₁ h₂ => rw [ih₁ h₁, ih₂ h₂]; rfl
  | letAbrupt _ ih₁ =>
      intro _ _ ht
      cases ht with
      | letE h₁ _ => exact ih₁ h₁
  | callNorm _ hD _ iha ihb =>
      intro _ _ ht
      cases ht with
      | call hS hpe harg =>
          have hfe : _ = false := effect_flag_false hpe
          obtain ⟨sig', hS', heff, hbody⟩ := hwt _ _ hD
          rw [hS] at hS'; cases hS'
          rw [heff, hfe] at hbody
          rw [iha harg, ihb hbody]; rfl
  | callReify _ hD _ iha ihb =>
      intro _ _ ht
      cases ht with
      | call hS hpe harg =>
          have hfe : _ = false := effect_flag_false hpe
          obtain ⟨sig', hS', heff, hbody⟩ := hwt _ _ hD
          rw [hS] at hS'; cases hS'
          rw [heff, hfe] at hbody
          rw [iha harg, ihb hbody]; rfl
  | callArgAbrupt _ iha =>
      intro _ _ ht
      cases ht with
      | call _ _ harg => exact iha harg
  | okNorm _ ih =>
      intro _ _ ht
      cases ht with
      | ok h => exact ih h
  | okAbrupt _ ih =>
      intro _ _ ht
      cases ht with
      | ok h => exact ih h
  | errNorm _ ih =>
      intro _ _ ht
      cases ht with
      | err h => exact ih h
  | errAbrupt _ ih =>
      intro _ _ ht
      cases ht with
      | err h => exact ih h
  | matchOk _ _ ihs ihb =>
      intro _ _ ht
      cases ht with
      | matchR hs h₁ _ => rw [ihs hs, ihb h₁]; rfl
  | matchErr _ _ ihs ihb =>
      intro _ _ ht
      cases ht with
      | matchR hs _ h₂ => rw [ihs hs, ihb h₂]; rfl
  | matchAbrupt _ ihs =>
      intro _ _ ht
      cases ht with
      | matchR hs _ _ => exact ihs hs
  | propOk _ _ => intro _ _ ht; cases ht
  | propErr _ _ => intro _ _ ht; cases ht
  | propAbrupt _ _ => intro _ _ ht; cases ht
  | orElseOk _ ih =>
      intro _ _ ht
      cases ht with
      | orElse he _ => exact ih he
  | orElseErr _ _ ihe ihd =>
      intro _ _ ht
      cases ht with
      | orElse he hd => rw [ihe he, ihd hd]; rfl
  | orElseAbrupt _ ih =>
      intro _ _ ht
      cases ht with
      | orElse he _ => exact ih he
  | printNorm _ _ => intro _ _ ht; cases ht
  | printAbrupt _ _ => intro _ _ ht; cases ht
  | seqNorm _ _ ih₁ ih₂ =>
      intro _ _ ht
      cases ht with
      | seq h₁ h₂ => rw [ih₁ h₁, ih₂ h₂]; rfl
  | seqAbrupt _ ih₁ =>
      intro _ _ ht
      cases ht with
      | seq h₁ _ => exact ih₁ h₁

end LambdaAlmd
