# secret

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](secret.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](secret.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](secret.zh-CN.md)



> 原文：[docs/stdlib/secret.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/secret.md)（固定コミット `35297a612fb29e3f78f6587aba253aa91d51f466`）。非規範的な読者向けスナップショット訳。自動同期されず、独立した実測・実装検証は行っていない。API シグネチャとコード内コメントは原文のまま保持し、説明訳をコード外へ添える。 表示上の隠蔽を、暗号化・メモリ消去・あらゆる経路の漏えい防止の保証へ拡張しない。定数時間比較の長さに関する条件は string の索引に記載され、ここで独立には検証していない。

すべてのレンダラが隠す値：`Secret[A]`。`import secret` を使う（自動 import は決してされないので、それ以外の場所では `secret` というローカル名を自分で使える）。

パスワード、API キー、セッショントークンを `Secret` に入れると、すべての束縛、関数、リスト、レコードを経ても保護を維持する。補間、それを含む値の `println`、`assert_eq` / `assert_ne` の失敗ブロック、abort メッセージ、`almide test` レポートはいずれも、`A` が何であれ `Secret(***)` と表示する。値を読み出す唯一の方法は、明示的に見える `secret.reveal(s)` である。

- **不透明。** このモジュール外では Secret のフィールドを読めず、レコードリテラルも書けない（E033）。`secret.new` と `secret.reveal` が入口と出口になる。
- **等値性。** `Secret[String]` と `Secret[Bytes]` の `==` / `!=` は定数時間で比較する（`string.is_equal_constant_time` / `bytes.is_equal_constant_time`）。比較するレコード、タプル、リスト、option、result 内の Secret フィールドも含む。それ以外の型の Secret に `==` はない（E103）。`secret.reveal(a) == secret.reveal(b)` と明示して比較すること。Secret を含む variant や map にも `==` はない。
- **Codec。** `Secret[A]` フィールドを持つ `: Codec` レコードは `decode` を導出する（フィールドは A の wire 形式を読む）が、`encode` は決して導出しない。`T.encode(x)` と `json.encode(x)` は E023 になる。送信する `Value` を手で構築し、送るつもりの一つのフィールドを reveal する。`Secret[Bytes]` フィールドは `Bytes` と同じく E023 になる。

型が repr（`fn T.repr`）を宣言しない限り、値はすべて構造に沿って表示する。`Secret` は repr を宣言する標準ライブラリ型である。

すべての関数が pure なので、両ターゲットで同じ動作になる。

### `secret.new[A](x: A) -> Secret[A]`

`x` を包む。以後は `Secret(***)` と表示される。

```almd check
import secret

type Login = { user: String, password: Secret[String] }

fn main() -> Unit = {
  let l = Login { user: "ana", password: secret.new("hunter2") }
  println("${l}")
}
```

### `secret.reveal[A](s: Secret[A]) -> A`

包まれた値。秘密の読み出しは必ずこの呼び出しを通るため、レビュー（または grep）でラッパーから出るすべての場所を見つけられる。

```almd check
import secret

fn main() -> Unit = {
  let key = secret.new("k-123")
  let given = secret.new("k-123")
  println(if key == given then "accepted" else "refused")
  println("${string.len(secret.reveal(key))} characters")
}
```

<a id="decoding-into-a-secret"></a>

### Secret へのデコード

```almd check
import json
import secret

type LoginInput: Codec = { user: String, password: Secret[String] }

effect fn main() -> Unit = {
  let input = LoginInput.decode(json.parse("{\"user\":\"ana\",\"password\":\"hunter2\"}")!)!
  println("${input}")
}
```

> 原文の生成済みシグネチャ索引の固定コピー（この訳は生成対象ではない）。

<a id="signature-index-2-functions"></a>

## シグネチャ索引（2 関数）

```
// x wrapped so that every renderer prints it as `Secret(***)`.
// @since unreleased
secret.new(x: A) -> Secret[A]

// The value s wraps.
// @since unreleased
secret.reveal(s: Secret[A]) -> A
```



## 索引コメントの説明訳

- `secret.new(x: A) -> Secret[A]` — x を包み、すべてのレンダラが `Secret(***)` と表示するようにする。 (@since unreleased)
- `secret.reveal(s: Secret[A]) -> A` — s が包む値。 (@since unreleased)
