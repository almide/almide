Description: Design of a per-function law proof layer using Lean as kernel: automatic pure-fragment translation, computed proved/tested/refuted levels, Int wrapping and remainder semantics, termination, translation trust chain, Lean licensing/trademarks, and staged plan.
<a id="law-証明層--設計-draft-2026-09-28"></a>

# Law proof layer — design (draft, 2026-09-28)

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](law-proofs-design.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](law-proofs-design.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](law-proofs-design.zh-CN.md)

> Non-normative reader snapshot of the pinned source; original remains authoritative. No automatic synchronization, independent implementation validation or execution of quoted instructions. Historical status, plans and comparative claims retain their source scope.
> [Original source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/law-proofs-design.md)

> Source caveat: unimplemented September 28 proposal, not a shipped law command or proven compiler. Stage-1 translation and statement links remain finite test-grade checks even if Lean checks the model proof; the opening Lean-strength aspiration is bounded by section 5. Arithmetic side-condition and Int64 alternatives are unresolved, and stages disagree on whether Map/Set enter in 2 or 3. Section numbering skips 1. Same-name Result mapping and mathematical/automatic-termination assertions are retained design claims, not independently validated translations. Six tasks/one model, 7/16 and roughly 20% are historical experiment reports, not new measurement or general language superiority. License/trademark statements are the dated source summary, not current legal advice; bundling requires fresh review. No Lean, generated command, model call or paid benchmark was run.


> **What this is**: a layer design backing **C-ASSERTED** in [behavioral-contract](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/behavioral-contract.md)—machines propose short properties, humans ratify only the wording—with **proofs using Lean as kernel**. Measurement evidence: [law-blocks-experiment](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/law-blocks-experiment.md).
>
> **In one sentence**: users write a per-function `law` (a Bool-expression property holding for all inputs). The compiler automatically translates pure functions and their laws into Lean, then attempts proofs by a fixed mechanical procedure bottom-up through the call graph. Closed laws receive **guarantees as strong as Lean's**; unclosed laws fall back to generated-input checking (`tested`). Laws with counterexamples are **compile errors**.
>
> **Status**: design only. Implementation not started. Begin at Stage 0 in §9.

---

<a id="0-先行例との関係と主張の範囲"></a>

## 0. Relation to prior work and scope of claims

The core idea—“users add lemma (law) **statements**, while a machine searches for proofs by induction and rewriting”—is not new.

- **Boyer–Moore provers / ACL2** (1970s onward): exactly this methodology—users add lemma statements where the prover gets stuck, machines search by induction and rewriting. Experiment 3 (7/16 with per-function laws) replicates it in Almide's context.
- **Composing per-function contracts along the call graph** is standard in Dafny / Why3 / Liquid Haskell.
- **Proofs = programs, induction = recursion** is the Curry–Howard correspondence, a foundation of type theory.
- Multiple 2024–25 studies also have **LLMs write specifications/lemmas and provers check them**, targeting Dafny / Verus.

This design adds two things: (1) assume the writer is an **LLM**, ask only for law statements, compute judgment levels and present counterexamples first as part of the **language surface**; (2) measure it with **MSR**. Both are still small experiments (6 tasks, 1 model).

**Scope limitation**: “proofs have the same shape as programs” is useful only for **proofs of programs over inductively constructed data**, such as lists and natural numbers. Proofs relying on excluded middle or choice (without corresponding programs), or infinite objects such as reals and continuity, are outside this layer. This is not a claim about all mathematics.

“Lean-equivalent” has three meanings, and this layer targets only one ([law-blocks-experiment](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/law-blocks-experiment.md) §5).

| Meaning | This layer |
|---|---|
| **Guarantee strength**—kernel-checked proof about Almide code | **Targeted** (pure fragment only) |
| Expressiveness—prove anything Lean can prove | Not targeted (reproducing dependent type theory and Mathlib rebuilds Lean) |
| Automation—writing a law always finds a proof | Impossible in principle; only keep raising the ceiling |

**Goals**

- G1. Users write only law **statements**, not tactics, lemma names or proof terms (nor ask LLMs to write them).
- G2. A law labeled `proved` holds **for that Almide code**—translation errors must not produce false `proved` (§5).
- G3. Proof structure matches the code's call structure; failure can identify which function lacks laws.
- G4. Judgments are **computed**, not declared. No source field for “this is proven.”

**Non-goals**

- Proofs of code containing `var`, effect fn, IO, captured closures (reconsider in Stage 3+).
- Float-property proofs (IEEE 754 modeling is separate research).
- A handwritten proof language. The fallback for unclosed laws is `tested`, not a way for humans to write proofs.
- An independent proof kernel. Lean performs final checking.

<a id="2-利用者から見た姿"></a>

## 2. User-facing form

```almide
fn rest_chunks(xs: List[Int], n: Int) -> List[List[Int]] =
  if list.len(xs) == 0 or n <= 0 then []
  else [list.take(xs, n)] + rest_chunks(list.drop(xs, n), n)

law "rest_chunks rebuilds its input" for (xs: List[Int], n: Int where it >= 1) {
  list.flatten(rest_chunks(xs, n)) == xs
}

law "rest_chunks makes no empty chunk" for (xs: List[Int], n: Int) {
  rest_chunks(xs, n) |> list.all((c) => list.len(c) > 0)
}
```

- `law` is a top-level declaration at function level; its body is a **Bool expression**.
- `for (...)` gives universal arguments. `where` supplies argument preconditions (decidable predicates only).
- The associated function is inferred from calls in the body; write `law rest_chunks "..."` to specify it explicitly (used in §4.3 proof ordering).

Example output of `almide check --laws`:

```
proved   rest_chunks rebuilds its input          (by recursion on rest_chunks)
proved   rest_chunks makes no empty chunk        (by recursion on rest_chunks)
proved   chunk rebuilds its input                (from: rest_chunks rebuilds its input)
tested   only the first chunk may be short       (1000 cases, no counterexample)
         not proved: needs arithmetic with a variable divisor (len % n); see §6.1
error[E1xx]: law "unique keeps length" is false
  counterexample (shrunk in 7 steps):
    xs = [0, 0]
```

<a id="3-判定のレベル"></a>

## 3. Judgment levels

| Level | Meaning | Computed condition |
|---|---|---|
| `refuted` | False—**compile error** | Generated input finds a counterexample |
| `proved` | Holds for all inputs (Lean kernel-checked, with §5 chain) | Translation + proof closes |
| `tested` | No counterexample found, but not established | Neither of the above |
| `trusted` | A person declares responsibility | Only an **out-of-source** ledger (e.g. `proofs/law-trust.toml`) |

Two principles: **reject only with evidence of error** (failure to establish a property lowers the level rather than rejecting). **Compute levels, do not declare them** (no source `@proved` field—no annotation for LLMs to hide in).

Both `tested` → `proved` promotions and `proved` → `tested` demotions (a code change prevents rediscovering the proof) are reported as diffs. CI can ratchet “do not reduce the number of `proved` laws.”

<a id="4-アーキテクチャ"></a>

## 4. Architecture

```
.almd ─ parse ─ check ─┬─ law の型検査(Bool 式・決定可能な where)
                       │
                       ├─ [tested] 生成器 + 縮小(seed 固定)で反例探索 ── 反例 → refuted(エラー)
                       │
                       └─ [proved] 純粋断片の抽出 → Lean 翻訳 → 証明手順 → lean(外部プロセス)
                                     │                                   │
                                     └── キャッシュ(IR ハッシュ + law 文 + 手順版)←┘
```

Diagram translation: .almd → parse → check branches into law type checking (Bool body and decidable where); tested generator + shrinking with fixed seed seeks counterexamples, which yield refuted errors; and proved extraction of pure fragment → Lean translation → proof procedure → external lean process. Cache is keyed by IR hash + law statement + procedure version.

<a id="41-生成と反例探索tested--refuted"></a>

### 4.1 Generation and counterexample search (`tested` / `refuted`)

Embed spike `research/spike/law-blocks/law.almd` in the compiler.

- Generators are **derived from types** (Int / Bool / String / List / Option / Result / records / variants).
- Interpret `where` as generator constraints (`it >= 1` → generation with lower bound). Uninterpretable predicates use rejection sampling.
- Shrinking must be **monotonic toward 0/empty** (the spike hit a cycle).
- Derive seed deterministically from file path and law name: the same law is checked on identical inputs every time.

<a id="42-翻訳almide-ir--lean"></a>

### 4.2 Translation (Almide IR → Lean)

Translate functions in the **pure fragment** (§4.4) and their laws. Translation starts **from typed IR, not source** (`IrProgram` has types on all nodes).

- Functions → Lean `def`; recursion → Lean recursion; `match` → `match`.
- Records → `structure`; variants → `inductive`; `Option` / `Result` → Lean types of the same names.
- Laws → `theorem`; `for` args are universal bindings, `where` assumptions, Bool body an `= true` proposition.
- Standard-library functions → mapping table of Lean models (§6.2).

Numeric semantics are decided in §5.2.

<a id="43-証明手順固定"></a>

### 4.3 Proof procedure (fixed)

Turn Experiment 3's discipline directly into a procedure.

1. Group laws by **target function**, order reverse-topologically along calls (bottom-up).
2. Apply the same fixed procedure to every law:
   `grind [対象関数の定義, 証明済みの law, 標準ライブラリの law]` →
   `simp_all [...]` → `fun_induction` following target-function recursion → `induction` over list arguments.
   The Japanese items in the retained tactic sketch mean target-function definition, proven laws, and standard-library laws.
3. Only proven laws may be used at higher levels; never assume unproven laws.
4. If it does not close, infer and report **where a missing law belongs** from the failure shape (e.g. induction gets stuck unable to state anything about callee `f`'s result → “`f` lacks a law”).

Version the procedure's content (tactic names, order, time limit) as a **procedure version** included in the cache key. Changing it rejudges all laws.

<a id="44-純粋な断片段階-1-の範囲"></a>

### 4.4 Pure fragment (Stage 1 scope)

| Included | Excluded |
|---|---|
| Non-effect `fn`, self/mutual recursion | `effect fn`, IO, `!` |
| `Int` / `Bool` / `String` / `List` / `Option` / `Result` / records / variants | `Float`, `Bytes`, `Map` / `Set` (Stage 2) |
| `match` / `if` / `let` / pipes / noncapturing lambdas | `var`, `for`, `while` (Stage 3) |
| Standard-library functions with Lean models | Standard functions without Lean models |

Laws calling outside-fragment functions automatically stop at `tested`; no `proved` attempt.

<a id="45-キャッシュ"></a>

### 4.5 Cache

Cache proofs as derived artifacts, not in source. Key:
“IR hashes of target function and transitive callees + law statement + procedure version + Lean version.”
Same idea as Almide's IR-keyed build cache: renames or comment changes do not trigger reproving.

<a id="5-信頼の鎖--偽の-proved-を出さないために"></a>

## 5. Chain of trust—avoiding false `proved`

The experiment proved **hand-copied models** ([law-blocks-experiment](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/law-blocks-experiment.md) §5). Tightening that chain is the layer's central challenge.

```
Almide の関数 ─①翻訳─ Lean の def ─②文の翻訳─ Lean の theorem ─③証明─ カーネル検査
```

Diagram translation: Almide function → ① translation → Lean def → ② statement translation → Lean theorem → ③ proof → kernel checking.

<a id="51-各輪の締め方"></a>

### 5.1 Tightening each link

| Link | Stage 1 (cheap) | Stage 2+ (stronger) |
|---|---|---|
| ① Function translation | **Differential checking**: run law-generator inputs (hundreds) in both Almide and Lean `#eval`, compare results. Do not issue `proved` on mismatch | Validate translation correctness against semantics in the same family as `proofs/ALS.v` |
| ② Statement translation | **Execution comparison**: evaluate Lean theorem statement as decidable Bool on the same inputs and compare with Almide law judgment | Include in same framework as ① |
| ③ Proof | Lean kernel | Same; check `#print axioms` every time for absence of `sorryAx` |

In Stage 1, ①② remain **test-grade** trust. Honestly label Stage-1 `proved` as “kernel-checked **within the range where translation passed differential checks**” (behavioral-contract §5: identify which claim from which source).

<a id="52-数の意味--実測で確認した二つの落とし穴"></a>

### 5.2 Numeric semantics—two measured pitfalls

Measured with Almide 0.64.0 (2026-09-28):

| Expression | Almide | Lean `Int` |
|---|---|---|
| `int.max_value() + 1` | `-9223372036854775808` (**silently wraps**) | Unbounded precision, no wrap |
| `(0 - 7) % 3` | `-1` (**truncating-division remainder**) | `2` (nonnegative remainder, `Int.emod`) |

Naively mapping `Int` to Lean `Int` labels a law `proved` that is **true in Lean but false in Almide** (e.g. `x + 1 > x` is true for Lean `Int`, false at Almide `max_value`). This violates G2.

**Policy**:

- Map `%` and `/` to Lean's **truncating variants**, `Int.tmod` / `Int.tdiv`, without confusion. Confirmed: `Int.tmod (-7) 3 = -1` and `Int.tdiv (-7) 3 = -2` match Almide `(0 - 7) % 3` and `(0 - 7) / 3`, while plain `(-7 : Int) % 3` gives `2` and differs (Lean 4.29.1).
- Handle `Int` in two layers:
  1. **First attempt the proof over unbounded `Int`** (strong automation).
  2. For every arithmetic operation in translated functions, also generate and prove a side condition: **this operation does not overflow**.
  3. Issue `proved` only if every side condition closes; otherwise demote to `tested`.
- Alternatively translate `Int` as Lean `Int64` (wrapping arithmetic on `BitVec 64`) and use `bv_decide` (SAT, core `Std.Tactic.BVDecide`, no Mathlib needed). Confirmed: `Int64`'s `max + 1` gives the same `-9223372036854775808` as Almide. Stronger for fixed-width arithmetic, but combination with list induction is unknown. Compare both in a small Stage-1 trial before deciding (open Q1).

<a id="53-停止性"></a>

### 5.3 Termination

Lean rejects nonterminating definitions. A law depending on a translated function that fails Lean's termination check cannot be `proved`.

- Structural recursion (e.g. recurse on list tail) passes automatically.
- Otherwise rely on Lean's automatic termination inference; if it fails, diagnostics ask “what decreases?” Users may write only a one-phrase declaration such as `shrinks len(xs)`; the machine proves it.
- In the experiment, this check found **at definition time** that `chunk` loops forever for `n <= 0` (neither tests nor laws had caught it). Termination failure itself merits a warning.

<a id="6-天井を上げる仕組み"></a>

## 6. Raising the ceiling

The 9 laws not closed in Experiment 3 fell into three classes: arithmetic, folds, numeric formatting.

<a id="61-算術--標準関数に閉じ込める"></a>

### 6.1 Arithmetic—encapsulate in standard functions

Division/remainder/multiplication by variables (nonlinear integer arithmetic) is generally undecidable. Avoid it by not making users write it in their code.

- Provide **standard functions with laws**, such as `rotate(xs, k)`, `chunks_of(xs, n)`, `page(xs, i, size)`. Prove each law once on the stdlib side, including manual work if needed.
- User laws compose using these standard-function laws, avoiding remainder arithmetic.
- If a function directly using `%` or `*` between variables has an unclosed law, diagnostics point to the corresponding standard function.

<a id="62-標準ライブラリの-law"></a>

### 6.2 Standard-library laws

Provide standard functions as a trio: **Lean model + law + proof**.

- Lean model: function meaning written in Lean, subject to §5 differential checks.
- Law: a property usable in user proofs (e.g. `list.take(xs, n) + list.drop(xs, n) == xs`).
- Also a law readable as documentation (published in `docs/stdlib/`).

In the experiment, 2 cases failed to close solely because the fixed list lacked a general law (`Pairwise.imp`, `pairwise_cons`). Stdlib-law coverage directly affects the ceiling. As with API-family coverage, add a **matrix coverage gate** (CLAUDE.md API-family rule).

<a id="63-畳み込み--不変条件の候補を機械が出す"></a>

### 6.3 Folds—machines propose invariant candidates

The invariant for an intermediate `foldl` accumulator is nowhere written in code. Options:

1. Ask users to write accumulator laws (something like `law for fold in dedup ...`).
2. Shift folds to standard forms with laws (e.g. `list.dedup_sorted`).
3. **Machines propose candidates**: learn invariant candidates from execution records (per-step fold states on generated inputs), then have the prover check them. The differentiable logical-gate network in `almide-graphics/nn` (extractable as logic circuits after training) may learn combinations of atomic predicates. Wrong candidates are merely rejected by the prover, leaving soundness intact.

Try option 3 at small scale in Stage 2: can it close “strictly increasing” for `dedup`?

<a id="7-lean-の利用条件"></a>

## 7. Conditions for using Lean

Checked against primary sources on 2026-09-28.

<a id="71-ライセンス"></a>

### 7.1 Licensing

- Lean 4 itself: **Apache License 2.0** ([LICENSE](https://github.com/leanprover/lean4/blob/master/LICENSE)).
- Mathlib: Apache License 2.0 (this layer assumes no Mathlib dependency).
- Third-party components in Lean distributions ([LICENSES](https://github.com/leanprover/lean4/blob/master/LICENSES)): LLVM (Apache 2.0 with LLVM Exceptions), **GNU C Library (LGPL 2.1)**, **GNU MP (LGPL 3)**, CaDiCaL (MIT), and others.

Applied to this layer:

| Usage | Conditions |
|---|---|
| Almide invokes Lean as an **external process** (user installs official toolchain via elan) | Fine under Apache 2.0; no redistribution obligations arise on Almide's side |
| Generated `.lean` files | Almide output, not Lean code; may use Almide-side licensing |
| **Bundle Lean toolchain** in Almide distributions | Apache 2.0 notice obligations (include LICENSE / NOTICE), plus obligations for bundled **LGPL components (glibc, GMP)**, such as providing ways to obtain source |

**Policy**: **do not bundle** in Stages 1–2. Users install the official toolchain via elan, pinned in `lean-toolchain` (existing Lean projects in the repo use `leanprover/lean4:v4.29.1`). If bundling becomes necessary, separately review LGPL component handling then.

<a id="72-商標"></a>

### 7.2 Trademarks

“Lean” name and logo are Lean FRO trademarks ([Lean Trademark Policy](https://lean-lang.org/trademark-policy/)).

- **Allowed without permission**: accurately describe the relationship to the software, e.g. “compatible with the Lean programming language and proof assistant” or “uses Lean,” commercially or noncommercially; references in documents/papers.
- **Permission required**: include Lean in another trademark, derive logos from Lean's logo, distribute substantially modified versions under the Lean name.
- Use it **as an adjective with a generic noun**, e.g. “Lean proof assistant.”

**Policy**: do not put “Lean” **as a product name** in feature names, commands, document headings (not “Almide Lean” or “Lean for programmers”). Describing “uses the Lean proof assistant to check proofs” is fine. Use feature names without Lean, e.g. `law` / `almide check --laws`.

(This section summarizes primary sources, not legal advice. Recheck when deciding bundling or commercial distribution.)

<a id="8-msr-と-llm-の観点"></a>

## 8. MSR and LLM perspective

- LLMs write only law statements (G1), excluding hallucinated lemma names and modification-fragile handwritten proofs from the start.
- Counterexamples directly aid the repair loop: shrunk, with argument names.
- Experiment 1 finding: weak-test MSR was inflated by roughly 20%. Law judgments (`refuted`) remove that inflation, so MSR may look lower. On introduction, report alongside it the rate of “survived but actually wrong.”
- Distinguish law rewrites (weakening laws to remove counterexamples) from code changes in diffs.
- Untested premise: **can LLMs write laws that close, particularly inner-function laws?** Include in Stage-1 exit criteria (§9).

<a id="9-段階計画"></a>

## 9. Staged plan

| Stage | Contents | Exit criterion (measurement) |
|---|---|---|
| 0. Scaffold | Compiler-built-in `law` syntax, type checking, generation and shrinking; only `tested` / `refuted` | Spec tests emit shrunk counterexamples for `refuted`. Rerun light MSR measurement (Grammar Lab `law-spec`) with develop compiler |
| 1. Translation | Pure IR → Lean, differential check ① and execution comparison ②, fixed procedure, cache | Reproduce experiment's 6 tasks by **automatic translation**, reaching same 7/16 `proved` as hand-copy version. Checks catch intentionally broken translation |
| 1.5 LLM | LLM writes law statements | How many LLM-written per-function laws reach `proved` (compare experimenter-written 7/16) |
| 2. Ceiling | Stdlib functions with laws (§6.1–6.2), `Int` side conditions (§5.2), fold-candidate trial (§6.3) | Close 3 arithmetic laws (chunk / rotate / paginate) via standard functions. Candidate generation finds `dedup` invariant |
| 3. Extension | Functions containing `var` / `for`, `Map` / `Set`, formal translation validation | Separate design |

<a id="10-未決事項"></a>

## 10. Open questions

- **Q1**: handle `Int` as unbounded + side conditions or `Int64` (`BitVec`) (§5.2)? Compare in Stage 1.
- **Q2**: `law` placement—immediately after function or separate `*_laws.almd` file? Measure both LLM writability and edit locality.
- **Q3**: `where` predicate scope—only decidable fragment (linear arithmetic and lengths)? Share grammar with `where` in [compile-time-contracts](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/compile-time-contracts.md)?
- **Q4**: behavior without Lean (some CI, playground, wasm)—fall back to `tested` and continue, or require reproducing `proved`?
- **Q5**: if raising procedure version demotes some `proved` laws to `tested`, treat it as regression?
- **Q6**: relation to ALS ledger (als repo)—include law judgment levels in contract-ledger evidence classes?

<a id="11-参照"></a>

## 11. References

- Measurements: [law-blocks-experiment](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/law-blocks-experiment.md) (MSR experiment, Lean automatic proofs, per-function laws)
- Parent design: [behavioral-contract](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/behavioral-contract.md) (C-ASSERTED)
- Preconditions: [compile-time-contracts](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/compile-time-contracts.md)
- Ownership-fragment semantics and translation validation: `proofs/ALS.v`
- Prior work: Boyer–Moore / ACL2 (add lemma statements, machine searches proofs), Dafny / Why3 / Liquid Haskell (modular verification of per-function contracts), Aeneas (Rust → Lean), hs-to-coq (Haskell → Coq), CLN2INV (learning loop invariants via continuously relaxed logic)
- Lean: [LICENSE](https://github.com/leanprover/lean4/blob/master/LICENSE), [LICENSES](https://github.com/leanprover/lean4/blob/master/LICENSES), [Trademark Policy](https://lean-lang.org/trademark-policy/)
