# Aggregate Verbs

Analysis of aggregation verbs across Almide's stdlib: `fold`, `reduce`, `scan`, `sum`, `product`, `min`, `max`, `count`, `any`, `all`.

---

## 1. Current State in Almide

### Verb-to-Module Matrix

| Verb | list | map | string | int | float | math | result |
|------|:----:|:---:|:------:|:---:|:-----:|:----:|:------:|
| `fold` | `List[A], B, Fn[B,A]->B -> B` | -- | -- | -- | -- | -- | -- |
| `reduce` | `List[A], Fn[A,A]->A -> Option[A]` | -- | -- | -- | -- | -- | -- |
| `scan` | `List[A], B, Fn[B,A]->B -> List[B]` | -- | -- | -- | -- | -- | -- |
| `sum` | `List[Int] -> Int` | -- | -- | -- | -- | -- | -- |
| `sum_float` | `List[Float] -> Float` | -- | -- | -- | -- | -- | -- |
| `product` | `List[Int] -> Int` | -- | -- | -- | -- | -- | -- |
| `product_float` | `List[Float] -> Float` | -- | -- | -- | -- | -- | -- |
| `min` | `List[A] -> Option[A]` | -- | -- | `Int,Int -> Int` | `Float,Float -> Float` | `Int,Int -> Int` | -- |
| `max` | `List[A] -> Option[A]` | -- | -- | `Int,Int -> Int` | `Float,Float -> Float` | `Int,Int -> Int` | -- |
| `count` | `List[A], Fn[A]->Bool -> Int` | -- | `String,String -> Int` | -- | -- | -- | -- |
| `any` | `List[A], Fn[A]->Bool -> Bool` | -- | -- | -- | -- | -- | -- |
| `all` | `List[A], Fn[A]->Bool -> Bool` | -- | -- | -- | -- | -- | -- |

**Observations:**

- All closure-taking aggregates live exclusively on `list`.
- `map` has `filter` and `map_values` but zero aggregate verbs.
- `min`/`max` are semantically split: on `list` they find the extremum of a collection; on `int`/`float`/`math` they compare two scalars. This is fine -- different arities, different purposes.
- `count` has a semantic split (see Section 4).

---

## 2. Per-Verb Cross-Language Comparison

### fold

| Language | Name | Signature sketch | Notes |
|----------|------|------------------|-------|
| **Rust** | `Iterator::fold` | `fold(init, f)` | Left fold. No `fold_right`. |
| **Kotlin** | `Iterable.fold` | `fold(init, f)` | Also `foldRight`, `foldIndexed`. |
| **Swift** | `Sequence.reduce` | `reduce(init, f)` | Swift calls fold "reduce". |
| **Gleam** | `list.fold` | `fold(list, init, f)` | Arg order: list, init, f. |
| **Python** | `functools.reduce` | `reduce(f, iterable, init)` | Python calls fold "reduce". |
| **Go** | -- | -- | No stdlib fold. |
| **Ruby** | `Enumerable#inject` / `reduce` | `inject(init) { \|acc, x\| ... }` | `inject` and `reduce` are aliases. |
| **TypeScript** | `Array.reduce` | `reduce(f, init)` | JS/TS calls fold "reduce". |
| **Almide** | `list.fold` | `fold(xs, init, f)` | Left fold. |

**Consensus:** Most languages have fold or call it reduce-with-init. Almide's `fold` is the correct name (avoids ambiguity with the no-init variant).

### reduce

| Language | Name | Signature sketch | Notes |
|----------|------|------------------|-------|
| **Rust** | `Iterator::reduce` | `reduce(f) -> Option<T>` | Returns `Option`. |
| **Kotlin** | `Iterable.reduce` | `reduce(f)` | Throws on empty. Also `reduceOrNull`. |
| **Swift** | -- | -- | No no-init variant. Must use `reduce(into:)`. |
| **Gleam** | `list.reduce` | `reduce(list, f) -> Result` | Returns `Result`. |
| **Python** | `functools.reduce` (no init) | `reduce(f, iterable)` | Raises on empty. |
| **Go** | -- | -- | No stdlib reduce. |
| **Ruby** | `Enumerable#reduce` (no init) | `reduce { \|a, b\| ... }` | Raises on empty. |
| **TypeScript** | `Array.reduce` (no init) | `reduce(f)` | Throws on empty. |
| **Almide** | `list.reduce` | `reduce(xs, f) -> Option[A]` | Returns `Option`. Safe. |

**Consensus:** Almide follows Rust's safe pattern (returns Option). Good choice.

### scan

| Language | Name | Signature sketch | Notes |
|----------|------|------------------|-------|
| **Rust** | `Iterator::scan` | `scan(init, f)` | More complex (returns Option per step). |
| **Kotlin** | `Iterable.scan` | `scan(init, f)` | Returns list including init value. |
| **Swift** | -- | -- | No stdlib scan. `scan` in Combine framework. |
| **Gleam** | `list.scan` | `scan(list, init, f)` | Returns list. |
| **Python** | `itertools.accumulate` | `accumulate(iterable, f, initial=...)` | Different name. |
| **Go** | -- | -- | No stdlib scan. |
| **Ruby** | -- | -- | No scan (custom via each_with_object). |
| **TypeScript** | -- | -- | No stdlib scan. `rxjs.scan` exists. |
| **Almide** | `list.scan` | `scan(xs, init, f) -> List[B]` | Returns list of intermediates (excludes init). |

**Note:** Almide's scan excludes the initial value from the output. Kotlin includes it. Gleam excludes it. This should be documented explicitly, but the current behavior is reasonable.

### sum / product

| Language | Name | Notes |
|----------|------|-------|
| **Rust** | `Iterator::sum`, `Iterator::product` | Exists. Requires `Sum`/`Product` trait. |
| **Kotlin** | `Iterable.sum()`, `Iterable.sumOf(f)` | No `product`. |
| **Swift** | -- | No built-in sum/product. Use `reduce`. |
| **Gleam** | `list.fold(list, 0, fn(a, b) { a + b })` | No dedicated sum. |
| **Python** | `sum(iterable)`, `math.prod(iterable)` | Built-in `sum`, `prod` added in 3.8. |
| **Go** | -- | No stdlib sum/product. |
| **Ruby** | `Enumerable#sum` | `sum` exists. No `product`. |
| **TypeScript** | -- | No stdlib sum/product. |
| **Almide** | `list.sum`, `list.product` | Int-only. Separate `sum_float`, `product_float`. |

**Assessment:** `sum` and `product` are convenience shortcuts for common folds. They exist to make intent explicit and help LLMs write correct code faster. The Int/Float split (`sum` vs `sum_float`) is the real issue:

- Redundant naming: `sum_float`/`product_float` exist because `sum`/`product` are monomorphic (hardcoded to `List[Int]`).
- If Almide had type-class-like dispatch or overloading, a single `sum` could work on both `List[Int]` and `List[Float]`.
- Current approach is pragmatic but introduces naming inconsistency. Every other verb is generic; these are not.

**Verdict:** Keep `sum`/`product`. They serve a real purpose -- they are the most common aggregates, and spelling them as `fold(xs, 0, fn(a, b) => a + b)` is verbose enough to hurt LLM accuracy. The `_float` variants are a necessary wart until the type system supports numeric traits.

### min / max (collection)

| Language | Name | Notes |
|----------|------|-------|
| **Rust** | `Iterator::min`, `Iterator::max` | Returns `Option`. Also `min_by`, `min_by_key`. |
| **Kotlin** | `Iterable.min()`, `Iterable.max()` | Returns nullable. Also `minBy`, `maxBy`. |
| **Swift** | `Sequence.min()`, `Sequence.max()` | Returns optional. Also `min(by:)`. |
| **Gleam** | -- | No list.min/max. Use fold. |
| **Python** | `min(iterable)`, `max(iterable)` | Built-in. Raises on empty. Also `key=` parameter. |
| **Go** | `slices.Min`, `slices.Max` | Added in Go 1.21. |
| **Ruby** | `Enumerable#min`, `Enumerable#max` | Also `min_by`, `max_by`. |
| **TypeScript** | `Math.min(...arr)`, `Math.max(...arr)` | Spread required. No method. |
| **Almide** | `list.min`, `list.max` | Returns `Option[A]`. No `min_by`/`max_by` yet. |

**Gap:** Almide lacks `min_by` and `max_by`. These are commonly needed (e.g., find the shortest string, the cheapest item). Currently requires `sort_by(xs, f).first` or manual fold, both of which are O(n log n) or verbose.

### count

| Language | Name | Notes |
|----------|------|-------|
| **Rust** | `Iterator::count()` (arity 0), `Iterator::filter(f).count()` | No predicate-count. |
| **Kotlin** | `Iterable.count(predicate)` | Predicate-based. |
| **Swift** | `Sequence.count(where:)` (proposal), `.filter(f).count` | Proposal exists. |
| **Gleam** | -- | No count. Use fold. |
| **Python** | `list.count(value)` (value-based), `str.count(sub)` | Python list.count checks equality, not predicate. |
| **Go** | -- | No stdlib count. |
| **Ruby** | `Enumerable#count(value)` or `count { predicate }` | Both value and predicate forms. |
| **TypeScript** | `.filter(f).length` | No dedicated count. |
| **Almide** | `list.count(xs, f)` (predicate), `string.count(s, sub)` (substring) | **Semantic split.** |

See Section 4 for the `count` inconsistency analysis.

### any / all

| Language | Name | Notes |
|----------|------|-------|
| **Rust** | `Iterator::any(f)`, `Iterator::all(f)` | Predicate-based. |
| **Kotlin** | `Iterable.any(f)`, `Iterable.all(f)`, `Iterable.none(f)` | Also `none`. |
| **Swift** | `Sequence.contains(where:)`, `Sequence.allSatisfy(_:)` | Different names. |
| **Gleam** | `list.any(list, f)`, `list.all(list, f)` | Same as Almide. |
| **Python** | `any(iterable)`, `all(iterable)` | Built-in. Operate on truthy values, not predicates. |
| **Go** | -- | No stdlib any/all. |
| **Ruby** | `Enumerable#any?`, `Enumerable#all?`, `Enumerable#none?` | Also `none?`. |
| **TypeScript** | `Array.some(f)`, `Array.every(f)` | Different names: `some`/`every`. |
| **Almide** | `list.any(xs, f)`, `list.all(xs, f)` | Predicate-based. List only. |

**Almide uses `any`/`all`** which aligns with Rust, Gleam, Python, and Ruby. Good choices.

---

## 3. Gap Analysis: Which Modules Should Have Aggregate Verbs?

### Map

The `map` module currently has: `new`, `get`, `get_or`, `set`, `contains`, `remove`, `keys`, `values`, `len`, `entries`, `merge`, `is_empty`, `from_entries`, `from_list`, `map_values`, `filter`.

**Missing aggregate verbs:**

| Verb | Proposed Signature | Justification |
|------|-------------------|---------------|
| `fold` | `map.fold(m, init, fn(acc, k, v) => ...)` | Rust: `HashMap` has no fold, but `iter().fold()` is standard. Kotlin: `Map.entries.fold`. Gleam: `dict.fold`. Every functional language folds over maps. |
| `any` | `map.any(m, fn(k, v) => ...) -> Bool` | Kotlin: `Map.any(predicate)`. Ruby: `Hash#any?`. Common need: "does any entry satisfy?" |
| `all` | `map.all(m, fn(k, v) => ...) -> Bool` | Kotlin: `Map.all(predicate)`. Ruby: `Hash#all?`. Dual of `any`. |
| `count` | `map.count(m, fn(k, v) => ...) -> Int` | Kotlin: `Map.count(predicate)`. How many entries match? |
| `each` | `map.each(m, fn(k, v) => ...)` | Gleam: `dict.each`. Kotlin: `Map.forEach`. Side-effect iteration over entries. |
| `find` | `map.find(m, fn(k, v) => ...) -> Option[(K, V)]` | Kotlin: `Map.entries.find(predicate)`. Find first matching entry. |
| `reduce` | -- | Not recommended. Reduce requires homogeneous types; map entries are (K, V) pairs. `fold` covers this. |
| `min_by` / `max_by` | -- | Low priority. Can use `entries \|> list.min_by(...)`. |

**Priority:** `fold` > `each` > `any`/`all` > `count` > `find`.

`fold` is the most critical gap. Without it, the only way to aggregate a map is `map.entries(m) |> list.fold(...)`, which forces materialization of the full entry list and is non-obvious to LLMs.

`each` is important because `map.entries(m) |> list.each(fn((k, v)) => ...)` requires tuple destructuring in the lambda, which is error-prone.

### String

String currently has `count` (substring-based) but no predicate-based aggregates.

| Verb | Should add? | Reasoning |
|------|------------|-----------|
| `any` | No | `string.chars(s) \|> list.any(f)` is clear and rare enough. |
| `all` | No | Same reasoning. |
| `fold` | No | `string.chars(s) \|> list.fold(...)` is clear. |

Strings are not collections in Almide's type system; they decompose into `List[String]` (of single-char strings) via `chars`. Adding aggregate verbs to string would create confusion about what the iteration unit is (bytes? chars? grapheme clusters?). The explicit `chars` + list pipeline is better.

### Result

| Verb | Should add? | Reasoning |
|------|------------|-----------|
| `fold` | Maybe | `result.fold(r, fn(ok_val) => ..., fn(err_val) => ...)` -- Kotlin has this. It collapses both branches. Currently done via match expression. Low priority. |

---

## 4. The `count` Inconsistency

### The Problem

```
list.count(xs, fn(x) => x > 2)    // predicate: count matching elements
string.count("banana", "an")       // substring: count occurrences
```

Same verb name, fundamentally different semantics:

| | `list.count` | `string.count` |
|-|-------------|----------------|
| Second arg type | `Fn[A] -> Bool` | `String` |
| Semantics | "how many elements satisfy f?" | "how many times does sub appear?" |
| Analog in other languages | Kotlin `count(predicate)`, Ruby `count { block }` | Python `str.count(sub)`, Ruby `String#count(chars)` |

### Is This Actually a Problem?

**Argument that it's fine:**

1. The types are unambiguous. `list.count` takes a closure; `string.count` takes a string. A programmer (or LLM) cannot accidentally use one when they mean the other.
2. Python does exactly the same thing: `list(filter(pred, xs))` for predicate-count, `"banana".count("an")` for substring-count. Nobody complains.
3. The alternative names are worse: `string.occurrences("banana", "an")` is long and rarely used in any language. `string.count_occurrences` is even longer.

**Argument that it's a problem:**

1. Almide's design goal is LLM accuracy. If an LLM sees `count` used with a predicate in one context, it may try `string.count(s, fn(c) => c == "a")` -- which won't work.
2. The verb-reform analysis (Section 6) aims to freeze a consistent verb set. A verb that means different things in different modules undermines that goal.

### Recommendation

**Accept the inconsistency. Document it. Do not rename.**

Renaming `string.count` to `string.occurrences` would break the alignment with Python and Ruby (both use `count` for substring counting). Renaming `list.count` would break alignment with Kotlin and Ruby.

The type system prevents misuse at compile time. The LLM hint system can catch the mistake at the diagnostic level.

---

## 5. Should `sum`/`product` Exist, or Should Users Write `fold`?

### Comparison

```
// With dedicated sum
let total = list.sum(prices)

// With fold
let total = prices.fold(0, fn(acc, x) => acc + x)
```

### Arguments For Keeping `sum`/`product`

1. **LLM accuracy.** "Sum a list" is one of the most common operations. A dedicated verb is unambiguous. `fold(xs, 0, fn(a, b) => a + b)` has multiple failure modes: wrong init value, wrong arg order in lambda, accidentally using `*` instead of `+`.
2. **Readability.** `list.sum(xs)` reads as intent. `fold` reads as mechanism.
3. **Precedent.** Python, Kotlin, Ruby, Rust all have dedicated `sum`. Gleam (which doesn't) is the outlier.

### Arguments For Removing

1. **Monomorphic wart.** `sum` only works on `List[Int]`, requiring `sum_float` for `List[Float]`. This breaks the pattern where all list verbs are generic.
2. **Proliferation.** If `sum` exists, why not `average`? `median`? `variance`? Where does it stop?

### Verdict

**Keep `sum` and `product`.** They are the two most common numeric aggregates. The line is clear: only operations that are identity-element folds (`+`/0, `*`/1) get dedicated verbs. `average` requires division and is not a simple fold; it stays out.

The `_float` variants are an acceptable cost. If Almide later gains numeric type classes, `sum_float`/`product_float` can be deprecated in favor of a unified generic `sum`.

---

## 6. Additional Gaps Worth Noting

### `min_by` / `max_by`

Currently missing from `list`. Every language with `min`/`max` on collections also provides a key-function variant:

| Language | Name |
|----------|------|
| Rust | `Iterator::min_by_key(f)`, `Iterator::max_by_key(f)` |
| Kotlin | `Iterable.minBy(f)`, `Iterable.maxBy(f)` |
| Python | `min(iterable, key=f)`, `max(iterable, key=f)` |
| Ruby | `Enumerable#min_by`, `Enumerable#max_by` |

**Proposed:**

```
list.min_by(xs, fn(x) => ...) -> Option[A]
list.max_by(xs, fn(x) => ...) -> Option[A]
```

Without these, users must write `xs |> sort_by(f) |> first` which is O(n log n) instead of O(n) and returns `Option` anyway.

### `none` (complement of `any`)

Some languages (Kotlin, Ruby) provide `none(predicate)` as the dual of `any`. In Almide this is `!list.any(xs, f)`, which is clear enough. Not recommended to add.

### `fold_right`

Some languages (Kotlin, Haskell, Gleam) provide right-fold. Rarely needed in practice and can cause stack overflows on large lists. Not recommended.

---

## 7. Summary of Recommendations

### Add to Map (high priority)

| Verb | Priority | Signature |
|------|----------|-----------|
| `map.fold` | P0 | `(Map[K,V], B, Fn[B,K,V] -> B) -> B` |
| `map.each` | P0 | `(Map[K,V], Fn[K,V] -> Unit) -> Unit` |
| `map.any` | P1 | `(Map[K,V], Fn[K,V] -> Bool) -> Bool` |
| `map.all` | P1 | `(Map[K,V], Fn[K,V] -> Bool) -> Bool` |
| `map.count` | P2 | `(Map[K,V], Fn[K,V] -> Bool) -> Int` |
| `map.find` | P2 | `(Map[K,V], Fn[K,V] -> Bool) -> Option[(K,V)]` |

### Add to List (medium priority)

| Verb | Priority | Signature |
|------|----------|-----------|
| `list.min_by` | P1 | `(List[A], Fn[A] -> B) -> Option[A]` |
| `list.max_by` | P1 | `(List[A], Fn[A] -> B) -> Option[A]` |

### Keep As-Is

| Decision | Reasoning |
|----------|-----------|
| Keep `sum`, `product`, `sum_float`, `product_float` | Common operations; LLM accuracy matters more than elegance |
| Keep `count` semantic split (list=predicate, string=substring) | Type system prevents confusion; both conventions are standard |
| Do not add aggregate verbs to `string` | Strings decompose via `chars`; aggregation belongs on the resulting list |
| Do not add `none` | `!any(...)` is clear |
| Do not add `fold_right` | Rarely needed, stack-overflow risk |

### Open Question

Should `map.fold` callback take `(acc, key, value)` (3 args) or `(acc, (key, value))` (2 args, tuple)?

- 3 args: more ergonomic, matches `map.filter(m, fn(k, v) => ...)` which already uses 2 separate args.
- 2 args (tuple): consistent with `list.fold` which takes `Fn[B, A] -> B`.

**Recommendation:** Use 3 args `Fn[B, K, V] -> B` to match `map.filter`'s existing convention.
