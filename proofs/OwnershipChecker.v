(* Almide v1 trust kernel — proof spine, brick 1.

   The flight-grade thesis in miniature (docs/roadmap/active/v1-mir-architecture.md
   §5; the tier-1 stack): we do NOT prove the compiler. We prove that a tiny
   CHECKER is sound:

       check(artifact) = accept  ==>  P(artifact)

   so the (large, untrusted, possibly-buggy) compiler can emit anything — if its
   output is wrong, the checker rejects it. Here P is the first flight-grade
   property, MEMORY SAFETY / RC BALANCE, over the MIR ownership op sequence
   (`almide-mir`'s `verify_ownership`, ported to Gallina). This file is checked
   by the Coq/Rocq kernel; later bricks add the certificate format, more
   properties, CertiCoq+CompCert extraction of the checker to machine code, and
   the translation validator V (model ⊒ wasm bytes). *)

From Stdlib Require Import List.
Import ListNotations.
From Stdlib Require Import ZArith.
From Stdlib Require Import String Ascii.
From Stdlib Require Import Lia Wf_nat.
Open Scope Z_scope.

(* The MIR ownership ops on a single reference-counted object — certificate
   format v1's ownership ALPHABET. Each op carries a SIGNED DELTA the checker
   folds; the DISTINCT constructors record a ground fact the untrusted compiler
   already decided (so the checker never re-derives it):
     Inc     = +1 FRESH acquire   (Alloc / fresh Dup / owned Call-result).
     Alias   = +1 ALIAS acquire   (a binding that incs an existing SHARED ref —
                                   the share-vs-move ground fact, G2.1). Folds
                                   like Inc; the separate constructor is the fact.
     Dec     = −1 plain release   (Drop).
     MoveOut = −1 MOVE-OUT        (Consume — ref transferred to a container /
                                   return / consuming callee). Folds like Dec.
   v0's {Inc,Dec} is the DEGENERATE case (Alias≡Inc, MoveOut≡Dec at the balance
   fold), so this strictly generalizes brick 1 with ZERO new proof obligations —
   the soundness proofs reason about the run's Z result, not the constructors.
     Reuse   = REFUSED (#4753). It once claimed a release on a UNIQUELY owned
               block, valid iff this line's count was 1. But a line counts one
               HOLDER's credits, not the block's: a credit moved out (`m`),
               shared out (`s`) or held by a borrowed parameter's caller is
               invisible to it, so `iamr` / `ar` / `isr` balanced and were
               accepted while another holder still held the block. No producer
               emits it; it now always FAULTS. Uniqueness is a RUNTIME fact:
               the `u` Probe (ADR-0034) plus the shipped count test.
     Borrow  = +0 USE of a live reference (a read-only borrow / in-place unique
               use — `Op::Borrow`/`MakeUnique`). No refcount change, but a
               LIVENESS guard: a borrow of a DEAD object (rc = 0 — every owned
               reference already released) is a use-after-free and FAULTS. This
               makes owned-object use-after-free WITNESSABLE (brick 5b): `idb`
               rejects. Borrowed PARAMS stay event-free (their liveness is the
               caller's obligation — the call-mode system, CallModes.v). *)
Inductive Op : Type :=
  | Inc : Op
  | Alias : Op
  | Dec : Op
  | MoveOut : Op
  | Reuse : Op
  | Borrow : Op
  (* ShareOut = +0 on ITS line, the SHARE-OUT of an arm (#4278): a stdlib arm
     returns its argument block as its owned result on some path, after an
     `rc_inc` on it. The +1 is the arm result's credit — the result's own line
     is born by `i` at the consumer — so on the ARGUMENT's line the share is
     the pair "share +1, credit moved into the result −1" (`am`), net 0. It is
     its own constructor (not a recorded `am`) because it names a different
     instruction: the RC byte check (almide-anchor-check rc.rs) holds it to
     the `rc_inc` inside the arm's own window, read through the argument's
     local. `expand` below is that reading, and `exec_expand` proves the fold
     agrees with it; `share_handover_sound` proves the two accounts on the one
     block (the argument's and the result's) never fault together and return
     the block to the count it had. *)
  | ShareOut : Op
  (* Take = +1 on the RECEIVING holder's line (ADR-0034, format v8: the `k`
     byte). A credit MOVED out of a container slot that was cleared in the
     same step — not an allocation (`i`), not an `rc_inc` (`a`): no RC
     instruction exists for it. It folds exactly like Inc; the distinct
     constructor is the ground fact the RC byte check holds to the slot's
     null store under a uniqueness test (almide-anchor-check rc/take.rs).
     The slot's own account loses the credit: `take_handover_sound`. *)
  | Take : Op
  (* Probe = +0 on the CONTAINER's line (ADR-0034, the `u` byte): the
     shipped code tested the container's runtime count for 1 here. The guard
     is Borrow's: this holder must own a credit (count > 0). Static ownership
     (this guard) and the dynamic count (the shipped test) together give
     exclusivity — `UniqueTake.probe_exclusive`; neither alone does. After a
     Probe the line may only probe again or end in one release (`drained`). *)
  | Probe : Op
  (* Reclaim = −1 on its line (the `w` byte, certificate format v9): the
     holder's credit is discharged by a REGION RESTORE, not by a release
     call. A `consume(produce(scalars))` region window (#1961) parks the
     producer's result for the consumer's borrowed parameter and never
     releases it block by block: `RegionRestore` rewinds the bump pointer and
     the class heads past every block the window took (RuntimeModel.v
     `region_restore_reclaims`). It folds exactly like Dec — a reclaim of a
     line holding no credit FAULTS — so, like Alias / MoveOut, it costs the
     balance theorems nothing; the distinct constructor is the ground fact
     the RC byte check holds to the shipped restore (almide-anchor-check
     rc.rs, `w-@<window>`), where a `d` is held to a release call. *)
  | Reclaim : Op.

(* OPERATIONAL SEMANTICS (the ALS side — "what actually happens").
   A refcount, or a FAULT (`None`) when a −1 op hits rc = 0: that is a
   double-free / use-after-free — releasing a reference that does not exist.
   Alias folds like Inc (+1), MoveOut like Dec (−1): the balance is about the
   DELTAS, which is exactly why adding those ground-fact constructors costs no new
   proof. Reuse is the ONE exception: besides its −1 it carries a UNIQUENESS guard
   (valid only at rc = 1), so it faults on a SHARED object — the reuse-soundness
   obligation, checked by the same fold (`check_reuse_sound`). *)
Fixpoint exec (ops : list Op) (rc : Z) : option Z :=
  match ops with
  | [] => Some rc
  | Inc :: rest => exec rest (rc + 1)
  | Alias :: rest => exec rest (rc + 1)
  | Dec :: rest => if rc <=? 0 then None else exec rest (rc - 1)
  | MoveOut :: rest => if rc <=? 0 then None else exec rest (rc - 1)
  (* #4753: a line's count is ONE holder's account, never the block's count,
     so "count = 1 here" is not uniqueness (`iamr`, `ar`, `isr` balanced and
     were accepted while another holder still held the block). Uniqueness is
     a RUNTIME fact (ADR-0034's Probe + the shipped test); no producer emits
     `r`. So a Reuse on an ownership line always FAULTS. *)
  | Reuse :: _ => None
  (* Borrow is a +0 USE: it needs a live reference (rc > 0) but releases nothing.
     A borrow at rc = 0 is a use-after-free — the object's owned references are
     all gone — and FAULTS, exactly like verify_ownership's live-check on
     `Op::Borrow`/`MakeUnique`. *)
  | Borrow :: rest => if rc <=? 0 then None else exec rest rc
  (* ShareOut: the share +1 moves straight into the arm's result (the result's
     line holds it), so this line's count is unchanged. *)
  | ShareOut :: rest => exec rest rc
  | Take :: rest => exec rest (rc + 1)
  | Probe :: rest => if rc <=? 0 then None else exec rest rc
  | Reclaim :: rest => if rc <=? 0 then None else exec rest (rc - 1)
  end.

Definition run (ops : list Op) : option Z := exec ops 0.

(* THE CHECKER K (a Gallina function — total, decidable). Accept iff the run
   neither faults nor leaks: it ends with rc = 0. *)
Definition check (ops : list Op) : bool :=
  match run ops with
  | Some z => Z.eqb z 0
  | None => false
  end.

(* The SEMANTIC PROPERTY P, defined against the operational semantics:
   - no double-free / use-after-free: the run never faults;
   - no leak: every acquired reference is released (final rc = 0). *)
Definition no_double_free (ops : list Op) : Prop := run ops <> None.
Definition no_leak (ops : list Op) : Prop := run ops = Some 0.

(* SOUNDNESS: the checker accepting GUARANTEES the property. This is the whole
   proof-carrying-code thesis for the RC-balance property: trust shrinks from the
   compiler to `check` (+ this theorem). *)
Theorem check_sound :
  forall ops, check ops = true -> no_double_free ops /\ no_leak ops.
Proof.
  intros ops H.
  unfold check in H.
  unfold no_double_free, no_leak.
  destruct (run ops) as [z |] eqn:E.
  - (* run did not fault; H : (z =? 0) = true, so z = 0. *)
    apply Z.eqb_eq in H. subst z.
    split.
    + discriminate.        (* Some 0 <> None *)
    + reflexivity.         (* run ops = Some 0 *)
  - (* run faulted; H : false = true, impossible. *)
    discriminate.
Qed.

(* A small SANITY check that the checker is non-vacuous (it does accept real
   balanced sequences and reject faulty / leaky ones), so `check_sound` is not
   trivially true for an always-false checker. *)
Example accepts_balanced : check [Inc; Inc; Dec; Dec] = true.
Proof. reflexivity. Qed.

Example rejects_double_free : check [Inc; Dec; Dec] = false.
Proof. reflexivity. Qed.

Example rejects_leak : check [Inc; Inc; Dec] = false.
Proof. reflexivity. Qed.

(* MULTI-OBJECT. The real MIR has MANY reference-counted objects; `verify_ownership`
   accounts each separately. A whole-function certificate is therefore one
   Inc/Dec stream PER object. `check_all` accepts iff every object is balanced. *)
Definition check_all (objs : list (list Op)) : bool := forallb check objs.

(* SOUNDNESS lifts to every object: accepting the function certificate means
   EVERY object is free of double-free and leak. *)
Theorem check_all_sound :
  forall objs, check_all objs = true ->
    forall ops, In ops objs -> no_double_free ops /\ no_leak ops.
Proof.
  intros objs H ops Hin.
  unfold check_all in H. rewrite forallb_forall in H.
  apply check_sound. apply H. exact Hin.
Qed.

Example accepts_two_balanced_objects :
  check_all [[Inc; Dec]; [Inc; Inc; Dec; Dec]] = true.
Proof. reflexivity. Qed.

Example rejects_if_any_object_faulty :
  check_all [[Inc; Dec]; [Inc; Dec; Dec]] = false.
Proof. reflexivity. Qed.

(* ─── certificate parsing, INTERNALIZED INTO COQ ───
   The byte→op tokenizer used to live in the OCaml driver, OUTSIDE the trusted
   base (a known-limitation). Here it is a proven Gallina function: the WHOLE
   "bytes ⟶ accept/reject" pipeline is now kernel-checked, shrinking the trusted
   base to just file I/O. Certificate format v1: one object per newline; within a
   line the ownership alphabet is `i`/`I` = fresh +1, `a`/`A` = alias +1,
   `d`/`D` = release −1, `m`/`M` = move-out −1, `r`/`R` = reuse-release −1
   (perceus mode); anything else (whitespace included) skipped. (`a`/`m`/`r`
   carry ground facts — share-vs-move, reuse-uniqueness — but fold like `i`/`d`,
   so v0 certificates remain valid: i/d is the degenerate case.) *)

Definition newline : ascii := ascii_of_nat 10.

Definition parse_byte (a : ascii) : option Op :=
  if orb (Ascii.eqb a "i"%char) (Ascii.eqb a "I"%char) then Some Inc
  else if orb (Ascii.eqb a "a"%char) (Ascii.eqb a "A"%char) then Some Alias
  else if orb (Ascii.eqb a "d"%char) (Ascii.eqb a "D"%char) then Some Dec
  else if orb (Ascii.eqb a "m"%char) (Ascii.eqb a "M"%char) then Some MoveOut
  else if orb (Ascii.eqb a "r"%char) (Ascii.eqb a "R"%char) then Some Reuse
  else if orb (Ascii.eqb a "b"%char) (Ascii.eqb a "B"%char) then Some Borrow
  else if orb (Ascii.eqb a "s"%char) (Ascii.eqb a "S"%char) then Some ShareOut
  else if orb (Ascii.eqb a "k"%char) (Ascii.eqb a "K"%char) then Some Take
  else if orb (Ascii.eqb a "u"%char) (Ascii.eqb a "U"%char) then Some Probe
  else if orb (Ascii.eqb a "w"%char) (Ascii.eqb a "W"%char) then Some Reclaim
  else None.

(* Fold the byte string into per-line op streams; flush the final line at end. *)
Fixpoint parse_go (s : string) (cur : list Op) : list (list Op) :=
  match s with
  | EmptyString => [rev cur]
  | String b rest =>
      if Ascii.eqb b newline then rev cur :: parse_go rest []
      else match parse_byte b with
           | Some op => parse_go rest (op :: cur)
           | None => parse_go rest cur
           end
  end.

Definition parse (s : string) : list (list Op) := parse_go s [].

(* The full proven checker over raw certificate bytes. *)
Definition check_cert (s : string) : bool := check_all (parse s).

(* SOUNDNESS over bytes: accepting the certificate bytes guarantees every object
   parsed from them is free of double-free and leak. The tokenizer is now inside
   the proof. *)
Theorem check_cert_sound :
  forall s, check_cert s = true ->
    forall ops, In ops (parse s) -> no_double_free ops /\ no_leak ops.
Proof.
  intros s H. unfold check_cert in H. apply check_all_sound. exact H.
Qed.

Example cert_balanced_accepts : check_cert "iidd"%string = true.
Proof. reflexivity. Qed.

Example cert_double_free_rejects : check_cert "idd"%string = false.
Proof. reflexivity. Qed.

(* a two-object certificate (newline-separated) — exercises line splitting *)
Definition cert_two_objs : string :=
  String "i"%char (String "d"%char (String newline (String "i"%char (String "d"%char EmptyString)))).
Example cert_two_objs_accepts : check_cert cert_two_objs = true.
Proof. reflexivity. Qed.

(* format-v1 alphabet: alias (`a`) and move-out (`m`) fold like inc/dec, so the
   share-vs-move ground fact rides along without changing the balance verdict. *)
Example accepts_alias_then_move : check [Inc; Alias; Dec; MoveOut] = true.
Proof. reflexivity. Qed.

Example cert_move_out_accepts : check_cert "im"%string = true.   (* alloc, move-out (the return_list witness) *)
Proof. reflexivity. Qed.

Example cert_alias_balanced : check_cert "iadd"%string = true.   (* alloc, alias, two releases *)
Proof. reflexivity. Qed.

Example cert_move_out_underflow_rejects : check_cert "m"%string = false. (* move-out with nothing owned = use-after-move *)
Proof. reflexivity. Qed.

(* #4753: `r` is refused on every ownership line — a line's count of 1 is not
   uniqueness (see `exec`). Even the alloc-then-reuse control rejects now. *)
Example cert_reuse_refused : check_cert "ir"%string = false.
Proof. reflexivity. Qed.

(* ─── REUSE SOUNDNESS (the perceus uniqueness obligation, A1 "+reuse健全性") ───
   A `Reuse` event asserts the block is UNIQUELY owned so it can be repurposed in
   place. `reuses_unique` is the decidable property "every Reuse in the run acts
   at rc = 1" — exactly what makes in-place reuse safe (no aliasing owner to
   corrupt). It mirrors `exec` but only watches the Reuse sites. *)
Fixpoint reuses_unique (ops : list Op) (rc : Z) : bool :=
  match ops with
  | [] => true
  | Inc :: rest => reuses_unique rest (rc + 1)
  | Alias :: rest => reuses_unique rest (rc + 1)
  | Dec :: rest => if rc <=? 0 then true else reuses_unique rest (rc - 1)
  | MoveOut :: rest => if rc <=? 0 then true else reuses_unique rest (rc - 1)
  | Reuse :: rest => if Z.eqb rc 1 then reuses_unique rest 0 else false
  (* Borrow mirrors exec: a fault (rc = 0) ends the run (vacuously true), a live
     borrow changes nothing. *)
  | Borrow :: rest => if rc <=? 0 then true else reuses_unique rest rc
  | ShareOut :: rest => reuses_unique rest rc
  | Take :: rest => reuses_unique rest (rc + 1)
  | Probe :: rest => if rc <=? 0 then true else reuses_unique rest rc
  | Reclaim :: rest => if rc <=? 0 then true else reuses_unique rest (rc - 1)
  end.

(* The bridge: a run that does NOT fault has every Reuse at rc = 1. This holds
   because `exec`'s Reuse arm FAULTS unless rc = 1 — so a non-faulting run cannot
   contain a shared reuse. (Pure proof-reuse of the tightened fold; no new axiom.) *)
Lemma exec_ok_reuses_unique :
  forall ops rc, exec ops rc <> None -> reuses_unique ops rc = true.
Proof.
  induction ops as [| o rest IH]; intros rc H.
  - reflexivity.
  - destruct o; simpl in *.
    + apply IH. exact H.
    + apply IH. exact H.
    + destruct (rc <=? 0) eqn:E. { exfalso. apply H. reflexivity. } apply IH. exact H.
    + destruct (rc <=? 0) eqn:E. { exfalso. apply H. reflexivity. } apply IH. exact H.
    + exfalso. apply H. reflexivity.
    + destruct (rc <=? 0) eqn:E. { exfalso. apply H. reflexivity. } apply IH. exact H.
    + apply IH. exact H.
    + apply IH. exact H.
    + destruct (rc <=? 0) eqn:E. { exfalso. apply H. reflexivity. } apply IH. exact H.
    + destruct (rc <=? 0) eqn:E. { exfalso. apply H. reflexivity. } apply IH. exact H.
Qed.

(* REUSE SOUNDNESS: an accepted certificate has every Reuse acting on a UNIQUELY
   owned object — so the compiler's in-place-reuse decision is safe, re-derived by
   the checker's own fold, never trusting the compiler's uniqueness analysis. *)
Theorem check_reuse_sound :
  forall ops, check ops = true -> reuses_unique ops 0 = true.
Proof.
  intros ops H. apply exec_ok_reuses_unique.
  unfold check, run in H. destruct (exec ops 0) eqn:E.
  - intro Hcon. discriminate Hcon.
  - discriminate H.
Qed.

(* Lifted to a whole-function / certificate: every object's reuses are sound. *)
Theorem check_all_reuse_sound :
  forall objs, check_all objs = true ->
    forall ops, In ops objs -> reuses_unique ops 0 = true.
Proof.
  intros objs H ops Hin. unfold check_all in H. rewrite forallb_forall in H.
  apply check_reuse_sound. apply H. exact Hin.
Qed.

Theorem check_cert_reuse_sound :
  forall s, check_cert s = true ->
    forall ops, In ops (parse s) -> reuses_unique ops 0 = true.
Proof.
  intros s H. unfold check_cert in H. apply check_all_reuse_sound. exact H.
Qed.

(* THE CLOSED HOLE (non-vacuous). `iard` = alloc, ALIAS (rc 1→2), REUSE (rc 2),
   release — it BALANCES to 0, so the bare RC-balance checker ACCEPTED it. But it
   reuses a SHARED object in place (an aliasing bug). The uniqueness fold now
   REJECTS it: a reuse at rc = 2 ≠ 1 faults. This is the gate that makes the
   shared-reuse class non-recurring. *)
Example cert_shared_reuse_rejects : check_cert "iard"%string = false.
Proof. reflexivity. Qed.

Example reuses_unique_ir : reuses_unique [Inc; Reuse] 0 = true.
Proof. reflexivity. Qed.

Example reuses_shared_iard_not_unique : reuses_unique [Inc; Alias; Reuse; Dec] 0 = false.
Proof. reflexivity. Qed.

(* ─── the `b` (borrow, +0) letter — brick 5b non-vacuity ───
   A borrow needs a LIVE owned reference and changes nothing: `ibd` accepts.
   A borrow AFTER the last release (`idb`) is a use-after-free — before this
   letter the cert could not witness it (the Borrow op was invisible); now it
   FAULTS. A borrow with nothing ever owned (`b`) faults the same way. *)
Example cert_borrow_live_accepts : check_cert "ibd"%string = true.
Proof. reflexivity. Qed.

Example cert_borrow_after_free_rejects : check_cert "idb"%string = false.
Proof. reflexivity. Qed.

Example cert_borrow_nothing_rejects : check_cert "b"%string = false.
Proof. reflexivity. Qed.

Example borrow_of_alias_accepts : check [Inc; Alias; Borrow; Dec; Dec] = true.
Proof. reflexivity. Qed.

(* ─── HEAP-LOOP-CARRIED extension (option C, the COMPLETENESS fix) ───
   The flat per-object cert above cannot express a loop-carried heap accumulator
   (`acc = acc + [x]` each iteration: drop the old object, alloc a new one, rebind
   the slot — different objects sharing one SLOT, the old's `d` an iteration after
   the new's `i`). So `verify_ownership` FALSE-REJECTS safe `acc + [x]` loops — an
   incompleteness. Here a cert LINE gains a LOOP item whose flat body runs ANY
   number of times; the accumulator slot's cert is `[COp Inc; CLoop [Dec; Inc];
   COp MoveOut]` (acquire once; each iteration release-old + acquire-new = net 0;
   move out the final). The rule: accept a loop iff its body PRESERVES rc (and does
   not fault) from the entry count — then EVERY iteration count is sound, PROVED
   below (`check_line_unroll_sound`). (OwnershipLoop.v proves the same rule on a
   focused alphabet; this is the production port over the full Inc/Alias/Dec/
   MoveOut/Reuse alphabet so the extracted checker accepts loop certs.) *)

(* ─── NESTED BRANCHES (certificate format v10) ───
   A one-shot branch whose arms may themselves contain one-shot branches: a
   sequence of ops (`NOp1`) and binary branches (`NBr`, its two arms and
   the rest of the arm it sits in). The producer writes
   one when the paths of a frame reach a point in the SAME holder state with
   different histories (a select whose two arms leave different holders that
   reconverge only later): the line is then `C{u|v}…` with `u`, `v` carrying
   branches of their own, instead of one whole path per history. Its fold
   `nexec` is CBranch's, recursively: both arms of every branch run
   fault-free from the count at the branch and AGREE, and the run continues
   at the agreed count. Kept out of CertItem itself so every other item stays
   flat; an arm here holds no loop, no exit and no abort (the parser poisons
   a marker or a loop inside a nested arm). One plain inductive, so every
   function over it is structural. *)
Inductive NOps : Type :=
  | NEnd : NOps
  | NOp1 : Op -> NOps -> NOps
  (* a branch: its two arms, then the rest of the arm it sits in *)
  | NBr : NOps -> NOps -> NOps -> NOps.

Fixpoint nexec (n : NOps) (rc : Z) : option Z :=
  match n with
  | NEnd => Some rc
  | NOp1 o rest =>
      match exec [o] rc with Some r => nexec rest r | None => None end
  | NBr t e rest =>
      match nexec t rc, nexec e rc with
      | Some a, Some b => if Z.eqb a b then nexec rest a else None
      | _, _ => None
      end
  end.

(* CONCRETE unrolling of a nested arm: every branch resolved to the ONE arm
   the runtime takes. *)
Inductive NUnrolls : NOps -> list Op -> Prop :=
  | NU_end : NUnrolls NEnd []
  | NU_op : forall o a b, NUnrolls a b -> NUnrolls (NOp1 o a) (o :: b)
  | NU_br : forall t e a ft b (choice : bool),
      NUnrolls (if choice then t else e) ft -> NUnrolls a b ->
      NUnrolls (NBr t e a) (ft ++ b).

Inductive CertItem : Type :=
  | COp : Op -> CertItem            (* a plain op *)
  | CLoop : list Op -> CertItem     (* a loop body run any number of times *)
  | CCondLoop : list Op -> list Op -> CertItem
    (* a CONDITIONAL loop (filter / filter_map): each iteration runs EITHER the
       `then` body (predicate true) OR the `else` body (predicate false). The
       number of `then` iterations is RUNTIME-VARIABLE. Accepted iff BOTH branch
       bodies PRESERVE rc from the entry count — then ANY per-iteration outcome
       sequence preserves it, for ANY iteration count (proved sound vs. the
       concrete unrolling `cond_concat` below). The unconditional `CLoop body`
       is the degenerate case `then = else = body`; this strictly generalizes it.
       Ported from OwnershipFilter.v (`cexec`/`ccheck_unroll_sound`, kernel-checked,
       axiom-clean) onto the production Inc/Alias/Dec/MoveOut/Reuse `exec`. *)
  | CBranch : list Op -> list Op -> CertItem
    (* a ONE-SHOT branch (brick 5a): the run takes EXACTLY ONE of the two flat
       arm bodies. Accepted iff BOTH arms execute from the entry count WITHOUT
       faulting to the SAME result — the arms AGREE on the leaving resource
       state (which may differ from the entry state: net +1 arms are the whole
       point — a heap-result branch). CCondLoop is the ITERATED cousin (its
       preservation requirement `r = rc` makes any iteration count sound); a
       one-shot branch only needs AGREEMENT. This retires the lowering's
       per-arm-balance TRUSTED convention: cross-arm compensation (an `i` in one
       arm balanced by a `d` in the other — runtime-unsafe either way the branch
       goes, yet flat-balanced) becomes structurally REJECTED. *)
  | CBranchRet : bool -> list Op -> list Op -> CertItem
    (* a one-shot branch whose flagged arm (true = then, false = else) ends in
       a frame-targeted EARLY RETURN (`Op::Return`, law 6 — format v5). The
       returning arm takes the WHOLE function-exit obligation at its exit:
       it must run fault-free from the entry count to EXACTLY 0 (the
       lowering's pre-return drops are its real `d`s; the returned value's
       move-out is its `m`). It then takes NO part in the join — the line
       CONTINUES from the SURVIVING arm's result alone (agreement is only
       between join inputs, and with one survivor there is nothing to agree
       with). On the runtime path that takes the returning arm, the rest of
       the line NEVER RUNS — which is exactly why the arm must already be at
       0: there is no later release to balance it. *)
  | CBranchAbort : bool -> list Op -> list Op -> CertItem
    (* a one-shot branch whose flagged arm (true = then, false = else) ends
       the PROCESS (format v6: `proc_exit` then `unreachable` — the emitter's
       exit validator, E083, pins that nothing runs after it). The arm must
       run fault-free from the entry count — no release at 0, no reuse of a
       shared block — but it takes NO exit obligation: the process and its
       heap are gone, so every credit still outstanding on that path is
       DISCHARGED, not leaked. The line continues from the SURVIVING arm's
       result alone (CBranchRet's shape), so every non-aborting path still
       balances exactly. *)
  | CNest : NOps -> NOps -> CertItem
    (* a one-shot branch whose arms may contain branches (format v10, the
       NOps sequence above): accepted iff `nexec` runs both arms from the
       entry count to the SAME result; the line continues at that count —
       CBranch's rule, applied at every depth. *)
  | CPoison : CertItem.
    (* an always-rejecting parse residue (format v5): a malformed exit marker
       (`x` not arm-terminal, both arms exiting, or `x` outside a `{…|…}`
       branch) parses to this instead of guessing — never a silent accept. *)

(* exec_line folds a cert LINE. A CLoop body is checked via the existing flat `exec`
   (its body is plain ops); accepted iff the body preserves rc (sufficient for any
   iteration count). Structural recursion on the item list — no nesting (bodies are
   plain `list Op`), so this compiles without the nested-inductive guard issue. *)
Fixpoint exec_line (cs : list CertItem) (rc : Z) : option Z :=
  match cs with
  | [] => Some rc
  | COp o :: rest =>
      match exec [o] rc with
      | Some rc' => exec_line rest rc'
      | None => None
      end
  | CLoop body :: rest =>
      match exec body rc with
      | Some rc' => if Z.eqb rc' rc then exec_line rest rc else None
      | None => None
      end
  | CCondLoop thenb elseb :: rest =>
      (* accept iff BOTH branches preserve rc (and neither faults) from the entry
         count — then any per-iteration choice preserves it (cf. OwnershipFilter
         cexec). *)
      match exec thenb rc, exec elseb rc with
      | Some rt, Some re =>
          if andb (Z.eqb rt rc) (Z.eqb re rc) then exec_line rest rc else None
      | _, _ => None
      end
  | CBranch thenb elseb :: rest =>
      (* one-shot branch: both arms run fault-free from the entry count to the
         SAME result (AGREEMENT, not preservation — the net may be nonzero);
         continue at that agreed count. *)
      match exec thenb rc, exec elseb rc with
      | Some rt, Some re => if Z.eqb rt re then exec_line rest rt else None
      | _, _ => None
      end
  | CBranchRet retthen thenb elseb :: rest =>
      (* law 6: the returning arm discharges the whole exit obligation INSIDE
         the arm (fault-free from the entry count to exactly 0); the line
         continues from the surviving arm's result alone — no agreement. *)
      match exec thenb rc, exec elseb rc with
      | Some rt, Some re =>
          if retthen
          then if Z.eqb rt 0 then exec_line rest re else None
          else if Z.eqb re 0 then exec_line rest rt else None
      | _, _ => None
      end
  | CBranchAbort abthen thenb elseb :: rest =>
      (* format v6: the aborting arm need only be fault-free from the entry
         count (the process ends there); the line continues from the
         surviving arm's result alone. *)
      match exec thenb rc, exec elseb rc with
      | Some rt, Some re => if abthen then exec_line rest re else exec_line rest rt
      | _, _ => None
      end
  | CNest thenb elseb :: rest =>
      match nexec thenb rc, nexec elseb rc with
      | Some rt, Some re => if Z.eqb rt re then exec_line rest rt else None
      | _, _ => None
      end
  | CPoison :: _ => None
  end.

Definition check_line (cs : list CertItem) : bool :=
  match exec_line cs 0 with Some z => Z.eqb z 0 | None => false end.

(* exec distributes over append (over the full alphabet). *)
Lemma exec_app :
  forall a b rc,
    exec (a ++ b) rc =
      match exec a rc with Some rc' => exec b rc' | None => None end.
Proof.
  induction a as [| o a IH]; intros b rc; simpl.
  - reflexivity.
  - destruct o; simpl.
    + apply IH.
    + apply IH.
    + destruct (rc <=? 0). reflexivity. apply IH.
    + destruct (rc <=? 0). reflexivity. apply IH.
    + reflexivity.
    + destruct (rc <=? 0). reflexivity. apply IH.
    + apply IH.
    + apply IH.
    + destruct (rc <=? 0). reflexivity. apply IH.
    + destruct (rc <=? 0). reflexivity. apply IH.
Qed.

(* exec over a cons = single-op step then the rest. *)
Lemma exec_cons :
  forall o b rc,
    exec (o :: b) rc =
      match exec [o] rc with Some rc' => exec b rc' | None => None end.
Proof. intros o b rc. change (o :: b) with ([o] ++ b). apply exec_app. Qed.

(* SOUNDNESS CORE of a nested arm: an accepted nested arm executes EVERY
   unrolling (each branch at every depth resolved to one arm) to its own
   result. *)
Lemma nexec_unroll :
  forall ns f, NUnrolls ns f -> forall rc r, nexec ns rc = Some r -> exec f rc = Some r.
Proof.
  intros ns f HU. induction HU as [| o a b HU IH | t e a ft b choice HT IHT HU IH];
    intros rc r Hx.
  - exact Hx.
  - cbn [nexec] in Hx. rewrite exec_cons.
    destruct (exec [o] rc) as [r1 |]; [| discriminate]. exact (IH r1 r Hx).
  - cbn [nexec] in Hx.
    destruct (nexec t rc) as [rt |] eqn:Et; [| discriminate].
    destruct (nexec e rc) as [re |] eqn:Ee; [| discriminate].
    destruct (Z.eqb rt re) eqn:Eq; [| discriminate].
    apply Z.eqb_eq in Eq. subst re.
    rewrite exec_app. destruct choice.
    + rewrite (IHT rc rt Et). exact (IH rt r Hx).
    + rewrite (IHT rc rt Ee). exact (IH rt r Hx).
Qed.

(* A flat body that PRESERVES rc, repeated n times, still preserves rc. *)
Lemma exec_repeat_preserve :
  forall body rc, exec body rc = Some rc ->
    forall n, exec (List.concat (List.repeat body n)) rc = Some rc.
Proof.
  intros body rc Hpres. induction n as [| n IH]; simpl.
  - reflexivity.
  - rewrite exec_app, Hpres. exact IH.
Qed.

(* CONCRETE unrolling of a CONDITIONAL loop: a list of bools `bs` (the runtime
   predicate outcomes) selects `thenb` or `elseb` each iteration. The real loop
   runs the SAME two branch bodies, choosing per the data — so this is exactly its
   concrete ownership trace. (Ported from OwnershipFilter.cond_concat over `list Op`.) *)
Fixpoint cond_concat (thenb elseb : list Op) (bs : list bool) : list Op :=
  match bs with
  | [] => []
  | true :: rest => thenb ++ cond_concat thenb elseb rest
  | false :: rest => elseb ++ cond_concat thenb elseb rest
  end.

(* Two rc-preserving branches ⇒ ANY choice sequence preserves rc and never faults.
   (Ported from OwnershipFilter.cond_concat_preserve; exec_flat → exec, exec_flat_app
   → exec_app.) *)
Lemma cond_concat_preserve :
  forall thenb elseb rc,
    exec thenb rc = Some rc ->
    exec elseb rc = Some rc ->
    forall bs, exec (cond_concat thenb elseb bs) rc = Some rc.
Proof.
  intros thenb elseb rc Ht He.
  induction bs as [| b bs IH]; simpl.
  - reflexivity.
  - destruct b.
    + rewrite exec_app, Ht. exact IH.
    + rewrite exec_app, He. exact IH.
Qed.

(* CONCRETE unrolling: a cert line unrolls to a flat run (each CLoop body → n copies). *)
Inductive UnrollsL : list CertItem -> list Op -> Prop :=
  | UL_nil : UnrollsL [] []
  | UL_op : forall o a b, UnrollsL a b -> UnrollsL (COp o :: a) (o :: b)
  | UL_loop : forall body a b n,
      UnrollsL a b -> UnrollsL (CLoop body :: a) (List.concat (List.repeat body n) ++ b)
  | UL_cond : forall thenb elseb a b bs,
      UnrollsL a b ->
      UnrollsL (CCondLoop thenb elseb :: a) (cond_concat thenb elseb bs ++ b)
  (* a one-shot branch unrolls to EXACTLY ONE arm — the runtime choice. *)
  | UL_branch : forall thenb elseb a b (choice : bool),
      UnrollsL a b ->
      UnrollsL (CBranch thenb elseb :: a) ((if choice then thenb else elseb) ++ b)
  (* a returning branch, EXIT path: the runtime takes the returning arm and the
     function exits — the REST OF THE LINE NEVER RUNS, so the unrolled run is
     the arm alone (no continuation). No UnrollsL constructor exists for
     CPoison: a poisoned line is never accepted, so it has nothing to unroll. *)
  | UL_branchret_exit : forall retthen thenb elseb a,
      UnrollsL (CBranchRet retthen thenb elseb :: a)
               (if retthen then thenb else elseb)
  (* a returning branch, SURVIVING path: the runtime takes the other arm and
     the line continues as usual. *)
  | UL_branchret_cont : forall retthen thenb elseb a b,
      UnrollsL a b ->
      UnrollsL (CBranchRet retthen thenb elseb :: a)
               ((if retthen then elseb else thenb) ++ b)
  (* an aborting branch, SURVIVING path (format v6): the runtime takes the
     other arm and the line continues. The ABORTING path is not a run of
     this predicate — it never returns; `AbortsL` below is its own. *)
  | UL_branchabort_cont : forall abthen thenb elseb a b,
      UnrollsL a b ->
      UnrollsL (CBranchAbort abthen thenb elseb :: a)
               ((if abthen then elseb else thenb) ++ b)
  (* a nested branch (format v10): the run takes ONE arm, and inside it one
     arm of every branch it reaches. *)
  | UL_nest : forall thenb elseb a ft b (choice : bool)
      (Hnest : NUnrolls (if choice then thenb else elseb) ft), UnrollsL a b ->
      UnrollsL (CNest thenb elseb :: a) (ft ++ b).

(* SOUNDNESS CORE: an accepting line, at any rc, executes EVERY unrolling either
   to the line's own result — or, on a path that EXITS through a returning
   branch arm (law 6, format v5), to EXACTLY 0: the arm's whole exit obligation
   was discharged inside it, and the rest of the line never ran. Both disjuncts
   collapse to `Some 0` at the accepted top level (check_line demands r = 0),
   which is all the headline theorem needs. *)
Lemma exec_line_unroll :
  forall cs fops, UnrollsL cs fops ->
    forall rc r, exec_line cs rc = Some r ->
      exec fops rc = Some r \/ exec fops rc = Some 0.
Proof.
  intros cs fops HU. induction HU; intros rc r Hexec.
  - (* nil *) left. simpl in *. exact Hexec.
  - (* COp o — case on the concrete op so exec/exec_line both reduce *)
    destruct o; simpl in *.
    + apply IHHU; exact Hexec.
    + apply IHHU; exact Hexec.
    + destruct (rc <=? 0); [discriminate | apply IHHU; exact Hexec].
    + destruct (rc <=? 0); [discriminate | apply IHHU; exact Hexec].
    + discriminate.
    + destruct (rc <=? 0); [discriminate | apply IHHU; exact Hexec].
    + apply IHHU; exact Hexec.
    + apply IHHU; exact Hexec.
    + destruct (rc <=? 0); [discriminate | apply IHHU; exact Hexec].
    + destruct (rc <=? 0); [discriminate | apply IHHU; exact Hexec].
  - (* CLoop body *) simpl in *.
    destruct (exec body rc) as [rc' |] eqn:Eb; [| discriminate].
    destruct (Z.eqb rc' rc) eqn:Eq; [| discriminate].
    apply Z.eqb_eq in Eq. subst rc'.
    rewrite exec_app, (exec_repeat_preserve body rc Eb n).
    apply IHHU. exact Hexec.
  - (* CCondLoop thenb elseb — copy of OwnershipFilter cexec_unroll's loop case *)
    simpl in *.
    destruct (exec thenb rc) as [rt |] eqn:Et; [| discriminate].
    destruct (exec elseb rc) as [re |] eqn:Ee; [| discriminate].
    destruct (andb (Z.eqb rt rc) (Z.eqb re rc)) eqn:Eb; [| discriminate].
    apply andb_prop in Eb. destruct Eb as [Hrt Hre].
    apply Z.eqb_eq in Hrt. apply Z.eqb_eq in Hre. subst rt re.
    rewrite exec_app, (cond_concat_preserve thenb elseb rc Et Ee bs).
    apply IHHU. exact Hexec.
  - (* CBranch thenb elseb — the run takes ONE arm; both agree on the result,
       so either way the line continues at the same count. *)
    simpl in *.
    destruct (exec thenb rc) as [rt |] eqn:Et; [| discriminate].
    destruct (exec elseb rc) as [re |] eqn:Ee; [| discriminate].
    destruct (Z.eqb rt re) eqn:Eq; [| discriminate].
    apply Z.eqb_eq in Eq. subst re.
    rewrite exec_app. destruct choice.
    + rewrite Et. apply IHHU. exact Hexec.
    + rewrite Ee. apply IHHU. exact Hexec.
  - (* CBranchRet, EXIT path — the returning arm ran to exactly 0 (its guard)
       and the rest of the line never runs: the run ends at 0. *)
    simpl in *.
    destruct (exec thenb rc) as [rt |] eqn:Et; [| discriminate].
    destruct (exec elseb rc) as [re |] eqn:Ee; [| discriminate].
    destruct retthen.
    + destruct (Z.eqb rt 0) eqn:Ez; [| discriminate].
      apply Z.eqb_eq in Ez. subst rt. right. exact Et.
    + destruct (Z.eqb re 0) eqn:Ez; [| discriminate].
      apply Z.eqb_eq in Ez. subst re. right. exact Ee.
  - (* CBranchRet, SURVIVING path — the line continues from the surviving
       arm's result alone. *)
    simpl in *.
    destruct (exec thenb rc) as [rt |] eqn:Et; [| discriminate].
    destruct (exec elseb rc) as [re |] eqn:Ee; [| discriminate].
    rewrite exec_app. destruct retthen.
    + destruct (Z.eqb rt 0); [| discriminate].
      rewrite Ee. apply IHHU. exact Hexec.
    + destruct (Z.eqb re 0); [| discriminate].
      rewrite Et. apply IHHU. exact Hexec.
  - (* CBranchAbort, SURVIVING path — the line continues from the surviving
       arm's result alone. *)
    simpl in *.
    destruct (exec thenb rc) as [rt |] eqn:Et; [| discriminate].
    destruct (exec elseb rc) as [re |] eqn:Ee; [| discriminate].
    rewrite exec_app. destruct abthen.
    + rewrite Ee. apply IHHU. exact Hexec.
    + rewrite Et. apply IHHU. exact Hexec.
  - (* CNest — the run takes one arm; nexec_unroll runs it to the agreed
       count, and the line continues there. *)
    simpl in *.
    destruct (nexec thenb rc) as [rt |] eqn:Et; [| discriminate].
    destruct (nexec elseb rc) as [re |] eqn:Ee; [| discriminate].
    destruct (Z.eqb rt re) eqn:Eq; [| discriminate].
    apply Z.eqb_eq in Eq. subst re.
    rewrite exec_app. destruct choice.
    + rewrite (nexec_unroll _ _ Hnest rc rt Et). apply IHHU. exact Hexec.
    + rewrite (nexec_unroll _ _ Hnest rc rt Ee). apply IHHU. exact Hexec.
Qed.

(* The headline: an ACCEPTED loop cert line guarantees EVERY concrete unrolling is
   free of double-free / use-after-free AND leak — the completeness the flat cert
   lacked, now SOUND (the false-rejection closed at the root). A run exiting
   through a returning branch arm lands on the second disjunct; at the accepted
   top level both disjuncts are `Some 0`. *)
Theorem check_line_unroll_sound :
  forall cs, check_line cs = true ->
    forall fops, UnrollsL cs fops ->
      run fops <> None /\ run fops = Some 0.
Proof.
  intros cs H fops HU. unfold check_line in H. unfold run.
  destruct (exec_line cs 0) as [z |] eqn:E; [| discriminate].
  apply Z.eqb_eq in H. subst z.
  destruct (exec_line_unroll cs fops HU 0 0 E) as [H0 | H0]; rewrite H0;
    split; (discriminate || reflexivity).
Qed.

(* ─── ABORTING runs (format v6) ───
   A run that ends in a process ABORT: the line's items up to an aborting
   branch, each resolved as on a surviving path, then the aborting arm — and
   nothing after it (the process is gone). Such a run owes no balance: the
   heap it would leak no longer exists. What it must still be is SAFE — no
   release at 0, no reuse of a shared block — on every op it executes. *)
Inductive AbortsL : list CertItem -> list Op -> Prop :=
  | AL_op : forall o a b, AbortsL a b -> AbortsL (COp o :: a) (o :: b)
  | AL_loop : forall body a b n,
      AbortsL a b -> AbortsL (CLoop body :: a) (List.concat (List.repeat body n) ++ b)
  | AL_cond : forall thenb elseb a b bs,
      AbortsL a b ->
      AbortsL (CCondLoop thenb elseb :: a) (cond_concat thenb elseb bs ++ b)
  | AL_branch : forall thenb elseb a b (choice : bool),
      AbortsL a b ->
      AbortsL (CBranch thenb elseb :: a) ((if choice then thenb else elseb) ++ b)
  | AL_branchret_cont : forall retthen thenb elseb a b,
      AbortsL a b ->
      AbortsL (CBranchRet retthen thenb elseb :: a)
              ((if retthen then elseb else thenb) ++ b)
  | AL_branchabort_cont : forall abthen thenb elseb a b,
      AbortsL a b ->
      AbortsL (CBranchAbort abthen thenb elseb :: a)
              ((if abthen then elseb else thenb) ++ b)
  | AL_nest : forall thenb elseb a ft b (choice : bool)
      (Hnest : NUnrolls (if choice then thenb else elseb) ft), AbortsL a b ->
      AbortsL (CNest thenb elseb :: a) (ft ++ b)
  (* the abort itself: the aborting arm runs, and the run ENDS. *)
  | AL_abort : forall abthen thenb elseb a,
      AbortsL (CBranchAbort abthen thenb elseb :: a)
              (if abthen then thenb else elseb).

(* SAFETY CORE for aborting runs: an accepting line, at any rc, executes
   every aborting run without a fault. *)
Lemma exec_line_abort :
  forall cs fops, AbortsL cs fops ->
    forall rc r, exec_line cs rc = Some r -> exec fops rc <> None.
Proof.
  intros cs fops HA. induction HA; intros rc r Hexec.
  - (* COp o *)
    destruct o; simpl in *.
    + eapply IHHA; exact Hexec.
    + eapply IHHA; exact Hexec.
    + destruct (rc <=? 0); [discriminate | eapply IHHA; exact Hexec].
    + destruct (rc <=? 0); [discriminate | eapply IHHA; exact Hexec].
    + discriminate.
    + destruct (rc <=? 0); [discriminate | eapply IHHA; exact Hexec].
    + eapply IHHA; exact Hexec.
    + eapply IHHA; exact Hexec.
    + destruct (rc <=? 0); [discriminate | eapply IHHA; exact Hexec].
    + destruct (rc <=? 0); [discriminate | eapply IHHA; exact Hexec].
  - (* CLoop body *) simpl in *.
    destruct (exec body rc) as [rc' |] eqn:Eb; [| discriminate].
    destruct (Z.eqb rc' rc) eqn:Eq; [| discriminate].
    apply Z.eqb_eq in Eq. subst rc'.
    rewrite exec_app, (exec_repeat_preserve body rc Eb n).
    eapply IHHA. exact Hexec.
  - (* CCondLoop *) simpl in *.
    destruct (exec thenb rc) as [rt |] eqn:Et; [| discriminate].
    destruct (exec elseb rc) as [re |] eqn:Ee; [| discriminate].
    destruct (andb (Z.eqb rt rc) (Z.eqb re rc)) eqn:Eb; [| discriminate].
    apply andb_prop in Eb. destruct Eb as [Hrt Hre].
    apply Z.eqb_eq in Hrt. apply Z.eqb_eq in Hre. subst rt re.
    rewrite exec_app, (cond_concat_preserve thenb elseb rc Et Ee bs).
    eapply IHHA. exact Hexec.
  - (* CBranch *) simpl in *.
    destruct (exec thenb rc) as [rt |] eqn:Et; [| discriminate].
    destruct (exec elseb rc) as [re |] eqn:Ee; [| discriminate].
    destruct (Z.eqb rt re) eqn:Eq; [| discriminate].
    apply Z.eqb_eq in Eq. subst re.
    rewrite exec_app. destruct choice.
    + rewrite Et. eapply IHHA. exact Hexec.
    + rewrite Ee. eapply IHHA. exact Hexec.
  - (* CBranchRet, surviving path *) simpl in *.
    destruct (exec thenb rc) as [rt |] eqn:Et; [| discriminate].
    destruct (exec elseb rc) as [re |] eqn:Ee; [| discriminate].
    rewrite exec_app. destruct retthen.
    + destruct (Z.eqb rt 0); [| discriminate].
      rewrite Ee. eapply IHHA. exact Hexec.
    + destruct (Z.eqb re 0); [| discriminate].
      rewrite Et. eapply IHHA. exact Hexec.
  - (* CBranchAbort, surviving path *) simpl in *.
    destruct (exec thenb rc) as [rt |] eqn:Et; [| discriminate].
    destruct (exec elseb rc) as [re |] eqn:Ee; [| discriminate].
    rewrite exec_app. destruct abthen.
    + rewrite Ee. eapply IHHA. exact Hexec.
    + rewrite Et. eapply IHHA. exact Hexec.
  - (* CNest *) simpl in *.
    destruct (nexec thenb rc) as [rt |] eqn:Et; [| discriminate].
    destruct (nexec elseb rc) as [re |] eqn:Ee; [| discriminate].
    destruct (Z.eqb rt re) eqn:Eq; [| discriminate].
    apply Z.eqb_eq in Eq. subst re.
    rewrite exec_app. destruct choice.
    + rewrite (nexec_unroll _ _ Hnest rc rt Et). eapply IHHA. exact Hexec.
    + rewrite (nexec_unroll _ _ Hnest rc rt Ee). eapply IHHA. exact Hexec.
  - (* the abort: the aborting arm ran fault-free (exec_line's guard). *)
    simpl in *.
    destruct (exec thenb rc) as [rt |] eqn:Et; [| discriminate].
    destruct (exec elseb rc) as [re |] eqn:Ee; [| discriminate].
    destruct abthen; [rewrite Et | rewrite Ee]; discriminate.
Qed.

(* The headline for aborting runs: an accepted line guarantees every run that
   ends in an abort is free of double-free / use-after-free up to the abort.
   (Every run that does NOT abort is `check_line_unroll_sound`'s, with its
   exact balance: the abort terminal weakens nothing on those.) *)
Theorem check_line_abort_sound :
  forall cs, check_line cs = true ->
    forall fops, AbortsL cs fops -> run fops <> None.
Proof.
  intros cs H fops HA. unfold check_line in H. unfold run.
  destruct (exec_line cs 0) as [z |] eqn:E; [| discriminate].
  exact (exec_line_abort cs fops HA 0 z E).
Qed.

(* PREFIX SAFETY: a run that aborts after a PREFIX of a run the line accepts
   is safe up to the abort — `exec` is a left fold, so a fault in the prefix
   would already be a fault in the whole. The recorder leans on this to show
   an aborting path that some returning path extends WITHOUT an abort arm of
   its own (crates/almide-wasm/src/witness_paths.rs `line`). *)
Lemma exec_prefix_safe :
  forall a b rc, exec (a ++ b) rc <> None -> exec a rc <> None.
Proof.
  intros a b rc H. rewrite exec_app in H.
  destruct (exec a rc); [discriminate | exact H].
Qed.

Theorem check_line_prefix_safe :
  forall cs, check_line cs = true ->
    forall fa fb, UnrollsL cs (fa ++ fb) -> run fa <> None.
Proof.
  intros cs H fa fb HU. unfold run.
  apply (exec_prefix_safe fa fb 0).
  destruct (check_line_unroll_sound cs H (fa ++ fb) HU) as [Hs _]. exact Hs.
Qed.

(* non-vacuity: the accumulator slot accepts; a leaky/draining loop body is rejected. *)
Example acc_slot_line_accepts : check_line [COp Inc; CLoop [Dec; Inc]; COp MoveOut] = true.
Proof. reflexivity. Qed.
Example leaky_loop_line_rejects : check_line [COp Inc; CLoop [Inc]; COp MoveOut] = false.
Proof. reflexivity. Qed.
Example draining_loop_line_rejects : check_line [COp Inc; CLoop [Dec]; COp MoveOut] = false.
Proof. reflexivity. Qed.

(* CONDITIONAL-loop (filter slot) non-vacuity: acquire once; each iteration EITHER
   drop-old+alloc-new (predicate true, net 0) OR nothing (predicate false, net 0);
   move out the final. ACCEPTS — the runtime-variable #appends is irrelevant. *)
Example filter_slot_line_accepts :
  check_line [COp Inc; CCondLoop [Dec; Inc] []; COp MoveOut] = true.
Proof. reflexivity. Qed.
(* THEN branch leaks (net +1) — REJECT. *)
Example filter_leaky_then_line_rejects :
  check_line [COp Inc; CCondLoop [Inc] []; COp MoveOut] = false.
Proof. reflexivity. Qed.
(* ELSE branch drains (net −1) — REJECT. *)
Example filter_draining_else_line_rejects :
  check_line [COp Inc; CCondLoop [Dec; Inc] [Dec]; COp MoveOut] = false.
Proof. reflexivity. Qed.

(* ONE-SHOT branch (brick 5a) non-vacuity. AGREEMENT at net +1: both arms
   acquire (a heap-result branch — the merge value), released twice after.
   entry 0 → `i` → 1; arms 1→2 AGREE; `dd` → 0. ACCEPTS. *)
Example branch_agree_net_plus_one_accepts :
  check_line [COp Inc; CBranch [Inc] [Inc]; COp Dec; COp Dec] = true.
Proof. reflexivity. Qed.
(* Arms DISAGREE (+1 vs 0): whichever way the branch goes, the line's later
   accounting is wrong for the other — REJECT. *)
Example branch_disagree_rejects :
  check_line [COp Inc; CBranch [Inc] []; COp Dec] = false.
Proof. reflexivity. Qed.
(* CROSS-ARM COMPENSATION (the closed hole): `i` in one arm, `d` in the other.
   FLAT the two would balance (the accept-but-unsafe class the per-arm-balance
   convention had to promise away); grouped, the `d` arm faults from entry 0 —
   REJECT, structurally. *)
Example branch_cross_arm_compensation_rejects :
  check_line [CBranch [Inc] [Dec]] = false.
Proof. reflexivity. Qed.
(* Both arms fault — REJECT. *)
Example branch_both_arms_fault_rejects :
  check_line [CBranch [Dec] [Dec]] = false.
Proof. reflexivity. Qed.

(* ─── LOOP-AWARE certificate parsing (format v2, backward-compatible) ───
   Extends the byte format with loop delimiters `(` … `)` around a flat loop body:
   e.g. `i(di)m` = COp Inc, CLoop [Dec; Inc], COp MoveOut (the accumulator slot).
   A cert with NO parens parses to all-`COp` lines, and `check_line` on those folds
   exactly like the flat `check` — so every existing v1 certificate is unchanged.
   The op alphabet inside/outside a loop is the same i/a/d/m/r. *)
Definition lparen : ascii := "("%char.
Definition rparen : ascii := ")"%char.

(* Flush the in-progress line to a finished `list CertItem` (closing a dangling
   loop defensively, though well-formed certs close every `(` before newline/EOF). *)
Definition finish_line (cur : list CertItem) (lp : option (list Op)) : list CertItem :=
  match lp with
  | Some body => rev (CLoop (rev body) :: cur)
  | None => rev cur
  end.

Fixpoint parse_lc (s : string) (cur : list CertItem) (lp : option (list Op))
                  {struct s} : list (list CertItem) :=
  match s with
  | EmptyString => [finish_line cur lp]
  | String b rest =>
      if Ascii.eqb b newline then finish_line cur lp :: parse_lc rest [] None
      else if Ascii.eqb b lparen then parse_lc rest cur (Some [])
      else if Ascii.eqb b rparen then
        match lp with
        | Some body => parse_lc rest (CLoop (rev body) :: cur) None
        | None => parse_lc rest cur None
        end
      else match parse_byte b with
           | Some op =>
               match lp with
               | Some body => parse_lc rest cur (Some (op :: body))
               | None => parse_lc rest (COp op :: cur) None
               end
           | None => parse_lc rest cur lp
           end
  end.

(* The full loop-aware checker over raw certificate bytes. *)
Definition check_cert_lc (s : string) : bool :=
  forallb check_line (parse_lc s [] None).

(* SOUNDNESS over bytes: an accepted loop certificate has, for EVERY parsed line and
   EVERY concrete unrolling of that line, no double-free / use-after-free and no
   leak. (Per-line via check_line_unroll_sound; lifted over all lines here.) *)
Theorem check_cert_lc_sound :
  forall s, check_cert_lc s = true ->
    forall cs, In cs (parse_lc s [] None) ->
      forall fops, UnrollsL cs fops -> run fops <> None /\ run fops = Some 0.
Proof.
  intros s H cs Hin fops HU.
  unfold check_cert_lc in H. rewrite forallb_forall in H.
  apply (check_line_unroll_sound cs (H cs Hin) fops HU).
Qed.

(* backward-compat + non-vacuity on real bytes *)
Example cert_lc_flat_accepts : check_cert_lc "iidd"%string = true.
Proof. reflexivity. Qed.
Example cert_lc_flat_rejects : check_cert_lc "idd"%string = false.
Proof. reflexivity. Qed.
Example cert_lc_acc_slot_accepts : check_cert_lc "i(di)m"%string = true.   (* accumulator slot *)
Proof. reflexivity. Qed.
Example cert_lc_leaky_loop_rejects : check_cert_lc "i(i)m"%string = false. (* loop body leaks *)
Proof. reflexivity. Qed.
Example cert_lc_draining_loop_rejects : check_cert_lc "i(d)m"%string = false. (* loop body drains *)
Proof. reflexivity. Qed.

(* ─── CONDITIONAL-LOOP-aware certificate parsing (format v3, backward-compatible) ───
   Extends format v2 with conditional-loop delimiters `[` then `|` else `]` around
   two flat branch bodies: e.g. `i[di|]m` = COp Inc, CCondLoop [Dec; Inc] [], COp
   MoveOut (the filter accumulator slot). `parse_clc` is a SUPERSET of `parse_lc`:
   it ALSO handles the `(` … `)` loop form, and a cert with NO `[` parses byte-for-
   byte as `parse_lc` (the `|` / `]` bytes are not in the op alphabet, so outside a
   `[` they are skipped exactly as `parse_lc` skips them) — full backward compat for
   every flat and CLoop certificate. The op alphabet inside a branch is the same
   i/a/d/m/r. Conditional-loop bodies are FLAT (no nested `(`/`[`). *)
Definition lbracket : ascii := "["%char.
Definition rbracket : ascii := "]"%char.
Definition bar : ascii := "|"%char.

(* in-progress conditional-loop state: (collecting-else?, then-acc-rev, else-acc-rev). *)
Definition condst : Type := (bool * list Op * list Op)%type.

(* Flush the in-progress line, defensively closing a dangling `(`-loop or `[`-cond. *)
Definition finish_line_c (cur : list CertItem) (lp : option (list Op))
                         (cp : option condst) : list CertItem :=
  match cp with
  | Some (_, th, el) => rev (CCondLoop (rev th) (rev el) :: cur)
  | None =>
      match lp with
      | Some body => rev (CLoop (rev body) :: cur)
      | None => rev cur
      end
  end.

Fixpoint parse_clc (s : string) (cur : list CertItem)
                   (lp : option (list Op)) (cp : option condst)
                   {struct s} : list (list CertItem) :=
  match s with
  | EmptyString => [finish_line_c cur lp cp]
  | String b rest =>
      if Ascii.eqb b newline then finish_line_c cur lp cp :: parse_clc rest [] None None
      else match cp with
      | Some (in_else, th, el) =>
          (* inside `[ … | … ]` — collect ops into the then/else branch *)
          if Ascii.eqb b bar then parse_clc rest cur lp (Some (true, th, el))
          else if Ascii.eqb b rbracket then
            parse_clc rest (CCondLoop (rev th) (rev el) :: cur) lp None
          else match parse_byte b with
               | Some op =>
                   if in_else then parse_clc rest cur lp (Some (true, th, op :: el))
                   else parse_clc rest cur lp (Some (false, op :: th, el))
               | None => parse_clc rest cur lp cp
               end
      | None =>
          if Ascii.eqb b lbracket then parse_clc rest cur lp (Some (false, [], []))
          else if Ascii.eqb b lparen then parse_clc rest cur (Some []) None
          else if Ascii.eqb b rparen then
            match lp with
            | Some body => parse_clc rest (CLoop (rev body) :: cur) None None
            | None => parse_clc rest cur None None
            end
          else match parse_byte b with
               | Some op =>
                   match lp with
                   | Some body => parse_clc rest cur (Some (op :: body)) None
                   | None => parse_clc rest (COp op :: cur) None None
                   end
               | None => parse_clc rest cur lp None
               end
      end
  end.

(* The full conditional-loop-aware checker over raw certificate bytes. *)
Definition check_clc (s : string) : bool :=
  forallb check_line (parse_clc s [] None None).

(* SOUNDNESS over bytes (1-line corollary of check_line_unroll_sound, now covering
   CCondLoop automatically via UL_cond + the exec_line_unroll CCondLoop case): an
   accepted conditional-loop certificate has, for EVERY parsed line and EVERY
   concrete unrolling, no double-free / use-after-free and no leak. *)
Theorem check_clc_unroll_sound :
  forall s, check_clc s = true ->
    forall cs, In cs (parse_clc s [] None None) ->
      forall fops, UnrollsL cs fops -> run fops <> None /\ run fops = Some 0.
Proof.
  intros s H cs Hin fops HU.
  unfold check_clc in H. rewrite forallb_forall in H.
  apply (check_line_unroll_sound cs (H cs Hin) fops HU).
Qed.

(* backward-compat (flat + `(`-loop certs parse/verify exactly as before) + the new
   conditional-loop (filter) certs, on real bytes *)
Example cert_clc_flat_accepts : check_clc "iidd"%string = true.
Proof. reflexivity. Qed.
Example cert_clc_flat_rejects : check_clc "idd"%string = false.
Proof. reflexivity. Qed.
Example cert_clc_loop_accepts : check_clc "i(di)m"%string = true.        (* v2 CLoop still accepts *)
Proof. reflexivity. Qed.
Example cert_clc_filter_slot_accepts : check_clc "i[di|]m"%string = true.   (* filter accumulator slot *)
Proof. reflexivity. Qed.
Example cert_clc_leaky_then_rejects : check_clc "i[i|]m"%string = false.    (* then branch leaks *)
Proof. reflexivity. Qed.
Example cert_clc_draining_else_rejects : check_clc "i[di|d]m"%string = false. (* else branch drains *)
Proof. reflexivity. Qed.

(* ─── BRANCH-aware certificate parsing (format v4, backward-compatible) ───
   Extends format v3 with ONE-SHOT branch delimiters `{` then `|` else `}` around
   two flat arm bodies: e.g. `i{i|i}dd` = COp Inc, CBranch [Inc] [Inc], COp Dec,
   COp Dec (a heap-result branch — both arms acquire, arms AGREE at net +1).
   `parse_bc` is a SUPERSET of `parse_clc`: it also handles `(`…`)` loops and
   `[`…`|`…`]` conditional loops, and a cert with NO `{` parses byte-for-byte as
   `parse_clc` (`{`/`}` are outside the op alphabet — previously skipped bytes) —
   full backward compat for every flat, CLoop and CCondLoop certificate. Arm
   bodies are FLAT (no nesting), like loop bodies. *)
Definition lbrace : ascii := "{"%char.
Definition rbrace : ascii := "}"%char.

(* Flush the in-progress line, defensively closing a dangling `{`-branch first
   (well-formed certs close every `{` before newline/EOF), else deferring to the
   format-v3 flush. *)
Definition finish_line_b (cur : list CertItem) (lp : option (list Op))
                         (cp bp : option condst) : list CertItem :=
  match bp with
  | Some (_, th, el) => rev (CBranch (rev th) (rev el) :: cur)
  | None => finish_line_c cur lp cp
  end.

Fixpoint parse_bc (s : string) (cur : list CertItem)
                  (lp : option (list Op)) (cp bp : option condst)
                  {struct s} : list (list CertItem) :=
  match s with
  | EmptyString => [finish_line_b cur lp cp bp]
  | String b rest =>
      if Ascii.eqb b newline then finish_line_b cur lp cp bp :: parse_bc rest [] None None None
      else match bp with
      | Some (in_else, th, el) =>
          (* inside `{ … | … }` — collect ops into the then/else arm *)
          if Ascii.eqb b bar then parse_bc rest cur lp cp (Some (true, th, el))
          else if Ascii.eqb b rbrace then
            parse_bc rest (CBranch (rev th) (rev el) :: cur) lp cp None
          else match parse_byte b with
               | Some op =>
                   if in_else then parse_bc rest cur lp cp (Some (true, th, op :: el))
                   else parse_bc rest cur lp cp (Some (false, op :: th, el))
               | None => parse_bc rest cur lp cp bp
               end
      | None =>
          match cp with
          | Some (in_else, th, el) =>
              (* inside `[ … | … ]` — exactly parse_clc's conditional-loop state *)
              if Ascii.eqb b bar then parse_bc rest cur lp (Some (true, th, el)) None
              else if Ascii.eqb b rbracket then
                parse_bc rest (CCondLoop (rev th) (rev el) :: cur) lp None None
              else match parse_byte b with
                   | Some op =>
                       if in_else then parse_bc rest cur lp (Some (true, th, op :: el)) None
                       else parse_bc rest cur lp (Some (false, op :: th, el)) None
                   | None => parse_bc rest cur lp cp None
                   end
          | None =>
              if Ascii.eqb b lbrace then parse_bc rest cur lp None (Some (false, [], []))
              else if Ascii.eqb b lbracket then parse_bc rest cur lp (Some (false, [], [])) None
              else if Ascii.eqb b lparen then parse_bc rest cur (Some []) None None
              else if Ascii.eqb b rparen then
                match lp with
                | Some body => parse_bc rest (CLoop (rev body) :: cur) None None None
                | None => parse_bc rest cur None None None
                end
              else match parse_byte b with
                   | Some op =>
                       match lp with
                       | Some body => parse_bc rest cur (Some (op :: body)) None None
                       | None => parse_bc rest (COp op :: cur) None None None
                       end
                   | None => parse_bc rest cur lp None None
                   end
          end
      end
  end.

(* The full branch-aware checker over raw certificate bytes (format v4). *)
Definition check_bc (s : string) : bool :=
  forallb check_line (parse_bc s [] None None None).

(* SOUNDNESS over bytes (1-line corollary of check_line_unroll_sound, covering
   CBranch via UL_branch + the exec_line_unroll CBranch case): an accepted
   branch certificate has, for EVERY parsed line and EVERY concrete unrolling —
   each branch resolved to the ONE arm the runtime takes — no double-free /
   use-after-free and no leak. *)
Theorem check_bc_unroll_sound :
  forall s, check_bc s = true ->
    forall cs, In cs (parse_bc s [] None None None) ->
      forall fops, UnrollsL cs fops -> run fops <> None /\ run fops = Some 0.
Proof.
  intros s H cs Hin fops HU.
  unfold check_bc in H. rewrite forallb_forall in H.
  apply (check_line_unroll_sound cs (H cs Hin) fops HU).
Qed.

(* backward-compat (flat + loop + cond-loop certs verify exactly as before) +
   the new one-shot branch certs, on real bytes *)
Example cert_bc_flat_accepts : check_bc "iidd"%string = true.
Proof. reflexivity. Qed.
Example cert_bc_flat_rejects : check_bc "idd"%string = false.
Proof. reflexivity. Qed.
Example cert_bc_loop_accepts : check_bc "i(di)m"%string = true.          (* v2 CLoop still accepts *)
Proof. reflexivity. Qed.
Example cert_bc_filter_accepts : check_bc "i[di|]m"%string = true.       (* v3 CCondLoop still accepts *)
Proof. reflexivity. Qed.
Example cert_bc_borrow_accepts : check_bc "ibd"%string = true.           (* 5b `b` rides format v4 *)
Proof. reflexivity. Qed.
Example cert_bc_branch_accepts : check_bc "i{i|i}dd"%string = true.      (* heap-result branch: arms agree at +1 *)
Proof. reflexivity. Qed.
Example cert_bc_branch_disagree_rejects : check_bc "i{i|}d"%string = false. (* arms disagree (+1 vs 0) *)
Proof. reflexivity. Qed.
Example cert_bc_cross_arm_rejects : check_bc "{i|d}"%string = false.     (* cross-arm compensation faults *)
Proof. reflexivity. Qed.

(* ─── EXIT-aware certificate parsing (format v5, backward-compatible) ───
   Extends format v4 with the frame-exit marker `x` as the LAST event of a
   `{ then | else }` arm: the marked arm ends in `Op::Return` (law 6), takes
   its whole exit obligation inside the arm (fault-free to exactly 0 — the
   pre-return drops are its `d`s, the returned value's move its `m`), and the
   line continues from the SURVIVING arm alone. `parse_xc` is a SUPERSET of
   `parse_bc`: a cert with no `x` parses byte-for-byte identically (`x` was
   previously a skipped byte), so every flat, CLoop, CCondLoop and CBranch
   certificate is unchanged. Malformed `x` — not arm-terminal, both arms
   marked, or outside a `{…}` — parses to the always-rejecting CPoison:
   never a silent accept. *)
Definition xmark_lc : ascii := "x"%char.
Definition xmark_uc : ascii := "X"%char.
Definition is_xmark (b : ascii) : bool :=
  orb (Ascii.eqb b xmark_lc) (Ascii.eqb b xmark_uc).

(* FORMAT v6: the arm-ABORT terminal `t` (CBranchAbort). Arm-terminal like
   `x`; at most one terminal arm per branch; outside a `{…}` it poisons. A
   byte v5 skipped, so every v5 certificate parses unchanged. *)
Definition tmark_lc : ascii := "t"%char.
Definition tmark_uc : ascii := "T"%char.
Definition is_tmark (b : ascii) : bool :=
  orb (Ascii.eqb b tmark_lc) (Ascii.eqb b tmark_uc).

(* in-progress branch state with the exit and abort markers:
   (in_else, then_exited, else_exited, then_aborted, else_aborted, malformed,
    then-acc-rev, else-acc-rev, open nested branches). Format v10: an arm
   collects NODES — an op, or a nested branch already closed — and a `{`
   inside an arm opens a nested branch; the stack holds each open one,
   innermost first, as (in_else, then-acc-rev, else-acc-rev). *)
Inductive NNode : Type :=
  | NNAt : Op -> NNode
  | NNBr : NOps -> NOps -> NNode.

Fixpoint nseq (l : list NNode) : NOps :=
  match l with
  | [] => NEnd
  | NNAt o :: r => NOp1 o (nseq r)
  | NNBr t e :: r => NBr t e (nseq r)
  end.

Definition nfrm : Type := (bool * list NNode * list NNode)%type.
Definition brxst : Type :=
  (bool * bool * bool * bool * bool * bool * list NNode * list NNode * list nfrm)%type.

(* Add a node to the innermost open arm: the top nested branch's current
   arm, else the branch's own. In the branch's own arm a node after that
   arm's terminal marker is malformed (`x` / `t` must be arm-terminal). *)
Definition brx_add (x : NNode) (st : brxst) : brxst :=
  match st with
  | (ie, tx, ex, ta, ea, bad, th, el, (fe, ft, fl) :: stk) =>
      (ie, tx, ex, ta, ea, bad, th, el,
       (if fe then (fe, ft, x :: fl) else (fe, x :: ft, fl)) :: stk)
  | (ie, tx, ex, ta, ea, bad, th, el, []) =>
      if ie then (ie, tx, ex, ta, ea, orb bad (orb ex ea), th, x :: el, [])
      else (ie, tx, ex, ta, ea, orb bad (orb tx ta), x :: th, el, [])
  end.

(* An arm that is all ops, as those ops (`None` once it holds a branch). *)
Fixpoint nflat (l : list NNode) : option (list Op) :=
  match l with
  | [] => Some []
  | NNAt o :: rest => match nflat rest with Some r => Some (o :: r) | None => None end
  | NNBr _ _ :: _ => None
  end.

(* Close a `{…|…}` region: malformed or both arms terminal → poison; one
   exit-marked arm → CBranchRet; one abort-marked arm → CBranchAbort (flag =
   which side); no marker → the ordinary CBranch. An arm holding a nested
   branch (format v10) makes the region a CNest — with no marker: an exit or
   abort arm is flat, so a marker there poisons. *)
Definition close_brx (st : brxst) : CertItem :=
  match st with
  | (_, tx, ex, ta, ea, bad, nth, nel, _) =>
      match nflat (rev nth), nflat (rev nel) with
      | Some th, Some el =>
          if orb bad (andb (orb tx ta) (orb ex ea)) then CPoison
          else if tx then CBranchRet true th el
          else if ex then CBranchRet false th el
          else if ta then CBranchAbort true th el
          else if ea then CBranchAbort false th el
          else CBranch th el
      | _, _ =>
          if orb bad (orb (orb tx ex) (orb ta ea)) then CPoison
          else CNest (nseq (rev nth)) (nseq (rev nel))
      end
  end.

(* STRICT STRUCTURE (#4631): a line is a sequence of ops, `(…)` loops,
   `[…|…]` conditional loops and `{…|…}` branches. Anything else poisons the
   line instead of being read as some nearby well-formed certificate:
   - a region still open at newline/EOF (a dropped `}` used to close silently,
     so `{|id` read as `{|id}`);
   - a `{…}` or `[…]` without its one `|`, or with a second one (a dropped `|`
     moved the else arm's ops into the then arm: `{i|d}` REJECT became `{id}`
     ACCEPT);
   - a delimiter inside a `{…}` / `[…]` arm (arms are flat; the recorder
     poisons a nested region itself);
   - a `|` or closer outside the region it closes, and a `(` inside an open
     `(` (a dropped `)` used to restart the body, discarding its ops). *)
Definition is_struct (b : ascii) : bool :=
  orb (orb (orb (Ascii.eqb b lbrace) (Ascii.eqb b rbrace))
           (orb (Ascii.eqb b lbracket) (Ascii.eqb b rbracket)))
      (orb (orb (Ascii.eqb b lparen) (Ascii.eqb b rparen)) (Ascii.eqb b bar)).

Definition finish_line_x (cur : list CertItem) (lp : option (list Op))
                         (cp : option condst) (bp : option brxst) : list CertItem :=
  match bp, cp, lp with
  | None, None, None => rev cur
  | _, _, _ => rev (CPoison :: cur)
  end.

Fixpoint parse_xc (s : string) (cur : list CertItem)
                  (lp : option (list Op)) (cp : option condst) (bp : option brxst)
                  {struct s} : list (list CertItem) :=
  match s with
  | EmptyString => [finish_line_x cur lp cp bp]
  | String b rest =>
      if Ascii.eqb b newline then finish_line_x cur lp cp bp :: parse_xc rest [] None None None
      else match bp with
      | Some ((in_else, tx, ex, ta, ea, bad, th, el, stk) as st) =>
          (* inside `{ … | … }` — collect nodes / the terminal marker per arm;
             a `{` opens a nested branch (format v10). *)
          if Ascii.eqb b bar then
            match stk with
            | (fe, ft, fl) :: stk' =>
                if fe then parse_xc rest (CPoison :: cur) lp cp None
                else parse_xc rest cur lp cp (Some (in_else, tx, ex, ta, ea, bad, th, el, (true, ft, fl) :: stk'))
            | [] =>
                if in_else then parse_xc rest (CPoison :: cur) lp cp None
                else parse_xc rest cur lp cp (Some (true, tx, ex, ta, ea, bad, th, el, []))
            end
          else if Ascii.eqb b rbrace then
            match stk with
            | (fe, ft, fl) :: stk' =>
                if fe
                then parse_xc rest cur lp cp
                       (Some (brx_add (NNBr (nseq (rev ft)) (nseq (rev fl))) (in_else, tx, ex, ta, ea, bad, th, el, stk')))
                else parse_xc rest (CPoison :: cur) lp cp None
            | [] =>
                if in_else
                then parse_xc rest (close_brx st :: cur) lp cp None
                else parse_xc rest (CPoison :: cur) lp cp None
            end
          else if Ascii.eqb b lbrace then
            (* a nested branch opens in the innermost arm; opened in the
               branch's own arm after its terminal marker, it is malformed. *)
            let bad' := match stk with
                        | [] => orb bad (if in_else then orb ex ea else orb tx ta)
                        | _ => bad
                        end in
            parse_xc rest cur lp cp (Some (in_else, tx, ex, ta, ea, bad', th, el, (false, [], []) :: stk))
          else if is_struct b then parse_xc rest (CPoison :: cur) lp cp None
          else if orb (is_xmark b) (is_tmark b) then
            match stk with
            | _ :: _ =>
                (* a terminal marker inside a nested arm: exit and abort arms
                   are flat — malformed. *)
                parse_xc rest (CPoison :: cur) lp cp None
            | [] =>
                if is_xmark b then
                  (* mark the CURRENT arm exited; a second terminal mark on the
                     same arm is malformed. *)
                  if in_else
                  then parse_xc rest cur lp cp (Some (true, tx, true, ta, ea, orb bad (orb ex ea), th, el, []))
                  else parse_xc rest cur lp cp (Some (false, true, ex, ta, ea, orb bad (orb tx ta), th, el, []))
                else
                  (* v6: mark the CURRENT arm aborting; a second terminal mark
                     on the same arm is malformed. *)
                  if in_else
                  then parse_xc rest cur lp cp (Some (true, tx, ex, ta, true, orb bad (orb ex ea), th, el, []))
                  else parse_xc rest cur lp cp (Some (false, tx, ex, true, ea, orb bad (orb tx ta), th, el, []))
            end
          else match parse_byte b with
               | Some op => parse_xc rest cur lp cp (Some (brx_add (NNAt op) st))
               | None => parse_xc rest cur lp cp bp
               end
      | None =>
          (* an exit or abort marker OUTSIDE a `{…}` branch is malformed —
             poison the line (a top-level terminal has no branch to survive
             it). *)
          if orb (is_xmark b) (is_tmark b) then parse_xc rest (CPoison :: cur) lp cp None
          else match cp with
          | Some (in_else, th, el) =>
              (* inside `[ … | … ]` — exactly parse_clc's conditional-loop state *)
              if Ascii.eqb b bar then
                if in_else then parse_xc rest (CPoison :: cur) lp None None
                else parse_xc rest cur lp (Some (true, th, el)) None
              else if Ascii.eqb b rbracket then
                if in_else then parse_xc rest (CCondLoop (rev th) (rev el) :: cur) lp None None
                else parse_xc rest (CPoison :: cur) lp None None
              else if is_struct b then parse_xc rest (CPoison :: cur) lp None None
              else match parse_byte b with
                   | Some op =>
                       if in_else then parse_xc rest cur lp (Some (true, th, op :: el)) None
                       else parse_xc rest cur lp (Some (false, op :: th, el)) None
                   | None => parse_xc rest cur lp cp None
                   end
          | None =>
              if Ascii.eqb b lbrace then parse_xc rest cur lp None (Some (false, false, false, false, false, false, [], [], []))
              else if Ascii.eqb b lbracket then parse_xc rest cur lp (Some (false, [], [])) None
              else if Ascii.eqb b lparen then
                match lp with
                | Some _ => parse_xc rest (CPoison :: cur) None None None
                | None => parse_xc rest cur (Some []) None None
                end
              else if Ascii.eqb b rparen then
                match lp with
                | Some body => parse_xc rest (CLoop (rev body) :: cur) None None None
                | None => parse_xc rest (CPoison :: cur) None None None
                end
              else if is_struct b then parse_xc rest (CPoison :: cur) lp None None
              else match parse_byte b with
                   | Some op =>
                       match lp with
                       | Some body => parse_xc rest cur (Some (op :: body)) None None
                       | None => parse_xc rest (COp op :: cur) None None None
                       end
                   | None => parse_xc rest cur lp None None
                   end
          end
      end
  end.

(* ─── OWNED-LINE RESURRECTION (#3229) ───
   `Alias` folds like `Inc`, with no liveness guard: a borrowed PARAM's line
   starts at 0 with the caller's reference uncounted, so an `a` at count 0 is
   that line's normal first event (`amamam`: dup the param, move the dup out,
   again). But the same fold also accepted `idam`: an OWNED object allocated,
   released to 0 — FREED — and then aliased and moved out. That is a
   use-after-free the balance cannot see (it nets to 0). The MIR lowering
   produced exactly that witness when a field-path copy-on-write rebound a
   `mut` param inside a model-one-iteration `while` frame: the frame-end drop
   (`drop_arm_locals`) freed the copy, and the write-back then `Dup`'d it.

   The rule, decided from the certificate alone: a line whose FIRST item is a
   top-level fresh acquire (`COp Inc`) is an OWNED object — born at that event,
   with no reference outside this frame. On such a line a count of 0 after the
   birth means the object is DEAD, so every later ALIAS (`a`, a new reference
   to an object that must already exist) must meet a live count. `guard_line`
   makes that obligation explicit by placing the existing `b` liveness probe
   (+0, faults at 0) before every `a` after the birth, inside loop bodies and
   branch arms too; the line is then judged by the unchanged `check_line`.
   A fresh `i` is NOT probed: it is a block straight from the allocator, so it
   can never revive a freed one — on a loop-carried slot's line it is the
   slot's next value (`i(di)m`: drop the old accumulator, allocate the new).
   A line that does not start with `COp Inc` (a borrowed param's, a slot whose
   first event is a loop) is judged exactly as before. No new byte: every
   certificate parses as it did. *)
Definition guard_op (o : Op) : list Op :=
  match o with
  | Alias => [Borrow; Alias]
  (* a share-out of an owned object is a share of it: it must be live too. *)
  | ShareOut => [Borrow; ShareOut]
  | _ => [o]
  end.

Definition guard_ops (ops : list Op) : list Op := flat_map guard_op ops.

(* ops in front of a nested arm *)
Fixpoint napp (l : list Op) (n : NOps) : NOps :=
  match l with [] => n | o :: r => NOp1 o (napp r n) end.

(* the probes inside a nested arm, at every depth *)
Fixpoint nguard (n : NOps) : NOps :=
  match n with
  | NEnd => NEnd
  | NOp1 o rest => napp (guard_op o) (nguard rest)
  | NBr t e rest => NBr (nguard t) (nguard e) (nguard rest)
  end.

Definition guard_item (c : CertItem) : list CertItem :=
  match c with
  | COp o => map COp (guard_op o)
  | CLoop body => [CLoop (guard_ops body)]
  | CCondLoop thenb elseb => [CCondLoop (guard_ops thenb) (guard_ops elseb)]
  | CBranch thenb elseb => [CBranch (guard_ops thenb) (guard_ops elseb)]
  | CBranchRet f thenb elseb => [CBranchRet f (guard_ops thenb) (guard_ops elseb)]
  | CBranchAbort f thenb elseb => [CBranchAbort f (guard_ops thenb) (guard_ops elseb)]
  | CNest thenb elseb => [CNest (nguard thenb) (nguard elseb)]
  | CPoison => [CPoison]
  end.

Definition guard_line (cs : list CertItem) : list CertItem :=
  match cs with
  | COp Inc :: rest => COp Inc :: flat_map guard_item rest
  | _ => cs
  end.

(* The probes only ADD faults: wherever the guarded run succeeds, the
   unguarded run succeeds with the same count. *)
Lemma exec_guard_op :
  forall o rc r, exec (guard_op o) rc = Some r -> exec [o] rc = Some r.
Proof.
  intros o rc r H. destruct o; simpl in *; try exact H;
    destruct (rc <=? 0); try discriminate; exact H.
Qed.

Lemma exec_guard_ops :
  forall ops rc r, exec (guard_ops ops) rc = Some r -> exec ops rc = Some r.
Proof.
  unfold guard_ops.
  induction ops as [| o ops IH]; intros rc r H.
  - exact H.
  - cbn [flat_map] in H. rewrite exec_app in H. rewrite exec_cons.
    destruct (exec (guard_op o) rc) as [r1 |] eqn:E1; [| discriminate].
    rewrite (exec_guard_op o rc r1 E1). exact (IH r1 r H).
Qed.

Lemma nexec_napp :
  forall l n rc,
    nexec (napp l n) rc = match exec l rc with Some r => nexec n r | None => None end.
Proof.
  induction l as [| o l IH]; intros n rc; [reflexivity |].
  cbn [napp nexec]. change (o :: l) with ([o] ++ l). rewrite exec_app.
  destruct (exec [o] rc) as [r1 |]; [apply IH | reflexivity].
Qed.

Lemma nexec_guard :
  forall n rc r, nexec (nguard n) rc = Some r -> nexec n rc = Some r.
Proof.
  induction n as [| o rest IH | t IHt e IHe rest IH]; intros rc r H.
  - exact H.
  - cbn [nguard] in H. rewrite nexec_napp in H. cbn [nexec].
    destruct (exec (guard_op o) rc) as [r1 |] eqn:E1; [| discriminate].
    rewrite (exec_guard_op o rc r1 E1). exact (IH r1 r H).
  - cbn [nguard nexec] in H |- *.
    destruct (nexec (nguard t) rc) as [rt |] eqn:Et; [| discriminate].
    destruct (nexec (nguard e) rc) as [re |] eqn:Ee; [| discriminate].
    rewrite (IHt rc rt Et), (IHe rc re Ee).
    destruct (Z.eqb rt re); [exact (IH rt r H) | discriminate].
Qed.

(* exec_line is a left fold over the items, like exec over the ops. *)
Lemma exec_line_app :
  forall a b rc,
    exec_line (a ++ b) rc =
      match exec_line a rc with Some r => exec_line b r | None => None end.
Proof.
  induction a as [| c a IH]; intros b rc; [reflexivity |].
  destruct c as [o | body | t e | t e | f t e | f t e | t e |]; cbn [app exec_line].
  - destruct (exec [o] rc); [apply IH | reflexivity].
  - destruct (exec body rc) as [r |]; [| reflexivity].
    destruct (Z.eqb r rc); [apply IH | reflexivity].
  - destruct (exec t rc) as [rt |]; destruct (exec e rc) as [re |]; try reflexivity.
    destruct (andb (Z.eqb rt rc) (Z.eqb re rc)); [apply IH | reflexivity].
  - destruct (exec t rc) as [rt |]; destruct (exec e rc) as [re |]; try reflexivity.
    destruct (Z.eqb rt re); [apply IH | reflexivity].
  - destruct (exec t rc) as [rt |]; destruct (exec e rc) as [re |]; try reflexivity.
    destruct f; [destruct (Z.eqb rt 0) | destruct (Z.eqb re 0)];
      first [apply IH | reflexivity].
  - destruct (exec t rc) as [rt |]; destruct (exec e rc) as [re |]; try reflexivity.
    destruct f; apply IH.
  - destruct (nexec t rc) as [rt |]; destruct (nexec e rc) as [re |]; try reflexivity.
    destruct (Z.eqb rt re); [apply IH | reflexivity].
  - reflexivity.
Qed.

Lemma exec_line_guard_item :
  forall c rc r, exec_line (guard_item c) rc = Some r -> exec_line [c] rc = Some r.
Proof.
  intros c rc r H.
  destruct c as [o | body | t e | t e | f t e | f t e | t e |]; cbn [guard_item] in H.
  - destruct o; simpl in *; try exact H;
      destruct (rc <=? 0); try discriminate; exact H.
  - cbn [exec_line] in H |- *.
    destruct (exec (guard_ops body) rc) as [r1 |] eqn:E; [| discriminate].
    rewrite (exec_guard_ops body rc r1 E). exact H.
  - cbn [exec_line] in H |- *.
    destruct (exec (guard_ops t) rc) as [rt |] eqn:Et; [| discriminate].
    destruct (exec (guard_ops e) rc) as [re |] eqn:Ee; [| discriminate].
    rewrite (exec_guard_ops t rc rt Et), (exec_guard_ops e rc re Ee). exact H.
  - cbn [exec_line] in H |- *.
    destruct (exec (guard_ops t) rc) as [rt |] eqn:Et; [| discriminate].
    destruct (exec (guard_ops e) rc) as [re |] eqn:Ee; [| discriminate].
    rewrite (exec_guard_ops t rc rt Et), (exec_guard_ops e rc re Ee). exact H.
  - cbn [exec_line] in H |- *.
    destruct (exec (guard_ops t) rc) as [rt |] eqn:Et; [| discriminate].
    destruct (exec (guard_ops e) rc) as [re |] eqn:Ee; [| discriminate].
    rewrite (exec_guard_ops t rc rt Et), (exec_guard_ops e rc re Ee). exact H.
  - cbn [exec_line] in H |- *.
    destruct (exec (guard_ops t) rc) as [rt |] eqn:Et; [| discriminate].
    destruct (exec (guard_ops e) rc) as [re |] eqn:Ee; [| discriminate].
    rewrite (exec_guard_ops t rc rt Et), (exec_guard_ops e rc re Ee). exact H.
  - cbn [exec_line] in H |- *.
    destruct (nexec (nguard t) rc) as [rt |] eqn:Et; [| discriminate].
    destruct (nexec (nguard e) rc) as [re |] eqn:Ee; [| discriminate].
    rewrite (nexec_guard t rc rt Et), (nexec_guard e rc re Ee). exact H.
  - discriminate.
Qed.

Lemma exec_line_guard_items :
  forall cs rc r,
    exec_line (flat_map guard_item cs) rc = Some r -> exec_line cs rc = Some r.
Proof.
  induction cs as [| c cs IH]; intros rc r H; [exact H |].
  cbn [flat_map] in H. rewrite exec_line_app in H.
  change (c :: cs) with ([c] ++ cs). rewrite exec_line_app.
  destruct (exec_line (guard_item c) rc) as [r1 |] eqn:E1; [| discriminate].
  rewrite (exec_line_guard_item c rc r1 E1). exact (IH r1 r H).
Qed.

(* Every line the guarded checker accepts, the unguarded one accepts: the
   resurrection rule only REJECTS more, so every soundness theorem proved
   about `check_line` carries over to `check_xc` unchanged. *)
Lemma check_line_guard_weaken :
  forall cs, check_line (guard_line cs) = true -> check_line cs = true.
Proof.
  intros cs H. destruct cs as [| c cs]; [exact H |].
  destruct c as [o | | | | | | |]; try exact H. destruct o; try exact H.
  unfold check_line in *. cbn [guard_line exec_line] in H |- *.
  destruct (exec [Inc] 0) as [r0 |]; [| exact H].
  destruct (exec_line (flat_map guard_item cs) r0) as [z |] eqn:E; [| discriminate].
  rewrite (exec_line_guard_items cs r0 z E). exact H.
Qed.

(* Unrolling commutes with the guard: a concrete run of an owned line's tail,
   with a probe before every alias, is a concrete run of the guarded tail. *)
Lemma guard_ops_app :
  forall a b, guard_ops (a ++ b) = guard_ops a ++ guard_ops b.
Proof. intros a b. unfold guard_ops. apply flat_map_app. Qed.

Lemma guard_ops_repeat :
  forall body n,
    guard_ops (List.concat (List.repeat body n)) =
      List.concat (List.repeat (guard_ops body) n).
Proof.
  intros body n. induction n as [| n IH]; [reflexivity |].
  cbn [List.repeat List.concat]. rewrite guard_ops_app, IH. reflexivity.
Qed.

Lemma guard_ops_cond :
  forall thenb elseb bs,
    guard_ops (cond_concat thenb elseb bs) =
      cond_concat (guard_ops thenb) (guard_ops elseb) bs.
Proof.
  intros thenb elseb bs. induction bs as [| b bs IH]; [reflexivity |].
  destruct b; cbn [cond_concat]; rewrite guard_ops_app, IH; reflexivity.
Qed.

Lemma unrolls_map_cop :
  forall l a b, UnrollsL a b -> UnrollsL (map COp l ++ a) (l ++ b).
Proof.
  induction l as [| o l IH]; intros a b H; [exact H |].
  cbn [map app]. apply UL_op. apply IH. exact H.
Qed.

Lemma nunrolls_napp :
  forall l a b, NUnrolls a b -> NUnrolls (napp l a) (l ++ b).
Proof.
  induction l as [| o l IH]; intros a b H; [exact H |].
  cbn [napp app]. apply NU_op. apply IH. exact H.
Qed.

Lemma nunrolls_guard :
  forall ns f, NUnrolls ns f -> NUnrolls (nguard ns) (guard_ops f).
Proof.
  intros ns f HU. induction HU as [| o a b HU IH | t e a ft b choice HT IHT HU IH];
    cbn [nguard].
  - constructor.
  - unfold guard_ops at 1. cbn [flat_map]. apply nunrolls_napp. exact IH.
  - rewrite guard_ops_app. destruct choice.
    + exact (NU_br (nguard t) (nguard e) _ _ _ true IHT IH).
    + exact (NU_br (nguard t) (nguard e) _ _ _ false IHT IH).
Qed.

Lemma unrolls_guard :
  forall cs fops, UnrollsL cs fops ->
    UnrollsL (flat_map guard_item cs) (guard_ops fops).
Proof.
  intros cs fops HU. induction HU; cbn [flat_map guard_item].
  - constructor.
  - unfold guard_ops at 1. cbn [flat_map]. apply unrolls_map_cop. exact IHHU.
  - rewrite guard_ops_app, guard_ops_repeat. apply UL_loop. exact IHHU.
  - rewrite guard_ops_app, guard_ops_cond. apply UL_cond. exact IHHU.
  - rewrite guard_ops_app. destruct choice.
    + exact (UL_branch (guard_ops thenb) (guard_ops elseb) _ _ true IHHU).
    + exact (UL_branch (guard_ops thenb) (guard_ops elseb) _ _ false IHHU).
  - destruct retthen.
    + exact (UL_branchret_exit true (guard_ops thenb) (guard_ops elseb) _).
    + exact (UL_branchret_exit false (guard_ops thenb) (guard_ops elseb) _).
  - rewrite guard_ops_app. destruct retthen.
    + exact (UL_branchret_cont true (guard_ops thenb) (guard_ops elseb) _ _ IHHU).
    + exact (UL_branchret_cont false (guard_ops thenb) (guard_ops elseb) _ _ IHHU).
  - rewrite guard_ops_app. destruct abthen.
    + exact (UL_branchabort_cont true (guard_ops thenb) (guard_ops elseb) _ _ IHHU).
    + exact (UL_branchabort_cont false (guard_ops thenb) (guard_ops elseb) _ _ IHHU).
  - rewrite guard_ops_app. apply nunrolls_guard in Hnest. destruct choice.
    + exact (UL_nest (nguard thenb) (nguard elseb) _ _ _ true Hnest IHHU).
    + exact (UL_nest (nguard thenb) (nguard elseb) _ _ _ false Hnest IHHU).
Qed.

(* ─── THE DRAIN RULE (ADR-0034 D2, format v8) ───
   A holder that PROBED a container (`u`) gives it up: once the container's
   slots may have been cleared by takes, the line reads it again only to
   probe it or to release it. On the line: after the first `Probe`, every
   later op is a `Probe`, or the line's one terminal release (`Dec` /
   `MoveOut`), and nothing follows that release. v1 accepts the flat form
   only — a probe inside a branch arm or a conditional loop, or a loop body
   that mixes a probe with anything else, rejects (completeness can widen
   later; a refusal is never unsound). *)
Definition is_probe (o : Op) : bool :=
  match o with Probe => true | _ => false end.

Definition probe_free (ops : list Op) : bool := forallb (fun o => negb (is_probe o)) ops.

Definition all_probes (ops : list Op) : bool := forallb is_probe ops.

Definition is_release_op (o : Op) : bool :=
  match o with Dec | MoveOut => true | _ => false end.

Fixpoint nprobe_free (n : NOps) : bool :=
  match n with
  | NEnd => true
  | NOp1 o rest => andb (negb (is_probe o)) (nprobe_free rest)
  | NBr t e rest => andb (andb (nprobe_free t) (nprobe_free e)) (nprobe_free rest)
  end.

Definition item_probe_free (c : CertItem) : bool :=
  match c with
  | COp o => negb (is_probe o)
  | CLoop b => probe_free b
  | CCondLoop t e | CBranch t e => andb (probe_free t) (probe_free e)
  | CBranchRet _ t e | CBranchAbort _ t e => andb (probe_free t) (probe_free e)
  | CNest t e => andb (nprobe_free t) (nprobe_free e)
  | CPoison => true
  end.

(* after the first probe: probes (flat, or a loop of probes only), then at
   most one release, then the end of the line. *)
Fixpoint drained_post (cs : list CertItem) : bool :=
  match cs with
  | [] => true
  | COp o :: rest =>
      if is_probe o then drained_post rest
      else andb (is_release_op o) (match rest with [] => true | _ => false end)
  | CLoop b :: rest => andb (all_probes b) (drained_post rest)
  | _ => false
  end.

Fixpoint drained (cs : list CertItem) : bool :=
  match cs with
  | [] => true
  | COp o :: rest => if is_probe o then drained_post rest else drained rest
  | CLoop b :: rest =>
      if probe_free b then drained rest
      else andb (all_probes b) (drained_post rest)
  | c :: rest => andb (item_probe_free c) (drained rest)
  end.

(* The same rule over a CONCRETE run: before the first probe anything; after
   it, probes and at most one final release. *)
Fixpoint ops_post (ops : list Op) : bool :=
  match ops with
  | [] => true
  | o :: rest =>
      if is_probe o then ops_post rest
      else andb (is_release_op o) (match rest with [] => true | _ => false end)
  end.

Fixpoint ops_drained (ops : list Op) : bool :=
  match ops with
  | [] => true
  | o :: rest => if is_probe o then ops_post rest else ops_drained rest
  end.

Lemma ops_drained_probe_free_app :
  forall a b, probe_free a = true -> ops_drained (a ++ b) = ops_drained b.
Proof.
  induction a as [| o a IH]; intros b H; [reflexivity |].
  cbn [probe_free forallb] in H. apply andb_prop in H. destruct H as [Ho Ha].
  destruct o; cbn in Ho |- *; try discriminate; apply IH; exact Ha.
Qed.

Lemma ops_drained_probe_free :
  forall a, probe_free a = true -> ops_drained a = true.
Proof.
  intros a H. rewrite <- (app_nil_r a). rewrite ops_drained_probe_free_app; auto.
Qed.

Lemma ops_post_probes_app :
  forall a b, all_probes a = true -> ops_post (a ++ b) = ops_post b.
Proof.
  induction a as [| o a IH]; intros b H; [reflexivity |].
  cbn [all_probes forallb] in H. apply andb_prop in H. destruct H as [Ho Ha].
  destruct o; cbn in Ho |- *; try discriminate; apply IH; exact Ha.
Qed.

Lemma ops_drained_probes_app :
  forall a b, all_probes a = true -> ops_post b = true ->
    ops_drained (a ++ b) = true \/ a = [] .
Proof.
  intros a b Ha Hb. destruct a as [| o a]; [right; reflexivity | left].
  cbn [all_probes forallb] in Ha. apply andb_prop in Ha. destruct Ha as [Ho Ha].
  destruct o; cbn in Ho |- *; try discriminate.
  rewrite ops_post_probes_app; assumption.
Qed.

Lemma probe_free_concat_repeat :
  forall b n, probe_free b = true -> probe_free (List.concat (List.repeat b n)) = true.
Proof.
  intros b n H. induction n as [| n IH]; [reflexivity |].
  cbn [List.repeat List.concat]. unfold probe_free in *.
  rewrite forallb_app, H, IH. reflexivity.
Qed.

Lemma all_probes_concat_repeat :
  forall b n, all_probes b = true -> all_probes (List.concat (List.repeat b n)) = true.
Proof.
  intros b n H. induction n as [| n IH]; [reflexivity |].
  cbn [List.repeat List.concat]. unfold all_probes in *.
  rewrite forallb_app, H, IH. reflexivity.
Qed.

Lemma probe_free_cond_concat :
  forall t e bs, probe_free t = true -> probe_free e = true ->
    probe_free (cond_concat t e bs) = true.
Proof.
  intros t e bs Ht He. induction bs as [| b bs IH]; [reflexivity |].
  destruct b; cbn [cond_concat]; unfold probe_free in *;
    rewrite forallb_app; rewrite ?Ht, ?He, IH; reflexivity.
Qed.

Lemma probe_free_if :
  forall (c : bool) t e, probe_free t = true -> probe_free e = true ->
    probe_free (if c then t else e) = true.
Proof. intros c t e Ht He. destruct c; assumption. Qed.

Lemma nprobe_free_unroll :
  forall ns f, NUnrolls ns f -> nprobe_free ns = true -> probe_free f = true.
Proof.
  intros ns f HU. induction HU as [| o a b HU IH | t e a ft b choice HT IHT HU IH];
    intros H; cbn [nprobe_free] in H.
  - reflexivity.
  - apply andb_prop in H. destruct H as [Ho Hr].
    unfold probe_free in *. cbn [forallb]. rewrite Ho. exact (IH Hr).
  - apply andb_prop in H. destruct H as [Hte Hr]. apply andb_prop in Hte. destruct Hte as [Ht He].
    unfold probe_free in *. rewrite forallb_app. rewrite (IH Hr).
    destruct choice; rewrite (IHT ltac:(assumption)); reflexivity.
Qed.

(* The post-probe half: a line accepted by `drained_post` unrolls only to
   runs `ops_post` accepts. *)
Lemma drained_post_unroll :
  forall cs fops, UnrollsL cs fops -> drained_post cs = true -> ops_post fops = true.
Proof.
  intros cs fops HU. induction HU; intros H; cbn [drained_post] in H.
  - reflexivity.
  - revert H; destruct (is_probe o) eqn:Ep; intro H.
    + destruct o; cbn in Ep; try discriminate. cbn. apply IHHU. exact H.
    + apply andb_prop in H. destruct H as [Hr Hn].
      destruct a; [| discriminate].
      match goal with Hu : UnrollsL [] _ |- _ => inversion Hu; subst end.
      cbn. rewrite Ep, Hr. reflexivity.
  - apply andb_prop in H. destruct H as [Hb Hr].
    rewrite ops_post_probes_app; [apply IHHU; exact Hr |].
    apply all_probes_concat_repeat. exact Hb.
  - discriminate.
  - discriminate.
  - discriminate.
  - discriminate.
  - discriminate.
  - discriminate.
Qed.

Lemma drained_unroll :
  forall cs fops, UnrollsL cs fops -> drained cs = true -> ops_drained fops = true.
Proof.
  intros cs fops HU. induction HU; intros H; cbn [drained item_probe_free] in H.
  - reflexivity.
  - revert H; destruct (is_probe o) eqn:Ep; intro H.
    + destruct o; cbn in Ep; try discriminate. cbn.
      eapply drained_post_unroll; eassumption.
    + cbn. rewrite Ep. apply IHHU. exact H.
  - revert H; destruct (probe_free body) eqn:Ef; intro H.
    + rewrite ops_drained_probe_free_app; [apply IHHU; exact H |].
      apply probe_free_concat_repeat. exact Ef.
    + apply andb_prop in H. destruct H as [Hb Hr].
      assert (Hpost : ops_post b = true) by (eapply drained_post_unroll; eassumption).
      destruct (ops_drained_probes_app _ b (all_probes_concat_repeat body n Hb) Hpost)
        as [Hd | He]; [exact Hd |].
      rewrite He. cbn [app].
      (* zero iterations: the rest of the line is in the post state; a post
         run is also a drained run. *)
      clear -Hpost. induction b as [| o b IH]; [reflexivity |].
      cbn in Hpost |- *. destruct (is_probe o); [exact Hpost |].
      apply andb_prop in Hpost. destruct Hpost as [Hr Hn].
      destruct b; [| discriminate]. reflexivity.
  - apply andb_prop in H. destruct H as [Hf Hr]. apply andb_prop in Hf. destruct Hf as [Ht He].
    rewrite ops_drained_probe_free_app; [apply IHHU; exact Hr |].
    apply probe_free_cond_concat; assumption.
  - apply andb_prop in H. destruct H as [Hf Hr]. apply andb_prop in Hf. destruct Hf as [Ht He].
    rewrite ops_drained_probe_free_app; [apply IHHU; exact Hr |].
    apply probe_free_if; assumption.
  - apply andb_prop in H. destruct H as [Hf _]. apply andb_prop in Hf. destruct Hf as [Ht He].
    apply ops_drained_probe_free. apply probe_free_if; assumption.
  - apply andb_prop in H. destruct H as [Hf Hr]. apply andb_prop in Hf. destruct Hf as [Ht He].
    rewrite ops_drained_probe_free_app; [apply IHHU; exact Hr |].
    apply probe_free_if; assumption.
  - apply andb_prop in H. destruct H as [Hf Hr]. apply andb_prop in Hf. destruct Hf as [Ht He].
    rewrite ops_drained_probe_free_app; [apply IHHU; exact Hr |].
    apply probe_free_if; assumption.
  - apply andb_prop in H. destruct H as [Hf Hr]. apply andb_prop in Hf. destruct Hf as [Ht He].
    rewrite ops_drained_probe_free_app; [apply IHHU; exact Hr |].
    apply (nprobe_free_unroll _ _ Hnest). destruct choice; assumption.
Qed.

(* The full exit-aware checker over raw certificate bytes (format v6), with
   the owned-line resurrection rule (#3229) and the drain rule (format v8,
   ADR-0034). *)
Definition check_xc (s : string) : bool :=
  forallb (fun cs => andb (check_line (guard_line cs)) (drained cs))
          (parse_xc s [] None None None).

(* Every line `check_xc` accepts passes the guarded balance check. *)
Lemma check_xc_line :
  forall s cs, check_xc s = true -> In cs (parse_xc s [] None None None) ->
    check_line (guard_line cs) = true /\ drained cs = true.
Proof.
  intros s cs H Hin. unfold check_xc in H. rewrite forallb_forall in H.
  apply andb_prop. exact (H cs Hin).
Qed.

(* THE DRAIN THEOREM: on an accepted certificate, every concrete run of every
   line, once it has probed its container, only probes it again and ends in
   at most one release — nothing reads a container whose slots a take may
   have cleared. *)
Theorem check_xc_drained_sound :
  forall s, check_xc s = true ->
    forall cs, In cs (parse_xc s [] None None None) ->
      forall fops, UnrollsL cs fops -> ops_drained fops = true.
Proof.
  intros s H cs Hin fops HU.
  destruct (check_xc_line s cs H Hin) as [_ Hd].
  exact (drained_unroll cs fops HU Hd).
Qed.

(* SOUNDNESS over bytes (1-line corollary of check_line_unroll_sound, covering
   CBranchRet via UL_branchret_exit / UL_branchret_cont): an accepted exit-aware
   certificate has, for EVERY parsed line and EVERY concrete unrolling — each
   branch resolved to the ONE arm the runtime takes, a returning arm ENDING the
   run — no double-free / use-after-free and no leak. *)
Theorem check_xc_unroll_sound :
  forall s, check_xc s = true ->
    forall cs, In cs (parse_xc s [] None None None) ->
      forall fops, UnrollsL cs fops -> run fops <> None /\ run fops = Some 0.
Proof.
  intros s H cs Hin fops HU.
  destruct (check_xc_line s cs H Hin) as [Hl _].
  apply (check_line_unroll_sound cs (check_line_guard_weaken cs Hl) fops HU).
Qed.

(* NO RESURRECTION (#3229): on an accepted certificate, every OWNED line (one
   born by a top-level fresh acquire) never aliases its object again once it
   is dead. Stated over the concrete runs: for EVERY unrolling `f` of the
   line's tail, the run with a liveness probe before each later alias
   (`guard_ops f`) is fault-free — so no `a` after the birth ever meets a
   count of 0 — and still balances. Without the rule, `idam` (freed, then
   aliased and moved out) balanced to 0 and was accepted. *)
Theorem check_xc_no_resurrection :
  forall s, check_xc s = true ->
    forall rest, In (COp Inc :: rest) (parse_xc s [] None None None) ->
      forall f, UnrollsL rest f ->
        run (Inc :: guard_ops f) <> None /\ run (Inc :: guard_ops f) = Some 0.
Proof.
  intros s H rest Hin f HU.
  destruct (check_xc_line s _ H Hin) as [Hline _]. cbn [guard_line] in Hline.
  apply (check_line_unroll_sound _ Hline).
  apply UL_op. apply unrolls_guard. exact HU.
Qed.

(* FORMAT v6 SAFETY over bytes: an accepted certificate has, for EVERY parsed
   line and EVERY run that ends in an ABORT (the `t` arm), no double-free /
   use-after-free up to the abort. Together with check_xc_unroll_sound (every
   non-aborting run: safe AND exactly balanced) this covers every run. *)
Theorem check_xc_abort_sound :
  forall s, check_xc s = true ->
    forall cs, In cs (parse_xc s [] None None None) ->
      forall fops, AbortsL cs fops -> run fops <> None.
Proof.
  intros s H cs Hin fops HA.
  destruct (check_xc_line s cs H Hin) as [Hl _].
  apply (check_line_abort_sound cs (check_line_guard_weaken cs Hl) fops HA).
Qed.

(* backward-compat (flat + loop + cond-loop + branch certs verify exactly as
   before) + the new exit-marked branch certs, on real bytes *)
Example cert_xc_flat_accepts : check_xc "iidd"%string = true.
Proof. reflexivity. Qed.
Example cert_xc_flat_rejects : check_xc "idd"%string = false.
Proof. reflexivity. Qed.
Example cert_xc_loop_accepts : check_xc "i(di)m"%string = true.          (* v2 CLoop still accepts *)
Proof. reflexivity. Qed.
Example cert_xc_filter_accepts : check_xc "i[di|]m"%string = true.       (* v3 CCondLoop still accepts *)
Proof. reflexivity. Qed.
Example cert_xc_branch_accepts : check_xc "i{i|i}dd"%string = true.      (* v4 CBranch still accepts *)
Proof. reflexivity. Qed.
(* #4631 strict structure: the witness mutation lab's drop-`|` and drop-`}`
   cells, a moved separator, and stray / unterminated delimiters. *)
Example cert_xc_lab_branch_accepts : check_xc "{|id}"%string = true.
Proof. reflexivity. Qed.
Example cert_xc_lab_drop_bar_rejects : check_xc "{id}"%string = false.
Proof. reflexivity. Qed.
Example cert_xc_lab_drop_rbrace_rejects : check_xc "{|id"%string = false.
Proof. reflexivity. Qed.
Example cert_xc_lab_drop_bar_im_rejects : check_xc "{im}"%string = false.
Proof. reflexivity. Qed.
Example cert_xc_lab_drop_rbrace_im_rejects : check_xc "{|im"%string = false.
Proof. reflexivity. Qed.
Example cert_xc_second_bar_rejects : check_xc "{|id|}"%string = false.
Proof. reflexivity. Qed.
Example cert_xc_stray_bar_rejects : check_xc "i|d"%string = false.
Proof. reflexivity. Qed.
Example cert_xc_stray_rparen_rejects : check_xc "id)"%string = false.
Proof. reflexivity. Qed.
Example cert_xc_open_loop_rejects : check_xc "(id"%string = false.
Proof. reflexivity. Qed.
Example cert_xc_open_cond_rejects : check_xc "[id|"%string = false.
Proof. reflexivity. Qed.
Example cert_xc_reopened_loop_rejects : check_xc "i(id(di)m"%string = false.
Proof. reflexivity. Qed.
Example cert_xc_nested_arm_rejects : check_xc "{(id)|}"%string = false.
Proof. reflexivity. Qed.
Example cert_xc_branch_disagree_rejects : check_xc "i{i|}d"%string = false. (* v4 disagreement still rejects *)
Proof. reflexivity. Qed.
(* THE `!` SHAPE: own a value; the err arm drops it and returns (`dx` — at 0 at
   the exit), the ok arm keeps it; the surviving path releases it later. *)
Example cert_xc_exit_then_accepts : check_xc "i{dx|}d"%string = true.
Proof. reflexivity. Qed.
(* the mirror: the ELSE arm is the returning one. *)
Example cert_xc_exit_else_accepts : check_xc "i{|dx}d"%string = true.
Proof. reflexivity. Qed.
(* the returned value itself: moved out (`m`) by the exit — same −1, accepts. *)
Example cert_xc_exit_move_accepts : check_xc "i{mx|}d"%string = true.
Proof. reflexivity. Qed.
(* LEAK ON THE EXIT PATH: the returning arm still owns the value (no `d` before
   `x`) — the rest of the line never runs there, so this MUST reject. *)
Example cert_xc_exit_leak_rejects : check_xc "i{x|}d"%string = false.
Proof. reflexivity. Qed.
(* malformed: `x` not arm-terminal / both arms exiting / outside a branch. *)
Example cert_xc_exit_midarm_rejects : check_xc "i{xd|}d"%string = false.
Proof. reflexivity. Qed.
Example cert_xc_exit_both_rejects : check_xc "i{dx|dx}"%string = false.
Proof. reflexivity. Qed.
Example cert_xc_exit_toplevel_rejects : check_xc "idx"%string = false.
Proof. reflexivity. Qed.

(* FORMAT v6 — the abort terminal `t`. An owned value the aborting arm still
   holds is DISCHARGED (the process is gone); the surviving path releases it. *)
Example cert_xc_abort_holding_accepts : check_xc "i{t|d}"%string = true.
Proof. reflexivity. Qed.
(* born on the aborting path only, never released: accepts. *)
Example cert_xc_abort_born_accepts : check_xc "{it|}"%string = true.
Proof. reflexivity. Qed.
(* the mirror: the ELSE arm aborts; the line continues from the then arm. *)
Example cert_xc_abort_else_accepts : check_xc "i{|t}d"%string = true.
Proof. reflexivity. Qed.
(* the aborting arm must still be SAFE: a release at 0 before the abort. *)
Example cert_xc_abort_fault_rejects : check_xc "{dt|}"%string = false.
Proof. reflexivity. Qed.
(* A PATH THAT "ABORTS" BUT CONTINUES: an op after `t` is malformed. *)
Example cert_xc_abort_continues_rejects : check_xc "i{td|d}"%string = false.
Proof. reflexivity. Qed.
(* outside a branch / both arms terminal / exit and abort on one site. *)
Example cert_xc_abort_toplevel_rejects : check_xc "it"%string = false.
Proof. reflexivity. Qed.
Example cert_xc_abort_both_rejects : check_xc "i{t|t}"%string = false.
Proof. reflexivity. Qed.
Example cert_xc_abort_and_exit_rejects : check_xc "i{dx|t}"%string = false.
Proof. reflexivity. Qed.
(* the SURVIVING path still balances exactly: a leak there rejects. *)
Example cert_xc_abort_survivor_leak_rejects : check_xc "i{t|}"%string = false.
Proof. reflexivity. Qed.
(* two terminal marks on one arm are malformed. *)
Example cert_xc_abort_double_mark_rejects : check_xc "i{tx|}d"%string = false.
Proof. reflexivity. Qed.

(* FORMAT v10 — NESTED BRANCHES. A branch arm holding a branch of its own:
   every branch, at every depth, runs both arms fault-free from its entry
   count and they AGREE. *)
Example cert_xc_nested_accepts : check_xc "i{a{d|m}|}d"%string = true.
Proof. reflexivity. Qed.
Example cert_xc_nested_two_levels_accepts : check_xc "i{{a{d|m}|}|b}d"%string = true.
Proof. reflexivity. Qed.
(* a nested arm that releases twice faults at its own entry count. *)
Example cert_xc_nested_double_release_rejects : check_xc "i{{dd|}|}d"%string = false.
Proof. reflexivity. Qed.
(* nested arms whose nets differ never agree: refused at that depth. *)
Example cert_xc_nested_unequal_nets_rejects : check_xc "i{{a|}|}d"%string = false.
Proof. reflexivity. Qed.
(* the nested branch moved before the share it releases (arms reordered):
   the probe before the share meets a dead owned object — refused. *)
Example cert_xc_nested_reordered_rejects : check_xc "i{{d|m}a|}d"%string = false.
Proof. reflexivity. Qed.
(* an arm swapped across the levels (the share that the nested branch
   releases moved into the other top-level arm): the arms no longer agree —
   refused. *)
Example cert_xc_nested_swapped_arm_rejects : check_xc "i{{d|m}|a}d"%string = false.
Proof. reflexivity. Qed.
(* exit and abort arms are flat: a marker in a nested arm, a marker beside a
   nested branch, a loop in a nested arm and an unclosed nested branch
   poison the line. *)
Example cert_xc_nested_exit_rejects : check_xc "i{{dx|}|}d"%string = false.
Proof. reflexivity. Qed.
Example cert_xc_nested_beside_exit_rejects : check_xc "i{{d|d}x|}d"%string = false.
Proof. reflexivity. Qed.
Example cert_xc_nested_loop_rejects : check_xc "i{{(d)|}|}d"%string = false.
Proof. reflexivity. Qed.
Example cert_xc_nested_unclosed_rejects : check_xc "i{{d|}|d"%string = false.
Proof. reflexivity. Qed.

(* #3229 — OWNED-LINE RESURRECTION. The witness the MIR lowering emitted for
   `drain(mut u)` with the copy-on-write in a modeled `while` frame: the copy
   is born, freed at frame end, then aliased (the write-back `Dup`) and moved
   out. Balanced, and accepted before the rule. *)
Example cert_xc_resurrect_rejects : check_xc "idam"%string = false.
Proof. reflexivity. Qed.
(* its two-level twin (`d.inner.xs`): both record levels copied, both freed. *)
Example cert_xc_resurrect_two_level_rejects : check_xc "iiddam"%string = false.
Proof. reflexivity. Qed.
(* the probe reaches into loop bodies and branch arms. *)
Example cert_xc_resurrect_in_loop_rejects : check_xc "id(ad)"%string = false.
Proof. reflexivity. Qed.
Example cert_xc_resurrect_in_branch_rejects : check_xc "id{ad|ad}"%string = false.
Proof. reflexivity. Qed.
(* a LIVE owned object may still be aliased, in a loop or an arm. *)
Example cert_xc_owned_alias_live_accepts : check_xc "i(ad)a{d|d}d"%string = true.
Proof. reflexivity. Qed.
(* a borrowed PARAM's line (no top-level birth): the caller's reference is
   uncounted, so re-aliasing at 0 stays legal — judged exactly as before. *)
Example cert_xc_param_realias_accepts : check_xc "amamam"%string = true.
Proof. reflexivity. Qed.
Example cert_xc_param_slot_accepts : check_xc "(id)am"%string = true.
Proof. reflexivity. Qed.

(* ─── THE ARM SHARE-OUT (#4278, format v7: the `s` byte) ───────────────────
   A stdlib arm may answer, on one path, with its ARGUMENT block as its owned
   result: `if c { rc_inc(p); p } else { fresh block }`. The result's line is
   born by the consumer's `i` on every path; on the share path that credit is
   the arm's `rc_inc` on `p`, which the argument's line records as `s`. *)

From Stdlib Require Import Lia.

(* What `s` stands for on the argument's line: the share +1 (`a`) and the
   credit moved into the arm's result (`m`). *)
Definition expand_op (o : Op) : list Op :=
  match o with
  | ShareOut => [Alias; MoveOut]
  (* a take is the receiving account's +1 (the slot's −1 is the other
     account's: `take_handover_sound`); a probe is a liveness read. *)
  | Take => [Inc]
  | Probe => [Borrow]
  | _ => [o]
  end.

Definition expand (ops : list Op) : list Op := flat_map expand_op ops.

(* THE SHARE IS COUNTED: from any count the fold reaches (never below 0), the
   line with its share-outs runs exactly as the line with each one spelled as
   the share and the move it is. So every soundness theorem about `check`
   holds of the spelled-out run, and `s` adds no new kind of step. *)
Lemma exec_expand :
  forall ops rc, 0 <= rc -> exec (expand ops) rc = exec ops rc.
Proof.
  unfold expand.
  induction ops as [| o rest IH]; intros rc Hrc; [reflexivity |].
  destruct o; simpl.
  - apply IH. lia.
  - apply IH. lia.
  - destruct (rc <=? 0) eqn:E; [reflexivity |]. apply Z.leb_gt in E. apply IH. lia.
  - destruct (rc <=? 0) eqn:E; [reflexivity |]. apply Z.leb_gt in E. apply IH. lia.
  - reflexivity.
  - destruct (rc <=? 0); [reflexivity | apply IH; exact Hrc].
  - assert (E : (rc + 1 <=? 0) = false) by (apply Z.leb_gt; lia).
    rewrite E. replace (rc + 1 - 1) with rc by lia. apply IH. exact Hrc.
  - apply IH. lia.
  - destruct (rc <=? 0); [reflexivity | apply IH; exact Hrc].
  - destruct (rc <=? 0) eqn:E; [reflexivity |]. apply Z.leb_gt in E. apply IH. lia.
Qed.

Theorem check_expand : forall ops, check (expand ops) = check ops.
Proof.
  intro ops. unfold check, run. rewrite exec_expand; [reflexivity | lia].
Qed.

(* THE ARGUMENT'S BALANCE IS UNCHANGED: a share-out moves no count on the
   argument's own line. *)
Theorem share_out_neutral : forall rc, exec [ShareOut] rc = Some rc.
Proof. reflexivity. Qed.

Theorem share_out_sound :
  forall ops, check ops = true ->
    no_double_free (expand ops) /\ no_leak (expand ops).
Proof. intros ops H. apply check_sound. rewrite check_expand. exact H. Qed.

(* THE HAND-OVER. After a share-out, one block is held by two accounts: the
   argument's line `p` and the arm result's line `res` (its `i` is the +1 the
   share made). The block's count is the caller's `base` plus both accounts,
   and the runtime performs their events in SOME interleaving. Accepted (and
   free of `Reuse`, a uniqueness claim no shared block can make), every
   interleaving never faults and returns the block to `base`: the result owns
   the shared credit and releases it exactly once, and the argument's credit
   is what it was. *)
Fixpoint no_reuse_ops (ops : list Op) : bool :=
  match ops with
  | [] => true
  | Reuse :: _ => false
  | _ :: rest => no_reuse_ops rest
  end.

Inductive Interleave : list Op -> list Op -> list Op -> Prop :=
  | IL_nil : Interleave [] [] []
  | IL_l : forall o a b w, Interleave a b w -> Interleave (o :: a) b (o :: w)
  | IL_r : forall o a b w, Interleave a b w -> Interleave a (o :: b) (o :: w).

Lemma interleave_exec :
  forall a b w, Interleave a b w ->
    forall r1 r2 k x y, 0 <= r1 -> 0 <= r2 -> 0 <= k ->
      no_reuse_ops a = true -> no_reuse_ops b = true ->
      exec a r1 = Some x -> exec b r2 = Some y ->
      exec w (r1 + r2 + k) = Some (x + y + k).
Proof.
  intros a b w HI. induction HI; intros r1 r2 k x y H1 H2 Hk Na Nb Ea Eb.
  - simpl in *. injection Ea as <-. injection Eb as <-. reflexivity.
  - destruct o; simpl in Ea, Na |- *.
    + replace (r1 + r2 + k + 1) with (r1 + 1 + r2 + k) by lia. apply IHHI; auto; lia.
    + replace (r1 + r2 + k + 1) with (r1 + 1 + r2 + k) by lia. apply IHHI; auto; lia.
    + destruct (r1 <=? 0) eqn:E; [discriminate |]. apply Z.leb_gt in E.
      assert (E2 : (r1 + r2 + k <=? 0) = false) by (apply Z.leb_gt; lia). rewrite E2.
      replace (r1 + r2 + k - 1) with (r1 - 1 + r2 + k) by lia. apply IHHI; auto; lia.
    + destruct (r1 <=? 0) eqn:E; [discriminate |]. apply Z.leb_gt in E.
      assert (E2 : (r1 + r2 + k <=? 0) = false) by (apply Z.leb_gt; lia). rewrite E2.
      replace (r1 + r2 + k - 1) with (r1 - 1 + r2 + k) by lia. apply IHHI; auto; lia.
    + discriminate Na.
    + destruct (r1 <=? 0) eqn:E; [discriminate |]. apply Z.leb_gt in E.
      assert (E2 : (r1 + r2 + k <=? 0) = false) by (apply Z.leb_gt; lia). rewrite E2.
      apply IHHI; auto.
    + apply IHHI; auto.
    + replace (r1 + r2 + k + 1) with (r1 + 1 + r2 + k) by lia. apply IHHI; auto; lia.
    + destruct (r1 <=? 0) eqn:E; [discriminate |]. apply Z.leb_gt in E.
      assert (E2 : (r1 + r2 + k <=? 0) = false) by (apply Z.leb_gt; lia). rewrite E2.
      apply IHHI; auto.
    + destruct (r1 <=? 0) eqn:E; [discriminate |]. apply Z.leb_gt in E.
      assert (E2 : (r1 + r2 + k <=? 0) = false) by (apply Z.leb_gt; lia). rewrite E2.
      replace (r1 + r2 + k - 1) with (r1 - 1 + r2 + k) by lia. apply IHHI; auto; lia.
  - destruct o; simpl in Eb, Nb |- *.
    + replace (r1 + r2 + k + 1) with (r1 + (r2 + 1) + k) by lia. apply IHHI; auto; lia.
    + replace (r1 + r2 + k + 1) with (r1 + (r2 + 1) + k) by lia. apply IHHI; auto; lia.
    + destruct (r2 <=? 0) eqn:E; [discriminate |]. apply Z.leb_gt in E.
      assert (E2 : (r1 + r2 + k <=? 0) = false) by (apply Z.leb_gt; lia). rewrite E2.
      replace (r1 + r2 + k - 1) with (r1 + (r2 - 1) + k) by lia. apply IHHI; auto; lia.
    + destruct (r2 <=? 0) eqn:E; [discriminate |]. apply Z.leb_gt in E.
      assert (E2 : (r1 + r2 + k <=? 0) = false) by (apply Z.leb_gt; lia). rewrite E2.
      replace (r1 + r2 + k - 1) with (r1 + (r2 - 1) + k) by lia. apply IHHI; auto; lia.
    + discriminate Nb.
    + destruct (r2 <=? 0) eqn:E; [discriminate |]. apply Z.leb_gt in E.
      assert (E2 : (r1 + r2 + k <=? 0) = false) by (apply Z.leb_gt; lia). rewrite E2.
      apply IHHI; auto.
    + apply IHHI; auto.
    + replace (r1 + r2 + k + 1) with (r1 + (r2 + 1) + k) by lia. apply IHHI; auto; lia.
    + destruct (r2 <=? 0) eqn:E; [discriminate |]. apply Z.leb_gt in E.
      assert (E2 : (r1 + r2 + k <=? 0) = false) by (apply Z.leb_gt; lia). rewrite E2.
      apply IHHI; auto.
    + destruct (r2 <=? 0) eqn:E; [discriminate |]. apply Z.leb_gt in E.
      assert (E2 : (r1 + r2 + k <=? 0) = false) by (apply Z.leb_gt; lia). rewrite E2.
      replace (r1 + r2 + k - 1) with (r1 + (r2 - 1) + k) by lia. apply IHHI; auto; lia.
Qed.

Theorem share_handover_sound :
  forall p res w base,
    check p = true -> check res = true ->
    no_reuse_ops p = true -> no_reuse_ops res = true ->
    Interleave p res w -> 0 <= base ->
    exec w base = Some base.
Proof.
  intros p res w base Hp Hr Np Nr HI Hb.
  unfold check, run in Hp, Hr.
  destruct (exec p 0) as [x |] eqn:Ep; [| discriminate].
  destruct (exec res 0) as [y |] eqn:Er; [| discriminate].
  apply Z.eqb_eq in Hp. apply Z.eqb_eq in Hr. subst x y.
  pose proof (interleave_exec p res w HI 0 0 base 0 0) as H.
  replace (0 + 0 + base) with base in H by lia.
  apply H; auto; lia.
Qed.

(* The kernel's own verdicts on the new byte, over real certificate bytes. *)
(* a borrowed argument's share-out, and the result's line releasing it. *)
Example cert_xc_share_out_accepts : check_xc "s
id"%string = true.
Proof. reflexivity. Qed.
(* an owned argument shares out while live, then releases its own credit. *)
Example cert_xc_share_out_owned_accepts : check_xc "isd
id"%string = true.
Proof. reflexivity. Qed.
(* NEGATIVE: the share is the RESULT's credit, never the argument's — a line
   that releases what it shared out releases a credit it does not hold. *)
Example cert_xc_share_out_no_credit_rejects : check_xc "sd"%string = false.
Proof. reflexivity. Qed.
(* NEGATIVE: a share-out of an owned object after its last release is a
   use-after-free (the #3229 probe guards `s` as it guards `a`). *)
Example cert_xc_share_out_dead_rejects : check_xc "ids"%string = false.
Proof. reflexivity. Qed.
(* NEGATIVE: the result that received the share must release it. *)
Example cert_xc_share_out_result_leak_rejects : check_xc "s
i"%string = false.
Proof. reflexivity. Qed.
(* The hand-over, concretely: the argument held at 1 by the caller, the share
   (the result's `i`) released inside the argument's own window. *)
Example share_handover_example :
  exec [Borrow; Inc; ShareOut; Dec] 1 = Some 1.
Proof. reflexivity. Qed.

(* ─── THE TAKE (ADR-0034, format v8: the `k` and `u` bytes) ───────────────
   A take-or-retain site: the container X is probed (`u` on X's line), and
   per element EITHER the slot's credit moves to the receiving line (`k`, the
   slot cleared, no RC instruction) OR the receiver takes an `rc_inc` of its
   own (`a`). Both arms leave the receiver holding one credit, so the site is
   an ordinary agreeing `CBranch` and the code after it is checked ONCE. *)

(* THE HAND-OVER. Before the take the element's count is `base` (its other
   holders) plus the slot's one credit. The slot's account gives that credit
   up (`MoveOut`, the cleared slot) and the receiving line takes it (`Take`),
   in some interleaving with the receiver's later events. Accepted (and
   reuse-free), every interleaving never faults and returns the element to
   `base`: the moved credit is released exactly once, by the receiver. *)
Theorem take_handover_sound :
  forall res w base,
    check res = true -> no_reuse_ops res = true ->
    Interleave [MoveOut] res w -> 0 <= base ->
    exec w (1 + base) = Some base.
Proof.
  intros res w base Hr Nr HI Hb.
  unfold check, run in Hr.
  destruct (exec res 0) as [y |] eqn:Er; [| discriminate].
  apply Z.eqb_eq in Hr. subst y.
  pose proof (interleave_exec [MoveOut] res w HI 1 0 base 0 0) as H.
  replace (1 + 0 + base) with (1 + base) in H by lia.
  replace (0 + 0 + base) with base in H by lia.
  apply H; try lia; auto.
Qed.

(* the strchurn site, per element: take-or-retain, then the body moves the
   element into a consuming callee — and its borrowing-callee twin. *)
Example cert_xc_take_site_accepts : check_xc "{k|a}m"%string = true.
Proof. reflexivity. Qed.
Example cert_xc_take_site_whole_accepts : check_xc "{km|am}"%string = true.
Proof. reflexivity. Qed.
Example cert_xc_take_site_borrowing_accepts : check_xc "{k|a}d"%string = true.
Proof. reflexivity. Qed.
(* the container: born, probed, released after the loop. *)
Example cert_xc_probe_drain_accepts : check_xc "iud"%string = true.
Proof. reflexivity. Qed.
Example cert_xc_probe_loop_accepts : check_xc "i(u)m"%string = true.
Proof. reflexivity. Qed.
(* NEGATIVE: a take arm that does not take — the arms disagree. *)
Example cert_xc_take_missing_rejects : check_xc "{k|}m"%string = false.
Proof. reflexivity. Qed.
(* NEGATIVE: a taken credit released twice. *)
Example cert_xc_take_double_release_rejects : check_xc "kdd"%string = false.
Proof. reflexivity. Qed.
(* NEGATIVE: a probe of a container this holder owns no credit on. *)
Example cert_xc_probe_unowned_rejects : check_xc "u"%string = false.
Proof. reflexivity. Qed.
Example cert_xc_probe_after_release_rejects : check_xc "idu"%string = false.
Proof. reflexivity. Qed.
(* NEGATIVE (the drain rule): after a probe, a read, a share, a share-out, a
   second release, a new birth or a take on the container's line. *)
Example cert_xc_drain_read_rejects : check_xc "iubd"%string = false.
Proof. reflexivity. Qed.
Example cert_xc_drain_share_rejects : check_xc "iuadd"%string = false.
Proof. reflexivity. Qed.
Example cert_xc_drain_shareout_rejects : check_xc "iusd"%string = false.
Proof. reflexivity. Qed.
Example cert_xc_drain_two_releases_rejects : check_xc "iiudd"%string = false.
Proof. reflexivity. Qed.
Example cert_xc_drain_birth_rejects : check_xc "iuidd"%string = false.
Proof. reflexivity. Qed.
Example cert_xc_drain_in_branch_rejects : check_xc "i{u|}d"%string = false.
Proof. reflexivity. Qed.
Example cert_xc_drain_mixed_loop_rejects : check_xc "i(ub)d"%string = false.
Proof. reflexivity. Qed.
(* NEGATIVE: the leak — probed, never released. *)
Example cert_xc_probe_leak_rejects : check_xc "iu"%string = false.
Proof. reflexivity. Qed.

(* #4753: `r` is refused on every ownership line. Each of these BALANCED and
   was accepted, though another holder still held the block it reused: the
   moved alias (`iamr`), the caller of a borrowed parameter (`ar`, `amar`),
   a loop's moved shares (`i(am)r`), the arm result of a share-out (`isr`),
   a second credit moved out (`iiamdr`). The control `ir` is refused too:
   uniqueness is a runtime fact (the probe `u` and the shipped test), never
   a line count. *)
Example cert_xc_reuse_moved_alias_rejects : check_xc "iamr"%string = false.
Proof. reflexivity. Qed.
Example cert_xc_reuse_borrowed_param_rejects : check_xc "ar"%string = false.
Proof. reflexivity. Qed.
Example cert_xc_reuse_param_moved_rejects : check_xc "amar"%string = false.
Proof. reflexivity. Qed.
Example cert_xc_reuse_loop_rejects : check_xc "i(am)r"%string = false.
Proof. reflexivity. Qed.
Example cert_xc_reuse_share_out_rejects : check_xc "isr"%string = false.
Proof. reflexivity. Qed.
Example cert_xc_reuse_second_credit_rejects : check_xc "iiamdr"%string = false.
Proof. reflexivity. Qed.
Example cert_xc_reuse_control_rejects : check_xc "ir"%string = false.
Proof. reflexivity. Qed.

(* AXIOM AUDIT (the "Print Assumptions ⊆ standard" gate). Soundness must rest on
   nothing but the Coq kernel — no admits, no extra axioms. Expected output:
   "Closed under the global context". *)
Print Assumptions check_sound.
Print Assumptions check_line_unroll_sound.
Print Assumptions check_cert_lc_sound.
Print Assumptions check_clc_unroll_sound.
Print Assumptions check_bc_unroll_sound.
Print Assumptions check_xc_unroll_sound.
Print Assumptions check_line_abort_sound.
Print Assumptions check_xc_abort_sound.
Print Assumptions check_xc_no_resurrection.
Print Assumptions check_line_prefix_safe.
Print Assumptions check_all_sound.
Print Assumptions check_cert_sound.
Print Assumptions check_reuse_sound.
Print Assumptions check_all_reuse_sound.
Print Assumptions check_cert_reuse_sound.
Print Assumptions exec_expand.
Print Assumptions share_out_sound.
Print Assumptions share_handover_sound.
Print Assumptions check_xc_drained_sound.
Print Assumptions take_handover_sound.
