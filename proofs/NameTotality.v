(* Almide v1 trust spine — 2nd property (critical-path brick 5): NAME TOTALITY.

   A reference to an undefined name (a dangling MIR ValueId, an unresolved
   symbol) is undefined behavior / a link error. This is the second flight-grade
   property, after RC balance. Same shape as OwnershipChecker: a DECIDABLE
   checker over a witness, and a soundness theorem `accept ⟹ P`.

   The witness (which almide-mir emits, like the ownership certificate) is the
   set of DEFINED names and the set of USED names; names are modeled as nat ids
   (a MIR ValueId is a nat). The checker accepts iff every used name is defined;
   soundness: acceptance guarantees no dangling reference. *)

From Stdlib Require Import List.
Import ListNotations.
From Stdlib Require Import Arith.

(* A name-totality witness: the defined names and the used names. *)
Record NameWitness := { defined : list nat; used : list nat }.

Definition mem (x : nat) (l : list nat) : bool := existsb (Nat.eqb x) l.

(* THE CHECKER: every used name is defined. *)
Definition check_names (w : NameWitness) : bool :=
  forallb (fun u => mem u (defined w)) (used w).

(* THE PROPERTY: no dangling reference. *)
Definition no_dangling (w : NameWitness) : Prop :=
  forall u, In u (used w) -> In u (defined w).

(* SOUNDNESS: acceptance guarantees totality. *)
Theorem check_names_sound :
  forall w, check_names w = true -> no_dangling w.
Proof.
  intros w H u Hu.
  unfold check_names in H. rewrite forallb_forall in H.
  specialize (H u Hu). unfold mem in H. rewrite existsb_exists in H.
  destruct H as [x [Hin Heq]]. apply Nat.eqb_eq in Heq.
  rewrite Heq. exact Hin.
Qed.

(* non-vacuous: accepts resolved witnesses, rejects a dangling reference. *)
Example accepts_resolved :
  check_names {| defined := [1; 2; 3]; used := [1; 3] |} = true.
Proof. reflexivity. Qed.

Example rejects_dangling :
  check_names {| defined := [1; 2]; used := [1; 5] |} = false.
Proof. reflexivity. Qed.

Print Assumptions check_names_sound.
