Original description metadata: Historical Perceus belt design, retired emission type-state and scoped proof evidence
<a id="almideperceusbelt"></a>

# AlmidePerceusBelt

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](almide-perceus-belt.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](almide-perceus-belt.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](almide-perceus-belt.zh-CN.md)

> Non-normative reader snapshot of the pinned source; original remains authoritative. No automatic synchronization, independent implementation validation or execution of quoted instructions. Historical status, plans and comparative claims retain their source scope.
> [Original source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/almide-perceus-belt.md)

> Source caveat: the opening retirement correction limits the later historical absolute-safety slogan and Phase B guarantees. The retired type-state boundary and old escape-hatch commands are not current CLI instructions. Zero violations refers only to the 394-file historical corpus, not every program. The corrected incident was a false positive, not missing Dec; adding one would double-free. The old future Phase A plan coexists with later proof work. Lean ownership proofs and Coq runtime models have different scopes; renderer stores and recording assumptions remain trust boundaries. No commands or proofs were rerun.


<a id="historical-type-state-design-and-present-proof-scope"></a>

## Historical type-state design and present proof scope

The Phase B `Verified`/`PerceusVerify` emission boundary below belonged to
the retired v0 emitter ([retirement record](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/crates/almide-codegen/src/lib.rs)).
It is not the current structural emitter's entry condition, and the old
`--emit-unverified` instructions are not current CLI guidance.

A checker establishes its modeled property over the witness it accepts;
it does not establish that all compiler bugs are impossible to ship. The
Lean belt models an ownership transform, while later Rocq runtime and
certificate work has its own coverage and recording assumptions. Read
[Memory Safety](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/design/MEMORY-SAFETY.md) for the current boundary.
The incident corrections and the later Coq appendix below are historical
evidence, not a claim that the old Phase A plan is still wholly unimplemented.

> Almide's equivalent of RustBelt. Type-guided memory safety, proven by the compiler.

<a id="architecture"></a>

## Architecture

```
Phase B (Rust type-state): 検証を通らないと emit できない    ← 今ここ
Phase A (Lean 4 proof):    検証ロジック自体が数学的に正しい  ← next
A + B = 完全体
```

Diagram translation: Phase B (Rust type-state): cannot emit without passing verification ← here now. Phase A (Lean 4 proof): the verification logic itself is mathematically correct ← next. A + B = complete form.

<a id="phase-b-rust-type-state-enforcement-current-session"></a>

## Phase B: Rust Type-State Enforcement (current session)

Use Rust's type system to enforce “unverified IR cannot be emitted.”

```rust
struct Verified<T>(T);  // 検証済みマーカー

impl PerceusPass {
    fn run(fb: FnBody) -> Verified<FnBody> { ... }
}

impl WasmEmitter {
    fn emit(fb: Verified<FnBody>) -> Vec<u8> { ... }  // Verified のみ受付
}
```

Code-comment translation: Verified marker; only Verified input is accepted.

<a id="status-hard-error-化済みb5-完了-2026-06-07"></a>

### Status: converted to hard errors (B.5 complete, 2026-06-07)

PerceusVerifyPass (perceus-belt) has **0 violations on the spec corpus** (measured by emitting 394 files to wasm). violation > 0 is a **hard error** (`almide build --target wasm` is rejected with `error: [COMPILER BUG] Perceus RC verification failed`).
- Global: every heap var has a Dec (no leaks)
- Global: dec_count <= inc_count + 1 (no double-free)
- Per-branch: Dec consistency across every if/else/match arm
- Exclusions (leak checks only; each means “ownership leaves this block” = Perceus's “moved” relation):
  - `EnvLoad` bindings (borrowed from a closure environment = non-owning)
  - **`returned_vars`** (function-return tail = ownership escapes the function)
  - **`moved_out_vars`** (`x` in `Block{expr:Some(Var x)}` = a bare-Var block tail. Ownership moves to the block's consumer, which owns the Dec. Structurally eliminates false positives for ANF-generated `__perceus_ret`/`__anf_*` tail temporaries — collected by exhaustive `IrVisitor` traversal, not a name-prefix band-aid)
  - `__tco_*` / `__br_*` name prefixes (TCO/branch temporaries. The Dec exists but is in a different block from the Bind, producing a false positive when flat per-block `count_decs` reads a local 0. Replacing this with scope-aware Dec accounting is a follow-up)

<a id="what-phase-b-guarantees"></a>

### What Phase B guarantees

“IR that fails verification cannot be converted to WASM”
= prevents internal compiler bugs from shipping undetected

<a id="what-phase-b-does-not-guarantee"></a>

### What Phase B does NOT guarantee

“The verification logic itself is correct”
= a bug in the verification function could let invalid IR pass as Verified

<a id="phase-b5-warn-mode--hard-error2026-06-07-完了"></a>

## Phase B.5: warn-mode → hard-error (completed 2026-06-07)

**Empirical evidence**: the v0.25.0 release **shipped with only warnings** for `[Perceus] N RC violation(s)` (HOF-closure `__perceus_ret`; repro = the `bench` fn in `research/benchmark/stdlib/precise_all.almd`; minimized = a HOF with `f()` followed by tail `println("[" + name + "]")`). `Verified::verify` (almide-codegen/src/lib.rs) was intentionally in warn-mode, issuing `Verified` even with violations — at that time `Verified` meant “verification ran,” not “verification passed.” The type documentation's “unverified programs cannot reach emission” contradicted reality and also violated contract C-041.

**Root-cause correction (important)**: this was **not** the initially assumed **missing** RcDec. RC was balanced: it was a **false positive**. `__perceus_ret` is the bare-Var tail of its defining block (`Block{expr:Some(Var ret)}`) = a **move-out**; ownership moves to the consumer's `Bind __anf_*`, which has `RcDec __anf_*` (demonstrated with dump_rc_trace: no `RcInc` on `__perceus_ret`, so it is a move; the consumer's `__anf_1` has a real Dec). **Adding a Dec here would double-free.** The real cause was verify failing to distinguish “a var escaping via function return” (`returned_vars`) from “a var moving out of a block,” misclassifying the latter as a leak. Flat `collect_all_tail_vars` (which, as tail-context analysis, does not descend into `Expr` statements of `Block{expr:None}`) missed `__perceus_ret`, leaving `returned_vars` empty.

**Fix**: introduced the separate `moved_out_vars` set, collecting every `Block`'s bare-Var tail structurally via `collect_moved_out_vars` (exhaustive `IrVisitor` traversal, total-by-construction) and exempting it only from leak checks. Rather than simply merging into `returned_vars` (= skipping all checks), only leak checks are exempted so that Dec on a moved value = double-free remains detectable. Consolidated 5 arguments into `VerifyCtx`. **Structural** move-out recognition, not a name-prefix band-aid.

**Decision: warnings address the wrong recipient.** A violation is a bug in compiler-inserted RC, not user code, and users cannot fix it. Its diagnostic class is therefore **ICE**. “Leaks, not unsoundness” concerns severity, not whether to open or close the gate (severity determines only the message's tone).

- [x] `Verified::verify`: violation > 0 is a **hard error** — `error: [COMPILER BUG] Perceus RC verification failed`, “This is a compiler bug. Please report it,” plus `--emit-unverified` guidance. Controlled `process::exit(1)` (the same approach and `[COMPILER BUG]` format as `assert_types_concretized`, neither a `panic!` backtrace nor a numeric E-code — the latter was avoided because it is confusable with rustc's `E060x` space)
- [x] Escape hatch `--emit-unverified` (`almide build` only): emission continues only with an explicit flag and emits a warning. `run`/`test` do not support waivers and always hard-error (do not silently run a leaking artifact). **Recording `perceus_verified: false` in the manifest is unimplemented** — the build-manifest mechanism itself is not in place, so this is sent to trust-layer receipts work (trust-layer.md) as a follow-up
- [x] Structurally fixed the `__perceus_ret` false positive (above) + **added a HOF-closure fixture to spec/** (`spec/wasm_cross/hof_closure_string_tail.almd`, `// @contract: C-041`). The real gap was the corpus — because spec lacked this pattern, the regression shipped while CI stayed green. The fixture covers move-out in HOF/nested/returned-String/map-churn, guaranteeing native==wasm byte identity (expanding C-041 evidence)
- [x] Corrected the Status above from “0 violations” to “0 violations **on the spec corpus**”

<a id="follow-upb5-で構造化しきれなかった分明示的トレードオフ"></a>

### Follow-up (not fully structural in B.5; explicit trade-offs)

- Replace the `__tco_*` / `__br_*` name-prefix exclusions with **scope-aware Dec accounting**. They currently suppress false positives correctly (the Dec exists in a different block from the Bind), but name dependence is not ideal. Investigate replacing them with a structural rule like `moved_out_vars` (a move-via-`Assign` flow). TCO's RC convention differs from ANF move-out, however (both `Assign loopvar = Var tmp` and `RcDec tmp` exist), and casual unification risks false negatives; handle it carefully as a separate task.
- Manifest recording of `--emit-unverified` (above).

<a id="phase-a-lean-4-formal-proof-future"></a>

## Phase A: Lean 4 Formal Proof (future)

Mechanically prove the correctness of Perceus rules in Lean 4, then convert to Rust with lean4-rust-backend.

<a id="plan"></a>

### Plan

1. Create a Lean 4 project (almide-perceus-belt)
2. Formalize FnBody, VarId, Ty in Lean 4
3. Define the 6 Perceus rules in Lean 4
4. Prove the theorem:
   ```
   theorem perceus_sound :
     ∀ (fb : FnBody) (vt : VarTable),
       well_typed fb vt →
       rc_balanced (perceus fb vt)
   ```
5. Convert to Rust code with lean4-rust-backend
6. Integrate generated Rust functions into almide-codegen
7. Integrate with Phase B's Verified&lt;T&gt;

<a id="dependencies"></a>

### Dependencies

- lean4-rust-backend: ~/workspace/github.com/O6lvl4/lean4-rust-backend
- Lean 4 toolchain: v4.28.0 (installed via elan)

<a id="references"></a>

### References

- Reinking et al., "Perceus: Garbage Free Reference Counting with Reuse" (ICFP 2021)
- Ullrich & de Moura, "Counting Immutable Beans" (IFL 2019)
- Jung et al., "RustBelt: Securing the Foundations of the Rust Programming Language" (POPL 2018)

<a id="exit-criteria"></a>

## Exit criteria

- Phase B: the `Verified<FnBody>` type is required as WASM emitter input
- Phase A: the `perceus_sound` theorem is proven in Lean 4
- A + B: proven logic enforced by types = the complete AlmidePerceusBelt


<a id="post-v0270-proof-coverage-gap-2026-06-11"></a>

## Post-v0.27.0 proof-coverage gap (2026-06-11)

The belt's Lean theorems certify the IR-level Inc/Dec balance. v0.27.0 made
the runtime side REAL (free-list push/reuse, the rc==0 double-free sentinel,
the rc_inc resurrection trap, region resets that clear the free list) — none
of which was in the proof surface at the time.

**This half landed in the Coq trust spine, not the Lean belt** — read
[`proofs/FreeList.v`](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/proofs/FreeList.v), not a `.lean` file:

- the allocator state machine (bump frontier, free-list, ghost live set) with
  reuse-safety — `alloc_not_live`: a VALIDATED allocation is never a currently
  live block (#909 / A1.2);
- **a region reset leaves no stale free-list entry** — `RegionSave`/`RegionRestore`
  preserve the allocator invariant across an ARBITRARY window body, so no
  free-list entry can point into the reclaimed region and reuse-safety survives
  the reset (`region_window_reuse_safe`,
  `region_reset_leaves_no_free_into_the_region`). The naive alternative — keeping
  the window's free-list — is proven to break the invariant AND to re-hand a live
  block, on a concrete trace;
- **PINNED_RC immortality** — a `$alloc8` / `__alloc_pinned` block (WASI scratch,
  the preopen tables) is never freed, never on the free-list, never returned by
  `$alloc`, over an arbitrary run, and survives a region reset
  (`pinned_stays_immortal_forever`, `p_region_reset_preserves_PINV`). This is the
  invariant C-042 violated.

- **"reuse restores rc=1"** — the COUNT side, in the companion
  [`proofs/FreeListRc.v`](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/proofs/FreeListRc.v). It puts the reference
  count in scope as `RuntimeModel.read_rc` (the same cell `WasmRcDec`/`WasmExec`
  bind to the emitted bytes) and proves the two halves separately: the ALLOCATOR
  hands a reused block back carrying no stale count — it arrives at exactly 0
  (`reuse_hands_back_a_zero_count_block`) — and the constructor's store leaves it
  at exactly `RC_INITIAL` = 1 (`reuse_restores_rc_1`). The invariant "a free-list
  block reads 0, a live block reads ≥ 1" survives an arbitrary run
  (`r_steps_preserve_RINV`) and a region reset
  (`r_region_reset_preserves_RINV`), which is what makes the `$rc_dec` sentinel a
  wall: `double_release_traps`. The pair is proven NECESSARY, not decorative —
  `omitting_the_rc_store_traps_on_the_first_release` runs the trace where the
  renderer's store is missing and the reused block traps at its first release.

STILL TRUSTED, and only this: that the EMITTER writes the `RC_INITIAL` store at
every rc-managed `call $alloc` site — a property of the renderer, not of the
allocator, so no proof about the allocator can reach it. It is gated executably
by `almide-mir`'s `every_rc_managed_alloc_site_initializes_the_rc_cell`, which
enumerates all 16 `$alloc` sites in the renderer sources, requires each to
initialize the rc cell or be a named raw-scratch site (the modelled-and-proven
`alloc_raw` category: argv/envp buffers, never released), and reads `RC_INITIAL`
out of the `.v` file so the constant cannot drift. Emitter-level rc operations
(stored-field dups, in-runtime incs) also remain invisible to the IR verifier by
construction.
