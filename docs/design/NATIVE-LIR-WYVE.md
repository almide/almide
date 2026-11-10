# Native LIR and the Wyve scheduler

Status: design for owner review, 2026-11-07. No LIR or scheduler implementation
is authorized by this note. The first requested decision is whether to build
the scalar fib slice described below, after reviewing its ABI and trust boundary.

## Problem and scope

The native build handoff measured fib at 285 ms after four optimizations,
against handwritten Rust at 45 ms. Those are historical measurements, not a
measurement of this design. Removing repeated runtime scanning and parsing
improves the existing route; bypassing rustc's frontend requires a separate
backend with explicit semantic and safety obligations.

The proposed common path is implemented in Rust:

```text
checked frontend → MIR → LIR → LLVM → object → lld → executable
                         │                    ↑
                         │          prebuilt Rust runtime staticlib
                         └ numeric loop nests
                              → Wyve schedule + contract checker
                                (Almide source, generated Rust stage0)
```

The existing rustc route remains available until the new route meets its
coverage and observation contracts. The shipped wasm route is currently the
structural emitter (`src/wasm_route.rs`, `crates/almide-wasm`), not the retired
MIR wasm renderer. This proposal does not reroute wasm or depend on that
retired renderer. Numeric scheduling could later be shared with wasm through
an explicit adapter, subject to its own verification.

Racket and Lean remain development/reference tools. Neither runs in an ordinary
Almide build. Only programs containing eligible numeric nests call the generated
Wyve scheduler; generic lowering and contract validation stay in Rust.

## LIR representation

Use typed SSA values, basic blocks with typed parameters, and explicit
terminators. A module carries target triple, data layout, runtime ABI version,
source locations, and a deterministic proof table. Suggested Rust components
are `LirModule`, `LirFunction`, `Block`, `Value`, `LayoutId`, `RegionId`, and
`ProofRef`; these are proposed names, not existing APIs.

Follow `crates/CLAUDE.md`'s layout discipline: flat tables, `u32` ids and
interned atoms, with a layout-ledger entry for new persistent formats. Blocks
and instructions refer to table ranges instead of owning pointer-rich trees.
Keep strings in an intern table and use separate columns for frequently scanned
properties where measurements justify them.

| Area | Proposed representation |
|---|---|
| Scalars | unit, bool, fixed-width signed/unsigned integers, f32, f64; language Int remains i64 |
| Aggregates | records, tuples and tagged variants with an explicit layout id; no implicit Rust layout |
| Memory | opaque managed handles and typed views with region, length, alignment and access mode; raw addresses only inside checked lowering/runtime adapters |
| Functions | typed direct calls; closure code pointer plus managed environment; explicit ABI and parameter ownership modes |
| Computation | constants, comparisons, wrapping/checked arithmetic, conversions, select, aggregate construction/projection |
| Control | branch, conditional branch, switch, return, and explicit language abort; block arguments become LLVM phi values |
| Storage | allocation, retain, release, checked load/store, borrow begin/end, region enter/leave; bounds and initialization facts name exact sites |
| Calls | program call, runtime call, host call; each has effects, transfer modes and error behavior |

Arithmetic ops name their Almide semantics. Wrapping arithmetic must not gain
overflow assumptions; divide, remainder, shifts and conversions retain the
current checks and failure observations. Floating-point operations preserve
the specified order, signed zero and NaN behavior. FMA, reassociation and
other relaxed operations require their own applicable contract.

The text form is deterministic and versioned. It is a debugging/test format,
not a new user language. Example syntax (illustrative):

```text
lir 1 target "x86_64-unknown-linux-gnu" runtime_abi 1
fn @fib(%n: i64) -> i64 {
  ^entry:
    %small = icmp.slt %n, 2
    condbr %small, ^base, ^rec
  ^base:
    return %n
  ^rec:
    %a = sub.wrap %n, 1
    %b = sub.wrap %n, 2
    %x = call @fib(%a)
    %y = call @fib(%b)
    %z = add.wrap %x, %y
    return %z
}
```

A separate verifier checks dominance, edge types, layouts, ownership paths,
call ABI, initialization, bounds and proof references before LLVM emission.
Verify again after each transformation that can change those facts. Stable
ids bind contracts to the actual module/function/op and compiler/checker
version, preventing a certificate for an earlier IR from licensing later code.

## Contracts and the missing-proof rule

The five Wyve contract classes are internal facts derived from the program;
users do not supply LLVM promises.

| Class | Evidence and derivation | Missing or insufficient evidence |
|---|---|---|
| Alias: owned, borrow, noalias | MIR ownership and native-call witnesses supply transfer/balance facts. Add an independently checked region/disjointness fact over the relevant access lifetime before deriving noalias. Account for nested handles, shared buffers, escapes and indirect calls. | Keep ordinary aliasing accesses when safe; reject a transformation requiring disjointness. RC balance alone never grants noalias. |
| Effect: pure, reads/writes | Combine language effect inference with a transitive LIR memory-effect summary, including allocation, refcounts, destructor behavior and runtime/host calls. | Use conservative effects; reject a required reordering whose observations cannot be preserved. |
| Layout: repr, align, soa | Checked layout table, allocator guarantees, target data layout and bounds/init evidence; SoA also needs a representation-equivalence argument for every consumer. | Keep original layout or reject unsupported ABI/layout. Never infer alignment from a convenient vector width. |
| Control: vectorize(require), unroll, fp | Loop bounds, dependence and effect checks, exact schedule theorem instance, and target support. Distinguish iteration reordering from changing a reduction's operation order. | Optional optimization is skipped. Required optimization fails with the missing obligation. |
| Emission: expect_ir, target_feature | Validate the produced LLVM/module/object properties against a named expectation and the selected target's feature set. | Required expectation fails the build; runtime dispatch needs a verified fallback and matching ABI. |

Missing evidence for an optional attribute removes that attribute. Missing
evidence for safe lowering is a named unsupported case; a contradicted
certificate or malformed generated IR is a compiler defect, never silently
treated as success. An explicit experimental LIR selection must report the
unsupported case, rather than hide it behind a rustc execution counted as LIR.

LLVM attributes have stronger meanings than language labels. Parameter
`noalias` constrains accesses during a call; `memory(none)` concerns memory
effects, and fast-math flags permit specific semantic relaxations. Emit them
only after proving those exact obligations. See the
[LLVM parameter attributes](https://llvm.org/docs/LangRef.html#parameter-attributes),
[function attributes](https://llvm.org/docs/LangRef.html#function-attributes), and
[fast-math rules](https://llvm.org/docs/LangRef.html#fast-math-flags).

Existing witnesses do not establish complete artifact memory safety or correct
lowering. Preserve the distinctions in
[proven vs trusted](../contracts/proven-vs-trusted.md). New LIR checkers,
MIR-to-LIR translation, ABI wrappers, LLVM, lld, and instruction recording need
explicit trust/evidence entries; replacing rustc does not inherit its checks.

## Runtime ABI and linking

Build `almide_rt` once per supported target as a Rust staticlib with a versioned
C interface. Existing Rust runtime signatures are not automatically that
interface: `Vec`, `String`, `AlmideString`, generic results and closures must not
cross as their private Rust layouts. Use opaque handles, fixed-width scalars,
and generated `repr(C)` descriptors; publish ownership and lifetimes per call.
The same manifest generates Rust wrappers and LIR declarations and is checked
for size, alignment, calling convention, symbol and transfer-mode agreement.

| Boundary group | Calls that must be covered |
|---|---|
| Initial fib slice | integer formatting/output, process entry/exit and language abort |
| Managed storage | allocation/deallocation, retain/release, copy-on-write uniqueness, region save/restore, OOM |
| Values and collections | string/bytes creation and conversion, list/map/set operations, record/variant/closure storage, equality/hash and nested destruction |
| Numeric helpers | defined division/conversion edge cases, formatting, mathematical functions, kernel entry points |
| Host operations | stdin/stdout/stderr, args/environment, filesystem, time/randomness, process/network and concurrency, according to the existing capability/host contracts |

For the initial slice, proposed symbols are `almide_lir_print_i64(i64)`,
`almide_lir_abort_utf8(ptr, len)` and a process-entry adapter calling the LIR
entry function. These names are provisional; none exists yet. Before widening,
generate an exhaustive manifest of actual runtime calls reachable from the
accepted corpus. Every such call must have an ABI row; unknown symbols wall.

Concrete adaptation points in the existing implementation are:

| Existing Rust operation | Proposed C-facing adaptation |
|---|---|
| `rt_println_fmt(std::fmt::Arguments)` / `almide_v1_line(&[u8])` in `render_native_shims.rs` | scalar/UTF-8 output wrappers; format and line buffering remain in Rust, never pass `fmt::Arguments` across the ABI |
| `almide_rt_int_to_string(i64) -> AlmideString` in `runtime/rs/src/int.rs` | return an owned string handle; fib can use the combined print-i64 wrapper to avoid a temporary handle |
| `almide_abort` / `almide_abort_oom` in the native floor header | UTF-8 language-error wrapper and dedicated OOM wrapper, preserving stderr and exit behavior |
| `rt_chk_div`, `rt_chk_mod`, `rt_chk_div_u`, `rt_chk_mod_u` | explicit LIR checked ops with equivalent error calls; retain a wrapper where duplicating edge semantics would create another source of truth |
| `almide_rt_map_get`, `almide_rt_map_set`, `almide_rt_map_len` in `runtime/rs/src/map.rs` | layout-specialized opaque-map entry points; optional payload/status is explicit, generic Rust values stay inside Rust |

This is the initial boundary inventory, not a claim that the remaining runtime
already has a C ABI. Allocation and ownership wrappers need to be designed
against the chosen handle representation before heap-value support is enabled.

Returned errors use explicit tagged status/out parameters or managed handles.
Rust panics must not unwind through a C boundary. Preserve language abort text,
stderr destination, exit code and output flush behavior through wrappers;
`llvm.trap` is not a substitute for a language error with observable output.
Lists of nested handles require an element-layout/destructor descriptor.
Flat numeric views avoid repeated nested-to-flat copies and remain borrowed
only for the checked duration of the kernel call.

Prefer LLVM's in-process API from Rust for the measured prototype, pinned to
one LLVM version and optimization pipeline. Record target CPU/features, LLVM,
lld, runtime ABI/content hash and sysroot in the build/cache identity. Emit an
object, then invoke lld with the target's startup objects, runtime archive and
required platform libraries. Do not assume an object plus a staticlib forms a
complete executable. Begin on Linux x86_64; other targets need their own tested
link manifests. Changing any identity component invalidates cached artifacts.

## Wyve proof scope and stage0

Reference inspected: Wyve commit
`fa5bda4fef131d7e531921184399c96ef9ed802d`, especially `docs/TRUST-MODEL.md`
and `proofs/Wyve/{Tile,Interchange,Parallel}.lean`, alongside the README,
DESIGN, STRATEGY and v2 Almide roadmap requested by the handoff.

The sum-reordering theorems require `AddCommMonoid` (and the matrix theorem
uses `CommRing`). IEEE floating-point addition does not meet the associativity
assumption. These proofs therefore do not by themselves prove bitwise equality
for an arbitrary floating-point reduction. `Parallel.write_comm` instead proves
commutation of writes to distinct locations with given values; the compiler
must still establish those locations and the independence of their values.
Tiling proofs over `M * B` also require a justified remainder path for general
sizes. These are concrete obligations, not reasons to abandon the architecture.

Initially allow schedules preserving each output's exact floating-point
operation sequence, with checked independence of outputs. For a reduction,
prove an appropriate ordered machine-arithmetic statement or leave its order
unchanged. A tolerance comparison or a different tree-order reference cannot
replace Almide's existing byte-observation contract. The `almide-kernel`
reference/optimized differential pattern remains useful evidence, with its
coverage and numerical equivalence relation stated per kernel.

The proposed stage0 gate records the Almide scheduler source, generated Rust,
bootstrap compiler commit, generator flags and proof/reference commit. In CI:

1. Regenerate with the pinned bootstrap on GCE and compare generated Rust
   byte-for-byte, failing on drift; generation must be deterministic.
2. Run reference and generated scheduler on the same serialized loop/contract
   cases; compare acceptance, rejection reason and normalized schedule.
3. Execute accepted schedules against the unscheduled reference. Check exact
   bits where required, including edge sizes, empty dimensions, remainders,
   overlapping views, NaNs, infinities, signed zero and cancellation.
4. Run the Lean proofs with pinned dependencies and record which theorem and
   hypotheses each accepted transform instantiates. Differential testing does
   not prove the translation from the Lean model to machine instructions.
5. Mutate dependence, bounds, alignment and effect facts and require rejection.
   Validate after composing passes, not just each isolated pass.

This makes the source/checked-in Rust seam reviewable and holds the Rust
implementation to a reference without starting Racket, Lean or wasm per build.

## Acceptance and rollout

After owner approval, first implement only integer fib: branches, recursion,
wrapping integer arithmetic, C runtime output and a direct lld link. Verify
original-input stdout, stderr and exit code against the rustc route before
timing. Report parsing/lowering/checking, LLVM, linking and end-to-end time,
including startup and artifact hashing. The handoff's 35–50 ms is an
unverified estimate; 10–20 ms LLVM and 11 ms linking leave an unknown frontend
and runtime-interface cost. A prototype is needed before claiming rustc parity.

Measure on the same GCE Spot VM as rustc: prebuild the runtime/dependencies,
warm up separately, alternate seven measured builds with a fresh IR-reaching
constant every time, publish raw samples and medians. Never execute the
large-constant build-time inputs. Separately report first-build/runtime-cache
cost and original-input runtime speed; preserve the eight-benchmark runtime
comparison before expanding the default route.

Then expand by explicit coverage groups: scalars/control, owned heap values,
collections/closures, host calls, and numeric scheduling. At each step run
the full `spec/wasm_cross` inventory against the rustc route under matching
inputs/hosts, comparing stdout/stderr/exit status using the contracts'
nondeterminism rules. Record unsupported rows separately; neither a skip nor a
rustc fallback is a passing LIR row. Require full corpus coverage before a
default-route change, plus `almide test`, ownership/call/bounds negative cases,
ABI checks and the relevant differential fuzz. A finite corpus remains finite
evidence, not a proof for every program.

New observable behavior must follow the contract-ledger process, including the
ALS-first rule. Retain contract ids and existing observation semantics rather
than relax them to fit the new backend. Add provenance that ties each tested
artifact to its LIR, proof bundle, runtime and compiler identity.

## Owner decisions before implementation

- Approve or revise the fib-only experimental slice and the Rust/Almide split.
- Approve the opaque-handle C ABI direction and Linux-first link manifest.
- Select the LLVM version/distribution policy and acceptable compiler package size.
- Accept the explicit proof gaps above and their proposed gates; no blanket
  noalias or floating-point reassociation is implied by the architecture.

No implementation or prototype measurement is claimed by this document.
