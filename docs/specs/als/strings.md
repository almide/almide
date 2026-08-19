# ALS — 文字列（Strings）

Almide Language Specification の文字列規範。実装（v0 native / v1 wasm）から独立に、
観測可能な振る舞い（stdout・stderr・終了コード）を定義する。各節は契約台帳
（docs/contracts/contracts.toml の `spec` フィールド）から参照され、
`scripts/check-contracts.sh` が節の実在を強制する。

## ALS-S1 コードポイント意味論

文字列は UTF-8 バイト列であり、`string.len`・`slice`・`chars`・`char_at`・
`index_of`・添字は**コードポイント単位**で数える（バイトでも grapheme でもない）。
マルチバイト文字（CJK・絵文字・結合文字を含む）に対する全操作は、この単位で
一貫していなければならない。範囲外の `char_at`/`slice` は空文字列を返す。
Contracts: C-016。

## ALS-S2 空パターンの検索規則

`string.count("")` は**コードポイント数 + 1** を返す（Rust `str::matches` の空
パターン意味論）。`last_index_of` は最後の一致の**コードポイント位置**を返し、
空パターンでは `len`（末尾位置）を返す。`index_of("")` は 0。
Contracts: C-017。

## ALS-S3 文字種述語

`is_alpha`・`is_digit`・`is_alnum`・`is_upper`・`is_lower` 等の述語は Rust の
対応する `char` メソッド（`char::is_alphabetic` 等、Unicode 全域）と一致する。
ASCII 限定の近似は不適合。空文字列に対する全称述語は true（vacuous truth）。
`replace_first`・`strip_prefix`・`strip_suffix`・`cmp` は Rust str の対応
メソッドと観測等価（`cmp` はバイト辞書順）。
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
