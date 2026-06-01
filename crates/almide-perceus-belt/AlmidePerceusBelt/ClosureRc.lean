/-
  AlmidePerceusBelt — Closure environment RC contract (Closure Architecture v2, P5).

  Closes the Perceus→binary proof chain for closures. The compiler lowers a
  closure's lifecycle to ordinary RcInc / RcDec IR nodes (PerceusPass + Rule 6
  in pass_perceus.rs):
    • create: inc each captured heap var (the env takes a reference), alloc cv;
    • drop:   dec cv (free the closure object), then dec each captured var.

  We model that lowering with the existing `inc` / `dec` / `vdecl` FnBody nodes —
  nothing axiomatic — and PROVE the env RC contract emerges: over its captures a
  closure inc's and dec's each variable the same number of times, so closures
  preserve `isFreed` / `allHeapFreed` (no leak, no double free), and the closure
  object itself is freed.
-/
import AlmidePerceusBelt.FnBody

namespace AlmidePerceusBelt

/-- Occurrences of `t` in a capture list. -/
def capCount : List VarId → VarId → Nat
  | [], _ => 0
  | c :: cs, t => (if c == t then 1 else 0) + capCount cs t

/-- `inc` each captured var in front of a continuation (env takes a ref). -/
def incCaps : List VarId → FnBody → FnBody
  | [], b => b
  | c :: cs, b => .inc c (incCaps cs b)

/-- `dec` each captured var in front of a continuation (env releases its refs). -/
def decCaps : List VarId → FnBody → FnBody
  | [], b => b
  | c :: cs, b => .dec c (decCaps cs b)

-- ══════ Count lemmas for incCaps / decCaps ══════

theorem incCaps_incs (caps : List VarId) (cont : FnBody) (t : VarId) :
    countIncs (incCaps caps cont) t = capCount caps t + countIncs cont t := by
  induction caps with
  | nil => simp [incCaps, capCount]
  | cons c cs ih => simp [incCaps, capCount, countIncs, ih]; omega

theorem incCaps_decs (caps : List VarId) (cont : FnBody) (t : VarId) :
    countDecs (incCaps caps cont) t = countDecs cont t := by
  induction caps with
  | nil => simp [incCaps]
  | cons c cs ih => simp [incCaps, countDecs, ih]

theorem decCaps_decs (caps : List VarId) (cont : FnBody) (t : VarId) :
    countDecs (decCaps caps cont) t = capCount caps t + countDecs cont t := by
  induction caps with
  | nil => simp [decCaps, capCount]
  | cons c cs ih => simp [decCaps, capCount, countDecs, ih]; omega

theorem decCaps_incs (caps : List VarId) (cont : FnBody) (t : VarId) :
    countIncs (decCaps caps cont) t = countIncs cont t := by
  induction caps with
  | nil => simp [decCaps]
  | cons c cs ih => simp [decCaps, countIncs, ih]

-- ══════ The closure RC lowering ══════

/-- Perceus lowering of a closure lifecycle, followed by continuation `cont`:
    inc each capture + alloc the closure object `cv` (create), then
    dec `cv` + dec each capture (drop / Rule 6). Built only from existing
    `inc` / `dec` / `vdecl` nodes — the env RC is *derived*, not assumed. -/
def closureScope (cv : VarId) (caps : List VarId) (cont : FnBody) : FnBody :=
  incCaps caps (.vdecl cv (.list .unit) (.dec cv (decCaps caps cont)))

theorem closureScope_incs (cv : VarId) (caps : List VarId) (cont : FnBody) (t : VarId) :
    countIncs (closureScope cv caps cont) t = capCount caps t + countIncs cont t := by
  simp [closureScope, incCaps_incs, countIncs, decCaps_incs]

theorem closureScope_decs (cv : VarId) (caps : List VarId) (cont : FnBody) (t : VarId) :
    countDecs (closureScope cv caps cont) t
      = (if cv == t then 1 else 0) + (capCount caps t + countDecs cont t) := by
  simp [closureScope, incCaps_decs, countDecs, decCaps_decs]

-- ══════ THE ENV RC CONTRACT ══════

/-- **Env RC contract.** Over its captures a closure inc's and dec's each
    captured variable the same number of times. With an empty continuation the
    closure's own contribution stands alone: incs == decs for every capture
    (`cv ≠ t`; a closure does not free itself through a capture). -/
theorem closure_env_rc_balanced (cv : VarId) (caps : List VarId) (t : VarId)
    (h : cv ≠ t) :
    countIncs (closureScope cv caps .nop) t = countDecs (closureScope cv caps .nop) t := by
  have hb : (cv == t) = false := beq_eq_false_iff_ne.mpr h
  rw [closureScope_incs, closureScope_decs, hb]
  simp [countIncs, countDecs]

/-- A closure preserves `isFreed` for every captured (borrowed) variable: its
    balanced inc/dec leaves the free-balance of the surrounding scope intact. -/
theorem closure_preserves_isFreed (cv : VarId) (caps : List VarId) (cont : FnBody)
    (t : VarId) (h : cv ≠ t) :
    isFreed (closureScope cv caps cont) t ↔ isFreed cont t := by
  have hb : (cv == t) = false := beq_eq_false_iff_ne.mpr h
  unfold isFreed
  rw [closureScope_incs, closureScope_decs, hb]
  simp; omega

/-- The closure object `cv` itself is freed by the lowering (the `dec cv` at
    drop), given the continuation does not itself touch `cv` and `cv` is not in
    its own capture list. -/
theorem closure_obj_freed (cv : VarId) (caps : List VarId) (cont : FnBody)
    (hcap : capCount caps cv = 0)
    (hi : countIncs cont cv = 0) (hd : countDecs cont cv = 0) :
    isFreed (closureScope cv caps cont) cv := by
  unfold isFreed
  rw [closureScope_incs, closureScope_decs, hcap, hi, hd]
  simp

-- ══════ allHeapFreed integration ══════

theorem incCaps_allHeapFreed (caps : List VarId) (cont : FnBody)
    (h : allHeapFreed cont) : allHeapFreed (incCaps caps cont) := by
  induction caps with
  | nil => simpa [incCaps] using h
  | cons c cs ih => simp only [incCaps, allHeapFreed]; exact ih

theorem decCaps_allHeapFreed (caps : List VarId) (cont : FnBody)
    (h : allHeapFreed cont) : allHeapFreed (decCaps caps cont) := by
  induction caps with
  | nil => simpa [decCaps] using h
  | cons c cs ih => simp only [decCaps, allHeapFreed]; exact ih

/-- The closure lowering is fully RC-correct: it satisfies `allHeapFreed` — the
    closure object `cv` is freed and every capture is balanced — whenever the
    continuation does. This folds closures into the main no-leak guarantee. -/
theorem closureScope_allHeapFreed (cv : VarId) (caps : List VarId) (cont : FnBody)
    (h : allHeapFreed cont) : allHeapFreed (closureScope cv caps cont) := by
  unfold closureScope
  apply incCaps_allHeapFreed
  simp only [allHeapFreed]
  refine ⟨fun _ => ?_, ?_⟩
  · -- the leading `dec cv` discharges `hasDec (.dec cv …) cv`
    unfold hasDec
    simp only [countDecs, beq_self_eq_true, if_true]
    omega
  · exact decCaps_allHeapFreed caps cont h

end AlmidePerceusBelt
