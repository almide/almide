说明：所有权 emit 事故历史与已实现的测量基础设施
<a id="wasm-所有権-emit-層の機械化-perceus-drift-の構造的封じ込め"></a>

# WASM 所有权 emit 层的机械化（从结构上遏制 Perceus 漂移）

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](wasm-ownership-emit-mechanization.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](wasm-ownership-emit-mechanization.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](wasm-ownership-emit-mechanization.zh-CN.md)

> 固定原文的非规范读者快照；原文仍为权威，不自动同步。未独立验证实现或执行引用指令。历史状态、计划及比较主张保留原范围。
> [Original source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/wasm-ownership-emit-mechanization.md)

> 原文注记：这是历史事故调查及修订计划，不是当前缺陷复现或内存安全证明。退役 v0 描述、早期 OPEN 标签、后来落地的 #643 修复及已实现计数基础设施有意并存。早期根本机制假设及已撤回 get_or 融合予以保留，不升级为当前诊断。九月订正将重点从缺少 helper 转向重复实现。日期、数量、约数百分比、比较及正确性/预防的原主张未独立重测。未执行引用追踪/构建/CI 指令；此读者副本不授权修复。


<a id="historical-diagnosis-and-implemented-measurement"></a>

## 历史诊断与已实现的测量

原有 `PerceusVerify`/`emit_wasm` 描述针对已退役的 v0 emitter，并非当前 structural Wasm 路径。九月附录也记录了最初 #643 修复已落地；较早的“still OPEN”标题保留调查当时的诊断。

所提实现数量基础设施现已存在：[impl-count-ledger.toml](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/proofs/impl-count-ledger.toml)。它的存在不意味着每个重复实现都已合并，或 emit 层内存安全已完全证明。source-to-witness 与未建模访问的限制仍见[内存安全](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/design/MEMORY-SAFETY.md)。应保留下述事故订正，不要将每个早期假设视为当前缺陷。

状态：active — 提案 + #643 精确分析
负责人：compiler
相关：#643（wasm rc double-free）、#668/#643 历史、project_wasm_frees_activation、project_completeness_roadmap、contract C-066

<a id="問題の本質-perceus-は解析層だけ機械化されている"></a>

## 问题本质：Perceus 仅机械化了分析层

Almide 的 WASM Perceus 分为两层。

1. **分析层（`pass_perceus.rs`）——已机械化。** 所有权分析决定 IR 中放置 `RcInc`/`RcDec` 节点的位置。`PerceusVerify` pass 机械验证 IR 层 inc/dec 平衡（不是 warn，而是 hard-error）。
2. **emit 层（`emit_wasm/*.rs`）——手写。** `UnwrapOr` / `list.get` / record 构造 / `ToOption` … 各操作的实际内存访问以 wasm 手写。**每个 emit 都有义务自行遵守分析层所假定的所有权契约**，但类型与验证器都未表达该契约，仅靠注释和纪律维持。

native 目标将所有权交给 Rust（= 外部 oracle），因此 `unwrap_or` 等自动正确 move/clone。WASM 手动重新实现这些行为，**重实现有缺口就会漂移**。这正是 rt-oracle 注册表存在的原因（约72%的跨目标缺陷是“wasm runtime 偏离 native oracle”）。

<a id="なぜ検証ゲートが防げないか"></a>

### 为什么验证门禁无法防止

`PerceusVerify` 只看见 **(1) 的 IR 层 inc/dec 平衡**。emit 层返回的是“带 +1 的 owned 指针”还是“不带 +1 的 borrow（alias）”，位于 IR 抽象以下，所以**不可见**。因此会有“IR 平衡但运行时 double-free/leak”。`list.get` 的 some-box 对元素执行 `rc_inc` 是 emit 层细节（`calls_list.rs`），验证器看不到。

<a id="643-の精密分析-still-open"></a>

## #643 精确分析（still OPEN）

复现（真正原因是 some-arm；issue 标题中的 none-arm 假设错误）：

```almide
let cs = ["a", "b"]
var out: List[String] = []
var i = 0
while i < 2 {
  let nx = list.get(cs, i) ?? ""     // in-bounds = some-arm
  list.push(out, list.slice(cs, i, i + 1) |> list.join(""))  // 後続 alloc
  i = i + 1
}
```

原代码注释意为：索引界内 = some-arm；后续分配。

- native：正确。wasm：`__rc_dec` 的 double-free sentinel（rc==0 时再次 dec）→ `unreachable` trap。
- 隔离（`ALMIDE_WASM_FREES=1` 为默认值）：
  - **some-arm（界内）+ 后续 alloc + 循环 → TRAP**
  - 仅 none-arm（始终 OOB）→ OK
  - 将 `?? ""` 替换为 `"z"` → OK
  - 将后续 alloc 替换为 `list.push(out, "lit")` → OK（潜伏）
  - 去除循环 → OK

post-Perceus IR（`ALMIDE_DUMP_IR=Perceus`）循环体：

```
bind nx = unwrap_or(runtime_call list_get(cs, idx), "")
rc_inc nx                       // VDecl alias-inc (yields_borrowed_alias(UnwrapOr)=true)
  bind t657 = { bind t656=list_slice(cs..); bind t659=list_join(t656); rc_dec t656; t659 }
  runtime_call list_push(out, t657)
  rc_dec t657
assign i = i + 1
rc_dec nx                       // scope-end
...teardown: rc_dec cs; rc_dec out
```

原 IR 注释意为：VDecl 的别名 inc（UnwrapOr 返回借用别名的判定为 true）、作用域结束时的 dec，以及清理时对 cs 和 out 执行 dec。

已确认事实：

- `nx`（alias-inc + scope-dec）在 IR 上平衡。
- `list.get` 的 some-box 对元素做 `rc_inc`（`calls_list.rs` 的 SHARE），但**不存在对该 box 做 dec 的 IR 节点**（UnwrapOr 的 inner 是中间表达式，未绑定任何变量，因此不附加 temp-dec）。
- `pass_perceus.rs:743` 注释明确说“UnwrapOr 宁可 leak（比 double-free 安全）”——**所以 double-free 违反设计**，有多余 dec 在某处混入。

<a id="確定--非割り当てトレーサで根本機構を特定-2026-06-13"></a>

### 确认——无分配追踪器识别根本机制（2026-06-13）

最初的插桩（`__println_int`）**自身调用 `__alloc`**，改变 free→reuse 模式，掩盖损坏（HEISENBUG）。换成**从固定低位内存 [16..36) 经 `fd_write(stderr)` 写原始字节的无分配追踪器**，观察 `__alloc` 返回的 ptr+size 以及 `__rc_dec` 的 free/double-free，得到 r643 完整时间线：

```
ALLOC 4184/16(cs list) 4208/8(out 空) 4224/4(Some box) 4240/12(slice) ...
FREE 4240(slice) ALLOC 4264/40 ALLOC 4224/12  ← 4224 は #2 で size4・未 free
FREE 4224 ... FREE 4264 FREE 4184 DOUBLE-FREE 4264 → trap
```

追踪标注意为：cs list、空 out、Some box、slice；4224 在 #2 时 size4，尚未 free。

**根本机制**：`out` 的元素槽从 `ptr + LIST_DATA_OFFSET(8)` 开始。容量0的 `out`（4208，size8）的 data[0] **与相邻 Some box（4224）的 header（4216）重叠**。`list.get(cs,i) ?? d` 的 Some box **未 dec 而 leak**（UnwrapOr 的 inner 中间表达式不附加 temp-dec），其 header（size=4）**被覆写成 size≥12**。于是通过 `__alloc` free-list 的 size 检查（`cur.size >= request`），**将仍存活且过小的块（4224）发放出去** → buffer overflow + 后续 rc_dec double-free（rc==0 sentinel）。
即“循环中反复 heap temp（Some box / slice / join）的 leak 损坏相邻块 header，`__alloc` 重新发放存活小块”这一类。

<a id="スケール依存--試した-get_or-融合は不十分--回帰"></a>

### 规模依赖——尝试的 get_or 融合不够且引入回归

将 `list.get(xs,i) ?? d` 融合为 `list.get_or(xs,i,d)`（不造 Some box），**报告的 len2 复现会在两目标上字节一致**，但：

- **len≥3 再次分歧 / len6 trap**——经 slice/join temp 的同类损坏仍在（Some box 只是一个实例）。
- `spec/wasm_cross/alias_combinator_rc` **在 byte gate 分歧/trap**——get_or 的 RC 路径不同于 get→unwrap，破坏既有 alias RC。

→ 所以**融合不是 #643 修复**（已撤回）。真正修复是 emit 层所有权机械化：(1) 确实 dec UnwrapOr/getter 分配的 Option box（或统一成不造 box 的路径），(2) 每轮正确释放反复 heap temp（slice/join 结果）的 RC，(3) 在 `__alloc` free-list reuse 添加 `cur.rc == 0` 不变量，防止“重新发放存活块”（将静默损坏转成 clean trap 的加固）。再给 `spec/churn/` 添加“循环×反复 heap temp”churn fixture。推测性补丁会造成新 leak/regression，故**尚未合并**，#643 作为本 roadmap §1 继续 OPEN。

<a id="関連-591-errorcontext-の-ok-パスが-main-の戻り値を汚染-still-open"></a>

## 相关：#591 error.context 的 OK 路径污染 main 返回值（still OPEN）

同属“emit 层在 IR 以下手写所有权/布局而漂移”。`error.context(ok(3), msg)` 在 native 正确（`3`），wasm 却 **exit=1，并在 stderr 输出带巨量空白的 `Error:` 行**，`m ?? -1` 得 `0`（经 `match` 值为 `3`，正确）。已隔离：

- err 路径（C-098 系 inner=err）native==wasm，正常。
- 仅 OK 路径失效。`error.context` 在 IR 为 `runtime_call almide_rt_error_context`，wasm 经 `dispatch_runtime_fallback("error","context")` → `emit_call` inline arm（`calls.rs` 的 `("error","context")`，手写 `if_i32`：OK 透传输入，err 插入 `": "` 再 wrap）。
- inline arm 的 OK 结果（Result[Int,String]，payload i64@4，12B）与 err 结果（手写8B `[tag][str@4]`）**布局不对称**。`??` 读 `0`、match 读 `3` 的不一致，加上 main 返回 Err，提示 typed-if 结果未完全消费，泄入 main tail/return 的嫌疑。

→ 正途是停止手写 inline，把 native oracle `almide_rt_error_context`（`r.clone().map_err(|e| format!("{ctx}: {e}"))`）变成**真正的 wasm runtime routine**并注册 rt-oracle（同上面的 `emit_extract_owned` 汇聚：让手写 emit 收敛到遵循 oracle 的单一实现）。固定用 fixture `spec/wasm_cross/error_context.almd` 尚未添加（需要创建——当前没有固定此主张的 fixture）。

<a id="提案-emit-層の所有権を単一ヘルパーに集約する"></a>

## 提案：将 emit 层所有权汇聚到单一 helper

停止零散手写 `rc_inc`/payload-load，将“从 container/box 取出 owned 值”汇聚到**一个 emit helper**：

```
fn emit_extract_owned(&mut self, payload_ty: &Ty, offset: u32)
//   load payload at offset; if is_heap_type(payload_ty) { rc_inc }   (ptr 透過)
```

原注释规定：在 offset 加载 payload，若类型在 heap 上则增加 RC（指针透明）。

- `UnwrapOr`（Option some / Result ok）、`Unwrap`、`Try`、`ToOption`、`OptionalChain`、map/list 返回 Option 的 accessor **全部经过同一纪律**。
- 在 box 会被 dec 的路径（consume-then-drop），以新 owned reference 取出 payload = 必须 +1 的契约汇聚到**一处**，从结构上防止 #668 那种“修了 pass-through combinator，却漏掉 UnwrapOr some-arm”的局部修复再次发生。

<a id="提案-静的検証では見えない層を-fuzz--churn-ゲートで塞ぐ"></a>

## 提案：用 fuzz / churn 门禁补静态验证看不到的层

IR 层 `PerceusVerify` 看不到 emit 层借用/所有权，因此：

- 在 `spec/churn/` 添加 **owned-extract → drop 形状（针对 heap 元素容器循环运行含 OOB-default 的 `list.get(...) ?? d`）** churn fixture，在 wall-clock 限制下用 wasmtime 与 native 输出比较。单一 byte fixture 不够（输出正确仍可能潜伏损坏）。
- 若可能，建立**用 runtime counter 比较 native↔wasm rc_inc/rc_dec 总数**的差分 harness（在 IR 以下直接观察所有权漂移）。

<a id="段階"></a>

## 阶段

1. **用 runtime trace 确认 #643 → 单点修复**（emit 层或 pass_perceus，符合“优先 leak”的设计意图）。再用 `spec/wasm_cross/option_fallback_rc.almd`（未添加——需创建）固定回归 + contract。
2. 提取 `emit_extract_owned` helper，迁移所有 payload 取出位置。
3. `spec/churn/` owned-extract fixture +（未来）RC-count 差分 harness。
4. 文档化：在 `emit_wasm/CLAUDE.md` 明确 emit 层必须遵守的所有权契约。

<a id="却下した案"></a>

## 否决方案

- **仅加强分析层**：emit 层 borrow-vs-owned 不进入 IR，再严格的分析也检测不到 emit 层漂移。
- **像 native 一样全交给 Rust**：wasm 不经过 Rust，不能这样做。正因此才需要把所有权纪律汇聚到“机械化的单一 helper”。

<a id="2026-09-21-再測定-段階-24-は未実行のまま同じ層から-blocker-が出続けている"></a>

## 2026-09-21 重新测量：阶段2～4仍未执行，同层不断产生 blocker

距离上述诊断（2026-07-30）七周。只执行阶段1（#643 单点修复）；阶段2的 `emit_extract_owned` grep 为0，阶段3 churn fixture、阶段4契约文档也没有。统计期间发生了什么。

**blocker 来源（2026-08-22～09-21 的 I-unsound / I-divergence / I-miscompile / regression 共51件，按机制）**：

| 机制 | 件数 | 示例 |
|---|---:|---|
| **wasm emit 层 RC 纪律**（消费位置遗漏 inc / guard） | **~12** | #1786 #1820 #2260 #2308 #2317 #2395 #2397 #2398 |
| wasm i32/u32 索引、宽度截断 | ~7 | #1907 #1909 #2385 #2396 |
| wasm 固定长度 buffer / park span | 5 | #1826 #2116 #2117 #2120 |
| 名称键碰撞（同一标识符空间中两种类别） | 4 | #1726 #1828 #2369 |
| native 借用推断遗漏写法 | 3 | #1713 #2315 #2377 |
| 递归深度 | 3 | #2291 #2306 #2307 |

最大桶正是本 roadmap 指名的层；**0.63.0-rc3 soak 发现4件中的3件**（#2395 / #2397 / #2398）在此。三件都是“同一操作的另一个副本未复制规则”：

- #2398：`list.update`——兄弟 `list.set` 通过 `ArgMode::Retain` 自动正确，`update` 却用 `self.lower(...)` 对 closure body 做 lowering，没有 mode。
- #2397：`list.fold`——staged 副本（`list.rs`）在 10fed0494 得到规则，但只在 `map`/`filter` 链通过的 fused 副本（`list_fuse.rs`）**写得更早**，没收到规则。
- #2395：`value.keys`——wasm 端和 `value_core.almd` 端**两者**都缺 tag 检查，只有 interp（因偶然的0）弃权。

<a id="診断の訂正-穴はヘルパーの欠如ではなく一操作に実装が何個あるか"></a>

### 诊断订正：缺口不是“没有 helper”，而是“一个操作有多少实现”

07-30 提案为“payload 取出汇聚到单一 helper”。这没错，但此次三件并非 helper 能防住的形态——原因是**同一操作重复实现，规则进入一处，却不复制到其余处**。计数：

| 操作（list.almd 的 `= _` 空洞） | native | structural emitter arm | self-host prim-floor 副本 |
|---|---:|---:|---:|
| `fold` | 1 intrinsic | **3**（staged / fused / enumerate） | **6**（`list_fold*.almd`，按 shape） |
| `sort` | 1 | 1 | 6 |
| `get` | 1 | 2 | 4 |
| `filter` | 1 | 2 | 4 |
| `map` | 1 | 2 | 3 |

`list.fold` 有 **10个实现**。self-host 副本按 shape 单态化（`hrec` = heap acc × heap elem、`hsca`、`ols`、`str_hacc` …），是直接写在 prim floor 的手写内存代码——并非 Koka/Lean/Roc 那种“stdlib 在语言内只有一个函数体，Perceus 统一处理”。stdlib 3,877 个函数中，`= _` 有 **694** 个。

**扫查为何遗漏（#2397 实证）**：ee 的闭包性扫查以“lower closure body 并存入 heap slot”为谓词，用**四种 guard 写法的数量**建立闭包性。accumulator rebind 是存入 local，不在总体内；`list_fuse.rs` 没有任何 guard 词汇，因此以“无物可数”登记为空。
**按 marker 有无计数的扫查看不到 marker 出现前的副本。“此文件0匹配”不是干净的标志，而是未转换副本的标志。**

按语法重新取得总体：`self.lower(<body>, …)` 共44处。仍无 guard 的是 `set.fold` / `map.fold`（`collections_set.rs:577` / `collections.rs:464`），但两者以 `Lowered::view` 返回 accumulator，调用方不释放，所以是 leak 级而非 freed read（#2408）。emit 层没有任何地方记录返回 view 承担关键作用。

<a id="段階の書き直し0630-final-の後"></a>

### 重写阶段（0.63.0 final 之后）

阶段2～4保留，前置**计数门禁**。顺序为“测量 → ratchet → 合并”。

1. **实现计数门禁**——每个 `= _` 空洞统计 (native intrinsic, emitter arm, self-host 副本) 三元组的台账 `proofs/impl-count-ledger.toml` 与只缩减 ratchet。新副本不添加台账行就让 CI 失败。#2397 理应在“fold emitter arm 从2→3”的瞬间可见。
2. **合并 `list.fold`**——10实现归为每 target **一个函数体**。fused/staged 两 arm 合成一个 lowering，融合放在前置 pass；self-host 六 shape 副本变为 type-erased 单一函数体（`list_fold_hrec.almd` 已是该形态）。每次合并台账数字下降，alloc 台账 / size ratchet / run parity 证明行为不变。
3. 继续 `update` / `filter_map` / `sort`，以同流程处理 **list.almd 的62个空洞**。
4. 07-30 阶段2～4在此之后。合并后，“单一 helper”只需汇聚到一个函数体。

否决：“增加 fixture 捕捉下次漏副本”——#2397 fixture 只能写成链会 fuse 的形状，不 fuse 的 fixture 全绿。捕捉它的是计数，而非行为。
