<a id="adr-0008-propagation-is-always-explicit--auto--is-abolished--is-the-only-marker-result-is-must-use"></a>

# ADR-0008：传播始终显式 — 废除 auto-?，`!` 是唯一标记，Result 必须使用

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](0008-explicit-propagation-only.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](0008-explicit-propagation-only.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](0008-explicit-propagation-only.zh-CN.md)

> [日文正本](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0008-explicit-propagation-only.md)的非规范性读者快照，固定提交为 `35297a612fb29e3f78f6587aba253aa91d51f466`。原 ADR 仍是权威且不可变的正本。本快照不自动同步，也不是检查器或生成器输入。下文决定、日期、测量、实现状态和主张均来自固定的历史原文，不代表独立验证或当前版本保证。代码块及其注释保持原样。

> 原文注意事项：v0.53.6 matrix、关于其他语言先例的断言和迁移日程均为本 ADR 的历史记述，并未在此独立复现。E0xx 是原文占位符，不是新分配的诊断代码。dojo MSR 反证条件作为历史标准保留，不是新的基准结果。

- **状态**：已接受
- **日期**：2026-08-05
- **决定范围**：effect fn／可失败 fn 内的失败传播机制 — 隐式传播（auto-`?`）的存废、统一传播标记、丢弃 Result 的处理。
- **相关**：[ADR-0002](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0002-fallibility-effect-orthogonal.md)（D4：`!` 仅用于传播。本 ADR 使 D3 的 lift 对等性变得自然成立）、[ADR-0005](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0005-operators-desugar-to-stdlib.md)（`?`、`??` 的语法糖定义 — 本 ADR 使其可以无特例地组合）、[ADR-0006](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0006-fallibility-polymorphic-hofs.md)（显式 `!` 的标准形式）、#1103（Phase 1）。
- **经过**：源于 2026-08-05 对错误表面 matrix 的 B 轨道 21 个单元（auto-`?` 的位置差异）的深入调查。经是／否裁决，决定采用“全部显式 + must-use 为错误”。

<a id="context--実測したまだらの全景v053621-セル"></a>

## 背景 — 实测的位置差异全貌（v0.53.6、21 个单元）

auto-`?`（在 effect fn 内隐式传播可能失败的调用的机制）**在某些位置生效，在其他位置不生效**：

| 隐式传播生效（5 类位置） | 不生效 — 需要显式 `!`（5 类位置） |
|---|---|
| 语句位置（单独的 `fail()`） | 函数参数（`double(get())` → E005） |
| 无注解的 let（`let r = int.parse(s)`） | 管道（`get() \|> double` → E005） |
| if 条件 | record 字段（→ E001） |
| match（值模式时） | 列表元素（→ E001） |
| 字符串插值（无任何警告） | 元组分量（→ 无编号错误） |

还有两个更糟糕的实测例子：

```almide
// 1. パターンの形が実行時の生死を決める:
match get() { ok(v) => ..., err(e) => ... }   // Result のまま受ける(err は自分で捌く)
match get() { 42 => ..., _ => ... }           // auto-? が差し込まれ、err ならプログラム停止

// 2. 注釈の有無が挙動を反転させる:
let r = int.parse("zz")                        // 暗黙伝搬 — err で停止
let xs: List[Result[Int, String]] = [get()]    // 保持 — 停止しない
```

先例调查：**不存在“仅部分位置隐式传播”的语言**。所有语言都处于两极之一 — 全显式（Rust `?` / Swift `try` / Zig `try`：传播位置必须有标记，无位置差异），或全隐式（Java/Python 异常、OCaml 5 / Koka 效果：无标记、所有位置统一）。关键在于，全隐式语言没有 Result 这种**值**（异常不是值）；把 Result 作为值的 Almide 若趋向全隐式，“保留为值还是传播”的歧义就会扩散到所有位置。

<a id="decision"></a>

## 决定

<a id="d1-auto--は廃止--伝搬の綴りは--一本"></a>

### D1. 废除 auto-`?` — 传播只有 `!` 一种写法

在 effect fn／可失败 fn 内，可能失败的调用**在任何位置都产生 Result 值**。想传播就写 `!`。不再有位置差异：

```almide
effect fn main() -> Unit = {
  let cfg  = fs.read_text(p)!            // 伝搬は常に明示
  let port = int.parse(s)!
  double(get()!)                          // 引数位置も同じ綴り
  println("v=${get()!}")                  // 補間も同じ綴り
  match get() { ok(v) => ..., err(e) => ... }   // Result は普通の値 — 普通に match
}
```

<a id="d2-捨てられた-result-は-check-エラーmust-use"></a>

### D2. 丢弃的 Result 是 check 错误（must-use）

不能在语句位置默默丢弃 Result（堵住废除 auto-`?` 后退化为静默吞错的漏洞）。有两种出路，在 hint 中明确写出：

```
error[E0xx]: この Result は使われていません — エラーが黙って捨てられます
  --> app.almd:3
   |   fail()
   = help: 伝搬するなら fail()!、意図的に捨てるなら let _ = fail()
```

`let _ =` 与 ADR-0004 D3-(b) 的 `(_)` 一样，沿用“显式表示有意丢弃”的原则。

<a id="d3----の特例則は消滅しadr-0005-の定義だけが残る"></a>

### D3. `?` / `??` 的特例规则消失，只保留 ADR-0005 的定义

现有的“`e()?` 先于 auto-? 生效”“`??` 抑制 auto-?”等与隐式机制的交互规则，随该机制一并消失。effect 调用始终是 Result 值，因此：

```almide
e()?          // ≡ result.to_option(e()) — ADR-0005 D1 そのまま
e() ?? fb     // ≡ unwrap_or_else — 同上。特例なしの普通の値演算
```

<a id="d4-移行は-e040-窓方式1-リリースの警告期間"></a>

### D4. 迁移采用 E040 窗口方式（一个发布版本的警告期）

1. 版本 N：在所有插入隐式传播的位置发出弃用警告和精确的 `!` 插入 hint（检测对象 = 背景中的 5 类位置；matrix 的 probe 作为复现测试）。
2. 版本 N+1：切换（警告 → 错误），并启用 D2 的 must-use。
3. 机械迁移仓库中的所有 .almd（stdlib、tools、spec）。同时扫描 6 个下游仓库（已知流程）。
4. 切换时，把 CLAUDE.md 的 codegen 测试规则（“effect fn 内的 fs.read_text() 无需手写 ? 就能编译”）改写为遵守本 ADR。

<a id="d5-adr-0002-d3lift-対等性は自明化"></a>

### D5. ADR-0002 D3（lift 对等性）自然成立

`-> T!`（纯的可失败函数）与 effect fn 的传播易用性完全一致：“两者都显式写 `!`”。#1103 的 D3 对等验收条件按遵守本 ADR 来解释。

<a id="rationale"></a>

## 理由

- **21 个单元的位置差异从结构上消失**：“模式形状决定存活”“注解反转行为”只能通过废除机制消除，而非修补。
- **传播写法统一为一种**：ADR-0002 D4 既然规定 `!` 专用于传播，第二种传播机制（auto-`?`）就违反了单一写法原则（ADR-0004/0005）。
- **先验分布**：Rust `?` / Swift `try` / Zig `try` — 全显式一方占主导。Swift 要求所有 throwing 调用都写 `try`，并能成立。
- **迁移是机械的**：按警告 hint 插入 `!` 即可 — 这是 LLM 特别擅长的修复类型。
- **Result-as-value 的一致性**：`?`、`??`、match、`|>` 可以无特例组合（ADR-0005 的定义表原样适用于所有位置）。

<a id="alternatives--検討して却下した案"></a>

## 替代方案 — 已考虑并拒绝

1. **完成全隐式化**（异常语言式）：在 Result 作为值存在的语言中，“保留为值／传播”的歧义会扩散到所有位置，依赖注解的行为将成为规范中心。**拒绝**。
2. **把现有位置清单写入规范，只修补意外单元**：依赖模式形状的存活差异不会因文档化而变得良性。两种机制（auto-? 和 `!`）也仍共存。**拒绝**。
3. **只在语句位置保留 auto-?**（最小隐式）：仍保留两套机制，同样有问题。语句位置使用 D2（must-use）+ `!` 也只差一个字符。**拒绝**。
4. **must-use 只作为警告**（Rust 式）：吞错是 silent-loss 家族最后的漏洞；既然有一种写法的出路（`let _ =`），设为错误也不会让作者无路可走。**拒绝**（反证条件 3 提供重新考虑条款）。

<a id="consequences"></a>

## 后果

- 为现有全部 effect fn 代码（stdlib、tools、spec、下游）机械添加 `!` — diff 大但简单。有警告期，因此不是同时切换。
- 冗余增加（相当于 Swift 的 `try`）：每次调用一个字符。
- B 轨道 21 个单元归并成一行规范：“effect／可失败调用始终是 Result 值”。`e()?` 拦截、`??` 抑制等未文档化特例，全部作为 ADR-0005 定义的普通组合记录。
- #1103（Phase 1）的设计依赖解除，可以全面着手。

<a id="falsifier"></a>

## 反证条件

1. **迁移后 dojo 的 MSR 显著变差时**（增加的 `!` 产生噪声）— 重新评估最小隐式（替代方案 3）。
2. **警告期间发现大量 hint 无法机械修复的歧义位置时** — 推迟切换，逐类设计歧义情况后再恢复。
3. **实测发现 must-use 错误的误报时**（确有合理的 fire-and-forget）— 将 D2 放宽为警告。

<a id="references"></a>

## 参考

- Swift — [Error Handling（每次调用显式写 `try`）](https://docs.swift.org/swift-book/documentation/the-swift-programming-language/errorhandling/)、SE-0413
- Rust — [The `?` operator](https://doc.rust-lang.org/reference/expressions/operator-expr.html#the-question-mark-operator)、[`#[must_use]` / unused_must_use](https://doc.rust-lang.org/std/result/#results-must-be-used)
- Zig — [try](https://ziglang.org/documentation/master/#try)
- 内部：2026-08-05 matrix 的 `autoq/*`、`qop/effect-*`、`qq/effect-*` 共 21 个单元（probe 已保存），ADR-0002 / 0004 / 0005 / 0006。
