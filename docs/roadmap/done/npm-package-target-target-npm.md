<!-- description: Compile Almide to publish-ready npm packages via --target npm -->
<!-- done: 2026-03-11 -->
# npm Package Target

Compile Almide code into a publish-ready npm package. Write libraries in Almide and distribute them to the JS ecosystem via `almide build --target npm`.

### Current Limitations

- `--target ts` / `--target js` inline ~300 lines of runtime at the top of the output
- Output is a single file (stdout) with no package structure
- Entry point code is included when `main()` exists
- Visibility (`Public`/`Mod`/`Local`) exists in the AST but is not reflected in exports

### Output Structure

```
dist/
  package.json        — name, version, type: "module", exports
  index.js            — ESM: import runtime + export public functions
  index.d.ts          — TypeScript type declarations
  _runtime/           — only stdlib modules actually used
    helpers.js         — __bigop, __div, __deep_eq, __concat, println
    list.js            — __almd_list
    string.js          — __almd_string
    ...
```

### Phase 1: Runtime Separation

Split monolithic RUNTIME/RUNTIME_JS into individual module files.

- [ ] Extract each `__almd_*` object in `emit_ts_runtime.rs` as an individually emittable string
- [ ] Separate helper functions (`__bigop`, `__div`, `__deep_eq`, `__concat`, `println`, etc.)
- [ ] Track which stdlib modules are used during codegen (compile-time tree-shaking)
- [ ] Emit each module as a standalone JS file with ESM `export`

### Phase 2: ESM Export Output

npm mode in `src/emit_ts/declarations.rs`:

- [ ] Skip entry point emission (`// ---- Entry Point ----`)
- [ ] `Visibility::Public` → `export function`, `Mod`/`Local` → no export
- [ ] `Decl::Type` (Public) → `export type` (d.ts)
- [ ] Import runtime via relative paths: `import { __almd_list } from "./_runtime/list.js";`
- [ ] Consider clean re-exports for sanitized names (`is_empty_hdlm_qm_` → `isEmpty` etc.)

### Phase 3: Package Scaffolding

- [ ] Output to a directory (`-o dist/` or default `dist/`)
- [ ] Generate `package.json`: read name/version from `almide.toml`, set `type: "module"`
- [ ] `index.js` — compiled user code (ESM import/export)
- [ ] `index.d.ts` — TypeScript type declarations for all exported functions
- [ ] `_runtime/*.js` — only modules actually used

### Phase 4: CLI Integration

- [ ] Add `"npm"` target to `src/cli.rs`
- [ ] `emit_ts::emit_npm_package()` — emit multiple files to a directory
- [ ] `-o <dir>` for directory output (file writes, not stdout)
- [ ] Existing `--target ts` / `--target js` remain unchanged (backwards compatible)

### Example

```almide
fn greet(name: String) -> String = "Hello, ${name}!"

fn fibonacci(n: Int) -> List[Int] = {
  list.take(
    list.fold(0..n, [0, 1], fn(acc, _i) => {
      let a = list.get_or(acc, list.len(acc) - 2, 0)
      let b = list.get_or(acc, list.len(acc) - 1, 0)
      acc ++ [a + b]
    }),
    n
  )
}
```

Output `dist/index.js`:
```javascript
import { __almd_list } from "./_runtime/list.js";
import { __concat } from "./_runtime/helpers.js";

export function greet(name) {
  return `Hello, ${name}!`;
}

export function fibonacci(n) {
  return __almd_list.take(/* ... */);
}
```

Output `dist/index.d.ts`:
```typescript
export declare function greet(name: string): string;
export declare function fibonacci(n: number): number[];
```

### Constraints

- Node.js 22+ (LTS) / modern bundlers (Vite, esbuild, Rollup)
- `__almd_` prefix is internal only — never exposed in the public API
- `_runtime/` is internal (conventional `_` prefix marks it as private)
- Int is i64 — BigInt handling (`__bigop`, `__div`) must be considered

---
