<!-- description: Built-in LLM commands for library generation, auto-fix, and code explanation -->
# LLM Integration

## Thesis

Embed LLMs into the compiler of "the language LLMs can write most accurately." LLMs write, LLMs fix, LLMs grow libraries — this loop runs with a single compiler.

## Subcommands

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

### `almide fix` — Self-repair

Pass compile errors to LLM for automatic fix.

```bash
almide fix app.almd
```

1. Collect errors with `almide check`
2. Send source code + error diagnostics to LLM
3. Apply the proposed fix
4. Verify pass with `almide check` again
5. Show diff and wait for approval (`--yes` to skip)

**Why:** An extension of error recovery. The compiler doesn't just say "fix it like this" — it actually fixes it.

### `almide explain` — Code explanation

```bash
almide explain app.almd
almide explain app.almd --fn parse_config
```

Generate Markdown explanations of source code. Can specify individual functions.

**Why:** Automated documentation generation. LLMs explain code that LLMs wrote.

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

## Scope Boundary

**Include (only things related to Almide code):**
- forge: Library generation
- fix: Automatic compile error fixing
- explain: Code explanation

**Exclude (not a general-purpose agent):**
- Chat UI
- Arbitrary task execution
- File operations outside Almide

## Implementation Plan

### Phase 1: `almide fix`
- [ ] Add HTTP client (`ureq` or `reqwest`)
- [ ] `[ai]` config loading
- [ ] `almide check` → errors + source → LLM API → fix diff → apply
- [ ] `--yes` / `--dry-run` flags

### Phase 2: `almide forge`
- [ ] `--ref` parser (`language:package` format)
- [ ] Prompt design for reference library API analysis
- [ ] Almide API design → implementation → test generation pipeline
- [ ] `gh repo create` + push integration

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

**Why:** `almide fix` is "fix after writing." pilot is "make it correct at the moment of writing." MoonBit has a prior implementation, but Almide's simple grammar and type system make implementation cost low. Being able to use multi-target (Rust + TS) type information is a strength unique to Almide.

### Phase 4: `almide explain`
- [ ] Per-function explanation generation
- [ ] Markdown output

## Differentiator

Rust, Go, TypeScript — none of their compilers have LLMs built in. But that's because they are "languages humans write." Almide is a language LLMs write. Having an LLM on the compiler side is a natural consequence and a strength unique to Almide.
