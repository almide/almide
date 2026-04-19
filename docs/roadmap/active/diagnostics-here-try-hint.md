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

## Progress

### 2026-04-19 — Phase 1 MVP

- `Diagnostic::here_snippet: Option<String>` + `with_here(s)` builder
  added in `almide-base`. Renderer emits a `  here: <snippet>` row
  between `in <context>` and `hint: ...` when the field is set.
- `display_with_source` auto-populates it from the primary span's
  source line (non-breaking: any existing diagnostic rendered with
  source now gains an inline `here:` row for free).
- `to_json` emits `"here":` and `"try":` fields (null when unset).
- 8 test cases in `tests/here_snippet_test.rs`.

### 2026-04-20 — Phase 2 harness

- `tests/diagnostics/<case>/` fixture structure defined. Each case has
  `broken.almd` (fails to compile), `fixed.almd` (compiles cleanly),
  and optional `meta.toml` declaring `expects_code` / `expects_error`
  / `hint_substring`.
- `tests/diagnostic_harness_test.rs` runs `almide check` on every
  case and enforces: (a) every case has broken+fixed, (b) broken
  produces the expected diagnostic, (c) fixed compiles cleanly.
- Seed fixtures: `bang-not`, `int-from-string`, `non-exhaustive-match`,
  `arity-mismatch`. Target for Phase 2 closing: 30 fixtures covering
  the full set of codes currently using `with_code(...)`.

### 2026-04-20 — Phase 3 foundations

Machinery for mechanically-applicable `Try:` snippets landed; no
diagnostic emits the new field yet (that's the Phase 3 body work).

- `Diagnostic::try_replace_span: Option<(line, col, end_col)>` —
  1-indexed, end-exclusive range the `try_snippet` is a drop-in
  replacement for.
- `Diagnostic::with_try_replace(line, col, end_col, snippet)` builder
  — sets both `try_snippet` and `try_replace_span` atomically.
- `Diagnostic::apply_try_to(source: &str) -> Option<String>` — byte-
  accurate char-indexed rewrite of `source` at the stored range; 8
  unit tests in `crates/almide-base/src/diagnostic.rs` cover the
  bang-replace / token-rename / multi-line / zero-width-insert /
  out-of-bounds cases.
- `diagnostic_render::to_json` emits `"try_replace":{line,col,end_col}`
  alongside the existing `try` string.
- `tests/diagnostic_harness_test.rs` gained `try_snippets_with_replace_span_apply_cleanly`:
  for every fixture, any diagnostic with both `try` + `try_replace`
  fields auto-rewrites `broken.almd`, compiles the result, and
  compares against `fixed.almd` (whitespace-normalised). The test
  is a no-op right now — nothing in the frontend populates
  `try_replace_span` — and acts as the regression gate for the
  upcoming per-diagnostic migrations.

**Next sub-arc (Phase 3 body):** migrate individual diagnostics to
`with_try_replace`, starting with the cases that already have a
precise name-token span at the emission site. The E002 rename path
(`string.length` → `string.len`) is the intended first target; it's
blocked on the parser exposing the full callee span (currently Member
exprs record only the `.` token, not `object.field`).

### 2026-04-20 — Phase 3 first migration (E002 stdlib alias rename)

- **Parser**: `ExprKind::Member`'s span now covers the full
  `object.field` range (from `object.span.col` to the field token's
  `end_col`) instead of just the `.` token. Multi-line members (rare —
  Almide disallows `.` across newlines) fall back to the field span.
- **Checker**: `check_named_call_spanned` accepts the callee's span
  via `callee_span_hint`; `check_call_with_type_args` threads this for
  both `Ident` and `Member` call shapes.
- **E002 emission**: `rich_snippet` still goes to `with_try` (display-
  only). Clean `fix_name` suggestions now go through `with_try_replace`
  with the callee span — `string.length` → `string.len` is now a
  mechanically-applicable fix.
- **Fixture**: `tests/diagnostics/string-length-alias/` pairs
  `broken.almd` (`string.length`) with `fixed.almd` (`string.len`).
  The `try_snippets_with_replace_span_apply_cleanly` harness test
  actively fires on this fixture — rewrites `broken.almd` via
  `apply_try_to`, compiles the result, and asserts it matches
  `fixed.almd`. End-to-end proof that a compiler-emitted Try snippet
  round-trips through the apply machinery.

**Remaining Phase 3 work:** every other `with_try(...)` call site
(12 remaining) that can be shown to round-trip. Rich multi-line
snippets (JDN sqrt conversion, operator suggestions) stay
display-only and document why.

### 2026-04-20 — Phase 3 second migration (E003 undefined variable)

E003 covers two shapes, both now mechanically-applicable:

- **Typo** (`y` where `x` is in scope): `try_replace` targets the
  offending Ident's `current_span`; the snippet is the suggested
  name. Fixture `tests/diagnostics/e003-typo-var/`.
- **Missing import** (`json.stringify(...)` without `import json`):
  zero-width `try_replace` at `(line: 1, col: 1, end_col: 1)` with
  snippet `"import <module>\n"`. `apply_try_to` handles `end_col ==
  col` as an insertion point. Fixture `tests/diagnostics/e003-missing-import/`.

### Remaining `with_try` sites (classified display-only)

Audit of the 11 surviving call sites — why each stays on `with_try`
rather than migrating to `with_try_replace`:

| Site | Class | Why display-only |
|------|-------|------------------|
| `solving.rs:45` (E001 Unit-leak) | Multi-line comment | Replacement goes at fn-body tail; span is structural (AST-level), not a contiguous source range. Comment-first snippet is the right format. |
| `calls.rs:171` (E002 method-UFCS) | Whole-expression rewrite | `x.to_uppercase()` → `string.to_upper(x)` requires capturing the object's source text, not just the method-name range. Deferred until source-extraction utility lands. |
| `calls.rs:364` (E002 rich snippet) | Multi-line wrapper | JDN sqrt, operator-replacement — conversion wrappers, not renames. |
| `calls.rs:374` (E002 fix_name fallback) | Safety net | Reached only when `callee_span_hint` is absent. Kept as backup. |
| `calls.rs:413` (E004 arity mismatch) | Placeholder snippet | `add(<x: Int>, <y: Int>)` — placeholders aren't valid Almide. |
| `infer.rs:238` (E013 no-field) | Whole-expression | Same rewrite-shape as E002 method-UFCS. |
| `infer.rs:878` (E009 immutable-reassign) | Structural fix | `let x` → `var x` requires rewriting the original binding site, not the assignment site. Cross-statement range. |
| `statements.rs:56` (let-rec) | Illustrative example | `let rec` is parser-level; snippet teaches the canonical form. Not a replacement for the broken code. |
| `statements.rs:128` (let-in) | Illustrative example | Same shape as let-rec. |
| `primary.rs:308` (while-do-done) | Illustrative example | Same shape. |
| `diagnostic.rs:291` | Test fixture | `with_try("fix")` in a unit test. N/A. |

The `with_try` API stays as the display-only escape hatch — eight
current call sites (five if you exclude the three parser "teach the
canonical form" snippets) are legitimately on it, and that's fine.

### Phase 3 status

Mechanical-apply path lands for E002 (rename-alias) + E003 (typo /
missing-import). Harness `try_snippets_with_replace_span_apply_cleanly`
now exercises three fixtures (string-length-alias, e003-typo-var,
e003-missing-import) for a full compile-and-diff round-trip per case.

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
