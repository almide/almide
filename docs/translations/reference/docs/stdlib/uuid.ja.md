# uuid

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](uuid.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](uuid.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](uuid.zh-CN.md)



> 原文：[uuid.md @ 35297a6](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/uuid.md)。非規範的な読者向け固定スナップショットです。自動生成・自動同期されません。署名・コード例・API 索引のコメントは原文を保持し、説明の訳を末尾に併記しています。計測・動作・例をこの翻訳作業で独立に再検証していません。
>
> 原文上の不整合: 冒頭は全関数が native/wasm で動くと述べる一方、v7 節末尾は「現在 native のみ」としています。訳は両方を保持し、どちらが現在の実装か独立検証していません。

RFC 9562 識別子。ランダムな version 4、時刻順の version 7、8-4-4-4-12 のテキスト形式、16 生バイトを扱います。`import uuid`。

`uuid.Uuid` は 16 バイトからなる値です。不透明型であり、このモジュールだけが構築するため、各 `Uuid` は正確に 16 バイトを保持します。生成は `effect fn`（`v4` は乱数、`v7` は時計も読む）、変換と比較は通常の `fn`s です。

`v4()` と `v7()` は通常の `Uuid` を返しますが、`effect fn`s なので呼出しは `Result` になります。`uuid.v4()!` と書きます。

**ターゲット。** 全関数が native と wasm で動きます。wasm の `v7` はモジュールインスタンスに直前の stamp を保持するため、同じ実行内で生成する各 `Uuid` は native と同様、直前のものより大きくなります。

## 生成

### `uuid.v4() -> Uuid`

ランダムな（version 4）`Uuid`。OS の暗号学的乱数源からの 122 ランダムビットです。

```almd check
import uuid

effect fn main() -> Unit = {
  let id = uuid.v4()!
  println(uuid.to_string(id))
}
```

### `uuid.v7() -> Uuid`

時刻順の（version 7）`Uuid`。Unix 時刻のミリ秒、カウンタ、ランダムビットの順です。一プロセス内では、同一ミリ秒に二つ生成しても各 `v7` は直前のものより大きくなるため、`compare` でソートすると生成順になります。時間とともに挿入する行のキーに向きます。現時点では native のみです。

```almd check
import uuid

effect fn main() -> Unit = {
  let a = uuid.v7()!
  let b = uuid.v7()!
  println(if uuid.compare(a, b) < 0 then "ordered" else "not ordered")
}
```

## テキストとバイト

### `uuid.parse(s: String) -> Result[Uuid, String]`

`s` が表す `Uuid`。8-4-4-4-12 の十六進数字で大小文字どちらも可、任意で `{...}` 内、または `urn:uuid:` の後に置けます。それ以外は `err`（`uuid.parse: not an 8-4-4-4-12 UUID: "..."`）です。

### `uuid.to_string(u: Uuid) -> String`

`u` を 8-4-4-4-12 の小文字十六進数字にします。

### `uuid.from_bytes(b: Bytes) -> Result[Uuid, String]`

正確に 16 バイトの `Uuid`。それ以外の長さは `err`。

### `uuid.to_bytes(u: Uuid) -> Bytes`

`u` の 16 バイト。

```almd run
import uuid

effect fn main() -> Unit = {
  let u = uuid.parse("{550E8400-E29B-41D4-A716-446655440000}")!
  println(uuid.to_string(u))
  println("${bytes.len(uuid.to_bytes(u))} bytes")
}
```
```output
550e8400-e29b-41d4-a716-446655440000
16 bytes
```

## 順序

### `uuid.compare(a: Uuid, b: Uuid) -> Int`

先頭からバイトごとに比較し、`a` が `b` より前・同じ・後なら、それぞれ負・ゼロ・正になります。`v7` の値では生成順です。



## 固定スナップショットの API 索引 (7 関数)

```
// A random (version 4) Uuid: 122 bits from the OS CSPRNG.
// @since unreleased
effect uuid.v4() -> Uuid

// A time-ordered (version 7) Uuid: Unix ms, a counter, random bits; each is greater than the last.
// @since unreleased
effect uuid.v7() -> Uuid

// The Uuid s spells: 8-4-4-4-12 hex digits, either case, optionally in {…} or after urn:uuid:.
// @since unreleased
uuid.parse(s: String) -> Result[Uuid, String]

// u as 8-4-4-4-12 lowercase hex digits.
// @since unreleased
uuid.to_string(u: Uuid) -> String

// The Uuid of exactly 16 bytes b; err for any other length.
// @since unreleased
uuid.from_bytes(b: Bytes) -> Result[Uuid, String]

// The 16 bytes of u.
// @since unreleased
uuid.to_bytes(u: Uuid) -> Bytes

// Negative, zero or positive as a orders before, with or after b, byte by byte.
// @since unreleased
uuid.compare(a: Uuid, b: Uuid) -> Int
```



## API 説明の訳（上記署名順）

- `effect uuid.v4() -> Uuid` — ランダムな（version 4）Uuid。OS CSPRNG からの 122 ビット。
- `effect uuid.v7() -> Uuid` — 時刻順の（version 7）Uuid。Unix ms、カウンタ、ランダムビット。各値は直前より大きくなります。
- `uuid.parse(s: String) -> Result[Uuid, String]` — s が表す Uuid。8-4-4-4-12 の十六進数字、大小文字可。任意で {…} 内または urn:uuid: の後に置けます。
- `uuid.to_string(u: Uuid) -> String` — u を 8-4-4-4-12 の小文字十六進数字にします。
- `uuid.from_bytes(b: Bytes) -> Result[Uuid, String]` — 正確に 16 バイトの b の Uuid。それ以外の長さは err。
- `uuid.to_bytes(u: Uuid) -> Bytes` — u の 16 バイト。
- `uuid.compare(a: Uuid, b: Uuid) -> Int` — a が b より前・同じ・後なら、バイトごとの比較で負・ゼロ・正。
