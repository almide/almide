(* Almide trust spine — the access a loop's exit test guards, read from the
   SHIPPED BYTES and judged in the kernel (the byte-level check's core, fourth
   brick).

   GuardWindow.v judges a list access whose bound test loads the count in the
   same straight-line run. A `for` loop does not ship that way: the count is
   read ONCE, before the loop, into a local, and every turn tests the counter
   against that local and leaves the loop when it is not below. Four byte
   strings are read, consecutive in one function body:

     pre    local.get b ; i32.load 4 ; [i32.const s ; i32.div_u ;] local.set n
            (the capture: the count of the block at `b`, in slots of `s`)
     mid    everything up to the exit test (the counter's start, `block`,
            `loop`, whatever the loop does before the test)
     ht     a straight-line run (GuardWindow's window language): the exit
            test `local.get i ; local.get n ; i32.ge_u ; br_if l`, then the
            access's operands, then the access
     rest   everything from the access to the `end` that closes the outermost
            block opened since `pre`

   The verdict is GuardWindow's own check, on the capture's instructions
   followed by those of `ht`. `loop_sound` is about the run that happens: the
   run of `ht` alone, from ANY locals in which `n` already holds what the
   capture would store there (any globals, any stack beneath, 32-bit
   wrap-around as the machine has it; the memory is the one the capture is
   read against). Its access touches only the payload of the block at `b`,
   as long as as many bytes as that memory's length word says lie in the
   address space.

   What the kernel also computes, without a theorem of its own:
   - `scan_keep` (GuardFresh.v) walks EVERY instruction of `mid ++ ht ++ rest`
     and refuses if one assigns `b` or `n`, or if the nesting leaves the block
     `pre` stands in or does not close what it opened. So on every turn of
     every loop in between, `b` and `n` hold what they held after the capture:
     the theorem's premise about `n`, for the memory the capture read.
   - one of three things about the block's length word, which the access must
     find as the capture read it: `scan_pure` finds no store, no call, no
     bulk-memory instruction and no `memory.grow` in `mid ++ ht ++ rest`
     (nothing writes memory at all); or the capture is preceded by
     `local.get b ; call INC` (`loop_hold`: the loop holds its own share of
     the block); or `loop_kept` finds that local `b` is read nowhere in `mid`
     and `rest`, that `ht` assigns no local and ends in a load, and that
     nothing `ht` leaves on the stack is computed from `b` (the block's
     address goes nowhere but into the access).

   Not here, and so trusted: that the strings are consecutive instruction
   runs of one function (the cutter is almide-anchor-check), that `step1`
   reads each instruction's length as the binary format has it, the step
   from the scans to the premise about `n` (a local changes only by
   `local.set` / `local.tee` of its own function; control enters a block only
   through its first instruction); for a held block, that no routine writes
   the length word of a block with a second holder and that the share is not
   given up inside the loop (the ownership witness); for a kept block, that
   code which was not handed the block's address reaches it only through
   another reference, which makes it shared, and that no other store lands
   on its length word; and that a block's length word never exceeds its
   payload (the heap invariant GuardWindow.v also assumes). *)

From Stdlib Require Import ZArith List Lia Bool.
From AlmideTrust Require Import GuardWindow GuardFresh.
Import ListNotations.
Open Scope Z_scope.

(* ══ THE CAPTURE ══════════════════════════════════════════════════════ *)

(* `local.set n`, and nothing after it. *)
Definition set_tail (bs : list Z) : option Z :=
  match bs with
  | op :: r =>
      if op =? 33 then
        match uleb 5 r with
        | Some (n, []) => Some n
        | _ => None
        end
      else None
  | [] => None
  end.

(* `pre` read as `local.get b ; i32.load (align) 4`, then either `local.set n`
   or `i32.const s ; i32.div_u | i32.shr_u ; local.set n`: the base, the
   count's local, and the constant with its opcode when there is one. Which
   stride the count is in is GuardWindow's to judge. *)
Definition loop_pre (pre : list Z) : option (Z * Z * option (Z * Z)) :=
  match pre with
  | g :: r =>
      if g =? 32 then
        match uleb 5 r with
        | Some (b, ld :: r1) =>
            if ld =? 40 then
              match uleb 5 r1 with
              | Some (al, r2) =>
                  if al <? 64 then
                    match uleb 5 r2 with
                    | Some (off, c :: r4) =>
                        if off =? 4 then
                          if c =? 65 then
                            match sleb 5 r4 with
                            | Some (s, dv :: r5) =>
                                if (dv =? 110) || (dv =? 118) then
                                  match set_tail r5 with
                                  | Some n => Some (b, n, Some (s mod M32, dv))
                                  | None => None
                                  end
                                else None
                            | _ => None
                            end
                          else
                            match set_tail (c :: r4) with
                            | Some n => Some (b, n, None)
                            | None => None
                            end
                        else None
                    | _ => None
                    end
                  else None
              | None => None
              end
            else None
        | _ => None
        end
      else None
  | [] => None
  end.

(* The capture's instructions, as GuardWindow's decoder writes them. *)
Definition cap_of (b n : Z) (sd : option (Z * Z)) : list ins :=
  match sd with
  | Some (s, dv) => [ILocalGet b; ILoad 40 4; IConst s; IBin dv; ILocalSet n]
  | None => [ILocalGet b; ILoad 40 4; ILocalSet n]
  end.

(* ══ RUNS ═════════════════════════════════════════════════════════════ *)

Section Runs.

Variable G : Z -> Z.
Variable memf : Z -> Z -> Z.
Variable uo : Z -> Z -> Z.
Variable bo : Z -> Z -> Z -> Z.

Lemma crun_app : forall a b c,
  crun G memf uo bo (a ++ b) c =
  match crun G memf uo bo a c with
  | Some c' => crun G memf uo bo b c'
  | None => None
  end.
Proof.
  induction a as [|i a IH]; intros b c.
  - reflexivity.
  - cbn [app crun]. destruct (cstep G memf uo bo i c) as [c1|]; [apply IH | reflexivity].
Qed.

(* One step from two local maps that agree pointwise: both fail, or both
   leave the same stack and maps that agree pointwise. *)
Lemma cstep_ext : forall i loc1 loc2 stk,
  (forall x, loc1 x = loc2 x) ->
  match cstep G memf uo bo i (loc1, stk), cstep G memf uo bo i (loc2, stk) with
  | Some (l1, s1), Some (l2, s2) => s1 = s2 /\ forall x, l1 x = l2 x
  | None, None => True
  | _, _ => False
  end.
Proof.
  intros i loc1 loc2 stk H.
  assert (Hu : forall x v y, upd_loc loc1 x v y = upd_loc loc2 x v y).
  { intros x v y. unfold upd_loc. destruct (x =? y); [reflexivity | apply H]. }
  destruct i as [x|x|x|g|z|op|op|op off| |].
  - cbn [cstep]. rewrite (H x). split; [reflexivity | exact H].
  - destruct stk as [|v r]; cbn [cstep]; [exact I |].
    split; [reflexivity | intro y; apply Hu].
  - destruct stk as [|v r]; cbn [cstep]; [exact I |].
    split; [reflexivity | intro y; apply Hu].
  - cbn [cstep]. split; [reflexivity | exact H].
  - cbn [cstep]. split; [reflexivity | exact H].
  - destruct stk as [|a r]; cbn [cstep]; [exact I |].
    split; [reflexivity | exact H].
  - destruct stk as [|b [|a r]]; cbn [cstep]; [exact I | exact I |].
    split; [reflexivity | exact H].
  - destruct stk as [|a r]; cbn [cstep]; [exact I |].
    split; [reflexivity | exact H].
  - destruct stk as [|a r]; cbn [cstep]; [exact I |].
    split; [reflexivity | exact H].
  - destruct stk as [|c r]; cbn [cstep]; [exact I |].
    destruct (c mod M32 =? 0); cbn; [split; [reflexivity | exact H] | exact I].
Qed.

Lemma crun_ext : forall ws loc1 loc2 stk,
  (forall x, loc1 x = loc2 x) ->
  match crun G memf uo bo ws (loc1, stk), crun G memf uo bo ws (loc2, stk) with
  | Some (l1, s1), Some (l2, s2) => s1 = s2 /\ forall x, l1 x = l2 x
  | None, None => True
  | _, _ => False
  end.
Proof.
  induction ws as [|i r IH]; intros loc1 loc2 stk H.
  - cbn [crun]. split; [reflexivity | exact H].
  - cbn [crun]. pose proof (cstep_ext i loc1 loc2 stk H) as E.
    destruct (cstep G memf uo bo i (loc1, stk)) as [[l1 s1]|];
      destruct (cstep G memf uo bo i (loc2, stk)) as [[l2 s2]|].
    + simpl in E. destruct E as [Es El]. subst s2. apply IH. exact El.
    + exfalso. exact E.
    + exfalso. exact E.
    + exact I.
Qed.

(* The capture writes local `n` and nothing else, and leaves the stack. *)
Lemma cap_run : forall b n sd loc stk,
  exists v, crun G memf uo bo (cap_of b n sd) (loc, stk) = Some (upd_loc loc n v, stk).
Proof.
  intros b n sd loc stk.
  destruct sd as [[s dv]|]; cbn [cap_of crun cstep]; eexists; reflexivity.
Qed.

End Runs.

(* ══ THE SCANS ════════════════════════════════════════════════════════ *)

(* Every instruction of `bs` is read, and none writes memory or calls. *)
Fixpoint scan_pure (fuel : nat) (bs : list Z) : bool :=
  match fuel with
  | O => false
  | S f =>
      match bs with
      | [] => true
      | _ :: _ =>
          match step1 bs with
          | None => false
          | Some (w, r) =>
              match w with
              | WStore => false
              | WCall => false
              | _ => scan_pure f r
              end
          end
      end
  end.

(* `hold` read as `local.get b ; call inc`, and nothing after it. *)
Definition loop_hold (inc b : Z) (hold : list Z) : bool :=
  match hold with
  | g :: r =>
      if g =? 32 then
        match uleb 5 r with
        | Some (x, cl :: r1) =>
            (x =? b) && (cl =? 16)
            && match uleb 5 r1 with
               | Some (f, []) => f =? inc
               | _ => false
               end
        | _ => false
        end
      else false
  | [] => false
  end.

(* Every instruction of `bs` is read, and none is `local.get b`. *)
Fixpoint scan_noget (fuel : nat) (b : Z) (bs : list Z) : bool :=
  match fuel with
  | O => false
  | S f =>
      match bs with
      | [] => true
      | op :: r =>
          if (op =? 32) && match uleb 5 r with
                           | Some (x, _) => x =? b
                           | None => true
                           end
          then false
          else match step1 bs with
               | Some (_, r') => scan_noget f b r'
               | None => false
               end
      end
  end.

(* Whether an expression is computed from local `b`. *)
Fixpoint tm_has (b : Z) (t : tm) : bool :=
  match t with
  | TL x => x =? b
  | TG _ => false
  | TK _ => false
  | TM _ _ a => tm_has b a
  | TU _ a => tm_has b a
  | TB _ a c => tm_has b a || tm_has b c
  end.

Definition no_assign (i : ins) : bool :=
  match i with
  | ILocalSet _ => false
  | ILocalTee _ => false
  | _ => true
  end.

(* The block's address goes nowhere but into the access: `mid` and `rest`
   never read local `b`; `ht` assigns no local, ends in a load, and leaves
   under that load's address nothing computed from `b`. *)
Definition loop_kept (b : Z) (mid ht rest : list Z) : bool :=
  scan_noget (S (length mid)) b mid
  && scan_noget (S (length rest)) b rest
  && match dec (S (length ht)) ht [] with
     | Some (ws, ALoad, _, _) =>
         forallb no_assign ws
         && match srun ws (mkS [] [] []) with
            | Some s =>
                match sstk s with
                | _ :: below => forallb (fun t => negb (tm_has b t)) below
                | [] => false
                end
            | None => false
            end
     | _ => false
     end.

(* ══ THE RULE ═════════════════════════════════════════════════════════ *)

(* The capture reads as one, the test and the access decode, the capture's
   instructions followed by theirs pass GuardWindow's check on the capture's
   own base, and no instruction from the capture to the end of the outermost
   block it opens assigns the base or the count. *)
Definition loop_core (pre mid ht rest : list Z) : bool :=
  match loop_pre pre with
  | Some (b, n, sd) =>
      match dec (S (length ht)) ht [] with
      | Some (ws, k, off, w) =>
          negb (b =? n)
          && match check (cap_of b n sd ++ ws) k off w with
             | Some t => tm_eqb t (TL b)
             | None => false
             end
          && scan_keep (S (length (mid ++ ht ++ rest))) b 0 (mid ++ ht ++ rest)
          && scan_keep (S (length (mid ++ ht ++ rest))) n 0 (mid ++ ht ++ rest)
      | None => false
      end
  | None => false
  end.

(* What stands behind "the access finds the length word as the capture read
   it": 0 the core refuses; otherwise 4, plus 1 when no instruction in
   between writes memory or calls, plus 2 when the loop holds the block,
   plus 8 when the block's address goes nowhere but into the access. *)
Definition loop_class (inc : Z) (hold pre mid ht rest : list Z) : Z :=
  if loop_core pre mid ht rest then
    4 + (if scan_pure (S (length (mid ++ ht ++ rest))) (mid ++ ht ++ rest) then 1 else 0)
      + match loop_pre pre with
        | Some (b, _, _) =>
            (if loop_hold inc b hold then 2 else 0) + (if loop_kept b mid ht rest then 8 else 0)
        | None => 0
        end
  else 0.

Definition loop_ok (inc : Z) (hold pre mid ht rest : list Z) : bool :=
  5 <=? loop_class inc hold pre mid ht rest.

(* What an accepted proposal means. `pre` reads as the capture of the count
   of the block at local `b` into local `n`, and the scans of everything up
   to the end of the outermost block held for both. Then take any locals `L`
   in which `n` holds what the capture, run on `L` and this memory, would
   store there. Every run of the test and the access from `L` reaches the
   access only inside the payload this memory's length word gives the block
   at `b`. *)
Theorem loop_sound : forall L G memf uo bo,
  (forall a, 0 <= memf 40 a < M32) ->
  forall pre mid ht rest,
  loop_core pre mid ht rest = true ->
  exists b n sd ws k off w,
    loop_pre pre = Some (b, n, sd)
    /\ b <> n
    /\ scan_keep (S (length (mid ++ ht ++ rest))) b 0 (mid ++ ht ++ rest) = true
    /\ scan_keep (S (length (mid ++ ht ++ rest))) n 0 (mid ++ ht ++ rest) = true
    /\ dec (S (length ht)) ht [] = Some (ws, k, off, w)
    /\ forall stk0 loc stk,
         (forall v, crun G memf uo bo (cap_of b n sd) (L, stk0) = Some (upd_loc L n v, stk0) ->
                    L n = v) ->
         crun G memf uo bo ws (L, stk0) = Some (loc, stk) ->
         exists av,
           addr_of k stk = Some av
           /\ (L b mod M32 + PAYLOAD + memf 40 (L b mod M32 + 4) <= M32 ->
               L b mod M32 + PAYLOAD <= av mod M32 + off
               /\ av mod M32 + off + w
                  <= L b mod M32 + PAYLOAD + memf 40 (L b mod M32 + 4)).
Proof.
  intros L G memf uo bo Hmem pre mid ht rest H. unfold loop_core in H.
  destruct (loop_pre pre) as [[[b n] sd]|] eqn:Ep; [| discriminate H].
  destruct (dec (S (length ht)) ht []) as [[[[ws k] off] w]|] eqn:Ed; [| discriminate H].
  apply andb_prop in H. destruct H as [H Hn].
  apply andb_prop in H. destruct H as [H Hb].
  apply andb_prop in H. destruct H as [Hne Hc].
  destruct (check (cap_of b n sd ++ ws) k off w) as [t|] eqn:Ec; [| discriminate Hc].
  apply tm_eqb_eq in Hc. subst t.
  exists b, n, sd, ws, k, off, w.
  split; [first [reflexivity | exact Ep] |].
  split; [apply negb_true_iff in Hne; apply Z.eqb_neq in Hne; exact Hne |].
  split; [exact Hb |].
  split; [exact Hn |].
  split; [first [reflexivity | exact Ed] |].
  intros stk0 loc stk Hcap Hrun.
  destruct (cap_run G memf uo bo b n sd L stk0) as [v Hv].
  pose proof (Hcap v Hv) as Ev.
  assert (Hext : forall x, upd_loc L n v x = L x).
  { intro x. unfold upd_loc. destruct (n =? x) eqn:E; [| reflexivity].
    apply Z.eqb_eq in E. subst x. symmetry. exact Ev. }
  pose proof (crun_ext G memf uo bo ws (upd_loc L n v) L stk0 Hext) as Hx.
  rewrite Hrun in Hx.
  destruct (crun G memf uo bo ws (upd_loc L n v, stk0)) as [[loc' stk']|] eqn:Er;
    [| exfalso; exact Hx].
  simpl in Hx. destruct Hx as [Hs _]. subst stk'.
  assert (Hall : crun G memf uo bo (cap_of b n sd ++ ws) (L, stk0) = Some (loc', stk)).
  { rewrite crun_app. rewrite Hv. exact Er. }
  destruct (check_sound L G memf uo bo Hmem (cap_of b n sd ++ ws) k off w (TL b)
              stk0 loc' stk Ec Hall) as [av [Ha Hin]].
  exists av. split; [exact Ha | exact Hin].
Qed.

(* ── a `for` over a list of 4-byte slots as it ships: the hold, the capture
   `local.get 8 ; i32.load 4 ; i32.const 4 ; i32.div_u ; local.set 9`, the
   counter's start and the loop's two openers, the exit test and the read
   `local.get 8 ; local.get 10 ; i32.const 4 ; i32.mul ; i32.add ;
   i32.load offset=12`, then the body, the step and the two `end`s ── *)
Example shipped_for_read :
  loop_class 17 [32;8;16;17]
    [32;8;40;2;4;65;4;110;33;9]
    [65;0;33;10;2;64;3;64]
    [32;10;32;9;79;13;1;32;8;32;10;65;4;108;106;40;2;12]
    [33;2;32;2;16;18;32;10;65;1;106;33;10;12;0;11;11] = 14.
Proof. vm_compute. reflexivity. Qed.

(* ── a loop over a string's bytes, no call and no store in it: the capture
   `local.get 0 ; i32.load 4 ; local.set 2`, the read
   `local.get 0 ; local.get 1 ; i32.add ; i32.load8_u offset=12` ── *)
Example shipped_byte_loop :
  loop_class 17 []
    [32;0;40;2;4;33;2]
    [65;0;33;1;66;0;33;3;2;64;3;64]
    [32;1;32;2;79;13;1;32;0;32;1;106;45;0;12]
    [65;192;1;113;26;32;1;65;1;106;33;1;12;0;11;11] = 13.
Proof. vm_compute. reflexivity. Qed.

(* ── two strings compared byte by byte: the test, a read of the other string
   and the read of the one whose length was captured, in one run; nothing in
   the loop writes or calls ── *)
Example shipped_compare_loop :
  loop_class 17 []
    [32;18;40;2;4;33;19]
    [65;0;33;20;2;64;3;64]
    [32;20;32;19;79;13;1;32;17;32;20;106;45;0;12;32;18;32;20;106;45;0;12]
    [71;4;64;65;0;33;21;12;2;11;32;20;65;1;106;33;20;12;0;11;11] = 13.
Proof. vm_compute. reflexivity. Qed.

(* A loop that hands the list itself to a call, with no hold before the
   capture: the core holds, and nothing stands behind the length word. *)
Example no_frame :
  loop_ok 17 []
    [32;8;40;2;4;65;4;110;33;9]
    [65;0;33;10;2;64;3;64]
    [32;10;32;9;79;13;1;32;8;32;10;65;4;108;106;40;2;12]
    [33;2;32;8;16;18;32;10;65;1;106;33;10;12;0;11;11] = false.
Proof. vm_compute. reflexivity. Qed.

(* The same loop when the test-and-access run leaves the list's own address
   on the stack for what follows: not kept, although `rest` never reads the
   local. *)
Example address_left_on_the_stack_is_not_kept :
  loop_class 17 []
    [32;8;40;2;4;65;4;110;33;9]
    [65;0;33;10;2;64;3;64]
    [32;10;32;9;79;13;1;32;8;32;8;32;10;65;4;108;106;40;2;12]
    [33;2;16;18;32;10;65;1;106;33;10;12;0;11;11] = 4.
Proof. vm_compute. reflexivity. Qed.

(* The read scales by 8 while the count is in slots of 4. *)
Example refuses_other_stride :
  loop_core
    [32;8;40;2;4;65;4;110;33;9]
    [65;0;33;10;2;64;3;64]
    [32;10;32;9;79;13;1;32;8;32;10;65;8;108;106;40;2;12]
    [33;2;32;10;65;1;106;33;10;12;0;11;11] = false.
Proof. vm_compute. reflexivity. Qed.

(* The read is one slot further than the test allows. *)
Example refuses_next_slot :
  loop_core
    [32;8;40;2;4;65;4;110;33;9]
    [65;0;33;10;2;64;3;64]
    [32;10;32;9;79;13;1;32;8;32;10;65;4;108;106;40;2;16]
    [33;2;32;10;65;1;106;33;10;12;0;11;11] = false.
Proof. vm_compute. reflexivity. Qed.

(* The count was captured from another block than the one read. *)
Example refuses_other_block :
  loop_core
    [32;7;40;2;4;65;4;110;33;9]
    [65;0;33;10;2;64;3;64]
    [32;10;32;9;79;13;1;32;8;32;10;65;4;108;106;40;2;12]
    [33;2;32;10;65;1;106;33;10;12;0;11;11] = false.
Proof. vm_compute. reflexivity. Qed.

(* The body assigns the list's local: the next turn reads another block
   against the old count. *)
Example refuses_reassigned_base :
  loop_core
    [32;8;40;2;4;65;4;110;33;9]
    [65;0;33;10;2;64;3;64]
    [32;10;32;9;79;13;1;32;8;32;10;65;4;108;106;40;2;12]
    [33;2;32;2;33;8;32;10;65;1;106;33;10;12;0;11;11] = false.
Proof. vm_compute. reflexivity. Qed.

(* The body assigns the count's local. *)
Example refuses_reassigned_count :
  loop_core
    [32;8;40;2;4;65;4;110;33;9]
    [65;0;33;10;2;64;3;64]
    [32;10;32;9;79;13;1;32;8;32;10;65;4;108;106;40;2;12]
    [33;2;65;99;33;9;32;10;65;1;106;33;10;12;0;11;11] = false.
Proof. vm_compute. reflexivity. Qed.

(* No exit test before the read. *)
Example refuses_untested_read :
  loop_core
    [32;8;40;2;4;65;4;110;33;9]
    [65;0;33;10;2;64;3;64]
    [32;8;32;10;65;4;108;106;40;2;12]
    [33;2;32;10;65;1;106;33;10;12;0;11;11] = false.
Proof. vm_compute. reflexivity. Qed.

(* The test leaves the loop when the counter is ABOVE the count, so the turn
   with the counter equal to it reads one slot past the end. *)
Example refuses_off_by_one_test :
  loop_core
    [32;8;40;2;4;65;4;110;33;9]
    [65;0;33;10;2;64;3;64]
    [32;10;32;9;75;13;1;32;8;32;10;65;4;108;106;40;2;12]
    [33;2;32;10;65;1;106;33;10;12;0;11;11] = false.
Proof. vm_compute. reflexivity. Qed.

(* The strings stop before the loop closes: an instruction after them could
   assign the base and the back edge bring it to the read. *)
Example refuses_open_loop :
  loop_core
    [32;8;40;2;4;65;4;110;33;9]
    [65;0;33;10;2;64;3;64]
    [32;10;32;9;79;13;1;32;8;32;10;65;4;108;106;40;2;12]
    [33;2;32;10;65;1;106;33;10;12;0] = false.
Proof. vm_compute. reflexivity. Qed.

(* ── the proposal almide-anchor-check's loop cutter prints for its own test
   module (crates/almide-anchor-check/tests/loops.rs): held, and with
   nothing in the loop that writes, calls or reads the list's local but the
   access ── *)
Example cutter_for_read :
  loop_class 0 [32;0;16;0]
    [32;0;40;2;4;65;4;110;33;1]
    [65;0;33;2;2;64;3;64]
    [32;2;32;1;79;13;1;32;0;32;2;65;4;108;106;40;2;12]
    [33;3;32;2;65;1;106;33;2;12;0;11;11] = 15.
Proof. vm_compute. reflexivity. Qed.

Example refuses_cutter_read_one_slot_further :
  loop_core
    [32;0;40;2;4;65;4;110;33;1]
    [65;0;33;2;2;64;3;64]
    [32;2;32;1;79;13;1;32;0;32;2;65;4;108;106;40;2;16]
    [33;3;32;2;65;1;106;33;2;12;0;11;11] = false.
Proof. vm_compute. reflexivity. Qed.

(* ══ A CONTROL ON THE LENGTH TABLE ════════════════════════════════════ *)

(* The scans rest on `step1` reading each instruction's length as the binary
   format has it. Nothing here proves that; this is the control that tests
   it: the lengths `step1` gives the instructions of a byte string, in order,
   compared with the lengths an independent decoder found for the same
   bytes. *)
Fixpoint scan_lens (fuel : nat) (bs : list Z) : option (list Z) :=
  match fuel with
  | O => None
  | S f =>
      match bs with
      | [] => Some []
      | _ :: _ =>
          match step1 bs with
          | None => None
          | Some (_, r) =>
              match scan_lens f r with
              | Some ls => Some (Z.of_nat (length bs) - Z.of_nat (length r) :: ls)
              | None => None
              end
          end
      end
  end.

Fixpoint zs_eqb (a b : list Z) : bool :=
  match a, b with
  | [], [] => true
  | x :: a', y :: b' => (x =? y) && zs_eqb a' b'
  | _, _ => false
  end.

Definition lens_ok (bs ls : list Z) : bool :=
  match scan_lens (S (length bs)) bs with
  | Some l => zs_eqb l ls
  | None => false
  end.

(* `i32.const 0 ; local.set 10 ; block ; loop ; local.get 300 ; i64.load
   offset=12 ; br 0 ; end ; end`: a two-byte local index among them. *)
Example lens_of_a_loop :
  lens_ok [65;0;33;10;2;64;3;64;32;172;2;41;3;12;12;0;11;11] [2;2;2;2;3;3;2;1;1] = true.
Proof. vm_compute. reflexivity. Qed.

Example lens_refuses_a_shifted_boundary :
  lens_ok [65;0;33;10;2;64;3;64;32;172;2;41;3;12;12;0;11;11] [2;2;2;2;2;4;2;1;1] = false.
Proof. vm_compute. reflexivity. Qed.

Print Assumptions loop_sound.
