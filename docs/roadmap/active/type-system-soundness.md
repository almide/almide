# Type System Soundness [ACTIVE]

## Summary
型システムの健全性を B+ → A に引き上げる。14個の具体的弱点のうち Critical 3 + High 4 を修正。

## Goal
- Unknown の伝播を最小化
- unification が全型コンストラクタを正しく処理
- occurs check が無限型を完全に防止
- TypeVar が解決不能時に情報を失わない

## Critical (型システムが壊れる)

### C-1: DoBlock が常に Unknown を返す
- **場所**: `src/check/expressions.rs:258-267`
- **問題**: DoBlock の最終式の型 `_ty` を計算するが捨てて `Ty::Unknown` を返す
- **修正**: `_ty` をそのまま返す。guard 付き do block は `Ty::Unit`

### C-2: Record 同士の unify_infer がない
- **場所**: `src/check/mod.rs:144-168`
- **問題**: `unify_infer()` に Record/OpenRecord のケースがない。`_ => false` にフォールスルー
- **修正**: Record のフィールド同士を再帰的に unify するケース追加

### C-3: Unknown が全てと unify 成功する
- **場所**: `src/types.rs:296-299`
- **問題**: 片方が Unknown なら無条件で `true`。推論失敗がマスクされる
- **修正**: Unknown 同士 → true。片方 Unknown → binding を試行して情報伝播

## High (型推論の精度に直結)

### H-1: occurs check が浅い (Tuple, Record, Fn 未チェック)
- **場所**: `src/types.rs:282-291`
- **問題**: `occurs_in()` が List, Option, Result, Map のみ。`T = (T, Int)` や `T = { x: T }` が通る
- **修正**: Tuple, Record, OpenRecord, Fn の再帰チェック追加

### H-2: 未解決 TypeVar が Unknown に落ちる
- **場所**: `src/types.rs:359`, `src/check/types.rs:44`
- **問題**: unbound TypeVar → `Ty::Unknown`。「未解決」と「エラー」の区別がつかない
- **修正**: `Ty::TypeVar(name)` のまま保持。codegen で必要なら codegen 側で fallback

### H-3: TypeVar binding 時に structural bounds 未検証
- **場所**: `src/types.rs:302-312`
- **問題**: `[T: { name: String }]` の T に `Int` を bind しても通る
- **修正**: binding 前に bound の compatible チェック

### H-4: Union unification が非決定的
- **場所**: `src/types.rs:347-348`
- **問題**: `.any()` で最初に成功した member の binding を採用。他の member で binding が異なる場合に非決定的
- **修正**: binding のスナップショット → 成功時に commit、失敗時に rollback

## Medium (edge case, 今回はスコープ外)

### M-1: Fn type の Result auto-unwrap が ad-hoc
- `types.rs:325-337`

### M-2: effect fn の return type 判定が body 依存
- `check/mod.rs:250-261`

### M-3: Unknown の refinement が不十分 (Record, Union)
- `check/expressions.rs`

## Files
```
src/types.rs          — unify, occurs_in, substitute 修正
src/check/mod.rs      — unify_infer に Record ケース追加
src/check/expressions.rs — DoBlock 戻り値修正
src/check/types.rs    — InferTy::to_ty の Unknown fallback 修正
```

## Verification
- `cargo test` — 既存テスト全パス
- `almide test spec/lang/` — spec テスト全パス
- 新規テスト: `spec/lang/type_soundness_test.almd`
  - DoBlock の型推論
  - Record unification
  - occurs check (infinite type rejection)
  - Union type narrowing
