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

Definition step_mem (o : Op) (m : Mem) (base : Z) : option Mem :=
  match o with
  | Inc | Alias => Some (rt_inc m base)
  | Dec | MoveOut => rt_dec m base
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

Print Assumptions mrun_tracks_exec.
Print Assumptions balanced_cert_no_memory_fault.
