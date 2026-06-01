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
| **L3 `Canonical<IrProgram>`** | a terminal `CanonicalizePass` puts functions/type-decls/variant-cases/record-fields into a content-derived **total** order; `emit` accepts only `Canonical` (sibling of `Verified`). Re-normalizes even if an upstream pass built a Vec in hash order | type-state (true Perceus spine) | roadmap |
| **L4 Runtime tripwire** | `check-host-determinism.sh` (wasm32-wasip1) + `check-browser-determinism.sh` (wasm32-unknown-unknown via node) byte-compare the compiler's output to native | test (open-world backstop, permanent) | **DONE** |
| L5 Lean proof | `canonicalize` is idempotent & input-order-independent | proof | optional |

## Done so far (v0.23.13 + this work)

- **L1**: `almide_base::profile::ProfileTimer` (the one sanctioned, wasm-safe clock); the three codegen clock sites route through it; `scripts/check-forbidden-impurities.sh` fails CI on raw `std::time|Instant|SystemTime|thread::spawn|thread_rng|fastrand|AtomicU64|fetch_add` in `crates/{almide-codegen,almide-frontend,almide-optimize,almide-ir}/src` (excluding `/generated/` `/runtime/` — those are runtime code emitted into the user's native program). Wired into the `checks` CI job.
- **The confirmed-live data-order leak**: `record_fields`/`variant_info` → `BTreeMap`; `closures.rs`/variant-eq/name-section sorted; **`almide-optimize/src/mono/mod.rs` `all_instances`/`new` → `BTreeMap`** (specialized-function append order, → WASM function indices, was HashMap-ordered → host-width + intern-order dependent).
- **L4**: both gates in CI; `spec/wasm_cross/` fixtures incl. `playground_default.almd` and `generics_mono.almd` (exercises monomorphization).

## Deferred (honest limits)

- **L1 is alias-fragile and `#[allow]`-bypassable** — it buys coverage + a CI failure, not Perceus-grade impossibility. The real spine is L3.
- **The `Sym` interner is a latent worse-than-RandomState hazard**: `Sym: Hash` is the intern-order `Spur` id over a process-global `ThreadedRodeo`, so `HashMap<Sym,_>` iteration order depends on process history (warm/cold `bundled_sigs`, N-th compile in the long-lived playground / `almide test`). L1+L2 must forbid iterating `Sym`-keyed hash maps outright; L3 sorts by `Sym::as_str()` content. NOT yet enforced.
- **egg's `EGG_FRESH_COUNTER` (`almide-egg-lab/src/bridge.rs`) is never reset per-compile** → `__egg_v{n}` names differ across compiles in one process. Fix: thread it through pass state / reset at pass entry. NOT yet done (egg-lab is outside the current grep scope).
- **L3's total sort key is a correctness minefield** — a non-total key makes canonicalize unstable (re-introducing non-determinism); it must run after order-dependent passes and the key must be provably total. Real, unbudgeted design risk.
- **No layer certifies the *artifact*** — determinism is a property of provenance, gone by the time you hold the bytes. The construction layers constrain the *process*; emitter faithfulness + the unknown N+1-th source axis are closed only empirically by L4. The runtime gate's reliance is permanent, not transitional.

## Next highest-leverage step

The `clippy.toml` `disallowed-methods` deny (Instant::now, SystemTime::now, thread_rng, fetch_add) + `#![deny(clippy::disallowed_methods)]` in the four crates — upgrades L1 from grep to compiler-checked for the method-call axis (cheap, no new toolchain). Then L2 `DetMap` for the `Sym`-iteration ban, then L3.
