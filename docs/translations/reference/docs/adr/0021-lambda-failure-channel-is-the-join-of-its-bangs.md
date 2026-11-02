<a id="adr-0021-a-lambdas-failure-channel-carries-the-error-type-its-s-agree-on--contextual-type-first-then-the-join-string-as-the-top"></a>

# ADR-0021: A lambda's failure channel carries the error type its `!`s agree on — contextual type first, then the join, String as the top

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](0021-lambda-failure-channel-is-the-join-of-its-bangs.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](0021-lambda-failure-channel-is-the-join-of-its-bangs.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](0021-lambda-failure-channel-is-the-join-of-its-bangs.zh-CN.md)

> Non-normative historical reader snapshot. Unchanged pinned original authoritative; no automatic synchronization, independent corpus sweep, benchmark rerun or proof of current guarantees. Initial implementation-not-started status and staged plans coexist with a dated #2725 implementation note. D2/D6 byte-unchanged output claims are qualified by that note: native repr differs for Option, Float and control-character escapes. The existing-corpus zero counts are scoped measurements, not all possible programs; the same attempt can contribute to multiple occurrence rows. Join totality concerns synthesis of the channel only, not absence of all typing errors. ADR-0003 text versus implemented E-to-String behavior remains explicitly unresolved. Cross-language runtime and success claims are source-dated, not newly verified. [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0021-lambda-failure-channel-is-the-join-of-its-bangs.md)

- **Status**: Accepted, design ruling; implementation not started, stages in D7.
- **Date**: 2026-09-27
- **Scope**: determine E of the lambda's own failure channel receiving body `!` propagation; typing rules, defaults, diagnostics/fix-its, relation to `__fallible_*` canonical rewrites, migration impact, implementation stages.
- **Related**: [ADR-0006](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0006-fallibility-polymorphic-hofs.md), **revise D1's fixed-String E and D4**; [ADR-0009](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0009-fn-type-quadrant-transparency.md), D5's #489 invariant unchanged; [ADR-0012](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0012-typed-error-refinement-in-the-marker.md), **replace D4-4**; [ADR-0003](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0003-error-type-conversion-at-propagation.md), unchanged, propagation rules apply directly to lambda channels; L3 in `docs/specs/result-option-effect.md`, fixed-String E revised here; #2601 origin; PR #2714 naming callbacks in E022.
- **Background**: #2601 ruling on 2026-09-27. Implementation offered A, current behavior + good E022; B, extend canonical forms to tail leaves; C, infer channel E. Decided using live probes of eight languages, Rust/Swift/Go/TypeScript/OCaml/Zig/Lean/MoonBit, primary sources, an Almide whole-corpus sweep, and dojo bank-pilot output. Research notes: `almide-references/RESEARCH-typed-errors-in-closures.md`.

<a id="context"></a>

## Context

A lambda is a mini fallible fn. Body `!` falls into the lambda's own failure channel, ADR-0006 D1/#489, never crossing closure boundaries. Today that channel is **fixed to `Result[_, String]`**: `infer_expr_g3_lambda` creates `Ty::result(fresh, Ty::String)`. Variant-E `!` operands silently fall into it as Debug strings.

One exception: when the entire callback body passed to a `list.*` / `fs.*` fallible HOF is **exactly** `call(..)!`, `normalize_fallible_hof_callback` strips the marker and routes through `__fallible_*`. The call's own `Result[_, E]` becomes the callback type unchanged. Only this spelling preserves E.

<a id="実測-現状の型は綴りで変わるalmide-0640almide-check"></a>

### Measurement: Current Types Depend on Spelling, Almide 0.64.0, `almide check`

`step: (Int, Int) -> Int!E`, enclosing fn `-> Int!E`, `xs |> list.fold(0, CB)!`:

| CB spelling | Result |
|---|---|
| `(acc, x) => step(acc, x)!` | ok, preserves E |
| `(acc, x) => { step(acc, x)! }`, only adding braces | **E022** |
| `(acc, x) => x \|> step(acc)!`, only rewriting as pipe | **E022** |
| `(acc, x) => if x == 100 then acc else step(acc, x)!` | **E022** |
| `(acc, x) => { let v = step(acc, x)!; v + 1 }` | **E022** |
| `(n) => step(n)!` into user HOF typed slot `f: (Int) -> Int!E` | **E005**, even canonical form fails |
| `let f: (Int) -> Int!E = (n) => step(n)!` | **E001** |

Meaning-preserving edits, braces/pipe/one `let` line, change a HOF call's type from `Result[_, E]` to `Result[_, String]`, reporting the error at distant `)!`. ADR-0012 D2 permits `T!E` in fn-type slots, yet not one lambda with `!` can inhabit them. This is exactly an MSR defect.

Two further measured harms:

- **A check-passes/rustc-fails hole remains in 0.64.0.** Consuming map with `match`, rather than `)!`, lets `(x) => match x { 0 => 0, _ => step(0, x)! }` pass `almide check`, then rustc fails with E0277 + E0308, wasm with E082, `ty-mismatch:Scalar(Str)-vs-Named(0)`. Only #2601's `)!` form becomes E022 after #2660.
- **Only native accepts erasure.** Typed `!` in a lambda within a `-> Int!` fn prints `err Neg(-2)` and passes on native; both wasm legs reject with `unwrap-err-ty-mismatch`, E082.

<a id="実測-llm-はこの形を書き全部落ちる"></a>

### Measurement: LLMs Write This Shape, and All Fail

Aggregated dojo bank pilot, run 36083949429, 2026-09-25, compiler 0.63.1. In typed-error tasks, 66 attempts put variant-E calls with `!` in callbacks, all llama-3.3-70b:

| Position | Occurrences | Failures |
|---|---:|---:|
| Canonical, whole body `f(..)!` | 36 | 30 |
| Mid-block `let v = f(x)!` | 26 | 26 |
| if/match tail leaf | 16 | 16 |

**37 attempts contained at least one noncanonical typed `!`; all 37 failed.** The model introduced the shape itself in 14: label-orders 6, ignore-expired-coupons 4, restore-isbn 3, retired-signing-key 1. Typically it expands baseline canonical `(cart, code) => apply_coupon(book, cart, code, today)!` into `{ let c = find(book, code)!; ... }` to make an edit. Exactly the edit-breakage path MSR measures. Three human bank baselines also contain it and do not compile on 0.64.0: `early-recovery/{drop-exact-repeats-of-a-reading, reorder-events-with-small-clock-skew,
merge-overlapping-shifts-in-the-timesheet}`. #2601 arose while authoring this bank.

<a id="実測-既存コードへの露出almide---emit-ast--解析器"></a>

### Measurement: Existing-Code Exposure, `almide --emit-ast` Plus Analyzer

Counted lambdas outside test blocks whose own bodies, excluding nested lambdas, contain `!`:

| Corpus | Count | Canonical | Tail leaves only, covered by B | Mid-block, C only | Variant-E `!` |
|---|---:|---:|---:|---:|---:|
| `spec/` | 109 | 79 | 11 | 19 | 0 |
| `stdlib/` | 2 | 0 | 2 | 0 | 0 |
| `tests/` + `examples/` | 18 | 14 | 0 | 4 | 0 |
| `research/benchmark/exercises/` | 0 | – | – | – | – |
| almide-dojo(main: tasks/msr/src/bank) | 6 | 6 | 0 | 0 | 0 |
| almai / comide / golemide / gramide-cli | 0 | – | – | – | – |
| **Total** | **135** | **99** | **13** | **23** | **0** |

Eleven operands had types not decidable from syntax alone; manually traced all to String, closure parameters of `Handler` / `HttpHandler`, `Order.total -> Money!`. Of 36 noncanonical forms, only 13, 36%, are tail leaves; 23, 64%, are mid-block. In bank outputs, 26 of 42 noncanonical occurrences, 62%, were mid-block.

<a id="survey--他言語は-closure-内の伝搬演算子の-e-をどう型付けるか"></a>

## Survey — How Other Languages Type E of Closure Propagation

Live probes, Rust 1.96.1 / Swift 6.3.3 / Go 1.27rc2 / TypeScript 5.9.3, and primary sources. Full probes, raw output and details are in the research notes.

| Language | E inside closure | Without constraints | Annotation needed when | Failure diagnostic, verbatim |
|---|---|---|---|---|
| Swift 6.3.3 | Infers whether it throws; **type only from annotation/context** | `any Error`, top | Concrete E desired without context, every time | `thrown expression type 'any Error' cannot be converted to error type 'E'`, on outer `try`, probe |
| Rust 1.96 | Contextual inference, `?` via `From` | None, E0282 | `let` closure without return-type context | `error[E0282]: type annotations needed … cannot infer type of the type parameter `E`` + `Ok::<i64, E>` suggestion, probe |
| MoonBit | Arrow `x => …` **infers precisely from body**, `raise Bad`; `fn` literal inference deprecated | Bare `raise` = `Error` | Unnecessary for arrow | `[4122] Function with error can only be used inside a function with error types…`, probe |
| OCaml 5.5 | `let*` inferred by HM, same E only; exceptions/effects absent from types | Type variable | Never | `The constant 42 has type int but an expression was expected of type string`, probe |
| Gleam | `use` anonymous fn inferred by HM, same E only | Type variable | Never | `Type mismatch … Expected type: Result(Int, Bool)` |
| Lean 4.29 | Lambda monad from expected type or body action | None, metavariable | `throw` alone without context | `typeclass instance problem is stuck MonadExcept String (?m.18 s)`, probe |
| Koka / Roc / jacquard | Infer/compose open rows or open tag unions | Open row/union | Almost never | `effects do not match … inferred effect: <…\|_e>`, Koka |
| Zig 0.16 | No closures; nested-fn error sets inferred, fn-pointer types explicit sets | Inferred set | Always for fn-pointer arguments | `function type cannot have an inferred error set`, probe |
| Go 1.27 / TypeScript 5.9 | No typed error channel, fixed `error` / `unknown` | Top | – | `TS2322: Type 'unknown' is not assignable to type 'Bad'`, catch side, probe |
| aver / vera / vibe / wado | aver/sui lack lambdas; vera requires effects annotation; vibe/wado infer from body | – | Always in vera | – |

Swift SE-0413 Future directions states precisely this rule: infer a closure's thrown type as `errorUnion(E1, …, EN)` of all uncaught throw sites. But existing code depends on closure `any Error`, so put it behind upcoming feature flag `FullTypedThrows` for source compatibility. Revision 6 records “Closure type inference did not get implemented in Swift 6.0”; compiler test `test/expr/closure/typed_throws.swift` says “We do not infer thrown error types from the body, because doing so would break existing code.” **Swift does not withhold inference because the rule is defective, but because it changes existing code's types.** Almide measured zero such existing cases, Context exposure table.

Four findings:

1. **HM/effect languages, OCaml/Haskell/Koka/Lean/Roc/Gleam, infer closure E from the body** without annotations. ADR-0012 D4-4's feared inference-error difficulty actually belongs to Koka row variables and Roc open/closed unions, not E inference itself.
2. **Context-derived E, Rust, fails without context.** E0282 arises because `?` goes through `From`, so operands do not determine E. Almide has no `From`, ADR-0003 D4; operand E directly supplies the candidate, structurally avoiding this failure class.
3. **Swift occupies the same position as current Almide and itself calls it unfinished.** Unannotated closure thrown types fall to `any Error`, top. Put typed `try` in a closure, then `try xs.map { ... }` from a `throws(E)` fn: `thrown expression type 'any Error' cannot be converted to error type 'E'` appears **on outer `try`**, same location/structure as Almide E022 before PR #2714. Body inference, FullTypedThrows, remains experimental even in 2026's 6.3.3, not enableable in production compilers. Swift has an escape via closure `throws(E)` annotations. Almide has no lambda return annotation, hence no escape.
4. **The newest LLM-era languages lean toward body inference.** MoonBit precisely infers arrow-lambda E, `(String) -> Int raise Bad`, and deprecated non-inferring `fn` literals. Vibe and wado infer closure effects from bodies too.

<a id="decision"></a>

## Decision

**Replace fixed-String lambda-channel E with a determined type: contextual expected type first; otherwise join E from body `!` operands. Default and top are String. Canonical `__fallible_*` stripping loses its typing role and becomes a lowering optimization.**

<a id="d1-型付け規則l3-の改訂"></a>

### D1. Typing Rules, Revising L3

Let lambda λ's failure channel be `Result[T, ε]`. Determine ε in this order:

1. **Checking mode, expected type has E**: when checking λ against `(A) -> B!E` / `(A) -> Result[B, E]`, ε := E. Sources: user HOF typed slots, `let f: (A) -> B!E = ...`, and value tails in fns returning `-> (A) -> B!E`. Each body `!` matches ε by exactly fn-body rules, `bang_error_channel`, ADR-0003 D1/D2. Matching propagates unchanged; String, Option, non-Result effect calls or another E' produce E022 at that `!`.
2. **Synthesis mode, expected type lacks E**, builtin `list.*` / `fs.*` HOFs and unannotated `let`: ε := ⊔ { err(o) | o is a `!` operand in λ's own body }. Here err(`Result[_, E]`) = E; err(`Option`) = String, none → err("none"), ADR-0003 D3; err(non-Result effect call) = String. ⊔ is least upper bound under the current implicit-conversion order, any E ≤ String: **all operands same E gives E; any disagreement gives String**.
3. **Default**: channels without `!` operands, e.g. effect-slot lambdas, or containing operands with unresolved E on leaving λ, use String. Both are today's behavior.

`!` falls into the innermost lambda channel, #489 / ADR-0009 D5 unchanged. `!` inside test blocks remains unwrap without a channel, L9 unchanged.

Three properties remove ADR-0012 D4-4 concerns:

- **⊔ is total**: top String prevents synthesis typing from failing. No row variables or generalization; zero new inference-error message kinds.
- **Acceptance preserved**: every currently checking lambda still checks. ε changes String→E only when all `!` operands agree on E.
- **Spelling-independent**: braces, pipes, extra `let`, movement into if/match leave operand sets and ε unchanged.

<a id="d2-観測可能な変化は-1-種類だけ"></a>

### D2. Only One Kind of Observable Change

A HOF supplied a lambda whose ε changes String→E changes type `Result[_, String]` → `Result[_, E]`. Three downstream consumption cases:

- `)!` in a `-> T!E` fn: **accepted**, requested #2601 fix.
- `)!` in a `-> T!` fn: accepted via fn-level E→String Debug conversion, with **byte-identical printing to today**. Only the location taking Debug of the same E value moves from inner `!` to outer `!`.
  (2026-09-27 implementation note, #2725: conversion strings were aligned to repr, printing `"${e}"`, rather than Rust `Debug`. Same bytes for `Neg(-2)` / `Bad("x")` / records / nested variants, but differ for Option, `Some(2.0)` versus `some(2)`; Float, `1e20` versus `100000000000000000000`; and control-character escapes. Only repr was reproducible by wasm/interp. Native printing changes only for these three forms.)
- Value-position consumption, e.g. `match r { err(e) => string.len(e) }`: only this changes the consumed type. **Zero cases in all existing corpora**, because the exposure table has no lambdas containing variant-E `!`.

<a id="d3-診断と-fix-it"></a>

### D3. Diagnostics and Fix-Its

Inference of ε itself cannot fail, D1. Diagnose four situations instead:

1. **Checking-mode operand disagrees with ε**: E022 at that `!`. Reuse fn-body wording/hints, `bang_error_channel`, with subject “the callback's error type is `E` (from the slot `f: (Int) -> Int!E`)”. Use ADR-0003 D2's directional canonical fix-its, filling the name if exactly one constructor carries String, such as `int.parse(s) |> result.map_err((e) => BadValue(e))!`. Today's E005/E001, “passed fn(Int) -> Result[Int, String],” disappear.
2. **Synthesis join falls to String and causes E022 at `)!` in a `-> T!E` fn**: extend PR #2714's E022 to **name both disagreeing operands**:
   ```
   error[E022]: operator '!' cannot propagate this error: the fn's error type is `ShiftErr`,
     but the callback's `!`s fail with different types — `ShiftErr` (line 43, col 20)
     and `String` (line 44, col 31) — so its channel is `String`
     hint: convert the odd one at its `!` so every `!` in the callback fails with `ShiftErr`:
       int.parse(raw) |> result.map_err((e) => BadNumber(e))!
   ```
3. **Unresolved operand defaults to String**: suggest binding the callback to a typed `let`, `let step_cb: (Acc, Line) -> Acc!ShiftErr = (s, l) => ...`, entering checking mode. Add no lambda-return-annotation syntax; existing fn-type annotations fill Swift's `throws(E)` role.
4. **Check-passes/rustc-fails hole, Context**: after D1, ε matches HOF-result type, structurally closing it. Close it earlier with E022 in D7 step 1, #2722.

<a id="d4-__fallible_-と正準形剥がしの格下げ"></a>

### D4. Demote `__fallible_*` Canonical Stripping

Today `normalize_fallible_hof_callback` does (a) reroute `list.map`, etc. to `__fallible_map`, etc.; (b) strip canonical markers to preserve E. After D1, (b) is unnecessary for typing: synthesized ε gives the same E and `__fallible_*` carriers already are E-generic. Canonical typed folds already print `ok 3` / `neg -2` on both native and wasm, evidence of this. Keep (a). A/B (b) using allocation ledger, size ratchet and performance: remove if no difference; otherwise retain as lowering optimization. Types are identical either way.

<a id="d5-他-adr-との関係明示の改訂"></a>

### D5. Relations to Other ADRs, Explicit Amendments

- **Replace ADR-0012 D4-4.** Withdraw “lambda channels remain String, ADR-0009 L3; use-driven E inference imports Koka-like inference errors.” Basis: D1 join is total without row variables, Survey 1/2, and the explicitly annotated Result escape does not exist, last two Context rows. Cited “ADR-0009 L3” does not exist in ADR-0009: L3 belongs to L1–L9 in `docs/specs/result-option-effect.md`, ratified 2026-08-07. Amend that L3.
- **Revise ADR-0006 D1's E fixed to String per ADR-0002 D2; withdraw D4.** HOF E flows from callback ε, transparent like the fallibility bit. D4, no E-generic traverse support, was already de facto broken by canonical paths. Falsifier 3, ≥3 real custom-E-traverse demands, fired: three human dojo baselines, 14 model-introduced attempts, and two shapes in #2601/body comments.
- **ADR-0009 unchanged.** D5's lambda `!` stays in its own channel; this ADR determines its E only. D2 transparency extends to E in the same way.
- **ADR-0003 unchanged.** Each lambda `!` matches ε under the same “`!` does not convert E” fn rule. One implementation note: ADR-0003 D2 says CustomE→String is a check error, but #2635 implementation, opening doc comment of `bang_error_channel.rs`, accepts fn-level Debug conversion. D1 join uses this **implemented** E≤String order. If D2 is literally enforced later, the order loses top and synthesis disagreement becomes D3-2 E022 + map_err fix-it rejection. The contextual→agreement→default framework survives.

<a id="d6-移行影響"></a>

### D6. Migration Impact

- Acceptance reduction: **0**, D1 preservation.
- Existing HOF calls changing type across spec/stdlib/tests/examples/exercises/dojo main/almai/comide/golemide/gramide-cli: **0**.
- Newly accepted: #2601's four shapes and two comment shapes; three human bank baselines; type-error portions of 37 failed noncanonical typed bank attempts, other errors separate; lambdas with `!` in typed slots / typed `let`.
- Contract ledger: native observable output unchanged, D2. Wasm accepts formerly E082-rejected forms, so pin one new cross-target contract and fixture. Allocate C-NNN first in almide/als, CLAUDE.md ordering.

<a id="d7-実装段階1-段階-1-pr各段階に-issue--すべて-2601-配下"></a>

### D7. Implementation Stages, One PR and Issue Each, All Under #2601

1. **Close the hole first**, #2722: map consumed with `match`, check passes→rustc E0277, becomes E022 under current rules. Extend #2714 `lambda_err_erasures` from `)!` to HOF value consumption. Bug fix, do not wait for D1.
2. **Checker: synthesize ε**, #2723: make `lambda_ret` E fresh, record operand E, join at λ exit, default String. Checking mode supplies expected E. Remove typed-slot E005/E001 refusal. Include spec L3 revision and pins in `spec/lang/fallible_lambda_test.almd`.
3. **Rust lowering**, #2724: emit ε≠String lambdas as `Result<_, E>` closures with nonconverting `?`. Keep today's Debug `map_err` for joins falling to String, `pass_lambda_type_resolve.rs` / `walker/expressions_control.rs`. Verify warning-free generated Rust compilation.
4. **Both wasm legs and interp**, #2725: route structural `lower_try_unwrap` through ε matching past `unwrap-err-ty-mismatch`. Incumbent and almide-interp also read ε. Add three-way oracle fixture and new C-NNN, als first.
5. **Diagnostics**, #2726: D3-2 names both operands, D3-3 typed `let` hint, broken/fixed pairs in `tests/diagnostics/`.
6. **A/B and demote canonical stripping**, #2727: compare D4(b) by allocation ledger, size ratchet and perf, then remove or keep as lowering optimization.
7. **Docs**, #2728: revise L3/non-goal 4 in `docs/specs/result-option-effect.md`. Add callback examples in CHEATSHEET / llms.txt with `almide check` fences only after implementation ships, #1483 rule.

<a id="rationale"></a>

## Rationale

- **Directly addresses MSR**: today's meaning-preserving brace/pipe/`let`/if edits change types. The bank's 37/37 failures show models actually take that path. D1 ε depends only on operand sets, not spelling.
- **B is insufficient**: tail-leaf extension covers 36% of existing noncanonical forms, 38%, 16/42, of bank noncanonical typed occurrences. Remaining mid-block forms are **inevitably** traversed when models expand canonical forms to edit them, ignore-expired-coupons example. B only widens canonical syntax one level, retaining spelling-dependent types.
- **C costs less than ADR-0012 estimated**: feared inference difficulty belongs to rows/unions, absent in a two-level join. Zero migration cases.
- **Swift is a cautionary example**: it added annotations as an escape to the same no-inference/top-default design while keeping inference experimental. Almide has no such escape, so staying there means unexpressible. Swift's reason is source break from changing existing closure types, Survey; Almide measured zero affected cases. D1-2 join instantiates SE-0413 Future directions' `errorUnion` on Almide's two-level E≤String order.

<a id="alternatives--検討して却下した案"></a>

## Alternatives — Considered and Rejected

1. **A: status quo + #2714 E022**: better diagnostics, but six of seven Context forms remain unexpressible. Even canonical `!` lambdas fail typed slots and typed `let`, emptying ADR-0012 D2's promise. Bank 37/37 failure unchanged. **Rejected**.
2. **B: extend canonical forms to tail leaves**: local, no inference, but only covers 36–38%. Merely pushes spelling-dependent typing one step back, leaving the mid-block `let` boundary at the most-used edit location. **Rejected**.
3. **C-strict: always error on disagreement, no implicit String join**: matches ADR-0003 D2's text, but while fn implementation accepts E→String, strict lambdas create a new asymmetric `!` rule and shrink acceptance. Move together when D2 is enforced, D5 note. **Deferred**.
4. **Add lambda-return annotations like Swift `throws(E)`**: existing fn-type annotations, `let` / slot, suffice to enter checking mode. New syntax means one meaning/two spellings. **Rejected**.
5. **Only make HOFs E-generic, leave lambdas String**: carriers already are E-generic; only lambda side is missing. **Not viable**.

<a id="consequences"></a>

## Consequences

**Gains**: spelling-independent callback types; usable typed slots, completing ADR-0012 D2; structural closure of check-passes/rustc-fails holes; human typed-error bank baselines compile unchanged; native/wasm acceptance alignment.

**Costs**: four implementation areas, checker fresh E/operand recording/exit join, Rust closure types, both wasm legs, interp. Seven D7 stages. Zero existing type changes, no manual migration.

<a id="falsifier"></a>

## Falsifier

1. **After D7 step 2, at least three spec/dojo cases where ε becoming E makes value consumption a type error**: withdraw synthesis, retain checking only, default synthesis back to String.
2. **Unresolved operands defaulting to String at λ exit become the main cause of typed-error E022 failures in dojo**: delay join until fn-level resolution, implementation change, rule unchanged.
3. **ADR-0003 D2, CustomE→String check errors, is enforced**: replace D1-2 disagreement→String with disagreement→E022 + map_err fix-it, move to C-strict.

<a id="references"></a>

## References

- Research notes: `almide-references/RESEARCH-typed-errors-in-closures.md`, full probes, raw output, analyzer, bank-pilot aggregation.
- Swift SE-0413 — [Typed throws](https://github.com/swiftlang/swift-evolution/blob/main/proposals/0413-typed-throws.md)
- Rust — [E0282](https://doc.rust-lang.org/error_codes/E0282.html), [`?` and `From`](https://doc.rust-lang.org/reference/expressions/operator-expr.html#the-question-mark-operator)
- Internal — #2601, PR #2714, #2635 (`bang_error_channel.rs`), #2660, `crates/almide-frontend/src/check/infer_calls_closures.rs` (`infer_expr_g3_lambda` / `accept_lambda_channel_prop` / `normalize_fallible_hof_callback`), `crates/almide-wasm/src/data.rs` (`unwrap-err-ty-mismatch`), `docs/specs/result-option-effect.md` (L1–L9), almide-dojo bank pilot run 36083949429.
