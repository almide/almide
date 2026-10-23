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
