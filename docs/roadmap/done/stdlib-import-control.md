<!-- description: Three-tier import visibility for stdlib modules -->
<!-- done: 2026-03-24 -->
# Stdlib Import Control

**優先度:** 1.0
**前提:** なし

## 問題

現在、全 stdlib モジュール（22モジュール）が `import` なしで使える。`math.abs(-5)` と書けば `import math` なしで動く。これは仕様違反。

## 理想形

Swift の Foundation / UIKit モデルを参考に、3層の可視性:

### Tier 1: 暗黙 import（import 不要）

言語のコア型に直結するモジュール。UFCS でも `module.func()` でも使える。

候補:
- `string` — String 型の操作
- `int` — Int 型の変換
- `float` — Float 型の変換
- `list` — List 型の操作
- `map` — Map 型の操作
- `set` — Set 型の操作
- `option` — Option 型の操作
- `result` — Result 型の操作
- `bool` — Bool 型の操作（存在すれば）

**根拠:** これらはコア型のメソッド群。`"hello".len()` (UFCS) が import 不要で動くなら、`string.len("hello")` も同等に動くべき。

### Tier 2: 明示 import 必要

汎用ユーティリティ。使わないプログラムも多い。

候補:
- `math` — 数学関数
- `json` — JSON 操作
- `regex` — 正規表現
- `random` — 乱数
- `datetime` — 日時
- `env` — 環境変数
- `fs` — ファイルシステム
- `http` — HTTP クライアント/サーバー
- `path` — パス操作
- `process` — プロセス
- `log` — ログ
- `time` — 時間
- `codec` — エンコード/デコード

### Tier 3: 組み込み（モジュール名なしで使える）

言語キーワードレベル:
- `println`, `eprintln` — トップレベル関数
- `assert`, `assert_eq` — テスト用
- `ok`, `err`, `some`, `none` — コンストラクタ
- `fan` — 並行処理

## 設計判断が必要な点

1. **Tier 1 のリストは正しいか？** string/int/float/list/map/set/option/result で足りるか、多すぎないか
2. **UFCS との関係:** `x.abs()` は型から `math.abs` に解決される。import なしで動くべきか？
   - 案A: UFCS は常に動く（型ベース解決は import と無関係）
   - 案B: UFCS も import が必要（`import math` しないと `(-5).abs()` も動かない）
   - **推奨: 案A** — UFCS はメソッド呼び出しの糖衣構文であり、import はモジュール名前空間の制御
3. **既存テストへの影響:** 多くのテストが `import` なしで stdlib を使っている。Tier 1 を正しく設定すれば大半は壊れない

## 実装方針

1. `TypeEnv` に `imported_stdlib: HashSet<String>` を追加
2. `check_program` で `program.imports` からモジュール名を収集
3. Tier 1 モジュールは暗黙的に `imported_stdlib` に追加
4. `resolve_module_call` (infer.rs) と `static_dispatch.rs` で `imported_stdlib` をチェック
5. `calls.rs` の stdlib フォールバック（UFCS 経由）は import チェックしない（型ベース解決）

## 実装進捗

| Phase | 内容 | 状態 |
|---|---|---|
| Phase 1 | Tier 分類の確定 (Swift モデル採用) | ✅ 完了 |
| Phase 2 | imported_stdlib + Tier 1 暗黙登録 | ✅ 完了 |
| Phase 3 | resolve_module_call + static_dispatch で import ゲート | ✅ 完了 |
| Phase 4 | 既存テスト全通過 (19ファイルに import 追加) | ✅ 完了 |
| Phase 5 | エラーメッセージ改善（"did you mean: import math?"） | 未着手 |
