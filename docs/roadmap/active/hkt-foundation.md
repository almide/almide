# HKT Foundation — Internal Type Constructor Infrastructure

**優先度:** 完了 (Phase 1-4) / 2.x (Phase 5-6)
**原則:** ユーザーに HKT syntax は見せない。コンパイラ内部の表現力を上げる。

> 「ユーザーにはシンプル、コンパイラは賢い」

---

## 完了

- [x] **Phase 1: Ty ヘルパー + TypeConstructor 基盤** → [done/hkt-foundation-phase1.md](../done/hkt-foundation-phase1.md)
- [x] **Phase 2: 代数法則テーブル** (6 法則)
- [x] **Phase 3: Stream Fusion Nanopass** — 全 6 代数法則の IR rewrite 実装完了
- [x] **Phase 4: Ty::Applied 統一** — List/Option/Result/Map 削除、Applied(TypeConstructorId, Vec<Ty>) に統一。23 ファイル、~200 match arms 移行。

---

## 残り (2.x)

### Phase 5: コンパイラ内部の Effect 情報付与 (構文変更なし)

コンパイラ内部で FnType に EffectSet を持たせる。
ユーザー構文には一切にじませない — `effect fn` が唯一のマーカー。

### Phase 6: Trait 統合 (構文は別途検討)

内部 HKT 表現上に Trait/Protocol を構築。
ユーザー定義型の自動 Mappable 等が可能になる。
構文の導入有無・形は別途設計判断。

---

## Stream Fusion — 全 6 法則 ✅

| 法則 | 変換 | 状態 |
|------|------|------|
| FunctorComposition | `map(map(x,f),g)` → `map(x, f>>g)` | ✅ |
| FunctorIdentity | `map(x, id)` → `x` | ✅ |
| FilterComposition | `filter(filter(x,p),q)` → `filter(x, p&&q)` | ✅ |
| MapFoldFusion | `fold(map(x,f),i,g)` → `fold(x, i, g∘f)` | ✅ |
| MapFilterFusion | `filter(map(x,f),p)` → `filter_map(x, ...)` | ✅ |
| MonadAssociativity | `flat_map(flat_map(x,f),g)` → `flat_map(x, f>>=g)` | ✅ |

### 将来の拡張
| 項目 | 状態 |
|------|------|
| Let-binding chain rewrite | 検出のみ (書き換え未実装) |
