<a id="adr-0012-error-surface-end-state--refinement-stays-in-the-marker-te-erasure-stays-the-default"></a>

# ADR-0012：错误表面的终态 — 精化留在标记内（`T!E`），擦除仍为默认

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](0012-typed-error-refinement-in-the-marker.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](0012-typed-error-refinement-in-the-marker.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](0012-typed-error-refinement-in-the-marker.zh-CN.md)

> 非规范性历史读者快照，不是当前实现保证。未修改固定原本为权威来源；不自动同步，未独立运行基准、检索文献或认证安全性。保留开头“实现尚未开始”与后续rollout证据的历史并存。2026-08-20修订将广泛保持语义的fmt规范化缩为collapse-only；2026-09-27修订取代D4-4，并指出显式Result lambda替代路径在实测中不存在。前文相反描述属于历史，不是同时有效的当前指南。跨语言、最佳/唯一/无研究、下载量和LLM成功率断言是原作者当时调查主张，此处未独立验证。代码及注释原样保留。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0012-typed-error-refinement-in-the-marker.md)

- **状态**：已接受（设计已批准，实现尚未开始）。
- **日期**：2026-08-11
- **决定范围**：错误类型规范的**终态**。两层（擦除层 = String / 精化层 = variant E）的分配准则、精化记法 `T!E`、fmt 规范形式，以及固定非目标（lambda / main / 转换钩子 / union 类结构 / 隐式传播）。
- **相关**：[ADR-0002](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0002-fallibility-effect-orthogonal.md)（**本 ADR 修订 D2**：从“E 固定 String”变为“E 默认 String、可在标记内精化”）、[ADR-0003](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0003-error-type-conversion-at-propagation.md)（不变，传播点不转换原则原样用于 `T!E`）、[ADR-0004](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0004-error-branchability-doctrine.md)（不变，stdlib 的 String 终态及拒绝 tag/error set 不变）、[ADR-0008](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0008-explicit-propagation-only.md)（不变，增加证据）、ADR-0006 / 0009 / 0010（记法一致性）。
- **经过**：2026-08-11，为确定“2026 年最好的错误类型规范是什么”调查其他语言（MoonBit / Swift / Zig / Rust / Gleam / Roc / Unison / Inko / Koka / Flix / Effekt）。基于各语言2024–2026的第一手提案、release notes、issue 裁定及生产报告。同日逐个具体事项以○×批准 D1–D4（顺序 D2 → D3 → D1 → D4，全部○）。

<a id="context--現状の非対称"></a>

## 背景 — 当时的不对称

Almide 错误表面在0.54–0.56阶段达到了两层结构：

- **擦除层（默认）**：E = String。`T!` ≡ `Result[T, String]`（ADR-0002 D2）、effect fn 隐式 lift、整个 stdlib、main 失败通道。context 使用 `map_err` 惯用法 + E036；E035 阻止字符串匹配分支（ADR-0004）。
- **精化层（可选）**：variant E。显式写 `Result[T, MyError]`，在 E 一致的传播点，`!` **保留结构**传播并可 match（ADR-0003 D1“typed error 官方路径”，#1103 已消除 ICE / 隐式 Debug 退化）。

语义已完成。**缺少的只是精化层记法**，这一缺失有两个实际危害：

1. **模型跳跃**：擦除层可失败性以“函数属性”（后缀 `!`）表达，精化的一刻却变成“泛型”（`Result[T, E]`）模型。Swift（`throws` → `throws(E)`）与 MoonBit（`raise` → `raise E`）都在标记内精化，只有 Almide 跨模型。
2. **精化强迫改写函数体**：pure fn 显式 `-> Result[T, E]` 没有 tail lift（lift 属于 `!` 标记和 effect fn，见 `crates/almide-syntax/src/parser/declarations.rs` 分支）。将 `-> Config!` 精化为 `-> Result[Config, ConfigError]`，不仅改签名，还需**将值 tail 重新包进 `ok(...)`**。最常见修改类别“将错误类型精化一级”无法只做最小差异，直接影响 MSR（modification survival rate）。

<a id="survey--2026-年の他言語実測一次資料"></a>

## 调查 — 2026 年其他语言的实证（第一手资料）

<a id="収斂-1-消去が既定型付きは閉域の-opt-inは業界横断の到達点"></a>

### 收敛1：“默认擦除，类型化限于封闭范围可选”是跨行业终点

| 语言 | 默认 | 精化层 | 证据 |
|---|---|---|---|
| Swift | 无类型 `throws` | `throws(E)`（SE-0413） | 提案原文说既有无类型 `throws` 对大多数 Swift 代码**仍是更好的默认**。发布两年后实际使用几乎限于 Embedded Swift 与泛型机制（`AsyncSequence.Failure`） |
| Rust | 应用用 `anyhow` | 库用 `thiserror` | 两极在2026年仍持续（累计下载约8.6亿 / 13亿）。std 错误改进实际停滞，provider API 在2026-02转向放弃（转向 RFC 3885） |
| MoonBit | 裸 `raise`（≡ `raise Error`） | `raise E`（suberror） | 最新官方推荐的 `@string.parse_int` 为**裸 raise + 消息字符串**。core 统计：`raise?` 329 / 裸 `raise` 68 / 类型化29。stringly 还是结构化，在 core PR #3997 **仍有争议** |
| Unison | — | 有 `Throw e` | 生态收敛于 catch-all `Exception`/`Failure`，说明仅靠精化层无法支撑外围 |
| Zig | 默认类型化 set（逆向） | — | 大规模采用“边界显式 set、内部推断”的纪律，并有向 `anyerror` 侵蚀的压力。推断 set 不支持递归（#2971），2026年仍 open |

<a id="収斂-2-エラーはペイロードを運べなければならない"></a>

### 收敛2：错误必须能携带 Payload

Zig 在2024-12**永久否决** payload 提案 #2647。Andrew Kelley 关闭评论只有一句：“**Error codes are for control flow.**” 官方推荐的 diagnostics out-parameter 变通被实践者称为“极其不便”，stdlib 内应用也不一致（std.json 不支持）。Almide variant E 构造器能携带 payload，因此从一开始就没有这个缺失。

<a id="収斂-3-境界の自動合成union--row--setは対価が高い"></a>

### 收敛3：边界自动组合（Union / Row / Set）代价高昂

- **Roc**（结构化 open tag union，组合极端）：零边界仪式，代价是 open/closed 推断成为语言最难概念。
- **Koka**（effect row）：自动合并，代价是 row 推断错误信息难懂（2025年实践报告确认，内部类型变量名泄漏给用户）。
- **Zig**（error set）：组合成立，但前提是**依赖封闭世界编译**（matklad 分析），不能直接移植给其他语言。
- **Inko**（决定性数据）：实际交付最简单的“签名里单一名义 throw 类型”，却在**0.11.0撤回**。原文理由：“This makes composing errors much easier” — 强制单一名义类型在边界失效。Almide **不强制** variant E（默认 String），不同于此失败形态，但将其用作“精化层不应默认”的证据。
- **Gleam**（名义 + 仅显式转换，与 Almide 同形）：保留 `map_error` 仪式，核心决定是“不改”（2024-11否决 `?` 运算符）。生态退路 snag（单一不透明类型）的作者明确说“不要在库中使用”，证明擦除层需求存在。

<a id="収斂-4matklad-2025-12-現代エラーモデルの三点合意と未提供の中間点"></a>

### 收敛4（Matklad，2025-12）：现代错误模型三点共识与缺失的中间地带

Go/Rust/Swift/Zig 收敛于：(1) **调用点可失败标记**，(2) **独立 panic 通道**，(3) **作为值的错误**（“The Second Great Error Model Convergence”）。Java checked exceptions 的失败在于 E 变化**影响整条签名链**；现代设计只为0→1（全函数→可失败）付费，不为N→N+1（增加 variant）付费。而“穷举与完全擦除之间的**中间地带，没有任何语言充分提供**”。

Almide 当时语义满足三点，variant E 是名义类型，**也通过N→N+1测试**（ConfigError 增加 variant 不改变任何签名）。中间地带（默认擦除 + 带 payload 的精化层 + 强制显式有损转换）已经占据，只剩记法 — 这是本调查结论。

<a id="moonbit-の-te-放棄の解剖--記法ではなく呼び出し点モデルの問題だった"></a>

### 剖析 MoonBit 放弃 `T!E` — 问题在调用点模型，不在记法

MoonBit 在2024–2025放弃 `fn f() -> Int!String`，迁移到 `-> Int raise E`。拆解过程，痛点全在**调用点**和**符号过载**：

- 调用点标记两年三变（`f(x)!` → `f!(x)`〔明确为了 IDE 补全〕→ 删除）。
- `!` 在 `type!` / `f!` / `!!` / `catch!` 各有不同含义。
- `f?(g!(..))` 的语义陷阱（自行注明无法机械迁移）。

**没有第一手资料表明返回记法 `T!E` 本身有缺陷**（没有专门撤回文件）。迁移后的 MoonBit 将传播**隐式化**，以“IDE 给可抛错调用加下划线”补偿可见性（beta-release 原文：“The IDE will automatically _underline_ functions that may throw”）。这种补偿对**以 raw text 为界面的写作者（LLM、diff 审阅）不可见**，正是 Almide 在 ADR-0008 选择显式传播的理由。Almide 的 `!` 仅有传播运算符和返回标记两义（不在 `!` 上承载 trap，ADR-0002 D4），不会重现 MoonBit 的过载条件。

<a id="実証の穴機会"></a>

### 实证空白（机会）

**不存在**测量类型化 E 与擦除 E 对 LLM 生成准确率和修改存活率影响的研究（截至2026-08，LMPL 2025也无对应论文）。除 MoonBit 外，也没有语言以“LLM 易写性”为由公布错误设计。拥有 MSR 测量基础设施 Dojo 的 Almide，处于唯一能用带可证伪条件的实测填补空白的位置。

<a id="decision"></a>

## 决定

**宣布两层错误模型为终态，精化记法在标记内完成。于 fn 声明返回位置和 fn 类型 slot 引入 `-> T!E` ≡ `Result[T, E]`。默认仍为 String（`T!` ≡ `T!String`），stdlib 仍为 String（ADR-0004不变），传播点不转换原则不变（ADR-0003）。**

<a id="d1-二層ドクトリンの終着宣言層割当規準の明文化"></a>

### D1. 两层准则的终态声明（明确层分配标准）

在 spec 用一段明确：

- **擦除层（默认）**：E = String 是**报告通道**，供人/LLM阅读。context 用 `map_err` 前置（ADR-0004 D2）。
- **精化层（可选）**：variant E 是**分支通道**，程序根据内容改变行为。切换信号原样采用 ADR-0004 D1：“想按内容分支时，使用 variant E”。适用范围为**封闭域**（在 module/package 内捕获全部错误的地方，与 SE-0413 三个条件相同边界）。
- 层间降级（variant E → String）必须在边界以 `map_err` 显式可见（ADR-0003）。

两层 + 不转换 + 携带 payload 的精化层，填补 matklad 所说的缺失中间地带，也是本 ADR 主张的“2026年应有形态”。

<a id="d2-te--精密化はマーカー内で完結する"></a>

### D2. `T!E` — 精化留在标记内

```almide
type ConfigError = | Missing(String) | BadValue(String)

// read_cache: (String) -> String!ConfigError
fn load(path: String) -> Config!ConfigError = {
  let text = read_cache(path)!          // E 一致 → 構造のまま伝搬(ADR-0003 D1)
  guard valid(text) else err(BadValue(path))
  parse_config(text)                    // 値 tail → ok(...) に lift(T! と同一の人間工学)
}

effect fn main() -> Unit = {
  let cfg = load(p) |> result.map_err((e) => "loading config: ${e}")!  // 降格は可視
  ...
}
```

- **去糖**：`-> T!E` ≡ `-> Result[T, E]`。`T!` 重新定义为 `T!String` 的简写（修订 ADR-0002 D2，从固定 String 改为默认 String）。
- **位置**：与 `!` 标记当时合法位置完全相同：fn 声明返回位置、fn 类型 slot 返回位置（`op: (Int) -> Int!ParseError`）。不是类型构造器（不允许 `List[Int!E]`，保留 ADR-0002 D2 属性模型）。
- **E 语法**：`!` 后同一行紧接**具名类型原子**（允许限定名 `mod.Error`、泛型）。E 自身不能后缀 `?` / `!`（若需要 Option E，显式 Result；尚未观察到需求）。当时语法 `!` 后出现类型名的位置空闲（此前这里只能有 `=` `{` `,` `)` 与换行），故一个 token 前瞻即可无歧义解析。
- **函数体易用性**：与 `T!` **相同**（值 tail ok-lift、`!` 传播、合法 `err(...)`）。因此 `T!` → `T!E` 精化**只改签名一个位置**，消除背景2中强迫改写函数体的问题。
- **与 effect fn 组合**：`effect fn f() -> T!E` 表示 effect 且可失败（带 E）。这是既有“不双重包装显式 Result”规则的语法糖，不增加新规则。
- **传播**：不变，“`!` 不转换 E”（ADR-0003）。`T!E` 只扩大既有规则适用对象，不加转换、推断或特例。

<a id="d3-fmt-正準形--戻り位置の-result-はマーカーへ"></a>

### D3. Fmt 规范形式 — 返回位置 Result 转为标记

`almide fmt` 将 fn 声明和 fn 类型 slot **返回位置**的 `Result[T, String]` 规范化为 `T!`，`Result[T, E]` 为 `T!E`（显式 `T!String` 也转为 `T!`）。返回位置**之外**（let 注解、字段类型、类型实参）的 `Result` 不变，遵循 ADR-0002/0010：`!` 是箭头属性，不是值类型。

这关闭 ADR-0002 留下的“fmt 向哪边规范化”问题，采用与 ADR-0010 D3（`Option[T]` → `T?`）同样方法消除“一类型两记法”的张力。规范化保持语义：显式 Result 函数体内合法内容在 lift 下全都同构；lift 只扩大接受范围，不改变已有 Result 类型 tail 的含义。

<a id="d4-非目標の固定再確認--新証拠"></a>

### D4. 固定非目标（再确认 + 新证据）

1. **Stdlib 保持 String**（ADR-0004不变）。MoonBit core 现实（推荐 parser 用裸 raise + 消息，结构化有争议）及 Unison catch-all 收敛，再次支持擦除层 stdlib。再考虑条件仍为 ADR-0004 Falsifier。
2. **不引入转换钩子（相当于 From）、union / row / error set**（ADR-0003 D4 / 0004不变）。新证据：Koka row 推断消息难懂（2025实测）、Roc open/closed 难点、Zig set 封闭世界依赖、Rust provider API 放弃。
3. **传播继续显式**（ADR-0008）。不采用 MoonBit 式“隐式 + IDE 补偿”，因为 raw-text 界面可见性变为零。本调查为此决定**提供证据**（收敛4调用点标记共识同向）。
4. **Lambda 失败通道保持 String**（ADR-0009 L3）。使用驱动的 E 推断会引入 Koka 式推断错误难题。typed E lambda 用显式注解的 Result 表达（作为普通值完全工作）。
5. **Main 失败通道保持 String**。边界降级 `map_err` 是显式可见点（ADR-0003 后果）。

<a id="rationale"></a>

## 理由

<a id="精密化は表記モデルを跨いではならない"></a>

### 精化不应跨记法模型

Swift 是 `throws` → `throws(E)`，MoonBit 是 `raise` → `raise E`，Zig 是 `!T` → `E!T` — **不存在不在标记内完成精化的语言**。只有 Almide 强制“属性→泛型”跳跃，增加一套需学习模型，并因丢失 tail lift（背景危害2）连带改写函数体。`T!E` 是消除跳跃的最小一步。

<a id="最小差分こそ-msr"></a>

### 最小差异正是 MSR

“精化错误类型”是封闭域设计最常见修改类别。D2 后只需签名增加一个 token；传播点 E 不匹配由 check 错误 + 规范形式提示承接（#1103已实现）。“读错误消息、贴规范形式”属于 LLM 成功率最高的修改类别（ADR-0003 理由），整个精化路径都建立在既有安全网之上。

<a id="string-単一文化の希釈リスクには表記ではなくドクトリンで答える"></a>

### 用准则而非记法回应 String 单一文化被稀释的风险

`T!E` 廉价可能导致 variant E 滥造；这与 SE-0413 原文警告同形：“Resist the temptation to use typed throws because there is only a single kind of error”。Almide 的答案在准则：切换信号是**分支需求**（ADR-0004 D1 + E035 lint），不是写法廉价。即使写了不分支的 variant E，实际危害也小，降为 String 的边界必然以 `map_err` 可见。只要 stdlib 固定 String（D4-1），单一文化骨架不动。

<a id="儀式コストは-gleam-水準に留まりinko-の失敗形にはならない"></a>

### 仪式成本保持 Gleam 水平，不成为 Inko 失败形态

名义 E + 显式转换的代价（边界 `map_err`）与 Gleam 在2024-11裁定可接受的成本相同；Almide 通过提示（#1103）比 Gleam 更轻。不同于 Inko 撤回的“强制单一名义类型”，Almide 精化层可选，默认擦除，因此组合困难不会泄漏出封闭域。

<a id="alternatives--検討して却下した案"></a>

## 替代方案 — 考虑后否决

1. **Keyword 方式（`raise E` / `throws(E)` 风格新关键字）**：与 MoonBit 迁移终点同形。否决，因为 Almide 已交付后缀 `!`（ADR-0002），增加关键字会在**语言核心**制造一义两写法。MoonBit 放弃 `!` 的痛点（符号过载、调用点标记）在 Almide 不存在。
2. **隐式传播 + IDE 补偿（MoonBit 2025式）**：否决，与 ADR-0008 冲突。IDE 下划线不存在于 raw text，LLM 和 review diff 看不到。MSR 的读者是未来修改者，只读文本。
3. **Error set / 结构化 union / row（Zig / Roc / Koka式）**：再确认否决（ADR-0004 替代项2/4）。新证据见调查收敛3，组合代价（推断消息难懂、封闭世界依赖）得到2025–2026实证支持。
4. **From 类型转换钩子**：再确认否决（ADR-0003 D4）。Rust 自身 provider API 放弃（2026-02）旁证依赖远方 impl 机制的维护成本。
5. **保持现状（精化只用显式 Result）**：否决，模型跳跃和强制改写函数体（背景危害1、2）仍在。“能工作”为真，“最小差异工作”为假。
6. **随 `T!E` 同时类型化部分 stdlib**：否决，侵蚀 ADR-0004 范围。stdlib 再考虑仅通过0004的 Falsifier。

<a id="consequences"></a>

## 后果

**收益**：精化变成改签名一处。错误规范所有决定可由三句说明：“标记 = 可失败属性；E 默认 String、一个原子即可精化；转换始终可见”。关闭 ADR-0002 的 fmt 规范形式问题。

**代价**：
- 实现：parser 两处（声明返回和 fn 类型 slot 的 `Bang` 后允许类型原子），resolver 一处（伪泛型 `!` 的 args.len()==2），fmt 打印与规范化，spec/lang 测试及 CHEATSHEET / result-option-effect.md / llms.txt 更新。既有 `!` matcher（module_inference / lower / check）只匹配名称、不看参数数目，无需修改。
- D3 规范化对 stdlib / spec 全域产生一次差异（splice-context source 用 `--no-import-edit`，negative diagnostics fixture 排除，遵循既有 fmt 规约）。
- 可观测行为不变（仅记法），**无需更新契约台账**。`T!E` 语义与既有 explicit-Result 路径相同，既有 fixture 可原样作为证据。仅增加 `spec/lang/` 记法测试（parse、fmt、lift 同构）。

<a id="falsifier--何が起きたらこの決定を撤回するか"></a>

## 可证伪条件 — 何时撤回

1. **若 Dojo MSR 测量显示引入 `T!E` 后，因 E 不匹配 check 错误导致的修改失败显著增加**（证明廉价写法导致 variant E 滥造、String 单一文化稀释无法靠 hint 回收），以新 ADR 退役声明位置 `T!E`，精化恢复仅显式 Result。
2. **若 D3 规范化造成实际危害**（发现返回位置 Result 的刻意区分确实存在），只撤回 D3，缩为 `T!String` → `T!`。
3. **若 ADR-0004 Falsifier 先触发**（重新考虑 stdlib 结构化），本 ADR D4-1 从属0004新裁定。层结构 D1 和记法 D2 即使如此仍独立存续。

<a id="references"></a>

## 参考资料

- Swift SE-0413 — [Typed throws](https://github.com/swiftlang/swift-evolution/blob/main/proposals/0413-typed-throws.md)
  （“untyped `throws` remains the better default”及“Resist the temptation…”原文出处）、
  [FullTypedThrows 尚未交付](https://forums.swift.org/t/where-is-fulltypedthrows/72346)、
  [SE-0421 AsyncSequence.Failure](https://github.com/swiftlang/swift-evolution/blob/main/proposals/0421-generalize-async-sequence.md)
- Zig — [#2647 永久否决payload：“Error codes are for control flow.”](https://github.com/ziglang/zig/issues/2647)、
  [#2971 推断set递归（open）](https://github.com/ziglang/zig/issues/2971)、
  [diagnostics模式实践报告](https://srcreigh.ca/posts/error-payloads-in-zig/)
- Rust — [thiserror](https://docs.rs/thiserror) / [anyhow](https://docs.rs/anyhow)(2026-08下载实测)、
  [provider API转向放弃（#99301）](https://github.com/rust-lang/rust/issues/99301)
- MoonBit — [error-handling docs](https://docs.moonbitlang.com/en/latest/language/error-handling.html)、
  [2025-06-03](https://www.moonbitlang.com/updates/2025/06/03/index) /
  [2025-06-16迁移表](https://www.moonbitlang.com/updates/2025/06/16/index)、
  [beta-release（IDE下划线补偿原文）](https://www.moonbitlang.com/blog/beta-release)、
  [core PR #3997（stringly与结构化争议）](https://github.com/moonbitlang/core/pull/3997)、
  旧记法： [0722](https://discuss.moonbitlang.com/t/the-moonbit-update-0722/271) /
  [0729](https://discuss.moonbitlang.com/t/the-moonbit-update-0729/272)
- matklad — [The Second Great Error Model Convergence(2025-12-29)](https://matklad.github.io/2025/12/29/second-error-model-convergence.html)
- Inko — [0.11.0撤回](https://inko-lang.org/news/inko-0-11-0-released/)
- Gleam — [`?` 运算符否决（2024-11）](https://github.com/gleam-lang/gleam/discussions/3908)、
  [snag](https://github.com/lpil/snag)
- Roc — [FAQ(tag union rationale)](https://www.roc-lang.org/faq)、
  [open/closed union分析](https://gist.github.com/j-maas/ed3d2811d808d0fa1386478575df928d)
- Unison — [abilities error handling](https://www.unison-lang.org/docs/fundamentals/abilities/error-handling/)
- Koka — [row-polymorphic effects](https://arxiv.org/abs/1406.2061)、
  [2025实践报告（row推断困难）](https://gfxmonk.net/2025/04/13/im-excited-about-koka.html)
- 内部 — ADR-0002/0003/0004/0006/0008/0009/0010、#1103、C-211、
  `crates/almide-syntax/src/parser/declarations.rs`(lift分支)、
  `crates/almide-frontend/src/canonicalize/resolve.rs`(`!` 去糖)


<a id="amendment-2026-08-20-d3-は-collapse-only-に縮小"></a>

## 修订（2026-08-20）：D3 缩为仅 Collapse

#1194 实现 rollout 对 D3“规范化保持语义”的前提提供了**三类反例**（均由实际程序/fixture 复现，为 formatter 引发行为变化）：

1. **具名 callback 路由**：ADR-0006 D1 将 HOF first-err 路由关联到目标声明的**标记写法**（`fallible_marker_fns` 对裸 `Ident` 作文本匹配）。把 `fn f() -> Result[Int, String]` 改写为 `-> Int!`，会让 `list.map(xs, f)` 从 `List[Result[..]]` 变成 first-err `Result[List[..], E]`。在 `"${list.map(ss, f)}"` 这类形式里，两种写法都编译通过，**只有输出改变**。
2. **Fn 类型 slot 接受范围**：显式 `(Int) -> Result[Int, String]` slot 不接受标记声明 callback，也不接受标记 lambda（3 error）；`(Int) -> Int!` 却接受。改写 slot 改变 checker 接受集合。
3. **V1 lowering wall 一致性**：fan 上下文的 effect fn（`fan_any_early_winner` 的 `ok11` 系）在显式 Result 下可 render 到 v1，标记写法却遇到 heap-result-match wall。绿色 fixture 因改写变红。

因此 D3 fmt 规范化暂时**只做 `T!String` → `T!` collapse**（标记内统一写法，完全不移动语义轴）。`Result[T, E]` → `T!E` 改写须等**整个编译器中标记/显式行为等价成立后**（先统一上述1–3）。冻结 A0-1 的规范规则确定为“两种写法都合法，fmt 仅 collapse”。

<a id="amendment-2026-09-27-d4-4-は-adr-0021-で置き換え"></a>

## 修订（2026-09-27）：ADR-0021 取代 D4-4

D4-4“lambda 失败通道保持 String”由 [ADR-0021](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0021-lambda-failure-channel-is-the-join-of-its-bangs.md) 取代。通道 E 若有期望类型则用它，否则为函数体 `!` operand 的 E 的 join（E ≤ String 顺序上的最小上界，全域），默认 String。担心的“Koka 式推断错误难题”不会在无 row 变量的全域 join 中发生。“typed E lambda 用显式注解 Result”的替代路径，经实测并不存在（类型化 slot 为 E005，类型化 `let` 为 E001）。D4-4 所引用的“ADR-0009 L3”实指 `docs/specs/result-option-effect.md` 的 L3。
