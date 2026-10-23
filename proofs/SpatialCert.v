(* Almide trust spine — #4005: SPATIAL safety from a static certificate.

   MemorySafe.v fixes what memory safety means over ONE run's heap trace. A
   per-build checker cannot see runs; it sees a certificate. This file is the
   first static half for the SPATIAL clause: a certificate language for the
   element accesses of one list block, a checker over it, and the theorem
   that EVERY run of an accepted certificate is SPATIAL (`accepts_spatial`).

     PAccess x k    read element `x` (an index slot), then run k:
                    emits ELoad b (HDR + x * W) W R
     PCheck x k     the bounds check the emitter places before a read:
                    `x >=u len` aborts the run, otherwise k runs
     PSet x k       the index slot is reassigned (to any i64)
     PLoop body k   the body runs any number of times, then k
     PBranch t e k  one of two arms runs, then k

   The checker accepts an access only when every path reaching it passes a
   check of the same slot with no reassignment in between. That turns the
   compiler's elision of a repeated check (#3345 `bounds_known`, #2319's
   hoisted count) into something the checker RE-DERIVES instead of an
   analysis it trusts: an elision the certificate cannot justify is rejected
   (the `rejects_*` controls below).

   The check is the emitter's single UNSIGNED compare
   (`emit_read_bounds_check`, crates/almide-wasm/src/emitter.rs): a negative
   i64 index reads as a value above every count, so one compare excludes
   both ends (`unsigned_ge_excludes`).

   HONEST SCOPE. One block of fixed length (a resize is a new block in the
   value semantics); loads only (a store is bounded the same way); and the
   compiler does not emit this certificate yet (#4013's design note, #1696).
   What is proven is the checker's soundness, for every run of an accepted
   certificate, against MemorySafe's own SPATIAL clause. *)

From AlmideTrust Require Import MemorySafe.
From Stdlib Require Import List ZArith Lia Bool Arith.
Import ListNotations.
Open Scope Z_scope.

(* ── The certificate language ─────────────────────────────────────────── *)

Definition Slot := nat.

(* Each instruction carries what follows it, so the type is not nested and
   every recursion below is structural. *)
Inductive Prog : Type :=
  | PEnd
  | PAccess (x : Slot) (k : Prog)
  | PCheck (x : Slot) (k : Prog)
  | PSet (x : Slot) (k : Prog)
  | PLoop (body : Prog) (k : Prog)
  | PBranch (thn els : Prog) (k : Prog).

(* The block: HDR header bytes, LEN elements of W bytes, representation R. *)
Record Layout := { HDR : Z; LEN : Z; W : Z; R : Repr }.

Definition well_formed (l : Layout) : Prop :=
  0 <= HDR l /\ 0 <= LEN l < 2 ^ 63 /\ 0 < W l.

Definition bytes_of (l : Layout) : Z := HDR l + LEN l * W l.

Definition blk : Blk := 0%nat.

(* ── Runs ─────────────────────────────────────────────────────────────── *)

(* The index slots' values: i64s, negatives included. *)
Definition Env := Slot -> Z.

Definition is_i64 (v : Z) : Prop := - 2 ^ 63 <= v < 2 ^ 63.
Definition i64 (e : Env) : Prop := forall x, is_i64 (e x).

Definition upd (e : Env) (x : Slot) (v : Z) : Env :=
  fun y => if Nat.eqb y x then v else e y.

(* The emitter's compare: `idx >=u count` on the i64 bit pattern. *)
Definition unsigned (v : Z) : Z := v mod 2 ^ 64.
Definition fails_check (l : Layout) (v : Z) : bool := LEN l <=? unsigned v.

Inductive Outcome := Done (e : Env) | Aborted.

(* `run l e p t o`: executing `p` from slot values `e` emits trace `t` and ends
   in `o`. A failed check aborts the run; a loop runs zero or more times; a
   reassignment writes an i64. *)
Inductive run (l : Layout) : Env -> Prog -> list Ev -> Outcome -> Prop :=
  | REnd : forall e, run l e PEnd [] (Done e)
  | RAccess : forall e x k t o,
      run l e k t o ->
      run l e (PAccess x k) (ELoad blk (HDR l + e x * W l) (W l) (R l) :: t) o
  | RCheckPass : forall e x k t o,
      fails_check l (e x) = false ->
      run l e k t o ->
      run l e (PCheck x k) t o
  | RCheckAbort : forall e x k,
      fails_check l (e x) = true ->
      run l e (PCheck x k) [] Aborted
  | RSet : forall e x v k t o,
      is_i64 v ->
      run l (upd e x v) k t o ->
      run l e (PSet x k) t o
  | RLoopExit : forall e body k t o,
      run l e k t o ->
      run l e (PLoop body k) t o
  | RLoopAgain : forall e body k t1 e1 t2 o,
      run l e body t1 (Done e1) ->
      run l e1 (PLoop body k) t2 o ->
      run l e (PLoop body k) (t1 ++ t2) o
  | RLoopAbort : forall e body k t1,
      run l e body t1 Aborted ->
      run l e (PLoop body k) t1 Aborted
  | RBranchThen : forall e thn els k t1 e1 t2 o,
      run l e thn t1 (Done e1) ->
      run l e1 k t2 o ->
      run l e (PBranch thn els k) (t1 ++ t2) o
  | RBranchThenAbort : forall e thn els k t1,
      run l e thn t1 Aborted ->
      run l e (PBranch thn els k) t1 Aborted
  | RBranchElse : forall e thn els k t1 e1 t2 o,
      run l e els t1 (Done e1) ->
      run l e1 k t2 o ->
      run l e (PBranch thn els k) (t1 ++ t2) o
  | RBranchElseAbort : forall e thn els k t1,
      run l e els t1 Aborted ->
      run l e (PBranch thn els k) t1 Aborted.

(* A whole program: allocate the block, then run the body. *)
Definition program_trace (l : Layout) (t : list Ev) : list Ev :=
  EAlloc blk (bytes_of l) (R l) :: t.

(* ── The checker ──────────────────────────────────────────────────────── *)

(* The slots known in bounds (membership is decidable). *)
Definition Known := list Slot.

Definition mem (k : Known) (x : Slot) : bool := existsb (Nat.eqb x) k.
Definition remove (k : Known) (x : Slot) : Known := filter (fun y => negb (Nat.eqb y x)) k.
Definition inter (a b : Known) : Known := filter (mem b) a.
Definition subset (a b : Known) : bool := forallb (mem b) a.

(* The slots a body may reassign. A loop entry cannot rely on them; the
   checker does not trust this function either — it re-checks, below, that
   what it assumed at the loop head still holds after the body. *)
Fixpoint kills (p : Prog) : list Slot :=
  match p with
  | PEnd => []
  | PAccess _ k | PCheck _ k => kills k
  | PSet x k => x :: kills k
  | PLoop body k => kills body ++ kills k
  | PBranch t e k => kills t ++ kills e ++ kills k
  end.

Definition minus (k : Known) (ks : list Slot) : Known := filter (fun y => negb (mem ks y)) k.

(* `check kn p`: Some kn' when every access in `p` is justified starting from
   the known set kn, ending with kn'; None otherwise. *)
Fixpoint check (kn : Known) (p : Prog) : option Known :=
  match p with
  | PEnd => Some kn
  | PAccess x k => if mem kn x then check kn k else None
  | PCheck x k => check (x :: kn) k
  | PSet x k => check (remove kn x) k
  | PLoop body k =>
      let k0 := minus kn (kills body) in
      match check k0 body with
      | Some kb => if subset k0 kb then check k0 k else None
      | None => None
      end
  | PBranch t e k =>
      match check kn t, check kn e with
      | Some kt, Some ke => check (inter kt ke) k
      | _, _ => None
      end
  end.

Definition accepts (p : Prog) : bool :=
  match check [] p with Some _ => true | None => false end.

(* ── Soundness ────────────────────────────────────────────────────────── *)

Lemma filter_idem : forall (A : Type) (f : A -> bool) (k : list A), filter f (filter f k) = filter f k.
Proof.
  intros A f k. induction k as [| a k IH]; simpl; [reflexivity |].
  destruct (f a) eqn:E; simpl; [rewrite E, IH; reflexivity | exact IH].
Qed.

(* What the known set promises about the slot values. *)
Definition sound (l : Layout) (kn : Known) (e : Env) : Prop :=
  forall x, mem kn x = true -> 0 <= e x < LEN l.

Lemma mem_spec : forall k x, mem k x = true <-> In x k.
Proof.
  intros k x. unfold mem. rewrite existsb_exists. split.
  - intros [y [Hy Hx]]. apply Nat.eqb_eq in Hx. subst. exact Hy.
  - intro H. exists x. split; [exact H | apply Nat.eqb_refl].
Qed.

Lemma mem_remove : forall k x y, mem (remove k y) x = true -> mem k x = true /\ x <> y.
Proof.
  intros k x y H. apply mem_spec in H. unfold remove in H. apply filter_In in H.
  destruct H as [H Hn]. split; [apply mem_spec; exact H |].
  intro E. subst. rewrite Nat.eqb_refl in Hn. discriminate.
Qed.

Lemma mem_inter : forall a b x, mem (inter a b) x = true -> mem a x = true /\ mem b x = true.
Proof.
  intros a b x H. apply mem_spec in H. unfold inter in H. apply filter_In in H.
  destruct H as [H Hb]. split; [apply mem_spec; exact H | exact Hb].
Qed.

Lemma mem_minus : forall k ks x, mem (minus k ks) x = true -> mem k x = true.
Proof.
  intros k ks x H. apply mem_spec in H. unfold minus in H. apply filter_In in H.
  apply mem_spec. tauto.
Qed.

Lemma subset_mem : forall a b x, subset a b = true -> mem a x = true -> mem b x = true.
Proof.
  intros a b x Hs Hx. unfold subset in Hs. rewrite forallb_forall in Hs.
  apply Hs. apply mem_spec. exact Hx.
Qed.

(* The emitter's one unsigned compare excludes a negative index and one at
   or past the count, for a count below 2^63 (the element count of any
   wasm32 or native allocation). *)
Lemma unsigned_ge_excludes : forall n v,
  0 <= n < 2 ^ 63 -> is_i64 v -> (n <=? unsigned v) = false -> 0 <= v < n.
Proof.
  intros n v Hn Hv Hc. apply Z.leb_gt in Hc. unfold unsigned, is_i64 in *.
  destruct (Z_lt_le_dec v 0) as [Hneg | Hpos].
  - exfalso.
    assert (Hm : v mod 2 ^ 64 = v + 2 ^ 64).
    { symmetry. apply Z.mod_unique with (q := -1); lia. }
    lia.
  - rewrite Z.mod_small in Hc by lia. lia.
Qed.
Print Assumptions unsigned_ge_excludes.

(* Every load an accepted certificate's run emits lies inside the block. *)
Definition in_block (l : Layout) (ev : Ev) : Prop :=
  match ev with
  | ELoad b off w r => b = blk /\ r = R l /\ w = W l /\ 0 <= off /\ off + w <= bytes_of l
  | _ => False
  end.

Lemma run_sound : forall l e p t o,
  run l e p t o ->
  well_formed l -> i64 e ->
  forall kn kn', sound l kn e -> check kn p = Some kn' ->
  Forall (in_block l) t /\ (forall e', o = Done e' -> i64 e' /\ sound l kn' e').
Proof.
  intros l e p t o Hrun Hwf.
  induction Hrun; intros Hi kn kn' Hs Hc; simpl in Hc.
  - (* REnd *)
    injection Hc as <-. split; [constructor |]. intros e' He. injection He as <-. tauto.
  - (* RAccess *)
    destruct (mem kn x) eqn:Hm; [| discriminate].
    destruct (IHHrun Hi kn kn' Hs Hc) as [Ht Ho]. split; [| exact Ho].
    constructor; [| exact Ht].
    specialize (Hs x Hm). unfold in_block, bytes_of, well_formed in *.
    repeat split; try reflexivity; nia.
  - (* RCheckPass *)
    apply (IHHrun Hi (x :: kn) kn'); [| exact Hc].
    intros y Hy. simpl in Hy. apply orb_true_iff in Hy. destruct Hy as [Hy | Hy].
    + apply Nat.eqb_eq in Hy. subst y. destruct Hwf as [_ [Hlen _]].
      exact (unsigned_ge_excludes (LEN l) (e x) Hlen (Hi x) H).
    + exact (Hs y Hy).
  - (* RCheckAbort *)
    split; [constructor | discriminate].
  - (* RSet *)
    assert (Hi' : i64 (upd e x v)) by (intro y; unfold upd; destruct (Nat.eqb y x); [exact H | exact (Hi y)]).
    apply (IHHrun Hi' (remove kn x) kn'); [| exact Hc].
    intros y Hy. destruct (mem_remove kn y x Hy) as [Hk Hne].
      unfold upd. destruct (Nat.eqb y x) eqn:E; [apply Nat.eqb_eq in E; contradiction |].
      exact (Hs y Hk).
  - (* RLoopExit *)
    destruct (check (minus kn (kills body)) body) as [kb |] eqn:Hb; [| discriminate].
    destruct (subset (minus kn (kills body)) kb) eqn:Hsub; [| discriminate].
    apply (IHHrun Hi (minus kn (kills body)) kn'); [| exact Hc].
    intros y Hy. exact (Hs y (mem_minus _ _ _ Hy)).
  - (* RLoopAgain *)
    destruct (check (minus kn (kills body)) body) as [kb |] eqn:Hb; [| discriminate].
    destruct (subset (minus kn (kills body)) kb) eqn:Hsub; [| discriminate].
    set (k0 := minus kn (kills body)) in *.
    assert (Hs0 : sound l k0 e) by (intros y Hy; exact (Hs y (mem_minus _ _ _ Hy))).
    destruct (IHHrun1 Hi k0 kb Hs0 Hb) as [Ht1 Ho1].
    destruct (Ho1 e1 eq_refl) as [Hi1 Hs1].
    assert (Hs01 : sound l k0 e1) by (intros y Hy; exact (Hs1 y (subset_mem _ _ _ Hsub Hy))).
    assert (Hc2 : check k0 (PLoop body k) = Some kn').
    { simpl. assert (Hmm : minus k0 (kills body) = k0) by (unfold k0, minus; apply filter_idem).
      rewrite Hmm, Hb, Hsub. exact Hc. }
    destruct (IHHrun2 Hi1 k0 kn' Hs01 Hc2) as [Ht2 Ho2].
    split; [apply Forall_app; split; assumption | exact Ho2].
  - (* RLoopAbort *)
    destruct (check (minus kn (kills body)) body) as [kb |] eqn:Hb; [| discriminate].
    assert (Hs0 : sound l (minus kn (kills body)) e) by (intros y Hy; exact (Hs y (mem_minus _ _ _ Hy))).
    destruct (IHHrun Hi _ kb Hs0 Hb) as [Ht _]. split; [exact Ht | discriminate].
  - (* RBranchThen *)
    destruct (check kn thn) as [kt |] eqn:Ht; [| discriminate].
    destruct (check kn els) as [ke |] eqn:He; [| discriminate].
    destruct (IHHrun1 Hi kn kt Hs Ht) as [Ht1 Ho1].
    destruct (Ho1 e1 eq_refl) as [Hi1 Hs1].
    assert (Hsi : sound l (inter kt ke) e1) by (intros y Hy; exact (Hs1 y (proj1 (mem_inter _ _ _ Hy)))).
    destruct (IHHrun2 Hi1 (inter kt ke) kn' Hsi Hc) as [Ht2 Ho2].
    split; [apply Forall_app; split; assumption | exact Ho2].
  - (* RBranchThenAbort *)
    destruct (check kn thn) as [kt |] eqn:Ht; [| discriminate].
    destruct (IHHrun Hi kn kt Hs Ht) as [Ht1 _]. split; [exact Ht1 | discriminate].
  - (* RBranchElse *)
    destruct (check kn thn) as [kt |] eqn:Ht; [| discriminate].
    destruct (check kn els) as [ke |] eqn:He; [| discriminate].
    destruct (IHHrun1 Hi kn ke Hs He) as [Ht1 Ho1].
    destruct (Ho1 e1 eq_refl) as [Hi1 Hs1].
    assert (Hsi : sound l (inter kt ke) e1) by (intros y Hy; exact (Hs1 y (proj2 (mem_inter _ _ _ Hy)))).
    destruct (IHHrun2 Hi1 (inter kt ke) kn' Hsi Hc) as [Ht2 Ho2].
    split; [apply Forall_app; split; assumption | exact Ho2].
  - (* RBranchElseAbort *)
    destruct (check kn thn) as [kt |] eqn:Ht; [| discriminate].
    destruct (check kn els) as [ke |] eqn:He; [| discriminate].
    destruct (IHHrun Hi kn ke Hs He) as [Ht1 _]. split; [exact Ht1 | discriminate].
Qed.
Print Assumptions run_sound.

(* ── The theorem: every run of an accepted certificate is SPATIAL ─────── *)

Definition block_of (l : Layout) : Block := mkBlock (bytes_of l) (R l) 1 (fun _ => false).

Lemma loads_spatial : forall l h t,
  well_formed l -> h blk = Some (block_of l) -> Forall (in_block l) t ->
  holds spatial_ev h t /\ holds type_ev h t.
Proof.
  intros l h t Hwf Hh Hall. induction Hall as [| ev t Hev _ IH]; simpl; [tauto |].
  destruct ev as [b n k | b o w k | b o w k | b | b]; simpl in Hev; try contradiction.
  destruct Hev as [-> [-> [-> [Ho Hw]]]]. simpl.
  destruct Hwf as [_ [_ HW]].
  split; split; try apply IH;
    exists (block_of l); split; try exact Hh; try reflexivity.
  unfold in_bounds, block_of. simpl. lia.
Qed.

(* `accepts_spatial`: an accepted certificate, a well-formed block and i64
   index slots give a SPATIAL trace (and a well-typed one: every load reads
   the block's representation) on EVERY run — every loop count, every arm,
   every reassigned value, and runs cut short by a failed check. *)
Theorem accepts_spatial : forall l e p t o,
  well_formed l -> accepts p = true -> i64 e -> run l e p t o ->
  Spatial (program_trace l t) /\ WellTyped (program_trace l t).
Proof.
  intros l e p t o Hwf Hacc Hi Hrun. unfold accepts in Hacc.
  destruct (check [] p) as [kn' |] eqn:Hc; [| discriminate].
  assert (Hs : sound l [] e) by (intros x Hx; discriminate).
  destruct (run_sound l e p t o Hrun Hwf Hi [] kn' Hs Hc) as [Hall _].
  set (h1 := step empty_heap (EAlloc blk (bytes_of l) (R l))).
  assert (Hh : h1 blk = Some (block_of l)) by (unfold h1; simpl; unfold set; reflexivity).
  destruct (loads_spatial l h1 t Hwf Hh Hall) as [Hsp Hty].
  unfold Spatial, WellTyped, program_trace. simpl. split; split; trivial; exact I.
Qed.
Print Assumptions accepts_spatial.

(* ── Controls ─────────────────────────────────────────────────────────── *)

(* Slot numbers below are nats. *)
Local Open Scope nat_scope.

(* Accepted: a check, then two reads with the second check elided (#3345). *)
Example accepts_elided_repeat : accepts (PCheck 0 (PAccess 0 (PAccess 0 PEnd))) = true.
Proof. reflexivity. Qed.

(* Accepted: a loop that re-checks after each reassignment. *)
Example accepts_loop_recheck :
  accepts (PLoop (PSet 0 (PCheck 0 (PAccess 0 PEnd))) PEnd) = true.
Proof. reflexivity. Qed.

(* Accepted: a check before the loop, a body that never reassigns the slot
   (#2319's hoisted check). *)
Example accepts_hoisted :
  accepts (PCheck 0 (PLoop (PAccess 0 (PSet 1 PEnd)) PEnd)) = true.
Proof. reflexivity. Qed.

(* Rejected: a read with no check. *)
Example rejects_unchecked : accepts (PAccess 0 PEnd) = false.
Proof. reflexivity. Qed.

(* Rejected: a check, a reassignment, then the read the check no longer covers. *)
Example rejects_stale_check : accepts (PCheck 0 (PSet 0 (PAccess 0 PEnd))) = false.
Proof. reflexivity. Qed.

(* Rejected: a check hoisted out of a loop whose body reassigns the slot. *)
Example rejects_hoist_past_reassign :
  accepts (PCheck 0 (PLoop (PAccess 0 (PSet 0 PEnd)) PEnd)) = false.
Proof. reflexivity. Qed.

(* Rejected: only one arm checks. *)
Example rejects_one_arm :
  accepts (PBranch (PCheck 0 PEnd) PEnd (PAccess 0 PEnd)) = false.
Proof. reflexivity. Qed.

(* Rejected: a check of a different slot. *)
Example rejects_other_slot : accepts (PCheck 1 (PAccess 0 PEnd)) = false.
Proof. reflexivity. Qed.

Local Open Scope Z_scope.

(* The rejections matter: the unchecked read's run IS out of bounds. A block
   of one 8-byte element and index 5 reads past the end. *)
Example unchecked_run_not_spatial :
  let l := {| HDR := 8; LEN := 1; W := 8; R := 0%nat |} in
  let t := [ELoad blk (HDR l + 5 * W l) (W l) (R l)] in
  run l (fun _ => 5) (PAccess 0%nat PEnd) t (Done (fun _ => 5)) /\
  ~ Spatial (program_trace l t).
Proof.
  simpl. split.
  - apply RAccess. apply REnd.
  - unfold Spatial, program_trace. simpl. unfold set. simpl.
    intros [_ [[x [Hx Hb]] _]]. injection Hx as <-. unfold in_bounds in Hb. cbv [size bytes_of HDR LEN W] in Hb. lia.
Qed.
Print Assumptions unchecked_run_not_spatial.

(* A negative index: the unsigned compare aborts it. Without the unsigned
   reading, `-1 >= 1` is false and the check would pass. *)
Example negative_index_aborts :
  fails_check {| HDR := 8; LEN := 1; W := 8; R := 0%nat |} (-1) = true.
Proof. reflexivity. Qed.
