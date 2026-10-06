(* Almide v1 trust spine — #576 slice 5: the WHOLE-RUN composition, with
   the large-block free list (#3348).

   Slices 1-4 established, per operation and kernel-checked end to end:

     emitted bytes  ==decode==  instruction trees  ==realize==  memory
     transformations (`rt_inc`, `rt_dec`'s store, the free-list push and
     pop, the header writes) — and LargeTree.v adds the two loop trees
     (`$lfree` / `$ltake`, inlined into `$free` / `$alloc` above the
     64 KiB class ceiling) realizing LargeList.v's coalescing insert and
     first-fit take with split.

   This file joins them: a run relation whose MEMORY evolution is the
   CONCRETE trees' output (`run_inc`/`run_dec`/`run_alloc` — the very
   terms the decode theorems bind to the emitter's bytes), coupled to two
   GHOST states — FreeList's point model for the class-sized blocks and
   LargeList's extent model (free list, live extents, frontier) for the
   large ones — preserves one invariant `KINV` on the CONCRETE memory:

     * small blocks: RINV (filed blocks read count 0, live ones at least 1,
       free and live disjoint);
     * large blocks: LINV (the free list is gapped, coalesced and below
       the frontier; live extents pairwise disjoint and disjoint from the
       free list), the in-memory list REPRESENTS the ghost free list
       (`lrep`, head at address 12), and every live extent reads count
       at least 1;
     * SEPARATION, as a state invariant rather than a static axiom: every
       small block's 16-byte header window lies below the frontier and
       outside every large extent, and distinct small blocks' windows do
       not overlap.

   The earlier revision assumed separation as a section hypothesis
   (`HBsep`: every base the allocator EVER hands out is 16 apart from
   every other). Splitting and coalescing make that false — a split
   remainder's base is new, a coalesced extent swallows old bases — so
   the hypothesis is gone; the window invariant above is proven to hold
   across every step instead. The small fresh bump no longer needs the
   point model's counter to coincide with the address frontier either:
   it takes the block at the frontier and the ghost records it
   (`fresh`), so consecutive bumps are representable.

   ── The coupling, stated honestly ─────────────────────────────────────
   * Each step carries its model transition as a PRECONDITION where the
     runtime cannot check it (liveness of the released block, the block's
     cap encoding its extent) — the PCC framing shared by the whole
     spine: the ownership certificate DISCHARGES these obligations.
   * The page count is a per-step witness bounded by the wasm32 limit
     (65536 pages); every bump step carries the no-grow fit.
   * Layout hypotheses: the heap floor is at least 16 (the sentinel's
     size word 4 and the list head 12 sit below it) and the 16-slot class
     table sits wholly below the floor (concretely 48 + 64 <= 112). *)

From AlmideTrust Require Import FreeList RuntimeModel FreeListRc
     StructuralRuntime StructuralAlloc LargeList LargeTree.
From Stdlib Require Import ZArith Lia List.
Import ListNotations.
Open Scope Z_scope.

(* RC_OFFSET = 0: the rc cell IS the base address. *)
Lemma read_rc_plain : forall m p, read_rc m p = m p.
Proof. intros m p. unfold read_rc, RC_OFFSET. f_equal. lia. Qed.

Lemma rc_at_plain : forall rs p, rc_at rs p = rmem rs p.
Proof. intros rs p. unfold rc_at. apply read_rc_plain. Qed.

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

(* `Z.land x (-4)` rounds down to a multiple of 4. *)
Lemma land_m4 : forall x, Z.land x (-4) = x - x mod 4.
Proof.
  intros x. replace (-4) with (Z.lnot 3) by reflexivity.
  rewrite <- Z.sub_land_same_l.
  replace 3 with (Z.ones 2) by reflexivity. rewrite Z.land_ones by lia. reflexivity.
Qed.

(* An aligned base shifts out of the rounding: the bump's
   `(g + 12 + len + 3) & -4` is `g + want`. *)
Lemma land_m4_shift : forall g x, g mod 4 = 0 ->
  Z.land (g + 12 + x + 3) (-4) = g + Z.land (x + 15) (-4).
Proof.
  intros g x Hg. rewrite !land_m4.
  replace (g + 12 + x + 3) with (g + (x + 15)) by lia.
  rewrite Z.add_mod by lia. rewrite Hg, Z.add_0_l, Z.mod_mod by lia. lia.
Qed.

Section Composition.

Variables floor fbase : Z.
Hypothesis Hfloor : 16 <= floor.
Hypothesis Hfb : 16 <= fbase.
Hypothesis Htable : fbase + 64 <= floor.

Definition tracked (a : AState) (x : Z) : Prop :=
  freeS a x = true \/ liveS a x = true.

(* The combined state: concrete memory with the point ghost, the large
   free list, the live large extents, and the bump frontier. *)
Record KState := mkK { krs : RState; kfl : list (Z * Z); kll : list (Z * Z); khp : Z }.

Definition lst (k : KState) : LState := {| heap := khp k; fl := kfl k; live := kll k |}.

Definition LMEM (m : Mem) (f l : list (Z * Z)) : Prop :=
  m 4 = 0 /\ lrep m (m LHEAD) f /\ Forall (fun e => 1 <= m (fst e)) l.

Record KINV (k : KState) : Prop := {
  ki_rinv : RINV (krs k);
  ki_small : forall x, tracked (ra (krs k)) x ->
               floor <= x /\ x + 16 <= khp k /\
               (forall a, x <= a < x + 16 -> ~ cov (kfl k ++ kll k) a);
  ki_sep : forall x y, tracked (ra (krs k)) x -> tracked (ra (krs k)) y -> x <> y ->
             x + 16 <= y \/ y + 16 <= x;
  ki_linv : LINV floor (lst k);
  ki_lmem : LMEM (rmem (krs k)) (kfl k) (kll k);
  ki_live16 : Forall (fun e => 16 <= snd e) (kll k);
  ki_hal : khp k mod 4 = 0;
  ki_fal : Forall al4 (kfl k);
  ki_lal : Forall al4 (kll k);
  ki_top : khp k <= MEMTOP;
  ki_floor : floor <= khp k
}.

(* ── shared facts ── *)

Lemma cov_In : forall l e a, In e l -> inside e a -> cov l a.
Proof. intros l e a Hin H. apply Exists_exists. exists e. split; assumption. Qed.

Lemma base_cov : forall l p s, In (p, s) l -> 0 < s -> cov l p.
Proof. intros l p s Hin Hs. apply (cov_In l (p, s)); [ exact Hin | unfold inside; cbn; lia ]. Qed.

Lemma small_off_large : forall k x a, KINV k -> tracked (ra (krs k)) x ->
  x <= a < x + 16 -> ~ cov (kfl k) a /\ ~ cov (kll k) a.
Proof.
  intros k x a HK Hx Ha. destruct (ki_small k HK x Hx) as [_ [_ Hw]].
  specialize (Hw a Ha). split; intros Hc; apply Hw, cov_app; [ left | right ]; exact Hc.
Qed.

Lemma linv_parts : forall k, KINV k ->
  gaps (kfl k) /\ Forall (fun e => floor <= fst e) (kfl k) /\ ends_below (kfl k) (khp k) /\
  Forall (fun e => floor <= fst e /\ 0 < snd e /\ fst e + snd e <= khp k) (kll k) /\
  pdisj (kll k) /\ (forall x, cov (kfl k) x -> cov (kll k) x -> False).
Proof.
  intros k HK. destruct (ki_linv k HK) as [[Hg Hlo] [Heb [Hlv [Hpd Hd]]]]. cbn in *. tauto.
Qed.

Lemma fl_ge16 : forall k, KINV k -> Forall (fun y => 16 <= fst y) (kfl k).
Proof.
  intros k HK. destruct (linv_parts k HK) as [_ [Hlo _]].
  eapply Forall_impl; [ | exact Hlo ]. intros a Ha. cbn in *. pose proof Hfloor. lia.
Qed.

Lemma fl_top : forall k, KINV k -> ends_below (kfl k) MEMTOP.
Proof.
  intros k HK. destruct (linv_parts k HK) as [_ [_ [Heb _]]].
  apply (ends_below_mono _ (khp k)); [ exact Heb | exact (ki_top k HK) ].
Qed.

Lemma cov_lo : forall k a, KINV k -> cov (kfl k) a \/ cov (kll k) a -> floor <= a < khp k.
Proof.
  intros k a HK [Hc | Hc]; destruct (linv_parts k HK) as [Hg [Hlo [Heb [Hlv _]]]].
  - split; [ exact (Forall_lo_cov _ _ _ Hlo Hc) | exact (cov_below _ _ _ Heb Hc) ].
  - exact (live_cov_below _ _ _ _ Hlv Hc).
Qed.

(* Memory changed only off the large structures keeps LMEM. *)
Lemma LMEM_frame : forall m m' f l,
  LMEM m f l -> gaps f -> Forall (fun e => 16 <= snd e) l ->
  (forall a, a = 4 \/ a = LHEAD \/ cov f a \/ cov l a -> m' a = m a) ->
  LMEM m' f l.
Proof.
  intros m m' f l [H4 [Hrep Hrc]] Hg H16 Hag.
  split; [ rewrite Hag by auto; exact H4 | split ].
  - rewrite (Hag LHEAD) by auto. apply (lrep_frame f m); [ | exact Hrep ].
    intros a Ha. apply Hag. right; right; left. exact (field_cov f a Hg Ha).
  - rewrite Forall_forall in Hrc |- *. intros [p s] Hin. cbn.
    rewrite Forall_forall in H16. specialize (H16 _ Hin). cbn in H16.
    rewrite Hag; [ exact (Hrc _ Hin) | right; right; right; exact (base_cov l p s Hin ltac:(lia)) ].
Qed.

Lemma upd_off : forall m a v x, x <> a -> upd m a v x = m x.
Proof. intros m a v x H. unfold upd. destruct (Z.eqb_spec x a); [ contradiction | reflexivity ]. Qed.

(* The fresh small allocation at the frontier, recorded in the ghost. *)
Definition fresh (a : AState) (p : Z) : AState :=
  {| bump := Z.max (bump a) (p + 1); freeS := freeS a; liveS := addS (liveS a) p |}.

Lemma rt_inc_plain : forall m p, rt_inc m p = upd m p (m p + 1).
Proof. intros m p. unfold rt_inc, read_rc, RC_OFFSET. rewrite Z.add_0_r. reflexivity. Qed.

Definition setmem (k : KState) (a : AState) (m : Mem) : KState :=
  mkK {| ra := a; rmem := m |} (kfl k) (kll k) (khp k).

(* ── tracked-set bookkeeping for the point ghost ── *)

Lemma tracked_free_op : forall a p a' x,
  free_op a p = Some a' -> (tracked a' x <-> tracked a x).
Proof.
  intros a p a' x Hf. unfold free_op in Hf.
  destruct (liveS a p) eqn:El; [ | discriminate ]. injection Hf as <-.
  unfold tracked; cbn [freeS liveS]; unfold addS, remS.
  destruct (Z.eqb_spec x p) as [-> | _]; [ | tauto ].
  split; intros _; [ right; exact El | left; reflexivity ].
Qed.

Lemma tracked_pop : forall a h a' x,
  INV a -> freeS a h = true -> alloc a h = Some a' -> (tracked a' x <-> tracked a x).
Proof.
  intros a h a' x [_ [Hbf _]] Hfr Ha. unfold alloc in Ha.
  destruct (Z.eqb_spec h (bump a)) as [E | _].
  - exfalso. specialize (Hbf h Hfr). lia.
  - rewrite Hfr in Ha. injection Ha as <-.
    unfold tracked; cbn [freeS liveS]; unfold addS, remS.
    destruct (Z.eqb_spec x h) as [-> | _]; [ | tauto ].
    split; intros _; [ left; exact Hfr | right; reflexivity ].
Qed.

Lemma tracked_fresh : forall a p x, tracked (fresh a p) x <-> x = p \/ tracked a x.
Proof.
  intros a p x. unfold tracked, fresh; cbn [freeS liveS]; unfold addS.
  destruct (Z.eqb_spec x p) as [-> | Hne];
    [ split; intros _; [ left; reflexivity | right; reflexivity ] | ].
  split; [ tauto | intros [H | H]; [ contradiction | exact H ] ].
Qed.

Lemma fresh_RINV : forall a m m' p,
  RINV {| ra := a; rmem := m |} -> ~ tracked a p -> m' p = 1 ->
  (forall x, tracked a x -> m' x = m x) ->
  RINV {| ra := fresh a p; rmem := m' |}.
Proof.
  intros a m m' p [[Hdis [Hbf Hbl]] [Hf Hl]] Hnt Hp Hag.
  cbn [ra rmem] in *. split; [ split; [ | split ] | split ].
  - intros x Hx Hx'. cbn [ra rmem freeS liveS bump fresh] in Hx, Hx'. unfold addS in Hx'.
    destruct (Z.eqb_spec x p) as [E | E]; [ subst x | ].
    + apply Hnt. left. exact Hx.
    + exact (Hdis x Hx Hx').
  - intros x Hx. cbn [ra rmem freeS liveS bump fresh] in *. specialize (Hbf x Hx). lia.
  - unfold below. intros y Hy. cbn [ra rmem freeS liveS bump fresh] in *. unfold addS in Hy.
    destruct (Z.eqb_spec y p) as [E | E]; [ subst y; lia | ].
    specialize (Hbl y Hy). lia.
  - intros x Hx. cbn [ra rmem freeS liveS bump fresh] in Hx. rewrite rc_at_plain. cbn [rmem].
    rewrite (Hag x (or_introl Hx)). specialize (Hf x Hx). rewrite rc_at_plain in Hf. exact Hf.
  - intros x Hx. cbn [ra rmem freeS liveS bump fresh] in Hx. unfold addS in Hx. rewrite rc_at_plain. cbn [rmem].
    destruct (Z.eqb_spec x p) as [E | E]; [ subst x; lia | ].
    rewrite (Hag x (or_intror Hx)). specialize (Hl x Hx). rewrite rc_at_plain in Hl. exact Hl.
Qed.

Lemma RINV_eta : forall rs, RINV rs -> RINV {| ra := ra rs; rmem := rmem rs |}.
Proof. intros [a m] H. exact H. Qed.

(* ── one-cell writes: inc and the shared dec ── *)

(* A small block's own count changes; nothing large can notice. *)
Lemma small_cell_ok : forall k p v,
  KINV k -> tracked (ra (krs k)) p ->
  RINV {| ra := ra (krs k); rmem := upd (rmem (krs k)) p v |} ->
  KINV (setmem k (ra (krs k)) (upd (rmem (krs k)) p v)).
Proof.
  intros k p v HK Ht HR.
  destruct (small_off_large k p p HK Ht ltac:(lia)) as [Hnf Hnl].
  destruct (ki_small k HK p Ht) as [Hfp _].
  pose proof Hfloor. pose proof (proj1 (linv_parts k HK)) as Hg.
  destruct HK. constructor; cbn [setmem krs kfl kll khp ra rmem]; try assumption.
  apply (LMEM_frame (rmem (krs k))); [ assumption | exact Hg | assumption | ].
  intros a Ha. apply upd_off. intros ->. unfold LHEAD in Ha.
  destruct Ha as [Ha | [Ha | [Ha | Ha]]]; [ lia | lia | exact (Hnf Ha) | exact (Hnl Ha) ].
Qed.

(* A large block's own count changes (to at least 1); nothing small and
   no free node can notice. *)
Lemma large_cell_ok : forall k p s v,
  KINV k -> In (p, s) (kll k) -> 1 <= v ->
  KINV (setmem k (ra (krs k)) (upd (rmem (krs k)) p v)).
Proof.
  intros k p s v HK Hin Hv.
  destruct (linv_parts k HK) as [Hg [_ [_ [Hlv [_ Hd]]]]].
  assert (Hs : 16 <= s) by (pose proof (ki_live16 k HK) as H; rewrite Forall_forall in H; exact (H _ Hin)).
  assert (Hpc : cov (kll k) p) by (apply (base_cov _ p s Hin); lia).
  assert (Hpf : floor <= p) by (pose proof (cov_lo k p HK (or_intror Hpc)); lia).
  pose proof Hfloor.
  destruct (ki_lmem k HK) as [H4 [Hrep Hrc]].
  constructor; cbn [setmem krs kfl kll khp ra rmem]; try (destruct HK; assumption).
  - apply (RINV_mem_agree _ (rmem (krs k))); [ | exact (RINV_eta _ (ki_rinv k HK)) ].
    intros x Hx. apply upd_off. intros ->.
    destruct (small_off_large k p p HK Hx ltac:(lia)) as [_ Hnl]. exact (Hnl Hpc).
  - split; [ rewrite upd_off by lia; exact H4 | split ].
    + unfold LHEAD. rewrite upd_off by lia. apply (lrep_frame (kfl k) (rmem (krs k))); [ | exact Hrep ].
      intros a Ha. apply upd_off. intros ->. apply (Hd p); [ exact (field_cov _ _ Hg Ha) | exact Hpc ].
    + rewrite Forall_forall in Hrc |- *. intros [q z] Hq. cbn. unfold upd.
      destruct (Z.eqb_spec q p); [ exact Hv | exact (Hrc _ Hq) ].
Qed.

(* A small-world step: the tracked set is unchanged (a block moved
   between free and live), the counts are RINV, and every write missed
   the large structures. *)
Lemma kinv_frame : forall k a' M,
  KINV k ->
  (forall x, tracked a' x <-> tracked (ra (krs k)) x) ->
  RINV {| ra := a'; rmem := M |} ->
  (forall a, a = 4 \/ a = LHEAD \/ cov (kfl k) a \/ cov (kll k) a -> M a = rmem (krs k) a) ->
  KINV (setmem k a' M).
Proof.
  intros k a' M HK Htr HR Hag.
  pose proof (proj1 (linv_parts k HK)) as Hg.
  constructor; cbn [setmem krs kfl kll khp ra rmem]; try (destruct HK; assumption).
  - intros x Hx. apply (ki_small k HK x). apply Htr. exact Hx.
  - intros x y Hx Hy. apply (ki_sep k HK); apply Htr; assumption.
  - apply (LMEM_frame (rmem (krs k))); [ exact (ki_lmem k HK) | exact Hg | exact (ki_live16 k HK) | exact Hag ].
Qed.

(* Writes inside a small block's window or the class table miss every
   large structure. *)
Lemma small_write_misses : forall k p a,
  KINV k -> tracked (ra (krs k)) p ->
  a = 4 \/ a = LHEAD \/ cov (kfl k) a \/ cov (kll k) a ->
  forall w, (p <= w < p + 16) \/ (fbase <= w < floor) -> a <> w.
Proof.
  intros k p a HK Ht Ha w Hw ->. pose proof Hfloor. pose proof Hfb. unfold LHEAD in Ha.
  destruct Hw as [Hw | Hw].
  - destruct (ki_small k HK p Ht) as [Hfp _].
    destruct (small_off_large k p w HK Ht Hw) as [Hnf Hnl].
    destruct Ha as [Ha | [Ha | [Ha | Ha]]]; [ lia | lia | exact (Hnf Ha) | exact (Hnl Ha) ].
  - destruct Ha as [Ha | [Ha | Ha]]; [ lia | lia | ].
    pose proof (cov_lo k w HK Ha). lia.
Qed.

(* ── dec to zero, small block: abandon or file by class ── *)
Lemma k_dec_unique_small_ok : forall k p a',
  KINV k -> liveS (ra (krs k)) p = true -> rmem (krs k) p = 1 ->
  free_op (ra (krs k)) p = Some a' ->
  Z.land (rmem (krs k) (p + 8) + 15) (-4) < 16 \/
  class_of (Z.land (rmem (krs k) (p + 8) + 15) (-4)) < 13 ->
  KINV (setmem k a' (cm (run_dec p floor fbase (mkC 0 0 0 (rmem (krs k)))))).
Proof.
  intros k p a' HK Hl Hrc Hfo Hcls.
  assert (Ht : tracked (ra (krs k)) p) by (right; exact Hl).
  destruct (ki_small k HK p Ht) as [Hfp _].
  pose proof Hfloor. pose proof Hfb. pose proof Htable.
  set (m := rmem (krs k)) in *.
  assert (HRmodel : RINV {| ra := a'; rmem := upd m p 0 |}).
  { apply (r_dec_preserves_RINV (krs k) p _ (ki_rinv k HK)).
    rewrite (r_dec_pos (krs k) p) by (rewrite rc_at_plain; fold m; lia).
    rewrite rc_at_plain. fold m. rewrite Hrc.
    replace (1 =? 1) with true by reflexivity. rewrite Hfo.
    replace (p + RC_OFFSET) with p by (unfold RC_OFFSET; lia).
    replace (1 - 1) with 0 by lia. reflexivity. }
  rewrite (dec_unique_hands_to_free p floor fbase (mkC 0 0 0 m) Hfp Hrc).
  cbn [ctot ccls ctmp cm].
  set (m0 := upd m p 0).
  assert (Hm08 : m0 (p + 8) = m (p + 8)) by (unfold m0; apply upd_off; lia).
  set (t := Z.land (m0 (p + 8) + 15) (-4)).
  assert (Htt : t = Z.land (m (p + 8) + 15) (-4)) by (unfold t; rewrite Hm08; reflexivity).
  rewrite <- Htt in Hcls.
  assert (Htr : forall x, tracked a' x <-> tracked (ra (krs k)) x)
    by (intros x; exact (tracked_free_op _ _ _ x Hfo)).
  destruct (Z.ltb_spec t 16) as [Et | Et].
  - rewrite (free_abandons_small p floor fbase (mkC 0 0 0 m0) Et).
    apply kinv_frame; [ exact HK | exact Htr | exact HRmodel | ].
    intros a Ha. cbn [cm]. unfold m0. apply upd_off.
    apply (small_write_misses k p a HK Ht Ha p). left. lia.
  - destruct Hcls as [Hc | Hc]; [ lia | ].
    assert (Hcl0 : 0 <= class_of t) by (apply class_nonneg; exact Et).
    rewrite (free_files_by_class p floor fbase (mkC 0 0 0 m0) t eq_refl Et Hc).
    cbv zeta. cbn [cm].
    apply kinv_frame; [ exact HK | exact Htr | | ].
    + apply (RINV_mem_agree a' m0 _); [ | exact HRmodel ].
      intros x Hx. assert (Hxt : tracked (ra (krs k)) x) by (apply Htr; exact Hx).
      destruct (ki_small k HK x Hxt) as [Hfx _].
      rewrite upd_off by lia.
      rewrite upd_off; [ reflexivity | ].
      destruct (Z.eq_dec x p) as [-> | Hne]; [ lia | ].
      destruct (ki_sep k HK p x Ht Hxt ltac:(congruence)); lia.
    + intros a Ha. unfold m0.
      rewrite upd_off by (apply (small_write_misses k p a HK Ht Ha); right; lia).
      rewrite upd_off by (apply (small_write_misses k p a HK Ht Ha); left; lia).
      apply upd_off. apply (small_write_misses k p a HK Ht Ha p). left. lia.
Qed.

Lemma Forall_mid : forall {A} (P : A -> Prop) l1 e l2,
  Forall P (l1 ++ e :: l2) -> P e /\ Forall P (l1 ++ l2).
Proof.
  intros A P l1 e l2 H. rewrite Forall_app in H. destruct H as [H1 H2].
  inversion H2; subst. split; [ assumption | apply Forall_app; split; assumption ].
Qed.

(* ── dec to zero, large block: the coalescing release ── *)
Lemma k_dec_unique_large_ok : forall k p t l1 l2,
  KINV k -> kll k = l1 ++ (p, t) :: l2 -> rmem (krs k) p = 1 ->
  Z.land (rmem (krs k) (p + 8) + 15) (-4) = t -> 13 <= class_of t ->
  KINV (mkK {| ra := ra (krs k);
               rmem := cm (run_dec p floor fbase (mkC 0 0 0 (rmem (krs k)))) |}
            (ins (kfl k) (p, t)) (l1 ++ l2) (khp k)).
Proof.
  intros k p t l1 l2 HK Hll Hrc Hcap Hcls.
  pose proof Hfloor.
  destruct (linv_parts k HK) as [Hg [Hflo [Heb [Hlv [Hpd Hd]]]]].
  assert (Hin : In (p, t) (kll k)) by (rewrite Hll; apply in_or_app; right; left; reflexivity).
  assert (Ht16 : 16 <= t) by (pose proof (ki_live16 k HK) as H16; rewrite Forall_forall in H16; exact (H16 _ Hin)).
  assert (Hpc : cov (kll k) p) by (apply (base_cov _ p t Hin); lia).
  assert (Hfp : floor <= p) by (pose proof (cov_lo k p HK (or_intror Hpc)); lia).
  assert (He_ll : forall a, inside (p, t) a -> cov (kll k) a) by (intros a Ha; exact (cov_In _ _ a Hin Ha)).
  set (m := rmem (krs k)) in *.
  rewrite (dec_unique_hands_to_free p floor fbase (mkC 0 0 0 m) Hfp Hrc).
  cbn [ctot ccls ctmp cm].
  set (m0 := upd m p 0).
  assert (Ht' : t = Z.land (m0 (p + 8) + 15) (-4)) by (unfold m0; rewrite upd_off by lia; symmetry; exact Hcap).
  rewrite (free_files_large p floor fbase (mkC 0 0 0 m0) t Ht' Ht16 Hcls). cbn [cm].
  destruct (ki_lmem k HK) as [H4 [Hrep Hrc_ll]]. fold m in H4, Hrep, Hrc_ll.
  assert (H40 : m0 4 = 0) by (unfold m0; rewrite upd_off by lia; exact H4).
  assert (Hrep0 : lrep m0 (m0 LHEAD) (kfl k)).
  { unfold m0, LHEAD. rewrite upd_off by lia. apply (lrep_frame (kfl k) m); [ | exact Hrep ].
    intros a Ha. apply upd_off. intros ->. exact (Hd p (field_cov _ _ Hg Ha) Hpc). }
  assert (Hdis : forall a, inside (p, t) a -> ~ cov (kfl k) a)
    by (intros a Ha Hc; exact (Hd a Hc (He_ll a Ha))).
  destruct (lfree_mem_spec (kfl k) p t (class_of t) m0 Hg (fl_ge16 k HK) (fl_top k HK) Hdis
              ltac:(lia) Ht16 H40 Hrep0) as [H4' [Hrep' Hfr']].
  set (m' := lfree_mem p t (class_of t) m0) in *.
  (* outside the release's reach, nothing moved *)
  assert (Hout : forall a, a <> LHEAD -> ~ inside (p, t) a -> ~ cov (kfl k) a -> m' a = m a).
  { intros a H12 Hi Hc. destruct (Z.eq_dec (m' a) (m0 a)) as [E | E].
    - rewrite E. unfold m0. apply upd_off. intros ->. apply Hi. unfold inside; cbn; lia.
    - exfalso. destruct (Hfr' a E) as [Ha | Ha]; [ exact (H12 Ha) | ].
      rewrite cov_ins in Ha by (first [ exact (gaps_nonneg _ Hg) | cbn; lia ]).
      destruct Ha as [Ha | Ha]; [ exact (Hi Ha) | exact (Hc Ha) ]. }
  pose proof (pdisj_app_rem l1 (p, t) l2 ltac:(rewrite <- Hll; exact Hpd)) as [_ Hed].
  constructor; cbn [krs kfl kll khp ra rmem lst].
  - apply (RINV_mem_agree _ m); [ | exact (RINV_eta _ (ki_rinv k HK)) ].
    intros x Hx. destruct (small_off_large k x x HK Hx ltac:(lia)) as [Hnf Hnl].
    destruct (ki_small k HK x Hx) as [Hfx _].
    apply Hout; [ unfold LHEAD; lia | intros Hi; exact (Hnl (He_ll x Hi)) | exact Hnf ].
  - intros x Hx. destruct (ki_small k HK x Hx) as [Hfx [Hhx Hw]].
    split; [ exact Hfx | split; [ exact Hhx | ] ].
    intros a Ha Hc. apply (Hw a Ha). apply cov_app in Hc as [Hc | Hc].
    + rewrite cov_ins in Hc by (first [ exact (gaps_nonneg _ Hg) | cbn; lia ]).
      apply cov_app. destruct Hc as [Hc | Hc]; [ right; exact (He_ll a Hc) | left; exact Hc ].
    + apply cov_app. right. rewrite Hll. apply cov_app_rem. exact Hc.
  - exact (ki_sep k HK).
  - pose proof (free_preserves_LINV floor (lst k) l1 l2 (p, t) (ki_linv k HK) Hll
                  ltac:(unfold MINSZ; cbn; lia)) as HL.
    exact HL.
  - split; [ exact H4' | split; [ exact Hrep' | ] ].
    apply Forall_forall. intros [q z] Hq.
    pose proof (Hrc_ll) as Hrc'. rewrite Forall_forall in Hrc'.
    assert (Hq' : In (q, z) (kll k)) by (rewrite Hll; apply in_or_app; apply in_app_or in Hq as [H1 | H1]; [ left; exact H1 | right; right; exact H1 ]).
    assert (Hz : 16 <= z) by (pose proof (ki_live16 k HK) as H16; rewrite Forall_forall in H16; exact (H16 _ Hq')).
    assert (Hqc : cov (kll k) q) by (apply (base_cov _ q z Hq'); lia).
    assert (Hqf : floor <= q) by (pose proof (cov_lo k q HK (or_intror Hqc)); lia).
    cbn. rewrite Hout; [ exact (Hrc' _ Hq') | unfold LHEAD; lia | | intros Hc; exact (Hd q Hc Hqc) ].
    intros Hi. apply (cov_of_Forall_disj (p, t) (l1 ++ l2) q Hed Hi).
    exact (base_cov _ q z Hq ltac:(lia)).
  - pose proof (ki_live16 k HK) as H16. rewrite Hll in H16. exact (proj2 (Forall_mid _ _ _ _ H16)).
  - exact (ki_hal k HK).
  - pose proof (ki_lal k HK) as Hla. rewrite Hll in Hla.
    apply ins_al4; [ exact (ki_fal k HK) | exact (proj1 (Forall_mid _ _ _ _ Hla)) ].
  - pose proof (ki_lal k HK) as Hla. rewrite Hll in Hla. exact (proj2 (Forall_mid _ _ _ _ Hla)).
  - exact (ki_top k HK).
  - exact (ki_floor k HK).
Qed.

(* Project the memory out of an alloc outcome — injection would
   numeral-normalize the record; a projector keeps `4 * cl` folded. *)
Definition out_mem (d : Mem) (o : aout) : Mem :=
  match o with AFall c0 => am c0 | ARet _ c0 => am c0 | AAbort => d end.

Definition out_val (o : aout) : Z :=
  match o with ARet v _ => v | _ => 0 end.

(* ── alloc, the classed pop ── *)
Lemma k_new_pop_ok : forall k lenv pages w cl h a' v c',
  KINV k ->
  w = Z.land (lenv + 15) (-4) -> 16 <= w -> cl = class_of w -> cl < 13 ->
  h = rmem (krs k) (fbase + 4 * cl) -> h <> 0 ->
  freeS (ra (krs k)) h = true -> alloc (ra (krs k)) h = Some a' ->
  run_alloc lenv fbase (mkA 0 0 0 0 (khp k) pages (rmem (krs k))) = ARet v c' ->
  KINV (setmem k a' (am c')).
Proof.
  intros k lenv pages w cl h a' v c' HK Hw H16 Hcl Hcl13 Hh Hnz Hfr Halloc Hrun.
  pose proof Hfloor. pose proof Hfb. pose proof Htable.
  set (m := rmem (krs k)) in *.
  rewrite (alloc_pops_filed_head lenv fbase (mkA 0 0 0 0 (khp k) pages m) w cl (fbase + 4 * cl) h
             Hw H16 Hcl Hcl13 eq_refl Hh Hnz) in Hrun.
  assert (Hmem := f_equal (out_mem m) Hrun). cbn [out_mem am] in Hmem. rewrite <- Hmem.
  assert (Hcl0 : 0 <= cl) by (rewrite Hcl; apply class_nonneg; exact H16).
  assert (Ht : tracked (ra (krs k)) h) by (left; exact Hfr).
  destruct (ki_small k HK h Ht) as [Hfh _].
  assert (HRmodel : RINV {| ra := a'; rmem := upd m h 1 |}).
  { apply (r_new_preserves_RINV (krs k) h _ (ki_rinv k HK)).
    unfold r_new, r_alloc. rewrite Halloc. unfold rc_init; cbn [ra rmem].
    replace (h + RC_OFFSET) with h by (unfold RC_OFFSET; lia). reflexivity. }
  apply kinv_frame; [ exact HK | intros x; exact (tracked_pop _ _ _ x (proj1 (ki_rinv k HK)) Hfr Halloc) | | ].
  - apply (RINV_mem_agree a' (upd m h 1) _); [ | exact HRmodel ].
    intros x Hx. assert (Hxt : tracked (ra (krs k)) x)
      by (apply (tracked_pop _ _ _ x (proj1 (ki_rinv k HK)) Hfr Halloc); exact Hx).
    destruct (ki_small k HK x Hxt) as [Hfx _].
    assert (Hx16 : x = h \/ x + 16 <= h \/ h + 16 <= x).
    { destruct (Z.eq_dec x h) as [E | Hne]; [ left; exact E | right ].
      assert (Hhx : h <> x) by congruence. destruct (ki_sep k HK h x Ht Hxt Hhx); lia. }
    unfold upd.
    repeat match goal with |- context [?a =? ?b] => destruct (Z.eqb_spec a b) end;
      try lia; reflexivity.
  - intros a Ha.
    rewrite upd_off by (apply (small_write_misses k h a HK Ht Ha); left; lia).
    rewrite upd_off by (apply (small_write_misses k h a HK Ht Ha); left; lia).
    rewrite upd_off by (apply (small_write_misses k h a HK Ht Ha); left; lia).
    apply upd_off. apply (small_write_misses k h a HK Ht Ha). right. lia.
Qed.

Lemma LINV_grow : forall lo st h',
  LINV lo st -> heap st <= h' -> LINV lo {| heap := h'; fl := fl st; live := live st |}.
Proof.
  intros lo st h' [Hwf [Heb [Hlv [Hpd Hd]]]] Hh. cbn.
  split; [ exact Hwf | split; [ exact (ends_below_mono _ _ _ Heb Hh) | split; [ | split; assumption ] ] ].
  eapply Forall_impl; [ | exact Hlv ]. intros a Ha. cbn in *. lia.
Qed.

Lemma pages_top : forall pages, pages <= 65536 -> Z.shiftl pages 16 <= MEMTOP.
Proof. intros pages H. rewrite Z.shiftl_mul_pow2 by lia. unfold MEMTOP. lia. Qed.

(* ── alloc, the classed fresh bump at the frontier ── *)
Lemma k_new_bump_ok : forall k lenv pages w cl v c',
  KINV k ->
  w = Z.land (lenv + 15) (-4) -> 16 <= w -> cl = class_of w -> cl < 13 ->
  rmem (krs k) (fbase + 4 * cl) = 0 -> 0 <= lenv -> pages <= 65536 ->
  khp k + 16 * 2 ^ cl <= Z.shiftl pages 16 ->
  run_alloc lenv fbase (mkA 0 0 0 0 (khp k) pages (rmem (krs k))) = ARet v c' ->
  KINV (mkK {| ra := fresh (ra (krs k)) (khp k); rmem := am c' |}
            (kfl k) (kll k) (khp k + 16 * 2 ^ cl)).
Proof.
  intros k lenv pages w cl v c' HK Hw H16 Hcl Hcl13 Hempty Hlen Hpg Hfit Hrun.
  pose proof Hfloor.
  set (m := rmem (krs k)) in *. set (hp := khp k) in *.
  assert (Hcl0 : 0 <= cl) by (rewrite Hcl; apply class_nonneg; exact H16).
  assert (Hpow : 1 <= 2 ^ cl) by (apply (Z.pow_le_mono_r 2 0 cl); lia).
  rewrite (alloc_bumps_fresh_classed lenv fbase (mkA 0 0 0 0 hp pages m) w cl
             Hw H16 Hcl Hcl13 Hempty Hlen Hfit) in Hrun.
  assert (Hmem := f_equal (out_mem m) Hrun). cbn [out_mem am agh] in Hmem. rewrite <- Hmem.
  set (M := upd (upd (upd m hp 1) (hp + 4) lenv) (hp + 8) (16 * 2 ^ cl - 12)).
  assert (HM : forall a, a < hp -> M a = m a) by (intros a Ha; unfold M; rewrite !upd_off by lia; reflexivity).
  assert (Hold : forall x, tracked (ra (krs k)) x -> x + 16 <= hp)
    by (intros x Hx; exact (proj1 (proj2 (ki_small k HK x Hx)))).
  assert (Hcovhp : forall a, cov (kfl k) a \/ cov (kll k) a -> a < hp)
    by (intros a Ha; exact (proj2 (cov_lo k a HK Ha))).
  pose proof (ki_floor k HK) as Hfl.
  constructor; cbn [krs kfl kll khp ra rmem lst].
  - apply (fresh_RINV _ m); [ exact (RINV_eta _ (ki_rinv k HK)) | | | ].
    + intros Ht. pose proof (Hold hp Ht). lia.
    + unfold M. rewrite !upd_off by lia. unfold upd. rewrite Z.eqb_refl. reflexivity.
    + intros x Hx. apply HM. pose proof (Hold x Hx). lia.
  - intros x Hx. apply tracked_fresh in Hx as [-> | Hx].
    + split; [ exact Hfl | split; [ lia | ] ].
      intros a Ha Hc. apply cov_app in Hc. pose proof (Hcovhp a Hc). lia.
    + destruct (ki_small k HK x Hx) as [Hfx [Hhx Hw']]. split; [ exact Hfx | split; [ lia | exact Hw' ] ].
  - intros x y Hx Hy Hne.
    apply tracked_fresh in Hx as [-> | Hx]; apply tracked_fresh in Hy as [-> | Hy].
    + contradiction.
    + pose proof (Hold y Hy). lia.
    + pose proof (Hold x Hx). lia.
    + exact (ki_sep k HK x y Hx Hy Hne).
  - apply (LINV_grow floor (lst k)); [ exact (ki_linv k HK) | cbn [lst heap khp]; fold hp; lia ].
  - apply (LMEM_frame m); [ exact (ki_lmem k HK) | exact (proj1 (linv_parts k HK)) | exact (ki_live16 k HK) | ].
    intros a Ha. apply HM. destruct Ha as [Ha | [Ha | Ha]]; [ unfold LHEAD in *; lia | unfold LHEAD in *; lia | exact (Hcovhp a Ha) ].
  - exact (ki_live16 k HK).
  - pose proof (ki_hal k HK) as Hh. fold hp in Hh.
    replace (hp + 16 * 2 ^ cl) with (hp + (4 * 2 ^ cl) * 4) by lia.
    rewrite Z_mod_plus_full. exact Hh.
  - exact (ki_fal k HK).
  - exact (ki_lal k HK).
  - pose proof (pages_top pages Hpg). lia.
  - lia.
Qed.

Lemma land_m4_mod : forall x, Z.land x (-4) mod 4 = 0.
Proof. intros x. rewrite land_m4. rewrite Zminus_mod, Z.mod_mod by lia. rewrite Z.sub_diag. reflexivity. Qed.

(* The memory after the large take, read through the step's own result. *)
Lemma lalloc_shape : forall st w,
  snd (lalloc st w) =
  match take (fl st) w (heap st) with
  | TFound q z l' => {| heap := heap st; fl := l'; live := (q, z) :: live st |}
  | TExtend p l' => {| heap := p + w; fl := l'; live := (p, w) :: live st |}
  | TMiss => {| heap := heap st + w; fl := fl st; live := (heap st, w) :: live st |}
  end /\
  fst (lalloc st w) =
  match take (fl st) w (heap st) with
  | TFound q _ _ => q | TExtend p _ => p | TMiss => heap st
  end.
Proof. intros st w. unfold lalloc. destruct (take (fl st) w (heap st)); split; reflexivity. Qed.

(* ── alloc above the class table: the large-list take, then (on no fit)
   the exact bump from the possibly-lowered frontier ── *)
Lemma k_new_large_ok : forall k lenv pages w v c',
  KINV k ->
  w = Z.land (lenv + 15) (-4) -> 16 <= w -> 13 <= class_of w -> 0 <= lenv ->
  pages <= 65536 -> khp k + w <= Z.shiftl pages 16 ->
  run_alloc lenv fbase (mkA 0 0 0 0 (khp k) pages (rmem (krs k))) = ARet v c' ->
  let st' := snd (lalloc (lst k) w) in
  v = fst (lalloc (lst k) w) /\
  KINV (mkK {| ra := ra (krs k); rmem := am c' |} (fl st') (live st') (heap st')).
Proof.
  intros k lenv pages w v c' HK Hw H16 Hcl Hlen Hpg Hfit Hrun st'.
  pose proof Hfloor. pose proof (pages_top pages Hpg) as Htop.
  set (m := rmem (krs k)) in *. set (hp := khp k) in *.
  destruct (linv_parts k HK) as [Hg [Hflo [Heb [Hlv [Hpd Hd]]]]].
  destruct (ki_lmem k HK) as [H4 [Hrep Hrc]]. fold m in H4, Hrep, Hrc.
  pose proof (ki_floor k HK) as Hfh. fold hp in Hfh.
  assert (Hw4 : w mod 4 = 0) by (rewrite Hw; apply land_m4_mod).
  assert (HL' : LINV floor st')
    by (exact (alloc_preserves_LINV floor (lst k) w (ki_linv k HK) ltac:(unfold MINSZ; lia) Hfh)).
  destruct (lalloc_shape (lst k) w) as [Hst Hv]. fold st' in Hst. cbn [lst fl heap live] in Hst, Hv. fold hp in Hst, Hv.
  pose proof (take_al4 (kfl k) w hp (ki_fal k HK) Hw4) as Hal.
  destruct (ltake_run_spec (kfl k) lenv 0 (28 - clz32 (w - 1)) w 0 m hp Hg (fl_ge16 k HK) (fl_top k HK)
              H16 ltac:(lia) H4 Hrep) as [s' [Hfr [_ [_ [Hl3 [_ Hres]]]]]].
  (* every tracked small cell and every live large base sat outside the take's reach *)
  assert (Hsm : forall x, tracked (ra (krs k)) x -> LargeTree.mem s' x = m x).
  { intros x Hx. destruct (small_off_large k x x HK Hx ltac:(lia)) as [Hnf _].
    destruct (ki_small k HK x Hx) as [Hfx _].
    destruct (Z.eq_dec (LargeTree.mem s' x) (m x)) as [E | E]; [ exact E | exfalso ].
    destruct (Hfr x E) as [Ha | Ha]; [ unfold LHEAD in Ha; lia | exact (Hnf Ha) ]. }
  assert (Hlb : forall q z, In (q, z) (kll k) -> LargeTree.mem s' q = m q /\ floor <= q /\ q + 16 <= hp + 0
                                                  /\ cov (kll k) q).
  { intros q z Hq. assert (Hz : 16 <= z) by (pose proof (ki_live16 k HK) as H1; rewrite Forall_forall in H1; exact (H1 _ Hq)).
    assert (Hqc : cov (kll k) q) by (apply (base_cov _ q z Hq); lia).
    rewrite Forall_forall in Hlv. specialize (Hlv _ Hq). cbn in Hlv.
    split; [ | split; [ lia | split; [ lia | exact Hqc ] ] ].
    destruct (Z.eq_dec (LargeTree.mem s' q) (m q)) as [E | E]; [ exact E | exfalso ].
    destruct (Hfr q E) as [Ha | Ha]; [ unfold LHEAD in Ha; lia | exact (Hd q Ha Hqc) ]. }
  destruct (take (kfl k) w hp) as [ q z l' | p l' | ] eqn:Et.
  - (* hit *)
    destruct Hres as [Hrun0 [Hgh [H4' [Hrep' [Hq1 [Hq4 Hq8]]]]]].
    rewrite (alloc_large_hit lenv fbase (mkA 0 0 0 0 hp pages m) w q s' Hw H16 Hcl Hrun0) in Hrun.
    assert (Hmem := f_equal (out_mem m) Hrun). cbn [out_mem am] in Hmem.
    assert (Hvq := f_equal out_val Hrun). cbn [out_val] in Hvq.
    destruct (take_found (kfl k) w hp q z l' Hg ltac:(unfold MINSZ; lia) Et) as [Hwz [Hmz [Hcv [Hrest Hgl]]]].
    destruct Hal as [Hqz Hl'].
    split; [ rewrite Hv; symmetry; exact Hvq | ].
    rewrite Hst. cbn [fl live heap]. rewrite <- Hmem.
    constructor; cbn [krs kfl kll khp ra rmem lst].
    + apply (RINV_mem_agree _ m); [ exact Hsm | exact (RINV_eta _ (ki_rinv k HK)) ].
    + intros x Hx. destruct (ki_small k HK x Hx) as [Hfx [Hhx Hw']].
      split; [ exact Hfx | split; [ exact Hhx | ] ].
      intros a Ha Hc. apply (Hw' a Ha). apply cov_app.
      apply cov_app in Hc as [Hc | Hc]; [ left; exact (proj1 (proj1 (Hrest a) Hc)) | ].
      apply cov_cons in Hc as [Hc | Hc]; [ left; apply Hcv; unfold inside in Hc; cbn in Hc; lia | right; exact Hc ].
    + exact (ki_sep k HK).
    + rewrite Hst in HL'. exact HL'.
    + split; [ exact H4' | split; [ exact Hrep' | ] ].
      constructor; [ cbn; lia | ].
      apply Forall_forall. intros [q0 z0] Hq0. destruct (Hlb q0 z0 Hq0) as [E _]. cbn. rewrite E.
      rewrite Forall_forall in Hrc. exact (Hrc _ Hq0).
    + constructor; [ cbn; lia | exact (ki_live16 k HK) ].
    + exact (ki_hal k HK).
    + exact Hl'.
    + constructor; [ exact Hqz | exact (ki_lal k HK) ].
    + exact (ki_top k HK).
    + exact Hfh.
  - (* no fit, the tail node ends at the frontier: extend it *)
    destruct Hres as [Hrun0 [Hgh [H4' Hrep']]].
    destruct (take_extend (kfl k) w hp p l' Hg Et) as [sp [Hfl [Hph [Hsp [Hends Hgl]]]]].
    destruct Hal as [Hp4 Hl'].
    assert (Hsp16 : 16 <= sp).
    { assert (Hin : In (p, sp) (kfl k)) by (rewrite Hfl; apply in_or_app; right; left; reflexivity).
      exact (gaps_In_size _ _ _ Hg Hin). }
    assert (Hpfl : floor <= p).
    { rewrite Forall_forall in Hflo. apply (Hflo (p, sp)). rewrite Hfl. apply in_or_app. right. left. reflexivity. }
    assert (Htail : forall a, p <= a < hp -> cov (kfl k) a).
    { intros a Ha. rewrite Hfl. apply cov_app. right. apply cov_cons. left. unfold inside; cbn; lia. }
    assert (Hnx : Z.land (p + 12 + lenv + 3) (-4) = p + w) by (rewrite land_m4_shift by exact Hp4; rewrite <- Hw; reflexivity).
    pose proof (alloc_large_bump lenv fbase (mkA 0 0 0 0 hp pages m) w s' Hw H16 Hcl Hrun0 Hl3) as Hb.
    cbv zeta in Hb. rewrite Hgh, Hnx in Hb.
    specialize (Hb ltac:(lia) ltac:(cbn [apages]; lia)).
    rewrite Hb in Hrun.
    assert (Hmem := f_equal (out_mem m) Hrun). cbn [out_mem am] in Hmem.
    assert (Hvq := f_equal out_val Hrun). cbn [out_val] in Hvq.
    split; [ rewrite Hv; symmetry; exact Hvq | ].
    rewrite Hst. cbn [fl live heap]. rewrite <- Hmem.
    set (M := upd (upd (upd (LargeTree.mem s') p 1) (p + 4) lenv) (p + 8) (w - 12)).
    assert (HM : forall a, a < p \/ p + 16 <= a -> M a = LargeTree.mem s' a)
      by (intros a Ha; unfold M; rewrite !upd_off by lia; reflexivity).
    constructor; cbn [krs kfl kll khp ra rmem lst].
    + apply (RINV_mem_agree _ m); [ | exact (RINV_eta _ (ki_rinv k HK)) ].
      intros x Hx. destruct (small_off_large k x x HK Hx ltac:(lia)) as [Hnf _].
      destruct (ki_small k HK x Hx) as [Hfx [Hhx _]].
      rewrite HM; [ exact (Hsm x Hx) | ].
      destruct (Z_lt_ge_dec x p) as [Hlt | Hge]; [ left; exact Hlt | right ].
      destruct (Z_lt_ge_dec x hp) as [Hlt' | Hge']; [ exfalso; exact (Hnf (Htail x ltac:(lia))) | lia ].
    + intros x Hx. destruct (ki_small k HK x Hx) as [Hfx [Hhx Hw']].
      split; [ exact Hfx | split; [ lia | ] ].
      intros a Ha Hc. apply (Hw' a Ha). apply cov_app.
      apply cov_app in Hc as [Hc | Hc]; [ left; rewrite Hfl; apply cov_app; left; exact Hc | ].
      apply cov_cons in Hc as [Hc | Hc]; [ left; apply Htail; unfold inside in Hc; cbn in Hc; lia | right; exact Hc ].
    + exact (ki_sep k HK).
    + rewrite Hst in HL'. exact HL'.
    + assert (Hfl' : forall a, field l' a -> a < p).
      { intros a Ha. destruct Ha as [b' [s'' [Hin Ha]]].
        rewrite Forall_forall in Hends. specialize (Hends _ Hin). cbn in Hends.
        pose proof (gaps_In_size _ _ _ Hgl Hin). lia. }
      split; [ rewrite HM by lia; exact H4' | split ].
      * unfold LHEAD. rewrite HM by lia. apply (lrep_frame l' (LargeTree.mem s')); [ | exact Hrep' ].
        intros a Ha. apply HM. left. exact (Hfl' a Ha).
      * constructor; [ cbn; unfold M; rewrite !upd_off by lia; unfold upd; rewrite Z.eqb_refl; lia | ].
        apply Forall_forall. intros [q0 z0] Hq0. destruct (Hlb q0 z0 Hq0) as [E [_ [_ Hqc]]]. cbn.
        rewrite HM; [ rewrite E; rewrite Forall_forall in Hrc; exact (Hrc _ Hq0) | ].
        destruct (Z_lt_ge_dec q0 p) as [Hlt | Hge]; [ left; exact Hlt | right ].
        destruct (Z_lt_ge_dec q0 hp) as [Hlt' | Hge'].
        -- exfalso. exact (Hd q0 (Htail q0 ltac:(lia)) Hqc).
        -- lia.
    + constructor; [ cbn; lia | exact (ki_live16 k HK) ].
    + rewrite Z.add_mod by lia. rewrite Hp4, Hw4. reflexivity.
    + exact Hl'.
    + constructor; [ split; assumption | exact (ki_lal k HK) ].
    + lia.
    + lia.
  - (* no fit and no tail at the frontier: bump fresh *)
    destruct Hres as [Hrun0 [Hgh Hsame]].
    assert (Hhp4 : hp mod 4 = 0) by exact (ki_hal k HK).
    assert (Hnx : Z.land (hp + 12 + lenv + 3) (-4) = hp + w) by (rewrite land_m4_shift by exact Hhp4; rewrite <- Hw; reflexivity).
    pose proof (alloc_large_bump lenv fbase (mkA 0 0 0 0 hp pages m) w s' Hw H16 Hcl Hrun0 Hl3) as Hb.
    cbv zeta in Hb. rewrite Hgh, Hnx in Hb.
    specialize (Hb ltac:(lia) ltac:(cbn [apages]; lia)).
    rewrite Hb in Hrun.
    assert (Hmem := f_equal (out_mem m) Hrun). cbn [out_mem am] in Hmem.
    assert (Hvq := f_equal out_val Hrun). cbn [out_val] in Hvq.
    split; [ rewrite Hv; symmetry; exact Hvq | ].
    rewrite Hst. cbn [fl live heap]. rewrite <- Hmem.
    set (M := upd (upd (upd (LargeTree.mem s') hp 1) (hp + 4) lenv) (hp + 8) (w - 12)).
    assert (HM : forall a, a < hp -> M a = m a)
      by (intros a Ha; unfold M; rewrite !upd_off by lia; exact (Hsame a)).
    assert (Hcovhp : forall a, cov (kfl k) a \/ cov (kll k) a -> a < hp)
      by (intros a Ha; exact (proj2 (cov_lo k a HK Ha))).
    constructor; cbn [krs kfl kll khp ra rmem lst].
    + apply (RINV_mem_agree _ m); [ | exact (RINV_eta _ (ki_rinv k HK)) ].
      intros x Hx. destruct (ki_small k HK x Hx) as [_ [Hhx _]]. apply HM. lia.
    + intros x Hx. destruct (ki_small k HK x Hx) as [Hfx [Hhx Hw']].
      split; [ exact Hfx | split; [ lia | ] ].
      intros a Ha Hc. apply cov_app in Hc as [Hc | Hc]; [ apply (Hw' a Ha); apply cov_app; left; exact Hc | ].
      apply cov_cons in Hc as [Hc | Hc]; [ unfold inside in Hc; cbn in Hc; lia | ].
      apply (Hw' a Ha). apply cov_app. right. exact Hc.
    + exact (ki_sep k HK).
    + rewrite Hst in HL'. exact HL'.
    + split; [ rewrite HM by lia; exact H4 | split ].
      * unfold LHEAD. rewrite HM by lia. apply (lrep_frame (kfl k) m); [ | exact Hrep ].
        intros a Ha. apply HM. apply Hcovhp. left. exact (field_cov _ _ Hg Ha).
      * constructor; [ cbn; unfold M; rewrite !upd_off by lia; unfold upd; rewrite Z.eqb_refl; lia | ].
        apply Forall_forall. intros [q0 z0] Hq0. destruct (Hlb q0 z0 Hq0) as [_ [_ [_ Hqc]]]. cbn.
        rewrite HM; [ rewrite Forall_forall in Hrc; exact (Hrc _ Hq0) | apply Hcovhp; right; exact Hqc ].
    + constructor; [ cbn; lia | exact (ki_live16 k HK) ].
    + rewrite Z.add_mod by lia. rewrite Hhp4, Hw4. reflexivity.
    + exact (ki_fal k HK).
    + constructor; [ split; assumption | exact (ki_lal k HK) ].
    + lia.
    + lia.
Qed.

(* ══ THE CONCRETE RUN RELATION ═════════════════════════════════════════
   Memory moves by the PROVEN TREES; the allocator states are the ghost. *)

Inductive kstep : KState -> KState -> Prop :=
| k_inc : forall k p,
    liveS (ra (krs k)) p = true ->
    kstep k (setmem k (ra (krs k)) (cm (run_inc p floor (mkC 0 0 0 (rmem (krs k))))))
| k_inc_large : forall k p s,
    In (p, s) (kll k) ->
    kstep k (setmem k (ra (krs k)) (cm (run_inc p floor (mkC 0 0 0 (rmem (krs k))))))
| k_dec_shared : forall k p,
    liveS (ra (krs k)) p = true -> 2 <= rmem (krs k) p ->
    kstep k (setmem k (ra (krs k)) (cm (run_dec p floor fbase (mkC 0 0 0 (rmem (krs k))))))
| k_dec_shared_large : forall k p s,
    In (p, s) (kll k) -> 2 <= rmem (krs k) p ->
    kstep k (setmem k (ra (krs k)) (cm (run_dec p floor fbase (mkC 0 0 0 (rmem (krs k))))))
| k_dec_unique : forall k p a',
    liveS (ra (krs k)) p = true -> rmem (krs k) p = 1 ->
    free_op (ra (krs k)) p = Some a' ->
    Z.land (rmem (krs k) (p + 8) + 15) (-4) < 16 \/
    class_of (Z.land (rmem (krs k) (p + 8) + 15) (-4)) < 13 ->
    kstep k (setmem k a' (cm (run_dec p floor fbase (mkC 0 0 0 (rmem (krs k))))))
| k_dec_unique_large : forall k p t l1 l2,
    kll k = l1 ++ (p, t) :: l2 -> rmem (krs k) p = 1 ->
    Z.land (rmem (krs k) (p + 8) + 15) (-4) = t -> 13 <= class_of t ->
    kstep k (mkK {| ra := ra (krs k);
                    rmem := cm (run_dec p floor fbase (mkC 0 0 0 (rmem (krs k)))) |}
                 (ins (kfl k) (p, t)) (l1 ++ l2) (khp k))
| k_new_pop : forall k lenv pages w cl h a' v c',
    w = Z.land (lenv + 15) (-4) -> 16 <= w -> cl = class_of w -> cl < 13 ->
    h = rmem (krs k) (fbase + 4 * cl) -> h <> 0 ->
    freeS (ra (krs k)) h = true -> alloc (ra (krs k)) h = Some a' ->
    run_alloc lenv fbase (mkA 0 0 0 0 (khp k) pages (rmem (krs k))) = ARet v c' ->
    kstep k (setmem k a' (am c'))
| k_new_bump : forall k lenv pages w cl v c',
    w = Z.land (lenv + 15) (-4) -> 16 <= w -> cl = class_of w -> cl < 13 ->
    rmem (krs k) (fbase + 4 * cl) = 0 -> 0 <= lenv -> pages <= 65536 ->
    khp k + 16 * 2 ^ cl <= Z.shiftl pages 16 ->
    run_alloc lenv fbase (mkA 0 0 0 0 (khp k) pages (rmem (krs k))) = ARet v c' ->
    kstep k (mkK {| ra := fresh (ra (krs k)) (khp k); rmem := am c' |}
                 (kfl k) (kll k) (khp k + 16 * 2 ^ cl))
| k_new_large : forall k lenv pages w v c',
    w = Z.land (lenv + 15) (-4) -> 16 <= w -> 13 <= class_of w -> 0 <= lenv ->
    pages <= 65536 -> khp k + w <= Z.shiftl pages 16 ->
    run_alloc lenv fbase (mkA 0 0 0 0 (khp k) pages (rmem (krs k))) = ARet v c' ->
    kstep k (mkK {| ra := ra (krs k); rmem := am c' |}
                 (fl (snd (lalloc (lst k) w))) (live (snd (lalloc (lst k) w)))
                 (heap (snd (lalloc (lst k) w)))).

Lemma large_base : forall k p s, KINV k -> In (p, s) (kll k) ->
  floor <= p /\ 1 <= rmem (krs k) p.
Proof.
  intros k p s HK Hin.
  destruct (linv_parts k HK) as [_ [_ [_ [Hlv _]]]]. rewrite Forall_forall in Hlv.
  destruct (Hlv _ Hin) as [Hf _]. cbn in Hf.
  destruct (ki_lmem k HK) as [_ [_ Hrc]]. rewrite Forall_forall in Hrc.
  split; [ exact Hf | exact (Hrc _ Hin) ].
Qed.

Theorem kstep_preserves_KINV : forall k k', kstep k k' -> KINV k -> KINV k'.
Proof.
  intros k k' Hs HK. destruct Hs.
  - (* inc, small *)
    assert (Ht : tracked (ra (krs k)) p) by (right; exact H).
    destruct (ki_small k HK p Ht) as [Hfp _].
    rewrite (inc_realizes_rt_inc p floor (mkC 0 0 0 (rmem (krs k))) Hfp). cbn [cm].
    rewrite rt_inc_plain. apply small_cell_ok; [ exact HK | exact Ht | ].
    rewrite <- rt_inc_plain.
    apply (r_inc_preserves_RINV (krs k) p _ (ki_rinv k HK)).
    unfold r_inc. rewrite H. reflexivity.
  - (* inc, large *)
    destruct (large_base k p s HK H) as [Hfp Hrc].
    rewrite (inc_realizes_rt_inc p floor (mkC 0 0 0 (rmem (krs k))) Hfp). cbn [cm].
    rewrite rt_inc_plain. apply (large_cell_ok k p s); [ exact HK | exact H | lia ].
  - (* dec, shared, small *)
    assert (Ht : tracked (ra (krs k)) p) by (right; exact H).
    destruct (ki_small k HK p Ht) as [Hfp _].
    destruct (dec_shared_realizes_rt_dec p floor fbase (mkC 0 0 0 (rmem (krs k))) Hfp H0) as [_ Hmem].
    cbn [cm] in Hmem. rewrite Hmem.
    apply small_cell_ok; [ exact HK | exact Ht | ].
    apply (r_dec_preserves_RINV (krs k) p _ (ki_rinv k HK)).
    rewrite (r_dec_pos (krs k) p) by (rewrite rc_at_plain; lia).
    rewrite rc_at_plain.
    replace (rmem (krs k) p =? 1) with false by (symmetry; apply Z.eqb_neq; lia).
    replace (p + RC_OFFSET) with p by (unfold RC_OFFSET; lia).
    reflexivity.
  - (* dec, shared, large *)
    destruct (large_base k p s HK H) as [Hfp _].
    destruct (dec_shared_realizes_rt_dec p floor fbase (mkC 0 0 0 (rmem (krs k))) Hfp H0) as [_ Hmem].
    cbn [cm] in Hmem. rewrite Hmem.
    apply (large_cell_ok k p s); [ exact HK | exact H | lia ].
  - apply (k_dec_unique_small_ok k p a'); assumption.
  - apply (k_dec_unique_large_ok k p t l1 l2); assumption.
  - apply (k_new_pop_ok k lenv pages w cl h a' v c'); assumption.
  - apply (k_new_bump_ok k lenv pages w cl v c'); assumption.
  - exact (proj2 (k_new_large_ok k lenv pages w v c' HK H H0 H1 H2 H3 H4 H5)).
Qed.

Inductive ksteps : KState -> KState -> Prop :=
| ksteps_refl : forall k, ksteps k k
| ksteps_step : forall k k' k'', kstep k k' -> ksteps k' k'' -> ksteps k k''.

Theorem structural_runs_preserve_KINV : forall k k',
  ksteps k k' -> KINV k -> KINV k'.
Proof.
  intros k k' Hst. induction Hst as [ k0 | k0 k1 k2 Hs _ IH ]; intro HK.
  - exact HK.
  - apply IH. exact (kstep_preserves_KINV k0 k1 Hs HK).
Qed.

(* Boot: nothing tracked, no large extents, the frontier at an aligned
   address at or above the floor. *)
Definition k_init (b h0 : Z) : KState := mkK (r_init b) [] [] h0.

Theorem structural_boot_KINV : forall b h0,
  floor <= h0 -> h0 mod 4 = 0 -> h0 <= MEMTOP -> KINV (k_init b h0).
Proof.
  intros b h0 Hf H4 Ht. constructor; cbn [k_init krs kfl kll khp lst].
  - apply r_init_RINV.
  - intros x [Hx | Hx]; discriminate Hx.
  - intros x y [Hx | Hx]; discriminate Hx.
  - split; [ split; [ exact I | constructor ] | split; [ constructor | split; [ constructor | split; [ exact I | ] ] ] ].
    intros x Hx. exfalso. exact (cov_nil x Hx).
  - split; [ reflexivity | split; [ reflexivity | constructor ] ].
  - constructor.
  - exact H4.
  - constructor.
  - constructor.
  - exact Ht.
  - exact Hf.
Qed.

(* ══ THE F-CLASS, AS VIOLATED LEMMAS OF THE EMITTED CODE ═══════════════
   Over ANY structural run from boot — however many allocations, shares
   and releases the program performed through the emitted trees, small
   or large: *)

Section Reachable.

Variables b h0 : Z.
Hypothesis Hh0f : floor <= h0.
Hypothesis Hh04 : h0 mod 4 = 0.
Hypothesis Hh0t : h0 <= MEMTOP.

Lemma reach_KINV : forall k, ksteps (k_init b h0) k -> KINV k.
Proof.
  intros k Hst. apply (structural_runs_preserve_KINV _ _ Hst).
  apply structural_boot_KINV; assumption.
Qed.

(* No aliased small handout: a block the ghost validates for allocation
   is never currently live, and off the free list it reads count 0. *)
Theorem structural_no_aliased_handout : forall k p a',
  ksteps (k_init b h0) k ->
  alloc (ra (krs k)) p = Some a' ->
  liveS (ra (krs k)) p = false
  /\ (freeS (ra (krs k)) p = true -> rmem (krs k) p = 0).
Proof.
  intros k p a' Hst Ha.
  destruct (ki_rinv k (reach_KINV k Hst)) as [Hi [Hf _]].
  split.
  - exact (alloc_not_live (ra (krs k)) p a' Hi Ha).
  - intro Hfr. specialize (Hf p Hfr). rewrite rc_at_plain in Hf. exact Hf.
Qed.

(* Counts stay honest: every filed small block reads 0, every live block
   — small or large — reads at least 1. *)
Theorem structural_counts_stay_honest : forall k,
  ksteps (k_init b h0) k ->
  (forall x, freeS (ra (krs k)) x = true -> rmem (krs k) x = 0)
  /\ (forall x, liveS (ra (krs k)) x = true -> 1 <= rmem (krs k) x)
  /\ Forall (fun e => 1 <= rmem (krs k) (fst e)) (kll k).
Proof.
  intros k Hst. pose proof (reach_KINV k Hst) as HK.
  destruct (ki_rinv k HK) as [_ [Hf Hl]].
  split; [ | split ].
  - intros x Hx. specialize (Hf x Hx). rewrite rc_at_plain in Hf. exact Hf.
  - intros x Hx. specialize (Hl x Hx). rewrite rc_at_plain in Hl. exact Hl.
  - exact (proj2 (proj2 (ki_lmem k HK))).
Qed.

(* No aliased large handout: the extent a large allocation hands out
   overlaps no live large extent and no small block's header window —
   reuse-after-free at extent granularity cannot happen, and the large
   list cannot hand out a small block's memory. *)
Theorem structural_large_handout_is_free : forall k w,
  ksteps (k_init b h0) k -> 16 <= w ->
  match live (snd (lalloc (lst k) w)) with
  | e :: _ =>
      Forall (disj e) (kll k) /\
      (forall x a, tracked (ra (krs k)) x -> x <= a < x + 16 -> ~ inside e a)
  | [] => False
  end.
Proof.
  intros k w Hst Hw. pose proof (reach_KINV k Hst) as HK.
  pose proof Hfloor.
  pose proof (alloc_disjoint_live floor (lst k) w (ki_linv k HK) ltac:(unfold MINSZ; lia) (ki_floor k HK)) as Hd.
  destruct (lalloc_shape (lst k) w) as [Hs _]. cbn [lst fl heap live] in Hs.
  pose proof (linv_parts k HK) as [Hg _].
  destruct (take (kfl k) w (khp k)) as [ q z l' | p l' | ] eqn:Et;
    rewrite Hs in Hd |- *; cbn [live] in Hd |- *; split; try exact Hd.
  - intros x a Hx Ha Hi. destruct (take_found (kfl k) w (khp k) q z l' Hg ltac:(unfold MINSZ; lia) Et) as [_ [_ [Hcv _]]].
    destruct (small_off_large k x a HK Hx Ha) as [Hnf _]. apply Hnf, Hcv. unfold inside in Hi; cbn in Hi; lia.
  - intros x a Hx Ha Hi. destruct (take_extend (kfl k) w (khp k) p l' Hg Et) as [sp [Hfl [Hph _]]].
    destruct (small_off_large k x a HK Hx Ha) as [Hnf _].
    destruct (ki_small k HK x Hx) as [_ [Hhx _]]. unfold inside in Hi; cbn in Hi.
    apply Hnf. rewrite Hfl. apply cov_app. right. apply cov_cons. left. unfold inside; cbn; lia.
  - intros x a Hx Ha Hi. destruct (ki_small k HK x Hx) as [_ [Hhx _]]. unfold inside in Hi; cbn in Hi. lia.
Qed.

End Reachable.

End Composition.
