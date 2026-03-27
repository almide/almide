<!-- description: Structured error codes (E001-E010) and JSON diagnostic output -->
<!-- done: 2026-03-17 -->
# Error Codes + JSON Output [DONE — 1.0 Phase II]

## 実装済み

- [x] エラーコード体系: E001-E010 (type mismatch, undefined function/variable, arg count/type, effect isolation, fan restrictions, assign to immutable, non-exhaustive match)
- [x] `almide check --json`: 1行1 diagnostic の構造化 JSON 出力
- [x] `almide check --explain E001`: エラーコードの詳細説明
- [x] `almide test --json`: 1行1ファイルの構造化テスト結果
- [x] check 速度: 298行で 14ms (debug) / 25ms (release) — 500行 < 1秒を大幅クリア
