<!-- description: Unify InferTy and Ty representations in the type checker -->
# Checker InferTy/Ty Unification

**優先度:** post-1.0 (1.x)
**見積:** ±1000行, 大。型システム根幹。

## 現状

型推論中は `InferTy` (unification variable付き), 確定後は `Ty`。毎回変換コスト。

## 理想

unified type で推論と確定を同じ型で表現。solutions テーブル管理の簡素化。

## タスク

- [ ] `InferTy` と `Ty` の統一型設計
- [ ] 変換コスト削減
- [ ] solutions テーブル管理の簡素化

## 判断

型システム根幹の変更。慎重に設計してから実施。1.0 後推奨。
