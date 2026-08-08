# Almide documentation

Start here. Four doorways, by what you are trying to do.

| I want to… | Read |
|---|---|
| **Write Almide** | [CHEATSHEET.md](./CHEATSHEET.md) — syntax, stdlib, idioms. The one an LLM should load first (see also [`/llms.txt`](../llms.txt)) |
| **Know what the language *is*** | [SPEC.md](./SPEC.md) (normative) · [GRAMMAR.md](./GRAMMAR.md) (EBNF) · [specs/](./specs/) (per-area, with test paths) |
| **Change the compiler** | [ARCHITECTURE.md](./ARCHITECTURE.md) — pipeline and module map |
| **Judge whether to trust it** | [TRUST-SPINE.md](./TRUST-SPINE.md) — what is proven, what is measured, what is neither |

## Directories

| Path | What is in it | Machine-read? |
|---|---|---|
| [`specs/`](./specs/) | Per-area language specs. Rules for writing them: [specs/CLAUDE.md](./specs/CLAUDE.md). `specs/als/` is the normative ALS sections the contract ledger keys against | ✅ CI path filter |
| [`stdlib/`](./stdlib/) | One page per stdlib module; the signature index in each is generated (`make stdlib-docs`). `semantics-manifest.toml` is a gate input | ✅ `check-semantics-manifest.sh` |
| [`diagnostics/`](./diagnostics/) | One page per `EXXX` code. **Compiled into the binary** — `build.rs` `include_str!`s this directory, and `almide explain EXXX` prints it | ✅ `build.rs` |
| [`contracts/`](./contracts/) | The behavior-contract ledger: every observable native ⇄ wasm promise, traceable to executable evidence. Index and conformance report are generated | ✅ `check-contracts.sh` |
| [`adr/`](./adr/) | Architecture Decision Records — the *why* behind accepted and rejected designs, numbered and immutable |  |
| [`roadmap/`](./roadmap/) | `active/` = in flight (≈20 are cited by name from Rust source comments), `on-hold/`, `done/`. README is generated from the tree | ✅ `almide-gates` |
| [`design/`](./design/) | Design rationale: [DESIGN.md](./design/DESIGN.md) (ambiguity removal), [REJECTED_PATTERNS.md](./design/REJECTED_PATTERNS.md), [HIDDEN_OPERATIONS.md](./design/HIDDEN_OPERATIONS.md), [EQUIVALENCE.md](./design/EQUIVALENCE.md) |  |
| [`wasm/`](./wasm/) | [WASM-OUTPUT.md](./wasm/WASM-OUTPUT.md) (what the wasm backend emits and commits to), capability system |  |
| [`project/`](./project/) | [BENCHMARKS.md](./project/BENCHMARKS.md), [BREAKING_CHANGE_POLICY.md](./project/BREAKING_CHANGE_POLICY.md), [CLAUDE_TEMPLATE.md](./project/CLAUDE_TEMPLATE.md) (`almide init` embeds this one) | ✅ `include_str!` |
| [`benchmarks/`](./benchmarks/) | `build-speed.txt` — the committed baseline the README block is rendered from | ✅ `almide-gates bench` |
| `assets/`, `figures/` | README images and generated benchmark charts |  |

## Before you move or delete anything here

`docs/` is not only prose. The ✅ column above marks directories a build step or
CI gate reads by path: `docs/diagnostics/` is linked into the compiler binary,
and `contracts/`, `stdlib/semantics-manifest.toml`, `specs/als/`, `TRUST-SPINE.md`
and `benchmarks/build-speed.txt` are gate inputs. About twenty `roadmap/active/`
pages are named from Rust source comments. Grep before you move.

MSR (modification survival rate) lives in
[almide-dojo](https://github.com/almide/almide-dojo), not here — this repo is
compiler correctness.
