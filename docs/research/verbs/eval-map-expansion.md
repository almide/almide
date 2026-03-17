# Evaluation: Map Module Expansion (+6 functions)

Analysis of the proposed Map module additions in stdlib-1.0.md: `fold`, `each`, `any`,
`all`, `count`, `find`. Plus: the `map_values` -> `map` rename, `from_entries` removal,
and whether `flat_map`/`partition`/`group_by` belong on Map.

---

## Part 1: Per-Function Evaluation of +6 New Functions

### 1. `map.fold(m, init, f) -> B`

**Proposed callback: `f(acc, k, v) -> B`**

**Needed: Yes, strongly.** Fold is the universal aggregation primitive. Without it, the
only way to aggregate a Map is `map.entries(m) |> list.fold(init, fn(acc, (k, v)) => ...)`,
which requires tuple destructuring in the callback and an intermediate List allocation.
Real-world evidence already exists in the codebase: `stdlib/url.almd` line 169 does
`list.fold(pairs, init, (acc, pair) => { ... map.set(acc, k, v) ... })` -- building a Map
inside a fold. With `map.fold`, many of these patterns would be direct.

**LLM frequency: High.** Fold is the go-to for any Map -> scalar or Map -> different-shape
conversion. Config aggregation, counting, building summaries -- all fold territory. LLMs
writing Almide will reach for this constantly.

**Callback signature: `(acc, k, v) -> B` is correct.**

Cross-language comparison:

| Language | Fold-over-Map callback | Notes |
|----------|----------------------|-------|
| **Rust** | `iter().fold(init, \|acc, (k, v)\| ...)` | 3-arg via tuple destructure |
| **Kotlin** | `entries.fold(init) { acc, (k, v) -> ... }` | 3-arg via destructure. Also has `fold` extension on Map directly. |
| **Python** | `functools.reduce(f, d.items(), init)` | f receives (acc, (k,v)) tuple |
| **Go** | `for k, v := range m { acc = f(acc, k, v) }` | Imperative, but 3 variables |
| **TypeScript** | No native Map.fold. Workaround: `Array.from(m.entries()).reduce((acc, [k,v]) => ...)` | 3-arg via destructure |
| **Gleam** | `dict.fold(d, init, fn(acc, k, v) { ... })` | 3-arg, exactly this signature |
| **Haskell** | `Map.foldlWithKey' (\acc k v -> ...) init m` | 3-arg |
| **Elm** | `Dict.foldl (\k v acc -> ...)` | 3-arg (different param order) |

Every language that supports Map fold uses 3 callback arguments. The only question is
parameter order. `(acc, k, v)` matches `list.fold`'s pattern of acc-first, which is the
right choice for Almide consistency: `list.fold(xs, init, fn(acc, x) => ...)` extends
naturally to `map.fold(m, init, fn(acc, k, v) => ...)`.

**Verdict: Ship as specified. No changes needed.**

---

### 2. `map.each(m, f) -> Unit`

**Proposed callback: `f(k, v) -> Unit`**

**Needed: Yes.** Side-effect iteration over maps is a basic operation. Without `each`,
users must write `for (k, v) in map.entries(m) { ... }`, which works but is less
composable (cannot be piped) and allocates an intermediate list of entries.

The codebase already has `for (k, v) in map.entries(m)` patterns in:
- `spec/stdlib/stdlib-test.almd:604`
- `spec/integration/codegen_patterns_test.almd:85`

These would not need to change (for-in is fine), but `each` enables the pipeline style:
`m |> map.each(fn(k, v) => println("${k}: ${v}"))`.

**LLM frequency: Medium.** Less than fold because for-in loops are often more natural for
side effects. But important for pipeline-heavy code.

**Callback signature: `(k, v) -> Unit` is correct.**

| Language | Iteration verb | Callback |
|----------|---------------|----------|
| **Rust** | `for (k, v) in &map { ... }` | k, v |
| **Kotlin** | `map.forEach { (k, v) -> ... }` | k, v |
| **Python** | `for k, v in d.items(): ...` | k, v |
| **Go** | `for k, v := range m { ... }` | k, v |
| **TypeScript** | `map.forEach((v, k) => ...)` | v, k (reversed!) |
| **Gleam** | `dict.each(d, fn(k, v) { ... })` | k, v |
| **Ruby** | `hash.each { \|k, v\| ... }` | k, v |

Every language except TypeScript's `Map.forEach` uses (k, v) order. TypeScript's reversal
is widely considered a design mistake (it matches Array.forEach's (value, index) but feels
wrong for Maps). Almide should use `(k, v)`, matching `map.filter`'s existing callback.

**Note on consistency with `list.each`:** `list.each(xs, fn(x) => ...)` takes one arg.
`map.each(m, fn(k, v) => ...)` takes two. This is the same divergence as `list.filter` vs
`map.filter` and is justified by the same reasoning: Map entries are inherently key-value
pairs. The verb `each` means "visit every element" -- for a Map, an element is a (k, v)
pair.

**Verdict: Ship as specified. No changes needed.**

---

### 3. `map.any(m, f) -> Bool`

**Proposed callback: `f(k, v) -> Bool`**

**Needed: Yes.** "Does any entry satisfy a condition?" is a fundamental query. Without it:
`map.entries(m) |> list.any(fn((k, v)) => ...)` -- requires tuple destructure in a callback,
which is syntactically heavier and allocates an intermediate list.

**LLM frequency: Medium.** Common in validation logic ("does any config value exceed a
threshold?"), access control ("does any role grant this permission?"), and data quality
checks.

**Callback signature: `(k, v) -> Bool` is correct.**

| Language | Any-over-Map | Callback |
|----------|-------------|----------|
| **Rust** | `map.iter().any(\|(k, v)\| ...)` | k, v via destructure |
| **Kotlin** | `map.any { (k, v) -> ... }` | k, v |
| **Python** | `any(f(k, v) for k, v in d.items())` | k, v |
| **Go** | Manual loop | k, v |
| **TypeScript** | No native `Map.some`. `Array.from(m.entries()).some(([k,v]) => ...)` | k, v |
| **Gleam** | No `dict.any` (must fold) | N/A |

Kotlin is the strongest precedent here with `map.any { (k, v) -> bool }`. The callback
shape matches `map.filter` and `map.each`, maintaining internal consistency.

**Verdict: Ship as specified.**

---

### 4. `map.all(m, f) -> Bool`

**Proposed callback: `f(k, v) -> Bool`**

**Needed: Yes.** The dual of `any`. "Do all entries satisfy a condition?" If you have `any`,
you need `all` -- every language that provides one provides the other.

**LLM frequency: Medium.** Same use cases as `any` but for universal quantification.
Schema validation ("all values are non-empty"), invariant checking.

**Callback signature: `(k, v) -> Bool` -- same analysis as `any`.**

| Language | All-over-Map |
|----------|-------------|
| **Rust** | `map.iter().all(\|(k, v)\| ...)` |
| **Kotlin** | `map.all { (k, v) -> ... }` |
| **Python** | `all(f(k, v) for k, v in d.items())` |

**Verdict: Ship as specified. Symmetric with `any`.**

---

### 5. `map.count(m, f) -> Int`

**Proposed callback: `f(k, v) -> Bool`**

**Needed: Yes, but lower priority than the others.** Count is expressible as
`map.fold(m, 0, fn(acc, k, v) => if f(k, v) then acc + 1 else acc)` or
`map.entries(m) |> list.count(fn((k, v)) => ...)`. Having a dedicated `count` is a
convenience, not a necessity.

However, `list.count` exists, and the design principle "Map is a first-class collection"
(principle 8 in the spec) argues for parity.

**LLM frequency: Low-Medium.** "How many entries have value > threshold?" is a real
question, but less common than fold/any/all. LLMs will use it in analytics-style code.

**Callback signature: `(k, v) -> Bool` is correct.** Matches `any`/`all`/`filter`.

| Language | Count-over-Map |
|----------|---------------|
| **Rust** | `map.iter().filter(\|(k, v)\| ...).count()` |
| **Kotlin** | `map.count { (k, v) -> ... }` |
| **Python** | `sum(1 for k, v in d.items() if f(k, v))` |

Kotlin provides `count` directly on Map. Rust does it via chaining. Either way, the
`(k, v) -> Bool` callback is standard.

**Verdict: Ship as specified. Keep for collection parity with list.count.**

---

### 6. `map.find(m, f) -> Option[(K, V)]`

**Proposed callback: `f(k, v) -> Bool`**

**Needed: Yes.** Finding the first entry matching a predicate is a core access pattern.
The return type `Option[(K, V)]` is correct -- unlike list.find which returns the element,
map.find must return the full entry because a value alone loses its key identity.

**LLM frequency: Medium.** "Find the entry where ..." is common in config lookup, data
search, and key discovery patterns.

**Callback signature: `(k, v) -> Bool` is correct.**

| Language | Find-over-Map | Returns |
|----------|--------------|---------|
| **Rust** | `map.iter().find(\|(k, v)\| ...)` | `Option<(&K, &V)>` |
| **Kotlin** | `map.entries.find { (k, v) -> ... }` | `Map.Entry<K, V>?` |
| **Python** | `next((k, v) for k, v in d.items() if f(k, v), None)` | tuple or None |

All return the full entry (key + value), which aligns with the proposed `Option[(K, V)]`.

**Ordering caveat:** Map iteration order in Almide is sorted by key (see `map.keys`
description: "Get all keys as a sorted list"). This means `find` returns the first entry
in key-sorted order that matches, which is deterministic. This is good -- it avoids the
"which one did you find?" ambiguity of hash-map iteration in Rust/Go/Python.

**Verdict: Ship as specified.**

---

## Part 2: `map.map_values` -> `map.map` Rename

### Should `f` be `(v) -> V2` or `(k, v) -> (K2, V2)`?

**Verdict: `(v) -> V2` is correct. Do NOT change to `(k, v) -> (K2, V2)`.**

This question was already resolved in `eval-naming-conventions.md` (Q2). Summarizing the
key arguments:

1. **Cross-container verb consistency.** `list.map(xs, fn(x) => ...)`,
   `option.map(o, fn(x) => ...)`, `result.map(r, fn(x) => ...)` all pass the "inner value."
   `map.map(m, fn(v) => ...)` extends this rule: map always receives the content of the
   container. For Map, keys are structure (indexes), values are content.

2. **80/20 rule.** Most Map transforms only touch values. `(k, v) -> (K2, V2)` would require
   users to return a tuple even when keys don't change, which is the majority case:
   ```
   // With (v) -> V2 (proposed):
   m |> map.map(fn(v) => v * 2)

   // With (k, v) -> (K2, V2) (rejected):
   m |> map.map(fn(k, v) => (k, v * 2))  // boilerplate: must return k unchanged
   ```

3. **When full-entry mapping is needed:** Use `map.entries(m) |> list.map(fn((k, v)) => ...) |> map.from_list`.
   This is explicit and rare enough to not warrant a dedicated verb.

4. **`(k, v) -> V2` (key available but only value returned) is a middle ground** used by
   Gleam's `dict.map_values` and Elm's `Dict.map`. This is tempting but violates the
   Almide principle that `map`'s callback shape matches across containers. If Map's `map`
   callback were `(k, v) -> V2`, the callback arity would differ from `list.map`,
   `option.map`, and `result.map`, making LLM generation less predictable.

**The rename from `map_values` to `map` is purely cosmetic** -- same `(v) -> V2` callback,
same behavior, shorter name that aligns with the universal `map` verb. This is the right call.

---

## Part 3: `map.from_entries` Removal

### Is `map.from_list` really better?

**Context:** The current codebase has two constructors:
- `map.from_entries(pairs)`: takes `List[(K, V)]`, no callback. Direct conversion.
- `map.from_list(xs, f)`: takes `List[A]` + `Fn[A] -> (K, V)`, applies f to build entries.

The spec proposes removing `from_entries` and keeping only `from_list`. But the current
`from_list` takes a callback -- it is not a drop-in replacement for `from_entries`.

**This is a problem.** The spec says `from_list(pairs) -> Map[K, V]` without a callback,
but the TOML definition shows `from_list(xs, f)` with a callback. These are two different
functions. The spec needs to clarify which one `from_list` is.

**Analysis of what's needed:**

1. **Direct construction from pairs** (`from_entries` semantics): Used in 4 places in the
   codebase (`spec/integration/codegen_ownership_test.almd:135`,
   `spec/lang/edge_cases_test.almd:241`, `spec/stdlib/map_generic_test.almd:15`,
   `spec/integration/codegen_pipes_test.almd:45`). This is a fundamental constructor.
   You cannot remove it.

2. **Construction with a mapper** (`from_list` semantics): Useful for `["alice", "bob"] |>
   map.from_list(fn(name) => (name, string.len(name)))`. Convenience, expressible as
   `list.map(xs, f) |> map.from_entries`.

**Recommendation:**

Option A (simplest): Keep `from_entries` as is. Remove `from_list` (it is sugar for
`list.map + from_entries`). This is the Go/Python philosophy: one obvious way.

Option B (rename): Rename `from_entries` to `from_list` (same signature: `List[(K, V)] ->
Map[K, V]`). Drop the callback variant. The name `from_list` is arguably better because it
aligns with the input type (`List`), while `from_entries` is Java/JS terminology. But this
is a naming-only change -- the function is identical.

Option C (keep both under `from_list` overload): Have `from_list` accept either
`List[(K, V)]` directly or `List[A]` + `Fn[A] -> (K, V)`. This requires function overloading
or a different dispatch mechanism, which may not exist in Almide's type system.

**Verdict: Option B is best.** Rename `from_entries` to `from_list` with signature
`from_list(pairs: List[(K, V)]) -> Map[K, V]` (no callback). Drop the callback variant.
The callback version is `list.map(xs, f) |> map.from_list` -- perfectly readable and not
worth the extra function.

But the spec as written is ambiguous on this point and needs to be explicit about the
callback removal.

---

## Part 4: Should Map Have `flat_map`? `partition`? `group_by`?

### `map.flat_map`

**Verdict: No. Do not add.**

`flat_map` on Map would mean: for each (k, v), produce zero or more (K2, V2) entries and
merge them. This is rarely useful and semantically confusing -- what happens when two
callbacks produce the same key?

| Language | Map.flatMap? | Notes |
|----------|-------------|-------|
| **Rust** | No | `flat_map` is on Iterator, not HashMap specifically |
| **Kotlin** | Yes (`flatMap`) | Returns `List<R>`, not `Map` -- it escapes the Map type |
| **Python** | No | |
| **Go** | No | |
| **Swift** | `compactMapValues` only | Filters nil values, not true flat_map |
| **Gleam** | No | |

Kotlin's `Map.flatMap` returns a `List`, not a `Map`. This is telling -- even Kotlin,
which adds extensions aggressively, doesn't define a Map -> Map flat_map because the
semantics are unclear.

If a user needs this, they can do:
```
map.entries(m)
|> list.flat_map(fn((k, v)) => make_entries(k, v))
|> map.from_list
```

**LLM frequency: Very low.** This pattern almost never appears in real code.

### `map.partition`

**Verdict: Not now, but reasonable for future.**

`map.partition(m, f) -> (Map[K, V], Map[K, V])` would split a map into two based on a
predicate. This is the Map analog of `list.partition`.

| Language | Map.partition? |
|----------|---------------|
| **Rust** | No (but `Iterator::partition` exists) |
| **Kotlin** | No native, but extension libraries have it |
| **Python** | No |
| **Ruby** | `Hash#partition` -- returns array of arrays, not hashes |
| **Gleam** | No |

Usage frequency is low. The workaround is straightforward:
```
let yes = map.filter(m, fn(k, v) => pred(k, v))
let no = map.filter(m, fn(k, v) => not pred(k, v))
```

This does iterate twice, but Map sizes in typical Almide programs are small. If partition
is added later, the callback should be `(k, v) -> Bool` matching `filter`.

**LLM frequency: Low.** Partition on lists is uncommon enough; partition on maps is rarer.

### `map.group_by`

**Verdict: No. This does not belong on Map.**

`group_by` takes a collection and groups elements by a key function into a
`Map[K, List[V]]`. The input is a flat collection (List), the output is a Map. It is a
List -> Map operation, which is why it lives on `list`:

```
list.group_by(xs, fn(x) => key_of(x))  // List[A] -> Map[B, List[A]]
```

What would `map.group_by` even mean? Group the entries of a Map by some derived key?
```
map.group_by(m, fn(k, v) => category(v))  // Map[K,V] -> Map[C, List[(K,V)]]
```

This is expressible as:
```
map.entries(m) |> list.group_by(fn((k, v)) => category(v))
```

No language provides `group_by` on Map directly. It is universally a List/Iterable
operation.

**LLM frequency: Near zero for a Map-native group_by.**

---

## Part 5: Summary Verdicts

### New functions: all 6 approved

| Function | Needed? | Callback | Notes |
|----------|---------|----------|-------|
| `fold(m, init, f)` | **Strong yes** | `(acc, k, v) -> B` | Universal aggregation primitive. Highest priority. |
| `each(m, f)` | **Yes** | `(k, v) -> Unit` | Pipeline-friendly side-effect iteration. |
| `any(m, f)` | **Yes** | `(k, v) -> Bool` | Existential quantification. |
| `all(m, f)` | **Yes** | `(k, v) -> Bool` | Universal quantification. Symmetric with `any`. |
| `count(m, f)` | **Yes** (lower priority) | `(k, v) -> Bool` | Collection parity with list. Expressible via fold. |
| `find(m, f)` | **Yes** | `(k, v) -> Bool`, returns `Option[(K, V)]` | Entry search. Return type includes key. |

### Callback consistency matrix

| Verb | list callback | map callback | Consistent? |
|------|--------------|--------------|-------------|
| `map` | `(a) -> B` | `(v) -> V2` | Yes -- inner value |
| `filter` | `(a) -> Bool` | `(k, v) -> Bool` | Yes -- full element |
| `fold` | `(acc, a) -> B` | `(acc, k, v) -> B` | Yes -- acc + full element |
| `any` | `(a) -> Bool` | `(k, v) -> Bool` | Yes -- full element |
| `all` | `(a) -> Bool` | `(k, v) -> Bool` | Yes -- full element |
| `count` | `(a) -> Bool` | `(k, v) -> Bool` | Yes -- full element |
| `each` | `(a) -> Unit` | `(k, v) -> Unit` | Yes -- full element |
| `find` | `(a) -> Bool` | `(k, v) -> Bool` | Yes -- full element |

The pattern is clean: `map` (the verb) takes the inner value only. All other verbs take
the full element -- for List that is `a`, for Map that is `(k, v)`. This is because `map`
(the verb) transforms content while preserving structure, whereas `filter`/`fold`/`any`/etc.
inspect the full element to make decisions.

### Rename/removal verdicts

| Change | Verdict |
|--------|---------|
| `map_values` -> `map` | **Approved.** `(v) -> V2` callback, no behavior change. |
| `from_entries` removal | **Clarification needed.** Rename to `from_list` (no callback) is correct. Drop the callback variant. Spec text is ambiguous. |

### Functions NOT to add

| Function | Verdict | Reason |
|----------|---------|--------|
| `flat_map` | **No** | Unclear semantics (key collision). Kotlin's version returns List, not Map. |
| `partition` | **Not now** | Low frequency. Two `filter` calls suffice. Revisit if demanded. |
| `group_by` | **No** | List -> Map operation. Already lives on `list`. |

### Implementation priority

1. **fold** -- unblocks the most patterns (aggregation, building other structures)
2. **each** -- most commonly needed side-effect operation
3. **find** -- entry lookup by predicate, no workaround besides entries+list.find
4. **any/all** -- pair, implement together
5. **count** -- lowest priority, expressible via fold
