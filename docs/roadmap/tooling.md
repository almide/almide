# Toolchain Extensions

## REPL (`almide repl`)
Evaluate expressions interactively. For learning and prototyping.
- Runs the AST → Rust emit → rustc → execute pipeline interactively
- History and completion

## LSP (`almide lsp`)
Editor integration. Equivalent to Go's gopls.
- Completion (function names, module functions, type names)
- Jump to definition
- Type display on hover
- Error display (checker integration)
- Formatting (almide fmt integration)

## Documentation Generation (`almide doc`)
- Add `///` doc comments to lexer/AST
- Generate HTML/Markdown documentation for modules, functions, and types

## Benchmarking (`almide bench`)
```almide
bench "list sort 1000 elements" {
  let xs = list.reverse(range(0, 1000))
  list.sort(xs)
}
```

## Package Registry
- `almide add fizzbuzz` fetches from a central registry
- Currently only Git URL direct references are supported
- Version resolution (semver)

## Comment Preservation in `almide fmt`
- Keep comments as tokens in the lexer
- Attach comment information to AST nodes
- Restore comments when formatting

## Priority
LSP > REPL > doc comments > benchmarking > registry > fmt comment preservation
