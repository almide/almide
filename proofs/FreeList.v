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

(* ══════════════════════════════════════════════════════════════════════════
   PINNED_RC IMMORTALITY (#909, sentinel invariant 2 of 2)

   Every host-written WASI buffer — the fd_out/stat/iov/nread/read-data scratch,
   the preopen tables, the resolve result — is allocated by `$alloc8` (the emit
   backend's `__alloc_pinned`): a BUMP-ONLY allocation, never routed through the
   free-list, whose rc cell holds the PINNED_RC sentinel. `rc_inc` and `rc_dec`
   early-out on that sentinel, so a pinned block never reaches rc 0 and therefore
   never joins the free-list: it is IMMORTAL by construction.

   That immortality is load-bearing, not decorative. C-042's true root cause was
   precisely its absence: `__init_preopen_dirs` wrote the preopen TABLE POINTER
   into the free-list head at boot, so `$alloc` handed the live table out as a
   reusable block to every fs-using program, in every release for months.

   The model adds a PIN set to the allocator. The invariant to prove is that a
   pinned address is in NEITHER the free-list NOR the churn-live set, and that
   this survives every operation — so `$alloc` can never return a pinned block
   (it is not the frontier and it is not on the free-list) and `$rc_dec` can
   never push one onto the free-list (the sentinel early-out fires first). *)

Record PState := { pa : AState; pinS : ASet }.

(* The pinned blocks form a THIRD category: allocated (below the frontier), but
   neither reusable (never on the free-list) nor churn-tracked (never live in the
   rc sense — their rc is the sentinel, not a count). *)
Definition PINV (ps : PState) : Prop :=
  INV (pa ps) /\ disjoint (pinS ps) (freeS (pa ps)) /\
  disjoint (pinS ps) (liveS (pa ps)) /\ below (pinS ps) (bump (pa ps)).

(* `$alloc` — the churn allocator, exactly as before; it does not know about pins. *)
Definition p_alloc (ps : PState) (p : Z) : option PState :=
  match alloc (pa ps) p with
  | Some a' => Some {| pa := a'; pinS := pinS ps |}
  | None => None
  end.

(* `$alloc8` / `__alloc_pinned` — the FRESH FRONTIER ONLY (it never consults the
   free-list), and the block joins the PIN set rather than the live set. *)
Definition p_alloc_pinned (ps : PState) (p : Z) : option PState :=
  if Z.eqb p (bump (pa ps))
  then Some {| pa := {| bump := bump (pa ps) + 1;
                        freeS := freeS (pa ps); liveS := liveS (pa ps) |};
               pinS := addS (pinS ps) p |}
  else None.

(* `$rc_dec` — the PINNED_RC sentinel early-out comes FIRST, so a release of a
   pinned block is a no-op and can never push it onto the free-list. *)
Definition p_free (ps : PState) (p : Z) : option PState :=
  if pinS ps p then None
  else match free_op (pa ps) p with
       | Some a' => Some {| pa := a'; pinS := pinS ps |}
       | None => None
       end.

(* SENTINEL 2a: the rc_dec early-out — a pinned block is never freed, so it can
   never reach the free-list push. *)
Theorem pinned_is_never_freed : forall ps p, pinS ps p = true -> p_free ps p = None.
Proof. intros ps p H. unfold p_free. rewrite H. reflexivity. Qed.

(* SENTINEL 2b: a pinned block is not on the free-list (the C-042 shape: the
   preopen table can never be sitting in the free-list head). *)
Theorem pinned_is_not_on_the_free_list : forall ps p,
  PINV ps -> pinS ps p = true -> freeS (pa ps) p = false.
Proof.
  intros ps p [_ [Hpf _]] Hp. destruct (freeS (pa ps) p) eqn:Ef; [ | reflexivity ].
  exfalso. apply (Hpf p Hp Ef).
Qed.

(* SENTINEL 2c, the punchline: `$alloc` REJECTS a pinned address outright. It is
   not the fresh frontier (pins lie strictly below it) and it is not on the
   free-list (2b) — so no runtime choice, however wrong, gets validated into
   handing a host buffer to a program object. *)
Theorem pinned_is_not_allocatable : forall ps p,
  PINV ps -> pinS ps p = true -> p_alloc ps p = None.
Proof.
  intros ps p [_ [Hpf [_ Hpb]]] Hp. unfold p_alloc, alloc.
  destruct (Z.eqb p (bump (pa ps))) eqn:Ep.
  - apply Z.eqb_eq in Ep. subst p. exfalso.
    apply (Z.lt_irrefl (bump (pa ps))). apply Hpb. exact Hp.
  - destruct (freeS (pa ps) p) eqn:Ef; [ | reflexivity ].
    exfalso. apply (Hpf p Hp Ef).
Qed.

(* The `alloc_not_live` twin: a VALIDATED allocation is never a pinned block. *)
Theorem p_alloc_not_pinned : forall ps p ps',
  PINV ps -> p_alloc ps p = Some ps' -> pinS ps p = false.
Proof.
  intros ps p ps' HP Ha. destruct (pinS ps p) eqn:Hp; [ | reflexivity ].
  rewrite (pinned_is_not_allocatable ps p HP Hp) in Ha. discriminate Ha.
Qed.

Theorem p_alloc_preserves_PINV : forall ps p ps',
  PINV ps -> p_alloc ps p = Some ps' -> PINV ps'.
Proof.
  intros ps p ps' HP Ha.
  assert (Hnp : pinS ps p = false) by (apply (p_alloc_not_pinned ps p ps' HP Ha)).
  destruct HP as [HINV [Hpf [Hpl Hpb]]].
  unfold p_alloc in Ha. destruct (alloc (pa ps) p) as [a'|] eqn:Ea; [ | discriminate ].
  injection Ha as <-.
  assert (HINV' : INV a') by (apply (alloc_preserves_INV (pa ps) p a' HINV Ea)).
  unfold PINV; simpl. split; [ exact HINV' | ].
  unfold alloc in Ea. destruct (Z.eqb p (bump (pa ps))) eqn:Ep.
  - apply Z.eqb_eq in Ep. subst p. injection Ea as <-. simpl.
    split; [ exact Hpf | split ].
    + unfold disjoint, addS. intros x Hx. destruct (Z.eqb x (bump (pa ps))) eqn:Ex.
      * apply Z.eqb_eq in Ex. subst x. rewrite Hnp in Hx. discriminate Hx.
      * intros Hl. apply (Hpl x Hx Hl).
    + unfold below. intros x Hx.
      assert (H : x < bump (pa ps)) by (apply Hpb; exact Hx). lia.
  - destruct (freeS (pa ps) p) eqn:Ef; [ | discriminate ]. injection Ea as <-. simpl.
    split; [ | split ].
    + unfold disjoint, remS. intros x Hx. destruct (Z.eqb x p) eqn:Ex.
      * intros Hc. discriminate Hc.
      * intros Hff. apply (Hpf x Hx Hff).
    + unfold disjoint, addS. intros x Hx. destruct (Z.eqb x p) eqn:Ex.
      * apply Z.eqb_eq in Ex. subst x. rewrite Hnp in Hx. discriminate Hx.
      * intros Hl. apply (Hpl x Hx Hl).
    + exact Hpb.
Qed.

Theorem p_alloc_pinned_preserves_PINV : forall ps p ps',
  PINV ps -> p_alloc_pinned ps p = Some ps' -> PINV ps'.
Proof.
  intros ps p ps' [HINV [Hpf [Hpl Hpb]]] Ha. unfold p_alloc_pinned in Ha.
  destruct (Z.eqb p (bump (pa ps))) eqn:Ep; [ | discriminate ].
  apply Z.eqb_eq in Ep. subst p. injection Ha as <-.
  destruct HINV as [Hdis [Hbf Hbl]]. unfold PINV; simpl.
  split; [ | split; [ | split ] ].
  - unfold INV; simpl. split; [ exact Hdis | split ].
    + unfold below. intros x Hx.
      assert (H : x < bump (pa ps)) by (apply Hbf; exact Hx). lia.
    + unfold below. intros x Hx.
      assert (H : x < bump (pa ps)) by (apply Hbl; exact Hx). lia.
  - unfold disjoint, addS. intros x Hx Hf.
    destruct (Z.eqb x (bump (pa ps))) eqn:Ex.
    + apply Z.eqb_eq in Ex. subst x.
      apply (Z.lt_irrefl (bump (pa ps))). apply Hbf. exact Hf.
    + apply (Hpf x Hx Hf).
  - unfold disjoint, addS. intros x Hx Hl.
    destruct (Z.eqb x (bump (pa ps))) eqn:Ex.
    + apply Z.eqb_eq in Ex. subst x.
      apply (Z.lt_irrefl (bump (pa ps))). apply Hbl. exact Hl.
    + apply (Hpl x Hx Hl).
  - unfold below, addS. intros x Hx. destruct (Z.eqb x (bump (pa ps))) eqn:Ex.
    + apply Z.eqb_eq in Ex. subst x. lia.
    + assert (H : x < bump (pa ps)) by (apply Hpb; exact Hx). lia.
Qed.

Theorem p_free_preserves_PINV : forall ps p ps',
  PINV ps -> p_free ps p = Some ps' -> PINV ps'.
Proof.
  intros ps p ps' [HINV [Hpf [Hpl Hpb]]] Hfr. unfold p_free in Hfr.
  destruct (pinS ps p) eqn:Hp; [ discriminate | ].
  destruct (free_op (pa ps) p) as [a'|] eqn:Ef; [ | discriminate ].
  injection Hfr as <-.
  assert (HINV' : INV a') by (apply (free_preserves_INV (pa ps) p a' HINV Ef)).
  unfold PINV; simpl. split; [ exact HINV' | ].
  unfold free_op in Ef. destruct (liveS (pa ps) p) eqn:El; [ | discriminate ].
  injection Ef as <-. simpl. split; [ | split ].
  - unfold disjoint, addS. intros x Hx. destruct (Z.eqb x p) eqn:Ex.
    + apply Z.eqb_eq in Ex. subst x. rewrite Hp in Hx. discriminate Hx.
    + intros Hff. apply (Hpf x Hx Hff).
  - unfold disjoint, remS. intros x Hx. destruct (Z.eqb x p) eqn:Ex.
    + intros Hc. discriminate Hc.
    + intros Hl. apply (Hpl x Hx Hl).
  - exact Hpb.
Qed.

(* A whole RUN of the pin-aware allocator: churn allocs, pinned allocs, frees. *)
Inductive p_steps : PState -> PState -> Prop :=
| p_steps_refl  : forall ps, p_steps ps ps
| p_steps_alloc : forall ps p ps' ps'',
    p_alloc ps p = Some ps' -> p_steps ps' ps'' -> p_steps ps ps''
| p_steps_pin   : forall ps p ps' ps'',
    p_alloc_pinned ps p = Some ps' -> p_steps ps' ps'' -> p_steps ps ps''
| p_steps_free  : forall ps p ps' ps'',
    p_free ps p = Some ps' -> p_steps ps' ps'' -> p_steps ps ps''.

Theorem p_steps_preserve_PINV : forall ps ps', p_steps ps ps' -> PINV ps -> PINV ps'.
Proof.
  intros ps ps' Hst. induction Hst as
    [ ps0 | ps0 p ps1 ps2 Ha _ IH | ps0 p ps1 ps2 Ha _ IH | ps0 p ps1 ps2 Hfr _ IH ];
    intros HP.
  - exact HP.
  - apply IH. apply (p_alloc_preserves_PINV ps0 p ps1 HP Ha).
  - apply IH. apply (p_alloc_pinned_preserves_PINV ps0 p ps1 HP Ha).
  - apply IH. apply (p_free_preserves_PINV ps0 p ps1 HP Hfr).
Qed.

(* Once pinned, ALWAYS pinned: no operation ever removes an address from the pin
   set — the IMMORTALITY half of PINNED_RC. *)
Theorem pins_are_immortal : forall ps ps' x,
  p_steps ps ps' -> pinS ps x = true -> pinS ps' x = true.
Proof.
  intros ps ps' x Hst. induction Hst as
    [ ps0 | ps0 p ps1 ps2 Ha _ IH | ps0 p ps1 ps2 Ha _ IH | ps0 p ps1 ps2 Hfr _ IH ];
    intros Hx.
  - exact Hx.
  - apply IH. unfold p_alloc in Ha.
    destruct (alloc (pa ps0) p); [ injection Ha as <-; exact Hx | discriminate ].
  - apply IH. unfold p_alloc_pinned in Ha.
    destruct (Z.eqb p (bump (pa ps0))); [ | discriminate ].
    injection Ha as <-. simpl. unfold addS.
    destruct (Z.eqb x p); [ reflexivity | exact Hx ].
  - apply IH. unfold p_free in Hfr. destruct (pinS ps0 p); [ discriminate | ].
    destruct (free_op (pa ps0) p); [ injection Hfr as <-; exact Hx | discriminate ].
Qed.

(* THE PINNED_RC PROPERTY, over an arbitrary run: a block pinned at boot is, at
   every later point, still pinned, still off the free-list, and still not
   allocatable. This is the invariant C-042 violated. *)
Theorem pinned_stays_immortal_forever : forall ps ps' p,
  PINV ps -> p_steps ps ps' -> pinS ps p = true ->
  pinS ps' p = true /\ freeS (pa ps') p = false /\ p_alloc ps' p = None.
Proof.
  intros ps ps' p HP Hst Hp.
  assert (HP' : PINV ps') by (apply (p_steps_preserve_PINV ps ps' Hst HP)).
  assert (Hp' : pinS ps' p = true) by (apply (pins_are_immortal ps ps' p Hst Hp)).
  split; [ exact Hp' | split ].
  - apply (pinned_is_not_on_the_free_list ps' p HP' Hp').
  - apply (pinned_is_not_allocatable ps' p HP' Hp').
Qed.

(* ── PINS ACROSS A REGION RESET ──────────────────────────────────────────
   The two sentinels meet here. A frontier reset would reclaim a pinned block
   allocated INSIDE the window — the host would go on writing into memory the
   bump allocator has handed back. The renderer forecloses that at the source
   level: a region clone is verified PURE (no host-capability prim, region_alloc.rs
   soundness condition 2), so no `$alloc8` can run inside a window. The model
   mirrors that exactly — a window body is a `steps` sequence, which contains NO
   pinned allocation, so every pin predates the save and lies below the saved
   frontier by PINV. Under that reading the reset provably keeps every pin. *)
Definition p_region_save (ps : PState) : PState :=
  {| pa := region_save (pa ps); pinS := pinS ps |}.
Definition p_region_restore (sn : RSnap) (ps : PState) : PState :=
  {| pa := region_restore sn (pa ps); pinS := pinS ps |}.

(* The window lemma is generic in the set it protects, so instantiate it with
   the PIN set in place of the saved free-list: both are sets of addresses below
   the saved frontier that the window must not touch. *)
Definition pin_snap (ps : PState) : RSnap :=
  {| s_bump := bump (pa ps); s_free := pinS ps |}.

Lemma pins_untouched_by_a_window : forall ps stend,
  PINV ps -> steps (region_save (pa ps)) stend ->
  forall x, pinS ps x = true -> freeS stend x = false /\ liveS stend x = false.
Proof.
  intros ps stend HP Hst.
  assert (Hw : WinINV (pin_snap ps) stend).
  { apply (WinINV_steps (pin_snap ps) (region_save (pa ps)) stend).
    - destruct HP as [_ [_ [_ Hpb]]]. unfold snap_wf, pin_snap; simpl. exact Hpb.
    - exact Hst.
    - destruct HP as [[Hdis [Hbf Hbl]] [Hpf [Hpl Hpb]]].
      unfold WinINV, pin_snap, region_save; simpl. split; [ | split ].
      + unfold INV; simpl. split; [ | split ].
        * unfold disjoint, emptyS. intros x H. discriminate H.
        * unfold below, emptyS. intros x H. discriminate H.
        * exact Hbl.
      + lia.
      + intros x Hx. split; [ reflexivity | ].
        destruct (liveS (pa ps) x) eqn:El; [ | reflexivity ].
        exfalso. apply (Hpl x Hx El). }
  destruct Hw as [_ [_ Hs]]. exact Hs.
Qed.

(* SENTINEL 1+2: a region reset preserves the PIN invariant — the immortal host
   buffers survive the frontier reset intact, still off the free-list. *)
Theorem p_region_reset_preserves_PINV : forall ps stend,
  PINV ps -> steps (region_save (pa ps)) stend ->
  PINV (p_region_restore (snap_of (pa ps)) {| pa := stend; pinS := pinS ps |}).
Proof.
  intros ps stend HP Hst.
  assert (Hpins := pins_untouched_by_a_window ps stend HP Hst).
  destruct HP as [HINV [Hpf [Hpl Hpb]]].
  unfold PINV. split; [ | split; [ | split ] ].
  - apply (region_window_preserves_INV (pa ps) stend HINV Hst).
  - exact Hpf.
  - unfold p_region_restore, region_restore, snap_of, disjoint, maskS; simpl.
    intros x Hx Hl. destruct (Z.ltb x (bump (pa ps))); [ | discriminate Hl ].
    destruct (Hpins x Hx) as [_ Hl0]. rewrite Hl0 in Hl. discriminate Hl.
  - exact Hpb.
Qed.

Theorem p_region_reset_keeps_pins_unallocatable : forall ps stend p,
  PINV ps -> steps (region_save (pa ps)) stend -> pinS ps p = true ->
  p_alloc (p_region_restore (snap_of (pa ps)) {| pa := stend; pinS := pinS ps |}) p = None.
Proof.
  intros ps stend p HP Hst Hp.
  apply pinned_is_not_allocatable; [ | exact Hp ].
  apply (p_region_reset_preserves_PINV ps stend HP Hst).
Qed.

(* ── TEETH: the C-042 shape, run end to end ──────────────────────────────
   Pin block 0 (the preopen table), then churn around it: allocate block 1, free
   it (it DOES reach the free-list), reuse it. The pinned table is never handed
   out and never freed, throughout. *)
Definition p_init (b : Z) : PState := {| pa := init b; pinS := emptyS |}.

Lemma p_init_PINV : forall b, PINV (p_init b).
Proof.
  intros b. unfold PINV, p_init; simpl. split; [ apply init_INV | ].
  unfold disjoint, below, emptyS.
  split; [ intros x H; discriminate H | ].
  split; intros x H; discriminate H.
Qed.

Definition c0 : PState := p_init 0.
Definition c1 : PState :=
  {| pa := {| bump := 1; freeS := emptyS; liveS := emptyS |}; pinS := addS emptyS 0 |}.
Definition c2 : PState :=
  {| pa := {| bump := 2; freeS := emptyS; liveS := addS emptyS 1 |};
     pinS := addS emptyS 0 |}.
Definition c3 : PState :=
  {| pa := {| bump := 2; freeS := addS emptyS 1; liveS := remS (addS emptyS 1) 1 |};
     pinS := addS emptyS 0 |}.
Definition c4 : PState :=
  {| pa := {| bump := 2; freeS := remS (addS emptyS 1) 1;
              liveS := addS (remS (addS emptyS 1) 1) 1 |};
     pinS := addS emptyS 0 |}.

Example pinned_table_survives_churn :
  p_alloc_pinned c0 0 = Some c1 /\   (* __alloc_pinned: the preopen table *)
  p_alloc c1 1 = Some c2 /\          (* ordinary churn above it *)
  p_free c2 1 = Some c3 /\           (* the churn block DOES reach the free-list *)
  p_alloc c3 1 = Some c4 /\          (* … and IS reused *)
  p_alloc c4 0 = None /\             (* but the table is never handed out *)
  p_free c4 0 = None.                (* and never freed (the rc sentinel) *)
Proof. repeat split; reflexivity. Qed.

Example churn_around_a_pin_is_well_formed : PINV c4.
Proof.
  apply (p_steps_preserve_PINV c0 c4); [ | apply p_init_PINV ].
  apply (p_steps_pin _ 0 c1); [ reflexivity | ].
  apply (p_steps_alloc _ 1 c2); [ reflexivity | ].
  apply (p_steps_free _ 1 c3); [ reflexivity | ].
  apply (p_steps_alloc _ 1 c4); [ reflexivity | ].
  apply p_steps_refl.
Qed.

(* WHAT REMAINS TRUSTED here (#909's third bullet, deliberately NOT claimed):
   "reuse restores rc=1". This model tracks ADDRESSES — which block is free, live
   or pinned — not reference COUNTS, so the rc value a reused block starts life
   with is still guarded by the churn + byte gates and by `WasmExec`'s byte-level
   rc theorems, not by this file. *)

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
Print Assumptions pinned_is_never_freed.
Print Assumptions pinned_is_not_on_the_free_list.
Print Assumptions pinned_is_not_allocatable.
Print Assumptions p_alloc_not_pinned.
Print Assumptions p_steps_preserve_PINV.
Print Assumptions pins_are_immortal.
Print Assumptions pinned_stays_immortal_forever.
Print Assumptions p_region_reset_preserves_PINV.
Print Assumptions p_region_reset_keeps_pins_unallocatable.
