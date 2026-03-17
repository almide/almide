# Evaluation: Naming Conventions Summary & Open Questions

Evaluation of the stdlib-1.0.md naming taxonomy and the four open questions,
with cross-language evidence and definitive recommendations.

---

## Part 1: Open Question Verdicts

### Q1: `json.to_string` -> `json.as_string` -- Should ALL json.to_* become json.as_*?

**Verdict: Yes. Rename all `json.to_*` extraction functions to `json.as_*`. Do NOT rename `json.stringify`.**

The core issue is that `json.to_string(j)` is ambiguous -- it could mean either:

- (A) "Serialize this JSON value to a text string" (stringify), or
- (B) "Extract the string if this JSON value is a string" (type narrowing)

The current `json.to_string` does (B), but every other `to_string` in the stdlib does rendering/serialization (closer to A). This creates a trap.

**Cross-language evidence:**

| Language/Tool | "Extract string from JSON value" | "Serialize JSON to text" |
|---------------|----------------------------------|--------------------------|
| **jq** | `.foo` (implicit), `tostring` converts any to string | `tojson` |
| **Python json** | `value` (plain dict access) | `json.dumps(obj)` |
| **Ruby JSON** | `hash["key"]` (plain hash) | `JSON.generate(obj)` |
| **TypeScript** | `(value as string)` / type guard | `JSON.stringify(obj)` |
| **Rust serde_json** | `v.as_str()` -> `Option<&str>` | `serde_json::to_string(&v)` |
| **Go encoding/json** | N/A (struct decode) | `json.Marshal(v)` |
| **Kotlin kotlinx** | `jsonElement.jsonPrimitive.content` | `Json.encodeToString(v)` |

The critical precedent is **Rust's serde_json**, which uses exactly `as_str()`, `as_i64()`, `as_bool()`, `as_array()` for type narrowing on `serde_json::Value`. This is the closest analogue to Almide's `Value` type.

jq's `tostring` is a red herring -- jq has no static types, so `tostring` converts any value to a string representation (more like `stringify`). It is not extraction.

**The `as_*` pattern is correct because:**

1. It signals "downcast / type narrowing" -- you are asserting the dynamic type, not converting.
2. It returns `Option`, which aligns with fallible narrowing (might be the wrong type).
3. `to_*` is reserved for infallible conversion in Almide's own taxonomy. `json.to_string` returning `Option` violates this rule.
4. serde_json in Rust uses identical naming for identical semantics.

**What about `json.stringify`?** Keep it. `json.stringify(v)` is serialization (Value -> String text), not extraction. It is infallible and produces a JSON text representation of any Value. This is `to_*` semantics, but `stringify` is the universally recognized verb for JSON serialization (JS, Python's `dumps` notwithstanding). No rename needed.

**Current state in json.toml:** Both `to_string`/`to_int` and `as_string`/`as_int` already exist as duplicates (same runtime call). The migration is: deprecate `json.to_string`/`json.to_int`, keep `json.as_*` as canonical.

**Note on `value` module:** The `value` module already uses `as_*` exclusively (`value.as_string`, `value.as_int`, etc.) and returns `Result` instead of `Option`. The json and value modules are consistent with each other on the `as_*` verb, but inconsistent on the return type (Option vs Result). This is a separate question worth tracking but not blocking.

---

### Q2: Map callback arity -- `map.map(m, f)`: should f be `(v) -> V2` or `(k, v) -> V2`?

**Verdict: f should be `(v) -> V2` (value-only). This is the right default.**

The question is whether `map.map` should pass only the value or also the key to the callback.

**Cross-language evidence:**

| Language | `map`/`mapValues` callback | Full-entry map available? |
|----------|---------------------------|---------------------------|
| **Rust** | `HashMap` has no `.map()`. Users do `iter().map(\|(k,v)\| ...)` -- full entry | No dedicated `map_values` |
| **Kotlin** | `mapValues { (k, v) -> ... }` -- key available but convention is to use `it.value` | `map { (k, v) -> pair }` for full entry |
| **Swift** | `mapValues { v in ... }` -- value-only | No full-entry map |
| **Gleam** | `dict.map_values(d, fn(key, value) { ... })` -- key+value | N/A |
| **Python** | `{k: f(v) for k, v in d.items()}` -- key available in comprehension | Same syntax |
| **Go** | `for k, v := range m { ... }` -- key+value | Same syntax |
| **Ruby** | `transform_values { \|v\| ... }` -- value-only | `map { \|k, v\| ... }` for entries |
| **Haskell** | `Data.Map.map f m` -- value-only | `mapWithKey f m` for key+value |
| **Elm** | `Dict.map (\k v -> ...)` -- key+value | N/A |

The split is clear:

- **Value-only callback**: Swift, Ruby, Haskell -- the "pure functional" tradition
- **Key+value callback**: Gleam, Elm, Python, Go -- the "practical" tradition
- **Both available**: Kotlin (separate methods)

**Arguments for value-only `(v) -> V2`:**

1. **Cross-container symmetry.** `list.map(xs, fn(x) => ...)`, `result.map(r, fn(a) => ...)`, `option.map(o, fn(a) => ...)` all pass the inner value. `map.map(m, fn(v) => ...)` doing the same creates a universal rule: `map` always receives the "content" of the container.
2. **UFCS ergonomics.** `m |> map.map(fn(v) => v * 2)` reads naturally. If the callback were `(k, v)`, users who do not need the key would write `fn(_, v) => v * 2` everywhere -- an annoyance tax.
3. **LLM predictability.** A single rule ("map's callback receives the contained value") is easier for LLMs to learn than "map's callback arity depends on the container type."
4. **80/20 rule.** The vast majority of map transforms operate on values only. When keys are needed, `map.entries(m) |> list.map(fn((k, v)) => ...) |> map.from_entries` handles it explicitly.

**Arguments for key+value `(k, v) -> V2`:**

1. Gleam and Elm do it this way, and they are Almide's closest ML-family relatives.
2. When you need the key, the workaround via `entries` is verbose.

**Recommendation:** Use `(v) -> V2` for `map.map`. This keeps the verb `map` semantically uniform across all container types ("transform the contained value"). If key-aware mapping is needed later, add `map.map_with_key(m, fn(k, v) => ...)` -- but do not add it proactively, as YAGNI applies.

This aligns with the existing `map.map_values` semantics (which already takes `fn(v) -> B`), making the rename from `map_values` to `map` purely cosmetic, not a behavior change.

The callback for `map.filter`, `map.fold`, `map.any`, `map.all`, `map.each`, and `map.find` should remain `(k, v)` because those operations inherently need both (you filter by key+value criteria, you fold over entries, etc.). This is not inconsistent -- `filter` is not `map`. The verb determines the callback shape.

---

### Q3: Option module implementation -- TOML+runtime vs bundled .almd?

**Verdict: TOML + runtime. Not bundled .almd.**

This question is about whether `option` should be defined in `stdlib/defs/option.toml` (like every other module) with backing runtime functions, or implemented as a `.almd` source file that ships with the compiler.

**Arguments for TOML + runtime:**

1. **Consistency.** Every other stdlib module (all 22 of them) uses TOML definitions. Making option the exception would mean a second code path for module resolution, UFCS dispatch, type checking, and codegen. That is engineering cost for zero user benefit.
2. **Codegen control.** TOML entries generate target-specific code. `option.map` in Rust can emit `({opt}).map(|x| ...)` -- a zero-cost abstraction. A `.almd` implementation would compile through the full pipeline, potentially generating less efficient code (extra function calls, boxing, etc.).
3. **LLM visibility.** LLMs generating Almide code need to know function signatures. TOML definitions feed directly into the type checker's signature database. A `.almd` file would need separate parsing infrastructure for type extraction.
4. **Error messages.** TOML-defined functions produce clear "unknown function" diagnostics when misused. Self-hosted `.almd` functions would produce error messages pointing into the bundled source -- confusing for users.
5. **Multi-target.** Option must work identically on Rust and TypeScript targets. TOML templates handle this (separate `rust` and `ts` fields). A `.almd` file would need to compile correctly to both targets, but Option's runtime semantics differ (Rust `Option<T>` vs TS nullable `T | null`).

**Arguments for bundled .almd:**

1. **Dogfooding.** Writing stdlib in Almide proves the language is expressive enough.
2. **Simplicity.** No new runtime functions needed.

**Why dogfooding loses here:**

Option is a fundamental type that interacts with pattern matching, the type checker, and codegen in ways that ordinary user code does not. `option.map` needs to understand that `Some(x)` should call `f(x)` and `None` should stay `None` -- this requires either pattern matching (which means the .almd file depends on the compiler supporting Option pattern matching already) or runtime functions (which is what TOML gives you). The circular dependency makes .almd impractical.

Furthermore, Rust's `Option<T>` and TypeScript's nullable representation are fundamentally different at the runtime level. A `.almd` implementation cannot abstract over this without the TOML template's per-target code.

**Recommendation:** TOML + runtime, like every other module. The signatures are:

```toml
[map]
type_params = ["A", "B"]
params = [{ name = "opt", type = "Option[A]" }, { name = "f", type = "Fn[A] -> B" }]
return = "Option[B]"
rust = "({opt}).map(|{f.args}| {{ {f.body} }})"
ts = "({opt}) != null ? (({f.args}) => {f.body})({opt}) : null"
```

This gives zero-cost Rust codegen and correct nullable semantics in TypeScript.

---

### Q4: `and_then` retention -- keep as alias alongside `flat_map`, or remove entirely?

**Verdict: Keep `and_then` as an alias. Do NOT remove it.**

**Cross-language evidence:**

| Language | Name for "flatMap on Result/Option" | Also has `flat_map`? |
|----------|-------------------------------------|---------------------|
| **Rust** | `and_then` | `flat_map` on Iterator only |
| **Gleam** | `result.try` (deprecated -> `use`) | N/A |
| **Elm** | `Result.andThen` | N/A |
| **Kotlin** | `flatMap` (Arrow) | Yes, same name |
| **Swift** | `flatMap` | Yes, same name |
| **Scala** | `flatMap` | Yes, same name |
| **Haskell** | `>>=` (bind) | N/A |
| **OCaml** | `Result.bind` / `Option.bind` | N/A |

The split is between two traditions:

- **ML/Haskell tradition**: `bind`, `and_then`, `>>=` -- emphasizes sequential composition
- **Scala/Kotlin/Swift tradition**: `flatMap` -- emphasizes structure (map then flatten)

Both names describe the same operation from different perspectives. Neither is wrong.

**Arguments for keeping `and_then` as alias:**

1. **Rust is Almide's primary codegen target.** Rust developers are the most likely early adopters. They expect `and_then` on Result. Removing it creates unnecessary friction.
2. **Elm uses `andThen` too.** This is not a Rust-only convention.
3. **Zero cost.** One extra TOML entry pointing to the same implementation. No runtime overhead, no documentation burden beyond a one-line "alias for `flat_map`" note.
4. **Backward compatibility.** `and_then` already exists in the current codebase. Removing it breaks existing code for no functional gain.

**Arguments for removing:**

1. Two names for the same thing increase cognitive load.
2. LLMs might use either name inconsistently.

**Why the removal arguments are weak:**

The cognitive load argument is real but minor -- documentation saying "flat_map (also: and_then)" is a single line. The LLM inconsistency argument is actually an argument FOR keeping it: if an LLM generates `result.and_then(r, f)`, it should work. Removing `and_then` means the LLM's code fails even though the intent was correct.

**What Gleam did is instructive:** Gleam deprecated `result.try` in favor of the `use` keyword. They did not add `flat_map` -- they went a completely different direction. This shows there is no industry consensus on which single name to pick, which means supporting both is the pragmatic choice.

**Recommendation:** Keep both. Document `flat_map` as canonical. `and_then` is a supported alias.

---

## Part 2: Naming Conventions Completeness Evaluation

The spec's Naming Conventions Summary table covers 11 verb patterns:

| Pattern | Covered? | Assessment |
|---------|----------|------------|
| `to_*` | Yes | Correct. "Infallible output-direction conversion." |
| `from_*` | Yes | Correct. "Target-side construction." |
| `parse` | Yes | Correct. "Fallible string interpretation." |
| `as_*` | Yes | Correct. "Dynamic Value type extraction." |
| `is_*` | Yes | Correct. "Bool predicate." |
| `get` | Yes | Correct. "Index/key access." |
| `find` | Yes | Correct. "Predicate search." |
| `map` | Yes | Correct. "Transform each element/inner value." |
| `flat_map` | Yes | Correct. "Map then flatten." |
| `fold` | Yes | Correct. "Accumulate with initial value." |
| `each` | Yes | Correct. "Side-effect iteration." |

### Missing from the table

The following verbs are used in the spec but not listed in the Naming Conventions Summary:

| Pattern | Meaning | Example | Should add? |
|---------|---------|---------|-------------|
| `stringify` | Infallible serialization to text | `json.stringify(v)` | **Yes** -- it is the inverse of `parse` for format modules. A distinct verb from `to_string`. |
| `contains` | Membership/existence test | `list.contains(xs, x)` | **Yes** -- it is a cross-module verb (list, map, string) with consistent semantics. |
| `filter` | Keep elements matching predicate | `list.filter(xs, f)` | **Yes** -- used on list, map, and planned for more. |
| `reduce` | No-init accumulation | `list.reduce(xs, f)` | **Consider** -- distinct from `fold` (no initial value, returns Option). |
| `sort` / `sort_by` | Ordering | `list.sort(xs)` | **Consider** -- important verbs but list-only. |
| `unwrap_or` | Extract with fallback | `result.unwrap_or(r, d)` | **Yes** -- cross-module (result, option). A key "escape hatch" verb. |
| `get_or` | Access with default | `list.get_or(xs, i, d)` | **Consider** -- the "safe default" counterpart to `get`. |

**Recommendation:** Add at minimum `stringify`, `contains`, `filter`, and `unwrap_or` to the naming conventions table. These are cross-module verbs with defined semantics that LLMs and users need to internalize.

### Edge cases and verbs that don't fit the taxonomy

**1. `count` -- semantic split (documented in aggregate.md)**

`list.count(xs, f)` takes a predicate. `string.count(s, sub)` takes a value. Same verb, different callback shapes. The type system prevents misuse, and both conventions are standard in other languages (Kotlin predicate-count, Python substring-count). Accept the inconsistency.

**2. `regex.full_match` -- missing `is_` prefix**

`regex.is_match` has the `is_` prefix; `regex.full_match` does not. Both return `Bool`. This is documented in predicates.md and the recommendation is to keep it as-is because `is_full_match` is grammatically awkward.

**3. `get_string`, `get_int`, `get_bool`, `get_array`, `get_float` -- compound verbs**

These json-specific verbs combine key lookup + type extraction. They break the single-word verb pattern but are justified by the frequency of the operation in JSON-heavy code. They are not cross-module and do not need to be in the naming conventions table.

**4. `value.to_camel_case` / `value.to_snake_case` -- are these `to_*`?**

These transform Value key names, not the Value's type. They are `to_*` in the sense of "convert key naming convention to X" but do not fit the "infallible type conversion" definition. They are closer to `string.to_upper` / `string.to_lower` -- case transforms. This is acceptable because the `to_*` pattern in Almide is defined as "infallible output-direction conversion," and case conversion qualifies (the output format is identified by the suffix).

**5. `strip_prefix` / `strip_suffix` -- return type outlier**

These return `Option[String]` (None if the prefix/suffix is not present). This makes them fallible -- closer to `as_*` semantics than `to_*`. But the verb `strip` is universally understood (Rust `strip_prefix`, Python `removeprefix`). They do not use `to_*` or `as_*` prefixes, so they do not violate the taxonomy. No change needed.

**6. `scan` -- unique to list**

`scan` (running fold) exists only on list and has no cross-module usage. It is a well-known FP verb (Kotlin, Gleam, Haskell `scanl`). No naming issue, but it could be noted in the frozen verb set.

**7. `merge` -- used in map and value**

`map.merge(m1, m2)` and `value.merge(a, b)` share the same verb with the same semantics (combine two containers, right-side wins on conflict). Consistent. Could be added to the naming conventions table if more modules gain `merge`.

---

## Part 3: Overall Assessment

The naming conventions in stdlib-1.0.md are **well-designed and nearly complete**. The taxonomy of `to_*`/`from_*`/`parse`/`as_*`/`is_*` is internally consistent and aligns with the strongest precedents in the Rust/Gleam/Kotlin ecosystem.

### Strengths

1. The `to_*` = infallible / `parse` = fallible distinction is the single most important naming rule. It eliminates the `string.to_int` class of confusion.
2. `as_*` for dynamic Value extraction is a direct match for serde_json's naming, which is the closest industry analogue.
3. Cross-module verb consistency (same verb, same semantics) is at a high level for the core operations (get, contains, is_empty, map, filter, fold).

### Weaknesses

1. The naming conventions table is incomplete -- it covers 11 of approximately 18 significant verb patterns.
2. The `count` semantic split (predicate vs substring) is accepted but undocumented in the spec itself.
3. The `json.to_string`/`json.as_string` duplication still exists in the codebase and needs to be cleaned up.

### Summary of Recommendations

| Question | Decision | Confidence |
|----------|----------|------------|
| Q1: json.to_* -> json.as_* | **Yes, rename all extraction functions** | High |
| Q2: map.map callback arity | **`(v) -> V2` (value-only)** | High |
| Q3: Option implementation | **TOML + runtime** | High |
| Q4: `and_then` retention | **Keep as alias** | High |

All four questions have clear answers supported by cross-language evidence. None require further deliberation.
