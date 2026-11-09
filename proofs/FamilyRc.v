(* Almide trust spine — ADR-0033 D1/D7: allocation FAMILIES are TEMPORAL.

   A family is one heap allocation (its ROOT, an ordinary block) holding
   many INTERIOR blocks, each with the ordinary layout of its type except
   its rc word: `0x8000_0000 | d`, d the byte distance back to the root.
   The runtime never counts an interior block on its own. The family
   variants of `$inc` / `$dec_flat` (StructuralRuntime.v, `*_family_*`)
   read the word, and on an INTERIOR word they act on the root instead:
   every inc and dec of a member is CREDITED to its root, and the family is
   freed — as the one root block — only when the root's count reaches 0.

   This file states that discipline over MemorySafe.v's heap traces and
   proves it TEMPORAL, extending `rc_check_gives_temporal`:

   - `credit fam off`: the event the runtime performs for a program event.
     An event on member b acts on the root `fam b` (a load or store at the
     member's offset `off b` inside the root's allocation); an allocation
     is an allocation (only roots are allocated — interior blocks are
     carved, never handed out by `$alloc`).
   - `fam_stream fam r t`: the ONE per-object op stream the RC checker
     sees for family r — the root's own events and every member's, merged
     in trace order (`credit_proj`: it is exactly the guarded projection
     of the credited trace onto r).
   - `family_rc_gives_temporal` (THE THEOREM): when the allocator hands
     out blocks that are not live and every family's merged stream is
     accepted by `OwnershipChecker.check` (with #3229's liveness guard),
     the credited trace is TEMPORAL: no member is touched after its family
     was freed, no family is freed twice, and no free resurrects.
   - `interior_never_counted`: under a well-formed family map, no credited
     event allocates, counts, frees or touches an interior block's own
     record — interior blocks are never freed on their own.
   - `member_dead_with_root`: once the root is not live, ANY event on ANY
     member is a TEMPORAL violation — the family dies as one.
   - The word ENCODING (`redirect`, `fam_of`): the root the runtime
     computes from an interior word is `b - d` (`redirect_interior`), an
     ordinary word leaves the block where it is (`redirect_ordinary`), and
     a memory whose interior words name ordinary roots induces a
     well-formed family map (`fam_of_wf`). `family_inc_is_credited` /
     `family_dec_is_credited` tie this to the EMITTED trees: the family
     `$inc` / `$dec_flat` act on exactly `redirect b (word b)`.
   - Negative controls, one per ADR-0033 drill this file can see:
     `redirect_dropped_not_temporal` (an interior dec on its own word),
     `root_off_by_4_not_temporal` (the encoded distance off by 4),
     `signed_cow_judge_reads_interior_unique` (a COW judge compared signed
     calls a family member unique; the unsigned judges never do,
     `unsigned_judges_read_interior_shared`).

   HONEST SCOPE.
   - Phase 2 families (json.parse) have no LINKS (references out of the
     family): releasing the family is the root's free. The link releases
     of ADR-0033 D4 are not modeled here; they need their own clause when
     D4 lands.
   - Like `rc_check_gives_temporal`, this is a theorem about traces: the
     per-object streams are the ownership certificate's lines, matched to
     calls (not located one by one), as today. What is new is that a
     member's events land on its ROOT's line, which the emitted trees do
     (StructuralRuntime.v) and this file proves sufficient.
   - The interior word's distance is fixed at carve time and never moves
     (the family is never reallocated, ADR-0033 D2); the proof takes the
     family map as a fixed function of the block. *)

(* StructuralRuntime first: MemorySafe's event constructors (ELoad …) must
   win over the tree language's expression constructors of the same name. *)
From AlmideTrust Require Import StructuralRuntime.
From AlmideTrust Require Import OwnershipChecker.
From AlmideTrust Require Import MemorySafe.
From Stdlib Require Import List ZArith Lia Bool Arith.
Import ListNotations.
Open Scope Z_scope.

(* ── Families ─────────────────────────────────────────────────────────── *)

(* The family map: every block's root. A root is its own root. *)
Definition Fam := Blk -> Blk.

Definition fam_wf (fam : Fam) : Prop := forall b, fam (fam b) = fam b.

(* The event the runtime performs for a program event. *)
Definition credit (fam : Fam) (off : Blk -> Z) (e : Ev) : Ev :=
  match e with
  | EAlloc b n k => EAlloc b n k
  | EStore b o w k => EStore (fam b) (off b + o) w k
  | ELoad b o w k => ELoad (fam b) (off b + o) w k
  | EInc b => EInc (fam b)
  | EDec b => EDec (fam b)
  end.

Definition credit_trace (fam : Fam) (off : Blk -> Z) (t : list Ev) : list Ev :=
  map (credit fam off) t.

(* The merged per-family op stream: the root's events and every member's,
   in trace order, each guarded as the production checker guards it. *)
Definition fam_ev (fam : Fam) (r : Blk) (e : Ev) : list Op :=
  match e with
  | EAlloc b _ _ => if Nat.eqb b r then [Inc] else []
  | EInc b => if Nat.eqb (fam b) r then [Alias] else []
  | EDec b => if Nat.eqb (fam b) r then [Dec] else []
  | EStore b _ _ _ | ELoad b _ _ _ => if Nat.eqb (fam b) r then [Borrow] else []
  end.

Definition fam_stream (fam : Fam) (r : Blk) (t : list Ev) : list Op :=
  flat_map (fun e => guard_ops (fam_ev fam r e)) t.

(* Crediting IS merging: the checker's view of the credited trace on r is
   family r's merged stream. *)
Lemma credit_proj : forall fam off r t,
  gproj r (credit_trace fam off t) = fam_stream fam r t.
Proof.
  intros fam off r t. induction t as [| e t IH]; [reflexivity |].
  unfold credit_trace, gproj, fam_stream in *. cbn [map flat_map]. rewrite IH.
  f_equal. destruct e; reflexivity.
Qed.
Print Assumptions credit_proj.

(* THE THEOREM. *)
Theorem family_rc_gives_temporal : forall fam off t,
  holds fresh_alloc_ev empty_heap (credit_trace fam off t) ->
  (forall r, check (fam_stream fam r t) = true) ->
  Temporal (credit_trace fam off t).
Proof.
  intros fam off t Hw Hc. apply rc_check_gives_temporal; [exact Hw |].
  intro r. rewrite credit_proj. apply Hc.
Qed.
Print Assumptions family_rc_gives_temporal.

(* ── Interior blocks are never counted, freed or touched on their own ── *)

Definition is_interior (fam : Fam) (b : Blk) : Prop := fam b <> b.

Lemma step_other : forall h e b, b <> ev_blk e -> step h e b = h b.
Proof.
  intros h e b Hne.
  destruct e as [b0 n k | b0 o w k | b0 o w k | b0 | b0]; simpl in *.
  - unfold set. destruct (Nat.eqb_spec b b0); [contradiction | reflexivity].
  - destruct (h b0); [| reflexivity].
    unfold set. destruct (Nat.eqb_spec b b0); [contradiction | reflexivity].
  - reflexivity.
  - destruct (h b0); [| reflexivity].
    unfold set. destruct (Nat.eqb_spec b b0); [contradiction | reflexivity].
  - destruct (h b0) as [x |]; [| reflexivity].
    destruct (0 <? rc x); [| reflexivity].
    unfold set. destruct (Nat.eqb_spec b b0); [contradiction | reflexivity].
Qed.

Definition run_heap (h : Heap) (t : list Ev) : Heap := fold_left step t h.

(* Only roots are allocated (interior blocks are carved inside a root). *)
Definition allocs_roots (fam : Fam) (t : list Ev) : Prop :=
  forall b n k, In (EAlloc b n k) t -> fam b = b.

Lemma credit_not_interior : forall fam off e b,
  fam_wf fam -> is_interior fam b ->
  (forall b0 n k, e = EAlloc b0 n k -> fam b0 = b0) ->
  b <> ev_blk (credit fam off e).
Proof.
  intros fam off e b Hwf Hi Ha Heq. unfold is_interior in Hi.
  destruct e as [b0 n k | b0 o w k | b0 o w k | b0 | b0]; simpl in Heq;
    subst b.
  - apply Hi. apply (Ha b0 n k). reflexivity.
  - apply Hi. apply Hwf.
  - apply Hi. apply Hwf.
  - apply Hi. apply Hwf.
  - apply Hi. apply Hwf.
Qed.

(* An interior block's heap record is exactly what it was before the
   credited trace ran — never allocated, counted, decremented or freed. *)
Theorem interior_never_counted : forall fam off t h b,
  fam_wf fam -> allocs_roots fam t -> is_interior fam b ->
  run_heap h (credit_trace fam off t) b = h b.
Proof.
  intros fam off t. induction t as [| e t IH]; intros h b Hwf Ha Hi;
    [reflexivity |].
  unfold credit_trace, run_heap in *. simpl.
  rewrite IH; [| exact Hwf | intros b0 n k Hin; apply (Ha b0 n k); right; exact Hin
               | exact Hi].
  apply step_other. apply (credit_not_interior fam off e b Hwf Hi).
  intros b0 n k ->. apply (Ha b0 n k). left. reflexivity.
Qed.
Print Assumptions interior_never_counted.

(* Once the root is not live, any event on any member violates TEMPORAL:
   the family dies as one block. *)
Theorem member_dead_with_root : forall fam off h e,
  (match e with EAlloc _ _ _ => False | _ => True end) ->
  ~ live h (fam (ev_blk e)) ->
  ~ temporal_ev h (credit fam off e).
Proof.
  intros fam off h e Hne Hd Ht.
  destruct e; simpl in *; [contradiction | | | |]; apply Hd; exact Ht.
Qed.
Print Assumptions member_dead_with_root.

(* ── The word encoding ────────────────────────────────────────────────── *)

(* The root the family trees compute from a block address and its word:
   `if (w <s 0) b - (w & 0x7fff_ffff) else b`. *)
Definition redirect (b w : Z) : Z :=
  if signed32 w <? 0 then b - Z.land w LOWMASK else b.

Lemma redirect_interior : forall b d,
  0 <= d < INTERIOR -> redirect b (INTERIOR + d) = b - d.
Proof.
  intros b d Hd. unfold redirect.
  rewrite (signed32_interior d Hd).
  unfold INTERIOR, LOWMASK in *.
  destruct (Z.ltb_spec (2147483648 + d - 4294967296) 0); [| lia].
  rewrite land_low by lia. reflexivity.
Qed.
Print Assumptions redirect_interior.

Lemma redirect_ordinary : forall b w,
  0 <= w < INTERIOR -> redirect b w = b.
Proof.
  intros b w Hw. unfold redirect.
  rewrite (signed32_ordinary w Hw).
  destruct (Z.ltb_spec w 0); [lia | reflexivity].
Qed.

(* The family map a memory of rc words induces (block b at address b). *)
Definition fam_of (m : Z -> Z) : Fam :=
  fun b => Z.to_nat (redirect (Z.of_nat b) (m (Z.of_nat b))).

(* A memory's words are well formed when every word is ordinary, or is
   interior with a distance of at least one header (12) whose target is an
   ordinary word (a root). *)
Definition words_wf (m : Z -> Z) : Prop :=
  forall b : Blk,
    (0 <= m (Z.of_nat b) < INTERIOR)
    \/ (exists d, 12 <= d < INTERIOR /\ d <= Z.of_nat b
                  /\ m (Z.of_nat b) = INTERIOR + d
                  /\ 0 <= m (Z.of_nat b - d) < INTERIOR).

Theorem fam_of_wf : forall m, words_wf m -> fam_wf (fam_of m).
Proof.
  intros m Hm b.
  assert (Hord : forall x : Blk, 0 <= m (Z.of_nat x) < INTERIOR -> fam_of m x = x).
  { intros x Hx. unfold fam_of. rewrite (redirect_ordinary _ _ Hx).
    apply Nat2Z.id. }
  destruct (Hm b) as [Ho | [d [Hd [Hle [Hw Hr]]]]].
  - rewrite (Hord b Ho). apply Hord. exact Ho.
  - assert (Hf : fam_of m b = Z.to_nat (Z.of_nat b - d)).
    { unfold fam_of. rewrite Hw. rewrite redirect_interior by lia. reflexivity. }
    rewrite Hf. apply Hord. rewrite Z2Nat.id by lia. exact Hr.
Qed.
Print Assumptions fam_of_wf.

(* An interior block is never its own root. *)
Lemma interior_word_is_interior : forall m b d,
  12 <= d < INTERIOR -> d <= Z.of_nat b -> m (Z.of_nat b) = INTERIOR + d ->
  is_interior (fam_of m) b.
Proof.
  intros m b d Hd Hle Hw Heq. unfold fam_of in Heq. rewrite Hw in Heq.
  rewrite redirect_interior in Heq by lia.
  apply (f_equal Z.of_nat) in Heq. rewrite Z2Nat.id in Heq by lia. lia.
Qed.

(* ── The emitted family trees act on `redirect b (word b)` ─────────────── *)

Theorem family_inc_is_credited : forall b floor c,
  floor <= b -> floor <= redirect b (cm c b) ->
  (0 <= cm c b < INTERIOR \/ exists d, 0 <= d < INTERIOR /\ cm c b = INTERIOR + d) ->
  run_inc_family b floor c = run_inc (redirect b (cm c b)) floor c.
Proof.
  intros b floor c Hf Hr [Ho | [d [Hd Hw]]].
  - rewrite (redirect_ordinary b (cm c b) Ho).
    apply inc_family_ordinary; [exact Hf | exact Ho].
  - rewrite Hw in *. rewrite redirect_interior in * by exact Hd.
    apply inc_family_interior; assumption.
Qed.
Print Assumptions family_inc_is_credited.

Theorem family_dec_is_credited : forall b floor fbase c,
  floor <= b -> floor <= redirect b (cm c b) ->
  (0 <= cm c b < INTERIOR \/ exists d, 0 <= d < INTERIOR /\ cm c b = INTERIOR + d) ->
  run_dec_family b floor fbase c = run_dec (redirect b (cm c b)) floor fbase c.
Proof.
  intros b floor fbase c Hf Hr [Ho | [d [Hd Hw]]].
  - rewrite (redirect_ordinary b (cm c b) Ho).
    apply dec_family_ordinary; [exact Hf | exact Ho].
  - rewrite Hw in *. rewrite redirect_interior in * by exact Hd.
    apply dec_family_interior; assumption.
Qed.
Print Assumptions family_dec_is_credited.

(* ── Negative controls (the ADR-0033 drills this file sees) ────────────── *)

(* A family: root 0 (64 bytes), member 1 inside it at offset 16. *)
Definition fam1 : Fam := fun b => match b with 1%nat => 0%nat | _ => b end.
Definition off1 : Blk -> Z := fun b => match b with 1%nat => 16 | _ => 0 end.
Definition fam_id : Fam := fun b => b.

(* Share the member, release the share, release the document. *)
Definition t1 : list Ev :=
  [EAlloc 0%nat 64 r1; EInc 1%nat; EDec 1%nat; EDec 0%nat].

(* Credited to the root, the trace is TEMPORAL (by the theorem). *)
Example credited_member_trace_is_temporal : Temporal (credit_trace fam1 off1 t1).
Proof.
  apply family_rc_gives_temporal.
  - unfold credit_trace, t1. simpl. split; [intros [x [Hx _]]; cbv in Hx; discriminate |]. repeat split.
  - intros r. destruct r as [| [| r]]; reflexivity.
Qed.

(* `family-redirect-dropped`: the interior inc/dec acts on the member's
   own word. The member was never allocated as a block of its own, so the
   trace is not TEMPORAL. *)
Theorem redirect_dropped_not_temporal : ~ Temporal (credit_trace fam_id off1 t1).
Proof.
  unfold Temporal, credit_trace, t1. simpl. intros [_ [[x [Hx _]] _]].
  cbv in Hx. discriminate.
Qed.
Print Assumptions redirect_dropped_not_temporal.

(* `family-root-off-by-4`: the member at address 28 belongs to the root at
   address 12 (distance 16). Encoded right, the word names the root; with
   the distance off by 4 it names address 8, which is no allocated block,
   and the credited trace is not TEMPORAL. *)
Definition word_ok : Z -> Z := fun a => if a =? 28 then INTERIOR + 16 else 1.
Definition word_off4 : Z -> Z := fun a => if a =? 28 then INTERIOR + 20 else 1.

Example word_ok_names_root : fam_of word_ok 28%nat = 12%nat.
Proof. reflexivity. Qed.

Example word_off4_names_wrong_block : fam_of word_off4 28%nat = 8%nat.
Proof. reflexivity. Qed.

Definition t2 : list Ev :=
  [EAlloc 12%nat 64 r1; EInc 28%nat; EDec 28%nat; EDec 12%nat].

Theorem root_off_by_4_not_temporal :
  ~ Temporal (credit_trace (fam_of word_off4) (fun _ => 0) t2).
Proof.
  assert (Ht : credit_trace (fam_of word_off4) (fun _ => 0) t2
               = [EAlloc 12%nat 64 r1; EInc 8%nat; EDec 8%nat; EDec 12%nat])
    by reflexivity.
  rewrite Ht. unfold Temporal. simpl. intros [_ [[x [Hx _]] _]].
  cbv in Hx. discriminate.
Qed.
Print Assumptions root_off_by_4_not_temporal.

Example root_correct_is_temporal :
  credit_trace (fam_of word_ok) (fun _ => 0) t2
  = [EAlloc 12%nat 64 r1; EInc 12%nat; EDec 12%nat; EDec 12%nat].
Proof. reflexivity. Qed.

(* `cow-judge-reads-interior-unique`: every copy-on-write judge compares
   the rc word UNSIGNED against 1, and an interior word is never 1, nor at
   most 1, so a member is always copied before it is mutated. Compared
   SIGNED, the same word reads as negative, i.e. "at most 1": the judge
   would mutate a shared family member in place. *)
Theorem unsigned_judges_read_interior_shared : forall d,
  0 <= d < INTERIOR ->
  (INTERIOR + d =? 1) = false /\ (INTERIOR + d <=? 1) = false
  /\ (1 <? INTERIOR + d) = true.
Proof.
  intros d Hd. unfold INTERIOR in *.
  split; [apply Z.eqb_neq; lia |]. split; [apply Z.leb_gt; lia | apply Z.ltb_lt; lia].
Qed.
Print Assumptions unsigned_judges_read_interior_shared.

Theorem signed_cow_judge_reads_interior_unique : forall d,
  0 <= d < INTERIOR ->
  (signed32 (INTERIOR + d) <=? 1) = true.
Proof.
  intros d Hd.
  rewrite (signed32_interior d Hd).
  unfold INTERIOR in *. apply Z.leb_le. lia.
Qed.
Print Assumptions signed_cow_judge_reads_interior_unique.
