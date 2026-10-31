# Project Status

Where Almide stands today. The stamped totals (stdlib size, test files, contracts, mutation score) are rendered on the [measurements page](./benchmarks/README.md#derived-counts), where CI checks them against their ledgers.

| Category | Status |
|----------|--------|
| Maturity | Pre-1.0, under active development on `develop`; the LLM-facing surface changes only through a dialect epoch with a migration diagnostic, per [STABILITY.md](./STABILITY.md) (declared 2026-08-20) |
| Support | Latest release line only, pre-1.0 — policy and versioning guarantees: [SUPPORT.md](../SUPPORT.md) · vulnerabilities: [SECURITY.md](../SECURITY.md) |
| Compiler | Pure Rust compiler; distributions also ship the independent `almide-verify` certificate checker |
| Targets | Rust (native), WASM (direct emit — the structural leg, see [How It Works](./how-it-works.md)) |
| Verified codegen | The structural wasm leg emits per-build ownership, bounds, name and capability witnesses; `almide verify` checks the bundle bound by SHA-256 to the shipped wasm file (#4011). This is not a proof of semantic correctness or whole-artifact memory safety: the coverage limits and remaining trust assumptions are explicit in [proven vs trusted](./contracts/proven-vs-trusted.md) and the [memory-safety gap table](./design/MEMORY-SAFETY.md#the-gap-table-2026-10-24-4011). The incumbent MIR→WAT renderer was retired by #2761 |
| Codegen | Rust: Nanopass + TOML templates; wasm: structural engine → direct emit (the v0 emitter and the incumbent MIR→WAT renderer are retired — a wall is an error, never a fallback) |
| Stdlib inspection | Every public stdlib function carries an `inspected` stamp in [`proofs/stdlib-inspection.toml`](../proofs/stdlib-inspection.toml): the same bytes on native and wasm, a per-call cost within its declared order, codopsy A, fresh fuzzed combinations, and the call through a module diamond. 0 uninspected as of 2026-10-19 (almide 0.68.0); `tests/stdlib_inspection_test.rs` voids a stamp when the signature or the body changes, and the uninspected count only falls |
| Artifacts | `.almdi` module interface files via `almide compile` |
| Playground | [Live](https://almide.github.io/playground/) — the compiler runs as WASM in the browser |

Release history: [CHANGELOG.md](../CHANGELOG.md) · plans: [roadmap](./roadmap/README.md).
