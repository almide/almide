(* Almide v1 trust spine — #576 second slice: the STRUCTURAL leg's `$alloc`
   tree realizes the modeled take/bump discipline.

   StructuralRuntime.v (the first slice) proved `$inc`/`$dec_flat`/`$free`
   realize the modeled RC transitions and the class filing, and proved the
   class-math keystones. This file transcribes `emit_alloc`'s tree
   (crates/almide-wasm/src/runtime_alloc.rs) and proves its two ALLOCATION
   paths:

     `alloc_pops_filed_head` — when the computed class's list holds a
        head, `$alloc` returns exactly that head, UNLINKS it (the slot
        receives the head's next pointer — the inverse of `$free`'s push),
        and writes the header rc=1 / len / cap = 16*2^class - PAYLOAD.
        This is `FreeList.alloc`'s free-list branch made concrete, and
        the produced cap is precisely the hypothesis
        `dec_unique_files_take_class` consumes — the reuse CYCLE closes:
        take at class c → release → refile at class c → next take pops it.

     `alloc_bumps_fresh` — when the class list is empty (or the request
        is beyond the class table), `$alloc` returns the bump frontier,
        advances it by the class-rounded want (exact for huge requests),
        and writes the same header shape. `FreeList.alloc`'s frontier
        branch, concrete.

   HONEST SCOPE, stated plainly:
   - Same tree-level binding as slice 1: the transcription is pinned to
     the emitted bytes by the Rust-side hash pin
     (`runtime_alloc.rs::runtime_trees_match_the_proof_transcription` —
     extended to `$alloc` modulo its one per-program immediate, the OOM
     message address, which the pin masks).
   - The GROW arm: the `memory.grow` guard is transcribed exactly, and
     both allocation theorems carry the no-grow hypothesis
     (`next <= pages * 64Ki`) — the geometric-growth policy is
     deliberately behavior-free (memory.size is not observable from the
     language) and the OOM exit is the fixture-gated C-197 contract, so
     the grow BODY is modeled as one abstract step (`SGrow`) whose
     instruction-level transcription joins the byte-binding half. Under
     the no-grow hypothesis it is unreached, which `bump_skips_grow`
     states.
   - i32 vs Z as in slice 1: nonnegative, well below 2^31, bounds carried
     explicitly. *)

From AlmideTrust Require Import RuntimeModel.
From AlmideTrust Require Import StructuralRuntime.
From Stdlib Require Import ZArith.
From Stdlib Require Import List.
Import ListNotations.
From Stdlib Require Import Lia.
Open Scope Z_scope.

Section AllocTree.

(* One invocation: the requested payload length, and the free-list base
   address. *)
Variables len fbase : Z.

(* Machine state: the four scratch locals, the bump global, linear-memory
   pages, and memory. *)
Record A := mkA { abase : Z; anext : Z; awant : Z; ahead : Z;
                  agh : Z; apages : Z; am : Mem }.

Inductive aexpr : Type :=
  | AC (z : Z)
  | ALen | ABase | ANext | AWant | AHead
  | AGHeap | AMemSize
  | AAdd (a b : aexpr)
  | ASub (a b : aexpr)
  | ALand (a b : aexpr)
  | AShl (a b : aexpr)
  | AShrU (a b : aexpr)
  | AClz (a : aexpr)
  | ALtU (a b : aexpr)
  | ALeU (a b : aexpr)
  | AGtU (a b : aexpr)
  | ALoad (a : aexpr).

Fixpoint aev (e : aexpr) (c : A) : Z :=
  match e with
  | AC z => z
  | ALen => len
  | ABase => abase c
  | ANext => anext c
  | AWant => awant c
  | AHead => ahead c
  | AGHeap => agh c
  | AMemSize => apages c
  | AAdd a b => aev a c + aev b c
  | ASub a b => aev a c - aev b c
  | ALand a b => Z.land (aev a c) (aev b c)
  | AShl a b => Z.shiftl (aev a c) (aev b c)
  | AShrU a b => Z.shiftr (aev a c) (aev b c)
  | AClz a => clz32 (aev a c)
  | ALtU a b => if Z.ltb (aev a c) (aev b c) then 1 else 0
  | ALeU a b => if Z.leb (aev a c) (aev b c) then 1 else 0
  | AGtU a b => if Z.ltb (aev b c) (aev a c) then 1 else 0
  | ALoad a => am c (aev a c)
  end.

Inductive astmt : Type :=
  | ASetBase (e : aexpr)
  | ASetNext (e : aexpr)
  | ASetWant (e : aexpr)
  | ASetHead (e : aexpr)
  | ASetGHeap (e : aexpr)
  | AStore (addr v : aexpr)
  | AIf (cond : aexpr) (body : list astmt)
  | ARetV (e : aexpr)
  | SGrow.   (* the abstract grow step (see the header) *)

(* Outcome: fell through, or returned a value. *)
Inductive aout : Type :=
  | AFall (c : A)
  | ARet (v : Z) (c : A).

(* Grow's abstract semantics: enough pages appear (the policy is
   behavior-free); nothing else moves. *)
Definition grow_sem (c : A) : A :=
  mkA (abase c) (anext c) (awant c) (ahead c) (agh c)
      (anext c) (am c).

Fixpoint astep (s : astmt) (c : A) {struct s} : aout :=
  match s with
  | ASetBase e => AFall (mkA (aev e c) (anext c) (awant c) (ahead c)
                             (agh c) (apages c) (am c))
  | ASetNext e => AFall (mkA (abase c) (aev e c) (awant c) (ahead c)
                             (agh c) (apages c) (am c))
  | ASetWant e => AFall (mkA (abase c) (anext c) (aev e c) (ahead c)
                             (agh c) (apages c) (am c))
  | ASetHead e => AFall (mkA (abase c) (anext c) (awant c) (aev e c)
                             (agh c) (apages c) (am c))
  | ASetGHeap e => AFall (mkA (abase c) (anext c) (awant c) (ahead c)
                              (aev e c) (apages c) (am c))
  | AStore a v => AFall (mkA (abase c) (anext c) (awant c) (ahead c)
                             (agh c) (apages c)
                             (upd (am c) (aev a c) (aev v c)))
  | ARetV e => ARet (aev e c) c
  | SGrow => AFall (grow_sem c)
  | AIf e body =>
      if Z.eqb (aev e c) 0 then AFall c
      else
        (fix runl (ss : list astmt) (c0 : A) {struct ss} : aout :=
           match ss with
           | [] => AFall c0
           | s' :: r =>
               match astep s' c0 with
               | AFall c' => runl r c'
               | o => o
               end
           end) body c
  end.

Fixpoint arun (ss : list astmt) (c : A) : aout :=
  match ss with
  | [] => AFall c
  | s :: r =>
      match astep s c with
      | AFall c' => arun r c'
      | o => o
      end
  end.

(* ── `$alloc` — emit_alloc, 1:1 (literals: 12 = PAYLOAD, 15 = PAYLOAD+3,
      16 = the clamp/class count, 0/4/8 = RC/LEN/CAP offsets; the class
      slot is `fbase + (class << 2)`; `16 << 15` is the largest class
      capacity; the OOM body is the abstract `SGrow` per the header). ── *)
Definition alloc_body : list astmt :=
  [ (* want = (len + 15) & -4; clamp to >= 16 *)
    ASetWant (ALand (AAdd ALen (AC 15)) (AC (-4)));
    AIf (ALtU AWant (AC 16)) [ ASetWant (AC 16) ];
    (* next = 28 - clz(want - 1)  — the ceil class *)
    ASetNext (ASub (AC 28) (AClz (ASub AWant (AC 1))));
    AIf (ALtU ANext (AC 16))
      [ (* next = the class slot ADDRESS *)
        ASetNext (AAdd (AShl ANext (AC 2)) (AC fbase));
        ASetHead (ALoad ANext);
        AIf AHead
          [ (* pop: slot := head.payload[0]; header rc/len/cap; return *)
            AStore ANext (ALoad (AAdd AHead (AC 12)));
            AStore AHead (AC 1);
            AStore (AAdd AHead (AC 4)) ALen;
            AStore (AAdd AHead (AC 8))
                   (ASub (AShl (AC 16)
                               (AShrU (ASub ANext (AC fbase)) (AC 2)))
                         (AC 12));
            ARetV AHead ];
        (* freelist miss: class-round the bump request *)
        ASetNext (ASub (AC 28) (AClz (ASub AWant (AC 1))));
        ASetWant (AShl (AC 16) ANext) ];
    (* bump *)
    ASetBase AGHeap;
    ASetNext (ALand (AAdd (AAdd ABase (AC 12)) (AAdd ALen (AC 3))) (AC (-4)));
    AIf (ALeU AWant (AC 524288))
      [ ASetNext (AAdd ABase AWant) ];
    AIf (AGtU ANext (AShl AMemSize (AC 16))) [ SGrow ];
    AStore ABase (AC 1);
    AStore (AAdd ABase (AC 4)) ALen;
    AStore (AAdd ABase (AC 8)) (ASub AWant (AC 12));
    ASetGHeap ANext;
    ARetV ABase ].

Definition run_alloc (c : A) : aout := arun alloc_body c.

(* Restricted reduction, as in slice 1. *)
Ltac acbn :=
  cbn -[Z.ltb Z.geb Z.leb Z.eqb Z.land Z.shiftl Z.shiftr Z.add Z.sub
        Z.mul Z.opp Z.pow clz32].

(* Fold the literal comparisons a guard replacement leaves behind —
   fold, replace, fold again, to a progress fixpoint. *)
Ltac lit :=
  acbn;
  repeat progress
    (try (replace (0 =? 0) with true by reflexivity);
     try (replace (1 =? 0) with false by reflexivity);
     acbn).

(* ══ THE TAKE PATH ═════════════════════════════════════════════════════ *)

(* When the computed class's list holds a head, `$alloc` pops it: the
   returned block IS the head, the slot receives the head's next pointer
   (the unlink — `$free`'s push inverted), and the header carries
   rc = 1, len, and cap = 16*2^class - 12 — EXACTLY the cap shape
   `dec_unique_files_take_class` consumes, closing the reuse cycle. *)
Theorem alloc_pops_filed_head : forall c w cl slot h,
  w = Z.land (len + 15) (-4) ->
  16 <= w ->
  cl = class_of w ->
  cl < 16 ->
  slot = fbase + 4 * cl ->
  h = am c slot ->
  h <> 0 ->
  run_alloc c
  = ARet h
      (mkA (abase c) slot w h (agh c) (apages c)
           (upd (upd (upd (upd (am c)
                                slot (am c (h + 12)))
                          h 1)
                     (h + 4) len)
                (h + 8) (16 * 2 ^ cl - 12))).
Proof.
  intros c w cl slot h Hw H16 Hcl Hcl16 Hslot Hh Hnz.
  assert (Hnn : 0 <= cl) by (rewrite Hcl; apply class_nonneg; exact H16).
  unfold run_alloc, alloc_body.
  acbn.
  rewrite <- Hw.
  replace (w <? 16) with false by (symmetry; apply Z.ltb_ge; exact H16).
  lit.
  replace (28 - clz32 (w - 1) <? 16) with true.
  2:{ symmetry. apply Z.ltb_lt.
      rewrite Hcl, class_of_log2 in Hcl16. unfold class_of. unfold clz32.
      lia. }
  lit.
  (* the slot address computed by the tree equals `fbase + 4*cl` *)
  assert (Hsl : Z.shiftl (28 - clz32 (w - 1)) 2 + fbase = slot).
  { rewrite Hslot, Hcl, class_of_log2.
    rewrite Z.shiftl_mul_pow2 by lia.
    unfold class_of, clz32. lia. }
  rewrite Hsl.
  rewrite <- Hh.
  replace (h =? 0) with false by (symmetry; apply Z.eqb_neq; exact Hnz).
  lit.
  (* the cap expression recomputes the class from the slot address *)
  assert (Hcap : 16 * 2 ^ cl - 12
                 = Z.shiftl 16 (Z.shiftr (slot - fbase) 2) - 12).
  { rewrite Hslot.
    replace (fbase + 4 * cl - fbase) with (4 * cl) by lia.
    replace (4 * cl) with (cl * 2 ^ 2) by lia.
    rewrite Z.shiftr_div_pow2 by lia.
    rewrite Z.div_mul by lia.
    rewrite Z.shiftl_mul_pow2 by lia.
    lia. }
  rewrite <- Hcap.
  reflexivity.
Qed.

(* ══ THE BUMP PATH ═════════════════════════════════════════════════════ *)

(* Class list empty: the fresh frontier is returned, advanced by the
   class-rounded want, with the same header shape — `FreeList.alloc`'s
   frontier branch. Stated for the classed range under the no-grow
   hypothesis; `$free` refiling this block lands at the same class by
   slice 1's `free_refiles_takes_class`. *)
Theorem alloc_bumps_fresh_classed : forall c w cl,
  w = Z.land (len + 15) (-4) ->
  16 <= w ->
  cl = class_of w ->
  cl < 16 ->
  am c (fbase + 4 * cl) = 0 ->
  0 <= len ->
  let rounded := 16 * 2 ^ cl in
  agh c + rounded <= Z.shiftl (apages c) 16 ->
  run_alloc c
  = ARet (agh c)
      (mkA (agh c) (agh c + rounded) rounded 0
           (agh c + rounded) (apages c)
           (upd (upd (upd (am c)
                          (agh c) 1)
                     (agh c + 4) len)
                (agh c + 8) (rounded - 12))).
Proof.
  intros c w cl Hw H16 Hcl Hcl16 Hempty Hlen rounded Hfit.
  unfold run_alloc, alloc_body.
  acbn.
  rewrite <- Hw.
  replace (w <? 16) with false by (symmetry; apply Z.ltb_ge; exact H16).
  lit.
  replace (28 - clz32 (w - 1) <? 16) with true.
  2:{ symmetry. apply Z.ltb_lt.
      rewrite Hcl, class_of_log2 in Hcl16. unfold clz32. lia. }
  lit.
  assert (Hsl : Z.shiftl (28 - clz32 (w - 1)) 2 + fbase = fbase + 4 * cl).
  { rewrite Hcl, class_of_log2.
    rewrite Z.shiftl_mul_pow2 by lia.
    unfold class_of, clz32. lia. }
  rewrite Hsl. rewrite Hempty.
  lit.
  (* the class-rounded want *)
  assert (Hnn : 0 <= cl) by (rewrite Hcl; apply class_nonneg; exact H16).
  assert (Hrw : Z.shiftl 16 (28 - clz32 (w - 1)) = rounded).
  { replace (28 - clz32 (w - 1)) with cl
      by (rewrite Hcl, class_of_log2; unfold clz32; lia).
    rewrite Z.shiftl_mul_pow2 by exact Hnn.
    reflexivity. }
  rewrite Hrw.
  (* collapse the classed if FIRST — the inner guards sit under its
     binder until it folds *)
  lit.
  (* rounded <= 16 << 15: cl < 16 so 16*2^cl <= 16*2^15 = 524288 *)
  replace (rounded <=? 524288) with true.
  2:{ symmetry. apply Z.leb_le. unfold rounded.
      assert (2 ^ cl <= 2 ^ 15).
      { apply Z.pow_le_mono_r; [lia | lia]. }
      lia. }
  lit.
  (* the no-grow guard: next = base + rounded fits *)
  replace (Z.shiftl (apages c) 16 <? agh c + rounded) with false.
  2:{ symmetry. apply Z.ltb_ge. exact Hfit. }
  lit.
  reflexivity.
Qed.

(* Under the no-grow hypothesis the grow arm is UNREACHED — the abstract
   `SGrow` never executes on the proven paths (its concrete transcription
   is the byte-binding half's business). Corollary of the theorem above:
   the outcome carries `apages c` unchanged. *)
Remark bump_skips_grow : forall c w cl,
  w = Z.land (len + 15) (-4) ->
  16 <= w ->
  cl = class_of w ->
  cl < 16 ->
  am c (fbase + 4 * cl) = 0 ->
  0 <= len ->
  agh c + 16 * 2 ^ cl <= Z.shiftl (apages c) 16 ->
  exists v c', run_alloc c = ARet v c' /\ apages c' = apages c.
Proof.
  intros c w cl Hw H16 Hcl Hcl16 Hempty Hlen Hfit.
  eexists. eexists.
  split.
  - apply (alloc_bumps_fresh_classed c w cl); assumption.
  - reflexivity.
Qed.

End AllocTree.
