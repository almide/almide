(* Almide v1 trust kernel — option C: the HEAP-LOOP-CARRIED ownership extension.

   The base OwnershipChecker.v cert is a FLAT per-object event stream — it has no
   notion of a loop, so a loop-carried heap accumulator (`acc = acc + [x]` each
   iteration: drop the old object, alloc a new one, rebind the slot) cannot be
   expressed: the old object's `i` is in iteration K, its `d` in iteration K+1,
   different objects sharing one SLOT. `verify_ownership` (flat, one pass) then
   sees an unbalanced `d`/`i` and the existing checker rejects a SAFE program — an
   INCOMPLETENESS of the proof spine (it false-rejects safe heap-loop-carried code).

   This file closes that incompleteness AT THE ROOT: it adds a `Loop` construct and
   PROVES the checker's loop rule sound w.r.t. the REAL semantics — accepting a loop
   cert guarantees EVERY concrete unrolling (the body run any number of times) is
   free of double-free and leak. The rule: a loop is accepted iff its body PRESERVES
   the refcount (and never faults) from the entry count; then any iteration count
   preserves it, by induction. The accumulator slot's cert is the abstract sequence
   `[Inc; Loop [FDec; FInc]; MoveOut]`: acquire once, each iteration release-the-old
   + acquire-the-new (net 0), move out the final list. Checked by the same fold — no
   compiler trust. Loop bodies are FLAT (`list FlatOp`, no nested loop) — sufficient
   for the v1 parser walls (flow_rec / collect_seq: one drop+alloc per iteration);
   nested loops are a future extension that composes (the rule reasons on the fold). *)

From Stdlib Require Import List.
Import ListNotations.
From Stdlib Require Import ZArith.
Open Scope Z_scope.

(* The CONCRETE per-iteration ownership letters (a flat loop body / an unrolled run). *)
Inductive FlatOp : Type :=
  | FInc : FlatOp        (* +1 acquire *)
  | FDec : FlatOp        (* −1 release  *)
  | FMove : FlatOp.      (* −1 move-out *)

(* The ABSTRACT cert alphabet: the flat letters plus a LOOP over a flat body. *)
Inductive Op : Type :=
  | Inc : Op
  | Dec : Op
  | MoveOut : Op
  | Loop : list FlatOp -> Op.

(* CONCRETE semantics: fold a flat run to an rc, faulting (None) on a −1 at rc = 0
   (double-free / use-after-free). This is "what actually happens" each iteration. *)
Fixpoint exec_flat (ops : list FlatOp) (rc : Z) : option Z :=
  match ops with
  | [] => Some rc
  | FInc :: rest => exec_flat rest (rc + 1)
  | FDec :: rest => if rc <=? 0 then None else exec_flat rest (rc - 1)
  | FMove :: rest => if rc <=? 0 then None else exec_flat rest (rc - 1)
  end.

(* ABSTRACT checker fold over a cert. The Loop arm verifies the body PRESERVES rc
   and does not fault, from the entry count — sufficient for ANY iteration count
   (proved sound vs. the concrete unrolling by check_unroll_sound). No nesting: the
   body is flat, so this is a plain structural recursion calling the defined
   `exec_flat`. *)
Fixpoint exec_list (ops : list Op) (rc : Z) : option Z :=
  match ops with
  | [] => Some rc
  | Inc :: rest => exec_list rest (rc + 1)
  | Dec :: rest => if rc <=? 0 then None else exec_list rest (rc - 1)
  | MoveOut :: rest => if rc <=? 0 then None else exec_list rest (rc - 1)
  | Loop body :: rest =>
      match exec_flat body rc with
      | Some rc' => if Z.eqb rc' rc then exec_list rest rc else None
      | None => None
      end
  end.

Definition run (ops : list Op) : option Z := exec_list ops 0.

Definition check (ops : list Op) : bool :=
  match run ops with
  | Some z => Z.eqb z 0
  | None => false
  end.

(* The semantic property over a CONCRETE (unrolled) run. *)
Definition fno_double_free (ops : list FlatOp) : Prop := exec_flat ops 0 <> None.
Definition fno_leak (ops : list FlatOp) : Prop := exec_flat ops 0 = Some 0.

(* ─── exec_flat distributes over append ─── *)
Lemma exec_flat_app :
  forall a b rc,
    exec_flat (a ++ b) rc =
      match exec_flat a rc with
      | Some rc' => exec_flat b rc'
      | None => None
      end.
Proof.
  induction a as [| o a IH]; intros b rc; simpl.
  - reflexivity.
  - destruct o; simpl.
    + apply IH.
    + destruct (rc <=? 0). reflexivity. apply IH.
    + destruct (rc <=? 0). reflexivity. apply IH.
Qed.

(* A flat body that PRESERVES rc, repeated n times, still preserves rc. *)
Lemma exec_flat_repeat_preserve :
  forall body rc, exec_flat body rc = Some rc ->
    forall n, exec_flat (concat (repeat body n)) rc = Some rc.
Proof.
  intros body rc Hpres. induction n as [| n IH]; simpl.
  - reflexivity.
  - rewrite exec_flat_app, Hpres. exact IH.
Qed.

(* ─── CONCRETE unrolling (the real semantics) ───
   `Unrolls ops fops` : the abstract cert `ops` unrolls to the concrete flat run
   `fops` by replacing each `Loop body` with `n` concatenated copies of `body`
   (n chosen per loop, ≥ 0). The real loop runs the SAME body each iteration (same
   code, different data — identical ownership op sequence), so n identical copies is
   exactly the concrete ownership trace. *)
Inductive Unrolls : list Op -> list FlatOp -> Prop :=
  | U_nil : Unrolls [] []
  | U_inc : forall a b, Unrolls a b -> Unrolls (Inc :: a) (FInc :: b)
  | U_dec : forall a b, Unrolls a b -> Unrolls (Dec :: a) (FDec :: b)
  | U_move : forall a b, Unrolls a b -> Unrolls (MoveOut :: a) (FMove :: b)
  | U_loop : forall body a b n,
      Unrolls a b ->
      Unrolls (Loop body :: a) (concat (repeat body n) ++ b).

(* THE SOUNDNESS CORE (generalized over rc for the induction): if the abstract
   checker accepts at rc (Some r), then EVERY concrete unrolling executes to the
   SAME result Some r — so no unrolling faults (no double-free / UAF) and the final
   rc matches (no leak when r = 0). *)
Lemma exec_unroll :
  forall ops fops, Unrolls ops fops ->
    forall rc r, exec_list ops rc = Some r -> exec_flat fops rc = Some r.
Proof.
  intros ops fops HU. induction HU; intros rc r Hexec; simpl in *.
  - (* nil *) exact Hexec.
  - (* inc *) apply IHHU. exact Hexec.
  - (* dec *) destruct (rc <=? 0); [discriminate | apply IHHU; exact Hexec].
  - (* move *) destruct (rc <=? 0); [discriminate | apply IHHU; exact Hexec].
  - (* loop: ops = Loop body :: a, fops = concat (repeat body n) ++ b *)
    destruct (exec_flat body rc) as [rc' |] eqn:Eb; [| discriminate].
    destruct (Z.eqb rc' rc) eqn:Eq; [| discriminate].
    apply Z.eqb_eq in Eq. subst rc'.
    (* exec_flat body rc = Some rc (body preserves rc) ⇒ n copies preserve rc. *)
    rewrite exec_flat_app, (exec_flat_repeat_preserve body rc Eb n).
    apply IHHU. exact Hexec.
Qed.

(* The headline: an ACCEPTED loop certificate guarantees EVERY concrete unrolling is
   free of double-free / use-after-free AND leak. This is the proof-carrying
   guarantee for heap-loop-carried accumulators — the completeness the flat cert
   lacked, now SOUND (the false-rejection closed at the root, not routed around). *)
Theorem check_unroll_sound :
  forall ops, check ops = true ->
    forall fops, Unrolls ops fops -> fno_double_free fops /\ fno_leak fops.
Proof.
  intros ops H fops HU. unfold check, run in H. unfold fno_double_free, fno_leak.
  destruct (exec_list ops 0) as [z |] eqn:E; [| discriminate].
  apply Z.eqb_eq in H. subst z.
  rewrite (exec_unroll ops fops HU 0 0 E).
  split. discriminate. reflexivity.
Qed.

(* ─── non-vacuity ─── *)
(* The accumulator slot cert `[Inc; Loop [FDec; FInc]; MoveOut]` = acquire once;
   each iteration release-old + acquire-new (net 0); move out the final. ACCEPTS. *)
Example acc_slot_accepts : check [Inc; Loop [FDec; FInc]; MoveOut] = true.
Proof. reflexivity. Qed.

(* A loop whose body LEAKS (net +1: alloc, no release) is REJECTED (body grows rc). *)
Example leaky_loop_rejects : check [Inc; Loop [FInc]; MoveOut] = false.
Proof. reflexivity. Qed.

(* A loop whose body DOUBLE-FREES (net −1: release, no acquire) is REJECTED. *)
Example draining_loop_rejects : check [Inc; Loop [FDec]; MoveOut] = false.
Proof. reflexivity. Qed.

(* A concrete unrolling of the accumulator slot (body run 3 times) — the soundness
   theorem in action: the abstract cert accepts, so this concrete run is balanced. *)
Example acc_unrolls_3 :
  Unrolls [Inc; Loop [FDec; FInc]; MoveOut]
          (FInc :: (concat (repeat [FDec; FInc] 3) ++ [FMove])).
Proof.
  apply U_inc.
  apply (U_loop [FDec; FInc] [MoveOut] [FMove] 3).
  apply U_move. apply U_nil.
Qed.

Example acc_unrolled_3_balanced :
  exec_flat (FInc :: (concat (repeat [FDec; FInc] 3) ++ [FMove])) 0 = Some 0.
Proof. reflexivity. Qed.

(* Axiom audit — soundness must rest on the Coq kernel alone (no admits/axioms). *)
Print Assumptions exec_unroll.
Print Assumptions check_unroll_sound.
