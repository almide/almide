<a id="als--文字列strings"></a>

# ALS — 文字列（Strings）

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](strings.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](strings.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](strings.zh-CN.md)

> [固定原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/strings.md)の非規範的な読者用コピー。以下の規範性・要件は原本に属し、このコピーに独立した規範性はありません。自動同期せず、歴史的な実装・適合・安全性の主張をこの翻訳作業で独立に再検証していません。コードと規則識別子は保持しています。

> Last updated: 2026-09-10

Almide Language Specification の文字列規範。実装（v0 native / v1 wasm）から独立に、
観測可能な振る舞い（stdout・stderr・終了コード）を定義する。各節は契約台帳
（docs/contracts/contracts.toml の `spec` フィールド）から参照され、
`scripts/check-contracts.sh` が節の実在を強制する。

<a id="als-s1-コードポイント意味論"></a>

## ALS-S1 コードポイント意味論

文字列は UTF-8 バイト列であり、`string.len`・`slice`・`chars`・`string.get`・
`index_of` は**コードポイント単位**で数える（バイトでも grapheme でもない;
`s[i]` 添字構文は String に対して E026 で拒否され、要素アクセスは
`string.get(s, i) -> Option[String]`）。
これらの操作は、マルチバイト文字（CJK・絵文字・結合文字を含む）に対して、この単位で
一貫していなければならない。範囲外は `string.get` が none、`slice` が
空文字列を返す。バイト単位の明示的な操作は ALS-S4 に定める。
Contracts: C-016。

<a id="als-s2-空パターンの検索規則"></a>

## ALS-S2 空パターンの検索規則

`string.count_matches("")` は**コードポイント数 + 1** を返す（Rust `str::matches` の空
パターン意味論）。`last_index_of` は最後の一致の**コードポイント位置**を返し、
空パターンでは `len`（末尾位置）を返す。`index_of("")` は 0。
Contracts: C-017。

<a id="als-s3-文字種述語"></a>

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

<a id="als-s4-バイト列との相互変換"></a>

## ALS-S4 バイト列との相互変換

`bytes.to_string_lossy` は **UTF-8 lossy デコード**（不正シーケンスは U+FFFD に
置換、`String::from_utf8_lossy` と同一の置換規則）。`bytes.from_string` は
UTF-8 バイト列をそのまま返し、有効な文字列に対して `to_string_lossy ∘ from_string`
は恒等。
`string.byte_slice(s, start, end)` は UTF-8 バイト単位の半開区間をコピーし、
両端がコードポイント境界かつ `0 <= start <= end <= バイト長` の場合だけ
`some(String)` を返す。それ以外は `none`。有効な境界上の空区間は
`some("")`。端点の検査は区間外を走査せず、コピー量は指定区間のバイト数。
コードポイント単位の `string.slice` のクランプ規則は変わらない。

Fixture: `spec/wasm_cross/string_from_bytes.almd`、
`spec/wasm_cross/string_byte_slice.almd`。
Contracts: C-022, C-348。

<a id="als-s5-split-の区切り規範"></a>

## ALS-S5 split の区切り規範

`string.split` は区切り文字列のエスケープ・特殊文字をリテラルとして扱う
（正規表現ではない）。連続区切りは空要素を生み、先頭/末尾の区切りも同様
（Rust `str::split` と観測等価）。
Contracts: C-050。

<a id="als-s6-規模不変性"></a>

## ALS-S6 規模不変性

split / replace 等の反復文字列操作は入力規模（MB 級）に対して結果が
規模非依存で正しい（内部バッファ境界・再割り当てで結果が変わらない）。
Contracts: C-074。

> 原文の文脈：ALS-S1 は範囲外の slice の結果を広く述べる一方、ALS-S4 は string.slice のクランプ規則も維持しています。統一的な境界条件を推測せず両方を保持しています。個別の区間については固定原文と fixture を確認してください。
