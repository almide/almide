(* Almide trust spine — #4005 A4: map slots and the runtime's routines, in
   bounds by checked contracts.

   SpatialCert.v certifies a list element read `xs[i]` against the block's
   element count. The runtime's own routines (list push / reserve, the map
   scan and its entry reads, the string helpers, the self-host `prim` loads
   and stores) read and write blocks in three more shapes, and lean on facts
   their callers established. This file is their certificate language, its
   checker, and the theorem that EVERY run of an accepted certificate is
   SPATIAL (`routine_accepts_spatial`).

   A block is `[rc @0][len @4][cap @8][payload @12, cap bytes]`
   (crates/almide-layout): its extent is PAYLOAD + cap. A routine reads

     QHdr x o w      a header word of slot x's block: o + w <= PAYLOAD.
                     It needs only that x names a live block (`blk`), which
                     the RC certificate and the parameter's heap type give.
     QAcc x s o w    payload slot `e x` of stride s, bytes o .. o + w of it
                     (o + w <= s): the load at PAYLOAD + e x * s + o. The
                     slot must be KNOWN at stride s.
     QSto / QHdrSto  the same, as stores.

   and makes a slot known by an existing test (no guard is added):

     QLen n x s      n := a count of slot x's block at stride s — the len
                     word divided by s (or the cap word): any value with
                     0 <= n and n * s <= cap. That a len word never exceeds
                     the cap word is the HEADER INVARIANT, proven below
                     (`header_invariant`) from the birth header StructuralAlloc
                     proves and the per-build check of every len-word store.
     QCheck x n s    the routine's own test `x >=u n` aborting (or skipping)
                     past it, n a count of x's block at stride s.
     QExit x n s     a loop's exit test of the same kind (#4280).
     QCall req ens body
                     a helper call: the caller proves the helper's
                     PRECONDITIONS `req` (each (x, s) known), the helper's body
                     is checked under them alone, and its POSTCONDITIONS
                     `ens` — slots its body leaves known, such as a scan's
                     entry pointer (`ent`) — are known after the call. This is
                     the contract composition (`call_contract_sound`).

   The marks of the ruled vocabulary map onto it: `blk p` is QHdr's premise,
   `idx k p W` a req entry (k, W), `room p n` a QCheck against the cap
   count, `ent e p S` an ens entry (e, S), and `wf p` the header invariant's
   len-store discipline.

   HONEST SCOPE. As for SpatialCert, what is proven is the checker's
   soundness for every run of an accepted certificate against MemorySafe's
   own SPATIAL (and TYPE) clause; that the certificate's events are the
   loads and stores the shipped bytes contain is held per build by
   almide-anchor-check, the recording discipline A1–A3 already rely on. A
   block's cap is fixed for its life (a growth is a new block from $alloc /
   $realloc, the only routines allowed to write the cap word). *)

From AlmideTrust Require Import MemorySafe.
From Stdlib Require Import List ZArith Lia Bool Arith.
Import ListNotations.
Open Scope Z_scope.

(* ── The certificate language ─────────────────────────────────────────── *)

Definition Slot := nat.

(* The header: rc, len, cap — every heap block's first PAYLOAD bytes. *)
Definition PAYLOAD : Z := 12.

Inductive Prog : Type :=
  | QEnd
  | QAcc (x : Slot) (s o w : Z) (k : Prog)
  | QSto (x : Slot) (s o w : Z) (k : Prog)
  | QHdr (x : Slot) (o w : Z) (k : Prog)
  | QHdrSto (x : Slot) (o w : Z) (k : Prog)
  | QLen (n x : Slot) (s : Z) (k : Prog)
  | QCheck (x n : Slot) (s : Z) (k : Prog)
  | QExit (x n : Slot) (s : Z) (k : Prog)
  | QSet (x : Slot) (k : Prog)
  | QLoop (body : Prog) (k : Prog)
  | QBranch (thn els : Prog) (k : Prog)
  | QCall (req ens : list (Slot * Z)) (body : Prog) (k : Prog).

(* A block: its payload capacity in bytes and its representation. *)
Record Layout := { CAP : Z; R : Repr }.

Definition well_formed (l : Layout) : Prop := 0 <= CAP l < 2 ^ 63.

Definition bytes_of (l : Layout) : Z := PAYLOAD + CAP l.

Record Mem := { B : Slot -> Blk; Lb : Blk -> Layout }.

Definition L (m : Mem) (x : Slot) : Layout := Lb m (B m x).

Definition wf_mem (m : Mem) : Prop := forall b, well_formed (Lb m b).

(* ── Runs ─────────────────────────────────────────────────────────────── *)

Definition Env := Slot -> Z.

Definition is_i64 (v : Z) : Prop := - 2 ^ 63 <= v < 2 ^ 63.
Definition i64 (e : Env) : Prop := forall x, is_i64 (e x).

Definition upd (e : Env) (x : Slot) (v : Z) : Env :=
  fun y => if Nat.eqb y x then v else e y.

Definition unsigned (v : Z) : Z := v mod 2 ^ 64.

(* The routine's compare `x >=u n`, failing when it holds. *)
Definition fails (bound v : Z) : bool := bound <=? unsigned v.

Inductive Outcome := Done (e : Env) | Aborted.

Definition slot_off (e : Env) (x : Slot) (s o : Z) : Z := PAYLOAD + e x * s + o.

Inductive run (m : Mem) : Env -> Prog -> list Ev -> Outcome -> Prop :=
  | REnd : forall e, run m e QEnd [] (Done e)
  | RAcc : forall e x s o w k t out,
      run m e k t out ->
      run m e (QAcc x s o w k) (ELoad (B m x) (slot_off e x s o) w (R (L m x)) :: t) out
  | RSto : forall e x s o w k t out,
      run m e k t out ->
      run m e (QSto x s o w k) (EStore (B m x) (slot_off e x s o) w (R (L m x)) :: t) out
  | RHdr : forall e x o w k t out,
      run m e k t out ->
      run m e (QHdr x o w k) (ELoad (B m x) o w (R (L m x)) :: t) out
  | RHdrSto : forall e x o w k t out,
      run m e k t out ->
      run m e (QHdrSto x o w k) (EStore (B m x) o w (R (L m x)) :: t) out
  (* A count of x's block: any value the header invariant allows. *)
  | RLen : forall e n x s v k t out,
      0 <= v -> v * s <= CAP (L m x) ->
      run m (upd e n v) k t out ->
      run m e (QLen n x s k) t out
  | RCheckPass : forall e x n s k t out,
      fails (e n) (e x) = false ->
      run m e k t out ->
      run m e (QCheck x n s k) t out
  | RCheckAbort : forall e x n s k,
      fails (e n) (e x) = true ->
      run m e (QCheck x n s k) [] Aborted
  | RExitPass : forall e x n s k t out,
      fails (e n) (e x) = false ->
      run m e k t out ->
      run m e (QExit x n s k) t out
  | RExitLeave : forall e x n s k,
      fails (e n) (e x) = true ->
      run m e (QExit x n s k) [] (Done e)
  | RSet : forall e x v k t out,
      is_i64 v ->
      run m (upd e x v) k t out ->
      run m e (QSet x k) t out
  | RLoopExit : forall e body k t out,
      run m e k t out ->
      run m e (QLoop body k) t out
  | RLoopAgain : forall e body k t1 e1 t2 out,
      run m e body t1 (Done e1) ->
      run m e1 (QLoop body k) t2 out ->
      run m e (QLoop body k) (t1 ++ t2) out
  | RLoopAbort : forall e body k t1,
      run m e body t1 Aborted ->
      run m e (QLoop body k) t1 Aborted
  | RBranchThen : forall e thn els k t1 e1 t2 out,
      run m e thn t1 (Done e1) ->
      run m e1 k t2 out ->
      run m e (QBranch thn els k) (t1 ++ t2) out
  | RBranchThenAbort : forall e thn els k t1,
      run m e thn t1 Aborted ->
      run m e (QBranch thn els k) t1 Aborted
  | RBranchElse : forall e thn els k t1 e1 t2 out,
      run m e els t1 (Done e1) ->
      run m e1 k t2 out ->
      run m e (QBranch thn els k) (t1 ++ t2) out
  | RBranchElseAbort : forall e thn els k t1,
      run m e els t1 Aborted ->
      run m e (QBranch thn els k) t1 Aborted
  (* A helper call runs the helper's body on the same slots, then the rest. *)
  | RCall : forall e req ens body k t1 e1 t2 out,
      run m e body t1 (Done e1) ->
      run m e1 k t2 out ->
      run m e (QCall req ens body k) (t1 ++ t2) out
  | RCallAbort : forall e req ens body k t1,
      run m e body t1 Aborted ->
      run m e (QCall req ens body k) t1 Aborted.

Definition alloc_ev (m : Mem) (b : Blk) : Ev := EAlloc b (bytes_of (Lb m b)) (R (Lb m b)).

Definition program_trace (m : Mem) (bs : list Blk) (t : list Ev) : list Ev :=
  map (alloc_ev m) bs ++ t.

(* ── The checker ──────────────────────────────────────────────────────── *)

(* Known: (slot, stride) pairs in bounds. Counts: (count, slot, stride). *)
Definition Known := list (Slot * Z).
Definition Counts := list (Slot * Slot * Z).

Definition keq (p q : Slot * Z) : bool := Nat.eqb (fst p) (fst q) && Z.eqb (snd p) (snd q).
Definition ceq (p q : Slot * Slot * Z) : bool :=
  Nat.eqb (fst (fst p)) (fst (fst q)) && Nat.eqb (snd (fst p)) (snd (fst q)) && Z.eqb (snd p) (snd q).

Definition memk (k : Known) (p : Slot * Z) : bool := existsb (keq p) k.
Definition memc (c : Counts) (p : Slot * Slot * Z) : bool := existsb (ceq p) c.

(* Forget what depended on slot y's value. *)
Definition killk (k : Known) (y : Slot) : Known := filter (fun p => negb (Nat.eqb (fst p) y)) k.
Definition killc (c : Counts) (y : Slot) : Counts := filter (fun p => negb (Nat.eqb (fst (fst p)) y)) c.

Definition inks (ks : list Slot) (y : Slot) : bool := existsb (Nat.eqb y) ks.
Definition minusk (k : Known) (ks : list Slot) : Known := filter (fun p => negb (inks ks (fst p))) k.
Definition minusc (c : Counts) (ks : list Slot) : Counts := filter (fun p => negb (inks ks (fst (fst p)))) c.

Definition interk (a b : Known) : Known := filter (memk b) a.
Definition interc (a b : Counts) : Counts := filter (memc b) a.
Definition subk (a b : Known) : bool := forallb (memk b) a.
Definition subc (a b : Counts) : bool := forallb (memc b) a.

Fixpoint kills (p : Prog) : list Slot :=
  match p with
  | QEnd => []
  | QAcc _ _ _ _ k | QSto _ _ _ _ k | QHdr _ _ _ k | QHdrSto _ _ _ k
  | QCheck _ _ _ k | QExit _ _ _ k => kills k
  | QLen n _ _ k => n :: kills k
  | QSet x k => x :: kills k
  | QLoop body k => kills body ++ kills k
  | QBranch t e k => kills t ++ kills e ++ kills k
  | QCall _ _ body k => kills body ++ kills k
  end.

Definition in_slot (s o w : Z) : bool := (0 <? s) && (0 <=? o) && (0 <? w) && (o + w <=? s).
Definition in_hdr (o w : Z) : bool := (0 <=? o) && (0 <? w) && (o + w <=? PAYLOAD).

Fixpoint check (kn : Known) (cn : Counts) (p : Prog) : option (Known * Counts) :=
  match p with
  | QEnd => Some (kn, cn)
  | QAcc x s o w k | QSto x s o w k =>
      if memk kn (x, s) && in_slot s o w then check kn cn k else None
  | QHdr _ o w k | QHdrSto _ o w k => if in_hdr o w then check kn cn k else None
  | QLen n x s k => if 0 <? s then check (killk kn n) ((n, x, s) :: killc cn n) k else None
  | QCheck x n s k => if memc cn (n, x, s) then check ((x, s) :: kn) cn k else None
  | QExit x n s k =>
      if memc cn (n, x, s) then
        match check ((x, s) :: kn) cn k with
        | Some (kb, cb) => Some (interk kn kb, interc cn cb)
        | None => None
        end
      else None
  | QSet x k => check (killk kn x) (killc cn x) k
  | QLoop body k =>
      let k0 := minusk kn (kills body) in
      let c0 := minusc cn (kills body) in
      match check k0 c0 body with
      | Some (kb, cb) => if subk k0 kb && subc c0 cb then check k0 c0 k else None
      | None => None
      end
  | QBranch t e k =>
      match check kn cn t, check kn cn e with
      | Some (kt, ct), Some (ke, ce) => check (interk kt ke) (interc ct ce) k
      | _, _ => None
      end
  | QCall req ens body k =>
      if subk req kn then
        match check req [] body with
        | Some (kb, _) =>
            if subk ens kb then check (ens ++ minusk kn (kills body)) (minusc cn (kills body)) k
            else None
        | None => None
        end
      else None
  end.

Definition accepts (p : Prog) : bool :=
  match check [] [] p with Some _ => true | None => false end.

(* ── Soundness ────────────────────────────────────────────────────────── *)

Lemma keq_spec : forall p q, keq p q = true <-> p = q.
Proof.
  intros [a b] [c d]. unfold keq. simpl. rewrite andb_true_iff, Nat.eqb_eq, Z.eqb_eq.
  split; [intros [-> ->]; reflexivity | intro H; injection H; tauto].
Qed.

Lemma ceq_spec : forall p q, ceq p q = true <-> p = q.
Proof.
  intros [[a b] c] [[d f] g]. unfold ceq. simpl. rewrite !andb_true_iff, !Nat.eqb_eq, Z.eqb_eq.
  split; [intros [[-> ->] ->]; reflexivity | intro H; injection H; tauto].
Qed.

Lemma memk_spec : forall k p, memk k p = true <-> In p k.
Proof.
  intros k p. unfold memk. rewrite existsb_exists. split.
  - intros [q [Hq Hpq]]. apply keq_spec in Hpq. subst. exact Hq.
  - intro H. exists p. split; [exact H | apply keq_spec; reflexivity].
Qed.

Lemma memc_spec : forall c p, memc c p = true <-> In p c.
Proof.
  intros c p. unfold memc. rewrite existsb_exists. split.
  - intros [q [Hq Hpq]]. apply ceq_spec in Hpq. subst. exact Hq.
  - intro H. exists p. split; [exact H | apply ceq_spec; reflexivity].
Qed.

Lemma inks_spec : forall ks y, inks ks y = true <-> In y ks.
Proof.
  intros ks y. unfold inks. rewrite existsb_exists. split.
  - intros [z [Hz E]]. apply Nat.eqb_eq in E. subst. exact Hz.
  - intro H. exists y. split; [exact H | apply Nat.eqb_refl].
Qed.

(* What the known slots and counts promise about the slot values. *)
Definition sound (m : Mem) (kn : Known) (cn : Counts) (e : Env) : Prop :=
  (forall x s, In (x, s) kn -> 0 < s /\ 0 <= e x /\ (e x + 1) * s <= CAP (L m x)) /\
  (forall n x s, In (n, x, s) cn -> 0 < s /\ 0 <= e n /\ e n * s <= CAP (L m x)).

(* The one unsigned compare excludes a negative index and one at or past
   the count, for a count below 2^63. *)
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

(* A passed test of x against a count of its block makes x known. *)
Lemma test_makes_known : forall m e x n s,
  wf_mem m -> i64 e -> 0 < s -> 0 <= e n -> e n * s <= CAP (L m x) ->
  fails (e n) (e x) = false ->
  0 <= e x /\ (e x + 1) * s <= CAP (L m x).
Proof.
  intros m e x n s Hwf Hi Hs Hn Hns Hf. unfold fails in Hf.
  destruct (Hwf (B m x)) as [Hc0 Hc1]. fold (L m x) in Hc0, Hc1.
  assert (Hn63 : 0 <= e n < 2 ^ 63) by nia.
  destruct (unsigned_ge_excludes (e n) (e x) Hn63 (Hi x) Hf) as [Hx0 Hxn].
  split; [exact Hx0 | nia].
Qed.

Definition inside (m : Mem) (b : Blk) (off w : Z) (r : Repr) : Prop :=
  (exists x, b = B m x) /\ r = R (Lb m b) /\ 0 <= off /\ 0 < w /\ off + w <= bytes_of (Lb m b).

Definition in_block (m : Mem) (ev : Ev) : Prop :=
  match ev with
  | ELoad b off w r | EStore b off w r => inside m b off w r
  | _ => False
  end.

Lemma inside_slot : forall m e x s o w,
  wf_mem m -> in_slot s o w = true -> 0 <= e x -> (e x + 1) * s <= CAP (L m x) ->
  inside m (B m x) (slot_off e x s o) w (R (L m x)).
Proof.
  intros m e x s o w Hwf Hsl Hx0 Hxs. unfold in_slot in Hsl.
  rewrite !andb_true_iff, Z.ltb_lt, Z.leb_le, Z.ltb_lt, Z.leb_le in Hsl.
  destruct Hsl as [[[Hs Ho] Hw] How].
  unfold inside, slot_off, bytes_of, L in *. unfold PAYLOAD in *.
  split; [exists x; reflexivity |]. repeat split; try reflexivity; nia.
Qed.

Lemma inside_hdr : forall m x o w,
  wf_mem m -> in_hdr o w = true -> inside m (B m x) o w (R (L m x)).
Proof.
  intros m x o w Hwf Hh. unfold in_hdr in Hh.
  rewrite !andb_true_iff, Z.leb_le, Z.ltb_lt, Z.leb_le in Hh. destruct Hh as [[Ho Hw] How].
  destruct (Hwf (B m x)) as [Hc0 _].
  unfold inside, bytes_of, L in *. unfold PAYLOAD in *.
  split; [exists x; reflexivity |]. repeat split; try reflexivity; lia.
Qed.

Lemma filter_In_k : forall (A : Type) (f : A -> bool) (l : list A) a, In a (filter f l) -> In a l /\ f a = true.
Proof. intros A f l a H. apply filter_In in H. exact H. Qed.

Lemma sound_killk : forall m kn cn e y v,
  sound m kn cn e -> sound m (killk kn y) (killc cn y) (upd e y v).
Proof.
  intros m kn cn e y v [Hk Hc]. split.
  - intros x s Hin. apply filter_In_k in Hin. destruct Hin as [Hin Hne]. simpl in Hne.
    unfold upd. destruct (Nat.eqb x y) eqn:E; [discriminate |]. exact (Hk x s Hin).
  - intros n x s Hin. apply filter_In_k in Hin. destruct Hin as [Hin Hne]. simpl in Hne.
    unfold upd. destruct (Nat.eqb n y) eqn:E; [discriminate |]. exact (Hc n x s Hin).
Qed.

Lemma sound_minus : forall m kn cn e ks,
  sound m kn cn e -> sound m (minusk kn ks) (minusc cn ks) e.
Proof.
  intros m kn cn e ks [Hk Hc]. split.
  - intros x s Hin. apply filter_In_k in Hin. exact (Hk x s (proj1 Hin)).
  - intros n x s Hin. apply filter_In_k in Hin. exact (Hc n x s (proj1 Hin)).
Qed.

Lemma sound_inter : forall m kt ct ke ce e,
  sound m kt ct e -> sound m (interk kt ke) (interc ct ce) e.
Proof.
  intros m kt ct ke ce e [Hk Hc]. split.
  - intros x s Hin. apply filter_In_k in Hin. exact (Hk x s (proj1 Hin)).
  - intros n x s Hin. apply filter_In_k in Hin. exact (Hc n x s (proj1 Hin)).
Qed.

Lemma sound_inter_r : forall m kt ct ke ce e,
  sound m ke ce e -> sound m (interk kt ke) (interc ct ce) e.
Proof.
  intros m kt ct ke ce e [Hk Hc]. split.
  - intros x s Hin. apply filter_In_k in Hin. destruct Hin as [_ Hm].
    apply memk_spec in Hm. exact (Hk x s Hm).
  - intros n x s Hin. apply filter_In_k in Hin. destruct Hin as [_ Hm].
    apply memc_spec in Hm. exact (Hc n x s Hm).
Qed.

Lemma sound_sub : forall m k0 c0 kb cb e,
  subk k0 kb = true -> subc c0 cb = true -> sound m kb cb e -> sound m k0 c0 e.
Proof.
  intros m k0 c0 kb cb e Hsk Hsc [Hk Hc]. unfold subk, subc in *.
  rewrite forallb_forall in Hsk, Hsc. split.
  - intros x s Hin. apply Hk. apply memk_spec. exact (Hsk _ Hin).
  - intros n x s Hin. apply Hc. apply memc_spec. exact (Hsc _ Hin).
Qed.

Lemma minus_idem_k : forall kn ks, minusk (minusk kn ks) ks = minusk kn ks.
Proof.
  intros kn ks. unfold minusk. induction kn as [| a kn IH]; simpl; [reflexivity |].
  destruct (negb (inks ks (fst a))) eqn:E; simpl; [rewrite E, IH; reflexivity | exact IH].
Qed.

Lemma minus_idem_c : forall cn ks, minusc (minusc cn ks) ks = minusc cn ks.
Proof.
  intros cn ks. unfold minusc. induction cn as [| a cn IH]; simpl; [reflexivity |].
  destruct (negb (inks ks (fst (fst a)))) eqn:E; simpl; [rewrite E, IH; reflexivity | exact IH].
Qed.

(* Slots a body does not reassign keep their values. *)
Lemma run_preserves : forall m e p t o,
  run m e p t o -> forall e', o = Done e' -> forall y, ~ In y (kills p) -> e' y = e y.
Proof.
  intros m e p t o Hrun. induction Hrun; intros e' Ho y Hy; simpl in Hy; try discriminate.
  - injection Ho as <-. reflexivity.
  - exact (IHHrun e' Ho y Hy).
  - exact (IHHrun e' Ho y Hy).
  - exact (IHHrun e' Ho y Hy).
  - exact (IHHrun e' Ho y Hy).
  - rewrite (IHHrun e' Ho y (fun H => Hy (or_intror H))). unfold upd.
    destruct (Nat.eqb y n) eqn:E; [apply Nat.eqb_eq in E; subst; exfalso; apply Hy; left; reflexivity | reflexivity].
  - exact (IHHrun e' Ho y Hy).
  - exact (IHHrun e' Ho y Hy).
  - injection Ho as <-. reflexivity.
  - rewrite (IHHrun e' Ho y (fun H => Hy (or_intror H))). unfold upd.
    destruct (Nat.eqb y x) eqn:E; [apply Nat.eqb_eq in E; subst; exfalso; apply Hy; left; reflexivity | reflexivity].
  - exact (IHHrun e' Ho y (fun H => Hy (in_or_app _ _ _ (or_intror H)))).
  - rewrite (IHHrun2 e' Ho y Hy). exact (IHHrun1 e1 eq_refl y (fun H => Hy (in_or_app _ _ _ (or_introl H)))).
  - rewrite (IHHrun2 e' Ho y (fun H => Hy (in_or_app _ _ _ (or_intror (in_or_app _ _ _ (or_intror H)))))).
    exact (IHHrun1 e1 eq_refl y (fun H => Hy (in_or_app _ _ _ (or_introl H)))).
  - rewrite (IHHrun2 e' Ho y (fun H => Hy (in_or_app _ _ _ (or_intror (in_or_app _ _ _ (or_intror H)))))).
    exact (IHHrun1 e1 eq_refl y (fun H => Hy (in_or_app _ _ _ (or_intror (in_or_app _ _ _ (or_introl H)))))).
  - rewrite (IHHrun2 e' Ho y (fun H => Hy (in_or_app _ _ _ (or_intror H)))).
    exact (IHHrun1 e1 eq_refl y (fun H => Hy (in_or_app _ _ _ (or_introl H)))).
Qed.

Lemma sound_frame : forall m kn cn e e' ks,
  sound m kn cn e -> (forall y, ~ In y ks -> e' y = e y) ->
  sound m (minusk kn ks) (minusc cn ks) e'.
Proof.
  intros m kn cn e e' ks [Hk Hc] Hpres. split.
  - intros x s Hin. apply filter_In_k in Hin. destruct Hin as [Hin Hn].
    assert (Hx : ~ In x ks) by (intro H; apply inks_spec in H; simpl in Hn; rewrite H in Hn; discriminate).
    rewrite (Hpres x Hx). exact (Hk x s Hin).
  - intros n x s Hin. apply filter_In_k in Hin. destruct Hin as [Hin Hn].
    assert (Hx : ~ In n ks) by (intro H; apply inks_spec in H; simpl in Hn; rewrite H in Hn; discriminate).
    rewrite (Hpres n Hx). exact (Hc n x s Hin).
Qed.

Lemma run_sound : forall m e p t o,
  run m e p t o ->
  wf_mem m -> i64 e ->
  forall kn cn kn' cn', sound m kn cn e -> check kn cn p = Some (kn', cn') ->
  Forall (in_block m) t /\ (forall e', o = Done e' -> i64 e' /\ sound m kn' cn' e').
Proof.
  intros m e p t o Hrun Hwf.
  induction Hrun; intros Hi kn cn kn' cn' Hs Hc; simpl in Hc.
  - (* REnd *)
    injection Hc as <- <-. split; [constructor |]. intros e' He. injection He as <-. tauto.
  - (* RAcc *)
    destruct (memk kn (x, s) && in_slot s o w) eqn:Hm; [| discriminate].
    apply andb_true_iff in Hm. destruct Hm as [Hm Hsl]. apply memk_spec in Hm.
    destruct (proj1 Hs x s Hm) as [_ [Hx0 Hxs]].
    destruct (IHHrun Hi kn cn kn' cn' Hs Hc) as [Ht Ho]. split; [| exact Ho].
    constructor; [exact (inside_slot m e x s o w Hwf Hsl Hx0 Hxs) | exact Ht].
  - (* RSto *)
    destruct (memk kn (x, s) && in_slot s o w) eqn:Hm; [| discriminate].
    apply andb_true_iff in Hm. destruct Hm as [Hm Hsl]. apply memk_spec in Hm.
    destruct (proj1 Hs x s Hm) as [_ [Hx0 Hxs]].
    destruct (IHHrun Hi kn cn kn' cn' Hs Hc) as [Ht Ho]. split; [| exact Ho].
    constructor; [exact (inside_slot m e x s o w Hwf Hsl Hx0 Hxs) | exact Ht].
  - (* RHdr *)
    destruct (in_hdr o w) eqn:Hh; [| discriminate].
    destruct (IHHrun Hi kn cn kn' cn' Hs Hc) as [Ht Ho]. split; [| exact Ho].
    constructor; [exact (inside_hdr m x o w Hwf Hh) | exact Ht].
  - (* RHdrSto *)
    destruct (in_hdr o w) eqn:Hh; [| discriminate].
    destruct (IHHrun Hi kn cn kn' cn' Hs Hc) as [Ht Ho]. split; [| exact Ho].
    constructor; [exact (inside_hdr m x o w Hwf Hh) | exact Ht].
  - (* RLen *)
    destruct (0 <? s) eqn:Hs0; [| discriminate]. apply Z.ltb_lt in Hs0.
    assert (Hi' : i64 (upd e n v)).
    { intro y. unfold upd. destruct (Nat.eqb y n); [| exact (Hi y)].
      destruct (Hwf (B m x)) as [_ Hc1]. fold (L m x) in Hc1. unfold is_i64. nia. }
    apply (IHHrun Hi' (killk kn n) ((n, x, s) :: killc cn n) kn' cn'); [| exact Hc].
    destruct (sound_killk m kn cn e n v Hs) as [Hk Hcn]. split; [exact Hk |].
    intros n1 x1 s1 Hin. destruct Hin as [Heq | Hin]; [| exact (Hcn n1 x1 s1 Hin)].
    injection Heq as <- <- <-. unfold upd. rewrite Nat.eqb_refl. tauto.
  - (* RCheckPass *)
    destruct (memc cn (n, x, s)) eqn:Hm; [| discriminate]. apply memc_spec in Hm.
    destruct (proj2 Hs n x s Hm) as [Hsp [Hn0 Hns]].
    destruct (test_makes_known m e x n s Hwf Hi Hsp Hn0 Hns H) as [Hx0 Hxs].
    apply (IHHrun Hi ((x, s) :: kn) cn kn' cn'); [| exact Hc].
    split; [| exact (proj2 Hs)].
    intros x1 s1 Hin. destruct Hin as [Heq | Hin]; [injection Heq as <- <-; tauto | exact (proj1 Hs x1 s1 Hin)].
  - (* RCheckAbort *)
    split; [constructor | discriminate].
  - (* RExitPass *)
    destruct (memc cn (n, x, s)) eqn:Hm; [| discriminate].
    destruct (check ((x, s) :: kn) cn k) as [[kb cb] |] eqn:Hk; [| discriminate].
    injection Hc as <- <-. apply memc_spec in Hm.
    destruct (proj2 Hs n x s Hm) as [Hsp [Hn0 Hns]].
    destruct (test_makes_known m e x n s Hwf Hi Hsp Hn0 Hns H) as [Hx0 Hxs].
    assert (Hs' : sound m ((x, s) :: kn) cn e).
    { split; [| exact (proj2 Hs)].
      intros x1 s1 Hin. destruct Hin as [Heq | Hin]; [injection Heq as <- <-; tauto | exact (proj1 Hs x1 s1 Hin)]. }
    destruct (IHHrun Hi ((x, s) :: kn) cn kb cb Hs' Hk) as [Ht Ho]. split; [exact Ht |].
    intros e' He'. destruct (Ho e' He') as [Hi' Hs1]. split; [exact Hi' |].
    exact (sound_inter_r m kn cn kb cb e' Hs1).
  - (* RExitLeave *)
    destruct (memc cn (n, x, s)) eqn:Hm; [| discriminate].
    destruct (check ((x, s) :: kn) cn k) as [[kb cb] |] eqn:Hk; [| discriminate].
    injection Hc as <- <-. split; [constructor |].
    intros e' He'. injection He' as <-. split; [exact Hi |].
    exact (sound_inter m kn cn kb cb e Hs).
  - (* RSet *)
    assert (Hi' : i64 (upd e x v)) by (intro y; unfold upd; destruct (Nat.eqb y x); [exact H | exact (Hi y)]).
    exact (IHHrun Hi' (killk kn x) (killc cn x) kn' cn' (sound_killk m kn cn e x v Hs) Hc).
  - (* RLoopExit *)
    destruct (check (minusk kn (kills body)) (minusc cn (kills body)) body) as [[kb cb] |] eqn:Hb; [| discriminate].
    destruct (subk (minusk kn (kills body)) kb && subc (minusc cn (kills body)) cb) eqn:Hsub; [| discriminate].
    exact (IHHrun Hi _ _ kn' cn' (sound_minus m kn cn e (kills body) Hs) Hc).
  - (* RLoopAgain *)
    destruct (check (minusk kn (kills body)) (minusc cn (kills body)) body) as [[kb cb] |] eqn:Hb; [| discriminate].
    destruct (subk (minusk kn (kills body)) kb && subc (minusc cn (kills body)) cb) eqn:Hsub; [| discriminate].
    apply andb_true_iff in Hsub. destruct Hsub as [Hsk Hsc].
    set (k0 := minusk kn (kills body)) in *. set (c0 := minusc cn (kills body)) in *.
    assert (Hs0 : sound m k0 c0 e) by exact (sound_minus m kn cn e (kills body) Hs).
    destruct (IHHrun1 Hi k0 c0 kb cb Hs0 Hb) as [Ht1 Ho1].
    destruct (Ho1 e1 eq_refl) as [Hi1 Hs1].
    assert (Hs01 : sound m k0 c0 e1) by exact (sound_sub m k0 c0 kb cb e1 Hsk Hsc Hs1).
    assert (Hc2 : check k0 c0 (QLoop body k) = Some (kn', cn')).
    { simpl. unfold k0, c0. rewrite minus_idem_k, minus_idem_c. fold k0 c0.
      rewrite Hb. unfold k0, c0 in Hsk, Hsc |- *. rewrite Hsk, Hsc. simpl. exact Hc. }
    destruct (IHHrun2 Hi1 k0 c0 kn' cn' Hs01 Hc2) as [Ht2 Ho2].
    split; [apply Forall_app; split; assumption | exact Ho2].
  - (* RLoopAbort *)
    destruct (check (minusk kn (kills body)) (minusc cn (kills body)) body) as [[kb cb] |] eqn:Hb; [| discriminate].
    destruct (IHHrun Hi _ _ kb cb (sound_minus m kn cn e (kills body) Hs) Hb) as [Ht _].
    split; [exact Ht | discriminate].
  - (* RBranchThen *)
    destruct (check kn cn thn) as [[kt ct] |] eqn:Ht; [| discriminate].
    destruct (check kn cn els) as [[ke ce] |] eqn:He; [| discriminate].
    destruct (IHHrun1 Hi kn cn kt ct Hs Ht) as [Ht1 Ho1].
    destruct (Ho1 e1 eq_refl) as [Hi1 Hs1].
    destruct (IHHrun2 Hi1 _ _ kn' cn' (sound_inter m kt ct ke ce e1 Hs1) Hc) as [Ht2 Ho2].
    split; [apply Forall_app; split; assumption | exact Ho2].
  - (* RBranchThenAbort *)
    destruct (check kn cn thn) as [[kt ct] |] eqn:Ht; [| discriminate].
    destruct (IHHrun Hi kn cn kt ct Hs Ht) as [Ht1 _]. split; [exact Ht1 | discriminate].
  - (* RBranchElse *)
    destruct (check kn cn thn) as [[kt ct] |] eqn:Ht; [| discriminate].
    destruct (check kn cn els) as [[ke ce] |] eqn:He; [| discriminate].
    destruct (IHHrun1 Hi kn cn ke ce Hs He) as [Ht1 Ho1].
    destruct (Ho1 e1 eq_refl) as [Hi1 Hs1].
    destruct (IHHrun2 Hi1 _ _ kn' cn' (sound_inter_r m kt ct ke ce e1 Hs1) Hc) as [Ht2 Ho2].
    split; [apply Forall_app; split; assumption | exact Ho2].
  - (* RBranchElseAbort *)
    destruct (check kn cn thn) as [[kt ct] |] eqn:Ht; [| discriminate].
    destruct (check kn cn els) as [[ke ce] |] eqn:He; [| discriminate].
    destruct (IHHrun Hi kn cn ke ce Hs He) as [Ht1 _]. split; [exact Ht1 | discriminate].
  - (* RCall *)
    destruct (subk req kn) eqn:Hreq; [| discriminate].
    destruct (check req [] body) as [[kb cb] |] eqn:Hb; [| discriminate].
    destruct (subk ens kb) eqn:Hens; [| discriminate].
    assert (Hsr : sound m req [] e).
    { split; [| intros n x s Hin; destruct Hin].
      unfold subk in Hreq. rewrite forallb_forall in Hreq.
      intros x s Hin. apply (proj1 Hs). apply memk_spec. exact (Hreq _ Hin). }
    destruct (IHHrun1 Hi req [] kb cb Hsr Hb) as [Ht1 Ho1].
    destruct (Ho1 e1 eq_refl) as [Hi1 Hs1].
    assert (Hpres := run_preserves m e body t1 (Done e1) Hrun1 e1 eq_refl).
    destruct (sound_frame m kn cn e e1 (kills body) Hs Hpres) as [Hfk Hfc].
    assert (Hse : sound m (ens ++ minusk kn (kills body)) (minusc cn (kills body)) e1).
    { split; [| exact Hfc]. intros x s Hin. apply in_app_or in Hin. destruct Hin as [Hin | Hin].
      - unfold subk in Hens. rewrite forallb_forall in Hens.
        apply (proj1 Hs1). apply memk_spec. exact (Hens _ Hin).
      - exact (Hfk x s Hin). }
    destruct (IHHrun2 Hi1 _ _ kn' cn' Hse Hc) as [Ht2 Ho2].
    split; [apply Forall_app; split; assumption | exact Ho2].
  - (* RCallAbort *)
    destruct (subk req kn) eqn:Hreq; [| discriminate].
    destruct (check req [] body) as [[kb cb] |] eqn:Hb; [| discriminate].
    assert (Hsr : sound m req [] e).
    { split; [| intros n x s Hin; destruct Hin].
      unfold subk in Hreq. rewrite forallb_forall in Hreq.
      intros x s Hin. apply (proj1 Hs). apply memk_spec. exact (Hreq _ Hin). }
    destruct (IHHrun Hi req [] kb cb Hsr Hb) as [Ht1 _]. split; [exact Ht1 | discriminate].
Qed.
Print Assumptions run_sound.

(* `call_contract_sound` (A4 item 3): a helper whose body the checker accepts
   under its preconditions `req` alone, ending with its postconditions `ens`
   known, reads and writes only inside its blocks on every run from slot
   values that meet `req`, and leaves `ens` true when it returns. This is
   what lets a caller rely on a helper's contract without reading its body. *)
Theorem call_contract_sound : forall m req ens body kb cb e t o,
  wf_mem m -> i64 e ->
  check req [] body = Some (kb, cb) -> subk ens kb = true ->
  (forall x s, In (x, s) req -> 0 < s /\ 0 <= e x /\ (e x + 1) * s <= CAP (L m x)) ->
  run m e body t o ->
  Forall (in_block m) t /\
  (forall e', o = Done e' -> forall x s, In (x, s) ens -> 0 < s /\ 0 <= e' x /\ (e' x + 1) * s <= CAP (L m x)).
Proof.
  intros m req ens body kb cb e t o Hwf Hi Hb Hens Hreq Hrun.
  assert (Hsr : sound m req [] e) by (split; [exact Hreq | intros n x s Hin; destruct Hin]).
  destruct (run_sound m e body t o Hrun Hwf Hi req [] kb cb Hsr Hb) as [Ht Ho]. split; [exact Ht |].
  intros e' He' x s Hin. destruct (Ho e' He') as [_ [Hk _]].
  unfold subk in Hens. rewrite forallb_forall in Hens.
  apply Hk. apply memk_spec. exact (Hens _ Hin).
Qed.
Print Assumptions call_contract_sound.

(* ── The theorem: every run of an accepted certificate is SPATIAL ─────── *)

Definition shaped (m : Mem) (h : Heap) (b : Blk) : Prop :=
  exists x, h b = Some x /\ size x = bytes_of (Lb m b) /\ repr x = R (Lb m b).

Lemma step_keeps_shape : forall m h b ev,
  in_block m ev -> shaped m h b -> shaped m (step h ev) b.
Proof.
  intros m h b ev Hev [x [Hx [Hsz Hr]]].
  destruct ev as [b' n k | b' o w k | b' o w k | b' | b']; simpl in Hev; try contradiction; simpl.
  - destruct (h b') as [y |] eqn:Hy; [| exists x; tauto].
    unfold set. cbv beta. destruct (Nat.eqb b b') eqn:E.
    + apply Nat.eqb_eq in E. subst b'. rewrite Hx in Hy. injection Hy as <-.
      exists (mkBlock (size x) (repr x) (rc x) (mark (init x) o w)). simpl. rewrite Nat.eqb_refl. split; [reflexivity | tauto].
    + exists x. rewrite E. tauto.
  - exists x. tauto.
Qed.

Lemma accesses_spatial : forall m h t,
  (forall b, (exists x, b = B m x) -> shaped m h b) -> Forall (in_block m) t ->
  holds spatial_ev h t /\ holds type_ev h t.
Proof.
  intros m h t Hh Hall. revert h Hh.
  induction Hall as [| ev t Hev _ IH]; intros h Hh; simpl; [tauto |].
  assert (Hh' : forall b, (exists x, b = B m x) -> shaped m (step h ev) b)
    by (intros b Hb; exact (step_keeps_shape m h b ev Hev (Hh b Hb))).
  destruct (IH (step h ev) Hh') as [Hs Ht].
  destruct ev as [b n k | b o w k | b o w k | b | b]; simpl in Hev; try contradiction;
    destruct Hev as [Hb [Hr [Ho [Hw Hend]]]];
    destruct (Hh b Hb) as [x [Hx [Hsz Hrx]]];
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

Lemma allocs_holds : forall P m bs h,
  (forall h b n k, P h (EAlloc b n k)) -> holds P h (map (alloc_ev m) bs).
Proof.
  intros P m bs. induction bs as [| b bs IH]; intros h HP; simpl; [exact I |].
  split; [apply HP | apply IH; exact HP].
Qed.

Theorem routine_accepts_spatial : forall m bs e p t o,
  wf_mem m -> (forall x, In (B m x) bs) ->
  accepts p = true -> i64 e -> run m e p t o ->
  Spatial (program_trace m bs t) /\ WellTyped (program_trace m bs t).
Proof.
  intros m bs e p t o Hwf Hbs Hacc Hi Hrun. unfold accepts in Hacc.
  destruct (check [] [] p) as [[kn' cn'] |] eqn:Hc; [| discriminate].
  assert (Hs : sound m [] [] e) by (split; intros; contradiction).
  destruct (run_sound m e p t o Hrun Hwf Hi [] [] kn' cn' Hs Hc) as [Hall _].
  assert (Hh : forall b, (exists x, b = B m x) ->
                 shaped m (fold_left step (map (alloc_ev m) bs) empty_heap) b).
  { intros b [x ->]. apply allocs_shaped. left. apply Hbs. }
  destruct (accesses_spatial m _ t Hh Hall) as [Hsp Hty].
  unfold Spatial, WellTyped, program_trace. rewrite !holds_app.
  split; split; try assumption; apply allocs_holds; intros; exact I.
Qed.
Print Assumptions routine_accepts_spatial.

(* ── The header invariant (A4 item 2) ─────────────────────────────────── *)

(* The len and cap words of every block, over a run's header events:
     HAlloc b len cap   a block born by $alloc / $realloc — StructuralAlloc
                        proves the shipped $alloc writes len and a cap of the
                        class size minus PAYLOAD, so 0 <= len <= cap;
     HLen b v g         a store of v to b's len word, under its guard g:
                        GCap  — the store follows the routine's own test of
                              v against the cap word (the grow test);
                        GShrink — v is at most the current len.
   No other event writes a header word: a cap-word store outside $alloc /
   $realloc is refused by the per-build check (almide-anchor-check), and a
   len-word store with no guard it can hold to the bytes is refused too. *)

Record Hdr := { hlen : Z; hcap : Z }.

Inductive Guard := GCap | GShrink.

Inductive HEv :=
  | HAlloc (b : Blk) (len cap : Z)
  | HLen (b : Blk) (v : Z) (g : Guard).

Definition HState := Blk -> option Hdr.

Definition hset (st : HState) (b : Blk) (h : Hdr) : HState :=
  fun b' => if Nat.eqb b' b then Some h else st b'.

Definition hstep (st : HState) (ev : HEv) : HState :=
  match ev with
  | HAlloc b len cap => hset st b {| hlen := len; hcap := cap |}
  | HLen b v _ =>
      match st b with
      | Some h => hset st b {| hlen := v; hcap := hcap h |}
      | None => st
      end
  end.

(* An event the per-build check accepts: the guard holds when it runs. *)
Definition hguarded (st : HState) (ev : HEv) : Prop :=
  match ev with
  | HAlloc _ len cap => 0 <= len <= cap
  | HLen b v GCap => 0 <= v /\ (forall h, st b = Some h -> v <= hcap h)
  | HLen b v GShrink => 0 <= v /\ (forall h, st b = Some h -> v <= hlen h)
  end.

Fixpoint hrun (st : HState) (t : list HEv) : HState :=
  match t with
  | [] => st
  | ev :: t' => hrun (hstep st ev) t'
  end.

Fixpoint hholds (st : HState) (t : list HEv) : Prop :=
  match t with
  | [] => True
  | ev :: t' => hguarded st ev /\ hholds (hstep st ev) t'
  end.

Definition hinv (st : HState) : Prop := forall b h, st b = Some h -> 0 <= hlen h <= hcap h.

Lemma hstep_inv : forall st ev, hinv st -> hguarded st ev -> hinv (hstep st ev).
Proof.
  intros st ev Hinv Hg b h Hb. destruct ev as [b0 len cap | b0 v g]; simpl in *.
  - unfold hset in Hb. destruct (Nat.eqb b b0); [injection Hb as <-; simpl; lia | exact (Hinv b h Hb)].
  - destruct (st b0) as [h0 |] eqn:H0; [| exact (Hinv b h Hb)].
    unfold hset in Hb. destruct (Nat.eqb b b0) eqn:E.
    + injection Hb as <-. simpl. apply Nat.eqb_eq in E. subst b0.
      destruct (Hinv b h0 H0) as [Hl Hc].
      destruct g; destruct Hg as [Hv Hle]; (first [specialize (Hle h0 H0) | specialize (Hle h0 eq_refl)]); lia.
    + exact (Hinv b h Hb).
Qed.

(* `header_invariant`: from the empty heap, a run of guarded header events
   keeps 0 <= len <= cap for every block at every state. *)
Theorem header_invariant : forall t st,
  hinv st -> hholds st t -> forall n, hinv (hrun st (firstn n t)).
Proof.
  induction t as [| ev t IH]; intros st Hinv Hh n.
  - destruct n; exact Hinv.
  - destruct n as [| n]; [exact Hinv |]. simpl. destruct Hh as [Hg Hh].
    exact (IH (hstep st ev) (hstep_inv st ev Hinv Hg) Hh n).
Qed.
Print Assumptions header_invariant.

(* The count a routine derives from a len word, at stride s, is a value
   QLen allows: 0 <= (len / s) and (len / s) * s <= cap. *)
Lemma len_count_le_cap : forall len cap s,
  0 <= len <= cap -> 0 < s -> 0 <= len / s /\ (len / s) * s <= cap.
Proof.
  intros len cap s [H0 H1] Hs. split.
  - apply Z.div_pos; lia.
  - pose proof (Z.mul_div_le len s Hs). lia.
Qed.
Print Assumptions len_count_le_cap.

(* ── Controls ─────────────────────────────────────────────────────────── *)

Local Open Scope nat_scope.

(* A header read needs no test. *)
Example accepts_header : accepts (QHdr 0 4 4 QEnd) = true.
Proof. reflexivity. Qed.

(* A header read past the header is refused. *)
Example rejects_header_past : accepts (QHdr 0 8 8 QEnd) = false.
Proof. reflexivity. Qed.

(* A byte loop: count of slot 0 at stride 1 into slot 1, the exit test, the
   read, the step. Accepted. *)
Example accepts_byte_loop :
  accepts (QLen 1 0 1 (QLoop (QExit 0 1 1 (QAcc 0 1 0 1 (QSet 0 QEnd))) QEnd)) = true.
Proof. reflexivity. Qed.

(* A read at stride 8 under a test made at stride 1 is refused. *)
Example rejects_stride_mismatch :
  accepts (QLen 1 0 1 (QCheck 0 1 1 (QAcc 0 8 0 8 QEnd))) = false.
Proof. reflexivity. Qed.

(* An entry field past the entry (offset 4 + width 8 > stride 8) is refused. *)
Example rejects_field_past_entry :
  accepts (QLen 1 0 8 (QCheck 0 1 8 (QAcc 0 8 4 8 QEnd))) = false.
Proof. reflexivity. Qed.

(* A test against a count of another block's slot does not cover the read. *)
Example rejects_other_block_count :
  accepts (QLen 1 2 1 (QCheck 0 1 1 (QAcc 0 1 0 1 QEnd))) = false.
Proof. reflexivity. Qed.

(* A count made stale by reassigning it no longer justifies a test. *)
Example rejects_stale_count :
  accepts (QLen 1 0 1 (QSet 1 (QCheck 0 1 1 (QAcc 0 1 0 1 QEnd)))) = false.
Proof. reflexivity. Qed.

(* A helper contract: the scan's body makes its result slot 2 (an entry
   index of the map's block) known at stride 8 by its own test against the
   block's entry count, and the caller reads the entry's value field
   (offset 4, width 4) after the call. Accepted. Its precondition is empty:
   the receiver's header is readable on `blk` alone. *)
Definition scan_body : Prog :=
  QLen 1 2 8 (QCheck 2 1 8 QEnd).

Example accepts_entry_after_scan :
  accepts (QCall [] [(2, 8%Z)] scan_body (QAcc 2 8 4 4 QEnd)) = true.
Proof. reflexivity. Qed.

(* The same read with a contract that does not promise the entry: refused. *)
Example rejects_entry_unpromised :
  accepts (QCall [] [] scan_body (QAcc 2 8 4 4 QEnd)) = false.
Proof. reflexivity. Qed.

(* A contract promising what the body does not establish: refused. *)
Example rejects_contract_lie :
  accepts (QCall [] [(3, 8%Z)] scan_body QEnd) = false.
Proof. reflexivity. Qed.

(* A precondition the caller has not proven: refused. *)
Example rejects_unmet_precondition :
  accepts (QCall [(0, 1%Z)] [] QEnd QEnd) = false.
Proof. reflexivity. Qed.

(* A helper whose body reads under its precondition, called after the
   caller's own test. Accepted. *)
Example accepts_precondition_met :
  accepts (QLen 1 0 1 (QCheck 0 1 1 (QCall [(0, 1%Z)] [] (QAcc 0 1 0 1 QEnd) QEnd))) = true.
Proof. reflexivity. Qed.

Local Open Scope Z_scope.

(* The rejections matter: the unchecked read IS out of bounds — a block of
   capacity 1, a byte read at index 5. *)
Definition one_byte : Mem := {| B := fun _ => 0%nat; Lb := fun _ => {| CAP := 1; R := 0%nat |} |}.

Example unchecked_routine_read_not_spatial :
  let t := [ELoad 0%nat (slot_off (fun _ => 5) 0%nat 1 0) 1 0%nat] in
  run one_byte (fun _ => 5) (QAcc 0%nat 1 0 1 QEnd) t (Done (fun _ => 5)) /\
  ~ Spatial (program_trace one_byte [0%nat] t).
Proof.
  simpl. split.
  - apply RAcc. apply REnd.
  - unfold Spatial, program_trace. simpl. unfold set. simpl.
    intros [_ [[x [Hx Hb]] _]]. injection Hx as <-. unfold in_bounds in Hb.
    cbv [size bytes_of slot_off PAYLOAD CAP Lb one_byte] in Hb. lia.
Qed.
Print Assumptions unchecked_routine_read_not_spatial.
