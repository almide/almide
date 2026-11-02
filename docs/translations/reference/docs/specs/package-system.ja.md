<a id="package-system-specification"></a>

# パッケージシステム仕様

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](package-system.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](package-system.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](package-system.zh-CN.md)

> 非規範の固定読者用スナップショット。固定原文を正本とし、自動同期や独立した実行検証を意味しない。§10 のレジストリは将来計画であり、サービス実装済みの主張ではない。信頼とネイティブコードの許可は原文が説明する仕組みであり、安全性監査やサンドボックスの保証ではない。§6.4 の「三つの経路」に対して箇条書きは四つある原文の差異を保持する。
> [Source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/package-system.md)


> Almide の依存関係管理、バージョン解決、モジュール分離。
> Go Modules（MVS）、Cargo（型の同一性）、pnpm（境界の強制）を参考にした設計。

<a id="1-design-principles"></a>

## 1. 設計原則

1. **Minimal Version Selection** — 常にすべての制約を満たす最小バージョンを選ぶ。SAT ソルバーを使わない。決定的で再現可能、高速。（Go Modules）
2. **厳格なモジュール境界** — コードがアクセスできるのは直接 import したモジュールのみ。推移的依存は見えない。幻の依存関係はない。（pnpm）
3. **(name, major) による型の同一性** — 同じ `(name, major)` を持つ二つのパッケージは同じ型を生成する。major が異なれば互換性のない型になる。（Cargo）
4. **単一の正本** — `almide.toml` は意図を宣言し、`almide.lock` は正確なコミットを記録する。一致するロックを尊重する。ソース変更には明示的な再ロックが必要で、`--locked` は変更を拒否する。
5. **将来のレジストリに備える** — 現在の git ベースの仕組みに Almide パッケージレジストリを追加する予定である。設計判断でこれを妨げてはならない。

<a id="2-package-identity"></a>

## 2. パッケージの同一性

```text
PkgId { name: String, major: u64 }
```

- `almide.toml` の `[package] version` または git タグから導出する。
- `0.x.y` では minor を major として使う（1.0 より前の破壊的変更）。
- `mod_name()` はコード生成のシンボル名前空間用に `"{name}_v{major}"` を返す。

<a id="3-version-resolution-mvs"></a>

## 3. バージョン解決：MVS

複数の依存元が同じパッケージ（同じ名前、同じ major）を要求した場合：

```text
B requires D >= 1.2.0
C requires D >= 1.5.0
→ resolve to D 1.5.0 (maximum of minimums)
```

major が異なる場合：

```text
B requires D v1.x (major=1)
C requires D v2.x (major=2)
→ both coexist as separate modules (D_v1, D_v2)
→ D_v1.Logger ≠ D_v2.Logger (different types)
```

<a id="4-module-boundaries"></a>

## 4. モジュール境界

<a id="41-direct-vs-transitive"></a>

### 4.1 直接依存と推移的依存

```text
A imports B, C
B imports D
C imports D
```

A から見た場合：
- `B.func()` ✓ — 直接依存
- `C.func()` ✓ — 直接依存
- `D.func()` ✗ — 推移的依存のみ（A も D を import している場合を除く）
- `D.Logger` ✗ — A が D を import しなければ型は見えない

A が D の型を必要とするなら、明示的に `import D` を宣言しなければならない。

<a id="42-enforcement"></a>

### 4.2 強制方法

型検査器は、各ファイルが `import` 文で取り込んだモジュールを追跡する。
`resolve_module_call` と `resolve_static_member` は、グローバルな `user_modules` ではなく、この集合に対して検査する。

<a id="43-re-exports"></a>

### 4.3 再エクスポート

モジュールはラップすることで依存先の型を再エクスポートできる：

```almide project
// file: d/mod.almd
type Logger = { name: String }
fn make(n: String) -> Logger = Logger { name: n }
// file: b/mod.almd
import d
type Logger = d.Logger  // re-export (future: explicit pub use)
fn make(n: String) -> Logger = d.make(n)
fn name_of(l: Logger) -> String = l.name
// file: main.almd
import b

test "the consumer sees D's type through B without importing D" {
  let l: b.Logger = b.make("x")
  assert_eq(b.name_of(l), "x")
}
```

<a id="5-codegen-versioned-symbols"></a>

## 5. コード生成：バージョン付きシンボル

`IrModule.versioned_name` が設定されている場合（例：`"json_v2"`）、コード生成ではそれを関数の接頭辞に使う：

```rust
// Without versioning (same major, no conflict)
pub fn almide_rt_json_parse(...) { ... }

// With versioning (different majors coexist)
pub fn almide_rt_json_v1_parse(...) { ... }
pub fn almide_rt_json_v2_parse(...) { ... }
```

型の衝突を防ぐため、構造体名にもバージョンを付ける：

```rust
pub struct JsonV1_Config { ... }
pub struct JsonV2_Config { ... }
```

<a id="6-dependency-declaration"></a>

## 6. 依存関係の宣言

```toml
# almide.toml
[package]
name = "myapp"
version = "0.1.0"

[dependencies]
bindgen = { git = "https://github.com/almide/almide-bindgen", tag = "v0.1.0" }
json = { git = "https://github.com/almide/json", tag = "v2.0.0" }
```

**各キーは同じテーブル内に一度だけ、各テーブルも一度だけ現れる**（#2583）。TOML はキーの重複を禁止する。`almide.toml` を読むすべてのコマンド（`check`、`run`、`test`、`build`、…）は、何かを取得したりロックに触れたりする前に重複を拒否し、両方の行を示す：

```text
error: almide.toml:7: dependency `almai` is declared twice in [dependencies] (first at line 6)
  hint: keep one of lines 6 and 7 and delete the other — TOML allows a key only once per table
```

この規則はマニフェストリーダーが読むすべてのテーブル（`[package]`、`[dependencies]`、`[permissions]`、`[native-deps]`）に適用され、テーブルヘッダーの重複も含む。同じ URL の二通りの表記（`…/almai` と `…/almai.git`）も、同じ依存関係の二重宣言である。

テスト：`tests/manifest_duplicate_key_test.rs`。

**`[package] almide = "X.Y.Z"` は最小コンパイラバージョンである**（#4172）。単純なバージョンまたは `">=X.Y.Z"` として書く。それ以外の演算子は無効な固定指定である。パッケージを検査またはコンパイルするすべてのコマンドは、ソースを読む前に古いコンパイラを拒否する。対象はすべてのターゲットの `check`、`emit`、`compile`、`build`、`run`、`test` と、`bench`、`fix`、`survive`、`apply`、`verify` である。以前は native ビルドだけがこれを読み、`check` と wasm ターゲットは任意のコンパイラでコンパイルしていた。パッケージ管理（`add`、`deps`、`update`、`install`）と `fmt` は引き続き実行できるので、固定指定を読んで変更できる。

```text
error: package 'myapp' requires almide >= 0.70.0
  installed version: 0.69.0
```

`ALMIDE_SKIP_VERSION_CHECK=1` でこれを迂回する。リリースビルドでは `--allow-switch ALMIDE_SKIP_VERSION_CHECK` を伴う場合に限って有効になる（#4129）。

テスト：`tests/version_pin_commands_test.rs`。

**どのリーダーも読まないキーは警告になる**（#3382）。以前は無言で捨てられたため、依存関係に `brnach = "main"` や `tags = "v0.1.0"` と書くと、記載された参照ではなく既定の参照からビルドしていた。現在、`almide.toml` を読む各コマンドは一度警告し、行、キー、そのテーブルが受け付けるキー、および存在すれば編集距離が一つのキー（挿入・削除・置換・隣接文字の入れ替え）を示す：

```text
warning: unknown key `brnach` in dependency `foo` is ignored — it changes nothing
  --> almide.toml:5
  hint: did you mean `branch`? The keys dependency `foo` accepts are `git`, `tag`, `branch`, `version`, `path`, `subdir`
```

| テーブル | 受け付けるキー |
|---|---|
| トップレベル | `[package]`, `[dependencies]`, `[native-deps]`, `[permissions]`, `[target]`, `[trust]` |
| `[package]` | `name`, `version`, `almide`。加えて、パッケージの説明用でどこからも読まれない `description`, `edition`, `license`, `repository`（警告なし） |
| `[dependencies]` の項目（インラインテーブル、ドット付きキー、または `[dependencies.<name>]`） | `git`, `tag`, `branch`, `version`, `path`, `subdir` |
| `[permissions]` | `allow`, `proc`, `native`, `fs` |
| `[trust]` | `owners` |

テーブルではない依存項目や、`git` と `path` のどちらも指定しない依存項目は何も宣言せず、同様に警告される。`[native-deps]` の項目は crate 名なので、どのキーも crate 名とみなす（その仕様の判断は Cargo に委ねる）。`[target.<platform>]` 内の未知のテーブルは、既にエラーである（§6.2）。

キーがない場合とまったく同じようにビルドは続行し、終了コードも変わらない（`check --deny-warnings` もこれを数えない）。`almide check
--json` は stdout 上の `"level":"warning"` 行として警告を含め、ほかのすべてのコマンドは stderr に出力する。警告を含むリリースを経た後、将来の dialect epoch でエラーへ格上げする予定だが、epoch の登録はまだない。キー一覧は `src/project.rs` 内の各リーダーの隣にあり、`tests/manifest_unknown_key_test.rs` は列挙されたすべてのキーが解析結果を変えること（そしてすべてのメタデータキーが変えないこと）を検証するため、一覧がリーダーから乖離することを防ぐ。

短縮形式（既定は github.com/almide/）：
```bash
almide add bindgen@v0.1.0
# → bindgen = { git = "https://github.com/almide/almide-bindgen", tag = "v0.1.0" }
```

一要素の名前は github.com/almide 配下のパッケージを指す。パッケージ名とリポジトリ名は異なる場合がある（`bindgen` は `almide-bindgen` にある）ため、コンパイラに埋め込まれた組織のパッケージ一覧スナップショット（`src/almide_packages.txt`、`scripts/gen-almide-packages.sh` で再生成）で名前を検索する。一覧にない名前は `github.com/almide/<name>` に解決する。`almide add almide/<repo>` は、そのリポジトリについて一覧が示すパッケージ名を使うため、`almide add almide/almide-bindgen` も `bindgen` を追加する（#4093）。

<a id="61-a-package-in-a-subdirectory-of-the-repository-subdir"></a>

### 6.1 リポジトリのサブディレクトリ内のパッケージ：`subdir`

一つのリポジトリに、それぞれ独自のディレクトリと `almide.toml` を持つ複数のパッケージを配置できる。git 依存は `subdir` でその一つを指定する（#3381）：

```toml
[dependencies]
ceangal = { git = "https://github.com/almide-graphics/ceangal2", tag = "v0.1.0", subdir = "ceangal" }
snaidhm = { git = "https://github.com/almide-graphics/ceangal2", tag = "v0.1.0", subdir = "snaidhm" }
```

指定名のパッケージをリポジトリから走査して探す方式（Cargo の規則）ではなく、パッケージを明示的に指定する。マニフェストはビルドするディレクトリを正確に示す。リポジトリにほかに何があるかに依存せず、`almide.toml` の読者は取得しなくても確認できる。

- **表記。** `subdir` はリポジトリルートに対する相対パスで、どのプラットフォームでも `/` 区切りを使う。使用前に正規化し、`./`、連続する `/`、末尾の `/` を除く（`./pkgs//c/` は `pkgs/c`）。ロックには正規化後の表記を記録する。`almide.toml` を読むすべてのコマンドは、文字列でない、空、ルート自体を指す（`.` — 代わりに `subdir` を削除する）、絶対パス（`/x`、`C:/x`）、`\` を使う、または `..` を含む `subdir` を、記載行を示して拒否する（`a/../b` も不可。一つのディレクトリには一つの表記）。
- **git 専用。** `path` と並ぶ `subdir` は拒否する。`path` は既にパッケージディレクトリを指すためで、ヒントに結合済みの `path` を示す。
- **取得時に検査。** ディレクトリはチェックアウト内に存在し、その内側に留まり（クローン外へのシンボリックリンクは拒否）、依存項目のキーと `[package] name` が一致する `almide.toml` を持つ必要がある。キーが import 名になる。各拒否はマニフェストの行と、リポジトリ内に存在するパッケージディレクトリ（深さ二段まで）を示す：

```text
error: almide.toml:6: dependency `ceangal` names subdir `snaidhm`, whose package is `snaidhm`
  hint: the dependency key is the import name and must be the package's name — rename the key to `snaidhm`, or point `subdir` at the package `ceangal`
error: almide.toml:6: subdir `pkgs/ceangal` of dependency `ceangal` does not exist in https://… at v0.1.0
  hint: the packages in this repository are `ceangal`, `snaidhm` — point `subdir` at one
```

  `subdir` のない git 依存で、リポジトリルートがパッケージではなく（`almide.toml`、`src/`、`.almd` ファイルがない）、サブディレクトリにパッケージがある場合も、追加すべき `subdir = "…"` を示して同様に拒否する。
- **同一性。** パッケージとはソースとその中のディレクトリである。同じリポジトリの二つの subdir は別のパッケージで、各自の要件について別々に走査し、二つの項目としてロックする。`PkgId`（name, major）と MVS は変わらない。名前は引き続きパッケージ自身の `[package] name` である。
- **クローンは一つ。** チェックアウトは `~/.almide/cache/.sources/<encoded-source>/@/<ref-or-commit>/` の下にソースをキーとして置く。同じソース・参照に依存するものは `subdir` の有無にかかわらずチェックアウトを共有し、それぞれその中のパッケージディレクトリを読む。旧 `.repos/.src-<hash>` と名前をキーにした配置は再利用しない（§6.3 参照）。
- **CLI。** `almide add <repo>[@<tag>] --subdir <dir>` は subdir の最後の要素をパッケージ名にする（`--subdir pkgs/ceangal` → `ceangal`）。`--git <url>` の場合は最初の引数が名前になる。`add` は `almide.toml` を書く**前に**パッケージを取得して検査するため、誤った `subdir` でもマニフェストは変更されない。`almide deps` は `… (<ref>) subdir <dir>` を、`almide dep-path <name>` はクローン内のパッケージディレクトリ（`src/` がある場合はそのディレクトリ）を表示する。`almide update` はほかの git 依存と同様にその項目を更新する。

テスト：`tests/dep_git_subdir_test.rs`（二つのパッケージを持つローカルリポジトリ：check、lock、一つの共有クローン、`dep-path`、`deps`、`add --subdir`、すべての拒否）、`project_fetch` 単体テスト（`package_dir_in_checks_the_subdir_against_the_checkout`、`subdir_packages_of_one_repository_share_a_clone_root`）、`tests/lock_roundtrip_test.rs`。

<a id="62-native-rust-dependencies-native-deps"></a>

### 6.2 ネイティブ Rust 依存：`[native-deps]`

`native/*.rs` モジュールまたは `@extern(rust, …)` 関数を持つパッケージは、それらが使う crates.io の crate を `[native-deps]` に宣言する。各項目は、すべての native ビルド経路（バイナリ、`--cdylib`、`--repr-c`、`almide run`、`almide test`）で生成 crate の Cargo.toml にそのままコピーされる。依存パッケージの `[native-deps]` も同様に追加する。

```toml
[native-deps]
anyhow = "1"
serde = { version = "1", features = ["derive"] }
```

**ターゲット別 native 依存**（#3350）。一部のターゲットでしかビルドできない crate は、Cargo の構文と意味を使う `[target.<platform>.native-deps]` に置く。これは `[target.<platform>.dependencies]` として出力されるため、ビルドターゲット（ホスト、または `--target <triple>`）が一致する場合だけ Cargo がその crate をコンパイルする：

```toml
[target.'cfg(not(any(target_os = "android", target_os = "ios")))'.native-deps]
arboard = "3"

[target.'cfg(target_os = "android")'.native-deps]
jni = "0.21"

[target.x86_64-pc-windows-gnu.native-deps]
winapi = "0.3"
```

`<platform>` は、Cargo が受け付けるとおりの `cfg(<expr>)` またはターゲットトリプルである。`<expr>` は `all(…)`、`any(…)`、`not(…)`、名前（`unix`）、または `name = "value"`。`almide.toml` を読むすべてのコマンドは、Cargo が拒否するキーを拒否し、そのキーと行を示す：

```text
error: almide.toml:4: invalid platform `cfg(target_os = android)` in [target.'cfg(target_os = android)'.native-deps]: expected a string after `target_os =`, found `android`
```

ターゲット別にできるのは `native-deps` だけで、`[target.<platform>]` 内のほかのテーブルはエラーとなる。ランタイム自身が必要とする crate（例：`zlib` 用の `flate2`、`http` 用の TLS crate）は常にすべてのターゲット向けに宣言する。同じ crate を特定ターゲット向けにも宣言した場合は、両方の宣言を書き、Cargo がマージする。

テスト：`tests/manifest_toml_reader_test.rs`、`tests/native_deps_target_cfg_test.rs`、および `cargo_build` 単体テスト。

<a id="63-trusted-sources-trust-owners"></a>

### 6.3 信頼するソース：`[trust] owners`

モデルは実在しないリポジトリ名を作り、その名前が複数回の実行で繰り返されるため、攻撃者はモデルが要求する前にその名前を登録できる（#4093）。このためツールチェーンは、**ルート**のマニフェストが信頼する所有者からのみ git 依存を取得する：

- `github.com/almide` は既定で信頼される。
- それ以外の所有者は、どのホストでも `[trust] owners` に `host/owner` として列挙する。
- `path =` 依存およびローカルファイルシステム上の git ソース（`file://…` または単純なパス）は、その場所に名前を登録する者がいないため対象外とする。
- 依存先自身の `[trust]` は決して読まない。パッケージがルートの代わりに信頼を付与することはできない。

```toml
[dependencies]
csv   = { git = "https://github.com/almide/csv", tag = "v0.1.0" }   # trusted by default
mylib = { git = "https://github.com/alice/mylib", tag = "v1.2.0" }

[trust]
owners = ["github.com/alice"]
```

検査は `almide add` とすべての取得（`build`、`run`、`test`、`bench`、`update`）で、ネットワークアクセスの前に行う。直接・推移的・既にロック済みの依存に等しく適用し、ロックは許可ではない。所有者は大文字小文字を区別せず比較する。パスに `.`、`..` の要素またはパーセントエスケープを含む URL は、その文字列から取得元の所有者を読み取れないため無条件に拒否する。すべてを信頼するフラグや環境変数はない。許可は常に、変更のほかの部分と一緒にレビューされるファイルで所有者を明示する。

**`almide add`** は指定を解決し、所有者を検査し、リポジトリの存在を確認（`git ls-remote`、プロンプト無効）して取得し、その後で初めて `almide.toml` と `almide.lock` を書く。拒否または失敗は終了コード 1 となり、両ファイルをバイト単位で変更しない：

```text
$ almide add alice/mylib@v1.2.0
Resolved mylib → https://github.com/alice/mylib (owner github.com/alice, NOT trusted)
error: refusing to add `mylib`: its owner is not trusted by this project
  source: https://github.com/alice/mylib
  owner: github.com/alice (not in [trust] owners)
  note: models often invent repository names, and attackers register the names models invent
  hint: if the user asked for this exact repository, run:
          almide add alice/mylib@v1.2.0 --trust github.com/alice
  hint: if you chose this name yourself, do not trust it; use a package from github.com/almide or ask the user
```

- `--trust <owner>` はソースの解決先の所有者（`host/owner`、github.com では `owner` も可）を繰り返し指定する必要がある。別の所有者ならエラーで、所有者を `[trust] owners` に書き込む。
- `--dry-run` は解決結果、信頼判定、リポジトリの存在有無を表示し、書き込みも取得も行わない（追加が拒否される場合は終了コード 1）。
- 存在しないリポジトリは、その旨を報告する。拒否または未発見の場合に限り、近い名前を実行可能な行（`hint: did you mean \`almide add csv\`?`）とともに提案する。対象は、別の所有者の下にある同名の almide パッケージ、大文字小文字・`-`・`_`・`.`・`almide-` 接頭辞を正規化した後で編集距離が 2 以内かつ名前の三分の一以内の almide パッケージ、または `almide` に近い綴りの所有者である。自動置換はしない。

**推移的な依存または手書きの依存**の所有者が信頼されていない場合、そこへ至る依存チェーンを示してビルドを停止する。許可する操作は `almide trust <owner>` の一手順である：

```text
error: dependency `util` (required by mylib → util) is fetched from an owner this project does not trust
  source: https://github.com/bob/util
  owner: github.com/bob (not in [trust] owners)
  ...
  hint: if this dependency is expected, run `almide trust github.com/bob` (it adds "github.com/bob" to [trust] owners in almide.toml)
```

**ロックを残したままのソース変更**は拒否する。直接依存のロック項目の `git` がマニフェストと異なる場合、`` `csv` is locked to <old> but almide.toml now says <new> `` とヒント `almide update csv` を示してビルドを停止する。この操作はタグ固定の依存も含め、新しいソースから再ロックする。以前は無言で再解決していた。

`build`、`run`、`test` の **`--locked`** は、ネットワークアクセスの前に `almide.lock` にない直接 git 依存を拒否し、ロックが変わる解決も拒否して、ロックを書かない。CI で使用する。（ロックが記録するのは直接依存だけで、推移的依存は従来どおり参照から解決する。）

**キャッシュのキーはソースである。** チェックアウトは `~/.almide/cache/.sources/<scheme>/<host>/<owner>/<repo>/@/<ref or commit>` に置き、URL の各部分を単射にエンコードする。そのため、`github.com/evil/svg` から取得した `svg` というパッケージが `github.com/almide/svg` の代わりに使われることはない。旧配置は URL を 64 ビット（FNV-1a）にハッシュ化していたが、これには衝突耐性がない。旧配置のディレクトリは一切読まず、`almide clean` で削除する。

信頼は意図なので `almide.toml` に置く。ロックは事実を記録し、信頼ビットを持たない。

テスト：`tests/dependency_trust_test.rs`、`trust` と `project_fetch` の単体テスト。

<a id="64-native-code-from-a-dependency-needs-a-grant-permissions-native"></a>

### 6.4 依存先のネイティブコードには許可が必要：`[permissions] native`

依存先のネイティブコードはビルド中に cargo によってコンパイル・実行され、あらゆる `[permissions]` 検査の前かつその外側で動く（#4093）。ビルドに入り込む経路は三つある：

- `native/*.rs` モジュールと `native/` サブディレクトリ内のファイルは生成 crate にコピーされる。
- `[native-deps]` と `[target.<platform>.native-deps]` の crate は生成 Cargo.toml に追加されるため、その `build.rs` と proc macro が動く。
- `@extern(rust, "path", "fn")` fn は任意の Rust パスに対する `path::fn` 呼び出しとして出力される（`std` を含み、`native/` が一切なくてもよい）。`@extern(c, …)` fn は C シンボルをリンクする。
- `@inline_rust("…")` fn のテンプレートは Rust テキストとして crate に差し込まれる（#4100）。パッケージが `@intrinsic` / `@wasm_intrinsic` を含む場合も列挙する（E085 は stdlib 外ではこれらを拒否する）。

このため `almide build`、`almide run`、`almide test`、`almide bench` は、直接依存でも別の依存を経由した依存でも、これらのいずれかを持ち込むパッケージを、ルートの `almide.toml` が `[permissions] native` に列挙していなければ拒否する。何かをコンパイルする前に、すべての `--target` で拒否し、終了コードは 1 となる（この規則は `[permissions]` 自体と同じく、個別ターゲットではなく依存グラフに属する）：

```text
error: dependency `nat` brings native code that ./almide.toml does not grant
  nat 0.1.0 at ../nat
  native code: native/natval.rs; [native-deps] itoa; @extern fn `number` (src/mod.almd:2)
  note: native code is compiled and run by cargo during the build, before and outside every [permissions] check
  hint: if you trust `nat`, grant it in ./almide.toml:
    [permissions]
    native = ["nat"]
```

```toml
[permissions]
native = ["nat"]
```

許可はバージョンではなくパッケージを指定し、そのパッケージだけに適用される。直接依存への許可は、その依存先が取り込む別パッケージのネイティブコードを許可しない。ルートマニフェストの許可だけが有効で、依存先自身の `[permissions] native` は読まない。ルートパッケージ自身の `native/`、`[native-deps]`、`@extern` fn は許可不要である。

各 native ビルドは、ネイティブコードを持ち込む許可済みの依存ごとに一行出力する：

```text
note: building native code from `nat` (granted by [permissions].native): native/natval.rs; [native-deps] itoa; @extern fn `number` (src/mod.almd:2)
```

`almide deps` は各依存（直接依存、続いてそこから到達する依存）について、バージョン、能力集合（`[permissions].allow` の検査対象カテゴリ。`@extern` fn はすべてのカテゴリとして扱う）、持ち込むネイティブコード、そのコードが許可されているかを表示する：

```text
nat = path ../nat
  version: 0.1.0
  capabilities: Abort, Env.read, Env.write, FS.read, FS.write, Fan, IO.stderr, IO.stdin, IO.stdout, Net.fetch, Net.listen, Proc, Rand, Time
  native code: native/natval.rs; [native-deps] itoa; @extern fn `number` (src/mod.almd:2) (granted by [permissions].native)
```

テスト：`tests/native_grant_test.rs`。

<a id="7-lock-file"></a>

## 7. ロックファイル

```toml
# almide.lock — auto-generated, do not edit

bindgen = { git = "https://github.com/almide/almide-bindgen", ref = "v0.1.0", commit = "a629eded8d20..." }
json = { git = "https://github.com/almide/json", ref = "v2.0.0", commit = "b8f3a1..." }
ceangal = { git = "https://github.com/almide-graphics/ceangal2", ref = "v0.1.0", commit = "c41d...", subdir = "ceangal" }
```

`subdir` 依存（§6.1）の項目は正規化済み `subdir` を記録し、ロックがパッケージを正確に指すようにする。subdir のない項目は従来どおり書く。同じリポジトリの二つのパッケージが `(git, ref, commit)` を共有してもよい。固定コミットは、その参照におけるリポジトリの事実なので、ほかの依存と同じく `(git, ref)` で検索し、両パッケージを同じコミットからビルドする。

直接依存ごとに一項目、一行とし、ライターは同じ名前を二度出力しない。同じ名前を二度持つロック（#2583 より前のコンパイラが、依存を二度宣言したマニフェストから作成したもの）は、両行と解決方法を示して拒否する。一方を削除して `git` が `almide.toml` と一致する方を残すか、ロックを削除して次回実行時に再作成させる。

**ロックは要求ではなく解決結果を記録する**（#2532）。各項目の `ref` と `commit` は、その依存パッケージに対して Minimal Version Selection が選んだバージョン、つまり実際にビルドしたチェックアウトのものであり、`Cargo.lock` と同様である。要求は `almide.toml` だけに置く。依存バージョンが引き上げられなければ両者は一致する。推移的要件によって引き上げられる場合（マニフェストが `foo@v1.0.0`、依存先が `foo@v1.1.0` を要求）、`almide.toml` がまだ `v1.0.0` でも、項目は `ref = "v1.1.0"` と v1.1.0 のコミットになる。これは乖離ではなく再解決もしない。各項目は一回の取得に属する `(git, ref, commit)` なので、常に同時に成立した三つ組となる。

固定コミットは、グラフ内の誰からであっても、同じソース・同じ参照への要求に適用されるため、ロックされたコミットこそが次のビルドでコンパイルされる。走査したが選ばれなかった要求（上記の `v1.0.0`）は参照から解決する。MVS は要件を読むためにそのマニフェストを読むが、それをビルドはしない。`almide.toml` の要求変更は従来どおり再解決し、新しい選択からロックを書き直す。#2532 より前に書かれ、ルートの要求を記録したロックも読み込める。次回実行時に、引き上げられた項目を解決後の記録へ書き直す。

テスト：`tests/manifest_duplicate_key_test.rs`、`tests/lock_roundtrip_test.rs`、`tests/dep_lock_pairing_test.rs`（`a_dependency_raised_by_mvs_is_locked_at_the_version_that_was_built`）。

<a id="8-resolution-algorithm"></a>

## 8. 解決アルゴリズム

```text
1. Parse almide.toml → direct dependencies
2. For each dep:
   a. If almide.lock has an entry for this (git, ref) → use its exact commit
   b. Else fetch tag/branch
   c. With `subdir`, the package is that directory of the checkout (§6.1)
3. Parse dep's almide.toml → transitive dependencies
4. Recurse (depth-first, leaves first)
5. Dedup by PkgId(name, major):
   - Same (name, major): keep maximum requested version (MVS)
   - Different major: both coexist with versioned names
6. Detect impossible constraints → error with explanation
7. Write almide.lock: per direct dependency, the (git, ref, commit) of the
   version step 5 selected for its package
```

<a id="9-error-messages"></a>

## 9. エラーメッセージ

```text
error: version conflict for package 'json'
  → myapp requires json >= 2.0.0 (via almide.toml)
  → bindgen requires json >= 1.0.0, < 2.0.0 (via bindgen/almide.toml)

  json v1.x and v2.x are different major versions and will coexist.
  However, json.Config from v1 cannot be passed to functions expecting json.Config from v2.

  hint: Update bindgen to a version that supports json v2.x,
        or add json v1.x as a separate dependency.
```

<a id="10-future-almide-package-registry"></a>

## 10. 将来：Almide パッケージレジストリ

レジストリでは次を予定している：
- semver メタデータを伴うパッケージのホスティング
- git URL なしの `almide add pkg` の提供
- パッケージ作者向けの `almide publish` のサポート
- 重複排除のためのコンテンツアドレス型ストレージ（pnpm のようなもの）
- パッケージ署名と来歴の強制（JSR のようなもの）

現在の git ベースの仕組みは将来に対応できる：
- `PkgId` は既に semver をサポート
- `almide.lock` は既に正確なコミットを記録
- `versioned_name` は既に共存をサポート
- 移行：`git = "..."` → `registry = "almide"`（または名前だけ）

<a id="references"></a>

## 参考資料

- [Go Modules: Minimal Version Selection](https://research.swtch.com/vgo-principles)
- [How Rust Solved Dependency Hell](https://stephencoakley.com/2019/04/24/how-rust-solved-dependency-hell)
- [PubGrub: Next-Generation Version Solving](https://nex3.medium.com/pubgrub-2fb6470504f)
- [pnpm: Flat node_modules is not the only way](https://pnpm.io/blog/2020/05/27/flat-node-modules-is-not-the-only-way)
