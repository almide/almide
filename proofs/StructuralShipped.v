(* Almide v1 trust spine — the structural runtime's SHIPPED FORMS decode to
   the proven trees.

   StructuralDecode.v proves that the byte lists the EMITTER produces for
   `$inc` / `$dec_flat` / `$free` / `$alloc` decode to the trees the
   realization theorems are about. A module ships after two more passes
   (crates/almide-wasi: the stock-WASI transform and the prune), and they
   edit those bodies:

   - a function index and a global index are renumbered;
   - `i32.const 1; call $proc_exit` stands in front of every `unreachable`;
   - the out-of-memory message's address is the program's own (the emitted
     lists carry the probe value 0 there).

   StructuralDecode.v's decoder is conservative about exactly these: it
   reads `global.get 3`, `call 33` and a bare `unreachable`, so it rejects
   every `$dec_flat` and every `$alloc` that ships. The decoders below read
   the shipped forms. They BIND an index where the first decoder compares it
   with a constant, read the exit call in front of a trap as part of the
   trap, and return, beside the tree, WHICH indices they read in which role.
   Nothing else is relaxed: every other opcode, local index, memarg and
   constant is read as StructuralDecode.v reads it.

   What a decode then says: "these bytes are the proven tree, with global g
   read as `$line_end`, function f read as `$free`, function x read as the
   exit called in front of a trap". Whether g, f and x ARE those is not
   decided here. f is: proofs/check-shipped-routines.sh requires, per
   module, that the function read as `$free` is the one whose own bytes
   decode to `free_body`. x is checked against the import section by the
   byte-level check (proved.rs: the import named `proc_exit`). g is a
   naming, as index 3 was.

   TRUSTED beyond StructuralDecode.v's list: what the imported exit does.
   The trees read `i32.const 1; call x; unreachable` as the trap they read
   `unreachable` as. Control never continues past it, whatever the import
   does, because the `unreachable` still follows; but the trees do not model
   the import's own effects (a host function runs with the exported memory in
   reach before it ends the process or returns into the trap). *)

From AlmideTrust Require Import StructuralRuntime.
From AlmideTrust Require LargeTree.
From AlmideTrust Require Import StructuralAlloc.
From AlmideTrust Require Import StructuralDecode.
From Stdlib Require Import ZArith.
From Stdlib Require Import List.
Import ListNotations.
Open Scope Z_scope.

(* ══ INDICES AS THEY ARE ENCODED ═════════════════════════════════════════

   A function or global index is an unsigned LEB128 number. The emitted
   lists carry indices below 128, one byte each, and StructuralDecode.v
   reads them as one byte. A shipped module can call function 195, which is
   the two bytes 195;1.

   `tokens` walks a function body instruction by instruction and replaces
   the immediate of every `call`, `global.get` and `global.set` by ONE
   element holding the index's value. Every other byte is kept as it is, so
   the decoders below read constants, memargs and local indices exactly as
   StructuralDecode.v does. They bind an index as one element and never
   inspect it, which is why their theorems hold for every index.

   It is strict: an opcode outside its table (a float constant, an indirect
   call, a table or reference instruction, a prefixed instruction) is no
   token list at all. *)

(* An unsigned LEB128 number of at most five bytes: its value and the rest. *)
Fixpoint uleb_go (n : nat) (sh acc : Z) (bs : list Z) : option (Z * list Z) :=
  match n, bs with
  | S n', b :: r =>
      if b <? 128 then Some (acc + Z.shiftl b sh, r)
      else uleb_go n' (sh + 7) (acc + Z.shiftl (b - 128) sh) r
  | _, _ => None
  end.

Definition uleb (bs : list Z) : option (Z * list Z) := uleb_go 5 0 0 bs.

(* The bytes of one LEB128 number, signed or unsigned (the continuation bit
   is the same), and the rest. *)
Fixpoint leb_span (n : nat) (bs : list Z) : option (list Z * list Z) :=
  match n, bs with
  | S n', b :: r =>
      if b <? 128 then Some ([b], r)
      else match leb_span n' r with
           | Some (s, r') => Some (b :: s, r')
           | None => None
           end
  | _, _ => None
  end.

Definition is_in (op : Z) (l : list Z) : bool := existsb (Z.eqb op) l.

Fixpoint tokens_go (fuel : nat) (bs : list Z) (acc : list Z) : option (list Z) :=
  match fuel with
  | O => None
  | S f =>
      match bs with
      | [] => Some (rev acc)
      | op :: r =>
          if is_in op [16; 35; 36] then
            (* call, global.get, global.set: the index, as its value *)
            match uleb r with
            | Some (v, r') => tokens_go f r' (v :: op :: acc)
            | None => None
            end
          else if is_in op [2; 3; 4; 12; 13; 32; 33; 34; 65; 66] then
            (* block, loop, if (block type); br, br_if; local.get / set / tee;
               i32.const, i64.const: one LEB128 immediate, kept *)
            match leb_span 10 r with
            | Some (s, r') => tokens_go f r' (rev s ++ op :: acc)
            | None => None
            end
          else if andb (40 <=? op) (op <=? 62) then
            (* loads and stores: alignment and offset, kept *)
            match leb_span 5 r with
            | Some (s1, r1) =>
                match leb_span 5 r1 with
                | Some (s2, r2) => tokens_go f r2 (rev s2 ++ rev s1 ++ op :: acc)
                | None => None
                end
            | None => None
            end
          else if is_in op [63; 64] then
            (* memory.size, memory.grow: the memory's index byte, kept *)
            match r with
            | b :: r' => tokens_go f r' (b :: op :: acc)
            | [] => None
            end
          else if orb (is_in op [0; 1; 5; 11; 15; 26; 27])
                      (andb (69 <=? op) (op <=? 196)) then
            (* unreachable, nop, else, end, return, drop, select, and the
               numeric instructions: no immediate *)
            tokens_go f r (op :: acc)
          else None
      end
  end.

Definition tokens (bs : list Z) : option (list Z) :=
  tokens_go (S (length bs)) bs [].

(* A two-byte index becomes one element; a constant's bytes are kept. *)
Example a_wide_index_is_one_token :
  tokens [65; 240; 2; 16; 195; 1; 35; 128; 1; 11] = Some [65; 240; 2; 16; 195; 35; 128; 11].
Proof. reflexivity. Qed.

Example an_unknown_opcode_is_no_token_list : tokens [32; 0; 17; 0; 0; 11] = None.
Proof. reflexivity. Qed.

(* ══ `$inc` / `$dec_flat` / `$free` ═══════════════════════════════════════ *)

(* Which indices a decode read in which role, latest first. *)
Record roles := mkRoles {
  r_floor : list Z;   (* globals read as `$line_end` *)
  r_free : list Z;    (* callees read as `$free` *)
  r_exit : list Z     (* callees of the exit call in front of a trap *)
}.

Definition no_roles : roles := mkRoles [] [] [].

(* StructuralDecode.decode_go, with the three shipped forms. *)
Fixpoint sdecode_go (fuel : nat) (nm : naming) (bs : list Z)
                    (stk : list expr) (acc : list stmt) (ro : roles)
  : option (list stmt * list Z * roles) :=
  match fuel with
  | O => None
  | S f =>
      match bs with
      | 11 :: r => (* end — the stack must be drained *)
          match stk with
          | [] => Some (rev acc, r, ro)
          | _ => None
          end
      | 15 :: r => (* return *)
          sdecode_go f nm r stk (SRet :: acc) ro
      | 32 :: i :: r => (* local.get *)
          match n_get nm i with
          | Some e => sdecode_go f nm r (e :: stk) acc ro
          | None => None
          end
      | 33 :: i :: r => (* local.set *)
          match n_set nm i, stk with
          | Some mk, v :: stk' => sdecode_go f nm r stk' (mk v :: acc) ro
          | _, _ => None
          end
      | 35 :: g :: r => (* global.get: read as $line_end, the index reported *)
          sdecode_go f nm r (EFloor :: stk) acc
            (mkRoles (g :: r_floor ro) (r_free ro) (r_exit ro))
      | 40 :: 2 :: off :: r => (* i32.load align=2 *)
          match stk with
          | a :: stk' => sdecode_go f nm r (ELoad (fold_off a off) :: stk') acc ro
          | [] => None
          end
      | 54 :: 2 :: off :: r => (* i32.store align=2 *)
          match stk with
          | v :: a :: stk' =>
              sdecode_go f nm r stk' (SStore (fold_off a off) v :: acc) ro
          | _ => None
          end
      | 65 :: 1 :: 16 :: x :: 0 :: r =>
          (* i32.const 1; call x; unreachable — the trap as it ships *)
          sdecode_go f nm r stk (STrap :: acc)
            (mkRoles (r_floor ro) (r_free ro) (x :: r_exit ro))
      | 65 :: b :: r => (* i32.const, single-byte SLEB *)
          sdecode_go f nm r (EC (sleb1 b) :: stk) acc ro
      | 69 :: r => (* i32.eqz *)
          match stk with
          | a :: stk' => sdecode_go f nm r (EEqz a :: stk') acc ro
          | [] => None
          end
      | 70 :: r => (* i32.eq *)
          match stk with
          | b :: a :: stk' => sdecode_go f nm r (EEq a b :: stk') acc ro
          | _ => None
          end
      | 0 :: r => (* unreachable — the trap as it is emitted *)
          sdecode_go f nm r stk (STrap :: acc) ro
      | 73 :: r => (* i32.lt_u *)
          match stk with
          | b :: a :: stk' => sdecode_go f nm r (ELtU a b :: stk') acc ro
          | _ => None
          end
      | 79 :: r => (* i32.ge_u *)
          match stk with
          | b :: a :: stk' => sdecode_go f nm r (EGeU a b :: stk') acc ro
          | _ => None
          end
      | 103 :: r => (* i32.clz *)
          match stk with
          | a :: stk' => sdecode_go f nm r (EClz a :: stk') acc ro
          | [] => None
          end
      | 106 :: r => (* i32.add *)
          match stk with
          | b :: a :: stk' => sdecode_go f nm r (EAdd a b :: stk') acc ro
          | _ => None
          end
      | 107 :: r => (* i32.sub *)
          match stk with
          | b :: a :: stk' => sdecode_go f nm r (ESub a b :: stk') acc ro
          | _ => None
          end
      | 113 :: r => (* i32.and *)
          match stk with
          | b :: a :: stk' => sdecode_go f nm r (ELand a b :: stk') acc ro
          | _ => None
          end
      | 116 :: r => (* i32.shl *)
          match stk with
          | b :: a :: stk' => sdecode_go f nm r (EShl a b :: stk') acc ro
          | _ => None
          end
      | 16 :: x :: r => (* call x with the block: read as $free, x reported *)
          match stk with
          | EBlk :: stk' =>
              sdecode_go f nm r stk' (SCallFree :: acc)
                (mkRoles (r_floor ro) (x :: r_free ro) (r_exit ro))
          | _ => None
          end
      | 4 :: 64 :: r => (* if — ONE statement, or an inlined loop tree *)
          match stk with
          | cnd :: stk' =>
              match ldecode_go f r [] [] with
              | Some (s0 :: ss, [], 15, 11 :: rest) =>
                  sdecode_go f nm rest stk' (SIf cnd (SLRun (s0 :: ss)) :: acc) ro
              | _ =>
              match sdecode_go f nm r [] [] ro with
              | Some ([s], rest, ro') =>
                  sdecode_go f nm rest stk' (SIf cnd s :: acc) ro'
              | _ => None
              end
              end
          | [] => None
          end
      | _ => None
      end
  end.

(* Decode one full function body (empty starting stack, drained end), given
   as tokens. *)
Definition sdecode (nm : naming) (bs : list Z) : option (list stmt * roles) :=
  match sdecode_go 1000 nm bs [] [] no_roles with
  | Some (ss, [], ro) => Some (ss, ro)
  | _ => None
  end.

(* The same, from a body's bytes. *)
Definition sdecode_bytes (nm : naming) (bs : list Z) : option (list stmt * roles) :=
  match tokens bs with
  | Some ts => sdecode nm ts
  | None => None
  end.

(* The emitted lists are their own tokens: their indices are one byte. *)
Theorem inc_bytes_are_tokens : tokens inc_bytes = Some inc_bytes.
Proof. reflexivity. Qed.

Theorem dec_bytes_are_tokens : tokens dec_bytes = Some dec_bytes.
Proof. reflexivity. Qed.

Theorem free_bytes_are_tokens : tokens free_bytes = Some free_bytes.
Proof. reflexivity. Qed.

(* The emitted lists, read by this decoder: the same trees, and the indices
   StructuralDecode.v's decoder fixes. *)
Theorem inc_bytes_sdecode :
  sdecode inc_naming inc_bytes = Some (inc_body, mkRoles [3] [] []).
Proof. reflexivity. Qed.

Theorem dec_bytes_sdecode :
  sdecode dec_naming dec_bytes = Some (dec_body, mkRoles [3] [33] []).
Proof. reflexivity. Qed.

Theorem free_bytes_sdecode :
  sdecode free_naming free_bytes = Some (free_body 48, no_roles).
Proof. reflexivity. Qed.

(* `$inc` as it ships, in tokens: its one global index is whatever the prune
   left. `g`, `fr` and `x` below are index VALUES, of any size. *)
Definition inc_shipped (g : Z) : list Z :=
  [32;0;35;g;73;4;64;15;11;
   32;0;32;0;40;2;0;65;1;106;54;2;0;
   11].

(* `$dec_flat` as it ships: the global, the callee, and the exit call in
   front of the double-free trap. *)
Definition dec_shipped (g fr x : Z) : list Z :=
  [32;0;35;g;73;4;64;15;11;
   32;0;40;2;0;65;1;107;33;1;
   32;1;65;127;70;4;64;65;1;16;x;0;11;
   32;0;32;1;54;2;0;
   32;1;69;4;64;32;0;16;fr;11;
   11].

(* For EVERY index: the shipped forms are the proven trees. The decoder
   binds an index and never inspects it, so this holds by computation. *)
Theorem inc_shipped_decodes : forall g,
  sdecode inc_naming (inc_shipped g) = Some (inc_body, mkRoles [g] [] []).
Proof. intro g. reflexivity. Qed.

Theorem dec_shipped_decodes : forall g fr x,
  sdecode dec_naming (dec_shipped g fr x) = Some (dec_body, mkRoles [g] [fr] [x]).
Proof. intros g fr x. reflexivity. Qed.

(* ══ `$alloc` ════════════════════════════════════════════════════════════ *)

(* StructuralDecode.ldecode_go (the loop sublanguage of the inlined
   large-list take), binding the bump frontier's global index and reporting
   it. *)
Fixpoint sldecode_go (fuel : nat) (bs : list Z) (stk : list LargeTree.lexpr)
                     (acc : list LargeTree.lstmt) (hs : list Z)
  : option (list LargeTree.lstmt * list LargeTree.lexpr * Z * list Z * list Z) :=
  match fuel with
  | O => None
  | S f =>
      match bs with
      | 11 :: r => Some (rev acc, stk, 11, r, hs)
      | 5 :: r => Some (rev acc, stk, 5, r, hs)
      | 69 :: 13 :: 1 :: r => Some (rev acc, stk, 13, r, hs)
      | 12 :: 0 :: 11 :: 11 :: r => Some (rev acc, stk, 12, r, hs)
      | 15 :: r =>
          match stk with
          | [] => Some (rev acc, [], 15, r, hs)
          | v :: stk' => sldecode_go f r stk' (LargeTree.LRet v :: acc) hs
          end
      | 32 :: i :: r =>
          if andb (0 <=? i) (i <? 16)
          then sldecode_go f r (LargeTree.LL (Z.to_nat i) :: stk) acc hs
          else None
      | 33 :: i :: r =>
          match stk with
          | v :: stk' =>
              if andb (0 <=? i) (i <? 16)
              then sldecode_go f r stk' (LargeTree.LSet (Z.to_nat i) v :: acc) hs
              else None
          | [] => None
          end
      | 35 :: g :: r => sldecode_go f r (LargeTree.LG :: stk) acc (g :: hs)
      | 36 :: g :: r =>
          match stk with
          | v :: stk' => sldecode_go f r stk' (LargeTree.LSetG v :: acc) (g :: hs)
          | [] => None
          end
      | 40 :: 2 :: off :: r =>
          match stk with
          | a :: stk' =>
              sldecode_go f r
                (LargeTree.LLoad (LargeTree.LAdd a (LargeTree.LC off)) :: stk') acc hs
          | [] => None
          end
      | 54 :: 2 :: off :: r =>
          match stk with
          | v :: a :: stk' =>
              sldecode_go f r stk'
                (LargeTree.LStore (LargeTree.LAdd a (LargeTree.LC off)) v :: acc) hs
          | _ => None
          end
      | 65 :: r =>
          match sleb_dec r with
          | Some (z, r') => sldecode_go f r' (LargeTree.LC z :: stk) acc hs
          | None => None
          end
      | 2 :: 64 :: 3 :: 64 :: r =>
          match sldecode_go f r [] [] hs with
          | Some ([], [cnd], 13, r1, hs1) =>
              match sldecode_go f r1 [] [] hs1 with
              | Some (body, [], 12, r2, hs2) =>
                  sldecode_go f r2 stk (LargeTree.LWhile cnd body :: acc) hs2
              | _ => None
              end
          | _ => None
          end
      | 4 :: 64 :: r =>
          match stk with
          | cnd :: stk' =>
              match sldecode_go f r [] [] hs with
              | Some (th, [], 11, r1, hs1) =>
                  sldecode_go f r1 stk' (LargeTree.LIf cnd th [] :: acc) hs1
              | Some (th, [], 5, r1, hs1) =>
                  match sldecode_go f r1 [] [] hs1 with
                  | Some (el, [], 11, r2, hs2) =>
                      sldecode_go f r2 stk' (LargeTree.LIf cnd th el :: acc) hs2
                  | _ => None
                  end
              | _ => None
              end
          | [] => None
          end
      | op :: r =>
          match stk with
          | b :: a :: stk' =>
              match lbin op a b with
              | Some e => sldecode_go f r (e :: stk') acc hs
              | None => None
              end
          | _ => None
          end
      | [] => None
      end
  end.

(* Which indices and which address an `$alloc` decode read, latest first. *)
Record aroles := mkARoles {
  a_heap : list Z;    (* globals read as the bump frontier *)
  a_eprint : list Z;  (* callees given the out-of-memory message *)
  a_exit0 : list Z;   (* callees of the emitter's own exit call *)
  a_exit : list Z;    (* callees of the exit call the transform adds *)
  a_oom : list Z      (* the out-of-memory message's address *)
}.

Definition no_aroles : aroles := mkARoles [] [] [] [] [].

(* The C-197 die, as emitted and as shipped:
     i32.const <message>; call $eprintln_block; i32.const 1; call $exit;
     [i32.const 1; call $proc_exit;] unreachable
   The message's address is read as a constant of any value, the callees are
   bound, and the bytes after the `unreachable` are returned. *)
Definition die (bs : list Z) : option (Z * Z * Z * option Z * list Z) :=
  match bs with
  | 65 :: r =>
      match sleb_dec r with
      | Some (z, 16 :: fe :: 65 :: 1 :: 16 :: fx :: 65 :: 1 :: 16 :: px :: 0 :: r') =>
          Some (z, fe, fx, Some px, r')
      | Some (z, 16 :: fe :: 65 :: 1 :: 16 :: fx :: 0 :: r') =>
          Some (z, fe, fx, None, r')
      | _ => None
      end
  | _ => None
  end.

Definition died (ro : aroles) (z fe fx : Z) (px : option Z) : aroles :=
  mkARoles (a_heap ro) (fe :: a_eprint ro) (fx :: a_exit0 ro)
           (match px with Some p => p :: a_exit ro | None => a_exit ro end)
           (z :: a_oom ro).

Definition heaped (ro : aroles) (g : Z) : aroles :=
  mkARoles (g :: a_heap ro) (a_eprint ro) (a_exit0 ro) (a_exit ro) (a_oom ro).

Definition with_heap (ro : aroles) (hs : list Z) : aroles :=
  mkARoles hs (a_eprint ro) (a_exit0 ro) (a_exit ro) (a_oom ro).

(* The grow arm's body up to its die: the page computation, `memory.grow`,
   and the test of its result against the constant 0 (NOT the message's
   address: runtime_alloc.rs emits `i32.const 0; i32.lt_s`). *)
Definition grow_head : list Z :=
  [32;2;63;0;65;16;116;107;65;255;255;3;106;65;16;118;63;0;
   32;2;63;0;65;16;116;107;65;255;255;3;106;65;16;118;63;0;
   75;27;64;0;65;0;72;4;64].

(* StructuralDecode.adecode_go, with the die read by `die` (so the message's
   address is no parameter) and the bump frontier's global bound. *)
Fixpoint sadecode_go (fuel : nat) (bs : list Z)
                     (stk : list aexpr) (acc : list astmt) (ro : aroles)
  : option (list astmt * list Z * aroles) :=
  match fuel with
  | O => None
  | S f =>
      match strip_prefix (4 :: 64 :: grow_head) bs with
      | Some r0 =>
          (* the grow arm: its die, the inner `end`, the arm's `end` *)
          match die r0, stk with
          | Some (z, fe, fx, px, 11 :: 11 :: r), cnd :: stk' =>
              sadecode_go f r stk' (AIf cnd [SGrow] :: acc) (died ro z fe fx px)
          | _, _ => None
          end
      | None =>
      match (match bs with
             | 4 :: 64 :: r0 =>
                 match die r0 with
                 | Some (z, fe, fx, px, 11 :: r) => Some (z, fe, fx, px, r)
                 | _ => None
                 end
             | _ => None
             end) with
      | Some (z, fe, fx, px, r) =>
          (* a wrap guard: its whole body is the die *)
          match stk with
          | cnd :: stk' =>
              sadecode_go f r stk' (AIf cnd [SOom] :: acc) (died ro z fe fx px)
          | [] => None
          end
      | None =>
      match bs with
      | 11 :: r =>
          match stk with
          | [] => Some (rev acc, r, ro)
          | [v] => Some (rev (ARetV v :: acc), r, ro)
          | _ => None
          end
      | 5 :: r =>
          match stk with
          | [] => Some (rev acc, 5 :: r, ro)
          | _ => None
          end
      | 15 :: r =>
          match stk with
          | v :: stk' => sadecode_go f r stk' (ARetV v :: acc) ro
          | [] => None
          end
      | 32 :: i :: r =>
          match a_get i with
          | Some e => sadecode_go f r (e :: stk) acc ro
          | None => None
          end
      | 33 :: i :: r =>
          match a_set i, stk with
          | Some mk, v :: stk' => sadecode_go f r stk' (mk v :: acc) ro
          | _, _ => None
          end
      | 34 :: i :: r =>
          match a_set i, a_get i, stk with
          | Some mk, Some rd, v :: stk' =>
              sadecode_go f r (rd :: stk') (mk v :: acc) ro
          | _, _, _ => None
          end
      | 35 :: g :: r => sadecode_go f r (AGHeap :: stk) acc (heaped ro g)
      | 36 :: g :: r =>
          match stk with
          | v :: stk' => sadecode_go f r stk' (ASetGHeap v :: acc) (heaped ro g)
          | [] => None
          end
      | 63 :: 0 :: r => sadecode_go f r (AMemSize :: stk) acc ro
      | 40 :: 2 :: off :: r =>
          match stk with
          | a :: stk' =>
              sadecode_go f r
                (ALoad (if off =? 0 then a else AAdd a (AC off)) :: stk') acc ro
          | [] => None
          end
      | 54 :: 2 :: off :: r =>
          match stk with
          | v :: a :: stk' =>
              sadecode_go f r stk'
                (AStore (if off =? 0 then a else AAdd a (AC off)) v :: acc) ro
          | _ => None
          end
      | 65 :: r =>
          match sleb_dec r with
          | Some (z, r') => sadecode_go f r' (AC z :: stk) acc ro
          | None => None
          end
      | 73 :: r =>
          match stk with
          | b :: a :: stk' => sadecode_go f r (ALtU a b :: stk') acc ro
          | _ => None
          end
      | 75 :: r =>
          match stk with
          | b :: a :: stk' => sadecode_go f r (AGtU a b :: stk') acc ro
          | _ => None
          end
      | 77 :: r =>
          match stk with
          | b :: a :: stk' => sadecode_go f r (ALeU a b :: stk') acc ro
          | _ => None
          end
      | 103 :: r =>
          match stk with
          | a :: stk' => sadecode_go f r (AClz a :: stk') acc ro
          | [] => None
          end
      | 106 :: r =>
          match stk with
          | b :: a :: stk' => sadecode_go f r (AAdd a b :: stk') acc ro
          | _ => None
          end
      | 107 :: r =>
          match stk with
          | b :: a :: stk' => sadecode_go f r (ASub a b :: stk') acc ro
          | _ => None
          end
      | 113 :: r =>
          match stk with
          | b :: a :: stk' => sadecode_go f r (ALand a b :: stk') acc ro
          | _ => None
          end
      | 116 :: r =>
          match stk with
          | b :: a :: stk' => sadecode_go f r (AShl a b :: stk') acc ro
          | _ => None
          end
      | 118 :: r =>
          match stk with
          | b :: a :: stk' => sadecode_go f r (AShrU a b :: stk') acc ro
          | _ => None
          end
      | 4 :: 64 :: r =>
          match stk with
          | cnd :: stk' =>
              match sadecode_go f r [] [] ro with
              | Some (body, 5 :: r1, ro1) =>
                  (* the else arm: the inlined large-list take *)
                  match sldecode_go f r1 [] [] (a_heap ro1) with
                  | Some (ss, [], 11, r2, hs) =>
                      sadecode_go f r2 stk'
                        (AIfElse cnd body [SLTake ss] :: acc) (with_heap ro1 hs)
                  | _ => None
                  end
              | Some (body, rest, ro1) =>
                  sadecode_go f rest stk' (AIf cnd body :: acc) ro1
              | None => None
              end
          | [] => None
          end
      | _ => None
      end
      end
      end
  end.

Definition sadecode (bs : list Z) : option (list astmt * aroles) :=
  match sadecode_go 1000 bs [] [] no_aroles with
  | Some (ss, [], ro) => Some (ss, ro)
  | _ => None
  end.

(* The same, from the body's bytes. *)
Definition sadecode_bytes (bs : list Z) : option (list astmt * aroles) :=
  match tokens bs with
  | Some ts => sadecode ts
  | None => None
  end.

Theorem alloc_bytes_are_tokens : tokens alloc_bytes = Some alloc_bytes.
Proof. reflexivity. Qed.

(* The emitted list, read by this decoder: the same tree, the indices
   StructuralDecode.v's decoder fixes, and the probe address. *)
Theorem alloc_bytes_sadecode :
  sadecode alloc_bytes
  = Some (alloc_body 48, mkARoles [1;1;1;1] [6;6;6] [2;2;2] [] [0;0;0]).
Proof. reflexivity. Qed.

(* `$alloc` around its three dies and its bump frontier's global: the
   emitted list is `alloc_form` of the emitted die and global 1. *)
Definition alloc_form (d : list Z) (h : Z) : list Z :=
  [32;0;65;15;106;65;124;113;33;3;
   32;3;32;0;73;4;64] ++ d ++ [11;
   32;3;65;16;73;4;64;65;16;33;3;11;
   65;28;32;3;65;1;107;103;107;33;2;
   32;2;65;13;73;4;64;
     32;2;65;2;116;65;48;106;33;2;
     32;2;40;2;0;34;4;4;64;
       32;2;32;4;40;2;12;54;2;0;
       32;4;65;1;54;2;0;
       32;4;32;0;54;2;4;
       32;4;65;16;32;2;65;48;107;65;2;118;116;65;12;107;54;2;8;
       32;4;15;11;
     65;28;32;3;65;1;107;103;107;33;2;
     65;16;32;2;116;33;3;5;
     65;0;40;2;12;33;7;2;64;3;64;32;7;65;0;71;32;7;
     40;2;4;32;3;73;113;69;13;1;32;6;33;5;32;7;33;6;
     32;7;40;2;12;33;7;12;0;11;11;32;7;65;0;71;4;64;
     32;7;40;2;4;33;8;32;8;32;3;107;65;128;128;4;79;4;
     64;32;7;32;3;106;33;9;32;9;65;0;54;2;0;32;9;32;
     8;32;3;107;54;2;4;32;9;32;7;40;2;12;54;2;12;32;
     6;32;9;54;2;12;32;3;33;8;5;32;6;32;7;40;2;12;
     54;2;12;11;32;7;65;1;54;2;0;32;7;32;0;54;2;4;
     32;7;32;8;65;12;107;54;2;8;32;7;15;11;32;6;32;6;
     40;2;4;106;35;h;70;4;64;32;5;65;0;54;2;12;32;6;
     36;h;11;11;
   35;h;33;1;
   32;1;65;12;106;32;0;106;65;3;106;65;124;113;33;2;
   32;3;65;128;128;4;77;4;64;32;1;32;3;106;33;2;11;
   32;2;32;1;73;4;64] ++ d ++ [11;
   32;2;63;0;65;16;116;75;
   4;64] ++ grow_head ++ d ++ [11;11;
   32;1;65;1;54;2;0;
   32;1;32;0;54;2;4;
   32;1;32;3;65;12;107;54;2;8;
   32;2;36;h;
   32;1;11].

(* The die as the emitter writes it, and as it ships. `oom` is the message
   address's SLEB bytes. *)
Definition die_emitted (oom : list Z) (fe fx : Z) : list Z :=
  [65] ++ oom ++ [16;fe;65;1;16;fx;0].

Definition die_shipped (oom : list Z) (fe fx px : Z) : list Z :=
  [65] ++ oom ++ [16;fe;65;1;16;fx;65;1;16;px;0].

Theorem alloc_form_is_the_emitted_list :
  alloc_form (die_emitted [0] 6 2) 1 = alloc_bytes.
Proof. reflexivity. Qed.

(* A real program's message is not at address 0. With the address 1024
   (SLEB 128;8), for EVERY callee and global index, the emitted and the
   shipped forms are the proven tree. (Before its grow span was corrected,
   StructuralDecode.adecode rejected every such body: it expected the
   message's address where the emitter writes the constant 0.) *)
Theorem alloc_emitted_decodes_at_1024 : forall fe fx h,
  sadecode (alloc_form (die_emitted [128;8] fe fx) h)
  = Some (alloc_body 48,
          mkARoles [h;h;h;h] [fe;fe;fe] [fx;fx;fx] [] [1024;1024;1024]).
Proof. intros fe fx h. reflexivity. Qed.

Theorem alloc_shipped_decodes_at_1024 : forall fe fx px h,
  sadecode (alloc_form (die_shipped [128;8] fe fx px) h)
  = Some (alloc_body 48,
          mkARoles [h;h;h;h] [fe;fe;fe] [fx;fx;fx] [px;px;px] [1024;1024;1024]).
Proof. intros fe fx px h. reflexivity. Qed.

(* The first decoder, on a body whose message is not at 0: it reads it now
   that its grow span compares the grow result with 0. *)
Theorem alloc_emitted_adecodes_at_1024 :
  adecode [128;8] (alloc_form (die_emitted [128;8] 6 2) 1) = Some (alloc_body 48).
Proof. reflexivity. Qed.

(* ══ ONE MODULE'S ROUTINES ═══════════════════════════════════════════════

   What proofs/check-shipped-routines.sh has the kernel check per shipped
   module, on the module's own BYTES (tokenised here). The tree is compared
   by conversion; the roles are folded into what must hold of them. *)

Definition all_are (x : Z) (l : list Z) : bool := forallb (Z.eqb x) l.

Definition same (l : list Z) : bool :=
  match l with
  | [] => true
  | x :: r => all_are x r
  end.

(* `$dec_flat`: its tree, the global it reads as `$line_end`, and whether it
   frees through `free_ix` and exits, in front of its trap, through
   `exit_ix`. *)
Definition dec_verdict (free_ix exit_ix : Z) (bs : list Z)
  : option (list stmt * list Z * bool) :=
  match sdecode_bytes dec_naming bs with
  | Some (t, ro) =>
      Some (t, r_floor ro,
            andb (all_are free_ix (r_free ro)) (all_are exit_ix (r_exit ro)))
  | None => None
  end.

(* `$alloc`: its tree, the globals it reads as the bump frontier, and whether
   every exit call the transform added is `exit_ix`, and its three dies name
   one message, one printer and one exit. *)
Definition alloc_verdict (exit_ix : Z) (bs : list Z)
  : option (list astmt * list Z * bool) :=
  match sadecode_bytes bs with
  | Some (t, ro) =>
      Some (t, a_heap ro,
            andb (all_are exit_ix (a_exit ro))
                 (andb (same (a_eprint ro))
                       (andb (same (a_exit0 ro)) (same (a_oom ro)))))
  | None => None
  end.

(* The verdicts on the emitted lists and on the shipped schemas. *)
Theorem dec_bytes_verdict :
  dec_verdict 33 (-1) dec_bytes = Some (dec_body, [3], true).
Proof. reflexivity. Qed.

Theorem dec_shipped_verdict :
  dec_verdict 9 1 (dec_shipped 0 9 1) = Some (dec_body, [0], true).
Proof. reflexivity. Qed.

(* A flat release that frees through another function than the module's
   free routine, or exits through another import: the tree is read, the
   verdict is false. *)
Theorem dec_freeing_elsewhere_is_refused :
  dec_verdict 9 1 (dec_shipped 0 10 1) = Some (dec_body, [0], false).
Proof. reflexivity. Qed.

Theorem dec_exiting_elsewhere_is_refused :
  dec_verdict 9 1 (dec_shipped 0 9 2) = Some (dec_body, [0], false).
Proof. reflexivity. Qed.

Theorem alloc_shipped_verdict :
  alloc_verdict 1 (alloc_form (die_shipped [128;8] 7 1 1) 0)
  = Some (alloc_body 48, [0;0;0;0], true).
Proof. reflexivity. Qed.

(* Indices of two bytes, as larger modules ship them: `$dec_flat` freeing
   through function 195 (bytes 195;1), and `$alloc` whose own exit routine
   is function 195 and whose message is at 368 (bytes 240;2). *)
Theorem dec_with_a_wide_callee_verdict :
  dec_verdict 195 1
    [32;0;35;1;73;4;64;15;11;
     32;0;40;2;0;65;1;107;33;1;
     32;1;65;127;70;4;64;65;1;16;1;0;11;
     32;0;32;1;54;2;0;
     32;1;69;4;64;32;0;16;195;1;11;
     11]
  = Some (dec_body, [1], true).
Proof. reflexivity. Qed.

Theorem alloc_with_a_wide_callee_verdict :
  alloc_verdict 1
    (alloc_form [65;240;2;16;3;65;1;16;195;1;65;1;16;1;0] 1)
  = Some (alloc_body 48, [1;1;1;1], true).
Proof. reflexivity. Qed.

(* Controls: what the shipped-form decoders still refuse. A changed
   constant, a dropped instruction, a store in place of the load. *)
Theorem a_changed_constant_is_not_the_tree :
  option_map fst (sdecode inc_naming
    [32;0;35;3;73;4;64;15;11; 32;0;32;0;40;2;0;65;2;106;54;2;0; 11])
  <> Some inc_body.
Proof. discriminate. Qed.

Theorem a_dropped_guard_is_not_the_tree :
  option_map fst (sdecode inc_naming [32;0;32;0;40;2;0;65;1;106;54;2;0;11])
  <> Some inc_body.
Proof. discriminate. Qed.

Theorem a_call_without_the_block_does_not_decode :
  sdecode dec_naming
    [32;0;35;3;73;4;64;15;11;
     32;0;40;2;0;65;1;107;33;1;
     32;1;65;127;70;4;64;0;11;
     32;0;32;1;54;2;0;
     32;1;69;4;64;32;1;16;33;11;
     11] = None.
Proof. reflexivity. Qed.
