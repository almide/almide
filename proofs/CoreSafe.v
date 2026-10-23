(* Almide trust spine — #4014: a typed core of the SOURCE language, and the
   theorem that its well-typed programs are MemorySafe.

   Until now the spine reasoned about witnesses the compiler emits; nothing
   said what a source program MEANS. This file fixes a small core of Almide
   — ints, bools, lists of ints with value semantics, `let`, assignment,
   element store, `if`, `while`, indexing with its bounds check — gives it a
   big-step semantics that EMITS the heap trace of MemorySafe.v, a type
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

   HONEST SCOPE. A core, not the language: no closures, records, variants,
   strings, maps, calls or effects; elements are ints; a `let` binds a fresh
   name (a front end renames shadowing away); no block is freed (the RC
   discipline that frees them is the ownership checker's, and its TEMPORAL
   guarantee is MemorySafe.rc_check_gives_temporal); only terminating and
   aborting runs are covered (the semantics is inductive). And nothing here
   connects the core to the compiler: lowering is still checked per build
   by the certificates, as #4014 allows. *)

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

Inductive expr :=
  | EInt (z : Z)
  | EBool (b : bool)
  | EVar (x : Var)
  | EBin (op : binop) (e1 e2 : expr)   (* Make n v = list.repeat(v, n); Index l i = l[i] *)
  | ELen (e : expr).

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

(* One operator on evaluated operands, from fresh-block counter n. *)
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
        [ELoad b (HDR + i * W) W RL] (Val (VInt (nth (Z.to_nat i) vs 0)) n)
  | BIndexOut : forall b vs i n, ~ (0 <= i < Z.of_nat (length vs)) ->
      binop_step Index (VList b vs) (VInt i) n [] Abort.

Inductive eval (r : Env) : nat -> expr -> list Ev -> res -> Prop :=
  | EvInt : forall n z, eval r n (EInt z) [] (Val (VInt z) n)
  | EvBool : forall n b, eval r n (EBool b) [] (Val (VBool b) n)
  | EvVar : forall n x v, r x = Some v -> eval r n (EVar x) [] (Val v n)
  | EvBin : forall n op e1 e2 t1 v1 n1 t2 v2 n2 t3 o,
      eval r n e1 t1 (Val v1 n1) -> eval r n1 e2 t2 (Val v2 n2) ->
      binop_step op v1 v2 n2 t3 o ->
      eval r n (EBin op e1 e2) (t1 ++ t2 ++ t3) o
  | EvBinAbort1 : forall n op e1 e2 t1,
      eval r n e1 t1 Abort -> eval r n (EBin op e1 e2) t1 Abort
  | EvBinAbort2 : forall n op e1 e2 t1 v1 n1 t2,
      eval r n e1 t1 (Val v1 n1) -> eval r n1 e2 t2 Abort ->
      eval r n (EBin op e1 e2) (t1 ++ t2) Abort
  | EvLen : forall n e t b vs n1,
      eval r n e t (Val (VList b vs) n1) ->
      eval r n (ELen e) (t ++ [ELoad b 0 HDR RL]) (Val (VInt (Z.of_nat (length vs))) n1)
  | EvLenAbort : forall n e t, eval r n e t Abort -> eval r n (ELen e) t Abort.

Inductive sres := Normal (r : Env) (n : nat) | SAbort.

Inductive exec : Env -> nat -> stmt -> list Ev -> sres -> Prop :=
  | XSkip : forall r n, exec r n SSkip [] (Normal r n)
  | XLet : forall r n x e t v n1,
      eval r n e t (Val v n1) -> exec r n (SLet x e) t (Normal (upd r x v) n1)
  | XAssign : forall r n x e t v n1,
      eval r n e t (Val v n1) -> exec r n (SAssign x e) t (Normal (upd r x v) n1)
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
                      EStore n2 (HDR + i * W) W RL])
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

Inductive ty := TInt | TBool | TList.

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
  | TyLen : forall e, etyped g e TList -> etyped g (ELen e) TInt.

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

(* ── The invariant ──────────────────────────────────────────────────────── *)

(* Blocks below the counter are allocated and never freed; above it, none. *)
Definition wfh (h : Heap) (n : nat) : Prop :=
  forall b, ((b < n)%nat -> exists x, h b = Some x /\ rc x = 1) /\ ((n <= b)%nat -> h b = None).

(* A value of a type, against the heap: a list is an allocated block of its
   length's size, of the list representation, fully initialized. *)
Definition vty (h : Heap) (n : nat) (v : val) (t : ty) : Prop :=
  match v, t with
  | VInt _, TInt => True
  | VBool _, TBool => True
  | VList b vs, TList =>
      (b < n)%nat /\ exists x, h b = Some x /\ size x = bsize (length vs) /\ repr x = RL /\
                               rc x = 1 /\ all_init (init x) 0 (size x)
  | _, _ => False
  end.

Definition envok (g : Ctx) (r : Env) (h : Heap) (n : nat) : Prop :=
  forall x t, g x = Some t -> exists v, r x = Some v /\ vty h n v t.

(* The heap only grows: a block keeps its size, representation and count,
   and an initialized byte stays initialized. *)
Definition grows (h h' : Heap) : Prop :=
  forall b x, h b = Some x -> exists y, h' b = Some y /\ size y = size x /\ repr y = repr x /\
                                         rc y = rc x /\ (forall i, init x i = true -> init y i = true).

Lemma grows_refl : forall h, grows h h.
Proof. intros h b x H. exists x. auto. Qed.

Lemma grows_trans : forall h1 h2 h3, grows h1 h2 -> grows h2 h3 -> grows h1 h3.
Proof.
  intros h1 h2 h3 A B b x Hx. destruct (A b x Hx) as [y [Hy [S1 [R1 [C1 I1]]]]].
  destruct (B b y Hy) as [z [Hz [S2 [R2 [C2 I2]]]]].
  exists z. repeat split; try congruence. intros i Hi. auto.
Qed.

Lemma vty_mono : forall h h' n n' v t,
  grows h h' -> (n <= n')%nat -> vty h n v t -> vty h' n' v t.
Proof.
  intros h h' n n' v t G Hn H. destruct v, t; simpl in *; try exact I; try contradiction.
  destruct H as [Hb [x [Hx [Hs [Hr [Hc Hi]]]]]]. split; [lia |].
  destruct (G b x Hx) as [y [Hy [S1 [R1 [C1 I1]]]]].
  exists y. repeat split; try congruence.
  intros i Hrange. apply I1. apply Hi. lia.
Qed.

Lemma envok_mono : forall g r h h' n n',
  grows h h' -> (n <= n')%nat -> envok g r h n -> envok g r h' n'.
Proof.
  intros g r h h' n n' G Hn E x t Hx. destruct (E x t Hx) as [v [Hv Ht]].
  exists v. split; [exact Hv | exact (vty_mono h h' n n' v t G Hn Ht)].
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

(* A fresh allocation. *)
Lemma alloc_ok : forall h n sz,
  wfh h n ->
  safe_ev h (EAlloc n sz RL) /\
  grows h (step h (EAlloc n sz RL)) /\ wfh (step h (EAlloc n sz RL)) (S n) /\
  step h (EAlloc n sz RL) n = Some (mkBlock sz RL 1 (fun _ => false)).
Proof.
  intros h n sz W0. pose proof (proj2 (W0 n) (le_n n)) as Hn.
  set (h1 := step h (EAlloc n sz RL)).
  assert (E : forall b, h1 b = if Nat.eqb b n then Some (mkBlock sz RL 1 (fun _ => false)) else h b)
    by reflexivity.
  repeat split.
  - unfold temporal_ev, live. intros [x [Hx _]]. rewrite Hn in Hx. discriminate.
  - intros b x Hx. exists x. rewrite E. destruct (Nat.eqb b n) eqn:B.
    + apply Nat.eqb_eq in B. subst. rewrite Hn in Hx. discriminate.
    + auto.
  - intro Hb. rewrite E. destruct (Nat.eqb b n) eqn:B.
    + eexists. split; reflexivity.
    + apply (proj1 (W0 b)). apply Nat.eqb_neq in B. lia.
  - intro Hb. rewrite E. destruct (Nat.eqb b n) eqn:B; [apply Nat.eqb_eq in B; lia |].
    apply (proj2 (W0 b)). lia.
  - rewrite E, Nat.eqb_refl. reflexivity.
Qed.

(* A store inside a live block of its representation. *)
Lemma store_ok : forall h n b x o w,
  wfh h n -> h b = Some x -> rc x = 1 -> 0 <= o -> 0 < w -> o + w <= size x ->
  safe_ev h (EStore b o w (repr x)) /\
  grows h (step h (EStore b o w (repr x))) /\ wfh (step h (EStore b o w (repr x))) n /\
  step h (EStore b o w (repr x)) b = Some (mkBlock (size x) (repr x) (rc x) (mark (init x) o w)).
Proof.
  intros h n b x o w W0 Hx Hc Ho Hw Hend.
  set (y := mkBlock (size x) (repr x) (rc x) (mark (init x) o w)).
  assert (E : forall b', step h (EStore b o w (repr x)) b' = if Nat.eqb b' b then Some y else h b')
    by (intro b'; simpl; rewrite Hx; reflexivity).
  repeat split.
  - exists x. split; [exact Hx |]. unfold in_bounds. lia.
  - exists x. split; [exact Hx | lia].
  - exists x. auto.
  - intros b' z Hz. rewrite E. destruct (Nat.eqb b' b) eqn:B.
    + apply Nat.eqb_eq in B. subst. rewrite Hx in Hz. injection Hz as <-.
      exists y. repeat split. intros i Hi. unfold y, mark. simpl. rewrite Hi. apply orb_true_r.
    + exists z. auto.
  - intro Hb. rewrite E. destruct (Nat.eqb b0 b) eqn:B.
    + exists y. split; [reflexivity | exact Hc].
    + exact (proj1 (W0 b0) Hb).
  - intro Hb. rewrite E. destruct (Nat.eqb b0 b) eqn:B.
    + apply Nat.eqb_eq in B. subst. exfalso. rewrite (proj2 (W0 b) Hb) in Hx. discriminate.
    + exact (proj2 (W0 b0) Hb).
  - rewrite E, Nat.eqb_refl. reflexivity.
Qed.

(* A load inside a live, initialized block of its representation. *)
Lemma load_ok : forall h b x o w,
  h b = Some x -> rc x = 1 -> 0 <= o -> 0 < w -> o + w <= size x -> all_init (init x) o w ->
  safe_ev h (ELoad b o w (repr x)) /\ step h (ELoad b o w (repr x)) = h.
Proof.
  intros h b x o w Hx Hc Ho Hw Hend Hi. repeat split.
  - exists x. split; [exact Hx |]. unfold in_bounds. lia.
  - exists x. split; [exact Hx | lia].
  - exists x. auto.
  - exists x. auto.
Qed.

Lemma mark_all : forall f o w i, o <= i < o + w -> mark f o w i = true.
Proof.
  intros f o w i H. unfold mark. replace ((o <=? i) && (i <? o + w)) with true; [reflexivity |].
  symmetry. apply andb_true_iff. split; [apply Z.leb_le | apply Z.ltb_lt]; lia.
Qed.

Lemma mark_mono : forall f o w i, f i = true -> mark f o w i = true.
Proof. intros f o w i H. unfold mark. destruct ((o <=? i) && (i <? o + w)); [reflexivity | exact H]. Qed.

(* Allocate and initialize a block of k elements. *)
Lemma fresh_block_ok : forall h n k,
  wfh h n ->
  let t := [EAlloc n (bsize k) RL; EStore n 0 (bsize k) RL] in
  holds safe_ev h t /\ grows h (fold_left step t h) /\ wfh (fold_left step t h) (S n) /\
  exists x, fold_left step t h n = Some x /\ size x = bsize k /\ repr x = RL /\ rc x = 1 /\
            all_init (init x) 0 (size x).
Proof.
  intros h n k W0 t.
  destruct (alloc_ok h n (bsize k) W0) as [S1 [G1 [W1 E1]]].
  set (h1 := step h (EAlloc n (bsize k) RL)) in *.
  set (x := mkBlock (bsize k) RL 1 (fun _ => false)).
  assert (Px := bsize_pos k).
  destruct (store_ok h1 (S n) n x 0 (bsize k) W1 E1 eq_refl ltac:(lia) Px ltac:(simpl; lia))
    as [S2 [G2 [W2 E2]]].
  unfold t. simpl. fold h1.
  split; [split; [exact S1 | split; [exact S2 | exact I]] |].
  split; [exact (grows_trans _ _ _ G1 G2) |].
  split; [exact W2 |].
  eexists. split; [exact E2 |]. simpl. repeat split. intros i Hi. apply mark_all. lia.
Qed.

(* ── Expressions ────────────────────────────────────────────────────────── *)

Lemma length_set_nth : forall vs i k, length (set_nth vs i k) = length vs.
Proof.
  induction vs as [| a vs IH]; intros i k; [reflexivity |]. destruct i; simpl; [reflexivity | f_equal; apply IH].
Qed.

Lemma binop_safe : forall op t1 t2 t v1 v2 n tr o h,
  optype op t1 t2 t -> vty h n v1 t1 -> vty h n v2 t2 -> wfh h n -> binop_step op v1 v2 n tr o ->
  holds safe_ev h tr /\ grows h (fold_left step tr h) /\
  (forall v n', o = Val v n' -> (n <= n')%nat /\ wfh (fold_left step tr h) n' /\
                                vty (fold_left step tr h) n' v t).
Proof.
  intros op t1 t2 t v1 v2 n tr o h Hop H1 H2 W0 Hs.
  destruct Hs; inversion Hop; subst; simpl.
  - (* Add *)
    split; [exact I |]. split; [apply grows_refl |].
    intros v n' Ev. injection Ev as <- <-. split; [lia |]. split; [exact W0 | exact I].
  - (* Lt *)
    split; [exact I |]. split; [apply grows_refl |].
    intros v n' Ev. injection Ev as <- <-. split; [lia |]. split; [exact W0 | exact I].
  - (* Make *)
    destruct (fresh_block_ok h n (Z.to_nat k) W0) as [S [G [W1 [y [Ey [Sy [Ry [Cy Iy]]]]]]]].
    simpl in S, G, W1, Ey. split; [exact S |]. split; [exact G |].
    intros v n' Ev. injection Ev as <- <-. split; [lia |]. split; [exact W1 |].
    simpl. split; [lia |]. exists y. rewrite repeat_length. auto.
  - (* Make, negative count *)
    split; [exact I |]. split; [apply grows_refl | discriminate].
  - (* Index *)
    destruct H1 as [Hb [x [Hx [Sx [Rx [Cx Ix]]]]]].
    assert (Hl : 0 <= HDR + i * W) by (unfold HDR, W; lia).
    rewrite <- Rx.
    destruct (load_ok h b x (HDR + i * W) W Hx Cx Hl ltac:(unfold W; lia)
               ltac:(rewrite Sx; unfold bsize, HDR, W in *; lia)
               ltac:(intros j Hj; apply Ix; rewrite Sx; unfold bsize, HDR, W in *; lia))
      as [S E]. try rewrite E.
    split; [split; [exact S | exact I] |]. split; [apply grows_refl |].
    intros v n' Ev. injection Ev as <- <-. split; [lia |]. split; [exact W0 | exact I].
  - (* Index, out of bounds *)
    split; [exact I |]. split; [apply grows_refl | discriminate].
Qed.

(* No events and a value: the heap is unchanged. *)
Ltac no_events W0 :=
  split; [exact I |]; split; [apply grows_refl |];
  let Ev := fresh "Ev" in intros ? ? Ev; injection Ev as <- <-; split; [lia |]; split; [exact W0 |].

Lemma eval_safe : forall r n e tr o,
  eval r n e tr o ->
  forall g t h, etyped g e t -> envok g r h n -> wfh h n ->
  holds safe_ev h tr /\ grows h (fold_left step tr h) /\
  (forall v n', o = Val v n' -> (n <= n')%nat /\ wfh (fold_left step tr h) n' /\
                                vty (fold_left step tr h) n' v t).
Proof.
  intros r n e tr o Hev. induction Hev; intros g ty h Ht E W0; inversion Ht; subst; simpl.
  - no_events W0. exact I.
  - no_events W0. exact I.
  - destruct (E x ty H1) as [v' [Hv' Hty]]. rewrite H in Hv'. injection Hv' as <-.
    no_events W0. exact Hty.
  - (* EvBin *)
    destruct (IHHev1 g t0 h ltac:(eassumption) E W0) as [S1 [G1 R1]].
    destruct (R1 v1 n1 eq_refl) as [N1 [W1 V1]].
    set (h1 := fold_left step t1 h) in *.
    destruct (IHHev2 g t4 h1 ltac:(eassumption) (envok_mono g r h h1 n n1 G1 N1 E) W1) as [S2 [G2 R2]].
    destruct (R2 v2 n2 eq_refl) as [N2 [W2 V2]].
    set (h2 := fold_left step t2 h1) in *.
    destruct (binop_safe op t0 t4 ty v1 v2 n2 t3 o h2 H3 (vty_mono h1 h2 n1 n2 v1 t0 G2 N2 V1) V2 W2 H)
      as [S3 [G3 R3]].
    rewrite !holds_app, !fold_left_app. fold h1. fold h2.
    split; [split; [exact S1 | split; [exact S2 | exact S3]] |].
    split; [exact (grows_trans _ _ _ G1 (grows_trans _ _ _ G2 G3)) |].
    intros vv nn Ev. destruct (R3 vv nn Ev) as [N3 R]. split; [lia | exact R].
  - (* EvBinAbort1 *)
    destruct (IHHev g _ h ltac:(eassumption) E W0) as [S1 [G1 _]]. split; [exact S1 |]. split; [exact G1 | discriminate].
  - (* EvBinAbort2 *)
    destruct (IHHev1 g _ h ltac:(eassumption) E W0) as [S1 [G1 R1]].
    destruct (R1 v1 n1 eq_refl) as [N1 [W1 V1]].
    destruct (IHHev2 g _ _ ltac:(eassumption) (envok_mono g r h _ n n1 G1 N1 E) W1) as [S2 [G2 _]].
    rewrite holds_app, fold_left_app. split; [split; [exact S1 | exact S2] |].
    split; [exact (grows_trans _ _ _ G1 G2) | discriminate].
  - (* EvLen *)
    destruct (IHHev g TList h ltac:(eassumption) E W0) as [S1 [G1 R1]].
    destruct (R1 (VList b vs) n1 eq_refl) as [N1 [W1 V1]].
    set (h1 := fold_left step t h) in *.
    destruct V1 as [Hb [x [Hx [Sx [Rx [Cx Ix]]]]]].
    rewrite <- Rx.
    destruct (load_ok h1 b x 0 HDR Hx Cx ltac:(lia) ltac:(unfold HDR; lia)
               ltac:(rewrite Sx; unfold bsize, HDR, W; lia)
               ltac:(intros j Hj; apply Ix; rewrite Sx; unfold bsize, HDR, W in *; lia)) as [S E2].
    rewrite holds_app, fold_left_app. fold h1. simpl. try rewrite E2.
    split; [split; [exact S1 | split; [exact S | exact I]] |].
    split; [exact G1 |].
    intros vv nn Ev. injection Ev as <- <-. split; [exact N1 |]. split; [exact W1 | exact I].
  - (* EvLenAbort *)
    destruct (IHHev g TList h ltac:(eassumption) E W0) as [S1 [G1 _]]. split; [exact S1 |]. split; [exact G1 | discriminate].
Qed.

(* ── Statements ─────────────────────────────────────────────────────────── *)

Definition extends (g g' : Ctx) : Prop := forall x t, g x = Some t -> g' x = Some t.

Lemma styped_extends : forall g s g', styped g s g' -> extends g g'.
Proof.
  intros g s g' H. induction H; intros y ty0 Hy; auto.
  unfold tupd. destruct (Nat.eqb y x) eqn:B; [apply Nat.eqb_eq in B; subst; congruence | exact Hy].
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

Lemma exec_safe : forall r n s tr o,
  exec r n s tr o ->
  forall g g' h, styped g s g' -> envok g r h n -> wfh h n ->
  holds safe_ev h tr /\ grows h (fold_left step tr h) /\
  (forall r' n', o = Normal r' n' -> (n <= n')%nat /\ wfh (fold_left step tr h) n' /\
                                     envok g' r' (fold_left step tr h) n').
Proof.
  intros r n s tr o Hx. induction Hx; intros g g' h Ht E W0; inversion Ht; subst.
  - (* XSkip *)
    split; [exact I |]. split; [apply grows_refl |].
    intros r' n' Eq. injection Eq as <- <-. split; [lia |]. split; [exact W0 | exact E].
  - (* XLet *)
    destruct (eval_safe r n e t _ H g _ h ltac:(eassumption) E W0) as [S1 [G1 R1]].
    destruct (R1 v n1 eq_refl) as [N1 [W1 V1]].
    split; [exact S1 |]. split; [exact G1 |].
    intros r' n' Eq. injection Eq as <- <-. split; [exact N1 |]. split; [exact W1 |].
    apply envok_upd; [exact (envok_mono g r h _ n n1 G1 N1 E) | exact V1].
  - (* XAssign *)
    destruct (eval_safe r n e t _ H g' _ h ltac:(eassumption) E W0) as [S1 [G1 R1]].
    destruct (R1 v n1 eq_refl) as [N1 [W1 V1]].
    split; [exact S1 |]. split; [exact G1 |].
    intros r' n' Eq. injection Eq as <- <-. split; [exact N1 |]. split; [exact W1 |].
    eapply envok_assign; [exact (envok_mono g' r h _ n n1 G1 N1 E) | eassumption | exact V1].
  - (* XLetAbort *)
    destruct (eval_safe r n e t _ H g _ h ltac:(eassumption) E W0) as [S1 [G1 _]].
    split; [exact S1 |]. split; [exact G1 | discriminate].
  - (* XAssignAbort *)
    destruct (eval_safe r n e t _ H g' _ h ltac:(eassumption) E W0) as [S1 [G1 _]].
    split; [exact S1 |]. split; [exact G1 | discriminate].
  - (* XSet *)
    rename H into Hrx.
    destruct (eval_safe r n ei t1 _ H0 g' TInt h ltac:(eassumption) E W0) as [S1 [G1 R1]].
    destruct (R1 (VInt i) n1 eq_refl) as [N1 [W1 _]].
    set (h1 := fold_left step t1 h) in *.
    assert (E1 := envok_mono g' r h h1 n n1 G1 N1 E).
    destruct (eval_safe r n1 ev t2 _ H1 g' TInt h1 ltac:(eassumption) E1 W1) as [S2 [G2 R2]].
    destruct (R2 (VInt k) n2 eq_refl) as [N2 [W2 _]].
    set (h2 := fold_left step t2 h1) in *.
    assert (E2 := envok_mono g' r h1 h2 n1 n2 G2 N2 E1).
    destruct (E2 x TList ltac:(eassumption)) as [vx [Hvx Vx]]. rewrite Hrx in Hvx. injection Hvx as <-.
    destruct Vx as [Hb [x0 [Hx0 [Sx [Rx [Cx Ix]]]]]].
    assert (Pz := bsize_pos (length vs)).
    destruct (load_ok h2 b x0 0 (bsize (length vs)) Hx0 Cx ltac:(lia) Pz ltac:(lia)
               ltac:(intros j Hj; apply Ix; lia)) as [SL EL].
    rewrite Rx in SL, EL.
    destruct (fresh_block_ok h2 n2 (length vs) W2) as [SF [GF [WF [y [Ey [Sy [Ry [Cy Iy]]]]]]]].
    cbn [fold_left holds] in SF, GF, WF, Ey.
    set (h3 := step (step h2 (EAlloc n2 (bsize (length vs)) RL)) (EStore n2 0 (bsize (length vs)) RL)) in *.
    assert (Hoff : 0 <= HDR + i * W) by (unfold HDR, W; lia).
    destruct (store_ok h3 (S n2) n2 y (HDR + i * W) W WF Ey Cy Hoff ltac:(unfold W; lia)
               ltac:(rewrite Sy; unfold bsize, HDR, W in *; lia)) as [SS [GS [WS ES]]].
    rewrite Ry in SS, GS, WS, ES.
    rewrite !holds_app, !fold_left_app. fold h1. fold h2. cbn [fold_left holds app]. rewrite EL. fold h3.
    destruct SF as [SA [SB _]].
    split; [split; [exact S1 | split; [exact S2 | split; [exact SL | split; [exact SA | split; [exact SB | split; [exact SS | exact I]]]]]] |].
    split; [exact (grows_trans _ _ _ G1 (grows_trans _ _ _ G2 (grows_trans _ _ _ GF GS))) |].
    intros r' n' Eq. injection Eq as <- <-. split; [lia |]. split; [exact WS |].
    eapply envok_assign; [| eassumption |].
    + exact (envok_mono g' r h2 _ n2 (S n2) (grows_trans _ _ _ GF GS) ltac:(lia) E2).
    + simpl. split; [lia |]. eexists. split; [exact ES |]. simpl.
      rewrite length_set_nth. split; [exact Sy |]. split; [reflexivity |]. split; [exact Cy |].
      intros j Hj. apply mark_mono. apply Iy. exact Hj.
  - (* XSetOut *)
    destruct (eval_safe r n ei t1 _ H0 g' TInt h ltac:(eassumption) E W0) as [S1 [G1 R1]].
    destruct (R1 (VInt i) n1 eq_refl) as [N1 [W1 _]].
    destruct (eval_safe r n1 ev t2 _ H1 g' TInt _ ltac:(eassumption) (envok_mono g' r h _ n n1 G1 N1 E) W1) as [S2 [G2 _]].
    rewrite holds_app, fold_left_app. split; [split; [exact S1 | exact S2] |].
    split; [exact (grows_trans _ _ _ G1 G2) | discriminate].
  - (* XSetAbort1 *)
    destruct (eval_safe r n ei t1 _ H g' TInt h ltac:(eassumption) E W0) as [S1 [G1 _]].
    split; [exact S1 |]. split; [exact G1 | discriminate].
  - (* XSetAbort2 *)
    destruct (eval_safe r n ei t1 _ H g' TInt h ltac:(eassumption) E W0) as [S1 [G1 R1]].
    destruct (R1 v n1 eq_refl) as [N1 [W1 _]].
    destruct (eval_safe r n1 ev t2 _ H0 g' TInt _ ltac:(eassumption) (envok_mono g' r h _ n n1 G1 N1 E) W1) as [S2 [G2 _]].
    rewrite holds_app, fold_left_app. split; [split; [exact S1 | exact S2] |].
    split; [exact (grows_trans _ _ _ G1 G2) | discriminate].
  - (* XSeq *)
    destruct (IHHx1 g g1 h ltac:(eassumption) E W0) as [S1 [G1 R1]].
    destruct (R1 r1 n1 eq_refl) as [N1 [W1 E1]].
    destruct (IHHx2 g1 g' _ ltac:(eassumption) E1 W1) as [S2 [G2 R2]].
    rewrite holds_app, fold_left_app. split; [split; [exact S1 | exact S2] |].
    split; [exact (grows_trans _ _ _ G1 G2) |].
    intros r' n' Eq. destruct (R2 r' n' Eq) as [N2 R]. split; [lia | exact R].
  - (* XSeqAbort *)
    destruct (IHHx g g1 h ltac:(eassumption) E W0) as [S1 [G1 _]].
    split; [exact S1 |]. split; [exact G1 | discriminate].
  - (* XIfT *)
    destruct (eval_safe r n c t _ H g' TBool h ltac:(eassumption) E W0) as [S1 [G1 R1]].
    destruct (R1 (VBool true) n1 eq_refl) as [N1 [W1 _]].
    destruct (IHHx g' g1 _ ltac:(eassumption) (envok_mono g' r h _ n n1 G1 N1 E) W1) as [S2 [G2 R2]].
    rewrite holds_app, fold_left_app. split; [split; [exact S1 | exact S2] |].
    split; [exact (grows_trans _ _ _ G1 G2) |].
    intros r' n' Eq. destruct (R2 r' n' Eq) as [N2 [W2 E2]].
    split; [lia |]. split; [exact W2 |].
    exact (envok_weaken g' g1 r' _ n' (styped_extends g' s1 g1 ltac:(eassumption)) E2).
  - (* XIfF *)
    destruct (eval_safe r n c t _ H g' TBool h ltac:(eassumption) E W0) as [S1 [G1 R1]].
    destruct (R1 (VBool false) n1 eq_refl) as [N1 [W1 _]].
    destruct (IHHx g' g2 _ ltac:(eassumption) (envok_mono g' r h _ n n1 G1 N1 E) W1) as [S2 [G2 R2]].
    rewrite holds_app, fold_left_app. split; [split; [exact S1 | exact S2] |].
    split; [exact (grows_trans _ _ _ G1 G2) |].
    intros r' n' Eq. destruct (R2 r' n' Eq) as [N2 [W2 E2]].
    split; [lia |]. split; [exact W2 |].
    exact (envok_weaken g' g2 r' _ n' (styped_extends g' s2 g2 ltac:(eassumption)) E2).
  - (* XIfAbort *)
    destruct (eval_safe r n c t _ H g' TBool h ltac:(eassumption) E W0) as [S1 [G1 _]].
    split; [exact S1 |]. split; [exact G1 | discriminate].
  - (* XWhileF *)
    destruct (eval_safe r n c t _ H g' TBool h ltac:(eassumption) E W0) as [S1 [G1 R1]].
    destruct (R1 (VBool false) n1 eq_refl) as [N1 [W1 _]].
    split; [exact S1 |]. split; [exact G1 |].
    intros r' n' Eq. injection Eq as <- <-. split; [exact N1 |]. split; [exact W1 |].
    exact (envok_mono g' r h _ n n1 G1 N1 E).
  - (* XWhileT *)
    destruct (eval_safe r n c t _ H g' TBool h ltac:(eassumption) E W0) as [S1 [G1 R1]].
    destruct (R1 (VBool true) n1 eq_refl) as [N1 [W1 _]].
    destruct (IHHx1 g' g1 _ ltac:(eassumption) (envok_mono g' r h _ n n1 G1 N1 E) W1) as [S2 [G2 R2]].
    destruct (R2 r1 n2 eq_refl) as [N2 [W2 E2]].
    destruct (IHHx2 g' g' _ Ht (envok_weaken g' g1 r1 _ n2 (styped_extends g' s g1 ltac:(eassumption)) E2) W2)
      as [S3 [G3 R3]].
    rewrite !holds_app, !fold_left_app. split; [split; [exact S1 | split; [exact S2 | exact S3]] |].
    split; [exact (grows_trans _ _ _ G1 (grows_trans _ _ _ G2 G3)) |].
    intros r' n' Eq. destruct (R3 r' n' Eq) as [N3 R]. split; [lia | exact R].
  - (* XWhileBodyAbort *)
    destruct (eval_safe r n c t _ H g' TBool h ltac:(eassumption) E W0) as [S1 [G1 R1]].
    destruct (R1 (VBool true) n1 eq_refl) as [N1 [W1 _]].
    destruct (IHHx g' g1 _ ltac:(eassumption) (envok_mono g' r h _ n n1 G1 N1 E) W1) as [S2 [G2 _]].
    rewrite holds_app, fold_left_app. split; [split; [exact S1 | exact S2] |].
    split; [exact (grows_trans _ _ _ G1 G2) | discriminate].
  - (* XWhileAbort *)
    destruct (eval_safe r n c t _ H g' TBool h ltac:(eassumption) E W0) as [S1 [G1 _]].
    split; [exact S1 |]. split; [exact G1 | discriminate].
Qed.

(* ── The theorem ────────────────────────────────────────────────────────── *)

Definition empty_env : Env := fun _ => None.
Definition empty_ctx : Ctx := fun _ => None.

(* `typed_core_memory_safe`: a well-typed core program, run from nothing,
   emits a MemorySafe trace — on every run that finishes and on every run a
   failed bounds check (or a negative `make` count) ends. *)
Theorem typed_core_memory_safe : forall s g' t o,
  styped empty_ctx s g' -> exec empty_env 0 s t o -> MemorySafe t.
Proof.
  intros s g' t o Ht Hx.
  assert (W0 : wfh empty_heap 0) by (intro b; split; [lia | reflexivity]).
  assert (E0 : envok empty_ctx empty_env empty_heap 0) by (intros x ty H; discriminate).
  exact (proj1 (exec_safe empty_env 0 s t o Hx empty_ctx g' empty_heap Ht E0 W0)).
Qed.
Print Assumptions typed_core_memory_safe.

(* ── Controls ───────────────────────────────────────────────────────────── *)

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
Example int_index_untyped : forall g t, ~ etyped g (EBin Index (EInt 5) (EInt 0)) t.
Proof. intros g t H. inversion H; subst. inversion H3; subst. inversion H5. Qed.

(* A run that the check aborts: `let xs = make(1, 7); xs[3] = 0` stops at
   the store's check, and its trace is still MemorySafe. *)
Example aborting_run_is_safe :
  let s := SSeq (SLet 0%nat (EBin Make (EInt 1) (EInt 7))) (SSet 0%nat (EInt 3) (EInt 0)) in
  exists t, exec empty_env 0 s t SAbort /\ MemorySafe t.
Proof.
  intro s.
  assert (Ht : styped empty_ctx s (tupd empty_ctx 0%nat TList)).
  { apply (TySeq _ _ (tupd empty_ctx 0%nat TList));
      [apply TyLet; [reflexivity | eapply TyBin; [apply OMake | apply TyInt | apply TyInt]] |].
    apply TySet; [reflexivity | apply TyInt | apply TyInt]. }
  eexists. match goal with |- ?A /\ _ => assert (Hx : A) end.
  { eapply XSeq.
    - apply XLet. eapply EvBin; [apply EvInt | apply EvInt | apply BMake; lia].
    - eapply XSetOut; [reflexivity | apply EvInt | apply EvInt |]. simpl. lia. }
  split; [exact Hx | exact (typed_core_memory_safe s _ _ _ Ht Hx)].
Qed.
