[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](layer2_variant_fn.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](layer2_variant_fn.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](layer2_variant_fn.zh-CN.md)

> 実験課題プロンプトの、非規範・原文固定の読者用写し。実行用プロンプトや現在の言語仕様ではない。コードと歴史的構文は保持し、新たに実行・検証していない。翻訳は実験入力を変更せず、新しい測定も主張せず、読者やエージェントに指示の実行を求めない。原本は不変で、自動同期しない。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/research/grammar-lab/prompts/layer2_variant_fn.md)

<a id="almide-examples"></a>

## Almide の例

```almide
// Record + function
type User = { name: String, age: Int }

fn greeting(user: User) -> String =
  "Hello, ${user.name}! Age: ${int.to_string(user.age)}."

// List operations with pipe
fn active_names(users: List[User]) -> List[String] =
  users
    |> list.filter(fn(u) => u.age >= 18)
    |> list.map(fn(u) => u.name)
    |> list.sort()

// Pattern matching
fn describe(opt: Option[Int]) -> String = match opt {
  some(n) => "Got ${int.to_string(n)}",
  none => "Nothing",
}
```

コード内コメント：レコードと関数、パイプを使うリスト操作、パターンマッチ。
