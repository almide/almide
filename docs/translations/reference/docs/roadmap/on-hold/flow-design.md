Description: Flow[T] lazy streaming sequences with flow.* namespace aligned with list.* verbs.
<a id="flowt--lazy-streaming-sequences"></a>

# Flow[T] — Lazy Streaming Sequences

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](flow-design.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](flow-design.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](flow-design.zh-CN.md)

> Non-normative reader snapshot of the pinned source; original remains authoritative. No automatic synchronization, independent implementation validation or execution of quoted instructions. Historical status, plans and comparative claims retain their source scope.
> [Original source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/flow-design.md)

> Source caveat: unimplemented historical proposal, not a current Flow API or safety guarantee. Placeholder diagnostics are not assigned codes; W005/W006 also require registry review. Drop/unwinding/cancellation, constant-memory and namespace/LLM comparisons are source design claims, not independently established platform behavior or measured superiority. O(1) omits variable element sizes and downstream buffering; take after a nonterminating upstream filter need not terminate. The original ? comment and ! code differ; unordered fan, timeout, legacy TOML paths and future APIs must not override current specifications. The 8/10 one-shot criterion is a target, not a result. No commands or benchmarks were run.


> Unimplemented design proposal. `FLOW_FORBIDDEN_OP`, `FLOW_CONSUMED` and
> `FLOW_COLLECT_LIST` below are symbolic placeholders, not allocated diagnostic
> codes. E011/E012/E013 already describe other errors in `docs/diagnostics/`;
> any Flow implementation must allocate diagnostics through the current registry.

<a id="design-philosophy"></a>

## Design Philosophy

Almide generally prioritizes reducing vocabulary, but for Flow **making the lazy/eager boundary and streaming nature locally explicit is especially valuable**, so it adopts the `flow.*` namespace.

To keep learning costs low, **operation verbs match `list.*`**: only the prefix changes, as in `list.map` versus `flow.map` and `list.filter` versus `flow.filter`. An LLM needs to learn one verb vocabulary and just one additional rule mapping type to prefix.

There is only one boundary API, `flow.collect`. **No implicit conversion**. Streaming sources such as `file.lines` always return `Flow[T]`.

> Almide generally prioritizes reducing vocabulary, but for Flow making the lazy/eager boundary and streaming nature locally explicit is especially valuable, so it adopts the `flow.*` namespace. To reduce learning cost, however, operation names match `list.*` as closely as possible.

Place this paragraph at the beginning of `docs/specs/flow.md` too, so everyone shares the principle.

<a id="why-flow-not-shared-list"></a>

## Why `flow.*` (not shared `list.*`)

The proposal to make `list.*` support both List[T] and Flow[T] was rejected for these reasons:

| Reason for rejection | Details |
|---|---|
| **No precedent** | MoonBit/Gleam/Kotlin/Rust all separate the namespaces |
| **Unnatural user functions** | Needs higher-kinded abstraction such as `Seq[T]`, with high implementation cost |
| **No transfer learning benefit** | LLMs have seen extensive `iterator.filter`/`stream.map` in training data |
| **Hard to track long-distance dependencies** | Without type information in the module name, an LLM cannot follow distant definitions |
| **Polymorphic stdlib / monomorphic user inconsistency** | “Magic only in stdlib, ordinary user code” confuses LLMs most |

Conservative `flow.*` separation with aligned verbs **better fits Almide's mission: the language LLMs can write most correctly**.

<a id="core-type"></a>

## Core Type

`Flow[T]` is a **lazy, single-pass, forward-only** sequence (corresponding to MoonBit's `Iter[T]`).

| Property | Meaning |
|---|---|
| **lazy** | Does not pull values until a terminal operation (fold/each/collect/find) is called |
| **single-pass** | Cannot be reused once consumed; double use is a compile error (below) |
| **forward-only** | No random access; `len`, `get`, `reverse`, `sort` are type errors |

<a id="decided-semantics"></a>

## Decided Semantics

Decide the following before Phase 1. Ambiguity here bogs down implementation.

<a id="d1-filelines-の型-effect--lazy-境界"></a>

### D1. Type of `file.lines` (Effect × lazy boundary)

```almide
effect fn file.lines(path: String) -> Flow[String]
```

**Decision**: Opening the file is fallible (via `effect fn` → `Result`); read errors after opening panic.

Reasons:
- Common errors such as a missing file can be handled with `!` or `?`
- Mid-read errors (such as disk disconnection) are extremely rare; panic suffices
- Wrapping every line in `Result[String, IOError]` makes pipelines verbose (see Rust's verbose `Lines::Item = io::Result<String>`)
- A pragmatic 80/20 choice

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

Code-comment translation: usage—return early with ? when opening fails; the marked call immediately yields Err if opening fails. The original comment says ? while the code uses !; neither is silently changed.

For robust per-line error handling in the future:
```almide
effect fn file.lines_checked(path: String) -> Flow[Result[String, IOError]]
```
consider this in Phase 3 or later. Phase 1 includes only `file.lines`.

<a id="d2-resource-cleanup-drop-意味論"></a>

### D2. Resource cleanup (Drop semantics)

**Decision**: Flow cleans up through Rust's `Drop` trait. Users write nothing.

```
Flow の内部表現: Box<dyn Iterator<Item = T>>
           ↓
File-backed Flow: BufReader<File> を Iterator として wrap
           ↓
Iterator が drop される時点で BufReader → File と順に drop
           ↓
ファイルハンドル自動 close
```

Diagram translation: Flow's internal representation is Box<dyn Iterator<Item = T>> → a file-backed Flow wraps BufReader<File> as an Iterator → when the Iterator is dropped, BufReader and then File are dropped → the file handle closes automatically.

Guaranteed behavior:

1. **Leaving scope**: exiting the function drops Flow → cleans up upstream resources
2. **Early termination** (no consumption after `take(10)`): after pulling 10 elements, Flow drops → remaining input unread and handle closed
3. **panic**: panic during fold unwinds the stack → drops Flow → cleanup
4. **Failure in fan scope**: a `fan.map` worker fails → scope exits → Flow drops → cleanup

**Rust Drop naturally avoids the Haskell lazy I/O failure pattern of handles outliving their scope**.

<a id="d3-single-pass-違反検出"></a>

### D3. Detecting single-pass violations

**Decision**: Flow is **move-only**. Almide ownership analysis makes double use a compile error.

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

Code-comment translation: the first fold consumes (moves) flow; the second tries to reuse it after consumption. The displayed error points to both the consumption and reuse and advises materializing first if two passes are needed.

Implementation only requires adding a “Flow cannot be cloned” rule to Almide's existing use-count analysis (stricter than ordinary types: Move only).

<a id="d4-flowcollect-without-flowtake-は-lint-warning"></a>

### D4. `flow.collect` without `flow.take` is a lint warning

**Decision**: Bare `flow.collect(flow)` (no upstream `take`) produces a lint warning.

```almide
file.lines(path)!
  |> flow.filter(p)
  |> flow.collect()       // ⚠️ warning: unbounded flow.collect
// warning[W005]: flow.collect() without upstream flow.take may OOM
//   hint: add `|> flow.take(n)` upstream if the source may be large,
//         or suppress with `_ = flow.collect(...)` if you are sure
```

This is a warning, not an error. It is fine if the user knows the upstream is finite, but it prevents an LLM from mechanically appending `collect()`.

<a id="d5-flowgenerate-の有限性は型で区別しない"></a>

### D5. Do not distinguish `flow.generate` finiteness in the type

**Decision**: `flow.generate(seed, step)` ends when step returns `None`. Its type is always `Flow[T]` (no finite/infinite distinction).

```almide
flow.generate(0, (n) => if n < 10 then some((n, n + 1)) else none)   // 有限 (10 要素)
flow.generate(0, (n) => some((n, n + 1)))                             // 無限
// どちらも Flow[Int]、型は同じ
```

Code-comment translation: the first generator is finite (10 elements), the second infinite; both have the same Flow[Int] type.

Finiteness is **hard to quantify**, so types do not track it. D4's lint prevents infinite-source accidents.

---

<a id="minimum-api-12-functions"></a>

## Minimum API (12 functions)

<a id="transformations-6"></a>

### Transformations (6)

**Use exactly the same verbs** as `list.*`.

```almide
flow.map[T, U](xs: Flow[T], f: (T) -> U) -> Flow[U]
flow.filter[T](xs: Flow[T], pred: (T) -> Bool) -> Flow[T]
flow.filter_map[T, U](xs: Flow[T], f: (T) -> Option[U]) -> Flow[U]
flow.flat_map[T, U](xs: Flow[T], f: (T) -> Flow[U]) -> Flow[U]
flow.take[T](xs: Flow[T], n: Int) -> Flow[T]
flow.drop[T](xs: Flow[T], n: Int) -> Flow[T]
```

<a id="terminal-operations-4"></a>

### Terminal operations (4)

**Evaluation runs here**. Mark these with 🔴 in documentation and the cheatsheet.

```almide
flow.fold[T, U](xs: Flow[T], init: U, combine: (U, T) -> U) -> U       // 🔴
flow.each[T](xs: Flow[T], f: (T) -> Unit) -> Unit                       // 🔴
flow.collect[T](xs: Flow[T]) -> List[T]                                 // 🔴
flow.find[T](xs: Flow[T], pred: (T) -> Bool) -> Option[T]               // 🔴 短絡
```

The find signature's Japanese comment means short-circuit.

`flow.find` **short-circuits** (stops pulling upstream at the first match). That is its difference from `fold`. Including `find` avoids the typical bug of implementing a short-circuit search with fold and accidentally scanning everything.

<a id="source-constructors-2"></a>

### Source constructors (2)

```almide
flow.from_list[T](xs: List[T]) -> Flow[T]
flow.generate[S, T](seed: S, step: (S) -> Option[(T, S)]) -> Flow[T]
```

Use `flow.from_list([])` instead of `flow.empty`. The `flow { emit(...) }` builder is Phase 3 or later.

<a id="組み込みソース-stdlib-他モジュール経由"></a>

### Built-in sources (through other stdlib modules)

- `file.lines(path: String) -> Flow[String]` (see D1, `effect fn`)
- Future: `io.read_lines() -> Flow[String]` (stdin streaming)
- Future: `http.stream(url: String) -> Flow[Bytes]`

---

<a id="key-rules-決定事項"></a>

## Key Rules (decisions)

<a id="r1-filelines-は-flowstring-を返す"></a>

### R1. `file.lines` returns `Flow[String]`

For reading an entire small file, use `fs.read_text` + `string.lines`. **No ambiguity**.

```almide
// 小さい config → fs.read_text + string.lines
let content = fs.read_text("config.toml")!
let lines: List[String] = string.lines(content)

// 巨大ログ → file.lines
let lines: Flow[String] = file.lines("system.log")!
```

Code-comment translation: small configuration → fs.read_text + string.lines; huge log → file.lines.

<a id="r2-flowcollect-が唯一の-flow--list-境界"></a>

### R2. `flow.collect` is the only Flow → List boundary

No implicit conversion, including coercion by type annotation.

```almide
let xs: List[String] = flow |> flow.collect()   // 明示
```

The comment means explicit conversion.

<a id="r3-flowtaken-の戻り型は-flowt"></a>

### R3. `flow.take(n)` returns `Flow[T]`

No special case. Append `|> flow.collect()` if a List is wanted.

```almide
let first_10: List[String] = file.lines(path)! |> flow.take(10) |> flow.collect()
```

<a id="r4-terminal-operations-は-docs-で明示マーク"></a>

### R4. Explicitly mark terminal operations in documentation

In the cheatsheet, mark `fold`, `each`, `collect`, `find` with 🔴 terminal, visually emphasizing “execution happens here.”

<a id="r5-list-と-flow-の動詞は必ず揃える"></a>

### R5. Always align verbs in `list.*` and `flow.*`

Functions present in both use **the same verb**. Do not rename one side only.

**Forbidden examples**:
- ❌ `flow.keep_if` (if list uses `filter`, flow also uses `filter`)
- ❌ `flow.transform` (if list uses `map`, flow also uses `map`)
- ❌ `flow.reduce` (if list uses `fold`, flow also uses `fold`)
- ❌ `flow.fetch_first` (if list uses `find`, flow also uses `find`)

<a id="r6-single-pass-を-cheatsheet-冒頭で宣言"></a>

### R6. Declare single-pass at the start of the cheatsheet

```markdown
## Flow の基本性質

Flow は single-pass です。一度 consume (fold/each/collect/find) した Flow は再利用できません。
Almide は所有権解析で二重使用をコンパイル時に検出します。
```

Quoted cheatsheet translation: “Flow basics. Flow is single-pass. A Flow once consumed (fold/each/collect/find) cannot be reused. Almide detects double use at compile time through ownership analysis.”

<a id="r7-listt--flowt-は-flowfrom_list-で明示"></a>

### R7. Make `List[T] → Flow[T]` explicit with `flow.from_list`

No automatic promotion. **Write it when using it**.

```almide
let xs: List[Int] = [1, 2, 3]
let f: Flow[Int] = flow.from_list(xs)
```

---

<a id="forbidden-operations-compile-time-拒否"></a>

## Forbidden Operations (compile-time rejection)

Calling the following on `Flow[T]` is a compile error (`FLOW_FORBIDDEN_OP`):

| Operation | Alternative |
|---|---|
| `list.len(flow)` | `flow.fold(0, (acc, _) => acc + 1)` |
| `list.get(flow, i)` | `flow.drop(i) \|> flow.take(1) \|> flow.find((_) => true)` |
| `list.reverse(flow)` | `flow.collect()` followed by `list.reverse` |
| `list.sort(flow)` | `flow.collect()` followed by `list.sort` |
| `list.contains(flow, x)` | `flow.find((y) => y == x) != none` |
| `list.last(flow)` | `flow.fold(none, (_, x) => some(x))` |

---

<a id="error-message-templates"></a>

## Error Message Templates

<a id="flow_forbidden_op--forbidden-operation-on-flow"></a>

### FLOW_FORBIDDEN_OP — Forbidden operation on Flow

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

**Always provide two hints**:
1. A Flow-style solution (recommended, memory-safe)
2. A materialization solution (with a memory warning)

So LLMs can learn both.

<a id="flow_consumed--flow-move-after-consume"></a>

### FLOW_CONSUMED — Flow move after consume

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

<a id="w005--unbounded-flowcollect"></a>

### W005 — Unbounded flow.collect

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

<a id="flow_collect_list--flowcollect-on-already-list"></a>

### FLOW_COLLECT_LIST — flow.collect on already-List

```
error[FLOW_COLLECT_LIST]: flow.collect expects Flow[T], got List[T]
  --> wrong.almd:2:6
   |
 2 |   [1, 2, 3] |> flow.collect()
   |                ^^^^^^^^^^^^^^
   = note: List is already materialized, no conversion needed
   = hint: just use the list directly: [1, 2, 3]
```

---

<a id="common-patterns"></a>

## Common Patterns

<a id="pattern-1-ログ解析-flow-の代表例"></a>

### Pattern 1: Log analysis (the representative Flow example)

```almide
import fs

effect fn count_errors(path: String) -> Result[Int, String] = {
  let n = file.lines(path)!
    |> flow.filter((line) => string.contains(line, "ERROR"))
    |> flow.fold(0, (acc, _) => acc + 1)
  ok(n)
}
```

**Characteristics**: O(1) memory, supports 10GB files; the handle closes as soon as the function exits.

<a id="pattern-2-csv--json-lines-変換"></a>

### Pattern 2: CSV → JSON Lines conversion

```almide
effect fn csv_to_jsonl(input: String, output: String) -> Result[Unit, String] = {
  file.lines(input)!
    |> flow.drop(1)                                        // ヘッダー除く
    |> flow.map((line) => row_to_json(string.split(line, ",")))
    |> flow.each((j) => fs.append_text(output, json.stringify(j) + "\n")!)
  ok(())
}
```

Code-comment translation: skip the header.

**Characteristics**: convert and output one line at a time. O(1) memory.

<a id="pattern-3-並列ダウンロード-with-streaming-phase-2-以降"></a>

### Pattern 3: Parallel download with streaming (Phase 2 onward)

```almide
effect fn fetch_all_urls(url_file: String) -> Result[Unit, String] = {
  file.lines(url_file)!
    |> flow.filter((url) => string.starts_with(url, "https://"))
    |> fan.map(limit: 10, (url) => http.get(url)!)         // Flow → Flow
    |> flow.each((body) => write_to_disk(body)!)
  ok(())
}
```

**Characteristics**: at most 10 concurrent operations, automatic upstream backpressure, memory only proportional to concurrency even for one million URLs.

<a id="pattern-4-無限ソース--有限化"></a>

### Pattern 4: Infinite source + bounding

```almide
fn primes_under(limit: Int) -> List[Int] = 
  flow.generate(2, (n) => some((n, n + 1)))
    |> flow.filter(is_prime)
    |> flow.take(100)                                      // 100 個で止める
    |> flow.filter((p) => p < limit)
    |> flow.collect()
```

Code-comment translation: stop at 100 items.

**Characteristics**: even an infinite sequence from `generate` is safe when bounded by `take`.

<a id="pattern-5-list-と-flow-の混在"></a>

### Pattern 5: Mixing List and Flow

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

Code-comment translation: allowed users are a small List; logs are a huge Flow.

**Characteristics**: `list.contains` can be called on the List (allowed_users); `flow.filter_map` on the Flow (lines). **Natural selection according to type**.

<a id="pattern-6-早期検索-find"></a>

### Pattern 6: Early search (find)

```almide
effect fn first_line_with_error(path: String) -> Result[Option[String], String] = {
  let found = file.lines(path)!
    |> flow.find((line) => string.contains(line, "ERROR"))
  ok(found)
}
```

**Characteristics**: stops at the first line containing ERROR, without reading the whole file. **Short-circuiting** is the key.

---

<a id="cost-model"></a>

## Cost Model

Help users understand Flow and List performance characteristics.

| Operation | List[T] | Flow[T] |
|---|---|---|
| Creation | O(n) memory | O(1) memory |
| `map`/`filter` | O(n) allocation (current) | O(1) (chain construction only) |
| `fold` | O(n) time | O(n) time |
| `len`/`get` | O(1) | **Compile error** |
| `collect` | — | O(n) memory (materialization) |
| Peak memory | O(n) always | O(1) (fold/each) / O(n) (collect) |

**Selection guidelines**:

| Situation | Choice | Reason |
|---|---|---|
| Elements < 1000 | List | No overhead; random access supported |
| Elements > 100,000 | Flow | O(1) memory |
| Unknown element count (I/O) | Flow | Conservative choice |
| Possibly infinite | Flow | Required |
| Multiple passes wanted | List | No single-pass restriction |
| Random access wanted | List | Flow is forward-only |
| Side effects wanted inside pipeline | Flow | Lazy evaluation makes timing explicit |

---

<a id="fan--flow-integration"></a>

## fan × Flow Integration

Flow and fan are **orthogonal yet integrated**. Details are delegated to `fan-concurrency-next.md` (removed from the tree — `git log --diff-filter=D -- docs/roadmap/done/fan-concurrency-next.md`). The five interaction principles follow.

<a id="原則-1-fanmap-は入力型で戻り型がディスパッチされる"></a>

### Principle 1: `fan.map` dispatches its return type by input type

```almide
// List 入力 → List 出力 (バッチ並行)
fan.map(xs: List[T], limit: Int?, f: (T) -> U) -> List[U]

// Flow 入力 → Flow 出力 (ストリーミング並行)
fan.map(xs: Flow[T], limit: Int?, f: (T) -> U) -> Flow[U]
```

Code-comment translation: List input → List output (batch concurrency); Flow input → Flow output (streaming concurrency).

The return type follows the input, allowing natural writing of `file.lines |> fan.map(limit: 10, f) |> flow.filter(p) |> flow.collect()`.

```almide
// List
[1, 2, 3] |> fan.map(process) |> list.len           // List[U] → Int

// Flow
file.lines(path)! |> fan.map(limit: 10, process) |> flow.fold(0, ...)   // Flow[U] → Int
```

<a id="原則-2-flow--limit-で自動バックプレッシャー"></a>

### Principle 2: Flow + `limit:` gives automatic backpressure

`limit` is the maximum simultaneous worker count. With Flow, **do not pull upstream until a worker becomes available**.

```almide
file.lines("huge.log")!                  // 100 GB
  |> fan.map(limit: 10, heavy_process)   // 最大 10 並列
  |> flow.each(store)                    // 消費
// メモリピーク: 並列数 10 + buffer 分のみ、ファイルサイズに関係ない
```

Code-comment translation: 100 GB input; at most 10 concurrent workers; consume; peak memory is only concurrency 10 plus buffers, independent of file size.

<a id="原則-3-limit-省略時の挙動"></a>

### Principle 3: Behavior when `limit:` is omitted

| Input type | When `limit:` omitted |
|---|---|
| `List[T]` | As much concurrency as the element count (unbounded) |
| `Flow[T]` | **Compiler warning W006** (unbounded parallelism on a possibly infinite source) |

<a id="原則-4-fan-スコープ終了で-flow-を-drop"></a>

### Principle 4: Drop Flow when fan scope exits

```almide
fan(timeout: 30000) {
  file.lines("huge.log")!
    |> flow.filter(is_error)
    |> fan.map(limit: 10, process)
    |> flow.each(write_result)
}
// 30 秒 timeout → 全 worker cancel → Flow drop → BufReader drop → file close
```

Code-comment translation: 30-second timeout → cancel all workers → drop Flow → drop BufReader → close file.

**Structured cancellation** also reaches Flow. Rust `thread::scope` + `Drop` can implement it naturally; users write no cancellation handling.

<a id="原則-5-順序保証"></a>

### Principle 5: Ordering guarantee

The output order of `fan.map(flow, limit: n, f)` is **undefined** (worker completion order). If input order is needed:

- Phase 2: write `enumerate + sort` manually
- Future: consider adding `fan.ordered_map(flow, limit: n, f)`

Phase 2 implements unordered first.

---

<a id="implementation-phases"></a>

## Implementation Phases

<a id="phase-1-core-flow-type--12-api-目標"></a>

### Phase 1: Core Flow type + 12 API (goal)

**Deliverable**: Flow alone supports log analysis, CSV conversion, infinite-sequence processing; no fan integration yet.

- [ ] Add `Ty::Flow(Box<Ty>)` to checker (`crates/almide-types`)
- [ ] Add Flow move-only restriction to use-count analysis (D3)
- [ ] Define proposed APIs in `stdlib/flow.almd` (check current stdlib registration procedure at implementation time)
- [ ] Runtime implementation of `file.lines` (`runtime/rs/src/file.rs`)
- [ ] Compile-time forbidden-op error (FLOW_FORBIDDEN_OP) with two hints
- [ ] FLOW_CONSUMED (move after consume), FLOW_COLLECT_LIST (collect on List), W005 (unbounded collect)
- [ ] Rust codegen: `Flow[T]` → `Box<dyn Iterator<Item = T>>` (embedded runtime)
- [ ] WASM codegen: disabled in Phase 1 (reject with type error; support in Phase 4)
- [ ] `spec/lang/flow_test.almd` — unit tests for 12 functions
- [ ] `spec/lang/flow_error_test.almd` — expect-fail tests for FLOW_FORBIDDEN_OP/FLOW_CONSUMED/FLOW_COLLECT_LIST/W005
- [ ] `docs/specs/flow.md` — detailed user specification (expanded version of this roadmap)
- [ ] Add Flow cheatsheet section and 🔴 terminal-operation marks

**Completion condition**: all Patterns 1–6 except fan work.

<a id="phase-2-fan--flow-統合"></a>

### Phase 2: fan × Flow integration

**Deliverable**: `fan.map` interacts with Flow; parallel streaming, automatic backpressure and structured cancellation work.

- [ ] Implement `fan.map(xs: Flow[T], limit: Int?, f)`
- [ ] Rust codegen: pull-based worker pool via `thread::scope` + bounded channel
- [ ] W006 warning (unbounded parallelism on Flow)
- [ ] Structured cancellation: leaving fan scope drops Flow and closes file handle
- [ ] `spec/integration/flow_fan_test.almd` — streaming × concurrency tests

**Completion condition**: Pattern 3 (parallel downloads with streaming) works; for 10GB data with 10 concurrent operations peak memory is proportional only to concurrency.

<a id="phase-3-polish--extension"></a>

### Phase 3: Polish & Extension

Add based on demand:

- [ ] `flow.take_while(pred)` / `flow.drop_while(pred)`
- [ ] `flow.enumerate` — with indices
- [ ] `flow.zip(other)` — two Flows in lockstep
- [ ] `flow.chain(other)` — concatenate two Flows
- [ ] `flow.inspect(f)` — debugging passthrough
- [ ] `flow.empty()` (if preferred to `from_list([])`)
- [ ] `flow.scan(init, step)` — emit intermediate state
- [ ] `file.lines_checked` — per-line errors (if robustness requires it)
- [ ] Decide whether `flow { emit(...) }` builder syntax is needed

<a id="phase-4-async--wasm-backend"></a>

### Phase 4: Async / WASM backend

In parallel with Phases 5–6 of `fan-concurrency-next.md` (removed from tree — `git log --diff-filter=D -- docs/roadmap/done/fan-concurrency-next.md`):

- [ ] Map `Flow[T]` → `Stream[T]` when moving to Rust async
- [ ] Map to WASI 0.3 `stream<T>`
- [ ] Flow implementation using JSPI
- [ ] Feature detection

---

<a id="original-implementation-sketch--not-a-current-file-edit-checklist"></a>

## Original implementation sketch — not a current file-edit checklist

The paths below record the proposal's original compiler layout. In particular,
`stdlib/defs/*.toml` is no longer the stdlib source of truth: declarations live
in `stdlib/*.almd`. Before implementation, remap the type/checker/lowering and
backend work to the current tree; these paths do not imply Flow is implemented.

<a id="phase-1"></a>

### Phase 1
- `crates/almide-types/src/types/mod.rs` — `Ty::Flow`, no subtype relationship (D3)
- `crates/almide-frontend/src/check/calls.rs` — forbidden-op detection, FLOW_FORBIDDEN_OP/FLOW_CONSUMED/FLOW_COLLECT_LIST
- `crates/almide-frontend/src/check/lint.rs` — W005 lint
- `crates/almide-ir/src/use_count.rs` — Flow move-only rule
- `crates/almide-codegen/src/walker/expressions.rs` — Flow codegen
- `runtime/rs/src/file.rs` — `file.lines` implementation (Box<dyn Iterator>)
- `stdlib/defs/flow.toml` — **new**, 12 function definitions
- `stdlib/defs/file.toml` — add `lines` function
- `build.rs` — register flow module
- `spec/lang/flow_test.almd` — **new**
- `spec/lang/flow_error_test.almd` — **new**
- `docs/specs/flow.md` — **new**, detailed specification
- `docs/CHEATSHEET.md` — add Flow section

<a id="phase-2"></a>

### Phase 2
- `runtime/rs/src/fan.rs` — `fan_map_flow` bounded consumer
- `crates/almide-codegen/src/pass_fan_lowering.rs` — Flow support
- `stdlib/defs/fan.toml` — Flow overload of `fan.map`
- `spec/integration/flow_fan_test.almd` — **new**

---

<a id="msr-benchmark-方向の実証"></a>

## MSR Benchmark (demonstrating the direction)

Alongside Phase 1 implementation, add **10 exercises** using Flow:

1. Count ERROR entries in a log (`flow.filter + flow.fold`) — Pattern 1
2. CSV → JSON Lines conversion (`flow.drop + flow.map + flow.each`) — Pattern 2
3. Parallel download of a URL list (`flow + fan.map + flow.each`) — Pattern 3 (depends on Phase 2)
4. First N lines of a huge file (`flow.take + flow.collect`)
5. Extract matching lines (grep-like, `flow.filter + flow.each`)
6. Find the first matching line (`flow.find`, verify short-circuiting)
7. Deduplicate lines (`flow.fold` with a set accumulator)
8. Stream stdin (`io.read_lines + flow.*`, future)
9. First N items of infinite generation (`flow.generate + flow.take + flow.collect`)
10. Complex pipeline (`flow.filter + flow.map + flow.filter_map + flow.fold`)

Place in `research/benchmark/msr/exercises/flow/`. Measure whether LLMs naturally write `flow.*` without confusing it with `list.*`.

**Success criterion**: models Haiku 4.5 or above pass at least 8 of 10 exercises in 1 shot.

---

<a id="references"></a>

## References

| Language | Lesson |
|---|---|
| **MoonBit `Iter[T]`** | Explicit single-pass; idiom of passing between functions (closest precedent) |
| **Kotlin Flow** | `flow { emit(...) }` builder, suspension-based backpressure |
| **Gleam `iterator`** | List / Iterator separation; caution about `fold` on infinite sources |
| **Rust `Iterator`** | Resource cleanup via Drop, short-circuit `find` |
| **C# LINQ** | Type-directed behavior with the same verbs (rejected by Almide but informative) |
| **Haskell lazy I/O** | A negative example to avoid in `file.lines` resource cleanup |

---

<a id="scope-boundary"></a>

## Scope Boundary

**In scope**:
- `Flow[T]` type + 12 `flow.*` functions
- `file.lines` returning Flow
- Compile-time rejection of forbidden operations
- Move-only restriction (enforce single-pass)
- Resource cleanup through Drop
- `fan.map` × Flow integration (Phase 2)

**Out of scope**:
- Shared `list.*` and `flow.*` namespace (rejected)
- Higher-kinded abstraction such as `Seq[T]`
- Implicit List → Flow promotion
- Implicit materialization through type annotations
- Type-level tracking of finiteness (D5)
- Algebraic effect handlers
- Stream-specific language syntax (`flow { emit }` considered in Phase 3)
