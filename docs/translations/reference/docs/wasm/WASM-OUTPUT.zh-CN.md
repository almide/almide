<a id="wasm-output--whats-in-the-binary-and-why"></a>

# WASM 输出——二进制中有什么，为什么

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](WASM-OUTPUT.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](WASM-OUTPUT.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](WASM-OUTPUT.zh-CN.md)

> [正本](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/wasm/WASM-OUTPUT.md)的非规范历史参考快照，固定提交：35297a612fb29e3f78f6587aba253aa91d51f466。不自动同步。未在此独立重新验证安全、证明、测量或实现主张，也未运行验证命令。以正本为准。正文中的“认证”指所述证书与检查器机制，不表示第三方安全认证。

Almide **直接**输出 WebAssembly——不用 LLVM、Cranelift、wasm-bindgen，模块中也没有已编译的标准库目标代码。唯一的 renderer 是 **structural 路径**（crates/almide-wasm、wasm-encoder、`--target wasm` 路径）。**原有 v1 路径**（带证书的 MIR→WAT renderer）在 #2752 失去最后一条路由，并于 #2761 删除；下文提及它的测量都是历史记录，无法在当前编译器上复现。本文逐字节剖析真实模块，并准确说明大小主张的含义。

代表性的 Hello, world 字节数来自 CI：`docs/benchmarks/wasm-size.txt`（由 `scripts/gen-readme-stats.sh` 重新测量，自 #1605 起受关卡约束）。以下其余内容于 2026-08-27 使用当时的 `develop` 编译器、`wasm-opt`（Binaryen）132 和 `wasm-objdump`（WABT）1.0.41 测量；Rust 比较行保留 2026-07-23 的测量（`rustc 1.94.1`）。可用文末命令复现各数值。

<a id="the-headline-numbers-measured-2026-08-27-the-incumbent-columns-are-historical-retired-by-2761"></a>

## 代表性数字（2026-08-27 测量；原有路径的列为历史记录，已由 #2761 退役）

| 程序 | structural（已发布） | structural + `-Oz` | 原有 v1（已退役） | 原有 + `-Oz`（已退役） |
|---|---:|---:|---:|---:|
| Hello, world | 4,459 B | **364 B** | 1,096 B | 788 B |
| FizzBuzz 1–100 | 4,716 B | **1,162 B** | 2,168 B | 1,346 B |
| Fibonacci（递归） | 4,490 B | **894 B** | 1,791 B | 1,035 B |
| 递归泛型 ADT repr¹ | **14,088 B** | **9,320 B** | 34,723 B | 21,883 B |

¹ `spec/wasm_cross/compound_repr_recursive_interp.almd`——递归 ADT、互递归记录、泛型实例化，以及它们完整的 `${…}` repr 机制。

**用一段话解释反转。** structural 路径携带固定约 3.5 KB 的运行时前导部分（allocator 家族、COW 关卡、itoa scratch、OOM 路径——Hello, world 模块中有 38 个支持函数），因此近乎空的程序，其原始模块比原有路径大。在真实程序上关系反转：ADT-repr 行为 structural 14.1 KB、原有 34.7 KB；在完整的 600 多 fixture 语料上，greenfield VERDICT 记录 **structural 合计 4.11 MB，原有 10.54 MB**。经 `wasm-opt -Oz` 后，在所有测量行中 structural 模块都是四列最小：前导部分静态可达（表驱动 allocator，模块自身调用图大多从未进入），`-Oz` 的全模块 DCE 删除 renderer 内到达性 pass 必须保守保留的部分。Hello, world 从 4,459 降到 364 字节，只剩 **1 个**函数。

两种诚实的表述方式：

- 7 月的 Rust 比较仍可作为背景：Rust `wasm32-wasip1` Hello, world 默认 64,430 B，完整大小优化 profile 下 40,754 B（`opt-level="z"`、`lto`、`strip`、`panic="abort"`、`codegen-units=1`，2026-07-23 测量）。差距是工具链基础开销的差异——Rust 为每个 `println!` 链接 `std` 格式化机制——不是语言质量评价。
- **交付的** Almide 二进制是经过验证的版本（见下文）。`wasm-opt` 列要求显式选择 `--wasm-opt`，这会使模块离开已验证范围。

<a id="section-anatomy--hello-world-structural-incumbent-row-historical"></a>

## 节结构——Hello, world（structural；原有路径的相关行是历史记录）

structural 路径的交付形态（4,459 B，来自 `wasm-objdump -h`）：

| 节 | 大小 | 内容 |
|---|---:|---|
| Type | 113 B | 19 个签名 |
| Import | 179 B | 5 个 WASI import（`fd_write`、`proc_exit`、`random_get`、`clock_time_get`、`fd_read`） |
| Function | 40 B | 39 个函数的索引表 |
| Table + Elem | 6 B | funcref 表（此处为空，没有闭包） |
| Memory | 3 B | 一个线性内存 |
| Global | 97 B | 14 个 global（堆游标、空闲链表头、park-buffer 状态等） |
| Export | 35 B | `_start` + memory |
| Code | 3,694 B | 39 个函数体：`main` + 38 函数的前导部分（带大小类别的 allocator 家族、RC/COW 关卡、itoa、字符串相等性、WASI park-buffer 胶水） |
| Data | 261 B | 字符串池（`"Hello, world!"` + OOM/trap 消息） |

同一模块经 `wasm-opt -Oz` 后（364 B）：Type 12 B、Import 35 B（只剩 `fd_write`）、Function 2 B、Memory 3 B、Global 8 B、Export 35 B、Code 92 B（**1 个**函数）、Data 149 B。这就是 structural 前导部分的全部缘由：它是链接进来的支持代码，对这么小的程序，全模块优化器能证明其中几乎所有代码都是 dead。（“tier-0 preamble”想法——当 DCE 证明动态分配不可达时，在 renderer 内去掉 allocator 家族——可以不离开验证范围，就使交付的 4.4 KB 大幅接近 364 B；若落地，会在本文测量。）

原有 v1 路径（历史记录——#2761 退役；1,096 B）：Type 26 B、Import 70 B（2 个 WASI import）、Function 9 B、Memory 3 B、Global 38 B、Export 40 B、Code 739 B（8 个函数——`alloc`、`rc_dec`、`main`、`print_str`、`_start` 和三个 helper）、Data 46 B，另有 98 字节的 `name` custom section（仅函数名，local 已剥离；wasmtime trap 回溯会打印它们，值得这些字节）。原有路径比 7 月测量增加的 393 B（703 → 1,096 B），来自 C-320 系列落地的确定性计量和 stdin 接线。

<a id="where-the-stdlib-went"></a>

### stdlib 去了哪里

Almide stdlib 有 49 个模块、1310 个函数，但它们**在 Almide 中自举实现**，按需链接。编译器扫描 lowering 后的程序，查找被调用的 dispatch 名（`string.len`、`map.set`、`list.sort_by` 等），仅链接匹配的自举源码，迭代到不动点，使已链接函数自身的被调用者也跟进。Hello, world 链接 **零** 个 stdlib 函数；FizzBuzz 链接 `int.to_string` 背后的少数函数。未使用模块不贡献任何内容；即使拉入了某些内容，若最终不可达，也会在汇编前被到达性 DCE 清除。

<a id="why-the-value-model-stays-small"></a>

### 值模型为何保持小巧

- **统一为 i64 的 slot。** 每个标量为 i64；每个堆值是以 i32 handle 寻址、带长度前缀的块。无逐类型布局，无 metadata。
- **variant 是 `tag @ slot 0`。** `match` 编译为整数比较，无 vtable、无类型描述符。
- **单态化 → 直接调用。** 泛型在编译时特化；唯一的间接调用是闭包，通过单个 funcref 表执行。
- **两层到达性 DCE。** 按需 linker 决定哪些自举 stdlib 源码进入程序；renderer 的到达性 pass 随后在组装模块前丢弃未到达 helper、import 和 data segment。

<a id="whats-still-not-the-smallest-possible-module-and-why"></a>

## 哪些地方还不是最小模块，以及为什么

已验证流水线**交付自身渲染过程产生的字节**。默认路径构建的每个模块都经过输出时验证。structural 路径还记录证书 bundle；`almide verify` 和证书检查语料关卡检查所支持的 core 制品，而非 component。witness 覆盖范围和记录信任均有限，不是完整制品内存安全的证明（见 [内存安全](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/design/MEMORY-SAFETY.md)）。`wasm-opt` 是另一类东西：**应用于 renderer 完成输出的外部、未验证变换**——运行它会把流水线产生的字节替换为另一个未经证书认证的工具重写的字节。正是这条边界使它仍需显式选择：

```bash
almide build app.almd --target wasm --wasm-opt   # runs: wasm-opt -Oz
#   --enable-nontrapping-float-to-int --enable-tail-call --enable-bulk-memory
#   --enable-mutable-globals
#   (the features the structural leg actually emits — no SIMD in the default output;
#    bulk-memory is the structural leg's memory.copy and mutable-globals its
#    exported globals, #1616)
```

代码注释译文：运行 wasm-opt -Oz。启用的是 structural 路径实际输出的 feature；默认输出没有 SIMD；bulk-memory 是 structural 路径的 memory.copy，mutable-globals 是其导出的 global（#1616）。

**`-Oz` 用速度换取这些字节。** 原有 renderer（现已由 #2761 退役）曾将热循环分化为带 guard 的快速路径，提前消除边界检查；`-Oz` 的代码折叠将几乎相同的副本重新合为一个带检查的循环，在 spectralnorm 上测得约慢 3 倍。应将 `-Oz` 用于大小敏感的冷代码；应用于计算 kernel 前先做基准测试。

<a id="determinism-and-the-cross-target-contract"></a>

## 确定性与跨目标契约

输出字节**跨宿主架构确定**：原生构建（x86-64/aarch64）的编译器与构建为 wasm32（playground）的编译器，对跨目标 fixture 语料产生逐字节相同的模块——这是 CI 关卡（`scripts/check-host-determinism.sh`），不是愿望。每个能为两个目标编译的程序，在 native ⇄ wasm 上产生**逐字节一致的 stdout/stderr/exit code**，并在 [docs/contracts/](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/contracts) 按契约跟踪。

<a id="the-embedded-hosts-call-stack-budget-3435"></a>

### 嵌入式宿主的调用栈预算（#3435）

递归超出目标调用栈资源时，native 与嵌入式宿主都会以定义好的 abort `Error: stack overflow` + exit 1 结束（C-196、ALS-T6）；触发深度由各目标自身决定并予以声明，不强行统一。嵌入式上限的位置是交付路径的选择：`almide run --target wasm`、`almide
bench --target wasm` 和 `almide test` 的 wasm 路径背后的嵌入式宿主，将 wasmtime 的 `max_wasm_stack` 设为 **8 MiB**（`crates/almide-wasm-run/src/host.rs` 中的 `EMBEDDED_WASM_STACK`），与 native 主线程栈大小相同。它在具有 64 MiB 原生栈的宿主线程上运行每个 guest 和每个 `fan` worker instance，因为 wasmtime 要求宿主栈超过 wasm 预算加上 host frame。wasmtime 自身默认 512 KiB，宿主在 #3435 前一直采用此值。

2026-10-06 测量（macOS aarch64，release build；仍能返回答案的最深参数）：

| 递归形态 | native | 嵌入式，512 KiB | 嵌入式，8 MiB |
|---|---:|---:|---:|
| 返回 `T!` 的非尾递归树遍历（#3434 的 `infer`） | ~43,000 | ~5,400 | ~87,000 |
| 每帧四个堆局部值的递归 | ~32,500 | ~5,400 | ~87,000 |
| 树上的 `1 + depth(f)` | 无限制（LLVM 转成循环） | ~32,500 | ~523,000 |

标准运行时保留自己的限制和失败形式——残余 C-196 声明：`wasmtime run` 默认 512 KiB（`-W
max-wasm-stack=N` 可提高），耗尽时报告自身的 `wasm trap: call
stack exhausted`、exit 134；浏览器各自设置。

<a id="measuring-allocation-the-watermark-and-the-counter-2407"></a>

## 测量分配：watermark 与 counter（#2407）

每个 structural 路径模块都将 bump-heap 指针导出为 `__heap` global（region window 回退它时还导出 `__heap_high`）。嵌入式宿主在运行后读取它，`crates/almide-wasm/tests/alloc_ledger.rs` 将它按语料 fixture 固定在 `crates/almide-wasm/tests/golden/alloc-baseline.txt`——这就是 **watermark**。它不增加成本（模块本来就维护该 global），而且是**峰值**：行值为 `pool size + the highest the heap ever reached`。峰值不能显示 churn（反复分配与释放）。allocator 保留按大小分类的空闲链表，因此每次迭代分配并释放新块的循环，watermark 几乎与一次分配相同。

**counter** 才是测量这种现象的工具，是 native `ALMIDE_ALLOC_COUNT`（#2228）的 wasm 对应物。它是输出时开关：

```bash
ALMIDE_WASM_ALLOC_COUNT=1 almide run app.almd --target wasm
# stderr, after the program's own output:
# __ALMD_WASM_ALLOC allocs=2000 reused=1996 bytes=9780 frees=2000 heap_end=65872
```

代码注释译文：在程序自身输出之后，将 counter 行输出到 stderr。

| 字段 | 计数内容 |
|---|---|
| `allocs` | 每次 `$alloc` 调用（空闲链表命中或 bump） |
| `reused` | 由大小类别空闲链表 pop 满足的 `$alloc` 调用（其余推进 bump head） |
| `bytes` | 请求的 payload 长度总和 |
| `frees` | 每次 `$free` 调用（归入空闲链表，或因过小／过大而放弃） |
| `reclaimed` | region window restore 未经 `$free` 而整批收回的块（#1961） |
| `live` | `allocs − frees − reclaimed`：从未释放的堆块（见下文） |
| `heap_end` | 同一行中的 watermark，可将两者一起阅读 |

启用开关后，emitter 在 top-let global **之后**追加五个 `i64` global（现有索引不移动，堆布局一个字节也不变），由 `$alloc` / `$free` / region window restore 增加它们；宿主读取为 `__alloc_count`、`__alloc_reused`、`__alloc_bytes`、`__free_count`、`__region_reclaimed`。**默认关闭时，完全不输出这些内容**：模块与从未听说过该开关的构建逐字节相同，因此大小棘轮和 alloc 台账均不变，按证明转录的运行时树（`proofs/StructuralRuntime.v`）仍是交付版本。未开启计量的模块将 counter 报告为不存在，绝非零（`RunResult::alloc_count` 为 `None`）。

counter 在 watermark 旁记入台账：同一次 `ALMIDE_UPDATE_ALLOC=1` 重新生成会写入 `crates/almide-wasm/tests/golden/alloc-count-baseline.txt`（每个 fixture 为 `allocs reused
bytes frees`，`~` 表示校准排除，`!` 表示拒绝）；检查运行还要求启用计量的模块与未启用者一致——相同 stdout、相同 watermark——从而证明计量工具不扰动测量对象。

**退出时的 live。** Native 在每个值离开作用域时 drop；wasm 路径手动 release，因此遗漏 release 是输出一致性看不到的 wasm 特有缺陷（#2932、#2944）。`live` 是从未释放的块数。top-let global 按设计存活到退出，因此开启计量的 `main` 会在 epilogue 前释放每个可 drop 的 top-let——交付模块从不这样做——故正常退出后的 `live` 恰为泄漏块数。同一次更新运行写入 `crates/almide-wasm/tests/golden/live-at-exit-baseline.txt`：仅列出 `live` 不为 0 的 fixture，每个注明负责其机制的 issue，只允许缩减（未列出的 fixture 必须为 0；行值只可下降，下降时必须调低；新行在指定 issue 前带 `#?`，检查会拒绝它）。不以 0 退出的运行不予判定。生成式 fuzzer 对每个干净程序应用同样测量（`LeakAtExit` finding 类型，在 `fuzz-leak` 标签下有独立的每夜类别）。

**一起阅读两者。** watermark 行变化时，count 行会指出变化来自哪一边：更多 `allocs` / `bytes` 表示分配增加；计数相同但 watermark 更高，则是存活时间变化（持有更久，空闲链表无块可交还）。即使 watermark 完全不变，count 行仍可变化——这就是 churn。以下三个程序体现这种差异：

| 程序 | `allocs` | `reused` | `bytes` | watermark 增量 |
|---|---:|---:|---:|---:|
| 一个列表字面量（`let xs = [1, 2, 3]`） | 1 | 0 | 24 | +0（基准） |
| 同样内容再加第二个字面量 | 2 | 0 | 48 | +64 |
| 1000 ×（`"row " + int.to_string(i)`，每次迭代 drop） | 2000 | 1996 | 9780 | +48 |

重点是第三行：2000 次分配共 9,780 字节，却只使 watermark 增加 48 字节，因为其中 1,996 次由空闲链表满足。单看 watermark，这个循环与两次分配无法区分。语料在更大规模上也呈现同样形态（两个台账中的行，2026-09-23）：

| fixture | `allocs` | `reused` | 请求 `bytes` | `__heap` 行 |
|---|---:|---:|---:|---:|
| `variant_result_one_credit_churn` | 1,638,000 | 1,629,810 | 104,832,000 | 1,114,096 |
| `mut_param_alias_cow` | 8,170 | 8,049 | 72,057,178 | 209,056 |
| `grain_gc_shapes` | 5,059 | 5,035 | 16,030,437 | 131,360 |

每个请求的字节数都是 watermark 显示量的100 至 1000 倍，而即使 churn 翻倍，其 watermark 行也不会变化。计量工具覆盖 structural 路径，自 #2761 起也就是所有 wasm 程序。

<a id="js-host---host-js-2265"></a>

## JS 宿主（`--host js`，#2265）

`almide build app.almd --target wasm --host js -o dist/app.wasm` 在模块旁写入 `dist/app.js` 和 `dist/app.d.ts`——编译器已掌握编写宿主胶水所需的全部信息，所以制品无需手写 WASI stub、import object 或 String marshalling，即可在网页或 node 下运行。`app.js` 是不带依赖的 ES module：

- `init(source?, hooks?)` 编译并实例化。`source` 可省略（加载 `.js` 旁的 `.wasm`：node 下用 `fs.readFile`，网页中用 `fetch`），也可为 bytes、`Response`、两者之一的 promise，或已编译的 `WebAssembly.Module`。`hooks.stdout` / `hooks.stderr` 接收原始 `Uint8Array` chunk（默认：node 下为 `process.stdout`，网页中逐行 `console.log`）。
- 仅为已交付模块命名的 `wasi_snapshot_preview1` import 提供 shim（`fd_write`、`proc_exit` → 抛出 `AlmideExit`，以及 clock/random/read 基础功能）。不链接其他内容，因此也不输出其他内容（#2276）：胶水从可选 `--wasm-opt` 重写后的字节导出，所以仅含 `println` 的程序用 `--host js --wasm-opt` 构建时，恰好携带 `fd_write` 和 `proc_exit`。shim 表外的 import 会在构建时点名拒绝，而非在网页中出现 `LinkError`。
- `@extern(wasm, "js", "name")` import 通过 `init(source, { js: { name } })` 接线；`String` 参数在用户函数运行前从块 header 解码，`String` 返回值编码到 guest 拥有的新块。structural 路径将每个声明 lowering 为声明的 import（#2275）：函数 slot 先输出为显式失败的 stub，再由完成字节上的后处理（`imports.rs`）在五个 `almide.*` import 之后，将每个 stub 转成 `(import module name (func ...))`，通过同一映射重新编号 call、export 和 table entry。ABI 与 export 相同（`Int` → i64、`Float` → f64、`Bool` → i32、`String` → i32 block、`Unit` → 无结果）；`String` 参数跨调用借用（宿主不释放任何内容），`String` 结果是 guest 拥有的新块。`rs`/`rust` extern 没有 wasm 宿主，仍被 wall 阻挡（E082）。`spec/wasm_host_js/extern_js.almd` 中的 `// @leg: structural` 行是使路由变化可见的棘轮行；`almide run --target wasm` 会点名拒绝此类程序（没有承接 import 的宿主），并指向 `--host js`。
- 每个 `pub fn` 都获得 wrapper：`Int` ↔ `number`（超出 ±2^53 报 `RangeError` 而不静默截断——要精确保留更宽值，可传入支持 `BigInt` 的 hook）、`Float`、`Bool`、`String`、`Unit`，以及（#3354）作为参数和返回值的块形态：`Bytes` ↔ `Uint8Array`（复制）、`List[T]` ↔ `Array<T>`（此处支持的任意元素类型，包括嵌套列表）、`Option[T]` ↔ `T | undefined`（`none` 是 `undefined`，输入也接受 `null`），record ↔ 按声明顺序带记录字段的普通 object（缺字段会在调用前报 `TypeError`）。每个块按 emitter 为各 export 记录的布局构建和读取（`almide_wasm::host_exports::export_params_noted` / `export_rets`：元素 stride、字段 offset、record size），绝不由宿主重新推导；记录与源码类型不符时构建拒绝。所有权遵循记录的参数所有权：callee 拥有的块由其释放，借用块由宿主在调用后释放。模块的 `__release` 是 flat 的，因此宿主在丢弃块的最后一个 credit 时遍历形态，释放该块持有的子对象。variant、`Map`、`Set`、tuple 和函数仍在构建时点名函数与类型后拒绝。随块 helper 交付的 `memoryBytes()` 报告线性内存大小，便于宿主检查泄漏。
- `effect fn` export（以及声明 `Result[T, String]` 的函数）无论声明的 `T` 是什么，都返回一个 `Result` 块（tag 位于 payload+0，0 = ok；value slot 位于 payload+8）（#3352）。wrapper 将其 unwrap：ok 是按上述方式 marshalling 的 `T`，err 则抛出 `AlmideError`（一种 `Error`），其 `message` 为 err String。块通过 `__release` 释放；若 Result 持有最后一个引用，也释放 slot 自身的 String 块。wrapper 按 emitter 为每个 export 记录的返回 ABI（`almide_wasm::host_exports::export_rets`）选择，而非源码类型；记录不符源码类型（或 err 不是 String）时，会点名函数并在构建时拒绝。`.d.ts` 给出 `T` 并声明 `AlmideError`；仅有 export unwrap Result 时才交付后者。关卡：`spec/wasm_host_js/effect_exports.almd` 在 node 下运行每种 marshalling 类型 × {fn, effect fn} × {ok, err}。
- 会 throw 的 hook（或标记 `returns: promise` 而 reject 的 hook）（#3356）。穿过 wasm 的异常展开会跳过被展开 frame 本应执行的每次 release，因此胶水绝不让异常穿过：
  - **可失败** extern 是 `effect fn`，或声明 `Result[T, String]` 的函数（`T` 为标量、`String` 或 `Unit`）。它作为宿主构建的 i32 `Result` 块 import。hook 的值成为 ok，throw 成为 err（消息）。Almide 调用者通过自身 release 路径用 `!` 传播 err，传播它的 export 会 reject 或 throw `AlmideError`。失败调用不会使内存增长：`spec/wasm_host_js/hook_errors{,_async}.almd` 用 3,000 轮和 600 轮固定这一点。改为展开时，同样 3,000 轮使线性内存从 0.9 MB 增至 7.3 MB。
  - **不可失败** extern（返回 `T` 的普通 `fn`）按声明不能失败，因此此处 throw 是 trap。胶水重新抛出注明 import 与修复方式（声明为 `effect fn`）的错误，并放弃 instance。此后每次调用均抛出 `… abandoned … call init() again`，而不是在已展开 frame 仍持有块的堆上继续运行。`init()` 启动新 instance。
- 返回 Promise 的 hook 在声明上标记为 `@extern(wasm, "js", "name", returns: promise)`（#3353、#3371），胶水通过 JSPI 在此挂起。未标记 hook 返回 thenable 时，会抛出 `almide: hooks.js.<name> returned a Promise; mark its @extern with
  returns: promise` 并放弃 instance，即使该 extern 可失败也是如此。设计：[JS-HOST-ASYNC-IMPORTS.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/wasm/JS-HOST-ASYNC-IMPORTS.md)。
- `main` 对应 `run()`；`init` 不调用 `_start`。

String marshalling 读取块布局（`almide-layout`：rc @0、len @4、cap @8、payload @12），并读取模块自身签名来确定各 slot 的 valtype（structural 路径上 `Bool` 为 `i32`）。宿主构建的块通过模块导出的 allocator（`__alloc`），宿主取出的块通过导出的 release（`__release`）释放；这两个 export 仅在 `--host js` 且宿主接口需要块 marshalling 时存在：String、Bytes、List、Option、record，或 unwrap Result 的错误消息（`src/cli/js_host.rs` 中的 `HostSurface::needs_string_abi`）。不需要这些内容的纯标量接口，模块与不带开关的构建逐字节相同，也没有块 marshalling helper。structural 路径还记录哪些导出参数归 callee 所有，因此宿主恰好释放自己仍持有的 credit。

关卡：`scripts/check-js-host.sh` 用 `--host js` 构建每个 `spec/wasm_host_js/*.almd`，在 node 下运行，将 stdout 与 `<fixture>.expected` 逐字节比较，运行 fixture 的 `<fixture>.host.mjs`（`js` hook 和执行 wrapper 的 `after(module)`）；对没有 `@extern` 的 fixture，还比较 native 二进制 stdout。它也断言交付内容（#2276）：纯标量情况的模块与不带 `--host js` 的构建逐字节相同；需要块的 marshalling 接口具备 allocator/release export；胶水的 `wasi.<name>` shim 集合等于交付模块的 `wasi_snapshot_preview1` import（`wasm-opt` 前后均如此）；每份胶水字节数不超过 `spec/wasm_host_js/glue-ceiling.txt` 的对应行（缩小静默通过，增长需要编辑台账）。CI 在 `checks` 作业中运行它。

<a id="reproducing-the-measurements"></a>

## 复现测量

```bash
# Almide — the structural leg, plain and -Oz (the incumbent columns cannot be reproduced since #2761)
printf 'fn main() -> Unit = {\n  println("Hello, world!")\n}\n' > hello.almd
almide build hello.almd --target wasm -o hello.wasm                    # structural, as shipped
almide build hello.almd --target wasm --wasm-opt -o hello.min.wasm     # structural, -Oz
wasm-objdump -h hello.wasm                                             # the section tables above

# Rust (same target, full size profile — 2026-07-23 numbers)
cargo new rhello && cd rhello
# [profile.release] opt-level="z", lto=true, strip=true, panic="abort", codegen-units=1
cargo build --release --target wasm32-wasip1
```

代码注释译文：Almide 使用 structural 路径，分别为普通版与 -Oz 版；原有路径列自 #2761 后不可复现。两个 build 分别生成交付形态和 -Oz 形态，wasm-objdump 显示上面的节表。Rust 使用同一目标和完整大小优化 profile，数值来自 2026-07-23。

---

原文注意事项：原有路径的大小与速度测量是历史记录，退役后无法复现；泛指可复现每个数字的说明并不取消此限制。带日期测量、CI 导出的代表值及后续 JS-host 功能属于不同表述层，并非新测结果。节表称优化后的 Hello world 仅保留 fd_write，而后面的 JS-host 段落称仅含 println 的优化构建携带 fd_write 与 proc_exit；两者均保留，不擅自统一范围。counter 示例省略了后文讨论的一些字段。“已验证”受所述语料、witness 覆盖及受信任记录限制，外部 wasm-opt 会离开这一范围。宿主 ABI、异常控制及测得的分配／泄漏行为，不是普遍内存安全证明或第三方认证。
