(* Almide v1 trust spine — #576 slice 5: the WHOLE-RUN composition.

   Slices 1-4 established, per operation and kernel-checked end to end:

     emitted bytes  ==decode==  instruction trees  ==realize==  memory
     transformations (`rt_inc`, `rt_dec`'s store, the free-list push and
     pop, the header writes).

   `FreeListRc.v` established, at the model level: arbitrary sequences of
   `r_new`/`r_inc`/`r_dec` preserve `RINV` — free blocks read count 0,
   live blocks read at least 1, free and live disjoint.

   This file joins the two: a run relation whose MEMORY evolution is the
   CONCRETE trees' output (`run_inc`/`run_dec`/`run_alloc` — the very
   terms the decode theorems bound to the emitter's bytes), coupled to
   FreeList's allocator state as GHOST bookkeeping, preserves `RINV` on
   the CONCRETE memory. The F-class (double-free, stale-count reuse,
   aliased handout) thereby becomes a violated lemma of the emitted
   code's semantics, not of a hand-built model.

   ── The coupling, stated honestly ─────────────────────────────────────
   * Each step carries the model transition as a PRECONDITION (the ghost
     `alloc`/`free_op` validates the runtime's choice; `liveS` guards
     inc/dec). This is the PCC framing shared by the whole spine: the
     verified-IR discipline — Perceus's `1 <= rc` certificate, the
     ownership checker's no-use-after-free — DISCHARGES these
     obligations; the theorem validates rather than assumes them.
   * The relation over-approximates the real machine: per-step frontier
     and page witnesses (`gh`, `pages`) are existential, so every real
     run is an instance and safety over the relation covers it.
   * The layout discipline is three NAMED section hypotheses, not
     axioms: block bases sit at or above the heap floor (`HBfloor`),
     block extents are at least 16 bytes and disjoint (`HBsep`), and
     the 16-slot class table sits wholly below the floor (`Htable` —
     concretely 48 + 64 <= 112 <= G_LINE_END). Every theorem below
     carries them as premises after the section closes.
   * Covered step shapes: inc, shared dec, unique dec (all three free
     outcomes: abandon-small, abandon-huge, file-by-class), classed
     pop, classed no-grow bump. The grow retry and the unclassed bump
     remain at slice 2's declared abstraction boundary. *)

From AlmideTrust Require Import FreeList RuntimeModel FreeListRc
     StructuralRuntime StructuralAlloc.
From Stdlib Require Import ZArith Lia List.
Import ListNotations.
Open Scope Z_scope.

(* RC_OFFSET = 0: the rc cell IS the base address. *)
Lemma read_rc_plain : forall m p, read_rc m p = m p.
Proof. intros m p. unfold read_rc, RC_OFFSET. f_equal. lia. Qed.

Lemma rc_at_plain : forall rs p, rc_at rs p = rmem rs p.
Proof. intros rs p. unfold rc_at. apply read_rc_plain. Qed.

Section Composition.

Variables floor fbase : Z.

(* The block-base discipline: `B` is the set of addresses the allocator
   ever hands out as block bases. *)
Variable B : Z -> bool.
Hypothesis HBfloor : forall b, B b = true -> floor <= b.
Hypothesis HBsep : forall b b', B b = true -> B b' = true -> b <> b' ->
                   b + 16 <= b' \/ b' + 16 <= b.
Hypothesis Htable : fbase + 64 <= floor.

(* Ghost-tracked blocks are disciplined bases. *)
Definition TB (a : AState) : Prop :=
  forall x, freeS a x = true \/ liveS a x = true -> B x = true.

Lemma TB_free_op : forall a p a',
  TB a -> free_op a p = Some a' -> TB a'.
Proof.
  intros a p a' HT Hf. unfold free_op in Hf.
  destruct (liveS a p) eqn:El; [ | discriminate ].
  injection Hf; intro E; subst a'.
  intros x Hx; cbn [freeS liveS] in Hx; unfold addS, remS in Hx.
  destruct (x =? p) eqn:Exp.
  - apply Z.eqb_eq in Exp; subst x. apply HT. right. exact El.
  - apply HT. exact Hx.
Qed.

Lemma TB_alloc : forall a p a',
  TB a -> B p = true -> alloc a p = Some a' -> TB a'.
Proof.
  intros a p a' HT HBp Ha. unfold alloc in Ha.
  destruct (Z.eqb_spec p (bump a)).
  - injection Ha; intro E; subst a'.
    intros x Hx; cbn [freeS liveS] in Hx; unfold addS in Hx.
    destruct (x =? p) eqn:Exp.
    + apply Z.eqb_eq in Exp; subst x. exact HBp.
    + apply HT. exact Hx.
  - destruct (freeS a p) eqn:Ef; [ | discriminate ].
    injection Ha; intro E; subst a'.
    intros x Hx; cbn [freeS liveS] in Hx; unfold addS, remS in Hx.
    destruct (x =? p) eqn:Exp.
    + apply Z.eqb_eq in Exp; subst x. exact HBp.
    + apply HT. exact Hx.
Qed.

(* rc cells live at bases; memories agreeing on every tracked base
   satisfy the same RINV. *)
Lemma RINV_mem_agree : forall a m m',
  (forall x, freeS a x = true \/ liveS a x = true -> m' x = m x) ->
  RINV {| ra := a ; rmem := m |} -> RINV {| ra := a ; rmem := m' |}.
Proof.
  intros a m m' Hag [Hi [Hf Hl]].
  split; [ exact Hi | split ].
  - intros x Hx. rewrite rc_at_plain; cbn.
    rewrite (Hag x (or_introl Hx)).
    specialize (Hf x Hx). rewrite rc_at_plain in Hf. exact Hf.
  - intros x Hx. rewrite rc_at_plain; cbn.
    rewrite (Hag x (or_intror Hx)).
    specialize (Hl x Hx). rewrite rc_at_plain in Hl. exact Hl.
Qed.

(* ══ THE CONCRETE RUN RELATION ═════════════════════════════════════════
   Memory moves by the PROVEN TREES; the allocator state is the ghost. *)

Inductive kstep : RState -> RState -> Prop :=
| k_inc : forall rs p,
    liveS (ra rs) p = true ->
    kstep rs {| ra := ra rs ;
                rmem := cm (run_inc p floor (mkC 0 0 0 (rmem rs))) |}
| k_dec_shared : forall rs p,
    liveS (ra rs) p = true ->
    2 <= rmem rs p ->
    kstep rs {| ra := ra rs ;
                rmem := cm (run_dec p floor fbase (mkC 0 0 0 (rmem rs))) |}
| k_dec_unique : forall rs p a',
    liveS (ra rs) p = true ->
    rmem rs p = 1 ->
    free_op (ra rs) p = Some a' ->
    kstep rs {| ra := a' ;
                rmem := cm (run_dec p floor fbase (mkC 0 0 0 (rmem rs))) |}
| k_new_pop : forall rs lenv gh pages w cl h a' v c',
    w = Z.land (lenv + 15) (-4) ->
    16 <= w ->
    cl = class_of w ->
    cl < 16 ->
    h = rmem rs (fbase + 4 * cl) ->
    h <> 0 ->
    B h = true ->
    alloc (ra rs) h = Some a' ->
    run_alloc lenv fbase (mkA 0 0 0 0 gh pages (rmem rs)) = ARet v c' ->
    kstep rs {| ra := a' ; rmem := am c' |}
| k_new_bump : forall rs lenv gh pages w cl a' v c',
    w = Z.land (lenv + 15) (-4) ->
    16 <= w ->
    cl = class_of w ->
    cl < 16 ->
    rmem rs (fbase + 4 * cl) = 0 ->
    0 <= lenv ->
    gh + 16 * 2 ^ cl <= Z.shiftl pages 16 ->
    B gh = true ->
    alloc (ra rs) gh = Some a' ->
    run_alloc lenv fbase (mkA 0 0 0 0 gh pages (rmem rs)) = ARet v c' ->
    kstep rs {| ra := a' ; rmem := am c' |}.

Definition KINV (rs : RState) : Prop := RINV rs /\ TB (ra rs).

(* ── inc ── *)
Lemma k_inc_preserves : forall rs p,
  liveS (ra rs) p = true ->
  KINV rs ->
  KINV {| ra := ra rs ;
          rmem := cm (run_inc p floor (mkC 0 0 0 (rmem rs))) |}.
Proof.
  intros rs p Hl [HR HT].
  assert (HBp : B p = true) by (apply HT; right; exact Hl).
  assert (Hfp : floor <= p) by (apply HBfloor; exact HBp).
  assert (Hm : cm (run_inc p floor (mkC 0 0 0 (rmem rs)))
               = rt_inc (rmem rs) p)
    by (exact (inc_realizes_rt_inc p floor (mkC 0 0 0 (rmem rs)) Hfp)).
  rewrite Hm.
  split; [ | exact HT ].
  apply (r_inc_preserves_RINV rs p _ HR).
  unfold r_inc. rewrite Hl. reflexivity.
Qed.

(* ── dec, shared (rc >= 2) ── *)
Lemma k_dec_shared_preserves : forall rs p,
  liveS (ra rs) p = true ->
  2 <= rmem rs p ->
  KINV rs ->
  KINV {| ra := ra rs ;
          rmem := cm (run_dec p floor fbase (mkC 0 0 0 (rmem rs))) |}.
Proof.
  intros rs p Hl Hrc [HR HT].
  assert (HBp : B p = true) by (apply HT; right; exact Hl).
  assert (Hfp : floor <= p) by (apply HBfloor; exact HBp).
  destruct (dec_shared_realizes_rt_dec p floor fbase
              (mkC 0 0 0 (rmem rs)) Hfp Hrc) as [_ Hmem].
  rewrite Hmem.
  split; [ | exact HT ].
  apply (r_dec_preserves_RINV rs p _ HR).
  rewrite (r_dec_pos rs p) by (rewrite rc_at_plain; lia).
  rewrite rc_at_plain.
  replace (rmem rs p =? 1) with false
    by (symmetry; apply Z.eqb_neq; lia).
  replace (p + RC_OFFSET) with p by (unfold RC_OFFSET; lia).
  reflexivity.
Qed.

(* ── dec, unique (rc = 1): the release, through all three of free's
   outcomes ── *)
Lemma k_dec_unique_preserves : forall rs p a',
  liveS (ra rs) p = true ->
  rmem rs p = 1 ->
  free_op (ra rs) p = Some a' ->
  KINV rs ->
  KINV {| ra := a' ;
          rmem := cm (run_dec p floor fbase (mkC 0 0 0 (rmem rs))) |}.
Proof.
  intros rs p a' Hl Hrc Hfo [HR HT].
  assert (HBp : B p = true) by (apply HT; right; exact Hl).
  assert (Hfp : floor <= p) by (apply HBfloor; exact HBp).
  (* the model step lands on `upd m p 0` *)
  assert (Hmodel : r_dec rs p = Some {| ra := a' ;
                                        rmem := upd (rmem rs) p 0 |}).
  { rewrite (r_dec_pos rs p) by (rewrite rc_at_plain; lia).
    rewrite rc_at_plain. rewrite Hrc.
    replace (1 =? 1) with true by reflexivity.
    rewrite Hfo.
    replace (p + RC_OFFSET) with p by (unfold RC_OFFSET; lia).
    replace (1 - 1) with 0 by lia.
    reflexivity. }
  assert (HRmodel : RINV {| ra := a' ; rmem := upd (rmem rs) p 0 |})
    by (exact (r_dec_preserves_RINV rs p _ HR Hmodel)).
  assert (HTa' : TB a') by (exact (TB_free_op (ra rs) p a' HT Hfo)).
  (* the concrete step hands the zeroed state to `$free` *)
  rewrite (dec_unique_hands_to_free p floor fbase
             (mkC 0 0 0 (rmem rs)) Hfp Hrc).
  cbn [ctot ccls ctmp cm].
  set (m0 := upd (rmem rs) p 0).
  set (t := Z.land (m0 (p + 8) + 15) (-4)).
  split; [ | exact HTa' ].
  destruct (Z.ltb t 16) eqn:Et.
  - (* abandon: too small to file *)
    apply Z.ltb_lt in Et.
    rewrite (free_abandons_small p floor fbase (mkC 0 0 0 m0) Et).
    exact HRmodel.
  - apply Z.ltb_ge in Et.
    destruct (Z.ltb (class_of t) 16) eqn:Ec.
    + (* the filing: two extra stores, both off every tracked base *)
      apply Z.ltb_lt in Ec.
      rewrite (free_files_by_class p floor fbase (mkC 0 0 0 m0) t
                 eq_refl Et Ec).
      cbv zeta.
      apply (RINV_mem_agree a' m0 _); [ | exact HRmodel ].
      intros x Hx.
      assert (HBx : B x = true) by (apply HTa'; exact Hx).
      assert (Hfx : floor <= x) by (apply HBfloor; exact HBx).
      assert (Hcl0 : 0 <= class_of t) by (apply class_nonneg; exact Et).
      unfold upd.
      destruct (Z.eqb_spec x (fbase + 4 * class_of t)) as [Ex|_];
        [ exfalso; lia | ].
      destruct (Z.eqb_spec x (p + 12)) as [Ex|_]; [ exfalso | reflexivity ].
      destruct (Z.eq_dec x p) as [->|Hne]; [ lia | ].
      destruct (HBsep p x HBp HBx ltac:(congruence)); lia.
    + (* abandon: class overflows the table *)
      apply Z.ltb_ge in Ec.
      rewrite (free_abandons_huge p floor fbase (mkC 0 0 0 m0) t
                 eq_refl Et Ec).
      exact HRmodel.
Qed.

(* ── alloc, the pop: the reuse cycle's other half ── *)
Lemma k_new_pop_preserves : forall rs lenv gh pages w cl h a' v c',
  w = Z.land (lenv + 15) (-4) ->
  16 <= w ->
  cl = class_of w ->
  cl < 16 ->
  h = rmem rs (fbase + 4 * cl) ->
  h <> 0 ->
  B h = true ->
  alloc (ra rs) h = Some a' ->
  run_alloc lenv fbase (mkA 0 0 0 0 gh pages (rmem rs)) = ARet v c' ->
  KINV rs ->
  KINV {| ra := a' ; rmem := am c' |}.
Proof.
  intros rs lenv gh pages w cl h a' v c'
         Hw H16 Hcl Hcl16 Hh Hnz HBh Halloc Hrun [HR HT].
  rewrite (alloc_pops_filed_head lenv fbase
             (mkA 0 0 0 0 gh pages (rmem rs)) w cl (fbase + 4 * cl) h
             Hw H16 Hcl Hcl16 eq_refl Hh Hnz) in Hrun.
  (* project the memory field out of the aout — injection would numeral-
     normalize the record; a projector keeps `4 * cl` folded *)
  assert (Hmem := f_equal
    (fun o => match o with AFall c0 => am c0 | ARet _ c0 => am c0 end) Hrun).
  cbn [am agh apages abase anext awant ahead] in Hmem.
  rewrite <- Hmem.
  assert (Hmodel : r_new rs h = Some {| ra := a' ;
                                        rmem := upd (rmem rs) h 1 |}).
  { unfold r_new, r_alloc. rewrite Halloc.
    unfold rc_init; cbn [ra rmem].
    replace (h + RC_OFFSET) with h by (unfold RC_OFFSET; lia).
    reflexivity. }
  assert (HRmodel := r_new_preserves_RINV rs h _ HR Hmodel).
  assert (HTa' : TB a') by (exact (TB_alloc (ra rs) h a' HT HBh Halloc)).
  split; [ | exact HTa' ].
  apply (RINV_mem_agree a' (upd (rmem rs) h 1) _); [ | exact HRmodel ].
  intros x Hx.
  assert (HBx : B x = true) by (apply HTa'; exact Hx).
  assert (Hfx : floor <= x) by (apply HBfloor; exact HBx).
  assert (Hcl0 : 0 <= cl)
    by (rewrite Hcl; apply class_nonneg; exact H16).
  unfold upd.
  destruct (Z.eq_dec x h) as [->|Hne].
  - destruct (Z.eqb_spec h (h + 8)) as [E|_]; [ exfalso; lia | ].
    destruct (Z.eqb_spec h (h + 4)) as [E|_]; [ exfalso; lia | ].
    rewrite Z.eqb_refl. reflexivity.
  - assert (Hsep : h + 16 <= x \/ x + 16 <= h)
      by (apply (HBsep h x HBh HBx); congruence).
    destruct (Z.eqb_spec x (h + 8)) as [E|_]; [ exfalso; lia | ].
    destruct (Z.eqb_spec x (h + 4)) as [E|_]; [ exfalso; lia | ].
    destruct (Z.eqb_spec x h) as [E|_]; [ exfalso; lia | ].
    destruct (Z.eqb_spec x (fbase + 4 * cl)) as [E|_];
      [ exfalso; lia | reflexivity ].
Qed.

(* ── alloc, the classed no-grow bump ── *)
Lemma k_new_bump_preserves : forall rs lenv gh pages w cl a' v c',
  w = Z.land (lenv + 15) (-4) ->
  16 <= w ->
  cl = class_of w ->
  cl < 16 ->
  rmem rs (fbase + 4 * cl) = 0 ->
  0 <= lenv ->
  gh + 16 * 2 ^ cl <= Z.shiftl pages 16 ->
  B gh = true ->
  alloc (ra rs) gh = Some a' ->
  run_alloc lenv fbase (mkA 0 0 0 0 gh pages (rmem rs)) = ARet v c' ->
  KINV rs ->
  KINV {| ra := a' ; rmem := am c' |}.
Proof.
  intros rs lenv gh pages w cl a' v c'
         Hw H16 Hcl Hcl16 Hempty Hlen Hfit HBg Halloc Hrun [HR HT].
  rewrite (alloc_bumps_fresh_classed lenv fbase
             (mkA 0 0 0 0 gh pages (rmem rs)) w cl
             Hw H16 Hcl Hcl16 Hempty Hlen Hfit) in Hrun.
  assert (Hmem := f_equal
    (fun o => match o with AFall c0 => am c0 | ARet _ c0 => am c0 end) Hrun).
  cbn [am agh apages abase anext awant ahead] in Hmem.
  rewrite <- Hmem.
  assert (Hmodel : r_new rs gh = Some {| ra := a' ;
                                         rmem := upd (rmem rs) gh 1 |}).
  { unfold r_new, r_alloc. rewrite Halloc.
    unfold rc_init; cbn [ra rmem].
    replace (gh + RC_OFFSET) with gh by (unfold RC_OFFSET; lia).
    reflexivity. }
  assert (HRmodel := r_new_preserves_RINV rs gh _ HR Hmodel).
  assert (HTa' : TB a') by (exact (TB_alloc (ra rs) gh a' HT HBg Halloc)).
  split; [ | exact HTa' ].
  apply (RINV_mem_agree a' (upd (rmem rs) gh 1) _); [ | exact HRmodel ].
  intros x Hx.
  assert (HBx : B x = true) by (apply HTa'; exact Hx).
  unfold upd.
  destruct (Z.eq_dec x gh) as [->|Hne].
  - destruct (Z.eqb_spec gh (gh + 8)) as [E|_]; [ exfalso; lia | ].
    destruct (Z.eqb_spec gh (gh + 4)) as [E|_]; [ exfalso; lia | ].
    rewrite Z.eqb_refl. reflexivity.
  - assert (Hsep : gh + 16 <= x \/ x + 16 <= gh)
      by (apply (HBsep gh x HBg HBx); congruence).
    destruct (Z.eqb_spec x (gh + 8)) as [E|_]; [ exfalso; lia | ].
    destruct (Z.eqb_spec x (gh + 4)) as [E|_]; [ exfalso; lia | ].
    destruct (Z.eqb_spec x gh) as [E|_];
      [ exfalso; lia | reflexivity ].
Qed.

(* ══ THE COMPOSITION ═══════════════════════════════════════════════════ *)

Theorem kstep_preserves_KINV : forall rs rs',
  kstep rs rs' -> KINV rs -> KINV rs'.
Proof.
  intros rs rs' Hs HK.
  destruct Hs.
  - apply k_inc_preserves; assumption.
  - apply k_dec_shared_preserves; assumption.
  - apply (k_dec_unique_preserves rs p a'); assumption.
  - apply (k_new_pop_preserves rs lenv gh pages w cl h a' v c'); assumption.
  - apply (k_new_bump_preserves rs lenv gh pages w cl a' v c'); assumption.
Qed.

Inductive ksteps : RState -> RState -> Prop :=
| ksteps_refl : forall rs, ksteps rs rs
| ksteps_step : forall rs rs' rs'',
    kstep rs rs' -> ksteps rs' rs'' -> ksteps rs rs''.

Theorem structural_runs_preserve_RINV : forall rs rs',
  ksteps rs rs' -> KINV rs -> KINV rs'.
Proof.
  intros rs rs' Hst. induction Hst as [ rs0 | rs0 rs1 rs2 Hs _ IH ];
    intro HK.
  - exact HK.
  - apply IH. exact (kstep_preserves_KINV rs0 rs1 Hs HK).
Qed.

Theorem structural_boot_KINV : forall b, KINV (r_init b).
Proof.
  intro b. split; [ apply r_init_RINV | ].
  intros x [Hx|Hx]; discriminate Hx.
Qed.

(* ══ THE F-CLASS, AS VIOLATED LEMMAS OF THE EMITTED CODE ═══════════════
   Over ANY structural run from boot — however many allocations, shares
   and releases the program performed through the emitted trees: *)

(* No aliased handout: a block the ghost validates for allocation is
   never currently live (reuse-after-free unrepresentable), and if it
   comes off the free-list it reads count 0 through the CONCRETE stores
   (no stale count survives the trip). *)
Theorem structural_no_aliased_handout : forall b rs p a',
  ksteps (r_init b) rs ->
  alloc (ra rs) p = Some a' ->
  liveS (ra rs) p = false
  /\ (freeS (ra rs) p = true -> rmem rs p = 0).
Proof.
  intros b rs p a' Hst Ha.
  destruct (structural_runs_preserve_RINV _ _ Hst (structural_boot_KINV b))
    as [[Hi [Hf _]] _].
  split.
  - exact (alloc_not_live (ra rs) p a' Hi Ha).
  - intro Hfr. specialize (Hf p Hfr). rewrite rc_at_plain in Hf. exact Hf.
Qed.

(* Counts stay honest: every filed block reads 0, every live block reads
   at least 1 — the double-free and the lost-reference shapes both
   violate this lemma. *)
Theorem structural_counts_stay_honest : forall b rs,
  ksteps (r_init b) rs ->
  (forall x, freeS (ra rs) x = true -> rmem rs x = 0)
  /\ (forall x, liveS (ra rs) x = true -> 1 <= rmem rs x).
Proof.
  intros b rs Hst.
  destruct (structural_runs_preserve_RINV _ _ Hst (structural_boot_KINV b))
    as [[_ [Hf Hl]] _].
  split; intros x Hx.
  - specialize (Hf x Hx). rewrite rc_at_plain in Hf. exact Hf.
  - specialize (Hl x Hx). rewrite rc_at_plain in Hl. exact Hl.
Qed.

End Composition.
