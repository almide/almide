原文の説明 metadata：実装済み fold/lint の基盤と、その限定された coverage を伴う traversal-totality 設計。

<a id="codegen-traversal-totality"></a>

# Codegen の走査完全性

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](codegen-traversal-totality.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](codegen-traversal-totality.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](codegen-traversal-totality.zh-CN.md)

> 固定原文の非規範読者用スナップショット。正本は原文であり、自動同期しない。独立した実装検証や引用指示の実行は行わない。歴史的状態、計画、比較主張は原文の範囲を保持する。
> [Original source (English) / 原文（英語）](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/codegen-traversal-totality.md)

> 原文注記：冒頭更新は網羅的 fold 再帰と legacy debt が空の lint が実装済みとする。後の Phase 0/1、fold 表の NO 欄、最初の PR 提案は歴史的記録であり、現在の未実装作業ではない。pass 一覧と約80%は過去の監査 snapshot。構成による全面的保証という広い表現は設計目標であり、実際の lint は列挙 file と構文形式を、差分テストは有限の入力範囲を対象にする。全 traversal や全意味的誤コンパイルをカバーするものではない。


<a id="implemented-foundations-and-remaining-guarantee-scope"></a>

## 実装済みの基盤と、残る保証範囲

`fold_expr` は今は網羅的な `map_children` primitive（[fold.rs](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/crates/almide-ir/src/fold.rs)）を通じて再帰する。強制 lint は [traversal_totality_lint.rs](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/tests/traversal_totality_lint.rs) として存在し、`LEGACY_DEBT` list は空である。したがって以下の Phase 0/1 と最初の PR の指示は、欠けた基盤ではなく過去の実装計画である。

lint が管轄するのは列挙した source file と構文上の catch-all pattern であり、任意の走査の完全性を証明したり、すべての意味的誤コンパイルを取り除いたりするものではない。以下の pass 一覧と「約80%」の見積りは元の監査 snapshot であり、現在の全数調査ではない。

**目標：**「pass が node kind の内側への再帰を忘れた」を**compile error または CI failure**にし、黙った native↔WASM divergence には決してしない。これにより、残る最大の*漸近的* divergence class（「走査を続ければ見つかり続ける」）を、構成による*静的*保証へ変換する。

<a id="how-we-got-here"></a>

## ここまでの経緯

`DIV2`（PR #347）：`xs |> group_by(..)`、次に `map.keys(g) |> map(k => …get(g,k)…)`、次に `map.values(g)` が native build に失敗した（E0382、move 済み `g` の borrow）。WASM は正常に動いた。原因は map や ownership では**ない**。`CaptureClonePass::transform_expr` が手書きの `match &expr.kind { …; _ => {} }` を使い、`_ => {}` が **`Borrow` で包まれた subtree を黙って落とした**（`list.join(&list.map(keys, k => …g…), …)`）。そのため `&(…)` の下の closure が capture analysis から見えず、capture した `g` が事前 clone されなかった。

修正は欠けた wrapper arm を追加した。しかし codegen pass 全 37 個の監査は、**これは単発事案ではなく class だ**と示す。約24 pass が、再帰しない `_ => {}` / `other => other` で終わる `match expr.kind` の再帰を手書きし、それぞれ適切な入れ子で顕在化する潜在 divergence となる。漸近的 sweep は 1 件ずつ見つけ続ける。この class 全体を閉じたい。

<a id="the-key-insight"></a>

## 中心的な洞察

修正してきたすべての native-only divergence は同じ形である。**native が IR の既知情報を rustc に再導出させようとし、emit で十分に指定しない**、または（この class）**pass が IR を十分に走査せず subtree を黙って skip する**。どちらも「pass が事実を運ぶ / node を訪れる代わりに何かに依存した」ということ。静的な対処は、その運搬 / 訪問を*構成上完全*にすることである。

とくに走査では、pass は**変換する node だけ**について `match expr.kind` し、**他の再帰はすべて、compile によって網羅性が強制される単一 primitive に委譲する**べきである。手書き `_ => {}` には決して委ねない。

<a id="the-good-news-the-infrastructure-already-exists"></a>

## 朗報：基盤はすでに存在する

| Primitive | File | 変更方式 | 網羅的（`_=>` なし） | すべての wrapper child へ再帰 |
|-----------|------|---------|----------------------|------------------------------|
| `IrExpr::map_children` | `almide-ir/src/lib.rs` | 値渡し | **はい**（文書化された集約点） | はい |
| `IrMutVisitor` / `walk_expr_mut` | `almide-ir/src/visit_mut.rs` | その場で変更 | **はい** | はい |
| `IrVisitor` / `walk_expr` | `almide-ir/src/visit.rs` | 読取り専用 | **はい** | はい |
| `substitute_var_in_expr` | `almide-ir/src/substitute.rs` | 値渡し | はい | はい |
| `fold_expr` | `almide-ir/src/fold.rs` | 値渡し | **いいえ — L123 の `_ => {}`** | はい（現時点） |

`IrExpr::map_children` が模範である。その doc はすでに*「すべての variant を明示列挙する（wildcard なし）ことで、新しい `IrExprKind` を追加するとここが compile error になり、その child をどう走査するか作者に判断させる」*と述べている。`free_vars.rs` は**context を引き回す** analysis の模範 consumer である。binder node（`Lambda`/`Block`/`Match`/`ForIn`）だけを override して `bound` set を push/pop し、それ以外は `walk_expr` へ流すため、subtree を落とし得ない。`pass_concretize_types` と `pass_result_propagation` の `resolve_err_types` はすでに `IrMutVisitor` を正しく使っており、安全である。

したがって**約80%まで来ている**。作業は (a) 基盤の 1 つの穴（`fold.rs`）を閉じ、(b) 手書き pass をこれらの primitive に移し、(c) lint で固定して退行を防ぐことである。

<a id="plan"></a>

## 計画

<a id="phase-0--infra-small-pure-safety"></a>

### Phase 0 — 基盤（小規模、安全性だけ）
- `fold_expr` の `_ => {}` を削除し、再帰を `IrExpr::map_children` 上で実装し直す（または match を網羅的にする）。現時点で挙動変更はない。variant 追加が compile error となる。

<a id="phase-1--enforcement-lint-anti-regression-the-static-lock"></a>

### Phase 1 — 強制 lint（退行防止の「静的」な固定）

**敵は wildcard ではなく*沈黙*である。** DIV2 は `_ => {}`、つまり*何もしない* catch-all が subtree を黙って落とすものだった。wildcard はよい。*黙った no-op* wildcard が bug である。catch-all は 3 形式あり、禁止は 1 つだけ：

| 形式 | 例 | 判定 |
|------|---------|---------|
| **委譲**（正しい既定再帰） | `_ => walk_expr_mut(self, e)` | ✅ 構成上 DIV2 class を閉じる |
| **大きく失敗する暫定形**（Swift `Never` model） | `_ => unreachable!("…")` / `_ => todo!()` / `panic!` | ✅ 問題の program で*codegen 時*に trap し、gap を表面化させ、divergence を出荷しない。pass を段階的に書ける。 |
| **黙った no-op** | `_ => {}` / `_ => e`（再帰せず返す） | ❌ 唯一の禁止形。subtree を黙って落とす |

これは*言語自身*の `hole`/`todo` に対応する。暫定 body は typecheck でき（型が context と単一化する）、`todo!()` として生成する（Rust の `Never`。到達すると大きく trap する）。コンパイラ内部の walk も同じ規律に従うべきである。未対応の node kind は*明白に失敗*（`unreachable!`）するか*正しく再帰*（`walk_*`）しなければならず、*沈黙*してはならない。

- CI test（`tests/traversal_lint.rs` または build-script check）を追加し、`crates/almide-codegen/src/pass_*.rs` と `crates/almide-ir/src/*.rs` を禁止形について scan する。禁止形は `IrExprKind` 値に対する `match` の catch-all body が**黙った no-op**（空の `{}`、または再帰せず node を返す）であるもの。一方、`walk_*` primitive に委譲するか diverge する（`unreachable!`/`todo!`/`panic!`/`Never` 型の expr）catch-all は**許可**する。正準 primitive 自身は allow-list にする。新たな黙って落とす walker は CI を失敗させ、大きく失敗する暫定 walker は通る。
- これが保証を*ある時点の snapshot*ではなく*静的*にし、段階的 / 暫定 pass も書けるようにする（Swift の `Never` escape hatch）。Almide の modification-survival-rate の使命に沿う。

<a id="phase-2--migrate-the-mutating-rewriters-prioritized-by-audit-risk"></a>

### Phase 2 — 変更を行う rewriter を移行する（監査リスク順）
各移行は**挙動が微妙**である（catch-all の一部は意図的に非再帰だった）。移行後のすべての pass に差分 check を行う：spec sweep の `native == wasm` + 既存 suite。

| 優先度 | Pass | 監査の注記 |
|------|--------|------------------|
| **HIGH** | `pass_clone` (CloneInsertion) | `IterChain`/`RcWrap` を落とす — **現存する DIV2 の近縁**。clone insertion が fused chain を見ない。最初に修正する。DIV2 のもう半分である。 |
| | `pass_borrow_inference` | 手書き walker 5 個、いくつかの落とす catch-all。native borrow mode の中心。 |
| | `pass_stdlib_lowering` | 約36 variant に対する `rewrite_expr` の `other => other`。「DIV2 のリスクはここ」。 |
| | `pass_perceus` | WASM RC。全 helper が手書きで、ほぼすべての wrapper を落とす。 |
| **MED** | `anf`, `box_deref`, `capture_clone`, `fan_lowering`, `lambda_type_resolve`, `list_pattern`, `match_subject`, `mut_param_lowering`, `peephole`, `result_propagation`（lift 経路）, `rust_lowering`（idx 経路）, `tco` | それぞれ一部の wrapper/collection subtree を落とす。`capture_clone` はすでに部分修正済み（#347）だが、再び乖離しないよう `IrMutVisitor`+binder-stack pattern へ移すべきである。 |
| **LOW / analysis** | `auto_parallel`, `builtin_lowering`, `closure_conversion`, `const_fold`, `effect_inference`, `licm`, `match_lowering`, `matrix_shape_spec` | 大部分は広い coverage または読取り専用。**`effect_inference` が `IterChain` を落とすのは本物の潜在 bug**（fused chain 内 effect の誤分類）なので検証する。読取り専用 analysis は `IrVisitor` へ移行。 |
| **DEAD** | `pass_shadow_resolve` | `targets() == None`。決して実行しない。残すか削除。 |

<a id="phase-3--differential-ci-gate-shared-with-closure-cross-target-work"></a>

### Phase 3 — 差分 CI gate（closure cross-target 作業と共用）
well-typed IR generator + CI 内の `native` 対 `wasm` 実行 assertion（Perceus proptest とバイト同一 determinism gate の拡張）は、有限入力空間上に残る traversal/codegen divergence を捕える。継続的な機械検査という意味で「静的」であり、lint の背後の安全網である。

<a id="sequencing"></a>

## 順序

Phase 0+1 は低リスクで効果が大きく、保証を設置する。Phase 2 が作業の大半で、挙動が微妙なため、巨大な diff 1 本ではなく、**優先度別にレビュー済み PR 数本**とし、それぞれ差分検査する。Phase 3 は `closure-cross-target-completeness.md` と基盤を共用する。

**最初の PR 提案：** Phase 0（`fold.rs`）+ Phase 1（lint）+ HIGH の `pass_clone` 移行（実証された DIV2 の近縁）。これで pattern を端から端まで示し、既知の現存 bug を閉じ、lint が新たなものを防ぐ。
