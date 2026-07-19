<!-- description: GOAL PROMPT — cert format brick 5: full mode (branch resource-state agreement + closure-env borrow + closure signatures) -->
<!-- done: 2026-07-08 -->
# GOAL PROMPT — cert format brick 5: full mode (branch agreement + `b` + closure signatures)

> **OUTCOME (2026-07-08): SHIPPED — all exit criteria met.** The full record is
> [certificate-format-v1](../active/certificate-format-v1.md) build-order item
> 5. Grounding results that reshaped the plan, recorded here:
> - Heap-result `if`/`match`/nested-chain/block-arm shapes were ALREADY
>   in-profile and per-arm balanced (the 126921e6 line) — 5a's win was not
>   unlocking walls but RETIRING the per-arm-balance trusted convention
>   (cross-arm compensation now structurally rejects). Corpus coverage
>   unchanged (4,693 in-profile / 317 walled) — no wall moved, none grew.
> - **The grouping found a REAL leak on first contact**: the effect-fn tail
>   `err(msg)` moved the error accumulator only on the error path (`{m|}`) —
>   one block leaked per happy-path call. Fixed by RELEASE PARITY in
>   `lower_heap_result_if_inner` (+ `verify_ownership` branch JOIN with
>   `BranchDisagreement`).
> - CAPTURING lambdas also defunc-inline for direct `list.map`; only
>   returned/stored funcrefs survive as `CallIndirect`. Capturing closures
>   still WALL at lowering, so 5c shipped possible-callee-set agreement (row
>   expansion — zero new Coq) and `b`'s closure-ENV emission awaits the
>   closure-env lowering brick.
> - MODE byte: RETIRED with rationale (the alphabet is self-guarding; policy
>   belongs in the manifest layer).

> **Read first**: [certificate-format-v1](../active/certificate-format-v1.md) (the ladder;
> bricks 1–2 shipped, 3a/3b + 4a/4b shipped), `proofs/OwnershipChecker.v`
> (exec / CertItem / UnrollsL — the architecture you will extend),
> `proofs/CallModes.v` (brick 2c — the composition-law house style to follow).
> Sibling context: [v1-heap-result-control-flow](v1-heap-result-control-flow.md).

## Context — where the ladder stands (2026-07-08, commit `c485bef9`)

- Bricks SHIPPED: 1 (i/a/d/m alphabet), 2a/2b/2c (effect calls, per-call-site
  caps, **param-mode signatures + manifest-declared caps**), 3a/3b (op→wasm
  table + runtime memory model), 4a/4b (perceus `r` + reuse soundness + FreeList
  + CowSafety + real-RC renderer).
- Spine state: `proofs/check.sh` green (coqc + coqchk + axiom audit +
  claim-drift + wat2wasm/wasmtime grounding); `gate.sh` 20 rows green;
  `corpus-wall.sh` ACCEPT over 4,673 fns / 4 properties; almide-mir 577/577.
- Remaining after this brick: 3c (WasmCert-Coq ISA byte binding — the single
  hardest piece), A2 raw-byte encoding, brick 6 (CertiCoq extraction).

## The goal (one line)

> **Ship certificate format v1 "full mode": one-shot BRANCHES whose arms agree
> on resource state, the `b` (borrow, +0) letter, and CLOSURE signatures — so
> control flow and closures, the two shapes real programs are made of, ride the
> proven checker instead of the lowering's trusted per-arm-balance convention.**

## Non-negotiable invariants (breaking any = the brick failed, revert)

1. **Checker-size tripwire** (certificate-format-v1 §"tripwire"): the checker
   NEVER walks a CFG, opens a callee, or runs inference. Every new rule is a
   per-line fold or a `mem`/`subset`/equality over ground facts. Branch
   agreement is the place a CFG-walk is most tempting — **resist it**: the rule
   is "both arm bodies fold from the entry count to the SAME result", nothing
   more.
2. **Axiom purity**: every new theorem `Print Assumptions` = *Closed under the
   global context*; coqchk re-checks; new public claims get TRUSTED_BASE.md
   ledger rows (the claim-drift gate enforces rows ⊆ kernel-checked).
3. **Backward compatibility**: every existing certificate byte-string parses
   and verdicts IDENTICALLY (new delimiters must be outside the current
   alphabet; follow the parse_clc superset pattern).
4. **Conservative reject**: anything the witness cannot see (unknowable
   indirect callee, malformed section) REJECTS — never a silent accept.
   Honest-scope EXCLUSIONS are allowed (and documented), silent ones are not.
5. **Non-vacuous demos**: every new rule lands with an ACCEPT and a REJECT
   example at THREE levels — Coq `Example`, extracted-checker bytes
   (build-checker.sh), and gate.sh (hand-built + real `.almd` where the
   lowering supports the shape).
6. Commit style: English, one line, no prefix. Push only at all-gates-green.

## Sub-brick 5a — branch resource-state agreement (`CBranch`)

**The hole**: `verify_ownership` (almide-mir/src/lib.rs) treats
`IfThen/Else/EndIf` as no-ownership markers and folds arm ops FLAT — sound only
because the lowering promises per-arm balance. That promise is a TRUSTED
convention, and it FALSE-REJECTS (or walls) the heap-result branch: `let x =
if c then [1] else [2]` nets +1 through EITHER arm — a flat fold sees both
arms (+2) and the leaving-balance breaks.

**Ground it first (do not skip)**: emit real certs for heap-result if/match
shapes (`emit_cert_from_source <fixture> main mir` / `ownership`) and record in
this file which shape is walled or mis-accounted today. If the current emitter
never produces a non-balanced arm (everything is walled upstream), the brick's
win is UNLOCKING those walls — cross-check the corpus-wall Unsupported
histogram before/after.

**Coq design** (OwnershipChecker.v, mirror the CCondLoop port):
- `CertItem` gains `CBranch : list Op -> list Op -> CertItem` (flat arm bodies,
  no nesting — same restriction as loop bodies).
- `exec_line` rule: `exec thenb rc = Some r1`, `exec elseb rc = Some r2`,
  accept iff `r1 =? r2`, continue at `r1`. (CCondLoop is the ITERATED cousin
  requiring net-0 preservation; a one-shot branch only needs AGREEMENT — the
  net may be +1, that is the whole point.)
- `UnrollsL` gains `UL_branch : forall thenb elseb a b (choice : bool), …` —
  the concrete run takes ONE arm. Extend `exec_line_unroll`; the headline
  `check_line_unroll_sound` statement is UNCHANGED (that is the beauty of the
  architecture — new item, same theorem).
- Parser: format v4 delimiters `{ then | else }` (`{`, `}` are outside the
  op/loop alphabets; `|` reuses the cond-loop separator logic). `parse_bc`
  (superset of `parse_clc`); `check_bc`; driver dispatches ownership to it.
- Non-vacuous: `i{i|i}dd` ACCEPTs (both arms alloc, net +1, released twice —
  wait, compute honestly: entry 0, `i`→1, branch arms `i`→2 agree, `dd`→0 ✓);
  `i{i|}d` REJECTS (arms disagree +1 vs 0); `{d|d}` REJECTS (both arms fault).
- Emitter (`ownership_certificate`): group per-object events between
  `IfThen`/`Else`/`EndIf` markers into `{…|…}` when the arms do NOT
  self-balance for that object; keep the flat emission when they do (zero churn
  on existing certs — ratchet safety).

## Sub-brick 5b — the `b` (borrow, +0) letter

- `Op` gains `Borrow` (Coq side): `exec` arm = `if rc <=? 0 then None else
  exec rest rc` — a borrow of a DEAD object is use-after-free (matches
  `Op::Borrow`'s live-check in verify_ownership); delta 0.
- Extend EVERY lemma that cases on the op — `exec_app`, `exec_cons` (free),
  `reuses_unique`, `no_reuse` (Borrow is not Reuse), and **CallModes.v's
  `exec_shift`** (Borrow is shift-safe: guard `rc > 0` survives +k) and
  `exec_fill`'s op case. `parse_byte` gains `b`/`B`. The soundness statements
  do not change.
- The LOAD-BEARING consumer is 5c (closure-env borrow). Until then `b` also
  gives the cert a way to witness `Op::Borrow`/`MakeUnique` liveness (today
  invisible). Decide and RECORD in certificate-format-v1.md whether the
  emitter starts emitting `b` for plain borrows immediately (stronger certs,
  bigger diff) or only for closure envs (minimal) — recommendation: closure
  envs only, plain borrows as a follow-up ratchet.
- **MODE byte decision**: the format doc promises `eager|perceus|full` gating
  (a witness must not use `r`/`b` before its obligation exists). `r` shipped
  without it. Either implement the MODE prefix now (small: first byte selects
  the legal alphabet, default full-back-compat) or retire the idea with a
  recorded rationale in certificate-format-v1.md. Do not leave it ambiguous.

## Sub-brick 5c — closure signatures (G3.1 / G3.3)

**Scout first (recorded fact, 2026-07-08)**: a NON-capturing lambda
(`xs |> list.map((x) => x + 1)`) is defunc-INLINED — main's MIR has no
FuncRef/CallIndirect at all. So the surface is exactly: (α) capturing lambdas,
(β) lambdas that survive as `FuncRef` + `CallIndirect` (returned/stored
funcrefs, `fan.*` thunks, bound-funcref dispatch — see the recent
`3626e7a8`/`9fdcef5d` execution-path commits). Dump MIR for those shapes
before designing; record the env representation here.

**Design sketch** (follow the caps precedent — account at CREATION, verify by
subset/equality, never open the callee):
- A closure's signature = its lambda's param modes (already in the sig table —
  lifted lambdas are program functions) PLUS its captured-env ownership: the
  env is a heap value the closure body BORROWS (`b` events on the env object;
  no scope-end release — the closure object owns it).
- `call_modes_witness` today SKIPS `CallIndirect` (documented honest scope).
  Close it: the compiler knows, per `CallIndirect` site, the POSSIBLE-CALLEE
  set from its own closure-table construction (the same ground truth caps uses
  at `FuncRef` creation). Emit it: site = `<possible callee indices> <actual
  modes…>`; checker verifies agreement against EVERY possible callee
  (`forallb` — flat, within the tripwire). An unknowable site (table index not
  traceable) emits the out-of-range sentinel → conservative REJECT.
- Coq: either extend CallModes.v (site = callee-SET) or a sibling
  `ClosureModes.v` importing it — pick whichever keeps `check_fill_sound`
  untouched; the new theorem is "agreement against every member of the set"
  (one `Forall` lift of `site_agrees`).
- Gate: a real capturing-closure fixture ACCEPT + a hand-built
  mismatched-callee-set REJECT.

## Verification ladder (run in this order, stop on first red)

```
cd proofs && ./check.sh          # coqc + coqchk + axiom audit + claim-drift
./build-checker.sh               # extraction + byte-level ACCEPT/REJECT demos
./gate.sh                        # compiler ⊳ proven checker (hand + real source)
cargo test -q -p almide-mir      # 577+ (extend, never shrink)
./corpus-wall.sh                 # 4,673 fns total; Unsupported histogram must not GROW
cargo test -q                    # workspace zero-fail
```

## Exit criteria (all must hold)

- [ ] `CBranch` + `b` + closure-signature checkers proven; ALL new theorems
      axiom-clean + coqchk'd; TRUSTED_BASE.md rows added; claim-drift green.
- [ ] Extracted checker handles formats v4 (`{…|…}`) and `b` bytes; ALL
      pre-existing build-checker/gate rows byte-identical in verdict.
- [ ] gate.sh: ≥6 new rows (branch ACCEPT/REJECT, borrow ACCEPT/REJECT,
      closure-sig ACCEPT/REJECT), incl. ≥2 real-source fixtures.
- [ ] corpus-wall ACCEPT; heap-result-branch walls unlocked or the blocker
      recorded here with the exact wall reason.
- [ ] certificate-format-v1.md: brick 5 marked SHIPPED with the honest scope;
      MODE-byte decision recorded.
- [ ] Pushed to develop at all-green; Trust Spine workflow green on the push.

## What NOT to do

- No CFG walk / path enumeration in the checker (the tripwire).
- No trusting a compiler-computed "arms balance" bit — the checker re-derives
  agreement from the arm bodies themselves.
- Do not start 3c / A2 / brick 6 here; do not refactor OwnershipChecker's
  existing items "while at it" — additive arms only.
- Do not weaken any existing REJECT example to make a new shape pass.
