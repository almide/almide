説明：lowering のみで heap-result match を認識型ではなく全域にする
<a id="camp-4-generalised-make-heap-result-match-total-instead-of-recognised"></a>

# Camp-4 の一般化：heap-result `match` を認識型ではなく全域にする

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](camp4-general-heap-match.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](camp4-general-heap-match.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](camp4-general-heap-match.zh-CN.md)

> 固定原文の非規範読者用スナップショット。正本は原文であり、自動同期しない。独立した実装検証や引用指示の実行は行わない。歴史的状態、計画、比較主張は原文の範囲を保持する。
> [Original source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/camp4-general-heap-match.md)

> 原文注記：8月17日の取り込みと有限403/403・7/10 corpus・下流実行は歴史的原文報告で、ここで再実行したテストや普遍的 match 対応ではない。初期 wall 記述と未着手段階は診断履歴として残し、その例を解消する場合は冒頭の取り込み結果が優先する。L2 はテスト範囲のみで成立。Map/Set typed twin は記録上の別の未着手範囲。RESEARCH.md と外部参照 tree はこの snapshot にない歴史的引用で独立監査証拠ではない。4鏡映箇所との数え方は twin/drop/linkage をまとめて列挙しており、その grouping を保存する。提案の gate 除去・削除・コマンドは実行しない。比較上の全域性や削除見積りは原主張で新検証ではない。


状態：**計画＋最初の取り込み済み作業。** 2026-08-17、新たな外部 wall（almide/dfa）と4つの RC 参照実装のソース読解をもとに記述。

<a id="landed-2026-08-17-the-1492-arc--all-gates-green-403403-no-cert-change"></a>

## 2026-08-17 取り込み済み（#1492 の作業）— 全ゲート green（403/403、cert 変更なし）

以下の L2 仮説はテストした範囲で成立した。全修正は**lowering のみ**に入り、checker・kernel・Coq の変更はない。これは L2 の予測どおり（`CBranch` 時代の checker は blocker でなく、旧 FLAT モデルで書かれた gate が blocker だった）。

- **Heap→heap Result payload の再 wrap を受理**（`let r = g(n)!` 後の `ok(r.node)`、#1492 の主題）：`try_lower_result_variant_ctor` の Ok arm が ctor builder から `lower_result_str_piece` に fallback。record cell が既に使うのと同じ合成。piece 側は既にほぼ全域（`lower_owned_heap_field`）。
- **再帰 payload の subject を RC guard 付き wrapper drop に流す**：`try_lower_result_match_value` は、Ok payload が再帰 drop を要する subject を `resrec:`（scalar-Ok/再帰-Err は `optrec:`）へ送るようになった。以前の flat dec は、arm が payload 全体を move せず子を取り出すと直ちに payload の field を leak した。全 payload の move は効果不変（再帰は1まで dec して停止）。Lean の派生値に対する incs-before-decs 規則を、欠けていた1か所に適用したもの。
- **`lower_result_str_piece` に他の全 Named-call 箇所と同じ ctor guard を追加**：variant ctor payload は未接続の `CallFn` を emit せず tagged block を構築する。このクラスを監査：user-named CallFn 22か所、ctor 位置 probe 10プログラム、全 BUILD+PARITY。残りは renderer の unlinked-call wall が支える。
- **module-level heap global を heap-result arm 値として受理**（`if c then DIGIT else …`）：arm の Var case は arm ごとの frame 内で `value_or_global` を使い、owned-heap-field の Var arm を鏡映する。
- **Scalar-Ok × record-Err を受理**（`Result[Int, {code, msg}]`）：variant-Err cell の record 版、len-as-tag `optrec:` wrapper。
- **`List[(scalar, …)]` variant ctor field を受理**（ADT brick 5、`Cls(List[(Int, Int)])` 形）：4つの鏡映箇所を同時拡張。ctor 受理、両 `needs_recursive_drop`、generator の field free（`__drop_list_str` 掃引、List[flat-variant] の先例そのまま）、`LIST_STR_DROP_SRC` linkage gate。

10形状 wall corpus（`w01–w10`）の結果：**2 → 6 BUILD+PARITY**、以下の combinator 作業で **10中7**。残る3つは Map/Set の typed twin。

<a id="landed-2026-08-17-second-arc-the-combinator-leg"></a>

## 2026-08-17 取り込み済み、第2作業：combinator 経路

linked typed twin のない型インスタンスに対する `result.map` / `map_err` / `flat_map` は、等価な `match` に desugar するようになった（`desugar_result_combinator_to_match`、counted-tree と lowering 両 chain）。これは combinator の参照コンパイラ方式（rustc の `?`、Swift のライブラリ `??`、Roc の canonicalization）。desugar はインスタンスが `_x` へ流れるかを `result_call_name` 自身に問い合わせ、twin の利用可否ロジックはちょうど1か所に置く。linked twin は byte-identical のままで、lowering が表現できない match は `_x` と同じく wall になる。lambda と named-fn の `f` 引数の両方、custom record Err 型で確認し、403/403。

<a id="the-remaining-brick-mapset-typed-twins-sized-not-started"></a>

## 残る brick：Map/Set typed twin（規模見積り済み、未着手）

`Map[List[Int], _]` / `Map[String, <record>]` / `Set[List[Int]]` は `_key_wall` / `_x` 経路（mod_p4_e.rs:609）で wall になる。これは lowering gap ではない。stdlib self-host はクラス別 prim-level hash table（`map*.almd` 3,238行、`set*.almd` 1,191行、key/value クラスごとに1ファイル。`set_str.almd` が String 版）。heap-key クラスには要素の構造的 eq+hash、table 内 owned-block drop routing、registry＋router＋usage gate の接続、さらに CLAUDE.md の family rule に従い、存在する cell を示す実行可能 matrix gate を同じ PR に含める必要がある。独立した本格的 stdlib 作業であり、lowering セッションに付け足してはいけない。

下流確認：`almide/dfa`（`Result[Cursor,_] → Result[Node,_]` を幹とする完全な再帰下降 regex parser）は wasm に build できるようになり（41KB、v1-verified）、50 tests が wasm で実行され、実入力で native/wasm byte parity。

Camp-4 を1例（`value.as_array : Result[List[Value], String]`）と、コード上はもう存在しない checker 制約に結び付けた `v1-value-model.md` §263-308 の位置づけを更新する。同文書の個別例分析はなお妥当だが、そこで名指した**根本原因**は何かを構築する前に再検証が必要。

---

<a id="1-what-walls"></a>

## 1. 何が wall になるか

失敗可能な関数が `Result` payload をある heap 型から別の heap 型へ変更できない。上流 almide/almide#1492 として起票。

```almide
type N = | L(Int) | C(List[N])
type R = { node: N, pos: Int }

fn g(n: Int) -> Result[R, String] = if n > 0 then ok(R { node: L(n), pos: n }) else err("neg")

fn f(n: Int) -> Result[N, String] = { let r = g(n)!; ok(r.node) }   // R -> N
```

```
heap-result `match` outside the executable subset cannot be faithfully returned
in this brick (would move out an empty deferred heap value)
```

原診断文の訳：この brick では実行可能 subset 外の heap-result match を忠実に返せない（空の deferred heap 値から move out してしまう）。最初のコード注釈 R → N は、payload 型を R から N へ変えることを示す。

`ok(r)`（payload 不変）は build できる。`ok(r.pos)`（payload → scalar）も build できる。heap → heap のみ wall。`!` の代わりに `match` でも同じ wall、`result.map` は wasm 定義自体がなく、3つの書き方すべてが閉じている。

これは特殊形状ではない。各段階が失敗可能な source → AST → IR → analysis → output という全段階変換の幹である。再帰下降 parser では cursor を捨てる1行で遭遇する：

```almide
fn parse_alt(cps: List[Int], pos: Int) -> Result[Cursor, String] = ...

fn parse(pattern: String) -> Result[Node, String] = {
  let c = parse_alt(to_codepoints(pattern), 0)!
  ok(c.node)                                       // the only line that walls
}
```

原コードのコメントは、末尾の行だけが wall になることを示す。

<a id="2-where-it-lives"></a>

## 2. どこにあるか

`lower_tail_heap_match`（`lower/tail_b.rs:355-440`）は手書きの形状認識器を固定順で試し、最後は `Unsupported` に落ちる：

| # | 認識器 | 受理対象 |
|---|---|---|
| 1 | `tuple_extract_match_index` | 単一 arm の tuple 分解 |
| 2 | `try_lower_custom_variant_match` | user ADT subject、heap arm |
| 3 | `try_lower_variant_value_match` | Option/Result subject — **scalar payload のみ** |
| 4 | `try_lower_result_match_value` | len-as-tag Result subject |
| 5 | `try_lower_option_match_value` | `Option[heap]` subject |
| 6 | `try_lower_list_match_value` | list subject |
| 7 | `try_lower_tuple_refinement_match` | scalar-tuple subject |
| 8 | `desugar_match_to_if` → `try_lower_heap_result_if` | if に還元可能 |
| — | その他 | **wall** |

形状認識方式の規模：

- `lower/` 配下に別々の `try_lower_*` / `lower_heap_*` 認識関数 **192**個
- Camp-4 gate を持つ6ファイル合計 **3,817**行（`control_p2.rs` 829、`control_p2_b.rs` 656、`control_p3.rs` 460、`result_match_value.rs` 246、`tail_b.rs` 813、`desugar_unwrap_b.rs` 813）
- `lower/` 合計 **64,930**行

コメントは1形状ずつ押し広げる frontier と明記する。`tail_b.rs:375`「heap payload がここで自ら None に戻る gate となる = 真の Camp-4 frontier」、`control_p2.rs:751`「単一 slot の borrow 表現がまだない」。

<a id="3-diagnosis-three-layers-and-only-one-of-them-is-real-work"></a>

## 3. 診断：3層のうち、本当の作業は1層だけ

<a id="l1--architectural-shape-and-ownership-are-entangled"></a>

### L1 — アーキテクチャ：形状と所有権の絡み合い

各認識器が自前の所有権説明を手で証明するため、**新形状は毎回新作業になり、未証明の形状はすべて wall**。`RESEARCH.md` 総合 point 5 は既に、調査した8コンパイラ全てにない anti-pattern として名指す：

> per-VALUE の「tracked/untracked」状態を持つコンパイラはない。[…] Almide の `materialized_results` / `heap_elem_lists` ValueId 集合こそこの anti-pattern。「UNTRACKED subject に対する match」という wall は8つのどれでも表現できない。

Survey 3a は代替を直接記す：

> 形状と所有権を分離：Roc の match lowering は layout 駆動かつ全域。borrow 対 incref は、その同じ layout に対する後段の全プログラム ARC pass。[…]「untracked → match を拒否」はどちらのコンパイラにもない。**型が正しい scrutinee はすべて構造上 match 可能。**

<a id="l2--stale-the-recorded-root-cause-names-a-checker-that-was-replaced"></a>

### L2 — 古い記録：記された根本原因は置き換え済み checker を指す

`v1-value-model.md:275` は blocker をこう記す：

> `verify_ownership` は if-arm を FLAT に処理するため、1つの subject の2つの drop を見て false double-free とする（checker は then/else の相互排他をモデル化しない）。

現在 `lib_c.rs:735-742` は明確に逆を記す：

> BRANCH JOIN（証明済み checker の `CBranch` 規則を鏡映）：`IfThen`/`Else`/`EndIf` の各 arm は同じ entry state から動き、全 object の退出時 count が一致しなければならない。[…] arm を FLAT に fold する**旧モデル**は両 arm の event を数え、arm 間の相殺を黙って受理していた。

`OwnershipChecker.v` は対応する規則とテストを持つ（`check_line [COp Inc; CBranch [Inc] [Inc]; COp Dec; COp Dec] = true`、`check_line [CBranch [Inc] [Dec]] = false`）。

この読みが正しければ、**旧モデルで書いた gate は見直されなかった**ので、192認識器の一部はもはや存在しない制約を守っている。

> これは組織自体の教訓「単独の深い/Coq 診断は、難しい wall の全てで誤っていた」が警告する種類の主張そのもの。ここでは足場とする発見ではなく、最初に**測定すべきもの**として記す。Stage 0 参照。

<a id="l3--real-no-uniform-borrow-representation-for-a-heap-payload"></a>

### L3 — 実在：heap payload に均一な borrow 表現がない

`control_p2.rs:751`：非 `String` heap payload には「単一 slot の borrow 表現がまだない」。さらに現在 `Result` には len-as-tag（37ファイル）と cap-as-tag（36ファイル）の**2つ**の物理 layout があり、`Result[Unit, String]` は生成した stdlib 関数次第で物理的に2種類になる。layout が型の全域関数になるまでは、何に match するのか分からないため、一般 match lowering は存在できない。

これは `RESEARCH.md:231` に既に記録された採用計画 Stage A。

<a id="4-what-the-references-actually-do"></a>

## 4. 参照実装が実際にすること

メモからの孫引きでなく、`almide-references/` 内の以下の行で読んだもの。

<a id="koka--kokasrcbackendcparchs-parcguard"></a>

### Koka — `koka/src/Backend/C/Parc.hs`、`parcGuard`

```haskell
return $ \liveInSomeBranch -> ... do
  let dups = S.intersection ownedPvs liveInThisBranch
  drops <- filterM isOwned (S.toList $ liveInSomeBranch \\ liveInThisBranch)
```

2行。`liveInSomeBranch` は arm 全体の union で、各 arm は union が持ち自分が持たないものを drop。pattern 変数が owned/borrowed になるのは **scrutinee** に対する `isOwned`（`ownedPats`）による。`inferShapes` は scrutinee から payload への alias map を記録する。Almide が現在 blocker とする aliasing が、ここでは名前付き first-class 概念である。

<a id="lean-4--lean4srcleancompilerlcnfexplicitrclean"></a>

### Lean 4 — `lean4/src/Lean/Compiler/LCNF/ExplicitRC.lean`

`addPrologForAlt`（:421）は同じ規則に、Almide が現在持たない順序制約1つを加える：

```lean
-- These are derived values who are no longer kept alive by a (potentially transitive)
-- parent value in this alternative and must thus be incremented. It is crucial that
-- these increments happen before the decrements as the decrements might contain an
-- operation that frees a parent.
```

原コメント訳：この alternative では、これらの派生値を（推移的かもしれない）親値がもう生かしていないため increment が必要。decrement は親を解放する操作を含み得るので、これらの increment を decrement より前に行うことが極めて重要。

`cases` 箇所（:624-653）は union を明示構築する：

```lean
let caseLiveVars := alts.foldl (init := {}) fun acc ⟨_, altLive⟩ => acc.union altLive
modifyLive fun _ => caseLiveVars
```

`withCtorAlt cs.discr c` は payload bind を**discriminant 由来**として登録。既定は borrow、bind が親より長生きする場合のみ increment。

<a id="roc--rocsrclirarczig"></a>

### Roc — `roc/src/lir/arc.zig`

match lowering は layout 駆動かつ全域、ARC は同じ layout 上の別の全プログラム pass。payload read は意図的に所有権を伝播**しない**（borrow＋anchor）。各 exit は `state − keep` を emit。

<a id="grain--graincompilersrccodegengarbage_collectionre719-731"></a>

### Grain — `grain/compiler/src/codegen/garbage_collection.re:719-731`

structured wasm 上の RC で、我々に最も近い target 形状。`MReturn` は**返却値を飛ばして**全 live managed binding を完全 cleanup。

<a id="the-law-44"></a>

### 法則、4/4

**型が正しい全 scrutinee は match 可能。** 所有権は後から、branch rule が `union ∖ here` の liveness 計算で決まる。payload 表現を理由に match を拒否する参照実装はない。

<a id="5-the-key-fact-almide-already-wrote-kokas-rule-in-the-wrong-place"></a>

## 5. 重要な事実：Almide は Koka の規則を既に書いたが、場所を誤った

`lower/result_match_value.rs:227-244`：

```rust
fn emit_arm_release_parity(
    &mut self,
    consumed_by_then: &[ValueId],
    consumed_by_else: &[ValueId],
    else_marker_at: usize,
) {
    for x in consumed_by_then {
        if !consumed_by_else.contains(x) { /* drop after THEN */ }
    }
    for x in consumed_by_else {
        if !consumed_by_then.contains(x) { /* drop at the ELSE marker */ }
    }
}
```

原コメント訳：THEN の後で drop、ELSE marker の位置で drop。

これは `liveInSomeBranch \\ liveInThisBranch` そのもの。参照実装との差は概念でなく全て構造的：

| | 現在の Almide | Koka / Lean |
|---|---|---|
| 範囲 | **1つ**の認識器内 | 1 pass、全 branch |
| arity | 2 arm | union fold で n arm |
| 入力 | 手収集した `consumed_by_*` list | arm ごとに**計算する** liveness |
| 順序 | drop のみ | **inc が dec より先**（Lean：dec は親を解放し得る） |
| borrow | 形状別 gate | `withCtorAlt` / `inferShapes` alias map |

従って規則の発明ではなく、**既存規則を1 brick の外へ引き上げ**、計算した liveness で駆動し、その規則が埋め合わせていた gate を削除する作業。

<a id="6-plan"></a>

## 6. 計画

<a id="stage-0--measure-before-building-no-production-code"></a>

### Stage 0 — 構築前に測る（production code なし）

以下の全計画は L2 に条件付く。まず実証する。

1. wall corpus を構築：`lower_tail_heap_match` と `try_lower_variant_value_match` が現在拒否する全形状について、期待実行出力を持つ最小 `.almd` を1つずつ。
2. FLAT-arm モデルを引用する各 gate について**その gate だけ**を外し、全 gate（corpus-wall ACCEPT×3性質、byte-verify、cargo test、output parity）を走らせる。無変更で通る形状を記録。
3. 件数を公表。有意な割合が通れば Stage 3 は大幅縮小し、Stage 1～2 より部分的に先行できる。

成果物：`shape → still-blocked? → by what` の表。何も merge しない。

**単独で行わない。** 組織の記録された教訓は、単独の深い/Coq 診断が全ての難しい wall で誤り、独立した fleet map と full-gate 検証が毎回健全な答えを見つけたというもの。Stage 0 をその方式で行う。

<a id="stage-1--one-type-one-layout"></a>

### Stage 1 — 1型、1 layout

len-as-tag と cap-as-tag の `Result` 表現を1つの canonical layout に統一し、`layout = f(type)` を成立させる。`RESEARCH.md:231` Stage A で、そこで約70ファイルと見積もる。`control_p2.rs:751` が欠けるとする単一 slot payload borrow を追加。heap payload 全体に**1つ**の borrow 表現であり、payload 型ごとではない。

gate：受理プログラム不変。この段階は純粋な表現変更。

<a id="stage-2--hoist-the-union-rule-into-a-pass"></a>

### Stage 2 — union 規則を pass へ引き上げる

lowering 後、`verify_ownership` 前に走る MIR pass：

1. branch arm ごとに live set を計算（Lean の `withCollectLiveVars`）。
2. `caseLive = ⋃ altLive`。
3. 各 arm：owned かつ非 borrow の `caseLive ∖ altLive` に `dec`、親より長生きする bind の borrowed 値に `inc`。**inc を dec より前に emit**。
4. payload bind は既定で borrow、subject 由来（`withCtorAlt`）。今の lowering を阻む alias を避けるのでなくモデル化する。

Almide の `CBranch` checker rule がずっと待っていた pass。checker は arm の一致を要求し、この pass が一致を**作る**。

gate：現在受理する全プログラムを引き続き受理し、旧経路が既に parity drop を emit していた箇所は byte-identical。`emit_arm_release_parity` は不要になり削除。

<a id="stage-3--make-the-lowering-total-delete-the-recognisers"></a>

### Stage 3 — lowering を全域にし、認識器を削除

layout が全域（Stage 1）、所有権は後段処理（Stage 2）となれば、`lower_tail_heap_match` は `Unsupported` fallback のない layout 駆動 dispatch になる。認識器を依存順で、毎回 full gate を通して退役させ、Camp-4 コメントも共に削除。

削除見込み：6 gate ファイルの3,817行の大部分と、Stage 0 で既に不要と判明した部分。`RESEARCH.md:434` は別に、同じ primitive で退役する9つの `!` 位置 pass を約2,800行と見積もる。

<a id="7-relationship-to-survey-4s-opreturn"></a>

## 7. Survey 4 の `Op::Return` との関係

`RESEARCH.md:422`（Survey 4e）は Zig 型の frame-targeted `Op::Return { val }` を推奨。これは9つの位置別 `!` desugar という**別の** wall であり、代替でなく補完：

- `Op::Return` は**値位置の early exit**を直す。
- 本計画は**heap subject で match し heap result を作ること**を直す。

両者は同じ verifier 作業（branch frame の `diverged`、`Return` の exit obligation）に収束するので、Stage 2 の pass は最初から diverging arm を受け入れる設計にする。

<a id="8-what-would-falsify-this-plan"></a>

## 8. 何がこの計画を反証するか

- **Stage 0 で全 gate がなお必須と分かる。** L2 は誤りで、6月分析が成立し、Stage 1 の borrow 表現が全コストを負う。
- **kernel 変更なしでは Almide の structured、certificate 付き MIR 上で union 規則を表現できない。** Koka と Lean は unstructured IR に emit。Grain が structured-wasm の参照で、Stage 2 commit 前に全文を読むべき。
- **layout 統一が byte 検証を壊す。** Stage 1 は全 `Result` 返却 stdlib 関数の emit byte に触れ、certificate を再確立できなければ順序を逆にする必要がある。

<a id="9-provenance"></a>

## 9. 由来

- wasm 向け多 pattern DFA matcher [almide/dfa](https://github.com/almide/dfa) 作成時に wall 発見、almide/almide#1492 起票。
- 読んだ参照：`koka/src/Backend/C/Parc.hs`（parcGuard）、`lean4/src/Lean/Compiler/LCNF/ExplicitRC.lean:421,624`、`roc/src/lir/arc.zig`、`grain/compiler/src/codegen/garbage_collection.re:719`。
- 引用の `RESEARCH.md` 総合／Survey 3a、4 はこのリポジトリ snapshot に含まれない。下記の行参照は歴史的由来であり、解決可能な in-tree source anchor ではない。
- リポジトリ内先例：[v1-value-model.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-value-model.md) の所有権／再帰 drop 議論。
