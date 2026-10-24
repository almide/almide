<!-- description: Stdlib redesign layer 0 — the cross-module naming, placement, mutation, failure and conversion rules every L1/L2/L3 verdict derives from -->
# L0: 全モジュールに共通する規則

> Last updated: 2026-10-21
> 根拠の表: [facts/naming-conventions.md](facts/naming-conventions.md)（9 言語の公式ドキュメントから集めた事実。(F) は今回取り寄せて確かめた箇所）
> 状態の索引: [README.md](README.md)

標準関数 999 個（44 モジュール）のリデザインでは、全体の統一感を最優先にします。
このページの規則が「上の層の裁定」で、L1（カテゴリ）・L2（モジュールの中身）・L3（関数 1 個）の判定は、すべてここから導きます。
個々の関数を名前で特別扱いしません。規則は、シグネチャ・戻り型・`mut` 宣言など、宣言 1 か所から機械的に判定できる形で書きます。

読み方の約束:
- `mut`（読み: ミュート）= 呼び出し元の `var` をその場で書き換える引数
- in-place（その場書き換え）/ コピー（新しい値を返す）
- `??`（読み: ナル合体）= 左が none / err なら右を使う演算子
- (a)〜(d) = 判定の 4 軸。(a) 他言語との過不足、(b) native と wasm の両方で成り立つか、(c) LLM が書き当てやすいか、(d) 変更の種類と互換

---

## 改名してよい条件（全規則の前提）

**R-L0-00 — ○ 採用**

綴りを変えてよいのは、次のどちらかに当てはまるときだけです。

1. **Almide の中で衝突している。** 同じ役割に綴りが 2 つある、または同じ動詞がモジュールによって違う意味になっている。
2. **9 言語の多数派（5 言語以上）が別の綴りで一致している。** しかも Almide の綴りに、言語として固有の理由がない。

多数派がない役割（例: 長さの `len`/`length`/`size`/`count`、大文字化の `to_upper`/`uppercase`）は、いまの綴りを据え置きます。

- (a) 9 言語のどれとも完全には一致しません。それでも「多数派に寄せる、割れているなら動かさない」と決めれば、改名の数を必要最小限に抑えられます。
- (c) LLM の学習データにある既存の綴りは、変えるたびに新旧が混ざる期間を生みます。必要のない改名は、それ自体が書き間違いの原因になります。
- (d) この規則そのものは変更ではありません。

---

## R-L0-01 同じ役割には綴りを 1 つ（モジュールをまたいでも） — ○ 採用

CHEATSHEET の「No synonyms」を、モジュールの境界を越えて適用します。いまのゲート `tests/stdlib_synonym_gate_test.rs` は 1 モジュールの中しか見ていません。
残すほうは「主語の型のモジュールにあるもの」（R-L0-02）と決めます。

- (a) 他言語にも重複はあります（Python の `math.fabs` と `abs` など）。それでも、標準の中に同じ計算が 2 つ並ぶのは、どの言語の設計指針でも推奨されていません。
- (b) 影響なし。どちらの綴りも同じ実行時の本体を呼んでいます。
- (c) 2 つあると、片方だけ挙動の端が違うときに書き分けを誤ります（`env.temp_dir` と `fs.temp_dir` がその例です）。
- (d) 撤去。互換の扱いは R-L0-14 に従います。
- 「同じ役割」には、ほかの公開関数を 1 回呼ぶだけの関数（転送）も含めます（例: `error.context` は `result.map_err` に接頭辞を付けるだけ）。
- 型の行列のセル（`bytes.append_i16_le` と `append_u16_le` のように、型の軸だけが違うもの）は、実行時の本体が同じでも同義語に数えません。型の軸そのものが意味を持つからです。ただし、その行列の形が宣言されていて、ゲートで守られていることを条件にします（CLAUDE.md「API の族は行列で広げる」）。
- 同じ形の役割には、同じ品詞を使います。n 個ずつに分ける関数は、単数形の動詞 `chunk` に揃えます（`list.chunk`。`bytes.chunks` は改名します）。
- ゲート: 同義語ゲートに規則を足します。異なるモジュールの同じシグネチャで実行時の本体が同じものと、同じモジュールで別の intrinsic が同じ本体を持つものと、ほかの公開関数を 1 回呼ぶだけのものを検出します（#3851 F の案を広げたもの）。許容リストは減らす方向にしか変えられません。
- issue: #3851（既存）

## R-L0-02 関数は主語の型のモジュールに置く — ○ 採用

第 1 引数（主語）の型が属するモジュールに置きます。主語のない構築子（`map.new`、`list.range`）は、返す型のモジュールに置きます。
`math` には、型モジュールに置き場所がない数学関数だけを置きます。境界は「`Int` にも同じ役割があるもの（`abs`・`min`・`max`・`clamp`・`sign`・`pow`）は型のモジュール、実数だけの数学関数（`sqrt`・`cbrt`・`hypot`、三角・指数・対数・ガンマ）と `factorial`・`choose` は `math`、丸め・判定・定数・ビット列・変換は `float`」です（R-math-03 で `sqrt` の置き場所を見直してこの形にしました）。
入れ物の要素の型に意味がある関数（`List[Option[T]]` を受け取る `option.collect`、`List[Result[T, E]]` を受け取る `result.partition`）は、要素の型のモジュールに置きます。

- (a) 多数派は 7/9 で、主語の型のモジュールやメソッドに置いています（Rust・Kotlin・Swift はメソッド、Gleam・Elixir・OCaml は型モジュール）。Go と Python の `math` は例外です。
- (c) `int.abs` と `math.abs` のように置き場所が 2 つあると、どちらに置かれているか LLM が当てられません。
- (d) 移動は撤去と追加の組です。R-L0-14 に従います。
- ゲート: 公開関数の第 1 引数の型、または戻り型が、そのモジュールの主語の型であることを検査します。例外の許容リストは減らす方向にしか変えられません。
- issue: #3949

## R-L0-03 引数の順序は主語を先頭にする — ○ 据え置き＋ゲート

`xs |> list.map(f)` の `|>`（読み: パイプ）は、第 1 引数に差し込みます。いまの 999 関数はすべてこの順を守っています。

- (a) データを先頭に置く言語は 7/9 です（Rust、Go、JS、Gleam、Elixir、Kotlin、Swift）。関数を先頭に置くのは OCaml Stdlib と Python の `map`/`filter` だけです。
- ゲート: R-L0-02 と同じテストで、主語の型の引数が先頭にあることを検査します。
- issue: #3949

## R-L0-04 コピーが既定。その場書き換えは、決まった動詞だけ — ○ 採用

**規則:**
1. 関数は既定でコピーを返します（`list.sort`、`list.set`、`map.set`、`set.insert`）。変数を更新したいときは `xs = list.set(xs, i, v)` と書きます。`@reuse` の付いた関数は、この形でもコピーせずにその場で動きます（map 200 万件で `map.set` と `map.insert` と添字代入はどれも約 0.07 秒、実測済み）。
2. `mut` 引数を取る関数（その場書き換え）は、次の動詞だけに限ります。コピー版と同じ速さで書けない、バッファを伸ばす操作・切り詰める操作・位置を指定して書く操作です。
   - `push`（末尾に 1 つ足す）、`pop`（末尾から 1 つ取る）、`clear`（空にする）、`fill`（全部同じ値にする）
   - 両端を持つ入れ物では、先頭を名前に付けた `push_front`・`pop_front`（`deque`。末尾は `push`・`pop`、R-deque-03）。`push`・`pop` と同じ族として数えます
   - `append_<型>`（バイト列の末尾に書き足す）、`put_<型>`（バイト列の位置を指定して上書きする）
   - `copy_from`、`copy_within`（バイト列の範囲をコピーする）
3. ひとつの動詞は、全モジュールで同じ様式（コピーかその場書き換えか）を表します。
4. `clear` は入れ物の型すべてに置きます（list・map・set・deque・heap・string・bytes）。確保した場所を保ったまま空にするので、コピーで同じ速さに書けないからです（set・deque・heap への追加は二巡目の見直しで決めました。R-deque-02、R-heap-02）。`set`・`insert`・`remove`・`delete`・`update` はコピー側の動詞です。

**この規則から決まること（L2 で個別に判定します）:**
- `map.insert(mut m, k, v)` と `map.delete(mut m, k)` は撤去します。`set.insert` と `list.insert` はコピーなのに、`map.insert` だけがその場書き換えです。`m[k] = v` と `m = map.remove(m, k)` が同じ速さで書けます。
- `bytes.set_<型>(mut b, pos, v)` は `bytes.put_<型>` に改名します。`list.set`・`map.set`・`bytes.set` はコピーです。`set_at` は `b[i] = v` と重複しています。

**理由:**
- (a) 9 言語は 3 つの流儀に分かれます。名前で区別する言語（Kotlin の `sort`/`sorted`、Swift の `-ed`/`form`）、型や戻り値で区別する言語（Rust、Go、OCaml）、そもそも書き換えのない言語（Gleam、Elixir）です。Almide は値の意味論（代入でコピーされる）で、関数型スタイルです。コピーが既定で `sort` もコピーを返す点は、Gleam・Elixir・OCaml List と同じ流儀です。そのうえで、Swift と Kotlin が名前で区別している部分を、「動詞の集合を決めて閉じる」形で取り入れます。
  - `append_<型>` と `put_<型>` は、Go の `encoding/binary` の `AppendUint32`（伸ばす）と `PutUint32`（位置を指定して上書きする）と同じ対です。https://pkg.go.dev/encoding/binary
- (b) 影響なし。その場書き換えは native では `&mut`、wasm では書き戻しで、すでに両方の経路があります。
- (c) いまは `insert` が、map ではその場書き換え、set と list ではコピーです。動詞から様式が読めません。しかも次の R-L0-05 のとおり、取り違えてもエラーになりません。
- (d) 改名と撤去です。R-L0-14 に従います。
- ゲート: `mut` 引数を持つ公開関数の名前が上の動詞集合に入っていること、その動詞が `mut` のない関数に使われていないことを、宣言から検査します（interface JSON の `mut` 欄、#3889）。
- issue: #3928

## R-L0-05 Unit 以外の値を文として捨てたらエラーにする — ○ 採用

**実測（2026-10-21、v0.68.0-rc2 の後の develop）:**

```almide
fn main() -> Unit = {
  var s = set.new()
  set.insert(s, 1)        // エラーにならない。何も起きない
  println(int.to_string(set.len(s)))   // 0 と表示される
}
```

`list.sort(xs)` や `string.trim(s)` を文として書いた場合も、黙って通ります。
いまの E042 が拒否するのは、文で捨てられる `Result` だけです。これを「Unit 以外のすべての値」に広げます。捨てたいときは、いまどおり `let _ = f()` と書きます。

- (a) Swift は、使われない戻り値を既定で警告します（`@discardableResult` で外せます）。Rust は `#[must_use]` を付けた型や関数で警告します。Gleam は、使われない純粋な式を警告します。
- (b) 検査器だけの変更です。両 target で同じです。
- (c) R-L0-04 で動詞を揃えても、LLM は他言語の癖でコピー関数をその場書き換えだと思って書きます。そのときに検査で止まるのが、この規則です。名前の規則と、検査による保険の 2 段構えにします。
- (d) 言語の変更です。dialect epoch 15 の `breaks` に入れます。入れる前に、spec・examples・stdlib・research・tests/diagnostics・下流 44 プロジェクトで、影響する件数を数えます。
- ゲート: 新しい E042 の fixture（broken と fixed）と、診断ハーネスで確かめます。
- issue: #3906

## R-L0-06 無印の名前は安全版。既定値版 `_or` は `??` に一本化する — ○ 採用

1. 失敗しうる取り出しは、無印の名前で `Option` を返します（`list.get`、`map.get`、`list.first`）。理由のある失敗は `Result[_, String]` を返します（`int.parse`、`fs.read_text`）。
2. 既定値を受け取る `_or` 版（`list.get_or`、`map.get_or`、`bytes.get_or`、`args.option_or`）は `xs |> list.get(i) ?? d` と同じなので、撤去します。`option.unwrap_or` と `result.unwrap_or` を `??` に寄せた ADR-0005 の延長です。
3. 処理を止める（abort）のは、呼び出し側の契約違反だけです（例: `list.chunk(xs, 0)`）。
4. 失敗しうる構築・変換は、失敗の目印になる値（空文字、0、空のリスト）を返しません。理由がなければ `Option`、理由があれば `Result[_, String]` を返します（例: `string.from_codepoint` がいま無効な値で `""` を返すのは、この規則に反します）。
5. 数値変換の無印 `to_<型>` は、結果の型で表せない値で abort します（R-L0-32）。3 の「確かめられる前提の違反は abort」の 1 つで、例外ではありません。ほかの扱いは接尾辞で選びます（`_checked`・`_saturating`・`_wrapping`。R-L0-34）。（六巡目に書き直し。以前は「無印は折り返す例外」と書いていました。）

- (a) 無印が安全版なのは Rust、OCaml Base、Elixir です。無印が例外版なのは OCaml Stdlib と Kotlin で、多数派はありません。Almide は例外を持たないので、無印が安全版である以外の選択肢はありません。既定値版は Python の `get(k, d)`、Kotlin の `getOrDefault`、Elixir の `get(m, k, default)` がありますが、Rust・Swift・JS・Gleam は演算子か汎用の関数で書きます。
- (c) `??` は CHEATSHEET が教える唯一の書き方です。`get_or` を覚えていなくても書けます。
- (d) 撤去です。R-L0-14 に従います。`removed_spellings.rs` のヒントに「`list.get(xs, i) ?? d`」と出します。
- issue: #3913

## R-L0-07 変換の向き: 値の変換は元の型の `to_<型>`、入れ物からの構築は先の型の `from_<元>` — ○ 採用

1. スカラー（数・文字）の変換は、元の型のモジュールに `to_<先の型>` で置きます（`int.to_float`、`int8.to_int`）。
2. 入れ物や表現から値を組み立てる関数は、先の型のモジュールに `from_<元>` で置きます（`map.from_list`、`set.from_list`、`bytes.from_list`、`string.from_codepoint`）。
3. 同じ変換が両側にあるときは、1 の側だけを残します。`float.from_int` は `int.to_float` と同じなので撤去します。
4. 向きの対は型も揃えます。`string.to_bytes` が `List[Int]` を返し、`bytes.from_string` が `Bytes` を返すような食い違いは解消します（L2 の string と bytes で判定します）。

- (a) 元の型の側に `to_X` を置く言語は 5/9 です（Rust、Gleam、Elixir、OCaml の新 API、Kotlin）。入れ物の構築に `from_list` を使うのは Gleam と Rust（`from_iter`）、JS（`Array.from`）です。この組み合わせは Gleam の流儀とそのまま同じです。
- (c) `x |> int.to_float` の形で、パイプの先頭の型から関数の場所が決まります。
- (d) `int.from_int8` 系は撤去し、`int8.to_int` 系を追加します。`float.from_int` は撤去します。R-L0-14 に従います。
- ゲート: 公開関数を `to_<T>` と `from_<T>` の組で突き合わせ、同じ変換が両側にないことを検査します。
- issue: #3935

## R-L0-08 数値変換の 4 種は接尾辞の行列 — ○ 据え置き＋穴埋め

- `to_<T>`: 結果の型で表せない値（範囲外、整数への NaN）で abort（R-L0-32）。丸めの位置は R-L0-35。
- `to_<T>_checked`: 正確に表せるときだけ some（R-L0-28）。
- `to_<T>_saturating`: 範囲外なら端の値に張り付く。
- `to_<T>_wrapping`: 整数から整数への変換で、下位のビットを残して折り返す（R-L0-34）。
- 折り返し演算 `int.wrap_add`、`wrap_sub`、`wrap_mul` と `int.wrap(x, bits)`: `Int` の中の計算です。型の変換には使いません（R-L0-34）。
- （六巡目に、R-L0-32・R-L0-34・R-L0-35 に合わせて書き直しました。）

- (a) 4 種を全部別の名前で持つのは Rust（接頭辞）と Swift（引数ラベル）だけで、多数派はありません。いまの接尾辞の行列はすでにゲートで守られているので、据え置きます。
  - 9 言語の外の参考として Zig も見ました。Zig は 1 語の変換を持たず、`@intCast`（範囲外は不正な動作で、安全モードでは止まる）と `@truncate`（折り返し）のように、意味ごとに名前を分けています（[言語リファレンス](https://ziglang.org/documentation/master/#intCast)）。Rust も `as` を禁じる・警告する lint（clippy の `as_conversions`・`cast_possible_truncation`）を持っています。意図を名前に書かせる方向は、Almide の接尾辞の行列と同じです。
- (c) 行列の穴（`wrap_sub` がない）は、LLM が規則から推測して書いたときに確実に外れます。
- 変換の演算子 `as` を足さない判定は R-L0-18、他言語の変換の書き方から案内する診断は R-L0-19 です。
- issue: #3854（既存）

## R-L0-09 Bool を返す関数は `is_`/`has_`、または決まった動詞 — ○ 採用

Bool を返す公開関数の名前は、`is_` か `has_` で始めます。そうでなければ、次の閉じた動詞集合のどれかにします: `contains`、`exists`、`starts_with`、`ends_with`、`any`、`all`。

- (a) `is` 接頭辞は 8/9 です（Elixir だけが末尾に `?` を付けます）。`contains` は 5/9、`starts_with` と `ends_with` も多数派です。
- (c) 名前から Bool が返ることが読めます。
- いまの例外（L2 で判定します）: `args.flag`、`bytes.eof`、`regex.full_match`、`net.unix_poll`。`bytes.read_bool` は Bool を読み出す関数なので、対象外です。
- 族の接頭辞: モジュールの中で関数の族を接頭辞で分けている場合（`net.tcp_*`・`net.udp_*`）、`is_` は族の接頭辞の後ろに置きます（`net.tcp_is_open`）。名前の先頭は族、残りがこの規則に従う、と読みます。
- ゲート: 戻り型が Bool の公開関数の名前を検査します。
- issue: #3895

## R-L0-10 高階関数の接尾辞の意味を 1 つに決める — ○ 採用

| 接尾辞 | 受け取る関数 | 例 |
|---|---|---|
| `_by` | キー関数 `(A) -> K` | `sort_by`、`unique_by`、`group_by` |
| `_with` | 値の代わりに受け取る関数（結合・比較・値を作る関数） | `zip_with`、`sort_with`（比較）、`regex.replace_with`（置換文字列を作る） |
| `_while` | 先頭から続く間の述語 | `take_while`、`drop_while` |
| `_map` | `Option` を返す変換で、none を除く | `filter_map`、`find_map` |
| `flat_map` | リストを返す変換をつなぐ | `list.flat_map`、`option.flat_map` |

- (a) `_by` の意味は言語で逆になっています。Elixir と Kotlin はキー関数、Rust の `sort_by` と Swift・Gleam の `by:` は比較関数です。Almide はすでにキー関数として揃っているので、それを規則として固定します。比較関数の版は Kotlin の `sortWith` に合わせて `_with` にします。
- ゲート: `_by` の公開関数は最後の引数が 1 引数でキーを返す関数であること、`_with` は最後の引数が関数であることを、型から検査します。
- issue: #3949

## R-L0-11 長さは `len` だけ — ○ 採用（既存の撤去を日付に載せる）

`list.length` と `string.length` は `@deprecated` ですが、撤去する epoch がまだ決まっていません。R-L0-14 の窓で撤去します。

- (a) 綴りは割れています（length 4、len 3）。R-L0-00 に従い、いまの `len` を据え置きます。
- issue: #3851 A（既存）

## R-L0-12 範囲は半開区間。はみ出しは丸め、abort は契約違反だけ — ○ 据え置き＋ゲート

`slice(start, end)` は `start` を含み `end` を含みません。範囲外の添字は端に丸めます。`take(n)` も `n` が長さを超えたら全部を返します。負の添字は範囲外として扱い、末尾からの位置には使いません。

- (a) 半開区間は 9 言語すべてが同じです。はみ出しを丸めるのは Python・JS・Gleam・Elixir で、Rust・Swift は panic・trap します。Almide は例外を持たないので、丸めるほうに揃えます。
- ゲート: `slice`、`take`、`drop`、`take_end`、`drop_end` を持つすべての型（list、string、bytes）で、同じ境界入力に対する結果を突き合わせる行列テストを置きます。

**位置を指定した書き込み:** 範囲外の位置への書き込み（`list.set`、`list.update`、`list.swap`、`list.remove_at`、`bytes.put_<型>` など）は、呼び出し側の契約違反として abort します。添字代入 `xs[i] = v` がすでに abort するのと揃えます。`list.insert` は `0..=len` の位置を受け付け、その外は abort します。

- (a) 範囲外の書き込みを、Rust・Go・Kotlin・Swift は panic・trap、Python・OCaml は例外で止めます。黙って何もしないのは、Elixir の `List.replace_at` だけです。
- (c) いまは関数が「何もしない」、添字代入が「abort」と、同じ書き込みで 2 つの挙動があります。何もしない書き込みは、間違えても気づけません。
- (d) 意味変更（定義域を狭める変更）です。正しい位置への書き込みの結果は変わらないので、epoch の `breaks` には入れません（「定義域を狭めても epoch にしない」という既存の扱い）。
- issue: #3944

## R-L0-13 環境を読む関数は effect — ○ 据え置き（穴を塞ぐ）

ADR-0022 のとおり、純粋な関数は書き出しと abort はできますが、環境を読むことはできません。いま `process.env` だけが純粋な `fn` のまま環境を読んでいるので、`env.get` に寄せて塞ぎます（#3851 B）。

## R-L0-14 互換の扱い（全案で共通） — ○ 採用

1. **窓:** 改名と撤去は、0.68.0 の次のリリースで `@deprecated(since=15, use="…")` を付けます。`proofs/dialect-epochs.toml` に `[[deprecation]]` ブロックを置き、`removal_epoch = 16` とします。使うと E052 の警告が出て、置き換え先が示されます。
2. **撤去:** epoch 16 の `breaks` で撤去します。撤去後の呼び出しは、`removed_spellings.rs` のヒント付きのエラーになります。
3. **機械的な書き換え:** 1 対 1 の改名（`bytes.set_u8` → `bytes.put_u8` など）は、`almide fix` が書き換えられるようにします。形が変わるもの（`list.get_or(xs, i, d)` → `list.get(xs, i) ?? d`）も、ヒントと fix の両方で示します。
4. **意味の付け替えは禁止:** 同じ名前のまま、旧い書き方が型の検査を通ったうえで別の結果になる変更はしません。旧い意味は、LLM の学習データに残り続けるからです。意味を変えるときは、新しい名前に移します。旧い書き方が必ず型エラーになる変更（引数の型を `String` から `Regex` に変える、戻り型を `String` から `Option[String]` に変えるなど）は、ヒントと `almide fix` の書き換えを付ければ許します。黙って違う結果にはならないからです。この許しによって、失敗の目印を返す版が無印の名前を持っている場合も、無印の名前のまま安全版に変えます（`io.read_line` は `Option[String]` を返すようにし、`read_line_opt` は撤去します）。`_opt` のような接尾辞で逃がすことはしません。入れ物の型を変えるときも同じです。`fs.read_bytes` と `io.write_bytes` は名前を保ったまま `List[Int]` から `Bytes` に変え、`Bytes` 版として別名で生き残っていた `fs.read_bytes_raw` などを撤去します。
5. **言語の変更**（R-L0-05）は、epoch 15 の `breaks` に入れます。影響した件数を `summary` に書きます。
6. **件数:** それぞれの案に、spec・tests・docs・stdlib での使用数を書きます。

- 前例: epoch 14 は、deprecation の窓を置かずに撤去しました。今回は改名が多いので、窓を置きます。
- issue: #3904

## R-L0-15 標準関数は native と wasm の両方で動く — ○ 採用

標準関数は、native（Rust）と wasm（WASI）の両方で同じように動くことを前提にします。host の機能が要る関数は、次のどれで wasm に乗せるかを案に書きます。
- 標準の WASI インタフェース（`wasi:filesystem`、`wasi:sockets`、`wasi:clocks`、`wasi:random`、`wasi:http`、`wasi:cli`）
- 暫定の組み込み host op（裁定済みの TCP・unix・shm と同じ形。ADR-0023 step 7 で退役）
- 標準から外して package にする

「native 専用のまま」を終点にはしません（ADR-0027 D8）。
- 4 つめの道（二巡目の見直しで追記）: private host capability（ADR-0025・ADR-0029）。`process` の全関数と `testing.assert_aborts` は、組み込みの host が提供する私的な機能で wasm に乗ります。標準の wasm ホストでは読み込み時に拒否され、黙って違う結果にはなりません。退役の予定はないので「暫定の host op」とは別に数えます。各 L2 の (b) は、標準 WASI・暫定の host op・private host capability・標準の外（package）の 4 つのどれかを名指しします。

## R-L0-16 順序のある型には `compare` を置く — ○ 採用

順序のある型（`int`、`float`、sized の数、`bigint`、`bool`、`string`、`bytes`、`duration`、`compute`、`uuid`）には、`<mod>.compare(a, b) -> Int` を置きます。小さければ負、等しければ 0、大きければ正を返します。`list.sort_with` などの比較関数は、この形の関数を受け取ります（R-L0-10）。

- (a) 3 値を返す比較関数は、Rust の `cmp`（`Ordering`）、Go の `cmp.Compare`、Kotlin の `compareTo`、OCaml の `compare`、Gleam の `int.compare`（`Order`）、Elixir の `compare` と、多数派が持っています。
- (c) `list.sort_with(xs, int.compare)` のように、比較関数を自分で書かずに渡せます。
- (d) 追加と改名です（`bytes.cmp` は `bytes.compare` に改名）。浮動小数点数の順序は、`list.sort` と同じ IEEE totalOrder にします。
- ゲート: 順序のある型のモジュールすべてに `compare` があることを、R-L0-G のテストで検査します。
- issue: #3902

## R-L0-17 呼び出しの型は宣言だけで決まる — ○ 採用

effect fn を呼んだときの型（`!` が要るか、`??` をそのまま書けるか）は、関数の宣言（引数・戻り型・`effect`）だけで決まるようにします。本体が intrinsic か Almide で書かれているかで変わってはいけません。

- いまの食い違い（実測）: `env.get(k) ?? d` は書けますが、Almide の本体を持つ `args.option(k) ?? d` はエラーになり、`args.option(k)! ?? d` と書く必要があります。interface JSON はどちらも `effect: true, error: string` で、宣言からは見分けられません。
- 影響する標準の関数: `env.get`、`args.option`、`args.positional_at`、`io.read_line`（[L2-env](L2-env.md) R-env-03）と、利用者が書く `Option` を返す effect fn。
- (c) 同じ形の宣言の関数が、本体の書き方しだいで呼び方を変えるので、LLM は宣言を読んでも正しい呼び方を決められません。
- (d) 言語の変更です。どちらの形に揃えるかは、ADR-0002（失敗と effect は別の軸）と ADR-0008（伝搬は明示だけ）に沿って、issue で設計します。揃えた結果が既存の呼び出しを変える場合は、epoch 15 の `breaks` に入れます。
- ゲート: 標準の effect fn すべてについて、intrinsic 版と Almide 版で `??` と `!` の受け付けが同じになることを、宣言ごとの fixture で確かめます。
- issue: #3900

## R-L0-18 型を変換する演算子 `as` は足さない — × 不採用

`x as Int8` のような、1 つの記号で型を変える演算子を足すかどうかの判定です。他の言語の `as` は、次の 3 つの別の仕組みに分かれます。

| 仕組み | 例 | Almide での受け持ち |
|---|---|---|
| 数の変換 | Rust `x as i8`（折り返し。float→int は張り付き）、Go `int8(x)`、Swift `Int8(x)`（止まる） | `to_<型>`・`_checked`・`_saturating`（R-L0-08） |
| 実行時の型の確認（ダウンキャスト） | Kotlin `as?`、Swift `as?`/`as!`、Go `x.(T)`、Rust `downcast` | 不要。継承や `Any` が無く、中身の分からない値（`Value` など）は `match` で場合分けする |
| 型検査を黙らせる | TypeScript `x as Foo`、Python `typing.cast` | 入れない。実行時に何も確かめない |
| 式の途中で型を決める | Swift `[] as [Int]`、`1 as Double` | 型注釈 `[]: List[Int]`（実測: E018 のヒントもこの形を教える） |

- (a) 数の変換を `as` 1 つで書くのは Rust だけです。Swift の `as` は数の変換に使えません（`x as Int8` はコンパイルエラーで、変換は初期化子 `Int8(x)` で書きます。[Type Casting](https://docs.swift.org/swift-book/documentation/the-swift-programming-language/typecasting/)）。Rust 自身も `as` を lint で禁じられるようにしています（[参照](https://doc.rust-lang.org/reference/expressions/operator-expr.html#type-cast-expressions)）。Zig は `as` に当たる 1 語の変換を持たず、意味ごとに名前を分けています。
- (b) 該当なし（足さない）。
- (c) 1 つの記号で「折り返す」「張り付く」「止まる」が型の組で切り替わると、どれが起きるかがコードから読めません。型検査を黙らせる `as` は、LLM が型エラーを消すために書く、よくある失敗の形です。
- (d) 足さないので互換の影響はありません。`as` は import の別名（`import x as y`）と JSON のフィールド名の付け替えにだけ使い続けます。
- 理由のまとめ: 数の変換は R-L0-08 の名前つき関数があり、R-L0-01（1 つの役割に綴りは 1 つ）に反します。ほかの 3 つの用途は、Almide の型の作りでは要らないか、目的に反します。他言語の癖で書かれた `as` は、R-L0-19 の診断で受け止めます。

## R-L0-19 他言語の変換の書き方から、変換の関数へ案内する — ○ 採用

- 内容:
  1. `x as Int8`・`Int8(x)`・`x.into()`／`x.try_into()` を見つけたら、元の型と先の型に合わせて、使う関数とはみ出したときの動きを 1 つの診断で出します（例: `int.to_int8(x)` は折り返す、`int.to_int8_checked(x)` は `Option`、`int.to_int8_saturating(x)` は張り付く）。
  2. 未定義の名前の「もしかして」表（`ALIASES`）を、L3 の関数表の「他言語の主な綴り」の列から生成します。全モジュールが対象です（list と string は #3905 で手で直す分）。
- 実測（develop `9352c73e3`）: `x as Int8` は「変数 `as` がない」とヒント「もしかして `x`?」に続けて、`Int8` についても別のエラーになります。`Int8(x)` は「型 `Int8` が呼べない」、`x.into()` は「メソッド `into` がない」で、どれも正しい関数を示しません。
- (a) 他言語の書き方は R-L0-18 の表のとおりです。
- (b) 検査器だけの変更です。
- (c) R-L0-18 で `as` を足さないので、Rust・TypeScript・Kotlin・Swift の癖で書かれた `as` は残ります。最初のエラーで正しい関数に着けるかどうかが、修正の生存率を決めます。
- (d) 診断の追加だけです。互換の影響はありません。
- 完全性の規則: L3 の「他言語の主な綴り」の各綴りは、生きている関数か正しい式を指す `ALIASES` のキーになっているか、外す理由（Almide で別の意味の生きている名前である、など）が書いてあるかのどちらかです。変換の行列のすべての組に、`as`・`T(x)`・`into` の案内があります。
- ゲート案: #3905 で提案した `tests/stdlib_alias_table_test.rs` で、生成した行と L3 の列を突き合わせます。`tests/diagnostics/` に 4 つの形の例を置き、診断が 1 つだけ出て関数名を示すことを確かめます。
- issue: #3958
- 二巡目の見直しで足すこと（issue: #3967）:
  3. 関数を置かずに構文や式で書く役割にも案内の行を置きます。L3 の表とは別に「役割 → 正しい式」の表を 1 枚持ちます。例: `list.for_each`・`each`・`iter` → `for x in xs { … }`、`result.collect` → `rs |> list.map((r) => r!)`、`option.unwrap` → `o ?? panic("…")`、`result.ok` → `result.to_option`、`string.char_at` → `string.get`、`list.concat` → `xs + ys`、`bigint.add` → `a + b`、`float32.is_nan` → `float32.to_float(x) |> float.is_nan`。
  4. 行列が意図して置かないセルの名前（`bytes.get_u8` → `bytes.get`、`bytes.append_u8` → `bytes.push`、`bytes.get_u64_le` → `get_i64_le`、`json.encode` → `json.stringify`、`value.to_string` → `json.stringify`、`random.next_bytes` → `next_int` の繰り返し）は、必ず案内の行を持ちます。
  5. 編集距離で選んだ候補が、意味の違う生きた関数を指すときは出しません。実測で誤っていた案内: `list.find_last` → `find_map`、`list.last_index_of` → `index_of`、`list.iter` → `filter`、`bytes.get_u8` → `set_u8`（書き込みの関数）、`value.to_string` → `as_string`、`print` → `println`（改行が付いて出力が変わる）。
  6. `String` と `Bytes` の取り違え（E005）には、`bytes.from_string(s)`・`bytes.to_string(b)` を示します。`fs.read_bytes`・`hash.fnv1a32`・`io.write_bytes` が `Bytes` に変わるので、この取り違えは増えます。
  7. `5 & 3` のような他言語のビット演算子は、いまは構文エラー「`)` がない」になります。案内（#3905 で予定）は、字句ではなく構文エラーの経路に置きます。

## R-L0-20 critical プロファイルで、黙って折り返す `to_<型>` を拒む — × 取り消し（→ R-L0-32 で解消）

- 内容: `almide check --profile critical` のときだけ、損失のある組の `to_<型>` をエラーにし、`_checked` か `_saturating` を書かせます。Rust の clippy の `cast_possible_truncation` と同じ考え方です。普段のコードは変わりません。
- 保留の理由: いまの critical プロファイル（ALS の bounded 規則）が保証するのは、安全さと資源の上限で、計算の正しさは含みません。`xs[i]` の範囲外や 0 での割り算も、critical で拒んでいません（2026-10-01 に実測）。正しさの規則（全域性）を critical に入れる件は、[certification-grade](../active/certification-grade.md) の CG-3 として先送りになっています。黙って折り返す変換だけを先に拒むと、critical の規則の性格がばらばらになるので、全域性の規則と同時に判断します。

- 取り消し（六巡目）: R-L0-32 で無印の狭める変換が abort になり、黙って折り返す無印がなくなりました。critical で拒む対象が残らないので、この案は閉じます。折り返しは `_wrapping` と名前に書いたときだけ起きます（R-L0-34）。
- 決めるときの順番: ALS（almide/als）に規則を足す → pin を進める → 検査器（`check/bounded.rs`）を実装する。
- issue: なし（CG-3 を再開するときに一緒に起票します）

## R-L0-21 無印の `to_<型>` を、はみ出したら失敗する版に変える — 保留 → R-L0-32・R-L0-34 で解消

- 内容: 損失のある組の `to_<型>` を `Option` を返す形（いまの `_checked` と同じ）に変え、折り返す版を `to_<型>_wrapping` にします。Zig と Swift の形です。R-L0-06 の 5 の例外がなくなります。

- 解消（六巡目）: 無印を「はみ出したら失敗」にする点は R-L0-32 で採用しました（none ではなく abort）。折り返す版を `_wrapping` にする点は R-L0-34 で採用しました。下の本文は保留していたときの記録です。
- (a) 1 語の損失ありの変換を持つ言語のうち、折り返すのは Rust の `as`・Go・Kotlin・OCaml の 4 つ、止まるのは Swift の 1 つです。R-L0-00 の改名の条件（5/9 以上）に届きません。
- (b) 純粋な計算です。両 target で同じです。
- (c) `int.to_int8(300)` が黙って `44` になるバグは、型検査も実行も通ります。この形にすれば起きません。一方で、変換を書くたびに `??` か `!` が要るようになります。
- (d) 意味変更です。戻り型が変わるので古いコードは型エラーになり、R-L0-14.4 の条件は満たします。`almide fix` で `to_<型>_wrapping` に書き換えられます。損失のある組すべてに及ぶ大きい変更です。
- 保留の理由: 多数派の裏付けがないので、LLM が実際に黙って折り返すバグをどれだけ書くかを測ってから決めます。測定は almide-dojo で行います。
- issue: [almide-dojo#63](https://github.com/almide/almide-dojo/issues/63)（測定）

## R-L0-22 組み込みの `Ord` プロトコルのメソッド名を `compare` にする — ○ 採用

- いま: 組み込みの `Ord` のメソッドは `fn cmp(a: Self, b: Self) -> Int` です（[type-system.md](../../specs/type-system.md) §10）。一方で R-L0-16 は、順序のある型のモジュールに `<mod>.compare(a, b) -> Int` を置きます。`Duration` は `Ord` を導出しているので、同じ比較に `d.cmp(e)` と `duration.compare(d, e)` の 2 つの綴りができます。
- 案: `Ord` のメソッド名を `compare` にします。利用者が書く `fn T.cmp(a: T, b: T) -> Int` は `fn T.compare(…)` になります。こうすると、組み込みの型の `Ord` のメソッドと、モジュールの `compare` 関数が同じ名前になり、綴りが 1 つになります。
- 根拠の L0 規則: R-L0-01（同じ役割に綴りは 1 つ）、R-L0-16、R-L0-00（内部の衝突は改名の理由になる）。
- (a) 3 値の比較を `compare` と呼ぶのは Go（`cmp.Compare`）、Gleam、Elixir、OCaml、Kotlin（`compareTo`）です。`cmp` は Rust だけです。
- (b) 検査器と、メソッドを名前で探すところだけの変更です。
- (c) `list.sort_with(xs, duration.compare)` と `a.compare(b)` が同じ単語になり、どちらから覚えても書けます。
- (d) 改名です。epoch 15 で `cmp` を `@deprecated(since=15, use="compare")` にし、epoch 16 で撤去します。`almide fix` が `fn T.cmp` の宣言と `.cmp(` の呼び出しを書き換えます。
- 実測（develop `437b8f127`）: `duration.ms(5).cmp(duration.s(1))` は `almide check` を通りますが、ビルドで内部エラー（`[COMPILER BUG] internal type resolution failed`）になります。これは綴りと関係なく直すべき不具合なので、別の issue #3960 にしました。
- 完全性の規則: `Ord` を持つ標準の型は、すべて `<mod>.compare` を持ち、それが `Ord` のメソッドそのものです（転送の関数を別に置きません）。
- ゲート案: R-L0-G のテストで、`Ord` を持つ標準の型ごとに `compare` が解決でき、check とビルドの両方を通ることを確かめます。
- issue: #3961

## R-L0-23 状態を進める関数は `next_` で始め、`(値, 次の状態)` を返す — ○ 採用

- いま: 状態を渡して、値と次の状態を受け取る関数が 2 つの族にあり、名前と戻り値の順序がそろっていません。
  - `random.next_int(rng, min, max) -> (Int, Rng)`: 名前は `next_`、値が先です。
  - `bytes.read_i32_le_at(b, pos) -> (Int, Option[Int])`、`bytes.take_at(b, pos, n) -> (Int, Option[Bytes])`: 名前は `read_…_at`、次の位置が先です。さらに、位置を指定して読むだけの `bytes.get_i32_le(b, pos)` もあります。
- 案: 状態を進める関数は `next_<何>` と名付け、`(値, 次の状態)` の順で返します。bytes の読み位置を進める族は、`bytes.next_<型>(b, pos) -> (Option[T], Int)` と `bytes.next_bytes(b, pos, n) -> (Option[Bytes], Int)` に改名します（細目は [L2-bytes](L2-bytes.md) R-bytes-13）。
- 根拠の L0 規則: R-L0-01（同じ形の役割に同じ綴り）。R-L0-03 の「主語を先頭に」は引数の規則で、戻り値の順序を決めていなかったので、ここで決めます。
- (a) 状態を明示して渡す乱数の API は、Haskell の `random :: g -> (a, g)`、Elixir の `:rand.uniform_s` が `{値, 状態}`、パーサーの慣習も `(値, 残り)` で、値が先です。Rust でよく使われる bytes ライブラリでは `get_i32_le` が「読み位置を進める」側の名前なので、Almide の `get_<型>`（位置を指定して読むだけ）と逆になります。`next_` にすれば、この取り違えも起きません。
- (b) 純粋な計算です。
- (c) 「`next_` は状態を進める、`get_` は位置を指定して読むだけ」が全モジュールで同じになります。`let (v, pos) = …` の分解の順序も 1 つになります。
- (d) 改名と意味変更です。戻りのタプルの型が変わるので、古いコードは型エラーで止まります（R-L0-14.4 を満たします）。epoch 15 で旧名を `@deprecated` にし、epoch 16 で撤去します。`almide fix` は名前を書き換え、タプルの分解の順序は書き換えられないのでヒントを出します。
- 完全性の規則: 戻り型が「状態の型を含むタプル」の公開関数は、名前が `next_` で始まり、タプルの最後が状態です。
- ゲート案: R-L0-G のテストに、この形の検査を足します（状態の型は `Rng` と、読み位置の `Int` を返す bytes の族）。
- issue: #3962

## R-L0-24 標準の面の型は正規の綴りで書く（`T?`） — ○ 採用

- いま: ADR-0010 は `T?` を `Option[T]` の正規の綴りと決めていて、`almide fmt` も `T?` に直します。ところが、標準の関数の宣言は `Option[T]` が 199、`T?` が 23 で、混ざっています。LLM が読む docs/stdlib と interface JSON も、この宣言をそのまま見せています。
- 案: 標準の `.almd` の宣言を `T?` にそろえ、docs/stdlib・interface JSON・この記録の [target-surface.md](target-surface.md) の表示も正規の綴りにします。
- 根拠の L0 規則: R-L0-01（同じ型に綴りは 1 つ）、ADR-0010。
- (b)・(d) 綴りだけの変更で、型も挙動も変わりません。互換の影響はありません。
- (c) 文書で 2 つの綴りを見た LLM は、どちらが正しいか迷います。正規の綴りだけを見せれば、`almide fmt` の結果とも一致します。
- 完全性の規則: 標準の公開関数の宣言に `Option[` が現れません。
- ゲート案: `stdlib/*.almd` の公開関数の宣言を対象に、`almide fmt --check --no-import-edit` と同じ正規化をかけ、差分が出ないことを確かめます。
- issue: #3963


## R-L0-25 文字列との変換の動詞を表で決める — ○ 採用

| 種類 | 文字列へ | 文字列から |
|---|---|---|
| 型つきの値（`Url`・`Uuid`・`Int`・`BigInt`・`Regex` など） | `to_string` | `parse`（Result） |
| 同じ値の別の表現（`Bytes` など） | `to_<表現>` | `from_<表現>` |
| 中身を保ったまま文字の並びだけを変える符号化（base64・hex・URL の component・form） | `encode_<変種>`（変種がなければ `encode`） | `decode_<変種>`（Result） |
| 例外 | `json.stringify`（学習データで圧倒的な綴り。R-json-03）、`url.build_query`（`parse_query` の逆。多数派なし） | — |

- 根拠の L0 規則: R-L0-07、R-L0-01、R-L0-00。この表は、二巡目の見直しで「綴りはすでにそろっているが、規則として書かれていない」ことが分かったので足しました（`path.from_string` は R-path-02 で消えます）。`build_query` の改名は、多数派がないので × です。
- (c) 新しいモジュールを足すときに綴りが揺れません（uuid は `parse`・`to_string`・`from_bytes`・`to_bytes` で、すでにこの表どおりです）。
- ゲート案: R-L0-G に「`String` から `Result[T, _]` を返す公開関数の名前は `parse`・`parse_<x>`・`decode_<x>`・`from_<x>` のどれか」を足します。許容リストは上の 2 つの例外から始め、減らす方向だけにします。
- issue: #3971

## R-L0-26 演算子を持つ型は、演算子と脱糖先の表を 1 か所で宣言する — ○ 採用

- いま: どの型でどの演算子が使えるかは、検査器と lowering に別々に書かれています。食い違いの実例: `-m`（Matrix の単項マイナス）は `almide check` を通るのに、lowering で内部エラー（「NegInt expects Int operand」）になります（#3965）。また、bigint と matrix は演算子の脱糖先（`bigint.add`・`matrix.add` など）を公開の関数としても持ち、同じ計算に 2 つの綴りがあります。
- 案:
  1. 型 × 演算子（`+ - * / %`、単項 `-`、比較）の表を 1 か所で宣言し、検査器と lowering の両方がそれを読みます。表にないセルは、内部エラーではなく診断つきの型エラーにします。
  2. 演算子の脱糖先は公開しません（`__add` などの内部の名前）。公開の書き方は演算子だけです。例外は、ほかの型にも同じ名前があって演算子と意味が違う関数（`floor_div`・`modulo`）だけです（R-bigint-04、R-matrix-14）。
- 根拠の L0 規則: R-L0-01、R-L0-15、R-L0-17（呼び方は宣言から決まる）。
- (c) `+` が使えるのを見た LLM は `-m` とも書きます。そこで利用者に直せない内部エラーになるのを防げます。
- 完全性の規則: 演算子を持つ型ごとに、表の各セルが「脱糖先がある」か「診断つきの型エラー」のどちらかです。
- ゲート案: 表のすべてのセルについて、`almide check` とビルドの結果を固定する fixture を置きます。
- issue: #3973

---

## R-L0-27 `next_` が none を返すときは、状態を進めない — ○ 採用

- いま: R-L0-23 は `next_<何>` が `(値, 次の状態)` を返すと決めましたが、値が none のときに次の状態がどうなるかは書いていません。`bytes.next_u32_le(b, pos)` で足りないとき、`random.next_choice(rng, [])` のときの 2 つめの値が決まっていません。
- 案: 値が none のときは、受け取った状態をそのまま返します（読み位置は動かさない、乱数の状態は進めない）。値を読まない位置の移動（`bytes.skip(b, pos, n) -> Int`）は値がないので、R-L0-23 の組ではなく次の状態だけを返します。
- 根拠の L0 規則: R-L0-23、R-L0-06（失敗しうる取り出しは none を返し、ほかの値を変えない）。
- (a) Rust の `Iterator::next` は、None を返しても次の呼び出しの意味を変えません。パーサーの慣習（失敗したら入力を消費しない）も同じです。
- (b) 純粋な計算です。
- (c) 足りなかったら別の型で読み直す、という書き方が、位置を戻す手間なしに書けます。
- (d) 新しい関数の約束を書くだけで、既存の関数は変わりません。
- ゲート案: `next_` の族の行列テストに「none のとき 2 つめの値が引数と等しい」を足します。
- issue: #3979

## R-L0-28 `_checked` の変換は「正確に表せるときだけ some」 — ○ 採用（記録の訂正）

- いま: R-L0-08 の説明は「`to_<T>_checked`: 範囲外なら `Option` で none」です。実装はもっと厳しく、正確に表せないときに none を返します（実測 `almide 0.68.0 dev 437b8f127`: `float.to_int32_checked(1.5)` は none、`int.to_float32_checked(16777217)` は none、`int.to_float32_checked(16777216)` は `some(16777216)`）。
- 案: 規則の文を実装に合わせます。「`to_<T>_checked`: 結果が元の値と正確に等しいときだけ some。範囲外、小数部の切り捨て、精度の損失はどれも none」。Swift の `init?(exactly:)` と同じ意味です。
- (d) 文書の訂正だけです。
- issue: なし

## R-L0-29 負の開始位置を丸めずに abort にする — ○ 採用（保留から変更）

- いま: R-L0-12 は「範囲は半開区間。はみ出しは丸め、abort は契約違反だけ」です。`list.slice(xs, -3, len)` は開始を 0 に丸め、止まらずに別の値を返します。Python・JS の「負の添字は末尾から数える」の慣習で書いたコードが、黙って違う結果になります。
- 案: 開始・終わりの位置が負のとき（`slice`・`take`・`drop` の数が負のときも）は abort にします。末尾から数えたいときは `take_end`・`drop_end` を使います。正の側のはみ出し（`len` より大きい）は今までどおり丸めます。
- 保留の理由: R-L0-12 はゲートつきで決めた規則で、正の側の丸めと対になっています。負の添字の書き誤りがどれだけ起きるかを Dojo で測ってから決めます（R-L0-21 と同じ扱い）。
- (a) Rust と Go は panic、Python と JS は末尾から数える。黙って 0 に丸める言語は比べた中にありません。
- 判定の変更（五巡目）: 3 つの外部レビューが続けて最も重い指摘に挙げ、比べた言語に「黙って 0 に丸める」ものが 1 つもないため、計測を待たずに採用します。R-L0-32（無印の狭める変換）と R-L0-33（範囲外の扱いを役割で分ける）と同じ「呼び出し側が確かめられる前提の違反は abort」（R-L0-06 の 3）にそろえます。
- (d) 意味変更。いまは丸めて返している呼び出しが abort になるので、epoch 15 の `breaks` に入れます。Dojo の計測（almide-dojo#63）は、採用後の書き誤りの数を見る項目として残します。
- issue: #3990

## R-L0-30 引数名は役割ごとに 1 つ — ○ 採用

- いま: 関数名は L0 で揃っていますが、引数名は揃っていません。同じ役割に 2 つ以上の綴りがあります。
  - `Bytes` の主語: `b` が 149 か所、`data` が 12 か所（`zlib.*`・`hash.fnv1a32`・`matrix.from_bytes_*`）。外に書き出すバイト列（`fs.write_bytes`・`net.tcp_write` など）は `data` で、こちらは揃っています。
  - 数の主語: `int`・`float` は `n`、サイズつきの型は `x`。
  - 要素: `x`（`list.push`）、`val`（`list.insert`・`bytes.*`）、`value`（`set.insert`）。
  - 添字: `bytes.get` は `i`、`bytes.insert` は `pos`。
  - 関数の引数: ほとんど `f` で、`result.filter` は `pred`、`heap.new_by` は `key`。
  - パターン: `regex.new` は `pat`、`fs.glob`・`http.route` は `pattern`。
- 引数名は表に出る約束です。Almide には名前つき引数があり（`list.len(xs: [1, 2])` が通ります。実測 `almide 0.68.0 dev`）、シグネチャは `llms.txt` と cheatsheet に載ります。
- 案: 役割ごとに名前を 1 つに決めます。

| 役割 | 名前 |
|---|---|
| 主語 `List`・`Bytes`・`Map`・`Set`・`Matrix`・`Value`・`Result`・`Option` | `xs`・`b`・`m`・`s`・`m`・`v`・`r`・`o` |
| 外に書き出すバイト列（`io`・`fs`・`net`・`unix`・`http`・`process`） | `data` |
| `int`・`float`・`bigint`・サイズつきの数のモジュールの主語 | `x`（2 つを取る演算は `a`・`b`） |
| `path` の主語 | `path`（`fs` と同じ） |
| 要素の値（list・set・deque・heap・bytes） | `x`（`map` だけは組の `key`・`value`。入れ物の要素でない値、たとえば `env.set(name, value)` は役割の名前のまま） |
| 要素の添字 | `i` |
| 型つきの読み書き（`bytes.get_*`・`put_*`・`next_*`）のバイト位置 | `pos` |
| 関数の引数 | `f`。比較関数は `cmp` |
| 名前のついた関数型の引数（`HttpHandler`・`HttpMiddleware`） | 型の役割の名前（`handler`・`middleware`） |
| 同じ型の 2 つの値を対等に取る関数（`compare`・`min`・`max`・`merge`・`union`・`xor`・`zip`） | `a`・`b`（主語の名前の規則より先に当てます） |
| パターンの文字列（`regex`・`fs.glob`・`http.route`） | `pattern` |
| 個数・量（`duration.ms(n)`・`random.bytes(n)`・`list.take(xs, n)`） | `n` |

- (c) 名前つき引数で古い名前を書くと、E004「no parameter named …」になり、ヒントに正しい名前が出ます。黙って違う値にはなりません。
- (d) 改名。`spec/`・`stdlib/`・`examples/`・exercise の中で、標準関数を名前つき引数で呼んでいる箇所は 0 件でした。epoch 15 の `breaks` に入れ、`almide fix` で書き換えます。
- 完全性の規則: 公開関数のすべての引数を役割で分け、1 つの役割に 2 つ目の綴りがあればゲートが落ちます。1 つのシグネチャに同じ引数名が 2 つあっても落ちます（四巡目の書き換えで `bytes.compare(b, b)` を作ってしまい、レビューで見つかりました）。
- 一覧（[target-surface.md](target-surface.md)）のシグネチャは、この表の名前で書いています。
- issue: #3987

## R-L0-31 同じ形を持つ型の組はプロトコルで宣言する — ○ 採用

- いま: `Duration` と `Compute` が同じ形であることは、R-duration-03 で `TimeAmount` というプロトコルに宣言しました。ほかにも、型ごとに同じ関数を並べ、その揃いを手書きの行列ゲートや文章で守っている組があります。
- 案: 「同じ役割の関数を、同じ名前・同じ形で持つ型の組」は、標準のプロトコルで宣言します。プロトコルの検査が、片方にだけ関数が足りない、というずれを型の検査で止めます。総称関数を 1 つ書けば、組のどの型にも使えます。
- 調べた組と判定:

| 組 | プロトコル | 判定 | 中身 |
|---|---|---|---|
| `Duration`・`Compute` | `TimeAmount` | ○（R-duration-03） | 6 単位の構築子と `compare` |
| `HttpRequest`・`HttpResponse` | `HttpMessage` | ○（R-http-12） | `header`・`header_values`・`headers`・`body`・`body_bytes`・`set_header`・`set_body`・`set_body_bytes`。`req_` の付いた 6 関数と `get_header` が 1 つの総称関数になります |
| `int` とサイズつきの整数 8 型 | `Integer` | ○（R-sized-08） | `compare`・`min`・`max`・`clamp`・`parse`・`parse_radix`・`to_string`・`to_string_radix`・`min_value`・`max_value` |
| `float`・`float32`・`float64` | `FloatingPoint` | ○（R-sized-08） | `Integer` と同じ選択・比較の族に、値の分類（`is_nan`・`is_infinite`・`is_finite`）と特別な値（`nan`・`infinity`・`epsilon`） |
| list・string・bytes・map・set・deque・heap | `Sized` | ○（この案） | `len`・`is_empty`。7 つの入れ物はすでに全部持っています |
| list・string・bytes・map・set・deque・heap の `contains`・`count` | — | × | `map.contains` はキー、`map.count` は `(K, V)` を受け取り、要素の型が 1 つに決まりません |
| `parse` を持つ型（`int`・`bool`・`uuid`・`url` など） | `Parse` | × | 総称関数の中で `T.parse(s)` と書けません（実測 `almide 0.68.0 dev`: 引数のない関数をプロトコルに書くことはできますが、`T.parse` は E002）。言語に「型で選ぶ関数呼び出し」が入ったら出し直します |
| `option`・`result` の `map`・`flat_map` | — | × | `Option[_]` という「型を取る型」を抽象する仕組み（高階の型）が言語にありません |
| `base64`・`hex` などの符号化 | — | × | 型ではなくモジュールの組で、プロトコルを持つ値がありません。綴りは R-L0-25 の表で揃えています |

- プロトコルが揃いを保証するのは、表の「中身」の列に挙げた関数だけです。組の一方にだけある関数（`float.floor`・`round`・`signum` など。`float32`・`float64` では `to_float` してから `float.*`）は、プロトコルの外です。
- 演算子（`+`・`<` など）はプロトコルに入りません（R-duration-03 と同じ。R-L0-26 の演算子の表で宣言します）。
- 引数のない関数（`min_value`・`nan`・6 単位の構築子）も、プロトコルに書いて「持っていること」を検査します。総称関数の中から呼ぶことは、いまの言語ではできません。
- 根拠の L0 規則: R-L0-01（同じ役割を 2 か所で手書きしない）、R-L0-12（族は行列で揃える）。CLAUDE.md の「性質は宣言 1 か所から導出する」。
- (a) Swift `BinaryInteger`・`BinaryFloatingPoint`・`Collection`、Rust の `num-traits` の `PrimInt`・`Float`、Go の `constraints.Integer`・`constraints.Float`。
- (b) 検査器の変更です。値の表現は変わりません。
- (c) `Float` を `Float32` に置き換えたコードが同じ関数名で書ける、ということが、型の宣言として保証されます。
- (d) 宣言の追加です。足りない関数を埋める分（R-sized-08）は追加です。
- ゲート案: 宣言した型ごとに、プロトコルの総称関数のテストを `spec/stdlib/` に置きます。R-L0-12 の手書きの行列ゲートのうち、プロトコルが覆うものは置き換えます。
- issue: #3989

## R-L0-32 無印の狭める変換は、範囲外と NaN で abort にする（入力の型によらない） — ○ 採用

- いま: R-L0-08 の無印の `to_<T>` は、入力の型によって範囲外の扱いが違います。整数から整数は折り返し、浮動小数点から整数は端に張り付き、NaN は 0 です（実測 `almide 0.68.0 dev 437b8f127`、native と wasm で同じ: `int.to_int8(300)` = 44、`float.to_int8(300.0)` = 127、`int.to_uint8(-1)` = 255、`float.to_uint8(-1.0)` = 0）。入力の型を書き換えただけのコードが、止まらずに別の値を返します。
- 案: 無印の狭める変換は、結果の型で表せないとき（範囲外、NaN）に abort します。浮動小数点からの小数部の切り捨て（`float.to_int(1.5)` = 1）は今のままです。ほかの扱いは、名前で選びます。
  - `_checked`: 正確に表せなければ none（R-L0-28）。
  - `_saturating`: 端に張り付く。浮動小数点からの変換にも、このセルを足します（`float`・`float32`・`float64` → 整数 9 型の 27 関数）。いまの無印の動きは、このセルに移ります。
  - 折り返しは `_wrapping` のセルで書きます（R-L0-34。六巡目に `int.wrap` への誘導から変更）。
- 根拠の L0 規則: R-L0-06 の 3（呼び出し側が確かめられる前提の違反は abort）。R-L0-08 の行列（損失のある変換は無印・`_checked`・`_saturating` の 3 つ）。
- (a) Swift の `Int8(x)` は範囲外で trap し、`init(exactly:)`・`init(clamping:)`・`init(truncatingIfNeeded:)` で扱いを選びます。Rust の `as` は折り返しと飽和が型で分かれ、レビューが指摘した食い違いそのものです。
- (b) 純粋な計算です。両 target で同じ判定です。
- (c) 書き誤りが、黙った別の値ではなく、その場の abort になります。
- (d) 意味変更。いまは折り返して返している呼び出しが abort になるので、epoch 15 の `breaks` に入れます。`almide fix` は、浮動小数点からの無印を `_saturating` に書き換えます（動きを保つ）。整数からの無印は `_wrapping` に書き換えます（動きを保つ。R-L0-34）。
- 完全性の規則（R-L0-08 を置き換え）: 損失のある変換は、どの入力の型でも、無印（abort）・`_checked`・`_saturating` の 3 つを持ちます。整数から整数への損失のある変換は、`_wrapping` を加えた 4 つです（R-L0-34）。変換先が浮動小数点のときは R-L0-35 です。
- issue: #3990

## R-L0-33 範囲外の扱いを関数の役割で分ける — ○ 採用

- いま: R-L0-12 は「範囲は半開区間。はみ出しは丸め、abort は契約違反だけ」と 1 文で書いています。一方で、位置を指定して 1 つを読む関数は none、1 つを書く関数は abort と、個別のページに書いてあります。どれを当てるかを、関数ごとに推して読む必要がありました。
- 案: 役割ごとの表を 1 枚にします。

| 役割 | 例 | 正の側のはみ出し | 負の位置・負の数 |
|---|---|---|---|
| 範囲を取り出す | `slice`・`take`・`drop`・`take_end`・`drop_end`・`byte_slice`・`bytes.slice` | 長さまで丸める | abort（R-L0-29） |
| 位置の 1 つを読む | `get`・`first`・`last`・`bytes.get_*`・`next_*` | none（`next_` は状態を進めない。R-L0-27） | none |
| 位置の 1 つを書く・差し込む・消す | `set`・`update`・`swap`・`insert`・`remove_at`・`bytes.put_*` | abort | abort |
| キーで既存の要素を書き換える | `map.update` | キーが無ければ abort（R-map-08） | — |
| 形を前提にする | `matrix` の演算、`bytes.xor`、`chunk(n <= 0)` | abort | abort |
| 数を作る | `list.repeat(x, n)`・`string.repeat` | — | abort |

- 根拠の L0 規則: R-L0-06（失敗しうる取り出しは none、確かめられる前提の違反は abort）、R-L0-12。
- (a) Rust の `get` は None、`v[i]` と `split_at` は panic、`iter().take(n)` は丸め。Kotlin の `getOrNull` は null、`take(-1)` は例外。
- (d) 表の多くは今の動きの書き直しです。変わるのは R-L0-29（負の位置）、R-map-08（`map.update`）と、負の数を丸めていた `repeat` です。
- issue: #3990

## R-L0-34 整数から整数への損失のある変換に `_wrapping` を置く — ○ 採用

- いま: R-L0-32 は、折り返しを `int.to_int8(int.wrap(x, 8))` で書くとしていました。`int.wrap(x: Int, bits: Int) -> Int` は `Int` の中の計算なので、`Int` に入らない `UInt64` の上半分（2^63 以上）を受け取れず、折り返した結果の `UInt64` も返せません。`uint64.to_int(uint64.max_value())` は R-L0-32 で abort になるので、`UInt64` を経由する折り返しの道がありませんでした。
- 案: 整数（`Int` と固定幅の整数 8 型）から整数への損失のある変換に、`to_<T>_wrapping` を置きます（46 関数）。下位のビットを残し、結果の型の範囲に折り返します（2 の補数）。`int.wrap`・`wrap_add`・`wrap_sub`・`wrap_mul` は `Int` の中の計算として残し、型の変換の案内には使いません。
  - 例: `int.to_uint8_wrapping(300)` = 44、`int.to_uint64_wrapping(-1)` = `uint64.max_value()`、`uint64.to_int_wrapping(uint64.max_value())` = -1。
  - `UInt64` の計算は `uint64` 同士の演算子で書きます（R-L0-26）。`Int` に移すときは、`to_int`（範囲外は abort）・`to_int_checked`・`to_int_saturating`・`to_int_wrapping` から選びます。
  - `BigInt` からの変換には置きません（任意の長さの整数の下位ビットを取る用途は、`bigint` の剰余で書けます）。
- 根拠の L0 規則: R-L0-08（変換の扱いは接尾辞の行列で選ぶ）、R-L0-32 の完全性の規則、CLAUDE.md の「族は行列で揃える」。
- (a) 折り返しを名前で選ぶのは Swift `init(truncatingIfNeeded:)` (F)。Rust の `as`、Go の `T(x)`、Kotlin の `toByte()` は、1 語の変換がそのまま折り返します。`_wrapping` の綴りは Rust の `wrapping_add` などの接頭辞と、Almide の `wrap_add` に揃えています。
- (b) 純粋な計算です。両 target で同じです。
- (c) 他言語の `as` で書く意図（折り返す）を、名前で 1 通りに書けます。`almide fix` が整数からの無印を `_wrapping` に書き換えれば、今の動きをそのまま保てます（R-L0-32 の (d) をこれで置き換え）。
- (d) 追加（46 関数）。
- 完全性の規則: 整数から整数への損失のある組は、無印・`_checked`・`_saturating`・`_wrapping` の 4 つを持ちます。行列のゲート（R-sized-01）にこの列を足します。
- issue: #3995

## R-L0-35 無印の変換の丸めと NaN を、変換先が整数か浮動小数点かで分けて書く — ○ 採用

- いま: R-L0-32 の「結果の型で表せない値は abort」は、小数部の切り捨て、`Float32` への精度の損失、浮動小数点同士の NaN にも当てはまるように読めます。
- 案: 規則を次の表にします。実装はいまの動きのままで、文を確定させる案です。

| 変換先 | 小数部・精度 | 範囲外 | NaN |
|---|---|---|---|
| 整数（`float.to_int` など） | 0 の方向に切り捨て（`float.to_int(1.5)` = 1、`float.to_int(-1.5)` = -1） | abort | abort |
| 整数（整数から） | — | abort | — |
| 浮動小数点（`float.to_float32`・`int.to_float32`・`int.to_float` など） | 最も近い値に丸める（偶数への丸め。`int.to_float32(16777217)` = 16777216.0） | 無限大（`float.to_float32(1e300)` = 無限大） | NaN のまま |

- `_checked` は、どの行でも「正確に表せるときだけ some」です（R-L0-28）。切り捨て・丸め・範囲外・NaN はどれも none です。無印との違いは、無印が上の表の丸めを許すことです。
- 根拠の L0 規則: R-L0-28、R-L0-32。
- (a) IEEE 754 の変換の既定（最も近い値に丸める）と、Rust・Swift・Go・Kotlin の float から int への 0 方向の切り捨て。
- (d) 文書の確定だけです。
- issue: #3995

## R-L0-36 「間違えても型エラーになる」は、それだけでは不採用の理由にしない — ○ 採用

- いま: いくつかの不採用の理由が「書き間違えても型エラーになり、黙って通らない」です。Almide の物差しは改変生存率で、最初に書いたコードが型エラーになることも失敗に数えます。黙って違う値になるよりは良いですが、「そのまま正しく動く割合」は下がります。
- 案: 型エラーになることは、害を小さくする事情として書きます。不採用の理由そのものには、変更すると失われる具体的な性質（今のコードが黙って違う値になる、同じ役割に 2 つの綴りができる、など）か、計測の結果を書きます。どちらもないときは保留にして、Dojo で候補ごとの生存率を測ります。
- 見直した案: R-map-04 を保留に変えました（Dojo で測る）。R-datetime-04・R-duration-04・R-result-02・R-option-03・R-regex-06 は、型エラー以外の理由（同じ役割に 2 つの綴り、意味から決まる形、ADR の決定）を持つので据え置きます。
- (d) 判定の規則の追加です。関数は変わりません。
- issue: なし
## R-L0-37 標準の具体型は「protocol の組み合わせ」と「その型だけの関数」でできている — ○ 採用（方向）

- いま: 型ごとに同じ役割の関数を並べ、名前と形の揃いを規則（R-L0-16 など）と手書きの行列ゲートで守っています。R-L0-31 で一部の組（`Integer`・`FloatingPoint`・`Sized`・`TimeAmount`・`HttpMessage`）を protocol にしました。
- 案: これを標準の型全体の作り方にします。
  1. 2 つ以上の型が同じ役割で持つ関数は、必ず protocol のメソッドとして 1 回だけ宣言します。
  2. 型のモジュールの関数（`int.compare`・`list.len`）は、その protocol のメソッドそのものです。転送の関数を別に置きません（R-L0-22 の規則を全 protocol に広げる）。
  3. 型の公開面は「準拠する protocol の一覧」と「その型だけの関数」で書き表せます。
  4. protocol は階層に組みます。手本は Swift の標準ライブラリです（`Equatable`→`Comparable`、`AdditiveArithmetic`→`Numeric`→`BinaryInteger`→`FixedWidthInteger`、`Sequence`→`Collection`→`BidirectionalCollection`→`RandomAccessCollection` など）。Swift にない層は、理由を書いて足します。階層・型ごとの準拠・穴・この階層で決めることは [protocol-view.md](protocol-view.md) にあります（protocol の定義と準拠から、target-surface.md と突き合わせて機械的に作る）。R-L0-31 で名前を付けた protocol（`Integer`・`FloatingPoint`・`Sized`・`TimeAmount`・`HttpMessage`）は、この階層の中に置き直し、中身は階層の定義に従います。
- 「protocol だけ」にしない理由: `string.split` や `regex.find` のように 1 つの型しか持たない関数を、メンバーが 1 つの protocol にしても、名前が 1 つ増えるだけで揃いは守れません。Swift も、具体型だけのメソッドを持っています。
- 実測（リデザイン後の面）: 階層を決めた時点で、値の型 31 種の関数 1,041 個のうち 663 個が protocol の要件か数の変換の族で、要件のうちまだ無い関数（穴）が 209 個ありました（168 個は、`int` が持つ算術とビット演算をサイズつきの整数 8 型が持たないこと）。七巡目の案で 164 関数を足して埋め、九巡目で変換を変換先の `from` に（R-L0-40）、十巡目で列のアルゴリズムを `Sequence`・`Ordered` の既定の実装に（R-L0-42）まとめました。ビット演算は関数のまま `Integer` の要件です（R-int-10、R-L0-43）。リデザイン後は 1,291 関数で、protocol の関数を 1 回と数えると覚える名前は 650 ほどです。
- 言語に足りないもの（実測 `almide 0.68.0 dev`。どれも今は書けない）:
  - 型引数を通して、受け手のないメソッドを呼ぶ（`T.zero()` が E002）。`parse`・`max_value`・`nan` に要ります。
  - protocol の継承（`protocol Integer: Ord`）。
  - 関連型（`type Elem`）。入れ物の `contains`・`count` を protocol にするのに要ります（R-L0-31 で × にした理由がこれです）。
  - 既定の実装（`compare` から `min`・`max`・`clamp` を導く）。
  - 条件つきの準拠（`List[A]` は `A: Hash` のときだけ `Hash`）。
  - 入れ物そのものを引数にとる型（`F[_]`）は要りません。Swift と同じく、`map` は protocol に入れず型ごとの関数のままにします。
- 進め方: 5 つの機能は ADR を書いてから入れます。入れる前に、protocol を使う総称関数が LLM の書くコードの生存率を下げないことを Dojo で確かめます。型ごとの書き方（`int.compare(a, b)`）は同じ関数なので、どちらでも書けます。
- 位置づけ（十五巡目に追記）: [protocol の階層](protocol-view.md)は**想定設計**です。いま実装で守るのは、[リデザイン後の関数一覧](target-surface.md)の名前と形（利用者から見える公開面）です。内部の実装は、型ごとの具体的な関数のままでかまいません。階層は、上の 5 つの機能が言語に入るたびに、内部をそちらへ寄せていく将来の目標です。
  - 一覧の総称関数のうち、いまの言語で書けない形（組み込みの型を、利用者が宣言した protocol に準拠させる必要があるもの。`list.sum[A: Numeric]`、数の型の `from[S: Numeric]` など）は、いまの具体的な形を残します。言語の機能が入った時点で一覧の形に揃えます。どの行がこれに当たるかは、`scripts/check-stdlib-target.sh` が「言語の機能待ち」として数え、それ以外の差だけを減らしていく対象にします。
  - 利用者が宣言できる protocol で書けるもの（`io.read[H: Read]`、`http.header[M: HttpMessage]`）は、いま一覧の形で実装します。
- (d) 方向の宣言です。階層の要件を埋める関数は、七巡目の案（R-L0-38、R-sized-10・11、R-bigint-07、R-bool-03、R-list-20、R-string-16、R-deque-03・04）で足しました。R-L0-31 の × の行（`contains`・`count`・`Parse`）は、この階層の `Membership`・`Container`・`Parse` に置き直しています。
- issue: #4000

## R-L0-38 組み込みの `Repr` のメソッド名を `to_string` にする — ○ 採用

- いま: 組み込みの `Repr` のメソッドは `fn repr(v: Self) -> String` です。一方で数の型・`bigint`・`uuid`・`url` のモジュールは `<mod>.to_string` を持ち、`bool` は持ちません。同じ役割に 2 つの綴りがあり、型によって片方しかありません。
- 案: R-L0-22（`Ord` の `cmp` → `compare`）と同じく、`Repr` のメソッドを `to_string` にします。R-L0-37 のとおりモジュールの関数は protocol の関数そのものなので、すべての値の型が `<mod>.to_string` を持ち、中身は `"${x}"` と同じ文字列です。`Parse` を持つ型では `parse(to_string(x)) == ok(x)` が成り立ちます（Swift の `LosslessStringConvertible`）。
- (a) Swift は `CustomStringConvertible` の `description` を全型に持たせ、`LosslessStringConvertible` で `init?(_ description:)` と対にします。Rust の `Display`（`to_string`）と `FromStr` も同じ対です。
- (c) `int.to_string` から類推した `bool.to_string(b)` が通ります。型ごとに「この型には `to_string` があるか」を覚える必要がなくなります。
- (d) 改名と追加です。epoch 15 で利用者が書く `fn T.repr` を `@deprecated(since=15, use="to_string")` にし、epoch 16 で撤去します。`almide fix` が書き換えます。一覧には、型のページに行を持つ型（`Parse` を持つ型）の分を載せ、ほかの型の分は「一覧に出ない共通の約束」に書きます。
- issue: #4000

## R-L0-39 `^` を累乗の演算子から外し、累乗は `**` だけにする — ○ 採用

- いま: `^` は累乗（`**` の別名）です。実測（`almide 0.68.0 dev`）: `6 ^ 3` は `216` を返します。
- 問題: Rust・Go・Python・TS/JS・Swift の 5 言語で、`^` は排他的論理和（XOR）です。LLM が XOR のつもりで `a ^ b` と書くと、型エラーにならず黙って累乗の値になります。R-L0-32 の「黙って別の値を返さない」に反します。累乗に `^` を使うのは 9 言語のどれでもありません（Python・JS は `**`、ほかは関数）。
- 案: `^` を演算子の表から外し、字句エラーにします。ヒントは「累乗は `**`、ビットの排他的論理和は `int.bxor`」です。`**` はそのまま残ります。
- (b) 構文の変更で、両 target で同じです。
- (d) 構文の撤去です。epoch 15 で `^` に警告を出し、epoch 16 でエラーにします（R-L0-14）。`almide fix` が `a ^ b` を `a ** b` に書き換えます（いまの意味を保つ）。`proofs/dialect-epochs.toml` に行を足します。
- 空いた `^` は、別の意味で使い直しません（R-L0-43）。いつまでも「累乗は `**`、ビットの排他的論理和は `int.bxor`」と案内するエラーです。
- issue: #4002

## R-L0-40 数の変換は変換先の型に `from` で置く — ○ 採用

- いま: 変換は変換元のモジュールに、変換先ごとの名前で並びます（`int.to_float(n)`、`int8.to_uint16_checked(x)`）。13 の数の型の組み合わせで 347 関数あり、標準ライブラリの一覧の 4 分の 1 を占めます。
- 案: 変換先の型のモジュールに、どの数の型からでも受け取る総称関数として置きます。
  - `fn <T>.from[S: Numeric](x: S) -> T`（無印。結果の型で表せない値は abort）
  - `fn <T>.from_checked[S: Numeric](x: S) -> T?`
  - `fn <T>.from_saturating[S: Numeric](x: S) -> T`（整数の型だけ）
  - `fn <T>.from_wrapping[S: Integer](x: S) -> T`（整数の型だけ。整数からだけ）
  - 意味は R-L0-32・R-L0-34・R-L0-35 のまま変えません（綴りだけの変更）。値を失わない組では、無印は abort せず、`_checked` は必ず値を返します。
- (a) 変換先の側に置くのが多数派です: Rust `i8::from(x)`・`i8::try_from(x)`、Swift `Int8(x)`・`Int8(exactly:)`・`Int8(clamping:)`・`Int8(truncatingIfNeeded:)`、Python `float(n)`、Go `float64(n)`、JS `Number(x)`、OCaml `Float.of_int`。変換元の側は Kotlin（`toDouble()`）と Gleam（`int.to_float`）だけです。Swift の 4 つの初期化子と、Almide の 4 つの関数が 1 対 1 に対応します。
- (c) 覚える名前が、型 13 × 変換先 12 × 接尾辞の行列から、接尾辞 4 つになります。パイプでも同じに書けます（`n |> float.from`）。
- (d) 改名と撤去です。epoch 15 で `to_<T>…` を `@deprecated(since=15, use="<T>.from…")` にし、epoch 16 で撤去します。`almide fix` が `int.to_float(n)` を `float.from(n)` に機械的に書き換えます。数の型どうしの `to_string`・`parse` はこの案の外です。
- 言語に要るもの: 型引数つきの総称関数を protocol の境界（`S: Numeric`）で受け、受け手のない要件（`T.from`）を呼べること（#4000）。
- issue: #4003

## R-L0-41 ビット演算を演算子にし、`>>` を関数の合成から外す — × 取り消し（十二巡目。記号の意味を入れ替えないため → R-L0-43）

- いま: ビット演算は `int.band(a, b)` などの関数で、演算子はありません。`>>` は関数の合成（`f >> g`）、`^` は累乗でした（R-L0-39 で撤去）。
- 案: 整数の型（`int`・サイズつきの 8 型・`bigint`）に、`&`・`|`・`^`（XOR）・`~`（否定）・`<<`・`>>` を置きます。`>>` は、符号つきの型では符号を保ち、符号なしの型では 0 で埋めます（Swift・Rust・Go と同じ）。符号を無視した右シフトは、符号なしの型に移して書きます（`uint64.from_wrapping(x) >> n`）。
- 優先順位は Swift に合わせ、C の落とし穴（`a & b == c` が `a & (b == c)` になる）をなくします: `**` ＞ `<<` `>>` ＞ `*` `/` `%` `&` ＞ `+` `-` `|` `^` ＞ 範囲 ＞ `|>` ＞ 比較。
- 関数の合成 `f >> g` はやめ、`(x) => g(f(x))` か `x |> f |> g` で書きます。合成の演算子を持つのは 9 言語のうち少数で（F#・Haskell の系統）、Rust・Go・Python・JS・Swift の 5 言語で `>>` は右シフトです。実測: Almide のコード中で合成の `>>` を使うのは、合成そのものを試す spec の 7 行だけです。
- (a) R-int-05（×）は関数名だけを比べて「多数派がない」としました。演算子まで比べると、`&`・`|`・`^`・`<<`・`>>` は Rust・Go・Python・JS・Swift の 5 言語の綴りで、多数派です（R-L0-00）。
- (d) 構文の追加と意味の変更です。`&`・`|`・`~`・`<<` は epoch 15 から使えます。`^`（累乗）と `>>`（合成）は、epoch 15 で警告、epoch 16 でエラー、epoch 17 で新しい意味にします（同じ記号の意味を黙って入れ替えないため、間に 1 epoch のエラーを挟む）。`almide fix` が `int.band(a, b)` を `a & b` に、`f >> g` を `(x) => g(f(x))` に書き換えます。`band`・`bor`・`bxor`・`bnot`・`bshl`・`bshr`・`bshr_logical` は撤去します。
- issue: #4004

## R-L0-42 列のアルゴリズムは protocol の既定の実装として 1 回だけ書く — ○ 採用

- いま: `enumerate`・`zip`・`sort`・`min`・`take_while`・`chunk` など 30 の列のアルゴリズムが `list` だけにあります。`deque`・`set`・`string`・`bytes`・`map` では、`to_list` してから `list.*` と書くしかありません。`list.sum`・`product` は `Int` と `Float` の 2 つの形、`list.join` は `String` と `Bytes` の 2 つの形を持ちます。
- 案: [protocol の階層](protocol-view.md)の `Sequence` と `Ordered` に、これらを既定の実装として 1 回だけ置きます（Swift の `Sequence`・`Collection` の拡張と同じ）。R-L0-37 のとおり、準拠するどの列のモジュールにも同じ名前・同じ形で現れます。
  - `Sequence`（決まった順にたどれる列）: `enumerate`・`zip`・`zip_with`・`filter_map`・`find_map`・`reduce`・`scan`・`partition`・`group_by`・`sort`・`sort_by`・`sort_with`・`min`・`max`・`min_by`・`max_by`・`min_with`・`max_with`・`sum`・`product`。要素の型が変わる結果や並べ替えた結果は `List` を返します（Swift の `sorted()`・`map` が `Array` を返すのと同じ）。
  - `Ordered`（両端から数えられる列）: `find_index`・`take_while`・`drop_while`・`chunk`・`window`・`split_at`・`dedup`・`intersperse`・`unique`・`unique_by`。切り出した結果は同じ型を返します（Swift の `SubSequence`）。
  - 条件: `sum`・`product` は要素が `Numeric`、`sort`・`min`・`max` は要素が `Ord` のときだけです。`string` は `sum`・`product` を、`map` は `sort`・`min`・`max`・`sum`・`product` を持ちません。
  - `list.sum`・`product` は `sum[A: Numeric](xs: List[A]) -> A`、`list.join` は `join[T: Text](xs: List[T], sep: T) -> T` の 1 つの形にします。
- `Ord` の既定の実装から `min`・`max`・`clamp` を外し、`Integer`・`FloatingPoint` の要件に戻します（R-sized-08 の形）。`Ord` に置くと、`duration.min(n)`（n 分）や `string.min(s)`（最小の文字）と名前がぶつかるためです。
- (a) Swift（`Sequence`・`Collection` の拡張）、Rust（`Iterator` の提供メソッド）、Kotlin（`Iterable` の拡張関数）は、列のアルゴリズムを 1 か所に書き、すべての列で使えるようにしています。
- (d) 追加（`deque` 30・`set` 20・`string` 28・`bytes` 29・`map` 15）と、`list.sum`・`product`・`join` の型を広げる変更です。実装は既定の実装 1 つで、型ごとに書きません。
- issue: #4012

## R-L0-43 撤去した記号を別の意味で使い直さない — ○ 採用

- 規則:
  1. 演算子・記号は、撤去しても別の意味で使い直しません。撤去した記号は、いつまでも「撤去した」と案内するエラーのままです。
  2. 関数の名前を別の意味で使い直すのは、古い書き方が必ず型エラーか abort になるときだけです（例: `string.count(s, sub)` → 述語の `count(s, f)`。文字列を渡す古い書き方は型エラーになる。R-string-18）。
- 理由: epoch の間に 1 回エラーを挟んでも、古いコード・古い資料・LLM が覚えた書き方は残ります。同じ記号が、読む人と書かれた時期によって違う意味になります。整数の `a ^ b` を累乗から XOR に変えると、型検査を通ったまま違う値になり、R-L0-32 の「黙って別の値を返さない」に反します。
- この規則で九巡目の R-L0-41（ビット演算を演算子にし、`>>` を関数の合成から外す）を取り消します。`^` は累乗の別名をやめてエラーにするだけ（R-L0-39）、`>>` は関数の合成のままです。ビット演算は関数のまま（R-int-10）です。
- (d) 判定の規則の追加です。
- issue: #4004

## R-L0-44 入出力の protocol `Read`・`Write`・`Close` を置き、読み書きと閉じるを `io` の総称関数 1 組にする — ○ 採用

- いま: 読む・書く・閉じるが、取っ手の種類ごとに別の名前です。TCP は `net.tcp_read`・`tcp_write`・`tcp_close`・`tcp_close_listener`（取っ手は素の `Int`）、Unix ソケットは `unix.read`・`unix.write`・`unix.close`（`unix.Fd`）、UDP は `net.udp_close`（`UdpSocket`）、標準入出力は `io.read_bytes`・`io.write_bytes`（取っ手なし）。[protocol の階層](protocol-view.md)に入出力の層がありません。
- 案:
  ```almide
  protocol Read  { effect fn read(h: Self, max: Int) -> Bytes }      // 最大 max バイト。相手が閉じたら空
  protocol Write { effect fn write(h: Self, data: Bytes) -> Unit }   // 全部書く（Rust の write_all）
  protocol Close { effect fn close(h: Self) -> Unit }                // 2 回目も ok

  effect fn io.read[H: Read](h: H, max: Int) -> Bytes
  effect fn io.read_exact[H: Read](h: H, n: Int) -> Bytes            // 既定の実装。足りなければ err
  effect fn io.read_to_end[H: Read](h: H) -> Bytes                   // 既定の実装。閉じるまで全部
  effect fn io.write[H: Write](h: H, data: Bytes) -> Unit
  effect fn io.close[H: Close](h: H) -> Unit
  ```
  - 準拠する型: `net.TcpStream`（`Read`・`Write`・`Close`）、`net.TcpListener`・`net.UdpSocket`（`Close`）、`unix.Fd`（`Read`・`Write`・`Close`）、`io.Stdin`（`Read`）、`io.Stdout`・`io.Stderr`（`Write`）。
  - 総称関数は `io` に 1 回だけ置き、型ごとのモジュールには置きません（`http.header[M: HttpMessage]` と同じ形。R-http-12）。
- (a) 6 言語が、取っ手の種類をまたいで読み書きを共通にしています: Rust（`Read`・`Write` の trait。`read_exact`・`read_to_end` は既定の実装）、Go（`io.Reader`・`Writer`・`Closer`、`io.ReadFull`・`io.ReadAll`）、Python（file-like object）、Elixir（IO device）、OCaml（channel）、Kotlin（`InputStream`・`OutputStream`・`Closeable`）。Swift の標準ライブラリには入出力の protocol がありません（Foundation の `FileHandle` は具体型）。階層の骨組みは Swift ですが、Swift にない層は多数派で足します（`Container` と同じ）。
- 綴り: `read`・`write`・`close` は 9 言語の多数派です。`read_exact`・`read_to_end` は Rust の綴りです（Go は `ReadFull`・`ReadAll` ですが、`io.read_all()` はすでに標準入力をテキストで全部読む関数なので、同じ名前を 2 つの形に使えません）。
- 名前の使い直し（R-L0-43 の 2）: `io.write` はいま標準出力に `Bytes` を書く 1 引数の関数で、R-io-01 で撤去が決まっています。新しい `io.write(h, data)` は 2 引数なので、古い `io.write(data)` は型エラーになり、黙って意味が変わりません。
- (d) 撤去と追加です。内訳は L2-net R-net-07、L2-unix R-unix-04、L2-io R-io-05。
- issue: #4015

## R-L0-45 メモリ安全でない標準の関数は `ffi.read` と `ffi.write` の 2 つだけにする — ○ 採用

- いま: 生の番地を読み書きする関数が、import なしで見える `bytes` にあります（`bytes.from_raw_ptr`・`copy_to_ptr`）。どれが「Almide の値の外のメモリに触る関数」かを、標準全体で数えたことがありません。メモリ安全の証明（#4011）では、ここが言語の保証と外の世界の境目になります。
- 案:
  1. リデザイン後の標準で、Almide の値の外のメモリを読み書きする関数は `ffi.read` と `ffi.write` の 2 つだけです（R-ffi-01・R-ffi-02）。`ffi.address`・`ffi.to_int` は番地を値として作るだけで、メモリに触りません。
  2. `RawPtr` を引数か戻り値に持つ公開関数は、`ffi` の外に置きません。
  3. `ffi.read`・`ffi.write` は effect fn で、`import ffi` が要ります。critical プロファイルで拒むこと、ビルドの記録に「`ffi` を使った」と出すことは #4008 で決めます。
  4. ほかのすべての標準の関数は、範囲外を R-L0-33 の役割どおりに扱い（R-L0-46）、メモリを壊す手段を持ちません。
- 実測（リデザイン後の 1,289 関数）: `RawPtr` が現れるのは `ffi` の 4 関数だけです。
- (a) 9 言語とも、生の番地の読み書きは名前のはっきりした場所（Rust `unsafe`、Go `unsafe`、Python `ctypes`、Kotlin `cinterop`、Swift `UnsafeRawPointer`）に閉じています。
- ゲート案: 公開関数の型に `RawPtr` が現れるのは `ffi` だけ、を検査します。`ffi` に関数を足すときは、この規則の 1 の一覧を同じ PR で直します。
- issue: #4008、#4013

## R-L0-46 位置を受け取る関数を全数で表にし、役割の規則（R-L0-33）をゲートにする — ○ 採用

- いま: R-L0-33 が役割ごとの範囲外の扱いを決めていますが、位置を受け取る関数を全部数えて当てはめたことがありません。メモリ安全の空間の項（#4005）が検査する対象は、この一覧です。
- 全数（リデザイン後の 1,289 関数のうち、`Int` の位置・端・オフセットを受け取るもの）: 108 関数。1 つを読む 60、1 つを書く・差し込む・消す 33、範囲を取り出す 5、そのほか 10（`list.range` は位置ではなく値、`value.index` はパスを作るだけ、`bytes.skip`・`is_eof` は読み位置を動かす・調べる、`matrix.from_bytes_*` は none を返す、`matrix.slice_rows`・`fs.fold_lines_range` は範囲）。
- 1 つを読む 60 関数は、どれも型で none を返せます（`get_*` は `T?`、`next_*` は `(T?, Int)`）。
- 外れていたもの（実測 `almide 0.68.0 dev 437b8f127`）:
  - `fs.fold_lines_range(path, -2, 2, …)` は開始を 0 に丸めて畳みます。
  - `matrix.slice_rows(m, -1, 2)` は 0 行の行列を返します。
  - どちらも範囲を取り出す役割なので、R-L0-29 のとおり負の位置で abort にします（正の側のはみ出しは丸めたまま）。
  - `bytes.skip(b, 0, -5)` は負の読み位置 `-5` を返し、続く `bytes.read_u8(b, -5)` は `0` を返します。R-bytes-13・R-bytes-21 ですでに「`n < 0` は abort」に決めてあり、その判定が正しいことを実測で確かめました。
- ゲート案: 位置を受け取る公開関数の一覧を生成し、役割の列が空の行、1 つを読むのに none を返せない行、範囲・書き込みで負の位置の fixture（abort を確かめる）が無い行を拒みます。
- issue: #4005

## R-L0-47 信頼しない入力を読む関数は、時間と大きさに上限を持つ — ○ 採用

- いま: 外から来たバイト列や文字列を読む関数のうち、入力より大きく膨らむもの・深く入れ子になるもの・時間が爆発しうるものに、上限がありません（#4019・#4020）。
  - `zlib.decompress`・`inflate`・`gunzip` は出力に上限がなく、圧縮爆弾はメモリが尽きて abort するまで展開を続けます。
  - `json.parse` は入れ子の深さに上限がなく、深い入れ子はスタックの溢れで abort します（C-196）。サーバなら 1 つの要求でプロセスが落ちます。
  - 正規表現はバックトラックで一致を探し、`(a+)+$` のようなパターンと長い入力で時間が指数的に伸びます（ReDoS）。
- 案（規則）: 信頼しない入力を読む標準の関数は、次のどれかを満たします。
  1. 時間も出力の大きさも入力の大きさに比例する（`base64`・`hex` の decode、`url.parse`、`string.split` など。いまのままでよい）。
  2. 比例しないものは、既定の上限を持ち、上限に達したら err を返す（abort にしない）。上限は関数の説明に数で書く。
  3. 上限を変えたいときのために、上限を引数にとる関数を 1 つ置く（既定の引数は言語にないため）。
- 当てはめ: R-regex-12（線形時間の照合）、R-zlib-04（展開の出力の上限）、R-json-04（入れ子の深さの上限）。HTTP のサーバ・クライアントの上限はすでにあります（`http_server_core.rs`、`http_route_core.rs`）。
- (a) Go（`regexp` は線形時間、`encoding/json` は深さ 10000 で止まる）、Rust（`regex` は線形時間、`serde_json` は深さ 128）、Python（`json` は再帰の上限、`zlib` の `max_length`）、Node（`zlib` の `maxOutputLength`）。2026 年の Go の脆弱性（`archive/tar`・`encoding/asn1`・`net/mail` の資源の使い果たし）はこの規則の外で起きました。
- (c) 外からの入力をそのまま渡すのは、LLM がいちばん書きやすい形です。上限を書かせる設計にすると書き忘れが起き、既定で守られる設計なら書き忘れが起きません。
- ゲート案: 当てはめの 3 関数に、上限ちょうどと上限を 1 超える入力の cross fixture（err の文言まで両 target で同じ）。
- issue: #4019、#4020

## R-L0-48 Codec の入力: フィールドを読まない属性・`decode_strict`・フィールドの属性の語彙 — ○ 採用

- いま: 導出した `T.decode` は、宣言したフィールドを入力からすべて読みます。`type User: Codec = { name: String, is_admin: Bool = false }` に要求の本文 `{"name": "m", "is_admin": true}` を渡すと `is_admin` が true になります（mass assignment、#4108）。フィールドに `@skip` や `@serde(skip)` を書いても、属性は読まれずに捨てられ、何も起きませんでした。
- 案（規則）:
  1. レコードのフィールドに `@skip`（decode でも encode でも扱わない）と `@skip_decode`（decode では読まない、encode では書く）を置けます。どちらも既定値（`= 値`）が必要で、無いと E104 です。読まないフィールドのキーが入力にあっても、知らないキーと同じに扱います。別名の構文（`@skip_decode role as "role_name": String = "user"`）と組み合わせられます。
  2. 知らないキーは、これまでどおり `T.decode` では無視します。拒みたいときのために、Codec のレコード型すべてに `fn T.decode_strict(v: Value) -> Result[T, String]` を導出します。フィールドを読む前に、decode で読むフィールドのキー以外のキーを探し、文書の順で最初のものを `err("unknown key '<k>'")` で返します。variant 型には導出しません。
  3. レコードのフィールドに置ける属性は `@builtin`・`@location`・`@skip`・`@skip_decode` だけです。ほかの属性はエラーの E053 です（関数の属性の E053 は警告のまま）。ほかの言語の綴りには、Almide の綴りをヒントで示します（`@serde(skip)` → `@skip`、`@skip_deserializing` → `@skip_decode`、`@redact` → 型 `Secret[String]`）。
- 族: {decode, encode} × {読む, 読まない} の 4 マスのうち、`@skip` と `@skip_decode` が 2 マス、印の無いフィールドが 1 マスです。出力だけから隠すマス（`@skip_encode`）は意図して置きません。出力から隠すのは `Secret[A]` の型か、出力用の別の型の役目です。表は `crates/almide-frontend/src/attr_vocab.rs` の `CODEC_FIELD_SKIPS` に 1 回だけ宣言し、導出の両半分とゲートがそこを読みます。
- (a) 知らないキーを既定で無視するのは 8 つ中 6 つ（serde・Go・pydantic・Rails・Ecto・Swift）。拒む切り替えは serde の `deny_unknown_fields`、Go の `DisallowUnknownFields`、pydantic の `extra='forbid'`。フィールドを入力から外すのは serde の `skip`・`skip_deserializing`、Jackson の `READ_ONLY`、kotlinx の `@Transient` で、どれも既定値を求めます。
- (c) 知らないキーを拒んでも mass assignment は防げません。`is_admin` は知っているキーだからです。防ぐのは「入力用の型に decode してから、領域の型を組み立てる」書き方（Rails の strong parameters、Ecto の changeset、FastAPI の `UserIn`、OWASP）と、両方向の Codec が要る型に付ける `@skip_decode` です。CHEATSHEET はこの書き方を教えます。
- (d) `@skip`・`@skip_decode`・`decode_strict` は追加です。フィールドの知らない属性をエラーにするのは、受け入れていたコードを拒むので epoch 15 の breaks に 1 行あります。`decode_strict` は導出のメソッドで、標準のモジュールの関数ではないため target-surface.json には載せず、「一覧に出ない共通の約束」に書きます。
- ゲート: `spec/wasm_cross/codec_field_skip_matrix.almd`（{印なし, `@skip`, `@skip_decode`} × 入力 {キーなし, キーあり, 知らないキー} × {`decode`, `decode_strict`} と `encode`。C-426・C-427）、`tests/codec_field_skip_test.rs`（族の表から両 leg を検査し、`@skip_decode` を無視させる対照で admin=false が admin=true に変わることを確かめる）、`tests/diagnostics/e104-*`・`e053-unknown-field-attribute-*`（C-428）。
- issue: #4108

## ゲートの束（R-L0-G） — ○ 採用

R-L0-02、03、04、07、09、10、12、16 の検査を、1 つのテスト `tests/stdlib_surface_conventions_test.rs` にまとめます。入力は interface JSON（`mut` 欄つき）です。規則ごとの許容リストは、L2 の案で解消するたびに減らします。増やす方向には変えられません。

- issue: #3949
