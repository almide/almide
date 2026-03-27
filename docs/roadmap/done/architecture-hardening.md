<!-- description: Fix structural weaknesses in compiler architecture -->
<!-- done: 2026-03-15 -->
# Architecture Hardening

Fix structural weaknesses in the compiler. Preemptively remove landmines that will inevitably be triggered as the language grows and new features are added.

## P1: Remove IrProgram clone

**Problem:** `emit_with_options()` deep-copies IrProgram and all module IRs.

```rust
// emit_rust/mod.rs:145-146
emitter.ir_program = Some(ir.clone());
emitter.module_irs = module_irs.clone();
```

Megabytes of wasted allocations in large projects.

**Fix:**
- [ ] Add lifetime parameter to `Emitter`: `Emitter<'a>`
- [ ] Change to `ir_program: Option<&'a IrProgram>`, `module_irs: &'a HashMap<String, IrProgram>`
- [ ] Verify lifetimes are sufficient at `emit_with_options` call sites

**Affected files:** emit_rust/mod.rs, program.rs, ir_expressions.rs, ir_blocks.rs

## P1: Emitter state management refactor

**Problem:** 25+ fields, interior mutability via RefCell/Cell, state flags (`in_effect`, `in_do_block`, `skip_auto_q`) managed independently create consistency risks.

```rust
pub(crate) in_do_block: std::cell::Cell<bool>,
pub(crate) skip_auto_q: std::cell::Cell<bool>,
pub(crate) anon_record_structs: std::cell::RefCell<HashMap<...>>,
```

**Fix:**
- [ ] Separate context state into `CodegenContext` struct (`in_effect`, `in_do_block`, `skip_auto_q`, `in_test`)
- [ ] Move `anon_record_structs` and `anon_record_counter` to a pre-collection pass (remove mutation during codegen)
- [ ] RefCell → pre-computed tables, Cell → explicit stack management

## P1: Fixpoint iteration convergence guarantee ✅

**Fixed.** Upper bound changed to `max(fn_count, 20)`. Outputs warning when convergence fails.

## P1: Module circular reference detection ✅

**Already implemented.** `resolve.rs` has cycle detection via `loading: HashSet<String>`. Emits `circular import detected: ...` error.

**Fix:**
- [ ] Build import graph in `resolve.rs` and verify it is a DAG
- [ ] Error on cycle detection: `"circular import: A → B → A"`
- [ ] Test: add circular import test cases

## P2: build.rs template validation

**Problem:** Unknown placeholders in stdlib TOML `rust:` templates are silently output as literals. Breaks generated code.

```rust
// build.rs — {unknown_param} is output as-is into Rust code
```

**Fix:**
- [ ] Verify all `{placeholder}` are known parameters during template scan
- [ ] Build error on unknown placeholders
- [ ] Fix closure type arity analysis to handle nested brackets

## P2: scope push/pop balance validation

**Problem:** `LowerCtx` scope stack may break due to push/pop imbalance. Cases where pop is not called on error paths.

```rust
fn pop_scope(&mut self) { self.scopes.pop(); }  // panics if empty
```

**Fix:**
- [ ] Validate push/pop balance with debug_assert
- [ ] RAII guard pattern: `let _guard = ctx.push_scope()` for automatic pop on Drop
- [ ] Change empty stack pop to graceful error

## P2: Parser and precedence.toml consistency

**Problem:** `grammar/precedence.toml` is only used for documentation; actual parser precedence is hardcoded. May diverge.

**Fix:**
- [ ] Add cargo test to verify precedence.toml matches parser precedence
- [ ] Future: generate parser code from precedence.toml

## P2: unsafe indexing safety

**Problem:** `--fast` mode uses `get_unchecked` without index bounds validation. `as usize` converts negative numbers to huge positive numbers.

```rust
format!("unsafe {{ *{}.get_unchecked({} as usize) }}", obj, idx)
```

**Fix:**
- [ ] Insert `debug_assert!(idx >= 0 && (idx as usize) < {}.len())` before `unsafe` block during codegen
- [ ] Or: add negative number check before `as usize`

## P2: Parser recursion depth limit ✅

**Fixed.** Added `depth: usize` field to `Parser`. `enter_depth()` at the entry of `parse_expr` and `parse_type_expr` → error when exceeding `MAX_DEPTH(500)`.

## P3: VarId u32 overflow ✅

**Fixed.** Added `debug_assert!(self.entries.len() < u32::MAX as usize)`.

**Fix:**
- [ ] Add `assert!(self.entries.len() < u32::MAX as usize, "too many variables")`
