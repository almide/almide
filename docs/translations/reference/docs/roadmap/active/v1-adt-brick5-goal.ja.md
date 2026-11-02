<!-- description: 履歴ADT brick-5目標prompt、完了済みrecursive dropと却下された試行の記録 -->
<a id="goal-prompt--finish-adt-brick-5-heap-field-bind--recursive-drop-to-the-1-lever"></a>

# 目標PROMPT — ADT brick 5（heap-field bind + recursive drop）を仕上げ、第1の足がかりを実現する

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](v1-adt-brick5-goal.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](v1-adt-brick5-goal.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](v1-adt-brick5-goal.zh-CN.md)

> 非規範的な履歴読者資料であり、実行taskやコマンド実行権限ではない。固定原文が正本で原本不変更。自動同期・新検証・証明なし。冒頭のbrick完了記録が旧残作業promptに優先する。所有権trace受理は生成raw-memory drop本体の検証ではない。元の手順主張と却下試行を保持し、現行の普遍安全保証へ格上げしない。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-adt-brick5-goal.md)

<a id="historical-goal-prompt--completed-brick"></a>

## 履歴目標prompt — 完了済みbrick

正本の[ADT value-model記録](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-adt-value-model.md)では、5cの後に5bのrecursive dropが`c98486be`で完了したと記録されている。以下のpromptは、それ以前のbranch内調査を記録するもので、未完の実装taskとして再開してはいけない。却下された試行とtrusted-drop境界は依然有用な証拠である。所有権traceの受理は、生成されたraw-memory drop routineの本体が正しいことを証明しない。

branch **`develop-v1`**でv1 trust-spine ADT value-model作業を継続する。brick 1–4 + 5aは完了しpush済み（`git log`参照）。仕事は、**brick 5c（heap-field match bind）と5b（recursive drop）を入れ、再帰的な`Expr`のto_stringをv0とbyte一致させること。これはrepo横断wallを取り除く第1の足がかり（約41 wall）である。** 最初に`docs/roadmap/active/v1-adt-value-model.md`を読むこと。設計・状態の正本である。

<a id="the-goal-concretely"></a>

## 具体的な目標

次のprogramをv1 trust spine（`render_program` → wat → `wasmtime`）でv0（`almide run`）とbyte一致させる：

```almide
type Expr = Lit(Int) | Add(Expr, Expr) | Neg(Expr)
fn tos(e: Expr) -> String = match e {
  Lit(n)    => int.to_string(n),
  Add(l, r) => "(" + tos(l) + " + " + tos(r) + ")",
  Neg(x)    => "-" + tos(x),
}
fn main() -> Unit = { println(tos(Add(Lit(1), Neg(Lit(2))))) }   // v0: (1 + -2)
```

当時は正直にwallになる。完了条件は、これと`Expr`のcreate+drop leak loopでv0==v1 byte一致、`bash proofs/corpus-wall.sh`の3性質すべてACCEPT、`cargo test -p
almide-mir`で新しい失敗なし（既存2件の`self_hosted_json_*`の`$__drop_value`失敗は無関係）。

<a id="non-negotiable-discipline-the-②-cardinal-rule--already-re-proven-twice-this-arc"></a>

## 譲れない規律（②の根本規則 — このarcで既に2回再確認）

**kernelで証明された`[ownership]`検査器が拒否するMIRは出荷しない。** heap-field variant loweringは`wasmtime`でbyte一致しても潜在double-freeになりうる。`corpus-wall.sh`の証明済み検査器だけが検出する。単純な5cの2試行（borrow `LoadHandle`+`param_values`と`lower_heap_result_arm`内Option式move-out auto-Dup）はともに`[ownership] REJECT`だった（ownershipは16085→16162になる）。したがって各段階のgateはcorpus-wall ACCEPT。REJECTならrevertして出荷せず、理屈で通り抜けない。小さな増分で進め、毎回corpus-wallを実行する。

<a id="where-the-code-is"></a>

## コードの場所

- 構築：`crates/almide-mir/src/lower/binds.rs`の`try_lower_variant_ctor`（5aのscalar + leaf `String` fieldは既に入った。`String` fieldをmove-inし`record_masks`でmask、既存DropListStrで解放）。heapの非String/nested-variant fieldはwall。
- Match：`crates/almide-mir/src/lower/control.rs` — `try_lower_custom_variant_match`（value）、`lower_custom_variant_unit_match`（unit-stmt）、`parse_variant_arms`（共通arm parser、当時heap-field bind拒否）、`bind_variant_arm`（scalar bindのみ）、`emit_variant_arm_chain` / `emit_variant_unit_chain`。接続先：tail.rs（scalar+heap）、binds.rs（let-bind）、calls.rs（operand/argとarg位置のctor構築）、control.rs:約122（unit statement）。
- v0参照layout（#782で削除、git履歴から読む）：`emit_wasm/collections.rs::emit_record`、`equality.rs::{variant_alloc_size,find_variant_tag_by_ctor}`、`mod.rs`のvariant登録。（v1は独自のuniform-i64-slot block、tag@slot0、fields@slot1..、slot_countまでpaddingを使う。v0のbyte-packed layoutではない。v0と一致すべきなのは観測可能なstdoutだけ。）

<a id="step-5c--heap-field-match-bind-the-blocker-use-aware-binding-is-the-answer"></a>

## Step 5c — heap-field match bind（blocker。答えは使用方法を考慮した束縛）

当sessionで診断完了。corpusの正確な再現例と2つの失敗形態が判明：

- 拒否されるcorpus関数は **`spec/wasm_cross/generic_fn_in_inferred_lambda.almd`** — `fn unbox[T](b: Box[T]) -> T = match b { B(x) => x }`を`Box[String]`でmonomorphizeしたもの。borrowされたparamからString fieldを束縛しmove-outする（`B(x) => x`）。その単一ownership.cert lifecycleがbugを特定する。
  - **borrow bind**（`LoadHandle`+`param_values`、さらに`lower_heap_result_arm`のVar auto-Dup）：move-out `B(x)=>x`は問題なし（`am`）。しかし消費する再利用`Ctor(s) => Other(s)`はrc 0のborrowに`Consume`/`m`をemitしてREJECT。
  - **Dupしたowned bind**（`LoadHandle`+`Op::Dup`、`live_heap_handles`へpushしarm frameがdrop）：消費する再利用は問題なし。しかしmove-out `B(x)=>x`は二重取得になる。bindの`a` + `lower_heap_result_arm`のauto-Dup `a` + その`m` + arm-frameの`d` + returnの`m` = `aamdm`、正味−1の過剰解放でREJECT。
  再現：`cargo run -q -p almide-mir --example classify_corpus -- --out /tmp/cw
  spec/wasm_cross/generic_fn_in_inferred_lambda.almd`の後に`cat /tmp/cw/ownership.cert`（Dupなら`aamdm`、borrowなら釣り合った`am`）。

修正は**使用方法を考慮した束縛**。束縛するheap fieldは、arm本体の使い方からbind modeを選ぶ必要がある：
  - read-only / borrow calleeへのborrow渡し（`string.len(x)`、`tos(l)`）：**borrow**（`LoadHandle`+`param_values`、`live_heap_handles`に入れずdropなし）。`ad`なしで均衡。
  - arm結果としてmove-out（`B(x) => x`）：**borrow + 既存auto-Dup**、`lower_heap_result_arm`のVar case（`am`）。先にDupしない。二重取得になるため。
  - 新ctor / 消費callへ渡す（`Ctor(s) => Other(s)`）：**`Dup`してからmove**（`am`）。先にDupし、borrowではなくowned refをmoveする。
  小さなuse-visitorで各束縛fieldのarm本体での使用（borrow渡し/結果へのmove-out/消費）を分類し、それに応じて束縛する。arm-frame helper `lower_variant_arm_value` / `lower_variant_unit_arm`は既存（bind前のmarkから`drop_arm_locals`を使ってarm終端のowned bindをdropし、条件分岐でも安全）。注意：条件付きarm内でowned localを`live_heap_handles`から除かない。lhhはcompile時、branchはruntimeで、他のarmにはscope dropがまだ必要。helperのようにIfThen/Else領域内でarmごとにdropする。

検証：corpus-wall ACCEPT（gate）、`unbox`式と`tos`式のbyte一致、leak loop。

<a id="step-5c--status--done-commit-b50e95a3"></a>

## Step 5c — 状態：✅ 完了（commit `b50e95a3`）

複数arm matchのheap-field（String）bindが入った（borrow + move-out時auto-Dup + consume時Dup）。blockerはbind modelではなく、**単一arm direct-retの二重move**だった（`unbox` newtype `B(x)=>x`：`Op::Consume`→`m` + heap `func.ret`→`m`）。単一arm heap caseは当時wallにし、複数armは進める。corpus-wall ACCEPT。再帰`Expr` to_stringの足がかりには5bだけが残る。

<a id="step-5b--recursive-drop-the-last-piece--concrete-recipe-designed-this-session"></a>

## Step 5b — recursive drop（最後の部分）— 具体的手順（当sessionで設計）

重要な洞察：`$__drop_value`（stdlib/value_core.almd）は`prim.*` op（`prim.handle`、`prim.load32`、`prim.load_handle`、`prim.rc_dec` + Unit自己再帰）で作るself-hosted Almideである。`prim`は検査器ではno-opなので、drop fnのownership certは空。つまりtrusted routineであり、leak/double-freeの正しさの検査を担うのはcertではなくleak loop。したがってcustom-variant recursive dropはrender/runtime変更ではなく、同じ形の型ごとに生成するAlmide fnで、value_core同様auto-linkし、構成上cert-cleanとなる。

手順：nested-variant heap ctor fieldを持つvariant型ごとに生成する（v1 layout = `[rc@0][len@4][cap@8][tag=slot0@12][field slot i @ 12+(1+i)*8]`、tag格納幅8）：

```almide
fn __drop_Expr(e: Expr) -> Unit = {
  let h = prim.handle(e)
  if prim.load32(h + 0) == 1 then {        // last ref
    let tag = prim.load64(h + 12)          // slot 0 = tag
    if tag == 1 then {                      // Add: fields slot1@20, slot2@28
      __drop_Expr(prim.load_handle(h + 20))
      __drop_Expr(prim.load_handle(h + 28))
    } else if tag == 2 then {               // Neg: field slot1@20
      __drop_Expr(prim.load_handle(h + 20))
    } else ()                               // Lit (tag 0): scalar, nothing to free
  } else ()
  prim.rc_dec(h)
}
```

（variant/heap fieldだけを再帰処理する。leaf String fieldは`prim.rc_dec(prim.load64(h+off))`、scalar fieldはskip。ctorにloopが必要なら`__drop_value`のtag-5 array caseと`__vdrop_arr` helperの形に倣う。variantはctorごとに固定arityなのでloop不要。）

接続：
1. MIRに`Op::DropVariant { v, ty }`。certは1個の`d`（lib.rs + certificate.rs、他Drop* opに並べる）。Render → `(call $__drop_<ty> (local.get v))`。
2. program組立時に再帰variant型ごとに`__drop_<ty>` Almide sourceを生成しauto-linkする。最も自然なhookはalmide-types/src/self_host_registry.rsの`self_host_runtime()`付近。ただしregistryは固定fileの`include_str!`なので、動的generated-source経路を追加する。`VariantLayouts`からsource文字列を作り、`lower_source`と同じfrontend feederを通し、fnを`__drop_<ty>`へ改名しprogram関数へ追加。render_program（trust spine）とrender_wasmの`lower_source` test feederの両方でlinkが必要。
3. 構築：`try_lower_variant_ctor`でnested-variant heap fieldを許可する（String経路と同じく、`lower_owned_heap_field`でchild handleをmove-in。そのVar caseは既に`Dup`する。さらにmask）。構築値を追跡し、scope終端dropを`Op::DropVariant{ty}`とする（`value_handles` → `DropValue`と同様の新追跡set）。
4. Match-bind：`parse_variant_arms`の5c `Ty::String` heap-field bindをvariant型fieldにも拡張（borrow。`tos`の`l`/`r`は再帰callへborrow渡しされ、move-outしない複数arm。単一arm gateが退化newtypeを既に覆う）。
5. 各段階の検証：corpus-wall ACCEPT、`Expr`のcreate+drop leak loop（freelistによりleakがOOB trapになる）、`tos(Add(Lit(1), Neg(Lit(2)))) == "(1 + -2)"`のbyte一致。

<a id="step-5b--original-notes"></a>

## Step 5b — 元のメモ

nested-variant ctor field（`Add(Expr, Expr)`）はchild slotに平坦な`rc_dec`を行うだけでは孫をleakする。既存`Op::DropValue` / `$__drop_value`（dynamic-Value recursive drop）と同様の**tag駆動の再帰free**が必要。想定設計：

- `try_lower_variant_ctor`でnested-variant heap ctor fieldを許可（child handleをmove-inしmask）。
- 再帰variant型ごとに`$__drop_<Type>(ptr)` runtime fnをemit。tag@slot0を読みswitchし、ctorごとに各variant fieldを再帰`$__drop_<FieldType>`、各leaf fieldを`rc_dec`、最後にblockをfree。（あるいはemitした型ごとのlayout tableで動く汎用`$__drop_variant`。）
- 新しい`Op::DropVariant { v, type }`（またはDropValue機構再利用）をrenderがそのfnへのcallにlower。証明済み検査器は`$__drop_value`同様、単一のtrusted `d`として受理する。
- 構築した再帰variant値を追跡し、scope終端dropで`Op::DropVariant`をemit。
- 検証：`Expr`のto_string byte一致、create+drop leak loop、corpus-wall ACCEPT（ownership）。検査器は再帰`d`をleak/double-freeなしとして受理しなければならない。

<a id="verification-commands-run-after-every-increment"></a>

## 検証コマンド（増分ごとに実行）

```
cargo build -q -p almide-mir --example render_program
# byte-match: almide run <f>  vs  render_program <f> | wasmtime
bash proofs/corpus-wall.sh        # MUST stay ACCEPT (all 3) — the soundness gate
bash proofs/diff-fuzz.sh 40       # generative v0==v1 (variant templates already present)
cargo test -q -p almide-mir       # no NEW failures (2 pre-existing json fails are unrelated)
```

`Expr`の足がかりがcorpus-wall ACCEPTとともにbyte一致したら、決定的wasmtime cargo test（`crates/almide-mir/src/render_wasm/tests_part3.rs`の`custom_variant_*`付近）とdiff-fuzz再帰variant templateに追加し、`docs/roadmap/active/v1-adt-value-model.md`で5b/5cを完了と記し、commit（英語1行、`Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>`）。依頼された場合にだけ`develop-v1`へpushする。

<a id="guardrails"></a>

## ガードレール

- branchは`develop-v1`。他agentが並行作業中なので、自分が変更していないfileに`git checkout`/`restore`/`stash`を使わない。手動Editでrevertする。
- `?? p03*.*`など他のuntracked fileは自分のものではない。触らない。
- §4.1の手書きWAT runtimeを増やさない。
- cert-soundにできない段階は正直にwallとして記録する。潜在double-freeよりwallがよい。部分的でもsound（例：5a構築）なら真の前進であり、検査器に拒否されたものはそうではない。

コード内コメントの説明訳：最初の例のv0出力は(1 + -2)。drop例は最後の参照のときだけtag（slot 0）を読み、Addのfieldはslot1@20とslot2@28、Negのfieldはslot1@20を再帰解放し、Lit（tag 0）はscalarなので解放対象なし。検証コマンドのコメントは、v0とrender_programのbyte比較、3性質すべてACCEPT維持という健全性gate、variant templateを用意済みの生成的v0==v1比較、新しい失敗なし（既存json失敗2件は無関係）を指す。これらは履歴指示の説明で、実行していない。
