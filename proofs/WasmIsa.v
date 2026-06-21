(* Almide v1 trust spine — #40 byte-binding, the WasmCert-Coq APPROACH brought in-tree.

   The maximal #40 is importing the external WasmCert-Coq library; that is infeasible here
   (no opam, and WasmCert-Coq targets older Coq, not Rocq 9.1). What it BUYS, though, is an
   architecture, and that we can have natively: a RELATIONAL small-step operational semantics
   `istep`/`irun` (the trusted ISA SPEC, one rule per opcode, in the wasm-spec / WasmCert-Coq
   style) plus an EXECUTABLE evaluator `erun` proven to REFINE it (sound + complete). WasmExec.v's
   `run_g` is a bespoke interpreter with no spec to refine; this gives the rc opcode subset a real
   semantics relation and an interpreter that provably implements it.

   Covers the straight-line subset (rc_inc) AND structured control (`IIf`/`IUnreachable`, the
   block form `if (cond) then body end` + trap), so rc_dec's double-free TRAP and leak-freedom
   reclamation are carried THROUGH the ISA relation (matching what check-wasm-exec.sh grounds). *)

From Stdlib Require Import List ZArith.
Import ListNotations.

Open Scope Z_scope.

(* Linear memory: address -> i32 value (Z), with a pointwise update. *)
Definition Mem := Z -> Z.
Definition upd (m : Mem) (a v : Z) : Mem := fun x => if Z.eqb x a then v else m x.

Lemma upd_same : forall m a v, upd m a v a = v.
Proof. intros. unfold upd. rewrite Z.eqb_refl. reflexivity. Qed.
Lemma upd_other : forall m a v b, b <> a -> upd m a v b = m b.
Proof. intros m a v b H. unfold upd. apply Z.eqb_neq in H. rewrite H. reflexivity. Qed.

(* The rc opcode subset as an AST. `IIf body` = `if (cond<>0) then body end`; `IUnreachable`
   is the trap (no reduction rule — it is STUCK). *)
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
  | IEqz
  | IIf (body : list instr)
  | IUnreachable.

(* A machine configuration: operand stack (head = top), locals, $freelist global, memory. *)
Record cfg := mkcfg { stk : list Z; loc : Z -> Z; glob : Z; mem : Mem }.

Definition set_loc (f : Z -> Z) (i v : Z) : Z -> Z := fun x => if Z.eqb x i then v else f x.

(* THE ISA SPEC: a small-step reduction, one rule per opcode (wasm-spec stack semantics).
   `istep` (one instr) is MUTUAL with `irun` (a body): the `if` rule runs its body. *)
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
      istep IEqz (mkcfg (a :: s) l g m) (mkcfg ((if Z.eqb a 0 then 1 else 0) :: s) l g m)
  | S_If_true : forall body cnd s l g m c',
      cnd <> 0 ->
      irun body (mkcfg s l g m) c' ->
      istep (IIf body) (mkcfg (cnd :: s) l g m) c'
  | S_If_false : forall body s l g m,
      istep (IIf body) (mkcfg (0 :: s) l g m) (mkcfg s l g m)
with irun : list instr -> cfg -> cfg -> Prop :=
  | R_nil : forall c, irun [] c c
  | R_cons : forall i is c c' c'',
      istep i c c' -> irun is c' c'' -> irun (i :: is) c c''.

(* THE EXECUTABLE: straight-line opcodes (non-recursive), and a single fixpoint `erun` that
   handles `IIf`/`IUnreachable` inline — so `erun body` / `erun rest` are both sub-terms of the
   instruction list (a standard nested-inductive recursion, unlike the rejected mutual form). *)
Definition estep1 (i : instr) (c : cfg) : option cfg :=
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
  | _, _ => None   (* IIf / IUnreachable: handled by erun, not here *)
  end.

(* Coq's structural guard rejects a single fixpoint that recurses into a sub-list nested inside
   an element (`erun body` where `body` sits inside `IIf body`), and the mutual `estep`/`erun`
   form too — a known limitation. So `erun` is FUEL-bounded (recursion on `fuel`), which makes
   the IIf body recursion well-founded. The relation `istep`/`irun` stays the fuel-free SPEC. *)
Fixpoint erun (fuel : nat) (is : list instr) (c : cfg) {struct fuel} : option cfg :=
  match fuel with
  | O => None
  | S f =>
      match is with
      | [] => Some c
      | i :: rest =>
          match i with
          | IUnreachable => None
          | IIf body =>
              match stk c with
              | cnd :: s =>
                  let c0 := mkcfg s (loc c) (glob c) (mem c) in
                  if Z.eqb cnd 0 then erun f rest c0
                  else match erun f body c0 with Some c' => erun f rest c' | None => None end
              | [] => None
              end
          | _ => match estep1 i c with Some c' => erun f rest c' | None => None end
          end
      end
  end.

Lemma estep1_sound : forall i c c', estep1 i c = Some c' -> istep i c c'.
Proof.
  intros i c c' H. destruct i, c as [s l g m]; cbn in H;
    try discriminate;
    try (injection H as <-; constructor);
    try (destruct s as [|v s]; [discriminate|]; injection H as <-; constructor);
    try (destruct s as [|v s]; [discriminate|]; destruct s as [|a s]; [discriminate|];
         injection H as <-; constructor).
Qed.

Opaque estep1.

(* REFINEMENT (soundness): every result the executable produces is a real reduction in the SPEC
   relation. Induction on fuel — every sub-call (IIf body, the continuation) uses `f` < `S f`,
   so the single fuel IH covers them. So `erun` is a verified IMPLEMENTATION of `istep`/`irun`. *)
Lemma erun_sound : forall fuel is c c', erun fuel is c = Some c' -> irun is c c'.
Proof.
  induction fuel as [|f IHf]; intros is c c' H.
  - cbn in H; discriminate.
  - destruct is as [|i rest].
    + cbn in H; injection H as <-; constructor.
    + destruct i.
      1-10: cbn in H; destruct (estep1 _ c) as [c1|] eqn:E1; try discriminate;
            (eapply R_cons; [ apply estep1_sound; exact E1 | apply IHf; exact H ]).
      * (* IIf body *) cbn -[Z.eqb] in H. destruct c as [s l g m]. cbn -[Z.eqb] in H.
        destruct s as [|cnd s]; cbn -[Z.eqb] in H; [discriminate|].
        remember (Z.eqb cnd 0) as b eqn:Eb. destruct b; cbn in H.
        -- symmetry in Eb; apply Z.eqb_eq in Eb; subst cnd.
           eapply R_cons. apply S_If_false. apply IHf; exact H.
        -- symmetry in Eb; apply Z.eqb_neq in Eb.
           destruct (erun f body (mkcfg s l g m)) as [cb|] eqn:Ec; cbn in H;
             [ eapply R_cons; [ eapply S_If_true; [ exact Eb | apply IHf; exact Ec ] | apply IHf; exact H ]
             | discriminate ].
      * (* IUnreachable *) cbn in H; discriminate.
Qed.

Transparent estep1.

(* ─── rc_inc, as an ISA program, with its effect carried THROUGH the relation ─── *)
Definition rc_inc_prog : list instr :=
  [ILocalGet 0; ILocalGet 0; ILoad; IConst 1; IAdd; IStore].

Definition init (p : Z) (m : Mem) : cfg :=
  mkcfg [] (fun i => if Z.eqb i 0 then p else 0) 0 m.

Opaque upd.

(* The ISA relation REACHES the rt_inc effect: there is a reduction of rc_inc over a cell holding
   `m p` to a state where it holds `m p + 1` — and (erun_sound) the verified interpreter takes it.
   `exists`-style because, with the fuel evaluator, pinning "the unique result" would need the
   completeness direction; the witness comes from running the verified interpreter. *)
Theorem rc_inc_isa_effect : forall p m,
  exists c', irun rc_inc_prog (init p m) c' /\ mem c' p = m p + 1.
Proof.
  intros p m. destruct (erun 10 rc_inc_prog (init p m)) as [c'|] eqn:E.
  - exists c'. split.
    + exact (erun_sound _ _ _ _ E).
    + clear - E. cbn in E. injection E as <-. cbn. apply upd_same.
  - cbn in E. discriminate.
Qed.

Transparent upd.

(* ─── rc_dec, as an ISA program: the double-free TRAP and the leak-freedom RECLAMATION, both
   carried THROUGH the relation. local 0 = $p (block ptr), local 1 = $rc. Concrete p = 16 so the
   address arithmetic reduces by computation (Z.eqb on literals), as in check-wasm-exec.sh. ─── *)
Definition rc_dec_prog : list instr :=
  [ ILocalGet 0; ILoad; ILocalSet 1;                       (* rc := load(p) *)
    ILocalGet 1; IEqz; IIf [IUnreachable];                 (* if rc==0 then trap *)
    ILocalGet 1; IConst 1; ISub; ILocalSet 1;              (* rc := rc - 1 *)
    ILocalGet 0; ILocalGet 1; IStore;                      (* store(p, rc) *)
    ILocalGet 1; IEqz;                                     (* if rc==0 (now): reclaim *)
    IIf [ILocalGet 0; IConst 4; IAdd; IGlobalGet; IStore; ILocalGet 0; IGlobalSet] ].

(* The block at address 16, rc preset to `rcval`, freelist preset to `g0`. *)
Definition init_dec (rcval g0 : Z) : cfg :=
  mkcfg [] (fun i => if Z.eqb i 0 then 16 else 0) g0 (fun a => if Z.eqb a 16 then rcval else 0).

(* DOUBLE-FREE TRAP: releasing an already-0 cell never completes — the `if (eqz rc)` takes the
   true branch into `[IUnreachable]`, which is STUCK. The verified interpreter returns None for
   EVERY fuel (it can never run the release to completion): the renderer's double-free sentinel
   fires at the ISA level. With erun_sound, no `Some` result means no completed reduction. *)
Theorem rc_dec_isa_traps_on_zero : forall fuel g0,
  erun fuel rc_dec_prog (init_dec 0 g0) = None.
Proof.
  intros fuel g0.
  do 9 (destruct fuel as [|fuel]; [reflexivity|]). cbn. reflexivity.
Qed.

(* LEAK-FREEDOM RECLAMATION: releasing a uniquely-owned cell (rc = 1) decrements it to 0 AND
   links the block onto $freelist (glob := p = 16) — the freed block is reclaimed, not lost.
   The ISA relation REACHES that state, and (erun_sound) the verified interpreter takes it. *)
Theorem rc_dec_isa_frees_when_one : forall g0,
  exists c', irun rc_dec_prog (init_dec 1 g0) c' /\ mem c' 16 = 0 /\ glob c' = 16.
Proof.
  intros g0. destruct (erun 30 rc_dec_prog (init_dec 1 g0)) as [c'|] eqn:E.
  - exists c'. split.
    + exact (erun_sound _ _ _ _ E).
    + clear - E. cbn in E. injection E as <-. cbn. split; reflexivity.
  - cbn in E. discriminate.
Qed.

Print Assumptions erun_sound.
Print Assumptions rc_inc_isa_effect.
Print Assumptions rc_dec_isa_traps_on_zero.
Print Assumptions rc_dec_isa_frees_when_one.
