(* Almide trust spine — #4013: ONE definition of memory safety.

   The spine proved separate properties — RC balance (OwnershipChecker.v),
   reuse safety (FreeList.v, FreeListRc.v), copy-on-write alias safety
   (CowSafety.v), the allocator's runtime trees (Structural*.v) — and no
   theorem said what "memory safe" means. So "Almide proves memory safety"
   could not be stated, only "Almide proves some properties". This file
   states it once, over a HEAP TRACE: the sequence of heap events a run
   performs, each naming a block.

     EAlloc b n k    a fresh block b of n bytes, holding representation k
     EStore b o w k  write w bytes at offset o of b, as representation k
     ELoad  b o w k  read  w bytes at offset o of b, as representation k
     EInc b / EDec b take / release one reference (the count reaching 0
                     frees the block)

   `MemorySafe t` is the conjunction of four clauses, each checked at the
   state the prefix before the event has produced:

     SPATIAL         every load and store lies inside its live block:
                     0 <= o, 0 < w, o + w <= n                       (#4005)
     TEMPORAL        no load, store, inc or dec touches a block whose count
                     is 0 (use-after-free, double free), and an allocation
                     never hands out a block that is still live
     INITIALIZATION  every byte a load reads was stored since the block was
                     allocated
     TYPE            a load or store uses the representation the block was
                     allocated with (no heap-handle repr confusion)

   The state update is PERMISSIVE: it performs every event whether or not
   the clauses hold. So the four clauses are judged against one and the same
   sequence of states, and `MemorySafe` splits into them exactly
   (`memory_safe_iff_clauses`).

   What is proven here:
   - `check_trace_sound` / `check_trace_complete`: a decidable checker K
     accepts a trace exactly when the trace is memory safe;
   - `memory_safe_iff_clauses`: the definition is the four clauses, no more;
   - `rc_check_gives_temporal`: the EXISTING per-object RC checker
     (OwnershipChecker.check over each block's op stream, with #3229's
     liveness guard before every alias), on a trace whose allocations hand
     out blocks that are not live, yields the whole TEMPORAL clause. That
     is the map from the spine's oldest theorem into this definition.
   - `unguarded_rc_check_misses_resurrection`: why the guard is needed.
     The bare fold accepts an inc of a FREED block (a fresh acquire and an
     alias are the same +1 to it), which is not memory safe.
   - SPATIAL, INITIALIZATION and TYPE have no counterpart in the existing
     spine: no theorem there speaks of a block's size, its written bytes or
     its representation. K is the only checker of them so far.
   - negative controls, at least one per clause (`rejects_*`): a trace that
     breaks that clause is rejected by K, and a safe one is accepted.

   HONEST SCOPE. This is the definition and a sound, complete checker over
   traces. It is not yet a theorem about the bytes a build ships: that needs
   the compiler to emit each function's heap events in its certificate (the
   MIR side of #4005) and the translation from MIR to wasm or Rust to preserve
   them (#1696, #4006). Until then the theorem to aim at,
   `K(c, a) accepts ==> MemorySafe a`, has its right-hand side fixed here. *)

From AlmideTrust Require Import OwnershipChecker.
From Stdlib Require Import List ZArith Lia Bool Arith.
Import ListNotations.
Open Scope Z_scope.

(* ── Events and state ─────────────────────────────────────────────────── *)

Definition Blk := nat.
Definition Repr := nat.

Inductive Ev : Type :=
  | EAlloc (b : Blk) (n : Z) (k : Repr)
  | EStore (b : Blk) (o w : Z) (k : Repr)
  | ELoad  (b : Blk) (o w : Z) (k : Repr)
  | EInc   (b : Blk)
  | EDec   (b : Blk).

Record Block := mkBlock {
  size : Z;
  repr : Repr;
  rc   : Z;
  init : Z -> bool
}.

(* None: never allocated. A freed block keeps its record with rc = 0, so a
   later touch of it is visible as use-after-free. *)
Definition Heap := Blk -> option Block.

Definition empty_heap : Heap := fun _ => None.

Definition set (h : Heap) (b : Blk) (x : Block) : Heap :=
  fun b' => if Nat.eqb b' b then Some x else h b'.

Definition mark (f : Z -> bool) (o w : Z) : Z -> bool :=
  fun i => if (o <=? i) && (i <? o + w) then true else f i.

(* The permissive update: every event takes effect. *)
Definition step (h : Heap) (e : Ev) : Heap :=
  match e with
  | EAlloc b n k => set h b (mkBlock n k 1 (fun _ => false))
  | EStore b o w _ =>
      match h b with
      | Some x => set h b (mkBlock (size x) (repr x) (rc x) (mark (init x) o w))
      | None => h
      end
  | ELoad _ _ _ _ => h
  | EInc b =>
      match h b with
      | Some x => set h b (mkBlock (size x) (repr x) (rc x + 1) (init x))
      | None => h
      end
  | EDec b =>
      match h b with
      | Some x => if 0 <? rc x then set h b (mkBlock (size x) (repr x) (rc x - 1) (init x)) else h
      | None => h
      end
  end.

Definition live (h : Heap) (b : Blk) : Prop :=
  exists x, h b = Some x /\ 0 < rc x.

(* ── The four clauses, per event ──────────────────────────────────────── *)

Definition in_bounds (x : Block) (o w : Z) : Prop :=
  0 <= o /\ 0 < w /\ o + w <= size x.

Definition spatial_ev (h : Heap) (e : Ev) : Prop :=
  match e with
  | EStore b o w _ | ELoad b o w _ => exists x, h b = Some x /\ in_bounds x o w
  | _ => True
  end.

Definition temporal_ev (h : Heap) (e : Ev) : Prop :=
  match e with
  | EAlloc b _ _ => ~ live h b
  | EStore b _ _ _ | ELoad b _ _ _ | EInc b | EDec b => live h b
  end.

Definition all_init (f : Z -> bool) (o w : Z) : Prop :=
  forall i, o <= i < o + w -> f i = true.

Definition init_ev (h : Heap) (e : Ev) : Prop :=
  match e with
  | ELoad b o w _ => exists x, h b = Some x /\ all_init (init x) o w
  | _ => True
  end.

Definition type_ev (h : Heap) (e : Ev) : Prop :=
  match e with
  | EStore b _ _ k | ELoad b _ _ k => exists x, h b = Some x /\ repr x = k
  | _ => True
  end.

(* A trace satisfies a per-event predicate when every event does, at the
   state its prefix produced. *)
Fixpoint holds (P : Heap -> Ev -> Prop) (h : Heap) (t : list Ev) : Prop :=
  match t with
  | [] => True
  | e :: t' => P h e /\ holds P (step h e) t'
  end.

Definition safe_ev (h : Heap) (e : Ev) : Prop :=
  spatial_ev h e /\ temporal_ev h e /\ init_ev h e /\ type_ev h e.

(* THE DEFINITION. *)
Definition MemorySafe (t : list Ev) : Prop := holds safe_ev empty_heap t.

Definition Spatial (t : list Ev) : Prop := holds spatial_ev empty_heap t.
Definition Temporal (t : list Ev) : Prop := holds temporal_ev empty_heap t.
Definition Initialized (t : list Ev) : Prop := holds init_ev empty_heap t.
Definition WellTyped (t : list Ev) : Prop := holds type_ev empty_heap t.

Lemma holds_and : forall P Q t h,
  holds (fun h e => P h e /\ Q h e) h t <-> holds P h t /\ holds Q h t.
Proof.
  intros P Q t. induction t as [| e t IH]; intro h; simpl.
  - tauto.
  - rewrite IH. tauto.
Qed.

Theorem memory_safe_iff_clauses : forall t,
  MemorySafe t <-> Spatial t /\ Temporal t /\ Initialized t /\ WellTyped t.
Proof.
  intro t. unfold MemorySafe, Spatial, Temporal, Initialized, WellTyped, safe_ev.
  rewrite holds_and. rewrite holds_and. rewrite holds_and. tauto.
Qed.
Print Assumptions memory_safe_iff_clauses.

(* ── The checker K over traces ────────────────────────────────────────── *)

Definition liveb (h : Heap) (b : Blk) : bool :=
  match h b with Some x => 0 <? rc x | None => false end.

Definition in_boundsb (x : Block) (o w : Z) : bool :=
  (0 <=? o) && (0 <? w) && (o + w <=? size x).

(* `all_initb f o n` checks the n bytes o, o+1, ..., o+n-1. *)
Fixpoint all_initb (f : Z -> bool) (o : Z) (n : nat) : bool :=
  match n with
  | O => true
  | S n' => f o && all_initb f (o + 1) n'
  end.

Definition check_ev (h : Heap) (e : Ev) : bool :=
  match e with
  | EAlloc b _ _ => negb (liveb h b)
  | EInc b | EDec b => liveb h b
  | EStore b o w k =>
      match h b with
      | Some x => (0 <? rc x) && in_boundsb x o w && Nat.eqb (repr x) k
      | None => false
      end
  | ELoad b o w k =>
      match h b with
      | Some x => (0 <? rc x) && in_boundsb x o w && Nat.eqb (repr x) k
                  && all_initb (init x) o (Z.to_nat w)
      | None => false
      end
  end.

Fixpoint check_from (h : Heap) (t : list Ev) : bool :=
  match t with
  | [] => true
  | e :: t' => check_ev h e && check_from (step h e) t'
  end.

Definition check_trace (t : list Ev) : bool := check_from empty_heap t.

Lemma liveb_spec : forall h b, liveb h b = true <-> live h b.
Proof.
  intros h b. unfold liveb, live. destruct (h b) as [x |].
  - split.
    + intro H. exists x. split; [reflexivity | apply Z.ltb_lt; exact H].
    + intros [y [Hy Hr]]. inversion Hy; subst. apply Z.ltb_lt. exact Hr.
  - split; [discriminate | intros [y [Hy _]]; discriminate].
Qed.

Lemma in_boundsb_spec : forall x o w, in_boundsb x o w = true <-> in_bounds x o w.
Proof.
  intros x o w. unfold in_boundsb, in_bounds.
  rewrite !andb_true_iff, Z.leb_le, Z.ltb_lt, Z.leb_le. tauto.
Qed.

Lemma all_initb_spec : forall f n o,
  all_initb f o n = true <-> (forall i, o <= i < o + Z.of_nat n -> f i = true).
Proof.
  intros f n. induction n as [| n IH]; intro o; simpl.
  - split; [intros _ i Hi; lia | reflexivity].
  - rewrite andb_true_iff, IH. split.
    + intros [H0 Hr] i Hi.
      destruct (Z.eq_dec i o) as [-> | Hne]; [exact H0 |].
      apply Hr. lia.
    + intro H. split.
      * apply H. lia.
      * intros i Hi. apply H. lia.
Qed.

Lemma all_init_spec : forall f o w,
  all_initb f o (Z.to_nat w) = true <-> (0 < w -> all_init f o w) /\ (w <= 0 -> True).
Proof.
  intros f o w. rewrite all_initb_spec. unfold all_init.
  destruct (Z_lt_le_dec 0 w) as [Hw | Hw].
  - rewrite Z2Nat.id by lia. split; [intro H; split; [intros _; exact H | trivial] | intros [H _]; apply H; exact Hw].
  - assert (Hz : Z.to_nat w = O) by (destruct w; simpl; try reflexivity; lia).
    rewrite Hz. simpl. split; [intros _; split; [intro; lia | trivial] | intros _ i Hi; lia].
Qed.

Lemma check_ev_spec : forall h e, check_ev h e = true <-> safe_ev h e.
Proof.
  intros h e. unfold safe_ev.
  destruct e as [b n k | b o w k | b o w k | b | b]; simpl.
  - (* EAlloc *)
    rewrite negb_true_iff. split.
    + intro H. split; [trivial |]. split; [| split; trivial].
      intro Hl. apply liveb_spec in Hl. congruence.
    + intros [_ [Hn _]]. destruct (liveb h b) eqn:E; [| reflexivity].
      exfalso. apply Hn. apply liveb_spec. exact E.
  - (* EStore *)
    unfold live. destruct (h b) as [x |] eqn:E.
    + rewrite !andb_true_iff, Z.ltb_lt, in_boundsb_spec, Nat.eqb_eq. split.
      * intros [[Hr Hb] Hk]. split; [exists x; split; [reflexivity | exact Hb] |].
        split; [exists x; split; [reflexivity | exact Hr] |].
        split; [trivial | exists x; split; [reflexivity | exact Hk]].
      * intros [[y [Hy Hb]] [[y' [Hy' Hr]] [_ [y'' [Hy'' Hk]]]]].
        inversion Hy; inversion Hy'; inversion Hy''; subst. tauto.
    + split; [discriminate |].
      intros [[y [Hy _]] _]. discriminate.
  - (* ELoad *)
    unfold live. destruct (h b) as [x |] eqn:E.
    + rewrite !andb_true_iff, Z.ltb_lt, in_boundsb_spec, Nat.eqb_eq, all_init_spec. split.
      * intros [[[Hr Hb] Hk] [Hi _]].
        split; [exists x; split; [reflexivity | exact Hb] |].
        split; [exists x; split; [reflexivity | exact Hr] |].
        split; [exists x; split; [reflexivity | apply Hi; destruct Hb as [_ [Hw _]]; exact Hw] |].
        exists x; split; [reflexivity | exact Hk].
      * intros [[y [Hy Hb]] [[y' [Hy' Hr]] [[y'' [Hy'' Hi]] [z [Hz Hk]]]]].
        inversion Hy; inversion Hy'; inversion Hy''; inversion Hz; subst.
        split; [tauto | split; [intros _; exact Hi | trivial]].
    + split; [discriminate |].
      intros [[y [Hy _]] _]. discriminate.
  - (* EInc *)
    rewrite liveb_spec. tauto.
  - (* EDec *)
    rewrite liveb_spec. tauto.
Qed.

Lemma check_from_spec : forall t h, check_from h t = true <-> holds safe_ev h t.
Proof.
  intro t. induction t as [| e t IH]; intro h; simpl.
  - tauto.
  - rewrite andb_true_iff, check_ev_spec, IH. tauto.
Qed.

(* K accepts ==> MemorySafe. *)
Theorem check_trace_sound : forall t, check_trace t = true -> MemorySafe t.
Proof. intros t H. apply check_from_spec. exact H. Qed.
Print Assumptions check_trace_sound.

(* And K rejects nothing that is memory safe. *)
Theorem check_trace_complete : forall t, MemorySafe t -> check_trace t = true.
Proof. intros t H. apply check_from_spec. exact H. Qed.
Print Assumptions check_trace_complete.

(* ── Negative controls: one trace per clause, each rejected by K ─────── *)

(* Block 0, representations 1 and 2 (nat literals; offsets stay in Z). *)
Definition b0 : Blk := 0%nat.
Definition r1 : Repr := 1%nat.
Definition r2 : Repr := 2%nat.

(* A safe trace: allocate 4 bytes, write them, read them, release. *)
Example accepts_safe :
  check_trace [EAlloc b0 4 r1; EStore b0 0 4 r1; ELoad b0 0 4 r1; EDec b0] = true.
Proof. reflexivity. Qed.

(* SPATIAL — the #4043 shape: a 4-byte block read for 4096 bytes. *)
Example rejects_out_of_bounds :
  check_trace [EAlloc b0 4 r1; EStore b0 0 4 r1; ELoad b0 0 4096 r1] = false.
Proof. reflexivity. Qed.

(* TEMPORAL — a read after the last reference is released. *)
Example rejects_use_after_free :
  check_trace [EAlloc b0 4 r1; EStore b0 0 4 r1; EDec b0; ELoad b0 0 4 r1] = false.
Proof. reflexivity. Qed.

(* TEMPORAL — a second release of the same block. *)
Example rejects_double_free :
  check_trace [EAlloc b0 4 r1; EDec b0; EDec b0] = false.
Proof. reflexivity. Qed.

(* TEMPORAL — an inc that brings a freed block back. The RC checker alone
   cannot see this one (below). *)
Example rejects_resurrection :
  check_trace [EAlloc b0 4 r1; EDec b0; EInc b0] = false.
Proof. reflexivity. Qed.

(* INITIALIZATION — a read of bytes nothing wrote. *)
Example rejects_uninitialized_read :
  check_trace [EAlloc b0 4 r1; EStore b0 0 2 r1; ELoad b0 0 4 r1] = false.
Proof. reflexivity. Qed.

(* TYPE — a block allocated as representation 1 read as representation 2. *)
Example rejects_repr_confusion :
  check_trace [EAlloc b0 4 r1; EStore b0 0 4 r1; ELoad b0 0 4 r2] = false.
Proof. reflexivity. Qed.

(* ── What the existing RC checker covers ──────────────────────────────── *)

(* Each block's events, projected onto OwnershipChecker's alphabet: an
   allocation is a fresh acquire, an inc an ALIAS (a new reference to a
   block that must already exist), a dec a release, a load or store a
   borrow. *)
Definition proj_ev (b : Blk) (e : Ev) : list Op :=
  match e with
  | EAlloc b' _ _ => if Nat.eqb b' b then [Inc] else []
  | EInc b' => if Nat.eqb b' b then [Alias] else []
  | EDec b' => if Nat.eqb b' b then [Dec] else []
  | EStore b' _ _ _ | ELoad b' _ _ _ => if Nat.eqb b' b then [Borrow] else []
  end.

Definition proj (b : Blk) (t : list Ev) : list Op := flat_map (proj_ev b) t.

(* The production checker does not fold that stream as it is: `guard_line`
   (OwnershipChecker.v, #3229) puts a liveness probe before every alias of
   an owned object, so an alias of a freed one faults. `gproj` is the stream
   with that guard in place. *)
Definition gproj_ev (b : Blk) (e : Ev) : list Op := guard_ops (proj_ev b e).
Definition gproj (b : Blk) (t : list Ev) : list Op := flat_map (gproj_ev b) t.

(* The one fact the RC checker takes as given: an allocation hands out a
   block that is not live. That is the allocator's job, proven of the
   runtime's free list by FreeList.alloc_not_live. *)
Definition fresh_alloc_ev (h : Heap) (e : Ev) : Prop :=
  match e with
  | EAlloc b _ _ => ~ live h b
  | _ => True
  end.

(* TEMPORAL for every event but the allocation. *)
Definition temporal_use_ev (h : Heap) (e : Ev) : Prop :=
  match e with
  | EAlloc _ _ _ => True
  | _ => temporal_ev h e
  end.

Definition rc_of (h : Heap) (b : Blk) : Z :=
  match h b with Some x => rc x | None => 0 end.

Definition ev_blk (e : Ev) : Blk :=
  match e with
  | EAlloc b _ _ | EStore b _ _ _ | ELoad b _ _ _ | EInc b | EDec b => b
  end.

Lemma exec_app : forall l1 l2 r,
  exec (l1 ++ l2) r = match exec l1 r with Some r' => exec l2 r' | None => None end.
Proof.
  intro l1. induction l1 as [| op l1 IH]; intros l2 r; simpl; [reflexivity |].
  destruct op.
  - apply IH.
  - apply IH.
  - destruct (r <=? 0); [reflexivity | apply IH].
  - destruct (r <=? 0); [reflexivity | apply IH].
  - destruct (Z.eqb r 1); [apply IH | reflexivity].
  - destruct (r <=? 0); [reflexivity | apply IH].
Qed.

(* OwnershipChecker's count never goes below 0 from a count at or above it. *)
Lemma exec_nonneg : forall ops r r', 0 <= r -> exec ops r = Some r' -> 0 <= r'.
Proof.
  intro ops. induction ops as [| op ops IH]; intros r r' Hr Hx; simpl in Hx.
  - injection Hx as <-. exact Hr.
  - destruct op.
    + apply (IH (r + 1)); [lia | exact Hx].
    + apply (IH (r + 1)); [lia | exact Hx].
    + destruct (r <=? 0) eqn:E; [discriminate |]. apply Z.leb_gt in E.
      apply (IH (r - 1)); [lia | exact Hx].
    + destruct (r <=? 0) eqn:E; [discriminate |]. apply Z.leb_gt in E.
      apply (IH (r - 1)); [lia | exact Hx].
    + destruct (Z.eqb r 1); [apply (IH 0); [lia | exact Hx] | discriminate].
    + destruct (r <=? 0); [discriminate | apply (IH r); [exact Hr | exact Hx]].
Qed.

Lemma rc_of_set_same : forall h b x, rc_of (set h b x) b = rc x.
Proof. intros. unfold rc_of, set. rewrite Nat.eqb_refl. reflexivity. Qed.

Lemma rc_of_set_other : forall h b b' x, b' <> b -> rc_of (set h b x) b' = rc_of h b'.
Proof.
  intros h b b' x Hne. unfold rc_of, set.
  destruct (Nat.eqb b' b) eqn:E; [apply Nat.eqb_eq in E; contradiction | reflexivity].
Qed.

Lemma rc_of_some : forall h b x, h b = Some x -> rc_of h b = rc x.
Proof. intros h b x E. unfold rc_of. rewrite E. reflexivity. Qed.

Lemma rc_of_none : forall h b, h b = None -> rc_of h b = 0.
Proof. intros h b E. unfold rc_of. rewrite E. reflexivity. Qed.

Lemma rc_of_step_other : forall h e b, b <> ev_blk e -> rc_of (step h e) b = rc_of h b.
Proof.
  intros h e b Hne. destruct e as [b0 n k | b0 o w k | b0 o w k | b0 | b0]; simpl in *.
  - apply rc_of_set_other. exact Hne.
  - destruct (h b0); [apply rc_of_set_other; exact Hne | reflexivity].
  - reflexivity.
  - destruct (h b0); [apply rc_of_set_other; exact Hne | reflexivity].
  - destruct (h b0) as [x |]; [| reflexivity].
    destruct (0 <? rc x); [apply rc_of_set_other; exact Hne | reflexivity].
Qed.

Lemma gproj_ev_other : forall e b, b <> ev_blk e -> gproj_ev b e = [].
Proof.
  intros e b Hne. unfold gproj_ev. destruct e; simpl in *;
    (destruct (Nat.eqb _ b) eqn:E; [apply Nat.eqb_eq in E; congruence | reflexivity]).
Qed.

(* On the block an event names, the guarded op moves OwnershipChecker's
   count exactly as the event moves the heap's whenever it does not fault,
   and not faulting means the event touched a live block. *)
Lemma step_named : forall h e,
  fresh_alloc_ev h e -> (forall b, 0 <= rc_of h b) ->
  exec (gproj_ev (ev_blk e) e) (rc_of h (ev_blk e)) <> None ->
  exec (gproj_ev (ev_blk e) e) (rc_of h (ev_blk e)) = Some (rc_of (step h e) (ev_blk e))
  /\ temporal_use_ev h e.
Proof.
  intros h e Hw Hn Hx. unfold gproj_ev in *.
  destruct e as [b n k | b o w k | b o w k | b | b]; simpl in *;
    rewrite Nat.eqb_refl in *; simpl in *.
  - (* EAlloc: the block is not live, so its count is 0 and becomes 1. *)
    split; [| trivial]. rewrite rc_of_set_same. simpl. f_equal.
    specialize (Hn b). destruct (h b) as [x |] eqn:E.
    + rewrite (rc_of_some h b x E) in *.
      destruct (Z_lt_le_dec 0 (rc x)) as [Hp | Hp]; [| lia].
      exfalso. apply Hw. exists x. split; [exact E | exact Hp].
    + rewrite (rc_of_none h b E). reflexivity.
  - (* EStore: a borrow; the count is unchanged and must be positive. *)
    destruct (h b) as [x |] eqn:E.
    + rewrite (rc_of_some h b x E) in *. rewrite rc_of_set_same. simpl.
      destruct (rc x <=? 0) eqn:Er; [contradiction |].
      apply Z.leb_gt in Er. split; [reflexivity | exists x; split; [exact E | exact Er]].
    + rewrite (rc_of_none h b E) in *. simpl in Hx. contradiction.
  - (* ELoad *)
    destruct (h b) as [x |] eqn:E.
    + rewrite (rc_of_some h b x E) in *.
      destruct (rc x <=? 0) eqn:Er; [contradiction |].
      apply Z.leb_gt in Er. split; [reflexivity | exists x; split; [exact E | exact Er]].
    + rewrite (rc_of_none h b E) in *. simpl in Hx. contradiction.
  - (* EInc: the guard's probe needs a live block; then both add 1. *)
    destruct (h b) as [x |] eqn:E.
    + rewrite (rc_of_some h b x E) in *.
      destruct (rc x <=? 0) eqn:Er; [contradiction |].
      apply Z.leb_gt in Er. rewrite rc_of_set_same. simpl.
      split; [reflexivity | exists x; split; [exact E | exact Er]].
    + rewrite (rc_of_none h b E) in *. simpl in Hx. contradiction.
  - (* EDec: the count must be positive; then both take 1 away. *)
    destruct (h b) as [x |] eqn:E.
    + rewrite (rc_of_some h b x E) in *.
      destruct (rc x <=? 0) eqn:Er; [contradiction |].
      apply Z.leb_gt in Er.
      assert (Hlt : (0 <? rc x) = true) by (apply Z.ltb_lt; exact Er). rewrite Hlt.
      rewrite rc_of_set_same. simpl.
      split; [reflexivity | exists x; split; [exact E | exact Er]].
    + rewrite (rc_of_none h b E) in *. simpl in Hx. contradiction.
Qed.

Lemma nonneg_step : forall h e,
  fresh_alloc_ev h e -> (forall b, 0 <= rc_of h b) ->
  exec (gproj_ev (ev_blk e) e) (rc_of h (ev_blk e)) <> None ->
  forall b, 0 <= rc_of (step h e) b.
Proof.
  intros h e Hw Hn Hx b.
  destruct (Nat.eq_dec b (ev_blk e)) as [-> | Hne];
    [| rewrite rc_of_step_other by exact Hne; apply Hn].
  destruct (step_named h e Hw Hn Hx) as [Hs _].
  exact (exec_nonneg _ _ _ (Hn (ev_blk e)) Hs).
Qed.

(* The invariant the induction carries: every block's guarded rest of the
   trace runs without fault from the count the heap holds now. *)
Lemma rc_check_from : forall t h,
  holds fresh_alloc_ev h t ->
  (forall b, 0 <= rc_of h b) ->
  (forall b, exec (gproj b t) (rc_of h b) <> None) ->
  holds temporal_use_ev h t.
Proof.
  intro t. induction t as [| e t IH]; intros h Hw Hn Hx; simpl; [trivial |].
  destruct Hw as [Hwe Hwt].
  assert (Hxe : exec (gproj_ev (ev_blk e) e) (rc_of h (ev_blk e)) <> None).
  { intro Hnone. apply (Hx (ev_blk e)). unfold gproj. simpl.
    rewrite exec_app. rewrite Hnone. reflexivity. }
  destruct (step_named h e Hwe Hn Hxe) as [Hs Hu].
  split; [exact Hu |].
  apply IH; [exact Hwt | apply nonneg_step; assumption |].
  intro b. specialize (Hx b). unfold gproj in Hx. simpl in Hx. rewrite exec_app in Hx.
  fold (gproj b t) in Hx.
  destruct (Nat.eq_dec b (ev_blk e)) as [-> | Hne].
  - rewrite Hs in Hx. exact Hx.
  - rewrite gproj_ev_other in Hx by exact Hne. simpl in Hx.
    rewrite rc_of_step_other by exact Hne. exact Hx.
Qed.

Lemma temporal_split : forall t h,
  holds temporal_ev h t <-> holds fresh_alloc_ev h t /\ holds temporal_use_ev h t.
Proof.
  intro t. induction t as [| e t IH]; intro h; simpl; [tauto |].
  rewrite IH. destruct e; simpl; tauto.
Qed.

(* THE MAP from the spine's oldest theorem into the definition: if every
   block's guarded stream is accepted by OwnershipChecker.check, and the
   allocator hands out blocks that are not live, the trace is TEMPORAL —
   no use-after-free, no double free, no resurrection. *)
Theorem rc_check_gives_temporal : forall t,
  holds fresh_alloc_ev empty_heap t ->
  (forall b, check (gproj b t) = true) ->
  Temporal t.
Proof.
  intros t Hw Hc. unfold Temporal. apply temporal_split. split; [exact Hw |].
  apply rc_check_from; [exact Hw | intro b; unfold rc_of, empty_heap; lia |].
  intros b Hnone. specialize (Hc b). unfold check, run in Hc.
  unfold rc_of, empty_heap in Hnone. rewrite Hnone in Hc. discriminate.
Qed.
Print Assumptions rc_check_gives_temporal.

(* And why the guard is needed: the UNGUARDED fold accepts the resurrection
   of a freed block, which is not memory safe. This is #3229's witness, here
   as a theorem against the definition. *)
Theorem unguarded_rc_check_misses_resurrection :
  let t := [EAlloc b0 4 r1; EDec b0; EInc b0; EDec b0] in
  (forall b, check (proj b t) = true) /\ ~ MemorySafe t.
Proof.
  simpl. split.
  - intro b. unfold proj, b0. simpl. destruct b; reflexivity.
  - intro H. apply check_trace_complete in H. discriminate H.
Qed.
Print Assumptions unguarded_rc_check_misses_resurrection.

(* The guarded fold rejects the same trace. *)
Example guarded_rc_check_rejects_resurrection :
  check (gproj b0 [EAlloc b0 4 r1; EDec b0; EInc b0; EDec b0]) = false.
Proof. reflexivity. Qed.
