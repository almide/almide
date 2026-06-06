# Mission

**Almide is the language LLMs can write most accurately.** Every design decision serves one metric: modification survival rate.

# Critical Safety Rules

- **NEVER run `git checkout`, `git restore`, or `git stash` on files you did not modify yourself.** Other agents may be working on those files concurrently. Reverting their changes destroys their work and cannot be recovered.
- **NEVER run destructive git operations without explicit user confirmation.** This includes `git reset`, `git checkout -- <file>`, `git clean`, and `git stash drop`.
- **If you see unexpected changes in `git status`, ASK the user before touching them.** They may belong to another agent or an in-progress task.

# Project Rules

## Branch Strategy

- **main** — protected. Never commit directly. Only accepts PRs from `develop`
- **develop** — the working branch. All commits go here
- Always confirm `git branch` before committing

## Git Commit Rules

- Write commit messages in **English only**
- No prefix (feat:, fix:, etc.)
- Keep it to one concise line
- Focus on what changed, not why
- Commit messages must be in **English only** (enforced by `english-only` commit-msg hook)

## Release Procedure

The release workflow (`.github/workflows/release.yml`, triggered by `v*` tag pushes) **owns release creation**. Do NOT manually `gh release create` after pushing a tag — you will race the workflow and the workflow step will fail with "a release with the same tag name already exists".

Correct flow:

1. Bump `Cargo.toml` version on `develop`, commit, push
2. Wait for `develop` CI to be green
3. PR `develop → main`, merge (requires green CI — do not force-merge releases)
4. `git tag vX.Y.Z <merge-commit>` and `git push origin vX.Y.Z`
5. **Let the workflow create the release.** It auto-generates notes from commits.
6. If you want custom notes, edit after the workflow completes: `gh release edit vX.Y.Z --notes "..."`

If you already shipped a broken release:

- `gh release delete vX.Y.Z --yes`
- `git push --delete origin vX.Y.Z && git tag -d vX.Y.Z`
- Fix on `develop`, bump to `vX.Y.(Z+1)`, repeat

## Development Setup

After cloning, install the git hooks:

```bash
brew install lefthook  # or: https://github.com/evilmartians/lefthook
lefthook install
```

## Project Overview

Almide is a programming language (.almd files) compiled via a pure-Rust compiler with dual-target codegen (Rust, WASM).

- **Architecture**: [docs/ARCHITECTURE.md](./docs/ARCHITECTURE.md) — compiler pipeline, module map
- **Language reference**: [docs/CHEATSHEET.md](./docs/CHEATSHEET.md) — syntax, stdlib, idioms (for AI code generation)
- **Stdlib spec**: [docs/STDLIB-SPEC.md](./docs/STDLIB-SPEC.md) — stdlib function reference
- **Module system**: [docs/specs/module-system.md](./docs/specs/module-system.md) — import, サブモジュール, ダイヤモンド依存
- **Package system**: [docs/specs/package-system.md](./docs/specs/package-system.md) — 依存管理, MVS, バージョン共存

## Building & Installing

After modifying compiler source, always rebuild and install so the PATH binary is up to date:

```bash
make install   # cargo build --release + install to ~/.local/bin/almide
```

## Usage

```bash
cargo build --release

almide run app.almd              # Compile + execute
almide run app.almd --target wasm  # Compile + execute on wasmtime (byte-identical to native)
almide run app.almd -- arg1 arg2 # Program args go after --
almide build app.almd -o app     # Build binary
almide build app.almd --target wasm  # Build WASM
almide test                      # Find all .almd with test blocks (recursive)
almide test spec/lang/           # Run tests in a directory
almide test spec/lang/expr_test.almd  # Run a single test file
almide test --run "pattern"      # Filter tests by name
almide compile                    # Module interface (project)
almide compile parser             # Module interface (by name)
almide compile app.almd --json    # Module interface (JSON)
almide check app.almd             # Type check only
almide fmt app.almd               # Format source
almide clean                     # Clear dependency cache
almide add almide/pkg@v0.1.0    # Add dependency (github.com/almide/ default)
almide deps                      # List dependencies
almide dep-path bindgen          # Print cached source dir of a dependency
almide app.almd --target rust    # Emit Rust source
almide app.almd --target rust --repr-c  # Emit with #[repr(C)]
almide app.almd --emit-ast       # Emit AST as JSON
```

## Test Structure

`almide test` recursively finds all `.almd` files containing `test` blocks.

- **Inline tests**: Write `test "name" { }` in any `.almd` file
- **Test files**: Use `*_test.almd` suffix for dedicated test files (convention)

```
spec/
├── lang/            Language feature tests (*_test.almd)
├── stdlib/          Stdlib tests (*_test.almd)
└── integration/     Multi-module / integration tests
tests/               Rust compiler unit tests (.rs, Cargo auto-discovery)
```

Run tests:
```bash
almide test                      # All .almd with test blocks (recursive)
almide test spec/lang/           # Language tests only
almide test spec/stdlib/         # Stdlib tests only
```

## Testing Rules

Changes to the compiler MUST be verified against **all exercises and tests**:

```bash
almide test
```

When adding or modifying stdlib functions:
- Add/edit the definition in `stdlib/defs/<module>.toml` (type sig + codegen templates)
- Implement the Rust runtime in `runtime/rust/<module>.rs`
- `cargo build` auto-generates all codegen dispatch — no manual edits needed
- Write a test in `spec/stdlib/` (as `*_test.almd` or inline `test` block)

When modifying codegen:
- Test ownership: variables used after `for...in` must still work
- Test effect fn: `fs.read_text()` inside effect fn must compile without manual `?`
- Test that generated Rust compiles without warnings

## Writing Idiomatic Almide

When writing `.almd` code (stdlib, packages, examples), follow these idioms:

### Prefer match over if/else chains
```almide
// ✗ avoid
if kind == "int" then "i64"
else if kind == "float" then "f64"
else if kind == "string" then "String"
else "unknown"

// ✓ use match
match kind {
  "int"    => "i64",
  "float"  => "f64",
  "string" => "String",
  _        => "unknown",
}
```

### Prefer list combinators over var + for
```almide
// ✗ avoid
var result: List[String] = []
for item in items {
  result = result + [transform(item)]
}
result

// ✓ use map / flat_map / filter_map
items |> list.map((item) => transform(item))

// ✓ with index: list.enumerate
cases |> list.enumerate |> list.map((entry) => {
  let (idx, case) = entry
  "${int.to_string(idx)}: ${case}"
})
```

### Prefer list.find over var + for search
```almide
// ✗ avoid
var result = json.null()
for t in types {
  if get_str(t, "name") == name then result = t else result = result
}
result

// ✓ use list.find
types |> list.find((t) => get_str(t, "name") == name) ?? json.null()
```

### Prefer recursion over var + while + flag
```almide
// ✗ avoid
var i = p
var go = true
while i < len and go {
  let c = peek(t, i)
  if is_ws(c) then { i = i + 1 }
  else { go = false }
}
i

// ✓ use recursion
fn skip_ws(t: String, p: Int) -> Int =
  if p < string.len(t) and is_ws(peek(t, p)) then skip_ws(t, p + 1)
  else p

// ✓ or use scan_while for common patterns
fn scan_while(t: String, p: Int, pred: (String) -> Bool) -> Int =
  if p < string.len(t) and pred(peek(t, p)) then scan_while(t, p + 1, pred)
  else p
```

### Use heredoc for static text blocks
```almide
// ✗ avoid: array of strings joined
let code = [
  "#[no_mangle]",
  "pub extern \"C\" fn alloc(len: i32) -> *mut u8 {",
  "    let buf = Vec::<u8>::with_capacity(len as usize);",
  "    buf.as_mut_ptr()",
  "}",
] |> list.join("\n")

// ✓ use heredoc: no escapes, reads like actual code
let code = """
  #[no_mangle]
  pub extern "C" fn alloc(len: i32) -> *mut u8 {
      let buf = Vec::<u8>::with_capacity(len as usize);
      buf.as_mut_ptr()
  }
  """
```

### Use pipe for data transformation chains
```almide
// ✓ pipe chains
fields
  |> list.map((f) => "${get_str(f, "name")}: ${go_type(get_type(f))}")
  |> list.join(", ")
```

### Use ?? for fallback, ? for Result→Option, ! for unwrap
```almide
value.get(v, key) ?? default_val      // Result/Option fallback
json.get(v, "field")?                  // Result → Option
parse_int(s)!                          // unwrap, propagate err (effect fn only)
```

### Imports
- Stdlib modules (`string`, `int`, `float`, `list`, `value`, `map`, `set`, etc.) are auto-imported — do NOT write `import string`
- `json` requires explicit `import json`
- External packages require `import pkg_name`
- Package self-reference: `import self as pkg_name`

## Key Design Decisions

- **Multi-target**: Same IR emits to Rust or WASM via `--target rust|wasm` (TS codegen は削除済み)
- **Codegen v3**: Nanopass pipeline (semantic rewrites) + TOML template renderer (syntax)
- **Effect fn (Rust)**: `effect fn` → `Result<T, String>`, auto `?` propagation
- **`==`/`!=`**: `almide_eq!` macro in Rust
- **`+`**: Concatenation for strings and lists (overloaded with addition)
- **Diagnostics**: Every error includes file:line, context, and actionable hint

## Repo Boundary: almide vs almide-dojo

- **This repo** = compiler correctness. `spec/` tests, `cargo test`, grammar-lab experiments, lang-bench.
- **[almide/almide-dojo](https://github.com/almide/almide-dojo)** = LLM writability. Daily MSR measurement, task bank, malicious-hint detection, diagnostics feedback loop.
- New MSR work goes to Dojo. `research/benchmark/msr/` and `research/benchmark/framework/` are archived.
- The bridge: Dojo's PR gate will run a task subset as part of this repo's CI (future).

## Behavior Contracts

Every observable cross-target promise (native Rust ⇄ wasm32: stdout, stderr, exit
code) is a named `[[contract]]` in [docs/contracts/contracts.toml](./docs/contracts/contracts.toml),
traceable to executable evidence: a `spec/wasm_cross/*.almd` fixture, a
differential fuzz, an emit-time Σ-probe, or a Lean theorem. The index is
[docs/contracts/README.md](./docs/contracts/README.md) (auto-generated).

**Changing observable behavior = update the contract ledger in the SAME PR.**

- A new behavior = a new `C-NNN` + ≥1 fixture, and the fixture declares it on a
  `// @contract: C-NNN` header line (the reverse link is mandatory and symmetric).
- Removing a divergence = flip `status` to `active`, drop the flag, lower the
  ratchet — same PR. The `flagged-for-revision` count may only go DOWN.
- `scripts/check-contracts.sh` (CI `checks` job + a lefthook pre-commit hook)
  enforces that every contract has evidence of class ≥ fixture, every fixture
  names its contract(s), and the link is bidirectional. The evidence-class
  vocabulary is shared with the rt-oracle-registry via
  `scripts/lib/contract-classes.txt`.

## Documentation

- 言語仕様: `docs/specs/` — ルールは [docs/specs/CLAUDE.md](./docs/specs/CLAUDE.md)
- コンパイラ設計: `docs/ARCHITECTURE.md`
- 言語リファレンス: `docs/CHEATSHEET.md`
- ロードマップ: `docs/roadmap/`
- 振る舞い契約: `docs/contracts/` — クロスターゲット等価性の契約台帳
