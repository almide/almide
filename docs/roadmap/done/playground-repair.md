<!-- description: Playground AI-powered error repair and type checker integration -->
# Playground Repair Turn

## Vision

ユーザーがPlaygroundでコードを書いて Run → エラー → 「Fix with AI」→ LLMがエラーを読んで修正 → ユーザーが修正過程を見る。

Almideの「エラーメッセージが明確だからLLMが1ターンで直せる」を体験できるデモ。

## Current State (v0.4.7)

### Done
- [x] Playground に type checker 追加（parse → **check** → emit）
- [x] AI生成フローでの修復ターン（generate → error → auto-repair × 3）
- [x] 手動Run時の「Fix with AI」ボタン（Run → error → button → repair loop）
- [x] Anthropic / OpenAI / Gemini ストリーミング対応
- [x] repair-log UI（error/fix/ok/fail のステップ表示）

### Not Yet
- [ ] Playground の almide 依存を v0.4.7 に更新（PR #15 マージ後）
- [ ] CLAUDE.md (system prompt) にlist.swap、immutable patterns を含める
- [ ] 修復結果の diff 表示
- [ ] 修復前後のコード比較（side-by-side or inline diff）
- [ ] 「Accept fix」/「Reject fix」ボタン

## Architecture

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

## Repair prompt strategy

Current:
```
{phase} error:
{error message}

Fix the code and output ONLY the corrected .almd source. No explanations.
```

Improved (TODO):
- system prompt に Almide の文法概要 + よくあるエラーパターンを含める
- `cannot reassign immutable binding` → `var` にするか、tuple return パターンに変えるか判断
- `list.get returns Option` → `list.get_or` or `list.swap` を提案

## Roadmap

### Tier 1 — 完成度 (short-term)

#### 1.1 Playground の almide 依存更新
PR #15 マージ後、`Cargo.toml` の almide git ref を更新。
checker が WASM で動くことを確認。

#### 1.2 System prompt 最適化
Playground の LLM 修復プロンプトに Almide 特有のパターンを含める:
- `var` vs `let` の使い分け
- `list.swap` for in-place algorithms
- `list.get` returns nullable → use `list.get_or`
- tuple return for functions that modify + return

#### 1.3 Diff 表示
修復前後のコードを差分表示。ユーザーが「何が変わったか」を一目で理解できる。
簡易 inline diff（追加行は緑、削除行は赤）で十分。

### Tier 2 — UX 改善 (medium-term)

#### 2.1 Accept / Reject
「Fix with AI」後、修復コードを仮表示。ユーザーが Accept か Reject を選ぶ。
Reject → 元のコードに戻す。

#### 2.2 部分修復
エラー箇所だけハイライトして、その関数だけ修復する。
全体書き換えより LLM の精度が上がる。

#### 2.3 修復履歴
複数回の修復ターンを時系列で表示。
「何を試して何が直ったか」がわかる。

### Tier 3 — ベンチマーク連携 (long-term)

#### 3.1 Modification survival rate の可視化
修復ターン数を記録し、「Almide は平均 1.2 ターンで修復」のようなメトリクスを表示。
他言語との比較データを Playground に埋め込む。

#### 3.2 Auto-repair mode
Run ボタンに「Auto-repair」トグル。ON にすると、エラーが出たら自動的に修復を開始。
ユーザーは修復過程をリアルタイムで見る。

## Success Metric

quicksort (immutable patterns) を Playground で:
1. ユーザーがmutableパターンで書く → `cannot reassign immutable binding` エラー
2. 「Fix with AI」→ LLM が `var` + tuple return + `list.swap` に修正
3. 1 ターンで修復完了、ソートされた結果が表示される

これが動けば「Almide はエラーが明確で LLM が直せる」のデモとして完成。
