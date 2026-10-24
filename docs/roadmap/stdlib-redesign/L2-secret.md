<!-- description: Stdlib redesign L2/L3 — secret: Secret[A] that every renderer prints as Secret(***), constant-time ==, Codec decode only -->
# L2/L3: secret

> Last updated: 2026-10-24
> 前提: [L0](L0-conventions.md)・[L1](L1-categories.md)（R-L1-31）・#4107
> 出典: Rust https://docs.rs/secrecy ・ Python https://docs.pydantic.dev/latest/api/types/#pydantic.types.SecretStr ・ Go https://pkg.go.dev/log/slog#LogValuer ・ Kotlin https://kotlinlang.org/docs/data-classes.html ・ Swift https://developer.apple.com/documentation/os/oslogprivacy ・ TS/JS https://nodejs.org/api/util.html#utilinspectcustom ・ Elixir https://hexdocs.pm/elixir/Inspect.html ・ https://hexdocs.pm/ecto/Ecto.Schema.html#field/3 ・ Java https://docs.oracle.com/en/java/javase/21/docs/api/java.base/java/lang/Record.html#toString()
> 実測: `almide 0.68.0 (dev, fcb2e6827)`、native と `--target wasm`。

## L2: 役割の比較

| 役割 | Almide | Rust (secrecy) | Python (pydantic) | Go | Kotlin | Swift | TS/JS | Elixir | Java |
|---|---|---|---|---|---|---|---|---|---|
| 秘密を包む型 | `Secret[A]`、`secret.new` | `SecretBox<T>` | `SecretStr`、`Secret[T]` | —（`String()` を手で書く） | —（`toString` を手で書く） | —（`os.Logger` の `<private>`） | — | —（`@derive {Inspect, except:}`） | — |
| 中身を取り出す | `secret.reveal` | `expose_secret()` | `get_secret_value()` | — | — | `privacy: .public` | — | フィールドを読む | — |
| 表示 | 常に `Secret(***)` | `[REDACTED]` | `'**********'` | 手書き | 手書き | `<private>` | 手書き | `**redacted**` | 手書き |
| 書き出し（encode） | 拒否（E023） | 拒否（`SerializableSecret` が要る） | 伏せ字で書く | そのまま書く | そのまま書く | そのまま書く | `toJSON` 次第 | 別設定 | そのまま書く |
| 読み込み（decode） | 導出する | 導出する | 導出する | — | — | — | — | — | — |
| `==` | 定数時間（String / Bytes）、ほかは E103 | 無い | 値の比較 | — | — | — | — | — | — |

レコードを既定で全フィールド表示するのは 8/8 です。伏せるのはどれも明示の選択です。専用の型は 2/8（Rust secrecy・pydantic）で、どちらも中身を暗黙には外へ出しません。

### 完全性の規則

1. `Secret[A]` に入る道は `secret.new`、出る道は `secret.reveal` の 2 つだけです。モジュールの外ではフィールドの読み出しとレコードリテラルが E033 です。
2. 表示は 1 か所の宣言（`Secret.repr`）から全経路に届きます: 埋め込み `"${…}"`、外側のレコード・リスト・Option・Result・タプル・Map の中、`assert_eq` / `assert_ne` の失敗ブロック、abort のメッセージ、`almide test` の報告。どの経路も `Secret(***)` です。
3. `==` は `Secret[String]` と `Secret[Bytes]` だけが持ち、定数時間で比べます。ほかの `A` の `Secret`、`Secret` を持つ variant・Map は `==` を持ちません（E103）。
4. Codec: `Secret[A]` のフィールドは A の形で decode され、encode は導出しません（E023）。`Secret[Bytes]` は `Bytes` と同じく E023 です。
5. **意図した抜け**: Hash・Ord（Map のキー・並べ替えのキー）は持たせません。`map` / `set` の中身表示や比較の有無が観測できること自体を避けます（いまは型として拒否まではしていません）。

### L2 の判定

- 抜けている役割（5 言語以上）はありません。専用の型は 2 言語ですが、#4107 の「1 つの宣言からすべての表示経路を導く」要求に応える形は型だけです（フィールド属性は 1 つのレコードの 1 つの表示経路しか守れません）。
- 実測: `Login { user: "u", pw: secret.new("CANARY") }` の `"${l}"`、`[l]`、`some(s)`、`(1, s)`、variant の payload、`map.from_list([("k", s)])` は、native・wasm とも `Secret(***)` を含み、カナリアを含みませんでした。

## 案

### R-secret-01 `secret.new` と `secret.reveal` を足す — ○ 採用

- 内容: `fn secret.new[A](x: A) -> Secret[A]`、`fn secret.reveal[A](s: Secret[A]) -> A`。`import secret` が要ります（自動 import しない）。
- 根拠の L0 規則: R-L0-00（2/8 の型の形）、R-L0-03（主語は先頭）。
- (b) 純粋です。両 target で同じです。
- (c) `secret` という名前のローカル変数がよく書かれるので、自動 import にすると衝突します。import 行が見える opt-in です。
- issue: #4107

### R-secret-02 表示は `Secret(***)`、宣言 1 か所 — ○ 採用

- 内容: 型の `repr` 宣言（`Secret.repr`）を、埋め込み・入れ物の中・assert / abort・テスト報告のすべてが読みます。型が repr を宣言していない値は、これまでどおり構造のまま表示します。
- issue: #4107

### R-secret-03 `==` は定数時間、String / Bytes 以外は E103 — ○ 採用

- 内容: `Secret[String]` / `Secret[Bytes]` の `==` / `!=` は `string.is_equal_constant_time` / `bytes.is_equal_constant_time` で比べます。レコード・タプル・リスト・Option・Result の中の `Secret` も同じです。
- issue: #4107（#4106 の定数時間比較を使う）

### R-secret-04 Codec は decode だけ導出、encode は E023 — ○ 採用

- 内容: Rust secrecy の「拒否」の列を採ります。伏せ字で書き出す（pydantic）と、データベースや外向きの要求に `***` が黙って入り、原因から遠い所で壊れます。
- issue: #4107

## L3: 関数表

| 関数 | シグネチャ(現行, mut 表示) | 他言語の主な綴り | 判定 | 案 |
|---|---|---|---|---|
| `new` | `secret.new[A](x: A) -> Secret[A]` | Rust `SecretBox::new` / pydantic `SecretStr(x)` | 追加 | R-secret-01 |
| `reveal` | `secret.reveal[A](s: Secret[A]) -> A` | Rust `expose_secret()` / pydantic `get_secret_value()` | 追加 | R-secret-01 |
