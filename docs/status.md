# Project Status

Where Almide stands today. The stamped totals (stdlib size, test files, contracts, mutation score) are rendered on the [measurements page](./benchmarks/README.md#derived-counts), where CI checks them against their ledgers.

| Category | Status |
|----------|--------|
| Maturity | Pre-1.0, under active development on `develop`; the LLM-facing surface is frozen by [STABILITY.md](./STABILITY.md) (declared 2026-08-20) |
| Support | Latest release line only, pre-1.0 — policy and versioning guarantees: [SUPPORT.md](../SUPPORT.md) · vulnerabilities: [SECURITY.md](../SECURITY.md) |
| Compiler | Pure Rust, single binary |
| Targets | Rust (native), WASM (direct emit — the structural leg, see [How It Works](./how-it-works.md)) |
| Verified codegen | Structural wasm leg: byte-exact corpus and mutation gates, trusted with its per-build certificate pending (#2755–#2760); the PCC-certified incumbent leg (per-build re-verification since 0.29.0) was retired by #2761 |
| Codegen | Rust: Nanopass + TOML templates; wasm: structural engine → direct emit (the v0 emitter and the incumbent MIR→WAT renderer are retired — a wall is an error, never a fallback) |
| Stdlib inspection | Every public stdlib function carries an `inspected` stamp in [`proofs/stdlib-inspection.toml`](../proofs/stdlib-inspection.toml): the same bytes on native and wasm, a per-call cost within its declared order, codopsy A, fresh fuzzed combinations, and the call through a module diamond. 0 uninspected as of 2026-10-19 (almide 0.68.0); `tests/stdlib_inspection_test.rs` voids a stamp when the signature or the body changes, and the uninspected count only falls |
| Artifacts | `.almdi` module interface files via `almide compile` |
| Playground | [Live](https://almide.github.io/playground/) — the compiler runs as WASM in the browser |

Release history: [CHANGELOG.md](../CHANGELOG.md) · plans: [roadmap](./roadmap/README.md).
