# path

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](path.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](path.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](path.zh-CN.md)



> 原文：[path.md @ 35297a6](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/path.md)。読者向けの非規範的な固定スナップショットです。API 索引は原文固定時点の内容であり、この訳は自動生成・自動同期の対象ではありません。署名・型・`@since` とコード例（コメントを含む）は原文のまま保持し、API 説明の訳を末尾に併記しています。コード例はこの翻訳作業では独立実行していません。

パス文字列の操作。`import path`。

以下の関数はすべて純粋です。`path` はファイルシステムに一切触れません。パスの*文字列*を操作するので両ターゲットで同じ動作をし、ケイパビリティも不要です。読み書きを行うものには `fs` を使います。

<a id="path-components"></a>

## パスの構成要素

### `path.join_within(base: String, child: String) -> Result[String, String]`

`base` 内に留まる相対 `child` なら `path.join(base, child)`、それ以外（絶対パスの child、どちらの区切り文字でも `..`、`~`、ドライブ文字、NUL）なら `err("path traversal rejected: <child>")` を返し、結果を字句上 `base` の配下に保ちます。これはファイルシステム上の包含検査ではありません。シンボリックリンクは別の場所へ導くことがあります。信頼できないファイルシステムアクセスには、ルートを固定した `fs` 操作を使い、プラットフォーム固有の包含制限を確認してください。記述子ベースの実装は、対応する64ビット Linux/macOS ホストに限定されます。フォールバックするプラットフォームには同じ競合耐性がありません。

```almd run
import path

fn main() -> Unit = {
  println(path.join_within("/srv/uploads", "a/b.txt") ?? "refused")
  println(path.join_within("/srv/uploads", "/etc/passwd") ?? "refused")
  println(path.join_within("/srv/uploads", "../../etc/passwd") ?? "refused")
}
```
```output
/srv/uploads/a/b.txt
refused
refused
```

### `path.join(base: String, child: String) -> String`

2つのセグメントを結合します。`base` の末尾の区切り文字を1つ除いてから、区切り文字と child を追加します。絶対パスの `child` は `base` を置き換え、`child` 内の `..` はその外へ出ます。信頼境界を越えて来た child には `path.join_within` を使ってください。

```almd run
import path

fn main() -> Unit = {
  println(path.join("/usr", "bin"))
  println(path.join("/usr/", "bin"))
}
```
```output
/usr/bin
/usr/bin
```

### `path.dirname(path: String) -> String`

最後の区切り文字より前のすべてです。

```almd run
import path

fn main() -> Unit = {
  println(path.dirname("/usr/bin/node"))
}
```
```output
/usr/bin
```

### `path.basename(path: String) -> String`

最後の空でないパスセグメントです。`/` または空のパスでは空になります。

```almd run
import path

fn main() -> Unit = {
  println(path.basename("/usr/bin/node"))
}
```
```output
node
```

### `path.stem(path: String) -> String`

ベース名から拡張子を取り除いたものです。

```almd run
import path

fn main() -> Unit = {
  println(path.stem("/tmp/report.tar.gz"))
}
```
```output
report.tar
```

### `path.extension(path: String) -> Option[String]`

ドットを含まない拡張子です。なければ `none`。ドットで始まるファイル名（`.gitignore`）には拡張子がありません。

```almd run
import path

fn show(o: Option[String]) -> String = match o {
  some(s) => "some(\"${s}\")",
  none => "none",
}

fn main() -> Unit = {
  println(show(path.extension("file.txt")))
  println(show(path.extension("Makefile")))
  println(show(path.extension(".gitignore")))
  println(show(path.extension(".bashrc.bak")))
}
```
```output
some("txt")
none
none
some("bak")
```

### `path.is_absolute(path: String) -> Bool`

パスがファイルシステムのルートから始まるとき true です。

```almd run
import path

fn main() -> Unit = {
  println("${path.is_absolute("/usr")}")
  println("${path.is_absolute("relative/path")}")
}
```
```output
true
false
```

### `path.normalize(path: String) -> String`

`.` セグメントを取り除き、`..` を直前のセグメントに対して解決し、重複する区切り文字を圧縮します。純粋に字句的な操作で、シンボリックリンク解決もファイルシステムアクセスもないため、リンクが関係すると OS の結果と異なる場合があります。コストはパスの長さに対して線形です。

```almd run
import path

fn main() -> Unit = {
  println(path.normalize("/a/./b/../c"))
}
```
```output
/a/c
```



<a id="signature-index-11-functions"></a>

## 固定スナップショットの API 索引（11関数）

```
// base/child; an absolute child replaces base.
// @since 0.2.0 or earlier
path.join(base: String, child: String) -> String

// base/child; err if child could leave base.
// @since unreleased
path.join_within(base: String, child: String) -> Result[String, String]

// path minus last segment; "" if no slash.
// @since 0.2.0 or earlier
path.dirname(path: String) -> String

// Last non-empty segment; "" for / or empty path.
// @since 0.2.0 or earlier
path.basename(path: String) -> String

// After basename's last dot; none if no dot or dotfile.
// @since 0.2.0 or earlier
path.extension(path: String) -> Option[String]

// True when path starts with /.
// @since 0.2.0 or earlier
path.is_absolute(path: String) -> Bool

// Basename minus last extension; dotfiles as-is.
// @since 0.3.0 or earlier
path.stem(path: String) -> String

// Resolves . and .. lexically; "" becomes ".".
// @since 0.3.0 or earlier
path.normalize(path: String) -> String

// Segments of normalize(path), led by "/" when absolute; ["."] for "" or ".".
// @since unreleased
path.components(path: String) -> List[String]

// path relative to base, lexically; "." when equal; none when not under base.
// @since unreleased
path.relative_to(path: String, base: String) -> Option[String]

// path with its last extension replaced by ext; "" removes it; unchanged without a file name.
// @since unreleased
path.with_extension(path: String, ext: String) -> String
```




## API 説明の訳（上記署名順）

- `path.join(base: String, child: String) -> String` — base/child。絶対パスの child は base を置き換える。
- `path.join_within(base: String, child: String) -> Result[String, String]` — base/child。child が base 外へ出られる場合は err。
- `path.dirname(path: String) -> String` — 最後のセグメントを除いた path。スラッシュがなければ ""。
- `path.basename(path: String) -> String` — 最後の空でないセグメント。/ または空パスなら ""。
- `path.extension(path: String) -> Option[String]` — ベース名の最後のドットの後。ドットなし、またはドットファイルなら none。
- `path.is_absolute(path: String) -> Bool` — path が / から始まると true。
- `path.stem(path: String) -> String` — 最後の拡張子を除いたベース名。ドットファイルはそのまま。
- `path.normalize(path: String) -> String` — . と .. を字句的に解決。"" は "." になる。
- `path.components(path: String) -> List[String]` — normalize(path) のセグメント。絶対パスなら先頭は "/"。"" または "." なら ["."]。
- `path.relative_to(path: String, base: String) -> Option[String]` — base に対する path の字句的な相対パス。等しければ "."、base 配下でなければ none。
- `path.with_extension(path: String, ext: String) -> String` — path の最後の拡張子を ext に置き換える。"" なら削除。ファイル名がなければ変更なし。
