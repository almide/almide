<a id="historical-production-readiness-plan"></a>

# 历史生产就绪计划

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](PRODUCTION_READY.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](PRODUCTION_READY.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](PRODUCTION_READY.zh-CN.md)

> 固定原文提交的非规范阅读快照；以原文为准，不自动同步。未独立执行、验证实现、检查证明或测量。历史命令与设计指令是资料，不是已执行的指令。
> [Original source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/PRODUCTION_READY.md)

> 原文注记：这是归档 pre-1.0 计划，不是发布批准或当前永久兼容契约。已退役 TS/JS target、旧 fan 名、TOML template codegen、延期 LSP/package/native 项与后续纠正并存；不得把旧 checklist 标记或18/18视为当前验证。旧 security layer 和完整并发模型主张是原文定位，不是普遍安全或全功能完成保证。不同 test/API/pattern 计数及错误码范围按不同历史快照保留。未独立核查竞品历史、预测和时间值；未执行 release、实现、test 或 benchmark。

保留早期 pre-1.0 定位、快照及拟议条件，不是当前发布批准或永久兼容承诺。当前 API/target 由 [SPEC](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/SPEC.md)、[stdlib](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/README.md)、[CLI](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/cli.md) 规定。LSP 及 package/native 集成已存在，下方延期功能表属于历史。18/18 不是当前 checklist 结果。

> **1.0 = 稳定性契约。** 不破坏既有代码的承诺，不是功能清单。
>
> — Ruby、Rust、TypeScript、Go、Gleam 的1.0 都遵循此原则。
>
> **按日期发布。** 被 feature gate 一再拖延，就会像 Zig 一样 pre-1.0 长达10年。

---

<a id="positioning"></a>

## 定位

**“Better Rust for LLMs”** — 正如 Kotlin 以“Better Java”成功，Almide 保留 Rust 的类型安全、性能，同时结构性消除 LLM 易跌倒的复杂性（borrow checker, async/await, Pin/Unpin, lifetime annotations）。

Almide **已避开**的其他语言失败：

| 其他语言的失败 | Almide 设计 |
|-------------|--------------|
| Python asyncio：函数着色问题 | `fan` — 无 async/await，编译器自动插入 |
| Rust async：Pin/Unpin、runtime 选择 | `fan` — thread backend，不向用户暴露 runtime |
| Go：`if err != nil` 地狱 | `effect fn` + 显式 `Result` / `!` 处理 + `guard` |
| Go：nil panic | `Option[T]` — 不存在 null |
| Go：无 sum type | `type | Variant` + 穷尽 `match` |
| Ruby：默认可变 + monkey patching | 不可变值、无元编程 |
| Python 2→3：str 语义变化 | String 始终 UTF-8，不改变 core type 语义 |
| Swift：连续3个版本 breaking | 全部 breaking change 在1.0 前完成 |
| TypeScript：enum/namespace 遗憾 | Rejected Patterns 清单防止再提议 |
| Zig：feature gate 导致迟迟无1.0 | 按日期发布1.0 |

---

<a id="現在地-v0370この節の詳細は当時のスナップショット--最新は-readme--llmstxt"></a>

## 当时位置：v0.37.0（本节详情是历史快照；最新见 README / llms.txt）

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

保留快照的完整译文：
- 编译器：84文件 / 19,536行。生成代码无需外部 crate（自含 stdlib runtime）。
- stdlib：22模块 / 390函数 / runtime 100%。
- 测试：167个 .almd 测试文件 + Rust unit tests（含6个 TypeVar regression）。
- target：Rust、WASM（TS/JS/npm 于2026-03-28 删除；playground 执行 WASM）。
- Exercises：25个 / 6 tiers。
- 并发：fan { }, fan.map, fan.race, fan.any, fan.settle。
- 安全：Layer 1（Effect Isolation）+ Layer 2（经 almide.toml [permissions] 的 Capability Restriction）。
- Effect 推断：自动 capability 推断（IO/Net/Env/Time/Rand/Fan）+ almide check --effects。
- 错误处理：仅 Option + Result 2种机制（避免 Swift 3种机制的失败）。
- Codec：自动派生 encode/decode、Value 类型、JSON roundtrip。
- IR：Typed IR + constant folding、dead code elimination + 12 nanopass。
- 优化：全部6条 Stream Fusion 定律 — map+map、filter+filter、map+fold、map+filter→filter_map、消除 identity、flat_map+flat_map。
- Borrow：基于 use-count 插入/删除 clone。
- 诊断：file:line + context + actionable hint + error recovery。
- Codegen：v3: TOML templates, is_rust()=0, 106/106 cross-target。

---

<a id="v010--v060"></a>

## v0.1.0 → v0.6.0

| 功能 | v0.1.0 | v0.6.0 |
|------|--------|--------|
| 类型系统 | Int, String, Bool | + Float, List, Map, Tuple, Record, Variant, Option, Result, Generics, Union |
| 错误处理 | 无 | effect fn, auto-?, guard |
| 并发 | 无 | fan 系列6 API |
| 安全 | 无 | Effect Isolation |
| Codec | 无 | auto-derive, Value, JSON roundtrip |
| target | 仅 Rust | Rust + WASM |
| 模式匹配 | 基本 | 穷尽检查、嵌套、guard |
| 诊断 | 仅行号 | file:line + context + hint + error recovery |
| 测试 | 0 | 2,033+ |
| stdlib | 少数函数 | 22模块 / 390函数 |
| 工具 | 仅 `almide run` | run, build, test, check, fmt, clean, init |

---

<a id="10-の定義"></a>

## 1.0 的定义

> TypeScript 1.0 交付时没有 strictNullChecks。Rust 1.0 没有 async。Gleam 1.0 带19个 stdlib 模块交付。Go 1.0 没有 generics 仍繁荣12年。它们都只凭“不破坏既有代码”承诺自称1.0。

<a id="当初提案した-10-安定性方針現行の無期限保証ではない"></a>

### 最初提议的1.0 稳定性方针（不是当前无限期保证）

1. **冻结语法**：`effect fn`, `fan`, `guard`, `match`, `for...in`, `while` — 当前语法永久保留。
2. **冻结核心 stdlib API**：22模块 / 390函数签名不变。增加函数，不修改已有函数。
3. **跨 target 一致目标**：当前对象 native、Wasm，TS 输出已退役。实际一致、例外、环境依赖遵循当前契约与 fixture 范围。
4. **edition 字段**：向 `almide.toml` 添加 `edition = "2026"`。未来破坏性变更以新 edition 吸收（Rust editions 的教训）。
5. **永久兼容**：今天能编译的 `.almd` 永远能编译（Go 1 compatibility promise）。

<a id="当初の延期判断現在の実装有無を表す一覧ではない"></a>

### 最初延期判断（不是当前实现有无清单）

| 延期功能 | 理由 | 先例 | 计划 |
|-------------|------|------|------|
| LSP | `almide check` + hint 是 LLM 主要 UX | Gleam：LSP 在1.0 前但最小化 | 1.x |
| FFI / Rainbow Bridge | stdlib 足以写 CLI、Web API | TypeScript：.d.ts 后来才有 | 1.x |
| 包注册表 | git 依赖足够 | Go：module proxy 在1.13 | 1.x |
| Go / Python target | Rust + TS 覆盖90% | Kotlin MP：JVM 优先，其他后续 | 2.x |
| Security Layer 2-5 | 仅 Layer 1 就足够差异化 | — | 2.x |
| Self-Hosting | 用户价值低 | Zig：自建 backend 是陷阱 | 2.x+ |
| 700+函数 | 390实用，Gleam 以19模块到1.0 | 所有语言 | 1.x |
| Algebraic effects | `effect fn` 仅作为 I/O 标记 | Gleam：无 effect system 也成功 | 不考虑 |

<a id="当初の-fan-位置づけと現行の参照先"></a>

### 最初 fan 定位与当前参考

> Rust async 在1.0 后花了4.5年。Python asyncio 分裂生态。Swift 等了7年到5.5。只有 Go goroutine 从首日起有效。

Almide 选择不要求用户写 `async` / `await` 的表面，但仅此不意味着所有并发用途均完成。旧 six-head 清单不是当前内容，`fan.any` 已改为 `??` / `list.find_map`，`fan.race` 改为 clock 必需的 `fan.min`。当前表面和限制见 [SPEC §13](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/SPEC.md#13-concurrency-fan) 与 [ADR-0028](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0028-fan-selection-names-and-cost-contract.md)。

---

<a id="当初の-10-チェックリスト履歴"></a>

## 最初1.0 checklist（历史）

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

保留清单的完整译文（所有标记均属历史）：

Almide 1.0 = 以下全部：

稳定性契约：
- ■ 冻结语法：全部关键词、语法最终确认（verb reform 完成、protocol 添加）。
- ■ 冻结 stdlib API：在 FROZEN_API.md 记录22模块 / 390函数。
- ■ edition 字段：almide.toml 已实现 edition = "2026"。
- ■ 破坏性变更政策：BREAKING_CHANGE_POLICY.md。
- ■ Rejected Patterns 清单：REJECTED_PATTERNS.md（25+项）。

编译器正确性：
- ■ 跨 target CI：106/106（100%）— GitHub Actions 已自动化。
- ■ ICE = 0：测试套件发现 ICE 为零（Rust 1.0 同标准）。
- ■ 生成代码通过 rustc/tsc。
- ■ stdlib runtime 100%。

target 质量：
- ■ Tier 1（Rust）：161/161测试文件全通过，全部 exercises 工作。
- ■ Tier 2（TS/JS）：106/106 pass（100%）。
- ■ Tier 3（WASM）：167/194 pass（0 failed, 27 skipped），mutable collections、fs.write/read_text、top-level let。

测试：
- ■ 测试2,500+（2,681 ✅）。

包管理：
- ■ almide.lock 保证可复现（已实现）。
- ■ almide.toml [dependencies] + git 解析（已实现）。

错误质量：
- ■ 稳定错误码：E001-E010 已实现。
- ■ almide check --json：结构化输出已实现。
- ■ almide check < 1秒：298行14ms（debug）/ 25ms（release）。

■ = 当时达成笔记（18/18），不表示当前1.0 可发布。

1.0 后持续改善（不阻塞）：
- ○ 持续减少 ICE（发现即修复）。
- ○ 测量/改善 hint 应用修复率。
- ○ MSR 测量（Grammar Lab）。
- ○ 首次正确率基准（基于 exercises）。
- ○ 创建/公开5个 showcase 程序。

---

<a id="当初の-pre-10-変更記録"></a>

## 最初 pre-1.0 变更记录

> Pre-1.0 是唯一能做破坏性变更的窗口。Swift 在1→2→3 有3次 breaking change，迁移成本大。TypeScript 回顾了 enum 设计。LLM 学会语法后，改变将指数级变难。

- [x] Verb system reform 完成（stdlib-verb-system.md）。
- [x] 核心类型 API 审核/冻结：`docs/FROZEN_API.md`。
- [x] `fan` 命名最终确认 — fan { }, fan.map, fan.race, fan.any, fan.settle, fan.timeout。
- [x] `effect fn` 标记最终确认 — Effect Isolation（Layer 1）已实现。
- [x] Rejected Patterns 清单：`docs/design/REJECTED_PATTERNS.md`（20+项）。
- [x] Hidden operations 已文档化：`docs/design/HIDDEN_OPERATIONS.md`。

---

<a id="当初の-10-への道元のターゲット機能計画を保存"></a>

## 最初通往1.0 的路线（保留原 target/功能计划）

<a id="phase-i-正確性--クロスターゲット-ci"></a>

### Phase I：正确性 + 跨 target CI

> TypeScript：“选择 target 不得改变程序行为” — 最重要质量门禁。

- 构建跨 target CI（全部测试在 Rust + TS 执行，输出 diff）。
- 修复 Borrow/Clone gaps。
- 强化 Unknown 传播。
- ICE 归零。

<a id="phase-ii-安定性契約の準備"></a>

### Phase II：准备稳定性契约

- 完成 Verb system reform（1.0 前最后破坏性变更）。
- 冻结核心类型 API。
- 添加 edition 字段。
- 稳定错误码（E0001-）。
- 实现 `almide check --json`。
- 让 `almide check` 在500行时 < 1秒（Go 的教训）。

<a id="phase-iii-パッケージ管理--テスト拡充"></a>

### Phase III：包管理 + 扩充测试

> Cargo 在 Rust 1.0 前6个月出现。Go 后悔 GOPATH 的6年。

- 实现 `almide.lock`。
- 测试 +467 → 2,500+。
- 定义/验证5个 showcase 程序。
- 实现 `almide test --json`。

<a id="phase-iv-llm-計測--10-リリース"></a>

### Phase IV：LLM 测量 → 1.0 发布

- 在 Grammar Lab 测量 MSR。
- 基于 exercises 的首次正确率。
- hint 修复率基准。
- 公开 Rejected Patterns 清单。
- 公开测量结果 → **发布1.0**。

---

<a id="1x-エコシステム拡張"></a>

## 1.x：生态扩展

> Ruby：RubyGems（2004）→ Rails（2005），包基础设施先于杀手应用。Rust：1.0 后以6周发布周期逐步强化。

- ~~set 模块~~ → 纳入1.0（20函数已实现）。
- ~~User-defined generics + protocol~~ → 纳入1.0（protocol system 已实现）。
- stdlib 逐步扩充：csv, toml, url, html, sorted（作为 first-party package）。
- LSP（diagnostics → hover → go-to-def）。
- FFI / Rainbow Bridge。
- `almide doc` 生成（MoonBit：doc 加速 community adoption）。
- `assert_snapshot` + `almide test --update`（MoonBit 教训）。
- `almide run` 缓存（生成 .rs 相同则跳过 rustc — MoonBit/Zig 教训）。
- 每月/隔月发布（Rust train model）。

---

<a id="beyond-1x"></a>

## 1.x 之后

| 项目 | roadmap | 先例 |
|------|---------|------|
| Go / Python codegen | on-hold/new-codegen-targets.md | Kotlin MP：JVM 优先，其他逐步 |
| Almide Shell | on-hold/almide-shell.md | Ruby IRB, Gleam：tooling > marketing |
| Self-Hosting | on-hold/self-hosting.md | Zig：自建 backend 是陷阱；MoonBit：成功 |
| Security Layer 2-5 | active/security-model.md | — |
| Async Backend | on-hold/async-backend.md | Rust：4.5年后，不必急 |
| Streaming | on-hold/streaming.md | — |
| LLM → IR 直接生成 | on-hold/llm-ir-generation.md | MoonBit：constrained sampler |
| Web Framework | on-hold/web-framework.md | Ruby：Rails 定义了语言 |
| Almide UI | on-hold/almide-ui.md | — |
| 包注册表 | on-hold/package-registry.md | Python：23年；Go：module proxy 在1.13 |
| Typed errors | — | MoonBit：`T!E`，Zig：error union |
| WASM playground | — | Gleam：playground 加速 adoption |

---

<a id="10-言語からの教訓"></a>

## 来自10种语言的教训

| 教训 | 来源 | Almide 中的体现 |
|------|------|----------------|
| 1.0 = 稳定性契约 | Rust, TS, Go, Gleam | 冻结语法 + stdlib API |
| 按日期发布1.0 | Zig（10年 pre-1.0） | 不因 feature gate 延迟 |
| 破坏性变更只表现为 compile error | Python 2→3 | 禁止静默行为变化 |
| stdlib 是 lean core | Python, Ruby, Gleam（19 modules） | 22 built-in 足够 |
| 包管理 > 杀手应用 | Ruby, Rust, Go | 1.0 纳入 almide.lock |
| edition 保证未来演进 | Rust | edition 字段 |
| fan 是完成的并发模型 | Rust async, Python asyncio, Swift 5.5 | 不是“async 未实现” |
| 错误消息是产品 | Rust, TS, MoonBit | 错误码 + JSON 输出 |
| check 速度决定采用 | Go（sub-second compile） | `almide check` < 1秒 |
| Rejected Patterns 防 feature creep | Ruby, Python PEP | 显式拒绝清单 |
| target 质量层级 | Swift（iOS >> server >> WASM） | Rust > TS > WASM |
| 错误机制最多2种 | Swift（3种造成混乱） | 仅 Option + Result |
| 文档化 hidden ops | Zig（no hidden control flow） | clone, auto-?, runtime |
| 定期发布 | Rust（6周）、Ruby（每年）、TS（3个月） | 每月/隔月 |
| tooling > marketing | Gleam（playground, cheatsheets） | WASM playground |

原详细 `docs/research/lang-lessons-*.md` 已移出树（保留于 git 历史）。
