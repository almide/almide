(* Almide trust spine — the guarded bulk copy, read from the SHIPPED BYTES and
   judged in the kernel.

   GuardWindow.v judges one load or store behind the emitter's one compare,
   by matching that compare. A bulk copy sits behind tests of other shapes
   (a clamp by `select`, a signed floor, a subtraction on the right of the
   compare), and the defects that reached releases were in those tests: an
   addition on the left that wraps. So this file does not match a shape. A
   COPY WINDOW is a straight-line run of the shipped function body that ends
   in `memory.copy`: no store, no call, no loop, no block; an aborting arm or
   a `br_if` is passed with its condition zero, and an `if` whose arm does
   not abort is ENTERED with its condition non-zero. The window is decoded
   from the module's own bytes (`pdec`), run symbolically from an empty stack
   (`srun`), and what that leaves is a statement in integer arithmetic,

       the conditions the run passed  ->  one range of the copy lies in
                                          the payload of the named block,

   quantified over every entry state (`vc_body`). Each window's statement is
   proved on its own and the kernel checks that proof; nothing here decides
   arithmetic.

   `copy_sound`: if a window's statement for one side of its copy holds,
   then in EVERY run of the decoded instructions that reaches the copy (any
   locals, any globals, any memory, any stack beneath, with 32- and 64-bit
   wrap-around as the machine has it), that side's range (the bytes written,
   or the bytes read) lies in `[B + 12, B + 12 + len)`, for `B` the value the
   named local holds at the copy and `len` the word at `B + 4`, provided
   that block lies in the 32-bit address space. The two sides are stated and
   proved apart: a copy into a block just allocated has a source side this
   file can judge and a destination side it cannot.

   What is modelled, and so trusted: the meaning of the instructions `usem`
   and `bsem` give one (each reduces its operands to its width first, so no
   assumption is made about what a local holds), that every other numeric
   instruction is some function of its operands, that a load reads memory
   and changes nothing, that `select` picks by its third operand, that an
   arm ending in `unreachable` with no branch does not fall through, and
   that `memory.copy` touches `n` bytes from each of its two addresses. A
   trap only removes runs. Not here: that the bytes are a run of one
   function's instructions at one nesting depth, in which the `if`s entered
   are still open at the copy (the cutter), and that a named local holds a
   live block whose length word is its payload length (the heap invariant). *)

From Stdlib Require Import ZArith List Lia Bool.
From AlmideTrust Require Import GuardWindow.
Import ListNotations.
Open Scope Z_scope.

Definition H31 : Z := 2147483648.
Definition H63 : Z := 9223372036854775808.

(* ══ THE COPY WINDOW LANGUAGE ══════════════════════════════════════════ *)

Inductive uop : Type :=
  | UEqz32 | UEqz64 | UWrap | UExtU | UExtS | UOther (op : Z).

Inductive bop : Type :=
  | BEq32 | BNe32 | BLtS32 | BLtU32 | BGtS32 | BGtU32 | BLeS32 | BLeU32 | BGeS32 | BGeU32
  | BEq64 | BNe64 | BLtS64 | BLtU64 | BGtS64 | BGtU64 | BLeS64 | BLeU64 | BGeS64 | BGeU64
  | BAdd32 | BSub32 | BMul32 | BDivU32 | BAnd32 | BOr32 | BShl32 | BShrU32
  | BAdd64 | BSub64 | BMul64
  | BOther (op : Z).

Inductive pin : Type :=
  | PGet (x : Z)
  | PSet (x : Z)
  | PTee (x : Z)
  | PGlobal (g : Z)
  | PConst (z : Z)
  | PUn (o : uop)
  | PBin (o : bop)
  | PLoad (op off : Z)
  | PDrop
  | PSelect
  | PZero        (* an aborting arm or a br_if: the run goes on with a zero *)
  | PNonzero.    (* an `if` entered: the run goes on with a non-zero *)

Inductive ptm : Type :=
  | XL (x : Z)
  | XG (g : Z)
  | XK (z : Z)
  | XM (op off : Z) (a : ptm)
  | XU (o : uop) (a : ptm)
  | XB (o : bop) (a b : ptm)
  | XS (a b c : ptm).

(* ── the run over expressions: syntax only ── *)

Fixpoint plook (e : list (Z * ptm)) (x : Z) : ptm :=
  match e with
  | [] => XL x
  | (y, t) :: r => if y =? x then t else plook r x
  end.

(* A fact: the term was non-zero (true) or zero (false) where the run passed. *)
Record psym : Type := mkP { penv : list (Z * ptm); pstk : list ptm; pfacts : list (bool * ptm) }.

Definition pstep (i : pin) (s : psym) : option psym :=
  match i with
  | PGet x => Some (mkP (penv s) (plook (penv s) x :: pstk s) (pfacts s))
  | PSet x => match pstk s with
              | t :: r => Some (mkP ((x, t) :: penv s) r (pfacts s))
              | [] => None
              end
  | PTee x => match pstk s with
              | t :: r => Some (mkP ((x, t) :: penv s) (t :: r) (pfacts s))
              | [] => None
              end
  | PGlobal g => Some (mkP (penv s) (XG g :: pstk s) (pfacts s))
  | PConst z => Some (mkP (penv s) (XK z :: pstk s) (pfacts s))
  | PUn o => match pstk s with
             | a :: r => Some (mkP (penv s) (XU o a :: r) (pfacts s))
             | [] => None
             end
  | PBin o => match pstk s with
              | b :: a :: r => Some (mkP (penv s) (XB o a b :: r) (pfacts s))
              | _ => None
              end
  | PLoad op off => match pstk s with
                    | a :: r => Some (mkP (penv s) (XM op off a :: r) (pfacts s))
                    | [] => None
                    end
  | PDrop => match pstk s with
             | _ :: r => Some (mkP (penv s) r (pfacts s))
             | [] => None
             end
  | PSelect => match pstk s with
               | c :: b :: a :: r => Some (mkP (penv s) (XS a b c :: r) (pfacts s))
               | _ => None
               end
  | PZero => match pstk s with
             | c :: r => Some (mkP (penv s) r ((false, c) :: pfacts s))
             | [] => None
             end
  | PNonzero => match pstk s with
                | c :: r => Some (mkP (penv s) r ((true, c) :: pfacts s))
                | [] => None
                end
  end.

Fixpoint prun (ws : list pin) (s : psym) : option psym :=
  match ws with
  | [] => Some s
  | i :: r => match pstep i s with Some s' => prun r s' | None => None end
  end.

(* What a window leaves: the conditions passed, the copy's destination,
   source and count, and what the named local holds at the copy. *)
Definition cplan (ws : list pin) (h : Z)
  : option (list (bool * ptm) * ptm * ptm * ptm * ptm) :=
  match prun ws (mkP [] [] []) with
  | Some s =>
      match pstk s with
      | n :: sr :: d :: _ => Some (pfacts s, d, sr, n, plook (penv s) h)
      | _ => None
      end
  | None => None
  end.

Section Semantics.

Variable L0 : Z -> Z.
Variable G : Z -> Z.
Variable memf : Z -> Z -> Z.
Variable uo : Z -> Z -> Z.
Variable bo : Z -> Z -> Z -> Z.

Definition u32 (v : Z) : Z := v mod M32.
Definition u64 (v : Z) : Z := v mod M64.
Definition s32 (v : Z) : Z := (v mod M32 + H31) mod M32 - H31.
Definition s64 (v : Z) : Z := (v mod M64 + H63) mod M64 - H63.

Definition usem (o : uop) (v : Z) : Z :=
  match o with
  | UEqz32 => if u32 v =? 0 then 1 else 0
  | UEqz64 => if u64 v =? 0 then 1 else 0
  | UWrap => u64 v mod M32
  | UExtU => u32 v
  | UExtS => s32 v mod M64
  | UOther op => uo op v
  end.

Definition bsem (o : bop) (a b : Z) : Z :=
  match o with
  | BEq32 => if u32 a =? u32 b then 1 else 0
  | BNe32 => if u32 a =? u32 b then 0 else 1
  | BLtS32 => if s32 a <? s32 b then 1 else 0
  | BLtU32 => if u32 a <? u32 b then 1 else 0
  | BGtS32 => if s32 b <? s32 a then 1 else 0
  | BGtU32 => if u32 b <? u32 a then 1 else 0
  | BLeS32 => if s32 b <? s32 a then 0 else 1
  | BLeU32 => if u32 b <? u32 a then 0 else 1
  | BGeS32 => if s32 a <? s32 b then 0 else 1
  | BGeU32 => if u32 a <? u32 b then 0 else 1
  | BEq64 => if u64 a =? u64 b then 1 else 0
  | BNe64 => if u64 a =? u64 b then 0 else 1
  | BLtS64 => if s64 a <? s64 b then 1 else 0
  | BLtU64 => if u64 a <? u64 b then 1 else 0
  | BGtS64 => if s64 b <? s64 a then 1 else 0
  | BGtU64 => if u64 b <? u64 a then 1 else 0
  | BLeS64 => if s64 b <? s64 a then 0 else 1
  | BLeU64 => if u64 b <? u64 a then 0 else 1
  | BGeS64 => if s64 a <? s64 b then 0 else 1
  | BGeU64 => if u64 a <? u64 b then 0 else 1
  | BAdd32 => (u32 a + u32 b) mod M32
  | BSub32 => (u32 a - u32 b) mod M32
  | BMul32 => (u32 a * u32 b) mod M32
  | BDivU32 => u32 a / u32 b
  | BAnd32 => Z.land (u32 a) (u32 b)
  | BOr32 => Z.lor (u32 a) (u32 b)
  | BShl32 => (u32 a * 2 ^ (u32 b mod 32)) mod M32
  | BShrU32 => u32 a / 2 ^ (u32 b mod 32)
  | BAdd64 => (u64 a + u64 b) mod M64
  | BSub64 => (u64 a - u64 b) mod M64
  | BMul64 => (u64 a * u64 b) mod M64
  | BOther op => bo op a b
  end.

Fixpoint pden (t : ptm) : Z :=
  match t with
  | XL x => L0 x
  | XG g => G g
  | XK z => z
  | XM op off a => memf op (u32 (pden a) + off)
  | XU o a => usem o (pden a)
  | XB o a b => bsem o (pden a) (pden b)
  | XS a b c => if u32 (pden c) =? 0 then pden b else pden a
  end.

(* ── the machine ── *)

Definition pconf : Type := ((Z -> Z) * list Z)%type.

Definition qstep (i : pin) (c : pconf) : option pconf :=
  let (loc, stk) := c in
  match i with
  | PGet x => Some (loc, loc x :: stk)
  | PSet x => match stk with v :: r => Some (upd_loc loc x v, r) | [] => None end
  | PTee x => match stk with v :: r => Some (upd_loc loc x v, v :: r) | [] => None end
  | PGlobal g => Some (loc, G g :: stk)
  | PConst z => Some (loc, z :: stk)
  | PUn o => match stk with a :: r => Some (loc, usem o a :: r) | [] => None end
  | PBin o => match stk with b :: a :: r => Some (loc, bsem o a b :: r) | _ => None end
  | PLoad op off => match stk with a :: r => Some (loc, memf op (u32 a + off) :: r) | [] => None end
  | PDrop => match stk with _ :: r => Some (loc, r) | [] => None end
  | PSelect => match stk with
               | c :: b :: a :: r => Some (loc, (if u32 c =? 0 then b else a) :: r)
               | _ => None
               end
  | PZero => match stk with
             | c :: r => if u32 c =? 0 then Some (loc, r) else None
             | [] => None
             end
  | PNonzero => match stk with
                | c :: r => if u32 c =? 0 then None else Some (loc, r)
                | [] => None
                end
  end.

Fixpoint qrun (ws : list pin) (c : pconf) : option pconf :=
  match ws with
  | [] => Some c
  | i :: r => match qstep i c with Some c' => qrun r c' | None => None end
  end.

Definition holds (f : bool * ptm) : Prop :=
  if fst f then u32 (pden (snd f)) <> 0 else u32 (pden (snd f)) = 0.

Lemma pstep_sound : forall i loc stk e st fs rest loc' stk' s',
  stk = map pden st ++ rest ->
  (forall x, loc x = pden (plook e x)) ->
  Forall holds fs ->
  pstep i (mkP e st fs) = Some s' ->
  qstep i (loc, stk) = Some (loc', stk') ->
  stk' = map pden (pstk s') ++ rest
  /\ (forall x, loc' x = pden (plook (penv s') x))
  /\ Forall holds (pfacts s').
Proof.
  intros i loc stk e st fs rest loc' stk' s' Hs Hl Hf Hss Hcs. subst stk.
  destruct i.
  - (* local.get *)
    cbn [pstep qstep penv pstk pfacts map app] in Hss, Hcs.
    injection Hss as Es. injection Hcs as El Ek. subst s' loc' stk'.
    cbn [pstk penv pfacts map app].
    split; [rewrite Hl; reflexivity | split; [exact Hl | exact Hf]].
  - (* local.set *)
    destruct st as [|t r]; cbn [pstep qstep penv pstk pfacts map app] in Hss, Hcs;
      [discriminate Hss|].
    injection Hss as Es. injection Hcs as El Ek. subst s' loc' stk'.
    cbn [pstk penv pfacts].
    split; [reflexivity | split; [| exact Hf]].
    intro y. unfold upd_loc. cbn [plook]. destruct (x =? y); [reflexivity | apply Hl].
  - (* local.tee *)
    destruct st as [|t r]; cbn [pstep qstep penv pstk pfacts map app] in Hss, Hcs;
      [discriminate Hss|].
    injection Hss as Es. injection Hcs as El Ek. subst s' loc' stk'.
    cbn [pstk penv pfacts map app].
    split; [reflexivity | split; [| exact Hf]].
    intro y. unfold upd_loc. cbn [plook]. destruct (x =? y); [reflexivity | apply Hl].
  - (* global.get *)
    cbn [pstep qstep penv pstk pfacts map app] in Hss, Hcs.
    injection Hss as Es. injection Hcs as El Ek. subst s' loc' stk'.
    cbn [pstk penv pfacts map app pden].
    split; [reflexivity | split; [exact Hl | exact Hf]].
  - (* const *)
    cbn [pstep qstep penv pstk pfacts map app] in Hss, Hcs.
    injection Hss as Es. injection Hcs as El Ek. subst s' loc' stk'.
    cbn [pstk penv pfacts map app pden].
    split; [reflexivity | split; [exact Hl | exact Hf]].
  - (* unary *)
    destruct st as [|a r]; cbn [pstep qstep penv pstk pfacts map app] in Hss, Hcs;
      [discriminate Hss|].
    injection Hss as Es. injection Hcs as El Ek. subst s' loc' stk'.
    cbn [pstk penv pfacts map app pden].
    split; [reflexivity | split; [exact Hl | exact Hf]].
  - (* binary *)
    destruct st as [|b [|a r]]; cbn [pstep qstep penv pstk pfacts map app] in Hss, Hcs;
      [discriminate Hss | discriminate Hss |].
    injection Hss as Es. injection Hcs as El Ek. subst s' loc' stk'.
    cbn [pstk penv pfacts map app pden].
    split; [reflexivity | split; [exact Hl | exact Hf]].
  - (* load *)
    destruct st as [|a r]; cbn [pstep qstep penv pstk pfacts map app] in Hss, Hcs;
      [discriminate Hss|].
    injection Hss as Es. injection Hcs as El Ek. subst s' loc' stk'.
    cbn [pstk penv pfacts map app pden].
    split; [reflexivity | split; [exact Hl | exact Hf]].
  - (* drop *)
    destruct st as [|a r]; cbn [pstep qstep penv pstk pfacts map app] in Hss, Hcs;
      [discriminate Hss|].
    injection Hss as Es. injection Hcs as El Ek. subst s' loc' stk'.
    cbn [pstk penv pfacts].
    split; [reflexivity | split; [exact Hl | exact Hf]].
  - (* select *)
    destruct st as [|c [|b [|a r]]]; cbn [pstep qstep penv pstk pfacts map app] in Hss, Hcs;
      [discriminate Hss | discriminate Hss | discriminate Hss |].
    injection Hss as Es. injection Hcs as El Ek. subst s' loc' stk'.
    cbn [pstk penv pfacts map app pden].
    split; [reflexivity | split; [exact Hl | exact Hf]].
  - (* past an aborting arm or a br_if: the condition was zero *)
    destruct st as [|c r]; cbn [pstep qstep penv pstk pfacts map app] in Hss, Hcs;
      [discriminate Hss|].
    destruct (u32 (pden c) =? 0) eqn:E; try rewrite E in Hcs; [|discriminate Hcs].
    injection Hss as Es. injection Hcs as El Ek. subst s' loc' stk'.
    cbn [pstk penv pfacts].
    split; [reflexivity | split; [exact Hl |]].
    constructor; [unfold holds; cbn [fst snd]; apply Z.eqb_eq; exact E | exact Hf].
  - (* inside an `if`: the condition was not zero *)
    destruct st as [|c r]; cbn [pstep qstep penv pstk pfacts map app] in Hss, Hcs;
      [discriminate Hss|].
    destruct (u32 (pden c) =? 0) eqn:E; try rewrite E in Hcs; [discriminate Hcs|].
    injection Hss as Es. injection Hcs as El Ek. subst s' loc' stk'.
    cbn [pstk penv pfacts].
    split; [reflexivity | split; [exact Hl |]].
    constructor; [unfold holds; cbn [fst snd]; apply Z.eqb_neq; exact E | exact Hf].
Qed.

Lemma prun_sound : forall ws loc stk e st fs rest loc' stk' s',
  stk = map pden st ++ rest ->
  (forall x, loc x = pden (plook e x)) ->
  Forall holds fs ->
  prun ws (mkP e st fs) = Some s' ->
  qrun ws (loc, stk) = Some (loc', stk') ->
  stk' = map pden (pstk s') ++ rest
  /\ (forall x, loc' x = pden (plook (penv s') x))
  /\ Forall holds (pfacts s').
Proof.
  induction ws as [|i r IH]; intros loc stk e st fs rest loc' stk' s' Hs Hl Hf Hss Hcs.
  - cbn [prun qrun] in Hss, Hcs.
    injection Hss as Es. injection Hcs as El Ek. subst s' loc' stk'.
    cbn [pstk penv pfacts]. split; [exact Hs | split; [exact Hl | exact Hf]].
  - cbn [prun qrun] in Hss, Hcs.
    destruct (pstep i (mkP e st fs)) as [[e1 st1 fs1]|] eqn:E1; try rewrite E1 in Hss;
      [|discriminate Hss].
    destruct (qstep i (loc, stk)) as [[loc1 stk1]|] eqn:E2; try rewrite E2 in Hcs;
      [|discriminate Hcs].
    destruct (pstep_sound i loc stk e st fs rest loc1 stk1 (mkP e1 st1 fs1) Hs Hl Hf E1 E2)
      as [H1 [H2 H3]].
    cbn [pstk penv pfacts] in H1, H2, H3.
    exact (IH loc1 stk1 e1 st1 fs1 rest loc' stk' s' H1 H2 H3 Hss Hcs).
Qed.

(* ══ THE STATEMENT A WINDOW LEAVES ═════════════════════════════════════ *)

(* `n` bytes from address `a` lie in the payload of the block at `B`, as long
   as that block lies in the address space. *)
Definition fits (B a n : Z) : Prop :=
  u32 B + PAYLOAD + memf 40 (u32 B + 4) <= M32 ->
  u32 B + PAYLOAD <= u32 a
  /\ u32 a + u32 n <= u32 B + PAYLOAD + memf 40 (u32 B + 4).

Fixpoint imp (fs : list (bool * ptm)) (P : Prop) : Prop :=
  match fs with
  | [] => P
  | f :: r => holds f -> imp r P
  end.

Lemma imp_elim : forall fs P, imp fs P -> Forall holds fs -> P.
Proof.
  induction fs as [|f r IH]; intros P H Hf; cbn [imp] in H.
  - exact H.
  - inversion Hf as [|f' r' Hh Hr]; subst. exact (IH P (H Hh) Hr).
Qed.

(* One side of the copy: its destination range (`dst` true) or its source
   range, inside the block the named local holds. *)
Definition vc_body (dst : bool) (p : list (bool * ptm) * ptm * ptm * ptm * ptm) : Prop :=
  let '(fs, d, sr, n, b) := p in
  imp fs (fits (pden b) (pden (if dst then d else sr)) (pden n)).

Theorem plan_sound : forall ws dst h p rest loc stk,
  cplan ws h = Some p ->
  vc_body dst p ->
  qrun ws (L0, rest) = Some (loc, stk) ->
  exists nv sv dv r,
    stk = nv :: sv :: dv :: r
    /\ fits (loc h) (if dst then dv else sv) nv.
Proof.
  intros ws dst h p rest loc stk Hp Hv Hrun. unfold cplan in Hp.
  destruct (prun ws (mkP [] [] [])) as [[e st fs]|] eqn:Es; [| discriminate Hp].
  cbn [pstk pfacts penv] in Hp.
  destruct st as [|n [|sr [|d r]]]; try discriminate Hp.
  injection Hp as Ep. subst p. cbn beta iota in Hv.
  destruct (prun_sound ws L0 rest [] [] [] rest loc stk (mkP e (n :: sr :: d :: r) fs)
              eq_refl (fun x => eq_refl) (Forall_nil holds) Es Hrun) as [Hs [Hl Hf]].
  cbn [pstk pfacts penv] in Hs, Hl, Hf.
  pose proof (imp_elim fs _ Hv Hf) as Hfit.
  exists (pden n), (pden sr), (pden d), (map pden r ++ rest).
  split; [subst stk; reflexivity |].
  rewrite (Hl h). destruct dst; exact Hfit.
Qed.

End Semantics.

(* ══ THE BYTES ═════════════════════════════════════════════════════════ *)

Definition uop_of (op : Z) : uop :=
  if op =? 69 then UEqz32 else if op =? 80 then UEqz64
  else if op =? 167 then UWrap else if op =? 172 then UExtS
  else if op =? 173 then UExtU else UOther op.

Definition bop_of (op : Z) : bop :=
  if op =? 70 then BEq32 else if op =? 71 then BNe32
  else if op =? 72 then BLtS32 else if op =? 73 then BLtU32
  else if op =? 74 then BGtS32 else if op =? 75 then BGtU32
  else if op =? 76 then BLeS32 else if op =? 77 then BLeU32
  else if op =? 78 then BGeS32 else if op =? 79 then BGeU32
  else if op =? 81 then BEq64 else if op =? 82 then BNe64
  else if op =? 83 then BLtS64 else if op =? 84 then BLtU64
  else if op =? 85 then BGtS64 else if op =? 86 then BGtU64
  else if op =? 87 then BLeS64 else if op =? 88 then BLeU64
  else if op =? 89 then BGeS64 else if op =? 90 then BGeU64
  else if op =? 106 then BAdd32 else if op =? 107 then BSub32
  else if op =? 108 then BMul32 else if op =? 110 then BDivU32
  else if op =? 113 then BAnd32 else if op =? 114 then BOr32
  else if op =? 116 then BShl32 else if op =? 118 then BShrU32
  else if op =? 124 then BAdd64 else if op =? 125 then BSub64
  else if op =? 126 then BMul64 else BOther op.

(* A copy window's bytes: its instructions, then `memory.copy 0 0` (0xfc 0x0a
   0x00 0x00) and nothing after. A store, a call, a block, a loop, an `else`
   or an `end` before the copy is not a window. An `if` is read as an
   aborting arm when its bytes are one (GuardWindow's `darm`), and as an
   `if` entered otherwise: the bytes after it are then its arm, and since no
   `end` is read, the copy is still inside it. *)
Fixpoint pdec (fuel : nat) (bs : list Z) (acc : list pin) : option (list pin) :=
  match fuel with
  | O => None
  | S f =>
      match bs with
      | [] => None
      | op :: r =>
          if op =? 32 then
            match uleb 5 r with Some (x, r') => pdec f r' (PGet x :: acc) | None => None end
          else if op =? 33 then
            match uleb 5 r with Some (x, r') => pdec f r' (PSet x :: acc) | None => None end
          else if op =? 34 then
            match uleb 5 r with Some (x, r') => pdec f r' (PTee x :: acc) | None => None end
          else if op =? 35 then
            match uleb 5 r with Some (x, r') => pdec f r' (PGlobal x :: acc) | None => None end
          else if op =? 65 then
            match sleb 5 r with Some (z, r') => pdec f r' (PConst (z mod M32) :: acc) | None => None end
          else if op =? 66 then
            match sleb 10 r with Some (z, r') => pdec f r' (PConst (z mod M64) :: acc) | None => None end
          else if op =? 26 then pdec f r (PDrop :: acc)
          else if op =? 27 then pdec f r (PSelect :: acc)
          else if op =? 13 then
            match uleb 5 r with Some (_, r') => pdec f r' (PZero :: acc) | None => None end
          else if op =? 4 then
            match r with
            | bt :: r1 =>
                if bt =? 64 then
                  match darm f r1 with
                  | Some r2 => pdec f r2 (PZero :: acc)
                  | None => pdec f r1 (PNonzero :: acc)
                  end
                else None
            | [] => None
            end
          else if op =? 252 then
            match r with
            | [10; 0; 0] => Some (rev acc)
            | _ => None
            end
          else if is_un op then pdec f r (PUn (uop_of op) :: acc)
          else if is_bin op then pdec f r (PBin (bop_of op) :: acc)
          else if (40 <=? op) && (op <=? 53) then
            match uleb 5 r with
            | Some (al, r1) =>
                if al <? 64 then
                  match uleb 5 r1 with
                  | Some (off, r2) => pdec f r2 (PLoad op off :: acc)
                  | None => None
                  end
                else None
            | None => None
            end
          else None
      end
  end.

(* The statement of a window's bytes for one side of its copy (`dst` true:
   the range written; false: the range read) and the local named as the
   block that range lies in. *)
Definition copy_vc (bs : list Z) (dst : bool) (h : Z) : Prop :=
  match pdec (S (length bs)) bs [] with
  | Some ws =>
      match cplan ws h with
      | Some p =>
          forall L0 G memf uo bo,
            (forall a, 0 <= memf 40 a < M32) ->
            vc_body L0 G memf uo bo dst p
      | None => False
      end
  | None => False
  end.

Lemma copy_vc_intro : forall bs dst h ws p,
  pdec (S (length bs)) bs [] = Some ws ->
  cplan ws h = Some p ->
  (forall L0 G memf uo bo,
     (forall a, 0 <= memf 40 a < M32) -> vc_body L0 G memf uo bo dst p) ->
  copy_vc bs dst h.
Proof.
  intros bs dst h ws p Hd Hp H. unfold copy_vc. rewrite Hd, Hp. exact H.
Qed.

(* What a proved statement means: the window's decoded instructions, run
   from any state, reach the copy only with that side's range inside the
   payload of the block the named local holds there. The copy's operands are
   the count, the source address and the destination address, from the top
   of the stack down. *)
Theorem copy_sound : forall bs dst h,
  copy_vc bs dst h ->
  exists ws,
    pdec (S (length bs)) bs [] = Some ws
    /\ forall L0 G memf uo bo,
         (forall a, 0 <= memf 40 a < M32) ->
         forall rest loc stk,
           qrun G memf uo bo ws (L0, rest) = Some (loc, stk) ->
           exists nv sv dv r,
             stk = nv :: sv :: dv :: r
             /\ fits memf (loc h) (if dst then dv else sv) nv.
Proof.
  intros bs dst h H. unfold copy_vc in H.
  destruct (pdec (S (length bs)) bs []) as [ws|] eqn:Ed; [| contradiction].
  destruct (cplan ws h) as [p|] eqn:Ep; [| contradiction].
  exists ws. split; [reflexivity |].
  intros L0 G memf uo bo Hmem rest loc stk Hrun.
  exact (plan_sound L0 G memf uo bo ws dst h p rest loc stk Ep (H L0 G memf uo bo Hmem) Hrun).
Qed.

Print Assumptions copy_sound.

(* ══ PROVING ONE WINDOW'S STATEMENT ════════════════════════════════════ *)

(* The arithmetic a window leaves is words, wraps (`mod`) and compares
   (`if`). `lia` is never asked about a `mod`: they are removed one at a
   time, innermost first, by the range of the operand, and split where the
   range is within one wrap either way; a `mod` of something unbounded (a
   local as the run found it) is forgotten down to its bound. *)

Lemma mod_down : forall x m, 0 < m -> m <= x < 2 * m -> x mod m = x - m.
Proof.
  intros x m Hm Hx. symmetry. apply (Z.mod_unique x m 1 (x - m)); [left; lia | lia].
Qed.

Lemma mod_up : forall x m, 0 < m -> - m <= x < 0 -> x mod m = x + m.
Proof.
  intros x m Hm Hx. symmetry. apply (Z.mod_unique x m (-1) (x + m)); [left; lia | lia].
Qed.

Ltac split_ifs :=
  repeat first
    [ match goal with
      | |- context [if ?a <? ?b then _ else _] => destruct (Z.ltb_spec a b)
      | H : context [if ?a <? ?b then _ else _] |- _ => destruct (Z.ltb_spec a b)
      | |- context [if ?a =? ?b then _ else _] => destruct (Z.eqb_spec a b)
      | H : context [if ?a =? ?b then _ else _] |- _ => destruct (Z.eqb_spec a b)
      end ].

Ltac innermost x :=
  lazymatch x with
  | context [Z.modulo _ _] => fail
  | _ => idtac
  end.

Ltac easy_mod x m :=
  innermost x;
  first
    [ rewrite (Z.mod_small x m) in * by lia
    | rewrite (mod_down x m) in * by lia
    | rewrite (mod_up x m) in * by lia ].

Ltac split_mod x m :=
  innermost x;
  let C := fresh "C" in
  assert (C : (0 <= x < m) \/ (m <= x < 2 * m) \/ (- m <= x < 0)) by lia;
  destruct C as [C | [C | C]];
  [ rewrite (Z.mod_small x m) in * by lia
  | rewrite (mod_down x m) in * by lia
  | rewrite (mod_up x m) in * by lia ].

Ltac forget_mod x m :=
  innermost x;
  let r := fresh "r" in
  let Hr := fresh "Hr" in
  assert (Hr : 0 <= x mod m < m) by (apply Z.mod_pos_bound; lia);
  set (r := x mod m) in *; clearbody r.

Ltac any_easy :=
  first
    [ match goal with
      | H : context [?x mod ?m] |- _ => easy_mod x m
      end
    | match goal with
      | |- context [?x mod ?m] => easy_mod x m
      end ].

Ltac any_split :=
  first
    [ match goal with
      | H : context [?x mod ?m] |- _ => split_mod x m
      end
    | match goal with
      | |- context [?x mod ?m] => split_mod x m
      end ].

Ltac any_forget :=
  first
    [ match goal with
      | H : context [?x mod ?m] |- _ => forget_mod x m
      end
    | match goal with
      | |- context [?x mod ?m] => forget_mod x m
      end ].

(* A bitwise `and` / `or` of two numerals, computed. *)
Ltac pnum p :=
  lazymatch p with
  | xH => idtac
  | xO ?q => pnum q
  | xI ?q => pnum q
  end.

Ltac znum t :=
  lazymatch t with
  | Z0 => idtac
  | Zpos ?p => pnum p
  | Zneg ?p => pnum p
  end.

Ltac bits_of a b op :=
  znum a; znum b;
  let v := eval vm_compute in (op a b) in
  change (op a b) with v in *.

Ltac any_bits :=
  first
    [ match goal with
      | H : context [Z.land ?a ?b] |- _ => bits_of a b Z.land
      | H : context [Z.lor ?a ?b] |- _ => bits_of a b Z.lor
      end
    | match goal with
      | |- context [Z.land ?a ?b] => bits_of a b Z.land
      | |- context [Z.lor ?a ?b] => bits_of a b Z.lor
      end ].

Ltac kill_mods := repeat first [ any_easy | any_bits | any_split | any_forget ].

(* Every load is a word the run found: name it, keep its range. *)
Ltac name_loads Hmem :=
  repeat match goal with
         | |- context [?memf 40 ?a] =>
             lazymatch type of Hmem with
             | forall x, 0 <= memf 40 x < _ => idtac
             end;
             let w := fresh "w" in
             let Hw := fresh "Hw" in
             pose proof (Hmem a) as Hw;
             set (w := memf 40 a) in *; clearbody w
         end.

Ltac copy_arith :=
  unfold M32, M64, H31, H63, PAYLOAD in *;
  split_ifs; kill_mods; split_ifs; kill_mods; lia.

Ltac copy_auto :=
  lazymatch goal with
  | |- copy_vc ?bs ?dst ?h =>
      let ws := eval vm_compute in (pdec (S (length bs)) bs []) in
      lazymatch ws with
      | Some ?w =>
          let p := eval vm_compute in (cplan w h) in
          lazymatch p with
          | Some ?q =>
              apply (copy_vc_intro bs dst h w q);
              [ vm_compute; reflexivity | vm_compute; reflexivity | ]
          | None => fail "the window does not end in a copy of three operands"
          end
      | None => fail "the bytes are not a copy window"
      end
  end;
  let L0 := fresh "L0" in
  let G := fresh "G" in
  let memf := fresh "memf" in
  let uo := fresh "uo" in
  let bo := fresh "bo" in
  let Hmem := fresh "Hmem" in
  intros L0 G memf uo bo Hmem;
  cbv [vc_body imp holds fits pden usem bsem u32 u64 s32 s64 fst snd];
  name_loads Hmem;
  intros;
  copy_arith.

(* ── a copy inside one list, as the emitter guards it: the count clamped to
     the length, the destination floored at zero and held under
     `len - count` by a subtraction ── *)
Example guarded_copy_within :
  copy_vc
    [ 32; 2; 32; 0; 40; 2; 4; 173; 32; 2; 32; 0; 40; 2; 4; 173; 84; 27; 33; 2;
      32; 1; 32; 2; 84;
      32; 3; 66; 0; 89; 113;
      32; 3; 32; 0; 40; 2; 4; 173; 32; 2; 32; 1; 125; 125; 87; 113;
      4; 64;
      32; 0; 65; 12; 106; 32; 3; 167; 106;
      32; 0; 65; 12; 106; 32; 1; 167; 106;
      32; 2; 32; 1; 125; 167;
      252; 10; 0; 0 ] true 0.
Proof. copy_auto. Qed.

(* ── `list.copy_within` as it ships (the reproduction of issue 2474,
     function 15 of its module): the same guard, the block reached through
     two locals. Both sides are proved. ── *)
Example shipped_copy_within_written :
  copy_vc
    [ 32; 8; 33; 3; 32; 8; 33; 11; 32; 0; 33; 35; 32; 1; 33; 36; 32; 2; 33; 37;
      32; 36; 32; 11; 40; 2; 4; 173; 32; 36; 32; 11; 40; 2; 4; 173; 84; 27; 33; 36;
      32; 35; 32; 36; 84; 32; 37; 66; 0; 89; 113; 32; 37; 32; 11; 40; 2; 4; 173; 32;
      36; 32; 35; 125; 125; 87; 113; 4; 64; 32; 11; 65; 12; 106; 32; 37; 167; 106; 32; 11;
      65; 12; 106; 32; 35; 167; 106; 32; 36; 32; 35; 125; 167; 252; 10; 0; 0 ] true 3.
Proof. copy_auto. Qed.

Example shipped_copy_within_read :
  copy_vc
    [ 32; 8; 33; 3; 32; 8; 33; 11; 32; 0; 33; 35; 32; 1; 33; 36; 32; 2; 33; 37;
      32; 36; 32; 11; 40; 2; 4; 173; 32; 36; 32; 11; 40; 2; 4; 173; 84; 27; 33; 36;
      32; 35; 32; 36; 84; 32; 37; 66; 0; 89; 113; 32; 37; 32; 11; 40; 2; 4; 173; 32;
      36; 32; 35; 125; 125; 87; 113; 4; 64; 32; 11; 65; 12; 106; 32; 37; 167; 106; 32; 11;
      65; 12; 106; 32; 35; 167; 106; 32; 36; 32; 35; 125; 167; 252; 10; 0; 0 ] false 3.
Proof. copy_auto. Qed.

(* ── Control: the same function as the compiler emitted it with the defect
     of issue 2474 put back. Its test is `(dst + end) - start <=u len`, an
     addition that wraps for a destination near the top of the 64-bit range,
     and the copy then writes below the payload. The statement for the bytes
     written is not proved; the bytes read are still inside (`start < end <=
     len` is tested apart). ── *)
Goal copy_vc
    [ 32; 8; 33; 3; 32; 8; 33; 11; 32; 0; 33; 35; 32; 1; 33; 36; 32; 2; 33; 37;
      32; 36; 32; 11; 40; 2; 4; 173; 32; 36; 32; 11; 40; 2; 4; 173; 84; 27; 33; 36;
      32; 35; 32; 36; 84; 32; 37; 32; 36; 124; 32; 35; 125; 32; 11; 40; 2; 4; 173; 88;
      113; 4; 64; 32; 11; 65; 12; 106; 32; 37; 167; 106; 32; 11; 65; 12; 106; 32; 35; 167;
      106; 32; 36; 32; 35; 125; 167; 252; 10; 0; 0 ] true 3.
Proof. Fail copy_auto. Abort.

Example defect_2474_still_reads_inside :
  copy_vc
    [ 32; 8; 33; 3; 32; 8; 33; 11; 32; 0; 33; 35; 32; 1; 33; 36; 32; 2; 33; 37;
      32; 36; 32; 11; 40; 2; 4; 173; 32; 36; 32; 11; 40; 2; 4; 173; 84; 27; 33; 36;
      32; 35; 32; 36; 84; 32; 37; 32; 36; 124; 32; 35; 125; 32; 11; 40; 2; 4; 173; 88;
      113; 4; 64; 32; 11; 65; 12; 106; 32; 37; 167; 106; 32; 11; 65; 12; 106; 32; 35; 167;
      106; 32; 36; 32; 35; 125; 167; 252; 10; 0; 0 ] false 3.
Proof. copy_auto. Qed.
