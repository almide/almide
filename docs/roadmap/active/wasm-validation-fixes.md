# Type System Architecture [ACTIVE]

## Vision

Kind = KType | KArrow Kind Kind — 全ての判断が構造から自明になるコンパイラ。

## Status: Rust 153/153, WASM 12 compile failures, 21/73 pass

## Landed (fix/wasm-scratch-depth branch)

| Change | Effect |
|--------|--------|
| Scratch depth fix (ForIn, Call, Lambda, Try) | 4 files unblocked |
| Closure env typed zero-init | scope_test pass |
| **Union-Find 型推論** (HashMap → 等価クラスモデル) | TypeVar leak 構造的消滅 |
| lambda current_ret isolation | ok/err 型漏洩バグ修正 |
| VarTable mono update | generics_test pass |
| Name-based VarId fallback + BinOp VarTable fallback | default_fields pass |

**Total: 17→12 compile failures, Rust 153/153 維持**

## Next: Protocol/type_system [4 files]

protocol_advanced/extreme/stress, type_system_test

Generic/protocol 関数の mono 後に TypeVar 残留、または effect fn の Result unwrap が不完全。
個別分析が必要。**次の branch で対応。**

## Next: Codec WASM skip [8 files]

auto_derive, codec_convenience/list/nested/p0/test/weather, value_utils

Codec は JSON serialization 機能。WASM target では不要。
テストに WASM skip マーカーを追加して compile failure から除外する。

## Future: Kind-Aware Type Representation

```
Kind = KType | KArrow Kind Kind
```

- 全型定義に Kind 付与
- Kind inference / Kind checker
- HKT の自然な拡張基盤
