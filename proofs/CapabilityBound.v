(* Almide v1 trust spine — 4th property (brick 5): CAPABILITY BOUND.

   This is the trust-layer's headline promise: a program uses ONLY the
   capabilities it declares — "this does X and nothing else" / "no network".
   It is the foundation of the receipt's sandbox claim (`C-SAFE`, declared use
   = sandbox execution). The checker accepts iff every USED capability is in the
   DECLARED allowlist; soundness: accept ⟹ no undeclared capability is used.

   Capabilities are modeled as nat ids (network, fs-read, fs-write, … — a fixed
   registry the compiler emits). Same membership shape as name totality; the
   value is what it certifies: an auditor (or a deployer) reads the receipt and
   knows the artifact cannot reach a capability outside the declared set. *)

From Stdlib Require Import List.
Import ListNotations.
From Stdlib Require Import Arith.

Record CapWitness := { allowed : list nat; used_caps : list nat }.

Definition cap_mem (x : nat) (l : list nat) : bool := existsb (Nat.eqb x) l.

(* THE CHECKER: every used capability is in the declared allowlist. *)
Definition check_caps (w : CapWitness) : bool :=
  forallb (fun c => cap_mem c (allowed w)) (used_caps w).

(* THE PROPERTY: the artifact stays within its declared capability bound. *)
Definition within_bound (w : CapWitness) : Prop :=
  forall c, In c (used_caps w) -> In c (allowed w).

(* SOUNDNESS: acceptance guarantees no undeclared capability. *)
Theorem check_caps_sound :
  forall w, check_caps w = true -> within_bound w.
Proof.
  intros w H c Hc.
  unfold check_caps in H. rewrite forallb_forall in H.
  specialize (H c Hc). unfold cap_mem in H. rewrite existsb_exists in H.
  destruct H as [x [Hin Heq]]. apply Nat.eqb_eq in Heq.
  rewrite Heq. exact Hin.
Qed.

(* non-vacuous: a "no network" program (network = cap 0 not in allowlist) is
   accepted when it uses only fs-read (cap 1); rejected if it reaches network. *)
Example accepts_within_sandbox :
  check_caps {| allowed := [1; 2]; used_caps := [1] |} = true.
Proof. reflexivity. Qed.

Example rejects_undeclared_network :
  check_caps {| allowed := [1; 2]; used_caps := [1; 0] |} = false.
Proof. reflexivity. Qed.

Print Assumptions check_caps_sound.
