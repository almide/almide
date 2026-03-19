# HKT Foundation — Internal Type Constructor Infrastructure

**優先度:** 1.x (generic fn と同時進行)
**前提:** Generics Phase 1 完了済み
**原則:** ユーザーに HKT syntax は見せない。コンパイラ内部の表現力を上げる。

> 「ユーザーにはシンプル、コンパイラは賢い」

---

## 完了 (Phase 1-3) → [done/hkt-foundation-phase1.md](../done/hkt-foundation-phase1.md)

- [x] **Phase 1: Ty ヘルパー + TypeConstructor 基盤** — children(), map_children(), map_children_mut(), TypeConstructorRegistry, IrProgram 統合, 14 関数簡素化
- [x] **Phase 2: 代数法則テーブル** — 6 法則 (FunctorComposition, FilterComposition, MapFoldFusion, etc.)
- [x] **Phase 3: Stream Fusion Nanopass** — map+map, filter+filter, map+fold の 3 融合を実装。ネスト呼び出し + let-binding チェーン検出

---

## 残り

### Phase 4: Ty 統一リファクタ (1.x)

Ty enum の variant を `Applied(TypeConstructorId, Vec<Ty>)` に統一。Phase 1-3 のヘルパー群が下地。

```rust
// Before: ハードコード
Ty::List(Box<Ty>)
Ty::Option(Box<Ty>)
Ty::Result(Box<Ty>, Box<Ty>)
Ty::Map(Box<Ty>, Box<Ty>)

// After: 統一表現
Ty::Applied(TypeConstructorId::List, vec![inner])
Ty::Applied(TypeConstructorId::Option, vec![inner])
Ty::Applied(TypeConstructorId::Result, vec![ok, err])
Ty::Applied(TypeConstructorId::Map, vec![key, val])
```

影響: 30+ ファイル、559 箇所 (ただしヘルパー使用済み箇所は影響軽微)

### Phase 5: Effect 型統合 (2.x)

Effect set を型レベル表現に昇格。HKT Foundation + Effect System が合流。

### Phase 6: Trait 統合 (2.x)

内部 HKT 表現上に Trait を構築。ユーザー定義型の自動 Mappable 等。

---

## Stream Fusion 拡張 (将来)

| 法則 | 状態 |
|------|------|
| FunctorComposition (map+map) | ✅ 実装済み |
| FilterComposition (filter+filter) | ✅ 実装済み |
| MapFoldFusion (map+fold) | ✅ 実装済み |
| MapFilterFusion (map+filter → filter_map) | 未実装 |
| FunctorIdentity (map(id) 消去) | 未実装 |
| MonadAssociativity (flat_map+flat_map) | 未実装 |
| Let-binding chain rewrite | 検出のみ (書き換え未実装) |
