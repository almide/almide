<a id="adr-0004-error-branchability--string-stays-terminal-doctrine-lint-and-context-instead-of-tags"></a>

# ADR-0004：错误可分支性——String 保持终点，以原则、lint 和上下文替代标签

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](0004-error-branchability-doctrine.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](0004-error-branchability-doctrine.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](0004-error-branchability-doctrine.zh-CN.md)

> 非规范性的固定读者快照。以原文为准，不自动同步，未独立重新验证实现、测量或安全性。
> [Source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0004-error-branchability-doctrine.md)
> 已接受为历史决策状态，不证明当前实现。D2 与替代方案7拒绝 context 糖衣，但保留代码例仍称 result.context 为 D2 糖衣；保留原文矛盾，不宣称它是受支持API。stdlib 表面不变的后果与 fs 四单元提案并存。跨语言定理及需求估计是原文论证，非独立证实的结果。


- **状态**：已接受
- **日期**：2026-08-05
- **决策范围**：错误值表示原则——保持收敛到 String，还是增加“可按内容分支的结构”（标签、error set）。
- **相关**：[ADR-0002](../../../../adr/0002-fallibility-effect-orthogonal.md)（可失败性轴）、[ADR-0003](../../../../adr/0003-error-type-conversion-at-propagation.md)（无损原则、variant E 正式路径）、C-029 系列（消息字节契约，本 ADR 保持全文固定）。
- **经过**：延续 2026-08-05 错误表面矩阵讨论，以每问一个具体事项的是非决定批准。采纳原则、lint、context 三点，拒绝带标签 String（条码）和 error set 类型，**连有条件保留的提案也一并拒绝**。初稿的分阶段门禁构想（Stage 1 标签 → Stage 2 error set）在替代方案中连同拒绝理由保留。

<a id="context--問題の定式化"></a>

## 背景——问题的表述

Almide 错误收敛到 String：stdlib 的 E 是 String 单一文化，`-> T!` 的 E=String（ADR-0002），main 的失败通道也是 String。这足以用于报告（由人／LLM 阅读），但没有用于**分支**（程序按错误内容改变行为）的结构。当前只能 string-match，这正是 Go 至 1.12 用十年证明有害的模式。

```almide
// 今日「なければデフォルト、それ以外は失敗」を書くとこうなる — Go 〜1.12 の再演
fn load_config(path: String) -> Result[String, String] =
  match fs.read_text(path) {
    ok(t)  => ok(t),
    err(e) =>
      if string.contains(e, "No such file")   // ✗ メッセージ本文に依存した分岐
      then ok(default_config())
      else err(e),
  }
```

缓解因素：

1. `??` 原生提供“不查看内容的 fallback”，吸收大部分分支需求。
2. 事前谓词（如 `fs.exists`）可在进入错误路径前决定许多分支。
3. 自定义领域可用 variant E（ADR-0003 D1 正式路径）做结构分支。

<a id="各言語が証明した定理"></a>

### 各语言证明的定理

| 语言 | 证明的内容 |
|---|---|
| Go 至 1.12 → 1.13 | String 收敛在规模扩大后必然导致 string-match 分支；后补修复（`%w`/`errors.As`）可行但笨拙 |
| Rust（thiserror/anyhow 分化） | 连人也拒绝逐类型仪式；应用侧收敛于类型擦除。anyhow 核心不在擦除，而在 context 链 |
| Zig（error sets） | 零仪式与可匹配兼容：无需标签声明、集合推断、隐式无损 coerce 到超集 |
| Swift（SE-0413） | 行业终点是两层：“默认擦除，类型化作为主动选择的精化” |
| Roc / OCaml 多态 variant | 结构标签 union 无需声明即可自动合成，代价是推断错误难懂 |
| Erlang/Elixir | 裸标签足以满足大多数分支需求，没有类型也可依靠约定 |
| Gleam（snag） | 无类型类的小语言中，“String + cause 链”是现实的中间解 |

<a id="decision"></a>

## 决策

**以 String 为默认及终点。不在错误值中引入机器可读结构（前缀标签、error set）。用原则与 lint 阻止 string-match 分支。不提供 stdlib 错误的内容分支——以 `??` fallback、事前谓词、自定义领域的 variant E 应对。报告质量（context 链）靠记录 `map_err` 惯用法，不增加糖衣（D2）。**

<a id="d1-ドクトリン行"></a>

### D1. 原则行

在 spec / CHEATSHEET 中用一行明确写出：
**“想按错误的*内容*分支时，就是切换到 variant E（ADR-0003 D1 正式路径）的信号。不得通过 string-match 错误字符串来分支。”**

```almide
// ✓ 自分の定義域では E を variant で設計する(0003 D1 の公式ルート)
type ConfigError = | Missing(String) | BadValue(String)

fn get_port(cfg: Map[String, String]) -> Result[Int, ConfigError] = ...

match get_port(cfg) {
  ok(p)            => p,
  err(Missing(_))  => 8080,                 // ← 構造で分岐。メッセージ変更に不感
  err(BadValue(m)) => process.exit(1),
}
```

<a id="d2-context-チェーンは糖衣なし--map_err-イディオムを正準形として文書化"></a>

### D2. context 链不加糖衣——将 `map_err` 惯用法记为规范形式

为错误前置上下文时，将现有写法原样作为规范形式（拒绝 `result.context` 糖衣——替代方案 7）：

```almide
// 正準形(CHEATSHEET に明記): 区切りは ": "、元エラーは ${e} で末尾に残す
let cfg = fs.read_text(path) |> result.map_err((e) => "loading config: ${e}")!

// 連鎖すると anyhow 流の文脈チェーンになる(v0.53.6 実測):
//   Error: starting server: loading config: No such file or directory (os error 2)
```

已知风险：漏写 `${e}` 仍然类型匹配、编译通过，原错误静默消失（已实测确认）。用诊断而非糖衣防范（D3-(b)）。

```almide
// 今も書ける正準形:
let cfg = fs.read_text(path) |> result.map_err((e) => "loading config: ${e}")!

// D2 の糖衣(意味は上と同一):
let cfg = fs.read_text(path) |> result.context("loading config")!

// 呼び出しが連なると anyhow 流の文脈チェーンになる:
//   Error: starting server: loading config: No such file or directory
```

<a id="d3-エラー扱いの-lint-2-種実装-1105"></a>

### D3. 两种错误处理 lint（实现：#1105）

**(a) string-match 分支警告**——对来自 err 值的 String 用 `string.contains` 或相等比较分支。通过诊断提前防范 Go 的失败模式：

```
warning[W0xx]: エラー文字列の本文で分岐しています
  --> app.almd:12
   |   if string.contains(e, "No such file") then ok(default_config())
   = help: エラーの内容で分岐するなら variant E を定義してください(ADR-0004 D1)。
           内容を見ない fallback なら ?? を検討
```

**(b) 遗漏 `${e}` 的警告**——传给 `map_err` 的 lambda 未使用错误参数时触发。堵住 D2 规范惯用法中唯一不能由类型捕获的书写失误（原错误静默消失，已实测确认）。写 `(_) => ...` 表示有意丢弃，不警告：

```
warning[W0xx]: map_err のラムダがエラー値 `e` を使っていません
  --> app.almd:5
   |   result.map_err((e) => "loading config")
   = help: 元のエラーを残すなら "loading config: ${e}"。
           意図的に破棄するなら (_) => ... と書いてください
```

<a id="d4-stdlib-エラーの内容分岐は提供しない明示的な非目標"></a>

### D4. 不提供 stdlib 错误内容分支（明确非目标）

不提供按种类分支 stdlib 错误（E=String）的正式手段。正式手段为：

1. **`??` fallback**——不管种类，以默认值继续（大多数需求）。
2. **事前谓词**——用 `fs.exists(path)` 等在进入错误路径前判断。
3. **variant E**——分支为本质需求的领域，用自定义 E 设计（0003 D1）。

真实分支需求通过**API 形状**逐一应对，而非添加错误结构：变体将“不存在”作为成功通道的 Option 返回（与 `map.get` 返回 Option 而非 err 同理）。首个应用已批准 fs content-reader family（`read_text_if_exists` 等四个单元）——#1106。

```almide
let cfg = fs.read_text_if_exists(path)! ?? default_config()
//        none = 不在(正常系) / err = 権限・IO 等の本物の失敗
```

<a id="rationale"></a>

## 理由

<a id="分岐需要の実態が構造を正当化しなかった"></a>

### 实际分支需求不足以证明“结构”的必要

被 `??` 与事前谓词吸收后，剩余“按错误种类分支”场景局限于 fs not_found 等。为此重写所有 stdlib 消息（标签化）或扩展类型系统（error set）代价不成比例。局部需求可由 API 形状（D4 的 Option 变体）及现有类型应对。

<a id="即時-3-点d1d3は安くて可逆"></a>

### 立即采用的三点（D1～D3）便宜且可逆

原则行仅涉及文档，context 是对现有 map_err 惯用法的命名，lint 仅警告，不破坏现有代码。为提前阻止 string-match 进入生产，这些措施独立于结构引入，也值得现在投入。

<a id="契約の単純さを守る"></a>

### 保持契约简单

标签化（拒绝方案 A）涉及将契约面从“全文字节”重新设计为“仅标签”，使可复现保证复杂化。全文字节固定存在 Hyrum 式冻结的另一面，但只要遵守“不按消息分支”（D1），冻结的实际损害仅限改进诊断时的摩擦，可用通常的契约更新流程承担。

<a id="alternatives--検討して却下した案"></a>

## 替代方案——讨论后拒绝

1. **带标签 String（条码）**：将 stdlib 错误统一为“`タグ: 詳細文`”，用 `error.is(e, "not_found")` 判断，契约仅固定标签（类似 Node 的 `err.code === 'ENOENT'`、C errno）。**拒绝**——相对局部的分支需求（理由 1），全 stdlib 改写与契约重设计过重。
   ```almide
   fs.read_text("x.toml")        // => err("not_found: x.toml")(却下案)
   ```
2. **error set 类型（Zig 系）**：原生 `#not_found` 标签 + 集合推断 + 穷尽性检查。它是唯一与 ADR-0003 无损原则一致、可隐式合成的结构，理论上最强，但**连“若 A/B 实证有效则采用”的有条件保留方案也拒绝**。当前需求不足以证明类型系统扩展（S 成本）的合理性。未来若恢复，应从本节分析出发：选择集合语义，而非 nominal enum。
   ```almide
   fn load(p: String) -> Result[Config, #not_found | #parse] = ...   // 却下案
   ```
3. **将 stdlib 改为 nominal typed E（Rust 式）**：Rust 历史（应用侧拒绝仪式、转向 anyhow）与 ADR-0003（每个边界都需 map_err 仪式）形成双重反证。**拒绝**。
4. **Roc / OCaml 式结构标签 union**：row 多态推断错误难懂，S 成本最高。**拒绝**。
5. **从标准分类（errno/HTTP）借用标签词汇的规则**：词汇每次单独设计，不用规则限制（标签本身已拒绝，规则对象也不再存在）。**拒绝**。
6. **什么都不做（D1～D3 也推迟）**：毫无防备地等待 string-match 进入生产，重演 Go 至 1.12。**拒绝**。
7. **`result.context(r, "msg")` 糖衣**（相当于 anyhow / snag 的 `context`）：虽可用零自由度写法阻止漏写 `${e}`，但优先**不为同一件事增加第二种 stdlib 写法**，因此**拒绝**（2026-08-05，先批准一次、退回后最终决定）。以惯用法文档（D2）与诊断措施（反证条件 3）替代。#1104 以拒绝关闭。

<a id="consequences"></a>

## 后果

- 获得：确定错误规范终点（String + variant E，不新增结构）。规范、契约、实现均不增加新机制。由 D1/D3 阻止 string-match 流入。
- 代价：按 stdlib 错误种类分支仍无正式手段。fs 等出现真实需求时，逐个用 D4 的 API 形状（Option 变体）应对。
- 实现仅 #1105（lint + 原则行 + D2 惯用法文档）和 #1106（fs family）。契约、类型系统、stdlib 消息、stdlib 表面不变。

<a id="falsifier"></a>

## 反证条件

1. 若 dojo 测得**即使有 D3 lint 与 `??`，仍持续编写 string-match 分支**，本 ADR“可吸收分支需求”的前提即错误；从重新评估替代方案 1（标签）/ 2（error set）开始重启讨论。
2. **若 D4 的 Option 变体 API 开始增殖**（如 `*_if_exists` family 扩散到 fs 之外至少三个模块），这证明需求普遍而非局部，应重新考虑结构引入（替代方案 2）。
3. **若有 D3-(b) 警告，dojo／实际语料仍显著发现漏写 `${e}` 导致的上下文丢失**，视为诊断无法阻止的证据，重新考虑已拒绝糖衣（替代方案 7）。

<a id="references"></a>

## 参考

- Go — [Working with Errors in Go 1.13](https://go.dev/blog/go1.13-errors)、[Errors are values](https://go.dev/blog/errors-are-values)。
- Rust — [anyhow](https://docs.rs/anyhow)、[thiserror](https://docs.rs/thiserror)、[`io::ErrorKind`](https://doc.rust-lang.org/std/io/enum.ErrorKind.html)（与拒绝方案 1 同系）。
- Node.js — [`err.code`(errno)](https://nodejs.org/api/errors.html#errorcode)（与拒绝方案 1 同系）。
- Zig — [Errors](https://ziglang.org/documentation/master/#Errors)（与拒绝方案 2 同系）。
- Swift Evolution SE-0413 — [Typed throws](https://github.com/swiftlang/swift-evolution/blob/main/proposals/0413-typed-throws.md)。
- Gleam — [snag](https://hexdocs.pm/snag/)（String + cause 链中间解）。
- 内部：ADR-0002 / ADR-0003、C-029 系列、2026-08-05 矩阵、#1104、#1105。

## 保留代码注释与诊断的译文

按原文顺序：首例说明今天“缺失则默认，否则失败”的写法重演 Go 至1.12，并把依赖消息正文的分支标为错误。下一例在自己的领域设计 variant E（0003 D1）；Missing 分支依靠结构，不受消息变更影响。规范 context 例指定分隔符“: ”，用插值在末尾保留原错误；连续调用产生所示 anyhow 式上下文链，原文称已在 v0.53.6 实测。下一例先标出现有规范写法，再将已拒绝的 result.context 写法标作“D2 糖衣，含义相同”，并说明调用链形成 anyhow 式上下文链。这是保留的历史矛盾，不表示已采用该 API。

首条警告意为“正在按错误字符串正文分支”；帮助要求按内容分支时定义 variant E（ADR-0004 D1），不查看内容的后备处理则考虑 fallback 运算符。第二条警告意为“map_err lambda 未使用错误值 e”；帮助要求用所示插值保留原错误，或在有意丢弃时写下划线参数。fs 例说明 none 是正常的不存在，err 是权限／IO 等真正失败。最后两个代码例明确标作拒绝方案。替代方案1保留的日语行内标签意为“标签: 详细消息”。
