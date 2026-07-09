(* Almide v1 trust spine — 3c: the raw-byte ⟶ ISA binding (the "last mile" of G1.2,
   in the in-tree WasmCert-Coq style).

   WasmEncode.v binds the rc instruction TREE to the assembler's bytes (grounded in
   wat2wasm by check-wasm-bytes.sh). WasmIsa.v gives the opcode subset a REAL
   relational small-step semantics (`istep`/`irun`) with a verified interpreter
   (`erun`, sound + complete). What was missing is the direction the CHECKER needs:
   from the RAW BYTES to the ISA — so the trap/reclamation theorems hold OF THE
   BYTES, not of a hand-transcribed AST.

   This file provides the proven DECODER `decode : list Z -> option (list instr)`
   for the rc opcode subset and proves, by computation:

     decode rc_inc_bytes     = Some …   (the REAL renderer `$rc_inc` bytes,
                                          wat2wasm-grounded in WasmEncode.v)
     decode rc_dec_isa_bytes = Some rc_dec_prog

   and then carries the ISA theorems onto the bytes themselves:

     - the decoded `$rc_inc` bytes' EVERY ISA reduction adds exactly +1 to the cell;
     - the `$rc_dec` bytes' ISA reduction of an already-0 cell CANNOT complete
       (double-free is stuck in the spec, at the byte level);
     - the `$rc_dec` bytes' every reduction of a uniquely-owned cell zeroes it AND
       links the block onto $freelist (leak-freedom reclamation, at the byte level).

   Chain closed: real bytes ==(decode, proven)== ISA program ==(irun, proven)==
   the memory effect. The residual trusted base is the wasm ENGINE implementing the
   ISA spec (the same residual WasmCert-Coq itself has), the assembler grounding
   (wat2wasm, re-checked per build), and the kernel. *)

From AlmideTrust Require Import WasmEncode.
From AlmideTrust Require Import WasmIsa.
From Stdlib Require Import ZArith List.
Import ListNotations.
Open Scope Z_scope.

(* ─── the decoder ───
   Structurally recursive on FUEL (byte count bounds it: each step consumes ≥ 1
   byte); a body is decoded until its terminating `end` (0x0b). Opcode subset =
   exactly WasmIsa's instr alphabet; immediates are single-byte LEB128 (0..63 —
   the rc fragment; the wat2wasm grounding validates this per build). An unknown
   opcode, a malformed memarg/blocktype, a global index ≠ 0, or missing `end` all
   decode to `None` — conservative reject, never a silent skip. *)

Fixpoint decode_body (fuel : nat) (bs : list Z) : option (list instr * list Z) :=
  match fuel with
  | O => None
  | S f =>
      match bs with
      | 11 :: rest => Some ([], rest)                        (* end — closes this body *)
      | 0 :: rest =>
          match decode_body f rest with
          | Some (is_, r) => Some (IUnreachable :: is_, r)
          | None => None
          end
      | 32 :: i :: rest =>
          match decode_body f rest with
          | Some (is_, r) => Some (ILocalGet i :: is_, r)
          | None => None
          end
      | 33 :: i :: rest =>
          match decode_body f rest with
          | Some (is_, r) => Some (ILocalSet i :: is_, r)
          | None => None
          end
      | 35 :: 0 :: rest =>                                   (* global.get 0 — the single $freelist global *)
          match decode_body f rest with
          | Some (is_, r) => Some (IGlobalGet :: is_, r)
          | None => None
          end
      | 36 :: 0 :: rest =>                                   (* global.set 0 *)
          match decode_body f rest with
          | Some (is_, r) => Some (IGlobalSet :: is_, r)
          | None => None
          end
      | 40 :: 2 :: 0 :: rest =>                              (* i32.load align=2 offset=0 *)
          match decode_body f rest with
          | Some (is_, r) => Some (ILoad :: is_, r)
          | None => None
          end
      | 54 :: 2 :: 0 :: rest =>                              (* i32.store align=2 offset=0 *)
          match decode_body f rest with
          | Some (is_, r) => Some (IStore :: is_, r)
          | None => None
          end
      | 65 :: z :: rest =>
          match decode_body f rest with
          | Some (is_, r) => Some (IConst z :: is_, r)
          | None => None
          end
      | 69 :: rest =>
          match decode_body f rest with
          | Some (is_, r) => Some (IEqz :: is_, r)
          | None => None
          end
      | 106 :: rest =>
          match decode_body f rest with
          | Some (is_, r) => Some (IAdd :: is_, r)
          | None => None
          end
      | 107 :: rest =>
          match decode_body f rest with
          | Some (is_, r) => Some (ISub :: is_, r)
          | None => None
          end
      | 4 :: 64 :: rest =>                                   (* if (blocktype void) … end *)
          match decode_body f rest with
          | Some (body, r1) =>
              match decode_body f r1 with
              | Some (is_, r) => Some (IIf body :: is_, r)
              | None => None
              end
          | None => None
          end
      | _ => None
      end
  end.

(* A function BODY: decode to the final `end`, which must consume ALL bytes. *)
Definition decode (bs : list Z) : option (list instr) :=
  match decode_body (List.length bs) bs with
  | Some (is_, []) => Some is_
  | _ => None
  end.

(* ─── $rc_inc: the REAL renderer bytes (wat2wasm-grounded, WasmEncode.v) decode,
   and their ISA execution has the rt_inc effect ─── *)

(* The decoded form of the REAL `$rc_inc` body — the renderer's shape (with its
   `p + 0` address adds), NOT the simplified rc_inc_prog: the bytes are the
   authority, the AST follows them. *)
Definition rc_inc_real_prog : list instr :=
  [ILocalGet 0; IConst 0; IAdd;
   ILocalGet 0; IConst 0; IAdd; ILoad;
   IConst 1; IAdd;
   IStore].

Theorem rc_inc_bytes_decode :
  decode rc_inc_bytes = Some rc_inc_real_prog.
Proof. reflexivity. Qed.

(* The rt_inc effect, carried onto the BYTES: every ISA reduction of the decoded
   real `$rc_inc` bytes over a cell holding `m p` leaves it holding `m p + 1`.
   (erun computes one reduction; isa determinism makes it THE reduction.) *)
Theorem rc_inc_bytes_isa_effect : forall p m prog c',
  decode rc_inc_bytes = Some prog ->
  irun prog (init p m) c' -> mem c' p = m p + 1.
Proof.
  intros p m prog c' Hd H.
  rewrite rc_inc_bytes_decode in Hd. injection Hd as <-.
  destruct (erun 14 rc_inc_real_prog (init p m)) as [c2|] eqn:E; [|cbn in E; discriminate].
  pose proof (irun_det _ _ _ H _ (erun_sound _ _ _ _ E)) as ->.
  clear - E. cbn in E. injection E as <-. cbn.
  rewrite Z.add_0_r. apply WasmIsa.upd_same.
Qed.

(* ─── $rc_dec: the REAL full release bytes (trap-on-zero, decrement, store,
   free-list reclaim-on-zero — the renderer's shape, with its `p + 0` address
   adds), EXACTLY the sequence check-wasm-bytes.sh re-assembles with wat2wasm
   every build (its `RC_DEC_BODY` grounding row). ─── *)

Definition rc_dec_real_bytes : list Z :=
  [32;0; 65;0; 106; 40;2;0; 33;1;         (* rc := load(p + 0) *)
   32;1; 69; 4;64; 0; 11;                 (* if rc==0 then unreachable end *)
   32;1; 65;1; 107; 33;1;                 (* rc := rc - 1 *)
   32;0; 65;0; 106; 32;1; 54;2;0;         (* store(p + 0, rc) *)
   32;1; 69;                              (* rc == 0 (now)? *)
   4;64; 32;0; 65;4; 106; 35;0; 54;2;0;   (* then: store(p+4, $freelist) *)
   32;0; 36;0; 11;                        (*       $freelist := p; end *)
   11].                                   (* function end *)

(* Its decoded form — the bytes are the authority, the AST follows them. *)
Definition rc_dec_real_prog : list instr :=
  [ILocalGet 0; IConst 0; IAdd; ILoad; ILocalSet 1;
   ILocalGet 1; IEqz; IIf [IUnreachable];
   ILocalGet 1; IConst 1; ISub; ILocalSet 1;
   ILocalGet 0; IConst 0; IAdd; ILocalGet 1; IStore;
   ILocalGet 1; IEqz;
   IIf [ILocalGet 0; IConst 4; IAdd; IGlobalGet; IStore; ILocalGet 0; IGlobalSet]].

Theorem rc_dec_bytes_decode :
  decode rc_dec_real_bytes = Some rc_dec_real_prog.
Proof. reflexivity. Qed.

(* DOUBLE-FREE IS STUCK, AT THE BYTE LEVEL: the decoded real `$rc_dec` bytes over
   an already-0 cell cannot run to completion at ANY fuel — so by completeness no
   ISA reduction completes: the double-free sentinel fires in the spec itself. *)
Lemma rc_dec_real_traps_on_zero : forall fuel g0,
  erun fuel rc_dec_real_prog (init_dec 0 g0) = None.
Proof.
  intros fuel g0.
  do 11 (destruct fuel as [|fuel]; [reflexivity|]). cbn. reflexivity.
Qed.

Theorem rc_dec_bytes_isa_traps : forall g0 prog c',
  decode rc_dec_real_bytes = Some prog ->
  ~ irun prog (init_dec 0 g0) c'.
Proof.
  intros g0 prog c' Hd H.
  rewrite rc_dec_bytes_decode in Hd. injection Hd as <-.
  apply erun_complete in H. destruct H as [n Hn].
  rewrite rc_dec_real_traps_on_zero in Hn. discriminate.
Qed.

(* LEAK-FREEDOM RECLAMATION, AT THE BYTE LEVEL: every ISA reduction of the decoded
   real `$rc_dec` bytes over a uniquely-owned cell (rc = 1) zeroes it AND links the
   block onto $freelist. *)
Theorem rc_dec_bytes_isa_frees : forall g0 prog c',
  decode rc_dec_real_bytes = Some prog ->
  irun prog (init_dec 1 g0) c' -> mem c' 16 = 0 /\ glob c' = 16.
Proof.
  intros g0 prog c' Hd H.
  rewrite rc_dec_bytes_decode in Hd. injection Hd as <-.
  destruct (erun 40 rc_dec_real_prog (init_dec 1 g0)) as [c2|] eqn:E; [|cbn in E; discriminate].
  pose proof (irun_det _ _ _ H _ (erun_sound _ _ _ _ E)) as ->.
  clear - E. cbn in E. injection E as <-. cbn. split; reflexivity.
Qed.

(* Non-vacuity of the conservative reject: junk bytes, a truncated body (missing
   `end`), and a non-zero global index all decode to None. *)
Example decode_junk_rejects : decode [200; 1; 2] = None.
Proof. reflexivity. Qed.
Example decode_truncated_rejects : decode [32;0; 40;2;0] = None.
Proof. reflexivity. Qed.
Example decode_bad_global_rejects : decode [35;1; 11] = None.
Proof. reflexivity. Qed.

(* AXIOM AUDIT. *)
Print Assumptions rc_inc_bytes_decode.
Print Assumptions rc_inc_bytes_isa_effect.
Print Assumptions rc_dec_bytes_decode.
Print Assumptions rc_dec_bytes_isa_traps.
Print Assumptions rc_dec_bytes_isa_frees.
