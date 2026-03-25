# LLM Benchmark Execution

**優先度:** 高 — Almide の存在意義「LLM が最も正確に書ける言語」の実証
**前提:** benchmark/ フレームワーク構築済み（runner.py + 3 問題セット）

---

## 現状

- フレームワーク構築完了: `benchmark/runner.py` + adapters + LLM client
- 3 問題セット用意: pangram (L1), calculator (L2), config-merger (L3)
- dry-run 動作確認済み
- **実際の LLM 呼び出しは未実行**

## Phase 1: パイロット実行

- [ ] `config.yaml` に API key 設定
- [ ] 3 問 × 3 言語(almide, python, ts) × 3 試行 = 27 呼び出しでフロー検証
- [ ] プロンプト調整、結果パーサーのデバッグ
- [ ] Almide adapter が `almide test` で正しく判定するか確認

## Phase 2: 問題セット拡充

- [ ] 既存 exercises/ から 12 問追加（合計 15 問）
- [ ] 新規 15 問作成（合計 30 問）
- [ ] 各問題に modification 仕様 2 種追加（MSR 用）
- [ ] Go, Rust 版テンプレート・テスト・模範解答作成

## Phase 3: 本番実行

- [ ] FAR: 30 問 × 5 言語 × 10 試行 = 1,500 呼び出し
- [ ] MSR: 30 問 × 5 言語 × 2 変更 × 10 試行 = 3,000 呼び出し
- [ ] FLE: fail ケースの修正ループ（最大 5 ループ）
- [ ] 推定コスト: ~$84 (Sonnet), ~$408 (Opus)

## Phase 4: 分析・公開

- [ ] 集計、統計的有意性検定 (Fisher exact test)
- [ ] Markdown レポート生成
- [ ] README / サイトへの結果掲載
