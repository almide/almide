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

(* ══════════════════════════════════════════════════════════════════════════
   REGION RESET (#909, sentinel invariant 1 of 2)

   The renderer's region window (crates/almide-mir/src/region_alloc.rs, rendered
   by render_wasm_p2_b.rs) brackets a pure, closed, escape-free call tree with
   two allocator ops:

     RegionSave     sp := bump | freelist<<32 ;  freelist := 0   (EMPTIED)
     RegionRestore  bump := sp.lo ;  freelist := sp.hi           (both restored)

   Every allocation inside the window is therefore a pure frontier bump (the
   free-list scan is dead), and teardown is a single frontier RESET — the whole
   object graph born inside is reclaimed at once, with no per-node rc_dec.

   The reset introduces a danger the plain alloc/free story never sees: a block
   FREED INSIDE the window sits on the WINDOW's free-list, and if the reset left
   that list in place its entries would point ABOVE the restored frontier — into
   memory the reset just handed back to the bump allocator. The next `alloc`
   would VALIDATE such an entry (it is on the free-list) and hand out a block the
   region's own next bump also hands out: two live objects at one address, i.e.
   reuse-after-free. That is exactly the hazard #909 names.

   What is proven below: RegionSave/RegionRestore, as the renderer performs them
   (restore the SAVED list, not the window's), preserve INV across an ARBITRARY
   window body — any sequence of allocs and frees — so `alloc_not_live` still
   applies after the reset. And the naive alternative (keep the window's list)
   provably breaks INV and provably re-hands a LIVE block, on a concrete trace:
   the invariant has teeth, it is not vacuous. *)

(* The allocator snapshot RegionSave packs into `sp`. *)
Record RSnap := { s_bump : Z; s_free : ASet }.
Definition snap_of (st : AState) : RSnap :=
  {| s_bump := bump st; s_free := freeS st |}.
Definition snap_wf (sn : RSnap) : Prop := below (s_free sn) (s_bump sn).

(* RegionSave: keep the frontier, EMPTY the free-list (the snapshot holds it). *)
Definition region_save (st : AState) : AState :=
  {| bump := bump st; freeS := emptyS; liveS := liveS st |}.

(* Everything at or above the saved frontier was born inside the window and is
   reclaimed WHOLESALE by the frontier reset — so it leaves the ghost live set. *)
Definition maskS (s : ASet) (n : Z) : ASet := fun y => if Z.ltb y n then s y else false.

(* RegionRestore: frontier and free-list both come from the SNAPSHOT. *)
Definition region_restore (sn : RSnap) (st : AState) : AState :=
  {| bump := s_bump sn; freeS := s_free sn; liveS := maskS (liveS st) (s_bump sn) |}.

(* A window BODY is any sequence of validated allocs and frees. Modelling it as
   a relation (rather than a fixed trace) is what makes the reset theorem hold
   for every program the pass may put inside a region. *)
Inductive steps : AState -> AState -> Prop :=
| steps_refl  : forall st, steps st st
| steps_alloc : forall st p st' st'', alloc st p = Some st' -> steps st' st'' -> steps st st''
| steps_free  : forall st p st' st'', free_op st p = Some st' -> steps st' st'' -> steps st st''.

(* The WINDOW invariant: INV still holds, the frontier only grew, and every
   address on the SAVED free-list is still neither free nor live inside the
   window — the property that makes handing the saved list back sound. It holds
   because RegionSave EMPTIED the list: nothing inside can re-free a saved block
   (a free needs the block LIVE, and a saved-free block is never allocated
   inside — the window's free-list starts empty and only ever gains blocks that
   were live). *)
Definition WinINV (sn : RSnap) (st : AState) : Prop :=
  INV st /\ s_bump sn <= bump st /\
  (forall x, s_free sn x = true -> freeS st x = false /\ liveS st x = false).

Lemma save_snap_wf : forall st, INV st -> snap_wf (snap_of st).
Proof. intros st [_ [Hbf _]]. unfold snap_wf, snap_of; simpl. exact Hbf. Qed.

Lemma save_WinINV : forall st, INV st -> WinINV (snap_of st) (region_save st).
Proof.
  intros st [Hdis [Hbf Hbl]]. unfold WinINV, snap_of, region_save; simpl.
  split; [ | split ].
  - unfold INV; simpl. split; [ | split ].
    + unfold disjoint, emptyS. intros x H. discriminate H.
    + unfold below, emptyS. intros x H. discriminate H.
    + exact Hbl.
  - lia.
  - intros x Hf. split; [ reflexivity | ].
    destruct (liveS st x) eqn:El; [ | reflexivity ].
    exfalso. apply (Hdis x Hf El).
Qed.

Lemma WinINV_alloc : forall sn st p st',
  snap_wf sn -> WinINV sn st -> alloc st p = Some st' -> WinINV sn st'.
Proof.
  intros sn st p st' Hwf [HINV [Hb Hs]] Ha.
  assert (HINV' : INV st') by (apply (alloc_preserves_INV st p st' HINV Ha)).
  unfold alloc in Ha. destruct (Z.eqb p (bump st)) eqn:Ep.
  - apply Z.eqb_eq in Ep. subst p. injection Ha as <-.
    split; [ exact HINV' | split ]; simpl.
    + lia.
    + intros x Hf. destruct (Hs x Hf) as [Hf1 Hl1]. split; [ exact Hf1 | ].
      unfold addS. destruct (Z.eqb x (bump st)) eqn:Ex.
      * apply Z.eqb_eq in Ex. subst x. exfalso.
        assert (Hlt : bump st < s_bump sn) by (apply Hwf; exact Hf). lia.
      * exact Hl1.
  - destruct (freeS st p) eqn:Ef; [ | discriminate ]. injection Ha as <-.
    split; [ exact HINV' | split ]; simpl.
    + lia.
    + intros x Hf. destruct (Hs x Hf) as [Hf1 Hl1].
      assert (Hne : Z.eqb x p = false).
      { destruct (Z.eqb x p) eqn:Ex; [ | reflexivity ].
        apply Z.eqb_eq in Ex. subst x. rewrite Ef in Hf1. discriminate Hf1. }
      unfold remS, addS. rewrite Hne. split; [ exact Hf1 | exact Hl1 ].
Qed.

Lemma WinINV_free : forall sn st p st',
  WinINV sn st -> free_op st p = Some st' -> WinINV sn st'.
Proof.
  intros sn st p st' [HINV [Hb Hs]] Hfr.
  assert (HINV' : INV st') by (apply (free_preserves_INV st p st' HINV Hfr)).
  unfold free_op in Hfr. destruct (liveS st p) eqn:El; [ | discriminate ].
  injection Hfr as <-. split; [ exact HINV' | split ]; simpl.
  - lia.
  - intros x Hx. destruct (Hs x Hx) as [Hf1 Hl1].
    assert (Hne : Z.eqb x p = false).
    { destruct (Z.eqb x p) eqn:Ex; [ | reflexivity ].
      apply Z.eqb_eq in Ex. subst x. rewrite El in Hl1. discriminate Hl1. }
    unfold addS, remS. rewrite Hne. split; [ exact Hf1 | exact Hl1 ].
Qed.

(* The window invariant survives an ARBITRARY body. *)
Lemma WinINV_steps : forall sn st st',
  snap_wf sn -> steps st st' -> WinINV sn st -> WinINV sn st'.
Proof.
  intros sn st st' Hwf Hst. induction Hst as
    [ st0 | st0 p st1 st2 Ha _ IH | st0 p st1 st2 Hfr _ IH ]; intros Hw.
  - exact Hw.
  - apply IH. apply (WinINV_alloc sn st0 p st1 Hwf Hw Ha).
  - apply IH. apply (WinINV_free sn st0 p st1 Hw Hfr).
Qed.

(* SENTINEL 1a: the reset re-establishes INV. Since INV contains
   `below (freeS st) (bump st)`, this IS the statement that the reset leaves NO
   free-list entry pointing into the reclaimed region. *)
Theorem restore_preserves_INV : forall sn st,
  snap_wf sn -> WinINV sn st -> INV (region_restore sn st).
Proof.
  intros sn st Hwf [HINV [Hb Hs]]. unfold INV, region_restore; simpl.
  split; [ | split ].
  - unfold disjoint. intros x Hf Hl. destruct (Hs x Hf) as [_ Hl1].
    unfold maskS in Hl. destruct (Z.ltb x (s_bump sn)).
    + rewrite Hl1 in Hl. discriminate Hl.
    + discriminate Hl.
  - exact Hwf.
  - unfold below, maskS. intros x Hl. destruct (Z.ltb x (s_bump sn)) eqn:Ex.
    + apply Z.ltb_lt in Ex. exact Ex.
    + discriminate Hl.
Qed.

(* SENTINEL 1b, said directly: after the reset every free-list entry lies BELOW
   the restored frontier — no entry points into the region just reclaimed, so
   no later `alloc` can validate a block inside it via the free-list path. *)
Theorem region_reset_leaves_no_free_into_the_region : forall sn st x,
  snap_wf sn -> freeS (region_restore sn st) x = true -> x < bump (region_restore sn st).
Proof. intros sn st x Hwf H. simpl in *. apply Hwf. exact H. Qed.

(* SENTINEL 1c: the reset reclaims the WHOLE window — every block born at or
   above the saved frontier is dead afterwards (that is what "no per-node
   rc_dec" costs nothing). *)
Theorem region_reset_reclaims_the_window : forall sn st x,
  s_bump sn <= x -> liveS (region_restore sn st) x = false.
Proof.
  intros sn st x H. simpl. unfold maskS.
  destruct (Z.ltb x (s_bump sn)) eqn:Ex; [ | reflexivity ].
  apply Z.ltb_lt in Ex. lia.
Qed.

(* SENTINEL 1d: and it reclaims ONLY the window — an object that predates the
   region is untouched by the reset (the window is not allowed to free the
   outside world's blocks out from under it). *)
Theorem region_reset_preserves_outside : forall sn st x,
  x < s_bump sn -> liveS (region_restore sn st) x = liveS st x.
Proof.
  intros sn st x H. simpl. unfold maskS.
  destruct (Z.ltb x (s_bump sn)) eqn:Ex; [ reflexivity | ].
  apply Z.ltb_ge in Ex. lia.
Qed.

(* END TO END: open a region on a well-formed allocator, run ANY window body,
   reset — and the allocator is well-formed again, so reuse-safety
   (`alloc_not_live`) holds on the other side of the reset. *)
Theorem region_window_preserves_INV : forall st stend,
  INV st -> steps (region_save st) stend -> INV (region_restore (snap_of st) stend).
Proof.
  intros st stend HINV Hst.
  apply restore_preserves_INV; [ apply save_snap_wf; exact HINV | ].
  apply (WinINV_steps (snap_of st) (region_save st) stend).
  - apply save_snap_wf; exact HINV.
  - exact Hst.
  - apply save_WinINV; exact HINV.
Qed.

Theorem region_window_reuse_safe : forall st stend p st',
  INV st -> steps (region_save st) stend ->
  alloc (region_restore (snap_of st) stend) p = Some st' ->
  liveS (region_restore (snap_of st) stend) p = false.
Proof.
  intros st stend p st' HINV Hst Ha.
  apply (alloc_not_live (region_restore (snap_of st) stend) p st'); [ | exact Ha ].
  apply (region_window_preserves_INV st stend HINV Hst).
Qed.

(* ── TEETH: the invariant is not vacuous, and the hazard is real ──────────
   A concrete window: open at frontier 0, bump-allocate blocks 0 and 1 inside,
   free block 1 inside (it lands on the WINDOW free-list), close. *)
Definition r1 : AState := {| bump := 1; freeS := emptyS; liveS := addS emptyS 0 |}.
Definition r2 : AState := {| bump := 2; freeS := emptyS; liveS := addS (addS emptyS 0) 1 |}.
Definition r3 : AState :=
  {| bump := 2; freeS := addS emptyS 1; liveS := remS (addS (addS emptyS 0) 1) 1 |}.
Definition sn0 : RSnap := snap_of (init 0).

Example region_window_trace : steps (region_save (init 0)) r3.
Proof.
  apply (steps_alloc _ 0 r1); [ reflexivity | ].
  apply (steps_alloc _ 1 r2); [ reflexivity | ].
  apply (steps_free _ 1 r3); [ reflexivity | ].
  apply steps_refl.
Qed.

(* The REAL reset: the block freed inside the window is neither on the restored
   free-list nor live — it went away with the frontier, leaving no stale entry. *)
Example region_reset_drops_the_window_free_list :
  freeS (region_restore sn0 r3) 1 = false /\ liveS (region_restore sn0 r3) 1 = false.
Proof. split; reflexivity. Qed.

Example region_reset_is_well_formed : INV (region_restore sn0 r3).
Proof.
  apply (region_window_preserves_INV (init 0) r3).
  - apply init_INV.
  - apply region_window_trace.
Qed.

(* The NAIVE reset — restore the frontier but KEEP the window's free-list. *)
Definition naive_restore (sn : RSnap) (st : AState) : AState :=
  {| bump := s_bump sn; freeS := freeS st; liveS := maskS (liveS st) (s_bump sn) |}.

(* It is immediately ill-formed: the stale entry (block 1) sits ABOVE the
   restored frontier (0) — INV's `below (freeS st) (bump st)` is exactly the
   clause that catches a free-list pointing into a reclaimed region. *)
Example naive_restore_breaks_INV : ~ INV (naive_restore sn0 r3).
Proof.
  intros [_ [Hbf _]]. assert (H : 1 < 0) by (apply Hbf; reflexivity). lia.
Qed.

(* And ill-formed HERE means unsafe, not merely untidy: with the stale entry the
   allocator validates block 1 off the free-list (frontier stays 0), then
   bump-allocates block 0, and the NEXT fresh bump hands out block 1 AGAIN —
   while it is still LIVE. Two live objects at one address: the reuse-after-free
   the region reset would have introduced. *)
Definition n1 : AState :=
  {| bump := 0; freeS := remS (addS emptyS 1) 1;
     liveS := addS (maskS (liveS r3) 0) 1 |}.
Definition n2 : AState :=
  {| bump := 1; freeS := remS (addS emptyS 1) 1;
     liveS := addS (addS (maskS (liveS r3) 0) 1) 0 |}.

Example naive_reset_reallocates_a_live_block :
  alloc (naive_restore sn0 r3) 1 = Some n1 /\
  alloc n1 0 = Some n2 /\
  liveS n2 1 = true /\
  (exists s, alloc n2 1 = Some s).
Proof.
  split; [ reflexivity | ]. split; [ reflexivity | ]. split; [ reflexivity | ].
  eexists. reflexivity.
Qed.

(* AXIOM AUDIT — soundness rests on the kernel alone. *)
Print Assumptions alloc_not_live.
Print Assumptions alloc_preserves_INV.
Print Assumptions free_preserves_INV.
Print Assumptions restore_preserves_INV.
Print Assumptions region_reset_leaves_no_free_into_the_region.
Print Assumptions region_reset_reclaims_the_window.
Print Assumptions region_reset_preserves_outside.
Print Assumptions region_window_preserves_INV.
Print Assumptions region_window_reuse_safe.
