<!-- description: Embed EffectSet into FnType for type-level effect tracking -->
# Effect Type Integration — Embed EffectSet in FnType

**優先度:** 2.x
**前提:** Effect System Phase 1-2 完了、HKT Foundation Phase 1-4 完了
**構文制約:** ユーザー構文に一切にじませない。`effect fn` が唯一のマーカー。

## 概要

現在 Effect 推論結果は `IrProgram.effect_map` (HashMap) に格納されている。
これを型レベルに昇格し、`FnType` の一部として EffectSet を持たせる。

## 現状

```rust
// IrProgram に外付け
pub effect_map: EffectMap,  // HashMap<String, FunctionEffects>
```

## 目標

```rust
// FnType の一部として
struct FnType {
    params: Vec<Ty>,
    ret: Ty,
    effects: EffectSet,  // {IO, Net, ...} — コンパイラが自動推論
}
```

## メリット

- 型チェッカーが effect を型の一部として制約に使える
- 「この関数は {Net} しか使わない」を型レベルで保証
- Trait system と統合した際に effect-polymorphic な抽象が可能 (内部的に)
- almide check --effects の精度が型推論と連動

## 構文への影響

**なし。** ユーザーは引き続き `fn` / `effect fn` だけ書く。
Effect の粒度 (IO/Net/Env 等) は型システムの内部情報。
`effect fn[IO]` のような構文は **導入しない**。
