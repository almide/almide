> Description (fixed history): Stdlib redesign L2/L3 — fs: Bytes is the only byte container (read_bytes/write_bytes keep their names and switch to Bytes, the _raw duplicates retire), stat owns size and mtime, and the _if_exists and streaming families keep their gates.
<a id="l2l3-fs"></a>

# L2/L3: fs

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](L2-fs.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](L2-fs.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](L2-fs.zh-CN.md)

> Non-normative historical redesign snapshot. Pinned original authoritative and unchanged; no automatic synchronization, independent filesystem/security probes or current API verification. Original comparisons, usage counts and measured outcomes remain original-author claims. Preserve initial append_bytes rejection versus later acceptance, initial write/append names versus later text variants, and proposal plain-fn signature versus effect table. Root confinement is explicitly limited by the original check-before-use symlink replacement race; the earlier cannot-escape wording is not a race-free security guarantee. lexical path.normalize is not symlink resolution. Type-placeholder means type; root/segment placeholders retain Japanese source spelling. [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-fs.md)

> Last updated: 2026-10-21
> Prerequisites: [L0](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L0-conventions.md) / [L1](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L1-categories.md) (R-L1-25: temporary directories belong to fs).
> Sources: Rust https://doc.rust-lang.org/std/fs/ / Go https://pkg.go.dev/os / Python https://docs.python.org/3/library/pathlib.html / TS/JS https://nodejs.org/api/fs.html / Gleam https://gleam-stdlib.hexdocs.pm/ (no fs in stdlib) / Elixir https://hexdocs.pm/elixir/File.html / OCaml https://ocaml.org/manual/latest/api/In_channel.html / Kotlin https://kotlinlang.org/api/core/kotlin-stdlib/kotlin.io/java.io.-file/ / Swift https://developer.apple.com/documentation/foundation/filemanager

<a id="l2-役割の比較"></a>

## L2: Role comparison

| Role | Almide | Rust | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin | Swift |
|---|---|---|---|---|---|---|---|---|---|---|
| Read all as text | `read_text` | `read_to_string` | `os.ReadFile`+`string` | `read_text` | `readFile(p, "utf8")` | — | `File.read` | `In_channel.input_all` | `readText` | `String(contentsOfFile:)` |
| Read all as bytes | `read_bytes`(List[Int])・`read_bytes_raw`(Bytes)→ `read_bytes`(Bytes) consolidated | `fs::read` | `os.ReadFile` | `read_bytes` | `readFile(p)` | — | `File.read` | `In_channel.input_all` | `readBytes` | `Data(contentsOf:)` |
| Read a list of lines | `read_lines` | `read_to_string`+`lines` | `bufio.Scanner` | `read_text().splitlines()` | — | — | `File.stream!` | `In_channel.input_lines` | `readLines` | — |
| Read, none if absent | `read_*_if_exists` | — | — | — | — | — | — | — | — | — |
| Stream and fold lines | `fold_lines`・`for_each_line`・`fold_lines_range`・`fold_lines_chunked` | `BufRead::lines` | `Scanner` | `for line in f` | `readline` | — | `File.stream!` | `In_channel.fold_lines` | `forEachLine`・`useLines` | — |
| Write (overwrite) | `write`・`write_bytes`(List[Int])・`write_bytes_raw` → `write`・`write_bytes`(Bytes) | `fs::write` | `os.WriteFile` | `write_text`・`write_bytes` | `writeFile` | — | `File.write` | `Out_channel.with_open_text` | `writeText`・`writeBytes` | `write(to:)` |
| Append | `append` | `OpenOptions::append` | `OpenFile(O_APPEND)` | `open("a")` | `appendFile` | — | `File.write(…, [:append])` | `open_gen [Open_append]` | `appendText` | `FileHandle.seekToEnd` |
| Create directory | `mkdir_p`(including parents) | `create_dir`・`create_dir_all` | `Mkdir`・`MkdirAll` | `mkdir(parents=)` | `mkdir({recursive})` | — | `mkdir`・`mkdir_p` | `Sys.mkdir` | `mkdir`・`mkdirs` | `createDirectory` |
| Remove | `remove`・`remove_all` | `remove_file`・`remove_dir_all` | `Remove`・`RemoveAll` | `unlink`・`shutil.rmtree` | `rm` | — | `rm`・`rm_rf` | `Sys.remove` | `delete`・`deleteRecursively` | `removeItem` |
| Copy / move | `copy`・`rename` | `copy`・`rename` | —・`Rename` | `shutil.copy`・`rename` | `copyFile`・`rename` | — | `cp`・`rename` | —・`Sys.rename` | `copyTo`・`renameTo` | `copyItem`・`moveItem` |
| List / traverse / glob | `list_dir`・`walk`・`glob` | `read_dir` | `ReadDir`・`WalkDir`・`Glob` | `iterdir`・`rglob`・`glob` | `readdir`・`glob` | — | `ls`・`Path.wildcard` | `Sys.readdir` | `listFiles`・`walk` | `contentsOfDirectory` |
| Existence and kind | `exists`・`is_dir`・`is_file`・`is_symlink` | `exists`・`is_dir`・`is_file`・`is_symlink` | `Stat`+`IsDir` | `exists`・`is_dir`・`is_file`・`is_symlink` | `existsSync`・`stat` | — | `exists?`・`dir?`・`regular?` | `Sys.file_exists`・`is_directory` | `exists`・`isDirectory`・`isFile` | `fileExists` |
| Metadata | `stat`・`file_size`・`modified_at` | `metadata` | `Stat` | `stat`・`getsize`・`getmtime` | `stat` | — | `stat` | `Unix.stat` | `length`・`lastModified` | `attributesOfItem` |
| Temporary files | `temp_dir`・`create_temp_file`・`create_temp_dir` | `env::temp_dir` | `CreateTemp`・`MkdirTemp` | `tempfile` | `mkdtemp` | — | `System.tmp_dir` | `Filename.temp_file` | `createTempFile` | `temporaryDirectory` |
| Create/read symbolic links | — | `symlink`・`read_link` | `Symlink`・`Readlink` | `symlink_to`・`readlink` | `symlink`・`readlink` | — | `ln_s`・`read_link` | `Unix.symlink` | — | `createSymbolicLink` |
| Permissions | — | `set_permissions` | `Chmod` | `chmod` | `chmod` | — | `chmod` | `Unix.chmod` | `setExecutable` | `setAttributes` |
| File handles | — | `File` | `os.File` | `open` | `open` | — | `File.open` | `In_channel`・`Out_channel` | `File` | `FileHandle` |
| Resolve real path | — | `canonicalize` | `EvalSymlinks` | `resolve` | `realpath` | — | — | `Unix.realpath` | `canonicalPath` | `resolvingSymlinksInPath` |

<a id="完全性の規則"></a>

### Completeness rules

- **Content-reading family:** three members {text `read_text`, lines `read_lines`, bytes `read_bytes` (`Bytes`)}. Each has exactly one `_if_exists` version (`Result[Option[T], String]`: none if absent, err for other failures). Do not add this suffix to metadata, directory operations, writes or deletions (per the existing `tests/fs_if_exists_family_gate_test.rs` rationale).
- **Line-streaming family:** limited to {`fold_lines`, `for_each_line`, `fold_lines_range`, `fold_lines_chunked`}. Fallible-callback versions exist only for the two sequential operations where the first err is determined (`tests/fs_streaming_family_gate_test.rs`, ADR-0006).
- **Writing family:** {overwrite `write` (String), `write_bytes` (Bytes), append `append` (String)}. Do not provide Bytes append (R-fs-04 below).
- **`Bytes` is the only byte container** (R-L0-07 item 4).
- **Metadata belongs to `stat`.** Do not provide functions merely projecting one `FileStat` field (R-L0-01). Retain existence/kind predicates (`exists`, `is_dir`, `is_file`, `is_symlink`): they return false when absent, unlike `stat` (err when absent).

<a id="l2-の判定"></a>

### L2 verdict

- Type mismatch and duplication: `read_bytes`, `write_bytes`, `read_bytes_if_exists` use `List[Int]`, while same-role `_raw` versions use `Bytes` → R-fs-01 (switch unsuffixed names to `Bytes`, remove `_raw`).
- Duplication: `file_size` = `stat(p).size`, `modified_at` = `stat(p).modified` (measured agreement, probe-fs) → R-fs-02.
- Cross-module duplication: `env.temp_dir` = `fs.temp_dir` → fs remains (L2-env R-env-01).
- Family gate: R-fs-01 reduces `_if_exists` family 4 → 3 → R-fs-03.
- Missing roles (at least 5/9): create one directory level, create/read symbolic links, permissions, file handles, real paths → R-fs-04 (rejections and reasons).

<a id="案"></a>

## Proposals

<a id="r-fs-01-バイトの読み書きを無印の名前のまま-bytes-にし_raw-版を撤去する---採用"></a>

### R-fs-01 Keep unsuffixed byte I/O names, switch to `Bytes`, remove `_raw` versions — ○ Accepted

- Content:
  - Use `fs.read_bytes(p) -> Bytes`, `fs.write_bytes(p, data: Bytes)`, `fs.read_bytes_if_exists(p) -> Bytes?`. Keep names; replace `List[Int]` with `Bytes`.
  - Remove same-role `fs.read_bytes_raw` (→ `fs.read_bytes`), `fs.write_bytes_raw` (→ `fs.write_bytes`), `fs.read_bytes_raw_if_exists` (→ `fs.read_bytes_if_exists`).
- Names: changing types guarantees old code passing/receiving `List[Int]` becomes a type error. No silently different results, so R-L0-14 item 4 permits it. `_raw` only means “version with a different byte-sequence type”; once `Bytes` is the sole container it loses meaning. Align io too (`io.read_bytes`, `io.write_bytes`, L2-io R-io-01).
- Supporting L0 rules: R-L0-07 item 4, R-L0-01, R-L0-14 item 4.
- (a) All nine languages use dedicated byte-sequence types for file bytes; none return number lists.
- (b) `wasi:filesystem`. New `read_bytes`/`write_bytes` reuse the current `_raw` bodies unchanged. These bodies already run on native and wasm (no wall rows).
- (c) `fs.write_bytes` writes only the low 8 bits of Int (`-1` becomes 255). Out-of-range values change silently. `List[Int]` consumes 8 bytes per byte (#2078 measurement).
- (d) Semantic change (type change) and removal.
  - Three unsuffixed functions: old calls become type errors, rewritten with hints and `almide fix`. When `fs.read_bytes(p)!` is used as `List[Int]`, use `bytes.to_list(fs.read_bytes(p)!)`; rewrite `fs.write_bytes(p, xs)` as `fs.write_bytes(p, bytes.from_list(xs))`. Put type changes in epoch 15 `breaks`, counts in `summary`. Usage counts: `read_bytes` 8/13/20/2, `write_bytes` 12/1/13/3, `read_bytes_if_exists` 10/4/5/2.
  - Three `_raw` functions: `@deprecated(since=15, use="fs.read_bytes")` (`write_bytes_raw`: `use="fs.write_bytes"`; `read_bytes_raw_if_exists`: `use="fs.read_bytes_if_exists"`), `removal_epoch = 16`. A name-only one-to-one rename, so `almide fix` rewrites it.
  - Introduce unsuffixed type changes in epoch 15, the same as the `_raw` window. During that window, `_raw` code still works unchanged; only code using unsuffixed names with `List[Int]` becomes a type error.
- Proposed gate: R-L0-G “reject `List[Int]` byte APIs with a Bytes counterpart” (same check as L2-io R-io-01).
- Issue: #3896

<a id="r-fs-02-file_size-と-modified_at-を撤去しstat-に一本化する---採用"></a>

### R-fs-02 Remove `file_size` and `modified_at`, consolidate on `stat` — ○ Accepted

- Content: `fs.file_size(p)` → `fs.stat(p)!.size`, `fs.modified_at(p)` → `fs.stat(p)!.modified`. Also align `stat`'s one-line “Size, kind and mtime” description with actual field names (`size`, `is_dir`, `is_file`, `modified`) (measured: `.mtime` produces E013).
- Supporting L0 rule: R-L0-01 (do not return the same value under two spellings).
- (a) Rust, Go, Node, Elixir and OCaml only have metadata structures. Python (`getsize`) and Kotlin (`length`) also have individual functions.
- (b) Unchanged (`wasi:filesystem` `stat`).
- (c) Post-removal hints are mechanical. Only `stat` needs remembering.
- (d) Remove. `@deprecated(since=15, use="fs.stat(path)!.size")` (`modified_at`: `use="fs.stat(path)!.modified"`), `removal_epoch = 16`. `almide fix` rewrites. Usage counts: `file_size` 8/2/9/1, `modified_at` 3/0/8/1.
- Proposed gate: adding “return value equals one field of another public function's return” to synonym gate rule 5 cannot be detected by body comparison, so stop it with the R-L0-G allowlist (adding a new projection function fails).
- Issue: #3912

<a id="r-fs-03-2-つの族のゲートを新しい形に合わせる---採用"></a>

### R-fs-03 Align the two family gates with the new shape — ○ Accepted

- Content: make the `tests/fs_if_exists_family_gate_test.rs` family three members {`read_text`, `read_lines`, `read_bytes`} (removing `read_bytes_raw_if_exists` changes 4 → 3 in epoch 16). Do not count `@deprecated` rows during the window. Also gate that `read_bytes_if_exists` has type `Bytes?`. Keep the line-streaming family gate unchanged.
- Basis: CLAUDE.md “API families are extended by matrix”, R-L0-14.
- (b) No impact. (c) Family shape matches documentation. (d) Gate-only change.
- Issue: same as R-fs-01
- Issue: #3896

<a id="r-fs-04-59-以上が持つが-almide-に無い役割---不採用いまは足さない"></a>

### R-fs-04 Roles present in at least 5/9 but absent in Almide — × Rejected (not added now)

- Reason: each role is adequately covered by existing functions (`mkdir` by `mkdir_p`, handles by line-streaming functions and `append`), lacks a WASI route (chmod), or has no requests yet (individual reasons below).
- Single-level directory creation `mkdir`: `mkdir_p` suffices. “Err if already present” is used only for locks; having two invites incorrect choices (R-L0-01).
- Creating/reading symbolic links: `wasi:filesystem` has `symlink-at`/`readlink-at`, but no requests yet. `is_symlink` supports detection. Add the `symlink`/`read_link` pair when requested.
- Permissions (chmod): WASI filesystem has no permission operations. Of R-L0-15's three routes, there is no standard WASI interface and no reason (usage request) for temporary host ops. If requested, provide a package (the package decides native permission handling and wasm's lack thereof). Do not add to the standard library.
- File handles: line-streaming functions suffice for reading, `append` for writing. Adding a handle type gives two ways to do the same task (same reason as L1's lazy-iterator rejection).
- Real-path resolution: `path.normalize` suffices lexically. A symlink-following version requires an R-L0-15 implementation (repeated `readlink-at`), so add it when requested.
- Bytes append: `append_<型>` is reserved by R-L0-04 for “append to a `mut` byte sequence”, so `fs.append_bytes` conflicts with naming rules. No usage examples either.
- Issue: none

<a id="r-fs-05-writeappend-を-write_textappend_text-にしappend_bytes-を足す---採用"></a>

### R-fs-05 Rename `write`/`append` to `write_text`/`append_text`, add `append_bytes` — ○ Accepted

- Current: reads use `read_text`/`read_bytes`, writes `write`/`write_bytes`, append only `append`. Writing `write_text` as the counterpart of `read_text` causes E002; no byte-append function exists.
- Proposal: rename `write` → `write_text`, `append` → `append_text`; add `fn fs.append_bytes(path: String, data: Bytes) -> Unit`. Reading/writing/appending each have two forms, `_text` and `_bytes`.
- Supporting L0 rules: R-L0-12 (families align as matrices), R-L0-00 (spellings for the same role differ within Almide).
- (a) Python `write_text`/`write_bytes`; Kotlin `writeText`/`writeBytes`/`appendText`/`appendBytes`. Rust `fs::write` and Go `WriteFile` distinguish by types, so have one, but Almide does not dispatch by type.
- (d) Rename 2, add 1. `@deprecated(since=15, use="fs.write_text")`, `use="fs.append_text"`, `removal_epoch = 16`; rewrite with `almide fix`.
- Completeness rule: whole-file I/O in `fs` comprises six cells, {read, write, append} × {text, bytes}.
- Issue: #3979

<a id="r-fs-06-根を決めてその下だけを触る関数fsopen_rootfsroot_を足す---採用"></a>

### R-fs-06 Add a root and operations confined beneath it (`fs.open_root`/`fs.root_*`) — ○ Accepted

- Current: `fs.*` accepts any `String` path. Joining untrusted input to a base directory allows escaping via `..`, absolute paths or symbolic links (#4022, security table row 10). `path.from_string` rejects lexical `..` but does not inspect symlinks.
- Proposal: add root type `FsRoot`, corresponding to Go 1.24's `os.Root`, and functions operating only beneath it.
  - `effect fn open_root(dir: String) -> FsRoot` (err if not a directory)
  - `root_read_text(root, path) -> String`, `root_read_bytes(root, path) -> Bytes`, `root_write_text(root, path, content) -> Unit`, `root_list_dir(root, path) -> List[String]`, `root_exists(root, path) -> Bool`, `root_remove(root, path) -> Unit`
  - If `path` is absolute, contains `..` escaping above the root, or has any symlink component beneath the root (regardless of its destination), return `err("fs: path escapes root: <path>")` without touching anything. Otherwise return the result of the ordinary fs function on `<根>/<段>` with `.`/`..` folded lexically.
  - Write the checks only once in Almide (`stdlib/fs.almd`), on top of ordinary fs functions. Native and wasm reject the same paths with the same wording.
- Names: a minimal read/write/list/existence/delete set. Prefix existing verbs `read_text`, `read_bytes`, `write_text` (R-fs-05's new spelling), `list_dir`, `exists`, `remove` with `root_`. Argument names: subject `root`, path `path` (R-L0-30), written contents `content`. All return copies, so R-L0-04 verb restrictions do not apply. `root_exists` wraps `Bool` in `Result` to return err, not silently false, when asked about outside the root.
- Supporting L0 rules: R-L0-30, R-L0-15 (same on both targets), R-L0-47 (protect untrusted input by default).
- (a) Go 1.24 `os.Root` (`OpenRoot`, `Root.Open`/`ReadFile`/`WriteFile`/`Remove`), Rust `cap-std` `Dir`, Linux `openat2(RESOLVE_BENEATH)`, WASI preopens. Go follows symlinks that stay inside the root; this design does not follow them (like `RESOLVE_NO_SYMLINKS`). Following requires `read_link`, not yet in fs (R-fs-04).
- (b) wasm: Almide layered over ordinary fs functions, so the same route on both targets.
- (c) Joining external names is the easiest form for an LLM to write. Opening/passing a root means it cannot escape even if checks for `..` or links are forgotten.
- (d) Additions only. Existing function meanings stay unchanged.
- Remaining gap: checks run before the call, so replacing a directory with a link in between creates a race. Closing this requires descriptor-based resolution (native `openat2`, wasm preopens). The program-wide ceiling is `almide.toml` `[permissions] fs` (docs/specs/effect-system.md).
- Gates: `spec/wasm_cross/fs_root_escape.almd` (`..`, absolute paths, nesting), `tests/fs_scope_test.rs` (a link beneath the root points outside; same err on both targets), `spec/stdlib/fs_root_test.almd`.
- Issue: #4022

<a id="l3-関数表"></a>

## L3: Function table

| Function | Signature (current, mut shown) | Main spellings in other languages | Verdict | Proposal |
|---|---|---|---|---|
| `fs.read_text` | `effect fn read_text(path: String) -> String` | Rust `read_to_string` / Python `read_text` / Kotlin `readText` / Elixir `File.read` | Retain | — |
| `fs.read_bytes` | `effect fn read_bytes(path: String) -> List[Int]` | Rust `fs::read` / Go `ReadFile` / Python `read_bytes` | Semantic change (`-> Bytes`; sole byte-reading API) | R-fs-01 |
| `fs.read_bytes_raw` | `effect fn read_bytes_raw(path: String) -> Bytes` | Same as above | Remove (→ `read_bytes`) | R-fs-01 |
| `fs.read_lines` | `effect fn read_lines(path: String) -> List[String]` | Kotlin `readLines` / OCaml `input_lines` | Retain | — |
| `fs.read_text_if_exists` | `effect fn read_text_if_exists(path: String) -> String?` | — (Almide-specific; ADR-0004 D4) | Retain | R-fs-03 |
| `fs.read_bytes_if_exists` | `effect fn read_bytes_if_exists(path: String) -> List[Int]?` | — | Semantic change (`-> Bytes?`) | R-fs-01・R-fs-03 |
| `fs.read_lines_if_exists` | `effect fn read_lines_if_exists(path: String) -> List[String]?` | — | Retain | R-fs-03 |
| `fs.read_bytes_raw_if_exists` | `effect fn read_bytes_raw_if_exists(path: String) -> Bytes?` | — | Remove (→ `read_bytes_if_exists`) | R-fs-01・R-fs-03 |
| `fs.fold_lines` | `effect fn fold_lines[A](path: String, init: A, f: (A, String) -> A) -> A` | OCaml `In_channel.fold_lines` / Kotlin `useLines` | Retain | — |
| `fs.for_each_line` | `effect fn for_each_line(path: String, f: (String) -> Unit) -> Unit` | Kotlin `forEachLine` / Rust `lines()` | Retain | — |
| `fs.fold_lines_range` | `effect fn fold_lines_range[A](path: String, start: Int, end: Int, init: A, f: (A, String) -> A) -> A` | —(Almide-specific) | Semantic change (abort at negative positions) | R-L0-29、R-L0-46 |
| `fs.fold_lines_chunked` | `effect fn fold_lines_chunked[A](path: String, workers: Int, init: A, f: (A, String) -> A) -> List[A]` | —(Almide-specific; `@concurrent`) | Retain | — |
| `fs.write` | `effect fn write(path: String, content: String) -> Unit` | Rust `fs::write` / Go `WriteFile` / Python `write_text` / Elixir `File.write` | Rename → `write_text` | R-fs-05 |
| `fs.write_bytes` | `effect fn write_bytes(path: String, bytes: List[Int]) -> Unit` | Python `write_bytes` / Kotlin `writeBytes` | Semantic change (`data: Bytes`) | R-fs-01 |
| `fs.write_bytes_raw` | `effect fn write_bytes_raw(path: String, data: Bytes) -> Unit` | Same as above | Remove (→ `write_bytes`) | R-fs-01 |
| `fs.append` | `effect fn append(path: String, content: String) -> Unit` | Node `appendFile` / Kotlin `appendText` | Rename → `append_text` | R-fs-04、R-fs-05 |
| `fs.mkdir_p` | `effect fn mkdir_p(path: String) -> Unit` | Rust `create_dir_all` / Go `MkdirAll` / Elixir `mkdir_p` | Retain | R-fs-04 |
| `fs.exists` | `effect fn exists(path: String) -> Bool` | Rust・Python `exists` / Elixir `exists?` | Retain (R-L0-09 verb set) | — |
| `fs.is_dir` | `effect fn is_dir(path: String) -> Bool` | Rust `is_dir` / Kotlin `isDirectory` | Retain | — |
| `fs.is_file` | `effect fn is_file(path: String) -> Bool` | Rust `is_file` / Kotlin `isFile` | Retain | — |
| `fs.is_symlink` | `effect fn is_symlink(path: String) -> Bool` | Rust `is_symlink` / Python `is_symlink` | Retain | — |
| `fs.remove` | `effect fn remove(path: String) -> Unit` | Rust `remove_file` / Go `Remove` / Elixir `rm` | Retain | — |
| `fs.remove_all` | `effect fn remove_all(path: String) -> Unit` | Rust `remove_dir_all` / Go `RemoveAll` / Elixir `rm_rf` | Retain | — |
| `fs.list_dir` | `effect fn list_dir(path: String) -> List[String]` | Rust `read_dir` / Go `ReadDir` / Python `listdir` / Elixir `ls` | Retain (no majority) | — |
| `fs.walk` | `effect fn walk(dir: String) -> List[String]` | Go `WalkDir` / Python `walk` | Retain | — |
| `fs.glob` | `effect fn glob(pattern: String) -> List[String]` | Go `Glob` / Python `glob` / Node `glob` | Retain | — |
| `fs.copy` | `effect fn copy(src: String, dst: String) -> Unit` | Rust `copy` / Elixir `cp` | Retain | — |
| `fs.rename` | `effect fn rename(src: String, dst: String) -> Unit` | Rust・Go・Python・Node `rename` | Retain | — |
| `fs.stat` | `effect fn stat(path: String) -> FileStat` | Rust `metadata` / Go `Stat` / Python `stat` / Node `stat` | Retain (fix one-line description) | R-fs-02 |
| `fs.file_size` | `effect fn file_size(path: String) -> Int` | Python `getsize` / Kotlin `length` | Remove (→ `stat(p)!.size`) | R-fs-02 |
| `fs.modified_at` | `effect fn modified_at(path: String) -> Int` | Python `getmtime` / Kotlin `lastModified` | Remove (→ `stat(p)!.modified`) | R-fs-02 |
| `fs.temp_dir` | `effect fn temp_dir() -> String` | Rust `env::temp_dir` / Go `TempDir` / Elixir `System.tmp_dir` | Retain(`env.temp_dir` absorbed) | L2-env R-env-01 |
| `fs.create_temp_file` | `effect fn create_temp_file(prefix: String) -> String` | Go `CreateTemp` / Python `mkstemp` | Retain | — |
| `fs.create_temp_dir` | `effect fn create_temp_dir(prefix: String) -> String` | Go `MkdirTemp` / Python `mkdtemp` / Node `mkdtemp` | Retain | — |
| `fs.append_bytes` | `effect fn append_bytes(path: String, data: Bytes) -> Unit`(proposed) | Kotlin `appendBytes` / Python `open("ab")` | Add | R-fs-05 |
| `fs.open_root` | `effect fn open_root(dir: String) -> FsRoot`(proposed) | Go `os.OpenRoot` / Rust cap-std `Dir::open_ambient_dir` | Add | R-fs-06 |
| `fs.root_read_text` | `effect fn root_read_text(root: FsRoot, path: String) -> String`(proposed) | Go `Root.ReadFile` | Add | R-fs-06 |
| `fs.root_read_bytes` | `effect fn root_read_bytes(root: FsRoot, path: String) -> Bytes`(proposed) | Go `Root.ReadFile` | Add | R-fs-06 |
| `fs.root_write_text` | `effect fn root_write_text(root: FsRoot, path: String, content: String) -> Unit`(proposed) | Go `Root.WriteFile` | Add | R-fs-06 |
| `fs.root_list_dir` | `effect fn root_list_dir(root: FsRoot, path: String) -> List[String]`(proposed) | Go `Root.Open` + `ReadDir` | Add | R-fs-06 |
| `fs.root_exists` | `effect fn root_exists(root: FsRoot, path: String) -> Bool`(proposed) | Go `Root.Stat` | Add | R-fs-06 |
| `fs.root_remove` | `effect fn root_remove(root: FsRoot, path: String) -> Unit`(proposed) | Go `Root.Remove` | Add | R-fs-06 |
