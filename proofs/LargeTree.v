(* Almide v1 trust spine — #3348: the emitted `$lfree` / `$ltake` LOOP
   trees compute the LargeList.v model on the in-memory list.

   The two runtime functions (crates/almide-wasm/src/runtime_large.rs) walk
   an address-ordered singly-linked list of free extents. A node at base
   `n` keeps its SIZE at `n+4` and the next base at `n+12` (0 ends the
   list); its rc cell `n+0` reads 0. The list head lives at address 12 —
   the next-field of a SENTINEL node at address 0, whose size word `[4]`
   reads 0 (the null guard): a walk starts with `p = 0`, so linking after
   the predecessor is `[p+12] := x` with no head special case, and the
   sentinel can never be merged into (`0 + [4] = 0` is never a block base).

   This file transcribes the two bodies into a small structured language
   (`lstmt`: locals, loads/stores, the bump frontier, `if`/`else`, `while`,
   `return` — the structured shape the emitter writes, `block/loop/br_if`),
   gives it a fuel-indexed semantics, and proves:

     `lfree_realizes`  — from a memory holding list `l` (`lrep`), `$lfree b t`
                         ends with memory holding `ins l (b, t)`, and every
                         address it changed is the head cell 12 or a field of
                         a node of the RESULT list (so: inside a free extent);
     `ltake_realizes`  — `$ltake w len` returns exactly LargeList.take's
                         choice: a hit's base with the header rc=1/len/cap
                         written and the memory holding the rest of the list;
                         an extension (0, the tail node unlinked, the frontier
                         lowered to it); or a miss (0, nothing changed).

   Bounds: every value is a nonnegative address or size well below 2^31,
   so the i32 unsigned comparisons coincide with the Z ones (the
   StructuralRuntime.v convention); the theorems carry the needed facts. *)

From AlmideTrust Require Import RuntimeModel LargeList.
From Stdlib Require Import ZArith Lia List Bool.
Import ListNotations.
Open Scope Z_scope.

(* ══ THE LANGUAGE ═════════════════════════════════════════════════════ *)

Inductive lexpr : Type :=
  | LC (z : Z)
  | LL (i : nat)           (* local.get i *)
  | LG                     (* global.get $heap *)
  | LLoad (a : lexpr)      (* i32.load *)
  | LAdd (a b : lexpr)
  | LSub (a b : lexpr)
  | LLtU (a b : lexpr)
  | LGeU (a b : lexpr)
  | LEq (a b : lexpr)
  | LNe (a b : lexpr)
  | LEqz (a : lexpr)
  | LAnd (a b : lexpr).    (* i32.and of two 0/1 flags *)

Inductive lstmt : Type :=
  | LSet (i : nat) (e : lexpr)
  | LStore (a v : lexpr)
  | LSetG (e : lexpr)
  | LIf (c : lexpr) (th el : list lstmt)
  | LWhile (c : lexpr) (body : list lstmt)
  | LRet (e : lexpr).

Record LS := mkLS { loc : nat -> Z; mem : Mem; gh : Z }.

Definition setl (s : LS) (i : nat) (v : Z) : LS :=
  mkLS (fun j => if Nat.eqb j i then v else loc s j) (mem s) (gh s).

Fixpoint lev (e : lexpr) (s : LS) : Z :=
  match e with
  | LC z => z
  | LL i => loc s i
  | LG => gh s
  | LLoad a => mem s (lev a s)
  | LAdd a b => lev a s + lev b s
  | LSub a b => lev a s - lev b s
  | LLtU a b => if lev a s <? lev b s then 1 else 0
  | LGeU a b => if lev a s >=? lev b s then 1 else 0
  | LEq a b => if lev a s =? lev b s then 1 else 0
  | LNe a b => if lev a s =? lev b s then 0 else 1
  | LEqz a => if lev a s =? 0 then 1 else 0
  | LAnd a b => Z.land (lev a s) (lev b s)
  end.

Inductive lres := RNorm (s : LS) | RRet (v : Z) (s : LS) | RFuel.

(* Each statement costs one unit of fuel; a loop iteration re-enters the
   statement list. Real runs terminate (the walks follow a finite list);
   the theorems below exhibit enough fuel. *)
Fixpoint lexec (f : nat) (ss : list lstmt) (s : LS) : lres :=
  match f with
  | O => RFuel
  | S f' =>
      match ss with
      | [] => RNorm s
      | st :: rest =>
          match st with
          | LSet i e => lexec f' rest (setl s i (lev e s))
          | LStore a v => lexec f' rest (mkLS (loc s) (upd (mem s) (lev a s) (lev v s)) (gh s))
          | LSetG e => lexec f' rest (mkLS (loc s) (mem s) (lev e s))
          | LIf c th el =>
              match lexec f' (if lev c s =? 0 then el else th) s with
              | RNorm s' => lexec f' rest s'
              | r => r
              end
          | LWhile c body =>
              if lev c s =? 0 then lexec f' rest s
              else match lexec f' body s with
                   | RNorm s' => lexec f' (LWhile c body :: rest) s'
                   | r => r
                   end
          | LRet e => RRet (lev e s) s
          end
      end
  end.

(* One unfolding step, stated so proofs can rewrite without `cbn`
   unfolding the recursive call underneath. *)
Lemma lexec_cons : forall f st rest s,
  lexec (S f) (st :: rest) s =
  match st with
  | LSet i e => lexec f rest (setl s i (lev e s))
  | LStore a v => lexec f rest (mkLS (loc s) (upd (mem s) (lev a s) (lev v s)) (gh s))
  | LSetG e => lexec f rest (mkLS (loc s) (mem s) (lev e s))
  | LIf c th el =>
      match lexec f (if lev c s =? 0 then el else th) s with
      | RNorm s' => lexec f rest s'
      | r => r
      end
  | LWhile c body =>
      if lev c s =? 0 then lexec f rest s
      else match lexec f body s with
           | RNorm s' => lexec f (LWhile c body :: rest) s'
           | r => r
           end
  | LRet e => RRet (lev e s) s
  end.
Proof. reflexivity. Qed.

Lemma lexec_nil : forall f s, lexec (S f) [] s = RNorm s.
Proof. reflexivity. Qed.

Ltac lstep H := rewrite lexec_cons in H; cbv beta iota in H.
Ltac lstepg := rewrite lexec_cons; cbv beta iota.

(* ── fuel monotonicity: a run that finishes keeps its outcome with more fuel ── *)
Lemma lexec_mono : forall f ss s r,
  lexec f ss s = r -> r <> RFuel -> lexec (S f) ss s = r.
Proof.
  induction f as [ | f IH ]; intros ss s r H Hr; [ cbn in H; subst r; contradiction | ].
  destruct ss as [ | st rest ]; [ rewrite lexec_nil in H |- *; exact H | ].
  rewrite lexec_cons in H |- *.
  destruct st as [ i e | a v | e | c th el | c body | e ]; cbv beta iota in H |- *.
  - apply IH; assumption.
  - apply IH; assumption.
  - apply IH; assumption.
  - destruct (lexec f (if lev c s =? 0 then el else th) s) as [ s' | v s' | ] eqn:E.
    + rewrite (IH _ _ _ E ltac:(discriminate)). apply IH; assumption.
    + rewrite (IH _ _ _ E ltac:(discriminate)). exact H.
    + subst r. contradiction.
  - destruct (lev c s =? 0).
    + apply IH; assumption.
    + destruct (lexec f body s) as [ s' | v s' | ] eqn:E.
      * rewrite (IH _ _ _ E ltac:(discriminate)). apply IH; assumption.
      * rewrite (IH _ _ _ E ltac:(discriminate)). exact H.
      * subst r. contradiction.
  - exact H.
Qed.

Lemma lexec_mono_le : forall f f' ss s r,
  (f <= f')%nat -> lexec f ss s = r -> r <> RFuel -> lexec f' ss s = r.
Proof.
  intros f f' ss s r Hle H Hr. induction Hle; [ exact H | ].
  apply lexec_mono; assumption.
Qed.

(* Sequencing: a prefix that finishes normally hands its state on. *)
Lemma lexec_app : forall f1 f2 ss1 ss2 s s1 r,
  lexec f1 ss1 s = RNorm s1 -> lexec f2 ss2 s1 = r -> r <> RFuel ->
  exists f, lexec f (ss1 ++ ss2) s = r.
Proof.
  intros f1. induction f1 as [ | f1 IH ]; intros f2 ss1 ss2 s s1 r H1 H2 Hr; [ discriminate | ].
  destruct ss1 as [ | st rest ].
  - rewrite lexec_nil in H1. injection H1 as <-. exists f2. exact H2.
  - rewrite lexec_cons in H1.
    destruct st as [ i e | a v | e | c th el | c body | e ]; cbv beta iota in H1.
    + destruct (IH f2 rest ss2 _ s1 r H1 H2 Hr) as [f Hf].
      exists (S f). rewrite <- app_comm_cons, lexec_cons. exact Hf.
    + destruct (IH f2 rest ss2 _ s1 r H1 H2 Hr) as [f Hf].
      exists (S f). rewrite <- app_comm_cons, lexec_cons. exact Hf.
    + destruct (IH f2 rest ss2 _ s1 r H1 H2 Hr) as [f Hf].
      exists (S f). rewrite <- app_comm_cons, lexec_cons. exact Hf.
    + destruct (lexec f1 (if lev c s =? 0 then el else th) s) as [ s' | v s' | ] eqn:E;
        try discriminate.
      destruct (IH f2 rest ss2 _ s1 r H1 H2 Hr) as [f Hf].
      exists (S (max f f1)). rewrite <- app_comm_cons, lexec_cons. cbv beta iota.
      rewrite (lexec_mono_le f1 (max f f1) _ _ _ ltac:(lia) E ltac:(discriminate)).
      exact (lexec_mono_le f (max f f1) _ _ _ ltac:(lia) Hf Hr).
    + destruct (lev c s =? 0) eqn:Ec.
      * destruct (IH f2 rest ss2 _ s1 r H1 H2 Hr) as [f Hf].
        exists (S f). rewrite <- app_comm_cons, lexec_cons. cbv beta iota. rewrite Ec. exact Hf.
      * destruct (lexec f1 body s) as [ s' | v s' | ] eqn:E; try discriminate.
        destruct (IH f2 (LWhile c body :: rest) ss2 _ s1 r H1 H2 Hr) as [f Hf].
        exists (S (max f f1)). rewrite <- app_comm_cons, lexec_cons. cbv beta iota. rewrite Ec.
        rewrite (lexec_mono_le f1 (max f f1) _ _ _ ltac:(lia) E ltac:(discriminate)).
        exact (lexec_mono_le f (max f f1) _ _ _ ltac:(lia) Hf Hr).
    + discriminate.
Qed.

(* ══ THE IN-MEMORY LIST ═══════════════════════════════════════════════ *)

(* Head cell: the sentinel's next field. *)
Definition LHEAD : Z := 12.

Fixpoint lrep (m : Mem) (x : Z) (l : list ext) : Prop :=
  match l with
  | [] => x = 0
  | (b, s) :: r => x = b /\ m (b + 4) = s /\ lrep m (m (b + 12)) r
  end.

(* The fields a list's representation reads. *)
Definition field (l : list ext) (a : Z) : Prop :=
  exists b s, In (b, s) l /\ (a = b + 4 \/ a = b + 12).

Lemma lrep_frame : forall l m m' x,
  (forall a, field l a -> m' a = m a) -> lrep m x l -> lrep m' x l.
Proof.
  induction l as [ | [b s] r IH ]; intros m m' x Hf H; cbn in *; [ exact H | ].
  destruct H as [Hx [Hs Hr]]. split; [ exact Hx | split ].
  - rewrite Hf; [ exact Hs | exists b, s; split; [ left; reflexivity | left; reflexivity ] ].
  - rewrite Hf by (exists b, s; split; [ left; reflexivity | right; reflexivity ]).
    apply (IH m); [ | exact Hr ].
    intros a [b' [s' [Hin Ha]]]. apply Hf. exists b', s'. split; [ right; exact Hin | exact Ha ].
Qed.

(* Segments: the representation of a prefix ending in a link to `y`. *)
Fixpoint lseg (m : Mem) (x : Z) (l : list ext) (y : Z) : Prop :=
  match l with
  | [] => x = y
  | (b, s) :: r => x = b /\ m (b + 4) = s /\ lseg m (m (b + 12)) r y
  end.

Lemma lrep_app : forall m pre post x,
  lrep m x (pre ++ post) <-> exists y, lseg m x pre y /\ lrep m y post.
Proof.
  induction pre as [ | [b s] r IH ]; intros post x; cbn.
  - split; [ intros H; exists x; split; [ reflexivity | exact H ] | ].
    intros [y [-> H]]. exact H.
  - rewrite IH. split.
    + intros [Hx [Hs [y [Hy Hp]]]]. exists y. split; [ split; [ exact Hx | split; assumption ] | exact Hp ].
    + intros [y [[Hx [Hs Hy]] Hp]]. split; [ exact Hx | split; [ exact Hs | exists y; split; assumption ] ].
Qed.

(* ══ EXTENT FACTS USED BY THE FRAMES ══════════════════════════════════ *)

(* In a gapped list, two different nodes have disjoint extents. *)
Lemma gaps_In_disj : forall l b1 s1 b2 s2,
  gaps l -> In (b1, s1) l -> In (b2, s2) l -> (b1, s1) <> (b2, s2) ->
  b1 + s1 <= b2 \/ b2 + s2 <= b1.
Proof.
  induction l as [ | e r IH ]; intros b1 s1 b2 s2 Hg H1 H2 Hne; [ inversion H1 | ].
  pose proof (gaps_head_le e r Hg) as Hf. rewrite Forall_forall in Hf.
  destruct H1 as [H1 | H1]; destruct H2 as [H2 | H2].
  - subst. contradiction.
  - subst e. specialize (Hf _ H2). cbn in Hf. lia.
  - subst e. specialize (Hf _ H1). cbn in Hf. lia.
  - exact (IH b1 s1 b2 s2 (gaps_tail _ _ Hg) H1 H2 Hne).
Qed.

Lemma gaps_In_size : forall l b s, gaps l -> In (b, s) l -> 16 <= s.
Proof.
  induction l as [ | e r IH ]; intros b s Hg H; [ inversion H | ].
  destruct H as [H | H]; [ subst e; destruct Hg as [Hs _]; unfold MINSZ in Hs; exact Hs | ].
  exact (IH b s (gaps_tail _ _ Hg) H).
Qed.

(* A field of a gapped list lies inside its node's extent, so an address
   outside every extent is never a field. *)
Lemma field_cov : forall l a, gaps l -> field l a -> cov l a.
Proof.
  intros l a Hg [b [s [Hin Ha]]].
  pose proof (gaps_In_size l b s Hg Hin) as Hs.
  apply Exists_exists. exists (b, s). split; [ exact Hin | unfold inside; cbn; lia ].
Qed.

Lemma field_cov' : forall l a,
  Forall (fun e => 16 <= snd e) l -> field l a -> cov l a.
Proof.
  intros l a Hs [b [s [Hin Ha]]]. rewrite Forall_forall in Hs.
  specialize (Hs _ Hin). cbn in Hs.
  apply Exists_exists. exists (b, s). split; [ exact Hin | unfold inside; cbn; lia ].
Qed.

Lemma lseg_frame : forall l m m' x y,
  (forall a, field l a -> m' a = m a) -> lseg m x l y -> lseg m' x l y.
Proof.
  induction l as [ | [b s] r IH ]; intros m m' x y Hf H; cbn in *; [ exact H | ].
  destruct H as [Hx [Hs Hr]]. split; [ exact Hx | split ].
  - rewrite Hf; [ exact Hs | exists b, s; split; [ left; reflexivity | left; reflexivity ] ].
  - rewrite Hf by (exists b, s; split; [ left; reflexivity | right; reflexivity ]).
    apply (IH m); [ | exact Hr ].
    intros a [b' [s' [Hin Ha]]]. apply Hf. exists b', s'. split; [ right; exact Hin | exact Ha ].
Qed.

Lemma field_cons : forall e l a, field (e :: l) a <-> (a = fst e + 4 \/ a = fst e + 12) \/ field l a.
Proof.
  intros [b s] l a. split.
  - intros [b' [s' [[H | H] Ha]]].
    + injection H as <- <-. left. exact Ha.
    + right. exists b', s'. split; assumption.
  - intros [Ha | [b' [s' [Hin Ha]]]].
    + exists b, s. split; [ left; reflexivity | exact Ha ].
    + exists b', s'. split; [ right; exact Hin | exact Ha ].
Qed.

Lemma field_app : forall l1 l2 a, field (l1 ++ l2) a <-> field l1 a \/ field l2 a.
Proof.
  intros l1 l2 a. unfold field. split.
  - intros [b [s [Hin Ha]]]. apply in_app_or in Hin as [H | H]; [ left | right ]; exists b, s; auto.
  - intros [[b [s [Hin Ha]]] | [b [s [Hin Ha]]]]; exists b, s; split; auto using in_or_app.
Qed.

(* The last base of a nonempty segment, and the link it holds. *)
Fixpoint lastb (l : list ext) (d : Z) : Z :=
  match l with [] => d | (b, _) :: r => lastb r b end.

Lemma lseg_last : forall l m x y d,
  lseg m x l y -> l <> [] -> field l (lastb l d + 12) /\ m (lastb l d + 12) = y.
Proof.
  induction l as [ | [b s] r IH ]; intros m x y d H Hne; [ contradiction | ].
  destruct H as [Hx [Hs Hr]]. destruct r as [ | e r' ].
  - cbn in Hr |- *. split; [ exists b, s; split; [ left; reflexivity | right; reflexivity ] | exact Hr ].
  - destruct (IH m _ y b Hr ltac:(discriminate)) as [Hf Hm].
    cbn [lastb]. split; [ apply field_cons; right; exact Hf | exact Hm ].
Qed.

Lemma lseg_app_lrep : forall m x pre y l,
  lseg m x pre y -> lrep m y l -> lrep m x (pre ++ l).
Proof. intros m x pre y l H1 H2. apply lrep_app. exists y. split; assumption. Qed.

Definition hdb (l : list ext) : Z := match l with [] => 0 | e :: _ => fst e end.

Lemma lrep_hd : forall m x l, lrep m x l -> x = hdb l.
Proof. intros m x [ | [b s] r ] H; cbn in *; [ exact H | apply H ]. Qed.

(* ══ $lfree ═══════════════════════════════════════════════════════════ *)

(* Locals inside `$free`: 0 = b (the block), 1 = t (its total), 3 = pp,
   4 = p, 5 = q (fresh, zero on entry; 2 is $free's class scratch). *)
Definition at_ (e : lexpr) (off : Z) : lexpr := LLoad (LAdd e (LC off)).

Definition lfree_walk : lstmt :=
  LWhile (LAnd (LNe (LL 5) (LC 0)) (LLtU (LL 5) (LL 0)))
    [ LSet 3 (LL 4); LSet 4 (LL 5); LSet 5 (at_ (LL 5) 12) ].

Definition lfree_fin : list lstmt :=
  [ LIf (LEq (LAdd (LL 0) (LL 1)) (LL 5))
        [ LSet 1 (LAdd (LL 1) (at_ (LL 5) 4)); LSet 5 (at_ (LL 5) 12) ] [];
    LIf (LEq (LAdd (LL 4) (at_ (LL 4) 4)) (LL 0))
        [ LSet 1 (LAdd (LL 1) (at_ (LL 4) 4)); LSet 0 (LL 4); LSet 4 (LL 3) ] [];
    LStore (LAdd (LL 0) (LC 0)) (LC 0);
    LStore (LAdd (LL 0) (LC 4)) (LL 1);
    LStore (LAdd (LL 0) (LC 12)) (LL 5);
    LStore (LAdd (LL 4) (LC 12)) (LL 0) ].

Definition lfree_tree : list lstmt :=
  LSet 5 (at_ (LC 0) LHEAD) :: lfree_walk :: lfree_fin.

Ltac lsimpl := cbn [lev at_ loc mem gh setl Nat.eqb] in *.

Ltac cnd := change (1 =? 0) with false in *; change (0 =? 0) with true in *; cbv beta iota.
Ltac run := repeat progress (first [ rewrite lexec_nil | rewrite lexec_cons ]; cbv beta iota; lsimpl).
Ltac eqbs := repeat match goal with |- context [?a =? ?b] => destruct (Z.eqb_spec a b) end.
Ltac eqbs_in H := repeat match type of H with context [?a =? ?b] => destruct (Z.eqb_spec a b) end.

Lemma upd_same : forall m a v x, m a = v -> upd m a v x = m x.
Proof. intros m a v x H. unfold upd. destruct (Z.eqb_spec x a); congruence. Qed.

(* The straight-line tail after the walk: x = (px, sx) is the
   predecessor (the sentinel (0,0) when pre = []), r the successors. *)
Lemma lfree_finish : forall s pre px sx r b t pp,
  loc s 0%nat = b -> loc s 1%nat = t -> loc s 3%nat = pp -> loc s 4%nat = px -> loc s 5%nat = hdb r ->
  lseg (mem s) 0 pre px -> mem s (px + 4) = sx -> mem s (px + 12) = hdb r ->
  lrep (mem s) (hdb r) r ->
  (forall a, field pre a -> a < px) ->
  (forall a, field r a -> b + t <= a) ->
  px + sx <= b ->
  ((pre = [] /\ sx = 0) \/ (pre <> [] /\ 16 <= px /\ 16 <= sx)) ->
  (pre <> [] -> field pre (pp + 12) /\ mem s (pp + 12) = px) ->
  16 <= b -> 16 <= t ->
  exists s', lexec 20 lfree_fin s = RNorm s' /\ gh s' = gh s /\
    lrep (mem s') 0 (pre ++ link_pred (px, sx) (merge_succ (b, t) r)) /\
    (forall a, mem s' a <> mem s a -> (b <= a < b + t) \/ (px <= a < px + sx) \/ a = px + 12).
Proof.
  intros s pre px sx r b t pp Hb Ht Hpp Hp Hq Hseg Hsx Hnx Hr Hfp Hfr Hxb Hcase Hlast Hb16 Ht16.
  assert (Hpx0 : 0 <= px).
  { destruct Hcase as [[-> _] | [_ [H _]]]; [ cbn in Hseg; lia | lia ]. }
  unfold lfree_fin. lstepg. lsimpl. rewrite Hb, Ht, Hq.
  destruct (Z.eqb_spec (b + t) (hdb r)) as [Esm | Esm]; cnd; run;
    rewrite ?Hb, ?Ht, ?Hp, ?Hq, ?Hpp in *.
  all: rewrite ?Hsx.
  all: destruct (Z.eqb_spec (px + sx) b) as [Epm | Epm]; cnd; run;
    rewrite ?Hb, ?Ht, ?Hp, ?Hq, ?Hpp in *.
  all: eexists; split; [ reflexivity | ]; cbn [mem gh]; split; [ reflexivity | ].
  (* the facts every case reads *)
  all: assert (Hx : (pre = [] /\ px = 0 /\ sx = 0) \/ (pre <> [] /\ 16 <= px /\ 16 <= sx))
         by (destruct Hcase as [[-> ->] | H]; [ left; cbn in Hseg; split; [ reflexivity | lia ] | right; exact H ]).
  all: clear Hcase.
  all: assert (Hpre : forall m', (forall a, a < px -> m' a = mem s a) -> lseg m' 0 pre px)
         by (intros m' Hm; apply (lseg_frame pre (mem s)); [ intros a Ha; apply Hm, Hfp, Ha | exact Hseg ]).
  all: assert (Hrest : forall m' l, (l = r \/ exists y, r = y :: l) ->
                 (forall a, b + t <= a -> m' a = mem s a) -> lrep m' (hdb l) l)
         by (intros m' l Hl Hm; destruct Hl as [-> | [y ->]];
             [ apply (lrep_frame r (mem s)); [ intros a Ha; apply Hm, Hfr, Ha | exact Hr ]
             | destruct y as [qy sy]; cbn in Hr; destruct Hr as [_ [_ Hr']];
               rewrite (lrep_hd _ _ _ Hr') in Hr';
               apply (lrep_frame l (mem s)); [ intros a Ha; apply Hm, Hfr, field_cons; right; exact Ha
                                             | exact Hr' ] ]).
  - (* both merges: r = y :: r'' with b + t = fst y, px + sx = b *)
    destruct r as [ | [qy sy] r'' ]; cbn [hdb fst snd] in *; [ lia | ].
    destruct Hx as [[_ [-> ->]] | [Hne [Hp16 Hs16]]]; [ lia | ].
    destruct (Hlast Hne) as [Hfpp Hmpp]. pose proof (Hfp _ Hfpp).
    pose proof Hr as Hr0. cbn in Hr0. destruct Hr0 as [_ [Hsy Hr'']].
    rewrite (lrep_hd _ _ _ Hr'') in *.
    rewrite ?Hsy, ?Hsx in *.
    cbn [merge_succ fst snd]. rewrite Esm, Z.eqb_refl. cbn [link_pred fst snd]. rewrite Epm, Z.eqb_refl.
    split.
    + apply (lseg_app_lrep _ _ _ px).
      * apply Hpre. intros a Ha. unfold upd. eqbs; try lia. subst a. symmetry. exact Hmpp.
      * cbn. split; [ reflexivity | ]. unfold upd. eqbs; try lia. split; [ lia | ].
        apply Hrest; [ right; eexists; reflexivity | ]. intros a Ha. unfold upd. eqbs; lia.
    + intros a Ha. revert Ha. unfold upd. eqbs; intros Ha; try lia. subst a. congruence.
  - (* successor merge only *)
    destruct r as [ | [qy sy] r'' ]; cbn [hdb fst snd] in *; [ lia | ].
    pose proof Hr as Hr0. cbn in Hr0. destruct Hr0 as [_ [Hsy Hr'']].
    rewrite (lrep_hd _ _ _ Hr'') in *.
    rewrite ?Hsy in *.
    cbn [merge_succ fst snd]. rewrite Esm, Z.eqb_refl. cbn [link_pred fst snd].
    replace (px + sx =? b) with false by (symmetry; apply Z.eqb_neq; exact Epm).
    assert (Hxb' : px + 12 < b) by (destruct Hx as [[_ [-> ->]] | [_ [? ?]]]; lia).
    split.
    + apply (lseg_app_lrep _ _ _ px).
      * apply Hpre. intros a Ha. unfold upd. eqbs; lia.
      * cbn. split; [ reflexivity | ]. unfold upd. eqbs; try lia. split; [ exact Hsx | ].
        split; [ reflexivity | ]. split; [ reflexivity | ].
        apply Hrest; [ right; eexists; reflexivity | ]. intros a Ha. unfold upd. eqbs; lia.
    + intros a Ha. revert Ha. unfold upd. eqbs; intros Ha; lia.
  - (* predecessor merge only *)
    destruct Hx as [[_ [-> ->]] | [Hne [Hp16 Hs16]]]; [ lia | ].
    destruct (Hlast Hne) as [Hfpp Hmpp]. pose proof (Hfp _ Hfpp).
    rewrite ?Hsx in *.
    assert (Hms : merge_succ (b, t) r = (b, t) :: r).
    { destruct r as [ | [qy sy] r'' ]; [ reflexivity | ]. cbn [merge_succ fst snd hdb] in *.
      replace (b + t =? qy) with false by (symmetry; apply Z.eqb_neq; exact Esm). reflexivity. }
    rewrite Hms. cbn [link_pred fst snd]. rewrite Epm, Z.eqb_refl.
    split.
    + apply (lseg_app_lrep _ _ _ px).
      * apply Hpre. intros a Ha. unfold upd. eqbs; try lia. subst a. symmetry. exact Hmpp.
      * cbn. split; [ reflexivity | ]. unfold upd. eqbs; try lia. split; [ lia | ].
        apply Hrest; [ left; reflexivity | ]. intros a Ha. unfold upd. eqbs; lia.
    + intros a Ha. revert Ha. unfold upd. eqbs; intros Ha; try lia. subst a. congruence.
  - (* no merge *)
    assert (Hms : merge_succ (b, t) r = (b, t) :: r).
    { destruct r as [ | [qy sy] r'' ]; [ reflexivity | ]. cbn [merge_succ fst snd hdb] in *.
      replace (b + t =? qy) with false by (symmetry; apply Z.eqb_neq; exact Esm). reflexivity. }
    rewrite Hms. cbn [link_pred fst snd].
    replace (px + sx =? b) with false by (symmetry; apply Z.eqb_neq; exact Epm).
    assert (Hxb' : px + 12 < b) by (destruct Hx as [[_ [-> ->]] | [_ [? ?]]]; lia).
    split.
    + apply (lseg_app_lrep _ _ _ px).
      * apply Hpre. intros a Ha. unfold upd. eqbs; lia.
      * cbn. split; [ reflexivity | ]. unfold upd. eqbs; try lia. split; [ exact Hsx | ].
        split; [ reflexivity | ]. split; [ reflexivity | ].
        apply Hrest; [ left; reflexivity | ]. intros a Ha. unfold upd. eqbs; lia.
    + intros a Ha. revert Ha. unfold upd. eqbs; intros Ha; lia.
Qed.

(* ins from a known predecessor x (base below e): the walk's view. *)
Fixpoint insp (x : ext) (r : list ext) (e : ext) : list ext :=
  match r with
  | y :: r' => if fst y <? fst e then x :: insp y r' e else link_pred x (merge_succ e r)
  | [] => link_pred x (merge_succ e [])
  end.

Lemma ins_insp : forall r x e, fst x < fst e -> ins (x :: r) e = insp x r e.
Proof.
  induction r as [ | y r IH ]; intros x e H; cbn [ins insp].
  - replace (fst x <? fst e) with true by (symmetry; apply Z.ltb_lt; exact H). reflexivity.
  - replace (fst x <? fst e) with true by (symmetry; apply Z.ltb_lt; exact H).
    destruct (Z.ltb_spec (fst y) (fst e)) as [Hy | Hy]; [ | reflexivity ].
    f_equal. rewrite <- (IH y e Hy). cbn [ins].
    replace (fst y <? fst e) with true by (symmetry; apply Z.ltb_lt; exact Hy). reflexivity.
Qed.

(* The sentinel (0,0) in front: insp from it is the sentinel before ins. *)
Lemma insp_sentinel : forall L e, 0 < fst e -> insp (0, 0) L e = (0, 0) :: ins L e.
Proof.
  intros L [b t] Hb. cbn [fst] in Hb.
  assert (Hl : forall l, link_pred (0, 0) (merge_succ (b, t) l) = (0, 0) :: merge_succ (b, t) l).
  { assert (E : (0 + 0 =? b) = false) by (apply Z.eqb_neq; lia).
    intros [ | [q s] l ]; unfold merge_succ, link_pred; cbn [fst snd];
      [ | destruct (b + t =? q) ]; cbn [fst snd]; rewrite E; reflexivity. }
  destruct L as [ | y r ]; cbn [insp fst]; [ apply Hl | ].
  destruct (Z.ltb_spec (fst y) b) as [Hy | Hy]; cbv beta iota.
  - f_equal. symmetry. apply ins_insp. exact Hy.
  - rewrite Hl. f_equal. cbn [ins fst].
    replace (fst y <? b) with false by (symmetry; apply Z.ltb_ge; exact Hy). reflexivity.
Qed.

Lemma lseg_snoc : forall pre m x px sx y,
  lseg m x pre px -> m (px + 4) = sx -> m (px + 12) = y -> lseg m x (pre ++ [(px, sx)]) y.
Proof.
  induction pre as [ | [b s] r IH ]; intros m x px sx y H Hs Hn; cbn in *.
  - subst x. repeat split; assumption.
  - destruct H as [Hx [Hb Hr]]. repeat split; try assumption. apply IH; assumption.
Qed.

Lemma land11 : Z.land 1 1 = 1. Proof. reflexivity. Qed.
Lemma land0x : forall x, Z.land 0 x = 0. Proof. reflexivity. Qed.
Lemma land10 : Z.land 1 0 = 0. Proof. reflexivity. Qed.

Lemma lfree_loop : forall L b t r pre px sx s,
  loc s 0%nat = b -> loc s 1%nat = t -> loc s 4%nat = px -> loc s 5%nat = hdb r ->
  lseg (mem s) 0 pre px -> mem s (px + 4) = sx -> mem s (px + 12) = hdb r ->
  lrep (mem s) (hdb r) r ->
  (pre <> [] -> field pre (loc s 3%nat + 12) /\ mem s (loc s 3%nat + 12) = px) ->
  (forall a, field pre a -> a < px) ->
  ((pre = [] /\ px = 0 /\ sx = 0) \/ (pre <> [] /\ 16 <= px /\ 16 <= sx /\ In (px, sx) L)) ->
  px + sx <= b ->
  gaps r -> Forall (fun y => px + sx < fst y) r -> incl r L ->
  Forall (fun y => 16 <= fst y) L -> (forall a, inside (b, t) a -> ~ cov L a) ->
  16 <= b -> 16 <= t ->
  exists f s', (f <= 5 * length r + 21)%nat /\ lexec f (lfree_walk :: lfree_fin) s = RNorm s' /\ gh s' = gh s /\
    lrep (mem s') 0 (pre ++ insp (px, sx) r (b, t)) /\
    (forall a, mem s' a <> mem s a -> inside (b, t) a \/ a = 12 \/ cov L a).
Proof.
  intros L b t r. induction r as [ | [qy sy] r' IH ];
    intros pre px sx s Hb Ht Hp Hq Hseg Hsx Hnx Hr Hlast Hfp Hx Hxb Hg Hord Hinc HL16 Hdis Hb16 Ht16.
  - (* exit: no successor *)
    destruct (lfree_finish s pre px sx [] b t (loc s 3%nat) Hb Ht eq_refl Hp Hq Hseg Hsx Hnx Hr Hfp
                (fun a Ha => ltac:(destruct Ha as [? [? [[] _]]])) Hxb
                ltac:(destruct Hx as [[? [? ?]] | [? [? [? ?]]]]; [ left | right ]; auto)
                Hlast Hb16 Ht16) as [s' [Hrun [Hgh [Hrep Hfr]]]].
    exists 21%nat, s'. split; [ cbn [length]; lia | ]. split; [ | split; [ exact Hgh | split; [ exact Hrep | ] ] ].
    + rewrite lexec_cons. cbv beta iota. unfold lfree_walk. lsimpl. rewrite Hq, Hb. cbn [hdb].
      change ((if 0 =? 0 then 0 else 1)) with 0. rewrite land0x. change (0 =? 0) with true.
      cbv beta iota. exact Hrun.
    + intros a Ha. destruct (Hfr a Ha) as [H | [H | H]]; [ left; exact H | | ].
      * destruct Hx as [[_ [-> ->]] | [_ [_ [Hs16 Hin]]]]; [ lia | ].
        right; right. apply Exists_exists. exists (px, sx). split; [ exact Hin | unfold inside; cbn; lia ].
      * destruct Hx as [[_ [-> ->]] | [_ [_ [Hs16 Hin]]]]; [ right; left; lia | ].
        right; right. apply Exists_exists. exists (px, sx). split; [ exact Hin | unfold inside; cbn; lia ].
  - cbn [hdb fst] in Hq, Hnx.
    pose proof Hr as Hr0. cbn in Hr0. destruct Hr0 as [Hqy [Hsy Hr'']].
    pose proof (lrep_hd _ _ _ Hr'') as Hnq. rewrite Hnq in Hr''.
    assert (HinL : In (qy, sy) L) by (apply Hinc; left; reflexivity).
    assert (Hq16 : 16 <= qy) by (rewrite Forall_forall in HL16; exact (HL16 _ HinL)).
    assert (Hsy16 : 16 <= sy) by (destruct Hg as [H _]; unfold MINSZ in H; exact H).
    destruct (Z.ltb_spec qy b) as [Hlt | Hge].
    + (* step: y is below b, it becomes the predecessor *)
      assert (Hyb : qy + sy <= b).
      { destruct (Z_le_gt_dec (qy + sy) b) as [ | Hc ]; [ assumption | exfalso ].
        apply (Hdis b); [ unfold inside; cbn; lia | ].
        apply Exists_exists. exists (qy, sy). split; [ exact HinL | unfold inside; cbn; lia ]. }
      set (s1 := setl (setl (setl s 3 (loc s 4)) 4 (loc s 5)) 5 (mem s (loc s 5 + 12))).
      assert (Hord1 : Forall (fun y => qy + sy < fst y) r') by exact (gaps_head_le _ _ Hg).
      assert (Hpx_lt : px < qy) by (rewrite Forall_forall in Hord; specialize (Hord _ (or_introl eq_refl)); cbn in Hord;
                                    destruct Hx as [[_ [-> ->]] | [_ [_ [? _]]]]; lia).
      assert (E0 : loc s1 0%nat = b) by (unfold s1; lsimpl; exact Hb).
      assert (E1 : loc s1 1%nat = t) by (unfold s1; lsimpl; exact Ht).
      assert (E3 : loc s1 3%nat = px) by (unfold s1; lsimpl; exact Hp).
      assert (E4 : loc s1 4%nat = qy) by (unfold s1; lsimpl; rewrite Hq; reflexivity).
      assert (E5 : loc s1 5%nat = hdb r') by (unfold s1; lsimpl; rewrite Hq; exact Hnq).
      assert (Hseg1 : lseg (mem s1) 0 (pre ++ [(px, sx)]) qy) by (apply lseg_snoc; assumption).
      assert (Hnx1 : mem s1 (qy + 12) = hdb r') by exact Hnq.
      assert (Hlast1 : pre ++ [(px, sx)] <> [] ->
                field (pre ++ [(px, sx)]) (loc s1 3%nat + 12) /\ mem s1 (loc s1 3%nat + 12) = qy).
      { intros _. rewrite E3. split; [ apply field_app; right; exists px, sx; split; [ left; reflexivity | right; reflexivity ] | exact Hnx ]. }
      assert (Hfp1 : forall a, field (pre ++ [(px, sx)]) a -> a < qy).
      { intros a Ha. apply field_app in Ha as [Ha | Ha]; [ pose proof (Hfp a Ha); lia | ].
        destruct Ha as [b' [s'' [[H | []] Ha]]]. injection H as <- <-.
        destruct Hx as [[_ [-> ->]] | [_ [_ [? _]]]]; [ lia | ].
        rewrite Forall_forall in Hord. specialize (Hord _ (or_introl eq_refl)). cbn in Hord. lia. }
      assert (Hx1 : (pre ++ [(px, sx)] = [] /\ qy = 0 /\ sy = 0) \/
                    (pre ++ [(px, sx)] <> [] /\ 16 <= qy /\ 16 <= sy /\ In (qy, sy) L)).
      { right. split; [ destruct pre; discriminate | auto ]. }
      destruct (IH (pre ++ [(px, sx)]) qy sy s1 E0 E1 E4 E5 Hseg1 Hsy Hnx1 Hr'' Hlast1 Hfp1 Hx1 Hyb
                  (gaps_tail _ _ Hg) Hord1 (fun z Hz => Hinc z (or_intror Hz)) HL16 Hdis Hb16 Ht16)
        as [f1 [s' [Hf1 [Hrun [Hgh [Hrep Hfr]]]]]].
      exists (S (4 + f1)), s'. split; [ cbn [length]; lia | ]. split; [ | split; [ exact Hgh | split ] ].
      * rewrite lexec_cons. cbv beta iota. unfold lfree_walk at 1. lsimpl. rewrite Hq, Hb.
        replace (qy =? 0) with false by (symmetry; apply Z.eqb_neq; lia).
        replace (qy <? b) with true by (symmetry; apply Z.ltb_lt; exact Hlt).
        cbv beta iota. rewrite land11. change (1 =? 0) with false. cbv beta iota.
        change (4 + f1)%nat with (S (S (S (S f1)))).
        rewrite lexec_cons, lexec_cons, lexec_cons, lexec_nil. cbv beta iota.
        apply (lexec_mono_le f1); [ lia | | discriminate ].
        unfold lfree_walk. exact Hrun.
      * cbn [insp fst]. replace (qy <? b) with true by (symmetry; apply Z.ltb_lt; exact Hlt).
        rewrite <- app_assoc in Hrep. exact Hrep.
      * intros a Ha. exact (Hfr a Ha).
    + (* exit: y is at or past b *)
      assert (Hfr : forall a, field ((qy, sy) :: r') a -> b + t <= a).
      { intros a Ha.
        assert (Hall : Forall (fun z => b <= fst z /\ 16 <= snd z) ((qy, sy) :: r')).
        { constructor; [ cbn; lia | ]. apply Forall_forall. intros z Hz.
          pose proof (gaps_head_le _ _ Hg) as Hh. rewrite Forall_forall in Hh. specialize (Hh _ Hz).
          split; [ cbn in Hh; lia | exact (gaps_In_size r' (fst z) (snd z) (gaps_tail _ _ Hg) ltac:(destruct z; exact Hz)) ]. }
        destruct Ha as [b' [s' [Hin Ha]]]. rewrite Forall_forall in Hall. destruct (Hall _ Hin) as [Hb' Hs'].
        cbn in Hb', Hs'.
        destruct (Z_le_gt_dec (b + t) b') as [ | Hc ]; [ lia | exfalso ].
        apply (Hdis b'); [ unfold inside; cbn; lia | ].
        apply Exists_exists. exists (b', s'). split; [ apply Hinc; exact Hin | unfold inside; cbn; lia ]. }
      destruct (lfree_finish s pre px sx ((qy, sy) :: r') b t (loc s 3%nat) Hb Ht eq_refl Hp Hq Hseg Hsx Hnx Hr Hfp
                  Hfr Hxb
                  ltac:(destruct Hx as [[? [? ?]] | [? [? [? ?]]]]; [ left | right ]; auto)
                  Hlast Hb16 Ht16) as [s' [Hrun [Hgh [Hrep Hfr']]]].
      exists 21%nat, s'. split; [ cbn [length]; lia | ]. split; [ | split; [ exact Hgh | split ] ].
      * rewrite lexec_cons. cbv beta iota. unfold lfree_walk. lsimpl. rewrite Hq, Hb. cbn [hdb fst].
        replace (qy =? 0) with false by (symmetry; apply Z.eqb_neq; lia).
        replace (qy <? b) with false by (symmetry; apply Z.ltb_ge; exact Hge).
        cbv beta iota. rewrite land10. change (0 =? 0) with true. cbv beta iota. exact Hrun.
      * cbn [insp fst]. replace (qy <? b) with false by (symmetry; apply Z.ltb_ge; exact Hge). exact Hrep.
      * intros a Ha. destruct (Hfr' a Ha) as [H | [H | H]]; [ left; exact H | | ].
        -- destruct Hx as [[_ [-> ->]] | [_ [_ [Hs16 Hin]]]]; [ lia | ].
           right; right. apply Exists_exists. exists (px, sx). split; [ exact Hin | unfold inside; cbn; lia ].
        -- destruct Hx as [[_ [-> ->]] | [_ [_ [Hs16 Hin]]]]; [ right; left; lia | ].
           right; right. apply Exists_exists. exists (px, sx). split; [ exact Hin | unfold inside; cbn; lia ].
Qed.

(* ══ THE $lfree THEOREM ══════════════════════════════════════════════ *)

(* From a memory holding the gapped list L (head at 12, sentinel size
   [4] = 0), releasing an extent (b, t) disjoint from L leaves a memory
   holding `ins L (b, t)`; every changed address is the head cell or lies
   inside an extent of the result. *)
Theorem lfree_realizes : forall L b t s,
  gaps L -> Forall (fun y => 16 <= fst y) L ->
  (forall a, inside (b, t) a -> ~ cov L a) -> 16 <= b -> 16 <= t ->
  mem s 4 = 0 -> lrep (mem s) (mem s LHEAD) L ->
  loc s 0%nat = b -> loc s 1%nat = t -> loc s 4%nat = 0 ->
  exists f s', (f <= 5 * length L + 22)%nat /\ lexec f lfree_tree s = RNorm s' /\ gh s' = gh s /\
    mem s' 4 = 0 /\ lrep (mem s') (mem s' LHEAD) (ins L (b, t)) /\
    (forall a, mem s' a <> mem s a -> a = LHEAD \/ cov (ins L (b, t)) a).
Proof.
  intros L b t s Hg H16 Hdis Hb16 Ht16 H4 Hrep Hb Ht Hp.
  set (s0 := setl s 5 (mem s 12)).
  pose proof (lrep_hd _ _ _ Hrep) as Hh. unfold LHEAD in *.
  assert (Hord : Forall (fun y => 0 + 0 < fst y) L)
    by (eapply Forall_impl; [ | exact H16 ]; intros [q z]; cbn; lia).
  assert (E0 : loc s0 0%nat = b) by (unfold s0; lsimpl; exact Hb).
  assert (E1 : loc s0 1%nat = t) by (unfold s0; lsimpl; exact Ht).
  assert (E4 : loc s0 4%nat = 0) by (unfold s0; lsimpl; exact Hp).
  assert (E5 : loc s0 5%nat = hdb L) by (unfold s0; lsimpl; exact Hh).
  assert (Hr0 : lrep (mem s0) (hdb L) L) by (unfold s0; cbn [mem]; rewrite <- Hh; exact Hrep).
  destruct (lfree_loop L b t L [] 0 0 s0 E0 E1 E4 E5 eq_refl H4 Hh Hr0
              (fun H => ltac:(contradiction)) (fun a Ha => ltac:(destruct Ha as [? [? [[] _]]]))
              (or_introl (conj eq_refl (conj eq_refl eq_refl))) ltac:(lia) Hg Hord
              (fun z Hz => Hz) H16 Hdis Hb16 Ht16)
    as [f [s' [Hf [Hrun [Hgh [Hrep' Hfr]]]]]].
  exists (S f), s'. split; [ lia | ]. rewrite insp_sentinel in Hrep' by (cbn; lia). cbn [app lrep] in Hrep'.
  destruct Hrep' as [_ [H4' Hrep']]. replace (0 + 4) with 4 in H4' by lia. replace (0 + 12) with 12 in Hrep' by lia.
  split; [ | split; [ exact Hgh | split; [ exact H4' | split; [ exact Hrep' | ] ] ] ].
  - unfold lfree_tree. rewrite lexec_cons. exact Hrun.
  - intros a Ha. destruct (Hfr a Ha) as [H | [H | H]].
    + right. apply cov_ins; [ exact (gaps_nonneg _ Hg) | cbn; lia | left; exact H ].
    + left. exact H.
    + right. apply cov_ins; [ exact (gaps_nonneg _ Hg) | cbn; lia | right; exact H ].
Qed.

(* ══ $ltake ═══════════════════════════════════════════════════════════ *)

(* Locals inside `$alloc`: 0 = len, 3 = want (w), 5 = pp, 6 = p, 7 = q,
   8 = z, 9 = r (5..9 fresh, zero on entry). *)
Definition ltake_walk : lstmt :=
  LWhile (LAnd (LNe (LL 7) (LC 0)) (LLtU (at_ (LL 7) 4) (LL 3)))
    [ LSet 5 (LL 6); LSet 6 (LL 7); LSet 7 (at_ (LL 7) 12) ].

Definition ltake_hit : list lstmt :=
  [ LSet 8 (at_ (LL 7) 4);
    LIf (LGeU (LSub (LL 8) (LL 3)) (LC SPLIT))
      [ LSet 9 (LAdd (LL 7) (LL 3));
        LStore (LAdd (LL 9) (LC 0)) (LC 0);
        LStore (LAdd (LL 9) (LC 4)) (LSub (LL 8) (LL 3));
        LStore (LAdd (LL 9) (LC 12)) (at_ (LL 7) 12);
        LStore (LAdd (LL 6) (LC 12)) (LL 9);
        LSet 8 (LL 3) ]
      [ LStore (LAdd (LL 6) (LC 12)) (at_ (LL 7) 12) ];
    LStore (LAdd (LL 7) (LC 0)) (LC 1);
    LStore (LAdd (LL 7) (LC 4)) (LL 0);
    LStore (LAdd (LL 7) (LC 8)) (LSub (LL 8) (LC 12));
    LRet (LL 7) ].

Definition ltake_fin : list lstmt :=
  [ LIf (LNe (LL 7) (LC 0)) ltake_hit [];
    LIf (LEq (LAdd (LL 6) (at_ (LL 6) 4)) LG)
      [ LStore (LAdd (LL 5) (LC 12)) (LC 0); LSetG (LL 6) ] [] ].

Definition ltake_tree : list lstmt :=
  LSet 7 (at_ (LC 0) LHEAD) :: ltake_walk :: ltake_fin.

(* take from a known, already-skipped predecessor x: the walk's view. *)
Definition tlift (e : ext) (r : tres) : tres :=
  match r with
  | TFound a b l => TFound a b (e :: l)
  | TExtend p l => TExtend p (e :: l)
  | TMiss => TMiss
  end.

Fixpoint tkx (x : ext) (r : list ext) (w h : Z) : tres :=
  match r with
  | [] => if fst x + snd x =? h then TExtend (fst x) [] else TMiss
  | (q, z) :: r' =>
      if z <? w then tlift x (tkx (q, z) r' w h)
      else if SPLIT <=? z - w then TFound q w (x :: (q + w, z - w) :: r')
      else TFound q z (x :: r')
  end.

Lemma take_tkx : forall r q z w h, z < w -> take ((q, z) :: r) w h = tkx (q, z) r w h.
Proof.
  induction r as [ | [q1 z1] r IH ]; intros q z w h Hz; cbn [take tkx fst snd].
  - replace (z <? w) with true by (symmetry; apply Z.ltb_lt; exact Hz). reflexivity.
  - replace (z <? w) with true by (symmetry; apply Z.ltb_lt; exact Hz).
    destruct (Z.ltb_spec z1 w) as [H1 | H1].
    + rewrite <- (IH q1 z1 w h H1). cbn [take].
      replace (z1 <? w) with true by (symmetry; apply Z.ltb_lt; exact H1). reflexivity.
    + cbn [take]. replace (z1 <? w) with false by (symmetry; apply Z.ltb_ge; exact H1).
      destruct (SPLIT <=? z1 - w); reflexivity.
Qed.

Lemma tkx_sentinel : forall L w h, h <> 0 -> tkx (0, 0) L w h = tlift (0, 0) (take L w h).
Proof.
  intros [ | [q z] r ] w h Hh; cbn [tkx fst snd].
  - replace (0 + 0 =? h) with false by (symmetry; apply Z.eqb_neq; lia). reflexivity.
  - destruct (Z.ltb_spec z w) as [Hz | Hz].
    + rewrite take_tkx by exact Hz. reflexivity.
    + cbn [take]. replace (z <? w) with false by (symmetry; apply Z.ltb_ge; exact Hz).
      destruct (SPLIT <=? z - w); reflexivity.
Qed.

Lemma lseg_snoc_iff : forall pre m x px sx y,
  lseg m x (pre ++ [(px, sx)]) y <-> lseg m x pre px /\ m (px + 4) = sx /\ m (px + 12) = y.
Proof.
  induction pre as [ | [b s] r IH ]; intros m x px sx y; cbn.
  - split; [ intros [-> [H1 H2]]; auto | intros [-> [H1 H2]]; auto ].
  - rewrite IH. tauto.
Qed.

(* The hit: y = (qy, zy) fits (zy >= w); x = (px, sx) precedes it. *)
Lemma ltake_hit_ok : forall s pre px sx qy zy r' w len,
  loc s 0%nat = len -> loc s 3%nat = w -> loc s 6%nat = px -> loc s 7%nat = qy ->
  lseg (mem s) 0 pre px -> mem s (px + 4) = sx -> mem s (px + 12) = qy ->
  mem s (qy + 4) = zy -> mem s (qy + 12) = hdb r' -> lrep (mem s) (hdb r') r' ->
  (forall a, field pre a -> a < px) ->
  (forall a, field r' a -> qy + zy <= a) ->
  ((pre = [] /\ px = 0 /\ sx = 0) \/ (pre <> [] /\ 16 <= px /\ 16 <= sx)) ->
  px + sx < qy -> 16 <= qy -> 16 <= w <= zy ->
  exists s', lexec 30 ltake_hit s = RRet qy s' /\ gh s' = gh s /\
    lrep (mem s') 0 (pre ++ (px, sx) ::
      (if SPLIT <=? zy - w then (qy + w, zy - w) :: r' else r')) /\
    mem s' qy = 1 /\ mem s' (qy + 4) = len /\
    mem s' (qy + 8) = (if SPLIT <=? zy - w then w else zy) - 12 /\
    (forall a, mem s' a <> mem s a -> (qy <= a < qy + zy) \/ a = px + 12).
Proof.
  intros s pre px sx qy zy r' w len Hl Hw Hp Hq Hseg Hsx Hnx Hzy Hny Hr Hfp Hfr Hx Hxq Hq16 Hwz.
  unfold ltake_hit. run. rewrite ?Hl, ?Hw, ?Hp, ?Hq, ?Hzy in *.
  unfold SPLIT. destruct (Z.leb_spec 65536 (zy - w)) as [Hs | Hs].
  - replace (zy - w >=? 65536) with true by (symmetry; apply Z.geb_le; lia). cnd. run.
    rewrite ?Hl, ?Hw, ?Hp, ?Hq, ?Hzy, ?Hny in *.
    eexists. split; [ reflexivity | ]. cbn [mem gh]. split; [ reflexivity | ].
    assert (Hpq : px + 12 < qy) by (destruct Hx as [[_ [-> ->]] | [_ [_ ?]]]; lia).
    split; [ | split; [ | split; [ | split ] ] ].
    + apply (lseg_app_lrep _ _ _ px).
      * apply (lseg_frame pre (mem s)); [ | exact Hseg ].
        intros a Ha. pose proof (Hfp a Ha). unfold upd. eqbs; lia.
      * cbn. split; [ reflexivity | ]. unfold upd. eqbs; try lia.
        split; [ exact Hsx | ]. split; [ reflexivity | ]. split; [ reflexivity | ].
        apply (lrep_frame r' (mem s)); [ | rewrite ?Hny; exact Hr ].
        intros a Ha. pose proof (Hfr a Ha). unfold upd. eqbs; lia.
    + unfold upd. eqbs; lia.
    + unfold upd. eqbs; lia.
    + unfold upd. eqbs; lia.
    + intros a Ha. revert Ha. unfold upd. eqbs; intros Ha; lia.
  - replace (zy - w >=? 65536) with false by (symmetry; rewrite Z.geb_leb; apply Z.leb_gt; lia). cnd. run.
    rewrite ?Hl, ?Hw, ?Hp, ?Hq, ?Hzy, ?Hny in *.
    eexists. split; [ reflexivity | ]. cbn [mem gh]. split; [ reflexivity | ].
    assert (Hpq : px + 12 < qy) by (destruct Hx as [[_ [-> ->]] | [_ [_ ?]]]; lia).
    split; [ | split; [ | split; [ | split ] ] ].
    + apply (lseg_app_lrep _ _ _ px).
      * apply (lseg_frame pre (mem s)); [ | exact Hseg ].
        intros a Ha. pose proof (Hfp a Ha). unfold upd. eqbs; lia.
      * cbn. split; [ reflexivity | ]. unfold upd. eqbs; try lia.
        split; [ exact Hsx | ].
        apply (lrep_frame r' (mem s)); [ | rewrite ?Hny; exact Hr ].
        intros a Ha. pose proof (Hfr a Ha). unfold upd. eqbs; lia.
    + unfold upd. eqbs; lia.
    + unfold upd. eqbs; lia.
    + unfold upd. eqbs; lia.
    + intros a Ha. revert Ha. unfold upd. eqbs; intros Ha; lia.
Qed.

(* The walk's result, as the state after it must show it. *)
Definition tpost (pre : list ext) (res : tres) (h len : Z) (s s' : LS) (out : lres) : Prop :=
  match res with
  | TFound q z l' => out = RRet q s' /\ gh s' = h /\ lrep (mem s') 0 (pre ++ l') /\
                     mem s' q = 1 /\ mem s' (q + 4) = len /\ mem s' (q + 8) = z - 12
  | TExtend p l' => out = RNorm s' /\ gh s' = p /\ lrep (mem s') 0 (pre ++ l')
  | TMiss => out = RNorm s' /\ gh s' = h /\ (forall a, mem s' a = mem s a)
  end.

(* The predecessor link of a nonempty prefix: its last node (pp, spp). *)
Definition plast (L : list ext) (pre : list ext) (pp px : Z) : Prop :=
  pre <> [] -> exists pre0 spp, pre = pre0 ++ [(pp, spp)] /\
    (forall a, field pre0 a -> a < pp) /\
    ((pre0 = [] /\ pp = 0 /\ spp = 0) \/ (16 <= pp /\ 16 <= spp /\ In (pp, spp) L)).

(* No fit: q = 0, x = (px, sx) is the last node (or the sentinel). *)
Lemma ltake_nofit : forall L s pre px sx h len,
  loc s 6%nat = px -> loc s 7%nat = 0 -> gh s = h ->
  lseg (mem s) 0 pre px -> mem s (px + 4) = sx -> mem s (px + 12) = 0 ->
  plast L pre (loc s 5%nat) px ->
  ((pre = [] /\ px = 0 /\ sx = 0) \/ (pre <> [] /\ 16 <= px /\ 16 <= sx)) ->
  h <> 0 ->
  exists s' out, lexec 10 ltake_fin s = out /\
    tpost pre (tkx (px, sx) [] 1 h) h len s s' out /\
    (forall a, mem s' a <> mem s a -> a = 12 \/ cov L a).
Proof.
  intros L s pre px sx h len Hp Hq Hh Hseg Hsx Hnx Hlast Hx Hh0.
  unfold ltake_fin. run. rewrite ?Hp, ?Hq, ?Hh in *. cnd. run. rewrite ?Hp, ?Hq, ?Hh, ?Hsx in *.
  cbn [tkx fst snd tpost].
  destruct (Z.eqb_spec (px + sx) h) as [E | E]; cnd; run.
  - destruct Hx as [[_ [-> ->]] | [Hne [Hp16 Hs16]]]; [ lia | ].
    destruct (Hlast Hne) as [pre0 [spp [-> [Hf0 Hpp]]]].
    apply lseg_snoc_iff in Hseg as [Hseg0 [Hspp Hlink]].
    set (pp := loc s 5%nat) in *.
    eexists. eexists. split; [ reflexivity | ].
    split; [ split; [ reflexivity | split; [ cbn [gh]; exact Hp | ] ] | ].
    + cbn [mem]. rewrite app_nil_r. apply lrep_app. exists pp. split.
      * apply (lseg_frame pre0 (mem s)); [ | exact Hseg0 ].
        intros a Ha. pose proof (Hf0 a Ha). unfold upd. eqbs; lia.
      * cbn [lrep]. split; [ reflexivity | ]. unfold upd. eqbs; lia.
    + cbn [mem]. intros a Ha. revert Ha. unfold upd. eqbs; intros Ha; [ | contradiction ].
      subst a. destruct Hpp as [[_ [-> _]] | [Hq16 [Hs16' Hin]]]; [ left; lia | right ].
      apply Exists_exists. exists (pp, spp). split; [ exact Hin | unfold inside; cbn; lia ].
  - eexists. eexists. split; [ reflexivity | ].
    split; [ split; [ reflexivity | split; [ exact Hh | reflexivity ] ] | ].
    intros a Ha. contradiction.
Qed.

Lemma tpost_lift : forall pre x res h len s s' out,
  tpost (pre ++ [x]) res h len s s' out -> tpost pre (tlift x res) h len s s' out.
Proof.
  intros pre x [ q z l | p l | ] h len s s' out H; cbn [tlift tpost] in *;
    rewrite <- ?app_assoc in H; exact H.
Qed.

Lemma ltake_loop : forall L w h len r pre px sx s,
  loc s 0%nat = len -> loc s 3%nat = w -> loc s 6%nat = px -> loc s 7%nat = hdb r -> gh s = h ->
  lseg (mem s) 0 pre px -> mem s (px + 4) = sx -> mem s (px + 12) = hdb r ->
  lrep (mem s) (hdb r) r ->
  plast L pre (loc s 5%nat) px ->
  (forall a, field pre a -> a < px) ->
  ((pre = [] /\ px = 0 /\ sx = 0) \/ (pre <> [] /\ 16 <= px /\ 16 <= sx /\ In (px, sx) L)) ->
  gaps r -> Forall (fun y => px + sx < fst y) r -> incl r L ->
  Forall (fun y => 16 <= fst y) L -> h <> 0 -> 16 <= w ->
  exists f s' out, (f <= 5 * length r + 33)%nat /\ lexec f (ltake_walk :: ltake_fin) s = out /\
    tpost pre (tkx (px, sx) r w h) h len s s' out /\
    (forall a, mem s' a <> mem s a -> a = 12 \/ cov L a).
Proof.
  intros L w h len r. induction r as [ | [qy zy] r' IH ];
    intros pre px sx s Hl Hw Hp Hq Hh Hseg Hsx Hnx Hr Hlast Hfp Hx Hg Hord Hinc HL16 Hh0 Hw16.
  - destruct (ltake_nofit L s pre px sx h len Hp Hq Hh Hseg Hsx Hnx Hlast
                ltac:(destruct Hx as [H | [? [? [? _]]]]; [ left; exact H | right; auto ]) Hh0)
      as [s' [out [Hrun [Hpost Hfr]]]].
    exists 11%nat, s', out. split; [ cbn [length]; lia | ]. split; [ | split; [ exact Hpost | exact Hfr ] ].
    rewrite lexec_cons. cbv beta iota. unfold ltake_walk. lsimpl. rewrite Hq. cbn [hdb].
    change ((if 0 =? 0 then 0 else 1)) with 0. rewrite land0x. change (0 =? 0) with true.
    cbv beta iota. exact Hrun.
  - cbn [hdb fst] in Hq, Hnx.
    pose proof Hr as Hr0. cbn in Hr0. destruct Hr0 as [_ [Hzy Hr'']].
    pose proof (lrep_hd _ _ _ Hr'') as Hnq. rewrite Hnq in Hr''.
    assert (HinL : In (qy, zy) L) by (apply Hinc; left; reflexivity).
    assert (Hq16 : 16 <= qy) by (rewrite Forall_forall in HL16; exact (HL16 _ HinL)).
    assert (Hzy16 : 16 <= zy) by (destruct Hg as [H _]; unfold MINSZ in H; exact H).
    assert (Hxq : px + sx < qy)
      by (rewrite Forall_forall in Hord; exact (Hord _ (or_introl eq_refl))).
    destruct (Z.ltb_spec zy w) as [Hlt | Hge].
    + (* skip y: it becomes the predecessor *)
      set (s1 := setl (setl (setl s 5 (loc s 6)) 6 (loc s 7)) 7 (mem s (loc s 7 + 12))).
      assert (E0 : loc s1 0%nat = len) by (unfold s1; lsimpl; exact Hl).
      assert (E3 : loc s1 3%nat = w) by (unfold s1; lsimpl; exact Hw).
      assert (E5 : loc s1 5%nat = px) by (unfold s1; lsimpl; exact Hp).
      assert (E6 : loc s1 6%nat = qy) by (unfold s1; lsimpl; rewrite Hq; reflexivity).
      assert (E7 : loc s1 7%nat = hdb r') by (unfold s1; lsimpl; rewrite Hq; exact Hnq).
      assert (Hseg1 : lseg (mem s1) 0 (pre ++ [(px, sx)]) qy) by (apply lseg_snoc; assumption).
      assert (Hlast1 : plast L (pre ++ [(px, sx)]) (loc s1 5%nat) qy).
      { intros _. rewrite E5. exists pre, sx. split; [ reflexivity | split; [ exact Hfp | ] ].
        destruct Hx as [[-> [-> ->]] | [_ [? [? ?]]]]; [ left; auto | right; auto ]. }
      assert (Hfp1 : forall a, field (pre ++ [(px, sx)]) a -> a < qy).
      { intros a Ha. apply field_app in Ha as [Ha | Ha]; [ pose proof (Hfp a Ha); lia | ].
        destruct Ha as [b' [s'' [[H | []] Ha]]]. injection H as <- <-.
        destruct Hx as [[_ [-> ->]] | [_ [_ [? _]]]]; lia. }
      assert (Hx1 : (pre ++ [(px, sx)] = [] /\ qy = 0 /\ zy = 0) \/
                    (pre ++ [(px, sx)] <> [] /\ 16 <= qy /\ 16 <= zy /\ In (qy, zy) L)).
      { right. split; [ destruct pre; discriminate | auto ]. }
      destruct (IH (pre ++ [(px, sx)]) qy zy s1 E0 E3 E6 E7 Hh Hseg1 Hzy Hnq Hr'' Hlast1 Hfp1 Hx1
                  (gaps_tail _ _ Hg) (gaps_head_le _ _ Hg) (fun z Hz => Hinc z (or_intror Hz)) HL16 Hh0 Hw16)
        as [f1 [s' [out [Hf1 [Hrun [Hpost Hfr]]]]]].
      exists (S (4 + f1)), s', out. split; [ cbn [length]; lia | ]. split; [ | split; [ | exact Hfr ] ].
      * rewrite lexec_cons. cbv beta iota. unfold ltake_walk at 1. lsimpl. rewrite Hq, Hw, Hzy.
        replace (qy =? 0) with false by (symmetry; apply Z.eqb_neq; lia).
        replace (zy <? w) with true by (symmetry; apply Z.ltb_lt; exact Hlt).
        cbv beta iota. rewrite land11. change (1 =? 0) with false. cbv beta iota.
        change (4 + f1)%nat with (S (S (S (S f1)))).
        rewrite lexec_cons, lexec_cons, lexec_cons, lexec_nil. cbv beta iota.
        apply (lexec_mono_le f1); [ lia | | ].
        -- unfold ltake_walk. exact Hrun.
        -- destruct (tkx (qy, zy) r' w h); cbn [tpost] in Hpost; destruct Hpost as [-> _]; discriminate.
      * cbn [tkx]. replace (zy <? w) with true by (symmetry; apply Z.ltb_lt; exact Hlt).
        apply tpost_lift. exact Hpost.
    + (* y fits: the hit *)
      assert (Hfr' : forall a, field r' a -> qy + zy <= a).
      { intros a [b' [s'' [Hin Ha]]].
        pose proof (gaps_head_le _ _ Hg) as Hh'. rewrite Forall_forall in Hh'. specialize (Hh' _ Hin).
        cbn in Hh'. lia. }
      assert (Hx' : (pre = [] /\ px = 0 /\ sx = 0) \/ (pre <> [] /\ 16 <= px /\ 16 <= sx))
        by (destruct Hx as [H | [? [? [? _]]]]; [ left; exact H | right; auto ]).
      destruct (ltake_hit_ok s pre px sx qy zy r' w len Hl Hw Hp Hq Hseg Hsx Hnx Hzy Hnq Hr'' Hfp Hfr' Hx'
                  Hxq Hq16 ltac:(lia)) as [s' [Hrun [Hgh [Hrep [H1 [H2 [H3 Hfr]]]]]]].
      exists 33%nat, s', (RRet qy s'). split; [ cbn [length]; lia | ]. split; [ | split ].
      * rewrite lexec_cons. cbv beta iota. unfold ltake_walk at 1. lsimpl. rewrite Hq, Hw, Hzy.
        replace (qy =? 0) with false by (symmetry; apply Z.eqb_neq; lia).
        replace (zy <? w) with false by (symmetry; apply Z.ltb_ge; exact Hge).
        cbv beta iota. rewrite land10. change (0 =? 0) with true. cbv beta iota.
        unfold ltake_fin. rewrite lexec_cons. cbv beta iota. lsimpl. rewrite Hq.
        replace (qy =? 0) with false by (symmetry; apply Z.eqb_neq; lia).
        change (1 =? 0) with false. cbv beta iota.
        rewrite (lexec_mono_le 30 31 _ _ _ ltac:(lia) Hrun ltac:(discriminate)). reflexivity.
      * cbn [tkx]. replace (zy <? w) with false by (symmetry; apply Z.ltb_ge; exact Hge).
        destruct (SPLIT <=? zy - w); cbn [tpost];
          (split; [ reflexivity | split; [ rewrite Hgh; exact Hh | split; [ exact Hrep | auto ] ] ]).
      * intros a Ha. destruct (Hfr a Ha) as [H | H].
        -- right. apply Exists_exists. exists (qy, zy). split; [ exact HinL | unfold inside; cbn; lia ].
        -- destruct Hx as [[_ [-> ->]] | [_ [_ [Hs16 Hin]]]]; [ left; lia | right ].
           apply Exists_exists. exists (px, sx). split; [ exact Hin | unfold inside; cbn; lia ].
Qed.

(* ══ THE $ltake THEOREM ══════════════════════════════════════════════ *)

(* From a memory holding the gapped list L, `$ltake w len` does exactly
   what LargeList.take decides: a hit RETURNS the base with the header
   rc=1 / len / cap and the memory holding the rest; an extension
   lowers the frontier to the unlinked tail node; a miss changes
   nothing. Every changed address is the head cell or inside L. *)
Theorem ltake_realizes : forall L w h len s,
  gaps L -> Forall (fun y => 16 <= fst y) L -> 16 <= w -> h <> 0 ->
  mem s 4 = 0 -> lrep (mem s) (mem s LHEAD) L ->
  loc s 0%nat = len -> loc s 3%nat = w -> loc s 6%nat = 0 -> gh s = h ->
  exists f s' out, (f <= 5 * length L + 34)%nat /\ lexec f ltake_tree s = out /\
    (forall a, mem s' a <> mem s a -> a = LHEAD \/ cov L a) /\
    match take L w h with
    | TFound q z l' => out = RRet q s' /\ gh s' = h /\ mem s' 4 = 0 /\
                       lrep (mem s') (mem s' LHEAD) l' /\
                       mem s' q = 1 /\ mem s' (q + 4) = len /\ mem s' (q + 8) = z - 12
    | TExtend p l' => out = RNorm s' /\ gh s' = p /\ mem s' 4 = 0 /\
                      lrep (mem s') (mem s' LHEAD) l'
    | TMiss => out = RNorm s' /\ gh s' = h /\ (forall a, mem s' a = mem s a)
    end.
Proof.
  intros L w h len s Hg H16 Hw16 Hh0 H4 Hrep Hl Hw Hp Hh.
  set (s0 := setl s 7 (mem s 12)).
  pose proof (lrep_hd _ _ _ Hrep) as Hhd. unfold LHEAD in *.
  assert (Hord : Forall (fun y => 0 + 0 < fst y) L)
    by (eapply Forall_impl; [ | exact H16 ]; intros [q z]; cbn; lia).
  assert (E0 : loc s0 0%nat = len) by (unfold s0; lsimpl; exact Hl).
  assert (E3 : loc s0 3%nat = w) by (unfold s0; lsimpl; exact Hw).
  assert (E6 : loc s0 6%nat = 0) by (unfold s0; lsimpl; exact Hp).
  assert (E7 : loc s0 7%nat = hdb L) by (unfold s0; lsimpl; exact Hhd).
  assert (Hr0 : lrep (mem s0) (hdb L) L) by (unfold s0; cbn [mem]; rewrite <- Hhd; exact Hrep).
  destruct (ltake_loop L w h len L [] 0 0 s0 E0 E3 E6 E7 Hh eq_refl H4 Hhd Hr0
              (fun H => ltac:(contradiction)) (fun a Ha => ltac:(destruct Ha as [? [? [[] _]]]))
              (or_introl (conj eq_refl (conj eq_refl eq_refl))) Hg Hord
              (fun z Hz => Hz) H16 Hh0 Hw16)
    as [f [s' [out [Hf [Hrun [Hpost Hfr]]]]]].
  exists (S f), s', out. split; [ lia | ]. split; [ unfold ltake_tree; rewrite lexec_cons; exact Hrun | ].
  split; [ exact Hfr | ].
  rewrite tkx_sentinel in Hpost by exact Hh0.
  destruct (take L w h) as [ q z l' | p l' | ]; cbn [tlift tpost app lrep] in Hpost.
  - destruct Hpost as [Ho [Hg' [[_ [H4' Hrep']] Hhdr]]]. auto.
  - destruct Hpost as [Ho [Hg' [_ [H4' Hrep']]]]. auto.
  - exact Hpost.
Qed.

(* ══ FIXED-FUEL SEMANTICS FOR THE CALLERS' TREES ═════════════════════
   StructuralRuntime / StructuralAlloc run their trees as total
   functions; the two loops enter them through ONE fuel bound. A gapped
   list of nodes at or above 16 that ends below 2^32 (the i32 memory)
   has fewer than 2^28 nodes, and the realization theorems spend at most
   5 per node plus a constant, so `LFUEL` always suffices: the fixed-fuel
   run IS the realized run (`lfree_mem_spec`, `ltake_run_spec`). *)

Definition MEMTOP : Z := 4294967296.
Definition LFUELZ : Z := 1342177344.   (* 5 * 2^28 + 64 *)
Definition LFUEL : nat := Z.to_nat LFUELZ.

Lemma gaps_length : forall l lo,
  lo <= MEMTOP -> gaps l -> Forall (fun e => lo <= fst e) l -> ends_below l MEMTOP ->
  16 * Z.of_nat (length l) <= MEMTOP - lo.
Proof.
  induction l as [ | [b s] r IH ]; intros lo Hm Hg Hlo Hend; cbn [length].
  - lia.
  - rewrite Forall_cons_iff in Hlo. destruct Hlo as [Hb Hlo].
    unfold ends_below in Hend. rewrite Forall_cons_iff in Hend. destruct Hend as [He Hend].
    cbn [fst snd] in *.
    assert (Hs : 16 <= s) by (destruct Hg as [H _]; unfold MINSZ in H; exact H).
    destruct r as [ | e r' ].
    + cbn [length]. change (Z.of_nat 1) with 1. lia.
    + specialize (IH (b + s) ltac:(lia) (gaps_tail _ _ Hg)).
      assert (Hlo' : Forall (fun e0 => b + s <= fst e0) (e :: r')).
      { eapply Forall_impl; [ | exact (gaps_head_le _ _ Hg) ]. intros a Ha. cbn in Ha. lia. }
      specialize (IH Hlo' Hend). rewrite Nat2Z.inj_succ. lia.
Qed.

Lemma fuel_fits : forall (f : nat) (n : nat) k,
  (f <= 5 * n + k)%nat -> (k <= 64)%nat -> 16 * Z.of_nat n <= MEMTOP -> (f <= LFUEL)%nat.
Proof.
  intros f n k Hf Hk Hn. unfold LFUEL, LFUELZ.
  apply Nat2Z.inj_le. rewrite Z2Nat.id by lia. unfold MEMTOP in Hn. lia.
Qed.

(* `$lfree` as the function `$free`'s tree calls: locals b, t, the
   class scratch, and zeroed walk locals. *)
Definition lfree_locals (b t cls : Z) : nat -> Z :=
  fun j => match j with 0%nat => b | 1%nat => t | 2%nat => cls | _ => 0 end.

Definition lfree_mem (b t cls : Z) (m : Mem) : Mem :=
  match lexec LFUEL lfree_tree (mkLS (lfree_locals b t cls) m 0) with
  | RNorm s' => mem s'
  | _ => m
  end.

Theorem lfree_mem_spec : forall L b t cls m,
  gaps L -> Forall (fun y => 16 <= fst y) L -> ends_below L MEMTOP ->
  (forall a, inside (b, t) a -> ~ cov L a) -> 16 <= b -> 16 <= t ->
  m 4 = 0 -> lrep m (m LHEAD) L ->
  let m' := lfree_mem b t cls m in
  m' 4 = 0 /\ lrep m' (m' LHEAD) (ins L (b, t)) /\
  (forall a, m' a <> m a -> a = LHEAD \/ cov (ins L (b, t)) a).
Proof.
  intros L b t cls m Hg H16 Hend Hdis Hb Ht H4 Hrep m'.
  destruct (lfree_realizes L b t (mkLS (lfree_locals b t cls) m 0) Hg H16 Hdis Hb Ht H4 Hrep
              eq_refl eq_refl eq_refl) as [f [s' [Hf [Hrun [_ [H4' [Hrep' Hfr]]]]]]].
  assert (Hlen : 16 * Z.of_nat (length L) <= MEMTOP)
    by (pose proof (gaps_length L 16 ltac:(unfold MEMTOP; lia) Hg H16 Hend); lia).
  assert (HF : lexec LFUEL lfree_tree (mkLS (lfree_locals b t cls) m 0) = RNorm s')
    by (apply (lexec_mono_le f); [ exact (fuel_fits f _ 22 Hf ltac:(lia) Hlen) | exact Hrun | discriminate ]).
  unfold m', lfree_mem. rewrite HF. split; [ exact H4' | split; [ exact Hrep' | exact Hfr ] ].
Qed.

(* A tree that never sets local i leaves it alone. *)
Fixpoint noset (i : nat) (st : lstmt) : bool :=
  match st with
  | LSet j _ => negb (Nat.eqb i j)
  | LIf _ th el => forallb (noset i) th && forallb (noset i) el
  | LWhile _ b => forallb (noset i) b
  | _ => true
  end.

Definition res_loc (r : lres) (i : nat) (v : Z) : Prop :=
  match r with RNorm s' => loc s' i = v | RRet _ s' => loc s' i = v | RFuel => True end.

Lemma lexec_noset : forall f ss s i,
  forallb (noset i) ss = true -> res_loc (lexec f ss s) i (loc s i).
Proof.
  induction f as [ | f IH ]; intros ss s i H; [ exact I | ].
  destruct ss as [ | st rest ]; [ rewrite lexec_nil; reflexivity | ].
  cbn [forallb] in H. apply andb_prop in H as [Hst Hr].
  rewrite lexec_cons. destruct st as [ j e | a v | e | c th el | c body | e ]; cbv beta iota.
  - cbn [noset] in Hst. apply negb_true_iff, Nat.eqb_neq in Hst.
    pose proof (IH rest (setl s j (lev e s)) i Hr) as H1.
    assert (E : loc (setl s j (lev e s)) i = loc s i)
      by (cbn [loc setl]; destruct (Nat.eqb_spec i j); [ contradiction | reflexivity ]).
    rewrite E in H1. exact H1.
  - exact (IH rest _ i Hr).
  - exact (IH rest _ i Hr).
  - cbn [noset] in Hst. apply andb_prop in Hst as [Ht He].
    pose proof (IH (if lev c s =? 0 then el else th) s i ltac:(destruct (lev c s =? 0); assumption)) as H1.
    destruct (lexec f (if lev c s =? 0 then el else th) s) as [ s1 | v s1 | ]; cbn in H1 |- *; auto.
    specialize (IH rest s1 i Hr). rewrite H1 in IH. exact IH.
  - destruct (lev c s =? 0); [ exact (IH rest s i Hr) | ].
    cbn [noset] in Hst.
    pose proof (IH body s i Hst) as H1.
    destruct (lexec f body s) as [ s1 | v s1 | ]; cbn in H1 |- *; auto.
    specialize (IH (LWhile c body :: rest) s1 i ltac:(cbn [forallb noset]; rewrite Hst, Hr; reflexivity)).
    rewrite H1 in IH. exact IH.
  - reflexivity.
Qed.

(* `$ltake` as the function `$alloc`'s tree calls: locals len, base,
   next, want, head, and zeroed walk locals. *)
Definition ltake_locals (len base next want head : Z) : nat -> Z :=
  fun j => match j with
           | 0%nat => len | 1%nat => base | 2%nat => next | 3%nat => want | 4%nat => head
           | _ => 0 end.

Definition ltake_run (len base next want head : Z) (m : Mem) (g : Z) : lres :=
  lexec LFUEL ltake_tree (mkLS (ltake_locals len base next want head) m g).

Theorem ltake_run_spec : forall L len base next want head m h,
  gaps L -> Forall (fun y => 16 <= fst y) L -> ends_below L MEMTOP ->
  16 <= want -> h <> 0 -> m 4 = 0 -> lrep m (m LHEAD) L ->
  exists s',
    (forall a, mem s' a <> m a -> a = LHEAD \/ cov L a) /\
    loc s' 1%nat = base /\ loc s' 2%nat = next /\ loc s' 3%nat = want /\ loc s' 4%nat = head /\
    match take L want h with
    | TFound q z l' => ltake_run len base next want head m h = RRet q s' /\ gh s' = h /\
                       mem s' 4 = 0 /\ lrep (mem s') (mem s' LHEAD) l' /\
                       mem s' q = 1 /\ mem s' (q + 4) = len /\ mem s' (q + 8) = z - 12
    | TExtend p l' => ltake_run len base next want head m h = RNorm s' /\ gh s' = p /\
                      mem s' 4 = 0 /\ lrep (mem s') (mem s' LHEAD) l'
    | TMiss => ltake_run len base next want head m h = RNorm s' /\ gh s' = h /\
               (forall a, mem s' a = m a)
    end.
Proof.
  intros L len base next want head m h Hg H16 Hend Hw Hh0 H4 Hrep.
  set (s0 := mkLS (ltake_locals len base next want head) m h).
  destruct (ltake_realizes L want h len s0 Hg H16 Hw Hh0 H4 Hrep eq_refl eq_refl eq_refl eq_refl)
    as [f [s' [out [Hf [Hrun [Hfr Hres]]]]]].
  assert (Hlen : 16 * Z.of_nat (length L) <= MEMTOP)
    by (pose proof (gaps_length L 16 ltac:(unfold MEMTOP; lia) Hg H16 Hend); lia).
  assert (HF : ltake_run len base next want head m h = out).
  { unfold ltake_run. apply (lexec_mono_le f); [ exact (fuel_fits f _ 34 Hf ltac:(lia) Hlen) | exact Hrun | ].
    intros ->. destruct (take L want h); destruct Hres as [Ho _]; discriminate. }
  assert (Hloc : forall i, (1 <= i <= 4)%nat -> res_loc out i (loc s0 i)).
  { intros i Hi. rewrite <- Hrun.
    apply lexec_noset. unfold ltake_tree, ltake_walk, ltake_fin, ltake_hit.
    destruct i as [ | [ | [ | [ | [ | i ] ] ] ] ]; try lia; reflexivity. }
  assert (Hl : forall i, (1 <= i <= 4)%nat -> loc s' i = loc s0 i).
  { intros i Hi. specialize (Hloc i Hi).
    destruct (take L want h); destruct Hres as [-> _]; exact Hloc. }
  exists s'. split; [ exact Hfr | ].
  split; [ exact (Hl 1%nat ltac:(lia)) | split; [ exact (Hl 2%nat ltac:(lia)) |
    split; [ exact (Hl 3%nat ltac:(lia)) | split; [ exact (Hl 4%nat ltac:(lia)) | ] ] ] ].
  rewrite HF. exact Hres.
Qed.
