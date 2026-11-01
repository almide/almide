(* Almide trust spine — #4005 A4 items 4 and 5: the self-host bodies'
   procedures and their signed tests, in bounds by checked contracts.

   RoutineBounds.v certifies a routine whose helpers are spliced in at the
   call (QCall inlines the body). The self-host stdlib bodies are written as
   RECURSIVE helpers over Int arguments,

     fn __copy(dst: Int, src: Int, n: Int, i: Int) -> Unit =
       if i >= n then () else { prim.store64(dst + 12 + i * 8, ...); __copy(dst, src, n, i + 1) }

   and test their indices with SIGNED compares. This file is their
   certificate language: RoutineBounds' items, plus

     PCall f args    a call of NAMED procedure f, its parameter slots bound
                     to the caller's slots `args`. Each procedure's body is
                     checked ONCE, under its declared precondition alone; a
                     call (a recursive one included) is accepted when the
                     caller proves that precondition of the slots it passes.
                     Soundness is by induction on the run, so recursion needs
                     no unfolding (item 4).
     PCheck/PExit x n s true
                     the SIGNED test `x >=s n`: it excludes an index at or
                     past the count only for an x known not negative (item 5).
     PConst x c      x := c, a non-negative constant: x is not negative.
     PStep x y c s   x := y + c (0 <= c <= s) with y a known index at stride
                     s: no wrap is possible (y + c <= (y + 1) * s <= cap), so
                     x is not negative — the recursion's `i + 1`.

   A slot names a block and an index into it (RoutineBounds); a procedure's
   slots are its own frame's, so each frame has its own slot-to-block map,
   and a call binds the callee's parameter slots to the blocks and values of
   the caller's argument slots. Slots that index ONE block (the counter and
   its `i + 1`) are declared a GROUP: each slot maps to a representative
   slot, the frame's block map agrees across a group, and a count is a
   count of a group's block. A call is accepted only if the caller's
   argument slots for one callee group lie in one caller group. A call
   changes no slot of the caller (each frame's slots are its locals) and no cap (a block's cap is fixed for its
   life), so every fact the caller holds survives it.

   HONEST SCOPE. As for RoutineBounds: what is proven is the checker's
   soundness for every run of accepted certificates against MemorySafe's own
   SPATIAL and TYPE clauses. That the certificate's events are the loads,
   stores, compares and calls the shipped bytes contain is held per build by
   almide-anchor-check. *)

From AlmideTrust Require Import MemorySafe RoutineBounds.
From Stdlib Require Import List ZArith Lia Bool Arith.
Import ListNotations.
Open Scope Z_scope.

(* ── The language ─────────────────────────────────────────────────────── *)

Inductive PProg : Type :=
  | PEnd
  | PAcc (x : Slot) (s o w : Z) (k : PProg)
  | PSto (x : Slot) (s o w : Z) (k : PProg)
  | PHdr (x : Slot) (o w : Z) (k : PProg)
  | PHdrSto (x : Slot) (o w : Z) (k : PProg)
  | PLen (n x : Slot) (s : Z) (k : PProg)
  | PCheck (x n : Slot) (s : Z) (sgn : bool) (k : PProg)
  | PExit (x n : Slot) (s : Z) (sgn : bool) (k : PProg)
  | PSet (x : Slot) (k : PProg)
  | PConst (x : Slot) (c : Z) (k : PProg)
  | PStep (x y : Slot) (c s : Z) (k : PProg)
  | PLoop (body : PProg) (k : PProg)
  | PBranch (thn els : PProg) (k : PProg)
  | PCall (f : nat) (args : list (Slot * Slot)) (k : PProg).

(* Not-negative slots. *)
Definition NN := list Slot.

(* A procedure: its slot groups (slot -> representative), its precondition
   over its own slots — known indices, counts (count slot, parameter slot,
   stride), not-negative slots — and its body. *)
Definition Grp := list (Slot * Slot).
Definition Pre := (Known * Counts * NN)%type.
Definition Procs := list (Grp * Pre * PProg).

Definition gof (gs : Grp) (x : Slot) : Slot :=
  match find (fun a => Nat.eqb (fst a) x) gs with Some (_, r) => r | None => x end.

(* A frame's block map agrees across each group. *)
Definition resp (gs : Grp) (bm : Slot -> Blk) : Prop := forall a, bm a = bm (gof gs a).

Definition Lay := Blk -> Layout.
Definition Bm := Slot -> Blk.

Definition wf_lay (lb : Lay) : Prop := forall b, well_formed (lb b).

(* The compare `x >= n` failing: unsigned, or signed. *)
Definition failsp (sgn : bool) (bound v : Z) : bool :=
  if sgn then bound <=? v else fails bound v.

(* The two's-complement wrap of an i64 add. *)
Definition wrap (v : Z) : Z := (v + 2 ^ 63) mod 2 ^ 64 - 2 ^ 63.

Definition poff (e : Env) (x : Slot) (s o : Z) : Z := PAYLOAD + e x * s + o.

Section Run.
Variable P : Procs.
Variable lb : Lay.
Variable ok : Blk -> Prop.

Inductive prun : Bm -> Env -> PProg -> list Ev -> Outcome -> Prop :=
  | PREnd : forall bm e, prun bm e PEnd [] (Done e)
  | PRAcc : forall bm e x s o w k t out,
      prun bm e k t out ->
      prun bm e (PAcc x s o w k) (ELoad (bm x) (poff e x s o) w (R (lb (bm x))) :: t) out
  | PRSto : forall bm e x s o w k t out,
      prun bm e k t out ->
      prun bm e (PSto x s o w k) (EStore (bm x) (poff e x s o) w (R (lb (bm x))) :: t) out
  | PRHdr : forall bm e x o w k t out,
      prun bm e k t out ->
      prun bm e (PHdr x o w k) (ELoad (bm x) o w (R (lb (bm x))) :: t) out
  | PRHdrSto : forall bm e x o w k t out,
      prun bm e k t out ->
      prun bm e (PHdrSto x o w k) (EStore (bm x) o w (R (lb (bm x))) :: t) out
  | PRLen : forall bm e n x s v k t out,
      0 <= v -> v * s <= CAP (lb (bm x)) ->
      prun bm (upd e n v) k t out ->
      prun bm e (PLen n x s k) t out
  | PRCheckPass : forall bm e x n s g k t out,
      failsp g (e n) (e x) = false ->
      prun bm e k t out ->
      prun bm e (PCheck x n s g k) t out
  | PRCheckAbort : forall bm e x n s g k,
      failsp g (e n) (e x) = true ->
      prun bm e (PCheck x n s g k) [] Aborted
  | PRExitPass : forall bm e x n s g k t out,
      failsp g (e n) (e x) = false ->
      prun bm e k t out ->
      prun bm e (PExit x n s g k) t out
  | PRExitLeave : forall bm e x n s g k,
      failsp g (e n) (e x) = true ->
      prun bm e (PExit x n s g k) [] (Done e)
  | PRSet : forall bm e x v k t out,
      is_i64 v ->
      prun bm (upd e x v) k t out ->
      prun bm e (PSet x k) t out
  | PRConst : forall bm e x c k t out,
      is_i64 c ->
      prun bm (upd e x c) k t out ->
      prun bm e (PConst x c k) t out
  | PRStep : forall bm e x y c s k t out,
      prun bm (upd e x (wrap (e y + c))) k t out ->
      prun bm e (PStep x y c s k) t out
  | PRLoopExit : forall bm e body k t out,
      prun bm e k t out ->
      prun bm e (PLoop body k) t out
  | PRLoopAgain : forall bm e body k t1 e1 t2 out,
      prun bm e body t1 (Done e1) ->
      prun bm e1 (PLoop body k) t2 out ->
      prun bm e (PLoop body k) (t1 ++ t2) out
  | PRLoopAbort : forall bm e body k t1,
      prun bm e body t1 Aborted ->
      prun bm e (PLoop body k) t1 Aborted
  | PRBranchThen : forall bm e thn els k t1 e1 t2 out,
      prun bm e thn t1 (Done e1) ->
      prun bm e1 k t2 out ->
      prun bm e (PBranch thn els k) (t1 ++ t2) out
  | PRBranchThenAbort : forall bm e thn els k t1,
      prun bm e thn t1 Aborted ->
      prun bm e (PBranch thn els k) t1 Aborted
  | PRBranchElse : forall bm e thn els k t1 e1 t2 out,
      prun bm e els t1 (Done e1) ->
      prun bm e1 k t2 out ->
      prun bm e (PBranch thn els k) (t1 ++ t2) out
  | PRBranchElseAbort : forall bm e thn els k t1,
      prun bm e els t1 Aborted ->
      prun bm e (PBranch thn els k) t1 Aborted
  (* A call runs the procedure's body in a frame of its own: each parameter
     slot is bound to its argument slot's block and value, every other slot
     of the callee to any live block and any value. The caller's slots are
     untouched; the callee's final slots are dropped. *)
  | PRCall : forall bm e f args k gs' pre body bm' e' t1 e1 t2 out,
      nth_error P f = Some (gs', pre, body) ->
      (forall y, ok (bm' y)) -> resp gs' bm' -> i64 e' ->
      (forall p x, In (p, x) args -> bm' p = bm x /\ e' p = e x) ->
      prun bm' e' body t1 (Done e1) ->
      prun bm e k t2 out ->
      prun bm e (PCall f args k) (t1 ++ t2) out
  | PRCallAbort : forall bm e f args k gs' pre body bm' e' t1,
      nth_error P f = Some (gs', pre, body) ->
      (forall y, ok (bm' y)) -> resp gs' bm' -> i64 e' ->
      (forall p x, In (p, x) args -> bm' p = bm x /\ e' p = e x) ->
      prun bm' e' body t1 Aborted ->
      prun bm e (PCall f args k) t1 Aborted.
End Run.

(* ── The checker ──────────────────────────────────────────────────────── *)

Definition killn (nn : NN) (y : Slot) : NN := filter (fun z => negb (Nat.eqb z y)) nn.
Definition minusn (nn : NN) (ks : list Slot) : NN := filter (fun z => negb (inks ks z)) nn.
Definition intern (a b : NN) : NN := filter (inks b) a.
Definition subn (a b : NN) : bool := forallb (inks b) a.

(* x is known not negative: a not-negative fact, or a known index. *)
Definition nonneg (kn : Known) (nn : NN) (x : Slot) : bool :=
  inks nn x || existsb (fun p => Nat.eqb (fst p) x) kn.

Fixpoint pkills (p : PProg) : list Slot :=
  match p with
  | PEnd => []
  | PAcc _ _ _ _ k | PSto _ _ _ _ k | PHdr _ _ _ k | PHdrSto _ _ _ k
  | PCheck _ _ _ _ k | PExit _ _ _ _ k | PCall _ _ k => pkills k
  | PLen n _ _ k => n :: pkills k
  | PSet x k | PConst x _ k | PStep x _ _ _ k => x :: pkills k
  | PLoop body k => pkills body ++ pkills k
  | PBranch t e k => pkills t ++ pkills e ++ pkills k
  end.

Definition amap (args : list (Slot * Slot)) (p : Slot) : option Slot :=
  match find (fun a => Nat.eqb (fst a) p) args with Some (_, x) => Some x | None => None end.

(* The argument slots of one callee group lie in one caller group. *)
Definition groups_agree (gs gs' : Grp) (args : list (Slot * Slot)) : bool :=
  forallb (fun a1 => forallb (fun a2 =>
    negb (Nat.eqb (gof gs' (fst a1)) (gof gs' (fst a2))) || Nat.eqb (gof gs (snd a1)) (gof gs (snd a2))) args) args.

(* The caller (groups gs) proves the callee's (groups gs') precondition of
   the slots it passes. A precondition count (n, r, s) names the callee's
   parameter slot r, whose block is its argument's. *)
Definition pre_met (gs gs' : Grp) (kn : Known) (cn : Counts) (nn : NN) (args : list (Slot * Slot)) (pre : Pre) : bool :=
  let '(kq, cq, nq) := pre in
  groups_agree gs gs' args &&
  forallb (fun q => match amap args (fst q) with Some x => memk kn (x, snd q) | None => false end) kq &&
  forallb (fun q => match amap args (fst (fst q)), amap args (snd (fst q)) with
                    | Some n, Some x => memc cn (n, gof gs x, snd q)
                    | _, _ => false end) cq &&
  forallb (fun y => match amap args y with Some x => nonneg kn nn x | None => false end) nq.

Definition St := (Known * Counts * NN)%type.

(* The checker over one frame, its slot groups gs: a count is keyed by the
   representative slot of the block it counts. *)
Fixpoint pcheck (P : Procs) (gs : Grp) (kn : Known) (cn : Counts) (nn : NN) (p : PProg) : option St :=
  match p with
  | PEnd => Some (kn, cn, nn)
  | PAcc x s o w k | PSto x s o w k =>
      if memk kn (x, s) && in_slot s o w then pcheck P gs kn cn nn k else None
  | PHdr _ o w k | PHdrSto _ o w k => if in_hdr o w then pcheck P gs kn cn nn k else None
  | PLen n x s k =>
      if 0 <? s then pcheck P gs (killk kn n) ((n, gof gs x, s) :: killc cn n) (n :: killn nn n) k else None
  | PCheck x n s g k =>
      if memc cn (n, gof gs x, s) && (negb g || nonneg kn nn x) then pcheck P gs ((x, s) :: kn) cn nn k else None
  | PExit x n s g k =>
      if memc cn (n, gof gs x, s) && (negb g || nonneg kn nn x) then
        match pcheck P gs ((x, s) :: kn) cn nn k with
        | Some (kb, cb, nb) => Some (interk kn kb, interc cn cb, intern nn nb)
        | None => None
        end
      else None
  | PSet x k => pcheck P gs (killk kn x) (killc cn x) (killn nn x) k
  | PConst x c k =>
      if 0 <=? c then pcheck P gs (killk kn x) (killc cn x) (x :: killn nn x) k
      else pcheck P gs (killk kn x) (killc cn x) (killn nn x) k
  | PStep x y c s k =>
      if memk kn (y, s) && (0 <=? c) && (c <=? s) then
        pcheck P gs (killk kn x) (killc cn x) (x :: killn nn x) k
      else pcheck P gs (killk kn x) (killc cn x) (killn nn x) k
  | PLoop body k =>
      let k0 := minusk kn (pkills body) in
      let c0 := minusc cn (pkills body) in
      let n0 := minusn nn (pkills body) in
      match pcheck P gs k0 c0 n0 body with
      | Some (kb, cb, nb) =>
          if subk k0 kb && subc c0 cb && subn n0 nb then pcheck P gs k0 c0 n0 k else None
      | None => None
      end
  | PBranch t e k =>
      match pcheck P gs kn cn nn t, pcheck P gs kn cn nn e with
      | Some (kt, ct, nt), Some (ke, ce, ne) =>
          pcheck P gs (interk kt ke) (interc ct ce) (intern nt ne) k
      | _, _ => None
      end
  | PCall f args k =>
      match nth_error P f with
      | Some (gs', pre, _) => if pre_met gs gs' kn cn nn args pre then pcheck P gs kn cn nn k else None
      | None => None
      end
  end.

(* Every procedure's body is accepted under its precondition alone. *)
Definition procs_ok (P : Procs) : bool :=
  forallb (fun pb => let '(gs, (kq, cq, nq), body) := pb in
                     match pcheck P gs kq cq nq body with Some _ => true | None => false end) P.

Definition paccepts (P : Procs) (gs : Grp) (p : PProg) : bool :=
  procs_ok P && match pcheck P gs [] [] [] p with Some _ => true | None => false end.

(* ── Soundness ────────────────────────────────────────────────────────── *)

(* A count (n, r, s) counts the block of slot r. *)
Definition psound (lb : Lay) (bm : Bm) (kn : Known) (cn : Counts) (nn : NN) (e : Env) : Prop :=
  (forall x s, In (x, s) kn -> 0 < s /\ 0 <= e x /\ (e x + 1) * s <= CAP (lb (bm x))) /\
  (forall n x s, In (n, x, s) cn -> 0 < s /\ 0 <= e n /\ e n * s <= CAP (lb (bm x))) /\
  (forall x, In x nn -> 0 <= e x).

Definition pin (lb : Lay) (ok : Blk -> Prop) (ev : Ev) : Prop :=
  match ev with
  | ELoad b off w r | EStore b off w r =>
      ok b /\ r = R (lb b) /\ 0 <= off /\ 0 < w /\ off + w <= bytes_of (lb b)
  | _ => False
  end.

Lemma nonneg_spec : forall lb bm kn cn nn e x,
  psound lb bm kn cn nn e -> nonneg kn nn x = true -> 0 <= e x.
Proof.
  intros lb bm kn cn nn e x [Hk [_ Hn]] H. unfold nonneg in H. apply orb_true_iff in H.
  destruct H as [H | H].
  - apply inks_spec in H. exact (Hn x H).
  - apply existsb_exists in H. destruct H as [[y s] [Hin Heq]]. simpl in Heq.
    apply Nat.eqb_eq in Heq. subst y. exact (proj1 (proj2 (Hk x s Hin))).
Qed.

(* A passed test (unsigned, or signed of a not-negative index) makes the
   index known against a count of its block. *)
Lemma ptest_known : forall lb e x n s g cap,
  i64 e -> 0 < s -> 0 <= e n -> e n * s <= cap -> cap < 2 ^ 63 ->
  (g = true -> 0 <= e x) ->
  failsp g (e n) (e x) = false ->
  0 <= e x /\ (e x + 1) * s <= cap.
Proof.
  intros lb e x n s g cap Hi Hs Hn Hns Hcap Hg Hf.
  destruct g; unfold failsp in Hf.
  - apply Z.leb_gt in Hf. specialize (Hg eq_refl). split; [exact Hg | nia].
  - unfold fails in Hf. assert (Hn63 : 0 <= e n < 2 ^ 63) by nia.
    destruct (unsigned_ge_excludes (e n) (e x) Hn63 (Hi x) Hf) as [Hx0 Hxn].
    split; [exact Hx0 | nia].
Qed.

Lemma psound_kill : forall lb bm kn cn nn e y v,
  psound lb bm kn cn nn e -> psound lb bm (killk kn y) (killc cn y) (killn nn y) (upd e y v).
Proof.
  intros lb bm kn cn nn e y v [Hk [Hc Hn]]. split; [| split].
  - intros x s Hin. apply filter_In in Hin. destruct Hin as [Hin Hne]. simpl in Hne.
    unfold upd. destruct (Nat.eqb x y) eqn:E; [discriminate |]. exact (Hk x s Hin).
  - intros n x s Hin. apply filter_In in Hin. destruct Hin as [Hin Hne]. simpl in Hne.
    unfold upd. destruct (Nat.eqb n y) eqn:E; [discriminate |]. exact (Hc n x s Hin).
  - intros x Hin. apply filter_In in Hin. destruct Hin as [Hin Hne].
    unfold upd. destruct (Nat.eqb x y) eqn:E; [discriminate |]. exact (Hn x Hin).
Qed.

Lemma psound_minus : forall lb bm kn cn nn e ks,
  psound lb bm kn cn nn e -> psound lb bm (minusk kn ks) (minusc cn ks) (minusn nn ks) e.
Proof.
  intros lb bm kn cn nn e ks [Hk [Hc Hn]]. split; [| split].
  - intros x s Hin. apply filter_In in Hin. exact (Hk x s (proj1 Hin)).
  - intros n x s Hin. apply filter_In in Hin. exact (Hc n x s (proj1 Hin)).
  - intros x Hin. apply filter_In in Hin. exact (Hn x (proj1 Hin)).
Qed.

Lemma psound_inter : forall lb bm kt ct nt ke ce ne e,
  psound lb bm kt ct nt e -> psound lb bm (interk kt ke) (interc ct ce) (intern nt ne) e.
Proof.
  intros lb bm kt ct nt ke ce ne e [Hk [Hc Hn]]. split; [| split].
  - intros x s Hin. apply filter_In in Hin. exact (Hk x s (proj1 Hin)).
  - intros n x s Hin. apply filter_In in Hin. exact (Hc n x s (proj1 Hin)).
  - intros x Hin. apply filter_In in Hin. exact (Hn x (proj1 Hin)).
Qed.

Lemma psound_inter_r : forall lb bm kt ct nt ke ce ne e,
  psound lb bm ke ce ne e -> psound lb bm (interk kt ke) (interc ct ce) (intern nt ne) e.
Proof.
  intros lb bm kt ct nt ke ce ne e [Hk [Hc Hn]]. split; [| split].
  - intros x s Hin. apply filter_In in Hin. destruct Hin as [_ Hm].
    apply memk_spec in Hm. exact (Hk x s Hm).
  - intros n x s Hin. apply filter_In in Hin. destruct Hin as [_ Hm].
    apply memc_spec in Hm. exact (Hc n x s Hm).
  - intros x Hin. apply filter_In in Hin. destruct Hin as [_ Hm].
    apply inks_spec in Hm. exact (Hn x Hm).
Qed.

Lemma psound_sub : forall lb bm k0 c0 n0 kb cb nb e,
  subk k0 kb = true -> subc c0 cb = true -> subn n0 nb = true ->
  psound lb bm kb cb nb e -> psound lb bm k0 c0 n0 e.
Proof.
  intros lb bm k0 c0 n0 kb cb nb e Hsk Hsc Hsn [Hk [Hc Hn]].
  unfold subk, subc, subn in *. rewrite forallb_forall in Hsk, Hsc, Hsn. split; [| split].
  - intros x s Hin. apply Hk. apply memk_spec. exact (Hsk _ Hin).
  - intros n x s Hin. apply Hc. apply memc_spec. exact (Hsc _ Hin).
  - intros x Hin. apply Hn. apply inks_spec. exact (Hsn _ Hin).
Qed.

Lemma minusn_idem : forall nn ks, minusn (minusn nn ks) ks = minusn nn ks.
Proof.
  intros nn ks. unfold minusn. induction nn as [| a nn IH]; simpl; [reflexivity |].
  destruct (negb (inks ks a)) eqn:E; simpl; [rewrite E, IH; reflexivity | exact IH].
Qed.

Lemma amap_in : forall args p x, amap args p = Some x -> In (p, x) args.
Proof.
  intros args p x H. unfold amap in H.
  destruct (find (fun a => Nat.eqb (fst a) p) args) as [[q y] |] eqn:Hf; [| discriminate].
  injection H as <-. apply find_some in Hf. destruct Hf as [Hin Heq]. simpl in Heq.
  apply Nat.eqb_eq in Heq. subst q. exact Hin.
Qed.

(* The caller's facts of the argument slots give the callee's precondition
   in the callee's frame. *)
Lemma pre_met_sound : forall lb gs gs' bm kn cn nn e args kq cq nq bm' e',
  resp gs bm -> psound lb bm kn cn nn e -> pre_met gs gs' kn cn nn args (kq, cq, nq) = true ->
  (forall p x, In (p, x) args -> bm' p = bm x /\ e' p = e x) ->
  psound lb bm' kq cq nq e'.
Proof.
  intros lb gs gs' bm kn cn nn e args kq cq nq bm' e' Hr Hs Hm Hb.
  unfold pre_met in Hm. rewrite !andb_true_iff in Hm. destruct Hm as [[[_ Hk] Hc] Hn].
  rewrite forallb_forall in Hk, Hc, Hn. split; [| split].
  - intros p s Hin. specialize (Hk (p, s) Hin). simpl in Hk.
    destruct (amap args p) as [x |] eqn:Ha; [| discriminate].
    apply memk_spec in Hk. destruct (Hb p x (amap_in args p x Ha)) as [Hbp Hep].
    rewrite Hbp, Hep. exact (proj1 Hs x s Hk).
  - intros n p s Hin. specialize (Hc (n, p, s) Hin). simpl in Hc.
    destruct (amap args n) as [n' |] eqn:Han; [| discriminate].
    destruct (amap args p) as [x |] eqn:Hap; [| discriminate].
    apply memc_spec in Hc.
    destruct (Hb n n' (amap_in args n n' Han)) as [_ Hen].
    destruct (Hb p x (amap_in args p x Hap)) as [Hbp _].
    rewrite Hbp, Hen, (Hr x). exact (proj1 (proj2 Hs) n' (gof gs x) s Hc).
  - intros p Hin. specialize (Hn p Hin). simpl in Hn.
    destruct (amap args p) as [x |] eqn:Ha; [| discriminate].
    destruct (Hb p x (amap_in args p x Ha)) as [_ Hep]. rewrite Hep.
    exact (nonneg_spec lb bm kn cn nn e x Hs Hn).
Qed.

Lemma prun_preserves : forall P lb ok bm e p t o,
  prun P lb ok bm e p t o -> forall e', o = Done e' -> forall y, ~ In y (pkills p) -> e' y = e y.
Proof.
  intros P lb ok bm e p t o Hrun. induction Hrun; intros e'' Ho y Hy; simpl in Hy; try discriminate.
  - injection Ho as <-. reflexivity.
  - exact (IHHrun e'' Ho y Hy).
  - exact (IHHrun e'' Ho y Hy).
  - exact (IHHrun e'' Ho y Hy).
  - exact (IHHrun e'' Ho y Hy).
  - rewrite (IHHrun e'' Ho y (fun H => Hy (or_intror H))). unfold upd.
    destruct (Nat.eqb y n) eqn:E; [apply Nat.eqb_eq in E; subst; exfalso; apply Hy; left; reflexivity | reflexivity].
  - exact (IHHrun e'' Ho y Hy).
  - exact (IHHrun e'' Ho y Hy).
  - injection Ho as <-. reflexivity.
  - rewrite (IHHrun e'' Ho y (fun H => Hy (or_intror H))). unfold upd.
    destruct (Nat.eqb y x) eqn:E; [apply Nat.eqb_eq in E; subst; exfalso; apply Hy; left; reflexivity | reflexivity].
  - rewrite (IHHrun e'' Ho y (fun H => Hy (or_intror H))). unfold upd.
    destruct (Nat.eqb y x) eqn:E; [apply Nat.eqb_eq in E; subst; exfalso; apply Hy; left; reflexivity | reflexivity].
  - rewrite (IHHrun e'' Ho y (fun H => Hy (or_intror H))). unfold upd.
    destruct (Nat.eqb y x) eqn:E; [apply Nat.eqb_eq in E; subst; exfalso; apply Hy; left; reflexivity | reflexivity].
  - exact (IHHrun e'' Ho y (fun H => Hy (in_or_app _ _ _ (or_intror H)))).
  - rewrite (IHHrun2 e'' Ho y Hy). exact (IHHrun1 e1 eq_refl y (fun H => Hy (in_or_app _ _ _ (or_introl H)))).
  - rewrite (IHHrun2 e'' Ho y (fun H => Hy (in_or_app _ _ _ (or_intror (in_or_app _ _ _ (or_intror H)))))).
    exact (IHHrun1 e1 eq_refl y (fun H => Hy (in_or_app _ _ _ (or_introl H)))).
  - rewrite (IHHrun2 e'' Ho y (fun H => Hy (in_or_app _ _ _ (or_intror (in_or_app _ _ _ (or_intror H)))))).
    exact (IHHrun1 e1 eq_refl y (fun H => Hy (in_or_app _ _ _ (or_intror (in_or_app _ _ _ (or_introl H)))))).
  - exact (IHHrun2 e'' Ho y Hy).
Qed.

Lemma wrap_small : forall v, - 2 ^ 63 <= v < 2 ^ 63 -> wrap v = v.
Proof.
  intros v Hv. unfold wrap. rewrite Z.mod_small by lia. lia.
Qed.

Lemma wrap_i64 : forall v, is_i64 (wrap v).
Proof.
  intro v. unfold is_i64, wrap.
  pose proof (Z.mod_pos_bound (v + 2 ^ 63) (2 ^ 64) ltac:(lia)). lia.
Qed.

Lemma procs_ok_body : forall P f gs pre body,
  procs_ok P = true -> nth_error P f = Some (gs, pre, body) ->
  exists rb, pcheck P gs (fst (fst pre)) (snd (fst pre)) (snd pre) body = Some rb.
Proof.
  intros P f gs [[kq cq] nq] body HP Hf. unfold procs_ok in HP. rewrite forallb_forall in HP.
  specialize (HP _ (nth_error_In P f Hf)). simpl in HP. simpl.
  destruct (pcheck P gs kq cq nq body) as [rb |]; [exists rb; reflexivity | discriminate].
Qed.

Theorem prun_sound : forall P lb ok bm e p t o,
  prun P lb ok bm e p t o ->
  procs_ok P = true -> wf_lay lb -> (forall y, ok (bm y)) -> i64 e ->
  forall gs kn cn nn r, resp gs bm -> psound lb bm kn cn nn e -> pcheck P gs kn cn nn p = Some r ->
  Forall (pin lb ok) t /\
  (forall e', o = Done e' -> i64 e' /\ let '(kn', cn', nn') := r in psound lb bm kn' cn' nn' e').
Proof.
  intros P lb ok bm e p t o Hrun HP Hwf.
  induction Hrun; intros Hok Hi gs kn cn nn r Hr Hs Hc; simpl in Hc.
  - (* PEnd *)
    injection Hc as <-. split; [constructor |]. intros e' He. injection He as <-. tauto.
  - (* PAcc *)
    destruct (memk kn (x, s) && in_slot s o w) eqn:Hm; [| discriminate].
    apply andb_true_iff in Hm. destruct Hm as [Hm Hsl]. apply memk_spec in Hm.
    destruct (proj1 Hs x s Hm) as [_ [Hx0 Hxs]].
    destruct (IHHrun Hok Hi gs kn cn nn r Hr Hs Hc) as [Ht Ho]. split; [| exact Ho].
    constructor; [| exact Ht]. unfold in_slot in Hsl.
    rewrite !andb_true_iff, Z.ltb_lt, Z.leb_le, Z.ltb_lt, Z.leb_le in Hsl.
    destruct Hsl as [[[Hs0 Ho0] Hw0] How]. destruct (Hwf (bm x)) as [Hc0 _].
    unfold pin, poff, bytes_of, PAYLOAD in *. repeat split; try reflexivity; [exact (Hok x) | nia | lia | nia].
  - (* PSto *)
    destruct (memk kn (x, s) && in_slot s o w) eqn:Hm; [| discriminate].
    apply andb_true_iff in Hm. destruct Hm as [Hm Hsl]. apply memk_spec in Hm.
    destruct (proj1 Hs x s Hm) as [_ [Hx0 Hxs]].
    destruct (IHHrun Hok Hi gs kn cn nn r Hr Hs Hc) as [Ht Ho]. split; [| exact Ho].
    constructor; [| exact Ht]. unfold in_slot in Hsl.
    rewrite !andb_true_iff, Z.ltb_lt, Z.leb_le, Z.ltb_lt, Z.leb_le in Hsl.
    destruct Hsl as [[[Hs0 Ho0] Hw0] How]. destruct (Hwf (bm x)) as [Hc0 _].
    unfold pin, poff, bytes_of, PAYLOAD in *. repeat split; try reflexivity; [exact (Hok x) | nia | lia | nia].
  - (* PHdr *)
    destruct (in_hdr o w) eqn:Hh; [| discriminate].
    destruct (IHHrun Hok Hi gs kn cn nn r Hr Hs Hc) as [Ht Ho]. split; [| exact Ho].
    constructor; [| exact Ht]. unfold in_hdr in Hh.
    rewrite !andb_true_iff, Z.leb_le, Z.ltb_lt, Z.leb_le in Hh. destruct Hh as [[Ho0 Hw0] How].
    destruct (Hwf (bm x)) as [Hc0 _].
    unfold pin, bytes_of, PAYLOAD in *. repeat split; try reflexivity; [exact (Hok x) | lia | lia | lia].
  - (* PHdrSto *)
    destruct (in_hdr o w) eqn:Hh; [| discriminate].
    destruct (IHHrun Hok Hi gs kn cn nn r Hr Hs Hc) as [Ht Ho]. split; [| exact Ho].
    constructor; [| exact Ht]. unfold in_hdr in Hh.
    rewrite !andb_true_iff, Z.leb_le, Z.ltb_lt, Z.leb_le in Hh. destruct Hh as [[Ho0 Hw0] How].
    destruct (Hwf (bm x)) as [Hc0 _].
    unfold pin, bytes_of, PAYLOAD in *. repeat split; try reflexivity; [exact (Hok x) | lia | lia | lia].
  - (* PLen *)
    destruct (0 <? s) eqn:Hs0; [| discriminate]. apply Z.ltb_lt in Hs0.
    assert (Hi' : i64 (upd e n v)).
    { intro y. unfold upd. destruct (Nat.eqb y n); [| exact (Hi y)].
      destruct (Hwf (bm x)) as [_ Hc1]. unfold is_i64. nia. }
    apply (IHHrun Hok Hi' gs (killk kn n) ((n, gof gs x, s) :: killc cn n) (n :: killn nn n) r Hr); [| exact Hc].
    destruct (psound_kill lb bm kn cn nn e n v Hs) as [Hk [Hcn Hnn]]. split; [exact Hk | split].
    + intros n1 x1 s1 Hin. destruct Hin as [Heq | Hin]; [| exact (Hcn n1 x1 s1 Hin)].
      injection Heq as <- <- <-. unfold upd. rewrite Nat.eqb_refl, <- (Hr x). tauto.
    + intros y Hin. destruct Hin as [<- | Hin]; [unfold upd; rewrite Nat.eqb_refl; exact H | exact (Hnn y Hin)].
  - (* PCheckPass *)
    destruct (memc cn (n, gof gs x, s) && (negb g || nonneg kn nn x)) eqn:Hm; [| discriminate].
    apply andb_true_iff in Hm. destruct Hm as [Hm Hg]. apply memc_spec in Hm.
    destruct (proj1 (proj2 Hs) n (gof gs x) s Hm) as [Hsp [Hn0 Hns]]. rewrite <- (Hr x) in Hns.
    assert (Hgx : g = true -> 0 <= e x).
    { intro Hg1. subst g. simpl in Hg. exact (nonneg_spec lb bm kn cn nn e x Hs Hg). }
    destruct (Hwf (bm x)) as [_ Hcap].
    destruct (ptest_known lb e x n s g _ Hi Hsp Hn0 Hns Hcap Hgx H) as [Hx0 Hxs].
    apply (IHHrun Hok Hi gs ((x, s) :: kn) cn nn r Hr); [| exact Hc].
    split; [| exact (proj2 Hs)].
    intros x1 s1 Hin. destruct Hin as [Heq | Hin]; [injection Heq as <- <-; tauto | exact (proj1 Hs x1 s1 Hin)].
  - (* PCheckAbort *)
    split; [constructor | discriminate].
  - (* PExitPass *)
    destruct (memc cn (n, gof gs x, s) && (negb g || nonneg kn nn x)) eqn:Hm; [| discriminate].
    destruct (pcheck P gs ((x, s) :: kn) cn nn k) as [[[kb cb] nb] |] eqn:Hk; [| discriminate].
    injection Hc as <-. apply andb_true_iff in Hm. destruct Hm as [Hm Hg]. apply memc_spec in Hm.
    destruct (proj1 (proj2 Hs) n (gof gs x) s Hm) as [Hsp [Hn0 Hns]]. rewrite <- (Hr x) in Hns.
    assert (Hgx : g = true -> 0 <= e x).
    { intro Hg1. subst g. simpl in Hg. exact (nonneg_spec lb bm kn cn nn e x Hs Hg). }
    destruct (Hwf (bm x)) as [_ Hcap].
    destruct (ptest_known lb e x n s g _ Hi Hsp Hn0 Hns Hcap Hgx H) as [Hx0 Hxs].
    assert (Hs' : psound lb bm ((x, s) :: kn) cn nn e).
    { split; [| exact (proj2 Hs)].
      intros x1 s1 Hin. destruct Hin as [Heq | Hin]; [injection Heq as <- <-; tauto | exact (proj1 Hs x1 s1 Hin)]. }
    destruct (IHHrun Hok Hi gs ((x, s) :: kn) cn nn (kb, cb, nb) Hr Hs' Hk) as [Ht Ho]. split; [exact Ht |].
    intros e' He'. destruct (Ho e' He') as [Hi' Hs1]. split; [exact Hi' |].
    exact (psound_inter_r lb bm kn cn nn kb cb nb e' Hs1).
  - (* PExitLeave *)
    destruct (memc cn (n, gof gs x, s) && (negb g || nonneg kn nn x)) eqn:Hm; [| discriminate].
    destruct (pcheck P gs ((x, s) :: kn) cn nn k) as [[[kb cb] nb] |] eqn:Hk; [| discriminate].
    injection Hc as <-. split; [constructor |].
    intros e' He'. injection He' as <-. split; [exact Hi |].
    exact (psound_inter lb bm kn cn nn kb cb nb e Hs).
  - (* PSet *)
    assert (Hi' : i64 (upd e x v)) by (intro y; unfold upd; destruct (Nat.eqb y x); [exact H | exact (Hi y)]).
    exact (IHHrun Hok Hi' gs _ _ _ r Hr (psound_kill lb bm kn cn nn e x v Hs) Hc).
  - (* PConst *)
    assert (Hi' : i64 (upd e x c)) by (intro y; unfold upd; destruct (Nat.eqb y x); [exact H | exact (Hi y)]).
    destruct (0 <=? c) eqn:Hc0.
    + apply Z.leb_le in Hc0.
      apply (IHHrun Hok Hi' gs (killk kn x) (killc cn x) (x :: killn nn x) r Hr); [| exact Hc].
      destruct (psound_kill lb bm kn cn nn e x c Hs) as [Hk [Hcn Hnn]]. split; [exact Hk | split; [exact Hcn |]].
      intros y Hin. destruct Hin as [<- | Hin]; [unfold upd; rewrite Nat.eqb_refl; exact Hc0 | exact (Hnn y Hin)].
    + exact (IHHrun Hok Hi' gs _ _ _ r Hr (psound_kill lb bm kn cn nn e x c Hs) Hc).
  - (* PStep *)
    assert (Hi' : i64 (upd e x (wrap (e y + c)))).
    { intro z. unfold upd. destruct (Nat.eqb z x); [apply wrap_i64 | exact (Hi z)]. }
    destruct (memk kn (y, s) && (0 <=? c) && (c <=? s)) eqn:Hm.
    + rewrite !andb_true_iff, Z.leb_le, Z.leb_le in Hm. destruct Hm as [[Hm Hc0] Hcs].
      apply memk_spec in Hm. destruct (proj1 Hs y s Hm) as [Hsp [Hy0 Hys]].
      destruct (Hwf (bm y)) as [_ Hcap].
      assert (Hw : wrap (e y + c) = e y + c) by (apply wrap_small; nia).
      apply (IHHrun Hok Hi' gs (killk kn x) (killc cn x) (x :: killn nn x) r Hr); [| exact Hc].
      destruct (psound_kill lb bm kn cn nn e x (wrap (e y + c)) Hs) as [Hk [Hcn Hnn]].
      split; [exact Hk | split; [exact Hcn |]].
      intros z Hin. destruct Hin as [<- | Hin]; [unfold upd; rewrite Nat.eqb_refl, Hw; lia | exact (Hnn z Hin)].
    + exact (IHHrun Hok Hi' gs _ _ _ r Hr (psound_kill lb bm kn cn nn e x _ Hs) Hc).
  - (* PLoopExit *)
    destruct (pcheck P gs (minusk kn (pkills body)) (minusc cn (pkills body)) (minusn nn (pkills body)) body)
      as [[[kb cb] nb] |] eqn:Hb; [| discriminate].
    destruct (subk (minusk kn (pkills body)) kb && subc (minusc cn (pkills body)) cb
              && subn (minusn nn (pkills body)) nb) eqn:Hsub; [| discriminate].
    exact (IHHrun Hok Hi gs _ _ _ r Hr (psound_minus lb bm kn cn nn e (pkills body) Hs) Hc).
  - (* PLoopAgain *)
    destruct (pcheck P gs (minusk kn (pkills body)) (minusc cn (pkills body)) (minusn nn (pkills body)) body)
      as [[[kb cb] nb] |] eqn:Hb; [| discriminate].
    destruct (subk (minusk kn (pkills body)) kb && subc (minusc cn (pkills body)) cb
              && subn (minusn nn (pkills body)) nb) eqn:Hsub; [| discriminate].
    rewrite !andb_true_iff in Hsub. destruct Hsub as [[Hsk Hsc] Hsn].
    set (k0 := minusk kn (pkills body)) in *. set (c0 := minusc cn (pkills body)) in *.
    set (n0 := minusn nn (pkills body)) in *.
    assert (Hs0 : psound lb bm k0 c0 n0 e) by exact (psound_minus lb bm kn cn nn e (pkills body) Hs).
    destruct (IHHrun1 Hok Hi gs k0 c0 n0 (kb, cb, nb) Hr Hs0 Hb) as [Ht1 Ho1].
    destruct (Ho1 e1 eq_refl) as [Hi1 Hs1].
    assert (Hs01 : psound lb bm k0 c0 n0 e1) by exact (psound_sub lb bm k0 c0 n0 kb cb nb e1 Hsk Hsc Hsn Hs1).
    assert (Hc2 : pcheck P gs k0 c0 n0 (PLoop body k) = Some r).
    { simpl. unfold k0, c0, n0. rewrite minus_idem_k, minus_idem_c, minusn_idem. fold k0 c0 n0.
      rewrite Hb. unfold k0, c0, n0 in Hsk, Hsc, Hsn |- *. rewrite Hsk, Hsc, Hsn. simpl. exact Hc. }
    destruct (IHHrun2 Hok Hi1 gs k0 c0 n0 r Hr Hs01 Hc2) as [Ht2 Ho2].
    split; [apply Forall_app; split; assumption | exact Ho2].
  - (* PLoopAbort *)
    destruct (pcheck P gs (minusk kn (pkills body)) (minusc cn (pkills body)) (minusn nn (pkills body)) body)
      as [[[kb cb] nb] |] eqn:Hb; [| discriminate].
    destruct (IHHrun Hok Hi gs _ _ _ (kb, cb, nb) Hr (psound_minus lb bm kn cn nn e (pkills body) Hs) Hb) as [Ht _].
    split; [exact Ht | discriminate].
  - (* PBranchThen *)
    destruct (pcheck P gs kn cn nn thn) as [[[kt ct] nt] |] eqn:Ht; [| discriminate].
    destruct (pcheck P gs kn cn nn els) as [[[ke ce] ne] |] eqn:He; [| discriminate].
    destruct (IHHrun1 Hok Hi gs kn cn nn (kt, ct, nt) Hr Hs Ht) as [Ht1 Ho1].
    destruct (Ho1 e1 eq_refl) as [Hi1 Hs1].
    destruct (IHHrun2 Hok Hi1 gs _ _ _ r Hr (psound_inter lb bm kt ct nt ke ce ne e1 Hs1) Hc) as [Ht2 Ho2].
    split; [apply Forall_app; split; assumption | exact Ho2].
  - (* PBranchThenAbort *)
    destruct (pcheck P gs kn cn nn thn) as [[[kt ct] nt] |] eqn:Ht; [| discriminate].
    destruct (IHHrun Hok Hi gs kn cn nn (kt, ct, nt) Hr Hs Ht) as [Ht1 _]. split; [exact Ht1 | discriminate].
  - (* PBranchElse *)
    destruct (pcheck P gs kn cn nn thn) as [[[kt ct] nt] |] eqn:Ht; [| discriminate].
    destruct (pcheck P gs kn cn nn els) as [[[ke ce] ne] |] eqn:He; [| discriminate].
    destruct (IHHrun1 Hok Hi gs kn cn nn (ke, ce, ne) Hr Hs He) as [Ht1 Ho1].
    destruct (Ho1 e1 eq_refl) as [Hi1 Hs1].
    destruct (IHHrun2 Hok Hi1 gs _ _ _ r Hr (psound_inter_r lb bm kt ct nt ke ce ne e1 Hs1) Hc) as [Ht2 Ho2].
    split; [apply Forall_app; split; assumption | exact Ho2].
  - (* PBranchElseAbort *)
    destruct (pcheck P gs kn cn nn thn) as [[[kt ct] nt] |] eqn:Ht; [| discriminate].
    destruct (pcheck P gs kn cn nn els) as [[[ke ce] ne] |] eqn:He; [| discriminate].
    destruct (IHHrun Hok Hi gs kn cn nn (ke, ce, ne) Hr Hs He) as [Ht1 _]. split; [exact Ht1 | discriminate].
  - (* PCall *)
    rewrite H in Hc. destruct pre as [[kq cq] nq].
    destruct (pre_met gs gs' kn cn nn args (kq, cq, nq)) eqn:Hm; [| discriminate].
    destruct (procs_ok_body P f gs' (kq, cq, nq) body HP H) as [rb Hrb]. simpl in Hrb.
    assert (Hs' : psound lb bm' kq cq nq e')
      by exact (pre_met_sound lb gs gs' bm kn cn nn e args kq cq nq bm' e' Hr Hs Hm H3).
    destruct (IHHrun1 H0 H2 gs' kq cq nq rb H1 Hs' Hrb) as [Ht1 _].
    destruct (IHHrun2 Hok Hi gs kn cn nn r Hr Hs Hc) as [Ht2 Ho2].
    split; [apply Forall_app; split; assumption | exact Ho2].
  - (* PCallAbort *)
    rewrite H in Hc. destruct pre as [[kq cq] nq].
    destruct (pre_met gs gs' kn cn nn args (kq, cq, nq)) eqn:Hm; [| discriminate].
    destruct (procs_ok_body P f gs' (kq, cq, nq) body HP H) as [rb Hrb]. simpl in Hrb.
    assert (Hs' : psound lb bm' kq cq nq e')
      by exact (pre_met_sound lb gs gs' bm kn cn nn e args kq cq nq bm' e' Hr Hs Hm H3).
    destruct (IHHrun H0 H2 gs' kq cq nq rb H1 Hs' Hrb) as [Ht1 _]. split; [exact Ht1 | discriminate].
Qed.
Print Assumptions prun_sound.

(* ── The theorem: every run of an accepted program is SPATIAL ─────────── *)

Definition pshaped (lb : Lay) (h : Heap) (b : Blk) : Prop :=
  exists x, h b = Some x /\ size x = bytes_of (lb b) /\ repr x = R (lb b).

Lemma pstep_keeps_shape : forall lb ok h b ev,
  pin lb ok ev -> pshaped lb h b -> pshaped lb (step h ev) b.
Proof.
  intros lb ok h b ev Hev [x [Hx [Hsz Hr]]].
  destruct ev as [b' n k | b' o w k | b' o w k | b' | b']; simpl in Hev; try contradiction; simpl.
  - destruct (h b') as [y |] eqn:Hy; [| exists x; tauto].
    unfold set. cbv beta. destruct (Nat.eqb b b') eqn:E.
    + apply Nat.eqb_eq in E. subst b'. rewrite Hx in Hy. injection Hy as <-.
      exists (mkBlock (size x) (repr x) (rc x) (mark (init x) o w)). simpl. rewrite Nat.eqb_refl. split; [reflexivity | tauto].
    + exists x. rewrite E. tauto.
  - exists x. tauto.
Qed.

Lemma paccesses_spatial : forall lb ok h t,
  (forall b, ok b -> pshaped lb h b) -> Forall (pin lb ok) t ->
  holds spatial_ev h t /\ holds type_ev h t.
Proof.
  intros lb ok h t Hh Hall. revert h Hh.
  induction Hall as [| ev t Hev _ IH]; intros h Hh; simpl; [tauto |].
  assert (Hh' : forall b, ok b -> pshaped lb (step h ev) b)
    by (intros b Hb; exact (pstep_keeps_shape lb ok h b ev Hev (Hh b Hb))).
  destruct (IH (step h ev) Hh') as [Hs Ht].
  destruct ev as [b n k | b o w k | b o w k | b | b]; simpl in Hev; try contradiction;
    destruct Hev as [Hb [Hr [Ho [Hw Hend]]]];
    destruct (Hh b Hb) as [x [Hx [Hsz Hrx]]];
    (split; split; [| exact Hs | | exact Ht]);
    exists x; (split; [exact Hx |]); unfold in_bounds; try lia; congruence.
Qed.

Definition palloc_ev (lb : Lay) (b : Blk) : Ev := EAlloc b (bytes_of (lb b)) (R (lb b)).

Lemma pallocs_shaped : forall lb bs h b,
  (In b bs \/ pshaped lb h b) -> pshaped lb (fold_left step (map (palloc_ev lb) bs) h) b.
Proof.
  intros lb bs. induction bs as [| b0 bs IH]; intros h b Hb; simpl.
  - destruct Hb as [[] | Hb]. exact Hb.
  - apply IH. destruct (in_dec Nat.eq_dec b bs) as [Hin | Hnin]; [left; exact Hin |]. right.
    unfold palloc_ev, set. simpl. cbv beta. destruct (Nat.eqb b b0) eqn:E.
    + apply Nat.eqb_eq in E. subst b0.
      exists (mkBlock (bytes_of (lb b)) (R (lb b)) 1 (fun _ => false)). simpl. rewrite Nat.eqb_refl. split; [reflexivity | tauto].
    + destruct Hb as [[E' | Hin] | [x [Hx Hrest]]];
        [subst b0; rewrite Nat.eqb_refl in E; discriminate | contradiction |].
      exists x. simpl. rewrite E. tauto.
Qed.

Lemma pallocs_holds : forall Q lb bs h,
  (forall h b n k, Q h (EAlloc b n k)) -> holds Q h (map (palloc_ev lb) bs).
Proof.
  intros Q lb bs. induction bs as [| b bs IH]; intros h HQ; simpl; [exact I |].
  split; [apply HQ | apply IH; exact HQ].
Qed.

(* `procs_accepts_spatial`: blocks allocated up front (their layouts the
   header invariant keeps), procedures each accepted under their contract,
   an entry body accepted from no facts — every run's trace of loads and
   stores is SPATIAL and WELL-TYPED, through any depth of recursion. *)
Theorem procs_accepts_spatial : forall P gs lb bs bm e p t o,
  wf_lay lb -> (forall x, In (bm x) bs) -> resp gs bm ->
  paccepts P gs p = true -> i64 e ->
  prun P lb (fun b => In b bs) bm e p t o ->
  Spatial (map (palloc_ev lb) bs ++ t) /\ WellTyped (map (palloc_ev lb) bs ++ t).
Proof.
  intros P gs lb bs bm e p t o Hwf Hbs Hr Hacc Hi Hrun. unfold paccepts in Hacc.
  apply andb_true_iff in Hacc. destruct Hacc as [HP Hc].
  destruct (pcheck P gs [] [] [] p) as [r |] eqn:Hpc; [| discriminate].
  assert (Hs : psound lb bm [] [] [] e) by (split; [| split]; intros; contradiction).
  destruct (prun_sound P lb (fun b => In b bs) bm e p t o Hrun HP Hwf Hbs Hi gs [] [] [] r Hr Hs Hpc) as [Hall _].
  assert (Hh : forall b, In b bs -> pshaped lb (fold_left step (map (palloc_ev lb) bs) empty_heap) b).
  { intros b Hb. apply pallocs_shaped. left. exact Hb. }
  destruct (paccesses_spatial lb (fun b => In b bs) _ t Hh Hall) as [Hsp Hty].
  unfold Spatial, WellTyped. rewrite !holds_app.
  split; split; try assumption; apply pallocs_holds; intros; exact I.
Qed.
Print Assumptions procs_accepts_spatial.

(* ── Controls ─────────────────────────────────────────────────────────── *)

Local Open Scope nat_scope.

(* The recursive copy: slot 1 the source block's count (len / 8), 2 the
   counter indexing that block, 3 its `i + 1` — slot 3 is declared in slot
   2's group (the same block). Procedure 0's precondition: 1 a count of
   parameter 2's block at stride 8, slot 2 not negative. Its body: the
   signed test `i >=s n` (the `then` returns), the read of slot 2 in the
   `else`, the step i + 1 into slot 3, and the recursive call passing 1
   and 3. *)
Definition copy_grp : Grp := [(3, 2)].
Definition copy_pre : Pre := ([], [(1, 2, 8%Z)], [2]).
Definition copy_body : PProg :=
  PBranch PEnd (PCheck 2 1 8 true (PAcc 2 8 0 8 (PStep 3 2 1 8 (PCall 0 [(1, 1); (2, 3)] PEnd)))) PEnd.

(* The entry: count of slot 2's block into slot 1, counter 0, the call. *)
Definition copy_entry : PProg :=
  PLen 1 2 8 (PConst 2 0 (PCall 0 [(1, 1); (2, 2)] PEnd)).

Example accepts_recursive_copy : paccepts [(copy_grp, copy_pre, copy_body)] [] copy_entry = true.
Proof. reflexivity. Qed.

(* The signed test of an index NOT known to be non-negative is refused. *)
Example rejects_signed_unknown_sign :
  paccepts [(copy_grp, ([], [(1, 2, 8%Z)], []), copy_body)] [] copy_entry = false.
Proof. reflexivity. Qed.

(* A step of twice the stride does not keep the index in the no-wrap range
   the stride gives: the next call's precondition is refused. *)
Example rejects_step_past_stride :
  paccepts [(copy_grp, copy_pre,
             PBranch PEnd (PCheck 2 1 8 true (PAcc 2 8 0 8 (PStep 3 2 16 8 (PCall 0 [(1, 1); (2, 3)] PEnd)))) PEnd)]
    [] copy_entry = false.
Proof. reflexivity. Qed.

(* A recursive call passing a count the caller does not have: refused. *)
Example rejects_call_unproven_count :
  paccepts [(copy_grp, copy_pre,
             PBranch PEnd (PCheck 2 1 8 true (PAcc 2 8 0 8 (PStep 3 2 1 8 (PCall 0 [(1, 4); (2, 3)] PEnd)))) PEnd)]
    [] copy_entry = false.
Proof. reflexivity. Qed.

(* The step's slot NOT declared in the counter's group: its block is not
   the counted one, so the recursive call's count is refused. *)
Example rejects_step_outside_group :
  paccepts [([], copy_pre, copy_body)] [] copy_entry = false.
Proof. reflexivity. Qed.

(* An entry calling with a negative start: refused. *)
Example rejects_negative_start :
  paccepts [(copy_grp, copy_pre, copy_body)] [] (PLen 1 2 8 (PConst 2 (-1)%Z (PCall 0 [(1, 1); (2, 2)] PEnd))) = false.
Proof. reflexivity. Qed.

(* Two parameters of one callee group handed slots of two caller groups:
   refused (the callee would read one block through the other's count). *)
Example rejects_groups_disagree :
  paccepts [([(4, 2)], ([], [(1, 2, 8%Z)], [4]), PCheck 4 1 8 true (PAcc 4 8 0 8 PEnd))] []
    (PLen 1 2 8 (PConst 5 0 (PCall 0 [(1, 1); (2, 2); (4, 5)] PEnd))) = false.
Proof. reflexivity. Qed.

(* The same call with the counter in the counted slot's group: accepted. *)
Example accepts_groups_agree :
  paccepts [([(4, 2)], ([], [(1, 2, 8%Z)], [4]), PCheck 4 1 8 true (PAcc 4 8 0 8 PEnd))] [(5, 2)]
    (PLen 1 2 8 (PConst 5 0 (PCall 0 [(1, 1); (2, 2); (4, 5)] PEnd))) = true.
Proof. reflexivity. Qed.

(* A procedure whose body reads without a test, its precondition silent on
   the index: refused even with no caller. *)
Example rejects_body_unchecked :
  paccepts [([], ([], [], []), PAcc 0 8 0 8 PEnd)] [] PEnd = false.
Proof. reflexivity. Qed.

(* A call of a procedure that does not exist: refused. *)
Example rejects_unknown_procedure : paccepts [] [] (PCall 0 [] PEnd) = false.
Proof. reflexivity. Qed.

Local Open Scope Z_scope.

(* The rejections matter: a signed test passes a negative index, and the
   read lands before the block's payload — a block of capacity 8, index -2:
   offset 12 - 16 = -4. *)
Definition one_slot : Lay := fun _ => {| CAP := 8; R := 0%nat |}.

Example signed_test_negative_index_not_spatial :
  let e : Env := fun x => match x with 1%nat => 1 | _ => -2 end in
  let t := [ELoad 0%nat (poff e 2%nat 8 0) 8 0%nat] in
  prun [] one_slot (fun b => b = 0%nat) (fun _ => 0%nat) e
       (PCheck 2 1 8 true (PAcc 2 8 0 8 PEnd)) t (Done e) /\
  ~ Spatial (map (palloc_ev one_slot) [0%nat] ++ t).
Proof.
  simpl. split.
  - apply PRCheckPass; [reflexivity |]. apply PRAcc. apply PREnd.
  - unfold Spatial. simpl. unfold set. simpl.
    intros [_ [[x [Hx Hb]] _]]. injection Hx as <-. unfold in_bounds in Hb.
    cbv [poff PAYLOAD] in Hb. lia.
Qed.
Print Assumptions signed_test_negative_index_not_spatial.
