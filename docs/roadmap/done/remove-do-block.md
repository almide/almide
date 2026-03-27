<!-- description: Complete removal of do block from the language -->
<!-- done: 2026-03-24 -->
# Remove `do` Block

**Completed**: 2026-03-24

Completely removed `do` blocks from the language.

## Changes Made

### Phase 1: .almd file migration (66 locations)
- `effect fn ... = do { }` -> `= { }` (16 locations)
- `do { guard COND else break }` -> `while COND { }` (35 locations)
- `do { guard ... else ok(val)/err(val) }` -> `while` + tail expression (15 locations)

### Phase 2: Compiler removal
- Removed `Expr::DoBlock` from AST
- Removed `IrExprKind::DoBlock` from IR
- Parser: reject `do` keyword + migration hint
- 46 files changed, -349 lines

### Phase 3: Documentation + Rename
- `do_guard_test.almd` -> `guard_test.almd`
- `do_block_pure_test.almd` -> `while_loop_test.almd`
- `codegen_do_block_test.almd` -> `codegen_loop_guard_test.almd`

### Bug Fix
- Discovered and fixed a bug where StreamFusion's `inline_single_use_collection_lets` was not substituting variable references inside Guard stmts

## Results

- Loop syntax: unified to two forms: `for` + `while`
- `guard` statements remain valid inside `while` / `for`
- `try` blocks not introduced (`effect fn` auto-? is sufficient)
- 159/159 tests passing
