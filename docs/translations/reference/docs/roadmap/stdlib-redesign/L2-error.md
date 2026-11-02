# L2/L3: error

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](L2-error.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](L2-error.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](L2-error.zh-CN.md)

> Non-normative source-pinned reader snapshot. The unchanged original remains authoritative for its consumers. No automatic synchronization or independent execution. Historical claims, accepted proposals, implementation status and measured evidence retain their original scope; this translation does not independently verify them. Inline code remains verbatim, including original-language string literals. The October 21 deprecation/removal plan, epoch numbers, usage counts, probe results and external comparisons are source-time records, not a current API inventory. Preserve the semantic change in chain separators and the inability of message to distinguish ok from an empty error; rejection of result.context remains conditional on its evidence-based falsifier. No source API is removed by these reader files. [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-error.md)

> Original metadata, inactive: Stdlib redesign L2/L3—error: retire context, message and chain, each a second spelling of a map_err / match idiom already fixed by ADR-0004, emptying the module.

> Last updated: 2026-10-21
> Prerequisites: [L0](../../../../../roadmap/stdlib-redesign/L0-conventions.md), [L1](../../../../../roadmap/stdlib-redesign/L1-categories.md), [L2-result](../../../../../roadmap/stdlib-redesign/L2-result.md), ADR-0003 / ADR-0004 / ADR-0012
> Sources: Rust https://doc.rust-lang.org/std/error/trait.Error.html, Go https://pkg.go.dev/errors, Python https://docs.python.org/3/library/exceptions.html, TS/JS https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/Error/cause, Gleam https://gleam-stdlib.hexdocs.pm/gleam/result.html (F), Elixir https://hexdocs.pm/elixir/Exception.html, OCaml https://ocaml.org/manual/latest/api/Result.html (F), Kotlin https://kotlinlang.org/api/core/kotlin-stdlib/kotlin/-result/ (F), Swift https://developer.apple.com/documentation/swift/error

`error` is a module of 3 functions, all handling “err strings.” Almide errors default to String (ADR-0002 D2, ADR-0004); ADR-0004 D2 establishes `result.map_err((e) => "文脈: ${e}")` as the canonical form for prefixing context.

<a id="l2-役割の比較"></a>

## L2: Comparison of roles

| Role | Almide | Rust | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin | Swift |
|---|---|---|---|---|---|---|---|---|---|---|
| Prefix context to err | `error.context` / `result.map_err` | `map_err` (std), `context` (anyhow, outside std) | `fmt.Errorf("..: %w", err)` | `raise X from e` | `new Error(msg, { cause })` | `map_error` (F) | — | `map_error` (F) | `recoverCatching` / exception `cause` | — |
| Connect 2 errors “with a cause” | `error.chain` | `source()` | `%w` / `errors.Join` | `__cause__` | `cause` | — | — | — | `cause` | — |
| Extract the err string | `error.message` | `to_string()` | `err.Error()` | `str(e)` | `e.message` | — | `Exception.message` | — | `message` | `localizedDescription` |
| Follow causes | — | `source()` | `errors.Unwrap` / `Is` / `As` | `__cause__` | `cause` | — | — | — | `cause` | — |

<a id="完全性の規則"></a>

### Completeness rules

- One spelling for prefixing context: `result.map_err((e) => "文脈: ${e}")` (separator `": "`, original error at the end). ADR-0004 D2 and Alternatives 7 reject the sugar `result.context` as “a second spelling for the same thing.”
- Do not provide tools for branching on error contents (ADR-0004 D4). Thus the “follow causes” role (such as Go's `errors.Is`) is intentionally absent.

<a id="l2-の判定"></a>

### L2 ruling

- 3 duplicates: `error.context` is a fixed pattern of `result.map_err`; `error.message` is `match` or `result.to_err_option(r) ?? ""`; `error.chain` is equivalent to string interpolation. Moreover, the separator of `error.chain` (`"\ncaused by: "`) differs from the canonical `": "` (confirmed in probe-error/p.almd).
- Conclusion: remove the 3 functions and close the module (R-error-01). The L1 table makes the same ruling: “remove the 3 functions of `error`, leaving the module empty.”

<a id="案"></a>

## Proposal

<a id="r-error-01-errorcontexterrormessageerrorchain-を撤去しerror-モジュールを閉じる---採用"></a>

### R-error-01 Remove `error.context`, `error.message`, and `error.chain`, and close the `error` module — accepted

- Content:
  - `error.context(r, msg)` → `@deprecated(since=15, use="result.map_err")`. Replacement: `r |> result.map_err((e) => "${msg}: ${e}")`. Same behavior (probe: both `error.context(err("boom"), "loading")` and the `map_err` version produce `err("loading: boom")`).
  - `error.message(r)` → `@deprecated(since=15, use="result.to_err_option")`. Replacement: `result.to_err_option(r) ?? ""`. However, recommend `match` when distinguishing ok from `err("")`. Current `error.message` returns `""` for both ok and `err("")` and cannot distinguish them (confirmed by probe).
  - `error.chain(outer, cause)` → remove. Replacement: `"${outer}: ${cause}"`. If the `"\ncaused by: "` separator is essential, write that string literally. The replacement is not a function, so put the spelling itself in `use=` as `@deprecated(since=15, use="\"${outer}: ${cause}\"")`; after removal, the hint in `removed_spellings.rs` shows the same form. `almide fix` rewrites `error.chain(a, b)` → `"${a}: ${b}"` (the separator changes, so fix lists the rewritten locations).
  - All 3 have `[[deprecation]]` blocks (`removal_epoch = 16`), with an E002 hint after removal. After removal, unregister `stdlib/error.almd`, `error_chain.almd`, `error_message.almd`, `runtime/rs/src/error.rs`, and the `STDLIB_MODULES` entry.
- Supporting L0 rules: R-L0-01 (one spelling per role; a forwarder that calls another public function once also counts as the same role), R-L0-02 (functions whose subject is Result belong in result), R-L0-14.
- (a) None of the 9 languages has a dedicated standard function for “prefix context.” Rust's `context` belongs to the external anyhow crate; std uses `map_err`. Go uses `fmt.Errorf` formatting, JS uses a `cause` argument: all use general-purpose tools.
- (b) No effect. All replacements are pure computations already available on both targets.
- (c) With both `error.context` and `result.map_err`, writers must remember the context separator (`": "`) and ok behavior for both. `error.chain` even uses a different separator. Since `error.message` does not distinguish ok and empty err, writing `if error.message(r) == "" then ..` misidentifies ok. ADR-0004 D3-(b)'s lint (omitting `${e}`, #1105) diagnoses the hazard in the `map_err` form.
- (d) Removal. Usage counts (including definitions and generated doc rows): `error.context` spec 4 / tests 17 / docs 11 / stdlib 0; `error.message` spec 5 / tests 22 / docs 10 / stdlib 1; `error.chain` spec 4 / tests 2 / docs 5 / stdlib 1. Neither CHEATSHEET nor `llms.txt` mentions them. `almide fix` rewrites `error.context(r, m)` → `r |> result.map_err((e) => "${m}: ${e}")` and `error.message(r)` → `result.to_err_option(r) ?? ""`.
- Gate: fixtures for post-removal E002 hints (3 cases). The R-L0-01 synonym gate (an extension of #3851 F) also detects functions that merely call another public function once, so it fails if such forwarding returns after removal.
- issue: #3909

<a id="r-error-02-errorcontext-を-resultcontext-として残す---不採用"></a>

### R-error-02 Retain `error.context` as `result.context` — rejected

- Content: do not adopt the proposal to fix only the location (R-L0-02) and name it `result.context`.
- Supporting L0 rule: R-L0-01. ADR-0004 Alternatives 7 already rejected `result.context` for this reason (#1104 closed as rejected).
- (c) The condition for reconsideration is ADR-0004 Falsifier 3 (Dojo measures frequent omissions of `${e}` despite the lint). That evidence does not yet exist.
- issue: none

<a id="l3-関数表"></a>

## L3: Function table

| Function | Signature (current, mut shown) | Main spellings in other languages | Ruling | Proposal |
|---|---|---|---|---|
| `error.chain` | `chain(outer: String, cause: String) -> String` | Go `fmt.Errorf("%w")` / JS `cause` / Python `from` | Remove | R-error-01 |
| `error.context` | `context(r: Result[T, String], msg: String) -> Result[T, String]` | Rust map_err (anyhow context is outside std) / Gleam and OCaml map_error | Remove | R-error-01, R-error-02 |
| `error.message` | `message(r: Result[T, String]) -> String` | Go `Error()` / JS `message` / Kotlin `message` (all on the exception/error-value side) | Remove | R-error-01 |

(3 functions. No additions.)

<a id="l0-への差し戻し処理済み"></a>

## Sent back to L0 (handled)

- **Sent back to L1**: the L1 table now says “the 3 `error` functions are duplicates or forwarders, so remove them and leave the module empty.” It agrees with this page.
- Treatment of forwarders: R-L0-01 now includes “a function that calls another public function once (a forwarder) also counts as the same role,” citing `error.context` as an example. The synonym gate also has a rule detecting such single calls to other public functions.
