<!-- description: Seven compiler bugs discovered during test coverage expansion -->
<!-- done: 2026-03-12 -->
# Compiler Bugs Found by Test Expansion

7 compiler bugs discovered through test coverage expansion (806→1501). Currently worked around in tests; fix the compiler and restore tests to their intended form.

## Bugs

### 1. ~~`float.abs()` が Rust で free function `abs(x)` を生成~~

- **Actual**: `almide_rt_float_abs()` exists in the runtime and works correctly. Test description error.
- **Status**: [x] NOT A BUG

### 2. top-level let + String → `const` 生成で `to_string()` 呼べない

- **Expected**: Initialize with `lazy_static` or `static` or `let`
- **Actual**: `const NAME: String = "hello".to_string()` → `E0015: cannot call non-const method in constants`
- **Location**: `src/emit_rust/program.rs` TopLet codegen
- **Found by**: lang/top_let_test.almd
- **Fix**: String/non-const expressions → change to `static LazyLock<T>`, use `(*name).clone()` when referencing variables
- **Status**: [x] DONE

### 3. top-level let + float演算 → 型不一致

- **Expected**: `const TRIPLE_PI: f64 = PI * 3.0`
- **Actual**: `const TRIPLE_PI: i64 = (PI * 3.0f64)` → `E0308: mismatched types`
- **Location**: `src/emit_rust/program.rs` TopLet codegen + added `ir_expr_contains_float` helper
- **Found by**: lang/top_let_test.almd
- **Fix**: Infer f64 when IR expression contains float literals
- **Status**: [x] DONE

### 4. generic variant の型推論ヒント不足

- **Expected**: Generate type annotation like `let e1: Either<String, i64> = Right(5)`
- **Actual**: `let e1 = Right(5i64)` → `E0283: type annotations needed for Either<_, i64>`
- **Location**: `src/types.rs` add type args to Ty::Named, `src/emit_rust/ir_blocks.rs` handle Named with type args in ir_ty_annotation
- **Fix**: Extend Ty::Named(String) → Ty::Named(String, Vec<Ty>). Save type args in lower.rs/check/, generate type annotations like `Either<String, i64>` in codegen
- **Found by**: lang/type_system_test.almd
- **Status**: [x] DONE

### 5. generic container の borrow 推論不足

- **Expected**: `c` is cloned or borrowed in `container_add(c, 1)`
- **Actual**: `c` is moved and subsequent `assert_eq!(c.label)` gives `E0382: borrow of moved value`
- **Location**: `src/emit_rust/` borrow analysis (generic record type parameter inference)
- **Found by**: lang/type_system_test.almd
- **Fix**: Borrow analysis had already been improved to insert automatic clones. Also resolved by type information improvements from Bug #4/7 fix
- **Status**: [x] DONE

### 6. `map.from_list` クロージャ内の borrow 推論不足

- **Expected**: Closure argument `w` is cloned
- **Actual**: `|w| { (w, string.len(&*w)) }` → `w` borrowed after move → `E0382`
- **Location**: `src/emit_rust/` borrow analysis (closure capture), `stdlib/defs/map.toml`
- **Fix**: (1) Add `{f.clone_bindings}` to map.from_list TOML template, (2) Apply same fix to all closure functions in list.toml, (3) Generate .clone() at the first move position when the same variable is used multiple times in a Tuple expression
- **Found by**: lang/edge_cases_test.almd
- **Status**: [x] DONE

### 7. named record 型に構造体リテラルが代入できない

- **Expected**: `{ items: [], label: "x" }` can be assigned to `type Container = { items: List[T], label: String }`
- **Actual**: `cannot assign { items: List[Int], label: String } to Container`
- **Location**: `src/check/statements.rs` type checking, `src/types.rs` resolve_named
- **Fix**: Extend resolve_named to support resolution of Named types with type arguments. Resolve Named → struct in let/var type checking before compatibility check
- **Found by**: lang/type_system_test.almd
- **Status**: [x] DONE

## Priority

**P0 (codegen correctness)**: #1, #2, #3 — correct Almide code doesn't compile
**P1 (generics usability)**: #4, #7 — basic usage of generic types is broken
**P2 (borrow refinement)**: #5, #6 — borrow inference accuracy improvement

## Fix → Test Restore Flow

After fixing each bug:
1. Fix the compiler
2. Restore tests from "workaround version" to "intended form"
3. Verify all pass with `cargo test` + `almide test`
4. Update Status to `[x] DONE` in this file

## Fix log

| # | Bug | Fixed | Test Restored | Date |
|---|-----|-------|---------------|------|
| 1 | float.abs codegen | N/A (not a bug) | N/A | 2026-03-12 |
| 2 | top-level let String | done | done | 2026-03-12 |
| 3 | top-level let float type | done | done | 2026-03-12 |
| 4 | generic variant type annotation | done | done | 2026-03-12 |
| 5 | generic container borrow | done | done | 2026-03-12 |
| 6 | map.from_list borrow | done | done | 2026-03-12 |
| 7 | named record compatibility | done | done | 2026-03-12 |
