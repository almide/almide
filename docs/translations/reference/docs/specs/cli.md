<a id="cli-specification"></a>

# CLI Specification

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](cli.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](cli.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](cli.zh-CN.md)

> Non-normative reader translation of the [mixed Japanese/English canonical source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/cli.md), pinned at `35297a612fb29e3f78f6587aba253aa91d51f466`. This snapshot is not automatically synchronized and is not generator/checker input. The original remains authoritative. Commands, runtime flags, measurements, dates, implementation status and security claims below are retained as statements of that pinned source, not independently executed or validated for a current release. Code blocks and their comments remain verbatim. Generated-table markers are inactive reader notes. Compilation-budget excess handling does not imply a blanket guarantee that every compiler operation is crash/OOM/hang-free; profile, capability, platform and artifact boundaries retain their stated scope.


> Last updated: 2026-10-31

<a id="overview"></a>

## Overview

```
almide <command> [options] [arguments]
```

If the project root contains `almide.toml` + `src/main.almd`, the file argument may be omitted. `src/main.almd` is used automatically.

---

<a id="commands"></a>

## Commands

<a id="almide-run"></a>

### `almide run`

Compile and immediately run. With the default native target: generate Rust source → `cargo build` → execute the binary.

```bash
almide run                              # src/main.almd を実行
almide run app.almd                     # 指定ファイルを実行 (native)
almide run app.almd --target wasm       # wasm をビルドして埋め込み wasmtime ホストで実行
almide run -- --flag value              # -- 以降はプログラムの引数
almide run app.almd -- arg1 arg2        # ファイル指定 + プログラム引数
```

| Option | Description |
|---|---|
| `--target rust\|wasm` | Execution target: `rust` (default, native binary) or `wasm` (the structural emitter generates WASM, executed by the embedded wasmtime host; neither an external `wasmtime` CLI nor rustc installation is needed). Unsupported programs encounter a diagnostic wall, without falling back to another renderer. See [WASM-OUTPUT](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/wasm/WASM-OUTPUT.md) for the scope and limits of cross-target equivalence |
| `--no-check` | Skip type checking |
| `--locked` | Refuse fetching dependencies absent from `almide.lock` and resolution that changes the lock; do not write the lock (#4093). Intended for CI |
| `--release` | Optimized build: opt-level=3, fat LTO, codegen-units=1. On x86_64, **tune for `x86-64-v3` (AVX2/FMA; Intel Haswell from 2013 onward, AMD Zen from 2017 onward)** (#3604). Measurements (spectralnorm, runtime `n`) for generic → v3 were 87.8 → 67.2 ms on AMD Zen3 and 57.1 → 45.4 ms on Intel Xeon 8581C. `native` was instead slower on Zen3 at 109.8 ms; it was made the default and then reverted (#3727 → #3734). CPUs without AVX2 fail with `SIGILL`; use `--target-cpu portable` when distributing to them. Other targets, such as aarch64, remain generic |

`almide`'s own flags (`--target` / `--no-check` / `--release`) are interpreted before `--`; everything after `--` is passed unchanged to the program (the same convention as `cargo run`). Calling `env.args()` in the program returns the arguments after `--` as `List[String]`.

Test: `tests/run_target_flag_test.rs`

**There is no execution-time limit** (#2615): `almide run` lets the program run to completion on either target. Just as a native binary has no time limit, the embedded host for `--target wasm` installs no watchdog (previously it stopped after 30 seconds with `wasm trap: interrupt`, also paying an epoch-check cost at every loop head). Time limits belong to harnesses, which install them themselves: the in-process test runner (`run_wasm` / `run_wasm_with` / `run_wasm_capped`) has a 30-second watchdog configurable through `ALMIDE_WASM_WATCHDOG_SECS`. Fuzzing (`tools/xtarget-fuzz`'s `--timeout`) and sweep scripts have their own per-process limits. Test: `tests/wasm_run_no_time_limit_test.rs`

**Native build cache** (#2500): the native target builds generated Rust in a shared scratch directory (`ALMIDE_RUN_PROJECT_DIR`, default `<cache base>/run`) and stores the binary at `target/<profile>/almide-<hash>`, named by a content hash of the generated code. Identical generated code reuses this one binary whether requested by `almide run` or `almide build`, without invoking cargo. All directory writes (builds, eviction, cleanup) are serialized under `.almide-build.lock`; only cache hits run without the lock.

- **Location** (#4180): all native build caches (`run`, `build-cdylib`, runtime rlibs `almide-rtlib-<key>`, and `almide test` workers `test/native`) live beneath the per-user private base `<temp>/almide-<euid>`. The base is created exclusively with mode 0700. If it exists, it is used only when it is a real directory owned by the running user with no permissions for group or others. Otherwise it is refused (exit 1), not repaired, because its contents may have been written by someone else. This closes the hole in shared temporary storage such as Linux `/tmp` where a binary planted in a directory someone else created first could run with your privileges.
- **Hit condition** (#4180): at build time, the binary's SHA-256 is recorded in `almide-<hash>.sha256`; a hit requires the contents to match that record. A missing or mismatching record causes a rebuild. Test: `tests/build_cache_private_test.rs`

- **Eviction**: each hit refreshes the binary's mtime. At the next miss, `almide-<hash>` files unused for 7 days (and corresponding-generation `almide_out-*` objects remaining in `deps/`) are deleted under the lock. This sweep runs once a day (`.almide-evict-stamp`). Incremental sessions are untouched.
- **Size cap** (#2608): when the combined size of the same `almide-<hash>` and `almide_out-*` files exceeds `ALMIDE_CACHE_MAX_BYTES` (default `4G`; `0`/`off` disables it), files are deleted oldest-mtime first under the same lock. Files used within the last 2 minutes are retained. This sweep runs once a minute (`.almide-size-stamp`). Dependency-crate rlibs and incremental sessions are excluded. The default is twice the measured per-program artifact footprint on the development machine (2.0 GB).
- **Recovery from rustc ICE**: when an interrupted build (ENOSPC, kill) corrupts rustc's incremental session, subsequent builds of that form repeatedly fail with `the compiler unexpectedly panicked`. If a failed build's output contains this banner, `target/*/incremental` is deleted under the same lock and the build is retried once. On success, stderr receives the single line `note: rustc crashed on a stale incremental session; cleared … and rebuilt successfully`. If the retry also fails, the original error is reported unchanged (no loop).
- **`almide clean`** empties this directory (below).

Test: `tests/run_cache_recovery_test.rs`

**Wasm build cache**: `--target wasm` (and `almide bench`) stores the rendered Wasm module at `target/wasm/almide-<key>` in the same directory, avoiding re-rendering when rerunning an unchanged program. The key includes everything read by the structural leg: entry-file path and source; paths and sources of all modules read from disk by the resolver (own modules and dependency packages); dependency directory; working directory; the compiler itself (version line plus executable path, size and mtime, including bundled stdlib); and all `ALMIDE_*` environment variables. Parsing, type checking (including warnings), `[permissions]`, IR validation and E081 run before every hit, so hits emit the same diagnostics and reject the same inputs as misses. Only rendering (the structural leg's own front, emit and validation) is skipped. Rendering stopped at a wall is not cached. Hit conditions (recorded SHA-256 and agreement), 7-day eviction, the `ALMIDE_CACHE_MAX_BYTES` cap and `almide clean` match native artifacts. With `ALMIDE_TIME_PHASES=1`, the `[phase] render (cache hit)` line shows a hit. Test: `tests/wasm_run_cache_test.rs`

**Files without `main`**: on either target, run the file's `test` blocks with the same verdict as `almide test <file> --target <leg>` (emitting one stderr line: ``note: <file> declares no `main`; running its test blocks …``). Exit 0 if all pass, 1 if any fails, or 5 (“nothing to run”) if there are no `test` blocks either (`almide test`'s zero-count handling, #2204). Previously native ran the libtest harness directly while wasm refused. Program arguments after `--` are unused. Test: `tests/wasm_run_cache_test.rs`

**stdout buffering** (#2245): native binaries' `println` / `io.print` / `io.write(io.stdout()!, …)` go through one 64 KiB buffer (in program order). When stdout is a terminal, every write flushes. Writes containing a newline flush even to pipes or files. Other flush points are termination, panic, `main` returning err, before starting a subprocess, before reading stdin, and `io.print`, which always flushes (`io.print("")` is an explicit flush). stderr (`eprintln`) is unbuffered, so relative order with stdout is not preserved away from a terminal. Details: [docs/stdlib/io.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/io.md).

<a id="実行可能なスクリプト"></a>

### Executable scripts

Put a shebang at the start of the file and grant execute permission to run it directly. The environment must support `/usr/bin/env -S`, and `almide` must be on PATH.

`-S` is not decorative. Linux passes the rest of the shebang to `env` as **one argument**; without `-S`, `#!/usr/bin/env almide run` fails while looking for a program named `almide run`. macOS splits at whitespace, so it happens to work there. `almide check` warns with E062 on an `env` shebang containing two or more words without `-S`.

```almd
#!/usr/bin/env -S almide run

fn main() -> Unit = {
  println("shebang ok")
}
```

```bash
chmod +x script.almd
./script.almd
```

A shebang is recognized only at the start (including immediately after an optional UTF-8 BOM). Line numbers are preserved, and `fmt` keeps the shebang ahead of imports and the dialect stamp. `#` elsewhere remains an error.

Tests: `spec/cli/shebang.almd`, `tests/hot_native_regressions_test.rs`

---

<a id="almide-build"></a>

### `almide build`

Compile to a binary.

```bash
almide build                            # src/main.almd → パッケージ名のバイナリ
almide build app.almd -o myapp          # 出力ファイル名指定
almide build app.almd --target wasm     # WASM バイナリ（直接 emit、rustc 不要）
almide build app.almd --target linux-musl -o app  # Linux の静的 musl バイナリ（#2772）
almide build app.almd --target x86_64-unknown-linux-musl  # 任意の rustc ターゲット三つ組
almide build app.almd --target wasm --host js -o dist/app.wasm  # + dist/app.js, dist/app.d.ts (JS ホスト、#2265)
almide build app.almd --target wasm --debug   # + DWARF 行テーブル（.debug_* カスタムセクション、#1315）
almide build --release                  # 最適化ビルド (opt-level=3, fat LTO, x86_64 は x86-64-v3 向け)
almide build --release --target-cpu portable -o app  # AVX2 の無い CPU にも配るバイナリ（rustc の汎用 CPU）
```

| Option | Description |
|---|---|
| `-o <path>` | Output-file path. It means the same on every native route (binary / `--cdylib` / `--repr-c` / their combination): write to exactly the specified path, creating missing parent directories (#3349). Neither an extension nor a `lib` prefix is added |
| `--target wasm` | Generate a WASM binary (direct emission) |
| `--target <triple>` | Native-binary target (#2772): `rust` / `native` (default, this host), `linux-musl` (`<arch>-unknown-linux-musl` for the host architecture, x86_64 or aarch64), or a rustc target triple. Pass `--target` to cargo and collect artifacts from `target/<triple>/<profile>/`. Install the target's standard library with `rustup target add <triple>`. musl targets use Rust's default **static linking** (`crt-static`); almide adds no linker flags (glibc targets remain dynamic). On an x86_64 host, a dependency-free program can link with rustc's bundled musl crt without extra tools. Cross-architecture builds or `[native-deps]` containing C need the target linker / C compiler (such as `musl-tools`). Unknown values not shaped like triples, and `wasm32-*`, are refused with exit code 2 rather than silently building for the host |
| `--host js` | Only with `--target wasm`: write JS host `<mod>.js` (a dependency-free ES module) and `<mod>.d.ts` beside the module (#2265). `init(source?, hooks?)` instantiates it, `run()` invokes `main`, and each `pub fn` gets a wrapper. Wire `@extern(wasm, "js", "name")` through `init({ js: { name } })` (an extern declared as `effect fn` or `Result[T, String]` receives hook throw / reject as err, propagatable with `!` on the Almide side; if a hook throws for any other extern, the instance is discarded and cannot be called until `init()` again—#3356). For a hook returning Promise, write `@extern(wasm, "js", "name", returns: promise)` on the extern (#3353 / #3371): glue wraps it in `WebAssembly.Suspending`, infers from the call graph only the exports (and `run()`) that can reach it, and makes them async functions through `WebAssembly.promising` (`Promise<T>` in `.d.ts`). Calls into the instance are serialized one at a time in invocation order; a synchronous export attempted while an async call is suspended throws without entering. On engines without JSPI, `init()` refuses with a reason. Language types, effects and callers are unchanged; native ignores it. If an unmarked hook returns a thenable, it throws `almide: hooks.js.<name> returned a Promise; mark its @extern with returns: promise` and discards the instance (not err, even for a fallible extern). The provisional `--async-import` flag on develop was removed before v0.67.0-rc1 and appeared in no release. Design: docs/wasm/JS-HOST-ASYNC-IMPORTS.md. Marshaling supports Int (`number`, range-checked to ±2^53) / Float / Bool / String / Unit; exports additionally support Bytes (`Uint8Array`) / List[T] (arrays, nesting allowed) / Option[T] (`T | undefined`) / records (plain objects with declaration-order fields), for both arguments and returns (#3354; layout and build-side ownership follow what the emitter recorded per export). A `pub fn` with any other boundary type (variant / Map / Set / tuple / function) is refused at build time with its type named. Returns of `effect fn` (and functions returning `Result[T, String]`) are Result blocks in the module; wrappers use the return ABI recorded by the emitter: ok becomes a `T` value, err throws `AlmideError` (an `Error` subclass) with its message. An export whose record disagrees with the source type is also refused at build time with the function named (#3352). Only what the program uses is shipped (#2276): `__alloc`/`__release` exports and String glue helpers only when `String` crosses the boundary; WASI shims only for names imported by the shipped module (after `--wasm-opt`). Gate: `scripts/check-js-host.sh` (runs `spec/wasm_host_js/` in node, matches expected and native output, checks module byte identity, shim sets and `glue-ceiling.txt` limits). Specification: the “JS host” section of docs/wasm/WASM-OUTPUT.md |
| `--debug` | Only with `--target wasm`: append DWARF 4 `.debug_info` / `.debug_abbrev` / `.debug_line` / `.debug_str` / `.debug_ranges` custom sections to the shipped module (after `to_wasi`) to map code offsets (byte offsets from the Code section contents, the same “DWARF for WebAssembly” convention as LLVM/Emscripten) to `.almd` file:line:col (#1315). One compile unit per source file, one `DW_TAG_subprogram` (name, `low_pc`/`high_pc`) per function. Chrome DevTools and wasmtime + lldb read them without flags. The emitter records lines per instruction from IR spans (synthetic nodes lacking spans inherit the nearest ancestor span; instructions whose ancestors also lack one—such as exit-time releases—get line 0). Code itself is unchanged: default output without `--debug` remains byte-identical, outside `size-baseline*.txt`. Cannot combine with `--component` / `--wasm-opt` (both rewrite shipped bytes, invalidating table addresses; rejected with exit code 2). Also refused for native targets |
| `--release` | Optimized build: opt-level=3, fat LTO, codegen-units=1. On x86_64, **tune for `x86-64-v3` (AVX2/FMA; Intel Haswell from 2013 onward, AMD Zen from 2017 onward)** (#3604). Measurements (spectralnorm, runtime `n`) for generic → v3 were 87.8 → 67.2 ms on AMD Zen3 and 57.1 → 45.4 ms on Intel Xeon 8581C. `native` was instead slower on Zen3 at 109.8 ms; it was made the default and then reverted (#3727 → #3734). CPUs without AVX2 fail with `SIGILL`; use `--target-cpu portable` when distributing to them. Other targets, such as aarch64, remain generic |
| `--target-cpu <cpu>` | CPU tuning for native `--release` builds (#3604). Default depends on the target family: `x86-64-v3` on x86_64, rustc's generic CPU otherwise (determined by the target triple even in cross-builds). `portable` selects the generic CPU; other values pass unchanged as rustc CPU names (`native`, `x86-64-v4`, `neoverse-n1`, etc.). `native` is an error in cross-builds; specifying this option with `--target wasm` is also an error (wasm is CPU-independent). The runtime rlib is built for the same CPU, and the build-cache key includes the CPU |
| `--fast` | Same as `--release` (retained for compatibility) |
| `--unchecked-index` | Disable array bounds checks (unsafe) |
| `--no-check` | Skip type checking |
| `--locked` | Refuse fetching dependencies absent from `almide.lock` and resolution that changes the lock; do not write the lock (#4093). Intended for CI |
| `--repr-c` | Apply `#[repr(C)]` to struct/enum (C ABI compatibility) |
| `--cdylib` | Generate a shared library (`.so` / `.dylib` / `.dll`). Without `-o`, use `lib<名前>.<拡張子>` (name/extension) in the current directory (`<名前>.dll` on Windows). Cargo's crate name comes from the package name (`[package].name` in `almide.toml`, or the entry filename when absent), not `-o` (#3349) |

**`CARGO_BUILD_TARGET`** (#2772): without `--target`, the `CARGO_BUILD_TARGET` environment variable is treated like `--target <triple>` (`--target rust` cancels it and selects the host). In either case almide explicitly passes the target to cargo and removes `CARGO_BUILD_TARGET` from the child environment. Previously this variable leaked to cargo, placing artifacts in `target/<triple>/` while almide picked up `target/<profile>/almide-out`; it silently output the previous host binary or failed with `expected binary not found`. Old artifacts there are deleted before building, so even if cargo's `build.target` configuration redirects output, no old binary is emitted: it stops with `expected binary not found`. The triple is in the build-cache key. Since `almide run` / `almide test` execute binaries on this machine, they always build for the host (`CARGO_BUILD_TARGET` is not passed to child cargo).

Default output filename:
- With `almide.toml`: `[package] name`
- Otherwise: source filename without `.almd`

<a id="rustc-の下限4110"></a>

#### Minimum rustc version (#4110)

Native builds (`almide run` / `build` / `test` / `bench`, cdylib and install routes) read `rustc --version` (rustc on PATH and `$RUSTC` if configured) before starting rustc or cargo. Below **rustc 1.97.0**, they refuse with exit code 1 without building anything. The error includes the detected and required versions and the fix (`rustup update`). The floor is defined in one place, `MIN_RUSTC` in `src/cli/rustc_floor.rs`, as max(1.77.2, the `RUST_TOOLCHAIN` CI uses to validate generated code). 1.77.2 fixes CVE-2024-24576 (Windows `Command` did not sanitize `.bat` / `.cmd` arguments for cmd.exe); binaries built with old rustc reopen shell injection through `process.exec`. `--target wasm` (direct emission) does not use rustc and is excluded.

---

<a id="almide-test"></a>

### `almide test`

Discover and execute `test "name" { ... }` blocks in `.almd` files.

```bash
almide test                             # カレントディレクトリ以下を再帰スキャン
almide test spec/lang/                  # ディレクトリ指定
almide test spec/lang/expr_test.almd    # ファイル指定
almide test --run "pattern"             # テスト名でフィルタ
almide test --target wasm               # WASM ターゲットでテスト
almide test --json                      # 結果を JSONL (1行1ファイル) で出力
almide test --update-snapshots x_test.almd  # スナップショットの受理(期待値リテラルを書き換え)
```

| Option | Description |
|---|---|
| `-r, --run <pattern>` | Test-name pattern filter (below) |
| `--no-check` | Skip type checking |
| `--locked` | Refuse fetching dependencies absent from `almide.lock` and resolution that changes the lock; do not write the lock (#4093). Intended for CI |
| `--json` | Output results as JSON |
| `--target wasm` | Execute only the wasm leg (no native fallback) |
| `--update-snapshots` | Accept `testing.assert_snapshot` mismatches and rewrite the caller's expected-value literal in source (same as `ALMIDE_UPDATE_SNAPSHOTS=1`) |
| `--ci` | CI mode: never write snapshots (same as `CI=true`). Both new and divergent snapshots fail |
| `--allow-no-tests` | Exit 0 even if there are zero tests to execute (default 5) |
| `--show-output` | Display everything all tests wrote to stdout / stderr, including passing tests |

Every execution reports the **number of tests run**: `2 tests in 1 file`. When `--run` excludes anything, the filtered count is included, as in `0 tests in 1 file (2 filtered out)`. With `--json`, each line has `"tests"` and `"filtered_out"`.

**Zero-count handling** (#2084, #4264):

| Situation | Exit code | Reason |
|---|---|---|
| `--run` matches nothing | **5** | Nothing ran. A typo in a CI filter must not look green. The message states that the pattern did not match. Opt out with `--allow-no-tests` |
| No tests were available to execute | **5** | An accident. Opt out with `--allow-no-tests` |
| Tests existed, but all files were skipped and none ran | **5** | Nothing ran (#4325). The message says `no test ran: N file(s) were skipped`. Opt out with `--allow-no-tests` |

“No tests” applies **both when no test files were found and when a named file contains no `test` blocks**. 5 is a dedicated code distinct from 1 (test failure), so callers can distinguish them without reading output.

This table is **target-independent**: for the same file and arguments, `--target wasm` returns the same exit code and count line (`0 tests in 1 file`). A file with neither `main` nor `test` blocks is “nothing to run,” not a renderer wall (WALL: that leg declined); both legs return 5 (#2204).

**Skips are green only if at least one test ran in the same execution** (#4325). `--target wasm` has three kinds of skip, each counted in the summary (`1 passed, 0 failed, 2 skipped: 1 walled,
1 declared `// wasm:skip` (of 3 files)`; count line: `1 test in 3 files, 2 files skipped`):

| Skip | Meaning |
|---|---|
| declared (`// wasm:skip` in the first 3 lines) | The author declared, with a reason, that this file cannot run on wasm |
| walled (`WALL` line) | The wasm leg could not lower the program (same verdict as E082) |
| host unavailable | The environment cannot start the lane (for example, CLI missing with `ALMIDE_TEST_WASM_RUNNER=wasmtime`) |

All mean “tests exist but did not run on this leg,” not a pass. Thus one rule applies: **if zero tests ran across the execution, return 5 regardless of skips**. A declared `// wasm:skip` file also returns 5 when run alone with `--target wasm` (nothing ran). In a directory run, if even one test in another file ran, the declared skip remains green. Previously skips caused exit 0 even at zero tests, letting negative controls that could not lower on wasm pass as `0 passed, 0 failed, 1 skipped`.

**An `@extern(wasm)` import absent from the embedded host is not a miscompile** (#4324). A program importing `@extern(wasm, "m", "f")` is refused with the same wording as `almide run --target wasm` (`this program imports `m.f` … has no host for it` and a `--host js` hint):

- Default `almide test` (wasm first + native fallback): the file runs natively, with `note: <file>: the wasm leg cannot run this file; its tests run on the native fallback` and the reason. It is not a WASM TRAP attributed to a “wasm-only miscompile.”
- `--target wasm`: **FAIL** (exit 1), as with `almide run --target wasm`. Declare a file requiring that host with `// wasm:skip <理由>` (reason).

Tests: `tests/test_zero_outcomes_test.rs`; cross-target parity: `tests/test_zero_exit_parity_test.rs`; skip and missing-host verdicts: `tests/wasm_test_skip_verdict_test.rs`

**A failing test's output follows its failure report** (#2538), so intermediate values printed with `println` / `eprintln` are not discarded when it fails:

```
FAILED: e.almd
  test: eprintln inside a failing test
  at:   e.almd:4
  expected: 2
  found:    1
  stdout:
    STDOUT: value is 42
  stderr (whole file — stderr carries no per-test boundary):
    DEBUG: value is 42
```

- **stdout is per test.** Native (libtest with `--test-threads=1`) and the wasm runner both print test boundaries to stdout; the interval is that test's output.
- **stderr is per file.** Neither harness writes test boundaries to stderr, so it cannot be split and includes stderr from passing tests in the same file. The label says so.
- Passing-test output is silent by default. `--show-output` displays it for all tests.
- Identical on native, wasm, and the default lane (wasm first, failures rerun natively).

Test: `tests/test_failure_shows_output_test.rs`

**Running a file's tests runs only that file's own `test` blocks** (#2550). Imported modules' `test` blocks are not included in the importing file's build; they run when their own module file is tested. Consequently, `almide test` on a directory runs each test exactly once, and reports failures using the path and source name of the file defining the test. Identical on native, wasm, and the default lane.

Test: `tests/imported_module_tests_not_rerun_test.rs`

`--run <pattern>` is a **case-sensitive substring match against the test's own name** (the string in `test "…"`, or `名前 / case`—name / case—for a `where` case) (#4264). Passing the exact name from a failure report selects it: `--run "beta fails"` (spaces intact) and non-ASCII names such as `--run "値が無ければ"` both match. Generated function names (`__test_almd_0001_beta_fails`, `almide_ir::test_fn_name`) are not matched. Previously they were, so names containing spaces, symbols or non-ASCII characters (folded and hashed in function names) could not be selected using their own names.

This rule is **identical for native and wasm legs** (#2085). There is one predicate, `almide_base::names::test_name_matches_filter`: wasm filters with it while synthesizing the runner; native uses it to select full test paths, then passes them with `--exact` to the Rust harness. A pattern matching nothing exits 5 as in the table above.

**A file's verdict is not determined by exit code alone** (#4255). Calling `process.exit(0)` inside a test ends the process with 0, but the remaining tests have not run. Native appends each test's name to the file designated by `ALMIDE_TEST_LEDGER` when its body reaches the end (`test_module` template; the harness's own record, not program stdout), and passes only when completion records exist for every selected test. Wasm's embedded host checks that the runner's `main` returned all the way. Missing completions fail the file and name the first test that did not finish.

Test: `tests/wasm_test_filter_parity_test.rs`

The wasm leg runs in the wasmtime host embedded in the `almide` binary (the same as `almide run --target wasm`), so PATH need not contain `wasmtime` (#3046). Only with `ALMIDE_TEST_WASM_RUNNER=wasmtime` does it execute the stock artifact from `almide build --target wasm` (after `to_wasi`) with the `wasmtime` CLI (a comparison route). On either route, if any file's wasm leg could not start, emit once per cause `note: the wasm leg could not start for N file(s) (<原因>); they run on the native fallback` (cause). A count alone is never the only trace.

Test: `tests/test_wasm_leg_runner_test.rs`

Scratch artifacts (`.wasm` modules passed to wasmtime by the CLI route) live in a per-execution unique `<cache base>/test-run-<pid>-<nonce>/`, named by a hash of the file's **absolute path**, and are deleted on exit (`ALMIDE_KEEP_SCRATCH=1` retains them and prints the location to stderr). Concurrent runs of same-named files or same-named files in different directories never share a path (#1877). The native fallback build cache persists at `<cache base>/test/native/` (also keyed by absolute path).

There is **one worker directory per absolute test-file path**, each with its own `target/`. Left alone, nothing disappears (renamed tests, deleted worktrees and vanished branches leave their directories; 4,510 directories / 39 GB measured on 2026-09-22). Therefore `almide test` **empties worker directories unused by anyone for 7 days** on startup (#2504). It judges mtimes of “files directly under that directory and under `target/<profile>/`”: cache hits refresh the executed binary's mtime (#2500), so a directory that was used survives even without a build. The sweep runs once a day (`native/.almide-evict-stamp`), acquires each directory's `.almide-build.lock` **without waiting**, and skips it if acquisition fails (another process is building). It retains the lockfile, leaving an otherwise empty directory (reason in [`almide clean`](#almide-clean)). No sweep with `ALMIDE_KEEP_SCRATCH=1`. `almide clean` empties all worker directories regardless of age.

The age rule does not fire during **a burst testing many different files** (on 2026-09-24, 7,144 directories / 84.9 GB in 2 hours filled the disk, #2608). Hence a **size cap** covers the whole worker cache: above `ALMIDE_CACHE_MAX_BYTES` (default `4G`, disabled by `0`/`off`), empty worker directories in oldest-last-use order by the same procedure (nonblocking per-directory lock, retain lockfile, recheck under lock that last use has not changed). Directories used within the last 2 minutes are retained even above the cap (protecting the interval between a lock-free cache hit's existence check and exec). This sweep runs once a minute (`native/.almide-size-stamp`). The default 4 GiB is twice a measured week-long working set (65 worker directories / 2.0 GB) on a development machine with multiple worktrees and 5 concurrent agents.

The same rule covers the **runtime rlib cache** `<cache base>/almide-rtlib-<key>/` (#2504). One directory exists per runtime-source × rustc-version × optimization-level combination. Rebuilding the compiler or upgrading the toolchain changes the key, leaving old directories never linked again (31 directories / 102 MB measured on 2026-09-22). Every link refreshes the rlib mtime; directories not linked for 7 days are emptied, excluding the currently used one. The sweep runs once a day; its stamp is `<cache base>/.almide-evict-stamp`.

Tests: `tests/test_scratch_race_test.rs`, `tests/run_cache_recovery_test.rs`

Failures use a **structured block** (`FAILED: <file>` followed by `test:` / `at:` / `hint:` / `diff:` or `expected:` `found:`). Multiline strings, lists and records get actual per-unit diffs. Failures within a file appear in **source-line order**, so two runs can be diffed directly.

`--json` emits JSONL, one line per file:

```json
{"file":"spec/lang/x_test.almd","status":"fail","exit_code":101,
 "failures":[{"name":"string mismatch","file":"spec/lang/x_test.almd","line":4,
              "op":"assert_eq","expected":"\"a\\nB\"","found":"\"a\\nb\"",
              "diff":"      a\n    - B\n    + b\n","message":"…"}]}
```

Writing tests:

```almide
test "addition" {
  assert_eq(1 + 2, 3)
}

test "string concat" {
  assert_eq("a" + "b", "ab")
}
```

- `test` blocks can appear in any `.almd` file
- The `*_test.almd` suffix is conventional, not mandatory
- A `test` block is implicitly an effect context (I/O calls are allowed)

<a id="スナップショット-testingassert_snapshot"></a>

#### Snapshots (`testing.assert_snapshot`)

The expected value is a **literal in source** (the second argument), with no sidecar file (expect-test style). Start with `""`, then run `almide test --update-snapshots <file>` to write the measured value back as a literal (heredoc for multiple lines). Only the literal at the call is rewritten; the rest of the file stays byte-identical.

```almide
import testing

test "render" {
  testing.assert_snapshot(render(x), "")   // → --update-snapshots で実測値に書き換わる
}
```

- Mismatches are reported as ordinary failures (`expected:` / `found:` or `diff:`, with an acceptance command on the `accept:` line). The runtime stop block is identical on both targets (`Error: snapshot mismatch` / `at: line N` / `expected:` / `found:`, exit 1, contract C-336).
- `--update-snapshots` repeats “execute → rewrite → rerun” per file until convergence (execution stops at the first mismatch). If argument 2 is not a literal (a variable or interpolated string), it fails without rewriting.
- In CI mode (`--ci` / `CI=true`), `--update-snapshots` writes nothing, and both new (`""`) and divergent snapshots fail. Commit and review snapshots like code.

---

<a id="almide-check"></a>

### `almide check`

Type checking only, without binary generation. For CI and editor integration.

```bash
almide check                            # パッケージ内: src/ 配下の .almd を全部チェック (#2165)
almide check app.almd                   # 指定ファイルをチェック
almide check --deny-warnings            # 警告をエラーとして扱う
almide check --json                     # 診断を JSON で出力(パッケージ内なら src/ 全部、#2253)
almide check --explain E001             # エラーコードの説明
almide check --effects                  # 各関数のエフェクト分析を表示
almide check --timings                  # フロントエンドの phase 別内訳
almide check app.almd --profile critical --allow IO  # critical profile (#567)
```

| Option | Description |
|---|---|
| `--deny-warnings` | Treat warnings as errors |
| `--profile critical` | Apply the bounded profile to every function; start capabilities at deny-all (below) |
| `--allow <CAP>` | Grant one capability under `--profile critical` (repeatable) |
| `--json` | Output diagnostics as JSON, one per line, for editor/agent integration |
| `--explain <code>` | Explain an error code (same as `almide explain <code>`) |
| `--budget NAME=N` | Set one compilation-cost budget (below, repeatable; refused under `--profile critical`) |

If `./almide.toml` contains a key no reader consumes (such as `brnach = "main"`), every command, including `check` / `build` / `run` / `test`, warns once (#3382): it gives the line (`almide.toml:LINE`), key, keys accepted by that table, and candidates at edit distance 1. The build proceeds as though the key were absent, with no exit-code change (`--deny-warnings` does not count it either). With `--json`, it appears on stdout as a `"level":"warning"` line; otherwise on stderr. Accepted keys per table: [package-system.md §6](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/package-system.md#6-dependency-declaration).

`almide explain --list [--json]` (#2149) emits every code, one per line: `{code, mnemonic, severity, since, verdict}`. `mnemonic` is the title of `docs/diagnostics/<CODE>.md`; `verdict` is its `## Fix-it verdict` (mechanical / conditional / not-fixable); `severity` (error / warning / error|warning) and `since` (the release first emitting the code) come from `docs/diagnostics/codes.toml`. The set of rows matches the compiler's emitted code set (77 from `with_code`, plus E081–E083 emitted directly as `error[EXXX]` by build routes). Severity is checked against actual levels in all fixtures (`tests/explain_list_test.rs`).
| `--effects` | Per-function effect/capability analysis. A function calling a received closure is shown as `{} + whatever f (arg 1) does`, not counted as pure (#3268). Closure category sets flow with values and are charged at the caller's call site (ADR-0026 D1, `docs/specs/effect-system.md`). `[permissions]` violations show a path on a `path:` line (D4) |
| `--timings` | Per-phase wall time for lex / parse / check (#1311) |

<a id="コンパイルコストの予算adr-0030-4136"></a>

#### Compilation-cost budgets (ADR-0030, #4136)

The compiler handles untrusted input (PR CI, LSP upon opening a file, graders, playground), so every phase whose work grows with input has a named **budget**. Budgets count nodes, instances, bytes and levels; they do not measure seconds or available memory. Thus the same input gets the same verdict on every machine, in the native or wasm-hosted compiler, in CLI or LSP. Exceeding a budget ends with a defined diagnostic, not a crash, OOM or hang.

- **Source cannot raise budgets.** No attribute, `almide.toml`, `almide.lock` or environment-variable route reads a budget.
- **Only the command line can raise them**: `--budget NAME=N` (repeatable, shared by all subcommands). `--profile critical` rejects `--budget` (it judges under default budgets).
- `almide budgets` lists them; `almide budgets --md` emits precisely the table below (`tests/compile_budget_test.rs` checks equality).
- Hidden flag `--budget-report` prints each budget's observed maximum and counted work to stderr on exit (read by `scripts/measure-compile-budgets.sh` and the gate above).

> Source-generation marker (inactive in this reader snapshot): almide budgets --md: begin

| Budget | Default | Counts | Exceeding it |
|---|---|---|---|
| `nesting` | 500 levels | syntactic nesting of one expression or pattern, counted by the parser as it descends | parse error `expression nesting too deep (max N)` |
| `type_size` | 256 nodes | nodes of one inferred type, counted as the checker builds an expression's type and as it resolves an inference variable — `let t1 = (t0, t0)` doubled 22 times was 3.7 GB (#4114) | E111 |
| `generic_instances` | 1000 instances | specialized instances of protocol-bounded generic fns one program monomorphizes, each instance's type arguments also held to `type_size`; polymorphic recursion would add a larger one per round without end | E112 |
| `manifest_bytes` | 1048576 bytes | size of one `almide.toml` or `almide.lock` read, the project's own or a dependency's, checked before and while it is read (#4194) | error `<file> is larger than N bytes` |
| `dependency_count` | 1000 packages | distinct packages one dependency resolution visits, the transitive closure included, counted before each is fetched | error `too many dependencies (N, max M)` |
| `dependency_depth` | 64 levels | length of the chain of dependencies from the root package to the one being resolved, counted before it is fetched | error `dependency chain too deep (N, max M)` |
| `exhaustiveness_steps` | 400 steps | recursive steps of the exhaustiveness and reachability check of one `match` (pattern-matrix usefulness is exponential in the worst case) | E113 |

> Source-generation marker (inactive in this reader snapshot): almide budgets --md: end

<a id="--profile-critical"></a>

#### `--profile critical`

`--profile critical` (#567) applies the bounded profile (ALS §B, E070–E078) to **every function**. No `@bounded` attribute is needed. Code passing critical always passes ordinary mode too (a subset, not a dialect). Higher-order calls and closure creation produce E074.

Capabilities start at deny-all. Host-touching stdlib calls produce E076, and only those granted with `--allow` pass. The `--allow` vocabulary and modules each grants are `CAPABILITY_GRANTS` in `crates/almide-frontend/src/check/bounded.rs`:

| `--allow` | Modules granted |
|---|---|
| `IO` | `io`, `fs` |
| `Net` | `http`, `net`, `unix` |
| `Env` | `env`, `args` |
| `Time` | `datetime`, `duration` |
| `Rand` | `random`, `uuid` |
| `Process` | `process` |

There is no `Fan`. `fan.*` lies outside the critical profile (#1628). This vocabulary is distinct from the leaf / parent capability vocabulary of `[permissions].allow` in `almide.toml`. For example, critical uses `Process` while the manifest uses `Proc`; the manifest also has `Fan`. For exact leaves and parent expansions, see `docs/specs/effect-system.md` §8.

```
$ almide check rd.almd --profile critical
error[E076]: an effect outside the declared capability is not admissible under `--profile critical`
  hint: capabilities start deny-all — grant one with --allow IO|Net|Env|Time|Rand|Process
$ almide check rd.almd --profile critical --allow IO
No errors found
```

`--profile critical` rejects memory-unsafe `ffi.*` calls and declarations of `@extern` functions with E100; `--allow` cannot grant them. Even ordinary mode requires an explicit import to use `ffi`. The current surface consists of opaque `Handle` and `Region` carrying length and owner, not examples using removed `RawPtr` or `bytes.from_raw_ptr`. See [ffi](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/ffi.md) and `stdlib/ffi.almd` (ADR-0031).

Regardless of profile, `almide build` lists use of `@extern` and foreign pointers on the `Built` line. This displays the guarantee boundary; absence of that display is not proof of complete memory safety.

Test: `tests/build_outside_guarantee_test.rs`

The following are refused (all with exit code 1):

- A name outside the vocabulary: ``error: unknown capability `io` in --allow — grantable capabilities are IO, Net, Env, Time, Rand, Process`` and ``hint: did you mean `IO`?``. Wording matches rejection by `[permissions].allow` (#3247).
- `--allow` without `--profile`: `error: --allow requires --profile critical`
- A profile name other than `critical`: ``error: unknown profile `strict` — the only profile is `critical` ``
- Combining with `--effects`: `error: --profile is not supported with --effects`

`--json` and the whole-package form (FILE omitted) use the same profile verdict.

Tests: `tests/critical_profile_test.rs`, `tests/manifest_permissions_test.rs` (`critical_allow_suggests_the_nearest_capability`)

<a id="--timings"></a>

#### `--timings`

Split frontend time into lex / parse / check and emit two lines on stderr: first human-readable, second machine-readable (key names are an API read by the per-phase ratchet in `scripts/check-edit-loop-scale.sh`):

```
$ almide check --timings spec/lang/expr_test.almd
timings: lex 1.8ms (16.9%, 2443k lines/s) parse 1.6ms (15.3%, 2692k lines/s) check 3.9ms (35.9%, 1149k lines/s) | other 3.4ms | total 10.7ms over 4426 lines in 43 sources
almide-timings {"lex_ns":1812250,"parse_ns":1644211,"check_ns":3851626,"total_ns":10717583,"lines":4426,"bytes":109057,"sources":43}
```

- Accounting is enabled only with `--timings`. Runs without it never read the clock (so measuring does not perturb the measured system; measured A/B data is at the start of `research/benchmark/editloop/scale.py`).
- `lines` / `sources` cover **everything actually lexed**: entry and own-project modules, plus auto-imported bundled stdlib that every check must process. This is the denominator for lines/sec.
- `other` is the remainder outside the three phases (file I/O, import resolution, canonicalize, lowering for unused warnings). An unnamed remainder could absorb any regression.
- A check ending in error emits no timings, because time is spent in diagnostic rendering.

Tests: `tests/check_timings_test.rs` (complete key set, every phase nonzero, no double counting, no output without `--timings`). Ratchet: `scripts/check-edit-loop-scale.sh`.

Each `--json` line is `{level, code, message, hint, here, try, try_replace, applicability, suggestions, repair?, context, file, line, col, end_col, secondary}`. `try` is a pasteable fix snippet, `try_replace` the span it replaces, and `suggestions[]` the same fix structured as `{line, col, end_col, replacement, applicability}`.

**`repair`** (#2149): structured repair field, emitted only for diagnostics with a fix (bytes of diagnostics without fixes remain unchanged). Shape: `{"primary": Edit|null, "alternatives": [Edit], "example": "…"|null}`, where `Edit` matches a `suggestions[]` element: `{line, col, end_col, replacement, applicability}`.

- `primary`: one span-exact edit for which the diagnostic takes responsibility. `almide fix` applies only those whose `applicability` is `"machine-applicable"` unattended. A code classified **mechanical** by `## Fix-it verdict` in `docs/diagnostics/<CODE>.md` must emit this as machine-applicable (`tests/diagnostic_coverage_test.rs` checks the binary and verifies `almide check` passes after `almide fix`). Conversely, a non-mechanical code emitting machine-applicable also fails (`diagnostic_harness_test.rs`).
- `alternatives`: edits based on other interpretations, always `"maybe-incorrect"`, never applied automatically.
- `example`: display-only snippet (placeholders allowed) when no span-exact edit exists.

`suggestions[]` is `primary` followed by `alternatives`. `suggestions` in `almide fix --json` and LSP quickfixes are built from the same `repair` (a machine-applicable `primary` is preferred in LSP).

**Position units** (#2250): `line` is a 1-based line number; `col` / `end_col` are **1-based character counts** (Unicode scalar values), not bytes or display width. Insertion at the end of a 66-character (98-byte) line containing CJK has `col = 67`. `end_col` is exclusive (up to before that column); `col == end_col` is a zero-width insertion point. A byte-counting harness corrupts UTF-8.

**Positions always refer to real locations in the file**: `${…}` on the third heredoc line is reported on that line and column (formerly on the string's first line, at a column counting the entire decoded template). When a call's `)` is on a later line, `Span` lacks an end line and carries only the `(` span. In that case E041/E042's `!` fix appears only in `try`, without `try_replace` / `suggestions` (omit a position rather than inserting `!` in the middle of an expression). Tests: `tests/fixit_span_on_line_test.rs`, `tests/interpolation_span_parity_test.rs`.

**Syntax errors also emit JSON**: for a file with no successful top-level declaration, `Parser::parse` returns `Err` and shared `parse_file` emits human text on stderr and exits. `--json` extracts diagnostics directly from the parser and emits JSON on this route too (the error kind LLMs produced most often had been the sole text-only kind).

**Exit codes match plain `almide check`** (#2350): `1` if there is a parse error or any diagnostic with `level` `error`, `0` for warnings only. Previously every parseable file returned `0`, outputting type errors as JSON lines while dropping them from the exit code. Consumers of `--json` are editors, CI and harnesses, not humans; they branch on exit code and inspect the payload only on failure—so every rejected program had passed as “successful.” This matches `fmt --json`, which has always returned the same exit code as `--check`; only `check --json` was the exception.

Test: `tests/mcp_test.rs` (`json_check_reports_a_total_parse_failure_as_json`)

**Whole-package JSON** (#2253): without FILE, `almide check --json` judges all entries under `src/` in the same order as plain `almide check`, emitting one diagnostic per line. Each row's `file` identifies its entry, so there is no other multi-file form (no per-entry `: ok` lines either—only diagnostics). Exit code aggregates the single-file form: `1` if any entry is not clean. Judging does not short-circuit: stopping at the first failing entry would erase all later diagnostics, so every entry is judged before aggregation. `--effects` remains single-entry resolution, as its report is per function.

Tests: `tests/check_package_entries_test.rs` (`json_judges_every_entry_and_each_row_names_its_file`, `json_exit_code_is_one_when_an_entry_does_not_parse`), `tests/check_json_exit_code_test.rs` (exit-code parity across both modes and equivalence of “red exit code ⇔ an error line emitted”)

Error codes:

| Code | Description |
|---|---|
| E001 | Type mismatch |
| E002 | Undefined function |
| E003 | Undefined variable |
| E004 | Wrong argument count |
| E005 | Wrong argument type |
| E006 | Calling an effect function from a pure function |
| E007 | Fan block in a pure function |
| E008 | var capture in fan |
| E009 | Assignment to let/parameter |
| E010 | Non-exhaustive match |

See [../diagnostics/](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/diagnostics) for diagnostic-code explanations (the table above contains representative examples).

---

<a id="almide-fmt"></a>

### `almide fmt`

Format source files.

The line-length limit is **100 characters** (#2161). Call argument lists and record literals use one member per line (with trailing commas and the closing delimiter at the original indent) if either:

- The author already split the members across source lines
- The single-line form would exceed 100 characters from its starting position

Otherwise they stay on one line. Single-member containers are not stacked. Both layouts are fixed points (formatting the formatted result again does not change it).

```bash
almide fmt                              # src/**/*.almd を整形
almide fmt app.almd                     # 指定ファイルを整形
almide fmt --check                      # 差分があれば非ゼロで終了（CI 用）
almide fmt --check --json               # 同じゲートを JSON 1 オブジェクトで（機械可読）
almide fmt --dry-run                    # 書き込みせず差分表示
almide fmt --no-import-edit stdlib/     # import 行を一切触らず整形(splice-context ソース用)
```

| Option | Description |
|---|---|
| `--check` | Compare only. List unformatted files on stderr; exit 1 if any exist |
| `--json` | Machine-readable `--check` (implies `--check`). One object on stdout, same exit code |
| `--dry-run` | Only print formatted output to stdout; do not write |
| `--no-import-edit` | Do not edit import lines at all |

`--json` output:

```json
{"checked":2,"unformatted":["src/a.almd"],"unreadable":[],"verify_failed":false,"ok":false}
```

Neither `--json` nor `--check` writes anything. Apply formatting with `almide fmt <path>`.

Test: `tests/mcp_test.rs` (`fmt_check_json_reports_drift_and_keeps_the_gate_exit_code`)

<a id="コメントの付け先"></a>

#### Comment attachment

After formatting, fmt reparses and verifies AST identity and comment count (E054, #1309). If any comment has no attachment location, it refuses without touching the file. There are four locations (`ExprComments`, #1404 / #1714 / #1326):

| Written position | Attachment | Re-emission |
|---|---|---|
| `f(/* c */ a)`—before a node on the same line | Following node's leading | `/* c */ a` (moves with the node) |
| `f(a /* c */, b)` / `1 /* c */ + 2`—after a node, before a separator, closing delimiter or operator | Previous node's trailing | `a /* c */` (does not cross the comma) |
| `1 + // c` ↵ `2` / `xs // c` ↵ `\|> f` / `x // c` ↵ `.m()`—line end, expression continues on the next line | line_trailing of the **operand ending the line** | `1 // c` ↵ `  + 2`—break immediately after the operand; operator / `\|>` / `.` leads the continuation line (only `...` stays at line end) |
| `xs` ↵ `  // c` ↵ `  \|> f`—standalone line between continuations | Previous operand's line_between | Remains a standalone line at continuation indentation |

A line-ending `//` produces the same output whether written before or after the operator (canonical form is operator-leading). `//` on a line ending with a keyword or delimiter (`then // c`, trailing `.`, `match { // c`) has no attachment and remains rejected.

---

<a id="almide-compile"></a>

### `almide compile`

Generate a Module Interface: a JSON / `.almdi` artifact for external tools such as binding generators to read type information.

```bash
almide compile                          # プロジェクト全体
almide compile parser                   # モジュール名指定
almide compile app.almd --json          # JSON 出力（stdout）
almide compile --dry-run                # 人間向け表示
almide compile -o target/compile        # 出力ディレクトリ指定
```

JSON output structure:

```json
{
  "module": "mathlib",
  "types": [{
    "name": "Point",
    "kind": { "kind": "record", "fields": [{"name": "x", "type": {"kind": "float"}}] },
    "abi": { "size": 16, "align": 8, "fields": [{"name": "x", "offset": 0, "size": 8}] }
  }],
  "functions": [{
    "name": "distance",
    "params": [{"name": "a", "type": {"kind": "named", "name": "Point"}}],
    "return": {"kind": "float"},
    "effect": false
  }],
  "constants": [],
  "dependencies": []
}
```

The `abi` field is added only to concrete (non-generic) types. It describes C ABI layout (size, align, field offset).

---

<a id="almide-mcp"></a>

### `almide mcp`

Start a Model Context Protocol server over stdio, giving agents (such as Claude Code) access to the compiler through **typed tool calls**. The aim is to avoid an intermediate step in which the model interprets human-readable output—the very step where accuracy leaks.

```bash
almide mcp                              # stdio で JSON-RPC 2.0（改行区切り）を待つ
```

Supported methods: `initialize` / `tools/list` / `tools/call` / `ping`. Others return JSON-RPC `-32601` (`capabilities` declares only `tools`).

Tools (six; all use existing CLI machine-readable output):

| Tool | Implementation | Returns |
|---|---|---|
| `almide_check` | `almide check --json` | Array of structured diagnostics, including `try`/`try_replace` |
| `almide_test` | `almide test --json` | Per-file pass/fail plus raw runner output |
| `almide_api` | `almide ide outline --json` / `stdlib-snapshot --json` | Public-declaration signatures |
| `almide_explain` | `almide explain <CODE>` | Diagnostic-code explanation (markdown) |
| `almide_fmt_check` | `almide fmt --check --json` | Unformatted-file list, without writing |
| `almide_survive` | `almide survive --with - --json` | Survival delta before writing an edit (`almide survive` below) |

Design constraints:

- **One entrance to the compiler.** Every tool launches `current_exe()` (itself) as a subprocess and reads JSON already emitted by the CLI. No MCP-specific output route (such a route would inevitably drift from CLI; nobody reading it visually would notice).
- **Never parse human-readable text.** Where no machine-readable form exists (individual test-failure details = #1313), return it unchanged in a field named `*_unstructured`.
- **No writing tools.** `fmt` is check-only (`--check`). `almide_survive` judges but does not write. Apply through CLI (`almide apply --if-survives` / `almide fmt` / `almide fix`), leaving edits in the agent's own transcript.

Claude Code plugin definition (MCP + LSP): `tools/claude-plugin/`; marketplace: `.claude-plugin/marketplace.json`; installation: [../mcp.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/mcp.md)

Test: `tests/mcp_test.rs`

---

<a id="almide-survive"></a>

### `almide survive`

Judge a proposed edit **before writing it** (#2147). Apply it in memory, run check, tests reaching the edit, and contract fixtures before and after, and return `{unchanged, newly_broken, newly_fixed, removed}` per diagnostic / test / contract. Never write files.

```bash
almide survive src/calc.almd --with fix.patch --json   # unified diff
almide survive src/calc.almd --with new.txt  --json    # ファイル全文
git diff src/calc.almd | almide survive src/calc.almd --with - --json
```

- `--with <PATH|->`: the edit, either unified diff (starting with `--- ` / `@@ `) or the new full file text. Force interpretation with `--as auto|patch|text` (default `auto`). A diff hunk applies only if context and removed lines match exactly (fuzzy application would judge a different edit, so it is refused with exit 2).
- **Reachable set**: the edited file itself and `.almd` files below the project root (`.` if `./almide.toml` exists, otherwise the file's directory) whose import closure reads it. Check covers all reached files; tests cover reached files with `test` blocks; contracts cover reached files with `// @contract: C-NNN` (byte equality of stdout and exit code between native and `--target wasm`).
- **In-memory application**: each leg is a child `almide` process; only the after-edit side receives hidden arguments `--survive-overlay <PATH> --survive-overlay-text <FILE>` (moved from environment variables in #4129, because an environment not controlled by the developer could otherwise make any build compile text different from disk). All compiler `.almd` reads go through `almide::source_overlay::read_to_string`; only reads of the target file return proposed text. Proposed text lives outside the tree, in the system temporary directory.
- `--timeout <秒>` (seconds, default 600): limit per child process. Exceeding it kills the process group; tests report `timeout`, contracts report `timeout`.
- Exit: `0` = survives, `1` = does not survive, `2` = cannot judge (unreadable / hunk mismatch). With `--json`, `2` also emits `{"schema_version":1,"error":"…"}` on stdout.

**Survives** means no error diagnostic, test or contract is `newly_broken`, and no test or contract is `removed` (#4267). Deleting tests creates nothing `newly_broken`, so deletion is not survival. Use `--allow-removed` (on both `survive` and `apply`) for intended removal. MCP's `almide_survive` never passes it. A test retaining its name but losing all value-reading assertions (`assert` / `assert_eq` / `assert_ne` / `testing.assert_*` with any nonliteral argument)—such as `assert(true)` or an empty body—becomes `newly_broken` (`after: "hollowed"`).

Warnings marked `newly_broken` are reported but do not affect the verdict (`almide check` itself does not fail on warnings; unused-variable lint runs only on error-free files, so fixing the last error reveals pre-existing warnings—rejecting these would disproportionately reject repairs).

**Diagnostic identity** (edits shift lines; this is crucial): a line diff (LCS) of before/after edited text maps unchanged lines from old to new line numbers. Edited lines map nowhere. Other files use the identity map.

1. `position`: same check target, same `file`, same level / code / message; the mapped before-position matches the after-position in both line and column.
2. `position_renumbered`: as 1, but messages match after removing digit sequences (messages citing line numbers such as “first defined at line 12” shift under the same insertion).
3. `content`: if either side lies on an edited line, same target, file, level / code / message means identical (an error remaining after rewriting its line was not fixed).

Unpaired before-diagnostics are `newly_fixed`; unpaired after-diagnostics are `newly_broken`. Since a diagnostic itself is a bad state, check's `removed` is always empty.

**Test/contract classification** (identity: file + test name / fixture + contract ID):

| Before | After | Classification |
|---|---|---|
| Present | Absent | `removed` |
| Absent or bad | Good | `newly_fixed` |
| Absent or good | Bad | `newly_broken` |
| Good | Good | `unchanged` |
| Bad | Bad | `unchanged` |

Test states: `pass` / `fail` / `ignored` / `not_run` (binary did not run that far) / `compile_error` / `timeout` (good = `pass` and `ignored`). Contract states: `holds` / `diverges` / `timeout` (good = `holds`).

**JSON (`schema_version: 1`)**:

```json
{
  "schema_version": 1,
  "file": "calc.almd",
  "edit": { "kind": "patch", "lines_before": 3, "lines_after": 3 },
  "written": false,
  "survives": false,
  "reach": { "root": ".", "checked": ["calc.almd", "calc_test.almd"], "tests": ["calc_test.almd"], "contracts": [] },
  "summary": { "check": {"unchanged":0,"newly_broken":0,"newly_fixed":0,"removed":0}, "tests": {"…": 0}, "contracts": {"…": 0} },
  "check": {
    "unchanged":    [ { "checked": "…", "matched_by": "position", "before": {…}, "after": {…} } ],
    "newly_broken": [ { "checked": "…", "before": null, "after": {…} } ],
    "newly_fixed":  [ { "checked": "…", "before": {…}, "after": null } ],
    "removed": []
  },
  "tests":     { "newly_broken": [ { "file": "calc_test.almd", "name": "add adds", "before": "pass", "after": "fail", "failure": {…} } ], "…": [] },
  "contracts": { "unchanged": [ { "fixture": "…", "contract": "C-NNN", "before": "holds", "after": "holds", "detail": {…} } ], "…": [] }
}
```

check's `before` / `after` carry diagnostic objects from `almide check --json` **unchanged** (new fields in `almide check --json` arrive without changes). `failure` has the same shape as failure records in `almide test --json`. A check execution failing without JSON is retained with stderr in `check.unstructured`, never silently treated as clean. Increment `schema_version` when changing the shape (names, types or classification rules).

LSP's custom request `almide/survive` (params `{textDocument: {uri}, text?, patch?}`; if neither is supplied, treat the open buffer as the proposal) and MCP's `almide_survive` return the same JSON. Both run the CLI in a child process.

Tests: `tests/survive_test.rs` (four golden deltas in `tests/survive/`: a test-breaking patch; a full-text edit with one error persisting across line insertion and another fixed; a neutral annotation-only patch; a patch carrying E031 with `repair` (#2149) unchanged into `newly_broken`; plus non-writing and apply rejection/acceptance), `tests/mcp_test.rs`, `tests/lsp_test.rs`

---

<a id="almide-apply"></a>

### `almide apply`

Combine `survive`'s verdict and writing into one operation (verify-then-write).

```bash
almide apply src/calc.almd --with fix.patch --if-survives        # 生き残るときだけ書く
almide apply src/calc.almd --with fix.patch --force               # 判定に関わらず書く（判定は報告する）
```

- `--if-survives[=true|false]`: values are only `true` / `false`. Anything else (`maybe`, `1`, etc.) exits 2 without evaluating or writing (fail closed). `--if-survives=false` has meaning only combined with `--force`.
- `apply` with neither `--if-survives` nor `--force` exits 2 (writing requires an explicit gate).
- `--force` is a valueless flag (`--force=yes` is a usage error).
- Writing is atomic (write and fsync a temporary file in the same directory, copy original permissions, rename). If the file changed while being judged, do not write; exit 2.
- Exit: `0` = written, `1` = not written because it did not survive, `2` = refused before judgment.
- `--json` adds `written` and `forced` to `survive`'s JSON.

Test: `tests/survive_test.rs`

---

<a id="almide-init"></a>

### `almide init`

Create a new project.

```bash
almide init
```

Generated files:

```
almide.toml               [package] name, version, edition
src/
  main.almd               effect fn main テンプレート
tests/                    テスト用ディレクトリ
CLAUDE.md                 AI 向けプロジェクト説明
```

`name` is generated automatically from the current directory name.

---

<a id="almide-update"></a>

### `almide update`

Advance locked git dependencies to the **current remote head** of their refs (#1131).

```bash
almide update almai      # 指定の依存だけ
almide update            # tag 固定でない全依存
```

- `almide.lock` is intentionally sticky (`fetch_all_deps` reuses pins for reproducibility; `almide add` on an existing dependency also writes back the same pin). This command is the only canonical way to advance them.
- **Do not move tag-pinned dependencies**, as that would change the manifest's requirement itself; report a skip.
- Rewrite only changed entries; other pins remain byte-identical.
- On advancement, discard that ref's cache directory so the next fetch reacquires it.
- Resolve with one `git ls-remote`, without cloning.
- Check owners against `[trust] owners` before network access (#4093).
- If a dependency's `git` in `almide.toml` differs from its lock (source change), discard the old pin and relock from the new source, even for a tag pin. Builds reject source changes and direct users to this command ([package-system.md §6.3](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/package-system.md)).

Output is `name <old12> -> <new12>` (first lock: `name -> <new12>`).

<a id="almide-add"></a>

### `almide add`

Add a dependency package. Order: resolution → owner-trust check → existence check (`git ls-remote`) → fetch → write `almide.toml` and `almide.lock` (#4093). Every rejection/failure exits 1 without changing either file.

```bash
almide add bindgen                      # github.com/almide/almide-bindgen(パッケージ一覧のスナップショットで解決)
almide add almide/almide-bindgen        # 同上(パッケージ名 bindgen)
almide add user/repo@v0.1.0 --trust github.com/user   # github.com/almide 以外の所有者は --trust が要る
almide add --git https://example.com/team/repo.git --tag v1.0 --trust example.com/team mylib
almide add almide-graphics/ceangal2@v0.1.0 --subdir ceangal --trust almide-graphics  # サブディレクトリのパッケージ
almide add user/repo --dry-run          # 解決・信頼判定・存在確認だけを表示し、何も書かない
```

| Flag | Meaning |
|---|---|
| `--trust <owner>` | Trust the source owner (`host/owner`, or just `owner` for github.com) and write it to `[trust] owners`. Error if it differs from the resolved owner |
| `--dry-run` | Show resolution, trust verdict and existence check, then exit. Neither fetch nor write (exit 1 if the addition would be refused) |

There is no `--yes` / `--force` or environment variable trusting everything. Only permission naming the owner passes ([package-system.md §6.3](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/package-system.md)). A nonexistent repository is reported as not found; if a similar Almide package exists, append an executable `did you mean \`almide add <x>\`` (suggest only; do not substitute).

Shorthand:
- `almide add name` → the `name` package on github.com/almide (if package and repository names differ, look up embedded snapshot `src/almide_packages.txt`; if no entry is found, use `https://github.com/almide/{name}`)
- `almide add user/repo` → `https://github.com/{user}/{repo}`
- `@v0.1.0` → `tag = "v0.1.0"`
- `--subdir <dir>` → `subdir = "<dir>"` (#3381, [package-system.md §6.1](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/package-system.md)). Package name is the last subdir component (the first argument when using `--git`).

Write `almide.toml` and `almide.lock` only after fetching and checks pass (subdir exists, package name in `almide.toml` matches dependency name, transitive dependency owners). Restore both on failure.

---

<a id="almide-trust"></a>

### `almide trust`

Add an owner to `[trust] owners` in `almide.toml` (#4093). This is the step suggested by a rejection message for dependencies not added through `almide add` (transitive dependencies, handwritten manifest entries).

```bash
almide trust github.com/bob     # [trust] owners = ["github.com/bob"]
almide trust bob                # github.com/bob と同じ
```

---

<a id="almide-install"></a>

### `almide install`

```bash
almide install github.com/almide/<repo>          # 既定のブランチの最新コミット
almide install <spec> --tag v1.2.0               # タグ
almide install ./path/to/project                 # ローカルのディレクトリ
almide install <spec> --name tool --bin-dir ~/bin
```

Fetch a package, build it with `--release` as the root project, and place it in the bin directory (precedence: `--bin-dir`, `ALMIDE_INSTALL`, default `~/.local/bin`).

- **Trust** (#4184): the source owner (`host/owner`) must be the default-trusted `github.com/almide` or explicitly named with `--trust <owner>`. Refuse all others **before fetching**, the same rule as `almide add`. Local directories require no trust grant.
- **Native code** (#4184): the package's own native code (Rust under `native/`, `[native-deps]` crates and their build scripts, `@extern(rust|c)`) runs at build time with the user's privileges. Since `[permissions] native` considers dependencies only, install lists this code and refuses without `--allow-native`. Pure Almide packages install without the flag.
- **Destination** (#4183): the name (`[package] name` or `--name`) must be one plain identifier (letters, digits, `_`, not starting with a digit). Create output under a temporary name in the bin directory and rename it into place, never writing through a destination symlink. Existing files not installed by almide (no record in bin directory's `.almide-installed`) and symlinks are not replaced without `--force`.

Tests: `tests/install_destination_test.rs`, `tests/install_native_gate_test.rs`

<a id="almide-deps"></a>

### `almide deps`

List dependency packages. Fetch dependencies and show, per package (direct dependencies followed by those reached transitively), version, capability set (categories checked by `[permissions].allow`; an `@extern` fn has every category), native code brought in (`native/`, `[native-deps]`, `@extern(rust|c)` functions) and whether it is granted by `[permissions].native` (#4093). `build` / `run` / `test` / `bench` refuse dependencies with ungranted native code (`docs/specs/package-system.md` §6.4).

```bash
almide deps
# bindgen = https://github.com/almide/almide-bindgen (v0.1.0)
#   version: 0.1.0
#   capabilities: none
#   native code: none
# nat = path ../nat
#   version: 0.1.0
#   capabilities: Abort, Env.read, Env.write, FS.read, FS.write, Fan, IO.stderr, IO.stdin, IO.stdout, Net.fetch, Net.listen, Proc, Rand, Time
#   native code: native/natval.rs; @extern fn `number` (src/mod.almd:2) (granted by [permissions].native)
# ceangal = https://github.com/almide-graphics/ceangal2 (v0.1.0) subdir ceangal
#   ...
```

---

<a id="almide-dep-path"></a>

### `almide dep-path`

Print a dependency package's local cache directory.

```bash
almide dep-path bindgen
# ~/.almide/cache/.sources/<encoded-source>/@/<commit>/src
almide dep-path ceangal     # subdir 依存: 共有クローン内のパッケージディレクトリ
# ~/.almide/cache/.sources/<encoded-source>/@/<commit>/ceangal/src
```

Use: obtain a path when executing a dependency's `.almd` file through `process.exec("almide", ["run", path])`.

---

<a id="almide-clean"></a>

### `almide clean`

Clear caches. Six targets:

| Target | Location |
|---|---|
| Dependency cache | `~/.almide/cache/` |
| Incremental cache | `./.almide/cache/` |
| Compilation cache | `./target/compile/` |
| Native build scratch (#2500) | `ALMIDE_RUN_PROJECT_DIR` (default `<cache base>/run`) and `<cache base>/build-cdylib` |
| `almide test` worker directories (#2504) | Each `<cache base>/test/native/<key>/` |
| Runtime rlibs (#2504) | Each `<cache base>/almide-rtlib-<key>/` |

**“Emptying” build scratch is the completed behavior, not unfinished cleanup.** Contents are deleted only after acquiring each directory's `.almide-build.lock` (ongoing builds finish before removal), leaving an empty directory with only its lockfile. Keeping the lockfile is deliberate: deleting it lets “a builder waiting with the old lockfile open” and “the next builder creating a new lockfile” **lock different inodes and break mutual exclusion**. Two builds could then run in the same directory simultaneously, turning one binary into the other's (#1877's failure mode). What disappears is disk usage (several MB per worker); what remains is one zero-byte file.

Emit one `Cleaned <path>` stderr line per emptied directory (worker and rlib directories each aggregate to one line: `Cleaned <native> (N test worker dir(s))` / `Cleaned <temp>/almide-rtlib-* (N runtime rlib dir(s))`). If nothing exists, print `No cache to clean`.

```bash
almide clean
```

Test: `tests/run_cache_recovery_test.rs`

---

<a id="almide-verify"></a>

### `almide verify`

Have the certificates (ownership / call-modes / bounds / names / caps / caps-transitive witnesses) of the wasm module shipped by `almide build --target wasm` rechecked by **the separately versioned independent binary `almide-verify`** (#2152, #4011). `almide verify` itself is a subprocess shim with no checker: it searches for `almide-verify` **beside the almide executable, then on PATH**, launches it, and forwards standard I/O and exit code unchanged. If missing, there is **no linked fallback**; it exits 127 with named error `error[verifier-missing]`.

```bash
almide build app.almd --target wasm               # app.wasm を書く
almide verify app.almd                            # 証明書 bundle を生成し、app.wasm の SHA-256 に結びつけて almide-verify bundle に渡す
almide verify app.almd --artifact out/app.wasm    # 結びつける成果物を明示する（無ければ・一致しなければエラー）
almide verify app.almd --emit app.bundle          # bundle をファイルに残す（almide-verify が無くても書く）
almide verify --mir app.almd --emit m.bundle      # MIR lowering の witness（coverage.sh 用、ビルドとは無関係）
almide verify ownership w.cert                    # .almd 以外の引数は almide-verify にそのまま渡る
almide-verify --version                           # 検査器自身の版数（コンパイラとは独立）
```

- Only the **untrusted producer** runs on the `almide` side. It builds through the same route as `almide build --target wasm` (structural leg) and writes witnesses recorded during emission into a bundle (`crates/almide-wasm/src/witness_bundle.rs`, the same producer of bundles judged by CI's `proofs/structural-wall.sh`). The verdict always belongs to `almide-verify`.
- **Artifact binding**: if a file named by `--artifact`, or the default `almide build` output (`app.wasm` for `app.almd`), exists, check that its bytes match the module this build produces and write its SHA-256 into the bundle (version 2's `artifact` row). `almide-verify` recomputes the digest from the file and REJECTs a mismatch. The producer rejects a mismatching (stale) file with exit code 1. If no file exists, produce a version 1 bundle (a verdict about the module produced by the build) and emit a note to that effect.
- Programs shipped as components, such as with `http.serve`, are excluded (exit code 1 with a reason). The target is the core module written by plain `almide build --target wasm`.
- `almide-verify` links no compiler crate (`crates/almide-verify`, zero dependencies). Each property's verdict transcribes a Coq checker in `proofs/` (`check_xc` / `check_names_cert` / `check_caps_cert` / `check_prog_cert` / `check_modes_cert` / `check_bounds_cert`); the theorems are checked by the Rocq kernel.
- Frames for which a witness could not be recorded appear by name as `uncertified` in the bundle, never silently skipped.
- If `./almide.toml` has `[permissions].allow`, declared capabilities of `effect fn` are restricted to it (allowing caps witnesses to reject). A plain `fn`'s upper bound is console output (ADR-0022); the manifest cannot expand it.
- `almide verify --mir app.almd [--emit b]` is the pre-#4011 behavior: write MIR-lowering witnesses (per-function ownership / names / caps and call-modes). Since #2761, no shipped wasm bytes come from MIR, so this judges MIR, not a build. It remains because `proofs/coverage.sh` drives the MIR certificate producer through this route and its coverage floor is a ratchet (cannot combine with `--artifact`). The gates judging MIR witnesses are `proofs/gate.sh` / `proofs/corpus-wall.sh`.

`almide-verify` exit codes: 0 = every witness ACCEPT (CERTIFIED); 1 = any REJECT, or zero witnesses; 3 = every witness ACCEPT but some functions uncertified (INCOMPLETE); 2 = misuse, unreadable input or malformed bundle.

Tests: `tests/verify_shim_test.rs` (shim delegation, named missing-verifier error, exit-code forwarding, artifact binding and stale-file rejection, REJECT for capabilities exceeding the manifest), `crates/almide-verify/tests/coq_examples.rs` (all Coq `Example` entries and fixed lines of `build-checker.sh`), `crates/almide-verify/tests/cli.rs`

---

<a id="legacy-mode"></a>

## Legacy Mode

If the first argument is a filename ending in `.almd`, treat it as the `emit` command:

```bash
almide app.almd --target rust           # → almide emit app.almd --target rust
almide app.almd --target rust --repr-c  # #[repr(C)] 付き Rust 出力
almide app.almd --emit-ast              # AST を JSON で出力
almide app.almd --emit-ir               # 型付き IR を JSON で出力
```

---

<a id="exit-codes"></a>

## Exit Codes

| Code | Meaning |
|---|---|
| 0 | Success |
| 1 | Compilation error, test failure, dependency-resolution failure, fmt --check differences, etc. |
| 2 | Command-specific usage, input or indeterminate-verdict error (see each section) |
| 3 | almide verify: witnesses ACCEPT, but uncertified functions remain (INCOMPLETE) |
| 5 | almide test: zero executable tests (changeable with --allow-no-tests) |
| 127 | `almide verify`: independent checker `almide-verify` not found (`error[verifier-missing]`) |

---

<a id="global-options"></a>

## Global Options

| Option | Description |
|---|---|
| `-h, --help` | Show help (also available per command) |
| `-V, --version` | Show version (for example `almide 0.34.2`) |

---

<a id="環境変数"></a>

## Environment variables

The sole registry for `ALMIDE_*` environment variables is `almide_base::env::SWITCHES` (`crates/almide-base/src/env.rs`),
and `almide switches` lists them (#2205). The table below is the output of `almide switches --md` itself.
`scripts/check-env-switches.sh` (CI `checks`) rejects names absent from the registry and direct compiler-side
`std::env::var("ALMIDE_…")` reads; `tools/almide-gates env-switches-doc` rejects differences between this table and the registry.

- Boolean values have one meaning: unset, empty, `0`, `false`, `off`, and `no` mean OFF; everything else means ON.
- When a switch of kind `route` (forces the execution leg) or `gate` (bypasses a default check) is ON,
  it prints one line to stderr (`[almide] ALMIDE_X is set: …`) when first read in that process.
  A verdict obtained on a forced route identifies itself as such.
- **Switch lock** (#4129): kinds `route` / `gate` / `ablation` / `trap` / `instrument` change what gets compiled or which checks run.
  Release builds (those whose `almide --version` identifies them as `(release…)`) ignore these switches inherited from the environment
  and print one stderr line, `[almide] ALMIDE_X is set but IGNORED: …`. To enable one, explicitly name it on the command line
  with `--allow-switch ALMIDE_X` (repeatable). This prevents environments outside the developer's control—CI, `.envrc`,
  devcontainers, editor tasks, and so on—from making the compiler compile sources other than those reviewed or skip checks.
  Switches set by the tool itself (`ALMIDE_NO_RTLIB` for `almide test`, `ALMIDE_FUEL_PROBE` for `--time-report`) are exempt from the lock.
  Development builds continue reading the environment as before; the hidden `--lock-switches` flag applies the same lock as a release build.
  What the lock suppresses depends on each switch's declared kind, so those declarations are checked:
  enabling any switch of a kind outside the lock (`debug` / `tool` / `runtime` / `harness` / `ci`) must not change the output
  of the generated Rust, wasm, or native program (`tests/switch_scope_test.rs`).
  The default engagement of the lock in release builds is checked by the `almide_base::env::lock_engaged_by_default` unit test
  and by `scripts/check-release-switch-lock.sh`, which the release workflow runs on the artifact itself.
- `--profile critical` rejects even one such switch passed as ON through the environment, regardless of `--allow-switch` (exit 1).
- `instrument` adds instrumentation (allocation counters, fixed or poisoning allocators) to the artifact:
  the program's computation is the same, but the artifact differs from the shipped one, so it is locked.
- `harness` denotes hooks read by test harnesses (`tests/`, `crates/*/tests`, `tools/`); `ci` denotes switches used only by workflows
  and `scripts/`; `runtime` denotes those read at runtime by the compiled program itself.

> Fixed source-snapshot table; this reader copy is not generated or automatically synchronized. The statements above describe the canonical source and its checks, not checks of this translated table.

| Variable | Kind | Description |
|---|---|---|
| `ALMIDE_ABORT_SITE=value` | runtime | the testing.assert_aborts site a native test child runs (ADR-0029): set by the parent when it re-executes the test binary; never set by hand |
| `ALMIDE_ALLOC_COUNT` | instrument | build the native program with a counting allocator that prints `__ALMD_ALLOC allocs=N deallocs=N reallocs=N peak=N` on stderr when `__almide_main` returns (the native borrow oracle's allocation lane, tests/native_borrow_oracle_test.rs) |
| `ALMIDE_BANG_RETURN` | ablation | turn OFF the per-position `!` desugars of the v1 lowering so every `!` reaches the bind-position rule or walls loudly (the decline-matrix probe) |
| `ALMIDE_BASELINE_BIN=value` | harness | a compiler from before the sockets route: tests/net_wasm_default_route_test.rs byte-compares a socket-free program's preview 1 module against it |
| `ALMIDE_BENCH_DIR=value` | harness | the fixture directory of the structural leg's perf probe (default `crates/almide-wasm/tests/perf`) |
| `ALMIDE_BENCH_MAIN_NS=value` | harness | the file `almide bench` points a benched native binary at: the spliced `main` timer writes its elapsed nanoseconds there (#2980, the main-only boundary) |
| `ALMIDE_BIN=value` | harness | path of the `almide` binary the test harnesses, scripts and workflows drive (default: `target/release/almide`, then PATH) |
| `ALMIDE_BORROW_OWN_ALL` | ablation | make BorrowInsertion own every borrow-eligible param, as before inference existed — the ablation the ownership certifier's C4 sensitivity test drives, and the borrow-inference perf knob |
| `ALMIDE_BOUNDED_DEBUG` | debug | print why a bounded-loop bind declined (v1 lowering) |
| `ALMIDE_BUILD_PROVENANCE=value` | ci | read by `build.rs` at BUILD time: `release` makes `almide --version` say `(release)`, anything else (including unset) says `(dev)`. Set only by `.github/workflows/release.yml`, the one thing that builds from a tag, so a binary claiming to be a release had to come from there (#2384) |
| `ALMIDE_BUILD_SHA=value` | ci | read by `build.rs` at BUILD time: the commit `almide --version` names beside the build kind, truncated to 9 characters. Passed in by `make install` and the release workflow rather than read from git in the build script, which would rebuild the root crate after every commit (#2384) |
| `ALMIDE_CACHE_MAX_BYTES=value` | tool | the size bound of each native build cache under the private per-user cache base — the `almide test` worker cache (`<cache base>/test/native`, least recently used worker dirs emptied first) and a build scratch dir's per-program binaries and objects (`<cache base>/run`): `4G` (default), `512M`, a byte count, or `0`/`off` for no bound (#2608) |
| `ALMIDE_CAPTURE_MOVE_OFF` | ablation | make CaptureClone clone every capture again, as before #2231, instead of moving a value whose sole user is the closure — the ablation the ownership certifier's sensitivity test drives |
| `ALMIDE_CERTIFY_OWNERSHIP=value` | debug | run the native ownership certifier after the pass pipeline (#2231): `report` prints every violation, `fail` aborts the build on one, `off` skips it; unset = `fail` in a debug build, `off` in a release build |
| `ALMIDE_CI_BASE=value` | harness | the commit a sliced CI run diffs HEAD against to find the change's own files (a PR's base sha, a merge group's base_sha): every fixture the change touches joins the `ALMIDE_CI_SLICE` slice; scripts/ci-slice-prepare.sh resolves it into `ALMIDE_CI_TOUCHED` |
| `ALMIDE_CI_SEED=value` | harness | the commit sha whose first 7 hex digits rotate the `ALMIDE_CI_SLICE` residue class, so consecutive commits judge different slices (CI passes `github.sha`); unset = offset 0 |
| `ALMIDE_CI_SLICE=value` | harness | `k` > 1 makes every corpus-iterating test and script judge a commit-rotated 1-in-k slice of its fixtures plus the ones the change touches, skip its whole-corpus staleness / completeness checks and never rewrite a ledger (crates/almide-corpus/src/ci_slice.rs, scripts/lib/ci-slice.sh); CI sets it on PRs and merge groups whose base is not main; unset or 1 = the full run, unchanged |
| `ALMIDE_CI_TOUCHED=value` | harness | a file listing the repo-relative paths the change touches, one per line (written by scripts/ci-slice-prepare.sh); read instead of running `git diff` against `ALMIDE_CI_BASE` |
| `ALMIDE_COMPILER_STACK=value` | tool | stack size in bytes of the compiler driver thread (default 256 MiB); a deep input that overflows it is the regression test's subject |
| `ALMIDE_COMPONENT_ADAPTER` | route | route `--component` through the preview1 adapter instead of the direct component emission |
| `ALMIDE_COMPONENT_P3` | route | emit a WASI 0.3 component (stdio over component-model streams, the async canonical ABI) under `--component` (a program whose op set names a tcp / udp / name-lookup op builds as this component without the switch, since preview 1 has no sockets); runs on a stock wasmtime 46+ with no feature flag (`wasmtime run app.wasm`, `-S http` for an http client — ADR-0023 step 1) |
| `ALMIDE_CONTROL_ADMIT_CODEC_AT_INPUT` | ablation | make an `@input_sink` parameter (`http.decode_json[T]`) admit a type that is also `Encode` (a `Codec` type) instead of E110: the negative control of the row-22 input-type gate (tests/input_sink_test.rs), whose over-post cell must flip from E110 to `is_admin=true`; never set otherwise |
| `ALMIDE_CONTROL_ADMIT_HAND_DECODE` | ablation | make the request-decode flow gate (E110's flow half, #4232) admit request data decoded by hand into a type that is also sent out (`User.decode(json.parse(http.body(req))!)` into a `Codec` type): the negative control of tests/input_sink_test.rs, whose hand-decode program must flip from E110 to `is_admin=true`; never set otherwise |
| `ALMIDE_CONTROL_BIN=value` | harness | an `almide` binary the security route tests (`tests/effect_value_laundering_test.rs`, `proc_category_test.rs`, `permissions_routes_test.rs`) run INSTEAD of the one under test — the negative control: a build without the check fails their assertions (#4151, #4155) |
| `ALMIDE_CONTROL_IGNORE_SKIP_DECODE` | ablation | make the Codec derive read a `@skip_decode` field from input again: the negative control of the #4108 field-skip matrix gate (tests/codec_field_skip_test.rs), which must flip its `admin=false` cell; never set otherwise |
| `ALMIDE_CONTROL_NO_TYPE_SHARING` | ablation | make every clone of a type copy its whole tree instead of sharing its element lists (ADR-0030 D5): the negative control of the type-sharing gate (tests/compile_budget_test.rs), whose doubling chain with the type-size budget lifted must then build exponentially many type lists; never set otherwise |
| `ALMIDE_CORPUS_FILTER=value` | harness | substring filter over the fixture paths the 3-way oracle test evaluates |
| `ALMIDE_CORPUS_SHARD=value` | harness | `k/N` (1-based) walks the k-th modulo slice of the SORTED spec/wasm_cross corpus in the six corpus giants (#2381), read after the sort; `merge/N` reads the N shards' partials from `ALMIDE_CORPUS_SHARD_DIR` and judges the whole-corpus ceilings and the name-keyed bridge ledger; unset = the unsharded gate |
| `ALMIDE_CORPUS_SHARD_DIR=value` | harness | directory where a sharded corpus gate writes its walked-fixture list and its partial counts / bridge names, and where `merge/N` reads them; unset = a local slice that writes nothing |
| `ALMIDE_CORPUS_WEIGHTS_DIR=value` | harness | directory where a corpus gate records the wall it measured per fixture (`weights/<column>.<gate>[.<k>-of-<N>].txt`, `stem<TAB>ms`) for scripts/gen-corpus-weights.sh to render into proofs/corpus-weights.txt, the table the balanced `k/N` slices read (#2457); CI points it at the shard-partials dir so the committed table is rendered from the runner's own ratios (#2502); unset = nothing recorded |
| `ALMIDE_COVERAGE_CONDITION=value` | ci | the coverage ratchet's condition tag (which baseline row a push is judged against) |
| `ALMIDE_COVERAGE_NATIVE_LEG=value` | ci | `1` makes proofs/coverage.sh also run `almide test spec/ --target rust`, the native codegen workload (#3195); defaults to ALMIDE_COVERAGE_CONDITION |
| `ALMIDE_CWD=value` | runtime | the writer's working directory, set by `almide run` for the wasm host so relative fs paths resolve as on native (C-137); never set by hand |
| `ALMIDE_DBG_ANF` | debug | print why a lambda lift or statement inline declined (v1 lowering) |
| `ALMIDE_DBG_BANG` | debug | print the `!` unwrap decisions of the v1 bind lowering |
| `ALMIDE_DBG_BORROW=value` | debug | dump every native borrow-inference signature key containing the value, per fixed-point iteration, and the keys each call site consults |
| `ALMIDE_DBG_CELLS` | debug | print the captured / mutated / celled variable sets (v1 lowering) |
| `ALMIDE_DBG_CONTLIFT` | debug | print why a poison-oracle continuation lift rolled back (v1 lowering) |
| `ALMIDE_DBG_DESUGAR_FN=value` | debug | print the fully desugared body of the fn named by the value (v1 lowering; was `DBG_DESUGAR_FN` before #2205) |
| `ALMIDE_DBG_DESUGAR_RAW` | debug | with `ALMIDE_DBG_DESUGAR_FN`, print the raw pre-desugar body too (was `DBG_DESUGAR_RAW`) |
| `ALMIDE_DBG_ELEM` | debug | print why a list-literal Block element declined (v1 lowering) |
| `ALMIDE_DBG_FAN` | debug | print the fan lowering's prefetch, pattern and instance-parallel decisions (structural leg and embedded host) |
| `ALMIDE_DBG_GINIT` | debug | print the eager top-let init runner's admission set and why an extended runner declined (v1 lowering, C-007) |
| `ALMIDE_DBG_LINK` | debug | dump the wasm link demand set and what each key resolves to |
| `ALMIDE_DBG_LOWER_FN=value` | debug | print the fully desugared body the v1 lowering actually lowers, for the fn named by the value (was `DBG_LOWER_FN`) |
| `ALMIDE_DBG_NESTED_MATCH` | debug | print why a nested-match chain was refused (v1 lowering) |
| `ALMIDE_DBG_QQ` | debug | print which path lowered each `??` (match-first vs route fallback, v1 lowering) |
| `ALMIDE_DBG_ROUTER` | debug | print a stdlib call name refused for its registered signature, with the mismatch and the argument types (v1 lowering) |
| `ALMIDE_DBG_TCO` | debug | print the tail-call-to-loop admission decisions (v1 lowering) |
| `ALMIDE_DBG_TRAP` | debug | print the wasm trap's backtrace and host state when the embedded host catches one |
| `ALMIDE_DBG_WHILE` | debug | print the while-loop lowering decisions (v1 lowering) |
| `ALMIDE_DEBUG_CALL_OPS` | harness | print the call ops of every lowered fn (the classify_corpus example) |
| `ALMIDE_DEBUG_EFFECTS` | debug | print the effect inference pass's per-function results (native codegen) |
| `ALMIDE_DEBUG_MIR_OPS` | harness | print every MIR op with its index (the classify_corpus example — the certificate-bisection instrument) |
| `ALMIDE_DEFAULTS_DEBUG` | debug | print the record-default resolution when no default keys were found (native codegen) |
| `ALMIDE_DEPS_CACHE_ONLY` | tool | resolve dependencies only from what is already on disk: a dependency not in the cache is an error, nothing is cloned and `almide.lock` is never written. Set by `almide mcp` on every tool child (#4179) |
| `ALMIDE_DISABLE_OPT` | ablation | run the optimiser pipeline with every optional pass off (ablation; the always-on enabler passes still run) |
| `ALMIDE_DUMP_IR=value` | debug | dump the IR after the named passes (comma-separated, or `all`) on the native pipeline, and the post-chain body of every fn whose name contains the value on the v1 pipeline |
| `ALMIDE_DUMP_MIR` | debug | print every lowered fn's op stream before the native render runs |
| `ALMIDE_DUMP_VERIFY` | debug | print the native render's verification transcript |
| `ALMIDE_DUMP_WMIR=value` | debug | print the lowered wasm-leg op stream of every fn whose name contains the value |
| `ALMIDE_DWARF_FIXTURES=value` | harness | the DWARF line-table span-resolution measurement (`crates/almide-wasm/tests/dwarf_lines.rs`, ignored test): the `.almd` files to measure, comma-separated (#1315) |
| `ALMIDE_EXPECT_BASELINE` | harness | make tests/net_wasm_default_route_test.rs fail instead of skipping the byte comparison when ALMIDE_BASELINE_BIN is unset |
| `ALMIDE_EXPECT_TOOLS` | harness | make a harness test FAIL instead of skipping when an external tool (wasmtime, wasm-tools) is missing; CI sets it |
| `ALMIDE_F32_SWEEP_N=value` | harness | how many xorshift32 bit patterns the Float32 printer sweep prints (`${x}` and `float32.to_string`) and compares with Rust f32 Display on each leg (default 100000; tests/float32_to_string_cross_target_test.rs) |
| `ALMIDE_FAKE_RUSTC_VERSION=value` | harness | the version a fake `rustc` reports in tests/rustc_floor_test.rs, which forwards every other invocation to `ALMIDE_REAL_RUSTC` (the rustc-floor refusal's probe) |
| `ALMIDE_FALLBACK_NAMES` | tool | make `almide test` print one `FALLBACK <file>` line per file the wasm leg did not pass — the wasm coverage ratchet's data feed |
| `ALMIDE_FAN_COST_OFF` | ablation | go parallel on every `fan` offer the cost model (#3341) would keep sequential — the pre-#3341 behaviour, on both legs (native threads and the embedded wasm host's instances); the ablation the crossover measurement and the parallel-path tests use |
| `ALMIDE_FAN_PAR_OFF` | ablation | keep `fan` chunk maps off the structural leg's instance-parallel offer (#3003): every chunk runs sequentially in the run's own instance |
| `ALMIDE_FAN_SEQUENTIAL` | runtime | run `fan.*` sequentially in the native runtime, and build a `--host js` fan over async hooks with one suspension per element instead of the overlap protocol (#3383) — a determinism lever for measurement; the observable result is the same by contract |
| `ALMIDE_FAN_THREADS=value` | runtime | cap the worker count of `fan`'s parallel maps on both legs — native threads and the embedded wasm host's chunk instances (#3003; a measurement lever for thread-scaling curves, the observable result is the same by contract; default: available parallelism) |
| `ALMIDE_FLAGS_BIN=value` | tool | the compiler research/benchmark/perf/bench.py asks for the native `--release` rustflags (`almide build --print-release-rustflags`) when the measured one predates that flag (#3883); it must share the measured build's release flags; unset asks the measured compiler |
| `ALMIDE_FLOAT_SWEEP_N=value` | harness | how many xorshift64 bit patterns the float printer sweep prints and compares with Rust `format!` on each leg (default 100000; tests/float_to_string_cross_target_test.rs) |
| `ALMIDE_FMOD_SWEEP_N=value` | harness | how many xorshift64 bit-pattern pairs the float `%` sweep compares with Rust `%` on each leg, as Float and as Float32 (default 20000; tests/float_fmod_cross_target_test.rs) |
| `ALMIDE_FN_ESCAPE_OFF` | ablation | make BorrowInsertion borrow EVERY fn-typed param as `&dyn Fn`, escaping or not (#2288) — the ablation the ownership certifier's C5 sensitivity test drives |
| `ALMIDE_FORCE_METER_CODEGEN` | route | build a metered (fan.bounded / fan.min / fan.timeout) native program through the standard codegen with its own deterministic meter, even when the whole program renders on v1 or its regions splice — the differential that holds the codegen meter to the v1 render (#3673) |
| `ALMIDE_FORCE_METER_SPLICE` | route | build a metered (fan.bounded / fan.min / fan.timeout) native program through the standard codegen with the v1-rendered regions linked in, even when the whole program renders on v1 — the differential that holds the two routes' spends equal (#3669) |
| `ALMIDE_FUEL_PROBE` | route | insert fuel charges into the native render (the charge probe); `almide run --time-report` sets it internally. Native-only: the wasm leg carries no charge trace (#2752) |
| `ALMIDE_FUZZ_BASE=value` | harness | the first seed of the differential fuzz's fixed seed range (default 0) |
| `ALMIDE_FUZZ_HOST_ORACLE` | harness | run the differential fuzz in host-oracle mode (arm selection is deterministic per seed and mode) |
| `ALMIDE_FUZZ_ITERS=value` | harness | how many seeds the differential fuzz's fixed range covers (default 200) |
| `ALMIDE_GPU_REQUIRED` | harness | fail the GPU leg's parity tests when no wgpu adapter is found instead of printing SKIP (#4236; the `gpu-parity` CI job sets it over lavapipe; crates/almide-gpu-run/tests/int_map_parity.rs) |
| `ALMIDE_HEAP_TRACE` | debug | print the interpreter's heap-block allocations and frees |
| `ALMIDE_HTTP_MAX_RESPONSE_BYTES=value` | runtime | the http client's cap on a buffered response in bytes, read by the compiled program (default 1 GiB; 0 = no cap, #2825) |
| `ALMIDE_HTTP_TIMEOUT_SECS=value` | runtime | the http client's connect and read timeout in seconds, read by the compiled program (default 30; 0 = none) |
| `ALMIDE_INLINE_OFF` | ablation | turn the structural leg's small scalar-fn inliner off (#2980): every call kept, for an A/B |
| `ALMIDE_INSTALL=value` | tool | the directory `almide install` installs binaries into (overrides the default `~/.local/bin`) |
| `ALMIDE_INTERP_SWEEP_THREADS=value` | harness | interp sweep thread count; 1 = serial (#2381) |
| `ALMIDE_IR_FAULT=value` | route | inject an IR violation after the named optimiser pass, so the per-pass verifier can be watched turning red in the release binary (a release build honours it only with `--allow-switch ALMIDE_IR_FAULT`, #4129) |
| `ALMIDE_KEEP_SCRATCH` | tool | keep the `almide test` scratch build directory instead of deleting it |
| `ALMIDE_LSP_TRACE` | debug | print every LSP request and response the language server handles |
| `ALMIDE_MANIFEST_TREE_CHECK=value` | ci | the parity-manifest generators' stale-tree refusal (#2405, scripts/lib/oracle-header.sh): `strict` (default) refuses an ORACLE that is not `<Cargo.toml version> (dev…)`, is stamped with a commit other than HEAD, or is unstamped and older than the sources; a worktree behind its upstream; and an untracked spec/ fixture. `gate` keeps only the untracked-fixture check (scripts/check-parity-goldens.sh vouches for CI's artifact). `off` is the deliberate override |
| `ALMIDE_MONO_DEBUG` | debug | print the monomorphisation discovery and instantiation decisions |
| `ALMIDE_MP_PROBE` | debug | print the mut-param analysis decisions (IR) |
| `ALMIDE_MUTATION_BASE=value` | ci | the base ref the mutation gate diffs against |
| `ALMIDE_MUTATION_BUDGET_SECS=value` | ci | the mutation gate's time budget in seconds: past it no new mutant starts and the rest are named as deferred (0 / unset = none) |
| `ALMIDE_MUTATION_SCOPE=value` | ci | which mutation set the mutation gate runs |
| `ALMIDE_MUTATION_SHARD=value` | ci | this job's shard index of the mutation gate |
| `ALMIDE_MUTATION_SHARDS=value` | ci | how many shards the mutation gate is split into |
| `ALMIDE_NAMES_DEBUG` | debug | print the native name-verification map and its scoped shadowing decisions |
| `ALMIDE_NO_AVAIL_CHECK` | gate | bypass the E081 stdlib availability check (the measurement escape the availability probe builds through) |
| `ALMIDE_NO_RTLIB` | route | build the native runtime inline instead of linking the prebuilt runtime crate (the self-contained cargo path; `almide test` sets it for the harness build) |
| `ALMIDE_NO_VERIFIED_OK` | gate | re-enable the retired `--no-verified` legs (the v0 fallback) instead of refusing the flag |
| `ALMIDE_OMEGA=value` | route | the baked ω ordinal for deterministic wall-deadline replay: the artifact cuts at the n-th wall check without reading the clock (`-1` / unset = live) |
| `ALMIDE_OMEGA_RECORD` | route | make the native artifact print `__ALMD_OMEGA <ord>` at each region exit whose deadline fired (record on native, replay anywhere) |
| `ALMIDE_ONLY_PASS=value` | ablation | run the optimiser with ONLY the named optional pass (plus the always-on enablers); an unknown name is a hard error |
| `ALMIDE_ORACLE_KEEP` | harness | keep the programs the native borrow-mode oracle generates (tests/native_borrow_oracle_test.rs) instead of deleting them after the run |
| `ALMIDE_ORG_DIR=value` | ci | the checkout directory of the org repos the cross-repo verification scripts walk |
| `ALMIDE_P3_HTTP_STOP=value` | ablation | make the p3 http shim answer its static error right after stage N of the request build (1..=5), to localise a hang |
| `ALMIDE_PASS_EDGES=value` | harness | extra `A<B` pass-order edges (comma-separated) the shuffle honours — the bisection instrument that names the pair a shuffle divergence needs declared |
| `ALMIDE_PROBE_DUMP=value` | harness | the path the heap probe writes its emitted wasm to |
| `ALMIDE_PROBE_IR=value` | harness | the path the heap probe writes its lowered IR to |
| `ALMIDE_PROBE_OUT=value` | harness | the file a charge-probed native program writes its `__ALMD_PROBE` meter line to; `almide run --time-report` sets it to a fresh temp path so the program's own stderr is never read as the reading (#3489). Unset = the line goes to stderr |
| `ALMIDE_PROBE_SRC=value` | harness | the source file the heap probe compiles (unset = the probe is skipped) |
| `ALMIDE_PROFILE` | debug | print per-pass and per-phase timings of the native pipeline |
| `ALMIDE_REAL_RUSTC=value` | harness | the real `rustc` the fake one in tests/rustc_floor_test.rs runs for anything but `--version` |
| `ALMIDE_REGION_DEBUG` | debug | print the region-window pass's decisions (native and structural leg) |
| `ALMIDE_REGION_OFF` | ablation | turn the region-window allocation pass off (native and structural leg) |
| `ALMIDE_REGION_TRAP_STALE` | trap | arm the native region prelude's stale-reference trap (#2200) |
| `ALMIDE_REPO=value` | ci | the repository slug a release script targets |
| `ALMIDE_RUN_PROJECT_DIR=value` | tool | the scratch dir `almide run` / `almide build` compile native binaries in, instead of `<temp>/almide-run` (the content-keyed binary cache, its cargo target, its rustc incremental sessions); `almide clean` empties it |
| `ALMIDE_SCOPED_CLOSURE_OFF` | ablation | make BorrowInsertion treat every `let`-bound closure as a closure value, never as a non-escaping scope (#3455) — its captures are owned again, as before the scope rule |
| `ALMIDE_SEMLAW_CASES=value` | harness | how many cases the semantic-laws property test draws |
| `ALMIDE_SHUFFLE_PASSES=value` | gate | run the native passes in the seeded random order the declared dependency edges permit — a pass-dependency probe: the emitted Rust must not change (#2186) |
| `ALMIDE_SIZE_ALONE=value` | harness | the one fixture a child process of the size ratchet measures alone, for its isolation check (#2309); the ratchet sets it on the processes it spawns |
| `ALMIDE_SIZE_PRESSURE=value` | harness | the allocation pressure (`plain` / `quarantine` / `ballast`) a size-ratchet child builds its one fixture under, for the determinism check (#3143); the ratchet sets it on the processes it spawns |
| `ALMIDE_SKIP_PASS=value` | ablation | skip the named optional passes (comma-separated) — a pass-dependency probe: output must not change |
| `ALMIDE_SKIP_VERSION_CHECK` | gate | skip the project's `almide` version requirement check |
| `ALMIDE_STDLIB_COVERAGE_PROBE=value` | harness | the one program (its repo-relative path) tests/stdlib_test_coverage_test.rs measures, for probing a single fixture |
| `ALMIDE_STDLIB_COVERAGE_REPORT=value` | harness | where tests/stdlib_test_coverage_test.rs writes its per-function coverage table (scripts/check-stdlib-test-coverage.sh) |
| `ALMIDE_STREAM_FUSION_OFF` | ablation | turn the stream-fusion pass off |
| `ALMIDE_SURVIVE_OVERLAY=value` | harness | RETIRED (#4129): no longer read. `almide survive` hands its overlay to a child run as `--survive-overlay` / `--survive-overlay-text` arguments, so an environment cannot make the compiler read other text than the disk file; kept registered so the regression test can set it and watch nothing change |
| `ALMIDE_SURVIVE_OVERLAY_TEXT=value` | harness | RETIRED (#4129) with `ALMIDE_SURVIVE_OVERLAY`: no longer read |
| `ALMIDE_TCO_DEBUG` | debug | print the native tail-call loop rewrite decisions |
| `ALMIDE_TEST_ABORT_PROBE` | runtime | marks a native testing.assert_aborts child (ADR-0029) so `panic` takes the C-219 abort path there; set by the parent, never by hand |
| `ALMIDE_TEST_LAX_WASM` | gate | let the default `almide test` lane (wasm first, native fallback) PASS a file whose wasm leg diverged — trapped where the native re-run passed; without it a diverged leg fails the run |
| `ALMIDE_TEST_LEDGER=value` | runtime | the completion-ledger file a native `almide test` binary appends each finished test's name to (#4255): set by the runner, never by hand |
| `ALMIDE_TEST_SIGNALLED_CALL_URL=value` | harness | the loopback URL that turns the almide-rt-core http test's re-run of itself into the signalled-call child (#3857): it spawns a missing command while a call waits on that URL; set by the parent test, never by hand |
| `ALMIDE_TEST_VERBOSE` | tool | show the full cargo / rustc output of the `almide test` harness build |
| `ALMIDE_TEST_WASM_RUNNER=value` | tool | `wasmtime` runs the wasm leg of `almide test` on the `wasmtime` CLI (the stock `to_wasi` artifact) instead of the embedded host every `almide` binary carries (#3046) — the comparison route; unset is the embedded host |
| `ALMIDE_TIME_PHASES` | debug | print the wall-clock time of each `almide run` phase |
| `ALMIDE_TMPDBG` | debug | print the temporary-drop decisions of the v1 lowering |
| `ALMIDE_TOPLET_DEBUG` | debug | print the cross-module top-let type writes and reads of the checker |
| `ALMIDE_TRACE_PASSES` | debug | name each optimiser pass BEFORE it runs, so a pass that never returns is identifiable |
| `ALMIDE_UPDATE_ALLOC` | harness | regenerate the structural leg's allocation baseline |
| `ALMIDE_UPDATE_DUMPS` | harness | regenerate the structural leg's section-dump goldens |
| `ALMIDE_UPDATE_GAUNTLET` | harness | regenerate the gauntlet manifest |
| `ALMIDE_UPDATE_INSPECTION` | harness | add the missing rows to the stdlib inspection ledger and reset the stale ones |
| `ALMIDE_UPDATE_INTERP_LEDGER` | harness | regenerate the interpreter abstain and bridge-fallback ledgers |
| `ALMIDE_UPDATE_NAME_SITES` | harness | remove stale rows from the name-keyed property ledger and append the new sites (each still needs its `why`) |
| `ALMIDE_UPDATE_NATIVE_OWN` | harness | regenerate the native result-ownership ledger |
| `ALMIDE_UPDATE_NATIVE_VERIFIED_FLOOR` | harness | regenerate proofs/native-verified-floor.txt, the spec/wasm_cross fixtures the verified native renderer covers (#4006, tests/native_verified_floor_test.rs) |
| `ALMIDE_UPDATE_SIZES` | harness | regenerate the structural leg's size baselines |
| `ALMIDE_UPDATE_SIZE_LADDER` | harness | regenerate the stdlib-linking size ladder ledger (#2141) |
| `ALMIDE_UPDATE_SNAPSHOTS` | tool | same as `almide test --update-snapshots` |
| `ALMIDE_UPDATE_STDLIB_COVERAGE` | harness | rewrite the stdlib test-coverage ledger's rows from the measured run |
| `ALMIDE_UPDATE_SURFACE` | harness | regenerate the exercised-surface golden |
| `ALMIDE_UPDATE_WITNESS_FLOOR` | harness | regenerate the certificate witness floor |
| `ALMIDE_VERBOSE` | debug | same as `almide -v`: surface the native wall-and-fallback notes that a quiet run hides |
| `ALMIDE_VERIFIED_DEBUG` | debug | name the wasm leg that rendered, and why the other declined (the route oracle) |
| `ALMIDE_VERSION_LINE=value` | ci | NOT read from the environment at run time: `build.rs` EMITS it as `cargo:rustc-env`, and `src/main.rs` reads it with `env!` at compile time. It is the string `almide --version` prints — `<version> (<kind>[, <sha>])` — assembled from ALMIDE_BUILD_PROVENANCE and ALMIDE_BUILD_SHA (#2384) |
| `ALMIDE_WALL_REASON` | debug | make `almide test` say WHICH stage of the wasm leg declined a fallback file, not just `v1 wall` |
| `ALMIDE_WASM_ALLOC_COUNT` | instrument | emit the structural wasm leg's allocation counters (#2407: five i64 globals `$alloc`/`$free`/a region restore bump, exported as `__alloc_count` / `__alloc_reused` / `__alloc_bytes` / `__free_count` / `__region_reclaimed`; `main` also releases its top-let globals before returning) and make `almide run --target wasm` print `__ALMD_WASM_ALLOC allocs=N reused=N bytes=N frees=N reclaimed=N live=N heap_end=N` on stderr after the run, `live` being the heap blocks never released; off, the module is byte-identical to a build without the switch (the wasm twin of `ALMIDE_ALLOC_COUNT`; the ledgers are crates/almide-wasm/tests/golden/alloc-count-baseline.txt and live-at-exit-baseline.txt) |
| `ALMIDE_WASM_CANONICAL_ALLOC` | instrument | emit the structural wasm leg's `$alloc` without its free-list paths, so every request bumps (the stdlib inspection meter's fixed allocator, owner ruling 2026-10-15): a measured bytes-moved series then depends on the program's requests alone, not on which freed blocks the heap holds, and is an upper bound of the shipped module's copies; off, the module is byte-identical to a build without the switch (crates/almide-wasm/src/canonical_alloc.rs) |
| `ALMIDE_WASM_CHECKED_ALLOC` | instrument | emit the structural wasm leg's `$free` poisoning a freed size-class block with 0xA5 and `$alloc` checking that poison before reusing the block, trapping on a write that landed while it was free (#4009: the wasm leg's use-after-free and stray-write detector; scripts/wasm-checked-alloc.sh runs the corpus under it); off, the module is byte-identical to a build without the switch (crates/almide-wasm/src/checked_alloc.rs) |
| `ALMIDE_WASM_FREES` | ci | the frees-churn gate's switch; its compiler reader retired with the v0 emitter (#782), the gate that still sets it is #2207's |
| `ALMIDE_WASM_SKIP_STOCK_AUDIT` | route | skip the stock-WASI host-op audit on `almide build --target wasm` (the emitter-frontier probe the parity, diff-fuzz and availability gates build under); named ALMIDE_WASM_STRUCTURAL until #2934 |
| `ALMIDE_WASM_WATCHDOG_SECS=value` | harness | the wall-time seconds the IN-PROCESS test runner's epoch watchdog (`run_wasm` / `run_wasm_with` / `run_wasm_capped` in almide-wasm-run) lets a module run before it traps with `interrupt` (default 30); `almide run --target wasm` and `almide bench --target wasm` arm no watchdog on any setting, as native has none (#2615) |
| `ALMIDE_WITNESS_DUMP` | harness | print every fixture's certificate witness in the witness-floor test |
| `ALMIDE_WRITE_FUZZ_CORPUS` | harness | write the generated fuzz programs to disk |

Outside `ALMIDE_*`:

| Variable | Description |
|---|---|
| `CI` | `true` is equivalent to `almide test --ci` (does not write snapshots) |
