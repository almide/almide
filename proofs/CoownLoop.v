(* Almide v1 trust kernel — 柱C Brick 3 FOUNDATION: the CO-OWN COPY / RECURSIVE-DROP balance.

   OwnershipLoop.v closes the LOOP-CARRIED-SLOT incompleteness: a loop whose body
   PRESERVES the refcount (net 0) is sound for any iteration count. But it explicitly
   EXCLUDES the CO-OWN copy loop — a producer that `rc_inc`s each loaded element into a
   fresh container (body `[FInc]`, net +1 per element) is rejected there (leaky_loop),
   because its balancing release lives NOT in the same loop but in a SEPARATE recursive
   drop (`__drop_value`/`__vdrop_arr`) that runs later, over the SAME elements. The
   balance is therefore CROSS-loop, keyed by CONTAINER ELEMENT COUNT, not per-iteration.
   This is the structural blocker that left Value-rc co-own producers (value.array,
   __copy_value, value.object, list.set_value, value.merge) on the differential-test
   floor — cert-invisible, only the leak-loop guarding them (the柱C Brick-3 scope).

   This file proves the CORE composition lemma AT THE ROOT: model the container's
   elements as a VECTOR of refcounts (one per immediate element, each source-owned at
   rc >= 1). The producer CO-OWN FILL bumps each (+1, the rc_inc); the container's
   RECURSIVE DROP releases each (-1, faulting on a <= 0 = double-free). The theorem:
   fill-then-drop returns EVERY element to its source rc (no leak) and never faults (no
   double-free) — for ANY element count. The source's own references are untouched, so
   the copy is rc-NEUTRAL on the shared elements: exactly the safety the leak-loop
   checks empirically, here PROVED on the Coq kernel. (Shallow: the immediate elements.
   A deeper level fires only at an element's LAST ref — the SOURCE's drop — which is the
   source's own already-proven concern, so the shallow vector is the producer's whole
   contribution. The cert/MIR integration — pairing a producer's fill events with the
   matching recursive-drop by container identity, via a typed nested-element model — is
   the remaining ENGINEERING brick; this file is its proven spine.) *)

From Stdlib Require Import List.
Import ListNotations.
From Stdlib Require Import ZArith.
From Stdlib Require Import Lia.
Open Scope Z_scope.

(* The container's IMMEDIATE elements as a vector of refcounts — one Z per element, its
   current refcount. A source-owned element has rc >= 1 (the source list holds one ref). *)
Definition rcvec := list Z.

(* CO-OWN FILL: the producer (`__varr_copy`/__copy_value/__vobj_fill) loads each element
   and `rc_inc`s it into the fresh copy — +1 on every element. Always valid (an acquire
   never faults), so it is total. This is the loop body `[FInc]` per element. *)
Definition coown_fill (v : rcvec) : rcvec := map (fun r => r + 1) v.

(* RECURSIVE DROP: the container's `__drop_value` releases each element — -1 on every
   element, FAULTING (None) on a release at rc <= 0 (a double-free / use-after-free). It
   walks the SAME elements the fill did (same count N = length v). This is the separate
   recursive-drop loop whose body is `[FDec]` per element. *)
Fixpoint rec_drop (v : rcvec) : option rcvec :=
  match v with
  | [] => Some []
  | r :: rest =>
      if r <=? 0 then None
      else match rec_drop rest with
           | Some rest' => Some ((r - 1) :: rest')
           | None => None
           end
  end.

(* The semantic safety properties of running the producer's fill THEN the container's
   recursive drop over a source vector `v`. *)
Definition no_double_free (v : rcvec) : Prop := rec_drop (coown_fill v) <> None.
Definition no_leak (v : rcvec) : Prop := rec_drop (coown_fill v) = Some v.

(* ─── the fill and the drop iterate the SAME element count (the cross-loop key) ─── *)
Lemma coown_fill_length : forall v, length (coown_fill v) = length v.
Proof. intro v. unfold coown_fill. apply length_map. Qed.

Lemma rec_drop_some_length :
  forall v w, rec_drop v = Some w -> length w = length v.
Proof.
  induction v as [| r v IH]; intros w H; simpl in *.
  - injection H as <-. reflexivity.
  - destruct (r <=? 0) eqn:Er; [discriminate |].
    destruct (rec_drop v) as [v' |] eqn:Ev; [| discriminate].
    injection H as <-. simpl. f_equal. apply IH. reflexivity.
Qed.

(* ─── THE CORE COMPOSITION LEMMA ───
   For a source vector where every element is owned (rc >= 1), the producer's co-own
   fill followed by the container's recursive drop returns EXACTLY the source vector:
   each element's `+1` (the copy's acquire) is matched by a `-1` (the copy's release),
   leaving the source's own reference intact. No `rec_drop` step faults (every release is
   at rc >= 2). This is the net-+1-balanced-by-a-separate-recursive-drop case the
   per-iteration loop rule could not express. *)
Lemma coown_fill_drop_neutral :
  forall v, Forall (fun r => 1 <= r) v -> rec_drop (coown_fill v) = Some v.
Proof.
  induction v as [| r v IH]; intro Hall; simpl.
  - reflexivity.
  - inversion Hall as [| x xs Hr Hrest]; subst.
    (* coown_fill (r :: v) = (r+1) :: coown_fill v; rec_drop sees (r+1) at the head. *)
    assert (Hpos : (r + 1 <=? 0) = false).
    { apply Z.leb_gt. lia. }
    rewrite Hpos.
    rewrite (IH Hrest).
    (* (r+1) - 1 = r *)
    replace (r + 1 - 1) with r by lia.
    reflexivity.
Qed.

(* The two headline safety properties fall out directly. *)
Theorem coown_copy_no_double_free :
  forall v, Forall (fun r => 1 <= r) v -> no_double_free v.
Proof.
  intros v Hall. unfold no_double_free.
  rewrite (coown_fill_drop_neutral v Hall). discriminate.
Qed.

Theorem coown_copy_no_leak :
  forall v, Forall (fun r => 1 <= r) v -> no_leak v.
Proof.
  intros v Hall. unfold no_leak.
  exact (coown_fill_drop_neutral v Hall).
Qed.

(* ─── non-vacuity ─── *)

(* A 3-element source Array, each element owned at rc 1: the copy's co-own fill + the
   recursive drop return every element to rc 1 (the source keeps its refs). BALANCED. *)
Example three_owned_elements_balanced :
  rec_drop (coown_fill [1; 1; 1]) = Some [1; 1; 1].
Proof. reflexivity. Qed.

(* Elements already shared (rc 2, e.g. co-owned by two lists): still balanced — the copy
   adds and removes one ref, the other owners' refs are preserved. *)
Example shared_elements_balanced :
  rec_drop (coown_fill [2; 3]) = Some [2; 3].
Proof. reflexivity. Qed.

(* The DANGER the model rules out: a recursive drop run WITHOUT the matching fill (an
   extra drop, or a fill that under-counts the elements) faults — a release at rc 0 is a
   double-free, caught as None. Here a drop on a fresh-but-unfilled [0] element. *)
Example unmatched_drop_double_frees :
  rec_drop [0] = None.
Proof. reflexivity. Qed.

(* And a fill WITHOUT the recursive drop leaks: the elements end at rc+1, never returning
   to the source — the value of pairing the producer with the container's drop. (Shown as
   the fill result differing from the source.) *)
Example unmatched_fill_leaks :
  coown_fill [1; 1] = [2; 2] /\ [2; 2] <> [1; 1].
Proof. split; [reflexivity | discriminate]. Qed.

(* Axiom audit — soundness must rest on the Coq kernel alone (no admits / axioms). *)
Print Assumptions coown_fill_drop_neutral.
Print Assumptions coown_copy_no_double_free.
Print Assumptions coown_copy_no_leak.
