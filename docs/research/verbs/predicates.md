# Predicate / Test Verbs

Analysis of Almide's stdlib predicate verbs: `contains`, `is_empty`, `is_*`, `starts_with`, `ends_with`, `any`, `all`.

---

## 1. Inventory

### `contains`

| Module | Signature | Semantics |
|--------|-----------|-----------|
| `list` | `contains(xs: List[A], x: A) -> Bool` | Value membership |
| `map` | `contains(m: Map[K, V], key: K) -> Bool` | Key existence |
| `string` | `contains(s: String, sub: String) -> Bool` | Substring search |

### `is_empty`

| Module | Signature |
|--------|-----------|
| `list` | `is_empty(xs: List[A]) -> Bool` |
| `map` | `is_empty(m: Map[K, V]) -> Bool` |
| `string` | `is_empty(s: String) -> Bool` |

### `starts_with` / `ends_with`

| Module | Signature |
|--------|-----------|
| `string` | `starts_with(s: String, prefix: String) -> Bool` |
| `string` | `ends_with(s: String, suffix: String) -> Bool` |

String-only. No list equivalent.

### `any` / `all`

| Module | Signature |
|--------|-----------|
| `list` | `any(xs: List[A], f: Fn[A] -> Bool) -> Bool` |
| `list` | `all(xs: List[A], f: Fn[A] -> Bool) -> Bool` |

List-only. No map equivalent.

### `is_*` (type/state tests)

| Module | Verb | Tests |
|--------|------|-------|
| `string` | `is_digit` | All chars are ASCII digits |
| `string` | `is_alpha` | All chars are alphabetic |
| `string` | `is_alphanumeric` | All chars are alphanumeric |
| `string` | `is_whitespace` | All chars are whitespace |
| `string` | `is_upper` | All chars are uppercase |
| `string` | `is_lower` | All chars are lowercase |
| `float` | `is_nan` | Value is NaN |
| `float` | `is_infinite` | Value is +/- infinity |
| `result` | `is_ok` | Result is ok variant |
| `result` | `is_err` | Result is err variant |
| `fs` | `is_dir` | Path is a directory |
| `fs` | `is_file` | Path is a regular file |
| `fs` | `is_symlink` | Path is a symbolic link |
| `uuid` | `is_valid` | String is a valid UUID |
| `regex` | `is_match` | Pattern matches somewhere in string |
| `datetime` | `is_before` | Timestamp a < timestamp b |
| `datetime` | `is_after` | Timestamp a > timestamp b |

---

## 2. Cross-Language Comparison

### `contains`

| Language | List | Map/Dict | String |
|----------|------|----------|--------|
| **Rust** | `vec.contains(&x)` | `map.contains_key(&k)` | `s.contains(sub)` |
| **Kotlin** | `list.contains(x)` | `map.containsKey(k)`, `map.containsValue(v)` | `s.contains(sub)` |
| **Swift** | `arr.contains(x)` | `dict.keys.contains(k)` | `s.contains(sub)` |
| **Gleam** | `list.contains(xs, x)` | `map.has_key(m, k)` | `string.contains(s, sub)` |
| **Python** | `x in list` | `k in dict` | `sub in s` |
| **Go** | `slices.Contains(s, x)` | `_, ok := m[k]` | `strings.Contains(s, sub)` |
| **Ruby** | `arr.include?(x)` | `hash.key?(k)`, `hash.has_key?(k)` | `s.include?(sub)` |
| **TypeScript** | `arr.includes(x)` | `map.has(k)` | `s.includes(sub)` |
| **Almide** | `list.contains(xs, x)` | `map.contains(m, key)` | `string.contains(s, sub)` |

Key observations:

- **Rust** distinguishes map key lookup explicitly: `contains_key`. This avoids the ambiguity of what "contains" means for a map.
- **Gleam** uses `has_key` for maps, keeping `contains` for lists and strings.
- **Kotlin** has both `containsKey` and `containsValue` on maps.
- **TypeScript/Ruby** use a different verb entirely for maps (`has`, `key?`).
- **Python/Go** use the same operator/function name across types, relying on context.
- **Almide** unifies under `contains` for all three, like Python.

### `is_empty`

| Language | Name |
|----------|------|
| **Rust** | `.is_empty()` |
| **Kotlin** | `.isEmpty()` |
| **Swift** | `.isEmpty` (property) |
| **Gleam** | `list.is_empty(xs)`, `string.is_empty(s)` |
| **Python** | `len(x) == 0` or `not x` |
| **Go** | `len(x) == 0` |
| **Ruby** | `.empty?` |
| **TypeScript** | `.length === 0` |
| **Almide** | `list.is_empty(xs)`, `map.is_empty(m)`, `string.is_empty(s)` |

Universal consensus. Almide is aligned with Rust, Kotlin, Gleam.

### `starts_with` / `ends_with`

| Language | String | List |
|----------|--------|------|
| **Rust** | `s.starts_with(p)` | `s.starts_with(&[...])` (slices) |
| **Kotlin** | `s.startsWith(p)` | -- |
| **Swift** | `s.hasPrefix(p)` | `a.starts(with: b)` |
| **Gleam** | `string.starts_with(s, p)` | -- |
| **Python** | `s.startswith(p)` | -- |
| **Go** | `strings.HasPrefix(s, p)` | -- |
| **Ruby** | `s.start_with?(p)` | -- |
| **TypeScript** | `s.startsWith(p)` | -- |
| **Almide** | `string.starts_with(s, p)` | -- |

Most languages only provide string versions. Almide matches the majority. Swift's `hasPrefix`/`hasSuffix` is an outlier; Go's `HasPrefix` is another naming variant. Almide's `starts_with`/`ends_with` follows Rust and Gleam exactly.

### `any` / `all`

| Language | Name | Available on |
|----------|------|-------------|
| **Rust** | `.any()`, `.all()` | Iterator (covers Vec, HashMap, etc.) |
| **Kotlin** | `.any { }`, `.all { }` | Iterable, Map |
| **Swift** | `.contains(where:)`, `.allSatisfy { }` | Sequence |
| **Gleam** | `list.any(xs, f)`, `list.all(xs, f)` | list only |
| **Python** | `any(gen)`, `all(gen)` | any iterable |
| **Go** | (manual loop) | -- |
| **Ruby** | `.any? { }`, `.all? { }` | Enumerable (covers Array, Hash) |
| **TypeScript** | `.some(f)`, `.every(f)` | Array only |
| **Almide** | `list.any(xs, f)`, `list.all(xs, f)` | list only |

Notable: TypeScript calls them `some`/`every`. Swift uses `contains(where:)` instead of `any`. Almide follows Rust/Python/Ruby naming.

Almide currently lacks `map.any` and `map.all`. Rust and Kotlin provide these through iterators/iterable. The verb-reform-analysis already flags this as a 1.x addition.

### `is_ok` / `is_err`

| Language | Name |
|----------|------|
| **Rust** | `.is_ok()`, `.is_err()` |
| **Kotlin** | `.isSuccess`, `.isFailure` (on `Result`) |
| **Swift** | pattern match `case .success`, `case .failure` |
| **Gleam** | `result.is_ok(r)`, `result.is_error(r)` |
| **Python** | N/A (exceptions) |
| **Go** | `err != nil` |
| **Ruby** | N/A (exceptions) |
| **TypeScript** | N/A (exceptions) |
| **Almide** | `result.is_ok(r)`, `result.is_err(r)` |

Almide follows Rust exactly. Gleam spells out `is_error` (not `is_err`). `is_err` is shorter and matches Rust's convention. Good choice for Almide.

### `regex.is_match`

| Language | Name |
|----------|------|
| **Rust** | `regex.is_match(s)` |
| **Kotlin** | `regex.containsMatchIn(s)`, `regex.matches(s)` |
| **Swift** | `s.contains(regex)`, `s.wholeMatch(of:)` |
| **Gleam** | `regex.check(pattern, s)` |
| **Python** | `re.search(pat, s)` (truthy), `re.fullmatch(pat, s)` |
| **Go** | `regexp.MatchString(s)` |
| **Ruby** | `s.match?(pat)` |
| **TypeScript** | `regex.test(s)` |
| **Almide** | `regex.is_match(pat, s)`, `regex.full_match(pat, s)` |

Almide's `is_match` mirrors Rust. Having both `is_match` (partial) and `full_match` (anchored) is clear. `full_match` does not use the `is_` prefix because it reads better as an action ("perform a full match") than a test, though it returns Bool.

### String character class tests (`is_digit`, `is_alpha`, etc.)

| Language | API | Operates on |
|----------|-----|-------------|
| **Rust** | `c.is_ascii_digit()`, `c.is_alphabetic()` | `char` |
| **Kotlin** | `c.isDigit()`, `c.isLetter()` | `Char` |
| **Swift** | `c.isNumber`, `c.isLetter` | `Character` |
| **Gleam** | -- (no stdlib) | -- |
| **Python** | `s.isdigit()`, `s.isalpha()` | `str` (all chars) |
| **Go** | `unicode.IsDigit(r)`, `unicode.IsLetter(r)` | `rune` |
| **Ruby** | `s.match?(/\A\d+\z/)` | manual regex |
| **TypeScript** | (manual regex) | -- |
| **Almide** | `string.is_digit(s)`, `string.is_alpha(s)` | `String` (all chars) |

Most languages put character class tests on `char`/`rune`, not `string`. Python is the notable exception -- `"123".isdigit()` tests all characters, which is exactly Almide's behavior.

Almide has no `Char` type (characters are single-character strings), so placing these on `string` is the only viable choice. This matches Python's model.

---

## 3. The `?` Suffix Question

Almide removed `?` from all predicate names. Many languages use `?` as a suffix for boolean-returning functions:

| Language | Convention | Examples |
|----------|-----------|----------|
| Ruby | `?` suffix | `empty?`, `include?`, `nil?`, `valid?` |
| Gleam | No `?` | `is_empty`, `contains`, `is_ok` |
| Rust | No `?` (but `?` is error propagation) | `is_empty`, `contains`, `is_ok` |
| Kotlin | `is` prefix | `isEmpty`, `isBlank`, `isDigit` |
| Swift | `is` prefix (property) | `isEmpty`, `isNumber` |
| Clojure | `?` suffix | `empty?`, `contains?`, `nil?` |
| Elixir | `?` suffix | `Enum.empty?`, `String.valid?` |

### Arguments for `?`

1. **Visual grep**: `?` makes predicates instantly recognizable in code.
2. **Ruby/Elixir precedent**: Battle-tested convention in two major languages.
3. **Shorter names**: `empty?` vs `is_empty`. `contains?` vs `contains` (saves nothing here, but `valid?` vs `is_valid`).

### Arguments against `?` (Almide's choice)

1. **`?` is taken**: In Almide, `?` is the error propagation operator (like Rust). Using `?` for both would create grammar ambiguity: `xs.empty?()` -- is this "call empty and propagate error" or "call the predicate empty?"?
2. **`is_` prefix is universal**: Rust, Kotlin, Swift, Gleam all use `is_` for boolean functions. No ambiguity.
3. **Not all predicates use `is_`**: `contains`, `starts_with`, `ends_with`, `any`, `all` already read as questions without any prefix or suffix.
4. **Tooling simplicity**: No special characters in identifiers simplifies parsing and syntax highlighting.

### Verdict

**Removing `?` was correct.** The `?` operator for error propagation is too valuable to sacrifice. The `is_` prefix handles the cases where a bare verb would be ambiguous (`is_empty`, `is_ok`, `is_nan`), while verbs that are naturally boolean (`contains`, `starts_with`, `any`, `all`) need no prefix at all.

This matches Rust and Gleam exactly, both of which also reserve `?` for error handling.

---

## 4. `contains` Consistency Analysis

### The three meanings

```
list.contains([1, 2, 3], 2)              // value membership
map.contains(scores, "alice")             // key existence
string.contains("hello world", "world")   // substring search
```

These are three different operations unified under one verb. The question: is this confusing?

### Semantic alignment

| Module | `contains(collection, x)` | What is x? |
|--------|--------------------------|------------|
| `list` | Does the list contain this **element**? | An element |
| `map` | Does the map contain this **key**? | A key |
| `string` | Does the string contain this **substring**? | A substring |

The second argument's type reveals the semantics:
- `list.contains(List[A], A)` -- A is an element
- `map.contains(Map[K, V], K)` -- K is a key, not a value
- `string.contains(String, String)` -- String is a substring

### The `map.contains` concern

`map.contains(m, "alice")` checks for a **key**, not a value. This could surprise someone who expects it to check for a value (like `list.contains` checks for a value). Two mitigations:

1. **The description says "Check if a key exists in the map"** -- documentation is clear.
2. **The type signature makes it obvious**: `contains(m: Map[K, V], key: K)` -- the parameter is named `key`.

### Should we add `map.has_key`?

| Option | Pros | Cons |
|--------|------|------|
| Keep only `map.contains` | Consistent verb across modules. Python-like. | "contains" could mean value. |
| Add `map.has_key` alias | Explicit. Go `_, ok := m[k]` pattern. Gleam uses `has_key`. | Adds a second way to do the same thing. |
| Rename to `map.has_key` | Maximum clarity. | Breaks `contains` cross-module consistency. |

**Recommendation**: Keep `map.contains` as the primary. Consider adding `map.has_key` as an alias in 1.x if user confusion arises. The current design is consistent with Python (`key in dict`) and is unambiguous given the type signature.

### Should we add `map.contains_value`?

```
// Hypothetical
map.contains_value(scores, 100)  // does any value == 100?
```

| Language | Has it? |
|----------|---------|
| Rust | `map.contains_key(&k)` only. `.values().any(|v| v == &x)` for values. |
| Kotlin | `map.containsValue(v)` |
| Java | `map.containsValue(v)` |
| Python | `v in dict.values()` |
| Go | (manual loop) |

**Recommendation**: Not needed. `map.values(m).contains(v)` or `map.values(m).any(fn(v) => v == target)` covers this. Adding `contains_value` would be Kotlin-style completionism that Almide's minimalist stdlib doesn't need.

---

## 5. Missing Predicates

### Option: `is_some` / `is_none`

Almide has `result.is_ok` and `result.is_err` but no `option.is_some` / `option.is_none`.

| Language | API |
|----------|-----|
| Rust | `opt.is_some()`, `opt.is_none()` |
| Kotlin | `val != null` |
| Swift | `opt != nil` |
| Gleam | `option.is_some(opt)`, `option.is_none(opt)` |
| Haskell | `isJust`, `isNothing` |

Currently, Almide has no `option` module at all. The verb-reform-analysis lists `option` module creation (with `is_some`, `is_none`, `map`, `flat_map`, `unwrap_or`) as a 1.x item.

**Assessment**: This is a clear gap. Pattern matching (`match opt { some(x) => ..., none => ... }`) covers the extraction case, but a simple boolean check like `if option.is_some(x) then ...` has no clean equivalent today. Priority: medium. Blocked on the `option` module existing.

### Map: `any` / `all`

As noted above, `map.any` and `map.all` are missing. Already tracked in verb-reform-analysis as 1.x additions. The callback would receive `(K, V)`:

```
map.any(m, fn(k, v) => v > 100)
map.all(m, fn(k, v) => string.len(k) > 0)
```

### List: `starts_with` / `ends_with`

No list equivalents exist. Rarely needed in practice.

| Language | Has it? |
|----------|---------|
| Rust | `slice.starts_with(&[1, 2])` |
| Kotlin | -- |
| Python | -- |
| Ruby | -- |

**Assessment**: Low priority. Rust has it but few other languages do. Can be expressed as `list.take(xs, list.len(prefix)) == prefix`.

---

## 6. Module Placement Questions

### Are `string.is_digit`, `string.is_alpha` etc. on the right module?

**Yes.** Almide has no `Char` type. Characters are single-character `String` values. Therefore character class tests must live on `string`.

The semantics are "test all characters in the string", matching Python's `str.isdigit()` etc. This is the only viable design given Almide's type system.

Potential concern: `string.is_digit("")` returns `true` (vacuous truth, matching Python). This is mathematically correct ("all zero characters are digits") but could surprise users. Documenting this edge case is sufficient.

### Are `datetime.is_before` / `datetime.is_after` necessary?

These are trivial wrappers:
```
// datetime.is_before(a, b) compiles to: a < b
// datetime.is_after(a, b)  compiles to: a > b
```

Since `DateTime` is just `Int` (Unix timestamp), users can write `a < b` directly.

| Language | Has named comparators? |
|----------|----------------------|
| Rust (chrono) | `dt < other` (operator) |
| Python (datetime) | `dt < other` (operator) |
| Go (time) | `t.Before(other)`, `t.After(other)` |
| Java (Instant) | `t.isBefore(other)`, `t.isAfter(other)` |

**Assessment**: These verbs add readability for datetime-heavy code (`datetime.is_before(deadline, now)` reads better than `deadline < now` when the variables aren't obviously timestamps). Keep them. Zero runtime cost (compile to `<` / `>`).

### Is `regex.full_match` misnamed?

`full_match` returns `Bool` but doesn't use the `is_` prefix. Compare:

```
regex.is_match(pat, s)    // partial match, has is_ prefix
regex.full_match(pat, s)  // full match, no is_ prefix
```

The inconsistency exists because `full_match` reads as a verb phrase ("perform a full match") while `is_match` reads as a state test ("is there a match?"). Both return `Bool`.

| Alternative | Assessment |
|-------------|-----------|
| `regex.is_full_match(pat, s)` | Grammatically awkward |
| `regex.matches(pat, s)` | Ambiguous -- partial or full? |
| `regex.matches_fully(pat, s)` | Verbose |
| Keep `regex.full_match` | Acceptable. The return type is `Bool`, which is self-documenting. |

**Recommendation**: Keep as-is. The slight naming inconsistency is less bad than any alternative.

---

## 7. Summary Table

| Verb | Modules | Consistent? | Notes |
|------|---------|-------------|-------|
| `contains` | list, map, string | Mostly | map checks key (not value). Acceptable given type sig. |
| `is_empty` | list, map, string | Yes | Universal across languages. |
| `starts_with` | string | Yes | String-only is fine. |
| `ends_with` | string | Yes | String-only is fine. |
| `any` | list | Gap | Missing on map. Planned for 1.x. |
| `all` | list | Gap | Missing on map. Planned for 1.x. |
| `is_ok` / `is_err` | result | Yes | Matches Rust exactly. |
| `is_nan` / `is_infinite` | float | Yes | Matches Rust exactly. |
| `is_dir` / `is_file` / `is_symlink` | fs | Yes | Standard filesystem predicates. |
| `is_digit` / `is_alpha` / ... | string | Yes | Correct module (no Char type). |
| `is_match` | regex | Yes | Matches Rust. |
| `is_valid` | uuid | Yes | Clear and specific. |
| `is_before` / `is_after` | datetime | Yes | Convenience over `<` / `>`. |
| `is_some` / `is_none` | (missing) | Gap | Needs `option` module. |

## 8. Action Items

### No change needed

- `contains` cross-module consistency is acceptable
- `?` suffix removal was correct
- `string.is_*` character tests are on the correct module
- `regex.full_match` naming is fine despite `is_` inconsistency

### Planned (1.x, already tracked)

- Add `option` module with `is_some`, `is_none`
- Add `map.any`, `map.all`

### Consider (low priority)

- `map.has_key` as alias for `map.contains` if user confusion emerges
- Document `string.is_digit("")` vacuous truth behavior
