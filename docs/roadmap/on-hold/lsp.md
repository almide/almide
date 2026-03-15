# LSP Server [ACTIVE]

Language Server Protocol implementation for editor integration. Split from [tooling.md](../on-hold/tooling.md) as highest-priority tooling item.

## Why This Is Critical

Almide currently has **zero editor integration**. No completion, no jump-to-definition, no hover types, no inline errors. This is the single biggest barrier to adoption — developers won't use a language they can't navigate in their editor.

## Architecture

```
Editor (VS Code / Neovim / etc.)
    ↕ LSP JSON-RPC (stdio)
almide lsp
    │
    ├── Document sync (open/change/close)
    ├── Lexer + Parser (per-file, incremental)
    ├── Checker (per-file, cached)
    └── Responses (completions, diagnostics, hover, etc.)
```

### Implementation Options

**Option A: Built into `almide` binary**
- Add `almide lsp` subcommand
- Reuse existing lexer/parser/checker directly
- No separate process or IPC
- Pro: Zero additional dependencies
- Con: Compiler pipeline not designed for incremental use

**Option B: Separate `almide-lsp` binary**
- Import `almide` as a library crate (`src/lib.rs` already exists)
- Add LSP protocol handling via `tower-lsp` or `lsp-server` crate
- Pro: Cleaner separation, can add caching independently
- Con: Additional binary to distribute

**Recommendation:** Option A for Phase 1 (simple, no new deps). Option B if performance becomes an issue.

## Phase 1: Core Features

### 1a. Diagnostics (textDocument/publishDiagnostics)

On file save or change, run lexer → parser → checker and publish errors/warnings as LSP diagnostics.

- Map `Diagnostic` spans to LSP `Position` (line/col)
- Include hints as `relatedInformation`
- Severity mapping: error → Error, warning → Warning

### 1b. Hover (textDocument/hover)

Show type information on hover.

- For variables: show `name: Type` from `VarTable` / `expr_types`
- For function calls: show signature `fn name(params) -> RetType`
- For module calls: show stdlib signature from `stdlib_sigs.rs`

### 1c. Go to Definition (textDocument/definition)

- Variables → declaration site (from `VarInfo.span`)
- Functions → `fn` declaration line
- Types → `type` declaration line
- Module functions → link to stdlib docs or source file

### 1d. Completion (textDocument/completion)

- After `.` → UFCS candidates for the receiver type + field names for records
- After `module.` → list module functions
- Top-level → function names, type names, keywords
- Inside patterns → constructor names for the matched type

## Phase 2: Enhanced Features

- **Signature help** (textDocument/signatureHelp) — parameter hints during function calls
- **Document symbols** (textDocument/documentSymbol) — outline view
- **Rename** (textDocument/rename) — rename variable/function across file
- **Formatting** (textDocument/formatting) — delegate to `almide fmt`
- **Code actions** — "add missing match arm", "import module"

## Phase 3: Cross-File

- **Multi-file project support** — resolve imports, check across modules
- **Workspace symbols** — find function/type across all project files
- **Go to definition across modules** — jump to imported module's source

## Dependencies

- LSP protocol library: `lsp-server` (lightweight, used by rust-analyzer) or `tower-lsp` (async, tower-based)
- Incremental parsing: Currently full re-parse on every change. Acceptable for Phase 1 (files are typically small). Tree-sitter integration possible for Phase 3.

## Editor Support

- **VS Code**: Extension already exists (unpublished). Add LSP client configuration.
- **Neovim**: Native LSP client. Just needs `almide lsp` binary path.
- **Other editors**: Any LSP-compatible editor works automatically.

## Affected Files

| File | Change |
|------|--------|
| `src/cli.rs` | Add `lsp` subcommand |
| `src/main.rs` | Dispatch to LSP handler |
| `src/lsp.rs` (new) | LSP protocol handling |
| `Cargo.toml` | Add `lsp-server` dependency |

## Priority

P0 for Phase 1 (diagnostics + hover + go-to-def + completion). This is the single highest-impact tooling improvement.
