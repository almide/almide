<!-- description: Interactive shell replacing Bash/Zsh with type-safe LLM-friendly syntax -->
# Almide Shell

An interactive shell that replaces Bash/Zsh — combining Almide's type system, Result-based error handling, and LLM-friendly syntax with OS-level process execution.

## Vision

**The shell that AI agents can script most accurately.** Bash syntax is the #1 source of LLM-generated bugs (quoting, arrays, error handling). Almide Shell eliminates these by design.

```
almide> let files = fs.glob("**/*.rs")
almide> files.map(f => { name: f, loc: fs.read_text(f).lines().len() })
         |> list.sort_by(r => r.loc)
         |> list.reverse()
         |> list.each(r => println("{r.loc}\t{r.name}"))

almide> exec("git", ["status"])
almide> env.get("HOME")
```

## Architecture

`almide shell` launches an interactive REPL that extends the language REPL with process execution and environment control.

```
input → parse → check → lower → emit Rust → compile → execute → print result
                                    ↑
                          stdlib: exec, env, pipe
```

## Phase 0: Foundation — stdlib modules

New stdlib modules required before the shell can exist.

### `process` module

| Function | Signature | Description |
|----------|-----------|-------------|
| `exec` | `fn(cmd: String, args: List[String]) -> ProcessResult` | Run command, wait, return result |
| `spawn` | `fn(cmd: String, args: List[String]) -> Process` | Start without waiting |
| `shell` | `fn(cmd: String) -> ProcessResult` | Run via `/bin/sh -c` (escape hatch) |

```
record ProcessResult {
  stdout: String
  stderr: String
  code: Int
}
```

### `env` module

| Function | Signature | Description |
|----------|-----------|-------------|
| `get` | `fn(key: String) -> Option[String]` | Read env var |
| `set` | `fn(key: String, value: String) -> ()` | Set env var |
| `all` | `fn() -> Map[String, String]` | All env vars |
| `home` | `fn() -> String` | `$HOME` shorthand |
| `cwd` | `fn() -> String` | Current working directory |
| `set_cwd` | `fn(path: String) -> ()` | `cd` equivalent |

### Runtime

- Rust: `std::process::Command`, `std::env`
- TS: `child_process.execSync`, `process.env`

## Phase 1: Basic REPL

`almide shell` subcommand — interactive evaluation with state.

- [ ] Expression evaluation with result display
- [ ] `let` / `var` / `fn` / `type` persist across inputs
- [ ] Multi-line input (detect incomplete blocks)
- [ ] Error display (same format as compiler)
- [ ] History (arrow keys, persistent via `~/.almide/shell_history`)
- [ ] Line editing via `rustyline`

## Phase 2: Shell Features

Features that make it a real shell replacement.

- [ ] **Bare command execution** — `git status` without `exec()` wrapping
- [ ] **Process pipes** — `exec("cat", ["data.csv"]) |> exec("sort") |> exec("uniq")`
- [ ] **cd** — builtin `cd` calls `env.set_cwd()`
- [ ] **Shebang** — `#!/usr/bin/env almide run` for executable `.almd` scripts
- [ ] **Exit codes** — `exit(1)`, `$?` equivalent via `last_result.code`
- [ ] **Tab completion** — functions, modules, file paths, command names from `$PATH`
- [ ] **Prompt customization** — `~/.almide/shellrc.almd`

## Phase 3: AI-Native Shell

The differentiator — natural language to Almide code generation.

- [ ] **Natural language mode** — prefix with `?` to describe intent, get generated Almide code
- [ ] **Review-before-execute** — generated code shown for approval, type-checked before run
- [ ] **Script capture** — session history exportable as `.almd` script
- [ ] **Error explanation** — failed commands get AI-generated explanation + fix suggestion

```
almide> ? find all TODO comments in Rust files
  fs.glob("**/*.rs")
    |> list.flat_map(f => {
      fs.read_text(f).lines()
        |> list.enumerate()
        |> list.filter((i, line) => line.contains("TODO"))
        |> list.map((i, line) => "{f}:{i + 1}: {line.trim()}")
    })
    |> list.each(println)

  [Enter] run / [e] edit / [Esc] cancel
```

## Phase 4: Production Shell

- [ ] **Job control** — `&` for background, Ctrl+Z suspend
- [ ] **Signal handling** — Ctrl+C, trap equivalent
- [ ] **Aliases** — `alias gs = exec("git", ["status"])`
- [ ] **Startup file** — `~/.almide/shellrc.almd` sourced on launch
- [ ] **Login shell** — register in `/etc/shells`, usable as default shell

## Dependencies

| Dependency | Phase | Purpose |
|------------|-------|---------|
| `process` stdlib module | 0 | External command execution |
| `env` stdlib module | 0 | Environment variable access |
| `rustyline` crate | 1 | Line editing, history, completion |
| IR Interpreter (optional) | 1+ | Instant eval without rustc |
| LLM integration | 3 | Natural language → code |

## Affected Files

| File | Change |
|------|--------|
| `stdlib/defs/process.toml` (new) | Process execution type sigs |
| `stdlib/defs/env.toml` (new) | Environment variable type sigs |
| `src/emit_rust/core_runtime.txt` | Runtime for exec/env |
| `src/emit_ts_runtime.rs` | TS runtime for exec/env |
| `src/cli.rs` | Add `shell` subcommand |
| `src/shell.rs` (new) | Shell REPL loop, state accumulation |
| `Cargo.toml` | Add `rustyline` |
