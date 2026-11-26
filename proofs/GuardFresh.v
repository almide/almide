(* Almide trust spine — the access into a block just allocated, read from the
   SHIPPED BYTES and judged in the kernel (the byte-level check's core, third
   brick).

   GuardWindow.v judges a guarded list access from one straight-line run of
   instructions. This file judges the other common shape: a block is
   allocated with a CONSTANT size, the result is kept in a local, and a load
   or a store later addresses that local plus a constant. Three byte strings
   are read, consecutive in one function body:

     pre    i32.const N ; call A ; local.set x     (or local.tee x)
     mid    any instructions, at any nesting depth, none of which assigns x
     tail   a straight-line run (GuardWindow's window language) that ends in
            the access, whose address is  x + c  with  c + off + width <= 12 + N

   `fresh_tail_sound`: in EVERY run of an accepted tail (any locals, any
   globals, any memory, any stack beneath, 32-bit wrap-around as the machine
   has it), the access touches only bytes in `[r, r + 12 + N)`, `r` being the
   value local `x` held when the tail started, provided that range lies in
   the 32-bit address space.

   What the kernel also computes, without a theorem of its own: `scan_keep`
   walks EVERY instruction of `mid` (`step1` knows each one's length) and
   refuses if one is `local.set x` / `local.tee x`, or if the nesting leaves
   the block `pre` stands in. So `x` still holds what `pre` stored — a local
   changes only by those two instructions of its own function.

   Not here, and so trusted: that the three strings are consecutive
   instruction runs of one function (the cutter is almide-anchor-check), that
   `step1` reads each instruction's length as the binary format has it, the
   step from "no instruction of mid assigns x" to "x is unchanged", and that
   function `A` returns a block of at least `N` payload bytes behind a
   12-byte header (the allocator, StructuralAlloc.v, over unbounded
   integers). *)

From Stdlib Require Import ZArith List Lia Bool.
From AlmideTrust Require Import GuardWindow.
Import ListNotations.
Open Scope Z_scope.

(* ══ ANY INSTRUCTION'S LENGTH ═════════════════════════════════════════ *)

(* What an instruction is, for a scan that only has to step over it. *)
Inductive what : Type :=
  | WOther            (* nothing the scans below look at *)
  | WOpen             (* block, loop, if *)
  | WElse
  | WEnd
  | WSet (x : Z)      (* local.set x, local.tee x *)
  | WStore            (* a store, memory.copy / fill / init, memory.grow *)
  | WCall.            (* call, call_indirect *)

Definition drop_bytes (n : nat) (bs : list Z) : option (list Z) :=
  if (n <=? length bs)%nat then Some (skipn n bs) else None.

Fixpoint ulebs (n : nat) (bs : list Z) : option (list Z) :=
  match n with
  | O => Some bs
  | S m => match uleb 5 bs with Some (_, r) => ulebs m r | None => None end
  end.

Definition one_uleb (w : what) (bs : list Z) : option (what * list Z) :=
  match uleb 5 bs with Some (_, r) => Some (w, r) | None => None end.

Definition memarg (w : what) (bs : list Z) : option (what * list Z) :=
  match uleb 5 bs with
  | Some (al, r1) =>
      if al <? 64 then
        match uleb 5 r1 with Some (_, r2) => Some (w, r2) | None => None end
      else None
  | None => None
  end.

(* One instruction off the front of `bs`: what it is, and the bytes after it.
   An opcode this table does not know reads as nothing, so a scan stops. *)
Definition step1 (bs : list Z) : option (what * list Z) :=
  match bs with
  | [] => None
  | op :: r =>
      if (op =? 0) || (op =? 1) || (op =? 15) || (op =? 26) || (op =? 27) then Some (WOther, r)
      else if (op =? 2) || (op =? 3) || (op =? 4) then
        match sleb 5 r with Some (_, r') => Some (WOpen, r') | None => None end
      else if op =? 5 then Some (WElse, r)
      else if op =? 11 then Some (WEnd, r)
      else if (op =? 12) || (op =? 13) then one_uleb WOther r
      else if op =? 14 then
        match uleb 5 r with
        | Some (n, r1) =>
            if (0 <=? n) && (n <=? Z.of_nat (length r1)) then
              match ulebs (Z.to_nat n) r1 with
              | Some r2 => one_uleb WOther r2
              | None => None
              end
            else None
        | None => None
        end
      else if op =? 16 then one_uleb WCall r
      else if op =? 17 then
        match uleb 5 r with Some (_, r1) => one_uleb WCall r1 | None => None end
      else if op =? 28 then
        match uleb 5 r with
        | Some (n, r1) =>
            if (0 <=? n) && (n <=? Z.of_nat (length r1)) then
              match drop_bytes (Z.to_nat n) r1 with Some r2 => Some (WOther, r2) | None => None end
            else None
        | None => None
        end
      else if (op =? 32) || (op =? 35) || (op =? 36) || (op =? 37) || (op =? 38) || (op =? 210) then
        one_uleb WOther r
      else if op =? 208 then
        match sleb 5 r with Some (_, r') => Some (WOther, r') | None => None end
      else if op =? 209 then Some (WOther, r)
      else if (op =? 33) || (op =? 34) then
        match uleb 5 r with Some (x, r') => Some (WSet x, r') | None => None end
      else if (40 <=? op) && (op <=? 53) then memarg WOther r
      else if (54 <=? op) && (op <=? 62) then memarg WStore r
      else if op =? 63 then
        match r with z :: r' => if z =? 0 then Some (WOther, r') else None | [] => None end
      else if op =? 64 then
        match r with z :: r' => if z =? 0 then Some (WStore, r') else None | [] => None end
      else if op =? 65 then match sleb 5 r with Some (_, r') => Some (WOther, r') | None => None end
      else if op =? 66 then match sleb 10 r with Some (_, r') => Some (WOther, r') | None => None end
      else if op =? 67 then match drop_bytes 4 r with Some r' => Some (WOther, r') | None => None end
      else if op =? 68 then match drop_bytes 8 r with Some r' => Some (WOther, r') | None => None end
      else if (69 <=? op) && (op <=? 196) then Some (WOther, r)
      else if op =? 252 then
        match uleb 5 r with
        | Some (sub, r1) =>
            if (0 <=? sub) && (sub <=? 7) then Some (WOther, r1)
            else if sub =? 10 then
              match r1 with
              | a :: b :: r2 => if (a =? 0) && (b =? 0) then Some (WStore, r2) else None
              | _ => None
              end
            else if sub =? 11 then
              match r1 with
              | a :: r2 => if a =? 0 then Some (WStore, r2) else None
              | _ => None
              end
            else if sub =? 8 then
              match uleb 5 r1 with
              | Some (_, a :: r2) => if a =? 0 then Some (WStore, r2) else None
              | _ => None
              end
            else if (sub =? 12) || (sub =? 14) then
              match uleb 5 r1 with Some (_, r2) => one_uleb WOther r2 | None => None end
            else if (sub =? 9) || (sub =? 13) || ((15 <=? sub) && (sub <=? 17)) then
              one_uleb WOther r1
            else None
        | None => None
        end
      else None
  end.

(* Every instruction of `bs` is read, none assigns local `x`, and the
   nesting never leaves the block the run starts in and ends back in it. *)
Fixpoint scan_keep (fuel : nat) (x depth : Z) (bs : list Z) : bool :=
  match fuel with
  | O => false
  | S f =>
      match bs with
      | [] => depth =? 0
      | _ :: _ =>
          match step1 bs with
          | None => false
          | Some (w, r) =>
              match w with
              | WSet y => if y =? x then false else scan_keep f x depth r
              | WOpen => scan_keep f x (depth + 1) r
              | WEnd => if depth <=? 0 then false else scan_keep f x (depth - 1) r
              | WElse => if depth <=? 0 then false else scan_keep f x depth r
              | _ => scan_keep f x depth r
              end
          end
      end
  end.

(* ══ THE TAIL ═════════════════════════════════════════════════════════ *)

(* The tail's access addresses local `x` plus a constant, inside the header
   and the first `n` payload bytes of the block `x` points at. *)
Definition fresh_tail (x n : Z) (tail : list Z) : bool :=
  match dec (S (length tail)) tail [] with
  | Some (ws, k, off, w) =>
      match srun ws (mkS [] [] []) with
      | Some s =>
          match addr_of k (sstk s) with
          | Some a =>
              match flat a with
              | ([TL y], c) =>
                  (y =? x) && (0 <=? c) && (0 <=? off) && (0 <? w) && (0 <=? n)
                  && (c + off + w <=? PAYLOAD + n)
              | _ => false
              end
          | None => false
          end
      | None => false
      end
  | None => false
  end.

Section Semantics.

Variable L0 : Z -> Z.
Variable G : Z -> Z.
Variable memf : Z -> Z -> Z.
Variable uo : Z -> Z -> Z.
Variable bo : Z -> Z -> Z -> Z.

(* Every run of an accepted tail that reaches its access touches only the
   header and the first `n` payload bytes of the block at the value local
   `x` held on entry, as long as that block lies in the address space. *)
Theorem fresh_tail_sound : forall x n tail ws k off w rest loc stk,
  fresh_tail x n tail = true ->
  dec (S (length tail)) tail [] = Some (ws, k, off, w) ->
  crun G memf uo bo ws (L0, rest) = Some (loc, stk) ->
  exists av,
    addr_of k stk = Some av
    /\ (L0 x mod M32 + PAYLOAD + n <= M32 ->
        L0 x mod M32 <= av mod M32 + off
        /\ av mod M32 + off + w <= L0 x mod M32 + PAYLOAD + n).
Proof.
  intros x n tail ws k off w rest loc stk Hok Hd Hrun.
  unfold fresh_tail in Hok. rewrite Hd in Hok.
  destruct (srun ws (mkS [] [] [])) as [[e st fs]|] eqn:Es; [| discriminate Hok].
  cbn [sstk] in Hok.
  destruct (addr_of k st) as [a|] eqn:Ea; [| discriminate Hok].
  destruct (flat a) as [xs c] eqn:Ef.
  destruct xs as [|t1 xs']; [discriminate Hok |].
  destruct t1 as [y|g|z|op1 off1 a1|op1 a1|op1 a1 b1]; try discriminate Hok.
  destruct xs' as [|t2 xs'']; [| discriminate Hok].
  repeat (apply andb_prop in Hok; destruct Hok as [Hok ?]).
  apply Z.eqb_eq in Hok. subst y.
  repeat match goal with
         | H : (_ <=? _) = true |- _ => apply Z.leb_le in H
         | H : (_ <? _) = true |- _ => apply Z.ltb_lt in H
         end.
  destruct (run_sound L0 G memf uo bo ws L0 rest [] [] [] rest loc stk (mkS e st fs)
              eq_refl (fun v => eq_refl) (Forall_nil (zero32 L0 G memf uo bo)) Es Hrun)
    as [Hs _].
  cbn [sstk] in Hs.
  exists (den L0 G memf uo bo a). split.
  - subst stk.
    destruct k; destruct st as [|s1 [|s2 r']]; cbn [addr_of] in Ea; try discriminate Ea;
      injection Ea as Ea'; subst; reflexivity.
  - intro Hfit.
    pose proof (flat_sound L0 G memf uo bo a) as Hfl.
    rewrite Ef in Hfl. cbn [fst snd sumd den] in Hfl.
    assert (Hr : 0 <= L0 x mod M32 < M32) by (apply Z.mod_pos_bound; unfold M32; lia).
    assert (Hav : den L0 G memf uo bo a mod M32 = L0 x mod M32 + c).
    { rewrite Hfl.
      replace (L0 x + 0 + c) with (L0 x + c) by lia.
      rewrite Z.add_mod by (unfold M32; lia).
      rewrite (Z.mod_small c M32) by (unfold PAYLOAD in *; lia).
      apply Z.mod_small. unfold PAYLOAD in *. lia. }
    rewrite Hav. unfold PAYLOAD in *. lia.
Qed.

End Semantics.

(* ══ THE THREE STRINGS ════════════════════════════════════════════════ *)

(* `pre` read as `i32.const n ; call alloc ; local.set x` (or `local.tee x`),
   and nothing after it: the local and the constant. *)
Definition fresh_pre (alloc : Z) (pre : list Z) : option (Z * Z) :=
  match pre with
  | c :: r =>
      if c =? 65 then
        match sleb 5 r with
        | Some (n, cl :: r1) =>
            if cl =? 16 then
              match uleb 5 r1 with
              | Some (a, st :: r2) =>
                  if (a =? alloc) && ((st =? 33) || (st =? 34)) then
                    match uleb 5 r2 with
                    | Some (x, []) => Some (x, n)
                    | _ => None
                    end
                  else None
              | _ => None
              end
            else None
        | _ => None
        end
      else None
  | [] => None
  end.

(* `pre` allocates `n` payload bytes into local `x`, no instruction of `mid`
   assigns `x` and its nesting ends where it began, and the tail's access is
   inside the block. *)
Definition fresh_ok (alloc : Z) (pre mid tail : list Z) : bool :=
  match fresh_pre alloc pre with
  | Some (x, n) =>
      (PAYLOAD + n <? M32) && scan_keep (S (length mid)) x 0 mid && fresh_tail x n tail
  | None => false
  end.

(* What an accepted triple means: `pre` reads as the allocation of `n` bytes
   into `x`, the scan of `mid` held, and every run of the tail that reaches
   its access touches only the block of `12 + n` bytes at the value `x` holds
   when the tail starts. *)
Theorem fresh_sound : forall L0 G memf uo bo alloc pre mid tail,
  fresh_ok alloc pre mid tail = true ->
  exists x n ws k off w,
    fresh_pre alloc pre = Some (x, n)
    /\ scan_keep (S (length mid)) x 0 mid = true
    /\ dec (S (length tail)) tail [] = Some (ws, k, off, w)
    /\ 0 <= n /\ PAYLOAD + n < M32
    /\ forall rest loc stk,
         crun G memf uo bo ws (L0, rest) = Some (loc, stk) ->
         exists av,
           addr_of k stk = Some av
           /\ (L0 x mod M32 + PAYLOAD + n <= M32 ->
               L0 x mod M32 <= av mod M32 + off
               /\ av mod M32 + off + w <= L0 x mod M32 + PAYLOAD + n).
Proof.
  intros L0 G memf uo bo alloc pre mid tail H. unfold fresh_ok in H.
  destruct (fresh_pre alloc pre) as [[x n]|] eqn:Ep; [| discriminate H].
  apply andb_prop in H. destruct H as [H Ht].
  apply andb_prop in H. destruct H as [Hn Hk].
  apply Z.ltb_lt in Hn.
  assert (Ht' := Ht). unfold fresh_tail in Ht'.
  destruct (dec (S (length tail)) tail []) as [[[[ws k] off] w]|] eqn:Ed; [| discriminate Ht'].
  exists x, n, ws, k, off, w.
  split; [first [reflexivity | exact Ep] |].
  split; [exact Hk |].
  split; [first [reflexivity | exact Ed] |].
  assert (Hn0 : 0 <= n).
  { destruct (srun ws (mkS [] [] [])) as [s|]; [| discriminate Ht'].
    destruct (addr_of k (sstk s)) as [a0|]; [| discriminate Ht'].
    destruct (flat a0) as [xs c0].
    destruct xs as [|t1 xs']; [discriminate Ht' |].
    destruct t1; try discriminate Ht'.
    destruct xs'; [| discriminate Ht'].
    repeat (apply andb_prop in Ht'; destruct Ht' as [Ht' ?]).
    match goal with H : (0 <=? n) = true |- _ => apply Z.leb_le in H; exact H end. }
  split; [exact Hn0 | split; [exact Hn |]].
  intros rest loc stk Hrun.
  exact (fresh_tail_sound L0 G memf uo bo x n tail ws k off w rest loc stk Ht Ed Hrun).
Qed.

(* ── a store into a fresh record: `i32.const 16 ; call 6 ; local.set 4`,
   then a value computed with a call in between, then
   `local.get 4 ; local.get 1 ; i64.store offset=20` ── *)
Example fresh_store :
  fresh_ok 6 [65; 16; 16; 6; 33; 4]
             [32; 0; 16; 9; 33; 1]
             [32; 4; 32; 1; 55; 3; 20] = true.
Proof. vm_compute. reflexivity. Qed.

(* one slot too far: offset 28 of a 16-byte payload ends at 36 > 12 + 16 *)
Example refuses_store_past_the_block :
  fresh_ok 6 [65; 16; 16; 6; 33; 4] [32; 0; 16; 9; 33; 1] [32; 4; 32; 1; 55; 3; 28] = false.
Proof. vm_compute. reflexivity. Qed.

(* the local is assigned in between, inside a nested block *)
Example refuses_reassigned_local :
  fresh_ok 6 [65; 16; 16; 6; 33; 4] [2; 64; 32; 0; 33; 4; 11] [32; 4; 32; 1; 55; 3; 20] = false.
Proof. vm_compute. reflexivity. Qed.

(* another function than the allocator was called *)
Example refuses_other_callee :
  fresh_ok 6 [65; 16; 16; 7; 33; 4] [] [32; 4; 32; 1; 55; 3; 20] = false.
Proof. vm_compute. reflexivity. Qed.

(* the run between leaves the block it started in *)
Example refuses_mid_that_closes_a_block :
  fresh_ok 6 [65; 16; 16; 6; 33; 4] [11] [32; 4; 32; 1; 55; 3; 20] = false.
Proof. vm_compute. reflexivity. Qed.

(* the address is another local *)
Example refuses_other_base :
  fresh_ok 6 [65; 16; 16; 6; 33; 4] [] [32; 5; 32; 1; 55; 3; 20] = false.
Proof. vm_compute. reflexivity. Qed.

(* The triple crates/almide-anchor-check/tests/fresh.rs holds the cutter to:
   `i32.const 16 ; call 0 ; local.set 2`, then `local.get 1 ; call 1 ;
   local.set 3`, then `local.get 2 ; local.get 0 ; i64.store offset=20`,
   byte for byte the three lists that test expects the cutter to print. *)
Example cutter_fresh_store :
  fresh_ok 0 [65; 16; 16; 0; 33; 2] [32; 1; 16; 1; 33; 3] [32; 2; 32; 0; 55; 3; 20] = true.
Proof. vm_compute. reflexivity. Qed.

(* The same with the store one slot further: the cutter proposes it (it
   tests no bound) and it is refused here. *)
Example refuses_cutter_store_past_the_block :
  fresh_ok 0 [65; 16; 16; 0; 33; 2] [32; 1; 16; 1; 33; 3] [32; 2; 32; 0; 55; 3; 28] = false.
Proof. vm_compute. reflexivity. Qed.

Print Assumptions fresh_sound.
