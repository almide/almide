# Almide Roadmap

## モジュールシステム ✅ 実装完了

### プロジェクト構造

```
myapp/
  almide.toml
  src/
    main.almd              # エントリポイント
    utils.almd             # import self.utils
    http/
      client.almd          # import self.http.client
      server.almd          # import self.http.server
  tests/
    utils_test.almd
```

### almide.toml

```toml
[package]
name = "myapp"
version = "0.1.0"

[dependencies]
json = { git = "https://github.com/almide/json", tag = "v1.0.0" }
```

パッケージ名は `almide.toml` で管理。ソースファイルに `module` 宣言は不要。

### import 構文 ✅

```almide
import self.utils              // utils.add(1, 2)
import self.http.client        // client.get(url)
import self.http.client as c   // c.get(url)
import json                    // 外部依存 (almide.toml)
```

- `self.xxx` = ローカルモジュール → `src/` 配下を解決
- それ以外 = stdlib or 外部依存
- `as` でエイリアス可能
- ユーザーモジュールが stdlib と同名の場合、ユーザーモジュール優先

### モジュールファイルの対応 ✅

| import文 | 探す場所 |
|---|---|
| `import self.utils` | `src/utils.almd` |
| `import self.http.client` | `src/http/client.almd` |
| `import json` | 依存: almide.toml → `~/.almide/cache/` |

### 3レベル可視性 ✅

| 書き方 | スコープ | Rust出力 |
|---|---|---|
| `fn f()` | public（デフォルト） | `pub fn` |
| `mod fn f()` | 同一プロジェクト内のみ | `pub(crate) fn` |
| `local fn f()` | このファイル内のみ | `fn` (private) |

- `type` にも同じ修飾子が使える
- `pub` キーワードは後方互換で受け付ける（デフォルトがpubなので意味なし）

### テストリポジトリ

- https://github.com/almide/mod-sample — visibility + self import の動作確認用

---

## 残りの改善

### checker で可視性エラーを出す ✅

- [x] `local fn` を外部モジュールから呼んだとき checker 段階でエラー
- [x] `mod fn` を外部パッケージから呼んだとき checker 段階でエラー（`is_external` フラグで判定）
- [x] エラーメッセージ: 「function 'xxx' is not accessible from module 'yyy'」

---

## ユーザー定義ジェネリクス

現状 `List[T]`, `Option[T]`, `Result[T, E]` 等はコンパイラ組み込みだが、ユーザーが独自のジェネリック型・関数を定義できない。

### 構文案

```almide
// ジェネリック型
type Stack[T] =
  | Empty
  | Push(T, Stack[T])

// ジェネリック関数
fn map[A, B](xs: List[A], f: fn(A) -> B) -> List[B] =
  match xs {
    [] => []
    [head, ...tail] => [f(head)] ++ map(tail, f)
  }

fn identity[T](x: T) -> T = x
```

### 実装ステップ

- [ ] パーサー: `fn name[T, U](...) -> ...` のジェネリックパラメータ解析（パース自体は `try_parse_generic_params` で部分対応済み）
- [ ] 型チェッカー: 型変数の導入、型推論（単一化ベース）
- [ ] Rust emitter: `fn name<T, U>(...) -> ...` に変換
- [ ] TS emitter: `function name<T, U>(...): ...` に変換（JSモードでは型消去）

### 設計判断

- 型パラメータは `[T]` 記法（almide既存の `List[T]` と一貫）
- 型推論を基本とし、呼び出し側で明示的な型引数は不要にしたい
- 型制約は trait 実装後に `fn sort[T: Ord](xs: List[T])` のような形で導入

---

## trait / impl

パーサーは対応済み（`trait` / `impl` 宣言をパースしてASTに格納）。checker と emitter が未実装。

### 構文（パーサー対応済み）

```almide
trait Show {
  fn show(self) -> String
}

impl Show for Color {
  fn show(self) -> String = match self {
    Red => "red"
    Green => "green"
    Blue => "blue"
  }
}
```

### 実装ステップ

- [ ] checker: trait メソッドのシグネチャ登録、impl の型チェック
- [ ] checker: impl が trait のメソッドを全て実装しているか検証
- [ ] Rust emitter: `impl Trait for Type { ... }` をそのまま出力
- [ ] TS emitter: trait を interface として出力、メソッド呼び出しをディスパッチ
- [ ] `self` パラメータの扱い: UFCS（`show(color)` と `color.show()` 両方可能）

### 設計メモ

- Almide に class はない。trait + impl + パターンマッチが型の振る舞いを定義する方法
- `self` は trait メソッドのレシーバ専用。class のインスタンス参照ではない
- `import self.xxx` の `self` とは別の用途（文脈で区別可能: import 文 vs パラメータリスト）
- deriving は既にパーサー対応済み（`type Color = ... deriving Show, Eq`）

---

## その他

- [ ] パッケージレジストリ (将来検討)
