# Direct WASM Emission [ACTIVE]

## Status: 129/129 tests passing (100%), WASI imports operational

## Architecture

```
src/codegen/emit_wasm/
├── mod.rs              WasmEmitter, FuncCompiler, DepthGuard, assembly (1200+ lines)
├── wasm_macro.rs       wasm! DSL macro for instruction emission
├── values.rs           Ty → ValType mapping, byte_size, field_offset
├── strings.rs          String literal interning
├── scratch.rs          ScratchAllocator (bump/reuse locals)
├── runtime.rs          Runtime function registration + WASI imports
├── rt_string.rs        String runtime functions (split, join, replace, etc.)
├── rt_value.rs         Value stringify + JSON parse runtime
├── rt_regex.rs         Backtracking regex engine (~1400 lines)
├── expressions.rs      emit_expr + operators + Option/Result
├── calls.rs            Call dispatch + datetime/random/fan/env/http
├── calls_value.rs      Value/JSON/Codec helpers (pick, omit, merge, encode/decode)
├── calls_regex.rs      Regex module dispatch
├── calls_lambda.rs     Lambda closure emission (lambda_id matching)
├── calls_option.rs     Option module dispatch
├── calls_list.rs       List module dispatch
├── calls_list_helpers.rs  List helper operations
├── calls_list_closure.rs  List higher-order (map, filter, fold, etc.)
├── calls_list_closure2.rs Additional list closures (take_while, partition, etc.)
├── calls_map.rs        Map module dispatch
├── calls_set.rs        Set module dispatch
├── calls_numeric.rs    Int/Float module dispatch
├── collections.rs      Record/Tuple/List/Map construction
├── control.rs          Match/pattern matching/do-block/for-in
├── equality.rs         Deep equality (Option, Result, List, Variant)
├── statements.rs       Statement emission + local scan
├── closures.rs         Lambda/closure pre-scan and compilation
└── functions.rs        IrFunction → WASM function
```

## Fully Implemented

- All language features (expressions, control flow, closures, generics, variants, records, tuples)
- All stdlib modules: string, list, map, set, int, float, math, option, result, value, json, regex, datetime, random, fan, env, http
- Codec auto-derive (encode/decode for records and variants, Option/List/default fields)
- Deep equality for all types including variants
- WASI imports: fd_write, clock_time_get, proc_exit, random_get
- ScratchAllocator + DepthGuard RAII
- Lambda identification via lambda_id

## WASI Imports

| Import | Status | Usage |
|--------|--------|-------|
| `fd_write` | Active | stdout (println) |
| `clock_time_get` | Active | `datetime.now()` — realtime clock |
| `random_get` | Active | PRNG seed initialization |
| `proc_exit` | Imported | Available for future exit(code) |

## Next Steps

1. **StreamFusion対応** — 現在WASM pipelineから除外。emitterがfused IR（map+filter融合等）を処理できるようにすればパフォーマンス向上
2. **fd_read** — stdin for interactive programs
3. **args_get / environ_get** — CLI args and env vars
4. **File I/O** — path_open/fd_read/fd_write/fd_close/fd_seek

## Done

- [x] 129/129 tests passing (100%)
- [x] All stdlib modules implemented
- [x] WASI imports (fd_write, clock_time_get, random_get, proc_exit)
- [x] Codec auto-derive
- [x] Dead Code Elimination (DCE)

## Binary Size (DCE後)

| Program | Binary Size |
|---------|------------:|
| Hello World | 1,028 B |
| FizzBuzz | 1,286 B |
| Fibonacci | 1,361 B |
| Closure | 1,443 B |
| Variant | 1,777 B |

アロケータ・文字列処理・ランタイム全部入りの自己完結バイナリ。
