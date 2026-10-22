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
   - 両端を持つ入れ物では、端を名前に付けた `push_front`・`push_back`・`pop_front`・`pop_back`（`deque`）。`push`・`pop` と同じ族として数えます
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
5. 例外: 数値変換の無印 `to_<型>` は、損失のある組でも折り返す版です（R-L0-08）。範囲外の値を黙って別の値に変えるので、1 の「無印は安全版」に当てはまりません。9 言語の多数派に合わせた例外として据え置き、安全版は `_checked`・`_saturating` の接尾辞で選びます。この例外をやめるかどうかは R-L0-21 で保留しています。

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

- `to_<T>`: 損失あり。整数は折り返し、浮動小数点から整数への変換は規則に従って丸める。
- `to_<T>_checked`: 範囲外なら `Option` で none。
- `to_<T>_saturating`: 範囲外なら端の値に張り付く。
- 折り返し演算 `int.wrap_add`、`wrap_sub`、`wrap_mul` と `int.wrap(n, bits)`: いまは `wrap_sub` と `wrap` がない（#3854）。

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

順序のある型（`int`、`float`、sized の数、`bigint`、`string`、`bytes`、`duration`、`compute`、`uuid`）には、`<mod>.compare(a, b) -> Int` を置きます。小さければ負、等しければ 0、大きければ正を返します。`list.sort_with` などの比較関数は、この形の関数を受け取ります（R-L0-10）。

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

## R-L0-20 critical プロファイルで、黙って折り返す `to_<型>` を拒む — 保留

- 内容: `almide check --profile critical` のときだけ、損失のある組の `to_<型>` をエラーにし、`_checked` か `_saturating` を書かせます。Rust の clippy の `cast_possible_truncation` と同じ考え方です。普段のコードは変わりません。
- 保留の理由: いまの critical プロファイル（ALS の bounded 規則）が保証するのは、安全さと資源の上限で、計算の正しさは含みません。`xs[i]` の範囲外や 0 での割り算も、critical で拒んでいません（2026-10-01 に実測）。正しさの規則（全域性）を critical に入れる件は、[certification-grade](../active/certification-grade.md) の CG-3 として先送りになっています。黙って折り返す変換だけを先に拒むと、critical の規則の性格がばらばらになるので、全域性の規則と同時に判断します。
- 決めるときの順番: ALS（almide/als）に規則を足す → pin を進める → 検査器（`check/bounded.rs`）を実装する。
- issue: なし（CG-3 を再開するときに一緒に起票します）

## R-L0-21 無印の `to_<型>` を、はみ出したら失敗する版に変える — 保留

- 内容: 損失のある組の `to_<型>` を `Option` を返す形（いまの `_checked` と同じ）に変え、折り返す版を `to_<型>_wrapping` にします。Zig と Swift の形です。R-L0-06 の 5 の例外がなくなります。
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
  2. 演算子の脱糖先は公開しません（`__add` などの内部の名前）。公開の書き方は演算子だけです。例外は、ほかの型にも同じ名前があって演算子と意味が違う関数（`floor_div`・`mod`）だけです（R-bigint-04、R-matrix-14）。
- 根拠の L0 規則: R-L0-01、R-L0-15、R-L0-17（呼び方は宣言から決まる）。
- (c) `+` が使えるのを見た LLM は `-m` とも書きます。そこで利用者に直せない内部エラーになるのを防げます。
- 完全性の規則: 演算子を持つ型ごとに、表の各セルが「脱糖先がある」か「診断つきの型エラー」のどちらかです。
- ゲート案: 表のすべてのセルについて、`almide check` とビルドの結果を固定する fixture を置きます。
- issue: #3973

---

## ゲートの束（R-L0-G） — ○ 採用

R-L0-02、03、04、07、09、10、12、16 の検査を、1 つのテスト `tests/stdlib_surface_conventions_test.rs` にまとめます。入力は interface JSON（`mut` 欄つき）です。規則ごとの許容リストは、L2 の案で解消するたびに減らします。増やす方向には変えられません。

- issue: #3949
