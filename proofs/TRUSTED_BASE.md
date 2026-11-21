# Trusted base ledger (config management — tier-1 layer 7)

The flight-grade discipline: **naming exactly what is trusted is what lets the
rest be called "proven."** A reviewer reads this page to know the boundary. It
is the honest counterpart to the receipt's `C-PROVEN` claim.

## Toolchain pin

| component | version | role |
|---|---|---|
| Rocq / Coq | **9.1.1** | kernel + `coqchk` independent re-check (canonical, local source build) |
| OCaml | **5.4.1** | extraction target (until CertiCoq) |

Reproduce every claim: `make verify-trust` (or `proofs/check.sh` + `proofs/gate.sh`).

**CI cross-version note (honest).** The `Trust Spine` GitHub Actions workflow
re-derives the whole spine on opam's Rocq **9.3** — rocq-core pinned to 9.3.0
(`ROCQ_CORE_VERSION`, also the cache key), with rocq-stdlib 9.2.0 (opam has no
9.1.1; the canonical pin above is a local source build). The proofs are
kernel-checked and **axiom-clean on 9.1.1, 9.2 and 9.3** — a cross-version
re-derivation, not a single-version artifact (a strength, not a gap). 9.3
reserves `is`, so no proof binds it (#3368). Rocq 9.2+
ships only the `rocq` driver, so CI provides `coqc`/`coqchk` as thin shims over
`rocq compile` / `rocq check` (the latter IS the Rocq Proof Checker — the
independent De Bruijn re-check is genuine).

## Build and run toolchain (#4010)

The proofs below are about Almide's checkers and models. The bytes that run
are produced and executed by tools no proof here covers, so each is trusted
by name and version. `scripts/check-toolchain-pins.sh` keeps this list equal
to what the workflows and `Cargo.toml` actually use
(`scripts/toolchain-pins.txt` is the machine-readable copy). A version used
but not listed fails, and so does a version listed but no longer used.

| component | version | what it does | what checks it independently |
|---|---|---|---|
| rustc | **1.97.0** (`RUST_TOOLCHAIN`) | builds the compiler, and compiles the native leg's generated Rust | nothing proves it. The native leg is compared byte for byte with the wasm leg on the contract corpus |
| LLVM | the copy inside that rustc (the 22.1 series for 1.97; `rustc -vV` prints it) | native code generation | as rustc |
| rustc (Ferrocene) | **1.95.0** | builds the generated Rust of the run-manifest corpus with stock rustc at the recorded Ferrocene upstream pin (#573) | compatibility evidence only; the qualified Ferrocene binary is not used by this lane |
| rustc (stable, perf) | **1.99.1** (`RUST_STABLE_TOOLCHAIN`) | builds the handwritten-Rust side of the perf ratio in `perf-ratchet` and `perf-profile-arm`; nothing shipped is built with it | the ratio gates themselves; pinning it keeps their baseline still |
| rustc nightly | **nightly-2026-08-01**, **nightly-2025-06-01** | coverage instrumentation (branch coverage, MC/DC) and Miri over `almide-str`; nothing shipped is built with them | — |
| wasmtime (embedded crate) | **50.0.1** | `almide run` / `almide test` on wasm, the fan workers | the stock CLI runs below |
| wasmtime (stock CLI in CI) | **50.0.1** | runs built `.wasm` artifacts in the CI gates, one version everywhere | the runtime `tests/wasm_vm_parity_test.rs` (REQ-VM-9) compares `almide-wasm-vm` against |
| almide-wasm-vm | this tree | a second, independent wasm interpreter (standard library only, no `unsafe`) for the Critical-profile surface | REQ-VM-9: every `spec/wasm_cross` fixture inside its five WASI imports runs equal to the stock runtime, and to native when it passes `--profile critical` |
| OS, libc, CPU | the CI runners' and the user's | everything | accepted, not checked |

**What CI fetches (security row 16).** Every binary or archive a workflow
or CI script downloads is checked against a sha256 stated in
`.github/pinned-downloads.txt` (`scripts/fetch-pinned.sh`). This covers
wasmtime, cargo-nextest, wasm-pack, elan, the Lean toolchain and Node. A
first-party `latest` almide release is checked against its release
attestation instead. Every `git clone` checks out a 40-hex commit (codopsy),
the opam repository is pinned to a commit whose opam files carry each source
archive's checksum, and every third-party action is declared there with what
it fetches. `scripts/check-action-pins.sh` refuses anything else. Four
channels are trusted rather than pinned here:
- **rustup.** `dtolnay/rust-toolchain` runs it for an exact toolchain. rustup
  checks every component against the sha256 in that release's channel
  manifest, which static.rust-lang.org serves. The Rust release
  infrastructure is trusted, as rustc itself is.
- **apt and Homebrew packages** (binaryen, musl-tools, opam, wabt, valgrind,
  Vulkan drivers). These are checked against the distribution's signed
  package index, not against a digest stated here.
- **`actions/cache` entries.** Only this repository's own runs write them; a
  pull request's cache is scoped to its own ref. A cached release tarball is
  re-hashed by `fetch-pinned.sh` before use. An unpacked tree (the Lean
  toolchain under `~/.elan`, the codopsy binary keyed by its pinned commit,
  the opam root) is not.
- **Clones marked `# supply: head-tracked`.** In these the moving head is the
  thing being measured. The downstream canary tests each project at its
  default-branch HEAD: it is dispatch-only, its token is read-only, and the
  leg holds no secrets. The grammar drift check reads `almide-grammar` as
  data and never builds or runs it.

**Two engines.** wasm execution does not rest on wasmtime alone.
`almide-wasm-vm` shares no code with wasmtime, and REQ-VM-9 diffs it against
the stock runtime on the whole `spec/wasm_cross` corpus. Its scope is the
Critical-profile surface: an artifact that imports more than its five WASI
calls (fs, env, args) is refused at load, so for those artifacts wasmtime
remains the only engine.

**The embedded host is not a sandbox.** The core-module lane's
filesystem operations honor `[permissions] fs` grants; without that key they
retain access to the host filesystem. The p3 component lane
(`run_component`, `crates/almide-wasm-run/src/component_host.rs`) preopens
the granted roots, or the current working directory and temporary directory
when the key is absent, and inherits the environment. Wasm bounds the guest's memory
accesses, and the host bounds what the guest can make it allocate: one
linear-memory budget per run across all its instances, and every guest span
checked against memory before the host reads it (#4135). It does not bound
what the guest may ask the host to do. `docs/wasm/README.md` ("What the
embedded host grants") has the per-lane table. Filesystem scoping (#4022)
is implemented; it is not a general sandbox for every host capability.

**Floating steps (debt).** The remaining unversioned stable step is
`fuzz-nightly.yml:coverage-ratchet`, listed in `scripts/toolchain-pins.txt`.
Pinning it requires re-taking its measured baseline and deleting that ledger
entry. The two perf jobs use `RUST_STABLE_TOOLCHAIN`; see the table above.

**On a toolchain bump.** Raising `RUST_TOOLCHAIN` or a wasmtime version
changes a trusted component, so before it lands:

1. Read the upstream issues labelled `I-unsound` (rust-lang/rust) and the
   security advisories for wasmtime that are fixed or introduced between the
   two versions. Write in the PR which of them the generated Rust or the
   emitted wasm can reach. The generated Rust keeps its `unsafe` to
   `runtime/rs` and two compiler-emitted preludes (the region arena and the
   SIGPIPE setup, #4007), so a soundness bug in safe-Rust checking is the
   case that matters most.
2. Run the cross-target corpus and REQ-VM-9 on the new version. A difference
   is a finding, not noise.
3. Update `scripts/toolchain-pins.txt` and this table in the same PR. The
   gate refuses the PR otherwise.

**The user's rustc (#4110).** CI's pin says nothing about the toolchain on
the user's machine, and a std security fix reaches a native Almide binary
only if that toolchain has it (CVE-2024-24576: on Windows, `Command` running
a `.bat` / `.cmd` passed arguments through cmd.exe unescaped before 1.77.2,
which would re-open shell injection through `process.exec`). So every native
build (`almide run` / `build` / `test` / `bench`, the cdylib and install
routes) reads `rustc --version` before rustc or cargo runs, and refuses
anything below rustc 1.97.0 — `MIN_RUSTC` in `src/cli/rustc_floor.rs`, the
one place it is written: max(1.77.2, the `RUST_TOOLCHAIN` the generated code
is verified with). It is raised when a std fix Almide relies on lands; the
constant's comment lists the advisory each bump covers.
`tests/rustc_floor_test.rs` runs a fake rustc reporting 1.77.1 and one patch
below the floor (refused) and the floor (built and run), and checks this
paragraph and docs/specs/cli.md against the constant. Out of its reach: the
Ferrocene lane compiles emitted Rust with its own rustc directly, and a
binary already in the build cache is reused without a new check.

## The irreducible base (cannot be discharged by proof — "消えない底")

These four are trusted by necessity; everything else is proven against them:

1. **The Coq/Rocq kernel.** Decades of adversarial scrutiny; `coqchk` re-checks
   every `.vo` independently (the De Bruijn criterion). New logics have zero
   accumulated scrutiny — hence we borrow, not invent.
2. **OCaml extraction + the OCaml compiler — RETIRED as a trust root for the
   gate verdict (brick 6b, 2026-07-09).** The proven checkers still extract to
   OCaml (`ocamlopt`) as the FAST PATH, but every `gate.sh` row's verdict AND
   the entire corpus witness set are independently re-certified by the Rocq
   KERNEL itself: a generated assertion file inlines the witness bytes verbatim
   and `vm_compute` evaluates the proven checker inside the kernel-checked
   logic (`kernel_verify` in gate.sh; the batch oracle in corpus-wall.sh,
   measured ~4 min on the full 27k-object corpus). Binary/kernel divergence
   fails the build, and a TAMPER DRILL (a corrupted witness + a simulated
   divergent verdict, both caught) runs every build — so the Thompson hole no
   longer reaches a verdict: the kernel (item 1) carries it. Residual, honest:
   the extraction pipeline is trusted only for the convenience binary's
   correctness-as-fast-path (cross-checked), and `vm_compute`'s bytecode
   evaluator is part of the kernel's own trusted computing base as always.
   Upstream, verified extraction (MetaRocq, Rocq 9.1) and CertiRocq (the
   CertiCoq → Rocq 9.1 port, 2026) now EXIST — recorded as a future fast-path
   ratchet once the local (source-built 9.1.1, no opam) and CI (opam 9.2)
   toolchains align; neither is needed for the verdict.
3. **Hardware.** The CPU executes the machine code faithfully.
4. **ALS validity** — that the formal semantics captures the INTENDED meaning.
   This is the one item checked empirically (interp + dojo + use), never proved.

## Trusted representation invariants (gated, to be discharged)

A runtime fact that some reads rely on and that no proof covers yet. Unlike
the four items above, each row is meant to go: it names the issue that
replaces it with a checked certificate. Until then a gate keeps it honest, and
the row lists where it does not hold yet.

| invariant | the reads that depend on it | how it is kept | where it does not hold yet | discharge |
|---|---|---|---|---|
| **A String block's payload is valid UTF-8.** On the wasm leg, the bytes at `handle + 12`, whose length is at `handle + 4`, are well-formed UTF-8 (RFC 3629). Native Strings are Rust `String`s, and Rust keeps this invariant for them. | **U1**: these read continuation bytes after a lead byte and do not check the end: `__to_upper_cp` (stdlib/string_to_upper.almd), `__to_lower_cp` (string_to_lower), `__cls_cp` (string_class), `__trim_cp` (string_trim), `__ws_all` (string_first_last), `__jp_cp` (json_parse), `__rx_cp` (regex_engine). **U2**: in these, a lead byte's width moves a pointer or sizes a copy, unchecked against the end: `string.first` / `string.last` (`__cp_len` + `__copy_n`, string_first_last), `__pad_copy` (string_pad), `__rev_copy_n` (string_reverse; it writes each character at the output's tail), `__copy_bytes` (string_chars), `__rep_copy` (string_search; the empty-needle replace walks character starts), and the regex engine's `__rx_eol` / `__rx_bol` / `__rx_back` / `__rx_at_boundary` / `__rx_copy`. **U3**: these size an output block from UTF-8 widths: the case mappers' `alloc_str(blen * 3 + 8)` (string_to_upper, string_to_lower) and `__jp_utf8_write` (json_parse) into a span-sized block. | `proofs/utf8-producers.toml`, gated by `scripts/check-utf8-producers.py` (CI `checks`; `--self-test` is its kept drill). It ledgers every producer that can put bytes into a String block, and the row set is derived from declarations: the public stdlib signatures, the host op declarations, the self-hosted `prim.*` calls, the `prim.alloc_str` minting sources and the emitter's helpers. A new producer without a row fails. Each row either validates or builds by construction, and cites a test that feeds invalid bytes (`spec/stdlib/utf8_*_producers_test.almd` on both targets, `tests/utf8_host_boundary_test.rs`). | The ledger's `open` rows, shrink-only under `open_ceiling`. On the stock preview-1 shim, these hand raw host bytes to the guest: environment values, `env.vars`, argv, the working directory, the temp root, the temp creators' paths, and directory names (ops 10, 11, 21, 23, 26, 28, 29, 33, 39, 77, 122). The component shim's string http client (ops 43–49) stages the body unchecked. `fs.write_bytes`'s self-hosted body copies a Bytes payload into a String block internally (stdlib/fs_write_bytes.almd). | #4395 |

## Axiom ledger (the "Print Assumptions ⊆ standard" gate)

The spine has 164 audited theorems; `proofs/check.sh` fails when this number differs from the measured count.

Every theorem rests on **nothing but the kernel** — `Print Assumptions` reports
*Closed under the global context* for all of them (no `Admitted`, no extra
axioms). Verified by `proofs/check.sh`. The table itself is **claim-drift-gated**
(`check.sh`'s claim-drift gate, #34 / indicator ⑤): every theorem named below is
mechanically confirmed to be a constant the kernel re-checker (`coqchk`) actually
verified, so a public claim in this ledger can never drift past what is proven (a
fabricated row fails the gate). The table is a representative sample of the spine:

| theorem | file | assumptions |
|---|---|---|
| `check_sound` | OwnershipChecker.v | Closed under the global context |
| `check_all_sound` | OwnershipChecker.v | Closed under the global context |
| `check_cert_sound` | OwnershipChecker.v | Closed under the global context |
| `check_reuse_sound` | OwnershipChecker.v | Closed under the global context |
| `check_clc_unroll_sound` | OwnershipChecker.v | Closed under the global context |
| `eager_copy_refines_safety` | ALS.v | Closed under the global context |
| `mrun_tracks_exec` | RuntimeModel.v | Closed under the global context |
| `alloc_not_live` | FreeList.v | Closed under the global context |
| `region_window_reuse_safe` | FreeList.v | Closed under the global context |
| `region_reset_leaves_no_free_into_the_region` | FreeList.v | Closed under the global context |
| `pinned_stays_immortal_forever` | FreeList.v | Closed under the global context |
| `p_region_reset_preserves_PINV` | FreeList.v | Closed under the global context |
| `reuse_restores_rc_1` | FreeListRc.v | Closed under the global context |
| `reuse_hands_back_a_zero_count_block` | FreeListRc.v | Closed under the global context |
| `double_release_traps` | FreeListRc.v | Closed under the global context |
| `r_region_reset_preserves_RINV` | FreeListRc.v | Closed under the global context |
| `rc_dec_prog_realizes_rt_dec` | WasmRcDec.v | Closed under the global context |
| `rc_inc_bytes_encode_the_instruction_tree` | WasmEncode.v | Closed under the global context |
| `rc_inc_bytes_execute_to_rt_inc` | WasmExec.v | Closed under the global context |
| `rc_dec_bytes_trap_on_zero` | WasmExec.v | Closed under the global context |
| `rc_dec_bytes_frees_when_one` | WasmExec.v | Closed under the global context |
| `make_unique_yields_unique` | CowSafety.v | Closed under the global context |
| `check_fill_sound` | CallModes.v | Closed under the global context |
| `check_modes_cert_sound` | CallModes.v | Closed under the global context |
| `check_bc_unroll_sound` | OwnershipChecker.v | Closed under the global context |
| `check_xc_unroll_sound` | OwnershipChecker.v | Closed under the global context |
| `check_line_abort_sound` | OwnershipChecker.v | Closed under the global context |
| `check_xc_abort_sound` | OwnershipChecker.v | Closed under the global context |
| `check_xc_no_resurrection` | OwnershipChecker.v | Closed under the global context |
| `check_line_prefix_safe` | OwnershipChecker.v | Closed under the global context |
| `check_xc_drained_sound` | OwnershipChecker.v | Closed under the global context |
| `take_handover_sound` | OwnershipChecker.v | Closed under the global context |
| `probe_exclusive` | UniqueTake.v | Closed under the global context |
| `take_preserves` | UniqueTake.v | Closed under the global context |
| `retain_preserves` | UniqueTake.v | Closed under the global context |
| `release_walk_once` | UniqueTake.v | Closed under the global context |
| `take_then_release` | UniqueTake.v | Closed under the global context |
| `merge_subset_sound` | Subset.v | Closed under the global context |
| `subset_check_fast_sound` | Subset.v | Closed under the global context |
| `rc_inc_bytes_isa_effect` | WasmDecode.v | Closed under the global context |
| `rc_dec_bytes_isa_traps` | WasmDecode.v | Closed under the global context |
| `rc_dec_bytes_isa_frees` | WasmDecode.v | Closed under the global context |
| `memory_safe_iff_clauses` | MemorySafe.v | Closed under the global context |
| `check_trace_sound` | MemorySafe.v | Closed under the global context |
| `check_trace_complete` | MemorySafe.v | Closed under the global context |
| `rc_check_gives_temporal` | MemorySafe.v | Closed under the global context |
| `unguarded_rc_check_misses_resurrection` | MemorySafe.v | Closed under the global context |
| `unsigned_ge_excludes` | SpatialCert.v | Closed under the global context |
| `run_sound` | SpatialCert.v | Closed under the global context |
| `accepts_spatial` | SpatialCert.v | Closed under the global context |
| `unchecked_run_not_spatial` | SpatialCert.v | Closed under the global context |
| `check_bounds_cert_sound` | BoundsCert.v | Closed under the global context |
| `typed_core_memory_safe` | CoreSafe.v | Closed under the global context |
| `unchecked_index_not_safe` | CoreSafe.v | Closed under the global context |
| `certified_spatial_temporal` | Certified.v | Closed under the global context |
| `certified_with_bounds_cert` | Certified.v | Closed under the global context |
| `certified_memory_safe` | Certified.v | Closed under the global context |
| `shared_list_survives_mutation` | CoreSafe.v | Closed under the global context |
| `missing_inc_is_use_after_free` | CoreSafe.v | Closed under the global context |
| `box_frees_and_keeps_content` | CoreSafe.v | Closed under the global context |
| `box_store_without_share_is_use_after_free` | CoreSafe.v | Closed under the global context |
| `site_window` | InitCert.v | Closed under the global context |
| `sited_init` | InitCert.v | Closed under the global context |
| `sited_alloc_ok` | InitCert.v | Closed under the global context |
| `certified_sited_memory_safe` | InitCert.v | Closed under the global context |
| `reorder_memory_safe` | InitCert.v | Closed under the global context |
| `check_init_cert_sound` | InitCert.v | Closed under the global context |
| `fill_window` | InitCert.v | Closed under the global context |
| `rsite_window` | FieldCert.v | Closed under the global context |
| `ftrace_safe` | FieldCert.v | Closed under the global context |
| `fields_memory_safe` | FieldCert.v | Closed under the global context |
| `check_fields_cert_sound` | FieldCert.v | Closed under the global context |

## Known limitations (what is NOT yet proven — recorded, not hidden)

**What "memory safe" means here (#4013).** `MemorySafe.v` is the one
definition: over a heap trace (alloc / store / load / inc / dec, each naming a
block), every load and store is inside its live block (SPATIAL), nothing
touches a freed block and no allocation hands out a live one (TEMPORAL), every
byte read was written since allocation (INITIALIZATION), and every access uses
the block's representation (TYPE). A decidable trace checker is sound and
complete for it (`check_trace_sound` / `check_trace_complete`). Of the four
clauses, the spine's earlier theorems reach only TEMPORAL:
`rc_check_gives_temporal` derives it from the guarded RC check plus the
allocator's freshness (`FreeList.alloc_not_live`). SPATIAL, INITIALIZATION and
TYPE have no counterpart in any other file, except that `SpatialCert.v`
(#4005) proves a static bounds-check certificate checker sound for SPATIAL
(and TYPE) over the element reads and writes of fixed-length list blocks:
`accepts_spatial` — every run of an accepted certificate, at every loop count,
arm and reassigned i64 index, is `Spatial` — with the emitter's one unsigned
compare excluding negative indices (`unsigned_ge_excludes`). `BoundsCert.v`
puts the certificate's byte parser inside the proof and extracts
`check_bounds_cert` (driver mode `bounds`, `check_bounds_cert_sound`), and
the structural wasm leg emits it per build as the `bounds` witness (crates/almide-wasm/src/witness_bounds.rs) for every index-operator access — each `xs[i]` read and store of a list and each `b[i]` read and store of Bytes, the fact candidates of #3345 in their stretch and every other access under a fresh slot of its own — with the check recorded where the compare is emitted and the access where the load or store is emitted; almide-verify and proofs/structural-wall.sh judge it with the extracted checker and the kernel, and scripts/witness-reinject.sh bounds re-injects five lost-instruction bugs into the emitter and requires each witness rejected. Coverage on the certified corpus (trust-spine run 44227745821, 2026-11-18): 1390/1390 bounds witnesses of 1054 fixtures accepted, recording 2841 reads and 189 writes against 1095 emitted checks and 1470 loop exit tests in 501 non-empty witnesses; on the first measurement (trust-spine run 40085282713, 2026-10-24) 954/954, with 365 reads and 82 writes against 434 emitted checks in 93 non-empty witnesses — before this every-access recording, 5 witnesses were non-empty. Since #4011 (d) each bounds and init event also carries an ANCHOR into the shipped module, and `almide-anchor-check` (crates/almide-anchor-check — wasmparser's decoder and validator, no compiler code; itself trusted, with its accept/refuse unit modules and 15 byte-level drills, `scripts/witness-reinject.sh anchors`) requires every located event to be the instruction it names on every structural-wall artifact: a check an unsigned compare opening an `if` that aborts, an access a load or store, a site's allocation size and its stores' width, offset and address local with no branch out and no unrecorded store, a fill's exact countdown. Since sec-rows-2-3-4 (2026-10-27) the compare's OPERANDS are checked too (crates/almide-anchor-check/src/operands.rs): each located access is held to its slot's check over the shipped instructions — same index, `len / S` of the block it addresses in its own slot size, within one slot, the check dominating, and no change of the index or the list between (a write only by the list's own copy-on-write judge, whose calls the anchor file's `judges` line locates; no other call handed the list; no store of its count word) — the hoisted count and the payload pointer included, which also checks what the kills stood for; 2519 of 2519 located accesses on the 1054-fixture corpus (trust-spine run 44227745821), and the operand drills in `scripts/witness-reinject.sh anchors`. TRUSTED: that a located judge call is a copy-on-write judge (returns a block of its argument's count), that only a call handed a block shrinks it in place, the representation, that the anchor file's `alloc` index is `$alloc`, and every heap access outside the index operator and the string reads below (#4281), which no witness records. And
at the SOURCE level, `CoreSafe.v` (#4014) gives a typed core of Almide (ints,
bools, int lists with value semantics, `let`, assignment, element store,
`if`, `while`, indexing with its bounds check, `if` expressions and
recursive one- and two-argument functions, the two parameters distinct and
each argument given back in order when the call returns) a big-step semantics that
emits the heap trace — with reference counting: a shared read increments,
a consumed operand, an overwritten variable and a returning call's
parameter decrement, a block whose count reaches zero is freed, and freeing
a box (a block whose one slot holds any value) gives back what its slot
held — and
proves `typed_core_memory_safe`: every finishing or
check-aborted run of a well-typed core program is `MemorySafe`, all four
clauses, for any function table whose bodies are well typed. It is a core
(no closures, records, strings, multi-argument calls; a binding is released
when overwritten, not at scope end, so the rest leaks;
terminating and aborting runs only), and nothing ties it to the compiler's
lowering. `Certified.v` composes the two per-build certificates on ONE
trace: `certified_with_bounds_cert` — the guarded RC check accepting every
block's stream, a fresh allocator, layout-true allocations, and the trace's
accesses being a run of an accepted bounds certificate give `Spatial`,
`Temporal` and `WellTyped`; `certified_memory_safe` adds INITIALIZATION from
one more hypothesis, the constructor discipline (every allocation is
immediately followed by the store that initializes its whole extent).
`InitCert.v` (#4013) replaces that hypothesis and the layout one
(`alloc_ok`) with a per-build certificate: a construction site (an
allocation's header, slot width, slot count and representation, then the
slots it stores) is accepted when every slot is stored and none past the end;
`site_window` proves an accepted site's window MemorySafe and leaves its block
initialized over its whole extent; `sited_init` and `sited_alloc_ok` give
INITIALIZATION and the layout of every allocation in a trace built from
accepted sites; `certified_sited_memory_safe` composes the RC certificate,
the bounds certificate and the sites into `MemorySafe`, all four clauses,
with no hypothesis about initialization or layout left; `reorder_memory_safe`
proves MemorySafe invariant under swapping adjacent events on different
blocks, so a site's window may interleave with its elements' own events;
`check_init_cert_sound` puts the byte parser inside the proof (driver mode
`init`); `fill_window` extends it to a LOOP FILL — a block sized from a
run-time count and filled by a countdown loop — accepted when the loop counts
down the same local the block was sized from and its cursor advances by one
slot, and then MemorySafe and initializing at every count. The structural leg
emits the `init` witness per build for every list literal and every range
list (a loop fill) (crates/almide-wasm/src/witness_init.rs) and proofs/structural-wall.sh
judges it with the extracted checker, the kernel and almide-verify (on the
certified corpus, trust-spine run 44227745821: 3655 sites in 640 non-empty
witnesses, all accepted). Record, tuple and variant construction is recorded
by the fields witness below, not by this one; every other allocation site
(strings, Option and Result blocks, maps, the runtime's routines)
records nothing yet, so `sited` — every allocation a recorded site — holds of
a run only when all its allocations are list literals or ranges. TRUSTED: the recording
discipline, and that a site's block stays private until its last store. And
no certificate carries a trace yet, so the theorem the definition is for — `K(c, a) accepts ⟹ MemorySafe a`
for a shipped artifact `a` — is still open (#4005, #4006, #1696). Until it is
proven, "memory safe" is a claim about the trace model, not about the bytes.

**Record, tuple and variant fields (#4013 TYPE clause, #4279).** A field is
read and written at a CONSTANT offset with no check, so it is in bounds and
initialized exactly when its block was built with the layout the access
assumes. `FieldCert.v` adds the layout-typed construction site and the field
access as a third per-build certificate, `fields`: a layout table (per kind —
a record or tuple, or one case of a variant — its payload size and each
field's offset and width, a variant case's field 0 its tag word), sites that
must store every field of their kind, accesses that must name a declared
field, and tag reads that must be field 0 of every case. `rsite_window` proves
an accepted site's window MemorySafe and leaves its block of the layout with
every field initialized; `ftrace_safe` proves every event on a block of a
recorded kind MemorySafe in a trace whose allocations of such blocks are
accepted sites and whose loads and stores of them name declared fields of
their kind, given TEMPORAL; `fields_memory_safe` composes that with the other
certificates' clauses for every other block into `MemorySafe`;
`check_fields_cert_sound` puts the byte parser inside the proof (driver mode
`fields`). The structural leg emits the witness per build
(crates/almide-wasm/src/witness_fields.rs) for record, tuple and variant
literals and constructors, member and positional reads, a dying read's slot
take, pattern tests and binds and spread overwrites, and
`almide-anchor-check` holds every located event to the shipped bytes against
the SAME layout table: the allocation's payload size, each store's and each
access's width and offset, a variant site's tag store of its own case, and
each variant CASE field's read to a tag test of its own address that
guards it (an enclosing `if` whose condition implies `tag == C`, the address
the same variable or the same field re-loaded with nothing that may write
memory between; a call only when the callee's body writes none, a fixpoint
over the module). On the 1054-fixture certified corpus (trust-spine run 44227745821): 1054/1054
`fields` witnesses accepted by all three verdicts and FIELDS OK on every
artifact — 4338 sites / 9355 stores, 5882 field accesses (576 variant case
fields held to their tag test), 642 tag reads. TRUSTED: that a block's static type is its layout (the
representation, trusted already for the bounds and init witnesses; the IR is
cast-free and FieldAssign is refused),
and the one row #4279 adds (owner ruling, 2026-10-30):

> **TRUSTED (#4279): nothing rewrites a variant block's tag word after its
> construction window.**
> Scope: only the case a variant CASE-FIELD read relies on. Both ends are
> checked on the bytes: the construction site records its tag store as field
> 0 and `almide-anchor-check` requires that store to store the site's own case
> constant; every recorded case-field read must sit under a tag test of its
> own address for that case (`operands.rs` `case_guarded`). What is trusted is
> only the span between them — that no instruction stores to the tag word in
> between — because the heap model (`MemorySafe.v`) carries no values. Tag
> reads themselves need no such row: field 0 is the tag word in every case
> (`ITag`). The drill `case-field-unguarded` (scripts/witness-reinject.sh
> anchors) moves a case field's read above its test and must read ANCHORS
> REJECT.

The runtime's own routines over these blocks (typed drop, copy,
equality, comparison, display bodies) record nothing yet (#4278), and a
nullary case is a static pool block outside the allocation model.

**String bytes (#4281).** Every byte the structural leg's string surfaces
read in user code (`get`, `first` / `last`, `capitalize`, `to_codepoint`,
`to_list`, `reverse`, `lines`, `pad_*`, `starts_with` / `ends_with`,
`strip_prefix` / `strip_suffix`, `io.read_line`) is recorded in the
`bounds` witness, slot of one byte, with the test the surface ALREADY
emits — no test is added for the record, and the shipped bytes of the
1030 corpus and perf artifacts are identical to the merge base's or
smaller (1021 identical, 4 shrink by 27 B in all, 5 differ only in a local
index). No new event and no new Rocq clause: each test is a `c` or an `e`
of `BoundsCert.v`, and what makes it cover its read is checked on the
bytes by `almide-anchor-check` (operands.rs `covers`), one rule per fact:
- the surface's own `idx >=u len(s)` / `len(s) == 0` opening an `if`, the
  read (`s + idx` / byte 0) in its `else` — `c` anchored at the `if`;
- a byte loop's exit test `k >=u n; br_if` over `n = len(s)`, or over an
  `n` an enclosing test bounds by it (`n >u len(s)` or `len(s) != n`, the
  loop in its `else`): `k < n <= len(s)`;
- a suffix read at `(len(s) - n) + k` under the same two facts:
  `len(s) - n + k < len(s)`;
- the byte before a cursor, `s + k - 1`, under the exit test and a test
  making `k` positive (`k >u x`, then-arm): `0 <= k - 1 < len(s)`;
- a cursor WALKING DOWN, exit `p == 0; br_if` (by induction over the loop):
  on entry `p = len(s)` or, in the `else` of `len(s) == 0`, `len(s) - 1`;
  every write of `p` in the loop is `p - 1` after the exit test; the read
  is `s + p` or `s + p - 1` with the two offsets adding to at least one;
- a fresh block's own size: `i32.const N; call $alloc` (a `c` anchored at
  the call), the read inside its N bytes (a live block's allocation never
  changes size; its liveness is the ownership witness's).
Each reads `s`'s count unchanged from every `len` read to the access (the
existing count rule), and a loop-guarded read must lie inside its loop.
On the 1001-fixture certified corpus (GCE, e23e4ea28): 1001/1001 bounds
witnesses accepted by all three verdicts, recording 1083 loop exit tests
(429 before) and 850 emitted checks, and ANCHORS OK on every artifact with
1899 located accesses held to their guards (1018 before); on the
1054-fixture corpus of trust-spine run 44227745821, 1470 loop exit tests, 1095
emitted checks and 2519 located accesses.
The drills `string-test-one-byte-loose`, `string-loop-exit-loosened` and
`string-prefix-length-test-loosened` (`scripts/witness-reinject.sh
anchors`) each read ANCHORS REJECT, and 33 accept/refuse modules sit in
`crates/almide-anchor-check/tests/string_bytes.rs`. The runtime helpers'
byte loops in the same shape are recorded too (#4005,
`witness::bounds::helper_loop_exit`): the char count (`$str_len_chars`), the
codepoint offset's continuation walk (`$cp_off`), the UTF-8 check of
`bytes.to_string` (its lead byte) and `err_text`'s byte walk — each read `e N a N` on its loop's
exit `k >=u len(s); br_if 1`, the payload offset in the address (`s + 12 +
k` at memarg offset 0, which almide-anchor-check reads as the same byte as
`s + k` at offset 12). A helper body that does not ship records nothing.
Not recorded: the rest of the string routines' own bodies, runtime routines
owed to #4278's per-helper certificates — 21 byte reads in the emitter's
helpers (runtime.rs 4, runtime_str.rs 6, utf8_helpers.rs 4, kmp.rs 4,
value_helpers.rs, json_helpers.rs, map_index.rs 1 each; the empty
separator's split reads its lead byte after handing the string to
`$str_len_chars`, which the count rule refuses) and 120 `prim.load8` in the 24
self-hosted `stdlib/string_*.almd` bodies.

**The native leg (#4006).** For the programs the verified native renderer
renders (the grow-only `proofs/native-verified-floor.txt` subset of
`spec/wasm_cross`), `proofs/native-wall.sh` judges per build the ownership
certificate of every rendered function — taken from the exact MIR the
renderer rendered (`try_render_rust_source_certified`), so the probe charges,
the result-window rewrite and the make-unique elision are inside what is
certified — with the extracted checker, the Rocq kernel and almide-verify,
and pins the rendered Rust source's SHA-256. Its poison leg rejects the
witness of a render that lost a release with its Rust-mirror check bypassed
(`scripts/witness-reinject.sh native`). TRUSTED: that rustc compiles the
rendered source into a binary keeping that balance, and every program off the
floor, which ships from the standard codegen with no certificate.

Native nominal releases use the checked concrete native representation.
A runtime-backed value may be an owned Vec or Option/Result carrier rather
than a generated record, even when MIR names a synthesized nominal drop
routine. Rust releases that owned value recursively at scope end, as for
other typed drops. Borrowed/scalar/unknown shapes remain rejected, and the
closure drop retains its separate shape check. This reuses the existing
ownership certificate and trusted Rust realization; it adds no proof rule.
Unit tests refuse borrowed shapes, and source tests compare stdlib-owned
and shadowing user types through independent certificates and four routes.

Native HTTP request, response and call values retain their declared runtime
types, including the response's raw byte buffer and the call's shared
cancellation owner. Intrinsic argument/return signatures are checked against
those nominal types; a wasm string-list representation is not used on native.
Runtime type impls, including retained native-only TLS impls, travel through
the generated dependency index. Runtime behavior, Rust Clone/Drop and scoped
thread Send/Sync checking remain trusted. Guarded payload reads may move into
the common branch containing all their uses before ownership verification;
the renderer still checks the carrier tag at the read. This supplies no
fabricated default handle and introduces no new certificate rule. HTTP raw
byte/alias tests and the call lifecycle tests require the native v1 route.

Native foreign calls retain their selected Rust/C target and declared effect
class in MIR. Arguments use the existing borrowed-import ownership rule;
returned heap values are copied into a fresh native owned representation.
The adapters use checked types and the shared C ABI table, rather than
assuming wasm pointer layouts. FOREIGN (8) records the trusted boundary;
effect-declared calls also reach FOREIGN_EFFECT (11), propagated to callers.
These are the structural wasm leg's registry ids and the existing capability
subset rule. Host code, declared purity, ABI conversions and Rust realization
remain trusted; this does not prove a foreign library's memory safety.
Pure mutable foreign parameters use typed copies with the shared IR
write-back transformation. Borrowed callbacks use native typed dispatch;
their host signatures retain the program's value types, including shared
strings and Bytes/Matrix handles. Neither proves properties of host code.
Fallible mutable and nominal adapters, and every other foreign address,
still refuse the v1 route.

**A Region lent to an `@extern(rust)` call on native.** When a program
passes an `ffi` Region to an `@extern(rust)` fn, the native v1 render keeps
the Region's memory in the runtime's `AlmideFfiBlock`: one fixed-size `Rc`
allocation of `Cell<u8>` that every copy of the Region shares
(`native_region.rs`; `runtime/rs/src/ffi.rs`). The call passes
`(ptr, len)`: the Region's offset into its block and its length
(`native_foreign.rs`, `almide_rt_ffi_block_ptr`), the pair v0 and the wasm
leg hand their hosts for the same declaration. TRUSTED: while the call runs,
foreign code may read and write `[ptr, ptr + len)`; the block is fixed-size,
kept alive by the Region argument, and neither moved nor freed during the
call; the host stays inside that range and keeps no pointer after it
returns. Almide's own reads and writes of the block are bounds-checked safe
code. The ownership witness sees the block as an ordinary counted value. A C
binding, an export, a Region inside another value, a returned, written-back
or callback Region, and a host call that takes the address itself are
refused, so this is the one place a block's address leaves Almide. Target-priority tests use deliberately different fallback bodies so
an inliner cannot silently substitute them for the selected foreign call.

**Native test harness.** Checked test bodies and their reachable helpers enter
the ordinary native MIR pipeline, with separate internal function identities
and libtest display names. The certificate bundle binds the whole generated
Rust artifact, including its libtest wrappers, but the witnesses prove only
their modeled MIR properties. The wrappers, test selection, completion ledger,
per-file process isolation, global initialization and libtest's unwinding are
trusted, not proved by the ownership checker. Test assertion helpers retain
the frontend's diagnostic and source line. Raw `panic` messages pass through
a compiler-created typed identity helper; the original caller still contains
the checked Die operation. The renderer uses that provenance to unwind an
ordinary test and to preserve controlled termination in an abort-probe child.
It never chooses this behavior from a user's message prefix. Ordinary
controlled aborts and explicit process exits retain their termination behavior.
`compile_driver_native_tests` checks independent artifact-bound certificates
and compares native v1/v0 test verdicts, diagnostics, completion records and
abort-child stderr. The CLI harness suites cover filtering, merged compilation,
file isolation and refusal to count a premature exit as completed tests.
These tests do not establish full language coverage or a proof of Rust's
panic/unwind implementation; those remain separate acceptance obligations.

**Fan arms on threads (#4006 family H).** A `fan { … }` block of two or more
arms renders on the certified native path as the standard codegen renders it:
each arm on a scoped thread of the shipped fan runtime
(`crates/almide-codegen/src/prelude_fan.rs`, emitted verbatim), with one output
timeline per arm and a trap waiting for every arm below it (ADR-0024 D5/D6).
Each arm is lifted into a function of its own (`crates/almide-mir/src/fan_native.rs`)
that is lowered, verified and witnessed like any other — `proofs/native-wall.sh`
judges its ownership certificate — and the MIR that calls them is sequential.
TRUSTED: that running the arms of one block concurrently is observation-equal
to that sequential order, which is what the certificate judges. Its
preconditions, each checked by the lift (a program that fails one keeps the
standard codegen; `fan_native::tests` refuse each): every arm is a direct
call of a user function, or an expression of literals, operators, `if`,
`??`, pure module calls and direct calls, or a zero-argument scope returning
an explicit Result. Free immutable locals (no `var` reaches a threaded arm,
and no top-level value) become the lifted function's parameters;
every argument and result is of
a type that may cross a thread (scalars, `String`, and lists, options,
results, tuples and records of them; no function value); the program has no
metered region. Callback literals created inside an arm remain local to that
arm; captured free values still pass the same threadability checks. Native values DO carry
counts: a `String` is an `AlmideStr` block with a reference count, so every
heap value an arm receives is marked thread-shared before the spawn
(`almide_str_mark_reachable`, ADR-0013 design point 3), after which its count
changes only atomically. Also trusted: the fan runtime text itself and the
glue that routes this render's output and exits through it
(`crates/almide-mir/src/render_native_fan.rs`).
The fan runtime text includes the shared offer pool's gate
(`runtime/rs/src/list.rs`, `almide_fan_deal` .. `almide_fan_steal`), whose
`unsafe` is checked, not proven: Miri samples its schedules
(`scripts/miri-fan-gate.sh`) and loom enumerates its protocol's interleavings
within stated bounds (`scripts/loom-fan-gate.sh`). Until #4452 a helper held
the gate through a reference parameter, which the aliasing model keeps live
until the call returns, so a caller freeing the gate right after the last
helper's leaving decrement was undefined behaviour (Miri run 41854359018);
the helper now holds a raw pointer and leaves through the counter alone, and
loom models a reference parameter to the gate as read at its function's
return. Still trusted: aliasing-model rules other than that protector, which
loom does not model, and schedules neither sampled nor enumerated.

Element-wise fan calls whose callbacks the shared product parallel selector
keeps ordered are lowered through the ordinary certified self-hosted traversal.
That traversal runs every element and preserves the lowest-index error. Selected
parallel map sites keep a fresh registered wrapper and their literal callback;
the input is evaluated at the original call site before captured values. The
wrapper and the linked registry call identify the runtime site independently of
source spans or user-spelled prefixes. Its mandatory runtime adapter uses the
compiler builtin callback/result schema shared with frontend constraints for
fan.map, or the actual bundled declaration for list callbacks, and checks it
against the actual parallel runtime ABI. The shared product selector accepts
early module calls through their declared intrinsic and its existing runtime
pair table. Registered wrappers bypass sequential loop defunctionalization;
the native declaration fallback may retain the checked base registry call. The adapter records the
actual runtime symbol in the native-calls witness, marks typed closure captures'
reachable strings before the call, and emits the fan output/trap glue. Rust
rechecks Send/Sync at this boundary. A refused adapter cannot silently take the
sequential self-host body. Metered regions and mutable/global captures retain
walls; mutable-global reachability includes these wrappers. Higher-order list
calls inside fan blocks retain selected runtime sites, including a single arm.
Tests require each declared parallel list family member to appear exactly once
in the native runtime call records, distinguish ordered
callbacks from selected parallel sites, check registry reset/name collisions,
and compare captures, all-elements-on-error, first-error ordering and aborts on
four routes. Schedule equivalence and the runtime remain trusted as above.

The native runtime index carries a conditional function family only when it
has exactly two definitions with identical complete function heads and exactly
complementary `cfg(P)` / `cfg(not(P))` guards. Both guarded definitions travel
together; the compiler host never chooses the emitted target's branch. Runtime
scanning preserves existing `unsafe impl` qualifiers and distinguishes callback
bounds from the actual function parameter list. The native string header exports
the same reachable-string sharing hook the runtime's blanket `AlmideShare` impl
uses. A separate parallel-runtime harness closes dependencies under the actual
fan header, compiles them, and tests ordered output, every-element/first-error,
shared strings and trap prefixes in threaded and forced-sequential modes. This
isolated harness complements source-level certificates and execution comparisons;
it does not itself establish source coverage. Lifted callbacks' declared heap
Result signatures are included in carrier discovery, so a returned record is
kept as its typed Result rather than mistaken for a String-Ok allocation window.

Lazy fallback expressions inside Option/Result constructors are materialized
once through the normal binding path before wrapping their value. Nested Result
values delivered to an owned loop slot likewise use a fresh owned binding,
then the existing drop-old/SetLocal discipline. Native concatenation of the
remaining admitted heap-element families takes its recursive drop from the same
classification used by list construction. Both operands are owned snapshots,
evaluated left to right before the concatenation; a right-side slot replacement
cannot change the saved left operand. These reuse the existing ownership witness
rules and do not add a writable-borrow theorem. Execution comparisons exercise
lazy fallback effects, Result[(List[Value], Int), String] payloads, nested Map lists,
and aliases retained before loop updates.

Program heap slots are thread-local cells. They may coexist with threaded
arms only when a MIR reachability check, before slot rewriting, proves no
arm reaches a slot address. It follows named calls and function references
transitively and rejects unknown indirect/imported calls. This check is a
trusted rendering precondition, not a new certificate theorem. The negative
controls cover direct, transitive, closure and unknown accesses; a program
whose slots are used only outside its arms exercises the admitted case.

Native host I/O uses std's line-buffered stdout alongside the rendered
print shims. Raw stdout and stderr writes retain their bytes, including
invalid UTF-8; stdin reads and explicit prints retain their flush points.
In threaded fans both byte sinks join the same per-arm timelines as
formatted output. These sinks and the OS I/O implementations remain
trusted; the existing host-op capability accounting and ordinary call
witnesses cover their call sites. `tests/native_io_routes_test.rs` checks
raw bytes, text decoding, output ordering and traps against the ordinary
native and structural wasm routes.

Fan blocks classified for inline execution by the shared
`pass_fan_lowering::mark_inline_fans` decision retain that decision here.
Their ordinary MIR lowering settles all arms in source order before
propagating the first error; they do not use the threaded-arm lift. The
shared classification and the preservation of that evaluation order remain
trusted lowering steps, covered by the fan output/error regressions.

**Typed native cells.** Shared captured variables use the existing owned
cell operations: read an owned snapshot, take and release an old occupant,
then transfer the new occupant into the cell. The native representation is
`Rc<RefCell<Option<T>>>`, with `T` derived from the checked binding type.
Native-only `CellClass::NativeOwned` admits recursive occupants whose
release Rust performs by type; it does not admit a writable borrow across
a call or change the wasm cell layout classes. Ownership and bounds
certificates cover the MIR operations. The Rust representation and its
recursive destructors remain trusted, checked by execution comparisons.

**Native typed library bodies and aggregate values.** When no native floor
serves a call, a reached primary Almide library body takes precedence over
a representation-specific self-host implementation. It is checked,
monomorphized, namespaced and lowered through the same MIR path as user
code. Intrinsic calls retain their declaration checks and native-call
witnesses. This selection is a trusted lowering decision, not an additional
semantic theorem. Typed Result fields are constructed without fabricated
Default values; native moves retain a copy when a surviving symbolic
address still reads the source. Captured mutable parameters are copied into
owned local storage before the existing shared-cell lowering, while the
existing move-mode return carries changes back to the caller.

Native static global/member receivers of declared plain mutators are
copied into an owned temporary and published back immediately after that
call. Other arguments evaluate before reading the current receiver, as in
the ordinary runtime's cell borrow. Read-only global projections stay in
their operand scope, so sibling mutations are visible in source order.
Indexed mutation places and callback-bearing mutators keep their separate
admission checks. This uses existing copies, field updates and witnesses;
it adds no writable-borrow proof rule. Regression comparisons cover aliases,
consecutive pops, sibling reads, nested calls, branches and loops.

Literal defaults on admitted runtime declarations are materialized with
their declared type before native lowering. Integer tokens use the same
decoder as the frontend, preserving large literals exactly. Unsupported
default expressions retain the existing refusal. Float enum slots and
unconstrained generic scalar arguments preserve IEEE bits across the
native scalar/slot boundary. Generic comparison bounds cannot use that
conversion to compare Float bits as integers. These conversions remain
trusted rendering; the tests cover signed zero, NaN and finite payloads.

Nested native sequence patterns are normalized into typed length/tag tests
and guarded payload reads before MIR lowering. The subject is evaluated
once, then each arm's guard runs only after its shape matches. This
normalization is trusted semantic lowering; the ordinary MIR witnesses
still cover its operations. Differential tests include subject and guard
side effects and heap values retained after the match.
Boolean tuple refinements may omit a final wildcard only when bounded
enumeration covers every Boolean combination; other fields stay abstract,
and guarded rows cannot establish coverage. Disjoint Option tags can be
selected first, retaining payload-row order and guards. Tuple payloads in
native Option constructors retain their declared field types and recursive
Rust drop. These transformations add no independent semantic proof rule.

Bundled native modules retain their checked primary function bodies with
their nominal type declarations. The legacy self-host registry filter is
not applied to those bodies: mixing its String-based Path implementation
with the primary tuple-based Path declaration would violate the ABI.
Native floor calls still require their existing site checks and witnesses;
retained Almide bodies pass ordinary MIR verification. Differential tests
exercise path construction through folds, lookup, removal and retained
aliases; the linking and representation choice remain trusted lowering.

The declared native floor can carry the same embedded `almide_kernel`
module as the ordinary runtime. Its target-feature dispatch and cfg twins
travel intact, with no external kernel dependency or replacement scalar
implementation. Native-call certificates cover the declared matrix calls,
not SIMD machine-code semantics; the Rust kernels, their dispatch, and
rustc remain trusted. Matrix execution comparisons check observable
results, including the existing no-FMA and signed-zero cases.

Unconditional named runtime modules can also travel as whole scoped items,
with their nested cfg alternatives intact. Only a glob re-export makes
their members available as unqualified runtime names; unresolved parent
dependencies still refuse. This carries the existing OS entropy wrappers
from `random.rs` and `sys.rs` for `random.bytes`. The foreign implementation
and OS entropy quality remain trusted, with the existing `Entropy` host-op
accounting retained. Tests cover zero length, the 256-byte chunk boundary,
negative-length aborts and UUID v7's fixed observable properties, without
substituting deterministic bytes for the OS source.

**Native fs host calls (#4006 class E).** A program on the certified native
render calls the runtime's own fs functions as trusted host calls, as the wasm
leg calls its host's fs imports: the certificate covers how the program hands
each call its arguments and takes its result (borrowed arguments, a fresh owned
result, the call's capabilities counted), not what the host does on the
filesystem. The render carries those functions' code from the runtime's `fs`
module (`runtime/rs/src/fs.rs` with the rt-core files it includes), the text
the standard build compiles, each `#[cfg]` twin under its own gate
(`crates/almide-mir/src/native_floor_host.rs`). The named set, in witness-id
order: `almide_rt_fs_read_text`, `almide_rt_fs_read_bytes`, `almide_rt_fs_read_lines`, `almide_rt_fs_read_text_if_exists`, `almide_rt_fs_list_dir`, `almide_rt_fs_exists`, `almide_rt_fs_is_dir`, `almide_rt_fs_is_file`, `almide_rt_fs_is_symlink`, `almide_rt_fs_walk`, `almide_rt_fs_glob`, `almide_rt_fs___root_read_text`, `almide_rt_fs___root_read_bytes`, `almide_rt_fs___root_list_dir`, `almide_rt_fs___root_exists`, `almide_rt_fs_temp_dir`, `almide_rt_fs_write`, `almide_rt_fs_write_bytes`, `almide_rt_fs_mkdir_p`, `almide_rt_fs_remove_all`, `almide_rt_fs___root_write_text`, `almide_rt_fs_rename`, `almide_rt_fs_copy`, `almide_rt_fs_append`, `almide_rt_fs_append_bytes`, `almide_rt_fs_remove`, `almide_rt_fs___root_remove`, `almide_rt_fs_create_temp_dir`, `almide_rt_fs_create_temp_file`, `almide_rt_fs_read_bytes_if_exists`, `almide_rt_fs_read_lines_if_exists`, `almide_rt_fs_stat`, `almide_rt_fs_fold_lines`, `almide_rt_fs_for_each_line`, `almide_rt_fs_fold_lines_effect`, `almide_rt_fs_for_each_line_effect`, `almide_rt_fs_fold_lines_range`, `almide_rt_fs_fold_lines_chunked`. Every call to one is recorded as a host call in the program's
`native-calls` witness, and `NativeCalls.hosts_ok` (the extracted checker,
the Rocq kernel and almide-verify) refuses a host call outside the set; a
floor body outside the set that would carry fs code is refused by the render.
The runtime's generated Send-free twin is another implementation of its named
operation, with the same host witness id and capabilities. The native adapter
uses this sequential implementation when it cannot establish an effect-free
callback, preserving the accepted closure-value route's observable order.
The twin's signature and dependency closure are checked separately; its host
semantics remain trusted like those of the original runtime implementation.
Under `[permissions] fs`, both native generators use the same source-derived
scoped twin and path guards (`almide-codegen::fs_scope`). The certified render
includes their runtime dependencies before binding the generated artifact.
The named host identity and ownership witness are unchanged; descriptor-relative
path traversal and access refusals remain trusted host behavior, checked by
`tests/fs_scope_test.rs` and the runtime's deterministic symlink-race tests.
`tests/native_fs_host_calls_test.rs` holds this list, `host_ops::NATIVE_FS_HOST_CALLS`,
`NativeCalls.native_fs_host_calls` and almide-verify's copy to one list.
TRUSTED: the runtime fs code itself (tested on both legs by the cross-target
corpus, not proven). The optional raw-byte read extends this named set using the same host-call
contract as the optional text read: one borrowed path, a fresh owned
`Result<Option<Bytes>, String>`, and `FsRead`. Its runtime keeps NotFound as
None, raw bytes unchanged, and other errors labelled with the actual API.
The filesystem operation and those host error semantics remain trusted;
this does not claim a proof of the native filesystem implementation.
The optional line reader and metadata query extend that same named host
boundary: borrowed path, fresh owned Result, and FsRead. Optional lines keep
absence distinct from an empty file and other errors, using the bounded UTF-8
reader and its line semantics. Metadata maps the runtime record to the checked
four scalar slots (size, is_dir, is_file, modified), with booleans encoded as
0/1 and modified in Unix seconds. Field labels, types, order and the runtime
return type must agree. Filesystem behavior remains trusted in both additions.
The four streaming callback entries use the existing declared runtime adapter
contract: the path and captured environment are borrowed; a runtime-owned
accumulator and each fresh line are lent to the checked callback, which
returns an owned result. The host does not retain the environment or borrowed
arguments after returning. Its selected runtime symbol is recorded in the
native-calls host set, and capability reach includes FsRead and the retained
callback's transitive effects. Only the callback definition gets a static
adapter; the fs module and its platform gates remain intact. The runtime
keeps incremental reads, line-ending semantics and first-error termination.
Host IO and callback scheduling are trusted, not proved by the ownership
checker; callback bodies and call adapters still undergo their existing checks.
A program built under `[permissions] fs` takes the
standard build, whose fs calls the scope wraps.

**Native unix host calls.** (Owner ruling 2026-11-10, moved before rc1 on
2026-11-13.) A program on the certified native render calls the runtime's
own unix functions (Unix-domain sockets, descriptor passing and shared-memory
files) as trusted host calls, under the same contract as the fs class above:
the certificate covers how the program hands each call its arguments and takes
its result (borrowed arguments, a fresh owned result, `Net` counted), not what
the host does with the descriptors. The render carries those functions' code
from the runtime's `unix` module (`runtime/rs/src/unix.rs` with the rt-core
files it includes: `net_unix_core.rs` and its one sanctioned `unsafe` syscall
module `net_unix_sys.rs`), the text the standard build compiles, each
`#[cfg(unix)]` / `#[cfg(not(unix))]` twin under its own gate and the
descriptor table's gated impls with it (`crates/almide-mir/src/native_floor_host.rs`).
NEWLY TRUSTED here is nothing the project did not already trust: the embedded
wasm host serves this same core (`net_unix_core.rs`) as its interim ops
104..=116, and the standard native build compiles it unchanged. The named set,
in witness-id order after the fs set's: `almide_rt_unix___os_connect`, `almide_rt_unix___os_listen`, `almide_rt_unix___os_accept`, `almide_rt_unix___os_send`, `almide_rt_unix___os_recv`, `almide_rt_unix___os_close`, `almide_rt_unix___os_wait`, `almide_rt_unix___os_take_fds`, `almide_rt_unix___os_shm_create`, `almide_rt_unix___os_shm_resize`, `almide_rt_unix___os_shm_write`, `almide_rt_unix___os_shm_read`. Every call the render makes to
one of them is recorded by that id in the `native-calls` witness;
`NativeCalls.hosts_ok` accepts an id below the length of
`native_host_calls` (the fs set, then this set), and a floor body outside the
set that would carry unix code is refused by the render, as is a body that
would carry the other class's module. `tests/native_fs_host_calls_test.rs`
holds this list, `host_ops::NATIVE_UNIX_HOST_CALLS`,
`NativeCalls.native_unix_host_calls` and almide-verify's copy to one list.
TRUSTED: the runtime unix code itself (descriptor bookkeeping, `sendmsg` /
`recvmsg` / `poll` / `fcntl` system calls), tested on both legs by the
`spec/embedded_cross/net_unix_*.almd` and `net_huge_read_lengths.almd`
fixtures, not proven.

## FFI: an `@extern`'s declared purity is the author's claim

An `@extern(wasm, ..)` import is code outside the program, and the
capability certificate cannot see into it. Every call of one records
FOREIGN (`cert_project::cap::FOREIGN`), so the report always shows that
foreign code is reachable. Whether that code has host effects is what the
extern's declaration says: a plain `fn` extern records FOREIGN alone, which a
plain `fn`'s bound admits, as the checker does, and an `effect fn` extern
also records FOREIGN_EFFECT, which only an `effect fn` admits. The purity of a
plain `fn` extern is trusted, not checked. It is the author's declaration,
the same trust every FFI boundary takes. Pinned by
`witness_decls::tests::a_plain_caller_of_an_effect_extern_is_refused` and
`tests/verify_shim_test.rs::a_plain_fn_calling_a_pure_extern_builds_under_the_verifier_and_reports_foreign`.

## Native argument and iteration snapshots

Native tail-loop conversion stages changed arguments in source order as owned
MIR bindings before replacing carried slots. This includes cyclic heap carries;
Bytes result slots begin with a genuine empty Bytes value. A for-in source whose
place may be written by its body is similarly captured once, using the shared
call-write analysis for direct assignments, globals and closure captures. These
copies and releases remain ordinary MIR ownership events. The native map view
uses an entry count and separate key/value windows; wasm retains the physical
map layout selected for each key/value pair. The native renderer and this view
mapping remain trusted code, not a new source-to-Rust equivalence theorem.

Library-local calls retain their declaring module even when the chosen runtime
route excludes the library body. Generic runtime constructors bind result-only
types from the checked source call; empty string lists keep that type through
joins. Float32 library arithmetic explicitly promotes its binary32 operands and
demotes its result once. Targeted certificates and execution comparisons cover
these changes; full-corpus acceptance is still required before raising coverage.


Synthesized equality functions retain their checked operand type on both MIR
parameters. Native list-of-tuple comparisons call the typed field comparison
helper rather than interpreting tuples as wasm list storage. A generic runtime
call specialized consistently to Float by its registered declaration converts
MIR Float bits to runtime f64 values; comparisons therefore preserve signed-zero
and NaN semantics. These declaration and representation mappings remain trusted
renderer logic. Independent ownership certificates and differential execution
checks do not establish a general semantic-preservation theorem for the mapping.


Native narrow exponentiation fuses a sole-use `int.pow` result and its declared
width rewrap into a scalar operation. Multiplication stays at that width; the
exponent and the negative-exponent abort are preserved. This is trusted numeric
lowering, with no ownership events and no new ownership rule. Native fan arm
capture checks recurse into maps, sets and variant fields as well as records;
function values remain excluded from threaded captures. Eligible arms retain
the scoped-thread runtime, with reachable strings marked before spawning.

## Use-relativized completeness

Completeness is declared per model and use, not absolutely. The historical
eager-copy result concerns its ownership fragment; it is not the shipping
structural runtime's current completeness boundary. The trace-model theorems,
runtime refinement and per-artifact certificate checks have distinct
hypotheses and coverage. None is a proof of full source-to-artifact semantics
or of `MemorySafe` for every shipped program. The receipt names the claims
and remaining trusted assumptions for its certified artifacts.

The native call-operand snapshot uses that same write analysis to bind an
affected argument prefix in source order before a later operand can change
it. Block arguments use ordinary MIR bindings, including scalar copies and
owned heap shares. Structural-to-named aggregate conversion also applies to
owned parameters; it rebuilds the declared representation without admitting
borrowed arguments to move parameters or equating distinct nominal records.

Loop guard return normalization preserves one result and an exit flag through
nested statement blocks, branches and loops. A set flag suppresses following
statements and further while-condition evaluation. This normalization remains
trusted semantic lowering; independent ownership certificates check the MIR
it produces, not the correctness of the source transformation. The regression
checks compare native, v0, structural wasm and interpreter observations,
including guard exits before later effects and argument rebinding order.

Unit fields and list elements retain the MIR zero-valued lane; a Unit function
return remains void. Native runtime Map arguments copy matching Option/Result
carriers and do not require a fabricated Default for aggregate keys or values.
Map equality helpers now use typed entry iteration, lookup and value equality;
Set helpers use cardinality and subset operations. Native nested Set equality
uses these same declared runtime operations, restricted to recursively admitted
Int/String/Bytes/List/Set representations and aggregates whose declared fields
have the same equality. The runtime equality contracts and
semantic lowering remain trusted; independent certificates cover the resulting
MIR ownership and call boundaries, not full source equivalence.

Native collection routing retains checked key and member types before wasm
storage normalization. Structural-to-named keys and Set inputs are rebuilt by
their checked field mapping; distinct nominal records remain distinct. Tuple
ordering is admitted only for source fields whose order agrees with the native
representation. Named records and variants additionally require an explicit
`Ord` derivation, including every named node in a recursive comparison. Region
handles, floating bit lanes and unsigned order remain excluded from this generic
route. Float sort/min/max instead name their typed runtime declarations, and
Float sort keys use the canonical total-order transform. Both sort-by runtimes
evaluate a key once per element, including a singleton. These are trusted
representation and runtime semantics, tested against the other execution routes.

A scalar Option join is ownership-neutral only when both incoming arms were
rewritten to value-only Option constructors. Its copies then use `OptCopy`,
and no heap release is emitted for the value-only join. An arm with a call's
ownership credit prevents that classification. The ordinary ownership checker
and independent certificate rules are unchanged.

Result payload binding retains the checked tuple/list read shape and recognizes
its existing recursive drop descriptor when borrowing the Err payload. The four
heap-and-Int tuple Result families accept an already-owned payload copy as well
as a literal. Wrapping consumes that copy and preserves the original binding;
its recursive drop descriptor stays attached to the Result. Constructor retries
must change the expression shape: a variable payload is never rebound into
another identical retry. These are lowerer fixes, not additional proof rules.

Native typed display expansion includes sized integer/Float lists and Bytes
lists, keeping UInt64 unsigned digits and Float32's declared rounding. Sized
integer Set members use canonical integer lanes with bit-exact equality; this
does not admit floating-point Set/Map keys through integer equality. Nested
integer-list and Bytes comparisons use their existing declared equality bodies.
Result comparison emits an explicit guard for each operand before reading either
payload. The resulting MIR goes through the unchanged ownership and call checks;
formatter equivalence and the source-level comparison transformation remain
trusted and are tested across the four execution routes.

Only minted scalar Options lose their Consume during the native value rewrite.
Call-returned Options retain their transfer credit, and a declared heap Option
keeps its handle even when this frame only stores/passes it. Checked String-list
allocations retain that type when payload reads inside a Result arm cannot be
inferred by the flow-insensitive slot prepass. Optional-chain heap fields take
an owned snapshot within the selected arm before wrapping it in Some; the arm
frees its other temporaries and transfers that snapshot through the existing
ownership rules. No new proof rule or fallback exemption is introduced.

Native value-producing branches can terminate the process instead of yielding a
value. The producer emits the existing format-v6 `t` event only when a MIR
`Die` or `ProcExit` primitive immediately precedes a valueless `Else` or
`EndIf` belonging to a value-producing join. A type annotation or ordinary
function call is not evidence of termination. The surviving branch must still
balance its ownership, and invalid accesses before termination remain rejected.
The native renderer takes the join type and value only from a surviving arm.
This uses the existing abort checker rule; it adds no ownership rule. The
compiler's association of this event with process termination and the generated
Rust's semantics remain trusted, rather than proved by the trace checker.

The range and chunked line folds use the runtime's existing generic `Fn`
parameter directly after checking its full argument and result signature.
The original runtime generic constraints, including chunked Clone/Send/Sync,
remain in the generated Rust. Range ownership of lines, byte partitioning,
worker scheduling and IO behavior remain trusted runtime semantics. The
selected original host symbol is retained in the native-calls witness.

Native borrowed tail calls may be rendered as parameter rebinding and a loop
when the MIR frame has no owned parameters, moves, allocations, or reference-count
changes, and the self-call's result flows through every enclosing join to
the actual return. Arguments are staged before any parameter is changed and
must keep their exact borrowed native types. The MIR and its call witnesses
remain unchanged. A branch-local variant tag fact, including equality between
two input tags, permits borrowing the actual payload without a temporary
Default value. Facts are scoped to the selected arm and the declared variant
case must contain the requested slot/type. No writable-borrow rule is added.
These native transformations and their correspondence to the checked MIR are
trusted code generation; the trace checker does not prove Rust semantics or
constant stack usage. A certified source regression exercises recursive
comparison, NaN, signed zero and retained inputs; a generated-code harness
checks 100000 levels at opt-level 0 on a 128KiB stack, isolating destruction.

Typed errors coerced to a String failure channel use ordinary typed display
helpers prepared before MIR lowering. A compilation-scoped table connects the
later propagation desugar to those helpers, including recursive variant and
scalar-carrier payload types. The helpers are lowered, ownership checked and
witnessed like other functions. This is not a proof exemption for formatting;
observable text is checked against v0 and the other supported execution legs.

Native Float32 Map keys/values and Set members use Rust `f32` at the runtime
container boundary, with bit-preserving conversion from and back to MIR's
binary32 lane. Lookup and insertion therefore preserve signed-zero equality
and NaN non-reflexivity; iteration keeps insertion order and returns lane bits.
This admission is for direct Float32 elements, not arbitrary nested float
fields whose Rust representation remains integer lanes. Float32 map equality
expands into ordinary typed lookup/iteration IR, preserving operand order.
The existing MIR witnesses remain checked; these boundary bit casts and the
runtime container implementation are trusted code generation/runtime, not a
new theorem about Rust floating-point semantics. The certified regression
covers zero, NaN, retained values, updates, removals, iteration and equality.
Its native/v0/WASI observations agree. The extra regression's interpreter
route explicitly refuses a Float collection scalar core, so that route is an
abstention rather than an equality result; the fixed corpus is unchanged.

## Retained history

The brick-by-brick progression of the MIR certificate pipeline and the retired
incumbent wasm renderer, the 2026-07-03 proven-vs-trusted boundary map of that
pipeline, and the five trusted-zone bug classes of that day are in
[TRUSTED_BASE-HISTORY.md](./TRUSTED_BASE-HISTORY.md). They are dated evidence,
not current guarantees. The current boundary map is
[docs/contracts/proven-vs-trusted.md](../docs/contracts/proven-vs-trusted.md).
