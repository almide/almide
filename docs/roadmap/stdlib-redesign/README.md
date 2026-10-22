<!-- description: Stdlib redesign — index of the three-layer redesign of the standard library: the rules, the per-module pages and every proposal with its verdict and issue -->
# 標準ライブラリのリデザイン（3 層）

> Last updated: 2026-10-22

Almide の標準関数 1,001 個（44 モジュール。一巡目の記録は 999 個と数えていましたが、二巡目で記録から漏れていた `bytes.heap_save`・`heap_restore` を見つけました）を、9 言語（Rust・Go・Python・TypeScript/JavaScript・Gleam・Elixir・OCaml・Kotlin・Swift）の標準ライブラリと並べて見直した記録です。実装はここでは行いません。採用した案（○）はすべて issue にしてあります。

二巡目（2026-10-22）: リデザイン後の面（[target-surface.md](target-surface.md)）を、L0 の規則での機械的な点検と、4 つの分野（コレクション・データ形式・システム・数）の見直しにかけ、モジュールをまたぐ不統一と族の穴を埋めました。R-L0-22〜26 と、各 L2 の二巡目の案がそれです。

三巡目（2026-10-22）: リデザイン後の一覧を外部のレビューにかけ、指摘を記録と突き合わせました。記録の誤り（`net` の撤去先の名前）を直し、R-L0-27・28、R-L1-30（`bool`）と、各 L2 の三巡目の案を足しました。すでに理由つきで決めていた指摘（`map.map` のコールバック、時間の引数を `Int` のミリ秒にすること、固定幅の型に算術関数を置かないことなど）は変えていません。

四巡目（2026-10-22）: 2 つめの外部レビューを同じように突き合わせました。2 つのレビューがそろって指摘した綴り（`value.str`、`hash` の `_hex` の入力）は見直して改名・変更し、R-L0-29（負の位置を abort にする案）は Dojo の計測まで保留にしました。一覧の生成の誤り（撤去欄の空の移行先、切れた注記、演算子の脱糖先の行）も直しました。

五巡目（2026-10-22）: 命名の統一を一覧の全関数で機械的に点検しました。関数名は L0 のとおり揃っていましたが、引数名は同じ役割に 2 つ以上の綴りがありました（`Bytes` の `b`/`data`、要素の `x`/`val`/`value` など）。R-L0-30 で役割ごとに 1 つに決め、一覧のシグネチャをその名前で書き直しました。

## 読む順番

1. [L0: 全モジュールに共通する規則](L0-conventions.md) — 名前・置き場所・その場書き換え・失敗の表し方・変換の向き・互換の扱い。下の層の判定はすべてここから導きます。
2. [L1: カテゴリ（モジュール）の一覧](L1-categories.md) — 足すカテゴリ、足さないカテゴリ、分け方と持ち分。
3. L2/L3: モジュールごとのページ — 役割の比較表（L2）と、全関数の判定表（L3）。
4. [リデザイン後の標準ライブラリ](target-surface.md) — ○ の案をすべて適用したあとの公開関数の一覧（モジュールごと、シグネチャつき、撤去した関数の移行先つき）。
5. 根拠の事実: [facts/naming-conventions.md](facts/naming-conventions.md)（命名の慣習 20 項目 × 9 言語）・[facts/categories.md](facts/categories.md)（約 100 カテゴリ × 9 言語）。

判定の 4 軸: (a) 他言語と比べた過不足、(b) native（Rust）と wasm（WASI）の両方で成り立つか、(c) LLM が書き当てやすいか、(d) 変更の種類と互換。

互換の扱い（R-L0-14）: 改名と撤去は epoch 15 で `@deprecated(since=15, use=…)` を付け、epoch 16 で撤去します。`almide fix` が機械的に書き換えられるものは書き換え、形が変わるものはヒントを出します。

## モジュールのページ

[args](L2-args.md) ・ [base64](L2-base64.md) ・ [bigint](L2-bigint.md) ・ [bool](L2-bool.md) ・ [bytes](L2-bytes.md) ・ [compute](L2-compute.md) ・ [datetime](L2-datetime.md) ・ [deque](L2-deque.md) ・ [duration](L2-duration.md) ・ [env](L2-env.md) ・ [error](L2-error.md) ・ [ffi](L2-ffi.md) ・ [float](L2-float.md) ・ [fs](L2-fs.md) ・ [hash](L2-hash.md) ・ [heap](L2-heap.md) ・ [hex](L2-hex.md) ・ [html](L2-html.md) ・ [http](L2-http.md) ・ [int](L2-int.md) ・ [io](L2-io.md) ・ [json](L2-json.md) ・ [list](L2-list.md) ・ [map](L2-map.md) ・ [math](L2-math.md) ・ [matrix](L2-matrix.md) ・ [net](L2-net.md) ・ [option](L2-option.md) ・ [path](L2-path.md) ・ [process](L2-process.md) ・ [random](L2-random.md) ・ [regex](L2-regex.md) ・ [result](L2-result.md) ・ [set](L2-set.md) ・ [sized-numbers](L2-sized-numbers.md) ・ [string](L2-string.md) ・ [testing](L2-testing.md) ・ [unix](L2-unix.md) ・ [url](L2-url.md) ・ [uuid](L2-uuid.md) ・ [value](L2-value.md) ・ [zlib](L2-zlib.md)

## 案の状態（全 295 件: ○ 237・× 53・保留 5）

| 案 | 内容 | 判定 | issue |
|---|---|---|---|
| [R-L0-00](L0-conventions.md#改名してよい条件全規則の前提) | 改名してよい条件（全規則の前提） | ○ | —（変更なし） |
| [R-L0-01](L0-conventions.md#r-l0-01-同じ役割には綴りを-1-つモジュールをまたいでも---採用) | 同じ役割には綴りを 1 つ（モジュールをまたいでも） | ○ | #3851 |
| [R-L0-02](L0-conventions.md#r-l0-02-関数は主語の型のモジュールに置く---採用) | 関数は主語の型のモジュールに置く | ○ | #3949 |
| [R-L0-03](L0-conventions.md#r-l0-03-引数の順序は主語を先頭にする---据え置きゲート) | 引数の順序は主語を先頭にする | ○ | #3949 |
| [R-L0-04](L0-conventions.md#r-l0-04-コピーが既定その場書き換えは決まった動詞だけ---採用) | コピーが既定。その場書き換えは、決まった動詞だけ | ○ | #3928 |
| [R-L0-05](L0-conventions.md#r-l0-05-unit-以外の値を文として捨てたらエラーにする---採用) | Unit 以外の値を文として捨てたらエラーにする | ○ | #3906 |
| [R-L0-06](L0-conventions.md#r-l0-06-無印の名前は安全版既定値版-_or-は--に一本化する---採用) | 無印の名前は安全版。既定値版 `_or` は `??` に一本化する | ○ | #3913 |
| [R-L0-07](L0-conventions.md#r-l0-07-変換の向き-値の変換は元の型の-to_型入れ物からの構築は先の型の-from_元---採用) | 変換の向き: 値の変換は元の型の `to_<型>`、入れ物からの構築は先の型の `from_<元>` | ○ | #3935 |
| [R-L0-08](L0-conventions.md#r-l0-08-数値変換の-4-種は接尾辞の行列---据え置き穴埋め) | 数値変換の 4 種は接尾辞の行列 | ○ | #3854 |
| [R-L0-09](L0-conventions.md#r-l0-09-bool-を返す関数は-is_has_または決まった動詞---採用) | Bool を返す関数は `is_`/`has_`、または決まった動詞 | ○ | #3895 |
| [R-L0-10](L0-conventions.md#r-l0-10-高階関数の接尾辞の意味を-1-つに決める---採用) | 高階関数の接尾辞の意味を 1 つに決める | ○ | #3949 |
| [R-L0-11](L0-conventions.md#r-l0-11-長さは-len-だけ---採用既存の撤去を日付に載せる) | 長さは `len` だけ | ○ | #3851 |
| [R-L0-12](L0-conventions.md#r-l0-12-範囲は半開区間はみ出しは丸めabort-は契約違反だけ---据え置きゲート) | 範囲は半開区間。はみ出しは丸め、abort は契約違反だけ | ○ | #3944 |
| [R-L0-13](L0-conventions.md#r-l0-13-環境を読む関数は-effect---据え置き穴を塞ぐ) | 環境を読む関数は effect | ○ | #3851 |
| [R-L0-14](L0-conventions.md#r-l0-14-互換の扱い全案で共通---採用) | 互換の扱い（全案で共通） | ○ | #3904 |
| [R-L0-15](L0-conventions.md#r-l0-15-標準関数は-native-と-wasm-の両方で動く---採用) | 標準関数は native と wasm の両方で動く | ○ | —（変更なし） |
| [R-L0-16](L0-conventions.md#r-l0-16-順序のある型には-compare-を置く---採用) | 順序のある型には `compare` を置く | ○ | #3902 |
| [R-L0-17](L0-conventions.md#r-l0-17-呼び出しの型は宣言だけで決まる---採用) | 呼び出しの型は宣言だけで決まる | ○ | #3900 |
| [R-L0-18](L0-conventions.md#r-l0-18-型を変換する演算子-as-は足さない---不採用) | 型を変換する演算子 `as` は足さない | × | — |
| [R-L0-19](L0-conventions.md#r-l0-19-他言語の変換の書き方から変換の関数へ案内する---採用) | 他言語の変換の書き方から、変換の関数へ案内する | ○ | #3958 |
| [R-L0-20](L0-conventions.md#r-l0-20-critical-プロファイルで黙って折り返す-to_型-を拒む--保留) | critical プロファイルで、黙って折り返す `to_<型>` を拒む | 保留 | — （CG-3 と同時に判断） |
| [R-L0-21](L0-conventions.md#r-l0-21-無印の-to_型-をはみ出したら失敗する版に変える--保留) | 無印の `to_<型>` を、はみ出したら失敗する版に変える | 保留 | [almide-dojo#63](https://github.com/almide/almide-dojo/issues/63)（測定） |
| [R-L0-22](L0-conventions.md#r-l0-22-組み込みの-ord-プロトコルのメソッド名を-compare-にする---採用) | 組み込みの `Ord` プロトコルのメソッド名を `compare` にする | ○ | #3961 |
| [R-L0-23](L0-conventions.md#r-l0-23-状態を進める関数は-next_-で始め値-次の状態-を返す---採用) | 状態を進める関数は `next_` で始め、`(値, 次の状態)` を返す | ○ | #3962 |
| [R-L0-24](L0-conventions.md#r-l0-24-標準の面の型は正規の綴りで書くt---採用) | 標準の面の型は正規の綴りで書く（`T?`） | ○ | #3963 |
| [R-L0-25](L0-conventions.md#r-l0-25-文字列との変換の動詞を表で決める---採用) | 文字列との変換の動詞を表で決める | ○ | #3971 |
| [R-L0-26](L0-conventions.md#r-l0-26-演算子を持つ型は演算子と脱糖先の表を-1-か所で宣言する---採用) | 演算子を持つ型は、演算子と脱糖先の表を 1 か所で宣言する | ○ | #3973 |
| [R-L0-27](L0-conventions.md#r-l0-27-next_-が-none-を返すときは状態を進めない---採用) | `next_` が none を返すときは、状態を進めない | ○ | #3979 |
| [R-L0-28](L0-conventions.md#r-l0-28-_checked-の変換は正確に表せるときだけ-some---採用記録の訂正) | `_checked` の変換は「正確に表せるときだけ some」 | ○ | —（記録の訂正） |
| [R-L0-29](L0-conventions.md#r-l0-29-負の開始位置を丸めずに-abort-にする--保留) | 負の開始位置を丸めずに abort にする | 保留 | almide-dojo#63 |
| [R-L0-30](L0-conventions.md#r-l0-30-引数名は役割ごとに-1-つ---採用) | 引数名は役割ごとに 1 つ | ○ | #3987 |
| [R-L0-G](L0-conventions.md#ゲートの束r-l0-g---採用) | ゲートの束（R-L0-G） | ○ | #3949 |
| [R-L1-01](L1-categories.md#r-l1-01-両端キュー-deque---採用) | 両端キュー `deque` | ○ | #3932 |
| [R-L1-02](L1-categories.md#r-l1-02-優先度キュー-heap---採用) | 優先度キュー `heap` | ○ | #3933 |
| [R-L1-03](L1-categories.md#r-l1-03-任意精度整数-bigint---採用) | 任意精度整数 `bigint` | ○ | #3931 |
| [R-L1-04](L1-categories.md#r-l1-04-random-にシード付き乱数と暗号用の乱数を足す---採用) | `random` に、シード付き乱数と暗号用の乱数を足す | ○ | #3939 |
| [R-L1-05](L1-categories.md#r-l1-05-hash-を鍵を使わない要約の置き場にするcrc32adler-32-を-zlib-から移しmd5sha-1sha-512-を足す---採用) | `hash` を「鍵を使わない要約」の置き場にする。CRC32・Adler-32 を zlib から移し、MD5・SHA-1・SHA-512 を足す | ○ | #3914 |
| [R-L1-06](L1-categories.md#r-l1-06-net-に-udpdnsip-アドレスを足す---採用) | `net` に UDP・DNS・IP アドレスを足す | ○ | #3930 |
| [R-L1-07](L1-categories.md#r-l1-07-uuid---採用) | `uuid` | ○ | #3934 |
| [R-L1-08](L1-categories.md#r-l1-08-utf-16latin-1-との変換---採用新しいモジュールにはしない) | UTF-16・Latin-1 との変換 | ○ | #3948 |
| [R-L1-20](L1-categories.md#r-l1-20-固定幅の数のモジュールint8uint64float32float64---統合しない) | 固定幅の数のモジュール（`int8`〜`uint64`、`float32`、`float64`） | × | — |
| [R-L1-21](L1-categories.md#r-l1-21-compute-と-duration---統合しないadr-0001) | `compute` と `duration` | × | — |
| [R-L1-22](L1-categories.md#r-l1-22-matrix-を標準に残し3852-の切り出しを終える---採用) | `matrix` を標準に残し、#3852 の切り出しを終える | ○ | #3852 |
| [R-L1-23](L1-categories.md#r-l1-23-hex-と-base64-を別のモジュールのまま残す---まとめない) | `hex` と `base64` を別のモジュールのまま残す | × | — |
| [R-L1-25](L1-categories.md#r-l1-25-os-とプロセスまわりの持ち分を-1-つに決める---採用) | OS とプロセスまわりの持ち分を 1 つに決める | ○ | #3851 |
| [R-L1-26](L1-categories.md#r-l1-26-生ポインタを-ffi-にunix-ソケットと共有メモリを-unix-に移す---採用) | 生ポインタを `ffi` に、Unix ソケットと共有メモリを `unix` に移す | ○ | #3853 |
| [R-L1-27](L1-categories.md#r-l1-27-json-はテキストとの変換value-は動的な値の木---採用) | `json` はテキストとの変換、`value` は動的な値の木 | ○ | #3953 |
| [R-L1-28](L1-categories.md#r-l1-28-url-の符号化は-url-に置く---採用) | URL の符号化は `url` に置く | ○ | #3950 |
| [R-L1-29](L1-categories.md#r-l1-29-html-と-testing---動かさない) | `html` と `testing` | × | — |
| [R-L1-30](L1-categories.md#r-l1-30-bool-モジュールを置きboolparse-を足す---採用) | `bool` モジュールを置き、`bool.parse` を足す | ○ | #3980 |
| [R-args-01](L2-args.md#r-args-01-argsflag-を-argshas_flag-にし頭文字の自動の短縮形をやめる---採用) | `args.flag` を `args.has_flag` にし、頭文字の自動の短縮形をやめる | ○ | #3893 |
| [R-args-02](L2-args.md#r-args-02-argsraw-と-argsoption_or-を撤去する---採用) | `args.raw` と `args.option_or` を撤去する | ○ | #3851 |
| [R-args-03](L2-args.md#r-args-03-positional_at-を撤去する---採用) | `positional_at` を撤去する | ○ | #3979 |
| [R-base64-01](L2-base64.md#r-base64-01-decode-は--だけdecode_url-は--_-だけを受け取る---採用) | `decode` は `+/` だけ、`decode_url` は `-_` だけを受け取る | ○ | #3894 |
| [R-base64-02](L2-base64.md#r-base64-02--を付けない符号化を足す---不採用) | `=` を付けない符号化を足す | × | — |
| [R-base64-03](L2-base64.md#r-base64-03-hex-とモジュールをまとめる---不採用r-l1-23) | hex とモジュールをまとめる | × | — |
| [R-bigint-01](L2-bigint.md#r-bigint-01-任意精度整数のモジュール-bigint-を足す---採用) | 任意精度整数のモジュール `bigint` を足す | ○ | #3931 |
| [R-bigint-02](L2-bigint.md#r-bigint-02-123n-のような-bigint-のリテラルを足す---不採用) | `123n` のような BigInt のリテラルを足す | × | — |
| [R-bigint-03](L2-bigint.md#r-bigint-03-int-と-bigint-を混ぜた演算を自動で広げる---不採用) | `Int` と `BigInt` を混ぜた演算を自動で広げる | × | — |
| [R-bigint-04](L2-bigint.md#r-bigint-04-bigintaddsubmuldivremneg-を公開しない演算子だけにする---採用) | `bigint.add`/`sub`/`mul`/`div`/`rem`/`neg` を公開しない（演算子だけにする） | ○ | #3973 |
| [R-bigint-05](L2-bigint.md#r-bigint-05-bigintclamp-を足す---採用) | `bigint.clamp` を足す | ○ | #3972 |
| [R-bigint-06](L2-bigint.md#r-bigint-06-uint64-と-bigint-を値を失わずに行き来させる---採用) | `uint64` と `BigInt` を値を失わずに行き来させる | ○ | #3972 |
| [R-bool-01](L2-bool.md#r-bool-01-boolparse-を足す---採用) | `bool.parse` を足す | ○ | #3980 |
| [R-bytes-01](L2-bytes.md#r-bytes-01-位置を指定した上書きを-put_型-にする---採用) | 位置を指定した上書きを `put_<型>` にする | ○ | #3898 |
| [R-bytes-02](L2-bytes.md#r-bytes-02-1-バイトの追記は-push1-バイトの上書きは-put_u8-だけにする---採用) | 1 バイトの追記は `push`、1 バイトの上書きは `put_u8` だけにする | ○ | #3851 |
| [R-bytes-03](L2-bytes.md#r-bytes-03-書き足しを-append_-に揃え型--向き--操作の行列を閉じる---採用) | 書き足しを `append_` に揃え、型 × 向き × 操作の行列を閉じる | ○ | #3851 |
| [R-bytes-04](L2-bytes.md#r-bytes-04-endian-引数の系統12-関数を撤去し接尾辞の系統を唯一にする---採用) | `Endian` 引数の系統（12 関数）を撤去し、接尾辞の系統を唯一にする | ○ | #3851 |
| [R-bytes-05](L2-bytes.md#r-bytes-05-長さ前置きの補助関数と-read_string_at-を撤去しappend_bytes-を足す---採用) | 長さ前置きの補助関数と `read_string_at` を撤去し、`append_bytes` を足す | ○ | #3852 |
| [R-bytes-06](L2-bytes.md#r-bytes-06-生ポインタを-ffi-に移す---採用) | 生ポインタを `ffi` に移す | ○ | #3853 |
| [R-bytes-07](L2-bytes.md#r-bytes-07-get_or-を撤去しeof-を-is_eof-に改名する---採用) | `get_or` を撤去し、`eof` を `is_eof` に改名する | ○ | #3913 |
| [R-bytes-08](L2-bytes.md#r-bytes-08-列の関数を-liststring-と同じ綴りにする---採用) | 列の関数を list・string と同じ綴りにする | ○ | #3899 |
| [R-bytes-09](L2-bytes.md#r-bytes-09-位置指定の読みを番兵-0-を返す-read_型-から-option-を返す-get_型-に移す---採用) | 位置指定の読みを、番兵 0 を返す `read_<型>` から Option を返す `get_<型>` に移す | ○ | #3897 |
| [R-bytes-10](L2-bytes.md#r-bytes-10-utf-8-の両方向は-bytes-が持つ---採用据え置き) | UTF-8 の両方向は bytes が持つ | ○ | #2078 |
| [R-bytes-11](L2-bytes.md#r-bytes-11-範囲外の位置への書き込みを-abort-にする---採用) | 範囲外の位置への書き込みを abort にする | ○ | #3937 |
| [R-bytes-12](L2-bytes.md#r-bytes-12-slice-の負の開始位置を-0-に丸める---採用) | `slice` の負の開始位置を 0 に丸める | ○ | #3944 |
| [R-bytes-13](L2-bytes.md#r-bytes-13-読み位置を進める族を-next_型-にし値-次の位置-を返す---採用) | 読み位置を進める族を `next_<型>` にし、`(値, 次の位置)` を返す | ○ | #3962 |
| [R-bytes-14](L2-bytes.md#r-bytes-14-bytesheap_savebytesheap_restore-を公開の面から外す---採用) | `bytes.heap_save`・`bytes.heap_restore` を公開の面から外す | ○ | #3970 |
| [R-bytes-15](L2-bytes.md#r-bytes-15-i8-の行を読み書きの行列に足す---採用) | `i8` の行を読み書きの行列に足す | ○ | #3970 |
| [R-bytes-16](L2-bytes.md#r-bytes-16-bytes-も--で連結しbytesconcat-を撤去する---採用) | `Bytes` も `+` で連結し、`bytes.concat` を撤去する | ○ | #3970 |
| [R-bytes-17](L2-bytes.md#r-bytes-17-byteslines-を撤去するstringlines-と-r-の扱いが違う---採用) | `bytes.lines` を撤去する（`string.lines` と `\r` の扱いが違う） | ○ | #3970 |
| [R-bytes-18](L2-bytes.md#r-bytes-18-bytes-に高階関数の族を置くfilterfoldanyallcountfind---採用) | bytes に高階関数の族を置く（`filter`・`fold`・`any`・`all`・`count`・`find`） | ○ | #3970 |
| [R-bytes-19](L2-bytes.md#r-bytes-19-_array-の読みだけがある理由を規則に書く---採用文書だけ) | `_array` の読みだけがある理由を規則に書く | ○ | #3970 |
| [R-bytes-20](L2-bytes.md#r-bytes-20-firstlastpop-を足す---採用) | `first`・`last`・`pop` を足す | ○ | #3980 |
| [R-bytes-21](L2-bytes.md#r-bytes-21-bytesskip-を-next_-の形にする---不採用) | `bytes.skip` を `next_` の形にする | × | — |
| [R-bytes-22](L2-bytes.md#r-bytes-22-byteschunk-は-size--0-で-abort---採用) | `bytes.chunk` は `size <= 0` で abort | ○ | #3985 |
| [R-bytes-23](L2-bytes.md#r-bytes-23-bytesxor-は長さが違うと-abort---採用) | `bytes.xor` は長さが違うと abort | ○ | #3985 |
| [R-bytes-24](L2-bytes.md#r-bytes-24-bytesset-と-put_u8-を-1-つにする---不採用) | `bytes.set` と `put_u8` を 1 つにする | × | — |
| [R-compute-01](L2-compute.md#r-compute-01-computecompare-を足す---採用) | `compute.compare` を足す | ○ | #3902 |
| [R-compute-02](L2-compute.md#r-compute-02-duration-と統合するまたは読み出し関数を足す---不採用) | `duration` と統合する、または読み出し関数を足す | × | — |
| [R-compute-03](L2-compute.md#r-compute-03-compute-を共通のプロトコル-timeamount-に従わせる---採用) | `Compute` を共通のプロトコル `TimeAmount` に従わせる | ○ | #3964 |
| [R-datetime-01](L2-datetime.md#r-datetime-01-ミリ秒の現在時刻-datetimenow_ms-を足す---採用) | ミリ秒の現在時刻 `datetime.now_ms` を足す | ○ | #3851 |
| [R-datetime-02](L2-datetime.md#r-datetime-02-時刻専用の型instant-などを作る---不採用) | 時刻専用の型（`Instant` など）を作る | × | — |
| [R-datetime-03](L2-datetime.md#r-datetime-03-何もしない関数と演算子の双子を撤去する---採用) | 何もしない関数と演算子の双子を撤去する | ○ | #3903 |
| [R-datetime-04](L2-datetime.md#r-datetime-04-weekday-を数を返す関数にする---不採用) | `weekday` を数を返す関数にする | × | — |
| [R-datetime-05](L2-datetime.md#r-datetime-05-format-の--を--にする---不採用) | `format` の `%%` を `%` にする | × | — |
| [R-datetime-06](L2-datetime.md#r-datetime-06-add_単位-の族を行列として宣言し演算子の双子の検出から外す---採用記録とゲートの修正) | `add_<単位>` の族を行列として宣言し、「演算子の双子」の検出から外す | ○ | #3974 |
| [R-datetime-07](L2-datetime.md#r-datetime-07-from_parts-は範囲外の部品を-err-にする---採用) | `from_parts` は範囲外の部品を err にする | ○ | #3974 |
| [R-deque-01](L2-deque.md#r-deque-01-deque-モジュールを足す---採用) | `deque` モジュールを足す | ○ | #3932 |
| [R-deque-02](L2-deque.md#r-deque-02-deque-に高階関数の族と-containsclear-を足す---採用) | deque に高階関数の族と `contains`・`clear` を足す | ○ | #3968 |
| [R-duration-01](L2-duration.md#r-duration-01-durationcompare-を足す---採用) | `duration.compare` を足す | ○ | #3902 |
| [R-duration-02](L2-duration.md#r-duration-02-読み出し関数小数の構築子日の単位を足す---不採用) | 読み出し関数・小数の構築子・日の単位を足す | × | — |
| [R-duration-03](L2-duration.md#r-duration-03-duration-と-compute-を-1-つのプロトコル-timeamount-に従わせる---採用) | `Duration` と `Compute` を 1 つのプロトコル `TimeAmount` に従わせる | ○ | #3964 |
| [R-duration-04](L2-duration.md#r-duration-04-min分を-minutes-などの完全な綴りにする---不採用) | `min`（分）を `minutes` などの完全な綴りにする | × | — |
| [R-env-01](L2-env.md#r-env-01-os-まわりの持ち分を-1-つにする---採用) | OS まわりの持ち分を 1 つにする | ○ | #3851 |
| [R-env-02](L2-env.md#r-env-02-envvars-と-envremove-を足す---採用) | `env.vars` と `env.remove` を足す | ○ | #3908 |
| [R-env-03](L2-env.md#r-env-03-envget-を--で受ける形を自前の-effect-fn-と同じにする---採用r-l0-17-に従う) | `env.get` を `??` で受ける形を、自前の effect fn と同じにする | ○ | #3900 |
| [R-error-01](L2-error.md#r-error-01-errorcontexterrormessageerrorchain-を撤去しerror-モジュールを閉じる---採用) | `error.context`・`error.message`・`error.chain` を撤去し、`error` モジュールを閉じる | ○ | #3909 |
| [R-error-02](L2-error.md#r-error-02-errorcontext-を-resultcontext-として残す---不採用) | `error.context` を `result.context` として残す | × | — |
| [R-ffi-01](L2-ffi.md#r-ffi-01-ffi-を作りbytes-の生ポインタ-5-関数を移す---採用) | `ffi` を作り、`bytes` の生ポインタ 5 関数を移す | ○ | #3853 |
| [R-ffi-02](L2-ffi.md#r-ffi-02-ffiread-と-ffiwrite-を-effect-fn-にする---採用) | `ffi.read` と `ffi.write` を effect fn にする | ○ | #3974 |
| [R-ffi-03](L2-ffi.md#r-ffi-03-ffiwrite-の引数を書き込む先が先にする---採用) | `ffi.write` の引数を「書き込む先が先」にする | ○ | #3979 |
| [R-float-01](L2-float.md#r-float-01-floatfrom_intfrom_float32from_float64-を撤去する---採用) | `float.from_int`・`from_float32`・`from_float64` を撤去する | ○ | #3935 |
| [R-float-02](L2-float.md#r-float-02-ビット列の変換を-to_bits--from_bits-の対にそろえる---採用) | ビット列の変換を `to_bits` / `from_bits` の対にそろえる | ○ | #3910 |
| [R-float-03](L2-float.md#r-float-03-ieee-の定数-infinitynanepsilonmax_valuemin_value-を足す---採用) | IEEE の定数 `infinity`・`nan`・`epsilon`・`max_value`・`min_value` を足す | ○ | #3910 |
| [R-float-04](L2-float.md#r-float-04-trunc-と-is_finite-を足す---採用) | `trunc` と `is_finite` を足す | ○ | #3911 |
| [R-float-05](L2-float.md#r-float-05-いまの-floatsign-を-floatsignum-に改名する---採用) | いまの `float.sign` を `float.signum` に改名する | ○ | #3972 |
| [R-float-06](L2-float.md#r-float-06-floatminmax-の-nan-の扱いを-listminmax-とそろえる--保留) | `float.min`/`max` の NaN の扱いを `list.min`/`max` とそろえる | 保留 | —（測定は [almide-dojo#63](https://github.com/almide/almide-dojo/issues/63) の枠で） |
| [R-float-07](L2-float.md#r-float-07-round-と-to_fixed-の同点の丸めの違いを文書とゲートで固定する---採用関数の面は変えない) | `round` と `to_fixed` の同点の丸めの違いを文書とゲートで固定する | ○ | #3972 |
| [R-float-08](L2-float.md#r-float-08-float-の床方向の剰余-floatmod-を足す---不採用) | Float の床方向の剰余 `float.mod` を足す | × | — |
| [R-fs-01](L2-fs.md#r-fs-01-バイトの読み書きを無印の名前のまま-bytes-にし_raw-版を撤去する---採用) | バイトの読み書きを無印の名前のまま `Bytes` にし、`_raw` 版を撤去する | ○ | #3896 |
| [R-fs-02](L2-fs.md#r-fs-02-file_size-と-modified_at-を撤去しstat-に一本化する---採用) | `file_size` と `modified_at` を撤去し、`stat` に一本化する | ○ | #3912 |
| [R-fs-03](L2-fs.md#r-fs-03-2-つの族のゲートを新しい形に合わせる---採用) | 2 つの族のゲートを新しい形に合わせる | ○ | #3896 |
| [R-fs-04](L2-fs.md#r-fs-04-59-以上が持つが-almide-に無い役割---不採用いまは足さない) | 5/9 以上が持つが Almide に無い役割 | × | — |
| [R-fs-05](L2-fs.md#r-fs-05-writeappend-を-write_textappend_text-にしappend_bytes-を足す---採用) | `write`・`append` を `write_text`・`append_text` にし、`append_bytes` を足す | ○ | #3979 |
| [R-hash-01](L2-hash.md#r-hash-01-fnv-1a-を-fnv1a32b-bytes---int-の-1-つにする---採用) | FNV-1a を `fnv1a32(b: Bytes) -> Int` の 1 つにする | ○ | #3916 |
| [R-hash-02](L2-hash.md#r-hash-02-crc-32-と-adler-32-を-zlib-から移す---採用) | CRC-32 と Adler-32 を zlib から移す | ○ | #3914 |
| [R-hash-03](L2-hash.md#r-hash-03-md5sha-1sha-512-を足す---採用) | MD5・SHA-1・SHA-512 を足す | ○ | #3915 |
| [R-hash-04](L2-hash.md#r-hash-04-_hex-のセルの入力を-bytes-に変える---不採用) | `_hex` のセルの入力を Bytes に変える | × | — |
| [R-hash-05](L2-hash.md#r-hash-05-_hex-も-bytes-を受け取る完全性の規則-2-の見直し---採用) | `_hex` も `Bytes` を受け取る（完全性の規則 2 の見直し） | ○ | #3985 |
| [R-heap-01](L2-heap.md#r-heap-01-heap-モジュールを足す---採用) | `heap` モジュールを足す | ○ | #3933 |
| [R-heap-02](L2-heap.md#r-heap-02-heap-には中の並びに依存しないセルだけを置くcontainsanyallcountfilterclear---採用) | heap には中の並びに依存しないセルだけを置く（`contains`・`any`・`all`・`count`・`filter`・`clear`） | ○ | #3968 |
| [R-heap-03](L2-heap.md#r-heap-03-heapnew_with-の例を引き算にしない---採用文書だけ) | `heap.new_with` の例を引き算にしない | ○ | #3968 |
| [R-heap-04](L2-heap.md#r-heap-04-new_minnew_max-に分ける---不採用) | `new_min`・`new_max` に分ける | × | — |
| [R-hex-01](L2-hex.md#r-hex-01-3-関数を据え置く---採用変更なし) | 3 関数を据え置く | ○ | —（変更なし） |
| [R-hex-02](L2-hex.md#r-hex-02-decode_upper-を足す---不採用) | `decode_upper` を足す | × | — |
| [R-html-01](L2-html.md#r-html-01-5-関数を据え置く---採用変更なし) | 5 関数を据え置く | ○ | —（変更なし） |
| [R-html-02](L2-html.md#r-html-02-unescape-を足す---不採用) | `unescape` を足す | × | — |
| [R-html-03](L2-html.md#r-html-03-safehtml-の列をまとめる-htmljoin-を足す---不採用) | `SafeHtml` の列をまとめる `html.join` を足す | × | — |
| [R-http-01](L2-http.md#r-http-01-client-を-22--ハンドルにまとめる3854-2---採用) | client を 2×2 ＋ ハンドルにまとめる（#3854 §2） | ○ | #3854 |
| [R-http-02](L2-http.md#r-http-02-httpstatus-を-httpset_status-にする---採用) | `http.status` を `http.set_status` にする | ○ | #3854 |
| [R-http-03](L2-http.md#r-http-03-要求の側のヘッダを応答と同じ-3-つ組にする---採用) | 要求の側のヘッダを応答と同じ 3 つ組にする | ○ | #3917 |
| [R-http-04](L2-http.md#r-http-04-httpurl_decode-を撤去するr-l1-28---採用) | `http.url_decode` を撤去する（R-L1-28） | ○ | #3950 |
| [R-http-05](L2-http.md#r-http-05-client-の既定の振る舞い2827の判定---採用項目ごとに判定) | client の既定の振る舞い（#2827）の判定 | ○ | #2827 |
| [R-http-06](L2-http.md#r-http-06-応答の本文を差し替える-httpset_body-を足す---採用) | 応答の本文を差し替える `http.set_body` を足す | ○ | #3974 |
| [R-http-07](L2-http.md#r-http-07-応答のヘッダの-get_header-と要求の-req_header-の不揃い---不採用規則を書く) | 応答のヘッダの `get_header` と要求の `req_header` の不揃い | × | — |
| [R-http-08](L2-http.md#r-http-08-取り出しの-status_code-を-status-にする---採用epoch-17) | 取り出しの `status_code` を `status` にする | ○ | #3974 |
| [R-http-09](L2-http.md#r-http-09-ヘッダの全体を-liststring-string-で表す---採用) | ヘッダの全体を `List[(String, String)]` で表す | ○ | #3976 |
| [R-http-10](L2-http.md#r-http-10-query_params-を-liststring-string-にする---採用) | `query_params` を `List[(String, String)]` にする | ○ | #3976 |
| [R-http-11](L2-http.md#r-http-11-本文の-bytes-の族を閉じるreq_body_bytesset_body_bytes---採用) | 本文の `Bytes` の族を閉じる（`req_body_bytes`・`set_body_bytes`） | ○ | #3976 |
| [R-int-01](L2-int.md#r-int-01-intfrom_型-の列を撤去し元の型の-to_int-に移す---採用) | `int.from_<型>` の列を撤去し、元の型の `to_int` に移す | ○ | #3935 |
| [R-int-02](L2-int.md#r-int-02-折り返しの族を-4-セルにそろえto_u32to_u8-を撤去する---採用) | 折り返しの族を 4 セルにそろえ、`to_u32`/`to_u8` を撤去する | ○ | #3854 |
| [R-int-03](L2-int.md#r-int-03-床方向の割り算と剰余-intfloor_div--intmod-を足す---採用) | 床方向の割り算と剰余 `int.floor_div` / `int.mod` を足す | ○ | #3918 |
| [R-int-04](L2-int.md#r-int-04-基数の変換を-to_string_radix--parse_radix-の対にしfrom_hex-を撤去する---採用) | 基数の変換を `to_string_radix` / `parse_radix` の対にし、`from_hex` を撤去する | ○ | #3920 |
| [R-int-05](L2-int.md#r-int-05-bandborbxor-などを多数派の綴りに改名する---不採用) | `band`/`bor`/`bxor` などを多数派の綴りに改名する | × | — |
| [R-int-06](L2-int.md#r-int-06-論理右シフト-intbshr_logical-を足す---採用) | 論理右シフト `int.bshr_logical` を足す | ○ | #3919 |
| [R-int-07](L2-int.md#r-int-07-pop_count-を-count_ones-に改名する---採用) | `pop_count` を `count_ones` に改名する | ○ | #3985 |
| [R-io-01](L2-io.md#r-io-01-読み取りをeof-を区別する-1-行bytesにそろえる---採用) | 読み取りを「EOF を区別する 1 行」「Bytes」にそろえる | ○ | #3896 |
| [R-io-02](L2-io.md#r-io-02-iowrite_bytes-を-effect-fn-に直す---採用) | `io.write_bytes` を effect fn に直す | ○ | #3907 |
| [R-io-03](L2-io.md#r-io-03-改行なしで標準エラーに書く-ioeprint-を足しprint-の綴りから-ioprint-へ案内する---採用) | 改行なしで標準エラーに書く `io.eprint` を足し、`print` の綴りから `io.print` へ案内する | ○ | #3974 |
| [R-io-04](L2-io.md#r-io-04-標準入力をバイト列で全部読む-ioread_all_bytes-を足す---採用) | 標準入力をバイト列で全部読む `io.read_all_bytes` を足す | ○ | #3974 |
| [R-json-01](L2-json.md#r-json-01-型つきの取り出しと道筋の操作を-value-に移す---採用) | 型つきの取り出しと道筋の操作を `value` に移す | ○ | #3953 |
| [R-json-02](L2-json.md#r-json-02-parse-は文書の後ろの余計な文字を-err-にする---採用) | `parse` は文書の後ろの余計な文字を err にする | ○ | #3921 |
| [R-json-03](L2-json.md#r-json-03-stringify-と-stringify_pretty-を据え置く---採用変更なし) | `stringify` と `stringify_pretty` を据え置く | ○ | —（変更なし） |
| [R-list-00](L2-list.md#r-list-00-その場書き換えとコピーの様式を据え置きr-l0-05-の保険をかける---採用) | その場書き換えとコピーの様式を据え置き、R-L0-05 の保険をかける | ○ | #3928 |
| [R-list-01](L2-list.md#r-list-01-listget_or-を撤去し--に一本化する---採用) | `list.get_or` を撤去し `??` に一本化する | ○ | #3913 |
| [R-list-02](L2-list.md#r-list-02-listlength-を-epoch-16-で撤去する---採用) | `list.length` を epoch 16 で撤去する | ○ | #3851 |
| [R-list-03](L2-list.md#r-list-03-listwindows-を撤去しlistwindow-を残す---採用) | `list.windows` を撤去し、`list.window` を残す | ○ | #3851 |
| [R-list-04](L2-list.md#r-list-04-listbundled_probe-を公開面から外す---採用) | `list.bundled_probe` を公開面から外す | ○ | #3850 |
| [R-list-05](L2-list.md#r-list-05-listshuffle-を撤去しrandomshuffle-に一本化する---採用) | `list.shuffle` を撤去し、`random.shuffle` に一本化する | ○ | #3943 |
| [R-list-06](L2-list.md#r-list-06-比較関数の-sort_with-とキー関数の-min_bymax_by-を足す---採用) | 比較関数の `sort_with` と、キー関数の `min_by`・`max_by` を足す | ○ | #3924 |
| [R-list-07](L2-list.md#r-list-07-listunzip-を足す---採用) | `list.unzip` を足す | ○ | #3925 |
| [R-list-08](L2-list.md#r-list-08-listsum-と-listproduct-を-listfloat-でも使えるようにする---採用) | `list.sum` と `list.product` を `List[Float]` でも使えるようにする | ○ | #3922 |
| [R-list-09](L2-list.md#r-list-09-刻み幅つきの-listrange_step-を足す---採用) | 刻み幅つきの `list.range_step` を足す | ○ | #3923 |
| [R-list-10](L2-list.md#r-list-10-境界の扱いをそろえる-slice-の負の開始位置chunkwindow-の-n--0---採用) | 境界の扱いをそろえる: `slice` の負の開始位置、`chunk`・`window` の `n <= 0` | ○ | #3944 |
| [R-list-11](L2-list.md#r-list-11-e002-のもしかして表を生きている関数に向ける---採用) | E002 の「もしかして」表を、生きている関数に向ける | ○ | #3905 |
| [R-list-12](L2-list.md#r-list-12-多数派に届かない役割は足さない---不採用) | 多数派に届かない役割は足さない | × | — |
| [R-list-13](L2-list.md#r-list-13-範囲外の位置への書き込みを-abort-にする---採用) | 範囲外の位置への書き込みを abort にする | ○ | #3937 |
| [R-list-14](L2-list.md#r-list-14-listtail-を撤去しlistdropxs-1-に一本化する---採用) | `list.tail` を撤去し、`list.drop(xs, 1)` に一本化する | ○ | #3969 |
| [R-list-15](L2-list.md#r-list-15-listbinary_search-を順序のある要素型すべてで使えるようにする---採用) | `list.binary_search` を順序のある要素型すべてで使えるようにする | ○ | #3969 |
| [R-list-16](L2-list.md#r-list-16-listjoin-を-listbytes-にも使えるようにするbytesjoin-は置かない---採用) | `list.join` を `List[Bytes]` にも使えるようにする（`bytes.join` は置かない） | ○ | #3969 |
| [R-list-17](L2-list.md#r-list-17-binary_search_bybinary_search_with-を足す---採用) | `binary_search_by`・`binary_search_with` を足す | ○ | #3980 |
| [R-list-18](L2-list.md#r-list-18-listrepeat-を列全体の繰り返しにする---不採用) | `list.repeat` を列全体の繰り返しにする | × | — |
| [R-list-19](L2-list.md#r-list-19-min_withmax_with-を足す---採用) | `min_with`・`max_with` を足す | ○ | #3985 |
| [R-map-01](L2-map.md#r-map-01-mapinsert-と-mapdeleteその場書き換えを撤去する---採用) | `map.insert` と `map.delete`（その場書き換え）を撤去する | ○ | #3928 |
| [R-map-02](L2-map.md#r-map-02-mapget_or-を撤去して--に一本化する---採用) | `map.get_or` を撤去して `??` に一本化する | ○ | #3913 |
| [R-map-03](L2-map.md#r-map-03-mapto_list-を足す---不採用) | `map.to_list` を足す | × | — |
| [R-map-04](L2-map.md#r-map-04-mapmap-を-map_values-に改名するまたはコールバックを-k-v-にする---不採用) | `map.map` を `map_values` に改名する、またはコールバックを `(K, V)` にする | × | — |
| [R-map-05](L2-map.md#r-map-05-取り出して消す-pop合わせ方つきの-merge_withキーを変換する-map_keys-を足す---不採用) | 取り出して消す `pop`、合わせ方つきの `merge_with`、キーを変換する `map_keys` を足す | × | — |
| [R-map-06](L2-map.md#r-map-06-高階関数の-family-を-listmapset-の行列でゲートする---採用) | 高階関数の family を list・map・set の行列でゲートする | ○ | #3901 |
| [R-map-07](L2-map.md#r-map-07-mapentries-を-mapto_list-に改名するr-map-03-の見直し---採用) | `map.entries` を `map.to_list` に改名する（R-map-03 の見直し） | ○ | #3969 |
| [R-math-01](L2-math.md#r-math-01-型の決まる算術を-intfloat-に移す---採用) | 型の決まる算術を `int`・`float` に移す | ○ | #3851 |
| [R-math-02](L2-math.md#r-math-02-逆三角関数双曲線関数hypotcbrt-を足す---採用) | 逆三角関数・双曲線関数・`hypot`・`cbrt` を足す | ○ | #3926 |
| [R-math-03](L2-math.md#r-math-03-実数の数学関数は-math-に集めるsqrt-を-math-に戻しfloatsqrt-を撤去---採用r-math-01-の-sqrt-の部分を見直す) | 実数の数学関数は `math` に集める（`sqrt` を `math` に戻し、`float.sqrt` を撤去） | ○ | #3972 |
| [R-math-04](L2-math.md#r-math-04-expm1log1p-を足す---採用) | `expm1`・`log1p` を足す | ○ | #3980 |
| [R-math-05](L2-math.md#r-math-05-逆双曲線関数-asinhacoshatanh-を足す完全性の規則-2-の見直し---採用) | 逆双曲線関数 `asinh`・`acosh`・`atanh` を足す（完全性の規則 2 の見直し） | ○ | #3980 |
| [R-matrix-01](L2-matrix.md#r-matrix-01-推論用のカーネルを-almidennlib-へ移す---採用) | 推論用のカーネルを `almide/nnlib` へ移す | ○ | #3852 |
| [R-matrix-02](L2-matrix.md#r-matrix-02-fusion-の対象を内部の名前にする---採用) | fusion の対象を内部の名前にする | ○ | #3852 |
| [R-matrix-03](L2-matrix.md#r-matrix-03-32-bit-の試験版-matrixfloat32-を撤去する---採用) | 32 bit の試験版 `Matrix[Float32]` を撤去する | ○ | #3852 |
| [R-matrix-04](L2-matrix.md#r-matrix-04-select_rows-を撤去しgather_rows-に一本化する---採用) | `select_rows` を撤去し、`gather_rows` に一本化する | ○ | #3852 |
| [R-matrix-05](L2-matrix.md#r-matrix-05-形長さ添字の不一致を-abort-にする---採用) | 形・長さ・添字の不一致を abort にする | ○ | #3927 |
| [R-matrix-06](L2-matrix.md#r-matrix-06-撤去予定の別名-concat_cols_manyrow_dot-を-epoch-16-で撤去する---採用) | 撤去予定の別名 `concat_cols_many`・`row_dot` を epoch 16 で撤去する | ○ | #3851 |
| [R-matrix-07](L2-matrix.md#r-matrix-07-mul-を-matmul-に改名し要素ごとの積を足す---不採用) | `mul` を `matmul` に改名し、要素ごとの積を足す | × | — |
| [R-matrix-08](L2-matrix.md#r-matrix-08-matrixget-は範囲外で-option-を返す---採用) | `matrix.get` は範囲外で `Option` を返す | ○ | #3927 |
| [R-matrix-09](L2-matrix.md#r-matrix-09-要素ごとの二項演算を-matrixzip_with-で書けるようにする---採用) | 要素ごとの二項演算を `matrix.zip_with` で書けるようにする | ○ | #3972 |
| [R-matrix-11](L2-matrix.md#r-matrix-11-行方向の連結を-concat_rowslistmatrix-にしconcat_cols-と形をそろえる---採用) | 行方向の連結を `concat_rows(List[Matrix])` にし、`concat_cols` と形をそろえる | ○ | #3972 |
| [R-matrix-12](L2-matrix.md#r-matrix-12-to_bytes_f16_le-を足してバイト列の変換を対にする---採用) | `to_bytes_f16_le` を足して、バイト列の変換を対にする | ○ | #3972 |
| [R-matrix-13](L2-matrix.md#r-matrix-13-matrixpow-を撤去する---採用) | `matrix.pow` を撤去する | ○ | #3973 |
| [R-matrix-14](L2-matrix.md#r-matrix-14-matrix-の-addsubmuldivneg-を公開しない演算子だけにする---採用) | matrix の `add`/`sub`/`mul`/`div`/`neg` を公開しない（演算子だけにする） | ○ | #3973 |
| [R-matrix-15](L2-matrix.md#r-matrix-15-from_bytes__le-はデータが足りないと-none-を返すr-matrix-05-の見直し---採用) | `from_bytes_*_le` はデータが足りないと none を返す（R-matrix-05 の見直し） | ○ | #3978 |
| [R-net-01](L2-net.md#r-net-01-unix_shm_-を-unix-モジュールへ移す---採用) | `unix_*`・`shm_*` を `unix` モジュールへ移す | ○ | #3853 |
| [R-net-02](L2-net.md#r-net-02-udp-を足す---採用) | UDP を足す | ○ | #3930 |
| [R-net-03](L2-net.md#r-net-03-名前の解決と-ip-アドレスの型を足す---採用) | 名前の解決と IP アドレスの型を足す | ○ | #3929 |
| [R-net-04](L2-net.md#r-net-04-tcp_is_open-を-effect-fn-にする---採用) | `tcp_is_open` を effect fn にする | ○ | #3907 |
| [R-net-05](L2-net.md#r-net-05-tcp-のハンドルを不透明な型にする---不採用) | TCP のハンドルを不透明な型にする | × | — |
| [R-net-06](L2-net.md#r-net-06-udp_recv_from-の-3-つ組をレコードにする---採用) | `udp_recv_from` の 3 つ組をレコードにする | ○ | #3974 |
| [R-option-01](L2-option.md#r-option-01-optionunwrap_or-を-epoch-16-で撤去する---採用) | `option.unwrap_or` を epoch 16 で撤去する | ○ | #1107 |
| [R-option-02](L2-option.md#r-option-02-optioncollect-と-optioncollect_map-を残し置き場所の例外として登録する---採用据え置き) | `option.collect` と `option.collect_map` を残し、置き場所の例外として登録する | ○ | #3949 |
| [R-option-03](L2-option.md#r-option-03-抜けている役割の関数を足さない---不採用) | 抜けている役割の関数を足さない | × | — |
| [R-option-04](L2-option.md#r-option-04-option-と-result-の対応を行列のゲートにする---採用) | option と result の対応を行列のゲートにする | ○ | #3949 |
| [R-path-01](L2-path.md#r-path-01-components-と-relative_to-を足す---採用) | `components` と `relative_to` を足す | ○ | #3936 |
| [R-path-02](L2-path.md#r-path-02-safepathfrom_stringtrustedto_stringを撤去する---採用) | `SafePath`（`from_string`・`trusted`・`to_string`）を撤去する | ○ | #3971 |
| [R-path-03](L2-path.md#r-path-03-with_extension-を足す---採用) | `with_extension` を足す | ○ | #3985 |
| [R-process-01](L2-process.md#r-process-01-argv環境変数待ちstdin-を持ち主に返す---採用) | argv・環境変数・待ち・stdin を持ち主に返す | ○ | #3851 |
| [R-process-02](L2-process.md#r-process-02-processcommand-と-output-で一度きりの実行をまとめる---採用) | `ProcessCommand` と `output` で一度きりの実行をまとめる | ○ | #2587 |
| [R-process-03](L2-process.md#r-process-03-ハンドル2587を足しspawnis_alive-を置き換える---採用) | ハンドル（#2587）を足し、`spawn`・`is_alive` を置き換える | ○ | #2587 |
| [R-process-04](L2-process.md#r-process-04-pid-を-effect-fn-にする---採用) | `pid` を effect fn にする | ○ | #3907 |
| [R-process-05](L2-process.md#r-process-05-processpollwait-の形を-httppollwait-に合わせる---採用) | `process.poll`・`wait` の形を `http.poll`・`wait` に合わせる | ○ | #3974 |
| [R-process-06](L2-process.md#r-process-06-動いている子の-pid-を読む-processchild_pid-を足す---採用) | 動いている子の pid を読む `process.child_pid` を足す | ○ | #3974 |
| [R-process-07](L2-process.md#r-process-07-run-も-processcommand-を受け取りrun_in-を撤去する---採用) | `run` も `ProcessCommand` を受け取り、`run_in` を撤去する | ○ | #3974 |
| [R-random-01](L2-random.md#r-random-01-シード付きの生成器-rng-を足す---採用) | シード付きの生成器 `Rng` を足す | ○ | #3939 |
| [R-random-02](L2-random.md#r-random-02-暗号用の乱数-randombytes-を足す---採用) | 暗号用の乱数 `random.bytes` を足す | ○ | #3938 |
| [R-random-03](L2-random.md#r-random-03-並べ替えは-randomshuffle-に一本化する---採用) | 並べ替えは `random.shuffle` に一本化する | ○ | #3943 |
| [R-random-04](L2-random.md#r-random-04-randomint-の-max--min-を-abort-にする---不採用) | `random.int` の `max < min` を abort にする | × | — |
| [R-random-05](L2-random.md#r-random-05-シード付きの族は-bytes-のセルを持たないと宣言する---採用ゲートの宣言) | シード付きの族は `bytes` のセルを持たない、と宣言する | ○ | #3974 |
| [R-random-06](L2-random.md#r-random-06-randomintmin-max-の上端を含むことを変える---不採用文書だけ) | `random.int(min, max)` の上端を含むことを変える | × | — |
| [R-regex-01](L2-regex.md#r-regex-01-regexfull_match-を-regexis_full_match-に改名する---採用) | `regex.full_match` を `regex.is_full_match` に改名する | ○ | #3940 |
| [R-regex-02](L2-regex.md#r-regex-02-コンパイル済みの-regex-型を入れ不正なパターンを作る時点で止める---採用) | コンパイル済みの `Regex` 型を入れ、不正なパターンを作る時点で止める | ○ | #3942 |
| [R-regex-03](L2-regex.md#r-regex-03-パイプの取り違えを宣言の印で止める---不採用r-regex-02-の型で止まるため) | パイプの取り違えを、宣言の印で止める | × | — |
| [R-regex-04](L2-regex.md#r-regex-04-エスケープ位置全一致のグループ大文字小文字の区別なしを足す---採用) | エスケープ・位置・全一致のグループ・大文字小文字の区別なしを足す | ○ | #3941 |
| [R-regex-05](L2-regex.md#r-regex-05-一致ごとに関数で置換文字列を作る-regexreplace_with---採用) | 一致ごとに関数で置換文字列を作る `regex.replace_with` | ○ | #3941 |
| [R-regex-06](L2-regex.md#r-regex-06-名前つきグループを足さない---不採用) | 名前つきグループを足さない | × | — |
| [R-regex-07](L2-regex.md#r-regex-07-replace_first_with-を足して置換の-22-を閉じる---採用) | `replace_first_with` を足して、置換の 2×2 を閉じる | ○ | #3971 |
| [R-regex-08](L2-regex.md#r-regex-08-捕捉グループを-string-で返す---採用) | 捕捉グループを `String?` で返す | ○ | #3977 |
| [R-regex-09](L2-regex.md#r-regex-09-対象の文字列を第-1-引数にする---不採用) | 対象の文字列を第 1 引数にする | × | — |
| [R-result-01](L2-result.md#r-result-01-resultunwrap_or-を-epoch-16-で撤去する---採用) | `result.unwrap_or` を epoch 16 で撤去する | ○ | #1107 |
| [R-result-02](L2-result.md#r-result-02-resultfilter-の-3-引数を据え置く---不採用撤去も改名もしない) | `result.filter` の 3 引数を据え置く | × | — |
| [R-result-03](L2-result.md#r-result-03-resultto_err_option-を改名しない---不採用) | `result.to_err_option` を改名しない | × | — |
| [R-result-04](L2-result.md#r-result-04-抜けている役割の関数を足さない---不採用) | 抜けている役割の関数を足さない | × | — |
| [R-result-05](L2-result.md#r-result-05-resultpartition-を-result-に残す---採用据え置き) | `result.partition` を result に残す | ○ | #3949 |
| [R-set-01](L2-set.md#r-set-01-setcount-と-setfind-を足す---採用) | `set.count` と `set.find` を足す | ○ | #3901 |
| [R-set-02](L2-set.md#r-set-02-setis_superset-を足す---不採用) | `set.is_superset` を足す | × | — |
| [R-set-03](L2-set.md#r-set-03-setpartition-を足す---不採用) | `set.partition` を足す | × | — |
| [R-set-04](L2-set.md#r-set-04-setinsert-を-setadd-に改名する---不採用) | `set.insert` を `set.add` に改名する | × | — |
| [R-sized-01](L2-sized-numbers.md#r-sized-01-変換の行列を-12-個の運び手に広げ向きを元の型の側にそろえる---採用) | 変換の行列を 12 個の運び手に広げ、向きを元の型の側にそろえる | ○ | #3935 |
| [R-sized-02](L2-sized-numbers.md#r-sized-02-すべての運び手に-parse-と-compare-を置く---採用) | すべての運び手に `parse` と `compare` を置く | ○ | #3902 |
| [R-sized-03](L2-sized-numbers.md#r-sized-03-float32float64-に浮動小数点の-7-セルを置く---採用) | `float32`・`float64` に浮動小数点の 7 セルを置く | ○ | #3910 |
| [R-sized-04](L2-sized-numbers.md#r-sized-04-floatto_整数_saturating-8-関数を撤去する---採用) | `float.to_<整数>_saturating` 8 関数を撤去する | ○ | #3935 |
| [R-sized-05](L2-sized-numbers.md#r-sized-05-固定幅のモジュールをまとめる算術関数を足す---不採用) | 固定幅のモジュールをまとめる・算術関数を足す | × | — |
| [R-sized-06](L2-sized-numbers.md#r-sized-06-整数の運び手すべてに-parse_radixto_string_radix-を置く---採用) | 整数の運び手すべてに `parse_radix`・`to_string_radix` を置く | ○ | #3972 |
| [R-sized-07](L2-sized-numbers.md#r-sized-07-float32float64-に-is_nan-などの判定を足す---不採用) | `float32`・`float64` に `is_nan` などの判定を足す | × | — |
| [R-string-01](L2-string.md#r-string-01-stringjoin-を撤去し-listjoin-に一本化する---採用) | `string.join` を撤去し `list.join` に一本化する | ○ | #3947 |
| [R-string-02](L2-string.md#r-string-02-utf-8-の変換は両方向とも-bytes-に置きstringto_bytes-と-stringfrom_bytes-を撤去する---採用) | UTF-8 の変換は両方向とも `bytes` に置き、`string.to_bytes` と `string.from_bytes` を撤去する | ○ | #2078 |
| [R-string-03](L2-string.md#r-string-03-stringrun_length_encode-を撤去する---採用) | `string.run_length_encode` を撤去する | ○ | #3852 |
| [R-string-04](L2-string.md#r-string-04-stringlength-の撤去を-epoch-16-に載せる---採用) | `string.length` の撤去を epoch 16 に載せる | ○ | #3851 |
| [R-string-05](L2-string.md#r-string-05-stringcomparea-b---int-を足す---採用) | `string.compare(a, b) -> Int` を足す | ○ | #3902 |
| [R-string-06](L2-string.md#r-string-06-stringbyte_lens---int-を足す---採用) | `string.byte_len(s) -> Int` を足す | ○ | #3945 |
| [R-string-07](L2-string.md#r-string-07-utf-16-との変換を足すlatin-1-は足さない---採用) | UTF-16 との変換を足す（Latin-1 は足さない） | ○ | #3948 |
| [R-string-08](L2-string.md#r-string-08-stringsplit_whitespace-を足す---不採用) | `string.split_whitespace` を足す | × | — |
| [R-string-09](L2-string.md#r-string-09-他言語の綴りへのヒントを意味の合う書き方に直す---採用) | 他言語の綴りへのヒントを、意味の合う書き方に直す | ○ | #3905 |
| [R-string-10](L2-string.md#r-string-10-指定文字の-trimsplitnコードポイントのリスト化を足す---不採用) | 指定文字の trim、`splitn`、コードポイントのリスト化を足す | × | — |
| [R-string-11](L2-string.md#r-string-11-stringfrom_codepoint-を-optionstring-を返す形にする---採用) | `string.from_codepoint` を `Option[String]` を返す形にする | ○ | #3946 |
| [R-string-12](L2-string.md#r-string-12-stringcodepoint-を-stringto_codepoint-に改名する---採用) | `string.codepoint` を `string.to_codepoint` に改名する | ○ | #3979 |
| [R-string-13](L2-string.md#r-string-13-stringcount-を述語で数える形にする---不採用) | `string.count` を述語で数える形にする | × | — |
| [R-testing-01](L2-testing.md#r-testing-01-大小と含むの族の規則をゲートに書く---採用関数は変えない) | 大小と含むの族の規則をゲートに書く | ○ | #3949 |
| [R-testing-02](L2-testing.md#r-testing-02-assert_gtassert_lt-を順序のある型すべてで使えるようにする---採用) | `assert_gt`・`assert_lt` を順序のある型すべてで使えるようにする | ○ | #3980 |
| [R-unix-01](L2-unix.md#r-unix-01-unix-モジュールを作りnetunix_netshm_-を移す---採用) | `unix` モジュールを作り、`net.unix_*`・`net.shm_*` を移す | ○ | #3853 |
| [R-unix-02](L2-unix.md#r-unix-02-unixwait-を-unixselect-にする---採用) | `unix.wait` を `unix.select` にする | ○ | #3974 |
| [R-unix-03](L2-unix.md#r-unix-03-unixrecvunixsend-を-unixreadunixwrite-にする---採用) | `unix.recv`・`unix.send` を `unix.read`・`unix.write` にする | ○ | #3974 |
| [R-url-01](L2-url.md#r-url-01-フォーム形式の対-encode_form--decode_form-を足しhttpurl_decode-を撤去する---採用) | フォーム形式の対 `encode_form` / `decode_form` を足し、`http.url_decode` を撤去する | ○ | #3950 |
| [R-url-02](L2-url.md#r-url-02-クエリの読みを復号する-parse_query-にする---採用) | クエリの読みを、復号する `parse_query` にする | ○ | #3951 |
| [R-url-03](L2-url.md#r-url-03-相対-url-の解決-resolve-を足す---採用) | 相対 URL の解決 `resolve` を足す | ○ | #3952 |
| [R-url-04](L2-url.md#r-url-04-url-の欄を-option-にする---不採用) | `Url` の欄を Option にする | × | — |
| [R-url-05](L2-url.md#r-url-05-url-に-userinfo-を足す---採用) | `Url` に userinfo を足す | ○ | #3971 |
| [R-uuid-01](L2-uuid.md#r-uuid-01-uuid-モジュールを足す---採用) | `uuid` モジュールを足す | ○ | #3934 |
| [R-value-01](L2-value.md#r-value-01-valuestringify-を撤去する---採用) | `value.stringify` を撤去する | ○ | #3953 |
| [R-value-02](L2-value.md#r-value-02-種類の族を閉じる-as_object-と-get_種類-を置く---採用) | 種類の族を閉じる: `as_object` と `get_<種類>` を置く | ○ | #3955 |
| [R-value-03](L2-value.md#r-value-03-道筋の操作を-json-から移す---採用) | 道筋の操作を `json` から移す | ○ | #3953 |
| [R-value-04](L2-value.md#r-value-04-keys-を撤去する---採用) | `keys` を撤去する | ○ | #3954 |
| [R-value-05](L2-value.md#r-value-05-to_camel_case-と-to_snake_case-を撤去する---採用) | `to_camel_case` と `to_snake_case` を撤去する | ○ | #3852 |
| [R-value-06](L2-value.md#r-value-06-構築子-str-を-string-に改名する---不採用) | 構築子 `str` を `string` に改名する | × | — |
| [R-value-07](L2-value.md#r-value-07-valuefield-を-valuegetoptionにしてget_種類-の族の頭にする---採用) | `value.field` を `value.get`（Option）にして、`get_<種類>` の族の頭にする | ○ | #3971 |
| [R-value-08](L2-value.md#r-value-08-valuestr-を-valuestring-に改名する完全性の規則-5-の見直し---採用) | `value.str` を `value.string` に改名する（完全性の規則 5 の見直し） | ○ | #3985 |
| [R-zlib-01](L2-zlib.md#r-zlib-01-crc32-と-adler32-を-hash-に移す---採用) | `crc32` と `adler32` を `hash` に移す | ○ | #3914 |
| [R-zlib-02](L2-zlib.md#r-zlib-02-展開の-3-関数から-effect-を外す---採用) | 展開の 3 関数から `effect` を外す | ○ | #3956 |
| [R-zlib-03](L2-zlib.md#r-zlib-03-圧縮の出力を両-target-で同じにしeffect-を外す--保留) | 圧縮の出力を両 target で同じにし、`effect` を外す | 保留 | #3892 |
