(* Almide v1 trust spine — 2nd property (critical-path brick 5): NAME TOTALITY.

   A reference to an undefined name (a dangling MIR ValueId, an unresolved
   symbol) is undefined behavior / a link error. This is the second flight-grade
   property, after RC balance.

   It is exactly the shared membership-SUBSET law (Subset.v) at one naming:
   used ⊆ defined. The checker, soundness theorem, and witness parser come from
   Subset; here we only give them the name-totality reading. *)

From Stdlib Require Import List.
Import ListNotations.
From AlmideTrust Require Import Subset.
From Stdlib Require Import String.

(* A name-totality witness: the defined names and the used names. *)
Record NameWitness := { defined : list nat; used : list nat }.

(* THE CHECKER: every used name is defined. *)
Definition check_names (w : NameWitness) : bool := subset_check (defined w) (used w).

(* THE PROPERTY: no dangling reference. *)
Definition no_dangling (w : NameWitness) : Prop := subset_prop (defined w) (used w).

(* SOUNDNESS: acceptance guarantees totality (the shared law, named). *)
Theorem check_names_sound :
  forall w, check_names w = true -> no_dangling w.
Proof. intros w. apply subset_check_sound. Qed.

(* non-vacuous: accepts resolved witnesses, rejects a dangling reference. *)
Example accepts_resolved :
  check_names {| defined := [1; 2; 3]; used := [1; 3] |} = true.
Proof. reflexivity. Qed.

Example rejects_dangling :
  check_names {| defined := [1; 2]; used := [1; 5] |} = false.
Proof. reflexivity. Qed.

(* end-to-end: the witness is `<defined ids>|<used ids>` (Subset's parser). *)
Definition parse_name_witness (s : string) : NameWitness :=
  let (d, u) := parse_pair s in {| defined := d; used := u |}.

Definition check_names_cert (s : string) : bool := check_names (parse_name_witness s).

Theorem check_names_cert_sound :
  forall s, check_names_cert s = true -> no_dangling (parse_name_witness s).
Proof.
  intros s H. apply check_names_sound. exact H.
Qed.

Example cert_resolved : check_names_cert "1 2 3|1 3" = true.
Proof. reflexivity. Qed.
Example cert_dangling : check_names_cert "1 2|1 5" = false.
Proof. reflexivity. Qed.

Print Assumptions check_names_sound.
Print Assumptions check_names_cert_sound.
