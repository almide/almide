<!-- description: The retired v0 wasm emitter's Try/Unwrap/Fan early-return heap leak, kept as the historical record of the class -->
<a id="v0-wasm-codegen-tryunwrapfan-early-return-heap-leak"></a>

# v0 wasm codegen：Try/Unwrap/Fanの早期returnによるheap leak

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](v0-unwrap-early-return-leak.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](v0-unwrap-early-return-leak.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](v0-unwrap-early-return-leak.zh-CN.md)

> 非規範的な履歴参照資料。固定原文を正本とし、自動同期しない。測定・証明 gate・現行実装の検査を独立に再実行していない。引用コマンドは実行しない。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v0-unwrap-early-return-leak.md)
> 退役v0の履歴バグ記録。冒頭の実装・検証済み状態と以前の推奨方式／Why not now節が併存する段階を保持し、新しい修正や再実行とは主張しない。最終状態のemitter側実装とPerceus注記の推奨は異なり、現行実装を推測しない。leak/double-freeに関する保証は明示的除外・pass順条件付きの当時の範囲に限り、普遍的memory安全性ではない。旧no-return/auto-?言語主張は現行構文指示ではない。

**状態**：実装＋検証済み（emitter側`emit_early_return_decs`）。v1 wallは解除、−59分を回復（in-profile 4083）。leakなし（100k err loopが完了）＋double-freeなし（260file wasm corpusがgreen）。
**発見**：2026-06-14、v1 trust-spineのadversarial-verify workflow（`wf_54ac302a`）が`(あ)`小規模lowering欠落の層を走査した際。
**重大度**：MEDIUM — leakであり、use-after-free / double-freeではない。wasm linear memoryはinstance破棄時に全体を回収するため、短いprogramではコストが有界で、error早期return経路を繰り返し通る長時間processでのみ無界になる。通常のOk経路は影響なし。

<a id="the-bug"></a>

## バグ

`effect fn`では、`x!`（`IrExprKind::Unwrap`）、effect-fnのauto-`?`（`IrExprKind::Try`）、`Fan`のauto-unwrapが、すべて早期returnで`Err`を伝搬する。v0の**wasm** emitterはこのErr経路をcleanupなしの早期returnとしてrenderする：

```
// v0's emit_wasm/expressions.rs (deleted with #782 — read via git history)
//   Unwrap/Result : 846   local_get(scratch); return_;
//   Unwrap/Option : 826   ... return_;
//   Try           : 786   ... local_get(res); return_;
//   Fan single    : 715   ... return_;
//   Fan multi     : 749   ... return_;
```

関数のheap localごとの`rc_dec`による解放は、Perceus passが末端`Ret`/`Nop`にだけ挿入する（v0の`pass_perceus.rs:389-441 insert_decs_before_ret` — #782で削除、git履歴参照）。早期`return_`は関数末尾への無条件制御移動であり、それらのdecをすべて飛び越す。そのため、早期return時点ですでに束縛され、まだliveなheap localはErr経路でleakする。

**Rustにはこのleakがない**：`x!`はRustの`?`としてrenderされる（`crates/almide-codegen/src/walker/expressions.rs:710-753`）。早期`return Err(..)`がscope内の全localのscope-exit `Drop`を自動実行するためだ。つまりこれは**wasmのみ**のcodegen leakである。

**`break`/`continue`は修正済みなのにTry/Unwrapは未修正だった理由**：`flatten_exit_tail_blocks`（pass_perceus.rs:661-704）は末尾にある`continue`/`break`を持ち上げ、`insert_decs_before_ret`が末端として扱って、その前にdecを出すようにする。対象は`Continue`/`Break`（`tail_block_ends_in_exit`）かつ末尾位置のみ。`Try`/`Unwrap`/`Fan`は式途中の値で、block末尾ではないため、末尾への昇格は適用されない。さらにdesugar先となる**`IrExprKind::Return` nodeがない**（Almideには`return` keywordがない）ため、Perceusの既存return-dec logicに見える明示的IR制御フローnodeへ早期returnを変換できない。

<a id="v1-trust-spine-interim-done-commit-4fbbefe7"></a>

## v1 trust-spineの暫定対応（完了、commit 4fbbefe7）

v1 MIR loweringは、owned heap localがliveな場合の`Try`/`Unwrap`をwallにする（`expr_has_early_return` + `!live_heap_handles.is_empty()`、`lower_stmt`/`lower_tail`のguard）。これにより証明済みcheckerがleak形状を認定するのを防ぐ（さもなければ`no_leak`節について**acceptだがunsafe**になる）。代償：in-profile 4081→4022（−59）。**このv0修正で59件を回復する** — wasm renderにleakがなくなればdeferred-continue certが再び忠実になるため、wallを解除できる（`live_heap_handles` guardを削除）。

<a id="the-core-constraint-per-exit-liveness"></a>

## 核心の制約：exitごとのliveness

早期return点ごとにliveなheap localの集合は異なる（それより前に束縛され、まだmove/decされていないlocal）。全heap localをdecする単一の関数epilogueでは、早期経路で未束縛のものをdouble-freeするか、すでにmove-outされたlocalをdecしてしまう。したがって、そのexitでliveな集合だけを正確にdecしなければならない。その集合の唯一の正本は**Perceus liveness解析**であり、emitterが再導出してはならない（emit層のownershipは手書きで解析からdriftする。独立したemitter live集合は典型的なdouble-freeの原因）。

<a id="recommended-approach-perceus-annotates-emitter-emits"></a>

## 推奨方式：Perceusが注記しemitterが出力する

1. **Perceus**（pass_perceus.rs）は既存liveness pass中に、各`Unwrap`/`Try`/`Fan` nodeでliveなheap-local `VarId`集合を計算する（それ以前にallocされ、まだdecされていない — 末端dec配置ですでに追跡しているものと同じ）。安定したnode IDをキーとするside-table、またはUnwrap/Try/Fan IR node上の新しい`Vec<VarId>` fieldで付与する（IR形状変更になるため、全IR consumerに触れずに済むside-tableを推奨）。
2. **wasm emitter**（emit_wasm/expressions.rs）は、Unwrap/Try/Fan Err経路に出す各`return_`の前に、そのnodeのlive集合に属する各`VarId v`について`local_get(v); call(rc_dec)`を出す（side-tableから検索）。返却中の値（Err Result ptr）はlive集合には含まれない（結果であってlocalではない）ため、二重decされない。
3. **native emitter**：変更なし（Rustの`?`はすでにDropを実行するので、decを追加するとdouble-free）。

これはdriftなし（live集合はemitterではなくPerceusのもの）で、局所的な変更でもある（emitterは渡された集合を出力するだけ）。

<a id="alternatives-considered-rejected"></a>

### 検討した代替案（不採用）

- **emitter自身によるlive-local追跡**（emitterが出すAlloc/Decごとに集合更新）：driftしやすい。emit層はすでに解析からずれ、誤集合はdouble-freeを起こす。不採用。
- **`IrExprKind::Return` nodeを追加し、Perceus前にUnwrap→`match{Ok=>v,Err=>return}`へdesugar**：原則に沿うが、中程度のwasm leakに対して大きな言語／IR変更（新node、全visitor/consumer、両backend）となる。釣り合わず、現時点では不採用。
- **`br`経由の単一cleanup epilogue**：exitごとのlivenessは依然必要（なければdouble-free）。核心の制約を避けられず、簡単にはならない。

<a id="test-strategy-mandatory-before-shipping--rc-critical"></a>

## テスト戦略（出荷前に必須 — RC上重要）

1. **fixture** `effect fn f(s) { let big = make_heap(); let n = parse(s)!; use(big,n) }`を`--target wasm`でcompileし、Err経路が`big`をdecすることをassert（wat2wasm traceで`return_`前に`big` localの`rc_dec`が見える）。
2. **double-freeなし**：unwrap対象自体が唯一のheap上のもの（他のlive localなし）のfixtureでは、Err経路に追加decを出してはならない（result ptrは返すのであって解放しない）。
3. **cross-target byte安定性**：nativeには影響せず、wasm対native差分gate（`spec/wasm_cross/`）はgreenを維持する（native出力不変）。
4. **`--target wasm`で全`almide test`**＋wasm determinism harness。double-freeはtrapするがleakは無言なので、fixture harnessにleak数assert（alloc/free収支）を加える。
5. **v1 trust-spineを再実行**し、`live_heap_handles`のUnwrap/Try wall（lower_stmt/lower_tailの`expr_has_early_return` guard）を解除する。corpus-wallは3性質すべてACCEPTを維持し、in-profile ≈ 4081へ戻るはず。

<a id="verified-algorithm-workflow-wf_dbf7590c-6-agents"></a>

## 検証済みアルゴリズム（workflow wf_dbf7590c、6 agents）

adversarial-verify workflowが重大リスクを解決し、正確なアルゴリズムを固定した：

- **moveによるdouble-freeはない。**Perceusモデルは*alias-by-RcInc、move-nothing-user*。list/record/tuple/concat/callへ渡すheap localは、渡すpointerへの`RcInc`で共有され、元は自身の末端Decを保持する（emit側「SHARE dup」、expressions.rs:1265-1308）。対応する`FnBody::Dec`なしに、user heap VDeclが関数途中で消費されることはない（`collect_heap_vdecls`が全heap VDeclを列挙し、`insert_decs_before_ret`が各々に末端Decを与える）。したがって`vdecld-minus-decd`はまだownedなlocalを`live`に保つ — それこそerr経路でdecすべき集合である。`collect_moved_out_vars` verifier（pass_perceus.rs:880-910）が「moved」と認識するのはbare-Var block末尾とfor-in-iterableだけで、どちらもleak診断の免除でありDecの削除ではない。⟹この修正全体の最悪の失敗はdec漏れ（leak）であり、double-freeではない — 安全側の方向。

- **live集合は外包chain-stack全体の和集合でなければならない。**各Block、`while`/`for-in`本体、`if`/`match` arm本体は独自の`FnBody` chain（`block_to_fnbody`）であり、外側chainの`collect_heap_vdecls`から見えない。早期`return_`は関数全体を抜けるため、nodeのlive集合＝そのnodeから関数rootまでの全外包chainについて、（そのchain内でnode前に束縛されたheap VDecl）−（その前のheap `Dec`）の和集合。単一の平坦walkでは外側scopeと反復ごとのlocalがleakする（HIGH重大度の2反証holeで確認 — 両方leakで、破壊ではない）。入れ子chainを処理する同じ再帰の中、pass_perceus内で計算し、各nested-chain walkへ外側のlive集合を渡す。

- **除外（既存述語を厳密に写さないとdouble-freeの危険）：**
  (1) `__tco_`/`__br_`/`__perceus_*` temp — insert_decs_before_ret:414-418、Assignのmove免除:309-315と同じprefix検査（自分のrefを渡し、Decを受けないため、decするとuse-after-free）。
  (2) `EnvLoad`に束縛されたborrow local — collect_heap_vdecls:358と同じ`!matches!(expr.kind, EnvLoad{..})` guard（closure envが所有する）。
  (3) 束縛途中の変数 — `let n = …!`のinitializer内のTry/Unwrapには`n`を含めてはならない（式の評価後に初めて束縛される）。
  VarIdで重複排除する。

- **返すErr値は`ScratchAllocator` temp（生のwasm localでVarIdなし）であり、live集合の要素にはならない** → result ptrは二重decされない。

- **実装方法**：node上のfield（Unwrap/Try/Fanの`early_return_decs: Vec<VarId>`）は約13のOR-pattern arm＋15constructorに変更が必要だが、live集合がnodeと一緒に移動する（key付け不要）。関数ごとのpre-order node indexをキーとするside-tableは1structで済むが、Perceusからemitの間のpassがTry/Unwrap/Fan nodeを並べ替え／追加／削除しない場合にのみ健全（keyの不一致で誤集合を取るとdouble-free）。Perceusのpass位置で選ぶ：emit直前の最後のIR passならside-table indexは安定し安価、そうでなければ安全性のためfieldを選ぶ。

- **Emit**（expressions.rs約706/770/798、早期`return_`の3箇所）：live集合の各（VarId v）をvar_map＋var_tableでv →（local L、ty T）へ解決（statements.rs:358-392のRcDec handlerと全く同じ）。`local_get(scratch); return_`の前に`local_get(L);
  emit_typed_rc_dec(&T, L)`を出す。`scratch`を壊してはならない。

- **入れ子を明示的にテスト**する（平坦walkのholeはfixture #1だけでは不足することを示す）：`while`/`for`本体内および`if` arm内のUnwrapについて、それをまたいでliveな外側scopeのheap localをそれぞれ用意し、Err経路で外側localがdecされることをassert（alloc/free収支）。関数最上位だけの平坦fixtureは、chainごとの集合が誤っていても通る。

<a id="why-not-now"></a>

## なぜ今ではないか

手書きでdriftしやすいwasm emit層へのRC上重要な変更。誤ったlive集合はtrust spineだけでなく出荷済みwasm programすべてにdouble-free（memory corruption）を起こす。目標設定時の「専用セッション向き」という指摘どおり、十分に休んだ新しい専用セッションで、上記の全テスト群とともに取り組むに値する。その間もv1の暫定wallでtrust spineは正直に保たれる。
