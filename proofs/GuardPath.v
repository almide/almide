(* Almide trust spine — an access at the end of a PATH, read from the SHIPPED
   BYTES and judged in the kernel.

   GuardWindow.v, GuardFresh.v and GuardLoop.v each judge an access behind
   one shape of code, by matching the shape. The memory defects that reached
   releases sat in tests of other shapes: a clamp by `select`, a signed
   floor, a product held under a quotient, an offset tested against a length
   it was first added to. So this file does not match a shape. A PATH is a
   run of one function body's bytes, from some instruction to an access
   (a load, a store, a `memory.copy`, a `memory.fill`), read like this:

     - a constant, a local or global read or write, a numeric instruction,
       a load, `drop`, `select`: as the machine runs it;
     - an arm that ends in `unreachable`, and a `br_if`: passed, with the
       condition zero;
     - an `if` whose `end` lies after the access: its first arm is ENTERED
       with the condition non-zero when its `else` also lies after the
       access, and SKIPPED with the condition zero when the access is in
       the second arm;
     - an `if` / `else` / `end` before the access whose arms only compute a
       value: the value of the arm its condition picks;
     - any other block, loop or `if` that closes before the access: every
       local one of its instructions assigns is unknown after it, and so
       are memory and the globals when one of them may write them;
     - a `loop` whose `end` lies after the access: the same, at its first
       instruction, for the whole loop body (so the run stands for every
       turn);
     - a `block` whose `end` lies after the access: entered;
     - a call of the allocator: a new block, memory and globals unknown
       after it; a store, `memory.copy` or `memory.fill` before the access:
       memory unknown after it.

   The path is decoded from the module's own bytes (`pdec`), run
   symbolically from an empty stack (`prun`), and what that leaves is a
   statement in integer arithmetic,

       the conditions the run passed  ->  the bytes the access touches lie
                                          in the payload of the named block,

   quantified over every entry state, every memory, every allocation result
   and every value a forgotten local may hold (`vc_body`). Each path's
   statement is proved on its own and the kernel checks that proof; nothing
   here decides arithmetic.

   One range is stated at a time (the bytes written, or the bytes read),
   against one of two blocks:

     `WLen h v`   the block the local `h` holds at the access, its payload
                  being the `len` bytes its length word (at `+ 4`) said in
                  memory number `v` (0: as the path found it; memory gets a
                  new number at each allocation, each write and each
                  forgetting);
     `WFresh k`   the block the path's allocation number `k` returned, its
                  payload being the size that allocation was called with.

   `path_sound`: if a path's statement holds, then in EVERY run of the
   decoded instructions that reaches the access (any locals, any globals,
   any memories, any allocation results, any values for what is forgotten,
   any stack beneath, with 32- and 64-bit wrap-around as the machine has
   it), that range is empty or lies in `[B + 12, B + 12 + len)` for the
   block and the `len` just named, provided that payload lies in the 32-bit
   address space.

   What the kernel also computes, without a theorem of its own: `scan_arm`
   walks EVERY instruction of a block that is passed or of a loop that is
   entered (GuardFresh.v's `step1` knows each one's length) and collects the
   locals it assigns and whether it may write memory or a global; these are
   what the run forgets there.

   What is modelled, and so trusted: the meaning of the instructions `usem`
   and `bsem` give one (each reduces its operands to its width first, so no
   assumption is made about what a local holds), that every other numeric
   instruction is some function of its operands, that a load reads memory
   and changes nothing, that `select` picks by its third operand, that an
   arm ending in `unreachable` with no branch does not fall through, that
   an `if` / `else` / `end` whose arms only compute leaves what the arm its
   condition picks leaves, that a call takes one operand and leaves one
   result and may change memory and globals but no local, that a store
   takes two operands and a bulk write three and neither changes a local or
   a global, and the step from the scans to the forgetting (a local changes
   only by `local.set` / `local.tee` of its own function; a block's first
   instruction is reached only from the instruction before the block, and a
   loop's also from inside the loop; the stack beneath a block is as the
   block found it). A trap only removes runs. Not here: that the bytes are a
   run of one function's instructions and that the bytes after the access
   are the ones that follow it in that function (the cutter); that the index
   called is the allocator's (the anchor file's `alloc`; that the function
   there is the proven allocator is proofs/check-shipped-routines.sh's
   verdict); and the heap model: a named local holds a live block whose
   length word is its payload length, the allocator returns a block with at
   least the payload asked for, blocks do not overlap, and so a write that
   lies in one block's payload leaves the length word of every block as it
   was. A statement `WLen h v` is about the length word in memory `v`: that
   the access finds it unchanged is this heap model's, once every write
   between the two is itself proved to lie in a block's payload. *)

From Stdlib Require Import ZArith List Lia Bool.
From AlmideTrust Require Import GuardWindow GuardFresh.
Import ListNotations.
Open Scope Z_scope.

Definition H31 : Z := 2147483648.
Definition H63 : Z := 9223372036854775808.

(* ══ THE PATH LANGUAGE ═════════════════════════════════════════════════ *)

Inductive uop : Type :=
  | UEqz32 | UEqz64 | UWrap | UExtU | UExtS | UOther (op : Z).

Inductive bop : Type :=
  | BEq32 | BNe32 | BLtS32 | BLtU32 | BGtS32 | BGtU32 | BLeS32 | BLeU32 | BGeS32 | BGeU32
  | BEq64 | BNe64 | BLtS64 | BLtU64 | BGtS64 | BGtU64 | BLeS64 | BLeU64 | BGeS64 | BGeU64
  | BAdd32 | BSub32 | BMul32 | BDivU32 | BAnd32 | BOr32 | BShl32 | BShrU32
  | BAdd64 | BSub64 | BMul64 | BDivS64 | BDivU64
  | BOther (op : Z).

Inductive pin : Type :=
  | PGet (x : Z)
  | PSet (x : Z)
  | PTee (x : Z)
  | PGlobal (g : Z)
  | PGSet        (* global.set: the globals are new after it *)
  | PConst (z : Z)
  | PUn (o : uop)
  | PBin (o : bop)
  | PLoad (op off : Z)
  | PDrop
  | PSelect
  | PPick        (* after both arms of a computing if/else: condition, first, second *)
  | PZero        (* an aborting arm, a br_if, a skipped first arm: the condition was zero *)
  | PNonzero     (* an `if` entered: the condition was not zero *)
  | PAlloc       (* a call of the allocator: memory and globals are new after it *)
  | PStore       (* a store before the access: memory is new after it *)
  | PBulk        (* memory.copy / memory.fill before the access: the same *)
  | PHavoc (xs : list Z) (m g r : bool).
                 (* a block passed or a loop entered: locals `xs` are forgotten,
                    memory (`m`) and the globals (`g`) are new, and a block that
                    leaves a value (`r`) leaves an unknown one *)

(* A value as an expression in what the path found: the locals on entry,
   the globals and the memory under each of their numbers, what each
   allocation returned, and what each forgetting left. *)
Inductive ptm : Type :=
  | XL (x : Z)
  | XG (v : nat) (g : Z)
  | XK (z : Z)
  | XM (v : nat) (op off : Z) (a : ptm)
  | XA (k : nat)
  | XH (k : nat) (x : Z)
  | XJ (k : nat)
  | XU (o : uop) (a : ptm)
  | XB (o : bop) (a b : ptm)
  | XS (a b c : ptm).

(* ── the run over expressions: syntax only ── *)

Fixpoint plook (e : list (Z * ptm)) (x : Z) : ptm :=
  match e with
  | [] => XL x
  | (y, t) :: r => if y =? x then t else plook r x
  end.

Fixpoint inb (x : Z) (xs : list Z) : bool :=
  match xs with
  | [] => false
  | y :: r => (y =? x) || inb x r
  end.

Fixpoint hav_env (k : nat) (xs : list Z) (e : list (Z * ptm)) : list (Z * ptm) :=
  match xs with
  | [] => e
  | x :: r => (x, XH k x) :: hav_env k r e
  end.

Lemma plook_hav : forall k xs e x,
  plook (hav_env k xs e) x = if inb x xs then XH k x else plook e x.
Proof.
  induction xs as [|y r IH]; intros e x; cbn [hav_env plook inb]; [reflexivity|].
  destruct (y =? x) eqn:E; cbn [orb].
  - apply Z.eqb_eq in E. subst y. reflexivity.
  - apply IH.
Qed.

(* A fact: the term was non-zero (true) or zero (false) where the run passed.
   `pver` numbers the events so far (an allocation, a write, a forgetting, a
   global.set); `pmem` and `pglo` are the numbers of the memory and of the
   globals the run now reads; `pallocs` keeps the size each allocation was
   called with, under its event's number. *)
Record psym : Type := mkP {
  penv : list (Z * ptm);
  pstk : list ptm;
  pfacts : list (bool * ptm);
  pver : nat;
  pmem : nat;
  pglo : nat;
  pallocs : list (nat * ptm)
}.

Definition pstep (i : pin) (s : psym) : option psym :=
  let put e st fs := Some (mkP e st fs (pver s) (pmem s) (pglo s) (pallocs s)) in
  let v := pver s in
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
  | PGlobal g => put (penv s) (XG (pglo s) g :: pstk s) (pfacts s)
  | PGSet => match pstk s with
             | _ :: r => Some (mkP (penv s) r (pfacts s) (S v) (pmem s) (S v) (pallocs s))
             | [] => None
             end
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
                    | a :: r => put (penv s) (XM (pmem s) op off a :: r) (pfacts s)
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
  | PPick => match pstk s with
             | b :: a :: c :: r => put (penv s) (XS a b c :: r) (pfacts s)
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
              | a :: r => Some (mkP (penv s) (XA v :: r) (pfacts s)
                                    (S v) (S v) (S v) ((v, a) :: pallocs s))
              | [] => None
              end
  | PStore => match pstk s with
              | _ :: _ :: r => Some (mkP (penv s) r (pfacts s) (S v) (S v) (pglo s) (pallocs s))
              | _ => None
              end
  | PBulk => match pstk s with
             | _ :: _ :: _ :: r => Some (mkP (penv s) r (pfacts s) (S v) (S v) (pglo s) (pallocs s))
             | _ => None
             end
  | PHavoc xs m g r =>
      Some (mkP (hav_env v xs (penv s))
                (if r then XJ v :: pstk s else pstk s)
                (pfacts s)
                (S v) (if m then S v else pmem s) (if g then S v else pglo s)
                (pallocs s))
  end.

Fixpoint prun (ws : list pin) (s : psym) : option psym :=
  match ws with
  | [] => Some s
  | i :: r => match pstep i s with Some s' => prun r s' | None => None end
  end.

(* The access a path ends in, and the range of it a statement is about. *)
Inductive fin : Type :=
  | FCopy
  | FFill
  | FLoad (off w : Z)
  | FStore (off w : Z).

Inductive range : Type :=
  | RBulk (a n : ptm)            (* `n` bytes from address `a` *)
  | RWord (a : ptm) (off w : Z). (* `w` bytes from address `a` plus `off` *)

(* `dst` true: the bytes written; false: the bytes read. *)
Definition frange (f : fin) (dst : bool) (st : list ptm) : option range :=
  match f with
  | FCopy => match st with
             | n :: sr :: d :: _ => Some (RBulk (if dst then d else sr) n)
             | _ => None
             end
  | FFill => match st with
             | n :: _ :: d :: _ => if dst then Some (RBulk d n) else None
             | _ => None
             end
  | FLoad off w => match st with
                   | a :: _ => if dst then None else Some (RWord a off w)
                   | [] => None
                   end
  | FStore off w => match st with
                    | _ :: a :: _ => if dst then Some (RWord a off w) else None
                    | _ => None
                    end
  end.

(* The block a range is stated against, as the proposer names it and as the
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

(* What a path leaves: the conditions passed, the range, and the block the
   statement is about. *)
Definition pplan (ws : list pin) (f : fin) (dst : bool) (w : why)
  : option (list (bool * ptm) * range * goal) :=
  match prun ws (mkP [] [] [] O O O []) with
  | Some s =>
      match frange f dst (pstk s) with
      | Some rg =>
          match w with
          | WLen h v => Some (pfacts s, rg, GLen (plook (penv s) h) v)
          | WFresh k =>
              match size_of (pallocs s) k with
              | Some sz => Some (pfacts s, rg, GFresh k sz)
              | None => None
              end
          end
      | None => None
      end
  | None => None
  end.

Definition u32 (v : Z) : Z := v mod M32.
Definition u64 (v : Z) : Z := v mod M64.
Definition s32 (v : Z) : Z := (v mod M32 + H31) mod M32 - H31.
Definition s64 (v : Z) : Z := (v mod M64 + H63) mod M64 - H63.

(* `n` bytes from `lo` lie in the `len` bytes of payload of the block at `B`,
   as long as that payload lies in the address space. A range of no bytes
   touches none, wherever it starts (a block whose empty payload ends the
   address space has its payload address wrap to zero). *)
Definition inside (len B lo n : Z) : Prop :=
  u32 B + PAYLOAD + len <= M32 ->
  n = 0
  \/ (u32 B + PAYLOAD <= lo
      /\ lo + n <= u32 B + PAYLOAD + len).

(* The same range in the machine's own terms: the operands are on the stack
   as the access finds them. *)
Definition crange (f : fin) (dst : bool) (stk : list Z) : option (Z * Z) :=
  match f with
  | FCopy => match stk with
             | n :: sr :: d :: _ => Some (u32 (if dst then d else sr), u32 n)
             | _ => None
             end
  | FFill => match stk with
             | n :: _ :: d :: _ => if dst then Some (u32 d, u32 n) else None
             | _ => None
             end
  | FLoad off w => match stk with
                   | a :: _ => if dst then None else Some (u32 a + off, w)
                   | [] => None
                   end
  | FStore off w => match stk with
                    | _ :: a :: _ => if dst then Some (u32 a + off, w) else None
                    | _ => None
                    end
  end.

Section Semantics.

(* One run's surroundings: the locals on entry; the globals and the memory
   (as each load opcode reads it at an effective address) under each number;
   what each allocation returned; what each forgetting left in a local and
   on the stack; and the instructions not given a meaning below. All
   arbitrary. *)
Variable L0 : Z -> Z.
Variable G : nat -> Z -> Z.
Variable mem : nat -> Z -> Z -> Z.
Variable ret : nat -> Z.
Variable hav : nat -> Z -> Z.
Variable jv : nat -> Z.
Variable uo : Z -> Z -> Z.
Variable bo : Z -> Z -> Z -> Z.

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
  | BDivS64 => Z.quot (s64 a) (s64 b) mod M64
  | BDivU64 => u64 a / u64 b
  | BOther op => bo op a b
  end.

Fixpoint pden (t : ptm) : Z :=
  match t with
  | XL x => L0 x
  | XG v g => G v g
  | XK z => z
  | XM v op off a => mem v op (u32 (pden a) + off)
  | XA k => ret k
  | XH k x => hav k x
  | XJ k => jv k
  | XU o a => usem o (pden a)
  | XB o a b => bsem o (pden a) (pden b)
  | XS a b c => if u32 (pden c) =? 0 then pden b else pden a
  end.

(* ── the machine: the locals, the stack, the number of events so far, the
     numbers of the memory and of the globals now read, and the size each
     allocation was called with (a record only: nothing reads it) ── *)

Definition pconf : Type :=
  ((Z -> Z) * list Z * nat * nat * nat * list (nat * Z))%type.

Definition qstep (i : pin) (c : pconf) : option pconf :=
  let '(loc, stk, v, mv, gv, log) := c in
  match i with
  | PGet x => Some (loc, loc x :: stk, v, mv, gv, log)
  | PSet x => match stk with
              | a :: r => Some (upd_loc loc x a, r, v, mv, gv, log)
              | [] => None
              end
  | PTee x => match stk with
              | a :: r => Some (upd_loc loc x a, a :: r, v, mv, gv, log)
              | [] => None
              end
  | PGlobal g => Some (loc, G gv g :: stk, v, mv, gv, log)
  | PGSet => match stk with
             | _ :: r => Some (loc, r, S v, mv, S v, log)
             | [] => None
             end
  | PConst z => Some (loc, z :: stk, v, mv, gv, log)
  | PUn o => match stk with
             | a :: r => Some (loc, usem o a :: r, v, mv, gv, log)
             | [] => None
             end
  | PBin o => match stk with
              | b :: a :: r => Some (loc, bsem o a b :: r, v, mv, gv, log)
              | _ => None
              end
  | PLoad op off => match stk with
                    | a :: r => Some (loc, mem mv op (u32 a + off) :: r, v, mv, gv, log)
                    | [] => None
                    end
  | PDrop => match stk with
             | _ :: r => Some (loc, r, v, mv, gv, log)
             | [] => None
             end
  | PSelect => match stk with
               | c :: b :: a :: r =>
                   Some (loc, (if u32 c =? 0 then b else a) :: r, v, mv, gv, log)
               | _ => None
               end
  | PPick => match stk with
             | b :: a :: c :: r =>
                 Some (loc, (if u32 c =? 0 then b else a) :: r, v, mv, gv, log)
             | _ => None
             end
  | PZero => match stk with
             | c :: r => if u32 c =? 0 then Some (loc, r, v, mv, gv, log) else None
             | [] => None
             end
  | PNonzero => match stk with
                | c :: r => if u32 c =? 0 then None else Some (loc, r, v, mv, gv, log)
                | [] => None
                end
  | PAlloc => match stk with
              | a :: r => Some (loc, ret v :: r, S v, S v, S v, (v, a) :: log)
              | [] => None
              end
  | PStore => match stk with
              | _ :: _ :: r => Some (loc, r, S v, S v, gv, log)
              | _ => None
              end
  | PBulk => match stk with
             | _ :: _ :: _ :: r => Some (loc, r, S v, S v, gv, log)
             | _ => None
             end
  | PHavoc xs m g r =>
      Some (fun y => if inb y xs then hav v y else loc y,
            if r then jv v :: stk else stk,
            S v, if m then S v else mv, if g then S v else gv, log)
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

(* The run over expressions stands for the machine's: every value on the
   stack and in a local is the value of its expression, the conditions hold,
   and the numbers agree. *)
Definition sim (under : list Z) (s : psym) (c : pconf) : Prop :=
  let '(loc, stk, v, mv, gv, log) := c in
  stk = map pden (pstk s) ++ under
  /\ (forall x, loc x = pden (plook (penv s) x))
  /\ Forall holds (pfacts s)
  /\ pver s = v
  /\ pmem s = mv
  /\ pglo s = gv
  /\ log = den_log (pallocs s).

Ltac step_done Hss Hcs :=
  injection Hss as Es; injection Hcs; intros; subst;
  cbn [sim penv pstk pfacts pver pmem pglo pallocs map app pden den_log fst snd];
  repeat split; auto.

Lemma pstep_sound : forall i under s c s' c',
  sim under s c ->
  pstep i s = Some s' ->
  qstep i c = Some c' ->
  sim under s' c'.
Proof.
  intros i under [e st fs v mv gv al] [[[[[loc stk] v0] mv0] gv0] log]
         s' [[[[[loc' stk'] v'] mv'] gv'] log'] H Hss Hcs.
  cbn [sim penv pstk pfacts pver pmem pglo pallocs] in H.
  destruct H as (Hs & Hl & Hf & Hv & Hm & Hg & Hlog). subst stk v0 mv0 gv0 log.
  destruct i; cbn [pstep qstep penv pstk pfacts pver pmem pglo pallocs] in Hss, Hcs.
  - (* local.get *)
    step_done Hss Hcs. rewrite Hl. reflexivity.
  - (* local.set *)
    destruct st as [|t r]; cbn [map app] in Hcs; [discriminate Hss|].
    step_done Hss Hcs.
    intro y. unfold upd_loc. cbn [plook]. destruct (x =? y); [reflexivity | apply Hl].
  - (* local.tee *)
    destruct st as [|t r]; cbn [map app] in Hcs; [discriminate Hss|].
    step_done Hss Hcs.
    intro y. unfold upd_loc. cbn [plook]. destruct (x =? y); [reflexivity | apply Hl].
  - (* global.get *)
    step_done Hss Hcs.
  - (* global.set *)
    destruct st as [|t r]; cbn [map app] in Hcs; [discriminate Hss|].
    step_done Hss Hcs.
  - (* const *)
    step_done Hss Hcs.
  - (* unary *)
    destruct st as [|a r]; cbn [map app] in Hcs; [discriminate Hss|].
    step_done Hss Hcs.
  - (* binary *)
    destruct st as [|b [|a r]]; cbn [map app] in Hcs; [discriminate Hss | discriminate Hss |].
    step_done Hss Hcs.
  - (* load *)
    destruct st as [|a r]; cbn [map app] in Hcs; [discriminate Hss|].
    step_done Hss Hcs.
  - (* drop *)
    destruct st as [|a r]; cbn [map app] in Hcs; [discriminate Hss|].
    step_done Hss Hcs.
  - (* select *)
    destruct st as [|c [|b [|a r]]]; cbn [map app] in Hcs;
      [discriminate Hss | discriminate Hss | discriminate Hss |].
    step_done Hss Hcs.
  - (* the value of a computing if/else *)
    destruct st as [|b [|a [|c r]]]; cbn [map app] in Hcs;
      [discriminate Hss | discriminate Hss | discriminate Hss |].
    step_done Hss Hcs.
  - (* the condition was zero *)
    destruct st as [|c r]; cbn [map app] in Hcs; [discriminate Hss|].
    destruct (u32 (pden c) =? 0) eqn:E; [|discriminate Hcs].
    step_done Hss Hcs.
    constructor; [unfold holds; cbn [fst snd]; apply Z.eqb_eq; exact E | exact Hf].
  - (* the condition was not zero *)
    destruct st as [|c r]; cbn [map app] in Hcs; [discriminate Hss|].
    destruct (u32 (pden c) =? 0) eqn:E; [discriminate Hcs|].
    step_done Hss Hcs.
    constructor; [unfold holds; cbn [fst snd]; apply Z.eqb_neq; exact E | exact Hf].
  - (* the allocator *)
    destruct st as [|a r]; cbn [map app] in Hcs; [discriminate Hss|].
    step_done Hss Hcs.
  - (* a store *)
    destruct st as [|b [|a r]]; cbn [map app] in Hcs; [discriminate Hss | discriminate Hss |].
    step_done Hss Hcs.
  - (* a bulk write *)
    destruct st as [|c [|b [|a r]]]; cbn [map app] in Hcs;
      [discriminate Hss | discriminate Hss | discriminate Hss |].
    step_done Hss Hcs.
  - (* a forgetting *)
    step_done Hss Hcs;
      first [ destruct r; reflexivity
            | destruct m; reflexivity
            | destruct g; reflexivity
            | intro y; rewrite plook_hav; destruct (inb y xs); [reflexivity | apply Hl] ].
Qed.

Lemma prun_sound : forall ws under s c s' c',
  sim under s c ->
  prun ws s = Some s' ->
  qrun ws c = Some c' ->
  sim under s' c'.
Proof.
  induction ws as [|i r IH]; intros under s c s' c' H Hss Hcs; cbn [prun qrun] in Hss, Hcs.
  - injection Hss as Es. injection Hcs as Ec. subst s' c'. exact H.
  - destruct (pstep i s) as [s1|] eqn:E1; [|discriminate Hss].
    destruct (qstep i c) as [c1|] eqn:E2; [|discriminate Hcs].
    exact (IH under s1 c1 s' c' (pstep_sound i under s c s1 c1 H E1 E2) Hss Hcs).
Qed.

(* ══ THE STATEMENT A PATH LEAVES ═══════════════════════════════════════ *)

Definition rlo (r : range) : Z :=
  match r with
  | RBulk a _ => u32 (pden a)
  | RWord a off _ => u32 (pden a) + off
  end.

Definition rlen (r : range) : Z :=
  match r with
  | RBulk _ n => u32 (pden n)
  | RWord _ _ w => w
  end.

Lemma frange_den : forall f dst st under rg,
  frange f dst st = Some rg ->
  crange f dst (map pden st ++ under) = Some (rlo rg, rlen rg).
Proof.
  intros f dst st under rg H.
  destruct f; destruct st as [|x1 [|x2 [|x3 r]]]; destruct dst;
    cbn [frange crange map app] in *; try discriminate H;
    injection H as E; subst rg; reflexivity.
Qed.

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

Definition vc_body (p : list (bool * ptm) * range * goal) : Prop :=
  let '(fs, rg, g) := p in
  imp fs (match g with
          | GLen b v => inside (mem v 40 (u32 (pden b) + 4)) (pden b) (rlo rg) (rlen rg)
          | GFresh k sz => inside (u32 (pden sz)) (ret k) (rlo rg) (rlen rg)
          end).

(* The same in the machine's own terms. *)
Definition claim (w : why) (loc : Z -> Z) (log : list (nat * Z)) (lo n : Z) : Prop :=
  match w with
  | WLen h v => inside (mem v 40 (u32 (loc h) + 4)) (loc h) lo n
  | WFresh k => exists sz, In (k, sz) log /\ inside (u32 sz) (ret k) lo n
  end.

Theorem plan_sound : forall ws f dst w p under loc stk v mv gv log,
  pplan ws f dst w = Some p ->
  vc_body p ->
  qrun ws (L0, under, O, O, O, []) = Some (loc, stk, v, mv, gv, log) ->
  exists lo n,
    crange f dst stk = Some (lo, n)
    /\ claim w loc log lo n.
Proof.
  intros ws f dst w p under loc stk v mv gv log Hp Hv Hrun. unfold pplan in Hp.
  destruct (prun ws (mkP [] [] [] O O O [])) as [s|] eqn:Es; [| discriminate Hp].
  assert (H0 : sim under (mkP [] [] [] O O O []) (L0, under, O, O, O, [])).
  { cbn [sim penv pstk pfacts pver pmem pglo pallocs map app den_log plook pden].
    repeat split; auto. }
  pose proof (prun_sound ws under _ _ s _ H0 Es Hrun) as H.
  cbn [sim] in H. destruct H as (Hs & Hl & Hf & _ & _ & _ & Hlog).
  destruct (frange f dst (pstk s)) as [rg|] eqn:Er; [| discriminate Hp].
  exists (rlo rg), (rlen rg).
  split; [subst stk; exact (frange_den f dst (pstk s) under rg Er) |].
  destruct w as [h vh | k].
  - injection Hp as Ep. subst p. cbn beta iota in Hv.
    pose proof (imp_elim _ _ Hv Hf) as Hin.
    cbn [claim]. rewrite (Hl h). exact Hin.
  - destruct (size_of (pallocs s) k) as [sz|] eqn:Ez; [| discriminate Hp].
    injection Hp as Ep. subst p. cbn beta iota in Hv.
    pose proof (imp_elim _ _ Hv Hf) as Hin.
    cbn [claim]. exists (pden sz). split; [| exact Hin].
    subst log. unfold den_log.
    exact (in_map (fun q => (fst q, pden (snd q))) (pallocs s) (k, sz)
             (size_of_in (pallocs s) k sz Ez)).
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
  else if op =? 126 then BMul64 else if op =? 127 then BDivS64
  else if op =? 128 then BDivU64 else BOther op.

(* What a walk over a block found: the bytes ran out before it closed (the
   access is inside it); an instruction `step1` does not read, or an `else`
   where none can be; or its `else` (`els`) or `end`, with the locals
   assigned on the way, whether memory or a global may have been written,
   and the bytes after. *)
Inductive scan : Type :=
  | SOut
  | SBad
  | SAt (els : bool) (xs : list Z) (m g : bool) (r : list Z).

Fixpoint scan_arm (fuel : nat) (els : bool) (depth : Z) (bs : list Z)
                  (xs : list Z) (m g : bool) : scan :=
  match fuel with
  | O => SBad
  | S f =>
      match bs with
      | [] => SOut
      | op :: _ =>
          match step1 bs with
          | None => SBad
          | Some (w, r) =>
              match w with
              | WOpen => scan_arm f els (depth + 1) r xs m g
              | WEnd => if depth =? 0 then SAt false xs m g r
                        else scan_arm f els (depth - 1) r xs m g
              | WElse => if depth =? 0 then (if els then SAt true xs m g r else SBad)
                         else scan_arm f els depth r xs m g
              | WSet x => scan_arm f els depth r (x :: xs) m g
              | WStore => scan_arm f els depth r xs true g
              | WCall => scan_arm f els depth r xs true true
              | WOther => scan_arm f els depth r xs m (g || (op =? 36))
              end
          end
      end
  end.

Definition memarg2 (bs : list Z) : option (Z * list Z) :=
  match uleb 5 bs with
  | Some (al, r1) =>
      if al <? 64 then uleb 5 r1 else None
  | None => None
  end.

(* An arm that only computes, up to the `else` or `end` that closes it: its
   instructions (onto `acc`), how many values it leaves, which of the two
   closed it, and the bytes after. It reads nothing below where it starts. *)
Fixpoint parm (fuel : nat) (bs : list Z) (acc : list pin) (depth : Z)
  : option (list pin * Z * Z * list Z) :=
  match fuel with
  | O => None
  | S f =>
      match bs with
      | [] => None
      | op :: r =>
          if (op =? 5) || (op =? 11) then Some (acc, depth, op, r)
          else if op =? 32 then
            match uleb 5 r with Some (x, r') => parm f r' (PGet x :: acc) (depth + 1) | None => None end
          else if op =? 35 then
            match uleb 5 r with Some (x, r') => parm f r' (PGlobal x :: acc) (depth + 1) | None => None end
          else if op =? 65 then
            match sleb 5 r with
            | Some (z, r') => parm f r' (PConst (z mod M32) :: acc) (depth + 1)
            | None => None
            end
          else if op =? 66 then
            match sleb 10 r with
            | Some (z, r') => parm f r' (PConst (z mod M64) :: acc) (depth + 1)
            | None => None
            end
          else if is_un op then
            if 1 <=? depth then parm f r (PUn (uop_of op) :: acc) depth else None
          else if is_bin op then
            if 2 <=? depth then parm f r (PBin (bop_of op) :: acc) (depth - 1) else None
          else if (40 <=? op) && (op <=? 53) then
            match memarg2 r with
            | Some (off, r') => if 1 <=? depth then parm f r' (PLoad op off :: acc) depth else None
            | None => None
            end
          else None
      end
  end.

(* Both arms of an `if` / `else` / `end`, each leaving one value. *)
Definition pure_arms (f : nat) (bs : list Z) (acc : list pin) : option (list pin * list Z) :=
  match parm f bs acc 0 with
  | Some (acc1, d1, t1, r2) =>
      if (d1 =? 1) && (t1 =? 5) then
        match parm f r2 acc1 0 with
        | Some (acc2, d2, t2, r3) => if (d2 =? 1) && (t2 =? 11) then Some (acc2, r3) else None
        | None => None
        end
      else None
  | None => None
  end.

(* A block type this file reads: none, or one number. *)
Definition is_val (bt : Z) : bool :=
  (bt =? 127) || (bt =? 126) || (bt =? 125) || (bt =? 124).

(* `n` bytes, least first, as one number (the bits of a float constant). *)
Fixpoint le_bytes (n : nat) (bs : list Z) : option (Z * list Z) :=
  match n with
  | O => Some (0, bs)
  | S m =>
      match bs with
      | b :: r =>
          match le_bytes m r with
          | Some (z, r') => Some (b + 256 * z, r')
          | None => None
          end
      | [] => None
      end
  end.

(* A path's bytes `bs`: its instructions, ending in the access and nothing
   after; `rest` is what follows the access in the function, read only to
   find where a loop the path entered closes. An unconditional branch, a
   `return`, a call of anything but the function at `alloc`, or an `else` or
   `end` of a block the path did not open, is not a path. *)
Fixpoint pdec (alloc : Z) (fuel : nat) (bs rest : list Z) (acc : list pin)
  : option (list pin * fin) :=
  match fuel with
  | O => None
  | S f =>
      match bs with
      | [] => None
      | op :: r =>
          if op =? 1 then pdec alloc f r rest acc
          else if op =? 32 then
            match uleb 5 r with Some (x, r') => pdec alloc f r' rest (PGet x :: acc) | None => None end
          else if op =? 33 then
            match uleb 5 r with Some (x, r') => pdec alloc f r' rest (PSet x :: acc) | None => None end
          else if op =? 34 then
            match uleb 5 r with Some (x, r') => pdec alloc f r' rest (PTee x :: acc) | None => None end
          else if op =? 35 then
            match uleb 5 r with Some (x, r') => pdec alloc f r' rest (PGlobal x :: acc) | None => None end
          else if op =? 36 then
            match uleb 5 r with Some (_, r') => pdec alloc f r' rest (PGSet :: acc) | None => None end
          else if op =? 65 then
            match sleb 5 r with
            | Some (z, r') => pdec alloc f r' rest (PConst (z mod M32) :: acc)
            | None => None
            end
          else if op =? 66 then
            match sleb 10 r with
            | Some (z, r') => pdec alloc f r' rest (PConst (z mod M64) :: acc)
            | None => None
            end
          else if op =? 67 then
            match le_bytes 4 r with Some (z, r') => pdec alloc f r' rest (PConst z :: acc) | None => None end
          else if op =? 68 then
            match le_bytes 8 r with Some (z, r') => pdec alloc f r' rest (PConst z :: acc) | None => None end
          else if op =? 26 then pdec alloc f r rest (PDrop :: acc)
          else if op =? 27 then pdec alloc f r rest (PSelect :: acc)
          else if op =? 13 then
            match uleb 5 r with Some (_, r') => pdec alloc f r' rest (PZero :: acc) | None => None end
          else if op =? 16 then
            match uleb 5 r with
            | Some (x, r') => if x =? alloc then pdec alloc f r' rest (PAlloc :: acc) else None
            | None => None
            end
          else if op =? 2 then
            match r with
            | bt :: r1 =>
                if (bt =? 64) || is_val bt then
                  match scan_arm f false 0 r1 [] false false with
                  | SBad => None
                  | SOut => pdec alloc f r1 rest acc
                  | SAt _ xs m g r2 => pdec alloc f r2 rest (PHavoc xs m g (is_val bt) :: acc)
                  end
                else None
            | [] => None
            end
          else if op =? 3 then
            match r with
            | bt :: r1 =>
                if (bt =? 64) || is_val bt then
                  match scan_arm f false 0 r1 [] false false with
                  | SBad => None
                  | SAt _ xs m g r2 => pdec alloc f r2 rest (PHavoc xs m g (is_val bt) :: acc)
                  | SOut =>
                      match scan_arm f false 0 (r1 ++ rest) [] false false with
                      | SAt _ xs m g _ => pdec alloc f r1 rest (PHavoc xs m g false :: acc)
                      | _ => None
                      end
                  end
                else None
            | [] => None
            end
          else if op =? 4 then
            match r with
            | bt :: r1 =>
                if (bt =? 64) || is_val bt then
                  match (if bt =? 64 then darm f r1 else None) with
                  | Some r2 => pdec alloc f r2 rest (PZero :: acc)
                  | None =>
                      match scan_arm f true 0 r1 [] false false with
                      | SBad => None
                      | SOut => pdec alloc f r1 rest (PNonzero :: acc)
                      | SAt false xs m g r2 =>
                          if bt =? 64 then pdec alloc f r2 rest (PHavoc xs m g false :: PDrop :: acc)
                          else None
                      | SAt true xs m g r2 =>
                          match scan_arm f false 0 r2 xs m g with
                          | SBad => None
                          | SOut => pdec alloc f r2 rest (PZero :: acc)
                          | SAt _ xs' m' g' r3 =>
                              match (if is_val bt then pure_arms f r1 acc else None) with
                              | Some (acc', r3') => pdec alloc f r3' rest (PPick :: acc')
                              | None =>
                                  pdec alloc f r3 rest (PHavoc xs' m' g' (is_val bt) :: PDrop :: acc)
                              end
                          end
                      end
                  end
                else None
            | [] => None
            end
          else if op =? 252 then
            match r with
            | 10 :: 0 :: 0 :: r' =>
                match r' with
                | [] => Some (rev acc, FCopy)
                | _ :: _ => pdec alloc f r' rest (PBulk :: acc)
                end
            | 11 :: 0 :: r' =>
                match r' with
                | [] => Some (rev acc, FFill)
                | _ :: _ => pdec alloc f r' rest (PBulk :: acc)
                end
            | _ => None
            end
          else if is_un op then pdec alloc f r rest (PUn (uop_of op) :: acc)
          else if is_bin op then pdec alloc f r rest (PBin (bop_of op) :: acc)
          else if (40 <=? op) && (op <=? 53) then
            match memarg2 r with
            | Some (off, r') =>
                match r' with
                | [] => match width op with Some w => Some (rev acc, FLoad off w) | None => None end
                | _ :: _ => pdec alloc f r' rest (PLoad op off :: acc)
                end
            | None => None
            end
          else if (54 <=? op) && (op <=? 62) then
            match memarg2 r with
            | Some (off, r') =>
                match r' with
                | [] => match width op with Some w => Some (rev acc, FStore off w) | None => None end
                | _ :: _ => pdec alloc f r' rest (PStore :: acc)
                end
            | None => None
            end
          else None
      end
  end.

(* The statement of a path's bytes for one range of its access (`dst` true:
   the bytes written; false: the bytes read) and the block named. `alloc` is
   the index the path may call. *)
Definition path_vc (alloc : Z) (bs rest : list Z) (dst : bool) (w : why) : Prop :=
  match pdec alloc (S (length bs + length rest)) bs rest [] with
  | Some (ws, f) =>
      match pplan ws f dst w with
      | Some p =>
          forall L0 G mem ret hav jv uo bo,
            (forall v a, 0 <= mem v 40 a < M32) ->
            vc_body L0 G mem ret hav jv uo bo p
      | None => False
      end
  | None => False
  end.

Lemma path_vc_intro : forall alloc bs rest dst w ws f p,
  pdec alloc (S (length bs + length rest)) bs rest [] = Some (ws, f) ->
  pplan ws f dst w = Some p ->
  (forall L0 G mem ret hav jv uo bo,
     (forall v a, 0 <= mem v 40 a < M32) -> vc_body L0 G mem ret hav jv uo bo p) ->
  path_vc alloc bs rest dst w.
Proof.
  intros alloc bs rest dst w ws f p Hd Hp H. unfold path_vc. rewrite Hd, Hp. exact H.
Qed.

(* What a proved statement means: the path's decoded instructions, run from
   any state, reach the access only with that range inside the payload of
   the block named. The run starts with no event behind it, on memory and
   globals number 0. *)
Theorem path_sound : forall alloc bs rest dst w,
  path_vc alloc bs rest dst w ->
  exists ws f,
    pdec alloc (S (length bs + length rest)) bs rest [] = Some (ws, f)
    /\ forall L0 G mem ret hav jv uo bo,
         (forall v a, 0 <= mem v 40 a < M32) ->
         forall under loc stk v mv gv log,
           qrun G mem ret hav jv uo bo ws (L0, under, O, O, O, []) = Some (loc, stk, v, mv, gv, log) ->
           exists lo n,
             crange f dst stk = Some (lo, n)
             /\ claim mem ret w loc log lo n.
Proof.
  intros alloc bs rest dst w H. unfold path_vc in H.
  destruct (pdec alloc (S (length bs + length rest)) bs rest []) as [[ws f]|] eqn:Ed; [| contradiction].
  destruct (pplan ws f dst w) as [p|] eqn:Ep; [| contradiction].
  exists ws, f. split; [reflexivity |].
  intros L0 G mem ret hav jv uo bo Hmem under loc stk v mv gv log Hrun.
  exact (plan_sound L0 G mem ret hav jv uo bo ws f dst w p under loc stk v mv gv log Ep
           (H L0 G mem ret hav jv uo bo Hmem) Hrun).
Qed.

Print Assumptions path_sound.

(* ══ PROVING ONE PATH'S STATEMENT ══════════════════════════════════════ *)

(* The arithmetic a path leaves is words, wraps (`mod`), compares (`if`),
   and now and then a quotient or a product of two words. `lia` is never
   asked about a `mod`. First each condition the run passed is read back as
   the compare it was (an `and` of compares is every one of them; a `select`
   on a compare is an `if` on it). Then the `mod`s are removed one at a
   time, innermost first, by the range of the operand; where the range is
   within one wrap either way the three cases are split, and a `mod` of
   something unbounded (a local as the run found it) is forgotten down to
   its bound, under a name it keeps. A quotient of a non-negative word by a
   positive one is named, with the two inequalities that define it; a word
   known to be at most that quotient is multiplied through. A branch whose
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

(* A quotient of a non-negative number by a positive one: what defines it. *)
Lemma div_bounds : forall a b, 0 <= a -> 0 < b ->
  0 <= a / b <= a /\ b * (a / b) <= a < b * (a / b) + b.
Proof.
  intros a b Ha Hb.
  pose proof (Z.div_pos a b Ha Hb) as H0.
  pose proof (Z.mul_div_le a b Hb) as H1.
  pose proof (Z.mul_succ_div_gt a b Hb) as H2.
  unfold Z.succ in H2. rewrite Z.mul_add_distr_l, Z.mul_1_r in H2.
  repeat split; try assumption.
  apply Z.div_le_upper_bound; [exact Hb | nia].
Qed.

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
   term back (a local that a `select` passes through), it is the same word.
   A quotient is never forgotten: it is named with what defines it. *)
Definition keep (r x m : Z) : Prop := r = x mod m.

Ltac forget_mod x m :=
  innermost x;
  lazymatch x with
  | context [Z.quot _ _] => fail
  | _ => idtac
  end;
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

(* A bitwise `and` / `or`, or a quotient, of two numerals, computed. *)
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
      | H : context [Z.quot ?a ?b] |- _ => bits_of a b Z.quot
      end
    | match goal with
      | |- context [Z.land ?a ?b] => bits_of a b Z.land
      | |- context [Z.lor ?a ?b] => bits_of a b Z.lor
      | |- context [Z.quot ?a ?b] => bits_of a b Z.quot
      end ].

(* A quotient of two words whose signs are known: the truncating division is
   the flooring one, and the quotient is named with its two inequalities. *)
Ltac quot_one a b :=
  innermost a; innermost b;
  rewrite (Z.quot_div_nonneg a b) in * by lia;
  let Hq := fresh "Hq" in
  pose proof (div_bounds a b ltac:(lia) ltac:(lia)) as Hq;
  let q := fresh "q" in
  set (q := a / b) in *; clearbody q.

Ltac any_quot :=
  first
    [ match goal with
      | H : context [Z.quot ?a ?b] |- _ => quot_one a b
      end
    | match goal with
      | |- context [Z.quot ?a ?b] => quot_one a b
      end ].

(* `c <= q` with `r * q <= a` in sight: `r * c <= r * q`. *)
Ltac mono_step :=
  match goal with
  | Hq : _ /\ ?r * ?q <= _ < _, Hc : ?c <= ?q |- _ =>
      lazymatch goal with
      | _ : r * c <= r * q |- _ => fail
      | _ => idtac
      end;
      pose proof (Z.mul_le_mono_nonneg_l c q r ltac:(lia) Hc);
      try (pose proof (Z.mul_nonneg_nonneg r c ltac:(lia) ltac:(lia)))
  end.

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

(* What a signed reading leaves once its wrap is gone: `x + 2^63 - 2^63`. *)
Ltac add_clean := rewrite Z.add_simpl_r in *.

Ltac path_arith :=
  norm_facts; norm_selects;
  unfold M32, M64, H31, H63, PAYLOAD in *;
  repeat first
    [ add_clean | any_known | any_easy | any_bits | any_quot | if_clean | mono_step
    | (any_split; try lia) | any_forget | if_any ];
  lia.

Ltac path_auto :=
  lazymatch goal with
  | |- path_vc ?alloc ?bs ?rest ?dst ?why =>
      let d := eval vm_compute in (pdec alloc (S (length bs + length rest)) bs rest []) in
      lazymatch d with
      | Some (?w, ?f) =>
          let p := eval vm_compute in (pplan w f dst why) in
          lazymatch p with
          | Some ?q =>
              apply (path_vc_intro alloc bs rest dst why w f q);
              [ vm_compute; reflexivity | vm_compute; reflexivity | ]
          | None => fail "the access has no such range, or the statement names an allocation the path does not make"
          end
      | None => fail "the bytes are not a path"
      end
  end;
  let L0 := fresh "L0" in
  let G := fresh "G" in
  let mem := fresh "mem" in
  let ret := fresh "ret" in
  let hav := fresh "hav" in
  let jv := fresh "jv" in
  let uo := fresh "uo" in
  let bo := fresh "bo" in
  let Hmem := fresh "Hmem" in
  intros L0 G mem ret hav jv uo bo Hmem;
  cbv [vc_body imp holds inside rlo rlen pden usem bsem u32 u64 s32 s64 fst snd];
  name_loads Hmem;
  intros;
  path_arith.

(* ── the copy windows GuardCopy.v proved, as paths: a copy inside one list
     (the count clamped to the length, the destination floored at zero and
     held under `len - count` by a subtraction), and a copy into a block
     just allocated ── *)
Example guarded_copy_within :
  path_vc 7
    [ 32; 2; 32; 0; 40; 2; 4; 173; 32; 2; 32; 0; 40; 2; 4; 173; 84; 27; 33; 2;
      32; 1; 32; 2; 84;
      32; 3; 66; 0; 89; 113;
      32; 3; 32; 0; 40; 2; 4; 173; 32; 2; 32; 1; 125; 125; 87; 113;
      4; 64;
      32; 0; 65; 12; 106; 32; 3; 167; 106;
      32; 0; 65; 12; 106; 32; 1; 167; 106;
      32; 2; 32; 1; 125; 167;
      252; 10; 0; 0 ] [] true (WLen 0 0).
Proof. path_auto. Qed.

Example copy_into_fresh_block_written :
  path_vc 7
    [ 32; 0; 40; 2; 4; 34; 1; 16; 7; 33; 2;
      32; 2; 65; 12; 106; 32; 0; 65; 12; 106; 32; 1;
      252; 10; 0; 0 ] [] true (WFresh 0).
Proof. path_auto. Qed.

Example copy_into_fresh_block_read :
  path_vc 7
    [ 32; 0; 40; 2; 4; 34; 1; 16; 7; 33; 2;
      32; 2; 65; 12; 106; 32; 0; 65; 12; 106; 32; 1;
      252; 10; 0; 0 ] [] false (WLen 0 0).
Proof. path_auto. Qed.

(* Controls: the same with a call of another function (index 8) is not a
   path; and with one byte more copied than asked for, the bytes written
   are not proved inside the new block. *)
Goal path_vc 7
    [ 32; 0; 40; 2; 4; 34; 1; 16; 8; 33; 2;
      32; 2; 65; 12; 106; 32; 0; 65; 12; 106; 32; 1;
      252; 10; 0; 0 ] [] true (WFresh 0).
Proof. Fail path_auto. Abort.

Goal path_vc 7
    [ 32; 0; 40; 2; 4; 34; 1; 16; 7; 33; 2;
      32; 2; 65; 12; 106; 32; 0; 65; 12; 106; 32; 1; 65; 1; 106;
      252; 10; 0; 0 ] [] true (WFresh 0).
Proof. Fail path_auto. Abort.
