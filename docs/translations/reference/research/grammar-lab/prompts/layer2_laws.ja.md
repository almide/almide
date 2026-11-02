[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](layer2_laws.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](layer2_laws.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](layer2_laws.zh-CN.md)

> 実験課題プロンプトの、非規範・原文固定の読者用写し。実行用プロンプトや現在の言語仕様ではない。コードと歴史的構文は保持し、新たに実行・検証していない。翻訳は実験入力を変更せず、新しい測定も主張せず、読者やエージェントに指示の実行を求めない。原本は不変で、自動同期しない。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/research/grammar-lab/prompts/layer2_laws.md)

<a id="laws"></a>

## 法則
`law` ブロックは、すべての入力で成り立つべき性質を記述する:

    law "reverse twice is identity" for (xs: List[Int]) {
      list.reverse(list.reverse(xs)) == xs
    }

チェッカーは多数の入力（空リスト、ゼロ、負の数を含む）を生成し、性質が偽なら見つかった最小の反例を報告する。`where it >= 1` は入力を制限する。
法則は課題によって固定される。変更もコピーもせず、法則を満たすようにコードを変更する。
