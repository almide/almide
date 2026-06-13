(* Almide v1 trust spine — the SHARED MEMBERSHIP-SUBSET law.

   Two of the flight-grade properties are, structurally, the SAME decidable
   check over the SAME witness shape (two `|`-separated decimal-nat lists):

     - name totality   (NameTotality.v):    used ⊆ defined   (no dangling ref)
     - capability bound (CapabilityBound.v): used ⊆ allowed   (sandbox promise)

   So the checker, its soundness theorem, and the internalized witness parser
   live here ONCE; the two properties are thin namings of `subset_*`. One proof
   to audit instead of three near-identical copies — a smaller trusted base and
   no parser duplication (the patchwork this would otherwise be). *)

From Stdlib Require Import List.
Import ListNotations.
From Stdlib Require Import Arith.
From Stdlib Require Import String Ascii.

Definition mem (x : nat) (l : list nat) : bool := existsb (Nat.eqb x) l.

(* THE CHECKER: every element of `sub` is in `sup`. *)
Definition subset_check (sup sub : list nat) : bool :=
  forallb (fun x => mem x sup) sub.

(* THE PROPERTY: `sub` is contained in `sup`. *)
Definition subset_prop (sup sub : list nat) : Prop :=
  forall x, In x sub -> In x sup.

(* SOUNDNESS: acceptance guarantees containment. *)
Theorem subset_check_sound :
  forall sup sub, subset_check sup sub = true -> subset_prop sup sub.
Proof.
  intros sup sub H x Hx.
  unfold subset_check in H. rewrite forallb_forall in H.
  specialize (H x Hx). unfold mem in H. rewrite existsb_exists in H.
  destruct H as [y [Hin Heq]]. apply Nat.eqb_eq in Heq.
  rewrite Heq. exact Hin.
Qed.

(* ─── witness parsing, INTERNALIZED INTO COQ (end-to-end like check_cert) ───
   Format: the SUPERSET ids, then `|`, then the SUBSET-to-check ids — each a
   whitespace-separated list of decimal nats. The parser is total; what it
   produces is what the checker validates (parse correctness = the
   cert-faithfulness obligation, tested compiler-side). The whole
   "bytes ⟶ accept/reject" pipeline is kernel-checked. *)

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

(* (superset ids, subset ids) *)
Definition parse_pair (s : string) : list nat * list nat :=
  let (l, r) := split_bar s EmptyString in (pnats l None [], pnats r None []).

Definition subset_cert (s : string) : bool :=
  subset_check (fst (parse_pair s)) (snd (parse_pair s)).

Theorem subset_cert_sound :
  forall s, subset_cert s = true ->
    subset_prop (fst (parse_pair s)) (snd (parse_pair s)).
Proof.
  intros s H. apply subset_check_sound. exact H.
Qed.

(* non-vacuous: accepts a contained witness, rejects one with an outside member. *)
Example cert_contained : subset_cert "1 2 3|1 3" = true.
Proof. reflexivity. Qed.
Example cert_outside : subset_cert "1 2|1 5" = false.
Proof. reflexivity. Qed.

Print Assumptions subset_check_sound.
Print Assumptions subset_cert_sound.
