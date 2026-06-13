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
   - **2b: user-function call signatures + per-call-site subset rule** (pending):
     each function carries a SIGNATURE (param move/borrow modes, return mode,
     declared caps); a call site is checked against the signature by the subset
     rule — the checker never opens the callee. Unblocks heap-param programs and
     manifest-declared caps (the ACCEPT case).
3. **Byte-binding section + op→wasm pattern table** (per-build matcher; the
   runtime memory-model refinement library is the heavy parallel track).
4. **perceus mode: `r` + the reuse-uniqueness subset section** → leak-freedom on
   the real-RC renderer.
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
