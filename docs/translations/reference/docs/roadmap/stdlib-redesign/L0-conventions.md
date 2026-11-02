Reader description: Stdlib redesign layer 0 — the cross-module naming, placement, mutation, failure and conversion rules every L1/L2/L3 verdict derives from.
<a id="l0-全モジュールに共通する規則"></a>

# L0: Rules common to all modules

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](L0-conventions.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](L0-conventions.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](L0-conventions.zh-CN.md)

> Non-normative fixed-source historical redesign reader. Original pinned and unchanged; no automatic synchronization, new measurements, implementation changes or gate execution. Accepted/canceled/held decisions and dated implementation claims are those of the source, not independently confirmed current guarantees. Preserve successive revisions: source-side to versus destination from; R20 cancellation; R21 none versus later abort; negative clamping versus R29 abort; canceled R41 versus R43 retaining composition; intended protocol hierarchy versus dated implemented public generic APIs. Original counts, the three-mechanisms/four-row table, and remaining contradictory formulations are retained, not reconciled. Safety, linear-time and foreign-language vulnerability assertions are source claims, not new proofs or fresh advisory verification. F marks denote the original research. Preserved placeholders mean: 型 type, 元 source, 先の型 destination type, 値 value, 次の状態 next state, 残り remainder, 何 what, 表現 representation, 変種 variant. Historical commands are not new instructions. [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L0-conventions.md)

> Last updated: 2026-10-21
> Evidence table: [facts/naming-conventions.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/facts/naming-conventions.md) (facts collected from 9 languages' official documentation; (F) marks material retrieved and checked in this round)
> Status index: [README.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/README.md)

Redesigning 999 standard functions (44 modules), we prioritize overall consistency.
This page's rules are the “higher-layer decisions”; every L1 (category), L2 (module contents), L3 (individual function) verdict derives from them.
No special treatment by individual function name. Rules must be mechanically decidable from one declaration: signature, return type, `mut` declarations, etc.

Reading conventions:
- `mut` (read “mute”) = an argument that rewrites the caller's `var` in place
- in-place (mutation at the same location) / copy (returns a new value)
- `??` (read “null coalescing”) = operator using the right operand when the left is none / err
- (a)–(d) = 4 evaluation axes: (a) gaps/excesses relative to other languages, (b) validity on both native/wasm, (c) ease of correct LLM spelling, (d) change category and compatibility

---

<a id="改名してよい条件全規則の前提"></a>

## Conditions permitting renaming (prerequisite for all rules)

**R-L0-00 — ○ Accepted**

Change a spelling only if either condition holds:

1. **An internal Almide collision.** Two spellings for the same role, or one verb with different meanings across modules.
2. **A majority of the 9 languages (at least 5) agrees on another spelling**, and Almide's spelling has no language-specific reason.

Keep existing spellings where there is no majority (e.g. length `len`/`length`/`size`/`count`, uppercase `to_upper`/`uppercase`).

- (a) No complete match to any of the 9 languages. Still, “follow the majority; do not move if split” minimizes renaming.
- (c) Every change to a spelling already present in LLM training data creates a period mixing old and new. Unnecessary renaming itself causes writing errors.
- (d) This rule is not itself a change.

---

<a id="r-l0-01-同じ役割には綴りを-1-つモジュールをまたいでも---採用"></a>

## R-L0-01 One spelling per role (even across modules) — ○ Accepted

Apply CHEATSHEET's “No synonyms” across module boundaries. Current gate `tests/stdlib_synonym_gate_test.rs` looks only within each module.
Retain the one “in the subject type's module” (R-L0-02).

- (a) Other languages also duplicate (Python `math.fabs` and `abs`, etc.). Still, no language's design guidance recommends two copies of the same computation in its standard library.
- (b) No impact; both spellings call the same runtime body.
- (c) Two spellings invite confusion when only one differs at the boundaries (`env.temp_dir` and `fs.temp_dir` are an example).
- (d) Removal; compatibility under R-L0-14.
- “Same role” includes functions calling another public function just once (forwarding), e.g. `error.context` merely adds a prefix through `result.map_err`.
- Type-matrix cells (e.g. `bytes.append_i16_le` and `append_u16_le`, differing only on the type axis) are not synonyms even with identical runtime bodies: the type axis itself is meaningful. But the matrix shape must be declared and gate-protected (CLAUDE.md “Expand API families as matrices”).
- Use the same part of speech for roles with the same shape. Functions splitting into n-item groups use singular verb `chunk` (`list.chunk`; rename `bytes.chunks`).
- Gate: extend the synonym gate to detect identical signatures/runtime bodies across modules, different intrinsics sharing a body within a module, and functions merely calling another public function once (extends #3851 F). Allowlist changes may only reduce it.
- Issue: #3851 (existing).

<a id="r-l0-02-関数は主語の型のモジュールに置く---採用"></a>

## R-L0-02 Functions belong in the subject type's module — ○ Accepted

Place functions in the module of their first argument (subject) type. Subjectless constructors (`map.new`, `list.range`) belong in the result type's module.
`math` contains only mathematical functions lacking a home in a type module. Boundary: “roles also present for `Int` (`abs`, `min`, `max`, `clamp`, `sign`, `pow`) belong in type modules; real-only mathematics (`sqrt`, `cbrt`, `hypot`, trigonometric/exponential/logarithmic/gamma), plus `factorial` and `choose`, belong in `math`; rounding/predicates/constants/bit patterns/conversions in `float`” (revised to this after reconsidering `sqrt` placement in R-math-03).
Functions where container element type carries meaning (`option.collect` taking `List[Option[T]]`, `result.partition` taking `List[Result[T, E]]`) belong in the element type's module.

- (a) A 7/9 majority places functions in the subject type's module or methods (Rust/Kotlin/Swift methods; Gleam/Elixir/OCaml type modules). Go and Python `math` are exceptions.
- (c) With two possible locations, e.g. `int.abs` and `math.abs`, an LLM cannot predict which contains the function.
- (d) A move is removal plus addition, under R-L0-14.
- Gate: verify that a public function's first argument or return type is the module's subject type. Exception allowlists may only shrink.
- Issue: #3949.

<a id="r-l0-03-引数の順序は主語を先頭にする---据え置きゲート"></a>

## R-L0-03 Put the subject first in argument order — ○ Retain + gate

The `|>` (“pipe”) in `xs |> list.map(f)` inserts at the first argument. All current 999 functions follow this order.

- (a) 7/9 languages put data first (Rust, Go, JS, Gleam, Elixir, Kotlin, Swift). Only OCaml Stdlib and Python `map`/`filter` put the function first.
- Gate: the same test as R-L0-02 checks the subject-typed argument is first.
- Issue: #3949.

<a id="r-l0-04-コピーが既定その場書き換えは決まった動詞だけ---採用"></a>

## R-L0-04 Copy by default; only a fixed set of verbs mutates in place — ○ Accepted

**Rules:**
1. Functions return copies by default (`list.sort`, `list.set`, `map.set`, `set.insert`). To update a variable, write `xs = list.set(xs, i, v)`. Functions marked `@reuse` operate in place without copying even in this form (2 million map entries: `map.set`, `map.insert`, indexed assignment all approximately 0.07 seconds, measured).
2. Functions taking `mut` arguments (in-place mutation) are restricted to these verbs: buffer growth, truncation, and positional writing that cannot be expressed equally fast in copy form.
   - `push` (append one), `pop` (remove one from end), `clear` (empty), `fill` (set all to one value)
   - For double-ended containers, `push_front`/`pop_front` with a front qualifier (`deque`; end uses `push`/`pop`, R-deque-03). Count as the same family as `push`/`pop`
   - `append_<型>` (append bytes), `put_<型>` (overwrite bytes at a position)
   - `copy_from`, `copy_within` (copy byte ranges)
3. A verb denotes the same mode (copy or in-place) in all modules.
4. Provide `clear` for every container type (list/map/set/deque/heap/string/bytes). It empties while retaining allocation and cannot be written as fast by copying (addition to set/deque/heap decided in second-round review; R-deque-02, R-heap-02). `set`, `insert`, `remove`, `delete`, `update` are copying verbs.

**Consequences (individual L2 decisions):**
- Remove `map.insert(mut m, k, v)` and `map.delete(mut m, k)`. `set.insert` and `list.insert` copy, while only `map.insert` mutates. `m[k] = v` and `m = map.remove(m, k)` can be written equally fast.
- Rename `bytes.set_<型>(mut b, pos, v)` to `bytes.put_<型>`. `list.set`, `map.set`, `bytes.set` copy. `set_at` duplicates `b[i] = v`.

**Reasons:**
- (a) The 9 languages split into 3 styles: name distinction (Kotlin `sort`/`sorted`, Swift `-ed`/`form`), type/return-value distinction (Rust, Go, OCaml), no mutation at all (Gleam, Elixir). Almide has value semantics (copy on assignment) and functional style. Copy-by-default and copying `sort` match Gleam/Elixir/OCaml List. On top of that, incorporate Swift/Kotlin's name distinction through a closed verb set.
  - `append_<型>` and `put_<型>` match Go `encoding/binary`'s pair `AppendUint32` (grow) and `PutUint32` (positional overwrite). https://pkg.go.dev/encoding/binary
- (b) No impact; in-place mutation already has both native `&mut` and wasm write-back paths.
- (c) Currently `insert` mutates map but copies set/list. The verb does not reveal the mode; as R-L0-05 below shows, confusion does not even cause an error.
- (d) Renaming and removal, under R-L0-14.
- Gate: inspect declarations to ensure public functions with `mut` arguments use the above verbs and those verbs are not used for non-`mut` functions (interface JSON `mut` field, #3889).
- Issue: #3928.

<a id="r-l0-05-unit-以外の値を文として捨てたらエラーにする---採用"></a>

## R-L0-05 Error when discarding a non-Unit value as a statement — ○ Accepted

**Measurement (2026-10-21, develop after v0.68.0-rc2):**

```almide
fn main() -> Unit = {
  var s = set.new()
  set.insert(s, 1)        // エラーにならない。何も起きない
  println(int.to_string(set.len(s)))   // 0 と表示される
}
```

Statements such as `list.sort(xs)` or `string.trim(s)` also silently pass.
Current E042 rejects only discarded `Result` statements. Expand to “all non-Unit values.” To discard intentionally, keep writing `let _ = f()`.

- (a) Swift warns on unused returns by default (opt out with `@discardableResult`). Rust warns on types/functions marked `#[must_use]`. Gleam warns on unused pure expressions.
- (b) Checker-only change, identical on both targets.
- (c) Even after R-L0-04 aligns verbs, LLMs use copying functions as mutators from habits learned in other languages. This rule catches it during checking. Two layers: naming rules and checking insurance.
- (d) Language change; include in dialect epoch 15 `breaks`. Before landing, count affected sites in spec/examples/stdlib/research/tests/diagnostics and 44 downstream projects.
- Gate: new E042 fixtures (broken/fixed) plus diagnostic harness.
- Issue: #3906.

<a id="r-l0-06-無印の名前は安全版既定値版-_or-は--に一本化する---採用"></a>

## R-L0-06 Unsuffixed names are safe variants; unify default-value `_or` on `??` — ○ Accepted

1. Fallible retrieval returns `Option` under an unsuffixed name (`list.get`, `map.get`, `list.first`). Failure with a reason returns `Result[_, String]` (`int.parse`, `fs.read_text`).
2. Remove default-argument `_or` forms (`list.get_or`, `map.get_or`, `bytes.get_or`, `args.option_or`), as they match `xs |> list.get(i) ?? d`. Extends ADR-0005's shift of `option.unwrap_or` and `result.unwrap_or` to `??`.
3. Stop processing (abort) only for caller contract violations (e.g. `list.chunk(xs, 0)`).
4. Fallible construction/conversion must not return failure sentinel values (empty string, 0, empty list). Return `Option` without a reason or `Result[_, String]` with one (e.g. current `string.from_codepoint` returning `""` on invalid values violates this).
5. Unsuffixed numeric `to_<型>` aborts on values unrepresentable in the result type (R-L0-32). This is a case of item 3's “checkable precondition violations abort,” not an exception. Select other handling by suffix (`_checked`, `_saturating`, `_wrapping`; R-L0-34). (Rewritten in the sixth round; previously “unsuffixed wrapping is an exception.”)

- (a) Unsuffixed-safe in Rust, OCaml Base, Elixir; unsuffixed-throwing in OCaml Stdlib and Kotlin: no majority. Almide has no exceptions, leaving no choice other than unsuffixed-safe. Default-value forms exist in Python `get(k, d)`, Kotlin `getOrDefault`, Elixir `get(m, k, default)`, while Rust/Swift/JS/Gleam use operators or general-purpose functions.
- (c) `??` is the only spelling taught by CHEATSHEET; usable without remembering `get_or`.
- (d) Removal, under R-L0-14. `removed_spellings.rs` hints “`list.get(xs, i) ?? d`.”
- Issue: #3913.

<a id="r-l0-07-変換の向き-値の変換は元の型の-to_型入れ物からの構築は先の型の-from_元---採用"></a>

## R-L0-07 Conversion direction: values use source-type `to_<型>`; construction from containers uses destination-type `from_<元>` — ○ Accepted

1. Scalar (number/character) conversions live in the source type's module as `to_<先の型>` (`int.to_float`, `int8.to_int`).
2. Functions building a value from a container/representation live in the destination type's module as `from_<元>` (`map.from_list`, `set.from_list`, `bytes.from_list`, `string.from_codepoint`).
3. If the same conversion exists on both sides, keep only item 1's side. Remove `float.from_int`, identical to `int.to_float`.
4. Align types in directional pairs too. Resolve discrepancies such as `string.to_bytes` returning `List[Int]` but `bytes.from_string` returning `Bytes` (decide in L2 string/bytes).

- (a) 5/9 put `to_X` on the source type (Rust, Gleam, Elixir, new OCaml APIs, Kotlin). Container construction uses `from_list` in Gleam and Rust (`from_iter`), JS (`Array.from`). This combination directly matches Gleam style.
- (c) In `x |> int.to_float`, the pipeline's starting type determines function location.
- (d) Remove the `int.from_int8` family; add `int8.to_int` family. Remove `float.from_int`. Follow R-L0-14.
- Gate: compare public `to_<T>` / `from_<T>` pairs, checking no identical conversion on both sides.
- Issue: #3935.

<a id="r-l0-08-数値変換の-4-種は接尾辞の行列---据え置き穴埋め"></a>

## R-L0-08 Four numeric conversion kinds form a suffix matrix — ○ Retain + fill holes

- `to_<T>`: abort on values unrepresentable in result type (out of range, NaN to integer; R-L0-32). Rounding position in R-L0-35.
- `to_<T>_checked`: some only when exactly representable (R-L0-28).
- `to_<T>_saturating`: clamp out-of-range values to endpoints.
- `to_<T>_wrapping`: integer-to-integer conversion keeps lower bits and wraps (R-L0-34).
- Wrapping arithmetic `int.wrap_add`, `wrap_sub`, `wrap_mul` and `int.wrap(x, bits)` computes within `Int`; not used for type conversions (R-L0-34).
- (Rewritten in the sixth round to align with R-L0-32/R-L0-34/R-L0-35.)

- (a) Only Rust (prefixes) and Swift (argument labels) name all 4 separately; no majority. The existing suffix matrix is already gate-protected, so retain it.
  - Also examined Zig, outside the 9 languages. Zig has no single-word conversion, separating semantics into names such as `@intCast` (out of range is illegal behavior, stops in safe modes) and `@truncate` (wrapping) ([language reference](https://ziglang.org/documentation/master/#intCast)). Rust also has lints prohibiting/warning about `as` (Clippy `as_conversions`, `cast_possible_truncation`). Requiring intent in names points in the same direction as Almide's suffix matrix.
- (c) A matrix hole (missing `wrap_sub`) inevitably rejects an LLM's rule-based guess.
- R-L0-18 rejects adding conversion operator `as`; R-L0-19 covers diagnostics guiding from other languages' conversion forms.
- Issue: #3854 (existing).

<a id="r-l0-09-bool-を返す関数は-is_has_または決まった動詞---採用"></a>

## R-L0-09 Bool-returning functions use `is_`/`has_` or fixed verbs — ○ Accepted

Public Bool-returning function names start with `is_` or `has_`; otherwise use one of this closed verb set: `contains`, `exists`, `starts_with`, `ends_with`, `any`, `all`.

- (a) The `is` prefix is 8/9 (only Elixir appends `?`). `contains` is 5/9; `starts_with` and `ends_with` also have majorities.
- (c) The name signals a Bool return.
- Current exceptions (decide in L2): `args.flag`, `bytes.eof`, `regex.full_match`, `net.unix_poll`. `bytes.read_bool` reads a Bool, so is outside scope.
- Family prefixes: if a module splits function families by prefix (`net.tcp_*`, `net.udp_*`), place `is_` after that prefix (`net.tcp_is_open`). Read the leading part as family and the rest under this rule.
- Gate: inspect names of public functions returning Bool.
- Issue: #3895.

<a id="r-l0-10-高階関数の接尾辞の意味を-1-つに決める---採用"></a>

## R-L0-10 Fix one meaning for each higher-order function suffix — ○ Accepted

| Suffix | Function accepted | Examples |
|---|---|---|
| `_by` | Key function `(A) -> K` | `sort_by`, `unique_by`, `group_by` |
| `_with` | Function accepted instead of a value (combining, comparing, producing values) | `zip_with`, `sort_with` (comparison), `regex.replace_with` (produces replacement strings) |
| `_while` | Predicate holding from the start | `take_while`, `drop_while` |
| `_map` | Transformation returning `Option`, excluding none | `filter_map`, `find_map` |
| `flat_map` | Chain transformations returning lists | `list.flat_map`, `option.flat_map` |

- (a) `_by` means opposite things across languages: key function in Elixir/Kotlin, comparator in Rust `sort_by` and Swift/Gleam `by:`. Almide already consistently uses keys; fix that as the rule. Comparator variants use `_with`, following Kotlin `sortWith`.
- Gate: type-check that public `_by` functions end with a unary key-returning function argument; `_with` functions end with a function argument.
- Issue: #3949.

<a id="r-l0-11-長さは-len-だけ---採用既存の撤去を日付に載せる"></a>

## R-L0-11 Length is only `len` — ○ Accepted (schedule existing removals)

`list.length` and `string.length` are `@deprecated`, but their removal epoch is not set. Remove within the R-L0-14 window.

- (a) Spellings split (length 4, len 3). Retain current `len` under R-L0-00.
- Issue: #3851 A (existing).

<a id="r-l0-12-範囲は半開区間はみ出しは丸めabort-は契約違反だけ---据え置きゲート"></a>

## R-L0-12 Half-open ranges; clamp excess, abort only contract violations — ○ Retain + gate

`slice(start, end)` includes `start`, excludes `end`. Clamp out-of-range indices to endpoints. `take(n)` also returns all when `n` exceeds length. Negative indices are out of range, not positions from the end.

- (a) All 9 languages use half-open ranges. Python/JS/Gleam/Elixir clamp excess; Rust/Swift panic/trap. With no exceptions, Almide aligns on clamping.
- Gate: matrix test across every type with `slice`, `take`, `drop`, `take_end`, `drop_end` (list/string/bytes), comparing results for identical boundary inputs.

**Positional writes:** out-of-range writes (`list.set`, `list.update`, `list.swap`, `list.remove_at`, `bytes.put_<型>`, etc.) abort as caller contract violations, matching existing indexed assignment `xs[i] = v`. `list.insert` accepts positions `0..=len`, aborts outside.

- (a) Rust/Go/Kotlin/Swift stop out-of-range writes with panic/trap, Python/OCaml with exceptions. Only Elixir `List.replace_at` silently no-ops.
- (c) Currently function calls “do nothing” while indexed assignment “aborts”: two behaviors for the same write. No-op writes hide mistakes.
- (d) Semantic change (domain narrowing). Valid-position results unchanged, so not in epoch `breaks` (existing rule: “domain narrowing does not trigger an epoch”).
- Issue: #3944.

<a id="r-l0-13-環境を読む関数は-effect---据え置き穴を塞ぐ"></a>

## R-L0-13 Environment-reading functions are effect — ○ Retain (close holes)

Under ADR-0022, pure functions may write output and abort, but cannot read the environment. Currently only `process.env` still reads the environment as pure `fn`; close the hole by consolidating on `env.get` (#3851 B).

<a id="r-l0-14-互換の扱い全案で共通---採用"></a>

## R-L0-14 Compatibility handling (common to all proposals) — ○ Accepted

1. **Window:** renames/removals get `@deprecated(since=15, use="…")` in the release after 0.68.0. Add a `[[deprecation]]` block to `proofs/dialect-epochs.toml` with `removal_epoch = 16`. Use triggers E052 warning with replacement guidance.
2. **Removal:** remove in epoch 16 `breaks`. Calls afterward error with hints from `removed_spellings.rs`.
3. **Mechanical rewrite:** make one-to-one renames (`bytes.set_u8` → `bytes.put_u8`, etc.) rewritable by `almide fix`. Shape changes (`list.get_or(xs, i, d)` → `list.get(xs, i) ?? d`) also receive both hint and fix.
4. **No semantic repurposing:** do not keep the same name while old code passes type checking and yields different results. Old meanings persist in LLM training data. Move changed meanings to new names. Changes where old code necessarily becomes a type error (argument `String` → `Regex`, return `String` → `Option[String]`, etc.) are allowed with hints and `almide fix`, because results do not silently change. Thus unsuffixed sentinel-returning variants also become safe variants under the same unsuffixed names (`io.read_line` returns `Option[String]`, remove `read_line_opt`). Do not escape via suffixes like `_opt`. Same for container type changes: retain names `fs.read_bytes` and `io.write_bytes` while changing `List[Int]` to `Bytes`, removing separately named `Bytes` variants such as `fs.read_bytes_raw`.
5. **Language changes** (R-L0-05) enter epoch 15 `breaks`; record affected counts in `summary`.
6. **Counts:** each proposal records usage in spec/tests/docs/stdlib.

- Precedent: epoch 14 removed without a deprecation window. Many renames this time, so provide a window.
- Issue: #3904.

<a id="r-l0-15-標準関数は-native-と-wasm-の両方で動く---採用"></a>

## R-L0-15 Standard functions work on both native and wasm — ○ Accepted

Standard functions are assumed to behave identically on native (Rust) and wasm (WASI). For host-dependent functions, proposals identify which route brings them to wasm:
- Standard WASI interfaces (`wasi:filesystem`, `wasi:sockets`, `wasi:clocks`, `wasi:random`, `wasi:http`, `wasi:cli`)
- Provisional embedded host ops (same form as adjudicated TCP/unix/shm; retired at ADR-0023 step 7)
- Move out of standard library into packages

“Remain native-only” is not an endpoint (ADR-0027 D8).
- Fourth route (added in second-round review): private host capability (ADR-0025/ADR-0029). All `process` functions and `testing.assert_aborts` reach wasm through private capabilities of the embedded host. Standard wasm hosts reject them at load time instead of silently producing different results. No retirement is planned, so count separately from “provisional host ops.” Each L2 (b) explicitly names one of the 4: standard WASI, provisional host op, private host capability, outside standard library (package).

<a id="r-l0-16-順序のある型には-compare-を置く---採用"></a>

## R-L0-16 Ordered types provide `compare` — ○ Accepted

Ordered types (`int`, `float`, sized numbers, `bigint`, `bool`, `string`, `bytes`, `duration`, `compute`, `uuid`) provide `<mod>.compare(a, b) -> Int`: negative for less, 0 for equal, positive for greater. Comparators such as `list.sort_with` accept this shape (R-L0-10).

- (a) A majority provides three-way comparison: Rust `cmp` (`Ordering`), Go `cmp.Compare`, Kotlin `compareTo`, OCaml `compare`, Gleam `int.compare` (`Order`), Elixir `compare`.
- (c) Pass `list.sort_with(xs, int.compare)` without writing the comparator yourself.
- (d) Additions and renaming (`bytes.cmp` → `bytes.compare`). Floating-point order is IEEE totalOrder, matching `list.sort`.
- Gate: R-L0-G tests verify `compare` in every ordered type module.
- Issue: #3902.

<a id="r-l0-17-呼び出しの型は宣言だけで決まる---採用"></a>

## R-L0-17 Call types depend only on declarations — ○ Accepted

Types when calling effect fn (whether `!` is required, whether `??` can be written directly) must depend only on the declaration (arguments, return type, `effect`), not whether the body is intrinsic or Almide-written.

- Current discrepancy (measured): `env.get(k) ?? d` works, but Almide-bodied `args.option(k) ?? d` errors and requires `args.option(k)! ?? d`. Both interface JSON entries say `effect: true, error: string`; declarations do not distinguish them.
- Affected standard functions: `env.get`, `args.option`, `args.positional_at`, `io.read_line` ([L2-env](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-env.md) R-env-03), plus user-written effect fn returning `Option`.
- (c) Same-shape declarations require different call forms depending on body implementation; an LLM cannot determine the correct call by reading declarations.
- (d) Language change. Design the chosen uniform form in the issue, following ADR-0002 (failure/effect orthogonal) and ADR-0008 (explicit propagation only). If alignment changes existing calls, include in epoch 15 `breaks`.
- Gate: declaration-specific fixtures verify identical `??`/`!` acceptance for intrinsic/Almide versions of every standard effect fn.
- Issue: #3900.

<a id="r-l0-18-型を変換する演算子-as-は足さない---不採用"></a>

## R-L0-18 Do not add type-conversion operator `as` — × Rejected

Decision on adding a single-symbol type-changing operator such as `x as Int8`. Other languages' `as` separates into these 3 mechanisms:

| Mechanism | Examples | Almide responsibility |
|---|---|---|
| Numeric conversion | Rust `x as i8` (wrap; float→int saturates), Go `int8(x)`, Swift `Int8(x)` (stops) | `to_<型>`, `_checked`, `_saturating` (R-L0-08) |
| Runtime type check (downcast) | Kotlin `as?`, Swift `as?`/`as!`, Go `x.(T)`, Rust `downcast` | Unnecessary: no inheritance or `Any`; unknown-content values (`Value` etc.) are case-split with `match` |
| Silence type checking | TypeScript `x as Foo`, Python `typing.cast` | Do not introduce; checks nothing at runtime |
| Determine type within an expression | Swift `[] as [Int]`, `1 as Double` | Type annotation `[]: List[Int]` (measured: E018 hints also teach this) |

- (a) Only Rust writes numeric conversions with one `as`. Swift `as` cannot convert numbers (`x as Int8` is a compile error; use initializer `Int8(x)`; [Type Casting](https://docs.swift.org/swift-book/documentation/the-swift-programming-language/typecasting/)). Rust itself supports banning `as` through lints ([reference](https://doc.rust-lang.org/reference/expressions/operator-expr.html#type-cast-expressions)). Zig has no one-word `as`-equivalent conversion, naming distinct semantics separately.
- (b) Not applicable (not added).
- (c) If one symbol switches among wrapping/saturating/stopping by type pair, code does not reveal which occurs. Type-silencing `as` is a common failure pattern LLMs write to remove type errors.
- (d) No addition, no compatibility impact. Continue using `as` only for import aliases (`import x as y`) and JSON field renaming.
- Summary rationale: numeric conversions already have named R-L0-08 functions, so this violates R-L0-01 (one spelling per role). Other 3 uses are unnecessary under Almide's type design or contrary to its goals. Handle habitual other-language `as` with R-L0-19 diagnostics.

<a id="r-l0-19-他言語の変換の書き方から変換の関数へ案内する---採用"></a>

## R-L0-19 Guide other-language conversion forms to conversion functions — ○ Accepted

- Content:
  1. On `x as Int8`, `Int8(x)`, `x.into()` / `x.try_into()`, produce one diagnostic naming functions and overflow behavior for the source/destination pair (e.g. `int.to_int8(x)` wraps, `int.to_int8_checked(x)` yields `Option`, `int.to_int8_saturating(x)` saturates).
  2. Generate undefined-name “did you mean” table (`ALIASES`) from L3's “major spellings in other languages” column for every module (list/string manual fixes in #3905).
- Measurement (develop `9352c73e3`): `x as Int8` gives “no variable `as`” and “did you mean `x`?”, then a separate error for `Int8`. `Int8(x)` says “type `Int8` is not callable,” `x.into()` says “no method `into`”; none points to the correct function.
- (a) Other-language forms are in R-L0-18's table.
- (b) Checker-only change.
- (c) R-L0-18 does not add `as`, so Rust/TypeScript/Kotlin/Swift habits still produce `as`. Reaching the right function at the first error determines modification survival.
- (d) Diagnostic addition only; no compatibility impact.
- Completeness rule: every spelling in L3's other-language column either becomes an `ALIASES` key pointing to a live function/correct expression or has a stated exclusion reason (e.g. already a live Almide name with a different meaning). Every conversion-matrix pair gets `as`, `T(x)`, `into` guidance.
- Proposed gate: #3905's `tests/stdlib_alias_table_test.rs` compares generated rows against L3. Put examples of 4 forms in `tests/diagnostics/`, verifying exactly one diagnostic naming a function.
- Issue: #3958.
- Second-round additions (issue #3967):
  3. Add guidance for roles implemented via syntax/expressions rather than functions. Maintain one “role → correct expression” table separate from L3. Examples: `list.for_each`, `each`, `iter` → `for x in xs { … }`; `result.collect` → `rs |> list.map((r) => r!)`; `option.unwrap` → `o ?? panic("…")`; `result.ok` → `result.to_option`; `string.char_at` → `string.get`; `list.concat` → `xs + ys`; `bigint.add` → `a + b`; `float32.is_nan` → `float32.to_float(x) |> float.is_nan`.
  4. Intentionally absent matrix cells always get guidance (`bytes.get_u8` → `bytes.get`, `bytes.append_u8` → `bytes.push`, `bytes.get_u64_le` → `get_i64_le`, `json.encode` → `json.stringify`, `value.to_string` → `json.stringify`, `random.next_bytes` → repeated `next_int`).
  5. Do not emit edit-distance suggestions pointing to live functions with different meanings. Measured wrong guidance: `list.find_last` → `find_map`, `list.last_index_of` → `index_of`, `list.iter` → `filter`, `bytes.get_u8` → `set_u8` (write function), `value.to_string` → `as_string`, `print` → `println` (adds newline, changes output).
  6. For `String`/`Bytes` confusion (E005), show `bytes.from_string(s)` / `bytes.to_string(b)`. This confusion will increase as `fs.read_bytes`, `hash.fnv1a32`, `io.write_bytes` change to `Bytes`.
  7. Other-language bitwise operators like `5 & 3` currently cause syntax error “missing `)`.” Put guidance (planned in #3905) on the syntax-error path, not lexing.

<a id="r-l0-20-critical-プロファイルで黙って折り返す-to_型-を拒む---取り消し-r-l0-32-で解消"></a>

## R-L0-20 Reject silently wrapping `to_<型>` under critical profile — × Cancelled (resolved by R-L0-32)

- Content: only under `almide check --profile critical`, error on lossy-pair `to_<型>` and require `_checked` or `_saturating`, like Rust Clippy `cast_possible_truncation`. Ordinary code unchanged.
- Reason for hold: current critical profile (ALS bounded rules) guarantees safety/resource bounds, not computational correctness. Neither out-of-range `xs[i]` nor division by 0 is rejected under critical (measured 2026-10-01). Adding correctness (totality) rules to critical is deferred as CG-3 of [certification-grade](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/certification-grade.md). Rejecting only silent wrapping first would make critical's rule character inconsistent; decide alongside totality rules.

- Cancellation (sixth round): R-L0-32 makes unsuffixed narrowing abort, eliminating silently wrapping unsuffixed conversions. No critical-rejection target remains, so close this proposal. Wrapping occurs only when `_wrapping` is written in the name (R-L0-34).
- Decision order: add rules in ALS (almide/als) → advance pin → implement checker (`check/bounded.rs`).
- Issue: none (file together when CG-3 resumes).

<a id="r-l0-21-無印の-to_型-をはみ出したら失敗する版に変える--保留--r-l0-32r-l0-34-で解消"></a>

## R-L0-21 Change unsuffixed `to_<型>` to fail on overflow — On hold → resolved by R-L0-32/R-L0-34

- Content: lossy-pair `to_<型>` returns `Option` (like current `_checked`); wrapping becomes `to_<型>_wrapping`. Zig/Swift shape. Removes R-L0-06 item 5's exception.

- Resolution (sixth round): unsuffixed “fail on overflow” accepted by R-L0-32 (abort, not none); wrapping suffix `_wrapping` accepted by R-L0-34. Text below records the earlier hold.
- (a) Of languages with one-word lossy conversion, 4 wrap (Rust `as`, Go, Kotlin, OCaml), 1 stops (Swift). Below R-L0-00's 5/9 rename threshold.
- (b) Pure computation, identical on both targets.
- (c) The bug `int.to_int8(300)` silently becoming `44` passes both checking/execution. This form prevents it, but requires `??` or `!` on every conversion.
- (d) Semantic change. Return-type change makes old code type-error, satisfying R-L0-14.4. `almide fix` can rewrite to `to_<型>_wrapping`. Large change across all lossy pairs.
- Hold reason: without majority evidence, first measure how often LLMs write silent-wrapping bugs in practice, using almide-dojo.
- Issue: [almide-dojo#63](https://github.com/almide/almide-dojo/issues/63) (measurement).

<a id="r-l0-22-組み込みの-ord-プロトコルのメソッド名を-compare-にする---採用"></a>

## R-L0-22 Rename built-in `Ord` method to `compare` — ○ Accepted

- Current: built-in `Ord` method is `fn cmp(a: Self, b: Self) -> Int` ([type-system.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/type-system.md) §10). R-L0-16 places `<mod>.compare(a, b) -> Int` in ordered type modules. `Duration` derives `Ord`, giving two spellings for the same comparison: `d.cmp(e)` and `duration.compare(d, e)`.
- Proposal: rename `Ord` method to `compare`. User-written `fn T.cmp(a: T, b: T) -> Int` becomes `fn T.compare(…)`. Built-in-type `Ord` methods and module `compare` functions then share one spelling.
- Supporting L0 rules: R-L0-01 (one spelling per role), R-L0-16, R-L0-00 (internal collisions justify renaming).
- (a) Three-way comparison is called `compare` in Go (`cmp.Compare`), Gleam, Elixir, OCaml, Kotlin (`compareTo`). Only Rust uses `cmp`.
- (b) Change only the checker and name-based method lookup.
- (c) `list.sort_with(xs, duration.compare)` and `a.compare(b)` share a word, learnable from either direction.
- (d) Rename. Mark `cmp` as `@deprecated(since=15, use="compare")` at epoch 15; remove at epoch 16. `almide fix` rewrites `fn T.cmp` declarations and `.cmp(` calls.
- Measurement (develop `437b8f127`): `duration.ms(5).cmp(duration.s(1))` passes `almide check` but build fails internally (`[COMPILER BUG] internal type resolution failed`). This bug needs fixing independently of spelling; separate issue #3960.
- Completeness rule: every standard type with `Ord` has `<mod>.compare`, which is the `Ord` method itself (no separate forwarding function).
- Proposed gate: R-L0-G verifies `compare` resolution and both check/build success for each standard `Ord` type.
- Issue: #3961.

<a id="r-l0-23-状態を進める関数は-next_-で始め値-次の状態-を返す---採用"></a>

## R-L0-23 State-advancing functions start with `next_` and return `(値, 次の状態)` — ○ Accepted

- Current: two families take state and return a value plus next state, with inconsistent names and return order.
  - `random.next_int(rng, min, max) -> (Int, Rng)`: `next_` name, value first.
  - `bytes.read_i32_le_at(b, pos) -> (Int, Option[Int])`, `bytes.take_at(b, pos, n) -> (Int, Option[Bytes])`: `read_…_at` name, next position first. There is also `bytes.get_i32_le(b, pos)`, which only reads at a specified position.
- Proposal: call state-advancing functions `next_<何>` and return `(値, 次の状態)` (value, next state). Rename bytes cursor-advancing readers to `bytes.next_<型>(b, pos) -> (Option[T], Int)` and `bytes.next_bytes(b, pos, n) -> (Option[Bytes], Int)` (details: [L2-bytes](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-bytes.md) R-bytes-13).
- Supporting L0 rules: R-L0-01 (same spelling for roles of the same shape). R-L0-03's “subject first” covers arguments, not return order; decide that here.
- (a) Explicit-state random APIs put the value first: Haskell `random :: g -> (a, g)`, Elixir `:rand.uniform_s` uses `{値, 状態}` (value, state); parser convention likewise uses `(値, 残り)` (value, remainder). In the commonly used Rust bytes library, `get_i32_le` advances the cursor, opposite to Almide `get_<型>` (read only at a given position). `next_` avoids this confusion too.
- (b) Pure computation.
- (c) Across modules, “`next_` advances state; `get_` only reads at a specified position.” Destructuring `let (v, pos) = …` also has one order.
- (d) Rename and semantic change. Return tuple types change, so old code stops with a type error (satisfying R-L0-14.4). Mark old names `@deprecated` in epoch 15, remove in epoch 16. `almide fix` rewrites names but cannot reorder tuple destructuring, so emits a hint.
- Completeness rule: public functions returning a tuple containing a state type start with `next_`, with state last in the tuple.
- Proposed gate: add this shape check to R-L0-G tests (state types: `Rng` and the bytes family returning cursor `Int`).
- issue: #3962

<a id="r-l0-24-標準の面の型は正規の綴りで書くt---採用"></a>

## R-L0-24 Write standard-surface types in canonical spelling (`T?`) — ○ Accepted

- Current: ADR-0010 establishes `T?` as canonical for `Option[T]`, and `almide fmt` rewrites to `T?`. Yet standard function declarations mix 199 `Option[T]` and 23 `T?`. LLM-facing docs/stdlib and interface JSON expose those declarations unchanged.
- Proposal: standardize declarations in standard `.almd` files on `T?`; use canonical spelling in docs/stdlib, interface JSON, and this record's [target-surface.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/target-surface.md).
- Supporting L0 rules: R-L0-01 (one spelling per type), ADR-0010.
- (b), (d) Spelling only; types and behavior unchanged. No compatibility impact.
- (c) Seeing two spellings in documentation leaves an LLM unsure which is correct. Showing only canonical spelling also matches `almide fmt` results.
- Completeness rule: standard public function declarations contain no `Option[`.
- Proposed gate: apply the same normalization as `almide fmt --check --no-import-edit` to public function declarations in `stdlib/*.almd`, checking for no diff.
- issue: #3963


<a id="r-l0-25-文字列との変換の動詞を表で決める---採用"></a>

## R-L0-25 Define string-conversion verbs in a table — ○ Accepted

| Kind | To string | From string |
|---|---|---|
| Typed values (`Url`, `Uuid`, `Int`, `BigInt`, `Regex`, etc.) | `to_string` | `parse` (Result) |
| Different representation of the same value (`Bytes`, etc.) | `to_<表現>` | `from_<表現>` |
| Encoding that changes the character sequence while retaining content (base64, hex, URL component, form) | `encode_<変種>` (`encode` if no variant) | `decode_<変種>` (Result) |
| Exceptions | `json.stringify` (overwhelming training-data spelling; R-json-03), `url.build_query` (inverse of `parse_query`; no majority) | — |

- Supporting L0 rules: R-L0-07, R-L0-01, R-L0-00. Added after the second review found spellings already aligned but not expressed as a rule (`path.from_string` disappears under R-path-02). Renaming `build_query` is × because there is no majority.
- (c) New modules will not introduce spelling variation (uuid already follows this table: `parse`, `to_string`, `from_bytes`, `to_bytes`).
- Proposed gate: add to R-L0-G: “a public function taking `String` and returning `Result[T, _]` is named `parse`, `parse_<x>`, `decode_<x>`, or `from_<x>`.” Start the allowlist with the above 2 exceptions and only shrink it.
- issue: #3971

<a id="r-l0-26-演算子を持つ型は演算子と脱糖先の表を-1-か所で宣言する---採用"></a>

## R-L0-26 Types with operators declare operators and desugaring targets in one table — ○ Accepted

- Current: supported type/operator combinations are separately written into the checker and lowering. Actual mismatch: `-m` (Matrix unary minus) passes `almide check` but produces a lowering internal error (“NegInt expects Int operand”) (#3965). bigint and matrix also expose operator desugaring targets (`bigint.add`, `matrix.add`, etc.) as public functions, giving two spellings for one computation.
- Proposal:
  1. Declare a type × operator table (`+ - * / %`, unary `-`, comparisons) once, read by both checker and lowering. Missing cells yield diagnosed type errors, not internal errors.
  2. Do not expose operator desugaring targets (internal names such as `__add`). Operators are the only public spelling. Exceptions only for functions whose names also exist on other types and whose semantics differ from operators (`floor_div`, `modulo`) (R-bigint-04, R-matrix-14).
- Supporting L0 rules: R-L0-01, R-L0-15, R-L0-17 (calling follows declarations).
- (c) An LLM seeing `+` available also writes `-m`. This prevents internal errors users cannot fix.
- Completeness rule: for each operator-bearing type, every table cell either has a desugaring target or yields a diagnosed type error.
- Proposed gate: fixtures fix both `almide check` and build results for every table cell.
- issue: #3973

---

<a id="r-l0-27-next_-が-none-を返すときは状態を進めない---採用"></a>

## R-L0-27 When `next_` returns none, do not advance state — ○ Accepted

- Current: R-L0-23 specifies that `next_<何>` returns `(値, 次の状態)`, but not the next state when the value is none. The second value is undefined when `bytes.next_u32_le(b, pos)` lacks bytes or in `random.next_choice(rng, [])`.
- Proposal: when the value is none, return the received state unchanged (do not move the cursor or advance random state). Movement without reading a value (`bytes.skip(b, pos, n) -> Int`) has no value, so returns only next state, not an R-L0-23 pair.
- Supporting L0 rules: R-L0-23, R-L0-06 (fallible extraction returns none without changing other values).
- (a) Rust `Iterator::next` does not change the next call's meaning even when returning None. Parser convention (failure consumes no input) is the same.
- (b) Pure computation.
- (c) If insufficient, try reading a different type without restoring position.
- (d) Only documenting new functions' contract; existing functions unchanged.
- Proposed gate: add “when none, second value equals the argument” to `next_` family matrix tests.
- issue: #3979

<a id="r-l0-28-_checked-の変換は正確に表せるときだけ-some---採用記録の訂正"></a>

## R-L0-28 `_checked` conversions return some only when exactly representable — ○ Accepted (record correction)

- Current: R-L0-08 says “`to_<T>_checked`: out of range gives none in `Option`.” Implementation is stricter: none when not exactly representable (measured `almide 0.68.0 dev 437b8f127`: `float.to_int32_checked(1.5)` is none, `int.to_float32_checked(16777217)` is none, `int.to_float32_checked(16777216)` is `some(16777216)`).
- Proposal: align wording with implementation: “`to_<T>_checked`: some only when the result exactly equals the original value. Out of range, fractional truncation, and precision loss all yield none.” Same semantics as Swift `init?(exactly:)`.
- (d) Documentation correction only.
- issue: none

<a id="r-l0-29-負の開始位置を丸めずに-abort-にする---採用保留から変更"></a>

## R-L0-29 Abort rather than clamp negative start positions — ○ Accepted (changed from hold)

- Current: R-L0-12 says “half-open ranges; clamp overflow; abort only for contract violations.” `list.slice(xs, -3, len)` clamps start to 0 and returns a different value without stopping. Code written with Python/JS negative-indices-count-from-end habits silently produces a different result.
- Proposal: abort when start/end positions are negative (also negative counts for `slice`, `take`, `drop`). Use `take_end`, `drop_end` to count from the end. Positive overflow (beyond `len`) remains clamped.
- Reason for hold: R-L0-12 was a gated decision paired with positive clamping. Decide after measuring negative-index mistakes in Dojo (same treatment as R-L0-21).
- (a) Rust and Go panic; Python and JS count from the end. None of the compared languages silently clamps to 0.
- Decision change (fifth round): 3 external reviews in succession ranked this among the most serious findings, and no compared language silently clamps to 0, so accept without waiting for measurement. Align with R-L0-32 (unmarked narrowing conversion) and R-L0-33 (role-based out-of-range behavior): “violation of a caller-checkable precondition aborts” (R-L0-06 item 3).
- (d) Semantic change. Currently clamped calls now abort, so include in epoch 15 `breaks`. Keep Dojo measurement (almide-dojo#63) as an item counting mistakes after adoption.
- issue: #3990

<a id="r-l0-30-引数名は役割ごとに-1-つ---採用"></a>

## R-L0-30 One parameter name per role — ○ Accepted

- Current: L0 aligns function names but not parameter names. One role has two or more spellings.
  - `Bytes` subject: 149 `b`, 12 `data` (`zlib.*`, `hash.fnv1a32`, `matrix.from_bytes_*`). Outbound bytes (`fs.write_bytes`, `net.tcp_write`, etc.) consistently use `data`.
  - Numeric subject: `int`/`float` use `n`, sized types use `x`.
  - Element: `x` (`list.push`), `val` (`list.insert`, `bytes.*`), `value` (`set.insert`).
  - Index: `bytes.get` uses `i`, `bytes.insert` uses `pos`.
  - Function argument: mostly `f`; `result.filter` uses `pred`, `heap.new_by` uses `key`.
  - Pattern: `regex.new` uses `pat`; `fs.glob`/`http.route` use `pattern`.
- Parameter names are public contracts. Almide has named arguments (`list.len(xs: [1, 2])` passes; measured `almide 0.68.0 dev`); signatures appear in `llms.txt` and cheatsheet.
- Proposal: choose one name per role.

| Role | Name |
|---|---|
| Subject `List`, `Bytes`, `Map`, `Set`, `Matrix`, `Value`, `Result`, `Option` | `xs`, `b`, `m`, `s`, `m`, `v`, `r`, `o` |
| Outbound bytes (`io`, `fs`, `net`, `unix`, `http`, `process`) | `data` |
| Subject of `int`, `float`, `bigint`, sized-number modules | `x` (binary operations use `a`, `b`) |
| `path` subject | `path` (same as `fs`) |
| Element value (list, set, deque, heap, bytes) | `x` (only `map` uses paired `key`, `value`. Non-container-element values, e.g. `env.set(name, value)`, retain role names) |
| Element index | `i` |
| Byte position for typed reads/writes (`bytes.get_*`, `put_*`, `next_*`) | `pos` |
| Function argument | `f`; comparator `cmp` |
| Named function-type argument (`HttpHandler`, `HttpMiddleware`) | Type-role name (`handler`, `middleware`) |
| Functions taking two equal-status values of the same type (`compare`, `min`, `max`, `merge`, `union`, `xor`, `zip`) | `a`, `b` (takes precedence over subject-name rules) |
| Pattern string (`regex`, `fs.glob`, `http.route`) | `pattern` |
| Count/amount (`duration.ms(n)`, `random.bytes(n)`, `list.take(xs, n)`) | `n` |

- (c) An old named argument produces E004 “no parameter named …” with the correct name in its hint, not a silently different value.
- (d) Rename. There were 0 standard-function calls with named arguments in `spec/`, `stdlib/`, `examples/`, and exercises. Include in epoch 15 `breaks`; rewrite via `almide fix`.
- Completeness rule: classify every public parameter by role; a second spelling for one role fails the gate. Duplicate parameter names within a signature also fail (fourth-round rewriting produced `bytes.compare(b, b)`, caught in review).
- [target-surface.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/target-surface.md) signatures use this table's names.
- issue: #3987

<a id="r-l0-31-同じ形を持つ型の組はプロトコルで宣言する---採用"></a>

## R-L0-31 Declare groups of same-shaped types through protocols — ○ Accepted

- Current: R-duration-03 declared `Duration` and `Compute`'s common shape in the `TimeAmount` protocol. Other groups repeat the same functions per type, maintaining alignment through handwritten matrix gates or prose.
- Proposal: declare standard protocols for “groups of types with same-role functions sharing names and shapes.” Protocol checking catches one-sided missing functions in type checking. One generic function works on any type in the group.
- Groups examined and decisions:

| Group | Protocol | Decision | Contents |
|---|---|---|---|
| `Duration`, `Compute` | `TimeAmount` | ○ (R-duration-03) | 6 unit constructors and `compare` |
| `HttpRequest`, `HttpResponse` | `HttpMessage` | ○ (R-http-12) | `header`, `header_values`, `headers`, `body`, `body_bytes`, `set_header`, `set_body`, `set_body_bytes`. The 6 `req_` functions and `get_header` become single generic functions |
| `int` and 8 sized integer types | `Integer` | ○ (R-sized-08) | `compare`, `min`, `max`, `clamp`, `parse`, `parse_radix`, `to_string`, `to_string_radix`, `min_value`, `max_value` |
| `float`, `float32`, `float64` | `FloatingPoint` | ○ (R-sized-08) | Same selection/comparison families as `Integer`, plus classification (`is_nan`, `is_infinite`, `is_finite`) and special values (`nan`, `infinity`, `epsilon`) |
| list, string, bytes, map, set, deque, heap | `Sized` | ○ (this proposal) | `len`, `is_empty`; all 7 containers already have both |
| `contains`, `count` of list, string, bytes, map, set, deque, heap | — | × | `map.contains` takes keys, `map.count` takes `(K, V)`; no single element type |
| Types with `parse` (`int`, `bool`, `uuid`, `url`, etc.) | `Parse` | × | Cannot write `T.parse(s)` inside a generic function (measured `almide 0.68.0 dev`: parameterless functions may be declared in protocols, but `T.parse` gives E002). Resubmit when the language gains type-selected function calls |
| `map`, `flat_map` of `option`, `result` | — | × | The language lacks higher-kinded types: abstraction over a type-taking type such as `Option[_]` |
| Encodings such as `base64`, `hex` | — | × | Groups of modules, not types; no values carrying protocols. R-L0-25's table aligns spelling |

- Protocols guarantee alignment only for functions listed in the contents column. Functions on just one member (`float.floor`, `round`, `signum`, etc.; `float32`/`float64` use `to_float` then `float.*`) lie outside protocols.
- Operators (`+`, `<`, etc.) are outside protocols (same as R-duration-03; declare them in R-L0-26's operator table).
- Parameterless functions (`min_value`, `nan`, 6 unit constructors) are also declared in protocols to check their presence. The current language cannot call them from generic functions.
- Supporting L0 rules: R-L0-01 (do not handwrite one role twice), R-L0-12 (align families by matrix); CLAUDE.md's “derive properties from one declaration.”
- (a) Swift `BinaryInteger`, `BinaryFloatingPoint`, `Collection`; Rust `num-traits`' `PrimInt`, `Float`; Go `constraints.Integer`, `constraints.Float`.
- (b) Checker change, not value representation.
- (c) Type declarations guarantee that code replacing `Float` with `Float32` can use the same function names.
- (d) Add declarations. Filling missing functions (R-sized-08) is additive.
- Proposed gate: put protocol-generic function tests in `spec/stdlib/` for every declared type. Replace R-L0-12's handwritten matrix gates where protocols cover them.
- issue: #3989

<a id="r-l0-32-無印の狭める変換は範囲外と-nan-で-abort-にする入力の型によらない---採用"></a>

## R-L0-32 Unmarked narrowing conversions abort out of range or on NaN, regardless of input type — ○ Accepted

- Current: unmarked `to_<T>` in R-L0-08 handles out-of-range input differently by type. Integer-to-integer wraps, float-to-integer saturates, NaN becomes 0 (measured `almide 0.68.0 dev 437b8f127`, native and wasm agree: `int.to_int8(300)` = 44, `float.to_int8(300.0)` = 127, `int.to_uint8(-1)` = 255, `float.to_uint8(-1.0)` = 0). Merely changing input type yields a different value without stopping.
- Proposal: unmarked narrowing conversions abort when the result type cannot represent the value (out of range, NaN). Float fractional truncation (`float.to_int(1.5)` = 1) stays unchanged. Choose other behavior by name.
  - `_checked`: none unless exactly representable (R-L0-28).
  - `_saturating`: saturate at endpoints. Add this cell for floating inputs too (27 functions: `float`, `float32`, `float64` → 9 integer types). Current unmarked behavior moves here.
  - Wrapping uses the `_wrapping` cell (R-L0-34; changed from guidance to `int.wrap` in round six).
- Supporting L0 rules: R-L0-06 item 3 (caller-checkable precondition violations abort); R-L0-08's matrix (lossy conversions have unmarked, `_checked`, `_saturating`: 3 variants).
- (a) Swift `Int8(x)` traps out of range; select behavior via `init(exactly:)`, `init(clamping:)`, `init(truncatingIfNeeded:)`. Rust `as` switches wrapping/saturation by type, precisely the mismatch review identified.
- (b) Pure computation, same decision on both targets.
- (c) A mistake aborts immediately rather than silently returning a different value.
- (d) Semantic change. Currently wrapping calls now abort; include in epoch 15 `breaks`. `almide fix` rewrites unmarked floating-input conversions to `_saturating` (preserves behavior); unmarked integer-input conversions to `_wrapping` (preserves behavior; R-L0-34).
- Completeness rule (replaces R-L0-08): for any input type, lossy conversions have unmarked (abort), `_checked`, `_saturating`: 3 variants. Lossy integer-to-integer conversions add `_wrapping` for 4 (R-L0-34). Floating destinations follow R-L0-35.
- issue: #3990

<a id="r-l0-33-範囲外の扱いを関数の役割で分ける---採用"></a>

## R-L0-33 Separate out-of-range behavior by function role — ○ Accepted

- Current: R-L0-12 states in one sentence “half-open ranges, clamp overflow, abort only for contract violations.” Individual pages say reading one position returns none, writing one aborts. Readers had to infer the applicable rule per function.
- Proposal: one role-based table.

| Role | Examples | Positive overflow | Negative position/count |
|---|---|---|---|
| Extract range | `slice`, `take`, `drop`, `take_end`, `drop_end`, `byte_slice`, `bytes.slice` | Clamp to length | abort (R-L0-29) |
| Read one position | `get`, `first`, `last`, `bytes.get_*`, `next_*` | none (`next_` does not advance state; R-L0-27) | none |
| Write/insert/delete one position | `set`, `update`, `swap`, `insert`, `remove_at`, `bytes.put_*` | abort | abort |
| Update an existing element by key | `map.update` | abort if key absent (R-map-08) | — |
| Assume shape | `matrix` operations, `bytes.xor`, `chunk(n <= 0)` | abort | abort |
| Create a quantity | `list.repeat(x, n)`, `string.repeat` | — | abort |

- Supporting L0 rules: R-L0-06 (fallible extraction yields none; checkable precondition violations abort), R-L0-12.
- (a) Rust `get` returns None, `v[i]` and `split_at` panic, `iter().take(n)` clamps. Kotlin `getOrNull` returns null, `take(-1)` throws.
- (d) Most of the table restates current behavior. Changes are R-L0-29 (negative positions), R-map-08 (`map.update`), and `repeat`, which previously clamped negative counts.
- issue: #3990

<a id="r-l0-34-整数から整数への損失のある変換に-_wrapping-を置く---採用"></a>

## R-L0-34 Add `_wrapping` for lossy integer-to-integer conversions — ○ Accepted

- Current: R-L0-32 prescribed `int.to_int8(int.wrap(x, 8))` for wrapping. `int.wrap(x: Int, bits: Int) -> Int` computes within `Int`, so cannot accept the upper half of `UInt64` (2^63 and above) outside `Int`, or return wrapped `UInt64`. R-L0-32 makes `uint64.to_int(uint64.max_value())` abort, leaving no wrapping path through `UInt64`.
- Proposal: add `to_<T>_wrapping` for lossy conversions from integers (`Int` and 8 fixed-width types) to integers (46 functions). Keep low bits and wrap to the result type's range (two's complement). Retain `int.wrap`, `wrap_add`, `wrap_sub`, `wrap_mul` as computations within `Int`, not conversion guidance.
  - Examples: `int.to_uint8_wrapping(300)` = 44, `int.to_uint64_wrapping(-1)` = `uint64.max_value()`, `uint64.to_int_wrapping(uint64.max_value())` = -1.
  - Write `UInt64` computations using operators between `uint64` values (R-L0-26). To move to `Int`, select `to_int` (abort out of range), `to_int_checked`, `to_int_saturating`, or `to_int_wrapping`.
  - Do not add for conversions from `BigInt` (use `bigint` remainder to extract low bits of arbitrary-length integers).
- Supporting L0 rules: R-L0-08 (select behavior through suffix matrix), R-L0-32 completeness, CLAUDE.md's “align families by matrix.”
- (a) Swift names wrapping via `init(truncatingIfNeeded:)` (F). Rust `as`, Go `T(x)`, Kotlin `toByte()` wrap with one-word conversions. `_wrapping` follows Rust prefixes such as `wrapping_add` and Almide `wrap_add`.
- (b) Pure computation, identical on both targets.
- (c) The intent expressed with other languages' `as` (wrapping) gets one named spelling. Having `almide fix` rewrite integer-input unmarked conversions to `_wrapping` preserves behavior (replaces R-L0-32(d)).
- (d) Addition (46 functions).
- Completeness rule: lossy integer pairs have unmarked, `_checked`, `_saturating`, `_wrapping`: 4 variants. Add this column to the matrix gate (R-sized-01).
- issue: #3995

<a id="r-l0-35-無印の変換の丸めと-nan-を変換先が整数か浮動小数点かで分けて書く---採用"></a>

## R-L0-35 Specify unmarked conversion rounding and NaN separately for integer/floating destinations — ○ Accepted

- Current: R-L0-32's “abort values unrepresentable in the result type” can be read as applying to fractional truncation, `Float32` precision loss, and NaN between floating types too.
- Proposal: use the following table. This fixes wording while keeping current implementation behavior.

| Destination | Fraction/precision | Out of range | NaN |
|---|---|---|---|
| Integer (`float.to_int`, etc.) | Truncate toward 0 (`float.to_int(1.5)` = 1, `float.to_int(-1.5)` = -1) | abort | abort |
| Integer (from integer) | — | abort | — |
| Floating (`float.to_float32`, `int.to_float32`, `int.to_float`, etc.) | Round to nearest, ties to even (`int.to_float32(16777217)` = 16777216.0) | Infinity (`float.to_float32(1e300)` = infinity) | Remains NaN |

- In every row, `_checked` returns some only when exactly representable (R-L0-28). Truncation, rounding, out-of-range, and NaN all yield none. Unmarked conversions differ by allowing the rounding in this table.
- Supporting L0 rules: R-L0-28, R-L0-32.
- (a) IEEE 754 default conversion (round to nearest); Rust, Swift, Go, Kotlin float-to-int truncation toward 0.
- (d) Finalize documentation only.
- issue: #3995

<a id="r-l0-36-間違えても型エラーになるはそれだけでは不採用の理由にしない---採用"></a>

## R-L0-36 “Mistakes become type errors” alone is not a rejection reason — ○ Accepted

- Current: some rejection reasons say “a misspelling becomes a type error, not silent acceptance.” Almide measures modification survival; initially type-erroneous code also counts as failure. Better than a silently different value, but lowers the proportion working correctly unchanged.
- Proposal: describe type errors as mitigating harm. Rejection itself must cite a concrete property lost by changing (existing code silently changes value, two spellings for one role, etc.) or measurement results. With neither, hold and measure candidate survival rates in Dojo.
- Revisited proposals: R-map-04 changed to hold (measure in Dojo). R-datetime-04, R-duration-04, R-result-02, R-option-03, R-regex-06 retain decisions because reasons other than type errors exist (two spellings per role, form determined by semantics, ADR decisions).
- (d) Add decision rule; functions unchanged.
- issue: none
<a id="r-l0-37-標準の具体型はprotocol-の組み合わせとその型だけの関数でできている---採用方向"></a>

## R-L0-37 Standard concrete types combine protocols with type-specific functions — ○ Accepted (direction)

- Current: functions of the same role are repeated per type, with naming/shape alignment enforced by rules (R-L0-16, etc.) and handwritten matrix gates. R-L0-31 made some groups (`Integer`, `FloatingPoint`, `Sized`, `TimeAmount`, `HttpMessage`) protocols.
- Proposal: make this the construction principle for all standard types.
  1. Functions shared by two or more types in the same role must be declared once as protocol methods.
  2. Type-module functions (`int.compare`, `list.len`) are those protocol methods themselves, not separate forwarding functions (extend R-L0-22 to all protocols).
  3. Describe a type's public surface as its conformance list plus type-specific functions.
  4. Organize protocols hierarchically, modeled on Swift's standard library (`Equatable`→`Comparable`, `AdditiveArithmetic`→`Numeric`→`BinaryInteger`→`FixedWidthInteger`, `Sequence`→`Collection`→`BidirectionalCollection`→`RandomAccessCollection`, etc.). Give reasons for layers absent in Swift. [protocol-view.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/protocol-view.md) contains the hierarchy, per-type conformances, holes, and decisions it makes (mechanically generated from protocol definitions/conformances against target-surface.md). Reposition R-L0-31's named protocols (`Integer`, `FloatingPoint`, `Sized`, `TimeAmount`, `HttpMessage`) within it; contents follow hierarchy definitions.
- Why not protocols only: making singleton protocols for functions only one type has, such as `string.split` or `regex.find`, adds a name without enforcing alignment. Swift also has concrete-type-only methods.
- Measurement (redesigned surface): when defining the hierarchy, 663 of 1,041 functions across 31 value types were protocol requirements or numeric-conversion families; 209 required functions were missing (168: the 8 sized integer types lacked `int` arithmetic/bit operations). Round seven added 164 functions; round nine consolidated conversions into destination `from` (R-L0-40); round ten consolidated sequence algorithms as `Sequence`/`Ordered` defaults (R-L0-42). Bit operations remain functions required by `Integer` (R-int-10, R-L0-43). The redesigned total is 1,291 functions, or about 650 names to learn when counting protocol functions once.
- Language gaps (measured `almide 0.68.0 dev`; none currently expressible):
  - ~~Call receiverless methods through type parameters~~ → Added (after wave 2, 2026-10-24). Writing `M.empty()` under `[M: Monoid]` becomes a concrete type function at monomorphization (declared types: `fn T.empty`; built-ins: standard-library module functions). `list.sum` uses this (`spec/lang/protocol_static_requirement_test.almd`).
  - Protocol inheritance (`protocol Integer: Ord`).
  - Associated types (`type Elem`), needed for container `contains`/`count` protocols (the rejection reason in R-L0-31).
  - Default implementations (derive `min`, `max`, `clamp` from `compare`).
  - Conditional conformance (`List[A]` is `Hash` only if `A: Hash`).
  - Types taking containers themselves (`F[_]`) are unnecessary. Like Swift, leave `map` as type-specific functions, not protocol members.
- Process: write ADRs before adding the 5 features. Before introduction, verify in Dojo that protocol-generic functions do not lower survival of LLM-written code. Type-specific spelling (`int.compare(a, b)`) denotes the same function, so both work.
- Position (added in round fifteen): the [protocol hierarchy](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/protocol-view.md) is an **intended design**. What implementation currently enforces is names/shapes in the [redesigned function list](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/target-surface.md), the user-visible public surface. Internals may remain concrete type-specific functions. The hierarchy is a future target, moving internals toward it as the above 5 language features arrive.
  - Listed generic functions (`list.sum[A: Numeric]`, `list.join[T: Text]`, numeric `from[S: Numeric]`, etc.) were implemented in listed form (2026-10-24). `Numeric`, `Integer`, `Text` are built-in protocols; conforming built-in types and requirements are declared once in the compiler (`BUILTIN_PROTOCOLS`). Requirements include unpublished internal functions (`__trunc`, `__int_exact`, `__sum`, `__join`, etc.; `__` names absent from the list). Generic bodies call requirements through type parameters; monomorphization replaces them with concrete module functions. The `almide docs-gen --check` matrix gate verifies that conforming types have all requirements. `lang_wait` in `scripts/check-stdlib-target.py` is 0.
  - Still intended design: hierarchy itself (inheritance, associated types, defaults, conditional conformance) and conformance of built-in types to user-declared protocols. No public functions remain intended-only designs.
  - Where user-declarable protocols suffice (`io.read[H: Read]`, `http.header[M: HttpMessage]`), implement the listed form now.
- (d) Declaration of direction. Round-seven proposals (R-L0-38, R-sized-10/11, R-bigint-07, R-bool-03, R-list-20, R-string-16, R-deque-03/04) added functions filling requirements. Reposition R-L0-31's × rows (`contains`, `count`, `Parse`) as `Membership`, `Container`, `Parse` in this hierarchy.
- issue: #4000

<a id="r-l0-38-組み込みの-repr-のメソッド名を-to_string-にする---採用"></a>

## R-L0-38 Name the built-in `Repr` method `to_string` — ○ Accepted

- Current: built-in `Repr` has `fn repr(v: Self) -> String`. Numeric, `bigint`, `uuid`, `url` modules have `<mod>.to_string`, while `bool` does not. Same role, two spellings, and some types have only one.
- Proposal: like R-L0-22 (`Ord`'s `cmp` → `compare`), name the `Repr` method `to_string`. Under R-L0-37, module functions are protocol functions themselves, so all value types have `<mod>.to_string`, yielding the same string as `"${x}"`. Types with `Parse` satisfy `parse(to_string(x)) == ok(x)` (Swift's `LosslessStringConvertible`).
- (a) Swift gives all types `CustomStringConvertible`'s `description`, paired with `init?(_ description:)` in `LosslessStringConvertible`. Rust `Display` (`to_string`) and `FromStr` form the same pair.
- (c) `bool.to_string(b)`, inferred from `int.to_string`, works. No need to remember per type whether `to_string` exists.
- (d) Rename and additions. In epoch 15 mark user-written `fn T.repr` as `@deprecated(since=15, use="to_string")`; remove in epoch 16. `almide fix` rewrites it. List entries for types with rows on their pages (types with `Parse`); describe other types under common contracts not listed individually.
- issue: #4000

<a id="r-l0-39--を累乗の演算子から外し累乗は--だけにする---採用"></a>

## R-L0-39 Remove `^` as exponentiation; keep only `**` — ○ Accepted

- Current: `^` is exponentiation (alias of `**`). Measured (`almide 0.68.0 dev`): `6 ^ 3` returns `216`.
- Problem: in 5 languages—Rust, Go, Python, TS/JS, Swift—`^` is exclusive OR (XOR). An LLM writing `a ^ b` intending XOR silently gets exponentiation, not a type error, violating R-L0-32's “do not silently return another value.” None of the 9 languages uses `^` for exponentiation (Python/JS use `**`, others functions).
- Proposal: remove `^` from the operator table, making it a lexical error. Hint: “exponentiation is `**`, bitwise exclusive OR is `int.bxor`.” Keep `**`.
- (b) Syntax change, identical on both targets.
- (d) Syntax removal. Warn on `^` in epoch 15, error in epoch 16 (R-L0-14). `almide fix` rewrites `a ^ b` to `a ** b` (preserving current meaning). Add a row to `proofs/dialect-epochs.toml`.
- Never repurpose freed `^` (R-L0-43). Permanently an error guiding to “exponentiation is `**`, bitwise exclusive OR is `int.bxor`.”
- issue: #4002

<a id="r-l0-40-数の変換は変換先の型に-from-で置く---採用"></a>

## R-L0-40 Put numeric conversions on destination types as `from` — ○ Accepted

- Current: source modules list destination-specific conversion names (`int.to_float(n)`, `int8.to_uint16_checked(x)`). Combinations among 13 numeric types produce 347 functions, a quarter of the standard-library list.
- Proposal: destination modules expose generic functions accepting any numeric type.
  - `fn <T>.from[S: Numeric](x: S) -> T` (unmarked; abort values unrepresentable in the result type)
  - `fn <T>.from_checked[S: Numeric](x: S) -> T?`
  - `fn <T>.from_saturating[S: Numeric](x: S) -> T` (integer types only)
  - `fn <T>.from_wrapping[S: Integer](x: S) -> T` (integer types only; integer input only)
  - Semantics unchanged from R-L0-32, R-L0-34, R-L0-35 (spelling only). For lossless pairs, unmarked never aborts and `_checked` always returns a value.
- (a) Destination-side is the majority: Rust `i8::from(x)`, `i8::try_from(x)`; Swift `Int8(x)`, `Int8(exactly:)`, `Int8(clamping:)`, `Int8(truncatingIfNeeded:)`; Python `float(n)`; Go `float64(n)`; JS `Number(x)`; OCaml `Float.of_int`. Only Kotlin (`toDouble()`) and Gleam (`int.to_float`) are source-side. Swift's 4 initializers map one-to-one to Almide's 4 functions.
- (c) Names to learn shrink from 13 types × 12 destinations × suffix matrix to 4 suffixes. Pipes work the same (`n |> float.from`).
- (d) Rename and removal. Mark `to_<T>…` as `@deprecated(since=15, use="<T>.from…")` in epoch 15; remove in epoch 16. `almide fix` mechanically rewrites `int.to_float(n)` to `float.from(n)`. Numeric types' `to_string`/`parse` lie outside this proposal.
- Language requirement: generic type-parameter functions bounded by protocols (`S: Numeric`) can call receiverless requirements (`T.from`) (#4000).
- Implementation status (2026-10-24): implemented. The 44 `from`, `from_checked`, `from_saturating`, `from_wrapping` functions for 9 integer types, 3 floating types, and `bigint` are available in listed form (e.g. `fn int8.from[S: Numeric](x: S) -> Int8`). One generic body calls source-type internal requirements (`__trunc`, `__int_exact`, `__uint64_exact`, `__int_sat`, `__to_float`, etc.—conversion pivot values) through type parameters. No per-source/destination functions needed. Semantics follow R-L0-32/34/35: unmarked truncates toward 0 and aborts out of range, on NaN/infinity; `_checked` yields none unless exactly representable (fraction, NaN, out of range); `_saturating` clamps to endpoints, NaN to 0; `_wrapping` accepts only integers (`Integer`; floating input is a check error), interpreting low bits as two's complement. Handles `UInt64` values ≥2^63 and single rounding to `Float32` (without passing through `Float`). Current `to_<T>` columns remain (removal starts with epoch 15 `@deprecated`; R-L0-40(d)). `spec/stdlib/numeric_from_test.almd` checks both targets.
- issue: #4003

<a id="r-l0-41-ビット演算を演算子にし-を関数の合成から外す---取り消し十二巡目記号の意味を入れ替えないため--r-l0-43"></a>

## R-L0-41 Make bit operations operators and remove `>>` composition — × Canceled (round twelve: do not repurpose symbols → R-L0-43)

- Current: bit operations are functions such as `int.band(a, b)`, not operators. `>>` is composition (`f >> g`), and `^` was exponentiation (removed by R-L0-39).
- Proposal: give integer types (`int`, 8 sized types, `bigint`) `&`, `|`, `^` (XOR), `~` (negation), `<<`, `>>`. `>>` preserves sign for signed types and fills zeros for unsigned (like Swift/Rust/Go). For sign-ignoring right shift, convert to unsigned (`uint64.from_wrapping(x) >> n`).
- Match Swift precedence, avoiding C's pitfall (`a & b == c` becoming `a & (b == c)`): `**` > `<<` `>>` > `*` `/` `%` `&` > `+` `-` `|` `^` > range > `|>` > comparison.
- Remove composition `f >> g`; use `(x) => g(f(x))` or `x |> f |> g`. Composition operators are a minority among 9 languages (F#/Haskell lineage); in 5—Rust/Go/Python/JS/Swift—`>>` is right shift. Measured: Almide composition `>>` appears only in 7 spec lines testing composition itself.
- (a) R-int-05 (×) compared only function names and found no majority. Including operators, `&`, `|`, `^`, `<<`, `>>` appear in Rust/Go/Python/JS/Swift: 5-language majority (R-L0-00).
- (d) Syntax additions and semantic change. `&`, `|`, `~`, `<<` available from epoch 15. `^` (exponentiation), `>>` (composition): warn in epoch 15, error in epoch 16, new meaning in epoch 17 (interpose one error epoch to avoid silent replacement). `almide fix` rewrites `int.band(a, b)` to `a & b`, `f >> g` to `(x) => g(f(x))`. Remove `band`, `bor`, `bxor`, `bnot`, `bshl`, `bshr`, `bshr_logical`.
- issue: #4004

<a id="r-l0-42-列のアルゴリズムは-protocol-の既定の実装として-1-回だけ書く---採用"></a>

## R-L0-42 Write sequence algorithms once as protocol defaults — ○ Accepted

- Current: 30 sequence algorithms such as `enumerate`, `zip`, `sort`, `min`, `take_while`, `chunk` exist only on `list`. For `deque`, `set`, `string`, `bytes`, `map`, one must `to_list` then use `list.*`. `list.sum`/`product` have `Int`/`Float` forms, and `list.join` has `String`/`Bytes` forms.
- Proposal: put these once as defaults on `Sequence` and `Ordered` in the [protocol hierarchy](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/protocol-view.md) (like Swift `Sequence`/`Collection` extensions). Under R-L0-37 they appear with identical names/shapes on every conforming sequence module.
  - `Sequence` (traversable in a fixed order): `enumerate`, `zip`, `zip_with`, `filter_map`, `find_map`, `reduce`, `scan`, `partition`, `group_by`, `sort`, `sort_by`, `sort_with`, `min`, `max`, `min_by`, `max_by`, `min_with`, `max_with`, `sum`, `product`. Results with changed element types or reordered results return `List` (like Swift `sorted()`/`map` returning `Array`).
  - `Ordered` (countable from both ends): `find_index`, `take_while`, `drop_while`, `chunk`, `window`, `split_at`, `dedup`, `intersperse`, `unique`, `unique_by`. Sliced results return the same type (Swift `SubSequence`).
  - Conditions: `sum`/`product` require `Numeric` elements; `sort`/`min`/`max` require `Ord`. `string` lacks `sum`/`product`; `map` lacks `sort`/`min`/`max`/`sum`/`product`.
  - Unify `list.sum`/`product` into `sum[A: Numeric](xs: List[A]) -> A`, and `list.join` into `join[T: Text](xs: List[T], sep: T) -> T`.
- Remove `min`, `max`, `clamp` from `Ord` defaults and return them to `Integer`/`FloatingPoint` requirements (R-sized-08). On `Ord` they collide with `duration.min(n)` (n minutes) and `string.min(s)` (minimum character).
- (a) Swift (`Sequence`/`Collection` extensions), Rust (`Iterator` provided methods), Kotlin (`Iterable` extensions) define sequence algorithms once for all sequences.
- (d) Additions (`deque` 30, `set` 20, `string` 28, `bytes` 29, `map` 15), plus broader types for `list.sum`/`product`/`join`. One default implementation, not per-type copies.
- issue: #4012

<a id="r-l0-43-撤去した記号を別の意味で使い直さない---採用"></a>

## R-L0-43 Never repurpose removed symbols — ○ Accepted

- Rules:
  1. Never reuse a removed operator/symbol for another meaning. It permanently remains an error explaining its removal.
  2. Reuse function names for another meaning only when old spelling necessarily causes a type error or abort (e.g. `string.count(s, sub)` → predicate `count(s, f)`; old string argument is a type error; R-string-18).
- Reason: even one error epoch between meanings does not erase old code/materials/LLM-learned spelling. The same symbol means different things to different readers and writing eras. Changing integer `a ^ b` from exponentiation to XOR passes checking yet changes the value, violating R-L0-32's no-silent-different-value rule.
- This cancels round-nine R-L0-41 (bit operators and removing `>>` composition). `^` only loses its exponentiation alias and becomes an error (R-L0-39); `>>` remains composition. Bit operations remain functions (R-int-10).
- (d) Additional decision rule.
- issue: #4004

<a id="r-l0-44-入出力の-protocol-readwriteclose-を置き読み書きと閉じるを-io-の総称関数-1-組にする---採用"></a>

## R-L0-44 Add I/O protocols `Read`, `Write`, `Close`; unify reads/writes/closing as one set of generic `io` functions — ○ Accepted

- Current: read/write/close have different names by handle kind. TCP uses `net.tcp_read`, `tcp_write`, `tcp_close`, `tcp_close_listener` (plain `Int` handles); Unix sockets `unix.read`, `unix.write`, `unix.close` (`unix.Fd`); UDP `net.udp_close` (`UdpSocket`); standard I/O `io.read_bytes`, `io.write_bytes` (no handles). The [protocol hierarchy](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/protocol-view.md) has no I/O layer.
- Proposal:
  ```almide
  protocol Read  { effect fn read(h: Self, max: Int) -> Bytes }      // 最大 max バイト。相手が閉じたら空
  protocol Write { effect fn write(h: Self, data: Bytes) -> Unit }   // 全部書く（Rust の write_all）
  protocol Close { effect fn close(h: Self) -> Unit }                // 2 回目も ok

  effect fn io.read[H: Read](h: H, max: Int) -> Bytes
  effect fn io.read_exact[H: Read](h: H, n: Int) -> Bytes            // 既定の実装。足りなければ err
  effect fn io.read_to_end[H: Read](h: H) -> Bytes                   // 既定の実装。閉じるまで全部
  effect fn io.write[H: Write](h: H, data: Bytes) -> Unit
  effect fn io.close[H: Close](h: H) -> Unit
  ```
  - Conforming types: `net.TcpStream` (`Read`, `Write`, `Close`); `net.TcpListener`, `net.UdpSocket` (`Close`); `unix.Fd` (`Read`, `Write`, `Close`); `io.Stdin` (`Read`); `io.Stdout`, `io.Stderr` (`Write`).
  - Put generic functions once in `io`, not per-type modules (same form as `http.header[M: HttpMessage]`; R-http-12).
- (a) 6 languages share read/write across handle kinds: Rust (`Read`/`Write` traits; `read_exact`/`read_to_end` defaults), Go (`io.Reader`, `Writer`, `Closer`, `io.ReadFull`, `io.ReadAll`), Python (file-like objects), Elixir (IO devices), OCaml (channels), Kotlin (`InputStream`, `OutputStream`, `Closeable`). Swift's standard library has no I/O protocols (Foundation `FileHandle` is concrete). Swift supplies the hierarchy skeleton, but majority practice adds absent layers (as with `Container`).
- Spelling: `read`, `write`, `close` are the majority among 9 languages. `read_exact`, `read_to_end` follow Rust (Go uses `ReadFull`, `ReadAll`, but `io.read_all()` already reads all standard input as text, so cannot reuse that name for two forms).
- Name reuse (R-L0-43 item 2): `io.write` currently takes one `Bytes` argument for stdout and is slated for removal in R-io-01. New `io.write(h, data)` has two arguments, so old `io.write(data)` is a type error, not a silent semantic change.
- (d) Removals/additions: L2-net R-net-07, L2-unix R-unix-04, L2-io R-io-05.
- issue: #4015

<a id="r-l0-45-メモリ安全でない標準の関数は-ffiread-と-ffiwrite-の-2-つだけにする---採用"></a>

## R-L0-45 Only `ffi.read` and `ffi.write` are memory-unsafe standard functions — ○ Accepted

- Current: raw-address reads/writes live in import-free `bytes` (`bytes.from_raw_ptr`, `copy_to_ptr`). No standard-wide inventory of functions touching memory outside Almide values exists. This is the boundary between language guarantees and the outside world for memory-safety proof (#4011).
- Proposal:
  1. In the redesigned standard library, only `ffi.read` and `ffi.write` read/write memory outside Almide values (R-ffi-01/02). `ffi.address`, `ffi.to_int` only construct addresses as values, not access memory. (Added 2026-10-29: `ffi.slice`, introduced by the ADR-0031 addendum, likewise only constructs a read-only `Region` within its parent's range; no memory access.)
  2. No public function outside `ffi` has `RawPtr` in its arguments or return type.
  3. `ffi.read`, `ffi.write` are effect fn requiring `import ffi`. #4008 decides rejection in critical and recording `ffi` usage in build records.
  4. Every other standard function handles out-of-range cases by R-L0-33 role (R-L0-46) and has no means to corrupt memory.
- Measurement (1,289 redesigned functions): `RawPtr` appears only in 4 `ffi` functions.
- (a) All 9 languages confine raw-address reads/writes to explicitly named areas (Rust `unsafe`, Go `unsafe`, Python `ctypes`, Kotlin `cinterop`, Swift `UnsafeRawPointer`).
- Proposed gate: check that `RawPtr` appears in public function types only under `ffi`. Adding an `ffi` function requires updating item 1's inventory in the same PR.
- issue: #4008, #4013

<a id="r-l0-46-位置を受け取る関数を全数で表にし役割の規則r-l0-33をゲートにする---採用"></a>

## R-L0-46 Inventory every position-taking function; gate role rules (R-L0-33) — ○ Accepted

- Current: R-L0-33 defines role-based out-of-range handling but has never inventoried and applied it to every position-taking function. This inventory is the subject of the spatial memory-safety check (#4005).
- Total (among 1,289 redesigned functions, those taking `Int` positions/endpoints/offsets): 108 functions. Single-item reads 60, single-item writes/inserts/deletes 33, range extractions 5, others 10 (`list.range` concerns values not positions; `value.index` only builds paths; `bytes.skip`/`is_eof` move/check cursors; `matrix.from_bytes_*` return none; `matrix.slice_rows`/`fs.fold_lines_range` concern ranges).
- All 60 single-item readers can return none by type (`get_*` returns `T?`; `next_*` returns `(T?, Int)`).
- Deviations (measured `almide 0.68.0 dev 437b8f127`):
  - `fs.fold_lines_range(path, -2, 2, …)` clamps start to 0 and folds.
  - `matrix.slice_rows(m, -1, 2)` returns a zero-row matrix.
  - Both extract ranges, so abort negative positions under R-L0-29 (keep positive-overflow clamping).
  - `bytes.skip(b, 0, -5)` returns negative cursor `-5`; subsequent `bytes.read_u8(b, -5)` returns `0`. R-bytes-13/21 already decided “`n < 0` aborts”; measurement confirmed that decision.
- Proposed gate: generate a list of public position-taking functions; reject rows lacking roles, single-item readers unable to return none, and range/write rows lacking negative-position abort fixtures.
- issue: #4005

<a id="r-l0-47-信頼しない入力を読む関数は時間と大きさに上限を持つ---採用"></a>

## R-L0-47 Functions reading untrusted input have time/size limits — ○ Accepted

- Current: among functions reading external bytes/strings, those expanding beyond input size, nesting deeply, or potentially exploding in time have no limits (#4019, #4020).
  - `zlib.decompress`, `inflate`, `gunzip` have unlimited output; compression bombs expand until memory exhaustion aborts.
  - `json.parse` has no depth limit; deep nesting aborts through stack overflow (C-196). One request can terminate a server process.
  - Regex uses backtracking; patterns such as `(a+)+$` with long inputs cause exponential time (ReDoS).
- Proposal (rule): standard functions reading untrusted input satisfy one of:
  1. Time and output size proportional to input size (`base64`/`hex` decode, `url.parse`, `string.split`, etc.; unchanged is fine).
  2. Otherwise, have a default limit and return err when reached, not abort. Document the limit numerically.
  3. Provide one limit-parameter function for changing limits (language lacks default arguments).
- Applications: R-regex-12 (linear-time matching), R-zlib-04 (decompressed-output cap), R-json-04 (nesting-depth cap). HTTP server/client limits already exist (`http_server_core.rs`, `http_route_core.rs`).
- (a) Go (`regexp` linear; `encoding/json` stops at depth 10000), Rust (`regex` linear; `serde_json` depth 128), Python (`json` recursion limit; `zlib`'s `max_length`), Node (`zlib`'s `maxOutputLength`). Go's 2026 resource-exhaustion vulnerabilities in `archive/tar`, `encoding/asn1`, `net/mail` occurred outside this rule.
- (c) Passing external input directly is easiest for an LLM to write. Requiring explicit limits invites omission; default protection avoids it.
- Proposed gate: cross fixtures for the 3 applications at the exact limit and one beyond, with identical err text on both targets.
- issue: #4019, #4020

<a id="r-l0-48-codec-の入力-フィールドを読まない属性decode_strictフィールドの属性の語彙---採用"></a>

## R-L0-48 Codec input: skipped-field attributes, `decode_strict`, field-attribute vocabulary — ○ Accepted

- Current: derived `T.decode` reads all declared fields from input. Giving `type User: Codec = { name: String, is_admin: Bool = false }` request body `{"name": "m", "is_admin": true}` sets `is_admin` true (mass assignment, #4108). Writing field `@skip` or `@serde(skip)` previously discarded attributes without reading them, doing nothing.
- Proposal (rules):
  1. Record fields may use `@skip` (neither decode nor encode) and `@skip_decode` (not read in decode; written in encode). Both require defaults (`= 値`), otherwise E104. A skipped field's input key is treated as unknown. May combine with alias syntax (`@skip_decode role as "role_name": String = "user"`).
  2. `T.decode` continues ignoring unknown keys. For rejection, derive `fn T.decode_strict(v: Value) -> Result[T, String]` for all Codec record types. Before reading fields, find keys outside decoded fields' keys; return the first in document order as `err("unknown key '<k>'")`. Do not derive for variants.
  3. Allowed record-field attributes are only `@builtin`, `@location`, `@skip`, `@skip_decode`. Others are E053 errors (function-attribute E053 remains a warning). Hint Almide spelling for other-language forms (`@serde(skip)` → `@skip`, `@skip_deserializing` → `@skip_decode`, `@redact` → type `Secret[String]`).
- Family: of 4 cells {decode, encode} × {read, do not read}, `@skip`/`@skip_decode` cover 2; unmarked fields cover 1. Deliberately omit output-only hiding (`@skip_encode`); that belongs to type `Secret[A]` or a separate output type. Declare the table once as `CODEC_FIELD_SKIPS` in `crates/almide-frontend/src/attr_vocab.rs`; both derivation halves and gates read it.
- (a) 6 of 8 ignore unknown keys by default (serde, Go, pydantic, Rails, Ecto, Swift). Rejection switches: serde `deny_unknown_fields`, Go `DisallowUnknownFields`, pydantic `extra='forbid'`. Input exclusion: serde `skip`/`skip_deserializing`, Jackson `READ_ONLY`, kotlinx `@Transient`; all require defaults.
- (c) Rejecting unknown keys does not prevent mass assignment: `is_admin` is known. Prevention comes from decoding into an input type then constructing the domain type (Rails strong parameters, Ecto changeset, FastAPI `UserIn`, OWASP), plus `@skip_decode` on types needing bidirectional Codec. CHEATSHEET teaches this pattern.
- (d) `@skip`, `@skip_decode`, `decode_strict` are additions. Unknown-field-attribute errors reject previously accepted code, so have one epoch 15 breaks row. `decode_strict` is derived, not a standard-module function: absent from target-surface.json, described under common unlisted contracts.
- Gates: `spec/wasm_cross/codec_field_skip_matrix.almd` ({unmarked, `@skip`, `@skip_decode`} × input {key absent, key present, unknown key} × {`decode`, `decode_strict`}, plus `encode`; C-428/C-429); `tests/codec_field_skip_test.rs` (check both legs from family table; control ignoring `@skip_decode` verifies admin=false changes to admin=true); `tests/diagnostics/e104-*`, `e053-unknown-field-attribute-*` (C-430).
- Spelling: `decode_strict` follows R-L0-25's “`decode_<変種>` (Result).” No majority spelling for unknown-key rejection switches (serde `deny_unknown_fields`, Go `DisallowUnknownFields`, pydantic `extra='forbid'`; kotlinx rejects by default, relaxes via `ignoreUnknownKeys`). `@skip` is the shortest spelling for bidirectional exclusion shared by serde, Go (`json:"-"`), kotlinx (`@Transient`), Jackson (`@JsonIgnore`). `@skip_decode` rewrites serde `skip_deserializing` with Almide's verb `decode` (R-L0-25).
- issue: #4108

<a id="ゲートの束r-l0-g---採用"></a>

## Gate bundle (R-L0-G) — ○ Accepted

Combine R-L0-02, 03, 04, 07, 09, 10, 12, 16 checks in one test, `tests/stdlib_surface_conventions_test.rs`. Input is interface JSON (including `mut` fields). Shrink each rule's allowlist as L2 proposals resolve exceptions; expansion is forbidden.

- issue: #3949

> Reader explanation of preserved Japanese code comments: in the set example, insertion does not error and does nothing, and the printed length is 0. In the I/O proposal, Read returns at most max bytes and empty when the peer closes; Write writes everything (Rust write_all); Close is also ok the second time. read_exact is a default implementation returning err if insufficient, while read_to_end is a default implementation reading everything until closure. These are historical proposal comments, not new execution results.
