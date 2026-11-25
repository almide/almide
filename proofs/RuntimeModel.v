(* Almide v1 trust spine — brick 3b (start): the runtime HEAP-CELL MACHINE.

   G1.2, the single hardest gap, was: "the abstract Dec is just a token; there is
   no model of what the runtime (__alloc / __rc_dec / free-list) DOES to linear
   memory." This file builds that model — at the memory-cell level — and proves
   it REFINES the abstract RC semantics (OwnershipChecker.exec):

     an object's refcount lives in a CELL of linear memory at `base + RC_OFFSET`;
     the runtime's inc/dec are concrete memory writes; and running an op sequence
     on this MEMORY MACHINE faults (a real double-free: a dec of a 0/freed cell)
     EXACTLY when the abstract refcount would go below zero, and otherwise leaves
     the cell holding exactly the abstract refcount.

   So the abstract RC-balance proof (which the per-build checker enforces) is
   realized by the concrete memory machine — not a free-floating token. The
   REMAINING step (binding this memory machine to the actual wasm BYTES: that the
   wasm `call $rc_dec` instruction executes precisely these cell writes) is the
   WasmCert-Coq layer — the further, ISA-level half of G1.2. *)

From AlmideTrust Require Import OwnershipChecker.
From Stdlib Require Import ZArith List Lia.
Import ListNotations.
Open Scope Z_scope.

(* Linear memory: address -> value (total; unwritten cells read 0). *)
Definition Mem := Z -> Z.
Definition upd (m : Mem) (a v : Z) : Mem := fun x => if Z.eqb x a then v else m x.

(* The object's refcount cell offset — a layout constant the renderer and the
   runtime agree on (a concrete Definition, so the trusted base gains no axiom). *)
Definition RC_OFFSET : Z := 0.
Definition read_rc (m : Mem) (base : Z) : Z := m (base + RC_OFFSET).

(* The runtime's concrete cell operations. inc/dec are memory writes to the rc
   cell; dec FAULTS (None) on a 0/freed cell — a real double-free. *)
Definition rt_inc (m : Mem) (base : Z) : Mem :=
  upd m (base + RC_OFFSET) (read_rc m base + 1).
Definition rt_dec (m : Mem) (base : Z) : option Mem :=
  if read_rc m base <=? 0 then None
  else Some (upd m (base + RC_OFFSET) (read_rc m base - 1)).

(* REUSE-release: valid ONLY on a UNIQUELY-owned cell (rc = 1) — reusing a SHARED
   cell in place would corrupt the aliasing owner, so it FAULTS. On rc = 1 it
   brings the cell to 0 (the block is repurposed). Mirrors exec's tightened Reuse,
   so the memory machine stays in lockstep with the abstract semantics. *)
Definition rt_reuse (m : Mem) (base : Z) : option Mem :=
  if Z.eqb (read_rc m base) 1 then Some (upd m (base + RC_OFFSET) 0) else None.

(* BORROW: a read through a handle — memory is untouched, but the cell must be
   LIVE (rc > 0): reading a freed cell is the concrete use-after-free. Mirrors
   exec's Borrow guard so the machine and the abstract semantics fault together. *)
Definition rt_borrow (m : Mem) (base : Z) : option Mem :=
  if read_rc m base <=? 0 then None else Some m.

Definition step_mem (o : Op) (m : Mem) (base : Z) : option Mem :=
  match o with
  | Inc | Alias => Some (rt_inc m base)
  | Dec | MoveOut => rt_dec m base
  (* #4753: refused on every ownership line (OwnershipChecker.exec), so the
     machine faults with it; `rt_reuse` stays as the model of the in-place
     write itself, never reached from a witness. *)
  | Reuse => None
  | Borrow => rt_borrow m base
  (* a share-out: the +1 is the arm result's cell (its `i`), not this one. *)
  | ShareOut => Some m
  (* ADR-0034 take: this ACCOUNT gains the credit the slot's account lost (the
     physical cell is unchanged — UniqueTake.take_preserves); per account it
     steps like Inc. *)
  | Take => Some (rt_inc m base)
  (* ADR-0034 probe: a liveness read, like Borrow. *)
  | Probe => rt_borrow m base
  (* #1961 region reclaim: this ACCOUNT's credit is discharged (it steps like
     Dec); the block itself goes back to the allocator with the whole window
     at `RegionRestore` — `region_restore_reclaims` below. *)
  | Reclaim => rt_dec m base
  end.

Fixpoint mrun (ops : list Op) (m : Mem) (base : Z) : option Mem :=
  match ops with
  | nil => Some m
  | cons o rest =>
      match step_mem o m base with
      | Some m' => mrun rest m' base
      | None => None
      end
  end.

(* The refcount the machine state denotes (None = a fault occurred). *)
Definition rc_of (om : option Mem) (base : Z) : option Z :=
  match om with Some m => Some (read_rc m base) | None => None end.

(* Reading the rc cell back after a write to it yields the written value. *)
Lemma read_upd_same : forall m base v, read_rc (upd m (base + RC_OFFSET) v) base = v.
Proof.
  intros m base v. unfold read_rc, upd. rewrite Z.eqb_refl. reflexivity.
Qed.

Lemma read_rt_inc : forall m base, read_rc (rt_inc m base) base = read_rc m base + 1.
Proof. intros. unfold rt_inc. apply read_upd_same. Qed.

(* THE REFINEMENT. The concrete memory machine's rc cell evolves EXACTLY as the
   abstract refcount (OwnershipChecker.exec): so the abstract RC semantics the
   checker proves are realized in linear memory, faulting precisely together. *)
Theorem mrun_tracks_exec :
  forall ops m base, rc_of (mrun ops m base) base = exec ops (read_rc m base).
Proof.
  induction ops as [| o rest IH]; intros m base.
  - reflexivity.
  - destruct o; cbn [mrun step_mem exec].
    + (* Inc *) pose proof (IH (rt_inc m base) base) as IH2.
      rewrite read_rt_inc in IH2. exact IH2.
    + (* Alias *) pose proof (IH (rt_inc m base) base) as IH2.
      rewrite read_rt_inc in IH2. exact IH2.
    + (* Dec *) unfold rt_dec. destruct (read_rc m base <=? 0) eqn:E.
      * reflexivity.
      * pose proof (IH (upd m (base + RC_OFFSET) (read_rc m base - 1)) base) as IH2.
        rewrite (read_upd_same m base (read_rc m base - 1)) in IH2. exact IH2.
    + (* MoveOut *) unfold rt_dec. destruct (read_rc m base <=? 0) eqn:E.
      * reflexivity.
      * pose proof (IH (upd m (base + RC_OFFSET) (read_rc m base - 1)) base) as IH2.
        rewrite (read_upd_same m base (read_rc m base - 1)) in IH2. exact IH2.
    + (* Reuse: refused — both fault (#4753) *) reflexivity.
    + (* Borrow *) unfold rt_borrow. destruct (read_rc m base <=? 0) eqn:E.
      * (* dead cell: both fault (use-after-free) *)
        reflexivity.
      * (* live: memory untouched, count unchanged *)
        apply IH.
    + (* ShareOut: this cell untouched, count unchanged *) apply IH.
    + (* Take: the account steps like Inc *) pose proof (IH (rt_inc m base) base) as IH2.
      rewrite read_rt_inc in IH2. exact IH2.
    + (* Probe: Borrow's guard *) unfold rt_borrow. destruct (read_rc m base <=? 0) eqn:E.
      * reflexivity.
      * apply IH.
    + (* Reclaim: the account steps like Dec *) unfold rt_dec. destruct (read_rc m base <=? 0) eqn:E.
      * reflexivity.
      * pose proof (IH (upd m (base + RC_OFFSET) (read_rc m base - 1)) base) as IH2.
        rewrite (read_upd_same m base (read_rc m base - 1)) in IH2. exact IH2.
Qed.

(* COROLLARY: an accepted certificate (the abstract run ends balanced from rc 0)
   is realized by a memory machine that NEVER double-frees — the runtime cell
   never decrements below zero. Binds OwnershipChecker's `check` to real memory. *)
Corollary balanced_cert_no_memory_fault :
  forall ops m base, read_rc m base = 0 -> check ops = true -> mrun ops m base <> None.
Proof.
  intros ops m base Hrc Hchk Hnone.
  pose proof (mrun_tracks_exec ops m base) as Href.
  rewrite Hrc, Hnone in Href. cbn in Href.
  unfold check, run in Hchk. rewrite <- Href in Hchk. discriminate.
Qed.

(* LEAK-FREEDOM, at the memory level. An accepted certificate (balanced from
   rc 0) leaves the runtime cell at 0 — the object's last reference is released,
   so the cell is FREED, not leaked. The RC-regime wasm renderer (A1.1b) realizes
   exactly this: a `Drop` emits `call $rc_dec`, bringing the cell to 0. (The eager
   fragment did NOT — it emitted no release; eager_copy_refines_safety remains the
   dual-oracle baseline.) The witness's "ends at 0"
   (the leak-free half of `check`) is thereby bound to real freed memory. *)
Corollary balanced_cert_frees_in_memory :
  forall ops m base m',
    read_rc m base = 0 -> check ops = true -> mrun ops m base = Some m' ->
    read_rc m' base = 0.
Proof.
  intros ops m base m' Hrc Hchk Hsome.
  apply check_sound in Hchk. destruct Hchk as [_ Hnoleak].
  unfold no_leak, run in Hnoleak.
  pose proof (mrun_tracks_exec ops m base) as Href.
  rewrite Hsome, Hrc in Href. cbn in Href.
  rewrite Hnoleak in Href. injection Href as Href'. exact Href'.
Qed.

(* ─── THE REGION WINDOW (#1961): why a `Reclaim` discharges its block ───
   The structural leg runs `consume(produce(scalars))` inside a bump window:
   `RegionSave` files the bump pointer and the size-class heads and ZEROES the
   heads; `RegionRestore` puts both back (crates/almide-wasm/src/region.rs).
   The model: the allocator state is the bump pointer and the free blocks the
   heads hold (one list — the class split does not matter here); a window
   allocation takes a free block or bumps, a window free files a block. The
   window's escape condition (the compiler's region-purity check, TRUSTED:
   proofs/TRUSTED_BASE.md) is `frees_above`: inside the window only blocks the
   window itself took are freed, and no size is negative. Then every block the
   window hands out lies at or above the saved bump pointer and on no saved
   free list, and the restore returns the allocator to the saved state: each
   such block is back beyond the bump pointer — reclaimed without a release
   call, which is what the witness's `w` asserts for the parked temporary. *)
Record RgAlloc : Type := { rg_bump : Z; rg_heads : list Z }.

Inductive WOp : Type :=
  | WAlloc : Z -> WOp
  | WFree : Z -> WOp.

Definition walloc (s : RgAlloc) (sz : Z) : RgAlloc * Z :=
  match rg_heads s with
  | cons a t => ({| rg_bump := rg_bump s; rg_heads := t |}, a)
  | nil => ({| rg_bump := rg_bump s + sz; rg_heads := nil |}, rg_bump s)
  end.

Definition wfree (s : RgAlloc) (a : Z) : RgAlloc := {| rg_bump := rg_bump s; rg_heads := cons a (rg_heads s) |}.

(* The window run: the final allocator state and every block handed out. *)
Fixpoint wrun (s : RgAlloc) (ops : list WOp) : RgAlloc * list Z :=
  match ops with
  | nil => (s, nil)
  | cons (WAlloc sz) r =>
      let p := walloc s sz in
      let q := wrun (fst p) r in
      (fst q, cons (snd p) (snd q))
  | cons (WFree a) r => wrun (wfree s a) r
  end.

Fixpoint frees_above (b0 : Z) (ops : list WOp) : Prop :=
  match ops with
  | nil => True
  | cons (WAlloc sz) r => 0 <= sz /\ frees_above b0 r
  | cons (WFree a) r => b0 <= a /\ frees_above b0 r
  end.

Definition region_save (s : RgAlloc) : RgAlloc := {| rg_bump := rg_bump s; rg_heads := nil |}.
Definition region_restore (saved _cur : RgAlloc) : RgAlloc := saved.

Lemma walloc_above :
  forall s sz b0, 0 <= sz -> b0 <= rg_bump s -> Forall (fun a => b0 <= a) (rg_heads s) ->
    b0 <= snd (walloc s sz) /\ b0 <= rg_bump (fst (walloc s sz))
      /\ Forall (fun a => b0 <= a) (rg_heads (fst (walloc s sz))).
Proof.
  intros s sz b0 Hsz Hb Hh. unfold walloc.
  destruct (rg_heads s) as [| x t] eqn:Eh; cbn.
  - split; [exact Hb | split; [lia | constructor]].
  - rewrite Eh in Hh. inversion Hh as [| y l Hx Ht]; subst.
    split; [exact Hx | split; [exact Hb | exact Ht]].
Qed.

Lemma wrun_above :
  forall ops s b0, b0 <= rg_bump s -> Forall (fun a => b0 <= a) (rg_heads s) ->
    frees_above b0 ops -> Forall (fun a => b0 <= a) (snd (wrun s ops)).
Proof.
  induction ops as [| o r IH]; intros s b0 Hb Hh Hf.
  - cbn. constructor.
  - destruct o as [sz | a]; cbn [frees_above] in Hf; destruct Hf as [Ho Hr].
    + destruct (walloc_above s sz b0 Ho Hb Hh) as [Ha [Hb' Hh']].
      cbn [wrun snd]. constructor; [exact Ha |].
      apply IH; [exact Hb' | exact Hh' | exact Hr].
    + cbn [wrun]. apply IH; [exact Hb | | exact Hr].
      unfold wfree; cbn. constructor; [exact Ho | exact Hh].
Qed.

(* THE REGION THEOREM. From an allocator whose free blocks all lie below its
   bump pointer, a window that frees only its own blocks hands out blocks that
   are, after the restore, at or above the bump pointer and on no free list —
   the restore reclaims every one of them, and puts the allocator back exactly
   as the save found it. *)
Theorem region_restore_reclaims :
  forall s ops,
    Forall (fun a => a < rg_bump s) (rg_heads s) -> frees_above (rg_bump s) ops ->
    region_restore s (fst (wrun (region_save s) ops)) = s /\
    Forall (fun a => rg_bump s <= a /\ ~ In a (rg_heads s)) (snd (wrun (region_save s) ops)).
Proof.
  intros s ops Hh Hf. split; [reflexivity |].
  assert (H : Forall (fun a => rg_bump s <= a) (snd (wrun (region_save s) ops))).
  { apply wrun_above; [unfold region_save; cbn; lia | unfold region_save; cbn; constructor | exact Hf]. }
  apply Forall_forall. intros a Ha.
  pose proof (proj1 (Forall_forall _ _) H a Ha) as Hge.
  split; [exact Hge |]. intro Hin.
  pose proof (proj1 (Forall_forall _ _) Hh a Hin) as Hlt. lia.
Qed.

(* A reclaimed line's account: `w` steps like `d` on the holder's credit. *)
Theorem reclaim_steps_like_dec : forall m base, step_mem Reclaim m base = step_mem Dec m base.
Proof. reflexivity. Qed.

Print Assumptions mrun_tracks_exec.
Print Assumptions balanced_cert_no_memory_fault.
Print Assumptions balanced_cert_frees_in_memory.
