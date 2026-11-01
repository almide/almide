(* Almide trust spine — #4005 A4 items 4 and 5: the procedure certificates as
   BYTES, riding inside the existing `bounds` witness (no bundle format
   change).

   The structural leg writes ONE bounds witness per program, every
   function's stretches joined by `;`. A self-host body's procedure
   (RoutineProcs.v) travels in it as a stretch that begins with `P`.
   `check_pbounds_cert` judges every other stretch exactly as
   `check_rbounds_cert` does, and gathers the procedure stretches into one
   table, keyed by function id: it accepts when no id is declared twice and
   every procedure's body is accepted under its declared precondition, its
   calls proving their callees' (`procs_ok`). That a procedure with a
   non-empty precondition is called only from such bodies is held per build
   by almide-anchor-check, which locates every call instruction.

   Procedure stretch format (whitespace between tokens is ignored; N is one
   or more decimal digits):
     proc  ::= 'P' N '{' groups '|' known '|' counts '|' nonneg '|' pprog '}'
     groups ::= (N ':' N)*          slot : representative slot
     known  ::= (N ':' N)*          slot : stride
     counts ::= (N ',' N ',' N)*    count slot, parameter slot, stride
     nonneg ::= N*
     pprog ::= pitem*
     pitem ::= 'h' N ',' N ',' N            PHdr x o w
             | 'H' N ',' N ',' N            PHdrSto x o w
             | 'a' N ',' N ',' N ',' N      PAcc x s o w
             | 'w' N ',' N ',' N ',' N      PSto x s o w
             | 'n' N ',' N ',' N            PLen n x s
             | 'c' N ',' N ',' N            PCheck x n s, unsigned
             | 'C' N ',' N ',' N            PCheck x n s, signed
             | 'e' N ',' N ',' N            PExit x n s, unsigned
             | 'E' N ',' N ',' N            PExit x n s, signed
             | 's' N                        PSet x
             | 'k' N ',' N                  PConst x c (c >= 0)
             | 't' N ',' N ',' N ',' N      PStep x y c s
             | 'L' '{' pprog '}'            PLoop
             | 'B' '{' pprog '|' pprog '}'  PBranch
             | 'F' N '{' (N ':' N)* '}'     PCall f args (callee slot : caller slot)
   A string that does not parse is REJECTED. *)

From AlmideTrust Require Import MemorySafe Subset.
From AlmideTrust Require SpatialCert BoundsCert RoutineBounds RoutineBoundsCert RoutineProcs.
From Stdlib Require Import List String Ascii Arith Bool ZArith.
Import ListNotations.

Definition ch := BoundsCert.ch.
Definition is_space := BoundsCert.is_space.
Definition number := BoundsCert.number.
Definition nums := RoutineBoundsCert.nums.
Definition skip_space := RoutineBoundsCert.skip_space.
Definition zn := Z.of_nat.

(* `seq_at fuel item cs`: item* up to the first `|` or `}` (not consumed). *)
Fixpoint seq_at {A : Type} (fuel : nat) (item : list ascii -> option (A * list ascii)) (cs : list ascii)
  : option (list A * list ascii) :=
  match fuel with
  | O => None
  | S f =>
      match cs with
      | [] => None
      | c :: r =>
          if ch c "|"%char || ch c "}"%char then Some ([], cs)
          else if is_space c then seq_at f item r
          else
            match item cs with
            | Some (x, r1) =>
                match seq_at f item r1 with
                | Some (xs, r2) => Some (x :: xs, r2)
                | None => None
                end
            | None => None
            end
      end
  end.

(* `N ':' N`. *)
Definition pair_item (cs : list ascii) : option ((nat * nat) * list ascii) :=
  match number cs with
  | Some (x, col :: r1) =>
      if ch col ":"%char then
        match number r1 with
        | Some (y, r2) => Some ((x, y), r2)
        | None => None
        end
      else None
  | _ => None
  end.

(* `N ',' N ',' N`. *)
Definition triple_item (cs : list ascii) : option ((nat * nat * Z) * list ascii) :=
  match nums 3 cs with
  | Some ([a; b; c], r) => Some ((a, b, zn c), r)
  | _ => None
  end.

Definition num_item (cs : list ascii) : option (nat * list ascii) := number cs.

(* `c` then the rest. *)
Definition expect (c : ascii) (cs : list ascii) : option (list ascii) :=
  match skip_space cs with
  | d :: r => if ch d c then Some r else None
  | [] => None
  end.

Fixpoint pprog_at (fuel : nat) (cs : list ascii) : option (RoutineProcs.PProg * list ascii) :=
  match fuel with
  | O => None
  | S f =>
      match cs with
      | [] => Some (RoutineProcs.PEnd, [])
      | c :: r =>
          if ch c "}"%char || ch c "|"%char || ch c ";"%char then Some (RoutineProcs.PEnd, cs)
          else if is_space c then pprog_at f r
          else if ch c "h"%char || ch c "H"%char || ch c "n"%char || ch c "c"%char
                  || ch c "C"%char || ch c "e"%char || ch c "E"%char then
            match nums 3 r with
            | Some ([a; b; d], r1) =>
                match pprog_at f r1 with
                | Some (k, r2) =>
                    let i :=
                      if ch c "h"%char then RoutineProcs.PHdr a (zn b) (zn d) k
                      else if ch c "H"%char then RoutineProcs.PHdrSto a (zn b) (zn d) k
                      else if ch c "n"%char then RoutineProcs.PLen a b (zn d) k
                      else if ch c "c"%char then RoutineProcs.PCheck a b (zn d) false k
                      else if ch c "C"%char then RoutineProcs.PCheck a b (zn d) true k
                      else if ch c "e"%char then RoutineProcs.PExit a b (zn d) false k
                      else RoutineProcs.PExit a b (zn d) true k in
                    Some (i, r2)
                | None => None
                end
            | _ => None
            end
          else if ch c "a"%char || ch c "w"%char || ch c "t"%char then
            match nums 4 r with
            | Some ([x; s; o; w], r1) =>
                match pprog_at f r1 with
                | Some (k, r2) =>
                    Some ((if ch c "a"%char then RoutineProcs.PAcc x (zn s) (zn o) (zn w) k
                           else if ch c "w"%char then RoutineProcs.PSto x (zn s) (zn o) (zn w) k
                           else RoutineProcs.PStep x s (zn o) (zn w) k), r2)
                | None => None
                end
            | _ => None
            end
          else if ch c "k"%char then
            match nums 2 r with
            | Some ([x; v], r1) =>
                match pprog_at f r1 with
                | Some (k, r2) => Some (RoutineProcs.PConst x (zn v) k, r2)
                | None => None
                end
            | _ => None
            end
          else if ch c "s"%char then
            match number r with
            | Some (x, r1) =>
                match pprog_at f r1 with
                | Some (k, r2) => Some (RoutineProcs.PSet x k, r2)
                | None => None
                end
            | None => None
            end
          else if ch c "L"%char then
            match expect "{"%char r with
            | Some r0 =>
                match pprog_at f r0 with
                | Some (body, cl :: r1) =>
                    if ch cl "}"%char then
                      match pprog_at f r1 with
                      | Some (k, r2) => Some (RoutineProcs.PLoop body k, r2)
                      | None => None
                      end
                    else None
                | _ => None
                end
            | None => None
            end
          else if ch c "B"%char then
            match expect "{"%char r with
            | Some r0 =>
                match pprog_at f r0 with
                | Some (thn, bar :: r1) =>
                    if ch bar "|"%char then
                      match pprog_at f r1 with
                      | Some (els, cl :: r2) =>
                          if ch cl "}"%char then
                            match pprog_at f r2 with
                            | Some (k, r3) => Some (RoutineProcs.PBranch thn els k, r3)
                            | None => None
                            end
                          else None
                      | _ => None
                      end
                    else None
                | _ => None
                end
            | None => None
            end
          else if ch c "F"%char then
            match number r with
            | Some (g, r0) =>
                match expect "{"%char r0 with
                | Some r1 =>
                    match seq_at f pair_item r1 with
                    | Some (args, cl :: r2) =>
                        if ch cl "}"%char then
                          match pprog_at f r2 with
                          | Some (k, r3) => Some (RoutineProcs.PCall g args k, r3)
                          | None => None
                          end
                        else None
                    | _ => None
                    end
                | None => None
                end
            | None => None
            end
          else None
      end
  end.

Definition sep (c : ascii) (r : option (list ascii)) : option (list ascii) :=
  match r with
  | Some (d :: r') => if ch d c then Some r' else None
  | _ => None
  end.

(* One procedure after `P`: its id and its table entry. *)
Definition proc_at (cs : list ascii) : option ((nat * (RoutineProcs.Grp * RoutineProcs.Pre * RoutineProcs.PProg)) * list ascii) :=
  let fuel := S (List.length cs) in
  match number cs with
  | Some (fid, r0) =>
      match expect "{"%char r0 with
      | Some r1 =>
          match seq_at fuel pair_item r1 with
          | Some (gs, r2) =>
              match sep "|"%char (Some r2) with
              | Some r3 =>
                  match seq_at fuel pair_item r3 with
                  | Some (kq, r4) =>
                      match sep "|"%char (Some r4) with
                      | Some r5 =>
                          match seq_at fuel triple_item r5 with
                          | Some (cq, r6) =>
                              match sep "|"%char (Some r6) with
                              | Some r7 =>
                                  match seq_at fuel num_item r7 with
                                  | Some (nq, r8) =>
                                      match sep "|"%char (Some r8) with
                                      | Some r9 =>
                                          match pprog_at fuel r9 with
                                          | Some (body, cl :: r10) =>
                                              if ch cl "}"%char then
                                                Some ((fid, (gs, (map (fun q => (fst q, zn (snd q))) kq, cq, nq), body)), skip_space r10)
                                              else None
                                          | _ => None
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
                  | None => None
                  end
              | None => None
              end
          | None => None
          end
      | None => None
      end
  | None => None
  end.

(* A stretch of a bounds witness: a procedure after `P`, else as before. *)
Inductive PStretch :=
  | POld (x : RoutineBoundsCert.Stretch)
  | PProc (d : nat * (RoutineProcs.Grp * RoutineProcs.Pre * RoutineProcs.PProg)).

Definition pstretch_at (cs : list ascii) : option (PStretch * list ascii) :=
  match skip_space cs with
  | c :: r =>
      if ch c "P"%char then
        match proc_at r with
        | Some (d, r1) => Some (PProc d, r1)
        | None => None
        end
      else
        match RoutineBoundsCert.stretch_at cs with
        | Some (x, r1) => Some (POld x, r1)
        | None => None
        end
  | [] => Some (POld (RoutineBoundsCert.SList SpatialCert.PEnd), [])
  end.

Fixpoint pstretches_at (fuel : nat) (cs : list ascii) : option (list PStretch) :=
  match fuel with
  | O => None
  | S f =>
      match pstretch_at cs with
      | Some (p, []) => Some [p]
      | Some (p, sp :: r) =>
          if ch sp ";"%char then
            match pstretches_at f r with
            | Some ps => Some (p :: ps)
            | None => None
            end
          else None
      | None => None
      end
  end.

Definition parse_pstretches (s : string) : option (list PStretch) :=
  let cs := list_ascii_of_string s in pstretches_at (S (List.length cs)) cs.

Definition pstretch_ok (x : PStretch) : bool :=
  match x with
  | POld y => RoutineBoundsCert.stretch_ok y
  | PProc _ => true
  end.

Definition procs_of (ps : list PStretch) : RoutineProcs.Procs :=
  flat_map (fun x => match x with PProc d => [d] | POld _ => [] end) ps.

Fixpoint nodup_ids (P : RoutineProcs.Procs) : bool :=
  match P with
  | [] => true
  | (f, _) :: r => negb (existsb (fun q => Nat.eqb (fst q) f) r) && nodup_ids r
  end.

(* The `bounds` property's checker from A4 items 4–5 on. *)
Definition check_pbounds_cert (s : string) : bool :=
  match parse_pstretches s with
  | Some ps => forallb pstretch_ok ps && nodup_ids (procs_of ps) && RoutineProcs.procs_ok (procs_of ps)
  | None => false
  end.

(* ── Soundness ────────────────────────────────────────────────────────── *)

(* What ACCEPT means: every old stretch keeps its meaning (a list stretch
   Spatial by SpatialCert, a routine stretch by RoutineBounds), and in the
   table the witness declares, every procedure's every run from a state
   meeting its precondition — through any depth of calls and recursion —
   reads and writes only inside its blocks, with their representation. *)
Theorem check_pbounds_cert_sound : forall s,
  check_pbounds_cert s = true ->
  exists ps, parse_pstretches s = Some ps /    (forall x, In (POld x) ps -> RoutineBoundsCert.stretch_ok x = true) /    forall f gs kq cq nq body lb ok bm e t o,
      RoutineProcs.plook (procs_of ps) f = Some (gs, (kq, cq, nq), body) ->
      RoutineProcs.wf_lay lb -> (forall y, ok (bm y)) -> RoutineBounds.i64 e ->
      RoutineProcs.resp gs bm -> RoutineProcs.psound lb bm kq cq nq e ->
      RoutineProcs.prun (procs_of ps) lb ok bm e body t o ->
      Forall (RoutineProcs.pin lb ok) t.
Proof.
  intros s H. unfold check_pbounds_cert in H.
  destruct (parse_pstretches s) as [ps |] eqn:Hp; [| discriminate].
  rewrite !andb_true_iff in H. destruct H as [[Hold _] HP].
  exists ps. split; [reflexivity | split].
  - rewrite forallb_forall in Hold. intros x Hin. exact (Hold _ Hin).
  - intros f gs kq cq nq body lb ok bm e t o Hf Hwf Hok Hi Hr Hs Hrun.
    destruct (RoutineProcs.procs_ok_body _ f gs (kq, cq, nq) body HP Hf) as [rb Hrb]. simpl in Hrb.
    exact (proj1 (RoutineProcs.prun_sound _ lb ok bm e body t o Hrun HP Hwf Hok Hi gs kq cq nq rb Hr Hs Hrb)).
Qed.
Print Assumptions check_pbounds_cert_sound.

(* ── Controls ─────────────────────────────────────────────────────────── *)

(* The old stretches keep their verdicts. *)
Example old_stretches_unchanged :
  check_pbounds_cert "c0 a0 c1 a1 w0 w1" = true /\ check_pbounds_cert "c0 s0 a0" = false /\
  check_pbounds_cert "R n1,0,1 L{ e0,1,1 a0,1,0,1 s0 }" = true /\ check_pbounds_cert "R a0,1,0,1" = false.
Proof. repeat split; reflexivity. Qed.

(* RoutineProcs' recursive copy, as bytes: procedure 7, slot 3 in slot 2's
   group, count 1 of parameter 2's block at stride 8, slot 2 not negative;
   the body tests, reads, steps and calls itself. *)
Definition copy_text : string :=
  "P7{ 3:2 | | 1,2,8 | 2 | B{ | C2,1,8 a2,8,0,8 t3,2,1,8 F7{1:1 2:3} } }".

Example accepts_copy :
  check_pbounds_cert copy_text = true /\ check_pbounds_cert (String.append "c0 a0 ; " copy_text) = true.
Proof. split; reflexivity. Qed.

(* The same body without the not-negative precondition: the signed test is
   refused. *)
Example rejects_copy_unsigned_fact :
  check_pbounds_cert "P7{ 3:2 | | 1,2,8 | | B{ | C2,1,8 a2,8,0,8 t3,2,1,8 F7{1:1 2:3} } }" = false.
Proof. reflexivity. Qed.

(* A call of a procedure the witness does not declare: refused. *)
Example rejects_undeclared_callee : check_pbounds_cert "P7{ | | | | F8{} }" = false.
Proof. reflexivity. Qed.

(* One id declared twice: refused. *)
Example rejects_duplicate_id : check_pbounds_cert "P7{ | | | | } ; P7{ | | | | }" = false.
Proof. reflexivity. Qed.

(* A procedure stretch beside a refused old stretch: refused. *)
Example rejects_old_stretch_beside : check_pbounds_cert (String.append "R a0,1,0,1 ; " copy_text) = false.
Proof. reflexivity. Qed.

(* Malformed procedure bytes: refused per witness. *)
Example rejects_proc_garbage :
  check_pbounds_cert "P7{ 3:2 | | 1,2 | | }" = false /\ check_pbounds_cert "P{ | | | | }" = false /\
  check_pbounds_cert "P7{ | | | | B{ }" = false.
Proof. repeat split; reflexivity. Qed.
