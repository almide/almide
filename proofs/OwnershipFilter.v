(* Almide v1 trust kernel — the CONDITIONAL-acquire loop extension (filter / filter_map).

   OwnershipLoop.v added a `Loop` over a FLAT body that must PRESERVE rc EXACTLY
   (net 0) every iteration — sufficient for an UNCONDITIONAL append accumulator
   (`acc = acc + [x]`: drop-old + alloc-new, net 0 each iteration). But a
   `list.filter` / `list.filter_map` accumulator updates CONDITIONALLY:

       for x in xs { if pred(x) then { new = out + [x]; drop out; out = new } }

   The output slot's per-iteration body is therefore a BRANCH: the THEN branch
   `[FDec; FInc]` (drop the old accumulator, alloc the new `out + [x]`) when the
   predicate holds, the ELSE branch `[]` (no append) when it does not. The number
   of THEN iterations (k = #predicate-trues) is RUNTIME-VARIABLE. OwnershipLoop's
   net-0 `Loop` rule cannot express this: its body is a single fixed FlatOp list,
   and a conditionally-taken append is not one fixed sequence. An agent's C1-inline
   of a capturing filter byte-matched at runtime but the FLAT/net-0 checker REJECTed
   its certificate — an INCOMPLETENESS exactly one step beyond the append accumulator.

   This file closes it AT THE ROOT, reusing OwnershipLoop's concrete `exec_flat`
   semantics. The KEY observation: BOTH branches of the filter slot's body PRESERVE
   rc (the THEN `[FDec; FInc]` is net 0; the ELSE `[]` is net 0). So ANY sequence of
   per-iteration predicate outcomes preserves the slot's rc — hence the slot is
   leak/double-free-free for ANY input and ANY predicate. We add a `CondLoop thenb
   elseb` construct, a checker rule that accepts iff BOTH branches preserve rc (and
   neither faults) from the entry count, and PROVE the rule sound w.r.t. the real
   semantics: a concrete unrolling is a list of bools (the per-iteration predicate
   outcomes) selecting THEN/ELSE each iteration; an accepted CondLoop guarantees
   EVERY such unrolling is free of double-free / use-after-free AND leak.

   Loop bodies are FLAT (no nested loop) — sufficient for the v1 filter walls (one
   conditional drop+alloc per iteration). The UNCONDITIONAL `Loop` is the special
   case `thenb = elseb` (every iteration the same net-0 body), so this strictly
   generalizes OwnershipLoop's accumulator rule. *)

From Stdlib Require Import List.
Import ListNotations.
From Stdlib Require Import ZArith.
Open Scope Z_scope.

From AlmideTrust Require Import OwnershipLoop.

(* ─── the abstract cert alphabet, extended with a CONDITIONAL loop ─── *)
(* Reuses OwnershipLoop.FlatOp (FInc / FDec / FMove) and its exec_flat. A `COp`
   lifts a flat letter; a `CondLoop thenb elseb` is a loop whose body each iteration
   runs `thenb` (predicate true) or `elseb` (predicate false). *)
Inductive COp : Type :=
  | CInc : COp
  | CDec : COp
  | CMove : COp
  | CondLoop : list FlatOp -> list FlatOp -> COp.

(* ─── the checker fold ───
   The CondLoop arm accepts iff BOTH branches preserve rc from the entry count
   (and neither faults) — then ANY per-iteration choice preserves it, for ANY
   number of iterations (proved sound vs. the concrete unrolling below). *)
Fixpoint cexec (ops : list COp) (rc : Z) : option Z :=
  match ops with
  | [] => Some rc
  | CInc :: rest => cexec rest (rc + 1)
  | CDec :: rest => if rc <=? 0 then None else cexec rest (rc - 1)
  | CMove :: rest => if rc <=? 0 then None else cexec rest (rc - 1)
  | CondLoop thenb elseb :: rest =>
      match exec_flat thenb rc, exec_flat elseb rc with
      | Some rt, Some re =>
          if andb (Z.eqb rt rc) (Z.eqb re rc) then cexec rest rc else None
      | _, _ => None
      end
  end.

Definition crun (ops : list COp) : option Z := cexec ops 0.

Definition ccheck (ops : list COp) : bool :=
  match crun ops with
  | Some z => Z.eqb z 0
  | None => false
  end.

(* ─── the concrete (unrolled) semantics ───
   A CondLoop unrolls to a CONCATENATION of per-iteration branch runs: a list of
   bools `bs` (the runtime predicate outcomes) selects `thenb` or `elseb` each
   iteration. The real loop runs the SAME two branch bodies (same code), choosing
   per the data — so `cond_concat thenb elseb bs` is exactly its concrete ownership
   trace. *)
Fixpoint cond_concat (thenb elseb : list FlatOp) (bs : list bool) : list FlatOp :=
  match bs with
  | [] => []
  | true :: rest => thenb ++ cond_concat thenb elseb rest
  | false :: rest => elseb ++ cond_concat thenb elseb rest
  end.

(* `CUnrolls ops fops` : the abstract cert `ops` unrolls to the concrete flat run
   `fops` by replacing each `CondLoop thenb elseb` with `cond_concat thenb elseb bs`
   for some per-loop outcome list `bs`. *)
Inductive CUnrolls : list COp -> list FlatOp -> Prop :=
  | CU_nil  : CUnrolls [] []
  | CU_inc  : forall a b, CUnrolls a b -> CUnrolls (CInc :: a) (FInc :: b)
  | CU_dec  : forall a b, CUnrolls a b -> CUnrolls (CDec :: a) (FDec :: b)
  | CU_move : forall a b, CUnrolls a b -> CUnrolls (CMove :: a) (FMove :: b)
  | CU_loop : forall thenb elseb a b bs,
      CUnrolls a b ->
      CUnrolls (CondLoop thenb elseb :: a) (cond_concat thenb elseb bs ++ b).

(* ─── the core lemma: two rc-preserving branches ⇒ any choice sequence preserves ───
   If BOTH `thenb` and `elseb` preserve rc (each `exec_flat _ rc = Some rc`), then
   for ANY outcome list `bs`, the concatenated concrete run preserves rc and never
   faults. (Generalizes OwnershipLoop.exec_flat_repeat_preserve from one repeated
   body to a mixed sequence of two net-0 branch bodies.) *)
Lemma cond_concat_preserve :
  forall thenb elseb rc,
    exec_flat thenb rc = Some rc ->
    exec_flat elseb rc = Some rc ->
    forall bs, exec_flat (cond_concat thenb elseb bs) rc = Some rc.
Proof.
  intros thenb elseb rc Ht He.
  induction bs as [| b bs IH]; simpl.
  - reflexivity.
  - destruct b.
    + rewrite exec_flat_app, Ht. exact IH.
    + rewrite exec_flat_app, He. exact IH.
Qed.

(* ─── soundness core (generalized over rc for the induction) ───
   If the abstract checker accepts at rc (Some r), EVERY concrete unrolling executes
   to the SAME result Some r — so no unrolling faults (no double-free / UAF) and the
   final rc matches (no leak when r = 0). *)
Lemma cexec_unroll :
  forall ops fops, CUnrolls ops fops ->
    forall rc r, cexec ops rc = Some r -> exec_flat fops rc = Some r.
Proof.
  intros ops fops HU. induction HU; intros rc r Hexec; simpl in *.
  - (* nil *) exact Hexec.
  - (* CInc *) apply IHHU. exact Hexec.
  - (* CDec *) destruct (rc <=? 0); [discriminate | apply IHHU; exact Hexec].
  - (* CMove *) destruct (rc <=? 0); [discriminate | apply IHHU; exact Hexec].
  - (* CondLoop: ops = CondLoop thenb elseb :: a, fops = cond_concat thenb elseb bs ++ b *)
    destruct (exec_flat thenb rc) as [rt |] eqn:Et; [| discriminate].
    destruct (exec_flat elseb rc) as [re |] eqn:Ee; [| discriminate].
    destruct (andb (Z.eqb rt rc) (Z.eqb re rc)) eqn:Eb; [| discriminate].
    apply andb_prop in Eb. destruct Eb as [Hrt Hre].
    apply Z.eqb_eq in Hrt. apply Z.eqb_eq in Hre. subst rt re.
    (* both branches preserve rc ⇒ any outcome list preserves rc *)
    rewrite exec_flat_app, (cond_concat_preserve thenb elseb rc Et Ee bs).
    apply IHHU. exact Hexec.
Qed.

(* THE headline: an ACCEPTED conditional-loop certificate guarantees EVERY concrete
   unrolling (any predicate-outcome sequence, any iteration count) is free of
   double-free / use-after-free AND leak. This is the proof-carrying guarantee for
   a `list.filter` / `list.filter_map` accumulator — the completeness the net-0
   Loop rule lacked, now SOUND. *)
Theorem ccheck_unroll_sound :
  forall ops, ccheck ops = true ->
    forall fops, CUnrolls ops fops -> fno_double_free fops /\ fno_leak fops.
Proof.
  intros ops H fops HU. unfold ccheck, crun in H.
  unfold fno_double_free, fno_leak.
  destruct (cexec ops 0) as [z |] eqn:E; [| discriminate].
  apply Z.eqb_eq in H. subst z.
  rewrite (cexec_unroll ops fops HU 0 0 E).
  split. discriminate. reflexivity.
Qed.

(* ─── non-vacuity ─── *)
(* The filter accumulator slot cert `[CInc; CondLoop [FDec;FInc] []; CMove]`:
   acquire the empty `out` once; each iteration EITHER drop-old+alloc-new (predicate
   true, net 0) OR nothing (predicate false, net 0); move out the final list.
   ACCEPTS — the runtime-variable number of appends is irrelevant to balance. *)
Example filter_slot_accepts :
  ccheck [CInc; CondLoop [FDec; FInc] []; CMove] = true.
Proof. reflexivity. Qed.

(* A conditional loop whose THEN branch LEAKS (net +1: append without dropping the
   old accumulator) is REJECTED. *)
Example filter_leaky_then_rejects :
  ccheck [CInc; CondLoop [FInc] []; CMove] = false.
Proof. reflexivity. Qed.

(* A conditional loop whose ELSE branch DOUBLE-FREES (net −1) is REJECTED. *)
Example filter_draining_else_rejects :
  ccheck [CInc; CondLoop [FDec; FInc] [FDec]; CMove] = false.
Proof. reflexivity. Qed.

(* A concrete unrolling of the filter slot with the outcome list
   [true; false; true] (append, skip, append — k = 2 of 3 elements kept): the
   soundness theorem in action — the abstract cert accepts, so this concrete run is
   balanced. *)
Example filter_unrolls_tft :
  CUnrolls [CInc; CondLoop [FDec; FInc] []; CMove]
           (FInc :: (cond_concat [FDec; FInc] [] [true; false; true] ++ [FMove])).
Proof.
  apply CU_inc.
  apply (CU_loop [FDec; FInc] [] [CMove] [FMove] [true; false; true]).
  apply CU_move. apply CU_nil.
Qed.

Example filter_unrolled_tft_balanced :
  exec_flat (FInc :: (cond_concat [FDec; FInc] [] [true; false; true] ++ [FMove])) 0 = Some 0.
Proof. reflexivity. Qed.

(* ─── the UNCONDITIONAL Loop is the special case thenb = elseb ───
   When both branches are the SAME net-0 body, CondLoop degenerates to OwnershipLoop's
   accumulator Loop: every iteration runs that one body. So this strictly generalizes
   the append-accumulator rule. (Witness: the append slot `[FDec;FInc]` as a CondLoop
   with identical branches accepts, exactly like OwnershipLoop's `Loop [FDec;FInc]`.) *)
Example cond_generalizes_uncond :
  ccheck [CInc; CondLoop [FDec; FInc] [FDec; FInc]; CMove] = true.
Proof. reflexivity. Qed.

(* Axiom audit — soundness must rest on the Coq kernel alone (no admits/axioms). *)
Print Assumptions cexec_unroll.
Print Assumptions ccheck_unroll_sound.
