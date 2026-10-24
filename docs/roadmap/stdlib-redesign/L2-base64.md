<!-- description: Stdlib redesign L2/L3 — base64: keep encode/decode/_url, make each decoder accept only its own alphabet so decode and decode_url stop being twins -->
# L2/L3: base64

> Last updated: 2026-10-21
> 前提: [L0](L0-conventions.md)・[L1](L1-categories.md)・対になるページ [L2-hex](L2-hex.md)
> 出典: Rust —（標準に無い。https://doc.rust-lang.org/std/ ）・ Go https://pkg.go.dev/encoding/base64 ・ Python https://docs.python.org/3/library/base64.html (F) ・ TS/JS https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/Uint8Array/fromBase64 (F) ・ Gleam https://gleam-stdlib.hexdocs.pm/gleam/bit_array.html (F) ・ Elixir https://elixir.hexdocs.pm/Base.html (F) ・ OCaml —（標準に無い。https://ocaml.org/manual/latest/api/index.html ）・ Kotlin https://kotlinlang.org/api/core/kotlin-stdlib/kotlin.io.encoding/-base64/ (F) ・ Swift https://developer.apple.com/documentation/foundation/data/base64encodedstring(options:)
> 実測: `almide 0.68.0 (dev, 437b8f127)`、native と `--target wasm`。プローブは scratchpad の `probe-url/p1.almd`。

## L2: 役割の比較

| 役割 | Almide | Rust | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin | Swift |
|---|---|---|---|---|---|---|---|---|---|---|
| 標準の英字（`+/`）で符号化 | `encode` | — | `StdEncoding.EncodeToString` | `b64encode` (F) | `toBase64()` | `base64_encode(b, padding)` (F) | `encode64` (F) | — | `Base64.Default.encode` (F) | `base64EncodedString()` |
| 標準の英字で復号 | `decode`（`-_` も受け取る） | — | `StdEncoding.DecodeString` | `b64decode` (F) | `fromBase64()`（`-_` は SyntaxError）(F) | `base64_decode` (F) | `decode64` (F) | — | `Base64.Default.decode`（`-_` は例外）(F) | `Data(base64Encoded:)` |
| URL 用の英字（`-_`）で符号化 | `encode_url`（`=` あり） | — | `URLEncoding` | `urlsafe_b64encode`（`=` あり）(F) | `toBase64({alphabet: "base64url"})` (F) | `base64_url_encode` (F) | `url_encode64` (F) | — | `Base64.UrlSafe` (F) | — |
| URL 用の英字で復号 | `decode_url`（`decode` と同じ本体） | — | `URLEncoding.DecodeString` | `urlsafe_b64decode` (F) | `fromBase64({alphabet: "base64url"})` (F) | `base64_url_decode` (F) | `url_decode64` (F) | — | `Base64.UrlSafe.decode` (F) | — |
| `=` を付けない | —（`string.replace(s, "=", "")`） | — | `RawURLEncoding` | — | `omitPadding` | `padding: False` (F) | `padding: false` (F) | — | `PaddingOption` (F) | — |

### 完全性の規則

1. 動詞は `encode` / `decode` の対だけ。英字の変種は `_<変種>` を両方に付けます（`encode_url` と `decode_url`）。hex と同じ形です（[L2-hex](L2-hex.md)）。
2. `decode_<変種>` を置くのは、変種が**英字そのものを変える**とき（base64 の `-_`）だけです。大文字・小文字のような**書き方の違い**は、無印の `decode` が全部受け取ります（hex の `encode_upper` に `decode_upper` が無いのはこのため）。
3. それぞれの `decode` は、自分の英字だけを受け取ります。他の英字の文字は err にします。`=` は encode が付ける形（長さが 4 の倍数、`=` は末尾に 2 個まで、`=` の下に隠れるビットは 0）だけを受け取ります（#4119）。
4. **意図した抜け**: `=` を付けない符号化（JWT など）は、`encode_url(b) |> string.replace("=", "")` で書けるので関数にしません。base32・base85 は 5 言語に届かないので持ちません（[facts/categories.md](facts/categories.md)）。

### L2 の判定

- 重複: `decode` と `decode_url` は同じ本体です。どちらも両方の英字を受け取ります（実測: `base64.decode("-_8A")` と `base64.decode_url("+/8A")` がどちらも `[251, 255, 0]`）→ R-base64-01。
- 綴り: `encode`/`decode`/`_url` は hex（`encode`/`decode`/`encode_upper`）と同じ形で揃っています。R-L1-23 のとおり、モジュールはまとめません。
- `=` の扱い: 符号化は `=` を付けます。復号は #4119 まで省略も、余分な `=` も、`=` の下の 0 でないビットも受け取っていました（`QQ==`・`QR==`・`QQ`・`QQ========` がすべて `A`、RFC 4648 §12 の可鍛性）。以前ここに「Python は省略を受け取る」と書いていましたが、実測では誤りです: Python 3.14 の `base64.b64decode("QQ", validate=True)` は `Incorrect padding`、`QQ========` と `QUJD=` は `Excess padding not allowed`、`=` と `====` は `Leading padding not allowed` で拒みます（受け取るのは `=` の下のビットだけで、`QR==` は `A` になります）。Go の `StdEncoding.Strict()` は省略・余分な `=`・0 でないビットのすべてを拒みます。RFC 4648 §3.2 は `=` を必須（参照する仕様が外さない限り）とし、§3.5 は 0 でないビットを拒んでよいとしています。多数派に合わせ、#4119 で `=` の付いた正規形だけを受け取るようにしました。

## 案

### R-base64-01 `decode` は `+/` だけ、`decode_url` は `-_` だけを受け取る — ○ 採用

- 内容: `base64.decode` が `-` と `_` を err にし、`base64.decode_url` が `+` と `/` を err にします。名前とシグネチャは変えません。
- 根拠の L0 規則: R-L0-01（同じ本体の 2 つの綴りを、別々の役割に分ける）、R-L0-00。定義域を狭める変更で、型と、受け取れる入力に対する答えは変わらないので、R-L0-14 の 4 の「同じ名前で別の結果」には当たりません。
- (a) 標準の復号が URL 用の英字を拒むのは Go、JS（SyntaxError）(F)、Kotlin（IllegalArgumentException）(F)、Elixir の 4 言語です。Python の `b64decode` は既定で、英字にない文字を**黙って捨てます**(F)。黙って捨てる形には寄せません。
- (b) 純粋な計算です。英字の表を 1 本の self-host 実装で引くので、両 target で同じ答えになります。
- (c) いまは `decode_url` を書いても `decode` を書いても同じ結果なので、取り違えに気づけません。分けると、取り違えたときに err で止まります。
- (d) 意味変更（定義域を狭める）。名前は残すので `@deprecated` は使いません。epoch の `breaks` には入れず、dialect epoch 15 の `notes` に書きます。拒まれた入力の err の文に「URL 用の英字は `decode_url` で」（逆向きも同じ）とヒントを入れます。使用数: `decode` 16/2/6/1、`decode_url` 4/2/5/0（spec/tests/docs/stdlib）。spec と stdlib の中に、他方の英字を渡している呼び出しがないかを、変更の前に数えます。
- ゲート案: `spec/stdlib/base64_test.almd` に、英字ごとの受け取り・拒否の 2 × 2 表を置きます。native と wasm で同じ結果になることを、wasm のテスト経路で確かめます。
- issue: #3894
- 実装: #4119 で着地しました（`=` の正規形の要求と同時）。(d) の err の文へのヒントは入れていません: 他の英字の文字は、他の不正な文字と同じ `invalid base64 character` です（両 leg で 1 つの場合に 1 つの固定の文）。

### R-base64-02 `=` を付けない符号化を足す — × 不採用

- 内容: `encode_url_raw` のような関数は足しません。
- 理由: 5 言語が持ちますが、どれも引数か設定（`padding: false`、`omitPadding`）で、別の名前ではありません。Almide で引数を足すと、`=` を付けたい既存のすべての呼び出しが型エラーになり、書き直しが要ります。`string.replace(s, "=", "")` で書けるので、その費用に見合いません（R-L0-01: 合成で書ける役割に綴りを足さない）。
- 根拠の L0 規則: R-L0-00、R-L0-01。
- issue: なし

### R-base64-03 hex とモジュールをまとめる — × 不採用（R-L1-23）

- 理由: R-L1-23 のとおりです。まとめたモジュールの名前に多数派がありません。関数の綴り（`encode`/`decode`/`encode_<変種>`）は両モジュールで揃っているので、これ以上動かしません。
- issue: なし

## L3: 関数表

| 関数 | シグネチャ(現行, mut 表示) | 他言語の主な綴り | 判定 | 案 |
|---|---|---|---|---|
| `encode` | `base64.encode(b: Bytes) -> String` | Go `StdEncoding.EncodeToString` / Python `b64encode` (F) / JS `toBase64` / Gleam `base64_encode` (F) / Elixir `encode64` (F) / Kotlin `Base64.Default.encode` (F) | 据え置き | — |
| `decode` | `base64.decode(s: String) -> Result[Bytes, String]` | Go `DecodeString` / Python `b64decode` (F) / JS `fromBase64` (F) / Gleam `base64_decode` (F) / Elixir `decode64` (F) | 意味変更（`+/` だけを受け取る） | R-base64-01 |
| `encode_url` | `base64.encode_url(b: Bytes) -> String` | Go `URLEncoding` / Python `urlsafe_b64encode` (F) / Gleam `base64_url_encode` (F) / Elixir `url_encode64` (F) / Kotlin `Base64.UrlSafe` (F) | 据え置き | — |
| `decode_url` | `base64.decode_url(s: String) -> Result[Bytes, String]` | Python `urlsafe_b64decode` (F) / JS `fromBase64({alphabet: "base64url"})` (F) / Gleam `base64_url_decode` (F) / Elixir `url_decode64` (F) | 意味変更（`-_` だけを受け取る） | R-base64-01 |
