<a id="architecture-decision-records"></a>

# Architecture Decision Records

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](README.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](README.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](README.zh-CN.md)

> Non-normative reader translation of the [mixed Japanese/English canonical source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/README.md), pinned at 35297a612fb29e3f78f6587aba253aa91d51f466. This snapshot is not automatically synchronized. The original remains authoritative; any normative force belongs to the original, not this reader. Dates, statuses, implementation and validation claims are retained as statements of that source, not independently revalidated. The template code block remains verbatim, including its Japanese text.

Record language-design and architectural **decisions**, their **rationale**, and **rejected alternatives** in chronological order.

<a id="なぜ-adr-か"></a>

## Why ADRs?

`docs/roadmap/` describes “what we will do,” and `docs/specs/` describes “what things are now.” Neither preserves **why the decision was made**. As a result, the same discussion plays out again six months later, and previously rejected proposals return without their rationale. ADRs provide this third axis.

[REJECTED_PATTERNS.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/design/REJECTED_PATTERNS.md) lists “features we will not adopt”; ADRs preserve **the process that led to the decision** (evidence investigated, alternatives compared, and conditions that would falsify it). When a rejection is listed in REJECTED_PATTERNS, it points to an ADR for its rationale.

<a id="形式"></a>

## Format

`NNNN-kebab-case-title.md`. Numbers are sequential; do not rename files (links would rot).

```markdown
# ADR-NNNN: Title in English

- **Status**: Proposed | Accepted | Superseded by [ADR-MMMM](./MMMM-....md)
- **Date**: YYYY-MM-DD
- **Context**: 何が問題だったか（決定の前に読者が知る必要のある事実）
- **Decision**: 何を決めたか（一文で言い切る）
- **Rationale**: なぜそれか（証拠つき）
- **Alternatives**: 検討して却下した案と、その理由
- **Consequences**: 何が良くなり、何を払うか
- **Falsifier**: 何が起きたらこの決定を撤回するか
- **References**: 一次情報の URL
```

The Japanese descriptions inside the unchanged template mean:
- Context: what the problem was—the facts a reader needs before the decision.
- Decision: what was decided, stated decisively in one sentence.
- Rationale: why this choice, with evidence.
- Alternatives: options considered and rejected, with reasons.
- Consequences: what improves and what costs are paid.
- Falsifier: what would cause this decision to be withdrawn.
- References: primary-source URLs.

<a id="規則"></a>

## Rules

1. **Status must be truthful.** If implementation diverges from the decision, revise the ADR or mark it Superseded. An abandoned ADR would repeat SPEC.md §13.
2. **Do not delete rejected proposals.** The reasons for rejection are precisely the asset that prevents the discussion from recurring. If later evidence shows that the rejection was wrong, supersede it with a new ADR and explain **why the previous judgment was wrong**.
3. **A Falsifier is required.** A decision for which you cannot state conditions for withdrawal is a preference, not a decision.
4. **Cite primary sources.** Rather than “it is said that…,” provide URLs and verbatim quotations from specifications, official documentation, or source code.

<a id="読み方と後続決定への案内"></a>

## Reading guide and pointers to subsequent decisions

ADR bodies record the evidence, reasons for rejection, and revision history at the time of the decision. `Accepted` means that the design was adopted, not that every item has been implemented. Read remaining “not implemented” statements and old APIs together with later additions and subsequent ADRs. For the current language specification, see [specs](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/); for public functions, see the [standard library](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/README.md).

The following guidance prevents misreading. It neither changes ADR bodies nor newly adopts or withdraws unresolved designs.

| Record | How to read subsequent decisions and implementation status |
|---|---|
| 0001 | For the name, clock argument, and measurement-cost scope of `fan.race`, see the accepted decisions in [0028](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0028-fan-selection-names-and-cost-contract.md). The decision about time units themselves remains in place. |
| 0002・0003・0012 | For explicit propagation, see [0008](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0008-explicit-propagation-only.md); for typed errors and lambda joins, see [0012](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0012-typed-error-refinement-in-the-marker.md) and [0021](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0021-lambda-failure-channel-is-the-join-of-its-bangs.md). The difference between 0003's strict rejection of `CustomE → String` and the implementation remains recorded in 0021 D5 and Falsifier 3; this index does not resolve it. |
| 0004 | Read the description of `result.context` together with the rejection in D2 and Alternatives 7. Current public functions can be checked in [result](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/result.md). |
| 0012・0021 | Do not conclude that work has not begun solely from the pre-implementation explanation at the beginning. Typed markers and callback propagation are tested in [typed_error_callback_channel_test.almd](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/spec/lang/typed_error_callback_channel_test.almd), among others. |
| 0020・0023 | [0023](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0023-wasm-http-over-wasi-http.md) replaces the export shape in 0020 §5.3. Read the later implementation notes in the body together with the [Wasm build route](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/src/cli/build_wasm.rs). Do not equate implementation of the serve export with completion of every HTTP client route or of host-op retirement. |
| 0025・0026 | For the `process` / `Env` classification in 0025 §2.4, see the later implementation note in 0026 D2 and the [effect category definitions](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/crates/almide-ir/src/effect.rs). The opening statement in 0026 that “everything except fixpoint is unimplemented” describes the state before later additions. |
| References to the external-function boundary in 0026 | “ADR-0027 §5” in the final addition is an unresolved reference. [0027](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0027-effect-functions-are-inspected-in-a-fixed-host-world.md) is a decision about the inspection harness and cannot serve as the basis for the external-function boundary. This index does not infer a replacement norm. |
| 0029 | See Status and the 2026-10-18 ruling in Open. The remaining opening text “questions … still need a ruling” predates that ruling. The rejected test-level attribute proposal remains as a rejected alternative in the body, not as a separate current ADR. |

The number 0013 is used for two different records. To preserve existing links, they are not renamed; the index below distinguishes them by title.

<a id="index"></a>

## Index

| # | Title | Status | Date |
|---|---|---|---|
| [0001](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0001-deterministic-time-units.md) | Deterministic budgets are written in time units | Accepted | 2026-08-01 |
| [0002](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0002-fallibility-effect-orthogonal.md) | Fallibility and effect are orthogonal axes; `-> T!` marks pure-fallible | Accepted (amended 2026-09-24: effect implies fallible, §D6) | 2026-08-05 |
| [0003](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0003-error-type-conversion-at-propagation.md) | Error-type conversion at propagation points — lossy conversion must be spelled | Accepted | 2026-08-05 |
| [0004](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0004-error-branchability-doctrine.md) | Error branchability — String stays terminal; doctrine, lint, and context instead of tags | Accepted | 2026-08-05 |
| [0005](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0005-operators-desugar-to-stdlib.md) | Value-level operators are desugarings of named stdlib functions | Accepted | 2026-08-05 |
| [0006](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0006-fallibility-polymorphic-hofs.md) | One-bit fallibility polymorphism for HOFs; the try_* family is dissolved | Accepted (amended 2026-09-27: E flows from the callback, D4 withdrawn — ADR-0021) | 2026-08-05 |
| [0007](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0007-partition-replaces-collect.md) | result.collect is removed; partition is the substance for all-errors collection | Accepted | 2026-08-05 |
| [0008](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0008-explicit-propagation-only.md) | Propagation is always explicit — auto-? abolished, `!` only, Result is must-use | Accepted | 2026-08-05 |
| [0009](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0009-fn-type-quadrant-transparency.md) | Function types complete the capability quadrant; fn params are transparent by default | Accepted | 2026-08-06 |
| [0010](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0010-option-type-shorthand.md) | T? is general Option shorthand — every type position, atom-tight binding, fmt-normalized | Accepted | 2026-08-06 |
| [0011](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0011-execution-substrate-is-a-free-variable.md) | The execution substrate is a free variable — and one arm's output is the last hole in the determinism claim | Proposed (amended 2026-10-04: wasm compute parallelism is per instance, memory never shared, §D2a) | 2026-08-07 |
| [0012](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0012-typed-error-refinement-in-the-marker.md) | Error-surface end state — refinement stays in the marker (`T!E`), erasure stays the default | Accepted (D4-4 replaced by ADR-0021) | 2026-08-11 |
| [0013](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0013-corpus-stays-id-keyed.md) | The conformance corpus stays contract-ID-keyed; semantic reorganization rejected, naming discipline gated | Accepted | 2026-08-20 |
| [0013 (String)](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0013-string-shared-immutable-buffer.md) | String becomes a shared immutable buffer; mutating operations use copy-on-write | Accepted | 2026-09-04 |
| [0014](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0014-expectations-are-derived.md) | Pinned expectations are machine-derived or absent — never hand-maintained | Accepted | 2026-08-20 |
| [0015](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0015-reference-evaluator-is-fresh-source-level-python.md) | The reference evaluator is a fresh, source-level, judge-owned evaluator behind a black-box protocol — seeded by λ_almd, ratcheted by an abstain ledger (amended: Rust) | Accepted | 2026-08-21 |
| [0016](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0016-numeric-determinism-is-normative.md) | Numeric determinism is one normative family, written once — not a property the targets happen to share | Accepted | 2026-08-21 |
| [0017](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0017-flight-profile-is-a-normative-subset.md) | The bounded profile (née flight profile) is a normative subset of the language, specified here before any checker enforces it (amended at adjudication: `@bounded`, chapter `bounded.md`, prefix `B`) | Accepted | 2026-08-21 |
| [0019](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0019-suppression-is-announced-dated-reasoned-acknowledged.md) | Suppression is announced, dated, reasoned and acknowledged — its shape is fixed before the construct exists | Accepted | 2026-09-22 |
| [0020](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0020-http-serving-and-handler-state.md) | A served handler cannot reach a `var`; state lives in a host-independent KV store; the app is a value and every host is a thin shell | Accepted (§5.3 export shape superseded by ADR-0023) | 2026-09-27 |
| [0021](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0021-lambda-failure-channel-is-the-join-of-its-bangs.md) | A lambda's failure channel carries the error type its `!`s agree on — contextual type first, then the join, String as the top | Accepted | 2026-09-27 |
| [0022](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0022-output-and-abort-builtins-are-admissible-in-a-pure-fn.md) | The output and abort builtins are admissible in a pure fn — writes and aborts, never reads, and they do not make the caller effectful | Accepted | 2026-09-27 |
| [0023](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0023-wasm-http-over-wasi-http.md) | Wasm HTTP is `wasi:http@0.3` — a program that uses HTTP builds as a p3 component, the embedded host serves the same imports, and no host serves a custom HTTP op | Accepted | 2026-09-28 |
| [0024](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0024-fan-effect-callbacks-run-concurrently-as-if-sequential.md) | `fan` runs effect callbacks concurrently on every target, as if sequential — `fan.map` runs every element, the compiler picks the substrate, and no concurrency concept reaches the writer | Accepted | 2026-09-30 |
| [0025](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0025-wasm-process-is-a-private-host-capability.md) | Wasm `process` is a private host capability — `almide:process/spawn`, shipped only when the op set names it, refused at load by a stock host, bounded by `[permissions] proc` | Accepted | 2026-10-03 |
| [0026](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0026-effect-categories-ride-the-transparent-fn-type.md) | Effect categories ride the transparent fn type — sets flow through callbacks unannotated, a two-level hierarchy, and inferred ⊆ manifest ⊆ host is one chain | Accepted (D1–D4) | 2026-10-03 |
| [0027](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0027-effect-functions-are-inspected-in-a-fixed-host-world.md) | Effect functions are inspected in a fixed host world — a seeded sandbox, property verdicts for what the host decides, and loopback fixtures for the network | Accepted (D1–D8, 2026-10-17) | 2026-10-17 |
| [0028](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0028-fan-selection-names-and-cost-contract.md) | Replace `fan.any` with `??` / `list.find_map`; rename `fan.race` to `fan.min` with a required clock; scope cost to a meter version | Accepted | 2026-10-18 |
| [0029](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0029-assert-aborts-runs-its-closure-in-a-child.md) | `testing.assert_aborts(expected, closure)` runs only its closure, in a child process or a fresh instance — the closure captures nothing, every abort counts, and `testing.assert_throws` leaves at dialect epoch 14 | Accepted (2026-10-18) | 2026-10-17 |
| [0030](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0030-compile-cost-is-a-contract.md) | Compile cost is a contract — every phase has a counted budget, the source cannot raise it, and exceeding it is a defined diagnostic | Accepted | 2026-10-24 |
| [0031](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0031-raw-pointers-carry-provenance.md) | A pointer the program can read through carries its provenance — `RawPtr` splits into an opaque `Handle` and a bounds-checked `Region`, and a foreign pointer is a `Region` only where its `@extern` states the length | Accepted | 2026-10-27 |
| [0032](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0032-a-response-content-type-is-its-body-type.md) | A response's Content-Type is its body's type — `Body` gives each body type its media type, header writes cannot name Content-Type, and a script-capable type is built only by its safe module | Accepted | 2026-10-29 |

## Source caveats

The source itself distinguishes adoption from implementation, records unresolved differences and references, and preserves two entries numbered 0013. This reader keeps those distinctions and the missing 0018 unchanged; it does not resolve them, infer an external-function norm, or turn historical implementation notes into current guarantees.
