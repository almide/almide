# almide-lang

Facade crate. Re-exports `almide-syntax` (AST, lexer, parser) and `almide-types` (Ty, stdlib_info) as a single dependency.

## Rules

- **No logic here.** This crate only re-exports. If you need shared logic, put it in the appropriate leaf crate.
- **Downstream crates depend on this** instead of separately depending on both almide-syntax and almide-types.
