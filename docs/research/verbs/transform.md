# Transform Verbs

Research document analyzing the five Transform verbs in Almide's stdlib:
`map`, `filter`, `filter_map`, `flat_map`, `flatten`.

---

## Current State in Almide

### Presence Matrix

| Verb | List | Map | Result | Option | String |
|------|------|-----|--------|--------|--------|
| `map` | yes | `map_values` | yes | **missing** | N/A |
| `filter` | yes | yes | **missing** | **missing** | N/A |
| `filter_map` | yes | **missing** | N/A | N/A | N/A |
| `flat_map` | yes | **missing** | `and_then` | **missing** | N/A |
| `flatten` | yes | N/A | **missing** | **missing** | N/A |

Notes:
- Option has no module at all. `option` appears in `PRELUDE_MODULES` but no `stdlib/defs/option.toml` exists and no bundled `.almd` source is present.
- Map's `map` is spelled `map_values`. No `map.map` exists.
- Result's `flat_map` is spelled `and_then`. No `result.flat_map` exists.
- Result has no `flatten`. `Result[Result[A, E], E] -> Result[A, E]` is not expressible.

---

## Verb-by-Verb Analysis

### 1. `map`

#### Current implementations

| Module | Function | Signature |
|--------|----------|-----------|
| list | `list.map(xs, f)` | `(List[A], Fn[A] -> B) -> List[B]` |
| map | `map.map_values(m, f)` | `(Map[K, V], Fn[V] -> B) -> Map[K, B]` |
| result | `result.map(r, f)` | `(Result[A, E], Fn[A] -> B) -> Result[B, E]` |
| option | -- | does not exist |

#### Cross-language comparison

| Language | List/Array | Map/Dict | Result/Either | Option/Maybe |
|----------|-----------|----------|---------------|-------------|
| **Rust** | `iter().map()` | `iter().map()` on entries | `.map()` | `.map()` |
| **Kotlin** | `.map()` | `.mapValues()`, `.map()` | `.map()` (Arrow) | `.let{}` / `?.` |
| **Swift** | `.map()` | `.mapValues()` | `.map()` | `.map()` |
| **Gleam** | `list.map()` | `dict.map_values()` | `result.map()` | `option.map()` |
| **Python** | `map()` / list comp | dict comp | N/A | N/A |
| **Go** | no stdlib; manual loop | manual loop | N/A | N/A |
| **Ruby** | `.map` | `.transform_values` | N/A | N/A |
| **TypeScript** | `.map()` | no native; manual | N/A | N/A |

Key observations:
- Every language with a `map` verb uses exactly that name for List.
- For Map containers, Kotlin and Swift both provide `mapValues` as a separate name because `map` on a Map applies to entries `(K, V)` and returns a different collection shape. Gleam follows the same pattern with `dict.map_values`.
- For Option/Result, every language that models them (Rust, Swift, Gleam) uses plain `map`.

#### Assessment

**list.map**: Correct. Keep.

**map.map_values**: The name `map_values` is a reasonable choice adopted by Kotlin, Swift, and Gleam. The question is whether Almide should also offer `map.map(m, f)` where `f` receives `(K, V)` and returns `(K2, V2)` (full entry mapping). The verb-reform roadmap plans to rename `map_values` to `map` and have the closure receive only the value. This is a simplification: in practice, most users want to transform values while keeping keys intact. If full entry mapping is needed, `entries().map(f) |> from_list()` covers it.

**Recommendation**: Add `map.map(m, f)` as the primary name (closure receives value only, same semantics as `map_values`). Deprecate `map_values` over time. This aligns with the principle "same verb, same meaning across all containers."

**result.map**: Correct. Keep.

**option.map**: Missing. Must be added when the `option` module is created. Signature: `(Option[A], Fn[A] -> B) -> Option[B]`.

---

### 2. `filter`

#### Current implementations

| Module | Function | Signature |
|--------|----------|-----------|
| list | `list.filter(xs, f)` | `(List[A], Fn[A] -> Bool) -> List[A]` |
| map | `map.filter(m, f)` | `(Map[K, V], Fn[K, V] -> Bool) -> Map[K, V]` |
| result | -- | N/A (filter makes no sense for a single-value wrapper) |
| option | -- | would be `filter(opt, f)`: `some(x)` -> if `f(x)` then `some(x)` else `none` |

#### Cross-language comparison

| Language | List/Array | Map/Dict | Option |
|----------|-----------|----------|--------|
| **Rust** | `iter().filter()` | `iter().filter()` on entries | `.filter()` |
| **Kotlin** | `.filter()` | `.filter()` | N/A |
| **Swift** | `.filter()` | `.filter()` | N/A |
| **Gleam** | `list.filter()` | `dict.filter()` | N/A |
| **Python** | `filter()` / list comp | dict comp | N/A |
| **Go** | manual | manual | N/A |
| **Ruby** | `.select` / `.filter` | `.select` / `.filter` | N/A |
| **TypeScript** | `.filter()` | no native | N/A |

Key observations:
- Universal name across all languages. No controversy.
- `Option.filter` exists in Rust and Scala but is absent from most other languages. It is useful but not essential.
- `Result.filter` does not exist in any surveyed language. It does not make semantic sense (what error would a failed filter produce?).

#### Assessment

**list.filter**: Correct. Keep.

**map.filter**: Correct. The closure receives `(K, V)` which is the right design for maps. Keep.

**option.filter**: Low priority. Rust has it, but most languages skip it. If added, the signature would be `(Option[A], Fn[A] -> Bool) -> Option[A]`. Can be deferred to post-1.0.

**result.filter**: Do not add. The concept is semantically unclear.

**Consistency note**: `list.filter(xs, f)` closure takes `A`, `map.filter(m, f)` closure takes `(K, V)`. This is intentional -- Map operates on entries. The verb-reform document confirms this design.

---

### 3. `filter_map`

#### Current implementations

| Module | Function | Signature |
|--------|----------|-----------|
| list | `list.filter_map(xs, f)` | `(List[A], Fn[A] -> Option[B]) -> List[B]` |
| map | -- | missing |
| result | N/A | N/A |
| option | N/A | N/A |

#### Cross-language comparison

| Language | List/Array | Map/Dict |
|----------|-----------|----------|
| **Rust** | `iter().filter_map()` | `iter().filter_map()` on entries |
| **Kotlin** | `.mapNotNull()` | `.mapNotNullValues()` (custom) |
| **Swift** | `.compactMap()` | `.compactMapValues()` |
| **Gleam** | `list.filter_map()` | N/A |
| **Python** | list comp with conditional | N/A |
| **Go** | manual | manual |
| **Ruby** | `.filter_map` | N/A |
| **TypeScript** | manual (`.map().filter()`) | N/A |

Key observations:
- Name varies: Rust/Gleam/Ruby use `filter_map`, Swift uses `compactMap`, Kotlin uses `mapNotNull`. Almide follows the Rust/Gleam convention.
- Map-level `filter_map` is less common. Swift has `compactMapValues` but most languages skip it. The operation can be composed: `m.entries().filter_map(f) |> map.from_list()`.
- `filter_map` only makes sense for collections, not for single-value wrappers (Option/Result).

#### Assessment

**list.filter_map**: Correct name, correct semantics. Keep.

**map.filter_map**: The verb-reform roadmap lists `filter_map` as a planned addition for Map in the "Category 1: Transform" table. Signature: `(Map[K, V], Fn[K, V] -> Option[(K2, V2)]) -> Map[K2, V2]` or the simpler form `(Map[K, V], Fn[V] -> Option[B]) -> Map[K, B]`. The simpler form (value-only closure, preserving keys) is more practical and matches the `map_values`-style convention. Add it.

**Recommendation**: Add `map.filter_map(m, f)` where `f: Fn[V] -> Option[B]`, returning `Map[K, B]` with `none` entries removed. This parallels `list.filter_map`.

---

### 4. `flat_map`

#### Current implementations

| Module | Function | Signature |
|--------|----------|-----------|
| list | `list.flat_map(xs, f)` | `(List[A], Fn[A] -> List[B]) -> List[B]` |
| map | -- | missing |
| result | `result.and_then(r, f)` | `(Result[A, E], Fn[A] -> Result[B, E]) -> Result[B, E]` |
| option | -- | missing |

#### Cross-language comparison

| Language | List/Array | Map/Dict | Result | Option |
|----------|-----------|----------|--------|--------|
| **Rust** | `iter().flat_map()` | N/A | `.and_then()` | `.and_then()` |
| **Kotlin** | `.flatMap()` | `.flatMap()` | `.flatMap()` (Arrow) | N/A |
| **Swift** | `.flatMap()` | N/A | `.flatMap()` | `.flatMap()` |
| **Gleam** | `list.flat_map()` | N/A | `result.try()` | `option.then()` |
| **Python** | list comp / chain | N/A | N/A | N/A |
| **Go** | manual | manual | N/A | N/A |
| **Ruby** | `.flat_map` | `.flat_map` | N/A | N/A |
| **TypeScript** | `.flatMap()` | N/A | N/A | N/A |

Key observations:
- `flat_map` (or `flatMap`) is the dominant name for List across all languages.
- For Result: Rust uses `and_then`, Kotlin/Swift use `flatMap`, Gleam uses `try`. There is a genuine split. Rust's `and_then` predates the `flat_map` naming trend.
- For Option: Rust uses `and_then`, Swift/Kotlin use `flatMap`, Gleam uses `then`.
- Map-level `flat_map` is uncommon. Kotlin has it but it maps entries to lists of entries, which is a niche operation.

#### The `result.and_then` vs `result.flat_map` question

This is the central naming tension for this verb:

**Arguments for keeping `and_then`**:
- Rust users expect it.
- Semantically, `and_then` reads well: "if ok, and then do this."
- It is already implemented and in use.

**Arguments for adding/preferring `flat_map`**:
- Cross-container consistency: `list.flat_map` + `result.flat_map` + `option.flat_map` all use the same verb for the same abstract operation (monadic bind).
- LLM learnability: one verb to learn, applicable everywhere.
- Modern trend: Kotlin, Swift, TypeScript all converge on `flatMap`.
- The verb-reform document already plans this.

**Recommendation**: Add `result.flat_map` as an alias for `result.and_then`. Both should work. Over time, documentation and examples should prefer `flat_map`. Do not remove `and_then` -- it is a valid Rust-ism and removing it would break existing code for no gain.

**option.flat_map**: Must be added when the `option` module is created. Signature: `(Option[A], Fn[A] -> Option[B]) -> Option[B]`.

**map.flat_map**: Low priority. The operation is "map each entry to a list of entries, then collect." This is niche. Composition via `entries().flat_map(f) |> from_list()` is sufficient. Defer.

---

### 5. `flatten`

#### Current implementations

| Module | Function | Signature |
|--------|----------|-----------|
| list | `list.flatten(xss)` | `(List[List[T]]) -> List[T]` |
| map | N/A | not applicable |
| result | -- | missing; would be `Result[Result[A, E], E] -> Result[A, E]` |
| option | -- | missing; would be `Option[Option[A]] -> Option[A]` |

#### Cross-language comparison

| Language | List/Array | Result | Option |
|----------|-----------|--------|--------|
| **Rust** | `iter().flatten()` | `.flatten()` (nightly/1.76) | `.flatten()` |
| **Kotlin** | `.flatten()` | N/A | N/A |
| **Swift** | `.joined()` | N/A | `.flatMap{$0}` idiom |
| **Gleam** | `list.flatten()` | N/A | N/A |
| **Python** | `itertools.chain.from_iterable()` | N/A | N/A |
| **Go** | manual | N/A | N/A |
| **Ruby** | `.flatten` | N/A | N/A |
| **TypeScript** | `.flat()` | N/A | N/A |

Key observations:
- `flatten` on List is universal. Swift is the outlier with `joined()`.
- `Option.flatten()` exists in Rust (stable) and is genuinely useful: collapsing `Option[Option[A]]` to `Option[A]`.
- `Result.flatten()` was stabilized in Rust 1.76 for `Result[Result[A, E], E]` where both error types must match. Useful but less common.
- Map flatten is not a meaningful concept.

#### Assessment

**list.flatten**: Correct. Keep.

**option.flatten**: Should be added when the `option` module is created. Signature: `(Option[Option[A]]) -> Option[A]`. Semantics: `some(some(x)) -> some(x)`, `some(none) -> none`, `none -> none`. This arises naturally when composing functions that return Option.

**result.flatten**: Should be added. Signature: `(Result[Result[A, E], E]) -> Result[A, E]`. Semantics: `ok(ok(x)) -> ok(x)`, `ok(err(e)) -> err(e)`, `err(e) -> err(e)`. Both error types must be the same. The verb-reform document already plans this. Note: the type checker must enforce that inner and outer error types unify.

---

## Gap Analysis

### Option module (does not exist yet)

The `option` module is the largest gap. It is listed in `PRELUDE_MODULES` but has no implementation. All surveyed languages with an Option type provide at minimum `map`, `flat_map`/`and_then`, and `unwrap_or`.

Required transform verbs for Option:

| Verb | Signature | Priority |
|------|-----------|----------|
| `map` | `(Option[A], Fn[A] -> B) -> Option[B]` | critical |
| `flat_map` | `(Option[A], Fn[A] -> Option[B]) -> Option[B]` | critical |
| `flatten` | `(Option[Option[A]]) -> Option[A]` | high |
| `filter` | `(Option[A], Fn[A] -> Bool) -> Option[A]` | low |

Non-transform verbs also needed (for completeness, not analyzed here):
`unwrap_or`, `unwrap_or_else`, `is_some`, `is_none`, `to_result`.

### Map module gaps

| Verb | Status | Priority |
|------|--------|----------|
| `map` (as alias for `map_values`) | planned | high |
| `filter_map` | missing | medium |
| `flat_map` | missing | low (niche) |

### Result module gaps

| Verb | Status | Priority |
|------|--------|----------|
| `flat_map` (as alias for `and_then`) | planned | high |
| `flatten` | missing | medium |

---

## Consistency Scorecard

How consistent is each verb across the four container types?

| Verb | Target state | Current state | Consistency |
|------|-------------|---------------|-------------|
| `map` | List + Map + Result + Option | List + Map(wrong name) + Result | 50% -- Map uses `map_values`, Option missing |
| `filter` | List + Map + Option(optional) | List + Map | 67% -- Option `filter` is low priority |
| `filter_map` | List + Map | List only | 50% -- Map missing |
| `flat_map` | List + Map(optional) + Result + Option | List + Result(wrong name) | 33% -- Result uses `and_then`, Map/Option missing |
| `flatten` | List + Result + Option | List only | 33% -- Result/Option missing |

---

## Recommendations Summary

### Immediate (pre-1.0)

| Action | Verb | Details |
|--------|------|---------|
| **Add alias** | `result.flat_map` | Same implementation as `and_then`. Both names work. |
| **Add alias** | `map.map` | Same implementation as `map_values`. Closure receives value only. |

### Next priority (Option module creation)

| Action | Verb | Details |
|--------|------|---------|
| **New function** | `option.map` | `(Option[A], Fn[A] -> B) -> Option[B]` |
| **New function** | `option.flat_map` | `(Option[A], Fn[A] -> Option[B]) -> Option[B]` |
| **New function** | `option.flatten` | `(Option[Option[A]]) -> Option[A]` |

### Medium priority

| Action | Verb | Details |
|--------|------|---------|
| **New function** | `result.flatten` | `(Result[Result[A, E], E]) -> Result[A, E]`. Requires type unification of inner/outer E. |
| **New function** | `map.filter_map` | `(Map[K, V], Fn[V] -> Option[B]) -> Map[K, B]` |

### Low priority / Defer

| Action | Verb | Details |
|--------|------|---------|
| Defer | `map.flat_map` | Niche. Composable via `entries().flat_map().from_list()`. |
| Defer | `option.filter` | Rust has it, most languages skip it. |

### Deprecation timeline

| Current name | Replacement | Phase |
|-------------|-------------|-------|
| `map.map_values` | `map.map` | Deprecate after `map.map` is added |
| `result.and_then` | `result.flat_map` | Keep both indefinitely (Rust-ism, non-harmful) |

---

## Design Decision: Should `result.and_then` and `result.flat_map` both exist?

**Yes, both should exist.** This is the one place where Almide intentionally provides two names for the same operation. The justification:

1. `flat_map` provides cross-container consistency (list/map/option/result all have `flat_map`).
2. `and_then` provides Rust-familiar ergonomics and reads well in imperative chains.
3. Neither name is wrong. They describe the same operation from different perspectives: `flat_map` emphasizes the "map then flatten" structure; `and_then` emphasizes sequential composition.
4. The cost of two aliases is near-zero (one TOML entry, one UFCS line). The cost of confusing Rust users by removing `and_then` is real.

Documentation should use `flat_map` as the canonical name but mention `and_then` as an alias.

## Design Decision: Is `map.map_values` the right name?

**No, but for understandable historical reasons.** The name `map_values` is borrowed from Kotlin and Gleam, where `Map.map` already operates on full entries. In Almide, however, there is no `map.map` that operates on entries -- `map_values` is the only transform operation. This means the `_values` suffix adds no disambiguating value; it just makes the name longer and breaks cross-container consistency.

**Plan**: Add `map.map(m, f)` with value-only closure semantics (identical to `map_values`). If full-entry mapping is ever needed, it should be a different name (e.g., `map.map_entries`), not bare `map`.

## Design Decision: Does Option need `map`, `flat_map`, `flatten`?

**Yes, all three.** Option is the second-most-used wrapper type after Result. Without these operations, users are forced into pattern matching for every Option transformation, which is verbose and error-prone. Every major language with an Option type (Rust, Swift, Kotlin/Arrow, Gleam, Scala, Haskell) provides at least `map` and `flat_map` on Option.

The `option` module should be created with these as day-one functions, not deferred.

`flatten` specifically arises when chaining operations that each return Option:
```almide
// Without flatten: nested Option
let name = map.get(config, "user")  // Option[Value]
  .map(fn(v) => json.as_string(v)) // Option[Option[String]] -- problem!

// With flatten:
let name = map.get(config, "user")
  .map(fn(v) => json.as_string(v))
  .flatten()                        // Option[String]

// Or equivalently with flat_map:
let name = map.get(config, "user")
  .flat_map(fn(v) => json.as_string(v)) // Option[String]
```

This pattern is common enough that `flatten` (and `flat_map` which is `map` then `flatten`) should be first-class.
