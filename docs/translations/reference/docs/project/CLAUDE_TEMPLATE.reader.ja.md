# Almide (.almd)

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](CLAUDE_TEMPLATE.reader.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](CLAUDE_TEMPLATE.reader.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](CLAUDE_TEMPLATE.reader.zh-CN.md)

> [正本](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/project/CLAUDE_TEMPLATE.md)（固定コミット 35297a612fb29e3f78f6587aba253aa91d51f466）の非規範・不活性な読者用スナップショット。履歴参照資料であり、有効なエージェント指示や生成用プロジェクトテンプレートではなく、コマンド実行権限を与えない。自動同期されない。原文の主張は独立した現時点の測定ではなく、コマンド実行やモデル呼び出しは行っていない。

<a id="stop--read-these-rules-first"></a>

## STOP — 最初にこれらのルールを読むこと

Almide は Rust／TypeScript／Python ではない。よくある間違いは次のとおり：

```
WRONG                          RIGHT
─────────────────────────────  ──────────────────────────────
a && b                         a and b
a || b                         a or b
!x                             not x
if cond { ... }                if cond then expr else expr
|x| x + 1                     (x) => x + 1
let mut x = 0                  var x = 0
return expr                    (last expression IS the return)
fn foo() -> Int { ... }        fn foo() -> Int = { ... }
import { json }                import json
struct Foo { x: Int }          type Foo = { x: Int }
x.len()                        WORKS! (UFCS — auto-resolves by type)
try { } catch(e) { }          match result { Ok(v) => ..., Err(e) => ... }
while cond { ... }             while cond { ... }  (SAME — while IS supported)
```

表の英語説明：左が誤り、右が正しい書き方。最後の式が戻り値となる。len のメソッド形式は UFCS により型から自動解決され、利用可能。while は対応しており、示された書き方は同じである。

<a id="quick-patterns"></a>

## よく使うパターン

```almide
// Function
fn add(a: Int, b: Int) -> Int = a + b

// Effect function (does I/O — returns Result; add `import fs` to the file)
effect fn read(path: String) -> Result[String, String] = {
  let text = fs.read_text(path)!
  ok(text)
}

// Main entry point (args via env.args(), Result is auto-wrapped)
effect fn main() -> Unit = {
  println("hello")
}

// If-then-else (omitting else gives Unit)
let x = if n > 0 then "pos" else "neg"

// Match
let name = match list.get(args, 1) {
  some(v) => v,
  none => "default",
}

// Lambda — params in parens, NO `fn` keyword (single or multi-arg)
let doubled = list.map(xs, (x) => x * 2)
let total = list.fold(xs, 0, (acc, x) => acc + x)

// For loop
for item in items {
  println(item)
}
for (i, item) in list.enumerate(items) {
  println(int.to_string(i) + ": " + item)
}

// While loop
var i = 0
while i < 10 {
  println(int.to_string(i))
  i = i + 1
}

// Top-level constant
let PI = 3.14159265358979323846

// Mutable state
var count = 0
count = count + 1

// List index read
let xs = [10, 20, 30]
let second = xs[1]       // 20 (direct indexing returns T; use list.get for Option[T])

// Guard (early exit)
guard list.len(args) > 1 else err("need args")

// String interpolation
let msg = "Hello ${name}, you are ${int.to_string(age)} years old"

// Record type
type Point = { x: Int, y: Int }
let p = Point { x: 1, y: 2 }

// Variant type
type Shape =
  | Circle(Float)
  | Rect(Float, Float)

// List/String concat
let combined = [1, 2] + [3, 4]
let greeting = "hello" + " " + "world"

// UFCS — the checker resolves by receiver type and scope, not an unconditional rewrite
let n = "hello".len()          // same as string.len("hello")
let m = [1, 2, 3].len()       // same as list.len([1, 2, 3])
let r = "hello".split(" ").reverse()  // chaining works too
```

コードコメントの説明（コード自体は原文のまま）：

- 通常の関数と、I/O を行う effect 関数を示す。後者は Result を返し、ファイルに fs の import を追加する。
- main はエントリーポイント。引数は env.args() 経由で、Result は自動で包まれる。
- if-then-else で else を省略すると Unit となる。その後に match の例がある。
- ラムダの引数は括弧に入れ、単一・複数引数とも fn キーワードを付けない。
- for ループ、while ループ、トップレベル定数、可変状態を示す。
- リストの直接インデックス読み取りは T を返す。Option[T] が必要なら list.get を使う。例の 2 番目の要素は 20。
- guard は早期脱出。文字列補間、レコード型、バリアント型、リスト／文字列連結の例が続く。
- UFCS はチェッカーがレシーバー型とスコープから解決し、無条件の書き換えではない。文字列の len は string.len と、リストの len は list.len と同じで、チェーンも利用できる。

<a id="stdlib-quick-reference"></a>

## 標準ライブラリ早見表

現在のシグネチャには[生成された標準ライブラリリファレンス](https://github.com/almide/almide/blob/develop/docs/stdlib/README.md)を、import と呼び出しの例には [CHEATSHEET](https://github.com/almide/almide/blob/develop/docs/CHEATSHEET.md) を使う。コアモジュールはモジュール名として自動 import されるのであって、すべての関数が非修飾名で import されるわけではない。
プラットフォーム／opt-in モジュールには明示的な import が必要。旧 `error` モジュールは廃止された。`io.read_all()` は引き続き利用可能で、`String` を返す。effect 文脈での呼び出し構文とターゲットごとの動作は IO リファレンスを参照する。

この抜粋した名前の索引は CI で標準ライブラリのソースと照合される。シグネチャと可用性についての正本は、引き続き生成されたリファレンスである。

```
AUTO-IMPORTED (no import needed):
  string: len trim split lines contains starts_with ends_with replace
          index_of slice to_upper to_lower to_list pad_start pad_end
  list:   len get first last map flat_map filter find fold enumerate
          zip sort sort_by reverse any all take drop unique join sum
  map:    new get set contains remove keys values to_list from_list merge len
  set:    new insert contains remove union intersection len
  int:    to_string parse parse_radix abs min max clamp pow to_hex from from_checked from_saturating
  float:  to_string from abs min max pow round floor ceil
  math:   sqrt log sin cos pi e
  value:  get get_string get_int get_bool get_float get_array get_path as_object
  datetime, value, option, result, bytes — also auto-imported

IMPORT REQUIRED (effectful / platform / opt-in):
  import fs      — read_text write_text read_lines append_text exists mkdir_p remove list_dir stat temp_dir
  import env     — get set args cwd os sleep_ms
  import process — exec output run
  import io      — read_line print read_all
  import json    — parse stringify stringify_pretty
  import random  — int float shuffle
  import regex   — is_match find find_all replace split captures
  import path    — join dirname basename extension stem normalize is_absolute
  import args    — has_flag option positional
```

索引の見出しの意味：前半は「自動 import（import 不要）」、後半は「import 必須（effectful／プラットフォーム／opt-in）」。datetime、value、option、result、bytes も自動 import の対象である。

---

この資料は実用の CLAUDE.md や生成器入力ではない。出力だけを行う effect main、WRONG/RIGHT 図の大文字表記、旧 API 名などの歴史的な例を、黙って現代化せず保持する。develop への可変リンクは原文のリンクとして残しており、現行シグネチャの固定証拠ではない。
