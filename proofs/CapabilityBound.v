(* Almide v1 trust spine — 4th property (brick 5): CAPABILITY BOUND.

   This is the trust-layer's headline promise: a program uses ONLY the
   capabilities it declares — "this does X and nothing else" / "no network".
   It is the foundation of the receipt's sandbox claim. The checker accepts iff
   every USED capability is in the DECLARED allowlist; soundness: accept ⟹ no
   undeclared capability is used.

   It is the shared membership-SUBSET law (Subset.v) at one naming:
   used ⊆ allowed. Capabilities are nat ids from a fixed registry the compiler
   emits (Stdout = 0, …; the registry mapping lives in almide-mir's
   `Capability::id` and MUST match it). An auditor reads the receipt and knows
   the artifact cannot reach a capability outside the declared set. *)

From Stdlib Require Import List.
Import ListNotations.
From AlmideTrust Require Import Subset.
From Stdlib Require Import String.

Record CapWitness := { allowed : list nat; used_caps : list nat }.

(* THE CHECKER: every used capability is in the declared allowlist. *)
Definition check_caps (w : CapWitness) : bool := subset_check (allowed w) (used_caps w).

(* THE PROPERTY: the artifact stays within its declared capability bound. *)
Definition within_bound (w : CapWitness) : Prop := subset_prop (allowed w) (used_caps w).

(* SOUNDNESS: acceptance guarantees no undeclared capability (the shared law). *)
Theorem check_caps_sound :
  forall w, check_caps w = true -> within_bound w.
Proof. intros w. apply subset_check_sound. Qed.

(* non-vacuous: a "no network" program (network = cap 0 not in allowlist) is
   accepted when it uses only fs-read (cap 1); rejected if it reaches network. *)
Example accepts_within_sandbox :
  check_caps {| allowed := [1; 2]; used_caps := [1] |} = true.
Proof. reflexivity. Qed.

Example rejects_undeclared_network :
  check_caps {| allowed := [1; 2]; used_caps := [1; 0] |} = false.
Proof. reflexivity. Qed.

(* end-to-end: the witness is `<allowed ids>|<used ids>` (Subset's parser). *)
Definition parse_cap_witness (s : string) : CapWitness :=
  let (a, u) := parse_pair s in {| allowed := a; used_caps := u |}.

Definition check_caps_cert (s : string) : bool := check_caps (parse_cap_witness s).

Theorem check_caps_cert_sound :
  forall s, check_caps_cert s = true -> within_bound (parse_cap_witness s).
Proof.
  intros s H. apply check_caps_sound. exact H.
Qed.

Example cert_within : check_caps_cert "0 1|0" = true.
Proof. reflexivity. Qed.
Example cert_undeclared : check_caps_cert "1 2|0" = false.
Proof. reflexivity. Qed.

Print Assumptions check_caps_sound.
Print Assumptions check_caps_cert_sound.
