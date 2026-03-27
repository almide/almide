<!-- description: Tooling roadmap (LSP, REPL, doc gen, bench) split to active -->
<!-- done: 2026-03-18 -->
# Tooling [ON HOLD — items split to active]

Most items from this roadmap have been promoted to dedicated active roadmaps:

## Promoted to Active

- **LSP** → [lsp.md](../active/lsp.md) — P0 priority, editor integration
- **REPL** → [repl.md](../active/repl.md) — interactive evaluation

## Remaining (ON HOLD)

### Documentation Generation (`almide doc`)
- Add `///` doc comments to lexer/AST
- Generate HTML/Markdown documentation for modules, functions, and types

### Benchmarking (`almide bench`)
```almide
bench "list sort 1000 elements" {
  let xs = list.reverse(range(0, 1000))
  list.sort(xs)
}
```

### Comment Preservation in `almide fmt`
- Keep comments as tokens in the lexer
- Attach comment information to AST nodes
- Restore comments when formatting

### Formatter `--check` Mode
- `almide fmt --check app.almd` — exit 1 if not formatted (for CI)
- No file modification, just validation

## Priority

LSP (active) > REPL (active) > doc comments > fmt --check > benchmarking > fmt comment preservation
