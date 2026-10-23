(* Almide trust spine — #4005: SPATIAL safety from a static certificate.

   MemorySafe.v fixes what memory safety means over ONE run's heap trace. A
   per-build checker cannot see runs; it sees a certificate. This file is the
   first static half for the SPATIAL clause: a certificate language for the
   element reads and writes of list blocks, a checker over it, and the
   theorem that EVERY run of an accepted certificate is SPATIAL
   (`accepts_spatial`).

   An index SLOT is one (list, index expression) pair the emitter keys its
   facts by (bounds_facts.rs: `(xs, i, c)` for `xs[i + c]`); `B x` is the
   slot's block, `Lb b` that block's layout.

     PAccess x k    read element `x` (an index slot), then run k:
                    emits ELoad (B x) (HDR + x * W) W R
     PStore x k     write element `x`: emits EStore at the same offset
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

   HONEST SCOPE. Blocks of fixed length: a resize is a new block in the value
   semantics, and a copy-on-write copy keeps the count and layout, so it is
   modelled as the same block. The compiler does not emit this certificate
   yet (#4013's design note, #1696).
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
  | PStore (x : Slot) (k : Prog)
  | PCheck (x : Slot) (k : Prog)
  | PSet (x : Slot) (k : Prog)
  | PLoop (body : Prog) (k : Prog)
  | PBranch (thn els : Prog) (k : Prog).

(* The block: HDR header bytes, LEN elements of W bytes, representation R. *)
Record Layout := { HDR : Z; LEN : Z; W : Z; R : Repr }.

Definition well_formed (l : Layout) : Prop :=
  0 <= HDR l /\ 0 <= LEN l < 2 ^ 63 /\ 0 < W l.

Definition bytes_of (l : Layout) : Z := HDR l + LEN l * W l.

(* Which block each slot indexes, and each block's layout. *)
Record Mem := { B : Slot -> Blk; Lb : Blk -> Layout }.

Definition L (m : Mem) (x : Slot) : Layout := Lb m (B m x).

Definition wf_mem (m : Mem) : Prop := forall b, well_formed (Lb m b).

Definition off_of (m : Mem) (e : Slot -> Z) (x : Slot) : Z := HDR (L m x) + e x * W (L m x).

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

(* `run m e p t o`: executing `p` from slot values `e` emits trace `t` and ends
   in `o`. A failed check aborts the run; a loop runs zero or more times; a
   reassignment writes an i64. *)
Inductive run (m : Mem) : Env -> Prog -> list Ev -> Outcome -> Prop :=
  | REnd : forall e, run m e PEnd [] (Done e)
  | RAccess : forall e x k t o,
      run m e k t o ->
      run m e (PAccess x k) (ELoad (B m x) (off_of m e x) (W (L m x)) (R (L m x)) :: t) o
  | RStore : forall e x k t o,
      run m e k t o ->
      run m e (PStore x k) (EStore (B m x) (off_of m e x) (W (L m x)) (R (L m x)) :: t) o
  | RCheckPass : forall e x k t o,
      fails_check (L m x) (e x) = false ->
      run m e k t o ->
      run m e (PCheck x k) t o
  | RCheckAbort : forall e x k,
      fails_check (L m x) (e x) = true ->
      run m e (PCheck x k) [] Aborted
  | RSet : forall e x v k t o,
      is_i64 v ->
      run m (upd e x v) k t o ->
      run m e (PSet x k) t o
  | RLoopExit : forall e body k t o,
      run m e k t o ->
      run m e (PLoop body k) t o
  | RLoopAgain : forall e body k t1 e1 t2 o,
      run m e body t1 (Done e1) ->
      run m e1 (PLoop body k) t2 o ->
      run m e (PLoop body k) (t1 ++ t2) o
  | RLoopAbort : forall e body k t1,
      run m e body t1 Aborted ->
      run m e (PLoop body k) t1 Aborted
  | RBranchThen : forall e thn els k t1 e1 t2 o,
      run m e thn t1 (Done e1) ->
      run m e1 k t2 o ->
      run m e (PBranch thn els k) (t1 ++ t2) o
  | RBranchThenAbort : forall e thn els k t1,
      run m e thn t1 Aborted ->
      run m e (PBranch thn els k) t1 Aborted
  | RBranchElse : forall e thn els k t1 e1 t2 o,
      run m e els t1 (Done e1) ->
      run m e1 k t2 o ->
      run m e (PBranch thn els k) (t1 ++ t2) o
  | RBranchElseAbort : forall e thn els k t1,
      run m e els t1 Aborted ->
      run m e (PBranch thn els k) t1 Aborted.

(* A whole program: allocate the blocks `bs`, then run the body. *)
Definition alloc_ev (m : Mem) (b : Blk) : Ev := EAlloc b (bytes_of (Lb m b)) (R (Lb m b)).

Definition program_trace (m : Mem) (bs : list Blk) (t : list Ev) : list Ev :=
  map (alloc_ev m) bs ++ t.

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
  | PAccess _ k | PStore _ k | PCheck _ k => kills k
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
  | PAccess x k | PStore x k => if mem kn x then check kn k else None
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
Definition sound (m : Mem) (kn : Known) (e : Env) : Prop :=
  forall x, mem kn x = true -> 0 <= e x < LEN (L m x).

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

(* Every access an accepted certificate's run emits lies inside its block. *)
Definition inside (m : Mem) (b : Blk) (off w : Z) (r : Repr) : Prop :=
  (exists x, b = B m x) /\ r = R (Lb m b) /\ w = W (Lb m b) /\ 0 <= off /\ off + w <= bytes_of (Lb m b).

Definition in_block (m : Mem) (ev : Ev) : Prop :=
  match ev with
  | ELoad b off w r | EStore b off w r => inside m b off w r
  | _ => False
  end.

Lemma inside_slot : forall m e x,
  wf_mem m -> 0 <= e x < LEN (L m x) ->
  inside m (B m x) (off_of m e x) (W (L m x)) (R (L m x)).
Proof.
  intros m e x Hwf Hx. specialize (Hwf (B m x)).
  unfold inside, off_of, L, bytes_of, well_formed in *.
  split; [exists x; reflexivity |]. repeat split; try reflexivity; nia.
Qed.

Lemma run_sound : forall m e p t o,
  run m e p t o ->
  wf_mem m -> i64 e ->
  forall kn kn', sound m kn e -> check kn p = Some kn' ->
  Forall (in_block m) t /\ (forall e', o = Done e' -> i64 e' /\ sound m kn' e').
Proof.
  intros m e p t o Hrun Hwf.
  induction Hrun; intros Hi kn kn' Hs Hc; simpl in Hc.
  - (* REnd *)
    injection Hc as <-. split; [constructor |]. intros e' He. injection He as <-. tauto.
  - (* RAccess *)
    destruct (mem kn x) eqn:Hm; [| discriminate].
    destruct (IHHrun Hi kn kn' Hs Hc) as [Ht Ho]. split; [| exact Ho].
    constructor; [| exact Ht]. exact (inside_slot m e x Hwf (Hs x Hm)).
  - (* RStore *)
    destruct (mem kn x) eqn:Hm; [| discriminate].
    destruct (IHHrun Hi kn kn' Hs Hc) as [Ht Ho]. split; [| exact Ho].
    constructor; [| exact Ht]. exact (inside_slot m e x Hwf (Hs x Hm)).
  - (* RCheckPass *)
    apply (IHHrun Hi (x :: kn) kn'); [| exact Hc].
    intros y Hy. simpl in Hy. apply orb_true_iff in Hy. destruct Hy as [Hy | Hy].
    + apply Nat.eqb_eq in Hy. subst y. destruct (Hwf (B m x)) as [_ [Hlen _]].
      exact (unsigned_ge_excludes (LEN (L m x)) (e x) Hlen (Hi x) H).
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
    assert (Hs0 : sound m k0 e) by (intros y Hy; exact (Hs y (mem_minus _ _ _ Hy))).
    destruct (IHHrun1 Hi k0 kb Hs0 Hb) as [Ht1 Ho1].
    destruct (Ho1 e1 eq_refl) as [Hi1 Hs1].
    assert (Hs01 : sound m k0 e1) by (intros y Hy; exact (Hs1 y (subset_mem _ _ _ Hsub Hy))).
    assert (Hc2 : check k0 (PLoop body k) = Some kn').
    { simpl. assert (Hmm : minus k0 (kills body) = k0) by (unfold k0, minus; apply filter_idem).
      rewrite Hmm, Hb, Hsub. exact Hc. }
    destruct (IHHrun2 Hi1 k0 kn' Hs01 Hc2) as [Ht2 Ho2].
    split; [apply Forall_app; split; assumption | exact Ho2].
  - (* RLoopAbort *)
    destruct (check (minus kn (kills body)) body) as [kb |] eqn:Hb; [| discriminate].
    assert (Hs0 : sound m (minus kn (kills body)) e) by (intros y Hy; exact (Hs y (mem_minus _ _ _ Hy))).
    destruct (IHHrun Hi _ kb Hs0 Hb) as [Ht _]. split; [exact Ht | discriminate].
  - (* RBranchThen *)
    destruct (check kn thn) as [kt |] eqn:Ht; [| discriminate].
    destruct (check kn els) as [ke |] eqn:He; [| discriminate].
    destruct (IHHrun1 Hi kn kt Hs Ht) as [Ht1 Ho1].
    destruct (Ho1 e1 eq_refl) as [Hi1 Hs1].
    assert (Hsi : sound m (inter kt ke) e1) by (intros y Hy; exact (Hs1 y (proj1 (mem_inter _ _ _ Hy)))).
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
    assert (Hsi : sound m (inter kt ke) e1) by (intros y Hy; exact (Hs1 y (proj2 (mem_inter _ _ _ Hy)))).
    destruct (IHHrun2 Hi1 (inter kt ke) kn' Hsi Hc) as [Ht2 Ho2].
    split; [apply Forall_app; split; assumption | exact Ho2].
  - (* RBranchElseAbort *)
    destruct (check kn thn) as [kt |] eqn:Ht; [| discriminate].
    destruct (check kn els) as [ke |] eqn:He; [| discriminate].
    destruct (IHHrun Hi kn ke Hs He) as [Ht1 _]. split; [exact Ht1 | discriminate].
Qed.
Print Assumptions run_sound.

(* ── The theorem: every run of an accepted certificate is SPATIAL ─────── *)

(* What the heap must hold for block b: its size and representation. *)
Definition shaped (m : Mem) (h : Heap) (b : Blk) : Prop :=
  exists x, h b = Some x /\ size x = bytes_of (Lb m b) /\ repr x = R (Lb m b).

Lemma step_keeps_shape : forall m h b ev,
  in_block m ev -> shaped m h b -> shaped m (step h ev) b.
Proof.
  intros m h b ev Hev [x [Hx [Hsz Hr]]].
  destruct ev as [b' n k | b' o w k | b' o w k | b' | b']; simpl in Hev; try contradiction; simpl.
  - (* EStore *)
    destruct (h b') as [y |] eqn:Hy; [| exists x; tauto].
    unfold set. cbv beta. destruct (Nat.eqb b b') eqn:E.
    + apply Nat.eqb_eq in E. subst b'. rewrite Hx in Hy. injection Hy as <-.
      exists (mkBlock (size x) (repr x) (rc x) (mark (init x) o w)). simpl. rewrite Nat.eqb_refl. split; [reflexivity | tauto].
    + exists x. rewrite E. tauto.
  - (* ELoad *) exists x. tauto.
Qed.

Lemma accesses_spatial : forall m h t,
  wf_mem m -> (forall b, (exists x, b = B m x) -> shaped m h b) -> Forall (in_block m) t ->
  holds spatial_ev h t /\ holds type_ev h t.
Proof.
  intros m h t Hwf Hh Hall. revert h Hh.
  induction Hall as [| ev t Hev _ IH]; intros h Hh; simpl; [tauto |].
  assert (Hh' : forall b, (exists x, b = B m x) -> shaped m (step h ev) b)
    by (intros b Hb; exact (step_keeps_shape m h b ev Hev (Hh b Hb))).
  destruct (IH (step h ev) Hh') as [Hs Ht].
  destruct ev as [b n k | b o w k | b o w k | b | b]; simpl in Hev; try contradiction;
    destruct Hev as [Hb [Hr [Hw [Ho Hend]]]];
    destruct (Hh b Hb) as [x [Hx [Hsz Hrx]]];
    destruct (Hwf b) as [_ [_ HW]];
    (split; split; [| exact Hs | | exact Ht]);
    exists x; (split; [exact Hx |]); unfold in_bounds; try lia; congruence.
Qed.

Lemma holds_app : forall P h a b,
  holds P h (a ++ b) <-> holds P h a /\ holds P (fold_left step a h) b.
Proof.
  intros P h a. revert h. induction a as [| ev a IH]; intros h b; simpl; [tauto |].
  rewrite IH. tauto.
Qed.

Lemma allocs_shaped : forall m bs h b,
  (In b bs \/ shaped m h b) -> shaped m (fold_left step (map (alloc_ev m) bs) h) b.
Proof.
  intros m bs. induction bs as [| b0 bs IH]; intros h b Hb; simpl.
  - destruct Hb as [[] | Hb]. exact Hb.
  - apply IH. destruct (in_dec Nat.eq_dec b bs) as [Hin | Hnin]; [left; exact Hin |]. right.
    unfold alloc_ev, set. simpl. cbv beta. destruct (Nat.eqb b b0) eqn:E.
    + apply Nat.eqb_eq in E. subst b0.
      exists (mkBlock (bytes_of (Lb m b)) (R (Lb m b)) 1 (fun _ => false)). simpl. rewrite Nat.eqb_refl. split; [reflexivity | tauto].
    + destruct Hb as [[E' | Hin] | [x [Hx Hrest]]];
        [subst b0; rewrite Nat.eqb_refl in E; discriminate | contradiction |].
      exists x. simpl. rewrite E. tauto.
Qed.

Lemma allocs_spatial : forall P m bs h,
  (forall h b n k, P h (EAlloc b n k)) -> holds P h (map (alloc_ev m) bs).
Proof.
  intros P m bs. induction bs as [| b bs IH]; intros h HP; simpl; [exact I |].
  split; [apply HP | apply IH; exact HP].
Qed.

(* `accepts_spatial`: an accepted certificate, well-formed blocks that the
   program allocates, and i64 index slots give a SPATIAL trace (and a
   well-typed one: every access uses its block's representation) on EVERY
   run — every loop count, every arm, every reassigned value, and runs cut
   short by a failed check. *)
Theorem accepts_spatial : forall m bs e p t o,
  wf_mem m -> (forall x, In (B m x) bs) ->
  accepts p = true -> i64 e -> run m e p t o ->
  Spatial (program_trace m bs t) /\ WellTyped (program_trace m bs t).
Proof.
  intros m bs e p t o Hwf Hbs Hacc Hi Hrun. unfold accepts in Hacc.
  destruct (check [] p) as [kn' |] eqn:Hc; [| discriminate].
  assert (Hs : sound m [] e) by (intros x Hx; discriminate).
  destruct (run_sound m e p t o Hrun Hwf Hi [] kn' Hs Hc) as [Hall _].
  assert (Hh : forall b, (exists x, b = B m x) ->
                 shaped m (fold_left step (map (alloc_ev m) bs) empty_heap) b).
  { intros b [x ->]. apply allocs_shaped. left. apply Hbs. }
  destruct (accesses_spatial m _ t Hwf Hh Hall) as [Hsp Hty].
  unfold Spatial, WellTyped, program_trace. rewrite !holds_app.
  split; split; try assumption; apply allocs_spatial; intros; exact I.
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

(* Accepted: a checked read then a store to the same element (#3345's
   fannkuch flip). Rejected: a store with no check. *)
Example accepts_read_then_store : accepts (PCheck 0 (PAccess 0 (PStore 0 PEnd))) = true.
Proof. reflexivity. Qed.

Example rejects_unchecked_store : accepts (PStore 0 PEnd) = false.
Proof. reflexivity. Qed.

(* Rejected: a check of a different slot. *)
Example rejects_other_slot : accepts (PCheck 1 (PAccess 0 PEnd)) = false.
Proof. reflexivity. Qed.

Local Open Scope Z_scope.

(* The rejections matter: the unchecked read's run IS out of bounds. One
   block of one 8-byte element, and index 5 reads past its end. *)
Definition one_block : Mem :=
  {| B := fun _ => 0%nat; Lb := fun _ => {| HDR := 8; LEN := 1; W := 8; R := 0%nat |} |}.

Example unchecked_run_not_spatial :
  let t := [ELoad 0%nat (off_of one_block (fun _ => 5) 0%nat) 8 0%nat] in
  run one_block (fun _ => 5) (PAccess 0%nat PEnd) t (Done (fun _ => 5)) /\
  ~ Spatial (program_trace one_block [0%nat] t).
Proof.
  simpl. split.
  - apply RAccess. apply REnd.
  - unfold Spatial, program_trace. simpl. unfold set. simpl.
    intros [_ [[x [Hx Hb]] _]]. injection Hx as <-. unfold in_bounds in Hb.
    cbv [size bytes_of off_of L HDR LEN W B Lb one_block] in Hb. lia.
Qed.
Print Assumptions unchecked_run_not_spatial.

(* A negative index: the unsigned compare aborts it. Without the unsigned
   reading, `-1 >= 1` is false and the check would pass. *)
Example negative_index_aborts :
  fails_check {| HDR := 8; LEN := 1; W := 8; R := 0%nat |} (-1) = true.
Proof. reflexivity. Qed.
