<!-- description: Automated verification that Rust, TS, and WASM produce identical output -->
# Cross-Target Parity Matrix

**優先度:** High — WASM 対応進行中の今が最適タイミング
**前提:** Cross-Target CI 完了済み、WASM 167/194 pass (0 failed, 27 skipped), TS テストランナー追加済み
**目標:** Rust/TS/WASM 3ターゲット間の挙動差異を体系的に検出・防止する自動検証基盤

> 「CI で "全テスト通過" は必要条件。パリティマトリクスは十分条件。」

---

## Why

Cross-Target CI は「同じテストが両方通るか」を見る。パリティマトリクスは「同じ入力に対して同じ出力を返すか」を見る。前者では検出できない差異がある:

- 浮動小数点の丸め差異
- 文字列のエンコーディング差異 (UTF-8 boundary)
- 整数オーバーフロー挙動
- エラーメッセージの文言差異
- コレクションの順序保証 (Map iteration order)
- ゼロ除算・NaN の伝播

WASM ターゲットが安定に向かう今、3ターゲット間の「見えない差異」を潰す仕組みが必要。

---

## Design

### パリティテストの構造

```almd
// spec/parity/numeric_parity_test.almd
test "integer overflow behaves identically" {
    let x = 2147483647  // i32 max
    assert_eq(x + 1, 2147483648)  // i64 なので wrap しない
}

test "float precision consistent" {
    assert_eq(0.1 + 0.2 == 0.30000000000000004, true)
}
```

パリティテストは通常のテストと同じ形式だが、`spec/parity/` に配置し、全ターゲットで出力を比較する。

### 検証レイヤー

| レイヤー | 検証内容 | 方法 |
|---|---|---|
| L1: 出力一致 | stdout が全ターゲットで一致 | 既存 CI の拡張 |
| L2: エッジケース | 型境界・精度・エンコーディング | 専用パリティテスト |
| L3: stdlib 網羅 | 全 stdlib 関数が全ターゲットで同一挙動 | モジュール別マトリクス |
| L4: エラー挙動 | panic/throw/trap の条件が一致 | エラーケーステスト |

### CI 統合

```yaml
# .github/workflows/ci-parity.yml
# develop push で自動実行
# 各テストを --target rust, --target ts, --target wasm で実行
# stdout を diff し、差異があれば fail
```

---

## Phases

### Phase 1: パリティテスト基盤 + 数値・文字列

- [ ] `spec/parity/` ディレクトリ作成
- [ ] パリティテストランナー (`almide test --parity`: 全ターゲットで実行 + diff)
- [ ] 数値パリティテスト (整数境界、浮動小数点精度、NaN/Inf)
- [ ] 文字列パリティテスト (UTF-8 境界、空文字列、絵文字、結合文字)
- [ ] CI ワークフロー追加

### Phase 2: stdlib マトリクス

- [ ] 22 モジュール × 3 ターゲットのパリティマトリクス自動生成
- [ ] 各 stdlib 関数にパリティテスト追加 (エッジケース重点)
- [ ] マトリクスレポート出力 (`almide test --parity --report`)

### Phase 3: エラー挙動パリティ

- [ ] ゼロ除算、範囲外アクセス、型ミスマッチのエラー挙動統一
- [ ] エラーメッセージのターゲット間差異の許容範囲定義
- [ ] trap/panic/throw の発生条件一致テスト

### Phase 4: 回帰防止

- [ ] 新規 stdlib 関数追加時にパリティテスト必須化 (CI gate)
- [ ] パリティ違反の自動分類 (意図的差異 vs バグ)
- [ ] ターゲット品質ダッシュボード (Tier 1/2/3 と連動)

---

## Success Criteria

- 全 stdlib 関数に対するパリティテストが存在する
- Rust/TS 間のパリティ 100%
- WASM との差異が文書化され、意図的差異のみ残る
- 新規 stdlib 追加時にパリティテストがないと CI fail
