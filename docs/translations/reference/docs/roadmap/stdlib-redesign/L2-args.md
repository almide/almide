<!-- description: Stdlib redesign L2/L3 — args: a minimal flag reader over env.args; has_flag replaces flag (Bool naming and the first-letter collision), raw and option_or retire -->
<a id="l2l3-args"></a>

# L2/L3: args

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](L2-args.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](L2-args.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](L2-args.zh-CN.md)

> Non-normative historical reader snapshot; the pinned Japanese source is authoritative and is not automatically synchronized. Accepted proposals, measurements, cross-language comparisons and dates are retained as recorded, not freshly verified or a claim of current implementation. The header date precedes the later security-fix entry; both are preserved. The source shorthand positional_at(i) and the table signature with options are retained, not silently reconciled. [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-args.md)

> Last updated: 2026-10-21
> Prerequisites: [L0](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L0-conventions.md) and [L1](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L1-categories.md) (R-L1-25: argv itself belongs to `env`, while reading flags belongs to `args`. §3 excludes a full-scale flag parser.)
> Sources: Rust https://doc.rust-lang.org/std/env/fn.args.html (no standard flag parser); Go https://pkg.go.dev/flag; Python https://docs.python.org/3/library/argparse.html; TS/JS https://nodejs.org/api/util.html#utilparseargsconfig; Gleam https://gleam-stdlib.hexdocs.pm/ (none); Elixir https://hexdocs.pm/elixir/OptionParser.html; OCaml https://ocaml.org/manual/latest/api/Arg.html; Kotlin https://kotlinlang.org/api/core/kotlin-stdlib/ (none); Swift https://github.com/apple/swift-argument-parser (official package).
> Facts about the nine languages come from [facts/categories.md §3.4](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/facts/categories.md).

<a id="l2-役割の比較"></a>

## L2: Comparison of roles

| Role | Almide | Rust | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin | Swift |
|---|---|---|---|---|---|---|---|---|---|---|
| Raw argv | `args.raw` / `env.args` | `env::args` | `os.Args` | `sys.argv` | `process.argv` | — | `System.argv` | `Sys.argv` | `main(args)` | `CommandLine.arguments` |
| Boolean flag | `args.flag` | — | `flag.Bool` | `store_true` | `parseArgs` `type: 'boolean'` | — | `OptionParser` `:boolean` | `Arg.Set` | — | `@Flag` |
| Option taking a value | `args.option` / `args.option_or` | — | `flag.String` | `add_argument("--x")` | `parseArgs` `type: 'string'` | — | `OptionParser` `:string` | `Arg.String` | — | `@Option` |
| Positional argument | `args.positional` / `args.positional_at` | — | `flag.Args` / `flag.Arg(i)` | `add_argument("x")` | `parseArgs` `positionals` | — | Remainder from `OptionParser` | anon_fun in `Arg.parse` | — | `@Argument` |
| Help-text generation | — | — | ● | ● | — | — | — | ● | — | ● |

<a id="完全性の規則"></a>

### Completeness rules

- `args` is a minimal, schema-free reader (a full parser is excluded by L1 §3). Its roles are closed to four: Boolean flags, value options, and positional arguments (all or the ith).
- Only `env.args` owns raw argv (R-L1-25).
- Do not provide a default-value variant. Write `args.option(n)! ?? d` (R-L0-06).
- Intentionally absent: help-text generation and typed values (convert to `Int` with `int.parse`).

<a id="l2-の判定"></a>

### L2 findings

- Duplicate: `args.raw` merely calls `env.args()` once (forwarding under R-L0-01, counted as the same role) → R-args-02.
- Rule violation: `args.flag` returns Bool but uses neither `is_` nor `has_` (the exception explicitly named by R-L0-09) → R-args-01.
- Rule violation: `args.option_or` is an `_or` variant (R-L0-06) → R-args-02.
- Documentation disagreement: `docs/stdlib/args.md` says `args.raw` includes argv[0], but measurement excludes it (`raw=-v,--out,x,in.txt`, both native and wasm; probe-args). Correct this during the removal window → R-args-02.

<a id="案"></a>

## Proposals

<a id="r-args-01-argsflag-を-argshas_flag-にし頭文字の自動の短縮形をやめる---採用"></a>

### R-args-01 Rename `args.flag` to `args.has_flag` and stop automatically abbreviating to the first letter — ○ Accepted

- Content: add `effect fn has_flag(name: String) -> Bool`. It is true when `--name` exists. If `name` has one character, `-n` also makes it true. Do not derive short forms from the first letter. Remove `args.flag`.
- Measurement (probe-args): with `almide run a.almd -- -v`, both `args.flag("verbose")` and `args.flag("version")` become true. `-v` matches two names.
- Supporting L0 rule: R-L0-09 (Bool uses `is_`/`has_`; L0 explicitly names `args.flag` as an exception). Since semantics also change, move to a new name in accordance with R-L0-14 item 4 (new name for changed semantics).
- (a) No language automatically derives a short form from a long name. Go `flag` treats `-name` and `--name` alike; short forms are separately defined. Python, Node and Elixir also specify short forms explicitly (`-v` through `alias`/`short`).
- (b) It is pure computation over `env.args`. Native and wasm are identical (`wasi:cli/environment`).
- (c) `has_flag` indicates Bool in its name. Expressing a short form as `args.has_flag("verbose") or args.has_flag("v")` conflicts with no language's habits. The first-letter collision is an error an LLM cannot notice unless it writes tests.
- (d) Rename and semantic change. Compatibility: mark `args.flag` with `@deprecated(since=15, use="args.has_flag")`, `removal_epoch = 16`. `almide fix` rewrites `args.flag("verbose")` to `(args.has_flag("verbose")! or args.has_flag("v")!)`, preserving its current meaning exactly. Usage counts: 7/9/13/0.
- Proposed gate: remove `args.flag` from the allowlist in R-L0-G's Bool-name check. Add a first-letter collision fixture (`has_flag("version")` is false with `-v`).
- Issue: #3893.

<a id="r-args-02-argsraw-と-argsoption_or-を撤去する---採用"></a>

### R-args-02 Remove `args.raw` and `args.option_or` — ○ Accepted

- Content: `args.raw()` → `env.args()`. `args.option_or(n, d)` → `args.option(n)! ?? d`. Before removal, correct “argv[0] included” to “excluded” in `docs/stdlib/args.md`.
- Supporting L0 rules: R-L0-01 (forwarding that calls another public function once also counts as the same role), R-L0-06 (unify `_or` variants into `??`), R-L1-25.
- (a) Almide alone also returns raw argv from its flag-parsing module (facts §3.4). Default-value arguments exist, such as Python's `default=`, but no language splits them into separate functions.
- (b) No change.
- (c) README and args.md disagree about `args.raw`, so LLMs reading either get half of it wrong.
- (d) Removal. `@deprecated(since=15, use="env.args")` and `use="args.option(name)! ?? fallback"`, `removal_epoch = 16`. `almide fix` rewrites both. The replacement form for `option_or` (whether `!` is needed) follows the call type determined by R-L0-17. Usage counts: `args.raw` 1/4/11/0; `args.option_or` 6/4/8/0.
- Proposed gate: synonym gate rule 5 (#3851 F). R-L0-G rejects public functions ending in `_or`.
- Issue: #3851 B (raw); new R-L0-06 issue (option_or).

<a id="r-args-03-positional_at-を撤去する---採用"></a>

### R-args-03 Remove `positional_at` — ○ Accepted

- Current: `args.positional_at(i)` has the same value as `list.get(args.positional(options)!, i)`. R-args-02 removed `option_or` for the same reason: it can be written as a combination of other functions.
- Proposal: remove it and write `list.get(args.positional(options)!, i)`.
- Supporting L0 rule: R-L0-01 (do not put the same role in two places).
- (d) One removal. `@deprecated(since=15, use="list.get(args.positional(options)!, i)")`, `removal_epoch = 16`; rewrite with `almide fix`.
- Issue: #3979.

<a id="l3-関数表"></a>

## L3: Function table

| Function | Signature (current, mut shown) | Main spellings in other languages | Decision | Proposal |
|---|---|---|---|---|
| `args.raw` | `effect fn raw() -> List[String]` | Rust `env::args` / Go `os.Args` / Python `sys.argv` | Remove (→ `env.args`) | R-args-02 |
| `args.flag` | `effect fn flag(name: String) -> Bool` | Go `flag.Bool` / Python `store_true` / Elixir `:boolean` | Remove (→ `has_flag`) | R-args-01 |
| `args.has_flag` | `effect fn has_flag(name: String) -> Bool` (proposed) | Same as above | Add | R-args-01 |
| `args.option` | `effect fn option(name: String) -> String?` | Go `flag.String` / Python `add_argument("--x")` / Node `parseArgs` | Keep | — |
| `args.option_or` | `effect fn option_or(name: String, fallback: String) -> String` | Python `default=` | Remove (→ `option(n)! ?? d`) | R-args-02 |
| `args.positional` | `effect fn positional(options: List[String]) -> List[String]` | Go `flag.Args` / Node `positionals` | Keep (the 2026-10-26 security fix #4144, C-181, made it accept the names of options to read in `options`, so the value after `--name` no longer enters positional arguments. The earlier statement that option values are also included is withdrawn.) | — |
| `args.positional_at` | `effect fn positional_at(i: Int, options: List[String]) -> String?` | Go `flag.Arg(i)` | Remove (→ `list.get(args.positional(options)!, i)`) | R-args-03 |

<a id="l0-への差し戻し処理済み"></a>

## Returned to L0 (resolved)

- Although `args.option` and `env.get` have the same `effect fn … -> String?`, they used different forms of `??`: L0 R-L0-17 settled this (a call's type depends only on its declaration). Since their declarations match, their call forms match. Which form to standardize on is designed in the new R-L0-17 issue; this page does not change function names or declarations (L2-env R-env-03).
