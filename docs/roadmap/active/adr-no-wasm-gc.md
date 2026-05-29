<!-- description: ADR — Almide does not adopt WASM-GC; stays linear-memory + Perceus, cherry-picks non-GC WASM features -->
# ADR: Almide does not adopt WASM-GC

**Status:** Accepted · **Scope:** WASM backend, language memory model · **Supersedes:** the "second GC backend" idea floated in wasm-v2-correctness-bugs discussion.

## Context

The WASM 3.0 baseline (2025) includes the GC proposal (struct/array/i31),
typed function references + `call_ref`, tail calls (`return_call`), exception
handling (`try_table`/`exnref`), multi-memory, and memory64. Most high-level
languages targeting WASM in 2026 (Kotlin/Wasm, Dart, OCaml, Scheme/Hoot, Java)
use **WASM-GC** to represent their heap.

While hardening the v2 engine we hit a class of correctness bugs from
hand-computed memory layout (named-record fields collapsing to offset 0;
`Unknown` element types guessing i64 width — see
[wasm-v2-correctness-bugs.md](wasm-v2-correctness-bugs.md)). WASM-GC would make
that bug class structurally impossible (GC structs/arrays are typed; the engine
would never compute byte offsets) and would obviate the not-yet-built Phase 2
Perceus-WASM runtime. That raised the question: should Almide adopt a WASM-GC
backend?

## Decision

**No. Almide stays linear-memory + Perceus and does not adopt WASM-GC.** We
cherry-pick the **non-GC** parts of WASM 3.0 that strengthen the existing
backend, and we shape the language so the workloads where GC would help do not
arise.

## Rationale

1. **GC is orthogonal to the mission.** Almide's metric is *LLM modification
   survival* — a frontend/semantics property. GC vs linear memory is a backend
   memory strategy that advances the mission by zero, while costing an enormous
   implementation budget.

2. **It would fork the memory model and break conceptual integrity.** One IR
   compiles to Rust *and* WASM; the IR carries Perceus `RcInc`/`RcDec`/`Clone`/
   `Borrow`. A GC backend would ignore all of that — it is not a second backend
   but a second compiler, diverging from the Rust target the model is validated
   against.

3. **It would discard the proven Perceus guarantee.** `almide-perceus-belt`
   proves heap is freed exactly once (Lean, 23 theorems). GC replaces that with
   "trust the host GC" — a regression in the very property Almide sells. Koka
   (Perceus' origin) and Roc (Perceus-family + WASM) both deliberately avoid
   host GC for the same reason.

4. **GC's main technical edge — cycle collection — is moot for Almide.** Almide
   is value-oriented; without first-class mutable back-references, cycles cannot
   form, so RC never leaks. We *choose* not to add mutable cyclic structures
   (see Consequences), which keeps this true by construction.

5. **The bug class GC would erase is already fixed properly.** The offset/width
   corruption was the engine guessing instead of resolving types; the fix
   (resolve types, reject on unresolved — Cat 1 & 2 in the correctness catalog)
   is done. The verifier + LayoutRegistry + honest-rejection discipline +
   differential gate (`scripts/wasm-v2-diff.sh`) are the correctness *moat*; GC
   would throw that investment away rather than build on it.

6. **Portability and determinism.** Linear-memory WASM runs on every runtime
   (wasm3, WAMR, embedded, 2017-era browsers); WASM-GC needs recent hosts.
   Perceus gives GC-pause-free, deterministic freeing (audio/games/real-time).

## Alternatives considered

- **Full WASM-GC backend** — rejected (reasons above).
- **Dual backend (linear+Perceus *and* GC)** — rejected: doubles the compiler,
  splits the memory model, and the GC half serves no mission goal.
- **GC only for host interop (DOM)** — unnecessary: `externref` (reference
  types, **not** GC) holds opaque host handles, and Component Model / WASI P2
  cover typed host boundaries, without an Almide-side GC heap.

## Consequences

**We commit to:**

- **Finish the Phase 2 Perceus-WASM runtime** (recursive `rc_dec` + free-list
  reuse). This is the honest cost of not using GC, paid to keep determinism +
  the proof.
- **Cherry-pick non-GC WASM 3.0 features** into the linear-memory backend:
  `return_call` (tail calls), typed `funcref` + `call_ref` (typed closure
  dispatch; env stays linear-memory + Perceus), `try_table`/`exnref` (native
  `?`/Result/effect propagation), Component Model / WASI P2 (host interop).
- **Language stance — no first-class mutable cyclic structures.** Graphs are
  expressed with a stdlib `Arena[T]`/`Slab[T]` + typed `Handle[T]` (index
  newtype). This is mission-aligned (no aliasing/lifetime traps, high survival),
  maps cleanly to Rust (avoids `Rc<RefCell<>>`) and WASM linear memory, keeps
  Perceus leak-free, and is what the compiler already does (IR uses `VarId`/
  indices, not parent pointers — dogfooded). "We don't do cyclic mutable
  aliasing; use handles" is a feature of a correctness-first language.
- **Host interop without GC** — `externref` opaque handles + Component Model /
  WASI P2. Almide targets standalone modules / plugins / edge / embedded / CLI,
  **not** being a DOM framework.

**Priority shifts:**

- **region inference** → demoted from "correctness necessity" to "optimization"
  (with no cycles, Perceus alone doesn't leak; regions are a speed lever).
- **Component Model / WASI P2** → kept, scoped to system/host boundaries.
- **WASM-GC** → out of scope. Reopen only if the language adds first-class
  mutable cyclic structures or dense GC-host interop — both currently rejected.

## Revisit triggers

Reopen this ADR only if a future decision adds (a) first-class mutable cyclic
data, or (b) a goal of dense interop with a GC-managed host heap. Absent those,
WASM-GC stays out.
