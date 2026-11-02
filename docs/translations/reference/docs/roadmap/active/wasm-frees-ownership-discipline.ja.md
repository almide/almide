説明：所有権規律を機械的に扱える形にし、参照カウントによる解放の backlog を解消する
<a id="wasm-reference-count-frees-the-ownership-discipline-drain"></a>

# WASM 参照カウント解放：所有権規律の問題解消

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](wasm-frees-ownership-discipline.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](wasm-frees-ownership-discipline.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](wasm-frees-ownership-discipline.zh-CN.md)

> 固定原文の非規範読者用スナップショット。正本は原文であり、自動同期しない。独立した実装検証や引用指示の実行は行わない。歴史的状態、計画、比較主張は原文の範囲を保持する。
> [Original source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/wasm-frees-ownership-discipline.md)

> 原文注記：6月7〜11日の各記録は時系列の歴史報告で、現在の測定や新規実行ではない。後段 Stage C/D 完了は旧残作業と当初の M2 原因断定を更新する。ptr < heap_start で heap_start が0なら常に true という原文説明の不整合は推測修正せず保持する。双gate見出しには実際4完了条件がある。leak の安全側／bounded は double-free crash との対比で、絶対安全や資源境界ではない。増大 accumulator と過剰計数の制限は残る。RC certificate はコンパイラの証拠で第三者セキュリティ認証ではない。default flag・command・process kill 指示は歴史的資料で実行していない。


**状態：** 進行中（2026-06-07）。branch は `perceus-belt-hard-error`、worktree は `xtarget-compound-display`。2 つの gate が完全に green になるまで、作業はすべて LOCAL／未 commit。

<a id="mission"></a>

## 使命

WASM はメモリ面の未開拓領域である。native Rust には借用検査器が付いてくるが、WASM backend は歴史的に**設計上すべての heap を leak させていた**。`compile_rc_inc` / `compile_rc_dec`（`emit_wasm/runtime.rs`）は意図的な no-op だった（`if ptr <
heap_start return` が、`0` のままの legacy `heap_start_global` field を読んでおり、guard が常に true になる）。Perceus belt 全体が検証していたのは NO-OP の RC balance だった。目標は、double-free を 1 件も導入**せずに解放を有効化**し、WASM が heap を回収する（churn O(n) ではなく O(1)）とともに、native↔WASM の byte 一致を保つこと。

これは「機械が書く software の trust layer」の核心である。同じ program を native または WASM sandbox で実行し、正しくメモリを解放して同じ byte を生成する。

<a id="the-discovery"></a>

## 発見

rc guard を正しい `HEAP_START_GLOBAL_IDX`（=4）へ切り替えると、構築済みの free-list 機構（alloc の走査と再利用、rc_dec の push）が有効になる。その直後、no-op model が隠していた**既存の潜在 double-free 群**が露呈した。heap 値を*借用／alias*、または *container へ move* する場所で、簡略化された Perceus は scope 末尾に `Dec` を挿入し、ほかがまだ所有する値を解放する。解放を有効にすると free-list cycle（無言の hang）になる。または `rc_dec` に追加した **double-free sentinel guard**（解放 block の rc を 0 にし、2 回目の Dec で `unreachable` trap）によって、cross-target gate が検出する明示的な WASM trap になる。

<a id="root-cause-the-simplified-perceus-has-no-ownershipmove-discipline"></a>

## 根本原因：簡略 Perceus に所有権／move 規律がない

本来の Perceus（Koka）は*所有権*を追跡する。新しい owner ができたら dup（Inc）、owner の最終使用で drop（Dec）、constructor／return に消費されたら **move**（所有権移転、別途 drop しない）する。Almide の WASM Perceus は簡略版で、scope 末尾に*すべての* heap local を Dec し、`Var` / `Clone` / `Deref` bind *だけ*を Inc していた。これは 2 方向で誤っている。

1. **Dec 対象 local に bind した借用／alias 値は過少計数**になる。local の scope 末尾 Dec が、出所がまだ保持する値を解放する（抽出 alias、env-load、直接 element accessor）。
2. **container へ move した値は過剰に Dec される**。container は値を参照で受け取り（copy／Inc なし）、その後 Perceus が source local も Dec して container の内容まで deep-free する。

この問題解消に現れる bug は、すべてこの単一の欠落の一面である。

<a id="the-completion-bar--both-gates-the-key-lesson"></a>

## 完了基準＝両方の gate（重要な教訓）

`almide test spec/ --target wasm`（test block の assertion corpus）は**必要だが十分ではない**。test block は値の assertion を試すが、形状を丸ごと見逃す。**`cargo test --release --test wasm_runtime_cross_target`** gate（約 100 の `spec/wasm_cross/*.almd` `@contract` fixture、stdout／stderr／exit の byte 一致）は、test corpus が試さない形状、たとえば **compound set/map key**（`compound_eq.almd`、contract C-015）を扱う。本当の完了基準は次である。

- `almide test spec/ --target wasm` が clean（assertion）、かつ
- `cargo test --release --test wasm_runtime_cross_target` が clean（byte 一致）、かつ
- churn benchmark が O(1)（leak gate、後述）、かつ
- native `almide test spec/` が後退しない。

「test corpus 240/0」は最初の偽の到達点だった。cargo gate は `compound_eq` の乖離が残っていることを発見した。

<a id="mechanisms-implemented-each-a-facet-of-the-one-discipline"></a>

## 実装した機構（同じ規律の各側面）

すべてが 1 つの分類器、`pass_perceus.rs` の `yields_borrowed_alias(e)` を再利用する（網羅的 `match`、wildcard なし＝全域的。新しい `IrExprKind` は意識的に分類する必要がある）。ALIAS 形式は参照を獲得し、FRESH 形式はすでに自分の参照を所有する。leak／crash の非対称性が安全側の default を決める。alias の Inc **不足**は double-free（crash）、fresh 値の Inc **過剰**は leak（安全側）。したがって、未知の形式は証明できた場所だけ FRESH、それ以外は ALIAS とする。

| # | 機構 | 場所 | 修正対象 |
|---|-----------|-------|-------|
| 1 | `yields_borrowed_alias` + **VDecl の bind 後 alias-Inc**（旧 Var/Clone/Deref Rule-1 を包含。Member/Index/TupleIndex/MapAccess/OptionalChain、Match/If/Block tail、Unwrap/Try/ToOption/UnwrapOr の剥離を扱う） | `pass_perceus.rs` ChainHead::VDecl | hamming, default_fields, result_option_matrix |
| 2 | **`emit_stored_field`** — constructor／value builder による alias 引数の dup | value str/array/object（`calls_value.rs`）、Record/List/Tuple（`collections.rs`）、OptionSome/ResultOk/ResultErr（`expressions.rs`） | codec/variant 群（11）、capture_clone, protocol_advanced, try_parse_list |
| 3 | **fold accumulator の dup** — `fold(xs,seed,f)` は seed を move-return するため alias seed を dup | `calls_list_closure2.rs:1046,1155` | option.collect-empty, is_balanced-empty |
| 4 | **EnvLoad 借用の除外** — closure body の `EnvLoad` bind local は env が所有する capture を借用するため、scope 末尾 Dec から除外 | `collect_heap_vdecls` | `>>` 合成 chain（compose_test） |
| 5 | **`is_alias_returning_runtime_call`** — `list.get_or` / `map.get_or` は element pointer を直接返す（alias） | `pass_perceus.rs` | protocol_extreme decode |

機構 #2/#3/#5 は `call rt.rc_inc` を**直接**出力する（IR `RcInc` node ではない）。

<a id="verification-status-2026-06-07"></a>

## 検証状態（2026-06-07）

- ✅ `almide test spec/ --target wasm`：**240 passed / 0 failed**（8 wasm:skip）。
- ✅ native `almide test spec/`：**248/248**。
- ✅ churn（{Codec record を構築 + encode + value.stringify} を 2M 回）：peak RSS **7.46 MB**＝O(1)、leak なし。
- ❌ cargo `wasm_runtime_cross_target` gate：**`compound_eq.almd`（C-015）が乖離**。`set.contains(sr, {record})` が F を返す（T が正しい）。さらに `main` 終了時に `rc_dec` sentinel trap。（乖離一覧の全体は gate 実行待ち。）

<a id="second-layer-the-collection-runtime-found-by-the-cargo-gate-2026-06-08"></a>

## 第 2 層：COLLECTION RUNTIME（cargo gate が 2026-06-08 に発見）

上記の expression／constructor／closure の問題解消は、test corpus と 2M 回 churn の**両方**で clean を確認した。しかし `cargo wasm_runtime_cross_target` gate（byte 一致、`almide test` が届かない部分）は**第 2 層**を露呈する。WASM collection runtime は heap element／key／value の POINTER を dup せず新しい構造へ copy するため、source の scope 末尾 Dec が新構造の保持するものを deep-free する。100 fixture を流し込む完全 byte sweep（stdout のみ。`control_flow` / `cross_module_spread` は md5 false-positive＝native compiler の stderr *warning* で、乖離ではない）で **4 件の実際の乖離**が見つかった。すべて frees-on 下の WASM `rc_dec` sentinel trap／破損である。

| Fixture | 操作 | 下位分類 | 状態 |
|---------|-------|-----------|--------|
| `compound_eq`（C-015） | record/tuple key に対する `set.from_list`/`set.insert`/`map.from_list`/`map.insert` | heap element/key RC | **set.from_list 修正済み**（stdout は一致）。exit はなお 134（set.insert/map 操作が残る） |
| `list_float_total_order` | `List[R]` に対する `list.sort_by` | heap element RC | 未解決 |
| `alias_cow` | `var b = a; a[0]=…` list COW | list BACKING／COW RC | 未解決（値はすべて正しく、main 終了時 teardown だけ double-free） |
| `list_count_index_truncation` | 巨大 index（2³²…）の `list.take/drop/slice` | 数値（おそらく u32 index 切り詰め → OOB → 破損）、frees と無関係かもしれない | 未解決／triage |

<a id="the-fix-tool--why-a-piecemeal-drain-corrupts-attempted--reverted-2026-06-08"></a>

### 修正ツールと、部分的な解消が破損する理由（2026-06-08 に試行・撤回）
`emit_elem_copy_owned(ty)`＝`emit_elem_copy` に、heap element の場合、stack-neutral な `rc_inc`（`i32_load; call rc_inc; i32_store`）を足したもの。**SHARE 地点のみ**（まだ生きた source の外へ copy）に使用し、構造内 MOVE 地点（grow して旧構造を放棄 → 余分な Inc は leak）には使わない。`set.from_list`（`calls_set.rs` の append-from-xs）へ適用すると、確かに `compound_eq` の stdout 値は直り（set.contains(record) F→T）、corpus 240/0 を維持した。**しかし** `set.from_list` + `set.insert` の churn loop が部分修正の罠を示した。10 回で native `sink=30` に対し **wasm `sink=31`**（1 個の set の重複除去後の長さが誤り）、さらに **1.5M 回で OOB**。set 操作には SHARE/MOVE 地点が複数ある。一部だけ直すと残存 double-free が解放済みメモリを再利用し、*次の*操作の重複除去が依存する element equality を壊し、長さが誤り、最終的に OOB になる。**教訓：collection の問題解消は構造ごとの SHARE/MOVE 地点をすべて原子的に直し、stdout/corpus だけでなく構造別 CHURN loop（1M 回以上）を gate にする。** 部分変更は検証済み既知の良好状態（240/0 + records churn 7.43 MB）へ**撤回**した。ツール／方針は正しいが、*変更単位*が誤っていた。

<a id="the-remaining-collection-drain-mechanical-but-per-site-sharemove-judgement"></a>

### 残る collection の問題解消（機械的だが地点ごとの SHARE/MOVE 判断が必要）
`calls_set.rs`（行 240,310,355,392,470,536,609 — insert/union/intersect/diff/…）と `calls_map.rs` に、`emit_elem_copy` / `emit_elem_copy_sized` が約 15–20 地点ある。dict path は `emit_dict_put_entry` + 102/510/519 の `emit_elem_copy_sized`。`emit_elem_copy_sized` は Ty ではなく SIZE を受け取るため、heap かどうかを渡すか、`key_ty`／val-ty に基づく owned variant が必要となる。加えて `list.sort_by`（`calls_list_closure.rs:941`）と slice/take/drop 群。各 copy を SHARE（source が残る → owned）／MOVE（source を放棄 → plain）に分類して差し替える。leak gate は test corpus ではなく **cargo gate／2M churn**。batch ごとに両方を検証する。

<a id="cow-alias_cow-is-its-own-sub-problem"></a>

### COW（`alias_cow`）は別の下位問題
`var b = a`（list alias）+ `a[0]=…`（copy-on-write）。値自体はすべて byte 一致し、main 終了時 teardown だけが COW 後の backing を double-free する。element-copy helper ではなく `AliasCowPass` と IndexAssign／var-alias RC に属する。別途慎重に直す。

<a id="remaining-work"></a>

## 残作業

1. **Compound set/map key**（`compound_eq`、C-015）。`set.from_list` / `map.from_list` は record/tuple element を参照で構造に move し、入力 list の scope 末尾 Dec が move 先の element（重複除去された duplicate も）を deep-free する。#2 と同じ move 規律の欠落が set/map runtime（`calls_set.rs`、`calls_map.rs`）にある。dup-on-store／move 除外を適用する。
2. **cargo gate 全体を実行**し、乖離 fixture をすべて解消する（test corpus は構造上これらの形状を見逃す）。
3. **Verifier からの不可視性（後続、belt を支える必須項目）：** #2/#3/#5 は emitter 内に `rc_inc` を出し、IR `RcInc` node ではないため、perceus-belt verifier は**見えない**。現在、唯一の leak gate は churn benchmark である。constructor／builder／HOF の dup を MODEL 化するよう verifier を拡張する（`value_object` / `Record` / `fold` / …は alias 引数を dup するものとして扱う）。実行時検査だけでなく*証明書の対象*にする。それまでは belt の「RC balance を証明する」という主張に穴がある。
4. **再取り込み**：M1（rc guard 切り替え）+ M2（TCO flatten + `tco_managed_params`）+ sentinel guard + #1–#5 + compound 修正を、両 gate と churn が green になった後、1 つの整合した commit として取り込む。

<a id="trap-log-environment"></a>

## 罠の記録（環境）

- `almide test spec/ --target wasm` は全 test block を wasmtime で実行する。cargo の `wasm_runtime_cross_target` は byte 一致 contract gate。**片方だけでは基準を満たさず、両方が必要。**
- sentinel guard は double-free hang を高速 trap に変えるため、corpus run が空回りせず終了する。
- **孤児 `rustc` の危険：** cargo gate は各 fixture の native 側を `rustc` で compile する。前の session が残した 3 日前の孤児 `rustc`（PPID=1）が、共有 build scratch `/tmp/claude-501/almide-build/` で CPU 99% のまま回り、gate を 47h 飢餓状態にした。計時前に `pgrep rustc` を確認し古い PPID=1 rustc を kill する。`cargo test` を `perl -e 'alarm N'` で包むと cargo は kill されるが test binary は切り離され実行を続ける。代わりに gate を background 実行する。

<a id="2026-06-1011--third-campaign-full-drain-under-the-flag"></a>

## 2026-06-10/11 — 第 3 キャンペーン：flag 下で全件解消

branch は `true-perceus`。有効化は emit 時の環境変数（`ALMIDE_WASM_FREES=1`）で制御し、**flag 下で 4 重基準すべてが green**：native corpus 264/264・wasm corpus（flag off）264/264・wasm corpus（flag ON）264/264・cargo byte gate（flag ON）67/67・2M 回 record churn が正しく RSS も平坦（13.3 MB ≈ leak-mode baseline）。

frees-ON の乖離 14 件を全解消（機構 → 修正）：
- record の typed-drop glue は宣言順 layout に対し field を名前順に sort していた → sort を除去（2 行）。
- heap_restore が free list も reset。rc_inc/rc_dec に dead-zone guard（ptr ≥ heap_ptr）を追加。
- double-free sentinel（rc=0 を記録 → 2 回目 dec で trap）+ rc_inc の resurrection trap + 絶対走査上限（1M step）。
- string.join の len==1 が element pointer をそのまま返していた → 条件付き inc。
- tuple payload の variant constructor store → emit_stored_field。
- 機構 #6：insert_rc_ops に return-alias dup（callee 側 owned return）。unwrap_or runtime call を alias 分類。
- **順序修正**：Block 値への VDecl alias-Inc を block 内へ hoist（tail bind 後、temp dec 前）。遅い Inc は temp の typed dec が解放した payload を復活させていた。
- frees 下で emitter-level ownership を廃止（is_single_use_var → false）。in-place list 再利用／raw rc_dec は IR dec と二重に block を所有していた。
- list 群：15 地点の emit_elem_copy→owned、min/max、get-box、remove_at OOB inc、set/insert/push stored-field、sort 構築後 dup walk、filter loop 後 dup、enumerate、update の置換値 release。list.repeat は slot ごとの inc（for+concat-push → repeat rewrite が露呈させた）。
- concat：call-site で fresh result の element を SHARE dup walk。
- map 群：emit_elem_copy_sized(size, dup)、dict_recap dup Option<(K,V)>（set/merge=Some、grow=None）、remove の survivor dup、update の全 table dup と old-value capture-release／not-found input inc、map.map key inc。
- json/Value：value_get／get_typed(get_string|get_array)／as_type(tag 4|5) の interior-pointer inc。set_path の kept-key／unchanged-pair／path-key／no-op cur_built／unchanged-elem inc。remove_path の 7 no-op alias return と survivor 地点。set_path new_val は stored-field 経由。
- emit_elem_copy_owned を is_heap_type で制御（Int32/UInt32/Float32 の 4-byte scalar の罠を除去）。
- free-list size-sanity bound を除去（正当な解放済み node で false-positive。cap／sentinel／resurrection trap が該当群をカバー）。

default-ON 前の残り：Stage C（TCO loop 回収、M2 再取り込み。現在 loop は iteration ごとに leak、bounded）、commit 済み churn fixture 群（構造ごと 1M+ gate）、Stage D PIN（fs scratch を構造上保証）+ C-042 解禁、flag-ON 全基準 3 連続、それから default 切り替え + C-041 改訂 + 新 reclamation contract + perf suite（Stage G）。

<a id="stage-c-finding-2026-06-11-the-b8bbdfad-m2-cannot-be-cherry-picked"></a>

## Stage C の発見（2026-06-11）：b8bbdfad の M2 は cherry-pick できない

Round-3 accounting の上に b8bbdfad の flatten_exit_tail_blocks と pass_tco managed-param protocol を再取り込みすると、新しい受入形状 `spec/churn/tco_loop_churn.almd` で `__alloc` 内 OOB（free-list `next` の破損）が発生した。loop-VARIANT TCO 引数が LICM を妨げる一方、旧 probe は loop-invariant で気づかず hoist されていた。M2 前なら同じ形状は green（param は iteration ごとに leak、bounded、安全側）。原因分類：b8bbdfad の pass_perceus 差分 93 行中、flatten が約 80 行。残る loop exit 周辺の Dec 配置規則は旧 alias accounting と同時設計され、Round-3（alias-Inc hoist、#6、scope 末尾規則）と衝突する。M2 の部分移植は構造的に健全でない。

決定：Stage C は Round-3 規則に対する iteration ごとの TCO 回収の**再設計**とし、`scripts/check-frees-churn.sh` を gate にする（tco_loop_churn が受入 test）。退避差分：/tmp/m2park（flatten 移植を含む pass_tco.rs、pass_perceus.rs）。default-ON の blocker ではない。TCO loop は iteration ごとに leak（既存挙動）するが正しさは影響せず、non-TCO churn はすでに O(1)（record_churn）。

<a id="2026-06-11--the-flip-v0270-frees-are-the-default"></a>

## 2026-06-11 — 切り替え（v0.27.0）：解放が DEFAULT に

`wasm_frees_enabled()` の default は TRUE。`ALMIDE_WASM_FREES=0` が opt-out の退避手段。切り替え時の証拠：

- tree を触らず 3 重基準が 3 回すべて green：native 264/264・wasm default（frees）264/264・wasm opt-out 264/264・byte gate 67/67・churn gate（record 2M + tco 200k）・cargo full suite 失敗 0。
- Stage G micro-perf：ON／OFF は誤差内（string 0.022/0.021 s、list 0.033/0.034 s、map 0.011/0.011 s、wasmtime、3 回の最良値）。
- contract：C-041 を改訂、C-066（`spec/wasm_cross/rc_reclaim_churn.almd`）を追加。

切り替えでさらに 1 群を発見・修正した。iteration ごとの LOOP REGION reset（iter_scope、escape しない loop body の leak 時代の回収）が bump pointer だけ戻し、FREE LIST を戻していなかった。解放 node が復元 frontier より上に残り、次 iteration の bump が list の参照中の address を再配布した（`string.len` が free-list next pointer を読む現象として観測。新 C-066 fixture を書いて数分で検出）。region reset は free list も clear するようになった。region invariant「iteration から何も escape しない」は値だけでなく ALLOCATOR STATE も含める必要がある。reset は frees 下でも有効を維持する。回収 loop 内の既知 stored-field 過少ではなく**過剰計数**も覆い隠すからである（2M churn は平坦な約 13 MB に戻る。reset 無効時には過剰計数で 2M iteration 中約 61 MB leak。これは残る Koka 精度作業の測定規模で、regression ではない）。

<a id="stage-c-complete-2026-06-11-per-iteration-tco-reclamation"></a>

## Stage C 完了（2026-06-11）：iteration ごとの TCO 回収

b8bbdfad の M2 は clean に再取り込みできた。旧 free-list 破損は M2 の責任ではなかった。iteration ごとの loop REGION reset が free list を clear せず bump pointer を戻していたことが原因で（v0.27.0 切り替え時修正）、M2 の Dec が初めて region loop 内でその list に投入していた。Round-3 accounting に 3 点を改良して再適用した。

1. `is_fresh_alloc` を ownership を返す CALL 引数へ拡張。user fn は owned を返し（#6）、alias 形状の temp は TCO rewrite が引数を VDecl に変えた後、bind-level Inc を得る。したがって call 値の self-call 引数は安全に採用でき、param は管理対象となる。literal だけの test では `spin(n-1, string.take(acc + "x", 8))` が unmanaged＝無界になっていた。
2. inner-hoist alias-Inc は tail temp の**自身の値**が alias の場合だけ発火する。block 外へ move した fresh tail は自分を所有し、無条件 +1 は TCO loop で測定上 16 B/iteration の leak だった。
3. この改良で重要な過剰 inc が取り除かれ、その下の本当の穴（履歴調査が予想した群）が露呈した。`?? default` は `Call { option.unwrap_or }` として Perceus に届くことがあるが、alias 分類器は RuntimeCall 表記しか知らず、Call 形式を FRESH 扱いし、map 所有 payload が teardown で double-free した（byte gate が数分で map_insertion_order にて検出）。unwrap_or/get_or の Call{Module} 表記も alias 分類するようになった。

測定：深さ 2M の一定サイズ TCO 再帰が 4.27 GB → **平坦な 7.3 MB**（新 gate fixture `spec/churn/tco_deep_recursion_churn.almd`）。200k loop-variant TCO churn が green。全基準も green（corpus 3 mode、byte gate 67/67、churn、cargo）。既知の限界、記録したが未修正：単調増加 accumulator（`acc + "x"` を丸ごと保持）の peak はなお総和となる。解放 block が次の要求より 1 単位小さいため、first-fit は再利用できない。所有権 bug ではなく allocator policy（capacity growth／size class）の未解決領域である。C-066 の限定例外一覧は、回収 region 外の construct-from-temp 過剰計数に縮小する。

<a id="stage-d-complete-2026-06-11-fs-scratch-pinned--the-real-c-042-root"></a>

## Stage D 完了（2026-06-11）：fs scratch の pin と本当の C-042 原因

PIN の準備で、複数 fs 操作 trap の本当の原因が free-list lifetime だけではなかったと判明した。free_list が index 1 を取った後も、`register_runtime` に PRE-FREES の global 番号（`preopen_table_global = 1`、count=2）が残っていた。**`__init_preopen_dirs` が起動時に preopen TABLE POINTER を FREE LIST head に書いた**ため、`__alloc` の無条件走査はすべての fs 使用 program で table を free node として扱っていた。数か月の各 release で起きていた。`__resolve_path` が同じ古い index から table を読み返すため、2-op fixture を通るだけの自己整合性があった。v0.26.21 の純 leak mode でも `fs.exists` 2 回で再現した。

取り込み済み：index 修正（定数 1 組）+ `__alloc_pinned` / `PINNED_RC`（host が書くすべての WASI buffer、fd_out/stat/iov/nread/data scratch、preopen table、resolve result は構造上 immortal。rc_inc と rc_dec は sentinel で早期 return）+ C-007 init-order assertion。C-042 fixture は allocation churn を挟みながら fs 操作 6 回を実行し、byte 一致する。
