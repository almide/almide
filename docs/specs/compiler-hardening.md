# Compiler Hardening Specification

> Verified by: all exercises pass without panics; ICE fallbacks tested via missing emitter coverage audit.

---

## 1. Panic Elimination

All `unwrap()` and `panic!()` calls in the compiler source have been replaced with safe alternatives. The compiler never crashes on invalid input.

### 1.1 Parser

The parser previously called `panic!("Parser: no tokens available")` when the token stream was empty. This is replaced with a static EOF token fallback — the parser always has a valid token to inspect.

### 1.2 Emitter

- Character case conversion: `.unwrap()` replaced with `.unwrap_or(c)` — if case conversion fails, the original character is preserved (`emit_ts/mod.rs`).
- Do-block final expression: `final_expr.unwrap()` replaced with `.expect("guarded by is_some()")` — only reachable when `is_some()` was already checked (`emit_rust/blocks.rs`).

### 1.3 Checker

Import resolution: `path.last().unwrap()` replaced with `.map().unwrap_or()` — handles empty path segments gracefully (`check/mod.rs`).

### 1.4 CLI

File I/O operations in `init` and `build` commands: `.unwrap()` replaced with `if let Err` blocks that print an error message and call `exit(1)` (`cli.rs`).

### 1.5 Generated Code

- `/dev/urandom` read: `.unwrap()` replaced with `.map_err()?` error propagation (random module runtime).
- `UNIX_EPOCH` duration: `.unwrap()` replaced with `.unwrap_or_default()` (time/env module runtimes).
- Thread spawn/join: `.unwrap()` replaced with `.expect()` with descriptive messages (`emit_rust/program.rs`).
- Split results in project handling: `.unwrap()` replaced with `.expect()` with reason (`project.rs`).

---

## 2. Codegen Fallbacks (Internal Compiler Error)

All 16 stdlib module fallbacks in `emit_rust/calls.rs` previously generated `todo!()` in the output Rust code. This meant a mismatch between `lookup_sig()` (type checker) and the emitter would silently produce code that compiles but panics at runtime.

### 2.1 Current Behavior

When the emitter encounters a stdlib function call that has a type signature in `lookup_sig()` but no corresponding codegen implementation, the compiler:

1. Prints an error to stderr: `internal error: unimplemented codegen for <module>.<function>`
2. Exits with code 70 (EX_SOFTWARE, the sysexits convention for internal software errors)

This catches signature/emitter mismatches at compile time rather than at runtime.

### 2.2 Verified Coverage

All stdlib function signatures registered in `lookup_sig()` have corresponding emitter implementations. There is no gap between the type checker and code generator.

---

## 3. Error Message Improvements

All compiler errors follow a consistent format with three components:

```
error: <message>
  at <file>:<line>:<column>
  hint: <actionable suggestion>
```

### 3.1 File and Line Information

Every diagnostic includes the source file path, line number, and column number. The `Diagnostic` struct in `diagnostic.rs` carries `message`, `hint`, and `context` fields.

### 3.2 Actionable Hints

Every error message includes a `hint` field that tells the user what to do:

| Error | Hint |
|-------|------|
| Import resolution failure | Lists file paths tried; suggests checking for typos |
| Effect fn called outside effect context | `add 'effect' keyword to the enclosing function` |
| `mod fn` access from external module | `'func_name' has restricted visibility and cannot be accessed from here` |
| Type mismatch | Shows expected vs actual types |

### 3.3 Interpolated String Validation

String interpolation expressions (`${expr}`) are parsed and type-checked at the checker stage. If the expression inside `${}` contains a syntax error, the compiler reports it with the correct source location rather than producing malformed output code.

### 3.4 Parser Hints

The parser provides context-specific hints for common mistakes:

| Mistake | Hint |
|---------|------|
| Lowercase type name (e.g., `int`) | Type names must start with an uppercase letter: `Int` |
| Uppercase function name (e.g., `MyFunc`) | Function names must start with a lowercase letter |
| Wrong parameter name casing | Parameter names must be lowercase |
| Incorrect pattern syntax | Provides pattern syntax guide |

---

## 4. Design Rationale

Other mainstream languages (Go, Rust, Python) never crash on invalid input — they report errors and exit cleanly. A compiler crash destroys LLM workflow because the error message is lost in a stack trace. Clean error messages with hints allow LLMs to self-correct on the next attempt, directly improving modification survival rate.
