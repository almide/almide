(* Almide v1 trust kernel — proof spine, brick 1.

   The flight-grade thesis in miniature (docs/roadmap/active/v1-mir-architecture.md
   §5; the tier-1 stack): we do NOT prove the compiler. We prove that a tiny
   CHECKER is sound:

       check(artifact) = accept  ==>  P(artifact)

   so the (large, untrusted, possibly-buggy) compiler can emit anything — if its
   output is wrong, the checker rejects it. Here P is the first flight-grade
   property, MEMORY SAFETY / RC BALANCE, over the MIR ownership op sequence
   (`almide-mir`'s `verify_ownership`, ported to Gallina). This file is checked
   by the Coq/Rocq kernel; later bricks add the certificate format, more
   properties, CertiCoq+CompCert extraction of the checker to machine code, and
   the translation validator V (model ⊒ wasm bytes). *)

From Stdlib Require Import List.
Import ListNotations.
From Stdlib Require Import ZArith.
From Stdlib Require Import String Ascii.
Open Scope Z_scope.

(* The MIR ownership ops on a single reference-counted object — certificate
   format v1's ownership ALPHABET. Each op carries a SIGNED DELTA the checker
   folds; the DISTINCT constructors record a ground fact the untrusted compiler
   already decided (so the checker never re-derives it):
     Inc     = +1 FRESH acquire   (Alloc / fresh Dup / owned Call-result).
     Alias   = +1 ALIAS acquire   (a binding that incs an existing SHARED ref —
                                   the share-vs-move ground fact, G2.1). Folds
                                   like Inc; the separate constructor is the fact.
     Dec     = −1 plain release   (Drop).
     MoveOut = −1 MOVE-OUT        (Consume — ref transferred to a container /
                                   return / consuming callee). Folds like Dec.
   v0's {Inc,Dec} is the DEGENERATE case (Alias≡Inc, MoveOut≡Dec at the balance
   fold), so this strictly generalizes brick 1 with ZERO new proof obligations —
   the soundness proofs reason about the run's Z result, not the constructors.
     Reuse   = REUSE-eligible release (the PERCEUS mode): a release the compiler
               proved acts on a UNIQUELY-owned object, so the freed block may be
               reused IN PLACE. The uniqueness OBLIGATION is discharged by the
               FOLD, not a separate subset section: a Reuse is valid iff rc = 1 at
               that point (the checker's own count), so it derives uniqueness
               WITHOUT trusting the compiler's analysis. (A subset section would
               have had to trust a compiler-asserted "proven-unique" set — an
               inference the checker cannot re-derive; the fold already knows rc,
               so the guard is both simpler and strictly sound. A Reuse at rc > 1
               = SHARED = unsound, and FAULTS — see `check_reuse_sound`.)
     Borrow  = +0 USE of a live reference (a read-only borrow / in-place unique
               use — `Op::Borrow`/`MakeUnique`). No refcount change, but a
               LIVENESS guard: a borrow of a DEAD object (rc = 0 — every owned
               reference already released) is a use-after-free and FAULTS. This
               makes owned-object use-after-free WITNESSABLE (brick 5b): `idb`
               rejects. Borrowed PARAMS stay event-free (their liveness is the
               caller's obligation — the call-mode system, CallModes.v). *)
Inductive Op : Type :=
  | Inc : Op
  | Alias : Op
  | Dec : Op
  | MoveOut : Op
  | Reuse : Op
  | Borrow : Op.

(* OPERATIONAL SEMANTICS (the ALS side — "what actually happens").
   A refcount, or a FAULT (`None`) when a −1 op hits rc = 0: that is a
   double-free / use-after-free — releasing a reference that does not exist.
   Alias folds like Inc (+1), MoveOut like Dec (−1): the balance is about the
   DELTAS, which is exactly why adding those ground-fact constructors costs no new
   proof. Reuse is the ONE exception: besides its −1 it carries a UNIQUENESS guard
   (valid only at rc = 1), so it faults on a SHARED object — the reuse-soundness
   obligation, checked by the same fold (`check_reuse_sound`). *)
Fixpoint exec (ops : list Op) (rc : Z) : option Z :=
  match ops with
  | [] => Some rc
  | Inc :: rest => exec rest (rc + 1)
  | Alias :: rest => exec rest (rc + 1)
  | Dec :: rest => if rc <=? 0 then None else exec rest (rc - 1)
  | MoveOut :: rest => if rc <=? 0 then None else exec rest (rc - 1)
  (* Reuse is REUSE-eligible: the compiler asserts the block is UNIQUELY owned
     (rc = 1) so it may be repurposed IN PLACE. The checker does NOT trust that
     assertion — it derives uniqueness from its OWN fold: a Reuse is valid iff
     rc = 1 at this point (then it goes to 0). A Reuse at rc > 1 (a SHARED object)
     would corrupt the aliasing owner — it FAULTS; rc <= 0 is the usual underflow. *)
  | Reuse :: rest => if Z.eqb rc 1 then exec rest 0 else None
  (* Borrow is a +0 USE: it needs a live reference (rc > 0) but releases nothing.
     A borrow at rc = 0 is a use-after-free — the object's owned references are
     all gone — and FAULTS, exactly like verify_ownership's live-check on
     `Op::Borrow`/`MakeUnique`. *)
  | Borrow :: rest => if rc <=? 0 then None else exec rest rc
  end.

Definition run (ops : list Op) : option Z := exec ops 0.

(* THE CHECKER K (a Gallina function — total, decidable). Accept iff the run
   neither faults nor leaks: it ends with rc = 0. *)
Definition check (ops : list Op) : bool :=
  match run ops with
  | Some z => Z.eqb z 0
  | None => false
  end.

(* The SEMANTIC PROPERTY P, defined against the operational semantics:
   - no double-free / use-after-free: the run never faults;
   - no leak: every acquired reference is released (final rc = 0). *)
Definition no_double_free (ops : list Op) : Prop := run ops <> None.
Definition no_leak (ops : list Op) : Prop := run ops = Some 0.

(* SOUNDNESS: the checker accepting GUARANTEES the property. This is the whole
   proof-carrying-code thesis for the RC-balance property: trust shrinks from the
   compiler to `check` (+ this theorem). *)
Theorem check_sound :
  forall ops, check ops = true -> no_double_free ops /\ no_leak ops.
Proof.
  intros ops H.
  unfold check in H.
  unfold no_double_free, no_leak.
  destruct (run ops) as [z |] eqn:E.
  - (* run did not fault; H : (z =? 0) = true, so z = 0. *)
    apply Z.eqb_eq in H. subst z.
    split.
    + discriminate.        (* Some 0 <> None *)
    + reflexivity.         (* run ops = Some 0 *)
  - (* run faulted; H : false = true, impossible. *)
    discriminate.
Qed.

(* A small SANITY check that the checker is non-vacuous (it does accept real
   balanced sequences and reject faulty / leaky ones), so `check_sound` is not
   trivially true for an always-false checker. *)
Example accepts_balanced : check [Inc; Inc; Dec; Dec] = true.
Proof. reflexivity. Qed.

Example rejects_double_free : check [Inc; Dec; Dec] = false.
Proof. reflexivity. Qed.

Example rejects_leak : check [Inc; Inc; Dec] = false.
Proof. reflexivity. Qed.

(* MULTI-OBJECT. The real MIR has MANY reference-counted objects; `verify_ownership`
   accounts each separately. A whole-function certificate is therefore one
   Inc/Dec stream PER object. `check_all` accepts iff every object is balanced. *)
Definition check_all (objs : list (list Op)) : bool := forallb check objs.

(* SOUNDNESS lifts to every object: accepting the function certificate means
   EVERY object is free of double-free and leak. *)
Theorem check_all_sound :
  forall objs, check_all objs = true ->
    forall ops, In ops objs -> no_double_free ops /\ no_leak ops.
Proof.
  intros objs H ops Hin.
  unfold check_all in H. rewrite forallb_forall in H.
  apply check_sound. apply H. exact Hin.
Qed.

Example accepts_two_balanced_objects :
  check_all [[Inc; Dec]; [Inc; Inc; Dec; Dec]] = true.
Proof. reflexivity. Qed.

Example rejects_if_any_object_faulty :
  check_all [[Inc; Dec]; [Inc; Dec; Dec]] = false.
Proof. reflexivity. Qed.

(* ─── certificate parsing, INTERNALIZED INTO COQ ───
   The byte→op tokenizer used to live in the OCaml driver, OUTSIDE the trusted
   base (a known-limitation). Here it is a proven Gallina function: the WHOLE
   "bytes ⟶ accept/reject" pipeline is now kernel-checked, shrinking the trusted
   base to just file I/O. Certificate format v1: one object per newline; within a
   line the ownership alphabet is `i`/`I` = fresh +1, `a`/`A` = alias +1,
   `d`/`D` = release −1, `m`/`M` = move-out −1, `r`/`R` = reuse-release −1
   (perceus mode); anything else (whitespace included) skipped. (`a`/`m`/`r`
   carry ground facts — share-vs-move, reuse-uniqueness — but fold like `i`/`d`,
   so v0 certificates remain valid: i/d is the degenerate case.) *)

Definition newline : ascii := ascii_of_nat 10.

Definition parse_byte (a : ascii) : option Op :=
  if orb (Ascii.eqb a "i"%char) (Ascii.eqb a "I"%char) then Some Inc
  else if orb (Ascii.eqb a "a"%char) (Ascii.eqb a "A"%char) then Some Alias
  else if orb (Ascii.eqb a "d"%char) (Ascii.eqb a "D"%char) then Some Dec
  else if orb (Ascii.eqb a "m"%char) (Ascii.eqb a "M"%char) then Some MoveOut
  else if orb (Ascii.eqb a "r"%char) (Ascii.eqb a "R"%char) then Some Reuse
  else if orb (Ascii.eqb a "b"%char) (Ascii.eqb a "B"%char) then Some Borrow
  else None.

(* Fold the byte string into per-line op streams; flush the final line at end. *)
Fixpoint parse_go (s : string) (cur : list Op) : list (list Op) :=
  match s with
  | EmptyString => [rev cur]
  | String b rest =>
      if Ascii.eqb b newline then rev cur :: parse_go rest []
      else match parse_byte b with
           | Some op => parse_go rest (op :: cur)
           | None => parse_go rest cur
           end
  end.

Definition parse (s : string) : list (list Op) := parse_go s [].

(* The full proven checker over raw certificate bytes. *)
Definition check_cert (s : string) : bool := check_all (parse s).

(* SOUNDNESS over bytes: accepting the certificate bytes guarantees every object
   parsed from them is free of double-free and leak. The tokenizer is now inside
   the proof. *)
Theorem check_cert_sound :
  forall s, check_cert s = true ->
    forall ops, In ops (parse s) -> no_double_free ops /\ no_leak ops.
Proof.
  intros s H. unfold check_cert in H. apply check_all_sound. exact H.
Qed.

Example cert_balanced_accepts : check_cert "iidd"%string = true.
Proof. reflexivity. Qed.

Example cert_double_free_rejects : check_cert "idd"%string = false.
Proof. reflexivity. Qed.

(* a two-object certificate (newline-separated) — exercises line splitting *)
Definition cert_two_objs : string :=
  String "i"%char (String "d"%char (String newline (String "i"%char (String "d"%char EmptyString)))).
Example cert_two_objs_accepts : check_cert cert_two_objs = true.
Proof. reflexivity. Qed.

(* format-v1 alphabet: alias (`a`) and move-out (`m`) fold like inc/dec, so the
   share-vs-move ground fact rides along without changing the balance verdict. *)
Example accepts_alias_then_move : check [Inc; Alias; Dec; MoveOut] = true.
Proof. reflexivity. Qed.

Example cert_move_out_accepts : check_cert "im"%string = true.   (* alloc, move-out (the return_list witness) *)
Proof. reflexivity. Qed.

Example cert_alias_balanced : check_cert "iadd"%string = true.   (* alloc, alias, two releases *)
Proof. reflexivity. Qed.

Example cert_move_out_underflow_rejects : check_cert "m"%string = false. (* move-out with nothing owned = use-after-move *)
Proof. reflexivity. Qed.

(* perceus mode: a reuse-release `r` on a UNIQUE object (rc = 1) — alloc, reuse. *)
Example cert_reuse_balanced : check_cert "ir"%string = true.
Proof. reflexivity. Qed.

(* ─── REUSE SOUNDNESS (the perceus uniqueness obligation, A1 "+reuse健全性") ───
   A `Reuse` event asserts the block is UNIQUELY owned so it can be repurposed in
   place. `reuses_unique` is the decidable property "every Reuse in the run acts
   at rc = 1" — exactly what makes in-place reuse safe (no aliasing owner to
   corrupt). It mirrors `exec` but only watches the Reuse sites. *)
Fixpoint reuses_unique (ops : list Op) (rc : Z) : bool :=
  match ops with
  | [] => true
  | Inc :: rest => reuses_unique rest (rc + 1)
  | Alias :: rest => reuses_unique rest (rc + 1)
  | Dec :: rest => if rc <=? 0 then true else reuses_unique rest (rc - 1)
  | MoveOut :: rest => if rc <=? 0 then true else reuses_unique rest (rc - 1)
  | Reuse :: rest => if Z.eqb rc 1 then reuses_unique rest 0 else false
  (* Borrow mirrors exec: a fault (rc = 0) ends the run (vacuously true), a live
     borrow changes nothing. *)
  | Borrow :: rest => if rc <=? 0 then true else reuses_unique rest rc
  end.

(* The bridge: a run that does NOT fault has every Reuse at rc = 1. This holds
   because `exec`'s Reuse arm FAULTS unless rc = 1 — so a non-faulting run cannot
   contain a shared reuse. (Pure proof-reuse of the tightened fold; no new axiom.) *)
Lemma exec_ok_reuses_unique :
  forall ops rc, exec ops rc <> None -> reuses_unique ops rc = true.
Proof.
  induction ops as [| o rest IH]; intros rc H.
  - reflexivity.
  - destruct o; simpl in *.
    + apply IH. exact H.
    + apply IH. exact H.
    + destruct (rc <=? 0) eqn:E. { exfalso. apply H. reflexivity. } apply IH. exact H.
    + destruct (rc <=? 0) eqn:E. { exfalso. apply H. reflexivity. } apply IH. exact H.
    + destruct (Z.eqb rc 1) eqn:E. { apply IH. exact H. } exfalso. apply H. reflexivity.
    + destruct (rc <=? 0) eqn:E. { exfalso. apply H. reflexivity. } apply IH. exact H.
Qed.

(* REUSE SOUNDNESS: an accepted certificate has every Reuse acting on a UNIQUELY
   owned object — so the compiler's in-place-reuse decision is safe, re-derived by
   the checker's own fold, never trusting the compiler's uniqueness analysis. *)
Theorem check_reuse_sound :
  forall ops, check ops = true -> reuses_unique ops 0 = true.
Proof.
  intros ops H. apply exec_ok_reuses_unique.
  unfold check, run in H. destruct (exec ops 0) eqn:E.
  - intro Hcon. discriminate Hcon.
  - discriminate H.
Qed.

(* Lifted to a whole-function / certificate: every object's reuses are sound. *)
Theorem check_all_reuse_sound :
  forall objs, check_all objs = true ->
    forall ops, In ops objs -> reuses_unique ops 0 = true.
Proof.
  intros objs H ops Hin. unfold check_all in H. rewrite forallb_forall in H.
  apply check_reuse_sound. apply H. exact Hin.
Qed.

Theorem check_cert_reuse_sound :
  forall s, check_cert s = true ->
    forall ops, In ops (parse s) -> reuses_unique ops 0 = true.
Proof.
  intros s H. unfold check_cert in H. apply check_all_reuse_sound. exact H.
Qed.

(* THE CLOSED HOLE (non-vacuous). `iard` = alloc, ALIAS (rc 1→2), REUSE (rc 2),
   release — it BALANCES to 0, so the bare RC-balance checker ACCEPTED it. But it
   reuses a SHARED object in place (an aliasing bug). The uniqueness fold now
   REJECTS it: a reuse at rc = 2 ≠ 1 faults. This is the gate that makes the
   shared-reuse class non-recurring. *)
Example cert_shared_reuse_rejects : check_cert "iard"%string = false.
Proof. reflexivity. Qed.

Example reuses_unique_ir : reuses_unique [Inc; Reuse] 0 = true.
Proof. reflexivity. Qed.

Example reuses_shared_iard_not_unique : reuses_unique [Inc; Alias; Reuse; Dec] 0 = false.
Proof. reflexivity. Qed.

(* ─── the `b` (borrow, +0) letter — brick 5b non-vacuity ───
   A borrow needs a LIVE owned reference and changes nothing: `ibd` accepts.
   A borrow AFTER the last release (`idb`) is a use-after-free — before this
   letter the cert could not witness it (the Borrow op was invisible); now it
   FAULTS. A borrow with nothing ever owned (`b`) faults the same way. *)
Example cert_borrow_live_accepts : check_cert "ibd"%string = true.
Proof. reflexivity. Qed.

Example cert_borrow_after_free_rejects : check_cert "idb"%string = false.
Proof. reflexivity. Qed.

Example cert_borrow_nothing_rejects : check_cert "b"%string = false.
Proof. reflexivity. Qed.

Example borrow_of_alias_accepts : check [Inc; Alias; Borrow; Dec; Dec] = true.
Proof. reflexivity. Qed.

(* ─── HEAP-LOOP-CARRIED extension (option C, the COMPLETENESS fix) ───
   The flat per-object cert above cannot express a loop-carried heap accumulator
   (`acc = acc + [x]` each iteration: drop the old object, alloc a new one, rebind
   the slot — different objects sharing one SLOT, the old's `d` an iteration after
   the new's `i`). So `verify_ownership` FALSE-REJECTS safe `acc + [x]` loops — an
   incompleteness. Here a cert LINE gains a LOOP item whose flat body runs ANY
   number of times; the accumulator slot's cert is `[COp Inc; CLoop [Dec; Inc];
   COp MoveOut]` (acquire once; each iteration release-old + acquire-new = net 0;
   move out the final). The rule: accept a loop iff its body PRESERVES rc (and does
   not fault) from the entry count — then EVERY iteration count is sound, PROVED
   below (`check_line_unroll_sound`). (OwnershipLoop.v proves the same rule on a
   focused alphabet; this is the production port over the full Inc/Alias/Dec/
   MoveOut/Reuse alphabet so the extracted checker accepts loop certs.) *)

Inductive CertItem : Type :=
  | COp : Op -> CertItem            (* a plain op *)
  | CLoop : list Op -> CertItem     (* a loop body run any number of times *)
  | CCondLoop : list Op -> list Op -> CertItem
    (* a CONDITIONAL loop (filter / filter_map): each iteration runs EITHER the
       `then` body (predicate true) OR the `else` body (predicate false). The
       number of `then` iterations is RUNTIME-VARIABLE. Accepted iff BOTH branch
       bodies PRESERVE rc from the entry count — then ANY per-iteration outcome
       sequence preserves it, for ANY iteration count (proved sound vs. the
       concrete unrolling `cond_concat` below). The unconditional `CLoop body`
       is the degenerate case `then = else = body`; this strictly generalizes it.
       Ported from OwnershipFilter.v (`cexec`/`ccheck_unroll_sound`, kernel-checked,
       axiom-clean) onto the production Inc/Alias/Dec/MoveOut/Reuse `exec`. *)
  | CBranch : list Op -> list Op -> CertItem.
    (* a ONE-SHOT branch (brick 5a): the run takes EXACTLY ONE of the two flat
       arm bodies. Accepted iff BOTH arms execute from the entry count WITHOUT
       faulting to the SAME result — the arms AGREE on the leaving resource
       state (which may differ from the entry state: net +1 arms are the whole
       point — a heap-result branch). CCondLoop is the ITERATED cousin (its
       preservation requirement `r = rc` makes any iteration count sound); a
       one-shot branch only needs AGREEMENT. This retires the lowering's
       per-arm-balance TRUSTED convention: cross-arm compensation (an `i` in one
       arm balanced by a `d` in the other — runtime-unsafe either way the branch
       goes, yet flat-balanced) becomes structurally REJECTED. *)

(* exec_line folds a cert LINE. A CLoop body is checked via the existing flat `exec`
   (its body is plain ops); accepted iff the body preserves rc (sufficient for any
   iteration count). Structural recursion on the item list — no nesting (bodies are
   plain `list Op`), so this compiles without the nested-inductive guard issue. *)
Fixpoint exec_line (cs : list CertItem) (rc : Z) : option Z :=
  match cs with
  | [] => Some rc
  | COp o :: rest =>
      match exec [o] rc with
      | Some rc' => exec_line rest rc'
      | None => None
      end
  | CLoop body :: rest =>
      match exec body rc with
      | Some rc' => if Z.eqb rc' rc then exec_line rest rc else None
      | None => None
      end
  | CCondLoop thenb elseb :: rest =>
      (* accept iff BOTH branches preserve rc (and neither faults) from the entry
         count — then any per-iteration choice preserves it (cf. OwnershipFilter
         cexec). *)
      match exec thenb rc, exec elseb rc with
      | Some rt, Some re =>
          if andb (Z.eqb rt rc) (Z.eqb re rc) then exec_line rest rc else None
      | _, _ => None
      end
  | CBranch thenb elseb :: rest =>
      (* one-shot branch: both arms run fault-free from the entry count to the
         SAME result (AGREEMENT, not preservation — the net may be nonzero);
         continue at that agreed count. *)
      match exec thenb rc, exec elseb rc with
      | Some rt, Some re => if Z.eqb rt re then exec_line rest rt else None
      | _, _ => None
      end
  end.

Definition check_line (cs : list CertItem) : bool :=
  match exec_line cs 0 with Some z => Z.eqb z 0 | None => false end.

(* exec distributes over append (over the full alphabet). *)
Lemma exec_app :
  forall a b rc,
    exec (a ++ b) rc =
      match exec a rc with Some rc' => exec b rc' | None => None end.
Proof.
  induction a as [| o a IH]; intros b rc; simpl.
  - reflexivity.
  - destruct o; simpl.
    + apply IH.
    + apply IH.
    + destruct (rc <=? 0). reflexivity. apply IH.
    + destruct (rc <=? 0). reflexivity. apply IH.
    + destruct (Z.eqb rc 1). apply IH. reflexivity.
    + destruct (rc <=? 0). reflexivity. apply IH.
Qed.

(* exec over a cons = single-op step then the rest. *)
Lemma exec_cons :
  forall o b rc,
    exec (o :: b) rc =
      match exec [o] rc with Some rc' => exec b rc' | None => None end.
Proof. intros o b rc. change (o :: b) with ([o] ++ b). apply exec_app. Qed.

(* A flat body that PRESERVES rc, repeated n times, still preserves rc. *)
Lemma exec_repeat_preserve :
  forall body rc, exec body rc = Some rc ->
    forall n, exec (List.concat (List.repeat body n)) rc = Some rc.
Proof.
  intros body rc Hpres. induction n as [| n IH]; simpl.
  - reflexivity.
  - rewrite exec_app, Hpres. exact IH.
Qed.

(* CONCRETE unrolling of a CONDITIONAL loop: a list of bools `bs` (the runtime
   predicate outcomes) selects `thenb` or `elseb` each iteration. The real loop
   runs the SAME two branch bodies, choosing per the data — so this is exactly its
   concrete ownership trace. (Ported from OwnershipFilter.cond_concat over `list Op`.) *)
Fixpoint cond_concat (thenb elseb : list Op) (bs : list bool) : list Op :=
  match bs with
  | [] => []
  | true :: rest => thenb ++ cond_concat thenb elseb rest
  | false :: rest => elseb ++ cond_concat thenb elseb rest
  end.

(* Two rc-preserving branches ⇒ ANY choice sequence preserves rc and never faults.
   (Ported from OwnershipFilter.cond_concat_preserve; exec_flat → exec, exec_flat_app
   → exec_app.) *)
Lemma cond_concat_preserve :
  forall thenb elseb rc,
    exec thenb rc = Some rc ->
    exec elseb rc = Some rc ->
    forall bs, exec (cond_concat thenb elseb bs) rc = Some rc.
Proof.
  intros thenb elseb rc Ht He.
  induction bs as [| b bs IH]; simpl.
  - reflexivity.
  - destruct b.
    + rewrite exec_app, Ht. exact IH.
    + rewrite exec_app, He. exact IH.
Qed.

(* CONCRETE unrolling: a cert line unrolls to a flat run (each CLoop body → n copies). *)
Inductive UnrollsL : list CertItem -> list Op -> Prop :=
  | UL_nil : UnrollsL [] []
  | UL_op : forall o a b, UnrollsL a b -> UnrollsL (COp o :: a) (o :: b)
  | UL_loop : forall body a b n,
      UnrollsL a b -> UnrollsL (CLoop body :: a) (List.concat (List.repeat body n) ++ b)
  | UL_cond : forall thenb elseb a b bs,
      UnrollsL a b ->
      UnrollsL (CCondLoop thenb elseb :: a) (cond_concat thenb elseb bs ++ b)
  (* a one-shot branch unrolls to EXACTLY ONE arm — the runtime choice. *)
  | UL_branch : forall thenb elseb a b (choice : bool),
      UnrollsL a b ->
      UnrollsL (CBranch thenb elseb :: a) ((if choice then thenb else elseb) ++ b).

(* SOUNDNESS CORE: an accepting line, at any rc, executes EVERY unrolling to the
   same result — so no unrolling faults and the final rc matches. *)
Lemma exec_line_unroll :
  forall cs fops, UnrollsL cs fops ->
    forall rc r, exec_line cs rc = Some r -> exec fops rc = Some r.
Proof.
  intros cs fops HU. induction HU; intros rc r Hexec.
  - (* nil *) simpl in *. exact Hexec.
  - (* COp o — case on the concrete op so exec/exec_line both reduce *)
    destruct o; simpl in *.
    + apply IHHU; exact Hexec.
    + apply IHHU; exact Hexec.
    + destruct (rc <=? 0); [discriminate | apply IHHU; exact Hexec].
    + destruct (rc <=? 0); [discriminate | apply IHHU; exact Hexec].
    + destruct (Z.eqb rc 1); [apply IHHU; exact Hexec | discriminate].
    + destruct (rc <=? 0); [discriminate | apply IHHU; exact Hexec].
  - (* CLoop body *) simpl in *.
    destruct (exec body rc) as [rc' |] eqn:Eb; [| discriminate].
    destruct (Z.eqb rc' rc) eqn:Eq; [| discriminate].
    apply Z.eqb_eq in Eq. subst rc'.
    rewrite exec_app, (exec_repeat_preserve body rc Eb n).
    apply IHHU. exact Hexec.
  - (* CCondLoop thenb elseb — copy of OwnershipFilter cexec_unroll's loop case *)
    simpl in *.
    destruct (exec thenb rc) as [rt |] eqn:Et; [| discriminate].
    destruct (exec elseb rc) as [re |] eqn:Ee; [| discriminate].
    destruct (andb (Z.eqb rt rc) (Z.eqb re rc)) eqn:Eb; [| discriminate].
    apply andb_prop in Eb. destruct Eb as [Hrt Hre].
    apply Z.eqb_eq in Hrt. apply Z.eqb_eq in Hre. subst rt re.
    rewrite exec_app, (cond_concat_preserve thenb elseb rc Et Ee bs).
    apply IHHU. exact Hexec.
  - (* CBranch thenb elseb — the run takes ONE arm; both agree on the result,
       so either way the line continues at the same count. *)
    simpl in *.
    destruct (exec thenb rc) as [rt |] eqn:Et; [| discriminate].
    destruct (exec elseb rc) as [re |] eqn:Ee; [| discriminate].
    destruct (Z.eqb rt re) eqn:Eq; [| discriminate].
    apply Z.eqb_eq in Eq. subst re.
    rewrite exec_app. destruct choice.
    + rewrite Et. apply IHHU. exact Hexec.
    + rewrite Ee. apply IHHU. exact Hexec.
Qed.

(* The headline: an ACCEPTED loop cert line guarantees EVERY concrete unrolling is
   free of double-free / use-after-free AND leak — the completeness the flat cert
   lacked, now SOUND (the false-rejection closed at the root). *)
Theorem check_line_unroll_sound :
  forall cs, check_line cs = true ->
    forall fops, UnrollsL cs fops ->
      run fops <> None /\ run fops = Some 0.
Proof.
  intros cs H fops HU. unfold check_line in H. unfold run.
  destruct (exec_line cs 0) as [z |] eqn:E; [| discriminate].
  apply Z.eqb_eq in H. subst z.
  rewrite (exec_line_unroll cs fops HU 0 0 E). split. discriminate. reflexivity.
Qed.

(* non-vacuity: the accumulator slot accepts; a leaky/draining loop body is rejected. *)
Example acc_slot_line_accepts : check_line [COp Inc; CLoop [Dec; Inc]; COp MoveOut] = true.
Proof. reflexivity. Qed.
Example leaky_loop_line_rejects : check_line [COp Inc; CLoop [Inc]; COp MoveOut] = false.
Proof. reflexivity. Qed.
Example draining_loop_line_rejects : check_line [COp Inc; CLoop [Dec]; COp MoveOut] = false.
Proof. reflexivity. Qed.

(* CONDITIONAL-loop (filter slot) non-vacuity: acquire once; each iteration EITHER
   drop-old+alloc-new (predicate true, net 0) OR nothing (predicate false, net 0);
   move out the final. ACCEPTS — the runtime-variable #appends is irrelevant. *)
Example filter_slot_line_accepts :
  check_line [COp Inc; CCondLoop [Dec; Inc] []; COp MoveOut] = true.
Proof. reflexivity. Qed.
(* THEN branch leaks (net +1) — REJECT. *)
Example filter_leaky_then_line_rejects :
  check_line [COp Inc; CCondLoop [Inc] []; COp MoveOut] = false.
Proof. reflexivity. Qed.
(* ELSE branch drains (net −1) — REJECT. *)
Example filter_draining_else_line_rejects :
  check_line [COp Inc; CCondLoop [Dec; Inc] [Dec]; COp MoveOut] = false.
Proof. reflexivity. Qed.

(* ONE-SHOT branch (brick 5a) non-vacuity. AGREEMENT at net +1: both arms
   acquire (a heap-result branch — the merge value), released twice after.
   entry 0 → `i` → 1; arms 1→2 AGREE; `dd` → 0. ACCEPTS. *)
Example branch_agree_net_plus_one_accepts :
  check_line [COp Inc; CBranch [Inc] [Inc]; COp Dec; COp Dec] = true.
Proof. reflexivity. Qed.
(* Arms DISAGREE (+1 vs 0): whichever way the branch goes, the line's later
   accounting is wrong for the other — REJECT. *)
Example branch_disagree_rejects :
  check_line [COp Inc; CBranch [Inc] []; COp Dec] = false.
Proof. reflexivity. Qed.
(* CROSS-ARM COMPENSATION (the closed hole): `i` in one arm, `d` in the other.
   FLAT the two would balance (the accept-but-unsafe class the per-arm-balance
   convention had to promise away); grouped, the `d` arm faults from entry 0 —
   REJECT, structurally. *)
Example branch_cross_arm_compensation_rejects :
  check_line [CBranch [Inc] [Dec]] = false.
Proof. reflexivity. Qed.
(* Both arms fault — REJECT. *)
Example branch_both_arms_fault_rejects :
  check_line [CBranch [Dec] [Dec]] = false.
Proof. reflexivity. Qed.

(* ─── LOOP-AWARE certificate parsing (format v2, backward-compatible) ───
   Extends the byte format with loop delimiters `(` … `)` around a flat loop body:
   e.g. `i(di)m` = COp Inc, CLoop [Dec; Inc], COp MoveOut (the accumulator slot).
   A cert with NO parens parses to all-`COp` lines, and `check_line` on those folds
   exactly like the flat `check` — so every existing v1 certificate is unchanged.
   The op alphabet inside/outside a loop is the same i/a/d/m/r. *)
Definition lparen : ascii := "("%char.
Definition rparen : ascii := ")"%char.

(* Flush the in-progress line to a finished `list CertItem` (closing a dangling
   loop defensively, though well-formed certs close every `(` before newline/EOF). *)
Definition finish_line (cur : list CertItem) (lp : option (list Op)) : list CertItem :=
  match lp with
  | Some body => rev (CLoop (rev body) :: cur)
  | None => rev cur
  end.

Fixpoint parse_lc (s : string) (cur : list CertItem) (lp : option (list Op))
                  {struct s} : list (list CertItem) :=
  match s with
  | EmptyString => [finish_line cur lp]
  | String b rest =>
      if Ascii.eqb b newline then finish_line cur lp :: parse_lc rest [] None
      else if Ascii.eqb b lparen then parse_lc rest cur (Some [])
      else if Ascii.eqb b rparen then
        match lp with
        | Some body => parse_lc rest (CLoop (rev body) :: cur) None
        | None => parse_lc rest cur None
        end
      else match parse_byte b with
           | Some op =>
               match lp with
               | Some body => parse_lc rest cur (Some (op :: body))
               | None => parse_lc rest (COp op :: cur) None
               end
           | None => parse_lc rest cur lp
           end
  end.

(* The full loop-aware checker over raw certificate bytes. *)
Definition check_cert_lc (s : string) : bool :=
  forallb check_line (parse_lc s [] None).

(* SOUNDNESS over bytes: an accepted loop certificate has, for EVERY parsed line and
   EVERY concrete unrolling of that line, no double-free / use-after-free and no
   leak. (Per-line via check_line_unroll_sound; lifted over all lines here.) *)
Theorem check_cert_lc_sound :
  forall s, check_cert_lc s = true ->
    forall cs, In cs (parse_lc s [] None) ->
      forall fops, UnrollsL cs fops -> run fops <> None /\ run fops = Some 0.
Proof.
  intros s H cs Hin fops HU.
  unfold check_cert_lc in H. rewrite forallb_forall in H.
  apply (check_line_unroll_sound cs (H cs Hin) fops HU).
Qed.

(* backward-compat + non-vacuity on real bytes *)
Example cert_lc_flat_accepts : check_cert_lc "iidd"%string = true.
Proof. reflexivity. Qed.
Example cert_lc_flat_rejects : check_cert_lc "idd"%string = false.
Proof. reflexivity. Qed.
Example cert_lc_acc_slot_accepts : check_cert_lc "i(di)m"%string = true.   (* accumulator slot *)
Proof. reflexivity. Qed.
Example cert_lc_leaky_loop_rejects : check_cert_lc "i(i)m"%string = false. (* loop body leaks *)
Proof. reflexivity. Qed.
Example cert_lc_draining_loop_rejects : check_cert_lc "i(d)m"%string = false. (* loop body drains *)
Proof. reflexivity. Qed.

(* ─── CONDITIONAL-LOOP-aware certificate parsing (format v3, backward-compatible) ───
   Extends format v2 with conditional-loop delimiters `[` then `|` else `]` around
   two flat branch bodies: e.g. `i[di|]m` = COp Inc, CCondLoop [Dec; Inc] [], COp
   MoveOut (the filter accumulator slot). `parse_clc` is a SUPERSET of `parse_lc`:
   it ALSO handles the `(` … `)` loop form, and a cert with NO `[` parses byte-for-
   byte as `parse_lc` (the `|` / `]` bytes are not in the op alphabet, so outside a
   `[` they are skipped exactly as `parse_lc` skips them) — full backward compat for
   every flat and CLoop certificate. The op alphabet inside a branch is the same
   i/a/d/m/r. Conditional-loop bodies are FLAT (no nested `(`/`[`). *)
Definition lbracket : ascii := "["%char.
Definition rbracket : ascii := "]"%char.
Definition bar : ascii := "|"%char.

(* in-progress conditional-loop state: (collecting-else?, then-acc-rev, else-acc-rev). *)
Definition condst : Type := (bool * list Op * list Op)%type.

(* Flush the in-progress line, defensively closing a dangling `(`-loop or `[`-cond. *)
Definition finish_line_c (cur : list CertItem) (lp : option (list Op))
                         (cp : option condst) : list CertItem :=
  match cp with
  | Some (_, th, el) => rev (CCondLoop (rev th) (rev el) :: cur)
  | None =>
      match lp with
      | Some body => rev (CLoop (rev body) :: cur)
      | None => rev cur
      end
  end.

Fixpoint parse_clc (s : string) (cur : list CertItem)
                   (lp : option (list Op)) (cp : option condst)
                   {struct s} : list (list CertItem) :=
  match s with
  | EmptyString => [finish_line_c cur lp cp]
  | String b rest =>
      if Ascii.eqb b newline then finish_line_c cur lp cp :: parse_clc rest [] None None
      else match cp with
      | Some (in_else, th, el) =>
          (* inside `[ … | … ]` — collect ops into the then/else branch *)
          if Ascii.eqb b bar then parse_clc rest cur lp (Some (true, th, el))
          else if Ascii.eqb b rbracket then
            parse_clc rest (CCondLoop (rev th) (rev el) :: cur) lp None
          else match parse_byte b with
               | Some op =>
                   if in_else then parse_clc rest cur lp (Some (true, th, op :: el))
                   else parse_clc rest cur lp (Some (false, op :: th, el))
               | None => parse_clc rest cur lp cp
               end
      | None =>
          if Ascii.eqb b lbracket then parse_clc rest cur lp (Some (false, [], []))
          else if Ascii.eqb b lparen then parse_clc rest cur (Some []) None
          else if Ascii.eqb b rparen then
            match lp with
            | Some body => parse_clc rest (CLoop (rev body) :: cur) None None
            | None => parse_clc rest cur None None
            end
          else match parse_byte b with
               | Some op =>
                   match lp with
                   | Some body => parse_clc rest cur (Some (op :: body)) None
                   | None => parse_clc rest (COp op :: cur) None None
                   end
               | None => parse_clc rest cur lp None
               end
      end
  end.

(* The full conditional-loop-aware checker over raw certificate bytes. *)
Definition check_clc (s : string) : bool :=
  forallb check_line (parse_clc s [] None None).

(* SOUNDNESS over bytes (1-line corollary of check_line_unroll_sound, now covering
   CCondLoop automatically via UL_cond + the exec_line_unroll CCondLoop case): an
   accepted conditional-loop certificate has, for EVERY parsed line and EVERY
   concrete unrolling, no double-free / use-after-free and no leak. *)
Theorem check_clc_unroll_sound :
  forall s, check_clc s = true ->
    forall cs, In cs (parse_clc s [] None None) ->
      forall fops, UnrollsL cs fops -> run fops <> None /\ run fops = Some 0.
Proof.
  intros s H cs Hin fops HU.
  unfold check_clc in H. rewrite forallb_forall in H.
  apply (check_line_unroll_sound cs (H cs Hin) fops HU).
Qed.

(* backward-compat (flat + `(`-loop certs parse/verify exactly as before) + the new
   conditional-loop (filter) certs, on real bytes *)
Example cert_clc_flat_accepts : check_clc "iidd"%string = true.
Proof. reflexivity. Qed.
Example cert_clc_flat_rejects : check_clc "idd"%string = false.
Proof. reflexivity. Qed.
Example cert_clc_loop_accepts : check_clc "i(di)m"%string = true.        (* v2 CLoop still accepts *)
Proof. reflexivity. Qed.
Example cert_clc_filter_slot_accepts : check_clc "i[di|]m"%string = true.   (* filter accumulator slot *)
Proof. reflexivity. Qed.
Example cert_clc_leaky_then_rejects : check_clc "i[i|]m"%string = false.    (* then branch leaks *)
Proof. reflexivity. Qed.
Example cert_clc_draining_else_rejects : check_clc "i[di|d]m"%string = false. (* else branch drains *)
Proof. reflexivity. Qed.

(* ─── BRANCH-aware certificate parsing (format v4, backward-compatible) ───
   Extends format v3 with ONE-SHOT branch delimiters `{` then `|` else `}` around
   two flat arm bodies: e.g. `i{i|i}dd` = COp Inc, CBranch [Inc] [Inc], COp Dec,
   COp Dec (a heap-result branch — both arms acquire, arms AGREE at net +1).
   `parse_bc` is a SUPERSET of `parse_clc`: it also handles `(`…`)` loops and
   `[`…`|`…`]` conditional loops, and a cert with NO `{` parses byte-for-byte as
   `parse_clc` (`{`/`}` are outside the op alphabet — previously skipped bytes) —
   full backward compat for every flat, CLoop and CCondLoop certificate. Arm
   bodies are FLAT (no nesting), like loop bodies. *)
Definition lbrace : ascii := "{"%char.
Definition rbrace : ascii := "}"%char.

(* Flush the in-progress line, defensively closing a dangling `{`-branch first
   (well-formed certs close every `{` before newline/EOF), else deferring to the
   format-v3 flush. *)
Definition finish_line_b (cur : list CertItem) (lp : option (list Op))
                         (cp bp : option condst) : list CertItem :=
  match bp with
  | Some (_, th, el) => rev (CBranch (rev th) (rev el) :: cur)
  | None => finish_line_c cur lp cp
  end.

Fixpoint parse_bc (s : string) (cur : list CertItem)
                  (lp : option (list Op)) (cp bp : option condst)
                  {struct s} : list (list CertItem) :=
  match s with
  | EmptyString => [finish_line_b cur lp cp bp]
  | String b rest =>
      if Ascii.eqb b newline then finish_line_b cur lp cp bp :: parse_bc rest [] None None None
      else match bp with
      | Some (in_else, th, el) =>
          (* inside `{ … | … }` — collect ops into the then/else arm *)
          if Ascii.eqb b bar then parse_bc rest cur lp cp (Some (true, th, el))
          else if Ascii.eqb b rbrace then
            parse_bc rest (CBranch (rev th) (rev el) :: cur) lp cp None
          else match parse_byte b with
               | Some op =>
                   if in_else then parse_bc rest cur lp cp (Some (true, th, op :: el))
                   else parse_bc rest cur lp cp (Some (false, op :: th, el))
               | None => parse_bc rest cur lp cp bp
               end
      | None =>
          match cp with
          | Some (in_else, th, el) =>
              (* inside `[ … | … ]` — exactly parse_clc's conditional-loop state *)
              if Ascii.eqb b bar then parse_bc rest cur lp (Some (true, th, el)) None
              else if Ascii.eqb b rbracket then
                parse_bc rest (CCondLoop (rev th) (rev el) :: cur) lp None None
              else match parse_byte b with
                   | Some op =>
                       if in_else then parse_bc rest cur lp (Some (true, th, op :: el)) None
                       else parse_bc rest cur lp (Some (false, op :: th, el)) None
                   | None => parse_bc rest cur lp cp None
                   end
          | None =>
              if Ascii.eqb b lbrace then parse_bc rest cur lp None (Some (false, [], []))
              else if Ascii.eqb b lbracket then parse_bc rest cur lp (Some (false, [], [])) None
              else if Ascii.eqb b lparen then parse_bc rest cur (Some []) None None
              else if Ascii.eqb b rparen then
                match lp with
                | Some body => parse_bc rest (CLoop (rev body) :: cur) None None None
                | None => parse_bc rest cur None None None
                end
              else match parse_byte b with
                   | Some op =>
                       match lp with
                       | Some body => parse_bc rest cur (Some (op :: body)) None None
                       | None => parse_bc rest (COp op :: cur) None None None
                       end
                   | None => parse_bc rest cur lp None None
                   end
          end
      end
  end.

(* The full branch-aware checker over raw certificate bytes (format v4). *)
Definition check_bc (s : string) : bool :=
  forallb check_line (parse_bc s [] None None None).

(* SOUNDNESS over bytes (1-line corollary of check_line_unroll_sound, covering
   CBranch via UL_branch + the exec_line_unroll CBranch case): an accepted
   branch certificate has, for EVERY parsed line and EVERY concrete unrolling —
   each branch resolved to the ONE arm the runtime takes — no double-free /
   use-after-free and no leak. *)
Theorem check_bc_unroll_sound :
  forall s, check_bc s = true ->
    forall cs, In cs (parse_bc s [] None None None) ->
      forall fops, UnrollsL cs fops -> run fops <> None /\ run fops = Some 0.
Proof.
  intros s H cs Hin fops HU.
  unfold check_bc in H. rewrite forallb_forall in H.
  apply (check_line_unroll_sound cs (H cs Hin) fops HU).
Qed.

(* backward-compat (flat + loop + cond-loop certs verify exactly as before) +
   the new one-shot branch certs, on real bytes *)
Example cert_bc_flat_accepts : check_bc "iidd"%string = true.
Proof. reflexivity. Qed.
Example cert_bc_flat_rejects : check_bc "idd"%string = false.
Proof. reflexivity. Qed.
Example cert_bc_loop_accepts : check_bc "i(di)m"%string = true.          (* v2 CLoop still accepts *)
Proof. reflexivity. Qed.
Example cert_bc_filter_accepts : check_bc "i[di|]m"%string = true.       (* v3 CCondLoop still accepts *)
Proof. reflexivity. Qed.
Example cert_bc_borrow_accepts : check_bc "ibd"%string = true.           (* 5b `b` rides format v4 *)
Proof. reflexivity. Qed.
Example cert_bc_branch_accepts : check_bc "i{i|i}dd"%string = true.      (* heap-result branch: arms agree at +1 *)
Proof. reflexivity. Qed.
Example cert_bc_branch_disagree_rejects : check_bc "i{i|}d"%string = false. (* arms disagree (+1 vs 0) *)
Proof. reflexivity. Qed.
Example cert_bc_cross_arm_rejects : check_bc "{i|d}"%string = false.     (* cross-arm compensation faults *)
Proof. reflexivity. Qed.

(* AXIOM AUDIT (the "Print Assumptions ⊆ standard" gate). Soundness must rest on
   nothing but the Coq kernel — no admits, no extra axioms. Expected output:
   "Closed under the global context". *)
Print Assumptions check_sound.
Print Assumptions check_line_unroll_sound.
Print Assumptions check_cert_lc_sound.
Print Assumptions check_clc_unroll_sound.
Print Assumptions check_bc_unroll_sound.
Print Assumptions check_all_sound.
Print Assumptions check_cert_sound.
Print Assumptions check_reuse_sound.
Print Assumptions check_all_reuse_sound.
Print Assumptions check_cert_reuse_sound.

