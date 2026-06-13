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

Print Assumptions rc_inc_bytes_execute_to_rt_inc.
