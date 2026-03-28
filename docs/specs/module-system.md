# Module System Specification

> Last updated: 2026-03-28. Verified by `spec/integration/modules/diamond_test.almd` (11 tests) + error tests.

---

## 1. Package Structure

```
mypackage/
  almide.toml              [package] name = "mypackage", version = "0.1.0"
  src/
    mod.almd               パッケージのエントリポイント
    utils.almd             サブモジュール → mypackage.utils
    parser.almd            サブモジュール → mypackage.parser
    bindings/              ディレクトリ = 名前空間（mod.almd 不要）
      python.almd          サブモジュール → mypackage.bindings.python
      go.almd              サブモジュール → mypackage.bindings.go
      internal/
        helpers.almd       サブモジュール → mypackage.bindings.internal.helpers
```

**ルール:**
- `src/mod.almd` がパッケージのトップレベル。`import mypackage` → `mypackage.func()` で呼べる
- 同階層の `.almd` ファイル（`mod.almd`, `lib.almd`, `main.almd` を除く）が自動的にサブモジュールになる
- サブディレクトリは名前空間として機能する。`mod.almd` がなくてもドットパスの中間ノードになる
- 再帰的に任意の深さまでスキャンされる
- パッケージ名は `almide.toml` の `[package] name` で定義。ソースファイル内に module 宣言は不要

---

## 2. Import

```almide
import pkg                    // パッケージ全体 + 全サブモジュール
import pkg.sub                // 特定のサブモジュールのみ
import pkg as p               // エイリアス
import self                   // 自パッケージの mod.almd
import self.sub               // 自パッケージのサブモジュール
```

- `import pkg` でサブモジュール含め全て自動ロード。個別 import 不要
- `import pkg.sub` は最後のセグメント名で参照可能: `sub.func()`
- ワイルドカード `import pkg.*` は不可
- 循環インポートはコンパイルエラー

---

## 3. 呼び出し

```almide
import mypackage

// 1段: トップレベル関数
mypackage.version()

// 2段: サブモジュール関数
mypackage.utils.format(x)

// 3段: ディレクトリ内サブモジュール
mypackage.bindings.python.generate(iface)

// N段: 任意の深さ
mypackage.bindings.internal.helpers.escape(s)
```

AST の `Member` チェーン（`a.b.c.func()`）を末尾から辿り、`imported_user_modules` に登録されたモジュール名またはその子が存在する名前空間に一致する最長パスを見つけ、残りを関数名として解決する。

---

## 4. モジュール境界

**直接 import したパッケージのみアクセス可能。推移的依存は不可視。**

```almide
import B       // B は内部で D を import している

B.func()       // ✓ 直接 import した
D.func()       // ✗ undefined variable 'D'
```

D を使いたければ `import D` を明示する。npm の phantom dependency 問題を防ぐ設計。

---

## 5. ダイヤモンド依存

```
main → B → D
main → C → D
```

D は1回だけロードされ、1回だけコンパイル出力に含まれる。B と C は同じ D を参照する。

```almide
import B
import C
import D

B.from_b()           // "B says: from D" — B 経由で D を呼ぶ
C.from_c()           // "C says: from D" — C 経由で D を呼ぶ
D.shared()           // "from D"         — 直接 D を呼ぶ
```

### 型の同一性

D が定義した型は、B 経由でも C 経由でも同一の型として扱われる。

```almide
let logger = B.make_logger()     // D.Logger 型を返す
C.process_logger(logger)         // ✓ B が作った D.Logger を C が受け取れる
D.log_name(logger)               // ✓ 直接 D に渡すのも同じ型
```

### バージョン違いのダイヤモンド

`PkgId(name, major)` で管理。同じ `(name, major)` は1つに統一（MVS: 最大の最小バージョンを選択）。異なる major は別モジュールとして共存し、codegen でシンボル名にバージョンが付く（`pkg_v1_func`, `pkg_v2_func`）。異なる major の同名型は互換性がない。

```
B requires D v1.x → almide_rt_D_v1_func()
C requires D v2.x → almide_rt_D_v2_func()
D_v1.Logger ≠ D_v2.Logger
```

---

## 6. 可視性

| 修飾子 | スコープ | 例 |
|---|---|---|
| `fn` | public — どこからでもアクセス可 | `fn version() -> String` |
| `mod fn` | 同一プロジェクト内のみ | `mod fn internal() -> String` |
| `local fn` | 同一ファイル内のみ | `local fn helper() -> String` |

外部から `mod fn` / `local fn` にアクセスするとコンパイルエラー:

```
error: function 'internal' is not accessible from module 'extlib'
  hint: 'internal' has restricted visibility
```

---

## 7. `import self`

自パッケージの `src/mod.almd` を参照する。`main.almd` からライブラリ関数を呼ぶ場合に使う。

```almide
// main.almd
import self as mylib
mylib.exported_function()
```

`almide.toml` の `name` がデフォルトのモジュール名。`as` でエイリアス可。`src/mod.almd` が存在しない場合はエラー。

---

## 8. サブモジュールの依存解決

サブモジュールは stdlib や他パッケージを自由に import できる。親パッケージのロード時に再帰的に解決される。

```almide
// mypackage/src/formatter.almd
fn format_upper(s: String) -> String = string.to_upper(s)   // stdlib

// mypackage/src/utils.almd
import extlib
fn describe(s: String) -> String = extlib.info() + ": " + s  // 他パッケージ
```

サブモジュール内の型チェックでは、そのサブモジュールが import した stdlib / ユーザーモジュールが正しく認識される。

---

## 9. 依存管理

### almide.toml

```toml
[package]
name = "myapp"
version = "0.1.0"

[dependencies]
bindgen = { git = "https://github.com/almide/almide-bindgen.git", tag = "v0.1.0" }
json = { git = "https://github.com/almide/json.git", tag = "v2.0.0" }
```

### CLI

```bash
almide add almide/almide-bindgen          # github.com/almide/ がデフォルト
almide add almide/almide-bindgen@v0.1.0   # バージョン指定
almide add user/repo                      # 任意の GitHub リポジトリ
almide deps                               # 依存一覧
almide dep-path bindgen                   # キャッシュディレクトリを出力
```

### almide.lock

`almide.lock` は正確なコミットハッシュを記録する。存在すればそのコミットを使い、なければ tag/branch の HEAD をフェッチして生成。VCS にコミットすべき。

### キャッシュ

`~/.almide/cache/{name}/{tag_or_commit}/` にクローンされる。`almide clean` でクリア。

### バージョン解決

**Minimal Version Selection (MVS):** 複数の依存が同じパッケージを要求する場合、要求される最小バージョンの最大値を選択。SAT ソルバー不要、決定的。

---

## 10. Codegen

### Rust ターゲット

```rust
// トップレベル関数
pub fn almide_rt_mypackage_version() -> String { ... }

// サブモジュール関数（ドット → アンダースコア）
pub fn almide_rt_mypackage_utils_format(s: String) -> String { ... }

// 深いサブモジュール
pub fn almide_rt_mypackage_bindings_python_generate(iface: String) -> String { ... }
```

### バージョン付き（異 major 共存時）

```rust
pub fn almide_rt_mypackage_v2_version() -> String { ... }
```

`IrModule.versioned_name` が設定されている場合、codegen プレフィックスに使われる。

### struct / enum

```rust
#[derive(Clone, Debug, PartialEq, PartialOrd)]
pub struct Logger {
    pub level: i64,
    pub name: String,
}
```

`--repr-c` フラグで `#[repr(C)]` 付き出力。Module Interface JSON に ABI 情報（size, align, field offset）を含む。

---

## 11. @extern

ターゲット固有の実装に委譲する。

```almide
@extern(rs, "std::cmp", "min")
fn my_min(a: Int, b: Int) -> Int = if a < b then a else b   // フォールバック body

@extern(rs, "std::cmp", "max")
@extern(ts, "Math", "max")
fn my_max(a: Int, b: Int) -> Int   // body なし: 全ターゲットに @extern 必須
```

---

## 12. ファイル解決順序

`import pkg` の解決:

1. `{base_dir}/pkg.almd`
2. `{base_dir}/pkg/mod.almd`
3. `{base_dir}/pkg/src/mod.almd`
4. `{base_dir}/pkg/src/lib.almd` (非推奨)
5. `almide.toml` の `[dependencies]` → `~/.almide/cache/{name}/...`

依存パッケージの `src/mod.almd` が見つかった場合、同ディレクトリのサブモジュールとサブディレクトリを再帰スキャン。

---

## テスト

| ファイル | テスト数 | カバー範囲 |
|---|---|---|
| `spec/integration/modules/diamond_test.almd` | 11 | ダイヤモンド依存、型同一性、サブモジュール、4段ドット |
| `spec/integration/modules/vis_effect_test.almd` | 2 | effect fn のクロスモジュール呼び出し |
| `spec/integration/modules/vis_mod_error_test.almd` | error | `mod fn` の外部アクセス拒否 |
| `spec/integration/modules/vis_local_error_test.almd` | error | `local fn` の外部アクセス拒否 |
| `spec/integration/modules/phantom_dep_error_test.almd` | error | 推移的依存の直接アクセス拒否 |

### テスト用パッケージ

| パッケージ | 構造 | 目的 |
|---|---|---|
| `mylib` | mod.almd + parser + formatter + utils | サブモジュール基本 |
| `deeplib` | mod.almd + http/mod.almd + http/client.almd | 3段ネスト |
| `dmod_b`, `dmod_c`, `dmod_d` | B→D, C→D のダイヤモンド。D に型定義 + サブモジュール | ダイヤモンド + 型同一性 |
| `dmod_d/nested/deep.almd` | 4段ドット呼び出し | mod.almd なしディレクトリの名前空間 |
| `extlib` | fn + mod fn + local fn | 可視性 |
| `effectlib` | effect fn + pure fn | エフェクト関数 |
| `nomod_lib` | parser.almd のみ（mod.almd なし） | トップレベルなしパッケージ |
