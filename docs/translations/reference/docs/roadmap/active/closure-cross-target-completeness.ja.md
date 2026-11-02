原文の説明：歴史的な closure sweep、その後のアーキテクチャ文脈、証拠の限界
<a id="closure-cross-target-completeness"></a>

# Closure のクロスターゲット完全性

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](closure-cross-target-completeness.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](closure-cross-target-completeness.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](closure-cross-target-completeness.zh-CN.md)

> 固定原文の非規範読者用スナップショット。正本は原文であり、自動同期しない。独立した実装検証や引用指示の実行は行わない。歴史的状態、計画、比較主張は原文の範囲を保持する。
> [Original source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/closure-cross-target-completeness.md)

> 原文注記：歴史的計画であり、現行 gap の再現、全 closure プログラムの証明、CI 変更の指示ではない。5分類、退役 emitter 箇所、件数、提案修正は元の範囲を保持する。Class2 は不健全性修正済み・inner 参照先 leak のみ残存と明記するが、後段順序はなお不健全性＋leakと書くため整合化せず保存する。Class4 は大半 Class1 に統合されるが vp3 診断の確認は残る。回帰リスクなし・不確実性消滅・回帰不能の表現は原設計主張で、本翻訳の保証ではない。有限差分／golden test と guard で普遍的正しさは確立しない。workflow ID と memory 名は歴史的参照で、ここでアクセスして確認した証拠ではない。引用 copy/build 指示は実行していない。


<a id="historical-sweep-not-a-current-gap-census"></a>

## 歴史的 sweep であり、現在の gap 棚卸しではない

以下の PR #337–344 の sweep と5分類計画は、以前の closure 実装を説明する。240ファイル corpus と残存所見は、今回再現した現在の棚卸しではない。完了した [closure アーキテクチャ記録](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/done/closure-architecture-v2.md) と現在の [module-global storage pass](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/crates/almide-codegen/src/pass_top_let_storage.rs) は後の文脈であり、v0 の `emit_wasm` 箇所は退役コードを指す。

望む完全性は依然目標である。有限 sweep の合格や guard の導入で全 closure プログラムの正しさは証明できず、この更新は後の全 closure/host 形状を閉じたり、旧結果を再認証したりしない。

**目標**：native（Rust）と WASM を*すべての* closure プログラムで一致させる。敵対的 sweep を増やして通すのでなく（標本抽出では不存在を証明できない）、**構成によって bug class を除去**し、**恒久 CI 差分 gate**を加えて表面を閉じたままにする。

<a id="how-we-got-here"></a>

## ここに至る経緯

native 対 WASM の敵対的 closure sweep を6回実施（多様な closure プログラムを生成し、`almide run` と `wasmtime` を diff）。sweep 1–3 は `() -> Unit` 表面を閉じた（11修正、PR #337–340）。sweep 4–6 は**引数付き closure** 次元 `(T) -> U` を開いた。

| PR | 修正 |
|----|-------|
| #341 | fold ICE（HOF-lambda-param call の結果型）＋WASM build-gate |
| #342 | field-type の入れ子 Fn → `Rc<dyn Fn>`（再帰的） |
| #343 | closure を返す closure（`Box`→`Rc`＋trait-object cast） |
| #344 | closure 値の `Map`（map.set / `??` / from_list / HOF / variant）、5/6 |

**6回目の sweep**（正本となるもの：merge 済みの*安定* binary で実施）でも**実際の乖離12件**が見つかった。そこが要点で、位置ごとの patch は収束しない。以下の**5 bug class**に分類できる。2つは class 全体を不可能にする構造的修正、3つは対象を絞った修正を求め、CI gate が恒久化する。

> **Sweep binary の罠（学び）**：4回目は agent が新規 build でなく古い `~/.local/bin` binary を実行したため無駄になった。必ず `cp target/release/almide ~/.local/bin/almide` を行い、sweep 手順書で生成前の `--version` assert も行う。sweep *中*には rebuild しない（変動で結果が濁る）。

---

<a id="the-5-classes--the-gate"></a>

## 5分類と gate

<a id="class-1--boxing--type-unification-e0308-two-closures-cant-unify-e0562-impl-trait-in-field"></a>

### Class 1 — boxing / 型単一化（E0308「2 closure が単一化できない」、E0562「field の impl Trait」）
**症状**：格納した closure が具象の無名型に留まり、別 closure（`??` fallback、別の `map.set`、variant payload、`if`/`match` arm）と型を共有できない。さらに **a6 回帰**（#5 が Computed-call 引数を過剰 boxing → `Rc<Rc<dyn Fn>>`）と、未解決 **sc4**（現 box-pass 時点で `Ty::Fn` が未解決の let-bound closure）がある。

**修正 — Refactor 1（構成による）：`FnBoxingPass`＋単一不変条件。**
> **INV-BOX**：closure 値（`Ty::Fn`）は*あらゆる格納／単一化 slot*で `Rc<dyn Fn>`。raw `move ||` は閉じた列挙集合の3箇所だけ：(a) ここで直接呼ぶ、(b) `impl Fn` parameter へ値渡し、(c) `fan.*` の Send+Sync 位置。

- 単一 classifier `classify_fn_site(parent, slot) -> {Boxed, RawCall, RawImplFn, RawSendSync}`。既定は `Boxed`、3つの `Raw` だけが例外。`pass_rust_lowering.rs`/`walker/` の位置別 IR 規則約14個＋walker 規則5個を1判断に畳む。
- **新 pass `FnBoxingPass`** を再配置：`ConcretizeTypes` の**後**（全 `Ty::Fn` が確定し sc4 の古い型による失敗を消す）、`CloneInsertion` の**前**（boxed closure の clone が安い `Rc::clone` になる）。この順序変更が本質。
- type renderer：`render_type`（impl Fn）/`render_type_field_fn`/`render_type_rc_fn` を統合し、「storage ⇒ `Rc<dyn Fn>`」を1規則にする。`let` 束縛 closure は `_` でなく具体的 `Rc<dyn Fn>` として render し、call site の曖昧さをなくす。INV-BOX では既に `Rc` の値は `Boxed` site にあり再wrapしないので、未boxing時だけboxする規則が内在し、sc4＋a6を構成的に修正する。
- **最大リスク**：`Raw` 集合は網羅的でなければならない。direct-call/HOF/fan の誤 `Boxed` は Rust が `impl Fn` を求める場所へ `Rc<dyn Fn>` を渡し（E0277/E0631）、誤 `Raw` は E0308 を再導入する。#337–344 の全回帰テスト＋spec 240×2 target＋選別 sweep corpus（gate）でテストする。
- **包含するもの**：全 `box_node`/`box_fn_in_value` 規則、sc4、a6。**工数 L**。

<a id="class-2--wasm-mutable-capture-cell-residual-leak-unsoundness-already-fixed"></a>

### Class 2 — WASM mutable-capture cell：残る LEAK（不健全性は修正済み）
**状態（コード確認済み、訂正）**：**不健全性／trap は既に修正済み** — closure-v2 **P6**。mutable-capture cell の `RcDec{var}`（`emit_wasm/statements.rs:342-358`）は、今は **cell に単純 `rc_dec`**（line 352）を行い、明示的に `emit_typed_rc_dec(inner_ty, cell_ptr)` ではない。コメントに理由がある：typed dec は cell ptr を object として走査し、`cell[0]` を要素数と解釈→element-drop loop が garbage を decref→`List[String]` は trap、`List[Int]` は生存。
**残るのは LEAK だけ**：単純 `rc_dec` は *cell 自身*の allocation は解放するが、`cell[0]` の**内側 heap 参照先**を drop しない。そのため captured `List[String]`/`Map`/record は cell 消滅時に leak する。WASM 限定（Rust は `RcCow`/`thread_local RefCell<Rc<T>>` → RAII）。

**修正 — Refactor 2（構成による）：型付き cell destructor（leak を閉じる）。**
- line 352 の単純 `rc_dec` を `emit_typed_cell_rc_dec(inner_ty, cell_ptr)` に置換。cell RC を減らし、**0になったとき**、`is_heap_type(inner_ty)` なら `inner = cell[0]` を load して*既存* `emit_typed_rc_dec(inner_ty, inner)` を呼ぶ。**再利用し、再実装しない**（同じ再帰 child-dropper を今度は正しい inner ptr に根づかせる）。その後 cell を解放。Copy inner 型に inner free は不要で、現在の単純 `rc_dec` が既に正しい。
- 構成上正しい：capture は **cell** に単純 `rc_inc`（inner RC 不変、cell が1度所有）を行うので、`rc(cell)==0` gate 下で inner はちょうど1回解放される。
- **Lean 注記（MAP-DRIFT A）**：`ClosureRc.lean` は存在せず、belt は `Basic/FnBody/Heap.lean`。「cell drop が inner をちょうど1回解放（inc==dec、leakなし）」の証明は拡張でなく**新規**。任意だが memory model の機械検査付き完全性に当たる。
- **工数 M**（小修正でテスト済みコード再利用）、**Lean 証明も範囲に入るなら＋L**。

<a id="class-3--in-place-rewrite-on-a-non-owned-lvalue-silent-wrong---re-diagnosed"></a>

### Class 3 — 非所有 lvalue への in-place rewrite（静かな誤結果）🔴（再診断）
**closure bug ではない**。`var g: List[Int] = []; g = g + [n]` は Rust/WASM **両方**で `len = 0` を印字し、compile/run は正常。**closure なし**（`main` の top level の `g = g + [1]`）でも再現する。影響は `g = g + [single_literal]` のみで、`g = [1,2,3]` と `g = g + other_list` は正常。sweep の closure という枠づけは目くらましで、global mutation を目立たせただけだった。

**根本原因**：`try_rewrite_push`（`pass_rust_lowering.rs:497`）が、lvalue の storage class でなく *aliasing だけ*を見て `xs = xs + [v]` → `xs.push(v)`（Method-call）へ書き換える。shared closure cell では既に bail する：line 500 `if shared.contains(&var) { return None; }`、コメントは「`xs.push(v)` への書換えは捨てる clone に push してしまう」。しかし module-level global は `shared` 集合にないので peephole が発火する。method object は walker の `expressions.rs`（`is_rc_cow` だけ特別扱い、`ModuleRc` は扱わない）で、捨てられる一時値の読み取り専用 clone `G.with(|c| (**c.borrow()).clone()).push(v)` として render され、write-back が消える。WASM にも heap-global への `Assign` 経路（`emit_wasm/statements.rs`）に**独立した同類 bug**と、それを飲み込む静かな `drop` fallthrough がある。*根本原因はコード確認済み。「両 target、closure 不要」の再現範囲は設計 workflow `wf23aws8j` の実測による。*

**修正 — guard は既存で、globals を覆わないだけ**：
- **主修正（S）**：line 500 の既存 `shared.contains(&var)` bail を拡張し、`var` が module-global / `ModuleRc` lvalue のときも `None` を返す（module-origin 集合を `shared` と共に渡す）。元の `Assign` が正しい `ModuleRc` write-back 経路を通る。数行の修正で、新機構ではなく bail の型が既にそこにある。
- **防御（M）**：walker の mutating-method write-back（`expressions.rs:961-980`）を `VarStorage::ModuleRc` に拡張（1206 の `is_rc_cow` arm を反映）、あらゆる method-call source について class を閉じる。WASM heap-global `global_set` write-back を修正し、**両方の静かな `drop` fallthrough を hard `unreachable`＋emit 時診断へ置換**する（silent-discard が増幅器）。
- **構成による対策**：すべての in-place rewrite（push/index-assign/swap/reverse）が使える `LvalueClass {OwnedLocal, ModuleCell, ModuleRc, RcCow}` query を追加し、将来の peephole が非所有 lvalue を静かに対象化できなくする。**工数 M**。対象限定項目の最優先であり、silent-wrong の**一般的正しさ bug**、closure 専用ではない。

<a id="class-4--wasm-closure-env-width--mostly-collapses--not-a-separate-bug"></a>

### Class 4 — WASM closure-env 幅（大部分は統合される、独立 bug ではない）
「captured Int が i32 保存／env_ptr が garbage」という仮説は**反証された**。closure 値は heap 上の `[table_idx:i32][env_ptr:i32]` pair への単一 i32 pointer。env store（`equality.rs:561`）と load（`expressions.rs:141-157`）は対称 i32 で正しい。vp3 sweep 失敗は **curried-closure の non-Clone**、つまり **Class 1**（Refactor 1）領域で、env 幅 bug ではない。
残るのは潜在的不一致だけ：`byte_size(Ty::Fn)=4`（`values.rs:49`）対 hard-coded 8-byte env stride、現状は偶然正しい。**修正（S、強化）**：`Ty::Fn` に明示 `byte_size`/`ty_to_valtype` arm と、env stride ≥ `byte_size(Fn)` の `debug_assert` を加え、将来表現を変えると layout を壊す前に test が失敗するようにする。*未解決1点：vp3 の整合。sweep は native-OK/WASM-validator-fail を見たが設計は Class1 boxing 経路と言う。Refactor 1 後に vp3 が完全に吸収されたか検証する。*

<a id="class-5--enum-derive-through-closure-bearing-types-vp5--confirmed"></a>

### Class 5 — closure を含む型を経由する enum derive（vp5）（確認済み）
**症状**：`Lazy` が `() -> T` field を持つ `type Tree = Leaf | Node(Lazy[Int])` で、`Tree` に `#[derive(Clone, Debug, PartialEq)]` が付くが `Lazy<i64>` は `Debug` でも `PartialEq` でもなく、E0277＋E0369。
**根本原因**：derive 選択が**非対称**。`render_type_decl` は record に `has_fn_fields`/`has_non_eq_fields` を計算（`declarations.rs:30,34`）するが、**enum 分岐（66-124）はどちらも計算しない**（`has_hash` のみ）。`enum_decl`（`rust.toml:317-326`）にも Clone-only / no-PartialEq variant がない。推移的 `eq_blocked_types` 不動点（`compute_eq_blocked_types`、declarations.rs:373）は**既に enum を正しく扱う**のに、consumer が読まない。
**修正（S/M）**：enum 分岐で record 同様 `has_fn_fields`/`has_non_eq_fields` を計算して `enum_attrs` へ渡し、`enum_decl` に `when_attr="has_fn_fields"`（Clone-only）と `"has_non_eq_fields"`（Clone+Debug）variant を加える。**構成による対策**：struct/enum 共通 `select_derives(field_tys,
eq_blocked, has_hash, repr_c) -> attrs` を抽出し、再 drift しなくする。回帰リスクなし（推移的に `Fn` を含み `PartialEq` に依存する enum は現在不健全で、そもそも compile しない）。

<a id="the-gate--extend-the-existing-cross-target-ci--already-partly-there"></a>

### Gate — 既存クロスターゲット CI を拡張（一部は既に存在）
`.github/workflows/ci-cross-target.yml` は既に存在するが pass/fail の乖離だけを見て、出力の正しさは見ない。`wasm_cross_target_spec` test は `spec/wasm_cross/*` の native 対 wasm を diff する。
**修正（S/M）**：全 sweep corpus（`/tmp/sweep4_*`＋`/tmp/cbox/*`）を `spec/wasm_cross/` または `spec/closure_xt/` に **golden 期待出力**付きで commit。Class3 は両 target が同じ誤値を出し、cross-target *一致*だけでは不足と示した。`ci-cross-target.yml` を拡張し、任意の `native != wasm` または `!= golden` を失敗にする。「patch した」を「回帰できない」に変える。

---

> **設計段階の精緻化（2設計 workflow が変えたこと）**：6回目 sweep の「5分類」は分析で増えず、*小さく*なった。**Class3 は一般的正しさ bug**（lvalue storage class を無視する peephole、closure なし両 target 再現）で closure 専用でない。**Class4 は大半が Class1 に統合**（closure-env 表現は正しく、潜在 layout assertion だけが残る）。**CI gate も既存**で、出力の正しさ corpus だけが必要。実作業は独立未知6個ではなく、**2 refactor＋小さな対象限定修正2個＋gate 拡張1個**。

<a id="sequencing"></a>

## 順序

1. **Class3**（S 主修正／M 全体、silent-wrong）を最初に。非所有 lvalue の in-place rewrite は closure と独立した一般的正しさ bug で、silent-wrong が最も危険。S の peephole storage guard を即時導入し、M の `LvalueClass`＋静かな `drop` の除去は後続可能。
2. **Class5**（S/M、独立）で素早く variant/enum closure 形状を解放（共通 `select_derives` が構成的形）。
3. **Refactor1／Class1**（L、構成的）が主役。sc4＋a6＋全 boxing edge と **vp3/Class4 curried-closure 失敗**も吸収。14規則を安全に削除するため選別 corpus gate と共に着地させる。
4. **Refactor2／Class2**（M、構成的）：WASM cell 不健全性＋leak、1とは独立。
5. **Class4 残り**（S、強化）：Refactor1 後 vp3 吸収を確認し、`byte_size(Ty::Fn)` arm＋env-stride `debug_assert` を加える。現存 bug の patch は想定しない。
6. **CI gate**（S/M）：既存 `ci-cross-target.yml` を拡張し、golden 付き sweep corpus を追加。`native != wasm` または `!= golden` で失敗。各 class が閉じるごとに corpus を段階導入。
7. **任意の Lean 証明**：cell destructor の memory 完全性を機械検査する。

各段階で spec 240×2 target＋全 `cargo test`＋closure 回帰 suite＋選別 corpus diff を検証。各段階1PR（既存の root-cause→fix→verify→PR→CI→merge ループ）。

<a id="definition-of-done-completeness"></a>

## DONE の定義（完全性）
closure native↔WASM 表面が**完全**となる条件：(a) Class1/2 を構成で除去（INV-BOX＋typed cell destructor、位置別規則なし、leakなし、不健全性なし）、(b) Class3/5 を修正・被覆し Class4 は通過する layout assertion へ縮小、(c) 拡張 `ci-cross-target.yml` が毎 build 全 golden sweep corpus を実行し `native != wasm` または `!= golden` で失敗、(d) 新たな敵対的 sweep で**実乖離0件**。工数は**遠い**（大 refactor2＋小修正2＋gate、複数の集中 session）が、**不確実性は消えた** — 既知の有限リストで、新 edge のフラクタルではない。

> Refactor1＋2 の live 設計詳細：workflow `wj7yrlb5h`。Class3/4/5＋gate：`wf23aws8j`。
> 形状別 boxing 注記と sc4 edge は memory `project_closure_parametered_dimension.md` を参照。
