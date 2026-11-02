> 最后更新：2026-10-31

<a id="codegen-specification"></a>

# 代码生成规范

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](codegen.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](codegen.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](codegen.zh-CN.md)

> 本文是[固定原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/codegen.md)的非规范读者译文；原文仍是权威版本，本译文不会自动同步。




> 非规范性读者译本。固定于原文提交 `35297a612fb29e3f78f6587aba253aa91d51f466`，不自动同步。以原文为准，代码、类型及注释保持原样。实现、性能、所有权说明及对其他语言的描述均为原文主张，不代表翻译时进行了独立验证或执行。原文开头的“三层”与图中 Layer 1/2 的差异保留，不擅自统一。

Almide 的代码生成（`crates/almide-codegen`）采用三层架构，将有类型的 IR 转换为 **Rust** 目标（及 WGSL）的源码输出。所有语义决策都在输出任何文本前于 IR 中完成。

**WASM** 目标不经过此 crate。`src/wasm_route.rs` 中的共享路径使用 `crates/almide-wasm` 的结构化发射器直接生成 Wasm 字节；已退役的 WAT 渲染器不是后备路径。参见 [../ARCHITECTURE.md](../../../../ARCHITECTURE.md) 和 [../wasm/WASM-OUTPUT.md](../../../../wasm/WASM-OUTPUT.md)。

**源码**： `crates/almide-codegen/src/lib.rs`

```
IrProgram (typed IR)
    |
Layer 1: Nanopass Pipeline (target-specific semantic rewrites)
    |
Layer 2: Emit (target-specific output)
    Rust  -> Template Renderer (TOML-driven) -> source code
    WGSL  -> emit_wgsl                       -> shader source
```

<a id="1-entry-point"></a>

## 1. 入口

**源码**： `crates/almide-codegen/src/lib.rs`

唯一入口为 `codegen(program, target) -> CodegenOutput`（`codegen_with` 接受 `CodegenOptions`）。

```rust
pub enum CodegenOutput {
    Source(String),   // Rust / WGSL targets
    Binary(Vec<u8>),  // legacy variant; no longer produced
}
```

处理流程：

1. `target::configure(target)` 构建包含 nanopass 流水线和模板集的 `TargetConfig`
2. 流水线运行全部适用的 pass，原地转换 IR
3. 目标特定输出：
   - **Rust**：walker 通过模板渲染 IR，并在前面添加运行时前导内容
   - **WGSL**：`emit_wgsl::emit()` 渲染着色器源码
   - **Wasm**：`unreachable!`——到达此 crate 之前已路由到结构化 `almide-wasm` 发射器

```almd
// Input: hello.almd
fn greet(name: String) -> String =
  "Hello, " + name + "!"

fn main() =
  println(greet("world"))
```

```rust
// Output: Rust target (simplified, preamble omitted)
pub fn greet(name: String) -> String {
    format!("{}{}", format!("{}{}", "Hello, ".to_string(), name), "!".to_string())
}

pub fn main() -> () {
    println!("{}", greet("world".to_string()))
}
```

<a id="2-target-enum"></a>

## 2. Target 枚举

**源码**： `crates/almide-codegen/src/pass.rs`

```rust
pub enum Target {
    Rust,
    Wasm,   // tombstone: codegen arm is unreachable! (v0 emitter retired, #782)
    Wgsl,
}
```

此 crate 中的有效目标是 **Rust** 和 **WGSL**。TypeScript 代码生成已于 2026-03-28 移除。当前 Wasm 路径为 `src/wasm_route.rs` → `crates/almide-wasm`，不是已退役的 MIR-to-WAT 渲染器。

每个目标通过返回 `TargetConfig` 的 `target::configure()` 配置：

**源码**： `crates/almide-codegen/src/target.rs`

```rust
pub struct TargetConfig {
    pub target: Target,
    pub pipeline: Pipeline,
    pub templates: TemplateSet,
}
```

<a id="3-nanopass-pipeline"></a>

## 3. Nanopass 流水线

**源码**： `crates/almide-codegen/src/pass.rs`, `crates/almide-codegen/src/target.rs`

每个 pass 实现 `NanoPass` trait：

```rust
pub trait NanoPass: std::fmt::Debug {
    fn name(&self) -> &str;
    fn targets(&self) -> Option<Vec<Target>>;  // None = all targets
    fn depends_on(&self) -> Vec<&'static str>;
    fn run(&self, program: IrProgram, target: Target) -> PassResult;
}
```

各 pass 组合成 `Pipeline`。流水线运行器：
- 跳过与当前目标无关的 pass
- 验证声明的依赖（依赖尚未执行时 panic）
- 在**每种配置的每个 pass 后**验证 IR 完整性以及已建立的每个 `Postcondition`。违反条件属于编译器缺陷，会使构建失败，包括发布构建。逐 pass 遍历过去仅用于调试模式（`ALMIDE_VERIFY_IR` 可让发布构建启用），依据是每文件约 1.2 秒的过时成本说法；在 spec/lang 的 218 个文件上测得每文件约 8 ms，因此现在交付的配置运行与 `cargo test` 相同的遍历。`ALMIDE_IR_FAULT=<pass>`（测试框架开关）在指定 pass 后注入违反条件的状态，是发布二进制的阴性对照（`tests/ir_verify_every_profile_test.rs`）。

<a id="rust-pipeline-source-of-truth"></a>

### Rust 流水线的权威来源

[`build_pipeline(Target::Rust)`](../../../../../crates/almide-codegen/src/target.rs) 是 pass 顺序的权威来源，包含依赖和条件门禁。这是实现流水线，不是固定的语言级顺序。

其阶段包括类型／调用规范化、区域与所有权分析、借用／克隆、effect 与 Result 适配、流融合、Rust 特有 lowering、运行时调用规范化，以及最终链接／存储决策。`RegionWindowPass`、`StreamFusionPass` 和 `OwnedSourceHoistPass` 是当前实现的一部分；旧 `AutoParallelPass` 表已过时。并行调度遵循显式 `fan` 形式，而非已移除的 pass。

<a id="4-template-system"></a>

## 4. 模板系统

**源码**： `crates/almide-codegen/src/template.rs`, `crates/almide-codegen/templates/rust.toml`

模板只定义语法。所有语义决策都由 nanopass 的各 pass 完成。

<a id="structure"></a>

### 结构

```rust
pub struct TemplateRule {
    pub template: String,           // "{placeholder}" holes
    pub when_type: Option<String>,  // Guard: match on expression type
    pub when_attr: Option<String>,  // Guard: match on TargetAttrs flag
}

pub struct TemplateEntry {
    pub rules: Vec<TemplateRule>,   // First matching rule wins
}

pub struct TemplateSet {
    pub target_name: String,
    pub entries: HashMap<String, TemplateEntry>,
}
```

<a id="toml-format"></a>

### TOML 格式

每种构造一个规则：

```toml
[if_expr]
template = "if {cond} {{ {then} }} else {{ {else} }}"
```

带守卫的多条规则（数组语法）：

```toml
[[concat_expr]]
when_type = "String"
template = "format!(\"{{}}{{}}\", {left}, {right})"

[[concat_expr]]
when_type = "List"
template = "AlmideConcat::concat({left}, {right})"
```

以属性为守卫的变体：

```toml
[[call_expr]]
when_attr = "needs_try"
template = "{callee}({args})?"

[[call_expr]]
template = "{callee}({args})"
```

<a id="guard-system"></a>

### 守卫系统

**`when_type`**：匹配 IR 表达式的类型类别（`"Int"`、`"Float"`、`"String"`、`"List"`、`"Option"`）。用于 `+`（拼接或加法）、`**`（pow 或 powf）及 unwrap_or 等按类型分派的操作。

**`when_attr`**：匹配 nanopass 设置的 `TargetAttrs` 标志。用途：

| 标志 | 设置方 | 效果 |
|------|--------|--------|
| `needs_try` | ResultPropagationPass | 在调用后追加 `?` |
| `needs_clone` | CloneInsertionPass | 输出 `.clone()` |
| `needs_borrow` | BorrowInsertionPass | 在参数类型前添加 `&` |
| `none_type_hint` | TypeConcretizationPass | 输出显式类型的 `None::<T>` |
| `repr_c` | CodegenOptions | 给 struct/enum 添加 `#[repr(C)]` |

<a id="rule-priority"></a>

### 规则优先级

先检查带守卫的规则，再检查无守卫的默认规则。首个匹配规则生效。TOML 加载器在每种构造内将带守卫规则排在默认规则之前。

<a id="template-placeholders"></a>

### 模板占位符

`{name}` 被对应绑定替换。`{{` 和 `}}` 是字面花括号的转义序列。未知占位符保持原样。

<a id="key-template-constructs-rust"></a>

### 主要模板构造（Rust）

Rust 模板集（`crates/almide-codegen/templates/rust.toml`）定义以下构造：

| 类别 | 示例 |
|----------|----------|
| 表达式 | `if_expr`, `call_expr`, `binary_op`, `field_access`, `index_access`, `pipe_call` |
| Option/Result | `some_expr`, `none_expr`, `ok_expr`, `err_expr`, `unwrap_expr`, `unwrap_or_expr`, `try_expr`, `to_option_expr` |
| 判等 | `eq_expr` (`almide_eq!`), `ne_expr` (`almide_ne!`) |
| 拼接 | `concat_expr` (按类型分派：String 或 List) |
| 幂 | `power_expr` (按类型分派：`.pow()` 或 `.powf()`) |
| 字面量 | `int_literal` (`{value}i64`), `float_literal` (`{value}f64`), `string_literal` (`"{value}".to_string()`), `list_literal` (`vec![{elements}]`) |
| 语句 | `let_binding`, `var_binding`, `assignment` |
| 声明 | `fn_decl`, `effect_fn_decl`, `test_block`, `struct_decl`, `enum_decl` |
| 类型 | `type_int` (`i64`), `type_string` (`String`), `type_list` (`Vec<{inner}>`), `type_map` (`HashMap<{key}, {value}>`) |
| 循环 | `for_loop`, `while_loop`, `break_stmt`, `continue_stmt` |
| Lambda | `lambda` (`move \|{params}\| {{ {body} }}`), `lambda_single` |
| 匹配 | `match_expr`, `match_arm`, `pattern_some`, `pattern_ok`, `pattern_variant`, 等等 |
| 顶层 | `top_let_const`, `top_let_lazy` (`LazyLock`), `top_let_thread_lazy` (保存闭包值的逐线程槽) |
| 模块调用 | `module_call` (`almide_rt_{module}_{func}({args})`) |

<a id="5-walker"></a>

## 5. Walker

**源码**： `crates/almide-codegen/src/walker/mod.rs`

walker 遍历有类型的 IR 并使用模板渲染。它完全不依赖目标，没有任何 `if target == Rust` 检查。目标差异全部由 pass 和模板处理。

<a id="rendercontext"></a>

### RenderContext

```rust
pub struct RenderContext<'a> {
    pub templates: &'a TemplateSet,
    pub var_table: &'a VarTable,
    pub indent: usize,
    pub target: Target,
    pub auto_unwrap: bool,          // true inside effect fn (not test)
    pub ann: CodegenAnnotations,
    pub type_aliases: HashMap<Sym, Ty>,
    pub minimal_generic_bounds: bool, // Clone only for bundled .almd modules
    pub repr_c: bool,               // Emit #[repr(C)] on structs/enums
}
```

<a id="rendering-functions"></a>

### 渲染函数

分布在以下子模块中：

| 模块 | 函数 | 渲染内容 |
|--------|----------|---------|
| `walker/expressions.rs` | `render_expr()` | 表达式（递归渲染子表达式） |
| `walker/statements.rs` | `render_stmt()`, `render_pattern()` | 语句、match 模式 |
| `walker/types.rs` | `render_type()` | 类型注解（具名记录、泛型、元组） |
| `walker/declarations.rs` | `render_type_decl()` | struct、enum、别名声明 |
| `walker/mod.rs` | `render_function()`, `render_program()` | 函数声明、完整程序组装 |
| `walker/helpers.rs` | `terminate_stmt()`, `ty_contains_name()` | 实用工具 |

<a id="program-rendering-order"></a>

### 程序渲染顺序

`render_program()` 按以下顺序组装输出：

1. 匿名记录的 struct 定义
2. 类型声明（struct、enum、别名）
3. 顶层 let（const 和 lazy）
4. 非测试函数
5. 测试函数（包装在 `mod tests { use super::*; ... }` 中）
6. 导入模块的函数（带 `almide_rt_{module}_{func}` 前缀）

<a id="function-name-sanitization"></a>

### 函数名清理

为兼容目标，函数名会进行清理：空格、点和连字符变为下划线。特殊字符（`+`、`/`、`*`、`(`、`)` 等）替换为描述性名称（`_plus_`、`_div_`、`_mul_` 等）。目标特有关键字通过 `keyword_escape` 模板转义（Rust：`r#name`）。

测试函数命名为 `__test_almd_<NNNN>_<label>`（`almide_ir::test_fn_name`）：前缀将其与实际函数区分，声明顺序编号将标签清理后同名的两个测试区分开（`"a b"` / `"a_b"`，#3488）。

<a id="6-rust-target-specifics"></a>

## 6. Rust 目标细节

**源码**： `crates/almide-codegen/src/lib.rs`, `emit_source()`

<a id="preamble"></a>

### 前导内容

每份 Rust 输出都以下列内容开头：

1. `#![allow(...)]`——抑制未使用警告
2. `use std::collections::{HashMap, HashSet};`
3. `AlmideConcat` trait——重载 `+` 以拼接 String 和 Vec
4. `almide_eq!` / `almide_ne!` 宏——深度相等比较
5. 运行时模块（仅用户代码引用的模块）

<a id="runtime-inclusion"></a>

### 运行时引入

运行时模块通过 `include_str!`（由 `build.rs` 生成）嵌入编译器二进制。输出时，发射器在用户代码中扫描 `almide_rt_{module}_` 模式，只引入被引用的模块。为避免冲突，会从运行时源码中去除 `mod tests { ... }` 块。

会解析运行时模块间依赖（例如 `json` 依赖 `value`）。

```almd
// Using list.map triggers inclusion of the list runtime module
import list

fn main() =
  let xs = [1, 2, 3]
  let doubled = xs.map((x) => x * 2)
  println(doubled)
```

```rust
// Output includes list runtime functions (almide_rt_list_*)
// but NOT string, map, json, etc.
```

<a id="parameter-passing-on-the-native-target"></a>

### 原生目标的参数传递

**源码**： `pass_borrow_inference_ownership.rs` (`param_borrow`), `pass_borrow_lowering.rs`.

可借用类型（`String`、`List[T]`、记录）的非 `mut` 参数以**引用方式传递**（`&str` / `&[T]` / `&T`），除非函数体需要拥有该值。以下情况需要拥有它：

- 返回参数、拼接参数、将其构造成记录／列表／元组字面量的一部分、对其进行 match，或用其初始化 fold。
- 在闭包中捕获它（闭包通过无法持有借用的 `Rc<dyn Fn>` 捕获，因此捕获是移动，若参数还会再次读取则是克隆）。
- 将其传给会消费值的 **stdlib** 参数槽（`list.push` 的元素）；这些会 lowering 成接收该值的运行时调用。
- 用需要拥有**元素**的函数体遍历它（见下文）：`for` 循环，或被融合 pass 内联的列表组合器。

**迭代源的模式取决于元素，而不是组合器的参数槽**（#2287）。`list.map`、`fold`、`filter`、`any` 等将列表槽声明为 `@consume(xs)`，但这描述的是未融合的运行时对应实现；融合链及 `for` 循环根据函数体如何使用每个元素决定源的模式（`element_reads_only`，由借用判定、克隆 pass 和所有权认证器共同读取的一条规则）：

- `Copy` 元素（`List[Int]`）从不需要拥有源：`mapped(ns:
  List[Int]) = ns |> list.map((n) => n * 2)` 变为基于 `ns.iter().cloned()` 的 `mapped(ns: &[i64])`，调用方即使读取列表两次也不克隆任何内容。
- 对于所有接收它的 lambda 都只读取的堆元素——借用调用槽、字段读取、克隆、`&str` 比较、插值部分，或因同一迭代中再次读取绑定而无论如何都会由克隆 pass 克隆的消费用途——源保持借用，并从 `xs.iter()` 绑定 `&T`：`lens(ws: &[String]) = ws.iter().map(|w| len(w.as_str()))`，不克隆元素（Rust 向 `filter` 系步骤传入 `&&T`，因此用 `let w = *w` 重新绑定，而不是 `let w = w.clone()`）。
- 以拥有形式离开链的元素——作为映射值返回、拼接、构造成值、传入拥有槽、由 `filter` / `find` 收集——使源保持消费模式：基于 `ws.into_iter()` 的 `shouted(ws: Vec<String>)`。调用方最后一次使用会移动列表，每个元素可无额外成本移出，而借用源只能克隆它。

这是引用计数编译器采用的逐元素规则（读取列表会借用列表内部；仅在某个使用处需要时才保留元素），将其提升到源的模式，因为原生路径上的借用源只能克隆。

流融合 pass 内联的列表组合器回调（`list.map(xs, (x) =>
… t …)`、`filter`、`fold`、`any` 等）在此策略下**不是闭包**：该 pass 在借用 pass 之前运行，链步骤是一个作用域，在链内对每个元素运行一次且不会逃逸。仅读取的参数保持 `&T`——`(x) => x + list.len(t.names)` 保持 `t: &Table`——步骤渲染时没有 `move`，也没有捕获绑定，只在链持续期间借用其读取内容。步骤内发生消费的读取（为每个元素返回 `t`）无论参数是否拥有都会在那里克隆，因此不会让拥有该参数获得收益。只有比其调用活得更久的闭包——被返回、存储、由另一闭包捕获，或传入被调用方会保留的槽——才会捕获，而捕获持有所有权（Lean、Koka 和 Roc 做出相同区分：实际存在的闭包拥有其持有内容；收益来自在决定所有权前消除闭包）。

**函数类型参数在其可调用值不逃逸时被借用**（#2288）。用户高阶函数的 `f: (A) -> B` 参数，如果每处使用都仅调用、借用，或传给另一函数的借用函数槽，就为 `&dyn Fn(A) -> B`（与决定 `&T` 的不动点相同，对尚待决定或自递归的被调用方采用乐观判定）；若某处让可调用值活得比调用更久——返回、绑定到局部变量、构造进记录／列表、由闭包捕获、传入拥有槽（如 `list.map` 未融合形式的运行时对应实现、被存储的链回调）——则使用 `Rc<dyn Fn>` 句柄。借用槽中的 lambda 字面量因此成为类似链步骤的作用域：`apply(xs, &|x| x * k)`——没有 `Rc::new`、没有 `move`、没有捕获绑定，其读取的参数保持借用；该槽中的闭包**值**通过句柄借出（`&*g`）；尾递归函数像传递 `&Vec<u8>` 一样在循环中携带借用的可调用值。这就是无闭包参考实现作为普通参数传递的借用捕获记录，无需 lambda sets：函数类型参数本身就是 ABI，逃逸判定决定函数体需要两种形式中的哪一种。所有权认证器的 C5 在最终 IR 上重新读取该判定（逃逸的 `&dyn Fn` 参数；借用槽中的装箱闭包），`ALMIDE_FN_ESCAPE_OFF=1` 是其阴性对照。

具有相同生命周期的 `let` 绑定闭包也是作用域（#3455）。当绑定的每处使用都被规则允许（调用、借用、借给 `&dyn Fn` 槽——绝不返回、存储、被寿命更长的闭包捕获或用于 `fan` 分支），且它捕获的每个变量都从不被写入、没有 `var`，对堆值而言在函数内任何位置也从不被消费时，绑定就是非 `move` 的 `let f = &|i: i64| …`：读取是借用，所读参数保持 `&T`，调用方无需为调用而克隆实参。判定是每轮借用中的最大不动点，确定后表示为 `Borrow { Lambda }`，所以 `CaptureClone` 不添加 `__cap_*` 绑定；TCO 目标函数被排除。`ALMIDE_SCOPED_CLOSURE_OFF=1` 恢复拥有式捕获。

将参数直接交给被调用方的拥有槽（`stored(t, 1)`，其中 `stored` 将 `t` 保存在记录中），只有该位置是参数的**最后**一次使用时，才会使参数变为拥有。此时拥有可换来移动，仍需要该值的调用方克隆一次。若参数在该位置之后还被读取（`{ let h
= stored(t, 1); h.n + plain(t) }`），无论参数是否拥有，该位置都要克隆——所以参数保持 `&T`，仅该位置拥有它读取的值（`stored(t.clone(), 1)`）：该处一次克隆，所有调用方均无需克隆。分配台账（`tests/native_borrow_oracle_test.rs`）固定记录另一方案的成本：不论活跃性而在所有这类位置克隆（#2278 初稿），会增加全部五个固定程序的次数。`mut` 参数在整个函数体中为 `&mut T`，在每个按值位置采用同样的局部克隆（#2266）。

**变体**参数（`s: Shape`）遵循相同策略，另有一条 `match` 规则：当各分支引入的每个绑定都只被读取——`Copy` 标量，或每处使用都是非消费位置的堆载荷（借用实参、成员读取、比较、插值部分、进一步 match 的对象）——匹配对象位置按引用读取参数。分支随后绑定 `&T` 载荷（Rust 默认绑定模式：标量读取会解引用，堆载荷的 `&b` 就是裸绑定，装箱载荷再次以 `&**b` 匹配），调用方无需为传值而克隆。只要有一个载荷被返回、拼接、构造成值、捕获或交给拥有槽，参数就保持拥有：按值匹配可无额外成本移出载荷，而借用匹配需要克隆它。所有权认证器的 C4 读取同一谓词，因此匹配仅读取却仍拥有的变体参数会成为具名缺陷（`tests/variant_param_borrowed_test.rs`）。

拥有参数使每个仍需该值的调用方付出一次克隆成本；借用参数在调用处没有此成本。函数最终使用 `&T` 还是 `T`，可在输出的 Rust（`almide app.almd --target rust`）中查看；所有权认证器（`ALMIDE_CERTIFY_OWNERSHIP=report`）会指出没有任何使用处需要拥有、却仍被拥有的参数。

<a id="effect-functions"></a>

### Effect 函数

用户 `effect fn` 若声明成功类型 `T`，则具有可失败的 Result 接口（默认错误类型为 `String`）；显式声明的 Result 保留其错误类型。源码传播是显式的：写 `!`，或按 ADR-0008 处理／丢弃 Result。前端将此意图 lowering 为 IR；原生 `ResultPropagationPass` 将签名、尾部及调用适配为 Rust 表示。它不会恢复源码中的隐式传播。下面的 Rust 是示意代码（省略运行时包装类型）。

```almd
effect fn read_config(path: String) -> String =
  let content = fs.read_text(path)!
  content
```

```rust
pub fn read_config(path: String) -> Result<String, String> {
    let content: String = almide_rt_fs_read_text(&path)?;
    Ok(content)
}
```

<a id="generic-bounds"></a>

### 泛型约束

泛型约束分两级：

- **完整**（用户函数）：`T: Clone + std::fmt::Debug + PartialEq + PartialOrd`
- **最小**（随附 .almd 模块函数）：`T: Clone`

<a id="7-wasm-target-not-in-this-crate"></a>

## 7. WASM 目标（不在此 crate 中）

**源码**： `src/wasm_route.rs`, `crates/almide-wasm`

CLI 与库使用方共享一条路径：链接后的有类型 IR 经结构化发射器变为 Wasm 字节，而不经过 MIR-to-WAT 渲染或 CLI 的 `wat` 汇编步骤。不在支持范围内的程序会得到具名 wall（CLI 中为 `E082`）；不会回退到已退役渲染器。浏览器运行器使用的 stock-WASI 形式由 `almide_wasi::to_wasi` 派生。运行时导入及自托管标准库函数体是链接程序的一部分，不是选择另一渲染器的理由。

详情：[../wasm/WASM-OUTPUT.md](../../../../wasm/WASM-OUTPUT.md)、[../ARCHITECTURE.md](../../../../ARCHITECTURE.md)、[crates/CLAUDE.md](../../../../../crates/CLAUDE.md)。

<a id="8-codegenoptions"></a>

## 8. CodegenOptions

**源码**： `crates/almide-codegen/src/lib.rs`

```rust
pub struct CodegenOptions {
    pub repr_c: bool,  // Emit #[repr(C)] on structs/enums for stable C ABI layout
}
```

当 `repr_c` 为 true 时，`struct_decl` 和 `enum_decl` 模板选择 `when_attr = "repr_c"` 变体，在 derive 块前加上 `#[repr(C)]`。

<a id="9-module-function-naming"></a>

## 9. 模块函数命名

**源码**： `crates/almide-codegen/src/walker/mod.rs`, `render_program()`

导入模块的函数使用带前缀的名称输出：

```
fn almide_rt_{module}_{function}(...)
```

模块标识符来自 `IrModule.versioned_name`（若已设置）或 `IrModule.name`，将点替换为下划线。例如，版本为 `1.0` 的模块 `string` 设置 `versioned_name = "string_1_0"` 时，生成 `almide_rt_string_1_0_trim(...)`。

模板 `module_call` 处理调用位置：

```toml
[module_call]
template = "almide_rt_{module}_{func}({args})"
```

`StdlibLoweringPass` 根据 build.rs 生成的 `arg_transforms` 表，附加 IR 级参数修饰（BorrowStr、BorrowRef、ToVec、LambdaClone、Direct），将 `CallTarget::Module { module: "list", func: "map" }` 重写为 `CallTarget::Named { name: "almide_rt_list_map" }`。

<a id="10-codegenannotations"></a>

## 10. CodegenAnnotations

**源码**： `crates/almide-codegen/src/annotations.rs`

注解由 nanopass 的各 pass 填入，由 walker 读取。walker 从不直接检查类型或上下文。

```rust
pub struct CodegenAnnotations {
    pub lazy_vars: HashSet<VarId>,                          // Top-level lazy vars (need *DEREF)
    pub ctor_to_enum: HashMap<String, String>,              // Constructor -> enum name (Red -> Color)
    pub anon_records: HashMap<Vec<String>, String>,          // Field names -> generated struct name
    pub named_records: HashMap<Vec<String>, String>,         // Field names -> declared type name
    pub recursive_enums: HashSet<String>,                    // Enums with recursive variants
    pub boxed_fields: HashSet<(String, String)>,             // (ctor, field) pairs needing Box::new()
    pub default_fields: HashMap<(String, String), IrExpr>,   // Default field values for constructors
}
```

<a id="11-targetattrs"></a>

## 11. TargetAttrs

**源码**： `crates/almide-codegen/src/pass.rs`

由 pass 设置、模板渲染器使用的逐节点属性：

```rust
pub struct TargetAttrs {
    pub needs_try: bool,             // Append ? for auto-propagation
    pub needs_clone: bool,           // Emit .clone()
    pub needs_borrow: bool,          // Emit & reference
    pub needs_box: bool,             // Wrap in Box<T>
    pub none_type_hint: Option<String>, // None::<T> with explicit type
    pub match_as_str: bool,          // .as_str() on match subject
    pub lazy_init: bool,             // Top-level let -> LazyLock
}
```
