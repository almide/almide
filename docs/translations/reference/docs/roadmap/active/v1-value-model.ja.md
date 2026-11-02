説明：動的 Value モデル：self-host と、信頼する再帰 drop ルーチン 1 つ
<a id="v1-dynamic-value-model--the-yaml-keystone-path-a-self-host--one-trusted-recursive-drop-routine"></a>

# v1 動的 Value モデル — yaml の要石（経路 A：self-host + 信頼する再帰 drop ルーチン 1 つ）

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](v1-value-model.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](v1-value-model.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](v1-value-model.zh-CN.md)

> 原文コミット固定の非規範閲覧用スナップショット。原文を正本とし、自動同期しない。独立した実行、実装検証、証明検査、測定は行っていない。歴史的コマンドや設計指示は資料であり、実行した指示ではない。
> [Original source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-value-model.md)

> 原文の時層に関する注記：2026 年 6 月の設計・実装記録が重なった資料で、現時点の保証ではない。冒頭の着地済み更新と、旧 DESIGN・staged/reverted・残存 wall・提案 layout 節が併存する。22→15→13→11 は会話時点の集計で、現在の総数へ統合しない。後段は、証明書受理や往復試験が leak 不在を証明するという前段の主張を限定する。raw-handle 再帰ルーチンの証明書は空で信頼に残り、20000/100000 回の有限 probe は全 Value 操作の安全を証明しない。唯一のルーチンという説明と sibling drop helper・拡大する whitelist が併存する。冒頭の cap-as-tag/as_array と後段の別 shell・@16 破壊の議論、payload 自動取得と旧借用返却拒否は異なる段階であり、単一の整合した表現へ改変しない。Result→List[Value]→Value→String を原文が 3 層と呼ぶ点も黙って数え直さない。壊れた入れ子 backtick の bullet は原形保持し、その外で全文を訳す。後の実装状況は推定していない。

**状態：設計（2026-06-19）。CEO は経路 A（「Aでいくぞ」）を選択：信頼基盤を最小にし、Value モデルを証明する（constructor/extractor/serializer は .almd で self-host、証明書検査済み）。再帰 free は唯一の信頼する runtime ルーチンとする（既存 `DropListStr` と同様）。Coq 不要、各部品で v0 `runtime/rs/src/value.rs` とバイト一致検証。**

<a id="-landed-2026-06-19-the-recursive-value-drop-foundation-valuearray--3-level-recursive-free"></a>

## ★★★★ 2026-06-19 着地：再帰 Value-drop の基盤（value.array + 3 層の再帰 free）

経路 A の要石、信頼基盤への唯一の追加は構築・リーク検証済み（もう staged ではない）：
- **value.array**（value_core.almd）：自己完結 tag-5 Value `[rc@0][tag=5 @4][len@8][elem@12+i*8]`。`__varr_copy`（`prim.rc_inc`）で各要素 handle を浅くコピーする。登録済み。
- **__drop_value**（再帰、raw-handle、空証明書）：最後の参照（rc==1）で tag 分岐。tag 5 は `__vdrop_arr`→`__drop_value` で各要素 Value を解放、tag 4 は String を解放、scalar は何もしない。`Op::DropValue` は今は `(call $__drop_value …)` を render（Array の要素 Value を leak した平坦 inline drop を置換）。無回帰を検証（output-parity 64→65、既存 scalar/Str Value program 全件がバイト一致し、$__drop_value 経由で同一に drop）。
- **__drop_list_value** + **Op::DropListValue** + `value_elem_lists` 集合：`List[Value]` は `$__drop_value` で各要素を解放（平坦 `DropListStr` では各要素 Value の payload が leak）。証明書は `DropListValue` を 1 つの `d` と扱う（lib.rs + certificate.rs の grouping に追加）。`emit_scope_end_drops` と `drop_arm_locals` の両方に接続（iteration/arm ごとの teardown。leak loop が発見した bug：loop body は `drop_arm_locals` を使い、当初そこは平坦 `DropListStr` のままだった）。
- **List[Value] 実体化**：`try_lower_str_list_literal` が `[value.int(1), value.str("a")]`（Call 要素）を許可し `value_elem_lists` を記録。
- `prim.rc_dec`/`rc_inc` の whitelist += `__drop_list_value`。

§4.1 準拠：新しい手書き WAT runtime 関数なし。`$__drop_value`/`$__drop_list_value` は self-hosted Almide（value_core と自動 link）で、証明済み `$rc_dec`/`$rc_inc` を再利用。3 層再帰 free（Array → 要素 Value → String）は `spec/wasm_cross/value_array_leak_loop.almd` でリーク検証済み（Str 要素を含む 20000-iter create+drop loop が "done" で終了。allocator に free list があるため、leak なら約 320 iteration よりずっと前に 64KB を越え OOB-trap）。gate：corpus-wall ACCEPT（14491 heap、leak/double-free なし）+ cargo test（§4.1 + cert 含む）+ output-parity 65/65。これが CEO が design-first の gate を課した健全性の中核で、今は leak loop により経験的に検証された（証明書は tag-5 leak を見られない）。8 個の Value wall に残るのは `value.as_array`（read-back。Value を認識する drop を持つ Result-of-List[Value] repr が必要、別の tag-dispatched drop）、`value.stringify`（再帰 serializer）、value.object。

<a id="-landed-2026-06-19-valueas_array--yaml-1513-is_compound--emit_non_int"></a>

## ★★★★★ 2026-06-19 着地：value.as_array — yaml 15→13（is_compound + emit_non_int）

Value-array モデルの読み取り側。`value.as_array(v)` は cap-as-tag repr で `Result[List[Value], String]` を返す（str-result の `materialized_results_str` match 機構を再利用。tag @16、payload @12。Camp-4 borrow-bind `ok(items) => …` は fab14729 の str_heap_bind borrow+drop-after で、新しい match code なし）。新しい部分は drop：value-array Result は `List[Value]` payload を再帰的に解放（`Op::DropResultListValue` → self-hosted `$__drop_result_lv`。tag@16 0→`$__drop_list_value`、1→rc_dec。平坦 `DropListStr` では list の要素 Value が leak）。追跡は型駆動（`is_result_listval_ty`：Ok-arm が `List`）で、すべての str-result marking site（try_lower_variant_value_match、lower_branch、seed_variant_param、lower_bind）で健全。Ok が List なら `value_result_lists`、String なら `heap_elem_lists`。drop-op 選択を単一の `drop_op_for` helper に統合（emit_scope_end_drops + drop_arm_locals + variant-match subject drop）。潜在 leak も修正：`materialized_call_arg` は `Value` call-arg に、平坦 `Drop` でなく `value_handles`（→再帰 `DropValue`）を記録。`f(value.array([…]))` 引数は要素 Value を leak していた。検証：`value_as_array_roundtrip.almd` が v0 とバイト一致（arr_len 3/-1/0、is_compound 1/0。value.array 格納 + value.as_array 読取 + Ok(items) borrow + tag 分岐が正しい）、`value_as_array_leak_loop.almd`（3 層と記す Result→List[Value]→Value→String drop が "done" で終了）、corpus-wall ACCEPT + cargo test + output-parity。残る Value wall（6）：collect_map/collect_seq/seq_item/parse_lines/parse_nested/map_entry（parse 中に value.array/object を構築）+ value.object/value.stringify。

<a id="-landed-2026-06-19-list-iterator-tco-heap-loop-carried-escape--yaml-1311-oct_rec-bin_rec"></a>

## ★★★★★★ 2026-06-19 着地：list-iterator TCO（heap-loop-carried の回避）— yaml 13→11（oct_rec、bin_rec）

証明書拡張なしで初めて heap-loop-carried 再帰を解消。証明書を満たす変換（scalar-TCO を拡張）。縮小 list 上の末尾自己再帰 fn（`f(.., cs)` が `list.first(cs)` に match し、`list.drop(cs,1)` で再帰）を、`cs` が不変の借用 list + 合成 scalar index `idx` となるよう書換え：`match list.first(cs) { none => BASE, some(ch) => BODY }` → `if idx < list.len(cs) then { let ch = cs[idx]; BODY } else BASE`。各 `f(list.drop(cs,1), …)` で `idx += 1`（`tco_rewrite` 内）。`cs` は再代入されず、loop は証明済みで証明書を満たす scalar 形式。heap back-edge merge も `verify_ownership` 変更もない。`try_list_iter_rewrite`（lower/mod.rs）は `tco_collect`（`match` body で断念する）より前に動く。検証：`spec/wasm_cross/list_iter_tco.almd` は v0 とバイト一致（bin_rec 5/15/0/-1 direct-arm + block-arm `cnt` len 5/0）、corpus-wall ACCEPT（14548 heap）+ cargo test + output-parity 67/67。
途中で 2 バグ修正：
- `max_var_id` が pattern 束縛 var（`some(ch)`）を飛ばし、合成 `rk`/`idx` が `ch` と衝突。renderer は i32 handle と i64 flag に同じ local を使い invalid wasm。今は pattern bind も数える（既存 scalar-TCO も強化）。
- （commit 1601af5e）BinOp operand 位置の scalar `??`（`(int.parse(s) ?? 0) - 48`、`(codepoint(ch) ?? 0) - 48`）が `lower_scalar_value` を通り `Const 0` に落ち、BinOp 全体が黙って 0 を読んだ。一般的な既存の無言の誤コンパイル（当初 codepoint-borrow の欠落と誤診。codepoint(cs[i]) 単体は正常で、bug は `?? · - 48`）。`lower_scalar_value_inner` に scalar-`??` arm（`try_lower_option_unwrap_or`、scalar fallback に限定）を追加。これで oct_rec/bin_rec は end-to-end で v0 とバイト一致（oct 15/511/.../83、bin 5/15）。`spec/wasm_cross/unwrap_or_operand.almd` + oct/bin probe。oct_rec は wall が消えただけでなく lowering + render 全体が正しい。完全 yaml program 内で render するには float.parse がまだ必要。
次：append-accumulator TCO variant（collect_seq/flow_rec `acc + [x]` → in-place push）+ mutual-recursion（flow_rec↔flow_step）、次いで value.object/stringify + tuple-heap（block_*）。

<a id="historical-yaml-wall-countdown-june-2026-session-tally"></a>

## 歴史的 yaml wall のカウントダウン（2026 年 6 月セッション集計）

74 関数 → wall：22（セッション開始）→ 21（float.parse 認識）→ **19**（scalar-arg TCO、commit 77c91648）→ **17**（nested heap-result `match` arm、commit 5510dc47）→ **16**（str-result heap-payload bind = `emit`、commit fab14729）→ **15**（let-bound variant-call read-shape seed = `num_signed_base`、後述）。残り 15：oct_rec、bin_rec（heap-loop-carried + match-leaf）、flow_rec、flow_step（mutual-rec + heap acc）、block_scalar/block_line/block_nonblank（tuple-heap + let-bound heap-if）、parse_lines、map_entry、parse_nested、collect_map、collect_seq、seq_item、emit_non_int、is_compound（Value-array モデル：value.array/as_array/object/stringify + recursive drop）。

<a id="landed-2026-06-19-let-bound-variant-call-read-shape-seed--yaml-1615-num_signed_base"></a>

### 2026-06-19 着地：let-bound variant-call read-shape seed — yaml 16→15（num_signed_base）

`num_signed_base` の `let parsed = if kind=="o" then parse_oct(d) else parse_bin(d); match parsed {…}` は、(a) let-bound-heap-`if` の tail 複製（`desugar_let_bound_heap_branch`）が `match` を各 arm へ分配し、(b) nested-`match`-arm 部品が match arm を lowering するにもかかわらず wall。残る欠落：let-bound user-function Result/Option var に variant read-shape が seed されず、`match parsed` は未追跡 subject と見て wall。一方 direct call-arg 位置（`lower_call_args` の Named arm、calls.rs:1075）はすでに `seed_variant_param` で seed していた。修正：`lower_bind` 内の variant を返す Named user call にも `seed_variant_param(dst, ty)` を呼ぶ。direct 経路の鏡像。`seed_variant_param` が足すのは layout/read 知識のみ（束縛 `dst` はすでに scope 末 drop される owned heap value）なので ownership/cert 変更なし。検証：`spec/wasm_cross/letbound_variant_match.almd` が v0 とバイト一致（単純な `let p = mk(b); match p` と全 `let parsed = if … ; match parsed` num_signed_base 形の両方）、corpus-wall ACCEPT、output-parity + cargo test 成功。（block_scalar も let-bound-heap-`if` を共有するが `(Value, Int)` heap tuple を返すため tuple-heap の課題もあり、wall のまま。）

<a id="landed-2026-06-19-str-result-heap-payload-bind--yaml-1716-emit"></a>

### 2026-06-19 着地：str-result heap-payload bind — yaml 17→16（emit）

str-result（`value.as_string` の `Result[String, String]`。slot-0 @12 が唯一の String を所有、Ok/Err tag @16、`(i32.load)` で low-32 handle のみ読む）に対する Camp-4 前線。`emit`（`match value.as_string(v) { ok(s) => emit_scalar(s), err(_) => emit_non_string(v, ind) }`）は Ok String を束縛し heap result を返す。`try_lower_variant_value_match` が 936 で除外していた case（subject-drop-before-arms desugar は drop 済み subject を借用できない）。解決（証明書を満たし checker 変更なし）：str-result heap bind は payload を borrow として束縛（@12 の `LoadHandle`、`param_values` 内）、owned subject の drop を branch-join 後に遅らせる（borrow が両 arm を通じて生存）。裸 Var arm の自動取得（`Op::Dup`）に依存し、arm が payload を借用（call arg）しても返しても drop-after は slot-0 をちょうど一度解放。Ok-arm bind を `scalar_bind` から `heap_or_scalar_bind` へ拡張（`heap_elem_lists` で自己制限するので scalar Result は依然 heap bind を拒否、回帰なし）。非 str heap payload（heap-Result-of-list、Array 要素）には single-slot borrow repr がまだなく、真の Camp-4 前線（Value-array モデル）のまま。検証：`spec/wasm_cross/str_result_heap_bind.almd` が borrow-payload と return-payload の両形で v0 とバイト一致（5/5）、corpus-wall ACCEPT（4234 fn、14392 heap object。所有権証明書が leak/double-free check なので accept ⟹ drop-after は leak-free）、output-parity 62/62 + 新 fixture。baseline 外の MISMATCH 3 file は既存のまま。

<a id="landed-2026-06-19-nested-heap-result-match-arm--yaml-1917-try_decimal-parse_number"></a>

### 2026-06-19 着地：nested heap-result `match` arm — yaml 19→17（try_decimal、parse_number）

`lower_heap_result_arm`（control.rs）は Module-call / Named-call / nested-`if` / ctor arm を扱うが、nested `match` arm では `_ => None` に落ち、arm 自体が `match` の heap-result `if`/`match` をすべて wall にしていた。`try_decimal` 形（`match int.parse(c) { ok(n) => value.int(n), err(_) => match
float.parse(c) {…} }`）と `parse_number` の then-arm（`match int.from_hex(..) { ok(n) => value.int(n),
err(_) => value.str(raw) }`）。修正：`Match` arm case を追加し、tail 位置と同じ証明済み機構で再帰。variant subject は `try_lower_variant_value_match`（scalar payload 上の subject-drop-before-arms + heap-result-`if` 骨格）、Int-literal subject は `desugar_match_to_if` + `lower_heap_result_if_inner`。再帰はすでに各 leaf を `Consume` して merge された if-result `dst` を返すので、arm は追加 `Consume` を入れない（nested-`If` arm と同じ）。証明書を満たす：すでに証明され内部が均衡した 2 つの lowering を合成。新 MIR op なし、checker 変更なし。検証：特化 fixture（`spec/wasm_cross/nested_match_heap_arm.almd`、link 可能な int.parse/int.from_hex のみ）は v0 とバイト一致（5/5 case）、corpus-wall ACCEPT（4231 fn、14370 heap object、3 property）、output-parity baseline 61/61（無回帰）+ 新 fixture を ratchet に追加。baseline 外の MISMATCH 3 file は既存（A/B stash test で確認、本部品と無関係）。注：try_decimal/parse_number は今は lowering できる（wall 消失）が、利用する完全 yaml program の render にはまだ float.parse（strtod）が必要。lowering wall と link wall は独立の数。

<a id="why-this-is-the-keystone"></a>

## なぜこれが要石なのか

yaml の残り 22 個の v1 wall は動的 `Value` モデルが支配する。`value.array`/`value.object` が構築（collect_seq/collect_map）、`value.as_array` が array payload を束縛（yaml:249 `ok(items) => emit_seq(items, ind)`）、`value.stringify` + yaml `emit` serializer が読む。v1 は scalar Value（value_core.almd：Null/Bool/Int/Float tag 0–3、Str tag 4）を self-host するが、compound（Array tag 5、Object tag 6）はしない。阻害要因は再帰 free：`List[Value]`/Array の要素は自身の heap payload を持つ Value なので、既存 `DropListStr`（slot ごとの平坦 `rc_dec`）は leak させる。

<a id="the-pieces-a-coupled-unit--observable-only-end-to-end-via-stringify-so-build--byte-verify-together"></a>

## 構成部品（結合した単位 — stringify 経由の end-to-end でしか観測できないため、一緒に構築・バイト検証）

1. **`List[Value]` 実体化** — `try_lower_str_list_literal`（binds.rs:103）を拡張し、tuple/ResultOk Call 経路同様に Call 要素（`[value.int(1), value.int(2)]`）を許可。list を Value 要素 list（新集合 `value_elem_lists`）として記録し、drop を平坦 `DropListStr`（str/array/object 要素が leak）ではなく再帰 Value-aware なものにする。call-arg 位置も同様（calls.rs、f89cbfca List[String] 部品の鏡像）。

2. **信頼 runtime ルーチンの再帰 `$__drop_value`**（preamble、render_wasm.rs ~1040。`$rc_dec`/`$alloc`/inline DropListStr 同様、信頼への唯一の追加）。rc==1（最後の参照）では：
   - tag < 4（scalar）：nested なし → `rc_dec(p)`。
   - tag == 4（Str）：`rc_dec(payload@12)`（String を解放）→ `rc_dec(p)`。（現 inline DropValue と同じ。）
   - tag == 5（Array）：payload@12 = `List[Value]`。`for i in 0..len(list): $__drop_value(elem_addr(list,i))`、次いで `rc_dec(list)`、次いで `rc_dec(p)`。
   - tag == 6（Object）：payload = key/value store（v0 Object layout `Vec<(String, Value)>` に合わせる）。`for each pair: rc_dec(key String); $__drop_value(value)`、次いで `rc_dec(store)`、次いで `rc_dec(p)`。
   証明書は 1 つの `d`（opaque な `Op::DropValue`）を見る。再帰は信頼ルーチンで、rt-oracle-registry 差分テストで検証（`value.stringify` round-trip fixture が v0 に対する leak/double-free 不在を証明）。`value_elem_lists` List は sibling `$__drop_list_value`（要素ごとに `$__drop_value` を loop + list を free）で drop。

3. **`value.array`/`value.object` self-host**（value_core.almd）：`alloc_value`、`store32(h+4, 5/6)`、`store64(h+12, <list/store handle>)`。items を move する（v0 は `Array(items.clone())`、つまり deep copy。self-host は items をコピーするか所有権を取る。stringify round-trip で v0 のバイトに合わせる）。crates/almide-types/src/self_host_registry.rs（`self_host_runtime()`）に登録。

4. **`value.as_array`/`value.as_object`** — tag を読み、tag 5/6 → `Ok(payload list/store)`（heap payload を束縛、yaml:249）、それ以外 `Err`。heap-Ok Result（cap-as-tag、value.as_string 同様。commit 7b24ef8f の str-result 機構を再利用）。

5. **`value.stringify`** self-host — 再帰：scalar → "null"/"true"/n/float.to_string、Str → `"\"" + escape(s) + "\""`（`\\ " \n \r \t` を v0 value.rs:228 と厳密に同じに escape）、Array → `"[" + (items |> list.map(stringify) |> list.join(",")) + "]"`、Object → `"{" + (pairs |> list.map((k,v) => "\"" + escape(k) + "\":" + stringify(v)) |> join(",")) + "}"`。自己呼出は `list.map` 内（C1 で defunctionalize、非末尾）なので self-rec guard は適用せず、lowering できるはず。float は self-hosted float.to_string（#63）経由。検証：`value.stringify(value.array([value.int(1), value.str("a")]))` が v0 とバイト一致（`[1,"a"]`）。

<a id="41-compliant-recursive-drop--the-design-breakthrough--its-soundness-boundary-2026-06-19"></a>

## §4.1 準拠の再帰 drop — 設計の突破口と健全性境界（2026-06-19）

`handwritten_wasm_runtime_does_not_grow` gate（§4.1）は手書き WAT `$__drop_value` を禁止（試して revert した）。準拠経路は Almide で `__drop_value` を self-host し、`string_eq`/`__streq_at` 同様 raw handle（Int）を操作：`fn __drop_value(v: Int) -> Unit = { let tag
= prim.load32(v + 4); if tag == 5 { <loop: __drop_value(prim.load_handle(elem_addr)) > over the
List[Value]>; prim.rc_dec(list) } else if tag >= 4 { prim.rc_dec(payload) }; prim.rc_dec(v) }`。
raw Int（heap 型 value なし）なので所有権証明書は空（i/a/d/m なし）。string_eq 同様、証明書を満たす。`Op::DropValue` の render はそれへの `CallFn` になるが、op の cert `d` は維持（Value の alloc が釣り合う）。新 prim が 1 つ必要：`prim.rc_dec(addr: Int) -> Unit` → `(call $rc_dec …)` を出す `PrimKind::RcDec`。既存 `$rc_dec` を再利用、新 WAT fn なし、§4.1 準拠。

**健全性境界（注意点 — task #30「runtime Almide化」の領域、健全性の要）**：`verify_ownership` は `Op::Prim` を no-op と扱う（lib.rs:705）。ゆえに `prim.rc_dec` は未追跡 free で、これを使う self-host fn は checker に検出されず double-free できる（cert の穴）。これは `DropListStr` の inline 要素ごと rc_dec と同じ信頼水準（未追跡、rc==1 に限定、信頼）だが、全 self-host に `prim.rc_dec` を公開すると信頼面積が広がり、任意の .almd fn が誤用できる。封じ込め案（design-first、急がない）：(a) `prim.rc_dec` を whitelist に制限（`value_core.__drop_value` のみ呼べる）し purity registry 同様に強制、(b) lowering が drop routine 専用に出す `Op::DropValueRecursive`、(c) drop routine の total/safe を別途証明。ここが Value drop self-host の健全性の中核で、CEO の design-first・don't-rush 規律が適用される。突破口は実在する（raw-handle self-host は borrow-by-default/consuming-convention 問題を回避）が、rc_dec 公開こそ gate を課す段階。

<a id="-implemented--empirically-validated-2026-06-19-prim-infra-committed-self-host-staged-with-4-confirmed-blockers"></a>

## ★ 2026-06-19 実装 + 経験的検証済み（prim 基盤は commit、自前実装は確認済み 4 阻害要因とともに staged）

commit 済み（83176420）：`prim.rc_dec`/`prim.rc_inc`（PrimKind::RcDec/RcInc、証明済み `$rc_dec`/`$rc_inc` 再利用、新 WAT fn なし＝§4.1 準拠）。lower_prim_call で `fn_name ∈ {__drop_value, __varr_copy}` に whitelist 制限（未追跡 free の封じ込め。Prim は cert no-op なので、無制限 rc_dec は任意 fn の double-free を不可視にする）。whitelist + `prim.load_handle`（b50853ef）が基盤。

後述 self-host は作成・build 済みで、再帰 drop が render_program で正しく実行された（`let a = value.int(5); match value.as_int(a) {…}` → "5"、`$__drop_value` が link + call され、出力 wat の 758 行）。実装で発見した経験的な 4 阻害要因待ちで staged（tree を green にするため value_core.almd から revert）：

1. **value.stringify は TCO に阻まれ value_core を汚染する**。array case `__vstr_arr(v,i,n,acc) = if i>=n
   then acc else __vstr_arr(v,i+1,n, acc+sep+value_stringify(e))` は heap-result（String）自己再帰で self-rec guard に当たり lowering しない。未 link `$__vstr_arr` となり、value.* 呼出時 value_core はソース全体を自動 link するため全 Value program が wall。修正：stringify を Unit-extract-to-List[Value]（Unit 自己再帰は guard されない）+ `list.fold(",", value_stringify)` へ再構成（fold は C1-defunctionalize され、value_stringify 再帰は間接となり guard 不要）、または先に TCO を入れる。
2. **test harness と production の link が乖離**。render_program（corpus-wall/output-parity）は `__drop_value` を value_core 経由で自動 link するが、unit-test `lower_source`（tests_part1.rs:195、`lower_function_all_with_types` 経由）はしなかった。json unit test は `unknown func $__drop_value` で失敗する一方 production は正常。DropValue→__drop_value 接続前に lower_source の auto-link を render_program に合わせる。
3. **List[Value] 実体化が先に必要**。`value.array([value.int(1),…])` の arg は wall。`try_lower_str_list_literal`（binds.rs:103）は LitStr/Var/Record/Tuple/ConcatStr を許すが Call 要素は許さない。CallFn で各要素を実体化し slot に Consume する arm を `List[Value]` に限定して加える。
4. **drop は既存 gate では検証不能（健全性の核心）だが leak test は効く**。誤った tag-5 drop は無言の leak。print 後に leak するので output は正しく、cert は DropValue を均衡した 1 `d` としか見ず、output-parity も corpus-wall も捕捉しない。**2026-06-19 解決：allocator に free list がある（`$freelist`。rc 0 の `$rc_dec` が block を積み、`$alloc` が同サイズ block を再利用）ので leak test が可能。`while i < 100000 { let xs = [..]; … }` create+drop loop は解放されれば完了（freelist で page 1 内に収まる）、leak なら trap（64KB 越え OOB store）。証明：100000-iter List[String] create+drop loop が "done" まで動く → DropListStr は leak-free。**
   よって Value drop も信頼できる：`while i<100000 { let a = value.array([value.int(i)…]); … }` を leak test する。自己再帰で loop を駆動してはならない（TCO がなく約 100k 深度で stack overflow）。`while` loop が必要。

動作する self-host code（value_core.almd、1–3 が解決したら再貼付可能。自己完結 tag-5 `[rc][tag=5 @4][len @8][elem@12…]`、rc_inc で浅いコピー、raw-handle 再帰 drop = 空証明書）：
```almide
fn __varr_copy(src: Int, dst: Int, i: Int, n: Int) -> Unit =
  if i >= n then () else { let e = prim.load64(src + 12 + i * 8); prim.rc_inc(e); prim.store64(dst + 12 + i * 8, e); __varr_copy(src, dst, i + 1, n) }
fn value_array(items: List[Value]) -> Value = {
  let ih = prim.handle(items); let n = prim.load32(ih + 4); let v = prim.alloc_value(n); let vh = prim.handle(v)
  prim.store32(vh + 4, 5); prim.store32(vh + 8, n); __varr_copy(ih, vh, 0, n); v }
fn __vdrop_arr(v: Value, i: Int, n: Int) -> Unit =
  if i >= n then () else { let h = prim.handle(v); let e: Value = prim.load_handle(h + 12 + i * 8); __drop_value(e); __vdrop_arr(v, i + 1, n) }
fn __drop_value(v: Value) -> Unit = {
  let h = prim.handle(v)
  if prim.load32(h + 0) == 1 then { let tag = prim.load32(h + 4); if tag == 5 then __vdrop_arr(v, 0, prim.load32(h + 8)) else if tag == 4 then prim.rc_dec(prim.load64(h + 12)) else () } else ()
  prim.rc_dec(h) }
```
render 接続（revert 済み）：`Op::DropValue { v } => format!("    (call $__drop_value (local.get {}))\n", local(*v))`
+ `("value_array","value.array")` と、stringify 再構成後に `("value_stringify","value.stringify")` を登録。
Almide の注意点を確認：Unit literal は `()` であって `unit` ではない。

<a id="-full-build-attempt-2026-06-19--built-end-to-end-the-one-remaining-gate-is-the-layout-brick-as_array-usage"></a>

## ★★ 2026-06-19 全 build 試行 — end-to-end で構築、唯一残る gate は layout の課題（as_array の使用）

本セッションで結合 batch 全体を進め、各部品を build + run で確認後、再度 staged（end-to-end で検証不能 → ② に従い未出荷）。動くもの（検証済み）：
- `prim.rc_dec`/`rc_inc` + `prim.load_handle`（commit 済み）+ whitelist（`__drop_value`/`__varr_copy`/`__vfill`）。
- **List[Value] 実体化** — `[value.int(1), …]` が Call 要素を許可（Module value.* は lower_pure_module_value_call、Named は CallFn 経由）。nested-ownership list に Consume。構築済み（以前 wall、今 render）。
- **value.array**（自己完結 tag-5、`__varr_copy`+rc_inc で浅いコピー）— 出力 wat は目視で正しい（n slot 確保、tag@4=5 / len@8=n を store、各要素を rc_inc+store）。
- **__drop_value**（再帰 raw-handle、Unit self-rec helper `__vdrop_arr` は guard されない）— 以前 production で実行済み。
- **value.as_array**（`__vfill`+rc_inc で fresh List[Value] に抽出。`prim.alloc_list_str` は generic `[A]` なので `List[Value]` 型 `alloc_list_str` に新 prim 不要）— 正しく構築する。
- `Almide gotchas CONFIRMED: Unit literal is `()`; `value.at` is NOT a v0 fn (frontend rejects) — use the real v0 `value.as_array`; the allocator HAS a `$freelist` so the leak-loop gate works.`

原文の崩れた inline 表記を保持した直前項目の訳：Unit literal は丸括弧の単位値、value.at は v0 の関数ではなく frontend が拒否する。実際の v0 value.as_array を使う。allocator に free list があるので leak-loop gate が機能する。

唯一残る gate、**layout の課題（payload に正確な heap bind）**が as_array の使用を止める。`match value.as_array(a) { Ok(items) => items[i] / list.len(items) }` は誤束縛。結合した原因 2 つ：(1) `bind_pattern`（control.rs ~150）は subject 全体を Dup して heap payload を束縛（container 粒度。自身のコメントに payload-PRECISE identity needs the layout brick とある）。`items` は内側 list でなく Result shell。(2) heap-Result-of-list が未追跡：`materialized_results_str` は `Result[String,_]`（cap@16 str-result 機構、7b24ef8f）のみ記録し、`Result[List[Value],String]` は認識されず match が線形化／誤読。証拠：`value.array([7,8,9])` の後 `Ok(items)=>list.len(items)` が 3 でなく 1 を出す（list でなく shell を読む）。次は layout：payload に正確な heap-payload bind + heap-Result-of-list 追跡。その後 staged value.array/as_array/__drop_value を戻して登録し、Op::DropValue→$__drop_value を接続し、round-trip（`value.as_int(items[i])`）+ leak loop で検証。yaml の emit/collect が通る。

<a id="-the-final-gate-pinned-precisely-2026-06-19-the-camp-4-frontier-heap-payload-bind-over-a-heap-result"></a>

## ★★★ 2026-06-19 最後の gate を正確に特定：Camp-4 前線（heap-Result 上の heap-payload bind）

as_array 使用の layout 課題は、正確には `try_lower_result_match`（control.rs ~916-939）の **Camp-4 前線**。`match value.as_array(a) { Ok(items) => … }` で `items: List[Value]` が heap-Result 上の heap-payload bind となる。硬い停止箇所 2 つ：
- `Ok(inner)` が `scalar_bind`（919 行）を使い、scalar/wildcard しか許さず heap `items` → rollback。
- `heap_or_scalar_bind` でも 936 行が `heap_res` 時のすべての heap-payload bind を rollback（Camp-4 frontier: defer）。
根因（本当の cert 設計問題で、一行修正ではない）：cap-as-tag repr（value.as_string、7b24ef8f）では Ok payload が subject と alias（同じ handle）。heap-Result match は subject-drop-before-arms（965 行）で arm を明瞭な scalar-cond `if` にするが、payload borrow（`items` = LoadHandle@12 = subject）が dangling になる。代案（payload を各 arm bind に move し、arm が所有して drop）では、if-arm を平坦処理する `verify_ownership` が同じ subject の drop 2 回を見て false double-free（checker は then/else 排他性を model 化しない）。よって現時点では borrow も move も証明書を満たさない。
本当の修正はいずれか：(a) `verify_ownership` に then/else 排他性を教える（同じ object の `then` と `else` の drop は正味 1 回）。原則的な修正で heap-payload bind を一般に解放する。(b) 非共有 Result repr（別 Ok shell `[rc][tag][_][payload@12=list]`。payload は arm が move する別 owned object、shell drop は shell のみ解放）。(a) が長期的には良い（Err-heap-bind + Some-heap-bind の延期も解消）。これは ② の要（誤選択＝UAF/double-free）なので design-first、セッション末に急がない。Value モデルのその他すべては構築・検証済み（上の全 build 試行）。これが唯一の gate。

<a id="camp-4-depth-fully-investigated-2026-06-19-a-lowering-only-path-exists-but-needs-a-new-heap-result-of-list-rep"></a>

## 2026-06-19 Camp-4 を徹底調査：lowering のみの経路はあるが、新しい heap-Result-of-list repr が必要

朗報（kernel/Coq 変更不要）：cert はすでに難所を支える。heap-payload match が `items` を borrow（subject payload slot の LoadHandle）で束縛し、subject を arm の**後**に drop（現在の前でなく）すれば、`verify_ownership` は subject drop 1 回を見るので平坦証明書を満たす。no-alias 安全性も無料で得られる。borrow payload を返す arm（`ok(items) => items`）は非 owned value の return として cert がすでに拒否（lib.rs:725）。よって use-not-move arm（yaml `ok(items) => emit_seq(items, ind)`）だけが通る。Camp-4 は `try_lower_result_match` の lowering 変更（borrow-payload + drop-subject-after）であり checker/Coq 変更ではない（以前は保守的すぎた）。

残る実作業は新しい heap-Result-of-list 表現。`value.as_array : Result[List[Value],
String]` は str-result cap@16 repr（value.as_string、is_self_host_result_str_module_fn）を再利用できない。cap-as-tag は payload @16 に Ok/Err tag を書く。String なら byte 領域だが、`List[Value]` では @16 が `elem0` の高位 32 bit（slot は @12、@20、…）なので tag が要素 0 を破壊する。list-payload Result には別 Ok-shell repr `[rc][tag@4][_][payload@12=list]` が必要（match は tag@4 を読み、`items`=LoadHandle@12 を束縛）。str-result cap@16 機構とは別。計画：(1) `value`-result-of-heap repr + tracking set（別 shell、cap@16 でない）、(2) try_lower_result_match で Ok(heap) bind を許可（Ok を scalar_bind→heap_or_scalar_bind）し、この repr に限り borrow + drop-subject-after（証明済み str-result drop-before 経路は変えない）、(3) round-trip（`value.as_int(items[i])`）+ corpus-wall + str-result 無回帰を検証。限定された repr+lowering 課題（kernel/Coq でない）。cert が誤用を拒否するので検証可能だが繊細（誤 repr は corpus-wall cert が捕れない無言の破壊で、round-trip + leak gate が必要）。

<a id="prim-floor-layer-found-2026-06-19-the-foundation-below-piece-1"></a>

## 2026-06-19 PRIM-FLOOR 層を発見（部品 1 の下の基盤）

prim floor（load_str/store_str/load64/store64/handle）には Value @12 slot から `List[Value]` payload を読み出す型付き `load_list` がない（load_str は String 返却に固定）。ゆえに `value.array` が `List[Value]` payload を格納し `value.as_array`/`stringify` が読むには、先に prim-floor 追加が必要：`prim.load_list`（`List[Value]` を返す型付き `LoadHandle`、`load_str` の list 版）を `lower_prim_call`（calls.rs:1968 が load_str→LoadHandle を対応）と prim stdlib type sig に接続。（代案：Value-array を inline 要素 Value handle を持つ自己完結 tag-5 block として表現し、別 List payload/load_list を不要にする。ただし value.array は items の要素をコピーし、stringify/drop は inline slot を反復する。load_list 案と比較。）層は基盤から：`prim.load_list` → 再帰 `__drop_value` → List[Value] 実体化 → value.array/object/as_array/stringify self-host → registry。下の層を実装するたびに次の層が明らかになる（本セッションで経験的確認）。

<a id="gates-per-brick-the-proven-methodology--9-bricks-this-session"></a>

## gate（部品ごと、本セッション 9 部品での実証済み方法）

記録された gate は corpus-wall 受理、Value-model byte-match probe（build + stringify 対 v0）、cargo test、output-parity baseline。証明書受理が確立するのは model 化された所有権 event の性質であり、信頼する再帰 drop 内の全操作を検査するものではない。leak loop や差分 probe は runtime bug を露呈し得るが、有限テスト成功はすべての悪い drop が拒否される証明ではない。結果を読む際は前述の空証明書／信頼ルーチンの注意を保持する。後続 TCO/float.parse 作業は歴史的 yaml 計画に属する。

<a id="honest-scale"></a>

## 正直な規模

これは結合した単位（約 5 部品、再帰 runtime drop + serializer）で、実際に集中した複数部品の作業であり単一ターンの課題ではない。ただし各部品は Coq 不要でバイト検証可能、再帰 drop が唯一の信頼基盤追加（DropListStr 同様 1 ルーチン）。経路 A は検査済み所有権 event、経験的に試験した操作、信頼する再帰 drop 境界を組み合わせる。これらの gate 成功だけで Value 実装全体の正しさを証明しない。
