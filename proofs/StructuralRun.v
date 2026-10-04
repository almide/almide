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

End Composition.
