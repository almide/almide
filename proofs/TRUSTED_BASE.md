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
| `rc_dec_prog_realizes_rt_dec` | WasmRcDec.v | Closed under the global context |
| `rc_inc_bytes_encode_the_instruction_tree` | WasmEncode.v | Closed under the global context |
| `rc_inc_bytes_execute_to_rt_inc` | WasmExec.v | Closed under the global context |
| `rc_dec_bytes_trap_on_zero` | WasmExec.v | Closed under the global context |
| `rc_dec_bytes_frees_when_one` | WasmExec.v | Closed under the global context |
| `make_unique_yields_unique` | CowSafety.v | Closed under the global context |
| `check_fill_sound` | CallModes.v | Closed under the global context |
| `check_modes_cert_sound` | CallModes.v | Closed under the global context |

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
  machine + the FULL `$rc_dec` bytes' SAFETY — no double-free AND leak-freedom —
  executed on the renderer's real bytes by a general interpreter (`WasmExec`),
  operand-stack balance, and termination of the loop-free fragment — all
  kernel-checked and axiom-clean (37 audited theorems). What remains is DEPTH (the byte-binding ISA layer; and
  the RENDERER realizing the free-list/`rc_inc` — its safety MODEL is now proven,
  so that slice REFINES a proof rather than adding trusted runtime) and BREADTH
  (lowering beyond the subset: control flow, closures, stdlib) — not new properties
  on the subset.
- **The MIR-lowering WALL is empirically verified over the WHOLE v0 corpus**
  (`proofs/corpus-wall.sh`, in `make verify-trust` → CI). The entire v0 spec
  corpus (465 `.almd` files, 4195 functions reaching MIR) is driven through the
  REAL frontend → `lower_function`, and two soundness invariants are asserted on
  real source, not just hand-built MIR: (1) **the wall** — `lower_function` is
  TOTAL over the corpus: every function is `Ok` (in-profile) or an explicit
  `Unsupported` (walled); **zero panics, zero undetected refusals** (a program
  outside the value-semantics subset is rejected with a reason, never quietly
  mislowered); (2) **accept ⟹ safe on ALL THREE proven properties** — the
  kernel-proven checker re-verifies EVERY in-profile function's witness and
  ACCEPTs for ownership (no double-free/leak, `check_sound`), name totality (no
  dangling MIR reference, `check_names_cert_sound`), and capability bound (no
  undeclared host effect, `check_caps_cert_sound`). Witness granularity differs
  by property and the gate respects it: the ownership checker FOLDS over heap
  objects (one fold over the whole stream), while the name/capability checkers
  parse a SINGLE `<superset>|<subset>` witness, so those are re-verified one
  function at a time (a batched file would wrongly fold every function's ids into
  one superset — surfaced and encoded while building this gate). This is the
  step-4 "continuous corpus
  verification = the definition of parity" in its honest first form: it does NOT
  yet claim the *completion definition* (the proven profile accepting the full
  corpus), it establishes the *mechanism* that measures progress toward it and
  proves the boundary is a wall, not a hole. **Today's honest coverage: 4083/4195
  functions in-profile (97%) for ownership+names (caps-VERIFIED is the lower, parity-binding 3528 — see caps note)** (the value-semantics subset,
  plus **`Range` values, CLOSURE values, and unresolvable `Method`/`Computed`-target calls** (`f(0..n)`,
  `var g = (x) => …`, `obj.method()`, `(g)()` — a `Range` and a CLOSURE value (a fresh heap env) are fresh values; an unresolvable callee
  (dispatch / closure value not known here) is modeled as a DEFERRED fresh value (a
  heap `Alloc{Opaque}` / scalar `Const`), its receiver's/args' calls captured but the
  method/computed call itself ELIDED, so the `ir_calls > mir_calls` gate taints the
  function caps-unverified — honest, the callee's capabilities are unknown), plus
  **error operators** (`e!`/`e?`/`e ?? d`/`e?.field` yield a FRESH value — the
  unwrapped/defaulted/mapped result, deferred like every Opaque; the operand's
  calls are captured. Almide has NO `try`/`catch`: `e ?? d` (unwrap-or-default),
  `e?` AS `ToOption` (`Result → Option`) and `e?.f` (optional chaining) are TOTAL
  value maps with NO control flow — always safe to defer. **`e!` (`Unwrap`) and the
  effect-fn auto-`?` (`Try`) EARLY-RETURN `Err`**, and that early return was a v0 WASM
  codegen LEAK: the Err path is a bare `return_` that jumps PAST the function's
  scope-end heap frees (the Perceus rc_decs sat only at the terminal `Ret`), so a heap
  local live at the early return leaked (Rust was always leak-free: `e!`→`?`→scope-exit
  `Drop`). A deferred-continue cert is balanced (the checker proves it no_leak), so
  certifying that shape would have been **accept-but-unsafe**. **FIXED** (the underlying
  v0 bug, not merely walled): the wasm emitter now frees the heap locals LIVE at each
  `Try`/`Unwrap`/`Fan` Err-path `return_` before it propagates — `emit_early_return_decs`
  reads Perceus's own rc_dec liveness (a running owned-heap set, push on heap Bind / pop
  on RcDec, excluding env-borrows + donate-only `__*` temps; the returned Err ptr is a
  scratch temp, never freed). Empirically verified leak-free (a 100k-iteration ×100KB
  err-loop completes instead of OOM-ing) AND double-free-free (the whole 260-file wasm
  corpus + the cross-target byte gate stay green). So the early-return shape now LOWERS
  on both targets, faithful — the move/consume model never moves a user heap local
  without a Dec (alias-by-RcInc, verified), so the running set is the exact live set.
  See docs/roadmap/active/v0-unwrap-early-return-leak.md), plus
  **destructuring patterns** (a `match` arm's `Some(x)`/`Ok(v)`/`Foo(a,b)` and a
  `let Foo{..}=`/`let (a,b)=` bind their payloads CONTAINER-GRAIN — a heap binding
  aliases the whole subject (`Op::Dup`, reusing the proven `a` event; element/payload-
  PRECISE identity needs the layout brick, deferred like every Opaque), a scalar
  binding is a `Const`; a record shorthand field is walled),
  plus **higher-order pure combinators** (`list.map`/`filter`/`fold`/… with a
  closure, in VALUE or EFFECT/statement position — a pure combinator INVOKES the
  closure during the call and discards it, so it never escapes; the closure
  ARGUMENT is handled by its CAPABILITY (a `Lambda` body's calls, a
  `ClosureCreate`/`FnRef` callee → effect markers, so the closure's caps reach the
  witness and the `mir<=ir` gate taints a nested-higher-order/unanalyzable body)
  with its value DEFERRED and captures BORROWED; an OPAQUE function value
  (unanalyzable caps) is WALLED; a value result is fresh-owned, a Unit/scalar effect
  result carries no ownership, a discarded heap result is allocated and dropped),
  plus **`for`/`while` loops** (a PER-ITERATION scope frame makes one modeled
  iteration internally balanced ⟹ N runtime iterations are leak-free for any N, NO
  loop op; a heap iterable is borrowed or materialized via `lower_call_args`, the
  loop variable aliases the container per iteration (`Op::Dup`, container-grain) or
  is a scalar `Const`; a `break`/`continue` (nullary, value-less, label-less — Almide
  has no `break x`/`return`) over a SCALAR-only frame is admitted as a no-op (the frame
  holds no heap handle, so a real early exit skips no Drop = no leak on either target),
  but over a HEAP frame (a heap loop variable's `Op::Dup`, a heap body local) it is
  WALLED — the v0 wasm backend frees AFTER the break branch target, so a real early exit
  would LEAK the per-iteration heap handle (an accept-but-unsafe case caught by
  adversarial verification, not shipped); a heap reassignment — the loop
  ACCUMULATOR `acc = acc + [x]` — is DEFERRED, not rebound: `acc` keeps its still-live
  pre-loop handle across iterations, so no iteration drops a handle a later iteration
  reads (memory-safe; the accumulation itself is deferred like every Opaque) and is NOT
  a frame handle, so a scalar loop + heap accumulator + break is admitted; a scalar
  reassignment `i = i+1` is admitted), plus **`if`/`match`
  control flow** (statement / scalar- / Unit- / HEAP-tail and
  heap-bind position — arms LINEARIZED into the flat op stream with a per-arm scope
  frame, NO branch op: each arm internally balanced + vacuous on the other path; the
  result is one merged slot the caller emits — discarded (Unit/statement), a `Const`
  (scalar), or a fresh `Alloc{Opaque}` (heap, memory-safe by construction, its value
  CONTENT deferred like every Opaque); a heap `match` subject is MATERIALIZED (a
  fresh value into an owned temp dropped at scope end, a tracked var borrowed); a
  payload-binding pattern binds container-grain (see below); an arm guard is WALLED; an
  arm that reassigns a HEAP variable is DEFERRED — the var keeps its pre-branch handle,
  so a post-branch read never dereferences a handle the arm dropped (no path-dependent
  UAF)), plus
  **tuple destructuring** (`let (a,b) = (x,y)` component-wise, or `(a,b) = t`
  aliasing the container per component), **field/element assignment** (xs[i]=v / r.field=v → MakeUnique copy-on-write), **println of any heap arg** (`println("x")`/`f()`/`a++b` materialized + borrowed — reaches Stdout so caps-unverified, honest), and **reassignment** — `x = v` rebinds `x`; the old binding rides to scope-end
  and is dropped exactly once (a conservative lifetime extension, never a
  double-free); a read of the old `x` inside `v` borrows the still-live old
  handle (lowered before the rebind), never a UAF. Also incl.
  plus **field/element extraction** — `xs[i]` / `r.field` / `t.0` / `m[k]`: a
  scalar result is an unambiguous copy → `Const`; a HEAP result ALIASES the
  CONTAINER via the existing `Op::Dup` (the v1 container-grain field access) — the
  extracted value is a second handle on the whole container, which keeps it alive
  for the value's lifetime (a conservative lifetime widening, never a UAF), reuses
  the proven `a`/alias event so the Coq checker + backing gate are UNCHANGED, and
  honestly defers field-PRECISE aliasing (the value's own object) to the layout
  brick (LayoutId is a placeholder today); a nested-container extraction (`a.b.c`,
  no single tracked `src`) stays walled. Also incl.
  expression-bodied functions, direct heap-literal returns, direct
  named-call-result returns, functions taking **borrowed heap parameters**,
  **first-order pure stdlib `Module` calls**, **nested CALL arguments** —
  `f(g(x))` / `assert_eq(g(x), …)` materialized into an owned temp, borrowed into
  the outer call, dropped at scope end — **literal CALL arguments** — `f("x")` /
  `f([1,2,3])` / `f(3.14)`, a heap literal via `Alloc` or a scalar literal as a
  `Const` — **Option·Result constructors** — `Some(x)` / `Ok(e)` / `None` /
  `Err(e)`, heap variants materialized like a container literal — and **BinOp /
  UnOp** — `a+b` / `s1++s2` / `-n`, a FRESH computed value (heap concat via
  `Alloc`, scalar arithmetic/logic as a `Const`; operands carry their own
  ownership, value-semantics so the result is never an alias)); the rest are
  walled with a per-feature
  `Unsupported` histogram that names the next surface to admit (largest buckets
  now name exact stdlib functions: `list.map`/`filter`/`fold` with a closure
  argument — the higher-order brick — and call-as-argument materialization).
  **Stdlib `Module` calls use a PURE-ONLY admission**: a `<module>.<func>` call
  lowers to an `Op::CallFn` only when first-order (no closure argument) AND the
  callee is provably PURE (reaches no host capability). This is forced by
  capability soundness: the proven checker derives `used` capabilities only from
  `Op::Call`'s typed `RtFn`, so a `CallFn` to an effectful stdlib name would
  silently omit its capability from `used` = accept-but-unsafe. Pure callees reach
  the empty capability set, which the empty `used` witness faithfully represents;
  effectful (`fs`/`http`/`net`/`io`/`env`/`process`/`random`/`zlib` via the
  `effect fn` keyword) and impure-plain (`datetime`/`args`/`mem`/`testing` — host
  reach WITHOUT the keyword) and higher-order calls are WALLED, never lowered. The
  purity registry (`crates/almide-mir/src/purity.rs::PURE_MODULES`) lives entirely
  in the untrusted emitter; a **drift gate** (`proofs/check-stdlib-purity-registry.sh`,
  in `corpus-wall.sh`) re-derives the effectful set from `stdlib/*.almd` and fails
  if any admitted module ever gains an `effect fn`, or if a stdlib module is left
  unclassified — so a pure→effectful drift cannot silently ship. **The capability
  property is checked TRANSITIVELY and honestly SCOPED to Stdout.** A function's
  empty capability witness is only emitted (claimed caps-safe) when a conservative
  fold (`certificate::reaches_capability_or_unknown`) proves it reaches no Stdout
  across every `Op::CallFn` edge: an in-profile callee is folded; a pure stdlib
  `Module` call, a variant constructor, or a known Stdout-free builtin
  (`assert*`/`eprintln`/`panic`/`to_string` — these reach stderr/abort, NOT
  Stdout) is free; ANY other unknown callee (a walled or cross-file user function)
  TAINTS, so the function is reported `caps-unverified` (3528/4083 verified, 555
  unverified) rather than falsely accepted. **The gate verifies the REAL
  capability-bound property `reachable ⊆ declared`** (exactly what
  `proofs/CapabilityBound.v` proves), not a degenerate "reaches no capability at
  all". `lower_function` lowers each function's effect signature into a
  `declared_caps` bound — an `effect fn` declares `{Stdout}` (the one modeled
  cap), a pure `fn` declares ∅ — and the classifier folds the transitive
  *reachable* cap set (`certificate::reachable_caps_or_tainted`, returning `None`
  on any taint and `Some(set)` only when every edge is analyzable), then emits
  `<declared>|<reachable>` for the proven `check_caps_cert` to re-verify
  `reachable ⊆ declared`. So an effectful function is VERIFIED AGAINST ITS OWN
  declared bound (a printing `effect fn` is accepted because it declared the
  Stdout it uses), not merely excluded for touching a capability. A function that
  reaches a capability it did NOT declare — e.g. a non-`effect fn` that prints,
  since the frontend `is_effect` flag does not cover every Stdout reach — fails
  `reachable ⊆ declared` and is conservatively caps-unverified, never falsely
  accepted. **A call ELIDED by Opaque lowering** (a list element, ctor payload,
  BinOp operand, or scalar value — its sub-expressions are not lowered) is a
  second caps blind spot the fold cannot see. `lower::record_elided_calls` SURFACES
  each such call as a bare EFFECT MARKER `Op::CallFn{dst:None, args:[], result:None}`:
  the existing handlers treat a result-less, dst-less call as a PURE EFFECT — it
  emits NO ownership event and references NO value (so ownership/name witnesses and
  the `+1`-backing gate are unchanged), yet the caps fold matches it by NAME and
  folds the callee transitively. Only a SOUNDLY-modelled call is surfaced — a
  first-order `Named` call (the fold opens or honestly taints it) and a first-order
  PURE `Module` call; a higher-order / effectful-`Module` / `Method` / `Computed`
  call is left elided so the gate keeps the function tainted (no FALSE de-taint).
  SOUNDNESS BACKSTOP: a marker is recorded only at a wholesale-elided position, so
  the MIR call count can only rise TOWARD the IR's — the corpus gate asserts
  `mir_calls <= ir_calls`, making a double-count (the one way a marker could mask a
  real elision and falsely de-taint) a WALL BREACH, structurally impossible to
  ship. A function whose source STILL has more call nodes than its MIR (an
  un-materializable elided call), or any transitive caller of one, stays
  conservatively TAINTED — so the caps-verified count is HONEST, never over-claimed.
  This closes the direct-witness hole
  (`reachable_caps`'s honest-scope: an unknown callee contributed ∅). HONEST
  SCOPE: only `Capability::Stdout` is modeled, so the property is "no undeclared
  **Stdout** effect"; stderr, abort, fs, net are real host effects not yet named
  (a wider `Capability` set + frontend-lowered `declared_caps` is a later brick).
  **Heap parameters use
  a BORROW-BY-DEFAULT calling convention** (the caller owns the reference; a param
  contributes no owned `+1` to the certificate). This is the only convention sound
  under the current runtime: an owned-param `+1` would be SYNTHETIC — unbacked by
  any runtime `Alloc`/`rc_inc` — the gate-blind use-after-free class. Returning or
  releasing a borrowed param without first acquiring its own reference (a `Dup`)
  is explicitly walled (`returning a borrowed param directly`), and its
  certificate would be a release at rc 0 which the proven checker faults. A
  **non-recurring backing gate** (in `corpus-wall.sh`, mirrored by a unit test)
  asserts every certificate `+1` is backed by a real `Alloc`/`Dup`/heap-result —
  so re-introducing a synthetic param ownership is structurally impossible to
  ship. Note: the proven Coq checker is UNCHANGED by this brick (only the cert
  *emission* dropped the unbacked param `i`), so the checker-size invariant holds.
  Coverage is REPORTED, never gated on a brittle exact count — only the soundness
  invariants are hard, plus an anti-collapse floor (≥1 in-profile witness must
  reach the checker, so a silent coverage collapse to zero fails the gate).
- **Index-bounds memory safety — a found-and-walled hole.** The ownership checker
  proves the RC properties but does NOT check list-index bounds; the renderer's
  `$list_set`/`$list_get`/`$elem_addr` did no bounds check either, so an
  out-of-range index would compute an address OUTSIDE the block and a store there
  would CORRUPT memory — an accept-but-UNSAFE hole (a different memory-safety axis
  than RC). CLOSED with a WALL (not silent): `$elem_addr` now traps (`unreachable`)
  on `idx < 0 ∨ idx ≥ cap`, so every element access is bounds-checked and OOB is a
  controlled halt, never corruption (verified on wasmtime: OOB traps, in-bounds does
  not; the value-semantics output is byte-unchanged — all real accesses are
  in-bounds). The SAME audit closed a second unbounded-write gap: `$print_list`
  built the output line in a fixed buffer `[SCRATCH_ADDR, HEAP_BASE)` with no bound,
  so a very long list would overflow the line into the heap — it now traps before
  appending an element that would cross `HEAP_BASE` (the print-buffer wall). The
  deeper fix — the checker REJECTING OOB statically (a wall at check time, not run
  time) — is a later brick; today the runtime trap is the wall.
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
  skipped), beyond the fixed trap pattern; PLUS `global.get`/`set` (the `$freelist`
  global as a reserved cell, round-trip proven) and INDEXED locals + `local.set`
  (a locals env, for the `$rc` temp). With that, the FULL `$rc_dec` bytes — free-list
  included, with `global.get`/`set` — are GROUNDED against wat2wasm
  (check-wasm-bytes.sh) and BOTH rc_dec SAFETY properties are EXECUTED-PROVEN on
  those real bytes by the general interpreter: `rc_dec_bytes_trap_on_zero` (NO
  double-free — releasing an already-0 cell TRAPS) AND `rc_dec_bytes_frees_when_one`
  (LEAK-FREEDOM — a valid release leaves the rc cell at 0, the block freed), run on
  the renderer's ACTUAL `$rc_dec` byte sequence. Per the trust model ("we protect
  SAFETY, not functional correctness"), these two are the rc_dec byte-binding that
  matters; the free-list push's ORGANIZATION (which list the freed block joins) is
  functional, not a safety property. The interpreter's EXECUTION is GROUNDED both
  ways now: the bytes against wat2wasm, AND the execution against the production
  engine — a wasmtime differential test (`rc_cell_values_match_the_interpreter_on_-
  wasmtime`) confirms the REAL engine computes the same rc cell values run_g predicts
  (rc_inc 1→2, rc_dec 1→0). So the residual shrinks from "trust run_g = the wasm
  spec" to "wat2wasm/wasmtime = the spec" — production tools at the same trust level
  as the rest of the toolchain. NOT yet done: the functional list-op runtime (the
  bootstrap-runtime debt, to be SELF-HOSTED in Almide through the proven path, #30,
  not hand-bound); and a full in-Coq WasmCert-Coq ISA that would close the
  interpreter↔spec residual entirely (vs. grounding it against wat2wasm/wasmtime).
- **One real `.almd` now flows end-to-end** (`proofs/fixtures/return_list.almd`
  → the actual frontend → MIR → proven checker, for ownership + names — weekly
  indicator ① 0→1). The lowering covers only the value-semantics subset (heap
  literals, alias, index-assign copy-on-write, scalar/heap-move-out return — NO
  calls or control flow yet, #29), so the broader reject cases and the
  capability witness are still REPRESENTATIVE MIR shapes (emit_cert.rs).
- **Extraction is trusted** (item 2 above) until CertiCoq/CompCert.
- **Single independent checker.** Diversity (≥2 independent checkers) is brick 6.

## Proven-vs-trusted boundary map (flight-evidence-gaps F3-1)

The one-page answer to the auditor's first question: **what exactly is proven,
and what is trusted engineering?** (2026-07-03 — written after a hands-on pass
found five output-breaking bugs, ALL in the trusted zone.)

```
                         ┌──────────────────────────────────────────────┐
  .almd source ──parse──▶│ FRONTEND (trusted Rust)                      │
                         │  check / lower / optimize / mono / ir_link   │
                         └──────────────┬───────────────────────────────┘
                                        │ IR
                         ┌──────────────▼───────────────────────────────┐
                         │ MIR LOWERING (trusted Rust, WALLED)          │  ← the five 2026-07-03
                         │  lower_function + pre-desugars               │    bugs lived HERE
                         │  emits per-function WITNESSES                │
                         └──────────────┬───────────────────────────────┘
                                        │ MIR + certificates
                         ┌──────────────▼───────────────────────────────┐
                         │ CHECKER (Coq-PROVEN kernel)                  │  ← accept ⟹ ownership ∧
                         │  check_all_sound / names / caps-transitive   │    names ∧ caps (37 thms)
                         └──────────────┬───────────────────────────────┘
                                        │ accepted MIR
                         ┌──────────────▼───────────────────────────────┐
                         │ RENDERER render_wasm (trusted Rust;          │
                         │  rc_dec/rc_inc byte trees PROVEN — WasmExec) │
                         └──────────────┬───────────────────────────────┘
                                        │ wat
                              wasmtime (UNQUALIFIED tool)
```

What the proof gives: an ACCEPTED function cannot double-free, leak, dangle a
name, or reach an undeclared capability. What the proof does NOT give: that the
lowering picked the RIGHT semantics — a certified-sound function can still print
the wrong string. That gap is covered only by differential evidence
(output-parity baseline + org suites), whose reach is measured by
`proofs/coverage.sh`.

### The five 2026-07-03 trusted-zone bug classes and their regression pins

| class | what broke | pin (gate or fixture) |
|---|---|---|
| match linearization ran BOTH effectful arms | wrong output under a green wall | call-bearing-arm WALL guard (`lower_branch`) + `mutual_recursive_types` / grammar dispatch fixtures in the parity baseline |
| never-err strip vs REAL Result blocks | record fields read off a Result handle | `effect_assign_unwrap` (all five legs) in the parity baseline + lifted/self strip gate |
| scalar module-globals lowered to Const-0 | every use of a top-level `let` read 0 | const-init materialization + WALL for call-bearing inits; `top_let_test` in the baseline |
| lifted lambdas lost variant/global registries | `filter` dropped every element | sub-ctx inheritance (binds.rs); `closures_and_variants` in the baseline |
| `$_start` left an explicit-Result main on the stack | invalid wasm for every Result-main CLI | `$_start` tag-read/drop; grammar CLI matrix byte-verified |

| `prim.handle(<literal>)` resolved to deferred-Const **0** | the generated case tables read address 0 — every lookup silently missed (found while landing them, 2026-07-03) | literal materialization in the prim arg loop (calls_p4) + `string_case_unicode` in the parity baseline |

Pattern across all six: a REGISTRY or CONVENTION (tracking sets, layout tables,
calling convention, the scalar deferred-Const fallback) drifted between producer
and consumer inside the trusted zone. The deferred-Const fallback (a scalar tail
outside the subset lowers to `Op::Const` zero, calls elided) is the remaining
SYSTEMIC instance of this pattern — it trades output correctness for caps
coverage by design, which was sound when scalar fns were never output-observed
and is NOT sound now. Retiring it (wall instead of Const on the value-observed
paths) is recorded in flight-evidence-gaps F2. The structural fix direction is certificate-format-v1 / value-rc-cert
(shrink the trusted zone); the tactical direction is the wall discipline (every
consumer of an untracked/unknown shape must wall, never guess) — which is now
enforced at the match-linearization and Map-repr routing sites.

## Use-relativized completeness

Completeness is declared per use, not absolute. Today the proven property set is
complete for **memory-safety-of-the-ownership-fragment under the eager-copy
realization** (no double-free). It is NOT a claim of absolute-semantics coverage
(that diverges — CompCert-grade). The receipt names which use each artifact is
proven for.
