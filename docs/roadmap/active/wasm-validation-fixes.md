# Type System Architecture [ACTIVE]

## Vision

Almide の型システムを Kind 体系に基づいた一貫した世界観で構築する。
全ての判断が構造から自明になるコンパイラ。

```
Kind = KType | KArrow Kind Kind
```

## Current: Rust 153/153, WASM 14 compile failures (21/73 pass)

## Completed

### Union-Find 型推論 (commit f7d2989)
- `HashMap<TyVarId, Ty>` → `UnionFind` (等価クラスモデル)
- propagation hack / fixpoint iteration 不要に
- lambda `current_ret` isolation (潜在バグ修正)
- edge_cases_test, function_test, scope_test の validation pass

### Closure env typed zero-init (commit 99d36dc)
- `emit_lambda_closure` で capture 型に応じた zero 値

---

## 残り14件の根本原因（確定）

### Root Cause: Generic 関数の monomorphization が不完全

**現状**: `mono.rs` は structural bounds (`T: { name: String, .. }`) のみ specialize。
普通の generic パラメータ (`[T]`) は monomorphize されない。

- Rust codegen: Rust の generics がそのまま処理 → 問題なし
- WASM codegen: 全ての型が concrete でなければならない → TypeVar が残って型不一致

**具体例**:
```almide
fn unwrap_or[T](w: Wrapper[T], default: T) -> T = match w {
  Wrapped(v) => v,   // v の型が T(TypeVar) のまま → i32.load で読む
  Empty => default,
}
unwrap_or(Wrapped(99), 0)  // T=Int → v は i64.load で読むべき
```

**影響する14件**:
- generics_test, protocol_advanced/extreme/stress, type_system: generic 関数の TypeVar
- default_fields_test: variant constructor の Float field
- codec系 8件: Codec 生成コードの Value variant 構築

### Fix: Full monomorphization for WASM target

`mono.rs` を拡張して、**全ての generic 関数** を call-site の具体型で specialize。

```
Before: mono.rs only handles structural bounds
After:  mono.rs handles ALL generic type parameters
```

#### 実装ステップ

1. generic 関数の検出を拡張
   - `find_structurally_bounded_fns` → `find_generic_fns`
   - TypeVar を持つ全関数を対象にする

2. call-site から具体型を収集
   - `discover_instances` を拡張
   - `unwrap_or(Wrapped(99), 0)` → T=Int を検出

3. specialize で TypeVar を concrete に置換
   - `specialize_function` で TypeVar → 具体型を IR 全体に適用

4. WASM target でのみ full mono を有効化
   - Rust codegen は Rust generics に任せる（変更なし）
   - `--target wasm` のときだけ full monomorphization

#### 検証
- Rust 153/153 変わらず
- WASM compile failures: 14 → 7前後（generic 系が解消）
- Codec 系は別の根本原因がある可能性

---

## 後続 Phase

### Kind-Aware Type Representation
- `Kind = KType | KArrow Kind Kind`
- 全型定義に Kind 付与、Kind checker

### Codec WASM
- Codec 生成コードの WASM 対応
