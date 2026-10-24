(* Almide trust spine — #4013: INITIALIZATION, and the allocation half of
   TYPE, from a per-build certificate.

   Certified.v reached INITIALIZATION only from a stated hypothesis, the
   constructor discipline `init_disc` (every allocation immediately followed
   by ONE store of its whole extent), and TYPE only from another,
   `Forall (alloc_ok m) t` (every allocation has its block's layout). No
   witness checked either. This file replaces both with a CERTIFICATE the
   structural wasm leg emits per build and a checker over it.

   A CONSTRUCTION SITE is one allocation the emitter writes and the stores
   that fill it before the block is handed to anything else:

     h H  w W  n N  r R  s i1 s i2 ...

   a block of an H-byte header and N slots of W bytes, representation R; the
   allocator writes the header (rc / len / cap — StructuralAlloc.v proves
   `$alloc`'s tree does), then slot i1, i2, ... are stored. `site_ok`
   accepts a site when H > 0, W > 0, every slot below N is stored and no
   store names a slot at or past N. What is proven:

   - `site_window`: the events of an accepted site, run from a state where
     the block is not live, are MemorySafe, leave every other block as it
     was, and leave the block live, of the site's size and representation,
     and initialized over its whole extent.
   - `sited_init`: in a trace where every allocation is an accepted site's
     window (`sited`), every load that is in bounds of a live block reads
     initialized bytes — the INITIALIZATION clause, from SPATIAL and
     TEMPORAL.
   - `sited_alloc_ok`: in such a trace every allocation has the layout the
     site gives its block — Certified.v's `alloc_ok` hypothesis, discharged.
   - `certified_sited_memory_safe`: the RC certificate, the bounds
     certificate and the construction sites together give MemorySafe — all
     four clauses, with neither `init_disc` nor `alloc_ok` assumed.
   - `reorder_memory_safe`: MemorySafe is invariant under swapping adjacent
     events on different blocks. A site's window in a real run is
     interleaved with the events of what its elements evaluate (other
     blocks); the theorem says the interleaved trace is safe exactly when
     the windowed one is.
   - `check_init_cert_sound`: the byte parser inside the proof; an accepted
     witness denotes sites that are all `site_ok`.

   HONEST SCOPE. The witness covers the allocation sites the emitter
   records (crates/almide-wasm/src/witness_init.rs: list literals today), so
   `sited` holds of a run only when every allocation it makes comes from a
   recorded site; the corpus coverage is measured, not assumed
   (proofs/structural-wall.sh). TRUSTED, as for the other witnesses: that
   the recorded events are the instructions the bytes contain, and that a
   site's block is not reachable by anything else before its window ends
   (the emitter holds it in a private local until the last store). *)

From AlmideTrust Require Import MemorySafe SpatialCert Certified Subset OwnershipChecker.
From Stdlib Require Import List ZArith Lia Bool Arith String Ascii.
Import ListNotations.
Open Scope Z_scope.

(* ── Construction sites ─────────────────────────────────────────────────── *)

Record Site := mkSite { HDRs : nat; Ws : nat; COUNT : nat; Rs : Repr; STORES : list nat }.

Definition site_size (s : Site) : Z := Z.of_nat (HDRs s) + Z.of_nat (COUNT s) * Z.of_nat (Ws s).
Definition slot_off (s : Site) (i : nat) : Z := Z.of_nat (HDRs s) + Z.of_nat i * Z.of_nat (Ws s).

Definition site_ok (s : Site) : bool :=
  (0 <? HDRs s)%nat && (0 <? Ws s)%nat &&
  forallb (fun i => existsb (Nat.eqb i) (STORES s)) (seq 0 (COUNT s)) &&
  forallb (fun i => (i <? COUNT s)%nat) (STORES s).

(* The events a site performs on its own block. *)
Definition slot_store (b : Blk) (s : Site) (i : nat) : Ev :=
  EStore b (slot_off s i) (Z.of_nat (Ws s)) (Rs s).

Definition site_events (b : Blk) (s : Site) : list Ev :=
  EAlloc b (site_size s) (Rs s) :: EStore b 0 (Z.of_nat (HDRs s)) (Rs s) ::
  map (slot_store b s) (STORES s).

(* The layout the site gives its block, in SpatialCert's terms. *)
Definition site_layout (s : Site) : Layout :=
  {| HDR := Z.of_nat (HDRs s); LEN := Z.of_nat (COUNT s); W := Z.of_nat (Ws s); R := Rs s |}.

Lemma site_layout_bytes : forall s, bytes_of (site_layout s) = site_size s.
Proof. reflexivity. Qed.

Lemma site_ok_spec : forall s, site_ok s = true ->
  (0 < HDRs s)%nat /\ (0 < Ws s)%nat /\
  (forall i, (i < COUNT s)%nat -> In i (STORES s)) /\
  (forall i, In i (STORES s) -> (i < COUNT s)%nat).
Proof.
  intros s H. unfold site_ok in H. repeat rewrite andb_true_iff in H.
  destruct H as [[[H1 H2] H3] H4].
  apply Nat.ltb_lt in H1. apply Nat.ltb_lt in H2.
  rewrite forallb_forall in H3, H4.
  split; [exact H1 |]. split; [exact H2 |]. split.
  - intros i Hi. assert (Hs : In i (seq 0 (COUNT s))) by (apply in_seq; lia).
    specialize (H3 i Hs). apply existsb_exists in H3. destruct H3 as [j [Hj Heq]].
    apply Nat.eqb_eq in Heq. subst. exact Hj.
  - intros i Hi. apply Nat.ltb_lt. exact (H4 i Hi).
Qed.

(* ── Running a list of events ───────────────────────────────────────────── *)

Definition after (h : Heap) (t : list Ev) : Heap := fold_left step t h.

Lemma holds_app' : forall P t1 t2 h,
  holds P h (t1 ++ t2) <-> holds P h t1 /\ holds P (after h t1) t2.
Proof.
  intros P t1. induction t1 as [| e t1 IH]; intros t2 h; simpl.
  - tauto.
  - rewrite IH. unfold after. simpl. tauto.
Qed.

Lemma step_other : forall h e c, c <> ev_blk e -> step h e c = h c.
Proof.
  intros h e c Hc. destruct e as [b n k | b o w k | b o w k | b | b]; simpl in *; unfold set.
  - apply Nat.eqb_neq in Hc. rewrite Hc. reflexivity.
  - destruct (h b); [| reflexivity]. apply Nat.eqb_neq in Hc. rewrite Hc. reflexivity.
  - reflexivity.
  - destruct (h b); [| reflexivity]. apply Nat.eqb_neq in Hc. rewrite Hc. reflexivity.
  - destruct (h b) as [x |]; [| reflexivity]. destruct (0 <? rc x); [| reflexivity].
    apply Nat.eqb_neq in Hc. rewrite Hc. reflexivity.
Qed.

Lemma after_other : forall t h c, Forall (fun e => ev_blk e <> c) t -> after h t c = h c.
Proof.
  intro t. induction t as [| e t IH]; intros h c H; [reflexivity |].
  inversion H as [| ? ? He Ht]; subst. unfold after. simpl. fold (after (step h e) t).
  rewrite (IH _ _ Ht). apply step_other. auto.
Qed.

(* ── One site's window ──────────────────────────────────────────────────── *)

(* A live block of the site's shape, initialized at least where `f` says. *)
Definition shaped (s : Site) (x : Block) : Prop :=
  size x = site_size s /\ repr x = Rs s /\ rc x = 1.

Lemma slot_in_bounds : forall s i x, site_ok s = true -> (i < COUNT s)%nat -> shaped s x ->
  in_bounds x (slot_off s i) (Z.of_nat (Ws s)).
Proof.
  intros s i x Ok Hi [Sz _]. destruct (site_ok_spec s Ok) as [Hh [Hw _]].
  unfold in_bounds, slot_off. rewrite Sz. unfold site_size.
  assert (Z.of_nat i + 1 <= Z.of_nat (COUNT s)) by lia.
  assert (0 < Z.of_nat (Ws s)) by lia.
  split; [nia |]. split; [lia | nia].
Qed.

(* The slot stores: safe, the block stays shaped, everything it had stays
   initialized, and every stored slot is initialized. *)
Lemma stores_window : forall s b l h x,
  site_ok s = true -> (forall i, In i l -> (i < COUNT s)%nat) ->
  h b = Some x -> shaped s x ->
  holds safe_ev h (map (slot_store b s) l) /\
  exists y, after h (map (slot_store b s) l) b = Some y /\ shaped s y /\
    (forall j, init x j = true -> init y j = true) /\
    (forall i, In i l -> all_init (init y) (slot_off s i) (Z.of_nat (Ws s))).
Proof.
  intros s b l. induction l as [| i l IH]; intros h x Ok Hl Hx Sh.
  - split; [exact I |]. exists x. split; [exact Hx |]. split; [exact Sh |].
    split; [auto | intros i Hi; destruct Hi].
  - assert (Hi : (i < COUNT s)%nat) by (apply Hl; left; reflexivity).
    set (x1 := mkBlock (size x) (repr x) (rc x) (mark (init x) (slot_off s i) (Z.of_nat (Ws s)))).
    assert (E1 : step h (slot_store b s i) = set h b x1) by (unfold slot_store; simpl; rewrite Hx; reflexivity).
    assert (Hx1 : set h b x1 b = Some x1) by (unfold set; rewrite Nat.eqb_refl; reflexivity).
    assert (Sh1 : shaped s x1) by exact Sh.
    destruct (IH (set h b x1) x1 Ok (fun j Hj => Hl j (or_intror Hj)) Hx1 Sh1) as [Hs [y [Hy [Shy [Keep Cov]]]]].
    split.
    + cbn [map holds]. split.
      * unfold safe_ev, slot_store, live. simpl.
        destruct Sh as [Sz [Rp Rc]].
        split; [exists x; split; [exact Hx | exact (slot_in_bounds s i x Ok Hi (conj Sz (conj Rp Rc)))] |].
        split; [exists x; split; [exact Hx | lia] |].
        split; [exact I | exists x; split; [exact Hx | exact Rp]].
      * rewrite E1. exact Hs.
    + exists y. unfold after. cbn [map fold_left]. rewrite E1. fold (after (set h b x1) (map (slot_store b s) l)).
      split; [exact Hy |]. split; [exact Shy |]. split.
      * intros j Hj. apply Keep. simpl. apply mark_keeps. exact Hj.
      * intros k [Hk | Hk].
        -- subst k. intros j Hj. apply Keep. simpl. apply mark_covers. exact Hj.
        -- exact (Cov k Hk).
Qed.

(* Every byte of the extent: the header, or inside some slot below COUNT. *)
Lemma extent_covered : forall s j, site_ok s = true -> 0 <= j < site_size s ->
  j < Z.of_nat (HDRs s) \/
  exists i, (i < COUNT s)%nat /\ slot_off s i <= j < slot_off s i + Z.of_nat (Ws s).
Proof.
  intros s j Ok Hj. destruct (site_ok_spec s Ok) as [Hh [Hw _]].
  destruct (Z_lt_le_dec j (Z.of_nat (HDRs s))) as [Lt | Ge]; [left; exact Lt | right].
  set (q := (j - Z.of_nat (HDRs s)) / Z.of_nat (Ws s)).
  assert (Wp : 0 < Z.of_nat (Ws s)) by lia.
  assert (Q0 : 0 <= q) by (apply Z.div_pos; lia).
  assert (Qd := Z.mul_div_le (j - Z.of_nat (HDRs s)) (Z.of_nat (Ws s)) Wp).
  assert (Qm := Z.mod_pos_bound (j - Z.of_nat (HDRs s)) (Z.of_nat (Ws s)) Wp).
  assert (Qe := Z.div_mod (j - Z.of_nat (HDRs s)) (Z.of_nat (Ws s)) ltac:(lia)).
  fold q in Qd, Qe.
  exists (Z.to_nat q). unfold slot_off, site_size in *. rewrite Z2Nat.id by lia.
  split.
  - assert (q < Z.of_nat (COUNT s)) by nia. lia.
  - split; nia.
Qed.

Theorem site_window : forall s b h, site_ok s = true -> ~ live h b ->
  holds safe_ev h (site_events b s) /\
  (forall c, c <> b -> after h (site_events b s) c = h c) /\
  exists x, after h (site_events b s) b = Some x /\ shaped s x /\
    forall j, 0 <= j < size x -> init x j = true.
Proof.
  intros s b h Ok NL. destruct (site_ok_spec s Ok) as [Hh [Hw [Cov Rng]]].
  set (x0 := mkBlock (site_size s) (Rs s) 1 (fun _ => false)).
  set (h0 := set h b x0).
  assert (H0b : h0 b = Some x0) by (unfold h0, set; rewrite Nat.eqb_refl; reflexivity).
  set (x1 := mkBlock (site_size s) (Rs s) 1 (mark (fun _ => false) 0 (Z.of_nat (HDRs s)))).
  set (h1 := set h0 b x1).
  assert (E1 : step h0 (EStore b 0 (Z.of_nat (HDRs s)) (Rs s)) = h1) by (simpl; rewrite H0b; reflexivity).
  assert (H1b : h1 b = Some x1) by (unfold h1, set; rewrite Nat.eqb_refl; reflexivity).
  assert (Sh1 : shaped s x1) by (repeat split).
  destruct (stores_window s b (STORES s) h1 x1 Ok Rng H1b Sh1) as [Hs [y [Hy [Shy [Keep Slots]]]]].
  assert (Ss : site_size s >= Z.of_nat (HDRs s)) by (unfold site_size; nia).
  assert (E0 : step h (EAlloc b (site_size s) (Rs s)) = h0) by reflexivity.
  assert (Ev : after h (site_events b s) = after h1 (map (slot_store b s) (STORES s))).
  { unfold after, site_events. cbn [fold_left]. rewrite E0, E1. reflexivity. }
  split; [| split].
  - unfold site_events. cbn [holds]. rewrite E0, E1. split; [| split].
    + unfold safe_ev. simpl. tauto.
    + unfold safe_ev, live. simpl.
      split; [exists x0; split; [exact H0b | unfold in_bounds; simpl; lia] |].
      split; [exists x0; split; [exact H0b | simpl; lia] |].
      split; [exact I | exists x0; split; [exact H0b | reflexivity]].
    + exact Hs.
  - intros c Hc. rewrite Ev. rewrite after_other.
    + unfold h1, h0, set. apply Nat.eqb_neq in Hc. rewrite Hc. reflexivity.
    + apply Forall_forall. intros e He. apply in_map_iff in He. destruct He as [i [<- _]].
      unfold slot_store. simpl. auto.
  - exists y. split.
    + rewrite Ev. exact Hy.
    + split; [exact Shy |]. intros j Hj. pose proof Shy as [Sz _]. rewrite Sz in Hj.
      destruct (extent_covered s j Ok Hj) as [Hd | [i [Hi Hin]]].
      * apply Keep. simpl. apply mark_covers. lia.
      * exact (Slots i (Cov i Hi) j Hin).
Qed.
Print Assumptions site_window.

(* ── Traces built from sites ────────────────────────────────────────────── *)

(* Every allocation in the trace is an accepted site's window, with the
   layout the site gives its block. *)
Inductive sited (m : Mem) : list Ev -> Prop :=
  | SNil : sited m []
  | SSite : forall b s t, site_ok s = true -> Lb m b = site_layout s ->
      sited m t -> sited m (site_events b s ++ t)
  | SOther : forall e t, (forall b n r, e <> EAlloc b n r) -> sited m t -> sited m (e :: t).

Lemma window_no_load : forall b s h, holds init_ev h (site_events b s).
Proof.
  intros b s. unfold site_events.
  assert (G : forall l h, holds init_ev h (map (slot_store b s) l)).
  { intro l. induction l as [| i l IH]; intro h; simpl; [exact I |]. split; [exact I | apply IH]. }
  intro h. simpl. split; [exact I |]. split; [exact I | apply G].
Qed.

Theorem sited_init : forall m t h,
  sited m t -> full_init h -> holds temporal_ev h t -> holds spatial_ev h t ->
  holds init_ev h t.
Proof.
  intros m t h S. revert h. induction S as [| b s t Ok Lay S IH | e t NA S IH]; intros h F T Sp.
  - exact I.
  - apply holds_app'. apply holds_app' in T. apply holds_app' in Sp.
    destruct T as [Tw Tt]. destruct Sp as [_ St].
    assert (NL : ~ live h b) by (simpl in Tw; tauto).
    destruct (site_window s b h Ok NL) as [_ [Oth [x [Hx [_ Full]]]]].
    split; [apply window_no_load |].
    apply IH; [| exact Tt | exact St].
    intros c y Hy i Hi. destruct (Nat.eq_dec c b) as [-> | Ne].
    + rewrite Hx in Hy. injection Hy as <-. exact (Full i Hi).
    + rewrite (Oth c Ne) in Hy. exact (F c y Hy i Hi).
  - destruct T as [_ T]. destruct Sp as [Spe Sp]. simpl. split.
    + destruct e as [b n r | b o w r | b o w r | b | b]; simpl; try exact I.
      destruct Spe as [x [Hx [Ho [_ Hend]]]]. exists x. split; [exact Hx |].
      intros i Hi. apply (F b x Hx). lia.
    + apply IH; [| exact T | exact Sp]. apply step_full_init; [exact F | exact NA].
Qed.
Print Assumptions sited_init.

Theorem sited_alloc_ok : forall m t, sited m t -> Forall (alloc_ok m) t.
Proof.
  intros m t S. induction S as [| b s t Ok Lay S IH | e t NA S IH].
  - constructor.
  - apply Forall_app. split; [| exact IH]. unfold site_events. constructor.
    + simpl. rewrite Lay. split; reflexivity.
    + constructor; [exact I |]. apply Forall_forall. intros e He.
      apply in_map_iff in He. destruct He as [i [<- _]]. exact I.
  - constructor; [| exact IH].
    destruct e as [b n r | | | |]; simpl; try exact I. exfalso. exact (NA b n r eq_refl).
Qed.
Print Assumptions sited_alloc_ok.

(* THE COMPOSITION: the RC certificate, the bounds certificate and the
   construction sites give MemorySafe — all four clauses, no stated
   hypothesis about initialization or allocation layout left. *)
Theorem certified_sited_memory_safe : forall m t p e tb o,
  wf_mem m ->
  holds fresh_alloc_ev empty_heap t ->
  (forall b, OwnershipChecker.check (gproj b t) = true) ->
  sited m t ->
  accepts p = true -> i64 e -> SpatialCert.run m e p tb o ->
  filter is_access t = tb ->
  MemorySafe t.
Proof.
  intros m t p e tb o Wf Fr Rc S Acc Hi Hrun Hf.
  assert (Al := sited_alloc_ok m t S).
  destruct (certified_with_bounds_cert m t p e tb o Wf Fr Rc Al Acc Hi Hrun Hf) as [Sp [T Ty]].
  apply memory_safe_iff_clauses. split; [exact Sp |]. split; [exact T |]. split; [| exact Ty].
  unfold Initialized. apply (sited_init m t empty_heap S); [intros b x H; discriminate | exact T | exact Sp].
Qed.
Print Assumptions certified_sited_memory_safe.

(* ── Interleaving: MemorySafe does not see the order of different blocks ── *)

(* Each clause of an event reads only its own block's state. *)
Lemma safe_ev_local : forall h1 h2 e,
  h1 (ev_blk e) = h2 (ev_blk e) -> safe_ev h1 e -> safe_ev h2 e.
Proof.
  intros h1 h2 e E S. unfold safe_ev, spatial_ev, temporal_ev, init_ev, type_ev, live in *.
  destruct e as [b n k | b o w k | b o w k | b | b]; simpl in E; rewrite <- E; exact S.
Qed.

(* And a step changes only its own block, as a function of that block. *)
Lemma step_local : forall h1 h2 e,
  h1 (ev_blk e) = h2 (ev_blk e) -> step h1 e (ev_blk e) = step h2 e (ev_blk e).
Proof.
  intros h1 h2 e E. destruct e as [b n k | b o w k | b o w k | b | b]; simpl in *.
  - unfold set. rewrite Nat.eqb_refl. reflexivity.
  - rewrite E. destruct (h2 b) as [x |] eqn:H2.
    + unfold set. rewrite Nat.eqb_refl. reflexivity.
    + rewrite H2. exact E.
  - exact E.
  - rewrite E. destruct (h2 b) as [x |] eqn:H2.
    + unfold set. rewrite Nat.eqb_refl. reflexivity.
    + rewrite H2. exact E.
  - rewrite E. destruct (h2 b) as [x |] eqn:H2.
    + destruct (0 <? rc x); [unfold set; rewrite Nat.eqb_refl; reflexivity |]. rewrite H2. exact E.
    + rewrite H2. exact E.
Qed.

Definition heq (h1 h2 : Heap) : Prop := forall b, h1 b = h2 b.

Lemma step_heq : forall h1 h2 e, heq h1 h2 -> heq (step h1 e) (step h2 e).
Proof.
  intros h1 h2 e H c. destruct (Nat.eq_dec c (ev_blk e)) as [-> | Ne].
  - apply step_local. apply H.
  - rewrite (step_other h1 e c Ne), (step_other h2 e c Ne). apply H.
Qed.

Lemma holds_heq : forall t h1 h2, heq h1 h2 -> holds safe_ev h1 t -> holds safe_ev h2 t.
Proof.
  intro t. induction t as [| e t IH]; intros h1 h2 H S; [exact I |].
  destruct S as [Se St]. split.
  - exact (safe_ev_local h1 h2 e (H _) Se).
  - exact (IH _ _ (step_heq h1 h2 e H) St).
Qed.

Lemma swap_heq : forall h e1 e2, ev_blk e1 <> ev_blk e2 ->
  heq (step (step h e1) e2) (step (step h e2) e1).
Proof.
  intros h e1 e2 Ne c.
  destruct (Nat.eq_dec c (ev_blk e2)) as [-> | N2].
  - rewrite (step_other (step h e2) e1 (ev_blk e2)) by auto.
    apply step_local. apply step_other. auto.
  - rewrite (step_other (step h e1) e2 c N2).
    destruct (Nat.eq_dec c (ev_blk e1)) as [-> | N1].
    + apply step_local. symmetry. apply step_other. auto.
    + rewrite (step_other h e1 c N1), (step_other (step h e2) e1 c N1), (step_other h e2 c N2). reflexivity.
Qed.

Lemma swap_holds : forall h e1 e2 t, ev_blk e1 <> ev_blk e2 ->
  holds safe_ev h (e1 :: e2 :: t) -> holds safe_ev h (e2 :: e1 :: t).
Proof.
  intros h e1 e2 t Ne [S1 [S2 St]]. simpl. split; [| split].
  - apply (safe_ev_local (step h e1) h e2); [apply step_other; auto | exact S2].
  - apply (safe_ev_local h (step h e2) e1); [symmetry; apply step_other; auto | exact S1].
  - exact (holds_heq t _ _ (swap_heq h e1 e2 Ne) St).
Qed.

(* One swap of two adjacent events on different blocks, anywhere. *)
Inductive swap_step : list Ev -> list Ev -> Prop :=
  | SwapAt : forall pre e1 e2 post, ev_blk e1 <> ev_blk e2 ->
      swap_step (pre ++ e1 :: e2 :: post) (pre ++ e2 :: e1 :: post).

Inductive reorder : list Ev -> list Ev -> Prop :=
  | RRefl : forall t, reorder t t
  | RStep : forall t1 t2 t3, swap_step t1 t2 -> reorder t2 t3 -> reorder t1 t3.

Lemma swap_step_safe : forall t1 t2 h, swap_step t1 t2 -> holds safe_ev h t1 -> holds safe_ev h t2.
Proof.
  intros t1 t2 h Sw. destruct Sw as [pre e1 e2 post Ne]. revert h.
  induction pre as [| e pre IH]; intros h H; simpl in *.
  - exact (swap_holds h e1 e2 post Ne H).
  - destruct H as [He Ht]. split; [exact He | exact (IH _ Ht)].
Qed.

Theorem reorder_memory_safe : forall t1 t2, reorder t1 t2 -> MemorySafe t1 -> MemorySafe t2.
Proof.
  intros t1 t2 R. induction R as [t | t1 t2 t3 Sw R IH]; intro H; [exact H |].
  apply IH. exact (swap_step_safe t1 t2 empty_heap Sw H).
Qed.
Print Assumptions reorder_memory_safe.

(* ── Controls ───────────────────────────────────────────────────────────── *)

(* A list literal of three Int elements: header 12, slots of 8, all three
   stored. *)
Example accepts_literal : site_ok (mkSite 12 8 3 0 [0; 1; 2])%nat = true.
Proof. reflexivity. Qed.

(* Stores in any order, a slot stored twice. *)
Example accepts_any_order : site_ok (mkSite 12 8 3 0 [2; 0; 2; 1])%nat = true.
Proof. reflexivity. Qed.

(* A slot never stored: the bytes there were never written. *)
Example rejects_missing_slot : site_ok (mkSite 12 8 3 0 [0; 2])%nat = false.
Proof. reflexivity. Qed.

(* A store past the end. *)
Example rejects_slot_past_end : site_ok (mkSite 12 8 2 0 [0; 1; 2])%nat = false.
Proof. reflexivity. Qed.

(* The missing slot is a real read of unwritten bytes: the window of the
   rejected site, then a load of its slot 1, is not Initialized. *)
Example missing_slot_read_uninitialized :
  ~ Initialized (site_events 0%nat (mkSite 12 8 3 0 [0; 2])%nat ++ [ELoad 0%nat 20 8 0%nat]).
Proof.
  unfold Initialized. simpl. unfold set. simpl.
  intros [_ [_ [_ [_ [[x [Hx Hi]] _]]]]]. injection Hx as <-.
  specialize (Hi 20 ltac:(lia)). simpl in Hi. discriminate.
Qed.

(* ── The certificate as bytes ───────────────────────────────────────────── *)

(* Format (whitespace between tokens is ignored):
     cert ::= site (';' site)*
     site ::= 'h' N 'w' N 'n' N 'r' N ('s' N)*
   A string that does not parse is REJECTED. The empty string and an empty
   site list are vacuous (no allocation recorded). *)

Definition chr (c d : ascii) : bool := Ascii.eqb c d.

Definition blank (c : ascii) : bool :=
  chr c " "%char || chr c "010"%char || chr c "013"%char || chr c "009"%char.

Fixpoint skip_blank (cs : list ascii) : list ascii :=
  match cs with
  | c :: r => if blank c then skip_blank r else cs
  | [] => []
  end.

Fixpoint digits_of (acc : nat) (cs : list ascii) : nat * list ascii :=
  match cs with
  | c :: r => if is_digit c then digits_of (acc * 10 + digit c) r else (acc, cs)
  | [] => (acc, [])
  end.

(* `tag N`, after blanks: the number, and the rest. *)
Definition field (tag : ascii) (cs : list ascii) : option (nat * list ascii) :=
  match skip_blank cs with
  | c :: d :: r => if chr c tag && is_digit d then Some (digits_of 0 (d :: r)) else None
  | _ => None
  end.

(* The `s N` stores, to the first `;` or the end. *)
Fixpoint stores_at (fuel : nat) (cs : list ascii) : option (list nat * list ascii) :=
  match fuel with
  | O => None
  | S f =>
      match skip_blank cs with
      | [] => Some ([], [])
      | c :: r =>
          if is_semi c then Some ([], c :: r)
          else match field "s"%char (c :: r) with
               | Some (i, r1) =>
                   match stores_at f r1 with
                   | Some (ix, r2) => Some (i :: ix, r2)
                   | None => None
                   end
               | None => None
               end
      end
  end.

Definition site_at (cs : list ascii) : option (Site * list ascii) :=
  match field "h"%char cs with
  | Some (h, r1) =>
      match field "w"%char r1 with
      | Some (w, r2) =>
          match field "n"%char r2 with
          | Some (n, r3) =>
              match field "r"%char r3 with
              | Some (k, r4) =>
                  match stores_at (S (List.length r4)) r4 with
                  | Some (ix, r5) => Some (mkSite h w n k ix, r5)
                  | None => None
                  end
              | None => None
              end
          | None => None
          end
      | None => None
      end
  | None => None
  end.

Fixpoint sites_at (fuel : nat) (cs : list ascii) : option (list Site) :=
  match fuel with
  | O => None
  | S f =>
      match skip_blank cs with
      | [] => Some []
      | _ =>
          match site_at cs with
          | Some (s, []) => Some [s]
          | Some (s, sep :: r) =>
              if is_semi sep then
                match sites_at f r with
                | Some ss => Some (s :: ss)
                | None => None
                end
              else None
          | None => None
          end
      end
  end.

Definition parse_init (s : string) : option (list Site) :=
  let cs := list_ascii_of_string s in sites_at (S (List.length cs)) cs.

Definition check_init_cert (s : string) : bool :=
  match parse_init s with
  | Some ss => forallb site_ok ss
  | None => false
  end.

(* What ACCEPT means: the string denotes sites, every one accepted — so each
   one's window, wherever it runs, is MemorySafe and leaves its block fully
   initialized (`site_window`), and a trace built from them is MemorySafe
   with the two other certificates (`certified_sited_memory_safe`). *)
Theorem check_init_cert_sound : forall s,
  check_init_cert s = true ->
  exists ss, parse_init s = Some ss /\
    forall st, In st ss -> site_ok st = true /\
      forall b h, ~ live h b ->
        holds safe_ev h (site_events b st) /\
        exists x, after h (site_events b st) b = Some x /\ shaped st x /\
          forall j, 0 <= j < size x -> init x j = true.
Proof.
  intros s H. unfold check_init_cert in H.
  destruct (parse_init s) as [ss |] eqn:Hp; [| discriminate].
  exists ss. split; [reflexivity |]. intros st Hin.
  rewrite forallb_forall in H. specialize (H st Hin). split; [exact H |].
  intros b h NL. destruct (site_window st b h H NL) as [Hs [_ Hx]]. split; [exact Hs | exact Hx].
Qed.
Print Assumptions check_init_cert_sound.

Example parses_two_sites :
  parse_init "h12 w8 n2 r0 s0 s1 ; h12w4n1r3s0" =
    Some [(mkSite 12 8 2 0 [0; 1])%nat; (mkSite 12 4 1 3 [0])%nat].
Proof. reflexivity. Qed.

Example accepts_cert : check_init_cert "h12 w8 n3 r0 s0 s1 s2" = true.
Proof. reflexivity. Qed.

Example accepts_empty : check_init_cert "" = true /\ check_init_cert "h12 w8 n0 r0" = true.
Proof. split; reflexivity. Qed.

Example rejects_cert_missing_slot : check_init_cert "h12 w8 n3 r0 s0 s2" = false.
Proof. reflexivity. Qed.

Example rejects_cert_past_end : check_init_cert "h12 w8 n2 r0 s0 s1 s2" = false.
Proof. reflexivity. Qed.

Example rejects_zero_width : check_init_cert "h12 w0 n0 r0" = false.
Proof. reflexivity. Qed.

Example rejects_garbage : check_init_cert "h12 w8 n1 r0 s0 x" = false /\
  check_init_cert "h12 n1 w8 r0 s0" = false /\ check_init_cert "h12 w8 n1 r0 s" = false.
Proof. repeat split; reflexivity. Qed.

(* The edges crates/almide-verify/src/init.rs pins too: a zero header, a tag
   apart from its digits, a trailing `;`, leading zeros, a lone `;`. *)
Example init_edges :
  check_init_cert "h0 w8 n0 r0" = false /\ check_init_cert "h 12 w8 n0 r0" = false /\
  check_init_cert "h12 w8 n1 r0 s0 ;" = true /\ check_init_cert "h012 w8 n01 r0 s000" = true /\
  check_init_cert ";" = false.
Proof. repeat split; reflexivity. Qed.
