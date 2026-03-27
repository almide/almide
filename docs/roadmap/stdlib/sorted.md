<!-- description: Sorted collections (binary search, heap, deque) -->
# stdlib: sorted collections [Tier 3]

ソート済みデータ構造。Go/Python/Rust/Deno 全てに何らかの形で存在。

## 他言語比較

| データ構造 | Go (`sort`) | Python (`bisect`, `heapq`) | Rust (`BTreeMap`, `BinaryHeap`) | Deno (`@std/collections`) |
|-----------|-------------|---------------------------|-------------------------------|--------------------------|
| ソート済みマップ | ❌ | ❌ (SortedDict は 3rd party) | `BTreeMap` | ❌ |
| ソート済みセット | ❌ | ❌ (SortedSet は 3rd party) | `BTreeSet` | ❌ |
| 二分探索 | `sort.Search` | `bisect.bisect_left/right` | `slice.binary_search` | `@std/collections/binary-search-node` |
| ヒープ/優先度キュー | `container/heap` | `heapq` | `BinaryHeap` | ❌ |
| デック | `container/list` | `collections.deque` | `VecDeque` | ❌ |

## 追加候補 (~10 関数)

### P1 (二分探索)
- `list.binary_search(sorted_list, value) -> Option[Int]` — ソート済みリスト内の位置
- `list.insert_sorted(sorted_list, value) -> List[T]` — ソート位置に挿入

### P2 (ヒープ)
- `heap.new() -> Heap[T]`
- `heap.push(h, value) -> Heap[T]`
- `heap.pop(h) -> (T, Heap[T])`
- `heap.peek(h) -> Option[T]`
- `heap.from_list(xs) -> Heap[T]`
- `heap.len(h) -> Int`

### P3 (デック)
- `deque.new() -> Deque[T]`
- `deque.push_front(d, x) / push_back(d, x) / pop_front(d) / pop_back(d)`

## 実装戦略

list の二分探索は TOML + runtime で追加可能。ヒープ・デックは新しい型が必要（Ty::Heap, Ty::Deque）。
