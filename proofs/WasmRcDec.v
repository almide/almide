(* Almide v1 trust spine — A2 first slice: the EMITTED `$rc_dec` instructions
   realize the abstract `rt_dec`.

   RuntimeModel.v proved the abstract refcount machine (`rt_dec` = a cell write)
   refines OwnershipChecker.exec. The remaining "model <-> real wasm" gap (A2) is:
   does the wasm the RENDERER ACTUALLY EMITS for a release compute that cell write?
   This file closes the INSTRUCTION-TREE half of that gap. It models the small
   fragment of wasm the renderer emits in `$rc_dec` (a few pure expression
   operators + load/store/trap), encodes the EXACT instruction tree
   `render_wasm.rs` emits as DATA (`rc_dec_prog`), gives it an operational
   semantics, and proves: running that program realizes `RuntimeModel.rt_dec` —
   same fault (trap iff the cell is 0), same decrement.

   So the abstract `rt_dec` that the leak / no-double-free proofs rely on is what
   the emitted INSTRUCTIONS compute, not a free-floating token.

   HONEST scope: this binds `rt_dec` to the instruction TREE (the WAT the renderer
   emits, 1:1 with the ops). It does NOT yet bind to the raw BYTE encoding (the
   assembler / a full WasmCert-Coq ISA) — that is the remaining, heavier half of
   A2. The precondition `0 <= cell` is the no-negative-refcount invariant the rc
   machine maintains; under it, the wasm's `i32.eqz` trap (= 0) and `rt_dec`'s
   `<=? 0` fault coincide. *)

From AlmideTrust Require Import RuntimeModel.
From Stdlib Require Import ZArith.
From Stdlib Require Import List.
Import ListNotations.
From Stdlib Require Import Lia.
Open Scope Z_scope.

(* The wasm EXPRESSION operators the renderer emits in `$rc_dec` (pure, yield a
   Z). `Ptr` = `local.get $p` (the block base); `Tmp` = `local.get $rc` (the temp
   set in step 1). *)
Inductive expr :=
  | Const : Z -> expr
  | Ptr : expr
  | Tmp : expr
  | Add : expr -> expr -> expr
  | Sub : expr -> expr -> expr
  | Load : expr -> expr.

(* A config: the value of the `$rc` temp local, and linear memory (shared with
   RuntimeModel — `Mem`). The block base `$p` is a parameter. *)
Record Cfg := { tmpv : Z; cmem : Mem }.

Fixpoint eval (e : expr) (p : Z) (c : Cfg) : Z :=
  match e with
  | Const z => z
  | Ptr => p
  | Tmp => tmpv c
  | Add a b => eval a p c + eval b p c
  | Sub a b => eval a p c - eval b p c
  | Load a => cmem c (eval a p c)
  end.

(* The statements `$rc_dec` emits: set the temp, trap-if-zero, store. *)
Inductive stmt :=
  | SetTmp : expr -> stmt          (* local.set $rc e *)
  | TrapIfZero : expr -> stmt      (* if (i32.eqz e) (then unreachable) *)
  | Store : expr -> expr -> stmt.  (* i32.store addr val *)

Definition step (s : stmt) (p : Z) (c : Cfg) : option Cfg :=
  match s with
  | SetTmp e => Some {| tmpv := eval e p c; cmem := cmem c |}
  | TrapIfZero e => if Z.eqb (eval e p c) 0 then None else Some c
  | Store a v => Some {| tmpv := tmpv c; cmem := upd (cmem c) (eval a p c) (eval v p c) |}
  end.

Fixpoint run (ss : list stmt) (p : Z) (c : Cfg) : option Cfg :=
  match ss with
  | [] => Some c
  | s :: rest => match step s p c with Some c' => run rest p c' | None => None end
  end.

(* THE EMITTED PROGRAM, as data — exactly `render_wasm.rs`'s `$rc_dec` body:
     local.set $rc (i32.load (i32.add (local.get $p) (i32.const RC_OFFSET)))
     (if (i32.eqz (local.get $rc)) (then (unreachable)))
     i32.store (i32.add (local.get $p) (i32.const RC_OFFSET))
               (i32.sub (local.get $rc) (i32.const 1)) *)
Definition rc_dec_prog : list stmt :=
  [ SetTmp (Load (Add Ptr (Const RC_OFFSET)));
    TrapIfZero Tmp;
    Store (Add Ptr (Const RC_OFFSET)) (Sub Tmp (Const 1)) ].

(* THE A2 BINDING (instruction-tree level): running the EMITTED `$rc_dec`
   realizes `RuntimeModel.rt_dec` on the rc cell — it traps EXACTLY when rt_dec
   faults (cell 0) and otherwise leaves the cell decremented by one, the same
   memory rt_dec produces. The leak/no-double-free proofs' abstract release is
   thus what the real instructions compute. *)
Theorem rc_dec_prog_realizes_rt_dec :
  forall p m, 0 <= m (p + RC_OFFSET) ->
    match run rc_dec_prog p {| tmpv := 0; cmem := m |} with
    | Some c => rt_dec m p = Some (cmem c)
    | None => rt_dec m p = None
    end.
Proof.
  intros p m Hnn.
  unfold rc_dec_prog, run, step, rt_dec, read_rc. cbn [eval tmpv cmem].
  destruct (Z.eqb (m (p + RC_OFFSET)) 0) eqn:E.
  - (* cell = 0: wasm traps; rt_dec faults (0 <=? 0) *)
    apply Z.eqb_eq in E. rewrite E. cbn. reflexivity.
  - (* cell <> 0 and 0 <= cell  =>  cell > 0: both proceed, same decrement *)
    apply Z.eqb_neq in E.
    assert (Hleb : (m (p + RC_OFFSET) <=? 0) = false) by (apply Z.leb_gt; lia).
    rewrite Hleb. cbn [run]. reflexivity.
Qed.

(* The COUNTERPART: the `$rc_inc` body the sharing renderer (A1.3) will emit:
     i32.store (i32.add (local.get $p) (i32.const RC_OFFSET))
               (i32.add (i32.load (i32.add (local.get $p) (i32.const RC_OFFSET)))
                        (i32.const 1))
   It has no trap (an acquire never faults). Binding it now (before A1.3 emits it)
   completes the rc-primitive instruction pair: both `rt_inc` and `rt_dec` are
   realized by the exact instruction trees the renderer emits. *)
Definition rc_inc_prog : list stmt :=
  [ Store (Add Ptr (Const RC_OFFSET))
          (Add (Load (Add Ptr (Const RC_OFFSET))) (Const 1)) ].

Theorem rc_inc_prog_realizes_rt_inc :
  forall p m,
    run rc_inc_prog p {| tmpv := 0; cmem := m |} = Some {| tmpv := 0; cmem := rt_inc m p |}.
Proof.
  intros p m. unfold rc_inc_prog, run, step, rt_inc, read_rc. cbn [eval tmpv cmem].
  reflexivity.
Qed.

(* Non-vacuous, the safety-relevant direction: releasing a cell that is already
   0 (a would-be double-free) TRAPS — exactly `unreachable`, no silent wrap. *)
Example rc_dec_traps_on_zero :
  forall m, m (0 + RC_OFFSET) = 0 ->
    run rc_dec_prog 0 {| tmpv := 0; cmem := m |} = None.
Proof.
  intros m H. unfold rc_dec_prog, run, step. cbn [eval tmpv cmem].
  rewrite H. reflexivity.
Qed.

(* And a cell holding 1 is left FREED (0) — agreeing with rt_dec's decrement. *)
Example rc_dec_frees_a_one :
  forall m, m (0 + RC_OFFSET) = 1 ->
    exists c, run rc_dec_prog 0 {| tmpv := 0; cmem := m |} = Some c
              /\ read_rc (cmem c) 0 = 0.
Proof.
  intros m H. unfold rc_dec_prog, run, step. cbn [eval tmpv cmem].
  rewrite H. eexists. split; [ reflexivity | ].
  cbn [cmem]. rewrite read_upd_same. reflexivity.
Qed.

(* AXIOM AUDIT — soundness rests on the kernel alone. *)
Print Assumptions rc_dec_prog_realizes_rt_dec.
Print Assumptions rc_inc_prog_realizes_rt_inc.
