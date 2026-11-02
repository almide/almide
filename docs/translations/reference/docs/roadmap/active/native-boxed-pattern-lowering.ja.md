<a id="native-nested-ctorliteral-pattern-at-a-boxd-recursive-variant-field"></a>

# native：Box 化された（再帰 variant）フィールドでの入れ子 ctor／リテラルパターン

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](native-boxed-pattern-lowering.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](native-boxed-pattern-lowering.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](native-boxed-pattern-lowering.zh-CN.md)

> 非規範の固定読者用スナップショット。原文を正本とし、自動同期せず、実装・実測・安全性を独立に再検証したものではない。
> [Source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/native-boxed-pattern-lowering.md)
> 冒頭の実装済みという更新を、以下に残る「実装準備完了」という過去の診断より優先して読む。古い失敗、修正案、低リスク評価は履歴であり、新たな再現や再実装の指示ではない。


> 原文の説明：入れ子の boxed-pattern に関する過去の診断。実装済みの native lowering と回帰の参照先

<a id="implemented-mechanism-original-diagnosis-below"></a>

## 実装済みの機構；以下は当初の診断

ここで述べる native の入れ子 box パターン機構は、[statements_box_patterns.rs](../../../../../../crates/almide-codegen/src/walker/statements_box_patterns.rs) と [pass_box_deref.rs](../../../../../../crates/almide-codegen/src/pass_box_deref.rs) に実装済みで、後に入れ子パターン／キャプチャの改良（#757/#3174）が加わった。[入れ子 variant のフィクスチャ](../../../../../../spec/wasm_cross/nested_variant_tag_box.almd) が回帰例を記録している。以下の古い「実装準備完了」という状態とコンパイル失敗は、当初の #610 の診断であり、新たな再現やパスを再実装せよという指示ではない。

状態：active — 分析済み、実装準備完了（#610）
担当：compiler / codegen
対象：NATIVE（Rust）ターゲットのみ。wasm はすでに正しい（control.rs #607 が入れ子の boxed パターンをタグガード＋入れ子 match に組み替える）。

<a id="symptom"></a>

## 症状

```almide
type Tree = | Leaf(Int) | Node(Tree, Tree)
fn sum(t: Tree) -> Int = match t {
  Node(Leaf(a), Leaf(b)) => a + b,   // nested ctor at a Box'd field
  Node(l, r)             => sum(l) + sum(r),
  Leaf(n)                => n,
}
```

- wasm：正しい（`6`）。
- native：出力された Rust がコンパイルできない。結びついた二つの欠陥が、どちらも次の行にある：
  `Tree::Node(Tree::Leaf(a), Tree::Leaf(b)) => ((*a) + (*b))`。

  - **(a) E0614 不要な逆参照** — `a:Int` は box 化されていないのに `(*a)`/`(*b)` がある。`pass_box_deref.rs::collect_bind_vars`（約 450 行）は入れ子の `Constructor` の引数を無条件に再帰処理し、内側の束縛（`a`,`b`）を逆参照変数として印付けする。しかし逆参照すべきは直接 Box 化されたフィールド位置だけである。修正：`collect_bind_vars` は直接の `IrPattern::Bind` だけに印を付け、`Constructor { args }` の再帰を削除する（boxed フィールドの入れ子 ctor は欠陥 (b) が扱う問題で、内側の束縛は逆参照後の値であり box ポインタではない）。
  - **(b) E0308 box の不一致** — `Tree::Leaf(a)` を `Box<Tree>`（Node のフィールド）に対して直接パターン照合している。安定版 Rust には `box` パターンがない（生成 Rust は安定版で、`#![feature(box_patterns)]` は使わない）ため、boxed フィールドの入れ子 ctor／リテラルを直接照合できない。

(a) と (b) は必ず同時に取り込む必要がある。(a) だけ直すと問題を覆い隠していた逆参照が消えて E0308 が残り、(b) だけ直すと E0614 が残る。

<a id="fix-for-b-mirror-pass_pattern_literal_guardrs-for-ctors"></a>

## (b) の修正：ctor 向けに `pass_pattern_literal_guard.rs` と同様の処理を行う

`pass_pattern_literal_guard.rs` は、入れ子の STRING リテラルに関する同様の問題をすでに解決している（Rust 専用パス）。ペイロード内の `Literal{LitStr}` を新しい `Bind __lit_N` に置き換え、分岐ガードへ `__lit_N == "..."` を AND で追加する。これによりガードが偽なら後続分岐へ進み、wasm のタグ→値チェックと同じになる。`IrMatchArm` は `guard: Option<IrExpr>` フィールドを持ち、walker はこれを `pattern if guard` として描画する。

#610 では、このパスを拡張する（または同系統のパスを追加する）。型が `ty_contains_any_recursive` を満たすフィールド（Box 描画と欠陥 (a) の位置と同じ述語。`walker::ty_contains_any_recursive` と cycle set を再利用する）にある `IrPattern::Constructor`（または Int/Bool/Float 以外の `Literal`）を次のように書き換える。

1. 入れ子パターン `Leaf(a)` を新しい `Bind __box_N` に置き換える。
2. 分岐へタグガードを AND で追加する：`match *__box_N { Leaf(_) => true, _ => false }`（Bool 型の `IrExpr` Match）。ガードが偽なら後続分岐に進み、wasm とバイト単位で同じ到達可能性を保つ。
3. 元の本体が動く前に、分岐本体内で内側の変数を再束縛する。例えば `let Leaf(a) = *__box_N else { unreachable!() }` を前置する（ガードが形をすでに証明している）。または同等の内側の `match *__box_N { Leaf(a) => …}` を用いる。内側の束縛は逆参照済みの値なので、使用箇所で再び逆参照しないために、欠陥 (a) の再帰停止の変更が必要となる。

上記 `Tree`/`sum` の形を `tests/crossmod_matrix_test.rs`（または `spec/wasm_cross/` フィクスチャ）に固定する。native と wasm は両方とも `6` を出力しなければならない。ゲートはすでに wasm をバイト単位で照合している。

<a id="why-deferred-from-the-v0276-batch"></a>

## v0.27.6 のバッチから延期した理由

native のみの問題（クロスターゲットのバイト比較ゲートには影響せず、wasm は正しい）であり、欠陥 (b) は新しい match 再構成パス（タグガードの IrExpr 生成＋内側の束縛抽出）を必要とする。複数修正のリリースに混ぜるより、集中した実装と専用テスト実行に値する。発生条件は狭く（boxed フィールドの入れ子 ctor／リテラルで、現時点ではコンパイルに失敗する形）、現在通っているプログラムの回帰リスクは低い。
