<!-- description: Codegen bugs (effect fn unification) + DX papercuts — A/C landed, B spun off -->
<!-- done: 2026-04-20 -->
# DX & Codegen Papercuts

## 決着

このロードマップの 5 課題のうち、Codegen 系 (A-1/A-2) と Module-system spec 系
(C-1/C-2/B-3) は **v0.15.0 で全て解決**。残る DX 系 2 件 (B-1 / B-2) は別 arc
[`active/dx-testing-explain.md`](../active/dx-testing-explain.md) に切り出した。

### A-1 / A-2 (effect fn と built-in 混在) — 既に動作

v0.15.0 時点で両方通る。Roadmap 起票時の repro は過去の別 PR で効果的に消えていた:

- `[1, 2] |> list.fold(0, (acc, x) => { println("x"); acc + x })` は
  `println!("{}", "x")` に正しく展開 (macro `!` 欠落バグは修正済み)。
- `match cond { ok(_) => my_effect(), err(_) => { println("x"); 0 } }` は
  `effect fn` 内から bare effect fn を呼ぶ auto-? 伝播と整合して通る。

状況整理のため新規に最小再現を書いて両方緑を確認した後、roadmap doc が stale と
判断。追加実装不要。

### C-1 (`import x.{A, B}` 選択 import) — 既に動作

`import string.{len, trim}` も `import self.util.{greet, magic}` もそのまま通る。
Parser / resolution / codegen すべて揃っており、roadmap 起票時の「パーサーで認識
されない」は古い情報だった。

### C-2 (cross-module `let` 値アクセス) — v0.15.0 で修正

本項目だけが実バグだった。`let MAGIC: Int = 42` を `util.MAGIC` で参照すると
生成 Rust が `(*ALMIDE_RT_UTIL_MAGIC)` と scalar `const` を deref しようとして
rustc fail。加えて compound 型 (Result / Option / Map) では ascription lowering と
clone insertion の 2 バグが重なっていた。

v0.15.0 で以下 4 箇所を修正:

1. `walker/expressions.rs` — `ALMIDE_RT_` synthetic Var の lazy 判定を
   `CodegenAnnotations.lazy_top_let_names` set で行う (scalar Const は deref しない)
2. `frontend/src/lower/mod.rs` — TopLet 分岐の env.top_lets lookup を
   `{prefixed, unprefixed, expr_ty}` の 3 段 fallback に
3. `pass_clone.rs::split_clone_ids` — `ALMIDE_RT_` prefix を always-clone に追加
4. `canonicalize/registration.rs::infer_literal_type` を record / list / tuple /
   map / some / none / ok / err まで構造再帰に拡張 + `Checker.current_module_prefix`
   で module top_let の write-back を prefixed key にも流す

結果、Int / Float / Bool / String / List / Map / Set / Tuple / Option / Result /
nominal record / anonymous record / variant 全てが Rust と WASM 両方で cross-module
`let` として使える。

### B-3 (project-local import) — docs 反映済み

機能的には `import self.<sub>` が当時も動いていた。CHEATSHEET 追記で完了。

## 残件の行き先

B-1 (`almide test` stderr passthrough) と B-2 (`almide explain E003`) は純粋な DX
項目で、effect fn codegen や module system とは独立。
[`active/dx-testing-explain.md`](../active/dx-testing-explain.md) として再スタート。

## 学び

- ロードマップ起票時の「未実装」記述は時間とともに陳腐化する。新規に repro を
  書いて実挙動を確認してから修正着手するのがコスト的にも正しい (A-1/A-2/C-1 は
  全部この段階で閉じた)。
- cross-module `let` は小ネタに見えて 4 layer (walker / lower / clone pass /
  registration) にまたがる複合バグ。`ALMIDE_RT_` prefix という synthetic 命名規約
  が walker と codegen pass の複数箇所に散らばっており、「一か所 heuristic を
  足したら別箇所との整合が崩れる」典型だった。`lazy_top_let_names` のように
  事前計算した set を持ち回す方が、prefix 文字列判定のアドホックよりも長持ち
  する設計。
