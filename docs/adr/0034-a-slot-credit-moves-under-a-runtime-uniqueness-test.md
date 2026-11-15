# ADR-0034: A slot's credit may move out under a runtime uniqueness test — the take-or-retain site joins both arms at one ownership state, so the code after it is emitted and witnessed once

- **Status**: Accepted (2026-11-15). The owner ruled that this, not the doubled body, is the fix for #4612's wasm `list.map` fast path, and answered ○ to all nine [Owner decisions](#owner-decisions-). Accepted is the design; nothing of it is implemented yet (see the implementation plan in D10).
- **Date**: 2026-11-15
- **Scope**: the ownership witness alphabet (format v8: two new letters), `proofs/OwnershipChecker.v` and its dependants, a new `proofs/UniqueTake.v`, `crates/almide-verify` (ownership), `crates/almide-anchor-check` (rc), the structural recorder (`crates/almide-wasm/src/witness*.rs`), and the emitter's take sites, starting with `lower_list_map` (`list.rs`, `list_take.rs`).
- **Related**:
  - #4612 (strchurn: `string.split(s, ",") |> list.map((p) => string.to_upper(string.trim(p)))`), PR #4743 / merge `ddb96f5d3b` (the fast path, commit `49e2f2c4b5`), branch `fix-wall-strchurn` commit `3f6a5de073` (the doubled body, rejected), branch `revert-wasm-map-take` (the v0.70.0 revert).
  - #4277 / #4278 (the RC byte check; the `s` share-out byte, the precedent this follows), #3229 (`guard_line`), #3406 (dying moves and the judged spread take), #1219 (the in-place map judge).
  - ADR-0013 (String) copy-on-write, `proofs/CowSafety.v` (MakeUnique), ADR-0033 (allocation families; its `list.map` family assumes a uniquely owned source).

## Context

### What the fast path does

`lower_list_map` over a source that is an **owned temporary** (the arm releases it after the loop) whose element type is droppable, when the callback reads its parameter exactly once as an argument (`list_take::sole_call_arg_read`):

```
;; once, before the loop (list_take_open)
local.get X ; global.get $line_end ; i32.ge_u          ;; X is a heap block, not a pool static
local.get X ; i32.load offset=RC ; i32.const 1 ; i32.eq
i32.and ; local.set F                                  ;; F = "X is unique"
block loop
  p := X.slot[i]                                       ;; hof_elem_into
  local.get F
  if   X.slot[i] := 0                                  ;; take: the slot's credit moves to p
  else local.get p ; call $rc_inc                      ;; retain: p gets a credit of its own
  end
  <callback body>                                      ;; p is OWNED on both paths; its one read is a MOVE
  ...
end end
local.get X ; call $release                            ;; walks X's slots; a 0 slot is empty
```

On the unique path `string.trim` meets its argument at rc 1 and works in place; before, it met it at rc 2 and copied it. That is the whole #4612 win.

### Why it broke the wall

The structural witness records one event per RC instruction the emitter emits, and `almide-anchor-check` matches every `rc_inc` / release call of a certified frame against those events (#4277). The callback parameter of an inlined HOF is recorded as a **view** (`witness_callback_open` → `param_borrowed`), and the moving read records nothing (`witness_module_arg`, the `ArgMode::Retain if … is_moving` arm). So:

- the else arm's `call $rc_inc` had no event — **RC REJECT** in `proofs/structural-wall.sh`;
- the then arm's credit transfer had no event either — invisible, so nothing checked that the null store is there.

### The two answers already on the table

| | body | size (21 rows, measured) | witness |
|---|---|---|---|
| revert (no fast path) | once | baseline | as before |
| doubled body (`3f6a5de073`) | once per arm: each arm lowered and witnessed separately (take arm: move; retain arm: the old lent element) | **+859 B** over the single-body shape | existing alphabet |
| this ADR | once | **0 B** over the single-body shape #4743 shipped (the witness is metadata) | two new letters |

The doubled body costs a copy of every callback body at every take site, and every later in-place fast path (below) would double its user code the same way. The owner rejected it.

### What the existing kernel can and cannot say

`OwnershipChecker.v` folds one integer per **holder account** of one block (`witness_paths.rs`: "each line is one holder's account of one block. The block's count is the sum of its holders' counts"). Its `CBranch thenb elseb` already accepts a one-shot branch whose arms reach the **same** count from the entry count. So the element's account `{?|a}` followed by the body's events is already the right *shape*: the take arm and the retain arm both leave `p` holding one credit, the join is equal, and the body is checked once after it.

What is missing is a **name** for the take arm's +1. It is not an allocation (`i`) and not an `rc_inc` (`a`): it is a credit moving from one holder (the slot) to another (the frame), with no RC instruction at all. Spelling it `i` would make the kernel accept `{i|a}m` today — but an `i` is held to nothing in the bytes, so a lost null store (a double free) would pass every gate. That is a new trusted class; the anchor check's own rule is that a new trusted class is an owner decision, not a checker edit. This ADR makes the take a checked event instead.

## Prior art (what we adopt)

- **Perceus** (Reinking, Xie, de Moura, Leijen, PLDI 2021, "Perceus: Garbage Free Reference Counting with Reuse"), §2 *drop specialization*: when a matched cell `x` dies, `drop(x)` after `dup` of its fields is specialised to `if is-unique(x) then free(x) else { dup(fields); decref(x) }`; the fields are owned on both paths and **the continuation is shared**. This is exactly our site: the unique path moves the field's credit out (we null the slot instead of freeing the cell, because the spine is freed after the loop), the shared path dups. **Adopted**: the branch covers only the prologue; both arms leave the same ownership state; the body is after the join.
- **Koka FBIP / frame-limited reuse** (Lorenzen & Leijen, ICFP 2022, "Reference Counting with Frame Limited Reuse"; Lorenzen, Leijen, Swierstra, ICFP 2023, "FP²: Fully in-Place Functional Programming"): in-place updates are justified by a uniqueness test at run time plus a static discipline that the tested value is owned and not used again. **Adopted**: our soundness argument is the same split — the *static* fact (the frame holds a credit on `X`, and nothing reads `X` again but the site) is checked from the witness, the *dynamic* fact (`rc X = 1`) is the shipped test. Neither alone implies exclusivity; together they do.
- **Lean 4 reset/reuse** (Ullrich & de Moura, IFL 2019, "Counting Immutable Beans"; `src/Lean/Compiler/IR/ExpandResetReuse.lean`): `reset`/`reuse` are expanded into an `isShared` test whose fast and slow paths **each get a copy of the continuation**. That is the doubled body. **Not adopted**: it trades code size for not having to prove the join, which our checker can prove.
- **Verified RC**: Perceus' λ¹ calculus and FP² come with paper soundness proofs of garbage-freedom/in-place safety; we know of no mechanised kernel checking a *per-build witness* of such a site. Almide's own `CowSafety.v` (MakeUnique: `rc = 1` keeps the block, else clone) is the closest in-house precedent and the model this ADR's `UniqueTake.v` extends.

References need their URLs verified before Accepted (see [References](#references)).

## Decision

The owner answered ○ to all nine questions in [Owner decisions](#owner-decisions-) on 2026-11-15: the shared path retains; two letters `k` / `u` and no new `CertItem`; owned temporaries first; the drain rule in the kernel and in the anchor check; the judged spread take recorded as `k` in a follow-up; `UniqueTake.v` lands with the kernel letters; certificate v8 bumps the bundle version; the fast path returns after the v0.70.0 final; and the kernel's existing `Reuse` rule gets its own issue. D1–D9 below are written as so decided.

### D1. Two letters, certificate format v8

The ownership alphabet (`Op` in `OwnershipChecker.v`, `parse_byte`) gains:

| byte | `Op` | on whose line | fold | ground fact it carries |
|---|---|---|---|---|
| `k` / `K` | `Take` | the receiving holder's (the element `p`) | **+1**, like `Inc` | the credit was MOVED out of a container slot that was cleared in the same step; no RC instruction exists for it |
| `u` / `U` | `Probe` | the container's (`X`) | **+0**, faults at count ≤ 0, like `Borrow` | the shipped code tested `rc X = 1` here, and this holder owns a credit on `X` when it does |

No `CertItem` changes. The take-or-retain site is an ordinary one-shot `CBranch` on the element's line, `{k|a}`, followed by the body's events once. `a` is the existing letter: an `rc_inc` read through `p`.

A v7 certificate contains neither byte, so it parses exactly as before. A v8 certificate read by a v7 checker has `k` skipped (`{|a}` → arms disagree → reject: fails closed) but `u` skipped too (fails open on the drain rule below), so the bundle format version is bumped and every v7 reader refuses a v8 bundle.

### D2. The drain rule on the container's line

A holder that probed `X` gives `X` up: on any line, **after the first `u`, every later op is a `u`, or the line's one terminal release (`d` or `m`), and nothing follows that release.** A `b`, `a`, `s`, `i`, `k` or second release after a `u` rejects the line. (v1 accepts the flat form `…u…u d` / `…u m` only — a branch or loop item after the first `u` other than `(u)` rejects; widen later if a site needs it.)

This is what makes the null slots unobservable inside the frame: once the frame has started taking, it reads `X` again only to release it.

### D3. Where each obligation is discharged

| obligation | discharged by | how |
|---|---|---|
| the element account balances on both paths, and the body is checked once against the join | **kernel** (`check_xc`, unchanged `CBranch` rule + `Take` fold) | `{k|a}` arms both reach +1 from 0; the body's ops follow once |
| the frame owns `X` when it probes it | **kernel** (`Probe` guard) | `u` at count ≤ 0 faults |
| the frame does not use `X` after probing, except to release it | **kernel** (D2) and **anchor check** (R5) | witness level and byte level |
| `rc X = 1` ∧ frame owns `X` ⇒ no other holder; the take and the retain both preserve the heap invariant; the release walk frees each element once | **kernel-checked model** (`UniqueTake.v`, new) | theorems T1–T6 below |
| the `k` is a null store into the slot `p` was loaded from, under an `if` that tests the flag of a `u` on `X` | **anchor check** (R1–R4) | instruction shapes and operand chains |
| no RC call of the site is unrecorded | **anchor check** (existing RC rule) | the retain arm's `rc_inc` is the `a`; the release is the `d` |
| the release routine treats a 0 slot as empty | **existing trust**, unchanged | already relied on by the judged spread take (`dying_move.rs` `try_take_slot`, "a moved slot is NULL: a release of nothing") and `map_remove`; restated in `proven-vs-trusted.md`, with T5 proving the model of it |

### D4. Witness syntax (recorder → bundle)

Ownership certificate (per holder line, the bytes the three verdicts fold):

```
X's frame line          i u d           ;; born (the owned temporary), probed, released after the loop
p's activation line     {km|am}         ;; one iteration: take-or-retain, then the body moves p into string.trim
                        {kd|ad}         ;; …or a borrowing callee: p parked and released after the call
```

`witness_paths.rs` already renders a two-path activation as the whole-line branch `{p|q}`; `{k|a}m` and `{km|am}` are the same verdict.

RC anchor line (the `rc <position>|<events>` record `almide-anchor-check` reads; new tokens in bold):

```
… **u<X>@<pos>** … ( @<if>-<else>  **k<p>@<from>-<to>**  |@<if>-<else>  a<p> ) … d<X>
```

- `u<X>@<pos>`: the probe, at byte position `pos` of the frame body.
- `k<p>@<from>-<to>`: the take, held to the store inside that window.
- the site is an **anchored** branch (`(@<if>-<else>`, the existing `witness_branch_open_at` form).

### D5. Anchor-check rules (`crates/almide-anchor-check/src/rc/`, new `take.rs`)

- **R1 (probe shape).** At `pos`: `local.get X; global.get $line_end; i32.ge_u; local.get X; i32.load offset=RC; i32.const 1; i32.eq; i32.and; local.set F`. `F` has exactly this one write in the function.
- **R2 (site binding).** A branch site that holds a `k` is anchored; the instruction before its `if` is `local.get F` for an `F` of R1 whose `X` the site's `k` names through its load; the probe precedes, in the same block sequence, the outermost `block`/`loop` containing the site (so it dominates it). An unanchored `k`, or a `k` in a site that does not test a probe flag, is REJECT.
- **R3 (the take).** The `k<p>` window holds exactly one store, `i32.store` of `i32.const 0`, whose address chain (operands.rs `chain`) is the chain of the load whose value was last written to `p` (same locals `X` and `i`, same stride and offset), with none of `X`, `i`, `p` written between that load and the store. The window holds no call.
- **R4 (the retain).** The else arm is exactly `local.get p; call $rc_inc`, matched by `a<p>` under the existing rule.
- **R5 (confinement).** From the probe to the matched `d<X>`, every read of local `X` is one of: the probe's two reads, an element load's address chain, an R3 store's address chain, the loop's count load, the release. `X`'s value never flows into another local, a call argument other than the release, or a stored value.
- **R6 (release).** `d<X>` is matched as today, and is `X`'s last read (after.rs).

R1–R6 are written as one module with its own reinjection drills (Implementation plan, step 6).

### D6. What the Coq proof must extend

**The soundness property, stated for an accepted frame on every run** (the kernel verdict, the model theorems and the anchor rules together):

- **S1 no leak** — every credit the frame takes, the `k`/`a` credit on each element included, is released or moved on exactly once.
- **S2 no double free** — a credit moved out by `k` is not released again by `X`'s release (the slot is 0, R3; the walk skips 0, T5); on the retain path the slot keeps its credit and the frame has a new one (two credits, two releases).
- **S3 no use after free** — `X` stays live from the probe to its release (the frame's credit, `Probe` guard and D2); the element is live while the body reads it (it holds a credit on both paths).
- **S4 no dangling borrow / value semantics** — on the take path the cleared slot is unobservable: the frame holds a credit on `X` (`u` guard) and `rc X = 1` (the test), so no other holder exists (T1), and the frame itself never reads `X` again but to release it (D2, R5). On the retain path nothing observable changes.
- **S5 bytes ⇔ witness** — every RC call of the site is one recorded event (R4, R6), every `k` is the store R3 names, every `u` the test R1 names, every site the `if` R2 names.

**Changes, file by file** (precedent for size: the `ShareOut` byte, commit `d52887ec7f`, was +205 lines over 8 `.v` files):

| file | change | kind |
|---|---|---|
| `OwnershipChecker.v` | `Op` gains `Take`, `Probe`; `exec`: `Take` = `rc + 1`, `Probe` = `if rc <=? 0 then None else rc`; `parse_byte` gains `k`/`u`; one more case in every `destruct o` proof (`exec_app`, `reuses_unique`, `exec_ok_reuses_unique`, `exec_expand`, `interleave_exec`, the guard lemmas); `guard_op Take = [Take]`, `guard_op Probe = [Probe]` (a take is not a revival: the block is live in its slot); `expand_op Take = [Inc]`, `expand_op Probe = [Borrow]`, so `exec_expand` / `check_expand` show the letters add no new kind of step; **new** `drained : list CertItem -> bool` (D2) as a conjunct of `check_xc`, with `check_xc_drained_sound` (on the unrolled run, after the first `Probe` only `Probe`s and at most one final `Dec`/`MoveOut`); **new** `take_handover_sound` (the slot account's −1 and the receiving line's `Take` interleave without fault and leave the block's count where it was — `interleave_exec` with `Take` expanded); Examples: `{k\|a}m` and `iud` accept; `{k\|}m`, `kdd`, `u`, `iubd`, `iuad`, `iudd`, `iu` reject; `Print Assumptions` for every new theorem | mechanical + 2 new lemmas |
| `RuntimeModel.v` | `step_mem Take` = the account gains one (`rt_inc` on the line's account cell); `Probe` = `rt_borrow`; two cases in `mrun_tracks_exec` | mechanical |
| `CallModes.v`, `Termination.v`, `ALS.v`, `Translation.v`, `MemorySafe.v` | one or two arms each, as `ShareOut` needed (`Translation`: `Take` ↦ the null store, `Probe` ↦ the rc test) | mechanical |
| **`UniqueTake.v` (new)** | the runtime model of the site: blocks with a count and slots; holder accounts (the frame, every live slot, everything else); the invariant HS: for every heap block, count = Σ accounts. **T1** `probe_exclusive`: HS ∧ frame account on `X` ≥ 1 ∧ count `X` = 1 ⇒ every other account on `X` is 0. **T2** `take_preserves`: under T1's premises, clearing `X.slot[j]` (= `e`) and adding 1 to the frame's account on `e` preserves HS and leaves `e`'s count unchanged. **T3** `retain_preserves`: `rt_inc e` plus 1 to the frame's account on `e` preserves HS. **T4** `arms_join`: T2's and T3's frame accounts are equal (the single-body justification). **T5** `release_walk_once`: releasing `X` from count 1, with the walk skipping 0 and statics, decrements each element by exactly the number of slots that still hold it. **T6** `take_unobservable`: under T1, no account other than the frame's can read `X.slot[j]`. Built on `CowSafety.v`'s `below_fresh` style; `Print Assumptions` closed | new, model-level |

**No new axioms.** `Extract.v` re-extracts the checker; `build-checker.sh` gains the new `Print Assumptions` lines for the axiom audit. Estimated size: ~150 lines of mechanical cases, ~120 lines for the drain rule and hand-over, ~300 lines for `UniqueTake.v` — about 550–650 lines, roughly three times the `ShareOut` change. The probe's instruction tree is bound to the model by the anchor check (R1), not by a `WasmRcDec.v`-style tree proof; that proof is optional later work (Owner decision 6).

### D7. Emitter changes for `list.map`

- Restore `list_take_open` / the take-or-retain prologue of `49e2f2c4b5` unchanged (single body).
- New recorder hooks (`witness_hooks*.rs`, `witness_rest.rs`):
  - `witness_uniq_probe(X, pos)` after `list_take_open`: logs `u` on `X`'s line and `u<X>@<pos>` to the RC channel. `X` must be a line the recorder holds an owned credit on (the temporary's `i`), else poison.
  - the prologue opens an anchored site (`witness_branch_open_at`), records `k<p>` with the store's window in the then arm and `a<p>` in the else arm, closes the site.
  - `witness_callback_open` binds `p` as an **owner** on the taken route (not `param_borrowed`), so the moving read records `m` (or the borrowing callee's park/release records `d`) on `p`'s activation line.
- The site is taken only when (a) the source is an owned temporary (`rc_owned_result(xs)`), (b) the callback reads `p` once as an argument (unchanged), (c) the body has no exit, abort or loop jump (D2 v1 accepts a flat container line only). Otherwise the lent path, as before.
- `lower_list_map_fnvalue` (call_indirect) is unchanged: the closure ABI lends its parameter.

### D8. Generality: the sites that would use the same primitive

The primitive is "**move a credit out of a slot of a block this frame owns and has tested unique, else retain it; continue once**". Candidate emitter sites (each its own follow-up PR, each with its own fixtures):

| site | file | the moved credit goes to |
|---|---|---|
| `list.map` over a consumed source (this ADR) | `list.rs` `lower_list_map` | the callback's parameter |
| `list.filter` / `list.partition` over a consumed source | `list.rs` `lower_list_filter`, `list_flat.rs` `lower_list_partition` | the result spine (today `rc_share_guard` incs each kept element) |
| `list.filter_map` / `list.flat_map` | `list_flat.rs` | the callback's parameter |
| `list.fold` / `list.reduce` with a consuming step | `list.rs` fold, `list_comb.rs` `lower_list_reduce` | the step's element parameter |
| `list.find` / `min` / `max` over a consumed source | `list.rs` `lower_list_find`, `list_order.rs` | the one returned element (take one slot) |
| `for x in <consumed list>` | `stmts.rs` `lower_forin` | the loop variable |
| `match` on a dying variant / tuple / record | patterns (`emit_pattern_binds`) | the binders — Perceus' drop specialization proper |
| field read of a dying record | `data.rs` field projection | the read value |
| `map.update(m, k, f)` in place | `map_inplace.rs` | `f`'s argument: the old value moves out of the entry (after the existing judge, so the block is unique without a `u`; see below) |

Two related shapes **do not** need it: string append when unique and `list.set` when unique are already the copy-on-write judge (`runtime_alloc_cow.rs`, `map_inplace.rs`): the judge returns a unique block on both paths, so ownership never splits. After a judge, a slot take is unconditional; the existing judged spread take (`dying_move.rs` `try_take_slot`) is such a take, and recording it as `k` (held to a judge result instead of a probe flag — a second R2 form) retires its implicit trust. That is a follow-up (Owner decision 5).

### D9. Cost

Per take site, from the instruction sequences above:

| | bytes per site | per element, unique source | per element, shared source |
|---|---|---|---|
| no fast path | 0 | `rc_inc` on `p` at the call (Retain), the callee copies (alloc + copy), the source release later frees the original | same |
| doubled body | ≈ 40 B (probe + branch) **+ one more copy of the body** (+859 B over 21 rows ≈ +41 B per row on the size corpus; grows with the body) | take: 2 (flag test) + 7 (null store); no inc, no copy | 2 (flag test) + the lent path |
| **this ADR** | ≈ 40 B: probe 16 B (once, before the loop) + prologue ≈ 24 B — the bytes #4743 already shipped | 2 + 7 instructions, no inc, no copy, no extra free | 2 (flag test) + `local.get p; call $rc_inc` — one inc, as the no-fast-path route already does at the call |

So against the doubled body this ADR saves the whole body copy (−859 B on the measured 21 rows, more on bigger bodies) at the same instruction count; against no fast path it adds ≈ 40 B per site and two instructions per element on a shared source, and saves an inc, an allocation, a copy and a free per element on a unique one. The witness costs no shipped bytes (it is bundle metadata). Exact numbers are step 0 of the plan.

### D11. The kernel refuses `r` (#4753)

Owner decision 9 found a hole, verified 2026-11-15. The extracted checker, `almide-verify` and the Rocq kernel all accepted `iamr`, `ar`, `amar`, `i(am)r`, `isr` and `iiamdr`. Each of these reuses in place a block that another holder still holds. No producer emits `r` on an ownership line, so the hole was unreachable; it is #4753.

**Why the existing theorem did not catch it.** `check_reuse_sound` proves what it states: on an accepted line, every `Reuse` acts where *that line's* count is 1. The gap was in what "count" meant. Brick 1 modelled one stream per object, and `RuntimeModel.mrun_tracks_exec` binds a line to the block's rc cell starting from 0. Both make the line's count the block's count, which is the same as assuming the line is the block's only holder. That assumption stopped holding once lines became per-holder accounts:

- a move-out (`m`) hands a credit to a container that the line no longer counts;
- a borrowed parameter's line starts at 0 with the caller's credit uncounted;
- a share-out (`s`) puts a credit on the result's line.

The theorem stayed true, but its reading as "uniqueness" did not. Two places already worked around it without saying so: `share_handover_sound` required `no_reuse_ops`, and the helper-signature and native-call checkers require reuse-free lines (`CallModes.reuse_body_not_shift_safe`). `check_xc` did not.

**The new rule.** `r` is refused on every ownership line: `exec` faults on `Reuse`. The kernel has no other way to make a uniqueness claim from a line count. Uniqueness comes only from this ADR's split:

- the `u` Probe's guard: the frame holds a credit;
- the shipped runtime test: `rc X = 1`;
- `UniqueTake.probe_exclusive`, which shows that these two together leave no other holder.

The rule "a `u` must precede an `r` on the same path" would need path state in `exec`. Refusing `r` needs none, and nothing emits `r`. The six rows above, and the former control `ir`, are must-reject Examples in the kernel and rows in `almide-verify`'s test.

### D10. Implementation order

Each step lands green before the next; **[K]** touches the Coq kernel.

0. Measure the three shapes (revert, single body, doubled body): size, alloc and `strchurn` time.
1. **[K]** The letters `k` / `u` in `OwnershipChecker.v` and its dependants (`RuntimeModel.v`, `CallModes.v`, `Termination.v`, `ALS.v`, `Translation.v`, `MemorySafe.v`). Gate: the Trust Spine job, axiom audit clean.
2. **[K]** The drain rule (`drained`, `check_xc_drained_sound`) and `take_handover_sound`.
3. **[K]** `UniqueTake.v`: T1–T6.
4. **[K]** Re-extract; mirror in `almide-verify`; certificate v8 and the bundle version bump. Gate: `proofs/gate.sh` agreement, the seeded differential drawing `k` / `u`.
5. Poisoned certificates for every rejection class.
6. The anchor check R1–R6 with reinjection drills (`scripts/witness-reinject.sh`).
7. The recorder hooks (probe, anchored take site, `p` as an owner).
8. Re-land the single-body fast path (after the v0.70.0 final), with cross-target fixtures for unique, shared and static sources. Gate: `proofs/structural-wall.sh` (both legs), `spec/wasm_cross`.
9. Size, alloc and perf rows: equal to step 0's single-body rows.
10. Docs: `proven-vs-trusted.md`, `proofs/TRUSTED_BASE.md`, the witness format note.

## Alternatives considered

1. **The doubled body** (`3f6a5de073`): each arm lowered and witnessed separately. Rejected by the owner: +859 B on 21 rows, growing with every body and every later site; it is the Lean expansion, chosen there because nothing proves the join.
2. **No fast path** (the v0.70.0 revert): keeps the wasm strchurn loss #4612 was filed for.
3. **Spell the take `i`** with the existing alphabet: the kernel accepts `{i|a}m` today. Rejected: an `i` is held to no instruction, so a lost null store would be a double free every gate accepts — a new trusted class by the back door.
4. **Judge, then take unconditionally** (copy the source spine when shared, as the spread take does): no probe needed, but the shared path pays an allocation, an n-slot copy and n incs where retain pays n incs, and the take still needs `k`. Rejected for loops; kept for spreads, where the copy is needed anyway.
5. **Borrow on the shared path, with a "maybe-owned" element** (the body checked once against a pair of counts, the drop at the end chosen by the flag): needs a new `CertItem` with a two-component `exec`, a new `UnrollsL` constructor and new cases in every `exec_line` lemma; and a move of `p` (the motivating `string.trim` call) is invalid in the borrowed component, so it fails exactly on the case we need. It would save one inc/dec pair per element on the shared path only. Deferred (Falsifier 4).
6. **Hide the branch in a signed runtime helper** `$take_or_retain(X, i) -> elem` (an `rc-sig`: borrows `X`, returns owned): one call per element on the hot path, and the helper's own witness still needs `k` (its take arm has no RC call). Rejected for the loop; it moves the primitive, it does not remove it.

## Consequences

- One ownership primitive covers the in-place fast paths of D8; each new site costs its prologue bytes, not a second body.
- The kernel grows by two letters, one line-shape rule and one hand-over theorem; one new model file. No new axioms; no `CertItem` change.
- The trusted, unproven anchor check grows by R1–R6 (one module, with drills). `proven-vs-trusted.md` gains the row; the "0 slot is empty" walk trust is restated, not added.
- The certificate format goes to v8 and the bundle version is bumped; old `almide-verify` binaries refuse v8 bundles instead of misreading them.
- While the site is limited to owned temporaries (Owner decision 3), dying named variables keep the lent path.

## Falsifier

1. A reinjected bug — the null store dropped, the probe testing another local, `F` rewritten in the loop, an extra read of `X` in the body, the retain `rc_inc` dropped — is accepted by the kernel, the extracted checker, `almide-verify` and the anchor check together. Then R1–R6 are not the boundary they claim; withdraw.
2. The kernel change needs an axiom, or `UniqueTake.v`'s T1 needs a premise the emitter cannot meet. Then the split (static ownership × dynamic count) is wrong; withdraw.
3. The single-body form measures larger than the doubled body on the size corpus (it should be smaller by construction). Then something else changed; re-measure before landing.
4. A later site measures ≥ 5% runtime on a shared-source shape from the inc/dec pair the retain arm takes. Then revisit Alternative 5 for that site.

## Owner decisions (○×)

One decision per question, each with a recommendation. **Answered 2026-11-15: ○ on all nine.**

1. **The shared path RETAINS (an `rc_inc` recorded as `a`), so both paths join at "the element holds one credit" and the body is checked once.** Recommendation: ○. It is Perceus' drop specialization; the borrow-on-shared alternative needs a new branch kind and fails on a consuming callee.
2. **Two new letters, `k` (Take, +1) and `u` (Probe, +0 with a liveness guard), and no new `CertItem`.** Recommendation: ○. The take must be a checked event, not an `i`; the branch shape already exists.
3. **First landing: the source must be an owned temporary (`list.map` of #4612); dying named variables follow later.** Recommendation: ○. A temporary has no name, so R5 confinement holds by construction; a named variable needs the recorder to prove no credit-free view of its slots is live.
4. **Drain rule D2 in the kernel as well as R5 in the anchor check.** Recommendation: ○. It costs one syntactic predicate and gives a witness-level reason the null slots are never read.
5. **Record the existing judged spread take (`try_take_slot`) as `k` held to a judge result, in a follow-up.** Recommendation: ○, after this lands. It retires an implicit trust with the same letter.
6. **`UniqueTake.v` (the model, T1–T6) lands with the kernel letters, before the fast path returns; a `WasmRcDec.v`-style proof of the probe's instruction tree may follow.** Recommendation: ○. The model is what makes `u` mean exclusivity; the tree proof only shrinks R1's trust.
7. **Certificate v8 bumps the bundle format version, so an older `almide-verify` refuses instead of skipping `u`.** Recommendation: ○.
8. **The fast path returns after v0.70.0 (not in it), behind the gates of the implementation plan.** Recommendation: ○. The revert is the v0.70.0 answer.
9. **Open a separate issue on the kernel's existing `Reuse` (`r`) rule:** it takes "frame count = 1" as uniqueness, and `check_xc "iamr"` accepts (alloc, alias, move the alias out, reuse) although the moved credit is a second holder. This ADR does not lean on it (it uses the runtime count). Recommendation: ○ — file it, and check whether any producer still emits `r` on an ownership line. **Done: #4753; closed by D11.**

## References

To verify (URL and verbatim quote) before Status moves to Accepted, per the ADR rule 4:

- A. Reinking, N. Xie, L. de Moura, D. Leijen. *Perceus: Garbage Free Reference Counting with Reuse.* PLDI 2021. Microsoft Research technical report MSR-TR-2020-42. https://www.microsoft.com/en-us/research/publication/perceus-garbage-free-reference-counting-with-reuse/ — §2.3 drop specialization, §2.4 reuse.
- S. Ullrich, L. de Moura. *Counting Immutable Beans: Reference Counting Optimized for Purely Functional Programming.* IFL 2019. https://arxiv.org/abs/1908.05647 — reset/reuse.
- Lean 4 source, `src/Lean/Compiler/IR/ExpandResetReuse.lean` (the fast/slow path expansion). https://github.com/leanprover/lean4
- A. Lorenzen, D. Leijen. *Reference Counting with Frame Limited Reuse.* ICFP 2022. (URL to verify.)
- A. Lorenzen, D. Leijen, W. Swierstra. *FP²: Fully in-Place Functional Programming.* ICFP 2023. (URL to verify.)
- In-repo: `proofs/OwnershipChecker.v` (`CBranch`, `ShareOut`, `share_handover_sound`, `guard_line`), `proofs/CowSafety.v`, `proofs/RuntimeModel.v`, `crates/almide-anchor-check/src/rc.rs` (module doc), `crates/almide-wasm/src/witness_paths.rs` (module doc), `crates/almide-wasm/src/list_take.rs`, `crates/almide-wasm/src/dying_move.rs`.
