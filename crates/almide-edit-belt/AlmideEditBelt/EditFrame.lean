import AlmideEditBelt.Kernel

/-!
# The edit-frame theorem (L1) for λ_almd

`docs/specs/edit-locality.md` §1, L1: a signature-preserving edit to a
definition cannot change the observables of any execution that does not pass
through it. The kernel form is stronger and cleaner:

* `ev_agree` — evaluation depends only on the definitions it actually
  entered. This is the whole theorem; everything else is packaging. It is
  provable in a page BECAUSE the call rules read `D` at the called name and
  nowhere else — a language with overload resolution or dynamically scoped
  handlers would consult the rest of the table inside the call rule, and the
  induction would not go through. The proof is the design decision.
* `edit_frame` — L1, transport form: replacing `f` (by ANYTHING — the
  untyped kernel does not even need the signature-preservation hypothesis;
  that hypothesis lives in `typing_modular`, which keeps the edited program
  well-typed) preserves every execution that avoids `f`, verbatim: same
  result, same trace, same call ledger.
* `ev_det` — evaluation is deterministic, so the transported execution is
  the ONLY one the edited program has.
* `edit_frame_observables` — L1, observables form: the edited program's
  run agrees exactly with the original's.
-/

namespace LambdaAlmd

/-- **Evaluation reads only what it enters.** If `D'` agrees with `D` on
every definition the derivation actually called, the derivation transports
verbatim. -/
theorem ev_agree {D D' : Defs} {ρ : Env} {e : Expr} {r : Res} {t : Trace}
    {c : Calls} (h : Ev D ρ e r t c) (hag : ∀ g ∈ c, D' g = D g) :
    Ev D' ρ e r t c := by
  induction h with
  | intLit => exact .intLit
  | strLit => exact .strLit
  | var hx => exact .var hx
  | letNorm h₁ h₂ ih₁ ih₂ =>
      exact .letNorm (ih₁ fun g hg => hag g (List.mem_append_left _ hg))
        (ih₂ fun g hg => hag g (List.mem_append_right _ hg))
  | letAbrupt h₁ ih₁ => exact .letAbrupt (ih₁ hag)
  | callNorm ha hD hb iha ihb =>
      refine .callNorm (iha fun g hg => hag g (List.mem_append_left _ hg))
        ((hag _ (List.mem_append_right _ (List.mem_cons_self))).trans hD)
        (ihb fun g hg => hag g (List.mem_append_right _ (List.mem_cons_of_mem _ hg)))
  | callReify ha hD hb iha ihb =>
      refine .callReify (iha fun g hg => hag g (List.mem_append_left _ hg))
        ((hag _ (List.mem_append_right _ (List.mem_cons_self))).trans hD)
        (ihb fun g hg => hag g (List.mem_append_right _ (List.mem_cons_of_mem _ hg)))
  | callArgAbrupt ha iha => exact .callArgAbrupt (iha hag)
  | okNorm h ih => exact .okNorm (ih hag)
  | okAbrupt h ih => exact .okAbrupt (ih hag)
  | errNorm h ih => exact .errNorm (ih hag)
  | errAbrupt h ih => exact .errAbrupt (ih hag)
  | matchOk hs hb ihs ihb =>
      exact .matchOk (ihs fun g hg => hag g (List.mem_append_left _ hg))
        (ihb fun g hg => hag g (List.mem_append_right _ hg))
  | matchErr hs hb ihs ihb =>
      exact .matchErr (ihs fun g hg => hag g (List.mem_append_left _ hg))
        (ihb fun g hg => hag g (List.mem_append_right _ hg))
  | matchAbrupt hs ihs => exact .matchAbrupt (ihs hag)
  | propOk h ih => exact .propOk (ih hag)
  | propErr h ih => exact .propErr (ih hag)
  | propAbrupt h ih => exact .propAbrupt (ih hag)
  | orElseOk h ih => exact .orElseOk (ih hag)
  | orElseErr he hd ihe ihd =>
      exact .orElseErr (ihe fun g hg => hag g (List.mem_append_left _ hg))
        (ihd fun g hg => hag g (List.mem_append_right _ hg))
  | orElseAbrupt h ih => exact .orElseAbrupt (ih hag)
  | printNorm h ih => exact .printNorm (ih hag)
  | printAbrupt h ih => exact .printAbrupt (ih hag)
  | seqNorm h₁ h₂ ih₁ ih₂ =>
      exact .seqNorm (ih₁ fun g hg => hag g (List.mem_append_left _ hg))
        (ih₂ fun g hg => hag g (List.mem_append_right _ hg))
  | seqAbrupt h₁ ih₁ => exact .seqAbrupt (ih₁ hag)

/-- **The edit-frame theorem (L1), transport form.** An execution that never
enters `f` survives replacing `f`'s definition — by anything — verbatim.
(Signature preservation is a TYPED-level concern: see `typing_modular`,
which keeps the edited program well-typed; the runtime frame needs nothing.) -/
theorem edit_frame {D : Defs} {f : Name} {d' : Defn} {ρ : Env} {e : Expr}
    {r : Res} {t : Trace} {c : Calls}
    (h : Ev D ρ e r t c) (hf : f ∉ c) :
    Ev (upd D f d') ρ e r t c :=
  ev_agree h (fun g hg => upd_other D f g d' (fun heq => hf (heq ▸ hg)))

/-- **Determinism.** One expression, one environment, one outcome — result,
trace, and call ledger included. -/
theorem ev_det {D : Defs} {ρ : Env} {e : Expr} {r₁ : Res} {t₁ : Trace}
    {c₁ : Calls} (h₁ : Ev D ρ e r₁ t₁ c₁) :
    ∀ {r₂ t₂ c₂}, Ev D ρ e r₂ t₂ c₂ → r₁ = r₂ ∧ t₁ = t₂ ∧ c₁ = c₂ := by
  induction h₁ with
  | intLit => intro _ _ _ h₂; cases h₂; exact ⟨rfl, rfl, rfl⟩
  | strLit => intro _ _ _ h₂; cases h₂; exact ⟨rfl, rfl, rfl⟩
  | var hx =>
      intro _ _ _ h₂
      cases h₂ with
      | var hx' => rw [hx] at hx'; cases hx'; exact ⟨rfl, rfl, rfl⟩
  | letNorm ha hb iha ihb =>
      intro _ _ _ h₂
      cases h₂ with
      | letNorm ha' hb' =>
          obtain ⟨hr, ht, hc⟩ := iha ha'
          cases hr; subst ht; subst hc
          obtain ⟨hr₂, ht₂, hc₂⟩ := ihb hb'
          exact ⟨hr₂, by rw [ht₂], by rw [hc₂]⟩
      | letAbrupt ha' => obtain ⟨hr, -, -⟩ := iha ha'; cases hr
  | letAbrupt ha iha =>
      intro _ _ _ h₂
      cases h₂ with
      | letNorm ha' _ => obtain ⟨hr, -, -⟩ := iha ha'; cases hr
      | letAbrupt ha' =>
          obtain ⟨hr, ht, hc⟩ := iha ha'
          cases hr; exact ⟨rfl, ht, hc⟩
  | callNorm ha hD hb iha ihb =>
      intro _ _ _ h₂
      cases h₂ with
      | callNorm ha' hD' hb' =>
          obtain ⟨hr, ht, hc⟩ := iha ha'
          cases hr; subst ht; subst hc
          rw [hD] at hD'; cases hD'
          obtain ⟨hr₂, ht₂, hc₂⟩ := ihb hb'
          cases hr₂; exact ⟨rfl, by rw [ht₂], by rw [hc₂]⟩
      | callReify ha' hD' hb' =>
          obtain ⟨hr, ht, hc⟩ := iha ha'
          cases hr; subst ht; subst hc
          rw [hD] at hD'; cases hD'
          obtain ⟨hr₂, -, -⟩ := ihb hb'
          cases hr₂
      | callArgAbrupt ha' => obtain ⟨hr, -, -⟩ := iha ha'; cases hr
  | callReify ha hD hb iha ihb =>
      intro _ _ _ h₂
      cases h₂ with
      | callNorm ha' hD' hb' =>
          obtain ⟨hr, ht, hc⟩ := iha ha'
          cases hr; subst ht; subst hc
          rw [hD] at hD'; cases hD'
          obtain ⟨hr₂, -, -⟩ := ihb hb'
          cases hr₂
      | callReify ha' hD' hb' =>
          obtain ⟨hr, ht, hc⟩ := iha ha'
          cases hr; subst ht; subst hc
          rw [hD] at hD'; cases hD'
          obtain ⟨hr₂, ht₂, hc₂⟩ := ihb hb'
          cases hr₂; exact ⟨rfl, by rw [ht₂], by rw [hc₂]⟩
      | callArgAbrupt ha' => obtain ⟨hr, -, -⟩ := iha ha'; cases hr
  | callArgAbrupt ha iha =>
      intro _ _ _ h₂
      cases h₂ with
      | callNorm ha' _ _ => obtain ⟨hr, -, -⟩ := iha ha'; cases hr
      | callReify ha' _ _ => obtain ⟨hr, -, -⟩ := iha ha'; cases hr
      | callArgAbrupt ha' =>
          obtain ⟨hr, ht, hc⟩ := iha ha'
          cases hr; exact ⟨rfl, ht, hc⟩
  | okNorm h ih =>
      intro _ _ _ h₂
      cases h₂ with
      | okNorm h' =>
          obtain ⟨hr, ht, hc⟩ := ih h'
          cases hr; exact ⟨rfl, ht, hc⟩
      | okAbrupt h' => obtain ⟨hr, -, -⟩ := ih h'; cases hr
  | okAbrupt h ih =>
      intro _ _ _ h₂
      cases h₂ with
      | okNorm h' => obtain ⟨hr, -, -⟩ := ih h'; cases hr
      | okAbrupt h' =>
          obtain ⟨hr, ht, hc⟩ := ih h'
          cases hr; exact ⟨rfl, ht, hc⟩
  | errNorm h ih =>
      intro _ _ _ h₂
      cases h₂ with
      | errNorm h' =>
          obtain ⟨hr, ht, hc⟩ := ih h'
          cases hr; exact ⟨rfl, ht, hc⟩
      | errAbrupt h' => obtain ⟨hr, -, -⟩ := ih h'; cases hr
  | errAbrupt h ih =>
      intro _ _ _ h₂
      cases h₂ with
      | errNorm h' => obtain ⟨hr, -, -⟩ := ih h'; cases hr
      | errAbrupt h' =>
          obtain ⟨hr, ht, hc⟩ := ih h'
          cases hr; exact ⟨rfl, ht, hc⟩
  | matchOk hs hb ihs ihb =>
      intro _ _ _ h₂
      cases h₂ with
      | matchOk hs' hb' =>
          obtain ⟨hr, ht, hc⟩ := ihs hs'
          cases hr; subst ht; subst hc
          obtain ⟨hr₂, ht₂, hc₂⟩ := ihb hb'
          exact ⟨hr₂, by rw [ht₂], by rw [hc₂]⟩
      | matchErr hs' _ => obtain ⟨hr, -, -⟩ := ihs hs'; cases hr
      | matchAbrupt hs' => obtain ⟨hr, -, -⟩ := ihs hs'; cases hr
  | matchErr hs hb ihs ihb =>
      intro _ _ _ h₂
      cases h₂ with
      | matchOk hs' _ => obtain ⟨hr, -, -⟩ := ihs hs'; cases hr
      | matchErr hs' hb' =>
          obtain ⟨hr, ht, hc⟩ := ihs hs'
          cases hr; subst ht; subst hc
          obtain ⟨hr₂, ht₂, hc₂⟩ := ihb hb'
          exact ⟨hr₂, by rw [ht₂], by rw [hc₂]⟩
      | matchAbrupt hs' => obtain ⟨hr, -, -⟩ := ihs hs'; cases hr
  | matchAbrupt hs ihs =>
      intro _ _ _ h₂
      cases h₂ with
      | matchOk hs' _ => obtain ⟨hr, -, -⟩ := ihs hs'; cases hr
      | matchErr hs' _ => obtain ⟨hr, -, -⟩ := ihs hs'; cases hr
      | matchAbrupt hs' =>
          obtain ⟨hr, ht, hc⟩ := ihs hs'
          cases hr; exact ⟨rfl, ht, hc⟩
  | propOk h ih =>
      intro _ _ _ h₂
      cases h₂ with
      | propOk h' =>
          obtain ⟨hr, ht, hc⟩ := ih h'
          cases hr; exact ⟨rfl, ht, hc⟩
      | propErr h' => obtain ⟨hr, -, -⟩ := ih h'; cases hr
      | propAbrupt h' => obtain ⟨hr, -, -⟩ := ih h'; cases hr
  | propErr h ih =>
      intro _ _ _ h₂
      cases h₂ with
      | propOk h' => obtain ⟨hr, -, -⟩ := ih h'; cases hr
      | propErr h' =>
          obtain ⟨hr, ht, hc⟩ := ih h'
          cases hr; exact ⟨rfl, ht, hc⟩
      | propAbrupt h' => obtain ⟨hr, -, -⟩ := ih h'; cases hr
  | propAbrupt h ih =>
      intro _ _ _ h₂
      cases h₂ with
      | propOk h' => obtain ⟨hr, -, -⟩ := ih h'; cases hr
      | propErr h' => obtain ⟨hr, -, -⟩ := ih h'; cases hr
      | propAbrupt h' =>
          obtain ⟨hr, ht, hc⟩ := ih h'
          cases hr; exact ⟨rfl, ht, hc⟩
  | orElseOk h ih =>
      intro _ _ _ h₂
      cases h₂ with
      | orElseOk h' =>
          obtain ⟨hr, ht, hc⟩ := ih h'
          cases hr; exact ⟨rfl, ht, hc⟩
      | orElseErr h' _ => obtain ⟨hr, -, -⟩ := ih h'; cases hr
      | orElseAbrupt h' => obtain ⟨hr, -, -⟩ := ih h'; cases hr
  | orElseErr he hd ihe ihd =>
      intro _ _ _ h₂
      cases h₂ with
      | orElseOk h' => obtain ⟨hr, -, -⟩ := ihe h'; cases hr
      | orElseErr he' hd' =>
          obtain ⟨hr, ht, hc⟩ := ihe he'
          cases hr; subst ht; subst hc
          obtain ⟨hr₂, ht₂, hc₂⟩ := ihd hd'
          exact ⟨hr₂, by rw [ht₂], by rw [hc₂]⟩
      | orElseAbrupt h' => obtain ⟨hr, -, -⟩ := ihe h'; cases hr
  | orElseAbrupt h ih =>
      intro _ _ _ h₂
      cases h₂ with
      | orElseOk h' => obtain ⟨hr, -, -⟩ := ih h'; cases hr
      | orElseErr h' _ => obtain ⟨hr, -, -⟩ := ih h'; cases hr
      | orElseAbrupt h' =>
          obtain ⟨hr, ht, hc⟩ := ih h'
          cases hr; exact ⟨rfl, ht, hc⟩
  | printNorm h ih =>
      intro _ _ _ h₂
      cases h₂ with
      | printNorm h' =>
          obtain ⟨hr, ht, hc⟩ := ih h'
          cases hr; subst ht
          exact ⟨rfl, rfl, hc⟩
      | printAbrupt h' => obtain ⟨hr, -, -⟩ := ih h'; cases hr
  | printAbrupt h ih =>
      intro _ _ _ h₂
      cases h₂ with
      | printNorm h' => obtain ⟨hr, -, -⟩ := ih h'; cases hr
      | printAbrupt h' =>
          obtain ⟨hr, ht, hc⟩ := ih h'
          cases hr; exact ⟨rfl, ht, hc⟩
  | seqNorm ha hb iha ihb =>
      intro _ _ _ h₂
      cases h₂ with
      | seqNorm ha' hb' =>
          obtain ⟨hr, ht, hc⟩ := iha ha'
          cases hr; subst ht; subst hc
          obtain ⟨hr₂, ht₂, hc₂⟩ := ihb hb'
          exact ⟨hr₂, by rw [ht₂], by rw [hc₂]⟩
      | seqAbrupt ha' => obtain ⟨hr, -, -⟩ := iha ha'; cases hr
  | seqAbrupt ha iha =>
      intro _ _ _ h₂
      cases h₂ with
      | seqNorm ha' _ => obtain ⟨hr, -, -⟩ := iha ha'; cases hr
      | seqAbrupt ha' =>
          obtain ⟨hr, ht, hc⟩ := iha ha'
          cases hr; exact ⟨rfl, ht, hc⟩

/-- **L1, observables form.** Run the original without entering `f`; edit
`f`; the edited program's run of the same expression has EXACTLY the same
result, trace, and call ledger — `edit_frame` transports the execution and
`ev_det` says the edited program has no other. -/
theorem edit_frame_observables {D : Defs} {f : Name} {d' : Defn} {ρ : Env}
    {e : Expr} {r : Res} {t : Trace} {c : Calls} {r' : Res} {t' : Trace}
    {c' : Calls}
    (h : Ev D ρ e r t c) (hf : f ∉ c)
    (h' : Ev (upd D f d') ρ e r' t' c') :
    r' = r ∧ t' = t ∧ c' = c :=
  ev_det h' (edit_frame h hf)

end LambdaAlmd
