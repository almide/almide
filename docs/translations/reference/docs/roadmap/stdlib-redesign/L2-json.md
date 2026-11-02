<!-- description: Stdlib redesign L2/L3 — json: only the text codec stays (parse, stringify, stringify_pretty); typed getters and JsonPath move to value; parse rejects trailing text -->
<a id="l2l3-json"></a>

# L2/L3: json

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](L2-json.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](L2-json.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](L2-json.zh-CN.md)

> Non-normative dated redesign snapshot; pinned original authoritative and unchanged, no automatic synchronization, new corpus measurement or implementation verification. Accepted proposals, old measured behavior and migration gates are distinct. The broad nine-language rejection claim and later seven-with-standard/official-package explanation are retained. R-json-01 migration prose versus L3 result.map composition and the historical parameter-name table are not silently harmonized. Depth-limit proposal is not a general resource-safety guarantee. [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-json.md)

> Last updated: 2026-10-21
> Prerequisites: [L0](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L0-conventions.md), [L1](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L1-categories.md) (R-L1-27), paired page [L2-value](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-value.md).
> Sources: Rust — (not in std; external serde_json https://docs.rs/serde_json ); Go https://pkg.go.dev/encoding/json ; Python https://docs.python.org/3/library/json.html (F); TS/JS https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/JSON ; Gleam — (official pkg https://hexdocs.pm/gleam_json/ ); Elixir https://elixir.hexdocs.pm/JSON.html ; OCaml — (not in std); Kotlin — (official pkg https://kotlinlang.org/api/kotlinx.serialization/kotlinx-serialization-json/ ); Swift https://developer.apple.com/documentation/foundation/jsonserialization
> Measurement: `almide 0.68.0 (dev, 437b8f127)`, native and `--target wasm`. Probe: scratchpad `probe-json/p1.almd`.

<a id="l2-役割の比較"></a>

## L2: Comparing Roles

| Role | Almide | Rust | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin | Swift |
|---|---|---|---|---|---|---|---|---|---|---|
| Text → value | `json.parse` | pkg `from_str` | `Unmarshal` | `loads` (F) | `JSON.parse` | pkg `json.parse` | `JSON.decode` | — | pkg `Json.parseToJsonElement` | `JSONSerialization.jsonObject` |
| Value → text, compact | `json.stringify`、`value.stringify` (same output) | pkg `to_string` | `Marshal` | `dumps` (F) | `JSON.stringify` | pkg `json.to_string` | `JSON.encode!` | — | pkg `toString()` | `JSONSerialization.data` |
| Value → text, indented | `json.stringify_pretty` | pkg `to_string_pretty` | `MarshalIndent` | `dumps(indent=)` (F) | `JSON.stringify(v, null, 2)` | — | — | — | pkg `prettyPrint` | `.prettyPrinted` |
| Text with extra trailing characters | **Accepts**（`"1 garbage"` → `1`） | pkg err | err | err（`Extra data`）(F) | SyntaxError | pkg err | err | — | pkg err | err |
| Get typed value by key | `json.get_string`  and five others | pkg `v["k"].as_str()` | — | `d.get("k")` | `o.k` | — | — | — | pkg `jsonObject["k"]?.jsonPrimitive` | — |
| Read/write deep locations by path | `json.root`/`field`/`index` + `get_path`/`set_path`/`remove_path` | pkg `pointer` | — | — | — | — | `get_in`/`put_in`/`pop_in` + `Access.key`/`Access.at` (F) | — | — | — |

<a id="完全性の規則"></a>

### Completeness Rules

1. **`json` only converts to and from text**, R-L1-27: `parse` (text → `Value`), `stringify`, `stringify_pretty` (`Value` → text). Functions building or reading value trees belong in `value` unless text is involved.
2. `parse` follows RFC 8259 grammar and errors on non-whitespace characters after the document.
3. `stringify` and `stringify_pretty` use the same value rules: nonfinite Float becomes `null`; `1.0` becomes `1`.

<a id="l2-の判定"></a>

### L2 Judgments

- Location: the 12 functions `get_string`, `get_int`, `get_float`, `get_bool`, `get_array`; `root`, `field`, `index`, `get_path`, `set_path`, `remove_path`; and `to_map` are tree operations without text → R-json-01, R-L1-27.
- Duplication: `value.stringify` and `json.stringify` output identically, measured `eq true` on both targets. Remove the `value` version, R-value-01 in [L2-value](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-value.md).
- Acceptance: `json.parse("1 garbage")` → `ok(1)` and `json.parse("{\"a\":1} {")` → `ok({"a":1})`, measured on both targets. None of the nine languages accepts this → R-json-02.
- `json.field(path, name)` constructs a path, whereas `value.field(v, key)` reads a value: same spelling, different meaning. Separate their names when moving, R-json-01.

<a id="案"></a>

## Proposals

<a id="r-json-01-型つきの取り出しと道筋の操作を-value-に移す---採用"></a>

### R-json-01 Move Typed Getters and Path Operations to `value` — ○ Accepted

- Contents: new spellings are settled in R-value-02/03 of [L2-value](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-value.md).

  | Removed spelling | Destination |
  |---|---|
  | `json.get_string`・`get_int`・`get_float`・`get_bool`・`get_array` | `value.get_string`・`get_int`・`get_float`・`get_bool`・`get_array` (same signature) |
  | `json.root()` | `value.root()` |
  | `json.field(path, name)` | `value.key(path, name)` (distinct name from `value.field`) |
  | `json.index(path, i)` | `value.index(path, i)` (negative indexes out of bounds, not counted from the end) |
  | `json.get_path`・`set_path`・`remove_path` | `value.get_path`・`set_path`・`remove_path` (same signature) |
  | `type json.JsonPath` | `type value.Path` |
  | `json.to_map(j)` → `Option[Map[String, String]]` | `value.as_object(v)` and `map.from_list` (values remain `Value`) |

- L0 basis: R-L1-27; R-L0-02, subjects are `Value` and paths, not text; R-L0-00 condition 1, `field` means different things in json/value; R-L0-12, negative indexes out of bounds; R-L0-14.
- (a) Kotlin separates text conversion from value trees via `Json` / `JsonElement`, Gleam via `gleam_json` / `dynamic`. Path-part names resemble Elixir `Access.key` / `Access.at` (F).
- (b) Pure computation. Move bodies unchanged.
- (c) `value` is automatically imported, so usable without `import json`. Currently `import json` is needed solely to use `json.get_int`.
- (d) Remove 12 functions, rename one type. Add deprecation attributes such as `@deprecated(since=15, use="value.get_int")` pointing to each destination, with `use` following the table; remove at `removal_epoch = 16`. `almide fix` rewrites one-to-one and deletes unused `import json`. Calls passing negative indexes to `json.index`, and `json.to_map`, change shape and receive hints. Usage: five `get_*` functions spec 54 / tests 0 / docs 52 / stdlib 3 / research 19; six path functions spec 262 / tests 20 / docs 76 / stdlib 0; `to_map` spec 4 / tests 0 / docs 15 / stdlib 3.
- Proposed gate: R-L0-G's R-L0-02 check verifies that public `json` functions take or return `String`.
- Issue: #3953.

<a id="r-json-02-parse-は文書の後ろの余計な文字を-err-にする---採用"></a>

### R-json-02 `parse` Errors on Extra Text After a Document — ○ Accepted

- Contents: `json.parse` returns `err("unexpected trailing text at pos N")` if non-whitespace follows a value. Name and type unchanged.
- L0 basis: R-L0-06 item 4, fallible conversion must not hide failure as a value. This narrows the domain without changing correct JSON answers, so is not R-L0-14 item 4's “same name, different result”; L0 treats domain narrowing as not requiring an epoch.
- (a) All seven among the nine surveyed languages with JSON in std or official packages reject it; Python reports `Extra data` (F).
- (b) Pure computation. Both target parsers share the self-host implementation and change together.
- (c) Currently concatenated JSON documents or truncated log lines silently succeed using only the first value.
- (d) Semantic change, narrowed domain. Record in dialect epoch 15 `notes`, not epoch `breaks`. Before changing, count spec/tests/downstream calls using `parse` on text with trailing characters. `json.parse` usage itself: spec 203 / tests 41 / docs 77 / stdlib 3 / research 29.
- Proposed gate: two cases in `spec/stdlib/json_test.almd`, trailing whitespace only → ok and trailing characters → err, on both targets. Add JSONTestSuite `n_structure_trailing_*` cases as differential-fuzz seeds.
- Issue: #3921.

<a id="r-json-03-stringify-と-stringify_pretty-を据え置く---採用変更なし"></a>

### R-json-03 Keep `stringify` and `stringify_pretty` — ○ Accepted, Unchanged

- Contents: keep both functions. Align `stringify_pretty` parameter name `j` to `v`, a documentation-only change with no signature-meaning change.
- L0 basis: R-L0-00; `stringify` matches JS spelling, no majority but no collision.
- Issue: none.

<a id="r-json-04-parse-の入れ子の深さに上限を置く---採用"></a>

### R-json-04 Bound `parse` Nesting Depth — ○ Accepted

- Current: recursive-descent `json.parse` has no nesting limit, `runtime/rs/src/json.rs:47-223` and `stdlib/json_parse.almd`. Deeply nested `[[[[…]]]]` strings end in stack-overflow abort, C-196.
- Proposal: array/object nesting beyond 1000 levels returns `err("json: nesting deeper than 1000")`.
- L0 basis: R-L0-47 item 2. No function to change the limit, exception under R-L0-47 item 3. Among surveyed languages, only Rust `serde_json` offers `disable_recursion_limit`, and that merely removes the limit.
- (a) Go `encoding/json`: 10000; Rust `serde_json`: 128; Python `json`: recursion limit, approximately 1000 by default.
- (d) Semantic change: nesting above 1000 goes from abort to err. Inputs over 1000 that currently succeed already lie in the range that may abort depending on native stack size.
- Issue: #4020.

<a id="関連-codec-で型に-decode-するときr-l0-48"></a>

### Related: Decoding into Types with Codec, R-L0-48

Codec-derived `T.decode` and `T.decode_strict`, not json-module functions, read `json.parse` values into types. `decode_strict` rejects unknown keys; `@skip_decode` marks fields not read from input. Rules are in L0 R-L0-48.

<a id="l3-関数表"></a>

## L3: Function Table

| Function | Signature, current, mut shown | Main spellings in other languages | Judgment | Proposal |
|---|---|---|---|---|
| `parse` | `json.parse(text: String) -> Result[Value, String]` | JS `JSON.parse` / Python `loads` (F) / Go `Unmarshal` / Elixir `JSON.decode` | Semantic change: trailing text errs; nesting above 1000 errs | R-json-02、R-json-04 |
| `stringify` | `json.stringify(v: Value) -> String` | JS `JSON.stringify` / Python `dumps` (F) / Go `Marshal` / Elixir `JSON.encode!` | Keep | R-json-03 |
| `stringify_pretty` | `json.stringify_pretty(j: Value) -> String` | Go `MarshalIndent` / Python `dumps(indent=2)` (F) / serde `to_string_pretty` | Keep | R-json-03 |
| `get_string` | `json.get_string(j: Value, key: String) -> Option[String]` | Kotlin pkg `jsonPrimitive.contentOrNull` / serde `as_str()` | Remove → `value.get_string` | R-json-01 |
| `get_int` | `json.get_int(j: Value, key: String) -> Option[Int]` | serde `as_i64()` / Kotlin pkg `intOrNull` | Remove → `value.get_int` | R-json-01 |
| `get_float` | `json.get_float(j: Value, key: String) -> Option[Float]` | serde `as_f64()` / Kotlin pkg `doubleOrNull` | Remove → `value.get_float` | R-json-01 |
| `get_bool` | `json.get_bool(j: Value, key: String) -> Option[Bool]` | serde `as_bool()` / Kotlin pkg `booleanOrNull` | Remove → `value.get_bool` | R-json-01 |
| `get_array` | `json.get_array(j: Value, key: String) -> Option[List[Value]]` | serde `as_array()` / Kotlin pkg `jsonArray` | Remove → `value.get_array` | R-json-01 |
| `root` | `json.root() -> JsonPath` | serde `pointer("")` | Remove → `value.root` | R-json-01 |
| `field` | `json.field(path: JsonPath, name: String) -> JsonPath` | Elixir `Access.key` (F) | Remove → `value.key` | R-json-01 |
| `index` | `json.index(path: JsonPath, i: Int) -> JsonPath` | Elixir `Access.at` (F) | Remove → `value.index` (negative indexes out of bounds) | R-json-01 |
| `get_path` | `json.get_path(j: Value, path: JsonPath) -> Option[Value]` | Elixir `get_in` (F) / serde `pointer` | Remove → `value.get_path` | R-json-01 |
| `set_path` | `json.set_path(j: Value, path: JsonPath, value: Value) -> Result[Value, String]` | Elixir `put_in` (F) / serde `pointer_mut` | Remove → `value.set_path` | R-json-01 |
| `remove_path` | `json.remove_path(j: Value, path: JsonPath) -> Value` | Elixir `pop_in` (F) | Remove → `value.remove_path` | R-json-01 |
| `to_map` | `json.to_map(j: Value) -> Option[Map[String, String]]` | serde `as_object()` / Kotlin pkg `jsonObject` | Remove → `value.as_object(v) \|> result.map(map.from_list)` | R-json-01 |
