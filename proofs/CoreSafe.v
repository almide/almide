(* Almide trust spine — #4014: a typed core of the SOURCE language, and the
   theorem that its well-typed programs are MemorySafe.

   Until now the spine reasoned about witnesses the compiler emits; nothing
   said what a source program MEANS. This file fixes a small core of Almide
   — ints, bools, lists of ints with value semantics, boxes (a block holding
   one value of any type, lists and boxes included), `let`, assignment,
   element store, `if`, `while`, indexing with its bounds check, and
   functions (one parameter, an expression body, recursion allowed) — gives
   it a big-step semantics that EMITS the heap trace of MemorySafe.v, a type
   system, and proves:

     typed_core_memory_safe : a well-typed program's every run — one that
       finishes, or one a failed bounds check ends — has a trace that is
       MemorySafe: all four clauses (spatial, temporal, initialization,
       type), not only RC balance.

   The memory the semantics touches is the one the compiled code touches: a
   list is one block, HDR header bytes then W bytes per element; `make`
   allocates a fresh block and initializes it; `xs[i]` checks `0 <= i < len`
   and loads one element; `len(xs)` loads the header; `xs[i] = v` is value
   semantics, so it copies the block into a fresh one and stores the element
   there (the uniqueness-guarded in-place store is a refinement of this,
   CowSafety.v). A box is a block with a header and one slot.

   Blocks are REFERENCE COUNTED and FREED, the way the compiled code does it.
   Reading a variable that holds a list or a box shares its block (EInc)
   instead of copying it. Every reference is given back when it dies: an
   operand an operator consumes (`xs[i]`, `len(xs)`, `unbox(b)`), a
   variable's old value when it is reassigned or element-stored, a call's
   parameter when the call returns. Giving back a reference to a block
   someone else still holds is one EDec. Giving back the LAST reference to
   a box reads its slot, frees it, and gives back the reference the slot
   held — recursively, the way a Perceus drop does — so the semantics
   decides on the heap, and eval and exec are indexed by it.

   The proof's invariant is counting: every block's count is at least the
   references still held to it — by the program's variables, by caller
   frames, by values computed but not yet consumed, and by the slots of
   live boxes (a ghost store records each box's content). So a held
   reference always points at a live block. Two controls show the
   invariant is what the theorem rests on: leave out the EInc of a shared
   read (missing_inc_is_use_after_free) or of a value stored into a box
   (box_store_without_share_is_use_after_free, the shape of #2133) and a
   later read touches freed memory.

   HONEST SCOPE. A core, not the language: no closures, records with more
   than one field, variants, strings, maps or effects; a function takes one
   or two arguments (ECall, ECall2: each argument is owned by the callee for
   the call and given back, in order, when it returns) and its body is an
   expression; list elements are ints; a `let`
   binds a fresh name (a front end renames shadowing away). A binding is
   released when it is overwritten, not when its scope ends, and an
   aborting run releases nothing: both only leak, and a leak is not a
   MemorySafe violation. Only terminating and aborting runs are covered (the
   semantics is inductive). And nothing here connects the core to the
   compiler: lowering is still checked per build by the certificates, as
   #4014 allows. *)

From AlmideTrust Require Import MemorySafe.
From Stdlib Require Import List ZArith Lia Bool Arith.
Import ListNotations.
Open Scope Z_scope.

(* ── Layout ─────────────────────────────────────────────────────────────── *)

Definition HDR : Z := 8.
Definition W : Z := 8.
Definition RL : Repr := 1%nat.   (* a list block *)
Definition RB : Repr := 2%nat.   (* a box block: a header and one slot *)
Definition bsize (n : nat) : Z := HDR + Z.of_nat n * W.

(* ── Syntax ─────────────────────────────────────────────────────────────── *)

Definition Var := nat.

Inductive binop := Add | Lt | Make | Index.

Inductive ty := TInt | TBool | TList | TBox (t : ty).

Inductive expr :=
  | EInt (z : Z)
  | EBool (b : bool)
  | EVar (x : Var)
  | EBin (op : binop) (e1 e2 : expr)   (* Make n v = list.repeat(v, n); Index l i = l[i] *)
  | ELen (e : expr)
  | EIf (c e1 e2 : expr)               (* if c then e1 else e2 *)
  | ECall (f : nat) (a : expr)         (* a call of function f on one argument *)
  | ECall2 (f : nat) (a1 a2 : expr)    (* a call of two-argument function f *)
  | EBox (e : expr)                    (* a fresh box holding e's value *)
  | EUnbox (e : expr).                 (* the value a box holds *)

(* A function: one parameter, an expression body, a declared signature.
   Recursion (direct or mutual) is a call to a function in the same table. *)
Record FunDef := { fparam : Var; fpty : ty; fbody : expr; frty : ty }.

(* A two-argument function: two distinct parameters, both owned by the
   callee for the call and given back, in order, when it returns. *)
Record FunDef2 := { f2p1 : Var; f2t1 : ty; f2p2 : Var; f2t2 : ty; f2body : expr; f2rty : ty }.

Inductive stmt :=
  | SSkip
  | SLet (x : Var) (e : expr)
  | SAssign (x : Var) (e : expr)
  | SSet (x : Var) (i v : expr)        (* x[i] = v *)
  | SSeq (s1 s2 : stmt)
  | SIf (c : expr) (s1 s2 : stmt)
  | SWhile (c : expr) (s : stmt).

(* ── Values and the semantics ───────────────────────────────────────────── *)

Inductive val := VInt (z : Z) | VBool (b : bool) | VList (b : Blk) (vs : list Z) | VBox (b : Blk) (v : val).

Definition Env := Var -> option val.
Definition upd (r : Env) (x : Var) (v : val) : Env := fun y => if Nat.eqb y x then Some v else r y.

Inductive res := Val (v : val) (n : nat) | Abort.

Fixpoint set_nth (vs : list Z) (i : nat) (k : Z) : list Z :=
  match vs, i with
  | [], _ => []
  | _ :: r, O => k :: r
  | a :: r, S j => a :: set_nth r j k
  end.

(* A block's count (0 if it was never allocated). *)
Definition rcof (h : Heap) (b : Blk) : Z := match h b with Some x => rc x | None => 0 end.

(* Taking one more reference to a value. *)
Definition dup (v : val) : list Ev :=
  match v with VList b _ | VBox b _ => [EInc b] | _ => [] end.

(* Giving one back, on heap h. The last reference to a box reads the slot,
   frees the box, and gives back the reference the slot held. *)
Fixpoint dropv (h : Heap) (v : val) : list Ev :=
  match v with
  | VList b _ => [EDec b]
  | VBox b w =>
      if Z.eqb (rcof h b) 1
      then ELoad b HDR W RB :: EDec b :: dropv (step h (EDec b)) w
      else [EDec b]
  | _ => []
  end.

Definition dropo (h : Heap) (o : option val) : list Ev :=
  match o with Some v => dropv h v | None => [] end.

(* One operator on evaluated operands, from fresh-block counter n. An
   operator consumes its operands: `xs[i]` gives the list's reference back
   once the element is read. *)
Inductive binop_step : binop -> val -> val -> nat -> list Ev -> res -> Prop :=
  | BAdd : forall a b n, binop_step Add (VInt a) (VInt b) n [] (Val (VInt (a + b)) n)
  | BLt : forall a b n, binop_step Lt (VInt a) (VInt b) n [] (Val (VBool (a <? b)) n)
  | BMake : forall k x n, 0 <= k ->
      binop_step Make (VInt k) (VInt x) n
        [EAlloc n (bsize (Z.to_nat k)) RL; EStore n 0 (bsize (Z.to_nat k)) RL]
        (Val (VList n (repeat x (Z.to_nat k))) (S n))
  | BMakeNeg : forall k x n, k < 0 -> binop_step Make (VInt k) (VInt x) n [] Abort
  | BIndex : forall b vs i n, 0 <= i < Z.of_nat (length vs) ->
      binop_step Index (VList b vs) (VInt i) n
        [ELoad b (HDR + i * W) W RL; EDec b] (Val (VInt (nth (Z.to_nat i) vs 0)) n)
  | BIndexOut : forall b vs i n, ~ (0 <= i < Z.of_nat (length vs)) ->
      binop_step Index (VList b vs) (VInt i) n [] Abort.

(* The program's functions, by name. Everything below is stated for an
   arbitrary table; the theorem asks only that every body is well typed. *)
Section Calls.
Variable funs : nat -> option FunDef.
Variable funs2 : nat -> option FunDef2.

Definition empty_env : Env := fun _ => None.

(* eval h r n e t o: on heap h, environment r and fresh-block counter n, e
   emits trace t and ends in o. A later step starts on the heap the earlier
   steps' trace leaves (fold_left step t h). *)
Inductive eval : Heap -> Env -> nat -> expr -> list Ev -> res -> Prop :=
  | EvInt : forall h r n z, eval h r n (EInt z) [] (Val (VInt z) n)
  | EvBool : forall h r n b, eval h r n (EBool b) [] (Val (VBool b) n)
  (* reading a variable shares its value: one more reference *)
  | EvVar : forall h r n x v, r x = Some v -> eval h r n (EVar x) (dup v) (Val v n)
  | EvBin : forall h r n op e1 e2 t1 v1 n1 t2 v2 n2 t3 o,
      eval h r n e1 t1 (Val v1 n1) -> eval (fold_left step t1 h) r n1 e2 t2 (Val v2 n2) ->
      binop_step op v1 v2 n2 t3 o ->
      eval h r n (EBin op e1 e2) (t1 ++ t2 ++ t3) o
  | EvBinAbort1 : forall h r n op e1 e2 t1,
      eval h r n e1 t1 Abort -> eval h r n (EBin op e1 e2) t1 Abort
  | EvBinAbort2 : forall h r n op e1 e2 t1 v1 n1 t2,
      eval h r n e1 t1 (Val v1 n1) -> eval (fold_left step t1 h) r n1 e2 t2 Abort ->
      eval h r n (EBin op e1 e2) (t1 ++ t2) Abort
  | EvLen : forall h r n e t b vs n1,
      eval h r n e t (Val (VList b vs) n1) ->
      eval h r n (ELen e) (t ++ [ELoad b 0 HDR RL; EDec b]) (Val (VInt (Z.of_nat (length vs))) n1)
  | EvLenAbort : forall h r n e t, eval h r n e t Abort -> eval h r n (ELen e) t Abort
  | EvIfT : forall h r n c e1 e2 t n1 t1 o,
      eval h r n c t (Val (VBool true) n1) -> eval (fold_left step t h) r n1 e1 t1 o ->
      eval h r n (EIf c e1 e2) (t ++ t1) o
  | EvIfF : forall h r n c e1 e2 t n1 t2 o,
      eval h r n c t (Val (VBool false) n1) -> eval (fold_left step t h) r n1 e2 t2 o ->
      eval h r n (EIf c e1 e2) (t ++ t2) o
  | EvIfAbort : forall h r n c e1 e2 t, eval h r n c t Abort -> eval h r n (EIf c e1 e2) t Abort
  (* A call: the argument, moved into a fresh environment holding only the
     parameter; the body; then the parameter's reference is given back. *)
  | EvCall : forall h r n f d a t1 v n1 t2 v' n2,
      funs f = Some d -> eval h r n a t1 (Val v n1) ->
      eval (fold_left step t1 h) (upd empty_env (fparam d) v) n1 (fbody d) t2 (Val v' n2) ->
      eval h r n (ECall f a) (t1 ++ t2 ++ dropv (fold_left step t2 (fold_left step t1 h)) v) (Val v' n2)
  | EvCallBodyAbort : forall h r n f d a t1 v n1 t2,
      funs f = Some d -> eval h r n a t1 (Val v n1) ->
      eval (fold_left step t1 h) (upd empty_env (fparam d) v) n1 (fbody d) t2 Abort ->
      eval h r n (ECall f a) (t1 ++ t2) Abort
  | EvCallAbort : forall h r n f a t1, eval h r n a t1 Abort -> eval h r n (ECall f a) t1 Abort
  (* A two-argument call: both arguments, left to right, moved into a fresh
     environment holding only the two parameters; the body; then the first
     parameter's reference given back, then the second's. *)
  | EvCall2 : forall h r n f d a1 a2 t1 v1 n1 t2 v2 n2 t3 v' n3,
      funs2 f = Some d -> eval h r n a1 t1 (Val v1 n1) ->
      eval (fold_left step t1 h) r n1 a2 t2 (Val v2 n2) ->
      eval (fold_left step t2 (fold_left step t1 h)) (upd (upd empty_env (f2p1 d) v1) (f2p2 d) v2) n2
           (f2body d) t3 (Val v' n3) ->
      eval h r n (ECall2 f a1 a2)
        (t1 ++ t2 ++ t3 ++
         dropv (fold_left step t3 (fold_left step t2 (fold_left step t1 h))) v1 ++
         dropv (fold_left step (dropv (fold_left step t3 (fold_left step t2 (fold_left step t1 h))) v1)
                  (fold_left step t3 (fold_left step t2 (fold_left step t1 h)))) v2)
        (Val v' n3)
  | EvCall2BodyAbort : forall h r n f d a1 a2 t1 v1 n1 t2 v2 n2 t3,
      funs2 f = Some d -> eval h r n a1 t1 (Val v1 n1) ->
      eval (fold_left step t1 h) r n1 a2 t2 (Val v2 n2) ->
      eval (fold_left step t2 (fold_left step t1 h)) (upd (upd empty_env (f2p1 d) v1) (f2p2 d) v2) n2
           (f2body d) t3 Abort ->
      eval h r n (ECall2 f a1 a2) (t1 ++ t2 ++ t3) Abort
  | EvCall2Abort1 : forall h r n f a1 a2 t1, eval h r n a1 t1 Abort -> eval h r n (ECall2 f a1 a2) t1 Abort
  | EvCall2Abort2 : forall h r n f a1 a2 t1 v1 n1 t2,
      eval h r n a1 t1 (Val v1 n1) -> eval (fold_left step t1 h) r n1 a2 t2 Abort ->
      eval h r n (ECall2 f a1 a2) (t1 ++ t2) Abort
  (* A box: a fresh block, initialized, the value MOVED into its slot. *)
  | EvBox : forall h r n e t v n1,
      eval h r n e t (Val v n1) ->
      eval h r n (EBox e) (t ++ [EAlloc n1 (bsize 1) RB; EStore n1 0 (bsize 1) RB]) (Val (VBox n1 v) (S n1))
  | EvBoxAbort : forall h r n e t, eval h r n e t Abort -> eval h r n (EBox e) t Abort
  (* Unboxing reads the slot, takes a reference to what it holds, and gives
     the box back. *)
  | EvUnbox : forall h r n e t b w n1,
      eval h r n e t (Val (VBox b w) n1) ->
      eval h r n (EUnbox e)
        (t ++ ELoad b HDR W RB :: dup w ++ dropv (fold_left step (dup w) (fold_left step t h)) (VBox b w))
        (Val w n1)
  | EvUnboxAbort : forall h r n e t, eval h r n e t Abort -> eval h r n (EUnbox e) t Abort.

Inductive sres := Normal (r : Env) (n : nat) | SAbort.

Inductive exec : Heap -> Env -> nat -> stmt -> list Ev -> sres -> Prop :=
  | XSkip : forall h r n, exec h r n SSkip [] (Normal r n)
  | XLet : forall h r n x e t v n1,
      eval h r n e t (Val v n1) -> exec h r n (SLet x e) t (Normal (upd r x v) n1)
  (* the new value first, then the old one's reference given back: `x = x`
     shares before it releases *)
  | XAssign : forall h r n x e t v n1,
      eval h r n e t (Val v n1) ->
      exec h r n (SAssign x e) (t ++ dropo (fold_left step t h) (r x)) (Normal (upd r x v) n1)
  | XLetAbort : forall h r n x e t, eval h r n e t Abort -> exec h r n (SLet x e) t SAbort
  | XAssignAbort : forall h r n x e t, eval h r n e t Abort -> exec h r n (SAssign x e) t SAbort
  | XSet : forall h r n x b vs ei ev t1 i n1 t2 k n2,
      r x = Some (VList b vs) ->
      eval h r n ei t1 (Val (VInt i) n1) -> eval (fold_left step t1 h) r n1 ev t2 (Val (VInt k) n2) ->
      0 <= i < Z.of_nat (length vs) ->
      exec h r n (SSet x ei ev)
        (t1 ++ t2 ++ [ELoad b 0 (bsize (length vs)) RL;
                      EAlloc n2 (bsize (length vs)) RL;
                      EStore n2 0 (bsize (length vs)) RL;
                      EStore n2 (HDR + i * W) W RL;
                      EDec b])
        (Normal (upd r x (VList n2 (set_nth vs (Z.to_nat i) k))) (S n2))
  | XSetOut : forall h r n x b vs ei ev t1 i n1 t2 k n2,
      r x = Some (VList b vs) ->
      eval h r n ei t1 (Val (VInt i) n1) -> eval (fold_left step t1 h) r n1 ev t2 (Val (VInt k) n2) ->
      ~ (0 <= i < Z.of_nat (length vs)) ->
      exec h r n (SSet x ei ev) (t1 ++ t2) SAbort
  | XSetAbort1 : forall h r n x ei ev t1,
      eval h r n ei t1 Abort -> exec h r n (SSet x ei ev) t1 SAbort
  | XSetAbort2 : forall h r n x ei ev t1 v n1 t2,
      eval h r n ei t1 (Val v n1) -> eval (fold_left step t1 h) r n1 ev t2 Abort ->
      exec h r n (SSet x ei ev) (t1 ++ t2) SAbort
  | XSeq : forall h r n s1 s2 t1 r1 n1 t2 o,
      exec h r n s1 t1 (Normal r1 n1) -> exec (fold_left step t1 h) r1 n1 s2 t2 o ->
      exec h r n (SSeq s1 s2) (t1 ++ t2) o
  | XSeqAbort : forall h r n s1 s2 t1, exec h r n s1 t1 SAbort -> exec h r n (SSeq s1 s2) t1 SAbort
  | XIfT : forall h r n c s1 s2 t n1 t1 o,
      eval h r n c t (Val (VBool true) n1) -> exec (fold_left step t h) r n1 s1 t1 o ->
      exec h r n (SIf c s1 s2) (t ++ t1) o
  | XIfF : forall h r n c s1 s2 t n1 t2 o,
      eval h r n c t (Val (VBool false) n1) -> exec (fold_left step t h) r n1 s2 t2 o ->
      exec h r n (SIf c s1 s2) (t ++ t2) o
  | XIfAbort : forall h r n c s1 s2 t, eval h r n c t Abort -> exec h r n (SIf c s1 s2) t SAbort
  | XWhileF : forall h r n c s t n1,
      eval h r n c t (Val (VBool false) n1) -> exec h r n (SWhile c s) t (Normal r n1)
  | XWhileT : forall h r n c s t n1 t1 r1 n2 t2 o,
      eval h r n c t (Val (VBool true) n1) -> exec (fold_left step t h) r n1 s t1 (Normal r1 n2) ->
      exec (fold_left step t1 (fold_left step t h)) r1 n2 (SWhile c s) t2 o ->
      exec h r n (SWhile c s) (t ++ t1 ++ t2) o
  | XWhileBodyAbort : forall h r n c s t n1 t1,
      eval h r n c t (Val (VBool true) n1) -> exec (fold_left step t h) r n1 s t1 SAbort ->
      exec h r n (SWhile c s) (t ++ t1) SAbort
  | XWhileAbort : forall h r n c s t, eval h r n c t Abort -> exec h r n (SWhile c s) t SAbort.

(* ── Types ──────────────────────────────────────────────────────────────── *)

Definition Ctx := Var -> option ty.
Definition tupd (g : Ctx) (x : Var) (t : ty) : Ctx := fun y => if Nat.eqb y x then Some t else g y.

Inductive optype : binop -> ty -> ty -> ty -> Prop :=
  | OAdd : optype Add TInt TInt TInt
  | OLt : optype Lt TInt TInt TBool
  | OMake : optype Make TInt TInt TList
  | OIndex : optype Index TList TInt TInt.

Inductive etyped (g : Ctx) : expr -> ty -> Prop :=
  | TyInt : forall z, etyped g (EInt z) TInt
  | TyBool : forall b, etyped g (EBool b) TBool
  | TyVar : forall x t, g x = Some t -> etyped g (EVar x) t
  | TyBin : forall op e1 e2 t1 t2 t,
      optype op t1 t2 t -> etyped g e1 t1 -> etyped g e2 t2 -> etyped g (EBin op e1 e2) t
  | TyLen : forall e, etyped g e TList -> etyped g (ELen e) TInt
  | TyEIf : forall c e1 e2 t, etyped g c TBool -> etyped g e1 t -> etyped g e2 t -> etyped g (EIf c e1 e2) t
  | TyCall : forall f d a, funs f = Some d -> etyped g a (fpty d) -> etyped g (ECall f a) (frty d)
  | TyCall2 : forall f d a1 a2, funs2 f = Some d -> etyped g a1 (f2t1 d) -> etyped g a2 (f2t2 d) ->
      etyped g (ECall2 f a1 a2) (f2rty d)
  | TyBox : forall e t, etyped g e t -> etyped g (EBox e) (TBox t)
  | TyUnbox : forall e t, etyped g e (TBox t) -> etyped g (EUnbox e) t.

Inductive styped : Ctx -> stmt -> Ctx -> Prop :=
  | TySkip : forall g, styped g SSkip g
  | TyLet : forall g x e t, g x = None -> etyped g e t -> styped g (SLet x e) (tupd g x t)
  | TyAssign : forall g x e t, g x = Some t -> etyped g e t -> styped g (SAssign x e) g
  | TySet : forall g x i v, g x = Some TList -> etyped g i TInt -> etyped g v TInt ->
      styped g (SSet x i v) g
  | TySeq : forall g s1 g1 s2 g2, styped g s1 g1 -> styped g1 s2 g2 -> styped g (SSeq s1 s2) g2
  | TyIf : forall g c s1 g1 s2 g2, etyped g c TBool -> styped g s1 g1 -> styped g s2 g2 ->
      styped g (SIf c s1 s2) g
  | TyWhile : forall g c s g1, etyped g c TBool -> styped g s g1 -> styped g (SWhile c s) g.

(* Every function's body is well typed under its parameter alone. *)
Definition funs_ok : Prop :=
  forall f d, funs f = Some d -> etyped (fun y => if Nat.eqb y (fparam d) then Some (fpty d) else None) (fbody d) (frty d).

Hypothesis Hfuns : funs_ok.

(* The two parameters' context. *)
Definition ctx2 (d : FunDef2) : Ctx :=
  fun y => if Nat.eqb y (f2p2 d) then Some (f2t2 d) else if Nat.eqb y (f2p1 d) then Some (f2t1 d) else None.

(* Every two-argument function has distinct parameters and a body well typed
   under them alone. *)
Definition funs2_ok : Prop :=
  forall f d, funs2 f = Some d -> f2p1 d <> f2p2 d /\ etyped (ctx2 d) (f2body d) (f2rty d).

Hypothesis Hfuns2 : funs2_ok.

(* ── The invariant ──────────────────────────────────────────────────────── *)

(* Blocks below the counter have been allocated (some may since have been
   freed: their record stays, with count 0); above it, none. *)
Definition wfh (h : Heap) (n : nat) : Prop :=
  forall b, ((b < n)%nat -> exists x, h b = Some x) /\ ((n <= b)%nat -> h b = None).

(* The ghost store: what each box's slot holds (None for every other
   block). A box is immutable, so its entry never changes once made. *)
Definition Store := Blk -> option val.
Definition supd (s : Store) (b : Blk) (v : val) : Store := fun c => if Nat.eqb c b then Some v else s c.
Definition sext (s s' : Store) : Prop := forall c w, s c = Some w -> s' c = Some w.

Definition boxblk (h : Heap) (b : Blk) : Prop :=
  exists x, h b = Some x /\ size x = bsize 1 /\ repr x = RB /\ all_init (init x) 0 (size x).

(* A value of a type, against the store and the heap: a list is an
   allocated block of its length's size, of the list representation, fully
   initialized; a box is an allocated box block whose slot the store says
   holds the value's content, itself of the content type. That a block is
   still LIVE is not part of this; it follows from the counting below. *)
Fixpoint vty (s : Store) (h : Heap) (n : nat) (v : val) (t : ty) {struct t} : Prop :=
  match v, t with
  | VInt _, TInt => True
  | VBool _, TBool => True
  | VList b vs, TList =>
      (b < n)%nat /\ exists x, h b = Some x /\ size x = bsize (length vs) /\ repr x = RL /\
                               all_init (init x) 0 (size x)
  | VBox b w, TBox t' => (b < n)%nat /\ boxblk h b /\ s b = Some w /\ vty s h n w t'
  | _, _ => False
  end.

(* The store describes boxes below the counter, and their contents are
   well typed. *)
Definition sok (s : Store) (h : Heap) (n : nat) : Prop :=
  forall c w, s c = Some w -> (c < n)%nat /\ boxblk h c /\ exists t, vty s h n w t.

Definition envok (g : Ctx) (r : Env) (s : Store) (h : Heap) (n : nat) : Prop :=
  forall x t, g x = Some t -> exists v, r x = Some v /\ vty s h n v t.

(* The heap only grows: a block keeps its size and representation, and an
   initialized byte stays initialized. Its count is free to move. *)
Definition grows (h h' : Heap) : Prop :=
  forall b x, h b = Some x -> exists y, h' b = Some y /\ size y = size x /\ repr y = repr x /\
                                         (forall i, init x i = true -> init y i = true).

(* The counting. vref v b is how many references to b the value v is (its
   own block only — what a box holds is counted by the box); envrefs D r b
   how many the variables D of r hold; hrefs s h n b how many the slots of
   the LIVE boxes below n hold; cnt says every block's count covers K plus
   what the live boxes hold. *)
Definition vref (v : val) (b : Blk) : nat :=
  match v with VList b' _ | VBox b' _ => if Nat.eqb b' b then 1 else 0 | _ => 0 end.
Definition oref (o : option val) (b : Blk) : nat := match o with Some v => vref v b | None => 0 end.
Fixpoint envrefs (D : list Var) (r : Env) (b : Blk) : nat :=
  match D with [] => 0 | x :: D' => oref (r x) b + envrefs D' r b end.
Definition hterm (s : Store) (h : Heap) (c b : Blk) : nat :=
  if 0 <? rcof h c then oref (s c) b else 0.
Fixpoint hrefs (s : Store) (h : Heap) (n : nat) (b : Blk) : nat :=
  match n with O => 0 | S m => hrefs s h m b + hterm s h m b end.
Definition cnt (s : Store) (h : Heap) (n : nat) (K : Blk -> nat) : Prop :=
  forall b, Z.of_nat (K b + hrefs s h n b) <= rcof h b.

Definition dom_in (g : Ctx) (D : list Var) : Prop := forall x t, g x = Some t -> In x D.

Lemma grows_refl : forall h, grows h h.
Proof. intros h b x H. exists x. auto. Qed.

Lemma grows_trans : forall h1 h2 h3, grows h1 h2 -> grows h2 h3 -> grows h1 h3.
Proof.
  intros h1 h2 h3 A B b x Hx. destruct (A b x Hx) as [y [Hy [S1 [R1 I1]]]].
  destruct (B b y Hy) as [z [Hz [S2 [R2 I2]]]].
  exists z. repeat split; try congruence. intros i Hi. auto.
Qed.

Lemma sext_refl : forall s, sext s s.
Proof. intros s c w H. exact H. Qed.

Lemma sext_trans : forall s1 s2 s3, sext s1 s2 -> sext s2 s3 -> sext s1 s3.
Proof. intros s1 s2 s3 A B c w H. exact (B c w (A c w H)). Qed.

Lemma boxblk_mono : forall h h' b, grows h h' -> boxblk h b -> boxblk h' b.
Proof.
  intros h h' b G [x [Hx [Sx [Rx Ix]]]]. destruct (G b x Hx) as [y [Hy [S1 [R1 I1]]]].
  exists y. repeat split; try congruence. intros i Hi. apply I1. apply Ix. lia.
Qed.

Lemma vty_mono : forall t s s' h h' n n' v,
  grows h h' -> (n <= n')%nat -> sext s s' -> vty s h n v t -> vty s' h' n' v t.
Proof.
  induction t as [| | | t IH]; intros s s' h h' n n' v G Hn X H; destruct v; simpl in *; try exact I; try contradiction.
  - destruct H as [Hb [x [Hx [Hs [Hr Hi]]]]]. split; [lia |].
    destruct (G b x Hx) as [y [Hy [S1 [R1 I1]]]].
    exists y. repeat split; try congruence.
    intros i Hrange. apply I1. apply Hi. lia.
  - destruct H as [Hb [B [Hs V]]]. split; [lia |]. split; [exact (boxblk_mono h h' b G B) |].
    split; [exact (X b v Hs) | exact (IH s s' h h' n n' v G Hn X V)].
Qed.

Lemma envok_mono : forall g r s s' h h' n n',
  grows h h' -> (n <= n')%nat -> sext s s' -> envok g r s h n -> envok g r s' h' n'.
Proof.
  intros g r s s' h h' n n' G Hn X E x t Hx. destruct (E x t Hx) as [v [Hv Ht]].
  exists v. split; [exact Hv | exact (vty_mono t s s' h h' n n' v G Hn X Ht)].
Qed.

Lemma sok_mono : forall s h h' n n', grows h h' -> (n <= n')%nat -> sok s h n -> sok s h' n'.
Proof.
  intros s h h' n n' G Hn O c w Hc. destruct (O c w Hc) as [Hcn [B [t V]]].
  split; [lia |]. split; [exact (boxblk_mono h h' c G B) |].
  exists t. exact (vty_mono t s s h h' n n' w G Hn (sext_refl s) V).
Qed.

Lemma cnt_le : forall s h n K K', (forall b, (K' b <= K b)%nat) -> cnt s h n K -> cnt s h n K'.
Proof. intros s h n K K' L C b. specialize (C b). specialize (L b). lia. Qed.

(* A reference the program holds is to a live block. *)
Lemma cnt_live : forall s h n K b, cnt s h n K -> (1 <= K b)%nat -> exists x, h b = Some x /\ 0 < rc x.
Proof.
  intros s h n K b C L. specialize (C b). unfold rcof in C.
  destruct (h b) as [x |]; [exists x; split; [reflexivity | lia] | lia].
Qed.

Lemma envrefs_in : forall D r x b, In x D -> (oref (r x) b <= envrefs D r b)%nat.
Proof.
  induction D as [| a D IH]; intros r x b H; [destruct H |].
  destruct H as [<- | H]; simpl; [lia |]. specialize (IH r x b H). lia.
Qed.

Lemma envrefs_upd_notin : forall D r x v b, ~ In x D -> envrefs D (upd r x v) b = envrefs D r b.
Proof.
  induction D as [| a D IH]; intros r x v b H; [reflexivity |]. simpl.
  unfold upd at 1. destruct (Nat.eqb a x) eqn:B.
  - apply Nat.eqb_eq in B. subst. exfalso. apply H. left. reflexivity.
  - rewrite IH; [reflexivity |]. intro I. apply H. right. exact I.
Qed.

(* Overwriting a variable trades its old reference for the new one. *)
Lemma envrefs_upd : forall D r x v b, In x D -> NoDup D ->
  (envrefs D (upd r x v) b + oref (r x) b = envrefs D r b + vref v b)%nat.
Proof.
  induction D as [| a D IH]; intros r x v b H N; [destruct H |].
  inversion N as [| a' D' Hna Hnd]; subst. simpl. unfold upd at 1.
  destruct (Nat.eqb a x) eqn:B.
  - apply Nat.eqb_eq in B. subst. rewrite envrefs_upd_notin by exact Hna. simpl. lia.
  - destruct H as [-> | H]; [rewrite Nat.eqb_refl in B; discriminate |].
    specialize (IH r x v b H Hnd). lia.
Qed.

Lemma envrefs_empty : forall D b, envrefs D empty_env b = 0%nat.
Proof. induction D as [| a D IH]; intro b; [reflexivity | simpl; rewrite IH; reflexivity]. Qed.

(* What the live boxes hold changes only where liveness does. *)
Lemma hterm_same : forall s h h' c b, rcof h' c = rcof h c -> hterm s h' c b = hterm s h c b.
Proof. intros s h h' c b E. unfold hterm. rewrite E. reflexivity. Qed.

Lemma hrefs_change : forall s h h' d b n,
  (forall c, c <> d -> rcof h' c = rcof h c) ->
  (hrefs s h' n b + (if Nat.ltb d n then hterm s h d b else 0) =
   hrefs s h n b + (if Nat.ltb d n then hterm s h' d b else 0))%nat.
Proof.
  intros s h h' d b n H. induction n as [| m IH]; [reflexivity |]. simpl.
  destruct (Nat.ltb_spec d m) as [Lm | Lm]; destruct (Nat.ltb_spec d (S m)) as [Ls | Ls]; try lia.
  - rewrite (hterm_same s h h' m b (H m ltac:(lia))). lia.
  - assert (d = m) by lia. subst d. lia.
  - rewrite (hterm_same s h h' m b (H m ltac:(lia))). lia.
Qed.

Lemma hrefs_term_le : forall s h n d b, (d < n)%nat -> (hterm s h d b <= hrefs s h n b)%nat.
Proof.
  intros s h n d b. induction n as [| m IH]; intro L; [lia |]. simpl.
  destruct (Nat.eq_dec d m) as [-> | Ne]; [lia |]. specialize (IH ltac:(lia)). lia.
Qed.

Lemma hrefs_store : forall s s' h n b, (forall c, (c < n)%nat -> s' c = s c) -> hrefs s' h n b = hrefs s h n b.
Proof.
  intros s s' h n b E. induction n as [| m IH]; [reflexivity |]. simpl.
  rewrite IH by (intros c Hc; apply E; lia). unfold hterm. rewrite (E m ltac:(lia)). reflexivity.
Qed.

(* A change of one block's count that keeps its liveness keeps what the
   live boxes hold. *)
Lemma hrefs_same_live : forall s h h' d n b,
  (forall c, c <> d -> rcof h' c = rcof h c) -> (0 <? rcof h' d) = (0 <? rcof h d) ->
  hrefs s h' n b = hrefs s h n b.
Proof.
  intros s h h' d n b R L. pose proof (hrefs_change s h h' d b n R) as Hc.
  unfold hterm in Hc. rewrite L in Hc. destruct (Nat.ltb d n); lia.
Qed.

Lemma hrefs_rc : forall s h h' n b, (forall c, rcof h' c = rcof h c) -> hrefs s h' n b = hrefs s h n b.
Proof.
  intros s h h' n b R. induction n as [| m IH]; [reflexivity |]. simpl. rewrite IH.
  unfold hterm. rewrite R. reflexivity.
Qed.

(* ── The four clauses, event by event ───────────────────────────────────── *)

Lemma holds_app : forall P h a b,
  holds P h (a ++ b) <-> holds P h a /\ holds P (fold_left step a h) b.
Proof.
  intros P h a. revert h. induction a as [| ev a IH]; intros h b; simpl; [tauto |].
  rewrite IH. tauto.
Qed.

Lemma bsize_pos : forall k, 0 < bsize k.
Proof. intro k. unfold bsize, HDR, W. lia. Qed.

Lemma rcof_set : forall h b y c, rcof (set h b y) c = if Nat.eqb c b then rc y else rcof h c.
Proof. intros h b y c. unfold rcof, set. destruct (Nat.eqb c b); reflexivity. Qed.

(* Rewriting one allocated block, keeping its size and representation and
   only adding initialized bytes. *)
Lemma set_grows : forall h b x y, h b = Some x -> size y = size x -> repr y = repr x ->
  (forall i, init x i = true -> init y i = true) -> grows h (set h b y).
Proof.
  intros h b x y Hx S R I c z Hz. unfold set. destruct (Nat.eqb c b) eqn:B.
  - apply Nat.eqb_eq in B. subst. rewrite Hx in Hz. injection Hz as <-. exists y. auto.
  - exists z. split; [exact Hz |]. auto.
Qed.

Lemma set_wfh : forall h n b x y, wfh h n -> h b = Some x -> wfh (set h b y) n.
Proof.
  intros h n b x y W0 Hx c. unfold set. destruct (Nat.eqb c b) eqn:B.
  - apply Nat.eqb_eq in B. subst. split; [eauto |].
    intro Hc. rewrite (proj2 (W0 b) Hc) in Hx. discriminate.
  - exact (W0 c).
Qed.

(* A fresh allocation. *)
Lemma alloc_ok : forall h n sz k,
  wfh h n ->
  safe_ev h (EAlloc n sz k) /\
  grows h (step h (EAlloc n sz k)) /\ wfh (step h (EAlloc n sz k)) (S n) /\
  step h (EAlloc n sz k) n = Some (mkBlock sz k 1 (fun _ => false)) /\
  (forall c, rcof (step h (EAlloc n sz k)) c = if Nat.eqb c n then 1 else rcof h c).
Proof.
  intros h n sz k W0. pose proof (proj2 (W0 n) (le_n n)) as Hn.
  set (h1 := step h (EAlloc n sz k)).
  assert (E : forall b, h1 b = if Nat.eqb b n then Some (mkBlock sz k 1 (fun _ => false)) else h b)
    by reflexivity.
  split; [| split; [| split; [| split]]].
  - split; [exact I |]. split; [| split; exact I].
    unfold temporal_ev, live. intros [x [Hx _]]. rewrite Hn in Hx. discriminate.
  - intros b x Hx. exists x. rewrite E. destruct (Nat.eqb b n) eqn:B.
    + apply Nat.eqb_eq in B. subst. rewrite Hn in Hx. discriminate.
    + auto.
  - intro b. split.
    + intro Hb. rewrite E. destruct (Nat.eqb b n) eqn:B; [eexists; reflexivity |].
      apply (proj1 (W0 b)). apply Nat.eqb_neq in B. lia.
    + intro Hb. rewrite E. destruct (Nat.eqb b n) eqn:B; [apply Nat.eqb_eq in B; lia |].
      apply (proj2 (W0 b)). lia.
  - rewrite E, Nat.eqb_refl. reflexivity.
  - intro c. unfold rcof. rewrite E. destruct (Nat.eqb c n); reflexivity.
Qed.

(* A store inside a live block of its representation. *)
Lemma store_ok : forall h n b x o w,
  wfh h n -> h b = Some x -> 0 < rc x -> 0 <= o -> 0 < w -> o + w <= size x ->
  safe_ev h (EStore b o w (repr x)) /\
  grows h (step h (EStore b o w (repr x))) /\ wfh (step h (EStore b o w (repr x))) n /\
  step h (EStore b o w (repr x)) b = Some (mkBlock (size x) (repr x) (rc x) (mark (init x) o w)) /\
  (forall c, rcof (step h (EStore b o w (repr x))) c = rcof h c).
Proof.
  intros h n b x o w W0 Hx Hc Ho Hw Hend.
  set (y := mkBlock (size x) (repr x) (rc x) (mark (init x) o w)).
  assert (E : step h (EStore b o w (repr x)) = set h b y) by (simpl; rewrite Hx; reflexivity).
  rewrite E. split; [| split; [| split; [| split]]].
  - split; [exists x; split; [exact Hx | unfold in_bounds; lia] |].
    split; [exists x; split; [exact Hx | exact Hc] |]. split; [exact I | exists x; auto].
  - apply (set_grows h b x y Hx); [reflexivity | reflexivity |].
    intros i Hi. unfold y, mark. simpl. rewrite Hi. destruct ((o <=? i) && (i <? o + w)); reflexivity.
  - exact (set_wfh h n b x y W0 Hx).
  - unfold set. rewrite Nat.eqb_refl. reflexivity.
  - intro c. rewrite rcof_set. destruct (Nat.eqb c b) eqn:B; [| reflexivity].
    apply Nat.eqb_eq in B. subst. unfold rcof. rewrite Hx. reflexivity.
Qed.

(* A load inside a live, initialized block of its representation. *)
Lemma load_ok : forall h b x o w,
  h b = Some x -> 0 < rc x -> 0 <= o -> 0 < w -> o + w <= size x -> all_init (init x) o w ->
  safe_ev h (ELoad b o w (repr x)) /\ step h (ELoad b o w (repr x)) = h.
Proof.
  intros h b x o w Hx Hc Ho Hw Hend Hi. repeat split.
  - exists x. split; [exact Hx |]. unfold in_bounds. lia.
  - exists x. split; [exact Hx | lia].
  - exists x. auto.
  - exists x. auto.
Qed.

(* One more reference to a live block. *)
Lemma inc_ok : forall h n b x, wfh h n -> h b = Some x -> 0 < rc x ->
  safe_ev h (EInc b) /\ grows h (step h (EInc b)) /\ wfh (step h (EInc b)) n /\
  (forall c, rcof (step h (EInc b)) c = rcof h c + (if Nat.eqb c b then 1 else 0)).
Proof.
  intros h n b x W0 Hx Hc.
  set (y := mkBlock (size x) (repr x) (rc x + 1) (init x)).
  assert (E : step h (EInc b) = set h b y) by (simpl; rewrite Hx; reflexivity).
  rewrite E. split; [| split; [| split]].
  - split; [exact I |]. split; [exists x; split; [exact Hx | exact Hc] |]. split; exact I.
  - apply (set_grows h b x y Hx); auto.
  - exact (set_wfh h n b x y W0 Hx).
  - intro c. rewrite rcof_set. destruct (Nat.eqb c b) eqn:B; [| lia].
    apply Nat.eqb_eq in B. subst. unfold rcof. rewrite Hx. reflexivity.
Qed.

(* One reference given back to a live block; at zero the block is freed. *)
Lemma dec_ok : forall h n b x, wfh h n -> h b = Some x -> 0 < rc x ->
  safe_ev h (EDec b) /\ grows h (step h (EDec b)) /\ wfh (step h (EDec b)) n /\
  (forall c, rcof (step h (EDec b)) c = rcof h c - (if Nat.eqb c b then 1 else 0)).
Proof.
  intros h n b x W0 Hx Hc.
  set (y := mkBlock (size x) (repr x) (rc x - 1) (init x)).
  assert (E : step h (EDec b) = set h b y).
  { simpl. rewrite Hx. replace (0 <? rc x) with true by (symmetry; apply Z.ltb_lt; exact Hc). reflexivity. }
  rewrite E. split; [| split; [| split]].
  - split; [exact I |]. split; [exists x; split; [exact Hx | exact Hc] |]. split; exact I.
  - apply (set_grows h b x y Hx); auto.
  - exact (set_wfh h n b x y W0 Hx).
  - intro c. rewrite rcof_set. destruct (Nat.eqb c b) eqn:B; [| lia].
    apply Nat.eqb_eq in B. subst. unfold rcof. rewrite Hx. reflexivity.
Qed.

Lemma mark_all : forall f o w i, o <= i < o + w -> mark f o w i = true.
Proof.
  intros f o w i H. unfold mark. replace ((o <=? i) && (i <? o + w)) with true; [reflexivity |].
  symmetry. apply andb_true_iff. split; [apply Z.leb_le | apply Z.ltb_lt]; lia.
Qed.

Lemma mark_mono : forall f o w i, f i = true -> mark f o w i = true.
Proof. intros f o w i H. unfold mark. destruct ((o <=? i) && (i <? o + w)); [reflexivity | exact H]. Qed.

(* Allocate and initialize a block of size sz, representation k. *)
Lemma fresh_ok : forall h n sz k,
  wfh h n -> 0 < sz ->
  let t := [EAlloc n sz k; EStore n 0 sz k] in
  holds safe_ev h t /\ grows h (fold_left step t h) /\ wfh (fold_left step t h) (S n) /\
  (exists x, fold_left step t h n = Some x /\ size x = sz /\ repr x = k /\ rc x = 1 /\
            all_init (init x) 0 (size x)) /\
  (forall c, rcof (fold_left step t h) c = if Nat.eqb c n then 1 else rcof h c).
Proof.
  intros h n sz k W0 Px t.
  destruct (alloc_ok h n sz k W0) as [S1 [G1 [W1 [E1 R1]]]].
  set (h1 := step h (EAlloc n sz k)) in *.
  set (x := mkBlock sz k 1 (fun _ => false)) in *.
  destruct (store_ok h1 (S n) n x 0 sz W1 E1 ltac:(unfold x; simpl; lia) ltac:(lia) Px ltac:(unfold x; simpl; lia))
    as [S2 [G2 [W2 [E2 R2]]]].
  unfold t. simpl. fold h1.
  split; [split; [exact S1 | split; [exact S2 | exact I]] |].
  split; [exact (grows_trans _ _ _ G1 G2) |].
  split; [exact W2 |].
  split.
  - eexists. split; [exact E2 |]. simpl. repeat split. intros i Hi. apply mark_all. lia.
  - intro c. rewrite R2. apply R1.
Qed.

(* A fresh block changes what the live boxes hold only by its own slot. *)
Lemma hrefs_fresh : forall s s' h h' n b,
  (forall c, c <> n -> rcof h' c = rcof h c) -> rcof h' n = 1 -> (forall c, (c < n)%nat -> s' c = s c) ->
  hrefs s' h' (S n) b = (hrefs s h n b + oref (s' n) b)%nat.
Proof.
  intros s s' h h' n b R Rn E. simpl.
  pose proof (hrefs_change s h h' n b n R) as Hc. rewrite Nat.ltb_irrefl in Hc.
  rewrite (hrefs_store s s' h' n b E). unfold hterm. rewrite Rn. simpl. lia.
Qed.

(* A fresh block is no box the store knows. *)
Lemma sok_fresh : forall s h n, sok s h n -> s n = None.
Proof.
  intros s h n O. destruct (s n) as [w |] eqn:E; [| reflexivity].
  destruct (O n w E) as [L _]. lia.
Qed.

(* One more reference to a block the program holds, directly or through a
   live box's slot. *)
Lemma inc_case : forall s h n K b, wfh h n -> cnt s h n K -> (1 <= K b + hrefs s h n b)%nat ->
  holds safe_ev h [EInc b] /\ grows h (fold_left step [EInc b] h) /\ wfh (fold_left step [EInc b] h) n /\
  cnt s (fold_left step [EInc b] h) n (fun c => K c + (if Nat.eqb b c then 1 else 0))%nat.
Proof.
  intros s h n K b W0 C L.
  assert (Hl : exists x, h b = Some x /\ 0 < rc x).
  { specialize (C b). unfold rcof in C. destruct (h b) as [x |]; [exists x; split; [reflexivity | lia] | lia]. }
  destruct Hl as [x [Hx Hc]].
  destruct (inc_ok h n b x W0 Hx Hc) as [S [G [W1 R]]].
  assert (Rb : rcof h b = rc x) by (unfold rcof; rewrite Hx; reflexivity).
  cbn [fold_left holds]. split; [split; [exact S | exact I] |]. split; [exact G |]. split; [exact W1 |].
  intro c.
  rewrite (hrefs_same_live s h (step h (EInc b)) b n c
             ltac:(intros c' Ne; rewrite R; apply Nat.eqb_neq in Ne; rewrite Ne; lia)
             ltac:(rewrite R, Nat.eqb_refl, Rb; destruct (Z.ltb_spec 0 (rc x + 1)); destruct (Z.ltb_spec 0 (rc x)); lia)).
  rewrite R. specialize (C c). rewrite Nat.eqb_sym. destruct (Nat.eqb c b); lia.
Qed.

Lemma dup_ok : forall s h n K v, wfh h n -> cnt s h n K ->
  (forall b, (vref v b <= K b + hrefs s h n b)%nat) ->
  holds safe_ev h (dup v) /\ grows h (fold_left step (dup v) h) /\ wfh (fold_left step (dup v) h) n /\
  cnt s (fold_left step (dup v) h) n (fun b => K b + vref v b)%nat.
Proof.
  intros s h n K v W0 C L. destruct v as [z | bb | b vs | b w].
  - simpl. split; [exact I |]. split; [apply grows_refl |]. split; [exact W0 |].
    intro c. specialize (C c). lia.
  - simpl. split; [exact I |]. split; [apply grows_refl |]. split; [exact W0 |].
    intro c. specialize (C c). lia.
  - apply (inc_case s h n K b W0 C). specialize (L b). simpl in L. rewrite Nat.eqb_refl in L. lia.
  - apply (inc_case s h n K b W0 C). specialize (L b). simpl in L. rewrite Nat.eqb_refl in L. lia.
Qed.

(* Giving back a reference the program holds. The last reference to a box
   also gives back what its slot held. *)
Lemma dropv_ok : forall v s h n K t,
  wfh h n -> sok s h n -> vty s h n v t ->
  cnt s h n (fun b => K b + vref v b)%nat ->
  holds safe_ev h (dropv h v) /\ grows h (fold_left step (dropv h v) h) /\
  wfh (fold_left step (dropv h v) h) n /\ cnt s (fold_left step (dropv h v) h) n K.
Proof.
  induction v as [z | bb | b vs | b w IH]; intros s h n K t W0 O V C.
  - simpl. split; [exact I |]. split; [apply grows_refl |]. split; [exact W0 |].
    intro c. specialize (C c). simpl in C. lia.
  - simpl. split; [exact I |]. split; [apply grows_refl |]. split; [exact W0 |].
    intro c. specialize (C c). simpl in C. lia.
  - (* a list: one dec; its block holds nothing the store counts *)
    destruct t; try contradiction. destruct V as [Hb [x0 [Hx0 [Sx [Rx Ix]]]]].
    destruct (cnt_live s h n _ b C ltac:(simpl; rewrite Nat.eqb_refl; lia)) as [x [Hx Hc]].
    destruct (dec_ok h n b x W0 Hx Hc) as [S [G [W1 R]]].
    assert (Nb : s b = None).
    { destruct (s b) as [w |] eqn:Hs; [| reflexivity].
      destruct (O b w Hs) as [_ [[y [Hy [_ [Ry _]]]] _]]. rewrite Hx0 in Hy. injection Hy as <-.
      rewrite Rx in Ry. discriminate. }
    simpl. split; [split; [exact S | exact I] |]. split; [exact G |]. split; [exact W1 |].
    intro c.
    pose proof (hrefs_change s h (step h (EDec b)) b c n
                  ltac:(intros c' Ne; rewrite R; apply Nat.eqb_neq in Ne; rewrite Ne; lia)) as Hc'.
    unfold hterm in Hc'. rewrite Nb in Hc'. simpl in Hc'.
    assert (Z0 : forall (P : bool) (Q : bool), (if P then (if Q then 0 else 0) else 0)%nat = 0%nat)
      by (intros [|] [|]; reflexivity).
    rewrite !Z0 in Hc'.
    rewrite R. specialize (C c). simpl in C. rewrite Nat.eqb_sym in C. destruct (Nat.eqb c b); lia.
  - (* a box *)
    destruct t as [| | | t']; try contradiction. destruct V as [Hb [B [Hs Vw]]].
    destruct (cnt_live s h n _ b C ltac:(simpl; rewrite Nat.eqb_refl; lia)) as [x [Hx Hc]].
    destruct (dec_ok h n b x W0 Hx Hc) as [S [G [W1 R]]].
    set (h1 := step h (EDec b)) in *.
    assert (Rel : forall c', c' <> b -> rcof h1 c' = rcof h c')
      by (intros c' Ne; rewrite R; apply Nat.eqb_neq in Ne; rewrite Ne; lia).
    assert (Rb : rcof h b = rc x) by (unfold rcof; rewrite Hx; reflexivity).
    cbn [dropv]. destruct (Z.eqb_spec (rcof h b) 1) as [One | NotOne].
    + (* the last reference: read the slot, free the box, drop its content *)
      destruct B as [x' [Hx' [Sz [Rp Ini]]]]. rewrite Hx in Hx'. injection Hx' as <-.
      destruct (load_ok h b x HDR W Hx Hc ltac:(unfold HDR; lia) ltac:(unfold W; lia)
                  ltac:(rewrite Sz; unfold bsize, HDR, W; simpl; lia)
                  ltac:(intros i Hi; apply Ini; rewrite Sz; unfold bsize, HDR, W in *; simpl in *; lia)) as [SL EL].
      rewrite Rp in SL, EL.
      assert (C1 : cnt s h1 n (fun c => K c + vref w c)%nat).
      { intro c. pose proof (hrefs_change s h h1 b c n Rel) as Hch.
        replace (Nat.ltb b n) with true in Hch by (symmetry; apply Nat.ltb_lt; exact Hb).
        unfold hterm in Hch. rewrite Hs in Hch. simpl in Hch.
        assert (Dead : rcof h1 b = 0) by (rewrite R, Nat.eqb_refl; lia).
        rewrite Dead in Hch. replace (0 <? rcof h b) with true in Hch by (symmetry; apply Z.ltb_lt; lia).
        simpl in Hch. pose proof (C c) as Cc. simpl in Cc.
        destruct (Nat.eq_dec c b) as [-> | Ne].
        - (* the freed box itself: nothing referred to it but us *)
          rewrite Nat.eqb_refl in Cc. rewrite Dead.
          assert (Hw : vref w b = 0%nat).
          { pose proof (hrefs_term_le s h n b b Hb) as T. unfold hterm in T. rewrite Hs in T.
            replace (0 <? rcof h b) with true in T by (symmetry; apply Z.ltb_lt; lia). simpl in T. lia. }
          lia.
        - rewrite (Rel c Ne). apply Nat.eqb_neq in Ne. rewrite Nat.eqb_sym, Ne in Cc. lia. }
      destruct (IH s h1 n K t' W1 (sok_mono s h h1 n n G (le_n n) O)
                  (vty_mono t' s s h h1 n n w G (le_n n) (sext_refl s) Vw) C1) as [S2 [G2 [W2 C2]]].
      cbn [holds fold_left]. rewrite EL. fold h1.
      split; [split; [exact SL | split; [exact S | exact S2]] |].
      split; [exact (grows_trans _ _ _ G G2) |]. split; [exact W2 | exact C2].
    + (* someone else still holds the box: one dec, the box stays live *)
      cbn [holds fold_left]. fold h1.
      split; [split; [exact S | exact I] |]. split; [exact G |]. split; [exact W1 |].
      intro c.
      rewrite (hrefs_same_live s h h1 b n c Rel
                 ltac:(rewrite R, Nat.eqb_refl; pose proof (C b) as Cb; simpl in Cb; rewrite Nat.eqb_refl in Cb;
                       destruct (Z.ltb_spec 0 (rcof h b)); destruct (Z.ltb_spec 0 (rcof h b - 1)); lia)).
      rewrite R. specialize (C c). simpl in C. rewrite Nat.eqb_sym in C. destruct (Nat.eqb c b); lia.
Qed.

Lemma dropo_ok : forall o s h n K,
  wfh h n -> sok s h n -> (forall v, o = Some v -> exists t, vty s h n v t) ->
  cnt s h n (fun b => K b + oref o b)%nat ->
  holds safe_ev h (dropo h o) /\ grows h (fold_left step (dropo h o) h) /\
  wfh (fold_left step (dropo h o) h) n /\ cnt s (fold_left step (dropo h o) h) n K.
Proof.
  intros o s h n K W0 O V C. destruct o as [v |].
  - destruct (V v eq_refl) as [t Vt]. exact (dropv_ok v s h n K t W0 O Vt C).
  - simpl. split; [exact I |]. split; [apply grows_refl |]. split; [exact W0 |].
    intro c. specialize (C c). simpl in C. lia.
Qed.

(* ── Expressions ────────────────────────────────────────────────────────── *)

Lemma length_set_nth : forall vs i k, length (set_nth vs i k) = length vs.
Proof.
  induction vs as [| a vs IH]; intros i k; [reflexivity |]. destruct i; simpl; [reflexivity | f_equal; apply IH].
Qed.

(* A fresh list block: the store is unchanged, and the one reference to the
   block is the new value's. *)
Lemma fresh_list_ok : forall s h n k K,
  wfh h n -> sok s h n -> cnt s h n K ->
  let t := [EAlloc n (bsize k) RL; EStore n 0 (bsize k) RL] in
  holds safe_ev h t /\ grows h (fold_left step t h) /\ wfh (fold_left step t h) (S n) /\
  sok s (fold_left step t h) (S n) /\
  (exists x, fold_left step t h n = Some x /\ size x = bsize k /\ repr x = RL /\
            all_init (init x) 0 (size x)) /\
  cnt s (fold_left step t h) (S n) (fun c => K c + (if Nat.eqb n c then 1 else 0))%nat.
Proof.
  intros s h n k K W0 O C t.
  destruct (fresh_ok h n (bsize k) RL W0 (bsize_pos k)) as [SF0 [G [W1 [[y [Ey [Sy [Ry [_ Iy]]]]] R]]]].
  fold t in SF0, G, W1, Ey, R.
  split; [exact SF0 |]. split; [exact G |]. split; [exact W1 |].
  split; [exact (sok_mono s h _ n (S n) G ltac:(lia) O) |].
  split; [exists y; auto |].
  intro c.
  rewrite (hrefs_fresh s s h (fold_left step t h) n c
             ltac:(intros c' Ne; rewrite R; apply Nat.eqb_neq in Ne; rewrite Ne; reflexivity)
             ltac:(rewrite R, Nat.eqb_refl; reflexivity) ltac:(reflexivity)).
  rewrite (sok_fresh s h n O). simpl. rewrite R. specialize (C c).
  destruct (Nat.eqb c n) eqn:B.
  - apply Nat.eqb_eq in B. subst c. rewrite Nat.eqb_refl. unfold rcof in C. rewrite (proj2 (W0 n) (le_n n)) in C. lia.
  - rewrite Nat.eqb_sym, B. lia.
Qed.

(* An operator on two held operands: it gives back the ones it consumes,
   and its result is one held reference. *)
Lemma binop_safe : forall op t1 t2 t v1 v2 n tr o h s K,
  optype op t1 t2 t -> vty s h n v1 t1 -> vty s h n v2 t2 -> wfh h n -> sok s h n ->
  cnt s h n (fun b => K b + vref v1 b + vref v2 b)%nat -> binop_step op v1 v2 n tr o ->
  holds safe_ev h tr /\ grows h (fold_left step tr h) /\
  (forall v n', o = Val v n' -> (n <= n')%nat /\ wfh (fold_left step tr h) n' /\
     sok s (fold_left step tr h) n' /\
     vty s (fold_left step tr h) n' v t /\ cnt s (fold_left step tr h) n' (fun b => K b + vref v b)%nat).
Proof.
  intros op t1 t2 t v1 v2 n tr o h s K Hop H1 H2 W0 O C Hs.
  destruct Hs; inversion Hop; subst; simpl.
  - (* Add *)
    split; [exact I |]. split; [apply grows_refl |].
    intros v n' Ev. injection Ev as <- <-. split; [lia |]. split; [exact W0 |]. split; [exact O |].
    split; [exact I |]. intro c. specialize (C c). simpl in C |- *. lia.
  - (* Lt *)
    split; [exact I |]. split; [apply grows_refl |].
    intros v n' Ev. injection Ev as <- <-. split; [lia |]. split; [exact W0 |]. split; [exact O |].
    split; [exact I |]. intro c. specialize (C c). simpl in C |- *. lia.
  - (* Make *)
    destruct (fresh_list_ok s h n (Z.to_nat k) K W0 O ltac:(eapply cnt_le; [| exact C]; intro c; simpl; lia))
      as [S [G [W1 [O1 [[y [Ey [Sy [Ry Iy]]]] C1]]]]].
    simpl in S, G, W1, O1, Ey, C1. split; [exact S |]. split; [exact G |].
    intros v n' Ev. injection Ev as <- <-. split; [lia |]. split; [exact W1 |]. split; [exact O1 |].
    split; [simpl; split; [lia |]; exists y; rewrite repeat_length; auto |].
    exact C1.
  - (* Make, negative count *)
    split; [exact I |]. split; [apply grows_refl | discriminate].
  - (* Index: load the element, then give the list back *)
    destruct H1 as [Hb [x [Hx [Sx [Rx Ix]]]]].
    destruct (cnt_live s h n _ b C ltac:(simpl; rewrite Nat.eqb_refl; lia)) as [x' [Hx' Hc]].
    rewrite Hx in Hx'. injection Hx' as <-.
    assert (Hl : 0 <= HDR + i * W) by (unfold HDR, W; lia).
    rewrite <- Rx.
    destruct (load_ok h b x (HDR + i * W) W Hx Hc Hl ltac:(unfold W; lia)
               ltac:(rewrite Sx; unfold bsize, HDR, W in *; lia)
               ltac:(intros j Hj; apply Ix; rewrite Sx; unfold bsize, HDR, W in *; lia))
      as [SL EL]. try rewrite EL.
    destruct (dropv_ok (VList b vs) s h n K TList W0 O
                ltac:(simpl; split; [exact Hb | exists x; auto])
                ltac:(eapply cnt_le; [| exact C]; intro c; simpl; lia)) as [SD [GD [WD CD]]].
    simpl in SD, GD, WD, CD.
    split; [split; [exact SL | exact SD] |]. split; [exact GD |].
    intros v n' Ev. injection Ev as <- <-. split; [lia |]. split; [exact WD |].
    split; [exact (sok_mono s h _ n n GD (le_n n) O) |]. split; [exact I |].
    intro c. specialize (CD c). simpl. lia.
  - (* Index, out of bounds *)
    split; [exact I |]. split; [apply grows_refl | discriminate].
Qed.

Lemma eval_safe : forall h r n e tr o,
  eval h r n e tr o ->
  forall g t s D F, etyped g e t -> dom_in g D -> envok g r s h n -> wfh h n -> sok s h n ->
  cnt s h n (fun b => envrefs D r b + F b)%nat ->
  holds safe_ev h tr /\ grows h (fold_left step tr h) /\
  (forall v n', o = Val v n' -> (n <= n')%nat /\ exists s', sext s s' /\
     wfh (fold_left step tr h) n' /\ sok s' (fold_left step tr h) n' /\
     vty s' (fold_left step tr h) n' v t /\
     cnt s' (fold_left step tr h) n' (fun b => envrefs D r b + F b + vref v b)%nat).
Proof.
  intros h r n e tr o Hev. induction Hev; intros g ty s D F Ht Dg E W0 O C; inversion Ht; subst.
  - (* EvInt *)
    simpl. split; [exact I |]. split; [apply grows_refl |].
    intros v n' Ev. injection Ev as <- <-. split; [lia |]. exists s.
    split; [apply sext_refl |]. split; [exact W0 |]. split; [exact O |]. split; [exact I |].
    intro c. specialize (C c). cbn [vref] in *. lia.
  - (* EvBool *)
    simpl. split; [exact I |]. split; [apply grows_refl |].
    intros v n' Ev. injection Ev as <- <-. split; [lia |]. exists s.
    split; [apply sext_refl |]. split; [exact W0 |]. split; [exact O |]. split; [exact I |].
    intro c. specialize (C c). cbn [vref] in *. lia.
  - (* EvVar: share the variable's value *)
    destruct (E x ty H1) as [v' [Hv' Hty]]. rewrite H in Hv'. injection Hv' as <-.
    assert (L : forall b, (vref v b <= envrefs D r b + F b + hrefs s h n b)%nat).
    { intro b. pose proof (envrefs_in D r x b (Dg x ty H1)) as I. rewrite H in I. simpl in I. lia. }
    destruct (dup_ok s h n _ v W0 C L) as [S [G [W1 C1]]].
    split; [exact S |]. split; [exact G |].
    intros vv nn Ev. injection Ev as <- <-. split; [lia |]. exists s.
    split; [apply sext_refl |]. split; [exact W1 |].
    split; [exact (sok_mono s h _ n n G (le_n n) O) |].
    split; [exact (vty_mono ty s s h _ n n v G (le_n n) (sext_refl s) Hty) | exact C1].
  - (* EvBin *)
    destruct (IHHev1 g t0 s D F ltac:(eassumption) Dg E W0 O C) as [S1 [G1 R1]].
    destruct (R1 v1 n1 eq_refl) as [N1 [s1 [X1 [W1 [O1 [V1 C1]]]]]].
    set (h1 := fold_left step t1 h) in *.
    destruct (IHHev2 g t4 s1 D (fun b => F b + vref v1 b)%nat ltac:(eassumption) Dg
                (envok_mono g r s s1 h h1 n n1 G1 N1 X1 E) W1 O1
                ltac:(eapply cnt_le; [| exact C1]; intro b; cbv beta; lia)) as [S2 [G2 R2]].
    destruct (R2 v2 n2 eq_refl) as [N2 [s2 [X2 [W2 [O2 [V2 C2]]]]]].
    set (h2 := fold_left step t2 h1) in *.
    destruct (binop_safe op t0 t4 ty v1 v2 n2 t3 o h2 s2 (fun b => envrefs D r b + F b)%nat H3
                (vty_mono t0 s1 s2 h1 h2 n1 n2 v1 G2 N2 X2 V1) V2 W2 O2
                ltac:(eapply cnt_le; [| exact C2]; intro b; cbv beta; lia) H) as [S3 [G3 R3]].
    rewrite !holds_app, !fold_left_app. fold h1. fold h2.
    split; [split; [exact S1 | split; [exact S2 | exact S3]] |].
    split; [exact (grows_trans _ _ _ G1 (grows_trans _ _ _ G2 G3)) |].
    intros vv nn Ev. destruct (R3 vv nn Ev) as [N3 [W3 [O3 [V3 C3]]]]. split; [lia |].
    exists s2. split; [exact (sext_trans _ _ _ X1 X2) |]. split; [exact W3 |]. split; [exact O3 |].
    split; [exact V3 | exact C3].
  - (* EvBinAbort1 *)
    destruct (IHHev g _ s D F ltac:(eassumption) Dg E W0 O C) as [S1 [G1 _]].
    split; [exact S1 |]. split; [exact G1 | discriminate].
  - (* EvBinAbort2 *)
    destruct (IHHev1 g _ s D F ltac:(eassumption) Dg E W0 O C) as [S1 [G1 R1]].
    destruct (R1 v1 n1 eq_refl) as [N1 [s1 [X1 [W1 [O1 [V1 C1]]]]]].
    destruct (IHHev2 g _ s1 D (fun b => F b + vref v1 b)%nat ltac:(eassumption) Dg
                (envok_mono g r s s1 h _ n n1 G1 N1 X1 E) W1 O1
                ltac:(eapply cnt_le; [| exact C1]; intro b; cbv beta; lia)) as [S2 [G2 _]].
    rewrite holds_app, fold_left_app. split; [split; [exact S1 | exact S2] |].
    split; [exact (grows_trans _ _ _ G1 G2) | discriminate].
  - (* EvLen: load the header, then give the list back *)
    destruct (IHHev g TList s D F ltac:(eassumption) Dg E W0 O C) as [S1 [G1 R1]].
    destruct (R1 (VList b vs) n1 eq_refl) as [N1 [s1 [X1 [W1 [O1 [V1 C1]]]]]].
    set (h1 := fold_left step t h) in *.
    destruct V1 as [Hb [x [Hx [Sx [Rx Ix]]]]].
    destruct (cnt_live s1 h1 n1 _ b C1 ltac:(simpl; rewrite Nat.eqb_refl; lia)) as [x' [Hx' Hc]].
    rewrite Hx in Hx'. injection Hx' as <-.
    rewrite <- Rx.
    destruct (load_ok h1 b x 0 HDR Hx Hc ltac:(lia) ltac:(unfold HDR; lia)
               ltac:(rewrite Sx; unfold bsize, HDR, W; lia)
               ltac:(intros j Hj; apply Ix; rewrite Sx; unfold bsize, HDR, W in *; lia)) as [SL EL].
    destruct (dropv_ok (VList b vs) s1 h1 n1 (fun c => envrefs D r c + F c)%nat TList W1 O1
                ltac:(simpl; split; [exact Hb | exists x; auto]) C1) as [SD [GD [WD CD]]].
    cbn [dropv fold_left holds] in SD, GD, WD, CD.
    rewrite holds_app, fold_left_app. fold h1. cbn [fold_left holds app]. rewrite EL.
    split; [split; [exact S1 | split; [exact SL | exact SD]] |].
    split; [exact (grows_trans _ _ _ G1 GD) |].
    intros vv nn Ev. injection Ev as <- <-. split; [exact N1 |]. exists s1.
    split; [exact X1 |]. split; [exact WD |]. split; [exact (sok_mono s1 h1 _ n1 n1 GD (le_n n1) O1) |].
    split; [exact I |]. intro c. specialize (CD c). cbn [vref] in *. lia.
  - (* EvLenAbort *)
    destruct (IHHev g TList s D F ltac:(eassumption) Dg E W0 O C) as [S1 [G1 _]].
    split; [exact S1 |]. split; [exact G1 | discriminate].
  - (* EvIfT *)
    destruct (IHHev1 g TBool s D F ltac:(eassumption) Dg E W0 O C) as [S1 [G1 R1]].
    destruct (R1 (VBool true) n1 eq_refl) as [N1 [s1 [X1 [W1 [O1 [_ C1]]]]]].
    destruct (IHHev2 g ty s1 D F ltac:(eassumption) Dg (envok_mono g r s s1 h _ n n1 G1 N1 X1 E) W1 O1
                ltac:(eapply cnt_le; [| exact C1]; intro b; cbv beta; simpl; lia)) as [S2 [G2 R2]].
    rewrite holds_app, fold_left_app. split; [split; [exact S1 | exact S2] |].
    split; [exact (grows_trans _ _ _ G1 G2) |].
    intros vv nn Ev. destruct (R2 vv nn Ev) as [N2 [s2 [X2 R]]]. split; [lia |].
    exists s2. split; [exact (sext_trans _ _ _ X1 X2) | exact R].
  - (* EvIfF *)
    destruct (IHHev1 g TBool s D F ltac:(eassumption) Dg E W0 O C) as [S1 [G1 R1]].
    destruct (R1 (VBool false) n1 eq_refl) as [N1 [s1 [X1 [W1 [O1 [_ C1]]]]]].
    destruct (IHHev2 g ty s1 D F ltac:(eassumption) Dg (envok_mono g r s s1 h _ n n1 G1 N1 X1 E) W1 O1
                ltac:(eapply cnt_le; [| exact C1]; intro b; cbv beta; simpl; lia)) as [S2 [G2 R2]].
    rewrite holds_app, fold_left_app. split; [split; [exact S1 | exact S2] |].
    split; [exact (grows_trans _ _ _ G1 G2) |].
    intros vv nn Ev. destruct (R2 vv nn Ev) as [N2 [s2 [X2 R]]]. split; [lia |].
    exists s2. split; [exact (sext_trans _ _ _ X1 X2) | exact R].
  - (* EvIfAbort *)
    destruct (IHHev g TBool s D F ltac:(eassumption) Dg E W0 O C) as [S1 [G1 _]].
    split; [exact S1 |]. split; [exact G1 | discriminate].
  - (* EvCall: the callee holds the argument as its parameter; the caller's
       references wait in the frame *)
    match goal with Hd : funs f = Some d, Hd' : funs f = Some ?d0 |- _ => rewrite Hd in Hd'; injection Hd' as <- end.
    destruct (IHHev1 g (fpty d) s D F ltac:(eassumption) Dg E W0 O C) as [S1 [G1 R1]].
    destruct (R1 v n1 eq_refl) as [N1 [s1 [X1 [W1 [O1 [V1 C1]]]]]].
    set (h1 := fold_left step t1 h) in *.
    set (g1 := fun y => if Nat.eqb y (fparam d) then Some (fpty d) else None).
    assert (Ec : envok g1 (upd empty_env (fparam d) v) s1 h1 n1).
    { intros y ty0 Hy. unfold g1, upd in *. destruct (Nat.eqb y (fparam d)); [injection Hy as <-; eauto | discriminate]. }
    assert (Dc : dom_in g1 [fparam d]).
    { intros y ty0 Hy. unfold g1 in Hy. destruct (Nat.eqb y (fparam d)) eqn:B; [| discriminate].
      apply Nat.eqb_eq in B. left. symmetry. exact B. }
    assert (Rp : forall b, envrefs [fparam d] (upd empty_env (fparam d) v) b = vref v b).
    { intro b. simpl. unfold upd. rewrite Nat.eqb_refl. simpl. lia. }
    destruct (IHHev2 g1 (frty d) s1 [fparam d] (fun b => envrefs D r b + F b)%nat (Hfuns f d H) Dc Ec W1 O1
                ltac:(eapply cnt_le; [| exact C1]; intro b; cbv beta; rewrite Rp; lia)) as [S2 [G2 R2]].
    destruct (R2 v' n2 eq_refl) as [N2 [s2 [X2 [W2 [O2 [V2 C2]]]]]].
    set (h2 := fold_left step t2 h1) in *.
    destruct (dropv_ok v s2 h2 n2 (fun b => envrefs D r b + F b + vref v' b)%nat (fpty d) W2 O2
                (vty_mono (fpty d) s1 s2 h1 h2 n1 n2 v G2 N2 X2 V1)
                ltac:(eapply cnt_le; [| exact C2]; intro b; cbv beta; rewrite Rp; lia)) as [S3 [G3 [W3 C3]]].
    rewrite !holds_app, !fold_left_app. fold h1. fold h2.
    split; [split; [exact S1 | split; [exact S2 | exact S3]] |].
    split; [exact (grows_trans _ _ _ G1 (grows_trans _ _ _ G2 G3)) |].
    intros vv nn Ev. injection Ev as <- <-. split; [lia |]. exists s2.
    split; [exact (sext_trans _ _ _ X1 X2) |]. split; [exact W3 |].
    split; [exact (sok_mono s2 h2 _ n2 n2 G3 (le_n n2) O2) |].
    split; [exact (vty_mono _ s2 s2 h2 _ n2 n2 v' G3 (le_n n2) (sext_refl s2) V2) | exact C3].
  - (* EvCallBodyAbort *)
    match goal with Hd : funs f = Some d, Hd' : funs f = Some ?d0 |- _ => rewrite Hd in Hd'; injection Hd' as <- end.
    destruct (IHHev1 g (fpty d) s D F ltac:(eassumption) Dg E W0 O C) as [S1 [G1 R1]].
    destruct (R1 v n1 eq_refl) as [N1 [s1 [X1 [W1 [O1 [V1 C1]]]]]].
    set (h1 := fold_left step t1 h) in *.
    set (g1 := fun y => if Nat.eqb y (fparam d) then Some (fpty d) else None).
    assert (Ec : envok g1 (upd empty_env (fparam d) v) s1 h1 n1).
    { intros y ty0 Hy. unfold g1, upd in *. destruct (Nat.eqb y (fparam d)); [injection Hy as <-; eauto | discriminate]. }
    assert (Dc : dom_in g1 [fparam d]).
    { intros y ty0 Hy. unfold g1 in Hy. destruct (Nat.eqb y (fparam d)) eqn:B; [| discriminate].
      apply Nat.eqb_eq in B. left. symmetry. exact B. }
    assert (Rp : forall b, envrefs [fparam d] (upd empty_env (fparam d) v) b = vref v b).
    { intro b. simpl. unfold upd. rewrite Nat.eqb_refl. simpl. lia. }
    destruct (IHHev2 g1 (frty d) s1 [fparam d] (fun b => envrefs D r b + F b)%nat (Hfuns f d H) Dc Ec W1 O1
                ltac:(eapply cnt_le; [| exact C1]; intro b; cbv beta; rewrite Rp; lia)) as [S2 [G2 _]].
    rewrite holds_app, fold_left_app. fold h1. split; [split; [exact S1 | exact S2] |].
    split; [exact (grows_trans _ _ _ G1 G2) | discriminate].
  - (* EvCallAbort *)
    destruct (IHHev g _ s D F ltac:(eassumption) Dg E W0 O C) as [S1 [G1 _]].
    split; [exact S1 |]. split; [exact G1 | discriminate].
  - (* EvCall2: both arguments, then the body under the two parameters, then
       each parameter's reference given back *)
    match goal with Hd : funs2 f = Some d, Hd' : funs2 f = Some ?d0 |- _ => rewrite Hd in Hd'; injection Hd' as <- end.
    destruct (Hfuns2 f d H) as [Ne Hb].
    destruct (IHHev1 g (f2t1 d) s D F ltac:(eassumption) Dg E W0 O C) as [S1 [G1 R1]].
    destruct (R1 v1 n1 eq_refl) as [N1 [s1 [X1 [W1 [O1 [V1 C1]]]]]].
    set (h1 := fold_left step t1 h) in *.
    destruct (IHHev2 g (f2t2 d) s1 D (fun b => F b + vref v1 b)%nat ltac:(eassumption) Dg
                (envok_mono g r s s1 h h1 n n1 G1 N1 X1 E) W1 O1
                ltac:(eapply cnt_le; [| exact C1]; intro b; cbv beta; lia)) as [S2 [G2 R2]].
    destruct (R2 v2 n2 eq_refl) as [N2 [s2 [X2 [W2 [O2 [V2 C2]]]]]].
    set (h2 := fold_left step t2 h1) in *.
    set (env2 := upd (upd empty_env (f2p1 d) v1) (f2p2 d) v2).
    assert (V1' : vty s2 h2 n2 v1 (f2t1 d)) by exact (vty_mono _ s1 s2 h1 h2 n1 n2 v1 G2 N2 X2 V1).
    assert (Ec : envok (ctx2 d) env2 s2 h2 n2).
    { intros y ty0 Hy. unfold ctx2, env2, upd in *.
      destruct (Nat.eqb y (f2p2 d)); [injection Hy as <-; eauto |].
      destruct (Nat.eqb y (f2p1 d)); [injection Hy as <-; eauto | discriminate]. }
    assert (Dc : dom_in (ctx2 d) [f2p1 d; f2p2 d]).
    { intros y ty0 Hy. unfold ctx2 in Hy.
      destruct (Nat.eqb y (f2p2 d)) eqn:B2; [apply Nat.eqb_eq in B2; right; left; symmetry; exact B2 |].
      destruct (Nat.eqb y (f2p1 d)) eqn:B1; [apply Nat.eqb_eq in B1; left; symmetry; exact B1 | discriminate]. }
    assert (Rp : forall b, envrefs [f2p1 d; f2p2 d] env2 b = (vref v1 b + vref v2 b)%nat).
    { intro b. unfold env2, upd. simpl. rewrite Nat.eqb_refl.
      replace (Nat.eqb (f2p1 d) (f2p2 d)) with false by (symmetry; apply Nat.eqb_neq; exact Ne).
      rewrite Nat.eqb_refl. simpl. lia. }
    destruct (IHHev3 (ctx2 d) (f2rty d) s2 [f2p1 d; f2p2 d] (fun b => envrefs D r b + F b)%nat Hb Dc Ec W2 O2
                ltac:(eapply cnt_le; [| exact C2]; intro b; cbv beta; rewrite Rp; lia)) as [S3 [G3 R3]].
    destruct (R3 v' n3 eq_refl) as [N3 [s3 [X3 [W3 [O3 [V3 C3]]]]]].
    set (h3 := fold_left step t3 h2) in *.
    destruct (dropv_ok v1 s3 h3 n3 (fun b => envrefs D r b + F b + vref v' b + vref v2 b)%nat (f2t1 d) W3 O3
                (vty_mono (f2t1 d) s2 s3 h2 h3 n2 n3 v1 G3 N3 X3 V1')
                ltac:(eapply cnt_le; [| exact C3]; intro b; cbv beta; rewrite Rp; lia)) as [S4 [G4 [W4 C4]]].
    set (h4 := fold_left step (dropv h3 v1) h3) in *.
    destruct (dropv_ok v2 s3 h4 n3 (fun b => envrefs D r b + F b + vref v' b)%nat (f2t2 d) W4
                (sok_mono s3 h3 h4 n3 n3 G4 (le_n n3) O3)
                (vty_mono (f2t2 d) s2 s3 h2 h4 n2 n3 v2 (grows_trans _ _ _ G3 G4) N3 X3 V2)
                ltac:(eapply cnt_le; [| exact C4]; intro b; cbv beta; lia)) as [S5 [G5 [W5 C5]]].
    rewrite !holds_app, !fold_left_app. fold h1. fold h2. fold h3. fold h4.
    split; [split; [exact S1 | split; [exact S2 | split; [exact S3 | split; [exact S4 | exact S5]]]] |].
    split; [exact (grows_trans _ _ _ G1 (grows_trans _ _ _ G2 (grows_trans _ _ _ G3 (grows_trans _ _ _ G4 G5)))) |].
    intros vv nn Ev. injection Ev as <- <-. split; [lia |]. exists s3.
    split; [exact (sext_trans _ _ _ X1 (sext_trans _ _ _ X2 X3)) |]. split; [exact W5 |].
    split; [exact (sok_mono s3 h3 _ n3 n3 (grows_trans _ _ _ G4 G5) (le_n n3) O3) |].
    split; [exact (vty_mono _ s3 s3 h3 _ n3 n3 v' (grows_trans _ _ _ G4 G5) (le_n n3) (sext_refl s3) V3) | exact C5].
  - (* EvCall2BodyAbort *)
    match goal with Hd : funs2 f = Some d, Hd' : funs2 f = Some ?d0 |- _ => rewrite Hd in Hd'; injection Hd' as <- end.
    destruct (Hfuns2 f d H) as [Ne Hb].
    destruct (IHHev1 g (f2t1 d) s D F ltac:(eassumption) Dg E W0 O C) as [S1 [G1 R1]].
    destruct (R1 v1 n1 eq_refl) as [N1 [s1 [X1 [W1 [O1 [V1 C1]]]]]].
    set (h1 := fold_left step t1 h) in *.
    destruct (IHHev2 g (f2t2 d) s1 D (fun b => F b + vref v1 b)%nat ltac:(eassumption) Dg
                (envok_mono g r s s1 h h1 n n1 G1 N1 X1 E) W1 O1
                ltac:(eapply cnt_le; [| exact C1]; intro b; cbv beta; lia)) as [S2 [G2 R2]].
    destruct (R2 v2 n2 eq_refl) as [N2 [s2 [X2 [W2 [O2 [V2 C2]]]]]].
    set (h2 := fold_left step t2 h1) in *.
    set (env2 := upd (upd empty_env (f2p1 d) v1) (f2p2 d) v2).
    assert (V1' : vty s2 h2 n2 v1 (f2t1 d)) by exact (vty_mono _ s1 s2 h1 h2 n1 n2 v1 G2 N2 X2 V1).
    assert (Ec : envok (ctx2 d) env2 s2 h2 n2).
    { intros y ty0 Hy. unfold ctx2, env2, upd in *.
      destruct (Nat.eqb y (f2p2 d)); [injection Hy as <-; eauto |].
      destruct (Nat.eqb y (f2p1 d)); [injection Hy as <-; eauto | discriminate]. }
    assert (Dc : dom_in (ctx2 d) [f2p1 d; f2p2 d]).
    { intros y ty0 Hy. unfold ctx2 in Hy.
      destruct (Nat.eqb y (f2p2 d)) eqn:B2; [apply Nat.eqb_eq in B2; right; left; symmetry; exact B2 |].
      destruct (Nat.eqb y (f2p1 d)) eqn:B1; [apply Nat.eqb_eq in B1; left; symmetry; exact B1 | discriminate]. }
    assert (Rp : forall b, envrefs [f2p1 d; f2p2 d] env2 b = (vref v1 b + vref v2 b)%nat).
    { intro b. unfold env2, upd. simpl. rewrite Nat.eqb_refl.
      replace (Nat.eqb (f2p1 d) (f2p2 d)) with false by (symmetry; apply Nat.eqb_neq; exact Ne).
      rewrite Nat.eqb_refl. simpl. lia. }
    destruct (IHHev3 (ctx2 d) (f2rty d) s2 [f2p1 d; f2p2 d] (fun b => envrefs D r b + F b)%nat Hb Dc Ec W2 O2
                ltac:(eapply cnt_le; [| exact C2]; intro b; cbv beta; rewrite Rp; lia)) as [S3 [G3 _]].
    rewrite !holds_app, !fold_left_app. fold h1. fold h2.
    split; [split; [exact S1 | split; [exact S2 | exact S3]] |].
    split; [exact (grows_trans _ _ _ G1 (grows_trans _ _ _ G2 G3)) | discriminate].
  - (* EvCall2Abort1 *)
    destruct (IHHev g _ s D F ltac:(eassumption) Dg E W0 O C) as [S1 [G1 _]].
    split; [exact S1 |]. split; [exact G1 | discriminate].
  - (* EvCall2Abort2 *)
    destruct (IHHev1 g _ s D F ltac:(eassumption) Dg E W0 O C) as [S1 [G1 R1]].
    destruct (R1 v1 n1 eq_refl) as [N1 [s1 [X1 [W1 [O1 [V1 C1]]]]]].
    destruct (IHHev2 g _ s1 D (fun b => F b + vref v1 b)%nat ltac:(eassumption) Dg
                (envok_mono g r s s1 h _ n n1 G1 N1 X1 E) W1 O1
                ltac:(eapply cnt_le; [| exact C1]; intro b; cbv beta; lia)) as [S2 [G2 _]].
    rewrite holds_app, fold_left_app. split; [split; [exact S1 | exact S2] |].
    split; [exact (grows_trans _ _ _ G1 G2) | discriminate].
  - (* EvBox: a fresh box; the value moves into its slot *)
    destruct (IHHev g t0 s D F ltac:(eassumption) Dg E W0 O C) as [S1 [G1 R1]].
    destruct (R1 v n1 eq_refl) as [N1 [s1 [X1 [W1 [O1 [V1 C1]]]]]].
    set (h1 := fold_left step t h) in *.
    destruct (fresh_ok h1 n1 (bsize 1) RB W1 (bsize_pos 1)) as [SF [GF [WF [[y [Ey [Sy [Ry [_ Iy]]]]] RF]]]].
    set (h2 := fold_left step [EAlloc n1 (bsize 1) RB; EStore n1 0 (bsize 1) RB] h1) in *.
    assert (Fr : s1 n1 = None) by exact (sok_fresh s1 h1 n1 O1).
    set (s2 := supd s1 n1 v).
    assert (X2 : sext s1 s2).
    { intros c w Hc. unfold s2, supd. destruct (Nat.eqb c n1) eqn:B; [| exact Hc].
      apply Nat.eqb_eq in B. subst. rewrite Fr in Hc. discriminate. }
    assert (Bx : boxblk h2 n1) by (exists y; repeat split; auto).
    rewrite holds_app, fold_left_app. fold h1. fold h2.
    split; [split; [exact S1 | exact SF] |]. split; [exact (grows_trans _ _ _ G1 GF) |].
    intros vv nn Ev. injection Ev as <- <-. split; [lia |]. exists s2.
    split; [exact (sext_trans _ _ _ X1 X2) |]. split; [exact WF |].
    split.
    { intros c w Hc. unfold s2, supd in Hc. destruct (Nat.eqb c n1) eqn:B.
      - injection Hc as <-. apply Nat.eqb_eq in B. subst c. split; [lia |]. split; [exact Bx |].
        exists t0. exact (vty_mono t0 s1 s2 h1 h2 n1 (S n1) v GF ltac:(lia) X2 V1).
      - destruct (O1 c w Hc) as [Lc [Bc [tc Vc]]]. split; [lia |].
        split; [exact (boxblk_mono h1 h2 c GF Bc) |].
        exists tc. exact (vty_mono tc s1 s2 h1 h2 n1 (S n1) w GF ltac:(lia) X2 Vc). }
    split.
    { simpl. split; [lia |]. split; [exact Bx |]. split; [unfold s2, supd; rewrite Nat.eqb_refl; reflexivity |].
      exact (vty_mono t0 s1 s2 h1 h2 n1 (S n1) v GF ltac:(lia) X2 V1). }
    intro c.
    rewrite (hrefs_fresh s1 s2 h1 h2 n1 c
               ltac:(intros c' Ne; rewrite RF; apply Nat.eqb_neq in Ne; rewrite Ne; reflexivity)
               ltac:(rewrite RF, Nat.eqb_refl; reflexivity)
               ltac:(intros c' Lc; unfold s2, supd; replace (Nat.eqb c' n1) with false
                       by (symmetry; apply Nat.eqb_neq; lia); reflexivity)).
    unfold s2, supd at 1. rewrite Nat.eqb_refl. cbn [vref oref]. rewrite RF. specialize (C1 c). cbv beta in C1.
    destruct (Nat.eqb c n1) eqn:B.
    + apply Nat.eqb_eq in B. subst c. rewrite Nat.eqb_refl.
      unfold rcof in C1. rewrite (proj2 (W1 n1) (le_n n1)) in C1. lia.
    + rewrite Nat.eqb_sym, B. lia.
  - (* EvBoxAbort *)
    destruct (IHHev g _ s D F ltac:(eassumption) Dg E W0 O C) as [S1 [G1 _]].
    split; [exact S1 |]. split; [exact G1 | discriminate].
  - (* EvUnbox: read the slot, share what it holds, give the box back *)
    destruct (IHHev g (TBox ty) s D F ltac:(eassumption) Dg E W0 O C) as [S1 [G1 R1]].
    destruct (R1 (VBox b w) n1 eq_refl) as [N1 [s1 [X1 [W1 [O1 [V1 C1]]]]]].
    set (h1 := fold_left step t h) in *.
    pose proof V1 as V1'. destruct V1' as [Hb [[x [Hx [Sz [Rp Ini]]]] [Hs Vw]]].
    destruct (cnt_live s1 h1 n1 _ b C1 ltac:(simpl; rewrite Nat.eqb_refl; lia)) as [x' [Hx' Hc]].
    rewrite Hx in Hx'. injection Hx' as <-.
    destruct (load_ok h1 b x HDR W Hx Hc ltac:(unfold HDR; lia) ltac:(unfold W; lia)
                ltac:(rewrite Sz; unfold bsize, HDR, W; simpl; lia)
                ltac:(intros i Hi; apply Ini; rewrite Sz; unfold bsize, HDR, W in *; simpl in *; lia)) as [SL EL].
    rewrite Rp in SL, EL.
    assert (Lw : forall c, (vref w c <= (envrefs D r c + F c + vref (VBox b w) c) + hrefs s1 h1 n1 c)%nat).
    { intro c. pose proof (hrefs_term_le s1 h1 n1 b c Hb) as T. unfold hterm in T. rewrite Hs in T.
      replace (0 <? rcof h1 b) with true in T by (symmetry; apply Z.ltb_lt; unfold rcof; rewrite Hx; lia).
      simpl in T. lia. }
    destruct (dup_ok s1 h1 n1 _ w W1 C1 Lw) as [SU [GU [WU CU]]].
    set (h2 := fold_left step (dup w) h1) in *.
    destruct (dropv_ok (VBox b w) s1 h2 n1 (fun c => envrefs D r c + F c + vref w c)%nat (TBox ty) WU
                (sok_mono s1 h1 h2 n1 n1 GU (le_n n1) O1)
                (vty_mono (TBox ty) s1 s1 h1 h2 n1 n1 (VBox b w) GU (le_n n1) (sext_refl s1) V1)
                ltac:(eapply cnt_le; [| exact CU]; intro c; cbv beta; lia)) as [SD [GD [WD CD]]].
    set (h3 := fold_left step (dropv h2 (VBox b w)) h2) in *.
    rewrite holds_app, fold_left_app. fold h1. cbn [holds fold_left]. rewrite EL.
    rewrite holds_app, fold_left_app. fold h2. fold h3.
    split; [split; [exact S1 | split; [exact SL | split; [exact SU | exact SD]]] |].
    split; [exact (grows_trans _ _ _ G1 (grows_trans _ _ _ GU GD)) |].
    intros vv nn Ev. injection Ev as <- <-. split; [exact N1 |]. exists s1.
    split; [exact X1 |]. split; [exact WD |].
    split; [exact (sok_mono s1 h1 h3 n1 n1 (grows_trans _ _ _ GU GD) (le_n n1) O1) |].
    split; [exact (vty_mono ty s1 s1 h1 h3 n1 n1 w (grows_trans _ _ _ GU GD) (le_n n1) (sext_refl s1) Vw) |].
    exact CD.
  - (* EvUnboxAbort *)
    destruct (IHHev g _ s D F ltac:(eassumption) Dg E W0 O C) as [S1 [G1 _]].
    split; [exact S1 |]. split; [exact G1 | discriminate].
Qed.

(* ── Statements ─────────────────────────────────────────────────────────── *)

(* The variables a statement binds. *)
Fixpoint bv (s : stmt) : list Var :=
  match s with
  | SLet x _ => [x]
  | SSeq s1 s2 | SIf _ s1 s2 => bv s1 ++ bv s2
  | SWhile _ s1 => bv s1
  | _ => []
  end.

Definition extends (g g' : Ctx) : Prop := forall x t, g x = Some t -> g' x = Some t.

Lemma styped_extends : forall g s g', styped g s g' -> extends g g'.
Proof.
  intros g s g' H. induction H; intros y ty0 Hy; auto.
  unfold tupd. destruct (Nat.eqb y x) eqn:B; [apply Nat.eqb_eq in B; subst; congruence | exact Hy].
Qed.

Lemma styped_dom : forall g s g' D, styped g s g' -> dom_in g D -> incl (bv s) D -> dom_in g' D.
Proof.
  intros g s g' D H. induction H; intros Dg I; simpl in I; auto.
  - intros y ty0 Hy. unfold tupd in Hy. destruct (Nat.eqb y x) eqn:B; [| exact (Dg y ty0 Hy)].
    apply Nat.eqb_eq in B. subst. apply I. left. reflexivity.
  - apply incl_app_inv in I as [I1 I2]. exact (IHstyped2 (IHstyped1 Dg I1) I2).
Qed.

Lemma envok_weaken : forall g g' r s h n, extends g g' -> envok g' r s h n -> envok g r s h n.
Proof. intros g g' r s h n X E x t Hx. exact (E x t (X x t Hx)). Qed.

Lemma envok_upd : forall g r s h n x v t,
  envok g r s h n -> vty s h n v t -> envok (tupd g x t) (upd r x v) s h n.
Proof.
  intros g r s h n x v t E V y ty Hy. unfold tupd, upd in *.
  destruct (Nat.eqb y x); [injection Hy as <-; eauto | exact (E y ty Hy)].
Qed.

Lemma envok_assign : forall g r s h n x v t,
  envok g r s h n -> g x = Some t -> vty s h n v t -> envok g (upd r x v) s h n.
Proof.
  intros g r s h n x v t E Hx V y ty Hy. unfold upd.
  destruct (Nat.eqb y x) eqn:B; [apply Nat.eqb_eq in B; subst; rewrite Hx in Hy; injection Hy as <-; eauto |].
  exact (E y ty Hy).
Qed.

(* An expression evaluated by a statement: nothing waits in the frame. *)
Lemma eval_top : forall h r n e tr o g t s D,
  eval h r n e tr o -> etyped g e t -> dom_in g D -> envok g r s h n -> wfh h n -> sok s h n ->
  cnt s h n (envrefs D r) ->
  holds safe_ev h tr /\ grows h (fold_left step tr h) /\
  (forall v n', o = Val v n' -> (n <= n')%nat /\ exists s', sext s s' /\
     wfh (fold_left step tr h) n' /\ sok s' (fold_left step tr h) n' /\
     vty s' (fold_left step tr h) n' v t /\
     cnt s' (fold_left step tr h) n' (fun b => envrefs D r b + vref v b)%nat).
Proof.
  intros h r n e tr o g t s D Hev Ht Dg E W0 O C.
  destruct (eval_safe h r n e tr o Hev g t s D (fun _ => 0%nat) Ht Dg E W0 O
              ltac:(eapply cnt_le; [| exact C]; intro b; cbv beta; lia)) as [S [G R]].
  split; [exact S |]. split; [exact G |].
  intros v n' Ev. destruct (R v n' Ev) as [N [s' [X [W1 [O1 [V C1]]]]]]. split; [exact N |].
  exists s'. split; [exact X |]. split; [exact W1 |]. split; [exact O1 |].
  split; [exact V |]. eapply cnt_le; [| exact C1]. intro b. cbv beta. lia.
Qed.

Lemma exec_safe : forall h r n s tr o,
  exec h r n s tr o ->
  forall g g' st D, styped g s g' -> dom_in g D -> incl (bv s) D -> NoDup D ->
  envok g r st h n -> wfh h n -> sok st h n -> cnt st h n (envrefs D r) ->
  holds safe_ev h tr /\ grows h (fold_left step tr h) /\
  (forall r' n', o = Normal r' n' -> (n <= n')%nat /\ exists st', sext st st' /\
     wfh (fold_left step tr h) n' /\ sok st' (fold_left step tr h) n' /\
     envok g' r' st' (fold_left step tr h) n' /\ cnt st' (fold_left step tr h) n' (envrefs D r')).
Proof.
  intros h r n s tr o Hx. induction Hx; intros g g' st D Ht Dg Ib ND E W0 O C; inversion Ht; subst; simpl in Ib.
  - (* XSkip *)
    split; [exact I |]. split; [apply grows_refl |].
    intros r' n' Eq. injection Eq as <- <-. split; [lia |]. exists st.
    split; [apply sext_refl |]. split; [exact W0 |]. split; [exact O |]. split; [exact E | exact C].
  - (* XLet: the value moves into the variable *)
    destruct (eval_top h r n e t _ g _ st D H ltac:(eassumption) Dg E W0 O C) as [S1 [G1 R1]].
    destruct (R1 v n1 eq_refl) as [N1 [q1 [X1 [W1 [O1 [V1 C1]]]]]].
    split; [exact S1 |]. split; [exact G1 |].
    intros r' n' Eq. injection Eq as <- <-. split; [exact N1 |]. exists q1.
    split; [exact X1 |]. split; [exact W1 |]. split; [exact O1 |].
    split; [apply envok_upd; [exact (envok_mono g r st q1 h _ n n1 G1 N1 X1 E) | exact V1] |].
    intro c0. pose proof (envrefs_upd D r x v c0 (Ib x (or_introl eq_refl)) ND) as U.
    specialize (C1 c0). cbv beta in C1. lia.
  - (* XAssign: the new value moves in, the old one's reference is given back *)
    assert (Hin : In x D) by (match goal with Hg : g' x = Some _ |- _ => exact (Dg x _ Hg) end).
    destruct (eval_top h r n e t _ g' _ st D H ltac:(eassumption) Dg E W0 O C) as [S1 [G1 R1]].
    destruct (R1 v n1 eq_refl) as [N1 [q1 [X1 [W1 [O1 [V1 C1]]]]]].
    set (h1 := fold_left step t h) in *.
    assert (E1 := envok_mono g' r st q1 h h1 n n1 G1 N1 X1 E).
    destruct (dropo_ok (r x) q1 h1 n1 (envrefs D (upd r x v)) W1 O1
                ltac:(intros v0 Hv0; match goal with Hg : g' x = Some ?tx |- _ =>
                        destruct (E1 x tx Hg) as [v1 [Hv1 Vv1]]; rewrite Hv0 in Hv1; injection Hv1 as <-;
                        exists tx; exact Vv1 end)
                ltac:(intro c0; cbv beta; pose proof (envrefs_upd D r x v c0 Hin ND) as U; specialize (C1 c0);
                      cbv beta in C1; lia))
      as [S2 [G2 [W2 C2]]].
    rewrite holds_app, fold_left_app. fold h1.
    split; [split; [exact S1 | exact S2] |]. split; [exact (grows_trans _ _ _ G1 G2) |].
    intros r' n' Eq. injection Eq as <- <-. split; [exact N1 |]. exists q1.
    split; [exact X1 |]. split; [exact W2 |].
    split; [exact (sok_mono q1 h1 _ n1 n1 G2 (le_n n1) O1) |].
    split; [| exact C2].
    eapply envok_assign; [| eassumption |].
    + exact (envok_mono g' r q1 q1 h1 _ n1 n1 G2 (le_n n1) (sext_refl q1) E1).
    + exact (vty_mono _ q1 q1 h1 _ n1 n1 v G2 (le_n n1) (sext_refl q1) V1).
  - (* XLetAbort *)
    destruct (eval_top h r n e t _ g _ st D H ltac:(eassumption) Dg E W0 O C) as [S1 [G1 _]].
    split; [exact S1 |]. split; [exact G1 | discriminate].
  - (* XAssignAbort *)
    destruct (eval_top h r n e t _ g' _ st D H ltac:(eassumption) Dg E W0 O C) as [S1 [G1 _]].
    split; [exact S1 |]. split; [exact G1 | discriminate].
  - (* XSet: copy into a fresh block, store, give the old block back *)
    rename H into Hrx.
    assert (Hin : In x D) by (match goal with Hg : g' x = Some _ |- _ => exact (Dg x _ Hg) end).
    destruct (eval_top h r n ei t1 _ g' TInt st D H0 ltac:(eassumption) Dg E W0 O C) as [S1 [G1 R1]].
    destruct (R1 (VInt i) n1 eq_refl) as [N1 [q1 [X1 [W1 [O1 [_ C1]]]]]].
    set (h1 := fold_left step t1 h) in *.
    assert (E1 := envok_mono g' r st q1 h h1 n n1 G1 N1 X1 E).
    destruct (eval_top h1 r n1 ev t2 _ g' TInt q1 D H1 ltac:(eassumption) Dg E1 W1 O1
                ltac:(eapply cnt_le; [| exact C1]; intro c0; cbv beta; simpl; lia)) as [S2 [G2 R2]].
    destruct (R2 (VInt k) n2 eq_refl) as [N2 [q2 [X2 [W2 [O2 [_ C2]]]]]].
    set (h2 := fold_left step t2 h1) in *.
    assert (E2 := envok_mono g' r q1 q2 h1 h2 n1 n2 G2 N2 X2 E1).
    assert (C2' : cnt q2 h2 n2 (envrefs D r)) by (eapply cnt_le; [| exact C2]; intro c0; cbv beta; simpl; lia).
    destruct (E2 x TList ltac:(eassumption)) as [vx [Hvx Vx]]. rewrite Hrx in Hvx. injection Hvx as <-.
    pose proof Vx as Vx'. destruct Vx' as [Hb [x0 [Hx0 [Sx [Rx Ix]]]]].
    destruct (cnt_live q2 h2 n2 _ b C2' ltac:(pose proof (envrefs_in D r x b Hin) as L; rewrite Hrx in L;
                                         simpl in L; rewrite Nat.eqb_refl in L; lia)) as [x0' [Hx0' Hc]].
    rewrite Hx0 in Hx0'. injection Hx0' as <-.
    assert (Pz := bsize_pos (length vs)).
    destruct (load_ok h2 b x0 0 (bsize (length vs)) Hx0 Hc ltac:(lia) Pz ltac:(lia)
               ltac:(intros j Hj; apply Ix; lia)) as [SL EL].
    rewrite Rx in SL, EL.
    destruct (fresh_list_ok q2 h2 n2 (length vs) (envrefs D r) W2 O2 C2')
      as [SF [GF [WF [OF [[y [Ey [Sy [Ry Iy]]]] CF]]]]].
    cbn [fold_left holds] in SF, GF, WF, OF, Ey, CF.
    set (h3 := step (step h2 (EAlloc n2 (bsize (length vs)) RL)) (EStore n2 0 (bsize (length vs)) RL)) in *.
    assert (Hoff : 0 <= HDR + i * W) by (unfold HDR, W; lia).
    destruct (store_ok h3 (S n2) n2 y (HDR + i * W) W WF Ey
               ltac:(pose proof (CF n2) as Cn; cbv beta in Cn; rewrite Nat.eqb_refl in Cn; unfold rcof in Cn; rewrite Ey in Cn; lia)
               Hoff ltac:(unfold W; lia)
               ltac:(rewrite Sy; unfold bsize, HDR, W in *; lia)) as [SS [GS [WS [ES RS]]]].
    rewrite Ry in SS, GS, WS, ES, RS.
    set (h4 := step h3 (EStore n2 (HDR + i * W) W RL)) in *.
    set (nv := VList n2 (set_nth vs (Z.to_nat i) k)).
    assert (Hs4 : forall c0, hrefs q2 h4 (S n2) c0 = hrefs q2 h3 (S n2) c0).
    { intro c0. apply hrefs_rc. exact RS. }
    destruct (dropv_ok (VList b vs) q2 h4 (S n2) (envrefs D (upd r x nv)) TList WS
                (sok_mono q2 h2 h4 n2 (S n2) (grows_trans _ _ _ GF GS) ltac:(lia) O2)
                (vty_mono TList q2 q2 h2 h4 n2 (S n2) (VList b vs) (grows_trans _ _ _ GF GS) ltac:(lia) (sext_refl q2) Vx))
      as [SD [GD [WD CD]]].
    { intro c0. cbv beta. pose proof (envrefs_upd D r x nv c0 Hin ND) as U. rewrite Hrx in U.
      rewrite Hs4, (RS c0). specialize (CF c0). cbv beta in CF. unfold nv in U |- *. cbn [oref vref] in U |- *.
      destruct (Nat.eqb n2 c0); destruct (Nat.eqb b c0); lia. }
    cbn [dropv fold_left holds] in SD, GD, WD, CD.
    rewrite !holds_app, !fold_left_app. fold h1. fold h2. cbn [fold_left holds app]. rewrite EL. fold h3. fold h4.
    destruct SF as [SA [SB _]].
    split; [split; [exact S1 | split; [exact S2 | split; [exact SL | split; [exact SA | split; [exact SB | split; [exact SS | exact SD]]]]]] |].
    split; [exact (grows_trans _ _ _ G1 (grows_trans _ _ _ G2 (grows_trans _ _ _ GF (grows_trans _ _ _ GS GD)))) |].
    intros r' n' Eq. injection Eq as <- <-. split; [lia |]. exists q2.
    split; [exact (sext_trans _ _ _ X1 X2) |]. split; [exact WD |].
    split; [exact (sok_mono q2 h2 _ n2 (S n2) (grows_trans _ _ _ GF (grows_trans _ _ _ GS GD)) ltac:(lia) O2) |].
    split; [| exact CD].
    eapply envok_assign; [| eassumption |].
    + exact (envok_mono g' r q2 q2 h2 _ n2 (S n2) (grows_trans _ _ _ GF (grows_trans _ _ _ GS GD)) ltac:(lia) (sext_refl q2) E2).
    + apply (vty_mono TList q2 q2 h4 _ (S n2) (S n2) nv GD (le_n _) (sext_refl q2)).
      simpl. split; [lia |]. eexists. split; [exact ES |]. simpl.
      rewrite length_set_nth. split; [exact Sy |]. split; [reflexivity |].
      intros j Hj. apply mark_mono. apply Iy. exact Hj.
  - (* XSetOut *)
    destruct (eval_top h r n ei t1 _ g' TInt st D H0 ltac:(eassumption) Dg E W0 O C) as [S1 [G1 R1]].
    destruct (R1 (VInt i) n1 eq_refl) as [N1 [q1 [X1 [W1 [O1 [_ C1]]]]]].
    destruct (eval_top _ r n1 ev t2 _ g' TInt q1 D H1 ltac:(eassumption) Dg (envok_mono g' r st q1 h _ n n1 G1 N1 X1 E) W1 O1
                ltac:(eapply cnt_le; [| exact C1]; intro c0; cbv beta; simpl; lia)) as [S2 [G2 _]].
    rewrite holds_app, fold_left_app. split; [split; [exact S1 | exact S2] |].
    split; [exact (grows_trans _ _ _ G1 G2) | discriminate].
  - (* XSetAbort1 *)
    destruct (eval_top h r n ei t1 _ g' TInt st D H ltac:(eassumption) Dg E W0 O C) as [S1 [G1 _]].
    split; [exact S1 |]. split; [exact G1 | discriminate].
  - (* XSetAbort2 *)
    destruct (eval_top h r n ei t1 _ g' TInt st D H ltac:(eassumption) Dg E W0 O C) as [S1 [G1 R1]].
    destruct (R1 v n1 eq_refl) as [N1 [q1 [X1 [W1 [O1 [_ C1]]]]]].
    destruct (eval_top _ r n1 ev t2 _ g' TInt q1 D H0 ltac:(eassumption) Dg (envok_mono g' r st q1 h _ n n1 G1 N1 X1 E) W1 O1
                ltac:(eapply cnt_le; [| exact C1]; intro c0; cbv beta; simpl; lia)) as [S2 [G2 _]].
    rewrite holds_app, fold_left_app. split; [split; [exact S1 | exact S2] |].
    split; [exact (grows_trans _ _ _ G1 G2) | discriminate].
  - (* XSeq *)
    apply incl_app_inv in Ib as [I1 I2].
    destruct (IHHx1 g g1 st D ltac:(eassumption) Dg I1 ND E W0 O C) as [S1 [G1 R1]].
    destruct (R1 r1 n1 eq_refl) as [N1 [q1 [X1 [W1 [O1 [E1 C1]]]]]].
    destruct (IHHx2 g1 g' q1 D ltac:(eassumption) (styped_dom g s1 g1 D ltac:(eassumption) Dg I1) I2 ND E1 W1 O1 C1)
      as [S2 [G2 R2]].
    rewrite holds_app, fold_left_app. split; [split; [exact S1 | exact S2] |].
    split; [exact (grows_trans _ _ _ G1 G2) |].
    intros r' n' Eq. destruct (R2 r' n' Eq) as [N2 [q2 [X2 R]]]. split; [lia |].
    exists q2. split; [exact (sext_trans _ _ _ X1 X2) | exact R].
  - (* XSeqAbort *)
    apply incl_app_inv in Ib as [I1 I2].
    destruct (IHHx g g1 st D ltac:(eassumption) Dg I1 ND E W0 O C) as [S1 [G1 _]].
    split; [exact S1 |]. split; [exact G1 | discriminate].
  - (* XIfT *)
    apply incl_app_inv in Ib as [I1 I2].
    destruct (eval_top h r n c t _ g' TBool st D H ltac:(eassumption) Dg E W0 O C) as [S1 [G1 R1]].
    destruct (R1 (VBool true) n1 eq_refl) as [N1 [q1 [X1 [W1 [O1 [_ C1]]]]]].
    destruct (IHHx g' g1 q1 D ltac:(eassumption) Dg I1 ND (envok_mono g' r st q1 h _ n n1 G1 N1 X1 E) W1 O1
                ltac:(eapply cnt_le; [| exact C1]; intro c0; cbv beta; simpl; lia)) as [S2 [G2 R2]].
    rewrite holds_app, fold_left_app. split; [split; [exact S1 | exact S2] |].
    split; [exact (grows_trans _ _ _ G1 G2) |].
    intros r' n' Eq. destruct (R2 r' n' Eq) as [N2 [q2 [X2 [W2 [O2 [E2 C2]]]]]].
    split; [lia |]. exists q2. split; [exact (sext_trans _ _ _ X1 X2) |].
    split; [exact W2 |]. split; [exact O2 |]. split; [| exact C2].
    exact (envok_weaken g' g1 r' q2 _ n' (styped_extends g' s1 g1 ltac:(eassumption)) E2).
  - (* XIfF *)
    apply incl_app_inv in Ib as [I1 I2].
    destruct (eval_top h r n c t _ g' TBool st D H ltac:(eassumption) Dg E W0 O C) as [S1 [G1 R1]].
    destruct (R1 (VBool false) n1 eq_refl) as [N1 [q1 [X1 [W1 [O1 [_ C1]]]]]].
    destruct (IHHx g' g2 q1 D ltac:(eassumption) Dg I2 ND (envok_mono g' r st q1 h _ n n1 G1 N1 X1 E) W1 O1
                ltac:(eapply cnt_le; [| exact C1]; intro c0; cbv beta; simpl; lia)) as [S2 [G2 R2]].
    rewrite holds_app, fold_left_app. split; [split; [exact S1 | exact S2] |].
    split; [exact (grows_trans _ _ _ G1 G2) |].
    intros r' n' Eq. destruct (R2 r' n' Eq) as [N2 [q2 [X2 [W2 [O2 [E2 C2]]]]]].
    split; [lia |]. exists q2. split; [exact (sext_trans _ _ _ X1 X2) |].
    split; [exact W2 |]. split; [exact O2 |]. split; [| exact C2].
    exact (envok_weaken g' g2 r' q2 _ n' (styped_extends g' s2 g2 ltac:(eassumption)) E2).
  - (* XIfAbort *)
    destruct (eval_top h r n c t _ g' TBool st D H ltac:(eassumption) Dg E W0 O C) as [S1 [G1 _]].
    split; [exact S1 |]. split; [exact G1 | discriminate].
  - (* XWhileF *)
    destruct (eval_top h r n c t _ g' TBool st D H ltac:(eassumption) Dg E W0 O C) as [S1 [G1 R1]].
    destruct (R1 (VBool false) n1 eq_refl) as [N1 [q1 [X1 [W1 [O1 [_ C1]]]]]].
    split; [exact S1 |]. split; [exact G1 |].
    intros r' n' Eq. injection Eq as <- <-. split; [exact N1 |]. exists q1.
    split; [exact X1 |]. split; [exact W1 |]. split; [exact O1 |].
    split; [exact (envok_mono g' r st q1 h _ n n1 G1 N1 X1 E) |].
    eapply cnt_le; [| exact C1]. intro c0. cbv beta. simpl. lia.
  - (* XWhileT *)
    destruct (eval_top h r n c t _ g' TBool st D H ltac:(eassumption) Dg E W0 O C) as [S1 [G1 R1]].
    destruct (R1 (VBool true) n1 eq_refl) as [N1 [q1 [X1 [W1 [O1 [_ C1]]]]]].
    destruct (IHHx1 g' g1 q1 D ltac:(eassumption) Dg Ib ND (envok_mono g' r st q1 h _ n n1 G1 N1 X1 E) W1 O1
                ltac:(eapply cnt_le; [| exact C1]; intro c0; cbv beta; simpl; lia)) as [S2 [G2 R2]].
    destruct (R2 r1 n2 eq_refl) as [N2 [q2 [X2 [W2 [O2 [E2 C2]]]]]].
    destruct (IHHx2 g' g' q2 D Ht Dg Ib ND
                (envok_weaken g' g1 r1 q2 _ n2 (styped_extends g' s g1 ltac:(eassumption)) E2) W2 O2 C2)
      as [S3 [G3 R3]].
    rewrite !holds_app, !fold_left_app. split; [split; [exact S1 | split; [exact S2 | exact S3]] |].
    split; [exact (grows_trans _ _ _ G1 (grows_trans _ _ _ G2 G3)) |].
    intros r' n' Eq. destruct (R3 r' n' Eq) as [N3 [q3 [X3 R]]]. split; [lia |].
    exists q3. split; [exact (sext_trans _ _ _ X1 (sext_trans _ _ _ X2 X3)) | exact R].
  - (* XWhileBodyAbort *)
    destruct (eval_top h r n c t _ g' TBool st D H ltac:(eassumption) Dg E W0 O C) as [S1 [G1 R1]].
    destruct (R1 (VBool true) n1 eq_refl) as [N1 [q1 [X1 [W1 [O1 [_ C1]]]]]].
    destruct (IHHx g' g1 q1 D ltac:(eassumption) Dg Ib ND (envok_mono g' r st q1 h _ n n1 G1 N1 X1 E) W1 O1
                ltac:(eapply cnt_le; [| exact C1]; intro c0; cbv beta; simpl; lia)) as [S2 [G2 _]].
    rewrite holds_app, fold_left_app. split; [split; [exact S1 | exact S2] |].
    split; [exact (grows_trans _ _ _ G1 G2) | discriminate].
  - (* XWhileAbort *)
    destruct (eval_top h r n c t _ g' TBool st D H ltac:(eassumption) Dg E W0 O C) as [S1 [G1 _]].
    split; [exact S1 |]. split; [exact G1 | discriminate].
Qed.

(* ── The theorem ────────────────────────────────────────────────────────── *)

Definition empty_ctx : Ctx := fun _ => None.

(* `typed_core_memory_safe`: a well-typed core program, run from nothing,
   emits a MemorySafe trace — on every run that finishes and on every run a
   failed bounds check (or a negative `make` count) ends. *)
Theorem typed_core_memory_safe : forall s g' t o,
  styped empty_ctx s g' -> exec empty_heap empty_env 0 s t o -> MemorySafe t.
Proof.
  intros s g' t o Ht Hx.
  set (D := nodup Nat.eq_dec (bv s)).
  set (st0 := (fun _ => None) : Store).
  assert (W0 : wfh empty_heap 0) by (intro b; split; [lia | reflexivity]).
  assert (E0 : envok empty_ctx empty_env st0 empty_heap 0) by (intros x ty H; discriminate).
  assert (O0 : sok st0 empty_heap 0) by (intros c w H; discriminate).
  assert (C0 : cnt st0 empty_heap 0 (envrefs D empty_env)) by (intro b; rewrite envrefs_empty; unfold rcof; simpl; lia).
  exact (proj1 (exec_safe empty_heap empty_env 0 s t o Hx empty_ctx g' st0 D Ht
                  ltac:(intros x ty H; discriminate) ltac:(intros a Ha; apply nodup_In; exact Ha)
                  (NoDup_nodup Nat.eq_dec (bv s)) E0 W0 O0 C0)).
Qed.
End Calls.

Print Assumptions typed_core_memory_safe.

(* ── Controls ───────────────────────────────────────────────────────────── *)

(* A program with no functions. *)
Definition no_funs : nat -> option FunDef := fun _ => None.
Definition no_funs2 : nat -> option FunDef2 := fun _ => None.
Lemma no_funs_ok : funs_ok no_funs no_funs2.
Proof. intros f d H; discriminate. Qed.
Lemma no_funs2_ok : funs2_ok no_funs no_funs2.
Proof. intros f d H; discriminate. Qed.

(* The check is what the theorem rests on: the load an unchecked `xs[5]`
   would emit against a one-element list is NOT MemorySafe. *)
Example unchecked_index_not_safe :
  ~ MemorySafe [EAlloc 0%nat (bsize 1) RL; EStore 0%nat 0 (bsize 1) RL; ELoad 0%nat (HDR + 5 * W) W RL].
Proof.
  unfold MemorySafe. simpl. unfold set. simpl.
  intros [_ [_ [[[x [Hx Hb]] _] _]]]. injection Hx as <-. unfold in_bounds, bsize, HDR, W in Hb. simpl in Hb. lia.
Qed.
Print Assumptions unchecked_index_not_safe.

(* The type system is what keeps a list where an int is expected: `5[0]`
   (an int indexed) has no type, and no rule evaluates it. *)
Example int_index_untyped : forall fs fs2 g t, ~ etyped fs fs2 g (EBin Index (EInt 5) (EInt 0)) t.
Proof.
  intros fs fs2 g t H. inversion H; subst.
  match goal with Hop : optype Index _ _ _ |- _ => inversion Hop; subst end.
  match goal with He : etyped _ _ _ (EInt 5) TList |- _ => inversion He end.
Qed.

(* A run that the check aborts: `let xs = make(1, 7); xs[3] = 0` stops at
   the store's check, and its trace is still MemorySafe. *)
Example aborting_run_is_safe :
  let s := SSeq (SLet 0%nat (EBin Make (EInt 1) (EInt 7))) (SSet 0%nat (EInt 3) (EInt 0)) in
  exists t, exec no_funs no_funs2 empty_heap empty_env 0 s t SAbort /\ MemorySafe t.
Proof.
  intro s.
  assert (Ht : styped no_funs no_funs2 empty_ctx s (tupd empty_ctx 0%nat TList)).
  { apply (TySeq no_funs no_funs2 _ _ (tupd empty_ctx 0%nat TList));
      [apply TyLet; [reflexivity | eapply TyBin; [apply OMake | apply TyInt | apply TyInt]] |].
    apply TySet; [reflexivity | apply TyInt | apply TyInt]. }
  eexists. match goal with |- ?A /\ _ => assert (Hx : A) end.
  { eapply XSeq.
    - apply XLet. eapply EvBin; [apply EvInt | apply EvInt | apply BMake; lia].
    - eapply XSetOut; [reflexivity | apply EvInt | apply EvInt |]. simpl. lia. }
  split; [exact Hx | exact (typed_core_memory_safe no_funs no_funs2 no_funs_ok no_funs2_ok s _ _ _ Ht Hx)].
Qed.

(* Sharing, then mutation, then a read of the alias, then a release:
     let xs = make(1, 7); let ys = xs; xs[0] = 1; let z = ys[0]; ys = make(0, 0)
   `let ys = xs` shares xs's block (count 2); `xs[0] = 1` copies it and gives
   one reference back (count 1); `ys[0]` still reads the old block and sees 7
   (value semantics); reassigning ys gives the last reference back and the
   block is freed. The run is MemorySafe, and its trace really frees
   block 0. *)
Definition alias_prog : stmt :=
  SSeq (SLet 0%nat (EBin Make (EInt 1) (EInt 7)))
  (SSeq (SLet 1%nat (EVar 0%nat))
  (SSeq (SSet 0%nat (EInt 0) (EInt 1))
  (SSeq (SLet 2%nat (EBin Index (EVar 1%nat) (EInt 0)))
        (SAssign 1%nat (EBin Make (EInt 0) (EInt 0)))))).

Example shared_list_survives_mutation :
  exists t r n, exec no_funs no_funs2 empty_heap empty_env 0 alias_prog t (Normal r n) /\ MemorySafe t /\
    r 2%nat = Some (VInt 7) /\ rcof (fold_left step t empty_heap) 0%nat = 0.
Proof.
  set (g1 := tupd empty_ctx 0%nat TList).
  set (g2 := tupd g1 1%nat TList).
  assert (Ht : styped no_funs no_funs2 empty_ctx alias_prog (tupd g2 2%nat TInt)).
  { unfold alias_prog.
    apply (TySeq no_funs no_funs2 _ _ g1); [apply TyLet; [reflexivity | eapply TyBin; [apply OMake | apply TyInt | apply TyInt]] |].
    apply (TySeq no_funs no_funs2 _ _ g2); [apply TyLet; [reflexivity | apply TyVar; reflexivity] |].
    apply (TySeq no_funs no_funs2 _ _ g2); [apply TySet; [reflexivity | apply TyInt | apply TyInt] |].
    apply (TySeq no_funs no_funs2 _ _ (tupd g2 2%nat TInt));
      [apply TyLet; [reflexivity | eapply TyBin; [apply OIndex | apply TyVar; reflexivity | apply TyInt]] |].
    eapply TyAssign; [reflexivity | eapply TyBin; [apply OMake | apply TyInt | apply TyInt]]. }
  do 3 eexists. match goal with |- ?A /\ _ => assert (Hx : A) end.
  { unfold alias_prog. eapply XSeq; [apply XLet; eapply EvBin; [apply EvInt | apply EvInt | apply BMake; lia] |].
    eapply XSeq; [apply XLet; apply EvVar; reflexivity |].
    eapply XSeq; [eapply XSet; [reflexivity | apply EvInt | apply EvInt | simpl; lia] |].
    eapply XSeq; [apply XLet; eapply EvBin; [apply EvVar; reflexivity | apply EvInt | apply BIndex; simpl; lia] |].
    apply XAssign. eapply EvBin; [apply EvInt | apply EvInt | apply BMake; lia]. }
  split; [exact Hx |]. split; [exact (typed_core_memory_safe no_funs no_funs2 no_funs_ok no_funs2_ok alias_prog _ _ _ Ht Hx) |].
  split; [reflexivity | vm_compute; reflexivity].
Qed.
Print Assumptions shared_list_survives_mutation.

(* The sharing is what the theorem rests on: the same program with the
   EInc of `let ys = xs` left out frees block 0 at `xs[0] = 1`, and `ys[0]`
   then loads from freed memory — NOT MemorySafe. *)
Example missing_inc_is_use_after_free :
  ~ MemorySafe [EAlloc 0%nat (bsize 1) RL; EStore 0%nat 0 (bsize 1) RL;
                ELoad 0%nat 0 (bsize 1) RL; EAlloc 1%nat (bsize 1) RL; EStore 1%nat 0 (bsize 1) RL;
                EStore 1%nat (HDR + 0 * W) W RL; EDec 0%nat;
                ELoad 0%nat (HDR + 0 * W) W RL; EDec 0%nat].
Proof.
  unfold MemorySafe. intro H.
  change [EAlloc 0%nat (bsize 1) RL; EStore 0%nat 0 (bsize 1) RL;
          ELoad 0%nat 0 (bsize 1) RL; EAlloc 1%nat (bsize 1) RL; EStore 1%nat 0 (bsize 1) RL;
          EStore 1%nat (HDR + 0 * W) W RL; EDec 0%nat;
          ELoad 0%nat (HDR + 0 * W) W RL; EDec 0%nat]
    with ([EAlloc 0%nat (bsize 1) RL; EStore 0%nat 0 (bsize 1) RL;
           ELoad 0%nat 0 (bsize 1) RL; EAlloc 1%nat (bsize 1) RL; EStore 1%nat 0 (bsize 1) RL;
           EStore 1%nat (HDR + 0 * W) W RL; EDec 0%nat] ++
          [ELoad 0%nat (HDR + 0 * W) W RL; EDec 0%nat]) in H.
  apply holds_app in H as [_ H]. destruct H as [[_ [[x [Hx Hc]] _]] _].
  vm_compute in Hx. injection Hx as <-. simpl in Hc. lia.
Qed.
Print Assumptions missing_inc_is_use_after_free.

(* A list through a box, then a release of the box:
     let xs = make(1, 7); let bx = box(xs); xs[0] = 2; let ys = unbox(bx);
     let z = ys[0]; bx = box(make(0, 0))
   The box shares xs's block; `xs[0] = 2` copies, so the box still holds 7;
   `unbox(bx)` shares the box's content; reassigning bx gives the box's last
   reference back, which frees the box (block 1) and gives back the
   reference its slot held — block 0 stays live, ys holds it. *)
Definition box_prog : stmt :=
  SSeq (SLet 0%nat (EBin Make (EInt 1) (EInt 7)))
  (SSeq (SLet 1%nat (EBox (EVar 0%nat)))
  (SSeq (SSet 0%nat (EInt 0) (EInt 2))
  (SSeq (SLet 2%nat (EUnbox (EVar 1%nat)))
  (SSeq (SLet 3%nat (EBin Index (EVar 2%nat) (EInt 0)))
        (SAssign 1%nat (EBox (EBin Make (EInt 0) (EInt 0)))))))).

Example box_frees_and_keeps_content :
  exists t r n, exec no_funs no_funs2 empty_heap empty_env 0 box_prog t (Normal r n) /\ MemorySafe t /\
    r 3%nat = Some (VInt 7) /\
    rcof (fold_left step t empty_heap) 1%nat = 0 /\ rcof (fold_left step t empty_heap) 0%nat = 1.
Proof.
  set (g1 := tupd empty_ctx 0%nat TList).
  set (g2 := tupd g1 1%nat (TBox TList)).
  set (g3 := tupd g2 2%nat TList).
  set (g4 := tupd g3 3%nat TInt).
  assert (Ht : styped no_funs no_funs2 empty_ctx box_prog g4).
  { unfold box_prog.
    apply (TySeq no_funs no_funs2 _ _ g1); [apply TyLet; [reflexivity | eapply TyBin; [apply OMake | apply TyInt | apply TyInt]] |].
    apply (TySeq no_funs no_funs2 _ _ g2); [apply TyLet; [reflexivity | apply TyBox; apply TyVar; reflexivity] |].
    apply (TySeq no_funs no_funs2 _ _ g2); [apply TySet; [reflexivity | apply TyInt | apply TyInt] |].
    apply (TySeq no_funs no_funs2 _ _ g3); [apply TyLet; [reflexivity | apply TyUnbox; apply TyVar; reflexivity] |].
    apply (TySeq no_funs no_funs2 _ _ g4);
      [apply TyLet; [reflexivity | eapply TyBin; [apply OIndex | apply TyVar; reflexivity | apply TyInt]] |].
    eapply TyAssign; [reflexivity | apply TyBox; eapply TyBin; [apply OMake | apply TyInt | apply TyInt]]. }
  do 3 eexists. match goal with |- ?A /\ _ => assert (Hx : A) end.
  { unfold box_prog. eapply XSeq; [apply XLet; eapply EvBin; [apply EvInt | apply EvInt | apply BMake; lia] |].
    eapply XSeq; [apply XLet; apply EvBox; apply EvVar; reflexivity |].
    eapply XSeq; [eapply XSet; [reflexivity | apply EvInt | apply EvInt | simpl; lia] |].
    eapply XSeq; [apply XLet; apply EvUnbox; apply EvVar; reflexivity |].
    eapply XSeq; [apply XLet; eapply EvBin; [apply EvVar; reflexivity | apply EvInt | apply BIndex; simpl; lia] |].
    apply XAssign. apply EvBox. eapply EvBin; [apply EvInt | apply EvInt | apply BMake; lia]. }
  split; [exact Hx |]. split; [exact (typed_core_memory_safe no_funs no_funs2 no_funs_ok no_funs2_ok box_prog _ _ _ Ht Hx) |].
  split; [reflexivity |]. split; vm_compute; reflexivity.
Qed.
Print Assumptions box_frees_and_keeps_content.

(* Storing into a box is a share too — the shape of #2133. Leave out the
   EInc of `box(xs)` and reassigning xs frees block 0 while the box still
   holds it; `unbox(bx)` then takes a reference to freed memory — NOT
   MemorySafe. *)
Example box_store_without_share_is_use_after_free :
  ~ MemorySafe [EAlloc 0%nat (bsize 1) RL; EStore 0%nat 0 (bsize 1) RL;
                EAlloc 1%nat (bsize 1) RB; EStore 1%nat 0 (bsize 1) RB;
                EAlloc 2%nat (bsize 0) RL; EStore 2%nat 0 (bsize 0) RL; EDec 0%nat;
                EInc 1%nat; ELoad 1%nat HDR W RB; EInc 0%nat].
Proof.
  unfold MemorySafe. intro H.
  change [EAlloc 0%nat (bsize 1) RL; EStore 0%nat 0 (bsize 1) RL;
          EAlloc 1%nat (bsize 1) RB; EStore 1%nat 0 (bsize 1) RB;
          EAlloc 2%nat (bsize 0) RL; EStore 2%nat 0 (bsize 0) RL; EDec 0%nat;
          EInc 1%nat; ELoad 1%nat HDR W RB; EInc 0%nat]
    with ([EAlloc 0%nat (bsize 1) RL; EStore 0%nat 0 (bsize 1) RL;
           EAlloc 1%nat (bsize 1) RB; EStore 1%nat 0 (bsize 1) RB;
           EAlloc 2%nat (bsize 0) RL; EStore 2%nat 0 (bsize 0) RL; EDec 0%nat;
           EInc 1%nat; ELoad 1%nat HDR W RB] ++ [EInc 0%nat]) in H.
  apply holds_app in H as [_ H]. destruct H as [[_ [[x [Hx Hc]] _]] _].
  vm_compute in Hx. injection Hx as <-. simpl in Hc. lia.
Qed.
Print Assumptions box_store_without_share_is_use_after_free.

(* Recursion is in: function 0 counts its argument down to zero, calling
   itself, and its table is well typed — so every program over it is
   covered by typed_core_memory_safe. *)
Definition count_down_def : FunDef :=
  {| fparam := 0%nat; fpty := TInt;
     fbody := EIf (EBin Lt (EVar 0%nat) (EInt 1)) (EInt 0)
                  (ECall 0%nat (EBin Add (EVar 0%nat) (EInt (-1))));
     frty := TInt |}.

Definition count_down : nat -> option FunDef := fun f =>
  match f with O => Some count_down_def | _ => None end.

Example count_down_ok : funs_ok count_down no_funs2.
Proof.
  intros f d H. destruct f as [| f]; [| discriminate]. injection H as <-. simpl.
  apply TyEIf.
  - eapply TyBin; [apply OLt | apply TyVar; reflexivity | apply TyInt].
  - apply TyInt.
  - change TInt with (frty count_down_def) at 2.
    apply TyCall; [reflexivity |]. simpl.
    eapply TyBin; [apply OAdd | apply TyVar; reflexivity | apply TyInt].
Qed.

(* Two arguments are in (#4014): function 0 reads `xs[i]`, holding the list
   and the index as its two parameters; its table is well typed, so every
   program over it is covered by typed_core_memory_safe, and the list the
   callee held is given back when it returns. *)
Definition at_def : FunDef2 :=
  {| f2p1 := 0%nat; f2t1 := TList; f2p2 := 1%nat; f2t2 := TInt;
     f2body := EBin Index (EVar 0%nat) (EVar 1%nat); f2rty := TInt |}.

Definition at_funs : nat -> option FunDef2 := fun f =>
  match f with O => Some at_def | _ => None end.

Example at_funs_ok : funs2_ok no_funs at_funs.
Proof.
  intros f d H. destruct f as [| f]; [| discriminate]. injection H as <-. simpl.
  split; [discriminate |].
  eapply TyBin; [apply OIndex | apply TyVar; reflexivity | apply TyVar; reflexivity].
Qed.

(* Control: one name for both parameters is refused. Under one name the
   second argument overwrites the first in the callee's environment, which
   then holds one reference where the caller handed over two — the counting
   the release of both rests on. *)
Example same_param_refused :
  ~ funs2_ok no_funs (fun _ => Some {| f2p1 := 0%nat; f2t1 := TInt; f2p2 := 0%nat; f2t2 := TInt;
                                        f2body := EInt 0; f2rty := TInt |}).
Proof. intro H. destruct (H 0%nat _ eq_refl) as [Ne _]. apply Ne. reflexivity. Qed.
