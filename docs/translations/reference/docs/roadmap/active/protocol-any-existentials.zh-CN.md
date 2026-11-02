原文说明 metadata：声明式符合 + 主动选择的 `any P` 存在类型——采用 Go 的 interface 值易用性，而不采用其隐式满足与 nil-interface 陷阱；唯一值得借鉴的 Swift 想法，其余均不采用。

<a id="protocols-declared-conformance--opt-in-any-p"></a>

# Protocol：声明式符合 + 主动选择的 `any P`

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](protocol-any-existentials.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](protocol-any-existentials.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](protocol-any-existentials.zh-CN.md)

> 固定原文的非规范读者快照；原文仍为权威，不自动同步。未独立验证实现或执行引用指令。历史状态、计划及比较主张保留原范围。
> [Original source (English) / 原文（英語）](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/protocol-any-existentials.md)

> 原文注记：Active 表示设计已记录但未排期，不表示存在类型支持已实现。protocol 文档审计是独立前置步骤。格局图及语言比较是本提案的简化和偏好，不是独立核实的普遍行业或安全结论。enum 优先表示、内置交互、交集存在类型及返回 Self 的处理，均保留正文所述待设计评审的边界。


> 2026 年的 protocol 设计格局没有唯一赢家，但已有定论。本 roadmap 只采用一项功能（`any P` 存在类型），刻意拒绝其余部分，并记录理由以免重复争论。

状态：**Active**——设计已记录，尚未排期。前置步骤（文档漂移）立即处理。

<a id="where-almide-stands-today"></a>

## Almide 当前的位置

符合是**声明式且名义式的**（`type GreetAction: Action`），由约定方法（`fn GreetAction.execute(...)`）满足，并在声明处检查。8 个内置 protocol 注册于 `crates/almide-frontend/src/canonicalize/protocols.rs`：

| protocol | signature |
|---|---|
| `Eq` | `fn eq(a: Self, b: Self) -> Bool` |
| `Repr` | `fn repr(v: Self) -> String` |
| `Ord` | `fn cmp(a: Self, b: Self) -> Int` |
| `Hash` | `fn hash(v: Self) -> Int` |
| `Codec` | `fn encode(v: Self) -> Value`, `fn decode(v: Value) -> Result[Self, String]` |
| `Encode` / `Decode` | Codec 的两个单向部分 |
| `Numeric` | generics 的 numeric-bound |

**前置步骤：完成文档盘点。** CHEATSHEET 的 serialization 节已说明 `Encode` 和 `Decode`；声称它们完全没有文档已经过时。对照注册表审计主要 protocol 表及 `Numeric`，再加入 drift gate（llms.txt 的先例）。这项文档工作与实现 `any P` 存在类型分开。

<a id="the-2026-landscape-in-one-map"></a>

## 一张图看 2026 年格局

```text
                    conformance declaration
             implicit (structural)   explicit (declared)
           ┌───────────────────────┬───────────────────────┐
 dynamic   │  Go                   │  Almide + `any P`     │
 (existen- │  (interface values    │  ← THIS ROADMAP       │
  tials)   │   are the default)    │                       │
           ├───────────────────────┼───────────────────────┤
 static    │  TypeScript           │  Rust / Swift /       │
 (mono-    │  (structural types)   │  Almide today         │
  morphic) │                       │                       │
           └───────────────────────┴───────────────────────┘
```

图示说明：横轴为符合声明，左侧隐式（结构式），右侧显式（声明式）。上行是动态（存在类型）：左侧 Go 默认使用 interface 值，右侧 Almide 加 any P 即本 roadmap。下行是静态（单态）：左侧 TypeScript 的结构类型，右侧 Rust / Swift / 当前 Almide。

本提案采用的设计偏好（不是普遍行业定论）：

- **默认 static dispatch，存在类型必须显式**（Swift 的 `some`/`any`、Rust 的 `impl`/`dyn`）。本提案选择显式区分，不证明全行业已普遍趋同。
- **Coherence 不可妥协**（Scala 2 implicits 是警示）。Almide 在类型处声明符合，从构造上保证 coherence：一个类型的符合关系集中一处，所以不会出现“哪个 instance 胜出”。
- **Deriving 是杀手级用途**（Rust 的 serde 塑造了 ecosystem）。Almide 已押注于此（Eq/Repr/Ord/Hash/Codec 的 auto-derive）。

<a id="what-we-take-and-from-whom"></a>

## 从哪里采用什么

**从 Go——主动选择 interface 值的易用性。** 动机用例属于存在类型：异构 handler list、plugin table、“任何可写入的东西”。LLM 从 Python/TS/Go 带来这种先验，并不断写出：

```text
let handlers: List[any Action] = [GreetAction { .. }, LogAction { .. }]
for h in handlers { h.execute(ctx)! }
```

目前 Almide 无法表达它（list 是单态的）。LLM 写出、失败、MSR 受损。`any P` 用一个 keyword 填补此缺口。

**从 Swift——只有 `any` 写法及其清晰性。** 其他一概不取。

**从 Rust——没有新东西**（已有 deriving 的押注及 coherence）。

<a id="what-we-explicitly-reject-and-why"></a>

## 明确拒绝什么及原因

| 功能 | 拒绝原因 |
|---|---|
| **隐式结构满足**（Go） | 意外符合——方法名称偶然相同就让类型满足 protocol——是一类直接针对 LLM 所写 code 的静默意外。符合仍是声明的意图。 |
| **Go 的 nil-interface 语义** | 永恒的 Go #1 FAQ（“interface 里的 nil 不是 nil”）。存在类型保持主动选择，且 Almide 没有 null，因此该陷阱表面不存在。 |
| **追溯性符合**（Swift） | 允许两个 module 以不同方式让同一外部类型符合——Swift 6 自身也开始警告的 coherence 缺口（`@retroactive`）。Almide 在类型处声明的规则从结构上阻止此事，应保持。 |
| **关联类型**（Swift/Rust） | 类型级编程的错误信息对人不可读，对 LLM 更糟。推迟到有具体 stdlib 需求时；generics + concrete type 足以覆盖当前表面。 |
| **条件符合** | 内置类型已在关键处按结构工作（container 上的 Eq/Hash）。一般化增加的是复杂性，不是能力。 |
| **命名 instance / HKT** | 超出 Almide 使命范围；复杂性损耗 MSR。 |

<a id="design-sketch-any-p"></a>

## 设计草案：`any P`

- **类型**：`any Action` 是一等类型，任何可用类型的位置都能用（尤其 `List[any Action]`、record field、param、return）。
- **引入**：在声明符合边界做隐式 coercion——将 `GreetAction` 赋给或传到期待 `any Action` 的位置时包装它。只有声明的符合才有资格（不能靠结构偷偷通过）。
- **消去**：在 `any Action` 上调用 protocol 方法，动态 dispatch 到符合类型的约定方法。**v1 不做 downcast**（没有 `match v { as GreetAction g => .. }`）；仅在真实程序需要时添加。Downcast 是存在类型设计滋生问题之处。
- **Witness 表示**：
  - *native（Rust emit）*：对程序内符合类型生成 enum（每次编译为封闭世界——Almide 链接完整程序，所以可采用 enum dispatch，单态且 `match` 穷尽），或者 `Box<dyn>` 风格的 fn pointer record。**优先 enum**：保留 Eq/Repr 可派生性，并完全避开 object-safety 规则。
  - *wasm*：`[type_tag: i32, value_ptr: i32]` 对 + 借助现有 closure function-table 机制实现每个 protocol 的 method table（先例：`call_indirect` dispatch、#385 的逐类型 `__repr_<T>` 函数）。
  - *interpreter*：带 tag 的 `Value::Any { type_name, inner }`。第三个 judge 必须实现相同 dispatch（需要 3-way fixture）。
- **与内置功能交互**（在设计评审决定，需要 contract-ledger 条目）：
  - `${any_value}` 插值 → 按 `Repr` 风格 dispatch 到内部类型的 literal repr（符合 C-008..C-010）。
  - `any P` 上的 `==` → 仅当 `P` 要求 `Eq`；否则带 hint 的 compile error。`Ord`/`Hash` 同样门控（因此 set/map-key 中的 `any P` 在不满足 `P: Hash + Eq` 时拒绝）。
- **错误质量**：把不符合的类型赋给 `any P` 时，同时指出缺失的符合关系与缺失方法（E017 诊断标准）。

<a id="staging"></a>

## 分阶段

1. **前置步骤（现在）**：Encode/Decode/Numeric 文档同步 + protocol 表 drift gate。
2. **Phase 1**：checker（`any P` 类型、coercion 规则、method-call typing、==/hash 的内置 protocol 门控）+ native enum-witness emit。从第一天就有 fixture。
3. **Phase 2**：wasm emit（tag+ptr 对、method table）。带 `// @contract:` header 的 cross-target fixture；dispatch 和 repr 语义的新 contract 条目。
4. **Phase 3**：interpreter 支持（3-way），让 fuzzer generator 生成 `any P` program（异构 list 形状），更新 CHEATSHEET + specs。

每个 phase 均在现有 gate 下合入（cross-target byte-gate、contract ledger、新 runtime routine 的 oracle registry、3-way harness）。

<a id="open-questions-design-review-before-phase-1"></a>

## 开放问题（Phase 1 前设计评审）

- native 上的 closed-world enum witness 对 open fn-pointer witness：enum 更简单且可派生，但每加入一个符合类型就要重编 witness；对 whole-program compilation 可以接受，若未来有 separate compilation 再考虑。
- `any P1 + P2`（交集存在类型）：延期吗？Go 没有也能运作，Swift 有 `any P & Q`。倾向延期但预留 syntax。
- 返回 `Self` 的 protocol 方法：无法在 `any` 后表示（经典 object-safety 问题）——在 coercion site 带 hint 拒绝，类似 Rust object safety，但信息更友好。

<a id="related"></a>

## 相关

- [llm-first-language.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/llm-first-language.md)——评判此设计的 MSR 视角
- [cross-target-completeness.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/cross-target-completeness.md)——每个 phase 必须通过的 gate
- `docs/specs/type-system.md`——本提案扩展的 protocol system 规格
