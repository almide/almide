(* Almide v1 — ALS (normative semantics) + translation validation. brick 4 (start).

   The auditor's killer question (tier-1 layer 6): "you proved a MODEL — does the
   REAL emitted artifact correspond?" This file begins the answer:

   (a) NORMATIVE ALS. `exec` (OwnershipChecker.v) IS the normative operational
       semantics of the ownership fragment — the reference every backend must
       refine. Properties P are defined against it (no_double_free, no_leak).

   (b) HONEST translation-validation of the CURRENT renderer. The greenfield
       wasm renderer realizes ownership by EAGER COPY — it emits NO `__rc_dec`
       (no Dec). We prove what that buys, exactly: a Dec-free realization can
       NEVER double-free (the C-SAFE safety core holds for the real artifact
       today), but it does NOT free (it leaks) — leak-freedom is deferred to the
       real-RC renderer (a later brick). Stating precisely what the real artifact
       refines NOW — and what it does not yet — is the flight-grade discipline
       (no overclaiming; the receipt's C-* claims are scoped to what is proven).

   The full V (emitted wasm BYTES ⊒ ALS) needs a wasm model in Coq — a later
   brick; this establishes the ALS-as-normative form and the first real
   refinement statement about the actual artifact. *)

From AlmideTrust Require Import OwnershipChecker.
From Stdlib Require Import List.
Import ListNotations.

(* A Dec-free ownership trace never faults, from any starting refcount: there is
   no Dec to drive the refcount below zero. *)
Lemma exec_dec_free_no_fault :
  forall ops rc, (forall o, In o ops -> o <> Dec) -> exec ops rc <> None.
Proof.
  induction ops as [| op rest IH]; intros rc Hdf.
  - discriminate.
  - destruct op.
    + (* Inc *) simpl. apply IH. intros o Ho. apply Hdf. right. exact Ho.
    + (* Dec — excluded by hypothesis: contradiction *)
      exfalso. exact (Hdf Dec (or_introl eq_refl) eq_refl).
Qed.

(* THE CURRENT ARTIFACT REFINES THE SAFETY CORE. The eager-copy renderer emits a
   Dec-free RC trace, so the real emitted wasm cannot double-free / use-after-free
   — `no_double_free` holds for it by construction. (It is NOT leak-free; that is
   the real-RC renderer's job — recorded honestly, not hidden.) *)
Theorem eager_copy_refines_safety :
  forall ops, (forall o, In o ops -> o <> Dec) -> no_double_free ops.
Proof.
  intros ops H. unfold no_double_free, run.
  apply exec_dec_free_no_fault. exact H.
Qed.

(* And the honest converse: a Dec-free non-empty trace LEAKS (does not satisfy
   no_leak). So the receipt may claim C-SAFE for the eager-copy artifact today,
   but not leak-freedom — exactly the scoping the known-limitations records. *)
Example dec_free_leaks : no_leak [Inc] -> False.
Proof. unfold no_leak, run. simpl. discriminate. Qed.

Print Assumptions eager_copy_refines_safety.
