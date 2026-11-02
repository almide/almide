> Description (fixed history): Stdlib redesign L2/L3 — regex: a compiled Regex type that stops bad patterns and the pipe swap, is_full_match, and the escape / position / all-captures / replace_with holes.
<a id="l2l3-regex"></a>

# L2/L3: regex

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](L2-regex.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](L2-regex.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](L2-regex.zh-CN.md)

> Non-normative historical redesign snapshot. Pinned original authoritative and unchanged; no automatic synchronization, fresh retrieval, independent probes/benchmarks/security testing or current API verification. (F), outputs and language comparisons are original-author claims. Preserve early String capture elements versus later optional elements, parse/compile chronology, and early shared pure engine versus later separate native/wasm backtracking account. The final 8+7 count and claim that all new first arguments are Regex conflict with the table and remain unchanged. Pike VM/linear-time statements and timing gates are accepted proposals, not newly measured ReDoS immunity. Preserved Japanese code placeholders mean reason and [whole match, group 1, ...]. [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-regex.md)

> Last updated: 2026-10-21
> Prerequisites: [L0](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L0-conventions.md) / [L1](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L1-categories.md)
> Sources: Rust https://docs.rs/regex/latest/regex/ (non-standard-library crate) / Go https://pkg.go.dev/regexp (F) / Python https://docs.python.org/3/library/re.html (F) / TS/JS https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/RegExp (F) / Gleam https://hexdocs.pm/gleam_regexp/ (non-standard-library package) / Elixir https://elixir.hexdocs.pm/Regex.html (F) / OCaml https://ocaml.org/manual/latest/api/Str.html (F) / Kotlin https://kotlinlang.org/api/core/kotlin-stdlib/kotlin.text/-regex/ (F) / Swift https://developer.apple.com/documentation/swift/regex

`regex` has 8 functions and requires `import regex`. Patterns are currently passed as strings, with no compiled type (R-regex-02 introduces `Regex`). The engine is pure Almide (`stdlib/regex_engine.almd`), so native and wasm produce the same results (probe-regex/p.almd was run on both targets, with matching output).

<a id="l2-役割の比較"></a>

## L2: Role comparison

| Role | Almide | Rust (regex) | Go | Python | TS/JS | Gleam (regexp) | Elixir | OCaml (Str) | Kotlin | Swift |
|---|---|---|---|---|---|---|---|---|---|---|
| Matches anywhere | `regex.is_match` | `is_match` | `MatchString` (F) | `search` (F) | `test` (F) | `check` | `match?` (F) | `search_forward` (F) | `containsMatchIn` (F) | `contains` |
| Matches the whole input | `regex.full_match` | `^…$` with `is_match` | `^…$` | `fullmatch` (F) | `^…$` with `test` | — | — | `string_match` + length | `matches` (F) | `wholeMatch` |
| First matching string | `regex.find` | `find` | `FindString` (F) | `search().group()` | `match` (F) | `scan` | `run` (F) | `matched_string` (F) | `find` (F) | `firstMatch` |
| All matches | `regex.find_all` | `find_iter` | `FindAllString` (F) | `findall` (F) | `matchAll` (F) | `scan` | `scan` (F) | — | `findAll` (F) | `matches` |
| Replace all (literal) | `regex.replace` | `replace_all` + `NoExpand` | `ReplaceAllLiteralString` (F) | `sub`(`\1` interpreted) | `replaceAll` | `replace` | `replace` (F) | `global_replace` (F) | `replace` (F) | `replacing` |
| Replace first only | `regex.replace_first` | `replace` | — | `sub(count=1)` | `replace` | — | `replace(global: false)` | `replace_first` (F) | `replaceFirst` (F) | — |
| Split on matches | `regex.split` | `split` | `Split` (F) | `split` (F) | `split` (F) | `split` | `split` (F) | `split` (F) | `split` (F) | `split` |
| Groups of first match | `regex.captures` | `captures` | `FindStringSubmatch` (F) | `search().groups()` | `exec` (F) | `scan` | `run` (F) | `matched_group` (F) | `find().groupValues` | `firstMatch().output` |
| Groups for all matches | — | `captures_iter` | `FindAllStringSubmatch` (F) | `finditer` (F) | `matchAll` (F) | `scan` | `scan` (F) | — | `findAll().groupValues` | `matches` |
| Match position | — | `find().start()` | `FindStringIndex` (F) | `search().start()` | `search` (F) | — | `run(return: :index)` | `search_forward` return value (F) | `find().range` | `firstMatch().range` |
| Escape special characters | — | `escape` | `QuoteMeta` (F) | `escape` (F) | `RegExp.escape`(ES2025) | — | `escape` (F) | `quote` (F) | `Regex.escape` | — |
| Replacement using groups (function) | — | `replace_all(closure)` | `ReplaceAllStringFunc` (F) | `sub(fn)` (F) | `replace(fn)` | `match_map` | `replace(fn)` (F) | `global_substitute` (F) | `replace(transform)` (F) | `replacing(with: closure)` |
| Replacement-string `$1` | — | `$1` | `$1` (F) | `\1` | `$1` | — | `\1` | `\1` | `$1` | — |
| Case-insensitive | — | `(?i)` | `(?i)` | `re.I` / `(?i)` | `i` flag | `case_insensitive` | `i` (F) | `regexp_case_fold` (F) | `IGNORE_CASE` | `ignoresCase()` |
| Named groups | — | `(?P<n>)` / `(?<n>)` | `(?P<n>)` / `SubexpNames` (F) | `(?P<n>)` only (F) | `(?<n>)` | — | `named_captures` (F) | — (F) | `groups["n"]` | `Capture` |
| Report invalid patterns | — (silently accepted) | `Regex::new` returns `Result` | `Compile` returns error (F) | `re.error` | `SyntaxError` | `compile` returns `Result` | `compile` returns `{:error, _}` (F) | Exception | Exception | `throws` |
| Argument order | Pattern first | Pattern as subject | Pattern as subject | Pattern first (F) | Pattern as subject | Pattern first | Pattern first (F) | Pattern first (F) | Pattern as subject | String as subject |

<a id="完全性の規則"></a>

### Completeness rules

- Pairing with the string module: `regex.replace`/`replace_first`/`split`/`find_all` share verbs and meanings with `string.replace`/`replace_first`/`split` (`replace` replaces all; empty fragments remain). Preserve this correspondence.
- Positions use code-point indices, like `string.index_of`.
- Predicates (Bool) begin with `is_` (R-L0-09).

<a id="l2-の判定"></a>

### L2 verdict

- R-L0-09 exception: `regex.full_match` (R-regex-01).
- Two silently incorrect cases (measured):
  1. Invalid patterns do not error. `regex.is_match("(", "abc")` is `true`; `regex.find("[", "abc")` is `none`; `(?i)ABC` fails to match `abc`, returning `false`; named groups `(?P<n>a)` / `(?<n>a)` produce `none` (probe-regex/p.almd and q.almd, identical on native and wasm). All 9/9 languages report invalid patterns (R-regex-02).
  2. Piping with `|>` puts the string in the pattern position. `"order 42" |> regex.is_match("[0-9]+")` is `false`; `|> regex.find(..)` is `none` (probe-regex/swap.almd). `"  a \t b\nc  " |> string.trim |> regex.split("\\s+")` also silently produces a different result (probe-string/e.almd). Both arguments are String, so current type checking cannot stop this. The `Regex` type will stop it (R-regex-02).
- Missing roles present in at least 5/9: escaping (7), match position (8), groups for every match (8), case insensitivity (9), replacement by a function (9), named groups (7). Add the first four (R-regex-04). Add function-based replacement as `replace_with` (R-regex-05). Do not add named groups (R-regex-06).

<a id="案"></a>

## Proposals

<a id="r-regex-01-regexfull_match-を-regexis_full_match-に改名する---採用"></a>

### R-regex-01 Rename `regex.full_match` to `regex.is_full_match` — ○ Accepted

- Content: add `regex.is_full_match(pat: String, s: String) -> Bool`; annotate `regex.full_match` with `@deprecated(since=15, use="regex.is_full_match")`. Set `removal_epoch = 16` in the `[[deprecation]]` block.
- Supporting L0 rules: R-L0-09 (an exception explicitly named by L0), R-L0-14.
- (a) Spellings vary (Python `fullmatch`, Kotlin `matches`, Swift `wholeMatch`). Alongside Almide's `regex.is_match`, `is_full_match` reads naturally.
- (b) No impact. Calls the same runtime body.
- (c) In the `is_match`/`full_match` pair, only one lacks `is_`, causing writers to write `regex.is_full_match` and get E002.
- (d) Rename. `almide fix` rewrites one-to-one. Usage counts: spec 69 / tests 1 / docs 7 / stdlib 0.
- Gate: remove `regex.full_match` from the R-L0-09 gate allowlist.
- Issue: #3940

<a id="r-regex-02-コンパイル済みの-regex-型を入れ不正なパターンを作る時点で止める---採用"></a>

### R-regex-02 Introduce a compiled `Regex` type and stop invalid patterns at construction — ○ Accepted

- Content:
  1. Add opaque type `Regex`, with two constructors.
     - `regex.new(pat: String) -> Regex`: for a string-literal pattern, check syntax during `almide check` and stop with a new E code if invalid (unsupported syntax, such as named groups, also stops with an “unsupported” message). An invalid runtime-assembled pattern aborts as a caller contract violation (R-L0-06 item 3, like Go's `regexp.MustCompile`).
     - `regex.compile(pat: String) -> Result[Regex, String]`: construct a runtime-assembled pattern with validation and reasons (R-L0-06 item 1; Go `regexp.Compile`, Rust `Regex::new`, Gleam `regexp.from_string`, Elixir `Regex.compile`).
  2. Matching, searching, replacement and splitting take `Regex` as their first argument (for example, `regex.is_match(re: Regex, s: String) -> Bool`). Names stay unchanged.
  3. The abort message is the single line `Error: invalid regex: <理由>` with exit code 1, identical on native and wasm (new contract C-NNN and fixture; pass through the als ledger first).
- Supporting L0 rules: R-L0-00 (9/9 have regex types), R-L0-02 (subject = `Regex`), R-L0-03, R-L0-06, R-L0-14.
- (a) All nine languages have compiled regex types (Rust `Regex`, Go `*regexp.Regexp`, Python `re.Pattern`, JS `RegExp`, Gleam `Regexp`, Elixir `Regex` with the `~r` marker, OCaml `Str.regexp`, Kotlin `Regex`, Swift `Regex`). Only Python's `re` module functions accept string patterns directly; even Python has `re.compile`.
- (b) The engine is pure Almide (`stdlib/regex_engine.almd`), so `Regex`, a value containing compiled state, works identically on both targets. No host functionality is needed.
- (c) Both measured “silently incorrect” cases are stopped by types.
  1. Invalid patterns (`regex.is_match("(", "abc")` yielding `true`, `(?i)ABC` failing to match, etc.) error at `regex.new`.
  2. The mix-up when piping with `|>` (`"order 42" |> regex.is_match("[0-9]+")` yielding `false`) becomes a type error for passing `String` in the `Regex` position. No declaration marker for the checker to read is needed (old R-regex-03 proposal).
- (d) Argument type change. An old call `regex.find("[0-9]+", s)` becomes a type error, not a silent meaning change. The hint is `regex.find(regex.new("[0-9]+"), s)` (R-L0-14 item 4 forbids changing meaning while types still match; that does not apply here). `almide fix` mechanically wraps string-literal patterns in `regex.new(...)`. Do not implicitly convert `String` to `Regex` (that would create two ways to write it). Include this in epoch 15 `breaks`, recording spec (about 500 locations), tests and downstream counts in `summary`.
- Speed: currently each call parses the pattern. Constructing `let re = regex.new(p)` once and using it in a loop reduces parsing to one time.
- Gate: invalid-pattern matrix fixtures (`(`, `[`, `*`, `a{`, `(?P<n>a)`) check three outcomes: check-time error, `regex.new` abort and `regex.compile` err. Check native/wasm agreement using `spec/wasm_cross/` fixtures. In `tests/diagnostics/`, place a broken example piping a string into the pattern position and a fixed example using `regex.new`.
- Issue: #3942

<a id="r-regex-03-パイプの取り違えを宣言の印で止める---不採用r-regex-02-の型で止まるため"></a>

### R-regex-03 Stop pipe mix-ups using declaration markers — × Rejected (R-regex-02 types stop them)

- Reason: markers and separate types both make the mixed-up call a compile error, so survival rate is unchanged. There is no reason to add a new language mechanism for that. R-regex-02 makes the first argument `Regex`, so a mix-up such as `s |> regex.find(re)` is a type error. Do not add a new mechanism in which arguments are marked “no piping” for the checker to read (separate types can stop the same mix-up in other modules too).

<a id="r-regex-04-エスケープ位置全一致のグループ大文字小文字の区別なしを足す---採用"></a>

### R-regex-04 Add escaping, positions, all-match groups and case insensitivity — ○ Accepted

- Content:
  - `regex.escape(s: String) -> String`: prepend `\` to special characters.
  - `regex.index_of(re: Regex, s: String) -> Option[Int]`: start position of the first match (code-point index). Same spelling and units as `string.index_of`.
  - `regex.captures_all(re: Regex, s: String) -> List[List[String]]`: `[全体, グループ1, ...]` for each match.
  - Accept leading `(?i)` in patterns as the case-insensitive marker (no new function).
- Supporting L0 rules: R-L0-01 (`index_of` shares string's spelling), R-L0-03, R-L0-12 (position units).
- (a) Escaping 7/9, positions 8/9, all-match groups 8/9, case insensitivity 9/9 (many marked (F)).
- (b) Pure computation.
- (c) Without `regex.escape`, patterns embedding user input silently acquire different meanings.
- (d) Addition. Usage count 0.
- Gate: property tests check consistency between `captures_all` and `captures`/`find_all`. Add `index_of` to the R-L0-12 boundary-input matrix.
- Issue: #3941

<a id="r-regex-05-一致ごとに関数で置換文字列を作る-regexreplace_with---採用"></a>

### R-regex-05 `regex.replace_with`, constructing replacement strings with a function per match — ○ Accepted

- Content: `regex.replace_with(re: Regex, s: String, f: (List[String]) -> String) -> String`. `f` receives `[全体, グループ1, ...]`. Like `replace`, replaces all matches.
- Supporting L0 rule: R-L0-10 (`_with` = version accepting a function instead of a value).
- (a) All 9/9 languages have it (Rust `replace_all(closure)`, Go `ReplaceAllStringFunc`, Python `sub(fn)`, JS `replace(fn)`, Elixir `replace(fn)`, Kotlin `replace(transform)`, etc.).
- (b) Pure computation.
- (d) Addition.
- Do not interpret `$1` in replacement strings (× Rejected). Current `regex.replace` treats `$1` literally (probe: `replace("(a)", "xax", "$1$1")` → `x$1$1x`). Changing interpretation would silently change existing replacements containing `$`, violating R-L0-14 item 4. Use `replace_with` for replacements using groups.
- Issue: #3941

<a id="r-regex-06-名前つきグループを足さない---不採用"></a>

### R-regex-06 Do not add named groups — × Rejected

- Content: do not add `(?<name>...)` or name-based extraction APIs.
- (a) Present in 7/9, but syntax varies (Python only `(?P<n>)` (F); JS `(?<n>)`).
- (c) This creates a second form for the same role as positional `captures` (R-L0-01). When named syntax is written, R-regex-02's `regex.new` stops with “unsupported”, so it does not silently go wrong.
- Issue: none

<a id="r-regex-07-replace_first_with-を足して置換の-22-を閉じる---採用"></a>

### R-regex-07 Add `replace_first_with` to complete the replacement 2×2 — ○ Accepted

- Current: `replace` (all/string), `replace_first` (first/string), `replace_with` (all/function). The first-only/function cell is missing.
- Proposal: `fn regex.replace_first_with(re: Regex, s: String, f: (List[String]) -> String) -> String`.
- Supporting L0 rules: CLAUDE.md “expand families as matrices”, R-L0-10 (`_with`).
- (a) Five languages: Rust regex library `replace` (function), Python `sub(fn, s, count=1)`, JS `replace(re, fn)`, Elixir `Regex.replace(..., global: false)`, OCaml `Str.substitute_first`.
- (c) An LLM knowing `replace_first` and `replace_with` writes `replace_first_with` by the rule.
- (d) Add 1.
- Completeness rule: replacement has four cells, {all, first} × {string, function}.
- Proposed gate: property test `replace_first_with(re, s, (g) => rep) == replace_first(re, s, rep)`.
- Issue: #3971

<a id="r-regex-08-捕捉グループを-string-で返す---採用"></a>

### R-regex-08 Return capture groups as `String?` — ○ Accepted

- Current: `captures` is `Option[List[String]]`, using `""` for a group that did not participate (L3 table note). The first group when matching `(a)?(b)` against `"b"` cannot be distinguished from the empty match in `(a*)(b)`.
- Proposal: group elements become `String?`.
  - `fn captures(re: Regex, s: String) -> List[String?]?`
  - `fn captures_all(re: Regex, s: String) -> List[List[String?]]`
  - Callbacks for `replace_with`/`replace_first_with` also become `(List[String?]) -> String`.
  - Element 0 (the whole match) is always some.
- Supporting L0 rules: R-L0-06 (do not return the failure-marker value `""`), R-L0-24.
- (a) Rust `Captures::get` → `Option<Match>`, Python `groups()` → `None`, JS `exec` → `undefined`, Kotlin `groups[i]` → `null`, Swift `Regex` output → `Substring?`. Only Go returns `""` (position functions distinguish it with -1).
- (b) Both native and wasm regex implementations carry the nonparticipation marker through unchanged.
- (d) Semantic change (element types change, so string usage reveals it as a type error). Introduce `captures` in the same epoch as the first-argument change in R-regex-0x.
- Issue: #3977

<a id="r-regex-09-対象の文字列を第-1-引数にする---不採用"></a>

### R-regex-09 Put the target string first — × Rejected

- Reason: the subject of the `regex` module is `Regex`, following R-L0-02/R-L0-03 (place functions in the subject type's module and put the subject first). Rust `re.replace_all(s, …)`, Go `re.ReplaceAllString(s, …)` and Elixir `Regex.replace(re, s, …)` also put the regex first. Use `string.replace` to write from the string side.
- Issue: none

<a id="r-regex-11-パターンを確かめて作る関数を-regexcompile-と綴る---採用"></a>

### R-regex-11 Spell the validated pattern constructor `regex.compile` — ○ Accepted

- Content: spell R-regex-02's added `regex.parse(pat) -> Result[Regex, String]` as `regex.compile` (everything else stays as in R-regex-02).
- Reason: in the protocol hierarchy, `parse` reads a value's string representation that round-trips through `to_string` (`Parse`, Swift `LosslessStringConvertible`). Assembling a pattern has a different role. Go `regexp.Compile`, Python `re.compile` and Elixir `Regex.compile` use `compile`.
- (d) Rename a function added by this redesign.
- Issue: #4004

<a id="r-regex-12-一致の照合を線形時間にする---採用"></a>

### R-regex-12 Make matching linear-time — ○ Accepted

- Current: both native (`runtime/rs/src/regex.rs`) and wasm (`stdlib/regex_engine.almd`) use backtracking without a step limit. Passing `"aaaa…a!"` to `(a+)+$` makes time grow exponentially with input length.
- Proposal: use a Pike VM (advance Thompson NFAs concurrently, with capture groups), making time proportional to pattern size × input length. Almide regexes have neither backreferences nor lookahead/lookbehind (`docs/stdlib/regex.md`), so, as with RE2, Go and Rust, replacement can preserve results (leftmost prioritized matches and capture groups).
- Supporting L0 rule: R-L0-47 item 1.
- (a) Go `regexp`, Rust `regex` and RE2 are linear-time. Python, JS, Kotlin and Swift use backtracking, from which many ReDoS vulnerabilities arise.
- (b) Use one body (self-hosted Pike VM) on native/wasm, or two semantically equivalent implementations; verify that all existing regex cross fixtures return the same answers.
- (d) No semantic change (same match results; only time changes).
- Proposed gate: fixtures with pathological patterns (`(a+)+$`, `(a|aa)*b`) and 100,000-character inputs finish within a fixed time.
- Issue: #4019

<a id="l3-関数表"></a>

## L3: Function table

| Function | Signature (current, mut shown) | Main spellings in other languages | Verdict | Proposal |
|---|---|---|---|---|
| `regex.is_match` | `is_match(pat: String, s: String) -> Bool` | Rust is_match / Go MatchString / Python search / JS test / Kotlin containsMatchIn | Semantic change (first argument becomes `Regex`) | R-regex-02 |
| `regex.full_match` | `full_match(pat: String, s: String) -> Bool` | Python fullmatch / Kotlin matches / Swift wholeMatch | Rename (`is_full_match`; first argument is `Regex`) | R-regex-01、R-regex-02 |
| `regex.find` | `find(pat: String, s: String) -> Option[String]` | Rust find / Go FindString / Kotlin find / Swift firstMatch | Semantic change (first argument becomes `Regex`) | R-regex-02 |
| `regex.find_all` | `find_all(pat: String, s: String) -> List[String]` | Python findall / Go FindAllString / Kotlin findAll / Elixir scan | Semantic change (first argument becomes `Regex`) | R-regex-02 |
| `regex.replace` | `replace(pat: String, s: String, rep: String) -> String` | Rust replace_all / Go ReplaceAllLiteralString / JS replaceAll / Kotlin replace | Semantic change (first argument becomes `Regex`) | R-regex-02、R-regex-05(`$1` is not interpreted) |
| `regex.replace_first` | `replace_first(pat: String, s: String, rep: String) -> String` | Rust replace / Kotlin replaceFirst / OCaml replace_first | Semantic change (first argument becomes `Regex`) | R-regex-02 |
| `regex.split` | `split(pat: String, s: String) -> List[String]` | All languages: split | Semantic change (first argument becomes `Regex`) | R-regex-02 |
| `regex.captures` | `captures(re: Regex, s: String) -> Option[List[Option[String]]]` | Rust captures / Go FindStringSubmatch / JS exec / Elixir run | Semantic change (first argument becomes `Regex`; groups are `String?`) | R-regex-08 |
| `regex.is_full_match` | `is_full_match(re: Regex, s: String) -> Bool` | (same as full_match above) | Add | R-regex-01 |
| `regex.new` | `new(pat: String) -> Regex` | Go MustCompile / Rust Regex::new / Kotlin Regex() / JS RegExp / Swift Regex | Add | R-regex-02 |
| `regex.compile` | `compile(pat: String) -> Result[Regex, String]` | Go Compile / Rust Regex::new / Gleam from_string / Elixir compile | Add | R-regex-02 |
| `regex.replace_with` | `replace_with(re: Regex, s: String, f: (List[Option[String]]) -> String) -> String` | Rust replace_all(closure) / Go ReplaceAllStringFunc / Python sub(fn) / JS replace(fn) | Add | R-regex-05、R-regex-08 |
| `regex.escape` | `escape(s: String) -> String` | Rust escape / Go QuoteMeta / Python escape / Elixir escape / OCaml quote | Add | R-regex-04 |
| `regex.index_of` | `index_of(re: Regex, s: String) -> Option[Int]` | Go FindStringIndex / JS search / Rust find().start() | Add | R-regex-04 |
| `regex.captures_all` | `captures_all(re: Regex, s: String) -> List[List[Option[String]]]` | Rust captures_iter / Go FindAllStringSubmatch / Elixir scan | Add | R-regex-04、R-regex-08 |
| `regex.replace_first_with` | `replace_first_with(re: Regex, s: String, f: (List[Option[String]]) -> String) -> String`(proposed) | Python `sub(fn, s, count=1)` / JS `replace(re, fn)` / OCaml `substitute_first` | Add | R-regex-07、R-regex-08 |

(Current 8 functions + 7 added rows. All added functions take `Regex` as the first argument.)

Relation to R-L0-05: all regex functions are pure and return values; misuse as statements is rare. Nothing specific to note.

<a id="l0-への差し戻し処理済み"></a>

## Referrals back to L0 (resolved)

- Suffix for “constructing a value with a one-argument function”: R-L0-10 defines `_with` as “a version accepting a function instead of a value”, hence `regex.replace_with` (R-regex-05).
- Declaration markers to stop pipe mix-ups: not added. Separate types stop them (R-regex-02/R-regex-03).
