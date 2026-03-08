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

### その他

- [ ] ユーザー定義ジェネリクス
- [ ] trait / impl の実装
- [ ] パッケージレジストリ (将来検討)
