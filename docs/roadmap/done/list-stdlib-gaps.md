<!-- description: Cross-language comparison of missing list module operations -->
<!-- done: 2026-03-11 -->
# List Stdlib Gaps

Almide's `list` module compared against 7 languages. All operations are immutable (return new list).

## Current (40 functions)

len, get, get_or, sort, reverse, contains, each, map, filter, find, fold, any, all, enumerate, zip, flatten, take, drop, sort_by, unique, index_of, last, chunk, sum, product, first, is_empty?, flat_map, min, max, join, filter_map, take_while, drop_while, count, partition, reduce, group_by

## Cross-language comparison

| Operation | Python | JS | Rust | Haskell | Elixir | Kotlin | Go | Almide |
|-----------|--------|----|------|---------|--------|--------|----|--------|
| get by index | `xs[i]` | `xs[i]` | `xs[i]` / `get` | `!!` | `Enum.at` | `get` | `xs[i]` | `get` |
| set by index | `xs[i]=v` | `with(i,v)` | `xs[i]=v` | — | `replace_at` | `set` | `xs[i]=v` | `set` (v0.4.5) |
| **insert at** | `insert` | `toSpliced` | `insert` | — | `insert_at` | `add(i,v)` | `slices.Insert` | **MISSING** |
| **remove at** | `pop(i)` | `toSpliced` | `remove` | — | `delete_at` | `removeAt` | `slices.Delete` | **MISSING** |
| ~~swap~~ | — | — | `swap` | — | — | — | — | `swap` (v0.4.5) |
| **update at** | — | — | — | — | `update_at` | — | — | **MISSING** |
| **slice** | `xs[a:b]` | `slice` | `&xs[a..b]` | — | `Enum.slice` | `subList` | `xs[a:b]` | **MISSING** |
| **range** | `range` | — | `0..n` | `[1..n]` | `Range` | `IntRange` | — | **MISSING** |
| **repeat** | `[x]*n` | `Array(n).fill` | `vec![x; n]` | `replicate` | `duplicate` | — | — | **MISSING** |
| map | `map` | `map` | `iter().map` | `map` | `Enum.map` | `map` | — | `map` |
| filter | `filter` | `filter` | `iter().filter` | `filter` | `Enum.filter` | `filter` | — | `filter` |
| fold/reduce | `reduce` | `reduce` | `iter().fold` | `foldl` | `Enum.reduce` | `fold` | — | `fold` |
| find | — | `find` | `iter().find` | `find` | `Enum.find` | `find` | — | `find` |
| sort | `sort` | `toSorted` | `sort` | `sort` | `Enum.sort` | `sorted` | `slices.Sort` | `sort` |
| reverse | `reverse` | `toReversed` | `rev` | `reverse` | `Enum.reverse` | `reversed` | `slices.Reverse` | `reverse` |
| flatten | — | `flat` | `flatten` | `concat` | `Enum.flat_map` | `flatten` | — | `flatten` |
| zip | `zip` | — | `zip` | `zip` | `Enum.zip` | `zip` | — | `zip` |
| take/drop | — | — | `take`/`skip` | `take`/`drop` | `Enum.take`/`drop` | `take`/`drop` | — | `take`/`drop` |
| enumerate | `enumerate` | `entries` | `enumerate` | `zip [0..]` | `Enum.with_index` | `withIndex` | — | `enumerate` |
| chunk | — | — | `chunks` | — | `Enum.chunk_every` | `chunked` | — | `chunk` |
| **scan** | `accumulate` | — | `scan` | `scanl` | `Enum.scan` | `scan` | — | **MISSING** |
| **intersperse** | — | — | `intersperse` | `intersperse` | `Enum.intersperse` | — | — | **MISSING** |
| **windows** | — | — | `windows` | — | `Enum.chunk_every(n,1)` | `windowed` | — | **MISSING** |
| **dedup** | — | — | `dedup` | `nub` | `Enum.dedup` | `distinct` | `slices.Compact` | **MISSING** |
| **zip_with** | — | — | — | `zipWith` | `Enum.zip_with` | — | — | **MISSING** |
| **find_index** | `index` | `findIndex` | `position` | `findIndex` | `Enum.find_index` | `indexOfFirst` | — | **MISSING** |
| contains | `in` | `includes` | `contains` | `elem` | `Enum.member?` | `contains` | `slices.Contains` | `contains` |
| unique | `set(xs)` | `[...new Set]` | `dedup` | `nub` | `Enum.uniq` | `distinct` | `slices.Compact` | `unique` |
| group_by | — | — | — | `groupBy` | `Enum.group_by` | `groupBy` | — | `group_by` |

## Gap analysis

### Tier 1 — Blocking / very high frequency
LLMs generate these constantly. Missing them causes runtime errors or forces verbose workarounds.

| Function | Signature | Present in | Priority |
|----------|-----------|------------|----------|
| ~~set~~ | `(xs, i, value) -> List[T]` | Python, JS, Rust, Elixir, Kotlin, Go | **Done** (v0.4.5) |
| ~~range~~ | `(start, end) -> List[Int]` | Python, Rust, Haskell, Elixir, Kotlin | **Done** (v0.4.8) |
| ~~slice~~ | `(xs, start, end) -> List[T]` | Python, JS, Rust, Elixir, Kotlin, Go | **Done** (v0.4.8) |

### Tier 2 — Algorithm support
Needed for sorting, graph, tree manipulation tasks.

| Function | Signature | Present in |
|----------|-----------|------------|
| ~~insert~~ | `(xs, i, value) -> List[T]` | Python, JS, Rust, Elixir, Kotlin, Go | **Done** (v0.4.8) |
| ~~remove_at~~ | `(xs, i) -> List[T]` | Python, JS, Rust, Elixir, Kotlin, Go | **Done** (v0.4.8) |
| ~~swap~~ | `(xs, i, j) -> List[T]` | Rust | **Done** (v0.4.5) |
| ~~find_index~~ | `(xs, f) -> Option[Int]` | Python, JS, Rust, Haskell, Elixir, Kotlin | **Done** (v0.4.8) |

### Tier 3 — Functional transforms
Nice-to-have. Increases expressiveness for pipeline-heavy code.

| Function | Signature | Present in |
|----------|-----------|------------|
| ~~update~~ | `(xs, i, f) -> List[T]` | Elixir | **Done** (v0.4.8) |
| ~~repeat~~ | `(value, n) -> List[T]` | Python, Rust, Haskell, Elixir | **Done** (v0.4.8) |
| ~~scan~~ | `(xs, init, f) -> List[U]` | Python, Rust, Haskell, Elixir, Kotlin | **Done** (v0.4.8) |
| ~~intersperse~~ | `(xs, sep) -> List[T]` | Rust, Haskell, Elixir | **Done** (v0.4.8) |
| ~~windows~~ | `(xs, n) -> List[List[T]]` | Rust, Kotlin | **Done** (v0.4.8) |
| ~~dedup~~ | `(xs) -> List[T]` | Rust, Haskell, Elixir, Go | **Done** (v0.4.8) |
| ~~zip_with~~ | `(a, b, f) -> List[U]` | Haskell, Elixir | **Done** (v0.4.8) |

## Implementation order

1. **set** — blocking now
2. **range** — most commonly generated by LLMs after set
3. **slice, find_index** — high frequency
4. **insert, remove_at, swap** — algorithm tasks
5. **repeat, update, scan, dedup** — utility
6. **intersperse, windows, zip_with** — niche

## Sources

- [Python list methods](https://docs.python.org/3/tutorial/datastructures.html)
- [JavaScript Array (MDN)](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/Array)
- [JS immutable array methods (with/toSpliced/toSorted/toReversed)](https://claritydev.net/blog/immutable-array-operations-tosorted-tospliced-toreversed)
- [Rust Vec](https://doc.rust-lang.org/std/vec/struct.Vec.html)
- [Haskell Data.List](https://hackage.haskell.org/package/base/docs/Data-List.html)
- [Elixir Enum](https://hexdocs.pm/elixir/Enum.html) / [List](https://hexdocs.pm/elixir/List.html)
- [Kotlin collections](https://kotlinlang.org/docs/collection-operations.html)
- [Go slices package](https://pkg.go.dev/slices)
