# MSR (Modification Survival Rate) 計測設計

> **目的**: 「Almide は LLM が最も正確に書ける言語」を数字で証明する

## 定義

**MSR (Modification Survival Rate)** = LLM が生成したコードがコンパイル＋テスト通過する割合

```
MSR = (テスト通過した exercise 数) / (全 exercise 数) × 100%
```

## 計測対象

### Almide exercises (25本)

| Tier | Exercise | 難易度 | 主な言語機能 |
|------|----------|--------|-------------|
| 1 | collatz | 基本 | 再帰, if/else |
| 1 | raindrops | 基本 | 条件分岐, 文字列結合 |
| 1 | isogram | 基本 | 文字列処理, set |
| 1 | pangram | 基本 | 文字列走査 |
| 1 | hamming | 基本 | リスト処理, zip |
| 2 | bob | 中級 | 文字列分析, 複合条件 |
| 2 | scrabble-score | 中級 | map, fold |
| 2 | roman-numerals | 中級 | パターンマッチ, 再帰 |
| 2 | isbn-verifier | 中級 | 文字列パース, Result |
| 2 | phone-number | 中級 | バリデーション, Option |
| 3 | calculator | 上級 | variant 型, 再帰評価, effect fn |
| 3 | traffic-light | 上級 | variant, match, convention methods |
| 3 | todo-app | 上級 | レコード, リスト操作, CRUD |
| 3 | expression-eval | 上級 | 再帰 variant, パターンマッチ |
| 3 | affine-cipher | 上級 | 数学, 文字列変換 |
| 4 | config-merger | 応用 | Map, 再帰マージ |
| 4 | data-table | 応用 | レコード, ソート, フィルター |
| 4 | grade-report | 応用 | 集計, グルーピング |
| 4 | json-config | 応用 | JSON, Codec, Value |
| 4 | pipeline | 応用 | パイプ, 関数合成 |
| 5 | markdown-renderer | 高度 | パーサー, variant, 文字列処理 |
| 5 | named-things | 高度 | protocol/convention, 抽象化 |
| 5 | sortable | 高度 | ジェネリクス, 比較 |
| 6 | collatz-conjecture | 高度 | 数学, 最適化 |
| 6 | wasm-smoke | 特殊 | WASM ターゲット |

## 計測プロトコル

### Step 1: テスト抽出

各 exercise から実装を除去し、テストブロック + 型シグネチャのみ残す（= 問題文）。

```bash
almide msr extract exercises/bob/bob.almd > msr/prompts/bob.txt
```

抽出後の形式:
```
// Exercise: bob
// Implement the function `respond` that takes a string input and returns Bob's response.

fn respond(input: String) -> String = todo

test "stating something" { assert_eq(respond("Tom-ay-to, tom-ah-to."), "Whatever.") }
test "shouting" { assert_eq(respond("WATCH OUT!"), "Whoa, chill out!") }
...
```

### Step 2: LLM に解かせる

各 exercise について:
1. システムプロンプトに CHEATSHEET.md を含める
2. 問題文（テスト + シグネチャ）を渡す
3. LLM の出力を `.almd` ファイルとして保存
4. **修正なし** — LLM の初回出力をそのまま使う

### Step 3: 検証

```bash
almide msr verify msr/outputs/claude-opus/bob.almd
```

検証基準:
1. `almide check` — 型チェック通過
2. `almide test` — 全テスト通過 (target rust)
3. 両方通過 → ✅、どちらか失敗 → ❌

### Step 4: 結果集計

```
MSR Report — Claude Opus 4 × Almide exercises (target: rust)
═══════════════════════════════════════════════════════════
Tier 1:  5/5  (100%)  — collatz, raindrops, isogram, pangram, hamming
Tier 2:  4/5  ( 80%)  — bob ✅, scrabble ✅, roman ✅, isbn ❌, phone ✅
Tier 3:  3/5  ( 60%)  — calculator ✅, traffic ✅, todo ✅, expr ❌, affine ❌
Tier 4:  4/5  ( 80%)  — config ✅, data-table ✅, grade ✅, json ✅, pipeline ❌
Tier 5:  2/3  ( 67%)  — markdown ✅, named ✅, sortable ❌
Tier 6:  1/2  ( 50%)  — collatz-conj ✅, wasm ❌
─────────────────────────────────────────────────────────
Total:  19/25 (76%)
```

## 比較対象

同じ問題を Rust / TypeScript / Go / Python で書かせ、MSR を比較する。

### Rust での同等テスト

各 exercise を Rust 版に翻訳:
- 同じアルゴリズム、同じテストケース
- Almide の `effect fn` → Rust の `Result<T, String>`
- Almide の `match` → Rust の `match`

```
MSR Comparison (Claude Opus 4, 初回正答率)
══════════════════════════════════════════
Almide:     19/25 (76%)
Rust:       12/25 (48%)  ← borrow checker, lifetime で躓く
TypeScript: 17/25 (68%)  ← 型エラーが多い
Go:         15/25 (60%)  ← error handling パターンで躓く
Python:     20/25 (80%)  ← 動的型付けで通りやすいが型安全性なし
```

(↑ 数値は仮。実測で置き換える)

## 計測ツール

### `almide msr` サブコマンド（将来）

```bash
# テスト抽出
almide msr extract <exercise.almd> [--output <dir>]

# 検証
almide msr verify <solution.almd> [--target rust|ts|js]

# 全体レポート
almide msr report <solutions-dir> [--format markdown|json]
```

### 手動計測スクリプト (初回用)

```bash
#!/bin/bash
# msr_measure.sh — MSR 計測スクリプト
#
# 使い方:
#   1. exercises/ から問題テンプレートを msr/prompts/ に生成
#   2. LLM に解かせた結果を msr/outputs/<model>/ に配置
#   3. このスクリプトで検証

MODEL=${1:-"claude-opus"}
PASS=0
FAIL=0
TOTAL=0

for solution in msr/outputs/$MODEL/*.almd; do
  name=$(basename "$solution" .almd)
  TOTAL=$((TOTAL + 1))

  # Type check
  if ! almide check "$solution" > /dev/null 2>&1; then
    echo "❌ $name — type check failed"
    FAIL=$((FAIL + 1))
    continue
  fi

  # Test
  if almide test "$solution" > /dev/null 2>&1; then
    echo "✅ $name"
    PASS=$((PASS + 1))
  else
    echo "❌ $name — test failed"
    FAIL=$((FAIL + 1))
  fi
done

echo ""
echo "MSR: $PASS/$TOTAL ($((PASS * 100 / TOTAL))%)"
```

## 問題テンプレート生成スクリプト

```bash
#!/bin/bash
# msr_extract.sh — exercise からテンプレートを抽出
#
# 実装部分を `todo` に置き換え、テストブロックは残す

mkdir -p msr/prompts

for ex in exercises/*/*.almd; do
  name=$(basename "$(dirname "$ex")")
  outfile="msr/prompts/${name}.almd"

  # テスト行を抽出
  grep '^test ' "$ex" > /tmp/msr_tests.txt

  # 関数シグネチャを抽出（= の前まで）し、body を todo に
  # これは exercise の構造に依存するため、手動調整が必要な場合あり
  sed 's/ = {.*/ = todo/' "$ex" | sed 's/ = [^{].*/ = todo/' > "$outfile"

  echo "Extracted: $name"
done
```

## 公開計画

### データ形式

```json
{
  "date": "2026-03-20",
  "language": "almide",
  "version": "1.0.0",
  "target": "rust",
  "model": "claude-opus-4",
  "exercises": 25,
  "passed": 19,
  "msr": 0.76,
  "results": [
    { "name": "bob", "tier": 2, "passed": true, "check_ms": 14, "test_ms": 230 },
    { "name": "isbn-verifier", "tier": 2, "passed": false, "error": "type_check", "detail": "..." }
  ]
}
```

### 公開場所

- `docs/research/msr-results/` — JSON データ
- ブログ記事 — 分析と考察
- README.md — バッジ: `MSR: 76% (Claude Opus 4)`

## 注意事項

- **自己評価バイアス**: Almide のコードを書いた人（or その LLM）が計測すると有利になる。第三者に計測してもらうか、計測プロトコルを公開して再現可能にすべき
- **CHEATSHEET の影響**: システムプロンプトに CHEATSHEET を含めるかどうかで結果が大きく変わる。両条件で計測する
- **モデル依存**: Claude vs GPT-4 vs Gemini で結果が異なる。複数モデルで計測
- **target 依存**: `--target rust` と `--target ts` で MSR が異なる可能性がある

## 次のステップ

1. [ ] `msr/prompts/` に全 25 exercise のテンプレートを生成
2. [ ] 計測スクリプト (`msr_measure.sh`) の動作確認
3. [ ] Claude Opus で初回計測実行
4. [ ] Rust 版 exercise を作成し、比較計測
5. [ ] 結果をブログ記事にまとめる
