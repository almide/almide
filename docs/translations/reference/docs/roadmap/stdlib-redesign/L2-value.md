> Description (inactive source index metadata): Stdlib redesign L2/L3 — value: the dynamic tree owns building and reading (typed getters and paths move in from json, as_object fills the as_ family), and loses stringify, keys and the key-case converters
<a id="l2l3-value"></a>

# L2/L3: value

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](L2-value.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](L2-value.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](L2-value.zh-CN.md)

> Non-normative historical redesign snapshot; pinned original remains authoritative and unchanged. No automatic synchronization, fresh source checks, independent probes or current implementation verification. (F) and measured results are the original author’s historical reports. Preserve initial str-renaming rejection and later acceptance, field/Result versus later get/Option, and old/new path-negative-index behavior. Earlier family/gate counts and later expanded get family coexist; do not silently reconcile them. Keep ordered duplicate object pairs and Float-not-Int semantics. In preserved pseudocode, 種類 means kind. [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-value.md)

> Last updated: 2026-10-21
> Prerequisites: [L0](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L0-conventions.md) / [L1](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L1-categories.md) (R-L1-27); paired page [L2-json](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-json.md)
> Sources: Rust — (not standard; serde_json `Value` is external: https://docs.rs/serde_json/latest/serde_json/enum.Value.html ) / Go https://pkg.go.dev/encoding/json (parses to `any`) / Python https://docs.python.org/3/library/json.html (F) (parses to dict/list) / TS/JS https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/JSON / Gleam https://gleam-stdlib.hexdocs.pm/gleam/dynamic.html / https://gleam-stdlib.hexdocs.pm/gleam/dynamic/decode.html / Elixir https://elixir.hexdocs.pm/Access.html (F) / OCaml — (not standard) / Kotlin — (official pkg https://kotlinlang.org/api/kotlinx.serialization/kotlinx-serialization-json/kotlinx.serialization.json/-json-element/ ) / Swift https://developer.apple.com/documentation/foundation/jsonserialization
> Measurements: `almide 0.68.0 (dev, 437b8f127)`, native and `--target wasm`. Probes in scratchpad `probe-json/`.

Reading: `?` (question mark; converts `Result` to `Option`), `??` (null coalescing), `|>` (pipe).

<a id="l2-役割の比較"></a>

## L2: Role comparison

| Role | Almide | Rust (serde_json, external) | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin (pkg) | Swift |
|---|---|---|---|---|---|---|---|---|---|---|
| Construct values | `null`、`bool`、`int`、`float`、`str`、`array`、`object` | `json!`、`Value::String` | `any` literals | dict/list literals | object literals | pkg `json.string` etc. | terms themselves | — | `JsonPrimitive`、`buildJsonObject` | `Any` |
| Check kind and extract (reason on failure) | `as_string`・`as_int`・`as_float`・`as_bool`・`as_array`（Result） | `as_str`・`as_i64`・`as_object`（Option） | type assertion `v.(string)` | `isinstance` | `typeof` | `decode.string` etc. (Result) | pattern matching | — | `jsonPrimitive.content`、`jsonObject` | `as? String` |
| Extract as object | **—**（`keys` + `field` to construct） | `as_object` | `v.(map[string]any)` | dict | `Object.entries` | `decode.dict` | map | — | `jsonObject` | `as? [String: Any]` |
| Read child by key | `field`（Result） | `get`（Option） | `m["k"]` | `d.get("k")` | `o.k` | `decode.field` | `m["k"]` / `Access.key` (F) | — | `obj["k"]` | `d["k"]` |
| Read typed value by key | `json.get_int` etc. (in json) | `v["k"].as_i64()` | — | — | — | `decode.field("k", decode.int)` | — | — | `obj["k"]?.jsonPrimitive?.intOrNull` | — |
| Key list | `keys`（if not an object: `[]`） | `as_object()?.keys()` | `maps.Keys` | `d.keys()` | `Object.keys` | — | `Map.keys` | — | `obj.keys` | `d.keys` |
| Read / write nested locations by path | `json.get_path` etc. (in json) | `pointer` / `pointer_mut` | — | — | — | `decode.at` | `get_in`/`put_in`/`pop_in` + `Access.key`/`Access.at` (F) | — | — | — |
| Select / omit / merge parts | `pick`、`omit`、`merge` | — | — | dict comprehension | spread | — | `Map.take`/`drop`/`merge` | — | — | — |
| Convert to text | `stringify` (identical to `json.stringify`) | `to_string` | `Marshal` | `dumps` (F) | `JSON.stringify` | pkg `to_string` | `JSON.encode!` | — | `toString` | `JSONSerialization.data` |
| Change key-name style | `to_camel_case`、`to_snake_case`（first level only） | serde `rename_all`（type attribute） | — | — | — | — | — | — | `JsonNamingStrategy` | `keyDecodingStrategy` |

<a id="完全性の規則"></a>

### Completeness rules

1. **`value` constructs and reads value trees** (R-L1-27). Only `json` owns text conversion.
2. **Kind family**: for each kind with contents k ∈ {`bool`, `int`, `float`, `string`, `array`, `object`}, provide these 3 cells:
   - Constructors: `bool`, `int`, `float`, `str`, `array`, `object` (current spellings)
   - `as_<k>(v) -> Result[T, String]`: err with a reason on a different kind
   - `get_<k>(v, key) -> Option[T]`: none for missing key, non-object or wrong kind
   For `object`, `T` has the same shape as the constructor `object`'s input `List[(String, Value)]` (preserves order and duplicates). `null` has no contents and therefore no `as_` / `get_`; check it with `v == value.null()`.
3. **Paths (`Path`)**: construct with `root()`, `key(p, name)`, `index(p, i)`; use with `get_path` (Option), `set_path` (Result), `remove_path` (unchanged if absent). Negative indices are out of bounds (R-L0-12).
4. **Do not conceal failure with sentinels** (R-L0-06 item 4): reads on non-objects return Option or Result. `pick`, `omit`, `merge` are transformations rather than reads, so retain the current form returning non-objects unchanged.
5. **Intentional omission**: constructor `str` differs from `as_string` / `get_string`, but do not rename it (→ reconsidered in R-value-08; rename to `string`) (R-L0-00: not a same-role conflict within Almide, and no majority).

<a id="l2-の判定"></a>

### L2 verdict

- Duplication: `value.stringify` ≡ `json.stringify` (measured: identical strings on both targets) → R-value-01.
- Family gap: no `as_object`. Iterating an object therefore combines `keys` with `field`, spawning dedicated functions such as `json.to_map` → R-value-02.
- Location: `json.get_*` and path operations are tree operations → R-value-02 / R-value-03 (R-json-01 of [L2-json](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-json.md)).
- Sentinel: `value.keys(value.int(1))` returns `[]` (measured). Cannot distinguish a non-object from an empty object (R-L0-06 item 4) → R-value-04.
- Single-purpose functions: `to_camel_case` / `to_snake_case` change only first-level keys; `userID` becomes `user_i_d` (measured). Conflicts with CHEATSHEET's “wire types use wire names; do not have a name-conversion layer” → R-value-05 (#3852).

<a id="案"></a>

## Proposals

<a id="r-value-01-valuestringify-を撤去する---採用"></a>

### R-value-01 Remove `value.stringify` — ○ Accepted

- Content: remove `value.stringify(v)` in favor of `json.stringify(v)`.
- Supporting L0 rules: R-L1-27 (text conversion belongs to json), R-L0-01 (2 spellings for the same output), R-L0-14.
- (b) Same output (`1.0` → `1`, non-finite → `null`; measured on both targets).
- (c) `value` is auto-imported, while `json` needs import, so add `import json` when replacing.
- (d) Remove. `@deprecated(since=15, use="json.stringify")`, `removal_epoch = 16`. `almide fix` rewrites 1-to-1 and adds `import json`. Usage: spec 50 / tests 2 / docs 27 / stdlib 5 / research 2.
- Proposed gate: synonym-gate rule 5 of #3851 F (identical bodies across modules).
- issue: #3953

<a id="r-value-02-種類の族を閉じる-as_object-と-get_種類-を置く---採用"></a>

### R-value-02 Complete the kind family: provide `as_object` and `get_<種類>` — ○ Accepted

- Content:
  - Add `as_object(v) -> Result[List[(String, Value)], String]`.
  - Move from `json`: `get_string`, `get_int`, `get_float`, `get_bool`, `get_array` (same signatures; like `json.get_int`, do not turn Float into Int).
  - Add `get_object(v, key) -> Option[List[(String, Value)]]` (the final family cell).
- Supporting L0 rules: R-L1-27, R-L0-02, R-L0-06 (key reads use `get` and Option; reason-bearing failures use Result), CLAUDE.md's “extend families as matrices.”
- (a) Object extraction exists as serde `as_object`, Kotlin pkg `jsonObject`, Gleam `decode.dict`.
- (b) Pure computation.
- (c) An LLM knowing `as_array` writes `as_object`; currently that causes an undefined-name error.
- (d) Add 2, move 5 (paired with json removals; json-side `@deprecated(since=15, use="value.get_<種類>")` and `removal_epoch = 16` are in [L2-json](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-json.md) R-json-01).
- Proposed gate: family gate `tests/stdlib_value_family_test.rs` compares the names and types of rule 2's 6 kinds × 3 cells with interface JSON.
- issue: #3955

<a id="r-value-03-道筋の操作を-json-から移す---採用"></a>

### R-value-03 Move path operations from `json` — ○ Accepted

- Content: `type value.Path`, `value.root() -> Path`, `value.key(p: Path, name: String) -> Path`, `value.index(p: Path, i: Int) -> Path`, `value.get_path(v, p) -> Option[Value]`, `value.set_path(v, p, x) -> Result[Value, String]`, `value.remove_path(v, p) -> Value`.
  - `json.field` becomes `value.key`, because it cannot share the spelling of `value.field(v, key)` (reads a value).
  - Negative indices for `value.index` are out of bounds (current `json.index` counts from the end; measured: `index(root, -1)` yields `some(3)`). The name is new, so R-L0-14 item 4 does not apply.
- Supporting L0 rules: R-L1-27, R-L0-00 condition 1, R-L0-12, R-L0-14.
- (a) Elixir writes path components as `key` and `at`/`index` (`Access.key`, `Access.at`) (F).
- (b) Pure computation.
- (d) Add 6 functions and 1 type (paired with 6 json removals; json functions get `@deprecated(since=15, use="value.root")` and similar, removed at `removal_epoch = 16`). Usage (6 json functions): spec 262 / tests 20 / docs 76. `almide fix` rewrites 1-to-1 (only negative `json.index` indices get a hint).
- Proposed gate: add `value.index` to the R-L0-12 boundary matrix.
- issue: #3953

<a id="r-value-04-keys-を撤去する---採用"></a>

### R-value-04 Remove `keys` — ○ Accepted

- Content: remove `value.keys(v) -> List[String]` (`[]` for non-objects). Replace with `value.as_object(v)`.
- Supporting L0 rules: R-L0-06 item 4 (fallible reads do not return sentinels), R-L0-01 (not 2 spellings for the same read). R-L0-14 item 4 permits changing `keys`'s return type to `Result`, but that would give 2 spellings to the same read as family cell `as_object`. Therefore remove `keys` in favor of `as_object`.
- (a) serde (`as_object()?.keys()`) and Kotlin pkg (`jsonObject.keys`) confirm object type before retrieving key lists.
- (c) The current form hides that a non-object was passed. Iterating with `keys` and reading with `field` also searches again for each key. `as_object` allows iterating pairs in 1 pass.
- (d) Remove. `@deprecated(since=15, use="value.as_object")`, `removal_epoch = 16`. Meaning-preserving `almide fix` rewrite: `value.keys(v)` → `(value.as_object(v) ?? []) |> list.map(((k, _)) => k)`. Hint: “prefer iterating the `as_object` pairs directly.” Usage: spec 25 / tests 1 / docs 10 / stdlib 0 / research 1.
- issue: #3954

<a id="r-value-05-to_camel_case-と-to_snake_case-を撤去する---採用"></a>

### R-value-05 Remove `to_camel_case` and `to_snake_case` — ○ Accepted

- Content: remove the 2 functions as proposed in #3852. Provide no replacement function.
- Supporting L0 rules: R-L0-01, R-L0-14; CHEATSHEET Rule 1 (wire types use wire names).
- (a) None of the 9 languages provides naming-style conversion as a value-tree function (serde/Kotlin/Swift use type serialization settings).
- (c) The behavior “only first level; `userID` → `user_i_d`” cannot be inferred from the name (measured).
- (d) Remove. `@deprecated(since=15, use="Codec")` (no replacement function), `removal_epoch = 16`. No mechanical rewrite; hint to “write wire names in the `Codec` type.” Usage (2 functions): spec 14 / tests 0 / docs 12 / stdlib 2.
- issue: #3852 (existing)

<a id="r-value-06-構築子-str-を-string-に改名する---不採用-r-value-08-で採用に変更"></a>

### R-value-06 Rename constructor `str` to `string` — × Rejected (→ changed to accepted in R-value-08)

- Reason: `str` (construct) and `as_string` (read) have different roles, so R-L0-00 condition 1 (2 spellings for one role) does not apply. Nor is there a majority spelling. High usage (spec 111 / tests 20 / docs 29 / stdlib 4 / research 11) makes the harm of a period mixing old and new names greater.
- issue: none

<a id="r-value-07-valuefield-を-valuegetoptionにしてget_種類-の族の頭にする---採用"></a>

### R-value-07 Make `value.field` into `value.get` (Option), heading the `get_<種類>` family — ○ Accepted

- Now: `fn value.field(v: Value, key: String) -> Result[Value, String]` plus 6 functions including `fn value.get_int(v: Value, key: String) -> Option[Int]`. No `value.get`.
- Proposal: add `fn value.get(v: Value, key: String) -> Value?` and remove `value.field`. The 7 key-reading family members are `get` and `get_<種類>`, all Option. Use `as_<種類>` (Result) when a reason is needed.
- Supporting L0 rules: R-L0-06 item 1 (fallible extraction has an unsuffixed name and Option: `list.get`, `map.get`, `bytes.get`), R-L0-01 (2 verbs, `field` and `get_`, for key reading).
- (a) Key-based child reading uses Rust JSON library `Value::get` (Option), Python `dict.get`, Elixir `Map.get`. Only Gleam `decode.field` uses `field`.
- (c) Measured: `value.get(v, "a")` yields E002 with “did you mean `value.int`?” LLMs write the same spelling as `map.get`.
- (d) Add 1, remove 1. `@deprecated(since=15, use="value.get")`, remove at epoch 16. `almide fix` rewrites `value.field(v, k)` to `value.get(v, k)`; at sites that propagated with `!`, hint “if you need a reason, use `value.get(v, k) ?? …` or `match`.” Usage: spec 24 / tests 17 / docs 14 / stdlib 24.
- Completeness rule: `get(v, key)` and `get_<k>(v, key)` all return Option; `as_<k>(v)` all return Result.
- Proposed gate: add the `get` cell to R-value-02's family gate.
- issue: #3971

<a id="r-value-08-valuestr-を-valuestring-に改名する完全性の規則-5-の見直し---採用"></a>

### R-value-08 Rename `value.str` to `value.string` (revise completeness rule 5) — ○ Accepted

- Now: constructors `value.int`, `float`, `bool`, `array`, `object` use type names, but only string uses `str`. Extraction uses `as_string` / `get_string`. 2 external reviews both pointed out mistakenly writing `value.string`.
- Proposal: rename `value.str` → `value.string`.
- Supporting L0 rule: R-L0-00 (same-role spellings are split within Almide. Rule 5 called it “not a conflict,” but it was a split where just 1 constructor-family spelling differed).
- (a) gleam_json `json.string`, serde `Value::String`.
- (d) Rename. `@deprecated(since=15, use="value.string")`, `removal_epoch = 16`; rewrite via `almide fix`.
- issue: #3985

<a id="l3-関数表"></a>

## L3: Function table

| Function | Signature (current, mut notation) | Main spellings in other languages | Verdict | Proposal |
|---|---|---|---|---|
| `array` | `value.array(items: List[Value]) -> Value` | serde `Value::Array` / Kotlin pkg `JsonArray` / gleam_json `json.array` | Unchanged | R-value-02 |
| `as_array` | `value.as_array(v: Value) -> Result[List[Value], String]` | serde `as_array` / Gleam `decode.list` / Kotlin pkg `jsonArray` | Unchanged | R-value-02 |
| `as_bool` | `value.as_bool(v: Value) -> Result[Bool, String]` | serde `as_bool` / Gleam `decode.bool` | Unchanged | R-value-02 |
| `as_float` | `value.as_float(v: Value) -> Result[Float, String]` | serde `as_f64` / Gleam `decode.float` | Unchanged | R-value-02 |
| `as_int` | `value.as_int(v: Value) -> Result[Int, String]` | serde `as_i64` / Gleam `decode.int` | Unchanged | R-value-02 |
| `as_string` | `value.as_string(v: Value) -> Result[String, String]` | serde `as_str` / Gleam `decode.string` | Unchanged | R-value-02 |
| `bool` | `value.bool(b: Bool) -> Value` | serde `Value::Bool` / gleam_json `json.bool` | Unchanged | R-value-02 |
| `float` | `value.float(f: Float) -> Value` | gleam_json `json.float` / Kotlin pkg `JsonPrimitive` | Unchanged | R-value-02 |
| `int` | `value.int(n: Int) -> Value` | gleam_json `json.int` / Kotlin pkg `JsonPrimitive` | Unchanged | R-value-02 |
| `merge` | `value.merge(a: Value, b: Value) -> Value` | Elixir `Map.merge` / JS `{...a, ...b}` / Python `a \| b` | Unchanged | — |
| `null` | `value.null() -> Value` | serde `Value::Null` / gleam_json `json.null` / Kotlin pkg `JsonNull` | Unchanged | — |
| `object` | `value.object(pairs: List[(String, Value)]) -> Value` | gleam_json `json.object` / Kotlin pkg `buildJsonObject` | Unchanged | R-value-02 |
| `omit` | `value.omit(v: Value, keys: List[String]) -> Value` | Elixir `Map.drop` / lodash `omit` | Unchanged | — |
| `pick` | `value.pick(v: Value, keys: List[String]) -> Value` | Elixir `Map.take` / lodash `pick` | Unchanged | — |
| `str` | `value.str(s: String) -> Value` | gleam_json `json.string` / serde `Value::String` | Rename → `string` | R-value-08 |
| `stringify` | `value.stringify(v: Value) -> String` | JS `JSON.stringify` / Python `dumps` (F) | Remove → `json.stringify` | R-value-01 |
| `to_camel_case` | `value.to_camel_case(v: Value) -> Value` | Kotlin pkg `JsonNamingStrategy` / Swift `keyEncodingStrategy`（type settings） | Remove | R-value-05 |
| `to_snake_case` | `value.to_snake_case(v: Value) -> Value` | Same as above | Remove | R-value-05 |
| `field` | `value.field(v: Value, key: String) -> Result[Value, String]` | serde `get`（Option） / Gleam `decode.field` / Elixir `Access.key` (F) | Remove → `value.get`（Option） | R-value-07 |
| `keys` | `value.keys(v: Value) -> List[String]` | serde `as_object()?.keys()` / JS `Object.keys` / Python `d.keys()` | Remove → `as_object` | R-value-04 |
| `as_object` | `value.as_object(v: Value) -> Result[List[(String, Value)], String]`（Proposal） | serde `as_object` / Kotlin pkg `jsonObject` / Gleam `decode.dict` | Add | R-value-02 |
| `get_string` | `value.get_string(v: Value, key: String) -> Option[String]`（Proposal） | serde `v["k"].as_str()` | Add (moved from `json.get_string`） | R-value-02 |
| `get_int` | `value.get_int(v: Value, key: String) -> Option[Int]`（Proposal） | serde `v["k"].as_i64()` | Add (moved from `json.get_int`） | R-value-02 |
| `get_float` | `value.get_float(v: Value, key: String) -> Option[Float]`（Proposal） | serde `v["k"].as_f64()` | Add (moved from `json.get_float`） | R-value-02 |
| `get_bool` | `value.get_bool(v: Value, key: String) -> Option[Bool]`（Proposal） | serde `v["k"].as_bool()` | Add (moved from `json.get_bool`） | R-value-02 |
| `get_array` | `value.get_array(v: Value, key: String) -> Option[List[Value]]`（Proposal） | serde `v["k"].as_array()` | Add (moved from `json.get_array`） | R-value-02 |
| `get_object` | `value.get_object(v: Value, key: String) -> Option[List[(String, Value)]]`（Proposal） | serde `v["k"].as_object()` | Add | R-value-02 |
| `root` | `value.root() -> Path`（Proposal） | serde `pointer("")` | Add (moved from `json.root`） | R-value-03 |
| `key` | `value.key(p: Path, name: String) -> Path`（Proposal） | Elixir `Access.key` (F) | Add (moved from `json.field`） | R-value-03 |
| `index` | `value.index(p: Path, i: Int) -> Path`（Proposal） | Elixir `Access.at` (F) | Add (moved from `json.index`; negative indices out of bounds） | R-value-03 |
| `get_path` | `value.get_path(v: Value, p: Path) -> Option[Value]`（Proposal） | Elixir `get_in` (F) / serde `pointer` | Add (moved from `json.get_path`） | R-value-03 |
| `set_path` | `value.set_path(v: Value, p: Path, x: Value) -> Result[Value, String]`（Proposal） | Elixir `put_in` (F) / serde `pointer_mut` | Add (moved from `json.set_path`） | R-value-03 |
| `remove_path` | `value.remove_path(v: Value, p: Path) -> Value`（Proposal） | Elixir `pop_in` (F) | Add (moved from `json.remove_path`） | R-value-03 |
| `get` | `value.get(v: Value, key: String) -> Option[Value]`（Proposal） | Rust JSON `Value::get` / Python `dict.get` / Elixir `Map.get` | Add | R-value-07 |
