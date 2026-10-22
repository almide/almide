<!-- description: Stdlib redesign L2/L3 — string: one join (list.join), the UTF-8 codec moves wholly to bytes, run_length_encode and length retire, compare/byte_len/UTF-16 are added -->
# L2/L3: string

> Last updated: 2026-10-21
> 前提: [L0](L0-conventions.md)・[L1](L1-categories.md)
> 出典: Rust https://doc.rust-lang.org/std/primitive.str.html (F) ・ Go https://pkg.go.dev/strings (F) ・ Python https://docs.python.org/3/library/stdtypes.html (F) ・ TS/JS https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/String (F) ・ Gleam https://gleam-stdlib.hexdocs.pm/gleam/string.html (F) ・ Elixir https://elixir.hexdocs.pm/String.html (F) ・ OCaml https://ocaml.org/manual/latest/api/String.html (F) ・ Kotlin https://kotlinlang.org/api/core/kotlin-stdlib/kotlin.text/ (一部 F) ・ Swift https://developer.apple.com/documentation/swift/string （ページが JS 描画のため取り寄せ不可。Swift の欄は F なし）
> 実測: `almide 0.68.0 (dev, 437b8f127)`、プローブは `scratchpad/sr/probe-string/`（a〜e.almd）

読み方の約束（初出）:
- `|>`（読み: パイプ）= 左の値を右の関数の第 1 引数に差し込む
- `??`（読み: ナル合体）= 左が none / err なら右を使う
- `mut`（読み: ミュート）= 呼び出し元の `var` をその場で書き換える引数
- (F) = 今回、公式ドキュメントを取り寄せて確かめたセル

`string` はいま 50 関数です（`list` と同じく全ファイルから import なしで使えます）。すべて純粋な計算で、host の機能は使いません。native と wasm の差は契約台帳（C-xxx の string 族、`string.split("")` と RLE のコードポイント単位の契約など）で守られています。

---

## L2: 役割の比較

| 役割 | Almide | Rust | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin | Swift |
|---|---|---|---|---|---|---|---|---|---|---|
| 長さ（文字数） | `len`（コードポイント数） | `chars().count()` | `utf8.RuneCountInString` | `len` | `length`（UTF-16 単位） | `length`（書記素） (F) | `length`（書記素） (F) | — | `length`（UTF-16） | `count`（書記素） |
| 長さ（バイト数） | — | `len` (F) | `len` | `len(s.encode())` | — (F: `TextEncoder` が必要) | `byte_size` (F) | `byte_size`（Kernel）(F) | `length` (F) | `encodeToByteArray().size` | `utf8.count` |
| 空判定 | `is_empty` | `is_empty` | — | — | — | `is_empty` (F) | — | `is_empty` | `isEmpty` | `isEmpty` |
| 部分文字列を含む | `contains` | `contains` (F) | `Contains` (F) | `in` | `includes` | `contains` (F) | `contains?` (F) | `includes`（5.5〜）| `contains` | `contains` |
| 前方・後方一致 | `starts_with` / `ends_with` | `starts_with` / `ends_with` | `HasPrefix` / `HasSuffix` (F) | `startswith` / `endswith` (F) | `startsWith` / `endsWith` | `starts_with` / `ends_with` (F) | `starts_with?` / `ends_with?` (F) | `starts_with` / `ends_with` (F) | `startsWith` / `endsWith` | `hasPrefix` / `hasSuffix` |
| 位置を探す | `index_of` / `last_index_of` | `find` / `rfind` | `Index` / `LastIndex` (F) | `find` / `rfind` (F) | `indexOf` / `lastIndexOf` | — | — | `index_opt` / `rindex_opt` (F) | `indexOf` / `lastIndexOf` | `firstRange(of:)` |
| 出現数 | `count` | `matches().count()` (F) | `Count` (F) | `count` (F) | — | — | `count` (F) | — | — | — |
| 分割 | `split` / `split_once` / `lines` | `split` / `split_once` / `lines` (F) | `Split` / `Cut` / `Lines` (F) | `split` / `partition` / `splitlines` (F) | `split` (F) | `split` / `split_once` (F) | `split` / `split_at` (F) | `split_on_char` (F) | `split` / `lines` (F) | `split(separator:)` |
| 空白で分割 | — | `split_whitespace` (F) | `Fields` (F) | `split()` (F) | — (F) | — (F) | `split/1` (F) | — (F) | — | — |
| 連結 | `join`（`list.join` と重複） | `[..].join` | `Join` (F) | `sep.join` (F) | `Array.join` | `join` (F) | `Enum.join` | `concat` (F) | `joinToString` | `joined(separator:)` |
| 取り除き（空白） | `trim` / `trim_start` / `trim_end` | `trim` / `trim_start` / `trim_end` | `TrimSpace` (F) | `strip` / `lstrip` / `rstrip` (F) | `trim` / `trimStart` / `trimEnd` (F) | `trim` / `trim_start` / `trim_end` (F) | `trim` / `trim_leading` / `trim_trailing` (F) | `trim` (F) | `trim` / `trimStart` / `trimEnd` | `trimmingCharacters`（Foundation）|
| 接頭辞・接尾辞を外す | `strip_prefix` / `strip_suffix`（Option） | `strip_prefix`（Option）(F) | `CutPrefix` / `TrimPrefix` (F) | `removeprefix`（無ければそのまま）(F) | — | `remove_prefix` (F) | `replace_prefix` (F) | — | `removePrefix` | — |
| 置換 | `replace` / `replace_first` | `replace` / `replacen` | `ReplaceAll` / `Replace(n)` (F) | `replace(count)` (F) | `replaceAll` / `replace` (F) | `replace` (F) | `replace` (F) | — | `replace` / `replaceFirst` | `replacing` |
| 切り出し | `slice` / `take` / `drop` / `take_end` / `drop_end` | `[a..b]` / `get(a..b)` | `s[a:b]` | `s[a:b]` | `slice` | `slice` / `drop_start` / `drop_end` / `crop` (F) | `slice` (F) | `sub` | `substring` / `take` / `drop` | `prefix` / `dropFirst` |
| バイト範囲で切り出し | `byte_slice` | `get(a..b)` (F) | `s[a:b]` | — | — | — | `byte_slice` (F) | `sub` | — | — |
| 1 文字を取り出す | `get` / `first` / `last` | `chars().nth` | — | `s[i]` | `at` (F) | `first` / `last` (F) | `at` / `first` / `last` (F) | `get` | `get` / `first` / `last` | `first` / `last` |
| 文字のリスト | `chars` | `chars` (F) | `[]rune(s)` | `list(s)` | `[...s]` | `to_graphemes` (F) | `graphemes` / `codepoints` (F) | — | `toList` | `Array(s)` |
| 大文字・小文字 | `to_upper` / `to_lower` | `to_uppercase` / `to_lowercase` (F) | `ToUpper` / `ToLower` (F) | `upper` / `lower` (F) | `toUpperCase` / `toLowerCase` | `uppercase` / `lowercase` (F) | `upcase` / `downcase` (F) | `uppercase_ascii` | `uppercase` / `lowercase` | `uppercased` / `lowercased` |
| 先頭を大文字に | `capitalize`（残りはそのまま） | — | —（`Title` は非推奨 (F)） | `capitalize`（残りを小文字に）(F) | — (F) | `capitalise`（残りを小文字に）(F) | `capitalize`（残りを小文字に）(F) | `capitalize_ascii`（残りはそのまま）(F) | `replaceFirstChar` | `capitalized`（Foundation）|
| 詰め物 | `pad_start` / `pad_end` | — | — | `rjust` / `ljust`（1 文字だけ）(F) | `padStart` / `padEnd`（複数文字可）(F) | `pad_start` / `pad_end`（複数文字可）(F) | `pad_leading` / `pad_trailing` (F) | — | `padStart` / `padEnd`（Char） | `padding`（Foundation）|
| 繰り返し・反転 | `repeat` / `reverse` | `repeat` (F) / `chars().rev()` | `Repeat` (F) | `*` / `[::-1]` | `repeat` | `repeat` / `reverse` (F) | `duplicate` / `reverse` (F) | — | `repeat` / `reversed` | `String(repeating:)` / `reversed` |
| 文字の種類 | `is_digit` / `is_alpha` / `is_alphanumeric` / `is_whitespace` / `is_upper` / `is_lower` | `char::is_*` | `unicode.Is*` | `isdigit` / `isalpha` / `isspace` … (F) | — | — | — | — | `Char.isDigit` … | `Character.isNumber` … |
| 比較（Int を返す） | — | `cmp`（Ord）(F) | `Compare` (F) | — | `localeCompare`（ロケール依存）(F) | `compare`（Order）(F) | — | `compare` (F) | `compareTo` (F) | — |
| コードポイント | `codepoint` / `from_codepoint` | `char as u32` / `char::from_u32` | `rune` | `ord` / `chr` | `codePointAt` / `fromCodePoint` (F) | `utf_codepoint` / `from_utf_codepoints` (F) | `codepoints` / `to_charlist` (F) | `Uchar` | `codePointAt` (F) | `unicodeScalars` |
| UTF-8 バイト列との変換 | `to_bytes` / `from_bytes`（List[Int]）と `bytes.from_string` / `bytes.to_string`（Bytes） | `as_bytes` / `String::from_utf8` | `[]byte(s)` / `string(b)` | `encode` / `decode` (F) | `TextEncoder` / `TextDecoder` | `bit_array.from_string` / `to_string` | 文字列がそのまま binary | 文字列がそのままバイト列 | `encodeToByteArray` / `decodeToString` (F) | `utf8` / `String(decoding:as:)` |
| UTF-16 との変換 | — | `encode_utf16` / `String::from_utf16` (F) | `unicode/utf16` | `encode("utf-16")` (F) | 内部表現（`charCodeAt` / `fromCharCode`）(F) | — | `:unicode` | `get_utf_16le_uchar` ほか (F) | 内部表現 | `utf16` / `String(decoding:as:)` |
| 連長圧縮 | `run_length_encode` | — | — | — | — | — | — | — | — | — |
| その場で書き足す・空にする | `push` / `clear`（`mut`） | `push_str` / `clear` | `strings.Builder` | — | — | — | — | `Buffer` | `StringBuilder` | `append` / `removeAll` |

### 完全性の規則

1. **境界の族（R-L0-12）:** `slice`・`take`・`drop`・`take_end`・`drop_end` は `list` と同じ 5 本で、同じ境界入力に同じ答えを返します（負の `n`: `take` は全部、`drop` は空。実測 a.almd: `take("abc",-1)="abc"`、`drop("abc",-1)=""`）。list も同じ答えです（実測 probe-take: `list.take([1, 2, 3], -1)` は 3 要素、`list.drop` は空。`take_end`・`drop_end` も同じ）。負の `n` は添字ではなく個数なので、R-L0-12 の「負の添字は 0 に丸める」の対象ではありません。list と string で答えがそろっていて、変えると同じ名前のまま黙って結果が変わる（R-L0-14 の 4 が禁じる形）ので据え置きます。新しく足す `bytes.take` などもこの答えに合わせます。`bytes` も同じ行列に入ります。`slice` の負の開始位置は、R-L0-12（負の添字は範囲外で、0 に丸める）のとおり 0 に丸めます（`slice("abc", -1, 2) = "ab"`）。string はいまこの挙動で、list と bytes がこれに合わせます（L2-list R-list-10、L2-bytes R-bytes-12）。
2. **取り出しの族（R-L0-06）:** 失敗しうる取り出し（`get`、`first`、`last`、`index_of`、`last_index_of`、`strip_prefix`、`strip_suffix`、`codepoint`、`byte_slice`、`split_once`）は無印で `Option` を返します。いま穴はありません。`_or` 版もありません。
3. **その場書き換え（R-L0-04）:** `mut` を取るのは `push` と `clear` だけで、どちらも決まった動詞です。ほかの 48 関数はコピーを返します。
4. **符号化の置き場所（R-L0-07）:** UTF-8 のバイト列との変換は、両方向とも `bytes` に置きます（`bytes.from_string` / `bytes.to_string` / `bytes.to_string_lossy` が、`bytes.from_list` / `bytes.to_list` と同じ形の対になります）。`string` に残すのは、`Bytes` を経ない表現（コードポイント 1 個、UTF-16 の単位列）だけです。
5. **意図して持たないもの:** `split` の族は `{split, split_once, lines}` で閉じます（`stdlib/string.almd:24-25` の注記どおり、`rsplit_once` と `splitn` は持たない）。書式化（printf）は L1 で ×。文字型（Char）は持たず、1 文字は長さ 1 の String です。

### L2 の判定

- **重複:** `string.join` と `list.join` は同じ計算です（`stdlib/string_join.almd:50` が「identical」と書いている。プローブ a.almd でも同じ出力）。別々の intrinsic を持つので、いまの同義語ゲートでは見えません → R-string-01。
- **向きと型の食い違い:** `string.to_bytes` は `List[Int]`、`bytes.from_string` は `Bytes` を返します（#2078 の原因 1: 1 バイトが 8 バイトになる）。逆向きも `string.from_bytes`（List[Int]・壊れた列は置換）と `bytes.to_string`（Result）・`bytes.to_string_lossy` の 3 つがあります → R-string-02。
- **一つの用途だけの関数:** `run_length_encode`（#3852）→ R-string-03。
- **撤去日の決まっていない deprecated:** `string.length`（#3851 A）→ R-string-04。
- **失敗の目印を返す構築:** `from_codepoint` は不正な値（サロゲートや範囲外）で `""` を返します（実測: U+D800 で `""`）。R-L0-06 の 4（失敗しうる構築は目印の値を返さない）に反します → R-string-11。
- **抜けている役割（5 言語以上）:** 比較 `compare`（6 言語）→ R-string-05。バイト数（6 言語が専用の綴りを持つ。`byte_slice` があるのに、その範囲を決めるバイト数がコピーなしで取れない）→ R-string-06。UTF-16（R-L1-08）→ R-string-07。
- **足さない役割:** 空白で分割（4/9）→ R-string-08 ×。指定文字の trim、`splitn`、コードポイントのリスト化 → R-string-10 ×。
- **他言語の綴りを書いたときの案内:** `string.split_whitespace` を書くと、いまは E002 が `string.is_whitespace` を勧めます（実測 c.almd。意味がまったく違う）→ R-string-09。
- **R-L0-05 との関係:** `string.trim(s)` や `string.to_upper(s)` を文として書いても、いまはエラーになりません（実測 b.almd: `[  hi  ]` と表示）。string の 48 本のコピー関数はすべてこの事故の対象で、とくに `trim`・`replace`・`to_upper`・`to_lower` は、他言語（Python・JS の不変文字列を除く、Rust の `String` や Swift の `mutating` 関数）の癖でその場書き換えだと思われやすい関数です。R-L0-05 の E042 拡張で止まります。この page では新しい案を立てません。

---

## 案

### R-string-01 `string.join` を撤去し `list.join` に一本化する — ○ 採用

- 内容: `string.join(list, sep)` に `@deprecated(since=15, use="list.join")` を付け、epoch 16 で撤去します。`xs |> list.join(", ")` が唯一の綴りになります。
- 根拠の L0 規則: R-L0-01（同じ役割に綴りは 1 つ）、R-L0-02（主語 = 第 1 引数の型 `List[String]` のモジュールに置く）
- (a) 主語の型の側に置く言語が多数です（Rust の slice `join`、JS の `Array.join`、Kotlin の `joinToString`、Swift の `joined`、Elixir の `Enum.join`）。区切り文字の側（Python `sep.join`）や文字列モジュール（Go、Gleam、OCaml `concat`）もありますが、Almide の置き場所は R-L0-02 で決まります。
- (b) native と wasm で同じです（両方とも純粋な計算。`list.join` はすでに両 leg の契約に乗っています）。
- (c) `xs |> list.join(",")` は、パイプの先頭の型からモジュールが決まる形です。`string.join` が残ると「どちらでも書ける」選択肢が残ります。
- (d) 撤去。`removal_epoch = 16`。1 対 1 の改名なので `almide fix` が `string.join(xs, sep)` → `list.join(xs, sep)` に書き換えます。使用数（`\bstring\.join\b`、定義と生成文書を含む上限）: spec 104 / tests 26 / docs 13 / stdlib 5（`path.almd:45,120` ほか）/ research 28。`list.join` は spec 390 / tests 91 / docs 30 / stdlib 24。
- ゲート案: R-L0-01 の同義語ゲートの規則 6（同じ本体を持つ別 intrinsic）に、モジュールをまたぐ比較を足すと検出されます（`almide_rt_string_join` と `almide_rt_list_join` はどちらも `xs.join(sep)`）。
- issue: #3947

### R-string-02 UTF-8 の変換は両方向とも `bytes` に置き、`string.to_bytes` と `string.from_bytes` を撤去する — ○ 採用

- 内容:
  - `string.to_bytes(s) -> List[Int]` に `@deprecated(since=15, use="bytes.from_string")`。List[Int] が欲しいときのヒントは `bytes.from_string(s) |> bytes.to_list`。
  - `string.from_bytes(xs: List[Int]) -> String`（壊れた列は U+FFFD）に `@deprecated(since=15, use="bytes.to_string_lossy")`。ヒントは `bytes.from_list(xs) |> bytes.to_string_lossy`。
  - 残る正の綴り: `bytes.from_string`（String → Bytes）、`bytes.to_string`（Bytes → Result[String, String]）、`bytes.to_string_lossy`（Bytes → String）。
- 根拠の L0 規則: R-L0-07 の 2（入れ物の構築は先の型の `from_<元>`）と 4（向きの対は型も揃える）、R-L0-01。R-L0-14 の 4 は、戻り型を `Bytes` に変えて名前を残すこと（旧い書き方が型エラーになる変更）も許します。それでも名前ごと退けるのは、UTF-8 の両方向の置き場所が `bytes` に決まっていて（L2-bytes R-bytes-10）、`string.to_bytes` を `Bytes` 版で残すと `bytes.from_string` と同じ役割に綴りが 2 つになるからです（R-L0-01）。
- (a) Rust（`as_bytes` / `String::from_utf8`）・Kotlin（`encodeToByteArray` / `decodeToString`）は元の型の側、Gleam は `bit_array.from_string` / `bit_array.to_string` で「バイト列の側が両方向を持つ」形です。Almide の `bytes` はすでに `from_list` / `to_list` の対を持っているので、`from_string` / `to_string` もその隣に置くのが一番揃います。
- (b) native: `bytes.from_string` は `Vec<u8>`（#2078 の 1 バイト = 8 バイト問題が消える）。wasm: どちらも純粋な計算で、host 不要。
- (c) いまは「バイト列が欲しい」に 2 つの答え（List[Int] と Bytes）があり、性能が 8 倍違います。LLM は名前の短い `string.to_bytes` を選びます。
- (d) 撤去 2 本。`removal_epoch = 16`。使用数: `string.to_bytes` spec 27 / tests 3 / docs 11 / stdlib 3（すべてコメント）/ research 6、`string.from_bytes` spec 41 / tests 2 / docs 11 / stdlib 3（`http_url_decode.almd:41` の呼び出し 1 か所とコメント）/ research 2。形が変わる書き換え（`|> bytes.to_list` を足す）なので、`almide fix` は「List[Int] を受け取る位置か」で 2 通りに書き分けます。
- ゲート案: R-L0-07 のゲート（`to_<T>` と `from_<T>` を突き合わせ、同じ変換が 2 か所にないこと）に「String ⇄ UTF-8 バイト列」の組を登録します。`string` の公開関数に、戻り型・引数型が `List[Int]` で UTF-8 を意味するものがないことも検査します。
- issue: #2078（やること 4 の答え）。撤去そのものは新規

### R-string-03 `string.run_length_encode` を撤去する — ○ 採用

- 内容: `@deprecated(since=15, use="string.chars + list.fold")`、`removal_epoch = 16` で撤去。置き換え先は関数 1 つではないので `almide fix` は書き換えず、ヒントとして fold の 2 行（`string.chars(s) |> list.fold([], ...)`）を `removed_spellings.rs` に置きます。
- 根拠の L0 規則: R-L0-00 の前提（9 言語のどれも標準に持たない）、L1 の「足す」基準の裏返し（0/9）
- (a) 0/9 です。練習問題の答えであって、文字列の基本操作ではありません。復号の対もありません。
- (b) 撤去すると、contract（`string.split("")` と RLE のコードポイント単位）から RLE の行が外れます。fixture `spec/wasm_cross/string_split_rle_codepoint.almd` は split の行だけ残します。
- (c) 公開面の関数が 1 つ減れば、LLM の候補から「意味の狭い名前」が 1 つ減ります。
- (d) 撤去。使用数: spec 8 / tests 2 / docs 11 / stdlib 1（定義）/ research 0。
- ゲート案: なし（1 回限りの撤去）。
- issue: #3852（2 節の表の 1 行目）

### R-string-04 `string.length` の撤去を epoch 16 に載せる — ○ 採用

- 内容: すでに `@deprecated(since=3, use="string.len")`（E052 の警告と `almide fix` は動いている。実測 d.almd）。`proofs/dialect-epochs.toml` に `[[deprecation]]` を置き `removal_epoch = 16` とします。CHEATSHEET:1077 の `→ WRONG` 行は残します。
- 根拠の L0 規則: R-L0-11、R-L0-14
- (d) 撤去。since はいまの 3 のまま（すでに窓が開いているため、since=15 の窓は置き直しません）、`removal_epoch = 16`。`almide fix` は `string.length(s)` → `string.len(s)` に 1 対 1 で書き換えます。撤去後のヒント: 「`string.len(s)`」。使用数: spec 6 / tests 7 / docs 8 / stdlib 1（定義）。
- issue: #3851 A

### R-string-05 `string.compare(a, b) -> Int` を足す — ○ 採用

- 内容: 負・0・正の Int を返します。順序は `<` と同じ（UTF-8 のバイト順 = コードポイント順。実測 e.almd: `"B" < "a"` は true）。`list.sort_with(xs, string.compare)` のように、比較関数を取る関数にそのまま渡せる形です。
- 根拠の L0 規則: R-L0-16（順序のある型には `compare(a, b) -> Int` を置く）、R-L0-10（`_with` は比較関数を取る。`list.sort_with` を L2-list で足すので、比較関数の部品が要る）。
- (a) 6 言語が持ちます（Rust `cmp`、Go `Compare` (F)、Gleam `compare` (F)、OCaml `compare` (F)、Kotlin `compareTo` (F)、JS `localeCompare` (F、ロケール依存で意味が違う)）。綴りは `compare` が 3 言語で一番多く、Almide の `bytes.cmp` は少数派です。戻り値は Int が多数派（Go・OCaml・Kotlin・JS）で、Almide の `bytes.cmp` も Int です。
- (b) 純粋な計算。ロケールを見ないので、native と wasm で同じです。
- (c) 降順や複合キーの並べ替えで、LLM は比較関数を書きます。`(a, b) => if a < b then -1 else if a > b then 1 else 0` を毎回書かせると、符号の取り違えが起きます。
- (d) 追加。
- ゲート案: `string.compare(a, b) < 0` ⇔ `a < b` を、ランダムな文字列の組で native・wasm・interp の 3 方向で確かめます。
- issue: #3902

### R-string-06 `string.byte_len(s) -> Int` を足す — ○ 採用

- 内容: UTF-8 のバイト数を、コピーせずに返します。
- 根拠の L0 規則: R-L0-11（長さの綴りは `len`）、R-L0-02（主語は String）。`byte_` は既存の `byte_slice` と同じ接頭辞です。
- (a) 専用の綴りを持つのは 6 言語（Rust `len` (F)、Go `len`、Gleam `byte_size` (F)、Elixir `byte_size` (F)、OCaml `length` (F)、Swift `utf8.count`）。
- (b) native は `s.len()`、wasm は文字列の長さ欄をそのまま読むだけです。
- (c) `string.byte_slice(s, a, b)` の範囲を決めるのにバイト数が要りますが、いまは `bytes.from_string(s) |> bytes.len` でまるごとコピーするしかありません。
- (d) 追加。
- ゲート案: `string.byte_len(s) == bytes.len(bytes.from_string(s))` を性質テストにします。
- issue: #3945

### R-string-07 UTF-16 との変換を足す（Latin-1 は足さない） — ○ 採用

- 内容:
  - `string.to_utf16(s: String) -> List[Int]`（UTF-16 の符号単位 0..65535。BOM なし）
  - `string.from_utf16(units: List[Int]) -> Result[String, String]`（対になっていないサロゲートや範囲外の値は `err("invalid UTF-16 at index N")`）
  - Latin-1 は足しません。
- 根拠の L0 規則: R-L1-08（string の関数として足す）、R-L0-07 の 4（対の型を揃える: 両方 `List[Int]`）、R-L0-06 の 1（理由のある失敗は Result）
- (a) UTF-16 は 8 言語が持ちます（L1）。Latin-1 は Python・JS・Kotlin・Swift・Elixir の 5 言語ですが、どれも「文字コード名を渡す汎用の符号化器」の一つとして持っていて、専用の関数はありません。Almide に汎用の文字コード表を入れる理由がないので、×とします。
- (b) 純粋な計算。native と wasm で同じです。UTF-16 の単位列は `Bytes` ではない（16 ビット単位）ので、`bytes` ではなく `string` に置きます。
- (c) JS・Windows API・JSON の `\uXXXX` を扱う場面で、LLM は `encode_utf16` / `charCodeAt` の意味を前提に書きます。
- (d) 追加。既存の `from_codepoint` も、R-string-11 で失敗を `Option` で表す形にそろえます。
- ゲート案: `string.from_utf16(string.to_utf16(s)) == ok(s)` の往復、サロゲート対（U+1F980 など）、孤立サロゲートの err を fixture にし、native と wasm で一致させます。
- issue: #3948

### R-string-08 `string.split_whitespace` を足す — × 不採用

- 理由: 専用の関数を持つのは 4/9（Rust (F)、Go `Fields` (F)、Python `split()` (F)、Elixir `split/1` (F)）で、L1 の「5 言語以上」に届きません。Gleam・JS・OCaml は持たないことを確かめました (F)。`regex.split("\\s+", string.trim(s))` で書けます（引数の順は regex の判定に従います）。
- 代わりに R-string-09 のヒントで、この綴りを書いた LLM を正しい書き方へ案内します。
- 根拠の L0 規則: R-L0-00、L1 の判定基準

### R-string-09 他言語の綴りへのヒントを、意味の合う書き方に直す — ○ 採用

- 内容: E002 の「Did you mean」表（`crates/almide-frontend/src/stdlib.rs:156` の `ALIASES`）に、string の行を足します。
  - `split_whitespace` / `fields` / `words` → `regex.split("\\s+", string.trim(s))`
  - `to_utf8` / `as_bytes` / `encode` → `bytes.from_string`、`decode` / `from_utf8` → `bytes.to_string`
  - `cmp` / `compare_to` / `locale_compare` → `string.compare`
  - `byte_size` / `byte_length` → `string.byte_len`
  - `pad_left` / `pad_right` / `rjust` / `ljust` → `string.pad_start` / `string.pad_end`
  - `strip` / `lstrip` / `rstrip` → `string.trim` / `trim_start` / `trim_end`
  - `remove_prefix` / `trim_prefix` → `string.strip_prefix`
- 根拠の L0 規則: R-L0-01（綴りは 1 つ。別名は作らず、エラーで案内する）
- (a) 表の綴りは上の L2 表の各言語の綴りです。
- (b) 検査器だけの変更です。
- (c) いま `string.split_whitespace` を書くと `string.is_whitespace` を勧められます（実測 c.almd）。名前の近さで選んだ候補は、意味が逆のことがあります。
- (d) 追加（診断のみ。言語の表面は変わらない）。
- ゲート案: 表のすべての行について「キーは未定義、行き先は生きている公開関数か正しい式」であることを検査するテストを足します（表には、いま生きている関数を指すキーも残っています。このテストは L2-map の R-map-06 と同じものです）。
- issue: #3905

### R-string-10 指定文字の trim、`splitn`、コードポイントのリスト化を足す — × 不採用

- 指定文字の trim（Python `strip(chars)`、Rust `trim_matches`、Go `Trim(s, cutset)`、Kotlin `trim(chars)`、Elixir `trim(s, to_trim)`、Swift `trimmingCharacters`）: 6 言語が持ちますが、意味が「文字の集合」（Python・Go）、「繰り返す部分文字列」（Elixir）、「パターン」（Rust）に割れています。どれに揃えても、残りの言語の癖で書いた LLM は外れます。`strip_prefix` / `strip_suffix` と `regex.replace` で書けます。
- `splitn`: `stdlib/string.almd:24-25` の判断（2 つに分ける用途は `split_once` が担う）を据え置きます。
- コードポイントのリスト化（Gleam `to_utf_codepoints`、Elixir `to_charlist` など）: `string.chars(s) |> list.filter_map(string.codepoint)` の 1 行で書けるので、L1 基準の 3 番目（いまの関数でまかなえない）を満たしません。
- 根拠の L0 規則: R-L0-00、L1 の判定基準

---

### R-string-11 `string.from_codepoint` を `Option[String]` を返す形にする — ○ 採用

- 内容: `string.from_codepoint(n: Int) -> String` を `string.from_codepoint(n: Int) -> Option[String]` に変えます。不正な値（サロゲート U+D800〜U+DFFF、負の値、U+10FFFF より大きい値）は none です。いまは `""` を返します。名前は変えません。
- 根拠の L0 規則: R-L0-06 の 4（失敗しうる構築は目印の値を返さない。理由がなければ `Option`）、R-L0-14 の 4（旧い書き方が必ず型エラーになる変更は、ヒントと `almide fix` を付ければ同じ名前のまま許す）
- (a) Rust の `char::from_u32` は `Option`、Gleam の `utf_codepoint` は `Result`、Swift の `Unicode.Scalar(_:)` は失敗しうる初期化子（nil）です。Python の `chr` と JS の `fromCodePoint` は例外を投げます。空文字を返す言語はありません。
- (b) 純粋な計算です。native と wasm で同じ境界判定を使います。
- (c) いまは不正な値が `""` になって、そのまま文字列に連結されます。間違えても気づけません。`Option` なら `??` か `?` を書かないと値が取り出せません。
- (d) 意味変更（戻り型 `String` → `Option[String]`）。旧い書き方は、結果を `String` として使う箇所で必ず型エラーになるので、黙って違う結果にはなりません（R-L0-14 の 4）。`@deprecated` は付けず、epoch 15 の `breaks` に入れます。`almide fix` は `string.from_codepoint(n)` → `string.from_codepoint(n) ?? ""`（いまと同じ結果になる形）に書き換え、ヒントに「`?` で失敗を伝えるほうがよい」と出します。使用数は spec・tests・docs・stdlib で数えて PR に書きます。
- ゲート案: R-L0-G の番兵検査（戻り型が `String`・`Int` で、名前が `from_`・`parse`・`decode` で始まる公開関数を一覧にし、境界入力で目印を返さないことを確かめる。L2-bytes R-bytes-09 と同じテスト）。`from_codepoint(0xD800) == none` と `from_codepoint(0x1F980) == some("🦀")` の fixture を native と wasm で置きます。
- issue: #3946

### R-string-08 `string.slice` の終わりの既定値を外し、2 引数の形は `drop` に一本化する — ○ 採用

- いま: `fn string.slice(s: String, start: Int, end: Int = 末尾) -> String` は、標準の関数で唯一の既定値つきの引数です。`string.slice(s, n)` は `string.drop(s, n)` と同じ値になります。`list.slice` と `bytes.slice` は 3 引数だけで、2 引数で呼ぶと E004 です。
- 案: `end` の既定値を外し、`fn string.slice(s: String, start: Int, end: Int) -> String` にします。2 引数の呼び出しは E004 になり、ヒントで `string.drop(s, n)` を示します。list と bytes に既定値は足しません。
- 根拠の L0 規則: R-L0-01（2 引数の `slice` は `drop` の別名）、R-L0-12（境界の行列は list・string・bytes で同じ形）。
- (a) 終わりを省ける slice は JS・Python・Kotlin（`substring(a)`）にありますが、Almide にはすでに `drop` があります。3 つの型すべてに既定値を足すと、`slice(n)` と `drop(n)` の同義語が 3 組できます。
- (b) 検査器と宣言だけの変更です。
- (c) 実測: `string.slice("abcdef", 2)` は `cdef` を返し、`list.slice([1,2,3], 1)` は E004 です。string で覚えた 2 引数の形が、list と bytes では外れます。
- (d) 意味変更（受け付ける形を狭める）です。古い 2 引数の呼び出しは E004 になり、黙って結果は変わらないので、R-L0-14.4 を満たします。epoch 15 の `breaks` に入れ、`almide fix` が `string.slice(s, n)` を `string.drop(s, n)` に書き換えます。使用数（2 引数）: spec 6 / docs 1 / stdlib 2 / research 4 / examples 1。
- 完全性の規則: `slice`・`take`・`drop`・`take_end`・`drop_end` を持つ型（list・string・bytes）では、同じ名前の関数の引数の数が同じです。標準の公開関数に既定値つきの引数は置きません。
- ゲート案: R-L0-12 の境界の行列テストに「3 つの型の同名関数で引数の数が同じ」を足し、interface JSON で既定値つきの引数が 0 本であることを確かめます。
- issue: #3969

### R-string-12 `string.codepoint` を `string.to_codepoint` に改名する — ○ 採用

- いま: 逆向きは `string.from_codepoint(n) -> String?` です。往復の対が `codepoint` と `from_codepoint` で、`to_codepoint` と書くと E002 になります。
- 案: `fn string.to_codepoint(s: String) -> Int?`（最初の文字のコードポイント。空文字列なら none）。`from_codepoint` と `to_`/`from_` の対になります（`float.to_bits`/`from_bits` と同じ）。
- 根拠の L0 規則: R-L0-25（変換の動詞の表: 往復する対は `to_<X>`/`from_<X>`）。
- (a) JS `codePointAt`/`fromCodePoint`、Python `ord`/`chr`、Rust `char as u32`/`char::from_u32` と綴りは割れていて多数派はありません。Almide の中の対で決めます。
- (d) 改名 1。`@deprecated(since=15, use="string.to_codepoint")`、`removal_epoch = 16`、`almide fix` で書き換えます。
- issue: #3979

### R-string-13 `string.count` を述語で数える形にする — × 不採用

- 理由: 文字列の「数える」は、部分文字列の出現数が多数派です（Rust `matches().count()`・Go `strings.Count`・Python `str.count`・Elixir `String.count`…）。string は D3 の族（list・map・set・deque・heap・bytes の高階関数）の対象ではありません。述語で数えたいときは `string.chars(s) |> list.count(f)` と書きます。
- issue: なし

## L3: 関数表

| 関数 | シグネチャ(現行, mut 表示) | 他言語の主な綴り | 判定 | 案 |
|---|---|---|---|---|
| `trim` | `(s: String) -> String` | Rust/JS/Gleam/Kotlin `trim` / Python `strip` / Go `TrimSpace` | 据え置き | — |
| `split` | `(s: String, sep: String) -> List[String]` | 9/9 `split` 系（Go `Split`）。`split("", ",")` は `[""]`（Rust と同じ） | 据え置き | — |
| `split_once` | `(s: String, sep: String) -> Option[(String, String)]` | Rust/Gleam `split_once` / Go `Cut` / Python `partition` | 据え置き | — |
| `join` | `(list: List[String], sep: String) -> String` | Go/Gleam `Join` / Python `sep.join` / Rust・JS は列の側 | 撤去（→ `list.join`） | R-string-01 |
| `len` | `(s: String) -> Int` | Python `len` / Gleam・Elixir `length`（書記素）/ Swift `count` | 据え置き | — |
| `length` | `(s: String) -> Int`（deprecated） | — | 撤去（→ `string.len`） | R-string-04 |
| `contains` | `(s: String, sub: String) -> Bool` | Rust/Gleam/Kotlin/Swift `contains` / JS `includes` | 据え置き | — |
| `starts_with` | `(s: String, prefix: String) -> Bool` | Rust/Gleam/OCaml `starts_with` / Go `HasPrefix` | 据え置き | — |
| `ends_with` | `(s: String, suffix: String) -> Bool` | Rust/Gleam/OCaml `ends_with` / Go `HasSuffix` | 据え置き | — |
| `slice` | `(s: String, start: Int, end: Int) -> String` | Gleam/Elixir/JS `slice` / Python `s[a:b]` | 意味変更（`end` の既定値を外す。2 引数は `drop`） | R-string-08 |
| `byte_slice` | `(s: String, start: Int, end: Int) -> Option[String]` | Elixir `byte_slice` / Rust `get(a..b)` | 据え置き | — |
| `pad_start` | `(s: String, n: Int, ch: String) -> String` | JS/Gleam/Kotlin `padStart` / Python `rjust`。`ch` は先頭の 1 文字だけ使う（実測: `pad_start("7",5,"ab")="aaaa7"`） | 据え置き | — |
| `to_bytes` | `(s: String) -> List[Int]` | Rust `as_bytes` / Kotlin `encodeToByteArray` / Gleam `bit_array.from_string` | 撤去（→ `bytes.from_string`） | R-string-02 |
| `capitalize` | `(s: String) -> String` | Python/Elixir/Gleam は残りを小文字に、OCaml `capitalize_ascii` は残りそのまま (F)。多数派なしで据え置き | 据え置き | — |
| `to_upper` | `(s: String) -> String` | Rust `to_uppercase` / Go `ToUpper` / Python `upper` / Gleam `uppercase` | 据え置き | — |
| `to_lower` | `(s: String) -> String` | Rust `to_lowercase` / Go `ToLower` / Python `lower` / Gleam `lowercase` | 据え置き | — |
| `replace` | `(s: String, from: String, to: String) -> String` | Rust/Python/Gleam `replace` / JS・Go `replaceAll`/`ReplaceAll` | 据え置き | — |
| `get` | `(s: String, i: Int) -> Option[String]` | JS `at` / Elixir `at` / Rust `chars().nth` | 据え置き | — |
| `lines` | `(s: String) -> List[String]` | Rust/Kotlin `lines` / Python `splitlines` / Go `Lines` | 据え置き | — |
| `chars` | `(s: String) -> List[String]` | Rust `chars` / Gleam `to_graphemes` / Elixir `graphemes` | 据え置き | — |
| `index_of` | `(s: String, needle: String) -> Option[Int]` | Rust `find` / Go `Index` / JS `indexOf` / OCaml `index_opt` | 据え置き | — |
| `repeat` | `(s: String, n: Int) -> String` | Rust/JS/Gleam `repeat` / Go `Repeat` / Elixir `duplicate` | 据え置き | — |
| `from_bytes` | `(bytes: List[Int]) -> String` | Rust `String::from_utf8_lossy` / Kotlin `decodeToString` | 撤去（→ `bytes.from_list(xs) \|> bytes.to_string_lossy`） | R-string-02 |
| `is_digit` | `(s: String) -> Bool` | Python `isdigit` / Rust `is_ascii_digit`。空は false | 据え置き | — |
| `is_alpha` | `(s: String) -> Bool` | Python `isalpha` / Rust `is_alphabetic`。空は false | 据え置き | — |
| `is_alphanumeric` | `(s: String) -> Bool` | Python `isalnum` / Rust `is_alphanumeric`。空は false | 据え置き | — |
| `is_whitespace` | `(s: String) -> Bool` | Python `isspace`（空は false）/ Rust `is_whitespace`。Almide は空で true（文書どおり。空行の判定に使える形） | 据え置き | — |
| `is_upper` | `(s: String) -> Bool` | Python `isupper`（文字以外を無視、の意味も同じ） | 据え置き | — |
| `is_lower` | `(s: String) -> Bool` | Python `islower` | 据え置き | — |
| `codepoint` | `(s: String) -> Option[Int]` | Python `ord` / JS `codePointAt` / Gleam `utf_codepoint_to_int` | 改名 → `to_codepoint` | R-string-12 |
| `from_codepoint` | `(n: Int) -> String` | Python `chr` / JS `fromCodePoint` / Rust `char::from_u32`（Option）。不正な値は `""`（実測: U+D800 で `""`） | 意味変更（`Option[String]` を返す） | R-string-11 |
| `pad_end` | `(s: String, n: Int, ch: String) -> String` | JS/Gleam/Kotlin `padEnd` / Python `ljust`。`ch` が空なら空白 | 据え置き | — |
| `trim_start` | `(s: String) -> String` | Rust/JS/Gleam `trim_start` / Python `lstrip` | 据え置き | — |
| `trim_end` | `(s: String) -> String` | Rust/JS/Gleam `trim_end` / Python `rstrip` | 据え置き | — |
| `count` | `(s: String, sub: String) -> Int` | Python `count` / Go `Count`（空の sub で len+1、Almide も同じ (F)）。`list.count`（述語）と動詞は同じだが、引数の型が違うので取り違えは型エラーになる | 据え置き | — |
| `is_empty` | `(s: String) -> Bool` | Rust/Gleam/OCaml `is_empty` / Kotlin `isEmpty` | 据え置き | — |
| `reverse` | `(s: String) -> String` | Gleam/Elixir `reverse`（書記素単位）/ Rust `chars().rev()`。Almide はコードポイント単位 | 据え置き | — |
| `strip_prefix` | `(s: String, prefix: String) -> Option[String]` | Rust `strip_prefix`（Option）/ Python `removeprefix` / Gleam `remove_prefix` | 据え置き | — |
| `strip_suffix` | `(s: String, suffix: String) -> Option[String]` | Rust `strip_suffix` / Python `removesuffix` | 据え置き | — |
| `replace_first` | `(s: String, from: String, to: String) -> String` | Kotlin `replaceFirst` / Rust `replacen(..,1)` / JS `replace` | 据え置き | — |
| `last_index_of` | `(s: String, needle: String) -> Option[Int]` | Rust `rfind` / Go `LastIndex` / JS `lastIndexOf` | 据え置き | — |
| `first` | `(s: String) -> Option[String]` | Gleam/Elixir/Kotlin `first` | 据え置き | — |
| `last` | `(s: String) -> Option[String]` | Gleam/Elixir/Kotlin `last` | 据え置き | — |
| `take` | `(s: String, n: Int) -> String` | Kotlin `take` / Swift `prefix` | 据え置き | — |
| `take_end` | `(s: String, n: Int) -> String` | Kotlin `takeLast` / Swift `suffix` | 据え置き | — |
| `drop` | `(s: String, n: Int) -> String` | Kotlin `drop` / Gleam `drop_start` | 据え置き | — |
| `drop_end` | `(s: String, n: Int) -> String` | Kotlin `dropLast` / Gleam `drop_end` | 据え置き | — |
| `run_length_encode` | `(s: String) -> List[(String, Int)]` | 0/9 | 撤去（→ `string.chars` と `list.fold`） | R-string-03 |
| `push` | `(mut s: String, suffix: String) -> Unit` | Rust `push_str` / Swift `append` | 据え置き | — |
| `clear` | `(mut s: String) -> Unit` | Rust `clear` / Swift `removeAll` | 据え置き | — |
| `compare`（追加） | `(a: String, b: String) -> Int` | Go `Compare` / Gleam・OCaml `compare` / Rust `cmp` / Kotlin `compareTo` | 追加 | R-string-05（R-L0-16） |
| `byte_len`（追加） | `(s: String) -> Int` | Rust `len` / Go `len` / Gleam・Elixir `byte_size` / Swift `utf8.count` | 追加 | R-string-06 |
| `to_utf16`（追加） | `(s: String) -> List[Int]` | Rust `encode_utf16` / JS `charCodeAt` / Swift `utf16` | 追加 | R-string-07 |
| `from_utf16`（追加） | `(units: List[Int]) -> Result[String, String]` | Rust `String::from_utf16`（Result）/ JS `fromCharCode` | 追加 | R-string-07 |

現行 50 関数 + 追加 4 = 54 行。

---

## L0 への差し戻し（処理済み）

1. **比較関数の綴りと型。** L0 に R-L0-16（順序のある型には `<mod>.compare(a, b) -> Int` を置く）が入りました。`string.compare` は R-string-05 で足します。`bytes.cmp` は `bytes.compare` に改名します（L2-bytes R-bytes-08）。
2. **構築関数の失敗の表し方。** L0 に R-L0-06 の 4（失敗しうる構築・変換は目印の値を返さない）が入り、R-L0-14 の 4 が「旧い書き方が型エラーになる同じ名前の変更」を許しました。`string.from_codepoint` は名前を変えずに `Option[String]` を返す形にします（R-string-11）。
