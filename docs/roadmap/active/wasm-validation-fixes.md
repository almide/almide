# Type System Architecture [ACTIVE]

## Vision

Kind = KType | KArrow Kind Kind — 全ての判断が構造から自明になるコンパイラ。

## Current: Rust 153/153, WASM 12 compile failures, 21/73 pass

## Completed (this branch: fix/wasm-scratch-depth)

| Commit | Change | Effect |
|--------|--------|--------|
| b59aaeb..47d1602 | Scratch depth fix (ForIn, Call, Lambda, Try) | 4 files unblocked |
| 99d36dc | Closure env typed zero-init | scope_test pass |
| bba7476 | HashMap → Union-Find fixpoint | edge_cases, function_test pass |
| f7d2989 | Full Union-Find + lambda current_ret isolation | 153/153 Rust |
| 1ed3d2b | VarTable mono update | generics_test pass |
| 85dcd76 | Name-based VarId fallback in Var emit | default_fields partial |
| bd3a42e | BinOp VarTable fallback for Unknown types | default_fields pass |

**Total: 17→12 compile failures (5件改善)**

## Remaining: 12 compile failures

### Codec系 [8 files] — separate root cause
auto_derive, codec_convenience/list/nested/p0/test/weather, value_utils

Codec derive が生成する IR で Value variant の payload を i32 slot に store。

### Protocol/type_system [4 files] — TypeVar residue in monomorphized code
protocol_advanced/extreme/stress, type_system_test

Pattern: generic/protocol function の mono 後に TypeVar が残るか、
effect fn の Result unwrap が不完全で戻り値型が i32 (ptr) のまま。

## Next Steps

| Priority | What | Impact |
|----------|------|--------|
| 1 | Protocol/type_system の個別分析 | +4 files |
| 2 | Codec WASM support or skip | +8 files |
