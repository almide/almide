# Certificate Format v1 — design

> Status: **design accepted; brick 1 shipped** (ownership alphabet i/a/d/m).
> Supersedes the implicit "i/d-per-object" format. Grounded in a prior-art +
> adversarial design pass (Perceus/Koka reuse, linear/foundational PCC,
> WasmCert-Coq byte semantics, the v0 ownership taxonomy).

## Why this exists

The v1 trust thesis: **prove the CHECKER, not the compiler**. The untrusted
compiler emits a per-build *witness*; a tiny kernel-proven (Rocq) checker,
extracted to OCaml, re-verifies it — `accept(witness, artifact) ⟹ P(artifact)`.
The core selling point is that the qualification target (the checker) is a **few
hundred lines** an auditor can read.

The v0 witness (`ownership_certificate`: one per-object stream of `i`/`d`
balanced by `check_all`) can only say *"parentheses balance"*. The three gaps
ahead demand far more:

- **G1 witness ⟹ wasm bytes** — bind the witness to the actual emitted bytes and
  the runtime heap machine (`__alloc`/`__rc_dec`/free-list).
- **G2 frees (real-RC) renderer** — express SHARING (fresh vs alias vs move),
  reuse, and leak-freedom — the i/d stream is too coarse.
- **G3 slice → full language** — call, control-flow, closures, nested/recursive
  heap.

## The central tension, and the principle that resolves it

If the witness grows rich enough for all three, a naive checker grows with it —
and the few-hundred-line core collapses. The resolution:

> **Keep the checker small not by limiting what the witness EXPRESSES, but by
> limiting what the checker DECIDES.** The witness is *elaborated to
> checkability*: every GLOBAL decision (ownership inference, branch
> reconciliation, reuse safety, byte layout) is pre-resolved by the untrusted
> compiler into LOCAL ground facts the checker validates by a fixed rule. The
> checker NEVER infers, NEVER walks a CFG, NEVER opens a callee.

Checker size then scales with **#rules** (event letters + subset instantiations +
op→wasm pattern-table entries), *not* with program size, language complexity, or
compiler complexity.

## The format: flat-stream + side-table, ground facts only

A per-build witness is one program-level bundle of flat, newline/`|`-delimited
text sections — all ground facts (nats, `|`-lists, fixed alphabets); **no section
embeds a Coq term, higher-order structure, or an "infer X" directive**. Every
section is parsed by the two parser shapes already proven in Rocq (`parse_go`
line-folder; `parse_pair` `|`-split). The checker is the AND of per-section
verdicts plus a handful of cross-section *agreement* checks.

### 1. Ownership stream — the v0 core with a richer ALPHABET

One line per reference-counted OBJECT, one char per event; each char carries a
signed delta the existing fold handles, and the *letter itself* is the ground
fact the compiler already decided:

| letter | δ | ground fact | gap |
|---|---|---|---|
| `i` | +1 | FRESH acquire (Alloc / fresh Dup / owned Call-result) | — |
| `a` | +1 | ALIAS acquire (inc an existing SHARED ref) — share-vs-move | G2.1 |
| `d` | −1 | plain release (Drop) | — |
| `m` | −1 | MOVE-OUT (Consume → container / return / consuming callee) | move≠drop |
| `r` | −1 | REUSE-eligible release (drop where uniqueness was proven) | G2.2 |
| `b` |  0 | BORROW marker (closure-env body borrow; no scope-end release) | G2.3 |

The checker's per-line rule is one left-fold over `Z`: `i|a ⇒ +1`, `d|m|r ⇒ (if
rc≤0 reject else −1)`, `b ⇒ +0`; accept iff never `<0` and ends at `0`. **The
soundness proof reasons about the DELTAS, not the letters**, so adding
ground-fact letters costs ZERO new proof obligations. **v0's `i`/`d` is the
degenerate case** (Alias≡Inc, MoveOut≡Dec at the fold).

A `MODE` byte (`eager` | `perceus` | `full`) prefixes the bundle and selects
which letters are legal (eager: only `i a d m`; perceus: `+r`; full: `+b`),
keeping the staged rollout honest — a witness can't silently claim reuse before
the uniqueness obligation exists.

### 2. Side-table sections — all the ONE subset law (Subset.v)

`subset_check sup sub = forallb (fun x => mem x sup) sub`, instantiated several
ways — **one proof (`subset_check_sound`) covers all; adding a property adds a
NAMING, not a proof**:

- `used ⊆ defined` — name totality.
- `used-caps ⊆ allowed` — capability bound.
- **`actual-call-modes ⊆ declared-param-modes`** — call-site vs signature (below).
- `r-objects ⊆ proven-unique-at-drop` — Perceus reuse soundness (G2.2).
- `used-field-offsets ⊆ declared-heap-offsets` — byte layout (G1).

### 3. Call signatures — compositionality (the whole-language lever)

Each function carries a SIGNATURE line: per-param mode (move/borrow), return mode
(move-out/borrow), declared effects/caps. At a call site `call fn_id : actuals`
the checker looks up `fn_id`'s signature and checks `actuals` against it by the
subset rule — **it NEVER opens the callee**. Checking N calls is O(N) membership,
independent of callee size. A closure is a value whose signature includes its
captured-env ownership. This is what lets call / closure / recursion ride in as
"one rule + one signature kind", keeping the checker flat. → G3.1/G3.3.

### 4. Byte-binding — table now, semantics once (G1)

Per op: a tag into a fixed `op → wasm-instruction-pattern` translation table,
plus a layout-constants section (`rc_offset`, `free_list_offset`,
`heap_ptr_global`). The per-build check is a **syntactic** match of emitted bytes
against the table (cheap, in the small checker). The **semantic** half — that the
runtime functions (`__alloc`/`__rc_dec`/free-list) refine the abstract ±1 ops —
is proven ONCE against a wasm memory-model library (heavy, amortized; needs
WasmCert-Coq). `R(M,w)` = "M's bytes match the table for w's ops"; `V'` is the
per-build table-matcher. → closes G1.2's "Dec is an abstract token" without
re-proving the runtime every build.

## The checker-size invariant — and the tripwire to guard

> **INVARIANT.** Every checker rule is either (a) a left-fold of signed deltas
> over one line, or (b) a `mem`/`subset` lookup over a `|`-list of ground facts,
> or (c) a syntactic pattern-match against the fixed translation table. Size ∝
> #letters + #subset-instantiations + #table-entries.

> **TRIPWIRE (the moment the core dies).** The checker is forbidden to: open a
> callee, walk a CFG / follow control flow, or solve a meta-variable / run
> inference. The instant a rule needs one of these, the global decision leaked
> into the trusted base — push it back into the compiler-emitted ground facts.

This is the precise form of "整える勝負": expressiveness is bought by ADDING
SECTIONS (each its own internalized parser + soundness lemma reusing the shared
Subset/Balance laws), never by making a rule cleverer.

## Migration from i/d

`i`/`d` is a **degenerate instance** of the v1 alphabet. The OwnershipChecker.v
`exec`/`check`/`check_all`/`check_cert` and all soundness theorems are KEPT
verbatim; only `exec`'s match and `parse_byte` gain arms (proofs unchanged
because they are about the run's `Z` result). v0 certificates remain valid.

## Build order (each step gate-green; dual-oracle ratchet holds)

1. **Ownership alphabet (eager: i/a/d/m).** ✅ **SHIPPED.** `a` (alias) and `m`
   (move-out) are ground facts; `exec` is a 4-arm fold; `eager_copy_refines_safety`
   generalized from "Dec-free" to "increment-only" (MoveOut is also a −1).
   `return_list.almd` witness `id → im`. proof spine + gate + 31 tests + CI green.
2. **Calls.**
   - **2a: effect calls.** ✅ **SHIPPED.** `lower.rs` lowers `println(s)` → an
     `Op::Call{PrintStr}` that BORROWS the live string; a real printing program
     flows through the PCC chain (ownership `id` ACCEPT), and its **capability
     witness comes from real source** (`used=[Stdout]`) — undeclared, so the cap
     bound REJECTS it (`|0`): the sandbox promise catching a real host effect.
   - **2b: per-call-site capability subset rule.** ✅ **SHIPPED.** `lower.rs`
     lowers a user call `beep()` → `Op::CallFn`; the compiler folds each callee's
     reachable caps into the caller (`reachable_caps`, transitive over the call
     graph), and the proven `check_caps_cert` re-verifies `reachable ⊆ declared`
     — so `main`, with NO direct effect, is REJECTED for a Stdout it reaches only
     THROUGH `beep` (the `tcaps` witness `|0`). The checker never opens the
     callee; it does only the subset (Subset.v lever, zero new Coq). Closes the
     direct-only caps gap. Honest scope: the compiler's reachability fold is
     trusted per-build (verifying it per-edge, and an unknown callee = conservative
     reject, are the hardenings); ownership param-modes (move/borrow signatures
     for heap args) + manifest-declared caps (the ACCEPT case) remain (2c).
3. **Byte-binding: op→wasm pattern table.**
   - **3a: the table + per-build matcher.** ✅ **SHIPPED.** `proofs/Translation.v`
     formalizes the op→wasm-instruction table as a Coq object (the formal
     byte-binding `R(M,w)` — closes G1.1/G1.3/G1.4) and proves the eager-mode
     safety instance (reusing ALS); `translation_validation.rs::validate_translation`
     re-verifies, per build on the real WAT, that EVERY op's pattern is present
     AND no `rc_dec` — a strict strengthening of the bare Dec-free scan (it
     catches a renderer that drops an op). Honest scope: PRESENCE check (not the
     precise per-op byte-window bijection); the SEMANTIC realization (the runtime
     memory machine: `call $rc_dec` mutates the free-list as the abstract −1) is
     the runtime-memory-model + WasmCert-Coq library — **G1.2, the single hardest
     piece** (3b).
   - **3b: runtime memory model (the abstract-memory half).** ✅ **SHIPPED.**
     `proofs/RuntimeModel.v` models the runtime heap as a linear-memory state
     machine — an object's refcount lives in a CELL at `base + RC_OFFSET`,
     `rt_inc`/`rt_dec` are concrete memory writes — and proves `mrun_tracks_exec`:
     the cell evolves EXACTLY as the abstract refcount (`OwnershipChecker.exec`),
     faulting precisely together. Corollary `balanced_cert_no_memory_fault`: an
     accepted certificate (balanced from rc 0) is realized by a machine that
     NEVER double-frees in memory. So the abstract Dec is no longer a free-floating
     token — it is bound to a concrete memory operation (both theorems axiom-clean,
     coqchk-verified). REMAINING (3c): bind this memory machine to the actual wasm
     BYTES — that the wasm `call $rc_dec` INSTRUCTION executes precisely these cell
     writes — the WasmCert-Coq ISA layer, the last mile of G1.2.
4. **perceus mode (`r`) + leak-freedom.**
   - **4a: the `r` event + memory-level leak-freedom.** ✅ **SHIPPED.**
     `OwnershipChecker.v` gains `Reuse` (`r`) — a reuse-eligible release (perceus
     mode), folding like `−1` so `check_sound` is reused VERBATIM (zero new proof,
     like `a`/`m`); `RuntimeModel.v` proves `balanced_cert_frees_in_memory`: an
     accepted certificate leaves the runtime cell at **0 — FREED, not leaked**.
     So leak-freedom (the "ends-at-0" half of `check`, already proven at the
     witness level by `check_sound`'s `no_leak`) is now bound to real freed
     MEMORY — the property the eager-copy renderer cannot achieve (it emits no
     release) and a release-emitting renderer realizes. The extracted checker
     handles `r` end-to-end (`build-checker.sh` perceus demo). **DONE (4b) reuse
     SOUNDNESS — `check_reuse_sound`**: instead of a subset section (which would
     trust a compiler-asserted "proven-unique" SET — an inference the checker
     cannot re-derive), uniqueness is discharged by the FOLD: `exec`'s `Reuse` arm
     is tightened to valid-iff-`rc = 1`, so a Reuse of a SHARED object (rc > 1)
     FAULTS. The checker derives uniqueness from its OWN count — simpler and
     strictly sound. `RuntimeModel.step_mem`/`Termination.fuel_exec` are kept in
     lockstep (`rt_reuse`); the closed hole `iard` (a BALANCED cert that reuses a
     shared object) now REJECTs at both the Coq (`cert_shared_reuse_rejects`) and
     extracted-checker (`build-checker.sh shared_reuse.cert`) levels. **DONE (A1.2
     proof foundation) — `FreeList.alloc_not_live`**: the free-list allocator is
     modeled (bump + free-set + ghost live-set) and proven REUSE-SAFE — a valid
     allocation (the fresh frontier, or a block on the free-list) NEVER returns a
     currently-LIVE block (no reuse-after-free); INV-preservation across alloc/free
     lifts it to whole runs. This RESOLVES the A1.2 fork toward PROVE: the renderer
     slice that emits the physical free-list REFINES this model rather than adding
     trusted runtime. **DONE (A2 first slice) — `WasmRcDec.rc_dec_prog_realizes_rt_dec`**:
     the EXACT `$rc_dec` instruction tree the renderer emits, modeled as data with a
     small op-semantics (load/add/sub/store/trap over RuntimeModel's `Mem`), provably
     computes `rt_dec` — same trap, same decrement; so the release's SEMANTICS is now
     proven at the instruction-tree level (the remaining A2 gap is purely the raw-byte
     ENCODING — assembler / full WasmCert-Coq ISA). proof spine = **25 theorems
     axiom-clean**. **DONE (A1.3 cow safety) — `CowSafety.make_unique_yields_unique`**:
     the clone-on-shared discipline is modeled and proven to yield a UNIQUELY-owned
     block, so an in-place mutation of it corrupts no alias (the aliased-mutation
     class cannot occur); the sharing renderer's cow REFINES this. proof spine =
     **27 theorems axiom-clean** — A1's SAFETY classes (leak / double-free / reuse-
     soundness / reuse-after-free / aliased-mutation) are now ALL proven. **DONE
     (A1.2-render) — physical reclamation REALIZED**: `$rc_dec` at rc→0 returns the
     block to a free-list and `$alloc` reuses an exact-size head, refining `FreeList`;
     the double-free sentinel is PRESERVED (link in the dead len field, not the rc
     cell), verified on wasmtime (reuse `p1==p2` + double-free trap + value-semantics
     byte-unchanged). **DONE (A1.3-render) — SHARING + cow REALIZED**: `Dup` shares
     via `rc_inc` (no copy), `MakeUnique` is a cow (clone-on-shared, `rc_dec`-first so
     no temp), refining `CowSafety`; the rc cell now actually tracks the abstract
     refcount (1→2→1→0), exercising the proven rc machine, output byte-unchanged.
     **A1's renderer is now FULLY real-RC (share / cow / free-list / sentinel), every
     piece refining a proof, zero trusted runtime.** REMAINING (perf/encoding, not
     safety): A1.2 size-classes/walking (exact-size head-match today); A2 raw-byte
     encoding. `rc_dec`/`rc_inc` DONE (A1.1b / A1.3-render).
5. **full mode: `b` (closure-env borrow) + branch resource-state agreement** →
   control-flow + closures.

## Open risks (honest)

- Witness SIZE grows (more sections per build) — a throughput concern, not a
  trust concern; the checker stays small. Watch it doesn't make `gate.sh` slow.
- The byte-binding semantic library (WasmCert-Coq import + runtime memory model)
  is genuinely heavy and unbuilt — it is the single hardest remaining piece
  (G1.2). The table-match decomposition makes per-build cheap but does not remove
  the one-time proof.
- Branch resource-state agreement (brick 5) is the place a CFG-walk is most
  tempting — the tripwire must be guarded hardest there (encode per-edge state as
  ground facts, check join consistency locally).
