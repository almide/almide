(* Almide trust spine — #4013: the two per-build certificates, composed on
   ONE trace.

   MemorySafe.v proves the guarded RC check gives TEMPORAL
   (`rc_check_gives_temporal`); SpatialCert.v proves an accepted bounds
   certificate puts every access its runs make inside its block
   (`run_sound`). Each was stated alone. A build ships both certificates for
   one artifact, and its heap trace interleaves the RC events of the one with
   the accesses of the other. This file states what the PAIR gives:

     certified_spatial_temporal : a trace whose per-block RC streams the
       guarded checker accepts, whose allocator hands out non-live blocks,
       whose allocations follow the layout, and whose every access lies
       inside its block's layout, is Spatial, Temporal and WellTyped.

     certified_with_bounds_cert : the same, with the access hypothesis
       discharged by the bounds certificate — the trace's accesses ARE a
       run of an accepted bounds program.

   The point of composing: SPATIAL needs the block to EXIST at the access
   (an access to a freed block has no size to be inside of), and only the
   RC certificate says it does. Neither certificate alone gives SPATIAL over
   a trace that frees.

   HONEST SCOPE. INITIALIZATION is not reached: neither certificate tracks
   which bytes were written. And the link between the certificates' events
   and the instructions in the bytes stays the trusted recording discipline
   (TRUSTED_BASE.md), so this is `K(c, a) accepts ⟹ MemorySafe a` minus the
   INIT clause, over the trace the witnesses describe. *)

From AlmideTrust Require Import MemorySafe SpatialCert.
From Stdlib Require Import List ZArith Lia Bool Arith.
Import ListNotations.
Open Scope Z_scope.

(* An access inside its block's layout: what the bounds certificate gives. *)
Definition access_ok (m : Mem) (ev : Ev) : Prop :=
  match ev with
  | ELoad b off w r | EStore b off w r =>
      r = R (Lb m b) /\ w = W (Lb m b) /\ 0 <= off /\ off + w <= bytes_of (Lb m b)
  | _ => True
  end.

(* An allocation of the block's layout: what the allocator call site gives. *)
Definition alloc_ok (m : Mem) (ev : Ev) : Prop :=
  match ev with
  | EAlloc b n r => n = bytes_of (Lb m b) /\ r = R (Lb m b)
  | _ => True
  end.

(* Every block the heap holds has its layout's size and representation. *)
Definition laid_out (m : Mem) (h : Heap) : Prop :=
  forall b x, h b = Some x -> size x = bytes_of (Lb m b) /\ repr x = R (Lb m b).

Lemma step_laid_out : forall m h ev, laid_out m h -> alloc_ok m ev -> laid_out m (step h ev).
Proof.
  intros m h ev L A b x Hx.
  destruct ev as [b' n k | b' o w k | b' o w k | b' | b']; simpl in *.
  - unfold set in Hx. destruct (Nat.eqb b b') eqn:E.
    + apply Nat.eqb_eq in E. subst. injection Hx as <-. simpl. destruct A as [-> ->]. auto.
    + exact (L b x Hx).
  - destruct (h b') as [y |] eqn:Hy; [| exact (L b x Hx)].
    unfold set in Hx. destruct (Nat.eqb b b') eqn:E.
    + apply Nat.eqb_eq in E. subst. injection Hx as <-. simpl. exact (L b' y Hy).
    + exact (L b x Hx).
  - exact (L b x Hx).
  - destruct (h b') as [y |] eqn:Hy; [| exact (L b x Hx)].
    unfold set in Hx. destruct (Nat.eqb b b') eqn:E.
    + apply Nat.eqb_eq in E. subst. injection Hx as <-. simpl. exact (L b' y Hy).
    + exact (L b x Hx).
  - destruct (h b') as [y |] eqn:Hy; [| exact (L b x Hx)].
    destruct (0 <? rc y); [| exact (L b x Hx)].
    unfold set in Hx. destruct (Nat.eqb b b') eqn:E.
    + apply Nat.eqb_eq in E. subst. injection Hx as <-. simpl. exact (L b' y Hy).
    + exact (L b x Hx).
Qed.

(* Given TEMPORAL (the block exists and is live at each access) and the
   layout, every access is in bounds and of the block's representation. *)
Lemma temporal_layout_spatial : forall m t h,
  wf_mem m -> laid_out m h -> holds temporal_ev h t ->
  Forall (access_ok m) t -> Forall (alloc_ok m) t ->
  holds spatial_ev h t /\ holds type_ev h t.
Proof.
  intros m t. induction t as [| ev t IH]; intros h Wf L T A Al; simpl; [tauto |].
  destruct T as [Tev T]. inversion A as [| ? ? Aev A']; subst. inversion Al as [| ? ? Alev Al']; subst.
  destruct (IH (step h ev) Wf (step_laid_out m h ev L Alev) T A' Al') as [S Ty].
  destruct ev as [b n k | b o w k | b o w k | b | b]; simpl in Tev, Aev |- *;
    try (repeat split; assumption).
  - (* EStore *)
    destruct Tev as [x [Hx Hrc]]. destruct (L b x Hx) as [Sz Rp]. destruct Aev as [-> [-> [Ho Hend]]].
    destruct (Wf b) as [_ [_ HW]].
    split; (split; [| assumption]); exists x; (split; [exact Hx |]).
    + unfold in_bounds. lia.
    + exact Rp.
  - (* ELoad *)
    destruct Tev as [x [Hx Hrc]]. destruct (L b x Hx) as [Sz Rp]. destruct Aev as [-> [-> [Ho Hend]]].
    destruct (Wf b) as [_ [_ HW]].
    split; (split; [| assumption]); exists x; (split; [exact Hx |]).
    + unfold in_bounds. lia.
    + exact Rp.
Qed.

(* `certified_spatial_temporal`: the RC certificate's acceptance, a fresh
   allocator, layout-true allocations and in-layout accesses give a trace
   that is Spatial, Temporal and WellTyped. *)
Theorem certified_spatial_temporal : forall m t,
  wf_mem m ->
  holds fresh_alloc_ev empty_heap t ->
  (forall b, OwnershipChecker.check (gproj b t) = true) ->
  Forall (alloc_ok m) t ->
  Forall (access_ok m) t ->
  Spatial t /\ Temporal t /\ WellTyped t.
Proof.
  intros m t Wf Fr Rc Al Ac.
  assert (T : Temporal t) by exact (rc_check_gives_temporal t Fr Rc).
  assert (L0 : laid_out m empty_heap) by (intros b x H; discriminate).
  destruct (temporal_layout_spatial m t empty_heap Wf L0 T Ac Al) as [S Ty].
  split; [exact S |]. split; [exact T | exact Ty].
Qed.
Print Assumptions certified_spatial_temporal.

(* ── Discharging the access hypothesis with the bounds certificate ────── *)

Definition is_access (ev : Ev) : bool :=
  match ev with ELoad _ _ _ _ | EStore _ _ _ _ => true | _ => false end.

Lemma in_block_access_ok : forall m ev, in_block m ev -> access_ok m ev.
Proof.
  intros m ev H. destruct ev; simpl in *; try contradiction; destruct H as [_ [? [? [? ?]]]]; auto.
Qed.

Lemma forall_filter_access : forall m t,
  Forall (in_block m) (filter is_access t) -> Forall (access_ok m) t.
Proof.
  intros m t. induction t as [| ev t IH]; intro H; [constructor |].
  destruct ev as [b n k | b o w k | b o w k | b | b]; simpl in H.
  - constructor; [exact I | exact (IH H)].
  - inversion H as [| a l Ha Hl]; subst. constructor; [exact (in_block_access_ok m _ Ha) | exact (IH Hl)].
  - inversion H as [| a l Ha Hl]; subst. constructor; [exact (in_block_access_ok m _ Ha) | exact (IH Hl)].
  - constructor; [exact I | exact (IH H)].
  - constructor; [exact I | exact (IH H)].
Qed.

(* `certified_with_bounds_cert`: when the trace's accesses, in order, are
   the trace of a run of an accepted bounds certificate, the access
   hypothesis is the certificate's — so the two per-build certificates
   together give Spatial, Temporal and WellTyped. *)
Theorem certified_with_bounds_cert : forall m t p e tb o,
  wf_mem m ->
  holds fresh_alloc_ev empty_heap t ->
  (forall b, OwnershipChecker.check (gproj b t) = true) ->
  Forall (alloc_ok m) t ->
  accepts p = true -> i64 e -> SpatialCert.run m e p tb o ->
  filter is_access t = tb ->
  Spatial t /\ Temporal t /\ WellTyped t.
Proof.
  intros m t p e tb o Wf Fr Rc Al Acc Hi Hrun Hf.
  unfold accepts in Acc. destruct (SpatialCert.check [] p) as [kn' |] eqn:Hc; [| discriminate].
  assert (Hs : sound m [] e) by (intros x Hx; discriminate).
  destruct (run_sound m e p tb o Hrun Wf Hi [] kn' Hs Hc) as [Hall _].
  apply (certified_spatial_temporal m t Wf Fr Rc Al).
  apply forall_filter_access. rewrite Hf. exact Hall.
Qed.
Print Assumptions certified_with_bounds_cert.

(* ── INITIALIZATION: the constructor discipline ────────────────────────── *)

(* The runtime's constructor discipline: an allocation is IMMEDIATELY
   followed by the store that initializes its whole extent (a list
   literal, a copy-on-write copy and every constructor fill the block before
   anything else touches it). Neither certificate checks this today; it is
   stated here as the one hypothesis INITIALIZATION needs, so a future
   witness knows exactly what to carry. *)
Fixpoint init_disc (t : list Ev) : Prop :=
  match t with
  | [] => True
  | EAlloc b n r :: rest =>
      match rest with
      | EStore b' o w r' :: rest' => b' = b /\ o = 0 /\ w = n /\ r' = r /\ init_disc rest'
      | _ => False
      end
  | _ :: rest => init_disc rest
  end.

(* Every block the heap holds is initialized over its whole extent. *)
Definition full_init (h : Heap) : Prop :=
  forall b x, h b = Some x -> forall i, 0 <= i < size x -> init x i = true.

Lemma mark_keeps : forall f o w i, f i = true -> mark f o w i = true.
Proof. intros f o w i H. unfold mark. destruct ((o <=? i) && (i <? o + w)); [reflexivity | exact H]. Qed.

Lemma mark_covers : forall f o w i, o <= i < o + w -> mark f o w i = true.
Proof.
  intros f o w i H. unfold mark.
  replace ((o <=? i) && (i <? o + w)) with true; [reflexivity |].
  symmetry. apply andb_true_iff. split; [apply Z.leb_le | apply Z.ltb_lt]; lia.
Qed.

(* A store, an increment or a decrement keeps every block fully initialized. *)
Lemma step_full_init : forall h ev,
  full_init h -> (forall b n r, ev <> EAlloc b n r) -> full_init (step h ev).
Proof.
  intros h ev F NA b x Hx i Hi.
  destruct ev as [b' n r | b' o w r | b' o w r | b' | b']; simpl in Hx.
  - exfalso. exact (NA b' n r eq_refl).
  - destruct (h b') as [y |] eqn:Hy; [| exact (F b x Hx i Hi)].
    unfold set in Hx. destruct (Nat.eqb b b') eqn:E.
    + apply Nat.eqb_eq in E. subst. injection Hx as <-. simpl in *. apply mark_keeps. exact (F b' y Hy i Hi).
    + exact (F b x Hx i Hi).
  - exact (F b x Hx i Hi).
  - destruct (h b') as [y |] eqn:Hy; [| exact (F b x Hx i Hi)].
    unfold set in Hx. destruct (Nat.eqb b b') eqn:E.
    + apply Nat.eqb_eq in E. subst. injection Hx as <-. simpl in *. exact (F b' y Hy i Hi).
    + exact (F b x Hx i Hi).
  - destruct (h b') as [y |] eqn:Hy; [| exact (F b x Hx i Hi)].
    destruct (0 <? rc y); [| exact (F b x Hx i Hi)].
    unfold set in Hx. destruct (Nat.eqb b b') eqn:E.
    + apply Nat.eqb_eq in E. subst. injection Hx as <-. simpl in *. exact (F b' y Hy i Hi).
    + exact (F b x Hx i Hi).
Qed.

(* An allocation followed by its full-extent store leaves every block fully
   initialized. *)
Lemma alloc_store_full_init : forall h b n r,
  full_init h -> full_init (step (step h (EAlloc b n r)) (EStore b 0 n r)).
Proof.
  intros h b n r F b2 x Hx i Hi.
  set (x0 := mkBlock n r 1 (fun _ => false)) in *.
  assert (E1 : step h (EAlloc b n r) = set h b x0) by reflexivity.
  rewrite E1 in Hx.
  assert (E2 : set h b x0 b = Some x0) by (unfold set; rewrite Nat.eqb_refl; reflexivity).
  cbn [step] in Hx. rewrite E2 in Hx.
  unfold set at 1 in Hx. destruct (Nat.eqb b2 b) eqn:E.
  - apply Nat.eqb_eq in E. subst. injection Hx as <-. simpl in *. apply mark_covers. lia.
  - unfold set in Hx. rewrite E in Hx. exact (F b2 x Hx i Hi).
Qed.

Lemma laid_loads_initialized : forall m n t h,
  (length t <= n)%nat ->
  laid_out m h -> full_init h -> init_disc t -> holds temporal_ev h t ->
  Forall (access_ok m) t -> Forall (alloc_ok m) t ->
  holds init_ev h t.
Proof.
  intros m n. induction n as [| n IH]; intros t h Hlen L F D T A Al.
  - destruct t; [exact I | simpl in Hlen; lia].
  - destruct t as [| ev t]; [exact I |].
    destruct ev as [b k r | b o w r | b o w r | b | b].
    + (* EAlloc: the next event is its full-extent store *)
      simpl in D. destruct t as [| ev2 t]; [contradiction |].
      destruct ev2 as [? ? ? | b' o w r' | ? ? ? ? | ? | ?]; try contradiction.
      destruct D as [-> [-> [-> [-> D]]]].
      inversion A as [| ? ? _ A1]; subst. inversion A1 as [| ? ? _ A2]; subst.
      inversion Al as [| ? ? Alev Al1]; subst. inversion Al1 as [| ? ? Alev2 Al2]; subst.
      destruct T as [_ [_ T]].
      simpl. split; [exact I |]. split; [exact I |].
      apply (IH t); [simpl in Hlen; lia | | apply alloc_store_full_init; exact F | exact D | exact T | exact A2 | exact Al2].
      exact (step_laid_out m _ _ (step_laid_out m h _ L Alev) Alev2).
    + (* EStore *)
      simpl in D. inversion A as [| ? ? _ A1]; subst. inversion Al as [| ? ? Alev Al1]; subst.
      destruct T as [_ T]. cbn [holds]. split; [exact I |].
      apply (IH t); [simpl in Hlen; lia | exact (step_laid_out m h _ L Alev) | | exact D | exact T | exact A1 | exact Al1].
      refine (step_full_init h _ F _). intros ? ? ? Hc. discriminate Hc.
    + (* ELoad: the block is live, laid out, fully initialized, the load in bounds *)
      simpl in D. inversion A as [| ? ? Aev A1]; subst. inversion Al as [| ? ? Alev Al1]; subst.
      destruct T as [Tev T]. destruct Tev as [x [Hx _]].
      destruct (L b x Hx) as [Sz _]. simpl in Aev. destruct Aev as [_ [_ [Ho Hend]]].
      simpl. split.
      * exists x. split; [exact Hx |]. intros i Hi. apply (F b x Hx). lia.
      * apply (IH t); [simpl in Hlen; lia | exact L | exact F | exact D | exact T | exact A1 | exact Al1].
    + (* EInc *)
      simpl in D. inversion A as [| ? ? _ A1]; subst. inversion Al as [| ? ? Alev Al1]; subst.
      destruct T as [_ T]. cbn [holds]. split; [exact I |].
      apply (IH t); [simpl in Hlen; lia | exact (step_laid_out m h _ L Alev) | | exact D | exact T | exact A1 | exact Al1].
      refine (step_full_init h _ F _). intros ? ? ? Hc. discriminate Hc.
    + (* EDec *)
      simpl in D. inversion A as [| ? ? _ A1]; subst. inversion Al as [| ? ? Alev Al1]; subst.
      destruct T as [_ T]. cbn [holds]. split; [exact I |].
      apply (IH t); [simpl in Hlen; lia | exact (step_laid_out m h _ L Alev) | | exact D | exact T | exact A1 | exact Al1].
      refine (step_full_init h _ F _). intros ? ? ? Hc. discriminate Hc.
Qed.

(* `certified_memory_safe`: the two certificates, a fresh allocator,
   layout-true allocations and the constructor discipline give a trace that
   is MemorySafe — all four clauses. *)
Theorem certified_memory_safe : forall m t p e tb o,
  wf_mem m ->
  holds fresh_alloc_ev empty_heap t ->
  (forall b, OwnershipChecker.check (gproj b t) = true) ->
  Forall (alloc_ok m) t ->
  init_disc t ->
  accepts p = true -> i64 e -> SpatialCert.run m e p tb o ->
  filter is_access t = tb ->
  MemorySafe t.
Proof.
  intros m t p e tb o Wf Fr Rc Al D Acc Hi Hrun Hf.
  destruct (certified_with_bounds_cert m t p e tb o Wf Fr Rc Al Acc Hi Hrun Hf) as [S [T Ty]].
  assert (Ac : Forall (access_ok m) t).
  { unfold accepts in Acc. destruct (SpatialCert.check [] p) as [kn' |] eqn:Hc; [| discriminate].
    assert (Hs : sound m [] e) by (intros x Hx; discriminate).
    destruct (run_sound m e p tb o Hrun Wf Hi [] kn' Hs Hc) as [Hall _].
    apply forall_filter_access. rewrite Hf. exact Hall. }
  apply memory_safe_iff_clauses. split; [exact S |]. split; [exact T |]. split; [| exact Ty].
  unfold Initialized.
  apply (laid_loads_initialized m (length t) t empty_heap); [lia | intros b x H; discriminate | intros b x H; discriminate | exact D | exact T | exact Ac | exact Al].
Qed.
Print Assumptions certified_memory_safe.

(* The discipline matters: a block read before anything wrote it is not
   Initialized, though it is in bounds and live. *)
Example read_before_init_not_initialized :
  ~ Initialized [EAlloc 0%nat 16 0%nat; ELoad 0%nat 8 8 0%nat].
Proof.
  unfold Initialized. simpl. unfold set. simpl.
  intros [_ [[x [Hx Hi]] _]]. injection Hx as <-. specialize (Hi 8 ltac:(lia)). simpl in Hi. discriminate.
Qed.

(* ── Controls ───────────────────────────────────────────────────────────── *)

(* Without the RC certificate the composition fails: a block freed (its
   count dropped to 0) and then read in bounds is not Temporal — the
   in-layout access alone does not make the trace safe. *)
Example access_after_free_not_temporal :
  ~ Temporal [EAlloc 0%nat 16 0%nat; EDec 0%nat; ELoad 0%nat 8 8 0%nat].
Proof.
  unfold Temporal. simpl. unfold set. simpl.
  intros [_ [_ [[x [Hx Hrc]] _]]]. injection Hx as <-. simpl in Hrc. lia.
Qed.
