(* Almide trust spine — #4005 A4: the routine bounds certificate as BYTES,
   riding inside the existing `bounds` witness (no bundle format change).

   BoundsCert.v parses the bounds witness — stretches of SpatialCert
   programs joined by `;` — and proves an ACCEPT sound. A4's routine
   stretches (RoutineBounds.v) travel in the same witness: a stretch that
   begins with `R` is a routine program, every other stretch is parsed and
   judged exactly as before, so a witness with no `R` stretch gets the
   verdict `check_bounds_cert` gives it (`no_routine_stretch_same_verdict`).

   Routine stretch format (whitespace between tokens is ignored; N is one or
   more decimal digits; offsets, widths and strides are non-negative):
     rprog ::= ritem*
     ritem ::= 'h' N ',' N ',' N             QHdr x o w      header read
             | 'H' N ',' N ',' N             QHdrSto x o w   header write
             | 'a' N ',' N ',' N ',' N       QAcc x s o w    slot read
             | 'w' N ',' N ',' N ',' N       QSto x s o w    slot write
             | 'n' N ',' N ',' N             QLen n x s      a count of x's block
             | 'c' N ',' N ',' N             QCheck x n s    the routine's test
             | 'e' N ',' N ',' N             QExit x n s     a loop's exit test
             | 's' N                         QSet x
             | 'L' '{' rprog '}'             QLoop
             | 'B' '{' rprog '|' rprog '}'   QBranch
             | 'C' '{' pairs '|' pairs '|' rprog '}'
                                             QCall req ens body
     pairs ::= (N ':' N)*                    (slot : stride) facts
   A string that does not parse is REJECTED. *)

From AlmideTrust Require Import MemorySafe Subset.
From AlmideTrust Require SpatialCert BoundsCert RoutineBounds.
From Stdlib Require Import List String Ascii Arith Bool ZArith.
Import ListNotations.

Definition ch := BoundsCert.ch.
Definition is_space := BoundsCert.is_space.
Definition number := BoundsCert.number.

(* `nums k cs`: k numbers separated by `,`, and what follows. *)
Fixpoint nums (k : nat) (cs : list ascii) : option (list nat * list ascii) :=
  match k with
  | O => Some ([], cs)
  | S O =>
      match number cs with
      | Some (n, r) => Some ([n], r)
      | None => None
      end
  | S k' =>
      match number cs with
      | Some (n, c :: r) =>
          if ch c ","%char then
            match nums k' r with
            | Some (ns, r2) => Some (n :: ns, r2)
            | None => None
            end
          else None
      | _ => None
      end
  end.

(* `pairs_at fuel cs`: (N ':' N)* up to the first `|`. *)
Fixpoint pairs_at (fuel : nat) (cs : list ascii) : option (list (nat * Z) * list ascii) :=
  match fuel with
  | O => None
  | S f =>
      match cs with
      | [] => None
      | c :: r =>
          if ch c "|"%char then Some ([], cs)
          else if is_space c then pairs_at f r
          else
            match number cs with
            | Some (x, col :: r1) =>
                if ch col ":"%char then
                  match number r1 with
                  | Some (s, r2) =>
                      match pairs_at f r2 with
                      | Some (ps, r3) => Some ((x, Z.of_nat s) :: ps, r3)
                      | None => None
                      end
                  | None => None
                  end
                else None
            | _ => None
            end
      end
  end.

Definition zn := Z.of_nat.

Fixpoint rprog_at (fuel : nat) (cs : list ascii) : option (RoutineBounds.Prog * list ascii) :=
  match fuel with
  | O => None
  | S f =>
      match cs with
      | [] => Some (RoutineBounds.QEnd, [])
      | c :: r =>
          if ch c "}"%char || ch c "|"%char || ch c ";"%char then Some (RoutineBounds.QEnd, cs)
          else if is_space c then rprog_at f r
          else if ch c "h"%char || ch c "H"%char || ch c "n"%char || ch c "c"%char || ch c "e"%char then
            match nums 3 r with
            | Some ([a; b; d], r1) =>
                match rprog_at f r1 with
                | Some (k, r2) =>
                    let i :=
                      if ch c "h"%char then RoutineBounds.QHdr a (zn b) (zn d) k
                      else if ch c "H"%char then RoutineBounds.QHdrSto a (zn b) (zn d) k
                      else if ch c "n"%char then RoutineBounds.QLen a b (zn d) k
                      else if ch c "c"%char then RoutineBounds.QCheck a b (zn d) k
                      else RoutineBounds.QExit a b (zn d) k in
                    Some (i, r2)
                | None => None
                end
            | _ => None
            end
          else if ch c "a"%char || ch c "w"%char then
            match nums 4 r with
            | Some ([x; s; o; w], r1) =>
                match rprog_at f r1 with
                | Some (k, r2) =>
                    Some ((if ch c "a"%char then RoutineBounds.QAcc x (zn s) (zn o) (zn w) k
                           else RoutineBounds.QSto x (zn s) (zn o) (zn w) k), r2)
                | None => None
                end
            | _ => None
            end
          else if ch c "s"%char then
            match number r with
            | Some (x, r1) =>
                match rprog_at f r1 with
                | Some (k, r2) => Some (RoutineBounds.QSet x k, r2)
                | None => None
                end
            | None => None
            end
          else if ch c "L"%char then
            match r with
            | o :: r0 =>
                if ch o "{"%char then
                  match rprog_at f r0 with
                  | Some (body, cl :: r1) =>
                      if ch cl "}"%char then
                        match rprog_at f r1 with
                        | Some (k, r2) => Some (RoutineBounds.QLoop body k, r2)
                        | None => None
                        end
                      else None
                  | _ => None
                  end
                else None
            | [] => None
            end
          else if ch c "B"%char then
            match r with
            | o :: r0 =>
                if ch o "{"%char then
                  match rprog_at f r0 with
                  | Some (thn, bar :: r1) =>
                      if ch bar "|"%char then
                        match rprog_at f r1 with
                        | Some (els, cl :: r2) =>
                            if ch cl "}"%char then
                              match rprog_at f r2 with
                              | Some (k, r3) => Some (RoutineBounds.QBranch thn els k, r3)
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
          else if ch c "C"%char then
            match r with
            | o :: r0 =>
                if ch o "{"%char then
                  match pairs_at f r0 with
                  | Some (req, b1 :: r1) =>
                      if ch b1 "|"%char then
                        match pairs_at f r1 with
                        | Some (ens, b2 :: r2) =>
                            if ch b2 "|"%char then
                              match rprog_at f r2 with
                              | Some (body, cl :: r3) =>
                                  if ch cl "}"%char then
                                    match rprog_at f r3 with
                                    | Some (k, r4) => Some (RoutineBounds.QCall req ens body k, r4)
                                    | None => None
                                    end
                                  else None
                              | _ => None
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

(* A stretch: a routine program after `R`, else a list-element program. *)
Inductive Stretch := SList (p : SpatialCert.Prog) | SRoutine (p : RoutineBounds.Prog).

Fixpoint skip_space (cs : list ascii) : list ascii :=
  match cs with
  | c :: r => if is_space c then skip_space r else cs
  | [] => []
  end.

Definition stretch_at (cs : list ascii) : option (Stretch * list ascii) :=
  match skip_space cs with
  | c :: r =>
      if ch c "R"%char then
        match rprog_at (S (List.length r)) r with
        | Some (p, r1) => Some (SRoutine p, r1)
        | None => None
        end
      else
        match BoundsCert.prog_at (S (List.length cs)) cs with
        | Some (p, r1) => Some (SList p, r1)
        | None => None
        end
  | [] => Some (SList SpatialCert.PEnd, [])
  end.

Fixpoint stretches_at (fuel : nat) (cs : list ascii) : option (list Stretch) :=
  match fuel with
  | O => None
  | S f =>
      match stretch_at cs with
      | Some (p, []) => Some [p]
      | Some (p, sep :: r) =>
          if ch sep ";"%char then
            match stretches_at f r with
            | Some ps => Some (p :: ps)
            | None => None
            end
          else None
      | None => None
      end
  end.

Definition parse_stretches (s : string) : option (list Stretch) :=
  let cs := list_ascii_of_string s in stretches_at (S (List.length cs)) cs.

Definition stretch_ok (x : Stretch) : bool :=
  match x with
  | SList p => SpatialCert.accepts p
  | SRoutine p => RoutineBounds.accepts p
  end.

(* The `bounds` property's checker from A4 on. *)
Definition check_rbounds_cert (s : string) : bool :=
  match parse_stretches s with
  | Some ps => forallb stretch_ok ps
  | None => false
  end.

(* What ACCEPT means: every stretch the string denotes is Spatial and
   WellTyped on every run — a list stretch by SpatialCert, a routine stretch
   by RoutineBounds. *)
Theorem check_rbounds_cert_sound : forall s,
  check_rbounds_cert s = true ->
  exists ps, parse_stretches s = Some ps /\
    (forall p, In (SList p) ps ->
       forall m bs e t o,
         SpatialCert.wf_mem m -> (forall x, In (SpatialCert.B m x) bs) -> SpatialCert.i64 e ->
         SpatialCert.run m e p t o ->
         Spatial (SpatialCert.program_trace m bs t) /\ WellTyped (SpatialCert.program_trace m bs t)) /\
    (forall p, In (SRoutine p) ps ->
       forall m bs e t o,
         RoutineBounds.wf_mem m -> (forall x, In (RoutineBounds.B m x) bs) -> RoutineBounds.i64 e ->
         RoutineBounds.run m e p t o ->
         Spatial (RoutineBounds.program_trace m bs t) /\ WellTyped (RoutineBounds.program_trace m bs t)).
Proof.
  intros s H. unfold check_rbounds_cert in H.
  destruct (parse_stretches s) as [ps |] eqn:Hp; [| discriminate].
  exists ps. rewrite forallb_forall in H. split; [reflexivity | split].
  - intros p Hin m bs e t o Hwf Hbs Hi Hrun.
    exact (SpatialCert.accepts_spatial m bs e p t o Hwf Hbs (H _ Hin) Hi Hrun).
  - intros p Hin m bs e t o Hwf Hbs Hi Hrun.
    exact (RoutineBounds.routine_accepts_spatial m bs e p t o Hwf Hbs (H _ Hin) Hi Hrun).
Qed.
Print Assumptions check_rbounds_cert_sound.

(* ── Controls ─────────────────────────────────────────────────────────── *)

(* The list stretches keep their verdicts. *)
Example list_stretches_unchanged :
  check_rbounds_cert "c0 a0 c1 a1 w0 w1" = true /\ check_rbounds_cert "c0 s0 a0" = false /\
  check_rbounds_cert "L{ e0 a0 s0 }" = true /\ check_rbounds_cert "a0 e0" = false.
Proof. repeat split; reflexivity. Qed.

(* A byte loop in a routine. *)
Example accepts_routine_byte_loop :
  check_rbounds_cert "R n1,0,1 L{ e0,1,1 a0,1,0,1 s0 }" = true.
Proof. reflexivity. Qed.

(* A header read with no test, beside a list stretch. *)
Example accepts_header_and_list : check_rbounds_cert "R h0,4,4 ; c0 a0" = true.
Proof. reflexivity. Qed.

(* A scan contract, the entry's value read after the call. *)
Example accepts_scan_contract :
  check_rbounds_cert "R C{ | 2:8 | n1,2,8 c2,1,8 } a2,8,4,4" = true.
Proof. reflexivity. Qed.

(* Refused: a routine read with no test; a header read past the header; a
   contract the body does not keep; malformed bytes. *)
Example rejects_routine_unchecked : check_rbounds_cert "R a0,1,0,1" = false.
Proof. reflexivity. Qed.

Example rejects_header_past : check_rbounds_cert "R h0,8,8" = false.
Proof. reflexivity. Qed.

Example rejects_contract_lie : check_rbounds_cert "R C{ | 3:8 | n1,2,8 c2,1,8 }" = false.
Proof. reflexivity. Qed.

Example rejects_routine_garbage :
  check_rbounds_cert "R a0,1,0" = false /\ check_rbounds_cert "R x" = false /\
  check_rbounds_cert "R L{ s0" = false.
Proof. repeat split; reflexivity. Qed.
