# CLI Tool Authoring Issues [NEW]

Issues discovered while implementing the miniconf benchmark in Almide.

## 1. `err()` exit prints error value to stderr

### Problem

When `effect fn main` returns `err(Custom("e"))`, the generated Rust wrapper catches the error and prints it via `eprintln!("{}", e)`. This pollutes the output of CLI tools.

```
$ ./miniconf check bad.conf
ERROR: line 2: unterminated string    ← user's println
Custom("e")                           ← runtime prints this automatically
```

CLI tools want to print their own error message via `println`, then exit with code 1 cleanly. The extra output from the runtime breaks exact-match test expectations.

### Current workaround

```almide
println("ERROR: ...")
process.exit(1)
ok(())  (* unreachable, but needed to satisfy the type *)
```

### Proposed fix

- Suppress `eprintln!` in the generated main wrapper, OR
- Only print if the error has meaningful content (not a sentinel like `Custom("e")`), OR
- Support a Never/bottom type so `process.exit(1)` can be used without `ok(())` after it

### Location

`src/emit_rust/program.rs` line ~195: `eprintln!("{}", e);`

## 2. `almide run` requires `--` to pass program arguments

### Problem

```bash
almide run app.almd check foo.conf
# → 'check' is interpreted as an almide argument → error
```

Must use `--` explicitly:

```bash
almide run app.almd -- check foo.conf
```

### Impact

Build scripts that generate wrapper shells must include `--`, which is easy to forget. The benchmark's `build-conf.sh` was broken by this.

### Proposed fix

- Change clap config: after the file argument, treat all remaining args as program args without requiring `--`
- Or use `allow_hyphen_values` + positional args instead of `last = true`

### Location

`src/main.rs` line ~40: `#[arg(last = true)] program_args: Vec<String>`

## Priority

- Issue 1: High — LLMs naturally use `err()` pattern and get bitten by the extra output
- Issue 2: Medium — can be covered by build script templates
