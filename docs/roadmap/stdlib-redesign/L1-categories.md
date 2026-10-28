<!-- description: Stdlib redesign layer 1 — the module list against nine languages: categories to add, to keep apart, to move, and the ownership map for environment, time and processes -->
# L1: カテゴリ（モジュール）の一覧

> Last updated: 2026-10-21
> 前提: [L0](L0-conventions.md)
> 根拠の表: [facts/categories.md](facts/categories.md)（約 100 カテゴリ × 9 言語。● = 標準にある、◐ = 条件つきで標準にある、pkg = 公式パッケージだけ、[F] = 今回公式ドキュメントで確かめたセル）

## 判定の基準

**カテゴリを足す**のは、次の 3 つをすべて満たすときだけです。
1. 9 言語のうち 5 言語以上が標準に持っている（● と ◐ の合計）。
2. 純粋な計算で書けるか、WASI の標準インタフェースで wasm に乗る（R-L0-15）。
3. いまの関数や既存の裁定でまかなえない。

**カテゴリを分ける・まとめる・移す**のは、L0 の規則（R-L0-01 同じ役割には綴りを 1 つ、R-L0-02 主語の型のモジュールに置く）に反しているときだけです。多数派の分け方と違うだけでは動かしません。

---

## 1. いまの 44 モジュールと 9 言語

| 分類 | Almide | 9 言語での扱い（要約） | 判定 |
|---|---|---|---|
| 数 | `int` `float` `math` | 9/9 が持つ | 据え置き（中身は L2） |
| 固定幅の数 | `int8`〜`uint64`、`float32` `float64` | 型ごとのモジュールを持つ言語は 0。Rust・Kotlin・Swift は型のメソッド、Go は用途別のパッケージ | R-L1-20 |
| 文字列 | `string` `regex` | 9/9 | 据え置き |
| コレクション | `list` `map` `set` | 9/9 | 据え置き |
| Option・Result | `option` `result` `error` | Rust・Gleam・OCaml・Swift・Kotlin が持つ | `option` と `result` は据え置き。`error` の 3 関数は重複か転送なので撤去し、モジュールは空になります（[L2-error](L2-error.md)） |
| バイト列 | `bytes` | 9/9 | 中身は L2。生ポインタは R-L1-26 |
| 符号化 | `base64` `hex` `url` `html` | base64 と hex を 1 つにまとめる言語は 5 | R-L1-23 |
| データ形式 | `json` `value` | JSON は 8/9。形式に依存しない動的な値を標準に持つのは Gleam の dynamic だけ | R-L1-27 |
| ハッシュ・圧縮 | `hash` `zlib` | 非暗号ハッシュは 6+2、CRC32 は Go・Python・Erlang・JVM がハッシュ側に置く | R-L1-05 |
| 時間 | `datetime` `duration` `compute` | compute に当たるものは 9 言語に無い | R-L1-21 |
| OS・プロセス | `env` `process` `args` `fs` `path` `io` | argv と環境変数を 1 つのモジュールに置く言語が多数派 | R-L1-25 |
| ネットワーク | `net` `http` | TCP は 9/9。UDP・DNS・IP アドレス型は 7 | R-L1-06、R-L1-26 |
| 乱数 | `random` | シード付き乱数は 6、暗号用の乱数（CSPRNG）は 6 | R-L1-04 |
| 行列 | `matrix` | 標準に持つ言語は 0 | R-L1-22 |
| テスト | `testing` | 5+1 | 据え置き |

---

## 2. 足すカテゴリ

### R-L1-01 両端キュー `deque` — ○ 採用

- (a) 7 言語が標準に持っています（Rust の `VecDeque`、Python の `collections.deque`、Kotlin の `ArrayDeque`、OCaml の `Queue` など）。いまの Almide で先頭から取り出すと、`list.drop(xs, 1)` で毎回 O(n) かかります。
- (b) 純粋な計算です。host の機能は要りません。
- (c) 幅優先探索のようなアルゴリズム問題で、LLM は他言語の deque を前提に書きます。無いと、自前のキューを毎回書くことになり、書き間違いが増えます。
- (d) 追加です（新しいモジュール）。
- 関数（L0 の動詞に従う）: `deque.new`、`from_list`、`to_list`、`len`、`is_empty`、`push`、`push_front`、`pop`、`pop_front`、`first`、`last`。`push_*`・`pop_*` は `list.push`・`list.pop` と同じく、`mut` 引数を取るその場書き換えです（R-L0-04 の 3: 1 つの動詞は全モジュールで同じ様式）。詳細は [L2-deque](L2-deque.md)。
- ゲート: list と同じ境界入力の行列テストを使います。native と wasm で結果が一致することを確かめる fixture を置きます。
- issue: #3932

### R-L1-02 優先度キュー `heap` — ○ 採用

- (a) 6 言語が持っています（Rust の `BinaryHeap`、Go の `container/heap`、Python の `heapq`、OCaml の `Pqueue` など）。
- (b) 純粋な計算です。
- (c) ダイクストラ法や上位 k 件など、よく出る問題の定番の道具です。
- (d) 追加です。最小ヒープを既定にします（Python・Go・OCaml と同じ）。キーを指定する版は `heap.new_by(f)`（R-L0-10）にします。
- issue: #3933

### R-L1-03 任意精度整数 `bigint` — ○ 採用

- (a) 6 言語が持っています（Go の `math/big`、Python の `int`、JS の `BigInt`、Elixir の整数、JVM の `BigInteger`）。
- (b) 純粋な計算です。native と wasm で 1 バイトも違わない結果を、ADR-0016（数値の決定性）の約束に入れます。
- (c) 階乗や大きなフィボナッチ数などの問題で、いまは `Int` が折り返して、静かに間違った答えになります。
- (d) 追加です。演算子は ADR-0005 の desugar に乗せます（`+` は `bigint.add`）。
- issue: #3931

### R-L1-04 `random` に、シード付き乱数と暗号用の乱数を足す — ○ 採用

- (a) シード付き乱数は 6 言語、CSPRNG（暗号用の乱数）も 6 言語が持っています。いまの `random` は 4 関数で、シードを渡せません。
- (b) シード付き乱数は純粋な計算です（PCG などのアルゴリズムを Almide で書きます。native と wasm で同じ列になります）。暗号用の乱数は `wasi:random` の `get-random-bytes` で wasm に乗ります。
- (c) テストで再現できる乱数が欲しいとき、LLM は `Random(seed)` 形の API を書きます。
- (d) 追加です。`random.seeded(seed) -> Rng`、`random.next_int(rng, lo, hi) -> (Int, Rng)` などのコピー版と、`random.bytes(n)`（effect）を足します。
- issue: #3939

### R-L1-05 `hash` を「鍵を使わない要約」の置き場にする。CRC32・Adler-32 を zlib から移し、MD5・SHA-1・SHA-512 を足す — ○ 採用

- (a) MD5・SHA-1・SHA-512・SHA-3 は 7 言語が持っています。CRC32 は Go（`hash/crc32`）と Python（`zlib.crc32`）、Erlang、JVM が持っていて、置き場所は割れています。Almide は、要約を返す関数を `hash` に集めます（R-L0-02: 結果の種類が同じなら同じモジュール）。
- (b) 純粋な計算です。
- (c) 「CRC32 は zlib に」は Python 以外では当たりません。`hash` にあれば、名前から探せます。
- (d) `hash.crc32` と `hash.adler32` を追加し、`zlib.crc32` と `zlib.adler32` を撤去します（R-L0-14 の窓を使います。詳細は [L2-hash](L2-hash.md) R-hash-02 と [L2-zlib](L2-zlib.md) R-zlib-01）。`hash.md5`、`sha1`、`sha512` と、それぞれの `_hex` 版を追加します。鍵を使うもの（HMAC、AEAD、署名）は、#2596 の `crypto` に置きます。
- issue: #3914

### R-L1-06 `net` に UDP・DNS・IP アドレスを足す — ○ 採用

- (a) 7 言語が持っています。
- (b) `wasi:sockets` の `udp` と `ip-name-lookup` で wasm に乗ります。TCP と同じく、ADR-0023 の経路（p3 shim）に乗せます。新しい host op は足しません。
- (d) 追加です。
- issue: #3930

### R-L1-07 `uuid` — ○ 採用

- (a) 5 言語が持っています（Go 1.27 の `uuid`、Python、JS の `crypto.randomUUID`、Kotlin、Swift）。
- (b) v4 は乱数（`wasi:random`）、v7 は時計と乱数（`wasi:clocks` と `wasi:random`）を使います。どちらも effect です。`uuid.parse` と `uuid.to_string` は純粋な計算です。
- (d) 追加です（`uuid.v4()`、`uuid.v7()`、`uuid.parse`、`uuid.to_string`）。
- issue: #3934

### R-L1-08 UTF-16・Latin-1 との変換 — ○ 採用（新しいモジュールにはしない）

- (a) 8 言語が持っています。
- (b) 純粋な計算です。
- (d) `string.to_utf16`、`string.from_utf16` などを string の関数として足します（L2-string で判定します）。新しいモジュールにはしません。R-L0-07 に従い、入れ物から組み立てるのは先の型のモジュールです。
- issue: L2-string の案で起票します。

---
- issue: #3948

### R-L1-30 `bool` モジュールを置き、`bool.parse` を足す — ○ 採用

- いま: `int.parse`・`float.parse`・各固定幅の型の `parse` はあるのに、`Bool` を文字列から読む関数がありません。設定値を `s == "true"` で読むと、`"ture"` のような書き誤りが黙って `false` になります。
- 案: `bool` モジュールを置き（`int`・`float` と同じく import なしで使える）、`fn bool.parse(s: String) -> Result[Bool, String]` を足します。受け取るのは `"true"` と `"false"` だけです（前後の空白・大文字は err）。中身は [L2-bool](L2-bool.md)。
- 根拠: L1 の追加基準（5 言語以上）。R-L0-25（文字列からの変換は先の型の `parse`）。
- (a) Rust `"true".parse::<bool>()`、Go `strconv.ParseBool`、OCaml `bool_of_string_opt`、Kotlin `toBooleanStrictOrNull`、Swift `Bool("true")`。どれも不正な入力を拒みます。
- issue: #3980

### R-L1-31 秘密の値を包む `secret` — ○ 採用

- いま: パスワードや API キーを持つレコードは、埋め込み・`assert_eq` の失敗表示・`almide test` の報告で全フィールドをそのまま出します（#4107）。フィールドに付けた `@redact` のような属性は読まれず、黙って何も守りません。
- 案: `secret` モジュールを置き（自動 import しない。`import secret`）、型 `Secret[A]` と `secret.new`・`secret.reveal` を足します。どの表示経路も `Secret(***)` を出し、`==` は String / Bytes だけ定数時間で比べ、Codec は decode だけ導出します。中身は [L2-secret](L2-secret.md)。
- 根拠: 専用の型を持つのは 2/8（Rust secrecy・pydantic）で、5 言語の基準には届きません。それでも採るのは、1 つの宣言から全表示経路を導く形が型だけだからです（フィールド属性は 1 つのレコードの 1 経路しか守れない）。オーナー裁定 2026-10-24。
- issue: #4107

## 3. 足さないカテゴリ（× と理由）

| カテゴリ | 持つ言語 | 判定 | 理由 |
|---|---:|---|---|
| 遅延イテレータ | 8 | × | `|>` の連なりは fusion（融合）で 1 回のループになります（裁定 「暗黙の `|>` は fusion」）。遅延の型を別に足すと、同じ処理に書き方が 2 つできます（R-L0-01）。 |
| 文字列ビルダー | 8 | × | `string.push(mut s, x)` が、その場で書き足す関数として既にあります。 |
| printf 系の書式化 | 8 | × | 文字列の埋め込み `"${x}"` と `float.to_fixed`、`string.pad_start` で書けます。書式の小さな言語をもう 1 つ足すと、LLM の書き間違いの元になります。 |
| ロック・アトミック・スレッド・チャネル・async・取り消し | 5〜8 | × | 並行性は `fan` の決定的なデータ並列だけにする、という既存の方針があります（concurrency-stance）。取り消しと締め切りは `fan.timeout` と `fan.min(duration)`（#3788）が担います。 |
| バイナリ直列化 | 7 | × | `bytes` の読み書き関数の族（`append_<型>`、`read_<型>`）が既にあります。 |
| タイムゾーン（IANA） | 6 | × | 標準から外し、package で提供します。tz データベースは数百 KB あり、wasm では host に読みに行くか埋め込むかのどちらかになります。標準の `datetime` は、UTC と固定のオフセットだけを扱う決定的なものに保ちます。tz データを同梱する package（例: `almide/tz`）を推奨の置き場所とします。 |
| ソート済みマップ | 5 | × | `map` は挿入順で決定的です。順序が欲しいときは `map.entries(m) |> list.sort_by(...)` で書けます。2 つめの map 型を足すと、どちらを使うかの選択が増えます。 |
| XML | 6 | × | package にします。標準のデータ形式は JSON だけです。 |
| 暗号（AES、署名、TLS） | 6 | ○（既存） | #2596 で、純粋な Almide の `crypto` として進めます（AEAD、Ed25519、HMAC）。TLS は `http` の中にあります。 |
| ロギング | 5 | × | `eprintln` で標準エラーに書けます。重要度での振り分けはアプリ側の関心事です。 |
| 端末・ANSI | 5 | × | エスケープ列は文字列で書けます。package にします。 |
| tar・zip、bzip2・zstd | 5 | × | package にします。標準は `zlib`（deflate・gzip）だけです。 |
| フラグ解析の本格版 | 5 | × | `args` は最小の読み取り器のままにします。ヘルプ文の生成などはアプリ側の関心事です。 |
| タイマー | 5 | × | `fan.timeout` と、`env.sleep_ms` で書けます。 |

---

## 4. 分け方・置き場所

### R-L1-20 固定幅の数のモジュール（`int8`〜`uint64`、`float32`、`float64`） — × 統合しない

- (a) 型ごとにモジュールを持つ言語は 0 です。しかし多数派は、型にメソッドを付けています。Almide にはメソッドがない（UFCS だけ）ので、R-L0-02（主語の型のモジュールに置く）を当てはめると、型ごとのモジュールになります。
- (c) `x |> int8.to_int16` の形で、パイプの先頭の型からモジュールが決まります。
- 中身の不揃い（`int.to_u32` と `int.to_uint32`、sized int から Int への `to_int` がないこと）は、L2 で判定します（R-L0-07）。

### R-L1-21 `compute` と `duration` — × 統合しない（ADR-0001）

ADR-0001 が、決定的な計算時間（`Compute`）と実時間（`Duration`）を、互いに変換できない別の型と決めています。9 言語に `compute` に当たるものはありません。それでも、決定性を約束する Almide 固有の理由があるので、多数派に合わせません。

### R-L1-22 `matrix` を標準に残し、#3852 の切り出しを終える — ○ 採用

- (a) 標準に行列を持つ言語は 0 です。
- 残す理由: コンパイラの書き換え規則（`@rewrite`、egg による fusion）が、`matrix` の演算を対象にしています（`crates/almide-codegen/src/target.rs`）。package は、いまのところ書き換え規則を持ち込めません。
- そのうえで、推論用のカーネルと fusion の対象関数を表から外す #3852 を進めます。汎用の密行列演算だけを残します。
- issue: #3852（既存）

### R-L1-23 `hex` と `base64` を別のモジュールのまま残す — × まとめない

5 言語は 1 か所にまとめていますが、まとめたモジュールの名前が揃っていません（Elixir は `Base`、Deno は `@std/encoding`、Python は `base64` の中の `b16encode`）。Go は `encoding/hex` と `encoding/base64` に分けています。R-L0-00 に従い、多数派の綴りがないので動かしません。

### R-L1-25 OS とプロセスまわりの持ち分を 1 つに決める — ○ 採用

| 持ち分 | モジュール | 撤去する重複 |
|---|---|---|
| argv、環境変数、作業ディレクトリ、OS 名 | `env` | `args.raw`（= `env.args`）、`process.args`（→ `env.args`）、`process.env`（純粋な fn で環境を読む穴） |
| 時計（実時間・単調時計） | `datetime` | `env.unix_timestamp`（= `datetime.now`）、`env.millis` は `datetime.now_ms` に移します |
| 待ち | `env.sleep_ms` | `process.sleep`（負の値の扱いだけが違う） |
| 一時ディレクトリ・ファイル | `fs` | `env.temp_dir`（= `fs.temp_dir`） |
| 標準入力 | `io` | `process.stdin_lines`（→ `string.lines(io.read_all()!)`） |
| 子プロセス、自分の終了、pid | `process` | — |
| フラグの読み取り | `args`（`env.args` の上に作る） | — |

- (a) argv と環境変数を同じモジュールに置くのは、Rust の `std::env`、Go の `os`、Kotlin の `System` などです。
- (c) 同じ値を返す関数が 2 つあり、しかも端の挙動が違うと、LLM はどちらで書いても半分は外れます。
- (d) 撤去です。R-L0-14 に従います。
- issue: #3851 B（既存）。`env.millis` の移動は新しく追加します。

### R-L1-26 生ポインタを `ffi` に、Unix ソケットと共有メモリを `unix` に移す — ○ 採用

#3853 の案どおりにします。`bytes` は全ファイルから import なしで使えるので、メモリ安全でない関数を置きません。`net` は wasm の `wasi:sockets` で乗るものだけにします。`unix.*` は、裁定済みの暫定 host op（104〜116）のまま移します。
- issue: #3853（既存）

### R-L1-27 `json` はテキストとの変換、`value` は動的な値の木 — ○ 採用

持ち分を決めます。テキストとの間の変換（`parse`、`stringify`）は `json` に置きます。木の組み立てと取り出しは `value` に置きます。`value.stringify` は `json.stringify` と重複しているので撤去します（R-L0-01）。詳細は L2 で判定します。
- issue: #3953

### R-L1-28 URL の符号化は `url` に置く — ○ 採用

`http.url_decode`（`+` を空白に戻すフォーム形式の復号）は URL の符号化の役割なので、`url` に新しく置く `url.decode_form` に移して撤去します（R-L0-01、R-L0-02）。書き換えは `url.decode_form(s) ?? s` です。詳細は [L2-url](L2-url.md) の R-url-01 と [L2-http](L2-http.md) の R-http-04 で判定します。
- issue: #3950

### R-L1-29 `html` と `testing` — × 動かさない

`html` は 5 関数です。「エスケープ済みの HTML を別の型にする」設計（SafeHtml）は、Go の `html/template` と同じ考えで、取り違えを型で防ぎます。`testing` は 5 言語が標準に持っています。

---

## 5. 予定されているもの（参考）

- `kv`（キーと値の保存、#2699）は ADR-0020 の手順として進行中です。ここでは扱いません。
- `crypto`（#2596）は R-L1-05 と対になります。鍵を使わない要約は `hash`、鍵を使うものは `crypto` に置きます。
