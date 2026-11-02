Historical description: First field measurements of C-ASSERTED: an MSR experiment showing LLMs law blocks (properties holding for all inputs), an experiment asking how many of the same laws Lean's automatic tactics alone can close, and an experiment asking whether per-function laws compose proofs without mathematical craft. Numbers, findings, what is missing for Lean-level assurance, and next steps.
<a id="law-ブロック実験--c-asserted-を測ってみた記録-2026-09-27"></a>

# Law-block experiment — a record of measuring C-ASSERTED (2026-09-27)

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](law-blocks-experiment.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](law-blocks-experiment.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](law-blocks-experiment.zh-CN.md)

> Non-normative historical reader snapshot of the unchanged pinned source; no autosync or independent rerun of experiments, proofs or implementation checks. Lean kernel-checked claims here concern manually transcribed Lean functions, not directly Almide source; translation and theorem-statement fidelity retain their weaker verification status. Experiment dates, tiny sample sizes, limits, post-hoc additions and incomplete next steps are preserved, not promoted to current guarantees. The source’s §4 repair-loop value is an interpretation, while §2 explicitly says successful counterexample-driven repair was not observed; these are not silently unified. The newer 7/16 experiment and older 4/16 or assisted 6 counts remain attached to their original sections. Historical reproduction commands and task lists are not new execution instructions. [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/law-blocks-experiment.md)

> **What this is**: a record of testing **C-ASSERTED** from [behavioral-contract](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/behavioral-contract.md)
> (machines propose short properties, humans ratify only their wording, properties remain in a decidable/automatable fragment)
> once through **measurement**, rather than design. The starting idea was “Lean's guarantees, with a programmer's
> writing style and without mathematical craft.” From there we made `law` blocks and measured three questions.
>
> 1. Does showing laws make LLM modifications correct? (MSR experiment)
> 2. Can machines alone prove those laws? (Lean experiment)
> 3. **If laws are written per function, can proofs compose without mathematical craft (lemmas, strengthening
>    induction, tactics)?** (§3.5 — a direct answer to the initial question)
>
> **Status**: experiments complete, next steps unstarted (§7). Artifacts under `research/` on develop (PR #2775),
> raw execution data outside the repository at `almide-docs-archive/law-spec-runs-2026-09-27/`.
> **Design**: [law-proofs-design](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/law-proofs-design.md) (proof-layer design informed by these measurements).
> **Sibling documents**: [behavioral-contract](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/behavioral-contract.md) (higher-level design),
> [compile-time-contracts](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/compile-time-contracts.md) (`where` preconditions, fall back to runtime checks if undecidable).

---

<a id="1-設計スケッチ実験の前提"></a>

## 1. Design sketch (experiment assumptions)

```
law "reverse twice is identity" for (xs: List[Int]) {
  list.reverse(list.reverse(xs)) == xs
}
```

- `law` is a property shaped like a **test over all inputs**. Propositions are Bool expressions.
- The LLM writes **only the law (statement)**. It does not write proof bodies, lemma names or tactics.
- Status is **computed, not declared**: a counterexample means **error** / a proof means `proved` /
  neither means `tested`. `trusted` belongs only in an external ledger (no annotation escape hatch for LLMs).
- **Counterexamples first**: on failure, display a shrunk minimal counterexample, not `unsolved goals`.
- Principle: **reject only when there is evidence of error. If merely unable to show it, lower the level rather than reject.**

MSR prediction (before the experiment): handwritten proofs are strongly negative because of hallucinated lemma names
and brittleness under modification; laws + counterexamples + automatic classification alone lean positive.
This document records measurements of that prediction.

<a id="2-実験-1--msrgrammar-lab-law-spec"></a>

## 2. Experiment 1 — MSR (Grammar Lab `law-spec`)

**Setup**: six tasks (rle / paginate / rotate / chunk / merge / top_k). Each task has “modification instructions +
visible tests + unseen hidden tests.” Variants are `tests` (tests only) and `laws` (tests + laws presented as reading,
fixed and appended by the grader). If the first attempt fails the gate (visible tests, plus laws for the laws variant),
return output once for repair. Hidden tests are grading-only — **gate passes ∧ hidden fails = “survived but actually wrong.”**
Model Haiku 4.5, three trials per cell.

| | tests | laws |
|---|---|---|
| gate passes (first attempt) = equivalent to current MSR | **18/18 (100%)** | 11/18 (61%) |
| hidden correct (final) | 14/18 (78%) | 15/18 (83%) |
| **survived but actually wrong (final)** | **4** | **0** |

**Interpretation**:

- The largest finding concerns **how MSR is measured**, not laws: with weak tests judging “survival,” the tests variant
  looks like 100% but is actually 78%. About 20% of survivors were wrong. Laws are less a mechanism for increasing MSR
  than **a mechanism for making MSR measure the right thing**.
- Typical law benefit (merge): the tests variant handled only “duplicates between lists” suggested by visible tests and
  failed all hidden tests; the laws variant read “the result is strictly increasing” and handled duplicates within lists
  and empty input too. What worked was **the law as reading material**. Repair from counterexample feedback
  **has not been observed even once**: the one case receiving a counterexample failed with a syntax error.
- Of seven first-attempt failures in the laws variant, only one was caught by a law counterexample. The rest were prompt
  ambiguity (two omitted type declarations), known compiler bug #2600 (two), and syntax errors (two).
  The tests variant had zero syntax failures. Showing laws may impose a cost by **encouraging an attempt at a complete
  solution, increasing opportunities to fail** (cannot disentangle at n=3).

**Reasons to discount**: small n (one model × three trials). The same human (experimenter) wrote laws and hidden tests,
so they target the same edge cases; alignment of the laws variant with hidden tests is partly expected by design.

<a id="3-実験-2--leanlaw-を機械だけで閉じられるか"></a>

## 3. Experiment 2 — Lean (can machines alone close laws?)

**Setup**: **manually** transcribe the six reference solutions to Lean 4.29.1 (check equivalence with `#eval` on all
hidden-test inputs), write 16 laws as theorems, and try without human-written proofs.
T0 = automatic tactics (`omega` / `decide` / `simp_all [defs]` / `grind [defs]`),
T1 = T0 + machine-enumerable induction (`induction` on list arguments, `fun_induction` after unfolding definitions).

| Task | Closed by machine alone | Not closed |
|---|---|---|
| paginate | pages below 1 are empty | concatenating all pages reconstructs the original |
| rotate | preserves length / full rotation is identity | reverse rotation restores the original |
| top_k | at most k items | count/name order / does not miss top entries |
| rle / chunk / merge | — | all (8 laws) |

**4 of 16 (25%)**. Every closed property was shallow, resolved by unfolding definitions. Those requiring induction:
zero, even with T1. **“Machines handle proofs” does not work out of the box.**

**The sticking point is not proof but “lemma shape”**: laws concern wrapper functions (`chunk`, `merge`),
while induction applies to inner recursive helpers (`restChunks`, `mergeAll`), requiring lemmas generalizing
those arguments (strengthened induction). When a human wrote **only the statements** of three lemmas,
all three lemmas and two laws (chunk reconstruction/nonemptiness) closed with the same machine procedure.
The human's work was not proof but statements of “what holds about which function,” formally laws themselves.

**Finding while defining**: the original chunk code (Almide) loops forever for `n ≤ 0`. Lean rejected the nonterminating
definition, and even the termination proof did not pass automatically (it needed a `List.length_drop` hint).
No law or test had caught this hole.

<a id="35-実験-3--関数ごとの-law-で証明は組み上がるか-2026-09-28"></a>

## 3.5 Experiment 3 — do proofs compose with per-function laws? (2026-09-28)

**Question**: mathematical craft turned out not to be vocabulary, but strengthening induction, assembling lemmas,
proving termination and finding library facts (§3). If replaced with “**write laws per function** (like writing tests
per function),” can machines compose proofs along the call structure?

**Discipline (fixed before experiment)**: humans write **law statements only** (one to three for every function,
including helpers). One shared machine procedure for all laws: unfold target-function definitions, use proven laws,
a fixed set of 16 “standard-library laws,” induction along the target function's recursion, automatic tactics
(`grind` / `simp_all` / `omega`). Work bottom-up through calls, using only proven laws at higher levels.
Lean was a **measurement instrument** (not a claim that the proposed mechanism uses Lean; a guide to the upper
limit of what this discipline can close).

| | Experiment 2 (task laws only) | Experiment 3 (add per-function laws) |
|---|---|---|
| of the original 16, closed by machines alone | 4 | **7** (8 if one general law is added to the fixed list) |
| human-written material | three lemma statements (chunk only) | 23 law statements for helpers and others. Zero tactics |

The 19 proven laws were rechecked together and confirmed to depend on no axioms beyond Lean's standard axioms
(`propext` / `Classical.choice` / `Quot.sound`) — no unproven holes (`sorry`).

**Composition example (top_k “does not miss top entries”)**: what needed human help in experiment 2 closed with one
`grind` call by stacking laws bottom-up: `keyLe` is a total order (two laws: “one precedes the other” and “transitive”)
→ `topK` results follow that order → goal. Each is precisely a property one thinks about when implementing `Ord`.
The hypothesis that **proof structure takes the same shape as call structure** held here. Two chunk laws, two rotate
laws and one paginate law had the same shape.

**The nine unclosed laws fell into three categories**:

| Category | Laws | Content |
|---|---|---|
| division/remainder by variables | chunk “only the first is short” / rotate “reverse rotation” / paginate “all pages reconstruct the original” | `len % n`, `(k % n + n) % n`, `count × size` — divisors/multipliers are variables, outside linear arithmetic |
| fold accumulator values | three merge laws | `dedup` uses `foldl`, requiring accumulator laws. Rewriting recursively closed membership, but not “strictly increasing” or “fold version = recursive version” |
| number formatting | two rle laws | Properties of `Nat.toDigits` (decimal string conversion) |

The remaining one (top_k “count/name order”) closed by adding `List.Pairwise.imp` to the fixed list;
`mergeAll`'s ordering law also closed with `List.pairwise_cons`: the list was insufficient, not the discipline.
These were post-hoc additions, so are excluded from the main total (7/16).

**Implication**: structural parts (lemma assembly, induction strengthening, proof order) were replaced by stacking
per-function laws. Remaining walls are **arithmetic** and **folds**. Language design could avoid arithmetic (provide
remainder/page calculations as standard functions with laws, such as `rotate`, `chunks_of`, so users need not write
variable remainders). Folds are inherently difficult: choose between asking for accumulator laws and restricting folds
to standard forms with laws.

**Discounts**: the experimenter wrote the statements; whether LLMs or ordinary programmers can write the same ones
is unverified. The fixed list was chosen once. The engine was Lean's `grind`; there is no guarantee a custom prover
has the same power. Translation (Almide → Lean) remains manual (§5).

<a id="4-発見の要約"></a>

## 4. Findings summarized

1. **Weak tests inflate MSR** (about 20% here). Independently of adopting laws, it is worth measuring what fraction of
   Dojo's MSR is inflated.
2. **Laws show their value with the repair loop included**. In one-shot measurement they are mostly a loss.
3. **Align law granularity with recursive functions** to make automatic proof work. Laws on wrappers alone do not close.
   Having machines infer lemma shape (lemma speculation) is research-grade.
4. **Totality is a language issue**. As long as nonterminating functions are accepted, one cannot say “holds on all inputs.”
5. **Per-function laws compose the structural parts of proofs** (§3.5, 7/16 → the discipline's success reduces to
   “arithmetic” and “folds”). This is the skeleton of theorem proving without mathematical craft.
6. Byproduct: #2771 (for an undefined type, misleading E013 appears first, root-cause E029 last and without location —
   the LLM repair loop receives only the initial misdirection).

<a id="5-lean-相当に足りないもの重い順"></a>

## 5. What is missing for Lean-level assurance (heaviest first)

The parts **handled manually in this experiment** are exactly the remaining work:

1. **Trustworthy Almide → logic translation** (largest hole). These proofs concern manually transcribed Lean functions,
   not the Almide code. Semantics changed in three places during transcription (String to lists / chunk's `n ≤ 0` /
   Int remainder). Existing `proofs/*.v` describes ownership-fragment semantics; there is no value semantics for user
   functions yet. A precedent is Aeneas for Rust → Lean (a research project in itself).
2. **Totality** (termination checking).
3. **Matching numerical semantics** (Lean's `Int` is arbitrary precision, Almide is 64 bit).
4. **Escape when proofs do not close**. Since LLMs are not asked to write proofs, unclosed laws fall to `tested`:
   the realistic destination is not a “Lean-equivalent language” but “a language in which each function may or may not
   carry Lean-level guarantees, with that proportion visible.”
5. **Small kernel**. Can be borrowed by using Lean underneath (relatively cheap).

<a id="検証としての信頼の鎖今回どこまでが-lean-級か"></a>

### Chain of verification trust (what is Lean-level here?)

```
Almide のソース
   │ ① 手で Lean に写す          確認は hidden テストの約 25 入力の #eval だけ(= テスト級)
Lean の定義
   │ ② law を手で定理に写す      未確認
Lean の定理
   │ ③ 証明                     カーネル検査済み(Lean 級)
```

Only ③ is Lean-level; a chain is determined by its weakest link. What was shown is not “Almide functions are correct,”
but “**for the transcribed Lean functions**, four properties (six with lemma assistance) hold.”
For function value semantics, the question [`proofs/ALS.v`](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/proofs/ALS.v) asks of ownership fragments
(*you proved a MODEL — does the REAL emitted artifact correspond?*) remains unanswered.

- Weakness of ①: three intentional semantic changes; 25 inputs will not catch unintended differences.
- Weakness of ②: for example, “strictly increasing” was written with `getD i 0` (0 outside range).
  If a range condition is omitted, a default value can make **a false property trivially true**.
- The ③ figure (four of 16) is a “closure rate with zero human help,” not verification difficulty.
  All 16 are ordinarily provable by someone familiar with Lean. Read it only as an indicator of the scope in which
  the no-LLM-written-proofs design works.

<a id="6-成果物"></a>

## 6. Artifacts

| Path | Contents |
|---|---|
| `research/spike/law-blocks/law.almd` | Runtime side of laws (pure Almide): fixed-seed generator, shrinking (monotonic toward 0), counterexample display |
| `research/grammar-lab/` | Revived runner (syntax since ADR-0008) + hidden tests / fixed laws / repair round / task parallelism |
| `research/grammar-lab/experiments/law-spec/` | Six tasks, reference solutions, `validate.sh` (verifies all reference solutions pass and original code fails hidden tests) |
| `research/spike/law-blocks/lean/` | Hand-transcribed definitions, `laws.py` (machine-only proof sweep), `Assisted.lean` (lemma assistance), `modular.py` (bottom-up per-function law proof sweep, §3.5; generates `Modular.lean` when run) |

Measurement pitfalls (prevent recurrence):
- `almide test` retains a ~12 MB work directory for seven days **per absolute test-file path** (#2608).
  Using a separate path each trial filled the disk: the runner evaluates at a fixed path per lane.
- Shrinking cycles (0 → 1 → 0) unless **monotonic toward 0**. The law checker itself needs termination too.
- `fun_induction` attempts **never run** when a law names a wrapper (“no call in goal”), yet get counted as
  “not closed.” Check output to establish whether they ran.

<a id="7-次の一手未着手順不同"></a>

## 7. Next steps (unstarted, unordered)

Two ways to tighten the chain of trust (§5), cheapest first: they do not reach Lean-level, but bring every link to
at least test-level and identify the weak link (behavioral-contract §5: “state which claim from which source”):

- [ ] **① Translation differential checks**: feed inputs from law generators (hundreds) to both Almide functions and
      Lean transcriptions (`#eval`) and compare outputs. Move ① from “visual inspection on 25 inputs” to “differential
      checks on generated inputs.”
- [ ] **② Executable theorem-statement comparison**: evaluate Lean theorem statements as decidable Bools on the same
      inputs, compare with Almide-side law decisions. Catch misstatements (e.g. forms trivially true due to defaults).

- [ ] **Minimal automatic translation**: for a fragment with only recursion, lists and Int, auto-generate Lean definitions
      from Almide functions and check they match the manually transcribed six tasks. Only once this works does it become
      “proof about Almide code.” Decide the language-feature scope before starting.
- [x] **Per-function law sweep** → §3.5 (7/16).
- [ ] **Can standard functions avoid the arithmetic wall?** Measure whether the three chunk / rotate / paginate laws close
      in a version moving remainder/page calculations into standard functions with laws.
- [ ] **Can LLMs write law statements?** Replace the experimenter's 23 statements with LLM-written ones and measure
      how many close under the same discipline. Answer whether “humans write only law statements” works for LLMs too.
- [ ] **Rerun MSR with the develop compiler** (version fixing #2600; remove four instrument-caused failures and measure
      only the effect of counterexample feedback).
- [ ] **Measure MSR inflation**: add hidden tests to Dojo tasks and measure only the rate of “survived but actually wrong.”

Reader explanation of the preserved Japanese trust-chain diagram: Almide source → (① manually transcribe into Lean; checked only by #eval on about 25 hidden-test inputs, i.e. test-level) → Lean definitions → (② manually transcribe laws into theorems; unchecked) → Lean theorems → (③ proof; kernel-checked, Lean-level).
