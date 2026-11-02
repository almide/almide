说明：Flow[T] 惰性流式序列，flow.* 命名空间的动词与 list.* 对齐。
<a id="flowt--lazy-streaming-sequences"></a>

# Flow[T] — 惰性流式序列

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](flow-design.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](flow-design.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](flow-design.zh-CN.md)

> 固定原文的非规范读者快照；原文仍为权威，不自动同步。未独立验证实现或执行引用指令。历史状态、计划及比较主张保留原范围。
> [Original source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/flow-design.md)

> 原文注记：未实现的历史提案，不是现行 Flow API 或安全保证。符号诊断尚未分配，W005/W006 也需核对现行注册表。Drop、栈展开、取消、常量内存及命名空间/LLM 比较是原设计主张，非独立验证的平台行为或实测优势。O(1) 未计可变元素大小及下游缓冲；若上游 filter 不终止，之后的 take 未必终止。原 ? 注释与 ! 代码不同；无序 fan、timeout、旧 TOML 路径及未来 API 不覆盖现行规格。8/10 单次通过是目标，不是结果。未执行命令或基准。


> 尚未实现的设计提案。下文的 `FLOW_FORBIDDEN_OP`、`FLOW_CONSUMED` 和
> `FLOW_COLLECT_LIST` 是符号占位符，不是已分配的诊断代码。
> E011/E012/E013 已在 `docs/diagnostics/` 表示其他错误；
> 任何 Flow 实现都必须通过现行注册表分配诊断。

<a id="design-philosophy"></a>

## 设计理念

Almide 通常重视减少词汇，但 Flow **在局部明确惰性/立即求值边界及流式性质的价值很高**，因此采用 `flow.*` 命名空间。

为降低学习成本，**操作动词与 `list.*` 对齐**。如 `list.map` 与 `flow.map`、`list.filter` 与 `flow.filter`，只改变前缀。LLM 只需学习一套动词词汇，再增加一条“类型→前缀”的映射规则。

边界 API 只有 `flow.collect`。**不进行隐式转换**。`file.lines` 等流式数据源必须返回 `Flow[T]`。

> Almide 通常重视减少词汇，但 Flow 在局部明确惰性/立即求值边界及流式性质的价值很高，因此采用 `flow.*` 命名空间。不过为降低学习成本，操作名尽可能与 `list.*` 对齐。

此段也放在 `docs/specs/flow.md` 开头，让所有人共享这一原则。

<a id="why-flow-not-shared-list"></a>

## 为什么采用 `flow.*`（而不共用 `list.*`）

让 `list.*` 同时支持 List[T] / Flow[T] 的方案因以下理由被否决：

| 否决理由 | 详情 |
|---|---|
| **没有先例** | MoonBit/Gleam/Kotlin/Rust 都分开命名空间 |
| **用户函数不自然** | 需要 `Seq[T]` 一类高阶种类抽象，实现成本高 |
| **无法发挥迁移学习** | LLM 在训练数据中大量见过 `iterator.filter`/`stream.map` |
| **难以追踪远距离依赖** | 模块名不携带类型信息，LLM 就无法追踪远处定义 |
| **标准库多态 / 用户单态不一致** | “只有标准库有魔法，用户代码普通”最容易混淆 LLM |

保守地分离 `flow.*` 并统一动词，**更符合 Almide 的使命：让 LLM 写得最正确的语言**。

<a id="core-type"></a>

## 核心类型

`Flow[T]` 是**惰性、单次遍历、仅前向**序列（对应 MoonBit 的 `Iter[T]`）。

| 性质 | 含义 |
|---|---|
| **惰性** | 终结操作（fold/each/collect/find）调用前不拉取值 |
| **单次遍历** | 消费后不可复用；重复使用是编译错误（见后文） |
| **仅前向** | 不可随机访问；`len`、`get`、`reverse`、`sort` 是类型错误 |

<a id="decided-semantics"></a>

## 已决定的语义

以下项目在 Phase 1 前决定；模糊不清会使实现陷入困境。

<a id="d1-filelines-の型-effect--lazy-境界"></a>

### D1. `file.lines` 的类型（Effect × 惰性边界）

```almide
effect fn file.lines(path: String) -> Flow[String]
```

**决定**：打开文件之前可失败（通过 `effect fn` → `Result`），打开后的读取错误触发 panic。

理由：
- 文件不存在等常见错误可由 `!` 或 `?` 处理
- 读取途中错误（如磁盘断开）极少，panic 足够
- 每行包裹为 `Result[String, IOError]` 会使流水线冗长（参见 Rust 的 `Lines::Item = io::Result<String>`）
- 按 80/20 作出的务实选择

```almide
// 使い方: 開けない場合は ? で早期リターン
effect fn count_errors(path: String) -> Result[Int, String] = {
  let lines = file.lines(path)!      // ← 開けなければ即 Err
  let n = lines
    |> flow.filter((l) => string.contains(l, "ERROR"))
    |> flow.fold(0, (acc, _) => acc + 1)
  ok(n)
}
```

代码日文注释译文：打不开时用 ? 提前返回；箭头处若打不开则立即 Err。原注释写 ?，代码使用 !，这里不擅自统一。

未来若需要健壮的逐行错误处理：
```almide
effect fn file.lines_checked(path: String) -> Flow[Result[String, IOError]]
```
在 Phase 3 以后考虑。Phase 1 只有 `file.lines`。

<a id="d2-resource-cleanup-drop-意味論"></a>

### D2. 资源清理（Drop 语义）

**决定**：Flow 通过 Rust 的 `Drop` trait 清理，用户无需编写任何内容。

```
Flow の内部表現: Box<dyn Iterator<Item = T>>
           ↓
File-backed Flow: BufReader<File> を Iterator として wrap
           ↓
Iterator が drop される時点で BufReader → File と順に drop
           ↓
ファイルハンドル自動 close
```

图示译文：Flow 内部表示为 Box<dyn Iterator<Item = T>> → 文件型 Flow 将 BufReader<File> 包装成 Iterator → Iterator 被 drop 时，依次 drop BufReader、File → 自动关闭文件句柄。

保证的行为：

1. **退出作用域**：离开函数时 drop Flow → 清理上游资源
2. **提前结束**（`take(10)` 之后不再消费）：拉取 10 个元素后 drop Flow → 不读取剩余内容并关闭句柄
3. **panic**：fold 中 panic 导致栈展开 → drop Flow → 清理
4. **fan 作用域中失败**：`fan.map` 的 worker 失败 → 退出作用域 → drop Flow → 清理

**Rust Drop 自然避免 Haskell 惰性 I/O 的失败模式：句柄存活超出作用域**。

<a id="d3-single-pass-違反検出"></a>

### D3. 检测单次遍历违规

**决定**：Flow **只能移动**。Almide 所有权分析将重复使用视为编译错误。

```almide
let flow = file.lines(path)!
let n1 = flow |> flow.fold(0, (acc, _) => acc + 1)   // flow を消費 (move)
let n2 = flow |> flow.fold(0, (acc, _) => acc + 1)   // ❌ error: use after move
// error[FLOW_CONSUMED]: Flow[String] is move-only and has been consumed
//   --> file.almd:3:11
//    |
//  2 |   let n1 = flow |> flow.fold(...)
//    |            ---- consumed here
//  3 |   let n2 = flow |> flow.fold(...)
//    |            ^^^^ used after consumption
//    = hint: Flow is single-pass. If you need to iterate twice, use flow.collect()
//            to materialize first: let xs = flow.collect(flow)
```

注释及诊断译文：第一次 fold 消费（移动）flow，第二次在移动后使用。错误说明 Flow[String] 只能移动且已消费，分别标出消费和再次使用位置。提示：Flow 只能遍历一次，需要两次遍历时先用 flow.collect() 实体化为列表。

只需在 Almide 现有 use-count 分析中增加“Flow 不可 clone”规则即可实现（比普通类型严格，只允许 Move）。

<a id="d4-flowcollect-without-flowtake-は-lint-warning"></a>

### D4. 没有 `flow.take` 的 `flow.collect` 是 lint 警告

**决定**：单独 `flow.collect(flow)`（上游没有 `take`）会发出 lint 警告。

```almide
file.lines(path)!
  |> flow.filter(p)
  |> flow.collect()       // ⚠️ warning: unbounded flow.collect
// warning[W005]: flow.collect() without upstream flow.take may OOM
//   hint: add `|> flow.take(n)` upstream if the source may be large,
//         or suppress with `_ = flow.collect(...)` if you are sure
```

诊断译文：无上游 flow.take 的 flow.collect() 可能耗尽内存。若源可能很大，在上游加 |> flow.take(n)；若确定安全，可用 _ = flow.collect(...) 抑制警告。

这是警告而非错误。用户知道上游有限时没有问题，但可防止 LLM 机械地附加 `collect()`。

<a id="d5-flowgenerate-の有限性は型で区別しない"></a>

### D5. 类型不区分 `flow.generate` 的有限性

**决定**：`flow.generate(seed, step)` 在 step 返回 `None` 时结束。类型始终是 `Flow[T]`（不区分有限/无限）。

```almide
flow.generate(0, (n) => if n < 10 then some((n, n + 1)) else none)   // 有限 (10 要素)
flow.generate(0, (n) => some((n, n + 1)))                             // 無限
// どちらも Flow[Int]、型は同じ
```

注释译文：第一个有限（10 个元素），第二个无限；两者类型同为 Flow[Int]。

有限性**难以量化**，因此不由类型追踪；用 D4 的 lint 防止无限源事故。

---

<a id="minimum-api-12-functions"></a>

## 最小 API（12 个函数）

<a id="transformations-6"></a>

### 变换（6）

**动词与 `list.*` 完全一致**。

```almide
flow.map[T, U](xs: Flow[T], f: (T) -> U) -> Flow[U]
flow.filter[T](xs: Flow[T], pred: (T) -> Bool) -> Flow[T]
flow.filter_map[T, U](xs: Flow[T], f: (T) -> Option[U]) -> Flow[U]
flow.flat_map[T, U](xs: Flow[T], f: (T) -> Flow[U]) -> Flow[U]
flow.take[T](xs: Flow[T], n: Int) -> Flow[T]
flow.drop[T](xs: Flow[T], n: Int) -> Flow[T]
```

<a id="terminal-operations-4"></a>

### 终结操作（4）

**在这里开始求值**。文档和速查表加 🔴 标记。

```almide
flow.fold[T, U](xs: Flow[T], init: U, combine: (U, T) -> U) -> U       // 🔴
flow.each[T](xs: Flow[T], f: (T) -> Unit) -> Unit                       // 🔴
flow.collect[T](xs: Flow[T]) -> List[T]                                 // 🔴
flow.find[T](xs: Flow[T], pred: (T) -> Bool) -> Option[T]               // 🔴 短絡
```

find 签名的日文注释表示短路。

`flow.find` **短路求值**：首次找到时停止拉取上游。这是与 `fold` 的区别。加入 `find` 是为了避免用 fold 编写短路搜索却遍历全部的典型错误。

<a id="source-constructors-2"></a>

### 数据源构造器（2）

```almide
flow.from_list[T](xs: List[T]) -> Flow[T]
flow.generate[S, T](seed: S, step: (S) -> Option[(T, S)]) -> Flow[T]
```

用 `flow.from_list([])` 代替 `flow.empty`。`flow { emit(...) }` 构建器在 Phase 3 以后考虑。

<a id="組み込みソース-stdlib-他モジュール経由"></a>

### 内置数据源（经其他标准库模块）

- `file.lines(path: String) -> Flow[String]`（见 D1，`effect fn`）
- 未来：`io.read_lines() -> Flow[String]`（标准输入流）
- 未来：`http.stream(url: String) -> Flow[Bytes]`

---

<a id="key-rules-決定事項"></a>

## 关键规则（决定事项）

<a id="r1-filelines-は-flowstring-を返す"></a>

### R1. `file.lines` 返回 `Flow[String]`

读取整个小文件时使用 `fs.read_text` + `string.lines`。**不要含糊**。

```almide
// 小さい config → fs.read_text + string.lines
let content = fs.read_text("config.toml")!
let lines: List[String] = string.lines(content)

// 巨大ログ → file.lines
let lines: Flow[String] = file.lines("system.log")!
```

注释译文：小配置→fs.read_text + string.lines；巨大日志→file.lines。

<a id="r2-flowcollect-が唯一の-flow--list-境界"></a>

### R2. `flow.collect` 是唯一 Flow → List 边界

没有隐式转换，也没有由类型注解触发的强制转换。

```almide
let xs: List[String] = flow |> flow.collect()   // 明示
```

注释表示显式转换。

<a id="r3-flowtaken-の戻り型は-flowt"></a>

### R3. `flow.take(n)` 返回 `Flow[T]`

没有特例；需要 List 时接上 `|> flow.collect()`。

```almide
let first_10: List[String] = file.lines(path)! |> flow.take(10) |> flow.collect()
```

<a id="r4-terminal-operations-は-docs-で明示マーク"></a>

### R4. 文档明确标记终结操作

速查表为 `fold`、`each`、`collect`、`find` 标 🔴 terminal，视觉强调“在这里执行”。

<a id="r5-list-と-flow-の動詞は必ず揃える"></a>

### R5. `list.*` 与 `flow.*` 的动词必须一致

两边都有的函数使用**相同动词名**，不能只给一方另起名。

**禁止示例**：
- ❌ `flow.keep_if`（list 若用 `filter`，flow 也用 `filter`）
- ❌ `flow.transform`（list 若用 `map`，flow 也用 `map`）
- ❌ `flow.reduce`（list 若用 `fold`，flow 也用 `fold`）
- ❌ `flow.fetch_first`（list 若用 `find`，flow 也用 `find`）

<a id="r6-single-pass-を-cheatsheet-冒頭で宣言"></a>

### R6. 速查表开头声明单次遍历

```markdown
## Flow の基本性質

Flow は single-pass です。一度 consume (fold/each/collect/find) した Flow は再利用できません。
Almide は所有権解析で二重使用をコンパイル時に検出します。
```

引用速查表译文：“Flow 的基本性质。Flow 只能遍历一次。已经消费（fold/each/collect/find）的 Flow 不可复用。Almide 通过所有权分析在编译时检测重复使用。”

<a id="r7-listt--flowt-は-flowfrom_list-で明示"></a>

### R7. `List[T] → Flow[T]` 通过 `flow.from_list` 显式转换

没有自动提升。**使用时必须写出**。

```almide
let xs: List[Int] = [1, 2, 3]
let f: Flow[Int] = flow.from_list(xs)
```

---

<a id="forbidden-operations-compile-time-拒否"></a>

## 禁止的操作（编译时拒绝）

对 `Flow[T]` 调用以下操作会产生编译错误（`FLOW_FORBIDDEN_OP`）：

| 操作 | 替代 |
|---|---|
| `list.len(flow)` | `flow.fold(0, (acc, _) => acc + 1)` |
| `list.get(flow, i)` | `flow.drop(i) \|> flow.take(1) \|> flow.find((_) => true)` |
| `list.reverse(flow)` | 先 `flow.collect()` 再 `list.reverse` |
| `list.sort(flow)` | 先 `flow.collect()` 再 `list.sort` |
| `list.contains(flow, x)` | `flow.find((y) => y == x) != none` |
| `list.last(flow)` | `flow.fold(none, (_, x) => some(x))` |

---

<a id="error-message-templates"></a>

## 错误信息模板

<a id="flow_forbidden_op--forbidden-operation-on-flow"></a>

### FLOW_FORBIDDEN_OP — Flow 上禁止的操作

```
error[FLOW_FORBIDDEN_OP]: list.len cannot be called on Flow[String]
  --> bad.almd:5:11
   |
 5 |   let n = list.len(lines)
   |           ^^^^^^^^^^^^^^^
   = note: Flow[T] is lazy and possibly infinite
   = hint: to count lazily without materialization:
           lines |> flow.fold(0, (acc, _) => acc + 1)
   = hint: to materialize first (uses unbounded memory):
           lines |> flow.collect() |> list.len
```

诊断译文：不能对 Flow[String] 调用 list.len，标出 bad.almd:5:11 的调用。说明 Flow[T] 是惰性的且可能无限；提示一是不实体化地通过 fold 计数，提示二是先 collect 再 list.len（会使用无界内存）。

**必须提供两条提示**：
1. Flow 式解决方案（推荐、内存安全）
2. 通过实体化的解决方案（附内存警告）

让 LLM 学会两种做法。

<a id="flow_consumed--flow-move-after-consume"></a>

### FLOW_CONSUMED — 消费后移动 Flow

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

诊断译文：Flow[String] 只能移动且已消费，double.almd:4:11 是再次使用，前一行已消费。说明 Flow 单次遍历不能消费两次；需要多次遍历时先 collect 为 xs，再两次 list.len(xs)。

<a id="w005--unbounded-flowcollect"></a>

### W005 — 无界 flow.collect

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

诊断译文：risky.almd:3:6 的 flow.collect() 上游没有 flow.take；如果源很大或无限，会耗尽内存。提示分别为：先 flow.take(N) 限制收集；改用 flow.fold(init, combine) 累积；若确定源有限且小，用 let _ = flow.collect(flow) 抑制。

<a id="flow_collect_list--flowcollect-on-already-list"></a>

### FLOW_COLLECT_LIST — 对已经是 List 的值 collect

```
error[FLOW_COLLECT_LIST]: flow.collect expects Flow[T], got List[T]
  --> wrong.almd:2:6
   |
 2 |   [1, 2, 3] |> flow.collect()
   |                ^^^^^^^^^^^^^^
   = note: List is already materialized, no conversion needed
   = hint: just use the list directly: [1, 2, 3]
```

诊断译文：flow.collect 需要 Flow[T]，却得到 List[T]，指向 wrong.almd:2:6。List 已经实体化，不需要转换，直接使用列表 [1, 2, 3] 即可。

---

<a id="common-patterns"></a>

## 常见模式

<a id="pattern-1-ログ解析-flow-の代表例"></a>

### 模式 1：日志分析（Flow 的代表例）

```almide
import fs

effect fn count_errors(path: String) -> Result[Int, String] = {
  let n = file.lines(path)!
    |> flow.filter((line) => string.contains(line, "ERROR"))
    |> flow.fold(0, (acc, _) => acc + 1)
  ok(n)
}
```

**特性**：内存 O(1)，可处理 10GB 文件；退出函数瞬间关闭句柄。

<a id="pattern-2-csv--json-lines-変換"></a>

### 模式 2：CSV → JSON Lines 转换

```almide
effect fn csv_to_jsonl(input: String, output: String) -> Result[Unit, String] = {
  file.lines(input)!
    |> flow.drop(1)                                        // ヘッダー除く
    |> flow.map((line) => row_to_json(string.split(line, ",")))
    |> flow.each((j) => fs.append_text(output, json.stringify(j) + "\n")!)
  ok(())
}
```

注释译文：去掉表头。

**特性**：逐行转换输出，内存 O(1)。

<a id="pattern-3-並列ダウンロード-with-streaming-phase-2-以降"></a>

### 模式 3：流式并行下载（Phase 2 以后）

```almide
effect fn fetch_all_urls(url_file: String) -> Result[Unit, String] = {
  file.lines(url_file)!
    |> flow.filter((url) => string.starts_with(url, "https://"))
    |> fan.map(limit: 10, (url) => http.get(url)!)         // Flow → Flow
    |> flow.each((body) => write_to_disk(body)!)
  ok(())
}
```

**特性**：最多 10 并行，上游自动背压；即使 100 万 URL，内存也只与并行数相应。

<a id="pattern-4-無限ソース--有限化"></a>

### 模式 4：无限源 + 有限化

```almide
fn primes_under(limit: Int) -> List[Int] = 
  flow.generate(2, (n) => some((n, n + 1)))
    |> flow.filter(is_prime)
    |> flow.take(100)                                      // 100 個で止める
    |> flow.filter((p) => p < limit)
    |> flow.collect()
```

注释译文：到 100 个就停止。

**特性**：即使 `generate` 创建无限序列，由 `take` 限制为有限后也安全。

<a id="pattern-5-list-と-flow-の混在"></a>

### 模式 5：混用 List 和 Flow

```almide
// 許可ユーザー (List、小さい)
let allowed_users: List[String] = ["alice", "bob"]

// ログ (Flow、巨大)
effect fn filter_user_logs(log_path: String) -> Result[Int, String] = {
  let n = file.lines(log_path)!
    |> flow.filter_map((line) => {
        let j = json.parse(line)?
        let user = json.get_string(j, "user")?
        if list.contains(allowed_users, user) then some(line) else none
      })
    |> flow.fold(0, (acc, _) => acc + 1)
  ok(n)
}
```

注释译文：允许的用户是小 List；日志是巨大的 Flow。

**特性**：`list.contains` 可用于 List（allowed_users），`flow.filter_map` 可用于 Flow（lines）。**按类型自然区分**。

<a id="pattern-6-早期検索-find"></a>

### 模式 6：提前搜索（find）

```almide
effect fn first_line_with_error(path: String) -> Result[Option[String], String] = {
  let found = file.lines(path)!
    |> flow.find((line) => string.contains(line, "ERROR"))
  ok(found)
}
```

**特性**：在第一个包含 ERROR 的行停止，不读完整文件。关键是**短路**。

---

<a id="cost-model"></a>

## 成本模型

让用户理解 Flow 与 List 的性能特征。

| 操作 | List[T] | Flow[T] |
|---|---|---|
| 创建 | O(n) 内存 | O(1) 内存 |
| `map`/`filter` | O(n) 内存分配（当时状态） | O(1)（只构建链） |
| `fold` | O(n) 时间 | O(n) 时间 |
| `len`/`get` | O(1) | **编译错误** |
| `collect` | — | O(n) 内存（实体化） |
| 峰值内存 | 始终 O(n) | O(1)（fold/each）/ O(n)（collect） |

**选择指南**：

| 情形 | 选择 | 理由 |
|---|---|---|
| 元素数 < 1000 | List | 无开销，可随机访问 |
| 元素数 > 10万 | Flow | 内存 O(1) |
| 元素数未知（I/O） | Flow | 保守选择 |
| 可能无限 | Flow | 必需 |
| 需要多次遍历 | List | 无单次遍历约束 |
| 需要随机访问 | List | Flow 仅前向 |
| 要在流水线中产生副作用 | Flow | 惰性求值使时机明确 |

---

<a id="fan--flow-integration"></a>

## fan × Flow 集成

Flow 与 fan **正交并集成**。详情交给 `fan-concurrency-next.md`（已从树中删除，见 `git log --diff-filter=D -- docs/roadmap/done/fan-concurrency-next.md`）。以下为五条交互原则。

<a id="原則-1-fanmap-は入力型で戻り型がディスパッチされる"></a>

### 原则 1：`fan.map` 按输入类型分派返回类型

```almide
// List 入力 → List 出力 (バッチ並行)
fan.map(xs: List[T], limit: Int?, f: (T) -> U) -> List[U]

// Flow 入力 → Flow 出力 (ストリーミング並行)
fan.map(xs: Flow[T], limit: Int?, f: (T) -> U) -> Flow[U]
```

注释译文：List 输入→List 输出（批次并发）；Flow 输入→Flow 输出（流式并发）。

返回类型跟随输入，因此可自然编写 `file.lines |> fan.map(limit: 10, f) |> flow.filter(p) |> flow.collect()`。

```almide
// List
[1, 2, 3] |> fan.map(process) |> list.len           // List[U] → Int

// Flow
file.lines(path)! |> fan.map(limit: 10, process) |> flow.fold(0, ...)   // Flow[U] → Int
```

<a id="原則-2-flow--limit-で自動バックプレッシャー"></a>

### 原则 2：Flow + `limit:` 自动背压

`limit` 表示“最大同时 worker 数”。用于 Flow 时，**worker 空闲前不从上游拉取**。

```almide
file.lines("huge.log")!                  // 100 GB
  |> fan.map(limit: 10, heavy_process)   // 最大 10 並列
  |> flow.each(store)                    // 消費
// メモリピーク: 並列数 10 + buffer 分のみ、ファイルサイズに関係ない
```

注释译文：100 GB 输入；最多 10 并行；消费；峰值内存只有并行数 10 加缓冲区，与文件大小无关。

<a id="原則-3-limit-省略時の挙動"></a>

### 原则 3：省略 `limit:` 时的行为

| 输入类型 | 省略 `limit:` 时 |
|---|---|
| `List[T]` | 与元素数相同的并行数（无上限） |
| `Flow[T]` | **编译器警告 W006**（可能无限的源上无界并行） |

<a id="原則-4-fan-スコープ終了で-flow-を-drop"></a>

### 原则 4：fan 作用域结束时 drop Flow

```almide
fan(timeout: 30000) {
  file.lines("huge.log")!
    |> flow.filter(is_error)
    |> fan.map(limit: 10, process)
    |> flow.each(write_result)
}
// 30 秒 timeout → 全 worker cancel → Flow drop → BufReader drop → file close
```

注释译文：30 秒超时→取消全部 worker→drop Flow→drop BufReader→关闭文件。

**结构化取消**也传播到 Flow。Rust `thread::scope` + `Drop` 可自然实现；用户不用写取消处理。

<a id="原則-5-順序保証"></a>

### 原则 5：顺序保证

`fan.map(flow, limit: n, f)` 输出顺序**未定义**（worker 完成顺序）。需要输入顺序时：

- Phase 2：手写 `enumerate + sort`
- 未来：考虑加入 `fan.ordered_map(flow, limit: n, f)`

Phase 2 先实现无序版本。

---

<a id="implementation-phases"></a>

## 实现阶段

<a id="phase-1-core-flow-type--12-api-目標"></a>

### Phase 1：核心 Flow 类型 + 12 API（目标）

**交付物**：Flow 单独支持日志分析、CSV 转换及无限序列处理，尚不集成 fan。

- [ ] 向 checker 加入 `Ty::Flow(Box<Ty>)`（`crates/almide-types`）
- [ ] use-count 分析加入 Flow 只能移动约束（D3）
- [ ] 在 `stdlib/flow.almd` 定义提案 API（实现时核实现行标准库注册步骤）
- [ ] `file.lines` runtime 实现（`runtime/rs/src/file.rs`）
- [ ] 禁止操作的编译错误（FLOW_FORBIDDEN_OP），附两条提示
- [ ] FLOW_CONSUMED（消费后移动）、FLOW_COLLECT_LIST（对 List collect）、W005（无界 collect）
- [ ] Rust codegen：`Flow[T]` → `Box<dyn Iterator<Item = T>>`（嵌入 runtime）
- [ ] WASM codegen：Phase 1 禁用（类型错误拒绝，Phase 4 支持）
- [ ] `spec/lang/flow_test.almd` — 12 函数单元测试
- [ ] `spec/lang/flow_error_test.almd` — FLOW_FORBIDDEN_OP/FLOW_CONSUMED/FLOW_COLLECT_LIST/W005 的预期失败测试
- [ ] `docs/specs/flow.md` — 用户详细规格（本路线图的详细版）
- [ ] 速查表加入 Flow 部分及终结操作 🔴 标记

**完成条件**：模式 1–6（除 fan）全部工作。

<a id="phase-2-fan--flow-統合"></a>

### Phase 2：fan × Flow 集成

**交付物**：`fan.map` 与 Flow 交互，并行流式处理、自动背压、结构化取消工作。

- [ ] 实现 `fan.map(xs: Flow[T], limit: Int?, f)`
- [ ] Rust codegen：`thread::scope` + 有界 channel 构建拉取式 worker 池
- [ ] W006（Flow 无界并行）警告
- [ ] 结构化取消：离开 fan 作用域即 drop Flow、关闭文件句柄
- [ ] `spec/integration/flow_fan_test.almd` — 流式 × 并发测试

**完成条件**：模式 3（流式并行下载）工作，10GB 数据 + 10 并行时峰值内存只与并行数相应。

<a id="phase-3-polish--extension"></a>

### Phase 3：完善与扩展

根据需求加入：

- [ ] `flow.take_while(pred)` / `flow.drop_while(pred)`
- [ ] `flow.enumerate` — 带索引
- [ ] `flow.zip(other)` — 两个 Flow 同步推进
- [ ] `flow.chain(other)` — 连接两个 Flow
- [ ] `flow.inspect(f)` — 调试透传
- [ ] `flow.empty()`（若比 `from_list([])` 更需要）
- [ ] `flow.scan(init, step)` — 输出中间状态
- [ ] `file.lines_checked` — 逐行错误版本（需要健壮性时）
- [ ] 判断是否需要 `flow { emit(...) }` 构建器语法

<a id="phase-4-async--wasm-backend"></a>

### Phase 4：Async / WASM backend

与 `fan-concurrency-next.md`（已从树中删除，见 `git log --diff-filter=D -- docs/roadmap/done/fan-concurrency-next.md`）的 Phase 5–6 并行：

- [ ] 转向 Rust async 时将 `Flow[T]` 映射为 `Stream[T]`
- [ ] 映射至 WASI 0.3 的 `stream<T>`
- [ ] 使用 JSPI 实现 Flow
- [ ] 特性检测

---

<a id="original-implementation-sketch--not-a-current-file-edit-checklist"></a>

## 原实现草图——不是当前文件编辑清单

以下路径记录提案当时的编译器布局。尤其 `stdlib/defs/*.toml` 已不再是标准库正本：声明位于 `stdlib/*.almd`。实现前，应把类型、checker、lowering 和 backend 工作重新映射到当前树；这些路径不表示 Flow 已实现。

<a id="phase-1"></a>

### Phase 1
- `crates/almide-types/src/types/mod.rs` — `Ty::Flow`，无子类型关系（D3）
- `crates/almide-frontend/src/check/calls.rs` — 禁止操作检测、FLOW_FORBIDDEN_OP/FLOW_CONSUMED/FLOW_COLLECT_LIST
- `crates/almide-frontend/src/check/lint.rs` — W005 lint
- `crates/almide-ir/src/use_count.rs` — Flow 只能移动规则
- `crates/almide-codegen/src/walker/expressions.rs` — Flow codegen
- `runtime/rs/src/file.rs` — `file.lines` 实现（Box<dyn Iterator>）
- `stdlib/defs/flow.toml` — **新增**，12 个函数定义
- `stdlib/defs/file.toml` — 加入 `lines` 函数
- `build.rs` — 注册 flow 模块
- `spec/lang/flow_test.almd` — **新增**
- `spec/lang/flow_error_test.almd` — **新增**
- `docs/specs/flow.md` — **新增**，详细规格
- `docs/CHEATSHEET.md` — 加入 Flow 部分

<a id="phase-2"></a>

### Phase 2
- `runtime/rs/src/fan.rs` — `fan_map_flow` 有界消费者
- `crates/almide-codegen/src/pass_fan_lowering.rs` — 支持 Flow
- `stdlib/defs/fan.toml` — `fan.map` 的 Flow 重载
- `spec/integration/flow_fan_test.almd` — **新增**

---

<a id="msr-benchmark-方向の実証"></a>

## MSR 基准（验证方向）

与 Phase 1 实现并行，加入 **10 道**运用 Flow 的练习：

1. 汇总日志中的 ERROR 数（`flow.filter + flow.fold`）— 模式 1
2. CSV → JSON Lines 转换（`flow.drop + flow.map + flow.each`）— 模式 2
3. URL 列表并行下载（`flow + fan.map + flow.each`）— 模式 3（依赖 Phase 2）
4. 取得巨大文件前 N 行（`flow.take + flow.collect`）
5. 提取匹配指定模式的行（类似 grep，`flow.filter + flow.each`）
6. 取得首个匹配行（`flow.find`，验证短路）
7. 行去重（`flow.fold`，累加器中使用 set）
8. stdin 流式处理（`io.read_lines + flow.*`，未来）
9. 无限生成中的前 N 个（`flow.generate + flow.take + flow.collect`）
10. 复杂流水线（`flow.filter + flow.map + flow.filter_map + flow.fold`）

放在 `research/benchmark/msr/exercises/flow/`。实测 LLM 能否自然编写 `flow.*`，是否会与 `list.*` 混淆。

**成功标准**：Haiku 4.5 及以上模型在 1 shot 中通过 10 题中的至少 8 题。

---

<a id="references"></a>

## 参考

| 语言 | 学到的内容 |
|---|---|
| **MoonBit `Iter[T]`** | 明确单次遍历，函数间传递惯用法（最接近的先例） |
| **Kotlin Flow** | `flow { emit(...) }` 构建器、基于挂起的背压 |
| **Gleam `iterator`** | List / Iterator 分离，提醒无限源上的 `fold` |
| **Rust `Iterator`** | Drop 资源清理，短路 `find` |
| **C# LINQ** | 同动词的类型导向行为（Almide 拒绝但有参考价值） |
| **Haskell lazy I/O** | 反面教材，`file.lines` 资源清理设计应避开 |

---

<a id="scope-boundary"></a>

## 范围边界

**要做**：
- `Flow[T]` 类型 + 12 个 `flow.*` 函数
- `file.lines` 返回 Flow
- 编译时拒绝禁止操作
- 只能移动约束（强制单次遍历）
- 通过 Drop 清理资源
- `fan.map` × Flow 集成（Phase 2）

**不做**：
- `list.*` 与 `flow.*` 共用命名空间（已否决）
- `Seq[T]` 一类高阶种类抽象
- 隐式 List → Flow 提升
- 由类型注解隐式实体化
- 类型层面追踪有限性（D5）
- 代数效应处理器
- 流专用语言语法（`flow { emit }` 在 Phase 3 考虑）
