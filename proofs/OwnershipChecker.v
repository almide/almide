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

Require Import List.
Import ListNotations.
Require Import ZArith.
Open Scope Z_scope.

(* The MIR ownership ops on a single reference-counted object. Inc = the +1 of
   Alloc/Dup; Dec = the −1 of Drop/Consume. (Multi-object + Borrow/MakeUnique are
   later refinements; this is the irreducible RC-balance core.) *)
Inductive Op : Type :=
  | Inc : Op
  | Dec : Op.

(* OPERATIONAL SEMANTICS (the ALS side — "what actually happens").
   A refcount, or a FAULT (`None`) when a Dec hits rc = 0: that is a double-free
   / use-after-free — releasing a reference that does not exist. *)
Fixpoint exec (ops : list Op) (rc : Z) : option Z :=
  match ops with
  | [] => Some rc
  | Inc :: rest => exec rest (rc + 1)
  | Dec :: rest => if rc <=? 0 then None else exec rest (rc - 1)
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
