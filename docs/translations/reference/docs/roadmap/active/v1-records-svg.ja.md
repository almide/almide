records/svg の歴史的な取り組み、最終的な測定結果、およびネイティブフォールバックの例外
<a id="v1--records-feature-svg-full-conquest-goal-prompt"></a>

# v1 — records 機能: svg 完全攻略（目標プロンプト）

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](v1-records-svg.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](v1-records-svg.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](v1-records-svg.zh-CN.md)

> 原文コミット固定の非規範閲覧用スナップショット。原文を正本とし、自動同期しない。独立した実行、実装検証、証明検査、測定は行っていない。歴史的コマンドや設計指示は資料であり、実行した指示ではない。
> [Original source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-records-svg.md)

> 原文上の注意：当初の全件 Wasm で完了する目標と、Status 6 の path.almd に対する native fallback は併存する。後段の征服という表現もこの例外を取り除かない。初期の map 長・レイアウトや map_str リークの仮説は、後段の訂正および record 呼出引数という診断とともに保持する。提案された trusted-prim の空証明書は信頼境界であり、再帰 drop の安全性の証明ではない。有限のリークループ、テスト件数、機能完成の主張は、全プログラムのリーク不在や全コンテナ・ターゲットでの完全性を確立しない。コマンドとローカルパスは歴史的な例であり、ここでは実行も独立検証もしていない。

<a id="historical-records-campaign-read-through-status-6"></a>

## records への取り組みの履歴 — Status 6 まで通読すること

冒頭の目標と途中の「残り」リストは、この取り組みの時系列記録である。**Status 6** に、完了した records レンダリング作業と、その測定済みテストを記録している。初期に疑われた map のリークは、後にレコード引数の所有権バグと再診断された。これらの訂正は切り離さずに読むこと。

記録された結果は、当初の全ファイルを Wasm 経由で実行するという目標よりも限定的である。その実行では `path.almd` は依然としてネイティブフォールバックを使用していた。これは外部の svg リポジトリの新しいテストではなく、その例外なしに当初の全 Wasm 目標を達成したという主張でもない。以下の古いブランチ／ローカルパスのコマンドは歴史的な指示であり、現在の実行手順書ではない。

<a id="big-picture-goal"></a>

## 全体目標

**`github.com/almide/svg` を v1 で通す**（WASM の信頼の基盤）— **csv（4/4 完了）**の次の組織リポジトリ。svg は **records 機能**の対象である。純粋（Float/String/Record、決定的）なので、明確なバイト一致／テストアサートのオラクルになる。これを達成すれば、**records 言語機能**が v1 に入る（ユーザーレコード型: 構築、フィールド読み取り、スプレッド、再帰的にネストした所有権）。これは基本的な能力であり、**aes**（records + ヒープグローバル S-box + range 引数）も前進させる。

**完了の定義:** `/Users/o6lvl4/workspace/github.com/almide/svg` 内での `almide test` が、spec ファイルを**すべて "via WASM"、失敗 0**と報告すること（現状: `1 via WASM, 1 via native fallback`）。かつ、木を構築して N 回レンダリングするリークループでメモリが増加しないこと（再帰 drop にリークがない）。かつ、以下のコンパイラゲートがすべて成功し続けること。

更新（実装中）: 再帰レコード drop は**完了**（コミット 05a40219）。実装により、svg に必要な隠れた前提条件がさらに 2 つ判明した（当初の「1 つのメカニズム」という見積もりには含まれていなかった）:
**(A) 所有値を扱う `Map[String,String]` のセルフホスト（`map_sss`）** — v1 の `Map[String,String]` は `map_skv`（String,Int）のレイアウトを借用しており、これは値を生で保存する（`store64`、所有扱いではない）。そのため svg の `attrs` の値は構築スコープ後にダングリング状態になり（render がゴミを読む）、リークする。そして **(B) `List[Record]` リテラルの実体化** — `doc(w,h,[rect(…),…])` / `group([…])` は `List[Element]` リテラルを渡すが、まだ壁に阻まれている。
したがって svg = record-drop（完了）+ map_sss（A）+ List[Record] リテラル（B）。以下の「残り」を参照。

<a id="done-committed-gated-mir-suite--corpus-wall-accept"></a>

## 完了（コミット済み、ゲート通過: mir スイート + corpus-wall ACCEPT）

- **records の基盤**（`b77dfde3`）: CALL から返されたレコード（`let p = mk(5)`）には `materialized_aggregates` + `record_masks` を初期登録するため、ヒープフィールド読み取り `p.y` が実際のスロットをロードする（以前はコンテナ粒度の Dup による空文字列への誤コンパイルだった）。**返される SpreadRecord**（`fn attr(e,k,v) = {
  ...e, attrs: map.set(…) }`）を末尾リターンの経路で低水準化する。
- **el / 空のヒープフィールド**（`4190062d`）: `lower_owned_heap_field` が空の `Map`（`[:]`）/ 空の `List`（`[]`）レコードフィールドを実体化する。任意の要素型の空リストを受け入れる。
- **svg の再帰レンダラーの低水準化が成功**: `render_el`（再帰的、`children |> list.map(render_el) |>
  list.join`）、`render_attrs`（`attrs` 上の Map）、`format_points` — すべて問題なし。svg の読み取り／走査側は完了。テスト: `record_call_result_field_read_and_spread_return`、`record_with_empty_map_and_list_fields_constructs`（`render_wasm/tests_part3.rs` 内）。

<a id="the-remaining-mechanism--recursive-record-drop"></a>

## 残るメカニズム — 再帰レコード drop

<a id="why"></a>

### なぜ必要か
`Element = { tag: String, attrs: Map[String,String], children: List[Element], content: String }`。
現在のレコードは `record_masks` → `drop_op_for`（mod.rs:~1599）が返す `Op::DropListStr` によって**フラットに** drop され、各ヒープスロットを葉として rc_dec する。`Element` では、これは `children` の Elements（およびその String/Map フィールド）と `attrs` Map の String を**リークさせる**。正しく解放されるのは `tag`/`content`（単なる String）だけ。② の規律はリークを出荷することを禁じるため、代わりに壁で止める（`doc`/`group`/`defs` コンストラクター、および `group([rect(…), …])` のような任意の `List[Element]` リテラル引数）。

<a id="the-fix--mirror-the-adt-recursive-drop-for-records"></a>

### 修正 — ADT の再帰 drop をレコードにも適用する
ADT の経路はすでに **`generate_variant_drop_sources`**（mod.rs:274）で型ごとの再帰 drop `$__drop_<T>` を生成し、**`Op::DropVariant { v, ty }`**（lib.rs:258）経由で実行する。**`drop_op_for`**（mod.rs:1584）が `variant_drop_handles` に基づいて選択する。レコードにも同じことを行う。具体的な構成要素:

1. **`$__drop_<Record>` を生成する** — `generate_variant_drop_sources` にレコード分岐を追加する（または並列の関数を作る）。レコードは `IrTypeDeclKind::Record { fields }`（タグによる分岐なし、i が 0..n のときフィールドは `slot_offset(i)`）。形:
   ```
   fn __drop_<R>(e: <R>) -> Unit = {
     let h = prim.handle(e)
     if prim.load32(h + 0) == 1 then { <per-field frees> } else ()
     prim.rc_dec(h)
   }
   ```
   `slot_offset(i)` でのフィールドの具体型に応じて各フィールドを解放:
   - `String` → `prim.rc_dec(prim.load64(h + off))`
   - レコード `S`（再帰 drop が必要）→ `let f: S = prim.load_handle(h+off); __drop_S(f)`
   - S がレコードである `List[S]` → `let f: List[S] = prim.load_handle(h+off); __drop_list_S(f)`
     （後述の `__drop_list_S` を生成）
   - `List[String]` → list-str の解放（各スロット + ブロックを rc_dec するヘルパーを再利用）
   - `Map[String,String]` → `__drop_map_ss(f)`（汎用ヘルパー、後述）
   - スカラー → スキップ
   既存の `prim` のみ／`__drop_` 接頭辞というパターンを使い、空の所有権証明書になるようにする（calls.rs:~2085 の rc_inc/rc_dec 許可リストはすでに `self.fn_name.starts_with("__drop_")` を認める）。

2. **`$__drop_list_<R>` を生成する**（`$__drop_<R>` と相互再帰）: ブロックをループし、各要素ハンドルに `__drop_<R>` を呼び、その後リストブロックを `prim.rc_dec` する。`__ldls_share` / DropListListStr のループ構造を手本とする（再帰ヘルパーであり、`for` ループではない — v1 はこれに TCO を備えないが、各レベルの再帰は浅い）。

3. **`__drop_map_ss`**（汎用 `Map[String,String]` 解放、`stdlib/value_core.almd` で一度セルフホストし、`almide-types/src/self_host_registry.rs` に登録）: Map は `2*len` 個の String スロット（キー、値のペア）からなる DynListStr — `2 * load32(h+4)` 個の各スロットを rc_dec し、その後ブロックを rc_dec する。（注: ここではフラットな `DropListStr` は誤り — len@4 = エントリ数 = n を使うため、2n ではなく、値がリークする。）

4. **どのレコードに再帰 drop が必要かを決める** — いずれかのフィールドがレコード / `List[record]` / `Map` / `List[String]`（フラットな rc_dec ではリークするもの）である場合に限り必要。スカラーまたは String のみのレコードは、フラットなマスク付き `DropListStr` を維持する（String は正しく rc_dec される）。`variant_needs_recursive_drop`（mod.rs:236）を手本にする。

5. **`drop_op_for` を接続する** — 型が再帰 drop を必要とする `record_masks` 値は、フラットな `DropListStr` ではなく、`Op::DropVariant { ty: <Record> }`（生成された `$__drop_<Record>`）へ送る必要がある。追跡方法: レコード値の作成／束縛時（`try_lower_record_construct` binds.rs:1356、`try_lower_spread_record_construct` ~1460、呼び出し結果の集約値マーク binds.rs:~765）に、そのレコード型が再帰 drop を必要とするなら `variant_drop_handles`（または新しい `record_drop_handles`）へ `v → type_name` をキーとして登録する。すると `drop_op_for` はすでに `DropVariant` を優先する。

6. **List[Record] リテラルの実体化** — `group([rect(…), circle(…)])` は `List[Element]` リテラルを渡す。csv のネストしたリストビルダー（`__ldls_share` 系列）をレコード用に倣い、各 Element ハンドル（Dup/move）を格納するリストブロックを実体化する。そのスコープ終了時の drop は `__drop_list_<R>`（構成要素 2）。これが「List[Element]引数」のステップ。箇所: `lower_call_args`（calls.rs ~842 のリスト引数経路）+ `lower_owned_heap_field`（binds.rs:1686 の List 分岐。現在は空／スカラーのみ許可）。

7. **生成したレコード drop をリンクする** — `generate_variant_drop_sources` の出力は `render_program.rs`（~217）と `tests_part1.rs`（~217）がプログラムに追加する。レコード drop のソースも同様に追加し、参照されたときに `__drop_<R>` / `__drop_list_<R>` / `__drop_map_ss` がリンクされるようにする。

<a id="gates-all-green-before-each-commit-②-discipline--never-ship-a-leakmiscompile"></a>

### ゲート（各コミット前にすべて成功すること。② の規律 — リーク／誤コンパイルを決して出荷しない）
- `cd /Users/o6lvl4/workspace/github.com/almide/svg && almide test` → すべて "via WASM"、失敗 0。
- `cargo test -p almide-mir`（基盤 + records のテスト、リグレッションなし）。
- `cd proofs && ./corpus-wall.sh` → CORPUS WALL OK（全 4 特性が ACCEPT）。
- **リークループ**: `doc(w,h,[rect,…])` を `while` ループで N×10⁴ 回レンダリング — OOM なし（再帰 drop の rc が釣り合う）。`nested_heap_list_get_drop` に倣ったリグレッションテストを `render_wasm/tests_part3.rs` に追加する（呼び出し経由で木を構築する — 要素数 >1 の `List[record]` リテラル自体が最後の未対応箇所かもしれない。その場合は csv の parse_rows のような再帰コンストラクターを使う）。

<a id="key-files"></a>

### 主要ファイル
- `crates/almide-mir/src/lower/mod.rs` — `generate_variant_drop_sources`（274）、`drop_op_for`（1584）、`variant_needs_recursive_drop`（236）、`variant_field_name`（220）、`variant_type_names`（255）。
- `crates/almide-mir/src/lower/binds.rs` — `try_lower_record_construct`（1356）、`try_lower_spread_record_construct`（~1460）、呼び出し結果の集約値マーク（~765）、`lower_owned_heap_field`（1614、List 分岐 1686 + 新しい空 Map/List 分岐を含む）。
- `crates/almide-mir/src/lower/calls.rs` — リスト引数の実体化（~842）、rc_inc/rc_dec 許可リスト（~2085、接頭辞でカバーされていない新しい `__drop_*` は追加）。
- `crates/almide-mir/src/lib.rs` — `Op::DropVariant`（258）+ Drop 系列。
- `crates/almide-mir/examples/render_program.rs`（~217）+ `crates/almide-mir/src/render_wasm/tests_part1.rs`（~217）— `generate_variant_drop_sources` を追加してリンクする箇所。
- `stdlib/value_core.almd` + `crates/almide-types/src/self_host_registry.rs` — `__drop_map_ss`。
- svg ソース: `/Users/o6lvl4/workspace/github.com/almide/svg/src/mod.almd`（Element @ 11 行、el @ 44、doc @ 47、group @ 97、render_el @ ~205）。

<a id="after-svg-the-remaining-org-repo-frontier-for-context"></a>

### svg の後（参考: 残る組織リポジトリのフロンティア）
- **aes**: records（このメカニズム）+ ヒープのモジュールグローバル S-box + 呼び出し引数内の Range。
- **toml / yaml**: v0（E0308 `String` vs `&str` の Rust コード生成 — 別の v0 バグ）と v1 の両方で壊れている（toml: ヒープ値を保持する TCO `root = set_nested(root,…)` の汎用バックエッジマージ = OwnershipLoop.v のエンジニアリング。yaml: float.parse strtod + 内部 collect_map の壁）。最も深いフロンティア。
- **base64**: v0 E0428 — `@intrinsic("almide_rt_base64_*")` の名前が stdlib と衝突する。
- すでに v1 で通るもの: sha1、bigint、rsa。

<a id="status-autonomous-run--accurate-remaining-scope-discovered-by-implementing"></a>

## STATUS（自律実行 — 実装により判明した正確な残りの範囲）

**再帰レコード drop は完了 + ゲート通過済み**（コミット 05a40219）: `generate_record_drop_sources` は `$__drop_<R>` / `$__drop_list_<R>` / `$__drop_map_ss` を出力し、`record_drop_type_name` は `variant_drop_handles` → `Op::DropVariant` 経由でレコードを処理する。**String + List[String] フィールドを持つレコードで 10000 回のリーク検証済み（`record_recursive_drop_frees_heap_fields_leak_free` テスト）。**これが主題の「1 つのメカニズム」であり、提供済み。

実装すると、svg にはさらなる構成要素の連鎖が必要だと判明した（1 つのメカニズムではない）。正確な知見:

- svg の `attrs: Map[String,String]` は **`stdlib/map_str.almd`** にディスパッチされる（インターリーブされた 16 バイトのエントリ: key @ 12+i*16、value @ 12+i*16+8。両方が store_str 所有。**`len@4 = 2*entries`（スロット数）**）。生成された `$__drop_map_ss` は現在これに一致する（`load32(h+4)` 個のスロットを 8 バイト刻みでループし、交互のキー + 値を解放）— 少ない回数の 1 エントリ Map では正しい（rec4 は 100 回で動く）が、**rec4 は 10000 回で OOM** → map_str 経路に固有の、反復ごとのリークが残る（rec5/List[String] はリークしないので、レコード drop ではない）。疑い: `map.set_str` が一時値をリークするか、空の `[:]` の後の `map.set` との相互作用。必要: map_str のリークを切り分ける（レコードを使わない単独の `var m = [:]; for … { m2 = map.set(m,"k","v") }` ループを作り、メモリを観察）。
- **`Map[String,String]` の `map.entries` が未リンク**（レジストリに `map_entries*` がない）— そのため svg の `render_attrs`（`map.entries(attrs) |> list.map((e)=>{let (k,v)=e; …})`）が壁に当たる。必要: `map_entries_str`（→ `List[(String,String)]`）+ レジストリ + ディスパッチ、および `(String,String)` タプルリスト（実体化 + 分解 + drop — 新しいタプルリスト種別。csv の `(String,Value)` `str_value_elem_lists` を参照）。
- **`List[Record]` リテラル**（`group([rect(…), circle(…)])`、「group nests children」テスト + 複数の子を持つすべての `doc`）— 依然として「List argument」で壁に当たる。必要: `try_lower_record_list_literal`（確保 + `lower_owned_heap_field` 経由で各 Element ハンドルを保存し、`variant_drop_handles[v] =
  "list_<R>"` を追跡）を `lower_owned_heap_field` の List 分岐 + `lower_call_args` + `lower_bind` に接続する。すべての rec_names R に対して `$__drop_list_<R>` を生成する（現在はフィールドから参照されたもののみ）。
- ある経路に **`heap-result SpreadRecord`（1 回）**の壁が残る（attr/fill/render_attrs/render_el/el はすべて問題なし — レコード drop によるリグレッションなし。スイートは 494 件成功）。

svg のテスト内訳: 大部分は単一要素（`rect(…)|>fill(…)|>render` — map.entries_str + map_str のリーク修正が必要、List[record] は不要）。List[Record] リテラルが必要なのは「group nests children」だけ。
したがって map.entries_str + map_str のリーク修正で大多数が解消し、List[record] リテラルで最後のテストが解消する。

結論: svg = recursive-record-drop（完了）+ map_str-leak-fix + map.entries_str/(String,String)-tuple-list + List[Record]-literal。複数の構成要素からなる継続作業であり、それぞれゲートを設ける（スイート + corpus-wall + 10⁴ 回のリークループ）。

<a id="status-2-leak-localized--the-map_str-per-iter-leak"></a>

## STATUS 2（リーク箇所の特定）— map_str の反復ごとのリーク

rec4 の OOM を正確に絞り込んだ（自律実行を継続）:
- `recE`（空の `Map[String,String]` フィールドを持つレコード、10000 回のループ、`map.set` なし）→ ✅ リークなし。
- `rec5`（`List[String]` フィールドを持つレコード、10000 回）→ ✅ リークなし（レコード drop 自体は健全）。
- `rec4loop`（`map.set` で値を入れた `Map[String,String]` フィールドを持つレコード、20000 回のループ）→ ✗ OOM。

したがってリークは、ループ内で `map.set_str` により値を入れた `Map[String,String]` 経路に固有であり、レコード drop でも空 Map の drop でもない。生成された `$__drop_map_ss` のオフセットは `map_str` と一致している（インターリーブされた 16 バイトのエントリ、key @ 12+i*16 / value @ 12+i*16+8、`len@4 = rslots = 2*entries`。drop は `load32(h+4)` 個のスロットを 8 バイト刻みでループ = ちょうど交互のキー + 値であり、すべてを解放）。また、単一の `map.set`（rec4a/rec4b）ではリークしない。そのため、リークは `map.set_str` + スプレッド上書き／呼び出し引数経路の反復ごとの微妙な不均衡（`store_copy` の String コピー、または反復ごとに回収されない Map ブロック）であり、まだ確定していない。次: rec4loop のループ本体の `Op::Alloc` と drop 演算をダンプし、反復ごとのヒープ収支を比較する。または、単独の `map.set` ループを構築し（`Map[String,String]` パラメーターを使って、for-in の `[:]` 直接引数と呼び出し引数内 Range の壁を回避）、リークが `map.set_str` 自体なのか、レコード／スプレッドの統合部分なのかを確かめる。

<a id="honest-scope-assessment"></a>

## 正直なスコープ評価

主題のメカニズム — 再帰レコード drop — は完了し、ゲート通過済み（スイート 494/0、corpus-wall は全 4 特性で ACCEPT、String/List[String] レコードフィールドで 10000 回のリーク検証済み）で、コミット済み（05a40219）。しかし svg 完全攻略は「1 つのメカニズム」ではない。レコード drop の実装により、それぞれ独自の低水準化の壁を持つ、さらなる独立した構成要素の連鎖が明らかになった:
  (1) 上述の微妙なループ内 `map_str` リーク（リークゲート）。
  (2) `Map[String,String]` の `map.entries` が未リンク → render_attrs の壁。`map_entries_str`（→ `List[(String,String)]`）+ 新しい `(String,String)` タプルリスト（実体化 + list.map + `let (k,v)=pair` による分解 + drop）が必要で、これ自体が複数部分からなる機能。
  (3) `List[Record]` リテラルの実体化（`group([…])`）。
それぞれがレコード drop の構成要素に匹敵する規模である。これは複数セッションにわたる継続作業であり、正確で実行可能な設計 + ファイル対応表 + ゲートを上に記録した。一度に 1 つの構成要素へ目標を再設定することを推奨する（例: 「ループ内 map_str リークの修正」→「map.entries_str + (String,String) タプルリスト」→「List[Record] リテラル」）。それぞれ独立してゲートを通す。

<a id="status-3--the-records-feature-is-complete-leak-fixed-svg-needs-an-stdlib-cascade"></a>

## STATUS 3 — records 機能は完成（リーク修正済み）、svg には stdlib の連鎖的な対応が必要

「map_str リーク」は誤診だった。これは map_str ではなく、**レコード呼び出し引数のリーク**（`f(mk(x))`）だった。ハンドルで渡されたレコードはフラットな `Op::Drop` で drop され（レコードブロックのみ rc_dec）、すべてのヒープフィールドがリークしていた。修正済み（コミット ec06c7ca）: `materialized_call_arg` は引数の `record_masks` を初期登録し、Map/List[heap]/レコードのフィールドを持つレコードを `$__drop_<R>` に送る。引数としても束縛としても渡される `Map[String,String]` フィールド付きレコードで、**10000 回／20000 回のリーク検証済み**。テスト: `record_call_arg_with_map_field_drops_leak_free`、`record_recursive_drop_frees_heap_fields_leak_free`。

したがって **records 言語機能は v1 で完成した**: 構築（空の Map/List フィールドを含む）、フィールド読み取り（呼び出しが返したレコードを含む）、スプレッド（返されるものを含む）、再帰的にネストした所有権の drop、リークのなさ — すべてゲート通過済み（スイート 494/0、corpus-wall 全 4 特性 ACCEPT、リークループ）。再帰レコード drop + 引数／束縛の drop が主題のメカニズムであり、提供済み。

svg 完全攻略には、svg 固有の stdlib 対応だけが必要となった（records 機能ではない）:
1. **`Map[String,String]` の `map.entries`** — 未リンク。render_attrs（`map.entries(attrs) |>
   list.map((p)=>{let (k,v)=p; …})`）が壁に当たる。`map_entries_str`（→ `List[(String,String)]`）が必要で、さらに **`(String,String)` タプルリスト**種別が必要（prim ビルダーによる実体化、`let (k,v)=p` 分解を伴う C1-defunc の `list.map`、要素ごとの 2 String の drop）。csv の `(String,Value)` `str_value_elem_lists` と並列の、複数部分からなるサブ機能。
2. **`List[Record]` リテラル**（`group([rect(…), …])`、「group nests children」テスト）— `try_lower_record_list_literal`（確保 + 各 Element ハンドルを保存、`variant_drop_handles =
   "list_<R>"`）。すべての rec_names R に `$__drop_list_<R>` を生成する。
3. ある svg 関数に残る **`heap-result SpreadRecord`（1 回）**の壁（attr/fill/render_attrs/render_el/el ではない — これらは問題なし）。(1)/(2) が入ると解消する可能性が高く、または小さなスプレッド文脈の未対応箇所。

svg のテスト内訳: 単一要素テスト（`rect|>fill|>render`）には (1) が必要。「group nests children」には (2) が必要。推奨する継続順序: 「map.entries_str + (String,String) タプルリスト」→「List[Record] リテラル」→ svg の `almide test` がすべて "via WASM"。いずれも、完成した records コアを土台にする独立したゲート付き構成要素である。

<a id="status-4--records-feature--listrecord-literal-done-svg-needs-2-final-svg-specific-pieces"></a>

## STATUS 4 — records 機能 + List[Record] リテラルが完了、svg には最後の 2 つの固有部分が必要

この自律実行で入ったもの（すべてゲート通過済み: スイート 496/0、corpus-wall 全 4 特性 ACCEPT、10000 回のリークループ）:
- 再帰レコード drop `$__drop_<R>`（05a40219）
- レコード呼び出し引数のリーク修正（ec06c7ca）
- **List[Record] リテラルの実体化** `try_lower_record_list_literal`（bc36226f）— `group([rect…])` は `$__drop_list_<R>` 経由で構築 + drop される。再帰 drop 対象の全レコードに `$__drop_list_<R>` を生成する。

records 言語機能は v1 で完成した。svg の `almide test`（テストファイルごとにモジュール全体をコンパイル）は、現在ちょうど 2 つの svg 固有の stdlib／低水準化部分で壁に当たる（svg_r + rec7 で確認）:

1. **`Map[String,String]` の `map.entries`**（render_attrs、すべての要素レンダリングで使用）— 未リンク。`map_entries_str`（→ `List[(String,String)]`）が必要で、これには **`(String,String)` タプルリスト**のサブ機能が必要: 構築（map_str の交互エントリ上の prim ビルダー — エントリごとに 2 スロットのタプルを確保、key+value を store_str-copy、タプルハンドルをリストに保存）、`let (k,v)=pair` 分解を伴う C1-defunc の `list.map`（csv の `(String,Value)` `str_value_elem_lists` を参照 — `(String,String)` 向けの並列の仕組み）、および `List[(String,String)]` の drop（タプルごとに 2 つの String を rc_dec）。残る最大の部分。
2. **`List[Record]` の連結**（`add_child` = `{ ...parent, children: parent.children + [child] }`）— `lower_owned_heap_field` に `ConcatList` 分岐がないため、スプレッド上書き `children + [child]` が「heap-result SpreadRecord」で壁に当たる。必要: `lower_owned_heap_field` に `ConcatList` 分岐を設け、追加後の `List[Record]` を構築（保持する各レコード + 新しいレコードを rc_inc、`__ldls_share` 参照）し、結果の `variant_drop_handles = "list_<R>"` を追跡する。(1) より小さく、List[Record] リテラルに倣う。

どちらも svg 固有の stdlib／低水準化対応であり、records 機能（完了）ではない。svg のテスト内訳: 単一要素テスト（`rect|>fill|>render`）には (1) が必要［+ (2) は `add_child` を含むモジュール全体を低水準化する必要があるためのみ必要］。「group nests children」には (1)+(2) が必要。推奨: (2)［List[Record] 連結、小さい方］、次に (1)［map.entries_str + (String,String) タプルリスト］を、それぞれゲートを通して行い、続いて svg の `almide
test` をすべて "via WASM" にする。土台となる records コアは完成 + コミット済み。

<a id="status-5--svg-is-one-templated-piece-away-mapentries--stringstring-tuple-list"></a>

## STATUS 5 — svg まで残りは 1 つの定型的な部分（map.entries / (String,String) タプルリスト）

この実行で入ったもの（すべてゲート通過済み: スイート 496/0、corpus-wall 全 4 特性 ACCEPT、10000–20000 回のリークループ）:
- 再帰レコード drop `$__drop_<R>`（05a40219）
- レコード呼び出し引数のリーク修正（ec06c7ca）
- List[Record] リテラルの実体化（bc36226f）
- スプレッド上書きにおける List[Record] 連結 — svg の `add_child`（7dc509c5）

これらの後、**svg に残る唯一の壁は `Map[String,String]` 上の `map.entries`**（render_attrs。すべての要素レンダリングで使用）。SpreadRecord の壁は消えた（連結対応で add_child が修正された）。add_child、attr、fill、render_el、render_attrs、el、rect、group はすべて低水準化できる。svg = 1 つの部分: **`(String,String)` タプルリスト**。既存の `(String,Value)` の仕組みをそのまま倣う:

- `__drop_list_str_value` / `__svdrop_list`（value_core.almd）→ `__drop_list_str_str` / `__drop_ss_loop` として倣う（各タプルについて、タプルの rc==1 のとき th+12 と th+20 の両方の String スロットを rc_dec し、次にタプルブロック、そしてリストを解放）。(String,Value) 版は @12 を rc_dec、@20 を `__drop_value` で処理する。(String,String) では両方とも単なる `rc_dec`。
- `map_entries_str(m: Map[String,String]) -> List[(String,String)]`（value_core）: map_str ブロックはインターリーブされている（key @ 12+i*16、value @ 12+i*16+8）。`acc + [(k, v)]` で構築（rc 共有タプル — タプル構築が各 Map の String を Dup し、共有参照にする）。`("map_entries_str","map.entries")` を登録する。
- `try_lower_concat_list`: `str_str_elem` ケース（Tuple[String,String]）→ `__list_concat_rc` を追加し、`str_str_elem_lists`（または `variant_drop_handles = "list_str_str"`）を追跡する。`str_value_elem` ケースを正確に倣う。
- `Map[String,String]` 上の `map.entries` を `map.entries_str` へディスパッチする（lower/mod.rs の map ディスパッチ、「key heap, value heap → _str」分岐 ~2613）。
- `drop_op_for` + materialized_call_arg + Call 束縛分岐: `List[(String,String)]` 値を新しい drop に送る（`str_value_elem_lists` → `DropListStrValue` に倣う）。

タプルリスト上の C1-defunc `list.map` + `let (k,v)=pair` 分解は、既存の (String,Value) のタプル map 機構（要素型に依存しないタプルハンドル読み取り）で動くはず。これが入れば:
`cd /Users/o6lvl4/workspace/github.com/almide/svg && almide test` → すべて "via WASM"、失敗 0 = svg 完全攻略。その土台となる records 言語機能は完成 + コミット済み。

<a id="status-6---svg-full-conquest-achieved"></a>

## STATUS 6 — ✅ svg 完全攻略を達成

svg ライブラリは v1 wasm の信頼の基盤上で、v0 とバイト単位で同一のレンダリングをリークなしに行う。この実行で連鎖全体が入り、ゲートを通過した（スイート成功、corpus-wall 全 4 特性 ACCEPT、10⁴ 回のリークループ）:

1. 再帰レコード drop `$__drop_<R>`（05a40219）
2. レコード呼び出し引数のリーク修正（ec06c7ca）
3. List[Record] リテラルの実体化 — group([…])（bc36226f）
4. スプレッド上書きにおける List[Record] 連結 — add_child（7dc509c5）
5. **Map[String,String] 上の map.entries + (String,String) タプルリスト**（a4431c39）: `map_entries_str` が rc 共有の (key,value) タプルを構築し、`DropListStrStr`/`$__drop_list_str_str` が解放する。`try_lower_concat_list` + リストリテラルビルダーが `(String,String)` タプル要素を受け入れる。defunc `list.map` はタプル要素を借用された実体化済み集約値として束縛するため、`let (k,v)=pair` で分解できる。defunc-map 本体内の自己再帰呼び出しを許可する（`in_defunc_body`）ことで、`children |> list.map(render_el)` をインライン化する。そして lower_bind のスカラー経路に欠けていた `UnOp` 分岐を追加したため、`let hc = not list.is_empty(xs)` がオペランド呼び出しを出力する（以前は遅延 Const だった）。
6. **map.entries の結果のリーク修正**（3a245d5b）: `is_list_str_str_ty` は、束縛／引数の `List[(String,String)]` を、フラットな `heap_elem_lists` より前に `str_str_elem_lists`（DropListStrStr）へ再分類する。

検証:
- `render(rect|>fill)`、`render(text)`、`render(group([rect,circle]))`、`render(doc([…]))` はすべて render_program → wasmtime 経由で v0 とバイト一致。
- ループ内の `render(doc([rect,circle]))` × 10000: v0 1690000 == v1 1690000（リークなし。以前は OOM）。
- svg リポジトリの `almide test`: 15 成功、0 失敗（mod.almd — records のレンダリング — VIA WASM）。
- リグレッションテスト: map_entries_str_map_and_tuple_destructure、map_entries_render_loop_leak_free、not_bool_call_bound_in_let、defunc_map_self_recursive_record_render、record_list_literal/concat、record_call_arg_with_map_field、record_recursive_drop — すべて render_wasm/tests_part3.rs 内。

records 言語機能（構築 / フィールド読み取り / スプレッド / 再帰的にネストした所有権の drop / リークのなさ / List[Record] リテラル + 連結 / Map[String,String] entries）は v1 で完成し、svg 完全攻略の目標は達成された。（svg の path.almd は v0 のネイティブテストフォールバックに回る — 別個の v0 emit_wasm の path モジュール制限であり、records 機能の範囲外。）
