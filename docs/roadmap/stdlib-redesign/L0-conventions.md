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
`math` には、型モジュールに置き場所がない数学関数だけを置きます（超越関数、`factorial`、`choose`）。
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
3. ひとつの動詞は、全モジュールで同じ様式（コピーかその場書き換えか）を表します。`set`・`insert`・`remove`・`delete`・`update` はコピー側の動詞です。

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
- (c) 行列の穴（`wrap_sub` がない）は、LLM が規則から推測して書いたときに確実に外れます。
- issue: #3854（既存）

## R-L0-09 Bool を返す関数は `is_`/`has_`、または決まった動詞 — ○ 採用

Bool を返す公開関数の名前は、`is_` か `has_` で始めます。そうでなければ、次の閉じた動詞集合のどれかにします: `contains`、`exists`、`starts_with`、`ends_with`、`any`、`all`。

- (a) `is` 接頭辞は 8/9 です（Elixir だけが末尾に `?` を付けます）。`contains` は 5/9、`starts_with` と `ends_with` も多数派です。
- (c) 名前から Bool が返ることが読めます。
- いまの例外（L2 で判定します）: `args.flag`、`bytes.eof`、`regex.full_match`、`net.unix_poll`。`bytes.read_bool` は Bool を読み出す関数なので、対象外です。
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

---

## ゲートの束（R-L0-G） — ○ 採用

R-L0-02、03、04、07、09、10、12、16 の検査を、1 つのテスト `tests/stdlib_surface_conventions_test.rs` にまとめます。入力は interface JSON（`mut` 欄つき）です。規則ごとの許容リストは、L2 の案で解消するたびに減らします。増やす方向には変えられません。

- issue: #3949
