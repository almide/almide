# Almide v1 trust kernel (Coq/Rocq proof spine)

The flight-grade certification spine (docs/roadmap/active/v1-mir-architecture.md
§5, and the tier-1 stack). **We do not prove the compiler — we prove the
CHECKER.** The (large, untrusted) compiler emits an artifact + a certificate;
a tiny checker `K` verifies each build, and `K`'s soundness is proven here:

```
check(certificate, artifact) = accept  ⟹  P(artifact)
```

so trust shrinks from the ~100k-line compiler to a few-hundred-line checker
(the DO-330 qualification target). The seam between compiler and checker is the
certificate format, not a prover.

**Prover choice = Coq/Rocq (single TB), not Lean.** The daily proofs (RC
balance, stack balance) stay in the Lean Perceus belt (`crates/almide-perceus-belt/`),
but the flight-grade SPINE needs (a) a safety-certification precedent and (b) a
verified compiler IN THE SAME LOGIC to bring the checker to machine code — only
Coq/Rocq + CompCert/CertiCoq has both today (CompCert is the only verified
compiler with qualified use in safety-critical settings).

## Status

- **brick 1 (here): `OwnershipChecker.v` — KERNEL-VERIFIED.** The RC-balance /
  memory-safety checker `K` over the MIR ownership op sequence, and its soundness
  theorem `check_sound : check ops = true → no_double_free ops ∧ no_leak ops`.
  The irreducible single-object core. Verified three ways (Rocq 9.1.1):
  - kernel-checked (`coqc`), `Qed`, `0` admits;
  - **axiom-clean** — `Print Assumptions check_sound` = *Closed under the global
    context* (no extra axioms; the "Print Assumptions ⊆ standard" gate);
  - **independently re-checked** by `coqchk` (the De Bruijn criterion).

- **brick 4 start: `ALS.v` — ALS-as-normative + first honest translation-validation.**
  `exec` is named the NORMATIVE ownership semantics; `eager_copy_refines_safety`
  proves that an increment-only (eager) trace cannot double-free, and
  `dec_free_leaks` records that such a trace does NOT free (it emits no release).
  The wasm renderer has since moved to the RC regime (A1.1b: `Drop → call
  $rc_dec`), realizing cell-level leak-freedom (`balanced_cert_frees_in_memory`)
  with safety basis `balanced_cert_no_memory_fault`; the eager theorems remain the
  dual-oracle baseline. Kernel-checked, axiom-clean, coqchk-confirmed. (Full V =
  emitted wasm BYTES ⊒ ALS needs a wasm model in Coq — a later brick.)

- **brick 2: the proven checker RUNS on real bytes.** `Extract.v` extracts
  the kernel-proven `check` to OCaml; `driver.ml` + `build-checker.sh` link it
  into a `checker` binary that validates **certificate format v0** (a
  Metamath-simple token stream: `i`/`I` = an ownership +1, `d`/`D` = a −1,
  whitespace ignored). `./build-checker.sh` accepts a balanced certificate and
  rejects double-free / leak. This is the proof-carrying-code chain made
  operational — the proof stops being "about a model" and becomes "a running
  binary that carries its soundness proof and judges real output".

- **#4013: `MemorySafe.v` — the one definition of memory safety.** Over a heap
  trace (alloc / store / load / inc / dec), `MemorySafe` is four clauses:
  SPATIAL (in bounds), TEMPORAL (no use-after-free, double free or
  resurrection), INITIALIZATION (no read of an unwritten byte) and TYPE (no
  representation confusion); `memory_safe_iff_clauses` proves it is exactly
  those. A trace checker is sound and complete for it, with a negative
  control per clause. `rc_check_gives_temporal` maps brick 1 into it: the
  guarded RC check yields TEMPORAL, and only TEMPORAL. No certificate carries
  a trace yet, so this fixes the right-hand side of
  `K(c, a) accepts ⟹ MemorySafe a` rather than proving it of shipped bytes.

- **#4005: `SpatialCert.v` — the SPATIAL clause from a static certificate.**
  A certificate language for the element reads and writes of list blocks
  (read, write, bounds check, reassign the index, loop, branch; each index
  slot names its block), a checker that accepts an access only when every
  path to it passes a check of the same index slot with no reassignment
  since, and `accepts_spatial`: every run of an accepted
  certificate — any loop count, any arm, any i64 written to an index, runs cut
  short by a failed check — yields a `Spatial` (and `WellTyped`) trace. The
  check is the emitter's single unsigned compare; `unsigned_ge_excludes` shows
  it excludes negative indices too. Elided repeat checks (#3345) and hoisted
  checks (#2319) are re-derived by the checker, not trusted; the `rejects_*`
  controls show a stale check, a check hoisted past a reassignment and a
  one-armed check are refused, and `unchecked_run_not_spatial` that the
  refused shape does read out of bounds. `BoundsCert.v` adds the byte
  format and its parser inside the proof; `check_bounds_cert` is extracted
  (driver mode `bounds`) and `check_bounds_cert_sound` states what ACCEPT
  means. A malformed string is rejected, never read as a prefix. The
  structural wasm leg emits it per build as the `bounds` witness
  (crates/almide-wasm/src/witness_bounds.rs) for every access whose check
  #3345 may elide, and `structural-wall.sh` judges it three ways (extracted
  checker, kernel, almide-verify). That the witness's events are the
  accesses the bytes contain is trusted, as for the ownership witness.

- **#4014: `CoreSafe.v` — well-typed core programs are MemorySafe.** A
  typed core of the source language — ints, bools, int lists with value
  semantics, boxes (a block whose one slot holds any value, lists and boxes
  included), `let` (fresh names), assignment, element store (copy into a
  fresh block), `if`, `while`, `make`, indexing with its bounds check, `len`,
  `if` expressions and one- and two-argument functions with recursion (a
  call runs the body in a fresh environment holding only its parameters,
  two of them distinct, each argument given back in order on return) — with a
  big-step semantics that emits MemorySafe.v's heap trace in the
  layout the compiler uses (header, then one slot per element). Blocks are
  reference counted and freed: reading a list variable shares its block
  (`EInc`), and a consumed operand, an overwritten variable and a returning
  call's parameter give their reference back (`EDec`); at zero the block is
  freed, and freeing a box gives back the reference its slot held (the
  semantics is indexed by the heap to decide that). The invariant is that
  every block's count covers the references the program still holds,
  including those in the slots of live boxes (a ghost store records them).
  `typed_core_memory_safe`: every run of a well-typed program that finishes,
  or that a failed check ends, is `MemorySafe` — spatial, temporal,
  initialization and type. `unchecked_index_not_safe` shows the check is
  what spatial rests on, `shared_list_survives_mutation` runs an alias
  through a mutation and a free, `missing_inc_is_use_after_free` shows the
  same run without the shared read's `EInc` loads from freed memory,
  `box_frees_and_keeps_content` frees a box and keeps the list it held
  alive, `box_store_without_share_is_use_after_free` shows a box store
  without its `EInc` (the shape of #2133) reaches freed memory, and
  `count_down_ok` types a self-recursive function. Not yet: closures,
  records of more than one field, variants, strings, multi-argument calls, release at scope end
  (a binding is released when overwritten; the rest leaks), diverging runs,
  and any link to the compiler's lowering.

- **#4013: `Certified.v` — the two certificates, composed.** The RC
  certificate gives TEMPORAL (`rc_check_gives_temporal`); the bounds
  certificate puts every access inside its block's layout (`run_sound`).
  Neither alone gives SPATIAL over a trace that frees: an access needs its
  block to exist, which only the RC side says.
  `certified_with_bounds_cert` states what the pair gives on one interleaved
  trace — `Spatial`, `Temporal` and `WellTyped` — given a fresh allocator
  and layout-true allocations. `access_after_free_not_temporal` is the
  control: an in-layout read of a freed block is not safe.
  `certified_memory_safe` reaches all four clauses with one more
  hypothesis, `init_disc`: every allocation is immediately followed by the
  store that initializes its whole extent (the constructor discipline; no
  witness carries it yet). `read_before_init_not_initialized` is its
  control. Not reached: the link from the certificates' events to the bytes.

## Verify (the third-party `make verify`)

```
cd proofs
./check.sh           # coqc (+ axiom audit) then coqchk — the proof
./build-checker.sh   # extract → link → run the proven checker on certificates
```

## Known limitations (the honest base — config-management §7)

The trusted base is recorded so the rest may be called "proven":
- The **kernel** (Coq/Rocq) and **OCaml extraction** are trusted (CertiCoq +
  CompCert will close the extraction hole, brick 6).
- ~~The tokenizer in `driver.ml`~~ — **INTERNALIZED into Coq** (`parse` +
  `check_cert`, proven by `check_cert_sound`): the whole *bytes ⟶ accept/reject*
  pipeline is now kernel-checked. The trusted glue is reduced to file I/O only.
- The model is single-object RC; multi-object ✅ (`check_all`); the other
  properties (stack balance, name totality, …) are later bricks.

Rocq/Coq provides `coqc`/`coqchk`. Toolchain pin + axiom ledger is a later
brick (config management, §7 of the tier-1 stack).

## Byte grounding: two runtimes, one retired (#2753, #2761)

Two gates bound proven byte lists to bytes the compiler really emits. They
are about DIFFERENT runtimes:

| gate | proofs it grounds | bytes it compares against |
|---|---|---|
| `check-wasm-bytes.sh` (retired in #2761) (+ `check-wasm-exec.sh`) | `WasmEncode.v`, `WasmExec.v` (with `WasmRcDec.v`, `WasmDecode.v`, `WasmIsa.v`: FORMAL-CREDIT FC-4) | the **incumbent** WAT renderer's `$rc_inc` / `$rc_dec`, extracted from `render_program` output |
| `check-structural-bytes.sh` | `StructuralDecode.v` (with `StructuralRuntime.v`, `StructuralRun.v`, `StructuralAlloc.v`) | the **structural** emitter's `$inc` / `$dec_flat` / `$free` / `$alloc` code bodies (`almide-wasm` `dump_runtime_bytes`) |

The structural emitter is the default `--target wasm` leg, so its runtime
bytes are already grounded. The incumbent's byte lists cannot be re-grounded
on it: the two runtimes are different code, and the structural bodies already
have their own proven lists.

`check-wasm-bytes.sh` therefore **retired with the incumbent renderer**, in
the change that deleted it (#2761, #1696 step 5), and not earlier: until
then the incumbent still shipped programs, and the gate was true evidence
about those bytes. At the deletion:

- `check-wasm-bytes.sh`, its `proofs/check.sh` call and its
  gate-verification row were deleted; `check-wasm-exec.sh` keeps running
  `run_g` against a real engine, but its rc bodies are no longer "the
  renderer's";
- FC-4 and the C-PROVEN receipt line are restated as theorems about a
  MODELED runtime (the retired incumbent's), and `check-structural-bytes.sh`
  is named as the shipping-bytes grounding.

## Roadmap (the tier-1 stack, in critical-path order)

1. ✅ first property (RC balance) checker + soundness — this file.
2. Certificate format v0 (a Metamath-simple witness language) — the seam.
3. Wire the compiler to emit the witness; CI gates "K accepts every build".
4. ALS core (Coq small-step semantics) + translation validator `V`
   (emitted wasm bytes ⊒ the verified model), per build.
5. One property at a time: stack balance / name totality / capability bound /
   type concretize / termination — each defined against ALS, K-soundness proven,
   witness emitted, CI-gated, added to the receipt.
6. CertiCoq + CompCert: extract `K`/`V` to machine code, closing the Thompson
   hole. Diversity: ≥2 independent checkers. Config management: toolchain pin +
   axiom ledger + known-limitations (the irreducible base: Coq kernel,
   CompCert/CertiCoq TB, hardware, and ALS validity).
