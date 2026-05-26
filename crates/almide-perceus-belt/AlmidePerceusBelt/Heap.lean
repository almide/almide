/-
  AlmidePerceusBelt — Heap execution model.
  Proves Perceus operations maintain heap safety.
-/
import AlmidePerceusBelt.FnBody

namespace AlmidePerceusBelt

abbrev Addr := Nat

structure Heap where
  rc : Addr → Nat
  next : Addr

def Heap.empty : Heap := ⟨fun _ => 0, 0⟩

def Heap.alloc (h : Heap) : Heap × Addr :=
  (⟨fun x => if x == h.next then 1 else h.rc x, h.next + 1⟩, h.next)

def Heap.incRef (h : Heap) (a : Addr) : Heap :=
  ⟨fun x => if x == a then h.rc a + 1 else h.rc x, h.next⟩

def Heap.decRef (h : Heap) (a : Addr) : Heap :=
  ⟨fun x => if x == a then h.rc a - 1 else h.rc x, h.next⟩

abbrev Env := VarId → Option Addr

def execute : FnBody → Heap → Env → Heap
  | .vdecl v _ body, h, env =>
    let (h', a) := h.alloc
    execute body h' (fun x => if x == v then some a else env x)
  | .inc v body, h, env =>
    match env v with
    | some a => execute body (h.incRef a) env
    | none => execute body h env
  | .dec v body, h, env =>
    match env v with
    | some a => execute body (h.decRef a) env
    | none => execute body h env
  | .ret, h, _ | .nop, h, _ => h

-- ══════ Heap Proofs ══════

theorem alloc_sets_one (h : Heap) : (h.alloc).1.rc h.next = 1 := by
  simp [Heap.alloc]

theorem vdecl_dec_frees_heap (v : VarId) (h : Heap) (env : Env) :
    (execute (.vdecl v .string (.dec v .nop)) h env).rc h.next = 0 := by
  simp [execute, Heap.alloc, Heap.decRef]

theorem alias_frees_heap (v : VarId) (h : Heap) (env : Env) :
    (execute (.vdecl v .string (.inc v (.dec v (.dec v .nop)))) h env).rc h.next = 0 := by
  simp [execute, Heap.alloc, Heap.incRef, Heap.decRef]

theorem no_dec_leaks_heap (v : VarId) (h : Heap) (env : Env) :
    (execute (.vdecl v .string .nop) h env).rc h.next = 1 := by
  simp [execute, Heap.alloc]

theorem perceus_fixes_heap (v : VarId) (h : Heap) (env : Env) :
    (execute (insertDecBeforeEnd (.vdecl v .string .nop) v) h env).rc h.next = 0 := by
  simp [insertDecBeforeEnd, execute, Heap.alloc, Heap.decRef]

end AlmidePerceusBelt
