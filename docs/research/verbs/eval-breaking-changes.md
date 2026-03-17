# Evaluation: stdlib-1.0.md Breaking Changes

Review of the 12 proposed breaking changes in `docs/specs/stdlib-1.0.md`, section "破壊的変更リスト".

Evaluated against: Rust, Kotlin, Go, Python, TypeScript, Gleam conventions.

---

## Change 1: `string.to_int` removal

**Current**: `string.to_int(s) -> Result[Int, String]`
**Replacement**: `int.parse(s) -> Result[Int, String]`

### Cross-language comparison

| Language | Equivalent |
|---|---|
| Rust | `str::parse::<i32>()` on target type |
| Gleam | `int.parse(s)` on target module |
| Go | `strconv.Atoi()` / `strconv.ParseInt()` -- standalone |
| Python | `int("42")` -- constructor on target |
| Kotlin | `"42".toInt()` on source (but recommends `toIntOrNull()`) |
| TypeScript | `parseInt("42")` -- standalone |

`string.to_int` violates two rules: (1) `to_*` should be infallible, but this returns `Result`, and (2) the function belongs on the target module (`int`), not the source (`string`). Only Kotlin places parsing on the source type, and even Kotlin is moving away from it.

### Migration cost

6 call sites in `.almd` files: `research/grammar-lab/src/mod.almd`, `stdlib/url.almd`, `stdlib/toml.almd`, `spec/lang/error_test.almd`, `spec/integration/codegen_effect_fn_test.almd`, `spec/stdlib/stdlib-test.almd`. Zero exercise files affected. Mechanical find-and-replace.

### Risks

None. `int.parse` already exists with identical semantics and signature.

### Verdict: **APPROVE**

Strongly justified. This is the canonical example of a naming inconsistency that should be cleaned up before 1.0.

---

## Change 2: `string.to_float` removal

**Current**: `string.to_float(s) -> Result[Float, String]`
**Replacement**: `float.parse(s) -> Result[Float, String]`

### Cross-language comparison

Same analysis as `string.to_int`. Every major language places float parsing on the target type or as a standalone function.

### Migration cost

1 call site: `stdlib/toml.almd`. Trivial.

### Risks

None.

### Verdict: **APPROVE**

Same justification as Change 1.

---

## Change 3: `string.char_count` removal

**Current**: `string.char_count(s) -> Int`
**Replacement**: `string.len(s) -> Int`

### Cross-language comparison

| Language | Char length |
|---|---|
| Rust | `s.chars().count()` or `s.len()` (bytes) -- distinct concepts |
| Go | `utf8.RuneCountInString(s)` vs `len(s)` (bytes) |
| Python | `len(s)` -- always char count |
| Kotlin | `s.length` -- always char count |
| TypeScript | `s.length` -- UTF-16 code units |
| Gleam | `string.length(s)` -- grapheme clusters |

In Almide, `string.len` already counts characters (not bytes), making `char_count` a pure duplicate.

### Migration cost

3 call sites in `spec/stdlib/unicode_test.almd`. Only tests.

### Risks

If Almide ever adds a byte-length function, the distinction would matter. But `string.to_bytes(s) |> list.len` covers that case already. No practical risk.

### Verdict: **APPROVE**

Pure redundancy. `len` is the universal verb for "how many elements."

---

## Change 4: `string.char_at` -> `string.get`

**Current**: `string.char_at(s, i) -> Option[String]`
**New name**: `string.get(s, i) -> Option[String]`

### Cross-language comparison

| Language | Indexed access verb |
|---|---|
| Rust | `s.chars().nth(i)` -- no standard `get` for strings |
| Kotlin | `s[i]` or `s.getOrNull(i)` |
| Python | `s[i]` |
| TypeScript | `s.charAt(i)` or `s[i]` |
| Gleam | No direct equivalent |

The rename unifies `string.get` with `list.get` and `map.get`, creating a consistent `get` verb across all indexed/keyed types. This is the Almide design principle "1 verb = 1 meaning."

### Migration cost

18 call sites across 6 files (`stdlib/encoding.almd`, `spec/stdlib/string_test.almd`, `spec/stdlib/stdlib-test.almd`, `spec/lang/edge_cases_test.almd`, `spec/lang/string_test.almd`, and `list.group_by` example in TOML). The `encoding.almd` stdlib file is the only production code affected.

### Risks

LLMs trained on JavaScript/TypeScript might still generate `char_at`. The diagnostic should hint `string.get` when `char_at` is used.

### Verdict: **APPROVE**

Cross-container verb consistency outweighs the familiarity of `charAt`. This is a core Almide design principle.

---

## Change 5: `int.parse_hex` -> `int.from_hex`

**Current**: `int.parse_hex(s) -> Result[Int, String]`
**New name**: `int.from_hex(s) -> Result[Int, String]`

### Cross-language comparison

| Language | Hex parsing |
|---|---|
| Rust | `i64::from_str_radix(s, 16)` -- `from_*` pattern |
| Go | `strconv.ParseInt(s, 16, 64)` -- `Parse*` pattern |
| Python | `int(s, 16)` -- constructor with radix |
| Kotlin | `s.toInt(16)` -- method with radix |

Both `parse_hex` and `from_hex` are defensible. `parse_hex` emphasizes that this is string parsing (it's genuinely a parse operation on text). `from_hex` aligns with the `from_*` constructor pattern (`from_bytes`, `from_codepoint`, `from_entries`).

### Migration cost

5 call sites in `spec/stdlib/int_test.almd` (4 tests) + `stdlib/url.almd` (1 production).

### Risks

The earlier verb research document (`docs/research/verbs/conversion.md`, section 4.2) recommended **keeping `parse_hex` as primary and adding `from_hex` as alias**. The stdlib-1.0 spec goes further and removes `parse_hex` entirely. This creates a tension: `int.parse` stays (for decimal), but `int.parse_hex` is removed. The `parse_*` family (`parse`, `parse_hex`, `parse_iso`) would be broken.

Also: `from_hex` semantically implies "construct from hex" which could be confused with "construct from a hex *value*" rather than "construct from a hex *string*." The `parse` prefix makes the string-interpretation nature explicit.

### Verdict: **MODIFY**

Add `int.from_hex` as an alias and make it the *recommended* form, but keep `int.parse_hex` available (not deprecated) for consistency with the `parse` family. Breaking the `parse`/`parse_hex`/`parse_iso` family is unnecessary. Alternatively, if the spec is firm on one name only, keep `parse_hex` since it's more precise about what the function actually does (parse a string).

---

## Change 6: `result.and_then` -> `result.flat_map`

**Current**: `result.and_then(r, f) -> Result[B, E]`
**New name**: `result.flat_map(r, f) -> Result[B, E]`

### Cross-language comparison

| Language | Name |
|---|---|
| Rust | `and_then` (Result/Option) |
| Kotlin | `flatMap` |
| Scala | `flatMap` |
| Swift | `flatMap` |
| Gleam | `try` (deprecated) |
| Haskell | `>>=` (bind) |

`flat_map` wins on cross-container consistency: `list.flat_map`, `option.flat_map`, `result.flat_map` all mean "map then flatten." `and_then` is Rust-specific.

### Migration cost

Zero call sites in `.almd` files. `result.and_then` exists in the TOML definition but is never actually used in any Almide source code.

### Risks

Rust developers will reflexively reach for `and_then`. The diagnostic should hint `result.flat_map` when `and_then` is used.

Open Question #4 in the spec asks whether to keep `and_then` as an alias. Given Almide's "1 verb = 1 meaning" principle, having an alias contradicts the design. However, the migration cost is zero, so either approach works.

### Verdict: **APPROVE**

Zero migration cost, strong cross-container consistency argument. Do not keep `and_then` as alias -- Almide is not Rust, and the spec's own principle #1 says "1 verb = 1 meaning."

---

## Change 7: `map.map_values` -> `map.map`

**Current**: `map.map_values(m, f) -> Map[K, B]` where `f: (V) -> B`
**New name**: `map.map(m, f) -> Map[K, V2]` where `f: (V) -> V2`

### Cross-language comparison

| Language | Map transform |
|---|---|
| Rust | `.iter().map()` on entries, no `.map_values()` |
| Kotlin | `mapValues { (k, v) -> ... }` -- explicit `mapValues` |
| Python | `{k: f(v) for k, v in d.items()}` |
| Go | manual loop |
| Gleam | `dict.map_values(d, fn(k, v) { ... })` |

This is the most nuanced change. Open Question #2 in the spec asks whether `map.map(m, f)` should take `f(v) -> V2` or `f(k, v) -> V2`.

If `f` takes only `(v)`, this is truly a rename of `map_values` with no semantic change. If `f` takes `(k, v)`, it's a different function (and `map_values` behavior is lost).

Almide's spec says `map.map(m, f)` with `f(v) -> V2`. This is a pure rename.

### Migration cost

Zero call sites. `map.map_values` is defined in the TOML but never used in any `.almd` file.

### Risks

The name `map.map(m, f)` creates a `module.verb` collision: the module name and verb name are identical (`map.map`). This is visually awkward and may confuse LLMs. Kotlin explicitly avoids this by using `mapValues` and `mapKeys` as distinct names. However, `list.map`, `result.map`, and `option.map` all follow this pattern, so `map.map` is consistent.

### Verdict: **APPROVE**

Cross-container consistency (`map` as a universal verb) outweighs the visual awkwardness of `map.map`. Zero migration cost seals the decision.

---

## Change 8: `map.from_entries` removal

**Current**: `map.from_entries(pairs) -> Map[K, V]` (takes `List[(K, V)]`)
**Replacement**: `map.from_list(pairs) -> Map[K, V]`

### Critical issue

The spec says `map.from_list(pairs) -> Map[K, V]` as a replacement for `map.from_entries`. But the **current** `map.from_list` has a completely different signature:

```
Current map.from_list(xs, f) -> Map[K, V]   // takes List[A] + closure Fn[A] -> (K, V)
Spec    map.from_list(pairs) -> Map[K, V]    // takes List[(K, V)], no closure
```

The spec's `map.from_list` is a **signature-breaking change** to an existing function, not just a rename of `from_entries`. This is undocumented in the breaking changes table. Either:
1. The current `map.from_list` (with closure) needs a new name, or
2. The spec needs a different name for the `from_entries` replacement.

### Cross-language comparison

| Language | From pairs |
|---|---|
| Rust | `HashMap::from(vec)` or `.collect()` |
| Kotlin | `mapOf(pairs)` |
| Python | `dict(pairs)` |
| Go | manual loop |
| Gleam | `dict.from_list(pairs)` -- Gleam uses this exact name |

Gleam uses `dict.from_list(pairs)` (no closure), which matches the spec. But Almide already has `from_list` with a different meaning.

### Migration cost

4 call sites for `map.from_entries`: `spec/stdlib/map_generic_test.almd`, `spec/integration/codegen_ownership_test.almd`, `spec/lang/edge_cases_test.almd`, `spec/integration/codegen_pipes_test.almd`.

0 call sites for `map.from_list` (the closure version is defined but unused).

### Risks

High. Overloading `from_list` with a different signature is confusing. If the closure version is dropped, the functionality is lost (constructing a map by transforming each list element). If kept under a different name, users must learn a non-obvious name.

### Verdict: **MODIFY**

This change has a hidden collision. Recommendation: keep `map.from_entries` as the simple pairs-to-map constructor (it's already clear and matches Kotlin's `entries` terminology). Keep `map.from_list(xs, f)` for the closure variant. If consolidation is desired, rename the closure variant to `map.from_list_with(xs, f)` or `map.collect(xs, f)` and let `map.from_list(pairs)` replace `from_entries`. But the spec must explicitly address that the current `from_list` signature is being changed.

---

## Change 9: `list.remove_at` -> `list.remove`

**Current**: `list.remove_at(xs, i) -> List[A]`
**New name**: `list.remove(xs, i) -> List[A]`

### Cross-language comparison

| Language | Remove by index |
|---|---|
| Rust | `vec.remove(i)` |
| Kotlin | `list.removeAt(i)` |
| Python | `del lst[i]` or `lst.pop(i)` |
| Go | slice manipulation |
| TypeScript | `arr.splice(i, 1)` |
| Gleam | No built-in |

Rust uses `remove(i)` (by index). Kotlin uses `removeAt(i)`. The key question is whether `remove` should mean "remove by index" or "remove by value."

In the spec, `map.remove(m, key)` removes by key (value-based). `list.remove(xs, i)` removes by index. This is consistent because maps are keyed containers (remove by key) and lists are indexed containers (remove by index). The parameter name and type (`i: Int` vs `key: K`) disambiguate.

### Migration cost

6 call sites in `spec/stdlib/list_new_test.almd` and `spec/lang/data_types_test.almd`. Only tests.

### Risks

Users might expect `list.remove(xs, value)` to remove by value (like Python's `list.remove(x)`). But Almide's lists are functional (immutable returns), and the `Int` parameter type makes the intent clear. The `filter` function covers removal by value: `xs.filter(fn(x) => x != value)`.

### Verdict: **APPROVE**

Aligns with Rust's `vec.remove(i)`. The `_at` suffix is unnecessary when the parameter type (`Int`) already signals index-based removal. Low migration cost.

---

## Change 10: `json.to_string` -> `json.as_string`

**Current**: `json.to_string(j) -> Option[String]` (extract string from Value)
**New name**: `json.as_string(j) -> Option[String]`

### Cross-language comparison

| Language | Value extraction |
|---|---|
| Rust (serde_json) | `value.as_str()` |
| Kotlin (Gson) | `jsonElement.asString` |
| Go | type assertion `v.(string)` |
| Python | no equivalent (dict access) |
| TypeScript | no equivalent (any cast) |

`as_*` is the dominant convention for dynamic type extraction. Rust's serde_json uses `as_str()`, `as_i64()`, etc. Kotlin uses `.asString`, `.asInt`.

`json.to_string` is dangerously misleading: in every other module, `to_string` means "serialize to string representation." But `json.to_string(j)` means "extract if this is a string." This ambiguity alone justifies the rename.

### Migration cost

Zero call sites. `json.to_string` and `json.to_int` are defined but unused in `.almd` files. `json.as_string` and `json.as_int` already exist as aliases.

### Risks

None. The `as_*` versions already exist and work.

### Verdict: **APPROVE**

The `to_string` name is actively misleading. `as_*` is the correct verb for dynamic type extraction and matches serde_json/Kotlin conventions. Zero migration cost.

---

## Change 11: `json.to_int` -> `json.as_int`

Same analysis as Change 10.

### Verdict: **APPROVE**

Same justification. Zero migration cost.

---

## Change 12: int bitwise niche function relocation

**Current**: `int.wrap_add`, `int.wrap_mul`, `int.rotate_right`, `int.rotate_left` in `int` module
**Proposal**: Move to `math` or remove

### Cross-language comparison

| Language | Wrapping arithmetic |
|---|---|
| Rust | `i32::wrapping_add()`, `i32::rotate_left()` -- on the integer type |
| Go | no wrapping arithmetic (overflow panics or wraps silently) |
| Python | arbitrary precision, not applicable |
| Kotlin | no direct equivalent |
| TypeScript | bitwise ops auto-truncate to 32-bit |

### Current usage

3 call sites, all in `stdlib/hash.almd`: `int.wrap_add`, `int.rotate_right`, `int.rotate_left`. This is the SHA-256/hashing implementation, which is genuine production code (not a test).

### Migration cost

If moved to `math`: 3 call sites in `stdlib/hash.almd` change prefix from `int.` to `math.`. Trivial.
If removed: `stdlib/hash.almd` breaks and needs reimplementation using bitwise primitives.

### Risks

These functions serve a real purpose (cryptographic hashing). Removing them forces users to reimplement wrapping arithmetic from bitwise primitives, which is error-prone. Moving to `math` is harmless but the semantic home is debatable -- wrapping arithmetic is an integer operation, not a mathematical one.

### Verdict: **MODIFY**

Do not remove. Either keep in `int` (they are integer operations) or move to `math`. Moving to `math` only makes sense if `int` is meant to be a "clean" module with only common operations. If moved, also move `to_u32` and `to_u8` which are in the same niche category. But the simplest path is to keep them in `int` and not list them in the public-facing API documentation -- they are advanced/niche but not harmful.

---

## Summary Table

| # | Change | Verdict | Notes |
|---|---|---|---|
| 1 | `string.to_int` removal | **APPROVE** | Canonical fix for to_* semantics violation |
| 2 | `string.to_float` removal | **APPROVE** | Same as #1 |
| 3 | `string.char_count` removal | **APPROVE** | Pure redundancy |
| 4 | `string.char_at` -> `string.get` | **APPROVE** | Cross-container verb unification |
| 5 | `int.parse_hex` -> `int.from_hex` | **MODIFY** | Keep both; don't break parse_* family |
| 6 | `result.and_then` -> `result.flat_map` | **APPROVE** | Zero usage, strong consistency argument |
| 7 | `map.map_values` -> `map.map` | **APPROVE** | Zero usage, verb unification |
| 8 | `map.from_entries` -> `map.from_list` | **MODIFY** | Signature collision with existing from_list |
| 9 | `list.remove_at` -> `list.remove` | **APPROVE** | Matches Rust, clear from parameter type |
| 10 | `json.to_string` -> `json.as_string` | **APPROVE** | Fixes misleading name, zero migration |
| 11 | `json.to_int` -> `json.as_int` | **APPROVE** | Same as #10 |
| 12 | int bitwise niche relocation | **MODIFY** | Do not remove; keep in int or move to math |

**Score: 9 APPROVE, 3 MODIFY, 0 REJECT**

---

## Missing Breaking Changes

The following should be considered before 1.0 but are not listed in the spec:

### M1: `string.pad_left` / `string.pad_right` -> `string.pad_start` / `string.pad_end`

The 1.0 spec defines `pad_start` and `pad_end`, but the current TOML definitions use `pad_left` and `pad_right`. This rename is not listed in the breaking changes table.

**Usage**: 20+ call sites across `stdlib/time.almd`, `stdlib/hash.almd`, `research/grammar-lab/src/mod.almd`, and multiple test files. This is a high-impact rename.

**Justification**: The stdlib-verb-system roadmap establishes `_start`/`_end` as the directional suffix convention (Gleam style), matching `trim_start`/`trim_end`, `starts_with`/`ends_with`, `strip_prefix`/`strip_suffix`. JavaScript/TypeScript also use `padStart`/`padEnd`.

**Verdict**: This MUST be in the breaking changes list. It affects more code than most listed changes.

### M2: `map.from_list` signature change

As discussed in Change 8, the spec redefines `map.from_list` from `(List[A], Fn[A] -> (K, V)) -> Map[K, V]` to `(List[(K, V)]) -> Map[K, V]`. This is an undocumented breaking change that silently removes the closure-based constructor.

**Verdict**: Must be documented. The closure variant needs a new name or the non-closure variant needs a different name.

### M3: `value.as_*` return type inconsistency with `json.as_*`

Currently:
- `json.as_string(j) -> Option[String]`
- `value.as_string(v) -> Result[String, String]`

The `as_*` verb is used in both `json` and `value` modules but with different return types (`Option` vs `Result`). The 1.0 spec defines `as_*` as returning `Option` (design principle #5). The `value` module should be aligned.

**Verdict**: Either align `value.as_*` to return `Option` or document the deliberate difference.

### M4: No `map.flat_map` in the spec

The verb taxonomy document (stdlib-verb-system.md) specifies that `flat_map` should exist on all container types including Map. The 1.0 spec's Map section does not include `flat_map`. This may be intentional (what would `flat_map` on a Map mean?) but should be explicitly noted as a non-goal.

### M5: `string.from_codepoint` naming

The stdlib-verb-system roadmap proposed renaming to `string.from_char_code`. The 1.0 spec keeps `from_codepoint`. This is fine (the spec takes precedence), but the inconsistency between documents should be resolved.

### M6: Internal contradiction with `parse` vs `from_string`

The stdlib-verb-system roadmap (the active roadmap document) proposes deprecating `int.parse` in favor of `int.from_string`. The 1.0 spec keeps `int.parse`. These documents directly contradict each other. The 1.0 spec's decision is correct (see `conversion.md` analysis), but the roadmap document needs updating.

---

## Recommendations

1. **Add M1 to the breaking changes list** -- `pad_left`/`pad_right` -> `pad_start`/`pad_end` is a real break with high usage.
2. **Resolve the `map.from_list` collision** (Change 8 / M2) before proceeding. This is the only change with a hidden design flaw.
3. **Update `docs/roadmap/active/stdlib-verb-system.md`** to align with the 1.0 spec on `parse` vs `from_string`. The spec's decision to keep `parse` is correct.
4. **Add migration hints to the compiler diagnostic** for all renames. When the user writes `string.to_int(s)`, the error should say "use int.parse(s) instead."
5. **Total migration effort**: ~50 mechanical find-and-replace operations across 18 `.almd` files, plus TOML definition updates. No exercises are affected. This is a manageable pre-1.0 migration.
