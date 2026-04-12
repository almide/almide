<!-- description: Non-exhaustive match suggests missing arm code; unreachable arms become hard errors -->
# Variant Exhaustiveness Refinement

## Motivation

パターンマッチの網羅性チェックは既に動作しているが、**エラーメッセージが「不足枝の名前」止まり** で、LLM が自動修復するには情報が足りない。また、到達不能な枝は現状警告レベルだが、**到達不能な枝は LLM の生成誤りの直接のサイン** であり、エラーで返すべき。

本項目は「書いた LLM がそのままコピペして通る」レベルまでエラーメッセージを磨き、修正生存率を直接押し上げることを狙う。

## Current State

```
Non-exhaustive match at line 14:3
  Missing: Node
```

LLM はこれを受け取っても、コンストラクタの arity やフィールド名を覚えていない可能性がある。結果として再度失敗する。

## Design

### 1. Paste-ready missing arms

不足枝をそのまま貼れる形で提示する：

```
Non-exhaustive match at line 14:3
  Missing arms:
    Node(left, right) => _
    Empty              => _
  Hint: add the arms above, or use `_ => todo()` to compile incrementally.
```

- 各コンストラクタを **フィールド名付き** で列挙
- 本体は `_` プレースホルダ（compile-time hole）
- 「とりあえず通したい」場合のために `_ => todo()` ヒントも併記

### 2. Unreachable arms become errors

```
match tree {
  Leaf(_) => ...,
  Leaf(0) => ...,  // ← 到達不能
  Node(_, _) => ...,
}
```

現状これは警告。これをエラーにする。理由：

- LLM は警告を読まない（stdout がノイズになる）
- 到達不能な枝は **高確率で生成誤り**（直前の枝の条件を誤認識している）
- 意図的な「将来用の枝」であれば削除すべき

### 3. Nested exhaustiveness

ネストした Variant に対する網羅性：

```
match tree {
  Node(Leaf(_), _) => ...,
  // Missing: Node(Node(_,_), _)
}
```

現状、ネストの外層だけ見て網羅判定しているなら、内層まで追跡する。

### 4. Guard-aware exhaustiveness

ガード付きの枝は **網羅性の計算から除外** する。`if x > 0` ガードがあれば、その枝は「確実にマッチする」とは言えない。ガードのある枝のあとには `_ =>` を必須化するエラーを出す：

```
Guard at line 17 is not total
  Hint: guarded arms never contribute to exhaustiveness. Add a `_ => ...` arm.
```

### 5. Auto-derived Codec for variants

（関連）既存の Codec 規約（`Type.decode` / `Type.encode`）を variant にも均質に適用する。手書きの serialize は LLM の事故源なので、**デフォルトで自動導出**、必要なら `#[codec(manual)]` で opt-out。

## Acceptance Criteria

- 非網羅エラーが不足枝のコード片をそのまま提示する
- 到達不能な枝がエラーになる
- ネストした Variant の網羅性が検出される
- ガード付き枝の後に `_ =>` が無ければエラー
- Variant の Codec がデフォルトで自動導出される
- 既存 snapshot テストが更新され、診断のゴールデンファイルが新形式に揃う

## Dependencies

- なし（型チェッカと診断の局所的変更で完結）

## Risk

- 到達不能→エラー化は既存コードを壊す可能性。別フラグ（`--strict-unreachable`）を経て段階移行する選択肢を検討
