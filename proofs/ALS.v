(* Almide v1 — ALS (normative semantics) + translation validation. brick 4 (start).

   The auditor's killer question (tier-1 layer 6): "you proved a MODEL — does the
   REAL emitted artifact correspond?" This file begins the answer:

   (a) NORMATIVE ALS. `exec` (OwnershipChecker.v) IS the normative operational
       semantics of the ownership fragment — the reference every backend must
       refine. Properties P are defined against it (no_double_free, no_leak).

   (b) HONEST translation-validation of the CURRENT renderer. The greenfield
       wasm renderer realizes ownership by EAGER COPY — it clones at every bind
       and emits NO release op (no `__rc_dec`; no Dec, no MoveOut): its actual
       trace is INCREMENT-ONLY. We prove what that buys, exactly: an
       increment-only realization can NEVER double-free (the C-SAFE safety core
       holds for the real artifact today), but it does NOT free (it leaks) —
       leak-freedom is deferred to the real-RC renderer (a later brick). Stating
       precisely what the real artifact refines NOW — and what it does not yet —
       is the flight-grade discipline (no overclaiming; the receipt's C-* claims
       are scoped to what is proven). NOTE: this is about the eager-copy ARTIFACT
       trace (increment-only); the WITNESS may carry the full accounting (with
       `m`/`d` releases) — a different object, checked by `check_all`.

   The full V (emitted wasm BYTES ⊒ ALS) needs a wasm model in Coq — a later
   brick; this establishes the ALS-as-normative form and the first real
   refinement statement about the actual artifact. *)

From AlmideTrust Require Import OwnershipChecker.
From Stdlib Require Import List.
Import ListNotations.

(* The eager-copy property: every op is an ACQUIRE (Inc fresh / Alias) — no
   release (Dec) and no move-out (MoveOut). Both release ops are the −1s that
   could drive the refcount below zero; the eager-copy renderer emits neither. *)
Definition increments_only (ops : list Op) : Prop :=
  forall o, In o ops -> o = Inc \/ o = Alias.

(* An increment-only ownership trace never faults, from any starting refcount:
   there is no −1 op to drive the refcount below zero. *)
Lemma exec_inc_only_no_fault :
  forall ops rc, increments_only ops -> exec ops rc <> None.
Proof.
  induction ops as [| op rest IH]; intros rc Hinc.
  - discriminate.
  - assert (Hrest : increments_only rest).
    { intros o Ho. apply Hinc. right. exact Ho. }
    destruct op.
    + (* Inc *) simpl. apply IH. exact Hrest.
    + (* Alias *) simpl. apply IH. exact Hrest.
    + (* Dec — excluded by hypothesis *)
      exfalso. destruct (Hinc Dec (or_introl eq_refl)) as [H | H]; discriminate.
    + (* MoveOut — excluded by hypothesis *)
      exfalso. destruct (Hinc MoveOut (or_introl eq_refl)) as [H | H]; discriminate.
    + (* Reuse — excluded by hypothesis *)
      exfalso. destruct (Hinc Reuse (or_introl eq_refl)) as [H | H]; discriminate.
    + (* Borrow — excluded by hypothesis *)
      exfalso. destruct (Hinc Borrow (or_introl eq_refl)) as [H | H]; discriminate.
Qed.

(* THE CURRENT ARTIFACT REFINES THE SAFETY CORE. The eager-copy renderer emits an
   increment-only RC trace, so the real emitted wasm cannot double-free /
   use-after-free — `no_double_free` holds for it by construction. (It is NOT
   leak-free; that is the real-RC renderer's job — recorded honestly, not
   hidden.) *)
Theorem eager_copy_refines_safety :
  forall ops, increments_only ops -> no_double_free ops.
Proof.
  intros ops H. unfold no_double_free, run.
  apply exec_inc_only_no_fault. exact H.
Qed.

(* And the honest converse: an increment-only non-empty trace LEAKS (does not
   satisfy no_leak). So the receipt may claim C-SAFE for the eager-copy artifact
   today, but not leak-freedom — exactly the scoping the known-limitations
   records. *)
Example dec_free_leaks : no_leak [Inc] -> False.
Proof. unfold no_leak, run. simpl. discriminate. Qed.

Print Assumptions eager_copy_refines_safety.
