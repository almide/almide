<!-- description: Standardize diagnostics to Here/Try/Hint three-part format with CI-verified hint correctness -->
# Diagnostics: Here / Try / Hint Format

## Motivation

現在の診断メッセージは `hint:` フィールドを持っており、これは既に他言語より一歩進んでいる。しかし：

1. **形式が統一されていない** —— あるメッセージは hint を持ち、あるものは持たない
2. **hint の正しさが検証されていない** —— hint を機械適用して通るかどうか、誰もテストしていない
3. **LLM が "どこを直せばいいか" を視覚的に掴みづらい**

本項目は診断を **Here / Try / Hint の三段構え** に統一し、さらに **hint が貼り付ければ通ることを CI で証明する** 仕組みを導入する。

## Design

### 1. Three-part format

すべての診断を次の形式に統一：

```
'!' is not valid in Almide at line 5:12
  Here:  if !user.is_admin { ... }
  Try:   if not user.is_admin { ... }
  Hint:  '!' is unary negation in C-family; Almide uses 'not' (English word).
```

- **Here**：壊れた箇所を丸ごと 1 行で提示（複数行の場合は該当範囲）
- **Try**：修正後のコード片。**コピペすれば通る** ことが保証されている
- **Hint**：なぜそうなのか、1 文の理由

`Try` が機械的に導けない診断（例：型推論の矛盾で複数の解決がある場合）は、`Try` 欄を省略してよいが、その場合 `Hint` で具体的な選択肢を列挙する：

```
Hint: consider one of:
  - annotate the type: `let x: Int = ...`
  - pass a concrete value: `f(42)`
```

### 2. Hint self-test infrastructure

`tests/diagnostics/` に壊れた/直ったコードのペアを配置：

```
tests/diagnostics/
├── bang-not/
│   ├── broken.almd    # if !x { ... }
│   ├── fixed.almd     # if not x { ... }
│   └── meta.toml      # expected diagnostic code, line, etc.
```

CI が次を実行：

1. `broken.almd` をコンパイル
2. 診断を取得し、`Try:` のコード片を抽出
3. `broken.almd` の該当範囲を `Try:` の内容で置換
4. 置換後のファイルが `fixed.almd` と一致するかチェック
5. 置換後のファイルがコンパイル通過するかチェック

**hint が通らなければその診断は不良品** として CI が落ちる。これは Almide を「診断の正確さを自己検証する言語」にする。

### 3. Diagnostic code registry

すべての診断に一意なコード（`E0042` 形式）を付与し、`docs/diagnostics/E0042.md` にドキュメントを置く。LLM が診断コードを検索して過去の例を参照できるようにする。

### 4. Golden file updates

既存の診断 snapshot テストを三段構えに一括更新する。これが最大のチャーン源になるので、最初にゴールデン更新スクリプトを用意する。

## Implementation Phases

1. **Phase 1**: 三段構えのフォーマッタを実装（既存の `hint:` を `Try:` と `Hint:` に分割）
2. **Phase 2**: `tests/diagnostics/` のディレクトリ構造と harness を作成
3. **Phase 3**: 既存診断を段階的に移行（1 診断 1 PR で）
4. **Phase 4**: CI ゲート有効化：新しい診断は Here/Try/Hint 必須、hint self-test 必須
5. **Phase 5**: `docs/diagnostics/` のコードレジストリ公開

## Progress (2026-04-19)

- **Phase 1 MVP landed**. `Diagnostic::here_snippet: Option<String>` +
  `with_here(s)` builder added in `almide-base`. Renderer emits a
  `  here: <snippet>` row between `in <context>` and `hint: ...` when
  the field is set. `display_with_source` auto-populates it from the
  primary span's source line (non-breaking: any existing diagnostic
  rendered with source now gains an inline `here:` row for free).
  `to_json` emits `"here":` and `"try":` fields (null when unset).
  8 test cases in `tests/here_snippet_test.rs`. `Try:` / `Hint:`
  label capitalization + full migration left for Phase 3.

## Acceptance Criteria

- すべての診断が Here / Try / Hint（または Here / Hint）の形式で出力される
- `tests/diagnostics/` に少なくとも 30 のペアがあり、全て self-test が通る
- CI が Try 欄のコード片を機械適用してコンパイル検証する
- 診断コードが一意化され、ドキュメントが生成される
- 既存 snapshot ゴールデンが全て新形式に更新されている

## Non-goals

- 診断の日本語化（多言語対応は別項目）
- IDE との統合（LSP は別項目で扱う）

## Related

- [Almide Dojo](./almide-dojo.md) —— Dojo が「どの診断が LLM を誤らせたか」を検出するため、本項目と直接連携する
