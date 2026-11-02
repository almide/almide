<a id="playground-repair-turn"></a>

# Playground の修復ターン

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](playground-repair.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](playground-repair.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](playground-repair.zh-CN.md)

> 非規範の固定読者用スナップショット。原文を正本とし、自動同期せず、実装・実測・安全性を独立に再検証したものではない。
> [Source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/done/playground-repair.md)
> 完了メタデータと未完了チェック項目、v0.4.7 時点の状態が共存する履歴である。提案された平均 1.2 ターンは指標の例、1 ターン修復はデモの成功条件であって、現在の実測保証ではない。


> 原文の説明：Playground の AI によるエラー修復と型検査器の統合
> 原文の完了メタデータ：2026-03-11

<a id="vision"></a>

## ビジョン

利用者が Playground でコードを書いて実行し、エラーを受け取り、「Fix with AI」をクリックする。LLM がエラーを読んで修正し、利用者はその修復過程を見る。

Almide の「エラーメッセージが十分明確なので、LLM が 1 ターンで修正できる」ことを示すデモ。

<a id="current-state-v047"></a>

## 現状（v0.4.7）

<a id="done"></a>

### 完了
- [x] Playground に型検査器を追加（parse -> **check** -> emit）
- [x] AI 生成フロー内の修復ターン（生成 -> エラー -> 自動修復 x 3）
- [x] 手動 Run の「Fix with AI」ボタン（Run -> エラー -> ボタン -> 修復ループ）
- [x] Anthropic / OpenAI / Gemini のストリーミング対応
- [x] repair-log UI（error/fix/ok/fail のステップ表示）

<a id="not-yet"></a>

### 未完了
- [ ] Playground の almide 依存を v0.4.7 に更新（PR #15 のマージ後）
- [ ] CLAUDE.md（system prompt）に list.swap と immutable パターンを含める
- [ ] 修復結果の差分表示
- [ ] 修復前後のコード比較（左右並列またはインライン差分）
- [ ] 「Accept fix」／「Reject fix」ボタン

<a id="architecture"></a>

## アーキテクチャ

```
User writes code
       │
       ▼
   ┌──────────┐
   │   Run     │
   └────┬─────┘
        │
   ┌────▼─────┐     ok     ┌──────────┐
   │ compile   │───────────▶│  Output   │
   │ + run     │            └──────────┘
   └────┬─────┘
        │ error
   ┌────▼──────────┐
   │ Show error +   │
   │ "Fix with AI"  │
   └────┬──────────┘
        │ click
   ┌────▼──────────┐
   │ LLM repair    │◀──┐
   │ (stream)      │   │ error (max 3)
   └────┬──────────┘   │
        │              │
   ┌────▼─────┐   ┌───┴────┐
   │ compile   │──▶│ retry  │
   │ + run     │   └────────┘
   └────┬─────┘
        │ ok
   ┌────▼─────┐
   │ Output + │
   │ "Fixed!" │
   └──────────┘
```

<a id="repair-prompt-strategy"></a>

## 修復プロンプトの戦略

現行：
```
{phase} error:
{error message}

Fix the code and output ONLY the corrected .almd source. No explanations.
```

改善版（TODO）：
- システムプロンプトに Almide の文法概要＋よくあるエラーパターンを含める
- `cannot reassign immutable binding` -> `var` とタプル返却パターンのどちらを使うか判断
- `list.get returns Option` -> `list.get_or` または `list.swap` を提案

<a id="roadmap"></a>

## ロードマップ

<a id="tier-1--completeness-short-term"></a>

### Tier 1 — 完全性（短期）

<a id="11-update-playground-almide-dependency"></a>

#### 1.1 Playground の almide 依存を更新
PR #15 のマージ後、`Cargo.toml` の almide git ref を更新する。
検査器が WASM で動作することを確認する。

<a id="12-system-prompt-optimization"></a>

#### 1.2 システムプロンプトの最適化
Playground の LLM 修復プロンプトに Almide 固有のパターンを含める：
- `var` と `let` の使い方の違い
- in-place アルゴリズム向けの `list.swap`
- `list.get` は nullable を返す → `list.get_or` を使う
- 変更と返却を行う関数ではタプルを返す

<a id="13-diff-display"></a>

#### 1.3 差分表示
修復前後のコード差分を表示する。利用者が「何が変わったか」を一目で理解できる。
単純なインライン差分（追加行が緑、削除行が赤）で十分。

<a id="tier-2--ux-improvements-medium-term"></a>

### Tier 2 — UX の改善（中期）

<a id="21-accept--reject"></a>

#### 2.1 承認／却下
「Fix with AI」の後、修復したコードを仮に表示する。利用者が Accept または Reject を選ぶ。
Reject は元のコードを復元する。

<a id="22-partial-repair"></a>

#### 2.2 部分修復
エラー箇所だけを強調し、その関数だけを修復する。
LLM では全体を書き直すより正確。

<a id="23-repair-history"></a>

#### 2.3 修復履歴
複数の修復ターンを時系列に表示する。
「何を試し、何を直したか」を示す。

<a id="tier-3--benchmark-integration-long-term"></a>

### Tier 3 — ベンチマーク統合（長期）

<a id="31-modification-survival-rate-visualization"></a>

#### 3.1 Modification survival rate の可視化
修復ターン数を記録し、「Almide は平均 1.2 ターンで修復する」のような指標を表示する。
他言語との比較データを Playground に埋め込む。

<a id="32-auto-repair-mode"></a>

#### 3.2 自動修復モード
Run ボタンに「Auto-repair」切り替えを追加する。ON ならエラー時に自動で修復を開始する。
利用者は修復過程をリアルタイムで見る。

<a id="success-metric"></a>

## 成功指標

Playground での quicksort（immutable パターン）：
1. 利用者が mutable パターンで書く -> `cannot reassign immutable binding` エラー
2. 「Fix with AI」 -> LLM が `var`＋タプル返却＋`list.swap` で修正
3. 1 ターンで修復が完了し、ソート結果が表示される

これが動けば、「Almide のエラーは LLM が修正できるほど明確」というデモが完成する。
