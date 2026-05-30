<!-- description: Almide's ideal form — three reinforcing loops (guarantees, correctness, self-hosting) closing on a self-described, AI-maintainable language -->
# Almide — The Ideal Form

The north star above the individual roadmap items. Almide's mission is **the
language LLMs write most accurately** (modification survival rate). The ideal
form is the fixed point where three loops close and feed each other.

This doc sits above, and is realized by:
- [Guarantees Charter](guarantees-charter.md) — Loop 1
- [No WASM-GC ADR](adr-no-wasm-gc.md) + [correctness gaps](correctness-guarantee-gaps.md) + the v2 engine — Loop 2
- self-hosting (this doc, §Loop 3)

## The three loops

### Loop 1 — Guarantees (eliminate the worry)

Every category an AI would otherwise have to reason about — memory, null,
errors, **effects**, aliasing, bounds, races — is either *unexpressable* or
caught by an **inferred, local, one-edit-fixable** check. The AI reasons only
about the problem, never about the language's traps.

A function's signature becomes a complete mechanical contract: **types + effect
row + refinements**. This is the Charter's "proof without puzzles"; Perceus is
the template (proven, inferred, zero-annotation).

### Loop 2 — Correctness (emit only what's proven)

A single typed IR lowers to **Rust** (native: ownership / Perceus, no GC, fast)
and **WASM** (linear memory + Perceus, portable, deterministic, non-GC WASM 3.0
features). The engine emits only what it can prove correct — stack-effect
verifier, LayoutRegistry, honest rejection of unresolved layout — and a
**differential gate** (`scripts/wasm-v2-diff.sh`) guarantees the two targets
agree. "Well-typed source → correct binary" is mechanically backed end-to-end.

### Loop 3 — Self-hosting (the artifact is the proof)

The stdlib, then the tools, then the compiler itself are written in Almide. The
compiler is the canonical hard program; if an AI maintains it with high
modification survival, the mission is **proven by the artifact, not claimed**.
Hot paths keep native escape hatches (`@inline_rust`). Almide becomes its own
largest, most demanding test corpus.

## The fixed point

> A **self-described** language whose **mechanical guarantees** make its own
> (large, real) source **AI-maintainable**, compiling **correct-by-construction**
> to both a native and a portable target.

The loops reinforce: guarantees make self-hosting tractable; self-hosting
stress-tests guarantees and codegen on serious code; correct codegen makes
self-hosting trustworthy. Each closed loop is evidence for the thesis.

## What it looks like in use

- Humans and LLMs write Almide. The compiler, stdlib, JSON, formatter, LSP are
  Almide; a few native primitives use `@inline_rust`.
- `almide build --target rust` → fast native (no GC). `--target wasm` →
  portable, deterministic module (no GC host needed). **Same source, same
  semantics, differentially verified.**
- An AI given a feature request edits Almide source; the guarantee layer rejects
  mistakes locally and actionably; survival is high because the traps don't
  exist.

## Deliberately NOT in the ideal (the discipline is half the design)

- **No GC** — determinism, portability, and the Perceus proof are kept; cycles
  are made unexpressable (`Arena`/`Handle`), not collected. ([ADR](adr-no-wasm-gc.md))
- **No totality / dependent types** — would break "proof without puzzles."
- **No first-class mutable cyclic data.**
- The language stays **small and predictable**: guarantees come from inference +
  by-construction impossibility, never from a clever-but-unpredictable checker.

## Self-hosting ladder (concrete milestones)

1. **stdlib in Almide** — json → value → more; v2-compiled, differentially
   verified. *(in progress: `research/selfhost/json_parser.almd` — a JSON parser
   in Almide. Builds + runs on Rust/legacy-WASM; the v2 engine builds it but
   traps at runtime on recursion + heap-returning functions. Each isolated
   feature works (BindDestructure, variants, int.parse, list concat); the
   recursive-parser combination faults — the next v2 gap, likely the
   Perceus-WASM RC path or recursive call/param handling. NB: the build-time
   `wasmparser::validate` safety net catches structurally-invalid output, not
   runtime traps — the differential gate is the net for those.)*
2. **tools in Almide** — formatter, lint, LSP pieces.
3. **compiler passes in Almide** — one nanopass, then more.
4. **full compiler in Almide** — bootstrap: the Almide compiler compiles itself.

Milestone 4 is the symbolic fixed point.

## How the ideal is reached

Not by a leap — by **never shipping the unproven**. The disciplines already in
place are the method: incremental (stdlib-first), the Charter's four-condition
gate on every guarantee, the differential gate on codegen, and honest rejection
everywhere a layout/type can't be proven. The ideal form is the accumulation of
those, not a rewrite.
