(* Almide trust spine — #4005 A4: the linear-facts procedure certificates
   (RoutineLin.v) as BYTES, riding inside the existing `bounds` witness (no
   bundle format change).

   A stretch that begins with `Q` is one RoutineLin procedure; every other
   stretch is judged exactly as `check_pbounds_cert` judges it (RoutineProcs
   `P` stretches and the older ones). `check_lbounds_cert` gathers the `Q`
   stretches into one table keyed by function id: it accepts when no id is
   declared twice and every procedure's body is accepted under its declared
   precondition (`RoutineLin.procs_ok`). That a procedure with a non-empty
   precondition is called only from such bodies, and that each `Q` body is
   the translation of the function the shipped bytes contain, is held per
   build by almide-anchor-check.

   Stretch format. Whitespace and `,` between tokens are ignored; N is
   decimal digits, Z an optional `-` then decimal digits; a sequence ends at
   the first `|`, `}`, `]` or `)`.
     proc  ::= 'Q' N '{' facts '|' facts '|' N '|' lprog '}'   pre | post | result slot | body
     facts ::= lin*
     lin   ::= '[' (V N ':' Z)* ']' Z        Σ a·v + c, V one of s (slot), b (base), c (cap)
     pf    ::= '(' (N ':' Z)* ')' Z          (fact index : multiplier)*, scale
     pfs   ::= pf*
     lprog ::= item*
     item  ::= 'A' N N Z lin pf pf pf        LAcc r w off addr p0 p1 p2  (N for r, Z for w off)
             | 'W' N Z Z lin pf pf pf        LSto
             | 'h' N N                       LHandle h r
             | 'n' N N Z                     LLen n r s
             | 'a' N N lin pf pf             LAlloc h r sz lo hi
             | 'f' N                         LWf r
             | 'g' N Z Z                     LStatic r b c
             | 's' N                         LSet x
             | 'r' N Z Z                     LSetR x lo hi
             | '=' N lin pf pf               LAsn x l lo hi
             | 'T' ('+' | 'u') lin lin pf pf pf pf pf pf '{' lprog '|' lprog '}'
                                             LTest a b signed? (a-proofs) (b-proofs) (nonneg proofs) thn els
             | 'B' '{' lprog '|' lprog '}'   LBranch
             | 'L' '{' facts '|' pfs '|' lprog '}'
                                             LLoop inv pin body
             | 'c' '{' pfs '}'               LCont      (last in its sequence)
             | 'k'                           LBrk       (last)
             | 'x'                           LTrap      (last)
             | 'C' N '{' arg* '|' map* '|' map* '|' N '|' pfs '}'
                                             LCall f args reps outs res pfs
             | 'R' '{' pfs '}'               LRet       (last)
     arg   ::= N ':' lin ('!' pf pf)?
     map   ::= N ':' N
   A string that does not parse is REJECTED. *)

From AlmideTrust Require Import MemorySafe RoutineBounds RoutineLin.
From AlmideTrust Require Subset BoundsCert RoutineBoundsCert RoutineProcs RoutineProcsCert.
From Stdlib Require Import List String Ascii Arith Bool ZArith.
Import ListNotations.
Open Scope Z_scope.

Definition ch := BoundsCert.ch.
Definition is_space := BoundsCert.is_space.
Definition is_digit := Subset.is_digit.

Notation "'let*' ( x , r ) := a 'in' b" :=
  (match a with Some (x, r) => b | None => None end)
  (at level 10, x name, r name, a at level 100, b at level 200).

Fixpoint sk (cs : list ascii) : list ascii :=
  match cs with
  | c :: r => if is_space c || ch c ","%char then sk r else cs
  | [] => []
  end.

(* The next token is `c`. *)
Definition tok (c : ascii) (cs : list ascii) : option (unit * list ascii) :=
  match sk cs with
  | d :: r => if ch d c then Some (tt, r) else None
  | [] => None
  end.

Definition at_stop (cs : list ascii) : bool :=
  match sk cs with
  | d :: _ => ch d "|"%char || ch d "}"%char || ch d "]"%char || ch d ")"%char
  | [] => true
  end.

Fixpoint zdigits (acc : Z) (cs : list ascii) : Z * list ascii :=
  match cs with
  | c :: r => if is_digit c then zdigits (acc * 10 + Z.of_nat (Subset.digit c)) r else (acc, cs)
  | [] => (acc, [])
  end.

Definition unum (cs : list ascii) : option (Z * list ascii) :=
  match cs with
  | c :: _ => if is_digit c then Some (zdigits 0 cs) else None
  | [] => None
  end.

Definition nat_at (cs : list ascii) : option (nat * list ascii) :=
  let* (z, r) := unum (sk cs) in Some (Z.to_nat z, r).

Definition z_at (cs : list ascii) : option (Z * list ascii) :=
  match sk cs with
  | d :: r => if ch d "-"%char then let* (z, r1) := unum r in Some (- z, r1) else unum (d :: r)
  | [] => None
  end.

(* item* up to a stop. *)
Fixpoint many {A : Type} (fuel : nat) (item : list ascii -> option (A * list ascii)) (cs : list ascii)
  : option (list A * list ascii) :=
  match fuel with
  | O => None
  | S f =>
      if at_stop cs then Some ([], sk cs)
      else let* (x, r) := item cs in let* (xs, r1) := many f item r in Some (x :: xs, r1)
  end.

Definition term_at (cs : list ascii) : option ((Var * Z) * list ascii) :=
  match sk cs with
  | d :: r =>
      let* (n, r1) := nat_at r in
      let* (_u, r2) := tok ":"%char r1 in
      let* (a, r3) := z_at r2 in
      if ch d "s"%char then Some ((VS n, a), r3)
      else if ch d "b"%char then Some ((VB n, a), r3)
      else if ch d "c"%char then Some ((VC n, a), r3)
      else None
  | [] => None
  end.

Definition lin_at (fuel : nat) (cs : list ascii) : option (Lin * list ascii) :=
  let* (_u, r) := tok "["%char cs in
  let* (ts, r1) := many fuel term_at r in
  let* (_v, r2) := tok "]"%char r1 in
  let* (c, r3) := z_at r2 in Some (mkL ts c, r3).

Definition mult_at (cs : list ascii) : option ((nat * Z) * list ascii) :=
  let* (j, r) := nat_at cs in let* (_u, r1) := tok ":"%char r in let* (k, r2) := z_at r1 in Some ((j, k), r2).

Definition pf_at (fuel : nat) (cs : list ascii) : option (Pf * list ascii) :=
  let* (_u, r) := tok "("%char cs in
  let* (ms, r1) := many fuel mult_at r in
  let* (_v, r2) := tok ")"%char r1 in
  let* (mu, r3) := z_at r2 in Some ((ms, mu), r3).

Definition pf2_at (fuel : nat) (cs : list ascii) : option ((Pf * Pf) * list ascii) :=
  let* (a, r) := pf_at fuel cs in let* (b, r1) := pf_at fuel r in Some ((a, b), r1).

Definition map_at (cs : list ascii) : option ((Slot * Slot) * list ascii) :=
  let* (a, r) := nat_at cs in let* (_u, r1) := tok ":"%char r in let* (b, r2) := nat_at r1 in Some ((a, b), r2).

Definition arg_at (fuel : nat) (cs : list ascii) : option ((Slot * (Lin * option (Pf * Pf))) * list ascii) :=
  let* (p, r) := nat_at cs in
  let* (_u, r1) := tok ":"%char r in
  let* (l, r2) := lin_at fuel r1 in
  match tok "!"%char r2 with
  | Some (_, r3) => let* (pp, r4) := pf2_at fuel r3 in Some ((p, (l, Some pp)), r4)
  | None => Some ((p, (l, None)), r2)
  end.

Definition is_end (p : LProg) : bool := match p with LEnd => true | _ => false end.

Fixpoint lprog_at (fuel : nat) (cs : list ascii) : option (LProg * list ascii) :=
  match fuel with
  | O => None
  | S f =>
      if at_stop cs then Some (LEnd, sk cs) else
      match sk cs with
      | [] => Some (LEnd, [])
      | c :: r =>
          (* A terminal item: nothing may follow it in its sequence. *)
          let last (i : LProg) (r1 : list ascii) :=
            let* (k, r2) := lprog_at f r1 in if is_end k then Some (i, r2) else None in
          if ch c "A"%char || ch c "W"%char then
            let* (rg, r1) := nat_at r in let* (w, r2) := z_at r1 in let* (off, r3) := z_at r2 in
            let* (addr, r4) := lin_at f r3 in
            let* (p0, r5) := pf_at f r4 in let* (p1, r6) := pf_at f r5 in let* (p2, r7) := pf_at f r6 in
            let* (k, r8) := lprog_at f r7 in
            Some ((if ch c "A"%char then LAcc rg w off addr p0 p1 p2 k else LSto rg w off addr p0 p1 p2 k), r8)
          else if ch c "h"%char then
            let* (h, r1) := nat_at r in let* (rg, r2) := nat_at r1 in
            let* (k, r3) := lprog_at f r2 in Some (LHandle h rg k, r3)
          else if ch c "n"%char then
            let* (n, r1) := nat_at r in let* (rg, r2) := nat_at r1 in let* (s, r3) := z_at r2 in
            let* (k, r4) := lprog_at f r3 in Some (LLen n rg s k, r4)
          else if ch c "a"%char then
            let* (h, r1) := nat_at r in let* (rg, r2) := nat_at r1 in let* (sz, r3) := lin_at f r2 in
            let* (lo, r4) := pf_at f r3 in let* (hi, r5) := pf_at f r4 in
            let* (k, r6) := lprog_at f r5 in Some (LAlloc h rg sz lo hi k, r6)
          else if ch c "f"%char then
            let* (rg, r1) := nat_at r in let* (k, r2) := lprog_at f r1 in Some (LWf rg k, r2)
          else if ch c "g"%char then
            let* (rg, r1) := nat_at r in let* (b0, r2) := z_at r1 in let* (c0, r3) := z_at r2 in
            let* (k, r4) := lprog_at f r3 in Some (LStatic rg b0 c0 k, r4)
          else if ch c "s"%char then
            let* (x, r1) := nat_at r in let* (k, r2) := lprog_at f r1 in Some (LSet x k, r2)
          else if ch c "r"%char then
            let* (x, r1) := nat_at r in let* (lo, r2) := z_at r1 in let* (hi, r3) := z_at r2 in
            let* (k, r4) := lprog_at f r3 in Some (LSetR x lo hi k, r4)
          else if ch c "="%char then
            let* (x, r1) := nat_at r in let* (l, r2) := lin_at f r1 in
            let* (lo, r3) := pf_at f r2 in let* (hi, r4) := pf_at f r3 in
            let* (k, r5) := lprog_at f r4 in Some (LAsn x l lo hi k, r5)
          else if ch c "T"%char then
            match sk r with
            | sg :: r0 =>
                if ch sg "+"%char || ch sg "u"%char then
                  let* (a, r1) := lin_at f r0 in let* (b, r2) := lin_at f r1 in
                  let* (pa, r3) := pf2_at f r2 in let* (pb, r4) := pf2_at f r3 in let* (nn, r5) := pf2_at f r4 in
                  let* (_u, r6) := tok "{"%char r5 in
                  let* (thn, r7) := lprog_at f r6 in let* (_v, r8) := tok "|"%char r7 in
                  let* (els, r9) := lprog_at f r8 in let* (_w, r10) := tok "}"%char r9 in
                  let* (k, r11) := lprog_at f r10 in
                  Some (LTest a b (ch sg "+"%char) pa pb nn thn els k, r11)
                else None
            | [] => None
            end
          else if ch c "B"%char then
            let* (_u, r1) := tok "{"%char r in
            let* (thn, r2) := lprog_at f r1 in let* (_v, r3) := tok "|"%char r2 in
            let* (els, r4) := lprog_at f r3 in let* (_w, r5) := tok "}"%char r4 in
            let* (k, r6) := lprog_at f r5 in Some (LBranch thn els k, r6)
          else if ch c "L"%char then
            let* (_u, r1) := tok "{"%char r in
            let* (inv, r2) := many f (lin_at f) r1 in let* (_v, r3) := tok "|"%char r2 in
            let* (pin, r4) := many f (pf_at f) r3 in let* (_w, r5) := tok "|"%char r4 in
            let* (b, r6) := lprog_at f r5 in let* (_x, r7) := tok "}"%char r6 in
            let* (k, r8) := lprog_at f r7 in Some (LLoop inv pin b k, r8)
          else if ch c "c"%char then
            let* (_u, r1) := tok "{"%char r in let* (pfs, r2) := many f (pf_at f) r1 in
            let* (_v, r3) := tok "}"%char r2 in last (LCont pfs) r3
          else if ch c "k"%char then last LBrk r
          else if ch c "x"%char then last LTrap r
          else if ch c "R"%char then
            let* (_u, r1) := tok "{"%char r in let* (pfs, r2) := many f (pf_at f) r1 in
            let* (_v, r3) := tok "}"%char r2 in last (LRet pfs) r3
          else if ch c "C"%char then
            let* (g, r1) := nat_at r in let* (_u, r2) := tok "{"%char r1 in
            let* (args, r3) := many f (arg_at f) r2 in let* (_v, r4) := tok "|"%char r3 in
            let* (reps, r5) := many f map_at r4 in let* (_w, r6) := tok "|"%char r5 in
            let* (outs, r7) := many f map_at r6 in let* (_x, r8) := tok "|"%char r7 in
            let* (res, r9) := nat_at r8 in let* (_y, r10) := tok "|"%char r9 in
            let* (pfs, r11) := many f (pf_at f) r10 in let* (_z, r12) := tok "}"%char r11 in
            let* (k, r13) := lprog_at f r12 in Some (LCall g args reps outs res pfs k, r13)
          else None
      end
  end.

(* One procedure after `Q`: its id and its table entry. *)
Definition lproc_at (cs : list ascii) : option ((nat * Proc) * list ascii) :=
  let fuel := S (List.length cs) in
  let* (fid, r0) := nat_at cs in
  let* (_u, r1) := tok "{"%char r0 in
  let* (pr, r2) := many fuel (lin_at fuel) r1 in let* (_v, r3) := tok "|"%char r2 in
  let* (po, r4) := many fuel (lin_at fuel) r3 in let* (_w, r5) := tok "|"%char r4 in
  let* (rs, r6) := nat_at r5 in let* (_x, r7) := tok "|"%char r6 in
  let* (b, r8) := lprog_at fuel r7 in let* (_y, r9) := tok "}"%char r8 in
  Some ((fid, mkProc pr po rs b), sk r9).

Inductive LStretch :=
  | LOld (x : RoutineProcsCert.PStretch)
  | LQ (d : nat * Proc).

Definition lstretch_at (cs : list ascii) : option (LStretch * list ascii) :=
  match sk cs with
  | c :: r =>
      if ch c "Q"%char then let* (d, r1) := lproc_at r in Some (LQ d, r1)
      else let* (x, r1) := RoutineProcsCert.pstretch_at cs in Some (LOld x, r1)
  | [] => let* (x, r1) := RoutineProcsCert.pstretch_at cs in Some (LOld x, r1)
  end.

Fixpoint lstretches_at (fuel : nat) (cs : list ascii) : option (list LStretch) :=
  match fuel with
  | O => None
  | S f =>
      match lstretch_at cs with
      | Some (p, []) => Some [p]
      | Some (p, sp :: r) =>
          if ch sp ";"%char then
            match lstretches_at f r with
            | Some ps => Some (p :: ps)
            | None => None
            end
          else None
      | None => None
      end
  end.

Definition parse_lstretches (s : string) : option (list LStretch) :=
  let cs := list_ascii_of_string s in lstretches_at (S (List.length cs)) cs.

Definition olds (ps : list LStretch) : list RoutineProcsCert.PStretch :=
  flat_map (fun x => match x with LOld y => [y] | LQ _ => [] end) ps.

Definition lprocs_of (ps : list LStretch) : Procs :=
  flat_map (fun x => match x with LQ d => [d] | LOld _ => [] end) ps.

Fixpoint lnodup_ids (P : Procs) : bool :=
  match P with
  | [] => true
  | (f, _) :: r => negb (existsb (fun q => Nat.eqb (fst q) f) r) && lnodup_ids r
  end.

(* The `bounds` property's checker from the linear certificates on. *)
Definition check_lbounds_cert (s : string) : bool :=
  match parse_lstretches s with
  | Some ps =>
      let o := olds ps in
      forallb RoutineProcsCert.pstretch_ok o && RoutineProcsCert.nodup_ids (RoutineProcsCert.procs_of o) &&
      RoutineProcs.procs_ok (RoutineProcsCert.procs_of o) &&
      lnodup_ids (lprocs_of ps) && procs_ok (lprocs_of ps)
  | None => false
  end.

(* ── Soundness ────────────────────────────────────────────────────────── *)

(* What ACCEPT means: every older stretch keeps its meaning (as
   `check_pbounds_cert_sound` states it), and in the linear table the
   witness declares, every procedure's every run from a state meeting its
   precondition — through any depth of calls, recursion and loop
   iterations — reads and writes only inside its blocks, with their
   representation. *)
Theorem check_lbounds_cert_sound : forall s,
  check_lbounds_cert s = true ->
  exists ps, parse_lstretches s = Some ps /\
    (forall x, In x (olds ps) -> RoutineProcsCert.pstretch_ok x = true) /\
    RoutineProcs.procs_ok (RoutineProcsCert.procs_of (olds ps)) = true /\
    forall f d lb base ok bm e t o,
      plook (lprocs_of ps) f = Some d ->
      (forall b, 0 <= base b /\ 0 <= CAP (lb b) /\ base b + PAYLOAD + CAP (lb b) < ADDR) ->
      good lb base ok bm e (pre d) ->
      lrun (lprocs_of ps) lb base ok bm e (body d) t o ->
      Forall (lpin lb ok) t.
Proof.
  intros s H. unfold check_lbounds_cert in H.
  destruct (parse_lstretches s) as [ps |] eqn:Hp; [| discriminate].
  rewrite !andb_true_iff in H. destruct H as [[[[Hold _] HP] _] HQ].
  exists ps. split; [reflexivity | split; [| split; [exact HP |]]].
  - rewrite forallb_forall in Hold. intros x Hin. exact (Hold _ Hin).
  - intros f d lb base ok bm e t o Hf Hwf Hg Hrun.
    exact (proj1 (lrun_sound _ lb base ok Hwf HQ bm e (body d) t o Hrun (None, post d) (pre d) (None, None) Hg
                    (procs_ok_body _ HQ f d Hf))).
Qed.
Print Assumptions check_lbounds_cert_sound.

(* ── Controls ─────────────────────────────────────────────────────────── *)

(* The older stretches keep their verdicts. *)
Example older_stretches_unchanged :
  check_lbounds_cert "c0 a0 c1 a1 w0 w1" = true /\ check_lbounds_cert "c0 s0 a0" = false /\
  check_lbounds_cert RoutineProcsCert.copy_text = true.
Proof. repeat split; reflexivity. Qed.

(* RoutineLin's counted slot read, as bytes, as procedure 3's body (it
   returns). Region 7, slot 0 the handle, slot 1 the count. *)
Definition slot_text (c w : string) : string :=
  "Q3{ | | 0 | h0,7 f7 n1,7,4 T+ []" ++ c ++ " [s1:1]0 ()1()1 ()1()1 ()1()1 { R{} | A7," ++ w ++
  ",12 [s0:1]0 (7:1 3:1)1 (7:1)1 (6:1 0:4 2:1)1 R{} } }".

Example accepts_slot_read_text : check_lbounds_cert (slot_text "1" "8") = true.
Proof. reflexivity. Qed.

Example rejects_read_past_count_text : check_lbounds_cert (slot_text "0" "8") = false.
Proof. reflexivity. Qed.

(* A body that can fall through: refused. *)
Example rejects_fallthrough_text : check_lbounds_cert "Q3{ | | 0 | h0,7 }" = false.
Proof. reflexivity. Qed.

(* Anything after a terminal item: refused (malformed). *)
Example rejects_after_terminal : check_lbounds_cert "Q3{ | | 0 | R{} f7 }" = false.
Proof. reflexivity. Qed.

(* One id twice: refused. *)
Example rejects_duplicate_q : check_lbounds_cert "Q3{ | | 0 | R{} } ; Q3{ | | 0 | R{} }" = false.
Proof. reflexivity. Qed.

(* A refused older stretch beside an accepted procedure: refused. *)
Example rejects_old_beside_q : check_lbounds_cert "c0 s0 a0 ; Q3{ | | 0 | R{} }" = false.
Proof. reflexivity. Qed.

(* Negative constants and multipliers parse; a negative multiplier is
   refused by the checker, not the parser. *)
Example rejects_negative_multiplier_text :
  check_lbounds_cert "Q3{ | | 0 | h0,7 f7 A7,4,4 [s0:1]0 (4:1 0:1)1 (4:1)1 (3:1 1:-1)1 R{} }" = false /\
  check_lbounds_cert "Q3{ | | 0 | h0,7 f7 A7,4,4 [s0:1]0 (4:1 0:1)1 (4:1)1 (3:1 1:1)1 R{} }" = true.
Proof. split; reflexivity. Qed.

(* A recursive walk: slot 0 a pointer through region 7's payload to the end
   pointer slot 1, one byte read per step, then the call of itself with
   0 := s0 + 1 (its no-wrap proven, its precondition proven of the
   arguments). Reading two bytes per step is refused: the test proves one. *)
Definition walk_text (w : string) : string :=
  "Q5{ [b7:1 s0:-1]12 [s1:1 b7:-1 c7:-1]-12 | | 0 | f7 T+ [s0:1]0 [s1:1]0 ()1()1 ()1()1 ()1()1 { R{} | A7," ++ w ++
  ",0 [s0:1]0 (4:1 1:1)1 (4:1)1 (0:1 5:1)1 C5{ 0:[s0:1]1 ! (4:1 1:1)1 (0:1 5:1 3:1)1 1:[s1:1]0 ! ()1 ()1 | 7:7 | | 9 | (4:1)1 (5:1)1 } R{} } }".

Example accepts_recursive_walk : check_lbounds_cert (walk_text "1") = true.
Proof. reflexivity. Qed.

Example rejects_walk_two_bytes : check_lbounds_cert (walk_text "2") = false.
Proof. reflexivity. Qed.

(* A read of a static literal block: region 2 := the block at 144 of
   capacity 18756, the byte at payload offset 100 (address 256). Facts:
   0 `b2 - 144`, 1 `144 - b2`, 2 `c2 - 18756`, 3 `18756 - c2`. One byte
   past the capacity is refused. *)
Definition static_text (off : string) : string :=
  "Q1{ | | 0 | g2,144,18756 A2,1," ++ off ++ " []0 ()1 (0:1)1 (1:1 3:1)1 R{} }".

Example accepts_static_read : check_lbounds_cert (static_text "256") = true.
Proof. reflexivity. Qed.

Example rejects_static_past_cap : check_lbounds_cert (static_text "18912") = false.
Proof. reflexivity. Qed.
