説明：組織全体の v0==wasm バイト検証走査と、それで見つかった wasm のバグ分類
<a id="org-byte-verification--every-repos-own-vectors-on-both-targets"></a>

# 組織のバイト検証 — 各リポジトリ自身のベクトルを両ターゲットで

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](v1-org-byte-verification.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](v1-org-byte-verification.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](v1-org-byte-verification.zh-CN.md)

> 原文コミット固定の非規範閲覧用スナップショット。原文を正本とし、自動同期しない。独立した実行、実装検証、証明検査、測定は行っていない。歴史的コマンドや設計指示は資料であり、実行した指示ではない。
> [Original source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-org-byte-verification.md)

> 原文上の注意：これは 7 月 2–15 日の走査記録であり、新たな組織全体の検証ではない。冒頭の方法はスイートの独立した成功と明示的なバイト比較を区別しており、BYTE_VERIFIED ラベルや旧表題もその区別を消さない。production emit_wasm、MIR/render_program、native host/FFI の例外、browser-host build、per-file 分類、resolved-strict 測定は範囲が異なる。初期未解決項目と後段修正、古い残件一覧（grammar/TCO、第 6・7 回の重複も含む）は併存し、新しい現況として整合させない。wall 0 は記載の lowering／分類結果であり、普遍的な実行時正しさやリーク不在ではない。有限のテスト件数、リークループ、カーネル検査、native oracle 比較は記載の境界を保つ。host 環境の継承と snapshot 挙動は歴史的な実装詳細であって実行承認ではない。最後の 3 欠落という説明に名前付き 2 項と引継ぎ 1 項があることも含め、原文の数値を修正せず保持する。

セッション記録（2026-07-02、`v1-porta-read-message-handoff.md` の続き）。目標は引継ぎの手順 1–3：porta の native build を通し、組織全体で `wall=0`（lowering できる）から、**リポジトリ自身のテストベクトルを native と `--target wasm` でバイト同一に実行する**検証へ広げる。

<a id="method"></a>

## 方法

テストのある全組織リポジトリで、`almide test --target native` と `almide test --target wasm` の**両方**が全テストを通る必要がある。リポジトリ自身のスイートのアサーションがベクトルとなる。両ターゲットでの成功はそのアサーションを確立するのであって、任意の stdout/stderr/exit バイトの一致ではない。この記録がそれらの観測を明示的に比較した箇所だけでバイト同一性を主張し、独立に成功したスイートとは区別する。走査スクリプトのパターンはこのセッションの履歴に記録している。テストのないリポジトリ（almide-web、almide-sqlite）はこの方法では検証できず、🟡 のまま。

<a id="porta-native-build-handoff-step-1--fixed-52--0-errors"></a>

## porta native build（引継ぎ手順 1）— 修正済み、52 → 0 エラー

引継ぎでは porta の native 停止原因を「toml-dep borrow/clone codegen」としていた。実際の内訳は：

1. **22× E0308 二重ラップ** — ResultPropagation Phase 2b（81840f8d）が、Result を宣言した effect fn を呼ぶ match-tail arm を Ok で包んでいた（sig-lift されないため `lifted_fns` にないが、すでに Result 型）。修正：型がすでに Result の tail は決して包まない（`b03d71e7`）。
2. **28× E0425 extern-fn 不一致** — module の `@extern(rs, ...)` fn は `use bridge::f as f;`（裸の名前）を出力するのに、呼出箇所は flatten 接頭辞 `almide_rt_<mod>_<f>` を出力する。修正：alias に接頭辞付きの名前を使う（`71b22b08`）。
3. **Capability E0425 + CapabilitySet E0308** — develop 側 #697/#698/#699（loop-body Bind ty mangle、TCO shared-mut、TCO 事前構築 owned param）を cherry-pick して修正。
4. auto-? skip-set は `ok(match parsed { ok/err })`（value wrapper の内側の match）と、トップレベルより下にネストした Bind を見落としていた。全 Bind 深度で適用する網羅 visitor 走査で修正（`d5794a86`）。checker（infer_p5）と lowering（auto_try）の両方。

回帰ハーネス：新しい crossmod-matrix cell 3 件 + module-extern native gate（`6d6adf05`）。porta：native build はクリーン、`almide test` 8/8、wasm 側 7/7（+1 FFI ファイルは設計によりスキップ）。

<a id="the-wasm-bug-classes-the-sweep-flushed-out-all-pre-existing-also-on-develop"></a>

## 走査で見つかった wasm バグ分類（すべて既存、develop にも存在）

各件とも、リポジトリのスイートが wasm で trap／乖離することで発見し、純粋な stdlib の再現へ最小化し、根因を特定して修正し、`spec/wasm_cross` fixture + contract で固定した：

| 修正 | 分類 | contract |
|-----|-------|----------|
| `eb0a0fc3` | string の素通し fast path（replace/replace_first/pad_start/pad_end/capitalize）が +1 なしで入力を返し、検索に一致しない処理を連ねた pipe chain で rc が不足した（svg escape_attr trap） | C-121 |
| `b78fda19` | record spread が heap field を +1 なしでバイトコピーし、override が `emit_stored_field` を迂回した（svg doc が attrs Map を失った） | C-123 |
| `b17593d2` | value.merge/pick/omit/json.keys が pair list を 4 バイト不足で確保（cap word なし）、cap を未初期化のままにし、pair/key を借用でコピー。value.get/field の ok payload も借用（toml aot が field を黙って失った） | C-122 |
| `d4de9c5e` | `Value == Value` がポインタを比較（deep-eq runtime が存在せず）、`json.get(f,"import") ?? json.null() != json.null()` が全 fn を JS import と誤分類（almide-wasm-bindgen）。加えて as_array の ok payload が借用 | C-124 |
| `9e5927aa` | value.merge が a の key 位置を失い、native oracle と異なる非 object 処理をした。位置を保持する形へ書換え、古い `@xt-allow` 乖離 2 件（value_eq、value_merge）を削除 | C-103 |
| `86480293` | `bytes.set` が無条件で in-place 格納（oracle は clone）し、param 経由の set が呼出元の buffer を破壊（aes cfb8 NIST vector が誤り）。現在は値意味論で、AliasCowPass が拒否権を持つ `x = bytes.set(x, …)` の in-place fast path 付き | C-125 |

ほかに、v1 ファイル分割による rt-oracle registry のずれを修復（65 entry の参照先変更、`f121b1ff`）。gate は verified 137/137、grandfathered=0 で成功。

<a id="result"></a>

## 結果

テストスイートを持つ全組織リポジトリが両ターゲットで成功：yaml、sha1、toml、svg、rsa、porta、csv、bigint、base64、aes、almide-wasm-bindgen、almide-lander、almide-grammar。`scripts/org-trust-status.sh` の `BYTE_VERIFIED` と dashboard に新状態を記録。除外：almide-web / almide-sqlite（テストなし、先にベクトルが必要）、almide-dojo（task-bank fixture でコンパイル可能なスイートではない）、almide-bindgen（dashboard 参照）。

<a id="graphics--ai-stack-spot-check-same-day-follow-up"></a>

## Graphics / AI stack の抜き取り確認（同日、追跡）

dashboard の「Graphics / AI stack」に記録：svg/lumen/homullus は byte-verified（スイート、両ターゲット）。canvas/wasm-canvas/wasm-webgl/obsid は wasm build 成功（browser-hosted で headless run は非該当）。コンパイラでの発見は 3 件：

- **almide-aituber**：develop-v1 の wasm emit は構造検証に失敗（`type mismatch: expected i32, found i64`）するが develop v0.27.13 は build 成功。唯一見つかった v1 対 develop の乖離で、2026-07-02 セッション以前からある（59dfd762 で再現）。v1 branch の bisect が必要。
- **almai**：両 branch で `[COMPILER BUG] unresolvable bare type name(s) reached codegen`。8 provider module が同じ型名を定義し、裸の参照を解決できない（#433 分類）。
- **nn**：両 branch で未解決 `__tco_tmp_data`（ty=Unknown）。TCO temp に型解決がなく、build は正直に拒否される。

<a id="second-pass--the-v0-leftovers-rescued-onto-v1-2026-07-02-same-day"></a>

## 第 2 回 — v0 の残件を v1 に救済（2026-07-02、同日）

方針：v0 に backport せず、v1 をこれらが動く branch にする。

- **nn：0/13 → 13/13、両ターゲット**（現在 `BYTE_VERIFIED`）。各失敗形を先に最小化したコンパイラ修正 5 件：(1) TCO の borrow-preserved Bytes temp が実際の型を持つ。Rust 側「注釈なし」の意図を `codegen_annotations.infer_binding_tys` に移した（旧 `Ty::Unknown` の密輸は ConcretizeTypes gate が正しく拒否）。(2) #653 lambda-param pin は callee 自身の未束縛 generic を param に書かなくなった（nested-HOF 要素型が黙って Int になっていた — C-126）。一方、scope 内の rigid generic（`names[T: Labelled]`）は引き続き pin。(3) `AlmideMatrix` に `AlmideRepr` impl を追加（constructor 形式、Set の先例）。(4) SIMD fast-exp（avx2/neon/simd128）は入力を ±708 に clamp。softmax の `-1e9` mask が `(k+1023) << 52` exponent bit trick を折り返して行全体を壊した（masked attention が誤った NIST vector を返した）。(5) `unwrap_or` は chain 型未解決時に default 引数から payload サイズを決める（`list.find |> option.map |> option.unwrap_or` が `i64.const` arm を含む `if (result i32)` block を出力 — C-127）。
- **almide-aituber：修正済み** — v1 対 develop の差は develop 側 #717（auto-? が effect branch を unwrap した後に if/match/block 型を再計算）の欠落で、cherry-pick した。
- **almai：完了（2026-07-03）**。nominal/structural 分岐は STRUCTURAL-TWIN merge で解決。checker は同じ base name・同じ shape の record 宣言を module 間で実際に unify する（どの nominal name になるかは constraint 順序の偶然）。codegen もその意味論を実現するよう、flatten pass が宣言を（base name、shape fingerprint）でまとめ、全 twin を 1 つの canonical struct に写す。bare-ref repair は全 owner が twin の集合を許す。同名でも shape が異なる型は別 mangle を維持（checker も 1 package 内では E020 で拒否）。almai native suite：56 test 成功。guard cell：`structural_twin_records_flow_both_directions`。
  （置き換えられた説明も記録として残す：）E0063 分類は修正済み（flatten mangle は名前キーの codegen annotation — default/boxed field、ctor_to_enum — も remap し、module 型 field の default が入る）。残り約 37 エラーは `LLMResponse`/`ToolCall` 対 `openai.LLMResponse` などの根本的 NOMINAL/STRUCTURAL 分岐。checker は同じ shape の module 間 record を受理するが、codegen は別 Rust struct を出す。解決には言語判断（check で拒否、または変換挿入）が必要で、develop でも失敗。
- debug 補助：構造検証失敗時に `ALMIDE_DUMP_INVALID_WASM=<path>` が無効 module を出力（name section 保持）し、`wasm-tools
  validate/print` が壊れた関数を名指しできる。

<a id="third-pass--the-last-two-known-leftovers-2026-07-03"></a>

## 第 3 回 — 最後の既知残件 2 つ（2026-07-03）

- **svg の module 間 render stack overflow：修正済み**。overflow はコンパイラ側。異なる名前の nominal pair（`El` と module twin `lib.El`）への `unify_structural` が両側を展開して field に再帰し、再帰型では `children: List[El]` 内で同じ pair に永遠に戻る。等再帰 guard：checker に処理中 pair 集合を設け、再遭遇した pair は余帰納的に unify。svg は今や両ターゲットでバイト同一に render。guard cell：`recursive_record_type_cross_module`。debug 補助：`ALMIDE_TRACE_PASSES=1` が各 pass を実行前に名指しする。
- **module 間 `@inline_rust`：両経路を修正**。wasm は全 `@inline_rust` fn を「dispatch-only」として飛ばしたが、user package の fn は portable 実装として実際の Almide body を持つ（attr は native 専用最適化）。今は Hole body の宣言だけを飛ばし、wasm が body を compile。native は template の裸の struct token（`Cfb8State { .. }`）を mangle 後の世界に貼り付けた（E0422）。StdlibLowering が package template 自身の型 token を canonical dotted name に再修飾し、flatten pass が template 内の dotted token を flat struct name に書き換える。aes cfb8 NIST F.3.7 は module 間で成功し、両ターゲットでバイト同一。guard cell：`inline_rust_with_fallback_body_cross_module`。

<a id="fourth-pass--the-final-two-threads-2026-07-03"></a>

## 第 4 回 — 最後の 2 スレッド（2026-07-03）

- **almide-sqlite：検証済み**。根因：ハイフン付き package 名が `parse_toml` でエラーになり、それを黙って握りつぶして `[native-deps]` rusqlite injection が落ちた（後段の不透明な E0433。run.rs/build.rs は今は警告）。native binding も runtime の `AlmideMap` Map repr からずれていた。両方を直し、`:memory:` に対する全 API + file persistence round-trip を覆う 10 test の `spec/sqlite_test.almd` を書いた。native host package（porta と同じ）。
- **almide-web：検証済み**。`runtime/headless.mjs` は import surface 全体を実装する Node reference host（virtual-DOM handle table、console capture、決定的 timer/fetch event queue）。`spec/host_app.almd` を実行し、固定 expected output と byte-diff（`spec/run_host_test.sh`）。全 binding、string-intern protocol、callback 再入 2 種を行使。`runtime/web.js` の DOM TODO にも同じ意味論の実際の handle-table 実装を入れた。
- **引継ぎ手順 4：完了 — read_message が VERIFIED render_program 経路でバイト同一に動く**（stdin Content-Length framing → json.parse → record → response build → write_message を `almide run` と diff）。必要だったこと：
  1. `stdlib/json_parse.almd` — native oracle から節ごとに転記した純 Almide 再帰下降 JSON parser（char position、\u surrogate pair、末尾 separator に寛容、Int/Float 分離）、加えて `json_ctor.almd`（object/array/stringify を value_core に委譲）。self-host registry に登録。`json.parse` が str-result-module predicate に入り、`let v = json.parse(s)!` と `match` の両方を追跡。
  2. **健全性の穴を閉じた**：未追跡 Result subject の `match` は arm に call があっても両 arm の線形化に落ち、両 println arm が動いた（無言の誤コンパイル）。call を含む arm は今は wall。純 module-call subject（json.parse）は Named call と同様に追跡。
  3. **never-err strip を 2 通りで修正**：`can_err` は module call 上の `!` も見る（json.parse は err を返すのに parse_and_wrap は never-err と誤分類）。bind 位置の strip は LIFTED（または self-TCO）callee に限定。Result 宣言 fn は実際の Result block を作るので、その `!` を除くと consumer が Result handle から record field を読むことになった。
  4. statement と value の Result-match bind の両方で nested-variant payload（`ok(m)`、m: Option[record]）を追跡し、porta の `match m { some(req)/none }` が tag で分岐。
  5. 捨てる heap-result call（statement としての `write_message(..)!`）も返却 block を受け取り scope-drop する。裸の void call は wasm stack に残した（invalid wasm）。
  gate：corpus-wall は 4556 fn に TOTAL、output-parity baseline 126 → **151**（+25、JSON-codec cascade）、almide-mir test 501/6-known、spec 273/273。

<a id="fifth-pass--the-output-parity-frontier-2026-07-03-same-day"></a>

## 第 5 回 — output-parity の前線（2026-07-03、同日）

方針：救済を続ける。render_program 経路の MISMATCH（無言の誤コンパイル）と RUNERR（無効 wasm）を正面から攻めた。**MISMATCH 6 → 0、RUNERR 8 → 2、parity baseline 151 → 162**。得られたもの：

- **Value-model formatting**：stringify の Float arm は Rust の生 `{}` を出力（float.to_string の ".0" を除去）。float.parse は -0.0 の符号を維持（`0.0 -` でなく `-1.0 *`）、pow10 scale を f64-inf 境界で clamp し、accumulator（末尾）再帰を使う。以前は "1e99999" が call stack を使い切った。
- **Unicode whitespace**：string.trim/trim_start/trim_end は境界で UTF-8 を decode（char::is_whitespace の全 25 codepoint 集合）。int.parse / float.parse もそれを通る（MIR 経路の C-021）。
- **json.parse を prim byte floor 上に書換え** — char-indexed 初版は O(n²)（string.get + 文字ごとの concat）で、複数 KB の glTF で timeout。今は byte-addressed、write-cursor string decoder と \uXXXX 用 UTF-8 encode（surrogate pair 含む）。
- **lifted lambda が variant_layouts + global_inits を継承** — `list.filter((t) => match t …)` 内の custom-ADT match は空 registry で解決し、全要素を除いていた（closures_and_variants）。
- **heap-element combinator routing**：String 以外の heap payload（variant、Value）に対する filter/get_or/unwrap_or は rc-sharing self-host（`list.filter_rc`、`list.get_or_value`、option/result value-unwrap_or variant）へ送る。block の length word を byte count として読んでいた `_str` deep-copy（UAF ゴミ）の代わり。share できない combinator は未登録名へ送り、明瞭な wall とする。
- **scalar TCO を許可**：destructure のない末尾自己再帰に対して。self-host byte-walker（`__split_fill`、`__chunk_outer`）が大入力で stack を使い切らなくなった。tuple-destructure body は拒否（loop rewrite が誤処理）して実際の再帰を維持。
- **全箇所で `Try` が `Unwrap` と並ぶ**：monadic desugar、never-err strip、そして新しい rewrap（never-err call を明示的 `Result` 型 var に代入する場合 `ok(call)` に再ラップ）。effect_assign_unwrap matrix（assign/loop/index/annotated/unannotated）は全成功。
- **`_start` が明示的 Result main を処理**（tag を読み、Ok block を drop、Err で trap）。全 `fn main() -> Result[Unit, String]` CLI（porta、almide-grammar）は以前 invalid wasm（"values remaining"）を出していた。
- **CLI dispatch 形**（`match list.get(args,1) { some("cmd") => …, _ => usage }`）を実行可能な 2-arm 形式へ desugar。裸の Named call は一意に link した user module へ解決。`string.capitalize` を self-host。almide-grammar は今は render して dispatch matrix を実行する（output byte-diff はまだ乖離。module-record map 経路が未解決）。
- top_let_test が**scalar module-global が Const-0 に lowering**される件を露呈。const（call なし）initializer は実値を実体化し、推移的 const global（`SOLAR_MASS = 4.0 * PI * PI`）も含む。call を持つ init は wall。
- 全 `ok` lambda の `fan.map` を `list.map` に書換え（観測的に同一、fan lambda は var を capture できない）、defunctionalize。

第 6 回（2026-07-03、同日）：**almide-grammar の wall=0 を回復し、CLI として全バイト検証**。全 4 generator mode が同じ argv で `almide run` とバイト一致。3 層：Option-String literal dispatch desugar、裸 Named の一意 linked module への解決、module type layout の裸 base name への alias（裸 `Named` 参照が layout なしで record を読み、field が黙ってずれた。一意 owner のみ、曖昧なものは qualified のまま＝wall）。v1 の `env.args` がすでに v0 と一致することも確認（argv[0] を飛ばす。以前の差は `wasmtime -- args` が文字通り `--` を guest に渡したため）。`Map[Int, String]` / String 以外の値を持つ heap map は、誤 slot の plain/`_str` variant を link せず明瞭に wall（map_set_eq：invalid wasm → 正直な wall）。

第 7 回（2026-07-03、同日）：record-value surface を系統的に。porta protocol 層は全成功（実際の wall 2 → 1、残りは本質的に wasmtime-bridge 隣接）。入ったもの：heap-result if-arm 内で monadic `match` が動く（tail-duplicated `let x = if c then f()! else []` — resolve_env）。許可 class の空 list literal を実体化。structural record-list literal を合成 anon-record drop へ送る（欠落していた `$__drop_list_anonrec_<hash>` wrapper 生成も）。これにより `f([{…}])` 引数位置も通る。`list.get/first/last` は record 要素を share（layout 同一の `_value` accessor に elimination で対応付け）。record `??` default は handle-level value helper で選ぶ。最後の runtime RUNERR も落ちた：append-accumulator TCO が pure-call で包んだ成長も許可（`string.take(acc + "x", 8)` — tco_deep_recursion_churn の 2M-iteration spin がバイト一致）。parity：match=163、RUNERR 1（負荷による不安定）、MISMATCH 1 実在（float.parse の正確な丸め）。

この前線でまだ未解決：Map repr variant（`_ivh` scalar-key/heap-val、`_hval` heap-val-non-String — map_set_eq の壁）、`tco_deep_recursion_churn`（call を通じて作る heap accumulator — `string.take(acc + "x", 8)` — 汎用 heap back-edge が必要）、denormal/max 境界の float.parse の正確な decimal→f64 丸め（strtod 級の課題）、almide-grammar output 乖離、porta の native-FFI wall 2 件。

<a id="sixth-pass--org-wall-1010-arc-ceangal-7721-2026-07-14"></a>

## 第 6 回 — org wall 101→0 のアーク、ceangal 77→21（2026-07-14）

- **tail spread/record ctor**（返される `{ ...n, gap_main: v }` / `text()`）：tail.rs は SpreadRecord+Record を arm で先行して使っていた builder へ送る。`lower_owned_heap_field` に nested-spread arm と `value_or_global` Var arm を追加（`_default` など record global、scalar-only fallback も）。spread base は Member でもよい（`{ ...v._style, … }` は `try_lower_heap_field_borrow` で借用）。ceangal 77→42。
- **mutable module-level `var` の壁（scroll 分類 + 生きた誤コンパイル）**：read/assign が黙って init を実体化／local を再束縛（`5 3 0` 対 native `5 8 8`、probe 確認。`inline_pure_call_globals` も mutable call-init を inline して write が不可視）。今は各 `var` が `MG_SLOT_BASE + 8i` に linear-memory slot を持つ（bump base を移動、N=0 module はバイト同一）。read = `LoadHandle`+`Dup`（cert `a` = render の rc_inc）、assign = build-new → `$__mg_take`（CallFn-owned、証明書に忠実）→ 型で振分け drop → store+Consume。`g[i] = v` = take → MakeUnique(COW) → elem store → store-back。合成 `__mg_init` は `__global_init` より前（init が lowering 不能なら program を wall）。`__mg_take` は mir≤ir call count から除外（prim-class の注入 accessor）。ceangal 42→21（scroll 21→0）。module_var_test + wasm_cross_pkg は今は v1 で実行（corpus 0 は今や正直）。
- **Float export ABI**：Float param/ret を持つ `pub fn` export は `f64.reinterpret_i64` wrapper（`$__export_<fn>`）経由で render。public ABI は実際の f64 を提示（v0 parity）、非 Float param は実 repr を維持（heap は i32）。
- **既知 v0-wasm 乖離（記録、未修正）**：`let snapshot = g` の後 `g[i] = v` — v0-wasm は alias（snapshot が write を見る）、native は clone（`9 9` 対 `9 2.5`）。v1 の COW は native に一致。v0 廃止（Phase 3）で乖離経路が不要になる。
- **borrow-chain 追跡（同日）**：`node.children[item.idx]`（list が実体化 aggregate の field である index）は `try_lower_heap_field_borrow` で借用。`line.items[ii].idx`（index した要素からの scalar read）は IndexAccess container arm で解決。併せて Yoga layout core 全体（compute_size / do_layout / resolve_line_flex / layout_line / diff_tree）が開き、ceangal 21→14。fixture：`spec/lang/member_index_borrow_test.almd`。
- **ceangal 残り 14 — 全件 module 間で停止（per-file classify の下限）**：各 wall の実コードを読んだ。全件、per-file classify が解決できない module 間 state に依存（`lay.MARGIN_AUTO` record-field global、両 arm if 条件の `render.list_frame_height` module 間 mutable global、generic Cell API 経由 `td.todos.get()`、view_to_node/theme の `v.*`/`lay.*` sibling call）。ceangal の per-file wall 指標は module 間境界で下げ止まり。以後の前進は (a) sibling 解決付き分類ツール、(b) generic-module-fn mono/link 修正、(c) v1 pipeline の module 間 mutable global bridge（今の slot map は同 region 内）を通じる。
- **既存 checker bug（ceangal 自身の suite を止めるが、このアークの回帰ではない）**：ceangal の `almide test` は #433 name-pinning postcondition で失敗（build_lines/resolve_line_flex/layout_line/do_layout の `Node` は `layout.Node` に pin すべき）。公開済み v0.28.6 でも同一再現。単 file / 2-module の最小 probe では再現せず、ceangal の実 module graph が必要。別 checker 作業系統。
- **既存 build blocker（ceangal、v0.28.6 にも）**：`almide build
  src/mod.almd --target wasm` が IR verify で失敗、`call to unknown function
  'cell.get'`（todoapp）。module 間 UFCS で呼ばれる generic module fn（`fn get[T](c: Cell[T])`）が monomorphize/link されない。frontend（mono + ir_link）の作業系統。
- **homullus/almai 前線 = WASI-env runtime の壁**：主要 wall（`memory_path`/`memory_dir`/`sessions_dir`/`require_env` …）は `match env.get(..)` tail。v1 に `env.get` lowering がなく、v0-wasm も直ちに ICE（`[ICE] emit_wasm: no WASM dispatch for env.get`、calls_env.rs:63。公開 binary でも wall でなく panic）。この分類を開くには両 engine で WASI `environ_get` による env（+ env 観測可能性の C-contract）が必要。その後 match-tail 機構をそのまま使える。非 env の homullus wall（value_to_tool_call/value_to_message の Record/Value shape）は普通の lowering 課題。
  **出荷（同日）：両 engine の WASI-env 課題**。(1) v0-wasm `emit_env_call("get")` — `emit_wasi_argv_list` に倣った environ scan。some() は値 String を持つ boxed cell（`unwrap_or`/match が `*opt` を load する v0-wasm Option 規約。unboxed 初稿は match したが空を出力）。(2) runner + test-gate の wasmtime spawn 全 3 箇所で `-S inherit-env=y` を渡す（なしだと全 guest lookup は none、設計が予見した静かな乖離）。(3) v1：`PrimKind::EnvGet`（Capability::CliArgs、Env profile の canonical cap、新 id なし）、`$env_get` preamble WAT（4-aligned out-pointer — `$alloc` は alignment を保証せず、そのままでは wasmtime trap — と `$env_envp`/`$env_cnt` global に 1 回だけの environ snapshot。毎 call 再読版は scratch を leak し、120k leak-loop がハング）、`env_get.almd` self-host + `prim.env_get` 宣言、cert `i`-alloc + kernel 検査済み。(4) C-133（ALS-R5）+ `spec/wasm_cross/env_get.almd`（243 equal）。almai 7→0、homullus 9→6、RATCHET 0（fixture 自身が v1 で lowering）。

- **homullus 残り 6 = external-type 分類上の見かけ + glob path-model の欠落**：`ok(Resp{...})` tail は single-module なら lowering できる（probe 確認）。wall は `almai.LLMResponse`/`almai.ToolCall` が per-file classify に解決できない外部 package の型であるため（ceangal の module 間残件と同じ見かけの分類）。実際の欠落は 1 つ、`fs.glob`。native は絶対 canonical host path に pattern を照合（runtime/rs fs.rs `glob_recursive`）するが wasm preopen path model ではバイト同一に再現できない。v0-wasm にも glob dispatch はない（env.get と同じ潜在 ICE 分類）。先に path-model contract が必要で、それまでは設計上 byte-parity subset の外。
- **測定が org アークのボトルネックになった**：per-file classify は ceangal/homullus の見かけの壁で下げ止まった。次の高効果な課題は sibling/package 解決付き分類（render_program と同じ import resolution で classify）で、org 数値が実際の lowering 欠落だけを数えるようにする。
- **解決付き strict 測定（同日、render_program 経由。すでに canonical driver discovery を実行）**：**homullus = 0 wall、ceangal = 8**（per-file は 14）。theme 3 — `let gray_50 = v.rgb(…)` の module→module global ref は解決付きでも未束縛（module 間 top-let bridge が sibling module から参照する call-init global を見落とす、実 pipeline 欠落）。scroll 2 + layout 1 — strict-mode scalar-binding/for-in-var value-subset wall（permissive classify が隠した）。render 2 — view_to_node/zip_view_rects heap-result-if（実在、解決付きで測定）。真の org 前線 ≈ ceangal 8 + porta 2 + 単発。org-trust-status.sh は解決付き走査を headline 数にすべき。
  **8 件は正確に次の 3 課題へ分解**：(1) theme 3 = module→module top-let bridge（`bridge_cross_module_toplets` は main 側 ref のみ対応 — `module_origin` を持つ `ir.var_table`。sibling module ref は自身の `m.var_table` にあり bridge されない。module ごとの region map が必要。共有 union が許容する VarId 衝突に注意）。(2) scroll/layout 3 → scroll 2 は同日出荷：`let id =
  region_count` — RHS が裸 mutable-global Var の bind は alias arm（`value_for` は local のみ解決）と projection 試行（Var kind なし）の両方を外して strict wall。scalar-value 試行に Var を足し、`value_or_global` の slot Load 経由へ。ceangal resolved 8→6。layout の resolve_line_flex（for-in VAR wall）は残る。(3) render 2 = arm-block tail-helper lift：zip_view_rects の else arm は record を返す完全な block（global assign + for-loop + accumulator）。heap-result-if arm 機構は arm value を lowering するが、1792e5d7 dense-chain lift を一般化し、そのような arm block を helper fn に lift する必要がある。

<a id="seventh-pass--ceangal-0-2026-07-15"></a>

## 第 7 回 — CEANGAL 0（2026-07-15）

最後の render wall 2 件（view_to_node / zip_view_rects）は、計測を加えた bisect で 3 つの小さな欠落に分解され、各々 probe 検証 + parity 実行：

- **anon-record concat の許可**：`items + [{ x: …, content: nm, … }]` — checker は 19-field 要素を structural のままにし、concat の許可判定は Named 専用 `record_drop_type_name` を使用。`record_or_anon_drop_type_name` へ拡張（`$__drop_list_anonrec_<hash>` wrapper 生成はすでに存在）。
- **module-keyed top-let bridge**：`kind: v.ROW` — view.ROW と layout.ROW が名前衝突し、bridge の曖昧性規則が両方を削除（未束縛 → loop 全体 → arm → tail chain が wall）。今は（module_origin、NAME）をキーとし、裸名 map は fallback。衝突分類を避けたのでなく除去した。
- （第 6 回修正が残りを担った：alias-chase/ty-repair/pure-call subst、再束縛 list-registration 伝搬、member-container borrow。）

**ceangal resolved-strict：全 15 src module で wall 0**。zip-shape fixture：`spec/wasm_cross/zip_view_shape.almd`。60k-call leak-loop parity 成功。org 前線は今は porta 2（native-FFI、構造上除外）+ external-package classify の見かけの問題。org アークの lowering 側は、porta 分類の判断待ちを残して完了。

<a id="remaining-threads"></a>

## 残るスレッド

- **wall=0 件数 21 → 19（正直さであって回帰でない）**：linearization guard と never-err-strip 修正が、以前は黙って誤る code path（両 arm 実行／Result block を payload として読む）に lowering していた porta（2、native-FFI move-out 分類）と almide-grammar（1、未追跡 subject の call-bearing arm）の wall を露呈。両 repo 自身の suite は production（emit_wasm）経路で成功／byte-verified のまま。MIR 経路でこれらの形を忠実に支えることが次の課題。
- output-parity full-run の不安定さ：baseline の 2–3 file が full-gate の machine load 時のみ落ち、単独ならバイト一致（既存、記録済み）。
- almide-mir record-materialization の DEBUG test 6 件（別作業系統）。
