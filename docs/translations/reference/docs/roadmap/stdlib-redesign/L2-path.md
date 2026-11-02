<!-- description: Stdlib redesign L2/L3 — path: pure lexical path text; keep the ten functions, add components and relative_to (roles 6/9 languages have) -->
# L2/L3: path

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](L2-path.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](L2-path.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](L2-path.zh-CN.md)

> Non-normative historical reader snapshot of the [canonical Japanese source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-path.md), pinned to 35297a612fb29e3f78f6587aba253aa91d51f466. Not automatically synchronized. Accepted proposals, measurements and cross-language comparisons are the source’s historical record, not independently rerun tests or claims of current implementation. No referenced commands were executed.
> The header’s “keep ten functions” and initial “no violations” coexist with later accepted SafePath removal and additions. The rule saying only String is accepted/returned coexists with List/Option/Bool signatures; it is not silently repaired. The lexical “reject ..” hint is not a filesystem confinement, symlink or traversal-safety guarantee.

> Last updated: 2026-10-21
> Prerequisites: [L0](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L0-conventions.md), [L1](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L1-categories.md)
> Sources: Rust https://doc.rust-lang.org/std/path/struct.Path.html ; Go https://pkg.go.dev/path/filepath ; Python https://docs.python.org/3/library/pathlib.html ; TS/JS https://nodejs.org/api/path.html ; Gleam https://gleam-stdlib.hexdocs.pm/ (none); Elixir https://hexdocs.pm/elixir/Path.html ; OCaml https://ocaml.org/manual/latest/api/Filename.html ; Kotlin https://kotlinlang.org/api/core/kotlin-stdlib/kotlin.io.path/ ; Swift https://github.com/apple/swift-system (`FilePath`)

<a id="l2-役割の比較"></a>

## L2: Comparison of roles

| Role | Almide | Rust | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin | Swift |
|---|---|---|---|---|---|---|---|---|---|---|
| Join | `join` | `join` | `Join` | `/` / `joinpath` | `join` | — | `join` | `concat` | `resolve` | `appending` |
| Parent | `dirname` | `parent` | `Dir` | `parent` | `dirname` | — | `dirname` | `dirname` | `parent` | `removingLastComponent` |
| Final name | `basename` | `file_name` | `Base` | `name` | `basename` | — | `basename` | `basename` | `name` | `lastComponent` |
| Extension | `extension` | `extension` | `Ext` | `suffix` | `extname` | — | `extname` | `extension` | `extension` | `extension` |
| Name without extension | `stem` | `file_stem` | — | `stem` | — | — | `rootname`+`basename` | `remove_extension` | `nameWithoutExtension` | `stem` |
| Is absolute | `is_absolute` | `is_absolute` | `IsAbs` | `is_absolute` | `isAbsolute` | — | `type` | `is_relative` | `isAbsolute` | `isAbsolute` |
| Normalize (lexically) | `normalize` | — | `Clean` | `normpath` | `normalize` | — | `expand` (also uses cwd) | — | `normalize` | `lexicallyNormalized` |
| Split into components | — | `components` | `SplitList` (different meaning) / `strings.Split` | `parts` | `parse` | — | `split` | — | `iterator` | `components` |
| Make relative | — | `strip_prefix` | `Rel` | `relative_to` | `relative` | — | `relative_to` | — | `relativeTo` | — |
| Traversal-safe type | `SafePath` (`from_string` / `trusted` / `to_string`) | — | `os.Root` (1.24) | — | — | — | — | — | — | — |

<a id="完全性の規則"></a>

### Completeness rules

- `path` performs only lexical computation over strings (pure). Operations that inspect the filesystem (resolving real paths, existence) belong to `fs`.
- Decomposition/composition pairs: {`join` ↔ `components`}, {`dirname` / `basename` ↔ compose with `join`}, {`stem` / `extension`}.
- Failure representation (R-L0-06 items 1 and 4; no sentinels): potentially absent parts use `Option` (`extension`); rejection with a reason uses `Result` (`from_string`'s `..`). The `""` from `dirname("a")` is an “empty parent” value, not “no parent,” so it is not a sentinel (as in Python).
- Ordering: `path` values are `String`. `SafePath` has no ordering, so do not provide `path.compare` (R-L0-16 applies only to ordered types).

<a id="l2-の判定"></a>

### L2 findings

- Missing roles: decomposition into components (6/9), making relative paths (6/9) → R-path-01.
- Duplicates/rule violations: none (`is_absolute` fits R-L0-09; `from_string` follows R-L0-07's direction).
- Measurement (probe-path): `normalize("a/../../b/./c")` gives `../b/c`, `join("a", "/b")` gives `/b`, `extension(".bashrc")` gives none, and `from_string("a/../b")` gives err. As documented.

<a id="案"></a>

## Proposals

<a id="r-path-01-components-と-relative_to-を足す---採用"></a>

### R-path-01 Add `components` and `relative_to` — ○ Accepted

- Content:
  - `fn components(p: String) -> List[String]` — a sequence of nonempty components. An absolute path starts with `"/"`. Folding with `path.join` returns the same path after `normalize`.
  - `fn relative_to(p: String, base: String) -> String?` — if lexically beneath `base`, return that relative path; otherwise none. Do not generate `..`.
- Spelling: `components` for components (Rust/Swift), `relative_to` for relative paths (Python/Elixir; Kotlin `relativeTo`). Neither has a majority, so choose an existing spelling without a semantic collision. Do not use `split`, which conflicts with Python's `os.path.split` (splits in two).
- Supporting L0 rules: L1 criterion 1 (at least five languages), R-L0-06 (none when not beneath the base), R-L0-02 (string-path functions belong in path).
- (a) Components exist in 6/9 languages, relative paths in 6/9.
- (b) Pure computation, identical on native and wasm.
- (c) Without it, an LLM writes `string.split(p, "/")`, producing empty components for a leading `/` or repeated `/`.
- (d) Addition. Usage count 0.
- Proposed gate: property tests for the `components`/`join` round trip and the `relative_to`/`join` round trip (`join(base, relative_to(p, base)!) == normalize(p)`).
- Issue: #3936.

<a id="r-path-02-safepathfrom_stringtrustedto_stringを撤去する---採用"></a>

### R-path-02 Remove `SafePath` (`from_string` / `trusted` / `to_string`) — ○ Accepted

- Current: `path.from_string(s) -> Result[SafePath, String]`, `path.trusted(s) -> SafePath`, `path.to_string(p: SafePath) -> String`. Every other path function accepts `String`; not one standard function accepts `SafePath`.
- Proposal: remove the three functions and `type SafePath`. Add a safe-path type again when redesigning it in a form accepted by `fs` (docs/roadmap/on-hold/secure-by-design.md).
- Supporting L0 rules: the idea in R-L0-06 item 4 (the value lacks the property promised by the name), R-L0-00 item 1 (fallible functions creating typed values from String use `parse` in other modules; only this one uses `from_string`), R-L0-01 (`html.raw` and `path.trusted` are two spellings for the same role).
- (a) Only Go's `os.Root` (1.24) provides a safe-path type in the standard library, integrated with filesystem operations. No language offers a type calling itself “safe” based only on lexical treatment.
- (c) Measurement: both `path.from_string("/etc/passwd")` and `"a/%2e%2e/b"` yield `ok`. Only `..` components are rejected. If an LLM uses this for “safely build a path from user input,” it permits absolute paths while believing the result is safe.
- (d) Three removals. `@deprecated` in epoch 15; remove in epoch 16. Because the shape changes, do not use `almide fix`; instead give a hint: “to reject `..`, check `path.components(p) |> list.contains("..")`.”
- Completeness rule: public `path` functions accept and return only `String`.
- Issue: #3971.

<a id="r-path-03-with_extension-を足す---採用"></a>

### R-path-03 Add `with_extension` — ○ Accepted

- Current: `extension` reads an extension and `stem` returns the name without it, but there is no extension-replacement function.
- Proposal: `fn path.with_extension(p: String, ext: String) -> String`. If `ext` is `""`, remove the extension.
- Basis: L1 addition criterion (at least five languages).
- (a) Rust `with_extension`, Python `with_suffix`, Swift `appendingPathExtension`/`deletingPathExtension`, Kotlin (`resolveSibling`), Elixir `Path.rootname` + concatenation, OCaml `remove_extension` + concatenation.
- (b) Pure string computation.
- (d) One addition.
- Issue: #3985.

<a id="l3-関数表"></a>

## L3: Function table

| Function | Signature (current, mut shown) | Main spellings in other languages | Decision | Proposal |
|---|---|---|---|---|
| `path.from_string` | `fn from_string(s: String) -> Result[SafePath, String]` | Similar to Go `os.OpenRoot` | Remove (along with `SafePath`) | R-path-02 |
| `path.trusted` | `fn trusted(s: String) -> SafePath` | — | Remove (along with `SafePath`) | R-path-02 |
| `path.to_string` | `fn to_string(p: SafePath) -> String` | Rust `to_str` / Python `str(p)` | Remove (along with `SafePath`) | R-path-02 |
| `path.join` | `fn join(base: String, child: String) -> String` | Rust/Node/Elixir `join` / Go `Join` | Keep | — |
| `path.dirname` | `fn dirname(p: String) -> String` | Node/Elixir/OCaml `dirname` / Go `Dir` / Rust `parent` | Keep | — |
| `path.basename` | `fn basename(p: String) -> String` | Node/Elixir/OCaml `basename` / Go `Base` / Rust `file_name` | Keep | — |
| `path.extension` | `fn extension(p: String) -> String?` | Rust/OCaml/Kotlin `extension` / Python `suffix` / Node `extname` | Keep | — |
| `path.is_absolute` | `fn is_absolute(p: String) -> Bool` | Rust/Python `is_absolute` / Go `IsAbs` | Keep | — |
| `path.stem` | `fn stem(p: String) -> String` | Rust `file_stem` / Python `stem` / Swift `stem` | Keep | — |
| `path.normalize` | `fn normalize(p: String) -> String` | Go `Clean` / Python `normpath` / Node `normalize` | Keep | — |
| `path.components` | `fn components(p: String) -> List[String]` (proposed) | Rust `components` / Python `parts` / Elixir `split` | Add | R-path-01 |
| `path.relative_to` | `fn relative_to(p: String, base: String) -> String?` (proposed) | Python/Elixir `relative_to` / Go `Rel` / Node `relative` | Add | R-path-01 |
| `path.with_extension` | `fn with_extension(p: String, ext: String) -> String` (proposed) | Rust `with_extension` / Python `with_suffix` | Add | R-path-03 |
