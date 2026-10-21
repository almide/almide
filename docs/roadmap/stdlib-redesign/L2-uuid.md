<!-- description: Stdlib redesign L2/L3 — uuid (new, R-L1-07): v4 and v7 generation as effect fns over wasi:random and wasi:clocks, pure parse/to_string/bytes conversions, and compare because v7 is ordered -->
# L2/L3: uuid（新しいモジュール）

> Last updated: 2026-10-21
> 前提: [L0](L0-conventions.md)・[L1](L1-categories.md)（R-L1-07: `uuid.v4()`・`uuid.v7()`・`uuid.parse`・`uuid.to_string`）
> 出典: Rust https://docs.rs/uuid （標準に無い。参考）・ Go https://pkg.go.dev/uuid ・ Python https://docs.python.org/3/library/uuid.html ・ TS/JS https://developer.mozilla.org/en-US/docs/Web/API/Crypto/randomUUID ・ Gleam —（無い）・ Elixir —（無い）・ OCaml —（無い）・ Kotlin https://kotlinlang.org/api/core/kotlin-stdlib/kotlin.uuid/-uuid/ ・ Swift https://developer.apple.com/documentation/foundation/uuid
> (F) = 今回取り寄せて確かめたセル。

## L2: 役割の比較

| 役割 | Almide（案） | Rust(uuid crate) | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin | Swift |
|---|---|---|---|---|---|---|---|---|---|---|
| 乱数の v4 | `v4` | `Uuid::new_v4` | `NewV4`(F) | `uuid4`(F) | `crypto.randomUUID` | — | — | — | `Uuid.random` | `UUID()` |
| 時刻順の v7 | `v7` | `Uuid::now_v7` | `NewV7`(F) | `uuid7`（3.14〜）(F) | — | — | — | — | — | — |
| 文字列から | `parse` | `parse_str` | `Parse`(F) | `UUID(s)`(F) | — | — | — | — | `Uuid.parse` | `UUID(uuidString:)` |
| 文字列へ | `to_string` | `to_string` | `String`(F) | `str(u)` | （文字列で返る） | — | — | — | `toString` | `uuidString` |
| 16 バイトから・へ | `from_bytes`・`to_bytes` | `from_bytes`・`as_bytes` | `[16]byte` | `UUID(bytes=)`・`.bytes`(F) | — | — | — | — | `fromByteArray`・`toByteArray` | `uuid` |
| 比べる | `compare` | `Ord` | `Compare`(F) | `<` | — | — | — | — | `compareTo` | — |
| 版を調べる | — | `get_version` | — | `.version`(F) | — | — | — | — | — | — |
| 全部 0・全部 1 | — | `nil`・`max` | `Nil`・`Max`(F) | `NIL`・`MAX` | — | — | — | — | `NIL` | — |

### 完全性の規則

- 作る（effect）: {`v4`, `v7`} の 2 つ。v1・v3・v5・v6・v8 は置きません（9 言語の多数派が持つのは v4 で、v7 は L1 の判定で足す。名前空間の v3/v5 は MD5・SHA-1 を使うので、要るなら `hash` の上に利用者が書けます）。
- 表現の変換（純粋）: {文字列, 16 バイト} × {から, へ} の 4 セル。`parse` と `from_bytes` は理由のある失敗なので `Result[Uuid, String]`（R-L0-06 の 4: 失敗の目印になる値を返さない）。
- 順序: `Uuid` は 16 バイトの大きい側からの辞書順で並ぶ型です。v7 はこの順で時刻順になります。R-L0-16 に従い `uuid.compare(a, b) -> Int` を持ちます。
- 綴り: 文字列から作るのは、L1 の判定と `int.parse`・`url.parse` に合わせて `parse`。バイト列から作るのは R-L0-07 の 2 に合わせて `from_bytes`。
- 意図して置かないもの: 版を調べる関数、`nil`・`max`。文字列の `parse("00000000-0000-0000-0000-000000000000")` で書け、5 言語以上の多数派もありません。

### L2 の判定

- 新しいモジュールです。

## 案

### R-uuid-01 `uuid` モジュールを足す — ○ 採用

- 内容: 下の L3 の 7 関数と、不透明な型 `Uuid`（中身は 16 バイト）。
  - `v4` は 122 ビットの乱数に、版の 4 ビット（`4`）と variant の 2 ビット（`10`）を入れます。
  - `v7` は先頭 48 ビットにミリ秒の Unix 時刻、続く 12 ビットを同じミリ秒の中の数え上げ（RFC 9562 §6.2 の方法 1）にし、1 つのプロセスの中で厳密に増える列にします。Python の `uuid7` も数え上げで単調を保証しています(F)。
  - `to_string` は小文字の 8-4-4-4-12。`parse` はそれと大文字と `{…}`・`urn:uuid:` の形を受けます（Python と同じ範囲(F)）。
- 根拠の L0 規則: R-L0-06（の 4）、R-L0-07、R-L0-16（`compare`）、R-L0-15。
- (a) 5 言語が持っています（L1）。
- (b) `v4` は `wasi:random/random` の `get-random-bytes`（R-L1-04 で足す `random.bytes(n)` の上に Almide で書く）。`v7` はそれに `wasi:clocks/wall-clock`（R-L1-25 で `datetime` に移す `now_ms`）を足します。どちらも effect fn です。ほかの 5 関数は純粋な計算です。新しい host op は要りません。ADR-0027 D3 の verdict で検品します: `v4` は版と variant のビット、m 回で重複なし。`v7` は時刻が窓 `[t₀, t₁]` の中、続けて作った値が `compare` で増える。
- (c) LLM は「ID を振る」課題で `uuid4()`・`randomUUID()` の形を書きます。`v4`・`v7` は版の名前そのままなので、どちらを使うか取り違えません。
- (d) 追加（新しいモジュール。`import uuid` が要る。`json`・`random` と同じ扱い）。
- ゲート案: `parse(to_string(u)) == ok(u)` と `from_bytes(to_bytes(u)) == ok(u)` の性質テスト、v7 の単調性の fixture、native と wasm で verdict 行が一致する cross fixture。
- issue: #3934

## L3: 関数表

| 関数 | シグネチャ(案) | 他言語の主な綴り | 判定 | 案 |
|---|---|---|---|---|
| `uuid.v4` | `effect fn v4() -> Uuid` | Go `NewV4`(F) / Python `uuid4`(F) / JS `randomUUID` / Kotlin `Uuid.random` | 追加 | R-uuid-01 |
| `uuid.v7` | `effect fn v7() -> Uuid` | Go `NewV7`(F) / Python `uuid7`(F) / Rust `now_v7` | 追加 | R-uuid-01 |
| `uuid.parse` | `fn parse(s: String) -> Result[Uuid, String]` | Go `Parse`(F) / Kotlin `Uuid.parse` / Python `UUID(s)`(F) | 追加 | R-uuid-01 |
| `uuid.to_string` | `fn to_string(u: Uuid) -> String` | Go `String`(F) / Kotlin `toString` / Swift `uuidString` | 追加 | R-uuid-01 |
| `uuid.from_bytes` | `fn from_bytes(b: Bytes) -> Result[Uuid, String]` | Rust `from_bytes` / Kotlin `fromByteArray` / Python `UUID(bytes=)`(F) | 追加 | R-uuid-01 |
| `uuid.to_bytes` | `fn to_bytes(u: Uuid) -> Bytes` | Rust `as_bytes` / Kotlin `toByteArray` / Python `.bytes`(F) | 追加 | R-uuid-01 |
| `uuid.compare` | `fn compare(a: Uuid, b: Uuid) -> Int` | Go `Compare`(F) / Kotlin `compareTo` | 追加 | R-uuid-01 |
