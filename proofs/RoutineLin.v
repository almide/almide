(* Almide trust spine — #4005 A4, items 4–6 generalised: self-host procedures
   in bounds by checked LINEAR facts.

   The self-host stdlib bodies address memory with absolute pointers: a walk
   `p` against an end pointer `end = h + 12 + len`, an output cursor `o`
   bounded by the allocation it writes (`alloc_str(blen * 3 + 8)`), a header
   word at `h + 4`. This file is their certificate language. A frame's facts
   are linear inequalities `Σ a·v + c <= 0` over

     VS x   slot x's value (a wasm local),
     VB r   the base address of the block region r names,
     VC r   that block's payload capacity in bytes,

   and a fact is DERIVED only by a certificate-supplied Farkas combination: a
   nonnegative multiplier per known fact and a positive scale, checked by
   comparing coefficients (`entails`, sound by `entails_sound`). So the
   checker is linear algebra on integer coefficients; finding the
   multipliers is the untrusted prover's job.

   Facts come from what the code does:

     LHandle h r       h := r's base address (`prim.handle`);
     LLen n r s        n := a count of r's block at stride s (0 <= n, s·n <= cap);
     LAlloc h r sz     h := a fresh block, now region r, of capacity >= sz;
     LWf r             the block lies in the 32-bit address space (wasm32);
     LStatic r b c     region r := the static block at address b of
                       capacity c (a literal the data segment holds);
     LAsn x l          x := l, with a proof that l does not wrap (an
                       invertible step `x := x + d` keeps every fact on x);
     LSetR x lo hi     x := a value the instruction bounds (a byte load);
     LTest a b         the code's own compare `a >= b`: a fact in each arm;
     LCall             the callee's postcondition, its precondition proven;

   and an access `LAcc r w off addr` is accepted when the facts entail
   `0 <= addr`, `base(r) <= addr + off` and `addr + off + w <= base(r) + 12 +
   cap(r)`. A loop carries its invariant: proven on entry and at every
   `LCont` (the back edge), assumed at the head. A procedure's body is
   checked ONCE under its precondition and must end every path in `LRet`
   (proving its postcondition) or a trap; every call proves the callee's
   precondition of its arguments, and assumes its postcondition of the
   result. Soundness is by induction on the run (`lrun_sound`), so recursion
   needs no unfolding.

   HONEST SCOPE. Proven: for every run of an accepted table, every load and
   store event lies inside its block with its block's representation
   (MemorySafe's SPATIAL and TYPE clauses; `lin_accepts_spatial`). The
   semantics is wasm's: an i64 linear expression evaluates to its exact value
   wrapped to 64 bits, an address operand to that value mod 2^32, plus the
   unsigned memarg offset. Trusted, and held per build by almide-anchor-check's
   translation of the shipped bytes: that the events are the instructions the
   bytes contain and a slot is the local the bytes read; that `prim.handle`'s
   value is its block's base, a len word a count of its block, an
   allocation's block at least the size asked; and that every block lies in
   the 32-bit address space. *)

From AlmideTrust Require Import MemorySafe RoutineBounds.
From Stdlib Require Import List ZArith Lia Bool Arith.
Import ListNotations.
Open Scope Z_scope.

(* ── Linear terms ─────────────────────────────────────────────────────── *)

Inductive Var := VS (x : Slot) | VB (r : Slot) | VC (r : Slot).

Definition var_eqb (a b : Var) : bool :=
  match a, b with
  | VS x, VS y | VB x, VB y | VC x, VC y => Nat.eqb x y
  | _, _ => false
  end.

Lemma var_eqb_spec : forall a b, var_eqb a b = true <-> a = b.
Proof.
  intros [x | x | x] [y | y | y]; simpl; split; intro H; try discriminate;
    try (apply Nat.eqb_eq in H; subst; reflexivity);
    try (injection H as <-; apply Nat.eqb_refl).
Qed.

Lemma var_eqb_refl : forall a, var_eqb a a = true.
Proof. intro a. apply var_eqb_spec. reflexivity. Qed.


(* `Σ a·v + c`; as a fact it says `<= 0`. *)
Record Lin := mkL { terms : list (Var * Z); cst : Z }.

Definition Facts := list Lin.

(* A Farkas proof: (fact index, multiplier >= 0) pairs and a scale > 0. *)
Definition Pf := (list (nat * Z) * Z)%type.

Definition Val := Var -> Z.

Definition eval_terms (rho : Val) (ts : list (Var * Z)) : Z :=
  fold_right (fun p acc => snd p * rho (fst p) + acc) 0 ts.

Definition eval (rho : Val) (l : Lin) : Z := eval_terms rho (terms l) + cst l.

Definition lsat (rho : Val) (F : Facts) : Prop := forall l, In l F -> eval rho l <= 0.

Definition coef (ts : list (Var * Z)) (v : Var) : Z :=
  fold_right (fun p acc => (if var_eqb (fst p) v then snd p else 0) + acc) 0 ts.

Definition scale (k : Z) (ts : list (Var * Z)) : list (Var * Z) :=
  map (fun p => (fst p, k * snd p)) ts.

Definition sumv (f : Var -> Z) (vs : list Var) : Z := fold_right (fun v acc => f v + acc) 0 vs.

(* The combination Σ λ_j f_j a proof names; `None` when an index is out of
   range or a multiplier negative. *)
Fixpoint comb (F : Facts) (ps : list (nat * Z)) : option (list (Var * Z) * Z) :=
  match ps with
  | [] => Some ([], 0)
  | (j, k) :: r =>
      match nth_error F j, comb F r with
      | Some f, Some (ts, c) => if 0 <=? k then Some (scale k (terms f) ++ ts, k * cst f + c) else None
      | _, _ => None
      end
  end.

Fixpoint dedup (l : list Var) : list Var :=
  match l with
  | [] => []
  | x :: r => if existsb (var_eqb x) r then dedup r else x :: dedup r
  end.

Lemma dedup_in : forall l x, In x (dedup l) <-> In x l.
Proof.
  intros l x. induction l as [| y r IH]; simpl; [tauto |].
  destruct (existsb (var_eqb y) r) eqn:E; simpl; rewrite IH; [| tauto].
  split; [tauto |]. intros [<- | H]; [| exact H].
  apply existsb_exists in E. destruct E as [z [Hz Heq]]. apply var_eqb_spec in Heq. subst. exact Hz.
Qed.

Lemma dedup_nodup : forall l, NoDup (dedup l).
Proof.
  intro l. induction l as [| y r IH]; simpl; [constructor |].
  destruct (existsb (var_eqb y) r) eqn:E; [exact IH |]. constructor; [| exact IH].
  rewrite dedup_in. intro H.
  assert (existsb (var_eqb y) r = true) by (apply existsb_exists; exists y; split; [exact H | apply var_eqb_refl]).
  congruence.
Qed.

Definition vars (ts : list (Var * Z)) : list Var := dedup (map fst ts).

(* `F ⊢ t`: the proof's combination equals `μ·t` coefficient for coefficient,
   and its constant is at least `μ·cst t`. *)
Definition entails (F : Facts) (pf : Pf) (t : Lin) : bool :=
  let '(ps, mu) := pf in
  (0 <? mu) &&
  match comb F ps with
  | Some (ts, c) =>
      forallb (fun v => Z.eqb (coef ts v) (mu * coef (terms t) v)) (vars (ts ++ terms t)) &&
      (mu * cst t <=? c)
  | None => false
  end.

(* ── Soundness of entailment ──────────────────────────────────────────── *)

Lemma eval_terms_app : forall rho a b, eval_terms rho (a ++ b) = eval_terms rho a + eval_terms rho b.
Proof. intros rho a b. induction a as [| p a IH]; simpl; [reflexivity | rewrite IH; lia]. Qed.

Lemma eval_terms_scale : forall rho k ts, eval_terms rho (scale k ts) = k * eval_terms rho ts.
Proof. intros rho k ts. induction ts as [| p ts IH]; simpl; [lia | rewrite IH; lia]. Qed.

Lemma sumv_plus : forall f g vs, sumv (fun v => f v + g v) vs = sumv f vs + sumv g vs.
Proof. intros f g vs. induction vs as [| v vs IH]; simpl; [reflexivity | rewrite IH; lia]. Qed.

Lemma sumv_ext : forall f g vs, (forall v, In v vs -> f v = g v) -> sumv f vs = sumv g vs.
Proof.
  intros f g vs H. induction vs as [| v vs IH]; simpl; [reflexivity |].
  rewrite (H v (or_introl eq_refl)), IH; [reflexivity | intros u Hu; apply H; right; exact Hu].
Qed.

Lemma sumv_zero : forall vs, sumv (fun _ => 0) vs = 0.
Proof. induction vs as [| v vs IH]; simpl; [reflexivity | rewrite IH; reflexivity]. Qed.

Lemma sumv_zero_ext : forall f vs, (forall v, f v = 0) -> sumv f vs = 0.
Proof. intros f vs H. induction vs as [| v vs IH]; simpl; [reflexivity | rewrite H, IH; reflexivity]. Qed.

Lemma sumv_mul : forall mu f vs, sumv (fun v => mu * f v) vs = mu * sumv f vs.
Proof. intros mu f vs. induction vs as [| v vs IH]; simpl; [lia | rewrite IH; lia]. Qed.

(* The indicator of w sums to its weight over a duplicate-free list holding w. *)
Lemma sumv_point : forall vs w a, NoDup vs -> In w vs ->
  sumv (fun v => (if var_eqb w v then a else 0)) vs = a.
Proof.
  intros vs w a Hnd. induction Hnd as [| v vs Hn Hnd IH]; intro Hin; [destruct Hin |].
  simpl. destruct Hin as [Hvw | Hin].
  - subst w. rewrite var_eqb_refl.
    rewrite (sumv_ext _ (fun _ => 0)); [rewrite sumv_zero; lia |].
    intros u Hu. destruct (var_eqb v u) eqn:E; [| reflexivity].
    apply var_eqb_spec in E. subst. contradiction.
  - destruct (var_eqb w v) eqn:E.
    + apply var_eqb_spec in E. subst. contradiction.
    + rewrite (IH Hin). lia.
Qed.

Lemma eval_coef : forall rho ts vs, NoDup vs -> (forall v, In v (map fst ts) -> In v vs) ->
  eval_terms rho ts = sumv (fun v => coef ts v * rho v) vs.
Proof.
  intros rho ts vs Hnd. induction ts as [| [w a] ts IH]; intro Hc; simpl.
  - symmetry. apply sumv_zero_ext. intros. simpl. lia.
  - rewrite IH by (intros v Hv; apply Hc; right; exact Hv).
    rewrite (sumv_ext (fun v => ((if var_eqb w v then a else 0) + coef ts v) * rho v)
                      (fun v => (if var_eqb w v then a * rho w else 0) + coef ts v * rho v)).
    + rewrite sumv_plus. rewrite (sumv_point vs w); [reflexivity | exact Hnd | apply Hc; left; reflexivity].
    + intros v _. destruct (var_eqb w v) eqn:E; [apply var_eqb_spec in E; subst; lia | lia].
Qed.

Lemma comb_sound : forall rho F ps ts c,
  lsat rho F -> comb F ps = Some (ts, c) -> eval_terms rho ts + c <= 0.
Proof.
  intros rho F ps. induction ps as [| [j k] ps IH]; intros ts c HF Hc; simpl in Hc.
  - injection Hc as <- <-. simpl. lia.
  - destruct (nth_error F j) as [f |] eqn:Hj; [| discriminate].
    destruct (comb F ps) as [[ts' c'] |] eqn:Hr; [| discriminate].
    destruct (0 <=? k) eqn:Hk; [| discriminate]. apply Z.leb_le in Hk.
    injection Hc as <- <-. rewrite eval_terms_app, eval_terms_scale.
    pose proof (HF f (nth_error_In F j Hj)) as Hf. unfold eval in Hf.
    pose proof (IH ts' c' HF eq_refl). nia.
Qed.

Theorem entails_sound : forall rho F pf t, lsat rho F -> entails F pf t = true -> eval rho t <= 0.
Proof.
  intros rho F [ps mu] t HF H. unfold entails in H.
  apply andb_true_iff in H. destruct H as [Hmu H]. apply Z.ltb_lt in Hmu.
  destruct (comb F ps) as [[ts c] |] eqn:Hc; [| discriminate].
  apply andb_true_iff in H. destruct H as [Hco Hcst]. apply Z.leb_le in Hcst.
  rewrite forallb_forall in Hco.
  pose proof (comb_sound rho F ps ts c HF Hc) as Hs.
  set (vs := vars (ts ++ terms t)).
  assert (Hnd : NoDup vs) by apply dedup_nodup.
  assert (Hin : forall v, In v (map fst (ts ++ terms t)) -> In v vs) by (intros v Hv; apply dedup_in; exact Hv).
  rewrite (eval_coef rho ts vs Hnd) in Hs
    by (intros v Hv; apply Hin; rewrite map_app; apply in_or_app; left; exact Hv).
  rewrite (sumv_ext _ (fun v => mu * (coef (terms t) v * rho v))) in Hs.
  2:{ intros v Hv. specialize (Hco v Hv). apply Z.eqb_eq in Hco. rewrite Hco. lia. }
  rewrite sumv_mul in Hs.
  rewrite <- (eval_coef rho (terms t) vs Hnd) in Hs
    by (intros v Hv; apply Hin; rewrite map_app; apply in_or_app; right; exact Hv).
  unfold eval. nia.
Qed.

(* ── Term algebra ─────────────────────────────────────────────────────── *)

Definition v1 (v : Var) : Lin := mkL [(v, 1)] 0.
Definition kz (c : Z) : Lin := mkL [] c.
Definition ladd (a b : Lin) : Lin := mkL (terms a ++ terms b) (cst a + cst b).
Definition lneg (a : Lin) : Lin := mkL (scale (-1) (terms a)) (- cst a).
Definition lsub (a b : Lin) : Lin := ladd a (lneg b).
Definition lmul (k : Z) (a : Lin) : Lin := mkL (scale k (terms a)) (k * cst a).

Lemma eval_v1 : forall rho v, eval rho (v1 v) = rho v.
Proof. intros. unfold eval, eval_terms, v1. cbn [terms cst fold_right fst snd]. lia. Qed.
Lemma eval_kz : forall rho c, eval rho (kz c) = c.
Proof. intros. reflexivity. Qed.
Lemma eval_ladd : forall rho a b, eval rho (ladd a b) = eval rho a + eval rho b.
Proof. intros. unfold eval, ladd. simpl. rewrite eval_terms_app. lia. Qed.
Lemma eval_lneg : forall rho a, eval rho (lneg a) = - eval rho a.
Proof. intros. unfold eval, lneg. simpl. rewrite eval_terms_scale. lia. Qed.
Lemma eval_lsub : forall rho a b, eval rho (lsub a b) = eval rho a - eval rho b.
Proof. intros. unfold lsub. rewrite eval_ladd, eval_lneg. lia. Qed.
Lemma eval_lmul : forall rho k a, eval rho (lmul k a) = k * eval rho a.
Proof. intros. unfold eval, lmul. simpl. rewrite eval_terms_scale. lia. Qed.

Ltac evals := repeat (rewrite ?eval_v1, ?eval_kz, ?eval_ladd, ?eval_lneg, ?eval_lsub, ?eval_lmul in *).

Definition mentions (v : Var) (l : Lin) : bool := existsb (fun p => var_eqb (fst p) v) (terms l).

Definition rupd (rho : Val) (v : Var) (z : Z) : Val := fun u => if var_eqb u v then z else rho u.

Lemma eval_terms_ext : forall rho1 rho2 ts,
  (forall v, In v (map fst ts) -> rho1 v = rho2 v) -> eval_terms rho1 ts = eval_terms rho2 ts.
Proof.
  intros rho1 rho2 ts. induction ts as [| [v a] ts IH]; intro H; simpl; [reflexivity |].
  rewrite (H v (or_introl eq_refl)), IH; [reflexivity | intros u Hu; apply H; right; exact Hu].
Qed.

Lemma eval_ext : forall rho1 rho2 l,
  (forall v, In v (map fst (terms l)) -> rho1 v = rho2 v) -> eval rho1 l = eval rho2 l.
Proof. intros. unfold eval. rewrite (eval_terms_ext rho1 rho2); [reflexivity | assumption]. Qed.

(* `mentions` false: every term's variable differs from v. *)
Lemma mentions_false : forall v l, mentions v l = false -> forall u, In u (map fst (terms l)) -> u <> v.
Proof.
  intros v l H u Hu Huv. subst u. unfold mentions in H.
  apply in_map_iff in Hu. destruct Hu as [p [Hp Hin]].
  assert (existsb (fun p => var_eqb (fst p) v) (terms l) = true)
    by (apply existsb_exists; exists p; split; [exact Hin | rewrite Hp; apply var_eqb_refl]).
  congruence.
Qed.

Lemma eval_rupd_other : forall rho v z l, mentions v l = false -> eval (rupd rho v z) l = eval rho l.
Proof.
  intros rho v z l H. apply eval_ext. intros u Hu. unfold rupd.
  destruct (var_eqb u v) eqn:E; [| reflexivity].
  apply var_eqb_spec in E. exfalso. exact (mentions_false v l H u Hu E).
Qed.

Definition kill (F : Facts) (v : Var) : Facts := filter (fun l => negb (mentions v l)) F.

Lemma kill_sound : forall rho v z F, lsat rho F -> lsat (rupd rho v z) (kill F v).
Proof.
  intros rho v z F HF l Hl. unfold kill in Hl. apply filter_In in Hl. destruct Hl as [Hl Hm].
  apply negb_true_iff in Hm. rewrite eval_rupd_other by exact Hm. apply HF. exact Hl.
Qed.

Lemma kill_sub : forall F v l, In l (kill F v) -> In l F.
Proof. intros F v l H. apply filter_In in H. tauto. Qed.

Lemma lsat_kill : forall rho v F, lsat rho F -> lsat rho (kill F v).
Proof. intros rho v F HF l Hl. apply HF. exact (kill_sub F v l Hl). Qed.

Lemma lsat_app : forall rho A B, lsat rho A -> lsat rho B -> lsat rho (A ++ B).
Proof. intros rho A B HA HB l Hl. apply in_app_or in Hl. destruct Hl; [apply HA | apply HB]; assumption. Qed.

Lemma lsat_cons : forall rho a A, eval rho a <= 0 -> lsat rho A -> lsat rho (a :: A).
Proof. intros rho a A Ha HA l [<- | Hl]; [exact Ha | apply HA; exact Hl]. Qed.

Lemma lsat_ext : forall rho1 rho2 F, (forall v, rho1 v = rho2 v) -> lsat rho1 F -> lsat rho2 F.
Proof. intros rho1 rho2 F H HF l Hl. rewrite <- (eval_ext rho1 rho2 l) by (intros; apply H). apply HF. exact Hl. Qed.

(* Equalities v = l as two facts. *)
Definition eqs (v : Var) (l : Lin) : Facts := [lsub (v1 v) l; lsub l (v1 v)].

Lemma eqs_sound : forall rho v l, rho v = eval rho l -> lsat rho (eqs v l).
Proof. intros rho v l H f [<- | [<- | []]]; evals; lia. Qed.

(* The step `x := x + d` (x's coefficient 1 in l): every old fact f(x) holds
   of the new value as f(x - d). *)
Definition dropv (v : Var) (ts : list (Var * Z)) : list (Var * Z) := filter (fun p => negb (var_eqb (fst p) v)) ts.

Definition subst (v : Var) (d : Lin) (f : Lin) : Lin :=
  mkL (flat_map (fun p => if var_eqb (fst p) v then scale (snd p) (terms d) else [p]) (terms f))
      (cst f + coef (terms f) v * cst d).

Lemma eval_terms_cons : forall rho p ts, eval_terms rho (p :: ts) = snd p * rho (fst p) + eval_terms rho ts.
Proof. reflexivity. Qed.

Lemma coef_cons : forall p ts v, coef (p :: ts) v = (if var_eqb (fst p) v then snd p else 0) + coef ts v.
Proof. reflexivity. Qed.

Lemma eval_subst_terms : forall rho v d ts,
  eval_terms rho (flat_map (fun p => if var_eqb (fst p) v then scale (snd p) (terms d) else [p]) ts)
  + coef ts v * cst d = eval_terms (rupd rho v (eval rho d)) ts.
Proof.
  intros rho v d ts. induction ts as [| [u a] ts IH].
  - reflexivity.
  - cbn [flat_map]. rewrite eval_terms_app, coef_cons, eval_terms_cons. cbn [fst snd].
    unfold rupd at 1. destruct (var_eqb u v) eqn:E; cbv beta iota.
    + rewrite eval_terms_scale. rewrite <- IH. unfold eval. nia.
    + rewrite eval_terms_cons. cbn [fst snd eval_terms fold_right]. rewrite <- IH. lia.
Qed.

Lemma eval_subst : forall rho v d f, eval rho (subst v d f) = eval (rupd rho v (eval rho d)) f.
Proof.
  intros rho v d [ts c]. pose proof (eval_subst_terms rho v d ts) as H.
  unfold subst, eval in *. cbn [terms cst] in *. lia.
Qed.

Lemma eval_terms_drop : forall rho v ts, eval_terms rho ts = coef ts v * rho v + eval_terms rho (dropv v ts).
Proof.
  intros rho v ts. induction ts as [| [u a] ts IH]; simpl; [lia |].
  destruct (var_eqb u v) eqn:E; simpl.
  - apply var_eqb_spec in E. subst. lia.
  - lia.
Qed.

Lemma eval_terms_drop_rupd : forall rho v z ts, eval_terms (rupd rho v z) (dropv v ts) = eval_terms rho (dropv v ts).
Proof.
  intros rho v z ts. induction ts as [| [u a] ts IH]; simpl; [reflexivity |].
  destruct (var_eqb u v) eqn:E; simpl; [exact IH |]. unfold rupd at 1. rewrite E. rewrite IH. reflexivity.
Qed.

(* ── The language ─────────────────────────────────────────────────────── *)

Inductive LProg : Type :=
  | LEnd
  | LAcc (r : Slot) (w off : Z) (addr : Lin) (p0 p1 p2 : Pf) (k : LProg)
  | LSto (r : Slot) (w off : Z) (addr : Lin) (p0 p1 p2 : Pf) (k : LProg)
  | LHandle (h r : Slot) (k : LProg)
  | LLen (n r : Slot) (s : Z) (k : LProg)
  | LAlloc (h r : Slot) (sz : Lin) (lo hi : Pf) (k : LProg)
  | LWf (r : Slot) (k : LProg)
  | LStatic (r : Slot) (b0 c0 : Z) (k : LProg)
  | LSet (x : Slot) (k : LProg)
  | LSetR (x : Slot) (lo hi : Z) (k : LProg)
  | LAsn (x : Slot) (l : Lin) (lo hi : Pf) (k : LProg)
  (* `a >= b` (signed, or unsigned with both sides proven not negative):
     `thn` when it holds, `els` when not. *)
  | LTest (a b : Lin) (sgn : bool) (pa pb nn : Pf * Pf) (thn els : LProg) (k : LProg)
  | LBranch (thn els : LProg) (k : LProg)
  (* A loop: its invariant, a proof of each invariant fact on entry. *)
  | LLoop (inv : Facts) (pin : list Pf) (b : LProg) (k : LProg)
  | LCont (pfs : list Pf)
  | LBrk
  | LTrap
  (* A call of procedure f: each parameter := an argument (with no-wrap
     proofs when a contract names it), each callee region := a caller
     region, each `outs` caller region := a callee region on return, the
     result into `res`; one proof per precondition fact. *)
  | LCall (f : nat) (args : list (Slot * (Lin * option (Pf * Pf)))) (reps : list (Slot * Slot))
          (outs : list (Slot * Slot)) (res : Slot) (pfs : list Pf) (k : LProg)
  (* Return: one proof per postcondition fact. *)
  | LRet (pfs : list Pf).

(* A procedure: precondition and postcondition facts over its own slots and
   regions, the slot its result is in, its body. *)
Record Proc := mkProc { pre : Facts; post : Facts; rslot : Slot; body : LProg }.
Definition Procs := list (nat * Proc).

Definition plook (P : Procs) (f : nat) : option Proc :=
  match find (fun q => Nat.eqb (fst q) f) P with Some (_, d) => Some d | None => None end.

(* What a body may write: slots and region bindings. *)
Fixpoint writes (p : LProg) : list Slot :=
  match p with
  | LEnd | LCont _ | LBrk | LTrap | LRet _ => []
  | LAcc _ _ _ _ _ _ _ k | LSto _ _ _ _ _ _ _ k | LWf _ k | LStatic _ _ _ k => writes k
  | LHandle h _ k | LLen h _ _ k | LAlloc h _ _ _ _ k | LSet h k | LSetR h _ _ k | LAsn h _ _ _ k => h :: writes k
  | LTest _ _ _ _ _ _ thn els k | LBranch thn els k => writes thn ++ writes els ++ writes k
  | LLoop _ _ b k => writes b ++ writes k
  | LCall _ _ _ _ res _ k => res :: writes k
  end.

Fixpoint rebinds (p : LProg) : list Slot :=
  match p with
  | LEnd | LCont _ | LBrk | LTrap | LRet _ => []
  | LAcc _ _ _ _ _ _ _ k | LSto _ _ _ _ _ _ _ k | LWf _ k | LHandle _ _ k | LLen _ _ _ k
  | LSet _ k | LSetR _ _ _ k | LAsn _ _ _ _ k => rebinds k
  | LAlloc _ r _ _ _ k | LStatic r _ _ k => r :: rebinds k
  | LTest _ _ _ _ _ _ thn els k | LBranch thn els k => rebinds thn ++ rebinds els ++ rebinds k
  | LLoop _ _ b k => rebinds b ++ rebinds k
  | LCall _ _ _ outs _ _ k => map fst outs ++ rebinds k
  end.

(* ── Runs ─────────────────────────────────────────────────────────────── *)

Definition Lay := Blk -> Layout.
Definition Bm := Slot -> Blk.
Definition Base := Blk -> Z.

Definition wrap (v : Z) : Z := (v + 2 ^ 63) mod 2 ^ 64 - 2 ^ 63.

Definition cmp_ge (sgn : bool) (x y : Z) : bool :=
  if sgn then y <=? x else unsigned y <=? unsigned x.

Definition bupd (bm : Bm) (r : Slot) (b : Blk) : Bm := fun y => if Nat.eqb y r then b else bm y.

(* The caller's regions after a call: an `outs` region takes its callee
   region's final block. *)
Definition bouts (bm : Bm) (bm1 : Bm) (outs : list (Slot * Slot)) : Bm :=
  fun y => match find (fun q => Nat.eqb (fst q) y) outs with Some (_, r') => bm1 r' | None => bm y end.

Inductive Out := ODone (bm : Bm) (e : Env) | OBrk (bm : Bm) (e : Env) | OCont (bm : Bm) (e : Env)
               | ORet (bm : Bm) (e : Env) | OAbort.

Definition exits (o : Out) (bm : Bm) (e : Env) : Prop := o = ODone bm e \/ o = OBrk bm e.
Definition stops (o : Out) : Prop := match o with ODone _ _ => False | _ => True end.

Section Run.
Variables (P : Procs) (lb : Lay) (base : Base) (ok : Blk -> Prop).

Definition val (bm : Bm) (e : Env) : Val :=
  fun v => match v with VS x => e x | VB r => base (bm r) | VC r => CAP (lb (bm r)) end.

Definition ev_off (bm : Bm) (e : Env) (r : Slot) (off : Z) (addr : Lin) : Z :=
  eval (val bm e) addr mod 2 ^ 32 + off - base (bm r).

Inductive lrun : Bm -> Env -> LProg -> list Ev -> Out -> Prop :=
  | LREnd : forall bm e, lrun bm e LEnd [] (ODone bm e)
  | LRAcc : forall bm e r w off addr p0 p1 p2 k t o,
      lrun bm e k t o ->
      lrun bm e (LAcc r w off addr p0 p1 p2 k) (ELoad (bm r) (ev_off bm e r off addr) w (R (lb (bm r))) :: t) o
  | LRSto : forall bm e r w off addr p0 p1 p2 k t o,
      lrun bm e k t o ->
      lrun bm e (LSto r w off addr p0 p1 p2 k) (EStore (bm r) (ev_off bm e r off addr) w (R (lb (bm r))) :: t) o
  | LRHandle : forall bm e h r k t o,
      lrun bm (upd e h (base (bm r))) k t o -> lrun bm e (LHandle h r k) t o
  | LRLen : forall bm e n r s v k t o,
      is_i64 v -> 0 <= v -> s * v <= CAP (lb (bm r)) ->
      lrun bm (upd e n v) k t o -> lrun bm e (LLen n r s k) t o
  | LRAlloc : forall bm e h r sz lo hi b k t o,
      ok b -> wrap (eval (val bm e) sz) <= CAP (lb b) ->
      lrun (bupd bm r b) (upd e h (base b)) k t o -> lrun bm e (LAlloc h r sz lo hi k) t o
  | LRWf : forall bm e r k t o, lrun bm e k t o -> lrun bm e (LWf r k) t o
  | LRStatic : forall bm e r b0 c0 b k t o,
      ok b -> base b = b0 -> CAP (lb b) = c0 ->
      lrun (bupd bm r b) e k t o -> lrun bm e (LStatic r b0 c0 k) t o
  | LRSet : forall bm e x v k t o,
      is_i64 v -> lrun bm (upd e x v) k t o -> lrun bm e (LSet x k) t o
  | LRSetR : forall bm e x lo hi v k t o,
      is_i64 v -> lo <= v <= hi -> lrun bm (upd e x v) k t o -> lrun bm e (LSetR x lo hi k) t o
  | LRAsn : forall bm e x l lo hi k t o,
      lrun bm (upd e x (wrap (eval (val bm e) l))) k t o -> lrun bm e (LAsn x l lo hi k) t o
  | LRTest : forall bm e a b sgn pa pb nn thn els k t1 bm1 e1 t2 o,
      lrun bm e (if cmp_ge sgn (wrap (eval (val bm e) a)) (wrap (eval (val bm e) b)) then thn else els) t1 (ODone bm1 e1) ->
      lrun bm1 e1 k t2 o -> lrun bm e (LTest a b sgn pa pb nn thn els k) (t1 ++ t2) o
  | LRTestStop : forall bm e a b sgn pa pb nn thn els k t1 o,
      lrun bm e (if cmp_ge sgn (wrap (eval (val bm e) a)) (wrap (eval (val bm e) b)) then thn else els) t1 o ->
      stops o -> lrun bm e (LTest a b sgn pa pb nn thn els k) t1 o
  | LRBranch : forall bm e thn els k t1 bm1 e1 t2 o (side : bool),
      lrun bm e (if side then thn else els) t1 (ODone bm1 e1) -> lrun bm1 e1 k t2 o ->
      lrun bm e (LBranch thn els k) (t1 ++ t2) o
  | LRBranchStop : forall bm e thn els k t1 o (side : bool),
      lrun bm e (if side then thn else els) t1 o -> stops o -> lrun bm e (LBranch thn els k) t1 o
  | LRLoopExit : forall bm e inv pin b k t1 o1 bm1 e1 t2 o,
      lrun bm e b t1 o1 -> exits o1 bm1 e1 -> lrun bm1 e1 k t2 o ->
      lrun bm e (LLoop inv pin b k) (t1 ++ t2) o
  | LRLoopAgain : forall bm e inv pin b k t1 bm1 e1 t2 o,
      lrun bm e b t1 (OCont bm1 e1) -> lrun bm1 e1 (LLoop inv pin b k) t2 o ->
      lrun bm e (LLoop inv pin b k) (t1 ++ t2) o
  | LRLoopStop : forall bm e inv pin b k t1 o,
      lrun bm e b t1 o -> (o = OAbort \/ exists bm1 e1, o = ORet bm1 e1) ->
      lrun bm e (LLoop inv pin b k) t1 o
  | LRCont : forall bm e pfs, lrun bm e (LCont pfs) [] (OCont bm e)
  | LRBrk : forall bm e, lrun bm e LBrk [] (OBrk bm e)
  | LRTrap : forall bm e, lrun bm e LTrap [] OAbort
  (* A call runs the callee's body in a frame of its own: each parameter
     bound to its argument's (wrapped) value, each mapped region to the
     caller's block, every other slot and region to any value and live
     block. The caller continues with the result in `res`. *)
  | LRCall : forall bm e f args reps outs res pfs k d bm' e' t1 bm1 e1 t2 o,
      plook P f = Some d ->
      (forall y, ok (bm' y)) -> i64 e' ->
      (forall p a, In (p, a) args -> e' p = wrap (eval (val bm e) (fst a))) ->
      (forall r' r, In (r', r) reps -> bm' r' = bm r) ->
      lrun bm' e' (body d) t1 (ORet bm1 e1) ->
      lrun (bouts bm bm1 outs) (upd e res (e1 (rslot d))) k t2 o ->
      lrun bm e (LCall f args reps outs res pfs k) (t1 ++ t2) o
  | LRCallAbort : forall bm e f args reps outs res pfs k d bm' e' t1,
      plook P f = Some d ->
      (forall y, ok (bm' y)) -> i64 e' ->
      (forall p a, In (p, a) args -> e' p = wrap (eval (val bm e) (fst a))) ->
      (forall r' r, In (r', r) reps -> bm' r' = bm r) ->
      lrun bm' e' (body d) t1 OAbort ->
      lrun bm e (LCall f args reps outs res pfs k) t1 OAbort
  | LRRet : forall bm e pfs, lrun bm e (LRet pfs) [] (ORet bm e).
End Run.

(* ── The checker ──────────────────────────────────────────────────────── *)

Definition I64MIN : Z := - 2 ^ 63.
Definition I64MAX : Z := 2 ^ 63 - 1.
Definition ADDR : Z := 2 ^ 32.

(* l cannot wrap: a single slot (an i64 by construction), or both bounds
   proven. *)
Definition no_wrap (F : Facts) (l : Lin) (pf : Pf * Pf) : bool :=
  match terms l, cst l with
  | [(VS _, 1)], 0 => true
  | _, _ => entails F (fst pf) (lsub (kz I64MIN) l) && entails F (snd pf) (lsub l (kz I64MAX))
  end.

(* The loop invariant (inside a loop); the postcondition. *)
Definition Ctx := (option Facts * Facts)%type.

Definition St := option Facts.

Definition lin_eqb (a b : Lin) : bool :=
  Z.eqb (cst a) (cst b) &&
  (Nat.eqb (length (terms a)) (length (terms b)) &&
   forallb (fun pq => var_eqb (fst (fst pq)) (fst (snd pq)) && Z.eqb (snd (fst pq)) (snd (snd pq)))
           (combine (terms a) (terms b))).

Lemma lin_eqb_eq : forall a b, lin_eqb a b = true -> a = b.
Proof.
  intros [ta ca] [tb cb] H. unfold lin_eqb in H. simpl in H.
  rewrite !andb_true_iff in H. destruct H as [Hc [Hl Hf]].
  apply Z.eqb_eq in Hc. apply Nat.eqb_eq in Hl. subst cb. f_equal.
  revert tb Hl Hf. induction ta as [| [v a] ta IH]; intros [| [u b] tb] Hl Hf; simpl in *; try discriminate; [reflexivity |].
  rewrite andb_true_iff in Hf. destruct Hf as [Hh Ht]. rewrite andb_true_iff in Hh. destruct Hh as [Hv Hz].
  apply var_eqb_spec in Hv. apply Z.eqb_eq in Hz. subst. f_equal. apply IH; [lia | exact Ht].
Qed.

Definition fjoin (a b : St) : St :=
  match a, b with
  | None, x => x
  | x, None => x
  | Some fa, Some fb => Some (filter (fun l => existsb (lin_eqb l) fb) fa)
  end.

Definition memf (l : Lin) (F : Facts) : bool := existsb (lin_eqb l) F.

(* Every fact of G entailed by F, one proof each (or present verbatim). *)
Fixpoint proves (F : Facts) (G : Facts) (pfs : list Pf) : bool :=
  match G, pfs with
  | [], [] => true
  | g :: G, pf :: pfs => (memf g F || entails F pf g) && proves F G pfs
  | _, _ => false
  end.

Definition acc_ok (F : Facts) (r : Slot) (w off : Z) (addr : Lin) (p0 p1 p2 : Pf) : bool :=
  (0 <? w) && (0 <=? off) &&
  entails F p0 (lneg addr) &&
  entails F p1 (lsub (v1 (VB r)) (ladd addr (kz off))) &&
  entails F p2 (lsub (ladd addr (kz (off + w - PAYLOAD))) (ladd (v1 (VB r)) (v1 (VC r)))).

Definition test_facts (F : Facts) (a b : Lin) (sgn : bool) (pa pb nn : Pf * Pf) : Facts * Facts :=
  if no_wrap F a pa && no_wrap F b pb &&
     (sgn || (entails F (fst nn) (lneg a) && entails F (snd nn) (lneg b)))
  then ([lsub b a], [ladd (lsub a b) (kz 1)])
  else ([], []).

Definition kill_region (F : Facts) (r : Slot) : Facts := kill (kill F (VB r)) (VC r).

Definition asn_facts (F : Facts) (x : Slot) (l : Lin) (lo hi : Pf) : Facts :=
  if no_wrap F l (lo, hi) then
    let c := coef (terms l) (VS x) in
    if Z.eqb c 0 then eqs (VS x) l ++ kill F (VS x)
    else if Z.eqb c 1 then map (subst (VS x) (lsub (v1 (VS x)) (mkL (dropv (VS x) (terms l)) (cst l)))) F
    else kill F (VS x)
  else kill F (VS x).

(* ── Calls: the translation of a contract across the call ─────────────── *)

Fixpoint assoc {A : Type} (k : Slot) (l : list (Slot * A)) : option A :=
  match l with [] => None | (k', a) :: l => if Nat.eqb k' k then Some a else assoc k l end.

(* The arguments whose no-wrap proofs hold: the parameter equals the
   argument's exact value. *)
Definition valid_args (F : Facts) (args : list (Slot * (Lin * option (Pf * Pf)))) : list (Slot * Lin) :=
  flat_map (fun pa => match snd (snd pa) with
                      | Some pf => if no_wrap F (fst (snd pa)) pf then [(fst pa, fst (snd pa))] else []
                      | None => []
                      end) args.

Fixpoint tr_terms (tr : Var -> option Lin) (ts : list (Var * Z)) : option Lin :=
  match ts with
  | [] => Some (kz 0)
  | (v, a) :: ts =>
      match tr v, tr_terms tr ts with
      | Some d, Some rest => Some (ladd (lmul a d) rest)
      | _, _ => None
      end
  end.

Definition tr_lin (tr : Var -> option Lin) (l : Lin) : option Lin :=
  match tr_terms tr (terms l) with Some m => Some (ladd m (kz (cst l))) | None => None end.

Fixpoint tr_all (tr : Var -> option Lin) (G : Facts) : option Facts :=
  match G with
  | [] => Some []
  | g :: G => match tr_lin tr g, tr_all tr G with Some g', Some G' => Some (g' :: G') | _, _ => None end
  end.

Definition tr_some (tr : Var -> option Lin) (G : Facts) : Facts :=
  flat_map (fun g => match tr_lin tr g with Some g' => [g'] | None => [] end) G.

Definition tr_in (A : list (Slot * Lin)) (reps : list (Slot * Slot)) (v : Var) : option Lin :=
  match v with
  | VS p => assoc p A
  | VB r' => match assoc r' reps with Some r => Some (v1 (VB r)) | None => None end
  | VC r' => match assoc r' reps with Some r => Some (v1 (VC r)) | None => None end
  end.

(* The caller region an `outs` entry binds to callee region r'. *)
Definition out_of (outs : list (Slot * Slot)) (r' : Slot) : option Slot :=
  match find (fun q => match assoc (fst q) outs with Some x => Nat.eqb x r' | None => false end) outs with
  | Some (r, _) => Some r
  | None => None
  end.

Definition tr_out (A : list (Slot * Lin)) (reps outs : list (Slot * Slot)) (d : Proc) (res : Slot) (v : Var) : option Lin :=
  match v with
  | VS p => if Nat.eqb p (rslot d) then Some (v1 (VS res))
            else if existsb (Nat.eqb p) (writes (body d)) then None else assoc p A
  | VB r' => match out_of outs r' with
             | Some r => Some (v1 (VB r))
             | None => if existsb (Nat.eqb r') (rebinds (body d)) then None
                       else match assoc r' reps with Some r => Some (v1 (VB r)) | None => None end
             end
  | VC r' => match out_of outs r' with
             | Some r => Some (v1 (VC r))
             | None => if existsb (Nat.eqb r') (rebinds (body d)) then None
                       else match assoc r' reps with Some r => Some (v1 (VC r)) | None => None end
             end
  end.

(* The call's frame conditions: no argument mentions the result slot or an
   `outs` region, and no mapped region is also an `outs` region. *)
Definition call_ok (A : list (Slot * Lin)) (reps outs : list (Slot * Slot)) (res : Slot) : bool :=
  forallb (fun pl => negb (mentions (VS res) (snd pl)) &&
                     forallb (fun q => negb (mentions (VB (fst q)) (snd pl)) && negb (mentions (VC (fst q)) (snd pl))) outs) A &&
  forallb (fun q => negb (existsb (fun q' => Nat.eqb (fst q') (snd q)) outs)) reps.

Definition kill_outs (F : Facts) (outs : list (Slot * Slot)) : Facts :=
  fold_left (fun G q => kill_region G (fst q)) outs F.

Definition after_arms (lc : Facts -> option (St * St)) (rt re : option (St * St)) : option (St * St) :=
  match rt, re with
  | Some (st, bt), Some (se, be) =>
      match fjoin st se with
      | None => Some (None, fjoin bt be)
      | Some G => match lc G with
                  | Some (sk, bk) => Some (sk, fjoin (fjoin bt be) bk)
                  | None => None
                  end
      end
  | _, _ => None
  end.

Fixpoint lcheck (P : Procs) (C : Ctx) (F : Facts) (p : LProg) {struct p} : option (St * St) :=
  match p with
  | LEnd => Some (Some F, None)
  | LAcc r w off addr p0 p1 p2 k | LSto r w off addr p0 p1 p2 k =>
      if acc_ok F r w off addr p0 p1 p2 then lcheck P C F k else None
  | LHandle h r k => lcheck P C (eqs (VS h) (v1 (VB r)) ++ kill F (VS h)) k
  | LLen n r s k => lcheck P C (lneg (v1 (VS n)) :: lsub (lmul s (v1 (VS n))) (v1 (VC r)) :: kill F (VS n)) k
  | LAlloc h r sz lo hi k =>
      if no_wrap F sz (lo, hi) && negb (mentions (VS h) sz) && negb (mentions (VB r) sz) && negb (mentions (VC r) sz)
      then lcheck P C (eqs (VS h) (v1 (VB r)) ++ lsub sz (v1 (VC r)) :: kill_region (kill F (VS h)) r) k
      else None
  | LWf r k =>
      lcheck P C (lneg (v1 (VB r)) :: lneg (v1 (VC r)) ::
                  ladd (ladd (v1 (VB r)) (v1 (VC r))) (kz (PAYLOAD - ADDR + 1)) :: F) k
  | LStatic r b0 c0 k => lcheck P C (eqs (VB r) (kz b0) ++ eqs (VC r) (kz c0) ++ kill_region F r) k
  | LSet x k => lcheck P C (kill F (VS x)) k
  | LSetR x lo hi k => lcheck P C (lsub (kz lo) (v1 (VS x)) :: lsub (v1 (VS x)) (kz hi) :: kill F (VS x)) k
  | LAsn x l lo hi k => lcheck P C (asn_facts F x l lo hi) k
  | LTest a b sgn pa pb nn thn els k =>
      let '(ft, fe) := test_facts F a b sgn pa pb nn in
      after_arms (fun G => lcheck P C G k) (lcheck P C (ft ++ F) thn) (lcheck P C (fe ++ F) els)
  | LBranch thn els k =>
      after_arms (fun G => lcheck P C G k) (lcheck P C F thn) (lcheck P C F els)
  | LLoop inv pin b k =>
      if proves F inv pin then
        match lcheck P (Some inv, snd C) inv b with
        | Some (sb, bb) =>
            match fjoin sb bb with
            | None => Some (None, None)
            | Some G => lcheck P C G k
            end
        | None => None
        end
      else None
  | LCont pfs => match fst C with Some Iv => if proves F Iv pfs then Some (None, None) else None | None => None end
  | LBrk => Some (None, Some F)
  | LTrap => Some (None, None)
  | LCall f args reps outs res pfs k =>
      match plook P f with
      | Some d =>
          let A := valid_args F args in
          match tr_all (tr_in A reps) (pre d) with
          | Some pre' =>
              if proves F pre' pfs && call_ok A reps outs res then
                lcheck P C (tr_some (tr_out A reps outs d res) (post d) ++ kill (kill_outs F outs) (VS res)) k
              else None
          | None => None
          end
      | None => None
      end
  | LRet pfs => if proves F (snd C) pfs then Some (None, None) else None
  end.

Definition procs_ok (P : Procs) : bool :=
  forallb (fun q => match lcheck P (None, post (snd q)) (pre (snd q)) (body (snd q)) with
                    | Some (None, None) => true
                    | _ => false
                    end) P.

Definition laccepts (P : Procs) (p : LProg) : bool :=
  procs_ok P && match lcheck P (None, []) [] p with Some _ => true | None => false end.

(* ── Soundness: the pieces ───────────────────────────────────────────── *)

Lemma wrap_small : forall v, I64MIN <= v <= I64MAX -> wrap v = v.
Proof. intros v Hv. unfold wrap, I64MIN, I64MAX in *. rewrite Z.mod_small by lia. lia. Qed.

Lemma wrap_i64 : forall v, is_i64 (wrap v).
Proof.
  intro v. unfold is_i64, wrap.
  pose proof (Z.mod_pos_bound (v + 2 ^ 63) (2 ^ 64) ltac:(lia)). lia.
Qed.

Definition vi64 (rho : Val) : Prop := forall x, is_i64 (rho (VS x)).

Lemma no_wrap_sound : forall rho F l pf, vi64 rho -> lsat rho F -> no_wrap F l pf = true ->
  I64MIN <= eval rho l <= I64MAX.
Proof.
  intros rho F [ts c] pf Hi HF H. unfold no_wrap in H. cbn [terms cst] in H.
  assert (Hgen : entails F (fst pf) (lsub (kz I64MIN) (mkL ts c)) && entails F (snd pf) (lsub (mkL ts c) (kz I64MAX)) = true ->
                 I64MIN <= eval rho (mkL ts c) <= I64MAX).
  { intro Hb. apply andb_true_iff in Hb. destruct Hb as [H1 H2].
    pose proof (entails_sound rho F _ _ HF H1). pose proof (entails_sound rho F _ _ HF H2). evals. lia. }
  destruct ts as [| [[x | x | x] a] [| q ts]]; try (apply Hgen; exact H);
    destruct a as [| [ | | ] | ]; try (apply Hgen; exact H);
    destruct c; try (apply Hgen; exact H).
  unfold eval, eval_terms. cbn [terms cst fold_right fst snd]. specialize (Hi x). unfold is_i64, I64MIN, I64MAX in *. lia.
Qed.

Lemma memf_in : forall l F, memf l F = true -> In l F.
Proof.
  intros l F H. unfold memf in H. apply existsb_exists in H. destruct H as [l' [Hin Heq]].
  apply lin_eqb_eq in Heq. subst. exact Hin.
Qed.

Lemma proves_sound : forall rho F G pfs, lsat rho F -> proves F G pfs = true -> lsat rho G.
Proof.
  intros rho F G. induction G as [| g G IH]; intros [| pf pfs] HF H; simpl in H; try discriminate.
  - intros l [].
  - apply andb_true_iff in H. destruct H as [Hg H]. apply lsat_cons; [| exact (IH pfs HF H)].
    apply orb_true_iff in Hg. destruct Hg as [Hm | He].
    + apply HF. apply memf_in. exact Hm.
    + exact (entails_sound rho F pf g HF He).
Qed.

Lemma proves_length : forall F G pfs, proves F G pfs = true -> length G = length pfs.
Proof.
  intros F G. induction G as [| g G IH]; intros [| pf pfs] H; simpl in H; try discriminate; [reflexivity |].
  apply andb_true_iff in H. simpl. f_equal. apply IH. tauto.
Qed.

Lemma combine_diag : forall (ts : list (Var * Z)) x, In x (combine ts ts) -> fst x = snd x.
Proof.
  intros ts x. induction ts as [| p ts IH]; simpl; [tauto |].
  intros [<- | Hin]; [reflexivity | exact (IH Hin)].
Qed.

Lemma memf_refl : forall l F, In l F -> memf l F = true.
Proof.
  intros l F H. unfold memf. apply existsb_exists. exists l. split; [exact H |].
  destruct l as [ts c]. unfold lin_eqb. cbn [terms cst]. rewrite Z.eqb_refl, Nat.eqb_refl. simpl.
  apply forallb_forall. intros x Hin. pose proof (combine_diag ts x Hin) as Hd.
  destruct x as [[v a] [u b]]. cbn [fst snd] in *. injection Hd as -> ->.
  rewrite var_eqb_refl, Z.eqb_refl. reflexivity.
Qed.

Lemma proves_self : forall Full G pfs, (forall g, In g G -> In g Full) -> length G = length pfs -> proves Full G pfs = true.
Proof.
  intros Full G. induction G as [| g G IH]; intros [| pf pfs] Hs Hl; simpl in Hl; try discriminate; [reflexivity |].
  simpl. rewrite memf_refl by (apply Hs; left; reflexivity). simpl.
  apply IH; [intros h Hh; apply Hs; right; exact Hh | lia].
Qed.

Section Sound.
Variables (P : Procs) (lb : Lay) (base : Base) (ok : Blk -> Prop).
Hypothesis Hwf : forall b, 0 <= base b /\ 0 <= CAP (lb b) /\ base b + PAYLOAD + CAP (lb b) < ADDR.
Hypothesis HP : procs_ok P = true.

Definition good (bm : Bm) (e : Env) (F : Facts) : Prop :=
  lsat (val lb base bm e) F /\ i64 e /\ (forall y, ok (bm y)).

Definition gst (bm : Bm) (e : Env) (s : St) : Prop :=
  match s with Some G => good bm e G | None => False end.

Definition post_ok (C : Ctx) (r : St * St) (o : Out) : Prop :=
  match o with
  | ODone bm e => gst bm e (fst r)
  | OBrk bm e => gst bm e (snd r)
  | OCont bm e => gst bm e (fst C)
  | ORet bm e => good bm e (snd C)
  | OAbort => True
  end.

Definition lpin (ev : Ev) : Prop :=
  match ev with
  | ELoad b off w r | EStore b off w r => ok b /\ r = R (lb b) /\ 0 <= off /\ 0 < w /\ off + w <= bytes_of (lb b)
  | _ => False
  end.

Lemma val_vi64 : forall bm e, i64 e -> vi64 (val lb base bm e).
Proof. intros bm e H x. exact (H x). Qed.

Lemma val_upd : forall bm e x z v, val lb base bm (upd e x z) v = rupd (val lb base bm e) (VS x) z v.
Proof.
  intros bm e x z [y | y | y]; unfold val, rupd, upd; simpl; [| reflexivity | reflexivity].
  destruct (Nat.eqb y x); reflexivity.
Qed.

Lemma i64_upd : forall e x z, i64 e -> is_i64 z -> i64 (upd e x z).
Proof. intros e x z He Hz y. unfold upd. destruct (Nat.eqb y x); [exact Hz | apply He]. Qed.

Lemma good_kill_upd : forall bm e F x z, good bm e F -> is_i64 z ->
  lsat (val lb base bm (upd e x z)) (kill F (VS x)) /\ i64 (upd e x z) /\ (forall y, ok (bm y)).
Proof.
  intros bm e F x z [HF [Hi Ho]] Hz. split; [| split; [apply i64_upd; assumption | exact Ho]].
  apply (lsat_ext (rupd (val lb base bm e) (VS x) z)); [intro v; symmetry; apply val_upd |].
  apply kill_sound. exact HF.
Qed.

Lemma base_i64 : forall b, is_i64 (base b).
Proof. intro b. destruct (Hwf b) as [H0 [H1 H2]]. unfold is_i64, ADDR, PAYLOAD in *. lia. Qed.

Lemma gst_join_l : forall bm e a b, gst bm e a -> gst bm e (fjoin a b).
Proof.
  intros bm e [fa |] [fb |] H; simpl in *; try contradiction; try exact H.
  destruct H as [HF Hr]. split; [| exact Hr]. intros l Hl. apply filter_In in Hl. apply HF. tauto.
Qed.

Lemma gst_join_r : forall bm e a b, gst bm e b -> gst bm e (fjoin a b).
Proof.
  intros bm e [fa |] [fb |] H; simpl in *; try contradiction; try exact H.
  destruct H as [HF Hr]. split; [| exact Hr]. intros l Hl. apply filter_In in Hl. destruct Hl as [_ Hm].
  apply HF. apply memf_in. exact Hm.
Qed.

Lemma gst_join_some : forall bm e a b, gst bm e a -> exists G, fjoin a b = Some G /\ good bm e G.
Proof.
  intros bm e a b H. pose proof (gst_join_l bm e a b H) as Hj.
  destruct (fjoin a b) as [G |]; [exists G; tauto | contradiction].
Qed.

Lemma gst_join_some_r : forall bm e a b, gst bm e b -> exists G, fjoin a b = Some G /\ good bm e G.
Proof.
  intros bm e a b H. pose proof (gst_join_r bm e a b H) as Hj.
  destruct (fjoin a b) as [G |]; [exists G; tauto | contradiction].
Qed.

(* The arms of a test or branch, then the rest. *)
Lemma after_arms_sound : forall lc C rt re r o (thn_taken : bool) rarm,
  after_arms lc rt re = Some r ->
  (if thn_taken then rt else re) = Some rarm ->
  (* the arm stopped *)
  (stops o -> post_ok C rarm o -> post_ok C r o).
Proof.
  intros lc C [[st bt] |] [[se be] |] r o side rarm H Harm Hs Hp; unfold after_arms in H; try discriminate.
  destruct o as [bm e | bm e | bm e | bm e |]; simpl in Hs; try contradiction; simpl in Hp |- *; try exact Hp.
  (* OBrk: the arm's break joins the result's *)
  assert (Hb : gst bm e (fjoin bt be)).
  { destruct side; injection Harm as <-; simpl in Hp; [apply gst_join_l | apply gst_join_r]; exact Hp. }
  destruct (fjoin st se) as [G |].
  - destruct (lc G) as [[sk bk] |]; [| discriminate]. injection H as <-. simpl. apply gst_join_l. exact Hb.
  - injection H as <-. exact Hb.
Qed.

Lemma after_arms_done : forall lc C rt re r (side : bool) rarm bm1 e1,
  after_arms lc rt re = Some r ->
  (if side then rt else re) = Some rarm ->
  gst bm1 e1 (fst rarm) ->
  exists G rk, lc G = Some rk /\ good bm1 e1 G /\
    (forall o, post_ok C rk o -> post_ok C r o).
Proof.
  intros lc C [[st bt] |] [[se be] |] r side rarm bm1 e1 H Harm Hg; unfold after_arms in H; try discriminate.
  assert (Hj : exists G, fjoin st se = Some G /\ good bm1 e1 G).
  { destruct side; injection Harm as <-; simpl in Hg; [apply gst_join_some | apply gst_join_some_r]; exact Hg. }
  destruct Hj as [G [HG Hgood]]. rewrite HG in H.
  destruct (lc G) as [[sk bk] |] eqn:Hk; [| discriminate]. injection H as <-.
  exists G, (sk, bk). split; [exact Hk | split; [exact Hgood |]].
  intros [bm e | bm e | bm e | bm e |] Hp; simpl in *; try exact Hp. apply gst_join_r. exact Hp.
Qed.

(* ── Frames: what a run leaves alone ─────────────────────────────────── *)

Definition ostate (o : Out) : option (Bm * Env) :=
  match o with
  | ODone bm e | OBrk bm e | OCont bm e | ORet bm e => Some (bm, e)
  | OAbort => None
  end.

Lemma find_assoc : forall (outs : list (Slot * Slot)) y,
  match find (fun q => Nat.eqb (fst q) y) outs with Some (_, r') => Some r' | None => None end = assoc y outs.
Proof.
  intros outs y. induction outs as [| [a b] outs IH]; simpl; [reflexivity |].
  destruct (Nat.eqb a y); [reflexivity | exact IH].
Qed.

Lemma assoc_none_notin : forall (outs : list (Slot * Slot)) y, ~ In y (map fst outs) -> assoc y outs = None.
Proof.
  intros outs y. induction outs as [| [a b] outs IH]; intro H; simpl; [reflexivity |].
  simpl in H. destruct (Nat.eqb a y) eqn:E; [apply Nat.eqb_eq in E; exfalso; apply H; left; exact E |].
  apply IH. tauto.
Qed.

Lemma bouts_other : forall bm bm1 outs y, assoc y outs = None -> bouts bm bm1 outs y = bm y.
Proof.
  intros bm bm1 outs y H. unfold bouts. pose proof (find_assoc outs y) as Hf. rewrite H in Hf.
  destruct (find (fun q => Nat.eqb (fst q) y) outs) as [[a b] |]; [discriminate | reflexivity].
Qed.

Lemma bouts_out : forall bm bm1 outs y r', assoc y outs = Some r' -> bouts bm bm1 outs y = bm1 r'.
Proof.
  intros bm bm1 outs y r' H. unfold bouts. pose proof (find_assoc outs y) as Hf. rewrite H in Hf.
  destruct (find (fun q => Nat.eqb (fst q) y) outs) as [[a b] |]; [injection Hf as ->; reflexivity | discriminate].
Qed.

Lemma ni_if : forall (f : LProg -> list Slot) y (c : bool) a b k, ~ In y (f a ++ f b ++ f k) -> ~ In y (f (if c then a else b)).
Proof. intros f y [|] a b k H Hin; apply H; rewrite !in_app_iff; tauto. Qed.
Lemma ni_r3 : forall y (A B C : list Slot), ~ In y (A ++ B ++ C) -> ~ In y C.
Proof. intros y A B C H Hin. apply H. rewrite !in_app_iff. tauto. Qed.
Lemma ni_l2 : forall y (A B : list Slot), ~ In y (A ++ B) -> ~ In y A.
Proof. intros y A B H Hin. apply H. rewrite in_app_iff. tauto. Qed.
Lemma ni_r2 : forall y (A B : list Slot), ~ In y (A ++ B) -> ~ In y B.
Proof. intros y A B H Hin. apply H. rewrite in_app_iff. tauto. Qed.
Lemma ni_tl : forall y h (A : list Slot), ~ In y (h :: A) -> ~ In y A.
Proof. intros y h A H Hin. apply H. right. exact Hin. Qed.
Lemma upd_ni : forall e h v y (A : list Slot), ~ In y (h :: A) -> upd e h v y = e y.
Proof.
  intros e h v y A H. unfold upd. destruct (Nat.eqb y h) eqn:E; [| reflexivity].
  apply Nat.eqb_eq in E. subst. exfalso. apply H. left. reflexivity.
Qed.
Lemma bupd_ni : forall bm r b y (A : list Slot), ~ In y (r :: A) -> bupd bm r b y = bm y.
Proof.
  intros bm r b y A H. unfold bupd. destruct (Nat.eqb y r) eqn:E; [| reflexivity].
  apply Nat.eqb_eq in E. subst. exfalso. apply H. left. reflexivity.
Qed.

Lemma lrun_frame : forall bm e p t o, lrun P lb base ok bm e p t o ->
  forall bm1 e1, ostate o = Some (bm1, e1) ->
  (forall x, ~ In x (writes p) -> e1 x = e x) /\ (forall r, ~ In r (rebinds p) -> bm1 r = bm r).
Proof.
  intros bm e p t o Hrun. induction Hrun; intros bm2 e2 Hs; cbn [writes rebinds ostate] in *.
  - (* End *) injection Hs as <- <-. split; intros; reflexivity.
  - (* Acc *) exact (IHHrun bm2 e2 Hs).
  - (* Sto *) exact (IHHrun bm2 e2 Hs).
  - (* Handle *) destruct (IHHrun bm2 e2 Hs) as [Hw Hr]. split; intros y Hy; [| apply Hr; exact Hy].
    rewrite (Hw y (ni_tl _ _ _ Hy)). exact (upd_ni e h _ y _ Hy).
  - (* Len *) destruct (IHHrun bm2 e2 Hs) as [Hw Hr]. split; intros y Hy; [| apply Hr; exact Hy].
    rewrite (Hw y (ni_tl _ _ _ Hy)). exact (upd_ni e n _ y _ Hy).
  - (* Alloc *) destruct (IHHrun bm2 e2 Hs) as [Hw Hr]. split; intros y Hy.
    + rewrite (Hw y (ni_tl _ _ _ Hy)). exact (upd_ni e h _ y _ Hy).
    + rewrite (Hr y (ni_tl _ _ _ Hy)). exact (bupd_ni bm r b y _ Hy).
  - (* Wf *) exact (IHHrun bm2 e2 Hs).
  - (* Static *) destruct (IHHrun bm2 e2 Hs) as [Hw Hr]. split; intros y Hy; [exact (Hw y Hy) |].
    rewrite (Hr y (ni_tl _ _ _ Hy)). exact (bupd_ni bm r b y _ Hy).
  - (* Set *) destruct (IHHrun bm2 e2 Hs) as [Hw Hr]. split; intros y Hy; [| apply Hr; exact Hy].
    rewrite (Hw y (ni_tl _ _ _ Hy)). exact (upd_ni e x _ y _ Hy).
  - (* SetR *) destruct (IHHrun bm2 e2 Hs) as [Hw Hr]. split; intros y Hy; [| apply Hr; exact Hy].
    rewrite (Hw y (ni_tl _ _ _ Hy)). exact (upd_ni e x _ y _ Hy).
  - (* Asn *) destruct (IHHrun bm2 e2 Hs) as [Hw Hr]. split; intros y Hy; [| apply Hr; exact Hy].
    rewrite (Hw y (ni_tl _ _ _ Hy)). exact (upd_ni e x _ y _ Hy).
  - (* Test *) destruct (IHHrun1 bm1 e1 eq_refl) as [Hw1 Hr1]. destruct (IHHrun2 bm2 e2 Hs) as [Hw2 Hr2].
    split; intros y Hy.
    + rewrite (Hw2 y (ni_r3 _ _ _ _ Hy)). exact (Hw1 y (ni_if writes y _ _ _ _ Hy)).
    + rewrite (Hr2 y (ni_r3 _ _ _ _ Hy)). exact (Hr1 y (ni_if rebinds y _ _ _ _ Hy)).
  - (* TestStop *) destruct (IHHrun bm2 e2 Hs) as [Hw Hr].
    split; intros y Hy; [exact (Hw y (ni_if writes y _ _ _ _ Hy)) | exact (Hr y (ni_if rebinds y _ _ _ _ Hy))].
  - (* Branch *) destruct (IHHrun1 bm1 e1 eq_refl) as [Hw1 Hr1]. destruct (IHHrun2 bm2 e2 Hs) as [Hw2 Hr2].
    split; intros y Hy.
    + rewrite (Hw2 y (ni_r3 _ _ _ _ Hy)). exact (Hw1 y (ni_if writes y _ _ _ _ Hy)).
    + rewrite (Hr2 y (ni_r3 _ _ _ _ Hy)). exact (Hr1 y (ni_if rebinds y _ _ _ _ Hy)).
  - (* BranchStop *) destruct (IHHrun bm2 e2 Hs) as [Hw Hr].
    split; intros y Hy; [exact (Hw y (ni_if writes y _ _ _ _ Hy)) | exact (Hr y (ni_if rebinds y _ _ _ _ Hy))].
  - (* LoopExit *)
    assert (Hs1 : ostate o1 = Some (bm1, e1)) by (destruct H as [-> | ->]; reflexivity).
    destruct (IHHrun1 bm1 e1 Hs1) as [Hw1 Hr1]. destruct (IHHrun2 bm2 e2 Hs) as [Hw2 Hr2].
    split; intros y Hy.
    + rewrite (Hw2 y (ni_r2 _ _ _ Hy)). exact (Hw1 y (ni_l2 _ _ _ Hy)).
    + rewrite (Hr2 y (ni_r2 _ _ _ Hy)). exact (Hr1 y (ni_l2 _ _ _ Hy)).
  - (* LoopAgain *) destruct (IHHrun1 bm1 e1 eq_refl) as [Hw1 Hr1]. destruct (IHHrun2 bm2 e2 Hs) as [Hw2 Hr2].
    split; intros y Hy; [rewrite (Hw2 y Hy) | rewrite (Hr2 y Hy)]; [exact (Hw1 y (ni_l2 _ _ _ Hy)) | exact (Hr1 y (ni_l2 _ _ _ Hy))].
  - (* LoopStop *) destruct (IHHrun bm2 e2 Hs) as [Hw Hr].
    split; intros y Hy; [exact (Hw y (ni_l2 _ _ _ Hy)) | exact (Hr y (ni_l2 _ _ _ Hy))].
  - (* Cont *) injection Hs as <- <-. split; intros; reflexivity.
  - (* Brk *) injection Hs as <- <-. split; intros; reflexivity.
  - (* Trap *) discriminate.
  - (* Call *) destruct (IHHrun2 bm2 e2 Hs) as [Hw Hr]. split; intros y Hy.
    + rewrite (Hw y (ni_tl _ _ _ Hy)). exact (upd_ni e res _ y _ Hy).
    + rewrite (Hr y (ni_r2 _ _ _ Hy)). apply bouts_other. apply assoc_none_notin. exact (ni_l2 _ _ _ Hy).
  - (* CallAbort *) discriminate.
  - (* Ret *) injection Hs as <- <-. split; intros; reflexivity.
Qed.


(* ── Each item's facts hold of the state after it ───────────────────── *)

Lemma acc_sound : forall bm e F r w off addr p0 p1 p2, good bm e F -> acc_ok F r w off addr p0 p1 p2 = true ->
  ok (bm r) /\ 0 <= ev_off lb base bm e r off addr /\ 0 < w /\ ev_off lb base bm e r off addr + w <= bytes_of (lb (bm r)).
Proof.
  intros bm e F r w off addr p0 p1 p2 [HF [Hi Ho]] H. unfold acc_ok in H.
  rewrite !andb_true_iff in H. destruct H as [[[[Hw Hoff] H0] H1] H2].
  apply Z.ltb_lt in Hw. apply Z.leb_le in Hoff.
  apply (entails_sound _ _ _ _ HF) in H0. apply (entails_sound _ _ _ _ HF) in H1. apply (entails_sound _ _ _ _ HF) in H2.
  evals. unfold ev_off. remember (eval (val lb base bm e) addr) as A eqn:HA. clear HA.
  cbn [val] in *. destruct (Hwf (bm r)) as [Hb0 [Hc0 Hlt]].
  rewrite Z.mod_small by (unfold ADDR, PAYLOAD in *; lia).
  split; [apply Ho |]. unfold bytes_of, PAYLOAD in *. lia.
Qed.

Lemma lsat_upd : forall bm e x z F, lsat (rupd (val lb base bm e) (VS x) z) F -> lsat (val lb base bm (upd e x z)) F.
Proof. intros bm e x z F H. apply (lsat_ext (rupd (val lb base bm e) (VS x) z)); [intro v; symmetry; apply val_upd | exact H]. Qed.

Lemma upd_same : forall e x z, upd e x z x = z.
Proof. intros. unfold upd. rewrite Nat.eqb_refl. reflexivity. Qed.

Lemma good_handle : forall bm e F h r, good bm e F ->
  good bm (upd e h (base (bm r))) (eqs (VS h) (v1 (VB r)) ++ kill F (VS h)).
Proof.
  intros bm e F h r Hg. destruct (good_kill_upd bm e F h (base (bm r)) Hg (base_i64 _)) as [HK [Hi Ho]].
  split; [| tauto]. apply lsat_app; [| exact HK]. apply eqs_sound. evals. cbn [val]. apply upd_same.
Qed.

Lemma good_len : forall bm e F n r s v, good bm e F -> is_i64 v -> 0 <= v -> s * v <= CAP (lb (bm r)) ->
  good bm (upd e n v) (lneg (v1 (VS n)) :: lsub (lmul s (v1 (VS n))) (v1 (VC r)) :: kill F (VS n)).
Proof.
  intros bm e F n r s v Hg Hv H0 Hs. destruct (good_kill_upd bm e F n v Hg Hv) as [HK [Hi Ho]].
  split; [| tauto]. apply lsat_cons; [| apply lsat_cons; [| exact HK]]; evals; cbn [val]; rewrite upd_same; lia.
Qed.

Lemma good_setr : forall bm e F x lo hi v, good bm e F -> is_i64 v -> lo <= v <= hi ->
  good bm (upd e x v) (lsub (kz lo) (v1 (VS x)) :: lsub (v1 (VS x)) (kz hi) :: kill F (VS x)).
Proof.
  intros bm e F x lo hi v Hg Hv Hr. destruct (good_kill_upd bm e F x v Hg Hv) as [HK [Hi Ho]].
  split; [| tauto]. apply lsat_cons; [| apply lsat_cons; [| exact HK]]; evals; cbn [val]; rewrite upd_same; lia.
Qed.

Lemma good_wf : forall bm e F r, good bm e F ->
  good bm e (lneg (v1 (VB r)) :: lneg (v1 (VC r)) :: ladd (ladd (v1 (VB r)) (v1 (VC r))) (kz (PAYLOAD - ADDR + 1)) :: F).
Proof.
  intros bm e F r [HF Hr]. split; [| exact Hr]. destruct (Hwf (bm r)) as [H0 [H1 H2]].
  repeat apply lsat_cons; try exact HF; evals; cbn [val]; unfold PAYLOAD, ADDR in *; lia.
Qed.

Lemma val_alloc : forall bm e h r b v,
  val lb base (bupd bm r b) (upd e h (base b)) v =
  rupd (rupd (rupd (val lb base bm e) (VS h) (base b)) (VB r) (base b)) (VC r) (CAP (lb b)) v.
Proof.
  intros bm e h r b [y | y | y]; unfold val, rupd, upd, bupd; simpl; [destruct (Nat.eqb y h); reflexivity | |];
    destruct (Nat.eqb y r); reflexivity.
Qed.

Lemma good_alloc : forall bm e F h r sz lo hi b, good bm e F -> ok b ->
  no_wrap F sz (lo, hi) = true -> mentions (VS h) sz = false -> mentions (VB r) sz = false -> mentions (VC r) sz = false ->
  wrap (eval (val lb base bm e) sz) <= CAP (lb b) ->
  good (bupd bm r b) (upd e h (base b)) (eqs (VS h) (v1 (VB r)) ++ lsub sz (v1 (VC r)) :: kill_region (kill F (VS h)) r).
Proof.
  intros bm e F h r sz lo hi b [HF [Hi Ho]] Hb Hnw Hmh Hmb Hmc Hcap.
  pose proof (no_wrap_sound _ F sz _ (val_vi64 bm e Hi) HF Hnw) as Hr.
  rewrite wrap_small in Hcap by exact Hr.
  set (rho := val lb base bm e) in *.
  set (rho' := rupd (rupd (rupd rho (VS h) (base b)) (VB r) (base b)) (VC r) (CAP (lb b))).
  assert (Hv : forall v, rho' v = val lb base (bupd bm r b) (upd e h (base b)) v) by (intro v; symmetry; apply val_alloc).
  split; [| split].
  - apply (lsat_ext rho'); [exact Hv |]. apply lsat_app.
    + apply eqs_sound. evals. unfold rho', rupd. simpl. rewrite !Nat.eqb_refl. reflexivity.
    + apply lsat_cons.
      * evals.
        assert (H1 : rho' (VC r) = CAP (lb b)) by (unfold rho', rupd; simpl; rewrite Nat.eqb_refl; reflexivity).
        assert (H2 : eval rho' sz = eval rho sz) by (unfold rho'; rewrite !eval_rupd_other by assumption; reflexivity).
        lia.
      * unfold kill_region. unfold rho'. apply kill_sound, kill_sound, kill_sound. exact HF.
  - apply i64_upd; [exact Hi | apply base_i64].
  - intro y. unfold bupd. destruct (Nat.eqb y r); [exact Hb | apply Ho].
Qed.

Lemma val_static : forall bm e r b v,
  val lb base (bupd bm r b) e v = rupd (rupd (val lb base bm e) (VB r) (base b)) (VC r) (CAP (lb b)) v.
Proof.
  intros bm e r b [y | y | y]; unfold val, rupd, bupd; simpl; [reflexivity | |]; destruct (Nat.eqb y r); reflexivity.
Qed.

Lemma good_static : forall bm e F r b0 c0 b, good bm e F -> ok b -> base b = b0 -> CAP (lb b) = c0 ->
  good (bupd bm r b) e (eqs (VB r) (kz b0) ++ eqs (VC r) (kz c0) ++ kill_region F r).
Proof.
  intros bm e F r b0 c0 b [HF [Hi Ho]] Hb Hb0 Hc0.
  set (rho' := rupd (rupd (val lb base bm e) (VB r) (base b)) (VC r) (CAP (lb b))).
  split; [| split; [exact Hi |]].
  - apply (lsat_ext rho'); [intro v; symmetry; apply val_static |].
    repeat apply lsat_app.
    + apply eqs_sound. evals. unfold rho', rupd. simpl. rewrite Nat.eqb_refl. exact Hb0.
    + apply eqs_sound. evals. unfold rho', rupd. simpl. rewrite Nat.eqb_refl. exact Hc0.
    + unfold kill_region, rho'. apply kill_sound, kill_sound. exact HF.
  - intro y. unfold bupd. destruct (Nat.eqb y r); [exact Hb | apply Ho].
Qed.

Lemma eval_coef0 : forall rho v z l, coef (terms l) v = 0 -> eval (rupd rho v z) l = eval rho l.
Proof.
  intros rho v z l H. unfold eval.
  rewrite (eval_terms_drop (rupd rho v z) v), (eval_terms_drop rho v), eval_terms_drop_rupd, H. lia.
Qed.

Lemma rupd_back : forall rho v z u, rupd (rupd rho v z) v (rho v) u = rho u.
Proof.
  intros. unfold rupd. destruct (var_eqb u v) eqn:E; [apply var_eqb_spec in E; subst; reflexivity | reflexivity].
Qed.

Lemma good_asn : forall bm e F x l lo hi, good bm e F ->
  good bm (upd e x (wrap (eval (val lb base bm e) l))) (asn_facts F x l lo hi).
Proof.
  intros bm e F x l lo hi Hg. pose proof Hg as [HF [Hi Ho]].
  split; [| split; [apply i64_upd; [exact Hi | apply wrap_i64] | exact Ho]].
  apply lsat_upd. set (rho := val lb base bm e) in *. set (z := wrap (eval rho l)). unfold asn_facts.
  destruct (no_wrap F l (lo, hi)) eqn:Hnw; [| apply kill_sound; exact HF].
  pose proof (no_wrap_sound rho F l _ (val_vi64 bm e Hi) HF Hnw) as Hr.
  assert (Hz : z = eval rho l) by (apply wrap_small; exact Hr).
  destruct (Z.eqb (coef (terms l) (VS x)) 0) eqn:E0.
  - apply Z.eqb_eq in E0. apply lsat_app; [| apply kill_sound; exact HF].
    apply eqs_sound. unfold rupd at 1. rewrite var_eqb_refl. rewrite eval_coef0 by exact E0. exact Hz.
  - destruct (Z.eqb (coef (terms l) (VS x)) 1) eqn:E1; [| apply kill_sound; exact HF].
    apply Z.eqb_eq in E1. intros f Hf. apply in_map_iff in Hf. destruct Hf as [f0 [<- Hf0]].
    rewrite eval_subst.
    assert (Hd : eval (rupd rho (VS x) z) (lsub (v1 (VS x)) (mkL (dropv (VS x) (terms l)) (cst l))) = rho (VS x)).
    { evals. unfold eval at 1. cbn [terms cst]. rewrite eval_terms_drop_rupd.
      unfold rupd at 1. rewrite var_eqb_refl. rewrite Hz. unfold eval.
      rewrite (eval_terms_drop rho (VS x) (terms l)), E1. lia. }
    rewrite Hd. rewrite (eval_ext _ rho) by (intros; apply rupd_back). apply HF. exact Hf0.
Qed.

Lemma unsigned_small : forall v, 0 <= v <= I64MAX -> unsigned v = v.
Proof. intros v Hv. unfold unsigned, I64MAX in *. apply Z.mod_small. lia. Qed.

Lemma test_sound : forall bm e F a b sgn pa pb nn ft fe, good bm e F ->
  test_facts F a b sgn pa pb nn = (ft, fe) ->
  let rho := val lb base bm e in
  (cmp_ge sgn (wrap (eval rho a)) (wrap (eval rho b)) = true -> lsat rho ft) /\
  (cmp_ge sgn (wrap (eval rho a)) (wrap (eval rho b)) = false -> lsat rho fe).
Proof.
  intros bm e F a b sgn pa pb nn ft fe [HF [Hi Ho]] Ht rho. change (val lb base bm e) with rho in HF. unfold test_facts in Ht.
  destruct (no_wrap F a pa && no_wrap F b pb && (sgn || (entails F (fst nn) (lneg a) && entails F (snd nn) (lneg b)))) eqn:Hc;
    [| injection Ht as <- <-; split; intros _ l []].
  injection Ht as <- <-. rewrite !andb_true_iff in Hc. destruct Hc as [[Ha Hb] Hs].
  pose proof (no_wrap_sound rho F a pa (val_vi64 bm e Hi) HF Ha) as Hra.
  pose proof (no_wrap_sound rho F b pb (val_vi64 bm e Hi) HF Hb) as Hrb.
  rewrite (wrap_small _ Hra), (wrap_small _ Hrb).
  assert (Hcmp : cmp_ge sgn (eval rho a) (eval rho b) = (eval rho b <=? eval rho a)).
  { destruct sgn; [reflexivity |]. simpl in Hs. apply andb_true_iff in Hs. destruct Hs as [Hna Hnb].
    apply (entails_sound _ _ _ _ HF) in Hna. apply (entails_sound _ _ _ _ HF) in Hnb. evals.
    unfold cmp_ge. rewrite !unsigned_small by lia. reflexivity. }
  rewrite Hcmp. split; intro H.
  - apply Z.leb_le in H. apply lsat_cons; [evals; lia | intros l []].
  - apply Z.leb_gt in H. apply lsat_cons; [evals; lia | intros l []].
Qed.

(* ── Calls ────────────────────────────────────────────────────────────── *)

Lemma tr_terms_sound : forall tr rhoc rhof ts m, tr_terms tr ts = Some m ->
  (forall v d, tr v = Some d -> eval rhoc d = rhof v) -> eval rhoc m = eval_terms rhof ts.
Proof.
  intros tr rhoc rhof ts. induction ts as [| [v a] ts IH]; intros m Hm Htr; simpl in Hm.
  - injection Hm as <-. reflexivity.
  - destruct (tr v) as [d |] eqn:Hv; [| discriminate].
    destruct (tr_terms tr ts) as [rest |] eqn:Hr; [| discriminate].
    injection Hm as <-. evals. rewrite (Htr v d Hv), (IH rest eq_refl Htr). rewrite eval_terms_cons. simpl. lia.
Qed.

Lemma tr_lin_sound : forall tr rhoc rhof l l', tr_lin tr l = Some l' ->
  (forall v d, tr v = Some d -> eval rhoc d = rhof v) -> eval rhoc l' = eval rhof l.
Proof.
  intros tr rhoc rhof l l' H Htr. unfold tr_lin in H.
  destruct (tr_terms tr (terms l)) as [m |] eqn:Hm; [| discriminate]. injection H as <-.
  evals. rewrite (tr_terms_sound tr rhoc rhof (terms l) m Hm Htr). reflexivity.
Qed.

Lemma tr_all_sound : forall tr rhoc rhof G G', tr_all tr G = Some G' ->
  (forall v d, tr v = Some d -> eval rhoc d = rhof v) -> lsat rhoc G' -> lsat rhof G.
Proof.
  intros tr rhoc rhof G. induction G as [| g G IH]; intros G' H Htr HG' l Hl; [destruct Hl |].
  simpl in H. destruct (tr_lin tr g) as [g' |] eqn:Hg; [| discriminate].
  destruct (tr_all tr G) as [G'' |] eqn:HGs; [| discriminate]. injection H as <-.
  destruct Hl as [<- | Hl].
  - rewrite <- (tr_lin_sound tr rhoc rhof _ g' Hg Htr). apply HG'. left. reflexivity.
  - apply (IH G'' eq_refl Htr); [intros l' Hl'; apply HG'; right; exact Hl' | exact Hl].
Qed.

Lemma tr_some_sound : forall tr rhot rhos G,
  (forall v d, tr v = Some d -> eval rhot d = rhos v) -> lsat rhos G -> lsat rhot (tr_some tr G).
Proof.
  intros tr rhot rhos G Htr HG l Hl. unfold tr_some in Hl. apply in_flat_map in Hl.
  destruct Hl as [g [Hg Hl]]. destruct (tr_lin tr g) as [g' |] eqn:Htg; [| destruct Hl].
  destruct Hl as [<- | []]. rewrite (tr_lin_sound tr rhot rhos g g' Htg Htr). apply HG. exact Hg.
Qed.

Lemma assoc_in : forall {A : Type} (l : list (Slot * A)) k a, assoc k l = Some a -> In (k, a) l.
Proof.
  intros A l k a. induction l as [| [k' b] l IH]; simpl; [discriminate |].
  destruct (Nat.eqb k' k) eqn:E; intro H.
  - injection H as <-. apply Nat.eqb_eq in E. subst. left. reflexivity.
  - right. apply IH. exact H.
Qed.

Lemma valid_args_in : forall F args p l, In (p, l) (valid_args F args) ->
  exists pf, In (p, (l, Some pf)) args /\ no_wrap F l pf = true.
Proof.
  intros F args p l H. unfold valid_args in H. apply in_flat_map in H.
  destruct H as [[q [m [pf |]]] [Hin Hm]]; cbn [fst snd] in Hm; [| destruct Hm].
  destruct (no_wrap F m pf) eqn:Hn; [| destruct Hm]. destruct Hm as [Hq | []].
  injection Hq as -> ->. exists pf. tauto.
Qed.

Lemma arg_exact : forall bm e F args e' p l, good bm e F ->
  (forall p a, In (p, a) args -> e' p = wrap (eval (val lb base bm e) (fst a))) ->
  assoc p (valid_args F args) = Some l -> e' p = eval (val lb base bm e) l.
Proof.
  intros bm e F args e' p l [HF [Hi Ho]] Hargs Hl. apply assoc_in in Hl.
  destruct (valid_args_in F args p l Hl) as [pf [Hin Hn]].
  rewrite (Hargs p _ Hin). simpl. apply wrap_small. exact (no_wrap_sound _ F l pf (val_vi64 bm e Hi) HF Hn).
Qed.

Lemma tr_in_sound : forall bm e F args reps bm' e', good bm e F ->
  (forall p a, In (p, a) args -> e' p = wrap (eval (val lb base bm e) (fst a))) ->
  (forall r' r, In (r', r) reps -> bm' r' = bm r) ->
  forall v d, tr_in (valid_args F args) reps v = Some d -> eval (val lb base bm e) d = val lb base bm' e' v.
Proof.
  intros bm e F args reps bm' e' Hg Hargs Hreps [p | r' | r'] d H; simpl in H.
  - simpl. symmetry. exact (arg_exact bm e F args e' p d Hg Hargs H).
  - destruct (assoc r' reps) as [r |] eqn:Hr; [| discriminate]. injection H as <-.
    evals. simpl. rewrite (Hreps r' r (assoc_in reps r' r Hr)). reflexivity.
  - destruct (assoc r' reps) as [r |] eqn:Hr; [| discriminate]. injection H as <-.
    evals. simpl. rewrite (Hreps r' r (assoc_in reps r' r Hr)). reflexivity.
Qed.

Lemma kill_outs_in : forall outs F0 l, In l (kill_outs F0 outs) ->
  In l F0 /\ forall q, In q outs -> mentions (VB (fst q)) l = false /\ mentions (VC (fst q)) l = false.
Proof.
  intros outs. induction outs as [| q outs IH]; intros F0 l H; simpl in H; [split; [exact H | intros _ []] |].
  destruct (IH _ l H) as [Hk Hq]. unfold kill_region, kill in Hk.
  apply filter_In in Hk. destruct Hk as [Hk Hc]. apply filter_In in Hk. destruct Hk as [Hk Hb].
  apply negb_true_iff in Hb. apply negb_true_iff in Hc.
  split; [exact Hk |]. intros q' [<- | Hq']; [split; assumption | apply Hq; exact Hq'].
Qed.

Lemma notin_outs : forall l (outs : list (Slot * Slot)) y x,
  (forall q, In q outs -> mentions (VB (fst q)) l = false /\ mentions (VC (fst q)) l = false) ->
  assoc y outs = Some x -> (mentions (VB y) l = false /\ mentions (VC y) l = false).
Proof. intros l outs y x H Hy. apply assoc_in in Hy. exact (H _ Hy). Qed.

Lemma eval_bouts : forall bm bm1 outs e l,
  (forall q, In q outs -> mentions (VB (fst q)) l = false /\ mentions (VC (fst q)) l = false) ->
  eval (val lb base (bouts bm bm1 outs) e) l = eval (val lb base bm e) l.
Proof.
  intros bm bm1 outs e l H. apply eval_ext. intros [y | y | y] Hy; simpl; [reflexivity | |];
    destruct (assoc y outs) as [x |] eqn:Hx;
    try (rewrite bouts_other by exact Hx; reflexivity);
    destruct (notin_outs l outs y x H Hx) as [Hb Hc];
    [exfalso; exact (mentions_false _ l Hb _ Hy eq_refl) | exfalso; exact (mentions_false _ l Hc _ Hy eq_refl)].
Qed.

Lemma kill_outs_sound : forall bm bm1 outs e F,
  lsat (val lb base bm e) F -> lsat (val lb base (bouts bm bm1 outs) e) (kill_outs F outs).
Proof.
  intros bm bm1 outs e F HF l Hl. destruct (kill_outs_in outs F l Hl) as [Hin Hq].
  rewrite eval_bouts by exact Hq. apply HF. exact Hin.
Qed.

Lemma out_of_some : forall outs r' r, out_of outs r' = Some r -> assoc r outs = Some r'.
Proof.
  intros outs r' r H. unfold out_of in H.
  destruct (find (fun q => match assoc (fst q) outs with Some x => Nat.eqb x r' | None => false end) outs) as [[a b] |] eqn:Hf;
    [| discriminate]. injection H as <-. apply find_some in Hf. destruct Hf as [_ Hp]. simpl in Hp.
  destruct (assoc a outs) as [x |]; [apply Nat.eqb_eq in Hp; subst; reflexivity | discriminate].
Qed.

Lemma call_ok_args : forall A reps outs res, call_ok A reps outs res = true ->
  forall p l, In (p, l) A -> mentions (VS res) l = false /\
    forall q, In q outs -> mentions (VB (fst q)) l = false /\ mentions (VC (fst q)) l = false.
Proof.
  intros A reps outs res H p l Hin. unfold call_ok in H. apply andb_true_iff in H. destruct H as [H _].
  rewrite forallb_forall in H. specialize (H _ Hin). simpl in H. apply andb_true_iff in H. destruct H as [Hr Ho].
  apply negb_true_iff in Hr. split; [exact Hr |]. rewrite forallb_forall in Ho. intros q Hq.
  specialize (Ho q Hq). apply andb_true_iff in Ho. destruct Ho as [Hb Hc].
  apply negb_true_iff in Hb. apply negb_true_iff in Hc. tauto.
Qed.

Lemma call_ok_reps : forall A reps outs res, call_ok A reps outs res = true ->
  forall r' r, In (r', r) reps -> assoc r outs = None.
Proof.
  intros A reps outs res H r' r Hin. unfold call_ok in H. apply andb_true_iff in H. destruct H as [_ H].
  rewrite forallb_forall in H. specialize (H _ Hin). apply negb_true_iff in H. simpl in H.
  destruct (assoc r outs) as [x |] eqn:Hx; [| reflexivity]. apply assoc_in in Hx.
  assert (existsb (fun q' => Nat.eqb (fst q') r) outs = true)
    by (apply existsb_exists; exists (r, x); split; [exact Hx | apply Nat.eqb_refl]).
  congruence.
Qed.

Lemma existsb_nat_false : forall y l, existsb (Nat.eqb y) l = false -> ~ In y l.
Proof.
  intros y l H Hin. assert (existsb (Nat.eqb y) l = true) by (apply existsb_exists; exists y; split; [exact Hin | apply Nat.eqb_refl]).
  congruence.
Qed.

Lemma tr_out_sound : forall bm e F args reps outs res d bm' e' bm1 e1, good bm e F ->
  (forall p a, In (p, a) args -> e' p = wrap (eval (val lb base bm e) (fst a))) ->
  (forall r' r, In (r', r) reps -> bm' r' = bm r) ->
  call_ok (valid_args F args) reps outs res = true ->
  (forall x, ~ In x (writes (body d)) -> e1 x = e' x) ->
  (forall r, ~ In r (rebinds (body d)) -> bm1 r = bm' r) ->
  forall v l, tr_out (valid_args F args) reps outs d res v = Some l ->
  eval (val lb base (bouts bm bm1 outs) (upd e res (e1 (rslot d)))) l = val lb base bm1 e1 v.
Proof.
  intros bm e F args reps outs res d bm' e' bm1 e1 Hg Hargs Hreps Hok Hw Hr [p | r' | r'] l H; simpl in H.
  - destruct (Nat.eqb p (rslot d)) eqn:Ep.
    + injection H as <-. evals. simpl. rewrite upd_same. apply Nat.eqb_eq in Ep. subst. reflexivity.
    + destruct (existsb (Nat.eqb p) (writes (body d))) eqn:Ew; [discriminate |].
      simpl. rewrite (Hw p (existsb_nat_false _ _ Ew)), (arg_exact bm e F args e' p l Hg Hargs H).
      destruct (call_ok_args _ _ _ _ Hok p l (assoc_in _ _ _ H)) as [Hres Houts].
      rewrite eval_bouts by exact Houts.
      apply eval_ext. intros u Hu. rewrite val_upd. unfold rupd.
      destruct (var_eqb u (VS res)) eqn:E; [| reflexivity].
      apply var_eqb_spec in E. exfalso. exact (mentions_false _ l Hres u Hu E).
  - destruct (out_of outs r') as [r |] eqn:Ho.
    + injection H as <-. evals. simpl. rewrite (bouts_out bm bm1 outs r r' (out_of_some outs r' r Ho)). reflexivity.
    + destruct (existsb (Nat.eqb r') (rebinds (body d))) eqn:Eb; [discriminate |].
      destruct (assoc r' reps) as [r |] eqn:Hrr; [| discriminate]. injection H as <-.
      evals. simpl. pose proof (assoc_in _ _ _ Hrr) as Hin.
      rewrite (bouts_other bm bm1 outs r (call_ok_reps _ _ _ _ Hok r' r Hin)).
      rewrite (Hr r' (existsb_nat_false _ _ Eb)), (Hreps r' r Hin). reflexivity.
  - destruct (out_of outs r') as [r |] eqn:Ho.
    + injection H as <-. evals. simpl. rewrite (bouts_out bm bm1 outs r r' (out_of_some outs r' r Ho)). reflexivity.
    + destruct (existsb (Nat.eqb r') (rebinds (body d))) eqn:Eb; [discriminate |].
      destruct (assoc r' reps) as [r |] eqn:Hrr; [| discriminate]. injection H as <-.
      evals. simpl. pose proof (assoc_in _ _ _ Hrr) as Hin.
      rewrite (bouts_other bm bm1 outs r (call_ok_reps _ _ _ _ Hok r' r Hin)).
      rewrite (Hr r' (existsb_nat_false _ _ Eb)), (Hreps r' r Hin). reflexivity.
Qed.

Lemma procs_ok_body : forall f d, plook P f = Some d ->
  lcheck P (None, post d) (pre d) (body d) = Some (None, None).
Proof.
  intros f d Hf. unfold procs_ok in HP. rewrite forallb_forall in HP. unfold plook in Hf.
  destruct (find (fun q => Nat.eqb (fst q) f) P) as [[g d'] |] eqn:Hfd; [| discriminate].
  injection Hf as <-. apply find_some in Hfd. destruct Hfd as [Hin _].
  specialize (HP _ Hin). simpl in HP.
  destruct (lcheck P (None, post d') (pre d') (body d')) as [[[x |] [y |]] |]; try discriminate; reflexivity.
Qed.


Lemma after_arms_some : forall lc rt re r, after_arms lc rt re = Some r -> exists a b, rt = Some a /\ re = Some b.
Proof.
  intros lc [a |] [b |] r H; unfold after_arms in H; try discriminate; [| destruct a; discriminate].
  exists a, b. tauto.
Qed.

Lemma good_app : forall bm e A F, lsat (val lb base bm e) A -> good bm e F -> good bm e (A ++ F).
Proof. intros bm e A F HA [HF Hr]. split; [apply lsat_app; assumption | exact Hr]. Qed.

(* The soundness theorem: an accepted program's run touches only what its
   blocks hold, and ends in a state its check describes. *)
Theorem lrun_sound : forall bm e p t o, lrun P lb base ok bm e p t o ->
  forall C F r, good bm e F -> lcheck P C F p = Some r -> Forall lpin t /\ post_ok C r o.
Proof.
  intros bm e p t o Hrun. induction Hrun; intros C F rr Hg Hc; cbn [lcheck] in Hc.
  - (* End *) injection Hc as <-. split; [constructor | exact Hg].
  - (* Acc *) destruct (acc_ok F r w off addr p0 p1 p2) eqn:Ha; [| discriminate].
    destruct (acc_sound bm e F r w off addr p0 p1 p2 Hg Ha) as [Hok [H0 [Hw Hend]]].
    destruct (IHHrun C F rr Hg Hc) as [Ht Hp]. split; [constructor; [simpl; tauto | exact Ht] | exact Hp].
  - (* Sto *) destruct (acc_ok F r w off addr p0 p1 p2) eqn:Ha; [| discriminate].
    destruct (acc_sound bm e F r w off addr p0 p1 p2 Hg Ha) as [Hok [H0 [Hw Hend]]].
    destruct (IHHrun C F rr Hg Hc) as [Ht Hp]. split; [constructor; [simpl; tauto | exact Ht] | exact Hp].
  - (* Handle *) exact (IHHrun C _ rr (good_handle bm e F h r Hg) Hc).
  - (* Len *) exact (IHHrun C _ rr (good_len bm e F n r s v Hg H H0 H1) Hc).
  - (* Alloc *)
    destruct (no_wrap F sz (lo, hi) && negb (mentions (VS h) sz) && negb (mentions (VB r) sz) && negb (mentions (VC r) sz)) eqn:Hcd;
      [| discriminate].
    rewrite !andb_true_iff in Hcd. destruct Hcd as [[[Hnw Hmh] Hmb] Hmc].
    apply negb_true_iff in Hmh. apply negb_true_iff in Hmb. apply negb_true_iff in Hmc.
    exact (IHHrun C _ rr (good_alloc bm e F h r sz lo hi b Hg H Hnw Hmh Hmb Hmc H0) Hc).
  - (* Wf *) exact (IHHrun C _ rr (good_wf bm e F r Hg) Hc).
  - (* Static *) exact (IHHrun C _ rr (good_static bm e F r b0 c0 b Hg H H0 H1) Hc).
  - (* Set *) exact (IHHrun C _ rr (good_kill_upd bm e F x v Hg H) Hc).
  - (* SetR *) exact (IHHrun C _ rr (good_setr bm e F x lo hi v Hg H H0) Hc).
  - (* Asn *) exact (IHHrun C _ rr (good_asn bm e F x l lo hi Hg) Hc).
  - (* Test *)
    destruct (test_facts F a b sgn pa pb nn) as [ft fe] eqn:Htf.
    destruct (test_sound bm e F a b sgn pa pb nn ft fe Hg Htf) as [Hyes Hno]. cbv zeta in Hyes, Hno.
    destruct (after_arms_some _ _ _ _ Hc) as [ra [rb [Hra Hrb]]].
    destruct (cmp_ge sgn (wrap (eval (val lb base bm e) a)) (wrap (eval (val lb base bm e) b))) eqn:Hcmp.
    + destruct (IHHrun1 C (ft ++ F) ra (good_app bm e ft F (Hyes eq_refl) Hg) Hra) as [Ht1 Hp1].
      destruct (after_arms_done _ C _ _ rr true ra bm1 e1 Hc Hra Hp1) as [G [rk [HG [Hgood Htrans]]]].
      destruct (IHHrun2 C G rk Hgood HG) as [Ht2 Hp2].
      split; [apply Forall_app; tauto | apply Htrans; exact Hp2].
    + destruct (IHHrun1 C (fe ++ F) rb (good_app bm e fe F (Hno eq_refl) Hg) Hrb) as [Ht1 Hp1].
      destruct (after_arms_done _ C _ _ rr false rb bm1 e1 Hc Hrb Hp1) as [G [rk [HG [Hgood Htrans]]]].
      destruct (IHHrun2 C G rk Hgood HG) as [Ht2 Hp2].
      split; [apply Forall_app; tauto | apply Htrans; exact Hp2].
  - (* TestStop *)
    destruct (test_facts F a b sgn pa pb nn) as [ft fe] eqn:Htf.
    destruct (test_sound bm e F a b sgn pa pb nn ft fe Hg Htf) as [Hyes Hno]. cbv zeta in Hyes, Hno.
    destruct (after_arms_some _ _ _ _ Hc) as [ra [rb [Hra Hrb]]].
    destruct (cmp_ge sgn (wrap (eval (val lb base bm e) a)) (wrap (eval (val lb base bm e) b))) eqn:Hcmp.
    + destruct (IHHrun C (ft ++ F) ra (good_app bm e ft F (Hyes eq_refl) Hg) Hra) as [Ht1 Hp1].
      split; [exact Ht1 | exact (after_arms_sound _ C _ _ rr o true ra Hc Hra H Hp1)].
    + destruct (IHHrun C (fe ++ F) rb (good_app bm e fe F (Hno eq_refl) Hg) Hrb) as [Ht1 Hp1].
      split; [exact Ht1 | exact (after_arms_sound _ C _ _ rr o false rb Hc Hrb H Hp1)].
  - (* Branch *)
    destruct (after_arms_some _ _ _ _ Hc) as [ra [rb [Hra Hrb]]].
    assert (Harm : lcheck P C F (if side then thn else els) = (if side then Some ra else Some rb))
      by (destruct side; assumption).
    destruct (IHHrun1 C F (if side then ra else rb) Hg ltac:(destruct side; assumption)) as [Ht1 Hp1].
    assert (Hsel : (if side then lcheck P C F thn else lcheck P C F els) = Some (if side then ra else rb))
      by (destruct side; assumption).
    destruct (after_arms_done _ C _ _ rr side _ bm1 e1 Hc Hsel Hp1) as [G [rk [HG [Hgood Htrans]]]].
    destruct (IHHrun2 C G rk Hgood HG) as [Ht2 Hp2].
    split; [apply Forall_app; tauto | apply Htrans; exact Hp2].
  - (* BranchStop *)
    destruct (after_arms_some _ _ _ _ Hc) as [ra [rb [Hra Hrb]]].
    destruct (IHHrun C F (if side then ra else rb) Hg ltac:(destruct side; assumption)) as [Ht1 Hp1].
    assert (Hsel : (if side then lcheck P C F thn else lcheck P C F els) = Some (if side then ra else rb))
      by (destruct side; assumption).
    split; [exact Ht1 | exact (after_arms_sound _ C _ _ rr o side _ Hc Hsel H Hp1)].
  - (* LoopExit *)
    destruct (proves F inv pin) eqn:Hpr; [| discriminate]. cbv iota in Hc.
    destruct (lcheck P (Some inv, snd C) inv b) as [[sb bb] |] eqn:Hb; [| discriminate].
    assert (Hgi : good bm e inv) by (destruct Hg as [HF Hr]; split; [exact (proves_sound _ F inv pin HF Hpr) | exact Hr]).
    destruct (IHHrun1 (Some inv, snd C) inv (sb, bb) Hgi Hb) as [Ht1 Hp1].
    assert (Hj : exists G, fjoin sb bb = Some G /\ good bm1 e1 G).
    { destruct H as [-> | ->]; simpl in Hp1; [apply gst_join_some | apply gst_join_some_r]; exact Hp1. }
    destruct Hj as [G [HG Hgood]]. rewrite HG in Hc.
    destruct (IHHrun2 C G rr Hgood Hc) as [Ht2 Hp2]. split; [apply Forall_app; tauto | exact Hp2].
  - (* LoopAgain *)
    destruct (proves F inv pin) eqn:Hpr; [| discriminate]. cbv iota in Hc.
    destruct (lcheck P (Some inv, snd C) inv b) as [[sb bb] |] eqn:Hb; [| discriminate].
    assert (Hgi : good bm e inv) by (destruct Hg as [HF Hr]; split; [exact (proves_sound _ F inv pin HF Hpr) | exact Hr]).
    destruct (IHHrun1 (Some inv, snd C) inv (sb, bb) Hgi Hb) as [Ht1 Hp1]. simpl in Hp1.
    assert (Hc' : lcheck P C inv (LLoop inv pin b k) = Some rr).
    { cbn [lcheck]. rewrite (proves_self inv inv pin (fun g H => H) (proves_length F inv pin Hpr)).
      cbv iota. rewrite Hb. exact Hc. }
    destruct (IHHrun2 C inv rr Hp1 Hc') as [Ht2 Hp2]. split; [apply Forall_app; tauto | exact Hp2].
  - (* LoopStop *)
    destruct (proves F inv pin) eqn:Hpr; [| discriminate]. cbv iota in Hc.
    destruct (lcheck P (Some inv, snd C) inv b) as [[sb bb] |] eqn:Hb; [| discriminate].
    assert (Hgi : good bm e inv) by (destruct Hg as [HF Hr]; split; [exact (proves_sound _ F inv pin HF Hpr) | exact Hr]).
    destruct (IHHrun (Some inv, snd C) inv (sb, bb) Hgi Hb) as [Ht1 Hp1].
    split; [exact Ht1 |]. destruct H as [-> | [bm1 [e1 ->]]]; simpl in *; [exact I | exact Hp1].
  - (* Cont *)
    destruct (fst C) as [Iv |] eqn:HC; [| discriminate].
    destruct (proves F Iv pfs) eqn:Hpr; [| discriminate]. injection Hc as <-.
    split; [constructor |]. simpl. rewrite HC. destruct Hg as [HF Hr]. split; [exact (proves_sound _ F Iv pfs HF Hpr) | exact Hr].
  - (* Brk *) injection Hc as <-. split; [constructor | exact Hg].
  - (* Trap *) split; [constructor | exact I].
  - (* Call *)
    rewrite H in Hc.
    destruct (tr_all (tr_in (valid_args F args) reps) (pre d)) as [pre' |] eqn:Htr; [| discriminate].
    destruct (proves F pre' pfs && call_ok (valid_args F args) reps outs res) eqn:Hpc; [| discriminate].
    apply andb_true_iff in Hpc. destruct Hpc as [Hpr Hok].
    pose proof Hg as [HF [Hi Ho]].
    assert (Hge : good bm' e' (pre d)).
    { split; [| split; [exact H1 | exact H0]].
      apply (tr_all_sound _ (val lb base bm e) _ _ pre' Htr).
      - exact (tr_in_sound bm e F args reps bm' e' Hg H2 H3).
      - exact (proves_sound _ F pre' pfs HF Hpr). }
    destruct (IHHrun1 (None, post d) (pre d) (None, None) Hge (procs_ok_body f d H)) as [Ht1 Hp1]. simpl in Hp1.
    destruct (lrun_frame _ _ _ _ _ Hrun1 bm1 e1 eq_refl) as [Hw Hr].
    destruct Hp1 as [Hpost [Hi1 Ho1]].
    assert (Hgk : good (bouts bm bm1 outs) (upd e res (e1 (rslot d)))
                       (tr_some (tr_out (valid_args F args) reps outs d res) (post d) ++ kill (kill_outs F outs) (VS res))).
    { split; [| split].
      - apply lsat_app.
        + apply (tr_some_sound _ _ (val lb base bm1 e1)); [| exact Hpost].
          exact (tr_out_sound bm e F args reps outs res d bm' e' bm1 e1 Hg H2 H3 Hok Hw Hr).
        + apply lsat_upd. apply kill_sound. apply kill_outs_sound. exact HF.
      - apply i64_upd; [exact Hi | apply Hi1].
      - intro y. unfold bouts. destruct (find (fun q => Nat.eqb (fst q) y) outs) as [[_ r'] |]; [apply Ho1 | apply Ho]. }
    destruct (IHHrun2 C _ rr Hgk Hc) as [Ht2 Hp2]. split; [apply Forall_app; tauto | exact Hp2].
  - (* CallAbort *)
    rewrite H in Hc.
    destruct (tr_all (tr_in (valid_args F args) reps) (pre d)) as [pre' |] eqn:Htr; [| discriminate].
    destruct (proves F pre' pfs && call_ok (valid_args F args) reps outs res) eqn:Hpc; [| discriminate].
    apply andb_true_iff in Hpc. destruct Hpc as [Hpr Hok].
    pose proof Hg as [HF [Hi Ho]].
    assert (Hge : good bm' e' (pre d)).
    { split; [| split; [exact H1 | exact H0]].
      apply (tr_all_sound _ (val lb base bm e) _ _ pre' Htr).
      - exact (tr_in_sound bm e F args reps bm' e' Hg H2 H3).
      - exact (proves_sound _ F pre' pfs HF Hpr). }
    destruct (IHHrun (None, post d) (pre d) (None, None) Hge (procs_ok_body f d H)) as [Ht1 _].
    split; [exact Ht1 | exact I].
  - (* Ret *)
    destruct (proves F (snd C) pfs) eqn:Hpr; [| discriminate]. injection Hc as <-.
    split; [constructor |]. simpl. destruct Hg as [HF Hr]. split; [exact (proves_sound _ F _ pfs HF Hpr) | exact Hr].
Qed.

End Sound.
Print Assumptions lrun_sound.

(* ── The theorem: every run of an accepted program is SPATIAL ─────────── *)

Definition lshaped (lb : Lay) (h : Heap) (b : Blk) : Prop :=
  exists x, h b = Some x /\ size x = bytes_of (lb b) /\ repr x = R (lb b).

Lemma lstep_keeps_shape : forall lb ok h b ev,
  lpin lb ok ev -> lshaped lb h b -> lshaped lb (step h ev) b.
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

Lemma laccesses_spatial : forall lb ok h t,
  (forall b, ok b -> lshaped lb h b) -> Forall (lpin lb ok) t ->
  holds spatial_ev h t /\ holds type_ev h t.
Proof.
  intros lb ok h t Hh Hall. revert h Hh.
  induction Hall as [| ev t Hev _ IH]; intros h Hh; simpl; [tauto |].
  assert (Hh' : forall b, ok b -> lshaped lb (step h ev) b)
    by (intros b Hb; exact (lstep_keeps_shape lb ok h b ev Hev (Hh b Hb))).
  destruct (IH (step h ev) Hh') as [Hs Ht].
  destruct ev as [b n k | b o w k | b o w k | b | b]; simpl in Hev; try contradiction;
    destruct Hev as [Hb [Hr [Ho [Hw Hend]]]];
    destruct (Hh b Hb) as [x [Hx [Hsz Hrx]]];
    (split; split; [| exact Hs | | exact Ht]);
    exists x; (split; [exact Hx |]); unfold in_bounds; try lia; congruence.
Qed.

Definition lalloc_ev (lb : Lay) (b : Blk) : Ev := EAlloc b (bytes_of (lb b)) (R (lb b)).

Lemma lallocs_shaped : forall lb bs h b,
  (In b bs \/ lshaped lb h b) -> lshaped lb (fold_left step (map (lalloc_ev lb) bs) h) b.
Proof.
  intros lb bs. induction bs as [| b0 bs IH]; intros h b Hb; simpl.
  - destruct Hb as [[] | Hb]. exact Hb.
  - apply IH. destruct (in_dec Nat.eq_dec b bs) as [Hin | Hnin]; [left; exact Hin |]. right.
    unfold lalloc_ev, set. simpl. cbv beta. destruct (Nat.eqb b b0) eqn:E.
    + apply Nat.eqb_eq in E. subst b0.
      exists (mkBlock (bytes_of (lb b)) (R (lb b)) 1 (fun _ => false)). simpl. rewrite Nat.eqb_refl. split; [reflexivity | tauto].
    + destruct Hb as [[E' | Hin] | [x [Hx Hrest]]];
        [subst b0; rewrite Nat.eqb_refl in E; discriminate | contradiction |].
      exists x. simpl. rewrite E. tauto.
Qed.

Lemma lallocs_holds : forall Q lb bs h,
  (forall h b n k, Q h (EAlloc b n k)) -> holds Q h (map (lalloc_ev lb) bs).
Proof.
  intros Q lb bs. induction bs as [| b bs IH]; intros h HQ; simpl; [exact I |].
  split; [apply HQ | apply IH; exact HQ].
Qed.

(* `lin_accepts_spatial`: blocks allocated up front (each in the 32-bit
   address space), procedures each accepted under their contract, an entry
   accepted from no facts — every run's trace of loads and stores is
   SPATIAL and WELL-TYPED, through any depth of recursion and any number of
   loop iterations. *)
Theorem lin_accepts_spatial : forall P lb base bs bm e p t o,
  (forall b, 0 <= base b /\ 0 <= CAP (lb b) /\ base b + PAYLOAD + CAP (lb b) < ADDR) ->
  (forall x, In (bm x) bs) -> laccepts P p = true -> i64 e ->
  lrun P lb base (fun b => In b bs) bm e p t o ->
  Spatial (map (lalloc_ev lb) bs ++ t) /\ WellTyped (map (lalloc_ev lb) bs ++ t).
Proof.
  intros P lb base bs bm e p t o Hwf Hbs Hacc Hi Hrun. unfold laccepts in Hacc.
  apply andb_true_iff in Hacc. destruct Hacc as [HP Hc].
  destruct (lcheck P (None, []) [] p) as [r |] eqn:Hpc; [| discriminate].
  assert (Hg : good lb base (fun b => In b bs) bm e []) by (split; [intros l [] | split; assumption]).
  destruct (lrun_sound P lb base (fun b => In b bs) Hwf HP bm e p t o Hrun (None, []) [] r Hg Hpc) as [Hall _].
  assert (Hh : forall b, In b bs -> lshaped lb (fold_left step (map (lalloc_ev lb) bs) empty_heap) b).
  { intros b Hb. apply lallocs_shaped. left. exact Hb. }
  destruct (laccesses_spatial lb (fun b => In b bs) _ t Hh Hall) as [Hsp Hty].
  unfold Spatial, WellTyped. rewrite !holds_app.
  split; split; try assumption; apply lallocs_holds; intros; exact I.
Qed.
Print Assumptions lin_accepts_spatial.

(* ── Controls ─────────────────────────────────────────────────────────── *)

Local Open Scope nat_scope.

(* Slot 0 := the handle of region 7; the block in the address space; the
   len word read at h + 4. Facts after LWf: 0 `-VB7`, 1 `-VC7`,
   2 `VB7 + VC7 + 12 - 2^32 + 1`, 3 `VS0 - VB7`, 4 `VB7 - VS0`. *)
Definition hdr_read (off : Z) (p2 : Pf) : LProg :=
  LHandle 0 7 (LWf 7 (LAcc 7 4%Z off (v1 (VS 0)) ([(4, 1%Z); (0, 1%Z)], 1%Z) ([(4, 1%Z)], 1%Z) p2 LEnd)).

Example accepts_header_read : laccepts [] (hdr_read 4%Z ([(3, 1%Z); (1, 1%Z)], 1%Z)) = true.
Proof. reflexivity. Qed.

(* The same proof for a payload read at h + 12: nothing says the block holds
   4 payload bytes, and the combination falls short. *)
Example rejects_payload_read_uncounted : laccepts [] (hdr_read 12%Z ([(3, 1%Z); (1, 1%Z)], 1%Z)) = false.
Proof. reflexivity. Qed.

(* A negative multiplier is not a Farkas proof. *)
Example rejects_negative_multiplier : laccepts [] (hdr_read 4%Z ([(3, 1%Z); (1, (-1)%Z)], 1%Z)) = false.
Proof. reflexivity. Qed.

(* A count makes the payload read provable: slot 1 := the len at stride 4,
   then the code's test `c >= VS1`; its else arm knows VS1 >= c + 1, so the
   block holds 4·(c + 1) payload bytes. Else-arm facts: 0 `c + 1 - VS1`,
   1 `-VS1`, 2 `4·VS1 - VC7`, 3 `-VB7`, 4 `-VC7`, 5 wf, 6 `VS0 - VB7`,
   7 `VB7 - VS0`. The read of w bytes at h + 12 is the combination
   6 + 4·0 + 2: `VS0 - VB7 - VC7 + 4·(c + 1) <= 0`. *)
Definition kpf : Pf * Pf := (([], 1%Z), ([], 1%Z)).
Definition slot_read (c w : Z) : LProg :=
  LHandle 0 7 (LWf 7 (LLen 1 7 4%Z
    (LTest (kz c) (v1 (VS 1)) true kpf kpf kpf
      LEnd
      (LAcc 7 w 12%Z (v1 (VS 0)) ([(7, 1%Z); (3, 1%Z)], 1%Z) ([(7, 1%Z)], 1%Z) ([(6, 1%Z); (0, 4%Z); (2, 1%Z)], 1%Z) LEnd)
      LEnd))).

Example accepts_counted_slot_read : laccepts [] (slot_read 1%Z 8%Z) = true.
Proof. reflexivity. Qed.

(* One element fewer known (the test `0 >= VS1`): an 8-byte read past the
   4 bytes the count guarantees is refused. *)
Example rejects_read_past_count : laccepts [] (slot_read 0%Z 8%Z) = false.
Proof. reflexivity. Qed.

(* ... and the 4-byte read it does guarantee is accepted. *)
Example accepts_read_within_count : laccepts [] (slot_read 0%Z 4%Z) = true.
Proof. reflexivity. Qed.

(* A loop back edge outside any loop: refused. *)
Example rejects_cont_outside_loop : laccepts [] (LCont []) = false.
Proof. reflexivity. Qed.

(* A call of a procedure that does not exist: refused. *)
Example rejects_unknown_procedure : laccepts [] (LCall 0 [] [] [] 0 [] LEnd) = false.
Proof. reflexivity. Qed.

(* A procedure whose body can fall through instead of returning: refused. *)
Example rejects_body_falls_through : laccepts [(0, mkProc [] [] 0 LEnd)] LEnd = false.
Proof. reflexivity. Qed.

(* A procedure whose postcondition its return does not prove: refused. *)
Example rejects_unproven_post : laccepts [(0, mkProc [] [kz 1%Z] 0 (LRet [([], 1%Z)]))] LEnd = false.
Proof. reflexivity. Qed.
