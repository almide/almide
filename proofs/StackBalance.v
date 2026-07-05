(* Almide v1 trust spine — flight-grade property: WASM OPERAND-STACK BALANCE.

   A function's emitted wasm must keep the operand stack BALANCED: it must never
   pop below the function-entry depth (an underflowing / malformed module) and
   must end at the entry depth (no values left on the stack). Wasmtime validates
   this, but the trust layer PROVES it from a witness — so the property holds by
   a re-checkable certificate, not by trusting the validator.

   It is a BALANCE law — a left-fold of per-instruction stack DELTAS — the SAME
   shape as RC balance (OwnershipChecker.exec), now over the operand stack and
   with arbitrary integer deltas (a call pops its args, pushes its result). The
   witness is the per-instruction net-delta stream the (untrusted) renderer
   emits; `stack_check` accepts iff the fold never underflows and ends at 0.

   Scope (honest): the value-semantics renderer is balanced BY CONSTRUCTION (each
   op's WAT consumes its result into a `local.set`, net 0), so today this is the
   GUARD that catches a FUTURE renderer that breaks balance — exactly as
   `validate_safety` guards against a stray `rc_dec`. *)

From Stdlib Require Import ZArith List.
Import ListNotations.
Open Scope Z_scope.

(* Fold the per-instruction stack deltas over the operand-stack depth.
   `None` = UNDERFLOW: a pop drove the depth below 0 (a malformed module). *)
Fixpoint stack_exec (deltas : list Z) (depth : Z) : option Z :=
  match deltas with
  | [] => Some depth
  | d :: rest =>
      let depth' := depth + d in
      if depth' <? 0 then None else stack_exec rest depth'
  end.

Definition stack_run (deltas : list Z) : option Z := stack_exec deltas 0.

(* THE CHECKER: accept iff the operand stack never underflows and ends balanced. *)
Definition stack_check (deltas : list Z) : bool :=
  match stack_run deltas with
  | Some z => Z.eqb z 0
  | None => false
  end.

(* THE PROPERTY, against the operational fold. *)
Definition no_underflow (deltas : list Z) : Prop := stack_run deltas <> None.
Definition ends_balanced (deltas : list Z) : Prop := stack_run deltas = Some 0.

(* SOUNDNESS: acceptance guarantees a balanced operand stack — no underflow and
   no leftover values. Same argument as the RC-balance `check_sound`. *)
Theorem stack_check_sound :
  forall deltas, stack_check deltas = true -> no_underflow deltas /\ ends_balanced deltas.
Proof.
  intros deltas H. unfold stack_check in H.
  unfold no_underflow, ends_balanced.
  destruct (stack_run deltas) as [z |] eqn:E.
  - apply Z.eqb_eq in H. subst z. split; [ discriminate | reflexivity ].
  - discriminate.
Qed.

(* non-vacuous: accepts a balanced push/pop sequence, rejects underflow and
   a leftover value. (`+1` = push a value; `-1` = pop / consume one.) *)
Example accepts_balanced : stack_check [1; 1; -1; -1] = true.
Proof. reflexivity. Qed.

Example rejects_underflow : stack_check [1; -1; -1] = false.   (* pop below empty *)
Proof. reflexivity. Qed.

Example rejects_leftover : stack_check [1; 1; -1] = false.     (* a value left on the stack *)
Proof. reflexivity. Qed.

(* a call pattern: push 2 args (+1 +1), the call pops 2 pushes 1 (-1), the
   local.set consumes the result (-1) — net balanced. *)
Example accepts_call_pattern : stack_check [1; 1; -1; -1] = true.
Proof. reflexivity. Qed.

Print Assumptions stack_check_sound.
