(* Almide v1 trust spine — #40 byte-binding, the WasmCert-Coq APPROACH brought in-tree.

   The maximal #40 is importing the external WasmCert-Coq library; that is infeasible here
   (no opam, and WasmCert-Coq targets older Coq, not Rocq 9.1). What it BUYS, though, is an
   architecture, and that we can have natively: a RELATIONAL small-step operational semantics
   `istep` (the trusted ISA SPEC, one rule per opcode, in the wasm-spec / WasmCert-Coq style)
   plus an EXECUTABLE evaluator `erun` proven to REFINE it (sound + complete). WasmExec.v's
   `run_g` is a bespoke interpreter with no spec to refine; this gives the rc opcode subset a
   real semantics relation and an interpreter that provably implements it.

   This brick covers the STRAIGHT-LINE subset (the rc_inc shape: const/local/load/add/store);
   structured control (`if`/`end`, for rc_dec) is the next brick on the same relation. *)

From Stdlib Require Import List ZArith.
Import ListNotations.

Open Scope Z_scope.

(* Linear memory: address -> i32 value (Z), with a pointwise update. *)
Definition Mem := Z -> Z.
Definition upd (m : Mem) (a v : Z) : Mem := fun x => if Z.eqb x a then v else m x.

Lemma upd_same : forall m a v, upd m a v a = v.
Proof. intros. unfold upd. rewrite Z.eqb_refl. reflexivity. Qed.
(* Keep `upd` folded under cbn so the rc_inc effect proof closes with `upd_same`, not a
   raw `Z.eqb p p` that cbn cannot decide for an abstract address. *)
Opaque upd.

(* The rc opcode subset as an AST — the DECODED form of the bytes WasmEncode.v models. *)
Inductive instr : Type :=
  | IConst (z : Z)
  | ILocalGet (i : Z)
  | ILocalSet (i : Z)
  | IGlobalGet
  | IGlobalSet
  | ILoad
  | IStore
  | IAdd
  | ISub
  | IEqz.

(* A machine configuration: operand stack (head = top), locals, the $freelist global, memory. *)
Record cfg := mkcfg { stk : list Z; loc : Z -> Z; glob : Z; mem : Mem }.

Definition set_loc (f : Z -> Z) (i v : Z) : Z -> Z := fun x => if Z.eqb x i then v else f x.

(* THE ISA SPEC: a small-step reduction, one rule per opcode, matching the wasm spec's stack
   semantics (i32.store pops value then address; both operands consumed; result pushed). *)
Inductive istep : instr -> cfg -> cfg -> Prop :=
  | S_Const : forall z s l g m,
      istep (IConst z) (mkcfg s l g m) (mkcfg (z :: s) l g m)
  | S_LocalGet : forall i s l g m,
      istep (ILocalGet i) (mkcfg s l g m) (mkcfg (l i :: s) l g m)
  | S_LocalSet : forall i v s l g m,
      istep (ILocalSet i) (mkcfg (v :: s) l g m) (mkcfg s (set_loc l i v) g m)
  | S_GlobalGet : forall s l g m,
      istep IGlobalGet (mkcfg s l g m) (mkcfg (g :: s) l g m)
  | S_GlobalSet : forall v s l g m,
      istep IGlobalSet (mkcfg (v :: s) l g m) (mkcfg s l v m)
  | S_Load : forall a s l g m,
      istep ILoad (mkcfg (a :: s) l g m) (mkcfg (m a :: s) l g m)
  | S_Store : forall v a s l g m,
      istep IStore (mkcfg (v :: a :: s) l g m) (mkcfg s l g (upd m a v))
  | S_Add : forall b a s l g m,
      istep IAdd (mkcfg (b :: a :: s) l g m) (mkcfg ((a + b) :: s) l g m)
  | S_Sub : forall b a s l g m,
      istep ISub (mkcfg (b :: a :: s) l g m) (mkcfg ((a - b) :: s) l g m)
  | S_Eqz : forall a s l g m,
      istep IEqz (mkcfg (a :: s) l g m) (mkcfg ((if Z.eqb a 0 then 1 else 0) :: s) l g m).

(* Reflexive sequencing of the spec over an instruction list (the SPEC of running a body). *)
Inductive irun : list instr -> cfg -> cfg -> Prop :=
  | R_nil : forall c, irun [] c c
  | R_cons : forall i is c c' c'',
      istep i c c' -> irun is c' c'' -> irun (i :: is) c c''.

(* THE EXECUTABLE evaluator (one opcode), returning None on a stack underflow. *)
Definition estep (i : instr) (c : cfg) : option cfg :=
  match i, c with
  | IConst z, mkcfg s l g m => Some (mkcfg (z :: s) l g m)
  | ILocalGet i0, mkcfg s l g m => Some (mkcfg (l i0 :: s) l g m)
  | ILocalSet i0, mkcfg (v :: s) l g m => Some (mkcfg s (set_loc l i0 v) g m)
  | IGlobalGet, mkcfg s l g m => Some (mkcfg (g :: s) l g m)
  | IGlobalSet, mkcfg (v :: s) l g m => Some (mkcfg s l v m)
  | ILoad, mkcfg (a :: s) l g m => Some (mkcfg (m a :: s) l g m)
  | IStore, mkcfg (v :: a :: s) l g m => Some (mkcfg s l g (upd m a v))
  | IAdd, mkcfg (b :: a :: s) l g m => Some (mkcfg ((a + b) :: s) l g m)
  | ISub, mkcfg (b :: a :: s) l g m => Some (mkcfg ((a - b) :: s) l g m)
  | IEqz, mkcfg (a :: s) l g m => Some (mkcfg ((if Z.eqb a 0 then 1 else 0) :: s) l g m)
  | _, _ => None
  end.

Fixpoint erun (is : list instr) (c : cfg) : option cfg :=
  match is with
  | [] => Some c
  | i :: rest => match estep i c with Some c' => erun rest c' | None => None end
  end.

(* REFINEMENT, one step: the executable is SOUND (every result it gives is a real reduction)
   and COMPLETE (it produces the reduction's result) w.r.t. the ISA spec. *)
Lemma estep_sound : forall i c c', estep i c = Some c' -> istep i c c'.
Proof.
  intros i c c' H. destruct i, c as [s l g m];
    try (destruct s as [|v s]; [discriminate|]);
    try (destruct s as [|a s]; [discriminate|]);
    cbn in H; injection H as <-; constructor.
Qed.

Lemma estep_complete : forall i c c', istep i c c' -> estep i c = Some c'.
Proof. intros i c c' H. inversion H; subst; reflexivity. Qed.

(* REFINEMENT over a body: erun soundness + completeness w.r.t. irun. *)
Lemma erun_sound : forall is c c', erun is c = Some c' -> irun is c c'.
Proof.
  induction is as [|i is IH]; intros c c' H.
  - cbn in H. injection H as <-. constructor.
  - cbn in H. destruct (estep i c) as [c1|] eqn:E; [|discriminate].
    eapply R_cons. apply estep_sound, E. apply IH, H.
Qed.

Lemma erun_complete : forall is c c', irun is c c' -> erun is c = Some c'.
Proof.
  intros is c c' H. induction H as [c | i is c c' c'' Hstep Hrun IH].
  - reflexivity.
  - cbn. rewrite (estep_complete _ _ _ Hstep). exact IH.
Qed.

(* The ISA spec is DETERMINISTIC, so the executable result is the UNIQUE reduction result. *)
Lemma istep_det : forall i c c1 c2, istep i c c1 -> istep i c c2 -> c1 = c2.
Proof. intros i c c1 c2 H1 H2; inversion H1; subst; inversion H2; subst; reflexivity. Qed.

(* ─── rc_inc, as an ISA program, with its effect proven THROUGH the relation ───
   rc_inc(p): store at p the value (load p)+1. In stack order: push addr (local 0), compute
   value (load (local 0); + 1), then store. *)
Definition rc_inc_prog : list instr :=
  [ILocalGet 0; ILocalGet 0; ILoad; IConst 1; IAdd; IStore].

Definition init (p : Z) (m : Mem) : cfg :=
  mkcfg [] (fun i => if Z.eqb i 0 then p else 0) 0 m.

(* THE ISA EXECUTION FACT: any spec-reduction of the rc_inc program over a cell holding `m p`
   leaves that cell holding `m p + 1` — rt_inc, now carried by the relational ISA semantics
   (not a bespoke interpreter). Proven via the refinement: the relation forces erun's result. *)
Theorem rc_inc_isa_effect : forall p m c',
  irun rc_inc_prog (init p m) c' -> mem c' p = m p + 1.
Proof.
  intros p m c' H. apply erun_complete in H. cbn in H.
  injection H as Hc. subst c'. cbn. apply upd_same.
Qed.

(* Non-vacuous: a cell holding 4 ends holding 5 under the ISA relation. *)
Example rc_inc_isa_4_to_5 : forall c',
  irun rc_inc_prog (init 7 (fun a => if Z.eqb a 7 then 4 else 0)) c' -> mem c' 7 = 5.
Proof. intros c' H. apply rc_inc_isa_effect in H. cbn in H. exact H. Qed.

Print Assumptions rc_inc_isa_effect.
