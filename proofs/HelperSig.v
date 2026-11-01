(* Almide trust spine — PER-HELPER / PER-WINDOW RC SIGNATURES (#4278, B1).

   A runtime helper (a function of the shipped module: `$list_push`,
   `$bytes_push`, `$str_append`, …) and a compound routine or stdlib arm
   window (an instruction window inlined into a frame) each DECLARE, per heap
   parameter, one mode that holds on EVERY path:

     b (borrow, 0)  — the parameter enters with no owned reference and its
                      stream nets 0 (Swift `borrowing`, Lean `@&`, Koka `^`);
     c (consume, 1) — the parameter enters with the caller's one reference
                      and its stream releases, stores or returns it on every
                      path (Swift `consuming`, Lean / Koka owned).

   The modes are CallModes' `MBorrow` / `MMove`: a signature is the same
   object a table function's call-mode signature is, and the caller's side is
   the same agreement check (`check_modes_cert`).

   What this file adds:

   - `check_sig_cert`, the extracted per-build checker of a helper's OWN
     certificate: the declared modes, then one ownership line per heap
     parameter (the helper body's recorded events on it, in the full
     exit- and abort-aware line alphabet of `check_xc`), then any further
     lines (the helper's result, its temporaries, the elements it shares),
     which are checked as ordinary owned lines. A parameter line is judged
     from the entry count its mode declares (`seed`): a consumed one starts
     at 1 and must reach 0, a borrowed one starts at 0 and must return to 0.
     A signature that lies about its body — `b` on a parameter the body
     releases, `c` on one it only reads — is REJECTED.

   - `check_sig_cert_sound`: acceptance means every concrete run of each
     parameter line is a `param_stream_ok` body for its declared mode — the
     exact premise CallModes' `F_call` needs to inline it at a call site.

   - `window_fill_sound`: the composition law lifted from flat caller streams
     to caller LINES (loops, filters, one-shot and returning branches): a
     caller line, call sites abstracted to their mode markers, that the
     ownership checker accepts stays double-free- and leak-free in every
     concrete run under every inlining of mode-satisfying callee streams.
     A window inlined into a frame is such a call site, so a window whose own
     events satisfy its declared modes is accounted at the frame by its
     markers alone. *)

From Stdlib Require Import List.
Import ListNotations.
From Stdlib Require Import ZArith.
From Stdlib Require Import Arith.
From Stdlib Require Import Bool.
From Stdlib Require Import String Ascii.
From Stdlib Require Import Lia.
From AlmideTrust Require Import Subset.
From AlmideTrust Require Import OwnershipChecker.
From AlmideTrust Require Import CallModes.
Open Scope Z_scope.

(* ─── reuse-freedom of a line ─── *)

Definition item_no_reuse (c : CertItem) : bool :=
  match c with
  | COp o => no_reuse [o]
  | CLoop b => no_reuse b
  | CCondLoop t e => andb (no_reuse t) (no_reuse e)
  | CBranch t e => andb (no_reuse t) (no_reuse e)
  | CBranchRet _ t e => andb (no_reuse t) (no_reuse e)
  | CBranchAbort _ t e => andb (no_reuse t) (no_reuse e)
  | CPoison => false
  end.

Fixpoint line_no_reuse (cs : list CertItem) : bool :=
  match cs with
  | [] => true
  | c :: r => andb (item_no_reuse c) (line_no_reuse r)
  end.

Lemma no_reuse_app :
  forall a b, no_reuse (a ++ b) = andb (no_reuse a) (no_reuse b).
Proof.
  induction a as [| o a IH]; intros b; simpl; [reflexivity |].
  destruct o; simpl; try reflexivity; apply IH.
Qed.

Lemma no_reuse_concat_repeat :
  forall body n, no_reuse body = true -> no_reuse (List.concat (List.repeat body n)) = true.
Proof.
  intros body n H. induction n as [| n IH]; simpl; [reflexivity |].
  rewrite no_reuse_app, H, IH. reflexivity.
Qed.

Lemma no_reuse_cond_concat :
  forall t e bs, no_reuse t = true -> no_reuse e = true -> no_reuse (cond_concat t e bs) = true.
Proof.
  intros t e bs Ht He. induction bs as [| b bs IH]; [reflexivity |].
  destruct b; simpl; rewrite no_reuse_app, IH; rewrite ?Ht, ?He; reflexivity.
Qed.

(* Every concrete run of a reuse-free line is reuse-free. *)
Lemma line_no_reuse_unroll :
  forall cs fops, line_no_reuse cs = true -> UnrollsL cs fops -> no_reuse fops = true.
Proof.
  intros cs fops H HU. induction HU; simpl in H.
  - reflexivity.
  - apply andb_prop in H. destruct H as [H1 H2].
    destruct o; simpl in H1 |- *; try discriminate; apply IHHU; exact H2.
  - apply andb_prop in H. destruct H as [H1 H2].
    rewrite no_reuse_app, (no_reuse_concat_repeat body n H1), (IHHU H2). reflexivity.
  - apply andb_prop in H. destruct H as [H1 H2]. apply andb_prop in H1. destruct H1 as [Ht He].
    rewrite no_reuse_app, (no_reuse_cond_concat thenb elseb bs Ht He), (IHHU H2). reflexivity.
  - apply andb_prop in H. destruct H as [H1 H2]. apply andb_prop in H1. destruct H1 as [Ht He].
    rewrite no_reuse_app, (IHHU H2). destruct choice; simpl; rewrite ?Ht, ?He; reflexivity.
  - apply andb_prop in H. destruct H as [H1 _]. apply andb_prop in H1. destruct H1 as [Ht He].
    destruct retthen; simpl; assumption.
  - apply andb_prop in H. destruct H as [H1 H2]. apply andb_prop in H1. destruct H1 as [Ht He].
    rewrite no_reuse_app, (IHHU H2). destruct retthen; simpl; rewrite ?Ht, ?He; reflexivity.
  - apply andb_prop in H. destruct H as [H1 H2]. apply andb_prop in H1. destruct H1 as [Ht He].
    rewrite no_reuse_app, (IHHU H2). destruct abthen; simpl; rewrite ?Ht, ?He; reflexivity.
Qed.

(* ─── a parameter line, judged from its declared entry count ─── *)

(* The declared entry count as a line prefix: a consumed parameter starts
   holding the one reference its caller handed over. *)
Definition seed (m : Mode) (cs : list CertItem) : list CertItem :=
  match m with
  | MBorrow => cs
  | MMove => COp Inc :: cs
  end.

Definition param_line_ok (m : Mode) (cs : list CertItem) : bool :=
  andb (line_no_reuse cs) (check_line (seed m cs)).

(* THE CALLEE-SIDE OBLIGATION, per run: every concrete run of an accepted
   parameter line is a `param_stream_ok` body — the premise of CallModes'
   `F_call`, so the line's helper may be inlined at any call site that used
   the declared mode. *)
Theorem param_line_unroll_ok :
  forall m cs body, param_line_ok m cs = true -> UnrollsL cs body ->
    param_stream_ok m body = true.
Proof.
  intros m cs body H HU. unfold param_line_ok in H.
  apply andb_prop in H. destruct H as [Hnr Hck].
  unfold param_stream_ok. rewrite (line_no_reuse_unroll cs body Hnr HU). simpl.
  destruct m; unfold seed in Hck.
  - destruct (check_line_unroll_sound cs Hck body HU) as [_ Hr].
    unfold run in Hr. simpl. rewrite Hr. reflexivity.
  - assert (HU' : UnrollsL (COp Inc :: cs) (Inc :: body)) by (apply UL_op; exact HU).
    destruct (check_line_unroll_sound _ Hck _ HU') as [_ Hr].
    unfold run in Hr. simpl in Hr. simpl. rewrite Hr. reflexivity.
Qed.

(* ─── the composition law over caller LINES ─── *)

(* A caller line whose concrete run, call sites reduced to their markers,
   is a run of an ACCEPTED line: every inlining of mode-satisfying callee
   streams into that run is free of double-free and leak. *)
Theorem window_fill_sound :
  forall cs l fops,
    check_line cs = true -> UnrollsL cs (abstract l) -> FillsTo l fops ->
    no_double_free fops /\ no_leak fops.
Proof.
  intros cs l fops Hck HU HF.
  destruct (check_line_unroll_sound cs Hck (abstract l) HU) as [_ Hr].
  unfold run in Hr.
  pose proof (exec_fill l fops HF 0 0 (Z.le_refl 0) Hr) as E.
  unfold no_double_free, no_leak, run. rewrite E.
  split; [discriminate | reflexivity].
Qed.

(* ─── the extracted per-build checker of a helper's own certificate ───
   Witness format:

     <mode_0> <mode_1> … | <line for param 0> \n <line for param 1> \n … \n <other lines>

   modes are CallModes' nat ids (0 = borrow, 1 = consume), one per heap
   parameter in parameter order; the lines use the ownership alphabet and
   its loop / filter / branch / exit / abort delimiters (`check_xc`). *)

Fixpoint sig_lines_ok (ms : list nat) (ls : list (list CertItem)) : bool :=
  match ms, ls with
  | [], rest => forallb (fun cs => check_line (guard_line cs)) rest
  | m :: ms', cs :: ls' => andb (param_line_ok (mode_of_nat m) cs) (sig_lines_ok ms' ls')
  | _ :: _, [] => false
  end.

Definition parse_sig (s : string) : list nat * list (list CertItem) :=
  let (l, r) := split_bar s EmptyString in
  (pnats l None [], parse_xc r [] None None None).

Definition check_sig_cert (s : string) : bool :=
  let (ms, ls) := parse_sig s in andb (sig_wf ms) (sig_lines_ok ms ls).

Lemma sig_lines_nth :
  forall ms ls j m cs, sig_lines_ok ms ls = true ->
    nth_error ms j = Some m -> nth_error ls j = Some cs ->
    param_line_ok (mode_of_nat m) cs = true.
Proof.
  induction ms as [| m0 ms IH]; intros ls j m cs H Hm Hc.
  - destruct j; discriminate.
  - destruct ls as [| c0 ls]; [discriminate |]. simpl in H.
    apply andb_prop in H. destruct H as [H0 H1].
    destruct j as [| j]; simpl in Hm, Hc.
    + injection Hm as <-. injection Hc as <-. exact H0.
    + exact (IH ls j m cs H1 Hm Hc).
Qed.

Lemma sig_lines_rest :
  forall ms ls, sig_lines_ok ms ls = true ->
    forall cs, In cs (skipn (List.length ms) ls) -> check_line cs = true.
Proof.
  induction ms as [| m0 ms IH]; intros ls H cs Hin.
  - simpl in *. rewrite forallb_forall in H. exact (check_line_guard_weaken cs (H cs Hin)).
  - destruct ls as [| c0 ls]; [discriminate |]. simpl in H, Hin.
    apply andb_prop in H. destruct H as [_ H1]. exact (IH ls H1 cs Hin).
Qed.

(* SOUNDNESS over witness bytes: an accepted helper certificate declares
   well-formed modes, every concrete run of each parameter's line satisfies
   its declared mode, and every other line is an accepted ownership line. *)
Theorem check_sig_cert_sound :
  forall s, check_sig_cert s = true ->
    sig_wf (fst (parse_sig s)) = true /\
    (forall j m cs, nth_error (fst (parse_sig s)) j = Some m ->
       nth_error (snd (parse_sig s)) j = Some cs ->
       forall body, UnrollsL cs body -> param_stream_ok (mode_of_nat m) body = true) /\
    (forall cs, In cs (skipn (List.length (fst (parse_sig s))) (snd (parse_sig s))) ->
       forall fops, UnrollsL cs fops -> run fops <> None /\ run fops = Some 0).
Proof.
  intros s H. unfold check_sig_cert in H.
  destruct (parse_sig s) as [ms ls] eqn:E. simpl.
  apply andb_prop in H. destruct H as [Hwf Hl].
  split; [exact Hwf | split].
  - intros j m cs Hm Hc body HU.
    exact (param_line_unroll_ok _ cs body (sig_lines_nth ms ls j m cs Hl Hm Hc) HU).
  - intros cs Hin fops HU.
    exact (check_line_unroll_sound cs (sig_lines_rest ms ls Hl cs Hin) fops HU).
Qed.

(* non-vacuous, on witness bytes. *)

(* `$list_push`'s shape: the list is consumed (returned on the fast path,
   released on the grow path), the pushed value consumed (stored). *)
Example sig_list_push_accepts : check_sig_cert "1 1|{mx|}d
m" = true.
Proof. reflexivity. Qed.

(* The same body declared as BORROWING the list — the lie is refused: the
   fast path moves out a reference the parameter never held. *)
Example sig_list_push_borrow_lie_rejects : check_sig_cert "0 1|{mx|}d
m" = false.
Proof. reflexivity. Qed.

(* A helper that only reads its argument declared as CONSUMING it: the
   caller's reference is never released — refused (a leak). *)
Example sig_read_consume_lie_rejects : check_sig_cert "1|" = false.
Proof. reflexivity. Qed.

(* A borrowing reader: nothing on its line — accepted. *)
Example sig_read_borrow_accepts : check_sig_cert "0|" = true.
Proof. reflexivity. Qed.

(* A consumed parameter released on one path only — refused. *)
Example sig_some_paths_rejects : check_sig_cert "1|{d|}" = false.
Proof. reflexivity. Qed.

(* A further line (a fresh result moved out) is an ordinary owned line. *)
Example sig_result_line_accepts : check_sig_cert "0|
im" = true.
Proof. reflexivity. Qed.
Example sig_result_leak_rejects : check_sig_cert "0|
i" = false.
Proof. reflexivity. Qed.

(* A share-out on a borrowed parameter (+0 on its line). *)
Example sig_share_out_accepts : check_sig_cert "0|s" = true.
Proof. reflexivity. Qed.

(* A junk mode id — refused. *)
Example sig_junk_mode_rejects : check_sig_cert "2|d" = false.
Proof. reflexivity. Qed.

(* Fewer lines than declared parameters — refused. *)
Example sig_missing_line_rejects : check_sig_cert "1 1|d" = false.
Proof. reflexivity. Qed.

(* AXIOM AUDIT. *)
Print Assumptions param_line_unroll_ok.
Print Assumptions window_fill_sound.
Print Assumptions check_sig_cert_sound.
