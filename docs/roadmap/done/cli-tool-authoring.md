<!-- description: Fix issues discovered while implementing CLI tool benchmarks -->
# CLI Tool Authoring Issues

Issues discovered while implementing the miniconf benchmark in Almide. Both fixed.

## 1. `err()` exit no longer prints error value ✅

**Problem**: When `effect fn main` returned `err(Custom("e"))`, the generated Rust wrapper printed it via `eprintln!("{}", e)`, polluting CLI output.

**Fix**: Changed to `let _ = e;` — the error value is silently discarded. CLI tools print their own messages via `println` before returning `err()`.

**Location**: `src/emit_rust/program.rs`

## 2. `almide run` accepts program args without `--` ✅

**Problem**: `almide run app.almd check foo.conf` treated `check` as an almide argument.

**Fix**: Changed clap config to `trailing_var_arg = true` + `allow_hyphen_values = true`. All args after the source file are passed to the program.

**Location**: `src/main.rs`
