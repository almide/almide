(* Almide v1 trust spine — flight-grade property: TERMINATION (loop-free fragment).

   The value-semantics MIR has NO unbounded control flow — its op set is a fixed
   list of single-step ops (acquire / release / borrow / call), with no Loop or
   recursion constructor. So a function is a FINITE op list, and its
   interpretation HALTS: it reaches a final state in at most `length ops` steps,
   never diverging.

   We make this non-vacuous (not merely "the Coq Fixpoint is total") with a
   FUEL-counting interpreter — the same discipline almide-interp enforces ("fuel
   is mandatory; an adversarial loop must terminate as FuelExhausted, never
   hang"). The theorem: `length ops` fuel ALWAYS suffices — the interpreter never
   runs out, i.e. every loop-free program halts within a statically-known bound.

   (Termination of LOOPS / RECURSION — a ranking-function / well-founded argument
   — is a later brick, added with those ops. This certifies the fragment that
   exists today actually halts.) *)

From AlmideTrust Require Import OwnershipChecker.
From Stdlib Require Import ZArith List.
Import ListNotations.
Open Scope Z_scope.

(* A fuel-bounded interpreter over the op list. `None` = ran OUT of fuel (a
   would-be divergence); `Some r` = halted with result `r`. Structurally
   recursive on `fuel`. *)
Fixpoint fuel_exec (fuel : nat) (ops : list Op) (rc : Z) : option (option Z) :=
  match ops with
  | nil => Some (Some rc)
  | cons o rest =>
      match fuel with
      | O => None
      | S f =>
          match o with
          | Inc | Alias => fuel_exec f rest (rc + 1)
          | Dec | MoveOut =>
              if rc <=? 0 then Some None else fuel_exec f rest (rc - 1)
          | Reuse => if Z.eqb rc 1 then fuel_exec f rest 0 else Some None
          | Borrow => if rc <=? 0 then Some None else fuel_exec f rest rc
          end
      end
  end.

(* THE TERMINATION THEOREM: `length ops` fuel always suffices — the interpreter
   never runs out, so every loop-free program halts within `length ops` steps. *)
Theorem halts_with_length_fuel :
  forall ops rc, fuel_exec (length ops) ops rc <> None.
Proof.
  induction ops as [| o rest IH]; intros rc.
  - simpl. discriminate.
  - simpl. destruct o.
    + apply IH.
    + apply IH.
    + destruct (rc <=? 0); [ discriminate | apply IH ].
    + destruct (rc <=? 0); [ discriminate | apply IH ].
    + destruct (Z.eqb rc 1); [ apply IH | discriminate ].
    + destruct (rc <=? 0); [ discriminate | apply IH ].
Qed.

(* And the fueled interpreter AGREES with `exec` when it halts: termination does
   not change the answer (the bound is a budget, not a semantics change). *)
Theorem fuel_exec_agrees :
  forall ops rc, fuel_exec (length ops) ops rc = Some (exec ops rc).
Proof.
  induction ops as [| o rest IH]; intros rc.
  - reflexivity.
  - simpl. destruct o.
    + apply IH.
    + apply IH.
    + destruct (rc <=? 0); [ reflexivity | apply IH ].
    + destruct (rc <=? 0); [ reflexivity | apply IH ].
    + destruct (Z.eqb rc 1); [ apply IH | reflexivity ].
    + destruct (rc <=? 0); [ reflexivity | apply IH ].
Qed.

Example halts_example : fuel_exec (length [Inc; Alias; Dec; Dec]) [Inc; Alias; Dec; Dec] 0 = Some (Some 0).
Proof. reflexivity. Qed.

Print Assumptions halts_with_length_fuel.
Print Assumptions fuel_exec_agrees.
