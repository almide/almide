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
     Reuse   = −1 REUSE-eligible release (the PERCEUS mode): a release the
               compiler proved acts on a UNIQUELY-owned object, so the freed
               block may be reused IN PLACE. Folds like Dec (−1); the separate
               constructor records the uniqueness obligation (checked by a
               membership-subset section: r-objects ⊆ proven-unique).
   (Borrow b≡+0, the closure-env mode, is the remaining letter.) *)
Inductive Op : Type :=
  | Inc : Op
  | Alias : Op
  | Dec : Op
  | MoveOut : Op
  | Reuse : Op.

(* OPERATIONAL SEMANTICS (the ALS side — "what actually happens").
   A refcount, or a FAULT (`None`) when a −1 op hits rc = 0: that is a
   double-free / use-after-free — releasing a reference that does not exist.
   Alias folds like Inc (+1), MoveOut like Dec (−1): the balance is about the
   DELTAS, which is exactly why adding the ground-fact constructors costs no new
   proof. *)
Fixpoint exec (ops : list Op) (rc : Z) : option Z :=
  match ops with
  | [] => Some rc
  | Inc :: rest => exec rest (rc + 1)
  | Alias :: rest => exec rest (rc + 1)
  | Dec :: rest => if rc <=? 0 then None else exec rest (rc - 1)
  | MoveOut :: rest => if rc <=? 0 then None else exec rest (rc - 1)
  | Reuse :: rest => if rc <=? 0 then None else exec rest (rc - 1)
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

(* perceus mode: a reuse-release `r` folds like a plain release — alloc, reuse. *)
Example cert_reuse_balanced : check_cert "ir"%string = true.
Proof. reflexivity. Qed.

(* AXIOM AUDIT (the "Print Assumptions ⊆ standard" gate). Soundness must rest on
   nothing but the Coq kernel — no admits, no extra axioms. Expected output:
   "Closed under the global context". *)
Print Assumptions check_sound.
Print Assumptions check_all_sound.
Print Assumptions check_cert_sound.

