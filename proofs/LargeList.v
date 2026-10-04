(* Almide v1 trust spine — #3348: the LARGE-BLOCK free list, as a model.

   The structural wasm allocator files a freed block whose total is at
   most 512 KiB into one of 16 power-of-two classes (StructuralRuntime.v /
   StructuralAlloc.v). Before #3348 a bigger block was ABANDONED, so a loop
   that copied an 8 MB list grew linear memory by 8 MB per iteration
   (#3337 reached 2 GB). Above the class ceiling the runtime now keeps an
   ADDRESS-ORDERED, EXACT-SIZE free list with COALESCE on free and SPLIT on
   take:

     `$lfree b t`  walks to the last node below `b`, merges `[b, b+t)` with
                   its successor when they touch, then with its
                   predecessor when they touch, and links the result in.
     `$ltake w`    walks to the first node of size `>= w` (first fit); a
                   node with at least SPLIT bytes to spare is split (the
                   tail stays free), else it is handed out whole. With no
                   fit, a last node that ends AT the bump frontier is
                   unlinked and the frontier lowered to its base, so the
                   bump that follows extends it instead of starting past
                   it.

   This file is the list-level model of those two operations (`ins`,
   `take`) and the extent-level SAFETY argument the point model
   (FreeList.v) cannot state: blocks here have SIZES, and a split or a
   merge changes which addresses are block bases. The state carries the
   free list, the GHOST list of live large extents, and the bump frontier;
   `LINV` says the free list is well formed (ascending, coalesced — every
   gap strict — sizes >= 16, at or above the floor), every extent lies
   below the frontier, live extents are pairwise disjoint, and no address
   is both free and live. The theorems:

     `free_preserves_LINV`   — releasing a live extent keeps LINV;
     `alloc_preserves_LINV`  — every take outcome (found, extend, miss)
                               keeps LINV, and
     `alloc_disjoint_live`   — the handed-out extent overlaps NO live
                               extent: the reuse-after-free class at
                               extent granularity, proven.

   LargeTree.v proves the emitted loop trees compute exactly `ins` and
   `take` on the in-memory list. *)

From Stdlib Require Import ZArith Lia List Bool.
Import ListNotations.
Open Scope Z_scope.

(* An extent: (base, size). *)
Notation ext := (Z * Z)%type (only parsing).

(* A node is split only when at least this much would remain free. *)
Definition SPLIT : Z := 65536.
(* The smallest node: header (rc, size, cap) and the next pointer. *)
Definition MINSZ : Z := 16.

(* lia with the two size constants unfolded. *)
Ltac zl := unfold MINSZ, SPLIT in *; lia.

Definition inside (e : ext) (x : Z) : Prop := fst e <= x < fst e + snd e.
Definition cov (l : list ext) (x : Z) : Prop := Exists (fun e => inside e x) l.

(* Ascending with STRICT gaps (fully coalesced), every size >= MINSZ. *)
Fixpoint gaps (l : list ext) : Prop :=
  match l with
  | [] => True
  | e :: r =>
      MINSZ <= snd e /\
      match r with [] => True | e' :: _ => fst e + snd e < fst e' end /\
      gaps r
  end.

Definition wf (lo : Z) (l : list ext) : Prop :=
  gaps l /\ Forall (fun e => lo <= fst e) l.

Definition ends_below (l : list ext) (h : Z) : Prop :=
  Forall (fun e => fst e + snd e <= h) l.

Definition disj (e1 e2 : ext) : Prop :=
  fst e1 + snd e1 <= fst e2 \/ fst e2 + snd e2 <= fst e1.

Fixpoint pdisj (l : list ext) : Prop :=
  match l with
  | [] => True
  | e :: r => Forall (disj e) r /\ pdisj r
  end.

(* ── the operations, mirroring the trees' walks ── *)

(* Merge `(b,t)` with the head of `post` when they touch. *)
Definition merge_succ (e : ext) (post : list ext) : list ext :=
  match post with
  | (q, s) :: post' => if fst e + snd e =? q then (fst e, snd e + s) :: post'
                       else e :: post
  | [] => [e]
  end.

(* Link `x` in front of `l'`, merging when `x` touches its head. *)
Definition link_pred (x : ext) (l' : list ext) : list ext :=
  match l' with
  | y :: r => if fst x + snd x =? fst y then (fst x, snd x + snd y) :: r
              else x :: y :: r
  | [] => [x]
  end.

(* `$lfree`: walk while the NEXT node is below `b`; `x` is then the
   predecessor. *)
Fixpoint ins (l : list ext) (e : ext) : list ext :=
  match l with
  | [] => [e]
  | x :: r =>
      if fst x <? fst e then
        match r with
        | y :: _ => if fst y <? fst e then x :: ins r e
                    else link_pred x (merge_succ e r)
        | [] => link_pred x (merge_succ e [])
        end
      else merge_succ e l
  end.

Inductive tres :=
  | TFound (q z : Z) (l : list ext)   (* hand out [q, q+z); l is the rest *)
  | TExtend (p : Z) (l : list ext)    (* unlink the tail node at p; bump from p *)
  | TMiss.                            (* bump from the frontier *)

(* `$ltake`: first fit; split when SPLIT or more would remain. *)
Fixpoint take (l : list ext) (w h : Z) : tres :=
  match l with
  | [] => TMiss
  | (q, z) :: r =>
      if z <? w then
        match r with
        | [] => if q + z =? h then TExtend q [] else TMiss
        | _ :: _ =>
            match take r w h with
            | TFound a b r' => TFound a b ((q, z) :: r')
            | TExtend p r' => TExtend p ((q, z) :: r')
            | TMiss => TMiss
            end
        end
      else if SPLIT <=? z - w then TFound q w ((q + w, z - w) :: r)
      else TFound q z r
  end.

(* ══ LIST-LEVEL FACTS ═════════════════════════════════════════════════ *)

Lemma cov_cons : forall e l x, cov (e :: l) x <-> inside e x \/ cov l x.
Proof.
  intros e l x. unfold cov. split.
  - intros H. inversion H; subst; auto.
  - intros [H | H]; [ apply Exists_cons_hd | apply Exists_cons_tl ]; auto.
Qed.

Lemma cov_nil : forall x, ~ cov [] x.
Proof. intros x H. inversion H. Qed.

Lemma cov_app : forall l1 l2 x, cov (l1 ++ l2) x <-> cov l1 x \/ cov l2 x.
Proof.
  intros l1 l2 x. unfold cov. rewrite Exists_app. tauto.
Qed.

(* In a gapped list every node sits at or above the head's base. *)
Lemma gaps_head_le : forall e r, gaps (e :: r) ->
  Forall (fun e' => fst e + snd e < fst e') r.
Proof.
  intros e r. revert e. induction r as [ | e' r IH ]; intros e H.
  - constructor.
  - destruct H as [Hs [Hg Hr]]. constructor; [ exact Hg | ].
    specialize (IH e' Hr).
    destruct Hr as [Hs' _].
    eapply Forall_impl; [ | exact IH ]. intros a Ha. cbn in *. unfold MINSZ in *. zl.
Qed.

Lemma gaps_tail : forall e r, gaps (e :: r) -> gaps r.
Proof. intros e r [_ [_ H]]. exact H. Qed.

(* An address covered by the tail of a gapped list lies past the head. *)
Lemma cov_tail_after : forall e r x, gaps (e :: r) -> cov r x -> fst e + snd e < x + 1.
Proof.
  intros e r x Hg Hc.
  pose proof (gaps_head_le e r Hg) as Hf.
  unfold cov in Hc. apply Exists_exists in Hc as [e' [Hin Hx]].
  rewrite Forall_forall in Hf. specialize (Hf e' Hin).
  unfold inside in Hx. zl.
Qed.

(* Everything covered lies at or past the head's base. *)
Lemma cov_ge_head : forall e r x, gaps (e :: r) -> cov (e :: r) x -> fst e <= x.
Proof.
  intros e r x Hg Hc. apply cov_cons in Hc as [Hx | Hx].
  - unfold inside in Hx. zl.
  - pose proof (cov_tail_after e r x Hg Hx). destruct Hg as [Hs _]. unfold MINSZ in Hs. zl.
Qed.

(* ── merge_succ / link_pred coverage ── *)

Definition nonneg (l : list ext) : Prop := Forall (fun e => 0 <= snd e) l.

Lemma gaps_nonneg : forall l, gaps l -> nonneg l.
Proof.
  induction l as [ | e r IH ]; intros H; constructor.
  - destruct H as [H _]. unfold MINSZ in H. zl.
  - apply IH. exact (gaps_tail _ _ H).
Qed.

Lemma merge_succ_nonneg : forall e post, 0 <= snd e -> nonneg post -> nonneg (merge_succ e post).
Proof.
  intros [b t] post He Hp. destruct post as [ | [q s] post' ]; cbn in *.
  - constructor; [ exact He | constructor ].
  - inversion Hp; subst. destruct (b + t =? q); constructor; cbn in *; try zl; try assumption.
Qed.

Lemma cov_merge_succ : forall e post x, nonneg post -> 0 <= snd e ->
  cov (merge_succ e post) x <-> inside e x \/ cov post x.
Proof.
  intros [b t] post x Hp Ht. cbn in Ht. destruct post as [ | [q s] post' ]; cbn.
  - rewrite cov_cons. split; [ intros [H | H]; [ left; exact H | exfalso; apply (cov_nil x H) ] | ].
    intros [H | H]; [ left; exact H | exfalso; apply (cov_nil x H) ].
  - inversion Hp as [ | ? ? Hs _ ]; subst. cbn in Hs.
    destruct (Z.eqb_spec (b + t) q) as [E | _].
    + rewrite !cov_cons. unfold inside; cbn. split.
      * intros [H | H]; [ | right; right; exact H ].
        destruct (Z_lt_ge_dec x q); [ left; zl | right; left; zl ].
      * intros [H | [H | H]]; [ left; zl | left; zl | right; exact H ].
    + rewrite !cov_cons. tauto.
Qed.

Lemma cov_link_pred : forall x0 l x, nonneg l -> 0 <= snd x0 ->
  cov (link_pred x0 l) x <-> inside x0 x \/ cov l x.
Proof.
  intros [p sp] l x Hl Hp. cbn in Hp. destruct l as [ | [y sy] r ]; cbn.
  - rewrite cov_cons. split; [ intros [H | H]; [ left; exact H | exfalso; apply (cov_nil x H) ] | ].
    intros [H | H]; [ left; exact H | exfalso; apply (cov_nil x H) ].
  - inversion Hl as [ | ? ? Hs _ ]; subst. cbn in Hs.
    destruct (Z.eqb_spec (p + sp) y) as [E | _].
    + rewrite !cov_cons. unfold inside; cbn. split.
      * intros [H | H]; [ | right; right; exact H ].
        destruct (Z_lt_ge_dec x y); [ left; zl | right; left; zl ].
      * intros [H | [H | H]]; [ left; zl | left; zl | right; exact H ].
    + rewrite !cov_cons. tauto.
Qed.

Lemma ins_nonneg : forall l e, nonneg l -> 0 <= snd e -> nonneg (ins l e).
Proof.
  induction l as [ | x0 r IH ]; intros e Hl He; cbn [ins].
  - constructor; [ exact He | constructor ].
  - inversion Hl as [ | ? ? Hx0 Hr ]; subst.
    assert (Hlp : forall l', nonneg l' -> nonneg (link_pred x0 l')).
    { intros l' H'. destruct l' as [ | [y sy] r' ]; cbn.
      - constructor; [ exact Hx0 | constructor ].
      - inversion H'; subst. destruct (fst x0 + snd x0 =? y); constructor; cbn in *; try zl; try assumption. }
    destruct (fst x0 <? fst e).
    + destruct r as [ | y r' ].
      * apply Hlp. apply merge_succ_nonneg; [ exact He | constructor ].
      * destruct (fst y <? fst e).
        -- constructor; [ exact Hx0 | apply IH; assumption ].
        -- apply Hlp. apply merge_succ_nonneg; assumption.
    + apply merge_succ_nonneg; [ exact He | exact Hl ].
Qed.

Theorem cov_ins : forall l e x, nonneg l -> 0 <= snd e ->
  cov (ins l e) x <-> inside e x \/ cov l x.
Proof.
  induction l as [ | x0 r IH ]; intros e x Hl He; cbn [ins].
  - rewrite cov_cons. split; [ intros [H | H]; [ left; exact H | exfalso; apply (cov_nil x H) ] | ].
    intros [H | H]; [ left; exact H | exfalso; apply (cov_nil x H) ].
  - inversion Hl as [ | ? ? Hx0 Hr ]; subst.
    destruct (fst x0 <? fst e).
    + destruct r as [ | y r' ].
      * rewrite cov_link_pred, cov_merge_succ, cov_cons;
          [ tauto | constructor | exact He | apply merge_succ_nonneg; [ exact He | constructor ] | exact Hx0 ].
      * destruct (fst y <? fst e).
        -- rewrite cov_cons, IH by assumption. rewrite (cov_cons x0). tauto.
        -- rewrite cov_link_pred, cov_merge_succ;
             [ rewrite !cov_cons; tauto | exact Hr | exact He | apply merge_succ_nonneg; assumption | exact Hx0 ].
    + rewrite cov_merge_succ by assumption. rewrite cov_cons. tauto.
Qed.

(* ── well-formedness of ins ── *)

(* The first base of a nonempty list (anything past it otherwise). *)
Definition hd_base (l : list ext) (dflt : Z) : Z :=
  match l with [] => dflt | e :: _ => fst e end.

(* `gaps` of a cons, by its head relation. *)
Lemma gaps_cons : forall e r,
  MINSZ <= snd e ->
  (forall e', hd_error r = Some e' -> fst e + snd e < fst e') ->
  gaps r -> gaps (e :: r).
Proof.
  intros e r Hs Hh Hr. cbn. split; [ exact Hs | split; [ | exact Hr ] ].
  destruct r as [ | e' r' ]; [ exact I | ]. apply Hh. reflexivity.
Qed.

(* merge_succ keeps gaps when e ends at or before post's head and post is
   gapped; its head is e's base. *)
Lemma merge_succ_gaps : forall e post,
  MINSZ <= snd e -> gaps post ->
  (forall e', hd_error post = Some e' -> fst e + snd e <= fst e') ->
  gaps (merge_succ e post) /\ hd_base (merge_succ e post) 0 = fst e.
Proof.
  intros [b t] post Hs Hg Hh. destruct post as [ | [q s] post' ]; cbn in *.
  - split; [ split; [ exact Hs | split; exact I ] | reflexivity ].
  - specialize (Hh (q, s) eq_refl). cbn in Hh.
    destruct (Z.eqb_spec (b + t) q) as [E | NE].
    + destruct Hg as [Hsq [Hgq Hr]]. split; [ | reflexivity ].
      split; [ cbn; zl | split; [ | exact Hr ] ].
      destruct post' as [ | [q2 s2] p2 ]; [ exact I | cbn in *; zl ].
    + split; [ | reflexivity ].
      split; [ exact Hs | split; [ cbn; zl | exact Hg ] ].
Qed.

(* The end of the head of merge_succ is e's end, or q's end after a merge;
   either way it stays below what followed. *)
Lemma merge_succ_head_end : forall e post,
  gaps post ->
  (forall e', hd_error post = Some e' -> fst e + snd e <= fst e') ->
  forall e1, hd_error (merge_succ e post) = Some e1 ->
  fst e1 = fst e /\
  (forall e2, hd_error (tl (merge_succ e post)) = Some e2 -> fst e1 + snd e1 < fst e2).
Proof.
  intros [b t] post Hg Hh e1 He1. destruct post as [ | [q s] post' ]; cbn in *.
  - injection He1 as <-. split; [ reflexivity | intros e2 H; discriminate ].
  - specialize (Hh (q, s) eq_refl). cbn in Hh.
    destruct (Z.eqb_spec (b + t) q) as [E | NE]; cbn in He1.
    + injection He1 as <-. split; [ reflexivity | ]. intros e2 H2. cbn in H2.
      destruct post' as [ | e3 p3 ]; [ discriminate | ]. injection H2 as <-.
      destruct Hg as [_ [Hg _]]. cbn in *. zl.
    + injection He1 as <-. split; [ reflexivity | ]. intros e2 H2. cbn in H2.
      injection H2 as <-. cbn. zl.
Qed.

(* A gapped list whose head ends strictly before y, then y's tail. *)
Lemma link_pred_gaps : forall x l,
  MINSZ <= snd x -> gaps l ->
  (forall e', hd_error l = Some e' -> fst x + snd x <= fst e') ->
  gaps (link_pred x l).
Proof.
  intros [p sp] l Hs Hg Hh. destruct l as [ | [y sy] r ]; cbn in *.
  - split; [ exact Hs | split; exact I ].
  - specialize (Hh (y, sy) eq_refl). cbn in Hh.
    destruct (Z.eqb_spec (p + sp) y) as [E | NE].
    + destruct Hg as [Hsy [Hgy Hr]]. split; [ cbn; zl | split; [ | exact Hr ] ].
      destruct r as [ | [r1 sr1] r' ]; [ exact I | cbn in *; zl ].
    + split; [ exact Hs | split; [ cbn; zl | exact Hg ] ].
Qed.

(* The head of link_pred x l is x's base. *)
Lemma link_pred_head : forall x l, hd_base (link_pred x l) 0 = fst x.
Proof.
  intros [p sp] l. destruct l as [ | [y sy] r ]; cbn; [ reflexivity | ].
  destruct (p + sp =? y); reflexivity.
Qed.

Lemma Forall_lo_cov : forall lo l x, Forall (fun e => lo <= fst e) l -> cov l x -> lo <= x.
Proof.
  intros lo l x Hf Hc. unfold cov in Hc. apply Exists_exists in Hc as [e [Hin Hx]].
  rewrite Forall_forall in Hf. specialize (Hf e Hin). unfold inside in Hx. zl.
Qed.

(* The base of the head of `ins l e` is min(head base of l, base of e). *)
Lemma ins_head : forall l e,
  hd_base (ins l e) 0 = match l with [] => fst e | x :: _ =>
                           if fst x <? fst e then fst x else fst e end.
Proof.
  intros l e. destruct l as [ | x r ]; [ reflexivity | ]. cbn [ins].
  destruct (fst x <? fst e) eqn:Ex.
  - destruct r as [ | y r' ]; [ apply link_pred_head | ].
    destruct (fst y <? fst e); [ reflexivity | apply link_pred_head ].
  - destruct e as [b t], x as [q s]. unfold merge_succ. cbn.
    destruct (b + t =? q); reflexivity.
Qed.

(* A list whose head lies below e keeps that head (base) under ins. *)
Lemma ins_hd_below : forall y r e, fst y < fst e ->
  exists s, hd_error (ins (y :: r) e) = Some (fst y, s).
Proof.
  intros y r e H. cbn [ins].
  replace (fst y <? fst e) with true by (symmetry; apply Z.ltb_lt; exact H).
  assert (Hl : forall l', exists s, hd_error (link_pred y l') = Some (fst y, s)).
  { intros l'. destruct l' as [ | [q s] r' ]; cbn.
    - exists (snd y). destruct y; reflexivity.
    - destruct (fst y + snd y =? q); [ eexists; reflexivity | ].
      exists (snd y). destruct y; reflexivity. }
  destruct r as [ | y2 r2 ]; [ apply Hl | ].
  destruct (fst y2 <? fst e); [ | apply Hl ].
  exists (snd y). destruct y; reflexivity.
Qed.

(* ins keeps gaps, provided e is disjoint from the list. *)
Theorem ins_gaps : forall l e,
  gaps l -> MINSZ <= snd e ->
  (forall x, inside e x -> ~ cov l x) ->
  gaps (ins l e).
Proof.
  induction l as [ | x0 r IH ]; intros e Hg Hs Hd; cbn [ins].
  - split; [ exact Hs | split; exact I ].
  - destruct (Z.ltb_spec (fst x0) (fst e)) as [Hlt | Hge].
    + (* x0 is below e: e lies wholly past x0's end *)
      assert (Hx0e : fst x0 + snd x0 <= fst e).
      { destruct (Z_le_gt_dec (fst x0 + snd x0) (fst e)) as [ | Hc ]; [ assumption | ].
        exfalso. apply (Hd (fst e)).
        - unfold inside. zl.
        - apply cov_cons. left. unfold inside. zl. }
      destruct r as [ | y r' ].
      * apply link_pred_gaps; [ destruct Hg as [H _]; exact H | | ].
        -- apply (proj1 (merge_succ_gaps e [] Hs I (fun e' H => ltac:(discriminate)))).
        -- intros e' He'. cbn in He'. destruct e as [b t]. injection He' as <-. cbn in *. zl.
      * destruct (Z.ltb_spec (fst y) (fst e)) as [Hylt | Hyge].
        -- (* keep walking *)
           assert (Hgr : gaps (y :: r')) by (exact (gaps_tail _ _ Hg)).
           assert (Hdr : forall x, inside e x -> ~ cov (y :: r') x).
           { intros x Hx Hc. apply (Hd x Hx). apply cov_cons. right. exact Hc. }
           specialize (IH e Hgr Hs Hdr).
           apply gaps_cons; [ destruct Hg as [H _]; exact H | | exact IH ].
           intros e' He'. destruct (ins_hd_below y r' e Hylt) as [sy Hhd].
           change (hd_error (ins (y :: r') e) = Some e') in He'.
           rewrite Hhd in He'. injection He' as <-. cbn.
           destruct Hg as [_ [Hg _]]. exact Hg.
        -- (* x0 is the predecessor; y is the successor *)
           assert (Hey : fst e + snd e <= fst y).
           { destruct (Z_le_gt_dec (fst e + snd e) (fst y)) as [ | Hc ]; [ assumption | ].
             exfalso. apply (Hd (fst y)).
             - unfold inside. zl.
             - apply cov_cons. right. apply cov_cons. left. unfold inside.
               destruct Hg as [_ [_ [Hsy _]]]. unfold MINSZ in *. zl. }
           assert (Hgr : gaps (y :: r')) by (exact (gaps_tail _ _ Hg)).
           destruct (merge_succ_gaps e (y :: r') Hs Hgr) as [Hgm Hhm].
           { intros e' He'. cbn in He'. injection He' as <-. exact Hey. }
           apply link_pred_gaps; [ destruct Hg as [H _]; exact H | exact Hgm | ].
           intros e' He'.
           destruct (merge_succ e (y :: r')) as [ | e1 l1 ] eqn:Em; [ discriminate | ].
           cbn in He'. injection He' as <-. cbn in Hhm. rewrite Hhm. exact Hx0e.
    + (* e goes in front of x0 *)
      assert (Hex : fst e + snd e <= fst x0).
      { destruct (Z_le_gt_dec (fst e + snd e) (fst x0)) as [ | Hc ]; [ assumption | ].
        exfalso. apply (Hd (fst x0)).
        - unfold inside. destruct Hg as [Hs0 _]. unfold MINSZ in *. zl.
        - apply cov_cons. left. unfold inside. destruct Hg as [Hs0 _]. unfold MINSZ in *. zl. }
      apply (proj1 (merge_succ_gaps e (x0 :: r) Hs Hg
                     (fun e' He' => ltac:(cbn in He'; injection He' as <-; exact Hex)))).
Qed.

Theorem ins_wf : forall lo l e,
  wf lo l -> MINSZ <= snd e -> lo <= fst e ->
  (forall x, inside e x -> ~ cov l x) ->
  wf lo (ins l e).
Proof.
  intros lo l e [Hg Hf] Hs Hlo Hd. split; [ apply ins_gaps; assumption | ].
  apply Forall_forall. intros e' Hin.
  (* every node of `ins` starts at an address it covers *)
  assert (Hc : cov (ins l e) (fst e')).
  { apply Exists_exists. exists e'. split; [ exact Hin | ].
    unfold inside. pose proof (ins_gaps l e Hg Hs Hd) as Hgi.
    clear - Hgi Hin. induction (ins l e) as [ | a r IH ]; [ inversion Hin | ].
    destruct Hin as [-> | Hin].
    - destruct Hgi as [H _]. unfold MINSZ in H. zl.
    - apply IH; (exact Hin || exact (gaps_tail _ _ Hgi)). }
  rewrite cov_ins in Hc by (first [ exact (gaps_nonneg _ Hg) | zl ]).
  destruct Hc as [Hc | Hc].
  - unfold inside in Hc. zl.
  - exact (Forall_lo_cov lo l (fst e') Hf Hc).
Qed.

(* ══ TAKE ═════════════════════════════════════════════════════════════ *)

(* The found extent is covered by the list, serves the request, and the
   rest covers exactly what is left. *)
Theorem take_found : forall l w h q z l',
  gaps l -> MINSZ <= w ->
  take l w h = TFound q z l' ->
  w <= z /\ MINSZ <= z /\
  (forall x, q <= x < q + z -> cov l x) /\
  (forall x, cov l' x <-> cov l x /\ ~ (q <= x < q + z)) /\
  gaps l'.
Proof.
  induction l as [ | [q0 z0] r IH ]; intros w h q z l' Hg Hw Ht; cbn in Ht.
  - discriminate.
  - destruct (Z.ltb_spec z0 w) as [Hlt | Hge].
    + destruct r as [ | e1 r1 ]; [ destruct (q0 + z0 =? h); discriminate | ].
      destruct (take (e1 :: r1) w h) as [ a b r' | p r' | ] eqn:Er; try discriminate.
      injection Ht as <- <- <-.
      destruct (IH w h a b r' (gaps_tail _ _ Hg) Hw Er) as [Hwz [Hmz [Hcv [Hrest Hgr]]]].
      split; [ exact Hwz | split; [ exact Hmz | split ] ].
      { intros x Hx. apply cov_cons. right. apply Hcv. exact Hx. }
      split.
      * intros x. rewrite (cov_cons (q0, z0) r'), (cov_cons (q0, z0) (e1 :: r1)), Hrest.
        (* x in the head (q0,z0) is never inside [a, a+b) — that extent is
           covered by the tail, which lies past the head *)
        split.
        -- intros [H | [H1 H2]]; [ | split; [ right; exact H1 | exact H2 ] ].
           split; [ left; exact H | ]. intros Hab.
           pose proof (cov_tail_after (q0, z0) (e1 :: r1) a Hg (Hcv a ltac:(zl))) as Ha.
           unfold inside in H. cbn in *. zl.
        -- intros [[H | H] H2]; [ left; exact H | right; split; assumption ].
      * apply gaps_cons; [ destruct Hg as [H _]; exact H | | exact Hgr ].
        intros e' He'. destruct r' as [ | e2 r2 ]; [ discriminate | ].
        cbn in He'. injection He' as <-.
        (* e2's base is covered by the tail of the original list *)
        assert (Hc2 : cov r1 (fst e2) \/ cov [e1] (fst e2)).
        { assert (Hcr : cov (e2 :: r2) (fst e2)).
          { apply cov_cons. left. unfold inside. destruct Hgr as [H _]. unfold MINSZ in H. zl. }
          apply Hrest in Hcr as [Hcr _]. apply cov_cons in Hcr as [H | H].
          - right. apply cov_cons. left. exact H.
          - left. exact H. }
        assert (Hc3 : cov (e1 :: r1) (fst e2)).
        { destruct Hc2 as [H | H]; apply cov_cons; [ right; exact H | ].
          left. apply cov_cons in H as [H | H]; [ exact H | exfalso; apply (cov_nil _ H) ]. }
        (* strict: the head ends strictly before e1, and e2 starts at or past e1 *)
        pose proof (cov_ge_head e1 r1 (fst e2) (gaps_tail _ _ Hg) Hc3) as Hge.
        destruct Hg as [_ [Hgg _]]. cbn in *. zl.
    + destruct (Z.leb_spec SPLIT (z0 - w)) as [Hsp | Hns].
      * injection Ht as <- <- <-.
        split; [ zl | split; [ unfold SPLIT, MINSZ in *; zl | split ] ].
        { intros x Hx. apply cov_cons. left. unfold inside; cbn. unfold SPLIT in Hsp. zl. }
        split.
        -- intros x. rewrite !cov_cons. unfold inside; cbn. split.
           ++ intros [H | H].
              ** split; [ left; zl | zl ].
              ** split; [ right; exact H | ].
                 pose proof (cov_tail_after (q0, z0) r x Hg H) as Hx. cbn in Hx. zl.
           ++ intros [[H | H] Hn]; [ left; zl | right; exact H ].
        -- destruct Hg as [Hs [Hgg Hr]]. split; [ cbn; unfold SPLIT, MINSZ in *; zl | ].
           split; [ | exact Hr ].
           destruct r as [ | e1 r1 ]; [ exact I | cbn in *; zl ].
      * injection Ht as <- <- <-.
        split; [ zl | split; [ destruct Hg as [H _]; exact H | split ] ].
        { intros x Hx. apply cov_cons. left. unfold inside; cbn. zl. }
        split.
        -- intros x. rewrite cov_cons. unfold inside; cbn. split.
           ++ intros H. split; [ right; exact H | ].
              pose proof (cov_tail_after (q0, z0) r x Hg H) as Hx. cbn in Hx. zl.
           ++ intros [[H | H] Hn]; [ zl | exact H ].
        -- exact (gaps_tail _ _ Hg).
Qed.

(* Extend: the unlinked node is the LAST one, it ends at the frontier, and
   every remaining extent ends strictly before it. *)
Theorem take_extend : forall l w h p l',
  gaps l -> take l w h = TExtend p l' ->
  exists sp, l = l' ++ [(p, sp)] /\ p + sp = h /\ sp < w /\
             Forall (fun e => fst e + snd e < p) l' /\ gaps l'.
Proof.
  induction l as [ | [q0 z0] r IH ]; intros w h p l' Hg Ht; cbn in Ht; [ discriminate | ].
  destruct (Z.ltb_spec z0 w) as [Hlt | Hge].
  - destruct r as [ | e1 r1 ].
    + destruct (Z.eqb_spec (q0 + z0) h) as [E | _]; [ | discriminate ].
      injection Ht as <- <-. exists z0. split; [ reflexivity | ].
      split; [ exact E | split; [ exact Hlt | split; [ constructor | exact I ] ] ].
    + destruct (take (e1 :: r1) w h) as [ a b r' | p' r' | ] eqn:Er; try discriminate.
      injection Ht as Ep El. subst p' l'.
      destruct (IH w h p r' (gaps_tail _ _ Hg) Er) as [sp [Hl [Hh [Hs [Hf Hgr]]]]].
      exists sp. split; [ rewrite Hl; reflexivity | ].
      split; [ exact Hh | split; [ exact Hs | split ] ].
      * constructor; [ | exact Hf ].
        (* the head ends before e1, which is at or before p *)
        assert (Hcp : cov (e1 :: r1) p).
        { rewrite Hl. apply cov_app. right. apply cov_cons. left. unfold inside; cbn.
          assert (Hgl : gaps (r' ++ [(p, sp)])) by (rewrite <- Hl; exact (gaps_tail _ _ Hg)).
          clear - Hgl. induction r' as [ | a r'' IHr ]; cbn in Hgl.
          - destruct Hgl as [H _]. cbn in H. unfold MINSZ in H. zl.
          - apply IHr. destruct Hgl as [_ [_ H]]. exact H. }
        pose proof (cov_ge_head e1 r1 p (gaps_tail _ _ Hg) Hcp) as Hge.
        destruct Hg as [_ [Hgg _]]. cbn in *. zl.
      * apply gaps_cons; [ destruct Hg as [H _]; exact H | | exact Hgr ].
        intros e' He'. destruct r' as [ | e2 r2 ]; [ discriminate | ].
        cbn in He'. injection He' as <-.
        assert (Hce : cov (e1 :: r1) (fst e2)).
        { rewrite Hl. apply cov_app. left. apply cov_cons. left. unfold inside.
          destruct Hgr as [H _]. unfold MINSZ in H. zl. }
        pose proof (cov_ge_head e1 r1 (fst e2) (gaps_tail _ _ Hg) Hce) as Hge.
        destruct Hg as [_ [Hgg _]]. cbn in *. zl.
  - destruct (SPLIT <=? z0 - w); discriminate.
Qed.

(* ══ THE EXTENT-LEVEL SAFETY MODEL ════════════════════════════════════ *)

Record LState := { heap : Z; fl : list ext; live : list ext }.

Definition LINV (lo : Z) (st : LState) : Prop :=
  wf lo (fl st) /\
  ends_below (fl st) (heap st) /\
  Forall (fun e => lo <= fst e /\ 0 < snd e /\ fst e + snd e <= heap st) (live st) /\
  pdisj (live st) /\
  (forall x, cov (fl st) x -> cov (live st) x -> False).

(* Release: the live extent `e` (at its position in the ghost list)
   joins the free list. The ghost position is the PCC-style validation —
   the ownership certificate guarantees the block is live. *)
Definition lfree_st (st : LState) (live1 live2 : list ext) (e : ext) : LState :=
  {| heap := heap st; fl := ins (fl st) e; live := live1 ++ live2 |}.

(* Take `w` bytes: a free-list hit (whole or split), an extension of the
   tail node, or a fresh bump. Returns the handed-out base. *)
Definition lalloc (st : LState) (w : Z) : Z * LState :=
  match take (fl st) w (heap st) with
  | TFound q z l' => (q, {| heap := heap st; fl := l'; live := (q, z) :: live st |})
  | TExtend p l' => (p, {| heap := p + w; fl := l'; live := (p, w) :: live st |})
  | TMiss => (heap st, {| heap := heap st + w; fl := fl st;
                          live := (heap st, w) :: live st |})
  end.

(* ── helpers ── *)

Lemma Forall_disj_of_cov : forall e l,
  0 < snd e -> Forall (fun e' => 0 < snd e') l ->
  (forall x, inside e x -> ~ cov l x) -> Forall (disj e) l.
Proof.
  intros e l He Hp Hd. apply Forall_forall. intros e' Hin.
  rewrite Forall_forall in Hp. specialize (Hp e' Hin).
  unfold disj. destruct (Z_le_gt_dec (fst e + snd e) (fst e')) as [ | H1 ]; [ left; assumption | ].
  destruct (Z_le_gt_dec (fst e' + snd e') (fst e)) as [ | H2 ]; [ right; assumption | ].
  exfalso. apply (Hd (Z.max (fst e) (fst e'))).
  - unfold inside. zl.
  - apply Exists_exists. exists e'. split; [ exact Hin | ]. unfold inside. zl.
Qed.

Lemma cov_of_Forall_disj : forall e l x,
  Forall (disj e) l -> inside e x -> ~ cov l x.
Proof.
  intros e l x Hf Hx Hc. unfold cov in Hc. apply Exists_exists in Hc as [e' [Hin Hx']].
  rewrite Forall_forall in Hf. specialize (Hf e' Hin). unfold disj, inside in *. zl.
Qed.

Lemma pdisj_app_rem : forall l1 e l2, pdisj (l1 ++ e :: l2) ->
  pdisj (l1 ++ l2) /\ Forall (disj e) (l1 ++ l2).
Proof.
  induction l1 as [ | a l1 IH ]; intros e l2 H; cbn in *.
  - destruct H as [H1 H2]. split; [ exact H2 | exact H1 ].
  - destruct H as [Ha Hr]. destruct (IH e l2 Hr) as [Hp He].
    rewrite Forall_app in Ha. destruct Ha as [Ha1 Ha2]. inversion Ha2; subst.
    split.
    + split; [ apply Forall_app; split; assumption | exact Hp ].
    + constructor; [ | exact He ].
      unfold disj in *. zl.
Qed.

Lemma cov_app_rem : forall l1 e l2 x, cov (l1 ++ l2) x -> cov (l1 ++ e :: l2) x.
Proof.
  intros l1 e l2 x H. apply cov_app in H. apply cov_app.
  destruct H as [H | H]; [ left; exact H | right; apply cov_cons; right; exact H ].
Qed.

Lemma ends_below_mono : forall l h h', ends_below l h -> h <= h' -> ends_below l h'.
Proof.
  intros l h h' H Hle. eapply Forall_impl; [ | exact H ]. intros a Ha. cbn in Ha. zl.
Qed.

Lemma cov_below : forall l h x, ends_below l h -> cov l x -> x < h.
Proof.
  intros l h x Hf Hc. unfold cov in Hc. apply Exists_exists in Hc as [e [Hin Hx]].
  unfold ends_below in Hf. rewrite Forall_forall in Hf. specialize (Hf e Hin).
  unfold inside in Hx. zl.
Qed.

Lemma live_cov_below : forall lo l h x,
  Forall (fun e => lo <= fst e /\ 0 < snd e /\ fst e + snd e <= h) l -> cov l x -> lo <= x < h.
Proof.
  intros lo l h x Hf Hc. unfold cov in Hc. apply Exists_exists in Hc as [e [Hin Hx]].
  rewrite Forall_forall in Hf. specialize (Hf e Hin). unfold inside in Hx. zl.
Qed.

(* `ins` covers exactly the old list plus e, so its ends stay below a
   frontier both lay below. *)
Lemma ins_ends_below : forall l e h,
  gaps l -> MINSZ <= snd e ->
  gaps (ins l e) -> ends_below l h -> fst e + snd e <= h -> ends_below (ins l e) h.
Proof.
  intros l e h Hgl Hse Hg Hl He. unfold ends_below. apply Forall_forall. intros e' Hin.
  (* the last covered address of e' is covered by ins, hence below h *)
  assert (Hs : 0 < snd e').
  { clear - Hg Hin. induction (ins l e) as [ | a r IH ]; [ inversion Hin | ].
    destruct Hin as [-> | Hin]; [ destruct Hg as [H _]; unfold MINSZ in H; zl | ].
    apply IH; (exact Hin || exact (gaps_tail _ _ Hg)). }
  assert (Hc : cov (ins l e) (fst e' + snd e' - 1)).
  { apply Exists_exists. exists e'. split; [ exact Hin | unfold inside; zl ]. }
  rewrite cov_ins in Hc by (first [ exact (gaps_nonneg _ Hgl) | zl ]).
  destruct Hc as [Hc | Hc].
  - unfold inside in Hc. zl.
  - pose proof (cov_below l h _ Hl Hc). zl.
Qed.

(* ══ THE SAFETY THEOREMS ══════════════════════════════════════════════ *)

Theorem free_preserves_LINV : forall lo st live1 live2 e,
  LINV lo st ->
  live st = live1 ++ e :: live2 ->
  MINSZ <= snd e ->
  LINV lo (lfree_st st live1 live2 e).
Proof.
  intros lo st live1 live2 e [Hwf [Heb [Hlv [Hpd Hfl]]]] Hlive Hsz.
  pose proof Hwf as Hwf0.
  unfold lfree_st, LINV; cbn [live heap fl].
  rewrite Hlive in Hlv, Hpd, Hfl.
  assert (He : lo <= fst e /\ 0 < snd e /\ fst e + snd e <= heap st).
  { rewrite Forall_app in Hlv. destruct Hlv as [_ H]. inversion H; subst. assumption. }
  assert (Hd : forall x, inside e x -> ~ cov (fl st) x).
  { intros x Hx Hc. apply (Hfl x Hc). apply cov_app. right. apply cov_cons. left. exact Hx. }
  destruct (pdisj_app_rem live1 e live2 Hpd) as [Hpd' Hed].
  assert (Hwf' : wf lo (ins (fl st) e)) by (apply ins_wf; tauto).
  split; [ exact Hwf' | split ].
  - apply ins_ends_below; [ exact (proj1 Hwf0) | exact Hsz | exact (proj1 Hwf') | exact Heb | tauto ].
  - split.
    + rewrite Forall_app in Hlv |- *. destruct Hlv as [H1 H2]. inversion H2; subst. tauto.
    + split; [ exact Hpd' | ].
      intros x Hc Hl.
      rewrite cov_ins in Hc by (first [ exact (gaps_nonneg _ (proj1 Hwf0)) | zl ]).
      destruct Hc as [Hc | Hc].
      * exact (cov_of_Forall_disj e (live1 ++ live2) x Hed Hc Hl).
      * apply (Hfl x Hc). apply cov_app_rem. exact Hl.
Qed.

Theorem alloc_preserves_LINV : forall lo st w,
  LINV lo st -> MINSZ <= w -> lo <= heap st ->
  LINV lo (snd (lalloc st w)).
Proof.
  intros lo st w [[Hg Hlo] [Heb [Hlv [Hpd Hfl]]]] Hw Hh.
  assert (Hlvpos : Forall (fun e' => 0 < snd e') (live st)).
  { eapply Forall_impl; [ | exact Hlv ]. intros a [_ [Ha _]]. exact Ha. }
  unfold lalloc, LINV. destruct (take (fl st) w (heap st)) as [ q z l' | p l' | ] eqn:Et;
    cbn [snd live heap fl].
  - destruct (take_found (fl st) w (heap st) q z l' Hg Hw Et)
      as [Hwz [Hmz [Hcv [Hrest Hgl]]]].
    assert (Hq : lo <= q /\ q + z <= heap st).
    { split.
      - apply (Forall_lo_cov lo (fl st) q Hlo). apply Hcv. unfold MINSZ in Hmz. zl.
      - pose proof (cov_below _ _ _ Heb (Hcv (q + z - 1) ltac:(unfold MINSZ in Hmz; zl))). zl. }
    split; [ split; [ exact Hgl | ] | split; [ | split; [ | split ] ] ].
    + apply Forall_forall. intros e' Hin.
      assert (Hc : cov l' (fst e')).
      { apply Exists_exists. exists e'. split; [ exact Hin | ]. unfold inside.
        clear - Hgl Hin. induction l' as [ | a r IH ]; [ inversion Hin | ].
        destruct Hin as [-> | Hin]; [ destruct Hgl as [H _]; unfold MINSZ in H; zl | ].
        apply IH; (exact Hin || exact (gaps_tail _ _ Hgl)). }
      apply Hrest in Hc as [Hc _]. exact (Forall_lo_cov lo _ _ Hlo Hc).
    + unfold ends_below. apply Forall_forall. intros e' Hin.
      assert (Hs : 0 < snd e').
      { clear - Hgl Hin. induction l' as [ | a r IH ]; [ inversion Hin | ].
        destruct Hin as [-> | Hin]; [ destruct Hgl as [H _]; unfold MINSZ in H; zl | ].
        apply IH; (exact Hin || exact (gaps_tail _ _ Hgl)). }
      assert (Hc : cov l' (fst e' + snd e' - 1)).
      { apply Exists_exists. exists e'. split; [ exact Hin | unfold inside; zl ]. }
      apply Hrest in Hc as [Hc _]. pose proof (cov_below _ _ _ Heb Hc). zl.
    + constructor; [ cbn; unfold MINSZ in Hmz; zl | exact Hlv ].
    + cbn. split; [ | exact Hpd ].
      apply Forall_disj_of_cov; [ cbn; unfold MINSZ in Hmz; zl | exact Hlvpos | ].
      intros x Hx Hl. apply (Hfl x); [ apply Hcv; unfold inside in Hx; cbn in Hx; zl | exact Hl ].
    + intros x Hc Hl. apply cov_cons in Hl as [Hl | Hl].
      * apply Hrest in Hc as [_ Hn]. unfold inside in Hl. cbn in Hl. zl.
      * apply Hrest in Hc as [Hc _]. exact (Hfl x Hc Hl).
  - destruct (take_extend (fl st) w (heap st) p l' Hg Et) as [sp [Hl [Hph [Hsp [Hf Hgl]]]]].
    assert (Hp : lo <= p).
    { rewrite Hl in Hlo. rewrite Forall_app in Hlo. destruct Hlo as [_ H]. inversion H; subst. exact H2. }
    split; [ split; [ exact Hgl | ] | split; [ | split; [ | split ] ] ].
    + rewrite Hl in Hlo. rewrite Forall_app in Hlo. tauto.
    + unfold ends_below. eapply Forall_impl; [ | exact Hf ]. intros a Ha. cbn. zl.
    + constructor; [ cbn; unfold MINSZ in Hw; zl | ].
      eapply Forall_impl; [ | exact Hlv ]. intros a Ha. cbn. zl.
    + cbn. split; [ | exact Hpd ].
      apply Forall_disj_of_cov; [ cbn; unfold MINSZ in Hw; zl | exact Hlvpos | ].
      intros x Hx Hlx. unfold inside in Hx; cbn in Hx.
      destruct (Z_lt_ge_dec x (heap st)) as [Hlt | Hge].
      * (* [p, heap) was the free tail node *)
        apply (Hfl x); [ | exact Hlx ].
        rewrite Hl. apply cov_app. right. apply cov_cons. left. unfold inside; cbn. zl.
      * pose proof (live_cov_below _ _ _ x Hlv Hlx). zl.
    + intros x Hc Hlx. apply cov_cons in Hlx as [Hlx | Hlx].
      * pose proof (cov_below l' p x
          ltac:(unfold ends_below; eapply Forall_impl; [ | exact Hf ]; intros a Ha; cbn; zl) Hc).
        unfold inside in Hlx; cbn in Hlx. zl.
      * apply (Hfl x); [ | exact Hlx ]. rewrite Hl. apply cov_app. left. exact Hc.
  - split; [ split; [ exact Hg | exact Hlo ] | split; [ | split; [ | split ] ] ].
    + apply (ends_below_mono _ _ _ Heb). unfold MINSZ in Hw. zl.
    + constructor; [ cbn; unfold MINSZ in Hw; zl | ].
      eapply Forall_impl; [ | exact Hlv ]. intros a Ha. cbn. zl.
    + cbn. split; [ | exact Hpd ].
      apply Forall_disj_of_cov; [ cbn; unfold MINSZ in Hw; zl | exact Hlvpos | ].
      intros x Hx Hlx. unfold inside in Hx; cbn in Hx.
      pose proof (live_cov_below _ _ _ x Hlv Hlx). zl.
    + intros x Hc Hlx. apply cov_cons in Hlx as [Hlx | Hlx].
      * pose proof (cov_below _ _ _ Heb Hc). unfold inside in Hlx; cbn in Hlx. zl.
      * exact (Hfl x Hc Hlx).
Qed.

(* The headline: a large allocation never hands out an extent that overlaps
   a live one — reuse-after-free, at extent granularity, cannot happen. *)
Corollary alloc_disjoint_live : forall lo st w,
  LINV lo st -> MINSZ <= w -> lo <= heap st ->
  match live (snd (lalloc st w)) with
  | e :: _ => Forall (disj e) (live st)
  | [] => False
  end.
Proof.
  intros lo st w Hinv Hw Hh.
  pose proof (alloc_preserves_LINV lo st w Hinv Hw Hh) as [_ [_ [_ [Hpd _]]]].
  unfold lalloc in *. destruct (take (fl st) w (heap st)); cbn in *; destruct Hpd as [H _]; exact H.
Qed.

(* And the bound the design is for: the frontier moves only on a miss or an
   extension, never on a hit — a free-list hit reuses memory in place. *)
Remark hit_keeps_frontier : forall st w q z l',
  take (fl st) w (heap st) = TFound q z l' -> heap (snd (lalloc st w)) = heap st.
Proof. intros st w q z l' H. unfold lalloc. rewrite H. reflexivity. Qed.

(* ══ ALIGNMENT ════════════════════════════════════════════════════════
   Every block total is 4-aligned (`(cap + 15) & -4`) and so is every
   base the runtime produces; merges add and splits subtract aligned
   sizes, so the free list stays aligned. The composition (StructuralRun)
   needs it to read the bump's `(base + 12 + len + 3) & -4` as base + want. *)

Definition al4 (e : ext) : Prop := fst e mod 4 = 0 /\ snd e mod 4 = 0.

Lemma al4_sum : forall a b c, a mod 4 = 0 -> b mod 4 = 0 -> c mod 4 = 0 -> al4 (a, b + c).
Proof.
  intros a b c Ha Hb Hc. split; [ exact Ha | cbn ].
  rewrite Z.add_mod by lia. rewrite Hb, Hc. reflexivity.
Qed.

Lemma merge_succ_al4 : forall e post, al4 e -> Forall al4 post -> Forall al4 (merge_succ e post).
Proof.
  intros [b t] post [Hb Ht] Hp. destruct post as [ | [q s] post' ]; cbn.
  - constructor; [ split; assumption | constructor ].
  - inversion Hp as [ | ? ? [Hq Hs] Hp' ]; subst. cbn in *.
    destruct (b + t =? q).
    + constructor; [ apply al4_sum; assumption | exact Hp' ].
    + constructor; [ split; assumption | exact Hp ].
Qed.

Lemma link_pred_al4 : forall x l, al4 x -> Forall al4 l -> Forall al4 (link_pred x l).
Proof.
  intros [b t] l [Hb Ht] Hl. destruct l as [ | [q s] r ]; cbn.
  - constructor; [ split; assumption | constructor ].
  - inversion Hl as [ | ? ? [Hq Hs] Hr ]; subst. cbn in *.
    destruct (b + t =? q).
    + constructor; [ apply al4_sum; assumption | exact Hr ].
    + constructor; [ split; assumption | exact Hl ].
Qed.

Lemma ins_al4 : forall l e, Forall al4 l -> al4 e -> Forall al4 (ins l e).
Proof.
  induction l as [ | x r IH ]; intros e Hl He; cbn [ins].
  - constructor; [ exact He | constructor ].
  - inversion Hl as [ | ? ? Hx Hr ]; subst.
    destruct (fst x <? fst e).
    + destruct r as [ | y r' ].
      * apply link_pred_al4; [ exact Hx | apply merge_succ_al4; [ exact He | constructor ] ].
      * destruct (fst y <? fst e).
        -- constructor; [ exact Hx | apply IH; assumption ].
        -- apply link_pred_al4; [ exact Hx | apply merge_succ_al4; assumption ].
    + apply merge_succ_al4; assumption.
Qed.

Lemma take_al4 : forall l w h,
  Forall al4 l -> w mod 4 = 0 ->
  match take l w h with
  | TFound q z l' => al4 (q, z) /\ Forall al4 l'
  | TExtend p l' => p mod 4 = 0 /\ Forall al4 l'
  | TMiss => True
  end.
Proof.
  induction l as [ | [q z] r IH ]; intros w h Hl Hw; cbn [take]; [ exact I | ].
  inversion Hl as [ | ? ? [Hq Hz] Hr ]; subst. cbn in Hq, Hz.
  destruct (z <? w).
  - destruct r as [ | e r' ].
    + destruct (q + z =? h); [ split; [ exact Hq | constructor ] | exact I ].
    + specialize (IH w h Hr Hw).
      destruct (take (e :: r') w h) as [ a b l' | p l' | ]; [ | | exact I ].
      * destruct IH as [Hab Hl']. split; [ exact Hab | constructor; [ split; assumption | exact Hl' ] ].
      * destruct IH as [Hp Hl']. split; [ exact Hp | constructor; [ split; assumption | exact Hl' ] ].
  - destruct (SPLIT <=? z - w).
    + split; [ split; assumption | ].
      constructor; [ | exact Hr ]. split; cbn.
      * rewrite Z.add_mod by lia. rewrite Hq, Hw. reflexivity.
      * rewrite Zminus_mod. rewrite Hz, Hw. reflexivity.
    + split; [ split; assumption | exact Hr ].
Qed.
