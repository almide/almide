Description: Stdlib redesign L2/L3 — env owns argv, environment variables, cwd, OS name and sleep; twins in process/args/fs/datetime retire into their owners.
<a id="l2l3-env"></a>

# L2/L3: env

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](L2-env.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](L2-env.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](L2-env.zh-CN.md)

> Non-normative historical redesign snapshot; original authoritative and unchanged. No automatic synchronization, new probing or present implementation verification. Preserve proposal-versus-implementation differences: remove initially returns Unit, later implementation/table Result; set table still says Unit while text says err. Do not select the unresolved effect/Option call form. Secret display and constant-time claims reproduce the cited source contracts, not a universal leak-prevention or independent security assurance; no credentials were accessed. Duration rejection is a dated decision, not reconciliation with other ADR epochs. [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-env.md)

> Last updated: 2026-10-26
> Prerequisites: [L0](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L0-conventions.md), [L1](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L1-categories.md) (R-L1-25's ownership table underlies this page).
> Sources: Rust https://doc.rust-lang.org/std/env/; Go https://pkg.go.dev/os; Python https://docs.python.org/3/library/os.html; TS/JS https://nodejs.org/api/process.html; Gleam https://gleam-stdlib.hexdocs.pm/ (env is not in stdlib); Elixir https://hexdocs.pm/elixir/System.html; OCaml https://ocaml.org/manual/latest/api/Sys.html; Kotlin https://kotlinlang.org/api/core/kotlin-stdlib/kotlin.system/; Swift https://developer.apple.com/documentation/foundation/processinfo.
> Spellings across 9 languages come from the verified tables in [facts/naming-conventions.md §20](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/facts/naming-conventions.md) and [facts/categories.md §1.H / §3.10](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/facts/categories.md).

<a id="l2-役割の比較"></a>

## L2: Role comparison

| Role | Almide | Rust | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin | Swift |
|---|---|---|---|---|---|---|---|---|---|---|
| argv (arguments) | `env.args`（excludes argv[0]）・`process.args` (includes)・`args.raw`（= env.args） | `env::args` (includes) | `os.Args` (includes) | `sys.argv` (includes) | `process.argv` (includes)・`Deno.args` (excludes) | — | `System.argv` (excludes) | `Sys.argv` (includes) | `main(args)` (excludes) | `CommandLine.arguments` (includes) |
| Read one environment variable | `env.get`・`process.env` | `env::var` | `os.LookupEnv` | `os.environ.get` | `process.env.K` | — | `System.get_env/1` | `Sys.getenv_opt` | `System.getenv`(JVM) | `ProcessInfo.environment[k]` |
| Read all environment variables | — | `env::vars` | `os.Environ` | `os.environ` | `process.env` | — | `System.get_env/0` | `Unix.environment` | `System.getenv()`(JVM) | `ProcessInfo.environment` |
| Set environment variable | `env.set` | `env::set_var` | `os.Setenv` | `os.environ[k] = v` | `process.env.K = v` | — | `System.put_env` | `Unix.putenv` | — | `setenv`(libc) |
| Remove environment variable | — | `env::remove_var` | `os.Unsetenv` | `del os.environ[k]` | `delete process.env.K` | — | `System.delete_env` | — | — | `unsetenv`(libc) |
| Working directory | `env.cwd` | `env::current_dir` | `os.Getwd` | `os.getcwd` | `process.cwd()` | — | `File.cwd` | `Sys.getcwd` | JVM `user.dir` | `FileManager.currentDirectoryPath` |
| OS name | `env.os` | `env::consts::OS` | `runtime.GOOS` | `sys.platform` | `process.platform` | — | `:os.type` | `Sys.os_type` | — | `ProcessInfo.operatingSystemVersion` |
| Temporary directory | `env.temp_dir`・`fs.temp_dir` | `env::temp_dir` | `os.TempDir` | `tempfile.gettempdir` | `os.tmpdir()` | — | `System.tmp_dir` | `Filename.get_temp_dir_name` | JVM `java.io.tmpdir` | `FileManager.temporaryDirectory` |
| Wait (sleep) | `env.sleep_ms`・`process.sleep` | `thread::sleep` | `time.Sleep` | `time.sleep` | `setTimeout`（Node `timers/promises`） | — | `Process.sleep` | `Unix.sleep` | `Thread.sleep`(JVM) | `Thread.sleep` |
| Current time (real time) | `env.unix_timestamp`・`env.millis`・`datetime.now` | `time::SystemTime` | `time.Now` | `time.time` | `Date.now()` | — | `System.os_time` | `Unix.time` | `Clock.System.now()` | `Date()` |

<a id="完全性の規則"></a>

### Completeness rules

- `env` owns “what this process receives from outside” (R-L1-25): argv, environment variables, working directory, OS name, waiting.
- Complete the four environment-variable operations: {read one `get`, read all `vars`, set `set`, remove `remove`}. With `set` but no `remove`, there is no way to remove a variable before passing the environment to a child process.
- Clocks belong to `datetime`, temporary directories to `fs`, child processes/termination/pid to `process`, flag parsing to `args`. No aliases for those in `env` (R-L0-01).
- Intentionally absent: argv[0] (program name). Measurements show a temporary-directory binary path for native `almide run`, but `wasm-harness` for wasm (probe-args). It varies by invocation method; portable programs cannot rely on it (R-L0-15).

<a id="l2-の判定"></a>

### L2 verdict

- Duplicates: `env.temp_dir` = `fs.temp_dir`, `env.unix_timestamp` = `datetime.now`, `process.env` = `env.get`, `process.sleep` = `env.sleep_ms`, `args.raw` = `env.args` → R-env-01.
- Wrong location: `env.millis` is a clock, so belongs to `datetime` → R-env-01.
- Missing roles: reading all environment variables (7/9), removing one (6/9) → R-env-02.
- Inconsistent call form: only `env.get` allows `??` without `!` → R-env-03 (resolved by L0's R-L0-17).

<a id="案"></a>

## Proposals

<a id="r-env-01-os-まわりの持ち分を-1-つにする---採用"></a>

### R-env-01 Give OS-related responsibilities one owner — ○ Accepted

- Content:
  - Remove `env.temp_dir` (→ `fs.temp_dir()`), `env.unix_timestamp` (→ `datetime.now()`).
  - Move `env.millis` → `datetime.now_ms()` (added in L2-datetime, same body).
  - Consolidate here (removed on their respective pages): `process.env(k)` → `env.get(k)`, `process.sleep(ms)` → `env.sleep_ms(ms)`, `args.raw()` → `env.args()`.
  - Remove `process.args()` (includes argv[0]), replacing it with `env.args()`. Add no function reading argv[0] (completeness rules above).
  - Correct the obsolete `env.sleep_ms` description “a negative ms hangs.” Measurements return immediately for negative values (probe-env, both native and wasm).
- L0 basis: R-L0-01 (one spelling per role), R-L0-02 (subject type/owning module), R-L0-13 (`process.env` reads the environment while remaining a pure fn), and L1 R-L1-25.
- (a) Rust `std::env`, Go `os`, Elixir `System`, OCaml `Sys` place argv and environment variables together. Only Elixir and OCaml put time in `env`; Rust and Go use separate modules.
- (b) Native/wasm: keep the surviving implementation unchanged. `env.get`, `env.args`, `env.sleep_ms`, `env.cwd`, `env.os`, `env.temp_dir` run through `wasi:cli/environment@0.3.0` and monotonic-clock `wait-for` in p3 components, and existing ops in the embedded host (no wall rows in `proofs/target-availability.toml`). `datetime.now_ms` uses `wasi:clocks/wall-clock`. ADR-0027 D3 inspection compares clocks using “inside the window?” verdict rows, and sleep using “monotonic-clock difference at least k ms?” rows.
- (c) Both `env.temp_dir` and `fs.temp_dir` appear in docs (CHEATSHEET uses fs, llms.txt env). Consolidation leads LLMs trained on either to one answer. The argv[0] difference between `env.args` and `process.args` invites off-by-one mistakes.
- (d) Removal and relocation. Compatibility: mark `env.temp_dir` with `@deprecated(since=15, use="fs.temp_dir")`, `env.unix_timestamp` with `use="datetime.now"`, `env.millis` with `use="datetime.now_ms"`; use `[[deprecation]]` and `removal_epoch = 16` (same format for `process.args`, `process.env`, `process.sleep` in L2-process R-process-01 and `args.raw` in L2-args R-args-02). `almide fix` rewrites one-to-one: `env.temp_dir()` → `fs.temp_dir()`, `env.unix_timestamp()` → `datetime.now()`, `env.millis()` → `datetime.now_ms()`. Usage (spec/tests/docs/stdlib): `env.temp_dir` 9/0/22/3, `env.unix_timestamp` 5/0/16/5, `env.millis` 11/7/21/3.
- Proposed gate: synonym-gate rule 5 (same body across modules, #3851 F) and R-L0-01's rule that a function merely forwarding one call to another public function also counts as the same role prevent `args.raw`-style forwarders from returning.
- Issue: #3851 B (existing). Add the `env.millis` move to the same issue.

<a id="r-env-02-envvars-と-envremove-を足す---採用"></a>

### R-env-02 Add `env.vars` and `env.remove` — ○ Accepted

- Content: `effect fn vars() -> Map[String, String]` (all environment variables, map sorted by key), `effect fn remove(name: String) -> Unit` (remove from this process and children launched afterward; absence is allowed).
- L0 basis: R-L0-15 (add only WASI-compatible functions), L1 criterion 1 (at least 5 languages).
- (a) Reading all: 7/9; removing: 6/9. Names vary (`vars`/`Environ`/`environ`/`get_env`). Choose `vars` and `remove`, near Rust's `vars` and `remove_var`, paired with `set`.
- (b) `vars` is directly `get-environment` in `wasi:cli/environment`. Like `env.set`, `remove` uses the p1 shim's guest-side override record (op 37, #3223), so wasm behaves the same. No new host op. Results are deterministic in ADR-0027 D2's fixed env.
- (c) For “print all environment variables,” LLMs seek one function from habits in other languages. Without it they cannot write the task.
- (d) Addition. Usage 0.
- Proposed gate: family-matrix test checks all four env operations get/vars/set/remove (`tests/env_variable_family_gate_test.rs`).
- Implementation (#3908): `remove` validates names like `set` (empty, `=`, NUL), so like `set` returns `Result[Unit, String]` (`env.remove: empty name`, etc.).
- Issue: #3908.

<a id="r-env-03-envget-を--で受ける形を自前の-effect-fn-と同じにする---採用r-l0-17-に従う"></a>

### R-env-03 Make receiving `env.get` with `??` match user-defined effect fn — ○ Accepted (follow R-L0-17)

- Measurements (probe-io): even effect fns returning `String?` have different call types depending on whether their body is `@intrinsic` or Almide.
  - `env.get("X") ?? "d"` passes; `env.get("X")!` propagates none as err `"none"` (`match env.get("X")! { some(..) => … }` errors because some cannot match String). The version of `io.read_line` that returns `Option` under R-io-01 behaves likewise.
  - `args.option("x") ?? "d"` errors; write `args.option("x")! ?? "d"` (#3522 form).
- Content: follow L0 R-L0-17 (call type depends only on declaration), unifying call types of `env.get`, `io.read_line`, `args.option`, `args.positional_at` into one form determined solely by `effect fn … -> T?`. Whether that permits direct `??` or requires `!` before `??` is designed in the R-L0-17 issue following ADR-0002/ADR-0008. This page's functions follow that result without renaming or declaration changes.
- L0 basis: R-L0-17.
- (a) Not applicable (no language has effect and Option as separate axes).
- (b) Checker-only change, identical on both targets.
- (c) When identical declarations require different calls depending on implementation, an LLM cannot determine a correct call from the declaration.
- (d) Language change. If unification changes current calls, record it in epoch 15 `breaks` under R-L0-17, with a hint (such as “write `env.get(k)! ?? d`,” according to the chosen form) and `almide fix` rewrite. No function renaming.
- Proposed gate: add the `env.get`/`args.option` pair to R-L0-17 fixtures checking that every standard effect fn's intrinsic/Almide variants accept `??` and `!` identically.
- Issue: same as R-L0-17 (new).
- Issue: #3900.

<a id="r-env-04-待ち時間タイムアウトの引数を-duration-にする---不採用"></a>

### R-env-04 Use `Duration` for wait/timeout arguments — × Rejected

- Content: change time arguments of `env.sleep_ms(ms: Int)`, `net.tcp_set_timeout(stream, timeout_ms: Int)`, `net.tcp_read_timeout`, `unix.select(xs, timeout_ms: Int)`, `process.stop(c, grace_ms: Int)` to `Duration`, renaming `sleep_ms` to `sleep`.
- Reason: only 4 of 9 languages take a time type for waiting: Go (`time.Duration`), Rust (`Duration`), Swift (`Duration`), Kotlin (`delay`'s `Duration` version), below the 5-language threshold. JS/Elixir/Kotlin take integer milliseconds, Python/OCaml numeric seconds. Almide decided to express quantities as `Int` with units in names (`sleep_ms`, `add_seconds`; L2-datetime “Types of functions accepting time quantities”). Under ADR-0001, `Duration` is for `fan` budgets/deadlines and has no numeric accessor (S7). Using `Duration` only for waiting adds `duration.ms(x)` wrapping for callers with `Int` milliseconds; there is no evidence it reduces misreading compared with today's unit-bearing names.
- Reopen when deciding to add a `Duration` accessor (revisit ADR-0001 S7).
- Issue: none.

<a id="r-env-05-資格情報を-secretstring-で読む-envsecret-を足す---採用2026-10-26"></a>

### R-env-05 Add `env.secret` reading credentials as `Secret[String]` — ○ Accepted (2026-10-26)

- Current: API tokens and webhook keys are read by `env.get` as `String`. `String` displays unchanged everywhere (logs, assertion failures), and `==` returns at the first differing byte, so comparison timing leaks tokens (security-table row 20).
- Proposal: `effect fn env.secret(name: String) -> Secret[String]?`. Wrap the same value as `env.get(name)` with `secret.new`. Display it as `Secret(***)` everywhere (C-425); `==` compares in constant time (C-426). Keep `env.get` unchanged.
- Basis: make secrets `Secret` from their origin (L2-secret). Since `hash.hmac_sha256` keys became `Secret[Bytes]` (L2-hash R-hash-07), their origin should return `Secret` too.
- Contract: C-438 (`spec/wasm_cross/env_secret.almd`).
- Issue: none.

<a id="l3-関数表"></a>

## L3: Function table

| Function | Signature (current, mut shown) | Main spellings elsewhere | Verdict | Proposal |
|---|---|---|---|---|
| `env.args` | `effect fn args() -> List[String]` | Rust `env::args` / Go `os.Args` / Python `sys.argv` / Elixir `System.argv`（argv[0] excluded by Elixir/Kotlin/Deno） | Keep | R-env-01 (consolidate here) |
| `env.get` | `effect fn get(name: String) -> String?` | Rust `var` / Go `LookupEnv` / Python `environ.get` / OCaml `getenv_opt` / Elixir `get_env` | Keep | R-env-01、R-env-03 |
| `env.set` | `effect fn set(name: String, value: String) -> Unit` | Rust `set_var` / Go `Setenv` / Elixir `put_env` | Keep (empty name, name containing `=` or NUL, or NUL in value are err; #4200, C-421) | — |
| `env.cwd` | `effect fn cwd() -> String` | Rust `current_dir` / Go `Getwd` / Python `getcwd` / OCaml `getcwd` | Keep | — |
| `env.os` | `fn os() -> String` | Rust `consts::OS` / Go `GOOS` / OCaml `os_type` | Keep (target-specific constant; C-189) | — |
| `env.sleep_ms` | `effect fn sleep_ms(ms: Int) -> Unit` | Rust `thread::sleep` / Go `time.Sleep` / Python `time.sleep` / Elixir `Process.sleep` | Keep (correct description only) | R-env-01 |
| `env.temp_dir` | `effect fn temp_dir() -> String` | Rust `env::temp_dir` / Go `os.TempDir` / Elixir `System.tmp_dir` | Remove（→ `fs.temp_dir`） | R-env-01 |
| `env.unix_timestamp` | `effect fn unix_timestamp() -> Int` | Python `time.time` / Elixir `os_time` | Remove（→ `datetime.now`） | R-env-01 |
| `env.millis` | `effect fn millis() -> Int` | JS `Date.now()` / Elixir `os_time(:millisecond)` | Remove（→ `datetime.now_ms` relocation） | R-env-01 |
| `env.vars` | `effect fn vars() -> Map[String, String]` | Rust `vars` / Go `Environ` / Python `environ` / Elixir `get_env/0` | Add | R-env-02 |
| `env.remove` | `effect fn remove(name: String) -> Result[Unit, String]` | Rust `remove_var` / Go `Unsetenv` / Elixir `delete_env` | Add | R-env-02 |
| `env.secret` | `effect fn secret(name: String) -> Secret[String]?` | — | Add (value originates as `Secret`) | R-env-05 |

<a id="l0-への差し戻し処理済み"></a>

## Referral back to L0 (resolved)

1. **Call types of effect fns returning Option** (R-env-03): L0 now includes R-L0-17 (call types depend only on declarations). Stop treating an `@intrinsic` body as `Option[T]` and an Almide body as `Result[Option[T], String]`; unify into one declaration-determined form. Which form is designed in the new R-L0-17 issue. Affected: `env.get`, `io.read_line`, `args.option`, `args.positional_at`, user effect fns. R-env-03 is marked ○.
