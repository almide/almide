# ALS — 文字列（Strings）

> Last updated: 2026-08-21

Almide Language Specification の文字列規範。実装（v0 native / v1 wasm）から独立に、
観測可能な振る舞い（stdout・stderr・終了コード）を定義する。各節は契約台帳
（docs/contracts/contracts.toml の `spec` フィールド）から参照され、
`scripts/check-contracts.sh` が節の実在を強制する。

## ALS-S1 コードポイント意味論

文字列は UTF-8 バイト列であり、`string.len`・`slice`・`chars`・`string.get`・
`index_of` は**コードポイント単位**で数える（バイトでも grapheme でもない;
`s[i]` 添字構文は String に対して E026 で拒否され、要素アクセスは
`string.get(s, i) -> Option[String]`）。
マルチバイト文字（CJK・絵文字・結合文字を含む）に対する全操作は、この単位で
一貫していなければならない。範囲外は `string.get` が none、`slice` が
空文字列を返す。
Contracts: C-016。

## ALS-S2 空パターンの検索規則

`string.count("")` は**コードポイント数 + 1** を返す（Rust `str::matches` の空
パターン意味論）。`last_index_of` は最後の一致の**コードポイント位置**を返し、
空パターンでは `len`（末尾位置）を返す。`index_of("")` は 0。
Contracts: C-017。

## ALS-S3 文字種述語

文字種述語は**述語ごとに**持ち上げ規則が異なる。「空文字列に対する全称述語は true」
という一律の規則は**存在しない**（2026-08-21 裁定、参照評価器の所見 F8）。
規範は次の表で、各行を fixture が固定する:

| 述語 | 文字集合 | 判定 | `""` |
|------|----------|------|------|
| `string.is_alpha` | Unicode `Alphabetic`（`char::is_alphabetic`） | 全コードポイントが該当、かつ**非空** | false |
| `string.is_alphanumeric` | Unicode 英数（`char::is_alphanumeric`: 文字・数字・`²`・`½`・アラビア数字等を含む） | 全コードポイントが該当、かつ**非空** | false |
| `string.is_digit` | **ASCII `0-9` のみ**（全角・アラビア・インド数字は不可） | 全コードポイントが該当、かつ**非空** | false |
| `string.is_upper` / `is_lower` | 大小文字を持つ（cased）コードポイント | **cased が 1 つ以上**あり、cased の**全て**が大文字（小文字）。cased でない文字（`.`・空白・CJK・数字）は無視（`A.B` は upper、`日2` はどちらも false、`Ａa` はどちらも false） | false |
| `string.is_whitespace` | Unicode `White_Space`（ALS-T1 の集合） | 全コードポイントが該当。**空は vacuous に true** | **true** |

ASCII 限定の近似は `is_digit` 以外では不適合。`replace_first`・`strip_prefix`・
`strip_suffix`・`cmp` は Rust str の対応メソッドと観測等価（`cmp` はバイト辞書順）。

```almide
test "each predicate lifts its own way over the empty string" {
  assert_eq(string.is_alpha(""), false)
  assert_eq(string.is_alphanumeric(""), false)
  assert_eq(string.is_digit(""), false)
  assert_eq(string.is_upper(""), false)
  assert_eq(string.is_whitespace(""), true)
  assert_eq(string.is_upper("A.B"), true)
  assert_eq(string.is_alphanumeric("\u{00BD}"), true)
  assert_eq(string.is_digit("\u{FF11}"), false)
}
```

Fixture: `spec/wasm_cross/string_predicates.almd`、`spec/wasm_cross/string_ops_drain.almd`、
`spec/wasm_cross/string_whitespace.almd`、`spec/wasm_cross/string_is_alphanumeric.almd`。
Contracts: C-018, C-019。

## ALS-S4 バイト列との相互変換

`string.from_bytes` は **UTF-8 lossy デコード**（不正シーケンスは U+FFFD に
置換、`String::from_utf8_lossy` と同一の置換規則）。`string.to_bytes` は
UTF-8 バイト列をそのまま返し、有効な文字列に対して `from_bytes ∘ to_bytes`
は恒等。
Contracts: C-022。

## ALS-S5 split の区切り規範

`string.split` は区切り文字列のエスケープ・特殊文字をリテラルとして扱う
（正規表現ではない）。連続区切りは空要素を生み、先頭/末尾の区切りも同様
（Rust `str::split` と観測等価）。
Contracts: C-050。

## ALS-S6 規模不変性

split / replace 等の反復文字列操作は入力規模（MB 級）に対して結果が
規模非依存で正しい（内部バッファ境界・再割り当てで結果が変わらない）。
Contracts: C-074。
