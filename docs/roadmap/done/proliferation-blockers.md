<!-- description: Compiler bugs that blocked LLM module generation (all resolved) -->
# Proliferation Blockers [DONE]

Issues discovered during the first `almide proliferate` run (csv module). These were compiler-level problems that reduced LLM success rate and required workarounds in generated code.

## 1. `string.contains?` codegen missing (Rust target) — RESOLVED

**Status:** Fixed. All `?`-suffix functions (`contains?`, `starts_with?`, `ends_with?`, `is_empty?`, `is_dir?`, `is_file?`) are handled in `emit_rust/calls.rs` with both `?` and `_hdlm_qm_` variants.

## 2. Ownership / move errors when variable is reused after passing to function — RESOLVED

**Status:** Fixed. `gen_arg()` automatically clones every `Ident` argument passed to functions. For-in loops also clone the iterable. Stdlib runtime functions take references (`&`), avoiding moves entirely.

Verified working:
- Variable reuse after user function call
- Variable reuse after stdlib call (list.map, list.filter, etc.)
- Variable reuse after for-in loop
- Multiple calls with same variable

## 3. Record literals in `list.fold` don't compile (Rust target) — RESOLVED

**Status:** Fixed. Record literals inside `list.fold` lambdas now compile and run correctly. The Rust codegen handles anonymous record construction in expression position.

Verified working:
```almide
let result = list.fold(items, { sum: 0, count: 0 }, fn(acc, x) => {
  { sum: acc.sum + x, count: acc.count + 1 }
})
```

## Summary

All three original blockers have been resolved. No known proliferation blockers remain as of v0.4.3.
