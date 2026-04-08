<!-- description: Fix WASM fs.list_dir memory corruption when building List[String] result -->
<!-- done: 2026-04-08 -->
# Fix WASM fs.list_dir Memory Corruption

## Symptom

`fs.list_dir(".")` in WASM returns a `List[String]` where the string entries have corrupted byte-length fields. When serialized via `json.stringify`, the corrupted lengths cause raw heap memory to leak into the output.

Correct file names are visible in the corrupted output (e.g., "tests", "almide.toml", "README.md"), but interleaved with binary garbage from adjacent heap memory.

## Root Cause (hypothesis)

The WASM runtime function for `fd_readdir` builds a `List[String]` on the heap. The string entries are constructed by:
1. Parsing WASI `fd_readdir` buffer (name_len + name bytes per entry)
2. Allocating `[len:i32][data...]` strings on the heap
3. Building a `[count:i32][ptr0][ptr1]...` list

The corruption pattern suggests the string `len` field is being set to a value larger than the actual data, or the list element pointers are misaligned.

## Impact

- `fs.list_dir` produces garbage in WASM target
- `fs.write` may also be affected (not yet confirmed)
- `fs.read_text` works correctly
- Native (Rust) target is unaffected

## Reproduction

```almide
import fs
import json

effect fn main() -> Result[Unit, String] = {
  let files = fs.list_dir(".")!
  println(json.stringify(json.array(files |> list.map(json.from_string))))
  ok(())
}
```

Compile with `almide build --target wasm`, run with `wasmtime run --dir=.`.
