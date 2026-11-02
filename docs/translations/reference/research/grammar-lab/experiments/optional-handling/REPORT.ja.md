<a id="experiment-optional-handling"></a>

# 実験：optional-handling

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](REPORT.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](REPORT.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](REPORT.zh-CN.md)

> 固定原文の非規範的な履歴参照資料。以下のテスト・ブランチ状態・測定・修正は日付付き報告で、新規実行した検証や普遍的な正しさの保証ではない。コードは原文保持し、ベンチマークは実行していない。有意差・?. 構文・他モデルについての結論は原著者の解釈であり、この小標本だけで独立に確立した知見ではない。自動同期せず、原本を正本とする。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/research/grammar-lab/experiments/optional-handling/REPORT.md)

<a id="hypothesis"></a>

## 仮説

コンビネータ形式の Option 処理（`option.map` + `option.unwrap_or` のパイプ連鎖）は、match 形式（`match some/none`）より変更後の生存率が高い。

<a id="setup"></a>

## 設定

- **モデル**：Claude Haiku 4.5
- **試行**：各タスク・各方式につき3回
- **タスク**：10
- **総実行数**：60（10タスク × 2方式 × 3試行）

<a id="variants"></a>

### 方式

| | match | コンビネータ |
|---|---|---|
| Option の分解 | `match opt { some(x) => ..., none => ... }` | `opt \|> option.map((x) => ...) \|> option.unwrap_or(default)` |
| トランスパイル | なし | `option.map` → `match`、`option.unwrap_or` → `match` |
| Layer2 プロンプト | match 形式の例 | コンビネータ形式の例 |

<a id="results"></a>

## 結果

| 方式 | 合格 | 率 |
|---------|------|------|
| **match** | **21/30** | **70%** |
| **コンビネータ** | **12/30** | **40%** |

<a id="per-task-breakdown"></a>

### タスク別の内訳

| タスク | match | コンビネータ | 注記 |
|------|-------|-----------|-------|
| t01 change_default | 3/3 | 3/3 | 両方式とも合格 |
| t02 add_format | 3/3 | 0/3（コンパイル） | トランスパイルのバグ：`option.map` 本体内の文字列補間 `${}` で正規表現が破綻 |
| t03 add_function | 3/3 | 3/3 | 両方式とも合格 |
| t04 apply_discount | 3/3 | 3/3 | 両方式とも合格 |
| t05 chain_fields | 3/3 | 0/3（コンパイル） | トランスパイルのバグ：`option.map` 本体内の `int.to_string()` 呼び出しで正規表現が破綻 |
| t06 change_predicate | 3/3 | 0/3（コンパイル） | トランスパイルのバグ：`option.map` 本体内の `int.to_string()` 呼び出しで正規表現が破綻 |
| t07 change_none_msg | 0/3 | 0/3 | 両方式とも失敗 — LLM は none 分岐のメッセージ内での `target` 変数のキャプチャに苦戦 |
| t08 sort_direction | 0/3 | 0/3 | 両方式とも失敗 — LLM が `list.reverse()` を正しく削除できない、または関数名を変更 |
| t09 add_condition | 3/3 | 3/3 | 両方式とも合格 |
| t10 add_step | 0/3 | 0/3（コンパイル） | 両方式とも失敗 — LLM は複数段階のパイプライン作成に苦戦 |

<a id="adjusted-results-excluding-transpile-bug-tasks-t02-t05-t06"></a>

### 調整後の結果（トランスパイルのバグがある t02・t05・t06 を除外）

| 方式 | 合格 | 率 |
|---------|------|------|
| **match** | **12/21** | **57%** |
| **コンビネータ** | **12/21** | **57%** |

**同一である。** トランスパイル基盤が正しく動く場合、match とコンビネータの生存率は等しい。

<a id="conclusion"></a>

## 結論

**match 形式とコンビネータ形式の Option 処理に、統計的に有意な差はない。**

生の70%対40%の差は、トランスパイル基盤のバグ（正規表現が文字列補間内の入れ子の括弧を扱えない）で完全に説明される。該当タスクを除くと、両方式の生存率は同じ57%になる。

<a id="implications-for-almide-design"></a>

### Almide の設計への示唆

1. **Match で十分。** 標準ライブラリに `option.map` / `option.flat_map` を追加しても LLM の精度は改善しない。
2. **`?.` 構文も役立たない。** コンビネータが match を上回らないなら、コンビネータの構文糖衣（`?.` / `??`）も上回らない。
3. **タスクの難しさが支配的。** 差は方式間ではなくタスク間（易しい：100%、難しい：0%）にある。LLM の精度改善に必要なのは構文変更でなく、より良いエラーメッセージ・例・言語規則である。

<a id="caveats"></a>

### 注意点

- 各セル N=3 試行は少ない。N=5以上で再現すべきである。
- トランスパイルのバグがコンビネータ側の3/10タスクに影響した。標準ライブラリで `option.map` を適切に実装すれば、この交絡要因をなくせる。
- Haiku でしかテストしていない。Sonnet では天井効果（両方式100%）が出る可能性がある。
- 失敗タスク（t07・t08・t10）は、LLM の能力よりタスク設計に問題があるかもしれない。

<a id="lessons"></a>

## 学び

1. **トランスパイルの品質がコンビネータの結果を左右する** — lambda-syntax 実験と同じ教訓。正規表現によるトランスパイルは文字列内の入れ子括弧で破綻する。
2. **Layer 1 の規則は構文形式より重要** — 両方式が同じタスクで成功・失敗することは、言語規則のプロンプトが主要因であることを示唆する。
3. **Haiku はベンチマークに適したモデル** — 全体70%という率には意味のあるばらつきがある。Sonnet なら両方式とも90%以上に達する可能性が高い。
