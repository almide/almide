> 描述（固定历史）：标准库重设计 L2/L3 — map：撤除原地 insert/delete 对，将 get_or 统一为 ??，并检查与 list/set 共享的高阶函数族。
<a id="l2l3-map"></a>

# L2/L3: map

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](L2-map.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](L2-map.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](L2-map.zh-CN.md)

> 非规范性历史重设计参照版。固定原本权威且不变。不自动同步、重新获取官方页面或独立执行。(F)、计时和probe为原作者报告。保留早期update无操作与后来abort、否决新增to_list与后来采纳改名、分离回调与后来元组回调、旧25函数无新增与后来新增行的差异。R-map-04仍待Dojo测量。2026-10-27实现报告保留方法形式限制，不是当前验证。引用指令为不活性资料。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-map.md)

> 最后更新：2026-10-21
> 前提：[L0](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L0-conventions.md) / [L1](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L1-categories.md)
> 来源：Rust https://doc.rust-lang.org/std/collections/struct.HashMap.html / Go https://pkg.go.dev/maps (F) / Python https://docs.python.org/3/library/stdtypes.html#mapping-types-dict / TS/JS https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/Map / Gleam https://gleam-stdlib.hexdocs.pm/gleam/dict.html (F) / Elixir https://elixir.hexdocs.pm/Map.html (F) / OCaml https://ocaml.org/manual/latest/api/Map.S.html (F) / Kotlin https://kotlinlang.org/api/core/kotlin-stdlib/kotlin.collections/-map/ (F) / Swift https://developer.apple.com/documentation/swift/dictionary
> 共同判定：[collections 共同判定 D1–D11](#共通の判定との対応)（与 list/set/option/result/string 页内容相同）。

阅读约定（同 L0）：
- `mut`（读作 mute）= 原地改写调用方 `var` 的参数。
- `??`（空值合并）= 左边是 none 时采用右边的运算符。
- `|>`（管道）= 将左边的值作为右边函数的第一个参数。
- `m[k] = v`（下标赋值）= 将变量 `m` 的键 `k` 设为 `v` 的语句。含义同 `m = map.set(m, k, v)`，借助 `@reuse` 不复制即可运行。

当前公开函数有 25 个（interface JSON，标明 `mut`）。

---

<a id="l2-役割の比較"></a>

## L2：角色比较

(F) 标记本次获取官方页面后确认的格。in-place = 原地改写，copy = 返回新值。

| 角色 | Almide | Rust | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin | Swift |
|---|---|---|---|---|---|---|---|---|---|---|
| 构造空值 | `map.new` | `HashMap::new` | `make(map[K]V)` | `{}` / `dict()` | `new Map()` | `dict.new` (F) | `Map.new` (F) | `M.empty` | `mapOf()` | `[:]` |
| 提取（不存在则 none） | `map.get` | `get` → Option | `v, ok := m[k]` | `d.get(k)` | `m.get(k)` | `dict.get` → Result (F) | `Map.fetch` (F) | `find_opt` (F) | `m[k]` → `V?` (F) | `d[k]` → Optional |
| 带默认值提取 | `map.get_or` | — (`unwrap_or`) | 零值 | `d.get(k, d0)` | `?? d` | — (`result.unwrap`) | `Map.get(m,k,d)` (F) | — | `getOrDefault` (F) | `d[k, default:]` |
| 写入（copy） | `map.set` | — | — | `d \| {k: v}` | — | `dict.insert` (F) | `Map.put` (F) | `add` (F) | `m + (k to v)` (F) | — |
| 写入（in-place） | `map.insert(mut)`・`m[k] = v` | `insert` | `m[k] = v` | `d[k] = v` | `m.set` | — | — | — | `m[k] = v` | `d[k] = v` |
| 删除（copy） | `map.remove` | — | — | — | — | `dict.delete` (F) | `Map.delete` (F) | `remove` (F) | `m - k` (F) | — |
| 删除（in-place） | `map.delete(mut)` | `remove` | `delete(m,k)` | `del d[k]` | `m.delete` | — | — | — | `remove` | `removeValue` |
| 清空（in-place） | `map.clear(mut)` | `clear` | `clear(m)` | `d.clear()` | `m.clear()` | — | — | — | `clear` | `removeAll` |
| 键是否存在 | `map.contains` | `contains_key` | `_, ok := m[k]` | `k in d` | `m.has` | `dict.has_key` (F) | `Map.has_key?` (F) | `mem` (F) | `containsKey` (F) | `d[k] != nil` |
| 数量/空判定 | `map.len`・`map.is_empty` | `len`・`is_empty` | `len` | `len` | `size` | `size`・`is_empty` (F) | `map_size` | `cardinal`・`is_empty` (F) | `size`・`isEmpty` (F) | `count`・`isEmpty` |
| 键列表/值列表 | `map.keys`・`map.values` | `keys`・`values` | `maps.Keys` (F) | `keys()` | `keys()` | `dict.keys`・`values` (F) | `Map.keys`・`values` (F) | — | `keys`・`values` (F) | `keys`・`values` |
| 键值对列表 | `map.entries` | `iter()` | `maps.All` (F) | `items()` | `entries()` | `dict.to_list` (F) | `Map.to_list` (F) | `to_list`・`bindings` (F) | `entries`・`toList` (F) | 迭代 |
| 从键值对列表构造 | `map.from_list` | `from_iter` / `collect` | `maps.Collect` (F) | `dict(pairs)` | `new Map(pairs)` | `dict.from_list` (F) | `Map.new(list)` | `of_list` (F) | `toMap` | `Dictionary(uniqueKeysWithValues:)` |
| 合并两个（右侧优先） | `map.merge` | `extend` | `maps.Copy` (F) | `a \| b` | — | `dict.merge` (F) | `Map.merge/2` (F) | — | `a + b` (F) | `merging` |
| 由函数决定合并方式 | — | — | — | — | — | `dict.combine` (F) | `Map.merge/3` (F) | `union` (F) | — | `merging(_:uniquingKeysWith:)` |
| 仅变换值 | `map.map`（`(V) -> B`） | — | — | 推导式 | — | `dict.map_values`（`fn(k, v)`）(F) | — | `map`（值）・`mapi`（F） | `mapValues` (F) | `mapValues` |
| 筛选 | `map.filter`（`(K, V)`） | `retain` | `maps.DeleteFunc` (F) | 推导式 | — | `dict.filter` (F) | `Map.filter` (F) | `filter` (F) | `filter` (F) | `filter` |
| 折叠 | `map.fold` | `iter().fold` | — | — | — | `dict.fold` (F) | `Enum.reduce` | `fold` (F) | `entries.fold` | `reduce` |
| any / all / count / find | `map.any`・`all`・`count`・`find` | `iter().any` 等 | — | `any(...)` | — | — | `Enum.*` | `exists`・`for_all` (F) | `any`・`all`・`count` (F) | `contains(where:)` 等 |
| 存在时用函数更新（否则不做事） | `map.update` | `entry().and_modify` | — | — | — | — | `Map.update!`（不存在时异常） | `update`（option→option）(F) | — | — |
| 存在则更新，否则初值 | `map.upsert` | `entry().and_modify(f).or_insert(v)` | — | `setdefault` | — | `dict.upsert`（`fn(Option(v)) -> v`）(F) | `Map.update/4`（直接插入初值）(F) | `update` (F) | `getOrPut` | `d[k, default:] += 1` |
| 取出并删除 | — | `remove` → Option | — | `d.pop(k)` | — | — | `Map.pop` → `{v, map}` (F) | — | `remove` → `V?` | `removeValue` → Optional |

<a id="完全性の規則"></a>

### 完备性规则

1. **高阶函数族（list/map/set 共通）**：`map`、`filter`、`fold`、`any`、`all`、`count`、`find` 七项必须在三个集合中全部具备（共同判定 D3）。map 已有全部七项。set 缺少的 `count`、`find` 在 [L2-set](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-set.md) 中新增。
2. **基本集合（list/map/set 共通）**：`new`、`from_list`、`len`、`is_empty`、`contains`，map 全部具备。
3. **map 回调形态**：同时观察键和值的函数（`filter`、`fold`、`any`、`all`、`count`、`find`）接受 `(K, V)`。仅变换值的 `map.map`、`map.update`、`map.upsert` 接受 `(V)`。只改写值的函数不接受键是有意省略（键不变）。
4. **改写方式**：map 公开函数除 `clear` 外全为返回副本的版本（R-L0-04）。单键原地写入由语言的下标赋值 `m[k] = v` 承担，而非函数。
5. **有意省略**：不提供取出并删除（`pop`）、由函数决定合并方式的 `merge`、键变换 `map_keys`（R-map-05）。
6. **越界写入**：L0 的“按位置写入越界 abort”针对按索引/位置写入的函数。map 按键写入，不存在位置越界。`map.update` 键不存在时不做事，是与 `map.get` 返回 none 相同的普通“键不存在”，不是违约（同 Rust `entry().and_modify`、OCaml `update`）。维持现状。

<a id="l2-の判定"></a>

### L2 判定

- **方式冲突（R-L0-04）**：仅 `map.insert(mut)`、`map.delete(mut)` 使用副本侧动词 `insert`/`delete` 做原地改写。`set.insert`、`list.insert` 返回副本 → R-map-01。
- **默认值版重复（R-L0-06）**：`map.get_or` 与 `map.get(m, k) ?? d` 相同 → R-map-02。
- **缺失角色（至少 5/9）**：
  - 以 `to_list` 获取键值对列表（Gleam/Elixir/OCaml/Kotlin，4/9）→ 非多数，且 `map.entries` 已承担角色 → R-map-03（否决）。
  - 取出并删除（Rust/Python/Elixir/Kotlin/Swift，5/9）→ 可用两步（`get` 与 `remove`），`pop` 已保留给“从末尾取出”的原地动词 → R-map-05（否决）。
- **静默丢弃的调用（R-L0-05）**：实测中，作为语句的 `map.set(m, "z", 9)` 与 `map.remove(m, "b")` 在 native/wasm 均不报错且无变化（见下）。R-map-01 撤除 `map.insert` 后，沿用其他语言习惯而将 `map.set(m, k, v)` 写为语句的错误会增加，故 R-L0-05 先落地是前提。
- **族的缺口**：map 没有。R-map-06 设置全族 gate。

实测（2026-10-21，`almide 0.68.0 (dev, 437b8f127)`，native 与 `--target wasm` 输出相同；`scratchpad/sr/probe-map/a.almd`）：

```almide
var m: Map[String, Int] = map.new()
m["a"] = 1
m["b"] = 2
m["a"] = 10                 // 上書き。位置は変わらない → [("a", 10), ("b", 2)]
m = map.remove(m, "a")      // → [("b", 2)]
map.set(m, "z", 9)          // エラーにならず、何も起きない
map.remove(m, "b")          // エラーにならず、何も起きない → len は 1 のまま
map.update(m, "nope", f)    // キーが無ければ何もしない
map.upsert(m, "nope", 0, f) // 無ければ 0 がそのまま入る（f は呼ばれない）。あれば f(old)
map.merge(a, b)             // 同じキーは b が勝つ。順序は a、その後 b の新しいキー
```

代码注释说明：覆盖不改变位置；两个被丢弃的调用既不报错也无变化（长度仍为 1）；update 在键不存在时不做事；upsert 在键不存在时直接插入初值、不调用回调，存在时对旧值应用回调；merge 同键取右值，顺序为左侧之后追加右侧新键。

`map.to_list` 产生 E002，提示只有“参见 CHEATSHEET”，不指出 `map.entries`。

---

<a id="案"></a>

## 提案

<a id="r-map-01-mapinsert-と-mapdeleteその場書き換えを撤去する---採用"></a>

### R-map-01 撤除原地改写的 `map.insert` 与 `map.delete` — ○ 采纳

- 内容：
  - 对 `map.insert(mut m, k, v) -> Unit`、`map.delete(mut m, k) -> Unit` 添加 `@deprecated(since=15, use=…)`，epoch 16 撤除。
  - 替换：`map.insert(m, k, v)` → `m[k] = v`（`m` 是变量时）或 `m = map.set(m, k, v)`；`map.delete(m, k)` → `m = map.remove(m, k)`。
  - 保留 `map.clear(mut m)`（`clear` 属于 R-L0-04 动词集）。
  - 从 CHEATSHEET 第 193 行“in-place stdlib op”枚举及 `llms.txt` 两处（包括 E032 行）删除 `map.insert`。
- L0 依据：R-L0-04（`insert`/`delete` 是副本侧动词，L0 本文明确定名）、R-L0-01、R-L0-05、R-L0-14。
- (a) 九种语言中，不以名称区分方式的语言（Rust/JS/Python）将 `insert`/`set`/`delete` 用于原地改写；无改写的语言（Gleam/Elixir/OCaml Map）用相同动词返回副本。Almide 默认副本，故对齐后者。单键原地写入可像 Go/Python/Kotlin/Swift 一样使用下标赋值 `m[k] = v`。
- (b) native/wasm：无影响。下标赋值与 `map.set` + `@reuse` 已在两条路径运行（实测输出相同）。L0 测量中，200 万项时 `map.set`、`map.insert`、下标赋值均约 0.07 秒。
- (c) 当前 `set.insert` 返回副本、`map.insert` 原地改写，无法由动词判断方式。撤除后“`insert` 总是副本”。但若按 Rust 习惯写 `map.insert(m, k, v)`，撤除提示让人将 `map.set` 写为语句就会静默丢弃，故提示首选下标赋值。
- (d) 撤除。兼容：`@deprecated(since=15, use="m[k] = v")`、`@deprecated(since=15, use="map.remove")`；在 `proofs/dialect-epochs.toml` 放置 `[[deprecation]]`（`removal_epoch = 16`）。`almide fix`：第一参数为变量路径（`m`、`r.m`）时改为 `m[k] = v`；`map.delete(m, k)` 改为 `m = map.remove(m, k)`。撤除后 `removed_spellings.rs` 提示相同形式。
  - 使用数（`\bmap\.insert\b` / `\bmap\.delete\b` 出现次数，含定义/生成文档的上限）：insert 为 spec 96 / tests 27 / docs 19 / stdlib 1（仅注释）；delete 为 spec 16 / tests 19 / docs 10 / stdlib 0。
  - 顺序：要求 R-L0-05（丢弃语句值时报错）在同一 epoch 15 先落地，否则迁移后的 `map.set(m, k, v);` 会静默丢弃。
- gate：在 R-L0-G 的 `tests/stdlib_surface_conventions_test.rs` 检查 map 中带 `mut` 参数的公开函数只有 `clear`。在 `spec/stdlib/` 放入下标赋值与 `map.remove` 重新赋值的 native⇄wasm fixture。
- Issue：#3928

<a id="r-map-02-mapget_or-を撤去して--に一本化する---採用"></a>

### R-map-02 撤除 `map.get_or`，统一为 `??` — ○ 采纳

- 内容：为 `map.get_or(m, k, d)` 添加 `@deprecated(since=15, use="??")`，epoch 16 撤除。替换为 `map.get(m, k) ?? d`。
- L0 依据：R-L0-06（2）、R-L0-01、R-L0-14。
- (a) 默认值版有 Python `get(k, d)`、Elixir `Map.get(m, k, d)`、Kotlin `getOrDefault`、Swift `d[k, default:]`。Rust/JS/Gleam 采用运算符或通用函数。没有多数，Almide 采用 CHEATSHEET 所教的 `??`。
- (b) 无影响。ADR-0005 将 `??` 脱糖为 `option.unwrap_or_else`，两目标相同。
- (c) `get_or` 总会求值默认值，而 `??` 只在 none 时求值右侧。默认值计算昂贵时 `??` 更安全。
- (d) 撤除。兼容：`@deprecated(since=15, use="??")`、`removal_epoch = 16`，与 `#1107` 的 `option.unwrap_or` / `result.unwrap_or` 同窗口。`almide fix`：`map.get_or(m, k, d)` → `map.get(m, k) ?? d`（管道 `|>` 形式 `m |> map.get_or(k, d)` → `m |> map.get(k) ?? d`）。使用数：spec 161 / tests 29 / docs 21 / stdlib 0。用 `almide fix` 批量改写 161 项 spec，重新生成 run/check/ast 账本。
- gate：R-L0-G 增加“公开函数名不以 `_or` 结尾”检查（允许列表只能缩减）。
- Issue：#3913

<a id="r-map-03-mapto_list-を足す---不採用-r-map-07-で採用に変更"></a>

### R-map-03 新增 `map.to_list` — × 否决（→ R-map-07 改为采纳）

- 内容：不新增返回键值对列表的 `map.to_list`。保留 `map.entries` 为唯一拼写。改为修正 E002 在写 `map.to_list` 时的提示为“使用 `map.entries`”（当前只有“参见 CHEATSHEET”，已实测）。
- L0 依据：R-L0-00（无多数则维持）、R-L0-01（同一角色不设两种拼写）。
- (a) `to_list` 为 Gleam/Elixir/OCaml/Kotlin 的 4/9（OCaml 另有 `bindings`，Kotlin 另有 `entries`）。JS 是 `entries()`、Python `items()`、Rust `iter()`，不到 5/9。
- (b) 无影响。
- (c) 因有 `set.to_list`、`map.from_list`，LLM 会推测写 `map.to_list`。新增会成为同义词，故用提示承接。
- (d) 无变更（仅诊断提示文案）。
- 机制：在 E002“did you mean”表 `ALIASES`（`crates/almide-frontend/src/stdlib.rs:156-230`，仅名称未定义时查询）增加：`("map", "to_list", "map.entries")`、`("map", "put", "map.set")`、`("map", "items", "map.entries")`。R-map-01 撤除后再加 `("map", "insert", "m[k] = v")`、`("map", "delete", "map.remove")`（已撤拼写提示由 `removed_spellings.rs` 优先，故只放一处）。
- 同时修正表中旧行：
  - `("map", "count", "map.len")`（第 164 行）：`map.count` 已公开为谓词计数函数，不会查询该行；但表中保留了错误的 `count` = `len` 对应，应删除。
  - `("map", "new", "[:] (empty map literal)")`（第 223 行）：`map.new` 已公开，这也是不会查询的死行，删除。
- gate：在 `tests/diagnostics/` 添加 `map.to_list` broken/fixed，确认提示指向 `map.entries`。测试 `ALIASES` 每行左侧名称不是公开函数，防止死行残留。
- Issue：#3905（诊断改进）

<a id="r-map-04-mapmap-を-map_values-に改名するまたはコールバックを-k-v-にする--保留六巡目に--から変更dojo-で測る"></a>

### R-map-04 将 `map.map` 改名 `map_values`，或回调改为 `(K, V)` — 暂缓（第六轮由 × 改为此；用 Dojo 测量）

- 内容：保留 `map.map(m, f: (V) -> B)` 的名称与回调形态。
- L0 依据：R-L0-00、R-L0-14（4：禁止改换含义）。
- (a) 仅变换值的拼写为 `map_values`（Gleam）/`mapValues`（Kotlin/Swift），共 3/9；`map`（OCaml）为 1/9，无多数。也接受键的回调有 Gleam `map_values`、Kotlin `mapValues`、OCaml `mapi`。
- (c) `map.filter` 等为 `(K, V)`，但 `map.map` 为 `(V)`，可能误写 `(k, v) =>`；不过会产生参数数量不符的类型错误，不会静默通过。同名改形后，旧形仍会留在训练数据中（R-L0-14 第 4 项）。
- (d) 无变更。
- gate：无（类型检查阻止）。
- Issue：无。
- 暂缓理由（第六轮）：此前理由是“错了也会类型报错”，但按 R-L0-36，这本身不足以否决。有三个候选：当前 `map.map` 的 `(V)`、`map.map` 改为 `(K, V)`、改名 `map_values`。其他语言也分歧（Swift/Kotlin `mapValues`、OCaml 值版 `map` 与键值版 `mapi`、Gleam 键值版 `map_values`）。在 Dojo 用相同任务/模型测量各候选生存率后决定（almide-dojo#64）。

<a id="r-map-05-取り出して消す-pop合わせ方つきの-merge_withキーを変換する-map_keys-を足す---不採用"></a>

### R-map-05 新增取出并删除的 `pop`、自定义合并 `merge_with`、键变换 `map_keys` — × 否决

- 内容：三个都不新增。
- L0 依据：R-L0-04（`pop` 为原地从末尾取出）、L1 新增门槛（至少 5/9）、R-L0-00。
- (a)
  - 取出并删除：Rust/Python/Elixir/Kotlin/Swift 的 5/9 具备，但四种只作为原地改写的返回值，副本版只有 Elixir `Map.pop`。Almide 可用 `map.get(m, k)` 与 `m = map.remove(m, k)` 两步。将副本版命名 `pop` 会分裂 R-L0-04 已定的 `pop`“原地从末尾取出”含义。
  - 用函数决定合并方式：Gleam `combine`、Elixir `merge/3`、OCaml `union`、Swift `merging` 共 4/9，名称也不同。可写 `map.fold(b, a, (acc, (k, v)) => map.upsert(acc, k, v, (old) => g(old, v)))`。
  - 键变换：只有 Kotlin `mapKeys`。
- (b)(d) 无变更。
- Issue：无。

<a id="r-map-06-高階関数の-family-を-listmapset-の行列でゲートする---採用"></a>

### R-map-06 以 list/map/set 矩阵检查高阶函数族 — ○ 采纳

- 内容：从 interface JSON 检查 `map`、`filter`、`fold`、`any`、`all`、`count`、`find` 七项存在于 list/map/set，map 谓词回调为 `(K, V)`，list/set 为 `(A)`。map 不变。set 新增 `count`、`find` 见 [L2-set R-set-01](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-set.md)。
- L0 依据：R-L0-01、R-L0-10、CLAUDE.md“API 族按矩阵展开”。
- (a) 九种语言中，不同集合的高阶函数不齐全（如 Gleam set 无 `count`、`find`）会成为编写者尝试却失败之处。
- (b) 无影响（仅检查）。
- (c) 一个集合中学到的形式，在其他集合也直接有效。
- (d) 无新增（仅 gate）。
- gate：R-L0-G 测试增加“集合族矩阵”章节。缺格即失败，有意省略在允许列表中附理由（只能缩减）。
- Issue：#3901

---

<a id="r-map-07-mapentries-を-mapto_list-に改名するr-map-03-の見直し---採用"></a>

### R-map-07 将 `map.entries` 改名为 `map.to_list`（重审 R-map-03）— ○ 采纳

- 现状：`fn map.entries(m: Map[K, V]) -> List[(K, V)]` 与 `fn map.from_list(pairs: List[(K, V)]) -> Map[K, V]`。R-map-03 否决了新增 `to_list`（非多数/成为同义词）。
- 提案：不是新增而是改名。`map.entries` → `fn map.to_list(m: Map[K, V]) -> List[(K, V)]`。`keys`、`values` 不变。
- 新证据：重设计后，容器转 List 的拼写分为六个 `to_list` 模块（`set`、`deque`、`heap`、`bytes`、`option`、`result`）与只有 map 的 `entries`。有 `from_list` 却无 `to_list` 的容器只有 map。R-map-03 只看九种语言多数（R-L0-00 第 2 项），未看 Almide 内部冲突（第 1 项）。
- L0 依据：R-L0-00 第 1 项、R-L0-07 第 2 项（构造 `from_list` 及其逆向）。
- (a) `to_list` 是 Gleam/Elixir/OCaml/Kotlin 的 4/9，`entries` 为 JS/Kotlin 的 2/9。
- (b) 无影响（同一函数体）。
- (c) R-map-03 自认 LLM 会推测 `map.to_list`。名称与推测对齐后，`map.from_list(map.to_list(m)) == m` 也能从名称读懂。
- (d) 改名。`@deprecated(since=15, use="map.to_list")`，epoch 16 撤除，`almide fix` 一对一改写。使用数：spec 22 / tests 2 / docs 22 / stdlib 7 / research 2（合计 55）。
- 完备性：具有 `from_list` 的容器也具有 `to_list`，且 `from_list(to_list(x)) == x`。
- gate：R-L0-07 增加 `from_list`/`to_list` 配对检查，除 list 外所有容器设置往返性质测试。
- Issue：#3969

<a id="r-map-08-mapupdate-はキーが無ければ-abort---採用"></a>

### R-map-08 `map.update` 键不存在时 abort — ○ 采纳

- 现状：键不存在时不做事、返回同一 map（实测 `almide 0.68.0 dev 437b8f127`）。`list.update` 索引不存在时 abort，同属“改写已有元素”却无法预测不存在时行为。
- 提案：键不存在即 abort。不存在则创建的角色用 `upsert`；不确定存在时以 `map.contains` 检查或使用 `upsert`。
- L0 依据：R-L0-33（改写已有元素的角色 abort）、R-L0-06 第 3 项。
- (a) Elixir `Map.update!` 不存在时异常，Kotlin `getValue` 也异常。Rust `entry().and_modify` 不做事，但名称含 and，从写法可看见不存在时的分支。
- (d) 语义变更。当前无操作返回的调用会 abort，计入 epoch 15 `breaks`。
- Issue：#3990

<a id="r-map-09-map-を-k-v-の組の-container-にする---採用"></a>

### R-map-09 将 `map` 作为 `(K, V)` 对的 `Container` — ○ 采纳

- 现状：`map.filter`、`any`、`all`、`count`、`find` 回调有两个参数 `(K, V) -> Bool`，`fold` 三个参数 `(A, K, V) -> A`。其他容器回调接受一个元素，因此 `map` 不能进入 protocol 层级的 `Container`/`Sequence`。
- 提案：回调接受一个 `(K, V)` 对元素（`filter(m, f: ((K, V)) -> Bool)`、`fold(m, init: A, f: (A, (K, V)) -> A)`）。`map.to_list`/`from_list` 已是键值对序列。语言新增规则：期待接受一个元组的函数处，可写参数数目等于元组元素数的匿名函数，因此 `map.filter(m, (k, v) => v > 0)` 仍有效（同 Swift `dict.filter { key, value in … }`）。
- (a) Swift `Dictionary` 是 `(key, value)` 对的 `Collection`；Rust `HashMap` 迭代、Kotlin `Map.Entry`、Python `items()` 也逐个传递键值对。
- (d) 类型变更。两参数匿名函数仍直接有效；`fold` 三参数匿名函数由 `almide fix` 改为 `(acc, (k, v)) =>`。
- 实现状态（2026-10-27）：已实现。六个回调接受一个元组（`fold` 接受累积值和元组）。检查器将只期待一个元组处、参数数目等于元组元素数的匿名函数读为 `((k, v)) =>`（只依据期待类型，不看函数名）。`fold` 的 `(acc, k, v) =>` 产生 E004，提示 `(acc, (k, v)) =>`（epoch 15 `breaks`）。方法调用形式（`m.filter((k, v) => …)`）尚不支持此解释。
- Issue：#4004

<a id="l3-関数表"></a>

## L3：函数表

使用数只写在非保留判定行（参见 R-map-01/R-map-02）。

| 函数 | 签名（当前，显示 mut） | 其他语言主要拼写 | 判定 | 提案 |
|---|---|---|---|---|
| `map.new` | `map.new() -> Map[K, V]` | Rust `HashMap::new` / Gleam `dict.new` / Elixir `Map.new` / OCaml `empty` | 保留 | — |
| `map.get` | `map.get(m: Map[K, V], key: K) -> Option[V]` | Rust `get` / Python `get` / JS `get` / Gleam `get`(Result) / OCaml `find_opt` / Kotlin `m[k]` | 保留 | — |
| `map.get_or` | `map.get_or(m: Map[K, V], key: K, default: V) -> V` | Python `get(k, d)` / Elixir `get/3` / Kotlin `getOrDefault` / Rust・JS・Gleam — (运算符) | 撤除 | R-map-02 |
| `map.set` | `map.set(m: Map[K, V], key: K, value: V) -> Map[K, V]` | Gleam `insert`(copy) / Elixir `put` / OCaml `add` / Kotlin `m + (k to v)` | 保留 | R-map-01（替代目标） |
| `map.contains` | `map.contains(m: Map[K, V], key: K) -> Bool` | Rust `contains_key` / JS `has` / Gleam `has_key` / OCaml `mem` / Kotlin `containsKey` | 保留 | — |
| `map.remove` | `map.remove(m: Map[K, V], key: K) -> Map[K, V]` | Rust `remove` / Gleam `delete` / Elixir `delete` / OCaml `remove` / Kotlin `m - k` | 保留 | R-map-01（替代目标） |
| `map.keys` | `map.keys(m: Map[K, V]) -> List[K]` | 所有语言 `keys`（Go `maps.Keys`） | 保留 | — |
| `map.values` | `map.values(m: Map[K, V]) -> List[V]` | 所有语言 `values` | 保留 | — |
| `map.len` | `map.len(m: Map[K, V]) -> Int` | Rust `len` / Gleam `size` / JS `size` / OCaml `cardinal` / Swift `count` | 保留 | — (R-L0-11) |
| `map.entries` | `map.entries(m: Map[K, V]) -> List[(K, V)]` | JS `entries` / Kotlin `entries` / Gleam・Elixir・OCaml `to_list` / Python `items` | 改名 → `to_list` | R-map-07 |
| `map.merge` | `map.merge(a: Map[K, V], b: Map[K, V]) -> Map[K, V]` | Gleam `merge` / Elixir `merge/2` / Python `a \| b` / Kotlin `a + b`（均右侧优先） | 保留 | — |
| `map.is_empty` | `map.is_empty(m: Map[K, V]) -> Bool` | Rust `is_empty` / Gleam `is_empty` / OCaml `is_empty` / Kotlin `isEmpty` | 保留 | — |
| `map.from_list` | `map.from_list(pairs: List[(K, V)]) -> Map[K, V]` | Gleam `from_list` / OCaml `of_list` / Rust `from_iter` / Go `maps.Collect` | 保留 | — (R-L0-07) |
| `map.map` | `map.map(m: Map[K, V], f: (V) -> B) -> Map[K, B]` | Gleam `map_values` / Kotlin・Swift `mapValues` / OCaml `map` | 保留 | R-map-04 |
| `map.filter` | `map.filter(m: Map[K, V], f: ((K, V)) -> Bool) -> Map[K, V]` | Gleam `filter` / Elixir `filter` / OCaml `filter` / Kotlin `filter` / Rust `retain` | 语义变更（回调接受一个元组；`(k, v) =>` 仍直接有效） | R-map-09 |
| `map.fold` | `map.fold(m: Map[K, V], init: A, f: (A, (K, V)) -> A) -> A` | Gleam `fold` / OCaml `fold` / Elixir `Enum.reduce` | 语义变更（回调第二参数为元组；`almide fix` 负责改写） | R-map-09 |
| `map.any` | `map.any(m: Map[K, V], f: ((K, V)) -> Bool) -> Bool` | Kotlin `any` / OCaml `exists` / Rust `iter().any` | 语义变更（回调接受一个元组；`(k, v) =>` 仍直接有效） | R-map-09 |
| `map.all` | `map.all(m: Map[K, V], f: ((K, V)) -> Bool) -> Bool` | Kotlin `all` / OCaml `for_all` / Rust `iter().all` | 语义变更（回调接受一个元组；`(k, v) =>` 仍直接有效） | R-map-09 |
| `map.count` | `map.count(m: Map[K, V], f: ((K, V)) -> Bool) -> Int` | Kotlin `count` / Elixir `Enum.count/2` | 语义变更（回调接受一个元组；`(k, v) =>` 仍直接有效） | R-map-09 |
| `map.find` | `map.find(m: Map[K, V], f: ((K, V)) -> Bool) -> Option[(K, V)]` | Rust `iter().find` / Elixir `Enum.find` / Swift `first(where:)` | 语义变更（回调接受一个元组；`(k, v) =>` 仍直接有效） | R-map-09 |
| `map.update` | `map.update(m: Map[K, V], key: K, f: (V) -> V) -> Map[K, V]` | Rust `entry().and_modify` / OCaml `update` / Elixir `update!`（不存在时异常） | 语义变更（键不存在 abort） | R-map-08 |
| `map.upsert` | `map.upsert(m: Map[K, V], key: K, init: V, f: (V) -> V) -> Map[K, V]` | Elixir `update/4`（相同含义）/ Rust `entry().and_modify(f).or_insert(v)` / Gleam `upsert`（`fn(Option(v))`） | 保留 | — |
| `map.insert` | `map.insert(mut m: Map[K, V], key: K, value: V) -> Unit` | Rust `insert` / JS `set` / Python `d[k] = v`（均为 in-place） | 撤除 | R-map-01 |
| `map.delete` | `map.delete(mut m: Map[K, V], key: K) -> Unit` | Rust `remove` / JS `delete` / Go `delete`（in-place） | 撤除 | R-map-01 |
| `map.clear` | `map.clear(mut m: Map[K, V]) -> Unit` | Rust `clear` / Python `clear` / JS `clear` / Kotlin `clear` | 保留 | — (R-L0-04 的动词) |
| `map.enumerate` | （新增）`map.enumerate(m: Map[K, V]) -> List[(Int, (K, V))]` | Swift `enumerated` / Rust `enumerate` | 新增 | R-L0-42 |
| `map.zip` | （新增）`map.zip(m: Map[K, V], ys: List[B]) -> List[((K, V), B)]` | Swift `zip` / Rust `zip` | 新增 | R-L0-42 |
| `map.zip_with` | （新增）`map.zip_with(m: Map[K, V], ys: List[B], f: ((K, V), B) -> C) -> List[C]` | Kotlin `zip(other, f)` | 新增 | R-L0-42 |
| `map.filter_map` | （新增）`map.filter_map(m: Map[K, V], f: ((K, V)) -> B?) -> List[B]` | Swift `compactMap` / Rust `filter_map` | 新增 | R-L0-42 |
| `map.find_map` | （新增）`map.find_map(m: Map[K, V], f: ((K, V)) -> B?) -> B?` | Rust `find_map` | 新增 | R-L0-42 |
| `map.reduce` | （新增）`map.reduce(m: Map[K, V], f: ((K, V), (K, V)) -> (K, V)) -> (K, V)?` | Swift `reduce` / Rust `reduce` | 新增 | R-L0-42 |
| `map.scan` | （新增）`map.scan(m: Map[K, V], init: B, f: (B, (K, V)) -> B) -> List[B]` | Rust `scan` / Kotlin `runningFold` | 新增 | R-L0-42 |
| `map.partition` | （新增）`map.partition(m: Map[K, V], f: ((K, V)) -> Bool) -> (List[(K, V)], List[(K, V)])` | Kotlin `partition` | 新增 | R-L0-42 |
| `map.group_by` | （新增）`map.group_by(m: Map[K, V], f: ((K, V)) -> B) -> Map[B, List[(K, V)]]` | Kotlin `groupBy` / Swift `Dictionary(grouping:)` | 新增 | R-L0-42 |
| `map.sort_by` | （新增）`map.sort_by(m: Map[K, V], f: ((K, V)) -> B) -> List[(K, V)]` | Kotlin `sortedBy` | 新增 | R-L0-42 |
| `map.sort_with` | （新增）`map.sort_with(m: Map[K, V], cmp: ((K, V), (K, V)) -> Int) -> List[(K, V)]` | Swift `sorted(by:)` | 新增 | R-L0-42 |
| `map.min_by` | （新增）`map.min_by(m: Map[K, V], f: ((K, V)) -> U) -> (K, V)?` | Kotlin `minByOrNull` | 新增 | R-L0-42 |
| `map.max_by` | （新增）`map.max_by(m: Map[K, V], f: ((K, V)) -> U) -> (K, V)?` | Kotlin `maxByOrNull` | 新增 | R-L0-42 |
| `map.min_with` | （新增）`map.min_with(m: Map[K, V], cmp: ((K, V), (K, V)) -> Int) -> (K, V)?` | Swift `min(by:)` | 新增 | R-L0-42 |
| `map.max_with` | （新增）`map.max_with(m: Map[K, V], cmp: ((K, V), (K, V)) -> Int) -> (K, V)?` | Swift `max(by:)` | 新增 | R-L0-42 |

L3 行数：25（无新增）。判定分布：保留 22，撤除 3（`get_or`、`insert`、`delete`）。

---

<a id="共通の判定との対応"></a>

## 与共同判定的对应

为对齐 list/set/option/result/string 页，共同判定中与本页有关的项目：

| 共同判定 | 本页处理 |
|---|---|
| D1 `_or` 的撤除 | R-map-02 |
| D2 `map.insert` / `map.delete` 的撤除、`map.clear` 保留 | R-map-01 |
| D3 高阶族与 `map.to_list` × | R-map-06、R-map-03 |
| D8 兼容窗口（since=15、removal 16） | R-map-01、R-map-02 |
| D9 R-L0-05 阻止的调用 | `map.set`・`map.remove`・`map.update`・`map.upsert`・`map.merge`・`map.filter` 作为语句时（实测当前静默丢弃） |

<a id="l0-への差し戻し"></a>

## 向 L0 回报

无。
