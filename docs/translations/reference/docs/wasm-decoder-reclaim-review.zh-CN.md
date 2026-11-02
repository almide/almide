<a id="decoder-scratch-ownership-2046"></a>

# Decoder 临时空间所有权（#2046）

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](wasm-decoder-reclaim-review.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](wasm-decoder-reclaim-review.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](wasm-decoder-reclaim-review.zh-CN.md)

> 非规范、固定原文的历史读者快照。原文仍为权威来源，不替换机器输入或可执行 gate。不自动同步。记录的测试结果、数量、实现状态及证明主张仅限原文语境，并未在此独立重跑或确立为当前保证。 有限测量案例及保留的拒绝集合不证明所有泄漏都不存在，也不证明普遍内存安全。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/wasm-decoder-reclaim-review.md)

结构化 WASM decoder 曾泄漏由 `Try`/`Unwrap` 提取的临时 Result wrapper。双字段记录解码每次调用保留 128 字节：heap watermark 从 1000 次调用时的 196544 字节增长至 8000 次时的 1092544。

参考编译器是克隆于 `../almide-references/koka`、提交为 `3ac4f001ab7277b484d661fdbada1aaf8d01ecbf` 的 Koka。在 `src/Backend/C/Parc.hs` 中，`parcBorrowArg` 为产生的借用参数命名，并安排在消耗之后 drop，以保持求值顺序。Almide 已有相应临时绑定 pass。提取现在参与该 pass：wrapper 获得普通局部所有者，现有 exit plan 在成功或传播时将其释放。payload 仍为 view；保留或返回它时，通过现有路径获取独立 credit。

尾部提取保留现有 carrier-transfer 路径，因此为临时值命名不会隐藏尾调用。move-mode effect lowering 引入的 raw payload 注解仍交由该 ABI 的提取路径处理。回归还检查 200000 次尾调用及可变参数写回。回调尾部及其 `ok(unwrap(call))` carrier wrapper 也保留原形，以保持 fan 的预取和首个错误转发路径。

错误路径测量暴露出另外两处泄漏。在借用成功值旁返回新构造错误的 match，会在函数返回时给新错误一个多余引用。现在值 match 的每个分支向 join 交付一个拥有的 credit：借用分支获取它，新建分支移交它。缺失字段的错误构造也曾让中间拼接前缀保持分配；现在在最终拼接复制后释放该字符串。

回归比较 1000 与 8000 次调用时实际导出的 heap watermark，检查输出，并覆盖平坦记录、嵌套记录列表、成功解码一个字段后的类型错误，以及缺失字段。四种情况均保持平坦。现有 22 个 call-result 所有权测试和 exit 验证也通过。Codopsy 在现有复杂度阈值（20/30）下报告 WASM 源目录为 91/A。

重新生成分配台账后，79 个语料 watermark 下降，没有上升，并保留现有结构化拒绝集合。native-result 所有权和结构化 witness floor 均在未放宽基线的情况下通过。原始 100000 次调用复现，在先前耗尽内存的相同 4 MiB 堆上限下完成。
