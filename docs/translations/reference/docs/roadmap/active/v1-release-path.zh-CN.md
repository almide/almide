<a id="v1-リリース路線--段階リリース計画"></a>

# v1 发布路线 — 分阶段发布计划

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](v1-release-path.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](v1-release-path.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](v1-release-path.zh-CN.md)

> [日英混合正本](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-release-path.md)的非规范性读者快照，固定提交为 `35297a612fb29e3f78f6587aba253aa91d51f466`。原文仍是正本。本快照不自动同步，也不是生成器／检查器输入。下文日期、计划、完成状态、测量和比较主张均保留为历史原文记述，不是独立复现或对当前版本的验证。代码块及注释保持原样。历史收集元数据以不生效的读者注记显示。

> 历史描述：2026 年 7 月发布路线决定，并修正零风险与资格过度主张。

> 原文注意事项：原文开头的修正优先于下方保留的绝对安全前提。“风险0”、flight-grade 升级、独有价值及 4/8 束均为历史理由，不是当前保证、外部认证或复现的证明结果。旧 CLI 标志及自动 fallback 不是当前操作指南。译文不悄悄改写旧决策记录以调和差异。

<a id="correction-to-the-historical-release-rationale"></a>

## 对历史发布理由的修正

这是 2026 年 7 月 5 日的发布路线决定，不是当前标志或发布指南。其绝对化的“没有误编译风险”／“字节始终正确”前提过强：所有权／能力证书确立的是所建模的性质，字节 parity 确立的是被测程序上的观察。二者都不证明完整 lowering 语义。单靠字节绑定也不能获得飞行资格；这需要有明确范围的 assurance case 和剩余信任假设。

边界见[已证明与受信任部分](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/contracts/proven-vs-trusted.md)，当前路线见 [Wasm 概览](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/wasm/README.md)。下文选项及原始决定理由作为历史保留，并非重申零风险保证。

> 2026-07-05 制定。针对“想尽快发布 v1”，以**正确性（honest-wall + PCC）已经保证**为基础，选择最快推出“有实质内容的 v1”的路线记录。
> **相关**：[v1-kgi-kpi](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-kgi-kpi.md)（最终指标）/ [v1-v0-parity](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-v0-parity.md)（③覆盖）/ [certificate-format-v1](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/certificate-format-v1.md)（③证书）/ [flight-profile](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/flight-profile.md)（Gap 1 之后）。

<a id="前提これが全ての土台"></a>

## 前提（全部内容的基础）

- **v1 trust-spine“绝不输出错误字节”**（honest-wall）。能 lower 则与 v0 字节一致，否则 `try_render_wasm_source` 返回 `Err` 并形成拒绝屏障。始终保持 MISMATCH=0 / RUNERR=0。PCC（ownership/caps）ACCEPT。
- 因此，**任何发布路线都没有误编译风险**。差别仅在“能输出什么／工作量／交付给用户的价值”。

<a id="選定-①-opt-in-検証-codegenv0-fallback-を-beachhead-に"></a>

## 选择：以 ① opt-in 验证 codegen（v0 fallback）为立足点

**采用理由**：今天就能发布、风险为 0、使 v1 成为实际产品功能，且 ②③ 的成果自动在同一基础上转化为价值。通过发布包含 ① 的版本，结合使用 ④（version tag）。

<a id="4案のメリデメ意思決定の記録"></a>

## 四种方案的利弊（决策记录）

<a id="①-opt-in-検証-codegenv0-fallback採用進行中"></a>

### ① opt-in 验证 codegen（v0 fallback）★ 已采用、进行中

`almide build/run --target wasm --verified` 尝试 v1 → 通过则输出经 PCC 验证的 v1；遇到屏障则自动回退 v0。默认（无 flag）仍为 v0。

- **优点**：可立即发布（集成 + flag，无需研究）／风险 0（屏障时用 v0，与今天相同）／v1 成为真正可用的产品功能／opt-in 不影响现有用户／字节绑定证明（Gap 1）到位后，同一路径升级为 flight-grade。
- **缺点**：只有全部函数都在 subset 内时 v1 才启用（whole-program dispatch）→ 实际程序初期往往回退 v0，**启用机会少、不显眼**／因 ABI/runtime 共享，逐函数混用困难／**信任核心（证书 + checker）尚未向用户暴露**（执行侧可输出字节一致的 wasm，但验证叙述要到 ③ 才可见）。
- **工作量／风险**：中／正确性风险低、价值风险中。

<a id="②-実プログラムを端まで通すカバレッジ拡大-次段"></a>

### ② 让真实程序完整走通（扩大覆盖）— 下一阶段

持续打开屏障，扩大覆盖，直到真实 repo 仅用 v1 即可 lower（扩大无需 fallback 的区域）。

- **优点**：在完成区域，“v1 单独编译真实程序”是有力且诚实的里程碑／直接推进 Gap 3（真实程序覆盖）／在 ① 的基础上自动转化为“v1 启用范围扩大”的价值。
- **缺点**：慢／终点不易判断（只要一个函数遇到屏障，whole-program 就回退 v0）／剩余项混有 **capturing closure = 证明模型的根本限制**（[closure-architecture-v2](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/done/closure-architecture-v2.md)，扩展证明 = 研究）／单独做这项不会发布任何东西（需经 ①③ 产品化）。
- **验收条件示例**：目标 repo 全部 fn 都可由 v1 lower（零屏障）→ `--verified` 从不触发 v0 fallback。当前剩余屏障：Set（`Option/Result[Set]` 可自举，`some(set)` 构造已确认）／guards／一般 Map（非 String 值的顶层、`Map[Int,Int]`/`Map[String,String]` 可自举）／capturing closure（根本限制）／tuple-inner、record-inner interp（open-shape aggregate 需要逐形状 reader）。**进展汇总于 [v1-kgi-kpi](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-kgi-kpi.md) 的“进攻型 KPI：语言表面覆盖”。**

<a id="③-証明書-emit-を製品化--その次"></a>

### ③ 将证书输出产品化 — 再下一阶段

`almide build --certify` 输出 wasm + 证书，由数百行 checker 重新验证（README 中 v1 的核心：“building is hard, checking is cheap”）。`emit_cert`（验证侧机制）已部分存在。

- **优点**：这是 v1 独一无二的价值（trust model 核心）／第三方运行 checker，体验“接受 ⟹ 安全性束”，使信任可感知／可直接给 ① 的 verified 输出附上证书。
- **缺点**：当前证书为**安全性束 4/8**（mem/name/cap/type ✅，leak、reuse、call-mode、byte 尚余），**byte-binding（Gap 1“核心”）仍依靠信任** → 证书诚实但不完整（“bytes refine source” = C-FAITHFUL 未证明）／完整 KGI-T 需要 Coq 字节绑定证明，属于深度研究／需要设计无歧义的“证明 4/8”呈现方式（过度主张风险）。
- **验收条件示例**：`almide build --certify` 输出 `app.wasm` + `app.cert`，随附 checker（`Print Assumptions` ⊆ 信任底座，0 sorry/0 Axiom）接受 ⟹ 重新导出声明的安全性束（暂时 4/8，诚实明示）。**证书格式见 [certificate-format-v1](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/certificate-format-v1.md)。**

<a id="④-現状をバージョンリリース--①-と併用"></a>

### ④ 将当前状态作为版本发布 — 与 ① 结合

trust-spine 继续作为内部质量改善，bump `Cargo.toml` 并打 tag（release workflow 生成 release）。

- **优点**：最快（数分钟）／发布近期质量与覆盖改进／风险 0。
- **缺点**：单独这样做，不会让用户获得新可用的“v1”（纯维护发布）。
- **运作**：发布含 ① 集成的版本，成为“首次可使用 v1 opt-in 的发布”。以后 ②③ 各阶段每次覆盖／证书进展时 bump。

<a id="横断する正直な限界"></a>

## 各路线共同的诚实限制

- 任何集成路线，**v1 都只在“能 lower 的范围”内启用**（② 的覆盖发挥作用）。
- **完整信任主张（KGI-T）必须有 Gap 1 = Coq 字节绑定证明**，即便最快路线也需要研究阶段。①③ 的设计是在此之前先交付“诚实限定范围”的价值。
- 航空（flight-grade）落地见 [flight-profile](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/flight-profile.md) / [flight-rust-ferrocene](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/flight-rust-ferrocene.md)（通过 Rust→Ferrocene 绑定，绕开 Gap 1 的 wasm byte-binding 的 keystone (い)）。

<a id="実行順このドキュメントの結論"></a>

## 执行顺序（本文结论）

1. **以 ① 为立足点立即发布**（`--verified` opt-in、v0 fallback）= 进行中。
2. **通过 ④ 发布包含 ① 的版本**（“v1 opt-in 首次发布”）。
3. **通过 ② 扩大覆盖**（扩大 v1 启用范围；逐个处理剩余屏障，明确根本限制并绕开或转入研究）。
4. **通过 ③ 暴露证书**（诚实说明安全性束 4/8，使 trust model 可感知）。
5. **通过 Gap 1（字节绑定证明）／flight keystone** 走向 flight-grade（研究阶段，独立台账）。
