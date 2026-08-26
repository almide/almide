# ALS §B — The Bounded Profile (normative)

> Last updated: 2026-08-21

> **Status**: normative（ADR-0017、2026-08-21 裁定）。本章は**有界プロファイル**
> — 言語の**サブセット**であって方言ではない — を定める。`@bounded` を付けた関数は、
> 実行時間と記憶域が**静的に有界**で、呼び出しグラフが**閉じ**、効果が **capability
> で有界**であることを、実装が**検査器の判定として**保証する。判定は観測可能
> （受理 / 拒否コード）であり、各節の拒否規則は `tests/diagnostics/e07x-bounded-*/`
> の broken / fixed 対で実行検証される。本章は実装より先に規範化された
> （CONTRIBUTING「要求が先」）— 2026-08-21 時点の 0.58.0 リリースは `@bounded` を
> 未知の属性として警告（E053）し拒否規則を持たないので、本章の拒否テストはその
> リリースに対して赤である。それは判定者がリリースを記述している状態であり、
> 実装がピンを前進させると緑になる。
>
> 「bounded」はこの言語で**上限を持つこと**を指す一語である: `fan.bounded(c) { … }`
> （ALS-DT2）は計算を**実行時に予算で**有界化し、`@bounded`（本章）は関数を
> **コンパイル時に証明で**有界化する。機構は異なり、意味は同じ。
>
> **本章が表現しないもの（限界の自己申告 — 主張と取り違えないために）**:
> (1) 有界な深さの再帰（全面禁止、B6）; (2) 固定小数点および決定性 Float 演算
> （Float 演算は暫定禁止、B10 — 解禁条件は ALS-T19 ファミリーの規範化と認証席の
> Float 命令集合の両方）; (3) 静的サイズ配列（全ヒープ容器は動的サイズ、B8 は
> 実行時長の構築を拒否する）; (4) バイト単位のメモリ上限（オブジェクト数上限のみ）;
> (5) 上限以下（`≤ B`）の早期脱出（B5/B11 は厳密上限のため禁止）; (6) 機能正しさの
> トレーサビリティ — 本プロファイルは**安全**（メモリ・名前・capability・上限）を
> 保証し、制御則が**正しい**かは保証しない。
> Mirror status: this file mirrors ALS-B1 and ALS-B2 from the canonical almide/als
> normative text. Sections ALS-B3..ALS-B11 (the bounded checker's admission rules)
> enter this mirror together with the E070-E079 checker implementation — the
> contract ledger cites a section only once its evidence runs in this tree.

## ALS-B1 `@bounded` 属性と有界プロファイル

`@bounded` は `fn` / `effect fn` 宣言に置く属性で、その関数が有界プロファイルに
属することを宣言する。属性を置ける位置は**関数宣言のみ**（モジュール宣言に置く
糖衣は存在しない）。`@bounded` 関数が呼んでよいのは `@bounded` 関数と pure な
標準ライブラリモジュールの一階関数だけであり（B7）、ゆえに `@bounded` 関数から
到達可能な呼び出しグラフは閉じている。属性は型にも値にも影響しない: `@bounded`
関数は通常の関数としてそのまま呼べる。

プロファイルの各規則に違反した `@bounded` 関数は**型検査時に拒否**され、診断
コードは **E070–E079**（本章に予約）、メッセージは
`<construct> is not admissible in a @bounded function` の形で、各節が名指す
hint を伴う。違反は `@bounded` でない関数には一切影響しない（サブセットは
属性の内側だけを狭める）。

```almide
@bounded
fn scale(x: Int) -> Int = x * 3

test "a @bounded function is an ordinary function" {
  assert_eq(scale(4), 12)
}
```

テスト: `spec/wasm_cross/bounded_kernel.almd`、`spec/stdlib/bounded_profile_test.almd`。
Contracts: C-308。

## ALS-B2 サブセットであって方言ではない

`@bounded` が付いたプログラムは、属性を全て取り除いても**同じプログラム**である:
観測可能な挙動（stdout・stderr・exit code）は属性の有無で変わらない（SPARK ⊂ Ada
と同じ関係）。属性が変えるのは「検査器がさらに何を拒否するか」だけである。
`spec/wasm_cross/bounded_kernel.almd` と `bounded_kernel_plain.almd` は属性の
有無だけが異なる同一プログラムで、同じ出力を印字する。

```almide
@bounded
fn grid_sum() -> Int = {
  var acc = 0
  for i in 0..<4 { for j in 0..<3 { acc = acc + i * j } }
  acc
}

fn grid_sum_plain() -> Int = {
  var acc = 0
  for i in 0..<4 { for j in 0..<3 { acc = acc + i * j } }
  acc
}

test "the attribute changes nothing observable" {
  assert_eq(grid_sum(), grid_sum_plain())
  assert_eq(grid_sum(), 18)
}
```

テスト: `spec/wasm_cross/bounded_kernel.almd`、`spec/wasm_cross/bounded_kernel_plain.almd`。
Contracts: C-309。
