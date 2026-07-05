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
From Stdlib Require Import ZArith.
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

Definition step_mem (o : Op) (m : Mem) (base : Z) : option Mem :=
  match o with
  | Inc | Alias => Some (rt_inc m base)
  | Dec | MoveOut => rt_dec m base
  | Reuse => rt_reuse m base
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
    + (* Reuse *) unfold rt_reuse. destruct (Z.eqb (read_rc m base) 1) eqn:E.
      * (* rc = 1: cell → 0; abstract exec → exec rest 0 *)
        pose proof (IH (upd m (base + RC_OFFSET) 0) base) as IH2.
        rewrite (read_upd_same m base 0) in IH2. exact IH2.
      * (* rc <> 1: both fault (shared reuse / underflow) *)
        reflexivity.
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

Print Assumptions mrun_tracks_exec.
Print Assumptions balanced_cert_no_memory_fault.
Print Assumptions balanced_cert_frees_in_memory.
