(* Almide trust spine — ADR-0034: the TAKE-OR-RETAIN site is sound.

   OwnershipChecker.v judges each HOLDER's account of a block on its own line;
   a block's count is the sum of its holders' accounts (crates/almide-wasm/
   src/witness_paths.rs). That is exactly why a line's count of 1 is NOT
   uniqueness (#4753: a credit moved out, shared out, or held by a borrowed
   parameter's caller is another holder's account, invisible to the line).

   ADR-0034's site gets uniqueness from two facts, neither enough alone:
     STATIC  the frame holds a credit on the container X — the `u` Probe's
             guard (count > 0 on the frame's line), checked from the witness;
     DYNAMIC the shipped code tested X's runtime count for 1.
   Under the holder-sum invariant the two together leave no other holder of
   X (T1). Then, per element e loaded from slot j of X:
     TAKE    (X unique) the slot is cleared and the frame's account on e
             gains the slot's credit — e's count is unchanged (T2);
     RETAIN  (X shared) e's count gains one and so does the frame's account
             (T3);
   and both arms leave the frame's accounts EQUAL (T4) — the reason the code
   after the site is emitted and checked once. Releasing X afterwards walks
   its slots, skipping cleared (0) ones, and decrements each element by
   exactly the slots that still hold it (T5) — so a taken credit is never
   released twice. T6 restates T1 as what makes the cleared slot
   unobservable: no account but the frame's can read X.

   The model: one container X (its count, the frame's account, every other
   account, its slots as a list of handles — 0 is a cleared slot) and, per
   element handle h > 0, its count, the frame's account and every account
   outside X's slots. The invariant HS: every count is the sum of its
   accounts, and every account is nonnegative. No axioms. *)

From Stdlib Require Import ZArith List Lia Bool.
Import ListNotations.
Open Scope Z_scope.

Record St := {
  rcX : Z;               (* X's runtime count *)
  fX : Z;                (* the frame's account on X *)
  oX : Z;                (* every other holder's account on X *)
  slots : list Z;        (* X's slots: a handle, or 0 (cleared) *)
  rcE : Z -> Z;          (* each element's runtime count *)
  fE : Z -> Z;           (* the frame's account on each element *)
  oE : Z -> Z            (* every account outside X's slots and the frame *)
}.

(* how many of X's slots hold h *)
Fixpoint cnt (h : Z) (l : list Z) : Z :=
  match l with
  | [] => 0
  | x :: r => (if Z.eqb x h then 1 else 0) + cnt h r
  end.

Lemma cnt_nonneg : forall h l, 0 <= cnt h l.
Proof.
  intros h l. induction l as [| x r IH]; cbn [cnt]; [lia |].
  destruct (Z.eqb x h); lia.
Qed.

Definition HS (s : St) : Prop :=
  rcX s = fX s + oX s /\ 0 <= fX s /\ 0 <= oX s /\
  forall h, 0 < h ->
    rcE s h = fE s h + cnt h (slots s) + oE s h /\ 0 <= fE s h /\ 0 <= oE s h.

(* ── T1: static ownership × the dynamic count = exclusivity ── *)
Theorem probe_exclusive :
  forall s, HS s -> 1 <= fX s -> rcX s = 1 -> oX s = 0 /\ fX s = 1.
Proof. intros s [H1 [H2 [H3 _]]] Hf Hr. lia. Qed.

(* Neither fact alone: a frame credit with the count at 2 leaves another
   holder; the count at 1 with no frame credit is someone else's block. *)
Example frame_credit_alone_not_exclusive :
  exists s, HS s /\ 1 <= fX s /\ oX s = 1.
Proof.
  exists {| rcX := 2; fX := 1; oX := 1; slots := [];
            rcE := fun _ => 0; fE := fun _ => 0; oE := fun _ => 0 |}.
  split; [| cbn; lia].
  unfold HS; cbn. repeat split; try lia.
Qed.

Example count_alone_not_owned :
  exists s, HS s /\ rcX s = 1 /\ fX s = 0.
Proof.
  exists {| rcX := 1; fX := 0; oX := 1; slots := [];
            rcE := fun _ => 0; fE := fun _ => 0; oE := fun _ => 0 |}.
  split; [| cbn; lia].
  unfold HS; cbn. repeat split; try lia.
Qed.

(* clearing slot j *)
Fixpoint clear_at (j : nat) (l : list Z) : list Z :=
  match l, j with
  | [], _ => []
  | _ :: r, O => 0 :: r
  | x :: r, S j' => x :: clear_at j' r
  end.

(* clearing slot j removes exactly the one occurrence of its handle *)
Lemma cnt_clear_at :
  forall l j h, 0 < h -> (j < length l)%nat ->
    cnt h (clear_at j l) = cnt h l - (if Z.eqb (nth j l 0) h then 1 else 0).
Proof.
  induction l as [| x r IH]; intros j h Hh Hj; cbn in Hj; [lia |].
  destruct j as [| j]; cbn [clear_at cnt nth].
  - assert (E0 : Z.eqb 0 h = false) by (apply Z.eqb_neq; lia). rewrite E0. lia.
  - rewrite IH by lia. lia.
Qed.

Definition bump (f : Z -> Z) (e : Z) : Z -> Z :=
  fun h => if Z.eqb h e then f h + 1 else f h.

(* the TAKE arm: the slot cleared, the frame's account on e gains its credit *)
Definition take (s : St) (j : nat) : St :=
  let e := nth j (slots s) 0 in
  {| rcX := rcX s; fX := fX s; oX := oX s; slots := clear_at j (slots s);
     rcE := rcE s; fE := bump (fE s) e; oE := oE s |}.

(* the RETAIN arm: e's count and the frame's account each gain one *)
Definition retain (s : St) (j : nat) : St :=
  let e := nth j (slots s) 0 in
  {| rcX := rcX s; fX := fX s; oX := oX s; slots := slots s;
     rcE := bump (rcE s) e; fE := bump (fE s) e; oE := oE s |}.

(* ── T2: the take preserves the invariant and leaves every count as it was ── *)
Theorem take_preserves :
  forall s j, HS s -> (j < length (slots s))%nat -> 0 < nth j (slots s) 0 ->
    HS (take s j) /\ rcE (take s j) = rcE s.
Proof.
  intros s j [H1 [H2 [H3 H4]]] Hj He. split; [| reflexivity].
  unfold HS, take; cbn. split; [exact H1 |]. split; [exact H2 |]. split; [exact H3 |].
  intros h Hh. destruct (H4 h Hh) as [Ha [Hb Hc]].
  rewrite (cnt_clear_at (slots s) j h Hh Hj). unfold bump.
  destruct (Z.eqb h (nth j (slots s) 0)) eqn:E1.
  - apply Z.eqb_eq in E1. rewrite <- E1. rewrite Z.eqb_refl. lia.
  - assert (E2 : Z.eqb (nth j (slots s) 0) h = false) by (rewrite Z.eqb_sym; exact E1).
    rewrite E2. lia.
Qed.

(* ── T3: the retain preserves the invariant ── *)
Theorem retain_preserves :
  forall s j, HS s -> HS (retain s j).
Proof.
  intros s j [H1 [H2 [H3 H4]]].
  unfold HS, retain; cbn. split; [exact H1 |]. split; [exact H2 |]. split; [exact H3 |].
  intros h Hh. destruct (H4 h Hh) as [Ha [Hb Hc]]. unfold bump.
  destruct (Z.eqb h (nth j (slots s) 0)); lia.
Qed.

(* ── T4: both arms leave the frame's accounts equal ── *)
Theorem arms_join :
  forall s j, fE (take s j) = fE (retain s j) /\ fX (take s j) = fX (retain s j).
Proof. intros s j. split; reflexivity. Qed.

(* ── T5: the release walk frees each element exactly by the slots that hold it ──
   X's release from count 1 (the frame's one credit, T1) walks its slots: a
   cleared slot (0) and a static are skipped, every other slot decrements its
   handle's count once. *)
Fixpoint walk (l : list Z) (rc : Z -> Z) : Z -> Z :=
  match l with
  | [] => rc
  | x :: r => walk r (fun h => if andb (0 <? x) (Z.eqb h x) then rc h - 1 else rc h)
  end.

Lemma walk_spec :
  forall l rc h, 0 < h -> walk l rc h = rc h - cnt h l.
Proof.
  induction l as [| x r IH]; intros rc h Hh; cbn [walk cnt]; [lia |].
  rewrite IH by exact Hh. cbv beta.
  destruct (Z.eqb x h) eqn:E.
  - apply Z.eqb_eq in E. subst x.
    assert (P : (0 <? h) = true) by (apply Z.ltb_lt; exact Hh).
    rewrite P, Z.eqb_refl. cbn [andb]. lia.
  - assert (E2 : Z.eqb h x = false) by (rewrite Z.eqb_sym; exact E).
    rewrite E2, andb_false_r. lia.
Qed.

(* After the walk X is gone (its count and accounts 0, no slots) and every
   element's count is its remaining accounts — no count goes below them, so
   nothing is released twice, and a taken credit (its slot cleared) was not
   released by the walk at all. *)
Definition release_X (s : St) : St :=
  {| rcX := 0; fX := 0; oX := 0; slots := [];
     rcE := walk (slots s) (rcE s); fE := fE s; oE := oE s |}.

Theorem release_walk_once :
  forall s, HS s -> rcX s = 1 -> 1 <= fX s ->
    HS (release_X s) /\
    forall h, 0 < h -> rcE (release_X s) h = rcE s h - cnt h (slots s).
Proof.
  intros s Hs Hr Hf. destruct Hs as [H1 [H2 [H3 H4]]].
  split.
  - unfold HS, release_X; cbn [rcX fX oX slots rcE fE oE cnt].
    split; [lia |]. split; [lia |]. split; [lia |].
    intros h Hh. rewrite walk_spec by exact Hh.
    destruct (H4 h Hh) as [Ha [Hb Hc]]. repeat split; lia.
  - intros h Hh. cbn [release_X rcE]. apply walk_spec. exact Hh.
Qed.

(* The whole site, end to end: probe, take one slot, release X. The taken
   element keeps its count; the frame holds the credit the slot gave up. *)
Theorem take_then_release :
  forall s j, HS s -> rcX s = 1 -> 1 <= fX s ->
    (j < length (slots s))%nat -> 0 < nth j (slots s) 0 ->
    let e := nth j (slots s) 0 in
    HS (release_X (take s j)) /\
    rcE (release_X (take s j)) e = fE s e + 1 + oE s e.
Proof.
  intros s j Hs Hr Hf Hj He e.
  destruct (take_preserves s j Hs Hj He) as [Ht Hrc].
  destruct (release_walk_once (take s j) Ht Hr Hf) as [Hrel Hval].
  split; [exact Hrel |].
  rewrite (Hval e He). unfold take. cbn [slots rcE].
  destruct Hs as [_ [_ [_ H4]]]. destruct (H4 e He) as [Ha _].
  rewrite (cnt_clear_at (slots s) j e He Hj).
  fold e. rewrite Z.eqb_refl. lia.
Qed.

(* ── T6: under T1, the cleared slot is unobservable ── *)
Corollary take_unobservable :
  forall s, HS s -> 1 <= fX s -> rcX s = 1 -> oX s = 0.
Proof. intros s Hs Hf Hr. exact (proj1 (probe_exclusive s Hs Hf Hr)). Qed.

(* AXIOM AUDIT *)
Print Assumptions probe_exclusive.
Print Assumptions take_preserves.
Print Assumptions retain_preserves.
Print Assumptions arms_join.
Print Assumptions release_walk_once.
Print Assumptions take_then_release.
Print Assumptions take_unobservable.
