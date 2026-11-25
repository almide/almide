(* The guarded access, read from the SHIPPED BYTES and
   judged in the kernel (Section 7 of the paper).

   An earlier file of the development proves the arithmetic of a guarded list access. This file
   adds the instructions: a WINDOW is a straight-line run of the shipped
   function body that ends in a load or a store, with nothing in it that
   writes memory, calls, or branches, except aborting arms
   (`if ... unreachable end`). It is decoded from the module's own bytes
   (`dec`), run symbolically from an empty stack (`srun`), and accepted
   (`check`) when the access's address is

       base + 12 + idx * S + o      (o + width <= S)

   and a compare the window aborted on leaves `idx <u len / S`, `len` being
   the word the window read at `base + 4`.

   `check_sound`: in EVERY run of an accepted window that reaches the access
   (any locals, any globals, any memory, any stack beneath, with 32- and
   64-bit wrap-around as the machine has it), the access touches only bytes
   in `[B + 12, B + 12 + len)`, for `B` the value of the base expression the
   check names and `len` the word at `B + 4`, provided that range lies in the
   32-bit address space. No witness, no anchor and no other rule is used:
   the statement is about the bytes alone.

   What is modelled, and so trusted: the meaning of nine instructions
   (`usem`, `bsem`: i32.add, i32.mul, i32.shl, i32.div_u, i32.shr_u,
   i32.ge_u, i64.ge_u, i32.wrap_i64, i64.extend_i32_u), that every other
   numeric instruction is some function of its operands, that a load reads
   memory and changes nothing, and that an arm with no branch that ends in
   `unreachable` does not fall through. A trap only removes runs, so an
   instruction that may trap is modelled as total. Not here: that the bytes
   handed to `dec` are a run of one function's instructions at one nesting
   depth (the cutter is part of the byte-level check), and that the base is a live
   block whose length word is its payload length (the heap invariant). *)

From Stdlib Require Import ZArith List Lia Bool.
Import ListNotations.
Open Scope Z_scope.

Definition M32 : Z := 4294967296.
Definition M64 : Z := 18446744073709551616.
Definition PAYLOAD : Z := 12.

(* ══ THE WINDOW LANGUAGE ═══════════════════════════════════════════════ *)

(* An instruction keeps its wasm opcode, so the decoder has no table to get
   wrong beyond arity. *)
Inductive ins : Type :=
  | ILocalGet (x : Z)
  | ILocalSet (x : Z)
  | ILocalTee (x : Z)
  | IGlobalGet (g : Z)
  | IConst (z : Z)
  | IUn (op : Z)
  | IBin (op : Z)
  | ILoad (op off : Z)
  | IDrop
  | IAbortIf.

(* A value as an expression in what the window found on entry. *)
Inductive tm : Type :=
  | TL (x : Z)
  | TG (g : Z)
  | TK (z : Z)
  | TM (op off : Z) (a : tm)
  | TU (op : Z) (a : tm)
  | TB (op : Z) (a b : tm).

Fixpoint tm_eqb (a b : tm) : bool :=
  match a, b with
  | TL x, TL y => x =? y
  | TG x, TG y => x =? y
  | TK x, TK y => x =? y
  | TM o1 f1 a1, TM o2 f2 a2 => (o1 =? o2) && (f1 =? f2) && tm_eqb a1 a2
  | TU o1 a1, TU o2 a2 => (o1 =? o2) && tm_eqb a1 a2
  | TB o1 a1 b1, TB o2 a2 b2 => (o1 =? o2) && tm_eqb a1 a2 && tm_eqb b1 b2
  | _, _ => false
  end.

Lemma tm_eqb_eq : forall a b, tm_eqb a b = true -> a = b.
Proof.
  induction a; destruct b; simpl; intro H; try discriminate;
    repeat match goal with
           | H : _ && _ = true |- _ => apply andb_true_iff in H; destruct H
           | H : (_ =? _) = true |- _ => apply Z.eqb_eq in H
           end;
    subst; f_equal; auto.
Qed.

Section Semantics.

(* One entry state: the locals, the globals, memory as each load opcode
   reads it at an effective address, and the instructions not given a
   meaning below. All arbitrary. *)
Variable L0 : Z -> Z.
Variable G : Z -> Z.
Variable memf : Z -> Z -> Z.
Variable uo : Z -> Z -> Z.
Variable bo : Z -> Z -> Z -> Z.

(* An i32.load (opcode 40) yields a 32-bit word. *)
Hypothesis Hmem : forall a, 0 <= memf 40 a < M32.

Definition usem (op v : Z) : Z :=
  if op =? 167 then v mod M32                               (* i32.wrap_i64 *)
  else if op =? 173 then v mod M32                          (* i64.extend_i32_u *)
  else uo op v.

Definition bsem (op a b : Z) : Z :=
  if op =? 106 then (a + b) mod M32                         (* i32.add *)
  else if op =? 108 then (a * b) mod M32                    (* i32.mul *)
  else if op =? 116 then (a * 2 ^ (b mod 32)) mod M32       (* i32.shl *)
  else if op =? 110 then (a mod M32) / (b mod M32)          (* i32.div_u *)
  else if op =? 118 then (a mod M32) / 2 ^ (b mod 32)       (* i32.shr_u *)
  else if op =? 79 then (if (a mod M32) <? (b mod M32) then 0 else 1)   (* i32.ge_u *)
  else if op =? 90 then (if (a mod M64) <? (b mod M64) then 0 else 1)   (* i64.ge_u *)
  else bo op a b.

Fixpoint den (t : tm) : Z :=
  match t with
  | TL x => L0 x
  | TG g => G g
  | TK z => z
  | TM op off a => memf op (den a mod M32 + off)
  | TU op a => usem op (den a)
  | TB op a b => bsem op (den a) (den b)
  end.

Lemma bsem_add : forall a b, bsem 106 a b = (a + b) mod M32.
Proof. reflexivity. Qed.
Lemma bsem_mul : forall a b, bsem 108 a b = (a * b) mod M32.
Proof. reflexivity. Qed.
Lemma bsem_shl : forall a b, bsem 116 a b = (a * 2 ^ (b mod 32)) mod M32.
Proof. reflexivity. Qed.
Lemma bsem_divu : forall a b, bsem 110 a b = (a mod M32) / (b mod M32).
Proof. reflexivity. Qed.
Lemma bsem_shru : forall a b, bsem 118 a b = (a mod M32) / 2 ^ (b mod 32).
Proof. reflexivity. Qed.
Lemma bsem_geu32 : forall a b, bsem 79 a b = (if (a mod M32) <? (b mod M32) then 0 else 1).
Proof. reflexivity. Qed.
Lemma bsem_geu64 : forall a b, bsem 90 a b = (if (a mod M64) <? (b mod M64) then 0 else 1).
Proof. reflexivity. Qed.
Lemma usem_wrap : forall v, usem 167 v = v mod M32.
Proof. reflexivity. Qed.
Lemma usem_extu : forall v, usem 173 v = v mod M32.
Proof. reflexivity. Qed.

(* ── the machine ── *)

Definition conf : Type := ((Z -> Z) * list Z)%type.

Definition upd_loc (f : Z -> Z) (x v : Z) : Z -> Z :=
  fun y => if x =? y then v else f y.

Definition cstep (i : ins) (c : conf) : option conf :=
  let (loc, stk) := c in
  match i with
  | ILocalGet x => Some (loc, loc x :: stk)
  | ILocalSet x => match stk with v :: r => Some (upd_loc loc x v, r) | [] => None end
  | ILocalTee x => match stk with v :: r => Some (upd_loc loc x v, v :: r) | [] => None end
  | IGlobalGet g => Some (loc, G g :: stk)
  | IConst z => Some (loc, z :: stk)
  | IUn op => match stk with a :: r => Some (loc, usem op a :: r) | [] => None end
  | IBin op => match stk with b :: a :: r => Some (loc, bsem op a b :: r) | _ => None end
  | ILoad op off => match stk with a :: r => Some (loc, memf op (a mod M32 + off) :: r) | [] => None end
  | IDrop => match stk with _ :: r => Some (loc, r) | [] => None end
  | IAbortIf => match stk with
                | c :: r => if c mod M32 =? 0 then Some (loc, r) else None
                | [] => None
                end
  end.

Fixpoint crun (ws : list ins) (c : conf) : option conf :=
  match ws with
  | [] => Some c
  | i :: r => match cstep i c with Some c' => crun r c' | None => None end
  end.

(* ── the same run over expressions ── *)

Fixpoint look (e : list (Z * tm)) (x : Z) : tm :=
  match e with
  | [] => TL x
  | (y, t) :: r => if y =? x then t else look r x
  end.

Record sym : Type := mkS { senv : list (Z * tm); sstk : list tm; sfacts : list tm }.

Definition sstep (i : ins) (s : sym) : option sym :=
  match i with
  | ILocalGet x => Some (mkS (senv s) (look (senv s) x :: sstk s) (sfacts s))
  | ILocalSet x => match sstk s with
                   | t :: r => Some (mkS ((x, t) :: senv s) r (sfacts s))
                   | [] => None
                   end
  | ILocalTee x => match sstk s with
                   | t :: r => Some (mkS ((x, t) :: senv s) (t :: r) (sfacts s))
                   | [] => None
                   end
  | IGlobalGet g => Some (mkS (senv s) (TG g :: sstk s) (sfacts s))
  | IConst z => Some (mkS (senv s) (TK z :: sstk s) (sfacts s))
  | IUn op => match sstk s with
              | a :: r => Some (mkS (senv s) (TU op a :: r) (sfacts s))
              | [] => None
              end
  | IBin op => match sstk s with
               | b :: a :: r => Some (mkS (senv s) (TB op a b :: r) (sfacts s))
               | _ => None
               end
  | ILoad op off => match sstk s with
                    | a :: r => Some (mkS (senv s) (TM op off a :: r) (sfacts s))
                    | [] => None
                    end
  | IDrop => match sstk s with
             | _ :: r => Some (mkS (senv s) r (sfacts s))
             | [] => None
             end
  | IAbortIf => match sstk s with
                | c :: r => Some (mkS (senv s) r (c :: sfacts s))
                | [] => None
                end
  end.

Fixpoint srun (ws : list ins) (s : sym) : option sym :=
  match ws with
  | [] => Some s
  | i :: r => match sstep i s with Some s' => srun r s' | None => None end
  end.

Definition zero32 (f : tm) : Prop := den f mod M32 = 0.

Lemma step_sound : forall i loc stk e st fs rest loc' stk' s',
  stk = map den st ++ rest ->
  (forall x, loc x = den (look e x)) ->
  Forall zero32 fs ->
  sstep i (mkS e st fs) = Some s' ->
  cstep i (loc, stk) = Some (loc', stk') ->
  stk' = map den (sstk s') ++ rest
  /\ (forall x, loc' x = den (look (senv s') x))
  /\ Forall zero32 (sfacts s').
Proof.
  intros i loc stk e st fs rest loc' stk' s' Hs Hl Hf Hss Hcs. subst stk.
  destruct i.
  - (* local.get *)
    cbn [sstep cstep senv sstk sfacts map app] in Hss, Hcs.
    injection Hss as Es. injection Hcs as El Ek. subst s' loc' stk'.
    cbn [sstk senv sfacts map app].
    split; [rewrite Hl; reflexivity | split; [exact Hl | exact Hf]].
  - (* local.set *)
    destruct st as [|t r]; cbn [sstep cstep senv sstk sfacts map app] in Hss, Hcs;
      [discriminate Hss|].
    injection Hss as Es. injection Hcs as El Ek. subst s' loc' stk'.
    cbn [sstk senv sfacts].
    split; [reflexivity | split; [| exact Hf]].
    intro y. unfold upd_loc. cbn [look]. destruct (x =? y); [reflexivity | apply Hl].
  - (* local.tee *)
    destruct st as [|t r]; cbn [sstep cstep senv sstk sfacts map app] in Hss, Hcs;
      [discriminate Hss|].
    injection Hss as Es. injection Hcs as El Ek. subst s' loc' stk'.
    cbn [sstk senv sfacts map app].
    split; [reflexivity | split; [| exact Hf]].
    intro y. unfold upd_loc. cbn [look]. destruct (x =? y); [reflexivity | apply Hl].
  - (* global.get *)
    cbn [sstep cstep senv sstk sfacts map app] in Hss, Hcs.
    injection Hss as Es. injection Hcs as El Ek. subst s' loc' stk'.
    cbn [sstk senv sfacts map app den].
    split; [reflexivity | split; [exact Hl | exact Hf]].
  - (* const *)
    cbn [sstep cstep senv sstk sfacts map app] in Hss, Hcs.
    injection Hss as Es. injection Hcs as El Ek. subst s' loc' stk'.
    cbn [sstk senv sfacts map app den].
    split; [reflexivity | split; [exact Hl | exact Hf]].
  - (* unary *)
    destruct st as [|a r]; cbn [sstep cstep senv sstk sfacts map app] in Hss, Hcs;
      [discriminate Hss|].
    injection Hss as Es. injection Hcs as El Ek. subst s' loc' stk'.
    cbn [sstk senv sfacts map app den].
    split; [reflexivity | split; [exact Hl | exact Hf]].
  - (* binary *)
    destruct st as [|b [|a r]]; cbn [sstep cstep senv sstk sfacts map app] in Hss, Hcs;
      [discriminate Hss | discriminate Hss |].
    injection Hss as Es. injection Hcs as El Ek. subst s' loc' stk'.
    cbn [sstk senv sfacts map app den].
    split; [reflexivity | split; [exact Hl | exact Hf]].
  - (* load *)
    destruct st as [|a r]; cbn [sstep cstep senv sstk sfacts map app] in Hss, Hcs;
      [discriminate Hss|].
    injection Hss as Es. injection Hcs as El Ek. subst s' loc' stk'.
    cbn [sstk senv sfacts map app den].
    split; [reflexivity | split; [exact Hl | exact Hf]].
  - (* drop *)
    destruct st as [|a r]; cbn [sstep cstep senv sstk sfacts map app] in Hss, Hcs;
      [discriminate Hss|].
    injection Hss as Es. injection Hcs as El Ek. subst s' loc' stk'.
    cbn [sstk senv sfacts].
    split; [reflexivity | split; [exact Hl | exact Hf]].
  - (* the aborting arm: past it the condition was zero *)
    destruct st as [|c r]; cbn [sstep cstep senv sstk sfacts map app] in Hss, Hcs;
      [discriminate Hss|].
    destruct (den c mod M32 =? 0) eqn:E; try rewrite E in Hcs; [|discriminate Hcs].
    injection Hss as Es. injection Hcs as El Ek. subst s' loc' stk'.
    cbn [sstk senv sfacts].
    split; [reflexivity | split; [exact Hl |]].
    constructor; [unfold zero32; apply Z.eqb_eq; exact E | exact Hf].
Qed.

Lemma run_sound : forall ws loc stk e st fs rest loc' stk' s',
  stk = map den st ++ rest ->
  (forall x, loc x = den (look e x)) ->
  Forall zero32 fs ->
  srun ws (mkS e st fs) = Some s' ->
  crun ws (loc, stk) = Some (loc', stk') ->
  stk' = map den (sstk s') ++ rest
  /\ (forall x, loc' x = den (look (senv s') x))
  /\ Forall zero32 (sfacts s').
Proof.
  induction ws as [|i r IH]; intros loc stk e st fs rest loc' stk' s' Hs Hl Hf Hss Hcs.
  - cbn [srun crun] in Hss, Hcs.
    injection Hss as Es. injection Hcs as El Ek. subst s' loc' stk'.
    cbn [sstk senv sfacts]. split; [exact Hs | split; [exact Hl | exact Hf]].
  - cbn [srun crun] in Hss, Hcs.
    destruct (sstep i (mkS e st fs)) as [[e1 st1 fs1]|] eqn:E1; try rewrite E1 in Hss;
      [|discriminate Hss].
    destruct (cstep i (loc, stk)) as [[loc1 stk1]|] eqn:E2; try rewrite E2 in Hcs;
      [|discriminate Hcs].
    destruct (step_sound i loc stk e st fs rest loc1 stk1 (mkS e1 st1 fs1) Hs Hl Hf E1 E2)
      as [H1 [H2 H3]].
    cbn [sstk senv sfacts] in H1, H2, H3.
    exact (IH loc1 stk1 e1 st1 fs1 rest loc' stk' s' H1 H2 H3 Hss Hcs).
Qed.

(* ══ THE ACCESS ════════════════════════════════════════════════════════ *)

(* An i32 sum, flattened: its addends that are not constants, and the sum
   of those that are. *)
Fixpoint flat (t : tm) : list tm * Z :=
  match t with
  | TK z => ([], z)
  | TB op a b =>
      if op =? 106 then (fst (flat a) ++ fst (flat b), snd (flat a) + snd (flat b))
      else ([t], 0)
  | _ => ([t], 0)
  end.

Fixpoint sumd (xs : list tm) : Z :=
  match xs with
  | [] => 0
  | x :: r => den x + sumd r
  end.

Lemma sumd_app : forall xs ys, sumd (xs ++ ys) = sumd xs + sumd ys.
Proof.
  induction xs as [|x r IH]; intro ys; cbn [sumd app]; [reflexivity | rewrite IH; lia].
Qed.

Lemma M32_nz : M32 <> 0.
Proof. unfold M32. lia. Qed.

Lemma flat_sound : forall t,
  den t mod M32 = (sumd (fst (flat t)) + snd (flat t)) mod M32.
Proof.
  induction t as [x|g|z|op off a IHa|op a IHa|op a IHa b IHb]; cbn [flat].
  - cbn [fst snd sumd]. f_equal; lia.
  - cbn [fst snd sumd]. f_equal; lia.
  - cbn [fst snd sumd den]. f_equal; lia.
  - cbn [fst snd sumd]. f_equal; lia.
  - cbn [fst snd sumd]. f_equal; lia.
  - destruct (op =? 106) eqn:E; cbn [fst snd sumd].
    + apply Z.eqb_eq in E. subst op. cbn [den]. rewrite bsem_add.
      rewrite sumd_app.
      rewrite (Z.mod_mod (den a + den b) M32 M32_nz).
      rewrite (Z.add_mod (den a) (den b) M32 M32_nz).
      rewrite IHa, IHb.
      rewrite <- (Z.add_mod (sumd (fst (flat a)) + snd (flat a))
                            (sumd (fst (flat b)) + snd (flat b)) M32 M32_nz).
      f_equal; lia.
    + f_equal; lia.
Qed.

(* The length word of the block at `b`: an i32.load at `b + 4`. *)
Definition is_len (t b : tm) : bool :=
  match t with
  | TM op off a => (op =? 40) && (off =? 4) && tm_eqb a b
  | _ => false
  end.

Definition len_of (b : tm) : Z := memf 40 (den b mod M32 + 4).

Lemma is_len_sound : forall t b, is_len t b = true -> den t = len_of b.
Proof.
  intros t b H. destruct t; try discriminate H. cbn [is_len] in H.
  apply andb_true_iff in H. destruct H as [H H3].
  apply andb_true_iff in H. destruct H as [H1 H2].
  apply Z.eqb_eq in H1. apply Z.eqb_eq in H2. apply tm_eqb_eq in H3. subst.
  reflexivity.
Qed.

(* The block's count of slots of `s` bytes: `len / s`, as the emitter
   writes it (a division, or a shift for a power of two). *)
Definition is_count (t b : tm) (s : Z) : bool :=
  ((s =? 1) && is_len t b)
  || match t with
     | TB op l (TK k) =>
         ((op =? 110) && (k =? s) && is_len l b)
         || ((op =? 118) && (if (0 <=? k) && (k <? 32) then 2 ^ k =? s else false) && is_len l b)
     | _ => false
     end.
(* The power sits under the test of `k`, not beside it: an evaluator that
   computes both sides of `&&` would otherwise build `2 ^ k` for whatever
   constant the bytes hold. *)

Lemma len_range : forall b, 0 <= len_of b < M32.
Proof. intro b. unfold len_of. apply Hmem. Qed.

Lemma is_count_sound : forall t b s,
  is_count t b s = true -> 0 < s < M32 -> den t = len_of b / s.
Proof.
  intros t b s H Hs. unfold is_count in H.
  apply orb_true_iff in H. destruct H as [H | H].
  - apply andb_true_iff in H. destruct H as [H1 H2].
    apply Z.eqb_eq in H1. subst s. rewrite (is_len_sound t b H2).
    rewrite Z.div_1_r. reflexivity.
  - destruct t as [| | | | |op l r]; try discriminate H.
    destruct r as [| |k| | |]; try discriminate H.
    cbv beta iota in H.
    apply orb_true_iff in H. destruct H as [H | H].
    + apply andb_true_iff in H. destruct H as [H H3].
      apply andb_true_iff in H. destruct H as [H1 H2].
      apply Z.eqb_eq in H1. apply Z.eqb_eq in H2. subst op k.
      cbn [den]. rewrite bsem_divu. rewrite (is_len_sound l b H3).
      pose proof (len_range b) as Hr.
      rewrite (Z.mod_small (len_of b) M32 Hr).
      rewrite (Z.mod_small s M32) by lia.
      reflexivity.
    + apply andb_true_iff in H. destruct H as [H H5].
      apply andb_true_iff in H. destruct H as [H1 H4].
      destruct ((0 <=? k) && (k <? 32)) eqn:E; [| discriminate H4].
      apply andb_true_iff in E. destruct E as [H2 H3].
      apply Z.eqb_eq in H1. apply Z.leb_le in H2. apply Z.ltb_lt in H3.
      apply Z.eqb_eq in H4. subst op s.
      cbn [den]. rewrite bsem_shru. rewrite (is_len_sound l b H5).
      pose proof (len_range b) as Hr.
      rewrite (Z.mod_small (len_of b) M32 Hr).
      rewrite (Z.mod_small k 32) by lia.
      reflexivity.
Qed.

Lemma count_range : forall b s, 0 < s -> 0 <= len_of b / s < M32.
Proof.
  intros b s Hs. pose proof (len_range b) as Hr. split.
  - apply Z.div_pos; lia.
  - apply Z.div_lt_upper_bound; [lia |].
    assert (M32 <= s * M32) by (unfold M32; lia). lia.
Qed.

Lemma M32_divides_M64 : (M32 | M64).
Proof. exists M32. reflexivity. Qed.

Lemma low32_below : forall v c, 0 <= c < M32 -> v mod M64 < c -> v mod M32 < c.
Proof.
  intros v c Hc Hv.
  assert (H0 : 0 <= v mod M64 < M64) by (apply Z.mod_pos_bound; unfold M64; lia).
  assert (Hnz : M64 <> 0) by (unfold M64; lia).
  pose proof (Z.div_mod v M64 Hnz) as E.
  assert (Eq : v mod M64 = v mod M32).
  { apply (Z.mod_unique v M32 (M32 * (v / M64)) (v mod M64)).
    - left. lia.
    - unfold M32, M64 in *. lia. }
  rewrite <- Eq. exact Hv.
Qed.

(* A term the window found zero that leaves the index `ix` below the count
   of `b` at stride `s`: the emitter's one unsigned compare, 32 or 64 bit. *)
Definition fact_ok (f ix b : tm) (s : Z) : bool :=
  match f with
  | TB op y cnt =>
      ((op =? 79) && tm_eqb y ix && is_count cnt b s)
      || ((op =? 90)
          && match cnt with
             | TU o2 cnt' =>
                 (o2 =? 173) && is_count cnt' b s
                 && (match ix with
                     | TU o3 x2 => (o3 =? 167) && tm_eqb x2 y
                     | _ => false
                     end
                     || match y with
                        | TU o4 y' => (o4 =? 173) && tm_eqb y' ix
                        | _ => false
                        end)
             | _ => false
             end)
  | _ => false
  end.

Lemma fact_ok_sound : forall f ix b s,
  fact_ok f ix b s = true -> zero32 f -> 0 < s < M32 ->
  den ix mod M32 < len_of b / s.
Proof.
  intros f ix b s H Hz Hs. unfold zero32 in Hz.
  assert (Hs0 : 0 < s) by lia.
  pose proof (count_range b s Hs0) as Hc.
  destruct f as [| | | | |op y cnt]; try discriminate H. cbn [fact_ok] in H.
  apply orb_true_iff in H. destruct H as [H | H].
  - (* i32.ge_u *)
    apply andb_true_iff in H. destruct H as [H H3].
    apply andb_true_iff in H. destruct H as [H1 H2].
    apply Z.eqb_eq in H1. apply tm_eqb_eq in H2. subst op y.
    cbn [den] in Hz. rewrite bsem_geu32 in Hz.
    rewrite (is_count_sound cnt b s H3 Hs) in Hz.
    rewrite (Z.mod_small (len_of b / s) M32 Hc) in Hz.
    destruct (den ix mod M32 <? len_of b / s) eqn:E.
    + apply Z.ltb_lt in E. exact E.
    + try rewrite E in Hz. vm_compute in Hz. discriminate Hz.
  - (* i64.ge_u against the zero-extended count *)
    apply andb_true_iff in H. destruct H as [H1 H].
    apply Z.eqb_eq in H1. subst op.
    destruct cnt as [| | | |o2 cnt'|]; try discriminate H.
    cbv beta iota in H.
    apply andb_true_iff in H. destruct H as [H H4].
    apply andb_true_iff in H. destruct H as [H2 H3].
    apply Z.eqb_eq in H2. subst o2.
    cbn [den] in Hz. rewrite bsem_geu64 in Hz. rewrite usem_extu in Hz.
    rewrite (is_count_sound cnt' b s H3 Hs) in Hz.
    rewrite (Z.mod_small (len_of b / s) M32 Hc) in Hz.
    assert (Hc64 : 0 <= len_of b / s < M64) by (unfold M32, M64 in *; lia).
    rewrite (Z.mod_small (len_of b / s) M64 Hc64) in Hz.
    assert (Hlt : den y mod M64 < len_of b / s).
    { destruct (den y mod M64 <? len_of b / s) eqn:E.
      - apply Z.ltb_lt in E. exact E.
      - try rewrite E in Hz. vm_compute in Hz. discriminate Hz. }
    apply orb_true_iff in H4. destruct H4 as [H4 | H4].
    + (* the index is the wrapped i64 *)
      destruct ix as [| | | |o3 x2|]; try discriminate H4.
      cbv beta iota in H4.
      apply andb_true_iff in H4. destruct H4 as [H5 H6].
      apply Z.eqb_eq in H5. apply tm_eqb_eq in H6. subst o3 x2.
      cbn [den]. rewrite usem_wrap. rewrite (Z.mod_mod (den y) M32 M32_nz).
      apply low32_below; [exact Hc | exact Hlt].
    + (* the compared i64 is the zero-extended index *)
      destruct y as [| | | |o4 y'|]; try discriminate H4.
      cbv beta iota in H4.
      apply andb_true_iff in H4. destruct H4 as [H5 H6].
      apply Z.eqb_eq in H5. apply tm_eqb_eq in H6. subst o4 y'.
      cbn [den] in Hlt. rewrite usem_extu in Hlt.
      assert (Hr : 0 <= den ix mod M32 < M32) by (apply Z.mod_pos_bound; unfold M32; lia).
      assert (Hr64 : 0 <= den ix mod M32 < M64) by (unfold M32, M64 in *; lia).
      rewrite (Z.mod_small (den ix mod M32) M64 Hr64) in Hlt.
      exact Hlt.
Qed.

(* An addend as an index times a stride. *)
Definition scaled (t : tm) : tm * Z :=
  match t with
  | TB op a (TK k) =>
      if op =? 108 then (a, k)
      else if (op =? 116) && (0 <=? k) && (k <? 32) then (a, 2 ^ k)
      else (t, 1)
  | _ => (t, 1)
  end.

Lemma scaled_sound : forall t,
  den t mod M32 = (den (fst (scaled t)) mod M32 * snd (scaled t)) mod M32.
Proof.
  intro t.
  assert (Hid : forall u, den u mod M32 = (den u mod M32 * 1) mod M32).
  { intro u. rewrite Z.mul_1_r. rewrite (Z.mod_mod (den u) M32 M32_nz). reflexivity. }
  destruct t as [| | | | |op a r]; try (cbn [scaled fst snd]; apply Hid).
  destruct r as [| |k| | |]; try (cbn [scaled fst snd]; apply Hid).
  cbn [scaled]. destruct (op =? 108) eqn:E1.
  - apply Z.eqb_eq in E1. subst op. cbn [fst snd den]. rewrite bsem_mul.
    rewrite (Z.mod_mod (den a * k) M32 M32_nz).
    rewrite (Z.mul_mod_idemp_l (den a) k M32 M32_nz). reflexivity.
  - destruct ((op =? 116) && (0 <=? k) && (k <? 32)) eqn:E2.
    + apply andb_true_iff in E2. destruct E2 as [E2 E4].
      apply andb_true_iff in E2. destruct E2 as [E2 E3].
      apply Z.eqb_eq in E2. apply Z.leb_le in E3. apply Z.ltb_lt in E4. subst op.
      cbn [fst snd den]. rewrite bsem_shl.
      rewrite (Z.mod_small k 32) by lia.
      rewrite (Z.mod_mod (den a * 2 ^ k) M32 M32_nz).
      rewrite (Z.mul_mod_idemp_l (den a) (2 ^ k) M32 M32_nz). reflexivity.
    + cbn [fst snd]. apply Hid.
Qed.

(* One reading of the address: `b` the base, `sx` the index part, `c` the
   constants; the memarg adds `off`, and the access is `w` bytes wide. *)
Definition try_pair (facts : list tm) (b sx : tm) (c off w : Z) : bool :=
  (0 <? snd (scaled sx)) && (snd (scaled sx) <? M32)
  && (0 <=? c) && (0 <=? off) && (0 <? w)
  && (PAYLOAD <=? c + off) && (c + off - PAYLOAD + w <=? snd (scaled sx))
  && existsb (fun f => fact_ok f (fst (scaled sx)) b (snd (scaled sx))) facts.

Lemma pair_sound : forall facts b sx c off w av,
  try_pair facts b sx c off w = true ->
  Forall zero32 facts ->
  av mod M32 = (den b + den sx + c) mod M32 ->
  den b mod M32 + PAYLOAD + len_of b <= M32 ->
  den b mod M32 + PAYLOAD <= av mod M32 + off
  /\ av mod M32 + off + w <= den b mod M32 + PAYLOAD + len_of b.
Proof.
  intros facts b sx c off w av H Hf Hav Hfit. unfold try_pair in H.
  apply andb_true_iff in H. destruct H as [H H8].
  apply andb_true_iff in H. destruct H as [H H7].
  apply andb_true_iff in H. destruct H as [H H6].
  apply andb_true_iff in H. destruct H as [H H5].
  apply andb_true_iff in H. destruct H as [H H4].
  apply andb_true_iff in H. destruct H as [H H3].
  apply andb_true_iff in H. destruct H as [H1 H2].
  apply Z.ltb_lt in H1. apply Z.ltb_lt in H2. apply Z.leb_le in H3.
  apply Z.leb_le in H4. apply Z.ltb_lt in H5. apply Z.leb_le in H6.
  apply Z.leb_le in H7.
  apply existsb_exists in H8. destruct H8 as [f [Hin Hok]].
  assert (Hz : zero32 f) by (rewrite Forall_forall in Hf; apply Hf; exact Hin).
  pose proof (fact_ok_sound f (fst (scaled sx)) b (snd (scaled sx)) Hok Hz (conj H1 H2)) as Hix.
  pose proof (scaled_sound sx) as Hsx.
  pose proof (len_range b) as Hlen.
  assert (Hi : 0 <= den (fst (scaled sx)) mod M32 < M32)
    by (apply Z.mod_pos_bound; unfold M32; lia).
  assert (HB : 0 <= den b mod M32 < M32) by (apply Z.mod_pos_bound; unfold M32; lia).
  pose proof (Z.div_mod (den b) M32 M32_nz) as Eb.
  pose proof (Z.div_mod (den sx) M32 M32_nz) as Es.
  pose proof (Z.mul_div_le (len_of b) (snd (scaled sx)) H1) as Hd.
  unfold PAYLOAD in *.
  remember (snd (scaled sx)) as s eqn:Rs.
  remember (den (fst (scaled sx)) mod M32) as i eqn:Ri.
  remember (len_of b) as len eqn:Rl.
  remember (den b mod M32) as B eqn:RB.
  remember (den b / M32) as qb eqn:Rqb.
  remember (den sx / M32) as qs eqn:Rqs.
  clear Rs Ri Rl RB Rqb Rqs Hok Hz Hin Hf.
  assert (Hm : (i + 1) * s <= (len / s) * s) by (apply Z.mul_le_mono_nonneg_r; lia).
  rewrite (Z.mul_comm (len / s) s) in Hm.
  assert (His0 : 0 <= i * s) by (apply Z.mul_nonneg_nonneg; lia).
  assert (His : i * s + s <= len) by lia.
  assert (Hsmall : 0 <= i * s < M32) by lia.
  rewrite (Z.mod_small (i * s) M32 Hsmall) in Hsx.
  rewrite Hsx in Es.
  assert (Eav : av mod M32 = B + i * s + c).
  { rewrite Hav. symmetry.
    apply (Z.mod_unique (den b + den sx + c) M32 (qb + qs) (B + i * s + c)).
    - left. lia.
    - unfold M32 in *. lia. }
  rewrite Eav. lia.
Qed.

(* The access's address is `base + index part + constants`, either way round. *)
Definition recog (facts : list tm) (a : tm) (off w : Z) : option tm :=
  match flat a with
  | ([x; y], c) =>
      if try_pair facts x y c off w then Some x
      else if try_pair facts y x c off w then Some y
      else None
  | _ => None
  end.

Lemma recog_sound : forall facts a off w b,
  recog facts a off w = Some b ->
  Forall zero32 facts ->
  den b mod M32 + PAYLOAD + len_of b <= M32 ->
  den b mod M32 + PAYLOAD <= den a mod M32 + off
  /\ den a mod M32 + off + w <= den b mod M32 + PAYLOAD + len_of b.
Proof.
  intros facts a off w b H Hf Hfit. unfold recog in H.
  pose proof (flat_sound a) as Ha.
  revert H Ha. destruct (flat a) as [xs c]. cbn [fst snd]. intros H Ha.
  destruct xs as [|x [|y [|z r]]]; try discriminate H.
  cbv beta iota in H. cbn [sumd] in Ha.
  destruct (try_pair facts x y c off w) eqn:E1; try rewrite E1 in H.
  - injection H as Eb. subst b.
    apply (pair_sound facts x y c off w (den a) E1 Hf); [| exact Hfit].
    rewrite Ha. f_equal; lia.
  - destruct (try_pair facts y x c off w) eqn:E2; try rewrite E2 in H; [|discriminate H].
    injection H as Eb. subst b.
    apply (pair_sound facts y x c off w (den a) E2 Hf); [| exact Hfit].
    rewrite Ha. f_equal; lia.
Qed.

(* ══ THE WINDOW ════════════════════════════════════════════════════════ *)

Inductive akind : Type := ALoad | AStore.

(* The access's address operand: the top of the stack for a load, under the
   stored value for a store. *)
Definition addr_of {A : Type} (k : akind) (stk : list A) : option A :=
  match k, stk with
  | ALoad, a :: _ => Some a
  | AStore, _ :: a :: _ => Some a
  | _, _ => None
  end.

Definition check (ws : list ins) (k : akind) (off w : Z) : option tm :=
  match srun ws (mkS [] [] []) with
  | Some s =>
      match addr_of k (sstk s) with
      | Some a => recog (sfacts s) a off w
      | None => None
      end
  | None => None
  end.

(* Every run of an accepted window that reaches its access touches only the
   payload of the block at the base the check names, as long as that block
   lies in the address space. *)
Theorem check_sound : forall ws k off w b rest loc stk,
  check ws k off w = Some b ->
  crun ws (L0, rest) = Some (loc, stk) ->
  exists av,
    addr_of k stk = Some av
    /\ (den b mod M32 + PAYLOAD + len_of b <= M32 ->
        den b mod M32 + PAYLOAD <= av mod M32 + off
        /\ av mod M32 + off + w <= den b mod M32 + PAYLOAD + len_of b).
Proof.
  intros ws k off w b rest loc stk. unfold check.
  destruct (srun ws (mkS [] [] [])) as [[e st fs]|] eqn:Es; [| intro H0; discriminate H0].
  cbn [sstk sfacts].
  destruct (addr_of k st) as [a|] eqn:Ea; [| intro H0; discriminate H0].
  intros H Hrun.
  destruct (run_sound ws L0 rest [] [] [] rest loc stk (mkS e st fs)
              eq_refl (fun x => eq_refl) (Forall_nil zero32) Es Hrun) as [Hs [_ Hf]].
  cbn [sstk sfacts] in Hs, Hf.
  exists (den a). split.
  - subst stk.
    destruct k; destruct st as [|t1 [|t2 r]]; cbn [addr_of] in Ea; try discriminate Ea;
      injection Ea as Ea'; subst; reflexivity.
  - intro Hfit. exact (recog_sound fs a off w b H Hf Hfit).
Qed.

End Semantics.

(* ══ THE BYTES ═════════════════════════════════════════════════════════ *)

(* LEB128, unsigned and signed. *)
Fixpoint uleb (fuel : nat) (bs : list Z) : option (Z * list Z) :=
  match fuel, bs with
  | S f, b :: r =>
      if b <? 128 then Some (b, r)
      else match uleb f r with
           | Some (v, r') => Some (b - 128 + 128 * v, r')
           | None => None
           end
  | _, _ => None
  end.

Fixpoint sleb (fuel : nat) (bs : list Z) : option (Z * list Z) :=
  match fuel, bs with
  | S f, b :: r =>
      if b <? 128 then Some ((if b <? 64 then b else b - 128), r)
      else match sleb f r with
           | Some (v, r') => Some (b - 128 + 128 * v, r')
           | None => None
           end
  | _, _ => None
  end.

(* Numeric instructions by arity (the opcode space 0x45..0xc4). *)
Definition is_un (op : Z) : bool :=
  (op =? 69) || (op =? 80)
  || ((103 <=? op) && (op <=? 105)) || ((121 <=? op) && (op <=? 123))
  || ((139 <=? op) && (op <=? 145)) || ((153 <=? op) && (op <=? 159))
  || ((167 <=? op) && (op <=? 196)).

Definition is_bin (op : Z) : bool :=
  ((70 <=? op) && (op <=? 79)) || ((81 <=? op) && (op <=? 102))
  || ((106 <=? op) && (op <=? 120)) || ((124 <=? op) && (op <=? 138))
  || ((146 <=? op) && (op <=? 152)) || ((160 <=? op) && (op <=? 166)).

(* The bytes a load (0x28..0x35) or a store (0x36..0x3e) touches. *)
Definition width (op : Z) : option Z :=
  if (op =? 40) || (op =? 42) || (op =? 52) || (op =? 53)
     || (op =? 54) || (op =? 56) || (op =? 62) then Some 4
  else if (op =? 41) || (op =? 43) || (op =? 55) || (op =? 57) then Some 8
  else if (op =? 44) || (op =? 45) || (op =? 48) || (op =? 49)
          || (op =? 58) || (op =? 60) then Some 1
  else if (op =? 46) || (op =? 47) || (op =? 50) || (op =? 51)
          || (op =? 59) || (op =? 61) then Some 2
  else None.

(* An aborting arm, after its `if` (0x04 0x40): any run of constants, local
   and global reads and writes, calls, drops, numeric instructions, loads
   and stores, then `unreachable; end`. No block, no branch and no return is
   read, so nothing in it can leave the arm but the abort. *)
Fixpoint darm (fuel : nat) (bs : list Z) : option (list Z) :=
  match fuel with
  | O => None
  | S f =>
      match bs with
      | [] => None
      | op :: r =>
          if op =? 0 then
            match r with
            | e :: r' => if e =? 11 then Some r' else None
            | [] => None
            end
          else if (op =? 65) || (op =? 66) then
            match sleb 10 r with Some (_, r') => darm f r' | None => None end
          else if ((32 <=? op) && (op <=? 36)) || (op =? 16) then
            match uleb 5 r with Some (_, r') => darm f r' | None => None end
          else if (op =? 26) || is_un op || is_bin op then darm f r
          else if (40 <=? op) && (op <=? 62) then
            match uleb 5 r with
            | Some (_, r1) => match uleb 5 r1 with Some (_, r2) => darm f r2 | None => None end
            | None => None
            end
          else None
      end
  end.

(* A window's bytes: its instructions, then the access (kind, memarg
   offset, width). A store, a call, an unconditional branch or a block
   before the end is not a window; a `br_if` is read as an exit, like an
   aborting arm: the run goes on past it only when its condition was zero. *)
Fixpoint dec (fuel : nat) (bs : list Z) (acc : list ins)
  : option (list ins * akind * Z * Z) :=
  match fuel with
  | O => None
  | S f =>
      match bs with
      | [] => None
      | op :: r =>
          if op =? 32 then
            match uleb 5 r with Some (x, r') => dec f r' (ILocalGet x :: acc) | None => None end
          else if op =? 33 then
            match uleb 5 r with Some (x, r') => dec f r' (ILocalSet x :: acc) | None => None end
          else if op =? 34 then
            match uleb 5 r with Some (x, r') => dec f r' (ILocalTee x :: acc) | None => None end
          else if op =? 35 then
            match uleb 5 r with Some (x, r') => dec f r' (IGlobalGet x :: acc) | None => None end
          else if op =? 65 then
            match sleb 5 r with Some (z, r') => dec f r' (IConst (z mod M32) :: acc) | None => None end
          else if op =? 66 then
            match sleb 10 r with Some (z, r') => dec f r' (IConst (z mod M64) :: acc) | None => None end
          else if op =? 26 then dec f r (IDrop :: acc)
          else if op =? 13 then
            (* br_if: past it the condition was zero, as past an aborting arm *)
            match uleb 5 r with Some (_, r') => dec f r' (IAbortIf :: acc) | None => None end
          else if op =? 4 then
            match r with
            | bt :: r1 =>
                if bt =? 64 then
                  match darm f r1 with Some r2 => dec f r2 (IAbortIf :: acc) | None => None end
                else None
            | [] => None
            end
          else if is_un op then dec f r (IUn op :: acc)
          else if is_bin op then dec f r (IBin op :: acc)
          else if (40 <=? op) && (op <=? 62) then
            match uleb 5 r with
            | Some (al, r1) =>
                if al <? 64 then
                  match uleb 5 r1 with
                  | Some (off, r2) =>
                      match r2 with
                      | [] =>
                          match width op with
                          | Some w => Some (rev acc, (if op <=? 53 then ALoad else AStore), off, w)
                          | None => None
                          end
                      | _ :: _ => if op <=? 53 then dec f r2 (ILoad op off :: acc) else None
                      end
                  | None => None
                  end
                else None
            | None => None
            end
          else None
      end
  end.

(* The check on a window's bytes: the base expression it proves the access
   inside, or nothing. *)
Definition window (bs : list Z) : option tm :=
  match dec (S (length bs)) bs [] with
  | Some (ws, k, off, w) => check ws k off w
  | None => None
  end.

Definition window_ok (bs : list Z) : bool :=
  match window bs with Some _ => true | None => false end.

(* What an accepted byte string means: its decoded instructions, run from
   any state, reach the access only inside the payload. *)
Theorem window_sound : forall L0 G memf uo bo,
  (forall a, 0 <= memf 40 a < M32) ->
  forall bs b ws k off w,
  dec (S (length bs)) bs [] = Some (ws, k, off, w) ->
  window bs = Some b ->
  forall rest loc stk,
  crun G memf uo bo ws (L0, rest) = Some (loc, stk) ->
  exists av,
    addr_of k stk = Some av
    /\ (den L0 G memf uo bo b mod M32 + PAYLOAD + len_of L0 G memf uo bo b <= M32 ->
        den L0 G memf uo bo b mod M32 + PAYLOAD <= av mod M32 + off
        /\ av mod M32 + off + w <= den L0 G memf uo bo b mod M32 + PAYLOAD + len_of L0 G memf uo bo b).
Proof.
  intros L0 G memf uo bo Hmem bs b ws k off w Hd Hw rest loc stk Hrun.
  unfold window in Hw. rewrite Hd in Hw.
  exact (check_sound L0 G memf uo bo Hmem ws k off w b rest loc stk Hw Hrun).
Qed.

(* ── the emitter's read `xs[i]` as it ships: the compare, the aborting arm
     with the exit call the stock-WASI transform puts before its trap, the
     load ── *)
Example read_window :
  window_ok
    [ 32; 1; 32; 0; 40; 2; 4; 65; 8; 110; 173; 90;
      4; 64; 65; 128; 8; 16; 3; 65; 1; 16; 0; 0; 11;
      32; 0; 32; 1; 167; 65; 8; 108; 106; 41; 3; 12 ] = true.
Proof. vm_compute. reflexivity. Qed.

(* A read as one corpus module ships it (a contract
   program): a constant index kept in a local, and an arm
   that sets a local and makes four calls before its trap. *)
Example shipped_read_window :
  window_ok
    [ 66; 159; 141; 6; 34; 31; 32; 7; 40; 2; 4; 65; 8; 110; 173; 90;
      4; 64; 65; 240; 0; 33; 3; 65; 143; 1; 32; 3; 16; 5; 16; 2;
      65; 1; 16; 16; 65; 1; 16; 1; 0; 11;
      32; 7; 32; 31; 167; 65; 8; 108; 106; 41; 2; 12 ] = true.
Proof. vm_compute. reflexivity. Qed.

(* An arm that can leave without aborting is not read: a `br` in it. *)
Example refuses_arm_that_branches :
  window_ok
    [ 32; 1; 32; 0; 40; 2; 4; 65; 8; 110; 173; 90;
      4; 64; 12; 0; 0; 11;
      32; 0; 32; 1; 167; 65; 8; 108; 106; 41; 3; 12 ] = false.
Proof. vm_compute. reflexivity. Qed.

(* The window the cutter's own tests hold it to:
   the same read without the exit call, byte for byte the list that test
   expects the cutter to print. *)
Example cutter_read_window :
  window_ok
    [ 32; 1; 32; 0; 40; 2; 4; 65; 8; 110; 173; 90;
      4; 64; 65; 128; 8; 16; 0; 0; 11;
      32; 0; 32; 1; 167; 65; 8; 108; 106; 41; 3; 12 ] = true.
Proof. vm_compute. reflexivity. Qed.

(* Controls: the same bytes with the count in bytes (no division), with the
   access one slot off, with a wider access, and with the arm falling
   through, are refused. *)
Example refuses_count_in_bytes :
  window_ok
    [ 32; 1; 32; 0; 40; 2; 4; 173; 90;
      4; 64; 65; 128; 8; 16; 3; 0; 11;
      32; 0; 32; 1; 167; 65; 8; 108; 106; 41; 3; 12 ] = false.
Proof. vm_compute. reflexivity. Qed.

Example refuses_next_slot :
  window_ok
    [ 32; 1; 32; 0; 40; 2; 4; 65; 8; 110; 173; 90;
      4; 64; 65; 128; 8; 16; 3; 0; 11;
      32; 0; 32; 1; 167; 65; 8; 108; 106; 41; 3; 20 ] = false.
Proof. vm_compute. reflexivity. Qed.

Example refuses_other_index :
  window_ok
    [ 32; 1; 32; 0; 40; 2; 4; 65; 8; 110; 173; 90;
      4; 64; 65; 128; 8; 16; 3; 0; 11;
      32; 0; 32; 2; 167; 65; 8; 108; 106; 41; 3; 12 ] = false.
Proof. vm_compute. reflexivity. Qed.

Example refuses_arm_that_falls_through :
  window_ok
    [ 32; 1; 32; 0; 40; 2; 4; 65; 8; 110; 173; 90;
      4; 64; 65; 128; 8; 16; 3; 11;
      32; 0; 32; 1; 167; 65; 8; 108; 106; 41; 3; 12 ] = false.
Proof. vm_compute. reflexivity. Qed.

(* A constant of thirty-one binary digits where the slot size stands. The
   count match once built `2 ^ k` for such a `k` before it tested `k < 32`,
   and a run on these bytes did not end; it is refused like the others. *)
Example refuses_a_huge_divisor :
  window_ok
    [ 32; 1; 32; 0; 40; 2; 4; 65; 255; 255; 255; 255; 7; 110; 173; 90;
      4; 64; 65; 128; 8; 16; 3; 0; 11;
      32; 0; 32; 1; 167; 65; 8; 108; 106; 41; 3; 12 ] = false.
Proof. vm_compute. reflexivity. Qed.

Print Assumptions window_sound.
