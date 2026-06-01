<!-- description: Determinism/Purity Belt — a Perceus-analog that makes the compiler deterministic & target-portable by construction -->
# The Determinism / Purity Belt

> A Perceus-analog for compiler **output determinism + target portability**. AlmidePerceusBelt makes "if it emits, RC is balanced" a theorem; this makes "if it emits, the output is a pure function of (IR, target)" enforced by construction — so a wasm32 (browser playground) compile can't crash or diverge from x86-64.

## Why

The compiler runs compiled to **wasm32-unknown-unknown** in the browser playground. Two bugs shipped that ALL the verifiers (stack-effect, Perceus, the spec suite) missed, both the SAME class — codegen read a non-deterministic / impure / non-portable source instead of being a pure function of `(IR, target)`:

1. `std::time::Instant::now()` called unconditionally (ALMIDE_PROFILE timing) → panics on wasm32-unknown-unknown (no clock) → every in-browser compile crashed. (v0.23.13 fixed the call; the Belt makes it un-writable.)
2. `HashMap`/`HashSet` iteration assigned WASM function/table/index/offset order; hashbrown iteration is host-pointer-width dependent (h2 = `hash >> (usize_bits-7)`) AND `Sym`-keyed maps depend on intern order (process history). → wasm32 emits a divergent/trapping module.

The spec suite only runs the compiler on x86-64; the first determinism gate used wasm32-wasip1 (which HAS WASI time) and so missed bug #1. The Belt is the structural answer.

## The four layers (strong inner gates carry the load; broad outer gates catch what types can't)

| Layer | What | Strength | Status |
|---|---|---|---|
| **L1 Purity wall** | the compile-path crates can't *name* a clock/RNG/thread/atomic-counter source (grep gate `scripts/check-forbidden-impurities.sh` + planned `clippy.toml` `disallowed-methods`); timing only via `almide_base::profile::ProfileTimer` (cfg-gated, wasm-safe) | lint/deny (textual, alias-fragile — **not** the Perceus-grade type-state) | **DONE (Phase 0/1)** |
| **L2 `DetMap`/`DetSet` prelude** | the only collections a codegen-path crate may *iterate* are content-ordered (BTreeMap-backed); plain `HashMap` survives only for lookup-only caches behind an audited `// DET-ESCAPE` | type-state | roadmap |
| **L3 `Canonical<IrProgram>`** | a terminal `CanonicalizePass` sorts the emit-ordered function Vecs into content-derived order; `emit` accepts only `Canonical` (sibling of `Verified`), and `emit_wasm::emit` is now `pub(crate)` so the gate can't be bypassed. Re-normalizes even if an upstream pass built a Vec in hash order | type-state (true Perceus spine) | **DONE** |
| **L4 Runtime tripwire** | `check-host-determinism.sh` (wasm32-wasip1) + `check-browser-determinism.sh` (wasm32-unknown-unknown via node) byte-compare the compiler's output to native | test (open-world backstop, permanent) | **DONE** |
| L5 Lean proof | `canonicalize` is idempotent & input-order-independent | proof | optional |

## Done so far (v0.23.13 + this work)

- **L1**: `almide_base::profile::ProfileTimer` (the one sanctioned, wasm-safe clock); the three codegen clock sites route through it; `scripts/check-forbidden-impurities.sh` fails CI on raw `std::time|Instant|SystemTime|thread::spawn|thread_rng|fastrand|AtomicU64|fetch_add` in `crates/{almide-codegen,almide-frontend,almide-optimize,almide-ir}/src` (excluding `/generated/` `/runtime/` — those are runtime code emitted into the user's native program). Wired into the `checks` CI job.
- **The confirmed-live data-order leak**: `record_fields`/`variant_info` → `BTreeMap`; `closures.rs`/variant-eq/name-section sorted; **`almide-optimize/src/mono/mod.rs` `all_instances`/`new` → `BTreeMap`** (specialized-function append order, → WASM function indices, was HashMap-ordered → host-width + intern-order dependent).
- **L4**: both gates in CI; `spec/wasm_cross/` fixtures incl. `playground_default.almd`, `generics_mono.almd` (monomorphization), and `anon_records_and_fusion.almd` (anonymous-record naming + egg-fused pipelines).

## L3 shipped (this PR) — the type-state spine

A grounded codebase audit (emit boundary + Sym-iteration surface + egg liveness + every emit-ordered collection) drove the implementation and **corrected two premises**:

- **`CanonicalizePass`** (`pass_canonicalize.rs`): terminal WASM pass, stable-sorts `program.functions` and each `module.functions` by `(is_test, name)`. Safe because functions resolve by name at emit (sorting permutes WASM func indices but preserves semantics). It deliberately does **not** reorder `program.modules`/`top_lets` (top-level init is sequential and a later `let` may observe an earlier one — order is semantic) or `type_decls` (variant-tag order). Idempotent.
- **`Canonical<'a>` type-state** (`lib.rs`, sibling of `Verified`): `Canonical::certify` consumes a `Verified` (so RC-verified is a prerequisite) and asserts `is_canonical`. WASM emit accepts only `Canonical`, and **`emit_wasm::emit` is demoted to `pub(crate)`** — closing the prior bypass where any caller could call `emit` directly. Codegen path is now `codegen → Verified → Canonical → emit`.
- **egg counter — was a *live* leak, now fixed**: `EGG_FRESH_COUNTER` (`AtomicU64`) made `__egg_v{n}` drift across compiles in a long-lived process (confirmed on the Rust target; dormant on WASM, whose name section only names functions). Replaced with `vt.len()` (== the `VarId` the imminent `alloc` assigns): unique, per-compile-deterministic, no global state. *(This also removes a raw-atomic the L1 grep would otherwise flag once egg-lab enters scope.)*
- **anonymous-record naming — the "minefield", solved**: `__anon_record_{record_fields.len()}` (walk-order counter) → `__anon_record_{FNV1a64(field-shape)}`, a pure function of content (FNV, not `DefaultHasher`, to avoid `RandomState`). Resolves the "no total content key" worry — the dedup key the code already computes *is* the total key.
- **`default_fields`** (Rust struct-literal emit): `HashMap.keys()` order → sorted. The last RandomState-class emit-order site (Rust target).

Verified: full `spec/` suite **240/240** with debug asserts active (the `Canonical`/postcondition checks never fired); Rust + WASM output **byte-identical across repeated compiles**.

## Deferred (honest limits)

- **L1 is alias-fragile and `#[allow]`-bypassable** — it buys coverage + a CI failure, not Perceus-grade impossibility. The real spine is L3 (now shipped).
- **The `Sym` interner hazard is real but currently *dormant*, not live.** `Sym: Hash` is the intern-order `Spur` id over a process-global `ThreadedRodeo`, so `HashMap<Sym,_>` iteration order is process-history dependent. But the audit found **every** genuinely `Sym`-keyed map in the compile path is used lookup-only, for diagnostics, or to allocate `DefId`s that never reach emitted bytes — none leak through iteration order today. It remains a latent footgun (the next `for (sym, _) in map` that feeds output would go live); L2 should forbid iterating `Sym`-keyed maps, and the defensive sort of `lower/mod.rs` `env.top_lets` by `Sym::as_str()` is cheap insurance. NOT yet done.
- **L3 is WASM-only** (matching `Verified`'s scope). The Rust target flattens modules and is covered by the egg + `default_fields` fixes, not the type-state gate. Extending `Canonical` to Rust emit is future work.
- **L3's sort key totality** — handled by a *stable* sort on `(is_test, name)`: a true name collision falls back to the (already deterministic) upstream order, so canonicalize stays idempotent and input-order-independent. The minefield was the anon-record counter, now content-keyed.
- **No layer certifies the *artifact*** — determinism is a property of provenance, gone by the time you hold the bytes. The construction layers constrain the *process*; emitter faithfulness + the unknown N+1-th source axis are closed only empirically by L4. The runtime gate's reliance is permanent, not transitional.

## Next highest-leverage step

L3 closes the structural gap, so the remaining work is hardening the outer layers: (1) `clippy.toml` `disallowed-methods` deny (Instant::now, SystemTime::now, thread_rng, fetch_add) + `#![deny(clippy::disallowed_methods)]` — upgrades L1 from grep to compiler-checked; (2) L2 `DetMap`/`DetSet` to make the dormant `Sym`-iteration hazard un-writable; (3) optional L5 Lean proof that `canonicalize` is idempotent & input-order-independent.
