<a id="changelog"></a>

# 更新日志

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](CHANGELOG.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](CHANGELOG.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](CHANGELOG.zh-CN.md)

> 非规范性、固定原文的历史读者快照；未修改的原本仍为权威来源。本翻译不自动同步，也未独立执行、重新测量或验证证明。首项为2026-07-26的0.36.0，并非对最新版本的声明。“现在”、功能支持与移除、CI结果、等价性及性能/模型比较均属于各条目的日期，不代表当前实现或普遍保证。原文 certified 指所述编译器证书检查，并非第三方认证。保留原文不一致：0.15.0称十一个类别、十二个模块，但各列出十三项；S2硬契约/非Unknown声明与发布构建在诊断后继续编译的描述并存。0.14.6的已知缺口与后文已新增功能的描述、E001..E099声明与E420也并存。有限dojo结果及作者的SOTA/UFCS结论是历史观察和解释，并非对全部模型或任务的证明。包括历史路径和命令在内的代码标记保持原样。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/CHANGELOG.md)

本项目的重要变更均记录于此。本文件从 `0.14.6` 开始编写；更早的版本在文末“本文件创建之前”一节中回顾性地概述。

格式大致遵循 [Keep a Changelog](https://keepachangelog.com/en/1.1.0/)；各条目按照面向诊断、工具、语言或标准库的目的分组，因为这正是下游使用者（LLM 测试框架、编辑器、用户）所关心的内容。

## [0.36.0] — 2026-07-26

新增十项行为契约（C-165…C-174），移除 wasm 渲染器的静默延迟处理机制，从契约台账生成一致性报告，并新增两个旗舰示例，在每次 `cargo test` 时跨目标逐字节比较。

<a id="fixed--cross-target-equivalence"></a>

### 修复 — 跨目标等价性

- **TCO Map/Set 累加器的初始值**（C-174）：尾递归累加器中的 `acc + []` 以前绑定的是一个空的延迟块，导致非空初始值在 wasm 上静默消失——这是 #810 清查发现的一项实际误编译。现在通过真正的 `Op::Dup` 将初始值共享到累加器中；零次迭代、Set 累加器及 `_str` map 布局均由 fixture 固定覆盖。（#810）
- **整数文字量**（C-173）：范围检查同时考虑符号和 i64 载体，而不再只看绝对值——`let x: UInt32 = -5` 和超出载体范围的十六进制文字量分别获得自己的 E024 提示，说明类型的范围，并按原样显示数值（`-5`，而非 `5`）。检查会递归进入聚合集合元素以及记录字段 / 元组位置（模糊测试器发现的 check 与 build 之间的缺口）。#872 跟踪高于 `i64::MAX` 的 UInt64 路径。
- **类型擦除的 `unwrap_or`**（C-172）：对所有堆载荷实现自托管，清除了最后一类未链接调用的拒绝编译情形。
- **字节偏移边界**（C-171）：改用减法形式编写，防止接近 `i64::MAX` 的位置通过回绕越过边界。
- **整数算术生成回绕操作**（C-170）：在模块级 `let` 以及 `overflow-checks = true` 下，溢出规则也成立。
- **`list.repeat`**（C-169）：使用与 `string.repeat` 相同的大小上限——数量过大时，两个目标都以相同方式中止。原生运行器的 `string.repeat` 适配层也会将负数次数钳制，并在超过共享的 2^31 字节上限时中止，而非 panic。
- **`list.flatten`**（C-168）：借用其参数，因此可以在同一个表达式中与另一个借用组合。
- **`float.clamp`**（C-167）：采用 `f64::clamp` 所使用的比较链，使范围内的负零在两个目标上都保留其符号。
- **`map` 的 `to_string` 系列**（C-166）：通过 repr 路由器路由，不带后缀，并对 Float 值的显示实现自托管。
- **堆累加器折叠**（C-165）：对 skv map 布局、String 列表及 String 集合实现自托管。
- **`some(...)` 中返回堆类型的调用**不再能被存储为标量——Some 载荷检查读取载荷类型，因此在设计修复期间，#871 中产生错误值的形式会被拒绝编译。
- **含堆元素的元组捕获**会包裹 clone，因此闭包捕获 `(String, …)` 元组后，仍可使用原元组——这是夜间模糊测试发现的第二项实际问题。

<a id="hardened--the-deferred-bind-terminal-810-777"></a>

### 加固 — 延迟绑定终端（#810、#777）

- `Init::Opaque` 以前在一个类型中混合了三种含义：真正为空现在使用独立的 `Init::Empty`，TCO 累加器共享使用真正的 `Op::Dup`，无法构建则拒绝处理——绑定终端以默认拒绝方式工作，只允许三种形状（ClosureCreate / Lambda / Range），使 C-174 这一类问题无法再次静默发生。
- 嵌套深度超出 effect-unwrap 去糖覆盖范围的错误运算符，会按名称拒绝编译，而不是冒险生成延迟 Const；一项方向证明测试要求将来对去糖的扩展与原生输出逐字节一致。
- 每个降低后的函数都会通过先定义后使用的良构性检查，该检查对所有操作进行穷尽的定义 / 读取分类——新增 Op 变体若未分类就会导致编译错误。（#777）

<a id="added--evidence--conformance"></a>

### 新增 — 证据与一致性

- ALS 一致性报告由契约台账生成（`docs/contracts/conformance.md`，台账每节均有 fixture 支撑），并设有 CI 新鲜度检查；契约索引也新增同样的检查。（#811）
- 在 `docs/contracts/` 下新增已证明与受信任部分的边界图。
- 移除了两个 v0 时期的 wasm 测试跳过项；台账检查确保剩余每个跳过项都对应已声明的平台限制。
- 被拒绝编译的 `main` 会报告原因，而不再只说明其超出子集。

<a id="added--examples"></a>

### 新增 — 示例

- `examples/raytracer.almd`：直接在终端输出 ANSI 24 位半块字符图像的光线追踪器——三个镜面球、棋盘地板、硬阴影、镜面高光、三次镜面反弹——跨目标逐字节一致。
- `examples/lisp.almd`：一个支持闭包、词法作用域及后期绑定顶层定义的迷你 Scheme；`fact`、`fib`、`map` 以及快速排序在被解释的 Lisp 内运行——跨目标逐字节一致。
- `tests/examples_parity_test.rs`：每个示例要么属于 PARITY（每次 `cargo test` 都对原生与 wasm 逐字节比较），要么属于 LEDGERED，并注明无法比较的原因——两份列表均未列出的示例会使测试套件失败。

<a id="internal--code-health-codopsy-round-2"></a>

### 内部 — 代码健康度（codopsy 第 2 轮）

- 通过提取名称路由器、采用带完备性测试的表驱动查询，将 almide-frontend、almide-tools、almide-interp 和 almide-dialect 提升到 A 级；E015 reimpl-lint 建议现在是确定性的（对排序后的函数列表计算编辑距离）。
- 将 almide-mir 中最复杂的四个函数（圈复杂度 103–116）拆成阶段辅助函数，并在两个渲染器上对 986 个文件的语料进行字节验证（emit 与 wasm 哈希均未变化）。#852 继续推进。
- MIR 调试通道统一通过一个有文档的 trace 模块路由，并以名册测试确保所有通道都被列出。

## [0.35.0] — 2026-07-25

这是一次由差分模糊测试器和问题台账推动的正确性发布：修复了十项跨目标差异，关闭了三个 v1 子集拒绝编译缺口，以及两个 `almide check` 通过但生成代码无法构建的验收缺口。四项新行为契约（C-161…C-164）和八个新跨目标 fixture 固定这些结果。

<a id="fixed--cross-target-equivalence-1"></a>

### 修复 — 跨目标等价性

- **Matrix 构造器维度**（C-161）：负维度钳制为 0（空矩阵），元素数超过共享的 2^28 上限时，两个目标都以 `Error: matrix dimensions too large` + 退出码 1 中止。使用 `seq = -2147483648` 构建的 transformer 形状程序，以前在两条路径上会以不同形式崩溃——原生为 raw-vec 容量溢出 panic（退出码 101），wasm 为越界 trap（退出码 134）——T6 约定不允许其中任何一种。（#849）
- **`io.write` / `io.write_bytes`**（C-162）：两者现在都在 wasm 上基于单 iovec 的 `prim.fd_write` 底层接口实现自托管，与 `io.print` 使用相同的 `Capability::Stdout`。它们也会按程序顺序输出：原生以前通过一个仅在退出时刷新的 64 KiB `BufWriter` 输出，而 `println!` 使用 Rust 自身的 `Stdout`，因此交错输出会被重排——即使只看原生也不正确。（#805）
- **绑定到 let/var 的堆结果 `if`/`match`**（C-163）在两条路径上都会执行所选分支。遗漏的情形是在 `some(...)` 下使用 String 文字量载荷，正是归档的 café 模糊测试发现的问题。（#860）
- **对元组 / 记录 / 嵌套列表元素进行的列表修改与后缀复制**（C-164）：`set`/`update`/`insert`/`remove_at`/`swap` 和 `take_end`/`tail`/`drop_end` 现在都有引用计数正确的对应实现，因此结果共同拥有每个复制的元素，且源仍可读取。此前它们被路由到未注册名称并被拒绝编译。（#850、#808）
- **负偏移和负宽度都有定义**（C-034）：所有 `bytes.read_*` 偏移保护均使用 `checked_add`（负 `pos` 以前会变成很大的 `usize`，加法又回绕到范围内，在切片索引处 panic）；`bytes.read_f16_le` 的 wasm 自托管实现补上了同系列函数已有的负值保护；`string.pad_start`/`pad_end` 以有符号方式钳制宽度；`bytes.copy_to_ptr` 以无符号方式钳制容量；超过共享 2^31 字节上限的 `repeat` 在两条路径上以相同方式中止。
- **高精度下的 `float.to_fixed`**（C-155）：数字暂存区按 f64 精确展开约 1074 位的需求分配，但 `nd` 可达到 4096，因此超过边界的部分在 wasm 上会读取相邻堆内存。原有 fixture 无法发现这一点，因为它将展开结果截断到了六个字符。
- **空查找串的 `string.replace_first`**：“未找到”哨兵是 `send`，与空查找串在 `send` 处的合法匹配冲突，导致 wasm 上的 `replace_first("", "", to)` 原样返回被搜索字符串。
- **权重矩阵为空的 `matrix.swiglu_gate`**现在在两条路径上均返回空矩阵，与原生参考实现的空操作数保护一致。

<a id="fixed--check-accepted-must-build"></a>

### 修复 — check 接受的程序必须能构建

- **内建操作的借用方式来自运行时自身的签名**：`build.rs` 从 `runtime/rs/src/` 提取每个 `pub fn almide_rt_*` 的各参数是否为 `&mut`，确保只读辅助函数不会收到 `&mut`。旧启发式规则（“返回 Unit ⇒ 修改参数 0”）会针对 `&Vec<u8>` 参数生成 `almide_rt_io_write(&mut chunk)`——在 mandelbrot 基准中触发 E0596，对 `io.write_bytes` 和 `testing.assert_contains` 也存在潜在影响。（#803）
- **仅写入的 `var` 不会被无用绑定消除删除**：索引赋值是写入，因此 `use_count` 从不计入它，DCE 删除了 `let`，而回写仍引用它（E0425）。DCE 和未使用变量警告现在都会检查修改目标。（#857）
- **定宽整数集合文字量会做范围检查**：`let bs: List[Int8] =
  [1, 256]` 以前会先编译，再由 rustc 拒绝 `256i8`；现在元素文字量会固定到标注的元素类型，从而触发现有 E024 检查。
- **`mem.save` / `mem.restore` 有原生函数体**：此前它们声明了运行时从未定义的 `almide_rt_mem_*` 符号，导致任何调用它们的程序都生成无效 Rust。

<a id="fixed--diagnostics"></a>

### 修复 — 诊断

- **导入模块的类型错误会传递给导入方**（#862）：check 路径以前从不对模块体进行推断，而 build 驱动把 `infer_module` 的诊断附加到一个无人读取的列表。现在 `almide check`、`build` 和 `test` 都会失败，并指出模块自己的文件。一个 `base64` E006 问题曾因此隐藏数周。将其暴露后，也修复了之前被其掩盖的两个潜在误报：一个虚假的 E019（“构造器 'Wrap' 有歧义：在 Wrapper 和 Wrapper 中声明”），以及针对不同包中合法同名类型的虚假 E020。

<a id="changed--wasm-subset-coverage"></a>

### 变更 — wasm 子集覆盖

- **对于同时声明 `main` 的文件，`almide test` 会在 wasm 路径运行测试块**，与原生测试模式一致（原生会编译 `main`，但从不调用）。仅此一项就使 17 个 spec 文件不再回退到原生。（#813）
- **内置标准库模块按函数链接**：包含内建操作的模块，现在会将其纯 Almide 扩展（`list.split_at`、`iterate`、`bundled_probe`）作为普通同级函数链接，而由内建操作支撑的函数仍保持注册表分派。（#812）
- **按名称作出的标准库判定会移除单态化后缀**：`option.collect__Int` 以前未命中物化变体的读取形状检查，因此对其结果的 `match` 未被跟踪并被拒绝编译。（#812）
- `spec/` 中的原生回退项从 32 减至 11，其中 8 项确实是依赖宿主的 `// wasm:skip` 文件。
- **walled-real 棘轮指标回到 0**——自内置标准库链接工作以来，Trust Spine 检查一直未通过。原因有二：`classify_corpus` 以硬编码的标准库列表限制导入解析，因此 `import args` 从未进入该解析，`args.flag` 被当作能力未经验证的标准库调用而拒绝；此外，它保有自己的可链接模块规则副本，不了解按函数链接。两者现在都通过 `pipeline::linkable_module_fns`，使拒绝报告描述真实流水线实际编译的程序。（#848）

<a id="added--tooling"></a>

### 新增 — 工具

- **`ALMIDE_WALL_REASON=1`** 指出哪个阶段拒绝了 wasm 渲染（render / wat assemble / validate），并传递各函数的原因，因而无需再通过 `render_program` 手工推导回退文件的原因。
- **`almide compile <module> --json` 支持仅内置的模块**：二进制明明携带模块源码，`path`、`args`、`html` 和 `mem` 却曾报告“module not found”。`html` 完全没有 `bundled_source` 分支；现在通过单元测试保持两份列表一致。（#863）

<a id="added--documentation"></a>

### 新增 — 文档

- **新增十六个标准库页面**——现在每个内置模块都有页面（39/39，此前为 23）：`path`、`args`、`html`、`mem`、`net`、`zlib`、`base64`、`hex`，以及八个定宽数值转换模块。所有页面都包含机器维护的签名索引，其完整示例也可执行（`spec/stdlib/bundled_module_docs_test.almd`、`spec/stdlib/sized_numeric_docs_test.almd`）——本次发布中的三个缺陷正是这样发现的。（#863）
- **模块接口可以表示定宽数值类型**：`Int8`…`UInt64` 和 `Float32`/`Float64` 都有自己的 `Ty` 变体，而不是 `Named`，因此以前所有涉及它们的 `almide compile <mod> --json` 签名都被序列化为 `unknown`——`int.to_int8` 显示为 `(n: Int) -> ?`。
- **归档 `docs/wasm/` 设计备忘录**：六篇早于 v1 trust spine 的 2026-04 笔记移至 `docs/wasm/archive/`，并添加说明原因的提示。`memory-model.md` 描述了两块线性内存和不释放的 bump 分配器，而已发布编译器并未实现这种设计。（#863）
- **补全路线图索引**：补充 34 个缺失的 `<!-- description: -->` 和 21 个缺失的 `<!-- done: -->` 标签——描述根据各文件自身标题编写，日期从 git 历史推导。（#858）

## [0.29.0] — 2026-07-15

<a id="changed--compilation"></a>

### 变更 — 编译

- **v1 验证渲染器成为默认 wasm 路径**（97e1cd8c）：每次 `almide run` / `almide build --target wasm` 都通过带认证的 MIR 流水线编译——所有权、名称全域性和能力证书在每次构建时重新验证，检查器的健全性已在 Coq 中得到机器证明。`--no-verified` 可切回 v0 发射器。在切换之前，已修复组织级字节验证清查发现的六项默认验证路径输出差异（跨模块 ABI 注册表、void 闭包分派、嵌套 lambda 命名、缓存键的 `sort_by`、按几何级数增长的拼接容量、C-129 保护）。

<a id="added--language"></a>

### 新增 — 语言

- **验证路径上的可变模块级 `var`**（C-033）：模块全局值存放在线性内存槽中，具有经过认证的 take/drop 转换；存在别名的快照采用写时复制——`var g = [...]; let s = g; g[0] = 9` 观察到快照，与原生逐字节一致。这关闭了默认路径上最后一项已知输出差异，由 `spec/wasm_cross/module_var_alias_cow.almd` 固定覆盖。
- **`env.get`**（C-133、ALS-R5）：在两个 wasm 引擎上通过 WASI environ 读取宿主环境（每次启动 wasmtime 都接入 `-S inherit-env`）；原生读取进程环境。与所有具有 effect 的公开操作一样，受能力控制。

<a id="added--tooling-1"></a>

### 新增 — 工具

- **原生 trust-spine，第 1–4 阶段**（`--target rust --verified`）：标量、String 和列表程序从供给 wasm 路径的同一份认证 MIR 生成原生 Rust（Dup→clone、Drop→作用域结束处的消除、共享 ListLit/ListGet/ListSet 操作），以 v0 输出差分作为检查。子集之外的程序明确拒绝处理，并回退到 v0。

<a id="removed--language"></a>

### 移除 — 语言

- **移除 `fan.timeout`**（C-006、#765）：墙上时钟超时没有可移植的跨目标含义（wasm 没有时钟、调度器或线程），它也是唯一一个结果不是程序及其输入的函数的标准库操作——即使两次原生运行之间，期限是否触发也取决于机器负载。引用它现在会在 check 阶段产生带迁移提示的已移除功能错误（**E027**）；期限应由调用程序的宿主边界处理（`timeout 5 ./app`）。组织范围清查发现使用者为零。这移除了台账中最后一个 `flagged-for-revision` 契约——flagged 棘轮值为 0，README 的等价性声明现在写作“例外：无”。

<a id="fixed--wasm-codegen"></a>

### 修复 — WASM 代码生成

- **Perceus RC：`guard`-`else` 堆临时值**（#755）：在 `guard … else { … }` 块内绑定的堆局部值，在 WASM 目标上从未得到引用计数处理。`Guard` 属于 `IrStmtKind`，因此 `block_to_fnbody` 将其送入 perceus 的 `FnBody::Stmt` 直通分支，而该分支不递归处理任何内容——else 块的临时值没有作用域结束处的 `Dec`，WASM RC 验证遂拒绝构建（`[perceus-belt] LEAK: no RcDec`）。Stmt 分支现在对 Guard 的 `cond` 和 `else_` 执行 `perceus_expr`，使发散的 else 块与其他块一样经过往返处理并保持 Dec 平衡。

<a id="fixed--rust-codegen"></a>

### 修复 — Rust 代码生成

- **末尾位置返回裸值的 `@intrinsic` effect 函数**（#758）：对运行时返回普通值的内建 effect 函数的尾调用（`io.print →
  Unit`、`io.read_line → String`、`fs.exists → Bool`、`env.get → Option`），被错误地排除在 Result 尾部包装之外——豁免集合包含所有 `@intrinsic` effect 函数，而非仅包含运行时返回 `Result` 的函数——使自动包装为 `Result` 的函数末尾留下裸 `()` / 值（无效 Rust，E0308）。现在集合只由实际返回 `Result` 的内建函数组成（声明为 `-> Result[...]` 的函数，以及声明为 `-> Unit`、但运行时返回 `Result<(),
  String>` 的 `http.serve`），使裸值内建尾调用像其他尾部一样获得 `Ok(...)`。

## [0.23.3] — 2026-05-24

<a id="performance--wasm-parity-with-rust"></a>

### 性能 — WASM 与 Rust 持平

- **哈希表 map**：开放寻址，使用 FNV-1a/splitmix 哈希，在 75% 负载因子时扩容。`map.insert` 480ms → 0.5ms（960 倍），`map.get` 1041ms → 0.6ms（1700 倍）。
- **二叉递归变换**：`f(n-1) + f(n-2)` 模式 → 累加器循环。`fib(35)` 63ms → 35ms，与 Rust WASM 持平。
- **Lambda 内联**：无捕获 lambda 跳过闭包转换，函数体内联到 `list.map`/`filter`/`fold` 循环中。简单 lambda 获得零成本闭包。
- **排序连续段检测**：O(n) 升序 / 降序预扫描，直接交换反转。逆序 100k：5.6ms → 0.5ms。
- **WASM 流水线中的 TCO**：尾调用优化 pass 现在也对 WASM 目标运行（此前仅限 Rust）。
- **自适应暂存局部变量**：简单函数使用 4/2/2/0 个暂存局部变量，而非 48/16/16/8。`fib` 从 88 个局部变量降至 8 个。
- **字符串布局迁移**：修复 int_to_string、float_to_string、println_str、JSON 解析器、字符串谓词及其他 10 多个文件的数据偏移（4→8）。
- **WASM `datetime.monotonic_ns`**：新增分派，并修复 `clock_time_get` 的 8 字节对齐。

<a id="result"></a>

### 结果

- 相对 Rust WASM 有 3 项胜出：`int_tostring`（0.63 倍）、`int_parse`（0.83 倍）、`fib35`（≈1.0 倍）。
- 7 项持平：`list_map`、`list_fold`、`str_concat`、`math_sqrt`、`sort`、`map_insert`、`map_get`。
- 剩余 1 项差距：`list_filter`（3 倍，WASM 结构化分支与 LLVM cmov 的区别）。

## [0.23.2] — 2026-05-23

<a id="performance"></a>

### 性能

- **WASM 排序**：将 O(n²) 插入排序替换为 O(n log n) 自底向上归并排序——100k 元素上加速 222 倍。
- **WASM 字符串拼接**：基于容量的字符串布局 `[len][cap][data]`，通过 `__string_append` 运行时实现均摊 O(1) 追加；窥孔优化 `s = s + "x"` → 原地追加。
- **字符串文字量优化**：在 `&str` 借用上下文中，字符串文字量跳过 `.to_string()`——`string.contains` 572ms → 45ms。

<a id="added"></a>

### 新增

- 用于原地修改字符串的 `string.push(mut s, suffix)` 和 `string.clear(mut s)`。
- 标准库基准套件：18 个模块、43 项基准（`research/benchmark/stdlib/`）。

<a id="result-1"></a>

### 结果

- 组合基准（fib + sort + map + concat + bytes）中，Almide WASM 为 Rust WASM 的 0.81 倍（更快）。

## [0.16.1] — 2026-05-12

<a id="added-1"></a>

### 新增

- **almide-dialect crate**：纯 Rust 的 MLIR 方言 schema，具有 SSA 降低、dump、Rust 发射器和验证器。方言流水线通过了针对现有 Rust 代码生成的 7/7 项等价性测试。
- **编译期值参数**：Mojo 风格的 `fn f[N: Int]()`——自动将标量类型约束识别为 const 参数。端到端可用：`zeros[5]()`、`dot[3](a, b)`。
- **Mojo 对等性评估**：`docs/roadmap/active/mojo-parity-assessment.md` 中的 16 轴加权评分。

## [0.16.0] — 2026-05-11

<a id="added-2"></a>

### 新增

- **位自省**：`int` 标准库新增 10 个函数——`count_leading_zeros`、`count_trailing_zeros`、`pop_count`、`bit_reverse`、`byte_swap`、`bit_width`、`log2_floor`、`log2_ceil`、`next_power_of_two`、`prev_power_of_two`。Rust 运行时 + 原生 WASM 代码生成（`i64.clz`/`i64.ctz`/`i64.popcnt`）。
- **Hashable 协议**：编译期验证 Map 键类型——`Map[Float, V]` 和 `Map[Fn, V]` 现在会报错。内建类型（Int、String、Bool、定宽整数）自动注册为符合 Hash。带 `deriving Hash` 的用户类型生成 `#[derive(Eq, Hash)]`，可用作 Map 键。

## [0.15.10] — 2026-05-10

<a id="fixed"></a>

### 修复

- WASM JSON 解析器：现在能正确解析科学记数法（`1.5e-7`、`2E10`）。

## [0.15.9] — 2026-05-10

<a id="fixed-1"></a>

### 修复

- WASM 库模式：没有 `main` 的 `almide build --target wasm` 现在会跳过测试函数并省略 `_start` 导出，为浏览器 / JS 嵌入生成干净的库模块。

## [0.15.8] — 2026-05-10

缺陷修复补丁。重写 ResultPropagation 并修复 WASM guard-else。

<a id="fixed-2"></a>

### 修复

- match 分支中的 `err()`：match 类型不是 Result 时提前返回，否则作为值。
- 跨模块记录文字量（`module.Type { ... }`）无法解析的问题。
- 包含 `fs` 运行时时自动生成 `FileStat` 结构体。
- effect 函数中的 `guard ... else { println(...); () }` 现在包装为 `Ok(())`。
- WASM 中的 `guard ... else { err("msg")! }`：剥离 Block/Unwrap/Try 包装层。
- 被修改的 `Bytes`、`Vec`、`HashMap` 绑定未提升为 `let mut` 的问题。

<a id="changed"></a>

### 变更

- 重写 ResultPropagation pass：移除 700 行死代码，新增 `resolve_err_types`。
- ConcretizeTypes `kind_name()`：列出全部 IrExprKind 变体（此前有 26 个隐藏在 `(other)` 下）。
- ConcretizeTypes `infer_err_ty_from_enclosing`：支持提升前的 `enclosing_ret`。

## [0.15.0] — 2026-04-20

跨模块 `let` 达到 Swift/TypeScript 的对等水平，关闭 WASM 分派缺口（包括已有缺陷和最后三个矩阵操作阻碍），并新增 CI 检查防止未来分派偏离。变更均为增量添加——无破坏性 API 变更或公开功能移除——但模块系统的公开功能变化足够大，因此跨越一个次版本。

<a id="cross-module-let-now-works-for-every-type"></a>

### 跨模块 `let` 现在适用于所有类型

模块作用域的 `let` 绑定（`util.almd` 中的 `let FOO = ... `，`main.almd` 中的 `util.FOO`）此前在几种形式下存在问题：

- 标量常量（`let MAGIC: Int = 42`）编译为 `(*ALMIDE_RT_UTIL_MAGIC)`，但生成的 Rust 是 `const ALMIDE_RT_UTIL_MAGIC: i64 = 42i64;`——`i64` 不是引用，rustc 拒绝解引用。
- 复合值引用（`let RES: Result[Int, String] = ok(7)`）在模块降低时丢失类型标注。生成的 Rust 变成 `LazyLock<Result<i64, _>>`，rustc 以 `E0121` 拒绝。即使恢复标注，`*LAZY_VAR` 仍从解引用中移出值；消耗值的调用者（`result.unwrap_or((*LAZY_VAR), ...)`）会触发 `E0507`，因为 clone 插入 pass 遗漏了合成的跨模块 Var。
- 无标注文字量（`let ANON = { a: 1, b: "hi" }` / `let MAP = ["a": 1]`）在 `main` 的使用位置被类型检查为 `Unknown`，产生 `LazyLock<_>`（rustc）或 `ConcretizeTypes` 后置条件违反（WASM）。

四项独立修复依次落地：

- `walker/expressions.rs`——不再假定合成的 `ALMIDE_RT_<MOD>_<NAME>` Var 必然基于 `LazyLock`。`CodegenAnnotations` 新增 `lazy_top_let_names: HashSet<String>`，从各模块的 `TopLetKind::Lazy` 条目预先建立索引；标量 `Const` top_lets 以裸名称生成。
- `frontend/src/lower/mod.rs`——`TopLet` 分支的类型查询按 `{prefixed, unprefixed, expr_ty}` 回退，使模块作用域绑定找到注册阶段播种的标注类型，而不再静默地重新推断成缺少泛型信息的 `Result[Int, ?]`。
- `pass_clone.rs::split_clone_ids`——带 `ALMIDE_RT_` 前缀的名称现在总是 clone；合成跨模块 Var 通过 `.clone()` 实体化，不会从 LazyLock 移出值。
- `canonicalize/registration.rs::infer_literal_type` 现在递归处理 `Record` / `List` / `Tuple` / `MapLiteral` / `Some` / `None` / `Ok` / `Err` / `Paren`，在注册时返回具体类型。`Checker` 还新增 `current_module_prefix`，由 `infer_module` 设置，使 `TopLet` 写回同时命中带前缀（`util.ANON`）和不带前缀（`ANON`）的键，不受模块检查顺序影响。

测试的全部十一个类别在 Rust 和 WASM 上均可用：Int、Float、Bool、String、List、Map、Set、Tuple、Option、Result、名义记录、匿名记录、变体——无论有无类型标注。

<a id="wasm-dispatch-hardening"></a>

### WASM 分派加固

- **Mono `Discover` 模块名保护**——同一函数名存在于多个模块（`option.filter[A]`、`list.filter[A]`、`result.filter[A, E]`）时，第一个名称匹配会静默夺走特化，将其注册到错误的 `(module, fn, suffix)` 键下。随后 `Rewriter` 查询失败，调用位置保留为 `Module { m, f }`，触发 WASM ICE。`Discover` 现在像 `Rewriter` 一样按模块名过滤。`stdlib/result.almd` 的 thunk 变通方案 `filter_with(r, pred, () -> E)` 恢复为规格形式 `filter[A, E](r, pred, err_val: E)`。
- **`emit_<module>_call` 的 Named 分派回退**——`emit_wasm/calls.rs` 的 Module 分支现在在 panic 前尝试 `func_map["almide_rt_<m>_<f>"]`，与未知模块 `_ =>` 分支的回退链一致。应用于 `int / float / string / option / result / list / bytes /
  matrix / base64 / hex / map / math / set`（12 个模块）。
- **`error.message` / `error.context` ICE**——`dispatch_runtime_fallback` 新增 `"error"` 分支，通过 `CallTarget::Module` 重新进入 `emit_call`。Module 侧已经存在内联生成逻辑；RuntimeCall 路径只需重定向。

<a id="matrix-wasm-ops"></a>

### Matrix WASM 操作

三个原本仅由 Rust 运行时实现、在 WASM 目标上会产生 `[ICE] emit_wasm: RuntimeCall almide_rt_matrix_<op>` 的函数，现在具有内联 WASM 生成分支：

- `rms_norm_rows(m, gamma, eps)`——不含 mean/beta 的 layer-norm 变体。
- `attention_weights(q, kt, scale)`——融合的 `softmax_rows(scale * q×kt)`。第 1 阶段暂存局部变量（`q`、`kt`、`scale`、`k`、`acc`、`inner`）在阶段之间释放，使第 2 阶段（`row_off`、`maxv`、`sumv`、`v`）复用相同 WASM 局部槽。
- `swiglu_gate(x, w_gate, w_up)`——`out[i,j] = g · σ(g) · u`，两个点积共享同一个 `k` 循环。

将 `ScratchAllocator` 每函数的 i32 上限从 32 → 48，使嵌套矩阵流水线（SDPA、Llama 1-block）能够容纳。

<a id="new-ci-gate-wasm-dispatch-coverage"></a>

### 新 CI 检查：WASM 分派覆盖

`tests/wasm_dispatch_coverage_test.rs` 运行 `almide test spec/stdlib/ --target wasm`，解析摘要并断言 `failed == 0 && skipped <= SKIP_BASELINE`。基线为 3——仅限合理的 `// wasm:skip` 文件（仅原生的 `process` API，以及需要捕获 panic 的 `testing.assert_throws`）。

<a id="numbers"></a>

### 数字

| 范围 | 之前 | 之后 |
|---|---|---|
| WASM spec | 214 通过 / 7 跳过 | **218 通过 / 3 跳过** |
| Rust spec | 221/221 | 221/221（未变） |
| 跨模块 `let` | 仅标量，引用有缺陷 | 与 Swift/TS 完全对等 |
| `stdlib/` WASM 清查中的 ICE 次数 | 4 | **0** |

<a id="s2-flip--concretizetypes-postcondition-is-now-a-hard-contract"></a>

### S2 切换 — ConcretizeTypes 后置条件成为硬契约

按契约，`expr.ty` 可信。Pipeline 框架（`crates/almide-codegen/src/pass.rs`）不再读取 `ALMIDE_CHECK_IR` / `ALMIDE_VERIFY_IR`；pass 之间的 IR 验证与每个 pass 的 `Postcondition` 列表在每次构建时运行。调试构建违反时 panic（确保 CI 和本地 `cargo test` 不会放行带未解析类型的程序）；发布构建把相同的 `[POSTCONDITION VIOLATION]` / `[IR CHECK]` 诊断打印到 stderr 后继续编译，使最终用户的 `almide build` 不因编译器缺陷崩溃。

切换后，下游 pass（闭包转换、WASM 生成、标准库分派、`resolve_list_elem` 等）可无条件依赖非 Unknown 的 `IrExpr.ty`。在 v0.14.6–0.14.7 添加的“若类型仍为 Unknown，则尝试生成期回退 X”防御性适配处理，可在后续工作中删除。

`ClosureConversion` 产生的 WASM 目标提升后 lambda TypeVar 残留，仍是已有的 pass 边界问题，由 S3（`pass_resolve_calls` 第 1b-c 阶段）跟踪；默认 spec/ 清查不会暴露它们。

移除的公开功能：
- `ALMIDE_CHECK_IR` / `ALMIDE_VERIFY_IR` 环境变量（无替代——契约始终实施）。

<a id="s3-phase-1d--delete-emit_stub_call--emit_stub_call_named-helpers"></a>

### S3 第 1d 阶段 — 删除 `emit_stub_call` / `emit_stub_call_named` 辅助函数

`emit_wasm/calls.rs` 中这两个辅助函数，仅用于在到达标准库分派器的 `_` 分支时以 `[ICE]` 消息执行 `panic!`。每个调用位置都已替换为内联 `panic!("[ICE] emit_wasm: no WASM dispatch for `<m>.<func>` — ...")`，携带具体模块 / 分派器名称，使诊断直接指向缺失分支。由于没有调用者，辅助函数本身也已删除——`calls.rs`、`calls_io.rs`、`calls_value.rs`、`calls_http.rs`、`calls_datetime.rs`、`calls_process.rs`、`calls_fs.rs`、`calls_env.rs`、`calls_random.rs`、`calls_regex.rs`、`calls_list_helpers.rs` 现在全部使用内联 panic。

行为未变：在 S2 切换之后，到达分派器回退本就已是编译期 ICE。此清理去掉一层间接调用，并在错误消息中显示正确的模块 / 分派器。

## [0.14.8] — 2026-04-17

修复 v0.14.7 回归的热补丁：外部包分派（例如从 `almide.toml` 的 `[dependencies]` 加载的 `almai`）无法链接，因为第 1b 阶段的 `ResolveCallsPass` 将所有 `CallTarget::Module`——包括用户包模块——改写为无版本的修饰名称。walker 随后以带版本标识符（`almide_rt_<pkg>_v<major>_<fn>`）生成这些函数，而调用位置引用无版本形式（`almide_rt_<pkg>_<fn>`），在编译期产生 Rust E0425。

<a id="fix"></a>

### 修复

`ResolveCallsPass` 现在以 `stdlib_info::is_any_stdlib(m)` 限制 Module → Named 改写。仅改写从不带 `versioned_name` 的内置 Almide 标准库模块（`args`、`path`、`list`、…）。用户包调用保持为 `CallTarget::Module`，继续通过 `StdlibLoweringPass::rewrite_module_names`（Rust）或 WASM `func_map` 的裸名称回退；两者都知道如何查询带版本符号。

无公开功能变化。`almide test`（206 个文件）和 `cargo test` 通过；下游 `almide-dojo`（使用 `almai.defaults` / `almai.with_system` / `almai.call_with` 的 13 项测试）再次能够编译并运行。

## [0.14.7] — 2026-04-17

第 3 阶段“理想形式迁移”系列。六个交付点（`-phase3.1` 至 `-phase3.5`，加上中间的 B 修补）合并到本次发布。共同目标是将内置 Almide / 代码生成分派中补丁层的特殊情况降至零。本次发布后，每个 `CallTarget::Module` 要么解析为 TOML 标准库函数（按目标内联生成），要么改写为 `Named`（内置 Almide 路径）；未解析标准库调用成为编译期 ICE；单态化在 pass 后移除全部泛型原始函数；审计通过函数定位信息捕获残留。计划及收尾说明见 `docs/roadmap/done/bundled-almide-ideal-form.md` 和 `docs/roadmap/active/codegen-ideal-form.md §Phase 3 Arc`。

<a id="patch-layer-status-at-release"></a>

### 发布时的补丁层状态

- 内置 `option.almd` / `result.almd`（签名覆盖）：**已移除**（S1）
- WASM `func_map` 对内置函数的逐模块回退：**已移除**（A）
- mono 剪枝阶段的 `is_bundled_module` 过滤：**已移除**（S4）
- 跳过剪枝的 `monomorphize_module_fns` 提前返回：**已移除**（B）
- `emit_stub_call*` 运行时 trap：**已移除**（S3，现在是编译期 ICE）

<a id="s1--optionresult-signature-normalization"></a>

### S1 — Option/Result 签名规范化

移除了静默覆盖 `option.unwrap_or_else` 和 `option.or_else` TOML 签名的内置 `stdlib/option.almd` / `stdlib/result.almd`。根因是 TOML 声明 `Fn[Unit] -> X`，而调用者写 `() => x`，现已从源头修复：TOML 使用 `Fn[] -> X`，`stdlib_codegen.rs` TOML 解析器处理空参数情况。

公开功能变化：

- `stdlib/defs/option.toml` 的 `unwrap_or_else.f` / `or_else.f`：`Fn[Unit] -> X` → `Fn[] -> X`
- `stdlib/option.almd` / `stdlib/result.almd`：删除（不再需要签名覆盖；运行时分派一直基于 TOML）
- `BUNDLED_MODULES` / `AUTO_IMPORT_BUNDLED` / `get_bundled_source`：移除 `option` / `result` 条目。Tier-1 自动导入继续使用 `import_table.rs` 中的硬编码列表。

调用者可见行为无破坏性变化：`option.or_else(o, () => ...)` 现在直接针对 TOML 签名进行类型检查，不再通过内置覆盖。检查该相互依赖关系的 `spec/stdlib/coverage_misc_test.almd` 无需修改即可通过。

<a id="s2--concretizetypes-audit-always-on-bundled-stdlib-generic-cleanup"></a>

### S2 — ConcretizeTypes 审计常开；内置标准库泛型清理

`ConcretizeTypesPass::postconditions` 不再以 `ALMIDE_AUDIT_TYPES=1` 控制审计；`Custom(audit_remaining_unresolved)` 检查在每次构建中运行。违反时打印 `[POSTCONDITION VIOLATION] [ConcretizeTypes] N expressions remain ...`，在 `ALMIDE_CHECK_IR=1` 下升级为 `panic!`。

Rust 目标上的 `spec/` 在 `ALMIDE_CHECK_IR=1` 下无问题。WASM 目标在 `ALMIDE_CHECK_IR=1` 下仍会被 `ClosureConversion` 产生的提升后 lambda TypeVar 残留绊住；关闭该缺口是 S3 的工作。默认行为（不设 `ALMIDE_CHECK_IR`）未变——两个目标都像以前一样通过 spec/。

内置标准库 mono 清理：`monomorphize_module_fns` 以前在 `program.modules` 中保留未使用的泛型原始函数，使其带着完整 TypeVar 到达 WASM 发射器。现在在特化轮次后移除 `is_bundled_module(name)` 内每个泛型函数——特化实例同时位于 `module.functions`，不再需要泛型原始函数。

<a id="s3--wasm-emit_stub_call-panics-at-compile-time"></a>

### S3 — WASM `emit_stub_call*` 在编译期 panic

`emit_stub_call_named` 和 `emit_stub_call` 不再丢弃参数并生成运行时 `unreachable` 指令，而是以 `[ICE]` 前缀执行 `panic!()`。在 WASM 生成时到达其中任意一个，意味着 `module.func` 调用在没有 TOML 或内置 IR 目标的情况下通过了 `pass_resolve_calls`——这是应在名称解析器中修复的编译器缺陷，而不是待调试的运行时 trap。

移除 `ALMIDE_WASM_STUB_PANIC` / `ALMIDE_WASM_STUB_VERBOSE` / `ALMIDE_WASM_STUB_TRACE` 环境变量；v0.14.6 的 stub-panic 清查已证明 spec/ + nn 不会到达 stub。此步骤关闭 codegen-ideal-form 路线图第 1c 阶段。

<a id="s4--mono-drops-generic-source-fns-from-every-module-not-just-bundled"></a>

### S4 — Mono 从所有模块移除泛型原始函数，而不只是内置模块

`monomorphize_module_fns` 原本就会在 `program.modules` 全部模块中发现并特化泛型函数（v0.14.6 引入的仅内置过滤，仅应用于特化后剪枝步骤）。现在所有模块采用统一剪枝：特化轮次后移除每个泛型原始函数，不再只移除 `is_bundled_module(...)` 中的函数。带泛型函数的用户包模块（例如 `pkg.helper[T](x: T) -> List[T]`）获得与内置标准库模块相同的单态化后不变量。

<a id="b--mono-prune-always-runs-concretizetypes-audit-is-more-locatable"></a>

### B — Mono 剪枝始终运行；ConcretizeTypes 审计更容易定位

`monomorphize_module_fns` 以前在未发现泛型特化时提前返回。因此，恰恰在最重要的情况下会跳过循环后的剪枝：程序导入内置标准库模块，却从不调用其中任何泛型函数。未使用的泛型原始函数存活到代码生成阶段，在函数体携带 `TypeVar(T)`，触发 WASM 的 `ConcretizeTypes` 审计。修复方式是始终运行剪枝；仅条件式跳过改写循环（`rename` 为空时本就无操作）。

`audit_remaining_unresolved`（`Custom` 后置条件）现在报告每个违规表达式所在的函数名称及简短 `kind` 标签（“[list::iterate] List ty=...”），而不是不透明的 `Discriminant(NN)` 数字。

修复后，WASM 的 spec/ 在 `ALMIDE_CHECK_IR=1` 下为 **191/206 通过，15 跳过**。剩余 15 项是独立的类型推断缺口（空列表 `Applied(List, [Unknown])`、OpenRecord 传播、codec 派生列表字段、泛型 chain-b 参数等），由 `codegen-ideal-form.md §#4` 跟踪。默认行为（不设 `ALMIDE_CHECK_IR`）未变——两个目标仍像以前一样通过 spec/。

<a id="a--phase-1b-resolvecalls-rewrites-bundled-stdlib-calls-to-named"></a>

### A — 第 1b 阶段：ResolveCalls 将内置标准库调用改写为 `Named`

`pass_resolve_calls.rs` 不再仅做验证。现在对每个 `CallTarget::Module { module, func }` 执行：

- TOML 标准库（例如 `list.map`、`option.unwrap_or_else`）：保持 `Module { module, func }`，使按目标分派器应用参数修饰 / 内联生成（Rust 的 `pass_stdlib_lowering`、WASM 的 `emit_call`）。
- 内置 Almide 标准库（例如在 `stdlib/list.almd` 中定义、由 mono 特化的 `list.split_at`、`list.iterate`）：改写为 `CallTarget::Named { name: "almide_rt_<m>_<f>" }`，即代码生成注册的修饰符号。两个后端已按该名称注册内置函数，无需额外分派逻辑。
- 既不是 TOML 也不是内置 IR 函数：后置条件违反——此前延后为运行时 trap 的未解析标准库缺口，现在在 `ALMIDE_CHECK_IR=1` 下成为编译期 ICE。

移除 v0.14.6 作为补丁添加的 WASM `_ if module == "list"` 分支的内置函数回退（`func_map.get("almide_rt_list_*")`）。有了上述改写，内置函数根本不会到达 Module 分派分支，因此该回退已是死代码。

将 `bundled-almide-ideal-form.md` 移至 `done/`——所列全部 5 项已关闭。

## [0.14.6] — 2026-04-16

“LLM-first 语言”路线图第 2 阶段。**重点**：让编译器在诊断中生成可直接复制粘贴的修复片段，使 LLM 重试更快收敛。使用 [almide-dojo] 的 30 任务基准测量。

<a id="llm-writability-dojo-msr-2026-04-16"></a>

### LLM 编写便利性（dojo MSR，2026-04-16）

| 模型 | v0.14.5 基线 | 0.14.6 | Δ |
|---|---|---|---|
| Sonnet 4.6 | — | **30/30 (100%)** | — |
| llama-3.3-70b | 17/30 (57%) | **23/30 (77%)** | **+6 (+20pt)** |
| llama-3.1-8b | 13/30 (43%) | 10/30 (33%) | -3（在噪声带内，σ≈2） |

Sonnet 30/30 验证了核心设计：达到 SOTA 无需在语言上让步（UFCS、命令式循环）。完整分析见 [docs/roadmap/active/llm-first-language.md](../../roadmap/active/llm-first-language.md)。

<a id="added--diagnostics"></a>

### 新增 — 诊断

下列每项都附带含可复制粘贴代码的 `try:` 块：

- **E001 函数体 Unit 泄出**：从 AST 提取尾部 `let`/`var` 名称，使片段显示 `let <real_name> = ... / <real_name>`，而非 `<computation>` 占位符。
- **E001 if 分支 Unit 泄出**：若一个分支是裸赋值，片段使用实际变量（`let new_val = if cond then … else val`）。
- **E002 方法调用语法**：`x.to_upper()` → `string.to_upper(x)`——仅在有模糊匹配建议时生成片段，无候选时不生成。
- **E002 未定义函数**：模糊匹配建议 + 使用正确名称的修复片段。自由文本别名（例如 `xs + [x]`）会正确抑制片段，避免将非调用代码块粘贴到调用位置。
- **E003 未定义变量**：对可建议导入的标准库模块建议 `import json`，否则建议模糊匹配的重命名。
- **E004 参数数量不匹配**：片段显示带 `<name: Type>` 占位符的完整签名（`string.join(<list: List[String]>, <sep: String>)`）。
- **E009 let 重新赋值**：使用实际绑定名称建议 `var <name> = ...`。
- **针对幻觉的片段**：
  - `int.sqrt(n)` → 转换—平方根—转换（`float.sqrt(int.to_float(n))`）。
  - `int.gt(a, b)` / `int.lt` / `int.gte` / `int.lte` / `int.eq` / `int.neq`（及 float/string/bool 变体）→ 运算符映射表。
- **位置错误的 `test` 关键字**：提示指出两种可能原因（前面的声明未闭合，或测试框架提交的代码不应包含测试）。
- **rest/cons 模式**：`[head, ...tail]` / `[h, ..t]` / `head :: tail` 生成针对性提示，指向 Almide 中唯一惯用形式——`list.first` / `list.drop` 递归。
- **`while cond do ... done`**（Pascal/Ruby/OCaml 循环形式）检测现在提供更丰富的 `try:` 片段，同时给出递归形式（pure/effect 函数优先）和 Almide 的 `while cond { ... }` 形式（用于 `var` 累加器）。动机来自 dojo 二分查找 / 矩阵操作失败：`do ... done` 一直是首次尝试，而递归形式在重试中胜出。

<a id="added--tooling-2"></a>

### 新增 — 工具

- **`almide ide outline <file|@stdlib/<module>>`**——每项声明一行摘要（fn / type / let），旨在替代 LLM API 探索中的 `grep`。
- **`almide ide doc <symbol> [--file <f>]`**——标准库或用户符号的签名 + docstring。`string.to_upper` / `greet` 统一可用。
- **`almide ide stdlib-snapshot [--modules m1,m2,...] [--json]`**——拼接核心标准库 outline 的文本 / JSON。一个子进程替代 N 个。设计供框架嵌入 SYSTEM_PROMPT；默认 7 个模块测得文本约 3.5K token（JSON 14.5K）。
- `almide ide outline` 上供仪表板 / 自动化使用的 **`--json` 标志**。
- **快照测试**固定 `almide ide outline @stdlib/*` 和 `stdlib-snapshot` 输出格式，使标准库变化时下游框架嵌入的 SYSTEM_PROMPT 不会静默失效。

<a id="fixed--parser"></a>

### 修复 — 解析器

- **跨换行的 let-in 检测**：`let x = expr\n  in <body>` 现在触发 OCaml/Haskell 提示，而非连锁产生“Expected expression (got In 'in')”。部分 `Stmt::Let` 保留于 AST，使下游诊断（E001）仍能引用实际绑定名。
- **rustc 错误码泄出的包装**：rustc 的 4 位 `error[E\d{4}]` 代码（Almide 自身不生成，Almide 最高为 3 位 E001..E099）现在始终包裹在缺陷报告横幅内，即使输出不含 `src/main.rs`。避免框架分类器将代码生成缺陷误认为面向用户的语言错误。

<a id="changed-1"></a>

### 变更

- `almide ide outline` 的 `--filter` 现在匹配子字符串（文档曾写“前缀”，但实现一直是子字符串——现在文档与行为一致）。

<a id="added--phase-3-mvp"></a>

### 新增 — 第 3 阶段 MVP

- **`almide ide doc`** 接受 `@stdlib/<module>.<fn>` 前缀作为 `<module>.<fn>` 的别名，与 `almide ide outline @stdlib/<module>` 在使用方式上对称。
- **`almide fix` 退出码**：自动修复后文件无问题时返回 0，`manual_pending` 非空时返回 1。框架可按退出码控制重试，无需解析输出。
- **丰富诊断解释文档**（`docs/diagnostics/E001.md` 至 `E013.md` 全部）：每篇现在都包含编译器实际生成的 `try:` 片段形式、常见类型不匹配的转换表，以及 `llms.txt` / `CHEATSHEET.md` 交叉引用。若干文档内容不匹配（E010 描述为“scope error”，但代码含义是**非穷尽 match**；E011 描述为“init issue”，但含义是 **pure 函数中闭包内的可变 var**）；现已修正。新增 E012（重复定义）和 E013（对非记录访问字段）——检查器曾生成两者，但没有文档。
- **`almide fix <file> [--dry-run] [--json]`**——应用 `auto_imports`（添加缺失的 `import json` / `import fs` 等）、移除 OCaml 风格 `let x = expr in <body>` 的关键字（保留本体）、改写比较函数调用（`int.gt(n, 0)` → `n > 0`；int / float / string / bool 上 lt / gte / lte / eq / neq 同理）、**移除 `return` 关键字**（Go/Rust/JS 习惯——Almide 使用末尾表达式；多处出现时迭代到不动点），并将剩余带 `try:` 片段的诊断作为手动修复指引报告。`--json` 生成稳定 schema 报告（`schema_version`、imports_added、letin_removed、operator_rewrites、**return_removed**、manual_pending、changed、dry_run），用于 LLM 框架重试循环集成。cons 模式改写仍需手动处理（需要 AST 级模式变换，以及解析器恢复已丢失本体）。
- **`list.binary_search(List[Int], Int) -> Option[Int]`**——有序列表二分查找。dojo 二分查找任务此前在 70b 上失败；此接口将其简化为一次 API 调用。
- **`string.run_length_encode(String) -> List[(String, Int)]`**——RLE 配对，动机相同。
- 仓库根目录的 **`llms.txt`**——使命、CLI 参考、核心惯用法、标准库入口、诊断代码、反模式。供 LLM 工具通过单个 URL 获取。

<a id="known-gaps-documented-not-blockers"></a>

### 已知缺口（已记录，非阻碍项）

- 通过 `stdlib/<m>.almd` 内置源码扩展现有 `list.*` / `string.*` 模块目前不可用：`list.*` 降低被硬编码为生成 `almide_rt_list_<fn>`，无论函数来自 TOML 还是 Almide 源码。变通方法是以 TOML + 运行时添加函数（`stdlib/defs/<m>.toml` + `runtime/rs/src/<m>.rs`）。完整 Almide 源码分派属于第 3-2.2 阶段。
- `almide fix` 尚未机械应用 `let-in` / `head :: tail` / 运算符形式改写——显示 try: 片段，但需要手动编辑。第 3-1.2 阶段。
- `llms.txt` 为手写，尚未从规范文档（SPEC / cheatsheet）自动生成。第 3-3.2 阶段。

<a id="added--almide-docs-gen---check-doc-drift-guard"></a>

### 新增 — `almide docs-gen --check`（文档偏差防护）

检查 `llms.txt` 和 `docs/diagnostics/` 是否跟随其权威来源的一致性检查。MVP 覆盖四个维度：

- **版本**：`Cargo.toml` 版本字符串必须出现在 `llms.txt` 中。
- **llms.txt 引用的诊断代码**：`docs/diagnostics/` 下每个 `EXXX` 文件都必须列于 `llms.txt`。
- **自动导入的标准库**：`almide_lang::stdlib_info::AUTO_IMPORT_BUNDLED` 中每个模块都必须出现在 `llms.txt` 的“Fast facts”中。
- **诊断注册表双射**：编译器源码中每个 `with_code("EXXX")` 必须有对应的 `docs/diagnostics/EXXX.md`，每篇文档也必须对应实际生成的代码。

失败时以项目列表报告偏差，退出码为 1。`cargo test` 集成测试 `docs_gen_check_passes_on_clean_checkout` 使每个修改权威来源却遗漏文档的 PR 在 CI 中失败。

完整生成（而不仅是偏差检查）的范围在 `docs/roadmap/active/llms-txt-autogen.md`；注册表与生成操作的统一策略在 `docs/roadmap/active/diagnostic-emit-doc-unification.md`。

首次运行发现并修复的实际偏差：
- `llms.txt` 的 `E010-E013` 范围行并未将 `E011` / `E012` 包含为子字符串（用范围压缩冒充内容），现已展开。
- 编译器生成 `E420`（函数可见性违反），却没有文档。新增 `docs/diagnostics/E420.md`，并注明该编号不连续，是未来版本重新编号的候选。

<a id="added--bundled-almide-stdlib-dispatch-infrastructure"></a>

### 新增 — 内置 Almide 标准库分派（基础设施）

`stdlib/<module>.almd` 文件现在可用 Almide 编写的新函数扩展基于 TOML 的标准库模块。此前会静默失败：代码生成对每个 `module.func` 调用都生成 `almide_rt_<m>_<f>`，使内置 `fn binary_search_v2` 最终调用不存在的运行时函数。

修复分四层：

1. **前端解析**（`src/main.rs`）：即使 `is_stdlib_module(name) == true`，也将内置标准库模块降低为 IR。仅 TOML 标准库仍会提前短路。
2. **TOML 重复项剪枝**（`src/main.rs`）：移除与 TOML 运行时名称冲突的内置函数——这些走 rt_ 路径，仅内置的函数保留。
3. **IR 验证**（`almide-ir`）：在 `known_module_functions` 中跳过内置标准库模块，使内置函数体对 TOML 函数的调用（例如 `result.collect` 调用 `list.is_empty`）不会报“unknown function”。
4. **代码生成分派**（`pass_stdlib_lowering`）：为每个 pass 构建仅内置 `(module, func)` 对的注册表；对它们抑制 `Module → almide_rt_*` 改写，调用保留为 `Module`，使 walker 生成普通用户函数调用。

原有内置 `option`/`result` 的 `.almd` 源码被发现承担隐藏角色：代码生成将其剪枝（运行时分派优先 TOML），但类型检查器读取它们，且**优先使用其签名而非 TOML 签名**——因此 `option.or_else(o, () => ...)` 针对内置的 `fn() -> X` 而非 TOML 的 `Fn[Unit] -> X` 进行类型检查。移除它们会破坏测试套件。统一路线见 `roadmap/active/option-result-bundled-cleanup.md`。

<a id="added--bundled-list-fns-real-users-of-the-dispatch-path"></a>

### 新增 — 内置 `list.*` 函数（分派路径的实际使用者）

`stdlib/list.almd` 提供三个函数，由 `spec/stdlib/list_bundled_test.almd` 在 Rust 和 WASM 两个目标上覆盖：

- `list.bundled_probe(n)`——冒烟回归防护。
- `list.split_at(xs, n) -> (List[T], List[T])`——在索引 `n` 处分割列表。演示内置函数调用现有 TOML 函数（`list.take`、`list.drop`）。
- `list.iterate(seed, f, n) -> List[T]`——构建长度 `n` 的 `[seed, f(seed), f(f(seed)), ...]`。通过内置分派路径端到端执行纯 Almide 递归。

<a id="changed--monomorphize-extended-to-module-defined-generics"></a>

### 变更 — `monomorphize` 扩展到模块内定义的泛型

IR 模块中声明的泛型函数（例如 `stdlib/list.almd` 的 `list.split_at[T]`）现在也由单态化 pass 特化，而不再仅限顶层 `program.functions`。调用目标保持 `CallTarget::Module { module, func }`，使各后端代码生成继续通过同一标准库分派。这关闭了 WASM 缺口：`list.split_at([1, 2], 2)` 此前带着 `TypeVar` 到达发射器，并回退到 i32 元素尺寸。特化函数体内的自递归 `Named` 调用由 `specialize_function` 本身改写；顶层 rewrite_calls 仍是顶层函数的权威处理点。

路线图：剩余技术债（统一分派入口、淘汰 `stub_call → unreachable`、`ConcretizeTypes` 硬后置条件、option/result 签名规范化）见 `active/bundled-almide-ideal-form.md`。

预期无 MSR 变化（仅基础设施）；下游工作（`diagnostic-snippet-externalization`、Almide 自动改写规则）已解除阻碍。

<a id="internal-refactors-no-behavior-change-no-msr-effect"></a>

### 内部重构（无行为变化，无 MSR 影响）

- **`almide fix` 关键字移除规则**合并为单一 `KeywordRemoval { keyword, diag_matches, max_iter }` 引擎。`let-in` 和 `return` 规则改为数据驱动；新增第三种关键字删除规则只需一个 const 条目及一个调用位置。从原有两份副本中提取 `word_boundary_ok`。
- **比较运算符表**（`int.gt` / `.lt` / `.eq` 等 → `>` / `<` / `==`）统一到 `almide::stdlib::comparison_operator_of`。此前同一映射在 `suggest_alias`、`try_snippet_for_alias`、`cli/fix.rs::comparison_fn_to_operator` 中重复；现在各处均由同一权威函数派生。附带结果是 `string.eq` 和 `bool.eq` 现在也获得“Did you mean”提示（此前 `suggest_alias` 缺失，这是统一时发现的缺口）。
- **while-do `try:` 片段**由原先 Option A/B 块（≈15 行）缩短为 7 行：一个简洁 `while` 形式 + 一个 `fn loop` 递归骨架。复盘反馈认为，更长形式可能造成选择悖论并使重试上下文膨胀。

<a id="deferred-evidence-based"></a>

### 延后（基于证据）

- `almide ide peek-def` / `find-refs`：dojo 上下文未使用“检查现有函数体”工作流，因此预期不会提升 MSR。任务库增加重构类任务时再评估。
- UFCS 采用：dojo 数据显示 UFCS 仅对较弱模型有利（8b parse-err 9，而 70b parse-err 0）——Sonnet 在现有语法上 30/30 证明强模型不需要它。[docs/roadmap/active/llm-first-language.md](../../roadmap/active/llm-first-language.md) 中的 `Path A` 现已正式否决。

---

<a id="before-this-file-existed"></a>

## 本文件创建之前

- **v0.14.5**——LLM-first 路线图的基线（dojo 70b 17/30，8b 13/30）。
- **v0.14.x 及更早版本**——参见 git 历史；带标签版本的发布说明位于 GitHub Releases。

[almide-dojo]: https://github.com/almide/almide-dojo
