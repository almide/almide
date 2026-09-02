# The Equivalence Claim — Byte-Identical Across Targets

**Every program that compiles for both targets produces byte-identical observable output — stdout, stderr, exit code — whether it runs as a native binary or as WebAssembly.** Native is the oracle; `native == wasm` is a hard invariant, not a "target difference" to be documented around.

"Byte-identical" means the *execution output*, not the compiled artifacts — a native binary and a `.wasm` module are different bytes by construction; what must not differ is anything the program lets you observe.

## Scope

The guarantee is **continuous, with an explicit scope** — held by gates that re-run on every change, not a one-time snapshot:

- **In scope**: everything the program lets you observe — stdout, stderr, exit code — on every program that compiles for both targets.
- **Inherently nondeterministic sources** are ledger-managed, not waved away: their contracts certify the *deterministic invariant* (e.g. every `random.int(lo, hi)` draw stays in range — C-112) instead of exact bytes, and the sole wall-clock stdlib surface was removed outright (C-006) rather than documented around.
- **APIs not yet implemented on wasm** are compile- or run-time *refusals*: the program never runs far enough to emit wrong bytes — an honest wall, not a silent divergence.
- **Platform-reporting fns are the one exemption, and it is closed**: `env.os()` and `env.temp_dir()` report the host they run on, so native answers the real OS name and temp directory while the wasm leg answers the WASI sandbox. Making them agree would be the defect. C-189 bounds the carve-out to exactly those two and certifies the invariant that *is* identical (the platform set is closed; the temp path is non-empty and absolute), so a third exempt fn cannot appear without amending the contract and its fixture.

## The contract ledger

This claim is not prose. Every observable promise is a named contract in the [behavior-contract ledger](../contracts), each traceable to executable evidence of class ≥ `fixture`. The exceptions clause is auto-generated from the ledger into the [README claims block](../../README.md) by `scripts/gen-claims.sh`; the contract counts beside it are stamped from the ledger with their measurement date (`proofs/ledger-counts.toml`, `scripts/gen-ledger-counts.sh`, re-measured nightly by `scripts/check-ledger-counts.sh`); and `scripts/check-contracts.sh` fails CI if the block drifts from either — the public claim literally cannot desynchronize from what the gates verify.

Ledger mechanics:

- A new observable behavior requires a new `C-NNN` contract **and** at least one fixture declaring it via a `// @contract: C-NNN` header — the link is bidirectional and CI-enforced.
- Removing a divergence means flipping the contract's `status` to `active` in the same PR. The `flagged-for-revision` count may only go **down** (ratcheted; currently ceiling 0).

## Evidence layers

| Evidence layer | What it locks |
|---|---|
| [Contract ledger](../contracts) | every promise is a named `C-NNN`; an `active` contract must carry evidence of class ≥ `fixture` |
| [Cross-target fixture gate](../../tests/wasm_runtime_cross_target.rs) | every `spec/wasm_cross/*.almd` fixture runs on both targets; outputs byte-compared (`wasm_cross_target_spec`) |
| [Differential fuzz](../../tests/regex_fuzz_test.rs) | randomized programs and inputs, native vs wasm outputs compared |
| Emit-time Σ-probes | wasm Unicode/case tables exhaustively probed against Rust `std` over the full scalar domain at emit time |
| [Lean 4 belt](../../crates/almide-perceus-belt) | RC-insertion correctness machine-checked by the Lean kernel |
| [Org byte-verify sweep](../../scripts/org-byte-verify.sh) | every runnable repo in the almide org executed on both targets, stdout + exit byte-compared |

## Verify it yourself

```bash
almide test                        # every fixture, both targets where declared
bash scripts/check-contracts.sh    # ledger integrity + claims-block freshness
bash scripts/org-byte-verify.sh    # org-wide two-target byte comparison
```

The v1 line strengthens this from "gates verify it" to "a proven checker re-verifies it on every build" — see [TRUST-SPINE.md](../TRUST-SPINE.md).
