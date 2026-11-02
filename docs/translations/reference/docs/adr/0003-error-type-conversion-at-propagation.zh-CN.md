<a id="adr-0003-error-type-conversion-at-propagation-points--lossy-conversion-must-be-spelled"></a>

# ADR-0003：传播点的错误类型转换——有损转换必须明写

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](0003-error-type-conversion-at-propagation.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](0003-error-type-conversion-at-propagation.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](0003-error-type-conversion-at-propagation.zh-CN.md)

> 非规范性的固定读者快照。以原文为准，不自动同步，未独立重新验证实现、测量或安全性。
> [Source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0003-error-type-conversion-at-propagation.md)
> 已接受记录历史决策状态，不是独立确认实现。测量及语言比较，包括 From 转换为双射／无损的说法，保留为未经新验证的原文主张；本阅读版不把它确立为一般性质。区分现有行为、决策及反证条件。


- **状态**：已接受
- **日期**：2026-08-05
- **决策范围**：传播点（显式 `!`、effect／可失败上下文的 auto-`?`）中错误类型 E 的转换规则：匹配、不匹配（两个方向）、是否提供转换钩子，以及现有隐式 none→err 的定位。
- **相关**：[ADR-0002](../../../../adr/0002-fallibility-effect-orthogonal.md)（本 ADR 确认并细化其暂定 D4，而非修订）、#1103（Phase 1）。
- **经过**：2026-08-05 的实测矩阵发现，`!` 的三个 E 类型单元结果各不相同：“匹配=完全可用 / String→CustomE=ICE / CustomE→String=隐式 Debug 字符串化”。初稿提出 Swift 风格的“隐式向上转换到 String=⊤”，但讨论后选择严格规则（见替代方案 1，连同拒绝理由保留）。

<a id="context--実測が示した現状"></a>

## 背景——实测揭示的现状

```almide
type AppError = | NotFound(String) | Io(String)
```

| 操作数的 E | 外围声明的 E | 实测（v0.53.6） |
|---|---|---|
| AppError | AppError | ✅ 不转换，以原有结构传播；可匹配 `err(NotFound(m))` |
| String（如 `int.parse`） | AppError | ❌ check 直接通过 → codegen 出现“Almide bug” ICE（rustc E0277） |
| AppError | String | ⚠ 静默插入 `map_err(\|e\| format!("{:?}", e))`——退化成 Rust **Debug** 格式字符串 |

effect fn 中的 auto-`?` 使用相同机制，因此也有相同的三个单元结构（已实测：`effect fn -> Int` 内的 CustomE 操作数发生同样的隐式 Debug 转换）。

背景事实：

1. **stdlib 的 E 是 String 单一文化**。`int.parse`、`value.field`、`fs.*`、`option.to_result`、`-> T!`（ADR-0002）均为 String。
2. **main 的失败通道是 String**。未处理的 err 最终成为 `Error: <msg>` + exit 1。
3. `!` 将 Option 操作数的 none 隐式转换为 `err("none")`（现有规范）。

<a id="decision"></a>

## 决策

**传播点的 E 转换遵循“无损可隐式，有损必须在代码中可见”的原则。E 不匹配无论方向都是 check 时错误，由编译器在 hint 中给出转换的规范形式。**

<a id="d1-e-一致は無変換現状の一致セルを公式化"></a>

### D1. E 匹配时不转换（正式确认现有匹配单元）

操作数 E = 声明 E 时，原样传播值。结构化错误（variant E）在匹配路径上完整保留可匹配性。在 spec 中明确记为“typed error 的正式路径”。

<a id="d2-e-不一致は方向を問わず-check-時エラー--machine-hintadr-0002-d4-の確認"></a>

### D2. E 不匹配无论方向都为 check 时错误 + machine-hint（确认 ADR-0002 D4）

规则只有一句：**`!` / auto-`?` 不转换 E。** 不匹配产生新 E-code 的 check 错误，hint 按方向给出规范形式：

```
// CustomE → String(上方向・損失あり)
error[E0xx]: `!` は err(ConfigError) を main の失敗チャネル(String)へ変換できない
  try: get_port(cfg) |> result.map_err((e) => "${e}")!

// String → CustomE(下方向)
error[E0xx]: `!` は err(String) を f の失敗チャネル(ConfigError)へ変換できない
  try: int.parse(s) |> result.map_err((e) => BadValue(e))!
```

向下转换时，若只有一个携带 String 的构造器，连构造器名也填入；若有多个，则列出供选择。实现此检查将同时消除现有隐式 Debug 字符串化（§8-5）和 check 放过后发生的 ICE（§8-4）。

<a id="d3-暗黙変換の唯一の許容条件は無損失-noneerrnone-は維持"></a>

### D3. 唯一允许隐式转换的条件是“无损”——保留 none→err("none")

none 不带载荷，因此嵌入 `err("none")` 时**不存在被丢弃的信息**（无损）。现有隐式行为不是原则例外，而是应用实例，应予保留。相比之下，`NotFound("alice")` → `"NotFound alice"` 破坏可匹配性（有损），因此要求显式 `map_err`。这条界线就是本 ADR 的原则。

<a id="d4-from-型の暗黙変換フックは導入しない"></a>

### D4. 不引入 From 式隐式转换钩子

不引入 Rust `From`/`?` 的等价机制（自动应用用户定义转换）。相同的 `!` 因远处是否存在 impl 而改变含义，会降低可预测性（LLM 可编写性）。若 dojo 的 MSR 测量证明 map_err 噪声造成实际损害，再带着证据在另一 ADR 中重新考虑（见反证条件）。

<a id="d5-適用範囲は伝搬点で統一"></a>

### D5. 统一以“传播点”为适用范围

显式 `!` 与 auto-`?` 采用相同机制和规则。`?`（to-Option）与 `??` 丢弃 E，不在本 ADR 范围内。`try_*` family 对 E 多态（callback 与返回 E 为同一类型变量），也不在范围内。

<a id="rationale"></a>

## 理由

<a id="損失のある変換は次の修正者への情報である"></a>

### 有损转换向下一位修改者传达信息

MSR 衡量**下一次修改是否存活**，读者是未来的修改者（主要是 LLM）。`|> result.map_err((e) => "${e}")` 这一行在当地显示“结构在此边界降为 String”，使“不能在下游编写依赖结构化错误的修改”这一事实可 grep、可审查。隐式化会把该信息推到非局部的类型签名比对中。

<a id="儀式の書き手コストは-hint-がほぼゼロにする"></a>

### hint 将书写者的仪式成本降至几乎为零

向上转换的规范形式**只有一种写法**：`|> result.map_err((e) => "${e}")`，check 错误的 hint 原样提供。对 LLM 而言，“读错误文本并粘贴规范形式”属于成功率最高的修复类别，没有猜测转换方向的余地。初稿认为这是“不传达信息的仪式”，但讨论结论是：它传达的是**给读者的信息**，而非给书写者的信息。

<a id="規則が一文になる"></a>

### 规则缩成一句

“`!` 不转换 E。”相比按方向分情况（向上隐式、向下错误），规范、诊断、教材、LLM 提示都更短。唯一例外是 D3 的无损条件，可解释为“从无嵌入”，而非“转换”。

<a id="先例調査--暗黙の広げ先が-lossy-な言語は存在しない"></a>

### 先例调查——不存在隐式扩宽目标有损的语言

| 语言 | E 不匹配时的传播 | 相当于 ⊤ 的类型 | ⊤ 是否保留结构 |
|---|---|---|---|
| Rust | 有 `From` impl 则隐式转换，否则 E0277 | `Box<dyn Error>` / `anyhow::Error` | ✅ 可 downcast |
| Swift（SE-0413） | `throws(E)` → `throws` 隐式向上 | `any Error`（存在类型） | ✅ 可用 `catch let e as E` 匹配 |
| Zig | 隐式 coerce 到超集 error set | `anyerror` | ✅ 保留标签，可 `switch` |
| Go | 显式 wrap（`fmt.Errorf("…: %w", err)`） | `error` interface | ✅ `errors.As` / `errors.Is` |
| Kotlin/Java | throw 沿子类型向上（异常机制） | `Throwable` | ✅ 按类型 catch |
| Gleam / Elm / Haskell `Either` | 无隐式转换——显式 `map_error` | 不提供 | — |

共同点：**允许隐式发生的转换无一例外都是无损的**（From 的双射、向存在类型扩宽、超集 coercion、子类型），转换后仍可返回原结构。未找到相当于“隐式字符串化”的先例——替代方案 1 是无先例的发明。没有 ⊤ 的 Gleam/Elm 只采用显式 map_error，与本 ADR 决策同形（Gleam 连不采用类型类的设计约束也相同）。D3 的无损原则，是将各语言隐式转换都满足的条件表述出来。

<a id="一致セルの完動が-typed-error-の公式ルートを保証する"></a>

### 匹配单元完全可用，保证 typed error 的正式路径

严格化不损失能力：域内让 E 一致并贯通（D1，实测完全可用），仅在边界显式转换。这自然导出“接触 stdlib 的辅助函数使用 E=String，在边界 fn 转换”的惯用方式。

<a id="alternatives--検討して却下した案"></a>

## 替代方案——讨论后拒绝

1. **以 String 为 ⊤ 的隐式向上转换（本 ADR 初稿、Swift SE-0413 风格）**：类比 `throws(E)` → `throws(any Error)` 的隐式向上转换，允许 CustomE → String 隐式发生，并按规范 repr 字符串化。优点是消除 main 附近胶水代码中的仪式。拒绝理由：(a) 有损转换恰在下一位修改者应看到的地方变得不可见（理由 1 的反面）；(b) E=String 返回成为阻力最小路径，未正确贯通 typed error 的 fn（意外写成 `-> Result[User, String]`）会静默通过，而严格检查要求“确定 E”；(c) hint 几乎消除了书写成本差，因此出让可见性的回报很小。Swift 的 ⊤ 是可匹配的 `any Error`（存在类型），不丢失结构；Almide 字符串化不可逆。二者并不同形，这是决定性区别。
2. **Rust 式 From 钩子**：为可预测性而拒绝（有证据后再议，D4）。
3. **自动 wrap 到指定 variant**（自动注入携带 String 的 case）：存在多个候选时歧义，类型定义变化会改变行为。仅限于在 hint 中建议构造器（D2）。拒绝。
4. **带转换器的运算符**（如 `x!(Io)`）：不值得为 map_err 可表达的糖衣增加语法。拒绝。
5. **Zig 式 error set union**（推断 E1 ∪ E2）：需要引入错误类型作为集合合成的整个世界观，失去 Result[T, E] 的简单性。拒绝。

<a id="consequences"></a>

## 后果

- 规范变成一句，E 类型矩阵三个单元折叠为“匹配=正式路径 / 不匹配=错误+hint”两种状态。§8-4（ICE）与 §8-5（隐式 Debug 退化）由同一检查消除。
- main 附近跨边界处（CLI 约 5～15 处）显式写出 `|> result.map_err((e) => "${e}")`。这是为边界可见性付出的代价，而非视作成本。
- **兼容性**：依赖现有隐式 Debug 转换的代码变为 check 错误（由于原行为属于 SURPRISE 级，估计实际损害几乎没有，但应在发布说明中写明迁移 hint）。不会变更输出格式契约，因为隐式转换本身被移除。
- none→err("none") 作为无损原则应用保持不变（D3）。重新考虑措辞（比“none”更有信息的消息）不在本 ADR 范围内。

<a id="falsifier"></a>

## 反证条件

1. **若 dojo 的 MSR 测量表明 map_err 仪式本身是显著的修改失败来源**（弄错方向、写错 `"${e}"` 等），这证明 hint 未能防止问题，应通过新 ADR 重新考虑替代方案 1（隐式向上转换）。
2. **若真实语料测得边界仪式占主导**（如相当比例的可失败 fn 含 map_err 行），撤回 D4，提出 From 式钩子或向上转换的 ADR。
3. **若出现无损判定含糊的转换**（D3 界线无法分类的第三种情况），从原则定义开始修订。

<a id="references"></a>

## 参考

- 实测：2026-08-05 矩阵的 `bang/pure-Result-CustomE-fn/*`、`bang/effect-fn->T/CustomE-operand` 各单元（v0.53.6，已保存探针）。
- Swift Evolution SE-0413 — [Typed throws](https://github.com/swiftlang/swift-evolution/blob/main/proposals/0413-typed-throws.md)（替代方案 1 的比较对象：`any Error` 可匹配，与 Almide 字符串化不同形）。
- Rust — [`std::convert::From` and `?`](https://doc.rust-lang.org/std/convert/trait.From.html)、[The question mark operator](https://doc.rust-lang.org/reference/expressions/operator-expr.html#the-question-mark-operator)。
- Zig — [Error Set Type](https://ziglang.org/documentation/master/#Error-Set-Type)、[anyerror](https://ziglang.org/documentation/master/#The-Global-Error-Set)（无损 coercion 示例）。
- Go — [Error wrapping: `%w`, `errors.Is` / `errors.As`](https://go.dev/blog/go1.13-errors)（显式 wrap + 保留结构）。
- Gleam — [`gleam/result.map_error`](https://hexdocs.pm/gleam_stdlib/gleam/result.html#map_error)（无 ⊤、只显式转换的同形先例）。
- 内部：ADR-0002 D4（本 ADR 确认并细化）、C-211、`!` 的 none→err("none")（D3 应用实例）。

## 上方保留诊断文字的译文

首项是有损的 CustomE 向 String 转换：传播运算符无法把 err(ConfigError) 转为 main 的 String 失败通道。第二项是 String 向 CustomE 转换：无法把 err(String) 转为 f 的 ConfigError 失败通道。try 行原样保留建议调用。
