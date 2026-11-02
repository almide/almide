> Source description: Stdlib redesign L2/L3 — bool: a new module holding bool.parse, bool.to_string and bool.compare (Bool is ordered, as in seven of the nine languages)
<a id="l2l3-bool"></a>

# L2/L3: bool

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](L2-bool.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](L2-bool.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](L2-bool.zh-CN.md)

> Non-normative pinned reader snapshot. The original remains authoritative; this copy is not automatically synchronized and has not independently revalidated implementations, measurements, or safety.
> [Source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-bool.md)
> This is the source-time redesign decision record. Accepted additions and cross-language comparisons are not independently verified current implementation claims. The ninth/eleventh-pass change of decision and all three additions remain historical.


> Last updated: 2026-10-22
> Prerequisites: [L0](../../../../../roadmap/stdlib-redesign/L0-conventions.md), [L1](../../../../../roadmap/stdlib-redesign/L1-categories.md) (R-L1-30)
> Sources: Rust https://doc.rust-lang.org/std/primitive.bool.html · Go https://pkg.go.dev/strconv#ParseBool · Python — (`bool("false")` is True) · TS/JS — (`JSON.parse`) · Gleam — (not in the standard library) · Elixir — (`String.to_existing_atom`) · OCaml https://ocaml.org/manual/latest/api/Stdlib.html#VALbool_of_string_opt · Kotlin https://kotlinlang.org/api/core/kotlin-stdlib/kotlin.text/to-boolean-strict-or-null.html · Swift https://developer.apple.com/documentation/swift/bool/init(_:)-4dsgl

<a id="l2-役割の比較"></a>

## L2: Comparing roles

| Role | Almide | Rust | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin | Swift |
|---|---|---|---|---|---|---|---|---|---|---|
| Read from a string (fail if invalid) | `bool.parse` | `parse::<bool>` | `ParseBool` | — | — | — | — | `bool_of_string_opt` | `toBooleanStrictOrNull` | `Bool(s)` |
| Compare (three-way) | `bool.compare` (R-bool-02) | `cmp` (`Ord`) | — | `<` (`False < True`) | — | `bool.compare` | `<` (term ordering) | `Bool.compare` | `compareTo` | — (does not conform to `Comparable`) |
| Convert to a string | `bool.to_string` (`Repr`; R-bool-03) | `to_string` | `FormatBool` | `str` | `String` | `bool.to_string` | `to_string` | `string_of_bool` | `toString` | `description` |

<a id="完全性の規則"></a>

### Completeness rules

1. The `parse` family: fundamental types readable from strings (`Int`, `Float`, fixed-width numbers, and `Bool`) each have `<mod>.parse(s) -> Result[T, String]`.
2. The `compare` family (R-L0-16): `Bool` is an ordered type (`Ord`), with `false < true`. Provide `bool.compare(a, b) -> Int` (R-bool-02).
3. Deliberate omissions: no functions such as `not` or `and` (use the `not`, `and`, and `or` operators). No `to_int` either (it can be written as `if b then 1 else 0`; only Gleam and OCaml have a dedicated function).

<a id="案"></a>

## Proposals

<a id="r-bool-01-boolparse-を足す---採用"></a>

### R-bool-01 Add `bool.parse` — ○ Accepted

- Content: `fn bool.parse(s: String) -> Result[Bool, String]`. `"true"` → `ok(true)`, `"false"` → `ok(false)`, anything else is err. As specified in R-L1-30.
- (b) Pure computation. The same on both targets.
- (d) A new module and one addition. Put `bool`, like `int`, in the list available without imports (`AUTO_IMPORT_BUNDLED`).
- issue: #3980

<a id="r-bool-02-boolcompare-を足す---採用十一巡目に保留から戻す9-言語中-7-つが順序をつける"></a>

### R-bool-02 Add `bool.compare` — ○ Accepted (restored from hold in the eleventh pass; seven of nine languages order booleans)

- Content: `fn bool.compare(a: Bool, b: Bool) -> Int`. In the order `false < true`, return negative if smaller, 0 if equal, positive if greater.
- (a) Rust (`Ord`), Gleam, OCaml, and Kotlin have it.
- (b) Pure computation. The same on both targets. It matches the current `<` and `list.sort` ordering (`false` first).
- (c) R-L0-16 says “provide `compare` for ordered types,” but `Bool` was missing from its list. R-L1-30 creates the `bool` module, so fill that gap here.
- Implementation: do not write a separate function. It is the `compare` of `Bool`'s `Ord` itself (the completeness rule in R-L0-22). `a.compare(b)` and `bool.compare(a, b)` are the same function.
- (d) One addition.
- Decision history: in the ninth pass, when organizing the protocol hierarchy following Swift, this was put on hold because Swift's `Bool` is not `Comparable`. Comparing the nine languages again in the eleventh pass, seven impose an order: Rust (`Ord`), Python (`False < True`), TS/JS (compare after numeric conversion), Gleam (`bool.compare`), Elixir (term ordering), OCaml (`Bool.compare`), and Kotlin (`Comparable`). The two that do not are Go (`<` is a compile error) and Swift. Decide individual types' behavior by the majority (R-L0-00). Swift guides the hierarchy's skeleton, but which type belongs to which protocol is decided by a per-type majority.
- (c) Almide currently accepts `false < true` and can sort `List[Bool]` with `list.sort`. Removing it from `Ord` would break code that currently passes.
- issue: #3980

<a id="r-bool-03-boolto_string-を置く---採用"></a>

### R-bool-03 Provide `bool.to_string` — ○ Accepted

- Content: `fn bool.to_string(b: Bool) -> String`. Returns `"true"` or `"false"` and round-trips with `bool.parse`. It is the `Repr` function of R-L0-38; its contents are the same as `"${b}"`.
- (a) All nine languages have a function form (Rust `to_string`, Go `FormatBool`, Python `str`, TS/JS `String`, Gleam `bool.to_string`, Elixir `to_string`, OCaml `string_of_bool`, Kotlin `toString`, Swift `description`). Swift pairs `Bool(s)` and `description` in `LosslessStringConvertible`.
- Revise the first-pass decision “do not provide it because `Repr` exists” (under R-L0-38, it becomes the `Repr` function itself, with no separate implementation).
- (d) One addition.
- issue: #3980

<a id="l3-関数表"></a>

## L3: Function table

| Function | Signature (current, mut shown) | Main spellings in other languages | Decision | Proposal |
|---|---|---|---|---|
| `parse` | (addition) `bool.parse(s: String) -> Result[Bool, String]` | Rust `parse::<bool>` / Go `ParseBool` / Kotlin `toBooleanStrictOrNull` | Add | R-bool-01 |
| `compare` | (addition) `bool.compare(a: Bool, b: Bool) -> Int` | Rust `cmp` / Gleam `bool.compare` / OCaml `Bool.compare` / Kotlin `compareTo` | Add | R-bool-02 |
| `to_string` | (addition) `bool.to_string(b: Bool) -> String` | Rust `to_string` / Go `FormatBool` / Swift `description` | Add | R-bool-03 |
