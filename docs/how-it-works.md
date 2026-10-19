# How It Works

The compiler pipeline in one page. The module map and every pass in detail: [ARCHITECTURE.md](./ARCHITECTURE.md).

One frontend, one IR, one renderer per target:

```mermaid
flowchart LR
    SRC([".almd"]) --> FE["Lexer → Parser → Type Checker → Lowering"] --> IR(["IR"])
    IR --> NANO["Nanopass Pipeline<br/>semantic rewrites"] --> TMPL["Template Renderer<br/>TOML-driven"] --> RS([".rs → native binary"])
    IR --> STRUCT["structural leg<br/>commissioned engine, direct emit"] --> WASM([".wasm"])
```

**Native.** The Nanopass pipeline applies target-specific transformations — `ResultPropagation` (Rust `?`), `CloneInsertion` (Rust borrow analysis), `LICM` (loop-invariant code motion). The Template Renderer is purely syntactic: every semantic decision is already encoded in the IR.

**WebAssembly.** The **structural leg** — the engine commissioned in [#1599](https://github.com/almide/almide/pull/1599), `almide::wasm_leg` front + `crates/almide-wasm` emitter, entered through `render_wasm_module_routed` in `src/cli/build.rs` — is the only wasm renderer. It was accepted at 610/610 byte-identical to native on the `wasm_cross` corpus, and its build artifacts ship in the WASI form ([#1588](https://github.com/almide/almide/issues/1588)) so they run on stock runtimes. A program it does not lower is an honest error (`error[E082]`, naming the wall and the function), never a fallback: the incumbent v1 leg that once took those shapes lost its last route in #2752 and was deleted in #2761. `ALMIDE_VERIFIED_DEBUG=1` narrates the route.

```bash
almide run app.almd                  # Compile + execute (native)
almide build app.almd --target wasm  # Build WebAssembly (WASI)
almide test                          # Find and run all test blocks (recursive)
almide check app.almd                # Type check only
almide check app.almd --target wasm  # + the wasm build route: E081/E082 at check time (#1922)
almide fmt app.almd                  # Format source code
```

Run `almide --help` for the full command list (compile, add, deps, clean, …). Pipeline and module map: [docs/ARCHITECTURE.md](./ARCHITECTURE.md); the wasm leg in detail: [docs/wasm/](./wasm/README.md).

## What's next — v1, the Trust Spine

The Perceus checker proof ([memory safety](./benchmarks/README.md#memory-safety--proven-where-it-is-proven-trusted-where-it-is-trusted)) proves one compiler pass, once. v1 generalizes that principle to the **whole pipeline** — instead of proving the whole compiler, it proves a tiny *checker* and has the compiler emit a certificate on every build that the checker re-verifies. If the checker accepts, the artifact has the property — a theorem that never mentions the compiler's internals. That collapses the trusted base from the whole compiler (about 300,000 lines of Rust under `src/` and `crates/`, test directories excluded, counted 2026-10-03) to the extracted checker (~1,400 lines of OCaml, machine-derived from the proofs), and asks a harder question than testing ever can: **not "do the tests pass?" but "can a machine prove the output is correct?"** The architecture, the receipts (C-SAFE / C-REPRO / C-FAITHFUL / C-PROVEN), and why builds are slower on purpose: **[docs/TRUST-SPINE.md](./TRUST-SPINE.md)**.
