<a id="adr-0002-fallibility-and-effect-are-orthogonal-axes---t-marks-pure-fallible"></a>

# ADR-0002：可失败性与 effect 是正交轴；`-> T!` 标记纯可失败函数

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](0002-fallibility-effect-orthogonal.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](0002-fallibility-effect-orthogonal.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](0002-fallibility-effect-orthogonal.zh-CN.md)

> 非规范性历史读者快照。未修改的固定原本为权威来源；不自动同步，未独立执行、运行基准或证明安全性。D6撤回effect-total象限及Phase 2/3，保留pure-fallible的Phase 1。前文正交性和auto-?描述属于历史，不是当前API保证。静态可失败性分类不证明host全域性。其他语言比较和纯粹性概括是原作者当时的主张，此处未独立验证。保留测量日期、证据较弱的限定及已撤回计划。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0002-fallibility-effect-orthogonal.md)

- **状态**：已接受，**2026-09-24 部分撤回**（§D6）。Phase 1 已交付 #1103 / #1108。
  “effect・全”象限及为其引入的 Phase 2 / Phase 3 已撤回 — `effect fn f() -> T` 始终表示
  `T!`（#2563 为 not planned）。pure 一侧的 `-> T!` 不变。
  **Falsifier 2 已测量，2026-09-23**（#2556、develop 8e0c2dcbb、playground 958ddea）：
  987 个 effect fn 中，**无法静态分类的只有 4 个**（`-> T` intrinsic，无 Almide 函数体：
  `env.set` / `env.sleep_ms` / `http.serve` / `process.exit`）。其余为 fallible 574 / total 307 /
  intrinsic 但签名为 `-> Result` 因而可失败 82 / intrinsic 但 self-host 函数体为全函数 20
  （各列明细见 §Falsifier 2 表）。
  复现：`ALMIDE_BIN=target/release/almide python3 tools/effect_fn_fallibility.py stdlib=stdlib spec=spec playground=<playground>/web/examples`
  （参见 §Falsifier 2 追加内容。○ 2026-09-23 — Falsifier 2 未触发（剩余 4/987），Phase 2 建立 issue #2563。
  随后 2026-09-24 在 §D6 撤回整个象限，因此该测量不再用作 Phase 2 的依据。）
- **日期**：2026-08-05
- **决定范围**：函数声明的失败通道记法（`-> T!`）、重新分解 `effect fn` 的语义、
  固定 `!` 运算符的含义、以“值”还是“⊥（受控停止）”表示失败的准则
- **相关**：[#1055](https://github.com/almide/almide/issues/1055)（effect 类型的函数参数 — 本 ADR 提供术语）、
  C-211 / C-029（[contracts.toml](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/contracts/contracts.toml)）、
  [result-option-effect.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/result-option-effect.md)（须随本 ADR 修订）、
  #489（lambda 边界）、#840/#841（never-err elision）
- **批准**：2026-08-05 的设计讨论。起点是“`fn f(s: String) -> Int = int.parse(s)!` 会触发
  E022，理应如何处理”。采用同日对全部运算符 × 全部上下文的实测
  （v0.53.6、287 个单元、7 个切片并行 probe）作为证据。

<a id="context--何が問題だったか"></a>

## 背景 — 原先的问题是什么

<a id="1-effect-fn-は-3-つの性質の縮退である"></a>

### 1. `effect fn` 把 3 种性质合并在一起

当时的 `effect fn` 把以下内容合并为一个关键字：

1. **capability** — 可以调用 host effect（fs / http / process / random …）
2. **fallibility** — 声明 `-> T` 被提升为 `Result[T, String]`
3. **易用性** — 函数体内运行 auto-`?`，且 `!` 合法

因此，没有简短的写法表达“会失败但不接触外部世界的函数”。
像 `int.parse` 这样的函数只能手写 `-> Result[Int, String]`（19 个字符），
E022 的提示也只能列出“写 Result / 改为 effect fn / 使用 `??`”，即
**并列 3 条含义不同的退路**。

<a id="2-実測が示した縮退の歪み2026-08-05-matrix抜粋"></a>

### 2. 实测暴露出的合并问题（2026-08-05 matrix 摘录）

- 仅为 error-lift 而使用 `effect fn`，会附带 capability，因而不能从 pure 调用
  （E006）。反过来，重新声明为显式 `-> Result` 后，**竟然又可以从 pure fn 传递**
  （effect 洗白）— 这说明 capability 与 fallibility 是不同性质。
- codegen 已有 never-err elision（#840/#841），
  **在幕后作为优化识别**“不会失败的 effect fn”（`env.args`、`random.int`）。
  只是表层没有类型，实现在内部已经认识这两个轴。
- `try_*` 系列的 callback（`(A) -> Result[B, E]`）中，E 无法确定导致的摩擦
  （E025 簇）频繁出现 — 因为没有在 callback slot 表达 fallibility 的术语。
- C-211（0.53.5）已允许 pure fn `-> Result/Option` 中使用 `!`，
  **pure-fallible 象限在语义上已经存在**。缺少的只是记法。

<a id="3-pure-が-result-を返してよいのかへの回答が用語として無かった"></a>

### 3. 术语中没有回答“pure 是否可以返回 Result”

spec 没有写明 purity / fallibility / totality 的区别，
于是“明明 pure 却会失败，不奇怪吗”的困惑可能在每次设计讨论中重演。

<a id="decision--何を決めたか"></a>

## 决定 — 决定了什么

**将可失败性（fallibility）与效果（effect）分离为正交的两个轴。
`effect` 关键字只表示 capability，返回类型位置的后缀 `!`（`-> T!`）
只表示 fallibility。** 最终语法为 4 个象限：

> **2026-09-24 修订（§D6）**：第 3 行“effect・全”在用户声明中撤回。effect 蕴含可失败性，
> `effect fn f() -> Int` 确定为与第 4 行含义相同（有 lift）。stdlib 的 never-err effect fn
> 继续按既有方式在调用方返回裸值（§D6 的表）。

```almide
fn        f() -> Int      // pure  ・総(失敗しない)
fn        f() -> Int!     // pure  ・可謬     ← 新設(Phase 1)
effect fn f() -> Int      // effect・総       ← 撤回(2026-09-24、§D6)— 4 行目と同義
effect fn f() -> Int!     // effect・可謬     ← 今の effect fn
```

<a id="d1-用語の規準spec-に定義段落として置く"></a>

### D1. 术语准则（作为定义段落放入 spec）

- **pure** = 函数体不接触外部世界（没有 host effect）。引用透明、可复现。
- **fallible** = 返回值被提升为 `Result[T, String]`。**这是陪域形状的问题，
  与纯粹性无关**。`int.parse : String -> Result[Int, String]` 完全是纯函数
  （相同输入 → 相同 Result。使偏函数全函数化）。
- **失败的表示准则**：结果**仅由参数决定**的失败，以值（Result）表示。
  **依赖外部世界状态**的失败，在 effect 范畴表示。
  **以 ⊥（T6 受控停止）表示的失败是封闭枚举**（算术定义域错误、下标越界），
  不轻易扩大。转换为进程级失败（exit 1）只发生在
  main / effect 范畴 / 显式运算符的边界。

<a id="d2---t-の表面"></a>

### D2. `-> T!` 的表层形式

- `!` 只能写在 **fn 声明的返回类型位置**和 **fn 类型的参数 slot**（`f: (A) -> B!`）。
  **它不是类型构造器**（不允许 `List[Int!]`）— 与 Swift 的 `throws` 相同，
  采用“函数属性”模型。
- 指称含义：调用 `-> T!` 的函数会返回 `Result[T, String]`。仅此而已。
- **E 固定为 String**。自定义 E 仍显式写作 `-> Result[T, MyErr]`。
  （`!` 专用于“公共 E 的可失败性” — 适应 stdlib 的 E 全为 String 的实际情况。）

<a id="d3-宣言人間工学は-fallibility-軸の持ち物"></a>

### D3. 声明易用性属于 fallibility 轴

`-> T!`（以及 `-> Result[T, String]` 声明）的函数体获得与当时 effect fn **相同的**
lift 易用性：尾部 T 自动 ok、对可失败调用自动 `?`、允许 `!` 和 `err(...)`。
不附加任何 capability（E006 系）。
由此，“effect fn 的易用性”被拆成“fallibility 的易用性 + capability 检查”。

<a id="d4--演算子の意味は伝搬のみに固定する"></a>

### D4. 将 `!` 运算符的含义固定为“传播”

`x!` = “将 x 的失败转换到包围它的声明的失败通道并传播”。没有通道则报
E022。**不把 trap（unwrap-or-abort）语义加到 `!` 上。不悄悄提升签名**
（参见替代方案）。将 E022 重新表述为“没有失败通道”这一类错误，
按 operand 类型选择提示，对尾部形式给出可机器应用的
`try: fn f(s: String) -> Int! = int.parse(s)`。
同时，把 `!` 的 E 类型不匹配（String operand → CustomE fn）改为 **check 阶段错误**
（当时会通过 check → ICE。也废除反方向隐式 Debug 字符串化，
提示插入 `result.map_err`）。

<a id="d5-移行e040-deprecation-窓の前例を踏襲"></a>

### D5. 迁移（沿用 E040 deprecation 窗口的先例）

- **Phase 1（仅添加、非破坏性）**：为 pure 引入 `-> T!`。更新 E022 提示。
  将 `try_*` 系列等 callback slot 更新为 `(A) -> B!` 记法（含义不变）。
- ~~**Phase 2（警告窗口）**~~ **已撤回（§D6）**：允许 `effect fn f() -> T!` 作为当时 effect fn 的同义写法。
  对函数体可失败却写为 `-> T` 的 effect fn 发出 deprecation 警告，由 `almide fix` 机械插入 `!`。
- ~~**Phase 3（语义反转）**~~ **已撤回（§D6）**：将 `effect fn f() -> T` 重新定义为**全函数**（无 lift）。
  `env.args` / `random.int` 等签名变得如实，never-err elision 获得类型依据。
  批量更新 stdlib 声明。**Phase 3 实施时间未定**（先观察 Falsifier 条件）。

<a id="d6-改訂2026-09-24-effect-は可謬を含意する--effect総象限の撤回"></a>

### D6. 修订（2026-09-24）：effect 蕴含可失败性 — 撤回“effect・全”象限

**用户声明的 `effect fn f() -> T` 始终表示 `T!`（提升为 `Result[T, String]`），
以此为最终形式。** 不实施 Phase 2（接受 `-> T!` 写法和警告窗口、#2563）及 Phase 3
（将 `effect fn -> T` 语义反转为全函数）。pure 侧的两个象限（`-> T` / `-> T!`、Phase 1）不变。

**当时的事实（2026-09-24，在 0.63.0 dev 上实测 check）** — 调用方所见行为随声明来源而不同：

| 被调用方 | 例子 | 不带 `!` | 带 `!` |
|---|---|---|---|
| stdlib 的 never-err effect fn | `random.float()`、`io.read_line()` | 裸值（`Float` / `String`） | 通过（no-op） |
| 用户声明的 effect fn（即使函数体不失败） | `effect fn roll() -> Float = random.float()` | `let` 中 E041，语句中 E042 | 通过（传播） |

stdlib 侧的处理是 never-err elision（#840/#841）暴露到 checker 的结果，本修订
不改变这一点。fern 示例需要 `walk(..)!`，是因为 `walk` 是用户声明的 effect fn；
`random.float()` 从一开始就不需要 `!`。

理由：

1. **它与本 ADR 自己的准则（D1）矛盾。** D1 规定“依赖外部世界状态的失败在 effect 范畴表示”。
   effect fn 按定义接触外部世界，因此按该准则属于可失败。“effect・全”是准则以外的格子。
2. **用户声明的 effect fn 是否“全”，只是字面判断。** #2556 的分类器只看函数体中
   是否没有 `!` / `err`，并不看调用的 host 侧会不会因外部世界状态而失败
   （SIGPIPE、OS 随机源、信号中断）。stdlib 的 never-err 集合是在 runtime 侧
   逐个处理后的声明，用户代码没有相同保证。
3. **从 MSR 看，损失很大。**
   - 修改局部性：给全函数的 effect fn 增加一个可失败调用，就会让签名变为 `-> T!`，
     全部调用者都需要 `!`（向上传播）。当时所有 effect fn 已 lift，因此修改只限于函数内。
   - Phase 2 窗口使 `-> T` / `-> T!` 同义共存，恰是 Falsifier 1 担忧的情况。
   - 收益很小：没有意义的 `!` 只会从 spec 37 处 / playground 7 处消失。运行时性能
     已由 never-err elision（#840/#841）获得。
   - 当时的成本很小：Dojo 2026-09-22〜23 的运行中，漏写 `!`（E041/E042）在
     约 1,000 个 check 错误中只有 1 个（但包含 effect fn 的任务仅占 234 条记录中的 48 条，证据较弱）。

结果：

- 即使函数体不失败，用户声明的 effect fn（fern 的 `walk`）调用也需要 `!`，
  这一现状作为**规范**保留。对来自 Swift / Rust 的读者可能像仪式，但并非写作者（LLM）的失分来源。
  若统一写成“调用 effect 就加 `!`”，给 stdlib 的 never-err 调用加 `!` 也会
  以 no-op 通过，因此写作者不必区分调用目标。
- 写法保持 `effect fn f() -> T`。不采纳强制 `-> T!` 的方案，因为它重复表达与 `effect` 相同的
  信息，需要迁移约 880 个签名，还会引入“漏写 `!`”这一新错误。
- never-err elision 继续作为 codegen 优化，而不是类型性质存在。
- fn 类型一侧的镜像（ADR-0009 D1 的 `effect (A) -> B`）同样处理 — effect 位为真就可失败。

<a id="rationale--なぜそれか"></a>

## 理由 — 为什么这样决定

<a id="純粋性の理論"></a>

### 纯粹性理论

失败值（Result）完全不损害引用透明性。损害它的是 ⊥；
Haskell（`Either`）/ Elm（`Result`）/ Gleam 等纯粹性强的语言，
失败**只能用值**表示。`-> Int!` 是保持 purity 的方案，
trap 则是扩大已有妥协（T6）的方案。

<a id="他言語の実測比較2026-08-05-調査"></a>

### 其他语言的实测比较（2026-08-05 调查）

具有类型化错误的语言均“**拒绝**在没有失败通道的函数中 unwrap”，且
“trap 使用**另一种写法**”：

| 语言 | 传播 | 没有通道时传播 | trap 写法 |
|---|---|---|---|
| Rust | `expr?` | 以 E0277 拒绝（“the `?` operator can only be used in a function that returns `Result` or `Option` (or another type that implements `FromResidual`)”） | `.unwrap()` / `.expect()` |
| Swift | `try` + `throws` | 拒绝（“errors thrown from here are not handled”） | `try!` |
| Zig | `try expr` | 拒绝（返回类型不是 error union）。**可用 `fn f() !i32` 的一个符号声明，error set 由推断得出**（“An error set type and normal type can be combined with the `!` binary operator to form an error union type” — Zig Language Reference） | `catch unreachable` |
| Gleam | `use` 语法糖 | 结构上不可能 | `let assert Ok(x)` |
| Go | 无运算符 | —（`try` 提案 [golang/go#32437](https://github.com/golang/go/issues/32437) 因对显式性的顾虑而**被否决** — 对隐式传播的警示） | 无 |

`-> T!` 把 Zig 的“声明一个符号”引入 Swift 的“函数属性”模型。
不过 Swift `try!` / Kotlin `!!` 中，**`!` 一类才是 trap**，而 almide 的 `!`（传播）
与通常的符号直觉相反 — 在 CHEATSHEET / llms.txt 明确写明“almide 的 `!` 是 Swift 的 `try`，
不是 `try!`”（D4 的推论）。

<a id="e-変換"></a>

### E 转换

Rust 为 `?` 自动插入 `From::from(e)`，**没有 From 则在 check 阶段报错**。
almide 当时的状态（一个方向 ICE，反方向隐式 Debug）是该模型的退化形式，
D4 的 check 阶段报错就是纠正它。本 ADR 不决定是否引入转换钩子（相当于 From）
（开放问题）。

<a id="期待できる波及matrix-の実測に基づく"></a>

### 预期影响（基于 matrix 实测）

- `!` 依上下文而异的 4 种含义归为一个原则，E022 修正变成“一个字符”
- `try_map` 的签名可读为 `fn try_map[A, B](xs: List[A], f: (A) -> B!) -> List[B]!`，
  slot 的 `!` 将 E 确定为 String，因此 **E025 摩擦簇在结构上消失**
- 用两轴的类型术语回答 effect 洗白及“auto-lift 的 effect fn 无法传给 callback”的问题
  （#1055 的 capability 轴 × 本 ADR 的 fallibility 轴）
- `http.serve` 的 handler slot 可写成 `effect (HttpRequest) -> HttpResponse!`

<a id="alternatives--検討して却下した案"></a>

## 替代方案 — 考虑后未采纳的方案

1. **悄悄 lift**（仍为 `-> Int`，实际是 Result）：签名不真实，产生两种声明上无法区分的
   pure fn。effect fn 之所以允许，是因为有 `effect` 关键字作为
   标记。**拒绝**。
2. **赋予 `!` trap 语义**（err 时 T6 halt）：调用方无法拦截（`??` / match），
   从 effect fn 提取为 pure helper 时，语义会**悄悄**从“传播”变为“立即终止”。
   与 MSR（modification survival rate）目标直接冲突。**拒绝**。
3. **新增 trap 专用写法 `!!` / `assert` 一类**：具有类型化错误的语言都有
   （如 Rust `.unwrap()`），因此思路并不差；但 `-> T!` 使“需要 Int”的大部分需求
   能在可失败上下文中通过传播满足，必要性降低。**暂缓**（不是拒绝）。
   **2026-08-07 再确认：继续搁置**（在 #1133 裁定并关闭）。原因是 v0.54〜v0.56
   刚完成“减少写法”的阶段（删除 auto-?、删除 7 个 try_ 函数、删除 collect、`Option[T]`→`T?`），
   此后立即添加新符号方向相反。#1133 明确列出 3 个放行条件：
   (1) 至少一个 sentinel 传播错误值的实例，或 Dojo MSR 中测得受阻的实例，
   (2) 引入时同时确定后缀族（`!`/`!!`/`?`/`??`/`?.`）的优先级表，
   (3) 同时裁定消除 L9 的 test 分叉（拆开会重复工作）。
4. **Zig 风格 E 推断**（从函数体推断 `!T` 的 error set）：E 的多态化不符合 stdlib
   “公共 E = String”的现实，会降低类型展示的可预测性（LLM writability）。
   **拒绝**（String 固定 + 自定义 E 使用显式 Result）。
5. **`T?` = Option 语法糖**（Kotlin 风格）：这不是 fallibility 轴，而是一般类型记法问题。
   作为范围外事项**分离**（是否采纳另行讨论）。
6. **维持现状**（仅显式 `Result[T, String]`）：可以运行（C-211），但 E022 提示的
   陷阱（对 Result operand 提议 Option 不成立）、E025 摩擦、effect fn 合并问题仍存在。
   19 个字符的摩擦直接影响“将失败写入类型”文化的普及速度。**拒绝**。

<a id="consequences--何が良くなり何を払うか"></a>

## 后果 — 收益与代价

**收益**：上述理由中的 4 项影响。此外，`effect` / `!` 回到一个关键字一个含义，
stdlib 的签名变得如实（Phase 3）。**2026-09-24 修订：Phase 3 已撤回（§D6）。
用户声明的 `effect` 除 capability 外也蕴含可失败性 — “一个关键字一个含义”只在 pure 侧成立。**

**代价**：
- 增加语法表面（返回位置的 `!`、fn 类型 slot 的 `!`）及其教学成本。
- **两种记法**：`(A) -> B!` 与 `(A) -> Result[B, String]` 指称相同。
  与 0.53.5 的 one-name-one-meaning 运动（#1078）存在张力，但后者是“一个函数两个名字”，
  这里是“一个类型两种记法”（如 `16` 与 `0x10`），用一条指称规则固定。
  **fmt 应规范化到哪种形式尚未决定**（开放问题）。
- ~~Phase 3 是触及全部 effect fn 签名的迁移~~（已撤回，§D6）（虽可机械改写，但对外部包的
  影响仍需像 E040 时一样扫描下游）。
- 向 pure-fallible 引入 auto-`?`，也把 auto-`?` 的位置不对称
  （matrix B 论点：行为因位置、注解、模式形状不同而变化）带入 pure 范畴。
  改革位置矩阵本身是**另一条路线**（本 ADR 的附属开放问题）。

<a id="falsifier--何が起きたらこの決定を撤回するか"></a>

## 可证伪条件 — 发生什么时撤回决定

1. **若 dojo 的 MSR 测量表明 `T!` / `Result[T, String]` 两种记法共存，
   相比单一记法显著降低存活率**（若证实记法重复比 19 字符摩擦危害更大，
   则以新 ADR 取代本决定，放弃 `!` 记法，或让 Result 记法退出 fn 声明位置）。
2. **若 Phase 2 警告窗口内发现大量无法静态分成可失败/全函数的 effect fn**
   （Phase 3 的语义反转无法成立 — 退回保持 effect fn 强制 lift 的形式）。
   **测量（2026-09-23、#2556，在建立 Phase 2 issue 前进行一次）** — `tools/effect_fn_fallibility.py`
   （由 `almide --emit-ast` 驱动。分类规则见 #2556 表：`!` / `err(..)` / 显式 `-> T!`・`-> Result[..]`
   = fallible，intrinsic 函数体 = unclassifiable，其余 = total。同时列出 codegen never-err 判定
   `compute_can_err`（crates/almide-mir/src/lower/mod_p2.rs）的镜像）：

   不将 intrinsic（`= _`）全部放入一个“unclassifiable”列，而按什么能确定它分成 3 列。
   Falsifier 只针对 **opaque 列**，其余两列已静态确定：

   | corpus | effect fn | fallible | total | intrinsic：由签名确定可失败 | intrinsic：由函数体确定为全函数 | **intrinsic：opaque** |
   |---|---|---|---|---|---|---|
   | stdlib | 174 | 57 | 11 | 82 | 20 | **4** |
   | spec | 792 | 502 | 290 | 0 | 0 | **0** |
   | playground（web/examples、958ddea） | 21 | 15 | 6 | 0 | 0 | **0** |

   - 由签名确定可失败 = 函数体为 `= _`，但签名为 `-> Result[..]`（fs / http / net / process / zlib …）。
   - 由函数体确定为全函数 = 有 self-host 函数体（self_host_registry.rs），且该函数体分类为 total
     （`random.*`、`env.args` 等 — fern 的 `!` 的来源）。
   - **opaque** = `-> T` 且无 Almide 函数体的 4 个（`env.set`、`env.sleep_ms`、`http.serve`、
     `process.exit` — 均为 `-> Unit`/`-> Never`）。逐个通过声明决定为 `-> T` / `-> T!`。
   - 工具原始输出分为 3 类（unclassifiable = 106 = 82 + 20 + 4）。明细通过
     `unclassifiable refined:` 行和 `--list unclassifiable` 输出。
   虽写有 `!` 但目标为 never-err、在 codegen 中为全函数（Phase 3 会消除 `!` 的形式）的数量：
   spec 37 / playground 7（含 fern 的 `walk(..)!`）/ stdlib 0。
   spec 有 1 个未测量（`spec/gauntlet/cells/s3_module_qualified_protocol`，parse 拒绝单元）。
   **○ 2026-09-23 — Falsifier 2 未触发（剩余 4/987），Phase 2 建立 issue #2563**（数值如上，判断见 #2556 的 Exit 节）。
   **2026-09-24 — 本 Falsifier 连同问题本身一并消失**：§D6 撤回“effect・全”象限，不再实施 Phase 2 / 3，
   因而无需再将 effect fn 分成可失败/全函数（#2563 为 not planned）。
3. **若 #1055 的设计得出“fallibility 应为类型构造器而非函数属性”的结论**
   （与 D2 属性模型矛盾，因此用统一 ADR 取代）。

<a id="references"></a>

## 参考资料

- 实测数据：2026-08-05 运算符 × 上下文矩阵（v0.53.6、287 个单元。
  probe：会话 scratchpad `matrix/`、汇总 `matrix_results.json`）
- Rust: The Rust Reference — [The question mark operator](https://doc.rust-lang.org/reference/expressions/operator-expr.html#the-question-mark-operator)、
  [`std::ops::FromResidual`](https://doc.rust-lang.org/std/ops/trait.FromResidual.html)
- Swift: [The Swift Programming Language — Error Handling](https://docs.swift.org/swift-book/documentation/the-swift-programming-language/errorhandling/)
- Zig: [Zig Language Reference — Errors](https://ziglang.org/documentation/master/#Errors)
- Go: [proposal: Go 2: error handling: try statement](https://github.com/golang/go/issues/32437)（已否决）
- Gleam: [Gleam Tour — let assert](https://tour.gleam.run/advanced-features/let-assert/)
- 内部：C-211 / C-029（docs/contracts/contracts.toml）、#489、#840/#841、#1051、#1055、#1078
