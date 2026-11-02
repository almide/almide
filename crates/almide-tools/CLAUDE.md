# almide-tools

Developer-facing tools: formatter, module interface extraction, and `.almdi` artifacts.

## Formatter (`fmt.rs`)

- `format_program(program) -> String` — AST → formatted source.
- Preserves comments, infers short/long formatting by expression complexity.
- `auto_imports()` — Add missing stdlib imports, remove unused.

## Module Interface (`interface.rs`)

- `extract_module_interface(ir_program) -> ModuleInterface` — Produces JSON API description.
- Includes type signatures, docs, deprecation flags, ABI layout for C FFI.
- Used by export tooling to generate pip/npm/gem packages.

## Module Artifacts (`almdi.rs`)

- `.almdi` serialization: public interface plus type-checked IR.
- The language server is implemented by the root CLI in `src/cli/lsp*.rs`.

## Rules

- **Formatter must be idempotent.** `format(format(x)) == format(x)` always.
- **Interface JSON is the contract.** External tools parse this — breaking changes require version bumps.
- **ABI layout must match codegen.** Field offsets in `AbiLayout` must agree with what `almide-codegen` actually emits. If codegen changes record layout, update interface extraction.
