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

   The verdict is GuardWindow's own. `pre ++ ht` is a window: the capture,
   then the test and the access. `loop_sound` is `window_sound` for that byte
   string: in EVERY run of the capture followed, from the locals the capture
   left, by the test and the access (any locals, any globals, any memory, any
   stack beneath, 32-bit wrap-around as the machine has it), the access
   touches only the payload of the block at `b`, as long as as many bytes as
   the length word then said lie in the address space.

   What the kernel also computes, without a theorem of its own:
   - `scan_keep` (GuardFresh.v) walks EVERY instruction of `mid ++ ht ++ rest`
     and refuses if one assigns `b` or `n`, or if the nesting leaves the block
     `pre` stands in or does not close what it opened. So on every turn of
     every loop in between, `b` and `n` hold what they held after the capture.
   - `ht` loads nothing before its access, so what its run computes depends on
     the locals alone.
   - one of two things about memory: `scan_pure` finds no store, no call, no
     bulk-memory instruction and no `memory.grow` in `mid ++ ht ++ rest` (the
     length word is as the capture read it), or the capture is preceded by
     `local.get b ; call INC` (`loop_hold`: the loop holds its own share of
     the block).

   Not here, and so trusted: that the strings are consecutive instruction
   runs of one function (the cutter is almide-anchor-check), that `step1`
   reads each instruction's length as the binary format has it, the step
   from the scans to "the run of `ht` in the loop is the run of `ht` after
   the capture" (a local changes only by `local.set` / `local.tee` of its own
   function; control enters a block only through its first instruction), for
   a held block that no routine resizes or frees a block with a second holder
   and that the share is not given up inside the loop (the ownership
   witness), and that a block's length word never exceeds its payload (the
   heap invariant GuardWindow.v also assumes). *)

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
   or `i32.const s ; i32.div_u | i32.shr_u ; local.set n`: the base and the
   count's local. Which stride the count is in is GuardWindow's to judge. *)
Definition loop_pre (pre : list Z) : option (Z * Z) :=
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
                            | Some (_, dv :: r5) =>
                                if (dv =? 110) || (dv =? 118) then
                                  match set_tail r5 with
                                  | Some n => Some (b, n)
                                  | None => None
                                  end
                                else None
                            | _ => None
                            end
                          else
                            match set_tail (c :: r4) with
                            | Some n => Some (b, n)
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

(* ══ THE SCANS ════════════════════════════════════════════════════════ *)

Definition no_load (i : ins) : bool :=
  match i with
  | ILoad _ _ => false
  | _ => true
  end.

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

(* ══ THE RULE ═════════════════════════════════════════════════════════ *)

(* The capture reads as one, the capture followed by the test and the access
   is a window on the capture's own base, the test and the access load
   nothing in between, and no instruction from the capture to the end of the
   outermost block it opens assigns the base or the count. *)
Definition loop_core (pre mid ht rest : list Z) : bool :=
  match loop_pre pre with
  | Some (b, n) =>
      negb (b =? n)
      && match window (pre ++ ht) with
         | Some t => tm_eqb t (TL b)
         | None => false
         end
      && match dec (S (length ht)) ht [] with
         | Some (ws, _, _, _) => forallb no_load ws
         | None => false
         end
      && scan_keep (S (length (mid ++ ht ++ rest))) b 0 (mid ++ ht ++ rest)
      && scan_keep (S (length (mid ++ ht ++ rest))) n 0 (mid ++ ht ++ rest)
  | None => false
  end.

(* What stands behind "the length word is still what the capture read":
   0 the core refuses; 4 nothing; 5 no instruction in between writes memory
   or calls; 6 the loop holds the block; 7 both. *)
Definition loop_class (inc : Z) (hold pre mid ht rest : list Z) : Z :=
  if loop_core pre mid ht rest then
    4 + (if scan_pure (S (length (mid ++ ht ++ rest))) (mid ++ ht ++ rest) then 1 else 0)
      + match loop_pre pre with
        | Some (b, _) => if loop_hold inc b hold then 2 else 0
        | None => 0
        end
  else 0.

Definition loop_ok (inc : Z) (hold pre mid ht rest : list Z) : bool :=
  5 <=? loop_class inc hold pre mid ht rest.

(* What an accepted proposal means: `pre` reads as the capture of the count
   of the block at local `b` into local `n`, the scans of everything up to
   the end of the outermost block held for both, and every run of the
   capture followed by the test and the access reaches the access only
   inside the payload the length word then gave the block at `b`. *)
Theorem loop_sound : forall L0 G memf uo bo,
  (forall a, 0 <= memf 40 a < M32) ->
  forall pre mid ht rest,
  loop_core pre mid ht rest = true ->
  exists b n ws k off w,
    loop_pre pre = Some (b, n)
    /\ b <> n
    /\ scan_keep (S (length (mid ++ ht ++ rest))) b 0 (mid ++ ht ++ rest) = true
    /\ scan_keep (S (length (mid ++ ht ++ rest))) n 0 (mid ++ ht ++ rest) = true
    /\ dec (S (length (pre ++ ht))) (pre ++ ht) [] = Some (ws, k, off, w)
    /\ forall stk0 loc stk,
         crun G memf uo bo ws (L0, stk0) = Some (loc, stk) ->
         exists av,
           addr_of k stk = Some av
           /\ (L0 b mod M32 + PAYLOAD + memf 40 (L0 b mod M32 + 4) <= M32 ->
               L0 b mod M32 + PAYLOAD <= av mod M32 + off
               /\ av mod M32 + off + w
                  <= L0 b mod M32 + PAYLOAD + memf 40 (L0 b mod M32 + 4)).
Proof.
  intros L0 G memf uo bo Hmem pre mid ht rest H. unfold loop_core in H.
  destruct (loop_pre pre) as [[b n]|] eqn:Ep; [| discriminate H].
  apply andb_prop in H. destruct H as [H Hn].
  apply andb_prop in H. destruct H as [H Hb].
  apply andb_prop in H. destruct H as [H Hl].
  apply andb_prop in H. destruct H as [Hne Hw].
  destruct (window (pre ++ ht)) as [t|] eqn:Ew; [| discriminate Hw].
  apply tm_eqb_eq in Hw. subst t.
  assert (Ew' := Ew). unfold window in Ew'.
  destruct (dec (S (length (pre ++ ht))) (pre ++ ht) []) as [[[[ws k] off] w]|] eqn:Ed;
    [| discriminate Ew'].
  exists b, n, ws, k, off, w.
  split; [first [reflexivity | exact Ep] |].
  split; [apply negb_true_iff in Hne; apply Z.eqb_neq in Hne; exact Hne |].
  split; [exact Hb |].
  split; [exact Hn |].
  split; [first [reflexivity | exact Ed] |].
  intros stk0 loc stk Hrun.
  destruct (window_sound L0 G memf uo bo Hmem (pre ++ ht) (TL b) ws k off w Ed Ew stk0 loc stk Hrun)
    as [av [Ha Hin]].
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
    [33;2;32;2;16;18;32;10;65;1;106;33;10;12;0;11;11] = 6.
Proof. vm_compute. reflexivity. Qed.

(* ── a loop over a string's bytes, no call and no store in it: the capture
   `local.get 0 ; i32.load 4 ; local.set 2`, the read
   `local.get 0 ; local.get 1 ; i32.add ; i32.load8_u offset=12` ── *)
Example shipped_byte_loop :
  loop_class 17 []
    [32;0;40;2;4;33;2]
    [65;0;33;1;66;0;33;3;2;64;3;64]
    [32;1;32;2;79;13;1;32;0;32;1;106;45;0;12]
    [65;192;1;113;26;32;1;65;1;106;33;1;12;0;11;11] = 5.
Proof. vm_compute. reflexivity. Qed.

(* A loop with a call in it and no hold before the capture: the core holds,
   nothing stands behind the length word. *)
Example no_frame :
  loop_ok 17 []
    [32;8;40;2;4;65;4;110;33;9]
    [65;0;33;10;2;64;3;64]
    [32;10;32;9;79;13;1;32;8;32;10;65;4;108;106;40;2;12]
    [33;2;32;2;16;18;32;10;65;1;106;33;10;12;0;11;11] = false.
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

Print Assumptions loop_sound.
