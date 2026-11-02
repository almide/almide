说明：Flow[T] 面向用户的规范草案——在阶段 1 后提升到 docs/specs/flow.md
<a id="flowt--user-specification-draft"></a>

# Flow[T] — 用户规范（草案）

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](flow-spec-draft.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](flow-spec-draft.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](flow-spec-draft.zh-CN.md)

> 固定原文的非规范读者快照；原文仍为权威，不自动同步。未独立验证实现或执行引用指令。历史状态、计划及比较主张保留原范围。
> [Original source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/flow-spec-draft.md)

> 原文注记：这是未发布提案，不是当前 API 或诊断分配。标题称12函数，但列表为13（3源、6变换、4终结）。file.lines 标题缺少 Result wrapper，正文和示例却描述构造错误及 !。W005 及抑制方式是未验证的草案主张。内存界限要求元素、行、累加器和 worker 结果大小有界；不产出元素的 filter 后接 take 不保证终止。panic 清理是拟议的 Rust Drop 叙述，不是对 abort 或待实现 WASM 后端的无条件保证。混合示例引用片段之外的 lines。他语言比较是历史原文主张，未重新核验。


> 状态：**草案**——[flow-design.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/flow-design.md) 的阶段 1 实现后，将提升到 `docs/specs/flow.md`。
>
> 这是拟议的用户参考，不是已发布的 API。设计理由见 [flow-design.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/flow-design.md)。
> `FLOW_FORBIDDEN_OP`、`FLOW_CONSUMED` 和 `FLOW_COLLECT_LIST` 是符号化诊断占位符，不是已分配的编译器代码。此前的 E011/E012/E013 标签已属于无关诊断。

---

<a id="1-overview"></a>

## 1. 概览

`Flow[T]` 是**惰性、单遍、仅向前**的值序列。需要在不将全部数据载入内存的情况下处理数据时使用：日志文件、流式 I/O、无限生成器，或元素数量未知的流水线。

```almide
import fs

effect fn count_errors(path: String) -> Result[Int, String] = {
  let n = file.lines(path)!
    |> flow.filter((line) => string.contains(line, "ERROR"))
    |> flow.fold(0, (acc, _) => acc + 1)
  ok(n)
}
```

对于需要随机访问或多遍遍历的小型有界数据，改用 `List[T]`。

---

<a id="2-core-properties"></a>

## 2. 核心属性

<a id="21-lazy"></a>

### 2.1 惰性

元素处理推迟到调用**终结操作**（`fold`、`each`、`collect`、`find`）时。按照 flow-design D1（其构造时 Err 示例），`file.lines(path)!` 会立即打开源，并可能在构造时失败；惰性不会推迟资源获取。组合流水线不会读取文件。

```almide
let pipeline = file.lines("huge.log")!
  |> flow.filter(is_error)
  |> flow.map(parse_line)
// Nothing has been read yet

let count = pipeline |> flow.fold(0, (acc, _) => acc + 1)
// The already-open file is read one line at a time, counted, and closed
```

原代码注释／表头译文：尚未读取任何内容；已打开的文件按行读取、计数、关闭。

<a id="22-single-pass"></a>

### 2.2 单遍

一个 Flow 只能消费一次。消费后再使用 Flow 是编译错误。

```almide
let flow = file.lines(path)!
let n1 = flow |> flow.fold(0, (acc, _) => acc + 1)  // consumes flow
let n2 = flow |> flow.fold(0, (acc, _) => acc + 1)  // ❌ FLOW_CONSUMED: use after consumption
```

原代码注释／表头译文：第一次 fold 消费 flow；第二次是消费后使用，产生 FLOW_CONSUMED。

需要多遍遍历时，先用 `flow.collect()` 物化：

```almide
let xs: List[String] = file.lines(path)! |> flow.collect()  // now a List
let n1 = list.len(xs)      // OK
let n2 = list.len(xs)      // OK
```

原代码注释／表头译文：现在为 List；随后两次 list.len 都可用。

<a id="23-forward-only"></a>

### 2.3 仅向前

不能按索引访问、反转、排序 Flow，也不能在不消费它的情况下计数。随机访问操作是编译错误：

```almide
let flow = file.lines(path)!
list.len(flow)       // ❌ FLOW_FORBIDDEN_OP: Flow does not support len
list.get(flow, 5)    // ❌ FLOW_FORBIDDEN_OP: Flow does not support random access
list.reverse(flow)   // ❌ FLOW_FORBIDDEN_OP: Flow is forward-only
list.sort(flow)      // ❌ FLOW_FORBIDDEN_OP: Flow cannot be sorted
```

原代码注释／表头译文：FLOW_FORBIDDEN_OP：Flow 不支持 len，不支持随机访问，只能向前，不能排序。

完整列表和替代方案见第 6 节（禁止的操作）。

---

<a id="3-when-to-use-flow-vs-list"></a>

## 3. 何时使用 Flow，何时使用 List

| 场景 | 使用 | 理由 |
|---|---|---|
| 少于约 1,000 个元素 | `List[T]` | Flow 的开销不值得 |
| 多于约 100,000 个元素 | `Flow[T]` | 内存 O(1)，而非 O(n) |
| 大小未知（文件、网络、stdin） | `Flow[T]` | I/O 最稳妥的默认选择 |
| 可能无限的源 | `Flow[T]` | 必需 |
| 需要按索引随机访问 | `List[T]` | Flow 仅向前 |
| 需要多次迭代 | `List[T]` | Flow 是单遍的 |
| 需要不遍历就获取长度 | `List[T]` | Flow 没有长度 |
| 有界并行处理 | `Flow[T]` + `fan.map(limit: n)` | 自动背压 |

**简单规则**：数据能放入 RAM 且已知大小，用 `List`；否则或可能不能，用 `Flow`。

---

<a id="4-api-reference"></a>

## 4. API 参考

<a id="41-sources"></a>

### 4.1 源

<a id="flowfrom_listt---flowt"></a>
<a id="flowfrom_listtxs-listt---flowt"></a>

#### `flow.from_list[T](xs: List[T]) -> Flow[T]`

将 List 转为 Flow，用于将 List 数据接入 Flow 流水线。

```almide
let f: Flow[Int] = flow.from_list([1, 2, 3])
f |> flow.map((n) => n * 2) |> flow.fold(0, (acc, n) => acc + n)  // => 12
```

<a id="flowgenerates-t---optiont-s---flowt"></a>
<a id="flowgenerates-tseed-s-step-s---optiont-s---flowt"></a>

#### `flow.generate[S, T](seed: S, step: (S) -> Option[(T, S)]) -> Flow[T]`

从种子生成 Flow。`step` 函数接收当前状态，返回下一个值与新状态，或返回 `none` 终止。

```almide
// Finite: 1..10
flow.generate(1, (n) => if n <= 10 then some((n, n + 1)) else none)

// Infinite: all natural numbers
flow.generate(1, (n) => some((n, n + 1)))

// Fibonacci
flow.generate((0, 1), (pair) => {
  let (a, b) = pair
  some((a, (b, a + b)))
})
```

原代码注释／表头译文：有限：1..10；无限：所有自然数；Fibonacci。

无限 Flow 没有问题——但在 `flow.collect()` 或使用无界累加器的 `flow.fold()` 前，务必先用 `flow.take(n)`。

<a id="filelinespath-string---flowstring--effect"></a>

#### `file.lines(path: String) -> Flow[String]` 🔴 效应

流式读取文件，每行一个元素。Flow 被消费或丢弃时，文件句柄自动关闭。

```almide
effect fn main() = {
  file.lines("log.txt")!
    |> flow.each((line) => println(line))
}
```

**错误**：打开文件失败是 `Result::Err`。读取中途的 I/O 错误会 panic（未来阶段可用 `file.lines_checked` 逐行处理错误）。

小文件优先用 `fs.read_text(path)!` + `string.lines(...)`，直接得到 `List[String]`。

---

<a id="42-transformations-lazy"></a>

### 4.2 变换（惰性）

所有变换都是惰性的：调用终结操作前不会拉取值。动词名称与 `list.*` 完全一致。

<a id="flowmapt-u---u---flowu"></a>
<a id="flowmapt-uxs-flowt-f-t---u---flowu"></a>

#### `flow.map[T, U](xs: Flow[T], f: (T) -> U) -> Flow[U]`

对每个元素应用 `f`。

```almide
file.lines(path)!
  |> flow.map(string.trim)
  |> flow.map(string.to_upper)
```

<a id="flowfiltert---bool---flowt"></a>
<a id="flowfiltertxs-flowt-pred-t---bool---flowt"></a>

#### `flow.filter[T](xs: Flow[T], pred: (T) -> Bool) -> Flow[T]`

保留 `pred` 返回 `true` 的元素。

```almide
flow.filter(lines, (l) => string.len(l) > 0)
```

<a id="flowfilter_mapt-u---optionu---flowu"></a>
<a id="flowfilter_mapt-uxs-flowt-f-t---optionu---flowu"></a>

#### `flow.filter_map[T, U](xs: Flow[T], f: (T) -> Option[U]) -> Flow[U]`

应用 `f`；仅保留 `some(...)` 结果并解包。适用于部分输入无效的解析场景。

```almide
file.lines(path)!
  |> flow.filter_map((line) => parse_int(line))
```

<a id="flowflat_mapt-u---flowu---flowu"></a>
<a id="flowflat_mapt-uxs-flowt-f-t---flowu---flowu"></a>

#### `flow.flat_map[T, U](xs: Flow[T], f: (T) -> Flow[U]) -> Flow[U]`

应用返回 Flow 的 `f`，并连接结果。

```almide
file.lines(path)!
  |> flow.flat_map((line) => flow.from_list(string.split(line, " ")))
```

<a id="flowtaket---flowt"></a>
<a id="flowtaketxs-flowt-n-int---flowt"></a>

#### `flow.take[T](xs: Flow[T], n: Int) -> Flow[T]`

仅保留前 `n` 个元素。达到 `n` 个后停止拉取上游。

```almide
file.lines("big.log")!
  |> flow.take(100)            // only pull first 100 lines
  |> flow.collect()            // List[String] with up to 100 elements
```

原代码注释／表头译文：仅拉取前100行；得到至多100元素的 List[String]。

<a id="flowdropt---flowt"></a>
<a id="flowdroptxs-flowt-n-int---flowt"></a>

#### `flow.drop[T](xs: Flow[T], n: Int) -> Flow[T]`

跳过前 `n` 个元素，输出其余元素。

```almide
file.lines("csv.txt")!
  |> flow.drop(1)              // skip header
  |> flow.each(process_row)
```

原代码注释／表头译文：跳过表头。

---

<a id="43-terminal-operations-"></a>

### 4.3 终结操作 🔴

终结操作消费 Flow 并产生结果。**求值真正发生在这里。**

<a id="flowfoldt-u---u---u-"></a>
<a id="flowfoldt-uxs-flowt-init-u-combine-u-t---u---u-"></a>

#### `flow.fold[T, U](xs: Flow[T], init: U, combine: (U, T) -> U) -> U` 🔴

通过 `combine` 传递累加器，将 Flow 归约为单个值。

```almide
// Count elements
flow.fold(0, (acc, _) => acc + 1)

// Sum
flow.fold(0, (acc, n) => acc + n)

// Build a Map
flow.fold(map.empty(), (m, (k, v)) => map.insert(m, k, v))
```

原代码注释／表头译文：计数元素；求和；构建 Map。

`fold` 遍历**整个** Flow。需要提前终止时，用 `flow.find`，或在上游用 `flow.take` 变换。

<a id="floweacht---unit---unit-"></a>
<a id="floweachtxs-flowt-f-t---unit---unit-"></a>

#### `flow.each[T](xs: Flow[T], f: (T) -> Unit) -> Unit` 🔴

对每个元素应用 `f`，以执行副作用。不返回内容。

```almide
file.lines(path)!
  |> flow.each((line) => println(line))
```

<a id="flowcollectt---listt-"></a>
<a id="flowcollecttxs-flowt---listt-"></a>

#### `flow.collect[T](xs: Flow[T]) -> List[T]` 🔴

将 Flow 物化为 List。**这是唯一的 Flow → List 边界。**

```almide
let first_10: List[String] = file.lines(path)!
  |> flow.take(10)
  |> flow.collect()
```

**警告（W005）**：若源大小无界，在没有上游 `flow.take` 的情况下调用 `flow.collect()` 会触发编译器警告。如果已知源有限且较小，可用 `let _ = flow.collect(xs)` 抑制，或加上 `|> flow.take(N)`。

<a id="flowfindt---bool---optiont--short-circuit"></a>
<a id="flowfindtxs-flowt-pred-t---bool---optiont--short-circuit"></a>

#### `flow.find[T](xs: Flow[T], pred: (T) -> Bool) -> Option[T]` 🔴（短路）

返回首个使 `pred` 返回 `true` 的元素，否则返回 `none`。**短路**：一旦找到匹配项就停止拉取上游。

```almide
// First ERROR line in the file (stops reading after finding it)
file.lines("big.log")!
  |> flow.find((l) => string.contains(l, "ERROR"))
```

原代码注释／表头译文：文件中第一条 ERROR 行，找到后停止读取。

搜索应使用 `find` 而不是 `fold`：`fold` 会遍历整个源。

---

<a id="5-resource-cleanup"></a>

## 5. 资源清理

持有外部资源（文件句柄、网络套接字）的 Flow，在被消费或离开作用域时自动清理。不需要手动关闭。

```almide
effect fn read_first_5(path: String) -> Result[List[String], String] = {
  let xs = file.lines(path)!
    |> flow.take(5)
    |> flow.collect()
  // File is already closed here — flow was consumed by collect()
  ok(xs)
}
```

原代码注释／表头译文：此处文件已经关闭：collect() 消费了 flow。

即使流水线在中途被放弃（panic、提前返回、退出作用域），Flow 也会被丢弃，资源得到释放：

```almide
effect fn process(path: String) -> Result[Unit, String] = {
  let _n = file.lines(path)!
    |> flow.take(10)           // only read 10 lines
    |> flow.fold(0, (acc, _) => acc + 1)
  // Remaining lines are never read; file handle closed after take(10)
  ok(())
}
```

原代码注释／表头译文：只读取10行；剩余行永不读取，take(10) 后文件句柄关闭。

在 Almide 中**永远不写 `.close()`**。作用域管理生命周期。

---

<a id="6-forbidden-operations"></a>

## 6. 禁止的操作

对 `Flow[T]` 执行下列操作会产生**编译错误**（`FLOW_FORBIDDEN_OP`），各有建议的替代方式。

| 操作 | 禁止理由 | 替代方式 |
|---|---|---|
| `list.len(flow)` | Flow 可能无限 | `flow.fold(0, (acc, _) => acc + 1)` |
| `list.get(flow, i)` | Flow 仅向前 | `flow.drop(i) \|> flow.take(1) \|> flow.find((_) => true)` |
| `list.reverse(flow)` | Flow 仅向前 | `flow.collect() \|> list.reverse` |
| `list.sort(flow)` | 排序需要全部元素 | `flow.collect() \|> list.sort` |
| `list.contains(flow, x)` | 可能遍历无限源 | `flow.find((y) => y == x) != none` |
| `list.last(flow)` | 会遍历无限源 | `flow.fold(none, (_, x) => some(x))` |

编译器同时给出“惰性替代”提示（首选、内存安全）和“先物化”提示（附内存警告）。错误消息示例见第 8 节。

---

<a id="7-common-patterns"></a>

## 7. 常见模式

<a id="71-log-analysis"></a>

### 7.1 日志分析

```almide
import fs

effect fn count_errors(path: String) -> Result[Int, String] = {
  let n = file.lines(path)!
    |> flow.filter((line) => string.contains(line, "ERROR"))
    |> flow.fold(0, (acc, _) => acc + 1)
  ok(n)
}
```

**内存**：O(1)，与文件大小无关。

<a id="72-csv--json-lines-conversion"></a>

### 7.2 CSV → JSON Lines 转换

```almide
effect fn csv_to_jsonl(input: String, output: String) -> Result[Unit, String] = {
  file.lines(input)!
    |> flow.drop(1)                                       // skip header
    |> flow.map((line) => row_to_json(string.split(line, ",")))
    |> flow.each((j) => fs.append_text(output, json.stringify(j) + "\n")!)
  ok(())
}
```

原代码注释／表头译文：跳过表头。

**内存**：O(1)。每次处理一行。

<a id="73-early-search-find"></a>

### 7.3 提前搜索（find）

```almide
effect fn first_error_line(path: String) -> Result[Option[String], String] = {
  let found = file.lines(path)!
    |> flow.find((line) => string.contains(line, "ERROR"))
  ok(found)
}
```

**内存**：O(1)。首次匹配后停止读取。

<a id="74-first-n-lines"></a>

### 7.4 前 N 行

```almide
effect fn head(path: String, n: Int) -> Result[List[String], String] = {
  let lines = file.lines(path)!
    |> flow.take(n)
    |> flow.collect()
  ok(lines)
}
```

**内存**：O(n)，由参数限制。

<a id="75-infinite-source--finite-use"></a>

### 7.5 无限源 + 有限使用

```almide
fn first_100_primes() -> List[Int] = 
  flow.generate(2, (n) => some((n, n + 1)))
    |> flow.filter(is_prime)
    |> flow.take(100)
    |> flow.collect()
```

**内存**：O(100)。由于 `take` 限制消费，无限生成器是安全的。

<a id="76-mixing-list-and-flow"></a>

### 7.6 混合 List 与 Flow

```almide
let allowed_users: List[String] = ["alice", "bob"]          // small, List

effect fn filter_user_logs(log_path: String) -> Result[Int, String] = {
  let n = file.lines(log_path)!                              // huge, Flow
    |> flow.filter_map((line) => {
        let j = json.parse(line)?
        let user = json.get_string(j, "user")?
        if list.contains(allowed_users, user) then some(line) else none
      })
    |> flow.fold(0, (acc, _) => acc + 1)
  ok(n)
}
```

原代码注释／表头译文：allowed_users 很小，使用 List；日志很大，使用 Flow。

**注意**：`list.contains` 用于 `allowed_users`（List）；`flow.filter_map` 用于 `lines`（Flow）。各模块操作自己的类型。

<a id="77-parallel-processing-phase-2"></a>

### 7.7 并行处理（阶段 2+）

```almide
effect fn fetch_all(url_file: String) -> Result[Unit, String] = {
  file.lines(url_file)!
    |> flow.filter((url) => string.starts_with(url, "https://"))
    |> fan.map(limit: 10, (url) => http.get(url)!)           // bounded parallel
    |> flow.each((body) => write_to_disk(body)!)
  ok(())
}
```

原代码注释／表头译文：有界并行。

**内存**：由 `limit` 限制（10 个并发 worker）。背压自动生效：有 worker 空闲前，不拉取上游 Flow。

---

<a id="8-error-messages"></a>

## 8. 错误消息

<a id="flow_forbidden_op--forbidden-operation-on-flow"></a>

### FLOW_FORBIDDEN_OP — 对 Flow 执行禁止的操作

```
error[FLOW_FORBIDDEN_OP]: list.len cannot be called on Flow[String]
  --> bad.almd:5:11
   |
 5 |   let n = list.len(lines)
   |           ^^^^^^^^^^^^^^^
   = note: Flow[T] is lazy and possibly infinite
   = hint: to count lazily without materialization:
           lines |> flow.fold(0, (acc, _) => acc + 1)
   = hint: to materialize first (may use unbounded memory):
           lines |> flow.collect() |> list.len
```

诊断全文译文：不能对 Flow[String] 调用 list.len（bad.almd:5:11，指向该调用）。Flow[T] 惰性且可能无限。提示：如需不物化而惰性计数，使用所列 lines → fold；如需先物化，使用所列 lines → collect → list.len，但可能占用无界内存。

每条 FLOW_FORBIDDEN_OP 错误含有**两个提示**：推荐的惰性替代方案，以及附内存警告的先物化备选方案。

<a id="flow_consumed--flow-consumed-twice"></a>

### FLOW_CONSUMED — Flow 被消费两次

```
error[FLOW_CONSUMED]: Flow[String] is move-only and has been consumed
  --> double.almd:4:11
   |
 3 |   let n1 = flow |> flow.fold(0, (acc, _) => acc + 1)
   |            ---- consumed here
 4 |   let n2 = flow |> flow.fold(0, (acc, _) => acc + 1)
   |            ^^^^ used after consumption
   = note: Flow is single-pass and cannot be consumed twice
   = hint: materialize first if you need multiple passes:
           let xs = flow |> flow.collect()
           let n1 = list.len(xs)
           let n2 = list.len(xs)
```

诊断全文译文：Flow[String] 为 move-only，已经消费（double.almd:4:11）。第3行是消费位置，第4行是消费后使用。Flow 为单遍，不能消费两次。提示：若需多遍，先物化；所列 xs = collect 之后，用 n1、n2 两次获取 List 长度。

<a id="flow_collect_list--flowcollect-on-a-list"></a>

### FLOW_COLLECT_LIST — 对 List 调用 `flow.collect`

```
error[FLOW_COLLECT_LIST]: flow.collect expects Flow[T], got List[T]
  --> wrong.almd:2:6
   |
 2 |   [1, 2, 3] |> flow.collect()
   |                ^^^^^^^^^^^^^^
   = note: List is already materialized, no conversion needed
   = hint: just use the list directly: [1, 2, 3]
```

诊断全文译文：flow.collect 期待 Flow[T]，收到 List[T]（wrong.almd:2:6，指向 collect 调用）。List 已物化，无需转换。提示：直接使用所列列表 [1, 2, 3]。

<a id="w005--unbounded-flowcollect"></a>

### W005 — 无界 `flow.collect`

```
warning[W005]: flow.collect() on a flow without upstream flow.take
  --> risky.almd:3:6
   |
 3 |   |> flow.collect()
   |      ^^^^^^^^^^^^^^
   = note: if the source is large or infinite, this will OOM
   = hint: bound the collection:
           flow.take(N) |> flow.collect()
   = hint: or use flow.fold for accumulation:
           flow.fold(init, combine)
   = hint: if you know the source is finite and small, suppress with:
           let _ = flow.collect(flow)
```

警告全文译文：对没有上游 flow.take 的 flow 调用 flow.collect()（risky.almd:3:6，指向 collect）。若源很大或无限，将 OOM。提示：用所列 take(N) → collect 限制收集；或用 fold(init, combine) 累积；若已知源有限且小，用所列 let _ = flow.collect(flow) 抑制警告。

---

<a id="9-quick-reference"></a>

## 9. 速查

<a id="all-12-functions-at-a-glance"></a>

### 全部 12 个函数一览

```
Sources
  flow.from_list(xs)                          List[T]     -> Flow[T]
  flow.generate(seed, step)                   S           -> Flow[T]
  file.lines(path)            🔴 effect       String      -> Flow[String]

Transformations (lazy)
  flow.map(xs, f)                             Flow[T]     -> Flow[U]
  flow.filter(xs, pred)                       Flow[T]     -> Flow[T]
  flow.filter_map(xs, f)                      Flow[T]     -> Flow[U]
  flow.flat_map(xs, f)                        Flow[T]     -> Flow[U]
  flow.take(xs, n)                            Flow[T]     -> Flow[T]
  flow.drop(xs, n)                            Flow[T]     -> Flow[T]

Terminal operations
  flow.fold(xs, init, combine)  🔴            Flow[T]     -> U
  flow.each(xs, f)              🔴            Flow[T]     -> Unit
  flow.collect(xs)              🔴            Flow[T]     -> List[T]
  flow.find(xs, pred)           🔴 (short)    Flow[T]     -> Option[T]
```

原代码注释／表头译文：Sources 为源；Transformations (lazy) 为惰性变换；Terminal operations 为终结操作；effect 为效果操作，short 为短路。所有签名和输入/输出类型保持在原表中。

<a id="verb-alignment-with-list"></a>

### 与 `list.*` 的动词对齐

| list.* | flow.* |
|---|---|
| `list.map` | `flow.map` |
| `list.filter` | `flow.filter` |
| `list.filter_map` | `flow.filter_map` |
| `list.flat_map` | `flow.flat_map` |
| `list.take` | `flow.take` |
| `list.drop` | `flow.drop` |
| `list.fold` | `flow.fold` |
| `list.each` | `flow.each` |
| `list.find` | `flow.find` |
| `list.from_list` | （不需要） |
| （list 中没有） | `flow.collect`（边界） |
| （list 中没有） | `flow.generate`（源） |

---

<a id="10-faq"></a>

## 10. 常见问题

**问：能迭代一个 Flow 两次吗？**
答：不能。Flow 是单遍的。需要两次迭代时先物化：`let xs = flow |> flow.collect()`，再多次迭代 `xs`（List）。

**问：文件只有几 KB，应使用 `file.lines` 还是 `fs.read_text`？**
答：使用 `fs.read_text(path)!` + `string.lines(...)`，得到 `List[String]`，小文件这样更简单。

**问：如何统计巨大文件的行数？**
答：`file.lines(path)! |> flow.fold(0, (acc, _) => acc + 1)`。内存为 O(1)。

**问：需要随机访问元素，可以使用 Flow 吗？**
答：不可以。Flow 仅向前。使用 List，或先 `flow.collect()`。

**问：可以对无限 Flow 执行 `flow.collect` 吗？**
答：编译器会警告（W005），但可以尝试，最终会 OOM。收集无界源前始终先用 `flow.take(n)`。

**问：怎样组合两个 Flow？**
答：阶段 1 没有 `flow.chain`。可用 `flow.flat_map(flow.from_list([a, b]), (f) => f)` 暂时代替，或等待阶段 3。

**问：代码在 `flow.fold` 期间 panic，文件句柄会怎样？**
答：句柄自动关闭。Almide 使用 Rust 的 `Drop` trait，Flow 持有的资源在 Flow 被丢弃时释放，包括 panic 时。

**问：可以不消费 Flow 就获取长度吗？**
答：不能。惰性、可能无限的序列长度未知。用 `flow.fold(0, (acc, _) => acc + 1)` 计数（会消费 Flow），或用 `flow.collect() |> list.len` 先物化。

**问：Flow 与 Rust Iterator 或 Kotlin Sequence 相比如何？**
答：语义类似（惰性、仅向前）。区别：
- Almide 使用模块名 `flow.*`；Rust 调用 Iterator 的方法，Kotlin 使用 `Sequence` 扩展函数。
- Almide 在编译时拒绝 `list.len(flow)`；Rust 和 Kotlin 允许，但会在无限源上挂起。
- Almide 与 Rust 一样在 Drop 时自动清理文件句柄，不同于 Kotlin 需要作用域块的 `useLines`。

**问：Flow 怎样与 `fan.map` 配合？**
答：`fan.map(flow, limit: n, f)` 并行处理 Flow，并自动背压。结果为 `Flow[U]`，仍然惰性、流式，适合并行 HTTP 获取或文件处理。

---

<a id="11-comparison-with-other-languages"></a>

## 11. 与其他语言比较

用于 LLM／开发者的迁移学习：

| Almide | Rust | Kotlin | MoonBit | Haskell |
|---|---|---|---|---|
| `Flow[T]` | `impl Iterator<Item=T>` | `Sequence<T>` | `Iter[T]` | `[T]`（全部惰性） |
| `flow.map` | `.map` | `.map` | `.map` | `fmap` |
| `flow.filter` | `.filter` | `.filter` | `.filter` | `filter` |
| `flow.fold` | `.fold` | `.fold` | `.fold` | `foldl'` |
| `flow.find` | `.find` | `.find` | `.find` | `find` |
| `flow.collect` | `.collect::<Vec<_>>()` | `.toList()` | `.collect()` | （自动） |
| `flow.take(n)` | `.take(n)` | `.take(n)` | `.take(n)` | `take n` |
| `file.lines(path)` | `BufRead::lines()` | `File.useLines { }` | `@fs.lines` | `lines <$> readFile` |
| 仅移动 | 移动／借用 | （可变） | 单遍 | （纯惰性） |
| 自动清理 | Drop | `useLines { }` 作用域 | 手动 | 惰性 I/O（有问题） |

**最接近的先例**：MoonBit 的 `Iter[T]`。Almide Flow 与其共享单遍、惰性、推荐链式组合的设计，但模块命名惯例不同。

---

<a id="12-implementation-status"></a>

## 12. 实现状态

本文描述**目标设计**。实现跟踪见 [flow-design.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/flow-design.md)。

- [ ] 阶段 1：核心 `Flow[T]` 类型 + 12 个 API + 编译期检查
- [ ] 阶段 2：`fan.map` × Flow 集成（并行流式处理）
- [ ] 阶段 3：附加操作（`zip`、`chain`、`take_while` 等）
- [ ] 阶段 4：异步／WASM 后端

阶段 1 完成后，本文移至 `docs/specs/flow.md`，加上 `> Last updated: YYYY-MM-DD` 页头，并移除草案状态。
