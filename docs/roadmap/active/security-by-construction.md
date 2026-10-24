<!-- description: The security claim Almide is working toward — the vulnerability classes behind the October 2026 PHP and Go advisories, what makes each impossible in Almide today, the gaps with their issues, and the rule for when the claim may be stated -->
# Security by construction

> Last updated: 2026-10-24
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
| 3 | Use after free, double free | partly | Perceus reference counting, no manual free in the language. The checker proves RC balance of a witnessed MIR (Rocq); `CowSafety`, `FreeList` proofs cover the retired incumbent runtime. | No per-build certificate on shipped wasm bytes (#1696); native trusted (#4006); the double-free trap is off by default (#4023); the native region arena's `unsafe` traps only behind a switch (#4023); the runtime's `unsafe` lines (#4007, gated by `scripts/check-runtime-unsafe.sh` from #4017). |
| 4 | Compiler bug removes a bounds check | partly | Every `xs[i]` and stdlib index is checked (C-067; native macros always check). The wasm emitter elides or hoists checks (`bounds_facts.rs`, `len_hoist.rs`, `payload_ptr.rs`, `cow_hoist.rs`) with a soundness argument in each header, no proof. | A bounds term in the certificate (#4005) on top of the `MemorySafe` definition (#4013); until it lands, the differential fuzz runs with the elisions off as the reference (#4009). |
| 5 | Memory errors through FFI or raw pointers | partly | Raw memory is an explicit opt-in, as in Rust's `unsafe`. The stdlib functions that read or write at a raw address, or hand out the address that does, are declared `@memory_unsafe` once at the function (`ffi.*` and the deprecated `bytes` raw-pointer spellings). Outside the stdlib a call to one, in any spelling, is E100 unless the calling file writes `import ffi` (#4043, epoch 15), so a program whose files have no `import ffi` and no `@extern` cannot read or write memory outside its values on either target. `--profile critical` refuses `ffi`, those spellings and every `@extern` declaration outright, with no `--allow` grant (E100, #4008). `@extern` is counted as every capability (epoch 8). | Inside a file that opted in, nothing checks that an address and a length describe memory the program owns: the caller promises it. A build does not yet say that a program left the guarantee — `almide build` prints the same line for a program that calls `ffi.write` (#4008's receipt criterion). The runtime functions behind `ffi.read` / `ffi.write` are among the `unsafe` lines #4007 audits. |
| 6 | Data race corrupts memory | partly | Values are copied, not shared (C-033); fan work needs `T: Send` and Rc-backed values are not `Send`; E008; wasm fan is sequential. | The fan synchronisation itself is `unsafe` (lifetime-erasing transmute, `unsafe impl Sync`): Miri and loom over it (#4007, #4009). |
| 7 | Integer overflow becomes memory corruption | **closed** | Overflow wraps by contract (C-170, C-180) but never reaches memory: every index and size is checked after the arithmetic (C-067), allocation sizes saturate into the out-of-memory abort. `MIN / -1` aborts (C-002). | Silent wraparound is a correctness question, not a memory one; the checked / saturating / wrapping spellings are the stdlib redesign's R-L0-32/34 and R-int-12. |
| 8 | Null dereference | **closed** | No null in the language (`docs/SPEC.md:84`); absence is `Option`. Only `RawPtr` can be null and its bridge guards it (C-062). | — |
| 9 | Resource exhaustion from crafted input | partly | The HTTP server caps body, line, headers and time, serves through a 64-connection pool and answers a request head not whole within 10 s with 408, so a slow client holds only its own connection (C-389, #4021); the client caps response size. The zlib decoders stop at 256 MiB of output with one err text (C-390), and `json.parse` refuses nesting past 1000 levels (C-391) (#4020). Regex matching is linear in pattern size × subject length (#4019). Deep recursion ends in a defined abort (C-196). | A regex whose program exceeds 20,000 VM states (a large counted repetition) falls back to the unbudgeted backtracker (#4083). |
| 10 | Path traversal, symlink escape | partly | File access needs the `IO` capability. `fs.open_root` and the `fs.root_*` family refuse a path that leaves the root by `..`, an absolute path or a symlink, with one err text on every target (C-392); the symlink refusal is the call itself — openat2 RESOLVE_BENEATH \| RESOLVE_NO_SYMLINKS on Linux, an O_NOFOLLOW segment walk from the root's descriptor elsewhere and in the stock-runtime fs service — so a directory swapped for a symlink mid-call is never followed (#4084). `[permissions] fs` in almide.toml bounds every fs call of `almide run` / `almide build` / `almide test` and of the embedded wasm host to the granted directories, symlinks resolved, and the p3 component host preopens only those directories instead of `/` (C-393, #4022). | `[permissions] fs` on native and the embedded host still resolves the path and then calls: a tree rewritten in between can race it (the p3 component's preopen bound does not race; the rooted family does not race). |
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
| 14 | A hallucinated or typo'd dependency | **open** | `almide.lock` pins each locked dependency to a commit. | `almide add owner/repo` fetches any owner's repository with no check and no prompt before the first fetch (`src/project_fetch.rs:334`) (#4093). |
| 15 | A dependency's code runs at build time | **open** | `@extern` is counted as every capability (epoch 8). | A dependency's `native/*.rs` is injected into the build, and `[native-deps]` crates run their `build.rs` on the developer's machine during `almide build` / `run` / `test`, outside `[permissions]` and with no grant (#4093). |
| 16 | A compromised CI action | partly | Release assets carry Sigstore attestations and sha256 checksums (#1534); the default workflow token is read-only; Dependabot covers the `github-actions` ecosystem. | Every `uses:` is a tag or branch, not a commit, and the repository does not require commit pins. `release.yml`, which can write and attest a release, is exposed to a moved tag (#4094). |

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
   `random.bytes` (#3938). Then the dependency grant (#4093), which needs a ruling on its options.

## How the page stays true

Each row's status changes only in the PR that lands its gate. A row moves to **closed** when the
mechanism is in CI and its negative control fails as expected; the PR quotes the control. The
claim at the top is stated in the README and llms.txt only when every row is closed.
