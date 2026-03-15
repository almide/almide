# stdlib: set [Tier 2]

集合型。Python, Rust, JS 全てにある基本データ構造。Almide にはない。

## 他言語比較

| 操作 | Python (`set`) | Rust (`HashSet`) | JS (`Set`) | Go |
|------|---------------|-----------------|------------|-----|
| 生成 | `set()`, `{1,2,3}` | `HashSet::new()`, `HashSet::from([1,2,3])` | `new Set([1,2,3])` | `map[T]bool{}` |
| 追加 | `s.add(x)` | `s.insert(x)` | `s.add(x)` | `m[x] = true` |
| 削除 | `s.remove(x)`, `s.discard(x)` | `s.remove(&x)` | `s.delete(x)` | `delete(m, x)` |
| 含む | `x in s` | `s.contains(&x)` | `s.has(x)` | `m[x]` |
| サイズ | `len(s)` | `s.len()` | `s.size` | `len(m)` |
| 和集合 | `a \| b`, `a.union(b)` | `a.union(&b)` | manual | manual |
| 積集合 | `a & b`, `a.intersection(b)` | `a.intersection(&b)` | manual | manual |
| 差集合 | `a - b`, `a.difference(b)` | `a.difference(&b)` | manual | manual |
| 対称差 | `a ^ b`, `a.symmetric_difference(b)` | `a.symmetric_difference(&b)` | manual | manual |
| 部分集合 | `a <= b`, `a.issubset(b)` | `a.is_subset(&b)` | manual | manual |
| リスト変換 | `list(s)` | `s.into_iter().collect::<Vec<_>>()` | `[...s]` | manual |

## 追加候補 (~12 関数)

### P0
- `set.new() -> Set[T]` — 空セット
- `set.from_list(xs) -> Set[T]` — リストから生成
- `set.add(s, x) -> Set[T]` — 追加（immutable、新セット返却）
- `set.remove(s, x) -> Set[T]` — 削除
- `set.contains?(s, x) -> Bool` — 含む
- `set.len(s) -> Int` — サイズ
- `set.to_list(s) -> List[T]` — リスト変換

### P1
- `set.union(a, b) -> Set[T]`
- `set.intersection(a, b) -> Set[T]`
- `set.difference(a, b) -> Set[T]`
- `set.is_subset?(a, b) -> Bool`
- `set.is_empty?(s) -> Bool`

## 実装戦略

TOML + runtime。Rust: `HashSet<T>`。TS: `Set<T>`。
型システムに `Ty::Set(Box<Ty>)` を追加する必要がある。
