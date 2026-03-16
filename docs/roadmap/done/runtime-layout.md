# Runtime Layout Unification [ACTIVE]

## Problem

Rust と TypeScript のランタイムが別々の場所・別々の形式で管理されている。

```
現状:
  runtime/rust/src/*.rs        Rust ランタイム（素の .rs、include_str! で埋め込み）
  src/emit_ts_runtime/*.rs     TS ランタイム（Rust const 文字列として TS コードを保持）
```

- TS 側は `.rs` ファイル内に TS コードが文字列リテラルで埋め込まれている
- IDE のシンタックスハイライト・補完が効かない
- テストが書けない（文字列なので）
- Rust/TS で構造が非対称

## Goal

```
runtime/
├── rust/src/*.rs       Rust ランタイム（現状維持）
└── ts/
    ├── string.ts       TS ランタイム（素の .ts）
    ├── list.ts
    ├── map.ts
    ├── int.ts
    ├── float.ts
    ├── math.ts
    ├── json.ts
    ├── result.ts
    ├── io.ts
    ├── net.ts
    └── ...
```

- 両ターゲットが `runtime/{lang}/` 以下に統一
- TS ランタイムが素の `.ts` ファイルになり、IDE サポート・単体テストが可能
- コンパイラは `include_str!("../../runtime/ts/string.ts")` で読み込み

## Migration Steps

1. `src/emit_ts_runtime/core.rs` の各 `const MOD_*_TS: &str = r#"..."#` を `runtime/ts/*.ts` に切り出し
2. `src/emit_ts_runtime/collections.rs` 同様
3. `src/emit_ts_runtime/data.rs` 同様
4. `src/emit_ts_runtime/io.rs` 同様
5. `src/emit_ts_runtime/net.rs` 同様
6. `src/emit_ts_runtime/mod.rs` を修正: `include_str!("../../runtime/ts/*.ts")` で読み込み
7. `src/emit_ts_runtime/*.rs` の Rust const 定義を削除
8. `src/emit_ts_runtime/` 配下は `mod.rs`（ランタイム結合ロジック）のみ残す
9. Cross-Target CI で TS/JS テストが通ることを確認

## Notes

- Rust 側は `runtime/rust/src/` で既に正しい構造。変更不要
- TS ファイルに切り出す際、`r#"..."#` のエスケープを解除するだけ（コード自体は同じ）
- Deno で `runtime/ts/` の単体テストを書けるようになる（将来）
