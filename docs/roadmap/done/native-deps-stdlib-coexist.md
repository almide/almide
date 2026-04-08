<!-- description: Fix native-deps and stdlib HTTP coexistence in generated Cargo.toml -->
<!-- done: 2026-04-08 -->
# Native Deps + Stdlib HTTP Coexistence

## Problem

When a project uses both `[native-deps]` and `import http` (stdlib), the build failed with:
- `error[E0432]: unresolved import 'bridge'` — native modules not found
- Native deps appended after `[profile]` section in generated Cargo.toml

## Root Causes

1. **`almide.toml` lookup was CWD-relative**: `almide build /path/to/main.almd` only found `almide.toml` in CWD, not in the input file's directory. Native deps were silently empty.
2. **`source_root` was hardcoded to `.`**: Native `*.rs` files were searched in CWD instead of the source directory.
3. **Cargo.toml dep injection position**: Native deps were appended to end of file instead of inserted into the `[dependencies]` section.

## Fix

- `almide.toml` is now searched in the input file's directory first, then CWD
- `source_root` derives from the input file's parent directory
- Native deps are inserted directly after the `[dependencies]` header line
- Both `build.rs` and `run.rs` paths fixed
