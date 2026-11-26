(* Almide trust spine — the guarded bulk copy, read from the SHIPPED BYTES and
   judged in the kernel.

   GuardWindow.v judges one load or store behind the emitter's one compare,
   by matching that compare. A bulk copy sits behind tests of other shapes
   (a clamp by `select`, a signed floor, a subtraction on the right of the
   compare), and the defects that reached releases were in those tests: an
   addition on the left that wraps. So this file does not match a shape. A
   COPY WINDOW is a straight-line run of the shipped function body that ends
   in `memory.copy`: no store, no loop, no block, no call but the
   allocator's; an aborting arm or a `br_if` is passed with its condition
   zero, and an `if` whose arm does not abort is ENTERED with its condition
   non-zero. The window is decoded from the module's own bytes (`pdec`), run
   symbolically from an empty stack (`prun`), and what that leaves is a
   statement in integer arithmetic,

       the conditions the run passed  ->  one range of the copy lies in
                                          the payload of the named block,

   quantified over every entry state, every memory and every allocation
   result (`vc_body`). Each window's statement is proved on its own and the
   kernel checks that proof; nothing here decides arithmetic.

   One side of the copy is stated at a time (the bytes written, or the bytes
   read), against one of two blocks:

     `WLen h v`   the block the local `h` holds at the copy, its payload
                  being the `len` bytes its length word (at `+ 4`) said in
                  the memory as it was after the window's `v`-th allocation
                  (`v = 0`: as the window found it);
     `WFresh k`   the block the window's `k`-th allocation returned, its
                  payload being the size that allocation was called with.

   `copy_sound`: if a window's statement holds, then in EVERY run of the
   decoded instructions that reaches the copy (any locals, any globals, any
   memory before and after each allocation, any allocation results, any
   stack beneath, with 32- and 64-bit wrap-around as the machine has it),
   that side's range is empty (a count of zero) or lies in
   `[B + 12, B + 12 + len)` for the block and the `len` just named, provided
   that payload lies in the 32-bit address space.

   What is modelled, and so trusted: the meaning of the instructions `usem`
   and `bsem` give one (each reduces its operands to its width first, so no
   assumption is made about what a local holds), that every other numeric
   instruction is some function of its operands, that a load reads memory
   and changes nothing, that `select` picks by its third operand, that an
   arm ending in `unreachable` with no branch does not fall through, that a
   call takes one operand and leaves one result and may change memory and
   globals but no local, and that `memory.copy` touches `n` bytes from each
   of its two addresses. A trap only removes runs. Not here: that the bytes
   are a run of one function's instructions at one nesting depth, in which
   the `if`s entered are still open at the copy (the cutter); that the index
   called is the allocator's (the anchor file's `alloc`; that the function
   there is the proven allocator is proofs/check-shipped-routines.sh's
   verdict); and the heap model: a named local holds a live block whose
   length word is its payload length, the allocator returns a block with at
   least the payload asked for, and it leaves the length word of every
   other live block as it was. *)

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
  | PNonzero     (* an `if` entered: the run goes on with a non-zero *)
  | PAlloc.      (* a call of the allocator: memory and globals are new after it *)

(* A value as an expression in what the window found: the locals on entry,
   the globals and memory of each stretch between allocations (numbered from
   0), and what each allocation returned. *)
Inductive ptm : Type :=
  | XL (x : Z)
  | XG (v : nat) (g : Z)
  | XK (z : Z)
  | XM (v : nat) (op off : Z) (a : ptm)
  | XA (k : nat)
  | XU (o : uop) (a : ptm)
  | XB (o : bop) (a b : ptm)
  | XS (a b c : ptm).

(* ── the run over expressions: syntax only ── *)

Fixpoint plook (e : list (Z * ptm)) (x : Z) : ptm :=
  match e with
  | [] => XL x
  | (y, t) :: r => if y =? x then t else plook r x
  end.

(* A fact: the term was non-zero (true) or zero (false) where the run passed.
   `pver` counts the allocations so far; `pallocs` keeps the size each was
   called with. *)
Record psym : Type := mkP {
  penv : list (Z * ptm);
  pstk : list ptm;
  pfacts : list (bool * ptm);
  pver : nat;
  pallocs : list (nat * ptm)
}.

Definition pstep (i : pin) (s : psym) : option psym :=
  let put e st fs := Some (mkP e st fs (pver s) (pallocs s)) in
  match i with
  | PGet x => put (penv s) (plook (penv s) x :: pstk s) (pfacts s)
  | PSet x => match pstk s with
              | t :: r => put ((x, t) :: penv s) r (pfacts s)
              | [] => None
              end
  | PTee x => match pstk s with
              | t :: r => put ((x, t) :: penv s) (t :: r) (pfacts s)
              | [] => None
              end
  | PGlobal g => put (penv s) (XG (pver s) g :: pstk s) (pfacts s)
  | PConst z => put (penv s) (XK z :: pstk s) (pfacts s)
  | PUn o => match pstk s with
             | a :: r => put (penv s) (XU o a :: r) (pfacts s)
             | [] => None
             end
  | PBin o => match pstk s with
              | b :: a :: r => put (penv s) (XB o a b :: r) (pfacts s)
              | _ => None
              end
  | PLoad op off => match pstk s with
                    | a :: r => put (penv s) (XM (pver s) op off a :: r) (pfacts s)
                    | [] => None
                    end
  | PDrop => match pstk s with
             | _ :: r => put (penv s) r (pfacts s)
             | [] => None
             end
  | PSelect => match pstk s with
               | c :: b :: a :: r => put (penv s) (XS a b c :: r) (pfacts s)
               | _ => None
               end
  | PZero => match pstk s with
             | c :: r => put (penv s) r ((false, c) :: pfacts s)
             | [] => None
             end
  | PNonzero => match pstk s with
                | c :: r => put (penv s) r ((true, c) :: pfacts s)
                | [] => None
                end
  | PAlloc => match pstk s with
              | a :: r => Some (mkP (penv s) (XA (pver s) :: r) (pfacts s)
                                    (S (pver s)) ((pver s, a) :: pallocs s))
              | [] => None
              end
  end.

Fixpoint prun (ws : list pin) (s : psym) : option psym :=
  match ws with
  | [] => Some s
  | i :: r => match pstep i s with Some s' => prun r s' | None => None end
  end.

(* The block a side is stated against, as the proposer names it and as the
   run resolves it. *)
Inductive why : Type :=
  | WLen (h : Z) (v : nat)
  | WFresh (k : nat).

Inductive goal : Type :=
  | GLen (b : ptm) (v : nat)
  | GFresh (k : nat) (sz : ptm).

Fixpoint size_of (al : list (nat * ptm)) (k : nat) : option ptm :=
  match al with
  | [] => None
  | (j, t) :: r => if Nat.eqb j k then Some t else size_of r k
  end.

(* What a window leaves: the conditions passed, the copy's destination,
   source and count, and the block the statement is about. *)
Definition cplan (ws : list pin) (w : why)
  : option (list (bool * ptm) * ptm * ptm * ptm * goal) :=
  match prun ws (mkP [] [] [] O []) with
  | Some s =>
      match pstk s with
      | n :: sr :: d :: _ =>
          match w with
          | WLen h v => Some (pfacts s, d, sr, n, GLen (plook (penv s) h) v)
          | WFresh k =>
              match size_of (pallocs s) k with
              | Some sz => Some (pfacts s, d, sr, n, GFresh k sz)
              | None => None
              end
          end
      | _ => None
      end
  | None => None
  end.

Section Semantics.

(* One run's surroundings: the locals on entry; the globals and the memory
   (as each load opcode reads it at an effective address) of each stretch
   between allocations; what each allocation returned; and the instructions
   not given a meaning below. All arbitrary. *)
Variable L0 : Z -> Z.
Variable G : nat -> Z -> Z.
Variable mem : nat -> Z -> Z -> Z.
Variable ret : nat -> Z.
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
  | XG v g => G v g
  | XK z => z
  | XM v op off a => mem v op (u32 (pden a) + off)
  | XA k => ret k
  | XU o a => usem o (pden a)
  | XB o a b => bsem o (pden a) (pden b)
  | XS a b c => if u32 (pden c) =? 0 then pden b else pden a
  end.

(* ── the machine: the locals, the stack, how many allocations so far, and
     the size each was called with (a record only: nothing reads it) ── *)

Definition pconf : Type := ((Z -> Z) * list Z * nat * list (nat * Z))%type.

Definition qstep (i : pin) (c : pconf) : option pconf :=
  let '(loc, stk, v, log) := c in
  match i with
  | PGet x => Some (loc, loc x :: stk, v, log)
  | PSet x => match stk with a :: r => Some (upd_loc loc x a, r, v, log) | [] => None end
  | PTee x => match stk with a :: r => Some (upd_loc loc x a, a :: r, v, log) | [] => None end
  | PGlobal g => Some (loc, G v g :: stk, v, log)
  | PConst z => Some (loc, z :: stk, v, log)
  | PUn o => match stk with a :: r => Some (loc, usem o a :: r, v, log) | [] => None end
  | PBin o => match stk with b :: a :: r => Some (loc, bsem o a b :: r, v, log) | _ => None end
  | PLoad op off => match stk with
                    | a :: r => Some (loc, mem v op (u32 a + off) :: r, v, log)
                    | [] => None
                    end
  | PDrop => match stk with _ :: r => Some (loc, r, v, log) | [] => None end
  | PSelect => match stk with
               | c :: b :: a :: r => Some (loc, (if u32 c =? 0 then b else a) :: r, v, log)
               | _ => None
               end
  | PZero => match stk with
             | c :: r => if u32 c =? 0 then Some (loc, r, v, log) else None
             | [] => None
             end
  | PNonzero => match stk with
                | c :: r => if u32 c =? 0 then None else Some (loc, r, v, log)
                | [] => None
                end
  | PAlloc => match stk with
              | a :: r => Some (loc, ret v :: r, S v, (v, a) :: log)
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

Definition den_log (al : list (nat * ptm)) : list (nat * Z) :=
  map (fun p => (fst p, pden (snd p))) al.

Lemma pstep_sound : forall i loc stk v log e st fs al rest loc' stk' v' log' s',
  stk = map pden st ++ rest ->
  (forall x, loc x = pden (plook e x)) ->
  Forall holds fs ->
  log = den_log al ->
  pstep i (mkP e st fs v al) = Some s' ->
  qstep i (loc, stk, v, log) = Some (loc', stk', v', log') ->
  stk' = map pden (pstk s') ++ rest
  /\ (forall x, loc' x = pden (plook (penv s') x))
  /\ Forall holds (pfacts s')
  /\ pver s' = v'
  /\ log' = den_log (pallocs s').
Proof.
  intros i loc stk v log e st fs al rest loc' stk' v' log' s' Hs Hl Hf Hg Hss Hcs.
  subst stk log.
  destruct i.
  - (* local.get *)
    cbn [pstep qstep penv pstk pfacts pver pallocs map app] in Hss, Hcs.
    injection Hss as Es. injection Hcs as El Ek Ev Eg. subst s' loc' stk' v' log'.
    cbn [pstk penv pfacts pver pallocs map app].
    repeat split; auto. rewrite Hl. reflexivity.
  - (* local.set *)
    destruct st as [|t r]; cbn [pstep qstep penv pstk pfacts pver pallocs map app] in Hss, Hcs;
      [discriminate Hss|].
    injection Hss as Es. injection Hcs as El Ek Ev Eg. subst s' loc' stk' v' log'.
    cbn [pstk penv pfacts pver pallocs].
    repeat split; auto.
    intro y. unfold upd_loc. cbn [plook]. destruct (x =? y); [reflexivity | apply Hl].
  - (* local.tee *)
    destruct st as [|t r]; cbn [pstep qstep penv pstk pfacts pver pallocs map app] in Hss, Hcs;
      [discriminate Hss|].
    injection Hss as Es. injection Hcs as El Ek Ev Eg. subst s' loc' stk' v' log'.
    cbn [pstk penv pfacts pver pallocs map app].
    repeat split; auto.
    intro y. unfold upd_loc. cbn [plook]. destruct (x =? y); [reflexivity | apply Hl].
  - (* global.get *)
    cbn [pstep qstep penv pstk pfacts pver pallocs map app] in Hss, Hcs.
    injection Hss as Es. injection Hcs as El Ek Ev Eg. subst s' loc' stk' v' log'.
    cbn [pstk penv pfacts pver pallocs map app pden].
    repeat split; auto.
  - (* const *)
    cbn [pstep qstep penv pstk pfacts pver pallocs map app] in Hss, Hcs.
    injection Hss as Es. injection Hcs as El Ek Ev Eg. subst s' loc' stk' v' log'.
    cbn [pstk penv pfacts pver pallocs map app pden].
    repeat split; auto.
  - (* unary *)
    destruct st as [|a r]; cbn [pstep qstep penv pstk pfacts pver pallocs map app] in Hss, Hcs;
      [discriminate Hss|].
    injection Hss as Es. injection Hcs as El Ek Ev Eg. subst s' loc' stk' v' log'.
    cbn [pstk penv pfacts pver pallocs map app pden].
    repeat split; auto.
  - (* binary *)
    destruct st as [|b [|a r]]; cbn [pstep qstep penv pstk pfacts pver pallocs map app] in Hss, Hcs;
      [discriminate Hss | discriminate Hss |].
    injection Hss as Es. injection Hcs as El Ek Ev Eg. subst s' loc' stk' v' log'.
    cbn [pstk penv pfacts pver pallocs map app pden].
    repeat split; auto.
  - (* load *)
    destruct st as [|a r]; cbn [pstep qstep penv pstk pfacts pver pallocs map app] in Hss, Hcs;
      [discriminate Hss|].
    injection Hss as Es. injection Hcs as El Ek Ev Eg. subst s' loc' stk' v' log'.
    cbn [pstk penv pfacts pver pallocs map app pden].
    repeat split; auto.
  - (* drop *)
    destruct st as [|a r]; cbn [pstep qstep penv pstk pfacts pver pallocs map app] in Hss, Hcs;
      [discriminate Hss|].
    injection Hss as Es. injection Hcs as El Ek Ev Eg. subst s' loc' stk' v' log'.
    cbn [pstk penv pfacts pver pallocs].
    repeat split; auto.
  - (* select *)
    destruct st as [|c [|b [|a r]]]; cbn [pstep qstep penv pstk pfacts pver pallocs map app] in Hss, Hcs;
      [discriminate Hss | discriminate Hss | discriminate Hss |].
    injection Hss as Es. injection Hcs as El Ek Ev Eg. subst s' loc' stk' v' log'.
    cbn [pstk penv pfacts pver pallocs map app pden].
    repeat split; auto.
  - (* past an aborting arm or a br_if: the condition was zero *)
    destruct st as [|c r]; cbn [pstep qstep penv pstk pfacts pver pallocs map app] in Hss, Hcs;
      [discriminate Hss|].
    destruct (u32 (pden c) =? 0) eqn:E; try rewrite E in Hcs; [|discriminate Hcs].
    injection Hss as Es. injection Hcs as El Ek Ev Eg. subst s' loc' stk' v' log'.
    cbn [pstk penv pfacts pver pallocs].
    repeat split; auto.
    constructor; [unfold holds; cbn [fst snd]; apply Z.eqb_eq; exact E | exact Hf].
  - (* inside an `if`: the condition was not zero *)
    destruct st as [|c r]; cbn [pstep qstep penv pstk pfacts pver pallocs map app] in Hss, Hcs;
      [discriminate Hss|].
    destruct (u32 (pden c) =? 0) eqn:E; try rewrite E in Hcs; [discriminate Hcs|].
    injection Hss as Es. injection Hcs as El Ek Ev Eg. subst s' loc' stk' v' log'.
    cbn [pstk penv pfacts pver pallocs].
    repeat split; auto.
    constructor; [unfold holds; cbn [fst snd]; apply Z.eqb_neq; exact E | exact Hf].
  - (* the allocator *)
    destruct st as [|a r]; cbn [pstep qstep penv pstk pfacts pver pallocs map app] in Hss, Hcs;
      [discriminate Hss|].
    injection Hss as Es. injection Hcs as El Ek Ev Eg. subst s' loc' stk' v' log'.
    cbn [pstk penv pfacts pver pallocs map app pden].
    repeat split; auto.
Qed.

Lemma prun_sound : forall ws loc stk v log e st fs al rest loc' stk' v' log' s',
  stk = map pden st ++ rest ->
  (forall x, loc x = pden (plook e x)) ->
  Forall holds fs ->
  log = den_log al ->
  prun ws (mkP e st fs v al) = Some s' ->
  qrun ws (loc, stk, v, log) = Some (loc', stk', v', log') ->
  stk' = map pden (pstk s') ++ rest
  /\ (forall x, loc' x = pden (plook (penv s') x))
  /\ Forall holds (pfacts s')
  /\ pver s' = v'
  /\ log' = den_log (pallocs s').
Proof.
  induction ws as [|i r IH];
    intros loc stk v log e st fs al rest loc' stk' v' log' s' Hs Hl Hf Hg Hss Hcs.
  - cbn [prun qrun] in Hss, Hcs.
    injection Hss as Es. injection Hcs as El Ek Ev Eg. subst s' loc' stk' v' log'.
    cbn [pstk penv pfacts pver pallocs]. repeat split; auto.
  - cbn [prun qrun] in Hss, Hcs.
    destruct (pstep i (mkP e st fs v al)) as [[e1 st1 fs1 v1 al1]|] eqn:E1; try rewrite E1 in Hss;
      [|discriminate Hss].
    destruct (qstep i (loc, stk, v, log)) as [[[[loc1 stk1] w1] log1]|] eqn:E2; try rewrite E2 in Hcs;
      [|discriminate Hcs].
    destruct (pstep_sound i loc stk v log e st fs al rest loc1 stk1 w1 log1
                (mkP e1 st1 fs1 v1 al1) Hs Hl Hf Hg E1 E2) as [H1 [H2 [H3 [H4 H5]]]].
    cbn [pstk penv pfacts pver pallocs] in H1, H2, H3, H4, H5. subst w1.
    exact (IH loc1 stk1 v1 log1 e1 st1 fs1 al1 rest loc' stk' v' log' s' H1 H2 H3 H5 Hss Hcs).
Qed.

(* ══ THE STATEMENT A WINDOW LEAVES ═════════════════════════════════════ *)

(* `n` bytes from address `a` lie in the `len` bytes of payload of the block
   at `B`, as long as that payload lies in the address space. A copy of no
   bytes touches none, wherever its address is (a block whose empty payload
   ends the address space has its payload address wrap to zero). *)
Definition fits (len B a n : Z) : Prop :=
  u32 B + PAYLOAD + len <= M32 ->
  u32 n = 0
  \/ (u32 B + PAYLOAD <= u32 a
      /\ u32 a + u32 n <= u32 B + PAYLOAD + len).

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

Lemma size_of_in : forall al k sz, size_of al k = Some sz -> In (k, sz) al.
Proof.
  induction al as [|[j t] r IH]; intros k sz H; cbn [size_of] in H; [discriminate H|].
  destruct (Nat.eqb j k) eqn:E.
  - injection H as Et. subst t. apply Nat.eqb_eq in E. subst j. left. reflexivity.
  - right. exact (IH k sz H).
Qed.

(* One side of the copy: its destination range (`dst` true) or its source
   range, inside the block the statement is about. *)
Definition vc_body (dst : bool) (p : list (bool * ptm) * ptm * ptm * ptm * goal) : Prop :=
  let '(fs, d, sr, n, g) := p in
  imp fs (match g with
          | GLen b v =>
              fits (mem v 40 (u32 (pden b) + 4)) (pden b) (pden (if dst then d else sr)) (pden n)
          | GFresh k sz =>
              fits (u32 (pden sz)) (ret k) (pden (if dst then d else sr)) (pden n)
          end).

(* The same in the machine's own terms. *)
Definition claim (w : why) (loc : Z -> Z) (log : list (nat * Z)) (a n : Z) : Prop :=
  match w with
  | WLen h v => fits (mem v 40 (u32 (loc h) + 4)) (loc h) a n
  | WFresh k => exists sz, In (k, sz) log /\ fits (u32 sz) (ret k) a n
  end.

Theorem plan_sound : forall ws dst w p rest loc stk v log,
  cplan ws w = Some p ->
  vc_body dst p ->
  qrun ws (L0, rest, O, []) = Some (loc, stk, v, log) ->
  exists nv sv dv r,
    stk = nv :: sv :: dv :: r
    /\ claim w loc log (if dst then dv else sv) nv.
Proof.
  intros ws dst w p rest loc stk v log Hp Hv Hrun. unfold cplan in Hp.
  destruct (prun ws (mkP [] [] [] O [])) as [[e st fs v1 al]|] eqn:Es; [| discriminate Hp].
  cbn [pstk pfacts penv pallocs] in Hp.
  destruct st as [|n [|sr [|d r]]]; try discriminate Hp.
  destruct (prun_sound ws L0 rest O [] [] [] [] [] rest loc stk v log
              (mkP e (n :: sr :: d :: r) fs v1 al)
              eq_refl (fun x => eq_refl) (Forall_nil holds) eq_refl Es Hrun)
    as [Hs [Hl [Hf [_ Hg]]]].
  cbn [pstk pfacts penv pallocs] in Hs, Hl, Hf, Hg.
  exists (pden n), (pden sr), (pden d), (map pden r ++ rest).
  split; [subst stk; reflexivity |].
  destruct w as [h vh | k].
  - injection Hp as Ep. subst p. cbn beta iota in Hv.
    pose proof (imp_elim fs _ Hv Hf) as Hfit.
    cbn [claim]. rewrite (Hl h). destruct dst; exact Hfit.
  - destruct (size_of al k) as [sz|] eqn:Ez; [| discriminate Hp].
    injection Hp as Ep. subst p. cbn beta iota in Hv.
    pose proof (imp_elim fs _ Hv Hf) as Hfit.
    cbn [claim]. exists (pden sz). split.
    + subst log. unfold den_log.
      exact (in_map (fun q => (fst q, pden (snd q))) al (k, sz) (size_of_in al k sz Ez)).
    + destruct dst; exact Hfit.
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
   0x00 0x00) and nothing after. A store, a block, a loop, an `else`, an
   `end`, or a call of anything but the function at `alloc`, before the copy
   is not a window. An `if` is read as an aborting arm when its bytes are
   one (GuardWindow's `darm`), and as an `if` entered otherwise: the bytes
   after it are then its arm, and since no `end` is read, the copy is still
   inside it. *)
Fixpoint pdec (alloc : Z) (fuel : nat) (bs : list Z) (acc : list pin) : option (list pin) :=
  match fuel with
  | O => None
  | S f =>
      match bs with
      | [] => None
      | op :: r =>
          if op =? 32 then
            match uleb 5 r with Some (x, r') => pdec alloc f r' (PGet x :: acc) | None => None end
          else if op =? 33 then
            match uleb 5 r with Some (x, r') => pdec alloc f r' (PSet x :: acc) | None => None end
          else if op =? 34 then
            match uleb 5 r with Some (x, r') => pdec alloc f r' (PTee x :: acc) | None => None end
          else if op =? 35 then
            match uleb 5 r with Some (x, r') => pdec alloc f r' (PGlobal x :: acc) | None => None end
          else if op =? 65 then
            match sleb 5 r with Some (z, r') => pdec alloc f r' (PConst (z mod M32) :: acc) | None => None end
          else if op =? 66 then
            match sleb 10 r with Some (z, r') => pdec alloc f r' (PConst (z mod M64) :: acc) | None => None end
          else if op =? 26 then pdec alloc f r (PDrop :: acc)
          else if op =? 27 then pdec alloc f r (PSelect :: acc)
          else if op =? 13 then
            match uleb 5 r with Some (_, r') => pdec alloc f r' (PZero :: acc) | None => None end
          else if op =? 16 then
            match uleb 5 r with
            | Some (x, r') => if x =? alloc then pdec alloc f r' (PAlloc :: acc) else None
            | None => None
            end
          else if op =? 4 then
            match r with
            | bt :: r1 =>
                if bt =? 64 then
                  match darm f r1 with
                  | Some r2 => pdec alloc f r2 (PZero :: acc)
                  | None => pdec alloc f r1 (PNonzero :: acc)
                  end
                else None
            | [] => None
            end
          else if op =? 252 then
            match r with
            | [10; 0; 0] => Some (rev acc)
            | _ => None
            end
          else if is_un op then pdec alloc f r (PUn (uop_of op) :: acc)
          else if is_bin op then pdec alloc f r (PBin (bop_of op) :: acc)
          else if (40 <=? op) && (op <=? 53) then
            match uleb 5 r with
            | Some (al, r1) =>
                if al <? 64 then
                  match uleb 5 r1 with
                  | Some (off, r2) => pdec alloc f r2 (PLoad op off :: acc)
                  | None => None
                  end
                else None
            | None => None
            end
          else None
      end
  end.

(* The statement of a window's bytes for one side of its copy (`dst` true:
   the range written; false: the range read) and the block named. `alloc` is
   the index the window may call. *)
Definition copy_vc (alloc : Z) (bs : list Z) (dst : bool) (w : why) : Prop :=
  match pdec alloc (S (length bs)) bs [] with
  | Some ws =>
      match cplan ws w with
      | Some p =>
          forall L0 G mem ret uo bo,
            (forall v a, 0 <= mem v 40 a < M32) ->
            vc_body L0 G mem ret uo bo dst p
      | None => False
      end
  | None => False
  end.

Lemma copy_vc_intro : forall alloc bs dst w ws p,
  pdec alloc (S (length bs)) bs [] = Some ws ->
  cplan ws w = Some p ->
  (forall L0 G mem ret uo bo,
     (forall v a, 0 <= mem v 40 a < M32) -> vc_body L0 G mem ret uo bo dst p) ->
  copy_vc alloc bs dst w.
Proof.
  intros alloc bs dst w ws p Hd Hp H. unfold copy_vc. rewrite Hd, Hp. exact H.
Qed.

(* What a proved statement means: the window's decoded instructions, run
   from any state, reach the copy only with that side's range inside the
   payload of the block named. The copy's operands are the count, the source
   address and the destination address, from the top of the stack down; the
   run starts with no allocation made. *)
Theorem copy_sound : forall alloc bs dst w,
  copy_vc alloc bs dst w ->
  exists ws,
    pdec alloc (S (length bs)) bs [] = Some ws
    /\ forall L0 G mem ret uo bo,
         (forall v a, 0 <= mem v 40 a < M32) ->
         forall rest loc stk v log,
           qrun G mem ret uo bo ws (L0, rest, O, []) = Some (loc, stk, v, log) ->
           exists nv sv dv r,
             stk = nv :: sv :: dv :: r
             /\ claim mem ret w loc log (if dst then dv else sv) nv.
Proof.
  intros alloc bs dst w H. unfold copy_vc in H.
  destruct (pdec alloc (S (length bs)) bs []) as [ws|] eqn:Ed; [| contradiction].
  destruct (cplan ws w) as [p|] eqn:Ep; [| contradiction].
  exists ws. split; [reflexivity |].
  intros L0 G mem ret uo bo Hmem rest loc stk v log Hrun.
  exact (plan_sound L0 G mem ret uo bo ws dst w p rest loc stk v log Ep
           (H L0 G mem ret uo bo Hmem) Hrun).
Qed.

Print Assumptions copy_sound.

(* ══ PROVING ONE WINDOW'S STATEMENT ════════════════════════════════════ *)

(* The arithmetic a window leaves is words, wraps (`mod`) and compares
   (`if`). `lia` is never asked about a `mod`. First each condition the run
   passed is read back as the compare it was (an `and` of compares is every
   one of them; a `select` on a compare is an `if` on it). Then the `mod`s
   are removed one at a time, innermost first, by the range of the operand;
   where the range is within one wrap either way the three cases are split,
   and a `mod` of something unbounded (a local as the run found it) is
   forgotten down to its bound, under a name it keeps. A branch whose
   conditions contradict is closed as soon as it appears. *)

Lemma mod_down : forall x m, 0 < m -> m <= x < 2 * m -> x mod m = x - m.
Proof.
  intros x m Hm Hx. symmetry. apply (Z.mod_unique x m 1 (x - m)); [left; lia | lia].
Qed.

Lemma mod_up : forall x m, 0 < m -> - m <= x < 0 -> x mod m = x + m.
Proof.
  intros x m Hm Hx. symmetry. apply (Z.mod_unique x m (-1) (x + m)); [left; lia | lia].
Qed.

Lemma land_nz : forall a b, Z.land a b mod M32 <> 0 -> a <> 0 /\ b <> 0.
Proof.
  intros a b H. split; intro E; subst; apply H;
    [rewrite Z.land_0_l | rewrite Z.land_0_r]; reflexivity.
Qed.

Lemma nz_lt : forall x y, (if x <? y then 1 else 0) mod M32 <> 0 -> x < y.
Proof.
  intros x y H. destruct (Z.ltb_spec x y) as [L|L]; [exact L | exfalso; apply H; reflexivity].
Qed.

Lemma nz_ge : forall x y, (if x <? y then 0 else 1) mod M32 <> 0 -> y <= x.
Proof.
  intros x y H. destruct (Z.ltb_spec x y) as [L|L]; [exfalso; apply H; reflexivity | exact L].
Qed.

Lemma z_lt : forall x y, (if x <? y then 1 else 0) mod M32 = 0 -> y <= x.
Proof.
  intros x y H. destruct (Z.ltb_spec x y) as [L|L]; [vm_compute in H; discriminate H | exact L].
Qed.

Lemma z_ge : forall x y, (if x <? y then 0 else 1) mod M32 = 0 -> x < y.
Proof.
  intros x y H. destruct (Z.ltb_spec x y) as [L|L]; [exact L | vm_compute in H; discriminate H].
Qed.

Lemma nz_eq : forall x y, (if x =? y then 1 else 0) mod M32 <> 0 -> x = y.
Proof.
  intros x y H. destruct (Z.eqb_spec x y) as [L|L]; [exact L | exfalso; apply H; reflexivity].
Qed.

Lemma nz_ne : forall x y, (if x =? y then 0 else 1) mod M32 <> 0 -> x <> y.
Proof.
  intros x y H. destruct (Z.eqb_spec x y) as [L|L]; [exfalso; apply H; reflexivity | exact L].
Qed.

Lemma z_eq : forall x y, (if x =? y then 1 else 0) mod M32 = 0 -> x <> y.
Proof.
  intros x y H. destruct (Z.eqb_spec x y) as [L|L]; [vm_compute in H; discriminate H | exact L].
Qed.

Lemma z_ne : forall x y, (if x =? y then 0 else 1) mod M32 = 0 -> x = y.
Proof.
  intros x y H. destruct (Z.eqb_spec x y) as [L|L]; [exact L | vm_compute in H; discriminate H].
Qed.

Lemma sel_lt : forall x y (a b : Z),
  (if (if x <? y then 1 else 0) mod M32 =? 0 then b else a) = (if x <? y then a else b).
Proof. intros x y a b. destruct (x <? y); reflexivity. Qed.

Lemma sel_ge : forall x y (a b : Z),
  (if (if x <? y then 0 else 1) mod M32 =? 0 then b else a) = (if x <? y then b else a).
Proof. intros x y a b. destruct (x <? y); reflexivity. Qed.

Lemma sel_eq : forall x y (a b : Z),
  (if (if x =? y then 1 else 0) mod M32 =? 0 then b else a) = (if x =? y then a else b).
Proof. intros x y a b. destruct (x =? y); reflexivity. Qed.

Lemma sel_ne : forall x y (a b : Z),
  (if (if x =? y then 0 else 1) mod M32 =? 0 then b else a) = (if x =? y then b else a).
Proof. intros x y a b. destruct (x =? y); reflexivity. Qed.

Ltac norm_facts :=
  repeat match goal with
         | H : Z.land _ _ mod M32 <> 0 |- _ =>
             let H1 := fresh "F" in
             let H2 := fresh "F" in
             apply land_nz in H; destruct H as [H1 H2]
         | H : (if _ <? _ then 1 else 0) mod M32 <> 0 |- _ => apply nz_lt in H
         | H : (if _ <? _ then 0 else 1) mod M32 <> 0 |- _ => apply nz_ge in H
         | H : (if _ <? _ then 1 else 0) mod M32 = 0 |- _ => apply z_lt in H
         | H : (if _ <? _ then 0 else 1) mod M32 = 0 |- _ => apply z_ge in H
         | H : (if _ =? _ then 1 else 0) mod M32 <> 0 |- _ => apply nz_eq in H
         | H : (if _ =? _ then 0 else 1) mod M32 <> 0 |- _ => apply nz_ne in H
         | H : (if _ =? _ then 1 else 0) mod M32 = 0 |- _ => apply z_eq in H
         | H : (if _ =? _ then 0 else 1) mod M32 = 0 |- _ => apply z_ne in H
         end.

Ltac norm_selects :=
  repeat first
    [ rewrite sel_lt in * | rewrite sel_ge in * | rewrite sel_eq in * | rewrite sel_ne in * ].

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

(* A forgotten `x mod m` keeps its name: when a later split brings the same
   term back (a local that a `select` passes through), it is the same word. *)
Definition keep (r x m : Z) : Prop := r = x mod m.

Ltac forget_mod x m :=
  innermost x;
  let r := fresh "r" in
  let Hr := fresh "Hr" in
  let E := fresh "E" in
  assert (Hr : 0 <= x mod m < m) by (apply Z.mod_pos_bound; lia);
  remember (x mod m) as r eqn:E in *;
  change (keep r x m) in E.

Ltac known E :=
  let E2 := fresh "E" in
  pose proof E as E2; unfold keep in E2; rewrite <- E2 in *; clear E2.

Ltac any_known :=
  first
    [ match goal with
      | E : keep ?r ?x ?m, H : context [?x mod ?m] |- _ => known E
      end
    | match goal with
      | E : keep ?r ?x ?m |- context [?x mod ?m] => known E
      end ].

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

(* An `if` on a compare of two operands that hold no `mod` (so a branch that
   contradicts the conditions is seen and closed), then any `if`. *)
Ltac if_clean :=
  match goal with
  | |- context [if ?a <? ?b then _ else _] =>
      innermost a; innermost b; destruct (Z.ltb_spec a b); try lia
  | H : context [if ?a <? ?b then _ else _] |- _ =>
      innermost a; innermost b; destruct (Z.ltb_spec a b); try lia
  | |- context [if ?a =? ?b then _ else _] =>
      innermost a; innermost b; destruct (Z.eqb_spec a b); try lia
  | H : context [if ?a =? ?b then _ else _] |- _ =>
      innermost a; innermost b; destruct (Z.eqb_spec a b); try lia
  end.

Ltac if_any :=
  match goal with
  | |- context [if ?a <? ?b then _ else _] => destruct (Z.ltb_spec a b); try lia
  | H : context [if ?a <? ?b then _ else _] |- _ => destruct (Z.ltb_spec a b); try lia
  | |- context [if ?a =? ?b then _ else _] => destruct (Z.eqb_spec a b); try lia
  | H : context [if ?a =? ?b then _ else _] |- _ => destruct (Z.eqb_spec a b); try lia
  end.

(* Every load is a word the run found: name it, keep its range. *)
Ltac name_loads Hmem :=
  repeat match goal with
         | |- context [?mem ?v 40 ?a] =>
             lazymatch type of Hmem with
             | forall u x, 0 <= mem u 40 x < _ => idtac
             end;
             let w := fresh "w" in
             let Hw := fresh "Hw" in
             pose proof (Hmem v a) as Hw;
             set (w := mem v 40 a) in *; clearbody w
         end.

Ltac copy_arith :=
  norm_facts; norm_selects;
  unfold M32, M64, H31, H63, PAYLOAD in *;
  repeat first
    [ any_known | any_easy | any_bits | if_clean | (any_split; try lia) | any_forget | if_any ];
  lia.

Ltac copy_auto :=
  lazymatch goal with
  | |- copy_vc ?alloc ?bs ?dst ?why =>
      let ws := eval vm_compute in (pdec alloc (S (length bs)) bs []) in
      lazymatch ws with
      | Some ?w =>
          let p := eval vm_compute in (cplan w why) in
          lazymatch p with
          | Some ?q =>
              apply (copy_vc_intro alloc bs dst why w q);
              [ vm_compute; reflexivity | vm_compute; reflexivity | ]
          | None => fail "the window does not end in a copy of three operands, or names an allocation it does not make"
          end
      | None => fail "the bytes are not a copy window"
      end
  end;
  let L0 := fresh "L0" in
  let G := fresh "G" in
  let mem := fresh "mem" in
  let ret := fresh "ret" in
  let uo := fresh "uo" in
  let bo := fresh "bo" in
  let Hmem := fresh "Hmem" in
  intros L0 G mem ret uo bo Hmem;
  cbv [vc_body imp holds fits pden usem bsem u32 u64 s32 s64 fst snd];
  name_loads Hmem;
  intros;
  copy_arith.

(* ── a copy inside one list, as the emitter guards it: the count clamped to
     the length, the destination floored at zero and held under
     `len - count` by a subtraction ── *)
Example guarded_copy_within :
  copy_vc 7
    [ 32; 2; 32; 0; 40; 2; 4; 173; 32; 2; 32; 0; 40; 2; 4; 173; 84; 27; 33; 2;
      32; 1; 32; 2; 84;
      32; 3; 66; 0; 89; 113;
      32; 3; 32; 0; 40; 2; 4; 173; 32; 2; 32; 1; 125; 125; 87; 113;
      4; 64;
      32; 0; 65; 12; 106; 32; 3; 167; 106;
      32; 0; 65; 12; 106; 32; 1; 167; 106;
      32; 2; 32; 1; 125; 167;
      252; 10; 0; 0 ] true (WLen 0 0).
Proof. copy_auto. Qed.

(* ── a copy into a block just allocated: `n` bytes asked for, `n` bytes
     written from its payload on; the bytes read are the whole payload of
     the old block, whose length is `n` (read before the allocation) ── *)
Example copy_into_fresh_block_written :
  copy_vc 7
    [ 32; 0; 40; 2; 4; 34; 1; 16; 7; 33; 2;
      32; 2; 65; 12; 106; 32; 0; 65; 12; 106; 32; 1;
      252; 10; 0; 0 ] true (WFresh 0).
Proof. copy_auto. Qed.

Example copy_into_fresh_block_read :
  copy_vc 7
    [ 32; 0; 40; 2; 4; 34; 1; 16; 7; 33; 2;
      32; 2; 65; 12; 106; 32; 0; 65; 12; 106; 32; 1;
      252; 10; 0; 0 ] false (WLen 0 0).
Proof. copy_auto. Qed.

(* Controls: the same with a call of another function (index 8) is not a
   window; and with one byte more copied than asked for, the bytes written
   are not proved inside the new block. *)
Goal copy_vc 7
    [ 32; 0; 40; 2; 4; 34; 1; 16; 8; 33; 2;
      32; 2; 65; 12; 106; 32; 0; 65; 12; 106; 32; 1;
      252; 10; 0; 0 ] true (WFresh 0).
Proof. Fail copy_auto. Abort.

Goal copy_vc 7
    [ 32; 0; 40; 2; 4; 34; 1; 16; 7; 33; 2;
      32; 2; 65; 12; 106; 32; 0; 65; 12; 106; 32; 1; 65; 1; 106;
      252; 10; 0; 0 ] true (WFresh 0).
Proof. Fail copy_auto. Abort.

(* ── `list.copy_within` as it ships (the reproduction of issue 2474,
     function 15 of its module): the same guard, the block reached through
     two locals. Both sides are proved. ── *)
Example shipped_copy_within_written :
  copy_vc 7
    [ 32; 8; 33; 3; 32; 8; 33; 11; 32; 0; 33; 35; 32; 1; 33; 36; 32; 2; 33; 37;
      32; 36; 32; 11; 40; 2; 4; 173; 32; 36; 32; 11; 40; 2; 4; 173; 84; 27; 33; 36;
      32; 35; 32; 36; 84; 32; 37; 66; 0; 89; 113; 32; 37; 32; 11; 40; 2; 4; 173; 32;
      36; 32; 35; 125; 125; 87; 113; 4; 64; 32; 11; 65; 12; 106; 32; 37; 167; 106; 32; 11;
      65; 12; 106; 32; 35; 167; 106; 32; 36; 32; 35; 125; 167; 252; 10; 0; 0 ] true (WLen 3 0).
Proof. copy_auto. Qed.

Example shipped_copy_within_read :
  copy_vc 7
    [ 32; 8; 33; 3; 32; 8; 33; 11; 32; 0; 33; 35; 32; 1; 33; 36; 32; 2; 33; 37;
      32; 36; 32; 11; 40; 2; 4; 173; 32; 36; 32; 11; 40; 2; 4; 173; 84; 27; 33; 36;
      32; 35; 32; 36; 84; 32; 37; 66; 0; 89; 113; 32; 37; 32; 11; 40; 2; 4; 173; 32;
      36; 32; 35; 125; 125; 87; 113; 4; 64; 32; 11; 65; 12; 106; 32; 37; 167; 106; 32; 11;
      65; 12; 106; 32; 35; 167; 106; 32; 36; 32; 35; 125; 167; 252; 10; 0; 0 ] false (WLen 3 0).
Proof. copy_auto. Qed.

(* ── Control: the same function as the compiler emitted it with the defect
     of issue 2474 put back. Its test is `(dst + end) - start <=u len`, an
     addition that wraps for a destination near the top of the 64-bit range,
     and the copy then writes below the payload. The statement for the bytes
     written is not proved; the bytes read are still inside (`start < end <=
     len` is tested apart). ── *)
Goal copy_vc 7
    [ 32; 8; 33; 3; 32; 8; 33; 11; 32; 0; 33; 35; 32; 1; 33; 36; 32; 2; 33; 37;
      32; 36; 32; 11; 40; 2; 4; 173; 32; 36; 32; 11; 40; 2; 4; 173; 84; 27; 33; 36;
      32; 35; 32; 36; 84; 32; 37; 32; 36; 124; 32; 35; 125; 32; 11; 40; 2; 4; 173; 88;
      113; 4; 64; 32; 11; 65; 12; 106; 32; 37; 167; 106; 32; 11; 65; 12; 106; 32; 35; 167;
      106; 32; 36; 32; 35; 125; 167; 252; 10; 0; 0 ] true (WLen 3 0).
Proof. Fail copy_auto. Abort.

Example defect_2474_still_reads_inside :
  copy_vc 7
    [ 32; 8; 33; 3; 32; 8; 33; 11; 32; 0; 33; 35; 32; 1; 33; 36; 32; 2; 33; 37;
      32; 36; 32; 11; 40; 2; 4; 173; 32; 36; 32; 11; 40; 2; 4; 173; 84; 27; 33; 36;
      32; 35; 32; 36; 84; 32; 37; 32; 36; 124; 32; 35; 125; 32; 11; 40; 2; 4; 173; 88;
      113; 4; 64; 32; 11; 65; 12; 106; 32; 37; 167; 106; 32; 11; 65; 12; 106; 32; 35; 167;
      106; 32; 36; 32; 35; 125; 167; 252; 10; 0; 0 ] false (WLen 3 0).
Proof. copy_auto. Qed.

(* ── `list.take` as it ships (the reproduction of issue 2685, function 13 of
     its module, the call with a count near the top of the 64-bit range):
     the byte count is `count * 8` only when `0 <= count < len / 8`, and the
     length otherwise; the new block is asked for that many bytes, and that
     many are copied from the old payload. Both sides are proved, the bytes
     written against the new block, the bytes read against the old one's
     length as read before the allocation. ── *)
Example shipped_take_written :
  copy_vc 7
    [ 32; 0; 33; 9; 66; 255; 255; 255; 255; 255; 255; 255; 255; 255; 0; 33; 33; 32; 33; 66;
      0; 32; 33; 66; 0; 85; 27; 33; 33; 32; 9; 40; 2; 4; 32; 33; 66; 8; 126; 167;
      32; 33; 66; 0; 83; 32; 33; 32; 9; 40; 2; 4; 65; 8; 110; 173; 89; 114; 27; 33;
      10; 32; 10; 16; 7; 33; 11; 32; 11; 65; 12; 106; 32; 9; 65; 12; 106; 32; 10; 252;
      10; 0; 0 ] true (WFresh 0).
Proof. copy_auto. Qed.

Example shipped_take_read :
  copy_vc 7
    [ 32; 0; 33; 9; 66; 255; 255; 255; 255; 255; 255; 255; 255; 255; 0; 33; 33; 32; 33; 66;
      0; 32; 33; 66; 0; 85; 27; 33; 33; 32; 9; 40; 2; 4; 32; 33; 66; 8; 126; 167;
      32; 33; 66; 0; 83; 32; 33; 32; 9; 40; 2; 4; 65; 8; 110; 173; 89; 114; 27; 33;
      10; 32; 10; 16; 7; 33; 11; 32; 11; 65; 12; 106; 32; 9; 65; 12; 106; 32; 10; 252;
      10; 0; 0 ] false (WLen 0 0).
Proof. copy_auto. Qed.

(* ── Control: the same call as the compiler emitted it with the defect of
     issue 2685 put back. The byte count is `min(count * 8, len)` taken
     after the multiplication, which wraps: the bytes read are not proved
     inside the old block. The bytes written still are (the new block is
     asked for exactly the count copied). ── *)
Goal copy_vc 7
    [ 32; 0; 33; 9; 66; 255; 255; 255; 255; 255; 255; 255; 255; 255; 0; 33; 33; 32; 33; 66;
      0; 32; 33; 66; 0; 85; 27; 33; 33; 32; 9; 40; 2; 4; 32; 9; 40; 2; 4; 173;
      32; 33; 66; 8; 126; 32; 33; 66; 8; 126; 32; 9; 40; 2; 4; 173; 85; 27; 167; 32;
      33; 66; 0; 83; 27; 33; 10; 32; 10; 16; 7; 33; 11; 32; 11; 65; 12; 106; 32; 9;
      65; 12; 106; 32; 10; 252; 10; 0; 0 ] false (WLen 0 0).
Proof. Fail copy_auto. Abort.

Example defect_2685_still_writes_inside :
  copy_vc 7
    [ 32; 0; 33; 9; 66; 255; 255; 255; 255; 255; 255; 255; 255; 255; 0; 33; 33; 32; 33; 66;
      0; 32; 33; 66; 0; 85; 27; 33; 33; 32; 9; 40; 2; 4; 32; 9; 40; 2; 4; 173;
      32; 33; 66; 8; 126; 32; 33; 66; 8; 126; 32; 9; 40; 2; 4; 173; 85; 27; 167; 32;
      33; 66; 0; 83; 27; 33; 10; 32; 10; 16; 7; 33; 11; 32; 11; 65; 12; 106; 32; 9;
      65; 12; 106; 32; 10; 252; 10; 0; 0 ] true (WFresh 0).
Proof. copy_auto. Qed.
