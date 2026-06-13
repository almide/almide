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
re-derives the whole spine on opam's latest Rocq 9.x — currently **9.2** (opam
has no 9.1.1; the canonical pin above is a local source build). The proofs are
kernel-checked and **axiom-clean on BOTH 9.1.1 and 9.2** — a cross-version
re-derivation, not a single-version artifact (a strength, not a gap). Rocq 9.2
ships only the `rocq` driver, so CI provides `coqc`/`coqchk` as thin shims over
`rocq compile` / `rocq check` (the latter IS the Rocq Proof Checker — the
independent De Bruijn re-check is genuine).

## The irreducible base (cannot be discharged by proof — "消えない底")

These four are trusted by necessity; everything else is proven against them:

1. **The Coq/Rocq kernel.** Decades of adversarial scrutiny; `coqchk` re-checks
   every `.vo` independently (the De Bruijn criterion). New logics have zero
   accumulated scrutiny — hence we borrow, not invent.
2. **OCaml extraction + the OCaml compiler.** The proven `check_cert` is
   extracted to OCaml and compiled by `ocamlopt`. This is the Thompson hole;
   **CertiCoq + CompCert close it** (extract to CompCert Clight → machine code,
   all in-logic) — brick 6, not yet done.
3. **Hardware.** The CPU executes the machine code faithfully.
4. **ALS validity** — that the formal semantics captures the INTENDED meaning.
   This is the one item checked empirically (interp + dojo + use), never proved.

## Axiom ledger (the "Print Assumptions ⊆ standard" gate)

Every theorem rests on **nothing but the kernel** — `Print Assumptions` reports
*Closed under the global context* for all of them (no `Admitted`, no extra
axioms). Verified by `proofs/check.sh`:

| theorem | file | assumptions |
|---|---|---|
| `check_sound` | OwnershipChecker.v | Closed under the global context |
| `check_all_sound` | OwnershipChecker.v | Closed under the global context |
| `check_cert_sound` | OwnershipChecker.v | Closed under the global context |
| `check_reuse_sound` | OwnershipChecker.v | Closed under the global context |
| `eager_copy_refines_safety` | ALS.v | Closed under the global context |
| `mrun_tracks_exec` | RuntimeModel.v | Closed under the global context |
| `alloc_not_live` | FreeList.v | Closed under the global context |
| `rc_dec_prog_realizes_rt_dec` | WasmRcDec.v | Closed under the global context |
| `rc_inc_bytes_encode_the_instruction_tree` | WasmEncode.v | Closed under the global context |
| `rc_inc_bytes_execute_to_rt_inc` | WasmExec.v | Closed under the global context |
| `make_unique_yields_unique` | CowSafety.v | Closed under the global context |

## Known limitations (what is NOT yet proven — recorded, not hidden)

The receipt's claims are scoped to exactly this:

- **The flight-grade property SET is complete on the value-semantics subset**:
  RC balance (memory safety), name totality, capability bound (incl. transitive),
  type-concretization, memory-model leak-freedom, reuse soundness (a `Reuse` acts
  only on a UNIQUELY-owned object — no aliased in-place reuse), free-list
  reuse-safety (a valid allocation never returns a currently-LIVE block — no
  reuse-after-free, `FreeList.alloc_not_live`), copy-on-write alias-safety
  (`MakeUnique` yields a uniquely-owned block — no aliased in-place mutation,
  `CowSafety.make_unique_yields_unique`), byte-binding table + the `$rc_dec` /
  `$rc_inc` instruction-trees realizing `rt_dec` / `rt_inc` (`WasmRcDec`) + the
  rc_inc instruction tree encoding to the REAL wasm bytes (`WasmEncode`, grounded
  against wat2wasm) + those real bytes EXECUTING to `rt_inc` on a wasm stack
  machine — incl. the double-free trap control flow — (`WasmExec`), operand-stack
  balance, and termination of the loop-free fragment — all kernel-checked and
  axiom-clean (32 theorems). What remains is DEPTH (the byte-binding ISA layer; and
  the RENDERER realizing the free-list/`rc_inc` — its safety MODEL is now proven,
  so that slice REFINES a proof rather than adding trusted runtime) and BREADTH
  (lowering beyond the subset: control flow, closures, stdlib) — not new properties
  on the subset.
- **The wasm renderer is in the RC regime (A1.1b): it emits a release per drop.**
  A `Drop` now renders as `call $rc_dec`, decrementing the refcount cell (laid at
  heap offset 0 by the A1.1a relayout = `RuntimeModel.RC_OFFSET`) to 0 — so the
  binary actually FREES at the cell level. The safety basis moved accordingly:
  no longer `eager_copy_refines_safety` (the artifact is no longer Dec-free) but
  `RuntimeModel.balanced_cert_no_memory_fault` — an accepted (balanced)
  certificate has no double-free in the memory machine — together with
  `balanced_cert_frees_in_memory` — its cell ends FREED (rc 0). Both are already
  kernel-proven and axiom-clean (this slice is pure proof-REUSE: no `.v` changed).
  The per-build `validate_translation_perceus` V binds each witness drop to a
  `call $rc_dec` byte (one release per drop, no fewer), so the proof transfers to
  the REAL bytes; and the `$rc_dec` runtime SENTINEL traps a double-free at run
  (`unreachable` on an already-0 cell — verified firing on wasmtime). So `C-SAFE`'s
  no-double-free AND cell-level leak-freedom are now claimable for the EMITTED
  artifact, not just the model. PHYSICAL reclamation is now REALIZED (A1.2-render):
  `$rc_dec` at rc→0 returns the block to a free-list and `$alloc` reuses an
  exact-size head, REFINING the proven `FreeList.v` model (`alloc_not_live`: a valid
  allocation never returns a currently-LIVE block — no reuse-after-free). The
  double-free sentinel is PRESERVED — the free-list link lives in the dead LEN field,
  NOT the rc cell, so a re-release of a freed block still traps (verified: the
  double-free trap test and a reuse test, `p1==p2` on alloc/free/realloc, both pass
  on wasmtime; the value-semantics output is byte-unchanged). HONEST scope of what is
  SHARING is now REALIZED too (A1.3-render): `Dup` shares via `rc_inc` (no copy) and
  `MakeUnique` is a copy-on-write (clone-on-shared — `rc_dec`-FIRST so the alias keeps
  the original alive and no temp is needed, then `list_copy`), refining
  `CowSafety.make_unique_yields_unique`. The rc cell now ACTUALLY tracks the abstract
  refcount (1→2→1→0), exercising the proven rc machine (`WasmRcDec`/`RuntimeModel`),
  with byte-unchanged value-semantics output. So A1's renderer is now FULLY real-RC —
  share / cow / free-list / double-free sentinel — every piece REFINING a proof, zero
  trusted runtime added. NOT yet done (perf/encoding, not safety): the free-list is
  exact-size HEAD-match only (a mismatched-size freed block is not yet reused — missed
  reuse, NEVER unsafe; size-classes / walking is a later slice); and the raw-BYTE
  encoding of the instruction trees (A2's deferred heavy half).
- **Byte-binding is partial.** The op→wasm-instruction TABLE is a formal Coq
  object (`Translation.v`) and the runtime heap is modeled as a memory state
  machine whose rc cell provably tracks the abstract refcount
  (`RuntimeModel.mrun_tracks_exec`); `validate_translation` re-checks per build
  that each op's pattern is emitted (a drop's is `call $rc_dec`) and
  `validate_translation_perceus` that one release is emitted per drop. The model's
  `RC_OFFSET = 0` now COINCIDES with the renderer's physical rc-cell offset (the
  A1.1a relayout) and `call $rc_dec` writes that cell. **A2 first slice DONE
  (instruction-tree level), `WasmRcDec.rc_dec_prog_realizes_rt_dec`**: the EXACT
  `$rc_dec` instruction tree the renderer emits (modeled as data, with a small
  operational semantics for the load/add/sub/store/trap fragment) provably computes
  `RuntimeModel.rt_dec` — same trap (cell 0), same decrement. So the abstract
  release the leak/no-double-free proofs use is what the emitted INSTRUCTIONS
  compute, not a token. The byte ENCODING is now bound too (A2 byte slice,
  `WasmEncode.rc_inc_bytes_encode_the_instruction_tree`): a Coq wasm-binary encoder
  produces EXACTLY the bytes `wat2wasm` emits for the renderer's `$rc_inc`, GROUNDED
  per build by `proofs/check-wasm-bytes.sh` (re-assemble, compare — so the opcode
  constants are the real wasm bytes, not a guess: non-circular). Composed with
  `rc_inc_prog_realizes_rt_inc`, the emitted BYTES encode an instruction tree that
  computes `rt_inc`. And the EXECUTION is now bound too
  (`WasmExec.rc_inc_bytes_execute_to_rt_inc`): a minimal wasm STACK MACHINE runs
  the real rc_inc bytes and the memory effect is EXACTLY `rt_inc` — so the bytes,
  EXECUTED, compute the abstract acquire (not merely encode an instruction that
  would). So `rc_inc` is bound END TO END: instruction tree ↔ real bytes ↔
  execution ↔ rt_inc, the trust chain reaching the ACTUAL wasm bytes. The
  interpreter also EXECUTES the double-free TRAP control flow
  (`WasmExec.trap_bytes_trap_on_zero` / `_pass_on_nonzero`: the bytes for
  `(if (i32.eqz cell) (then unreachable))` trap IFF the cell is 0 — the sentinel,
  on real grounded bytes), so it handles both straight-line code AND the
  safety-critical trap. The FOUNDATION for GENERAL structured control flow is also
  built (`WasmExec.skip_block` + `imm_len`): an immediate-aware structure finder
  that locates a block's matching `end` WITHOUT being fooled by immediates that
  collide with opcodes (`i32.const 4` = `41 04` where 0x04 is `if`; `i32.const 11`
  = `41 0b` where 0x0b is `end`) — proven on those exact collision cases. This
  shows general control flow needs only a small per-opcode immediate-length table,
  NOT a full WasmCert-Coq parser. And it is now WIRED into a general `if` EXECUTOR
  (`WasmExec.run_g` + `split_block`): a fuel-bounded interpreter that runs a general
  structured `if … end` — the then-body EXECUTES when the condition is nonzero and
  is SKIPPED otherwise (proven on `if (cond) (then store 0:=42)` — body runs / is
  skipped), beyond the fixed trap pattern. NOT yet done: global state (`global.get`/
  `set`, a reserved cell) + model alignment to bind the FULL `rc_dec` (free-list)
  end-to-end on `run_g`; the rest of the module; and that this small inspectable
  interpreter matches the FULL wasm spec / ISA (the residual — WasmCert-Coq).
- **One real `.almd` now flows end-to-end** (`proofs/fixtures/return_list.almd`
  → the actual frontend → MIR → proven checker, for ownership + names — weekly
  indicator ① 0→1). The lowering covers only the value-semantics subset (heap
  literals, alias, index-assign copy-on-write, scalar/heap-move-out return — NO
  calls or control flow yet, #29), so the broader reject cases and the
  capability witness are still REPRESENTATIVE MIR shapes (emit_cert.rs).
- **Extraction is trusted** (item 2 above) until CertiCoq/CompCert.
- **Single independent checker.** Diversity (≥2 independent checkers) is brick 6.

## Use-relativized completeness

Completeness is declared per use, not absolute. Today the proven property set is
complete for **memory-safety-of-the-ownership-fragment under the eager-copy
realization** (no double-free). It is NOT a claim of absolute-semantics coverage
(that diverges — CompCert-grade). The receipt names which use each artifact is
proven for.
