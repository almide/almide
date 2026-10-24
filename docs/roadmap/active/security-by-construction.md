<!-- description: The security claim Almide is working toward — the vulnerability classes behind the October 2026 PHP and Go advisories, what makes each impossible in Almide today, the gaps with their issues, and the rule for when the claim may be stated -->
# Security by construction

> Last updated: 2026-10-25
> Parent issue: #4011 (memory safety with no gap). Related record: the stdlib redesign's R-L0-45 / R-L0-46.

## The claim

> A program written in Almide cannot be made to corrupt memory, run attacker-supplied code, or
> be steered outside the files and capabilities its manifest grants — whatever input it is given.
> Untrusted input can make it fail, but only with a defined error or a defined abort, within
> documented bounds on time and memory.

It may be stated without qualification only when every row of every table below reads **closed**:
the class is impossible by construction, or a mechanism prevents it and a gate in CI fails when
the mechanism stops working. Until then the claim is stated per class, with this page as the
reference. What stays trusted even then (the Rocq kernel, rustc / LLVM / wasmtime, the hardware,
code behind `@extern`) is listed in [proofs/TRUSTED_BASE.md](../../../proofs/TRUSTED_BASE.md) and
[docs/contracts/proven-vs-trusted.md](../../contracts/proven-vs-trusted.md).

## The classes, from the October 2026 advisories

PHP2Shell (CVE-2026-109618 / 109619, disclosed 2026-10-21, exploited in the wild): an
out-of-memory interruption leaves request data half-initialised and cleanup treats it as valid;
request state survives into the next request on the same worker after its memory was freed.
Go's 2026 advisories: memory corruption from a compiler bug in bounds-check elimination
(CVE-2026-27143) and in pointer moves (CVE-2026-27144); a cgo double free (CVE-2026-33811);
unbounded allocation from crafted tar / DER / TLS / mail input; a symlink escape from `os.Root`.
The PHP-classic classes (eval, loose equality, object deserialization, shell strings) are in the
table because they are what "PHP-like" means to most readers.

| # | Class | Status | Mechanism today | Gap and work |
|---|---|---|---|---|
| 1 | Out-of-memory leaves half-built data in use | **closed** | Allocation failure never returns: wasm `memory.grow` failure prints and exits 1 (`crates/almide-wasm/src/runtime_alloc.rs:149-160`); native fallible sites call `almide_abort`, the rest hit Rust's abort. Nothing catches it (the only `catch_unwind` is fan's, for panics). C-197. | Native non-fallible sites abort with SIGABRT, not the contracted exit 1 (cosmetic; folded into #4023's audit). |
| 2 | Request state survives into the next request | partly | No mutable variable can be reached from a handler: `var` in a concurrent body is E008 (epoch 6), an app capturing a local of its server fn is E095 (epoch 11). | Freed memory reused across requests reduces to class 3 on the long-lived instance (native loop, wasm P3 reuse up to 128 requests). Closes with class 3. |
| 3 | Use after free, double free | partly | Perceus reference counting, no manual free in the language. The checker proves RC balance of a witnessed MIR (Rocq); `CowSafety`, `FreeList` proofs cover the retired incumbent runtime. Every wasm build traps a double free (a release at rc 0 hits `unreachable`, exit 134 under wasmtime; #4023, `StructuralRuntime.dec_freed_block_traps`). | No per-build certificate on shipped wasm bytes (#1696); native trusted (#4006); the native region arena's `unsafe` traps only behind a switch — off in shipped builds (+30% / +52% on the two region rows), armed over `spec/` and the differential fuzz in CI by `scripts/check-memory-traps.sh` (#4023); the runtime's `unsafe` lines (#4007, gated by `scripts/check-runtime-unsafe.sh` from #4017; the emitted preludes counted since #4023). |
| 4 | Compiler bug removes a bounds check | partly | Every `xs[i]` and stdlib index is checked (C-067; native macros always check). The wasm emitter elides or hoists checks (`bounds_facts.rs`, `len_hoist.rs`, `payload_ptr.rs`, `cow_hoist.rs`) with a soundness argument in each header, no proof. | A bounds term in the certificate (#4005) on top of the `MemorySafe` definition (#4013); until it lands, the differential fuzz runs with the elisions off as the reference (#4009). |
| 5 | Memory errors through FFI or raw pointers | partly | Raw memory is an explicit opt-in, as in Rust's `unsafe`. The stdlib functions that read or write at a raw address, or hand out the address that does, are declared `@memory_unsafe` once at the function (`ffi.*` and the deprecated `bytes` raw-pointer spellings). Outside the stdlib a call to one, in any spelling, is E100 unless the calling file writes `import ffi` (#4043, epoch 15), so a program whose files have no `import ffi` and no `@extern` cannot read or write memory outside its values on either target. `--profile critical` refuses `ffi`, those spellings and every `@extern` declaration outright, with no `--allow` grant (E100, #4008). `@extern` is counted as every capability (epoch 8). | Inside a file that opted in, nothing checks that an address and a length describe memory the program owns: the caller promises it. A build does not yet say that a program left the guarantee — `almide build` prints the same line for a program that calls `ffi.write` (#4008's receipt criterion). The runtime functions behind `ffi.read` / `ffi.write` are among the `unsafe` lines #4007 audits. |
| 6 | Data race corrupts memory | partly | Values are copied, not shared (C-033); fan work needs `T: Send` and Rc-backed values are not `Send`; E008; wasm fan is sequential. | The fan synchronisation itself is `unsafe` (lifetime-erasing transmute, `unsafe impl Sync`): Miri and loom over it (#4007, #4009). |
| 7 | Integer overflow becomes memory corruption | **closed** | Overflow wraps by contract (C-170, C-180) but never reaches memory: every index and size is checked after the arithmetic (C-067), allocation sizes saturate into the out-of-memory abort. `MIN / -1` aborts (C-002). | Silent wraparound is a correctness question, not a memory one; the checked / saturating / wrapping spellings are the stdlib redesign's R-L0-32/34 and R-int-12. |
| 8 | Null dereference | **closed** | No null in the language (`docs/SPEC.md:84`); absence is `Option`. Only `RawPtr` can be null and its bridge guards it (C-062). | — |
| 9 | Resource exhaustion from crafted input | **closed** | The HTTP server caps body, line, headers and time, serves through a 64-connection pool and answers a request head not whole within 10 s with 408, so a slow client holds only its own connection (C-389, #4021); the client caps response size. The zlib decoders stop at 256 MiB of output with one err text (C-390), and `json.parse` refuses nesting past 1000 levels (C-391) (#4020). Regex matching is linear in the subject's length for every pattern: the Pike VM's work per character is at most its state count (#4019), and `regex.new` / `regex.compile` refuse a pattern past 20,000 states with one reason on every target (C-395, #3942), so a pattern taken from input costs at most 20,000 steps per character. Negative control: with the cap removed, `patterns_at_the_state_limit_finish_on_long_input` (tests/regex_pike_differential_test.rs) failed with "the rows at the limit took 46.502124167s"; with it, the refused rows answer at once and the test passes in 0.06 s. Deep recursion ends in a defined abort (C-196). | — |
| 10 | Path traversal, symlink escape | **closed** on 64-bit Linux and macOS | File access needs the `IO` capability. `fs.open_root` and the `fs.root_*` family refuse a path that leaves the root by `..`, an absolute path or a symlink, with one err text on every target (C-392); the symlink refusal is the call itself — openat2 RESOLVE_BENEATH \| RESOLVE_NO_SYMLINKS on Linux, an O_NOFOLLOW segment walk from the root's descriptor elsewhere and in the stock-runtime fs service — so a directory swapped for a symlink mid-call is never followed (#4084). `[permissions] fs` in almide.toml bounds every fs call of `almide run` / `almide build` / `almide test` and of the embedded wasm host to the granted directories, symlinks resolved, and the p3 component host preopens only those directories instead of `/` (C-393, #4022); the scope's check only decides, and the call runs beneath the granted directory's held descriptor with the same walk, so a tree rewritten between the check and the call is refused by the call (#4084). `fs.walk` / `fs.glob` list a symlink as an entry and never descend into it (C-396, #4143), and the toolchain's `fmt` / `test` walkers follow no symlink and `fmt` rewrites a regular file whole, never through a link (#4140). `fs.create_temp_file` / `create_temp_dir` create a new private entry exclusively under an OS-entropy name (C-397, #4113). `path.from_string` admits only a path that stays inside its directory, on both separators, and `path.join_within` refuses an escaping child (C-398, #4146). | Path-based still, by what std exposes: an operation on a granted directory itself, and the metadata of an entry the process may not open. On other platforms `[permissions] fs` and the rooted family check and then call, and a rewrite can race them. |
| 11a | eval, dynamic code loading | **closed** | The language has neither. | — |
| 11b | Loose equality, type juggling | **closed** at run time | `==` is structural equality on one type. | `1 == "1"` passes `almide check` and is refused only by rustc / the wasm wall (#4018). |
| 11c | Deserialization that runs code | **closed** | `Codec` decode maps a `Value` onto typed fields; it creates no objects and calls no code (`stdlib/codec_decode.almd`). | — |
| 11d | Shell injection | **closed** | `process.*` takes a program and an argv list and spawns it without a shell (`crates/almide-rt-core/src/process_core.rs`). | — |
| 11e | HTML injection | **closed** | `SafeHtml` is opaque; `html.escape` is the only way in from a `String`, `html.raw` is the named opt-out. | — |

## Toolchain and supply chain

The first table covers what an Almide program can be made to do. These rows cover how attacker
code reaches a program before it runs: through the source a reviewer reads, the dependencies a
model writes, and the CI that builds and signs the release. The 2026 sources: the npm worms
spread by maintainer account takeover (Shai-Hulud and its August 2026 keyv / cacheable variant),
models inventing package and repository names that attackers then register (slopsquatting, and
July 2026's HalluSquatting, up to 85% on repository-cloning prompts), and moved action tags
(tj-actions/changed-files, CVE-2025-30066).

| # | Class | Status | Mechanism today | Gap and work |
|---|---|---|---|---|
| 12 | Predictable randomness used for a secret | **open** | wasm reads host entropy (`prim.random_get`, `wasi:random`). | Native `random.int` / `float` / `choice` / `shuffle` are a time-seeded xorshift (`runtime/rs/src/random.rs`), and no function promises cryptographic randomness, so a model writing a token with `random.int` writes a guessable one. `random.bytes` as the one CSPRNG, the others documented as not for secrets (#3938). |
| 13 | Source that reads differently from what compiles (Trojan Source) | **closed** | A raw bidirectional control character (U+202A–U+202E, U+2066–U+2069) anywhere in a source file — comment, string literal, heredoc, raw string — is E101, refused by the front end on every target (`report_bidi_controls`, fed by `find_bidi_controls` in `crates/almide-syntax/src/lexer.rs`; epoch 15, #4092); `\u{202E}` in a literal still decodes to the character, and a rendered diagnostic echoes the character as that escape. Identifiers are ASCII-only (`crates/almide-syntax/src/lexer.rs:212`), so homoglyph identifiers cannot be written. Gate: the three fixture families `tests/diagnostics/e101-*` (comment + literal + heredoc, inline block comment, raw string) and the escape near-miss `tests/diagnostics/silent/bidi-control-written-as-escape.almd`, run by `tests/diagnostic_harness_test.rs`, with `scripts/check-diagnostic-code-coverage.sh` holding the code at three families. Negative control: with the E101 check removed (`report_bidi_controls` not called), all three broken fixtures check clean (`almide check` exit 0) and `cargo test --release --test diagnostic_harness_test` fails (exit 101, "expected code E101 not found"). | — |
| 14 | A hallucinated or typo'd dependency | **closed** | A git dependency is fetched only from an owner the root `almide.toml` trusts: github.com/almide by default, any other `host/owner` listed in `[trust] owners` (`src/trust.rs`). The check runs before any network access on `almide add` and on every fetch (`build` / `run` / `test` / `bench` / `update`), for direct, transitive and locked dependencies; a dependency's own `[trust]` is never read. `almide add` resolves, checks, probes that the repository exists, fetches, and only then writes the manifest and lock; a refusal prints the source URL, the owner and one exact command (`--trust <owner>` must repeat the owner; there is no `--yes`). A not-found or refused name gets a runnable did-you-mean from an embedded snapshot of the org's packages. A source changed under the lock is refused until `almide update`; `--locked` refuses anything the lock does not name. The cache is keyed by the source URL, injectively, so `evil/svg` cannot answer for `almide/svg` (#4093). An `import` that names no module suggests the nearest stdlib module, declared dependency or almide package (`import jsonn` → `import json`) before it mentions `[dependencies]`, and never points at adding a guessed name (#4109; fixture `tests/diagnostics/import-near-stdlib-module/`, whose negative control, dropping the near-name step, fails `diagnostic_harness_test` with exit 101). Gate: `tests/dependency_trust_test.rs`. Negative control: `is_trusted` returning true makes 5 of its 11 tests fail (`cargo test --test dependency_trust_test` exit 101). | A local `file://` or plain-path git source needs no trust. The lock pins direct dependencies only; a transitive one is resolved by its ref. `almide install` builds a package under that package's own `[trust]`. |
| 15 | A dependency's code runs at build time | **closed** | A dependency that brings native code is refused on every target unless the root `almide.toml` grants it in `[permissions] native` (`src/native_grant.rs`): `native/` files, `[native-deps]` crates (whose `build.rs` and proc macros cargo runs), `@extern(rust|c)` fns (an `@extern(rust)` path is spliced verbatim, `std` included), and `@inline_rust` templates (#4100). The refusal names the dependency, the code it brings and the line to add; a granted one is named on every native build; `almide deps` lists each dependency's capabilities, native code and grant (#4093). `@extern` and, outside the stdlib, `@inline_rust` count as every capability (epoch 8, #4100), and E085 grants stdlib authority by identity with the embedded sources, not by a `stdlib/` path. Gates: `tests/native_grant_test.rs`, `tests/inline_rust_authority_test.rs`. Negative controls: `refusal` returning `None` makes 3 of 6 native-grant tests fail (exit 101); dropping the `@inline_rust` effect rule, the template detection, or the E085 identity check each fails one of the 3 inline-Rust tests (exit 101 each). | The root package's own native code needs no grant, by design. A grant names a package, not a version. |
| 16 | A compromised CI action | partly | Release assets carry Sigstore attestations and sha256 checksums (#1534); the default workflow token is read-only; Dependabot covers the `github-actions` ecosystem. | Every `uses:` is a tag or branch, not a commit, and the repository does not require commit pins. `release.yml`, which can write and attest a release, is exposed to a moved tag (#4094). |
| 17 | A manifest that says "no capabilities" grants all of them | **closed** | An explicit `[permissions] allow = []` allows no capability: every function that reaches one is a capability violation on `check`, `check --effects`, `build` and `run`. An absent `allow` key or table stays unchecked (#4101). Gate: `tests/manifest_permissions_test.rs` (`an_explicit_empty_allow_denies_every_capability`). Negative control: reading `allow = []` as absent again fails it (exit 101). | Whether a missing `[permissions]` table should stay unchecked is a separate decision. |

## Found by the red team (2026-10-24 / 25)

A red-team sweep against develop (programs, stdlib, runtime, embedded host, toolchain, and the
contract ledger) found classes the two tables above did not name, and gaps inside rows they do.
Every item was reproduced on a real binary, native and wasm, and has an issue with its probe.
The new classes are rows of this page like any other: the claim waits on them too.

| # | Class | Status | Mechanism today | Gap and work |
|---|---|---|---|---|
| 17 | A dependency's inline Rust escapes every check | **open** | `@intrinsic` / `@wasm_intrinsic` / `prim` outside the stdlib are refused (E085). | A package's `@inline_rust` is spliced into the native build with no effect category, outside E006, `[permissions]` and the native grant; E085 decides "is stdlib" by path (#4100). |
| 18 | A manifest that means "nothing" allows everything | **open** | A non-empty `[permissions] allow` is checked on every target (#3275). | `allow = []` is read as "no `[permissions]`", so deny-all cannot be written (#4101). |
| 19 | The module does not carry its capability set | **open** | Effects are inferred over the linked program. | Every wasm module imports `fs_call` / `host_read` whatever its effects (#4102). |
| 20 | A secret authenticated or compared with the wrong primitive | **open** | FIPS 180-4 digests on both legs. | No MAC and no constant-time compare, so `sha256(secret ++ body)` and `token == expected` are what a model writes (#4106). |
| 21 | A secret or internal state reaches output through a renderer | **open** | A type's own `T.repr` is used by interpolation. | An `assert` failure ignores `T.repr` (#4107); runtime error lines embed paths and arguments raw (#4126); an `http.serve` handler error becomes the 500 body and nothing is logged (#4150). |
| 22 | Input sets a field the program never meant to take from input | **open** | `Codec` runs no code (11c). | Mass assignment: decode fills a defaulted privileged field and drops unknown keys, with no exclusion or strict mode (#4108). |
| 23 | An outdated toolchain re-opens a fixed std vulnerability | **open** | `almide build` checks that `rustc` runs. | No minimum version; CVE-2024-24576 returns on Windows below 1.77.2 (#4110). |
| 24 | Input that two parsers read differently | **open** | Native and wasm parse identically for most inputs. | JSON deletes a lone surrogate (`ad\ud800min` → `admin`, #4111) and is lenient on duplicates and trailing data (#4112); HTTP request line and header leniency (#4112); url vs WHATWG (#4116); `query_params` native ≠ wasm (#4117); `parse_iso` year wrap (#4118); base64 malleability (#4119); case mapping and NaN / Unicode-space number parsing (#4121); router vs middleware path (#4133); truncated HTTP responses accepted (#4137); stdin errors read as "" on native (#4142); `args` `-d` collision (#4144); `float.to_int` with no checked twin (#4145); negative `take` (#4147); `from_parts` rollover (#4149). |
| 25 | The toolchain acts on an untrusted project before it is run | **open** | `didChange` never fetches (#927). | The LSP's `didOpen` and `almide ide` fetch dependencies and write `almide.lock` (#4128); environment switches replace the compiled source or skip checks in release builds (#4129); `fmt` / `test` walkers follow symlinks and rewrite files outside the target (#4140). |
| 26 | A capability reaches code through a function value or a misfiled category | **open** | E006 and `[permissions]` see direct calls and `let` aliases. | Passing an effect fn as a value (`list.map(fs.read_text)`, user higher-order functions, record fields, fn-typed parameters, `option.map`) lets a pure fn do I/O (#4151) and bypasses `[permissions]` (#4152); process spawning is the `Env` category (#4155); a protocol-bound call skips E006 (#4158); host functions declared as plain `fn` (#4159). |
| 27 | Compile cost from untrusted source | **open** | Syntactic nesting is capped at 500. | Inferred types grow 2^n with no sharing (#4114). ADR-0030 makes compile cost a contract (#4136). |

Gaps the sweep found inside rows 1–16 (each row's status changes only in its gate PR, as below):

- **Row 3**: C-376 is violated. Rebinding a `var` frees the block under a live `let p = ffi.address(v)`, and a write corrupts an unrelated value on native (#4153).
- **Row 9**:
  - hash flooding (#4104);
  - net reads allocate the requested length (#4123);
  - unix fds pile up (#4124);
  - embedded-host caps multiply per fan worker (#4135);
  - fan threads are unbounded (#4141);
  - `bytes.repeat` has no ceiling (#4154);
  - derived `Codec.decode` aborts on a short variant payload, a one-request DoS (#4164).
- **Row 10**:
  - no safe temp file, and `create_temp_*` use predictable names (#4113);
  - route params bind `..` (#4134);
  - `fs.walk` follows symlinks (#4143);
  - SafePath admits absolute paths (#4146);
  - wasm `read_text_if_exists` reads ELOOP as absent (#4154).
- **Row 11d**: `process.kill` / `is_alive` run `kill` from `PATH` and accept pid ≤ 0 (#4125); `process.run` is `ok` on failure (#4148).
- **Row 11e**: `html.escape` is text-context only, and no HTML response requires `SafeHtml` (#4120). The class is partly open in substance.
- **Row 12**: the embedded host serves `random_get` from a clock-seeded xorshift (#4122).
- **Row 14**: an unknown import hints at adding a dependency (#4109).
- **Row 16**: self-update and the installers fail open and do not verify the attestation (#4127).
- **Cross-target divergences** (no row of their own; each breaks the byte-identity promise): #4117, #4138, #4142, #4154, #4161, #4163, #4165.

## Order of work

1. **Close class 5 first.** Ordinary code can no longer corrupt memory: the stdlib redesign's
   `ffi` module (R-ffi-01/02, R-L0-45) took the raw-address surface, #4008 made `--profile critical`
   refuse `ffi` and `@extern`, and #4043 made every `@memory_unsafe` call outside the stdlib need
   the file's `import ffi` (negative fixtures `tests/diagnostics/e100-no-import-ffi-oob-*`).
   What is left is the build receipt: a build that reaches `ffi` or `@extern` must say so (#4008).
2. **Bound untrusted input** (#4019, #4020, #4021) and **scope file access** (#4022): these are
   behaviour changes with contracts and cross-target fixtures. They moved rows 9 and 10 to partly;
   #4083 and #4084 close them.
3. **Turn the traps on** (#4023) and **run fuzz under Miri / ASan / a checking allocator** (#4009):
   they turn the remaining trusted parts of rows 3 and 6 into findings CI can see.
4. **Certificates**: `MemorySafe` defined once (#4013), the bounds term (#4005), the structural
   leg's certificate (#1696), then the native leg (#4006). This is what turns rows 2, 3 and 4
   from "partly" to closed.
5. **Source semantics** (#4014) is the long tail: it proves the checker's verdict means what this
   page says.
6. **The toolchain rows** run alongside steps 1–5, since none of them depends on the compiler's
   memory model. Do the small ones first: commit-pinned actions (#4094), the bidi refusal (#4092),
   `random.bytes` (#3938). The dependency rows 14 and 15 closed with #4093, #4100 and #4101.

## How the page stays true

Each row's status changes only in the PR that lands its gate. A row moves to **closed** when the
mechanism is in CI and its negative control fails as expected; the PR quotes the control. The
claim at the top is stated in the README and llms.txt only when every row is closed.
