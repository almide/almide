(* Almide trust spine — the KIND of every value in a shipped function body
   (probe; not part of the gate, and no soundness theorem yet).

   GuardWindow.v, GuardFresh.v and GuardLoop.v judge an access against the
   block its base names. A read or a write of a block's first twelve bytes has
   no test to judge: it is inside the block whenever its address IS a block's
   base. `offset + width <= 12` alone would also pass a cursor into a payload,
   so this file first asks where the address came from.

   A value is WHOLE when no arithmetic of the function made it: a local or a
   parameter claimed whole, a 32- or 64-bit word read from memory, the result
   of a call whose result is claimed whole (the allocator's always), a global
   claimed whole; wrap and extend keep it. `KW c` is a whole value with the
   constant `c` added since, `KK c` a constant, `KA` anything else.

   `kpass` walks ONE body with the stack discipline of validation (blocks,
   loops, ifs, branches; the result of a construct is `KA`) and fails when a
   claim does not hold in this body:

   - a local claimed whole is assigned something that is not `KW 0` or a
     constant;
   - an argument at a position the callee's claims call whole is not;
   - the function's own result is claimed whole and a value it returns is not;
   - a global claimed whole is set to something that is not.

   On success it returns every load and store with the kind of its address.
   `header_ends` are the accesses whose address is `KW c` with
   `c + offset + width <= 12`.

   The claims are per module: a parameter is whole only if every call in every
   function of the module passes it a whole value, so a module's claims hold
   only when `kpass` accepts EVERY function of the module under the same
   claims (and the table's functions answer to their type's claims). That
   conjunction is taken outside this file.

   Not proved here: that in every run of an accepted module the address of a
   header access is a value some load, call or parameter delivered whole. *)

From Stdlib Require Import ZArith List Bool.
From AlmideTrust Require Import GuardWindow.
Import ListNotations.
Open Scope Z_scope.

Inductive kv : Type := KA | KW (c : Z) | KK (c : Z).

Definition kv_eqb (a b : kv) : bool :=
  match a, b with
  | KA, KA => true
  | KW x, KW y => x =? y
  | KK x, KK y => x =? y
  | _, _ => false
  end.

Definition whole (v : kv) : bool :=
  match v with
  | KW c => c =? 0
  | KK _ => true
  | KA => false
  end.

Definition kadd (a b : kv) : kv :=
  match a, b with
  | KW c, KK d => KW ((c + d) mod M32)
  | KK c, KW d => KW ((c + d) mod M32)
  | KK c, KK d => KK ((c + d) mod M32)
  | _, _ => KA
  end.

Fixpoint memz (x : Z) (l : list Z) : bool :=
  match l with
  | [] => false
  | y :: r => (x =? y) || memz x r
  end.

Fixpoint memzz (x y : Z) (l : list (Z * Z)) : bool :=
  match l with
  | [] => false
  | (a, b) :: r => ((x =? a) && (y =? b)) || memzz x y r
  end.

Definition nthz {A : Type} (l : list A) (n : Z) : option A :=
  if n <? 0 then None else nth_error l (Z.to_nat n).

(* A module's tables and claims. Function and type indices are the module's. *)
Record ctx : Type := mkC {
  c_sigs : list (Z * Z);        (* per function: parameters, results *)
  c_types : list (Z * Z);       (* per type: parameters, results *)
  c_alloc : Z;                  (* the allocator *)
  c_wpar : list (Z * Z);        (* (function, parameter) claimed whole *)
  c_wres : list Z;              (* functions whose result is claimed whole *)
  c_tpar : list (Z * Z);        (* (type, parameter) whole at an indirect call *)
  c_tres : list Z;              (* types whose indirect result is whole *)
  c_wglob : list Z              (* globals claimed whole *)
}.

(* One open construct: its operand stack, how many results and parameters it
   has, whether it is the function itself, whether the code now is dead. *)
Record frame : Type := mkF {
  f_stk : list kv; f_res : Z; f_par : Z; f_fn : bool; f_dead : bool
}.

Definition set_stk (fr : frame) (s : list kv) : frame :=
  mkF s (f_res fr) (f_par fr) (f_fn fr) (f_dead fr).

Definition pop1 (fs : list frame) : option (kv * list frame) :=
  match fs with
  | fr :: rest =>
      match f_stk fr with
      | v :: s => Some (v, set_stk fr s :: rest)
      | [] => if f_dead fr then Some (KW 0, fs) else None
      end
  | [] => None
  end.

Definition push1 (v : kv) (fs : list frame) : option (list frame) :=
  match fs with
  | fr :: rest => Some (set_stk fr (v :: f_stk fr) :: rest)
  | [] => None
  end.

Fixpoint pushn (n : nat) (v : kv) (fs : list frame) : option (list frame) :=
  match n with
  | O => Some fs
  | S k => match push1 v fs with Some fs' => pushn k v fs' | None => None end
  end.

Fixpoint popn (n : nat) (fs : list frame) : option (list frame) :=
  match n with
  | O => Some fs
  | S k => match pop1 fs with Some (_, fs') => popn k fs' | None => None end
  end.

(* Pop `n` arguments, the last one first; the argument at position `j` must be
   whole when `chk j`. *)
Fixpoint pop_args (n : nat) (chk : Z -> bool) (fs : list frame) : option (list frame) :=
  match n with
  | O => Some fs
  | S k =>
      match pop1 fs with
      | Some (v, fs') =>
          if chk (Z.of_nat k) && negb (whole v) then None else pop_args k chk fs'
      | None => None
      end
  end.

(* Code after an unconditional branch, a return or `unreachable`. *)
Definition kill (fs : list frame) : option (list frame) :=
  match fs with
  | fr :: rest => Some (mkF [] (f_res fr) (f_par fr) (f_fn fr) true :: rest)
  | [] => None
  end.

(* A return from here is acceptable: the function's result is not claimed
   whole, or the value on top of the current stack is whole. *)
Definition ret_ok (wres : bool) (fs : list frame) : bool :=
  if wres then
    match fs with
    | fr :: _ =>
        match f_stk fr with
        | v :: _ => whole v
        | [] => f_dead fr
        end
    | [] => false
    end
  else true.

(* A branch of depth `d` leaves the function when its target is the function's
   own frame. *)
Definition to_fn (fs : list frame) (d : Z) : option bool :=
  match nthz fs d with
  | Some fr => Some (f_fn fr)
  | None => None
  end.

(* A number with the count of bytes it took. *)
Definition ulebn (bs : list Z) : option (Z * list Z * Z) :=
  match uleb 5 bs with
  | Some (v, r) => Some (v, r, Z.of_nat (length bs) - Z.of_nat (length r))
  | None => None
  end.

Definition slebn (fuel : nat) (bs : list Z) : option (Z * list Z * Z) :=
  match sleb fuel bs with
  | Some (v, r) => Some (v, r, Z.of_nat (length bs) - Z.of_nat (length r))
  | None => None
  end.

(* A block type: (parameters, results). *)
Definition blocktype (c : ctx) (bs : list Z) : option (Z * Z * list Z) :=
  match bs with
  | b :: r =>
      if b =? 64 then Some (0, 0, r)
      else if (111 <=? b) && (b <=? 127) then Some (0, 1, r)
      else
        match sleb 5 bs with
        | Some (t, r') =>
            match nthz (c_types c) t with
            | Some (p, q) => Some (p, q, r')
            | None => None
            end
        | None => None
        end
  | [] => None
  end.

Definition is_keep (op : Z) : bool := (op =? 167) || (op =? 172) || (op =? 173).

(* One operand in, one out. *)
Definition is_unary (op : Z) : bool :=
  (op =? 69) || (op =? 80) || ((103 <=? op) && (op <=? 105)) || ((121 <=? op) && (op <=? 123))
  || ((139 <=? op) && (op <=? 145)) || ((153 <=? op) && (op <=? 159)) || ((167 <=? op) && (op <=? 196))
  || (op =? 209).

(* Two operands in, one out. *)
Definition is_binary (op : Z) : bool :=
  ((70 <=? op) && (op <=? 79)) || ((81 <=? op) && (op <=? 102)) || ((106 <=? op) && (op <=? 120))
  || ((124 <=? op) && (op <=? 138)) || ((146 <=? op) && (op <=? 152)) || ((160 <=? op) && (op <=? 166)).

(* An access the pass met: where it ends in the body, the kind of its address,
   its static offset and its width. *)
Definition acc : Type := (Z * kv * Z * Z)%type.

Fixpoint skip_table (n : nat) (bs : list Z) (fs : list frame) (wres : bool)
  : option (list Z * bool) :=
  (* the labels of a br_table: the bytes after them, and whether every label
     that leaves the function may *)
  match n with
  | O => Some (bs, true)
  | S k =>
      match uleb 5 bs with
      | Some (d, r) =>
          match to_fn fs d with
          | Some fn =>
              match skip_table k r fs wres with
              | Some (r', ok) => Some (r', ok && (negb fn || ret_ok wres fs))
              | None => None
              end
          | None => None
          end
      | None => None
      end
  end.

Section Pass.

Variable c : ctx.
Variable self : Z.            (* this function's index *)
Variable wl : list Z.         (* its locals claimed whole (parameters included) *)

Definition wres : bool := memz self (c_wres c).

(* `off` is where `bs` starts in the body; `bulk` turns true at a bulk memory
   instruction. *)
Fixpoint kpass (fuel : nat) (bs : list Z) (off : Z) (fs : list frame) (accs : list acc) (bulk : bool)
  : option (list acc * bool) :=
  match fuel with
  | O => None
  | S f =>
      match bs with
      | [] => None
      | op :: r =>
          let go := fun (r' : list Z) (fs' : list frame) =>
            kpass f r' (off + (Z.of_nat (length bs) - Z.of_nat (length r'))) fs' accs bulk in
          if op =? 0 then
            match kill fs with Some fs' => go r fs' | None => None end
          else if op =? 1 then go r fs
          else if (op =? 2) || (op =? 3) || (op =? 4) then
            match (if op =? 4 then pop1 fs else Some (KA, fs)) with
            | Some (_, fs1) =>
                match blocktype c r with
                | Some (p, q, r') =>
                    match popn (Z.to_nat p) fs1 with
                    | Some fs2 => go r' (mkF (repeat KA (Z.to_nat p)) q p false false :: fs2)
                    | None => None
                    end
                | None => None
                end
            | None => None
            end
          else if op =? 5 then
            match fs with
            | fr :: rest => go r (mkF (repeat KA (Z.to_nat (f_par fr))) (f_res fr) (f_par fr) (f_fn fr) false :: rest)
            | [] => None
            end
          else if op =? 11 then
            match fs with
            | fr :: rest =>
                if f_fn fr then
                  match r with
                  | [] => if f_dead fr || ret_ok wres fs then Some (accs, bulk) else None
                  | _ :: _ => None
                  end
                else
                  match pushn (Z.to_nat (f_res fr)) KA rest with
                  | Some fs' => go r fs'
                  | None => None
                  end
            | [] => None
            end
          else if op =? 12 then
            match uleb 5 r with
            | Some (d, r') =>
                match to_fn fs d with
                | Some fn =>
                    if negb fn || ret_ok wres fs then
                      match kill fs with Some fs' => go r' fs' | None => None end
                    else None
                | None => None
                end
            | None => None
            end
          else if op =? 13 then
            match uleb 5 r with
            | Some (d, r') =>
                match pop1 fs with
                | Some (_, fs1) =>
                    match to_fn fs1 d with
                    | Some fn => if negb fn || ret_ok wres fs1 then go r' fs1 else None
                    | None => None
                    end
                | None => None
                end
            | None => None
            end
          else if op =? 14 then
            match uleb 5 r with
            | Some (n, r1) =>
                match pop1 fs with
                | Some (_, fs1) =>
                    match skip_table (S (Z.to_nat n)) r1 fs1 wres with
                    | Some (r', true) =>
                        match kill fs1 with Some fs' => go r' fs' | None => None end
                    | _ => None
                    end
                | None => None
                end
            | None => None
            end
          else if op =? 15 then
            if ret_ok wres fs then
              match kill fs with Some fs' => go r fs' | None => None end
            else None
          else if (op =? 16) || (op =? 18) then
            match uleb 5 r with
            | Some (g, r') =>
                match nthz (c_sigs c) g with
                | Some (p, q) =>
                    match pop_args (Z.to_nat p) (fun j => memzz g j (c_wpar c)) fs with
                    | Some fs1 =>
                        let res := if (g =? c_alloc c) || memz g (c_wres c) then KW 0 else KA in
                        if op =? 18 then
                          if negb wres || whole res then
                            match kill fs1 with Some fs' => go r' fs' | None => None end
                          else None
                        else
                          match pushn (Z.to_nat q) res fs1 with
                          | Some fs' => go r' fs'
                          | None => None
                          end
                    | None => None
                    end
                | None => None
                end
            | None => None
            end
          else if (op =? 17) || (op =? 19) then
            match uleb 5 r with
            | Some (t, r1) =>
                match uleb 5 r1 with
                | Some (_, r') =>
                    match pop1 fs with
                    | Some (_, fs0) =>
                        match nthz (c_types c) t with
                        | Some (p, q) =>
                            match pop_args (Z.to_nat p) (fun j => memzz t j (c_tpar c)) fs0 with
                            | Some fs1 =>
                                let res := if memz t (c_tres c) then KW 0 else KA in
                                if op =? 19 then
                                  if negb wres || whole res then
                                    match kill fs1 with Some fs' => go r' fs' | None => None end
                                  else None
                                else
                                  match pushn (Z.to_nat q) res fs1 with
                                  | Some fs' => go r' fs'
                                  | None => None
                                  end
                            | None => None
                            end
                        | None => None
                        end
                    | None => None
                    end
                | None => None
                end
            | None => None
            end
          else if op =? 26 then
            match pop1 fs with Some (_, fs') => go r fs' | None => None end
          else if op =? 27 then
            match pop1 fs with
            | Some (_, fs1) =>
                match pop1 fs1 with
                | Some (b, fs2) =>
                    match pop1 fs2 with
                    | Some (a, fs3) =>
                        match push1 (if kv_eqb a b then a else KA) fs3 with
                        | Some fs' => go r fs'
                        | None => None
                        end
                    | None => None
                    end
                | None => None
                end
            | None => None
            end
          else if op =? 32 then
            match uleb 5 r with
            | Some (x, r') =>
                match push1 (if memz x wl then KW 0 else KA) fs with
                | Some fs' => go r' fs'
                | None => None
                end
            | None => None
            end
          else if (op =? 33) || (op =? 34) then
            match uleb 5 r with
            | Some (x, r') =>
                match pop1 fs with
                | Some (v, fs1) =>
                    if memz x wl && negb (whole v) then None
                    else if op =? 34 then
                      match push1 v fs1 with Some fs' => go r' fs' | None => None end
                    else go r' fs1
                | None => None
                end
            | None => None
            end
          else if op =? 35 then
            match uleb 5 r with
            | Some (g, r') =>
                match push1 (if memz g (c_wglob c) then KW 0 else KA) fs with
                | Some fs' => go r' fs'
                | None => None
                end
            | None => None
            end
          else if op =? 36 then
            match uleb 5 r with
            | Some (g, r') =>
                match pop1 fs with
                | Some (v, fs1) => if memz g (c_wglob c) && negb (whole v) then None else go r' fs1
                | None => None
                end
            | None => None
            end
          else if (40 <=? op) && (op <=? 62) then
            match uleb 5 r with
            | Some (al, r1) =>
                if al <? 64 then
                  match uleb 5 r1 with
                  | Some (o, r') =>
                      match width op with
                      | Some w =>
                          let e := off + (Z.of_nat (length bs) - Z.of_nat (length r')) in
                          if op <=? 53 then
                            match pop1 fs with
                            | Some (a, fs1) =>
                                match push1 (if (op =? 40) || (op =? 41) then KW 0 else KA) fs1 with
                                | Some fs' => kpass f r' e fs' ((e, a, o, w) :: accs) bulk
                                | None => None
                                end
                            | None => None
                            end
                          else
                            match pop1 fs with
                            | Some (_, fs1) =>
                                match pop1 fs1 with
                                | Some (a, fs') => kpass f r' e fs' ((e, a, o, w) :: accs) bulk
                                | None => None
                                end
                            | None => None
                            end
                      | None => None
                      end
                  | None => None
                  end
                else None
            | None => None
            end
          else if op =? 63 then
            match r with
            | _ :: r' => match push1 KA fs with Some fs' => go r' fs' | None => None end
            | [] => None
            end
          else if op =? 64 then
            match r with
            | _ :: r' =>
                match pop1 fs with
                | Some (_, fs1) => match push1 KA fs1 with Some fs' => go r' fs' | None => None end
                | None => None
                end
            | [] => None
            end
          else if op =? 65 then
            match sleb 5 r with
            | Some (z, r') => match push1 (KK (z mod M32)) fs with Some fs' => go r' fs' | None => None end
            | None => None
            end
          else if op =? 66 then
            match sleb 10 r with
            | Some (z, r') => match push1 (KK (z mod M64)) fs with Some fs' => go r' fs' | None => None end
            | None => None
            end
          else if op =? 67 then
            match r with
            | _ :: _ :: _ :: _ :: r' => match push1 KA fs with Some fs' => go r' fs' | None => None end
            | _ => None
            end
          else if op =? 68 then
            match r with
            | _ :: _ :: _ :: _ :: _ :: _ :: _ :: _ :: r' =>
                match push1 KA fs with Some fs' => go r' fs' | None => None end
            | _ => None
            end
          else if is_unary op then
            match pop1 fs with
            | Some (v, fs1) =>
                let v' := if is_keep op then match v with KW _ => v | _ => KA end else KA in
                match push1 v' fs1 with Some fs' => go r fs' | None => None end
            | None => None
            end
          else if is_binary op then
            match pop1 fs with
            | Some (b, fs1) =>
                match pop1 fs1 with
                | Some (a, fs2) =>
                    match push1 (if op =? 106 then kadd a b else KA) fs2 with
                    | Some fs' => go r fs'
                    | None => None
                    end
                | None => None
                end
            | None => None
            end
          else if op =? 252 then
            match uleb 5 r with
            | Some (sub, r1) =>
                if sub <=? 7 then
                  match pop1 fs with
                  | Some (_, fs1) => match push1 KA fs1 with Some fs' => go r1 fs' | None => None end
                  | None => None
                  end
                else if sub =? 10 then
                  match r1 with
                  | _ :: _ :: r' =>
                      match popn 3 fs with
                      | Some fs' => kpass f r' (off + (Z.of_nat (length bs) - Z.of_nat (length r'))) fs' accs true
                      | None => None
                      end
                  | _ => None
                  end
                else if sub =? 11 then
                  match r1 with
                  | _ :: r' =>
                      match popn 3 fs with
                      | Some fs' => kpass f r' (off + (Z.of_nat (length bs) - Z.of_nat (length r'))) fs' accs true
                      | None => None
                      end
                  | _ => None
                  end
                else None
            | None => None
            end
          else None
      end
  end.

End Pass.

(* The pass over a whole body: every access with the kind of its address, and
   whether the body holds a bulk memory instruction. `res` is how many values
   the function returns. *)
Definition kinded (c : ctx) (self : Z) (wl : list Z) (res : Z) (body : list Z)
  : option (list acc * bool) :=
  kpass c self wl (S (length body)) body 0 [mkF [] res 0 true false] [] false.

Definition is_header (a : acc) : bool :=
  match a with
  | (_, KW k, o, w) => (0 <=? k) && (k + o + w <=? 12)
  | _ => false
  end.

(* Where the header accesses of an accepted body end. *)
Definition header_ends (c : ctx) (self : Z) (wl : list Z) (res : Z) (body : list Z) : option (list Z) :=
  match kinded c self wl res body with
  | Some (accs, _) => Some (map (fun a => match a with (e, _, _, _) => e end) (filter is_header accs))
  | None => None
  end.

(* ── examples: a context with one function of one parameter and one result
      at index 0, the allocator at index 1 ── *)
Definition ex_ctx (wpar : list (Z * Z)) (wres : list Z) : ctx :=
  mkC [(1, 1); (1, 1)] [(1, 1)] 1 wpar wres [] [] [].

(* `local.get 0 ; i32.load offset=4 ; end` through a whole parameter: the load
   at offset 4 is a header access, and the word it reads is whole. *)
Example header_through_whole_parameter :
  header_ends (ex_ctx [(0, 0)] [0]) 0 [0] 1 [32; 0; 40; 2; 4; 11] = Some [5].
Proof. vm_compute. reflexivity. Qed.

(* The same body when the parameter is not claimed whole: accepted (the
   result, a loaded word, is whole), but the load is no header access. *)
Example no_header_through_arith_parameter :
  header_ends (ex_ctx [] [0]) 0 [] 1 [32; 0; 40; 2; 4; 11] = Some [].
Proof. vm_compute. reflexivity. Qed.

(* A cursor: `local.get 0 ; i32.const 12 ; i32.add ; local.set 1 ;
   local.get 1 ; i32.load8_u offset=0 ; end`. Local 1 claimed whole is refused
   (it is assigned a whole value plus twelve). *)
Example cursor_local_is_not_whole :
  kinded (ex_ctx [(0, 0)] []) 0 [0; 1] 1 [32; 0; 65; 12; 106; 33; 1; 32; 1; 45; 0; 0; 11] = None.
Proof. vm_compute. reflexivity. Qed.

(* Without that claim the body is accepted and its byte load is no header
   access. *)
Example cursor_read_is_no_header :
  header_ends (ex_ctx [(0, 0)] []) 0 [0] 1 [32; 0; 65; 12; 106; 33; 1; 32; 1; 45; 0; 0; 11] = Some [].
Proof. vm_compute. reflexivity. Qed.

(* A constant added in place keeps the base: `local.get 0 ; i32.const 4 ;
   i32.add ; i32.load offset=0` reads bytes 4 to 8 of the header; with the
   constant 12 it does not. *)
Example header_with_constant_added :
  header_ends (ex_ctx [(0, 0)] [0]) 0 [0] 1 [32; 0; 65; 4; 106; 40; 2; 0; 11] = Some [8].
Proof. vm_compute. reflexivity. Qed.

Example payload_with_constant_added :
  header_ends (ex_ctx [(0, 0)] [0]) 0 [0] 1 [32; 0; 65; 12; 106; 40; 2; 0; 11] = Some [].
Proof. vm_compute. reflexivity. Qed.

(* An argument computed by arithmetic at a parameter claimed whole is refused:
   `local.get 0 ; i32.const 12 ; i32.add ; call 0 ; end`. *)
Example arith_argument_at_whole_parameter :
  kinded (ex_ctx [(0, 0)] []) 0 [0] 1 [32; 0; 65; 12; 106; 16; 0; 11] = None.
Proof. vm_compute. reflexivity. Qed.

(* The allocator's result is whole: `i32.const 8 ; call 1 ; local.tee 1 ;
   i32.load offset=0 ; end`, local 1 claimed whole. *)
Example allocator_result_is_whole :
  header_ends (ex_ctx [] [0]) 0 [1] 1 [65; 8; 16; 1; 34; 1; 40; 2; 0; 11] = Some [9].
Proof. vm_compute. reflexivity. Qed.

(* A result claimed whole and a value returned that is not: `local.get 0 ;
   i32.const 1 ; i32.add ; end`. *)
Example arith_result_claimed_whole :
  kinded (ex_ctx [(0, 0)] [0]) 0 [0] 1 [32; 0; 65; 1; 106; 11] = None.
Proof. vm_compute. reflexivity. Qed.

(* The result of an `if` is not followed: `local.get 0 ; if (result i32)
   local.get 0 else local.get 0 end ; i32.load offset=0 ; end` is accepted when
   the function's result is not claimed, and the load is no header access. *)
Example if_result_is_not_whole :
  header_ends (ex_ctx [(0, 0)] []) 0 [0] 1 [32; 0; 4; 127; 32; 0; 5; 32; 0; 11; 40; 2; 0; 11] = Some [].
Proof. vm_compute. reflexivity. Qed.
