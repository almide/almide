<!-- description: LLM generates typed IR as JSON directly, bypassing parser errors -->
# LLM → IR Direct Generation

LLM がテキストではなく型付き IR (JSON) を直接生成し、パーサーエラーをゼロにする。

## Why

現在の LLM コード生成:
```
LLM → .almd テキスト → Lexer → Parser → AST → Checker → Lowering → IR → Codegen
```

問題: LLM はテキストを生成するため、構文エラー・インデントミス・トークン境界の曖昧さが発生する。modification survival rate の主要なボトルネック。

提案する新経路:
```
LLM → IR (JSON) → Codegen
```

IR は:
- JSON serializable (`serde::Serialize/Deserialize` 済み)
- 構造が固定 (30 種の `IrExprKind`, 8 種の `IrStmtKind`)
- 全ノードが型情報を持つ (Ty enum)
- パイプ・UFCS・string interpolation が脱糖済み

LLM が structured output (JSON) を生成する精度はテキスト生成より高い。特に OpenAI の structured outputs や Anthropic の tool use は JSON スキーマに準拠した出力を保証できる。

## Architecture

```
                    ┌─────────────────────┐
                    │  Traditional path   │
                    │  .almd → ... → IR   │
                    └──────────┬──────────┘
                               │
                               ▼
LLM → JSON ──────────────▶ IrProgram ──▶ Codegen ──▶ .rs / .ts
                               ▲
                               │
                    ┌──────────┴──────────┐
                    │  Validation pass    │
                    │  (type consistency, │
                    │   VarId resolution) │
                    └─────────────────────┘
```

## Phases

### Phase 1: IR round-trip validation
- `almide emit --emit-ir app.almd | almide compile --from-ir` のパイプライン
- IR JSON → deserialize → codegen → 元と同一出力の検証
- これは `--emit-ir` roadmap の延長

### Phase 2: IR validation pass
- 外部生成された IR JSON を受け取り、整合性チェック:
  - VarId が VarTable 内に存在するか
  - 各ノードの Ty が整合しているか
  - CallTarget の参照先が存在するか
- エラーがあれば修正ヒント付き diagnostic を返す

### Phase 3: LLM prompt engineering
- IR JSON Schema を LLM のシステムプロンプトに含める
- Few-shot examples: `.almd` ソース + 対応する IR JSON ペア
- structured output mode で IR を直接生成させる

### Phase 4: Hybrid mode
- LLM がまずテキスト `.almd` を生成 → パーサーエラーが出たら IR 直接生成にフォールバック
- `almide forge` (既存 LLM integration roadmap) との統合

## Key insight

IR redesign Phase 5 の完了が前提条件。codegen が `&IrProgram` のみを入力とすることが証明されたため、「LLM → IR → codegen」パスが技術的に成立する。AST フォールバックが残っていたら IR だけでは codegen できなかった。

## Risk

- IR の JSON は `.almd` テキストより冗長 (10-50x)。LLM のコンテキストウィンドウを消費する
- VarId の一貫性を LLM が維持できるか未検証
- ミティゲーション: VarId を名前ベースで生成し、post-pass で VarId に変換する中間形式を検討

## Related

- [--emit-ir](emit-ir.md) — IR JSON 出力の基盤
- [LLM Integration](llm-integration.md) — `almide forge` / `almide fix`
