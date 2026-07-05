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

(* ONE-STEP unfolding of `erun (S n)` on a cons, KEEPING the inner `erun n` calls FOLDED (plain
   `cbn` unfolds `erun (S n)` via the fuel match, dissolving the very subterm the mono/complete
   rewrites target). Definitional, so `reflexivity`. *)
Lemma erun_S_cons : forall n i rest c,
  erun (S n) (i :: rest) c =
    match i with
    | IUnreachable => None
    | IIf body =>
        match stk c with
        | cnd :: s =>
            if Z.eqb cnd 0 then erun n rest (mkcfg s (loc c) (glob c) (mem c))
            else match erun n body (mkcfg s (loc c) (glob c) (mem c)) with
                 | Some c' => erun n rest c' | None => None end
        | [] => None
        end
    | _ => match estep1 i c with Some c' => erun n rest c' | None => None end
    end.
Proof. intros n i rest [s l g m]; destruct i; reflexivity. Qed.

(* FUEL MONOTONICITY: a successful run stays successful with more fuel — so completeness can run
   the IIf body and the continuation at one common fuel bound. *)
Lemma erun_mono_S : forall n is c c', erun n is c = Some c' -> erun (S n) is c = Some c'.
Proof.
  induction n as [|f IHf]; intros is c c' H.
  - cbn in H; discriminate.
  - destruct is as [|i rest]; [ cbn in H |- *; exact H |].
    rewrite erun_S_cons in H |- *. destruct i;
      try (destruct (estep1 _ c) as [c1|] eqn:E1; [ apply IHf; exact H | discriminate ]).
    + (* IIf body *) destruct c as [s l g m]; cbn [stk loc glob mem] in H |- *.
      destruct s as [|cnd s]; [discriminate|].
      remember (Z.eqb cnd 0) as b. destruct b.
      * apply IHf; exact H.
      * destruct (erun f body (mkcfg s l g m)) as [cb|] eqn:Eb; [|discriminate].
        rewrite (IHf _ _ _ Eb). apply IHf; exact H.
    + (* IUnreachable *) discriminate.
Qed.

Lemma erun_mono_add : forall k n is c c', erun n is c = Some c' -> erun (n + k) is c = Some c'.
Proof.
  induction k as [|k IHk]; intros n is c c' H.
  - rewrite Nat.add_0_r; exact H.
  - rewrite Nat.add_succ_r; apply erun_mono_S; apply IHk; exact H.
Qed.

Transparent estep1.

(* DETERMINISM of the ISA spec: a configuration steps to at most one successor, so a reduction
   of a program has a UNIQUE final state. With erun_sound this UPGRADES the rc effects from
   "exists a reduction reaching E" to "EVERY reduction reaches E": any irun result equals the
   one the verified interpreter computes. By the combined mutual induction on the derivation. *)
Scheme istep_ind2 := Induction for istep Sort Prop
  with irun_ind2 := Induction for irun Sort Prop.
Combined Scheme step_run_ind from istep_ind2, irun_ind2.

Lemma isa_det :
  (forall i c c1, istep i c c1 -> forall c2, istep i c c2 -> c1 = c2) /\
  (forall is c c1, irun is c c1 -> forall c2, irun is c c2 -> c1 = c2).
Proof.
  apply step_run_ind;
    try (intros; match goal with [ H : istep _ _ _ |- _ ] => inversion H; subst; reflexivity end).
  - (* S_If_true *) intros body cnd s l g m c' Hne _ IH c2 H2.
    inversion H2; subst; [ apply IH; assumption | exfalso; apply Hne; reflexivity ].
  - (* S_If_false *) intros body s l g m c2 H2.
    inversion H2; subst; [ exfalso; match goal with [ H : _ <> _ |- _ ] => apply H; reflexivity end
                         | reflexivity ].
  - (* R_nil *) intros c c2 H2; inversion H2; subst; reflexivity.
  - (* R_cons *) intros i is c c1 c'' Hs1 IHstep Hr1 IHrun c2 H2.
    inversion H2; subst.
    match goal with
    | [ Hs2 : istep i c ?cm, Hr2 : irun is ?cm c2 |- _ ] =>
        assert (c1 = cm) as Hcm by (apply IHstep; exact Hs2);
        rewrite <- Hcm in Hr2; exact (IHrun _ Hr2)
    end.
Qed.

Definition irun_det := proj2 isa_det.

(* COMPLETENESS: every reduction is realized by the executable at SOME fuel. With the executable
   trap theorem this gives the RELATIONAL double-free trap (~irun). Combined induction on the
   derivation; the per-step property is "head composition at a head fuel `nh`", and the IIf case
   bumps the body/continuation to a common fuel via erun_mono_add. *)
Lemma erun_complete : forall is c c', irun is c c' -> exists n, erun n is c = Some c'.
Proof.
  enough (Hpair :
    (forall i c c', istep i c c' ->
       exists nh, forall rest cc n, erun n rest c' = Some cc -> erun (nh + n) (i :: rest) c = Some cc)
    /\ (forall is c c', irun is c c' -> exists n, erun n is c = Some c'))
    by exact (proj2 Hpair).
  apply step_run_ind.
  1-10: intros; exists 1%nat; intros rest cc n Hr; cbn; exact Hr.
  - (* S_If_true *) intros body cnd s l g m c' Hne _ IHbody.
    destruct IHbody as [nb Hnb]. exists (S nb). intros rest cc n Hr.
    apply (erun_mono_add n) in Hnb. apply (erun_mono_add nb) in Hr. rewrite Nat.add_comm in Hr.
    apply Z.eqb_neq in Hne.
    cbn [Nat.add]. rewrite erun_S_cons. cbn -[erun Z.eqb estep1]. rewrite Hne, Hnb. cbn -[erun]. exact Hr.
  - (* S_If_false *) intros body s l g m. exists 1%nat. intros rest cc n Hr.
    cbn [Nat.add]. rewrite erun_S_cons. cbn -[erun]. exact Hr.
  - (* R_nil *) intros c. exists 1%nat. reflexivity.
  - (* R_cons *) intros i is c c1 c'' _ IHstep _ IHrun.
    destruct IHstep as [nh Hh]. destruct IHrun as [n2 H2].
    exists (nh + n2)%nat. apply Hh. exact H2.
Qed.

(* ─── rc_inc, as an ISA program, with its effect carried THROUGH the relation ─── *)
Definition rc_inc_prog : list instr :=
  [ILocalGet 0; ILocalGet 0; ILoad; IConst 1; IAdd; IStore].

Definition init (p : Z) (m : Mem) : cfg :=
  mkcfg [] (fun i => if Z.eqb i 0 then p else 0) 0 m.

Opaque upd.

(* The rt_inc effect, FORALL: every reduction of rc_inc over a cell holding `m p` leaves it
   holding `m p + 1`. The verified interpreter computes one such reduction (erun_sound); isa_det
   makes it THE reduction, so any irun result equals it. *)
Theorem rc_inc_isa_effect : forall p m c',
  irun rc_inc_prog (init p m) c' -> mem c' p = m p + 1.
Proof.
  intros p m c' H.
  destruct (erun 10 rc_inc_prog (init p m)) as [c2|] eqn:E; [|cbn in E; discriminate].
  pose proof (irun_det _ _ _ H _ (erun_sound _ _ _ _ E)) as ->.
  clear - E. cbn in E. injection E as <-. cbn. apply upd_same.
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

(* THE SAME TRAP, RELATIONALLY (forall): NO reduction of rc_dec over an already-0 cell completes —
   `~ irun`. By completeness, a completing reduction would be realized by the interpreter at some
   fuel; but it returns None at EVERY fuel. So a double-free is stuck in the SPEC itself. *)
Theorem rc_dec_isa_traps_rel : forall g0 c',
  ~ irun rc_dec_prog (init_dec 0 g0) c'.
Proof.
  intros g0 c' H. apply erun_complete in H. destruct H as [n Hn].
  rewrite rc_dec_isa_traps_on_zero in Hn. discriminate.
Qed.

(* LEAK-FREEDOM RECLAMATION, FORALL: every reduction of rc_dec over a uniquely-owned cell (rc = 1)
   decrements it to 0 AND links the block onto $freelist (glob := p = 16) — the freed block is
   reclaimed, not lost. The verified interpreter computes the reduction; isa_det makes it unique. *)
Theorem rc_dec_isa_frees_when_one : forall g0 c',
  irun rc_dec_prog (init_dec 1 g0) c' -> mem c' 16 = 0 /\ glob c' = 16.
Proof.
  intros g0 c' H.
  destruct (erun 30 rc_dec_prog (init_dec 1 g0)) as [c2|] eqn:E; [|cbn in E; discriminate].
  pose proof (irun_det _ _ _ H _ (erun_sound _ _ _ _ E)) as ->.
  clear - E. cbn in E. injection E as <-. cbn. split; reflexivity.
Qed.

Print Assumptions erun_sound.
Print Assumptions rc_inc_isa_effect.
Print Assumptions rc_dec_isa_traps_on_zero.
Print Assumptions rc_dec_isa_frees_when_one.
Print Assumptions rc_dec_isa_traps_rel.
Print Assumptions erun_complete.
