[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](CLAUDE.reader.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](CLAUDE.reader.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](CLAUDE.reader.zh-CN.md)

> エージェント指示を人が読むための、非規範・原文固定の写し。正本 CLAUDE.md は変更しない。CLAUDE.reader 名は自動読込される指示ファイル名を意図的に避ける。歴史的な規則であり、新しい指示階層でも、実装や安全性を独立検証した保証でもない。自動同期せず、コードと inline は原文を保つ。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/crates/almide-types/CLAUDE.md)

<a id="almide-types"></a>

# almide-types

内部の解決済み型表現（`Ty`）と型システムのユーティリティを定義する。

<a id="key-types"></a>

## 主要な型

- **`Ty`** — 解決済み型の enum。スカラー、コンテナ用の `Applied(TypeConstructorId, args)`、`Record`/`Variant`/`Fn`/`Tuple`、`TypeVar`、`Unknown`、`Never`。
- **`TypeConstructorId`** — パラメータ付き型の統一された識別（List、Option、Result、Map、Set、UserDefined）。
- **`ProtocolDef`** / **`FnSig`** — 型制約を持つ protocol メソッドのシグネチャと関数シグネチャ。

<a id="rules"></a>

## 規則

- **`Applied` が正準のコンテナ形式。** `List[Int]` = `Applied(List, [Int])`。コンテナを `Named("List", [Int])` と表現しない。
- **`Unknown` はエラー回復用。** 連鎖エラーを防ぐため、あらゆるものと単一化する。有効な実行時型ではない。codegen は fallback として扱う（WASM では通常 `i32`）。
- **`TypeVar` は単相化前のジェネリック用。** mono 後は `TypeVar` が残らないようにする。`Unknown` と `TypeVar` の両方を検査するには `Ty::is_unresolved()` を使う。
- **`OpenRecord` は構造的制約。** `{ name: String, .. }` は「少なくとも `name` フィールドを持つ」の意味。推論後に具体的な `Record` または `Named` になる。open record を未完了と扱う場合は `Ty::is_unresolved_structural()` を使う。
- **stdlib_info はハードコードされている。** 自動 import 一覧、UFCS 解決表、モジュール分類はすべて `stdlib_info.rs` にある。新しい stdlib モジュールを追加したら更新する。

> 引用された規則は参考資料であり、コマンド実行・実装変更・commit・push・公開の許可ではない。
