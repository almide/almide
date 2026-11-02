<a id="variant-exhaustiveness-refinement"></a>

# Variant 網羅性の改善

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](variant-exhaustiveness-refinement.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](variant-exhaustiveness-refinement.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](variant-exhaustiveness-refinement.zh-CN.md)

> [日英混在の正本](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/done/variant-exhaustiveness-refinement.md)（固定コミット `35297a612fb29e3f78f6587aba253aa91d51f466`）の非規範的な読者用スナップショット。元の文書が正本であり、この写しは自動同期されず、生成器・チェッカーの入力でもない。以下の日付、計画、完了状態、測定値、比較主張は当時の原文の記述を保持したもので、独立した再現や現行版についての検証ではない。コードブロックとコメントは原文のまま保持する。過去の収集用メタデータは、動作しない読者向け注記として表示する。

> 過去の説明：非網羅 match は不足枝のコードを提案し、到達不能な枝は強制的なエラーとなる。完了メタデータ：2026-04-20。

> 原文の注意点：完了節はコンパイラ機能 4 項目と §5 の文書のみの閉鎖を区別し、コンパイラのデフォルト自動導出は延期している。旧設計、受入一覧、「まだ未解決」節を保持するが、Codec の自動導出が出荷済みという主張にはしない。ガード診断、一時的な hole、例は当時の範囲にとどまり、現行の実行手順や独立した MSR 結果ではない。

<a id="completion-status-2026-04-20"></a>

## 完了状態（2026-04-20）

一連の作業は 4 つの小節にわたり導入済み（§1–§4 はコンパイラ機能、§5 は文書レベル）：

- **§1 貼付け可能な不足枝**（2026-04-19）— `MissingArm` が短い witness pattern（`Node(_, _)`）と、そのまま貼れる枝テンプレートの両方を持つ。E010 hint はテンプレートブロックを出力する。
- **§2 到達不能 → エラー**（2026-04-20）— `find_unreachable_arms` による Maranget §3 usefulness 検査。各死んだ枝が E011 を発する。Opaque/Infinite の列型は default-matrix に回し、ジェネリックで偽陽性を出さない。
- **§3 入れ子の網羅性**（2026-04-20）— 検出は元から正しかった（Maranget は再帰的）。`fmt_arm_head` を再帰的にし、入れ子の witness を `Node(Node(arg1, arg2), Leaf) => _` と出力するようにした。
- **§4 guard の全域性注記**（2026-04-20）— どれかの枝に guard があると、E010 に「ガード付き枝は網羅性に数えない — guard は実行時に失敗しうる」を追加し、`X if cond => ...` が X を覆わなかった本当の理由を示す。
- **§5 Variant Codec 自動導出**（2026-04-20）— 文書レベルで範囲を閉じた。規約を `docs/CHEATSHEET.md`（「variant serialization — recommended pattern」）に明文化した。コンパイラレベルのデフォルト自動導出は延期した。コスト（stdlib とユーザーコードの variant 型ごとに fn が 2 個増える）と影響範囲（既存 Codec decode 経路には、単純でない payload に対して `"payload decode not yet implemented"` stub が残る）が、文書の一例ですでに `deriving Codec` を学ぶ LLM の MSR 上の利益を上回るためである。明示 derive 形式で LLM の逸脱が dojo MSR データに現れたら、別の一連の作業として範囲を再設定する。

<a id="motivation"></a>

## 動機

パターンマッチの網羅性チェックは既に動作しているが、**エラーメッセージが「不足枝の名前」止まり** で、LLM が自動修復するには情報が足りない。また、到達不能な枝は現状警告レベルだが、**到達不能な枝は LLM の生成誤りの直接のサイン** であり、エラーで返すべき。

本項目は「書いた LLM がそのままコピペして通る」レベルまでエラーメッセージを磨き、修正生存率を直接押し上げることを狙う。

<a id="current-state"></a>

## 現状

```
Non-exhaustive match at line 14:3
  Missing: Node
```

LLM はこれを受け取っても、コンストラクタの arity やフィールド名を覚えていない可能性がある。結果として再度失敗する。

<a id="design"></a>

## 設計

<a id="1-paste-ready-missing-arms"></a>

### 1. 貼付け可能な不足枝

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

<a id="2-unreachable-arms-become-errors"></a>

### 2. 到達不能な枝をエラーにする

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

<a id="3-nested-exhaustiveness"></a>

### 3. 入れ子の網羅性

ネストした Variant に対する網羅性：

```
match tree {
  Node(Leaf(_), _) => ...,
  // Missing: Node(Node(_,_), _)
}
```

現状、ネストの外層だけ見て網羅判定しているなら、内層まで追跡する。

<a id="4-guard-aware-exhaustiveness"></a>

### 4. ガードを考慮した網羅性

ガード付きの枝は **網羅性の計算から除外** する。`if x > 0` ガードがあれば、その枝は「確実にマッチする」とは言えない。ガードのある枝のあとには `_ =>` を必須化するエラーを出す：

```
Guard at line 17 is not total
  Hint: guarded arms never contribute to exhaustiveness. Add a `_ => ...` arm.
```

<a id="5-auto-derived-codec-for-variants"></a>

### 5. Variant の自動導出 Codec

（関連）既存の Codec 規約（`Type.decode` / `Type.encode`）を variant にも均質に適用する。手書きの serialize は LLM の事故源なので、**デフォルトで自動導出**、必要なら `#[codec(manual)]` で opt-out。

<a id="acceptance-criteria"></a>

## 受入基準

- 非網羅エラーが不足枝のコード片をそのまま提示する
- 到達不能な枝がエラーになる
- ネストした Variant の網羅性が検出される
- ガード付き枝の後に `_ =>` が無ければエラー
- Variant の Codec がデフォルトで自動導出される
- 既存 snapshot テストが更新され、診断のゴールデンファイルが新形式に揃う

<a id="progress"></a>

## 進捗

<a id="2026-04-19--1-paste-ready-missing-arms"></a>

### 2026-04-19 — §1 貼付け可能な不足枝

`check_exhaustiveness` は、短い witness pattern（`Node(_, _, _)`）と貼付け可能な枝テンプレート（`Node(arg1, arg2, arg3) => _`）の両方を持つ `Vec<MissingArm>` を返すようになった。E010 診断 hint は後者を使う。tuple variant には位置に応じた `argN` 束縛、record variant には宣言済みフィールド名、`some/ok/err` には `x` / `e` を使う。テストは `tests/exhaustiveness_hint_test.rs`（tuple variant、Option 不足枝、unit variant、Int の catch-all の 4 ケース）。

<a id="2026-04-20--2-unreachable--error"></a>

### 2026-04-20 — §2 到達不能 → エラー

`find_unreachable_arms`（Maranget §3 usefulness）は、先行枝に完全に覆われるパターンの枝を検出する。各死んだ枝は本体 span に `E011 unreachable match arm` を発し、絞り込み hint を付ける。ガード付き枝は shadowing の両方向でスキップする。後続枝を覆わず、それ自身も覆われたものとされない。

Opaque / Infinite の列型（TypeVar、Int、Float、String）は constructor-enumeration 分岐でなく default-matrix 分岐に回すため、`map_option[A, B]` のようなジェネリック fn は引き続きコンパイルできる。ctor-enum 分岐では、空の `[A]` ctor 集合に対し `some(v)` を死んだ枝と誤報してしまう。

<a id="2026-04-20--4-guard-totality-note"></a>

### 2026-04-20 — §4 guard の全域性注記

match にガード付き枝があるとき、E010 は「ガード付き枝（`pat if cond =>`）は網羅性に数えない — guard は実行時に失敗しうる」と追加する。このメッセージは、LLM の `X if cond => ...` 枝がパターン X を覆わなかった理由を明確にする。

<a id="2026-04-20--3-nested-exhaustiveness-arm-template"></a>

### 2026-04-20 — §3 入れ子の網羅性（枝テンプレート）

入れ子の未被覆パターンの検出はすでに動いていた。Maranget の usefulness アルゴリズムは部分パターンに対して再帰する。穴は `fmt_arm_template` formatter にあった。`field_names` を使い、内部 ctor の構造によらず常に位置に応じた `argN` 名を生成していた。今は `fmt_arm_head` が各引数を再帰的に処理する：

- `Pat::Wild` → 束縛名（ファイルスコープのカウンター由来の `argN`、または record フィールド名／Option/Result 単一フィールドの `x`）。
- `Pat::Ctor(c, args)` → 再帰呼出しによる入れ子の `c(...)`。

`type Tree = | Leaf | Node(Tree, Tree)` で `Leaf` と `Node(Leaf, _) => ...` だけを処理した結果：

```
hint: add arms for Node(Node(_, _), Leaf), Node(Node(Leaf, Leaf), Node(_, _)), Node(Node(Node(_, _), Leaf), Node(_, _)):
  Node(Node(arg1, arg2), Leaf) => _
  Node(Node(Leaf, Leaf), Node(arg1, arg2)) => _
  Node(Node(Node(arg1, arg2), Leaf), Node(arg3, arg4)) => _
```

`argN` カウンターは枝テンプレートごとにリセットし、貼付けた中で束縛が偶然一致しないようにする。各枝が独立なので、これは意図的である。

<a id="still-open"></a>

### まだ未解決

- **§5 Variant Codec 自動導出** — 既存の `derive Codec` 機構を拡張し、variant の encode / decode をデフォルトで生成する。自動導出 codegen パイプラインに手を入れる必要があり、別の一連の作業規模になる。

<a id="dependencies"></a>

## 依存関係

- なし（型チェッカと診断の局所的変更で完結）

<a id="risk"></a>

## リスク

- 到達不能→エラー化は既存コードを壊す可能性。別フラグ（`--strict-unreachable`）を経て段階移行する選択肢を検討
