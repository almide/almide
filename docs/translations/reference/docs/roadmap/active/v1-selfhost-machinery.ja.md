<!-- description: 初期self-host機構キャンペーンの履歴。後続closure/drop作業と残るlinking提案 -->
<a id="v1-stdlib-self-host--the-machinery-phase-option--list-building--closures"></a>

# v1 stdlib self-host — 機構の段階（Option / List構築 / closures）

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](v1-selfhost-machinery.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](v1-selfhost-machinery.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](v1-selfhost-machinery.zh-CN.md)

> 非規範的な履歴読者資料。固定原文を正本とし原本不変更、自動同期・独立測定なし。6月初期の件数・安全主張・nested-drop/function-table未対応境界や指示は履歴で、現行保証ではない。後続closure/records作業と未解決linking-v2提案を区別する。引用コマンドやagent指示は実行しない。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-selfhost-machinery.md)

<a id="historical-machinery-campaign"></a>

## 機構キャンペーンの履歴

以下の約28関数という状態、および「function table / nested dropがない」という到達境界は、2026年6月初期のself-hostキャンペーンの記録である。後の[closure architecture](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/done/closure-architecture-v2.md)と[records campaign](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-records-svg.md)に後続実装が記録されている。この古い件数は現行stdlibの対応範囲ではない。別の[self-host linking v2提案](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/selfhost-link-v2.md)には未解決のregistry/twin統合作業が残る。機構の過去の進展は、その提案の完了を意味しない。

状態：**約28関数がself-hostされ、実行結果がv0と一致（各commitでcorpus-wall ACCEPT 13139/4083/3582）。Optionのmatch実行機構は完了。キャンペーンは整ったバッチ生産段階（string/listのscalar/read述語）に入り、Optionを返す関数は具体化したOptionのlayoutを再利用する。** 目標 `/goal`：約381個のv0 stdlib関数すべてがbyte単位で一致して実行するまで続ける。難しい部分も確定した計画から実装できるよう、リスクを検討済みの設計を記録する。

<a id="done-self-hosted--executing--v0--28-fns"></a>

## 完了（self-hostされ、実行結果がv0と一致）— 約28関数
- **core**：int.to_string、print_str(println)
- **string**：len、repeat、is_empty、slice、trim、starts_with、ends_with、contains、count、index_of（Option、codepoint）、last_index_of（Option、codepoint）
- **math**：abs、max、min
- **list**：len、is_empty、sum、get_or、get（Option）、first（Option）、last（Option）、contains、index_of（Option）、product、max（Option）、min（Option）
- **map**（`Map[String,String]` = map_str）：new、set、get、keys、values、len、…、そして **entries**（`map_entries_str` → `List[(String,String)]`、2026-06-21）。RC共有の(key,value)タプルを構築し、`$__drop_list_str_str`で解放する（新しい`(String,String)`タプルlist：`DropListStrStr`、`str_str_elem` concat case、タプル要素のdefunc-mapと`let (k,v)=pair`分解）。svg render_attrsの足がかり（[[v1-records-svg]]）。

stringのパターン：prim.load8/32でheader（`handle(s)+4`=byte長、`+12`=data）を読み、prim.alloc_str+store8で構築し、loopには再帰を使う。listのパターン：`+4`=要素数、`+12`=8-byte i64 slot（prim.load64）。Registryはfileごとにまとめる：`self_host_runtime() = &[(source, &[(impl_fn,
call_name)])]`。auto-linkがimpl→callに改名（caps gateは既知のpureな`module.func`を読み続ける）。`lower_source`は名前で重複除去する（再帰的にlinkされたprint_strのcopy）。

<a id="machinery-1--option-unblocks-listget--first--last"></a>

## 機構1 — Option（list.get / first / lastを可能にする）
**状態：match実行の中核は完了**（commit "Execute match over a materialized Option…"）。直接束縛した`Some(scalar)`/`None`に対する`match opt { Some(x) => …, None => … }`は、選ばれたarmだけを実行し（=v0）、byte一致する：`Init::OptSome { payload }`（1要素list、`list_new(1,…)` + `list_set 0 payload`でrender）。`None`は`Init::Opaque`（len0）のまま。`try_lower_option_ctor`（binds.rs）が具体化し、新しい`materialized_options: HashSet<ValueId>`にdstを記録する。`try_lower_variant_match`（control.rs）は`len`をtagとして読む（`prim.Handle`+`Load{4}`@+4）。Some branchで`data[0]`（`Load{8}`@+12）を取り出し、armごとのframeを`IfThen`/`Else`/`EndIf`で囲む。`materialized_options` gateが健全性の鍵である。追跡済みsubjectだけをlenで読むため、具体化されていないOpaque Optionは線形化fallbackを維持する。また両armの収支がarmごとに釣り合っているため、gateを取り逃しても最悪で誤ったarmを選ぶ正しさのbugとなり、memory bugにはならない。Certには中立（Alloc = iでinitに依存せず、markerはno-op、scalar prim）：corpus-wall ACCEPTは13139/4083/3582のまま。テスト：`variant_match_over_a_materialized_
option_executes`（Some(42)/None → 42/none）、`option_allocating_loop_matches_bounded`（3000回の具体化+match+freeはbounded、leakなし）。scalar payloadとunit armのみ。
**名前付き関数も完了 — list.get / list.first / list.lastをself-host化**（commits "Self-host list.get…" + "Self-host list.first and list.last…"）。call-result追跡が入った。`is_self_host_option_module_fn(module,func)`（binds.rsのbound-var経路とcontrol.rsのdirect-subject経路で共有する単一の正本）がself-host Option callのdstを具体化済みと印付けし、`let o = list.get(xs,i); match o`と`match list.get(…)`の両方を実行する。実装（stdlib/list_get.almd）はtailで具体化するhelper（`__opt_some_at`/`__opt_none_int`、tail.rsのOptionSome/None hook）を介して`Some(scalar)`/`None`を返す。list.getのbodyはこれらcall armを持つheap-result-ifで、既に処理される。get(1)=Some(20)/get(5)=None、first([10,20,30])=Some(10)/last=Some(30)/first([])=Noneはすべてv0とbyte一致。list.get-and-matchを2000回繰り返すloopもbounded（leakなし）。corpus-wall ACCEPTは不変。
**次**：(a) scalar結果を返すarm（`let s = match o { Some(x)=>…, None=>…}`）。(b) heap payload（Some[String]、要素aliasの精密化）。(c) 具体化したOptionの`??`/`!`（unwrap演算子、当時は延期）。(d) string.split / list.map（より難しいList構築/closure機構、機構2/3）。

**Layout（リスク検討済み）：Option = 0または1要素のLIST block**。既存list layout `[rc][len@4][cap@8][data@12]`を再利用する。`None` = 0要素list（len=0）、`Some(x)` = 1要素list（len=1、`data[0]`=x）。新block種も新Initも不要。所有権certは不変：構築は依然1個の`Alloc`（`i`）で、当時のOpaqueと同じ。tag(len)+payload(data[0])のstoreが増えるだけで、所有権の差はない。
- 構築：`IrExprKind::OptionSome{inner}`を捕捉し、1-slot listを`Alloc`して`data[0]=inner`をstore（`[inner]` list literal同様）。`OptionNone` → 0-slot list（`[]`）。当時Opaqueになるbind/tail/arm位置（binds.rsのcatch-all）で行う。
- 分解：`match opt { Some(x) => A, None => B }` → `len`を読む（load32 `handle(opt)+4`）。`len != 0`なら`x = data[0]`を束縛（load64 `+12`）してA、そうでなければB。match lowering（`desugar_match_to_if` / `lower_branch`）を拡張し、`Some(bind)`/`None` patternをtag検査のifにする。
- 最初はscalar payload（Option[Int]）に限定：`x`はscalarで所有権なし、扱いが単純。heap payload（Option[String]）では`x`がOption要素のaliasとなる（`Dup`、container粒度）。これはaliasingの複雑さとして後続に回す。具体化+matchはscalar payloadにgateし、heap payloadのOptionは当時のOpaque/linearizeに残す。
- 敵対的検査：Option[Int]関数のcorpus certは変わる（Opaque→具体化、linearize→実行）。所有権は13139を維持する必要がある（どちらもAlloc `i`、match armはどちらもarmごとに均衡）。検証し反証担当を起動する（borrowされたparamを束縛するSome armは引き続き拒否しなければならない）。

<a id="machinery-2--list-building--floor-done-listint-flowing-liststring-blocked"></a>

## 機構2 — List構築：基礎は完了、List[Int]は流れ、List[String]はblocked
`Init::DynList` + `prim.alloc_list(n)` + `prim.store64`が入った（cert i、DynStr同様）。完了済み（List[Int]、i64 value slot、完全にsoundかつloop-bounded）：**list.reverse / take / drop / slice / range / repeat**。string builder（slice/trim/reverse/replace/replace_first/pad/trim_start/trim_end）も同様にprim.alloc_strを使う。

**List[String]のblocker（split/lines/mapに重要）**：`Op::Drop`はblockに対する平坦な`rc_dec`で、heap要素を再帰的に解放しない。そのため新しいstringを作ってlistに格納する関数（string.split、string.lines、list.mapの結果）は、listのdrop時にstringをleakし、loopはOOMしてloop-bounded規則に違反する。（List[String]に対するlist.reverse/repeatは問題ない。既存要素handleをaliasするだけで新stringを作らず、leakするものがない。）**string.split等には先にnested-heap RCが必要：listのheap要素を再帰的にrc_decするDrop**。certに触れる（containerが要素を所有すること、value-semantics convention / Brick #54をcertが追跡する必要がある）。Drop/renderは要素型を知る必要がある（List[Int]は要素dropなし、List[String]は要素ごとにdrop）。これが次の大きな機構作業で、入るまではList[Int]構築とList[String]のaliasingのみを認める。先にsubstringを数える（2-pass）か、拡張する。certに触れるnested owned heapなので敵対的検査が必要。

<a id="machinery-3--closures-unblocks-mapfilterfoldreducescanfindeachsort_by---hardest"></a>

## 機構3 — Closures（map/filter/fold/reduce/scan/find/each/sort_by等を可能にする）— 最難関
Self-hostは各要素でclosure `f`を呼ぶ必要がある（fがfn-value paramである`f(x)`）。**重要な調査結果：v1 wasm renderにはfunction table / `call_indirect` / `elem`が一切ない**。したがって間接call面全体を一から構築する最大の残作業となる。当時の状態：`IrExprKind::Lambda { params, body, lambda_id }`は`Alloc{Opaque}`にlowerされ（本当の関数ではない）、`Computed { callee }` call（`(g)(x)`）は`is_higher_order`（calls.rs:193）でwallになり延期される。必要な構成要素を順に示す：
1. **Lambda → 実際のMirFunction + table slot。** 各lambda bodyを独立したwasm `func`にする（通常のfnと同様にbodyをlower）。新しいwasm `(table funcref)` + `(elem)`に`lambda_id`をindexとして登録する。captureしないlambdaのclosure値はtable index（scalar i64）。captureするものはcapture値を保持するenv block（`ClosureCreate`/`EnvLoad`経路）も必要で、値は(table_idx, env_ptr)となる。まずcaptureなしから行う。
2. **`Computed` call → `call_indirect`。** fn-value local `f`の`f(x)`は`(call_indirect (type $sig) (args…) (local.get f))`へlowerする。arity/signatureごとに`(type)`が必要。
3. **Higher-orderのwall解除とcaps。** 認めたcaseの`is_higher_order` wallを除く。呼ばれるclosureのcapabilityは不明なので、呼出元fnはcaps-UNVERIFIEDにtaintしなければならない（省略callと同じ正直な扱い）。caps-safeとは主張しない。これでcapsのsoundnessを保つ（`used ⊆ declared`を証明できないので主張しない）。capsが3582以上を維持するか検証する（closureを使うcorpus fnは既にcaps-unverifiedで、regressionは予想しない）。
4. **所有権/規律：** table/elemは静的module構造で、手書きruntime WATではない。lambda関数とtableをinlineにrenderし、新しい`$rc_*`式runtime helperを加えなければ、`handwritten_wasm_runtime_does_not_grow` baselineは変わらない。closure値（scalar table_idx）に所有権はなく、capture envはheap object（i/d）である。
5. **次にlist.map/filterをself-host化**（alloc_list結果 + 各slotで`f(elem)`）。ただしList[String]結果には機構2のnested-heap RCがまだ必要なので、先にList[Int]→List[Int]のmapを入れる。
敵対的検査：higher-orderのwallを除くためcertに触れる。健全性はcaps taintに依存する（closure callをcaps-safeと主張しない）。未宣言Stdout closureをcaps検査に通そうとする反証担当を起動する。複数turnにわたり、stdlib関数より先に基盤（1+2）を実装し、各段階でcorpus-wall caps ACCEPTを検証する。

**健全性の核心（調査で判明した、accept-but-unsafeの穴が潜む一点）**：当時`Computed` callは省略されるので、`count_ir_calls`はir>mirを検出しfnをcaps-UNVERIFIEDにtaintする（closure capsが不明なため正直な扱い）。`call_indirect`をIR Computed nodeと1対1に対応するclean MIR opとしてemitすると、このtaintが消えfnはcaps-VERIFIEDになる。しかしclosureはStdoutへ到達しうるためcaps witnessがそれを見落とす。accept-but-unsafe、すなわち証明済み性質（trust spineの中核）の穴となる。したがって`CallIndirect`をcaps-cleanにしてはいけない。ir>mir taintを維持する（1対1に数えない）か、`cap_witness`が`CallIndirect`を全capability使用として扱う（保守的な`used ⊇ all`）必要がある。後者では全capsを宣言するfnにしか`used ⊆ declared`は成立せず、CallIndirect fnが黙ってcaps-verifiedになることはない。corpus-wall `caps`は3582を維持しなければならない（closure利用corpus fnは既にcaps-unverifiedで、正しい実装はその状態を保つ。3582未満への減少も偽の増加もtaintの誤りを示す）。このcaps taintはキャンペーン全体で最も危険な一点で、まず単独で実装し敵対的に検証する。

<a id="known-writing-idiom-gaps-use-the-workaround-now-fix-centrally-later"></a>

## 既知の記述idiomの欠落（今は回避策、後で中央で修正）
BinOp operandとしての`if`（`n + (if c …)`）、call引数としてのscalar call（`f(g(x))`）、if条件としてのBool call（`if is_ws(b) …`）はいずれもlowerできない。先に束縛する（`let t = …; …t`）。一般的なloweringの欠落（scalar-if-in-operand、scalar-call-in-arg/cond）であり、中央で修正する価値がある。
