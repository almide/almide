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
From Stdlib Require Import String Ascii.

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

(* ─── witness parsing, INTERNALIZED INTO COQ (end-to-end like check_cert) ───
   Witness format: the DEFINED ids, then `|`, then the USED ids — each a
   whitespace-separated list of decimal nats. The parser is total; its output is
   whatever the checker validates (parse correctness is the cert-faithfulness,
   tested compiler-side). The whole "bytes ⟶ accept/reject" is kernel-checked. *)

Definition is_digit (a : ascii) : bool :=
  andb (Nat.leb 48 (nat_of_ascii a)) (Nat.leb (nat_of_ascii a) 57).
Definition digit (a : ascii) : nat := nat_of_ascii a - 48.
Definition is_bar (a : ascii) : bool := Nat.eqb (nat_of_ascii a) 124. (* '|' *)

Fixpoint pnats (s : string) (cur : option nat) (acc : list nat) : list nat :=
  match s with
  | EmptyString => match cur with Some n => acc ++ [n] | None => acc end
  | String a r =>
      if is_digit a
      then pnats r (Some (match cur with Some n => n * 10 + digit a | None => digit a end)) acc
      else match cur with Some n => pnats r None (acc ++ [n]) | None => pnats r None acc end
  end.

Fixpoint split_bar (s : string) (left : string) : string * string :=
  match s with
  | EmptyString => (left, EmptyString)
  | String a r => if is_bar a then (left, r) else split_bar r (left ++ String a EmptyString)
  end.

Definition parse_name_witness (s : string) : NameWitness :=
  let (ld, lu) := split_bar s EmptyString in
  {| defined := pnats ld None []; used := pnats lu None [] |}.

Definition check_names_cert (s : string) : bool := check_names (parse_name_witness s).

Theorem check_names_cert_sound :
  forall s, check_names_cert s = true -> no_dangling (parse_name_witness s).
Proof.
  intros s H. unfold check_names_cert in H. apply check_names_sound. exact H.
Qed.

Example cert_resolved : check_names_cert "1 2 3|1 3" = true.
Proof. reflexivity. Qed.
Example cert_dangling : check_names_cert "1 2|1 5" = false.
Proof. reflexivity. Qed.

Print Assumptions check_names_sound.
Print Assumptions check_names_cert_sound.
