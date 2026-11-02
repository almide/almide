# Breaking Change Policy

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](BREAKING_CHANGE_POLICY.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](BREAKING_CHANGE_POLICY.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](BREAKING_CHANGE_POLICY.zh-CN.md)

> [固定原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/project/BREAKING_CHANGE_POLICY.md)の非規範的な読者用コピーです。新たな正本や生成器の出力ではなく、自動同期しません。原本と機械的な参照元は変更していません。歴史的な主張・例をここで独立に再検証したものではありません。



> Python 2→3 の教訓: silent な挙動変更はエコシステムに大きな負担をかける。
> Swift 1→2→3 の教訓: 連続 breaking release はユーザーの移行コストを増大させる。
> Go 1 の教訓: 「今日コンパイルできるコードは永遠にコンパイルできる」が最大の強み。

---

## 現行実装と post-1.0 方針の区別

以下の post-1.0 / edition 方針は将来の運用設計であり、現在の実装機能ではない。
現行 `src/project.rs::PACKAGE_METADATA_KEYS` の `edition` は挙動を変えない
metadata として受理される。edition ごとの parser 切替や `almide migrate`
subcommand は実装されていない。現在の removed syntax / migration hints は
`proofs/dialect-epochs.toml` と checker が扱う。将来方針自体は以下に保持する。

## 原則

### 1. Silent な挙動変更は絶対禁止

post-1.0 で既存コードの意味が変わる変更は行わない。もし型や演算子の挙動を変える必要がある場合:
- 旧コードは **コンパイルエラー** になること（silent に違う結果を返すのではなく）
- エラーメッセージに **migration hint** を含めること
- 新しい edition でのみ新挙動を有効にすること

### 2. API 削除は 2 minor version の deprecation 後

stdlib 関数の削除・改名:
1. **v1.x**: `#[deprecated]` warning を出す（コンパイルは通る）
2. **v1.(x+1)**: warning を継続
3. **v1.(x+2)**: 削除可能。ただし compile error + hint で代替関数を案内

### 3. 構文変更は edition で吸収（post-1.0 構想）

新キーワードの追加や構文の変更は `edition` フィールドで制御:
- `edition = "2026"` のコードは 2026 のルールでコンパイル
- `edition = "2027"` で新構文が有効
- 異なる edition のモジュールは相互運用可能（Rust editions と同じ）
- `almide migrate --edition 2027` で自動変換ツールを提供

### 4. コア型のシグネチャは不変

以下の型の既存メソッド/関数のシグネチャは post-1.0 で変更しない:
- `String`, `Int`, `Float`, `Bool`
- `List[T]`, `Map[K, V]`
- `Option[T]`, `Result[T, E]`
- `Tuple`, `Record`, `Variant`

関数の **追加** は可能。既存関数の **引数型・戻り型の変更** は禁止。

---

## 1.0 前の breaking change

1.0 前は breaking change が許可される。ただし:
- CHANGELOG に明記
- 可能な限り migration hint 付きの compile error を出す
- stdlib-verb-system reform は 1.0 前に完了する

---

## edition の仕組み（未実装の設計）

```toml
# almide.toml
[package]
name = "myapp"
version = "0.1.0"
edition = "2026"
```

- `edition` を省略した場合、コンパイラの最新 edition がデフォルト
- 異なる edition の依存パッケージは問題なく混在可能
- edition は 2-3 年ごとに更新（Rust の 3 年サイクルに準じる）
