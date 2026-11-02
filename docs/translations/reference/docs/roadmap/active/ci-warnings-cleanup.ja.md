# CI 警告の整理

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](ci-warnings-cleanup.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](ci-warnings-cleanup.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](ci-warnings-cleanup.zh-CN.md)



> 原文：[docs/roadmap/active/ci-warnings-cleanup.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/ci-warnings-cleanup.md)、固定コミット `35297a612fb29e3f78f6587aba253aa91d51f466`。過去のロードマップの非規範的な読者向けスナップショット訳で、自動同期しない。日付・状態・見積り・計測・計画は原記録のもので、新たな実装や独立検証を示さない。過去の承認待ちや推奨コマンドは現在の実行許可ではない。原文を正とする。

> 原文の説明メタデータ：CI が許容しているコンパイラ警告の継続台帳と、各分類をゼロにする計画。

> **状態**：Active（進行中）
> **観測時点**：v0.23.10、Windows と macOS の両 CI
> **影響**：CI ログのノイズ、4 件の ICE メッセージ（致命的ではない）

## 概要

CI 出力には、spec テスト全体で約 40 件の警告と 4 件の ICE メッセージがある。全テストは合格しており、これらは失敗ではなく診断ノイズである。ただし ICE メッセージは、実際に単相化に不足があることを示す。

## 分類

### 重大：ICE — 単相化後に TypeVar が残る（4 ファイル）

```
[ICE] N TypeVar(s) remain after monomorphization. Generic params should be fully substituted.
```

| ファイル | 残存 TypeVar 数 |
|------|--------------------|
| `spec/integration/codegen_patterns_test.almd` | 2 |
| `spec/lang/generics_test.almd` | 3 |
| `spec/lang/regression_v0_11_test.almd` | 6 |
| `spec/lang/type_system_test.almd` | 9 |

**根本原因**：ユーザー定義のジェネリック関数（`first[T]`、`zip_with[A,B,C]`、`unwrap_or[T]`）が完全には単相化されていない。mono パスがすべての型パラメーターを置換していない。

**関連**：`spec/lang/function_test.almd` にも残存 TypeVar が 1 つある（ジェネリックな階乗か？）。

### 重大：POSTCONDITION VIOLATION — ラムダ引数が未解決（1 ファイル）

```
[POSTCONDITION VIOLATION] Lambda param VarId(30) still unresolved: ir=Unknown vt=Unknown
[POSTCONDITION VIOLATION] [ConcretizeTypes] 3 expressions remain with unresolved types.
```

ファイル：`spec/lang/single_quote_test.almd` またはその近辺（関数 `__test_almd_override`）

**根本原因**：引数型が Unknown のラムダが ConcretizeTypes 後も残る。上書き／シャドーイングのパターンが型解決を混乱させている可能性がある。

### ノイズ：未使用変数警告（約 25 件）

spec テストファイルでは副作用だけのために変数を使う（例：`for (k, v) in m { ... }` で `v` だけを使う）。

**修正案**：spec ファイルで `_` を前置するか、テストモードで抑制する。

### ノイズ：未使用インポート警告（4 件）

- block_comment_raw_string_test の `value` インポート
- stdlib テストの `fs`、`env`、`process` インポート

**修正案**：未使用インポートを削除するか、`_` を前置する。

### ノイズ：E015 — stdlib のシャドーイング警告（5 件）

```
warning[E015]: fn 'factorial' has the same signature as stdlib `math.factorial`
```

ファイル：`codegen_stress_test`、`function_test`、`generics_test`、`regression_v0_11_test`、`type_system_test`

**バグではない**：テストファイル内の意図的な再実装である。テストブロック内の E015 を抑制できるかもしれない。

## 推奨アクション

1. **ICE の修正**：ユーザー定義ジェネリクスの mono パスを調査する。TypeVar 置換が不完全。
2. **POSTCONDITION の修正**：上書きパターンでのラムダ引数型解決。
3. **テストノイズの抑制**：未使用変数への `_` 前置、未使用インポートの削除。
4. **検討事項**：CI テスト手順用の `--quiet-warnings` フラグ、または `test` ブロック内での E015 抑制。
