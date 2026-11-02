説明: パーサー TCO の突破口 — リポジトリ横断の heap-result-expression を解放する
<a id="v1--the-parser-tco-lever-the-real-heap-result-expr-cross-repo-lever"></a>

# v1 — パーサー TCO の突破口（真の「heap-result-expr」リポジトリ横断の突破口）

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](v1-parser-tco-lever.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](v1-parser-tco-lever.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](v1-parser-tco-lever.zh-CN.md)

> 原文コミット固定の非規範閲覧用スナップショット。原文を正本とし、自動同期しない。独立した実行、実装検証、証明検査、測定は行っていない。歴史的コマンドや設計指示は資料であり、実行した指示ではない。
> [Original source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-parser-tco-lever.md)

> 原文の歴史的留保：これは非時系列の調査記録であり、現在の実装や安全性の保証ではない。後方のターン4記録が値を返す Camp-4 match の対応を報告していても、冒頭ターン5の計画は未完了として残る。旧来の壁、撤回した試作、ゲート変更案、診断は後続の訂正と併存する。value.str のムーブ対コピー、タプル結果の経路とタプル要素追加の適用条件、合成呼び出しのゲート計上ではなく value.null をインライン化する解決などである。旧 csv/Value TODO と AES/base64 の壁数は、後続報告に合わせて統一していない。yaml が動作しても反復解析がリークしない意味ではなく、原文は最終的な OOM と本番向け堅牢化を明記する。バイト比較、有限リークループ、スイート／コーパス件数、FIPS 197 の例の出力は原文が報告した確認に限られ、普遍的メモリ安全性、暗号の安全性、資格化、独立再測定ではない。ASCII 小文字化と浮動小数点解析の範囲は原文どおりであり、直列化の回避策は一般的 heap-result-if 低水準化を解消しない。native v0 オラクルの失敗と v1 MIR 基盤／旧 wasm コード生成の区別を維持する。証明書の受理は、信頼される runtime／renderer の全演算を網羅せず、すべてのバイト単位の誤コンパイルを検出するものでもない。歴史的な次セッションの指示は実行しない。解決できない wiki 形式の参照は、リンクを捏造せず原文テキストのまま保持する。

<a id="-decode_chunks-onion--fully-mapped-2026-06-22-turn-5-effortmax-the-remaining-base64-decode--toml-frontier-is-one-coherent-slice-tco-over-a-match-whose-result-is-a-result-with-an-ownership-critical-core-the-unwrap--desugar-turns-let-x--fp-recurse-accx-into-match-fp--okx--recurse-accx-erre--erre---recursion-inside-a-match-arm-returning-resultheap-string-to-lower-it-verified-repro-umr--a-1-unwrap-version-build-it-first-it-is-the-unit-test-for-the-whole-slice"></a>

## ⭐ decode_chunks の多層構造 — 全容を解明（2026-06-22、ターン 5、effort=max）。残る「base64 decode / toml」の未解決領域は、所有権上重要な核心を持つ一つのまとまった実装単位：**結果が `Result` である `match` に対する TCO**。unwrap-`!` の脱糖は `let x = f(p)!; recurse(.., acc+[x])` を `match f(p) { ok(x) => recurse(.., acc+[x]), err(e) => err(e) }` に変換する。つまり match アーム内で再帰し、`Result[heap, String]` を返す。これを低水準化するには（検証済み、再現例 `umr` = unwrap 1 個の版。まずこれを作ること。この実装単位全体の単体テストになる）：

1. **match に対する TCO** — `tco_collect` + `tco_rewrite` は `match` アームを再帰的に処理する必要がある（現状は `if`/`Block` のみ）。小規模で、`if` アームと対称的。（試作後に撤回済み。単独では健全だが不完全。）
2. **`tco_empty_for(Result[_,String])` → `err("")`** — 結果アキュムレータの仮の値（基底ケースが上書きする）。小規模。**これが TCO を発動させる**（なければ `tco_empty_for` が None を返す → TCO が適用を辞退 →「heap-result if」の壁）。試作では壁が第 4 層へ移り、1+2 が動くことを証明した。
3. **ループ本体の match の実行** — TCO のループ本体は `match f(p) { ok(x)=>{acc=acc+[x]; p=..}, err(e)=>{rk=..; result=err(e)} }` になる。このユーザー `Result` の文としての match は線形化せず実行する必要がある → lower_branch の対象追跡（control.rs:104）には、ユーザーの名前付き呼び出し `Result` のアームが必要（scalar-Ok→materialized_results+heap_elem_lists、heap-Ok→materialized_results_str cap-tag）。別の潜在的な線形化による誤コンパイルも修正する（hok2：Unit 文 match 中のユーザー heap-Ok `Result` は、現在両アームを実行してしまう）。
4. **⚠ スカラーループの Result スロットへの Assign（所有権上重要 — 最重大違反の層）** — ループ内の `result = ok(acc)` / `result = err(e)`（mod.rs:1532 のスカラーループ Assign は `acc=acc+[x]` の追加 + `[]`/`""` へのリセットしか扱わない。右辺が `ResultOk`/`ResultErr` だと延期 → try_lower_scalar_while が適用を辞退 →「while body heap-accumulator reassignment」の壁）。`materialize_result_str` で実体化し、かつ **持ち越すペイロード `acc` を移入前に `Dup` する**ことが必須。そうしないとループ末尾の `acc` の drop が、今は `result` の所有するリストを二重解放する。証明書は必ずこれを検証しなければならない（力ずくで通さず、慎重な実装単位として扱う理由）。
5. **実際の decode_chunks** にはさらに、深さ 4 の入れ子 unwrap-match（match 対応 TCO は再帰処理で入れ子を扱う）と、最内側 ok アーム内の `new_acc` 入れ子 if アキュムレータが加わる（導入済みの flatten-let-block + inline-tail-accumulator + let-bound-if の事前脱糖が match の入れ子と組み合わさる必要がある。抽出した関数で検証する）。

コーパスに触れる前に、1–4 をまとめて行い、`umr` で v0==v1 のバイトテストを行うこと（corpus-wall はバイト単位の誤コンパイルを検出しない）。この一つの実装単位で base64 decode（3 個の壁）と toml の大部分（22）を解消する。今回のターンでは 1+2+3 を試作し、TCO は発動した。ただし 4 の所有権 Dup と 3 の結合は、ばらばらでなく証明書の下で慎重に共同実装する必要があるため撤回した。


<a id="cross-repo-conquest-scoreboard-real-githubcomalmide-repos-on-the-v1-spine"></a>

## リポジトリ横断の攻略スコアボード（v1 基盤上の実際の `github.com/almide` リポジトリ）

- ✅ **csv** — 公開関数 4/4 がバイト一致（後述）。
- ✅ **svg** — 完全攻略（2026-06-21）。レコードベースのレンダラー（rect/text/group/doc/入れ子 children + `map.entries` 属性）は v0 とバイト単位で同一の出力を生成し、10⁴ 回でリークなし。レコードの言語機能（構築 / フィールド読み出し / spread / 再帰的な入れ子所有権の drop / List[Record] リテラル+連結 / 新しい `(String,String)` タプルリストによる `Map[String,String]` の entries）は v1 で完成。`almide test` は 15/0（mod.almd、WASM 経由）。設計全体とコミット履歴：[[v1-records-svg]]（STATUS 6）。すべてのリポジトリに役立つ横断的な修正：`not <bool-call>` の let アーム（lower_bind UnOp）、defunc-map の自己再帰の許可（`in_defunc_body`）、`List[(String,String)]` 用の `DropListStrStr`。
- ✅ **yaml — V1 で動作（2026-06-22）。** `parse` は約 13.7k 行の WAT にコンパイルされ（壁 0）、wasmtime 上で実際の YAML をバイト単位で正しく解析する。マップ、シーケンス、入れ子、文字列/整数/浮動小数点/真偽値がすべて `parse`+`stringify` で往復する（`a: 1 / b: hello / c:\n  - x\n  - y` → 同一）。全連鎖が実行される：list.enumerate + クロージャ付き (Int,String) に対する find、list.set_str、effect 関数の (Value,Int) タプル結果 TCO、heap-result-if の Match アーム一時フレーム、heap-Value の末尾/bind drop 修正。残り（「動作」を妨げない）：多数回の解析で現れるリークの裾野（マップは約 1000× まで概ね問題なし、シーケンス経路ではより早くリーク）。深いパーサーで呼び出しごとに少量の解放漏れがある。後述の STATUS（2026-06-22、runs）参照。
- 🟢 **bigint / rsa / sha1 / porta** — 低水準化の壁 0（全関数を低水準化できる）。`sha1` はそのままでテストベクトルとバイト一致（2026-06-21 のプローブ）。確定にはバイト一致の監査とリークループだけが必要。
- 🟡 **base64 (9/13) / aes (17/30) / almide-sqlite (20/28) / toml (22/48)** — 機構上の壁が残る。

<a id="yaml-status--the-1-wall-is-the-heap-result-tuple-tco-option-c-engineering-integration"></a>

## yaml の状況 — 残る 1 個の壁は heap-result-tuple TCO（選択肢 C の実装統合）

`collect_map` が壁に当たる（`heap-result if … would move out an empty deferred heap value`）。根本原因：`(Value, Int)` タプルを返し、`pairs: List[(String, Value)]` を蓄積する相互再帰パーサー（`collect_map ↔ map_entry ↔ after_colon`）である。末尾の自己呼び出し（`collect_map(…)!`、空行のスキップ）は heap-result アームの自己呼び出しゲートに拒否される。これは正しい。v1 にはヒープ結果の末尾再帰用 TCO がなく、深い yaml は wasm スタックをあふれさせるため。したがって自己呼び出しを許すのは不健全。正しい修正は **選択肢 C の追加型アキュムレータ TCO**（同階層のエントリループを実際のループにする。Coq のループ所有権健全性証明はすでに導入済み、コミット 7f673b4c。残るのはこの*実装統合*）。

進捗（2026-06-21、その後ツリーの健全性を保つため撤回）：リストリテラル構築器と `lower_owned_heap_field` の Tuple アームを `(String,Value)` タプルへ拡張すると（`DropListStrValue`/`str_value_elem_lists` は csv 由来ですでに存在）、`[(k, value.str(…))]` アキュムレータリテラルが実体化する。最小の `cm2`（リテラルキーの `(Value,Int)` TCO アキュムレータ）は壁 → **トラップ**へ移る。このトラップが核心であり、撤回した理由（②の規律 — 到達可能な誤コンパイルを出荷しない）：

- TCO は発動し（再帰が実際の `while` になる）、`pairs = pairs + [(k, value.str("x"))]` は追加型アキュムレータの Assign で低水準化される。しかしループ本体の `value.str(arg)` は String 引数を Value に移し、それがアキュムレータへ脱出する（Value → タプル → リスト → ループで持ち越される `pairs`）。それでも反復ごとの後始末（`drop_arm_locals`）はその String に依然 `rc_dec` を行い（`live_heap_handles` に残っている）、二重解放する（`rc_dec(v13)` トラップ）。これは **ループ本体からの脱出に伴う drop の収支バグ**：ループアキュムレータへ脱出する値コンストラクタ引数が、反復ごとのフレームから Consume されていない。（`sv1`/`sv2` — 同じ連結/タプルをループ外で行う — はリークなしなので、問題は特にループ脱出の相互作用にある。）
- 別に、実際の `collect_map`（キーはリテラルでなく `list.get(lines, pos) ?? ""`）はまだ壁に当たる。これは第 2 の問題（`??` をキーにする要素 / `map_entry` の相互インライン化による自己再帰形への変換）。

根本原因（2026-06-21 にリスクを完全に切り分け済み）：`tco_empty_for`（lower/mod.rs:2952、結果アキュムレータの初期空値）は `String`/`List` しか扱わない。`(Value, Int)` タプルでは `None` を返すため、TCO はループ内の結果アキュムレータ経路（csv parse の修正 646aa233）の適用を辞退し、旧来のループ後ディスパッチへ戻る。これはループ後に古い値を読みながら基底 `(value.object(pairs), pos)` を再計算する → `pos` は開始時の値（3 でなく `p=0`）を読む。したがって修正はタプル結果にも結果アキュムレータ経路を使えるようにすること。確定した 5 段階の計画（この計画から新たに実装し、各段階にゲートをかける）：

1. **`tco_empty_for` → Value + Tuple。** `Value → value.null()`（正しい空の Value）。スカラー → `0`/`0.0`/`false`。`Tuple[a,b] → (tco_empty_for(a), tco_empty_for(b))` を再帰的に適用。これで `(Value,Int)` 結果がループ内の結果アキュムレータへ進む（古い値を使うループ後ディスパッチを避ける）。
2. **タプル結果アキュムレータの drop。** アキュムレータスロットは `(Value, Int)` タプルを保持する。基底の再代入ごと（初期空値も含む）に、新しい `Op::DropTuple` 形式の演算で古い値を drop する（スロット 0 の `Value` を `$__drop_value` で解放し、スロット 1 のスカラーには何もせず、その後タプルブロックを解放）。既存の `record_masks`/スロットごとの再帰 drop を、Value スロットを持つ 2 スロットのタプルへ一般化する。（実行ごとに到達する基底は 1 個だけなので空値を 1 回 drop するが、複数基底のパーサーでも正しくなければならない。）
3. **`(String,Value)` / `(String,String)` のリストリテラル実体化を再導入する**（撤回した `try_lower_record_list_literal` の StrValue と `lower_owned_heap_field` の Tuple アーム）。これで `pairs +
   [(k, value.str(...))]` アキュムレータリテラルを構築できる。単独では健全（sv1/sv2 で検証済み）。
4. **ループ本体からの脱出の Consume。** ループ本体では `value.str(arg)` が `arg` をコピーする（ランタイムの `string.repeat(s,1)`）。そのため `arg` は呼び出し元の所有のままで、反復ごとの後始末で正しく `rc_dec` される。しかしトラップ（`rc_dec(v13)`）は 2 回解放されていることを示す。二重の `rc_dec` が反復ごとのフレームと古い持ち越し値によるのか、引数がアキュムレータのコピーと誤って共有されるのかを特定する。（段階 1-3 を入れた新しい `cm2.wat` から再導出する。以前のトラップは結果アキュムレータ経路の前だったため、段階 1 が正しく経路選択すれば消える可能性がある。）
5. **実際の `collect_map` の相互インライン化。** `inline_mutual_tail_recursion` は `map_entry`（および `after_colon` 連鎖の同階層の末尾）を `collect_map` にインライン化し、一つの入れ子階層内で純粋な自己再帰にしなければならない（`parse_nested` のより深い呼び出しは、有界の通常呼び出しのまま）。その後、段階 1-4 が適用される。`??` をキーにする要素（`list.get(lines,pos) ?? ""`）もループ本体で低水準化できなければならない。

その後：各段階で `parse`/`stringify` のバイト一致監査 + 10⁴ リークループ + corpus-wall + mir スイート。
ゲート上の注意：段階 1 だけで `cm2` が直るかもしれない（結果アキュムレータへ進むため）。段階 2/4 が必要と決めつける前に検証する。Coq のループ所有権健全性（選択肢 C、7f673b4c）が全体を支える。これは抽出/低水準化の統合だけである。

<a id="corrected-diagnosis-2026-06-21-sharper--supersedes-the-all-tuple-results-broken-framing"></a>

### 診断の訂正（2026-06-21、より精密に —「すべてのタプル結果が壊れている」という捉え方を置き換える）

区別のための 2 つのプローブで、バグを正確に絞り込んだ：
- `cm4`（`List[(String,Value)]` を返し、`acc + [(k, value.str(…))]` を蓄積する自己再帰パーサー）は **v0 とバイト一致**（長さ 3）。したがって str_value リストリテラルの実体化、追加型アキュムレータ TCO、`value.str` のコピーされた引数はすべて健全。（段階 3 は正しい。安心して再導入できる。）
- `cm2`（同じだが `(Value, Int)` タプル `(value.object(pairs), pos)` を返す）は不正な値（`pos=0`、Value も不正）と後始末時のトラップを生む。
- 既存の `parse_rows_rec` テスト（csv、タプルを返す自己再帰パーサー）は通る。**つまりタプル結果 TCO が全面的に壊れているわけではない。** 違い：`parse_rows_rec` の基底はループ本体ローカル（`let
  (field,np)=…`）を読む → `base_reads_loop_local=TRUE` → ループ内の結果アキュムレータ経路（動作する）。`cm2` / `collect_map` / `collect_seq` の基底は持ち越しパラメータ（`pairs`、`pos`）だけを読む → `base_reads_loop_local=FALSE` → ループ後ディスパッチがタプル基底を再計算して古い値を読む → 不正な値。

真のバグは **ループ後ディスパッチによるタプル基底の再計算**（`cm4` の `acc` のような単純な Var 基底とは異なり、ループ最後の持ち越しパラメータ値を参照できない）。「タプル結果の適用を辞退する」という広いガードは誤りで、`parse_rows_rec` を退行させる（検証済み：ガードを入れるとそのテストが失敗）。

修正案の改訂（より限定的）：持ち越しパラメータだけを読むタプル基底をループ内の結果アキュムレータへ進める（タプル結果でも `result_var` に振り分ける。`tco_empty_for(Value/Tuple)` と `(Value,Int)` タプルの drop が必要）。または、ループ後ディスパッチがループ最後の持ち越しローカルを読むタプル基底を低水準化するよう直す。いずれにせよ `parse_rows_rec`（ループローカル基底）は動き続けなければならない。最小再現例は `cm2`。`cm2`（正しくなること）と `parse_rows_rec`（正しいままであること）の両方でゲートする。

<a id="csv-full-conquest-status-4-public-fns-v0-vs-v1-byte-match-audit"></a>

## csv の完全攻略状況（公開関数 4 個、v0 対 v1 のバイト一致監査）

`almide/csv` の各公開関数を端から端まで監査した（ソースをインライン化し、v0 の `almide run` と v1 の render_program+wasmtime を比較）。**✅ 完全攻略 — 4 個中 4 個がバイト一致**（検証済み：バイト一致 + 100000× リークループで問題なし + corpus-wall ACCEPT + mir スイート）。別々の 3 機構を根本から修正：
- ✅ **stringify**（コミット 6fb48108 — 非キャプチャの heap-map のインライン化が必要だった。lift 経路で入れ子 map が `,` を返す無言の誤コンパイルを解消）。
- ✅ **stringify_records**（コミット b129ad45 — キャプチャする heap-map / Value に対する map クロージャ）。
- ✅ **parse**（コミット 646aa233 — TCO 結果アキュムレータ修正：parse_rows_rec の `paf(…, cur+[field])` 基底はループ本体ローカル `field` を読む。今は、`field` が死んでいたループ後ディスパッチではなく、結果アキュムレータでループ内に計算する。核心の drop 配置バグを根本から修正）。
- ✅ **parse_records**（コミット fba2e960 + af8dcdf7 — parse のトラップに隠れていた入れ子ヒープリストの要素演算：`List[List[String]]` に対する `list.get(rows,0)` / `list.drop(rows,1)`。`_str` 版は `string.repeat` で内部リストを深くコピーするが、その長さワードをバイト数として読む。get では無言の誤コンパイル、drop では二重解放トラップとなる。新しいハンドル共有 `list.get_liststr` / `list.{take,drop}_liststr` + `option.liststr_unwrap_or`：各内部リストを rc_inc + 生の store64（`__ldls_share`、`__varr_copy` と同様に許可リスト入り）で共同所有し、最後の参照で 1 回解放する）。
- ❌ **parse_records** — map 機構（enumerate+map の融合とタプル要素 map、コミット a9aecee5）および parse_rows_rec の二重解放（TCO 修正、646aa233）は両方完了。残るのは TCO のトラップが隠していた、別に明らかになった第 3 の問題：**入れ子ヒープリスト `List[List[String]]` に対する `list.get` / `list.drop`**（parse_rows が返す `rows`）。parse_records の本体を縮小して切り分けた（`va*` 再現例）：
  - `let header = list.get(rows, 0) ?? []` → **無言の誤コンパイル**（`va2`：v0 `[2]`、v1 `[0]`）。ディスパッチ（lower/mod.rs:2354）はすべてのヒープ要素を `list.get_str` に送り、`string.repeat` で要素を深くコピーする。葉の `String` 要素なら正しいが、複合ヒープ要素（`List[String]`、`List[_]`）はハンドルで共有し（:2350 の `is_value_ty` → `list.{f}_value` 分岐のように）、実際の drop 型で追跡しなければならない（`List[String]` → DropListStr。`_value` の DropValue ではない）。必要なのは複合ヒープ用アクセサと要素 drop の追跡であり、ディスパッチの 1 行だけではない。
  - `let data = list.drop(rows, 1)` → **トラップ**（`va3`）。部分リスト演算は `List[List[String]]` のヒープ要素に対応していない（移動された要素ハンドルの rc の扱いが誤っている）。
  したがって parse_records → 4/4 には、入れ子ヒープリストの要素アクセサ（`list.get`/`first`/`last` のハンドル共有 + 複合要素の drop 型追跡）と、ヒープ要素対応の `list.drop` が必要。`va2` の無言の誤コンパイルは②の規律の対象（`_str` への誤ディスパッチ）。最低限の健全な対応は、アクセサが導入されるまで複合ヒープ要素の `list.get` を壁で止めること（無言の `_str` 深いコピーではなく）。
- ❌ **parse**（および parse_records の残り）— 共通の `parse_rows_rec`/`parse_after_field` 二重解放（prr↔paf 相互再帰で rc_dec → `unreachable`）。原因は相互作用であり、3 つの最小再現例で絞り込んだ（以前の単一原因の推測を率直に訂正する）：
  - **dd**（`let (field,np)=pf(…)` を持つ直接自己再帰 `prr(…, cur + [field])`、paf なし）→ ✅ 動作する。したがって分解束縛 + `cur + [field]` だけなら健全（以前の「分解束縛がフィールド使用前にタプルを drop する」は誤り。dd はまさにその形で問題ない）。
  - **B**（prr↔paf の相互再帰、分解束縛なし）→ 壁に当たる（部分集合の外であり、トラップではない）。
  - **prrep**（両方：prr↔paf 相互再帰 + `let (field,np)=pf(…)` 分解束縛 + `cur +
    [field]`）→ トラップ（二重解放）。
  最終的な誘因（回帰テスト `parse_rows_rec_destructure_mutual_recursion_double_free`、#[ignore]）：正確な組み合わせは **自己再帰する兄弟アーム + 入れ子 else 内の分解束縛**。prr 内側の `if c == "," then prr(…) else { let (field,np)=pf(…); paf(…, cur+[field]) }` では THEN が自己再帰（`prr(…)`）、ELSE は所有タプルを分解束縛して借用された `field` を使う。所有する pf タプルが `cur+[field]` の連結で読まれる前に drop される（String スロットに rc_dec。wat：479 の drop < 551 の連結）→ 解放済み String をコピー → 二重解放。確認済みの誘因：`ns` 再現例は同じコードだが、内側 THEN が自己再帰 `prr(…)` でなく `paf(…)` であり、トラップしない（prr 全体が実行可能な heap-result-if として低水準化される）。つまり、自己再帰する兄弟アームが分解束縛の else を drop 配置を誤る経路へ押し込む。dd（平坦な `else { destructure;
  prr(…) }`、自己再帰だが入れ子 if なし）も問題ない（347 の連結の後、374 で drop）。フィールドを Dup しても直らない（Dup も同じ誤った地点で drop される）。分解束縛 × 自己再帰の兄弟アーム × 入れ子 if の相互作用に対する、範囲は限定的だが深い drop 配置修正であり、回帰リスクが高い。map 機構は完了（a9aecee5）。parse と parse_records の両方がここだけでトラップする。corpus-wall には含まれない（csv ∉ corpus）。

<a id="the-layout-brick-read-side--open-commits-5b7efec7-e43db65f"></a>

## レイアウト部品の読み出し側 — 開通（コミット 5b7efec7、e43db65f）

heap-Result-of-X の読み出し/`??` が、Value サブシステムで指摘されたレイアウト部品。今は Result 族全体で導入済み。各々 v0 とバイト一致、リークループに問題なし、corpus-wall ACCEPT、スイート成功：
- **value.get** → `Result[Value,String]`（セルフホスト：`__vobj_find` の線形走査で添字を求め、非再帰の `Ok(@12 borrow)`/`Err("missing field '<k>'")` で包む）。`match` が tag@16 を読み、@12 の Value を束縛する（`value_result_results` と分類 → 再帰的 `DropResultValue`）。以前は ok:0|err:0 に化けていた。
- **value.as_array** → `Result[List[Value],String]`。**value.as_string** → `Result[String,String]`。
- `??` は各 Ok ペイロード種別を、動作する match 読み出しを再利用したセルフホストのヘルパーへ送る：`result.value_unwrap_or` / `result.list_value_unwrap_or` / `result.str_unwrap_or`（Ok アームは @12 を Dup。厳密な `is_result_str_str_ty` が str ヘルパーと result.zip のタプル Ok を区別）。count_ir_calls が合成呼び出しを計上するので mir==ir。ハンドルの else-if と Var ケースが Value/List オペランドを許可する。

- ✅ **Option-of-Value の読み出し** — 完了（コミット cab0924b）。List[Value] の `list.get` はセルフホストの `list.get_value` へ送られる（要素を `string.repeat` でコピーして Object を `{}` に壊していた `_str` 版ではない）。`Some(@i)` で要素を共有する（`Some(Value)` コンストラクタは value.get の Ok と同様、借用された Value を Dup）。`??` は Option[Value] を prim ベースの `option.value_unwrap_or` に送る（値を返す match の Some アームの scalar_bind はヒープペイロードを拒否するため、ヘルパーは len-tag@4 + @12 を直接読む）。さらにリーク修正：`value.as_array ?? []` オペランドは内部リストを所有する → materialized_call_arg 内で value_result_lists（再帰的 `DropResultListValue`）に再分類（平坦な drop は要素の Value をリークさせ、ループで OOM になった）。Result[Value,String] の Ok は平坦なまま（共同所有）。Object 要素も含むバイト一致、2000x リークなし、corpus ACCEPT を検証済み。

- ✅ **Value に対する map クロージャ** — 完了（コミット b129ad45/264976e8）。csv の **stringify_records が v0 とバイト一致**（CSV の引用 `LA, CA` → `"LA, CA"` を含む）。C1 インライン `list.map` を、入力と結果の両方がヒープ要素である場合へ拡張した。List[Value]/List[String] に対するキャプチャ付きクロージャ（lift 可能な環境なし）は専用ループとしてインライン化する。要素は `LoadHandle` で読む（本体が使う借用 i32、例：`value.get(row, h)`）。要素ごとの本体を `lower_heap_result_arm` で低水準化する（一般的なヒープ式：呼び出し / 連結 / 入れ子 `list.map … list.join` / `value.as_string ?? ""` を unwrap ヘルパーへ送る新しい `??` アーム）。新たに所有された結果は Handle 拡張され、`heap_elem_lists` で追跡される DynListStr スロットへ格納される（再帰 drop）。入れ子のセル射影（`header` をキャプチャする rows 上の外側 map、`row` をキャプチャする header 上の内側 map）は両階層ともインライン化する。キャプチャ付きクロージャと String 要素の結果に限定するゲートを設ける（非キャプチャの heap map は検証済み `list.map_str` の lift 経路を維持）。検証済み：Value に対する map クロージャの全形状がバイト一致、2000x リークなし、corpus-wall ACCEPT（ヒープオブジェクト 16514、caps-transitive）、スイート 487/0、diff-fuzz 成功。**レイアウト部品による突破口全体が完了：heap-Result/Option の読み出し族 + Option[Value] + Value に対する map クロージャで csv stringify_records が実現する。**


org-trust ダッシュボードの最多の壁理由（約 40、toml/svg/aes/base64/csv を阻む）は「heap-result-expr 族」（`heap-result if`/`match` …「空の遅延ヒープ値をムーブアウトしてしまう」）と表示される。csv を対象にしたことで（toml と違って動く v0 オラクルがある。後述）、真の原因が判明した。

<a id="finding-it-is-not-the-heap-result-arm-shapes--those-already-lower"></a>

## 発見：heap-result のアーム形状ではない — それらはすでに低水準化できる

`lower_heap_result_arm`（control.rs）はすでにタプル構築アーム、Named/Module 呼び出しアーム、連結アーム、入れ子 if/match、ブロック、Option/Result コンストラクタを扱う。csv の `heap-result if` の壁は一つの意図的なガードによる：**自己再帰呼び出しのアームを壁で止める**（control.rs 約 2162）：

```
if name.as_str() == self.fn_name { return None; }  // v1 has NO TCO → deep recursion traps
```

コード内コメントの訳：v1 には TCO がないため、深い再帰はトラップする。

csv のパーサーはすべて末尾自己再帰：`parse_unquoted_field(text, pos+1, acc+c)`、`parse_quoted_field`、`parse_rows_rec`、`parse_after_field`。それぞれ再帰するため、各 heap-result `if` が自己再帰ガードに当たる。したがって突破口は **自己再帰するヒープ結果パーサー関数の TCO** であり、アーム形状ではない。

<a id="what-tco-already-covers-try_tco_rewrite-modrs2734-vs-the-gap"></a>

## TCO がすでに扱う範囲（`try_tco_rewrite`、mod.rs:2734）と不足部分

対応済み：(1) リストイテレータの前向き走査（`list.drop(cs,1)` を持ち越す）、(2) 追加型アキュムレータ（`acc + [x]`、`ConcatList`）→ 所有されたループ持ち越しスロット（選択肢 C、証明書 `check_cert_lc`）。yaml（バイト検証済み）が低水準化できるのは、パーサーがこの形に当てはまるため。

不足部分（csv/toml のパーサーコンビネータ形）：
- **String アキュムレータ** `acc + c`（`ConcatList` ではなく `ConcatStr`）— 追加型アキュムレータを String スロットへ拡張する（反復ごとに古い値を drop して新しい値を alloc する点は同じ、証明書 `i(id)m`）。
- **タプル結果の基底** `(acc, pos)` — 基底が返すのは持ち越す型そのものではなく、アキュムレータとスカラーの位置を持つ `(String, Int)`。
- **複数アキュムレータ + タプル分解束縛を伴う自己呼び出し**（`parse_rows_rec`：`rows` と `current_row` を持ち越し、両方 `List`。`let (field, np)
  = parse_quoted_field(...)` があるとき、自己呼び出しの引数は `current_row + [field]`）。

<a id="plan-byte-match-first-csv-has-a-working-v0-oracle"></a>

## 計画（バイト一致を最優先。csv には動く v0 オラクルがある）

オラクル：`parse("a,b,c\n1,2,3\n\"x,y\",4,5\n")` → v0 native = `[["a","b","c"],["1","2","3"],["x,y","4","5"]]`（確認済み）。ドライバー = csv/src/mod.almd + `parse` を呼ぶ `effect fn main`（単一ファイル → render_program）。目標：v1 == この結果。

1. `try_tco_rewrite` の追加型アキュムレータを **ConcatStr (String) アキュムレータ**と最小のタプル結果基底へ拡張し、`parse_unquoted_field`/`parse_quoted_field` を解放する。ゲート：corpus-wall ACCEPT（ループ持ち越し証明書 `check_cert_lc`）+ String アキュムレータのリークループ + バイト一致。
2. 複数アキュムレータ + タプル分解束縛を伴う自己呼び出し — `parse_rows_rec`/`parse_after_field`。
3. その後 `parse`（`ok(value.array(...))` の ResultOk）と `parse_records`（`list.map` クロージャ）が連鎖的に低水準化される。csv → `[["a"…]]` とバイト一致。

各段階を corpus-wall ACCEPT（TCO は正しさやリークの問題を起こしやすい。ループ持ち越しスロットの証明書がゲート）と csv の v0==v1 バイト一致の両方でゲートする。この突破口は toml/svg/aes/base64（すべてパーサー形）の同じ問題群も解消する。

<a id="progress-commit-63a7a1a6--step-1-done--a-pre-existing-miscompile-fixed"></a>

## 進捗（コミット 63a7a1a6）— 段階 1 完了 + 既存の誤コンパイルを修正

ConcatStr アキュムレータを接続中、バイト一致の確認で既存の無言の誤コンパイル（②の最重大違反）が見つかった。TCO ループ本体は `{ if base then … else step }` なので、基底判定はブロック末尾の `if` として届く。その末尾が直接 `lower_branch` に落ちていた（cond を実行から除いて記録だけにし、両アームを実行）。`if done then {rk:=k} else {step}` が無条件の `rk:=k` になり、ループはちょうど 1 回だけ実行された。本体にヒープの `let c = peek(...)` を持つ再帰パーサーはすべて該当（v0 `hello`、v1 `h`）。**修正**：ブロック末尾の if/match をまず `try_lower_unit_if` に送る（実際の分岐）。実行できない場合だけ `lower_branch` に落とす。これで誤コンパイルを除去し、スカラー添字を使う追加型アキュムレータのパーサーループも実行される。

このコミットで完了：
- ✅ ブロック末尾の基底判定が分岐するようになった（1 回だけ実行する誤コンパイルを list と string の両方で修正）。
- ✅ ConcatStr (String) アキュムレータ + タプル結果基底 `(String, Int)` — `is_self_append` が ConcatStr に一致し、先頭のスロットコピーが String 対応になった（`acc + ""`）。リークループ検証済み（2000×）。
- ✅ corpus-wall ACCEPT（ownership 16303）、diff-fuzz 成功、4 個の `*_loop_reclaims` テストは引き続き成功、新しい wasmtime cargo テスト（`string_accumulator_parser_tco_executes_on_wasmtime`）。

<a id="progress-commit-1d8bdd92--step-2-partial-multi-accumulator-reset--cross-read"></a>

## 進捗（コミット 1d8bdd92）— 段階 2 の一部：複数アキュムレータのリセット + 相互読み出し

複数アキュムレータの不足を 4 個の下位問題へ分解した（それぞれ最小再現例あり）。2 個は完了：
- ✅ ヒープアキュムレータを新しい空値へ **リセット**（`cur = []` / `acc = ""`）— ループ持ち越しスロットの更新として許可（区切り文字の後、パーサーは現在行のアキュムレータをリセットする）。
- ✅ **ヒープアキュムレータが別のヒープアキュムレータを読む**（`cur = ""` と同時に `out = out + cur`）— 反復ごとのヒープ代入を、読み出し依存のトポロジカル順（読む側が読まれる側より前）で出力する。循環がある場合だけ壁に当たる。String アキュムレータを 2 個持つパーサーが v0 とバイト一致するようになった（リークループ検証済み、cargo テスト `multi_accumulator_reset_and_cross_read_tco_executes_on_wasmtime`）。

完了（コミット cd8ad5e6）：✅ **スカラー変数のリストリテラル** `[pos]` — `lower_call_args` が `try_lower_scalar_list_construct` で実体化する（平坦な `DynList` + store64）。

完了（コミット fc4d8425）— 大物：✅ **入れ子ヒープ要素リスト** `List[List[String]]`。新しい `Op::DropListListStr` が入れ子の wasm ループを描画する（各行のセル、各行、その後外側ブロックを解放）。`try_lower_concat_list` は `List[String]` 要素を許可する（`rows + [cur]`、`__list_concat_rc`）。`try_lower_str_list_literal` が `[cur]` の単一要素リストを構築し、ループ内代入がリセット（`cur = []`）を扱う。この型のすべての値は新しい `list_list_str_lists` 集合へ送られる（`is_list_list_str_ty` により、`is_heap_elem_list_ty` より前に検査）。これにより drop は入れ子版になる。最初はリークループで OOM トラップ（呼び出し結果の一時値が平坦な drop に送られた）→ 全追跡箇所で経路を修正。csvcore は v0 とバイト一致、2000× リークループで問題なし、corpus-wall ACCEPT、csv 分類 **5/6 → 7/4**（parse_rows_rec + parse_after_field が低水準化可能になった）。

完了（コミット b871b73d）：✅ **`[]` の heap-result-if アーム** — `lower_heap_result_arm` が空リストのアームを実体化する（`if is_empty(t) then [] else parse_rows_rec(...)`）。csv 7/4 → 8/3（parse_rows を低水準化できる）。

発見（プローブ）：ダッシュボードが示唆した **`list.map` クロージャ**の突破口は、大半がすでに完了している。スカラー / String / Value / ブロック本体 / 入れ子 map / map|>join のクロージャはすべてバイト一致。実際に残る csv の壁はもっと限定的（特定の値構築）で、クロージャ全般の不足ではない。

完了（コミット 47301322）：✅ **`Result[Value, String]` の ok/err ラッパー**（csv `parse` の `ok(value.array(...))`）。新しい `Op::DropResultValue` → セルフホストの `$__drop_result_value`（タグで振り分け：Ok → `$__drop_value`、Err → `rc_dec`）。`try_lower_result_value_ctor`（lower_tail と if アーム内）が `lower_owned_heap_field` で Ok を実体化し（`value.*` と入れ子の `list.map` を扱う）、新しい `value_result_results` 集合へ送る（`is_value_result_ty`）。ok/err + match 読み出しの往復がバイト一致。corpus-wall ACCEPT、v1 で 2000× OOM なし。**csv 分類 8/3 → 11/2（parse を低水準化できる）。**

<a id="valuejson-subsystem-the-csv-records-dependency--phase-1-done"></a>

## Value/JSON サブシステム（csv-records の依存先）— 第 1 段階完了

残る csv の 2 関数には、動的 Value の Object と JSON 演算のセルフホストが必要（v1 の信頼基盤にはどれもなかった）。複数関数からなるサブシステムを分解：

完了（コミット a0dcf31d）：✅ **value.object + json.keys**（tag-6 の Object）。ペアごとに 2 スロットのブロック（キー String + 値 Value、rc_inc して入れる）。`__drop_value` の新しい tag-6 アーム経由で再帰的な `__vdrop_obj` を行う。発見した重要なバグ：`@8` はスロット数（2·pairs = フリーリストが回収する割り当てサイズ）を保持しなければならない。ここにペア数を入れると反復ごとに 2 スロットをリークした（2 ペアでの OOM をリークループが検出）。往復は v0 とバイト一致、2000× 複数ペアのリークループで問題なし、corpus-wall ACCEPT。

まだ未完了（より深い Value の構成要素。それぞれ実際の下位問題であり、簡単な追加ではない）：
- ❌ **value.get** — 走査（スカラー自己再帰 → 添字）と Ok/Err ラップは作成したが、(a) 自己再帰する `Result[Value,String]` 本体が不正な wasm を生成する（TCO×Result の相互作用）、(b) Value-Ok Result の `?? value.null()` による unwrap と `match ok(v)` の読み出しは誤束縛 / 壁になる（Value-Ok Result の unwrap_or と match 読み出しが未接続。String-Ok だけが接続済み）、(c) v0 の Err メッセージは固定文字列でなく `missing field
  '<k>'`（バイト一致には補間が必要）。撤回済み（壊れたコミットにせず、未構築のまま保持）。
- ✅ **value.stringify** — 完了（コミット 85e05369）。完全な再帰 JSON シリアライザーを value_core でセルフホストし、v0 とバイト単位で同一（null/bool/int/float/引用符付きエスケープ文字列/`[…]`/`{…}`）。配列/オブジェクトの再帰は String アキュムレータであり、区切り文字にスカラー if の `k` を使った `string.repeat(",", k)` を用いる。これで初回の試行を阻んだループ本体内の heap-result-if を回避する（本当の不足 = スカラー while 本体内の `let sep = if i==0 then "" else ","`。回避策に TCO 拡張は不要）。実現に必要だった 2 修正：(1) 自動リンク（render_program + lower_source）が内部 impl 名の呼び出しを改名後の call_name に書き換えるようになった（したがって `value_stringify` 経由で再帰する `__vstr_arr` は、改名された `value.stringify` 定義へ解決する）。(2) Module 呼び出し引数に使う `prim.load_str` の借用を、スコープ末尾 drop なしで Handle として渡す（drop すると Str Value の tag-4 ペイロードを二重解放した）。加えて既存問題の修正：Value-drop 演算があれば value_core を強制リンクするようになった（json.* で構築した Value の `__drop_value` が宙に浮いていた。json_scalar/json_string を修正）。7 種の値すべて + 入れ子 + エスケープが v0 とバイト一致、2000x リークループで問題なし、corpus-wall ACCEPT、スイート 484/0。注意：ループ本体内の heap-result-if は依然として一般的な不足（TCO 拡張、経路 (a)）。ここでは回避しただけで、解消はしていない。
- ❌ **parse_records のクロージャ** — `data |> list.map((row) => { … nested list.map → (key, value.str) …
  value.object(pairs) })`：`List[(String,Value)]` を構築してから Value オブジェクトを作るブロック本体クロージャ。value.object が存在するようになったこととは独立に、クロージャが壁に当たる（「unliftable closure」）。

<a id="valueget--stringify_records--blocked-by-the-layout-brick-heap-result-of-valuelist"></a>

## value.get + stringify_records — レイアウト部品（heap-Result-of-Value/List）が阻む

value.get を試みて撤回した（壁と値化け。壊れたまま出荷していない）。正確な発見：
- v0 の仕様：`value.get(o,k)` → `Ok(v)` / `Err("missing field '<k>'")`（補間メッセージ）。stringify_records では `value.get(row,h) ?? value.null()` として使う。
- value.get のコンストラクタの壁：本体 `if idx<0 then Err(msg) else { rc_inc; let val; Ok(val) }` は `Result[Value,String]` を返す heap-result `if` であり、「heap-result if outside the executable subset」の壁に当たる。`Ok(<rc_inc'd borrowed Value>)` + `Err(<concat/let message>)` のアームが `try_lower_result_value_ctor` の部分集合に入っていない（新しい `value.*` / list.map に対する Ok は扱うが、借用後に共同所有する Object スロットは扱わなかった）。
- value.get の読み出しで値化け：`match value.get(…){ ok(v)=>…, err(e)=>… }` は `ok:7|err:'missing field'` ではなく `ok:0|err:0` を返す。**Result[Value,String] の読み出し**（match + `??`）が未接続（`materialized_results_str` は `Result[String,_]` しか追跡せず、Value-Ok ペイロードの読み出しで誤束縛する）。
- stringify_records の最初の行は `value.as_array(v) ?? []` = `Result[List[Value],String]` の unwrap。同じレイアウト部品の阻害要因（as_array の誤束縛、value_core 101-112 行）で、value.get より前に到達する。

つまり value.get と stringify_records は両方ともレイアウト部品が必要：ペイロードに正確に対応するヒープペイロード束縛 + heap-Result-of-Value/List の構築と読み出し（match/`??`）。文書化された本格的な機構部品であり、簡単な追加ではない。（Value の構築と借用読み出しだけが必要な value.object/json.keys/value.stringify はすべて導入済み。不足は特に heap-Result-of-Value/List の往復にある。）

まだ壁に当たるもの（csv の完全なバイト一致に残る 2 個、どちらも value.object を構築するクロージャ）：
- ❌ `parse_records` — `data |> list.map((row) => { … value.object(pairs) … })`：`header`/`row` から `value.object` を構築するブロック本体クロージャ（`list.zip`/pairs の形）。`value.array(value.str)` の map より複雑な Value 構築。
- ❌ `stringify_records` — アームが `json.keys` + `value.object` + Value オブジェクトに対する入れ子 `list.map(... ) |> list.join` を使う heap-result `if`。
注意：バイト一致のドライバーにも `value.stringify` のセルフホストが必要（現在は未リンク）。低水準化とは直交する、別の標準ライブラリランタイムの突破口。

<a id="separate-blocker-tomls-v0-oracle-is-broken"></a>

## 別の阻害要因：toml の v0 オラクルが壊れている

toml が最初の対象候補だったが、`almide run`（native v0）は不正な Rust を出力する（`error[E0308]: expected String, found &str`、2×）。そのため v0 の Rust コード生成バグが直るまで、toml にはバイト一致のオラクルがない。代わりに csv を選んだ（v0 テストが通る）。toml の v0 バグは、toml をバイト検証できるようにする前に別途直すべき v0 バックエンドの問題。

<a id="status-2026-06-22--the-5-brick-tco-works-in-isolation-blocked-on-the-corpus-caps-gate"></a>

## 状況（2026-06-22）— 5 段階の TCO は単独で動作。コーパスの caps ゲートが阻む

5 段階すべての effect 関数タプル結果 TCO を実装し、単独では動作を証明したが、その後撤回した（②の規律 — corpus-wall に違反した）。動作するもの（検証後、ツリーの健全性を保つため撤回）：
- `cm2`（effect 関数 `(Value,Int)` の自己再帰パーサー、`pairs + [(k, value.str(x))]` アキュムレータ）→ **v0 とバイト一致** + `cm2leak` 10000× **リークなし**。
- `ftup`（タプルを返す effect 関数 + `let (v,p)=f()!` 分解束縛）→ ✅。`cm2p`（純粋）✅。`cm4`（List 結果）✅。`parse_rows_rec`（csv、List タプル）✅（退行なし）。mir スイート成功。
- **yaml モジュール → 低水準化の壁 0**（collect_map/collect_seq を低水準化）。

5 段階（すべてまとめて必要）：
1. `tco_empty_for` → Value（`value.null()`）/ スカラー（`0`）/ Tuple（再帰的）— 結果アキュムレータの空値。
2. **Value を含むタプル**結果に対する `result_var` の経路選択（`use_result_acc = base_reads_loop_local
   || tuple-with-Value`）。基底内に `value.object(..)` の呼び出しがあると、ループ後ディスパッチはタプル基底の兄弟スカラー持ち越し値を古い状態で読む。（csv `pf` の `(acc,pos)` のような Value を含まないタプルは、この経路に送ってはいけない。ディスパッチで動作しており、経路を変えると parse_rows_rec が退行する。）
3. `lower_owned_heap_field` 内の `(Value,scalar)` / `(String,Value)` タプル（try_lower_tuple_construct 経由）。
4. `__drop_value_tuple`（value_core）— 単一の `(Value,Int)` タプルの再帰 drop（@12 の Value スロットに DropValue + ブロック）。`variant_drop_handles="value_tuple"` 経由で振り分ける（平坦な record_masks では Value ペイロードがリーク → 10⁴ 回で OOM）。
5. 分解束縛のシード（`lower_destructure`）：`value.kind = Unwrap|Call` の場合、`value.ty` ではなくパターンからヒープスロットマスクを導く（effect 関数の `f()!` / never-err 除去済み `f()`。その `.ty` は Ty::Tuple ではなく effect の Result）。そうしないと分解束縛がコンテナ単位になる（`p` は 0 を読む）。Tuple 値には value.ty 経路を、単純な Var にはシードなしのコンテナ単位を維持する（`tuple_destructure_aliases_components` 単体テスト）。

**撤回した理由 — コーパスの caps ゲート（残作業、リスク切り分け済み）：**
- **mir > ir の caps 違反（1 関数）**：`tco_empty_for` の `value.null()` は TCO が挿入する合成呼び出しであり IR にはない。したがって `count_ir_calls`（IR）< mir の CallFn 数 → `mir<=ir` ゲートが破れる（[[project_v1_gate_count_vs_lower]] の問題群）。修正：既存の TCO 合成演算を計上するのと同様、`count_ir_calls`（またはゲート）が TCO の合成 `value.null` を計上するようにする（両方より前に脱糖するか、空値初期化呼び出しを数える）。
- 変更により **コーパスの関数がパニックする（全域性違反）**。特定し（corpus-wall の caps 段階で名前が表示される）、その形に対して新しいタプル経路を堅牢化する（おそらく (Value,scalar) アームまたはシードが誤処理するタプル/集約の形。パニックせず適切にゲートで除外する）。
- 別に、yaml 全体の `parse` にはまだ **float.parse / list.enumerate / string.to_lower** のセルフホストが必要（parse 経路が参照する標準ライブラリの不足。TCO とは独立）。

次のセッション（確定）：5 段階を再適用し（この節が正確な手順）、corpus-wall を再テストする前にコーパスゲートの 2 問題（合成呼び出し数 + パニック）を直す。その後、標準ライブラリの 3 不足、yaml `almide test` へ進む。毎段階で cm2（必ずバイト一致）+ parse_rows_rec + tuple_destructure + corpus-wall（4 ACCEPT）でゲートする。

<a id="status-2026-06-22-later--mirir-breach-solved-better-than-planned-suite-breaking-recursion-fixed"></a>

## 状況（2026-06-22、その後）— mir>ir 違反を解決（計画より良い方法）。スイートを壊す再帰も修正

基盤となる 2 コミットを develop-v1 に導入した（ゲート：mir スイート 501/0、単一スレッドと並列、corpus-wall 4/4 ACCEPT）：

1. **mir>ir の caps 違反を解消 — ゲート変更ではなく `value.null()` をインライン化**（コミット 6ca50e85、`lower/calls.rs`）。`count_ir_calls` に合成呼び出しの計上を教える代わりに（それでは低水準化依存のロジックを信頼ゲートへ加えることになる。守る系 KPI が禁じる [[project_v1_gate_count_vs_lower]] のアンチパターン）、`value.null()` を String/List の空値とまったく同じく、tag-0 の Value ブロック（Alloc + store32）へインライン低水準化するようにした。つまり CallFn ではない。TCO の合成空値は mir 呼び出しを 0 個追加する（ゲートに中立）。明示的な `value.null()` ソースノードは引き続き IR で数える（mir < ir、許容）。corpus-wall ACCEPT は不変。**ゲートに触らず、コーパスゲートの阻害要因 (a) を完全に取り除いた。** 段階 1 を再適用する際、`tco_empty_for` の `Value → value.null()` は今や追加コストなし。

2. **mir スイート全体をひそかに壊していた既存の無限再帰を修正**（コミット 4536f33c、`render_wasm/tests_part1.rs`）。`lower_source` テストヘルパーはセルフホストのソースを再帰的にリンクする。Value-drop の強制リンク（a40c1332）は、value_core 自身を低水準化しているときにも value_core に対して `any_called` を設定していた（その drop ヘルパーは DropValue 演算を出力する）。しかし `any_defined` は呼び出し名（`value.null`）だけに一致し、実装名（`value_null`）には一致しなかったため、value_core を永遠に再低水準化した → スタックオーバーフローでテストプロセスを中断（スイートと cargo-test ゲートの全作業を隠していた）。修正：`any_defined` が呼び出し名か実装名のどちらにも一致するようにした。`capturing_heap_map_over_value` テスト（およびスイート）が再び成功。注意：これは value.null のインライン化が原因ではない。インライン化を無効にしても再現する、独立したテストヘルパー基盤の問題。

yaml の残り：段階 1–5 を再適用（mir>ir の問題はもうない）、**コーパスのパニック**を特定して堅牢化（阻害要因 b、まだ未解決。corpus-wall の caps 段階で名前が表示される）、標準ライブラリの 3 不足（float.parse / list.enumerate / string.to_lower）、その後 yaml のバイト一致 + リークループ。

<a id="brick-re-application-probe-2026-06-22--bricks-12-drafted-then-reverted-sharper-blocker-map"></a>

### 段階の再適用プローブ（2026-06-22）— 1+2 を試作後、撤回（阻害要因をさらに精密化）

段階 1（`tco_empty_for` → Value/スカラー/Tuple）と 2（`tuple_with_value` を result_var へ振り分ける）を再適用し、`cm2` をプローブした。発見（3–5 なしでは退行するため、ツリーの健全性を保つべくすべて撤回）：

- **段階 1+2 だけでは corpus-wall のカバレッジが退行する**（ACCEPT/パニック 0 のままだが、数が減る：ownership 16556→16549、names 3850→3849）。以前はループ後ディスパッチで低水準化していた Value タプルを返すコーパス関数が、今は result_var に進んで壁に当たる（段階 3–5 のタプル構築/drop/分解束縛がないため）。したがって段階 1–5 は不可分であり、1+2 だけをコミットしてはいけない。
- **段階 1+2 後の `cm2` の本当の阻害要因はタプル構築ではない** — `try_lower_tuple_construct`（binds.rs:1121）はすでにヒープ型タプル要素を低水準化する（ヒープスロットごとに `lower_owned_heap_field` を呼び、`record_masks` を記録）。壁は `lower_while`（control.rs:3338、`body_reassigns_heap`）にある。TCO が生成する `while` の本体は `pairs = pairs + [("k", value.str("x"))]` を行う。これは `List[(String,Value)]` 要素に対するヒープ追加アキュムレータ。`try_lower_scalar_while`（control.rs:1940）が適用を辞退する（追加型アキュムレータ経路がまだ `(String,Value)` タプル要素を許可しないため）。結果、1 反復をモデル化するフォールバックが壁に当たる。**つまり段階 3 の実態は「`try_lower_scalar_while` のヒープ追加アキュムレータで (String,Value)/(Value,scalar) タプル要素を許可する」ことであり、単独の `try_lower_tuple_construct`（すでに動作）ではない。** それに合わせて段階 3 の範囲を修正する。
- `cm2`（`local effect fn cm2(n,pos,pairs)->(Value,Int)`）の v0 オラクル：`{"k":"x","k":"x","k":"x"}`、その後 `3`。v1 MIR 基盤だけでテストする（`examples/render_program` → WAT → wasmtime、または `lower_source`）。`almide run --target wasm` ではない（これは旧 almide-codegen パイプラインで、別のバックエンド。`List[(String,Value)]` 引数で、より前の ANF パスがパニックする。MIR 基盤とは無関係）。

<a id="status-2026-06-22-yaml--tco-wall-gone-yaml-lowers-to-14128-line-wat-1-stdlib--1-render-bug-left"></a>

## 状況（2026-06-22、yaml）— TCO の壁を解消。yaml は 14128 行の WAT へ低水準化、標準ライブラリ 1 件 + 描画バグ 1 件が残る

effect 関数のタプル結果 TCO（5 段階）が導入済みでゲート成功（コミット 141f6c09。cm2 バイト一致 + 10⁴ リークなし、corpus-wall 4/4 ACCEPT、パニック 0、mir スイート 501/0）。これと今セッションでセルフホストした標準ライブラリの葉関数 2 個により、**`almide/yaml` の `parse` が v1 MIR 基盤上で TCO の壁 0 で低水準化される**。ロードマップ全体が狙っていた構造上の阻害要因は解消した。

このセッションでセルフホストしたもの（コミット済み、単独でバイト一致、PURE_MODULES に分類）：
- **string.to_lower**（コミット、ASCII の大小文字変換）— `stdlib/string_to_lower.almd`。
- **float.parse**（コミット、符号/整数/.小数部/e 指数、正確な pow10 スケーリングを 1 回）— `stdlib/float_parse.almd`。タプルなし（単一 Int/Float のヘルパー）。yaml 文法では `s.trim().parse::<f64>()` とバイト一致。

yaml ドライバー = `mod.almd` + `parse("a: 1\nb: hello\nc:\n  - x\n  - y\n")` を呼ぶ `main`（`examples/render_program` → WAT → wasmtime でテスト。`almide run --target wasm` = 旧コード生成ではない）。上記の後、`render_program` が報告するのは `unlinked: list.enumerate` だけ。撤回済みの list.enumerate 試作を加えると壁 0（14128 行の WAT）に到達するが、実行時に失敗する。残るバグは 2 個：

1. **`list.enumerate`（最後の標準ライブラリ不足）。** (Int,String) タプルリストの**構築器**は動作する（`List[(Int,String)]` に対するタプルなしの `__enum_str_rec` 追加型アキュムレータ、`try_lower_concat_list`/`try_lower_str_list_literal`/`lower_owned_heap_field` に追加した `(Int,String)` 要素、`list_enumerate.almd` 内の再帰 drop `$__drop_list_int_str`、`enumerate→enumerate_str` のディスパッチ、rc_dec 許可リスト内の `__isdrop_list`、PURE_MODULES 内の `list_enumerate`。すべてゲート成功、corpus-wall 4/4、カバレッジ +50）。消費側が誤コンパイルするため撤回した。yaml は `lines |> list.enumerate |> list.find((e) => not is_blank(e.1))` を使う。`List[(Int,String)]` に対する `list.find` が、タプルの `.1` を読むクロージャを伴うと、C1 の脱関数化を受けない（find ∉ map/filter/fold の脱関数化集合）。そして `find_str` に誤ディスパッチする（各タプルを String として読む → 不正な値、`none`/`0:` 出力）。本当の残作業は、構築器ではなく、クロージャ付きヒープタプル要素に対する find（[[v1-selfhost-machinery]] Machinery-3 のクロージャ面）。構築器を再導入し（健全である）、それからクロージャ find を解決する。
2. **`seq_map` の WASM 型不一致（i64 対 i32）** が yaml 全体の低水準化でのみ現れた（`local effect fn seq_map -> (Value,Int)` = `parse_mapping(list.set(…))!` へのタプルを返す末尾呼び出し）。自己再帰ではない（コミット済み TCO ではない）。おそらく `list.set` / タプルを返す末尾呼び出し経路の描画バグ。seq_map 形の fixture で切り分ける。

注意 — yaml の v0 native オラクルは壊れている：現在の native Rust バックエンドは `find_colon` の借用を誤る（`find_colon_at(s.to_string())` E0308）。そのため `almide test`/`almide run` は v0 で yaml をコンパイルできない。yaml_test.almd の期待値をオラクルとして使う（または native の借用バグを直す）。v1 基盤に影響はない。

<a id="status-2026-06-22-late--listenumerate--closure-find-done-yaml-emits-valid-wasm--runs-the-parser"></a>

## 状況（2026-06-22、後半）— list.enumerate + クロージャ find 完了。yaml は有効な wasm を出力しパーサーを実行

ユーザーの「構築器を再導入してからクロージャ find を解決」は完了、コミット済み、ゲート成功：
- **`60deaf24` list.enumerate + クロージャ付き (Int,String) に対する find。** (Int,String) タプルリストの構築器（追加型アキュムレータ TCO + try_lower_concat_list / try_lower_str_list_literal / lower_owned_heap_field 内の (Int,String) 要素 + 再帰的な `$__drop_list_int_str`）、`list_find_int_str`（各タプルのハンドルをロードし、`e.1` を読む述語クロージャへ渡す）、ヒープペイロード `Option[(Int,String)]` の構築（`materialize_opt_int_str_some`、共同所有の Dup）+ 実体化の認識 + 値を返す match の `some((idx,line))` 分解束縛、そして `List[(Int,String)]` の呼び出し結果 drop の経路選択（`is_list_int_str_ty` → `variant_drop_handles`）。`enumerate |> find((e)=>…e.1…)` の形は v0 とバイト一致（`1:b`）+ **10⁴ リークなし**。corpus-wall 4/4 ACCEPT（カバレッジ 16620）、mir スイート 501/0。重要：yaml の match アームは Value 結果（`value.null()`/`v`）→ `try_lower_variant_value_match`（タプルペイロードの脱糖あり）であり、Unit 本体の `try_lower_variant_match` ではない。Unit 本体のテストは線形化されるが、それは yaml の形ではない。
- **`96e94de7` List[String] に対する list.set（`list.set_str`）。** 汎用 `list.set` の i64 val パラメータは String 要素（i32 ハンドル）と型が合わず → `seq_map` で不正な wasm。set_str は全スロットを rc コピーし（共同所有）、スロット i を置き換える（古い値を rc_dec、新しい値を共同所有）。結果は `DropListStr` で drop。v0 とバイト一致。

**yaml は今、有効な WASM（約 13.7k 行の WAT、壁 0）にコンパイルされ、パーサー全体を実行する** — `parse → parse_lines → dispatch → parse_mapping → collect_map → map_entry → after_colon → parse_inline
→ parse_scalar → scalar_typed → scalar_value → scalar_numeric → parse_number`。マップエントリの値で `parse_number` 内にトラップする（`rc_dec`、二重解放）。`parse("a: hello")` はトラップするが、単独のスカラー `parse("hello")` はトラップせず `{}` を返す。忠実な再現例でもトラップを再現できない（`match int.parse(c){ok=>value.int,err=>value.str(raw)}` の形、parse_number の値を使う collect_map の (String,Value) アキュムレータ、effect 関数 `(Value,Int)` `!` を通すループローカル `after`。すべて単独ではバイト一致）。したがって完全な数値解析連鎖にある微妙な相互作用。疑い（WAT から）：`Result[Int,String]` の drop（例：`int.from_hex` / `int.parse`）がペイロードスロットを要素ごとに `rc_dec` する。`Err(String)` なら問題ないが、`Ok(Int)` では二重解放 / スカラーをハンドルとして扱うことになる。または値がアキュムレータへ脱出するときに、借用された `raw`/`after` を二重 drop する。次：`parse("a: hello")` の WAT で `$parse_number` の正確にトラップする `rc_dec` ローカルをダンプし、元の drop まで追跡し、Result/借用の drop 経路を直す。その後 yaml が端から端まで動く（注意：v0-native オラクルはまだ `find_colon` の借用 E0308 で壊れているため、yaml_test.almd の期待値とバイト一致させる）。

<a id="status-2026-06-22-runs---almideyaml-runs-on-v1"></a>

## 状況（2026-06-22、動作）— 🎉 almide/yaml が V1 で動作

`almide/yaml` の `parse` は v1 MIR→WAT→wasmtime 基盤上で低水準化でき（壁 0）、バイト単位で正しく実行される。
確認した往復（parse → yaml 自身の stringify、入力と同一）：`a: 1\nb: hello\nc:\n  - x\n  - y`、`name: Alice\nport: 8080\npi: 3.14`、`items:\n  - one\n  - two\n  - three`、`nested:\n  a: 1\n  b: 2\nflag: true`。
「almide v1 で almide/yaml を動かす」という目標を達成。この push のコミット（すべてゲート成功、corpus-wall 4/4、mir スイート 501/0）：
- `60deaf24` list.enumerate + クロージャ付き (Int,String) に対する find（ヒープペイロード Option の機構）。
- `96e94de7` list.set_str（List[String] の set。i64 の val を取る汎用 list.set は不正な wasm を生成していた）。
- `6f2128f3` heap-result-if の **Match アーム**ごとのフレーム（対象評価の一時値 `string.drop(c,2)` が関数スコープ末尾まで漏れ、別のアームが実行されると無条件の `rc_dec(0)` トラップとなった。parse_number の 0x クラッシュ）。加えて `drop_arm_locals` のクランプ。
- `59a5164a` ヒープ Value の 2 リーク：(1) ヒープ結果の末尾 `f(string.replace(s,..),s)->Value` はスコープ末尾の drop を迂回し、所有する一時引数をリークしていた。CallFn 後に解放する。(2) let 束縛された Named 呼び出しの Value 結果には `value_handles`/DropValue の印がなかった（Module 経路にはあった）。

残り — リークの裾野（本番向けの堅牢化であり、「動作」の問題ではない）：解析を繰り返すと最終的に OOM になる（マップ入力は約 1000× まで問題ないが、シーケンス経路 — collect_seq/seq_item/dash_item/dash_after — ではより早くリークする）。OOM は次の alloc の場所で表面化する（修正が入るにつれ parse_number → find_colon_at）。したがって単一箇所ではなく、解析ごとに解放漏れが蓄積している。パターン：これまで見つかったリークはすべて、heap-result の末尾/アームが関数スコープ末尾の drop を迂回するために解放されないヒープ値（一時値 / let ローカル / 呼び出し結果）。seq 経路（dash_after の `(parse_inline(after), pos+1)` タプル + collect_seq の List[Value] アキュムレータ + `string.get(s,pos) ?? ""` 由来の `??` オペランドの Option 一時値）で同じ問題群を監査する。単発/少数回の解析は正しく、収支も合っている。これはループの反復深度に対する堅牢化。（v0-native オラクルは依然 `find_colon` の借用 E0308 で壊れている。往復 + yaml_test の期待値を使う。）

<a id="status-2026-06-22-base64tomlaes--aes-conquered-base64-94-the-two-deep-frontiers"></a>

## 状況（2026-06-22、base64/toml/aes）— aes 攻略、base64 9→4、二つの深い未解決領域

この push はリポジトリ固有の 3 つの壁（base64 / toml / aes）を対象とした。導入済み、すべてゲート成功（corpus-wall 4/4 ACCEPT、mir 501/0）。いずれも再利用可能な機構：

- ✅ **aes が動作**（FIPS 197 の `encrypt_block` が v0 とバイト一致）。突破口 = **定数のモジュールグローバル実体化**。const 初期化子を持つヒープのモジュールレベルグローバル（`let SBOX = bytes.from_list
  ([…])`、文字列リテラル、整数リストリテラル）は、今は直接 `Alloc` へ低水準化される。新しい `Init::Bytes`（任意バイトのブロック。S-box は 0x00–0xFF を持つため `Init::Str` とは異なる）。計算する初期化子は引き続き壁に当たる（実体化するとゲートの IR 側 `count_ir_calls` に見えない `CallFn` を挿入してしまう → mir>ir）。`global_inits` を render/gate/cert に通し、3 者を一致させる。（コミット ac7fae5c）
- ✅ base64 `char_to_val` — ヒープ結果 `Result` の **メッセージが `${}` 補間である Err アーム**（`err("bad '${ch}'")`）。ResultErr アームが補間を畳み込み、中間値をアームごとに解放する。（fec44e1a）
- ✅ base64 `encode_chunks` の自己追加 — **複数連結アキュムレータ** `acc + c0 + c1 + …`。TCO の is_self_append が ConcatStr の左側の連鎖を最左の葉まで再帰的にたどる。（4f2bfdeb）

base64 は壁 9→4、aes は 0。**残る base64 の 4 個の壁と toml の 22 個の大半は、二つの深い未解決領域がボトルネック。小さな積み増しではなく、設計上まとまった実装単位：**

<a id="frontier-a--the-let-bound-heap-result-if-in-a-looprecursive-body-highest-leverage"></a>

### 未解決領域 A — ループ/再帰本体内の let 束縛された heap-result `if`（最大の効果）
スカラーループ本体内の `let c2 = if cond then A else B`（base64 encode_chunks/decode_chunks、toml parse_val/read_basic/…）。binds.rs は一般的な let 束縛された heap-result `if` を壁で止める。let 束縛値は保持され、スコープ末尾で drop されるが、合流した `IfThen` の dst には平坦な証明書の下で健全なスコープ末尾 drop がない（1 個の drop を、2 アームの alloc のちょうど片方へ帰属させるには検査器/Coq の変更が必要）。関数末尾なら `desugar_heap_branches` ですでに動く（各アームへ末尾を複製）。**設計（Coq 変更なし）：継続が再帰である let 束縛 `if` に末尾複製を拡張する**。`{ let c = if k then A else B; recurse(acc+…+c) }` → `if k then recurse(acc+…+A) else recurse(acc+…+B)` と書き換え、継続（再帰呼び出し + アキュムレータ更新）を各アームへ複製することで、低水準化前に let 束縛 `if` を除去する。その後、各アームはリテラル `c` と自分の自己呼び出しを持つ。`tco_collect` は分岐した自己呼び出しを許可しなければならない（各アームに 1 個、それぞれ条件付きアキュムレータ SetLocal。ループ本体の Assign はすでに対応）。リスク：TCO と分岐した呼び出しの合成。モデル化した所有権チェックとランタイム/差分プローブで保護する（証明書は、信頼されるランタイムやレンダラーの全演算を網羅するわけではない）。この一つの突破口で base64 ×2 と toml のかなりの部分が解放される。

<a id="frontier-b--the-heap-payload-resultvariant-match-camp-4"></a>

### 未解決領域 B — ヒープペイロードの `Result`/バリアント match（Camp-4）
`Result[Bytes, String]` に対する `match bs { ok(bytes) => …, err(e) => … }`（base64 decode/decode_url）。`try_lower_variant_value_match` はスカラーペイロードのバリアントを許可する（対象をアームより前に drop）が、ヒープペイロードはゲートで除外する（アームが対象のヒープスロットを借用する。`Option[String]`/`Result[Bytes,_]`）。文書化済みの Camp-4 領域（[[v1-selfhost-machinery]] Machinery 2 を match 読み出しへ拡張）。ヒープペイロードを抽出してからアームへ進む機構が必要（アームはスロット 0 を借用として読み、対象は後で drop）。

<a id="toml--also-unwrap--in-a-call-arg-position--while-body-heap-accumulator-on-top-of-frontier-a"></a>

### toml — さらに呼び出し引数位置の `unwrap !` + while 本体のヒープアキュムレータ（未解決領域 A に加えて）。

<a id="status-2026-06-22-turn-2--base64-encode-conquered-camp-4-is-the-last-frontier"></a>

## 状況（2026-06-22、ターン 2 — base64 ENCODE を攻略、Camp-4 が最後の未解決領域）

このターンでは再利用可能な低水準化の突破口を 3 個導入した（すべてゲート成功、corpus-wall 4/4、mir 501/0）：
- ✅ **複数連結の自己追加 TCO**（`acc + c0 + c1 + …`）— is_self_append が ConcatStr の左側の連鎖を再帰的にたどる。（4f2bfdeb）
- ✅ **let 束縛された heap-result `if` を TCO より前に事前脱糖** — `try_tco_rewrite` より前に func.body へ `desugar_heap_branches` を適用する。これで `{ let c = if k then A else B; recurse(acc+c) }` が分岐した再帰 `if k then recurse(acc+A) else recurse(acc+B)` になる（tco_collect が両アームを再帰的に処理）。（8cf460e5）→ **base64 `encode_chunks` を低水準化でき、base64 ENCODE が動作**（`encode("hello")=aGVsbG8=`、標準的なバイト一致、standard + url）。（1a871eed + fixture spec/wasm_cross/base64_encode.almd）
- ✅ **let に束縛された unwrap-`!` の脱糖** — `{ …; let v = e!; rest }` → `{ …; match e { ok(v) =>
  {rest}, err($x) => err($x) } }`（`!` は Err 時の早期リターンであり、match が末尾になる）。コーパス所有権カバレッジ 16744→**16877 (+133)**。リポジトリ横断で最多の壁（unwrap-bound-to-let）をスカラー/文字列結果の形で解消。文字列結果の unwrap は v0 とバイト一致。（a564a12e）

**base64 = 壁 9→3（ENCODE 完了）。最後の 3 個（decode/decode_url/decode_chunks）と toml の 22 個すべてが、今は一つの未解決領域に集中：Camp-4 — ヒープペイロードの `Result`/バリアント match。**

<a id="camp-4-design-the-last-frontier--precise-mechanism-not-yet-implemented"></a>

### Camp-4 の設計（最後の未解決領域 — 正確な機構、まだ未実装）
`match e { ok(v) => …, err(x) => … }` で、束縛されるペイロード（`v` または `x`）がヒープ（`String`/`List`）の場合。
難易度が上がる順に二つの下位ケース：
1. **scalar-Ok / heap-Err**（`Result[Int,String]` に対する decode_chunks の `let v0 = char_to_val(c)!`）：err アームはスロット 0 の String を束縛し、unwrap 脱糖の `err($x)` が包み直す。必要なこと：`Result[scalar, String]` の対象を `heap_elem_lists` としても追跡する（`heap_or_scalar_bind` が String Err の束縛を許可し、`drop_op_for` が `DropListStr` を使うようにする。正しく、Ok=len0 は何も解放せず、Err=len1 はスロット 0 を解放）。同時に `materialized_results` の len@4 タグ読み出しを維持する。err アームは包み直す前に借用されたスロット 0 の String を `Op::Dup` しなければならない（そうしないと後で対象を drop したとき二重解放）。タグオフセット（len@4）と drop（DropListStr）が組み合わさることを検証する。リスクは誤オフセット読み出しか二重解放で、どちらもゲートで検出される。
2. **heap-Ok / heap-Err**（`Result[List[Int],String]` に対する decode の `match decode_with(..) { ok(bytes) => ok(string.from_bytes(bytes)) }`）：完全な Camp-4。Ok アームがヒープ List を借用し、消費する。これは文書化済みの借用スロットからのムーブアウト（match 読み出し用の [[v1-selfhost-machinery]] Machinery 2）。

Camp-4 は二重解放の危険領域（メモリに明示された疲労警告区域）。所有権イベント検査器とランタイム/差分プローブを安全網として、新たに実装する。証明書が受理されるだけでは、実行時のすべての二重解放を排除できない。これが base64（decode）を完成させ、toml の大部分を解放する単一の突破口。

<a id="status-2026-06-22-turn-3--7-levers-total-289-corpus-coverage-heap-ok-result-is-the-last-frontier"></a>

## 状況（2026-06-22、ターン 3）— 突破口は計 7 個、コーパスカバレッジ +289。heap-Ok Result が最後の未解決領域

このターンの追加（すべてゲート成功、corpus-wall 4/4、mir 501/0）：
- ✅ **let に束縛された unwrap-`!` の脱糖**（`let v=e!` → `match e {ok(v)=>rest, err(x)=>err(x)}`、+133）。
- ✅ **Camp-4 下位ケース 1**（scalar-Ok / heap-Err の `Result[Int,String]` match：Err-String 束縛用の heap_elem_lists 追跡 + DropListStr + ResultErr アームが借用ペイロードを Dup、+137）。
- ✅ **if アーム内 unwrap の脱糖**（`let v=if c then e! else d` → `let $r=if c then e else ok(d); $r!`）。

セッション合計：コーパス所有権カバレッジ 16744→17033 (+289)、再利用可能な低水準化の突破口 7 個（モジュールグローバル Init::Bytes、Err 補間アーム、複数連結 TCO、let 束縛 if の事前脱糖、let 内 unwrap、Camp-4 下位ケース 1、if アーム内 unwrap）。**aes が動作、base64 ENCODE が動作。**

<a id="the-last-frontier--heap-ok-result-camp-4-sub-case-2-resultlistint-string"></a>

### 最後の未解決領域 — heap-Ok Result（Camp-4 下位ケース 2）：`Result[List[Int], String]`
base64 `decode_chunks` は、`acc: List[Int]` のとき `ok(acc)` で壁に当たる。ヒープ Ok の Result である。base64 `decode`/`decode_with` と toml の heap-Ok match も同じものが必要。これは新しいレイアウト：既存の heap-Ok Result 処理は文字列結果専用（materialize_result_str + cap-tag@16 経由の `Result[String,String]`/`Result[Value,String]`/`Result[List[Value],String]`）。`Result[List[Int], String]` は String/Value ではない `List[Int]` の Ok ペイロードを持つため、(a) List-Ok Result 用のレイアウトとタグ、(b) heap-result-if アームでの `ok(list)` コンストラクタ実体化、(c) `match r { ok(bytes) => … }` の heap-List-Ok 束縛と drop が必要。base64 DECODE と toml の heap-Ok match を完成させる、最後の単一の突破口。本当の新レイアウトの実装単位（ゲートを二重解放の安全網とし、新たに行う）。

<a id="status-2026-06-22-turn-4-effortmax--camp-4-landed-decode_chunks-needs-tco-over-match"></a>

## 状況（2026-06-22、ターン 4、effort=max）— Camp-4 導入済み。decode_chunks には match に対する TCO が必要

このターンで導入したもの（すべてゲート成功、corpus-wall 4/4、mir 501/0。セッションのカバレッジ 16744→17088、+344）：
- ✅ **if アーム内 unwrap の脱糖**（`let v = if c then e! else d` → `!` を持ち上げる）。
- ✅ **flatten-let-block**（`let v = { s..; tail }` → `s..; let v = tail`）+ **inline-tail-accumulator**（1 回だけ使う let 束縛の `acc + …` を再帰引数にインライン化し、is_self_append が許可できるようにする）。入れ子の let 束縛 `if` の List アキュムレータを低水準化できるようになった（decode_chunks の `new_acc` 形を単独で検証）。
- ✅ **Camp-4 下位ケース 1**（scalar-Ok / heap-Err の `Result[Int,String]` match。err アームの String 束縛 + ResultErr Dup、カバレッジ +137）。
- ✅ **Camp-4 下位ケース 2（値を返す match）** — heap-Ok の `Result[List[Int],String]` に対する値を返す match は、len-tag @4 ではなく cap-tag @16 を読む（materialize_result_str の構築と一致）。レイアウト不一致を修正した（match が誤ったタグを読んでいた。潜在的な無言の誤コンパイル経路）。これでバイト一致する。

<a id="decode_chunkss-last-blocker--tco-over-a-match-recursion-in-a-match-arm"></a>

### decode_chunks の最後の阻害要因 — `match` に対する TCO（match アーム内の再帰）
base64 decode_chunks はまだ壁に当たる。根本原因（このターンで正確に特定、再現例 `umr`）：unwrap-`!` の脱糖は `let x = f(pos)!; recurse(.., acc+[x])` を `match f(pos) { ok(x) => recurse(..,
acc+[x]), err(e) => err(e) }` に変換する。再帰が match アーム内にある。しかし TCO の `tco_collect`/`tco_rewrite` は `if`/`Block` の末尾だけを再帰的に処理し、`match` アームは処理しない。そのため自己呼び出しは埋もれた非末尾の葉として扱われる → TCO が適用を辞退 → 壁。tco_collect/tco_rewrite に `Match` を追加すること（試行したが未検証として撤回）は必要だが、それだけでは不十分。さらに (a) 結果アキュムレータ / ループ後ディスパッチが Result の二つの基底 `ok(acc)`/`err(e)` を扱うこと（err 基底は match で束縛された `e` を読む）、(b) ユーザー Result（char_to_val）に対する TCO ループ本体の `match` を、線形化せず実行することが必要。つまり文としての match の対象追跡が、ユーザーの名前付き呼び出し Result を網羅しなければならない（別の潜在的な線形化による誤コンパイル、hok2）。この 3 個は連動する。バイトテストとともに、新たにまとめて行う（corpus-wall ゲートはバイト単位の誤コンパイルを検出しないので、それぞれ v0==v1 fixture が必要）。これが base64 decode と toml の heap-Ok / 呼び出し引数内 unwrap の壁の大部分を完成させる単一の突破口。
