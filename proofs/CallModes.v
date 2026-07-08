(* Almide v1 trust spine — CALL-MODE SIGNATURES (certificate format v1, brick 2c:
   ownership param-modes).

   The v1 calling convention fixes heap params as BORROWED (caller keeps its
   reference; the callee's cert starts the param at 0 owned refs) and heap
   returns as MOVED-OUT. Those conventions were ASSUMED to agree across the
   call boundary — nothing in the certificate pinned that the CALLER's
   treatment of a call site matches what the CALLEE's cert assumed.

   THE HOLE (why per-function balance alone is not compositional): let the
   callee treat a param as MOVE (its cert starts the param at rc 1 and releases
   it — balanced) while the caller treats the same call as BORROW (emits no
   event, later drops its ref — balanced). BOTH per-function certs ACCEPT; the
   inlined truth is a DOUBLE-FREE (`disagreement_double_frees` below). The fix
   is a SIGNATURE section: each function declares a per-heap-param MODE
   (borrow | move); each call site records the modes it actually used; the
   checker verifies positional agreement — it NEVER opens the callee
   (certificate-format-v1 §3; the checker-size invariant holds: one nat-list
   equality per call site).

   This file proves BOTH halves:
   - `check_fill_sound` — the COMPOSITION LAW: an accepted caller stream (call
     sites abstracted to their declared-mode markers: borrow = no event,
     move = `m`) stays double-free-free and leak-free under EVERY inlining of
     callee bodies that satisfy their declared modes (`param_stream_ok`). This
     is WHY checking signatures instead of opening callees is sound.
   - `check_modes_cert_sound` — the extracted per-build CHECKER: parsing the
     signature/call-site witness and accepting guarantees every call site's
     actual modes EQUAL the callee's declared signature (unknown callee =
     conservative reject).
   Built on the shared parser shapes (Subset.v) and the production ownership
   alphabet (OwnershipChecker.v). *)

From Stdlib Require Import List.
Import ListNotations.
From Stdlib Require Import ZArith.
From Stdlib Require Import Arith.
From Stdlib Require Import Bool.
From Stdlib Require Import String Ascii.
From Stdlib Require Import Lia.
From AlmideTrust Require Import Subset.
From AlmideTrust Require Import OwnershipChecker.
Open Scope Z_scope.

(* ─── the mode alphabet ─── *)

Inductive Mode : Type := MBorrow | MMove.

(* The callee-side ENTRY count the mode declares: a BORROWED param starts with
   NO owned reference (the caller keeps it — today's convention, and exactly
   how verify_ownership seeds params); a MOVED param starts with ONE (the
   caller transferred its reference). *)
Definition minit (m : Mode) : Z := match m with MBorrow => 0 | MMove => 1 end.

(* The caller-side MARKER the mode induces at the call site: a borrow leaves
   the caller's count untouched (no event); a move is the caller's `m`. *)
Definition marker (m : Mode) : list Op := match m with MBorrow => [] | MMove => [MoveOut] end.

(* Reuse-freedom of the callee's param stream. A `Reuse` pins rc = 1 EXACTLY,
   so a stream containing one is NOT shift-invariant: it could pass the callee's
   own check yet fault when inlined at a caller holding extra aliases. It is
   also semantically right to forbid: reusing a param the caller may still
   alias in place is precisely the corruption the modes exist to prevent.
   (Reuse of moved-in params under proven call-site uniqueness is a later,
   perceus-mode refinement.) *)
Fixpoint no_reuse (ops : list Op) : bool :=
  match ops with
  | [] => true
  | Reuse :: _ => false
  | _ :: rest => no_reuse rest
  end.

(* THE CALLEE-SIDE OBLIGATION: its event stream on one heap param, from the
   declared entry count, runs to 0 without faulting — and is Reuse-free. For
   BORROW this is today's rule verbatim (params seeded at 0: any release
   without a prior Dup faults); for MOVE the callee owns exactly the one
   transferred reference and must release it. Decidable — one fold. *)
Definition param_stream_ok (m : Mode) (body : list Op) : bool :=
  andb (no_reuse body)
       (match exec body (minit m) with Some z => Z.eqb z 0 | None => false end).

(* ─── the composition law ─── *)

(* A Reuse-free stream is SHIFT-INVARIANT above the fault floor: executing at
   `k` more owned references than checked cannot fault (the −1 guards only
   compare against 0) and lands exactly `k` higher. This is what lets ONE
   callee-side check (at the declared entry count) justify EVERY call site,
   whatever else the caller holds. *)
Lemma exec_shift :
  forall body r r' k, no_reuse body = true -> 0 <= k ->
    exec body r = Some r' -> exec body (r + k) = Some (r' + k).
Proof.
  induction body as [| o rest IH]; intros r r' k Hnr Hk Hex; simpl in *.
  - injection Hex as <-. reflexivity.
  - destruct o; simpl in *.
    + (* Inc *) replace (r + k + 1) with (r + 1 + k) by lia. apply IH; auto.
    + (* Alias *) replace (r + k + 1) with (r + 1 + k) by lia. apply IH; auto.
    + (* Dec *)
      destruct (r <=? 0) eqn:E; [discriminate|].
      apply Z.leb_gt in E.
      assert (Eg : (r + k <=? 0) = false) by (apply Z.leb_gt; lia).
      rewrite Eg.
      replace (r + k - 1) with (r - 1 + k) by lia. apply IH; auto.
    + (* MoveOut *)
      destruct (r <=? 0) eqn:E; [discriminate|].
      apply Z.leb_gt in E.
      assert (Eg : (r + k <=? 0) = false) by (apply Z.leb_gt; lia).
      rewrite Eg.
      replace (r + k - 1) with (r - 1 + k) by lia. apply IH; auto.
    + (* Reuse *) discriminate Hnr.
    + (* Borrow — shift-safe: the liveness guard rc > 0 only gets EASIER with
         k more references, and the count is unchanged. *)
      destruct (r <=? 0) eqn:E; [discriminate|].
      apply Z.leb_gt in E.
      assert (Eg : (r + k <=? 0) = false) by (apply Z.leb_gt; lia).
      rewrite Eg. apply IH; auto.
Qed.

(* An abstract caller line: plain ops, plus call sites carrying their mode. *)
Inductive MItem : Type :=
  | MOp : Op -> MItem
  | MCall : Mode -> MItem.

(* The caller's cert view of the line: each call site contributes exactly its
   mode's marker. This is what the (already-proven) ownership checker runs on. *)
Fixpoint abstract (l : list MItem) : list Op :=
  match l with
  | [] => []
  | MOp o :: rest => o :: abstract rest
  | MCall m :: rest => marker m ++ abstract rest
  end.

(* The INLINING relation: each call site is replaced by SOME callee stream
   satisfying the site's mode. This is the concrete whole-program truth the
   abstract line stands for. *)
Inductive FillsTo : list MItem -> list Op -> Prop :=
  | F_nil : FillsTo [] []
  | F_op : forall o a b, FillsTo a b -> FillsTo (MOp o :: a) (o :: b)
  | F_call : forall m body a b,
      param_stream_ok m body = true ->
      FillsTo a b -> FillsTo (MCall m :: a) (body ++ b).

(* SOUNDNESS CORE: an abstract line that executes to `res` (from any
   non-negative count) executes EVERY inlining to the same `res` — marker and
   callee stream have the same net effect, and shift-invariance absorbs the
   caller's surplus references. *)
Lemma exec_fill :
  forall l fops, FillsTo l fops ->
    forall r res, 0 <= r ->
      exec (abstract l) r = Some res -> exec fops r = Some res.
Proof.
  intros l fops HF. induction HF; intros r res Hr Hex.
  - (* nil *) simpl in *. exact Hex.
  - (* plain op — case on it so exec reduces on both sides *)
    destruct o; simpl in *.
    + apply IHHF; [lia | exact Hex].
    + apply IHHF; [lia | exact Hex].
    + destruct (r <=? 0) eqn:E; [discriminate|]. apply Z.leb_gt in E.
      apply IHHF; [lia | exact Hex].
    + destruct (r <=? 0) eqn:E; [discriminate|]. apply Z.leb_gt in E.
      apply IHHF; [lia | exact Hex].
    + destruct (Z.eqb r 1) eqn:E; [|discriminate]. apply Z.eqb_eq in E.
      apply IHHF; [lia | exact Hex].
    + destruct (r <=? 0) eqn:E; [discriminate|]. apply Z.leb_gt in E.
      apply IHHF; [lia | exact Hex].
  - (* call site: split the mode *)
    unfold param_stream_ok in H. apply andb_prop in H. destruct H as [Hnr Hend].
    destruct (exec body (minit m)) as [z |] eqn:Eb; [| discriminate].
    apply Z.eqb_eq in Hend. subst z.
    rewrite exec_app.
    destruct m; simpl in Hex; simpl in Eb.
    + (* BORROW: marker = [] — the callee nets 0 from the caller's count. *)
      assert (Hb : exec body (0 + r) = Some (0 + r)) by (apply exec_shift; auto).
      replace (0 + r) with r in Hb by lia.
      rewrite Hb. apply IHHF; [exact Hr | exact Hex].
    + (* MOVE: marker = [m] — the abstract guard gives r ≥ 1; the callee nets
         −1 from the caller's count, exactly the marker's effect. *)
      destruct (r <=? 0) eqn:E; [discriminate|]. apply Z.leb_gt in E.
      assert (Hb : exec body (1 + (r - 1)) = Some (0 + (r - 1))).
      { apply exec_shift; auto. lia. }
      replace (1 + (r - 1)) with r in Hb by lia.
      replace (0 + (r - 1)) with (r - 1) in Hb by lia.
      rewrite Hb. apply IHHF; [lia | exact Hex].
Qed.

(* THE COMPOSITION LAW (headline): if the caller's abstract line — call sites
   reduced to their declared-mode markers — passes the proven ownership check,
   then EVERY inlining of mode-satisfying callee streams is free of double-free
   and leak. Checking signatures at call sites (never opening the callee) is
   therefore sound: per-function certs + mode agreement compose. *)
Theorem check_fill_sound :
  forall l, check (abstract l) = true ->
    forall fops, FillsTo l fops ->
      no_double_free fops /\ no_leak fops.
Proof.
  intros l H fops HF.
  unfold check, run in H.
  destruct (exec (abstract l) 0) as [z |] eqn:E; [| discriminate].
  apply Z.eqb_eq in H. subst z.
  unfold no_double_free, no_leak, run.
  rewrite (exec_fill l fops HF 0 0 (Z.le_refl 0) E).
  split; [discriminate | reflexivity].
Qed.

(* non-vacuous: the two conventions, end-to-end through the composition law. *)

(* BORROW: caller `i () d`; callee aliases and releases (`ad`, nets 0). *)
Example borrow_abstract_accepts :
  check (abstract [MOp Inc; MCall MBorrow; MOp Dec]) = true.
Proof. reflexivity. Qed.
Example borrow_body_ok : param_stream_ok MBorrow [Alias; Dec] = true.
Proof. reflexivity. Qed.
Example borrow_fill :
  FillsTo [MOp Inc; MCall MBorrow; MOp Dec] [Inc; Alias; Dec; Dec].
Proof.
  apply F_op.
  apply (F_call MBorrow [Alias; Dec] [MOp Dec] [Dec]).
  - reflexivity.
  - apply F_op. apply F_nil.
Qed.
Example borrow_inlined_accepts : check [Inc; Alias; Dec; Dec] = true.
Proof. reflexivity. Qed.

(* MOVE: caller `i m`; callee owns the transferred ref and releases it (`d`). *)
Example move_abstract_accepts :
  check (abstract [MOp Inc; MCall MMove]) = true.
Proof. reflexivity. Qed.
Example move_body_ok : param_stream_ok MMove [Dec] = true.
Proof. reflexivity. Qed.
Example move_inlined_accepts : check [Inc; Dec] = true.
Proof. reflexivity. Qed.

(* THE CLOSED HOLE (non-vacuous): `[Dec]` is a fine MOVE body but NOT a BORROW
   body — and a caller that treats that call as BORROW (abstract `i () d`,
   balanced, ACCEPTED in isolation) inlines to a DOUBLE-FREE. Mode agreement is
   exactly what forbids pairing a borrow-marker caller with a move-mode callee. *)
Example callee_move_body_not_borrow : param_stream_ok MBorrow [Dec] = false.
Proof. reflexivity. Qed.
Example disagreement_double_frees : check [Inc; Dec; Dec] = false.
Proof. reflexivity. Qed.

(* Reuse-freedom is load-bearing (why `no_reuse` is in `param_stream_ok`):
   `[Inc; Reuse]` balances from 0, but at a caller holding an extra alias
   (entry 1) it FAULTS — reuse pins rc = 1 exactly, so it is not shift-safe. *)
Example reuse_body_not_shift_safe : exec [Inc; Reuse] 1 = None.
Proof. reflexivity. Qed.

(* `b` (Borrow, brick 5b) is shift-safe and composes: a callee that first
   acquires its own reference may witness borrows on it (`a b d` nets 0 from a
   borrowed param's entry 0). A BARE `b` on the zero-seeded param stream faults
   — which is exactly why plain param borrows stay event-free in the emitter:
   their liveness is the caller's obligation, discharged by the mode agreement,
   not by the callee's own count. *)
Example borrow_use_body_ok : param_stream_ok MBorrow [Alias; Borrow; Dec] = true.
Proof. reflexivity. Qed.
Example bare_borrow_body_needs_own_ref : param_stream_ok MBorrow [Borrow] = false.
Proof. reflexivity. Qed.

(* ─── the extracted per-build checker: signature/call-site agreement ───
   Witness format (all ground facts, nat mode ids: 0 = borrow, 1 = move):

     <sig_0>;<sig_1>;…;<sig_n> | <site_0>;<site_1>;…;<site_m>

   sig_k   = the space-separated heap-param modes of function k (line order;
             empty when it has no heap params).
   site_j  = `<callee index> <actual modes…>` — the modes the CALLER used at
             one call site.
   The checker: every declared mode is a known id (≤ 1), and every site's
   actual modes EQUAL its callee's declared signature, positionally — with an
   out-of-range callee a conservative REJECT (unknown callee, same discipline
   as the caps graph's universe node). One `|`-split, `;`-splits, and a
   nat-list equality per site: within the checker-size invariant. *)

Definition mode_of_nat (n : nat) : Mode :=
  match n with O => MBorrow | _ => MMove end.

Fixpoint nats_eqb (a b : list nat) : bool :=
  match a, b with
  | [], [] => true
  | x :: a', y :: b' => andb (Nat.eqb x y) (nats_eqb a' b')
  | _, _ => false
  end.

Lemma nats_eqb_eq : forall a b, nats_eqb a b = true -> a = b.
Proof.
  induction a as [| x a IH]; destruct b as [| y b]; simpl; try discriminate.
  - reflexivity.
  - intros H. apply andb_prop in H. destruct H as [H1 H2].
    apply Nat.eqb_eq in H1. f_equal; auto.
Qed.

Definition sig_wf (sig : list nat) : bool :=
  forallb (fun m => Nat.leb m 1) sig.

Definition site_ok (sigs : list (list nat)) (site : list nat) : bool :=
  match site with
  | (callee :: actual)%list =>
      andb (Nat.ltb callee (List.length sigs))
           (nats_eqb actual (List.nth callee sigs []))
  | [] => false
  end.

Definition modes_ok (sigs sites : list (list nat)) : bool :=
  andb (forallb sig_wf sigs) (forallb (site_ok sigs) sites).

(* parsing — the shared shapes: one `|` split, `;` segments, nat lists. *)
Definition parse_nat_lists (s : string) : list (list nat) :=
  List.map (fun seg => pnats seg None []) (split_semi s EmptyString []).

Definition nonempty (l : list nat) : bool :=
  negb (Nat.eqb (List.length l) O).

(* Signature positions are meaningful (function k = segment k) so empties are
   KEPT there (a function with no heap params); a site always names its callee,
   so an empty site segment is format noise and dropped (a program with no
   calls has an empty right side — vacuously accepted). *)
Definition parse_modes (s : string) : (list (list nat)) * (list (list nat)) :=
  let (l, r) := split_bar s EmptyString in
  (parse_nat_lists l, List.filter nonempty (parse_nat_lists r)).

Definition check_modes_cert (s : string) : bool :=
  let (sigs, sites) := parse_modes s in modes_ok sigs sites.

(* THE AGREEMENT PROPERTY a site satisfies when accepted. *)
Definition site_agrees (sigs : list (list nat)) (site : list nat) : Prop :=
  exists callee actual,
    site = (callee :: actual)%list /\
    (callee < List.length sigs)%nat /\
    actual = List.nth callee sigs [].

Lemma site_ok_agrees :
  forall sigs site, site_ok sigs site = true -> site_agrees sigs site.
Proof.
  intros sigs site H. destruct site as [| callee actual]; [discriminate|].
  simpl in H. apply andb_prop in H. destruct H as [Hlt Heq].
  apply Nat.ltb_lt in Hlt. apply nats_eqb_eq in Heq.
  exists callee, actual. auto.
Qed.

(* SOUNDNESS, end-to-end over witness bytes: acceptance guarantees EVERY parsed
   call site names an in-range callee and used EXACTLY its declared modes. With
   `check_fill_sound` this is the compositionality of the per-function
   ownership certs: agreed modes ⟹ the caller's markers match what each
   callee's cert assumed ⟹ every inlining is double-free- and leak-free. *)
Theorem check_modes_cert_sound :
  forall s, check_modes_cert s = true ->
    forall site, In site (snd (parse_modes s)) ->
      site_agrees (fst (parse_modes s)) site.
Proof.
  intros s H site Hin.
  unfold check_modes_cert in H.
  destruct (parse_modes s) as [sigs sites] eqn:E. simpl in *.
  unfold modes_ok in H. apply andb_prop in H. destruct H as [_ Hsites].
  rewrite forallb_forall in Hsites.
  apply site_ok_agrees. apply Hsites. exact Hin.
Qed.

(* non-vacuous, end-to-end on witness bytes. *)

(* fn0 declares [borrow; move], fn1 declares [move]; one site calls fn1 with
   [move] — agreement, ACCEPT. *)
Example modes_cert_agrees : check_modes_cert "0 1;1|1 1" = true.
Proof. reflexivity. Qed.

(* fn0 declares [borrow]; a site calls fn0 with [move] — REJECT (the exact
   pairing `disagreement_double_frees` shows is unsound). *)
Example modes_cert_mismatch : check_modes_cert "0|0 1" = false.
Proof. reflexivity. Qed.

(* arity mismatch is a length mismatch — REJECT. *)
Example modes_cert_arity : check_modes_cert "0 0|0 0" = false.
Proof. reflexivity. Qed.

(* unknown callee — conservative REJECT. *)
Example modes_cert_unknown_callee : check_modes_cert "0|5 0" = false.
Proof. reflexivity. Qed.

(* a junk mode id in a signature — REJECT (sig well-formedness). *)
Example modes_cert_junk_mode : check_modes_cert "7|0 7" = false.
Proof. reflexivity. Qed.

(* a program with functions but no call sites — vacuously ACCEPT (fn1 has no
   heap params: an empty signature segment). *)
Example modes_cert_no_calls : check_modes_cert "0 0;|" = true.
Proof. reflexivity. Qed.

(* AXIOM AUDIT: soundness rests on nothing but the kernel. *)
Print Assumptions check_fill_sound.
Print Assumptions check_modes_cert_sound.
