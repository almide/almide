<!-- description: almide test --target ts command with Deno/Node support -->
<!-- done: 2026-03-25 -->
# TypeScript Test Runner

**Completed:** 2026-03-25

## Implementation

Added the `almide test --target ts` command. TypeScript target test execution is now possible.

### Changes
- Added `cmd_test_ts()` to `src/cli/commands.rs` (229 lines)
- Added `--target ts`/`--target typescript` dispatch to `src/main.rs`
- Deno preferred, Node.js fallback
- `// ts:skip` marker for per-file skip support
- Codegen panics reported as SKIP
- Type errors displayed with source context
