<a id="l2l3-secret"></a>

# L2/L3: secret

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](L2-secret.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](L2-secret.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](L2-secret.zh-CN.md)

> [正本](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-secret.md)を 35297a612fb29e3f78f6587aba253aa91d51f466 に固定した非規範的な読者向け翻訳です。自動同期されず、原本が引き続き権威を持ちます。安全性・セキュリティ・比較・実装・測定の主張は設計時点の原本の記述として保持し、独立した実行、時間測定、監査、認証は行っていません。現行リリースについての新しい保証ではありません。コードブロックとコメントは原文のまま、メタデータは無効化した読者向け注記です。

> 原本の説明（無効化したメタデータ）: 標準ライブラリ再設計 L2/L3 — secret: すべての renderer が Secret(***) と表示する Secret[A]、定数時間の ==、Codec は decode のみ。

> 最終更新: 2026-10-24
> 前提: [L0](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L0-conventions.md)・[L1](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L1-categories.md)（R-L1-31）・#4107
> 出典: Rust https://docs.rs/secrecy ・ Python https://docs.pydantic.dev/latest/api/types/#pydantic.types.SecretStr ・ Go https://pkg.go.dev/log/slog#LogValuer ・ Kotlin https://kotlinlang.org/docs/data-classes.html ・ Swift https://developer.apple.com/documentation/os/oslogprivacy ・ TS/JS https://nodejs.org/api/util.html#utilinspectcustom ・ Elixir https://hexdocs.pm/elixir/Inspect.html ・ https://hexdocs.pm/ecto/Ecto.Schema.html#field/3 ・ Java https://docs.oracle.com/en/java/javase/21/docs/api/java.base/java/lang/Record.html#toString()
> 実測: `almide 0.68.0 (dev, fcb2e6827)`、native と `--target wasm`。

<a id="l2-役割の比較"></a>

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

<a id="完全性の規則"></a>

### 完全性の規則

1. `Secret[A]` に入る道は `secret.new`、出る道は `secret.reveal` の 2 つだけです。モジュールの外ではフィールドの読み出しとレコードリテラルが E033 です。
2. 表示は 1 か所の宣言（`Secret.repr`）から全経路に届きます: 埋め込み `"${…}"`、外側のレコード・リスト・Option・Result・タプル・Map の中、`assert_eq` / `assert_ne` の失敗ブロック、abort のメッセージ、`almide test` の報告。どの経路も `Secret(***)` です。
3. `==` は `Secret[String]` と `Secret[Bytes]` だけが持ち、定数時間で比べます。ほかの `A` の `Secret`、`Secret` を持つ variant・Map は `==` を持ちません（E103）。
4. Codec: `Secret[A]` のフィールドは A の形で decode され、encode は導出しません（E023）。`Secret[Bytes]` は `Bytes` と同じく E023 です。
5. **意図した抜け**: Hash・Ord（Map のキー・並べ替えのキー）は持たせません。`map` / `set` の中身表示や比較の有無が観測できること自体を避けます（いまは型として拒否まではしていません）。

<a id="l2-の判定"></a>

### L2 の判定

- 抜けている役割（5 言語以上）はありません。専用の型は 2 言語ですが、#4107 の「1 つの宣言からすべての表示経路を導く」要求に応える形は型だけです（フィールド属性は 1 つのレコードの 1 つの表示経路しか守れません）。
- 実測: `Login { user: "u", pw: secret.new("CANARY") }` の `"${l}"`、`[l]`、`some(s)`、`(1, s)`、variant の payload、`map.from_list([("k", s)])` は、native・wasm とも `Secret(***)` を含み、カナリアを含みませんでした。

<a id="案"></a>

## 案

<a id="r-secret-01-secretnew-と-secretreveal-を足す---採用"></a>

### R-secret-01 `secret.new` と `secret.reveal` を足す — ○ 採用

- 内容: `fn secret.new[A](x: A) -> Secret[A]`、`fn secret.reveal[A](s: Secret[A]) -> A`。`import secret` が要ります（自動 import しない）。
- 根拠の L0 規則: R-L0-00（2/8 の型の形）、R-L0-03（主語は先頭）、R-L0-30（要素の値は `x`。主語の名前は `s`）。
- 綴り: 主語のない構築子は `new`（R-L0-02。`map.new`・`regex.new` と同じ）。取り出しに多数派はありません（Rust secrecy `expose_secret`、pydantic `get_secret_value`、Go・Swift・Kotlin は持たない）。`to_string` は `Repr`（R-L0-38）の名前で、Secret では `Secret(***)` を返すので使えません（`html.to_string` は `SafeHtml` の中身を返しますが、SafeHtml は隠す型ではありません）。`unwrap` は Almide にありません（`??` に一本化、R-L0-06）。中身を出す箇所が grep で見つかる、ほかの意味を持たない動詞として `reveal` にします。
- protocol（R-L0-37）: `Repr`（常に `Secret(***)`）、`Eq` は `A` が `String`・`Bytes` のときだけ（条件つきの準拠。それ以外は E103）、`Hash`・`Ord` なし、`Codec` は decode の半分だけ。[protocol-view](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/protocol-view.md) の型の表に載せるのは、条件つきの準拠が言語に入ってからです（想定設計）。
- (b) 純粋です。両 target で同じです。
- (c) `secret` という名前のローカル変数がよく書かれるので、自動 import にすると衝突します。import 行が見える opt-in です。
- issue: #4107

<a id="r-secret-02-表示は-secret宣言-1-か所---採用"></a>

### R-secret-02 表示は `Secret(***)`、宣言 1 か所 — ○ 採用

- 内容: 型の `repr` 宣言（`Secret.repr`）を、埋め込み・入れ物の中・assert / abort・テスト報告のすべてが読みます。型が repr を宣言していない値は、これまでどおり構造のまま表示します。
- issue: #4107

<a id="r-secret-03--は定数時間string--bytes-以外は-e103---採用"></a>

### R-secret-03 `==` は定数時間、String / Bytes 以外は E103 — ○ 採用

- 内容: `Secret[String]` / `Secret[Bytes]` の `==` / `!=` は `string.is_equal_constant_time` / `bytes.is_equal_constant_time` で比べます。レコード・タプル・リスト・Option・Result の中の `Secret` も同じです。
- issue: #4107（#4106 の定数時間比較を使う）

<a id="r-secret-04-codec-は-decode-だけ導出encode-は-e023---採用"></a>

### R-secret-04 Codec は decode だけ導出、encode は E023 — ○ 採用

- 内容: Rust secrecy の「拒否」の列を採ります。伏せ字で書き出す（pydantic）と、データベースや外向きの要求に `***` が黙って入り、原因から遠い所で壊れます。
- issue: #4107

<a id="l3-関数表"></a>

## L3: 関数表

| 関数 | シグネチャ(現行, mut 表示) | 他言語の主な綴り | 判定 | 案 |
|---|---|---|---|---|
| `new` | `secret.new[A](x: A) -> Secret[A]` | Rust `SecretBox::new` / pydantic `SecretStr(x)` | 追加 | R-secret-01 |
| `reveal` | `secret.reveal[A](s: Secret[A]) -> A` | Rust `expose_secret()` / pydantic `get_secret_value()` | 追加 | R-secret-01 |

## 原本に関する注意

原本には採択済みの追加、カナリア実測の報告、想定段階の条件付き準拠設計が併存しています。Hash/Ord に関わる用途をまだ型として拒否していないという明記を保持します。表示の秘匿と定数時間比較は固定した設計記録の主張であり、新しいセキュリティ認証、独立したタイミング解析、あらゆる情報漏えい経路への保証ではありません。
