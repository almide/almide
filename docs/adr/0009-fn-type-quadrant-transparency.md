# ADR-0009: Function types complete the capability quadrant; fn params are transparent by default

- **Status**: Accepted
- **Date**: 2026-08-06
- **決定範囲**: fn 型(パラメータ位置の関数型)における effect / 可謬性の表現 —
  4象限の導入、素の fn 型パラメータのデフォルト意味論、明示スロットの用途、
  effect ロンダリング穴の閉じ方
- **関連**: [ADR-0002](./0002-fallibility-effect-orthogonal.md)(宣言側の4象限 —
  本 ADR はその型側への鏡映)、[ADR-0006](./0006-fallibility-polymorphic-hofs.md)
  (`!` ビットの1ビット多相 — 本 ADR で2ビットに完成)、
  [ADR-0008](./0008-explicit-propagation-only.md)(全明示 — lambda 内 `!` の意味を確定)、
  #1055(本 ADR が設計を与える)、#1051 / #489(現行の文脈資格モデル)
- **経緯**: 2026-08-06、エラー表面 matrix の #1055 タグ 15 セルの深掘りから。
  10 セルは ADR-0006/0008 で既に解消しており、残り 5 セル(effect ロンダリング健全性
  バグを含む)を○×裁定で解決。

## Context

現行モデル(#1051 で固定): **lambda は囲む fn の effect 資格を継承する**(文脈資格)。
fn 型には資格が載らないため、境界で実測 2 つの欠陥が出ている:

```almide
// 1. 健全性バグ: pure fn からコンビネータ経由で効果が実行できる(ロンダリング)
effect fn g(x: Int) -> Result[Int, String] = ...   // fs を触る
fn main() -> Unit = println("${list.try_map([1, 9], g)}")
// 実測: コンパイル通過・効果実行・exit 0。purity.rs の最適化前提を破る

// 2. 非対称な拒否: 同じ effect fn を auto-lift 形(-> Int)で渡すと
//    今度は E005 で拒否され、しかもエラーが lift 前の型を表示して意味不明
```

また http.serve のような「handler の資格をランタイム契約(Err→500)ごと固定したい」
API を型で書く手段がない(#1055 の原題)。

15 セルの残り(auto-? 系 6 セルは ADR-0008 が、lambda 内 `!` 禁止系 4 セルは
ADR-0006 が既に解消)。

## Decision

### D1. fn 型は4象限を持つ(ADR-0002 の宣言文法の鏡映)

```almide
(A) -> B            // pure ・総
(A) -> B!           // pure ・可謬(ADR-0006)
effect (A) -> B     // effect・総
effect (A) -> B!    // effect・可謬
```

### D2. 素の fn 型パラメータは「透過」— 引数のビットが呼び出しへ流れる

`f: (A) -> B` と書かれたパラメータは4象限のいずれも受け、**渡された引数の
ビットが HOF 呼び出し式に流れる**(ADR-0006 の1ビット規則の2ビット一般化。
各ビット独立):

```almide
fn apply(f: (Int) -> Int, x: Int) -> Int = f(x)

apply((n) => n * 2, 5)               // pure 総 → apply(...) も pure 総
apply((n) => int.parse(s)! + n, 5)   // 可謬   → apply(...)! が要る
apply((n) => count_files(n)!, 5)     // effect → apply(...) は effect 呼び出し
```

ロンダリング穴はこれで**正しい場所で**閉じる: `try_map([1,9], g)` は g の effect
ビットを受けて effect 呼び出しになり、pure main 内では E006 が **呼び出し側で**
出る。effect fn 内の効果つき map lambda(今日の合法コード)は無変更で合法 —
**移行破壊ゼロ**。

### D3. 明示スロットは「資格の要求・固定」

`effect (A) -> B!` と明示されたスロットに検査される lambda は、その資格の
本体エルゴノミクス(effect 呼び出し可・`!` 使用可)を得る。ランタイム契約を持つ
API はこれで宣言する:

```almide
effect fn serve(port: Int, f: effect (HttpRequest) -> HttpResponse!) -> Unit
// handler の err はランタイム契約(almide_http_serve)により 500 応答へ
```

### D4. 象限の上方包摂は暗黙(無損失)

pure lambda を `effect` スロットへ、総 lambda を `!` スロットへ渡すのは暗黙に合法
(要求より少ない資格しか使わないのは常に安全 — Swift の非 throwing closure が
throws スロットに入るのと同型)。ADR-0003 の無損失原理の適用例。

### D5. 実装の段階は #1108 に接続

`!` ビット = #1108(Phase 2)。effect ビットは同じ機構の Phase 3 として積む。
Phase 3 切替時に #1051 の文脈資格ルールは D2 の型規則に置き換わる
(D2 により観測挙動の破壊はない)。#489 の不変条件(伝搬はクロージャ境界を
越えない)は維持 — lambda 内の `!` は lambda 自身のチャネルに落ちる(ADR-0006)。

## Rationale

- **軸の完結**: 0002 が宣言に立てた2軸を、0006 が型の片ビットに載せた。
  もう片方を載せない理由がなく、載せないままの文脈資格モデルは実測で
  健全性バグを漏らしている
- **透過デフォルトは Swift rethrows の実績ある形**: 「暗黙でビットを流す」は
  10 年成立している設計。注釈の義務ゼロで既存コードが全部生きる
- **閉じた2ビットの位置**: Swift(1ビット・効果は野放し)と Koka / OCaml 5
  (row 多相・強力だが推論が重く事前分布も薄い)の中間。予測可能性優先の
  almide に合う唯一の点
- **健全性**: purity.rs が信頼する「pure fn は効果を実行しない」が型で保証される

## Alternatives — 検討して却下した案

1. **厳密デフォルト**(素の fn 型 = pure 総のみ受理): 既存の効果つき HOF lambda が
   全滅し、全 HOF に注釈を書く移行が要る。**却下**。
2. **文脈資格モデルの維持 + 穴の個別修理**: ロンダリングと非対称拒否は同モデルの
   構造的な産物で、修理しても角が生え続ける。**却下**。
3. **row 多相(Koka 型)**: S コスト最大・推論エラーが out-of-distribution。**却下**。
4. **effect ビットを fn 型に載せず named fn 専用に留める**: http.serve の handler が
   型で書けず、#1055 の原題が解けない。**却下**。

## Consequences

- 型文法に `effect` 前置 + `!` 後置の fn 型が入る(パーサ・checker・mono・
  native ⇄ wasm)。generic HOF は最大4態のモノモーフ化
- #1051 の文脈資格ルールは Phase 3 で型規則に吸収(挙動互換)
- ロンダリング穴(#1055 セル)は Phase 3 で check エラー化 — それまでの間は
  既知の健全性バグとして #1055 に記録
- http.serve 等ランタイム契約 API の handler 型が宣言可能になる

## Falsifier

1. **透過により generic HOF 内で callback の純粋性を仮定できなくなり、
   walled-real ベースラインの性能 ratchet が破られた場合** — 明示 opt-in の
   多相(rethrows キーワード型)へ後退する。
2. **4態モノモーフ化がバイナリサイズ予算(minigit 418KB 系の計測)を有意に
   膨らませた場合** — 態の共有(Result 形での単一実装)へ実装を寄せる。
3. **dojo で透過スロットと明示スロットの書き分けが有意な誤り源になった場合** —
   明示スロットの用途を stdlib 内部(ランタイム契約 API)に限定する。

## References

- Swift — [rethrows](https://docs.swift.org/swift-book/documentation/the-swift-programming-language/declarations/#Rethrowing-Functions-and-Methods)、SE-0413(非 throwing → throws スロットの包摂)
- Koka — [Effect types](https://koka-lang.github.io/koka/doc/book.html#sec-effect-types)、
  OCaml 5 — effect handlers(row 側の対比)
- 内部: #1055(原題スケッチ)、#1051 / #489(現行モデルの固定点)、
  2026-08-05 matrix の `purity/*`・`effectctx/*` セル(ロンダリング probe 保存済み)、
  ADR-0002 / 0003 / 0006 / 0008
