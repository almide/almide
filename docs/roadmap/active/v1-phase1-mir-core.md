<!-- description: Almide v1 Phase 1: MIR core + dual renderer — implementation design following the Phase 0 decision gate -->
# Almide v1 Phase 1: MIR コア + 二レンダラ — 実装設計

Status: active — 設計(design-first)。Phase 0 決定ゲート合格(5/5)を受けた本体設計。実装着手前のレビュー対象。
Owner: compiler
Parent: [v1-mir-architecture.md](./v1-mir-architecture.md)(憲法・正準形・Phase 計画)
Grounding: 既存コンパイラの所有権/レイアウト決定を6コンポーネント精読でマップした実測(本ドキュメント §1 の file:line は全て検証済みアンカー)。

---

## 0. このドキュメントの位置づけ

[v1-mir-architecture.md](./v1-mir-architecture.md) が **何を・なぜ**(憲法: 「所有権とレイアウトの決定は MIR で1回だけ。レンダラは再現するだけ」)を定める。本ドキュメントは Phase 1 の **どこを・どの順で・どう検証して**を、既存コードの実測マップに接地して定める実装設計。

**Phase 0 で確定したこと**(§8.1, `research/spike/v1-mir/GATE.md`): 5つの所有権が厄介な形すべてで「単一 Perceus 決定 → 両イディオムが構築で一致」、escape hatch 0。正準形の境界4点(per-binding 極性 / 構文 vs 意味 / value-等価検証 / 共有可変の縁)も判明。**Phase 1 はこのテーゼを言語全体スケールで、既存オラクルを使って実証しながら負債を畳む。**

---

## 1. 現状マップ — 「意味論の実装が2つある」の実体

精読で確定した最重要事実: **高 IR は既に「半 MIR」である。**

- `IrStmtKind::RcInc/RcDec` は**既に IR ノード**(almide-ir/lib.rs:723)。Perceus が IR レベルで dup/drop を置いている。§2.2 の「dup→__rc_inc / drop→__rc_dec」行は文字通り `emit_wasm/statements.rs:329/358`。
- `engine/layout.rs` の `LayoutRegistry`(:77)は**既に §2.1 Repr テーブルそのもの** — STRING/LIST/SWISS_MAP/SET/VARIANT/OPTION/RESULT を field offset(Fixed/AfterDynamic)+ MemType width + header_size で持ち、**このモジュール外にマジックナンバー0**(`list_layout.rs` は薄いアクセサ)。

→ Phase 1 は「ゼロから新 IR を建てる」のではなく、**既に中央化された事実を MIR 値に昇格し、散在する決定を1パスに畳む**作業。

### 1.1 5つの所有権パス + レンダラ再決定 → 1つの事実

| 現在のパス/サイト | ターゲット | 決めている事実 | MIR の担い手 | 出典(検証済み) |
|---|---|---|---|---|
| **PerceusPass** | wasm専用 | fresh-vs-alias 極性 → RcInc/RcDec 配置 | `Dup`/`Drop` ノード(既に IR) + per-binding 極性 | pass_perceus.rs:728 `yields_borrowed_alias`(全網羅・wildcard無), :612 `is_heap_type` |
| **BorrowInferencePass** | Rust専用 | param 極性 {Own/Ref/RefStr/RefSlice/RefMut} + 呼出側 Borrow 挿入 | 関数境界の per-param 極性 {own/borrow/borrow_mut}(Ref系は Repr 由来の**綴り**) | pass_borrow_inference.rs:497 is_heap_type, :516 check_needs_ownership, :371 is_derive_fn(@derived) |
| **CloneInsertionPass** | Rust専用 | last-use → move、それ以外 → Clone(=Rust の dup) | per-use 極性 `Dup`(非last) / `Consume`(last=move) | pass_clone.rs:141 split_clone_ids, :223 last-use move gate |
| **CaptureClonePass** | Rust専用 | capture 極性: Dup-into-env / value-copy / 共有セル | closure env field ごとの極性 + 共有可変ノード | pass_capture_clone.rs:100 detect_shared_mut, :448 wrap_lambda_with_clones |
| **AliasCowPass** | wasm専用 | needs_cow(別名可変+変更) → `__cow_check` | `MakeUnique` ノード | pass_alias_cow.rs, annotations.rs:85 needs_cow |
| **BoxDerefPass** | Rust専用 | 再帰型のどの field が box か → Deref 挿入 | `Repr::Boxed{layout}`(box性は MIR 事実、`&**deref` は綴り) | pass_box_deref.rs:132 find_recursive_enums, :18 Rust-only |
| **emit_typed_rc_dec** ⚠ | wasm専用・**レンダラ内** | 複合型の再帰 drop schedule(どの子ptrが owned・解放順) | **平坦化された `Drop` ノード列**(§3.4) | emit_wasm/statements.rs:734-939, :724 is_heap_type(`Mirrors pass_perceus::is_heap_type`) |

### 1.2 「二実装病」が具体的に見える drift 座標(Phase 1 が殺す対象)

精読が見つけた**同一事実の重複実装**(憲法違反の実体):

1. **`is_heap_type` が2コピー**: `pass_perceus.rs:612` と `emit_wasm/statements.rs:724`(後者のコメントが明示的に「Mirrors pass_perceus::is_heap_type」)。手で同期。
2. **共有可変検出が2実装**: `detect_shared_mut`(Rust, capture時) と `detect_mutated_captures`(wasm, ClosureConversion時)。構築でしか一致しない。
3. **variant tag 列挙が2箇所**: `mod.rs:1338` と `equality.rs:179`(両方 `enumerate()` index)。順序がズレれば eq が静かに壊れる。
4. **再帰判定が3箇所**: find_recursive_enums / walker annotation / wasm の自然なポインタ扱い。
5. **emit_typed_rc_dec が Ty を再walk**して drop schedule を**レンダラ内で再決定** — これが §643 クラスの本丸。`PerceusVerify`(IR の RcInc/RcDec しか見ない)にも byte gate(`verify_rc_balance` は `wasm_dec ≥ ir_dec` しか見ない、over-count を捕捉できない、mod.rs:1858)にも**不可視**。

> §643 が「heisenbug で検出をすり抜けた」のは偶然ではない。**所有権決定の一部がそもそも両検証器の射程外(レンダラ内のハンドコード)にある**から。Phase 1 はそれを MIR ノードに引き上げ、射程内に入れる。

---

## 2. MIR コアの定義

### 2.1 値モデル: `Repr`

すべての MIR 値/束縛に `Repr` を付す。`LayoutRegistry` + `byte_size`(values.rs:58)+ `find_recursive_enums` を**値の性質**に昇格したもの:

```
Repr =
  | Scalar { width }                  // Int=8, narrow=1/2/4, Bool=4, Float=8 — RC 不要、Dup/Drop 無
  | Ptr    { layout: LayoutId }       // String/List/Map/Set/Record — RC 管理、layout は offset/header/stride を持つ
  | Boxed  { layout: LayoutId }       // 再帰サイクル member field — Rust:Box<T> / wasm:bare ptr
```

- `layout: LayoutId` は `LayoutRegistry` の `MemLayout`(header_size, 順序付き MemField の FieldOffset::Fixed/AfterDynamic, MemType, elem_stride)を指す。**field offset は MIR の lookup**になり、walker の `byte_size` 累積も wasm の `4+foff` リテラルも消える。
- variant は layout に `tag_value`(case 宣言順 index, **1箇所**で決定 → mod.rs:1338 と equality.rs:179 の二重計算を撲滅)と `tag_offset`(現状固定0)+ `payload_layout` を持つ。将来の niche/tagless 最適化が**1箇所**の変更で済む。
- `clone_free`(top_let_storage.rs:67)⇔ `Repr::Scalar`。#531 の4射影テーブルは `Repr` 1つに畳まれる。

**前提条件(硬性)**: Repr は完全に concrete でなければならない。`is_heap_type` の `Ty::Unknown ⇒ heap`(pass_perceus.rs:620)/ `byte_size` の Unknown→4 fallback(values.rs:75, #525)は **silent miscompile の温床**。→ `AllTypesConcrete` ゲート(現状 pipeline 後の `assert_types_concretized`)を **Core→MIR の前**に移す(§4)。

### 2.2 所有権ノード集合

Phase 0 スパイクのノード集合を言語全体に拡張:

```
Alloc { repr }                 // ヒープ確保(+ field store)
Store / Load { offset }        // layout 経由のフィールド読み書き
Dup    v                       // +1 owned ref(Perceus dup) — wasm:__rc_inc / Rust:.clone()
Drop   v                       // -1 ref(以後未使用) — wasm:__rc_dec / Rust:スコープ末 Drop
Borrow v  (mut?)               // 消費せず参照 — wasm:ptr透過 / Rust:&v / &mut v
Consume v                      // 最後の使用=所有権移動 — wasm:ptr転送(inc無) / Rust:move
MakeUnique v                   // 共有され得る可変への変更前(§2.5)
Call ...
```

**不変条件**: `Dup`/`Drop`/`Borrow`/`Consume`/`MakeUnique`/`Repr` は **MIR が1回決める意味的事実**。レンダラがこれらの**数や極性を変えたら所有権の再決定=バグ**(§3 の lint)。

### 2.3 per-binding / per-parameter 極性(gate finding #1)

所有権の決定単位は「束縛/引数ごとの極性」:

- **関数境界**: 各 param に `own | borrow | borrow_mut`。`ParamBorrow` の Ref/RefStr/RefSlice は **MIR 事実ではない** — borrow × param の Repr から**綴りが落ちる**(`Ptr{String-layout}` の borrow → `&str`、`Ptr{List-layout}` の borrow → `&[T]`)。fixed-point sig 伝播(caller の極性は callee の極性に依存, pass_borrow_inference.rs:95)は **Core→MIR 内の dataflow** として1回走る。
- **各 use 点**: `Dup`(clone 数 >1)/ `Consume`(last use=move)/ `Borrow`。pass_clone の `eligible` last-use==0 move = 「カウント静的に1 → Perceus の affine 特殊形」、`always` 類(top-let/static/loop-bump)=「カウント静的に >1、Dup 必須」。

> 同じソース束縛でも consume なら move、borrow なら clone/dup。これは**レンダラの再決定ではなく MIR が束縛/引数ごとに1回決める**。

### 2.4 平坦化された drop schedule(最大の獲物)

`emit_typed_rc_dec`(statements.rs:734-939)が emit 時に Ty を再walk して「List/Set 要素・Map の key/val 別・Result/Option payload・record field・closure env」の再帰 drop を**レンダラ内で再決定**している。これを **Core→MIR が drop ごとに完全な所有権ツリーを1回決め、平坦な `Drop` ノード列**(owned sub-pointer 1つにつき1 Drop, 解放順込み)に落とす。

結果:
- wasm の RcDec arm は **1行**(`call __rc_dec`)、Rust はスコープ末 Drop。**どちらも Ty を再walk しない**。
- `is_heap_type` の重複(座標①)が**死ぬ**。
- §643 クラスの「レンダラが drop を再決定」が**構造的に不可能**になる。

⚠ **最高価値かつ最高リスク**: この再帰 drop(Map-entry/closure-env/nested-Named)は **corpus カバレッジが最も薄い**(§5 で専用 fixture を先に追加)。

### 2.5 共有可変ノード `MakeUnique`(gate finding #4 の縁)

**重要な実測**: 共有可変の escape hatch は**現状すでに production に存在する** — `detect_shared_mut` の非Copy枝が `SharedMut<T> = Rc<RefCell<T>>`(lib.rs:481)を、Copy枝が `Rc<Cell<T>>` を吐く。Phase 0 ゲートが「read-only capture は Rc ゼロ」と出したのは**形が違う**から(矛盾ではない)。

→ MIR の `MakeUnique` / 共有可変は **Cell(Copy, 借用越し変更なし)と RefCell(非Copy, `list.push` 等で in-place `&mut` 借用され得る)を区別する Repr 事実**を持つ必要がある。`needs_cow`(wasm cow_check)と Rust の clone-at-mutation は**この1ノードに統合**され、alias_cow.rs が「1決定」であることを実証済み。

---

## 3. 単一 Core→MIR パスと忠実なレンダラ契約

### 3.1 1つのパスが全部決める

§1.1 の6行(5パス + emit_typed_rc_dec)を **1本の Core→MIR 所有権+レイアウトパス**に統合。出力は MIR(Repr 付き値 + §2.2 ノード + per-param/use 極性)。PerceusPass(is_heap_type × yields_borrowed_alias)が既に正準 fresh/alias 分類器なので**ここを出発点**にし、Rust 側決定(clone/borrow)も同じ事実から導く。

### 3.2 忠実性契約を「操作可能な lint」にする

§3 の憲法を実装で守らせる:

> **どのレンダラも `is_heap_type` / `yields_borrowed_alias` / last-use / 再帰判定を自分で計算してはならない。MIR の事実を読むだけ。計算したら #643 再決定バグ。**

- **構文(SYNTACTIC)の差は吸収してよい**(gate finding #2): box-pattern 不在 → tag-guard + `&**deref`(BoxDeref は v1 では**レンダラの仕事**であって Core→MIR パスではない)、`&str` vs `&[T]` の選択、`__hoist` 一時、ANF lifting、match 展開、IterChain fusion。
- **意味(SEMANTIC)の決定は不可**: `Dup/Drop/Borrow/Consume/Repr/MakeUnique` の数・極性を変えたらバグ。
- 実装: レンダラから上記述語の**呼び出しを削除**し、MIR フィールド読み取りに置換。残存呼び出しを CI で grep 禁止(忠実性 lint)。

---

## 4. パイプライン挿入点(canonical cut)

精読が見つけた**非対称**: Rust path(main.rs:484)は `optimize_program`/DCE を**呼ばない**(ConstFold のみ)、wasm path(build.rs:294)は mono の**前**に呼ぶ。5つの所有権パスは mono/DCE/ir_link の**後**、nanopass Pipeline 内で走る。

→ Core→MIR は **両ターゲット共通の1つの cut**(mono + ir_link + DCE の後、いかなる所有権パスの前)に置く。前提:
1. **`AllTypesConcrete` を前倒し**(Repr が concrete 型を要求, §2.1)。今は pipeline 後の assert。
2. compile-order 非対称を解消(両 path が同じ Core IR を Core→MIR に渡す)。
3. ハード順序制約(BorrowInsertion→CloneInsertion / ANF→Perceus / StackBalance→Perceus / Canonicalize 終端)は**1パス内部の段**として再現(ANF/heap可視性は正しい Drop 配置の前提)。

---

## 5. 移行戦略 — オラクル駆動の脱リスク順序

**書き直さない。終状態を北極星に、既存 byte gate + corpus をオラクルに収束しながら旧実装を剥がす**(§6)。依存関係と「賭けを早く安く検証する」で順序を決める:

| 段 | 内容 | リスク | オラクル | なぜこの順 |
|---|---|---|---|---|
| **1. Repr-on-values** | LayoutRegistry+byte_size を MIR 値に昇格。AllTypesConcrete 前倒し。挙動不変(Repr が ride するだけ) | 低(中央化済・マジックナンバー0) | byte 一致(挙動変化なし) | drop 平坦化の前提。最も機械的 |
| **2. drop schedule 平坦化** | emit_typed_rc_dec → 平坦 Drop ノード列。wasm RcDec を1行化、is_heap_type 重複撲滅 | **高**(corpus 最薄、§643 本丸) | **専用 compound-drop fixture を先に追加**(Map-entry/closure-env/nested-Named)+ byte gate + corpus replay | ここで賭けが真に試される。早期に falsify 可能 |
| **3. 極性統一** | per-use(Dup/Consume)+ per-param(own/borrow)を1パスが両ターゲット分生成。pass_clone/borrow_inference/perceus を MIR 読取に refactor | 中(2実装の保守性差) | **value-等価必須**(byte gate)+ 型不一致は compile error で別途捕捉 | renderer が dumb 化する核心 |
| **4. capture+MakeUnique 統一** | detect_shared_mut/detect_mutated_captures → 1分析。needs_cow + clone-at-mutation → 1 MakeUnique(Cell/RefCell Repr 区別) | 中(縁・2検出器 drift) | value-等価 + host-determinism(BTreeSet 順序) | finding #4 の縁を明示ノード化 |
| **5. Rust レンダラ MIR 化** | Rust 側も MIR 消費に切替。両レンダラ dumb 完成 | 中 | 全 corpus byte/value 一致 | §7 の「次に Rust レンダラを切替」 |

### 5.1 検証 oracle の二層構造(完全性の核心 — 必読)

**unify は差分テストが頼る独立性そのものを手放す**。byte gate(native↔wasm 一致)が効くのは2つの**独立した**実装が偶然一致しにくいから。v1 では両レンダラが**1つの MIR から**出る → **Core→MIR の決定が間違うと全ターゲットが同一に間違い、native↔wasm 一致ゲートはそこで盲目**になる(両方 agree する)。これは仮説でなく Phase 0 alias_cow(§8.1-③)で実証済み: 「MakeUnique を省くと両イディオムが**同一に** `a` を破壊し RC はバランスのまま」= 共有決定バグは差分テストに不可視。

→ 完全性は byte gate だけには**絶対に乗らない**。還元ステップごとに、**検証対象から独立な oracle** を置く:

| ステップ | 保つ性質 | 独立 oracle | byte gate で覆える? |
|---|---|---|---|
| **Core→MIR(決定)** | source 意味を保存 | **意味法則の property test + 翻訳検証(#570)** | ❌ **覆えない(本最前線)** |
| MIR→wasm | MIR 小ステップに忠実 | interp 一致 + wasm-spec 忠実性証明 | △(移行中は旧 emit が独立版) |
| MIR→Rust | MIR 小ステップに忠実 | 移行中=旧 emit 差分、移行後=interp | △ |

**2行目の oracle(Core→MIR 正しさ)は本 Phase の load-bearing な新規作業**。byte gate / interp は renderer 忠実性用で、Core→MIR バグには無力。具体策:
- **意味法則の property test**(fixture でなく**法則**を proptest 化): 値意味論「`b=a; mutate(b)` で `a` 不変」、所有権不変量(dup/drop バランス、drop 後 use-after-free 無し、Consume 後の元 var 不参照)を**ソースレベルの真**として性質検査。第2実装を要さず共有決定バグを**上から**捕まえる。
- **interp は renderer バグは捕るが Core→MIR バグは捕らない**(間違った MIR を忠実に実行すれば間違った結果に**一致**する)。だから interp は MIR→target 用、Core→MIR には property+proof が要る。
- **翻訳検証証明(#570)**: Core→MIR の所有権決定が意味を保存する証明。Perceus 健全性の Lean は既に core にある → MIR 小ステップに対し述べ直す(Phase 2 で本格化、Phase 1 では property test で先行)。

### 5.2 renderer 忠実性ゲート(byte gate 群 — 既存資産)

- **value-等価(byte gate)が load-bearing**(renderer 層で)。leak/RC-balance だけでは**不足** — AliasCow 回帰は RC バランスのまま wrong-output(alias_cow.rs で実証)。Perceus-belt 式 RC 検証器だけ積んでも value 破壊を通す。
- **`new MIR-wasm == old emit_wasm`** を `wasm_cross_target_spec`(tests/wasm_runtime_test.rs:471, 両ターゲットで (exit,stdout,stderr) triple 比較)で。移行中は**凍結した旧 emit_wasm**を基準に broader corpus を replay(旧 emit が**独立版**として機能するのは移行中だけ — §5.3)。`@xt-allow` ratchet は down-only。
- **host-determinism**(check-host-determinism.sh): MIR パスが drop/cow 順序に HashMap 反復を入れると value gate は通るが host gate が割れる。BTreeSet 順序を維持。
- **heisenbug 対策**: corpus replay は必要だが歴史的に不十分(§643)。targeted drop-order fixture を併用。
- **同一 `ALMIDE_WASM_FREES`** を新旧両方で(leak body と RC body を比べて phantom divergence を出さない)。

### 5.3 退役順序の不変条件(危険な落とし穴)

親 §6 は「忠実性が立ったら byte gate を退役」と書くが、**順序が命**: 差分 oracle(旧 emit との差分 / native↔wasm 一致)を消してよいのは、**上位の独立 oracle(interp + property laws + wasm-spec 忠実性)が先に置き換わった後だけ**。さもないと退役の瞬間に Core→MIR 決定バグへの盲点が口を開く。**「差分を消すなら、その前に上位の独立 oracle を立てる」**を移行の硬性不変条件とする。

---

## 6. リスク台帳(接地済み)

| # | リスク | 出典 | ゲート/緩和 |
|---|---|---|---|
| R1 | emit_typed_rc_dec の Map-entry/closure-env/nested-Named drop が corpus 最薄。全 149 wasm_cross fixture を通っても未テスト複合 drop で drift | statements.rs:734-939; wasm_cross=149本(270 は broader corpus) | §5段2前に専用 fixture。MIR 化が射程内に入れる |
| R2 | name-string 所有権チャネル `__tco_`/`__br_`/`__perceus_*`(donate-vs-share)が naming 変更で静かに反転(7→55MB churn 回帰の実績) | pass_perceus.rs:309-315 | MIR が move-vs-dup を**明示事実**で持つ(name 非依存) |
| R3 | TCO が BorrowInsertion 後に param borrow 注釈を**剥がす**(owned loop var 化) | pass_tco.rs:1142 | loop 変換=owned を極性決定の**前**に。byte gate は E0308 で捕捉(value 等価では不足な例) |
| R4 | @intrinsic/@inline_rust テンプレートが borrow を綴る。MIR-決定 borrow と衝突し `&*&*` | pass_borrow_inference.rs:308 | runtime self-host(Phase 3)まで seam は脆い。移行中は template 側を Own 固定で隔離 |
| R5 | is_derive_fn/mono 除外(全Own)を落とすと cross-module owned-arg 呼出が E0308 | pass_borrow_inference.rs:371(@derived, #647) | MIR に「境界 fixed-own」属性 |
| R6 | in-place-mutation-counts-as-use の手維持対称性が desync すると use-after-move(E0382) | pass_clone.rs:116 | MIR drop/dup 配置が同一会計を再現 |
| R7 | module top-let は static(move 不可, E0507)→ always-clone | pass_clone.rs:156 module_origin | Repr/極性で borrow-only/Dup-always マーク |
| R8 | 再帰判定の over-inclusion は Rust では無害(余分 Box)だが wasm alloc/RC を駆動すると mis-size | walker/helpers.rs:39 | Boxed の意味を両レンダラで一致させる。byte gate |
| R9 | variant_alloc_size の 4+max_payload padding(wasm)と rustc enum layout(native)非対称。素朴 Repr が mem_eq を壊す | equality.rs:1272 | value-等価ゲート |
| R10 | mutable_captures は plain rc_dec(cell ptr を object として walk すると garbage decref → wasm trap) | statements.rs:360-368 | MIR が「cell, drop plain」事実を持つ |
| R11 | 段2-5 の cutover 中、旧 emit_typed_rc_dec と新 MIR Drop が共存 → is_heap_type 2コピーが live に drift | statements.rs:724 | cutover を fixture 単位で原子的に。両経路 byte 一致を常時 assert |
| R12 | 1つの極性ミスが corpus に fan-out(検証テール = §7 最大コスト) | — | 段ごとに corpus 全 re-green。賭けは段2で早期判定 |
| **R13** | **unify が差分独立性を消す → Core→MIR 決定バグが全ターゲットを同一に間違わせ、native↔wasm 一致ゲートが盲目に**(alias_cow §8.1-③ がカナリア) | §5.1 | **property-law oracle(値意味論/所有権不変量)を第一級ゲートに**。退役順序の不変条件(§5.3)。byte gate は renderer 忠実性専用と割切る |

---

## 7. Phase 1 完了の定義(exit criteria)

1. 単一 Core→MIR パスが §1.1 の6決定すべてを生成し、**両レンダラから `is_heap_type`/`yields_borrowed_alias`/last-use/再帰判定の呼び出しが消える**(忠実性 lint が CI で緑)。
2. emit_typed_rc_dec が削除され、wasm drop が平坦 Drop ノードの1行レンダリングになる。
3. 座標①〜⑤の重複実装が単一事実に統合される。
4. 全 corpus(native↔wasm)が **value-等価**で緑、host-determinism 緑、`@xt-allow` が増えていない。
5. **Core→MIR 決定の property-law oracle が緑**(値意味論/所有権不変量を proptest 化、§5.1)— byte gate が覆えない共有決定バグを上から捕まえる第一級ゲートとして稼働。
6. `PerceusVerify`(Lean 認証)が新 MIR Dup/Drop ノードに対して有効なまま(flight-grade Lean 資産が detach しない)。
7. §643 / #591 / #610 が**構造的に**(単発修正でなく)解消され、回帰 fixture で固定。
8. 差分 oracle(旧 emit 差分 / byte gate)は**まだ退役しない** — 退役は上位独立 oracle(interp 規範化 + property laws + wasm-spec 忠実性)が置き換わる Phase 2 以降、§5.3 の不変条件下でのみ。

→ 達成で Phase 2(MIR 形式化・interp を規範・Lean を MIR 意味論へ)へ。未達なら §8 の不合格分岐(正準形見直し / Rust 明示 Rc 許容 / 資格化専用割り切り)を**段2の早期 falsify で安く**選べる。

---

## 8. 実装状況と次の一手

### 8.0 段0(完了): 意味法則 property-law oracle ✅

§5.1 の独立 oracle を**手術前に**敷設・検証・コミット済み(`tests/semantic_laws_test.rs`)。
generator が値意味論の正解を plain Rust で独立計算し、同じ操作の Almide を native+wasm で実行、
**両方が独立モデルと一致するか**を検査(`native==expected AND wasm==expected` — `native==wasm`
より strictly stronger で unify 後も生きる)。モデル自体の正しさは alias_cow(C-033)既知形で
実コンパイラと突き合わせて self-check。現コンパイラに対し 32+ ケース・3テスト全緑。`ALMIDE_SEMLAW_CASES`
で深掘り可。これが段1以降の全所有権変更を上から監視する網。

### 8.1 段1(Repr-on-values)の grounding(実測)

- **`Repr` の種は既存**: `crates/almide-ir/src/wasm_repr.rs` の `WasmRepr{I32/I64/F64/Void}` が
  mono の ABI 互換判定に使われている。ただし**粗い4分類で layout を持たない** — §2.1 の
  `Repr{Scalar{width}/Ptr{LayoutId}/Boxed{LayoutId}}` は richer。
- **Repr は crate 跨ぎ**: enum 本体(scalar width)は almide-ir に置けるが、`LayoutId` が指す
  `LayoutRegistry`(`emit_wasm/engine/layout.rs:77`)と box 性(`pass_box_deref.rs:132
  find_recursive_enums)は codegen 側。→ 段1 は almide-ir(Repr 型)+ codegen(layout 解決・
  stamp)に跨る一体変更。LayoutRegistry の所属(codegen→共有クレートへ昇格?)が最初の設計判断。
- **再設計衝突なし**: `codegen-ideal-form.md` は **done 済**(commit 8ea2d234、crates/CLAUDE.md の
  active/ 参照は stale)。var-table/dispatch 統一も done。MIR 作業は既存統一の上に乗る。

### 8.2 段1 の着手手順

1. `AllTypesConcrete` 前倒し + compile-order 非対称解消(§4)の PoC — 挙動不変・byte 一致を確認。← **次の一手(手術の開始点)**
2. MIR 値モデル(`Repr` enum + LayoutId)を追加、mono 後に Repr を stamp。`WasmRepr` を吸収。
3. ~~段2 の専用 compound-drop fixture~~ ✅ **完了**: `spec/churn/{map_entry,closure_env,nested_named}_churn.almd` が
   emit_typed_rc_dec の3分岐(Map-entry/closure-env/nested-Named drop)を 200k churn で固定。`check-frees-churn.sh`
   で native==wasm + exit0 + no-trap、全6 fixture 緑。`??`(UnwrapOr)を避け #643 と分離。手術の最薄領域に網を先張り済み。

段1 は almide-ir+codegen 跨ぎの一体実装(複数セッション)。**手術前の網は2枚とも敷設・緑**: §8.0 の意味法則
oracle(値意味論を上から)+ §8.2-3 の compound-drop churn(drop schedule を最薄領域で)。各増分でこの両網が緑を保つことを確認しながら進める。
