# Evaluation: option module (10 functions)

> Research document for the proposed option module in stdlib-1.0.md.
> No code changes. Evaluation only.

---

## 1. Current State in Almide

Almide already has deep, built-in Option support at the language level:

| Feature | Implementation |
|---------|---------------|
| `some(x)` | AST node `Expr::Some`, lowers to `IrExprKind::OptionSome` |
| `none` | AST node `Expr::None`, lowers to `IrExprKind::OptionNone` |
| `unwrap_or(opt, default)` | Builtin in type checker (`check/calls.rs:156`), special-cased in both emitters |
| Pattern matching `some(x)` / `none` | AST patterns `Pattern::Some` / `Pattern::None`, exhaustiveness-checked |
| `Option[A]` type | First-class in the type system (`Ty::Option(Box<Ty>)`) |

**Rust backend**: `some(x)` emits `Some(x)`, `none` emits `None`, `unwrap_or` emits `.unwrap_or(default)`.

**TS backend**: `some(x)` erases to `x`, `none` erases to `null`, `unwrap_or` calls `__almd_unwrap_or(x, d)` which is `x !== null ? x : d`.

The `option` module name is already listed in `PRELUDE_MODULES` (stdlib.rs:13) but has no TOML definition, no generated signatures, and no generated codegen calls. It is a placeholder.

---

## 2. Per-Function Evaluation

### 2.1 `option.map(opt, f) -> Option[B]`

**Cross-language comparison**:
| Language | Equivalent |
|----------|-----------|
| Rust | `opt.map(f)` |
| Kotlin | `opt?.let { f(it) }` |
| Swift | `opt.map(f)` |
| Gleam | `option.map(opt, f)` |
| Haskell | `fmap f opt` |

**Verdict: NEEDED.** This is the most important function in the module. Without it, users must write `match opt { some(x) => some(f(x)), none => none }` every time they want to transform an Option's inner value. Every language with Option/Maybe has this.

**Naming: Correct.** Consistent with `list.map`, `result.map`. The verb "map" means "transform contents" across all Almide modules.

### 2.2 `option.flat_map(opt, f) -> Option[B]` where f: `Fn[A] -> Option[B]`

**Cross-language comparison**:
| Language | Equivalent |
|----------|-----------|
| Rust | `opt.and_then(f)` |
| Kotlin | `opt?.let { f(it) }` (returns nullable) |
| Swift | `opt.flatMap(f)` |
| Gleam | `option.then(opt, f)` |
| Haskell | `opt >>= f` / `join (fmap f opt)` |

**Verdict: NEEDED.** Essential for chaining fallible lookups. Example: `list.get(xs, 0) |> option.flat_map(fn(x) => map.get(m, x))`. Without this, nested match expressions become mandatory.

**Naming: Correct.** Consistent with `result.flat_map` (which replaces `result.and_then`). The stdlib-1.0 spec explicitly renames `and_then` to `flat_map` for cross-module consistency. Good decision.

### 2.3 `option.flatten(opt) -> Option[A]` where opt: `Option[Option[A]]`

**Cross-language comparison**:
| Language | Equivalent |
|----------|-----------|
| Rust | `opt.flatten()` |
| Kotlin | (no direct equivalent) |
| Swift | (no direct equivalent, but `Optional<Optional<T>>` auto-bridges) |
| Gleam | `option.flatten(opt)` |
| Haskell | `join opt` |

**Verdict: NICE TO HAVE.** Less frequently needed than `map` or `flat_map`, but logically required for completeness. `flatten` = `flat_map(opt, fn(x) => x)`. It parallels `list.flatten` and the proposed `result.flatten`. Including it maintains the "wrapper types have the same vocabulary" principle.

**Naming: Correct.** Consistent with `list.flatten`, `result.flatten`.

### 2.4 `option.unwrap_or(opt, default) -> A`

**Cross-language comparison**:
| Language | Equivalent |
|----------|-----------|
| Rust | `opt.unwrap_or(default)` |
| Kotlin | `opt ?: default` |
| Swift | `opt ?? default` |
| Gleam | `option.unwrap(opt, default)` |
| Haskell | `fromMaybe default opt` |

**Verdict: NEEDED, BUT OVERLAP EXISTS.**

This duplicates the existing builtin `unwrap_or(opt, default)`. The builtin is polymorphic -- it handles both `Option[A]` and `Result[A, E]` (see `check/calls.rs:156-163`). It also works via UFCS: `opt |> unwrap_or(42)`.

The question is whether `option.unwrap_or` should coexist with the builtin or replace it. See Section 4 for analysis.

**Naming: Correct.** Matches existing builtin name.

### 2.5 `option.unwrap_or_else(opt, f) -> A`

**Cross-language comparison**:
| Language | Equivalent |
|----------|-----------|
| Rust | `opt.unwrap_or_else(f)` |
| Kotlin | `opt ?: run { f() }` |
| Swift | `opt ?? f()` (but f is auto-closure) |
| Gleam | N/A (Gleam's `unwrap` is eager) |
| Haskell | `fromMaybe (f ()) opt` (lazy by default) |

**Verdict: NEEDED.** Important when the default is expensive to compute. Without this, users pay the cost of evaluating the default even when the Option is `some`. Matches `result.unwrap_or_else` for symmetry.

**Naming: Correct.** Consistent with `result.unwrap_or_else`.

### 2.6 `option.is_some(opt) -> Bool`

**Cross-language comparison**:
| Language | Equivalent |
|----------|-----------|
| Rust | `opt.is_some()` |
| Kotlin | `opt != null` |
| Swift | `opt != nil` |
| Gleam | `option.is_some(opt)` |
| Haskell | `isJust opt` |

**Verdict: NEEDED.** Common in guard conditions and filtering: `list.filter(opts, option.is_some)`. Pattern matching is the idiomatic way to decompose Options, but `is_some` is essential for predicate contexts where you don't need the inner value.

**Naming: Correct.** Follows `is_*` convention. Consistent with `result.is_ok`.

### 2.7 `option.is_none(opt) -> Bool`

**Cross-language comparison**: Mirrors `is_some` in all languages above.

**Verdict: NEEDED.** Logical complement of `is_some`. Could be derived as `!option.is_some(opt)`, but having both is standard practice and improves readability. Consistent with `result.is_err` being the complement of `result.is_ok`.

**Naming: Correct.** Follows `is_*` convention.

### 2.8 `option.to_result(opt, err_msg) -> Result[A, String]`

**Cross-language comparison**:
| Language | Equivalent |
|----------|-----------|
| Rust | `opt.ok_or(err)` / `opt.ok_or_else(|| err)` |
| Kotlin | `opt ?: throw Exception(msg)` |
| Swift | (no direct equivalent, usually `guard let`) |
| Gleam | `option.to_result(opt, err)` |
| Haskell | `maybe (Left err) Right opt` |

**Verdict: NEEDED.** Critical for bridging Option and Result worlds. Common pattern: `map.get(m, key) |> option.to_result("key not found")` to convert a lookup failure into an effect-fn-compatible Result. Gleam has the exact same function with the exact same signature.

**Naming: Correct.** Follows `to_*` = infallible convention. The error type is always String, which is consistent with Almide's `Result[A, String]` default. Mirrors `result.to_option`.

**Design note**: Fixing the error type to `String` is a deliberate simplification. Rust's `ok_or` is generic over the error type, but Almide's `Result` conventionally uses `String` for errors. This is the right call for Almide.

### 2.9 `option.to_list(opt) -> List[A]`

**Cross-language comparison**:
| Language | Equivalent |
|----------|-----------|
| Rust | `opt.into_iter().collect::<Vec<_>>()` |
| Kotlin | `listOfNotNull(opt)` |
| Swift | `opt.map { [$0] } ?? []` |
| Gleam | `option.to_list(opt)` (proposed) |
| Haskell | `maybeToList opt` |

**Verdict: NICE TO HAVE.** Useful for `list.flat_map` patterns where you want to filter and transform simultaneously: `xs |> list.flat_map(fn(x) => x.field |> option.to_list)`. Haskell's `maybeToList` is well-established. This is less critical than `map`/`flat_map`/`unwrap_or` but earns its place for compositional elegance.

**Naming: Correct.** Follows `to_*` convention. Mirrors `result.to_option`.

### 2.10 (Implicit: `option.filter`)

The spec lists 10 functions but only names 9 explicitly (map, flat_map, flatten, unwrap_or, unwrap_or_else, is_some, is_none, to_result, to_list). The count of 10 may include one unlisted function, or the count may be slightly off.

---

## 3. Missing Functions

### 3.1 `option.filter(opt, f) -> Option[A]` -- RECOMMEND ADDING

**Cross-language comparison**:
| Language | Equivalent |
|----------|-----------|
| Rust | `opt.filter(predicate)` |
| Kotlin | `opt?.takeIf { f(it) }` |
| Swift | (no direct equivalent) |
| Gleam | N/A |
| Haskell | `mfilter f opt` |

`some(x) |> option.filter(fn(v) => v > 0)` returns `some(x)` if `x > 0`, else `none`.

This is useful enough to justify inclusion. It completes the filter/map/flat_map triad that `list` already has. Without it, users write `match opt { some(x) if f(x) => some(x), _ => none }`.

### 3.2 `option.unwrap(opt) -> A` (panicking) -- DO NOT ADD

Almide's philosophy is to avoid panics. The language has no `unwrap()` that panics for Result either. Pattern matching is the safe alternative. Omitting this is correct.

### 3.3 `option.zip(a, b) -> Option[(A, B)]` -- CONSIDER FOR FUTURE

Combines two Options into a tuple Option. Useful but can wait for post-1.0.

### 3.4 `option.or(a, b) -> Option[A]` / `option.or_else(a, f)` -- CONSIDER FOR FUTURE

Fallback chaining: `option.or(primary, fallback)`. Useful in config/settings lookup chains. Could be added post-1.0.

### 3.5 `option.from_result(r) -> Option[A]` -- NOT NEEDED

Already covered by `result.to_option(r)`. Adding a mirror function would violate the "one way to do it" principle.

### 3.6 `option.get(opt) -> A` or `option.expect(opt, msg)` -- DO NOT ADD

Same reasoning as `unwrap`. Almide prefers pattern matching over panicking extractors.

---

## 4. Interaction with Existing Builtins

This is the most important design question for the option module.

### Current builtin behavior

| Syntax | Resolved by | Works on |
|--------|------------|----------|
| `some(42)` | Parser -> AST `Expr::Some` | Language-level construct |
| `none` | Parser -> AST `Expr::None` | Language-level construct |
| `unwrap_or(opt, 0)` | Type checker builtin | Both `Option[A]` and `Result[A, E]` |
| `opt |> unwrap_or(0)` | UFCS -> type checker builtin | Both `Option[A]` and `Result[A, E]` |
| `result.unwrap_or(r, 0)` | TOML-defined stdlib | `Result[A, E]` only |
| Pattern: `some(x)` / `none` | Parser -> AST patterns | Language-level |

### Interaction analysis

**`some()` and `none` are NOT affected.** They are parser-level constructs (like `ok()`, `err()`, `true`, `false`). The option module does not need to provide these -- they are language keywords, not stdlib functions.

**`unwrap_or` has a subtle overlap.** The builtin `unwrap_or` is polymorphic across Option and Result. The proposed `option.unwrap_or` and existing `result.unwrap_or` are module-specific. This creates two paths to the same operation:

```
// These should all work and produce identical output:
unwrap_or(opt, 0)              // builtin
opt |> unwrap_or(0)            // UFCS builtin
option.unwrap_or(opt, 0)       // module function (proposed)
```

**Recommendation**: Keep the builtin `unwrap_or` as-is. It is deeply embedded (checker, both emitters, UFCS resolution). The module-qualified `option.unwrap_or` should be a separate TOML-defined function that happens to do the same thing. Users who write `option.unwrap_or` get the explicit module path; users who write `unwrap_or` get the builtin shorthand. No conflict -- they are resolved through different mechanisms.

**UFCS resolution for new option functions**: Currently `stdlib.rs` `resolve_ufcs_candidates` does not list any option module methods. When the module is added, UFCS entries for `is_some`, `is_none`, `to_result`, `to_list` should be added with `vec!["option"]` since they are unambiguous. However, `map`, `flat_map`, `flatten`, `filter` are shared verbs with `list` (and `map` with `result`), so they must NOT be added to UFCS -- they require module qualification or type-based resolution.

---

## 5. Implementation Strategy: TOML vs. Bundled .almd

### Option A: TOML-defined module with runtime templates

This is how `result` is implemented today:

```toml
[map]
params = [{ name = "opt", type = "Option[A]" }, { name = "f", type = "Fn[A] -> B" }]
return = "Option[B]"
rust = "({opt}).map(|{f.args}| {{ {f.body} }})"
ts = "({opt}) !== null ? (({f.args}) => {f.body})({opt}) : null"
```

**Pros**:
- Consistent with how `result` module works
- Type signatures auto-generated into `stdlib_sigs.rs`
- Codegen templates auto-generated into `emit_rust_calls.rs` and `emit_ts_calls.rs`
- No runtime file needed for Rust (maps directly to `Option` methods)
- Minimal TS runtime needed (just null checks)

**Cons**:
- Each function needs both `rust` and `ts` templates
- Lambda parameter handling in templates is somewhat fragile

### Option B: Bundled .almd file

```almide
fn map[A, B](opt: Option[A], f: Fn[A] -> B) -> Option[B] =
  match opt {
    some(x) => some(f(x))
    none => none
  }
```

**Pros**:
- Self-hosted, dogfooding the language
- No template fragility
- Easy to read and understand

**Cons**:
- Bundled .almd modules go through the full compile pipeline for each use
- Generic type parameters (`[A, B]`) in bundled modules have not been battle-tested
- Pattern matching on `some`/`none` in bundled code requires the same codegen paths to work correctly
- Cannot use inline Rust `Option::map` / `Option::and_then` -- would emit match-based Rust code instead of idiomatic Rust

### Recommendation: TOML-defined module

**Use TOML**, same as `result`. The reasons are decisive:

1. **Rust codegen quality**: TOML templates emit `opt.map(|x| ...)` directly, which is idiomatic Rust. A bundled .almd would emit match-based code, which is correct but less efficient and less readable in the output.

2. **TS codegen control**: The TS backend erases `Option` to `T | null`. TOML templates can emit `v !== null ? f(v) : null` directly, which is the correct JavaScript pattern. A bundled .almd would need the TS emitter to correctly handle the pattern match erasure, which adds complexity.

3. **Consistency**: The `result` module is TOML-defined. Having `option` use a different mechanism would be confusing for contributors.

4. **Type signature generation**: TOML auto-generates `stdlib_sigs.rs`, which the type checker uses for function lookup. Bundled .almd modules rely on a different resolution path (parsed and checked at compile time), which is heavier.

---

## 6. Summary

### Function verdicts

| Function | Verdict | Notes |
|----------|---------|-------|
| `map` | MUST HAVE | Core transformation |
| `flat_map` | MUST HAVE | Chaining fallible operations |
| `flatten` | INCLUDE | Completeness, parallels list/result |
| `unwrap_or` | INCLUDE | Parallels builtin, module-qualified path |
| `unwrap_or_else` | INCLUDE | Lazy default computation |
| `is_some` | INCLUDE | Predicate for guards/filters |
| `is_none` | INCLUDE | Complement of is_some |
| `to_result` | MUST HAVE | Option-to-Result bridge |
| `to_list` | INCLUDE | Compositional utility |
| `filter` | RECOMMEND ADDING | Completes the filter/map/flat_map triad |

### Naming: all correct

Every proposed name follows the established Almide verb conventions and is consistent with the corresponding functions in `list` and `result` modules.

### Implementation: TOML-defined module

Same approach as `result.toml`. Direct Rust `Option` method calls in templates, null-check patterns for TS.

### Builtin interaction: no conflict

`some()`, `none` remain language-level constructs. The builtin `unwrap_or` coexists with `option.unwrap_or` through separate resolution paths. UFCS for unambiguous option verbs (`is_some`, `is_none`, `to_result`, `to_list`) should be added to `resolve_ufcs_candidates`.

### Final function count

If `filter` is added: **11 functions** (not 10).
If `filter` is deferred: **10 functions** as proposed (but verify the spec lists all 10 explicitly -- the current text only names 9 in the code block).
