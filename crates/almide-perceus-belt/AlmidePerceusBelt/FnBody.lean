/-
  AlmidePerceusBelt — Formal specification of Perceus RC rules.

  This module defines the FnBody continuation-based IR and the
  Perceus insertion rules. The soundness theorem states that
  applying these rules to a well-typed program produces output
  where every allocation is freed exactly once.

  References:
  - Reinking et al., "Perceus" (ICFP 2021)
  - Ullrich & de Moura, "Counting Immutable Beans" (IFL 2019)
-/

namespace AlmidePerceusBelt

-- Variable identifier
abbrev VarId := Nat

-- Simplified type system (heap vs non-heap)
inductive Ty where
  | int    : Ty
  | bool   : Ty
  | string : Ty
  | list   : Ty → Ty
  | record : List (String × Ty) → Ty
  | fn_    : List Ty → Ty → Ty
  | unit   : Ty
  deriving Repr, BEq

-- Is this type heap-allocated?
def Ty.isHeap : Ty → Bool
  | .string => true
  | .list _ => true
  | .record _ => true
  | .fn_ _ _ => true
  | _ => false

-- IR Expression (simplified)
inductive Expr where
  | var    : VarId → Expr
  | litInt : Int → Expr
  | litStr : String → Expr
  | call   : String → List Expr → Expr
  | list_  : List Expr → Expr
  deriving Repr

-- Continuation-based IR (Lean 4 / Koka style)
inductive FnBody where
  | vdecl  : VarId → Ty → Expr → FnBody → FnBody
  | assign : VarId → Expr → FnBody → FnBody
  | inc    : VarId → FnBody → FnBody
  | dec    : VarId → FnBody → FnBody
  | expr   : Expr → FnBody → FnBody
  | ret    : Expr → FnBody
  | nop    : FnBody
  deriving Repr

-- Variable table: maps VarId to type
abbrev VarTable := VarId → Ty

-- RC count for a variable: number of inc/dec operations
structure RcCount where
  incs : Nat
  decs : Nat
  deriving Repr

-- Collect RC operations from a FnBody chain
def collectRc (fb : FnBody) : VarId → RcCount :=
  go fb (fun _ => ⟨0, 0⟩)
where
  go : FnBody → (VarId → RcCount) → (VarId → RcCount)
  | .inc v body, acc => go body (fun x =>
      if x == v then { acc x with incs := (acc x).incs + 1 }
      else acc x)
  | .dec v body, acc => go body (fun x =>
      if x == v then { acc x with decs := (acc x).decs + 1 }
      else acc x)
  | .vdecl _ _ _ body, acc => go body acc
  | .assign _ _ body, acc => go body acc
  | .expr _ body, acc => go body acc
  | .ret _, acc => acc
  | .nop, acc => acc

-- Collect heap-typed VDecl variables
def collectHeapVars (fb : FnBody) (vt : VarTable) : List VarId :=
  go fb []
where
  go : FnBody → List VarId → List VarId
  | .vdecl v ty _ body, acc =>
      go body (if ty.isHeap then v :: acc else acc)
  | .inc _ body, acc | .dec _ body, acc
  | .assign _ _ body, acc | .expr _ body, acc => go body acc
  | .ret _, acc | .nop, acc => acc

-- ═══════════════════════════════════════════════════════════
-- SOUNDNESS DEFINITION
-- ═══════════════════════════════════════════════════════════

-- RC balance: for a variable v with initial RC = 1,
-- after all operations: RC = 1 + incs - decs
-- For non-returned vars: RC must reach 0 (freed)
-- For returned vars: RC must be 1 (transferred to caller)

def rcBalanced (fb : FnBody) (vt : VarTable) (returnedVars : List VarId) : Prop :=
  ∀ v : VarId,
    let rc := collectRc fb v
    let isHeap := (vt v).isHeap
    let isReturned := v ∈ returnedVars
    isHeap →
      if isReturned then
        -- Returned: RC stays at 1 (caller takes ownership)
        1 + rc.incs = rc.decs + 1
      else
        -- Not returned: RC reaches 0 (freed)
        1 + rc.incs = rc.decs

-- ═══════════════════════════════════════════════════════════
-- PERCEUS RULES (specification, not implementation)
-- ═══════════════════════════════════════════════════════════

-- Rule 1: Inc on alias
-- If VDecl(y, ty, Var(x), body) and ty.isHeap, insert Inc(x) before VDecl
def rule1 : FnBody → VarTable → FnBody := sorry

-- Rule 2: Dec at last use
-- For each heap VDecl v, insert Dec(v) after the last reference to v
def rule2 : FnBody → VarTable → FnBody := sorry

-- Rule 3: Dec on assign
-- If Assign(x, e, body) and (vt x).isHeap, insert Dec(x) before Assign
def rule3 : FnBody → VarTable → FnBody := sorry

-- Combined Perceus transformation
def perceus (fb : FnBody) (vt : VarTable) : FnBody :=
  rule3 (rule2 (rule1 fb vt) vt) vt

-- ═══════════════════════════════════════════════════════════
-- SOUNDNESS THEOREM
-- ═══════════════════════════════════════════════════════════

/--
  **Theorem (Perceus Soundness)**:
  For any well-formed FnBody `fb` and VarTable `vt`,
  applying the Perceus rules produces output where
  every heap allocation is freed exactly once.
-/
theorem perceus_sound
    (fb : FnBody) (vt : VarTable) (returned : List VarId) :
    rcBalanced (perceus fb vt) vt returned := by
  sorry -- Proof to be completed

/--
  **Theorem (Inc-Dec Cancellation)**:
  If a variable is immutable and single-use,
  the Inc/Dec pair is identity and can be removed.
-/
theorem inc_dec_cancel
    (fb : FnBody) (v : VarId) (vt : VarTable) :
    let rc := collectRc fb v
    rc.incs ≥ 1 → rc.decs ≥ 1 →
    -- Removing one inc and one dec preserves balance
    let rc' := { incs := rc.incs - 1, decs := rc.decs - 1 }
    1 + rc'.incs = 1 + rc.incs - 1 := by
  sorry -- Proof to be completed

end AlmidePerceusBelt
