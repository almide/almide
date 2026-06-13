(* Almide v1 trust spine — A1.2 proof foundation: the FREE-LIST allocator is
   REUSE-SAFE.

   A1.1b made the binary FREE at the cell level (rc -> 0). A1.2 adds PHYSICAL
   reclamation: a freed block returns to a free-list and is REUSED, so memory is
   bounded under churn. The danger class is REUSE-AFTER-FREE: handing a block to a
   NEW object while it is still LIVE through an old handle. This file models the
   free-list allocator abstractly and proves the safety core: a VALID allocation
   (the fresh bump frontier, or a block currently on the free-list) is NEVER a
   block that is currently LIVE. So the renderer's free-list, REFINING this model,
   cannot resurrect a live block — the physical-reclamation half of A1's leak-stop,
   PROVEN rather than trusted. (The complementary guarantee — that the OLD handle
   never accesses the reused block — is the ownership checker's dead-handle
   property, OwnershipChecker.check_sound's no-use-after-free.)

   PCC framing: the untrusted runtime CHOOSES which block to reuse; `alloc` takes
   that choice `p` and VALIDATES it (fresh, or on the free-list, else reject), and
   the proof shows any validated choice is safe — the checker never picks. *)

From Stdlib Require Import ZArith.
From Stdlib Require Import Lia.
Open Scope Z_scope.

(* Address sets as characteristic functions (decidable membership; no `pick`). *)
Definition ASet := Z -> bool.
Definition emptyS : ASet := fun _ => false.
Definition addS (s : ASet) (x : Z) : ASet := fun y => if Z.eqb y x then true else s y.
Definition remS (s : ASet) (x : Z) : ASet := fun y => if Z.eqb y x then false else s y.

(* Allocator state: the bump frontier, the free-list (a set of freed addresses),
   and a GHOST set of currently-live addresses (it tracks safety; the real runtime
   does not store it). *)
Record AState := { bump : Z; freeS : ASet; liveS : ASet }.

Definition disjoint (a b : ASet) : Prop := forall x, a x = true -> b x = true -> False.
Definition below (s : ASet) (n : Z) : Prop := forall x, s x = true -> x < n.

(* WELL-FORMEDNESS invariant: free and live are disjoint, and both lie below the
   bump frontier (every tracked block was once allocated). *)
Definition INV (st : AState) : Prop :=
  disjoint (freeS st) (liveS st) /\ below (freeS st) (bump st) /\ below (liveS st) (bump st).

(* ALLOCATE block p: valid iff p is the FRESH bump frontier, or p is currently on
   the free-list; any other (wild) address is rejected (None). *)
Definition alloc (st : AState) (p : Z) : option AState :=
  if Z.eqb p (bump st) then
    Some {| bump := bump st + 1; freeS := freeS st; liveS := addS (liveS st) p |}
  else if freeS st p then
    Some {| bump := bump st; freeS := remS (freeS st) p; liveS := addS (liveS st) p |}
  else None.

(* FREE block p: valid iff p is currently live, so a double-free or wild free is
   rejected — mirrors the rc-cell sentinel that traps a dec of an already-0 cell. *)
Definition free_op (st : AState) (p : Z) : option AState :=
  if liveS st p then
    Some {| bump := bump st; freeS := addS (freeS st) p; liveS := remS (liveS st) p |}
  else None.

(* SAFETY CORE: a valid allocation never returns a block that is currently LIVE —
   no reuse-after-free. Either p is the fresh frontier (nothing live sits at or
   above the frontier) or p is on the free-list (disjoint from live). *)
Theorem alloc_not_live :
  forall st p st', INV st -> alloc st p = Some st' -> liveS st p = false.
Proof.
  intros st p st' [Hdis [Hbf Hbl]] Ha. unfold alloc in Ha.
  destruct (Z.eqb p (bump st)) eqn:Ep.
  - apply Z.eqb_eq in Ep. subst p.
    destruct (liveS st (bump st)) eqn:El; [ | reflexivity ].
    exfalso. apply (Z.lt_irrefl (bump st)). apply Hbl. exact El.
  - destruct (freeS st p) eqn:Ef; [ | discriminate ].
    destruct (liveS st p) eqn:El; [ | reflexivity ].
    exfalso. apply (Hdis p Ef El).
Qed.

(* INV is preserved by a valid allocation, so the safety holds across a whole run
   of allocs/frees (induction lands on a state that still satisfies INV). *)
Theorem alloc_preserves_INV :
  forall st p st', INV st -> alloc st p = Some st' -> INV st'.
Proof.
  intros st p st' [Hdis [Hbf Hbl]] Ha. unfold alloc in Ha.
  destruct (Z.eqb p (bump st)) eqn:Ep.
  - apply Z.eqb_eq in Ep. subst p. injection Ha as <-. unfold INV; simpl.
    split; [ | split ].
    + unfold disjoint. intros x Hf Hl. unfold addS in Hl.
      destruct (Z.eqb x (bump st)) eqn:Ex.
      * apply Z.eqb_eq in Ex. subst x.
        apply (Z.lt_irrefl (bump st)). apply Hbf. exact Hf.
      * apply (Hdis x Hf Hl).
    + unfold below. intros x Hf. assert (Hx : x < bump st) by (apply Hbf; exact Hf). lia.
    + unfold below. intros x Hl. unfold addS in Hl.
      destruct (Z.eqb x (bump st)) eqn:Ex.
      * apply Z.eqb_eq in Ex. subst x. lia.
      * assert (Hx : x < bump st) by (apply Hbl; exact Hl). lia.
  - destruct (freeS st p) eqn:Ef; [ | discriminate ]. injection Ha as <-. unfold INV; simpl.
    split; [ | split ].
    + unfold disjoint. intros x. unfold remS, addS. destruct (Z.eqb x p) eqn:Ex.
      * intros Hcon. discriminate Hcon.
      * intros Hf Hl. apply (Hdis x Hf Hl).
    + unfold below. intros x. unfold remS. destruct (Z.eqb x p) eqn:Ex.
      * intros Hcon. discriminate Hcon.
      * intros Hf. apply Hbf. exact Hf.
    + unfold below. intros x. unfold addS. destruct (Z.eqb x p) eqn:Ex.
      * intros _. apply Z.eqb_eq in Ex. subst x. apply Hbf. exact Ef.
      * intros Hl. apply Hbl. exact Hl.
Qed.

(* A valid free acts only on a LIVE block (a double-free / wild free is rejected),
   and preserves INV — so the freed block lands on the free-list disjoint from the
   (now smaller) live set, ready for a SAFE later reuse by `alloc_not_live`. *)
Theorem free_preserves_INV :
  forall st p st', INV st -> free_op st p = Some st' -> INV st'.
Proof.
  intros st p st' [Hdis [Hbf Hbl]] Hf. unfold free_op in Hf.
  destruct (liveS st p) eqn:El; [ | discriminate ]. injection Hf as <-. unfold INV; simpl.
  split; [ | split ].
  - unfold disjoint. intros x. unfold addS, remS. destruct (Z.eqb x p) eqn:Ex.
    + intros _ Hcon. discriminate Hcon.
    + intros Hf' Hl'. apply (Hdis x Hf' Hl').
  - unfold below. intros x. unfold addS. destruct (Z.eqb x p) eqn:Ex.
    + intros _. apply Z.eqb_eq in Ex. subst x. apply Hbl. exact El.
    + intros Hf'. apply Hbf. exact Hf'.
  - unfold below. intros x. unfold remS. destruct (Z.eqb x p) eqn:Ex.
    + intros Hcon. discriminate Hcon.
    + intros Hl'. apply Hbl. exact Hl'.
Qed.

(* The initial allocator (empty free-list, nothing live) is well-formed, so a run
   starting from it stays safe by the two preservation theorems. *)
Definition init (b : Z) : AState := {| bump := b; freeS := emptyS; liveS := emptyS |}.

Lemma init_INV : forall b, INV (init b).
Proof.
  intros b. unfold INV, init, disjoint, below, emptyS; simpl.
  split; [ | split ]; intros x H; discriminate H.
Qed.

(* Non-vacuous, the full cycle: a fresh alloc of block 0, then free it (back to
   the free-list), then REUSE it — the reuse `alloc st2 0` is VALIDATED (0 is on
   the free-list) and SAFE (`alloc_not_live`: block 0 is not live at reuse). This
   is exactly the reuse-after-free pattern, shown safe end to end. *)
Example reuse_is_validated_and_safe :
  forall st1 st2 st3,
    alloc (init 0) 0 = Some st1 ->   (* fresh alloc of block 0 *)
    free_op st1 0 = Some st2 ->      (* free it -> onto the free-list *)
    alloc st2 0 = Some st3 ->        (* reuse block 0 *)
    liveS st2 0 = false.            (* the reuse was NOT of a live block *)
Proof.
  intros st1 st2 st3 Ha Hf Hr.
  assert (HINV2 : INV st2).
  { apply (free_preserves_INV st1 0 st2); [ | exact Hf ].
    apply (alloc_preserves_INV (init 0) 0 st1); [ apply init_INV | exact Ha ]. }
  apply (alloc_not_live st2 0 st3 HINV2 Hr).
Qed.

(* AXIOM AUDIT — soundness rests on the kernel alone. *)
Print Assumptions alloc_not_live.
Print Assumptions alloc_preserves_INV.
Print Assumptions free_preserves_INV.
