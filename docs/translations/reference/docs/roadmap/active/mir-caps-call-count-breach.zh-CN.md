<a id="reviving-the-monkey-regression-suite-what-spec-actually-is"></a>

# 恢复 monkey 回归测试集：`spec/` 究竟是什么

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](mir-caps-call-count-breach.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](mir-caps-call-count-breach.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](mir-caps-call-count-breach.zh-CN.md)

> [正本](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/mir-caps-call-count-breach.md)的非规范历史参考快照，固定提交：35297a612fb29e3f78f6587aba253aa91d51f466。不自动同步。未在此独立验证原文主张、数量、诊断或当前复现状态；未运行示例、编译器命令或关卡。以正本为准。历史提案不授权恢复测试集、修改编译器或改变关卡政策。

原文说明（仅供显示）：2026 年 8 月 monkey 测试集 caps 计数诊断、已找回的公开来源，以及尚待完成的当前复现。

<a id="reading-status-2026-10-31"></a>

## 阅读状态（2026-10-31）

本文保留 **2026-08-08 的诊断**，包括 15 个 wall 的统计、两份 MIR>IR 报告以及被否决的修复方案。这些并非新近复现的数量，也不表示今天的 CI 失败。原始语料可以从 [公开历史 `48621335`](https://github.com/almide/almide/tree/48621335e54af937dd4bb8432ba14c525c310842/spec/regression) 获取，后续的 [回退理由](https://github.com/almide/almide/commit/a5ebd06937b533dd2609a5bdc4a7daa961b4c6a7) 也已公开。恢复这些历史输入不需要私有归档。

当前复现仍待完成：最小示例通过 frontend 的 `almide check`，但该命令不会运行 `classify_corpus` 的 MIR/IR 调用计数安全网。在宣称缺陷已修复或仍可复现之前，需要固定版本的 classifier 构建及报告。不要仅为了让文档看起来完整而恢复测试集或扩大只能缩减的 baseline。以下设计选项仍是历史资料，不是新获授权的编译器或关卡政策决定。

<a id="original-report"></a>

## 原始报告

**发现时的状态**：未解决。2026-08-08，在恢复 monkey 回归测试集时发现。该测试集（`MONKEY_BUGS.md`，41 个文件，2026-04-05 在 v0.11.4 编写）一直放在仓库根目录的 `_scenarios/` 中，由于 CI 仅运行 `almide test spec/`，长期未执行而荒废。

测试集本身正确且通过：全部 41 个文件在两个目标上均成功运行（加入它们后，`almide test` 为 393 个文件、0 失败）。把它们移入 `spec/` 才会失败，原因正是本文要说明的。

<a id="spec-is-the-v0-corpus-not-a-test-directory"></a>

## `spec/` 是 v0 语料库，而非测试目录

`proofs/corpus-wall.sh`（CI 作业 `Trust Spine (v1)`、`make verify-trust`）将 `$ROOT/spec` 的整个目录送入真实 frontend → MIR，并强制三个性质。其中两个是**只能缩减的棘轮**。因此，往 `spec/` 加文件并非没有代价：任何 v1 lowering 尚未覆盖的新源码形态都会扩大棘轮，关卡明确给出政策：

```
WALLED-REAL RATCHET FAIL: function(s) walled that are NOT in proofs/walled-real-baseline.txt —
ship the lowering in the same change (adding entries is a reviewed regression, not a fix)
```

上述诊断的译文：WALLED-REAL RATCHET FAIL——出现不在 proofs/walled-real-baseline.txt 中的被 wall 阻挡函数；应在同一变更中交付 lowering（新增条目是需要审查的退步，不是修复）。

恢复的 41 个文件在 7 个文件中新增 **15 个 walled-real 函数**：

| 文件 | 新增 wall 数 |
|---|---|
| `scenario8_error_taxonomy_test.almd` | 3 |
| `scenario3_pipeline_test.almd` | 3 |
| `monkey24_variant_advanced_test.almd` | 3 |
| `monkey08_option_result_test.almd` | 2 |
| `monkey05_pattern_match_test.almd` | 2 |
| `monkey13_records_test.almd` | 1 |
| `monkey14_edge_cases_test.almd` | 1 |

它们都是普通 Almide：match 分支 guard、heap-result 的 `match`/`if` 尾部、对未跟踪 Option 操作数使用 `??`、带闭包列表参数的 `list.filter_map`、针对未跟踪对象且分支包含调用的 `match`。每个都是当前 `spec/` 语料未覆盖的真实 v1 lowering 缺口。

<a id="plus-one-hard-breach-mir_calls--ir_calls"></a>

## 还有一种硬性违例：`mir_calls > ir_calls`

其中两个文件还违反了 **caps 可靠性安全网**。它没有 baseline，必须为 0：`record_elided_calls` 标记只能使真正被省略的调用重新计入，因此 MIR 调用数最多只能增长到 IR 调用数；如果超过，说明标记重复计入了已 lowering 的调用，这可能掩盖另一个省略，错误地清除可达 Stdout 的函数的 taint。

```
MIR>IR monkey28_nested_data_test.almd::profile_summary (mir 8 > ir 7)
MIR>IR scenario5_plugin_test.almd::merge_sources        (mir 3 > ir 2)
```

<a id="minimal-repro"></a>

### 最小复现

```almide
type DS = { fetch: fn(String) -> Result[List[String], String] }

effect fn merge(sources: List[DS]) -> List[String] = {
  var all: List[String] = []
  for src in sources {
    let rows = src.fetch("*")!
    all = all + rows
  }
  all
}

test "t" { assert(true) }
```

```
$ ALMIDE_DEBUG_CALL_OPS=1 cargo run -p almide-mir --example classify_corpus -- --out /tmp/o <dir>
[call-op] merge: CallIndirect              <- src.fetch("*")
[call-op] merge: CallFn __list_concat_rc   <- all + rows
[call-op] merge: CallFn __str_concat       <- SPURIOUS: the source has no string concat
[call-count] ir=2
MIR>IR <dir>/a_test.almd::merge (mir 3 > ir 2)
```

上述输出注释的译文：SPURIOUS 表示多余调用；源码没有字符串拼接。

`ir=2` 是正确的（一个 `Call`、一个 `ConcatList`——`count_ir_calls` 对每个 `ConcatList` 节点计数）。第三个 MIR 操作才是缺陷：一个源码中任何位置都没有 `ConcatStr` 节点的函数里，出现了 `__str_concat` 的 `Op::CallFn`。

<a id="root-cause-confirmed-2026-08-08"></a>

### 根因（2026-08-08 确认）

`build_append_slot_subst`，`crates/almide-mir/src/lower/mod_p5_b.rs:513-527`。将累加器循环 lowering 为尾递归 append-slot 形式时，会合成累加器的副本。函数自身的文档注释就说：*“String 累加器通过 `acc + ""` 复制，列表通过 `acc + []` 复制。”*

```rust
let (empty, concat_op) = if matches!(list_ty, Ty::String) {
    (tco_ir(IrExprKind::LitStr { value: String::new() }, Ty::String), BinOp::ConcatStr)
} else {
    (tco_ir(IrExprKind::List { elements: vec![] }, list_ty.clone()), BinOp::ConcatList)
};
let copy = tco_ir(IrExprKind::BinOp { op: concat_op, left: Var(acc_var), right: empty }, ..);
```

这个 `acc + ""` 是**源码中从未有过的 `ConcatStr` 节点**。`try_lower_concat_str`（`lower/calls_p2.rs:140`）将它 lowering 成真实的 `Op::CallFn { name: "__str_concat" }`，于是 `mir_calls` 增加一个调用，而遍历源码函数体的 `count_ir_calls` 没有可计数的节点。因此出现 `mir 3 > ir 2`。对 emitter 插桩直接测得：

```
[sc] __str_concat  ty=String
     left  = Var { id: VarId(7) }
     right = LitStr { value: "" }
```

调查中排除了：`record_elided_calls`（`lower/calls_b.rs:170`——它的 push 对此函数没有打印任何内容）以及 `lower_interp_compound_wall`（`lower/calls_p2.rs:592`——复现中没有字符串插值）。

<a id="the-fix-is-a-decision-not-a-patch"></a>

### 修复需要决策，而不是一个补丁

注入的复制与 `$__mg_take` 属于同类，关卡已经按名字排除后者，理由是它是：

> 编译器注入的槽访问器，而不是任何 IR 调用节点的 lowering：可变全局变量的堆赋值会注入它，却没有 IR 对应项，因此将它计数会误报违反 `mir <= ir`。

但不能简单地把 `__str_concat` 加入该过滤器：真正的源码级字符串 `+` 也会 lowering 为 `__str_concat`，而关卡将每个源码 `ConcatStr` 节点记为一个 ir_call，正是为了继续把这些调用算进去。按名字整体排除会少计真实调用，削弱安全网，与其目的正相反。按侵入性递增，有三种出路：

1. **给注入的复制一个不同名字**（`__str_copy` / `__acc_copy`），让关卡像排除 `__mg_take` 一样准确排除它。会增加一个需要链接的 self-host 符号。
2. **在 IR 一侧计入**——让 `count_ir_calls` 知道 String append-slot 累加器意味着一次合成 concat。输出程序不变，但在关卡内复制了 lowering 的分派规则，恰好就是 `__mg_take` 注释警告的漂移。
3. **不用 `ConcatStr` 进行复制**——给 append-slot lowering 专用复制操作，而不是复用 `x + ""` 这个恒等式技巧。

(3) 最干净：复制不是拼接，只因 `x + ""` 是方便的恒等式才看起来像拼接。但它会改变经过所有权证书验证的 lowering，因此需要单独审查，而不是顺手修改。无论选择哪种方案，验收测试都一样：恢复 `spec/regression/`，观察 `proofs/corpus-wall.sh` 保持 0 违例。

<a id="state-and-proposed-recovery-at-the-time-2026-08-08"></a>

## 当时的状态与拟议恢复步骤（2026-08-08）

`spec/regression/` 已回退；41 个文件及此复现位于工作归档（`../almide-docs-archive/blocked-on-mir-caps-gate/`）和 git 历史（`git log --diff-filter=D -- spec/regression/`）中。

合入测试集是真正需要完成的工作，顺序如下：

1. **消除 caps 计数缺口**（2 个硬性违例）。这是可靠性安全网，必须修复，不能纳入 baseline。
2. **消除 15 个 lowering 缺口，或有意识地将它们纳入 baseline。** 棘轮自身的政策是“在同一变更中交付 lowering”；为 15 个条目建立 baseline 是需要审查的退步，需要决策，而非顺手修改。
3. 恢复 `spec/regression/`——恢复本身就是验收测试。

在此之前，测试集应留在 `spec/` 外，因为加入它会使 `Trust Spine (v1)` 失败，测试再有价值也不能成为可靠性关卡失败的理由。它已经带来的价值就是本文：15 种未覆盖的 lowering 形态，以及一种计数违例，而 `spec/` 在四个月的增长中从未使它们显现。

---

原文注意事项：8 月报告称缺陷未解决，加入测试集会使 trust 关卡失败；10 月阅读状态则明确说明当前复现仍待完成。这些陈述具有不同的历史范围，并非当前失败或已经修复的证据。基于列表的最小示例和 String 合成复制的诊断均按原文保留；本资料未通过重跑 classifier 来解释二者关系。“一种违例”指计数问题，而违反检查的文件／报告有两个。公开历史链接及私有归档引用均保留；后来的状态说明指出不需要该归档。未恢复测试集，也未扩大 baseline。
