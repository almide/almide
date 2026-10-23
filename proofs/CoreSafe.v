(* Almide trust spine — #4014: a typed core of the SOURCE language, and the
   theorem that its well-typed programs are MemorySafe.

   Until now the spine reasoned about witnesses the compiler emits; nothing
   said what a source program MEANS. This file fixes a small core of Almide
   — ints, bools, lists of ints with value semantics, `let`, assignment,
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
   CowSafety.v).

   Blocks are REFERENCE COUNTED and FREED, the way the compiled code does it.
   Reading a list variable shares its block (EInc) instead of copying it, so
   `let ys = xs` leaves two references to one block. Every reference is
   given back (EDec) when it dies: a list an operator consumed (`xs[i]`,
   `len(xs)`), a variable's old value when it is reassigned or element-
   stored, a call's parameter when the call returns. A block whose count
   reaches zero is freed, and MemorySafe's temporal clause then forbids any
   later touch of it. The proof's invariant is that every block's count is
   at least the number of references the program still holds to it — in its
   variables, in the caller frames, and in values computed but not yet
   consumed — so a held reference always points at a live block, and the
   control `missing_inc_is_use_after_free` shows the invariant is what the
   theorem rests on: drop the EInc of a shared read and a later read of the
   alias touches freed memory.

   HONEST SCOPE. A core, not the language: no closures, records, variants,
   strings, maps or effects; a function takes one argument and its body is
   an expression; elements are ints; a `let` binds a fresh name (a front end
   renames shadowing away). A binding is released when it is overwritten,
   not when its scope ends, and an aborting run releases nothing: both only
   leak, and a leak is not a MemorySafe violation. Only terminating and
   aborting runs are covered (the semantics is inductive). And nothing here
   connects the core to the compiler: lowering is still checked per build by
   the certificates, as #4014 allows. *)

From AlmideTrust Require Import MemorySafe.
From Stdlib Require Import List ZArith Lia Bool Arith.
Import ListNotations.
Open Scope Z_scope.

(* ── Layout ─────────────────────────────────────────────────────────────── *)

Definition HDR : Z := 8.
Definition W : Z := 8.
Definition RL : Repr := 1%nat.
Definition bsize (n : nat) : Z := HDR + Z.of_nat n * W.

(* ── Syntax ─────────────────────────────────────────────────────────────── *)

Definition Var := nat.

Inductive binop := Add | Lt | Make | Index.

Inductive ty := TInt | TBool | TList.

Inductive expr :=
  | EInt (z : Z)
  | EBool (b : bool)
  | EVar (x : Var)
  | EBin (op : binop) (e1 e2 : expr)   (* Make n v = list.repeat(v, n); Index l i = l[i] *)
  | ELen (e : expr)
  | EIf (c e1 e2 : expr)               (* if c then e1 else e2 *)
  | ECall (f : nat) (a : expr).        (* a call of function f on one argument *)

(* A function: one parameter, an expression body, a declared signature.
   Recursion (direct or mutual) is a call to a function in the same table. *)
Record FunDef := { fparam : Var; fpty : ty; fbody : expr; frty : ty }.

Inductive stmt :=
  | SSkip
  | SLet (x : Var) (e : expr)
  | SAssign (x : Var) (e : expr)
  | SSet (x : Var) (i v : expr)        (* x[i] = v *)
  | SSeq (s1 s2 : stmt)
  | SIf (c : expr) (s1 s2 : stmt)
  | SWhile (c : expr) (s : stmt).

(* ── Values and the semantics ───────────────────────────────────────────── *)

Inductive val := VInt (z : Z) | VBool (b : bool) | VList (b : Blk) (vs : list Z).

Definition Env := Var -> option val.
Definition upd (r : Env) (x : Var) (v : val) : Env := fun y => if Nat.eqb y x then Some v else r y.

Inductive res := Val (v : val) (n : nat) | Abort.

Fixpoint set_nth (vs : list Z) (i : nat) (k : Z) : list Z :=
  match vs, i with
  | [], _ => []
  | _ :: r, O => k :: r
  | a :: r, S j => a :: set_nth r j k
  end.

(* Taking one more reference to a value, and giving one back. *)
Definition dup (v : val) : list Ev := match v with VList b _ => [EInc b] | _ => [] end.
Definition drop (o : option val) : list Ev := match o with Some (VList b _) => [EDec b] | _ => [] end.

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

Definition empty_env : Env := fun _ => None.

Inductive eval : Env -> nat -> expr -> list Ev -> res -> Prop :=
  | EvInt : forall r n z, eval r n (EInt z) [] (Val (VInt z) n)
  | EvBool : forall r n b, eval r n (EBool b) [] (Val (VBool b) n)
  (* reading a variable shares its value: one more reference *)
  | EvVar : forall r n x v, r x = Some v -> eval r n (EVar x) (dup v) (Val v n)
  | EvBin : forall r n op e1 e2 t1 v1 n1 t2 v2 n2 t3 o,
      eval r n e1 t1 (Val v1 n1) -> eval r n1 e2 t2 (Val v2 n2) ->
      binop_step op v1 v2 n2 t3 o ->
      eval r n (EBin op e1 e2) (t1 ++ t2 ++ t3) o
  | EvBinAbort1 : forall r n op e1 e2 t1,
      eval r n e1 t1 Abort -> eval r n (EBin op e1 e2) t1 Abort
  | EvBinAbort2 : forall r n op e1 e2 t1 v1 n1 t2,
      eval r n e1 t1 (Val v1 n1) -> eval r n1 e2 t2 Abort ->
      eval r n (EBin op e1 e2) (t1 ++ t2) Abort
  | EvLen : forall r n e t b vs n1,
      eval r n e t (Val (VList b vs) n1) ->
      eval r n (ELen e) (t ++ [ELoad b 0 HDR RL; EDec b]) (Val (VInt (Z.of_nat (length vs))) n1)
  | EvLenAbort : forall r n e t, eval r n e t Abort -> eval r n (ELen e) t Abort
  | EvIfT : forall r n c e1 e2 t n1 t1 o,
      eval r n c t (Val (VBool true) n1) -> eval r n1 e1 t1 o -> eval r n (EIf c e1 e2) (t ++ t1) o
  | EvIfF : forall r n c e1 e2 t n1 t2 o,
      eval r n c t (Val (VBool false) n1) -> eval r n1 e2 t2 o -> eval r n (EIf c e1 e2) (t ++ t2) o
  | EvIfAbort : forall r n c e1 e2 t, eval r n c t Abort -> eval r n (EIf c e1 e2) t Abort
  (* A call: the argument, moved into a fresh environment holding only the
     parameter; the body; then the parameter's reference is given back. *)
  | EvCall : forall r n f d a t1 v n1 t2 v' n2,
      funs f = Some d -> eval r n a t1 (Val v n1) ->
      eval (upd empty_env (fparam d) v) n1 (fbody d) t2 (Val v' n2) ->
      eval r n (ECall f a) (t1 ++ t2 ++ drop (Some v)) (Val v' n2)
  | EvCallBodyAbort : forall r n f d a t1 v n1 t2,
      funs f = Some d -> eval r n a t1 (Val v n1) ->
      eval (upd empty_env (fparam d) v) n1 (fbody d) t2 Abort ->
      eval r n (ECall f a) (t1 ++ t2) Abort
  | EvCallAbort : forall r n f a t1, eval r n a t1 Abort -> eval r n (ECall f a) t1 Abort.

Inductive sres := Normal (r : Env) (n : nat) | SAbort.

Inductive exec : Env -> nat -> stmt -> list Ev -> sres -> Prop :=
  | XSkip : forall r n, exec r n SSkip [] (Normal r n)
  | XLet : forall r n x e t v n1,
      eval r n e t (Val v n1) -> exec r n (SLet x e) t (Normal (upd r x v) n1)
  (* the new value first, then the old one's reference given back: `x = x`
     shares before it releases *)
  | XAssign : forall r n x e t v n1,
      eval r n e t (Val v n1) -> exec r n (SAssign x e) (t ++ drop (r x)) (Normal (upd r x v) n1)
  | XLetAbort : forall r n x e t, eval r n e t Abort -> exec r n (SLet x e) t SAbort
  | XAssignAbort : forall r n x e t, eval r n e t Abort -> exec r n (SAssign x e) t SAbort
  | XSet : forall r n x b vs ei ev t1 i n1 t2 k n2,
      r x = Some (VList b vs) ->
      eval r n ei t1 (Val (VInt i) n1) -> eval r n1 ev t2 (Val (VInt k) n2) ->
      0 <= i < Z.of_nat (length vs) ->
      exec r n (SSet x ei ev)
        (t1 ++ t2 ++ [ELoad b 0 (bsize (length vs)) RL;
                      EAlloc n2 (bsize (length vs)) RL;
                      EStore n2 0 (bsize (length vs)) RL;
                      EStore n2 (HDR + i * W) W RL;
                      EDec b])
        (Normal (upd r x (VList n2 (set_nth vs (Z.to_nat i) k))) (S n2))
  | XSetOut : forall r n x b vs ei ev t1 i n1 t2 k n2,
      r x = Some (VList b vs) ->
      eval r n ei t1 (Val (VInt i) n1) -> eval r n1 ev t2 (Val (VInt k) n2) ->
      ~ (0 <= i < Z.of_nat (length vs)) ->
      exec r n (SSet x ei ev) (t1 ++ t2) SAbort
  | XSetAbort1 : forall r n x ei ev t1,
      eval r n ei t1 Abort -> exec r n (SSet x ei ev) t1 SAbort
  | XSetAbort2 : forall r n x ei ev t1 v n1 t2,
      eval r n ei t1 (Val v n1) -> eval r n1 ev t2 Abort -> exec r n (SSet x ei ev) (t1 ++ t2) SAbort
  | XSeq : forall r n s1 s2 t1 r1 n1 t2 o,
      exec r n s1 t1 (Normal r1 n1) -> exec r1 n1 s2 t2 o -> exec r n (SSeq s1 s2) (t1 ++ t2) o
  | XSeqAbort : forall r n s1 s2 t1, exec r n s1 t1 SAbort -> exec r n (SSeq s1 s2) t1 SAbort
  | XIfT : forall r n c s1 s2 t n1 t1 o,
      eval r n c t (Val (VBool true) n1) -> exec r n1 s1 t1 o -> exec r n (SIf c s1 s2) (t ++ t1) o
  | XIfF : forall r n c s1 s2 t n1 t2 o,
      eval r n c t (Val (VBool false) n1) -> exec r n1 s2 t2 o -> exec r n (SIf c s1 s2) (t ++ t2) o
  | XIfAbort : forall r n c s1 s2 t, eval r n c t Abort -> exec r n (SIf c s1 s2) t SAbort
  | XWhileF : forall r n c s t n1,
      eval r n c t (Val (VBool false) n1) -> exec r n (SWhile c s) t (Normal r n1)
  | XWhileT : forall r n c s t n1 t1 r1 n2 t2 o,
      eval r n c t (Val (VBool true) n1) -> exec r n1 s t1 (Normal r1 n2) ->
      exec r1 n2 (SWhile c s) t2 o -> exec r n (SWhile c s) (t ++ t1 ++ t2) o
  | XWhileBodyAbort : forall r n c s t n1 t1,
      eval r n c t (Val (VBool true) n1) -> exec r n1 s t1 SAbort ->
      exec r n (SWhile c s) (t ++ t1) SAbort
  | XWhileAbort : forall r n c s t, eval r n c t Abort -> exec r n (SWhile c s) t SAbort.

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
  | TyCall : forall f d a, funs f = Some d -> etyped g a (fpty d) -> etyped g (ECall f a) (frty d).

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

(* ── The invariant ──────────────────────────────────────────────────────── *)

(* Blocks below the counter have been allocated (some may since have been
   freed: their record stays, with count 0); above it, none. *)
Definition wfh (h : Heap) (n : nat) : Prop :=
  forall b, ((b < n)%nat -> exists x, h b = Some x) /\ ((n <= b)%nat -> h b = None).

(* A value of a type, against the heap: a list is an allocated block of its
   length's size, of the list representation, fully initialized. That it is
   still LIVE is not part of this; it follows from the counting below. *)
Definition vty (h : Heap) (n : nat) (v : val) (t : ty) : Prop :=
  match v, t with
  | VInt _, TInt => True
  | VBool _, TBool => True
  | VList b vs, TList =>
      (b < n)%nat /\ exists x, h b = Some x /\ size x = bsize (length vs) /\ repr x = RL /\
                               all_init (init x) 0 (size x)
  | _, _ => False
  end.

Definition envok (g : Ctx) (r : Env) (h : Heap) (n : nat) : Prop :=
  forall x t, g x = Some t -> exists v, r x = Some v /\ vty h n v t.

(* The heap only grows: a block keeps its size and representation, and an
   initialized byte stays initialized. Its count is free to move. *)
Definition grows (h h' : Heap) : Prop :=
  forall b x, h b = Some x -> exists y, h' b = Some y /\ size y = size x /\ repr y = repr x /\
                                         (forall i, init x i = true -> init y i = true).

(* The counting. rcof is a block's count (0 if never allocated); vref v b
   is how many references to b the value v is; envrefs D r b how many the
   variables D of r hold; cnt h K says every block's count covers K. *)
Definition rcof (h : Heap) (b : Blk) : Z := match h b with Some x => rc x | None => 0 end.
Definition vref (v : val) (b : Blk) : nat := match v with VList b' _ => if Nat.eqb b' b then 1 else 0 | _ => 0 end.
Definition oref (o : option val) (b : Blk) : nat := match o with Some v => vref v b | None => 0 end.
Fixpoint envrefs (D : list Var) (r : Env) (b : Blk) : nat :=
  match D with [] => 0 | x :: D' => oref (r x) b + envrefs D' r b end.
Definition cnt (h : Heap) (K : Blk -> nat) : Prop := forall b, Z.of_nat (K b) <= rcof h b.

Definition dom_in (g : Ctx) (D : list Var) : Prop := forall x t, g x = Some t -> In x D.

Lemma grows_refl : forall h, grows h h.
Proof. intros h b x H. exists x. auto. Qed.

Lemma grows_trans : forall h1 h2 h3, grows h1 h2 -> grows h2 h3 -> grows h1 h3.
Proof.
  intros h1 h2 h3 A B b x Hx. destruct (A b x Hx) as [y [Hy [S1 [R1 I1]]]].
  destruct (B b y Hy) as [z [Hz [S2 [R2 I2]]]].
  exists z. repeat split; try congruence. intros i Hi. auto.
Qed.

Lemma vty_mono : forall h h' n n' v t,
  grows h h' -> (n <= n')%nat -> vty h n v t -> vty h' n' v t.
Proof.
  intros h h' n n' v t G Hn H. destruct v, t; simpl in *; try exact I; try contradiction.
  destruct H as [Hb [x [Hx [Hs [Hr Hi]]]]]. split; [lia |].
  destruct (G b x Hx) as [y [Hy [S1 [R1 I1]]]].
  exists y. repeat split; try congruence.
  intros i Hrange. apply I1. apply Hi. lia.
Qed.

Lemma envok_mono : forall g r h h' n n',
  grows h h' -> (n <= n')%nat -> envok g r h n -> envok g r h' n'.
Proof.
  intros g r h h' n n' G Hn E x t Hx. destruct (E x t Hx) as [v [Hv Ht]].
  exists v. split; [exact Hv | exact (vty_mono h h' n n' v t G Hn Ht)].
Qed.

Lemma cnt_le : forall h K K', (forall b, (K' b <= K b)%nat) -> cnt h K -> cnt h K'.
Proof. intros h K K' L C b. specialize (C b). specialize (L b). lia. Qed.

(* A reference the program holds is to a live block. *)
Lemma cnt_live : forall h K b, cnt h K -> (1 <= K b)%nat -> exists x, h b = Some x /\ 0 < rc x.
Proof.
  intros h K b C L. specialize (C b). unfold rcof in C.
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
Proof. induction D as [| a D IH]; intro b; cbv beta; [reflexivity | simpl; rewrite IH; reflexivity]. Qed.

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
Lemma alloc_ok : forall h n sz,
  wfh h n ->
  safe_ev h (EAlloc n sz RL) /\
  grows h (step h (EAlloc n sz RL)) /\ wfh (step h (EAlloc n sz RL)) (S n) /\
  step h (EAlloc n sz RL) n = Some (mkBlock sz RL 1 (fun _ => false)) /\
  (forall c, rcof (step h (EAlloc n sz RL)) c = if Nat.eqb c n then 1 else rcof h c).
Proof.
  intros h n sz W0. pose proof (proj2 (W0 n) (le_n n)) as Hn.
  set (h1 := step h (EAlloc n sz RL)).
  assert (E : forall b, h1 b = if Nat.eqb b n then Some (mkBlock sz RL 1 (fun _ => false)) else h b)
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

(* Allocate and initialize a block of k elements: the one reference to it
   is the new value's. *)
Lemma fresh_block_ok : forall h n k,
  wfh h n ->
  let t := [EAlloc n (bsize k) RL; EStore n 0 (bsize k) RL] in
  holds safe_ev h t /\ grows h (fold_left step t h) /\ wfh (fold_left step t h) (S n) /\
  (exists x, fold_left step t h n = Some x /\ size x = bsize k /\ repr x = RL /\ rc x = 1 /\
            all_init (init x) 0 (size x)) /\
  (forall c, rcof (fold_left step t h) c = if Nat.eqb c n then 1 else rcof h c).
Proof.
  intros h n k W0 t.
  destruct (alloc_ok h n (bsize k) W0) as [S1 [G1 [W1 [E1 R1]]]].
  set (h1 := step h (EAlloc n (bsize k) RL)) in *.
  set (x := mkBlock (bsize k) RL 1 (fun _ => false)) in *.
  assert (Px := bsize_pos k).
  destruct (store_ok h1 (S n) n x 0 (bsize k) W1 E1 ltac:(unfold x; simpl; lia) ltac:(lia) Px ltac:(unfold x; simpl; lia))
    as [S2 [G2 [W2 [E2 R2]]]].
  unfold t. simpl. fold h1.
  split; [split; [exact S1 | split; [exact S2 | exact I]] |].
  split; [exact (grows_trans _ _ _ G1 G2) |].
  split; [exact W2 |].
  split.
  - eexists. split; [exact E2 |]. simpl. repeat split. intros i Hi. apply mark_all. lia.
  - intro c. rewrite R2. apply R1.
Qed.

(* Taking a reference to a value the program already holds. *)
Lemma dup_ok : forall h n K v, wfh h n -> cnt h K -> (forall b, (vref v b <= K b)%nat) ->
  holds safe_ev h (dup v) /\ grows h (fold_left step (dup v) h) /\ wfh (fold_left step (dup v) h) n /\
  cnt (fold_left step (dup v) h) (fun b => K b + vref v b)%nat.
Proof.
  intros h n K v W0 C L. destruct v as [z | bb | b vs]; simpl.
  - split; [exact I |]. split; [apply grows_refl |]. split; [exact W0 |].
    intro c. specialize (C c). lia.
  - split; [exact I |]. split; [apply grows_refl |]. split; [exact W0 |].
    intro c. specialize (C c). lia.
  - assert (L1 := L b). simpl in L1. rewrite Nat.eqb_refl in L1.
    destruct (cnt_live h K b C L1) as [x [Hx Hc]].
    destruct (inc_ok h n b x W0 Hx Hc) as [S [G [W1 R]]].
    split; [split; [exact S | exact I] |]. split; [exact G |]. split; [exact W1 |].
    intro c. rewrite R. specialize (C c). rewrite Nat.eqb_sym. destruct (Nat.eqb c b); lia.
Qed.

(* Giving back a reference the program holds. *)
Lemma drop_ok : forall h n K o, wfh h n -> cnt h (fun b => K b + oref o b)%nat ->
  holds safe_ev h (drop o) /\ grows h (fold_left step (drop o) h) /\
  wfh (fold_left step (drop o) h) n /\ cnt (fold_left step (drop o) h) K.
Proof.
  intros h n K o W0 C. destruct o as [[z | bb | b vs] |]; simpl.
  1,2,4: split; [exact I |]; split; [apply grows_refl |]; split; [exact W0 |];
         intro c; specialize (C c); simpl in C; lia.
  destruct (cnt_live h _ b C ltac:(simpl; rewrite Nat.eqb_refl; lia)) as [x [Hx Hc]].
  destruct (dec_ok h n b x W0 Hx Hc) as [S [G [W1 R]]].
  split; [split; [exact S | exact I] |]. split; [exact G |]. split; [exact W1 |].
  intro c. rewrite R. specialize (C c). simpl in C. rewrite Nat.eqb_sym in C. destruct (Nat.eqb c b); lia.
Qed.

(* ── Expressions ────────────────────────────────────────────────────────── *)

Lemma length_set_nth : forall vs i k, length (set_nth vs i k) = length vs.
Proof.
  induction vs as [| a vs IH]; intros i k; [reflexivity |]. destruct i; simpl; [reflexivity | f_equal; apply IH].
Qed.

(* An operator on two held operands: it gives back the ones it consumes,
   and its result is one held reference. *)
Lemma binop_safe : forall op t1 t2 t v1 v2 n tr o h K,
  optype op t1 t2 t -> vty h n v1 t1 -> vty h n v2 t2 -> wfh h n ->
  cnt h (fun b => K b + vref v1 b + vref v2 b)%nat -> binop_step op v1 v2 n tr o ->
  holds safe_ev h tr /\ grows h (fold_left step tr h) /\
  (forall v n', o = Val v n' -> (n <= n')%nat /\ wfh (fold_left step tr h) n' /\
     vty (fold_left step tr h) n' v t /\ cnt (fold_left step tr h) (fun b => K b + vref v b)%nat).
Proof.
  intros op t1 t2 t v1 v2 n tr o h K Hop H1 H2 W0 C Hs.
  destruct Hs; inversion Hop; subst; simpl.
  - (* Add *)
    split; [exact I |]. split; [apply grows_refl |].
    intros v n' Ev. injection Ev as <- <-. split; [lia |]. split; [exact W0 |]. split; [exact I |].
    intro c. specialize (C c). simpl in C |- *. lia.
  - (* Lt *)
    split; [exact I |]. split; [apply grows_refl |].
    intros v n' Ev. injection Ev as <- <-. split; [lia |]. split; [exact W0 |]. split; [exact I |].
    intro c. specialize (C c). simpl in C |- *. lia.
  - (* Make: the fresh block is unreferenced before, held once after *)
    destruct (fresh_block_ok h n (Z.to_nat k) W0) as [S [G [W1 [[y [Ey [Sy [Ry [Cy Iy]]]]] R]]]].
    simpl in S, G, W1, Ey, R. split; [exact S |]. split; [exact G |].
    intros v n' Ev. injection Ev as <- <-. split; [lia |]. split; [exact W1 |].
    split; [simpl; split; [lia |]; exists y; rewrite repeat_length; auto |].
    intro c. rewrite R. specialize (C c). simpl in C |- *.
    destruct (Nat.eqb c n) eqn:B.
    + apply Nat.eqb_eq in B. subst c. unfold rcof in C. rewrite (proj2 (W0 n) (le_n n)) in C.
      rewrite ?Nat.eqb_refl. lia.
    + rewrite (Nat.eqb_sym n c), B. lia.
  - (* Make, negative count *)
    split; [exact I |]. split; [apply grows_refl | discriminate].
  - (* Index: load the element, then give the list back *)
    destruct H1 as [Hb [x [Hx [Sx [Rx Ix]]]]].
    destruct (cnt_live h _ b C ltac:(simpl; rewrite Nat.eqb_refl; lia)) as [x' [Hx' Hc]].
    rewrite Hx in Hx'. injection Hx' as <-.
    assert (Hl : 0 <= HDR + i * W) by (unfold HDR, W; lia).
    rewrite <- Rx.
    destruct (load_ok h b x (HDR + i * W) W Hx Hc Hl ltac:(unfold W; lia)
               ltac:(rewrite Sx; unfold bsize, HDR, W in *; lia)
               ltac:(intros j Hj; apply Ix; rewrite Sx; unfold bsize, HDR, W in *; lia))
      as [SL EL]. try rewrite EL.
    destruct (dec_ok h n b x W0 Hx Hc) as [SD [GD [WD RD]]].
    split; [split; [exact SL | split; [exact SD | exact I]] |]. split; [exact GD |].
    intros v n' Ev. injection Ev as <- <-. split; [lia |]. split; [exact WD |]. split; [exact I |].
    intro c. rewrite RD. specialize (C c). cbn [vref] in C |- *. rewrite Nat.eqb_sym in C.
    destruct (Nat.eqb c b); lia.
  - (* Index, out of bounds *)
    split; [exact I |]. split; [apply grows_refl | discriminate].
Qed.

Lemma eval_safe : forall r n e tr o,
  eval r n e tr o ->
  forall g t h D F, etyped g e t -> dom_in g D -> envok g r h n -> wfh h n ->
  cnt h (fun b => envrefs D r b + F b)%nat ->
  holds safe_ev h tr /\ grows h (fold_left step tr h) /\
  (forall v n', o = Val v n' -> (n <= n')%nat /\ wfh (fold_left step tr h) n' /\
     vty (fold_left step tr h) n' v t /\
     cnt (fold_left step tr h) (fun b => envrefs D r b + F b + vref v b)%nat).
Proof.
  intros r n e tr o Hev. induction Hev; intros g ty h D F Ht Dg E W0 C; inversion Ht; subst; simpl.
  - split; [exact I |]. split; [apply grows_refl |].
    intros v n' Ev. injection Ev as <- <-. split; [lia |]. split; [exact W0 |]. split; [exact I |].
    intro c. specialize (C c). cbn [vref fold_left] in *. lia.
  - split; [exact I |]. split; [apply grows_refl |].
    intros v n' Ev. injection Ev as <- <-. split; [lia |]. split; [exact W0 |]. split; [exact I |].
    intro c. specialize (C c). cbn [vref fold_left] in *. lia.
  - (* EvVar: share the variable's value *)
    destruct (E x ty H1) as [v' [Hv' Hty]]. rewrite H in Hv'. injection Hv' as <-.
    assert (L : forall b, (vref v b <= envrefs D r b + F b)%nat).
    { intro b. pose proof (envrefs_in D r x b (Dg x ty H1)) as I. rewrite H in I. simpl in I. lia. }
    destruct (dup_ok h n _ v W0 C L) as [S [G [W1 C1]]].
    split; [exact S |]. split; [exact G |].
    intros vv nn Ev. injection Ev as <- <-. split; [lia |]. split; [exact W1 |].
    split; [exact (vty_mono h _ n n v ty G (le_n n) Hty) | exact C1].
  - (* EvBin *)
    destruct (IHHev1 g t0 h D F ltac:(eassumption) Dg E W0 C) as [S1 [G1 R1]].
    destruct (R1 v1 n1 eq_refl) as [N1 [W1 [V1 C1]]].
    set (h1 := fold_left step t1 h) in *.
    destruct (IHHev2 g t4 h1 D (fun b => F b + vref v1 b)%nat ltac:(eassumption) Dg
                (envok_mono g r h h1 n n1 G1 N1 E) W1
                ltac:(eapply cnt_le; [| exact C1]; intro b; cbv beta; lia))
      as [S2 [G2 R2]].
    destruct (R2 v2 n2 eq_refl) as [N2 [W2 [V2 C2]]].
    set (h2 := fold_left step t2 h1) in *.
    destruct (binop_safe op t0 t4 ty v1 v2 n2 t3 o h2 (fun b => envrefs D r b + F b)%nat H3
                (vty_mono h1 h2 n1 n2 v1 t0 G2 N2 V1) V2 W2
                ltac:(eapply cnt_le; [| exact C2]; intro b; cbv beta; lia) H)
      as [S3 [G3 R3]].
    rewrite !holds_app, !fold_left_app. fold h1. fold h2.
    split; [split; [exact S1 | split; [exact S2 | exact S3]] |].
    split; [exact (grows_trans _ _ _ G1 (grows_trans _ _ _ G2 G3)) |].
    intros vv nn Ev. destruct (R3 vv nn Ev) as [N3 R]. split; [lia | exact R].
  - (* EvBinAbort1 *)
    destruct (IHHev g _ h D F ltac:(eassumption) Dg E W0 C) as [S1 [G1 _]].
    split; [exact S1 |]. split; [exact G1 | discriminate].
  - (* EvBinAbort2 *)
    destruct (IHHev1 g _ h D F ltac:(eassumption) Dg E W0 C) as [S1 [G1 R1]].
    destruct (R1 v1 n1 eq_refl) as [N1 [W1 [V1 C1]]].
    destruct (IHHev2 g _ _ D (fun b => F b + vref v1 b)%nat ltac:(eassumption) Dg
                (envok_mono g r h _ n n1 G1 N1 E) W1
                ltac:(eapply cnt_le; [| exact C1]; intro b; cbv beta; lia)) as [S2 [G2 _]].
    rewrite holds_app, fold_left_app. split; [split; [exact S1 | exact S2] |].
    split; [exact (grows_trans _ _ _ G1 G2) | discriminate].
  - (* EvLen: load the header, then give the list back *)
    destruct (IHHev g TList h D F ltac:(eassumption) Dg E W0 C) as [S1 [G1 R1]].
    destruct (R1 (VList b vs) n1 eq_refl) as [N1 [W1 [V1 C1]]].
    set (h1 := fold_left step t h) in *.
    destruct V1 as [Hb [x [Hx [Sx [Rx Ix]]]]].
    destruct (cnt_live h1 _ b C1 ltac:(simpl; rewrite Nat.eqb_refl; lia)) as [x' [Hx' Hc]].
    rewrite Hx in Hx'. injection Hx' as <-.
    rewrite <- Rx.
    destruct (load_ok h1 b x 0 HDR Hx Hc ltac:(lia) ltac:(unfold HDR; lia)
               ltac:(rewrite Sx; unfold bsize, HDR, W; lia)
               ltac:(intros j Hj; apply Ix; rewrite Sx; unfold bsize, HDR, W in *; lia)) as [SL EL].
    destruct (dec_ok h1 n1 b x W1 Hx Hc) as [SD [GD [WD RD]]].
    rewrite holds_app, fold_left_app. fold h1. simpl. try rewrite EL.
    split; [split; [exact S1 | split; [exact SL | split; [exact SD | exact I]]] |].
    split; [exact (grows_trans _ _ _ G1 GD) |].
    intros vv nn Ev. injection Ev as <- <-. split; [exact N1 |]. split; [exact WD |]. split; [exact I |].
    intro c. rewrite RD. specialize (C1 c). simpl in C1. rewrite Nat.eqb_sym in C1.
    destruct (Nat.eqb c b); simpl; lia.
  - (* EvLenAbort *)
    destruct (IHHev g TList h D F ltac:(eassumption) Dg E W0 C) as [S1 [G1 _]].
    split; [exact S1 |]. split; [exact G1 | discriminate].
  - (* EvIfT *)
    destruct (IHHev1 g TBool h D F ltac:(eassumption) Dg E W0 C) as [S1 [G1 R1]].
    destruct (R1 (VBool true) n1 eq_refl) as [N1 [W1 [_ C1]]].
    destruct (IHHev2 g ty _ D F ltac:(eassumption) Dg (envok_mono g r h _ n n1 G1 N1 E) W1
                ltac:(eapply cnt_le; [| exact C1]; intro b; cbv beta; simpl; lia)) as [S2 [G2 R2]].
    rewrite holds_app, fold_left_app. split; [split; [exact S1 | exact S2] |].
    split; [exact (grows_trans _ _ _ G1 G2) |].
    intros vv nn Ev. destruct (R2 vv nn Ev) as [N2 R]. split; [lia | exact R].
  - (* EvIfF *)
    destruct (IHHev1 g TBool h D F ltac:(eassumption) Dg E W0 C) as [S1 [G1 R1]].
    destruct (R1 (VBool false) n1 eq_refl) as [N1 [W1 [_ C1]]].
    destruct (IHHev2 g ty _ D F ltac:(eassumption) Dg (envok_mono g r h _ n n1 G1 N1 E) W1
                ltac:(eapply cnt_le; [| exact C1]; intro b; cbv beta; simpl; lia)) as [S2 [G2 R2]].
    rewrite holds_app, fold_left_app. split; [split; [exact S1 | exact S2] |].
    split; [exact (grows_trans _ _ _ G1 G2) |].
    intros vv nn Ev. destruct (R2 vv nn Ev) as [N2 R]. split; [lia | exact R].
  - (* EvIfAbort *)
    destruct (IHHev g TBool h D F ltac:(eassumption) Dg E W0 C) as [S1 [G1 _]].
    split; [exact S1 |]. split; [exact G1 | discriminate].
  - (* EvCall: the callee holds the argument as its parameter; the caller's
       references wait in the frame *)
    match goal with Hd : funs f = Some d, Hd' : funs f = Some ?d0 |- _ => rewrite Hd in Hd'; injection Hd' as <- end.
    destruct (IHHev1 g (fpty d) h D F ltac:(eassumption) Dg E W0 C) as [S1 [G1 R1]].
    destruct (R1 v n1 eq_refl) as [N1 [W1 [V1 C1]]].
    set (h1 := fold_left step t1 h) in *.
    set (g1 := fun y => if Nat.eqb y (fparam d) then Some (fpty d) else None).
    assert (Ec : envok g1 (upd empty_env (fparam d) v) h1 n1).
    { intros y ty0 Hy. unfold g1, upd in *. destruct (Nat.eqb y (fparam d)); [injection Hy as <-; eauto | discriminate]. }
    assert (Dc : dom_in g1 [fparam d]).
    { intros y ty0 Hy. unfold g1 in Hy. destruct (Nat.eqb y (fparam d)) eqn:B; [| discriminate].
      apply Nat.eqb_eq in B. left. symmetry. exact B. }
    assert (Rp : forall b, envrefs [fparam d] (upd empty_env (fparam d) v) b = vref v b).
    { intro b. simpl. unfold upd. rewrite Nat.eqb_refl. simpl. lia. }
    destruct (IHHev2 g1 (frty d) h1 [fparam d] (fun b => envrefs D r b + F b)%nat (Hfuns f d H) Dc Ec W1
                ltac:(eapply cnt_le; [| exact C1]; intro b; cbv beta; rewrite Rp; lia)) as [S2 [G2 R2]].
    destruct (R2 v' n2 eq_refl) as [N2 [W2 [V2 C2]]].
    set (h2 := fold_left step t2 h1) in *.
    destruct (drop_ok h2 n2 (fun b => envrefs D r b + F b + vref v' b)%nat (Some v) W2
                ltac:(eapply cnt_le; [| exact C2]; intro b; cbv beta; rewrite Rp; simpl; lia)) as [S3 [G3 [W3 C3]]].
    rewrite !holds_app, !fold_left_app. fold h1. fold h2.
    split; [split; [exact S1 | split; [exact S2 | exact S3]] |].
    split; [exact (grows_trans _ _ _ G1 (grows_trans _ _ _ G2 G3)) |].
    intros vv nn Ev. injection Ev as <- <-. split; [lia |]. split; [exact W3 |].
    split; [exact (vty_mono h2 _ n2 n2 v' _ G3 (le_n n2) V2) | exact C3].
  - (* EvCallBodyAbort *)
    match goal with Hd : funs f = Some d, Hd' : funs f = Some ?d0 |- _ => rewrite Hd in Hd'; injection Hd' as <- end.
    destruct (IHHev1 g (fpty d) h D F ltac:(eassumption) Dg E W0 C) as [S1 [G1 R1]].
    destruct (R1 v n1 eq_refl) as [N1 [W1 [V1 C1]]].
    set (h1 := fold_left step t1 h) in *.
    set (g1 := fun y => if Nat.eqb y (fparam d) then Some (fpty d) else None).
    assert (Ec : envok g1 (upd empty_env (fparam d) v) h1 n1).
    { intros y ty0 Hy. unfold g1, upd in *. destruct (Nat.eqb y (fparam d)); [injection Hy as <-; eauto | discriminate]. }
    assert (Dc : dom_in g1 [fparam d]).
    { intros y ty0 Hy. unfold g1 in Hy. destruct (Nat.eqb y (fparam d)) eqn:B; [| discriminate].
      apply Nat.eqb_eq in B. left. symmetry. exact B. }
    assert (Rp : forall b, envrefs [fparam d] (upd empty_env (fparam d) v) b = vref v b).
    { intro b. simpl. unfold upd. rewrite Nat.eqb_refl. simpl. lia. }
    destruct (IHHev2 g1 (frty d) h1 [fparam d] (fun b => envrefs D r b + F b)%nat (Hfuns f d H) Dc Ec W1
                ltac:(eapply cnt_le; [| exact C1]; intro b; cbv beta; rewrite Rp; lia)) as [S2 [G2 _]].
    rewrite holds_app, fold_left_app. fold h1. split; [split; [exact S1 | exact S2] |].
    split; [exact (grows_trans _ _ _ G1 G2) | discriminate].
  - (* EvCallAbort *)
    destruct (IHHev g _ h D F ltac:(eassumption) Dg E W0 C) as [S1 [G1 _]].
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

Lemma envok_weaken : forall g g' r h n, extends g g' -> envok g' r h n -> envok g r h n.
Proof. intros g g' r h n X E x t Hx. exact (E x t (X x t Hx)). Qed.

Lemma envok_upd : forall g r h n x v t,
  envok g r h n -> vty h n v t -> envok (tupd g x t) (upd r x v) h n.
Proof.
  intros g r h n x v t E V y ty Hy. unfold tupd, upd in *.
  destruct (Nat.eqb y x); [injection Hy as <-; eauto | exact (E y ty Hy)].
Qed.

Lemma envok_assign : forall g r h n x v t,
  envok g r h n -> g x = Some t -> vty h n v t -> envok g (upd r x v) h n.
Proof.
  intros g r h n x v t E Hx V y ty Hy. unfold upd.
  destruct (Nat.eqb y x) eqn:B; [apply Nat.eqb_eq in B; subst; rewrite Hx in Hy; injection Hy as <-; eauto |].
  exact (E y ty Hy).
Qed.

(* An expression evaluated by a statement: nothing waits in the frame. *)
Lemma eval_top : forall r n e tr o g t h D,
  eval r n e tr o -> etyped g e t -> dom_in g D -> envok g r h n -> wfh h n -> cnt h (envrefs D r) ->
  holds safe_ev h tr /\ grows h (fold_left step tr h) /\
  (forall v n', o = Val v n' -> (n <= n')%nat /\ wfh (fold_left step tr h) n' /\
     vty (fold_left step tr h) n' v t /\
     cnt (fold_left step tr h) (fun b => envrefs D r b + vref v b)%nat).
Proof.
  intros r n e tr o g t h D Hev Ht Dg E W0 C.
  destruct (eval_safe r n e tr o Hev g t h D (fun _ => 0%nat) Ht Dg E W0
              ltac:(eapply cnt_le; [| exact C]; intro b; cbv beta; lia)) as [S [G R]].
  split; [exact S |]. split; [exact G |].
  intros v n' Ev. destruct (R v n' Ev) as [N [W1 [V C1]]]. split; [exact N |]. split; [exact W1 |].
  split; [exact V |]. eapply cnt_le; [| exact C1]. intro b. cbv beta. lia.
Qed.

Lemma exec_safe : forall r n s tr o,
  exec r n s tr o ->
  forall g g' h D, styped g s g' -> dom_in g D -> incl (bv s) D -> NoDup D ->
  envok g r h n -> wfh h n -> cnt h (envrefs D r) ->
  holds safe_ev h tr /\ grows h (fold_left step tr h) /\
  (forall r' n', o = Normal r' n' -> (n <= n')%nat /\ wfh (fold_left step tr h) n' /\
     envok g' r' (fold_left step tr h) n' /\ cnt (fold_left step tr h) (envrefs D r')).
Proof.
  intros r n s tr o Hx. induction Hx; intros g g' h D Ht Dg Ib ND E W0 C; inversion Ht; subst; simpl in Ib.
  - (* XSkip *)
    split; [exact I |]. split; [apply grows_refl |].
    intros r' n' Eq. injection Eq as <- <-. split; [lia |]. split; [exact W0 |]. split; [exact E | exact C].
  - (* XLet: the value moves into the variable *)
    destruct (eval_top r n e t _ g _ h D H ltac:(eassumption) Dg E W0 C) as [S1 [G1 R1]].
    destruct (R1 v n1 eq_refl) as [N1 [W1 [V1 C1]]].
    split; [exact S1 |]. split; [exact G1 |].
    intros r' n' Eq. injection Eq as <- <-. split; [exact N1 |]. split; [exact W1 |].
    split; [apply envok_upd; [exact (envok_mono g r h _ n n1 G1 N1 E) | exact V1] |].
    intro b. pose proof (envrefs_upd D r x v b (Ib x (or_introl eq_refl)) ND) as U.
    specialize (C1 b). simpl in C1. lia.
  - (* XAssign: the new value moves in, the old one's reference is given back *)
    assert (Hin : In x D) by (match goal with Hg : g' x = Some _ |- _ => exact (Dg x _ Hg) end).
    destruct (eval_top r n e t _ g' _ h D H ltac:(eassumption) Dg E W0 C) as [S1 [G1 R1]].
    destruct (R1 v n1 eq_refl) as [N1 [W1 [V1 C1]]].
    set (h1 := fold_left step t h) in *.
    destruct (drop_ok h1 n1 (envrefs D (upd r x v)) (r x) W1
                ltac:(intro c0; cbv beta; pose proof (envrefs_upd D r x v c0 Hin ND) as U; specialize (C1 c0); cbv beta in C1; lia))
      as [S2 [G2 [W2 C2]]].
    rewrite holds_app, fold_left_app. fold h1.
    split; [split; [exact S1 | exact S2] |]. split; [exact (grows_trans _ _ _ G1 G2) |].
    intros r' n' Eq. injection Eq as <- <-. split; [exact N1 |]. split; [exact W2 |].
    split; [| exact C2].
    eapply envok_assign; [| eassumption |].
    + exact (envok_mono g' r h _ n n1 (grows_trans _ _ _ G1 G2) N1 E).
    + exact (vty_mono h1 _ n1 n1 v _ G2 (le_n n1) V1).
  - (* XLetAbort *)
    destruct (eval_top r n e t _ g _ h D H ltac:(eassumption) Dg E W0 C) as [S1 [G1 _]].
    split; [exact S1 |]. split; [exact G1 | discriminate].
  - (* XAssignAbort *)
    destruct (eval_top r n e t _ g' _ h D H ltac:(eassumption) Dg E W0 C) as [S1 [G1 _]].
    split; [exact S1 |]. split; [exact G1 | discriminate].
  - (* XSet: copy into a fresh block, store, give the old block back *)
    rename H into Hrx.
    assert (Hin : In x D) by (match goal with Hg : g' x = Some _ |- _ => exact (Dg x _ Hg) end).
    destruct (eval_top r n ei t1 _ g' TInt h D H0 ltac:(eassumption) Dg E W0 C) as [S1 [G1 R1]].
    destruct (R1 (VInt i) n1 eq_refl) as [N1 [W1 [_ C1]]].
    set (h1 := fold_left step t1 h) in *.
    assert (E1 := envok_mono g' r h h1 n n1 G1 N1 E).
    destruct (eval_top r n1 ev t2 _ g' TInt h1 D H1 ltac:(eassumption) Dg E1 W1
                ltac:(eapply cnt_le; [| exact C1]; intro c0; cbv beta; simpl; lia)) as [S2 [G2 R2]].
    destruct (R2 (VInt k) n2 eq_refl) as [N2 [W2 [_ C2]]].
    set (h2 := fold_left step t2 h1) in *.
    assert (E2 := envok_mono g' r h1 h2 n1 n2 G2 N2 E1).
    assert (C2' : cnt h2 (envrefs D r)) by (eapply cnt_le; [| exact C2]; intro c0; cbv beta; simpl; lia).
    destruct (E2 x TList ltac:(eassumption)) as [vx [Hvx Vx]]. rewrite Hrx in Hvx. injection Hvx as <-.
    destruct Vx as [Hb [x0 [Hx0 [Sx [Rx Ix]]]]].
    destruct (cnt_live h2 _ b C2' ltac:(pose proof (envrefs_in D r x b Hin) as L; rewrite Hrx in L;
                                         simpl in L; rewrite Nat.eqb_refl in L; lia)) as [x0' [Hx0' Hc]].
    rewrite Hx0 in Hx0'. injection Hx0' as <-.
    assert (Pz := bsize_pos (length vs)).
    destruct (load_ok h2 b x0 0 (bsize (length vs)) Hx0 Hc ltac:(lia) Pz ltac:(lia)
               ltac:(intros j Hj; apply Ix; lia)) as [SL EL].
    rewrite Rx in SL, EL.
    destruct (fresh_block_ok h2 n2 (length vs) W2) as [SF [GF [WF [[y [Ey [Sy [Ry [Cy Iy]]]]] RF]]]].
    cbn [fold_left holds] in SF, GF, WF, Ey, RF.
    set (h3 := step (step h2 (EAlloc n2 (bsize (length vs)) RL)) (EStore n2 0 (bsize (length vs)) RL)) in *.
    assert (Hoff : 0 <= HDR + i * W) by (unfold HDR, W; lia).
    destruct (store_ok h3 (S n2) n2 y (HDR + i * W) W WF Ey ltac:(lia) Hoff ltac:(unfold W; lia)
               ltac:(rewrite Sy; unfold bsize, HDR, W in *; lia)) as [SS [GS [WS [ES RS]]]].
    rewrite Ry in SS, GS, WS, ES, RS.
    set (h4 := step h3 (EStore n2 (HDR + i * W) W RL)) in *.
    set (nv := VList n2 (set_nth vs (Z.to_nat i) k)).
    assert (Hn2 : h2 n2 = None) by exact (proj2 (W2 n2) (le_n n2)).
    destruct (drop_ok h4 (S n2) (envrefs D (upd r x nv)) (Some (VList b vs)) WS) as [SD [GD [WD CD]]].
    { intro c. pose proof (envrefs_upd D r x nv c Hin ND) as U. rewrite Hrx in U.
      rewrite RS, RF. specialize (C2' c). simpl in U |- *.
      destruct (Nat.eqb c n2) eqn:B.
      - apply Nat.eqb_eq in B. subst c. unfold rcof in C2'. rewrite Hn2 in C2'.
        rewrite Nat.eqb_refl in U. lia.
      - rewrite (Nat.eqb_sym n2 c), B in U. lia. }
    simpl in SD, GD, WD, CD.
    rewrite !holds_app, !fold_left_app. fold h1. fold h2. cbn [fold_left holds app]. rewrite EL. fold h3. fold h4.
    destruct SF as [SA [SB _]].
    split; [split; [exact S1 | split; [exact S2 | split; [exact SL | split; [exact SA | split; [exact SB | split; [exact SS | exact SD]]]]]] |].
    split; [exact (grows_trans _ _ _ G1 (grows_trans _ _ _ G2 (grows_trans _ _ _ GF (grows_trans _ _ _ GS GD)))) |].
    intros r' n' Eq. injection Eq as <- <-. split; [lia |]. split; [exact WD |].
    split; [| exact CD].
    eapply envok_assign; [| eassumption |].
    + exact (envok_mono g' r h2 _ n2 (S n2) (grows_trans _ _ _ GF (grows_trans _ _ _ GS GD)) ltac:(lia) E2).
    + apply (vty_mono h4 _ (S n2) (S n2) nv TList GD (le_n _)).
      simpl. split; [lia |]. eexists. split; [exact ES |]. simpl.
      rewrite length_set_nth. split; [exact Sy |]. split; [reflexivity |].
      intros j Hj. apply mark_mono. apply Iy. exact Hj.
  - (* XSetOut *)
    destruct (eval_top r n ei t1 _ g' TInt h D H0 ltac:(eassumption) Dg E W0 C) as [S1 [G1 R1]].
    destruct (R1 (VInt i) n1 eq_refl) as [N1 [W1 [_ C1]]].
    destruct (eval_top r n1 ev t2 _ g' TInt _ D H1 ltac:(eassumption) Dg (envok_mono g' r h _ n n1 G1 N1 E) W1
                ltac:(eapply cnt_le; [| exact C1]; intro c0; cbv beta; simpl; lia)) as [S2 [G2 _]].
    rewrite holds_app, fold_left_app. split; [split; [exact S1 | exact S2] |].
    split; [exact (grows_trans _ _ _ G1 G2) | discriminate].
  - (* XSetAbort1 *)
    destruct (eval_top r n ei t1 _ g' TInt h D H ltac:(eassumption) Dg E W0 C) as [S1 [G1 _]].
    split; [exact S1 |]. split; [exact G1 | discriminate].
  - (* XSetAbort2 *)
    destruct (eval_top r n ei t1 _ g' TInt h D H ltac:(eassumption) Dg E W0 C) as [S1 [G1 R1]].
    destruct (R1 v n1 eq_refl) as [N1 [W1 [_ C1]]].
    destruct (eval_top r n1 ev t2 _ g' TInt _ D H0 ltac:(eassumption) Dg (envok_mono g' r h _ n n1 G1 N1 E) W1
                ltac:(eapply cnt_le; [| exact C1]; intro c0; cbv beta; simpl; lia)) as [S2 [G2 _]].
    rewrite holds_app, fold_left_app. split; [split; [exact S1 | exact S2] |].
    split; [exact (grows_trans _ _ _ G1 G2) | discriminate].
  - (* XSeq *)
    apply incl_app_inv in Ib as [I1 I2].
    destruct (IHHx1 g g1 h D ltac:(eassumption) Dg I1 ND E W0 C) as [S1 [G1 R1]].
    destruct (R1 r1 n1 eq_refl) as [N1 [W1 [E1 C1]]].
    destruct (IHHx2 g1 g' _ D ltac:(eassumption) (styped_dom g s1 g1 D ltac:(eassumption) Dg I1) I2 ND E1 W1 C1)
      as [S2 [G2 R2]].
    rewrite holds_app, fold_left_app. split; [split; [exact S1 | exact S2] |].
    split; [exact (grows_trans _ _ _ G1 G2) |].
    intros r' n' Eq. destruct (R2 r' n' Eq) as [N2 R]. split; [lia | exact R].
  - (* XSeqAbort *)
    apply incl_app_inv in Ib as [I1 I2].
    destruct (IHHx g g1 h D ltac:(eassumption) Dg I1 ND E W0 C) as [S1 [G1 _]].
    split; [exact S1 |]. split; [exact G1 | discriminate].
  - (* XIfT *)
    apply incl_app_inv in Ib as [I1 I2].
    destruct (eval_top r n c t _ g' TBool h D H ltac:(eassumption) Dg E W0 C) as [S1 [G1 R1]].
    destruct (R1 (VBool true) n1 eq_refl) as [N1 [W1 [_ C1]]].
    destruct (IHHx g' g1 _ D ltac:(eassumption) Dg I1 ND (envok_mono g' r h _ n n1 G1 N1 E) W1
                ltac:(eapply cnt_le; [| exact C1]; intro c0; cbv beta; simpl; lia)) as [S2 [G2 R2]].
    rewrite holds_app, fold_left_app. split; [split; [exact S1 | exact S2] |].
    split; [exact (grows_trans _ _ _ G1 G2) |].
    intros r' n' Eq. destruct (R2 r' n' Eq) as [N2 [W2 [E2 C2]]].
    split; [lia |]. split; [exact W2 |]. split; [| exact C2].
    exact (envok_weaken g' g1 r' _ n' (styped_extends g' s1 g1 ltac:(eassumption)) E2).
  - (* XIfF *)
    apply incl_app_inv in Ib as [I1 I2].
    destruct (eval_top r n c t _ g' TBool h D H ltac:(eassumption) Dg E W0 C) as [S1 [G1 R1]].
    destruct (R1 (VBool false) n1 eq_refl) as [N1 [W1 [_ C1]]].
    destruct (IHHx g' g2 _ D ltac:(eassumption) Dg I2 ND (envok_mono g' r h _ n n1 G1 N1 E) W1
                ltac:(eapply cnt_le; [| exact C1]; intro c0; cbv beta; simpl; lia)) as [S2 [G2 R2]].
    rewrite holds_app, fold_left_app. split; [split; [exact S1 | exact S2] |].
    split; [exact (grows_trans _ _ _ G1 G2) |].
    intros r' n' Eq. destruct (R2 r' n' Eq) as [N2 [W2 [E2 C2]]].
    split; [lia |]. split; [exact W2 |]. split; [| exact C2].
    exact (envok_weaken g' g2 r' _ n' (styped_extends g' s2 g2 ltac:(eassumption)) E2).
  - (* XIfAbort *)
    destruct (eval_top r n c t _ g' TBool h D H ltac:(eassumption) Dg E W0 C) as [S1 [G1 _]].
    split; [exact S1 |]. split; [exact G1 | discriminate].
  - (* XWhileF *)
    destruct (eval_top r n c t _ g' TBool h D H ltac:(eassumption) Dg E W0 C) as [S1 [G1 R1]].
    destruct (R1 (VBool false) n1 eq_refl) as [N1 [W1 [_ C1]]].
    split; [exact S1 |]. split; [exact G1 |].
    intros r' n' Eq. injection Eq as <- <-. split; [exact N1 |]. split; [exact W1 |].
    split; [exact (envok_mono g' r h _ n n1 G1 N1 E) |].
    eapply cnt_le; [| exact C1]. intro c0. cbv beta. simpl. lia.
  - (* XWhileT *)
    destruct (eval_top r n c t _ g' TBool h D H ltac:(eassumption) Dg E W0 C) as [S1 [G1 R1]].
    destruct (R1 (VBool true) n1 eq_refl) as [N1 [W1 [_ C1]]].
    destruct (IHHx1 g' g1 _ D ltac:(eassumption) Dg Ib ND (envok_mono g' r h _ n n1 G1 N1 E) W1
                ltac:(eapply cnt_le; [| exact C1]; intro c0; cbv beta; simpl; lia)) as [S2 [G2 R2]].
    destruct (R2 r1 n2 eq_refl) as [N2 [W2 [E2 C2]]].
    destruct (IHHx2 g' g' _ D Ht Dg Ib ND
                (envok_weaken g' g1 r1 _ n2 (styped_extends g' s g1 ltac:(eassumption)) E2) W2 C2)
      as [S3 [G3 R3]].
    rewrite !holds_app, !fold_left_app. split; [split; [exact S1 | split; [exact S2 | exact S3]] |].
    split; [exact (grows_trans _ _ _ G1 (grows_trans _ _ _ G2 G3)) |].
    intros r' n' Eq. destruct (R3 r' n' Eq) as [N3 R]. split; [lia | exact R].
  - (* XWhileBodyAbort *)
    destruct (eval_top r n c t _ g' TBool h D H ltac:(eassumption) Dg E W0 C) as [S1 [G1 R1]].
    destruct (R1 (VBool true) n1 eq_refl) as [N1 [W1 [_ C1]]].
    destruct (IHHx g' g1 _ D ltac:(eassumption) Dg Ib ND (envok_mono g' r h _ n n1 G1 N1 E) W1
                ltac:(eapply cnt_le; [| exact C1]; intro c0; cbv beta; simpl; lia)) as [S2 [G2 _]].
    rewrite holds_app, fold_left_app. split; [split; [exact S1 | exact S2] |].
    split; [exact (grows_trans _ _ _ G1 G2) | discriminate].
  - (* XWhileAbort *)
    destruct (eval_top r n c t _ g' TBool h D H ltac:(eassumption) Dg E W0 C) as [S1 [G1 _]].
    split; [exact S1 |]. split; [exact G1 | discriminate].
Qed.

(* ── The theorem ────────────────────────────────────────────────────────── *)

Definition empty_ctx : Ctx := fun _ => None.

(* `typed_core_memory_safe`: a well-typed core program, run from nothing,
   emits a MemorySafe trace — on every run that finishes and on every run a
   failed bounds check (or a negative `make` count) ends. *)
Theorem typed_core_memory_safe : forall s g' t o,
  styped empty_ctx s g' -> exec empty_env 0 s t o -> MemorySafe t.
Proof.
  intros s g' t o Ht Hx.
  set (D := nodup Nat.eq_dec (bv s)).
  assert (W0 : wfh empty_heap 0) by (intro b; cbv beta; split; [lia | reflexivity]).
  assert (E0 : envok empty_ctx empty_env empty_heap 0) by (intros x ty H; discriminate).
  assert (C0 : cnt empty_heap (envrefs D empty_env)) by (intro b; cbv beta; rewrite envrefs_empty; unfold rcof; simpl; lia).
  exact (proj1 (exec_safe empty_env 0 s t o Hx empty_ctx g' empty_heap D Ht
                  ltac:(intros x ty H; discriminate) ltac:(intros a Ha; apply nodup_In; exact Ha)
                  (NoDup_nodup Nat.eq_dec (bv s)) E0 W0 C0)).
Qed.
End Calls.

Print Assumptions typed_core_memory_safe.

(* ── Controls ───────────────────────────────────────────────────────────── *)

(* A program with no functions. *)
Definition no_funs : nat -> option FunDef := fun _ => None.
Lemma no_funs_ok : funs_ok no_funs.
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
Example int_index_untyped : forall fs g t, ~ etyped fs g (EBin Index (EInt 5) (EInt 0)) t.
Proof.
  intros fs g t H. inversion H as [| | | op e1 e2 t1 t2 t' Hop He1 He2 | | | ]; subst.
  inversion Hop; subst. inversion He1.
Qed.

(* A run that the check aborts: `let xs = make(1, 7); xs[3] = 0` stops at
   the store's check, and its trace is still MemorySafe. *)
Example aborting_run_is_safe :
  let s := SSeq (SLet 0%nat (EBin Make (EInt 1) (EInt 7))) (SSet 0%nat (EInt 3) (EInt 0)) in
  exists t, exec no_funs empty_env 0 s t SAbort /\ MemorySafe t.
Proof.
  intro s.
  assert (Ht : styped no_funs empty_ctx s (tupd empty_ctx 0%nat TList)).
  { apply (TySeq no_funs _ _ (tupd empty_ctx 0%nat TList));
      [apply TyLet; [reflexivity | eapply TyBin; [apply OMake | apply TyInt | apply TyInt]] |].
    apply TySet; [reflexivity | apply TyInt | apply TyInt]. }
  eexists. match goal with |- ?A /\ _ => assert (Hx : A) end.
  { eapply XSeq.
    - apply XLet. eapply EvBin; [apply EvInt | apply EvInt | apply BMake; lia].
    - eapply XSetOut; [reflexivity | apply EvInt | apply EvInt |]. simpl. lia. }
  split; [exact Hx | exact (typed_core_memory_safe no_funs no_funs_ok s _ _ _ Ht Hx)].
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
  exists t r n, exec no_funs empty_env 0 alias_prog t (Normal r n) /\ MemorySafe t /\
    r 2%nat = Some (VInt 7) /\ rcof (fold_left step t empty_heap) 0%nat = 0.
Proof.
  set (g1 := tupd empty_ctx 0%nat TList).
  set (g2 := tupd g1 1%nat TList).
  assert (Ht : styped no_funs empty_ctx alias_prog (tupd g2 2%nat TInt)).
  { unfold alias_prog.
    apply (TySeq no_funs _ _ g1); [apply TyLet; [reflexivity | eapply TyBin; [apply OMake | apply TyInt | apply TyInt]] |].
    apply (TySeq no_funs _ _ g2); [apply TyLet; [reflexivity | apply TyVar; reflexivity] |].
    apply (TySeq no_funs _ _ g2); [apply TySet; [reflexivity | apply TyInt | apply TyInt] |].
    apply (TySeq no_funs _ _ (tupd g2 2%nat TInt));
      [apply TyLet; [reflexivity | eapply TyBin; [apply OIndex | apply TyVar; reflexivity | apply TyInt]] |].
    eapply TyAssign; [reflexivity | eapply TyBin; [apply OMake | apply TyInt | apply TyInt]]. }
  do 3 eexists. match goal with |- ?A /\ _ => assert (Hx : A) end.
  { unfold alias_prog. eapply XSeq; [apply XLet; eapply EvBin; [apply EvInt | apply EvInt | apply BMake; lia] |].
    eapply XSeq; [apply XLet; apply EvVar; reflexivity |].
    eapply XSeq; [eapply XSet; [reflexivity | apply EvInt | apply EvInt | simpl; lia] |].
    eapply XSeq; [apply XLet; eapply EvBin; [apply EvVar; reflexivity | apply EvInt | apply BIndex; simpl; lia] |].
    apply XAssign. eapply EvBin; [apply EvInt | apply EvInt | apply BMake; lia]. }
  split; [exact Hx |]. split; [exact (typed_core_memory_safe no_funs no_funs_ok alias_prog _ _ _ Ht Hx) |].
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

Example count_down_ok : funs_ok count_down.
Proof.
  intros f d H. destruct f as [| f]; [| discriminate]. injection H as <-. simpl.
  apply TyEIf.
  - eapply TyBin; [apply OLt | apply TyVar; reflexivity | apply TyInt].
  - apply TyInt.
  - change TInt with (frty count_down_def) at 2.
    apply TyCall; [reflexivity |]. simpl.
    eapply TyBin; [apply OAdd | apply TyVar; reflexivity | apply TyInt].
Qed.

