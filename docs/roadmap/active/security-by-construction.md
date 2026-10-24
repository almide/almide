<!-- description: The security claim Almide is working toward — the vulnerability classes behind the October 2026 PHP and Go advisories, what makes each impossible in Almide today, the gaps with their issues, and the rule for when the claim may be stated -->
# Security by construction

> Last updated: 2026-10-23
> Parent issue: #4011 (memory safety with no gap). Related record: the stdlib redesign's R-L0-45 / R-L0-46.

## The claim

> A program written in Almide cannot be made to corrupt memory, run attacker-supplied code, or
> be steered outside the files and capabilities its manifest grants — whatever input it is given.
> Untrusted input can make it fail, but only with a defined error or a defined abort, within
> documented bounds on time and memory.

It may be stated without qualification only when every row of both tables below reads **closed**:
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
| 5 | Memory errors through FFI or raw pointers | **open** | `@extern` is counted as every capability (epoch 8). | Safe code can read and write out of bounds today through `bytes.from_raw_ptr` / `copy_to_ptr`, even under `--profile critical` (probe on #4008). Fix: the stdlib redesign closes the surface to `ffi.read` / `ffi.write` behind `import ffi` (R-L0-45, R-ffi-01, #3853); critical refuses `ffi` and `@extern`, and builds record their use (#4008). |
| 6 | Data race corrupts memory | partly | Values are copied, not shared (C-033); fan work needs `T: Send` and Rc-backed values are not `Send`; E008; wasm fan is sequential. | The fan synchronisation itself is `unsafe` (lifetime-erasing transmute, `unsafe impl Sync`): Miri and loom over it (#4007, #4009). |
| 7 | Integer overflow becomes memory corruption | **closed** | Overflow wraps by contract (C-170, C-180) but never reaches memory: every index and size is checked after the arithmetic (C-067), allocation sizes saturate into the out-of-memory abort. `MIN / -1` aborts (C-002). | Silent wraparound is a correctness question, not a memory one; the checked / saturating / wrapping spellings are the stdlib redesign's R-L0-32/34 and R-int-12. |
| 8 | Null dereference | **closed** | No null in the language (`docs/SPEC.md:84`); absence is `Option`. Only `RawPtr` can be null and its bridge guards it (C-062). | — |
| 9 | Resource exhaustion from crafted input | **open** | The HTTP server caps body, line, headers and time; the client caps response size. Deep recursion ends in a defined abort (C-196). | Decompression bombs, json nesting, url/base64/hex sizes have no bound (#4020); ReDoS in the backtracking regex engine (#4019); one slow client stalls the native server (#4021). |
| 10 | Path traversal, symlink escape | **open** | File access needs the `IO` capability. | No path scoping; the embedded wasm host preopens `/` read-write; symlinks are not resolved against a root (#4022). |
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
| 13 | Source that reads differently from what compiles (Trojan Source) | partly | Identifiers are ASCII-only (`crates/almide-syntax/src/lexer.rs:212`), so homoglyph identifiers cannot be written. | Raw bidirectional control characters (U+202A–U+202E, U+2066–U+2069) in comments and string literals pass `almide check` with no diagnostic (#4092). |
| 14 | A hallucinated or typo'd dependency | **closed** | A git dependency is fetched only from an owner the root `almide.toml` trusts: github.com/almide by default, any other `host/owner` listed in `[trust] owners` (`src/trust.rs`). The check runs before any network access on `almide add` and on every fetch (`build` / `run` / `test` / `bench` / `update`), for direct, transitive and locked dependencies; a dependency's own `[trust]` is never read. `almide add` resolves, checks, probes that the repository exists, fetches, and only then writes the manifest and lock; a refusal prints the source URL, the owner and one exact command (`--trust <owner>` must repeat the owner; there is no `--yes`). A not-found or refused name gets a runnable did-you-mean from an embedded snapshot of the org's packages. A source changed under the lock is refused until `almide update`; `--locked` refuses anything the lock does not name. The cache is keyed by the source URL, injectively, so `evil/svg` cannot answer for `almide/svg` (#4093). Gate: `tests/dependency_trust_test.rs`. Negative control: `is_trusted` returning true makes 5 of its 11 tests fail (`cargo test --test dependency_trust_test` exit 101). | A local `file://` or plain-path git source needs no trust. The lock pins direct dependencies only; a transitive one is resolved by its ref. `almide install` builds a package under that package's own `[trust]`. |
| 15 | A dependency's code runs at build time | **closed** | A dependency that brings native code is refused on every target unless the root `almide.toml` grants it in `[permissions] native` (`src/native_grant.rs`): `native/` files, `[native-deps]` crates (whose `build.rs` and proc macros cargo runs), `@extern(rust|c)` fns (an `@extern(rust)` path is spliced verbatim, `std` included), and `@inline_rust` templates (#4100). The refusal names the dependency, the code it brings and the line to add; a granted one is named on every native build; `almide deps` lists each dependency's capabilities, native code and grant (#4093). `@extern` and, outside the stdlib, `@inline_rust` count as every capability (epoch 8, #4100), and E085 grants stdlib authority by identity with the embedded sources, not by a `stdlib/` path. Gates: `tests/native_grant_test.rs`, `tests/inline_rust_authority_test.rs`. Negative controls: `refusal` returning `None` makes 3 of 6 native-grant tests fail (exit 101); dropping the `@inline_rust` effect rule, the template detection, or the E085 identity check each fails one of the 3 inline-Rust tests (exit 101 each). | The root package's own native code needs no grant, by design. A grant names a package, not a version. |
| 16 | A compromised CI action | partly | Release assets carry Sigstore attestations and sha256 checksums (#1534); the default workflow token is read-only; Dependabot covers the `github-actions` ecosystem. | Every `uses:` is a tag or branch, not a commit, and the repository does not require commit pins. `release.yml`, which can write and attest a release, is exposed to a moved tag (#4094). |
| 17 | A manifest that says "no capabilities" grants all of them | **closed** | An explicit `[permissions] allow = []` allows no capability: every function that reaches one is a capability violation on `check`, `check --effects`, `build` and `run`. An absent `allow` key or table stays unchecked (#4101). Gate: `tests/manifest_permissions_test.rs` (`an_explicit_empty_allow_denies_every_capability`). Negative control: reading `allow = []` as absent again fails it (exit 101). | Whether a missing `[permissions]` table should stay unchecked is a separate decision. |

## Order of work

1. **Close class 5 first.** It is the only row where ordinary code can corrupt memory today. The
   stdlib redesign's `ffi` module (R-ffi-01/02, R-L0-45) lands ahead of the rest of its wave, and
   #4008 makes `--profile critical` refuse `ffi` and `@extern`.
2. **Bound untrusted input** (#4019, #4020, #4021) and **scope file access** (#4022): these are
   behaviour changes with contracts and cross-target fixtures, and they move rows 9 and 10 to closed.
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
