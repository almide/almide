<a id="edit-locality"></a>

# 编辑局部性

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](edit-locality.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](edit-locality.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](edit-locality.zh-CN.md)



> 权威原文：[docs/specs/edit-locality.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/edit-locality.md)，固定提交 `35297a612fb29e3f78f6587aba253aa91d51f466`。这是非规范性的读者快照译文，不自动同步。原规格及其声明的规范地位保持不变。模型证明、实现主张、历史状态和证据边界均保留原文范围；本译文不提供独立执行、测量、符合性或安全性证明。代码块内代码及注释逐字保留。

> 最后更新：2026-09-23

这是 Almide 使命指标背后的不变量。MSR（modification survival rate，修改存活率）通过实验衡量 LLM 的编辑能否存活；本文陈述使编辑能够存活的语义性质，将每条现有语言规则映射到其维持该性质的作用，并以 `file:line` 证据记录当前实现仍违反它的位置。建立在此陈述之上的研究计划（Lean 内核、后端精化、MSR 预测循环）位于 `docs/roadmap/active/edit-locality-theory.md`；本文区分模型主张、当前规则和下文保留的实现问题排查记录。

可观察量的集合由契约台账（`docs/contracts/contracts.toml`）一次确定：**标准输出字节、标准错误输出字节、退出码**。下文的“可观察量”始终只指这些。

**这里的 `L<n>` 是编辑局部性阶梯，而本仓库不只有这一套阶梯。** `docs/roadmap/active/trust-layer.md` 有自己的 L0–L4：L1 是“二进制证明自己在墙内能做什么”，L2–L4 是“不必相信任何人的话即可重新推导”。读者在路线图里遇到 `L3` 时，读到那套阶梯的可能性与读到这套一样大。两套阶梯互不相关：这里的任何条目都不是那里的前提。上下文不明确时，请说明指哪套阶梯。

**下文的 L1 到 L5 已占用。** 此阶梯的新证明义务应使用 **L6** 及以上，不能使用看似空闲的小编号。almide/almide#2009 与 #1998 基于本文件只命名了 L1 的旧版本编写，两者均为不同于此处定义的义务提出“L2”和“L3”（#2009 指声明框架保持，以及可组合的受检查 lowering）；它们在 §1a 中是 **L4 和 L5**，在 #1997 固定其量化的 `scoped` 表层语法后被采纳。把一个编号复用于第二种含义，正是本文要从程序中排除的缺陷，不应出现在文档自身。

<a id="1-the-invariant"></a>

## 1. 不变量

**L1 — 编辑框架。** 令 `P` 为良好类型的程序，`e` 为对一个定义 `f` 的函数体进行的编辑，且保持 `f` 的签名：名称、参数类型、返回类型、`effect` 标记与可见性。则 `P[e]` 的每次轨迹从不进入 `f` 的执行，都具有与 `P` 对应的同一执行完全相同的可观察量。

**L2 — 跨目标一致。** 可观察量仅由源码决定，与目标无关：native 与 wasm 逐字节一致。这是契约台账的长期承诺（当前契约行、`spec/wasm_cross/` 测试样例、差分模糊测试）；L1 能对“可观察量”量化，正是因为 L2 使这个词具有明确定义。

**L3 — 诊断局部性。** 保持签名却使程序类型不正确的编辑，应产生定位于被编辑定义内部而非未编辑代码的诊断。（E006 在尝试调用 effect 的纯 `fn` 内触发；E041/E042 在编辑后函数体中的违规语句处触发。）

L1 是承重部分。几乎没有已实现的语言满足它：重载解析、隐式实例、宏、全程序推断和动态作用域的 effect 处理器，都允许远处的编辑改变未编辑代码的含义。Almide 因 MSR 而逐项采纳的规则，恰是 L1 的前提条件；§2 的表格使这一主张可以核查。

对于内核演算 λ_almd，L1 是机器检查过的定理：`crates/almide-edit-belt`（第三条 0-sorry Lean 证明带，CI 的 `lean-proofs` 作业）证明了 `edit_frame` 加确定性（可观察量相同的形式）、`typing_modular`（相同签名的函数体替换保持良好类型，其他所有推导均不变）以及 `pure_silent`（在 `effect fn` 之外被赋型的代码具有空轨迹）。按照 `docs/contracts/README.md`，针对完整语言的实现级陈述仍受 §2 的证据约束。

> 译注：以上关于已实现语言的一般比较是原文主张，并非本译文独立比较验证的结果。不得把内核定理扩展成对完整实现的证明。

<a id="1a-above-l1-the-declared-frame-and-its-composition"></a>

## 1a. L1 之上：声明框架及其组合

L1 对确实经过被编辑定义的执行不作陈述，也无法作出这样的陈述：合法替换可能计算不同的值。此时保留下来的不是值，而是声明契约围绕定义划出的**框架**。L4 和 L5 是表达这一点的两项义务。

**L4 — 声明框架保持。** 满足 `f` 声明契约的 `f` 替换，保持该契约覆盖的框架。

- *Effect 框架 — 已在 λ_almd 中证明。* 若 `f` 声明为纯，则任何通过 `f` 未变签名检查的替换函数体，在编辑后程序的任何环境中都不产生可观察量。`effect` 标志是替换不能跨越的围栏。证据：`crates/almide-edit-belt/AlmideEditBelt/Contract.lean` 中的 `l4_pure_replacement_silent`，由 `typing_modular`（编辑后程序仍良好类型）与 `pure_silent` 组合而来。旁边有两个经过检查的见证：`l4_witness`（四项假设同时成立，说明定理并非空泛成立）与 `l4_loud_replacement_prints`（通过自身 *effectful* 签名检查的替换会打印 `"hi"`，去掉纯性假设后结论便不成立）。
- *资源框架（`scoped`，#1997）— 测试样例证据，不是证明。* λ_almd 没有堆、也没有区域，所以区域契约在那里无对象可量化；要机械化这一半，必须先为内核增加区域组件。在实现层面，它分为三条各有证据的契约：
  - C-362 — `scoped` 块的可观察量就是其块体的可观察量；区域本身不可观察。`spec/wasm_cross/scoped_region_value.almd`。
  - C-363 — `scoped fn` 在区域内外计算相同答案。`spec/wasm_cross/scoped_worker_in_and_out.almd`。
  - C-364 — scoped 片段之外的形状在检查时被拒绝，且两个目标一致。`tests/diagnostics/e087-scope-unscoped-callee/broken.almd`、`tests/scoped_region_test.rs`。

L4 对业务行为不作主张。纯替换可以返回不同的值，该值可能改变*调用者*随后打印的内容；L4 仅说明替换自身不向轨迹增加任何东西。

**L5 — 实现与组合。**

- *组合 — 已在 λ_almd 中证明。* 任意保持签名的替换序列都使良好类型的程序继续良好类型，全部替换之后，每个声明为纯的定义仍保持静默：无论多长的保契约编辑链，都无法打开 effect 围栏。证据：`crates/almide-edit-belt/AlmideEditBelt/Contract.lean` 中的 `l5_edits_compose` 与 `l5_pure_silent_after_edits`，对编辑列表作归纳证明。
- *两个目标上的实现 — 受信任。* 受检查 lowering（#1995 的 ExitPlan、#1996 的 E-OWN-LOWERING）在 native 与 wasm 上保持这些观察，是 λ_almd 与两个后端之间的精化义务。与本文件中其他后端主张一样，它受 `spec/wasm_cross` 测试样例和契约台账约束，并在 `docs/contracts/README.md` 中列为受信任，而非已证明。

<a id="2-what-enforces-it-today"></a>

## 2. 当前由什么强制保证

| 规则 | 局部性作用 | 证据 |
|---|---|---|
| `fn`／`effect fn` 的返回类型在语法上强制要求（硬解析错误，AST 中无 `Option<TypeExpr>`） | 调用者看到的签名是文本而不是推断结果，函数体编辑无法改变它 | `crates/almide-syntax/src/parser/fn_decls.rs:69-82`, `crates/almide-syntax/src/ast.rs:401` |
| `effect` 是签名的一部分；检查器读取 `sig.is_effect`，而非函数体 | effect 由“保持签名”限制；普通 effect 调用需要标记；六个 console／abort 内建函数有 ADR-0022 例外 | `crates/almide-frontend/src/check/calls.rs:381-408` — 规格：`effect-system.md` |
| 仅显式传播（ADR-0008）：E041/E042 由被调用者**声明的** `Result` 返回类型驱动 | 被调用者增减错误路径不能悄悄重接调用者控制流；每个传播点都在调用者自己的文本中可见（`!`） | `crates/almide-frontend/src/check/post_solve_validation.rs:399-434` — 测试：`tests/expr_stmt_diag_test.rs`, `spec/wasm_cross/expr_stmt_comment.almd` (C-252) |
| 没有 glob 导入；自动导入集合是编译期常量 | 向模块添加定义不能捕获别处既有的非限定引用 | `crates/almide-syntax/src/parser/declarations.rs:75-125`, `AUTO_IMPORT_BUNDLED` in `crates/almide-types/src/stdlib_info.rs:39-56` — 规格：`module-system.md` |
| 跨模块访问使用限定名或显式选择导入；没有传递访问 | 引用在原地或通过固定导入绑定指定模块；新增内容无法悄悄替换该绑定 | `crates/almide-frontend/src/import_table.rs:23-25` — 测试：`spec/integration/modules/` |
| 无重载、用户自定义运算符、隐式转换、实例声明；protocol 符合性通过声明（`deriving`）给出，查找遵循固定约定 | 不存在获胜对象取决于非局部信息的分派 | `docs/design/DESIGN.md:27`, `crates/almide-frontend/src/canonicalize/registration_validate.rs:237-265`, E012 (`docs/diagnostics/E012.md`) |
| 固定的 UFCS 优先阶梯，标准库优先 | 新的用户 `fn len` 不能抢走 `xs.len()` | `crates/almide-frontend/src/check/calls_ufcs.rs:36-67` — 测试：`spec/lang/user_fn_namespace_collision_test.almd` |
| 类型推断按模块隔离；调用返回类型来自声明签名；单态化仅以调用点参数类型为键 | 约束不跨模块边界；确定调用者类型时从不查阅被调用者函数体 | `crates/almide-frontend/src/check/module_inference.rs:16-19`, `check/calls.rs:276-310`, `crates/almide-optimize/src/mono/discovery.rs:28-42` |
| 标准库纯性是硬编码、由机器门禁约束的注册表，从不由函数体推断 | 无论标准库函数体如何编辑，优化使用方看到的答案固定 | `crates/almide-mir/src/purity.rs:51,303-306`, 门禁 `proofs/check-stdlib-purity-registry.sh` |
| LICM 只提升可安全投机执行的表达式：既无 effect，也无可能陷阱的操作（索引、map 访问、非字面量整数除法／取模）；表达式谓词和函数体不动点两处均强制执行 | 优化不能把陷阱泄漏到源码从未执行的路径（零次循环），因此改变推断纯性的函数体编辑不能制造新的可观察量 | `crates/almide-codegen/src/pass_licm_purity.rs` (`binop_may_trap` + 考虑陷阱的纯集合, #1424) — 测试：`spec/wasm_cross/licm_zero_trip_hoist.almd` (C-279) |
| 局部 `fn` 不得与选择导入的函数共享裸名称，构建导入表时产生硬错误 E050 | 每个裸名称必须恰有一个解析结果；添加定义不能悄悄改变既有调用目标（修复前检查器与 lowering 分歧，执行的函数不是类型检查的那个，#1425） | `crates/almide-frontend/src/import_table.rs` (`check_selective_import_fn_collisions`) — 测试：`tests/selective_import_collision_test.rs`, `docs/diagnostics/E050.md` |
| 构造器解析以所有者为作用域：限定的 `mod.Ctor` 仅在 `mod` 内解析（`lookup_ctor_owned`）；裸名称优先本模块自己的分支，只看本文件、其导入项与内置分支，不看传递模块，也不越过本文件自己的同名类型（#413、#2636、`lookup_ctor_in`，lowering 同步采用）；同模块重复分支名在注册时产生硬错误 E019 | 别处添加 variant 不能重新绑定既有构造器引用；模块内每个分支名恰有一项声明 | `crates/almide-frontend/src/type_env.rs` (`lookup_ctor_owned`), `canonicalize/registration.rs` (注册时 E019, #1426) — 测试：`tests/ctor_ambiguity_test.rs`, `spec/integration/modules/qualified_ctor_test.almd` |

<a id="3-where-it-breaks-today-hunt-of-2026-08-15"></a>

## 3. 当前在哪里失效（2026-08-15 的排查）

以下排查作为实现历史保留。旧 MIR／既有管线的路径不是当前发布版本的故障清单；把某行当作未解决缺陷之前，必须检查当前源码。借用推断一行已更新为当前求解器。此处列出机制。分类：**V** = 语言／编译器错误，L1 必须成立；**2026-08-15 排查的三个 V 发现均已修复，并提升为 §2 的规则行**（#1424 LICM 投机、#1425 选择导入冲突、#1426 构造器所有者作用域；未修复数量只能保持零）；**R** = 后端精化义务（路线图 Stage 3）；**S** = 不作修复、而在 §4 声明的附加条件。

<a id="r--backend-refinement-obligations"></a>

### R — 后端精化义务

它们位于表层语言之下；“作为精化的后端”（路线图 Stage 3）必须履行这些义务。原始排查记录了构建失败或错误代码生成机制；涉及已退役 MIR／既有管线的行描述的是先前路径：

| 发现 | 机制 | 证据 |
|---|---|---|
| 自举 splice 传递地链接整个源文件直至不动点；编辑标准库函数 `A` 会使只调用同文件 `B` 的程序也引入新文件 | `link_self_host_runtime_to_fixpoint` | `crates/almide-mir/src/pipeline_link.rs:229-262` |
| splice 链接冲突是硬编译错误；标准库函数体编辑可能破坏从不调用该函数的程序（实际案例：`__hex_fill`，almide#1068） | `dedup_linked_by_name` | `crates/almide-mir/src/pipeline_link.rs:184-222` |
| 全局 impl 名→调用名重写捕获用户标识符（用户的 `fn int_abs` 被重定向到 `int.abs`） | 对**所有** MIR 函数执行 `rewrite_impl_names_to_call_names` | `crates/almide-mir/src/pipeline_link.rs:272-286` |
| 借用推断是遍历函数体的全程序单调不动点；函数体编辑可能改变调用点的所有权注饰。旧轮次上限已移除：槽位沿 Ref → RefMut → Own 上升，并检查单调性与格高度 | `infer_borrow_signatures` | `crates/almide-codegen/src/pass_borrow_inference.rs:455-497,568-582`, `pass_borrow_inference_call_sites.rs:3-18` |
| `inline_pure_call_globals` 根据传递的函数体纯性决定是否将顶层 `let X = f()` 初始化式替换到使用点，把潜在陷阱从“启动时总发生”移至“仅读取时发生” | 递归进入被调用者函数体的 `expr_is_pure` | `crates/almide-mir/src/lower/newtype_erase.rs:385-462,517-521,643-656` |
| 增删 `test` 块会切换 `library_mode`，改变全程序的 wasm DCE 根 | `reachable_fn_names` | `crates/almide-codegen/src/reachability.rs:168-183` |
| `Method`／`Computed` 调用目标不提供可达性名称；函数体编辑若移除对经 UFCS 可达函数的最后一个 `Named` 引用，会错误裁剪该函数（按该 pass 自身的健全性注记，会产生 wasm 陷阱） | 可达性收集 | `crates/almide-codegen/src/reachability.rs:15-17,66-72` |
| 潜在问题：已有从函数体推断全程序 effect 的分析（当前仅用于分析）；一旦诊断或权限使用它，函数体编辑就会改变调用者可见的输出 | `pass_effect_inference` | `crates/almide-codegen/src/pass_effect_inference.rs:12,47-70` |

<a id="4-side-conditions"></a>

## 4. 附加条件

§1 的 L1 在**扣除**下列已声明例外后成立。例外要么在这里写明，要么就不存在；未写明的例外是错误。

- **S1 — 无注解的顶层 `let`／`var`。** `TopLet` 的类型从初始化式推断，并跨模块流动（`crates/almide-syntax/src/ast.rs:406`、`crates/almide-frontend/src/check/module_inference.rs:65-90`）。所以编辑顶层初始化式即使没有写出签名，也是*改变签名*的编辑。存储在此类绑定中的 lambda 同样如此。（`fn`／`effect fn` 不在此列，因为其签名是必写的文本，见 §2 第 1 行。）
- **S2 — `test` 块在框架之外。** 文件作用域的 `mod test where { path = expr }` 覆盖会有意重新绑定文件内每项测试中的引用（`crates/almide-syntax/src/ast.rs:409-432`、`crates/almide-frontend/src/lower/mod.rs:420-423,489-495`）；这是 L1 在程序代码中禁止的动态作用域的一种有界、已声明形式。测试块也作为 DCE 根（§3-R）。L1 量化的是程序执行，而非测试执行。
- **S3 — 模型纯性与实现边界。** ADR-0009 最初的高阶 effect 漏洞已是历史：函数类型 effect 标记和值流检查现已拒绝这种洗白（参见 `effect-system.md` 与 `tests/effect_value_laundering_test.rs`）。这并不使模型的 `pure_silent` 定理成为关于每个 Almide `fn` 的定理：ADR-0022 允许其中使用六个 console／abort prelude 内建函数。模型定理保留其自身的赋型前提；实现的 capability 核算遵循 ADR-0026。

<a id="5-the-gate"></a>

## 5. 门槛

每项语言或编译器变更合入前都要回答一个问题：**是否保持 L1？** 保持，就合入。需要附加条件，就在同一 PR 中将其写入 §4。违反，便是设计在说“不”。（动态作用域处理器、隐式实例、glob 再导出和任何从函数体推断的东西都无法通过此门槛；这正是门槛在发挥作用。）
