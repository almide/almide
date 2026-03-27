<!-- description: Warning infrastructure for code quality issues like unused variables -->
# Compiler Warnings [ACTIVE]

Infrastructure for emitting warnings (distinct from errors) for code quality issues. Currently the compiler has errors and hints but no formal warning system.

## 1. Unused Variable Warning

**Problem:** Variables bound with `let` or `var` but never referenced are silent. The infrastructure exists — `VarTable.use_count` tracks reference counts — but no warning is emitted.

**Implementation:**
- After `compute_use_counts()` in `ir.rs`, scan `VarTable` for entries with `use_count == 0`
- Exclude: `_` prefixed names (conventional "intentionally unused"), function parameters, loop variables used for side effects
- Emit: `warning: unused variable 'x' (declared at line N)`
- Hint: `prefix with _ to suppress: let _x = ...`

**Difficulty:** Low. Data already computed.

## 2. Unused Import Warning

**Problem:** Imported modules that are never referenced in the program waste compile time and clutter code.

**Implementation:**
- Track which imported modules are referenced in `CallTarget::Module` during lowering
- After lowering, compare against import list
- Emit: `warning: unused import 'http'`

## 3. Dead Code Warning

**Problem:** Code after `return`, `break`, `continue`, or unconditional `err()` is unreachable but silently compiled.

**Implementation:**
- In the checker or lowering, mark statements after early-exit expressions as unreachable
- Emit: `warning: unreachable code after 'return' at line N`

**Note:** This is distinct from dead code *elimination* (codegen-refinement.md item 5). Warnings inform the user; DCE removes code from output.

## 4. Trait Bounds Not Enforced Warning

**Problem:** `GenericParam.bounds` exists in the AST (parsed from `T: { name: String, .. }` syntax) but the checker ignores it. Users may write bounds expecting them to be enforced.

**Implementation:**
- In `src/check/mod.rs`, when processing generic function declarations, check if bounds are present
- If bounds exist but enforcement is not yet implemented, emit: `warning: generic bound 'T: { name: String, .. }' is parsed but not yet enforced`
- Remove warning once type-system.md Phase 3 (generic bounds) is implemented

**This is a temporary measure** until full trait bounds enforcement lands.

## 5. Shadowed Variable Warning

**Problem:** A `let` in an inner scope with the same name as an outer `let` silently shadows it. With VarId this doesn't cause bugs, but it confuses LLM-generated code.

**Implementation:**
- In `lower.rs` `define_var()`, check if the name already exists in an outer scope
- Emit: `warning: variable 'x' shadows a previous binding at line N`

## Warning Infrastructure

Currently `diagnostic.rs` only has error-level diagnostics. Need to add:

```rust
pub enum DiagnosticLevel {
    Error,
    Warning,
}
```

- Warnings should not prevent compilation (exit code 0 if only warnings)
- `--deny-warnings` flag to treat warnings as errors (useful for CI)
- Warnings use the same source display format as errors (file:line, caret, hint)

## Priority

| Warning | Data available? | Difficulty | Priority |
|---------|----------------|------------|----------|
| Unused variable | Yes (checker + use_count) | Low | P0 ✅ |
| Dead code | Partial | Medium | P1 |
| Unused import | Done (checker) | Low | P1 ✅ |
| Trait bounds | AST data exists | Low | P1 |
| Shadowed variable | Scope info exists | Low | P2 |
| Warning infra | ~~Needs DiagnosticLevel~~ Done | Low | P0 (prerequisite) ✅ |
