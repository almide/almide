# Almide Roadmap

> Auto-generated from directory structure. Run `bash docs/roadmap/generate-readme.sh > docs/roadmap/README.md` to update.
>
> [GRAND_PLAN.md](GRAND_PLAN.md) — 5-phase strategy
> [ROAD_TO_1_0.md](ROAD_TO_1_0.md) — 0.41 → 0.99 version ladder

## Active

82 items

| Item | Description |
|------|-------------|
| [AlmidePerceusBelt](active/almide-perceus-belt.md) | AlmidePerceusBelt — formal memory safety guarantee for Almide |
| [Async Inception — logical time as the language's time base](active/async-inception.md) | The async inception: thesis, grammar, semantics, proofs, claim and plan in one charter |
| [The 2026 Async Claim — audit and the runaway plan](active/async-world-claim.md) | Audit of the best-async-of-2026 claim: the axis, the rivals, the five runaway moves |
| [Behavioral Contract — 機械生成コードの「機能正しさ」を、王者の土俵を避けて取る層](active/behavioral-contract.md) | 機能正しさを王者(Dafny/F*/SPARK/Lean)の土俵を避けて取る層。C-FAITHFUL の上に C-PRESERVED(差分保存)と C-ASSERTED(批准性質)を積む。仕様の出所を機械生成レジームへずらす戦略。 |
| [Build Speed: Runtime rlib + Hot-Fn Inlining](active/build-speed-runtime-rlib.md) | Native build-speed — precompiled almide_rt runtime rlib, and recovering the shipping-build inlining gap with #[inline] |
| [Certificate Format v1 — design](active/certificate-format-v1.md) | The per-function ownership-certificate format — the i/a/d/m alphabet, call/branch/closure extensions, and which bricks have shipped |
| [Certification-Grade Hardening — 認証級への硬化](active/certification-grade.md) | Certification-grade hardening — adopt the mechanisms of DO-178C / ISO 26262 / IEC 61508 (spec, traceability, coverage, tool qualification, dossier) for the machine-written-software trust layer |
| [CI Warnings Cleanup](active/ci-warnings-cleanup.md) | Standing ledger of compiler warnings CI tolerates, and the plan to drive each class to zero |
| [Closure Architecture v2](active/closure-architecture-v2.md) | Closure Architecture v2 — one identity, one capture-set, lifting is lowering; separates closure REPRESENTATION from the inlining OPTIMIZATION |
| [Closure cross-target completeness](active/closure-cross-target-completeness.md) | Closing the closure feature gap between the native and wasm legs, sweep by sweep |
| [Codegen traversal totality](active/codegen-traversal-totality.md) | Making every codegen traversal exhaustive by construction, so a new IR node cannot be silently skipped |
| [Completeness by Construction](active/completeness-by-construction.md) | Design note: borrowing Perceus / borrow-checker style mechanical completeness guarantees for Almide's own passes |
| [Concurrency stance: deterministic data-parallelism](active/concurrency-stance.md) | The concurrency model decision and the fan-family fixes it settles |
| [Correctness Guarantee Gaps](active/correctness-guarantee-gaps.md) | Which layers of the well-typed-source-to-correct-binary chain still lack a mechanical or mathematical proof |
| [Cross-Target Completeness (the Lid)](active/cross-target-completeness.md) | Cross-target completeness lid — the staged path from "all known divergences fixed + byte-diff gate" to structural equivalence (drain → interpreter+fuzz → selfhost → kernel proofs), with the live drain queue |
| [The Determinism / Purity Belt](active/determinism-belt.md) | Determinism/Purity Belt — a Perceus-analog that makes the compiler deterministic & target-portable by construction |
| [Deterministic Bounds — legalising a bound on computation under byte-identity](active/deterministic-bounds.md) | Legalising fan.timeout under byte-identity — fuel, deterministic allocation, winner order, effect isolation |
| [Capability-Based Effect System](active/effect-system-capability.md) | Capability-based effect system for sandboxed AI agent containers |
| [Blueprint — making the ~27 effectful / raw-pointer stdlib fns FUNCTIONAL in v1](active/effectful-27-blueprint.md) | Blueprint for making the ~27 effectful / raw-pointer stdlib fns functional on the v1 leg, split by real cause |
| [Execution Inception — as-if 規則を、並行と target の向こうまで完成させる](active/execution-inception.md) | The execution inception: the as-if rule completed across concurrency and targets |
| [Fan v2 — reference examples](active/fan-v2-examples.md) | Reference examples for the fan v2 surface: every head, edge semantics, diagnostics |
| [Fan v2 — one execution-policy grammar](active/fan-v2.md) | Fan v2: the execution-policy grammar — heads x forms, thunk-free, typed budgets |
| [Flight Evidence Gaps — 実地監査所見台帳](active/flight-evidence-gaps.md) | Hands-on internal audit findings (2026-07-03) — the measured distance between DAL-A philosophy and DAL-A evidence, as 7 findings with corrective work items and acceptance criteria |
| [Flight-grade: the organization and track-record rungs](active/flight-organization.md) |  |
| [Flight Profile — 航空品質への接地分析と2つのキーストーン](active/flight-profile.md) | DO-178C flight-grade gap analysis: 6 pillars status + 2 engineering keystones |
| [Flight Qualification — DO-178C/DO-330/DO-333 マッピング + 資格化キット(G-F5 + G-F6)](active/flight-qualification.md) | Flight gates G-F5 + G-F6 — how the PCC certificate/receipt maps to DO-178C Table A objectives, the DO-330 "prove-the-checker" tool-qualification argument (compiler→TQL-5 output-verified, checker→qualified-by-proof), the DO-333 formal-methods credit per proven property, and the G-F6 qualification kit as a product (AbsInt/SCADE-KCG model): what's in the kit, the boundary (kit provides vs customer's domain process), and the customer integration story. |
| [Flight Reference App — PID 制御則カーネル + make verify + receipt(G-F4)](active/flight-reference-app.md) | Flight gate G-F4 — the reference application (a fixed-point Q16.16 PID control-law kernel over a counted sim loop) that passes `make verify` end-to-end, the 7-stage verify pipeline (exist vs gated on keystones), the receipt it emits (C-SAFE/C-PROVEN green, C-WCET/C-FAITHFUL pending), and the de-risking order (Slice 0 scalar-no-print green now → Slice 1 print=G-F0 frontier → Slice 2 keystone-あ unlocks C-WCET → Slice 3 keystone-い unlocks Ferrocene). |
| [Flight Keystone (い) — v1 MIR → Rust → Ferrocene(忠実性束縛)](active/flight-rust-ferrocene.md) | Concrete implementation design for flight keystone (い) — G-F3: a production v1 MIR→Rust renderer + a rust_pattern faithfulness layer, making Rust→Ferrocene the flight target. The proof is cheap (~75% of the spine is target-agnostic; the faithfulness theorem is `exact eager_copy_refines_safety`); the real cost is the production renderer (~5x render_wasm). Ferrocene owns Rust→machine, so the flight path bypasses the hardest wasm byte-binding proof (Gap 1). |
| [Almide Flight Profile — 規範仕様(Normative Subset)](active/flight-subset-spec.md) | The normative Almide Flight Profile — the SPARK/Ravenscar-class language subset for flight-grade code, machine-enforced by the per-build certificate (by proof, not review). Feature IN/OUT/RESTRICTED classification, the resolved keystone open questions (Dup-in-loop IN, break/continue OUT, recursion=acyclicity-reject, nested loops IN), the @flight enforcement architecture, the MISRA/Ravenscar mapping, and honest language residuals. |
| [Flight Keystone (あ) — Counted Loops + Bounded Allocation(WCET-by-construction)](active/flight-wcet-loops.md) | Concrete implementation design for flight keystone (あ) — G-F1/G-F2: counted-loop flight subset + lifting loops into Coq to prove bounded allocation (WCET-by-construction). The counted loop is a SEPARATE structural witness (preserving the flat-fold RC invariant), two Subset.v-shaped properties prove it, and try_lower_scalar_for_range already knows the trip count. |
| [Fuzz Findings Triage: Re-green the Nightly Differential Gate](active/fuzz-findings-triage.md) | Fuzz (nightly) is red — triage the differential findings to zero and re-green the workflow |
| [Integer literal domain — cross-language survey and Almide's target](active/integer-literal-domain.md) | How other languages range-check integer literals, and where Almide should land |
| [StringInterp is NOT special syntax — desugar it to `concat + to_string(part)`](active/interp-is-desugar-to-tostring.md) | Retiring StringInterp as special syntax by desugaring it to concat + to_string(part) |
| [Issue Ledger Burn-down — 完全性キャンペーンの残量計](active/issue-ledger-burndown.md) | The issue-ledger burn-down gauge: keeping the open-issue count an honest measure of remaining work |
| [LLM-first Language](active/llm-first-language.md) | Plan to make Almide the language LLMs write most accurately, measured by dojo MSR |
| [Logical-Time Async — the async grammar design](active/logical-time-async.md) | The async grammar: fuel as the logical clock, deterministic race, oracle tier |
| [Logical-Time Async — the implementation blueprint](active/logical-time-implementation.md) | Implementation blueprint: Op::Charge, fuel ABI, metered clones, race lowering, gates |
| [Logical-Time Async — the proof ledger](active/logical-time-proofs.md) | Proof ledger for the logical-time async semantics: theorems, Lean core, model gate |
| [Map / Set data-structure roadmap](active/map-data-structure-roadmap.md) | Map / Set data-structure roadmap, including the rejected seq-in-entry design and why it was wrong |
| [Reviving the monkey regression suite: what `spec/` actually is](active/mir-caps-call-count-breach.md) | spec/ is the trust spine's v0 corpus, not a test directory — the revived monkey suite grows two shrink-only ratchets and breaches the caps-soundness backstop |
| [native: nested ctor/literal pattern at a Box'd (recursive-variant) field](active/native-boxed-pattern-lowering.md) | Native lowering for a nested ctor/literal pattern at a Box'd recursive-variant field |
| [Native Trust Spine — Perceus as the single memory model (#764)](active/native-trust-spine.md) | Routing almide build --target rust through the same v1 Perceus MIR as wasm, so one memory model serves both legs |
| [Outside-Review Audit 2026-07 — 証拠層の負債バーンダウン](active/outside-review-audit-2026-07.md) | The 2026-07-27 five-lens outside-reviewer audit — the evidence layer lagged the v0→v1 transition and the honesty gradient inverted (internal docs honest, outward claims false); the layered burn-down to "zero false claims, every gate real" with issue links #913-#932 |
| [Playground UI Revamp](active/playground-revamp.md) | Playground UI revamp: file tabs, share URLs, TS-style example gallery |
| [Post-0.29 Improvement Sweep](active/post-0.29-improvement-sweep.md) | Post-0.29.0 improvement sweep — every open gap issue-ized and ordered, from face-fixes to the v0 wasm retirement |
| [Protocols: declared conformance + opt-in `any P`](active/protocol-any-existentials.md) | Declared conformance + opt-in `any P` existentials — take Go's interface-value ergonomics without its implicit-satisfaction and nil-interface traps; the one Swift idea worth stealing, none of the rest |
| [Receipt Logic — 受領書の論理](active/receipt-logic.md) | Formal foundation for the trust layer — receipt logic: claim types, threat model, trust bases, falsification procedures, completeness relative to use-case |
| [reconciliation follow-up — v0.28.0 で見送った develop 側の残件](active/reconciliation-followup.md) | v0.28.0 reconciliation follow-up: deferred develop commits for 0.28.1 |
| [Self-host linking v2 — link the mono instances, retire the twin matrix](active/selfhost-link-v2.md) | Self-host linking v2 — retire the shim registry's per-type twins by linking the monomorphized generic bodies the renderer already produces |
| [Ticks interface audit — is the demanded API shape different?](active/ticks-interface-audit.md) | Audit of ticks against timeout-shaped APIs: the unwritable-number problem, three fixes |
| [Trust Layer — 機械が書くソフトウェアの信頼層](active/trust-layer.md) | Category strategy — winning "the trust layer for machine-written software": MWS Trust Levels, receipts, critical path |
| [Type Where Constraints](active/type-where-constraints.md) | where clauses on type/fn definitions for type constraints |
| [v0 wasm codegen: Try/Unwrap/Fan early-return heap leak](active/v0-unwrap-early-return-leak.md) | The retired v0 wasm emitter's Try/Unwrap/Fan early-return heap leak, kept as the historical record of the class |
| [GOAL PROMPT — finish ADT brick 5 (heap-field bind + recursive drop) to the #1 lever](active/v1-adt-brick5-goal.md) | Goal prompt for ADT brick 5 — heap-field bind plus recursive drop, the highest-leverage remaining ADT step |
| [v1 ① — custom ADT (variant) as a first-class value](active/v1-adt-value-model.md) | Custom ADTs (variants) as first-class values in the v1 value model |
| [V1 Backlog — trust spine の残件一覧（優先度付き）](active/v1-backlog.md) | The v1 trust-spine backlog — soundness, drop completeness, self-host surface, walls |
| [v1 Bolt Backlog(AI-DLC 管理)](active/v1-bolt-backlog.md) | AI-DLC Bolt backlog for the v1 climb — the camps/steps roadmap expressed as intent-driven, time-boxed Bolts (each with intent / Definition-of-Done / gate / deps / status). The construction guardrails are the goal-prompt discipline; each Bolt's exit gate is independent review (reviewer agent + Trust Spine CI + unbiased dual-oracle corpus); humans (Mob) decide at the marked forks. Tracks "あと何 Bolt" to each summit. |
| [v1 → develop Reflow Strategy](active/v1-develop-reflow.md) | Strategy for flowing the develop-v1 trust spine back into develop — what moves, in what order, and what stays branch-local until its gate exists |
| [v1 heap-result `if`/`match` execution — design (DE-RISKED: no Coq change)](active/v1-heap-result-control-flow.md) | Design for executing a heap-result if/match on the v1 leg without changing the Coq kernel |
| [v1 join completeness + linear checkers](active/v1-join-completeness.md) | Kill continuation duplication via bind-position joins, and make the proven checkers linear in witness size |
| [v1 KGI / KPI スコアボード](active/v1-kgi-kpi.md) | v1 KGI/KPI scoreboard — the terminal goal indicators (trust + writability), the guard invariants that must never degrade (checker size, TB purity, axiom cleanliness, zero claim-drift), and the progress KPIs toward each gap. Weekly fill-in. |
| [v1 heap-loop-carried ownership — option C (cert-spine extension), the COMPLETENESS fix](active/v1-loop-ownership-cert.md) | Loop-carried heap ownership as a certificate-spine extension — the completeness fix, not a special case |
| [Almide v1: MIR を唯一の真とする単一意味論アーキテクチャ](active/v1-mir-architecture.md) | The single-semantics architecture: ownership and layout are decided once in MIR, and renderers only replay the decision |
| [Org byte-verification — every repo's own vectors on both targets](active/v1-org-byte-verification.md) | Org-wide v0==wasm byte-verification sweep and the wasm bug classes it flushed out |
| [v1 — the parser-TCO lever (the real "heap-result-expr" cross-repo lever)](active/v1-parser-tco-lever.md) | The parser-TCO lever — the cross-repo heap-result-expression unlock |
| [Almide v1 Phase 1: MIR コア + 二レンダラ — 実装設計](active/v1-phase1-mir-core.md) | Phase 1 implementation design: the MIR core plus the two renderers |
| [v1 Proof Architecture — 構想(着地形)](active/v1-proof-architecture.md) | v1 vision — the landed proof architecture: untrusted compiler + two tiny qualified checkers, ALS as normative semantics, single Coq trust base, per-build receipts. Self-contained, terminal-state. |
| [v1 Proof Spine — #31 progress (全V / leak-freedom / 推移的caps / 抽出穴)](active/v1-proof-spine-progress.md) | v1 proof-spine progress — what of task #31 (全V / leak-freedom / 推移的caps / 抽出穴) is PROVEN vs the honest remaining. Records the CapabilityReach.v transitive-caps theorem (2026-06-21). |
| [v1 — records feature: svg FULL CONQUEST (goal prompt)](active/v1-records-svg.md) | Goal prompt: take the records feature to full coverage against the svg package |
| [v1 リリース路線 — 段階リリース計画](active/v1-release-path.md) | v1 リリース路線 — opt-in 検証 codegen(v0 fallback) を beachhead に、カバレッジ→証明書 emit→flight-grade へ段階リリースする計画。4案のメリデメと選定理由、各段の受入基準。 |
| [v1 stdlib self-host — the machinery phase (Option / List-building / closures)](active/v1-selfhost-machinery.md) | The stdlib self-host machinery phase — Option, list building, and closures |
| [v1 self-host print floor — the ③ observability keystone](active/v1-selfhost-print-floor.md) | The self-hosted print floor, the observability keystone the rest of the self-host rests on |
| [v1 trust-spine correctness holes (adversarial sweep 2026-06-27)](active/v1-spine-correctness-holes.md) | Adversarial sweep of the v1 trust spine (2026-06-27) and the correctness holes it surfaced |
| [v1 System Map — 全体像(mermaid)](active/v1-system-map.md) | v1 system map — mermaid diagrams of the whole trust architecture: each component's what / why / which area it secures, the PCC trust flow, the trust base, the three pillars, the threat model, and the maturity ladder. |
| [v1 TCO — self-recursive tail calls → scalar-state loop (the yaml parser keystone)](active/v1-tco-self-recursion.md) | Self-recursive tail calls to a scalar-state loop — the yaml-parser keystone |
| [V1 → V0 Parity — the completion plan](active/v1-v0-parity.md) | The completion plan to bring the v1 MIR trust-spine to full v0 parity |
| [v1 dynamic Value model — the yaml keystone (path A: self-host + ONE trusted recursive-drop routine)](active/v1-value-model.md) | The dynamic Value model: self-host plus one trusted recursive-drop routine |
| [柱C extension: bring Value rc into the certified region](active/value-rc-cert.md) | Bringing Value reference counting inside the certified region |
| [WASM Reference-Count Frees: the Ownership-Discipline Drain](active/wasm-frees-ownership-discipline.md) | Draining the reference-count frees backlog by making the ownership discipline mechanical |
| [WASM 所有権 emit 層の機械化 (Perceus drift の構造的封じ込め)](active/wasm-ownership-emit-mechanization.md) | Mechanizing the WASM ownership emit layer to contain Perceus drift structurally |
| [WASM Platform Frontier — beyond core Wasm 3.0](active/wasm-platform-frontier.md) | Post-Wasm-3.0 platform tracking — WASI 0.3 / Component Model, stack switching, shared-everything-threads |
| [Zero Committed Shell](active/zero-committed-shell.md) | Drive committed shell to zero; the two primitives that block it |

## On Hold

30 items

| Item | Description |
|------|-------------|
| [Almide-to-Almide FFI via almide-lander](on-hold/almide-to-almide-ffi.md) | Use almide-lander to call compiled Almide libraries from Almide via shared library FFI |
| [Almide UI — Reactive Web Framework as Almide Library](on-hold/almide-ui.md) | SolidJS-like reactive UI framework built as a pure Almide library |
| [API Diff & Automatic Versioning](on-hold/api-diff-auto-versioning.md) | Automatic semver bump detection via public API diffing |
| [LLM Benchmark: Next Phase](on-hold/benchmark-next-phase.md) | LLM benchmark Phase 2-3: cross-language comparison, harder problems, publication |
| [Compile-Time Contracts](on-hold/compile-time-contracts.md) | Compile-time preconditions and type invariants via where clauses |
| [Externalize `try:` Snippets from Rust Literals](on-hold/diagnostic-snippet-externalization.md) | Move try: snippet text out of Rust literals into stdlib/diagnostics/*.almd |
| [Flow[T] — Lazy Streaming Sequences](on-hold/flow-design.md) | Flow[T] lazy streaming sequences with flow.* namespace aligned with list.* verbs |
| [Flow[T] — User Specification (Draft)](on-hold/flow-spec-draft.md) | Draft user-facing spec for Flow[T] — to be promoted to docs/specs/flow.md after Phase 1 |
| [GPU Compute — Matrix Type and Compiler-Driven GPU Execution](on-hold/gpu-compute.md) | Matrix primitive type with compiler-driven CPU/GPU execution |
| [IR Optimization Tier 2](on-hold/ir-optimization-tier2.md) | CSE and inlining passes for cross-target IR optimization |
| [LLM Integration](on-hold/llm-integration.md) | Built-in LLM commands for library generation, auto-fix, and code explanation |
| [LSP Code Actions](on-hold/lsp-code-actions.md) | LSP code actions for auto-fix, refactoring, and import management |
| [Tiny ML Inference Runtime](on-hold/ml-inference.md) | Tiny ML inference runtime using compile-time model specialization |
| [MLIR Backend + Egg Rewrite Engine](on-hold/mlir-backend-adoption.md) | Stage 2 progressive lowering — dialect walker passes all 227 spec tests |
| [`almide update` — Dependency Update Command](on-hold/package-manager-update.md) | Add almide update command to refresh dependencies and rewrite lock file |
| [Package Registry](on-hold/package-registry.md) | Lock file, semver resolution, and central package registry |
| [Package Version Resolution](on-hold/package-version-resolution.md) | MVS version resolution with semver constraints for almide.toml |
| [Performance Research: Path to World #1](on-hold/performance-research.md) | Research plan to surpass hand-written Rust via semantic-aware optimization |
| [Porta Embedded — Sub-10KB Almide IoT Agents on WASI Hosts](on-hold/porta-embedded.md) | Porta-style WASI agent runtime for IoT: <10KB Almide guests on tiny hosts |
| [Rainbow Bridge — Wrap External Code as Almide Packages](on-hold/rainbow-bridge.md) | Wrap external Rust/TS/Python code as native Almide packages via @extern |
| [Region-based Memory Management](on-hold/region-inference.md) | Region-based memory management — Phase 1+2 shipped, Phase 3 (full inference) on hold for server workloads |
| [Research: Modification Survival Rate Paper](on-hold/research-modification-survival-rate-paper.md) | Academic paper measuring LLM code modification survival across languages |
| [The Rumbling — Almide OSS Rewrite Campaign](on-hold/rumbling.md) | Campaign to rewrite OSS tools in Almide to prove WASM size and LLM accuracy |
| [Secure by Design](on-hold/secure-by-design.md) | Five-layer security model making web vulnerabilities compile-time errors |
| [Shell Completions](on-hold/shell-completions.md) | almide completions subcommand for bash/zsh/fish auto-completion |
| [Snapshot Testing](on-hold/snapshot-testing.md) | Built-in snapshot testing for output regression detection |
| [Supervision & Actors](on-hold/supervision-and-actors.md) | Erlang-style actors, supervisors, and typed channels as stdlib modules |
| [WASM Exception Handling](on-hold/wasm-exception-handling.md) | WASM native exception handling (try_table/throw) for zero-cost effect fn error propagation |
| [WASM HTTP Client](on-hold/wasm-http-client.md) | HTTP client support for the WASM target via WASI or host imports |
| [Web Framework](on-hold/web-framework.md) | First-party Hono-like web framework with template and Codec integration |

## Done

14 items

<details>
<summary>Show all 14 completed items</summary>

| Done | Item | Description |
|------|------|-------------|
| 2026-07-23 | [Code Health: Codopsy-Driven File Splits and Function Decomposition](done/code-health-codopsy.md) | Codopsy-driven code health: split 1000+ line files, decompose cog>100 fns |
| 2026-07-15 | [Claim wording: Perceus phrasing and the byte-identity guarantee scope](done/claim-wording-perceus-byte-identity.md) | Two claim-wording fixes so the public pitch is 100% backed by measurement |
| 2026-06-02 | [Closure Codegen Cross-Target Gaps](done/closure-codegen-cross-target-gaps.md) | Cross-target (native vs wasm) closure-codegen divergences found by the adversarial differential sweep — all 8 fixed |
| 2026-04-20 | [Variant Exhaustiveness Refinement](done/variant-exhaustiveness-refinement.md) | Non-exhaustive match suggests missing arm code; unreachable arms become hard errors |
| 2026-04-20 | [Reimpl Lint: Signature-Match Detection of Stdlib Reimplementations](done/reimpl-lint.md) | Detect user fns whose signature matches a stdlib fn, suggest delegation |
| 2026-04-19 | [Stdlib Declarative Unification — Toward a Single Source of Truth](done/stdlib-declarative-unification.md) | Drive stdlib toward a single source-of-truth: `.almd` + multi-target ABI attributes |
| 2026-04-19 | [Codegen Ideal Form](done/codegen-ideal-form.md) | WASM codegen redesign toward declarative dispatch and explicit symbol resolution |
| 2026-04-17 | [Bundled-Almide Stdlib — Ideal Form](done/bundled-almide-ideal-form.md) | Ideal form for bundled-Almide stdlib: one dispatch path, no patch-layer special cases |
| 2026-04-16 | [Bundled-Almide Dispatch for Stdlib Modules](done/bundled-almide-dispatch.md) | Let stdlib/<module>.almd extend TOML modules (codegen dispatch fix) |
| 2026-03-19 | [HKT Foundation — Phase 1-4 + Stream Fusion (All 6 Laws)](done/hkt-foundation-phase1.md) | HKT foundation phases 1-4 with type constructors and algebraic laws |
| 2026-03-19 | [Effect System — Phase 1-2](done/effect-system-phase1-2.md) | Effect inference engine with 7 categories and checker integration |
| 2026-03-11 | [Playground Repair Turn](done/playground-repair.md) | Playground AI-powered error repair and type checker integration |

</details>

