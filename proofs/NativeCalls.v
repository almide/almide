(* Almide trust spine — NATIVE CALL BINDING (#4006, class P: per-parameter
   move modes on the verified native render).

   The native render gives a heap parameter one of two modes, the modes of
   CallModes:

     borrow (0) — the caller keeps its reference; the callee's line on the
                  parameter starts at 0 and must return to 0;
     move   (1) — the caller hands its one reference over at the call; the
                  callee's line starts at 1 and must reach 0 (it returns,
                  stores or releases the value on every path).

   Two facts make per-function lines compose under these modes
   (CallModes.check_fill_sound, HelperSig.window_fill_sound):

   (a) AGREEMENT — every call site hands each heap argument over in the mode
       its callee declares (`site_ok`);
   (b) BINDING — every argument a site moves is a move-out event on the
       caller's own line for that argument, at the call: the caller's count
       loses the reference it handed over. A site that says "move" while the
       caller's line keeps the reference (a MISSING MOVE) would, inlined,
       release the one reference twice — and both functions' lines balance on
       their own.

   The call-mode witness of CallModes carries (a) only: its sites are a list
   apart from the ownership lines, so nothing ties a moved argument to an
   event. This checker carries both, in one witness for the whole program:
   one BLOCK per function, separated by `#`:

     <modes>|<sites>|<marks>\n<line 0>\n<line 1>\n…

     modes  the function's heap-parameter modes (space-separated nat ids,
            0 = borrow, 1 = move), in parameter order. The program's
            signature table is the list of every block's modes — read off the
            blocks themselves, so a caller and its callee cannot be judged
            against two different tables.
     sites  the function's call sites, `;`-separated, each
            `<callee block index> <actual modes…>` (CallModes' site rows).
     marks  one line index per argument the sites move (space-separated):
            the line of the caller's object that argument is.
     lines  the function's ownership lines in the alphabet and delimiters of
            `check_xc`, one per heap parameter first (parameter order), then
            every other object. A move at a call site is written `M` (it
            parses as the `m` it is); every other move-out is a lowercase `m`.

   A block is accepted iff
     - its modes are well-formed ids, and every site agrees with its callee's
       block (`site_ok`, CallModes);
     - BINDING: the number of marks is the number of moved arguments, every
       mark names a line of the block, and each line holds exactly as many
       `M` bytes as marks name it;
     - each parameter line, seeded with the reference its mode declares, and
       every other line pass the exit-aware ownership check with the owned-
       line guard (`check_line (guard_line …)`, as `check_xc` judges them),
       and no parameter line reuses (CallModes' shift-safety).

   `check_native_calls_sound` states what acceptance gives, per block: the
   agreement of every site, the binding of every moved argument, the callee-
   side obligation `param_stream_ok` for every parameter line (the premise of
   CallModes' `F_call`), and freedom from double-free and leak for every other
   line. *)

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
From AlmideTrust Require Import HelperSig.
Open Scope Z_scope.

(* ─── bytes ─── *)

Definition is_hash (a : ascii) : bool := Nat.eqb (nat_of_ascii a) 35.   (* '#' *)
Definition is_nl (a : ascii) : bool := Nat.eqb (nat_of_ascii a) 10.     (* '\n' *)
Definition is_call_move (a : ascii) : bool := Nat.eqb (nat_of_ascii a) 77. (* 'M' *)

(* Split a byte list at every byte `p` holds; the separators are dropped. *)
Fixpoint split_at_rev (p : ascii -> bool) (s : list ascii) (cur : list ascii)
    (acc : list (list ascii)) : list (list ascii) :=
  match s with
  | [] => rev (rev cur :: acc)
  | a :: r => if p a then split_at_rev p r [] (rev cur :: acc) else split_at_rev p r (a :: cur) acc
  end.

Definition split_at (p : ascii -> bool) (s : list ascii) : list (list ascii) :=
  split_at_rev p s [] [].

(* The bytes before the first byte `p` holds, and the bytes after it. *)
Fixpoint split_first (p : ascii -> bool) (s : list ascii) (cur : list ascii)
    : list ascii * list ascii :=
  match s with
  | [] => (rev cur, [])
  | a :: r => if p a then (rev cur, r) else split_first p r (a :: cur)
  end.

Definition str (l : list ascii) : string := string_of_list_ascii l.

(* ─── a block ─── *)

Record block : Type := {
  b_modes : list nat;
  b_sites : list (list nat);
  b_marks : list nat;
  b_lines : list (list CertItem);
  b_raw : list (list ascii)
}.

Definition parse_block (b : list ascii) : block :=
  let (header, body) := split_first is_nl b [] in
  let (modes, rest) := split_first is_bar header [] in
  let (sites, marks) := split_first is_bar rest [] in
  {| b_modes := pnats (str modes) None [];
     b_sites := List.filter nonempty (parse_nat_lists (str sites));
     b_marks := pnats (str marks) None [];
     b_lines := parse_xc (str body) [] None None None;
     b_raw := split_at is_nl body |}.

Definition parse_native_calls (s : string) : list block :=
  List.map parse_block (split_at is_hash (list_ascii_of_string s)).

(* ─── the binding ─── *)

(* The arguments one site moves: its `1` modes. *)
Definition site_moves (site : list nat) : nat :=
  match site with
  | [] => O
  | _ :: actual => List.length (List.filter (Nat.eqb 1) actual)
  end.

Definition call_moves_on (l : list ascii) : nat := List.length (List.filter is_call_move l).

Definition marks_on (marks : list nat) (j : nat) : nat := List.length (List.filter (Nat.eqb j) marks).

Fixpoint lines_bound (raw : list (list ascii)) (j : nat) (marks : list nat) : bool :=
  match raw with
  | [] => true
  | l :: rest => andb (Nat.eqb (call_moves_on l) (marks_on marks j)) (lines_bound rest (S j) marks)
  end.

Definition marks_ok (b : block) : bool :=
  andb (Nat.eqb (List.length (b_marks b)) (List.fold_right Nat.add O (List.map site_moves (b_sites b))))
       (andb (forallb (fun j => Nat.ltb j (List.length (b_raw b))) (b_marks b))
             (lines_bound (b_raw b) O (b_marks b))).

(* ─── the lines ─── *)

(* A parameter line, seeded with the reference its mode declares, under the
   owned-line guard `check_xc` applies to every owned line. *)
Definition nparam_line_ok (m : Mode) (cs : list CertItem) : bool :=
  andb (line_no_reuse cs) (check_line (guard_line (seed m cs))).

Fixpoint nlines_ok (ms : list nat) (ls : list (list CertItem)) : bool :=
  match ms, ls with
  | [], rest => forallb (fun cs => check_line (guard_line cs)) rest
  | m :: ms', cs :: ls' => andb (nparam_line_ok (mode_of_nat m) cs) (nlines_ok ms' ls')
  | _ :: _, [] => false
  end.

Definition block_ok (sigs : list (list nat)) (b : block) : bool :=
  andb (sig_wf (b_modes b))
       (andb (forallb (site_ok sigs) (b_sites b))
             (andb (marks_ok b) (nlines_ok (b_modes b) (b_lines b)))).

Definition check_native_calls (bs : list block) : bool :=
  forallb (block_ok (List.map b_modes bs)) bs.

Definition check_native_calls_cert (s : string) : bool :=
  check_native_calls (parse_native_calls s).

(* ─── soundness ─── *)

Lemma nparam_line_weaken :
  forall m cs, nparam_line_ok m cs = true -> param_line_ok m cs = true.
Proof.
  intros m cs H. unfold nparam_line_ok in H. apply andb_prop in H. destruct H as [Hnr Hck].
  unfold param_line_ok. rewrite Hnr. simpl. exact (check_line_guard_weaken _ Hck).
Qed.

Lemma nlines_nth :
  forall ms ls j m cs, nlines_ok ms ls = true ->
    nth_error ms j = Some m -> nth_error ls j = Some cs ->
    nparam_line_ok (mode_of_nat m) cs = true.
Proof.
  induction ms as [| m0 ms IH]; intros ls j m cs H Hm Hc.
  - destruct j; discriminate.
  - destruct ls as [| c0 ls]; [discriminate |]. simpl in H.
    apply andb_prop in H. destruct H as [H0 H1].
    destruct j as [| j]; simpl in Hm, Hc.
    + injection Hm as <-. injection Hc as <-. exact H0.
    + exact (IH ls j m cs H1 Hm Hc).
Qed.

Lemma nlines_rest :
  forall ms ls, nlines_ok ms ls = true ->
    forall cs, In cs (skipn (List.length ms) ls) -> check_line cs = true.
Proof.
  induction ms as [| m0 ms IH]; intros ls H cs Hin.
  - simpl in *. rewrite forallb_forall in H. exact (check_line_guard_weaken cs (H cs Hin)).
  - destruct ls as [| c0 ls]; [discriminate |]. simpl in H, Hin.
    apply andb_prop in H. destruct H as [_ H1]. exact (IH ls H1 cs Hin).
Qed.

Lemma lines_bound_nth :
  forall raw j0 marks, lines_bound raw j0 marks = true ->
    forall k l, nth_error raw k = Some l -> call_moves_on l = marks_on marks (j0 + k).
Proof.
  induction raw as [| l0 raw IH]; intros j0 marks H k l Hk.
  - destruct k; discriminate.
  - simpl in H. apply andb_prop in H. destruct H as [H0 H1].
    destruct k as [| k]; simpl in Hk.
    + injection Hk as <-. apply Nat.eqb_eq in H0. rewrite Nat.add_0_r. exact H0.
    + rewrite (IH (S j0) marks H1 k l Hk). f_equal. lia.
Qed.

(* What an accepted witness gives, for every block of the program:
   1. AGREEMENT: every site names an in-range callee block and used exactly
      that block's declared modes;
   2. BINDING: the marks are as many as the arguments the sites move, each
      names a line of the block, and each line holds exactly as many call-site
      move-outs (`M`) as marks name it;
   3. every parameter line, under its declared mode, is a `param_stream_ok`
      body in every concrete run (the premise CallModes' `F_call` inlines);
   4. every other line is free of double-free and leak in every concrete run. *)
Theorem check_native_calls_sound :
  forall s, check_native_calls_cert s = true ->
    let bs := parse_native_calls s in
    forall b, In b bs ->
      (forall site, In site (b_sites b) -> site_agrees (List.map b_modes bs) site) /\
      (List.length (b_marks b) = List.fold_right Nat.add O (List.map site_moves (b_sites b)) /\
       (forall j, In j (b_marks b) -> (j < List.length (b_raw b))%nat) /\
       (forall j l, nth_error (b_raw b) j = Some l -> call_moves_on l = marks_on (b_marks b) j)) /\
      (forall j m cs, nth_error (b_modes b) j = Some m -> nth_error (b_lines b) j = Some cs ->
         forall body, UnrollsL cs body -> param_stream_ok (mode_of_nat m) body = true) /\
      (forall cs, In cs (skipn (List.length (b_modes b)) (b_lines b)) ->
         forall fops, UnrollsL cs fops -> run fops <> None /\ run fops = Some 0).
Proof.
  intros s H bs b Hin. unfold check_native_calls_cert, check_native_calls in H.
  rewrite forallb_forall in H. specialize (H b Hin).
  unfold block_ok in H.
  apply andb_prop in H. destruct H as [_ H].
  apply andb_prop in H. destruct H as [Hsites H].
  apply andb_prop in H. destruct H as [Hmarks Hlines].
  split; [| split; [| split]].
  - intros site Hs. rewrite forallb_forall in Hsites. exact (site_ok_agrees _ _ (Hsites site Hs)).
  - unfold marks_ok in Hmarks.
    apply andb_prop in Hmarks. destruct Hmarks as [Hlen Hmarks].
    apply andb_prop in Hmarks. destruct Hmarks as [Hin_range Hbound].
    split; [apply Nat.eqb_eq; exact Hlen | split].
    + intros j Hj. rewrite forallb_forall in Hin_range. apply Nat.ltb_lt. exact (Hin_range j Hj).
    + intros j l Hl. exact (lines_bound_nth _ O _ Hbound j l Hl).
  - intros j m cs Hm Hc body HU.
    exact (param_line_unroll_ok _ cs body (nparam_line_weaken _ _ (nlines_nth _ _ j m cs Hlines Hm Hc)) HU).
  - intros cs Hcs fops HU.
    exact (check_line_unroll_sound cs (nlines_rest _ _ Hlines cs Hcs) fops HU).
Qed.

(* ─── non-vacuous, on witness bytes ─── *)

(* `keep(xs) = xs` with a MOVE parameter (block 1: the parameter's line is the
   move-out of the return), called by block 0 with its own list, moved at the
   call (`iM`: allocated, moved into `keep`) and the result released (`id`). *)
Example nc_move_accepts :
  check_native_calls_cert "|1 1|0
iM
id#1||
m" = true.
Proof. reflexivity. Qed.

(* The same program with every parameter borrowed: the caller releases its own
   list after the call, the callee copies before it returns. *)
Example nc_borrow_accepts :
  check_native_calls_cert "|1 0|
id
id#0||
am" = true.
Proof. reflexivity. Qed.

(* MISSING MOVE: the site moves the list, the caller's line keeps it and
   releases it after the call. Both functions' lines balance on their own;
   inlined, the one reference is released twice. REJECTED by the binding. *)
Example nc_missing_move_rejects :
  check_native_calls_cert "|1 1|0
id
id#1||
m" = false.
Proof. reflexivity. Qed.

(* A mark with no move at the site: as many marks as moved arguments. *)
Example nc_extra_mark_rejects :
  check_native_calls_cert "|1 0|0
iM
id#0||
im" = false.
Proof. reflexivity. Qed.

(* MODE LIE at the site: the callee declares a move parameter, the site lends
   the list and the caller releases it afterwards. REJECTED by agreement. *)
Example nc_site_lie_rejects :
  check_native_calls_cert "|1 0|
id
id#1||
m" = false.
Proof. reflexivity. Qed.

(* OWNED CLAIMED, BORROWED AND USED AFTER THE CALL: the site moves the list
   (`M`), and the caller then reads it (`b`) — a use after the move. The
   caller's own line faults. REJECTED. *)
Example nc_use_after_move_rejects :
  check_native_calls_cert "|1 1|0
iMb
id#1||
m" = false.
Proof. reflexivity. Qed.

(* MODE LIE at the callee: it declares a move parameter it only reads, so the
   caller's handed-over reference is never released. REJECTED (a leak). *)
Example nc_callee_lie_rejects :
  check_native_calls_cert "|1 1|0
iM
id#1||
" = false.
Proof. reflexivity. Qed.

(* A moved parameter freed and then aliased and moved out: the owned-line
   guard `check_xc` applies holds on a parameter line too. REJECTED. *)
Example nc_param_resurrect_rejects :
  check_native_calls_cert "|1 1|0
iM
id#1||
dam" = false.
Proof. reflexivity. Qed.

(* A callee block index out of range — REJECTED. *)
Example nc_unknown_callee_rejects :
  check_native_calls_cert "|5 0|
id" = false.
Proof. reflexivity. Qed.

(* AXIOM AUDIT. *)
Print Assumptions check_native_calls_sound.
