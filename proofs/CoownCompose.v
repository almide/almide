(* Almide v1 trust kernel — 柱C Brick 3 COMPOSITION: block-balance ∧ child-balance ⇒ fully safe.

   CoownLoop.v proves the CHILD account (a container's immediate elements are returned to their source
   refcount by fill-then-recursive-drop). OwnershipChecker.v / OwnershipLoop.v prove the BLOCK account
   (a heap object's own refcount is acquired once and released to zero exactly once, never faulting).
   A Value container needs BOTH, and they meet at exactly one point: when the container's block rc hits
   0 (its last reference dies), the runtime fires the recursive child drop. This file proves the two
   independent accounts COMPOSE — a container whose block lifecycle is balanced and whose children are
   source-owned is fully leak/double-free-free — by REUSING both proofs (no re-derivation): the block
   stream runs through OwnershipLoop.exec_flat on `blk`, the child fill/drop through CoownLoop.rec_drop
   on `kids`, joined at the blk=0 trigger. This is the Coq half of the Brick-3 integration the MIR cert
   section consumes: a recognized co-own producer (ContainerFill) paired with its recursive drop
   (ContainerDrop) at the container's death inherits `lifecycle_safe`. *)

From Stdlib Require Import List.
Import ListNotations.
From Stdlib Require Import ZArith.
From Stdlib Require Import Lia.
From AlmideTrust Require Import OwnershipLoop.   (* exec_flat, FlatOp (FInc/FDec/FMove) — the BLOCK account *)
From AlmideTrust Require Import CoownLoop.        (* coown_fill, rec_drop, coown_fill_drop_neutral — the CHILD account *)
Open Scope Z_scope.

(* A Value container = its OWN (block) refcount + its immediate children's refcounts. *)
Record container := mk_container { blk : Z ; kids : rcvec }.

(* The constructor (value.array / value.object / __copy_value): the block is born at rc 1 and the
   children are co-own-filled (+1 each — the producer's rc_inc). *)
Definition alloc_container (src : rcvec) : container :=
  mk_container 1 (coown_fill src).

(* The FULL container lifecycle. From the constructor (blk = 1, kids = coown_fill src), run the container
   HANDLE's post-alloc block-op stream (acquire/release of the container reference) via OwnershipLoop's
   exec_flat on `blk`. When that releases the block to 0 — the container's last reference dies — the
   runtime fires the recursive child drop (CoownLoop's rec_drop on the filled children). Result: the
   final (blk, kids), or None on ANY fault — a block double-free (exec_flat None) OR a child double-free
   (rec_drop None). *)
Definition lifecycle (src : rcvec) (block_ops : list FlatOp) : option (Z * rcvec) :=
  match exec_flat block_ops 1 with
  | None => None
  | Some b =>
      if Z.eqb b 0
      then match rec_drop (coown_fill src) with
           | Some k => Some (0, k)
           | None => None
           end
      else Some (b, coown_fill src)
  end.

Definition lc_no_double_free (src : rcvec) (ops : list FlatOp) : Prop :=
  lifecycle src ops <> None.
Definition lc_no_leak (src : rcvec) (ops : list FlatOp) : Prop :=
  lifecycle src ops = Some (0, src).

(* ─── THE COMPOSITION THEOREM ───
   A container whose BLOCK lifecycle is balanced (its handle ops release it to 0 without faulting — the
   guarantee OwnershipChecker.v / OwnershipLoop.v give for an accepted block cert) and whose CHILDREN are
   source-owned (rc >= 1 — CoownLoop.v's precondition) is FULLY safe: the block is freed (blk = 0) AND
   every child returns to its source refcount (the copy is rc-neutral on the shared elements). The two
   proofs compose because the block account (exec_flat on `blk`) and the child account (rec_drop on
   `kids`) are independent, joined only at the single freeing point. *)
Theorem lifecycle_safe :
  forall src block_ops,
    Forall (fun r => 1 <= r) src ->
    exec_flat block_ops 1 = Some 0 ->
    lifecycle src block_ops = Some (0, src).
Proof.
  intros src ops Hsrc Hblk. unfold lifecycle.
  rewrite Hblk. rewrite Z.eqb_refl.
  rewrite (coown_fill_drop_neutral src Hsrc). reflexivity.
Qed.

(* The two headline safety properties of the whole container lifecycle. *)
Corollary lifecycle_no_double_free :
  forall src ops,
    Forall (fun r => 1 <= r) src -> exec_flat ops 1 = Some 0 ->
    lc_no_double_free src ops.
Proof.
  intros src ops Hs Hb. unfold lc_no_double_free.
  rewrite (lifecycle_safe src ops Hs Hb). discriminate.
Qed.

Corollary lifecycle_no_leak :
  forall src ops,
    Forall (fun r => 1 <= r) src -> exec_flat ops 1 = Some 0 ->
    lc_no_leak src ops.
Proof.
  intros src ops Hs Hb. unfold lc_no_leak.
  exact (lifecycle_safe src ops Hs Hb).
Qed.

(* ─── non-vacuity ─── *)

(* A container handle shared once (FInc) then both references released (FDec; FDec), over a 2-element
   source each owned at rc 1: the block is freed and the children are returned to rc 1. The full Value
   "copy then drop both ends" lifecycle, certified. *)
Example shared_container_lifecycle_balanced :
  lifecycle [1; 1] [FInc; FDec; FDec] = Some (0, [1; 1]).
Proof. reflexivity. Qed.

(* A block that is NEVER released to 0 (the container leaks) is NOT in the certified state: blk stays
   alive (1) and the children are still co-owned — exactly the un-freed shape the block cert rejects. *)
Example leaked_container_not_freed :
  lifecycle [1] [] = Some (1, [2]).
Proof. reflexivity. Qed.

(* A block DOUBLE-FREE (one acquire, two releases) faults the lifecycle — caught as None, before any
   child drop. The block account alone rejects it; composition preserves that rejection. *)
Example block_double_free_faults :
  lifecycle [1] [FDec; FDec] = None.
Proof. reflexivity. Qed.

(* Axiom audit — the composition must rest on the Coq kernel alone (no admits / axioms), inheriting
   OwnershipLoop.v's and CoownLoop.v's axiom-clean status. *)
Print Assumptions lifecycle_safe.
Print Assumptions lifecycle_no_double_free.
Print Assumptions lifecycle_no_leak.
