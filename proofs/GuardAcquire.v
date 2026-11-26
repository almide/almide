(* Almide trust spine — a recorded acquire and the instruction that performs
   it, read from the SHIPPED BYTES and judged in the kernel.

   The ownership witness records, per frame, the shares the frame takes: an
   event `a<local>` says one more share was taken on the value that local
   holds. The proven checker judges those events; nothing proven says the
   shipped function performs them. The defects of this kind that reached
   releases were exactly that gap: the event was recorded and the call of
   the rc_inc routine was not emitted.

   This file states the smallest thing that closes it for one kind of event.
   For a frame, a list of ACQUIRES (the holders its named `a` events name)
   and, for each, a WINDOW: a run of the function body's bytes that ends in
   a call of the rc_inc routine. `acquire_ok` holds when

     - every window is a path (GuardPath.v's `pdec`) that starts at an
       instruction of the body and ends in a call of the rc_inc routine;
     - run over expressions, the path leaves as the call's operand the very
       expression its acquire's holder stands for: the value the local
       holds at the call (`HLocal`), a word loaded through the address that
       local holds (`HCell`: the occupant of a cell), or a global (`HGlobal`);
     - no two acquires share a call.

   `node_sound`: in EVERY run of a window's decoded instructions that
   reaches the call (any locals, globals, memories, any stack beneath), the
   operand the call is handed is that value. `acquire_sound` adds that the
   calls are distinct instructions of the body, one per acquire.

   What this does NOT say, and so is not claimed: that the call runs on
   exactly the paths on which the recorder logged the event (no semantics
   of the function's control flow is given here), nor anything about
   releases, moves or rebinds. It says each recorded acquire has its own
   instruction in the bytes that ship.

   Not here, and so trusted: that `body` is the bytes of the frame's
   function, that `inc` is the rc_inc routine's index (the anchor file's
   `rcfns`) and that the acquires listed are the frame's named `a` events
   (the anchor file's `rc` line; the script reads both), and GuardPath.v's
   own list for a path. *)

From Stdlib Require Import ZArith List Lia Bool.
From AlmideTrust Require Import GuardWindow GuardFresh GuardPath.
Import ListNotations.
Open Scope Z_scope.

(* ── expressions can be compared ── *)

Definition uop_eq_dec : forall a b : uop, {a = b} + {a <> b}.
Proof. decide equality; apply Z.eq_dec. Defined.

Definition bop_eq_dec : forall a b : bop, {a = b} + {a <> b}.
Proof. decide equality; apply Z.eq_dec. Defined.

Definition ptm_eq_dec : forall a b : ptm, {a = b} + {a <> b}.
Proof.
  decide equality;
    first [ apply Z.eq_dec | apply Nat.eq_dec | apply uop_eq_dec | apply bop_eq_dec ].
Defined.

Definition ptm_eqb (a b : ptm) : bool := if ptm_eq_dec a b then true else false.

Lemma ptm_eqb_eq : forall a b, ptm_eqb a b = true -> a = b.
Proof.
  intros a b H. unfold ptm_eqb in H. destruct (ptm_eq_dec a b) as [E|E]; [exact E | discriminate H].
Qed.

(* ── one acquire's holder and what the call must be handed ── *)

Inductive holder : Type :=
  | HLocal (l : Z)
  | HCell (l : Z)
  | HGlobal (g : Z).

Definition held (h : holder) (e : list (Z * ptm)) (t : ptm) : bool :=
  match h with
  | HLocal l => ptm_eqb t (plook e l)
  | HCell l =>
      match t with
      | XM _ op _ a => (op =? 40) && ptm_eqb a (plook e l)
      | _ => false
      end
  | HGlobal g =>
      match t with
      | XG _ g' => g' =? g
      | _ => false
      end
  end.

(* A window: a path that ends in a call of `inc`, whose operand is the
   holder's expression. *)
Definition node_ok (alloc inc : Z) (bs : list Z) (h : holder) : bool :=
  match pdec alloc (S (length bs + 0)) bs [] [] with
  | Some (ws, FCall f) =>
      (f =? inc)
      && match prun ws (mkP [] [] [] O O O []) with
         | Some s =>
             match pstk s with
             | t :: _ => held h (penv s) t
             | [] => false
             end
         | None => false
         end
  | _ => false
  end.

(* The same in the machine's own terms: the value on top of the stack is the
   holder's. *)
Definition handed (mem : nat -> Z -> Z -> Z) (G : nat -> Z -> Z)
                  (h : holder) (loc : Z -> Z) (a : Z) : Prop :=
  match h with
  | HLocal l => a = loc l
  | HCell l => exists v off, a = mem v 40 (u32 (loc l) + off)
  | HGlobal g => exists v, a = G v g
  end.

Theorem node_sound : forall alloc inc bs h,
  node_ok alloc inc bs h = true ->
  exists ws,
    pdec alloc (S (length bs + 0)) bs [] [] = Some (ws, FCall inc)
    /\ forall L0 G mem ret hav jv uo bo under loc stk v mv gv log,
         qrun G mem ret hav jv uo bo ws (L0, under, O, O, O, []) = Some (loc, stk, v, mv, gv, log) ->
         exists a r, stk = a :: r /\ handed mem G h loc a.
Proof.
  intros alloc inc bs h H. unfold node_ok in H.
  destruct (pdec alloc (S (length bs + 0)) bs [] []) as [[ws f]|] eqn:Ed; [| discriminate H].
  destruct f as [| | off w | off w | f]; try discriminate H.
  apply andb_prop in H. destruct H as [Hf H]. apply Z.eqb_eq in Hf. subst f.
  exists ws. split; [reflexivity |].
  intros L0 G mem ret hav jv uo bo under loc stk v mv gv log Hrun.
  destruct (prun ws (mkP [] [] [] O O O [])) as [s|] eqn:Es; [| discriminate H].
  assert (H0 : sim L0 G mem ret hav jv uo bo under (mkP [] [] [] O O O []) (L0, under, O, O, O, [])).
  { cbn [sim penv pstk pfacts pver pmem pglo pallocs map app den_log plook pden].
    repeat split; auto. }
  pose proof (prun_sound L0 G mem ret hav jv uo bo ws under _ _ s _ H0 Es Hrun) as Hs.
  cbn [sim] in Hs. destruct Hs as (Hstk & Hl & _).
  destruct (pstk s) as [|t r] eqn:Et; [discriminate H |].
  exists (pden L0 G mem ret hav jv uo bo t), (map (pden L0 G mem ret hav jv uo bo) r ++ under).
  split; [subst stk; reflexivity |].
  destruct h as [l | l | g]; cbn [held handed] in *.
  - apply ptm_eqb_eq in H. subst t. symmetry. apply Hl.
  - destruct t as [x | v1 g1 | z | v1 op1 off1 a1 | k | k x | k | o a1 | o a1 b1 | a1 b1 c1];
      try discriminate H.
    apply andb_prop in H. destruct H as [Ho H]. apply Z.eqb_eq in Ho. subst op1.
    apply ptm_eqb_eq in H. subst a1.
    exists v1, off1. cbn [pden]. rewrite <- Hl. reflexivity.
  - destruct t as [x | v1 g1 | z | v1 op1 off1 a1 | k | k x | k | o a1 | o a1 b1 | a1 b1 c1];
      try discriminate H.
    apply Z.eqb_eq in H. subst g1. exists v1. reflexivity.
Qed.

(* ── the frame: every acquire has its own call ── *)

(* `n` bytes of `bs` from byte `a`. *)
Definition sub (bs : list Z) (a n : nat) : list Z := firstn n (skipn a bs).

(* Byte `a` of `bs` is where an instruction starts (`step1` walks to it). *)
Fixpoint starts (fuel : nat) (bs : list Z) (a : nat) : bool :=
  match a with
  | O => true
  | S _ =>
      match fuel with
      | O => false
      | S f =>
          match step1 bs with
          | Some (_, r) =>
              let used := (length bs - length r)%nat in
              if (used <=? a)%nat && (0 <? used)%nat then starts f r (a - used) else false
          | None => false
          end
      end
  end.

Fixpoint nat_in (x : nat) (xs : list nat) : bool :=
  match xs with
  | [] => false
  | y :: r => Nat.eqb x y || nat_in x r
  end.

Fixpoint distinct (xs : list nat) : bool :=
  match xs with
  | [] => true
  | x :: r => negb (nat_in x r) && distinct r
  end.

(* One acquire's certificate: its holder, and its window as a start and a
   length in the body. *)
Definition cert : Type := (holder * nat * nat)%type.

Definition cert_ok (alloc inc : Z) (body : list Z) (c : cert) : bool :=
  let '(h, a, n) := c in
  (a + n <=? length body)%nat
  && starts (S (length body)) body a
  && node_ok alloc inc (sub body a n) h.

Definition acquire_ok (alloc inc : Z) (body : list Z) (cs : list cert) : bool :=
  forallb (cert_ok alloc inc body) cs
  && distinct (map (fun c : cert => let '(_, a, n) := c in (a + n)%nat) cs).

Theorem acquire_sound : forall alloc inc body cs,
  acquire_ok alloc inc body cs = true ->
  (forall h a n, In (h, a, n) cs ->
     (a + n <= length body)%nat
     /\ exists ws,
          pdec alloc (S (length (sub body a n) + 0)) (sub body a n) [] [] = Some (ws, FCall inc)
          /\ forall L0 G mem ret hav jv uo bo under loc stk v mv gv log,
               qrun G mem ret hav jv uo bo ws (L0, under, O, O, O, []) = Some (loc, stk, v, mv, gv, log) ->
               exists x r, stk = x :: r /\ handed mem G h loc x)
  /\ distinct (map (fun c : cert => let '(_, a, n) := c in (a + n)%nat) cs) = true.
Proof.
  intros alloc inc body cs H. unfold acquire_ok in H.
  apply andb_prop in H. destruct H as [Hall Hd]. split; [| exact Hd].
  intros h a n Hin. rewrite forallb_forall in Hall.
  pose proof (Hall (h, a, n) Hin) as Hc. unfold cert_ok in Hc.
  apply andb_prop in Hc. destruct Hc as [Hc Hn].
  apply andb_prop in Hc. destruct Hc as [Hlen _].
  apply Nat.leb_le in Hlen. split; [exact Hlen |].
  exact (node_sound alloc inc (sub body a n) h Hn).
Qed.

Print Assumptions acquire_sound.

(* ── `local.get 3 ; local.set 10 ; local.get 10 ; call 10`: the value local 3
     holds is handed to the rc_inc routine (index 10), through local 10 ── *)
Example acquire_through_a_copy :
  acquire_ok 7 10 [ 65; 0; 26; 32; 3; 33; 10; 32; 10; 16; 10; 32; 10; 26 ] [ (HLocal 3, 3%nat, 8%nat) ] = true.
Proof. vm_compute. reflexivity. Qed.

(* Controls: the same call is not an acquire on local 4; a window that does
   not start at an instruction is refused; two acquires do not share one
   call; and a call of another function is not the rc_inc routine's. *)
Example refuses_another_local :
  acquire_ok 7 10 [ 65; 0; 26; 32; 3; 33; 10; 32; 10; 16; 10; 32; 10; 26 ] [ (HLocal 4, 3%nat, 8%nat) ] = false.
Proof. vm_compute. reflexivity. Qed.

Example refuses_a_window_inside_an_instruction :
  acquire_ok 7 10 [ 65; 0; 26; 32; 3; 33; 10; 32; 10; 16; 10; 32; 10; 26 ] [ (HLocal 10, 8%nat, 3%nat) ] = false.
Proof. vm_compute. reflexivity. Qed.

Example refuses_a_shared_call :
  acquire_ok 7 10 [ 65; 0; 26; 32; 3; 33; 10; 32; 10; 16; 10; 32; 10; 26 ]
    [ (HLocal 3, 3%nat, 8%nat); (HLocal 10, 7%nat, 4%nat) ] = false.
Proof. vm_compute. reflexivity. Qed.

Example refuses_another_callee :
  acquire_ok 7 11 [ 65; 0; 26; 32; 3; 33; 10; 32; 10; 16; 10; 32; 10; 26 ] [ (HLocal 3, 3%nat, 8%nat) ] = false.
Proof. vm_compute. reflexivity. Qed.
