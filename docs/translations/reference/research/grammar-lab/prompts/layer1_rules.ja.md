[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](layer1_rules.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](layer1_rules.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](layer1_rules.zh-CN.md)

> 実験課題プロンプトの、非規範・原文固定の読者用写し。実行用プロンプトや現在の言語仕様ではない。コードと歴史的構文は保持し、新たに実行・検証していない。翻訳は実験入力を変更せず、新しい測定も主張せず、読者やエージェントに指示の実行を求めない。原本は不変で、自動同期しない。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/research/grammar-lab/prompts/layer1_rules.md)

<a id="almide-rules"></a>

## Almide の規則
- `return` はない。最後の式が値となる。
- `let` は文であり、式にはならない（`let ... in` はない）。ローカル束縛はブロック `{ }` に置く。
  ブロックの最後の行が値となる:
    fn area(w: Int, h: Int) -> Int = {
      let s = w * h
      s + 1
    }
- `if` は `then` を使う: `if x > 0 then "yes" else "no"`
- `+` は文字列とリストを連結する: `"a" + "b"`、`xs + [y]`
- ラムダ: `(x) => x + 1`。引数でペアを分解する: `((a, b)) => a + b`
- パイプ: `xs |> list.map((x) => x * 2) |> list.filter((x) => x > 0)`
- リストは不変。`xs[i]` は要素を読む。`list.take`、`list.drop`、`list.len`、`list.sort_by`
- リストパターンを使う `match`: `match xs { [] => 0, [h, ..rest] => h }`
- null はない。`Option[T]` の `some(v)` / `none` を使い、`??` で代替値を与える。
- 等値: `==` はレコードとリストの深い等値比較。
- テスト: `test "name" { assert_eq(a, b) }`
