<!-- description: Campaign to rewrite OSS tools in Almide to prove WASM size and LLM accuracy -->
# The Rumbling — Almide OSS Rewrite Campaign

**Status**: On Hold (Block 0 after language features mature)
**Priority**: Strategic — Primary driver of language adoption
**Prerequisite**: WASM direct emit complete, stdlib stable, self-hosting Phase 0-1

## Why

Language adoption is decided by "what can be built with it." Go won with Docker and Kubernetes. Rust claimed the "fast CLI tools" position with ripgrep and fd.

The campaign would test three hypotheses, not assume an existing advantage:
1. **Wasm binary size** — the original 4.5KB Hello World observation lacks a
   recorded version/build recipe here; measure current artifacts before reusing it.
2. **Native and Wasm output** — TS/JS code-generation targets are retired.
3. **LLM modification survival rate** — evaluate scoped tasks/models through
   Almide Dojo; do not infer a universal language ranking.

The Rumbling is a plan to rewrite license-compatible OSS in Almide to demonstrate these weapons.

## Execution Principles

- **Don't proceed past Block 0 unless Block 0 passes** — Rewriting others' tools in a language that can't write its own is a contradiction
- **Items within each block are independent** — Can start from any item
- **Post after each completion** — Only ship things that stand as articles on their own
- **No degraded copies** — Even with reduced features, ship only when clearly superior to the original in some dimension
- **Skip anything huge or where Almide's strengths don't apply** — Databases, crypto libraries, OS-level tools are out of scope

---

## Block 0: Dogfood (Our Own Tools)

Write Almide's own toolchain in Almide. The entry point for self-hosting.

| Tool | What it proves | Size |
|---|---|---|
| `almide fmt` | String processing works at production level | Small |
| `almide test` runner | CLI tools can be written | Small |
| Test framework | assert/matcher DSL can be written naturally | Small |

**Gate condition**: Block 1 does not start until all 3 items in Block 0 are working.

---

## Block 1: Wasm Showcase (measure size for the actual workload)

Demonstrate the extraordinarily small WASM binary sizes. All with browser-running demos.

| Tool | Existing | Almide's advantage |
|---|---|---|
| markdown → HTML | marked.js (original comparison, unversioned) | Measure runtime + host + artifact size for equivalent behavior |
| JSON formatter / validator | jq (original comparison, unversioned) | Compare equivalent workload/builds; no size advantage established here |
| TOML parser | toml-rs (crate) | Browser deployment comparison not verified by this record |
| base64 encode / decode | btoa/atob | Self-contained WASM |

**Success criteria**: Being able to say "the WASM running on this page is X KB."

---

## Block 2: Native/Wasm Showcase (distribution adapters are separate work)

Demonstrate native/Wasm reuse. npm or crates.io packaging would require explicit
wrapper, API and publication work; npm distribution must not be confused with
a retired TS/JS compiler target. The table below lists candidate distributions,
not available outputs or authorization to publish packages.

| Library | Purpose | Distribution |
|---|---|---|
| Slug generation | URL slugs | npm + crate + WASM |
| Semver parser | Version comparison | npm + crate + WASM |
| Color conversion | hex / rgb / hsl | npm + crate + WASM |
| uuid v4 | ID generation | npm + crate + WASM |

**Success criteria**: Create a state where "people are using an Almide-written library without knowing it."

---

## Block 3: LLM Modification Showcase (Proving Survival Rate)

Demonstrate that LLMs can modify code accurately. Each tool comes with a demo showing "asked LLM to do X and it worked without breaking."

| CLI | Existing | Almide's strength |
|---|---|---|
| HTTP client | httpie, curlie | LLM grasps the full codebase and adds headers without breaking |
| File watcher | watchexec | Simple enough for LLM to grasp the full codebase |
| Env management | direnv lite | Config file reading/writing |
| Task runner | just lite | TOML parsing + process execution |

**Success criteria**: Being able to produce measured modification survival rate data.

---

## Block 4: Platform (Ecosystem)

After credibility is built from Block 0-3.

| Project | Existing | Significance |
|---|---|---|
| Package registry | crates.io / npm | Foundation of the Almide ecosystem |
| Playground | Rust Playground | WASM compiler runs in browser (consequence of self-hosting) |
| LSP server | — | Serious developer experience |

---

## Not Doing

| Target | Reason |
|---|---|
| DB engine | Almide's strengths don't apply. Requires low-level I/O |
| Crypto library | Safety proofs exceed language maturity |
| OS-level tools | Requires direct syscalls. Doesn't match Almide's abstraction level |
| Massive frameworks | Not worth the effort. Ship many small, fast things instead |
