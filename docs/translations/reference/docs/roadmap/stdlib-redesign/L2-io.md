Description: Stdlib redesign L2/L3 — io: stdin/stdout; read_line keeps its name and distinguishes EOF with Option; read_bytes/write_bytes use Bytes like fs; writers become effect fns as ADR-0022 says.
<a id="l2l3-io"></a>

# L2/L3: io

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](L2-io.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](L2-io.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](L2-io.zh-CN.md)

> Non-normative historical redesign snapshot; original authoritative and unchanged. No automatic synchronization, new probes or current API verification. Preserve the sequence of proposals: R-io-01/02 initially retain or rename byte APIs, R-io-04 is withdrawn, and later R-io-05 instead unifies handle-based I/O. Do not apply old migration snippets as current instructions. The effect/Option call form remains deferred to R-L0-17 despite the resolved heading. Empty bytes at EOF remain distinct from the rejected line/Int sentinels; this is not a new universal I/O safety guarantee. F marks and usage counts are source-era research. [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-io.md)

> Last updated: 2026-10-21
> Prerequisites: [L0](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L0-conventions.md), [L1](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L1-categories.md).
> Sources: Rust https://doc.rust-lang.org/std/io/; Go https://pkg.go.dev/bufio; Python https://docs.python.org/3/library/sys.html#sys.stdin; TS/JS https://nodejs.org/api/process.html#processstdin; Gleam https://gleam-stdlib.hexdocs.pm/gleam/io.html (output only); Elixir https://hexdocs.pm/elixir/IO.html; OCaml https://ocaml.org/manual/latest/api/In_channel.html; Kotlin https://kotlinlang.org/api/core/kotlin-stdlib/kotlin.io/readln-or-null.html; Swift https://developer.apple.com/documentation/swift/readline(strippingnewline:).
> (F) = cells fetched and verified in this investigation.

<a id="l2-役割の比較"></a>

## L2: Role comparison

| Role | Almide | Rust | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin | Swift |
|---|---|---|---|---|---|---|---|---|---|---|
| Read one line (distinguish EOF) | `io.read_line_opt` | `read_line` (0 at EOF) | `Scanner.Scan` | `readline()` ("" at EOF) | `readline` event `line` | — | `IO.gets`（`:eof`） | `In_channel.input_line`（option） | `readlnOrNull`（null）(F) | `readLine()`（nil） |
| Read one line (EOF becomes "") | `io.read_line` | — | — | `sys.stdin.readline()` | — | — | — | — | — | — |
| Read all | `io.read_all` | `read_to_string` | `io.ReadAll` | `sys.stdin.read()` | — | — | `IO.read(:eof)` | `In_channel.input_all` | `System.in.readAllBytes` | `FileHandle.readDataToEndOfFile` |
| All lines as a list | `process.stdin_lines` | `lines()` | `Scanner` | `readlines()` | — | — | `IO.stream` | `input_lines` | `generateSequence(::readLine)` | — |
| Read bytes | `io.read_byte` (-1 at EOF)・`io.read_n_bytes`（List[Int]） | `read(&mut buf)` | `Read(p)` | `sys.stdin.buffer.read(n)` | `stdin.read(n)` | — | `IO.binread(n)` | `In_channel.input` | `System.in.read` | `FileHandle.readData(ofLength:)` |
| Write text without newline | `io.print` | `print!` | `fmt.Print` | `print(end="")` | `process.stdout.write` | `io.print` | `IO.write` | `print_string` | `print` | `print(terminator: "")` |
| Write bytes | `io.write`（Bytes）・`io.write_bytes`（List[Int]） | `write_all` | `os.Stdout.Write` | `sys.stdout.buffer.write` | `stdout.write(buf)` | — | `IO.binwrite` | `Out_channel.output_bytes` | `System.out.write` | `FileHandle.write` |

<a id="完全性の規則"></a>

### Completeness rules

- Stdin reading has three forms: {one line `read_line`, all `read_all`, up to n bytes `read_bytes`}. Do not add a list of all lines: compose `string.lines(io.read_all()!)` (R-L0-01's composable forms).
- Failure representation (R-L0-06 item 4: do not return sentinel values): a line is none at EOF; bytes are empty `Bytes` at EOF (same as `net.tcp_read`, like Rust `read` returning 0). Do not mark EOF with `-1` or `""`.
- `Bytes` is the sole byte-sequence container (R-L0-07 item 4). Match fs's names `read_bytes` / `write_bytes` (L2-fs R-fs-01).
- All output functions are effect fns. ADR-0022 D1 specifies that `io.print`, `io.write`, `io.write_bytes` are effect fns; calling them from pure fn gives E006.
- Built-in `eprintln` owns stderr output.

<a id="l2-の判定"></a>

### L2 verdict

- Rule violation (sentinels): `io.read_line` gives `""` for both EOF and empty lines; `io.read_byte` gives `-1` at EOF → R-io-01.
- Duplicate: `io.read_line_opt` and `io.read_line` share the one-line role → R-io-01 (make the unsuffixed name return `Option`).
- Type mismatch/duplicate: `io.read_n_bytes`, `io.write_bytes` use `List[Int]`, while same-role `io.write` uses `Bytes` → R-io-01.
- Wrong location: `process.stdin_lines` (stdin belongs to io) → R-io-01.
- Declaration hole: `io.write` and `io.write_bytes` are declared as pure `fn`. Measurement allows calling `io.write` from a pure fn without error (probe-io), violating ADR-0022 D1 → R-io-02.

<a id="案"></a>

## Proposals

<a id="r-io-01-読み取りをeof-を区別する-1-行bytesにそろえる---採用"></a>

### R-io-01 Unify reads into an EOF-distinguishing line and Bytes — ○ Accepted

- Content:
  - One line: keep `io.read_line`, change its return from `String` to `Option[String]` (none at EOF). Remove `io.read_line_opt` (→ `io.read_line`).
  - Byte reads: add `effect fn read_bytes(n: Int) -> Bytes` (up to n bytes, empty at EOF and for `n <= 0`); remove `io.read_byte` (→ `io.read_bytes(1)`) and `io.read_n_bytes` (→ `io.read_bytes(n)`).
  - Byte writes: keep `io.write_bytes`, change its argument from `List[Int]` to `Bytes`. Remove same-role `io.write` (`Bytes`) (→ `io.write_bytes`). This makes the read/write pair `read_bytes` / `write_bytes`, matching fs.
  - All lines: remove `process.stdin_lines()` (→ `string.lines(io.read_all()!)`, L2-process row).
- L0 basis: R-L0-06 items 1/4 (unsuffixed name is safe; no failure sentinels), R-L0-07 item 4 (`Bytes` is the sole byte container), R-L0-01 (one spelling per role), R-L0-14 item 4 (keep a name if old code necessarily becomes a type error).
- (a) Swift `readLine`, Kotlin `readlnOrNull` (F), OCaml `input_line` return EOF via `Option` (nil/null/none); Elixir also returns distinct `:eof`. Only Python `readline()` marks EOF with `""`.
- (b) Stdin is `wasi:cli/stdin`, stdout `wasi:cli/stdout`. Existing bodies work on native and the embedded host (no wall rows). New `read_line` uses the `read_line_opt` body; `read_bytes` uses `read_n_bytes` with return type changed to `Bytes`; new `write_bytes` uses `io.write`. ADR-0027 D2's fixed stdin permits deterministic inspection.
- (c) Code reading `io.read_line()` in a `while` and stopping on `""` stops prematurely at empty lines. LLMs miss this error. With `Option`, comparing to `""` becomes a type error, forcing a rewrite.
- (d) Semantic change (types) and removal.
  - `io.read_line`: return `Option[String]`. Old `String` usage becomes a type error, allowed under R-L0-14 item 4. Hint: “`io.read_line()` returns none at EOF; `io.read_line()! ?? ""` restores old behavior,” with `almide fix` rewrite. Record in epoch 15 `breaks`, counts in `summary`.
  - `io.read_line_opt`: `@deprecated(since=15, use="io.read_line")`, `removal_epoch = 16`. `almide fix` rewrites `io.read_line_opt()` to `io.read_line()` one-to-one.
  - `io.read_byte`: `@deprecated(since=15, use="io.read_bytes(1)")`; `io.read_n_bytes`: `@deprecated(since=15, use="io.read_bytes(n)")`; both `removal_epoch = 16`. Since `read_byte` changes shape (`Int` → `Bytes`), give only a hint: “none from `bytes.get_u8(io.read_bytes(1)!, 0)` means EOF.” `almide fix` rewrites `read_n_bytes` to `bytes.to_list(io.read_bytes(n)!)`.
  - `io.write_bytes`: argument becomes `Bytes`, making old code a type error; `almide fix` rewrites to `io.write_bytes(bytes.from_list(xs))`.
  - `io.write`: `@deprecated(since=15, use="io.write_bytes")`, `removal_epoch = 16`; `almide fix` rewrites one-to-one.
  - `process.stdin_lines`: handled in L2-process R-process-01.
  - Usage: `io.read_line` 5/24/20/8, `io.read_byte` 1/16/13/1, `io.read_n_bytes` 7/16/23/8, `io.write_bytes` 7/4/21/1, `io.write` 12/8/27/3. Change CHEATSHEET:885-890's `io.read_line` description to “returns `Option[String]`, none at EOF” (`check-llm-surface.sh`).
- Proposed gate: add “reject `List[Int]` byte APIs” to R-L0-G, shrinking io/fs/string allowlists. Native/wasm EOF fixtures distinguish EOF after an empty line as none versus some("").
- Issue: #3896.

<a id="r-io-02-iowrite_bytes-を-effect-fn-に直す---採用"></a>

### R-io-02 Correct `io.write_bytes` to effect fn — ○ Accepted

- Content: change `fn write_bytes(data: …) -> Unit` to `effect fn write_bytes(data: Bytes) -> Unit` (type change in R-io-01). Also change `io.write`, retained during the window, to `effect fn` in the same epoch.
- L0 basis: R-L0-13 (functions touching environment are effect) and ADR-0022 D1 (only 6 built-ins admitted in pure fn; neither `io.write` nor `io.write_bytes` is among them).
- (a) Not applicable (few languages have effect types).
- (b) Checker-only change.
- (c) One must read declarations to know `io.print` is effect but `io.write` pure. If ADR and implementation disagree, LLM code based on docs disagrees with checker results.
- (d) Semantic change (pure → effect), no renaming. Precedents are dialect epochs 8/13 (argv reads made effect); likewise add to `breaks`. Calls from pure fn become E006, with “make the caller `effect fn`” hint. Count in epoch 15 `breaks`. Usage: 12/8/27/3 (`io.write`), 7/4/21/1 (`io.write_bytes`).
- Proposed gate: compare ADR-0022 D1's name set with the `effect` field in `io` declarations.
- Issue: #3907.

<a id="r-io-03-改行なしで標準エラーに書く-ioeprint-を足しprint-の綴りから-ioprint-へ案内する---採用"></a>

### R-io-03 Add newline-free stderr `io.eprint`, guide `print` to `io.print` — ○ Accepted

- Current: newline-free output exists only as stdout `io.print`, not stderr. `print("x")` suggests “perhaps `println`?”, changing output by adding a newline.
- Proposal: add `effect fn io.eprint(s: String) -> Unit`. Guide undefined `print` / `eprint` to `io.print` / `io.eprint` (R-L0-19).
- (a) All 9/9 have newline-free stderr (Rust `eprint!` (F), Gleam `io.print_error` (F), Go `fmt.Fprint(os.Stderr, …)`, etc.).
- (b) `wasi:cli/stderr`, same path as `eprintln`.
- (c) LLMs almost always put progress displays (rewriting with `\r`) on stderr.
- (d) Addition and diagnostics.
- Completeness rule: output is the 4 cells {stdout, stderr} × {with/without newline}; built-ins own newline forms, `io` the others.
- Issue: #3974.

<a id="r-io-04-標準入力をバイト列で全部読む-ioread_all_bytes-を足す---取り消し十三巡目ioread_to_endiostdin-で書く--r-io-05"></a>

### R-io-04 Add `io.read_all_bytes` for all stdin bytes — × Withdrawn (thirteenth pass; use `io.read_to_end(io.stdin())` → R-io-05)

- Current: `io.read_all() -> String` (all text), `io.read_bytes(n) -> Bytes` (up to n bytes); no function reads all as bytes.
- Proposal: `effect fn io.read_all_bytes() -> Bytes`.
- (a) 8/9 have it (Python `sys.stdin.buffer.read()` (F), Go `io.ReadAll(os.Stdin)`, Rust `read_to_end`, etc.).
- (b) `wasi:cli/stdin`; same path as `read_all`, without converting to UTF-8.
- (c) Today, going through `io.read_all()` turns invalid UTF-8 into replacement characters, silently changing results.
- (d) Add 1.
- Completeness rule: stdin reads are {one line `read_line`, all text `read_all`, all bytes `read_all_bytes`, up to n bytes `read_bytes`}.
- Issue: #3974.

<a id="r-io-05-標準入出力を取っ手にしバイトの読み書きを-ioreadiowrite-に一本化する---採用"></a>

### R-io-05 Make standard streams handles, unify byte I/O as `io.read` / `io.write` — ○ Accepted

- Content:
  ```almide
  type Stdin    // Read
  type Stdout   // Write
  type Stderr   // Write
  effect fn stdin() -> Stdin
  effect fn stdout() -> Stdout
  effect fn stderr() -> Stderr
  ```
  - Read/write bytes just like other handles: `io.read(io.stdin(), n)`, `io.read_to_end(io.stdin())`, `io.write(io.stdout(), data)` (R-L0-44).
  - Do not add R-io-01's `io.read_bytes` or R-io-04's `io.read_all_bytes`. Remove `io.write_bytes` (→ `io.write(io.stdout(), data)`).
  - Keep text shortcuts `io.read_line`, `io.read_all`, `io.print`, `io.eprint`. All 9 languages have shortcuts to read a line/write text without a handle (`readLine`, `print`).
- (a) Standard input/output conform to the same read/write types as other handles in 6 languages: Rust `io::stdin()` (`Read`), Go `os.Stdin` (`io.Reader`), Python `sys.stdin.buffer`, Elixir `:stdio`, OCaml `stdin` (channel), Kotlin `System.in` (`InputStream`). Three distinct types, like Rust, make writing to stdin a type error.
- (d) Remove `io.write_bytes` (whose type was to change under R-io-01), and add the new APIs. `almide fix` rewrites `io.write_bytes(d)` to `io.write(io.stdout(), d)`.
- Issue: #4015.

<a id="l3-関数表"></a>

## L3: Function table

| Function | Signature (current, mut shown) | Main spellings elsewhere | Verdict | Proposal |
|---|---|---|---|---|
| `io.read_line` | `effect fn read_line() -> String` | Swift `readLine` / Kotlin `readlnOrNull`(F) / OCaml `input_line` | Semantic change (`-> String?` return; the sole line reader) | R-io-01 |
| `io.read_line_opt` | `effect fn read_line_opt() -> String?` | Same as above | Remove（→ `read_line`） | R-io-01 |
| `io.read_all` | `effect fn read_all() -> String` | Rust `read_to_string` / Go `io.ReadAll` / OCaml `input_all` | Keep | — |
| `io.read_byte` | `effect fn read_byte() -> Int` | Rust `read` / Python `read(1)` | Remove（→ `io.read(io.stdin(), 1)`） | R-io-01・R-io-05 |
| `io.read_n_bytes` | `effect fn read_n_bytes(n: Int) -> List[Int]` | Python `buffer.read(n)` / Elixir `binread(n)` | Remove（→ `io.read(io.stdin(), n)`） | R-io-01・R-io-05 |
| `io.print` | `effect fn print(s: String) -> Unit` | Rust `print!` / Go `fmt.Print` / Gleam `io.print` | Keep | — |
| `io.write` | `fn write(data: Bytes) -> Unit` | Rust `write_all` / Go `Stdout.Write` / Elixir `binwrite` | Semantic change (`Write` generic function `write(h, data)` replaces it; old one-argument calls become type errors) | R-L0-44・R-io-05 |
| `io.write_bytes` | `fn write_bytes(data: List[Int]) -> Unit` | Python `buffer.write` / Kotlin `writeBytes` | Remove（→ `io.write(io.stdout(), data)`） | R-io-01・R-io-02・R-io-05 |
| `io.eprint` | `effect fn eprint(s: String) -> Unit`（Proposal） | Rust `eprint!` (F) / Gleam `io.print_error` (F) / OCaml `prerr_string` | Add | R-io-03 |
| `io.stdin` | （Add）`effect fn io.stdin() -> Stdin` | Rust `io::stdin` / Go `os.Stdin` / Python `sys.stdin` | Add | R-io-05 |
| `io.stdout` | （Add）`effect fn io.stdout() -> Stdout` | Rust `io::stdout` / Go `os.Stdout` / Python `sys.stdout` | Add | R-io-05 |
| `io.stderr` | （Add）`effect fn io.stderr() -> Stderr` | Rust `io::stderr` / Go `os.Stderr` / Python `sys.stderr` | Add | R-io-05 |
| `io.read` | （Add）`effect fn io.read[H: Read](h: H, max: Int) -> Bytes` | Rust `Read::read` / Go `Reader.Read` / Python `read(n)` | Add | R-L0-44 |
| `io.read_exact` | （Add）`effect fn io.read_exact[H: Read](h: H, n: Int) -> Bytes` | Rust `read_exact` / Go `io.ReadFull` | Add | R-L0-44 |
| `io.read_to_end` | （Add）`effect fn io.read_to_end[H: Read](h: H) -> Bytes` | Rust `read_to_end` / Go `io.ReadAll` / Python `read()` | Add | R-L0-44 |
| `io.close` | （Add）`effect fn io.close[H: Close](h: H) -> Unit` | Rust（drop）/ Go `Closer.Close` / Python `close` | Add | R-L0-44 |

<a id="l0-への差し戻し処理済み"></a>

## Referral back to L0 (resolved)

1. **When an unsuffixed name belongs to a sentinel-returning version**: settled by R-L0-14 item 4. Changing return `String` to `Option[String]` necessarily makes old code a type error, so use the unsuffixed name for the safe version. `io.read_line` returns `Option[String]`; remove `read_line_opt` (R-io-01). No `_opt` suffix.
2. **`!` and `??` for effect fns returning `Option`**: settled by R-L0-17 (call type depends only on declaration). The call to `io.read_line` depends solely on `effect fn read_line() -> String?`, not the body's implementation. Which form to choose is designed in the R-L0-17 issue (same as L2-env R-env-03).
