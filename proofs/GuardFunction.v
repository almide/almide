(* Almide trust spine — a WHOLE function body in which the kernel has judged
   every load and every store (probe; not part of the gate).

   GuardWindow.v and GuardFresh.v judge the byte strings a cutter proposes.
   An access that nothing proposes is not refused: it is not looked at. This
   file turns the question round for one function at a time. It is given the
   instruction bytes of a function body and a list of CLAIMS, each naming a
   position in that body and the lengths of the strings one of the two rules
   reads there. The kernel then

   - walks the body with `step1`, instruction by instruction, and lists every
     load and store it finds (opcodes 40 to 62). A bulk memory instruction or
     a byte `step1` does not read ends the walk: the function is not judged;
   - cuts each claim's strings OUT OF THE BODY ITSELF, at the claimed
     position, requires that position (and, for a fresh-block claim, the two
     inner boundaries) to be where the walk starts an instruction, and asks
     the rule (`fresh_ok`, `window_ok`) about the strings it cut;
   - reads one more shape with the fresh-block rule: `pre` ends in
     `local.tee x` and the access that follows addresses the value the tee
     left on the stack. The kernel asks `fresh_ok` about `pre`, an empty
     `mid`, and the tail with `local.get x` put in front of it;
   - accepts when every load and store it listed ends exactly where an
     accepted claim ends.

   So an accepted body has no access the rules were not asked about, and the
   claims are a hint only: a wrong position, a wrong length or a missing claim
   makes the kernel refuse, and nothing a cutter omits can pass.

   `judged_covers` states the last step: in an accepted body every listed
   access is the end of a claim that the kernel accepted at its position.
   `fresh_sound` and `window_sound` then say what an accepted claim means.

   Not here, and so trusted: that the bytes given are the function's body as
   the module has it; that `step1` reads each instruction's length as the
   binary format has it, and that the rules' own decoders read the same
   instructions inside a claim; what the function's callees do (a call is not
   an access of this body); for the tee shape, that `local.tee x` leaves on
   the stack the value it stores, so that what runs after it is what runs
   after `local.set x ; local.get x`; and everything the two rules trust. *)

From Stdlib Require Import ZArith List Lia Bool.
From AlmideTrust Require Import GuardWindow GuardFresh.
Import ListNotations.
Open Scope Z_scope.

(* The length of a byte list, as a binary number. *)
Fixpoint zlen (bs : list Z) (acc : Z) : Z :=
  match bs with
  | [] => acc
  | _ :: r => zlen r (acc + 1)
  end.

(* The instruction at the front is a load or a store with a memarg. *)
Definition acc_op (bs : list Z) : bool :=
  match bs with
  | op :: _ => (40 <=? op) && (op <=? 62)
  | [] => false
  end.

(* The instruction at the front is memory.init, memory.copy or memory.fill
   (or a 252-prefixed instruction whose sub-opcode is not read). *)
Definition bulk_op (bs : list Z) : bool :=
  match bs with
  | op :: r =>
      if op =? 252 then
        match uleb 5 r with
        | Some (sub, _) => (sub =? 8) || (sub =? 10) || (sub =? 11)
        | None => true
        end
      else false
  | [] => false
  end.

(* One entry per instruction of `bs`, in order: the offset it starts at, the
   offset it ends at, and whether it is a load or a store. `total` is the
   length of the whole body and `off` the offset `bs` starts at. *)
Fixpoint layout (fuel : nat) (total off : Z) (bs : list Z) : option (list (Z * Z * bool)) :=
  match fuel with
  | O => None
  | S f =>
      match bs with
      | [] => Some []
      | _ :: _ =>
          if bulk_op bs then None
          else
            match step1 bs with
            | None => None
            | Some (_, r) =>
                match layout f total (total - zlen r 0) r with
                | Some l => Some ((off, total - zlen r 0, acc_op bs) :: l)
                | None => None
                end
            end
      end
  end.

(* A claim: where in the body a rule's strings start, and their lengths. *)
Inductive claim : Type :=
  | CFresh (s lp lm lt : Z)    (* pre, mid, tail of GuardFresh.v *)
  | CTee (s lp lt : Z)         (* pre ending in local.tee x, then the tail *)
  | CWindow (s n : Z).         (* one window of GuardWindow.v *)

Fixpoint list_eqb (a b : list Z) : bool :=
  match a, b with
  | [], [] => true
  | x :: a', y :: b' => (x =? y) && list_eqb a' b'
  | _, _ => false
  end.

Definition slice (s n : Z) (body : list Z) : list Z :=
  firstn (Z.to_nat n) (skipn (Z.to_nat s) body).

Definition starts_at (lay : list (Z * Z * bool)) (s : Z) : bool :=
  existsb (fun t => match t with (o, _, _) => o =? s end) lay.

(* Where an accepted claim ends; `None` for a claim that is refused. *)
Definition claim_end (alloc total : Z) (body : list Z) (lay : list (Z * Z * bool)) (c : claim)
  : option Z :=
  match c with
  | CFresh s lp lm lt =>
      if (0 <=? s) && (0 <? lp) && (0 <=? lm) && (0 <? lt) && (s + lp + lm + lt <=? total)
         && starts_at lay s && starts_at lay (s + lp) && starts_at lay (s + lp + lm)
         && fresh_ok alloc (slice s lp body) (slice (s + lp) lm body) (slice (s + lp + lm) lt body)
      then Some (s + lp + lm + lt)
      else None
  | CTee s lp lt =>
      match fresh_pre alloc (slice s lp body) with
      | Some (x, _) =>
          if (0 <=? s) && (2 <? lp) && (0 <? lt) && (s + lp + lt <=? total)
             && starts_at lay s && starts_at lay (s + lp)
             && (0 <=? x) && (x <? 128)
             && list_eqb (slice (s + lp - 2) 2 body) [34; x]
             && fresh_ok alloc (slice s lp body) [] (32 :: x :: slice (s + lp) lt body)
          then Some (s + lp + lt)
          else None
      | None => None
      end
  | CWindow s n =>
      if (0 <=? s) && (0 <? n) && (s + n <=? total) && starts_at lay s
         && window_ok (slice s n body)
      then Some (s + n)
      else None
  end.

Definition end_list (alloc total : Z) (body : list Z) (lay : list (Z * Z * bool)) (c : claim)
  : list Z :=
  match claim_end alloc total body lay c with
  | Some e => [e]
  | None => []
  end.

Definition covered (es : list Z) (t : Z * Z * bool) : bool :=
  match t with
  | (_, e, a) => if a then existsb (Z.eqb e) es else true
  end.

(* Every instruction of the body is read, and every load and store among them
   ends where an accepted claim ends. *)
Definition judged (alloc : Z) (body : list Z) (cs : list claim) : bool :=
  match layout (S (length body)) (zlen body 0) 0 body with
  | Some lay => forallb (covered (flat_map (end_list alloc (zlen body 0) body lay) cs)) lay
  | None => false
  end.

(* How many loads and stores the walk lists (for the report; `None` when the
   body is not read to its end). *)
Definition accesses (body : list Z) : option Z :=
  match layout (S (length body)) (zlen body 0) 0 body with
  | Some lay => Some (Z.of_nat (length (filter (fun t => match t with (_, _, a) => a end) lay)))
  | None => None
  end.

Theorem judged_covers : forall alloc body cs,
  judged alloc body cs = true ->
  exists lay,
    layout (S (length body)) (zlen body 0) 0 body = Some lay
    /\ forall o e, In (o, e, true) lay ->
         exists c, In c cs /\ claim_end alloc (zlen body 0) body lay c = Some e.
Proof.
  intros alloc body cs H. unfold judged in H.
  destruct (layout (S (length body)) (zlen body 0) 0 body) as [lay|] eqn:E.
  2: { try rewrite E in H. cbv beta iota in H. discriminate H. }
  try rewrite E in H. cbv beta iota in H.
  exists lay. split; [first [reflexivity | exact E] |].
  intros o e Hin.
  pose proof (proj1 (@forallb_forall _ _ _) H _ Hin) as Hc.
  cbv beta iota delta [covered] in Hc.
  destruct (proj1 (@existsb_exists _ _ _) Hc) as [e' [Hin' Heq]].
  apply Z.eqb_eq in Heq. subst e'.
  destruct (proj1 (@in_flat_map _ _ _ _ _) Hin') as [c [Hc1 Hc2]].
  exists c. split; [exact Hc1 |].
  unfold end_list in Hc2.
  destruct (claim_end alloc (zlen body 0) body lay c) as [e2|] eqn:E2.
  - try rewrite E2 in Hc2. simpl in Hc2.
    destruct Hc2 as [Hc2 | Hc2]; [| destruct Hc2].
    subst e2. first [reflexivity | exact E2].
  - try rewrite E2 in Hc2. simpl in Hc2. destruct Hc2.
Qed.
