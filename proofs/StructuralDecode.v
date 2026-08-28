(* Almide v1 trust spine — #576 third slice: the structural runtime's
   EMITTED BYTES decode to the proven trees.

   Slices 1-2 (StructuralRuntime.v / StructuralAlloc.v) proved the
   transcribed instruction trees realize the modeled discipline, with the
   tree↔bytes binding carried by a Rust-side hash pin. This file upgrades
   that binding for `$inc` / `$dec_flat` / `$free` from trust to THEOREM:
   a symbolic-stack DECODER maps raw wasm body bytes into the SAME
   `expr`/`stmt` language the realization theorems are stated over, and
   the decode of the emitted byte lists is proven — by computation — to
   be EXACTLY `inc_body` / `dec_body` / `free_body 48`.

   Chain: emitted bytes ==(decode, proven here)== the trees
          ==(realization, slices 1-2)== the modeled transitions.

   The byte lists are the code-section bodies the emitter produces
   (dumped by `runtime_alloc.rs::byte_dump::dump_runtime_bytes`, pinned
   by `runtime_trees_match_the_proof_transcription` — the hash pin now
   guards THESE lists' provenance rather than the transcription itself).
   Their constants are all single-byte LEB128: positive immediates < 64
   verbatim, and 124 = SLEB(-4); `48` is FREELIST_BASE (= ITOA_END, the
   layout constant), `3` is G_LINE_END's global index, `33` is `$free`'s
   fixed function index.

   The decoder is CONSERVATIVE: an unknown opcode, an unexpected local
   or global index, a call to anything but `$free` with anything but the
   block argument, a non-singleton `if` body, or a malformed memarg all
   decode to None — never a silent skip. The residual trusted base is
   unchanged from WasmDecode.v: the engine implementing the ISA, and the
   kernel. *)

From AlmideTrust Require Import StructuralRuntime.
From Stdlib Require Import ZArith.
From Stdlib Require Import List.
Import ListNotations.
Open Scope Z_scope.

(* A local-index naming: which reader expression and (for set) which
   binder statement an index means in the function under decode. *)
Record naming := mkNaming {
  n_get : Z -> option expr;
  n_set : Z -> option (expr -> stmt)
}.

(* Single-byte SLEB128 constant: 0..63 verbatim, 64..127 = negative. *)
Definition sleb1 (b : Z) : Z := if b <? 64 then b else b - 128.

(* Fold a memarg offset the way the transcription spells addresses:
   offset 0 is the bare base, a positive offset is `base + off`. *)
Definition fold_off (a : expr) (off : Z) : expr :=
  if off =? 0 then a else EAdd a (EC off).

(* The symbolic-stack decoder. One pass over the byte list with an
   expression stack; statement-producing opcodes drain it. `if` bodies
   recurse (fuel-bounded, as WasmDecode.v) and must be a SINGLE
   statement — every `if` in these trees is. Returns the decoded
   statements and the bytes after this body's `end`. *)
Fixpoint decode_go (fuel : nat) (nm : naming) (bs : list Z)
                  (stk : list expr) (acc : list stmt)
  : option (list stmt * list Z) :=
  match fuel with
  | O => None
  | S f =>
      match bs with
      | 11 :: r => (* end — the stack must be drained *)
          match stk with
          | [] => Some (rev acc, r)
          | _ => None
          end
      | 15 :: r => (* return *)
          decode_go f nm r stk (SRet :: acc)
      | 32 :: i :: r => (* local.get *)
          match n_get nm i with
          | Some e => decode_go f nm r (e :: stk) acc
          | None => None
          end
      | 33 :: i :: r => (* local.set *)
          match n_set nm i, stk with
          | Some mk, v :: stk' => decode_go f nm r stk' (mk v :: acc)
          | _, _ => None
          end
      | 35 :: 3 :: r => (* global.get $line_end (index 3) *)
          decode_go f nm r (EFloor :: stk) acc
      | 40 :: 2 :: off :: r => (* i32.load align=2 *)
          match stk with
          | a :: stk' => decode_go f nm r (ELoad (fold_off a off) :: stk') acc
          | [] => None
          end
      | 54 :: 2 :: off :: r => (* i32.store align=2 *)
          match stk with
          | v :: a :: stk' =>
              decode_go f nm r stk' (SStore (fold_off a off) v :: acc)
          | _ => None
          end
      | 65 :: b :: r => (* i32.const, single-byte SLEB *)
          decode_go f nm r (EC (sleb1 b) :: stk) acc
      | 69 :: r => (* i32.eqz *)
          match stk with
          | a :: stk' => decode_go f nm r (EEqz a :: stk') acc
          | [] => None
          end
      | 73 :: r => (* i32.lt_u *)
          match stk with
          | b :: a :: stk' => decode_go f nm r (ELtU a b :: stk') acc
          | _ => None
          end
      | 79 :: r => (* i32.ge_u *)
          match stk with
          | b :: a :: stk' => decode_go f nm r (EGeU a b :: stk') acc
          | _ => None
          end
      | 103 :: r => (* i32.clz *)
          match stk with
          | a :: stk' => decode_go f nm r (EClz a :: stk') acc
          | [] => None
          end
      | 106 :: r => (* i32.add *)
          match stk with
          | b :: a :: stk' => decode_go f nm r (EAdd a b :: stk') acc
          | _ => None
          end
      | 107 :: r => (* i32.sub *)
          match stk with
          | b :: a :: stk' => decode_go f nm r (ESub a b :: stk') acc
          | _ => None
          end
      | 113 :: r => (* i32.and *)
          match stk with
          | b :: a :: stk' => decode_go f nm r (ELand a b :: stk') acc
          | _ => None
          end
      | 116 :: r => (* i32.shl *)
          match stk with
          | b :: a :: stk' => decode_go f nm r (EShl a b :: stk') acc
          | _ => None
          end
      | 16 :: 33 :: r => (* call $free (fixed index 33): the block arg *)
          match stk with
          | EBlk :: stk' => decode_go f nm r stk' (SCallFree :: acc)
          | _ => None
          end
      | 4 :: 64 :: r => (* if, empty blocktype — body must be ONE stmt *)
          match stk with
          | cnd :: stk' =>
              match decode_go f nm r [] [] with
              | Some ([s], rest) =>
                  decode_go f nm rest stk' (SIf cnd s :: acc)
              | _ => None
              end
          | [] => None
          end
      | _ => None
      end
  end.

(* Decode one full function body (empty starting stack, drained end). *)
Definition decode (nm : naming) (bs : list Z) : option (list stmt) :=
  match decode_go 1000 nm bs [] [] with
  | Some (ss, []) => Some ss
  | _ => None
  end.

(* ── The namings, per function (the emitter's local layouts) ── *)

Definition inc_naming : naming :=
  mkNaming (fun i => if i =? 0 then Some EBlk else None)
           (fun _ => None).

Definition dec_naming : naming :=
  mkNaming (fun i => if i =? 0 then Some EBlk
                     else if i =? 1 then Some ETmp else None)
           (fun i => if i =? 1 then Some SSetTmp else None).

Definition free_naming : naming :=
  mkNaming (fun i => if i =? 0 then Some EBlk
                     else if i =? 1 then Some ETot
                     else if i =? 2 then Some ECls else None)
           (fun i => if i =? 1 then Some SSetTot
                     else if i =? 2 then Some SSetCls else None).

(* ── The emitted body bytes (code-section bodies, locals vector
      stripped; dumped by `dump_runtime_bytes`, provenance held by the
      hash pin) ── *)

Definition inc_bytes : list Z :=
  [32;0;35;3;73;4;64;15;11;
   32;0;32;0;40;2;0;65;1;106;54;2;0;
   11].

Definition dec_bytes : list Z :=
  [32;0;35;3;73;4;64;15;11;
   32;0;40;2;0;65;1;107;33;1;
   32;0;32;1;54;2;0;
   32;1;69;4;64;32;0;16;33;11;
   11].

Definition free_bytes : list Z :=
  [32;0;40;2;8;65;15;106;65;124;113;33;1;
   32;1;65;16;73;4;64;15;11;
   65;28;32;1;65;1;107;103;107;33;2;
   32;2;65;16;79;4;64;15;11;
   32;2;65;2;116;65;48;106;33;2;
   32;0;32;2;40;2;0;54;2;12;
   32;2;32;0;54;2;0;
   11].

(* ══ THE DECODE THEOREMS — by computation ══════════════════════════════

   The bytes decode to EXACTLY the trees slices 1-2 proved about. `48`
   is FREELIST_BASE (= ITOA_END). *)

Theorem inc_bytes_decode_to_the_tree :
  decode inc_naming inc_bytes = Some inc_body.
Proof. reflexivity. Qed.

Theorem dec_bytes_decode_to_the_tree :
  decode dec_naming dec_bytes = Some dec_body.
Proof. reflexivity. Qed.

Theorem free_bytes_decode_to_the_tree :
  decode free_naming free_bytes = Some (free_body 48).
Proof. reflexivity. Qed.
