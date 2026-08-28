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

(* ══════════════════════════════════════════════════════════════════════
   SLICE 4: `$alloc`'s bytes decode to StructuralAlloc's tree.

   New ground relative to the first decoder: `local.tee` (desugared to a
   binder statement plus the local's reader on the stack — exactly how
   the transcription spells it), the second global (`G_HEAP`, index 1)
   read AND written, `memory.size`, the value-carrying `return` and the
   value-carrying implicit fall-through at `end`, multi-byte SLEB128
   (524288 = [128;128;32]), and the GROW ARM: its guard decodes
   structurally, its BODY is matched VERBATIM against the emitted span
   (parameterized by the per-program OOM-message immediate) and
   abstracted to `SGrow` — the same honesty boundary slice 2 declared
   for its semantics. *)

From AlmideTrust Require Import StructuralAlloc.

(* Multi-byte SLEB128 (up to 3 bytes — the largest immediate here). *)
Definition sleb_dec (bs : list Z) : option (Z * list Z) :=
  match bs with
  | b0 :: r0 =>
      if b0 <? 128 then Some (sleb1 b0, r0)
      else match r0 with
           | b1 :: r1 =>
               if b1 <? 128
               then Some ((b0 - 128) + Z.shiftl (sleb1 b1) 7, r1)
               else match r1 with
                    | b2 :: r2 =>
                        if b2 <? 128
                        then Some ((b0 - 128) + Z.shiftl (b1 - 128) 7
                                   + Z.shiftl (sleb1 b2) 14, r2)
                        else None
                    | [] => None
                    end
           | [] => None
           end
  | [] => None
  end.

(* The emitted grow-arm BODY span, verbatim (select/grow/OOM), with the
   OOM-message immediate as a parameter (its SLEB bytes). Matched byte-
   for-byte and abstracted to `SGrow`. Call indices: 6 = $eprintln_block,
   2 = the exit import; 0 = unreachable. *)
Definition grow_span (oom_leb : list Z) : list Z :=
  [32;2;63;0;65;16;116;107;65;255;255;3;106;65;16;118;63;0;
   32;2;63;0;65;16;116;107;65;255;255;3;106;65;16;118;63;0;
   75;27;64;0] ++ [65] ++ oom_leb ++ [72;4;64] ++ [65] ++ oom_leb ++
  [16;6;65;1;16;2;0;11].

Fixpoint strip_prefix (pre bs : list Z) : option (list Z) :=
  match pre, bs with
  | [], r => Some r
  | p :: pre', b :: bs' => if p =? b then strip_prefix pre' bs' else None
  | _ :: _, [] => None
  end.

(* Local naming for $alloc: 0=len, 1=base, 2=next, 3=want, 4=head. *)
Definition a_get (i : Z) : option aexpr :=
  if i =? 0 then Some ALen else if i =? 1 then Some ABase
  else if i =? 2 then Some ANext else if i =? 3 then Some AWant
  else if i =? 4 then Some AHead else None.

Definition a_set (i : Z) : option (aexpr -> astmt) :=
  if i =? 1 then Some ASetBase else if i =? 2 then Some ASetNext
  else if i =? 3 then Some ASetWant else if i =? 4 then Some ASetHead
  else None.

(* The $alloc decoder: same symbolic-stack shape, over the slice-2
   grammar, with the tee desugar and the grow-span match. `if` bodies
   are statement LISTS here (slice 2's `AIf` takes a list). *)
Fixpoint adecode_go (fuel : nat) (oom_leb : list Z) (bs : list Z)
                    (stk : list aexpr) (acc : list astmt)
  : option (list astmt * list Z) :=
  match fuel with
  | O => None
  | S f =>
      match strip_prefix (4 :: 64 :: grow_span oom_leb ++ [11]) bs with
      | Some r =>
          match stk with
          | cnd :: stk' =>
              adecode_go f oom_leb r stk' (AIf cnd [SGrow] :: acc)
          | [] => None
          end
      | None =>
      match bs with
      | 11 :: r =>
          match stk with
          | [] => Some (rev acc, r)
          | [v] => Some (rev (ARetV v :: acc), r)
          | _ => None
          end
      | 15 :: r =>
          match stk with
          | v :: stk' => adecode_go f oom_leb r stk' (ARetV v :: acc)
          | [] => None
          end
      | 32 :: i :: r =>
          match a_get i with
          | Some e => adecode_go f oom_leb r (e :: stk) acc
          | None => None
          end
      | 33 :: i :: r =>
          match a_set i, stk with
          | Some mk, v :: stk' => adecode_go f oom_leb r stk' (mk v :: acc)
          | _, _ => None
          end
      | 34 :: i :: r =>
          match a_set i, a_get i, stk with
          | Some mk, Some rd, v :: stk' =>
              adecode_go f oom_leb r (rd :: stk') (mk v :: acc)
          | _, _, _ => None
          end
      | 35 :: 1 :: r => adecode_go f oom_leb r (AGHeap :: stk) acc
      | 36 :: 1 :: r =>
          match stk with
          | v :: stk' => adecode_go f oom_leb r stk' (ASetGHeap v :: acc)
          | [] => None
          end
      | 63 :: 0 :: r => adecode_go f oom_leb r (AMemSize :: stk) acc
      | 40 :: 2 :: off :: r =>
          match stk with
          | a :: stk' =>
              adecode_go f oom_leb r
                (ALoad (if off =? 0 then a else AAdd a (AC off)) :: stk') acc
          | [] => None
          end
      | 54 :: 2 :: off :: r =>
          match stk with
          | v :: a :: stk' =>
              adecode_go f oom_leb r stk'
                (AStore (if off =? 0 then a else AAdd a (AC off)) v :: acc)
          | _ => None
          end
      | 65 :: r =>
          match sleb_dec r with
          | Some (z, r') => adecode_go f oom_leb r' (AC z :: stk) acc
          | None => None
          end
      | 73 :: r =>
          match stk with
          | b :: a :: stk' => adecode_go f oom_leb r (ALtU a b :: stk') acc
          | _ => None
          end
      | 75 :: r =>
          match stk with
          | b :: a :: stk' => adecode_go f oom_leb r (AGtU a b :: stk') acc
          | _ => None
          end
      | 77 :: r =>
          match stk with
          | b :: a :: stk' => adecode_go f oom_leb r (ALeU a b :: stk') acc
          | _ => None
          end
      | 103 :: r =>
          match stk with
          | a :: stk' => adecode_go f oom_leb r (AClz a :: stk') acc
          | [] => None
          end
      | 106 :: r =>
          match stk with
          | b :: a :: stk' => adecode_go f oom_leb r (AAdd a b :: stk') acc
          | _ => None
          end
      | 107 :: r =>
          match stk with
          | b :: a :: stk' => adecode_go f oom_leb r (ASub a b :: stk') acc
          | _ => None
          end
      | 113 :: r =>
          match stk with
          | b :: a :: stk' => adecode_go f oom_leb r (ALand a b :: stk') acc
          | _ => None
          end
      | 116 :: r =>
          match stk with
          | b :: a :: stk' => adecode_go f oom_leb r (AShl a b :: stk') acc
          | _ => None
          end
      | 118 :: r =>
          match stk with
          | b :: a :: stk' => adecode_go f oom_leb r (AShrU a b :: stk') acc
          | _ => None
          end
      | 4 :: 64 :: r =>
          match stk with
          | cnd :: stk' =>
              match adecode_go f oom_leb r [] [] with
              | Some (body, rest) =>
                  adecode_go f oom_leb rest stk' (AIf cnd body :: acc)
              | None => None
              end
          | [] => None
          end
      | _ => None
      end
      end
  end.

Definition adecode (oom_leb : list Z) (bs : list Z) : option (list astmt) :=
  match adecode_go 1000 oom_leb bs [] [] with
  | Some (ss, []) => Some ss
  | _ => None
  end.

(* The emitted $alloc body (probe OOM immediate 0; wrapper and locals
   vector stripped — dumped by `dump_runtime_bytes`, grounded by
   check-structural-bytes.sh). *)
Definition alloc_bytes : list Z :=
  [32;0;65;15;106;65;124;113;33;3;
   32;3;65;16;73;4;64;65;16;33;3;11;
   65;28;32;3;65;1;107;103;107;33;2;
   32;2;65;16;73;4;64;
     32;2;65;2;116;65;48;106;33;2;
     32;2;40;2;0;34;4;4;64;
       32;2;32;4;40;2;12;54;2;0;
       32;4;65;1;54;2;0;
       32;4;32;0;54;2;4;
       32;4;65;16;32;2;65;48;107;65;2;118;116;65;12;107;54;2;8;
       32;4;15;11;
     65;28;32;3;65;1;107;103;107;33;2;
     65;16;32;2;116;33;3;11;
   35;1;33;1;
   32;1;65;12;106;32;0;106;65;3;106;65;124;113;33;2;
   32;3;65;128;128;32;77;4;64;32;1;32;3;106;33;2;11;
   32;2;63;0;65;16;116;75;
   4;64;32;2;63;0;65;16;116;107;65;255;255;3;106;65;16;118;63;0;
   32;2;63;0;65;16;116;107;65;255;255;3;106;65;16;118;63;0;
   75;27;64;0;65;0;72;4;64;65;0;16;6;65;1;16;2;0;11;11;
   32;1;65;1;54;2;0;
   32;1;32;0;54;2;4;
   32;1;32;3;65;12;107;54;2;8;
   32;2;36;1;
   32;1;11].

(* ══ THE ALLOC DECODE THEOREM — by computation ══════════════════════════

   With the probe OOM immediate ([0]) and 48 = FREELIST_BASE, the bytes
   decode to EXACTLY slice 2's proven tree. *)
Theorem alloc_bytes_decode_to_the_tree :
  adecode [0] alloc_bytes = Some (alloc_body 48).
Proof. reflexivity. Qed.

