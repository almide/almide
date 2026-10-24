<!-- description: The security claim Almide is working toward — the vulnerability classes behind the October 2026 PHP and Go advisories, what makes each impossible in Almide today, the gaps with their issues, and the rule for when the claim may be stated -->
# Security by construction

> Last updated: 2026-10-23
> Parent issue: #4011 (memory safety with no gap). Related record: the stdlib redesign's R-L0-45 / R-L0-46.

## The claim

> A program written in Almide cannot be made to corrupt memory, run attacker-supplied code, or
> be steered outside the files and capabilities its manifest grants — whatever input it is given.
> Untrusted input can make it fail, but only with a defined error or a defined abort, within
> documented bounds on time and memory.

It may be stated without qualification only when every row of the table below reads **closed**:
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
| 5 | Memory errors through FFI or raw pointers | **open** | `@extern` is counted as every capability (epoch 8). | Safe code can read and write out of bounds today through `bytes.from_raw_ptr` / `copy_to_ptr`, even under `--profile critical` (probe on #4008). Fix: the stdlib redesign closes the surface to `ffi.read` / `ffi.write` behind `import ffi` (R-L0-45, R-ffi-01, #3853); critical refuses `ffi` and `@extern`, and builds record their use (#4008). |
| 6 | Data race corrupts memory | partly | Values are copied, not shared (C-033); fan work needs `T: Send` and Rc-backed values are not `Send`; E008; wasm fan is sequential. | The fan synchronisation itself is `unsafe` (lifetime-erasing transmute, `unsafe impl Sync`): Miri and loom over it (#4007, #4009). |
| 7 | Integer overflow becomes memory corruption | **closed** | Overflow wraps by contract (C-170, C-180) but never reaches memory: every index and size is checked after the arithmetic (C-067), allocation sizes saturate into the out-of-memory abort. `MIN / -1` aborts (C-002). | Silent wraparound is a correctness question, not a memory one; the checked / saturating / wrapping spellings are the stdlib redesign's R-L0-32/34 and R-int-12. |
| 8 | Null dereference | **closed** | No null in the language (`docs/SPEC.md:84`); absence is `Option`. Only `RawPtr` can be null and its bridge guards it (C-062). | — |
| 9 | Resource exhaustion from crafted input | partly | The HTTP server caps body, line, headers and time, serves through a 64-connection pool and answers a request head not whole within 10 s with 408, so a slow client holds only its own connection (C-389, #4021); the client caps response size. The zlib decoders stop at 256 MiB of output with one err text (C-390), and `json.parse` refuses nesting past 1000 levels (C-391) (#4020). Regex matching is linear in pattern size × subject length (#4019). Deep recursion ends in a defined abort (C-196). | A regex whose program exceeds 20,000 VM states (a large counted repetition) falls back to the unbudgeted backtracker (#4083). |
| 10 | Path traversal, symlink escape | **closed** on 64-bit Linux and macOS | File access needs the `IO` capability. `fs.open_root` and the `fs.root_*` family refuse a path that leaves the root by `..`, an absolute path or a symlink, with one err text on every target (C-392); the symlink refusal is the call itself — openat2 RESOLVE_BENEATH \| RESOLVE_NO_SYMLINKS on Linux, an O_NOFOLLOW segment walk from the root's descriptor elsewhere and in the stock-runtime fs service — so a directory swapped for a symlink mid-call is never followed (#4084). `[permissions] fs` in almide.toml bounds every fs call of `almide run` / `almide build` / `almide test` and of the embedded wasm host to the granted directories, symlinks resolved, and the p3 component host preopens only those directories instead of `/` (C-393, #4022); the scope's check only decides, and the call runs beneath the granted directory's held descriptor with the same walk, so a tree rewritten between the check and the call is refused by the call (#4084). `fs.walk` / `fs.glob` list a symlink as an entry and never descend into it (C-396, #4143), and the toolchain's `fmt` / `test` walkers follow no symlink and `fmt` rewrites a regular file whole, never through a link (#4140). `fs.create_temp_file` / `create_temp_dir` create a new private entry exclusively under an OS-entropy name (C-397, #4113). `path.from_string` admits only a path that stays inside its directory, on both separators, and `path.join_within` refuses an escaping child (C-398, #4146). | Path-based still, by what std exposes: an operation on a granted directory itself, and the metadata of an entry the process may not open. On other platforms `[permissions] fs` and the rooted family check and then call, and a rewrite can race them. |
| 11a | eval, dynamic code loading | **closed** | The language has neither. | — |
| 11b | Loose equality, type juggling | **closed** at run time | `==` is structural equality on one type. | `1 == "1"` passes `almide check` and is refused only by rustc / the wasm wall (#4018). |
| 11c | Deserialization that runs code | **closed** | `Codec` decode maps a `Value` onto typed fields; it creates no objects and calls no code (`stdlib/codec_decode.almd`). | — |
| 11d | Shell injection | **closed** | `process.*` takes a program and an argv list and spawns it without a shell (`crates/almide-rt-core/src/process_core.rs`). | — |
| 11e | HTML injection | **closed** | `SafeHtml` is opaque; `html.escape` is the only way in from a `String`, `html.raw` is the named opt-out. | — |

## Order of work

1. **Close class 5 first.** It is the only row where ordinary code can corrupt memory today. The
   stdlib redesign's `ffi` module (R-ffi-01/02, R-L0-45) lands ahead of the rest of its wave, and
   #4008 makes `--profile critical` refuse `ffi` and `@extern`.
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

## How the page stays true

Each row's status changes only in the PR that lands its gate. A row moves to **closed** when the
mechanism is in CI and its negative control fails as expected; the PR quotes the control. The
claim at the top is stated in the README and llms.txt only when every row is closed.
