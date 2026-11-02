<a id="native-read-only-ownership-review"></a>

# Native 只读所有权审查

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](native-read-borrow-review.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](native-read-borrow-review.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](native-read-borrow-review.zh-CN.md)

> 非规范、固定原文的历史读者快照。原文仍为权威来源，不替换机器输入或可执行 gate。不自动同步。记录的测试结果、数量、实现状态及证明主张仅限原文语境，并未在此独立重跑或确立为当前保证。 与参考编译器比较不等于证明 Almide 分析。记录修订中的不安全临时规则被捕获并修正，但本快照不独立审计所有后续修订。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/native-read-borrow-review.md)

<a id="string-comparisons-2066"></a>

## String 比较（#2066）

旧编译器对借用记录发射 `almide_eq!(t.kind.clone(), want)`，并为字面量比较操作数分配 String。回归测试对这种发射结果失败。CloneInsertion 现在将相等和顺序比较中稳定的 String 操作数包装进现有 string-borrow IR 节点。常规活跃性遍历仍统计两次读取。现有 Borrow renderer 将字面量发射为 str，并借用字段，因此无需 walker 目标分支，也无需修改公开语言。

调用、索引和块保留其值求值路径。尤其 `t.kind == change(t)` 在右侧修改 `t` 时，必须比较原始字段值；跨越该调用借用字段会改变其生命周期。回归在 native 与结构化 WASM 上检验此情况和比较后重用，并覆盖字段／参数／字面量的相等及顺序比较。

<a id="list-fields-in-loops-2068"></a>

## 循环中的 List 字段（#2068）

现有循环证明只对单独 Var 省略 collection clone。Member chain 现在也识别同一个根绑定。若循环体不写入该根，循环就借用列表字段。接着，现有借用元素证明在每次使用都仅读取元素时避免元素复制。若循环体替换父对象、写入其字段或可变借用该 place，则保留 collection snapshot。测试在 native 和结构化 WASM 上检验两个分支及之后对父对象的使用。当字段根拥有所有权、这是它在外层循环之外的最后一次使用，且循环体需要拥有的元素时，consumed-loop 注解改为选择 `into_iter()`。所有权决策仍在 pass 内，walker 只读取该注解。从活跃的借用 collection 消耗元素仍需要拥有的副本。回归在最终消耗循环前调用借用父对象的 helper，并验证 native 与结构化两侧输出。

<a id="reference-milestone"></a>

## 参考实现里程碑

参考编译器克隆于 `../almide-references/rust`，提交为 `0d31508599a7814a7044e9a7a871e3dc5f037753`。`compiler/rustc_mir_build/src/builder/expr/as_rvalue.rs` 中的 closure-capture lowering，仅对无副作用且互不相交的 place 使用 place 构造，而非临时值。该区分为本次更改提供依据：稳定 place 的读取可以借用，而可能使 place 失效的求值保留 snapshot。所有者被修改时，循环保留副本。这是设计比较，不宣称 Rust 的实现证明了 Almide 的分析。

验证记录于 PR 及可执行回归：`tests/native_string_comparison_borrow_test.rs` 和 `tests/for_in_clone_elision_test.rs`。

<a id="validation-of-the-integrated-revision"></a>

## 集成修订的验证

全部 439 个 Almide 测试文件在 native 和默认路径（427 WASM、12 native fallback）上通过。43 个 nanopass 测试、13 个 snapshot、11 个循环所有权测试及一个字符串比较矩阵通过。原始比较 fixture 和只读字段循环 fixture 在修复前失败。借用父对象负例在修正前捕获了一条不安全的临时 move 规则。Codopsy 2.2.0 在阈值不变的情况下报告 codegen 整体为 90/A；提取出的 loop renderer 和 projection helper 让文件保持在 800 行以下。
