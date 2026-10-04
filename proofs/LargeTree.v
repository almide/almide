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
  | LL (i : Z)             (* local.get i *)
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
  | LSet (i : Z) (e : lexpr)
  | LStore (a v : lexpr)
  | LSetG (e : lexpr)
  | LIf (c : lexpr) (th el : list lstmt)
  | LWhile (c : lexpr) (body : list lstmt)
  | LRet (e : lexpr).

Record LS := mkLS { loc : Z -> Z; mem : Mem; gh : Z }.

Definition setl (s : LS) (i v : Z) : LS :=
  mkLS (fun j => if j =? i then v else loc s j) (mem s) (gh s).

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
