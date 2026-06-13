(* Almide v1 trust spine — A2 byte slice: the rc_inc INSTRUCTION TREE encodes to
   the REAL wasm bytes.

   WasmRcDec.v proved the rc_inc instruction tree (`rc_inc_prog`) computes
   `rt_inc`. The remaining "model <-> real wasm BYTES" gap (A2) is the encoding:
   do the bytes the renderer's `$rc_inc` actually assembles to correspond to that
   instruction tree? This file models the wasm BINARY encoder for the opcode
   subset the rc primitives use, and proves `encode_body rc_inc_prog = rc_inc_bytes`
   — the exact byte sequence the real assembler (wat2wasm) produces.

   GROUNDING (the anti-circularity): `rc_inc_bytes` is not a guess — it is the
   byte sequence `wat2wasm` emits for `$rc_inc`, re-verified per build by
   `proofs/check-wasm-bytes.sh` (assemble, extract the code, compare). So the
   opcode constants below are the REAL wasm opcodes, not my memory of them. Chain:
   real emitted bytes  ==(gate)==  `rc_inc_bytes`  ==(this proof)==  encode of
   `rc_inc_prog`  ==(WasmRcDec)==  realizes `rt_inc`. So the model's `rt_inc` is
   what the emitted BYTES encode.

   HONEST scope: this is the ENCODING half — it binds the instruction tree to the
   bytes the assembler produces, grounded in `wat2wasm`. It does NOT prove a wasm
   ENGINE executes those bytes per the spec (that is a verified interpreter /
   WasmCert-Coq — the residual trusted piece, like the kernel and hardware). It
   covers `rc_inc` (where the WasmRcDec model and the renderer's emitted code
   coincide exactly); `rc_dec`'s byte-binding needs the model aligned with the
   renderer's `local.set` intermediate (a follow-up). The immediates here are in
   0..63, so single-byte LEB128 = the value — the fragment the rc primitives use;
   the gate validates every opcode constant against the assembler. *)

From AlmideTrust Require Import WasmRcDec.
From Stdlib Require Import ZArith List.
Import ListNotations.
Open Scope Z_scope.

(* The REAL wasm binary opcodes (grounded against wat2wasm by check-wasm-bytes.sh). *)
Definition OP_UNREACHABLE : Z := 0.    (* 0x00 *)
Definition OP_IF          : Z := 4.    (* 0x04 *)
Definition OP_END         : Z := 11.   (* 0x0b *)
Definition OP_LOCAL_GET   : Z := 32.   (* 0x20 *)
Definition OP_LOCAL_SET   : Z := 33.   (* 0x21 *)
Definition OP_I32_LOAD    : Z := 40.   (* 0x28 *)
Definition OP_I32_STORE   : Z := 54.   (* 0x36 *)
Definition OP_I32_CONST   : Z := 65.   (* 0x41 *)
Definition OP_I32_EQZ     : Z := 69.   (* 0x45 *)
Definition OP_I32_ADD     : Z := 106.  (* 0x6a *)
Definition OP_I32_SUB     : Z := 107.  (* 0x6b *)
Definition BLOCKTYPE_VOID : Z := 64.   (* 0x40 *)
Definition MEMARG_ALIGN   : Z := 2.    (* align=2 (natural 4-byte i32 alignment) *)
Definition MEMARG_OFFSET  : Z := 0.

(* Local indices in the rc primitives: Ptr = $p = local 0, Tmp = $rc = local 1. *)
Definition LOCAL_PTR : Z := 0.
Definition LOCAL_TMP : Z := 1.

(* Encode an expr to wasm bytes (the stack-machine order). Immediates are in
   0..63 here, so a single byte = the value (single-byte LEB128). *)
Fixpoint encode_expr (e : expr) : list Z :=
  match e with
  | Const z => [OP_I32_CONST; z]
  | Ptr      => [OP_LOCAL_GET; LOCAL_PTR]
  | Tmp      => [OP_LOCAL_GET; LOCAL_TMP]
  | WasmRcDec.Add a b => encode_expr a ++ encode_expr b ++ [OP_I32_ADD]
  | WasmRcDec.Sub a b => encode_expr a ++ encode_expr b ++ [OP_I32_SUB]
  | Load a   => encode_expr a ++ [OP_I32_LOAD; MEMARG_ALIGN; MEMARG_OFFSET]
  end.

Definition encode_stmt (s : stmt) : list Z :=
  match s with
  | SetTmp e      => encode_expr e ++ [OP_LOCAL_SET; LOCAL_TMP]
  | TrapIfZero e  => encode_expr e ++ [OP_I32_EQZ; OP_IF; BLOCKTYPE_VOID; OP_UNREACHABLE; OP_END]
  | Store a v     => encode_expr a ++ encode_expr v ++ [OP_I32_STORE; MEMARG_ALIGN; MEMARG_OFFSET]
  end.

Definition encode_body (ss : list stmt) : list Z := flat_map encode_stmt ss ++ [OP_END].

(* The bytes wat2wasm produces for the `$rc_inc` function body — re-verified per
   build by proofs/check-wasm-bytes.sh (assemble $rc_inc, extract the code section,
   compare to this list). This is the GROUNDING that makes the theorem non-circular. *)
Definition rc_inc_bytes : list Z :=
  [32;0; 65;0; 106;
   32;0; 65;0; 106; 40;2;0;
   65;1; 106;
   54;2;0;
   11].

(* THE A2 BYTE BINDING for rc_inc: our encoder produces EXACTLY the assembler's
   bytes for WasmRcDec.rc_inc_prog. Composed with rc_inc_prog_realizes_rt_inc, the
   real emitted bytes ARE the encoding of an instruction tree that computes rt_inc. *)
Theorem rc_inc_bytes_encode_the_instruction_tree :
  encode_body rc_inc_prog = rc_inc_bytes.
Proof. reflexivity. Qed.

(* AXIOM AUDIT. *)
Print Assumptions rc_inc_bytes_encode_the_instruction_tree.
