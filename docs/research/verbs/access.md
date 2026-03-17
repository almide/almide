# Access Verbs

Analysis of access/lookup verbs across Almide's stdlib: `get`, `get_or`, `find`, `find_index`, `index_of`, `first`, `last`, `len`, `char_at`.

---

## 1. Current State in Almide

### `get`

| Module | Signature | Returns | Semantics |
|--------|-----------|---------|-----------|
| list | `get(xs: List[A], i: Int)` | `Option[A]` | Element at index |
| map | `get(m: Map[K,V], key: K)` | `Option[V]` | Value for key |
| json | `get(j: Value, key: String)` | `Option[Value]` | Nested value by key |
| env | `get(name: String)` | `Option[String]` | Env variable by name |
| http | `get(url: String)` | `Result[String, String]` | HTTP GET request (effect) |

**Consistency**: list, map, json, env share the same mental model: "look up by identifier, return Option if missing." http.get is a different verb entirely (HTTP method, not lookup). This is acceptable because the domain context makes it unambiguous.

### `get_or`

| Module | Signature | Returns | Semantics |
|--------|-----------|---------|-----------|
| list | `get_or(xs: List[A], i: Int, default: A)` | `A` | Element at index, or default |
| map | `get_or(m: Map[K,V], key: K, default: V)` | `V` | Value for key, or default |

**Gaps**: json and string lack `get_or`. For json this makes sense (typed accessors like `get_string` serve a different pattern). For string, `get_or` would be `char_at` with a default.

### `find`

| Module | Signature | Returns | Semantics |
|--------|-----------|---------|-----------|
| list | `find(xs: List[A], f: Fn[A] -> Bool)` | `Option[A]` | First element matching predicate |

**Single-module verb.** This is correct: `find` with a predicate is meaningful on sequences but not on maps or strings in their current API shape.

### `find_index`

| Module | Signature | Returns | Semantics |
|--------|-----------|---------|-----------|
| list | `find_index(xs: List[A], f: Fn[A] -> Bool)` | `Option[Int]` | First index where predicate holds |

**Single-module verb.** Complementary to `find`: returns position instead of element.

### `index_of`

| Module | Signature | Returns | Semantics |
|--------|-----------|---------|-----------|
| list | `index_of(xs: List[A], x: A)` | `Option[Int]` | First index of element (by value) |
| string | `index_of(s: String, needle: String)` | `Option[Int]` | First index of substring |

**Consistent.** Both return `Option[Int]` and search by value equality. The distinction from `find_index` is clear: `index_of` takes a value, `find_index` takes a predicate.

String also has `last_index_of` (no list counterpart).

### `first`

| Module | Signature | Returns | Semantics |
|--------|-----------|---------|-----------|
| list | `first(xs: List[A])` | `Option[A]` | First element, or none if empty |

**Single-module verb.** Equivalent to `get(xs, 0)` but more readable.

### `last`

| Module | Signature | Returns | Semantics |
|--------|-----------|---------|-----------|
| list | `last(xs: List[A])` | `Option[A]` | Last element, or none if empty |

**Single-module verb.** Equivalent to `get(xs, len(xs) - 1)`.

### `len`

| Module | Signature | Returns | Semantics |
|--------|-----------|---------|-----------|
| list | `len(xs: List[A])` | `Int` | Number of elements |
| map | `len(m: Map[K,V])` | `Int` | Number of entries |
| string | `len(s: String)` | `Int` | Number of characters |

**Consistent.** All three return `Int` and mean "size of this container." The semantics of string.len are character count (not byte count), which aligns with the user-facing model.

### `char_at`

| Module | Signature | Returns | Semantics |
|--------|-----------|---------|-----------|
| string | `char_at(s: String, i: Int)` | `Option[String]` | Character at index |

**Single-module verb.** This is the string analogue of `list.get(xs, i)`.

### `char_count`

| Module | Signature | Returns | Semantics |
|--------|-----------|---------|-----------|
| string | `char_count(s: String)` | `Int` | Number of Unicode characters |

**Redundant with `string.len`.** Both count Unicode characters (not bytes). Already flagged for deprecation in verb-reform-analysis.md.

---

## 2. Cross-Language Comparison

### `get` (indexed/keyed access)

| Language | List/Array | Map/Dict | String |
|----------|-----------|----------|--------|
| **Rust** | `v.get(i) -> Option<&T>` | `m.get(&k) -> Option<&V>` | `s.get(range)` (byte range, not char) |
| **Kotlin** | `list.get(i)` / `list[i]` | `map.get(k)` / `map[k]` | `s.get(i)` / `s[i]` returns Char |
| **Swift** | `arr[i]` (crashes on OOB) | `dict[k] -> V?` | `s[s.index(s.startIndex, offsetBy: i)]` |
| **Gleam** | no builtin; `list.at(l, i)` | `map.get(m, k) -> Result` | no builtin |
| **Python** | `lst[i]` (IndexError on OOB) | `d[k]` (KeyError) / `d.get(k)` | `s[i]` (IndexError) |
| **Go** | `a[i]` (panic on OOB) | `m[k]` (zero value + ok bool) | `s[i]` (byte, not rune) |
| **Ruby** | `a[i]` / `a.fetch(i)` | `h[k]` / `h.fetch(k)` | `s[i]` returns String or nil |
| **TypeScript** | `a[i]` (undefined on OOB) | `m.get(k)` (Map) / `o[k]` (object) | `s[i]` / `s.charAt(i)` |

**Key insight**: Most languages use the same verb or operator for list and map access. String access varies more because of encoding concerns. Almide's `list.get` and `map.get` are perfectly aligned with Rust, Kotlin, and TypeScript's `Map.get`. The absence of `string.get` is notable.

### `get_or` (access with default)

| Language | List/Array | Map/Dict |
|----------|-----------|----------|
| **Rust** | no direct equivalent (pattern match or `.unwrap_or()` on Option) | `.get(&k).unwrap_or(&default)` |
| **Kotlin** | `list.getOrElse(i) { default }` | `map.getOrDefault(k, default)` |
| **Swift** | no direct equivalent | `dict[k, default: v]` |
| **Gleam** | no builtin | no builtin (use `result.unwrap`) |
| **Python** | no direct equivalent | `d.get(k, default)` |
| **Go** | no direct equivalent | `m[k]` returns zero value (implicit default) |
| **Ruby** | `a.fetch(i, default)` | `h.fetch(k, default)` |
| **TypeScript** | no direct equivalent | no direct equivalent (use `??`) |

**Key insight**: `get_or` on maps is well-established (Python `dict.get`, Kotlin `getOrDefault`). On lists it is rarer but useful. Almide provides both, which is a superset of what most languages offer.

### `find` (predicate search)

| Language | Name | Returns |
|----------|------|---------|
| **Rust** | `iter.find(pred)` | `Option<&T>` |
| **Kotlin** | `list.find(pred)` | `T?` |
| **Swift** | `arr.first(where: pred)` | `T?` |
| **Gleam** | `list.find(l, pred)` | `Result(a, Nil)` |
| **Python** | `next(x for x in lst if pred(x))` (no built-in) | raises StopIteration |
| **Go** | `slices.IndexFunc(s, pred)` (returns index, not element) | `int` |
| **Ruby** | `arr.find(pred)` / `arr.detect(pred)` | element or nil |
| **TypeScript** | `arr.find(pred)` | `T \| undefined` |

**Key insight**: `find` as predicate search on lists is universal. Almide's `list.find` matches Rust, Kotlin, Gleam, Ruby, and TypeScript exactly.

### `find_index` (predicate position search)

| Language | Name | Returns |
|----------|------|---------|
| **Rust** | `iter.position(pred)` | `Option<usize>` |
| **Kotlin** | `list.indexOfFirst(pred)` | `Int` (-1 if not found) |
| **Swift** | `arr.firstIndex(where: pred)` | `Int?` |
| **Gleam** | no builtin | - |
| **Python** | no builtin (use `next(i for i, x ...)`) | - |
| **Go** | `slices.IndexFunc(s, pred)` | `int` (-1 if not found) |
| **Ruby** | `arr.index { pred }` | Integer or nil |
| **TypeScript** | `arr.findIndex(pred)` | `number` (-1 if not found) |

**Key insight**: Almide uses `find_index` matching TypeScript exactly. Rust calls it `position`, Kotlin calls it `indexOfFirst`. Almide's choice is the most readable for LLMs and non-expert programmers. Returning `Option[Int]` instead of `-1` is superior (follows the Almide pattern of explicit optionality).

### `index_of` (value position search)

| Language | List | String |
|----------|------|--------|
| **Rust** | `iter.position(\|x\| x == &val)` | `s.find(pat) -> Option<usize>` |
| **Kotlin** | `list.indexOf(v)` | `s.indexOf(sub)` |
| **Swift** | `arr.firstIndex(of: v)` | `s.range(of: sub)` |
| **Gleam** | no builtin | no builtin |
| **Python** | `lst.index(v)` | `s.index(sub)` / `s.find(sub)` |
| **Go** | `slices.Index(s, v)` | `strings.Index(s, sub)` |
| **Ruby** | `arr.index(v)` | `s.index(sub)` |
| **TypeScript** | `arr.indexOf(v)` | `s.indexOf(sub)` |

**Key insight**: `index_of` is the overwhelmingly standard name (Kotlin, TypeScript, Ruby, Python). Almide's naming aligns perfectly. Having it on both list and string is consistent with every language surveyed.

### `first` / `last`

| Language | First | Last |
|----------|-------|------|
| **Rust** | `iter.next()` / `slice.first()` | `slice.last()` |
| **Kotlin** | `list.first()` (throws), `firstOrNull()` | `list.last()` / `lastOrNull()` |
| **Swift** | `arr.first` (optional) | `arr.last` (optional) |
| **Gleam** | `list.first(l) -> Result` | `list.last(l) -> Result` |
| **Python** | `lst[0]` (IndexError) | `lst[-1]` (IndexError) |
| **Go** | `s[0]` (panic) | `s[len(s)-1]` (panic) |
| **Ruby** | `arr.first` | `arr.last` |
| **TypeScript** | `arr[0]` / `arr.at(0)` | `arr[arr.length-1]` / `arr.at(-1)` |

**Key insight**: `first` and `last` as explicit functions (rather than indexing) are standard in Rust, Kotlin, Swift, Gleam, Ruby. Almide returning `Option[A]` follows the Rust/Swift/Gleam pattern of safe access.

### `len` (size)

| Language | List | Map | String |
|----------|------|-----|--------|
| **Rust** | `.len()` | `.len()` | `.len()` (bytes!) / `.chars().count()` (chars) |
| **Kotlin** | `.size` | `.size` | `.length` |
| **Swift** | `.count` | `.count` | `.count` |
| **Gleam** | `list.length(l)` | `map.size(m)` | `string.length(s)` |
| **Python** | `len(lst)` | `len(d)` | `len(s)` |
| **Go** | `len(s)` | `len(m)` | `len(s)` (bytes!) / `utf8.RuneCountInString(s)` (runes) |
| **Ruby** | `.length` / `.size` | `.length` / `.size` | `.length` / `.size` |
| **TypeScript** | `.length` | `.size` (Map) | `.length` |

**Key insight**: Python and Go use the same function name across all types. Kotlin and Gleam use different names. Almide's choice to unify on `len` across list/map/string is the strongest consistency position, matching Python and Go. The fact that `string.len` counts characters (not bytes) is correct for Almide's model.

### `char_at` (string character access)

| Language | Function | Returns |
|----------|----------|---------|
| **Rust** | `s.chars().nth(i)` | `Option<char>` |
| **Kotlin** | `s[i]` / `s.get(i)` | `Char` (throws on OOB) |
| **Swift** | `s[s.index(s.startIndex, offsetBy: i)]` | `Character` (traps on OOB) |
| **Gleam** | no builtin (slice) | - |
| **Python** | `s[i]` | `str` (IndexError on OOB) |
| **Go** | `[]rune(s)[i]` | `rune` |
| **Ruby** | `s[i]` | `String` or nil |
| **TypeScript** | `s.charAt(i)` / `s[i]` | `string` (empty string on OOB) |

**Key insight**: TypeScript uses `charAt` which directly maps to Almide's `char_at`. Kotlin uniquely uses `get(i)` for string character access. Most languages use indexing operators.

---

## 3. Consistency Assessment

### Does `get` mean the same thing across modules?

| Module | `get` means | Consistent? |
|--------|-------------|-------------|
| list | "element at index" | Baseline |
| map | "value for key" | Yes -- keyed lookup |
| json | "nested value by key" | Yes -- keyed lookup on tree |
| env | "env variable by name" | Yes -- keyed lookup on env |
| http | "HTTP GET request" | **Different domain** -- acceptable |

**Verdict**: The four data-access uses of `get` are perfectly consistent (look up by identifier, return Option on miss). `http.get` is a different verb that happens to share a name. This is universally understood across all programming ecosystems and not a problem.

### The `find` vs `index_of` vs `find_index` distinction

| Verb | Input | Returns | "Search by" |
|------|-------|---------|-------------|
| `find` | predicate | `Option[A]` (element) | predicate |
| `find_index` | predicate | `Option[Int]` (position) | predicate |
| `index_of` | value | `Option[Int]` (position) | equality |

This three-way split is clean and unambiguous. The naming clearly signals what you pass in and what you get back.

---

## 4. Gap Analysis

### string.get(s, i) as alias for char_at(s, i)

**Current**: `string.char_at(s, 0)` -- unique verb, only exists on string.
**Proposed**: `string.get(s, 0)` as alias.

| Argument For | Argument Against |
|-------------|-----------------|
| Symmetry: `list.get(xs, 0)` and `string.get(s, 0)` look the same | `char_at` makes the return type obvious (it's a character, not a substring) |
| LLMs will guess `string.get` by analogy from list | `get` on string could mean "get substring" to some users |
| Kotlin uses `s.get(i)` for exactly this | TypeScript uses `charAt` which is the source of `char_at` |
| Reduces cognitive load: one verb for "access by position" | Adding aliases increases stdlib surface area |

**Recommendation**: Add `string.get` as an alias in 1.x (as already noted in verb-reform-analysis.md). The symmetry with `list.get` is compelling. Both take `(container, Int)` and return `Option`. The name `char_at` remains available for users who prefer explicitness.

### string.char_count(s) vs string.len(s)

**Current**: Both exist. Both count Unicode characters.

| Function | Implementation | Difference |
|----------|---------------|------------|
| `string.len(s)` | `chars().count()` | None |
| `string.char_count(s)` | `chars().count()` | None |

**Recommendation**: Deprecate `char_count`. It was likely introduced to distinguish from byte length, but `string.len` already counts characters in Almide (not bytes). There is no `string.byte_len` to disambiguate from. The existence of two functions that do the same thing is a trap for LLMs: they might use one where they should use the other, or waste tokens deciding.

### string.get_or(s, i, default)

**Not present.** Would be the string analogue of `list.get_or(xs, i, default)`.

| Pattern | list | map | string |
|---------|------|-----|--------|
| `get(_, key)` | Yes | Yes | No (`char_at`) |
| `get_or(_, key, default)` | Yes | Yes | No |

**Recommendation**: If `string.get` is added, `string.get_or` should follow for symmetry. Low priority -- pattern match on `char_at` result with `match` or use `|> Option.unwrap_or(default)` (once the option module exists).

### string.first(s) / string.last(s)

**Not present.** Would be the string analogues of `list.first` / `list.last`.

| Verb | list | string |
|------|------|--------|
| `first` | `first(xs) -> Option[A]` | Not present |
| `last` | `last(xs) -> Option[A]` | Not present |

These could be defined as:
- `string.first(s)` = `string.char_at(s, 0)`
- `string.last(s)` = `string.char_at(s, string.len(s) - 1)`

**Recommendation**: Consider for 1.x as convenience. Already noted in verb-reform-analysis.md under "String slice verbs."

### list.last_index_of

**Not present.** String has `last_index_of` but list does not.

| Verb | list | string |
|------|------|--------|
| `index_of` | Yes | Yes |
| `last_index_of` | No | Yes |

**Recommendation**: Low priority. Can be achieved with `list.reverse |> list.index_of |> ...`. Add if demand arises.

### json.get_string / get_int / get_bool / get_array / get_float

These are typed accessors that combine two steps: key lookup + type extraction. They return `Option` if the key is missing or the type doesn't match.

**Alternative approach**: Chain `json.get` with `json.as_string` etc.:
```
json.get(j, "name") |> Option.flat_map(json.as_string)
```

But the current approach is more LLM-friendly: one function call instead of a pipeline. The `get_<type>` pattern is also used by serde_json in Rust (`v["key"].as_str()`) and similar libraries.

**Assessment**: The `get_<type>` pattern is correct for json because:
1. JSON is dynamically typed, so type-aware access is essential
2. One call is simpler than a pipeline for the common case
3. The `as_<type>` family exists separately for when you already have a Value

The only concern is naming: `get_string` reads as "get a string" which is unambiguous, but it breaks the pattern of single-word verbs. This is acceptable because the alternative (`json.string(j, "key")`) would be confusing.

### map.find

**Not present.** Could find a value matching a predicate.

| Language | Map find |
|----------|---------|
| Rust | `iter().find(pred)` |
| Kotlin | `entries.find(pred)` |
| Ruby | `h.find { pred }` |

**Recommendation**: Low priority. Can be achieved with `map.entries |> list.find(...)`.

---

## 5. Summary Table

| Verb | list | map | string | json | env | http | Consistent? |
|------|------|-----|--------|------|-----|------|-------------|
| `get` | `(xs, i) -> Option[A]` | `(m, k) -> Option[V]` | -- | `(j, key) -> Option[Value]` | `(name) -> Option[String]` | `(url) -> Result` (HTTP) | Yes (data) |
| `get_or` | `(xs, i, d) -> A` | `(m, k, d) -> V` | -- | -- | -- | -- | Yes |
| `find` | `(xs, f) -> Option[A]` | -- | -- | -- | -- | -- | N/A |
| `find_index` | `(xs, f) -> Option[Int]` | -- | -- | -- | -- | -- | N/A |
| `index_of` | `(xs, x) -> Option[Int]` | -- | `(s, sub) -> Option[Int]` | -- | -- | -- | Yes |
| `first` | `(xs) -> Option[A]` | -- | -- | -- | -- | -- | N/A |
| `last` | `(xs) -> Option[A]` | -- | -- | -- | -- | -- | N/A |
| `len` | `(xs) -> Int` | `(m) -> Int` | `(s) -> Int` | -- | -- | -- | Yes |
| `char_at` | -- | -- | `(s, i) -> Option[String]` | -- | -- | -- | N/A |
| `char_count` | -- | -- | `(s) -> Int` | -- | -- | -- | Redundant with len |

---

## 6. Recommendations (Priority Order)

### Immediate (pre-1.0)

1. **Deprecate `string.char_count`** -- identical to `string.len`. Already flagged.

### 1.x Phase

2. **Add `string.get(s, i) -> Option[String]`** -- alias for `char_at`. Aligns string with list/map.
3. **Add `string.get_or(s, i, default) -> String`** -- follows from `string.get`, matches list/map pattern.
4. **Add `string.first(s) -> Option[String]`** and **`string.last(s) -> Option[String]`** -- mirrors list.

### No Change Needed

- `get` semantics across list/map/json/env -- already consistent.
- `index_of` on list and string -- already consistent.
- `find` / `find_index` split -- clean and well-named.
- `json.get_<type>` pattern -- correct for dynamically typed data.
- `http.get` -- different domain, universally understood.
- `len` across list/map/string -- strongest possible consistency.

### Monitor for Future

- `list.last_index_of` -- add if demand arises.
- `map.find` -- achievable via `map.entries |> list.find`.
- `json.len` -- would mean "number of keys" for objects, "number of elements" for arrays. Potentially confusing with mixed types. Hold off.
