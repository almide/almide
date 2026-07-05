(* Almide v1 trust spine — 3rd property (brick 5): TYPE CONCRETIZATION.

   An unresolved type (`Ty::Unknown`) reaching codegen is a silent miscompile
   (#525: a width-0 / pointer-shaped fallback striding the wrong slot). The
   property: every type in the artifact is concrete. The checker accepts iff no
   type tag is `Unknown`; soundness: accept ⟹ all concrete.

   NOTE on the mechanism (hard rail "型レベル > テスト > 手作業"): in almide-mir
   this property is enforced BY CONSTRUCTION — the MIR `Repr` enum has NO
   `Unknown` variant, and `lower::repr_of` rejects `Ty::Unknown` with an explicit
   `Unsupported` (test `unknown_type_is_rejected_at_repr`). So a well-formed MIR
   is type-concrete by its very type. This file gives the matching certificate-
   level checker + soundness for the proof chain. *)

From Stdlib Require Import List.
Import ListNotations.

Inductive TypeTag : Type :=
  | Concrete : TypeTag
  | Unknown : TypeTag.

Definition tag_is_concrete (t : TypeTag) : bool :=
  match t with Concrete => true | Unknown => false end.

(* THE CHECKER: no type tag is Unknown. *)
Definition check_types (ts : list TypeTag) : bool := forallb tag_is_concrete ts.

(* THE PROPERTY: every type is concrete. *)
Definition all_concrete (ts : list TypeTag) : Prop :=
  forall t, In t ts -> t = Concrete.

(* SOUNDNESS. *)
Theorem check_types_sound :
  forall ts, check_types ts = true -> all_concrete ts.
Proof.
  intros ts H t Hin.
  unfold check_types in H. rewrite forallb_forall in H.
  specialize (H t Hin). destruct t.
  - reflexivity.
  - discriminate.
Qed.

Example accepts_all_concrete : check_types [Concrete; Concrete] = true.
Proof. reflexivity. Qed.

Example rejects_any_unknown : check_types [Concrete; Unknown] = false.
Proof. reflexivity. Qed.

Print Assumptions check_types_sound.
