<a id="l0-9-言語の標準ライブラリ命名規約事実のみ"></a>

# L0：九种语言的标准库命名规范（仅事实）

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](naming-conventions.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](naming-conventions.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](naming-conventions.zh-CN.md)

> 非规范性固定原文历史调查读者版。固定正本不变，不自动同步、未运行API或独立重新查阅网页。10月21日与F标记指原作者的核验，不是本次翻译新执行的核验；无标记项的证据状态不变。API、版本、提案阶段、in-place/copy及失败行为均保留固定原文当时报告，不是当前建议。内部计数及更正并列保留（size 5/9后严格4、get 6后5、set add 3后4、to_string 5后6）；结尾无多数派清单仍含sort 6对3。JS trim别名行与弃用总结也并存，未擅自统一。引文是原文引文或其译文，不是重新认证的引用。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/facts/naming-conventions.md)

调查日期：2026-10-21。不包含设计判断。
图例：**(F)** = 本次通过 WebFetch 官方文档确认的单元格。未标记项为已知事实（稳定的官方 API 名称）。
in-place = 修改调用者的值 / copy = 返回新值 / “—” = 标准库中没有。

<a id="公式ドキュメント-url"></a>

## 官方文档 URL

| 语言 | URL |
|---|---|
| Rust | https://doc.rust-lang.org/std/ ・ API Guidelines https://rust-lang.github.io/api-guidelines/naming.html (F) ・ str https://doc.rust-lang.org/std/primitive.str.html (F) |
| Go | https://pkg.go.dev/std ・ slices https://pkg.go.dev/slices (F) ・ strings https://pkg.go.dev/strings ・ strconv https://pkg.go.dev/strconv ・ os https://pkg.go.dev/os |
| Python | https://docs.python.org/3/library/stdtypes.html (F) ・ https://docs.python.org/3/library/functions.html |
| TS/JS | https://tc39.es/ecma262/ ・ MDN https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects（Set 为 F） |
| Gleam | https://gleam-stdlib.hexdocs.pm/gleam/list.html (F) ・ dict / string / set / int / float 同目录 (F) |
| Elixir | https://elixir.hexdocs.pm/naming-conventions.html (F) ・ https://hexdocs.pm/elixir/Enum.html |
| OCaml | Stdlib https://ocaml.org/manual/latest/api/（List、String 为 F；5.5 版）・ Base https://ocaml.org/p/base/latest/doc/Base/List/index.html (F) |
| Kotlin | https://kotlinlang.org/api/core/kotlin-stdlib/（collections 为 F）・ 规范 https://kotlinlang.org/docs/coding-conventions.html (F) |
| Swift | https://developer.apple.com/documentation/swift ・ API Design Guidelines https://www.swift.org/documentation/api-design-guidelines/ (F) |

---

<a id="1-大きさsize--length--count"></a>

## 1. 大小（size / length / count）

| 语言 | list | string | map | set |
|---|---|---|---|---|
| Rust | `v.len()` → usize | `s.len()` = **字节数** (F)；字符数用 `s.chars().count()` | `m.len()` | `s.len()` |
| Go | `len(xs)` 内建 | `len(s)` = 字节；`utf8.RuneCountInString(s)` = rune 数 | `len(m)` | （无 set 类型，用 map 替代） |
| Python | `len(xs)` 内建 | `len(s)` = 码点数 | `len(d)` | `len(s)` |
| TS/JS | `a.length`（属性） | `s.length` = UTF-16 码元数 | `m.size` | `s.size` |
| Gleam | `list.length(xs)` → Int (F) | `string.length` = **字素（grapheme）数** (F)；`string.byte_size` = 字节 (F) | `dict.size` (F) | `set.size` (F) |
| Elixir | `length(xs)` (Kernel, O(n)) ・ `Enum.count` | `String.length` = 字素数；`byte_size` = 字节 | `map_size(m)` ・ `Enum.count` | `MapSet.size` |
| OCaml | `List.length` (F) | `String.length` = 字节 | `Hashtbl.length` ・ `Map.cardinal` | `Set.cardinal` |
| Kotlin | `xs.size`（属性）・ `count()` | `s.length` = UTF-16 码元 | `m.size` | `s.size` |
| Swift | `xs.count`（属性） | `s.count` = **Character（字素）数**；`s.utf8.count` = 字节 | `d.count` | `s.count` |

Elixir 规范 (F) 官方区分命名：“`size` 是常数时间，`length` 是线性时间”。
- 主流写法（list）：**len 系 3（Rust/Go/Python）、length 系 4（JS/Gleam/Elixir/OCaml）、size 1（Kotlin）、count 1（Swift）** → `length` 以 4/9 为相对多数。map/set 中 `size` 为 5/9（JS/Gleam/Kotlin、Elixir MapSet.size/map_size 等）— 严格说 size 4（JS、Gleam、Kotlin、Elixir）+ len 3 + count 1 + cardinal/length 1。
- string 单位：字节 4（Rust、Go、OCaml，另 Gleam 有单独函数）、码点 1（Python）、UTF-16 2（JS、Kotlin）、字素 3（Gleam、Elixir、Swift）。**无多数派**。

<a id="2-空判定"></a>

## 2. 判空

| 语言 | 写法 |
|---|---|
| Rust | `is_empty()` |
| Go | 无专用函数。`len(x) == 0` |
| Python | 无专用函数。`not xs`（truthiness） |
| TS/JS | 无专用函数。`a.length === 0` / `m.size === 0` |
| Gleam | `list.is_empty` / `string.is_empty` / `dict.is_empty` / `set.is_empty` (F) |
| Elixir | `Enum.empty?/1`；字符串用 `s == ""` |
| OCaml | `List.is_empty`（5.1 起，F）・ `String.is_empty`（5.5 起，F）・ `Map.is_empty`；Base `List.is_empty` |
| Kotlin | `isEmpty()` / `isNotEmpty()` / `isNullOrEmpty()` |
| Swift | `isEmpty`（属性） |

- 多数派：**is_empty / isEmpty 系 5/9**（Rust、Gleam、OCaml、Kotlin、Swift）。三种语言无专用函数。

<a id="3-所属判定"></a>

## 3. 成员判定

| 语言 | list 包含 | 字符串包含子串 | map 含键 | set 含元素 |
|---|---|---|---|---|
| Rust | `v.contains(&x)` | `s.contains(pat)` (F) | `m.contains_key(&k)` | `s.contains(&x)` |
| Go | `slices.Contains(xs, x)` (F) | `strings.Contains(s, sub)` | `_, ok := m[k]` | （map 中 `_, ok := m[k]`） |
| Python | `x in xs` | `sub in s` | `k in d` | `x in s` |
| TS/JS | `a.includes(x)` | `s.includes(sub)` | `m.has(k)` | `s.has(x)` (F) |
| Gleam | `list.contains(xs, x)` (F) | `string.contains(s, sub)` (F) | `dict.has_key(d, k)` (F) | `set.contains(s, x)` (F) |
| Elixir | `Enum.member?(xs, x)` / `x in xs` | `String.contains?(s, sub)` | `Map.has_key?(m, k)` | `MapSet.member?(s, x)` |
| OCaml | `List.mem x xs` | Stdlib 5.5 起 `String.includes ~affix s` (F)，此前没有。`String.contains s c` 针对 **char** (F)。Base：`String.is_substring s ~substring` | `Hashtbl.mem` / `Map.mem` | `Set.mem` |
| Kotlin | `xs.contains(x)` / `x in xs` | `s.contains(sub)` | `m.containsKey(k)` / `k in m` | `s.contains(x)` |
| Swift | `xs.contains(x)` | `s.contains(sub)` | `d[k] != nil` / `d.keys.contains(k)` | `s.contains(x)` |

- 多数派（list）：**contains 5/9**（Rust、Go、Gleam、Kotlin、Swift）。
- map 键写法：**has_key/has/containsKey/contains_key 系**，但具体拼写分散。`contains_key`/`containsKey` 2，`has_key`/`has_key?` 2，`has` 1，`mem` 1，运算符/下标 3。**无多数派**。

<a id="4-末尾に-1-要素追加"></a>

## 4. 末尾增加一个元素

| 语言 | in-place | copy |
|---|---|---|
| Rust | `v.push(x)` (&mut self) | —（无标准函数，如 `[v, vec![x]].concat()`） |
| Go | `xs = append(xs, x)`（可能共享底层数组，将返回值重新赋值） | `slices.Concat` (F) |
| Python | `xs.append(x)` → None | `xs + [x]` |
| TS/JS | `a.push(x)` → 新 length | `[...a, x]` / `a.concat([x])` |
| Gleam | — | `list.append(xs, ys)`（列表拼接）；头部用 `list.prepend(xs, x)` (F) / `[x, ..xs]` |
| Elixir | — | `xs ++ [x]`；头部 `[x \| xs]` |
| OCaml | —（List）；`Dynarray.add_last`（5.2 起） | `xs @ [x]`；头部 `x :: xs` |
| Kotlin | `MutableList.add(x)` / `xs += x` | `xs + x` (`plus`) |
| Swift | `xs.append(x)` (mutating) | `xs + [x]` |

- in-place 写法：`push` 2（Rust、JS），`append` 3（Go、Python、Swift），`add` 1（Kotlin）。**无多数派**（append 为相对多数）。
- copy 写法：`+` / `++` / `@` 运算符占多数（Python、JS-spread、Elixir、OCaml、Kotlin、Swift = 6/9）。Gleam 的 `list.append` 是“两列表拼接”，不是增加元素。

<a id="5-pop--insert-at--remove-at"></a>

## 5. pop / insert at / remove at

| 语言 | 删除末尾 | 插入位置 | 删除位置 |
|---|---|---|---|
| Rust | `v.pop()` → Option<T> | `v.insert(i, x)`（越界 panic） | `v.remove(i)` → T (panic) ・ `swap_remove` |
| Go | `xs = xs[:len(xs)-1]` | `slices.Insert(xs, i, x...)` → slice (F) | `slices.Delete(xs, i, j)` → slice (F) |
| Python | `xs.pop()` → 值（空时 IndexError） | `xs.insert(i, x)` | `xs.pop(i)` / `del xs[i]` |
| TS/JS | `a.pop()` → 值或 undefined | `a.splice(i, 0, x)`；copy 为 `a.toSpliced(i, 0, x)` (ES2023) | `a.splice(i, 1)`；copy `toSpliced(i,1)` |
| Gleam | — | — | — |
| Elixir | `List.pop_at(xs, -1)` → {v, rest} | `List.insert_at(xs, i, x)` (copy) | `List.delete_at(xs, i)` (copy) |
| OCaml | — | — | — |
| Kotlin | `removeLast()`（空时异常）/ `removeLastOrNull()` | `add(i, x)` | `removeAt(i)` (F) |
| Swift | `removeLast()`（空时 trap）/ `popLast()` → Optional | `insert(x, at: i)` | `remove(at: i)` |

- pop：`pop` 3（Rust、Python、JS），`removeLast` 2（Kotlin、Swift）。**无多数派**。
- insert：`insert` 4（Rust、Go、Python、Swift）/ `insert_at` 1。相对多数 `insert`。
- remove at：`remove` 1，`removeAt` 1，`remove(at:)` 1，`delete_at` 1，`Delete` 1，`pop(i)` 1。**无多数派**。

<a id="6-map辞書"></a>

## 6. map（字典）

| 语言 | 插入/覆盖 | 删除 | 获取（Option） | 获取（异常） | 带默认值 | 用函数更新 / upsert |
|---|---|---|---|---|---|---|
| Rust | `m.insert(k,v)` in-place → Option<旧值> | `m.remove(&k)` → Option | `m.get(&k)` → Option<&V> | `m[&k]` panic | `m.get(&k).copied().unwrap_or(d)` | `m.entry(k).and_modify(f).or_insert(v)` ・ `or_insert_with` |
| Go | `m[k] = v` in-place | `delete(m, k)` 内建 | `v, ok := m[k]` | — | `m[k]` 返回零值 | — |
| Python | `d[k] = v` in-place；copy `d \| {k: v}` (3.9) | `del d[k]` / `d.pop(k[, default])` | `d.get(k)` → None | `d[k]` KeyError | `d.get(k, default)` | `d.setdefault(k, v)`；`collections.defaultdict` |
| TS/JS | `m.set(k, v)` in-place → Map | `m.delete(k)` → bool | `m.get(k)` → undefined | — | `m.get(k) ?? d` | `Map.prototype.getOrInsert` / `getOrInsertComputed`（Stage 3〜4 提案，未确定） |
| Gleam | `dict.insert(d, k, v)` copy (F) | `dict.delete(d, k)` copy (F) | `dict.get(d, k)` → `Result(v, Nil)` (F) | — | `result.unwrap(dict.get(..), d)` | `dict.upsert(d, k, fn(Option(v)) -> v)` (F) |
| Elixir | `Map.put(m,k,v)` copy ・ `Map.put_new` | `Map.delete(m,k)` copy | `Map.fetch(m,k)` → `{:ok,v} \| :error` | `Map.fetch!` raise (F) | `Map.get(m,k,default)`（默认 nil）(F) | `Map.update(m,k,default,f)` (upsert) ・ `Map.update!` ・ `Map.get_and_update` |
| OCaml | `Hashtbl.replace` in-place（`add` 隐藏现有值）・ `Map.add` copy | `Hashtbl.remove` ・ `Map.remove` | `find_opt` | `find` → `Not_found` | — | `Map.update k (option -> option) m` ・ Base `Map.update ~f` |
| Kotlin | `m[k] = v` / `put` in-place；copy `m + (k to v)` | `remove(k)` in-place；copy `m - k` | `m[k]` / `get` → `V?` | `getValue(k)` 异常 (F) | `getOrDefault(k, d)` ・ `getOrElse(k) { }` (F) | `getOrPut(k) { }` (F) ・ `merge` (JVM) ・ `compute` (JVM) |
| Swift | `d[k] = v` / `updateValue(_:forKey:)` in-place | `d[k] = nil` / `removeValue(forKey:)` → Optional | `d[k]` → Optional | — | `d[k, default: d0]` | `d[k, default: 0] += 1` ・ `merge/merging(_:uniquingKeysWith:)` |

- 插入写法：`insert` 2（Rust、Gleam），`put` 2（Elixir、Kotlin），`set` 1（JS），`add`/`replace` 1（OCaml），下标赋值 3（Go、Python、Swift）。**无多数派**。
- 删除：`remove` 4（Rust、OCaml、Kotlin，另 Swift `removeValue`），`delete` 4（Go、JS、Gleam、Elixir），Python `del`/`pop`。**势均力敌，无多数派**。
- Option 获取写法：**`get` 6/9**（Rust、Python、JS、Gleam(Result)、Kotlin、Swift 用下标）— 严格说函数名为 `get` 且返回 Option/nullable 的是 Rust、Python、JS、Gleam、Kotlin，共 5 个。Elixir `get`=默认值，`fetch`=带标签。
- upsert 专用名：`upsert` 1（Gleam），`update` 2（Elixir、OCaml），`entry().or_insert` 1，`getOrPut` 1，`setdefault` 1。**无多数派**。

<a id="7-set"></a>

## 7. set

| 语言 | 添加 | 删除 | 并 / 交 / 差 |
|---|---|---|---|
| Rust | `insert` in-place → bool | `remove` → bool | `union` / `intersection` / `difference` / `symmetric_difference`（返回迭代器）；`&` `\|` `-` `^` 运算符生成新 set |
| Go | —（无 set 类型） | — | — |
| Python | `add` in-place | `remove`（不存在则 KeyError）/ `discard` | `union` / `intersection` / `difference` / `symmetric_difference` (copy) ・ `\|= &= -=` 和 `update` / `intersection_update` / `difference_update` (in-place) |
| TS/JS | `add` in-place (F) | `delete` → bool (F) | `union` / `intersection` / `difference` / `symmetricDifference` copy (ES2025, F) |
| Gleam | `set.insert` copy (F) | `set.delete` copy (F) | `union` / `intersection` / `difference` / `symmetric_difference` (F) |
| Elixir | `MapSet.put` copy | `MapSet.delete` copy | `MapSet.union` / `intersection` / `difference` / `symmetric_difference` |
| OCaml | `Set.add` copy | `Set.remove` copy | `Set.union` / `inter` / `diff` |
| Kotlin | `MutableSet.add` in-place；copy `s + x` | `remove`；copy `s - x` | infix `union` / `intersect` / `subtract` (F)（返回新 Set） |
| Swift | `insert` (mutating) → (inserted, member) | `remove` → Optional | copy `union` / `intersection` / `subtracting` / `symmetricDifference`；in-place `formUnion` / `formIntersection` / `subtract` / `formSymmetricDifference` |

- 添加：`insert` 3（Rust、Gleam、Swift），`add` 3（Python、JS、OCaml、Kotlin → 4），`put` 1。→ **`add` 4/8，`insert` 3/8**。
- 删除：`remove` 5（Rust、Python、OCaml、Kotlin、Swift），`delete` 3（JS、Gleam、Elixir）。**多数派 `remove` 5/8**。
- 并：**`union` 8/8**。交：**`intersection` 6/8**（OCaml `inter`、Kotlin `intersect`）。差：**`difference` 5/8**（OCaml `diff`、Kotlin `subtract`、Swift `subtracting`）。

<a id="8-sort--reverse"></a>

## 8. sort / reverse

| 语言 | sort in-place | sort copy | 键函数 | 比较函数 | reverse |
|---|---|---|---|---|---|
| Rust | `v.sort()` (stable) / `sort_unstable()` | —（std 中没有） | `sort_by_key(f)` / `sort_by_cached_key` | `sort_by(\|a,b\| …)` → Ordering | `v.reverse()` in-place；`iter().rev()` |
| Go | `slices.Sort(xs)` (F) | `slices.Sorted(seq)`（1.23，从 iter 得到新 slice，F） | — | `slices.SortFunc(xs, cmp)` / `SortStableFunc` (F) | `slices.Reverse` in-place (F) |
| Python | `xs.sort(key=, reverse=)` → None (F) | `sorted(xs, key=, reverse=)` | `key=` 参数 | （cmp 已移除；`functools.cmp_to_key`） | `xs.reverse()` → None (F)；`reversed(xs)` 迭代器 |
| TS/JS | `a.sort(cmp)` in-place（默认**字符串比较**） | `a.toSorted(cmp)` (ES2023) | — | `(a,b) => number` | `a.reverse()` in-place / `toReversed()` copy |
| Gleam | — | `list.sort(xs, by: compare)` (F) | — | 必须提供比较函数 (F) | `list.reverse` (F) |
| Elixir | — | `Enum.sort(xs)` / `Enum.sort(xs, sorter)` | `Enum.sort_by(xs, f)` | `Enum.sort(xs, fn a, b -> a <= b end)` / `:asc`/`:desc` | `Enum.reverse` |
| OCaml | `Array.sort cmp a` | `List.sort cmp xs` / `stable_sort` / `sort_uniq` (F) | —（Base `List.sort ~compare` (F)） | 必须有 `cmp` | `List.rev` (F) |
| Kotlin | `sort()` / `sortBy{}` / `sortWith(cmp)` / `sortDescending()` | `sorted()` / `sortedBy{}` / `sortedWith(cmp)` / `sortedDescending()` (F) | `sortBy` / `sortedBy` | `sortWith` / `sortedWith` | `reverse()` in-place / `reversed()` copy / `asReversed()` view (F) |
| Swift | `sort()` / `sort(by:)` (mutating) | `sorted()` / `sorted(by:)` | — | `by: (a,b) -> Bool` (areInIncreasingOrder) | `reverse()` / `reversed()` |

- “sort”=in-place：Rust、Go、Python、JS、Kotlin、Swift（6）。“sort”=copy：Gleam、Elixir、OCaml List（3）。copy 版名 `sorted` 出现在 Python、Kotlin、Swift、Go（`Sorted`）= 4。
- 键函数写法：`_by` 系 2（Elixir `sort_by`、Kotlin `sortBy`），Rust `sort_by_key`、Python `key=`。比较函数：Rust `sort_by`、Kotlin `sortWith`、Go `SortFunc`、Swift `by:`。**`sort_by` 意义因语言反转**（Rust=比较函数，Elixir/Kotlin=键函数）。无多数派。
- reverse：**`reverse` 8/9**（OCaml `rev`）。

<a id="9-破壊的--非破壊的の見分け方"></a>

## 9. 区分修改与不修改操作

| 语言 | 规则 | 官方措辞 |
|---|---|---|
| Rust | 用类型区分：接收 `&mut self` 的方法修改。无命名规则。转换前缀 `as_`/`to_`/`into_` 表达所有权及成本 (F) | API Guidelines C-CONV：`as_` 无开销、borrowed→borrowed / `to_` 昂贵 / `into_` 可变成本、owned→owned (F) |
| Go | 无规则。惯例是 in-place 函数无返回值（`slices.Sort`），产生新值的函数返回值（`slices.Sorted`、`Clone`）。`append`/`Insert`/`Delete` 同时修改和返回，为混合型 (F) | — |
| Python | in-place 方法返回 `None`，copy 是内建函数（`sorted`、`reversed`） | “为提醒用户它通过副作用操作，不返回排序后的序列（用 `sorted()` 明确请求新的已排序列表实例）。”(F) |
| TS/JS | 无规则（历史上混合）。ES2023 用 `to` 前缀添加 copy 版：`toSorted` / `toReversed` / `toSpliced` / `with` | — |
| Gleam | 全部不可变，无需区分 | — |
| Elixir | 全部不可变。`!` 意味“失败时异常”(F)，不是修改标记 | — |
| OCaml | 用类型区分（`Array`/`Hashtbl`/`Bytes` 可变，`List`/`Map`/`Set`/`String` 不可变）。in-place 函数返回 `unit` | — |
| Kotlin | 动词=in-place，过去分词=copy（`sort`/`sorted`、`reverse`/`reversed`、`shuffle`/`shuffled`）。也用类型区分（仅 `MutableList` 有 in-place 版） | “名称还应暗示方法是修改对象还是返回新对象。例如，`sort` 原地排序集合，而 `sorted` 返回集合的已排序副本。”(F) |
| Swift | 动词=mutating，`-ed`/`-ing`=不修改；名词操作中，名词=不修改，`form` 前缀=mutating。类型上还有 `mutating` 关键字 | “优先用动词过去分词命名不修改版（通常加 ed）” / “用动词现在分词、加 ing 命名不修改版” / “不修改方法使用名词，对应修改方法用 form 前缀”；例如 `x.sort()`/`x.sorted()`、`y.formUnion(z)`/`y.union(z)` (F) |

- 用名字区分：Kotlin、Swift、Python（函数/方法差别）、JS（仅新 API）= 4。用类型/返回值区分：Rust、Go、OCaml = 3。本来就没有可变集合：Gleam、Elixir = 2。

<a id="10-失敗しうるアクセスの命名"></a>

## 10. 可能失败的访问命名

| 语言 | 返回 Option/null | 异常/panic | 带默认值 | 规范 |
|---|---|---|---|---|
| Rust | `v.get(i)` → Option、`first()`、`last()`、`pop()` | `v[i]` panic、`.unwrap()`、`.expect(msg)` | `.unwrap_or(d)` / `unwrap_or_else` / `unwrap_or_default` | 返回类型区分；危险版用 `_unchecked`（unsafe）(F) |
| Go | `v, ok := m[k]` (comma-ok) | `xs[i]` panic | `m[k]` 为零值 | — |
| Python | `d.get(k)` → None | `xs[i]` IndexError、`d[k]` KeyError | `d.get(k, d0)`、`next(it, d0)` | — |
| TS/JS | `a[i]` / `a.at(i)` → undefined、`m.get(k)`、`find` | 几乎没有 | `?? d` | — |
| Gleam | `list.first`/`last` → `Result(a, Nil)` (F)；`dict.get` → Result (F)；**不存在 `list.at`** (F) | 无（使用方可用 `let assert` 终止） | `result.unwrap(r, d)` / `option.unwrap` | 返回 `Result(_, Nil)` 而非 Option |
| Elixir | `Map.fetch` → `{:ok, v} \| :error`；`Enum.fetch`；`Enum.at(xs, i)` → nil | `fetch!`、`Map.update!` (F) | `get(m, k, default)` (F)；`Enum.at(xs, i, default)` | `!`=raise、`fetch`=带标签、`get`=默认值 (F) |
| OCaml Stdlib | `List.nth_opt`、`find_opt`、`assoc_opt`、`Hashtbl.find_opt` (F) | `List.nth`、`find`、`List.hd`（异常） | `Option.value ~default` | **`_opt` 后缀=Option 版**（异常版无标记）(F) |
| OCaml Base | `List.nth`、`find`、`hd` → option (F) | `nth_exn`、`find_exn`、`hd_exn` (F) | `Option.value ~default` | **`_exn` 后缀=异常版**（Option 版无标记）(F) — 与 Stdlib 相反 |
| Kotlin | `getOrNull(i)`、`firstOrNull()`、`elementAtOrNull`、`m[k]` → `V?` (F) | `get(i)`/`[i]`、`first()`、`getValue(k)`、`elementAt` | `getOrElse(i) { }`、`getOrDefault`、`elementAtOrElse` (F) | **`OrNull` / `OrElse` / `OrDefault` 后缀** |
| Swift | `first`、`last`、`popLast()`、`d[k]` → Optional、`firstIndex(of:)` | `xs[i]` trap、`removeLast()` trap | `d[k, default:]`、`?? d` | 属性/返回类型区分 |

<a id="parse文字列int"></a>

### parse（字符串→Int）

| 语言 | 写法 | 失败时 |
|---|---|---|
| Rust | `s.parse::<i64>()` / `i64::from_str(s)` / `i64::from_str_radix` | `Result<_, ParseIntError>` (F) |
| Go | `strconv.Atoi(s)` / `strconv.ParseInt(s, base, bits)` | `(int, error)`；越界为 ErrRange 和最大值 |
| Python | `int(s)` | 抛出 `ValueError` |
| TS/JS | `Number.parseInt(s, 10)` / `Number(s)` | 返回 `NaN`（`parseInt("12abc")` = 12） |
| Gleam | `int.parse(s)` / `int.base_parse` | `Result(Int, Nil)`（float.parse 相同，F） |
| Elixir | `Integer.parse(s)` → `{int, rest} \| :error`；`String.to_integer(s)` | parse 返回元组，to_integer 抛异常 |
| OCaml | `int_of_string`（异常 `Failure`）/ `int_of_string_opt` → option；Base `Int.of_string`（异常）/ `Int.of_string_opt` | `_opt` 表示 Option |
| Kotlin | `s.toInt()` (`NumberFormatException`) / `s.toIntOrNull()` | `OrNull` 后缀 |
| Swift | `Int(s)` → `Int?`（failable init） | Optional |

- 多数派：parse 失败以**值（Result/Option/元组/null）返回 7/9**（Rust、Go、Gleam、Elixir parse、OCaml _opt、Kotlin OrNull、Swift）；仅抛异常 Python 1，NaN 的 JS 1。写法为 `parse` 4（Rust、Gleam、Elixir、JS parseInt），`of_string` 1，`toInt` 1，构造器 2（Python、Swift），`Atoi/ParseInt` 1。
- 用名字区分“Option 版/异常版”的语言：OCaml（`_opt`）、Base（`_exn`）、Kotlin（`OrNull`）、Elixir（`!`）、Rust（`_unchecked`/`unwrap`）= 5。**但无标记是哪一版因语言反转**（OCaml Stdlib/Kotlin：无标记=异常；Base/Rust/Elixir：无标记=安全）。

<a id="11-数値変換損失--検査--飽和--折返し"></a>

## 11. 数值转换（有损 / 检查 / 饱和 / 回绕）

| 语言 | 有损（默认） | 带检查 | 饱和 | 回绕 |
|---|---|---|---|---|
| Rust | `x as i32`（整数截位=回绕；float→int **饱和**，NaN→0） | `i32::try_from(x)` → Result；运算 `checked_add` → Option | 运算 `saturating_add`；类型转换用 `as`（float） | 运算 `wrapping_add`；`overflowing_add` →（值，bool） |
| Go | `int32(x)`（整数回绕，float→int 越界依实现） | — | — | 转换本身 |
| Python | `int(f)`（向 0 截断，任意精度所以不溢出） | — | — | — |
| TS/JS | `Math.trunc`、`\| 0`（32bit 回绕） | 用 `Number.isSafeInteger` 判定 | — | `BigInt.asIntN(bits, n)` |
| Gleam | `float.truncate` / `round` → Int (F)；`int.to_float` | — | `int.clamp` | — |
| Elixir | `trunc/1`、`round/1`（任意精度） | — | — | — |
| OCaml | `int_of_float` / `Float.to_int`（越界未定义）；`Int32.of_int`（回绕） | Base：`Int.of_float`（异常）、`Int64.to_int` → option、`to_int_exn` | — | Base `to_int_trunc` / Stdlib `Int32.of_int` |
| Kotlin | `Long.toInt()`（截位=回绕）；`Double.toInt()` = **饱和**，NaN→0 (F) | —（无标准函数） | `coerceIn(min, max)` | `Long.toInt()` |
| Swift | `Int(x)` = **越界 trap** | `Int(exactly: x)` → Optional (F) | `Int(clamping: x)` (F) | `Int(truncatingIfNeeded: x)` (F)；运算 `&+`、`addingReportingOverflow` |

- Swift API Guidelines (F)：“执行保值类型转换的初始化器应省略第一个参数标签” — 窄化转换需标签（如 `truncating`/`saturating`）。
- 四种全有独立名称的仅 Rust 和 Swift。写法模式：Rust 前缀 `checked_`/`saturating_`/`wrapping_`；Swift 参数标签 `exactly:`/`clamping:`/`truncatingIfNeeded:`。**无多数派**（其余七种语言只有 1〜2 类）。

<a id="12-変換の向きto_string"></a>

## 12. 转换方向与 to_string

| 语言 | 方向风格 | to_string |
|---|---|---|
| Rust | `to_X` / `into_X` / `as_X`（源类型方法）和 `X::from(y)` / `TryFrom` / `FromStr`（目标类型关联函数） | `x.to_string()` (Display) |
| Go | `pkg.Func(x)`：`strconv.Itoa`、`FormatInt`、`ParseInt`；类型转换 `T(x)` | `strconv.Itoa(n)`、`fmt.Sprint(x)`、`String()` 方法 (Stringer) |
| Python | 构造器 `int(x)`、`str(x)`、`list(it)`、`dict.fromkeys` | `str(x)` / `repr(x)` |
| TS/JS | 构造函数 `String(x)`、`Number(x)`、`Array.from(it)`、`Array.of(...)`、`Object.fromEntries` | `x.toString()` / `String(x)` |
| Gleam | 源模块 `to_X`：`int.to_string`、`int.to_float`、`list.to_...`；目标模块 `from_X`：`dict.from_list`、`set.from_list`、`string.from_utf_codepoints` | `int.to_string` (F) / `float.to_string` (F) / `string.inspect` (F) |
| Elixir | `Integer.to_string`、`String.to_integer`、`Map.new(list)`、`MapSet.new`、`Enum.into` | `to_string/1` (Kernel, String.Chars)、`inspect/1` |
| OCaml | Stdlib `int_of_string`/`string_of_int`（`B_of_A`）和 `Int.to_string`、`Float.of_string`（`of_`/`to_`）；Base 统一为 `of_X`/`to_X` | `string_of_int` / `Int.to_string` |
| Kotlin | `x.toX()`（`toInt`、`toList`、`toSet`、`toMap`）；构造 `listOf`、`mapOf`、`setOf` | `x.toString()` |
| Swift | 目标类型 init `Int(x)`、`String(x)`、`Array(seq)`、`Set(xs)` | `String(x)` / `"\(x)"` / `description` |

- 多数派：**源侧 `to_X` 方式 5/9**（Rust、Gleam、Elixir、新 OCaml、Kotlin），构造器方式 3（Python、JS、Swift）。兼用目标侧 `from_X`：Rust、Gleam、JS（`Array.from`）。
- to_string 写法：`to_string`/`toString` 5（Rust、Gleam、Elixir、OCaml `Int.to_string`、Kotlin、JS）→ 6/9；`str()`/`String()` 构造器 2（Python、Swift）；Go `Itoa`/`String()` 1。**多数派 `to_string` 系 6/9**。

<a id="13-述語の命名"></a>

## 13. 谓词命名

| 语言 | 风格 | 示例 |
|---|---|---|
| Rust | `is_X` / `has_X` / `starts_with` | `is_empty`、`is_some`、`is_ascii_digit` |
| Go | `IsX` / `HasX` / `Contains` | `unicode.IsDigit`、`strings.HasPrefix`、`math.IsNaN` |
| Python | `isX`（**无下划线**）/ `startswith` | `str.isdigit`、`str.isupper`、`os.path.isfile`、`math.isnan` |
| TS/JS | `isX` / `has` / `includes` | `Array.isArray`、`Number.isInteger`、`Set.has`、`isSubsetOf` (F) |
| Gleam | `is_X` / `has_key` / `contains` | `list.is_empty`、`dict.has_key`、`set.is_subset` (F)、`int.is_even` |
| Elixir | 后缀 `?`；可作 guard 的用 `is_X`（禁止两者并用）(F) | `Enum.empty?`、`String.contains?`、`Map.has_key?`、`is_list/1`、`Integer.is_even/1` |
| OCaml | 部分 `is_X`，很多为 `mem` / `exists` / `for_all` | `List.is_empty`、`String.starts_with` (F)、`Float.is_nan` |
| Kotlin | `isX` / `hasX` | `isEmpty`、`isNotEmpty`、`isBlank`、`isDigit` |
| Swift | `isX`（读作对主语的断言）(F) | `isEmpty`、`isMultiple(of:)`、`contains`、`hasPrefix` |

- 多数派：**`is` 前缀 8/9**（`is_` 3：Rust、Gleam、OCaml；`isX`/`IsX` 5：Go、Python〔无分隔〕、JS、Kotlin、Swift）。例外 Elixir（`?` 后缀）。

<a id="14-引数順データ先頭か関数先頭かとパイプ"></a>

## 14. 参数顺序（数据优先或函数优先）与管道

| 语言 | 顺序 | 管道 |
|---|---|---|
| Rust | 接收者优先（方法 `xs.iter().map(f)`） | 方法链替代 |
| Go | 数据优先 `slices.IndexFunc(xs, f)` | 无 |
| Python | **函数优先** `map(f, xs)`、`filter(f, xs)`；`sorted(xs, key=f)` 数据优先 → 混合 | 无 |
| TS/JS | 接收者优先（方法）；`Object.groupBy(items, fn)` 数据优先 | 无（`\|>` 是 TC39 Stage 2 提案） |
| Gleam | **数据优先** `list.map(xs, f)`（F，已确认签名） | `\|>` 插入**第一个参数** |
| Elixir | **数据优先** `Enum.map(xs, f)` | `\|>` 插入**第一个参数** |
| OCaml Stdlib | **函数优先** `List.map f xs`、`List.sort cmp xs` (F) | `\|>` 插入**最后参数**（`xs \|> List.map f`） |
| OCaml Base | **数据优先 + 标签** `List.map xs ~f` (F) | 因标签也可写 `xs \|> List.map ~f` |
| Kotlin | 接收者优先（扩展函数 `xs.map { }`） | 方法链 |
| Swift | 接收者优先（方法） | 方法链 |
| （参考 F#） | 函数优先 `List.map f xs` | `\|>` 插入最后参数 |

- 多数派：**数据（接收者）优先 7/9**（Rust、Go、JS、Gleam、Elixir、Kotlin、Swift；另 Base）。函数优先：OCaml Stdlib、Python map/filter。

<a id="15-高階関数の派生名_by--_with--_while--_map"></a>

## 15. 高阶函数派生命名（_by / _with / _while / _map）

| 语言 | 键函数 `_by` | 比较/结合 `_with` | `_while` | `filter_map` | `flat_map` | `find_map` |
|---|---|---|---|---|---|---|
| Rust | `sort_by_key`、`max_by_key`、`min_by_key`、`dedup_by_key` | `sort_by`（比较）、`max_by`（比较）、`zip` + `map` | `take_while`、`skip_while`、`map_while` | `filter_map` | `flat_map` | `find_map` |
| Go | — | `SortFunc`、`IndexFunc`、`ContainsFunc`、`MaxFunc`（**`Func` 后缀**）(F) | — | — | — | — |
| Python | `key=` 参数（`sorted`、`max`、`min`、`itertools.groupby`） | — | `itertools.takewhile`、`dropwhile` | （推导式） | `itertools.chain.from_iterable` | — |
| TS/JS | — | — | — | — | `flatMap` | —（`find`） |
| Gleam | `list.chunk(xs, by: f)` (F)、`group` | `sort(by:)`（比较）、`dict.combine(with:)` (F) | `take_while`、`drop_while` (F) | `filter_map` (F) | `flat_map` (F) | `find_map` (F) |
| Elixir | `sort_by`、`max_by`、`min_by`、`uniq_by`、`group_by`、`chunk_by`、`split_with`（谓词） | — | `take_while`、`drop_while`、`split_while` | （`filter_map` 已废弃；`Enum.flat_map`/`for`） | `flat_map` | `find_value` |
| OCaml | —（Base `List.max_elt ~compare`） | `List.sort_uniq cmp` | `take_while`、`drop_while`（5.3 起，F） | `filter_map` (F) | `concat_map` (F) | `find_map` (F) |
| Kotlin | `sortBy`、`sortedBy`、`maxBy`、`minBy`、`distinctBy`、`groupBy`、`associateBy` | `sortWith`、`sortedWith`、`maxWith`、`minWith`、`zip(other){…}` | `takeWhile`、`dropWhile` | `mapNotNull` (F) | `flatMap` | `firstNotNullOf(OrNull)` (F) |
| Swift | — | `sorted(by:)`、`max(by:)`（比较，参数标签） | `prefix(while:)`、`drop(while:)` | `compactMap` | `flatMap` | `first(where:)` + 手工处理 |

- `_by`=键函数：Rust（`_by_key`）、Elixir、Kotlin、Gleam `chunk(by:)` → Elixir/Kotlin 有明确体系。Rust、Swift、Gleam 的 `sort(by:)` 中，`by` = **比较函数**。**`_by` 的意义无多数派**。
- `_with`=比较函数：只有 Kotlin 系统采用（`sortWith`/`maxWith`）。Go 用 `Func` 后缀。
- `take_while`/`takeWhile` 系：**7/9**（Rust、Python、Gleam、Elixir、OCaml、Kotlin，包括 Swift `prefix(while:)`）。
- `filter_map`：该写法 3（Rust、Gleam、OCaml），别名 `mapNotNull`、`compactMap`。
- `flat_map`/`flatMap`：**6/9**（Rust、JS、Gleam、Elixir、Kotlin、Swift）；OCaml `concat_map`。
- `find_map`：3（Rust、Gleam、OCaml），Elixir `find_value`、Kotlin `firstNotNullOf`。

<a id="16-スライス"></a>

## 16. 切片

| 语言 | 提取区间 | take / drop | 负 index | 半开区间 |
|---|---|---|---|---|
| Rust | `&v[a..b]` (panic) / `v.get(a..b)` → Option (F) | `iter().take(n)`、`skip(n)` | 不可 | 半开 `a..b`；闭 `a..=b` |
| Go | `xs[a:b]` (panic) | — | 不可 | 半开 |
| Python | `xs[a:b]`（越界截到边界） | —（`itertools.islice`） | 可 | 半开 |
| TS/JS | `a.slice(start, end)`、`s.slice`、`s.substring` | — | `slice` 可，`substring` 不可 | 半开 |
| Gleam | `string.slice(s, at_index:, length:)` = **(开始, 长度)**，可负数 (F)；list 用 `take`/`drop` (F) | `list.take`/`drop` (F)；`string.drop_start` (F) | `string.slice` 可 (F) | — |
| Elixir | `String.slice(s, start, len)` / `String.slice(s, a..b)`；`Enum.slice(xs, a..b)` | `Enum.take`/`drop`（负数则从末尾） | 可 | Range 为**闭区间**（`a..b//1`） |
| OCaml | `String.sub s pos len` = **(开始, 长度)**，越界异常；`Array.sub` | `List.take`/`drop`（5.3 起，F） | 不可 | — |
| Kotlin | `subList(a, b)`、`slice(a..b)`（闭区间）、`substring(a, b)`（半开） | `take`/`drop`/`takeLast`/`dropLast` | 不可 | substring/subList 半开，slice(range) 闭 |
| Swift | `xs[a..<b]` / `xs[a...b]`（越界 trap）；String 必须用 `String.Index` | `prefix(n)`、`dropFirst(n)`、`suffix`、`dropLast` | 不可 | 运算符区分两者 |

- 多数派：(start, end) 半开 **5/9**（Rust、Go、Python、JS、Kotlin substring）；(start, length) 2（Gleam string、OCaml）；Elixir 两者都有。take/drop 写法：**`take`/`drop` 6/9**（Gleam、Elixir、OCaml、Kotlin、Rust take + skip 等）。允许负 index：Python、JS(slice)、Gleam、Elixir = 4。

<a id="17-大文字小文字"></a>

## 17. 大小写

| 语言 | 写法 |
|---|---|
| Rust | `to_uppercase` / `to_lowercase`；ASCII 版 `to_ascii_uppercase` (F) |
| Go | `strings.ToUpper` / `ToLower` |
| Python | `str.upper()` / `lower()`；`casefold` |
| TS/JS | `toUpperCase()` / `toLowerCase()` |
| Gleam | `string.uppercase` / `lowercase` (F) |
| Elixir | `String.upcase` / `downcase` |
| OCaml | `String.uppercase_ascii` / `lowercase_ascii`（Stdlib 仅 ASCII）；Base `String.uppercase` |
| Kotlin | `uppercase()` / `lowercase()`（1.5 起；`toUpperCase` 已弃用） |
| Swift | `uppercased()` / `lowercased()` |

- `to_upper` 系（`to_uppercase`、`ToUpper`、`toUpperCase`）3，`uppercase` 系（`uppercase`、`uppercase_ascii`、`uppercased`）4，`upper` 1，`upcase` 1。**相对多数 `uppercase` 4/9，未过半**。

<a id="18-trim-系"></a>

## 18. trim 系

| 语言 | 两端 | 开头 | 末尾 |
|---|---|---|---|
| Rust | `trim` | `trim_start`（`trim_left` 已弃用）(F) | `trim_end`（`trim_right` 已弃用）(F) |
| Go | `strings.TrimSpace`（`Trim(s, cutset)`） | `TrimLeft(s, cutset)` / `TrimPrefix` | `TrimRight` / `TrimSuffix` |
| Python | `strip` | `lstrip` | `rstrip` |
| TS/JS | `trim` | `trimStart`（`trimLeft` 别名） | `trimEnd`（`trimRight` 别名） |
| Gleam | `trim` (F) | `trim_start` (F) | `trim_end` (F) |
| Elixir | `String.trim` | `trim_leading` | `trim_trailing` |
| OCaml | `String.trim`（仅两端） | —（Base `lstrip`） | —（Base `rstrip`，Base 两端为 `strip`） |
| Kotlin | `trim` | `trimStart` | `trimEnd` |
| Swift | `trimmingCharacters(in: .whitespaces)` (Foundation) | — | — |

- 两端：**`trim` 6/9**。开头/末尾：**`trim_start`/`trim_end`（`trimStart`/`trimEnd`）4/9**（Rust、JS、Gleam、Kotlin）为相对多数；`lstrip/rstrip` 2（Python、Base）、`trim_leading/trailing` 1、`TrimLeft/Right` 1。left/right 写法在 Rust、JS 被弃用。

<a id="19-index-of"></a>

## 19. index-of

| 语言 | 写法 | 未找到时 |
|---|---|---|
| Rust | `s.find(pat)` (F) / `iter().position(f)` | `None` |
| Go | `strings.Index`、`slices.Index`、`IndexFunc` (F) | **-1** |
| Python | `s.find(sub)` / `s.index(sub)`、`xs.index(x)` | find 为 **-1**，index 为 ValueError |
| TS/JS | `indexOf`、`findIndex`、`lastIndexOf` | **-1** |
| Gleam | 无（string/list 均无 index-of 函数，F） | — |
| Elixir | `Enum.find_index(xs, f)`；字符串用 `:binary.match` | `nil` / `:nomatch` |
| OCaml | `String.index_opt s c`（char）(F)、`String.find_first ~sub s`（5.5 起）→ option (F)；`String.index` 抛异常 | `None` / 异常 |
| Kotlin | `indexOf`、`indexOfFirst`、`lastIndexOf` | **-1** |
| Swift | `firstIndex(of:)`、`firstIndex(where:)`、`lastIndex(of:)` | `nil` (Optional) |

- 返回 -1：Go、Python(find)、JS、Kotlin = 4。Option/nil：Rust、Elixir、OCaml、Swift = 4。Gleam 无此函数。**势均力敌，无多数派**（写法为 `index`/`indexOf` 系的有 5）。

<a id="20-プロセス--argv--env"></a>

## 20. 进程 / argv / env

| 语言 | argv | 获取 env | 设置 env | exit | cwd |
|---|---|---|---|---|---|
| Rust | `std::env::args()` (iter) | `std::env::var(k)` → `Result` | `std::env::set_var`（2024 edition 为 `unsafe`） | `std::process::exit(code)` | `std::env::current_dir()` → Result |
| Go | `os.Args` ([]string) | `os.Getenv(k)`（无则 ""）/ `os.LookupEnv` → (v, ok) | `os.Setenv` → error | `os.Exit(code)` | `os.Getwd()` → (string, error) |
| Python | `sys.argv` | `os.environ[k]` (KeyError) / `os.getenv(k, default)` / `os.environ.get` | `os.environ[k] = v` | `sys.exit(code)` | `os.getcwd()` |
| TS/JS | ECMAScript 无（Node `process.argv`、Deno `Deno.args`） | 同（`process.env.K`） | 同 | 同（`process.exit`） | 同（`process.cwd()`） |
| Gleam | gleam_stdlib 无（外部 `argv` package、`envoy`） | 无 | 无 | 无 | 无 |
| Elixir | `System.argv()` | `System.get_env(k)` → nil / `System.fetch_env` → {:ok,v}\|:error / `fetch_env!` | `System.put_env` | `System.halt(code)` / `System.stop` | `File.cwd()` → {:ok, p} / `File.cwd!` |
| OCaml | `Sys.argv` (array) | `Sys.getenv` (Not_found) / `Sys.getenv_opt` | `Unix.putenv`（unix 库） | `exit code` (Stdlib) | `Sys.getcwd ()` |
| Kotlin | `main(args: Array<String>)` | JVM `System.getenv(k)` → `String?` | （JVM 标准无） | `kotlin.system.exitProcess(code)` | JVM `System.getProperty("user.dir")` |
| Swift | `CommandLine.arguments` (stdlib) | Foundation `ProcessInfo.processInfo.environment[k]` | `setenv` (libc) | `exit(code)` (libc / Foundation) | `FileManager.default.currentDirectoryPath` |

- 放置位置：放在 **`env`/`os`/`sys`/`System` 等进程专用模块**的有 7/9（Rust `env`+`process`、Go `os`、Python `sys`+`os`、Elixir `System`、OCaml `Sys`、Kotlin `kotlin.system`、Swift `CommandLine`）。语言标准没有：JS(ECMAScript)、Gleam。
- 写法：argv 的 `args`/`Args`/`argv`/`arguments` 势均力敌（args 3：Rust、Go、Kotlin / argv 3：Python、Elixir、OCaml / arguments 1）。env 获取用 `getenv`/`get_env`/`Getenv` 的有 5（Go、Python、Elixir、OCaml、Kotlin），Rust 用 `var`。exit 为 **`exit` 6/9**（Rust、Go、Python、OCaml、Kotlin `exitProcess`、Swift）。cwd 为 `getcwd`/`Getwd` 4（Go、Python、OCaml，另 Rust `current_dir`）。

---

<a id="要約多数派が存在するもの--しないもの"></a>

## 总结（存在多数派 / 不存在多数派）

有多数派：判空 `is_empty` (5/9)、list 成员 `contains` (5/9)、set 删除 `remove` (5/8)、set 并 `union` (8/8) / 交 `intersection` (6/8) / 差 `difference` (5/8)、`reverse` (8/9)、to_string 系 (6/9)、源侧转换 `to_X` (5/9)、谓词 `is` 前缀 (8/9)、数据优先参数顺序 (7/9)、`flat_map` (6/9)、`take_while` 系 (7/9)、`trim` (6/9)、parse 失败返回值 (7/9)、exit (6/9)。

无多数派：长度写法（length 4 / len 3 / size / count）和字符串长度单位；in-place 添加（append/push/add）；pop / remove-at；map 插入（insert/put/set/下标）及删除（remove 4 对 delete 4）；upsert；sort 默认 in-place 还是 copy（6 对 3）；`_by` 意义（键函数或比较函数）；大写（uppercase 4 / to_upper 3）；index-of（-1 为 4 / Option 为 4）；四种数值转换（仅 Rust、Swift 全覆盖）；Option 版/异常版哪种无标记（OCaml Stdlib/Kotlin：无标记=异常，Base/Rust/Elixir：无标记=安全）。
