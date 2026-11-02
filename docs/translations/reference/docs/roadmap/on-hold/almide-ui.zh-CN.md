<!-- description: SolidJS-like reactive UI framework built as a pure Almide library -->
<a id="almide-ui--reactive-web-framework-as-almide-library"></a>

# Almide UI — 作为 Almide 库的响应式 Web 框架

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](almide-ui.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](almide-ui.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](almide-ui.zh-CN.md)

> 非规范性历史阅读快照。以固定原文为准，不自动同步。未独立重跑测量、证明 gate 或当前实现检查。未执行引用命令。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/almide-ui.md)
> 保留中的历史提案，不是可运行库。保留原代码、退役TS草图及比较图作为示例，不视为当前可运行语法或新benchmark结果。10–15声明覆盖全部DOM、V8最高速度及高度信心等仍是原文设计主张，同时保留未解决的host/ownership/优化前提。此处不确立实测优势或零开销保证。

<a id="thesis"></a>

## 主张

提案：用 Almide 构建响应式 UI 库，尽可能让框架专有知识留在编译器之外。最初设计依赖的 TS 输出已经退役。选择受支持的 browser host／DOM bridge 成为前提；尚未证明 Svelte/SolidJS 级性能或无需改动编译器的可行性。下方代码保留原设计草图，不是当前可运行的 Almide 框架。

```
What the user writes: Almide builder syntax + signal library
Original backend assumption (retired): optimize Almide code → TS output
Desired result: no virtual DOM, fine-grained updates; current host mapping unresolved
```

**设计目标：把框架知识留在库中。**在声称无需编译器工作之前，必须确立 host 互操作及优化覆盖范围。

<a id="why-this-is-the-right-approach"></a>

## 为什么这个方案合适

<a id="comparison-with-other-approaches"></a>

### 与其他方式比较

| 方式 | 示例 | 问题 |
|---|---|---|
| 框架内置编译器 | Svelte | 编译器复杂化；框架演化＝重做编译器 |
| JS runtime 库 | React, Vue | virtual DOM diff 的 runtime 成本；bundle 大小 |
| JS 库＋细粒度 | SolidJS | 最快级别；但缺少 JS 类型信息限制优化上限 |
| **Almide 库** | **本提案** | SolidJS 模型＋利用 Almide 类型信息的额外优化 |

<a id="benefits-of-being-an-almide-library"></a>

### 作为 Almide 库的好处

1. **尽量少改编译器** — 判断是否库实现就足够之前，先评估既有语言／host 支持
2. **框架用 Almide 编写** — 与用户代码同语言、同优化目标
3. **类型信息深入框架内部** — `Signal[Int]` 的 `.get()` 确定返回 Int；类型特化一直到最底层
4. **框架演化独立于编译器** — 升级库版本不要求发布编译器
5. **多目标收益** — 框架测试可通过 `--target rust` 快速运行

<a id="architecture"></a>

## 架构

<a id="layer-structure"></a>

### 分层结构

```
almide-ui/                    ← Almide package (library)
├── reactive.almd             Signal, Effect, Derived
├── dom.almd                  DOM primitives (bridged via @extern)
├── component.almd            Component lifecycle
├── renderer.almd             Signal→DOM update connection
└── html.almd                 builder Html + element functions (div, p, ...)
```

<a id="reactive-core"></a>

### 响应式核心

```almide
// reactive.almd

type Signal[T] = {
  var value: T,
  var subs: List[fn() -> Unit],
}

fn signal[T](initial: T) -> Signal[T] =
  { value: initial, subs: [] }

fn get[T](s: Signal[T]) -> T = s.value

fn set[T](s: Signal[T], new_val: T) -> Unit = {
  s.value = new_val
  for sub in s.subs { sub() }
}

fn subscribe[T](s: Signal[T], f: fn() -> Unit) -> Unit =
  s.subs = s.subs ++ [f]

fn derived[T](compute: fn() -> T) -> Signal[T] = {
  // Automatically tracks dependent signals and returns computed value
  // Equivalent to SolidJS's createMemo
  ...
}
```

这是提议的表示方式。一行函数体本身不能保证 inlining：当前 inliner 有 scalar、scope 和 ownership 准入规则。声称零开销前，应验证表示及生成代码。

<a id="dom-bridge"></a>

### DOM bridge

```almide
// dom.almd — minimal @extern bridge

@extern(ts, "document.createElement(tag)")
fn create_element(tag: String) -> DomNode

@extern(ts, "node.textContent = text")
fn set_text(node: DomNode, text: String) -> Unit

@extern(ts, "parent.appendChild(child)")
fn append(parent: DomNode, child: DomNode) -> Unit

@extern(ts, "node.addEventListener(event, handler)")
fn on(node: DomNode, event: String, handler: fn() -> Unit) -> Unit

@extern(ts, "node.setAttribute(name, value)")
fn set_attr(node: DomNode, name: String, value: String) -> Unit
```

用 10–15 个 `@extern` 声明可完全覆盖 DOM API。框架的工作是组合它们，提供声明式 API。

<a id="renderer--connecting-signals-to-the-dom"></a>

### Renderer — 连接 Signal 与 DOM

```almide
// renderer.almd

fn reactive_text(parent: DomNode, s: Signal[String]) -> Unit = {
  let node = create_element("span")
  set_text(node, s.get())
  append(parent, node)
  // When signal changes, only update textContent
  s.subscribe(fn() => set_text(node, s.get()))
}

fn reactive_attr(node: DomNode, name: String, s: Signal[String]) -> Unit = {
  set_attr(node, name, s.get())
  s.subscribe(fn() => set_attr(node, name, s.get()))
}

fn reactive_list[T](
  parent: DomNode,
  items: Signal[List[T]],
  render: fn(T) -> DomNode,
) -> Unit = {
  // List diff update (keyed reconciliation)
  ...
}
```

<a id="user-code"></a>

### 用户代码

```almide
import almide_ui exposing (signal, get, set, derived)
import almide_ui/html exposing (div, h1, p, button, ul, li)

var count = signal(0)
let doubled = derived(fn() => count.get() * 2)

template app() -> HtmlDoc = Html {
  div(class: "app") {
    h1 { "Counter" }
    p { "Count: ${count.get()}" }
    p { "Doubled: ${doubled.get()}" }
    button(onclick: fn() => count.set(count.get() + 1)) {
      "Increment"
    }
  }
}
```

用户只需知道 Signal 和 builder 语法，DOM 操作不可见。

<a id="compiler-optimization--what-happens-automatically"></a>

## 编译器优化 — 自动发生什么

以下是期望的优化示例，不是对该未实现 Signal API 保证执行的转换：

<a id="1-inlining"></a>

### 1. Inlining

```almide
// Definition of signal.get()
fn get[T](s: Signal[T]) -> T = s.value

// User code
p { "Count: ${count.get()}" }

// After inlining (compiler internal)
p { "Count: ${count.value}" }

// Original illustrative TS output (retired target)
p.textContent = `Count: ${count.value}`;
```

提议的 `.get()` / `.set()` 优化需要生成代码与 runtime 测量；仅凭函数大小不能确立 Signal 零开销。

<a id="2-type-specialization-monomorphization"></a>

### 2. 类型特化（单态化）

```almide
// Generic definition
fn get[T](s: Signal[T]) -> T = s.value

// Used with Signal[Int] → Int-specialized version is generated
// Used with Signal[String] → String-specialized version is generated
```

V8 以最高速度处理单态 call site。类型特化使全部 Signal 操作单态化。

<a id="3-pipeline-fusion"></a>

### 3. Pipeline fusion

```almide
let visible_names = items.get()
  |> list.filter((x) => x.active)
  |> list.map((x) => x.name)
```

融合为没有中间数组的单个循环，响应式 list 更新更快。

<a id="4-dead-code-elimination"></a>

### 4. 死代码消除

从输出移除未使用的 Signal method 和 DOM helper，尽量缩小 bundle。

<a id="future-option-compiler-aware-signal-tracking"></a>

## 未来选项：编译器感知 Signal 跟踪

识别 Signal 的 pass 是一个可选研究方向。纯库实现的性能等价性，以及该 pass 是否必要，均未确立：

```almide
// Compiler tracks Signal.get() calls inside builder blocks
Html {
  p { "Count: ${count.get()}" }     // ← depends on count
  p { "Name: ${name.get()}" }      // ← depends on name
  p { "Static text" }               // ← no dependencies
}
```

编译器看到依赖并自动生成更新函数：

```typescript
// create
const p0 = document.createElement('p');
p0.textContent = `Count: ${count.value}`;
const p1 = document.createElement('p');
p1.textContent = `Name: ${name.value}`;
const p2 = document.createElement('p');
p2.textContent = 'Static text';

// update — when count changes, only p0 updates
count.subscribe(() => { p0.textContent = `Count: ${count.value}`; });
// update — when name changes, only p1 updates
name.subscribe(() => { p1.textContent = `Name: ${name.value}`; });
// p2 has no subscription (static, no update needed)
```

runtime 依赖跟踪实现是另一候选。在它与编译器辅助方案之间选择前，需要测量正确性、host 集成及成本。

<a id="competitive-position"></a>

## 竞争定位

```
                    Compile-time analysis  Runtime cost       Ecosystem
Svelte              ◎ (compiler)           ◎ (minimal)        ○ (JS)
SolidJS             △ (JSX transform)      ◎ (signal)         ○ (JS)
React               × (runtime diff)       △ (vDOM)           ◎ (largest)
Leptos (Rust→WASM)  ○ (signal)             ○ (WASM overhead)  △ (WASM FFI)
Almide UI proposal  unmeasured           unmeasured         host mapping unresolved
```

Almide UI 的差异点：
- **客户端输出** — TypeScript backend 退役后仍未解决；原 plain-JS 性能／兼容性假设不是已交付能力或测量结果
- **共享 SSR／客户端代码** — 是目标，不是当前 TS 输出；仍需设计受支持的 client host 及边界
- **类型驱动优化** — 是提议的机会，不是已确立的相对 SolidJS 优势
- **库边界** — 是设计目标；仍需识别必要的编译器集成

<a id="relationship-to-other-roadmap-items"></a>

## 与其他 roadmap 项目的关系

- **原 TS emitter roadmap**：仅为历史输入，不是可用输出目标
- **Result Builder（template.md）**：`builder Html` 正是此框架的 UI 描述基础；builder 机制完成后，Almide UI 的 DOM 构建自然可行
- **跨目标语义**：任何 SSR／client adapter 都需要显式兼容性测试，不能沿用旧 TS 计划中的假设
- **Wasm／browser host**：可能的替代方向，不是已选定或实现的 UI backend

<a id="prerequisites"></a>

## 前提

1. **builder 机制（template.md Phase 1）** — 没有 `builder Html` 和 trailing block，便无法写 UI 描述
2. **受支持的 client backend** — 选择并验证 browser host／DOM 边界；旧 TS emitter 前提已退役
3. **generics 稳定** — `Signal[T]` 需要正确的类型检查与 codegen
4. **closure／ownership 语义** — 在选定 host 上验证订阅寿命和 callback

<a id="why-on-hold"></a>

## 为什么 ON HOLD

client backend、DOM 边界、builder 语法及订阅 ownership 仍是设计依赖。保留最初 library-first 理由，但它没有解决这些问题：

- **仍有设计工作** — Signal 草图、extern 声明与 builder 提案，不能确立可运行的 DOM 集成
- **编译器范围未决** — 选择框架专有工作之前，测量当前 generic 优化支持什么
- **可逐步构建** — 响应式核心 → DOM bridge → renderer → builder 集成，每步独立
- **先例动机** — library-first 细粒度响应性值得评估，但别处存在不等于本提案的 benchmark

信心很高。前提语言功能就绪后，框架自身不过是 Almide 代码。

### 保留代码与图示的说明译文

代码及注释保持原文字节。开头图表示：用户编写Almide builder语法＋signal库；原backend假设是优化Almide代码→TS输出（已退役）；期望无virtual DOM、细粒度更新，当前host映射未决。分层图设想reactive.almd负责Signal/Effect/Derived，dom.almd经extern桥接DOM primitive，component.almd处理component生命周期，renderer.almd连接Signal→DOM更新，html.almd提供builder Html与元素函数。derived注释说明自动跟踪依赖Signal并返回计算值，相当于SolidJS createMemo。renderer注释表示Signal变化时只更新textContent，list使用keyed reconciliation。

inlining示例注释依次为get定义、用户代码、编译器内部inline后、退役TS输出示例。单态化示例先定义generic，再设想Signal[Int]与Signal[String]使用分别生成Int/String特化版。跟踪示例设想编译器识别builder内get调用，将其分为依赖count、依赖name及无依赖。生成TS示例创建p0/p1/p2；count变化仅更新p0，name变化仅更新p1，静态p2无订阅、不需更新。

竞争图列为编译期分析、runtime成本、ecosystem。保留原评分符号及原文比较：Svelte为compiler／最小／JS，SolidJS为JSX转换／signal／JS，React为runtime差分／vDOM／最大生态，Leptos为signal／WASM overhead／WASM FFI。Almide UI的分析与运行成本均未测量，host映射未解决。这不是独立的当前比较。
