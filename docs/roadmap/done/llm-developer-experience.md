<!-- description: UFCS and almide init improvements for LLM-assisted development -->
# LLM Developer Experience [DONE / MERGED]

> UFCS: done. Remaining items (`almide init` config) merged into [LLM Integration](../active/llm-integration.md).

### `almide init` CLAUDE.md generation

Currently `almide init` always generates a `CLAUDE.md` file for AI-assisted development.
This should become opt-in or configurable:

- [ ] Add `--claude` / `--no-claude` flag to `almide init`
- [ ] Or prompt interactively: "Generate CLAUDE.md for AI-assisted development? [Y/n]"
- [ ] Consider a config in `almide.toml`: `[tools] claude_md = true`

### UFCS (Uniform Function Call Syntax) — DONE

Method-style calls like `x.len()`, `s.contains("a")` now resolve to the correct module
based on the receiver's type at compile time. No runtime dispatch needed.

- [x] Type-aware UFCS resolution in type checker (`resolve_ufcs_by_type`)
- [x] Works for String, List, Map, Int, Float receivers
- [x] Chained UFCS: `"hello world".split(" ").reverse()` resolves correctly
- [x] Fallback: runtime dispatch (TS) / first candidate (Rust) for Unknown types
- [x] Type checker returns correct return types for UFCS calls (enables chaining)
