<!-- description: Fix import self.sub.module resolution for nested submodules -->
<!-- done: 2026-04-07 -->
# Import Self Submodule Resolution

`import self.wasm.binary` fails to resolve when building from `src/mod.almd` even though `src/wasm/binary.almd` exists and `almide.toml` is present.

## Current behavior

```
almide build src/mod.almd -o porta
→ error[E002]: undefined function 'binary.load'
```

The compiler finds `almide.toml` and parses it, but does not scan `src/wasm/` for submodules.

## Expected behavior

Per the module system spec, `src/wasm/binary.almd` should be available as `porta.wasm.binary` (or `self.wasm.binary` from within the package). All of:

```almide
import self.wasm.binary          // → binary.load()
import self.wasm.binary as wb    // → wb.load()
```

should resolve correctly.

## Reproduction

```
porta/
  almide.toml          # [package] name = "porta"
  src/
    mod.almd           # import self.wasm.binary
    wasm/
      binary.almd      # effect fn load(...) -> ...
```

## Impact

Blocks porta from using multi-file project structure. Currently forced to use single-file or inline all code.
