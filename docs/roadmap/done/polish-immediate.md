# A. すぐ効果が出る磨き

## 1. ICE 警告の除去

**状態:** `[ICE] lower: missing type for expr id=N` が毎回 stderr に出る
**原因:** `instantiate_ty` で生成した fresh var の expr_id が checker の `expr_types` に登録されない。lower pass が `expr_types.get(id)` で見つからず ICE を出す
**修正:** `instantiate_ty` で新しい InferTy を返す時、対応する expr_id → Ty のマッピングも `infer_types` に追加する。または lower pass の `expr_ty()` で missing を warning ではなく silent fallback にする
**見積り:** 1時間

## 2. Integration テスト 8/12 → 12/12

**状態:** `spec/integration/` の4ファイルが失敗
**確認方法:** `./target/debug/almide test spec/integration/`
**修正:** spec/lang と同じアプローチで1ファイルずつエラーを確認して修正
**見積り:** 半日（spec/lang の経験があるので速い）

## 3. TS target E2E テスト

**状態:** TS Result 維持を実装したが、生成コードを実際に実行するテストがない
**修正:** Deno で `./target/debug/almide build test.almd --target ts -o test.ts && deno run test.ts` のような E2E テストを追加
**見積り:** 半日
