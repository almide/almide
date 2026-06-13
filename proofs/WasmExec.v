(* Almide v1 trust spine — A2 byte EXECUTION: the real rc_inc BYTES, run by a wasm
   stack machine, compute rt_inc.

   WasmEncode proved the rc_inc instruction tree ENCODES to the real wasm bytes
   (grounded against wat2wasm). This file closes the other half: a minimal wasm
   STACK MACHINE EXECUTES those exact bytes and the memory effect is precisely
   `RuntimeModel.rt_inc`. So the chain now reaches all the way down:

     real wasm BYTES --(this interpreter)--> rt_inc --(RuntimeModel)--> abstract rc.

   Combined with WasmEncode (instruction tree -> these bytes) and WasmRcDec
   (instruction tree realizes rt_inc), the rc_inc primitive is bound end to end:
   the instruction tree, the real bytes, and the execution all agree on rt_inc.

   Scope (honest): the interpreter covers the straight-line opcode subset rc_inc
   uses (local.get / i32.const / i32.add / i32.load / i32.store / end) over a Z
   operand stack and `RuntimeModel.Mem`. The residual trust shrinks from "a wasm
   engine runs the bytes correctly" to "this small, INSPECTABLE interpreter matches
   the wasm spec for these opcodes" (the full ISA / control flow is WasmCert-Coq).
   Local 0 = the `$p` parameter; memarg offset is honoured (0 here). *)

From AlmideTrust Require Import RuntimeModel WasmEncode.
From Stdlib Require Import ZArith List.
Import ListNotations.
Open Scope Z_scope.

(* A minimal wasm stack machine over a byte list. `p` = the value of local 0
   ($p). The Z operand stack grows leftward. Returns the final memory, or None on
   a malformed / out-of-subset stream. Structurally recursive on the byte list. *)
Fixpoint run (bytes : list Z) (p : Z) (st : list Z) (m : Mem) : option Mem :=
  match bytes with
  | [] => None
  | op :: rest =>
      if Z.eqb op 11 then Some m                                  (* 0x0b end *)
      else if Z.eqb op 32 then                                    (* 0x20 local.get idx *)
        match rest with _idx :: r => run r p (p :: st) m | _ => None end
      else if Z.eqb op 65 then                                    (* 0x41 i32.const v *)
        match rest with v :: r => run r p (v :: st) m | _ => None end
      else if Z.eqb op 106 then                                   (* 0x6a i32.add *)
        match st with b :: a :: s => run rest p ((a + b) :: s) m | _ => None end
      else if Z.eqb op 40 then                                    (* 0x28 i32.load align off *)
        match rest, st with
        | _al :: off :: r, addr :: s => run r p (m (addr + off) :: s) m
        | _, _ => None end
      else if Z.eqb op 54 then                                    (* 0x36 i32.store align off *)
        match rest, st with
        | _al :: off :: r, v :: addr :: s => run r p s (upd m (addr + off) v)
        | _, _ => None end
      else if Z.eqb op 69 then                                    (* 0x45 i32.eqz *)
        match st with a :: s => run rest p ((if Z.eqb a 0 then 1 else 0) :: s) m | _ => None end
      else if Z.eqb op 4 then  (* 0x04 if — the double-free TRAP pattern only:
                                  `if (void) (then unreachable) end`. cond<>0 runs
                                  the unreachable (None); cond=0 skips to after end. *)
        match rest, st with
        | 64 :: 0 :: 11 :: r, cond :: s =>
            if Z.eqb cond 0 then run r p s m else None
        | _, _ => None
        end
      else None
  end.

(* THE A2 EXECUTION BINDING: running the REAL rc_inc bytes on the stack machine
   yields exactly `RuntimeModel.rt_inc` — the emitted bytes COMPUTE the abstract
   acquire, not merely encode an instruction that would. *)
Theorem rc_inc_bytes_execute_to_rt_inc :
  forall p m, run rc_inc_bytes p [] m = Some (rt_inc m p).
Proof.
  intros p m. unfold rt_inc, read_rc, RC_OFFSET.
  cbn -[Z.add]. rewrite !Z.add_0_r. reflexivity.
Qed.

(* Non-vacuous: a cell holding 4 is left holding 5 by executing the real bytes. *)
Example rc_inc_bytes_increment_a_four :
  forall m, m (0 + RC_OFFSET) = 4 ->
    match run rc_inc_bytes 0 [] m with
    | Some m' => read_rc m' 0 = 5
    | None => False
    end.
Proof.
  intros m H. rewrite rc_inc_bytes_execute_to_rt_inc.
  unfold rt_inc. rewrite read_upd_same. unfold read_rc. rewrite H. reflexivity.
Qed.

(* ─── the DOUBLE-FREE TRAP, on real bytes (control flow) ───
   The bytes for `(i32.eqz (i32.load (local.get $p)))  (if (then unreachable))`:
   load the cell, and TRAP iff it is 0. (Its opcodes 0x45/0x04/0x40/0x00/0x0b are
   grounded by check-wasm-bytes.sh against wat2wasm's rc_dec disassembly.) This
   shows the byte interpreter executes the safety-critical double-free TRAP, not
   only straight-line code. *)
Definition trap_if_zero_bytes : list Z :=
  [32;0;  40;2;0;  69;  4;64;0;11;  11].

(* TRAP direction: a cell holding 0 (an already-freed block) traps — the
   double-free sentinel, executed on the real bytes. *)
Theorem trap_bytes_trap_on_zero :
  forall p m, m (p + 0) = 0 -> run trap_if_zero_bytes p [] m = None.
Proof.
  intros p m H. unfold trap_if_zero_bytes. cbn -[Z.add]. rewrite H. reflexivity.
Qed.

(* NO-TRAP direction: a live cell (nonzero) does NOT trap — the sentinel fires
   only on an already-freed cell, never on a valid release. *)
Theorem trap_bytes_pass_on_nonzero :
  forall p m, m (p + 0) <> 0 -> run trap_if_zero_bytes p [] m = Some m.
Proof.
  intros p m H. unfold trap_if_zero_bytes. cbn -[Z.add].
  apply Z.eqb_neq in H. rewrite H. reflexivity.
Qed.

(* ─── structure finding (the foundation for GENERAL control flow) ───
   To execute a general `if … end`, the interpreter must find the MATCHING end —
   which a naive byte scan gets WRONG because an immediate can collide with an
   opcode byte (e.g. `i32.const 4` = `41 04`, and 0x04 is the `if` opcode; or
   `i32.const 11` = `41 0b`, and 0x0b is `end`). The fix is small: a per-opcode
   IMMEDIATE-LENGTH table, so the scanner skips each instruction's immediates
   rather than misreading them. (This is NOT a full WasmCert-Coq ISA — just an
   immediate-length table for the dozen opcodes the renderer emits.) *)
Definition imm_len (op : Z) : nat :=
  if Z.eqb op 32 then 1        (* 0x20 local.get idx *)
  else if Z.eqb op 33 then 1   (* 0x21 local.set idx *)
  else if Z.eqb op 65 then 1   (* 0x41 i32.const v *)
  else if Z.eqb op 35 then 1   (* 0x23 global.get idx *)
  else if Z.eqb op 36 then 1   (* 0x24 global.set idx *)
  else if Z.eqb op 40 then 2   (* 0x28 i32.load memarg *)
  else if Z.eqb op 54 then 2   (* 0x36 i32.store memarg *)
  else 0.                      (* add/sub/eqz/unreachable: no immediate *)

Definition take_imm (n : nat) (l : list Z) : option (list Z) :=
  if Nat.leb n (length l) then Some (skipn n l) else None.

(* Find the matching `end` of the current block, returning the bytes AFTER it.
   Skips each instruction's immediates (so const-4 / const-11 do not fool it);
   tracks block depth on nested `if`/`end`. Fuel-bounded by the byte length. *)
Fixpoint skip_block (fuel : nat) (depth : nat) (bytes : list Z) : option (list Z) :=
  match fuel with
  | O => None
  | S f =>
      match bytes with
      | [] => None
      | op :: rest =>
          if Z.eqb op 11 then                    (* end *)
            match depth with O => Some rest | S d => skip_block f d rest end
          else if Z.eqb op 4 then                (* if: +1 depth, skip blocktype *)
            match rest with _bt :: r => skip_block f (S depth) r | _ => None end
          else
            match take_imm (imm_len op) rest with
            | Some r => skip_block f depth r
            | None => None
            end
      end
  end.

(* THE COLLISION-RESISTANCE FACT: the body `i32.const 4 ; i32.const 11` contains
   the bytes 0x04 (`if`) and 0x0b (`end`) AS IMMEDIATES — a naive scan would stop
   at the 0x0b. `skip_block` correctly skips both and finds the REAL matching end,
   returning the tail. This is exactly the case I had wrongly called "needs a full
   parser": it needs only the immediate-length table above. *)
Example skip_block_not_fooled_by_immediates :
  skip_block 10 0 [65;4; 65;11; 11; 99] = Some [99].
Proof. reflexivity. Qed.

(* And a NESTED if inside the block is matched at the right depth. *)
Example skip_block_handles_nesting :
  skip_block 10 0 [4;64; 0; 11; 11; 99] = Some [99].
Proof. reflexivity. Qed.

(* `split_block` returns BOTH the block BODY (before the matching end) and the
   bytes AFTER it — what a general `if` executor needs (run the body, then go on). *)
Fixpoint split_block (fuel depth : nat) (bytes acc : list Z) : option (list Z * list Z) :=
  match fuel with
  | O => None
  | S f =>
      match bytes with
      | [] => None
      | op :: rest =>
          if Z.eqb op 11 then
            match depth with
            | O => Some (rev acc, rest)
            | S d => split_block f d rest (op :: acc)
            end
          else if Z.eqb op 4 then
            match rest with bt :: r => split_block f (S depth) r (bt :: op :: acc) | _ => None end
          else
            let n := imm_len op in
            if Nat.leb n (length rest)
            then split_block f depth (skipn n rest) (rev (firstn n rest) ++ op :: acc)
            else None
      end
  end.

(* GENERAL wasm interpreter (fuel-bounded): straight-line ops PLUS general
   structured `if … end` — the then-body runs when the condition is nonzero and
   is SKIPPED (via split_block) otherwise. Generalizes `run`'s fixed trap pattern
   to ANY void block body. `[]` / `end` = block complete. (Void blocks here are
   stack-neutral, so the post-body stack is the pre-body stack.) *)
Fixpoint run_g (fuel : nat) (bytes : list Z) (p : Z) (st : list Z) (m : Mem) : option Mem :=
  match fuel with
  | O => None
  | S f =>
      match bytes with
      | [] => Some m
      | op :: rest =>
          if Z.eqb op 11 then Some m
          else if Z.eqb op 32 then
            match rest with _i :: r => run_g f r p (p :: st) m | _ => None end
          else if Z.eqb op 65 then
            match rest with v :: r => run_g f r p (v :: st) m | _ => None end
          else if Z.eqb op 106 then
            match st with b :: a :: s => run_g f rest p ((a + b) :: s) m | _ => None end
          else if Z.eqb op 40 then
            match rest, st with
            | _al :: off :: r, addr :: s => run_g f r p (m (addr + off) :: s) m
            | _, _ => None end
          else if Z.eqb op 54 then
            match rest, st with
            | _al :: off :: r, v :: addr :: s => run_g f r p s (upd m (addr + off) v)
            | _, _ => None end
          else if Z.eqb op 69 then
            match st with a :: s => run_g f rest p ((if Z.eqb a 0 then 1 else 0) :: s) m | _ => None end
          else if Z.eqb op 4 then          (* GENERAL if *)
            match rest with
            | _bt :: r =>
                match split_block (length r) 0 r [] with
                | Some (body, after) =>
                    match st with
                    | cond :: s =>
                        if Z.eqb cond 0 then run_g f after p s m
                        else match run_g f body p s m with
                             | Some m' => run_g f after p s m'
                             | None => None end
                    | _ => None end
                | None => None end
            | _ => None end
          else None
      end
  end.

(* The GENERAL structured-if EXECUTOR runs a non-trivial then-body. `if (cond)
   (then (i32.store 0 := 42))`: when cond is nonzero the store HAPPENS (cell 0 =
   42); when cond is 0 the body is SKIPPED (memory unchanged). This is beyond the
   fixed trap pattern — the body executes, found via the immediate-aware splitter. *)
Definition cond_store_bytes (c : Z) : list Z :=
  [65;c;  4;64;  65;0; 65;42; 54;2;0;  11;  11].

Theorem general_if_runs_body_when_true :
  forall p m, run_g 50 (cond_store_bytes 1) p [] m = Some (upd m 0 42).
Proof. intros p m. reflexivity. Qed.

Theorem general_if_skips_body_when_false :
  forall p m, run_g 50 (cond_store_bytes 0) p [] m = Some m.
Proof. intros p m. reflexivity. Qed.

Print Assumptions rc_inc_bytes_execute_to_rt_inc.
Print Assumptions trap_bytes_trap_on_zero.
Print Assumptions trap_bytes_pass_on_nonzero.
Print Assumptions general_if_runs_body_when_true.
Print Assumptions general_if_skips_body_when_false.
