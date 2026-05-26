/-
  AlmidePerceusBelt — FnBody IR + Perceus proofs.
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
  | ret : FnBody | nop : FnBody

def countIncs : FnBody → VarId → Nat
  | .inc v b, t => (if v == t then 1 else 0) + countIncs b t
  | .vdecl _ _ b, t | .dec _ b, t => countIncs b t
  | .ret, _ | .nop, _ => 0

def countDecs : FnBody → VarId → Nat
  | .dec v b, t => (if v == t then 1 else 0) + countDecs b t
  | .vdecl _ _ b, t | .inc _ b, t => countDecs b t
  | .ret, _ | .nop, _ => 0

def isFreed (fb : FnBody) (v : VarId) : Prop := countDecs fb v = countIncs fb v + 1

def insertDecBeforeEnd : FnBody → VarId → FnBody
  | .vdecl w ty b, v => .vdecl w ty (insertDecBeforeEnd b v)
  | .inc w b, v => .inc w (insertDecBeforeEnd b v)
  | .dec w b, v => .dec w (insertDecBeforeEnd b v)
  | .ret, v => .dec v .ret
  | .nop, v => .dec v .nop

theorem insertDec_adds_one (fb : FnBody) (v : VarId) :
    countDecs (insertDecBeforeEnd fb v) v = countDecs fb v + 1 := by
  induction fb with
  | vdecl _ _ _ ih => simp [insertDecBeforeEnd, countDecs]; exact ih
  | inc _ _ ih => simp [insertDecBeforeEnd, countDecs]; exact ih
  | dec _ _ ih => simp [insertDecBeforeEnd, countDecs]; omega
  | ret => simp [insertDecBeforeEnd, countDecs]
  | nop => simp [insertDecBeforeEnd, countDecs]

theorem insertDec_keeps_incs (fb : FnBody) (v : VarId) :
    countIncs (insertDecBeforeEnd fb v) v = countIncs fb v := by
  induction fb with
  | vdecl _ _ _ ih => simp [insertDecBeforeEnd, countIncs]; exact ih
  | inc _ _ ih => simp [insertDecBeforeEnd, countIncs]; omega
  | dec _ _ ih => simp [insertDecBeforeEnd, countIncs]; exact ih
  | ret => simp [insertDecBeforeEnd, countIncs]
  | nop => simp [insertDecBeforeEnd, countIncs]

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

-- ═══ GENERAL INDUCTION ═══

def perceusTransform : FnBody → FnBody
  | .vdecl v ty body =>
    if ty.isHeap then .vdecl v ty (insertDecBeforeEnd (perceusTransform body) v)
    else .vdecl v ty (perceusTransform body)
  | .inc v body => .inc v (perceusTransform body)
  | .dec v body => .dec v (perceusTransform body)
  | .ret => .ret
  | .nop => .nop

def hasDec (fb : FnBody) (v : VarId) : Prop := countDecs fb v ≥ 1

theorem perceus_covers_vdecl (v : VarId) (ty : Ty) (body : FnBody)
    (h_heap : ty.isHeap = true) (h_fresh : countDecs body v = 0) :
    hasDec (perceusTransform (.vdecl v ty body)) v := by
  unfold hasDec perceusTransform; simp [h_heap]; sorry

theorem perceus_preserves_dec (fb : FnBody) (v : VarId) (h : countDecs fb v ≥ 1) :
    countDecs (perceusTransform fb) v ≥ 1 := by
  induction fb with
  | vdecl w ty body ih =>
    simp [perceusTransform]; split
    · sorry
    · simp [countDecs] at h ⊢; exact ih h
  | inc _ _ ih => simp [perceusTransform, countDecs] at h ⊢; exact ih h
  | dec _ _ ih => simp [perceusTransform, countDecs] at h ⊢; omega
  | ret => simp [perceusTransform, countDecs] at h
  | nop => simp [perceusTransform, countDecs] at h

theorem perceus_idempotent (v : VarId) (ty : Ty) (body : FnBody)
    (h_heap : ty.isHeap = true) :
    hasDec (perceusTransform (.vdecl v ty body)) v →
    hasDec (perceusTransform (perceusTransform (.vdecl v ty body))) v := by
  intro h; exact perceus_preserves_dec _ v h


end AlmidePerceusBelt
