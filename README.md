<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="./docs/assets/brand/almide-cover-dark.png">
    <img src="./docs/assets/brand/almide-cover.png" alt="Almide" width="720">
  </picture>
</p>

<p align="center"><strong>When the model is wrong, the compiler names the fix.</strong></p>

<p align="center">
  <a href="https://github.com/almide/almide/actions/workflows/ci.yml"><img src="https://github.com/almide/almide/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
  <a href="./LICENSE"><img src="https://img.shields.io/badge/license-MIT%20%2F%20Apache--2.0-blue.svg" alt="License: MIT / Apache-2.0"></a>
  <a href="https://deepwiki.com/almide/almide"><img src="https://img.shields.io/badge/Ask-DeepWiki-blue.svg" alt="Ask DeepWiki"></a>
</p>

<p align="center">
  <a href="https://almide.github.io/playground/">Playground</a> ·
  <a href="./docs/CHEATSHEET.md">Cheatsheet</a> ·
  <a href="./docs/SPEC.md">Specification</a> ·
  <a href="#quick-start">Quick start</a> ·
  <a href="./docs/benchmarks/README.md">Evidence</a> ·
  <a href="./docs/how-it-works.md">How it works</a> ·
  <a href="./docs/status.md">Status</a>
</p>

<p align="center">
  <a href="README.md"><img alt="English" src="https://img.shields.io/badge/English-24292f?style=flat-square"></a>
  <a href="README.ja.md"><img alt="日本語" src="https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square"></a>
  <a href="README.zh-CN.md"><img alt="简体中文" src="https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square"></a>
</p>

---

Almide is a statically-typed language for code that AI writes and edits. The claim is not that the model gets it right. It is that **when the model is wrong, the compiler says so at author time and names the fix**. Programs compile to native binaries (via Rust) and to WebAssembly, with byte-identical output on every [contracted behaviour](./docs/contracts/). `almide verify` checks the ownership, bounds, name and capability witnesses of the wasm module `almide build` ships, pinned to that file by its SHA-256 ([what is proven, what is trusted](./docs/contracts/proven-vs-trusted.md)).

## An edit that survives

A model adds a case to a type and, as models do, touches nothing else:

```almd
type Shape =
  | Circle(Float)
  | Square(Float)
  | Triangle(Float, Float)   // the edit

fn area(s: Shape) -> Float =
  match s {
    Circle(r) => 3.14159 * r * r
    Square(w) => w * w
  }
```

```text
error[E010]: non-exhaustive match: missing Triangle(_, _)
  --> shape.almd:7:9
  in match
  here: match s {
  hint: add arms for Triangle(_, _):
  Triangle(arg1, arg2) => _
Or use `_ => todo()` to compile incrementally.
```

The diagnostic is the fix: the model's next turn is `Triangle(b, h) => 0.5 * b * h`, and the program then prints the same bytes natively and on wasm. The same question asked against Python, on eight injected modification mistakes: [demo/make-verify](./demo/make-verify/).

## Quick Start

**[Try it in your browser →](https://almide.github.io/playground/)** — no installation.

```bash
curl --proto '=https' --tlsv1.2 -fsSL https://raw.githubusercontent.com/almide/almide/main/tools/install.sh | sh   # macOS / Linux
irm https://raw.githubusercontent.com/almide/almide/main/tools/install.ps1 | iex        # Windows (PowerShell)
```

The installer checks the archive against the release's `almide-checksums.sha256`; every release asset is Sigstore-attested ([SECURITY.md](./SECURITY.md#release-integrity) shows how to verify one). Each archive also carries `almide-verify`, the certificate checker `almide verify` hands its witnesses to. From source, with [Rust](https://rustup.rs/) 1.96+: `make install`.

```almd
fn main() -> Unit = {
  println("Hello, world!")
}
```

```bash
almide run hello.almd                 # native
almide run hello.almd --target wasm   # same bytes, on wasmtime
```

## Features

- **Multi-target** — one source, a native binary (via Rust) or WebAssembly (direct emit, no LLVM), byte-identical output on every behaviour in the contract ledger ([contracts](./docs/contracts/))
- **Diagnostics that name the fix** — every error carries its location and its context, and a concrete repair where one is known ([codes](./docs/diagnostics/))
- **Explicit effects** — `effect fn` with `expr!` propagation; a bare fallible call is an error, never silent ([cheatsheet](./docs/CHEATSHEET.md))
- **Types** — generics, records, variants with exhaustive `match`, bidirectional inference, `Codec` auto-derive ([spec](./docs/SPEC.md))
- **Memory without annotations** — Perceus-style ownership inference, no GC, no lifetimes ([memory safety](./docs/design/MEMORY-SAFETY.md))
- **Fan** — structured concurrency on threads natively, in the same output order on wasm ([ADR-0024](./docs/adr/0024-fan-effect-callbacks-run-concurrently-as-if-sequential.md))
- **Standard library** — self-hosted `.almd` modules, each public function inspected on both targets ([reference](./docs/stdlib/))
- **Tooling built in** — `almide test`, `almide fmt`, `almide check`, modules with diamond resolution ([how it works](./docs/how-it-works.md))

## Learn more

- [Cheatsheet](./docs/CHEATSHEET.md) — syntax, stdlib and idioms; the page a model should load first (see also [`llms.txt`](./llms.txt))
- [Specification](./docs/SPEC.md) · [grammar](./docs/GRAMMAR.md) · [standard library](./docs/stdlib/) · [design philosophy](./docs/design/DESIGN.md) · [stability policy](./docs/STABILITY.md)
- [What is measured](./docs/benchmarks/README.md) — LLM writability, byte identity, memory safety, size and speed, each dated and gated · [benchmark methodology](./docs/project/BENCHMARKS.md)
- [Behavior contracts](./docs/contracts/) — every native ⇄ wasm promise and its evidence · [proven vs trusted](./docs/contracts/proven-vs-trusted.md)
- [How it works](./docs/how-it-works.md) — the pipeline and the Trust Spine · [architecture](./docs/ARCHITECTURE.md)
- [Project status](./docs/status.md) — pre-1.0; the LLM-facing surface changes only through a dialect epoch with a migration diagnostic ([stability policy](./docs/STABILITY.md)) · [roadmap](./docs/roadmap/README.md)
- [Written in Almide](./docs/written-in-almide.md) — agents, parsers, graphics and libraries outside this repository · editors: [vscode-almide](https://github.com/almide/vscode-almide), [tree-sitter-almide](https://github.com/almide/tree-sitter-almide)

## Contributing

Issues and pull requests are welcome. After cloning, run `brew install lefthook && lefthook install`; commits are in English (the commit-msg hook enforces it). Conventions: [CLAUDE.md](./CLAUDE.md).

## License

Licensed under either of [MIT](./LICENSE-MIT) or [Apache 2.0](./LICENSE-APACHE) at your option.
