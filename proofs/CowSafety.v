(* Almide v1 trust spine — A1.3 proof foundation: COPY-ON-WRITE (MakeUnique) is
   ALIAS-SAFE.

   When `Dup` SHARES (rc_inc) instead of copying (A1.3, a memory-efficiency slice),
   an in-place mutation of a SHARED block would corrupt the aliasing owner. The
   renderer prevents this with `MakeUnique` (clone-on-shared) before every in-place
   mutation. This file models that discipline and proves its safety core: the block
   `MakeUnique` returns is UNIQUELY owned (rc = 1), so mutating it in place affects
   no other handle — the aliased-mutation class cannot occur.

   (Until A1.3 the renderer eager-copies, so every block is already unique and
   `MakeUnique` is a no-op; this proof is the WALL the sharing renderer will REFINE,
   exactly as FreeList is the wall the free-list renderer refines.) *)

From Stdlib Require Import ZArith.
From Stdlib Require Import Lia.
Open Scope Z_scope.

Definition RcEnv := Z -> Z.
Record MuState := { fresh : Z; rcenv : RcEnv }.

(* Every owned block lies below the fresh frontier — so a fresh address is a
   genuinely NEW block, distinct from any existing one (the model cannot collide). *)
Definition below_fresh (st : MuState) : Prop :=
  forall a, 0 < rcenv st a -> a < fresh st.

(* MakeUnique on the handle pointing to block p:
   - rc p = 1: already unique — keep p, state unchanged;
   - rc p > 1: CLONE to the fresh frontier (rc 1) and drop our ref to p (rc p - 1).
   Returns (result block, new state). *)
Definition make_unique (st : MuState) (p : Z) : Z * MuState :=
  if Z.eqb (rcenv st p) 1 then (p, st)
  else (fresh st,
        {| fresh := fresh st + 1;
           rcenv := fun a => if Z.eqb a (fresh st) then 1
                             else if Z.eqb a p then rcenv st p - 1
                             else rcenv st a |}).

(* In the clone case the fresh address is DISTINCT from the original — the model
   does not collide, so a "rc = 1" result genuinely means "one owner" (the
   soundness crux, from below_fresh). *)
Lemma make_unique_clone_fresh_distinct :
  forall st p, below_fresh st -> 0 < rcenv st p -> rcenv st p <> 1 -> fresh st <> p.
Proof.
  intros st p Hbf Hp _ Heq. specialize (Hbf p Hp). lia.
Qed.

(* SAFETY CORE: the block MakeUnique returns is UNIQUELY owned (rc = 1). So an
   in-place mutation of it corrupts no aliasing owner — the cow discipline makes
   aliased mutation impossible. *)
Theorem make_unique_yields_unique :
  forall st p, 0 < rcenv st p ->
    rcenv (snd (make_unique st p)) (fst (make_unique st p)) = 1.
Proof.
  intros st p Hp. unfold make_unique.
  destruct (Z.eqb (rcenv st p) 1) eqn:E.
  - apply Z.eqb_eq in E. simpl. exact E.
  - simpl. rewrite Z.eqb_refl. reflexivity.
Qed.

(* INV threads: MakeUnique preserves below_fresh, so the safety holds across a
   whole run of shares / make-uniques. *)
Theorem make_unique_preserves_below_fresh :
  forall st p, below_fresh st -> below_fresh (snd (make_unique st p)).
Proof.
  intros st p Hbf. unfold make_unique.
  destruct (Z.eqb (rcenv st p) 1) eqn:E.
  - simpl. exact Hbf.
  - unfold below_fresh; simpl. intros a Ha.
    destruct (Z.eqb a (fresh st)) eqn:Ea.
    + apply Z.eqb_eq in Ea. subst a. lia.
    + destruct (Z.eqb a p) eqn:Ep.
      * apply Z.eqb_eq in Ep. subst a.
        assert (Hpp : 0 < rcenv st p) by lia. specialize (Hbf p Hpp). lia.
      * specialize (Hbf a Ha). lia.
Qed.

(* Non-vacuous: a SHARED block (rc 2) is cloned to a unique one; an ALREADY-unique
   block (rc 1) is returned as-is, still unique. Both leave the result mutable
   in place without aliasing. *)
Example shared_block_becomes_unique :
  forall st p, rcenv st p = 2 ->
    rcenv (snd (make_unique st p)) (fst (make_unique st p)) = 1.
Proof.
  intros st p H. apply make_unique_yields_unique. lia.
Qed.

Example unique_block_stays_itself :
  forall st p, rcenv st p = 1 -> fst (make_unique st p) = p.
Proof.
  intros st p H. unfold make_unique. rewrite H. cbn. reflexivity.
Qed.

(* AXIOM AUDIT — soundness rests on the kernel alone. *)
Print Assumptions make_unique_yields_unique.
Print Assumptions make_unique_preserves_below_fresh.
