<a id="adr-0002-fallibility-and-effect-are-orthogonal-axes---t-marks-pure-fallible"></a>

# ADR-0002: Fallibility and effect are orthogonal axes; `-> T!` marks pure-fallible

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](0002-fallibility-effect-orthogonal.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](0002-fallibility-effect-orthogonal.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](0002-fallibility-effect-orthogonal.zh-CN.md)

> Non-normative historical reader snapshot. The unchanged pinned original is authoritative; no automatic synchronization or independent execution, benchmarking, or safety proof. D6 withdraws the effect-total quadrant and Phase 2/3, while retaining pure-fallible Phase 1. Earlier orthogonality and auto-? passages remain historical, not current API guarantees. Static fallibility classification does not prove host totality. Other-language comparisons and purity generalizations are the source author’s dated claims, not independently verified here. Preserve all dated measurements, weak-evidence qualifications and withdrawn plans. [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0002-fallibility-effect-orthogonal.md)

- **Status:** Accepted, **partially withdrawn on 2026-09-24** (§D6). Phase 1 shipped in #1103 / #1108. The “effect-total” quadrant and Phase 2 / Phase 3 intended to create it are withdrawn: `effect fn f() -> T` always means `T!` (#2563 not planned). Pure-side `-> T!` remains.
  **Falsifier 2 measured 2026-09-23** (#2556, develop 8e0c2dcbb, playground 958ddea): of 987 effect fns, **only four could not be statically classified**, `-> T` intrinsics without an Almide body: `env.set` / `env.sleep_ms` / `http.serve` / `process.exit`. The rest were 574 fallible / 307 total / 82 intrinsic but fallible by `-> Result` signature / 20 intrinsic but total by self-hosted body (breakdown in §Falsifier 2).
  Reproduce: `ALMIDE_BIN=target/release/almide python3 tools/effect_fn_fallibility.py stdlib=stdlib spec=spec playground=<playground>/web/examples`
  (See §Falsifier 2 addendum. ○ 2026-09-23: Falsifier 2 did not fire, 4/987 remaining; Phase 2 filed as #2563. The quadrant was subsequently withdrawn in §D6 on 2026-09-24, so this measurement is not used as grounds for Phase 2.)
- **Date:** 2026-08-05
- **Decision scope:** function-declaration failure-channel notation (`-> T!`), re-decomposition of `effect fn` semantics, fixing the meaning of the `!` operator, criteria for expressing failure as a value versus ⊥ (controlled halt)
- **Related:** [#1055](https://github.com/almide/almide/issues/1055) (effect-typed fn parameters; this ADR supplies vocabulary), C-211 / C-029 ([contracts.toml](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/contracts/contracts.toml)), [result-option-effect.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/result-option-effect.md) (needs revision with this ADR), #489 (lambda boundary), #840/#841 (never-err elision)
- **Ratification:** design discussion on 2026-08-05, starting from “`fn f(s: String) -> Int = int.parse(s)!` gives E022; what should happen?” Evidence was the same day's full operator×context measurement (v0.53.6, 287 cells, seven probe slices in parallel).

<a id="context--何が問題だったか"></a>

## Context — What Was Wrong

<a id="1-effect-fn-は-3-つの性質の縮退である"></a>

### 1. `effect fn` Collapses Three Properties

Current `effect fn` fuses these into one keyword:

1. **Capability** — can call host effects (fs / http / process / random …)
2. **Fallibility** — declaration `-> T` is lifted to `Result[T, String]`
3. **Ergonomics** — auto-`?` runs in the body and `!` becomes legal

This leaves no lightweight spelling for “a function that can fail but does not touch the world.” A function such as `int.parse` must spell out `-> Result[Int, String]` (19 characters), and E022 hints could only offer **three semantically different escapes**: “write Result / make it effect fn / use `??`.”

<a id="2-実測が示した縮退の歪み2026-08-05-matrix抜粋"></a>

### 2. Distortions Shown by Measurement (2026-08-05 Matrix, Excerpts)

- Using `effect fn` solely for error lifting also brings capability, preventing calls from pure code (E006). Conversely, redeclaring it with explicit `-> Result` **allows it to be passed from a pure fn**, laundering effects — evidence that capability and fallibility differ.
- Codegen already has never-err elision (#840/#841), **privately identifying non-failing effect fns as an optimization**, such as `env.args` and `random.int`. The implementation knows the two axes; only the surface type is missing.
- Frequent friction where E remains unresolved in callbacks of the `try_*` family (`(A) -> Result[B, E]`, E025 cluster): no vocabulary to write fallibility in a callback slot.
- C-211 (0.53.5) already permits `!` in pure fns returning `-> Result/Option`, so **the pure-fallible quadrant already exists semantically**. Only notation is missing.

<a id="3-pure-が-result-を返してよいのかへの回答が用語として無かった"></a>

### 3. No Terminological Answer to “May Pure Code Return Result?”

The spec did not distinguish purity, fallibility and totality, allowing “isn't it odd for pure code to fail?” to recur in every design discussion.

<a id="decision--何を決めたか"></a>

## Decision — What Was Decided

**Separate fallibility and effect as two orthogonal axes. The `effect` keyword means capability only; postfix `!` in the return-type position (`-> T!`) means fallibility only.** Final grammar has four quadrants:

> **2026-09-24 revision (§D6):** the third row, “effect-total,” is withdrawn for user declarations. Effect implies fallibility; `effect fn f() -> Int` definitively means the same as row four, with lifting. Stdlib never-err effect functions retain their existing plain-value behavior at call sites (table in §D6).

```almide
fn        f() -> Int      // pure  ・総(失敗しない)
fn        f() -> Int!     // pure  ・可謬     ← 新設(Phase 1)
effect fn f() -> Int      // effect・総       ← 撤回(2026-09-24、§D6)— 4 行目と同義
effect fn f() -> Int!     // effect・可謬     ← 今の effect fn
```

<a id="d1-用語の規準spec-に定義段落として置く"></a>

### D1. Terminology Criteria (A Definition Paragraph in the Spec)

- **Pure** = the body does not touch the world, no host effects. Referentially transparent and reproducible.
- **Fallible** = the return value is lifted to `Result[T, String]`. **This is about codomain shape, unrelated to purity.** `int.parse : String -> Result[Int, String]` is entirely pure: same input → same Result, totalizing a partial function.
- **Failure-representation criteria:** a failure determined **only by arguments** is a value (Result). A failure depending on **world state** belongs in the effect domain. **Failures expressed as ⊥ (T6 controlled halt) form a closed enumeration**, arithmetic-domain errors and out-of-range indexing; do not casually expand it. Conversion to process failure, exit 1, occurs only at main / effect-domain / explicit-operator boundaries.

<a id="d2---t-の表面"></a>

### D2. Surface of `-> T!`

- `!` may appear only at a **fn declaration's return-type position** and **parameter slots of fn types** (`f: (A) -> B!`). **It is not a type constructor**: `List[Int!]` is invalid. This is the same “function attribute” model as Swift `throws`.
- Denotation: calling a function declared `-> T!` returns `Result[T, String]`. Nothing more.
- **E is fixed to String.** Custom E continues to use explicit `-> Result[T, MyErr]`. (`!` is specifically for fallibility with common E, matching stdlib's uniform String E.)

<a id="d3-宣言人間工学は-fallibility-軸の持ち物"></a>

### D3. Declaration Ergonomics Belong to the Fallibility Axis

Bodies declared `-> T!` (and `-> Result[T, String]`) get **the same** lifting ergonomics as current effect fn: auto-ok for trailing T, auto-`?` on fallible calls, legal `!` and `err(...)`. No capability (E006 family) is added. Thus “effect fn ergonomics” decomposes into “fallibility ergonomics + capability checking.”

<a id="d4--演算子の意味は伝搬のみに固定する"></a>

### D4. Fix the `!` Operator to Propagation Only

`x!` means “convert and propagate x's failure into the enclosing declaration's failure channel.” Without a channel, E022. **Do not put trap/unwrap-or-abort semantics on `!`; do not silently lift signatures** (see Alternatives). Reformulate E022 as a “missing failure channel” family, branching hints by operand type, and give a machine-applicable tail-form hint: `try: fn f(s: String) -> Int! = int.parse(s)`.
Also make E-type mismatches for `!` (String operand → CustomE fn) **check-time errors**. Currently they pass check then ICE. Remove implicit Debug stringification in the reverse direction too, and suggest inserting `result.map_err`.

<a id="d5-移行e040-deprecation-窓の前例を踏襲"></a>

### D5. Migration (Following the E040 Deprecation-Window Precedent)

- **Phase 1, additive and non-breaking:** introduce `-> T!` for pure code; update E022 hints. Update callback slots such as the `try_*` family to `(A) -> B!`, without changing meaning.
- ~~**Phase 2, warning window**~~ **withdrawn (§D6):** permit `effect fn f() -> T!` as a synonym of current effect fn. Warn on effect functions whose bodies are fallible but declare `-> T`; `almide fix` mechanically inserts `!`.
- ~~**Phase 3, semantic reversal**~~ **withdrawn (§D6):** redefine `effect fn f() -> T` as **total**, without lifting. Signatures of `env.args` / `random.int`, etc. become honest, with never-err elision backed by types. Bulk-update stdlib declarations. **Phase 3 timing undecided**, monitor falsifiers first.

<a id="d6-改訂2026-09-24-effect-は可謬を含意する--effect総象限の撤回"></a>

### D6. Revision (2026-09-24): Effect Implies Fallibility — Withdraw the Effect-Total Quadrant

**User-declared `effect fn f() -> T` always means `T!`, lifting to `Result[T, String]`; this is the final form.** Do not perform Phase 2, accepting `-> T!` spelling and warning window (#2563), or Phase 3, reversing `effect fn -> T` to total. The two pure-side quadrants (`-> T` / `-> T!`, Phase 1) remain unchanged.

**Facts as of 2026-09-24, measured by check on 0.63.0 dev:** caller-visible behavior differs by declaration source:

| Callee | Example | Without `!` | With `!` |
|---|---|---|---|
| Stdlib never-err effect fn | `random.float()`, `io.read_line()` | Plain value (`Float` / `String`) | Accepted, no-op |
| User-declared effect fn, even with a non-failing body | `effect fn roll() -> Float = random.float()` | E041 in `let`, E042 as a statement | Accepted, propagation |

Stdlib behavior exposes never-err elision (#840/#841) up through the checker; this revision does not change it. The fern example needs `walk(..)!` because `walk` is a user-declared effect fn; `random.float()` never needed `!`.

Reasons:

1. **It contradicted this ADR's own criterion (D1).** D1 places failures depending on world state in the effect domain. Effect fns by definition touch the world, making them fallible under that criterion. “Effect-total” was a cell outside the criterion.
2. **“Total” for user-declared effect fns is only a syntactic judgment.** #2556's classifier only checks absence of `!` / `err` in the body, not whether the called host can fail due to world state (SIGPIPE, OS randomness source, signal interruption). Stdlib's never-err set is declared after individual runtime handling; user code lacks the same guarantee.
3. **Large downside for MSR.**
   - Edit locality: adding one fallible call to a total effect fn changes its signature to `-> T!` and requires `!` at every caller, cascading upward. Currently all effect fns are already lifted, so the edit stays inside the function.
   - Phase 2 introduces synonymous `-> T` / `-> T!` coexistence, exactly Falsifier 1's concern.
   - Little gain: meaningless `!` disappears at only 37 spec / 7 playground sites. Never-err elision (#840/#841) already captures runtime performance.
   - Low current cost: Dojo runs of 2026-09-22–23 had one missing `!` (E041/E042) among about 1,000 check errors; however, only 48 of 234 records involved tasks with effect fns, so evidence is weak.

Consequences:

- The existing requirement for `!` even on non-failing user effect fns, such as fern's `walk`, remains **specification**. It may look ceremonial to Swift/Rust readers, but is not a source of writer (LLM) errors. A uniform “add `!` when calling effect” rule works: attaching `!` to stdlib never-err calls is also an accepted no-op, so writers need not distinguish callees.
- Keep spelling `effect fn f() -> T`. Requiring `-> T!` is rejected because it repeats information in `effect`, migrates about 880 signatures, and introduces a new “missing `!`” error.
- Never-err elision remains a codegen optimization, not a type property.
- The mirror in fn types, ADR-0009 D1's `effect (A) -> B`, follows the same treatment: an effect bit implies fallibility.

<a id="rationale--なぜそれか"></a>

## Rationale — Why This Choice

<a id="純粋性の理論"></a>

### Purity Theory

Failure values (Result) do not impair referential transparency at all; ⊥ does. Languages with strong purity such as Haskell (`Either`), Elm (`Result`) and Gleam can express failures **only as values**. `-> Int!` preserves purity; trap expands the existing compromise, T6.

<a id="他言語の実測比較2026-08-05-調査"></a>

### Measured Cross-Language Comparison (2026-08-05 Investigation)

Typed-error languages all **reject** propagation/unwrap in a function lacking a failure channel, and use **a separate spelling** for trap:

| Language | Propagation | Propagation without a channel | Trap spelling |
|---|---|---|---|
| Rust | `expr?` | Rejected with E0277: “the `?` operator can only be used in a function that returns `Result` or `Option` (or another type that implements `FromResidual`)” | `.unwrap()` / `.expect()` |
| Swift | `try` + `throws` | Rejected: “errors thrown from here are not handled” | `try!` |
| Zig | `try expr` | Rejected if return type is not an error union. **`fn f() !i32` declares it with one glyph and infers the error set**: “An error set type and normal type can be combined with the `!` binary operator to form an error union type” — Zig Language Reference | `catch unreachable` |
| Gleam | `use` sugar | Structurally impossible | `let assert Ok(x)` |
| Go | No operator | — (`try` proposal [golang/go#32437](https://github.com/golang/go/issues/32437) **declined** over explicitness concerns, a warning about implicit propagation) | None |

`-> T!` takes Zig's “one-glyph declaration” into Swift's “function attribute” model. In Swift `try!` / Kotlin `!!`, **the `!` family means trap**, opposite to Almide's `!` (propagation) intuition. Explicitly state in CHEATSHEET / llms.txt: Almide `!` is Swift `try`, not `try!` (a corollary of D4).

<a id="e-変換"></a>

### E Conversion

Rust automatically inserts `From::from(e)` for `?`, and **missing From causes a check error**. Almide's current one-way ICE / reverse implicit Debug behavior is a degraded version of this model; D4's check-time errors correct it. This ADR does not decide introducing a conversion hook equivalent to From; open question.

<a id="期待できる波及matrix-の実測に基づく"></a>

### Expected Effects (Based on Matrix Measurements)

- Four context-dependent meanings of `!` collapse into one principle, making E022 fixes “one character.”
- `try_map` reads as `fn try_map[A, B](xs: List[A], f: (A) -> B!) -> List[B]!`; the slot's `!` fixes E to String, **structurally eliminating the E025 friction cluster**.
- Two-axis type vocabulary answers effect laundering and “auto-lifted effect fn cannot be passed as callback”: #1055 capability axis × this ADR's fallibility axis.
- The `http.serve` handler slot can be written `effect (HttpRequest) -> HttpResponse!`.

<a id="alternatives--検討して却下した案"></a>

## Alternatives — Considered and Rejected

1. **Silent lifting**, keeping `-> Int` but returning Result: the signature lies, creating two indistinguishable kinds of pure fn. Effect fn is permitted because the `effect` keyword marks it. **Rejected.**
2. **Trap semantics for `!`**, T6 halt on err: callers cannot intercept with `??` / match; extracting an effect-fn body into a pure helper **silently** changes propagation into immediate death. Directly conflicts with MSR, modification survival rate. **Rejected.**
3. **New trap-only `!!` / `assert` spelling:** sensible given typed-error peers all have it, e.g. Rust `.unwrap()`, but `-> T!` satisfies most “want an Int” needs through fallible-context propagation, weakening demand. **Deferred, not rejected.** **Reconfirmed 2026-08-07: remain deferred**, decided/closed in #1133. Adding a new glyph immediately after the v0.54–v0.56 spelling-reduction arc — removing auto-?, seven try_ functions, collect, and `Option[T]`→`T?` — would go in the opposite direction. #1133 states three release conditions: (1) at least one concrete sentinel-wrong-value case or measured Dojo MSR blockage; (2) finalize the precedence table of postfix family `!`/`!!`/`?`/`??`/`?.` together with introduction; (3) simultaneously decide the L9 test-fork resolution, avoiding duplicate work.
4. **Zig-style E inference**, infer `!T` error sets from bodies: polymorphic E conflicts with stdlib's common E=String reality and reduces predictability of displayed types, LLM writability. **Rejected**: fixed String, explicit Result for custom E.
5. **`T?` Option sugar**, Kotlin style: general type notation, not fallibility axis. **Separated as out of scope**, acceptance decided elsewhere.
6. **Status quo**, only explicit `Result[T, String]`: works, C-211, but preserves E022 hint traps (Option suggestion invalid for Result operand), E025 friction and collapsed effect fn properties. Nineteen-character friction directly affects adoption of writing failure in types. **Rejected.**

<a id="consequences--何が良くなり何を払うか"></a>

## Consequences — Gains and Costs

**Gains:** four expected effects above; `effect` / `!` return to one keyword, one meaning, and stdlib signatures become honest in Phase 3. **2026-09-24 revision: Phase 3 withdrawn (§D6). User-declared `effect` implies fallibility as well as capability; “one keyword, one meaning” holds only on the pure side.**

**Costs:**
- Added grammar surface, `!` in return positions and `!` in fn-type slots, and teaching cost.
- **Duplicate notation:** `(A) -> B!` and `(A) -> Result[B, String]` have identical denotation. This strains the 0.53.5 one-name-one-meaning campaign (#1078), but that was two names for one function; this is two notations for one type, like `16` and `0x10`, fixed by one denotation rule. **Which form fmt normalizes to is undecided**, open question.
- ~~Phase 3 migration touches every effect-fn signature~~, withdrawn §D6. Mechanically rewritable, but downstream effects on external packages require the same scanning as E040.
- Adding auto-`?` to pure-fallible imports auto-`?` positional asymmetries into pure code: matrix B issues where position, annotation and pattern shape change behavior. Reforming the positional matrix is **a separate track**, a dependent open question of this ADR.

<a id="falsifier--何が起きたらこの決定を撤回するか"></a>

## Falsifiers — What Would Retract This Decision

1. **Dojo MSR shows coexistence of `T!` / `Result[T, String]` significantly lowers survival relative to one notation.** If duplication is worse than nineteen-character friction, supersede by an ADR dropping `!` notation or retiring Result notation in fn-declaration positions.
2. **Phase 2's warning window finds many effect fns impossible to classify statically as fallible/total.** Phase 3 semantic reversal then fails; fall back to retaining mandatory effect-fn lifting.
   **Measurement, 2026-09-23, #2556, once before filing Phase 2:** `tools/effect_fn_fallibility.py`, driven by `almide --emit-ast`. Classification rules in #2556: `!` / `err(..)` / explicit `-> T!` or `-> Result[..]` = fallible; intrinsic body = unclassifiable; remainder = total. Also prints the mirror of codegen never-err judgment `compute_can_err`, crates/almide-mir/src/lower/mod_p2.rs.

   Intrinsics (`= _`) are split into three columns by how classification resolves, rather than one “unclassifiable” column. The falsifier targets **only opaque**; the other two resolve statically:

   | corpus | effect fn | fallible | total | intrinsic: fallible by signature | intrinsic: total by body | **intrinsic: opaque** |
   |---|---|---|---|---|---|---|
   | stdlib | 174 | 57 | 11 | 82 | 20 | **4** |
   | spec | 792 | 502 | 290 | 0 | 0 | **0** |
   | playground(web/examples, 958ddea) | 21 | 15 | 6 | 0 | 0 | **0** |

   - Fallible by signature: body `= _`, but signature `-> Result[..]`, e.g. fs / http / net / process / zlib.
   - Total by body: self-hosted body in self_host_registry.rs classified total, including `random.*`, `env.args`, etc.; source of fern's `!`.
   - **Opaque:** four `-> T` functions without Almide bodies, `env.set`, `env.sleep_ms`, `http.serve`, `process.exit`, all `-> Unit`/`-> Never`. Decide each declaration as `-> T` / `-> T!` individually.
   - Raw tool output has three classes, unclassifiable =106=82+20+4. Breakdown appears under `unclassifiable refined:` and `--list unclassifiable`.
   Cases spelling `!` but calling never-err callees and thus total in codegen (where Phase 3 removes `!`): spec 37 / playground 7, including fern `walk(..)!` / stdlib 0. One unmeasured spec fn: `spec/gauntlet/cells/s3_module_qualified_protocol`, a parse-rejection cell.
   **○ 2026-09-23: Falsifier 2 did not fire, 4/987 remaining; Phase 2 filed as #2563**, numbers above, judgment in #2556 Exit section.
   **2026-09-24: this falsifier's entire issue disappears:** §D6 withdraws effect-total, so Phase 2/3 are not performed and effect-fn fallible/total classification is unnecessary; #2563 not planned.
3. **#1055 concludes fallibility should be a type constructor, not a function attribute.** This contradicts D2; supersede with an integrated ADR.

<a id="references"></a>

## References

- Measured data: 2026-08-05 operator×context matrix, v0.53.6, 287 cells. Probe: session scratchpad `matrix/`; aggregate `matrix_results.json`.
- Rust: The Rust Reference — [The question mark operator](https://doc.rust-lang.org/reference/expressions/operator-expr.html#the-question-mark-operator), [`std::ops::FromResidual`](https://doc.rust-lang.org/std/ops/trait.FromResidual.html)
- Swift: [The Swift Programming Language — Error Handling](https://docs.swift.org/swift-book/documentation/the-swift-programming-language/errorhandling/)
- Zig: [Zig Language Reference — Errors](https://ziglang.org/documentation/master/#Errors)
- Go: [proposal: Go 2: error handling: try statement](https://github.com/golang/go/issues/32437) (declined)
- Gleam: [Gleam Tour — let assert](https://tour.gleam.run/advanced-features/let-assert/)
- Internal: C-211 / C-029(docs/contracts/contracts.toml), #489, #840/#841, #1051, #1055, #1078
