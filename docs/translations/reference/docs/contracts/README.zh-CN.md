<a id="almide-behavior-contracts"></a>

# Almide 行为契约

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](README.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](README.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](README.zh-CN.md)

> 非规范性固定历史读者版。正本未改动且保持权威。下文规范性/已证明/安全/无泄漏/精确/跨target一致等陈述是原契约主张，不是新的验证或外部认证。本翻译没有运行命令、测试或证明，也不自动同步。388契约盖章值截至2026-10-21，而原文包含后来直至C-448的行；fixture数为0及证据类别照录，不作推断。保留C-006与C-208的fan.timeout、C-020与C-412的Unicode大小写、旧边界行为与后续规则等历史张力，不擅自合成为普遍当前保证。生成标记为不活性展示；本版不是正本生成表或规范性契约来源。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/contracts/README.md)

> 原本由 [contracts.toml](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/contracts/contracts.toml) 自动生成。
> 更新命令为 `bash docs/contracts/generate-readme.sh > docs/contracts/README.md`。
>
> 每个契约都是编译器在两种 target（native Rust + wasm32：stdout、stderr、退出码）上遵守的规范性、可观察承诺。
> native 是 oracle；native == wasm 是严格不变量。每个契约都可追溯到可执行证据
> （`spec/wasm_cross/*.almd` fixture、差分 fuzz、输出时 Σ-probe 或 Lean 定理），
> 没有任何声称的行为只依赖说明文字。

<a id="change-discipline"></a>

## 变更纪律

- **改变任何可观察行为，必须在同一个 PR 中同时更新契约陈述及其证据。**
- **新**行为 = 新 `C-NNN` + ≥1 fixture。
- **消除差异** = 同一 PR 将 `status` 改为 `active` 并移除 flag。`flagged-for-revision` 数量是棘轮，只能**减少**。
- 门禁（`scripts/check-contracts.sh`，CI + lefthook）强制每个契约拥有真实证据、每个 fixture 指明其契约、且链接双向。
- **汇总数量盖章固定，不按 PR 重新生成。** 下方“N contracts”合计（以及生成文档引用的所有其他合计）来自
  `proofs/ledger-counts.toml`，日期位于 `counts:generated` 块。fixture/契约 PR 重新生成各行，但不改此块。
  在发布封印前，或夜间 `scripts/check-ledger-counts.sh` 报告偏差时，通过 `bash scripts/gen-ledger-counts.sh` 更新。

证据类别（弱 → 强）：`doc-only` < `by-construction` < `fixture` < `fuzz` < `exhaustive` < `lean`。
**active** 契约必须有 ≥1 项类别 ≥ `fixture` 的证据。

> 原本生成总计块（不活性的历史展示）：截至 2026-10-21。proofs/ledger-counts.toml 的盖章总数，只由 scripts/gen-ledger-counts.sh 更新，绝不由 fixture/contract PR 更新。原标记之间禁止编辑。本参照版不自动更新。
388 个契约
> 原本生成总计块结束（不活性）。

| ID | 契约 | 引入版本 | 状态 | 最强证据 | Fixture 数 |
|----|----------|-------|--------|--------------------|-----------:|
| C-001 | 整数除零/模零是全定义的——abort，绝不 trap | 0.24.0 | active | fixture | 5 |
| C-002 | 有符号 MIN / -1 溢出在各位宽真正的 MIN 处 abort | 0.24.0 | active | fixture | 3 |
| C-003 | 不 abort 的整数除法/取模保持逐字节一致 | 0.24.0 | active | fixture | 1 |
| C-004 | fan.map / fan.settle 按列表顺序确定 | 0.24.0 | active | fixture | 11 |
| C-005 | fan 错误传播表现为统一的 main-error abort | 0.24.0 | active | fixture | 5 |
| C-006 | [fan.timeout 不存在——墙钟期限位于 host 边界](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/contracts/C-006-fan-timeout-removed.md) | 0.29.0 | active | fixture | 0 |
| C-007 | 可能 abort 的顶层 let 在启动时立即求值 | 0.24.0 | active | fixture | 4 |
| C-008 | [复合值插值呈现 Almide 字面量 repr（容器）](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/contracts/C-008-009-010-repr.md) | 0.24.0 | active | fixture | 3 |
| C-009 | [record / variant / 匿名 record 的插值 repr（字段排序）](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/contracts/C-008-009-010-repr.md) | 0.24.0 | active | fixture | 4 |
| C-010 | [递归/泛型 ADT 插值 repr 以实例化为键](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/contracts/C-008-009-010-repr.md) | 0.24.0 | active | fixture | 2 |
| C-011 | 裸 float 插值的 Display 去掉 .0，float.to_string 保留 | 0.24.0 | active | fuzz(100000) | 5 |
| C-012 | 常量折叠的非有限浮点数发出具名常量 | 0.24.0 | active | fixture | 1 |
| C-013 | Map 是 compact-ordered-dict：按插入顺序遍历 | 0.24.0 | active | fixture | 11 |
| C-014 | Set 按插入顺序且确定 | 0.24.0 | active | fixture | 3 |
| C-015 | 复合元素和堆值的结构性深度相等 | 0.24.0 | active | fixture | 10 |
| C-016 | 支持 UTF-8 代码点的字符串操作逐字节一致 | 0.24.0 | active | fixture | 3 |
| C-017 | 空模式的 count / last_index_of 遵循 native 的代码点/字节语义 | 0.24.0 | active | fixture | 2 |
| C-018 | Unicode 字符串谓词在整个定义域与 Rust char 方法一致 | 0.24.0 | active | fixture | 1 |
| C-019 | rt_string_extra 操作（replace_first、strip_*、谓词、cmp）与 native 一致 | 0.24.0 | active | fixture | 2 |
| C-020 | Unicode 大小写变换（to_upper/to_lower/capitalize）覆盖完整 Unicode | 0.24.0 | active | fixture | 1 |
| C-021 | 空白 trim / is_whitespace 使用完整 Unicode White_Space 属性 | 0.24.0 | active | fixture | 1 |
| C-022 | bytes.to_string_lossy 是 UTF-8 有损解码（bytes.from_string 的逆） | 0.24.0 | active | fixture | 1 |
| C-023 | float.to_string 是可往返的最短十进制表示，与 Rust Display 逐字节一致 | 0.24.0 | active | fuzz(100000) | 2 |
| C-024 | float.parse 正确舍入至最近偶数（Clinger AlgorithmM） | 0.24.0 | active | fixture | 1 |
| C-025 | float.to_fixed 对精确二进制值执行中点舍入到偶数 | 0.24.0 | active | fuzz(1000) | 1 |
| C-026 | 内置 libm 的 trig / exp / log / pow 跨 target 逐字节一致 | 0.24.0 | active | fuzz(4000) | 3 |
| C-027 | base64 编码/解码（标准＋URL-safe）包括错误均逐字节一致 | 0.24.0 | active | fixture | 1 |
| C-028 | int.parse_radix(s, 16)：trim、一个可选符号、一个可选 0x / 0X，再接十六进制数字 | 0.24.0 | active | fixture | 1 |
| C-029 | int.parse 的错误模式与 native ParseIntError 逐字节一致 | 0.24.0 | active | fixture | 1 |
| C-030 | hex.encode / hex.decode 连同位置错误详情逐字节一致 | 0.24.0 | active | fixture | 2 |
| C-031 | value get/set/remove_path 边界情形与从不因路径边界情形而失败的 native oracle 一致 | 0.24.0 | active | fixture | 3 |
| C-032 | Regex 引擎在 fuzz 文法范围内与 native 引擎逐字节一致 | 0.24.0 | active | fuzz(220) | 6 |
| C-033 | [具有别名的可变值采用值语义（copy-on-write）](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/contracts/C-033-cow-truth-table.md) | 0.24.0 | active | fixture | 22 |
| C-034 | 越界 list 操作平稳地夹取/不操作（无越界堆访问） | 0.24.0 | active | fixture | 8 |
| C-035 | Effect-main 错误统一终止：Error: <msg> ＋ exit 1 | 0.24.0 | active | fixture | 6 |
| C-036 | record、variant 和模式匹配逐字节一致 | 0.24.0 | active | fixture | 10 |
| C-037 | bytes.read_f16_le 一致地解码 IEEE-754 半精度浮点数 | 0.24.0 | active | fixture | 1 |
| C-038 | 固定位宽整数字面量缩窄至声明的字段宽度 | 0.24.0 | active | fixture | 4 |
| C-039 | 改变类型的 map.map / set.map 返回新类型的集合 | 0.24.0 | active | fixture | 2 |
| C-040 | codegen 输出对 host 架构具有确定性 | 0.24.0 | active | fixture | 3 |
| C-041 | 堆/RC 原语遵循经 Lean 证明的 Perceus 规则 | 0.24.0 | active | lean | 5 |
| C-042 | fs 的 preopen-dir 扫描＋路径解析在可观察行为上等价 | 0.24.0 | active | fixture | 1 |
| C-043 | 名为 Box 的用户类型与递归 enum 的堆间接引用共存 | 0.24.0 | active | fixture | 1 |
| C-044 | Result/Option 构造和匹配逐字节一致 | 0.24.0 | active | fixture | 6 |
| C-045 | List[String] 参数在 join / len / index / 遍历中工作 | 0.24.0 | active | fixture | 3 |
| C-046 | record 展开更新和跨模块单态化逐字节一致 | 0.24.0 | active | fixture | 1 |
| C-047 | 负指数的 int.pow 是全定义的——abort，不 trap 或回绕 | 0.24.0 | active | fixture | 2 |
| C-048 | int.wrap_* / int.rotate_* 在64位寄存器上运行，int.wrap 在 bits >= 64 时使掩码饱和 | 0.24.0 | active | fixture | 1 |
| C-049 | float.sign 是 f64::signum；float/math 的 min/max 忽略 NaN | 0.24.0 | active | fixture | 1 |
| C-050 | string.split(\"\") 以代码点为粒度 | 0.24.0 | active | fixture | 1 |
| C-051 | math.log_gamma 逐位一致（两种 target 都使用内置 musl-libm log） | 0.24.0 | active | fixture | 2 |
| C-052 | 对空集合 fold 要求集合具有元素类型（codegen 不设默认类型） | 0.24.0 | active | fixture | 1 |
| C-053 | list.min/max/sort/sort_by/unique_by 由类型决定且全定义，native == wasm | 0.24.0 | active | fixture | 5 |
| C-054 | List/string 的 Int 数量和下标在缩窄前按 i64 夹取——不截断、不越界 | 0.24.0 | active | fixture | 11 |
| C-055 | Float 的 list.min/max/sort/sort_by 使用 IEEE-754 totalOrder，在两种 target 上有效且相同 | 0.24.0 | active | fixture | 3 |
| C-056 | list.product 在 i64 溢出时回绕，与 list.sum 和无后缀 `*` 一致 | 0.24.0 | active | fixture | 1 |
| C-057 | 将返回 Unit 的原地修改函数的结果赋给变量，在两种 target 上均为 checker 错误 | 0.24.0 | active | fixture | 1 |
| C-058 | 无法推断元素类型的空集合在两种 target 上是编译错误，绝不静默采用默认类型 | 0.24.0 | active | fixture | 2 |
| C-059 | 编译宽或深输入不会使 native 栈溢出，所有 host 和 build profile 一致 | 0.25.0 | active | fixture | 1 |
| C-060 | Value 无论独立还是作为 Repr-record 字段，都在 native 和 WASM 上以逐字节一致的 JSON 文本 repr 显示 | 0.26.7 | active | fixture | 2 |
| C-061 | 原地修改的 mut Map 参数在两种 target 上均可构建，修改保留且逐字节一致 | 0.26.9 | active | fixture | 1 |
| C-062 | Region 移动字节在两种 target 上逐字节一致，区域外访问为 err | 0.26.15 | active | fixture | 1 |
| C-063 | 解析异构嵌套 glTF/JSON 文档并逐元素遍历其数组，在两种 target 上逐字节一致 | 0.26.19 | active | fixture | 1 |
| C-064 | effect-fn 的 Result 自动 unwrap 规则在各绑定位置相同且由类型决定，两种 target 逐字节一致 | 0.26.20 | active | fixture | 3 |
| C-065 | 字符串位置 API 在两种 target 上端到端按代码点索引 | 0.26.20 | active | fixture | 3 |
| C-066 | WASM 堆默认回收（真正的 Perceus） | 0.27.0 | active | fixture | 23 |
| C-067 | xs[i] 下标语法越界即 abort（读和写均如此） | 0.27.4 | active | fixture | 9 |
| C-068 | 构造位置的 Auto-? 由目标类型决定 | 0.27.4 | active | fixture | 3 |
| C-069 | effect-fn 尾部自递归在两种 target 上转换为 O(1) 栈的循环 | 0.27.4 | active | fixture | 3 |
| C-070 | 嵌套构造器模式在两种 target 上一致地匹配和绑定 | 0.27.6 | active | fixture | 15 |
| C-071 | 单部分插值的 RC 平衡 | 0.27.6 | active | fixture | 1 |
| C-072 | 推断出的具名 record 的 repr 一致 | 0.27.6 | active | fixture | 2 |
| C-073 | 测试 variant 构造器的 tuple 模式 | 0.27.6 | active | fixture | 1 |
| C-074 | 对大输入的迭代式 split/replace | 0.27.6 | active | fixture | 1 |
| C-075 | lowmisc 第5轮群组：借用参数的拥有型绑定、effect-Option auto-try 剥除、匹配错误的 ! 透传 | 0.27.6 | active | fixture | 3 |
| C-076 | 生产方模块内 variant 构造跨 target 稳定 | 0.27.6 | active | fixture | 3 |
| C-077 | 跨模块堆全局初始化顺序遵守依赖关系 | 0.27.6 | active | fixture | 1 |
| C-078 | phantom record 的泛型参数在 Rust target 上移除 | 0.27.6 | active | fixture | 1 |
| C-079 | 载荷为不同匿名 record 的 variant case 跨 target 稳定 | 0.27.6 | active | fixture | 1 |
| C-080 | 空 map.from_list / set.from_list 从结果类型解析元素类型 | 0.27.6 | active | fixture | 1 |
| C-081 | 参数类型推断 lambda 内的泛型 fn 解析其类型参数 | 0.27.6 | active | fixture | 1 |
| C-082 | 调用闭包类型的 lambda 参数返回调用结果，而非闭包 | 0.27.6 | active | fixture | 1 |
| C-083 | 取负的 i64::MIN 字面量可表示，不折叠为零 | 0.27.6 | active | fixture | 1 |
| C-084 | Codec/value 解码错误消息跨 target 逐字节一致 | 0.27.6 | active | fixture | 3 |
| C-085 | Float 解码将整数 JSON 数值扩宽至 f64 | 0.27.6 | active | fixture | 1 |
| C-086 | 透传型 stdlib 组合器给结果自身的引用 | 0.27.6 | active | fixture | 1 |
| C-087 | JSON 数值和 \\u 字符串解码跨 target 逐字节一致 | 0.27.6 | active | fixture | 4 |
| C-088 | 以 Rust 关键字为名的函数在两种 target 上可编译 | 0.27.6 | active | fixture | 2 |
| C-089 | 引用前面参数的默认参数以对应实参补齐 | 0.27.6 | active | fixture | 1 |
| C-090 | 对 List[Int] 参数的 bytes.from_list 在两种 target 上可编译 | 0.27.6 | active | fixture | 1 |
| C-091 | let 解构中的嵌套子模式绑定每个叶节点 | 0.27.6 | active | fixture | 1 |
| C-092 | 泛型 record 字段按构造时实例化类型确定大小 | 0.27.6 | active | fixture | 3 |
| C-093 | 相互递归 variant 类型在两种 target 上可编译 | 0.27.6 | active | fixture | 1 |
| C-094 | 对推断类型的 lambda 参数进行 protocol-method UFCS 调用可解析元素类型 | 0.27.6 | active | fixture | 1 |
| C-095 | json.stringify_pretty 跨 target 输出逐字节一致的缩进文本 | 0.27.6 | active | fixture | 2 |
| C-096 | env.args 在 WASM 工作并与 native 一致 | 0.27.6 | active | fixture | 1 |
| C-097 | 类型参数的泛型 + 跨 target 一致地连接字符串/列表 | 0.27.6 | active | fixture | 1 |
| C-098 | 跨模块派生 Codec 方法在 WASM 分派并与 native 一致 | 0.27.6 | active | fixture | 0 |
| C-099 | 比较/相等运算符在 v1 wasm 路径的全部操作数类型上与 native 逐字节一致 | 0.27.6 | active | fixture | 10 |
| C-100 | self-host String 分类/变换操作在 wasm 与 native 逐字节一致 | 0.27.6 | active | fixture | 4 |
| C-101 | 堆元素（String/Value）的 List 操作与 native 逐字节一致，无泄漏/重复释放 | 0.27.6 | active | fixture | 18 |
| C-102 | List 遍历、调用结果元素具体化、尾递归列表遍历与 native 逐字节一致 | 0.27.6 | active | fixture | 5 |
| C-103 | self-host 动态 Value 模型（merge、array/as_array 往返、tuple TCO）与 native 逐字节一致，在循环中无泄漏 | 0.27.6 | active | fixture | 8 |
| C-104 | 尾递归 accumulator 形态降低为与 native 逐字节一致的有界栈循环 | 0.27.6 | active | fixture | 6 |
| C-105 | var/append accumulator 循环（scalar、拥有型句柄、跨依赖、相互递归）在 wasm 与 native 逐字节一致 | 0.27.6 | active | fixture | 7 |
| C-106 | 从 if/match 分支绑定的堆值在 v1 wasm 路径与 native 逐字节一致 | 0.27.6 | active | fixture | 17 |
| C-107 | 堆上 Result-of-tuple / Result-of-list 的 Ok 载荷往返且与 native 逐字节一致 | 0.27.6 | active | fixture | 4 |
| C-108 | unwrap `!` 和 let-unwrap 脱糖在所有位置与 native 逐字节一致 | 0.27.6 | active | fixture | 10 |
| C-109 | self-host base64 编码在 v1 wasm 路径与规范形式/native 逐字节一致 | 0.27.6 | active | fixture | 1 |
| C-110 | 原地 bytes.push 修改 accumulator 在 v1 wasm 与 native 逐字节一致 | 0.27.6 | active | fixture | 1 |
| C-111 | 模块级 const 堆全局在 v1 wasm 与 native 上一致地初始化和读取 | 0.27.6 | active | fixture | 2 |
| C-112 | random.int 抽样在 v1 wasm 的 WASI 熵下限之下同样保持在范围内 | 0.27.6 | active | fixture | 1 |
| C-113 | 对 let 绑定的 ADT/Result variant 按 tag 匹配在 v1 wasm 与 native 逐字节一致 | 0.27.6 | active | fixture | 2 |
| C-114 | 匹配具有堆载荷的 Option 在 v1 wasm 与 native 逐字节一致 | 0.27.6 | active | fixture | 2 |
| C-115 | 管道传入产生值的块体 lambda 在 v1 wasm 与 native 逐字节一致 | 0.27.6 | active | fixture | 2 |
| C-116 | v1 scalar-value lowering 边界情形与 native 逐字节一致（尾部 Bool 字面量、float.parse inf/nan） | 0.27.7 | active | fixture | 2 |
| C-117 | 循环内 let 绑定的堆 if/match 提升至尾部 helper 并在 v1 显示 | 0.27.7 | active | fixture | 3 |
| C-118 | env.args 在 WASM 工作并与 native 一致（跳过 argv[0]） | 0.27.8 | active | fixture | 1 |
| C-119 | `for` 循环体内的 effect-`!` 传播 Err 并与 native 逐字节一致 | 0.27.6 | active | fixture | 1 |
| C-120 | 带捕获且分支体按条件保留/跳过的 filter_map 与 native 逐字节一致 | 0.27.6 | active | fixture | 1 |
| C-121 | String 透传快速路径返回拥有型（+1）引用 | 0.27.6 | active | fixture | 2 |
| C-122 | Value object 操作分配完整 list layout，并以 +1 共享键值对 | 0.27.6 | active | fixture | 2 |
| C-123 | Record spread 以 +1 共享复制的堆字段与别名覆盖值 | 0.27.6 | active | fixture | 1 |
| C-124 | Value 相等是深度结构相等，对应 native PartialEq | 0.27.6 | active | fixture | 1 |
| C-125 | bytes.set 采用值语义——绝不会通过输入观察到修改 | 0.27.6 | active | fixture | 1 |
| C-126 | 嵌套 lambda 的 HOF 参数保留推断链接（不固定 literal sig-generic） | 0.27.6 | active | fixture | 2 |
| C-127 | 链类型未解析时，unwrap_or 从默认值确定载荷大小 | 0.27.6 | active | fixture | 2 |
| C-128 | datetime.format 在每个 backend 一致地替换 strftime 指定符 | 0.28.1 | active | fixture | 1 |
| C-129 | list.chunk / list.window 的非正大小：负数保留已提升的规范，零以 T6 形式 abort | 0.28.4 | active | fixture | 5 |
| C-130 | option/map 组合器返回拥有型堆结果（无裸透传句柄） | 0.28.5 | active | fixture | 2 |
| C-131 | 循环重建缓冲区为 O(n)：COW 只保护活跃别名，LICM 绝不提升堆分配 | 0.28.6 | active | fixture | 3 |
| C-132 | 重新分配容器的 mut 参数在所有调用位置保留到调用方 | 0.28.6 | active | fixture | 31 |
| C-133 | env.get 在 native 与 wasm 一致地观察 host 环境 | 0.29.0 | active | fixture | 1 |
| C-134 | 内置 libm 的 atan / tanh 跨 target 逐字节一致 | 0.30.0 | active | fuzz(3000) | 1 |
| C-135 | 声明为 Unit 的 effect fn ABI 在定义与所有调用点一致 | 0.30.0 | active | fixture | 2 |
| C-136 | 对 place 的原地修改在两种 target 保留至后续读取 | 0.30.0 | active | fixture | 1 |
| C-137 | wasm 上的相对 fs 路径按 host CWD 解析 | 0.31.0 | active | fixture | 1 |
| C-138 | 以 stdlib 调用为载荷的 ok/err ctor 具体化真实值 | 0.31.0 | active | fixture | 1 |
| C-139 | 堆 Ok 的 Result 值组合器保留 tag 和载荷 | 0.31.0 | active | fixture | 2 |
| C-140 | float.round 保留零结果的符号 | 0.31.0 | active | fixture | 2 |
| C-141 | list.zip_with 按元素 repr 路由——String zip 工作，不链接错误类型 | 0.31.0 | active | fixture | 1 |
| C-142 | result.unwrap_or_else 在 Float 实例化的两条路径均为有效 wasm | 0.31.0 | active | fixture | 1 |
| C-143 | ctor 的 if 载荷具体化选中的分支 | 0.31.0 | active | fixture | 1 |
| C-144 | scalar-list 字面量绝不静默表现为空列表 | 0.31.0 | active | fixture | 1 |
| C-145 | 带单态化后缀的 stdlib 组合器名称按基名路由 | 0.31.0 | active | fixture | 1 |
| C-146 | 返回所捕获别名的提升闭包给出共享拥有引用 | 0.31.0 | active | fixture | 1 |
| C-147 | list.unique_by 按 key repr 路由——String 键按内容去重 | 0.31.0 | active | fixture | 1 |
| C-148 | list.scan 按 accumulator 自身宽度保存每个中间值 | 0.31.0 | active | fixture | 1 |
| C-149 | unwrap_or_else 返回共享拥有的堆 Ok 载荷 | 0.31.0 | active | fixture | 6 |
| C-150 | 包裹堆 var 的 ctor 是值复制——var 保持存活 | 0.31.0 | active | fixture | 1 |
| C-151 | 结果为堆 Ok 的 Result 组合器绝不链接 scalar 实现 | 0.31.0 | active | fixture | 1 |
| C-152 | ctor 内未获准的堆调用载荷以 wall 拒绝，绝不置零 | 0.31.0 | active | fixture | 1 |
| C-153 | 非测试 assert 失败在两种 target 以 T6 形式 abort | 0.31.0 | active | fixture | 6 |
| C-154 | 无效范围的 clamp 以 T6 形式 abort | 0.31.0 | active | fixture | 2 |
| C-155 | 小数位数超出定义域的 to_fixed 以 T6 形式 abort | 0.31.0 | active | fixture | 3 |
| C-156 | 由 if 合并的 some((String, String)) ctor 是真正被跟踪的 Option | 0.32.0 | active | fixture | 1 |
| C-157 | 未注解泛型 ctor 的 top-let 将已解析的载荷类型传给所有读取方 | 0.32.0 | active | fixture | 1 |
| C-158 | 载荷为 scalar 调用或 tuple 的 some/ok ctor 具体化真实值，绝非置零 ctor | 0.32.0 | active | fixture | 5 |
| C-159 | list.binary_search 对重复键在两种 target 返回同一下标 | 0.32.0 | active | fixture | 1 |
| C-160 | 纯 Almide 内置 stdlib 模块在 wasm 逐字节一致地链接和运行 | 0.34.4 | active | fixture | 1 |
| C-161 | Matrix 构造器维度夹取负数，超过共用上限则 abort | 0.35.0 | active | fixture | 14 |
| C-162 | io.write / io.write_bytes 与 println 交错时仍按程序顺序输出 | 0.35.0 | active | fixture | 1 |
| C-163 | 绑定至 let/var 的堆结果 if/match 在两种 target 执行所选分支 | 0.35.0 | active | fixture | 1 |
| C-164 | List 修改操作与后缀副本共享拥有 tuple / record / 嵌套 list 元素 | 0.35.0 | active | fixture | 1 |
| C-165 | 对 String 键 map、String list 或 String set 的 fold 在两种 target 传递堆 accumulator | 0.36.0 | active | fixture | 1 |
| C-166 | Map 插值在两种 target 呈现所有 self-host key/value 配对 | 0.36.0 | active | fixture | 1 |
| C-167 | float.clamp 原样返回范围内输入，包括符号位 | 0.36.0 | active | fixture | 1 |
| C-168 | list.flatten 借用其参数，可与同一绑定的另一借用组合 | 0.36.0 | active | fixture | 2 |
| C-169 | 超过大小上限的 list.repeat 在两种 target 以 T6 形式 abort | 0.36.0 | active | fixture | 1 |
| C-170 | 整数运算在所有位置回绕，包括模块级 let | 0.36.0 | active | fixture | 3 |
| C-171 | 字节偏移边界检查在 i64 边界不溢出 | 0.36.0 | active | fixture | 1 |
| C-172 | 任意堆载荷上的 unwrap_or 在两种 target 返回同值 | 0.36.0 | active | fixture | 2 |
| C-173 | 超出上下文可表示范围的整数字面量在两种 target 是 checker 错误，绝不静默产生值 | 0.36.0 | active | fixture | 2 |
| C-174 | 尾递归 Map/Set accumulator 在两种 target 保留 seed，包括非空 seed | 0.36.0 | active | fixture | 1 |
| C-175 | variant 类型的 List 字面量在绑定及堆结果 if 分支位置，均在两种 target 一致地构造和销毁 | 0.37.0 | active | fixture | 1 |
| C-176 | 包裹内联返回 tuple 的调用的 some/ok 在两种 target 具体化真实载荷 | 0.37.0 | active | fixture | 1 |
| C-177 | 循环体调用参数中的可变全局投影读取在每次迭代读取当前槽位 | 0.37.0 | active | fixture | 1 |
| C-178 | 相互尾递归链在 wasm target 以无界深度运行 | 0.37.0 | active | fixture | 2 |
| C-179 | UInt64 覆盖声明的完整定义域，每个观察方以无符号方式读取槽位 | 0.37.0 | active | fixture | 5 |
| C-180 | 固定位宽整数 +、-、*、^ 在两种 target 按声明宽度回绕 | 0.37.0 | active | fixture | 1 |
| C-181 | args.positional 返回全部非 flag 参数，args API 跨 target 一致 | 0.37.0 | active | fixture | 1 |
| C-182 | 取负的 float 字面量在两种 target 采用上下文的 float 类型 | 0.37.0 | active | fixture | 2 |
| C-183 | 对 scalar record 列表的循环携带 append 在两种 target 执行 | 0.37.0 | active | fixture | 1 |
| C-184 | 整数 `^` 运算符在两种 target 与 int.pow 是相同的全定义幂运算 | 0.37.0 | active | fixture | 1 |
| C-185 | `??` 链无论位于何处，都在两种 target 返回源码顺序的首个 Ok | 0.37.0 | active | fixture | 1 |
| C-186 | 将内联下标读取 append 至 list accumulator 可在两种 target 降低并运行 | 0.37.0 | active | fixture | 2 |
| C-187 | 通过模块级 `var` 缓冲区写入的原地修改函数在两种 target 行为一致 | 0.37.0 | active | fixture | 6 |
| C-188 | scalar `if` 分支执行语句效应——全局写入生效，外层 var 重赋值落到稳定 local | 0.37.0 | active | fixture | 1 |
| C-189 | env.os 与 temp-dir API 报告 HOST——等价律唯一的例外，且该例外集合封闭 | 0.37.0 | active | fixture | 1 |
| C-190 | 闭包捕获任意 scalar，不限于 Int/Bool，包括 Float 和所有固定位宽整数 | 0.37.0 | active | fixture | 1 |
| C-191 | 模块级 global 在每条分支路径读取真实值——具体化备忘限定于直线执行上下文 | 0.37.1 | active | fixture | 2 |
| C-192 | 字符串插值内自动 unwrap 的 effect 调用像调用参数一样提升并传播 | 0.37.1 | active | fixture | 3 |
| C-193 | 经可变 global 的反复原地写入保持原地——receiver 借用 COW 后的句柄 | 0.37.1 | active | fixture | 1 |
| C-194 | 目的地为可变 global 的 bytes.copy_from 通过存储槽写入 | 0.38.1 | active | fixture | 1 |
| C-195 | 对 checked-conversion 族的值位置 variant match 得以执行 | 0.38.1 | active | fixture | 1 |
| C-196 | 调用栈耗尽是各 target 指定深度上的已定义 abort | 0.41.0 | active | fixture | 1 |
| C-197 | linear memory 耗尽是具有已定义 abort 的资源限制 | 0.41.0 | active | fixture | 9 |
| C-198 | head-count 参数的 matrix 入口在 dialect epoch 14 离开 stdlib | 0.42.0 | active | fixture | 0 |
| C-199 | fan 块 join 所有 sibling，报告列表顺序的首个 Err | 0.42.0 | active | fixture | 2 |
| C-200 | fan sibling 中的 trap 经统一 main-error abort 收敛地退出 | 0.42.0 | active | fixture | 3 |
| C-201 | Option 组合器的 tuple 结果在所有元素类型组合中可具体化为拥有型元素 | 0.44.0 | active | fixture | 1 |
| C-202 | 时间构造器保护定义域：负值 abort，溢出饱和 | 0.47.0 | active | fixture | 3 |
| C-203 | 时间类型运算符代数在两种 target 单位精确且饱和 | 0.47.0 | active | fixture | 1 |
| C-204 | fan.bounded 判定仅由程序及其输入决定 | 0.47.0 | active | fixture | 9 |
| C-205 | fan.min 胜者是获准分支中 (spend, index) 的字典序最小者 | 0.47.0 | active | fixture | 8 |
| C-206 | settle 块按分支顺序将每个分支 settle 到其自身 tuple 槽位 | 0.47.0 | active | fixture | 1 |
| C-207 | CM-1 是带版本的常量，对墙钟时间只作比率声明 | 0.47.0 | active | fixture | 1 |
| C-208 | fan.timeout 相对于 omega：fixed 在所有环境结束，固定 omega 逐字节一致重放 | 0.47.0 | active | fixture | 1 |
| C-209 | Codec encode 省略 none；decode 将缺失/null 合为 none；Value 原样透传 | 0.52.0 | active | fixture | 2 |
| C-210 | NaN 观察是规范化的——deterministic-profile 符合性律 | 0.53.0 | active | fixture | 3 |
| C-211 | ! 在纯 Result/Option 返回 fn 中传播，跨 target 逐字节一致 | 0.53.5 | active | fixture | 1 |
| C-212 | 具有同名 private helper 的两个 self-host 模块要么正常链接要么 wall 拒绝——绝非无效模块 | 0.52.0 | active | fixture | 1 |
| C-213 | bytes 字节级写入在两种 target 编码相同缓冲区，包括越界写入 | 0.53.5 | active | fixture | 6 |
| C-214 | process.exec_status_timeout：触发路径错误固定，是否触发取决于 host | 0.53.6 | active | fixture | 0 |
| C-215 | fs 内容 reader：不存在经 runtime 分类，由 _if_exists 族给出 ok(none) | 0.53.7 | active | fixture | 4 |
| C-216 | 声明 Option 的 effect 调用上的显式 ! 与隐式 strip 完全相同 | 0.54.1 | active | fixture | 1 |
| C-217 | let _ = f() 丢弃 Result——err 不传播 | 0.55.0 | active | fixture | 1 |
| C-218 | 作为 fn 尾部返回的堆载荷 ?? 在两种 target 产生相同值 | 0.56.0 | active | fixture | 3 |
| C-219 | 分支内的 Never 类型调用在两种 target 执行并设置退出码 | 0.56.0 | active | fixture | 5 |
| C-220 | fs 逐行 stream walker 按 read_lines 的行语义 fold/each | 0.56.1 | active | fixture | 2 |
| C-221 | effect fn 类型槽位以统一 carrier 语义接受纯及可失败 lambda | 0.56.1 | active | fixture | 4 |
| C-222 | 表达式嵌套的 scalar unwrap 在两条路径一致地传播 err | 0.56.1 | active | fixture | 1 |
| C-223 | Matrix 运算使用内置 musl-libm 和 native 的乘积顺序，而非 platform 的 | 0.56.1 | active | fixture | 2 |
| C-224 | if let / guard let 在两种 target 一致地绑定和释放堆载荷 | 0.56.1 | active | fixture | 1 |
| C-225 | fs.read_lines 在两种 target 一致地具体化文件行 | 0.56.2 | active | fixture | 2 |
| C-226 | 跨越调用边界的 mut 参数在两种 target 修改调用方数据 | 0.56.2 | active | fixture | 14 |
| C-227 | fs 元数据/组合函数族在两种 target 回答一致 | 0.56.2 | active | fixture | 1 |
| C-228 | fs 组合函数族与 matrix 行选择器在两种 target 回答一致 | 0.56.2 | active | fixture | 4 |
| C-229 | 超出字节缓冲区的所选行在两种 target 均为全零行 | 0.57.1 | active | fixture | 5 |
| C-230 | flight 参考 PID kernel 在两种 target 一致地运行 | 0.57.1 | active | fixture | 1 |
| C-231 | 整数/boolean 字面量形式在两种 target 一致地求值 | 0.57.1 | active | fixture | 1 |
| C-232 | boolean 字面量及其显示形式在两种 target 相同 | 0.57.1 | active | fixture | 1 |
| C-233 | unit 字面量、其位置及自反相等在两种 target 相同 | 0.57.1 | active | fixture | 1 |
| C-234 | 括号分组与1-tuple 消歧在两种 target 行为相同 | 0.57.1 | active | fixture | 2 |
| C-235 | 一元运算符：`not` 否定与前缀减号在两种 target 一致地求值 | 0.57.1 | active | fixture | 1 |
| C-236 | tuple 构造/解构/索引/结构相等在两种 target 相同 | 0.57.1 | active | fixture | 1 |
| C-237 | Option/Result 构造器字面量在两种 target 一致地构造和消去 | 0.57.1 | active | fixture | 1 |
| C-238 | 一等 range 值在两种 target 遍历真实边界 | 0.57.1 | active | fixture | 10 |
| C-239 | 集合字面量/索引/空形式在两种 target 行为一致 | 0.57.1 | active | fixture | 1 |
| C-240 | Map 字面量与空 map 读取在两种 target 行为一致 | 0.57.1 | active | fixture | 2 |
| C-241 | 绑定语句：let shadowing、var 重赋值及检查时守卫在两种 target 相同 | 0.57.1 | active | fixture | 1 |
| C-242 | 条件表达式仅求值所选分支，两种 target 相同 | 0.57.1 | active | fixture | 1 |
| C-243 | 块表达式在两种 target 返回同一尾部值 | 0.57.1 | active | fixture | 1 |
| C-244 | while 循环在两种 target 一致地迭代条件检查的循环体 | 0.57.1 | active | fixture | 1 |
| C-245 | 字符串插值在两种 target 一致地嵌入各部分的规范显示 | 0.57.1 | active | fixture | 2 |
| C-246 | 标识符解析至最近绑定；未解析名称为检查时 E003 并指出该标识符 | 0.57.1 | active | fixture | 2 |
| C-247 | match 表达式在两种 target 一致地选择首个匹配分支；不穷尽为检查时 E010 | 0.57.1 | active | fixture | 1 |
| C-248 | for-in 在两种 target 一致地遍历 list 族的 head，可带 tuple 解构 | 0.57.1 | active | fixture | 1 |
| C-249 | tuple let 解构在两种 target 按位置一致地绑定各分量 | 0.57.1 | active | fixture | 2 |
| C-250 | 管道和组合按顺序作为普通应用求值，两种 target 相同 | 0.57.1 | active | fixture | 2 |
| C-251 | if let 通过裸 binder 隐式 unwrap Option 匹配对象，两种 target 相同 | 0.57.1 | active | fixture | 1 |
| C-252 | 表达式语句与注释：Unit 调用执行，丢弃显式，注释不可见 | 0.57.1 | active | fixture | 1 |
| C-253 | place 赋值和类型标注在两种 target 行为相同 | 0.57.1 | active | fixture | 1 |
| C-254 | 类型标注在任一 target 均不改变值，只提供期望类型 | 0.57.1 | active | fixture | 1 |
| C-255 | record 字面量/成员读取/spread 更新在两种 target 一致地保留值语义 | 0.57.1 | active | fixture | 2 |
| C-256 | break 与 continue 在两种 target 和 interp 中一致地于所在语句位置生效 | 0.57.1 | active | fixture | 2 |
| C-257 | scalar 错误运算符在两种 target 一致地求值 | 0.57.1 | active | fixture | 1 |
| C-258 | 具名调用和 lambda 在两种 target 一致地求值 | 0.57.1 | active | fixture | 6 |
| C-259 | variant 构造器引用构造的值在两种 target 由 match 一致地消去 | 0.57.1 | active | fixture | 1 |
| C-260 | 声明族在两种 target 一致地编译和运行 | 0.57.1 | active | fixture | 5 |
| C-261 | 包括负零符号的规范 float 值在两种 target 显示相同 | 0.57.1 | active | fixture | 1 |
| C-262 | fmt 稳定的字符串转义和空字符串在两种 target 一致地求值 | 0.57.1 | active | fixture | 1 |
| C-263 | 恢复节点绝不出现在获准程序中；损坏文件仍报告首个错误之后的问题 | 0.57.1 | active | fixture | 1 |
| C-264 | scalar 字段 optional chain 在两种 target 一致地产生 some/none | 0.57.1 | active | fixture | 1 |
| C-265 | guard 语句的通过/raise 路径在两种 target 行为一致 | 0.57.1 | active | fixture | 3 |
| C-266 | guard let 绑定成功载荷，在失败极性上 raise，两种 target 相同 | 0.57.1 | active | fixture | 1 |
| C-267 | 二元运算符 API 包括有符号除法裁定，在两种 target 一致地求值 | 0.57.1 | active | fixture | 1 |
| C-268 | 调用参数位置的 `_` 在两种 target 均为检查时 E046 | 0.57.1 | active | fixture | 0 |
| C-269 | 绑定至 let 的嵌套 variant match 在两种 target 一致地执行；对象缺少的构造器模式在两者均为检查时 E048 | 0.57.1 | active | fixture | 1 |
| C-270 | dialect epoch 14 移除的 stdlib 拼写产生指出去向的检查时错误 | 0.57.1 | active | fixture | 0 |
| C-271 | 对带句柄载荷，`carrier ?? call!` 在两种 target 返回 fallback 的载荷，而非其 Result 块 | 0.57.1 | active | fixture | 1 |
| C-272 | `fs.list_dir` 无论目录大小，都在两种 target 按字节排序返回全部条目 | 0.57.1 | active | fixture | 1 |
| C-273 | fs 写侧底层以 native 的 std::io Display 文本回答，不丢弃 fd_write 的 errno | 0.57.1 | active | fixture | 1 |
| C-274 | 可失败 callback 使 fs streaming walk 也可失败，并在首个 err 停止 | 0.57.1 | active | fixture | 1 |
| C-275 | http 响应头 API 对字段名不区分大小写，在两种 target 回答一致 | 0.57.1 | active | fixture | 1 |
| C-276 | 堆元素的 String 键 `list.sort_by` 在两种 target 一致地排序并共享拥有 | 0.57.1 | active | fixture | 1 |
| C-277 | 由统一固定、仅返回类型泛型的构造器，即使常量折叠消去固定上下文，也在两种 target 一致地构建和运行 | 0.57.1 | active | fixture | 1 |
| C-278 | rope 族在 dialect epoch 14 离开 stdlib | 0.57.1 | active | fixture | 0 |
| C-279 | 零次循环在任一 target 均不执行循环体的效应或 trap | 0.57.1 | active | fixture | 1 |
| C-280 | kernel-conformance 族在两种 target 遵循机器检查的 λ_almd 语义 | 0.57.1 | active | fixture | 1 |
| C-281 | 裸 Result 构造器与参数透传 effect 尾部按 kernel 语义一致地运行 | 0.57.1 | active | fixture | 3 |
| C-282 | matrix.get 在两种 target 越界返回 none；行归约返回其单位元 | 0.57.1 | active | fixture | 1 |
| C-283 | 通过 Fn 类型 local 的调用解析至该 local，绝非同名顶层 fn | 0.57.1 | active | fixture | 1 |
| C-284 | 由用户 effect fn callback 驱动的可失败 list HOF 在两种 target 一致地运行 | 0.57.1 | active | fixture | 8 |
| C-285 | regex.captures 在下标0返回整个匹配，`none` 只表示没有任何匹配 | 0.57.1 | active | fixture | 2 |
| C-286 | `??` 读取来源；Option 在消去后仍存活且可读 | 0.57.1 | active | fixture | 1 |
| C-287 | 带 none 分支的尾部堆结果 if/match 对 record/list 载荷执行所选分支 | 0.57.1 | active | fixture | 1 |
| C-288 | rich-variant list 上的 list.enumerate 在两种 target 执行 | 0.57.1 | active | fixture | 1 |
| C-289 | Float 相等族在两种 target 以 f64 相等比较 | 0.57.1 | active | fixture | 1 |
| C-290 | fs text reader 在两种 target 一致地拒绝非法 UTF-8；bytes reader 保留原始字节 | 0.57.2 | active | fixture | 1 |
| C-291 | 每个错误通道运算符在两种 target 与其定义函数一致 | 0.57.1 | active | fixture | 2 |
| C-292 | reference-suite 符合性语料跨 target 逐字节一致地运行 | 0.57.2 | active | fixture | 48 |
| C-293 | 两层容器嵌套逐字节一致地运行；第三层及 map 值形状为固定 wall 拒绝 | 0.57.2 | active | fixture | 6 |
| C-294 | 零 compute budget 在所有路径接受内联无循环被调用函数 | 0.57.2 | active | fixture | 2 |
| C-295 | guard-lowering 的 box pattern 经 checker 判定穷尽的 match 仍可在 native 构建 | 0.57.2 | active | fixture | 1 |
| C-296 | 生成的类型×操作矩阵跨 target 逐字节一致地运行 | 0.57.2 | active | fixture | 70 |
| C-297 | 闭包返回带别名 list 字段的 flat_map 在两种 target 保持 drop 平衡 | 0.57.2 | active | fixture | 1 |
| C-298 | Codec 符合性语料跨 target 逐字节一致地往返 | 0.57.2 | active | fixture | 17 |
| C-299 | hash 摘要跨 target 逐字节一致 | 0.58.0 | active | fixture | 1 |
| C-300 | Option-scalar 值的 map 字面量跨 target 运行，无泄漏 | 0.59.0 | active | fixture | 2 |
| C-301 | 全 String map 的 map.upsert 跨 target 运行 | 0.59.0 | active | fixture | 1 |
| C-302 | 数值确定性族首项：Float 结果仅由程序决定 | 0.58.0 | active | fixture | 6 |
| C-303 | Float 操作各自只舍入至最近偶数一次——不收缩运算、无 FMA | 0.58.0 | active | fixture | 2 |
| C-304 | 保留次正规 float——任何 target 都不 flush-to-zero | 0.58.0 | active | fixture | 1 |
| C-305 | 超越函数精度界：sqrt 正确舍入，exp/log/log2/log10/sin/cos/tan/pow 在1 ulp以内 | 0.58.0 | active | fixture | 1 |
| C-306 | 有符号零：IEEE 传播，相等忽略符号，min/max 将 -0 排在 +0 之前（IEEE 754-2019） | 0.60.0 | active | fixture | 2 |
| C-307 | Float 的 int.from_saturating 向零截断并饱和，NaN 为0；from_checked 族仅精确值或 none | 0.58.0 | active | fixture | 2 |
| C-308 | @bounded 是不改变任何类型/值的函数属性——bounded 函数仍是普通函数 | 0.60.0 | active | fixture | 1 |
| C-309 | 是子集而非方言：程序有无 @bounded 的可观察行为相同 | 0.60.0 | active | fixture | 2 |
| C-310 | @bounded 函数只接受 range 边界为编译时常量的计数循环（E070） | 0.60.0 | active | fixture | 0 |
| C-311 | @bounded 函数的计数循环内禁止分配（E071） | 0.60.0 | active | fixture | 0 |
| C-312 | @bounded 函数的循环内拒绝 break/continue（E072） | 0.60.0 | active | fixture | 0 |
| C-313 | bounded profile 拒绝递归——可达调用图为 DAG（E073） | 0.60.0 | active | fixture | 0 |
| C-314 | @bounded 函数只调用 @bounded 函数和一阶纯 stdlib 成员（E074） | 0.60.0 | active | fixture | 0 |
| C-315 | @bounded 函数拒绝运行时长度的堆构造（E075） | 0.60.0 | active | fixture | 0 |
| C-316 | @bounded effect fn 只可使用标准输出；其他 effect 模块、触及 host 的模块及 fan 均拒绝（E076） | 0.60.0 | active | fixture | 0 |
| C-317 | @bounded 函数暂时拒绝 Float 操作（E077） | 0.60.0 | active | fixture | 0 |
| C-318 | @bounded 函数的计数循环体内拒绝提前退出（E078） | 0.60.0 | active | fixture | 0 |
| C-319 | 被捕获并修改的 var List 是经闭包共享的存储 | 0.60.0 | active | fixture | 12 |
| C-320 | budget cut 执行退出记账：耗尽始终为 Err，region 独立，cut 放置位置不可观察 | 0.60.0 | active | fixture | 2 |
| C-321 | fan prefetch 拆分（全启动、按分支顺序 await、放弃败者）不可观察 | 0.60.0 | active | fixture | 1 |
| C-322 | 构造器是函数值，包括内置三种 | 0.60.0 | active | fixture | 1 |
| C-323 | or pattern：分支各候选以无 binder 并集匹配 | 0.60.0 | active | fixture | 7 |
| C-324 | 分支的 mut-param 转发：折叠为值流或拒绝，绝不静默出错 | 0.60.0 | active | fixture | 2 |
| C-325 | bytes.push 增长为摊销：保留量线性，大规模仍逐字节一致 | 0.60.0 | active | fixture | 1 |
| C-326 | 分支选择的 mut-fn 调用将写回提升到分支外：gzip-inflate 形状按结构降低 | 0.60.0 | active | fixture | 1 |
| C-327 | env.sleep_ms 在所有路径暂停后返回 | 0.60.1 | active | fixture | 3 |
| C-328 | http string 族在 native/embedded-wasm 路径回答一致 | 0.61.1 | active | fixture | 0 |
| C-329 | env.set 在所有路径由 env.get 一致地观察，p1 shim 提供 environ/args 操作 | 0.62.0 | active | fixture | 1 |
| C-330 | http string 族通过 p3 component 的 async-lowered 交换在 stock runtime 回答 | 0.62.0 | active | fixture | 0 |
| C-331 | zlib 在 wasm 上 self-host：解码逐字节一致，编码精确往返但字节依赖路径 | 0.62.0 | active | fixture | 1 |
| C-332 | list-rest pattern：prefix 之后的 tail 在所有路径一致地绑定为 list | 0.62.0 | active | fixture | 2 |
| C-333 | as pattern：整体值与其自身解构并列绑定，所有路径一致 | 0.62.0 | active | fixture | 1 |
| C-334 | 闭包调用对借用堆参数取 +1：RC-3 的被调用方拥有约定经 call_indirect 仍成立 | 0.62.0 | active | fixture | 4 |
| C-335 | path.extension 在两种 target 将开头 dot 视为隐藏文件标记而非扩展名 | 0.62.0 | active | fixture | 2 |
| C-336 | 每个测试在两种 target 从重新初始化的可变模块 global 开始 | 0.57.1 | active | fixture | 0 |
| C-337 | bytes 搜索/编辑/谓词族在所有路径回答一致 | 0.62.0 | active | fixture | 2 |
| C-338 | int bit 族、float32.from_bits、Int -> Float32 转换在所有路径一致 | 0.62.0 | active | fixture | 2 |
| C-339 | testing.assert_snapshot 不匹配在两种 target 以 T18 形式 abort，也包括测试块内 | 0.62.0 | active | fixture | 1 |
| C-340 | 即时求值 *_or 族——option/result.unwrap_or——在两种 target 选择前求值默认值 | 0.62.0 | active | fixture | 2 |
| C-341 | matrix.from_bytes_f16_le / _f32_le / _f64_le 在数据容不下时返回 none；按缓冲区侧判断，因此接近 i64::MAX 的 offset 为 none | 0.62.0 | active | fixture | 2 |
| C-342 | bytes.copy_from 在长度或 offset 为负时两种 target 都不复制任何内容 | 0.62.0 | active | fixture | 1 |
| C-343 | 对声明 Result 的普通被调用函数的尾部 `!` 在提升的 effect fn 中产生载荷 | 0.63.0 | active | fixture | 1 |
| C-344 | Result 绑定后、调用参数块内的 guard-else-err 在所有路径运行 | 0.63.0 | active | fixture | 1 |
| C-345 | 作为调用参数传入的 Result 在每个参数位置保留 Result，包括泛型参数 | 0.63.0 | active | fixture | 3 |
| C-346 | 堆载荷 ?? 在既有路径作为调用参数和 match 对象降低 | 0.63.0 | active | fixture | 1 |
| C-347 | 对已知字面量进行运算符往返，在两种 target 打印该字面量 | 0.63.0 | active | fixture | 1 |
| C-348 | 按字节索引的字符串 slice 验证 UTF-8 边界 | 0.63.0 | active | fixture | 1 |
| C-349 | fan 分支内的 `??` 在两种 target 保留默认值 | 0.63.0 | active | fixture | 1 |
| C-350 | process.exit 透传退出 status 0..=255，拒绝其他代码；WASI preview-1 构建以 wall 拒绝 126..=255 | 0.63.0 | active | fixture | 3 |
| C-351 | 字面量 process 退出码按退出 status 定义域检查 | 0.63.0 | active | fixture | 0 |
| C-352 | or pattern guard 按源码顺序对每个匹配候选求值一次 | 0.63.0 | active | fixture | 3 |
| C-353 | matrix kernel 的形状前提在两种 target 是已定义 abort | 0.63.0 | active | fixture | 1 |
| C-354 | matrix 行范围像 list.slice 一样夹取，part count 为0或更小则为空结果 | 0.63.0 | active | fixture | 1 |
| C-355 | matrix 取负 `-m` 为 IEEE 取负，因此两种 target 均翻转零的符号 | 0.63.0 | active | fixture | 1 |
| C-356 | Value 中的非有限 Float 在所有路径序列化为 JSON null | 0.63.0 | active | fixture | 1 |
| C-357 | value.get_int 仅针对 Int：Float 字段为 none，绝不截断 | 0.63.0 | active | fixture | 1 |
| C-358 | float.round、math.choose、int.rotate_*、math.log_gamma 在所有路径整个定义域上精确 | 0.63.0 | active | fixture | 4 |
| C-359 | epoch 前 timestamp 向下取整到所属日期，%Y 显示任意年份 | 0.63.0 | active | fixture | 1 |
| C-360 | datetime.parse_iso 应用 UTC offset，并指名拒绝越界字段 | 0.63.0 | active | fixture | 2 |
| C-361 | url.parse 在 host:port 拆分后验证 host，并指名拒绝范围外 authority | 0.63.0 | active | fixture | 1 |
| C-362 | `scoped` 块的可观察行为就是其块体的行为——region 不可观察 | 0.64.0 | active | fixture | 1 |
| C-363 | `scoped fn` 在 region 内外计算相同答案 | 0.64.0 | active | fixture | 1 |
| C-364 | scoped fragment 之外的形状在两种 target 一致地于检查时拒绝 | 0.64.0 | active | fixture | 0 |
| C-365 | 具有显式 conformance 的泛型 protocol 分派到唯一声明实现，两种 target 相同 | 0.64.0 | active | fixture | 1 |
| C-366 | http call handle：每次调用的限制触发时指出自身名称，cancel 关闭连接，poll 绝不阻塞 | 0.64.0 | active | fixture | 0 |
| C-367 | http.serve 在内嵌 wasm 路径上按 native 的单实例、顺序语义运行，返回相同 status、header 集合和 body | 0.64.0 | active | fixture | 0 |
| C-368 | http router：handler 是函数，由最具体的路由响应；拒绝损坏的表，404/405/400 由 router 产生，两种 target 相同 | 0.64.0 | active | fixture | 0 |
| C-369 | lambda 的失败通道承载其 ! 操作数一致的错误类型；String 通道以类型化错误的插值文本承载它，两种 target 相同 | 0.65.0 | active | fixture | 1 |
| C-370 | 会拆分请求的 http header，或由 client 管理的 header，在所有路径上均以相同 err 拒绝 | 0.66.0 | active | fixture | 0 |
| C-371 | Float32 算术在每条路径上都将每个结果舍入到 binary32 | 0.66.0 | active | fixture | 1 |
| C-372 | Float32 显示为能往返得到同一 binary32 的最短十进制表示 | 0.66.0 | active | fixture | 2 |
| C-373 | var 使用 let 的模式解构，绑定的每个名称都可变，两种 target 相同 | 0.67.0 | active | fixture | 1 |
| C-374 | 内嵌 wasm host 上的 process 返回与 native 相同的结果；启动子进程的独立 artifact 携带私有 almide:process/spawn import，在未定义它的环境中加载时被拒绝 | 0.67.0 | active | fixture | 0 |
| C-375 | 为 stock wasm 构建的 serve 形程序是 wasi:http/handler@0.3.0 export，wasmtime serve 无需标志即可运行，返回与 native 相同的结果 | 0.67.0 | active | fixture | 0 |
| C-376 | Region 保持其查看的 Bytes，因此 block 的生命周期覆盖该 view 的所有使用 | 0.67.0 | active | fixture | 1 |
| C-377 | net tcp：loopback、handle 编号、EOF、close、is_open、tcp_local_port 在 native、内嵌 wasm 路径和 stock WASI 0.3 component 上一致 | 0.68.0 | active | fixture | 0 |
| C-378 | net tcp：所有失败均为同一表中的无 errno 分类文本，在 native、内嵌 wasm 路径及 stock WASI 0.3 component 上相同 | 0.68.0 | active | fixture | 0 |
| C-379 | net tcp：stock WASI 0.3 component 由 guest 强制 timeout 与 tcp_available，返回与 native 和内嵌 wasm 路径相同的结果 | 0.68.0 | active | fixture | 0 |
| C-380 | 由 object 的键值对构建的 Map 每键保留一项；重复 object key 保持首次位置、采用最后值，所有路径相同 | 0.68.0 | active | fixture | 1 |
| C-381 | test block 外的 testing assert 失败时，在两种 target 上均按 T18 形式 abort | 0.68.0 | active | fixture | 8 |
| C-382 | net unix socket 与 shm file：bytes、接收的 descriptor、readiness 和 shm 内容在 native 与内嵌 wasm 路径一致；stock artifact 拒绝这些功能 | 0.68.0 | active | fixture | 0 |
| C-383 | net unix socket 与 shm file：所有失败在 native 与内嵌 wasm 路径上均为相同文本的 err；程序未持有的 descriptor 在任何 system call 前被拒绝 | 0.68.0 | active | fixture | 0 |
| C-384 | testing.assert_aborts 仅在新子进程中运行其无捕获 closure，当且仅当子进程以已定义 abort 结束、且文本包含预期子串时通过；native 与 wasm 相同 | 0.68.0 | active | fixture | 0 |
| C-385 | http client 自行对请求 body 进行 framing；所有路径都丢弃调用方的 Content-Length header | 0.68.0 | active | fixture | 0 |
| C-386 | record 形式的 fan block 是其各 field 的 record literal，各 field 作为并行兄弟运行 | 0.68.0 | active | fixture | 2 |
| C-387 | 标识符是以 NFC 保存的 UAX #31 名称，拒绝兼容字符；无大小写的名称仅凭声明归入类型侧，非 ASCII 名称在所有路径上逐字节行为相同 | 0.68.0 | active | fixture | 1 |
| C-388 | 裸调用指向自身 scope 的定义，绝不指向入口程序或其他 module 中同名 fn | 0.68.0 | active | fixture | 2 |
| C-389 | http.serve 同时读取最多 64 个连接，对 10 s 内未完整的 request head 返回 408，因此慢 client 只占用自身连接 | 0.69.0 | active | fixture | 0 |
| C-390 | zlib decoder 限制输出，超过上限便停止，所有路径使用同一 err 文本 | 0.69.0 | active | fixture | 1 |
| C-391 | json.parse 将嵌套限制为 1000 层，所有路径使用同一 err 文本 | 0.69.0 | active | fixture | 1 |
| C-392 | fs.open_root 族将每个 path 保持在 root 内；通过 ..、绝对 path 或 symlink 离开的 path，在所有 target 上以同一消息拒绝 | 0.69.0 | active | fixture | 1 |
| C-393 | [permissions] fs 解析 symlink 后，将每个 fs 调用限制在获准 directory 内；native 与内嵌 wasm host 使用同一拒绝方式 | 0.69.0 | active | fixture | 0 |
| C-394 | random.bytes 从 platform CSPRNG 获取 n bytes；n = 0 为空，负 n 在所有路径上以同一文本 abort | 0.69.0 | active | fixture | 2 |
| C-395 | 对超出 engine 文法或超过 20,000 states 的 pattern，regex.new abort、regex.compile 返回 err，所有路径理由相同 | 0.69.0 | active | fuzz(300) | 3 |
| C-396 | fs.walk 与 fs.glob 将 symlink 列为 entry，但绝不进入其内部，所有路径相同 | 0.69.0 | active | fixture | 0 |
| C-397 | fs.create_temp_file 与 fs.create_temp_dir 以熵生成的名称排他创建新 entry，仅其 owner 可访问 | 0.69.0 | active | fixture | 0 |
| C-398 | path.join_within 只接受连接后仍位于 base 内的 child，拒绝其他 child | 0.69.0 | active | fixture | 1 |
| C-399 | io.write 与 io.write_bytes 在所有 target 和 host 上精确输出其 bytes | 0.68.0 | active | fixture | 1 |
| C-400 | json.parse 拒绝孤立 surrogate、未知或不完整 escape、原始控制字符、noncharacter 及未终止字符串，绝不删除输入 | 0.69.0 | active | fixture | 2 |
| C-401 | json.parse 恰好接受一个 RFC 8259 值；重复 key、尾随 data、未加引号的 key、尾随 comma、缺失分隔符、非 JSON 空白及畸形数值均为 err | 0.69.0 | active | fixture | 3 |
| C-402 | url.parse 按 WHATWG 清理输入，在开头验证 scheme，拒绝 backslash、控制字符、host 中的 % 和非规范数值 host，并移除包括 %2e 的 dot segment | 0.69.0 | active | fixture | 2 |
| C-403 | url.resolve 按 url.parse 规则清理 reference，对所有 path 应用同一 dot-segment 规则，包括 %2e | 0.69.0 | active | fixture | 1 |
| C-404 | http.query_params 对 target 首个 ? 后的内容使用 url.parse_query；所有路径同一 decoder，保持每个 pair 的顺序，非法 escape 为 err | 0.69.0 | active | fixture | 1 |
| C-405 | socket host 对 RFC 9112 拒绝的 request head 返回 400：孤立 LF/CR、畸形 request line、非 origin target、header colon 前空白、无 colon 行、obs-fold，以及缺失或重复 Host | 0.69.0 | active | fixture | 0 |
| C-406 | router 及其前置 middleware 读取同一规范 path；非规范 path，以及解码后成为 traversal 的 parameter 均为 400 | 0.69.0 | active | fixture | 0 |
| C-407 | 除非 handler 已设置该 field，每个 http.serve response 都带有 X-Content-Type-Options: nosniff | 0.69.0 | active | fixture | 0 |
| C-408 | buffered http client 拒绝在 framing 完成前结束的 response，并像 call-handle client 一样限制 response head | 0.69.0 | active | fixture | 0 |
| C-409 | handler 的 err 不会到达 client；固定 500 body 标明 request 编号，stderr 在该编号下以引用形式记录 err | 0.69.0 | active | fixture | 0 |
| C-410 | datetime.parse_iso 明确以年份为由拒绝 0..=9999 之外的年，而非返回回绕的时刻 | 0.69.0 | active | fixture | 1 |
| C-411 | base64.decode 与 decode_url 仅接受各自 alphabet 的规范带 padding 形式 | 0.69.0 | active | fixture | 1 |
| C-412 | 大小写映射绝不将非 ASCII scalar 折叠为 ASCII；string.to_upper、to_lower、capitalize 保持该 scalar 不变 | 0.69.0 | active | fixture | 2 |
| C-413 | int.parse 与 float.parse 仅接受去除首尾 ASCII 空白后的有限十进制文本 | 0.69.0 | active | fixture | 6 |
| C-414 | int.from_checked 与 int.from_saturating 补全 Float 到 Int 的三种转换 | 0.69.0 | active | fixture | 1 |
| C-415 | random.int、float、choice、shuffle 在 native 与内嵌 wasm 路径上使用 OS 熵；同一 clock tick 内取样不同，同时启动的 thread 获取不同 stream | 0.69.0 | active | fixture | 0 |
| C-416 | net 读取与接收的 descriptor 有界；读取绝不按请求长度预分配，每个 Unix socket 最多等待 28 个尚未取走的 descriptor | 0.69.0 | active | fixture | 0 |
| C-417 | process.kill 通过 kill(2) 请求 kernel，拒绝 1..=2147483647 外的 pid，返回固定 err 文本，并像 kill command 一样受 [permissions] proc 限制 | 0.69.0 | active | fixture | 0 |
| C-418 | runtime error 文本转义其嵌入的 user data 的控制字符和 bidi 字符，所有路径使用同一规则、相同 bytes | 0.69.0 | active | fixture | 4 |
| C-419 | 非 2 的幂次基数的 bigint 文本限 4300 位，并使用固定文本；2 的幂次基数无限制。bigint.pow 在计算前拒绝必定超过 30 * 2^28 bits 的结果 | 0.69.0 | active | fixture | 1 |
| C-420 | list.range_step 列出严格位于 end 之前的每个 Int start + i * step，计数在 Int 边界不回绕 | 0.69.0 | active | fixture | 1 |
| C-421 | env.set 在所有路径上以相同 err 拒绝空名称、名称中的 `=` 或 NUL，以及值中的 NUL，不保存任何内容 | 0.69.0 | active | fixture | 1 |
| C-422 | hash.hmac_sha256/512 以 Secret[Bytes] key 为首参数，在所有路径上返回与 RFC 4231 vector 相等的 Secret[Bytes] tag；is_valid_ 只接受正确 tag | 0.69.0 | active | fixture | 1 |
| C-423 | bytes/string.is_equal_constant_time 无数据依赖分支地返回相等判断 | 0.69.0 | active | fixture | 1 |
| C-424 | md5 和 sha1 不在 stdlib 中；调用 hash.md5、md5_hex、sha1、sha1_hex 产生检查阶段错误，hint 指向 SHA-256 | 0.69.0 | active | fixture | 0 |
| C-425 | 值无论在何处呈现都通过其类型声明的 repr 输出；Secret 在所有路径上显示为 Secret(***) | 0.69.0 | active | fixture | 1 |
| C-426 | Secret[String] 和 Secret[Bytes] 上的 == 比较揭示的值；其他 Secret，以及包含它们的 variant 或 map，均为 E103 | 0.69.0 | active | fixture | 1 |
| C-427 | Codec record 从其值的 wire 形式解码 Secret field，不派生 encode（E023） | 0.69.0 | active | fixture | 1 |
| C-428 | record 的 `@skip` field 既不解码也不编码，`@skip_decode` field 从不解码但仍编码；两者都需要 default | 0.69.0 | active | fixture | 1 |
| C-429 | 派生的 `T.decode_strict` 以 `unknown key '<k>'` 拒绝 record 解码 field 之外的首个 key；`T.decode` 保持宽松 | 0.69.0 | active | fixture | 2 |
| C-430 | compiler 不读取的 record field 属性会产生检查错误（E053）；field 属性词汇为 @builtin、@location、@skip、@skip_decode | 0.69.0 | active | fixture | 0 |
| C-431 | fs 整文件读取函数读取至多 1 GiB 的普通 file；device、FIFO、socket 不阻塞而报 Not a regular file，较大 file 报 File too large，所有路径相同 | 0.69.0 | active | fixture | 1 |
| C-432 | runtime 支持的值（HttpRequest、HttpResponse、HttpCall）无论单独还是嵌套，都按其声明的 repr 插值，所有路径相同；http repr 统计 header 数量，绝不显示 header value | 0.69.0 | active | fixture | 2 |
| C-433 | value.set_path 拒绝构建比 json.parse 接受深度更深的值，所有路径返回 parse 的同一 err 文本 | 0.69.0 | active | fixture | 1 |
| C-434 | http.serve handler 内的 runtime abort 仅使该 request 失败：固定 C-409 的 500、一行 stderr，server 继续服务 | 0.69.0 | active | fixture | 0 |
| C-435 | `Codec` 是 `Encode + Decode`；`: Decode` 类型仅派生 `decode`（record 还派生 `decode_strict`），`: Encode` 类型仅派生 `encode`，输入类型不跳过任何 field | 0.69.0 | active | fixture | 1 |
| C-436 | `http.decode_json[T](req)` 只将 request body 解码为输入类型；`Codec` 的 T 为 E110，T 之外的 key 为 `400 Bad Request: unknown key '<k>'` | 0.69.0 | active | fixture | 2 |
| C-437 | process.start 的子进程若既未 wait 也未 stop，则在程序因 main return、process.exit 或 effect error 退出时被 stop | 0.69.0 | active | fixture | 0 |
| C-438 | env.secret(name) 将环境变量读取为 Option[Secret[String]]：值与 env.get 相同，显示为 Secret(***)，以常量时间比较 | 0.69.0 | active | fixture | 1 |
| C-439 | env.vars 按名称为 key、按名称排序列出环境，反映程序自身的 set 和 remove；env.remove 取消该名称或按 env.set 同样方式拒绝，所有路径 bytes 相同 | 0.69.0 | active | fixture | 1 |
| C-440 | 表中每个 compile-cost budget 都成立：其计数上限不能由源代码提高，并以规定诊断结束 | 0.69.0 | active | fixture | 0 |
| C-441 | uuid.v7 带 version 7 和 RFC 9562 variant，写为 8-4-4-4-12 小写十六进制；所有路径上每个值严格大于前一个，同一 millisecond 内亦然 | 0.69.0 | active | fixture | 1 |
| C-442 | net udp 与 lookup_host：binding、datagram 边界、max-bytes 规则、发送者 address 及 loopback 名称查询在 native 与内嵌 wasm 路径一致 | 0.69.0 | active | fixture | 0 |
| C-443 | net udp 与 lookup_host：所有失败均为 net 同一表中的无 errno 分类文本，在 native 与内嵌 wasm 路径相同 | 0.69.0 | active | fixture | 0 |
| C-444 | 嵌套于被呈现值中的 Bytes 值显示为其 byte 值的 list，所有 target 相同 | 0.69.0 | active | fixture | 1 |
| C-445 | 内嵌 wasm host 与 native 一样，在运行因 main return、process.exit 或 effect error 结束时，stop 既未 wait 也未 stop 的 process.start 子进程 | 0.69.0 | active | fixture | 0 |
| C-446 | Region 以 memory 地址宽度的 (base, len) 跨越 wasm host 边界；导出的 Region 在 host 释放地址前持续供 host 写入，跨边界借出的 Region 原地读写 | 0.69.0 | active | fixture | 0 |
| C-447 | fs.append_bytes 在所有路径上都是真正的 append；绝不读取 file，因此任意大小的 file 都接收追加 bytes，长度恰好增加其字节数 | 0.69.0 | active | fixture | 1 |
| C-448 | WASI 0.3 component 上的 net udp 与 lookup_host 通过 wasi:sockets@0.3，对 C-442、C-443 的程序返回与 native 逐字节相同的结果 | 0.69.0 | active | fixture | 0 |

