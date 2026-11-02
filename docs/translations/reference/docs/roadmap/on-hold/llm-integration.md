<a id="llm-integration"></a>

# LLM Integration

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](llm-integration.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](llm-integration.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](llm-integration.zh-CN.md)

> Non-normative reader snapshot of the [English canonical source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/llm-integration.md), pinned at `35297a612fb29e3f78f6587aba253aa91d51f466`. The original remains authoritative. This snapshot is not automatically synchronized and is not generator/checker input. Dates, plans, completion status, measurements and comparative claims below are retained as historical source statements, not independently reproduced or validated for a current release. Code blocks and comments remain verbatim. Historical collection metadata is displayed as an inactive reader note.

> Historical metadata: description — Built-in LLM commands for library generation, auto-fix, and code explanation.

> Source caveat: this on-hold proposal is explicitly unimplemented. Its future-tense workflows, speed/cost and uniqueness claims are the proposal’s rationale, not independently established capabilities or comparisons. The Scope Boundary still uses “fix” as a proposal label; it does not redefine shipped local `almide fix` or authorize source transmission. Provider consent and a distinct interface remain prerequisites.

<a id="status-and-current-cli-boundary-2026-10-31"></a>

## Status and current CLI boundary (2026-10-31)

This is an **unimplemented AI-integration proposal**, not a description of the
current CLI. In particular, shipped `almide fix` applies locally computed,
machine-applicable diagnostic repairs and rechecks them; it does **not** send
source code to an LLM. See [`src/cli/fix.rs`](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/src/cli/fix.rs).
A future AI-assisted repair command would need a distinct interface and explicit
source-transmission/provider consent; it must not silently change that contract.
`forge`, `pilot`, `[ai]`, and `--no-ai` below are proposed interfaces.
The per-function AI `explain` proposal is distinct from the shipped
`almide explain <diagnostic-code>` command.

<a id="thesis"></a>

## Thesis

Embed LLMs into the compiler of "the language LLMs can write most accurately." LLMs write, LLMs fix, LLMs grow libraries — this loop runs with a single compiler.

<a id="subcommands"></a>

## Subcommands

<a id="almide-forge--library-generation"></a>

### `almide forge` — Library generation

Specify a theme and reference implementations, and it automatically designs, implements, tests, and publishes an Almide library.

```bash
almide forge csv --ref python:csv,rust:csv,go:encoding/csv
```

1. Analyze reference library APIs (documentation or source)
2. Design Almide-idiomatic API (UFCS, effect fn, Result/Option, naming conventions)
3. Implementation + test generation
4. Verify all tests pass with `almide test`
5. Create GitHub repository + push

**Why:** Bootstrapping the ecosystem. Having LLMs mass-produce and humans review is faster than writing each library by hand.

<a id="proposed-ai-assisted-repair--not-the-shipped-almide-fix"></a>

### Proposed AI-assisted repair — not the shipped `almide fix`

Pass compile errors to LLM for automatic fix.

```bash
almide <proposed-ai-repair-command> app.almd
```

1. Collect errors with `almide check`
2. Send source code + error diagnostics to LLM
3. Apply the proposed fix
4. Verify pass with `almide check` again
5. Show diff and wait for approval (`--yes` to skip)

**Why:** An extension of error recovery. The compiler doesn't just say "fix it like this" — it actually fixes it.

<a id="almide-explain--code-explanation"></a>

### `almide explain` — Code explanation

```bash
almide explain app.almd
almide explain app.almd --fn parse_config
```

Generate Markdown explanations of source code. Can specify individual functions.

**Why:** Automated documentation generation. LLMs explain code that LLMs wrote.

<a id="configuration"></a>

## Configuration

```toml
# almide.toml
[ai]
provider = "anthropic"    # or "openai"
model = "claude-sonnet-4-20250514"
# api_key is read from ANTHROPIC_API_KEY / OPENAI_API_KEY env var
```

- `--no-ai` flag disables all AI features (operates as offline compiler)
- API key is read from environment variables (not hardcoded in toml)
- AI features do not affect the compiler core code paths (separate module)

<a id="scope-boundary"></a>

## Scope Boundary

**Include (only things related to Almide code):**
- forge: Library generation
- fix: Automatic compile error fixing
- explain: Code explanation

**Exclude (not a general-purpose agent):**
- Chat UI
- Arbitrary task execution
- File operations outside Almide

<a id="implementation-plan"></a>

## Implementation Plan

<a id="phase-1-separate-ai-assisted-repair-command"></a>

### Phase 1: Separate AI-assisted repair command
- [ ] Add HTTP client (`ureq` or `reqwest`)
- [ ] `[ai]` config loading
- [ ] `almide check` → errors + source → LLM API → fix diff → apply
- [ ] `--yes` / `--dry-run` flags

<a id="phase-2-almide-forge"></a>

### Phase 2: `almide forge`
- [ ] `--ref` parser (`language:package` format)
- [ ] Prompt design for reference library API analysis
- [ ] Almide API design → implementation → test generation pipeline
- [ ] `gh repo create` + push integration

<a id="phase-3-almide-pilot--path-optimization-constrained-decoding"></a>

### Phase 3: `almide pilot` — Path optimization (Constrained Decoding)

The compiler guides LLM token generation in real time. Each token during generation is verified by the parser/type checker, and invalid tokens are immediately rejected with correction candidates returned.

```
LLM → token generation → almide parser (incremental) → valid?
                                                          ├─ Yes → accept, next
                                                          └─ No  → return valid continuation candidates → LLM re-selects
```

- [ ] Incremental parser API (returns "set of valid next tokens" from partial input)
- [ ] Streaming mode for type checker (run type inference on partial AST)
- [ ] `almide pilot serve` — Launch as LSP-like JSON-RPC server, callable from external LLMs
- [ ] Speculation buffer: backtrack invalid tokens and return valid continuations as hints
- [ ] Benchmark: compare compile success rate and generation speed with/without constrained decoding

**Why:** the proposed AI-assisted repair is "fix after writing." pilot is "make it correct at the moment of writing." MoonBit has a prior implementation, but Almide's simple grammar and type system make implementation cost low. The current compiler targets Rust and Wasm; this proposal must use their shared checked IR, not the retired TypeScript target.

<a id="phase-4-almide-explain"></a>

### Phase 4: `almide explain`
- [ ] Per-function explanation generation
- [ ] Markdown output

<a id="differentiator"></a>

## Differentiator

Rust, Go, TypeScript — none of their compilers have LLMs built in. But that's because they are "languages humans write." Almide is a language LLMs write. Having an LLM on the compiler side is a natural consequence and a strength unique to Almide.
