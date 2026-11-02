<a id="variant-exhaustiveness-refinement"></a>

# Variant 穷尽性细化

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](variant-exhaustiveness-refinement.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](variant-exhaustiveness-refinement.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](variant-exhaustiveness-refinement.zh-CN.md)

> [日英混合正本](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/done/variant-exhaustiveness-refinement.md)的非规范性读者快照，固定提交为 `35297a612fb29e3f78f6587aba253aa91d51f466`。原文仍是正本。本快照不自动同步，也不是生成器／检查器输入。下文日期、计划、完成状态、测量和比较主张均保留为历史原文记述，不是独立复现或对当前版本的验证。代码块及注释保持原样。历史收集元数据以不生效的读者注记显示。

> 历史描述：非穷尽 match 建议缺失分支代码；不可达分支成为硬错误。完成元数据：2026-04-20。

> 原文注意事项：完成节区分四项编译器功能与 §5 仅文档层面的关闭，编译器默认自动派生被推迟。保留旧设计、验收清单和“仍开放”节，不表示自动 Codec 派生已发布。guard 诊断、临时 hole 和示例保持历史范围，不是当前可运行指南或独立 MSR 结果。

<a id="completion-status-2026-04-20"></a>

## 完成状态（2026-04-20）

这一系列工作已在 4 个子节落地（§1–§4 为编译器功能，§5 为文档层面）：

- **§1 可直接粘贴的缺失分支**（2026-04-19）— `MissingArm` 同时携带紧凑见证模式（`Node(_, _)`）和可粘贴分支模板；E010 hint 输出模板块。
- **§2 不可达 → 错误**（2026-04-20）— 通过 `find_unreachable_arms` 执行 Maranget §3 usefulness 检查；每个死分支触发 E011。Opaque/Infinite 列类型走 default-matrix，防止泛型误报。
- **§3 嵌套穷尽性**（2026-04-20）— 检测原本已正确（Maranget 是递归的）；`fmt_arm_head` 改为递归，使嵌套见证输出为 `Node(Node(arg1, arg2), Leaf) => _`。
- **§4 guard 全覆盖说明**（2026-04-20）— 任一分支有 guard 时，E010 附加“带 guard 的分支不计入穷尽性 — guard 可能在运行时失败”，让用户明白 `X if cond => ...` 为何未覆盖 X。
- **§5 Variant Codec 自动派生**（2026-04-20）— 在文档层面关闭范围：惯例写入 `docs/CHEATSHEET.md`（“variant serialization — recommended pattern”）。编译器级默认自动派生被推迟，因为成本（stdlib + 用户代码中每个 variant 类型多 2 个 fn）与影响范围（现有 Codec decode 路径对非平凡 payload 仍有 `"payload decode not yet implemented"` stub）超过了 MSR 收益，而 LLM 已能从一个文档示例学会 `deriving Codec`。若 dojo MSR 数据显示 LLM 在显式 derive 形式上偏离，再重新划为独立系列工作。

<a id="motivation"></a>

## 动机

模式匹配穷尽性检查已能工作，但**错误信息止于“缺失分支名称”**，不足以供 LLM 自动修复。另外，不可达分支目前为警告级，但**不可达分支是 LLM 生成错误的直接信号**，应以错误返回。

本项旨在把错误信息打磨到“编写它的 LLM 直接复制粘贴即可通过”的程度，直接提高修改存活率。

<a id="current-state"></a>

## 当前状态

```
Non-exhaustive match at line 14:3
  Missing: Node
```

LLM 即便收到该信息，也可能不记得构造器的 arity 或字段名，导致再次失败。

<a id="design"></a>

## 设计

<a id="1-paste-ready-missing-arms"></a>

### 1. 可直接粘贴的缺失分支

以可以直接粘贴的形式提示缺失分支：

```
Non-exhaustive match at line 14:3
  Missing arms:
    Node(left, right) => _
    Empty              => _
  Hint: add the arms above, or use `_ => todo()` to compile incrementally.
```

- 列出每个构造器并**带字段名**
- 函数体为 `_` 占位符（compile-time hole）
- 同时给出 `_ => todo()` hint，供“先让它通过”的情况使用

<a id="2-unreachable-arms-become-errors"></a>

### 2. 不可达分支成为错误

```
match tree {
  Leaf(_) => ...,
  Leaf(0) => ...,  // ← 到達不能
  Node(_, _) => ...,
}
```

目前这是警告，将其改为错误。理由：

- LLM 不读警告（stdout 成为噪声）
- 不可达分支**很可能是生成错误**（误解了前一分支的条件）
- 如果是有意保留的“未来分支”，就应删除

<a id="3-nested-exhaustiveness"></a>

### 3. 嵌套穷尽性

对嵌套 Variant 的穷尽性：

```
match tree {
  Node(Leaf(_), _) => ...,
  // Missing: Node(Node(_,_), _)
}
```

如果目前仅查看嵌套外层判断穷尽性，则追踪到内层。

<a id="4-guard-aware-exhaustiveness"></a>

### 4. 感知 guard 的穷尽性

带 guard 的分支**从穷尽性计算中排除**。若有 `if x > 0` guard，就不能说该分支“一定匹配”。发出错误，要求带 guard 分支后必须有 `_ =>`：

```
Guard at line 17 is not total
  Hint: guarded arms never contribute to exhaustiveness. Add a `_ => ...` arm.
```

<a id="5-auto-derived-codec-for-variants"></a>

### 5. Variant 的自动派生 Codec

（相关）将已有 Codec 惯例（`Type.decode` / `Type.encode`）一致应用于 variant。手写 serialize 是 LLM 事故源，因此**默认自动派生**，必要时用 `#[codec(manual)]` opt-out。

<a id="acceptance-criteria"></a>

## 验收条件

- 非穷尽错误直接展示缺失分支代码片段
- 不可达分支成为错误
- 检测嵌套 Variant 的穷尽性
- 带 guard 分支后没有 `_ =>` 则报错
- Variant Codec 默认自动派生
- 更新已有 snapshot 测试，使诊断 golden 文件统一为新格式

<a id="progress"></a>

## 进展

<a id="2026-04-19--1-paste-ready-missing-arms"></a>

### 2026-04-19 — §1 可直接粘贴的缺失分支

`check_exhaustiveness` 现在返回 `Vec<MissingArm>`，同时携带紧凑见证模式（`Node(_, _, _)`）和可粘贴分支模板（`Node(arg1, arg2, arg3) => _`）。E010 诊断 hint 使用后者；元组 variant 获得按位置的 `argN` 绑定，record variant 复用声明字段名，`some/ok/err` 获得 `x` / `e`。测试在 `tests/exhaustiveness_hint_test.rs`（4 个案例：元组 variant、Option 缺失分支、unit variant、Int 的兜底分支）。

<a id="2026-04-20--2-unreachable--error"></a>

### 2026-04-20 — §2 不可达 → 错误

`find_unreachable_arms`（Maranget §3 usefulness）检测模式完全被先前分支覆盖的分支。每个死分支在其函数体 span 上触发 `E011 unreachable match arm`，并提供收紧 hint。带 guard 分支在两个遮蔽方向都跳过 — 它们不覆盖后续分支，也不会被认定为遭遮蔽。

Opaque / Infinite 列类型（TypeVar、Int、Float、String）走 default-matrix 分支，而非构造器枚举分支，以使 `map_option[A, B]` 之类泛型 fn 继续编译；ctor-enum 分支会对空 `[A]` 构造器集合把 `some(v)` 误报为死分支。

<a id="2026-04-20--4-guard-totality-note"></a>

### 2026-04-20 — §4 guard 全覆盖说明

match 中存在带 guard 分支时，E010 现在附加“带 guard 分支（`pat if cond =>`）不计入穷尽性 — guard 可能在运行时失败”。该消息澄清 LLM 的 `X if cond => ...` 分支为何未覆盖模式 X。

<a id="2026-04-20--3-nested-exhaustiveness-arm-template"></a>

### 2026-04-20 — §3 嵌套穷尽性（分支模板）

嵌套未覆盖模式的检测原本已工作 — Maranget usefulness 算法对子模式递归。缺口在 `fmt_arm_template` formatter：它使用 `field_names`，不论内部构造器结构如何，总是生成位置性 `argN` 名称。现在 `fmt_arm_head` 对各参数递归：

- `Pat::Wild` → 绑定名（来自文件级计数器的 `argN`，或 record 字段名／Option/Result 单字段的 `x`）。
- `Pat::Ctor(c, args)` → 经递归调用形成嵌套 `c(...)`。

对于 `type Tree = | Leaf | Node(Tree, Tree)`，只处理 `Leaf` 与 `Node(Leaf, _) => ...` 时的结果：

```
hint: add arms for Node(Node(_, _), Leaf), Node(Node(Leaf, Leaf), Node(_, _)), Node(Node(Node(_, _), Leaf), Node(_, _)):
  Node(Node(arg1, arg2), Leaf) => _
  Node(Node(Leaf, Leaf), Node(arg1, arg2)) => _
  Node(Node(Node(arg1, arg2), Leaf), Node(arg3, arg4)) => _
```

`argN` 计数器按每个分支模板重置，避免粘贴后绑定意外相同；这是有意为之，因为各分支相互独立。

<a id="still-open"></a>

### 仍开放

- **§5 Variant Codec 自动派生** — 扩展现有 `derive Codec` 机制，默认为 variant 生成 encode / decode。需修改自动派生 codegen 管线，是独立系列规模的工作。

<a id="dependencies"></a>

## 依赖

- 无（类型检查器与诊断的局部变更即可完成）

<a id="risk"></a>

## 风险

- 不可达从警告变为错误可能破坏现有代码。考虑经独立标志（`--strict-unreachable`）分阶段迁移。
