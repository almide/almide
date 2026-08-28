(* Almide v1 trust spine — #576 first slice: the STRUCTURAL leg's emitted
   runtime trees realize the modeled RC/free-list transitions.

   FC-3 proved the MODEL (FreeList.v / FreeListRc.v / RuntimeModel.v): the
   free-list discipline never hands out an aliased or freed block, and the
   RC transitions preserve the heap invariant. FC-3's own coverage-boundary
   column names the remaining gap: the EMITTED allocator's conformance was
   gated (alloc ledger, alias killers), not proven. This file closes the
   instruction-TREE half of that gap for the structural wasm leg
   (crates/almide-wasm/src/runtime_alloc.rs) — the leg #1584 is retiring
   the incumbent toward, so the proof lands on the runtime that stays.

   WHAT IS BOUND (the WasmRcDec.v precedent, one leg over): the exact
   instruction trees `emit_inc` / `emit_dec_flat` / `emit_free` produce,
   transcribed 1:1 as data (`inc_body` / `dec_body` / `free_body`), given
   an operational semantics, and proven to realize `RuntimeModel.rt_inc`,
   `rt_dec`'s success branch, and the size-class LIST-PUSH that concretely
   implements `FreeList.free_op`'s free-set insertion. On top, the two
   CLASS-MATH keystones nothing else states:

     `class_covers` / `serves_request` — the ceil-class capacity written
        at take covers every length the class serves (an under-serving
        class would be silent heap corruption — the F1-F7 shape);
     `file_take_agree` — a block capped by take at class c is filed by
        free back into EXACTLY class c, so reuse always looks where
        filing landed (the churn gate once measured 123 MB of misses
        from precisely this drift).

   HONEST SCOPE, stated plainly:
   - Tree level, not byte level: the trees are transcribed from
     runtime_alloc.rs and pinned against the EMITTED bytes by the
     Rust-side hash pin (crates/almide-wasm/tests/runtime_tree_pin.rs);
     drift on either side fails loudly, but the byte-decode theorem
     (WasmDecode's role for rc_inc) is the recorded next half.
   - `$dec_flat` carries NO zero-sentinel (unlike the incumbent's
     `$rc_dec`, whose trap WasmRcDec.v models): the structural leg's
     double-free defense is the Perceus certificate upstream. The
     realization theorem therefore carries the precondition `1 <= rc` —
     the PCC framing: the model VALIDATES what the checker guarantees;
     the emitted code does not re-check it.
   - `clz32` is DEFINED as `31 - Z.log2` on positives; the i32-op
     agreement over the bounded operand ranges is part of the tree-level
     trust, like every operator spelling in the transcription.
   - `$alloc`'s own tree (take/bump/grow/OOM) is the next slice; this
     file proves the class MATH both of its paths share with `$free`,
     and the cap constant it writes (`16<<class - PAYLOAD`) appears here
     as the agreement theorem's hypothesis.

   Wasm i32 vs Z: every value these trees touch is a nonnegative address,
   length or count well below 2^31, and the theorems carry the needed
   bounds explicitly — under them the i32 comparisons coincide with the
   Z ones (the WasmRcDec convention). *)

From AlmideTrust Require Import RuntimeModel.
From Stdlib Require Import ZArith.
From Stdlib Require Import List.
Import ListNotations.
From Stdlib Require Import Lia.
Open Scope Z_scope.

(* ── Layout/runtime constants (matched by the Rust-side pin):
      almide_layout: RC = 0, CAP = 8, PAYLOAD = 12; 16 size classes. ── *)
Definition CAP_OFFSET : Z := 8.
Definition PAYLOAD : Z := 12.
Definition CLASSES : Z := 16.

(* count-leading-zeros of a positive i32, spelled through log2. *)
Definition clz32 (z : Z) : Z := 31 - Z.log2 z.

(* The ceil size-class both `$alloc` and `$free` compute: `28 - clz(x-1)`. *)
Definition class_of (x : Z) : Z := 28 - clz32 (x - 1).

(* ══ THE CLASS-MATH KEYSTONES ══════════════════════════════════════════ *)

Lemma class_of_log2 : forall x, class_of x = Z.log2 (x - 1) - 3.
Proof. intros x. unfold class_of, clz32. lia. Qed.

(* Every class-eligible request (the emitted `want >= 16` clamp) has a
   nonnegative class. *)
Lemma class_nonneg : forall x, 16 <= x -> 0 <= class_of x.
Proof.
  intros x Hx. rewrite class_of_log2.
  assert (H : 3 <= Z.log2 (x - 1)).
  { replace 3 with (Z.log2 8) by reflexivity.
    apply Z.log2_le_mono. lia. }
  lia.
Qed.

(* CEIL: the class capacity `16 * 2^class` covers x. An under-covering
   class hands out a block smaller than the request — the corruption
   shape. *)
Lemma class_covers : forall x, 16 <= x -> x <= 16 * 2 ^ class_of x.
Proof.
  intros x Hx. rewrite class_of_log2.
  set (k := Z.log2 (x - 1)).
  assert (Hk3 : 3 <= k).
  { subst k. replace 3 with (Z.log2 8) by reflexivity.
    apply Z.log2_le_mono. lia. }
  assert (Hup : x - 1 < 2 ^ Z.succ k).
  { subst k. apply Z.log2_spec. lia. }
  assert (Hsplit : 16 * 2 ^ (k - 3) = 2 ^ Z.succ k).
  { replace 16 with (2 ^ 4) by reflexivity.
    rewrite <- Z.pow_add_r by lia.
    f_equal. lia. }
  lia.
Qed.

(* `log2 (2^n - 1) = n - 1` for n >= 1 — the exact-capacity round trip. *)
Lemma log2_pow2m1 : forall n, 1 <= n -> Z.log2 (2 ^ n - 1) = n - 1.
Proof.
  intros n Hn.
  apply Z.log2_unique.
  - lia.
  - split.
    + assert (2 ^ (n - 1) < 2 ^ n) by (apply Z.pow_lt_mono_r; lia).
      lia.
    + replace (Z.succ (n - 1)) with n by lia. lia.
Qed.

(* AGREEMENT: a block whose class-rounded total is exactly the class-c
   capacity files back into class c — take looks exactly where free
   filed. *)
Lemma file_take_agree : forall c, 0 <= c -> class_of (16 * 2 ^ c) = c.
Proof.
  intros c Hc. rewrite class_of_log2.
  replace (16 * 2 ^ c - 1) with (2 ^ (c + 4) - 1).
  2:{ replace 16 with (2 ^ 4) by reflexivity.
      rewrite <- Z.pow_add_r by lia. f_equal. f_equal. lia. }
  rewrite log2_pow2m1 by lia. lia.
Qed.

(* SERVE: `$alloc` writes `cap = 16*2^class - PAYLOAD` on a class take;
   with `want >= PAYLOAD + len` (the align-up) and `want >= 16` (the
   clamp), the served capacity covers the requested length. The
   under-serve class is unrepresentable. *)
Lemma serves_request : forall len want,
  0 <= len ->
  PAYLOAD + len <= want ->
  16 <= want ->
  len <= 16 * 2 ^ class_of want - PAYLOAD.
Proof.
  intros len want Hlen Hwant H16.
  pose proof (class_covers want H16). unfold PAYLOAD in *. lia.
Qed.

(* The align-4 mask on `(4-aligned) + 3` is the identity — the shape the
   agreement theorem meets: free recomputes
   `total = (cap + PAYLOAD + 3) & -4` over the cap alloc wrote, and for a
   class take that sum is `16*2^c + 3` with `16*2^c` 4-aligned. *)
Lemma land_m4_aligned : forall k, 0 <= k -> Z.land (4 * k + 3) (-4) = 4 * k.
Proof.
  intros k Hk.
  replace (-4) with (Z.lnot 3) by reflexivity.
  assert (Hsub : Z.land (4 * k + 3) (Z.lnot 3)
                 = (4 * k + 3) - Z.land (4 * k + 3) 3).
  { symmetry. apply Z.sub_land_same_l. }
  rewrite Hsub.
  assert (Hmod : Z.land (4 * k + 3) (Z.ones 2) = (4 * k + 3) mod 2 ^ 2).
  { apply Z.land_ones. lia. }
  change (Z.ones 2) with 3 in Hmod.
  rewrite Hmod.
  replace (4 * k + 3) with (3 + k * 4) by lia.
  rewrite Z_mod_plus_full.
  replace (3 mod 2 ^ 2) with 3 by reflexivity.
  lia.
Qed.

(* The end-to-end round trip: a block taken at class c (cap written
   `16*2^c - PAYLOAD`) is re-totaled by free to exactly `16*2^c` and
   filed into class c. One theorem, both constants. *)
Theorem free_refiles_takes_class : forall c,
  0 <= c ->
  class_of (Z.land ((16 * 2 ^ c - PAYLOAD) + PAYLOAD + 3) (-4)) = c.
Proof.
  intros c Hc.
  replace ((16 * 2 ^ c - PAYLOAD) + PAYLOAD + 3) with (4 * (4 * 2 ^ c) + 3)
    by (unfold PAYLOAD; lia).
  rewrite land_m4_aligned.
  2:{ assert (0 < 2 ^ c) by (apply Z.pow_pos_nonneg; lia). lia. }
  replace (4 * (4 * 2 ^ c)) with (16 * 2 ^ c) by lia.
  apply file_take_agree. exact Hc.
Qed.

(* Pointwise-update disagreement (the WasmIsa.upd_other fact, local so
   this file leans only on RuntimeModel). *)
Lemma upd_neq : forall m a v b, b <> a -> upd m a v b = m b.
Proof.
  intros m a v b H. unfold upd.
  replace (b =? a) with false by (symmetry; apply Z.eqb_neq; exact H).
  reflexivity.
Qed.

(* ══ THE EMITTED TREES, OPERATIONALLY ══════════════════════════════════

   The tiny statement language mirrors runtime_alloc.rs 1:1. Expressions
   are pure; a body is a statement list; `SIf` guards exactly ONE
   statement (every `if` in these trees does), which keeps the runner
   structurally recursive with early `SRet` propagation. Layout constants
   appear as their computed literals (8 = CAP offset, 12 = PAYLOAD,
   15 = PAYLOAD + 3, 16 = the class count and the minimum classed total)
   so the operational goals stay in one spelling; the pure keystones
   above carry the named forms. *)

Section Trees.

(* Parameters of one invocation: the block argument, the heap floor
   (G_LINE_END's value), and the free-list base address. *)
Variables blk floor fbase : Z.

Inductive expr : Type :=
  | EC (z : Z)
  | EBlk            (* local.get $block *)
  | ETot            (* local.get $total  (free)      *)
  | ECls            (* local.get $class  (free)      *)
  | ETmp            (* local.get $rc     (dec_flat)  *)
  | EFloor          (* global.get $line_end          *)
  | EAdd (a b : expr)
  | ESub (a b : expr)
  | ELand (a b : expr)
  | EShl (a b : expr)
  | EClz (a : expr)
  | ELtU (a b : expr)
  | EGeU (a b : expr)
  | EEqz (a : expr)
  | ELoad (a : expr).

Record C := mkC { ctot : Z; ccls : Z; ctmp : Z; cm : Mem }.

Fixpoint ev (e : expr) (c : C) : Z :=
  match e with
  | EC z => z
  | EBlk => blk
  | ETot => ctot c
  | ECls => ccls c
  | ETmp => ctmp c
  | EFloor => floor
  | EAdd a b => ev a c + ev b c
  | ESub a b => ev a c - ev b c
  | ELand a b => Z.land (ev a c) (ev b c)
  | EShl a b => Z.shiftl (ev a c) (ev b c)
  | EClz a => clz32 (ev a c)
  | ELtU a b => if Z.ltb (ev a c) (ev b c) then 1 else 0
  | EGeU a b => if Z.geb (ev a c) (ev b c) then 1 else 0
  | EEqz a => if Z.eqb (ev a c) 0 then 1 else 0
  | ELoad a => cm c (ev a c)
  end.

Inductive stmt : Type :=
  | SSetTot (e : expr)
  | SSetCls (e : expr)
  | SSetTmp (e : expr)
  | SStore (addr v : expr)
  | SIf (cond : expr) (body : stmt)
  | SRet
  | SCallFree.

(* One statement: `(returned?, state)`. `fsem` is the semantics of
   `call $free` — instantiated below with free's own runner, so dec's
   theorem composes with free's. *)
Fixpoint sstep (fsem : C -> C) (s : stmt) (c : C) : bool * C :=
  match s with
  | SSetTot e => (false, mkC (ev e c) (ccls c) (ctmp c) (cm c))
  | SSetCls e => (false, mkC (ctot c) (ev e c) (ctmp c) (cm c))
  | SSetTmp e => (false, mkC (ctot c) (ccls c) (ev e c) (cm c))
  | SStore a v => (false, mkC (ctot c) (ccls c) (ctmp c)
                              (upd (cm c) (ev a c) (ev v c)))
  | SIf e b => if Z.eqb (ev e c) 0 then (false, c) else sstep fsem b c
  | SRet => (true, c)
  | SCallFree => (false, fsem c)
  end.

Fixpoint srun (fsem : C -> C) (ss : list stmt) (c : C) : C :=
  match ss with
  | [] => c
  | s :: rest =>
      let '(r, c') := sstep fsem s c in
      if (r : bool) then c' else srun fsem rest c'
  end.

Definition nofree : C -> C := fun c => c.

(* ── `$inc` — emit_inc, 1:1 (RC offset 0 folds into the bare address):
     if (block < line_end) return; rc(block) += 1. ── *)
Definition inc_body : list stmt :=
  [ SIf (ELtU EBlk EFloor) SRet;
    SStore EBlk (EAdd (ELoad EBlk) (EC 1)) ].

(* ── `$dec_flat` — emit_dec_flat, 1:1:
     if (block < line_end) return;
     rc = load(block) - 1; store(block, rc);
     if (rc == 0) call $free. ── *)
Definition dec_body : list stmt :=
  [ SIf (ELtU EBlk EFloor) SRet;
    SSetTmp (ESub (ELoad EBlk) (EC 1));
    SStore EBlk ETmp;
    SIf (EEqz ETmp) SCallFree ].

(* ── `$free` — emit_free, 1:1:
     total = (load(block+8 = CAP) + 12 + 3) & -4;
     if (total < 16) return;
     class = 28 - clz(total - 1);
     if (class >= 16 = CLASSES) return;
     class = (class << 2) + FREELIST_BASE;
     store(block + 12 = PAYLOAD, load(class));  // block.payload[0] = head
     store(class, block).                       // head = block ── *)
Definition free_body : list stmt :=
  [ SSetTot (ELand (EAdd (ELoad (EAdd EBlk (EC 8))) (EC 15)) (EC (-4)));
    SIf (ELtU ETot (EC 16)) SRet;
    SSetCls (ESub (EC 28) (EClz (ESub ETot (EC 1))));
    SIf (EGeU ECls (EC 16)) SRet;
    SSetCls (EAdd (EShl ECls (EC 2)) (EC fbase));
    SStore (EAdd EBlk (EC 12)) (ELoad ECls);
    SStore ECls EBlk ].

Definition run_inc (c : C) : C := srun nofree inc_body c.
Definition run_free (c : C) : C := srun nofree free_body c.
Definition run_dec (c : C) : C := srun run_free dec_body c.

(* Restricted reduction: compute our own runner/eval structure while the
   Z operators stay folded, so `replace` targets survive. *)
Ltac rcbn :=
  cbn -[Z.ltb Z.geb Z.leb Z.eqb Z.land Z.shiftl Z.add Z.sub Z.mul
        Z.opp Z.pow clz32].

(* ══ REALIZATION THEOREMS ══════════════════════════════════════════════ *)

(* Below the heap floor (pool statics, null): no-ops on memory — the
   blind-emit guard. *)
Theorem inc_below_floor_noop : forall c,
  blk < floor -> cm (run_inc c) = cm c.
Proof.
  intros c H. unfold run_inc, inc_body. cbn.
  replace (blk <? floor) with true by (symmetry; apply Z.ltb_lt; exact H).
  reflexivity.
Qed.

Theorem dec_below_floor_noop : forall c,
  blk < floor -> cm (run_dec c) = cm c.
Proof.
  intros c H. unfold run_dec, dec_body. cbn.
  replace (blk <? floor) with true by (symmetry; apply Z.ltb_lt; exact H).
  reflexivity.
Qed.

(* `$inc` realizes `RuntimeModel.rt_inc` — the cell bump, verbatim
   (RC_OFFSET = 0, so the cell address is the block base). *)
Theorem inc_realizes_rt_inc : forall c,
  floor <= blk ->
  cm (run_inc c) = rt_inc (cm c) blk.
Proof.
  intros c H. unfold run_inc, inc_body. cbn.
  replace (blk <? floor) with false
    by (symmetry; apply Z.ltb_ge; exact H).
  cbn. unfold rt_inc, read_rc, RC_OFFSET.
  rewrite Z.add_0_r. reflexivity.
Qed.

(* `$dec_flat`, shared block (rc >= 2): realizes `rt_dec`'s success
   branch — decrement only, `$free` NOT reached. rt_dec's `1 <= rc`
   precondition is subsumed. *)
Theorem dec_shared_realizes_rt_dec : forall c,
  floor <= blk ->
  2 <= cm c blk ->
  rt_dec (cm c) blk = Some (cm (run_dec c))
  /\ cm (run_dec c) = upd (cm c) blk (cm c blk - 1).
Proof.
  intros c Hf Hrc. unfold run_dec, dec_body. cbn.
  replace (blk <? floor) with false
    by (symmetry; apply Z.ltb_ge; exact Hf).
  cbn.
  replace (cm c blk - 1 =? 0) with false
    by (symmetry; apply Z.eqb_neq; lia).
  cbn.
  unfold rt_dec, read_rc, RC_OFFSET.
  rewrite Z.add_0_r.
  replace (cm c blk <=? 0) with false
    by (symmetry; apply Z.leb_gt; lia).
  split; reflexivity.
Qed.

(* `$dec_flat`, uniquely-held block (rc = 1): the cell reaches 0 (exactly
   `rt_dec`'s store), and control TRANSFERS to `$free` on the zeroed
   state — the filing half is free's own theorem, composed below. *)
Theorem dec_unique_hands_to_free : forall c,
  floor <= blk ->
  cm c blk = 1 ->
  run_dec c = run_free (mkC (ctot c) (ccls c) 0
                            (upd (cm c) blk 0)).
Proof.
  intros c Hf Hrc. unfold run_dec, dec_body. cbn.
  replace (blk <? floor) with false
    by (symmetry; apply Z.ltb_ge; exact Hf).
  cbn. rewrite Hrc. cbn.
  reflexivity.
Qed.

(* `$free`, the abandon guards: a total too small to hold the next
   pointer leaves memory untouched — the bump-graveyard behavior. *)
Theorem free_abandons_small : forall c,
  Z.land (cm c (blk + 8) + 15) (-4) < 16 ->
  cm (run_free c) = cm c.
Proof.
  intros c H. unfold run_free, free_body. cbn.
  set (t := Z.land (cm c (blk + 8) + 15) (-4)) in *.
  replace (t <? 16) with true by (symmetry; apply Z.ltb_lt; exact H).
  cbn. reflexivity.
Qed.

(* ... as does a total whose class overflows the 16-entry table. *)
Theorem free_abandons_huge : forall c t,
  t = Z.land (cm c (blk + 8) + 15) (-4) ->
  16 <= t ->
  16 <= class_of t ->
  cm (run_free c) = cm c.
Proof.
  intros c t Ht H16 Hcls. unfold run_free, free_body.
  rcbn. rewrite <- Ht.
  replace (t <? 16) with false by (symmetry; apply Z.ltb_ge; exact H16).
  rcbn. replace (0 =? 0) with true by reflexivity. rcbn.
  replace (28 - clz32 (t - 1) >=? 16) with true.
  2:{ symmetry. apply Z.geb_le. unfold class_of in Hcls. exact Hcls. }
  rcbn. replace (1 =? 0) with false by reflexivity. rcbn.
  reflexivity.
Qed.

(* `$free`, the FILING: for a class-eligible total, memory receives
   exactly the two-store LIST PUSH — `block.payload[0] := head` then
   `head := block` on the class slot `fbase + 4*class`. This is the
   concrete realization of `FreeList.free_op`'s free-set insertion: the
   freed block becomes the head the next same-class take pops. *)
Theorem free_files_by_class : forall c t,
  t = Z.land (cm c (blk + 8) + 15) (-4) ->
  16 <= t ->
  class_of t < 16 ->
  cm (run_free c)
  = (let slot := fbase + 4 * class_of t in
     upd (upd (cm c) (blk + 12) (cm c slot)) slot blk).
Proof.
  intros c t Ht H16 Hcls. unfold run_free, free_body.
  rcbn. rewrite <- Ht.
  replace (t <? 16) with false by (symmetry; apply Z.ltb_ge; exact H16).
  rcbn. replace (0 =? 0) with true by reflexivity. rcbn.
  replace (28 - clz32 (t - 1) >=? 16) with false.
  2:{ symmetry. rewrite Z.geb_leb. apply Z.leb_gt.
      unfold class_of in Hcls. exact Hcls. }
  rcbn. replace (0 =? 0) with true by reflexivity. rcbn.
  unfold class_of.
  rewrite Z.shiftl_mul_pow2 by lia.
  replace ((28 - clz32 (t - 1)) * 2 ^ 2 + fbase)
    with (fbase + 4 * (28 - clz32 (t - 1))) by lia.
  reflexivity.
Qed.

(* The COMPOSED release: a uniquely-held block whose cap was written by
   a class-c take (`16*2^c - 12`) decs to 0 and lands as the head of
   EXACTLY class c — `dec → free → the agreed slot`, end to end. The
   `c < 16` bound is the take path's own guard (only classed takes write
   this cap shape). *)
Theorem dec_unique_files_take_class : forall c cl,
  floor <= blk ->
  cm c blk = 1 ->
  0 <= cl ->
  cl < 16 ->
  cm c (blk + 8) = 16 * 2 ^ cl - 12 ->
  cm (run_dec c)
  = (let m0 := upd (cm c) blk 0 in
     let slot := fbase + 4 * cl in
     upd (upd m0 (blk + 12) (m0 slot)) slot blk).
Proof.
  intros c cl Hf Hrc Hcl Hclc Hcap.
  rewrite (dec_unique_hands_to_free c Hf Hrc).
  set (m0 := upd (cm c) blk 0).
  set (c0 := mkC (ctot c) (ccls c) 0 m0).
  assert (Hcap0 : m0 (blk + 8) = 16 * 2 ^ cl - 12).
  { unfold m0. rewrite upd_neq by lia. exact Hcap. }
  set (t := Z.land (m0 (blk + 8) + 15) (-4)).
  assert (Hpow : 0 < 2 ^ cl) by (apply Z.pow_pos_nonneg; lia).
  assert (Htv : t = 16 * 2 ^ cl).
  { unfold t. rewrite Hcap0.
    replace (16 * 2 ^ cl - 12 + 15) with (4 * (4 * 2 ^ cl) + 3) by lia.
    rewrite land_m4_aligned by lia.
    lia. }
  assert (H16 : 16 <= t) by lia.
  assert (Hclt : class_of t = cl).
  { rewrite Htv. apply file_take_agree. exact Hcl. }
  rewrite (free_files_by_class c0 t).
  - unfold c0. cbn [cm]. rewrite Hclt. reflexivity.
  - unfold t, c0. reflexivity.
  - exact H16.
  - rewrite Hclt. exact Hclc.
Qed.

End Trees.
