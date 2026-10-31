# The Equivalence Claim — Byte-Identical Across Targets

**Cross-target equivalence is checked contract by contract:** deterministic
observations (stdout, stderr, exit code) are compared for supported programs
under matching inputs and host assumptions. Host observations, randomness,
resource limits, and oracle-relative time require their stated invariants or
comparison conditions. Passing a finite corpus is not a proof for every
program that compiles for both targets.

"Byte-identical" means the *execution output*, not the compiled artifacts — a native binary and a `.wasm` module are different bytes by construction; the comparison concerns the observations covered by the contract.

## Scope

The guarantee is **continuous, with an explicit scope** — held by gates that re-run on every change, not a one-time snapshot:

- **Deterministic observations**: byte comparisons in the declared cross-target
  fixtures and differential tests, not unrestricted equality under different hosts.
- **Nondeterministic inputs**: contracts can assert an invariant rather than
  identical samples (e.g. random values stay in range). The old timeout removal
  (C-006) is historical; current `fan.timeout` is oracle-relative (C-208).
- **Unavailable operations**: the selected route may refuse a program or
  operation; consult `proofs/target-availability.toml`. A refusal is not a
  successful parity observation for that program.
- **Declared host/representation differences**: platform reporting and compressed
  output have scoped invariants, not universal byte equality. For example the
  zlib contract compares decoding/round trips while documenting different
  compression strategies. Consult the relevant contract and executable fixture
  rather than assuming the old two-function exemption list covers every case.

## The contract ledger

This claim is not prose. Every observable promise is a named contract in the [behavior-contract ledger](../contracts), each traceable to executable evidence of class ≥ `fixture`. The exceptions clause is auto-generated from the ledger into the [measurements claims block](../benchmarks/README.md) by `scripts/gen-claims.sh`; the contract counts beside it are stamped from the ledger with their measurement date (`proofs/ledger-counts.toml`, `scripts/gen-ledger-counts.sh`, re-measured nightly by `scripts/check-ledger-counts.sh`); and `scripts/check-contracts.sh` fails CI if the block drifts from either — the generated block is kept in sync with its declared inputs; this does not by itself establish that every prose statement or evidence link proves the claimed behavior.

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
| [Lean 4 belt](https://github.com/almide/almide/tree/develop/crates/almide-perceus-belt) | RC-insertion correctness machine-checked by the Lean kernel |
| [Org byte-verify sweep](https://github.com/almide/almide/blob/develop/scripts/org-byte-verify.sh) | every runnable repo in the almide org executed on both targets, stdout + exit byte-compared |

## Verify it yourself

```bash
almide test                        # every fixture, both targets where declared
bash scripts/check-contracts.sh    # ledger integrity + claims-block freshness
bash scripts/org-byte-verify.sh    # org-wide two-target byte comparison
```

Certificates establish modeled witness properties within their coverage and
trust assumptions; they do not prove native/Wasm semantic equivalence. See
[proven vs trusted](../contracts/proven-vs-trusted.md) and
[TRUST-SPINE.md](../TRUST-SPINE.md).
