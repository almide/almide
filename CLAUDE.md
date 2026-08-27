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
   - **Release blockers gate the final tag** (#1482): the workflow refuses a
     final release while an issue labeled `I-unsound` / `I-miscompile` /
     `I-divergence` / `regression` is open — the closed set in
     [docs/project/ISSUE-TAXONOMY.md](./docs/project/ISSUE-TAXONOMY.md).
     Check ahead with `bash scripts/count-release-blockers.sh`.
   - **RC channel** (#1484): for a release carrying language-surface or
     compiler-behaviour changes, tag `vX.Y.Z-rc1` first — it publishes as a
     GitHub PRERELEASE (excluded from "latest"; the blocker gate prints but
     does not fail) and buys a soak window. Tag the final `vX.Y.Z` once the
     soak is clean; the length is a per-release human call.
   - **Interface diff** (#1488): before the final tag, run
     `bash scripts/check-interface-diff.sh vPREV vX.Y.Z` — it classifies the
     public stdlib surface as identical / additive / breaking from the
     committed signature indexes, and refuses a breaking diff unless declared
     with `--allow-breaking` (a removal needs its `@deprecated` window and,
     when it can break written code, a `proofs/dialect-epochs.toml` entry).
5. **Let the workflow create the release.** It auto-generates notes from commits.
6. If you want custom notes, edit after the workflow completes: `gh release edit vX.Y.Z --notes "..."`
7. **Seal the release evidence** (audit freeze): `bash scripts/release-seal.sh gen vX.Y.Z`,
   fill the `[recorded]` fields (the release-gate fuzz run, the asset inventory), commit
   `proofs/releases/vX.Y.Z.toml` on `develop`. CI re-measures every `[derived]` field
   against the tag forever after — the seal is the release's immutable evidence record.

If you already shipped a broken release:

- `gh release delete vX.Y.Z --yes`
- `git push --delete origin vX.Y.Z && git tag -d vX.Y.Z`
- Fix on `develop`, bump to `vX.Y.(Z+1)`, repeat

## Development Setup

After cloning, fetch the submodules and install the git hooks:

```bash
git submodule update --init --recursive
brew install lefthook  # or: https://github.com/evilmartians/lefthook
lefthook install
```

Submodules (`actions/checkout` does NOT fetch them, so CI never sees these — they are
local-only conveniences and nothing in the build depends on them):

| Path | Repo | Used by |
|---|---|---|
| `grammar/` | [almide/almide-grammar](https://github.com/almide/almide-grammar) | grammar definition consumed by the editor/tree-sitter repos |
| `research/benchmark/lang-bench/upstream/` | [mame/ai-coding-lang-bench](https://github.com/mame/ai-coding-lang-bench) | `/almide-lang-bench` |

## Project Overview

Almide is a programming language (.almd files) compiled via a pure-Rust compiler with dual-target codegen (Rust, WASM).

- **Architecture**: [docs/ARCHITECTURE.md](./docs/ARCHITECTURE.md) — compiler pipeline, module map
- **Language reference**: [docs/CHEATSHEET.md](./docs/CHEATSHEET.md) — syntax, stdlib, idioms (for AI code generation)
- **Stdlib**: self-hosted `.almd` modules in `stdlib/` — registry in `crates/almide-types/src/stdlib_info.rs`, per-module docs in [docs/stdlib/](./docs/stdlib/)
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
almide check app.almd --profile critical --allow IO  # Critical profile (#567): bounded rules on every fn, capabilities deny-all
almide fmt app.almd               # Format source
almide fmt --check spec/          # Formatting gate (non-zero on drift)
almide fmt --no-import-edit stdlib/  # Format WITHOUT touching imports (splice-context sources)
almide clean                     # Clear dependency cache
almide add almide/pkg@v0.1.0    # Add dependency (github.com/almide/ default)
almide update [dep]              # Advance a locked git dep to its ref's remote head (tags never move)
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

When adding or modifying stdlib functions (the stdlib is self-hosted — `stdlib/defs/*.toml` and `runtime/rust/` no longer exist):
- Write the implementation in pure Almide in `stdlib/<module>[_<part>].almd`
- For WASM/v1 coverage, register it in `crates/almide-types/src/self_host_registry.rs` (add `(crate::embedded::SRC_<STEM>, &[("impl_fn", "module.fn")])` to `self_host_runtime()`); adjust `crates/almide-mir/src/purity.rs` if needed. Unlinked stdlib calls are a wall error in the wasm renderer
- Only if a native intrinsic is required: implement `almide_rt_*` in `runtime/rs/src/<module>.rs` and declare it with `@intrinsic("almide_rt_*")` in the module's `.almd`
- New modules: update `STDLIB_MODULES` / `BUNDLED_MODULES` / `bundled_source()` (and `AUTO_IMPORT_BUNDLED` if auto-imported) in `crates/almide-types/src/stdlib_info.rs`
- Write a test in `spec/stdlib/` (as `*_test.almd` or inline `test` block); add the almide-interp bridge glue so the 3-way oracle covers it
- **API families are extended by matrix, never point-wise.** If the function belongs to a family (numeric conversion trio `to_T`/`to_T_saturating`/`to_T_checked`, per-type bounds, Codec coverage, …), state the family's completeness rule (which cells must exist, which omissions are intentional — e.g. lossy⇒trio, lossless⇒plain only) and land/extend the executable matrix gate that asserts it in the SAME PR. A surface with a hand-maintained shape will drift; a gated matrix cannot

When modifying codegen:
- Test ownership: variables used after `for...in` must still work
- Test effect fn (ADR-0008): propagation is EXPLICIT — `fs.read_text(p)!` compiles and propagates; a bare `fs.read_text(p)` statement is E042 (must-use), an un-annotated `let x = fs.read_text(p)` is E041; `let _ = f()` discards without propagating
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

// ✓ with index: list.enumerate — destructure the pair in the parameter
cases |> list.enumerate |> list.map(((idx, case)) => "${idx}: ${case}")

// ✓ FALLIBLE body: the callback's `!` instantiates the fallible form
//   (first-err short-circuit, ADR-0006) — never var + for
files |> list.map((f) => read_meta(f)!)!
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
- Stdlib modules (`string`, `int`, `float`, `list`, `value`, `map`, `set`, `math`, `datetime`, `error`, `bytes`, sized ints, etc.) are auto-imported — do NOT write `import string`. Authoritative lists: `import_table.rs` seed + `AUTO_IMPORT_BUNDLED` in `stdlib_info.rs`
- `json`, `fs`, `http`, `env`, `io`, `random`, `regex`, `process`, `testing`, `url` require explicit `import`
- External packages require `import pkg_name`
- Package self-reference: `import self as pkg_name`

## Key Design Decisions

- **Multi-target**: Same IR emits to Rust or WASM via `--target rust|wasm` (TS codegen は削除済み)
- **Codegen v3**: Nanopass pipeline (semantic rewrites) + TOML template renderer (syntax)
- **Effect fn (Rust)**: `effect fn` → `Result<T, String>`。伝搬は **明示のみ**（ADR-0008）— `expr!` が `?` に落ちる。暗黙伝搬は E041、statement 位置の握り潰しは E042
- **`==`/`!=`**: `almide_eq!` macro in Rust
- **`+`**: Concatenation for strings and lists (overloaded with addition)
- **Diagnostics**: Every error includes file:line, context, and actionable hint

## Repo Boundary: almide vs almide-dojo

- **This repo** = compiler correctness. `spec/` tests, `cargo test`, grammar-lab experiments, lang-bench.
- **[almide/almide-dojo](https://github.com/almide/almide-dojo)** = LLM writability. Daily MSR measurement, task bank, malicious-hint detection, diagnostics feedback loop.
- All MSR work goes to Dojo. `research/benchmark/msr/` and `research/benchmark/framework/` were removed from this repo (2026-08-08) — the local harness had an empty `results/` since the April 2026 hand-off, so running it would have overwritten Dojo's README number with a locally-measured one. Recover with `git log --diff-filter=D -- research/benchmark/msr/`.
- Still here and live: `research/benchmark/perf/` (gated by `scripts/check-perf-ratio.sh`), `research/benchmark/lang-bench/`, `research/benchmark/exercises/` (corpus for `tools/v1_gap_measure.py`), `research/grammar-lab/`, `research/spike/`.
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
- **What is proven vs what is trusted**: [docs/contracts/proven-vs-trusted.md](./docs/contracts/proven-vs-trusted.md)
  — the boundary map, and what each gate does and does not claim.
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
