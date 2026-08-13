(* Almide v1 trust spine — A1.2, the COUNT half of reuse (#909, sentinel 3 of 3:
   "reuse restores rc=1").

   `FreeList.v` models the allocator over ADDRESSES: which block is free, which is
   live, which is pinned. That is enough for reuse-SAFETY (`alloc_not_live`: a
   validated allocation is never a currently-live block) but it says nothing about
   the number the reused block starts life with — #909's own words: "the rc side of
   reuse is still only guarded by the churn + byte gates; FreeList.v tracks
   addresses, not counts."

   This companion puts the COUNT in scope. It does NOT invent a fresh abstraction
   for it: the count is `RuntimeModel.read_rc m base` — the very cell whose
   instruction-tree and BYTE semantics are already bound to the emitted artifact
   (`WasmRcDec.rc_dec_prog_realizes_rt_dec`, `WasmExec.rc_dec_bytes_trap_on_zero`,
   `WasmExec.rc_dec_bytes_frees_when_one`). So the state modelled here is exactly
   FreeList's allocator PLUS RuntimeModel's linear memory, and nothing else.

   ── The renderer this is a model OF ───────────────────────────────────────
   Reuse in the emitted wasm is TWO instructions, from TWO different producers:

     (local.set $d (call $alloc n))                         ; the ALLOCATOR
     (i32.store (i32.add (local.get $d) (i32.const 0)) (i32.const 1))   ; the RENDERER

   `$alloc` (render_wasm_p3.rs) returns a raw block — the fresh bump frontier, or a
   block unlinked from the free-list. It never writes the rc cell. The `1` is
   written by the CONSTRUCTOR the renderer emits around it: `$list_new`
   (render_wasm_p3.rs), and the `Alloc` arms of render_wasm_p2.rs — every one of
   them stores `RC_INITIAL` at `LIST_RC_OFFSET` on the instruction after the call.

   So "reuse restores rc=1" is not an allocator property at all; it is a property
   of the PAIR, and this file proves the pair, keeping the two halves separate and
   naming what each contributes:

     ALLOCATOR half  — a block handed back off the FREE-LIST carries no stale
                       count: it arrives at exactly 0 (`reuse_hands_back_a_zero_
                       count_block`). This is the substantive half, and it is a
                       consequence of the invariant, not of the store.
     RENDERER half   — the constructor's store makes it exactly `RC_INITIAL` = 1
                       (`reuse_restores_rc_1`).

   And the pair is NECESSARY, not belt-and-braces: with the allocator alone (the
   store omitted) the reused block comes back at 0 and the FIRST release traps —
   `omitting_the_rc_store_traps_on_the_first_release` runs exactly that trace. The
   model sees the missing half.

   What stays trusted is stated at the bottom of the file, precisely: that the
   emitted wasm really is this pair at every allocation site is the renderer
   contract (proofs/TRUSTED_BASE.md §3), gated executably by
   `almide-mir`'s `every_rc_managed_alloc_site_initializes_the_rc_cell` — a proof
   cannot reach into the emitter, but a gate can, and the gate is named here so the
   boundary is visible rather than implied. *)

From AlmideTrust Require Import FreeList RuntimeModel.
From Stdlib Require Import ZArith.
From Stdlib Require Import Lia.
Open Scope Z_scope.

(* The renderer's initialization constant: `const RC_INITIAL: i32 = 1`
   (crates/almide-mir/src/render_wasm.rs). A plain Definition — the trusted base
   gains no axiom — and the Rust-side gate parses THIS line and fails if the
   renderer's constant ever drifts from it, so the number is not hand-copied. *)
Definition RC_INITIAL : Z := 1.

(* ══════════════════════════════════════════════════════════════════════════
   THE STATE: FreeList's allocator + RuntimeModel's linear memory.

   `AState` carries the frontier, the free-list and the ghost live set; `Mem`
   carries the rc CELLS. Nothing else is added: the rc of a block is read out of
   the same memory, at the same offset, that the byte-level theorems write. *)

Record RState := { ra : AState ; rmem : Mem }.

Definition rc_at (rs : RState) (p : Z) : Z := read_rc (rmem rs) p.

(* THE INVARIANT the whole file is about. On top of FreeList's `INV` (free and
   live disjoint, both below the frontier) it pins the two count facts:

     a block ON THE FREE-LIST reads 0   — no stale reference survived the trip,
                                          and the `$rc_dec` sentinel therefore
                                          traps a double release;
     a LIVE block reads at least 1      — the ghost live set is REALIZED by the
                                          rc cell, not merely accompanied by it. *)
Definition RINV (rs : RState) : Prop :=
  INV (ra rs)
  /\ (forall x, freeS (ra rs) x = true -> rc_at rs x = 0)
  /\ (forall x, liveS (ra rs) x = true -> 1 <= rc_at rs x).

(* Distinct blocks have distinct rc cells: the cells sit at `base + RC_OFFSET`,
   so a write to one block's cell leaves every other block's cell alone. (The
   only writes in this model are to rc cells of block BASES — payload writes are
   the scratch case, handled explicitly at the end.) *)
Lemma rc_upd_other : forall m p q v,
  q <> p -> read_rc (upd m (p + RC_OFFSET) v) q = read_rc m q.
Proof.
  intros m p q v Hne. unfold read_rc, upd.
  destruct (Z.eqb (q + RC_OFFSET) (p + RC_OFFSET)) eqn:E; [ | reflexivity ].
  apply Z.eqb_eq in E. exfalso. apply Hne. lia.
Qed.

(* Projection shorthands, so the proofs below never have to `simpl` a record
   apart (which would also unfold the memory operations and lose the rewrite
   targets). *)
Lemma rc_at_mk : forall a m p, rc_at {| ra := a ; rmem := m |} p = read_rc m p.
Proof. reflexivity. Qed.

Lemma maskS_true : forall s n x, maskS s n x = true -> s x = true.
Proof.
  intros s n x H. unfold maskS in H. destruct (Z.ltb x n); [ exact H | discriminate H ].
Qed.

(* ══════════════════════════════════════════════════════════════════════════
   THE OPERATIONS, one per emitted primitive. *)

(* `(call $alloc n)` — the ALLOCATOR ALONE. Exactly `FreeList.alloc` (validate the
   runtime's chosen block: the fresh frontier, or a free-list entry), and it does
   NOT touch memory: `$alloc`'s body writes the frontier, the free-list link and
   the page count, never the rc cell. *)
Definition r_alloc (rs : RState) (p : Z) : option RState :=
  match alloc (ra rs) p with
  | Some a' => Some {| ra := a' ; rmem := rmem rs |}
  | None => None
  end.

(* `(i32.store (i32.add $d (i32.const RC_OFFSET)) (i32.const RC_INITIAL))` — the
   CONSTRUCTOR's rc-initializing store, the renderer's half. *)
Definition rc_init (rs : RState) (p : Z) : RState :=
  {| ra := ra rs ; rmem := upd (rmem rs) (p + RC_OFFSET) RC_INITIAL |}.

(* The PAIR: what an `Op::Alloc` / `$list_new` actually emits. *)
Definition r_new (rs : RState) (p : Z) : option RState :=
  match r_alloc rs p with
  | Some rs' => Some (rc_init rs' p)
  | None => None
  end.

Lemma rc_at_rc_init_same : forall rs p, rc_at (rc_init rs p) p = RC_INITIAL.
Proof. intros rs p. unfold rc_at, rc_init; simpl. apply read_upd_same. Qed.

Lemma rc_at_rc_init_other : forall rs p x, x <> p -> rc_at (rc_init rs p) x = rc_at rs x.
Proof. intros rs p x H. unfold rc_at, rc_init; simpl. apply (rc_upd_other (rmem rs) p x _ H). Qed.

(* `$rc_inc` — a raw increment of the cell. The emitted body has no guard; the
   guarantee that it is only ever reached through a LIVE handle is the ownership
   checker's (`OwnershipChecker.check_sound`'s no-use-after-free), so in the PCC
   framing this model VALIDATES that precondition rather than assuming it. *)
Definition r_inc (rs : RState) (p : Z) : option RState :=
  if liveS (ra rs) p
  then Some {| ra := ra rs ; rmem := rt_inc (rmem rs) p |}
  else None.

(* `$rc_dec` — verbatim: load the cell, TRAP if it is already 0 (the double-free
   sentinel, which is exactly `RuntimeModel.rt_dec`'s None), store cell-1, and if
   that reached 0 push the block onto the free-list. The emitted test is
   `(i32.eqz $rc)` on the DECREMENTED value; "the new value is 0" and "the old
   value was 1" are the same condition, and the latter is written here because it
   does not have to look inside the memory the store just produced. The push is
   `FreeList.free_op`, which validates that the block really was live — the ghost
   half of the same event. *)
Definition r_dec (rs : RState) (p : Z) : option RState :=
  match rt_dec (rmem rs) p with
  | None => None
  | Some m' =>
      if Z.eqb (rc_at rs p) 1
      then match free_op (ra rs) p with
           | Some a' => Some {| ra := a' ; rmem := m' |}
           | None => None
           end
      else Some {| ra := ra rs ; rmem := m' |}
  end.

(* The two shapes `r_dec` can take, as rewrite rules — so no proof below has to
   take apart a match on a memory operation. *)
Lemma rt_dec_nonpos : forall m p, read_rc m p <= 0 -> rt_dec m p = None.
Proof.
  intros m p H. unfold rt_dec.
  replace (Z.leb (read_rc m p) 0) with true by (symmetry; apply Z.leb_le; exact H).
  reflexivity.
Qed.

Lemma rt_dec_pos : forall m p,
  1 <= read_rc m p -> rt_dec m p = Some (upd m (p + RC_OFFSET) (read_rc m p - 1)).
Proof.
  intros m p H. unfold rt_dec.
  replace (Z.leb (read_rc m p) 0) with false by (symmetry; apply Z.leb_gt; lia).
  reflexivity.
Qed.

Lemma r_dec_nonpos : forall rs p, rc_at rs p <= 0 -> r_dec rs p = None.
Proof.
  intros rs p H. unfold r_dec. rewrite (rt_dec_nonpos (rmem rs) p H). reflexivity.
Qed.

Lemma r_dec_pos : forall rs p, 1 <= rc_at rs p ->
  r_dec rs p =
    (if Z.eqb (rc_at rs p) 1
     then match free_op (ra rs) p with
          | Some a' => Some {| ra := a' ;
                               rmem := upd (rmem rs) (p + RC_OFFSET) (rc_at rs p - 1) |}
          | None => None
          end
     else Some {| ra := ra rs ;
                  rmem := upd (rmem rs) (p + RC_OFFSET) (rc_at rs p - 1) |}).
Proof.
  intros rs p H. unfold r_dec. rewrite (rt_dec_pos (rmem rs) p H). reflexivity.
Qed.

(* Boot: an empty allocator over ZERO-INITIALIZED linear memory (wasm's, and
   `memory.grow`'s, defined initial content). *)
Definition zero_mem : Mem := fun _ => 0.
Definition r_init (b : Z) : RState := {| ra := init b ; rmem := zero_mem |}.

Lemma r_init_RINV : forall b, RINV (r_init b).
Proof.
  intros b. unfold RINV, r_init; simpl. split; [ apply init_INV | ].
  unfold init, emptyS; simpl.
  split; intros x H; discriminate H.
Qed.

(* ══════════════════════════════════════════════════════════════════════════
   THE ALLOCATOR HALF — a reused block carries NO stale count.

   This is the part that is genuinely about the allocator, and it is where the
   invariant does the work: the block was pushed onto the free-list by an
   `$rc_dec` that had just written 0 into its cell, and `$alloc` does not write
   the cell, so it comes back at 0. Nothing in between can raise it: an inc needs
   a live block, and a free block is not live. *)
Theorem reuse_hands_back_a_zero_count_block : forall rs p rs',
  RINV rs -> freeS (ra rs) p = true -> r_alloc rs p = Some rs' -> rc_at rs' p = 0.
Proof.
  intros rs p rs' [_ [Hf _]] Hfree Ha. unfold r_alloc in Ha.
  destruct (alloc (ra rs) p); [ | discriminate ]. injection Ha as <-.
  unfold rc_at; simpl. apply (Hf p Hfree).
Qed.

(* The FRESH-frontier path is deliberately NOT claimed to arrive at 0. Memory
   above the frontier is zero at boot, but a region reset hands the same addresses
   back with whatever counts the reclaimed objects last held, and an un-rc'd
   scratch allocation (below) clobbers its own cell with payload bytes. The
   frontier block's cell is MEANINGLESS until the constructor writes it — which is
   precisely why the renderer's store is not redundant. *)

(* ══════════════════════════════════════════════════════════════════════════
   THE PAIR — #909's third sentinel. *)

(* REUSE RESTORES rc = 1, in full: the validated allocation hands back a block
   that is not live and (on the reuse path) carries no stale count, the
   constructor's store leaves it at EXACTLY 1, the block is live afterwards, and
   the whole count discipline is re-established. *)
Theorem reuse_restores_rc_1 : forall rs p rs',
  RINV rs -> r_new rs p = Some rs' ->
  rc_at rs' p = RC_INITIAL
  /\ liveS (ra rs) p = false
  /\ (freeS (ra rs) p = true -> rc_at rs p = 0)
  /\ liveS (ra rs') p = true
  /\ RINV rs'.
Proof.
  intros rs p rs' HR Hn.
  destruct HR as [HINV [Hfree Hlive]].
  unfold r_new, r_alloc in Hn.
  destruct (alloc (ra rs) p) as [a'|] eqn:Ea; [ | discriminate ].
  injection Hn as <-.
  assert (Hnl : liveS (ra rs) p = false) by (apply (alloc_not_live (ra rs) p a' HINV Ea)).
  assert (HINV1 : INV a') by (apply (alloc_preserves_INV (ra rs) p a' HINV Ea)).
  (* the store lands on p's cell: rc p = 1 *)
  assert (Hone : rc_at (rc_init {| ra := a' ; rmem := rmem rs |} p) p = RC_INITIAL)
    by (apply rc_at_rc_init_same).
  (* every other cell is untouched *)
  assert (Hoth : forall x, x <> p ->
                 rc_at (rc_init {| ra := a' ; rmem := rmem rs |} p) x = rc_at rs x).
  { intros x Hx. rewrite (rc_at_rc_init_other _ p x Hx). reflexivity. }
  destruct HINV as [Hdis [Hbf Hbl]].
  split; [ exact Hone | split; [ exact Hnl | split ] ].
  - intros Hpf. apply (Hfree p Hpf).
  - unfold alloc in Ea. destruct (Z.eqb p (bump (ra rs))) eqn:Ep.
    + apply Z.eqb_eq in Ep. subst p. injection Ea as <-.
      split; [ simpl; unfold addS; rewrite Z.eqb_refl; reflexivity | ].
      unfold RINV. split; [ exact HINV1 | ]. split.
      * intros x Hx. simpl in Hx. assert (Hne : x <> bump (ra rs)).
        { intros ->. apply (Z.lt_irrefl (bump (ra rs))). apply Hbf. exact Hx. }
        rewrite (Hoth x Hne). apply Hfree. exact Hx.
      * intros x Hx. simpl in Hx. unfold addS in Hx.
        destruct (Z.eqb x (bump (ra rs))) eqn:Ex.
        { apply Z.eqb_eq in Ex. subst x. rewrite Hone. unfold RC_INITIAL. lia. }
        { assert (Hne : x <> bump (ra rs)) by (intros ->; rewrite Z.eqb_refl in Ex; discriminate Ex).
          rewrite (Hoth x Hne). apply Hlive. exact Hx. }
    + destruct (freeS (ra rs) p) eqn:Ef; [ | discriminate ]. injection Ea as <-.
      split; [ simpl; unfold addS; rewrite Z.eqb_refl; reflexivity | ].
      unfold RINV. split; [ exact HINV1 | ]. split.
      * intros x Hx. simpl in Hx. unfold remS in Hx.
        destruct (Z.eqb x p) eqn:Ex; [ discriminate Hx | ].
        assert (Hne : x <> p) by (intros ->; rewrite Z.eqb_refl in Ex; discriminate Ex).
        rewrite (Hoth x Hne). apply Hfree. exact Hx.
      * intros x Hx. simpl in Hx. unfold addS in Hx. destruct (Z.eqb x p) eqn:Ex.
        { apply Z.eqb_eq in Ex. subst x. rewrite Hone. unfold RC_INITIAL. lia. }
        { assert (Hne : x <> p) by (intros ->; rewrite Z.eqb_refl in Ex; discriminate Ex).
          rewrite (Hoth x Hne). apply Hlive. exact Hx. }
Qed.

(* The two halves, split out so each can be cited on its own. *)
Theorem r_new_preserves_RINV : forall rs p rs',
  RINV rs -> r_new rs p = Some rs' -> RINV rs'.
Proof.
  intros rs p rs' HR Hn.
  destruct (reuse_restores_rc_1 rs p rs' HR Hn) as [_ [_ [_ [_ H]]]]. exact H.
Qed.

(* ══════════════════════════════════════════════════════════════════════════
   THE RC CELL REALIZES THE GHOST LIVE SET.

   FreeList's `liveS` is a ghost the runtime does not store. The invariant makes
   the rc CELL decide it for tracked blocks — which is what lets the emitted
   `$rc_dec` implement the abstract liveness precondition with a single load. *)

(* A DOUBLE RELEASE TRAPS: the block is on the free-list, so its cell reads 0, so
   `rt_dec` — i.e. the emitted `(if (i32.eqz $rc) (then (unreachable)))` — fires. *)
Theorem double_release_traps : forall rs p,
  RINV rs -> freeS (ra rs) p = true -> r_dec rs p = None.
Proof.
  intros rs p [_ [Hfree _]] Hf.
  apply r_dec_nonpos. rewrite (Hfree p Hf). lia.
Qed.

(* … and a LIVE block always releases without trapping: its cell reads at least
   1, so the sentinel does not fire, and if it reaches 0 the free-list push is
   validated (the block was live). Together with the previous theorem: on the
   tracked universe the cheap cell test and the abstract liveness test agree. *)
Theorem live_block_releases_without_trapping : forall rs p,
  RINV rs -> liveS (ra rs) p = true -> exists rs', r_dec rs p = Some rs'.
Proof.
  intros rs p [_ [_ Hlive]] Hl.
  assert (H1 : 1 <= rc_at rs p) by (apply (Hlive p Hl)).
  rewrite (r_dec_pos rs p H1). destruct (Z.eqb (rc_at rs p) 1).
  - unfold free_op. rewrite Hl. eexists. reflexivity.
  - eexists. reflexivity.
Qed.

(* The release that REACHES zero is exactly the one that frees: the block leaves
   the live set, joins the free-list, and its cell is left at 0 — which is both
   the leak-freedom fact (`WasmExec.rc_dec_bytes_frees_when_one` at the byte
   level) and the precondition the NEXT reuse needs. *)
Theorem release_at_one_frees_and_zeroes : forall rs p rs',
  RINV rs -> liveS (ra rs) p = true -> rc_at rs p = 1 -> r_dec rs p = Some rs' ->
  freeS (ra rs') p = true /\ liveS (ra rs') p = false /\ rc_at rs' p = 0.
Proof.
  intros rs p rs' _ Hl H1 Hd.
  assert (Hpos : 1 <= rc_at rs p) by lia.
  rewrite (r_dec_pos rs p Hpos) in Hd. rewrite H1 in Hd. simpl in Hd.
  unfold free_op in Hd. rewrite Hl in Hd. injection Hd as <-.
  split; [ | split ].
  - simpl. unfold addS. rewrite Z.eqb_refl. reflexivity.
  - simpl. unfold remS. rewrite Z.eqb_refl. reflexivity.
  - rewrite rc_at_mk. apply read_upd_same.
Qed.

Theorem r_dec_preserves_RINV : forall rs p rs',
  RINV rs -> r_dec rs p = Some rs' -> RINV rs'.
Proof.
  intros rs p rs' [HINV [Hfree Hlive]] Hd.
  destruct (Z.leb (rc_at rs p) 0) eqn:Ez.
  { apply Z.leb_le in Ez. rewrite (r_dec_nonpos rs p Ez) in Hd. discriminate Hd. }
  apply Z.leb_gt in Ez. assert (H1 : 1 <= rc_at rs p) by lia.
  rewrite (r_dec_pos rs p H1) in Hd.
  destruct (Z.eqb (rc_at rs p) 1) eqn:Ee.
  - (* reached 0: the block is freed *)
    apply Z.eqb_eq in Ee.
    unfold free_op in Hd. destruct (liveS (ra rs) p) eqn:El; [ | discriminate ].
    injection Hd as <-. unfold RINV.
    assert (HINV' : INV {| bump := bump (ra rs); freeS := addS (freeS (ra rs)) p;
                           liveS := remS (liveS (ra rs)) p |}).
    { apply (free_preserves_INV (ra rs) p); [ exact HINV | ].
      unfold free_op. rewrite El. reflexivity. }
    split; [ exact HINV' | ]. split.
    + intros x Hx. simpl in Hx. unfold addS in Hx. rewrite rc_at_mk.
      destruct (Z.eqb x p) eqn:Ex.
      * apply Z.eqb_eq in Ex. subst x. rewrite read_upd_same. lia.
      * assert (Hne : x <> p) by (intros ->; rewrite Z.eqb_refl in Ex; discriminate Ex).
        rewrite (rc_upd_other (rmem rs) p x _ Hne). apply (Hfree x Hx).
    + intros x Hx. simpl in Hx. unfold remS in Hx.
      destruct (Z.eqb x p) eqn:Ex; [ discriminate Hx | ].
      assert (Hne : x <> p) by (intros ->; rewrite Z.eqb_refl in Ex; discriminate Ex).
      rewrite rc_at_mk. rewrite (rc_upd_other (rmem rs) p x _ Hne). apply (Hlive x Hx).
  - (* still shared: only the cell moves *)
    injection Hd as <-. unfold RINV. split; [ exact HINV | ].
    assert (Hnz : rc_at rs p <> 1) by (apply Z.eqb_neq; exact Ee).
    assert (Hnf : freeS (ra rs) p = false).
    { destruct (freeS (ra rs) p) eqn:Ef; [ | reflexivity ].
      assert (H0 : rc_at rs p = 0) by (apply (Hfree p Ef)). lia. }
    split.
    + intros x Hx. simpl in Hx. rewrite rc_at_mk.
      assert (Hne : x <> p) by (intros ->; rewrite Hnf in Hx; discriminate Hx).
      rewrite (rc_upd_other (rmem rs) p x _ Hne). apply (Hfree x Hx).
    + intros x Hx. simpl in Hx. rewrite rc_at_mk. destruct (Z.eq_dec x p) as [->|Hne].
      * rewrite read_upd_same. lia.
      * rewrite (rc_upd_other (rmem rs) p x _ Hne). apply (Hlive x Hx).
Qed.

Theorem r_inc_preserves_RINV : forall rs p rs',
  RINV rs -> r_inc rs p = Some rs' -> RINV rs'.
Proof.
  intros rs p rs' [HINV [Hfree Hlive]] Hi. unfold r_inc in Hi.
  destruct (liveS (ra rs) p) eqn:El; [ | discriminate ]. injection Hi as <-.
  unfold RINV. split; [ exact HINV | ]. destruct HINV as [Hdis _]. split.
  - intros x Hx. simpl in Hx. rewrite rc_at_mk. unfold rt_inc.
    assert (Hne : x <> p) by (intros E; rewrite E in Hx; apply (Hdis p Hx El)).
    rewrite (rc_upd_other (rmem rs) p x _ Hne). apply (Hfree x Hx).
  - intros x Hx. simpl in Hx. rewrite rc_at_mk. unfold rt_inc.
    destruct (Z.eq_dec x p) as [->|Hne].
    + rewrite read_upd_same.
      assert (H1 : 1 <= read_rc (rmem rs) p) by (apply (Hlive p El)). lia.
    + rewrite (rc_upd_other (rmem rs) p x _ Hne). apply (Hlive x Hx).
Qed.

(* ══════════════════════════════════════════════════════════════════════════
   ARBITRARY RUNS. As in FreeList, the body is a RELATION, not a fixed trace, so
   the result holds for every program the renderer may emit. *)

Inductive r_steps : RState -> RState -> Prop :=
| r_steps_refl : forall rs, r_steps rs rs
| r_steps_new  : forall rs p rs' rs'', r_new rs p = Some rs' -> r_steps rs' rs'' -> r_steps rs rs''
| r_steps_inc  : forall rs p rs' rs'', r_inc rs p = Some rs' -> r_steps rs' rs'' -> r_steps rs rs''
| r_steps_dec  : forall rs p rs' rs'', r_dec rs p = Some rs' -> r_steps rs' rs'' -> r_steps rs rs''.

Theorem r_steps_preserve_RINV : forall rs rs', r_steps rs rs' -> RINV rs -> RINV rs'.
Proof.
  intros rs rs' Hst. induction Hst as
    [ rs0 | rs0 p rs1 rs2 H _ IH | rs0 p rs1 rs2 H _ IH | rs0 p rs1 rs2 H _ IH ]; intros HR.
  - exact HR.
  - apply IH. apply (r_new_preserves_RINV rs0 p rs1 HR H).
  - apply IH. apply (r_inc_preserves_RINV rs0 p rs1 HR H).
  - apply IH. apply (r_dec_preserves_RINV rs0 p rs1 HR H).
Qed.

(* THE PROPERTY, over an arbitrary run from boot: whatever the program did before
   — however many allocations, shares and releases — the next allocation the
   runtime chooses and the allocator validates comes back at exactly 1. *)
Theorem reuse_always_restores_rc_1 : forall b rs p rs',
  r_steps (r_init b) rs -> r_new rs p = Some rs' ->
  rc_at rs' p = RC_INITIAL /\ liveS (ra rs) p = false /\ RINV rs'.
Proof.
  intros b rs p rs' Hst Hn.
  assert (HR : RINV rs) by (apply (r_steps_preserve_RINV (r_init b) rs Hst (r_init_RINV b))).
  destruct (reuse_restores_rc_1 rs p rs' HR Hn) as [H1 [H2 [_ [_ H5]]]].
  split; [ exact H1 | split; [ exact H2 | exact H5 ] ].
Qed.

(* ══════════════════════════════════════════════════════════════════════════
   TEETH — the invariant is not vacuous and the pair is not redundant. *)

(* The FULL cycle the renderer performs, end to end, on block 0: allocate (rc 1),
   share (rc 2, `$rc_inc`), release (rc 1), release again (rc 0 → onto the
   free-list), then REUSE the freed block. The reused block comes back at 0 —
   no stale count — and leaves the constructor at 1. This is TRUSTED_BASE's
   "1→2→1→0" rc trace with the missing step, the reuse, appended. *)
Example full_cycle_reuse_restores_rc_1 :
  exists rs1 rs2 rs3 rs4 rs5,
    r_new (r_init 0) 0 = Some rs1 /\ rc_at rs1 0 = 1 /\
    r_inc rs1 0 = Some rs2 /\ rc_at rs2 0 = 2 /\
    r_dec rs2 0 = Some rs3 /\ rc_at rs3 0 = 1 /\
    r_dec rs3 0 = Some rs4 /\ rc_at rs4 0 = 0 /\ freeS (ra rs4) 0 = true /\
    r_new rs4 0 = Some rs5 /\ rc_at rs5 0 = 1 /\ liveS (ra rs5) 0 = true.
Proof.
  eexists. eexists. eexists. eexists. eexists.
  repeat split; reflexivity.
Qed.

(* … and it is well-formed throughout, by the run theorem rather than by hand. *)
Example full_cycle_is_well_formed :
  forall rs1 rs2 rs3 rs4 rs5,
    r_new (r_init 0) 0 = Some rs1 -> r_inc rs1 0 = Some rs2 ->
    r_dec rs2 0 = Some rs3 -> r_dec rs3 0 = Some rs4 -> r_new rs4 0 = Some rs5 ->
    RINV rs5.
Proof.
  intros rs1 rs2 rs3 rs4 rs5 H1 H2 H3 H4 H5.
  apply (r_steps_preserve_RINV (r_init 0) rs5); [ | apply r_init_RINV ].
  apply (r_steps_new _ 0 rs1); [ exact H1 | ].
  apply (r_steps_inc _ 0 rs2); [ exact H2 | ].
  apply (r_steps_dec _ 0 rs3); [ exact H3 | ].
  apply (r_steps_dec _ 0 rs4); [ exact H4 | ].
  apply (r_steps_new _ 0 rs5); [ exact H5 | ].
  apply r_steps_refl.
Qed.

(* THE PAIR IS NECESSARY. Run the same cycle but reuse with the ALLOCATOR ALONE —
   `$alloc` without the constructor's store, the renderer half omitted. The block
   comes back at 0 (`reuse_hands_back_a_zero_count_block` is exactly this), it is
   LIVE in the allocator's accounting, and the very first release TRAPS on the
   double-free sentinel: the object is unusable from birth. So the model SEES the
   missing store; "reuse restores rc=1" is not an accident of the allocator. *)
Example omitting_the_rc_store_traps_on_the_first_release :
  exists rs1 rs2 rs3 rs4 rs5,
    r_new (r_init 0) 0 = Some rs1 /\ r_inc rs1 0 = Some rs2 /\
    r_dec rs2 0 = Some rs3 /\ r_dec rs3 0 = Some rs4 /\
    r_alloc rs4 0 = Some rs5 /\           (* the allocator alone: NO rc store *)
    rc_at rs5 0 = 0 /\ liveS (ra rs5) 0 = true /\
    r_dec rs5 0 = None.                   (* first release: the sentinel traps *)
Proof.
  eexists. eexists. eexists. eexists. eexists.
  repeat split; reflexivity.
Qed.

(* THE OTHER CLAUSE HAS TEETH TOO. A block that reached the free-list still
   carrying a positive count — a stale reference surviving reuse — is exactly what
   RINV's first clause forbids, and forbidding it is what makes the sentinel a
   wall: in such a state the double release does NOT trap, it silently decrements
   a block that is already free. *)
Definition stale : RState :=
  {| ra := {| bump := 1 ; freeS := addS emptyS 0 ; liveS := emptyS |} ;
     rmem := upd zero_mem (0 + RC_OFFSET) 2 |}.

Example a_stale_count_defeats_the_double_free_sentinel :
  ~ RINV stale /\ freeS (ra stale) 0 = true /\ (exists rs, r_dec stale 0 = Some rs).
Proof.
  split.
  - intros [_ [Hf _]]. assert (H : rc_at stale 0 = 0) by (apply Hf; reflexivity).
    unfold rc_at, stale, read_rc, upd in H; simpl in H. discriminate H.
  - split; [ reflexivity | ]. eexists. reflexivity.
Qed.

(* ══════════════════════════════════════════════════════════════════════════
   THE UN-RC'D SCRATCH PATH — honesty about the OTHER callers of `$alloc`.

   Not every `(call $alloc n)` is followed by an rc store. `$init_args` /
   `$init_env` (render_wasm_p3.rs, render_wasm_fs_wat.rs) take raw byte scratch for
   the argv/envp vectors: no rc cell is written, the block is never released, and
   the payload bytes CLOBBER whatever sits at offset 0. Such a block is a third
   category — allocated, but neither reusable nor rc-tracked (the `$alloc`-side
   sibling of PINNED_RC).

   Modelled faithfully: the same validation as `$alloc`, the block does NOT join
   the live set, and the rc cell is overwritten with an ARBITRARY value. The
   theorem is that this cannot corrupt the count discipline FOR ANY value it
   leaves behind — because the block is in neither set the invariant constrains. *)
Definition alloc_raw (st : AState) (p : Z) : option AState :=
  if Z.eqb p (bump st) then
    Some {| bump := bump st + 1 ; freeS := freeS st ; liveS := liveS st |}
  else if freeS st p then
    Some {| bump := bump st ; freeS := remS (freeS st) p ; liveS := liveS st |}
  else None.

Definition r_alloc_scratch (rs : RState) (p v : Z) : option RState :=
  match alloc_raw (ra rs) p with
  | Some a' => Some {| ra := a' ; rmem := upd (rmem rs) (p + RC_OFFSET) v |}
  | None => None
  end.

Theorem scratch_alloc_cannot_corrupt_the_rc_invariant : forall rs p v rs',
  RINV rs -> r_alloc_scratch rs p v = Some rs' -> RINV rs'.
Proof.
  intros rs p v rs' [HINV [Hfree Hlive]] Ha. unfold r_alloc_scratch, alloc_raw in Ha.
  destruct HINV as [Hdis [Hbf Hbl]].
  destruct (Z.eqb p (bump (ra rs))) eqn:Ep.
  - apply Z.eqb_eq in Ep. subst p. injection Ha as <-. unfold RINV.
    split; [ | split ].
    + unfold INV; simpl. split; [ exact Hdis | split ].
      * unfold below. intros x Hx. assert (H : x < bump (ra rs)) by (apply Hbf; exact Hx). lia.
      * unfold below. intros x Hx. assert (H : x < bump (ra rs)) by (apply Hbl; exact Hx). lia.
    + intros x Hx. simpl in Hx. rewrite rc_at_mk.
      assert (Hne : x <> bump (ra rs))
        by (intros E; rewrite E in Hx; apply (Z.lt_irrefl (bump (ra rs))); apply Hbf; exact Hx).
      rewrite (rc_upd_other (rmem rs) (bump (ra rs)) x v Hne). apply (Hfree x Hx).
    + intros x Hx. simpl in Hx. rewrite rc_at_mk.
      assert (Hne : x <> bump (ra rs))
        by (intros E; rewrite E in Hx; apply (Z.lt_irrefl (bump (ra rs))); apply Hbl; exact Hx).
      rewrite (rc_upd_other (rmem rs) (bump (ra rs)) x v Hne). apply (Hlive x Hx).
  - destruct (freeS (ra rs) p) eqn:Ef; [ | discriminate ]. injection Ha as <-.
    unfold RINV. split; [ | split ].
    + unfold INV; simpl. split; [ | split ].
      * unfold disjoint, remS. intros x. destruct (Z.eqb x p) eqn:Ex.
        { intros Hc. discriminate Hc. }
        { intros Hf' Hl'. apply (Hdis x Hf' Hl'). }
      * unfold below, remS. intros x. destruct (Z.eqb x p) eqn:Ex.
        { intros Hc. discriminate Hc. }
        { intros Hx. apply Hbf. exact Hx. }
      * exact Hbl.
    + intros x Hx. simpl in Hx. unfold remS in Hx.
      destruct (Z.eqb x p) eqn:Ex; [ discriminate Hx | ].
      assert (Hne : x <> p) by (intros E; rewrite E in Ex; rewrite Z.eqb_refl in Ex;
                                discriminate Ex).
      rewrite rc_at_mk. rewrite (rc_upd_other (rmem rs) p x v Hne). apply (Hfree x Hx).
    + intros x Hx. simpl in Hx.
      assert (Hne : x <> p) by (intros E; rewrite E in Hx; apply (Hdis p Ef Hx)).
      rewrite rc_at_mk. rewrite (rc_upd_other (rmem rs) p x v Hne). apply (Hlive x Hx).
Qed.

(* ══════════════════════════════════════════════════════════════════════════
   COMPOSITION WITH SENTINEL 1 — reuse restores rc=1 ACROSS A REGION RESET.

   The reset (`RegionSave`/`RegionRestore`) hands the SAVED free-list back. Those
   blocks read 0 when they were saved; the theorem below is that nothing inside
   the window can have changed that — an inc or a dec needs the block LIVE, and
   `FreeList.WinINV` already proves a saved-free block is neither free nor live
   inside the window. So the restored free-list is still a list of zero-count
   blocks, and the next reuse on the other side of the reset still restores 1. *)

Definition r_region_save (rs : RState) : RState :=
  {| ra := region_save (ra rs) ; rmem := rmem rs |}.
Definition r_region_restore (sn : RSnap) (rs : RState) : RState :=
  {| ra := region_restore sn (ra rs) ; rmem := rmem rs |}.

(* Each r-step projects onto a FreeList `steps` step (an inc, and a dec that does
   not reach 0, leave the address state alone). *)
Lemma r_steps_project : forall rs rs', r_steps rs rs' -> steps (ra rs) (ra rs').
Proof.
  intros rs rs' Hst. induction Hst as
    [ rs0 | rs0 p rs1 rs2 H _ IH | rs0 p rs1 rs2 H _ IH | rs0 p rs1 rs2 H _ IH ].
  - apply steps_refl.
  - unfold r_new, r_alloc in H. destruct (alloc (ra rs0) p) as [a'|] eqn:Ea; [ | discriminate ].
    injection H as <-. apply (steps_alloc _ p a'); [ exact Ea | exact IH ].
  - unfold r_inc in H. destruct (liveS (ra rs0) p); [ | discriminate ].
    injection H as <-. exact IH.
  - destruct (Z.leb (rc_at rs0 p) 0) eqn:Ez.
    { apply Z.leb_le in Ez. rewrite (r_dec_nonpos rs0 p Ez) in H. discriminate H. }
    apply Z.leb_gt in Ez. assert (H1 : 1 <= rc_at rs0 p) by lia.
    rewrite (r_dec_pos rs0 p H1) in H.
    destruct (Z.eqb (rc_at rs0 p) 1).
    + destruct (free_op (ra rs0) p) as [a'|] eqn:Ef; [ | discriminate ].
      injection H as <-. apply (steps_free _ p a'); [ exact Ef | exact IH ].
    + injection H as <-. exact IH.
Qed.

(* The window invariant, plus the count fact that makes the restored free-list
   sound to hand back. *)
Definition WinRc (sn : RSnap) (rs : RState) : Prop :=
  WinINV sn (ra rs) /\ (forall x, s_free sn x = true -> rc_at rs x = 0).

Lemma WinRc_steps : forall sn rs rs',
  snap_wf sn -> r_steps rs rs' -> WinRc sn rs -> WinRc sn rs'.
Proof.
  intros sn rs rs' Hwf Hst. induction Hst as
    [ rs0 | rs0 p rs1 rs2 H _ IH | rs0 p rs1 rs2 H _ IH | rs0 p rs1 rs2 H _ IH ];
    intros [Hw Hz].
  - split; [ exact Hw | exact Hz ].
  - apply IH.
    (* the allocation's address is either the frontier (above every saved block)
       or a currently-free block (a saved block is not currently free) *)
    unfold r_new, r_alloc in H. destruct (alloc (ra rs0) p) as [a'|] eqn:Ea; [ | discriminate ].
    injection H as <-.
    assert (Hw' : WinINV sn a') by (apply (WinINV_alloc sn (ra rs0) p a' Hwf Hw Ea)).
    split; [ exact Hw' | ]. intros x Hx.
    assert (Hne : x <> p).
    { destruct Hw as [_ [Hb Hs]]. destruct (Hs x Hx) as [Hxf _].
      assert (Hlt : x < s_bump sn) by (apply Hwf; exact Hx).
      unfold alloc in Ea. destruct (Z.eqb p (bump (ra rs0))) eqn:Ep.
      - apply Z.eqb_eq in Ep. subst p. lia.
      - destruct (freeS (ra rs0) p) eqn:Ef; [ | discriminate ].
        intros E. rewrite E in Hxf. rewrite Ef in Hxf. discriminate Hxf. }
    rewrite (rc_at_rc_init_other _ p x Hne). rewrite rc_at_mk. apply (Hz x Hx).
  - apply IH. unfold r_inc in H. destruct (liveS (ra rs0) p) eqn:El; [ | discriminate ].
    injection H as <-. split; [ exact Hw | ]. intros x Hx.
    assert (Hne : x <> p).
    { destruct Hw as [_ [_ Hs]]. destruct (Hs x Hx) as [_ Hxl].
      intros E. rewrite E in Hxl. rewrite El in Hxl. discriminate Hxl. }
    rewrite rc_at_mk. unfold rt_inc.
    rewrite (rc_upd_other (rmem rs0) p x _ Hne). apply (Hz x Hx).
  - apply IH.
    destruct (Z.leb (rc_at rs0 p) 0) eqn:Ez.
    { apply Z.leb_le in Ez. rewrite (r_dec_nonpos rs0 p Ez) in H. discriminate H. }
    apply Z.leb_gt in Ez. assert (H1 : 1 <= rc_at rs0 p) by lia.
    rewrite (r_dec_pos rs0 p H1) in H.
    (* a saved block reads 0; the released block reads > 0; so they differ *)
    assert (Hne : forall x, s_free sn x = true -> x <> p).
    { intros x Hx E. assert (H0 : rc_at rs0 x = 0) by (apply (Hz x Hx)).
      rewrite E in H0. lia. }
    destruct (Z.eqb (rc_at rs0 p) 1).
    + destruct (free_op (ra rs0) p) as [a'|] eqn:Ef; [ | discriminate ].
      injection H as <-.
      assert (Hw' : WinINV sn a') by (apply (WinINV_free sn (ra rs0) p a' Hw Ef)).
      split; [ exact Hw' | ]. intros x Hx. rewrite rc_at_mk.
      rewrite (rc_upd_other (rmem rs0) p x _ (Hne x Hx)). apply (Hz x Hx).
    + injection H as <-. split; [ exact Hw | ]. intros x Hx.
      rewrite rc_at_mk. rewrite (rc_upd_other (rmem rs0) p x _ (Hne x Hx)).
      apply (Hz x Hx).
Qed.

Lemma r_save_WinRc : forall rs, RINV rs -> WinRc (snap_of (ra rs)) (r_region_save rs).
Proof.
  intros rs [HINV [Hfree _]]. unfold WinRc, r_region_save; simpl.
  split; [ apply (save_WinINV (ra rs) HINV) | ].
  intros x Hx. unfold snap_of in Hx; simpl in Hx. unfold rc_at; simpl. apply (Hfree x Hx).
Qed.

Lemma r_save_RINV : forall rs, RINV rs -> RINV (r_region_save rs).
Proof.
  intros rs [HINV [_ Hlive]]. destruct HINV as [_ [_ Hbl]].
  unfold RINV, r_region_save, region_save; simpl. split; [ | split ].
  - unfold INV; simpl. split; [ | split ].
    + unfold disjoint, emptyS. intros x H. discriminate H.
    + unfold below, emptyS. intros x H. discriminate H.
    + exact Hbl.
  - unfold emptyS. intros x H. discriminate H.
  - intros x Hx. apply (Hlive x Hx).
Qed.

(* SENTINEL 3 ∘ SENTINEL 1: the reset re-establishes the COUNT invariant too —
   every entry on the restored free-list still reads 0, so a reuse on the other
   side of a region window is still a reuse from 0. *)
Theorem r_region_reset_preserves_RINV : forall rs rs_end,
  RINV rs -> r_steps (r_region_save rs) rs_end ->
  RINV (r_region_restore (snap_of (ra rs)) rs_end).
Proof.
  intros rs rs_end HR Hst.
  assert (Hwf : snap_wf (snap_of (ra rs))) by (apply save_snap_wf; apply HR).
  assert (Hproj : steps (region_save (ra rs)) (ra rs_end))
    by (exact (r_steps_project _ _ Hst)).
  assert (HRend : RINV rs_end)
    by (apply (r_steps_preserve_RINV (r_region_save rs) rs_end Hst (r_save_RINV rs HR))).
  assert (Hwr : WinRc (snap_of (ra rs)) rs_end)
    by (apply (WinRc_steps (snap_of (ra rs)) (r_region_save rs) rs_end Hwf Hst
                           (r_save_WinRc rs HR))).
  destruct Hwr as [_ Hz]. destruct HRend as [_ [_ Hlive]].
  unfold RINV. split; [ | split ].
  - apply (region_window_preserves_INV (ra rs) (ra rs_end)); [ apply HR | exact Hproj ].
  - intros x Hx. apply (Hz x Hx).
  - intros x Hx. apply Hlive.
    apply (maskS_true (liveS (ra rs_end)) (s_bump (snap_of (ra rs))) x). exact Hx.
Qed.

Theorem reuse_after_a_region_reset_restores_rc_1 : forall rs rs_end p rs',
  RINV rs -> r_steps (r_region_save rs) rs_end ->
  r_new (r_region_restore (snap_of (ra rs)) rs_end) p = Some rs' ->
  rc_at rs' p = RC_INITIAL /\ RINV rs'.
Proof.
  intros rs rs_end p rs' HR Hst Hn.
  assert (HR' : RINV (r_region_restore (snap_of (ra rs)) rs_end))
    by (apply (r_region_reset_preserves_RINV rs rs_end HR Hst)).
  destruct (reuse_restores_rc_1 _ p rs' HR' Hn) as [H1 [_ [_ [_ H5]]]].
  split; [ exact H1 | exact H5 ].
Qed.

(* ══════════════════════════════════════════════════════════════════════════
   WHAT REMAINS TRUSTED — the boundary, stated rather than implied.

   PROVEN here: the allocator hands a reused block back at count 0 and the
   constructor's store leaves it at exactly `RC_INITIAL` = 1; the free-list holds
   only zero-count blocks and the live set only positive-count blocks, over an
   ARBITRARY run and across a region reset; a double release traps and a live
   release never does; and an un-rc'd scratch allocation cannot corrupt any of it.

   TRUSTED still, and only this: that the emitted wasm really is this PAIR at
   every rc-managed allocation site — `(call $alloc …)` immediately followed by
   the `RC_INITIAL` store. That is a property of the EMITTER, which no proof about
   the allocator can reach. It is gated executably rather than asserted:
   `almide-mir`'s `every_rc_managed_alloc_site_initializes_the_rc_cell` enumerates
   every `call $alloc` the renderer can emit and fails unless each one either
   initializes the rc cell or is an explicitly named scratch site (the
   `alloc_raw` category above), and it reads `RC_INITIAL` out of THIS file so the
   constant cannot drift. The refinement of `$rc_dec` itself — that the emitted
   bytes compute the load/trap/decrement/free-list-push this file calls `r_dec` —
   is `WasmRcDec.rc_dec_prog_realizes_rt_dec` and
   `WasmExec.rc_dec_bytes_frees_when_one`, already proven and grounded against
   wat2wasm/wasmtime.

   Not modelled: block SIZES (the free-list is exact-size first-fit — a size
   mismatch is a missed reuse, never an unsafe one) and the payload bytes of a
   block, which no rc property depends on. *)

(* AXIOM AUDIT — soundness rests on the kernel alone. *)
Print Assumptions reuse_hands_back_a_zero_count_block.
Print Assumptions reuse_restores_rc_1.
Print Assumptions r_new_preserves_RINV.
Print Assumptions double_release_traps.
Print Assumptions live_block_releases_without_trapping.
Print Assumptions release_at_one_frees_and_zeroes.
Print Assumptions r_dec_preserves_RINV.
Print Assumptions r_inc_preserves_RINV.
Print Assumptions r_steps_preserve_RINV.
Print Assumptions reuse_always_restores_rc_1.
Print Assumptions scratch_alloc_cannot_corrupt_the_rc_invariant.
Print Assumptions r_region_reset_preserves_RINV.
Print Assumptions reuse_after_a_region_reset_restores_rc_1.
