(* Almide trust spine — #4005: the bounds certificate as BYTES.

   SpatialCert.v proves the checker `accepts` sound for SPATIAL over a
   certificate program. A build hands the verifier bytes, not a Coq term, so
   this file puts the parser INSIDE the proof, the way OwnershipChecker and
   CallModes do: `check_bounds_cert` takes the witness string, parses it and
   checks every program in it, and `check_bounds_cert_sound` says what an
   ACCEPT means — every program the string denotes is accepted, so each of
   its runs is Spatial and WellTyped (`accepts_spatial`). The driver only
   reads the file.

   Format (whitespace between tokens is ignored):

     cert  ::= prog (';' prog)*            one program per checked stretch
     prog  ::= item*
     item  ::= 'c' N                       PCheck N
             | 'a' N                       PAccess N   (a read)
             | 'w' N                       PStore N    (a write)
             | 's' N                       PSet N      (the slot's index changes)
             | 'e' N                       PExit N     (a loop's exit test, #4280)
             | 'L' '{' prog '}'            PLoop
             | 'B' '{' prog '|' prog '}'   PBranch
     N     ::= one or more decimal digits

   A string that does not parse is REJECTED, never read as a shorter
   certificate. What stays trusted is the recording discipline: that the
   emitter writes an `a` / `w` for every access whose check it elided, and an
   `s` for every change of a slot's index, exactly as the ownership witness
   trusts that its events are the RC instructions the bytes contain. *)

From AlmideTrust Require Import MemorySafe SpatialCert Subset.
From Stdlib Require Import List String Ascii Arith Bool.
Import ListNotations.

(* ── Parsing ──────────────────────────────────────────────────────────── *)

(* Characters are compared as characters (`Ascii.eqb`, extracted to OCaml's
   char compare), and digits reuse Subset.v's tokenizer, so the extracted
   checker gains no spelled-out Peano literals. *)
Definition ch (c d : ascii) : bool := Ascii.eqb c d.

Definition is_space (c : ascii) : bool :=
  ch c " "%char || ch c "010"%char || ch c "013"%char || ch c "009"%char.

(* Digits as a number, and what follows them. *)
Fixpoint digits (acc : nat) (cs : list ascii) : nat * list ascii :=
  match cs with
  | c :: r => if is_digit c then digits (acc * 10 + digit c) r else (acc, cs)
  | [] => (acc, [])
  end.

(* At least one digit, or no number. *)
Definition number (cs : list ascii) : option (nat * list ascii) :=
  match cs with
  | c :: _ => if is_digit c then Some (digits 0 cs) else None
  | [] => None
  end.

(* `prog_at fuel cs`: the program at the head of cs, and the rest from the
   first token that ends it (`}`, `|`, `;` or the end). Every recursive call
   follows at least one consumed character, so length + 1 fuel is enough. *)
Fixpoint prog_at (fuel : nat) (cs : list ascii) : option (Prog * list ascii) :=
  match fuel with
  | O => None
  | S f =>
      match cs with
      | [] => Some (PEnd, [])
      | c :: r =>
          if ch c "}"%char || is_bar c || is_semi c then Some (PEnd, cs)
          else if is_space c then prog_at f r
          else if ch c "c"%char || ch c "a"%char || ch c "w"%char || ch c "s"%char
                  || ch c "e"%char then
            match number r with
            | Some (n, r1) =>
                match prog_at f r1 with
                | Some (k, r2) =>
                    let i := if ch c "c"%char then PCheck n k
                             else if ch c "a"%char then PAccess n k
                             else if ch c "w"%char then PStore n k
                             else if ch c "e"%char then PExit n k
                             else PSet n k in
                    Some (i, r2)
                | None => None
                end
            | None => None
            end
          else if ch c "L"%char then                                        (* L { body } k *)
            match r with
            | o :: r0 =>
                if ch o "{"%char then
                  match prog_at f r0 with
                  | Some (body, cl :: r1) =>
                      if ch cl "}"%char then
                        match prog_at f r1 with
                        | Some (k, r2) => Some (PLoop body k, r2)
                        | None => None
                        end
                      else None
                  | _ => None
                  end
                else None
            | [] => None
            end
          else if ch c "B"%char then                                        (* B { t | e } k *)
            match r with
            | o :: r0 =>
                if ch o "{"%char then
                  match prog_at f r0 with
                  | Some (thn, bar :: r1) =>
                      if is_bar bar then
                        match prog_at f r1 with
                        | Some (els, cl :: r2) =>
                            if ch cl "}"%char then
                              match prog_at f r2 with
                              | Some (k, r3) => Some (PBranch thn els k, r3)
                              | None => None
                              end
                            else None
                        | _ => None
                        end
                      else None
                  | _ => None
                  end
                else None
            | [] => None
            end
          else None
      end
  end.

(* The programs separated by `;`, to the end of the string. *)
Fixpoint progs_at (fuel : nat) (cs : list ascii) : option (list Prog) :=
  match fuel with
  | O => None
  | S f =>
      match prog_at (S (List.length cs)) cs with
      | Some (p, []) => Some [p]
      | Some (p, sep :: r) =>
          if is_semi sep then
            match progs_at f r with
            | Some ps => Some (p :: ps)
            | None => None
            end
          else None
      | None => None
      end
  end.

Definition parse_bounds (s : string) : option (list Prog) :=
  let cs := list_ascii_of_string s in progs_at (S (List.length cs)) cs.

(* ── The checker the driver runs ──────────────────────────────────────── *)

Definition check_bounds_cert (s : string) : bool :=
  match parse_bounds s with
  | Some ps => forallb accepts ps
  | None => false
  end.

(* What ACCEPT means: the string denotes programs, and every run of each —
   over any blocks the program allocates, any i64 index values — is Spatial
   and WellTyped. *)
Theorem check_bounds_cert_sound : forall s,
  check_bounds_cert s = true ->
  exists ps, parse_bounds s = Some ps /\
    forall p, In p ps ->
    forall m bs e t o,
      wf_mem m -> (forall x, In (B m x) bs) -> i64 e -> run m e p t o ->
      Spatial (program_trace m bs t) /\ WellTyped (program_trace m bs t).
Proof.
  intros s H. unfold check_bounds_cert in H.
  destruct (parse_bounds s) as [ps |] eqn:Hp; [| discriminate].
  exists ps. split; [reflexivity |].
  intros p Hin m bs e t o Hwf Hbs Hi Hrun.
  rewrite forallb_forall in H.
  exact (accepts_spatial m bs e p t o Hwf Hbs (H p Hin) Hi Hrun).
Qed.
Print Assumptions check_bounds_cert_sound.

(* ── Controls: the bytes, end to end ──────────────────────────────────── *)

(* The fannkuch flip (#3345): check two slots, read both, write both. *)
Example accepts_flip : check_bounds_cert "c0 a0 c1 a1 w0 w1" = true.
Proof. reflexivity. Qed.

(* Two stretches, a loop that re-checks, and a branch whose arms both check. *)
Example accepts_structured :
  check_bounds_cert "L{ s0 c0 a0 } ; B{ c3 | c3 a3 } a3" = true.
Proof. reflexivity. Qed.

(* The parse is what the checker sees. *)
Example parses_flip :
  parse_bounds "c0 a0 w0" = Some [PCheck 0 (PAccess 0 (PStore 0 PEnd))].
Proof. reflexivity. Qed.

(* An elided check after the index changed. *)
Example rejects_stale : check_bounds_cert "c0 s0 a0" = false.
Proof. reflexivity. Qed.

(* Knowledge does not cross `;`: each stretch starts from nothing. *)
Example rejects_across_stretches : check_bounds_cert "c0 ; a0" = false.
Proof. reflexivity. Qed.

(* One arm checks, the read after the join relies on it. *)
Example rejects_one_arm : check_bounds_cert "B{ c0 | } a0" = false.
Proof. reflexivity. Qed.

(* Malformed bytes are REJECTED, not read as a prefix. *)
Example rejects_garbage_tail : check_bounds_cert "c0 a0 x" = false.
Proof. reflexivity. Qed.

Example rejects_missing_number : check_bounds_cert "c a0" = false.
Proof. reflexivity. Qed.

Example rejects_unclosed_loop : check_bounds_cert "L{ c0 a0" = false.
Proof. reflexivity. Qed.

Example rejects_stray_close : check_bounds_cert "c0 a0 }" = false.
Proof. reflexivity. Qed.

(* The edges crates/almide-verify/src/bounds.rs pins too: slots compare as
   numbers, tokens need no space between them, the empty witness and a
   trailing `;` are vacuous, `L` must touch its `{`, and a `|` outside a
   branch is junk. *)
Example slots_are_numbers : check_bounds_cert "c007 a7" = true.
Proof. reflexivity. Qed.

Example tokens_need_no_space : check_bounds_cert "c0a0" = true.
Proof. reflexivity. Qed.

Example empty_is_vacuous : check_bounds_cert "" = true /\ check_bounds_cert ";" = true.
Proof. split; reflexivity. Qed.

Example rejects_detached_brace : check_bounds_cert "L {c0}" = false.
Proof. reflexivity. Qed.

Example rejects_stray_bar : check_bounds_cert "c0 | a0" = false.
Proof. reflexivity. Qed.

(* ── Controls: what the structural emitter writes (#4005) ─────────────── *)

(* Every index-operator access is recorded; one that is no fact candidate
   gets a fresh slot in the pass's loose stretch, decided by its own check.
   The flip probe's whole witness (crates/almide-wasm/tests/witness_bounds.rs):
   the loop's stretch, then the two reads of the `println` outside it. *)
Example accepts_flip_with_loose_reads :
  check_bounds_cert "c0 a0 c1 a1 w0 w1 s0 s1;c0 a0 c1 a1" = true.
Proof. reflexivity. Qed.

(* The three witnesses scripts/bounds-witness-reinject.sh gets from an
   emitter with one bounds check lost, pinned in proofs/poisoned-certs/bounds/:
   the read route's check dropped, a #3345 fact kept after its index moved,
   and the store route's check dropped. *)
Example rejects_drill_read_check_dropped : check_bounds_cert "a0" = false.
Proof. reflexivity. Qed.

Example rejects_drill_stale_fact : check_bounds_cert "c0 a0 s0 a0" = false.
Proof. reflexivity. Qed.

Example rejects_drill_store_check_dropped : check_bounds_cert "w0 w1 c2 a2" = false.
Proof. reflexivity. Qed.

(* #4280: a `for` loop over a list records its own stretch, the exit test
   then the element read (`e0 a0`), anchored at the `br_if` and the load. *)
Example parses_exit : parse_bounds "e0 a0" = Some [PExit 0 (PAccess 0 PEnd)].
Proof. reflexivity. Qed.

Example accepts_for_header : check_bounds_cert "c0 a0;e0 a0;e0 a0 c1 a1" = true.
Proof. reflexivity. Qed.

Example accepts_for_loop_bytes : check_bounds_cert "L{ e0 a0 s0 }" = true.
Proof. reflexivity. Qed.

(* The drill scripts/witness-reinject.sh bounds gets from an emitter that
   reads the element before the loop's exit test (proofs/poisoned-certs/
   bounds/4005-loop-exit-after-read.cert), and the other orders the checker
   must refuse. *)
Example rejects_drill_exit_after_read : check_bounds_cert "a0 e0" = false.
Proof. reflexivity. Qed.

Example rejects_exit_misplaced : check_bounds_cert "e0 L{ a0 s0 }" = false /\
  check_bounds_cert "L{ e0 s0 a0 }" = false /\ check_bounds_cert "e1 a0" = false /\
  check_bounds_cert "e a0" = false.
Proof. repeat split; reflexivity. Qed.
