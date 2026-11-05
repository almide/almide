(* Almide trust spine — NATIVE CALL BINDING (#4006, class P: per-parameter
   move modes on the verified native render).

   The native render gives a heap parameter one of three modes, the two of
   CallModes and a writable borrow:

     borrow (0) — the caller keeps its reference; the callee's line on the
                  parameter starts at 0 and must return to 0;
     move   (1) — the caller hands its one reference over at the call; the
                  callee's line starts at 1 and must reach 0 (it returns,
                  stores or releases the value on every path);
     writable borrow (2, #4491) — a `mut` heap parameter the body edits in
                  place (`&mut`): on the callee's side exactly the borrow
                  (its line starts at 0 and must return to 0, so the borrow
                  is never returned, stored or released); on the caller's
                  side the WRITABLE-BORROW rule below.

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

     <modes>|<sites>|<marks>|<hosts>|<wargs>\n<line 0>\n<line 1>\n…

     modes  the function's heap-parameter modes (space-separated nat ids,
            0 = borrow, 1 = move, 2 = writable borrow), in parameter order. The program's
            signature table is the list of every block's modes — read off the
            blocks themselves, so a caller and its callee cannot be judged
            against two different tables.
     sites  the function's call sites, `;`-separated, each
            `<callee block index> <actual modes…>` (CallModes' site rows).
     marks  one line index per argument the sites move (space-separated):
            the line of the caller's object that argument is.
     hosts  one id per call the function makes to a NATIVE FS HOST CALL
            (#4006 class E): the index of the runtime function it calls in
            `native_fs_host_calls` below — the named set of the trusted
            base's "Native fs host calls" row (proofs/TRUSTED_BASE.md). A
            call outside the set carries an out-of-range id.
     wargs  one row per WRITABLE SITE (a site with a mode-2 argument), in site
            order, `;`-separated: the caller's line of each heap argument of
            that site, in parameter order (#4491).
     lines  the function's ownership lines in the alphabet and delimiters of
            `check_xc`, one per heap parameter first (parameter order), then
            every other object. A move at a call site is written `M` (it
            parses as the `m` it is); every other move-out is a lowercase `m`.
            The liveness probe of a heap argument of a writable site is
            written `B` (it parses as the `b` it is); every other probe is a
            lowercase `b`.

   THE WRITABLE-BORROW RULE (#4491). A writable site is accepted iff its row
   names one line per heap argument and, for each entry j:
     - j is a line of the caller's own objects, not a parameter line
       (#modes ≤ j < #lines);
     - no other argument of the site names j (the buffer is not passed twice,
       in any mode — so it is neither moved nor read at the same call);
     - if the argument is the writable one, line j is UNSHARED: born by its
       `i`, and no later `i`, `a` or `s` (no alias, re-acquire or share-out),
       so at every point of every run the caller holds at most one reference;
   and every line holds exactly as many `B` bytes as the rows name it
   (BINDING, the `M` rule's shape). Then at the call the caller holds the one
   reference to the buffer (the `B` probe needs a live count, the line never
   exceeds one), no alias of it is live and none is passed beside it; the `B`
   leaves the count unchanged, so the caller's line goes on after the call
   on the same object (it sees the writes) and must still balance; and the
   callee's line, seeded at 0 as a borrow, cannot release, return or store
   the borrowed buffer (`param_stream_ok MBorrow`: it does not escape).

   A block is accepted iff
     - its modes are well-formed ids, and every site agrees with its callee's
       block (`site_ok`, CallModes);
     - BINDING: the number of marks is the number of moved arguments, every
       mark names a line of the block, and each line holds exactly as many
       `M` bytes as marks name it;
     - each parameter line, seeded with the reference its mode declares, and
       every other line pass the exit-aware ownership check with the owned-
       line guard (`check_line (guard_line …)`, as `check_xc` judges them),
       and no parameter line reuses (CallModes' shift-safety);
     - HOSTS: every host id names a function of `native_fs_host_calls`
       (`hosts_ok`): a host call outside the named set is refused;
     - WRITABLE: every writable site satisfies the writable-borrow rule above
       (`wargs_ok`).

   `check_native_calls_sound` states what acceptance gives, per block: the
   agreement of every site, the binding of every moved argument, the callee-
   side obligation `param_stream_ok` for every parameter line (the premise of
   CallModes' `F_call`), freedom from double-free and leak for every other
   line, and, for every writable site, the exclusivity of its writable
   argument and the binding of its `B` probes. *)

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
Definition is_call_borrow (a : ascii) : bool := Nat.eqb (nat_of_ascii a) 66. (* 'B' *)

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
  b_hosts : list nat;
  b_wargs : list (list nat);
  b_lines : list (list CertItem);
  b_raw : list (list ascii)
}.

Definition parse_block (b : list ascii) : block :=
  let (header, body) := split_first is_nl b [] in
  let (modes, rest) := split_first is_bar header [] in
  let (sites, rest') := split_first is_bar rest [] in
  let (marks, rest'') := split_first is_bar rest' [] in
  let (hosts, wargs) := split_first is_bar rest'' [] in
  {| b_modes := pnats (str modes) None [];
     b_sites := List.filter nonempty (parse_nat_lists (str sites));
     b_marks := pnats (str marks) None [];
     b_hosts := pnats (str hosts) None [];
     b_wargs := List.filter nonempty (parse_nat_lists (str wargs));
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

(* ─── the host calls ─── *)

(* The NATIVE FS HOST CALLS (#4006 class E; owner ruling 2026-11-05): the
   runtime fs functions a natively rendered program may call as trusted host
   calls — the list `crates/almide-mir/src/host_ops.rs`
   `NATIVE_FS_HOST_CALLS` and the trusted base's row name, in that order
   (`tests/native_fs_host_calls_test.rs` holds the three to one list). *)
Local Open Scope string_scope.
Definition native_fs_host_calls : list string :=
  ["almide_rt_fs_read_text";
   "almide_rt_fs_read_bytes";
   "almide_rt_fs_read_lines";
   "almide_rt_fs_read_text_if_exists";
   "almide_rt_fs_list_dir";
   "almide_rt_fs_exists";
   "almide_rt_fs_is_dir";
   "almide_rt_fs_is_file";
   "almide_rt_fs_is_symlink";
   "almide_rt_fs_walk";
   "almide_rt_fs_glob";
   "almide_rt_fs___root_read_text";
   "almide_rt_fs___root_read_bytes";
   "almide_rt_fs___root_list_dir";
   "almide_rt_fs___root_exists";
   "almide_rt_fs_temp_dir";
   "almide_rt_fs_write";
   "almide_rt_fs_write_bytes";
   "almide_rt_fs_mkdir_p";
   "almide_rt_fs_remove_all";
   "almide_rt_fs___root_write_text";
   "almide_rt_fs_rename";
   "almide_rt_fs_copy";
   "almide_rt_fs_append";
   "almide_rt_fs_append_bytes";
   "almide_rt_fs_remove";
   "almide_rt_fs___root_remove";
   "almide_rt_fs_create_temp_dir";
   "almide_rt_fs_create_temp_file"].

Definition hosts_ok (b : block) : bool :=
  forallb (fun h => Nat.ltb h (List.length native_fs_host_calls)) (b_hosts b).

(* ─── the writable borrow (#4491) ─── *)

(* The three mode ids. A writable borrow (2) is a borrow on the callee's
   side: its line starts at 0. *)
Definition nmode (n : nat) : Mode := match n with S O => MMove | _ => MBorrow end.

Definition nsig_wf (sig : list nat) : bool := forallb (fun m => Nat.leb m 2) sig.

(* A site with a writable argument. *)
Definition site_writes (site : list nat) : bool :=
  match site with
  | [] => false
  | _ :: actual => existsb (Nat.eqb 2) actual
  end.

Definition wsites (b : block) : list (list nat) := List.filter site_writes (b_sites b).

(* An op that cannot raise the count: no fresh acquire, alias or share-out. *)
Definition nsh (o : Op) : bool :=
  match o with Inc | Alias | ShareOut => false | _ => true end.

Definition item_nsh (c : CertItem) : bool :=
  match c with
  | COp o => nsh o
  | CLoop body => forallb nsh body
  | CCondLoop t e => andb (forallb nsh t) (forallb nsh e)
  | CBranch t e => andb (forallb nsh t) (forallb nsh e)
  | CBranchRet _ t e => andb (forallb nsh t) (forallb nsh e)
  | CBranchAbort _ t e => andb (forallb nsh t) (forallb nsh e)
  | CPoison => false
  end.

(* UNSHARED: an owned object, born by its `i`, never aliased, re-acquired or
   shared out afterwards. *)
Definition unshared (cs : list CertItem) : bool :=
  match cs with
  | COp Inc :: rest => forallb item_nsh rest
  | _ => false
  end.

(* One entry of a writable site's row: mode `m` of the position, line `j`. *)
Definition wentry_ok (b : block) (row : list nat) (mj : nat * nat) : bool :=
  let (m, j) := mj in
  andb (Nat.leb (List.length (b_modes b)) j)
   (andb (Nat.ltb j (List.length (b_raw b)))
    (andb (Nat.eqb (marks_on row j) 1)
          (if Nat.eqb m 2 then unshared (List.nth j (b_lines b) []) else true))).

Definition wrow_ok (b : block) (site row : list nat) : bool :=
  match site with
  | [] => false
  | _ :: actual =>
      andb (Nat.eqb (List.length row) (List.length actual))
           (forallb (wentry_ok b row) (combine actual row))
  end.

Fixpoint wrows_ok (b : block) (sites rows : list (list nat)) : bool :=
  match sites, rows with
  | [], [] => true
  | s :: ss, r :: rs => andb (wrow_ok b s r) (wrows_ok b ss rs)
  | _, _ => false
  end.

Definition call_borrows_on (l : list ascii) : nat := List.length (List.filter is_call_borrow l).

Definition named_on (rows : list (list nat)) (j : nat) : nat :=
  List.fold_right Nat.add O (List.map (fun row => marks_on row j) rows).

Fixpoint lines_wbound (raw : list (list ascii)) (j : nat) (rows : list (list nat)) : bool :=
  match raw with
  | [] => true
  | l :: rest => andb (Nat.eqb (call_borrows_on l) (named_on rows j)) (lines_wbound rest (S j) rows)
  end.

Definition wargs_ok (b : block) : bool :=
  andb (wrows_ok b (wsites b) (b_wargs b)) (lines_wbound (b_raw b) O (b_wargs b)).

(* ─── the lines ─── *)

(* A parameter line, seeded with the reference its mode declares, under the
   owned-line guard `check_xc` applies to every owned line. *)
Definition nparam_line_ok (m : Mode) (cs : list CertItem) : bool :=
  andb (line_no_reuse cs) (check_line (guard_line (seed m cs))).

Fixpoint nlines_ok (ms : list nat) (ls : list (list CertItem)) : bool :=
  match ms, ls with
  | [], rest => forallb (fun cs => check_line (guard_line cs)) rest
  | m :: ms', cs :: ls' => andb (nparam_line_ok (nmode m) cs) (nlines_ok ms' ls')
  | _ :: _, [] => false
  end.

Definition block_ok (sigs : list (list nat)) (b : block) : bool :=
  andb (nsig_wf (b_modes b))
       (andb (forallb (site_ok sigs) (b_sites b))
             (andb (marks_ok b) (andb (hosts_ok b)
                   (andb (nlines_ok (b_modes b) (b_lines b)) (wargs_ok b))))).

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
    nparam_line_ok (nmode m) cs = true.
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
         forall body, UnrollsL cs body -> param_stream_ok (nmode m) body = true) /\
      (forall cs, In cs (skipn (List.length (b_modes b)) (b_lines b)) ->
         forall fops, UnrollsL cs fops -> run fops <> None /\ run fops = Some 0) /\
      (forall h, In h (b_hosts b) -> exists f, nth_error native_fs_host_calls h = Some f).
Proof.
  intros s H bs b Hin. unfold check_native_calls_cert, check_native_calls in H.
  rewrite forallb_forall in H. specialize (H b Hin).
  unfold block_ok in H.
  apply andb_prop in H. destruct H as [_ H].
  apply andb_prop in H. destruct H as [Hsites H].
  apply andb_prop in H. destruct H as [Hmarks H].
  apply andb_prop in H. destruct H as [Hhosts H].
  apply andb_prop in H. destruct H as [Hlines _].
  split; [| split; [| split; [| split]]].
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
  - intros h Hh. unfold hosts_ok in Hhosts. rewrite forallb_forall in Hhosts.
    specialize (Hhosts h Hh). apply Nat.ltb_lt in Hhosts.
    destruct (nth_error native_fs_host_calls h) as [f |] eqn:E; [exists f; reflexivity |].
    apply nth_error_None in E. lia.
Qed.

(* ─── the writable borrow, soundness (#4491) ─── *)

(* An op stream that cannot raise the count never ends above where it began. *)
Lemma exec_nsh_le :
  forall ops r r', forallb nsh ops = true -> exec ops r = Some r' -> r' <= r.
Proof.
  induction ops as [| o ops IH]; intros r r' Hn Hx; simpl in *.
  - injection Hx as <-. lia.
  - apply andb_prop in Hn. destruct Hn as [Ho Hn].
    destruct o; simpl in Ho; try discriminate.
    + destruct (Z.leb r 0) eqn:E; [discriminate |].
      specialize (IH _ _ Hn Hx). lia.
    + destruct (Z.leb r 0) eqn:E; [discriminate |].
      specialize (IH _ _ Hn Hx). lia.
    + destruct (Z.eqb r 1) eqn:E; [| discriminate].
      apply Z.eqb_eq in E. specialize (IH _ _ Hn Hx). lia.
    + destruct (Z.leb r 0) eqn:E; [discriminate |]. exact (IH _ _ Hn Hx).
Qed.

Lemma forallb_nsh_app :
  forall a c, forallb nsh (a ++ c) = andb (forallb nsh a) (forallb nsh c).
Proof. intros a c. apply forallb_app. Qed.

Lemma nsh_concat_repeat :
  forall body n, forallb nsh body = true -> forallb nsh (List.concat (List.repeat body n)) = true.
Proof.
  intros body n H. induction n as [| n IH]; simpl; [reflexivity |].
  rewrite forallb_nsh_app, H, IH. reflexivity.
Qed.

Lemma nsh_cond_concat :
  forall t e bs, forallb nsh t = true -> forallb nsh e = true -> forallb nsh (cond_concat t e bs) = true.
Proof.
  intros t e bs Ht He. induction bs as [| b bs IH]; [reflexivity |].
  destruct b; simpl; rewrite forallb_nsh_app, IH; rewrite ?Ht, ?He; reflexivity.
Qed.

(* Every concrete run of a line of non-raising items is non-raising. *)
Lemma nsh_unroll :
  forall cs fops, forallb item_nsh cs = true -> UnrollsL cs fops -> forallb nsh fops = true.
Proof.
  intros cs fops H HU. induction HU; simpl in H.
  - reflexivity.
  - apply andb_prop in H. destruct H as [H1 H2]. simpl. rewrite H1. exact (IHHU H2).
  - apply andb_prop in H. destruct H as [H1 H2].
    rewrite forallb_nsh_app, (nsh_concat_repeat body n H1), (IHHU H2). reflexivity.
  - apply andb_prop in H. destruct H as [H1 H2]. apply andb_prop in H1. destruct H1 as [Ht He].
    rewrite forallb_nsh_app, (nsh_cond_concat thenb elseb bs Ht He), (IHHU H2). reflexivity.
  - apply andb_prop in H. destruct H as [H1 H2]. apply andb_prop in H1. destruct H1 as [Ht He].
    rewrite forallb_nsh_app, (IHHU H2). destruct choice; simpl; rewrite ?Ht, ?He; reflexivity.
  - apply andb_prop in H. destruct H as [H1 _]. apply andb_prop in H1. destruct H1 as [Ht He].
    destruct retthen; assumption.
  - apply andb_prop in H. destruct H as [H1 H2]. apply andb_prop in H1. destruct H1 as [Ht He].
    rewrite forallb_nsh_app, (IHHU H2). destruct retthen; simpl; rewrite ?Ht, ?He; reflexivity.
  - apply andb_prop in H. destruct H as [H1 H2]. apply andb_prop in H1. destruct H1 as [Ht He].
    rewrite forallb_nsh_app, (IHHU H2). destruct abthen; simpl; rewrite ?Ht, ?He; reflexivity.
Qed.

Lemma forallb_firstn :
  forall {A} (p : A -> bool) n l, forallb p l = true -> forallb p (firstn n l) = true.
Proof.
  intros A p n. induction n as [| n IH]; intros l H; [reflexivity |].
  destruct l as [| x l]; [reflexivity |]. simpl in *.
  apply andb_prop in H. destruct H as [H1 H2]. rewrite H1. exact (IH l H2).
Qed.

(* EXCLUSIVITY: on an unshared line the owner never holds more than one
   reference — at every point of every concrete run. *)
Theorem unshared_at_most_one :
  forall cs fops, unshared cs = true -> UnrollsL cs fops ->
    forall n r, exec (firstn n fops) 0 = Some r -> r <= 1.
Proof.
  intros cs fops Hu HU n r Hx.
  destruct cs as [| c rest]; [discriminate |].
  destruct c as [o | | | | | |]; try discriminate.
  destruct o; try discriminate.
  simpl in Hu. inversion HU as [| o' a b HU' | | | | | |]; subst.
  destruct n as [| n].
  - simpl in Hx. injection Hx as <-. lia.
  - simpl in Hx.
    exact (exec_nsh_le _ _ _ (forallb_firstn nsh n _ (nsh_unroll _ _ Hu HU')) Hx).
Qed.

Lemma nth_error_combine_in :
  forall {A B} (a : list A) (c : list B) k x y,
    nth_error a k = Some x -> nth_error c k = Some y -> In (x, y) (combine a c).
Proof.
  intros A B a. induction a as [| x0 a IH]; intros c k x y Ha Hc.
  - destruct k; discriminate.
  - destruct c as [| y0 c]; [destruct k; discriminate |].
    destruct k as [| k]; simpl in *.
    + injection Ha as <-. injection Hc as <-. left. reflexivity.
    + right. exact (IH c k x y Ha Hc).
Qed.

Lemma nth_error_nth_eq :
  forall {A} (l : list A) j x d, nth_error l j = Some x -> List.nth j l d = x.
Proof.
  intros A l. induction l as [| y l IH]; intros j x d H.
  - destruct j; discriminate.
  - destruct j as [| j]; simpl in *; [injection H as <-; reflexivity | exact (IH j x d H)].
Qed.

Lemma wrows_ok_in :
  forall b ss rs, wrows_ok b ss rs = true ->
    List.length ss = List.length rs /\ (forall s r, In (s, r) (combine ss rs) -> wrow_ok b s r = true).
Proof.
  intros b ss. induction ss as [| s0 ss IH]; intros rs H.
  - destruct rs; [| discriminate]. split; [reflexivity | intros s r []].
  - destruct rs as [| r0 rs]; [discriminate |]. simpl in H.
    apply andb_prop in H. destruct H as [H0 H1]. destruct (IH rs H1) as [Hl Hin].
    split; [simpl; lia |].
    intros s r [E | E]; [injection E as <- <-; exact H0 | exact (Hin s r E)].
Qed.

Lemma lines_wbound_nth :
  forall raw j0 rows, lines_wbound raw j0 rows = true ->
    forall k l, nth_error raw k = Some l -> call_borrows_on l = named_on rows (j0 + k).
Proof.
  induction raw as [| l0 raw IH]; intros j0 rows H k l Hk.
  - destruct k; discriminate.
  - simpl in H. apply andb_prop in H. destruct H as [H0 H1].
    destruct k as [| k]; simpl in Hk.
    + injection Hk as <-. apply Nat.eqb_eq in H0. rewrite Nat.add_0_r. exact H0.
    + rewrite (IH (S j0) rows H1 k l Hk). f_equal. lia.
Qed.

(* What an accepted witness gives for every WRITABLE SITE of every block:
   1. the modes are the three ids (≤ 2);
   2. one row per writable site, as many entries as the site has heap
      arguments, each a line of the caller's own objects (not a parameter
      line) that no other argument of the site names: the buffer is not
      passed twice, so it is neither moved nor read at the same call;
   3. EXCLUSIVITY: the writable argument's line is unshared, so at every
      point of every concrete run the caller holds at most one reference to
      it; with the line's fault-free run (`check_native_calls_sound` 4) its
      `B` probe meets a live count, so at the call it holds exactly one, and
      the probe leaves the count unchanged — the line goes on after the call
      on the same object;
   4. BINDING: every line holds exactly as many call-site probes `B` as the
      rows name it.
   The callee's side is `check_native_calls_sound` 3 under `nmode 2 =
   MBorrow`: the writable parameter's line starts at 0 and is a
   `param_stream_ok MBorrow` body, so the borrow is never released, returned
   or stored. *)
Theorem check_native_calls_writable_sound :
  forall s, check_native_calls_cert s = true ->
    let bs := parse_native_calls s in
    forall b, In b bs ->
      (forall m, In m (b_modes b) -> (m <= 2)%nat) /\
      List.length (wsites b) = List.length (b_wargs b) /\
      (forall site row, In (site, row) (combine (wsites b) (b_wargs b)) ->
         List.length row = List.length (tl site) /\
         forall k m j, nth_error (tl site) k = Some m -> nth_error row k = Some j ->
           (List.length (b_modes b) <= j < List.length (b_raw b))%nat /\
           marks_on row j = 1%nat /\
           (m = 2%nat -> forall cs, nth_error (b_lines b) j = Some cs ->
              forall fops, UnrollsL cs fops ->
                forall n r, exec (firstn n fops) 0 = Some r -> r <= 1)) /\
      (forall j l, nth_error (b_raw b) j = Some l -> call_borrows_on l = named_on (b_wargs b) j).
Proof.
  intros s H bs b Hin. unfold check_native_calls_cert, check_native_calls in H.
  rewrite forallb_forall in H. specialize (H b Hin).
  unfold block_ok in H.
  apply andb_prop in H. destruct H as [Hwf H].
  apply andb_prop in H. destruct H as [_ H].
  apply andb_prop in H. destruct H as [_ H].
  apply andb_prop in H. destruct H as [_ H].
  apply andb_prop in H. destruct H as [_ Hw].
  unfold wargs_ok in Hw. apply andb_prop in Hw. destruct Hw as [Hrows Hbound].
  destruct (wrows_ok_in _ _ _ Hrows) as [Hlen Hrow].
  split; [| split; [exact Hlen | split]].
  - intros m Hm. unfold nsig_wf in Hwf. rewrite forallb_forall in Hwf.
    apply Nat.leb_le. exact (Hwf m Hm).
  - intros site row Hsr. specialize (Hrow site row Hsr).
    destruct site as [| callee actual]; [discriminate |].
    unfold wrow_ok in Hrow. apply andb_prop in Hrow. destruct Hrow as [Hrl Hent].
    simpl. split; [apply Nat.eqb_eq; exact Hrl |].
    intros k m j Hk Hj.
    rewrite forallb_forall in Hent.
    specialize (Hent (m, j) (nth_error_combine_in _ _ _ _ _ Hk Hj)).
    unfold wentry_ok in Hent.
    apply andb_prop in Hent. destruct Hent as [Hlo Hent].
    apply andb_prop in Hent. destruct Hent as [Hhi Hent].
    apply andb_prop in Hent. destruct Hent as [Hone Hun].
    split; [split; [apply Nat.leb_le; exact Hlo | apply Nat.ltb_lt; exact Hhi] |].
    split; [apply Nat.eqb_eq; exact Hone |].
    intros Hm2 cs Hcs fops HU. subst m. simpl in Hun.
    rewrite (nth_error_nth_eq _ _ _ [] Hcs) in Hun.
    exact (unshared_at_most_one cs fops Hun HU).
  - intros j l Hl. exact (lines_wbound_nth _ O _ Hbound j l Hl).
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

(* A NATIVE FS HOST CALL: `main` calls host id 0, the set's first function
   (`almide_rt_fs_read_text`) — ACCEPTED. *)
Example nc_host_call_accepts :
  check_native_calls_cert "||| 0
id" = true.
Proof. reflexivity. Qed.

(* A host call to a runtime function OUTSIDE the named set (the out-of-range
   id the producer writes for it: the set's length, 29) — REJECTED. *)
Example nc_host_outside_set_rejects :
  check_native_calls_cert "||| 29
id" = false.
Proof. reflexivity. Qed.

(* A callee block index out of range — REJECTED. *)
Example nc_unknown_callee_rejects :
  check_native_calls_cert "|5 0|
id" = false.
Proof. reflexivity. Qed.

(* ─── the writable borrow, on witness bytes (#4491) ─── *)

(* `rec(mut b: Bytes, x)` (block 1, one WRITABLE parameter, its line empty:
   it edits the buffer through floor calls and never releases, returns or
   stores it), called by `main` (block 0) on its own buffer: the site's row
   names the buffer's line 0, which holds the one call-site probe `B`, is
   born by `i` and never shared, and is released after the call. ACCEPTED. *)
Example nc_writable_accepts :
  check_native_calls_cert "|1 2|||0
iBd#2||||
" = true.
Proof. reflexivity. Qed.

(* THE SAME BUFFER PASSED TWICE AS WRITABLE (`two(mut a, mut b)` called
   `two(buf, buf)`): the row names line 0 at both positions. REJECTED. *)
Example nc_writable_twice_rejects :
  check_native_calls_cert "|1 2 2|||0 0
iBBd#2 2||||

" = false.
Proof. reflexivity. Qed.

(* A USE OF THE CALLER'S BINDING INSIDE THE CALL: the buffer is the writable
   argument and also a borrowed one (`put(buf, buf)`). REJECTED. *)
Example nc_writable_read_beside_rejects :
  check_native_calls_cert "|1 2 0|||0 0
iBBd#2 0||||

" = false.
Proof. reflexivity. Qed.

(* ... or moved at the same call. REJECTED. *)
Example nc_writable_moved_beside_rejects :
  check_native_calls_cert "|1 2 1|0||0 0
iBM#2 1||||

m" = false.
Proof. reflexivity. Qed.

(* AN ESCAPING BORROW: the callee returns its writable parameter (`m` on a
   line seeded at 0). REJECTED by the callee-side borrow rule. *)
Example nc_writable_escape_rejects :
  check_native_calls_cert "|1 2|||0
iBd#2||||
m" = false.
Proof. reflexivity. Qed.

(* AN ALIAS LIVE AT THE CALL: the caller copied the buffer's reference (`a`)
   before the writable call. The line is not unshared. REJECTED. *)
Example nc_writable_aliased_rejects :
  check_native_calls_cert "|1 2|||0
iaBdd#2||||
" = false.
Proof. reflexivity. Qed.

(* A parameter line named as the writable argument (a caller passing on a
   buffer it does not own). REJECTED. *)
Example nc_writable_param_line_rejects :
  check_native_calls_cert "0|1 2|||0
#2||||
" = false.
Proof. reflexivity. Qed.

(* BINDING: a writable site with no `B` on the line its row names, and a `B`
   no row names. Both REJECTED. *)
Example nc_writable_unbound_rejects :
  check_native_calls_cert "|1 2|||0
ibd#2||||
" = false.
Proof. reflexivity. Qed.

Example nc_writable_stray_probe_rejects :
  check_native_calls_cert "|||
iBd" = false.
Proof. reflexivity. Qed.

(* A writable site with no row. REJECTED. *)
Example nc_writable_no_row_rejects :
  check_native_calls_cert "|1 2||
iBd#2||||
" = false.
Proof. reflexivity. Qed.

(* A mode outside the three ids. REJECTED. *)
Example nc_mode_three_rejects :
  check_native_calls_cert "|1 3||
id#3||
" = false.
Proof. reflexivity. Qed.

(* AXIOM AUDIT. *)
Print Assumptions check_native_calls_sound.
Print Assumptions check_native_calls_writable_sound.
