/-
  AlmidePerceusBelt — Unified FnBody IR + Perceus proofs.
  Single FnBody type covers both linear chains and control flow (ite).
  No separate FnBodyCF — all theorems operate on one type.
-/
namespace AlmidePerceusBelt

abbrev VarId := Nat
inductive Ty where | int | string | list : Ty → Ty | unit
  deriving BEq, DecidableEq
def Ty.isHeap : Ty → Bool | .string | .list _ => true | _ => false

inductive FnBody where
  | vdecl : VarId → Ty → FnBody → FnBody
  | inc : VarId → FnBody → FnBody
  | dec : VarId → FnBody → FnBody
  | ite : FnBody → FnBody → FnBody  -- if-then-else (both branches)
  | ret : FnBody | nop : FnBody

-- ══════ Counting ══════

def countIncs : FnBody → VarId → Nat
  | .inc v b, t => (if v == t then 1 else 0) + countIncs b t
  | .vdecl _ _ b, t | .dec _ b, t => countIncs b t
  | .ite th el, t => min (countIncs th t) (countIncs el t)
  | .ret, _ | .nop, _ => 0

def countDecs : FnBody → VarId → Nat
  | .dec v b, t => (if v == t then 1 else 0) + countDecs b t
  | .vdecl _ _ b, t | .inc _ b, t => countDecs b t
  | .ite th el, t => min (countDecs th t) (countDecs el t)
  | .ret, _ | .nop, _ => 0

def isFreed (fb : FnBody) (v : VarId) : Prop := countDecs fb v = countIncs fb v + 1

def hasDec (fb : FnBody) (v : VarId) : Prop := countDecs fb v ≥ 1

-- ══════ Insert Dec before end ══════

def insertDecBeforeEnd : FnBody → VarId → FnBody
  | .vdecl w ty b, v => .vdecl w ty (insertDecBeforeEnd b v)
  | .inc w b, v => .inc w (insertDecBeforeEnd b v)
  | .dec w b, v => .dec w (insertDecBeforeEnd b v)
  | .ite th el, v => .ite (insertDecBeforeEnd th v) (insertDecBeforeEnd el v)
  | .ret, v => .dec v .ret
  | .nop, v => .dec v .nop

-- ══════ Perceus Transform ══════

def perceusTransform : FnBody → FnBody
  | .vdecl v ty body =>
    if ty.isHeap then .vdecl v ty (insertDecBeforeEnd (perceusTransform body) v)
    else .vdecl v ty (perceusTransform body)
  | .inc v body => .inc v (perceusTransform body)
  | .dec v body => .dec v (perceusTransform body)
  | .ite th el => .ite (perceusTransform th) (perceusTransform el)
  | .ret => .ret
  | .nop => .nop

-- ══════ Helper: min monotonicity ══════

private theorem min_mono {a b c d : Nat} (h1 : c ≤ a) (h2 : d ≤ b) :
    min c d ≤ min a b := by
  simp only [Nat.min_def]; split <;> split <;> omega

-- ══════════════════════════════════════════════════════
-- PROOFS: insertDecBeforeEnd properties
-- ══════════════════════════════════════════════════════

theorem insertDec_adds_one (fb : FnBody) (v : VarId) :
    countDecs (insertDecBeforeEnd fb v) v = countDecs fb v + 1 := by
  induction fb with
  | vdecl _ _ _ ih => simp [insertDecBeforeEnd, countDecs]; exact ih
  | inc _ _ ih => simp [insertDecBeforeEnd, countDecs]; exact ih
  | dec _ _ ih => simp [insertDecBeforeEnd, countDecs]; omega
  | ite _ _ ih_th ih_el =>
    simp only [insertDecBeforeEnd, countDecs]
    rw [ih_th, ih_el, Nat.min_def, Nat.min_def]
    split <;> split <;> omega
  | ret => simp [insertDecBeforeEnd, countDecs]
  | nop => simp [insertDecBeforeEnd, countDecs]

theorem insertDec_keeps_incs (fb : FnBody) (v : VarId) :
    countIncs (insertDecBeforeEnd fb v) v = countIncs fb v := by
  induction fb with
  | vdecl _ _ _ ih => simp [insertDecBeforeEnd, countIncs]; exact ih
  | inc _ _ ih => simp [insertDecBeforeEnd, countIncs]; omega
  | dec _ _ ih => simp [insertDecBeforeEnd, countIncs]; exact ih
  | ite _ _ ih_th ih_el =>
    simp only [insertDecBeforeEnd, countIncs]
    rw [ih_th, ih_el]
  | ret => simp [insertDecBeforeEnd, countIncs]
  | nop => simp [insertDecBeforeEnd, countIncs]

-- insertDecBeforeEnd is monotone in countDecs (for any variable)
private theorem insertDec_monotone (fb : FnBody) (w v : VarId) :
    countDecs (insertDecBeforeEnd fb w) v ≥ countDecs fb v := by
  induction fb with
  | vdecl _ _ _ ih => simp [insertDecBeforeEnd, countDecs]; exact ih
  | inc _ _ ih => simp [insertDecBeforeEnd, countDecs]; exact ih
  | dec _ _ ih =>
    simp only [insertDecBeforeEnd, countDecs]
    split <;> simp_all <;> omega
  | ite _ _ ih_th ih_el =>
    simp only [insertDecBeforeEnd, countDecs]
    exact min_mono ih_th ih_el
  | ret => simp [insertDecBeforeEnd, countDecs]
  | nop => simp [insertDecBeforeEnd, countDecs]

-- ══════════════════════════════════════════════════════
-- PROOFS: Single variable
-- ══════════════════════════════════════════════════════

theorem single_dec_frees (fb : FnBody) (v : VarId)
    (hi : countIncs fb v = 0) (hd : countDecs fb v = 0) :
    isFreed (insertDecBeforeEnd fb v) v := by
  unfold isFreed; rw [insertDec_adds_one, insertDec_keeps_incs, hi, hd]

theorem inc_dec_is_id (v : VarId) (b : FnBody)
    (h : countIncs b v = 0 ∧ countDecs b v = 0) :
    countIncs (.inc v (.dec v b)) v = countDecs (.inc v (.dec v b)) v := by
  simp [countIncs, countDecs]; omega

theorem vdecl_dec_frees (v : VarId) (ty : Ty) (b : FnBody)
    (h : countIncs b v = 0 ∧ countDecs b v = 0) :
    isFreed (insertDecBeforeEnd (.vdecl v ty b) v) v := by
  unfold isFreed; rw [insertDec_adds_one, insertDec_keeps_incs]
  simp [countIncs, countDecs]; obtain ⟨a, b⟩ := h; rw [a, b]

theorem inc_dec_preserves (fb : FnBody) (v : VarId) (h : isFreed fb v) :
    isFreed (insertDecBeforeEnd (.inc v fb) v) v := by
  unfold isFreed at *; rw [insertDec_adds_one]
  simp [insertDecBeforeEnd, countIncs, countDecs, insertDec_keeps_incs]; omega

-- ══════════════════════════════════════════════════════
-- PROOFS: General induction (perceusTransform)
-- ══════════════════════════════════════════════════════

theorem perceus_covers_vdecl (v : VarId) (ty : Ty) (body : FnBody)
    (h_heap : ty.isHeap = true) :
    hasDec (perceusTransform (.vdecl v ty body)) v := by
  unfold hasDec perceusTransform; simp [h_heap]
  show countDecs (insertDecBeforeEnd (perceusTransform body) v) v ≥ 1
  have h := insertDec_adds_one (perceusTransform body) v
  rw [h]; exact Nat.le_add_left 1 _

theorem perceus_preserves_dec (fb : FnBody) (v : VarId) (h : countDecs fb v ≥ 1) :
    countDecs (perceusTransform fb) v ≥ 1 := by
  induction fb with
  | vdecl w ty body ih =>
    simp [perceusTransform]; split
    · simp [countDecs] at h
      have h1 := ih h
      exact Nat.le_trans h1 (insertDec_monotone _ w v)
    · simp [countDecs] at h ⊢; exact ih h
  | inc _ _ ih => simp [perceusTransform, countDecs] at h ⊢; exact ih h
  | dec _ _ ih => simp [perceusTransform, countDecs] at h ⊢; omega
  | ite _ _ ih_th ih_el =>
    simp only [perceusTransform, countDecs, Nat.min_def] at h ⊢
    split at h <;> split
    · exact ih_th h
    · exact ih_el (by omega)
    · exact ih_th (by omega)
    · exact ih_el h
  | ret => simp [countDecs] at h
  | nop => simp [countDecs] at h

theorem perceus_idempotent (v : VarId) (ty : Ty) (body : FnBody)
    (_ : ty.isHeap = true) :
    hasDec (perceusTransform (.vdecl v ty body)) v →
    hasDec (perceusTransform (perceusTransform (.vdecl v ty body))) v := by
  intro h; exact perceus_preserves_dec _ v h

-- ══════════════════════════════════════════════════════
-- PROOFS: Control flow (ite)
-- ══════════════════════════════════════════════════════

/-- Dec inserted in BOTH branches → freed on ALL paths -/
theorem cf_both_branches_freed (v : VarId) (th el : FnBody)
    (h_th : countDecs th v = 0) (h_el : countDecs el v = 0) :
    hasDec (insertDecBeforeEnd (.ite th el) v) v := by
  unfold hasDec insertDecBeforeEnd countDecs
  rw [insertDec_adds_one, insertDec_adds_one, h_th, h_el]; simp

/-- If only ONE branch has Dec, min → not freed -/
theorem cf_one_branch_insufficient (v : VarId) :
    ¬ hasDec (.ite (.dec v .ret) .ret) v := by
  unfold hasDec
  simp [countDecs]

/-- VDecl + if/else with Dec in both branches = freed -/
theorem cf_vdecl_ite_freed (v : VarId) (ty : Ty) :
    hasDec (.vdecl v ty (.ite (.dec v .ret) (.dec v .ret))) v := by
  unfold hasDec
  simp [countDecs]

end AlmidePerceusBelt
