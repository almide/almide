<a id="historical-production-readiness-plan"></a>

# Historical Production-Readiness Plan

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](PRODUCTION_READY.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](PRODUCTION_READY.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](PRODUCTION_READY.zh-CN.md)

> Non-normative reader snapshot pinned to the original commit; the original remains authoritative. No automatic synchronization or independent execution, implementation verification, proof checking or measurement. Historical commands and design instructions are documentation, not executed instructions.
> [Original source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/PRODUCTION_READY.md)

> Source caveat: this is an archived pre-1.0 plan, not release approval or a present perpetual compatibility contract. Retired TS/JS targets, old fan names, TOML-template codegen and postponed LSP/package/native items coexist with later corrections; do not read historical checklist marks or 18/18 as current verification. Old security-layer and complete-concurrency claims are source positioning, not universal safety or feature-completeness guarantees. The different test/API/pattern counts and error-code ranges are retained as separate historical snapshots. Comparative language histories, forecasts and timings were not independently checked; no release, implementation, test or benchmark was performed.

This preserves the earlier pre-1.0 positioning, snapshots and proposed criteria.
It is not a current release approval or permanent compatibility promise.
Current APIs/targets are specified in [SPEC](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/SPEC.md),
[stdlib](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/README.md) and [CLI](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/cli.md). LSP and package/native
integration already exist; the postponed-feature table below is historical.
The 18/18 tally is not a current checklist result.

> **1.0 = a stability contract.** A promise not to break existing code, not a feature checklist.
>
> — The 1.0 releases of Ruby, Rust, TypeScript, Go and Gleam all followed this principle.
>
> **Cut by date.** Continually postponing through feature gates means spending 10 years pre-1.0 like Zig.

---

<a id="positioning"></a>

## Positioning

**“Better Rust for LLMs”** — as Kotlin succeeded as “Better Java,” Almide maintains Rust's type safety and performance while structurally eliminating complexities that trip up LLMs (borrow checker, async/await, Pin/Unpin, lifetime annotations).

Failures of other languages Almide has **already avoided**:

| Failure in another language | Almide design |
|-------------|--------------|
| Python asyncio: function-coloring problem | `fan` — no async/await; compiler inserts automatically |
| Rust async: Pin/Unpin, runtime choice | `fan` — thread backend; runtime hidden from users |
| Go: `if err != nil` hell | `effect fn` + explicit `Result` / `!` handling + `guard` |
| Go: nil panic | `Option[T]` — null does not exist |
| Go: no sum types | `type | Variant` + exhaustive `match` |
| Ruby: mutable defaults + monkey patching | immutable values, no metaprogramming |
| Python 2→3: changing str semantics | String always UTF-8; core-type meanings do not change |
| Swift: breaking changes in 3 consecutive versions | Complete every breaking change before 1.0 |
| TypeScript: regrets about enum/namespace | Rejected Patterns list prevents reproposals |
| Zig: feature gates prevent 1.0 | Cut 1.0 by date |

---

<a id="現在地-v0370この節の詳細は当時のスナップショット--最新は-readme--llmstxt"></a>

## Position at v0.37.0 (details here are a historical snapshot; latest in README / llms.txt)

```
コンパイラ          84 ファイル / 19,536 行
                    生成コードは外部 crate 不要（stdlib ランタイムを自己内包）
stdlib             22 モジュール / 390 関数 / ランタイム 100%
テスト             167 .almd テストファイル + Rust unit tests (TypeVar regression 6件含む)
ターゲット          Rust, WASM（TS/JS/npm は 2026-03-28 に削除 — playground は WASM 実行）
Exercises          25 本 / 6 tiers
並行処理           fan { }, fan.map, fan.race, fan.any, fan.settle
セキュリティ       Layer 1 (Effect Isolation) + Layer 2 (Capability Restriction via almide.toml [permissions])
Effect推論         自動capability推論 (IO/Net/Env/Time/Rand/Fan) + almide check --effects
エラー処理         Option + Result の 2 機構のみ (Swift の 3 機構の失敗を回避)
Codec              auto-derive encode/decode, Value 型, JSON roundtrip
IR                 Typed IR + constant folding, dead code elimination + 12 nanopass
最適化             Stream Fusion 全6法則 — map+map, filter+filter, map+fold, map+filter→filter_map, identity消去, flat_map+flat_map
Borrow             use-count ベースの clone 挿入/削除
診断               file:line + context + actionable hint + error recovery
Codegen            v3: TOML templates, is_rust()=0, 106/106 cross-target
```

Full translation of the preserved snapshot:
- Compiler: 84 files / 19,536 lines. Generated code needs no external crates (self-contained stdlib runtime).
- stdlib: 22 modules / 390 functions / runtime 100%.
- Tests: 167 .almd test files + Rust unit tests (including 6 TypeVar regressions).
- Targets: Rust, WASM (TS/JS/npm removed on 2026-03-28; playground executes WASM).
- Exercises: 25 / 6 tiers.
- Concurrency: fan { }, fan.map, fan.race, fan.any, fan.settle.
- Security: Layer 1 (Effect Isolation) + Layer 2 (Capability Restriction via almide.toml [permissions]).
- Effect inference: automatic capability inference (IO/Net/Env/Time/Rand/Fan) + almide check --effects.
- Error handling: only 2 mechanisms, Option + Result (avoiding Swift's failure with 3 mechanisms).
- Codec: auto-derived encode/decode, Value type, JSON roundtrip.
- IR: Typed IR + constant folding, dead code elimination + 12 nanopasses.
- Optimization: all 6 Stream Fusion laws — map+map, filter+filter, map+fold, map+filter→filter_map, identity elimination, flat_map+flat_map.
- Borrow: use-count-based clone insertion/removal.
- Diagnostics: file:line + context + actionable hint + error recovery.
- Codegen: v3: TOML templates, is_rust()=0, 106/106 cross-target.

---

<a id="v010--v060"></a>

## v0.1.0 → v0.6.0

| Feature | v0.1.0 | v0.6.0 |
|------|--------|--------|
| Type system | Int, String, Bool | + Float, List, Map, Tuple, Record, Variant, Option, Result, Generics, Union |
| Error handling | None | effect fn, auto-?, guard |
| Concurrency | None | fan family, 6 APIs |
| Security | None | Effect Isolation |
| Codec | None | auto-derive, Value, JSON roundtrip |
| Targets | Rust only | Rust + WASM |
| Pattern matching | Basic | Exhaustiveness checks, nesting, guards |
| Diagnostics | Line number only | file:line + context + hint + error recovery |
| Tests | 0 | 2,033+ |
| stdlib | A few functions | 22 modules / 390 functions |
| Tools | Only `almide run` | run, build, test, check, fmt, clean, init |

---

<a id="10-の定義"></a>

## Definition of 1.0

> TypeScript 1.0 shipped without strictNullChecks. Rust 1.0 shipped without async. Gleam 1.0 shipped with 19 stdlib modules. Go 1.0 prospered for 12 years without generics. All called themselves 1.0 with just the promise that existing code would not break.

<a id="当初提案した-10-安定性方針現行の無期限保証ではない"></a>

### Originally proposed 1.0 stability policy (not a current indefinite guarantee)

1. **Syntax freeze**: `effect fn`, `fan`, `guard`, `match`, `for...in`, `while` — current syntax is permanent.
2. **Core stdlib API freeze**: signatures of 22 modules / 390 functions remain unchanged. Add functions, but do not change existing ones.
3. **Cross-target agreement goal**: current targets are native and Wasm. TS output is retired. Actual agreement, exceptions and environmental dependencies follow the current contracts' and fixtures' scope.
4. **Edition field**: add `edition = "2026"` to `almide.toml`. Absorb future breaking changes in new editions (lesson from Rust editions).
5. **Permanent compatibility**: `.almd` that compiles today compiles forever (Go 1 compatibility promise).

<a id="当初の延期判断現在の実装有無を表す一覧ではない"></a>

### Original postponement decisions (not an inventory of current implementation status)

| Postponed feature | Reason | Precedent | Schedule |
|-------------|------|------|------|
| LSP | `almide check` + hints are the main LLM UX | Gleam: LSP before 1.0, but minimal | 1.x |
| FFI / Rainbow Bridge | stdlib suffices for CLI / Web APIs | TypeScript: .d.ts came later | 1.x |
| Package registry | git-based dependencies suffice | Go: module proxy in 1.13 | 1.x |
| Go / Python targets | Rust + TS cover 90% | Kotlin MP: JVM first, others later | 2.x |
| Security Layer 2-5 | Layer 1 alone sufficiently differentiates | — | 2.x |
| Self-Hosting | Little user value | Zig: own backend is a trap | 2.x+ |
| 700+ functions | 390 practical; Gleam reached 1.0 with 19 modules | All languages | 1.x |
| Algebraic effects | Keep `effect fn` an I/O marker | Gleam succeeded without an effect system | Not considered |

<a id="当初の-fan-位置づけと現行の参照先"></a>

### Original fan positioning and current references

> Rust async took 4.5 years after 1.0. Python asyncio split the ecosystem. Swift waited 7 years until 5.5. Only Go's goroutines worked from day one.

Almide chose a surface that does not require users to write `async` / `await`, but that alone does not mean every concurrency use case is complete. The old six-head list is not current: `fan.any` changed to `??` / `list.find_map`, and `fan.race` to clock-required `fan.min`. See [SPEC §13](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/SPEC.md#13-concurrency-fan) and [ADR-0028](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0028-fan-selection-names-and-cost-contract.md) for the current surface and constraints.

---

<a id="当初の-10-チェックリスト履歴"></a>

## Original 1.0 checklist (historical)

```
Almide 1.0 = ALL of:

安定性契約
  ■ 構文凍結: 全キーワード・構文の最終確認 (verb reform 完了, protocol 追加)
  ■ stdlib API 凍結: FROZEN_API.md で 22 モジュール / 390 関数を文書化
  ■ edition フィールド: almide.toml に edition = "2026" 実装済み
  ■ 破壊的変更ポリシー: BREAKING_CHANGE_POLICY.md
  ■ Rejected Patterns リスト: REJECTED_PATTERNS.md (25+ 項目)

コンパイラ正確性
  ■ クロスターゲット CI: 106/106 (100%) — GitHub Actions 自動化済み
  ■ ICE = 0: テストスイートで発見される ICE ゼロ (Rust 1.0 も同基準)
  ■ 生成コードが rustc/tsc 通過
  ■ stdlib ランタイム 100%

ターゲット品質
  ■ Tier 1 (Rust): 161/161 テストファイル全通過、全 exercises 動作
  ■ Tier 2 (TS/JS): 106/106 pass (100%)
  ■ Tier 3 (WASM): 167/194 pass (0 failed, 27 skipped), mutable collections, fs.write/read_text, top-level let

テスト
  ■ テスト 2,500+                        (2,681 ✅)

パッケージ管理
  ■ almide.lock で再現性保証 (実装済み)
  ■ almide.toml [dependencies] + git ベース解決 (実装済み)

エラー品質
  ■ 安定エラーコード: E001-E010 実装済み
  ■ almide check --json: 構造化出力 実装済み
  ■ almide check < 1 秒: 298 行で 14ms (debug) / 25ms (release)

■ = 当時の達成メモ (18/18)。現行の 1.0 リリース可否は示さない

1.0 後の継続改善 (ブロックしない):
  ○ ICE の継続的削減 (発見次第修正)
  ○ hint 適用修復率の計測・改善
  ○ MSR 計測 (Grammar Lab)
  ○ 初回正答率ベンチマーク (exercises ベース)
  ○ 5 showcase プログラムの作成・公開
```

Full translation of the preserved checklist (all marks are historical):

Almide 1.0 = ALL of:

Stability contract:
- ■ Syntax freeze: final review of all keywords/syntax (verb reform complete, protocol added).
- ■ stdlib API freeze: document 22 modules / 390 functions in FROZEN_API.md.
- ■ Edition field: edition = "2026" implemented in almide.toml.
- ■ Breaking-change policy: BREAKING_CHANGE_POLICY.md.
- ■ Rejected Patterns list: REJECTED_PATTERNS.md (25+ items).

Compiler correctness:
- ■ Cross-target CI: 106/106 (100%) — automated with GitHub Actions.
- ■ ICE = 0: zero ICEs found by the test suite (same criterion as Rust 1.0).
- ■ Generated code passes rustc/tsc.
- ■ stdlib runtime 100%.

Target quality:
- ■ Tier 1 (Rust): all 161/161 test files pass, all exercises work.
- ■ Tier 2 (TS/JS): 106/106 pass (100%).
- ■ Tier 3 (WASM): 167/194 pass (0 failed, 27 skipped), mutable collections, fs.write/read_text, top-level let.

Tests:
- ■ Tests 2,500+ (2,681 ✅).

Package management:
- ■ Reproducibility guaranteed with almide.lock (implemented).
- ■ almide.toml [dependencies] + git-based resolution (implemented).

Error quality:
- ■ Stable error codes: E001-E010 implemented.
- ■ almide check --json: structured output implemented.
- ■ almide check < 1 second: 298 lines in 14ms (debug) / 25ms (release).

■ = achievement notes at the time (18/18), not present 1.0 release approval.

Continuous improvements after 1.0 (non-blocking):
- ○ Continue reducing ICEs (fix as found).
- ○ Measure/improve repair rate after applying hints.
- ○ Measure MSR (Grammar Lab).
- ○ First-attempt correctness benchmark (exercise-based).
- ○ Create/publish 5 showcase programs.

---

<a id="当初の-pre-10-変更記録"></a>

## Original pre-1.0 change record

> Pre-1.0 is the only window for breaking changes. Swift had 3 rounds of breaking changes, 1→2→3, with high migration costs. TypeScript has reflected on enum design. Once LLMs learn the syntax, change becomes exponentially harder.

- [x] Verb system reform complete (stdlib-verb-system.md).
- [x] Core-type API audit/freeze: `docs/FROZEN_API.md`.
- [x] Final `fan` naming review — fan { }, fan.map, fan.race, fan.any, fan.settle, fan.timeout.
- [x] Final `effect fn` marker review — Effect Isolation (Layer 1) implemented.
- [x] Rejected Patterns list: `docs/design/REJECTED_PATTERNS.md` (20+ items).
- [x] Hidden operations documented: `docs/design/HIDDEN_OPERATIONS.md`.

---

<a id="当初の-10-への道元のターゲット機能計画を保存"></a>

## Original path to 1.0 (preserving the original target/feature plan)

<a id="phase-i-正確性--クロスターゲット-ci"></a>

### Phase I: Correctness + cross-target CI

> TypeScript: “Target selection must not change program behavior” — the most important quality gate.

- Build cross-target CI (run all tests on Rust + TS, diff output).
- Fix Borrow/Clone gaps.
- Harden Unknown propagation.
- Eliminate ICEs.

<a id="phase-ii-安定性契約の準備"></a>

### Phase II: Prepare the stability contract

- Complete Verb system reform (last breaking change before 1.0).
- Freeze core-type APIs.
- Add edition field.
- Stable error codes (E0001-).
- Implement `almide check --json`.
- Get `almide check` below 1 second for 500 lines (Go's lesson).

<a id="phase-iii-パッケージ管理--テスト拡充"></a>

### Phase III: Package management + more tests

> Cargo appeared 6 months before Rust 1.0. Go regrets its 6 years of GOPATH.

- Implement `almide.lock`.
- Tests +467 → 2,500+.
- Define/verify 5 showcase programs.
- Implement `almide test --json`.

<a id="phase-iv-llm-計測--10-リリース"></a>

### Phase IV: LLM measurements → 1.0 release

- Measure MSR in Grammar Lab.
- Exercise-based first-attempt correctness.
- Hint repair-rate benchmarks.
- Publish Rejected Patterns list.
- Publish measurements → **release 1.0**.

---

<a id="1x-エコシステム拡張"></a>

## 1.x: Ecosystem expansion

> Ruby: RubyGems (2004) → Rails (2005). Package infrastructure preceded the killer app. Rust: gradual improvements on a 6-week release cadence after 1.0.

- ~~set module~~ → included in 1.0 (20 functions implemented).
- ~~User-defined generics + protocol~~ → included in 1.0 (protocol system implemented).
- Gradual stdlib expansion: csv, toml, url, html, sorted (as first-party packages).
- LSP (diagnostics → hover → go-to-def).
- FFI / Rainbow Bridge.
- `almide doc` generation (MoonBit: docs accelerate community adoption).
- `assert_snapshot` + `almide test --update` (MoonBit's lesson).
- `almide run` cache (skip rustc when generated .rs is identical — MoonBit/Zig's lesson).
- Monthly/bimonthly releases (Rust train model).

---

<a id="beyond-1x"></a>

## Beyond 1.x

| Item | Roadmap | Precedent |
|------|---------|------|
| Go / Python codegen | on-hold/new-codegen-targets.md | Kotlin MP: JVM first, others incrementally |
| Almide Shell | on-hold/almide-shell.md | Ruby IRB, Gleam: tooling > marketing |
| Self-Hosting | on-hold/self-hosting.md | Zig: own backend is a trap; MoonBit: success |
| Security Layer 2-5 | active/security-model.md | — |
| Async Backend | on-hold/async-backend.md | Rust: after 4.5 years; no need to rush |
| Streaming | on-hold/streaming.md | — |
| LLM → IR direct generation | on-hold/llm-ir-generation.md | MoonBit: constrained sampler |
| Web Framework | on-hold/web-framework.md | Ruby: Rails defined the language |
| Almide UI | on-hold/almide-ui.md | — |
| Package registry | on-hold/package-registry.md | Python: 23 years; Go: module proxy in 1.13 |
| Typed errors | — | MoonBit: `T!E`, Zig: error union |
| WASM playground | — | Gleam: playground accelerates adoption |

---

<a id="10-言語からの教訓"></a>

## Lessons from 10 languages

| Lesson | Source | Reflection in Almide |
|------|------|----------------|
| 1.0 = stability contract | Rust, TS, Go, Gleam | Syntax freeze + stdlib API freeze |
| Cut 1.0 by date | Zig (10 years pre-1.0) | Do not delay through feature gates |
| Breaking changes only as compile errors | Python 2→3 | Ban silent behavioral changes |
| stdlib as lean core | Python, Ruby, Gleam (19 modules) | 22 built-ins suffice |
| Package management > killer apps | Ruby, Rust, Go | almide.lock in 1.0 |
| Editions ensure future evolution | Rust | Edition field |
| fan is a complete concurrency model | Rust async, Python asyncio, Swift 5.5 | Not “async unimplemented” |
| Error messages are a product | Rust, TS, MoonBit | Error codes + JSON output |
| Check speed decides adoption | Go (sub-second compile) | `almide check` < 1 second |
| Rejected Patterns prevent feature creep | Ruby, Python PEP | Explicit rejection list |
| Target-quality tiers | Swift (iOS >> server >> WASM) | Rust > TS > WASM |
| At most 2 error mechanisms | Swift (3 cause confusion) | Option + Result only |
| Document hidden ops | Zig (no hidden control flow) | clone, auto-?, runtime |
| Regular releases | Rust (6 weeks), Ruby (annual), TS (3 months) | Monthly/bimonthly |
| tooling > marketing | Gleam (playground, cheatsheets) | WASM playground |

The former detailed `docs/research/lang-lessons-*.md` files were removed from the tree (retained in git history).
