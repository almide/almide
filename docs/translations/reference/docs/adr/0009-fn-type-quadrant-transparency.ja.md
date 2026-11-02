<a id="adr-0009-function-types-complete-the-capability-quadrant-fn-params-are-transparent-by-default"></a>

# ADR-0009: 関数型で能力の 4 象限を完成させる — fn パラメータは既定で透過的

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](0009-fn-type-quadrant-transparency.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](0009-fn-type-quadrant-transparency.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](0009-fn-type-quadrant-transparency.zh-CN.md)

> [日本語・英語混在の正本](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0009-fn-type-quadrant-transparency.md)を 35297a612fb29e3f78f6587aba253aa91d51f466 に固定した非規範的な読者向け翻訳です。自動同期されません。正本が引き続き権威を持ち、規範的な記述はこの読者版ではなく原本に属します。歴史的な実装・互換性・健全性・測定の主張は独立した実行や検証を行わず保持しています。コードブロックとコメントは原文のままです。

- **状態**: 採択済み
- **日付**: 2026-08-06
- **決定範囲**: fn 型(パラメータ位置の関数型)における effect / 可謬性の表現 —
  4象限の導入、素の fn 型パラメータのデフォルト意味論、明示スロットの用途、
  effect ロンダリング穴の閉じ方
- **関連**: [ADR-0002](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0002-fallibility-effect-orthogonal.md)(宣言側の4象限 —
  本 ADR はその型側への鏡映)、[ADR-0006](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0006-fallibility-polymorphic-hofs.md)
  (`!` ビットの1ビット多相 — 本 ADR で2ビットに完成)、
  [ADR-0008](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0008-explicit-propagation-only.md)(全明示 — lambda 内 `!` の意味を確定)、
  #1055(本 ADR が設計を与える)、#1051 / #489(現行の文脈資格モデル)
- **経緯**: 2026-08-06、エラー表面 matrix の #1055 タグ 15 セルの深掘りから。
  10 セルは ADR-0006/0008 で既に解消しており、残り 5 セル(effect ロンダリング健全性
  バグを含む)を○×裁定で解決。

<a id="context"></a>

## 背景

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

<a id="decision"></a>

## 決定

<a id="d1-fn-型は4象限を持つadr-0002-の宣言文法の鏡映"></a>

### D1. fn 型は4象限を持つ(ADR-0002 の宣言文法の鏡映)

```almide
(A) -> B            // pure ・総
(A) -> B!           // pure ・可謬(ADR-0006)
effect (A) -> B     // effect・総 — 2026-09-24 撤回(ADR-0002 §D6): effect は可謬を含意し、下の行と同義
effect (A) -> B!    // effect・可謬
```

<a id="d2-素の-fn-型パラメータは透過-引数のビットが呼び出しへ流れる"></a>

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

<a id="d3-明示スロットは資格の要求固定"></a>

### D3. 明示スロットは「資格の要求・固定」

`effect (A) -> B!` と明示されたスロットに検査される lambda は、その資格の
本体エルゴノミクス(effect 呼び出し可・`!` 使用可)を得る。ランタイム契約を持つ
API はこれで宣言する:

```almide
effect fn serve(port: Int, f: effect (HttpRequest) -> HttpResponse!) -> Unit
// handler の err はランタイム契約(almide_http_serve)により 500 応答へ
```

<a id="d4-象限の上方包摂は暗黙無損失"></a>

### D4. 象限の上方包摂は暗黙(無損失)

pure lambda を `effect` スロットへ、総 lambda を `!` スロットへ渡すのは暗黙に合法
(要求より少ない資格しか使わないのは常に安全 — Swift の非 throwing closure が
throws スロットに入るのと同型)。ADR-0003 の無損失原理の適用例。

<a id="d5-実装の段階は-1108-に接続"></a>

### D5. 実装の段階は #1108 に接続

`!` ビット = #1108(Phase 2)。effect ビットは同じ機構の Phase 3 として積む。
Phase 3 切替時に #1051 の文脈資格ルールは D2 の型規則に置き換わる
(D2 により観測挙動の破壊はない)。#489 の不変条件(伝搬はクロージャ境界を
越えない)は維持 — lambda 内の `!` は lambda 自身のチャネルに落ちる(ADR-0006)。

<a id="rationale"></a>

## 根拠

- **軸の完結**: 0002 が宣言に立てた2軸を、0006 が型の片ビットに載せた。
  もう片方を載せない理由がなく、載せないままの文脈資格モデルは実測で
  健全性バグを漏らしている
- **透過デフォルトは Swift rethrows の実績ある形**: 「暗黙でビットを流す」は
  10 年成立している設計。注釈の義務ゼロで既存コードが全部生きる
- **閉じた2ビットの位置**: Swift(1ビット・効果は野放し)と Koka / OCaml 5
  (row 多相・強力だが推論が重く事前分布も薄い)の中間。予測可能性優先の
  almide に合う唯一の点
- **健全性**: purity.rs が信頼する「pure fn は効果を実行しない」が型で保証される

<a id="alternatives--検討して却下した案"></a>

## 代替案 — 検討して却下した案

1. **厳密デフォルト**(素の fn 型 = pure 総のみ受理): 既存の効果つき HOF lambda が
   全滅し、全 HOF に注釈を書く移行が要る。**却下**。
2. **文脈資格モデルの維持 + 穴の個別修理**: ロンダリングと非対称拒否は同モデルの
   構造的な産物で、修理しても角が生え続ける。**却下**。
3. **row 多相(Koka 型)**: S コスト最大・推論エラーが out-of-distribution。**却下**。
4. **effect ビットを fn 型に載せず named fn 専用に留める**: http.serve の handler が
   型で書けず、#1055 の原題が解けない。**却下**。

<a id="consequences"></a>

## 帰結

- 型文法に `effect` 前置 + `!` 後置の fn 型が入る(パーサ・checker・mono・
  native ⇄ wasm)。generic HOF は最大4態のモノモーフ化
- #1051 の文脈資格ルールは Phase 3 で型規則に吸収(挙動互換)
- ロンダリング穴(#1055 セル)は Phase 3 で check エラー化 — それまでの間は
  既知の健全性バグとして #1055 に記録
- http.serve 等ランタイム契約 API の handler 型が宣言可能になる

<a id="falsifier"></a>

## 撤回条件

1. **透過により generic HOF 内で callback の純粋性を仮定できなくなり、
   walled-real ベースラインの性能 ratchet が破られた場合** — 明示 opt-in の
   多相(rethrows キーワード型)へ後退する。
2. **4態モノモーフ化がバイナリサイズ予算(minigit 418KB 系の計測)を有意に
   膨らませた場合** — 態の共有(Result 形での単一実装)へ実装を寄せる。
3. **dojo で透過スロットと明示スロットの書き分けが有意な誤り源になった場合** —
   明示スロットの用途を stdlib 内部(ランタイム契約 API)に限定する。

<a id="references"></a>

## 参考資料

- Swift — [rethrows](https://docs.swift.org/swift-book/documentation/the-swift-programming-language/declarations/#Rethrowing-Functions-and-Methods)、SE-0413(非 throwing → throws スロットの包摂)
- Koka — [Effect types](https://koka-lang.github.io/koka/doc/book.html#sec-effect-types)、
  OCaml 5 — effect handlers(row 側の対比)
- 内部: #1055(原題スケッチ)、#1051 / #489(現行モデルの固定点)、
  2026-08-05 matrix の `purity/*`・`effectctx/*` セル(ロンダリング probe 保存済み)、
  ADR-0002 / 0003 / 0006 / 0008

<a id="implementation-note--the-effect-bit-through-values-4151-2026-10-25"></a>

## 実装注記 — 値を通る effect ビット（#4151、2026-10-25）

値として使われる名前付き effect fn（`list.map(xs, fs.read_text)`、ユーザー HOF、レコードフィールド、pure helper を通じて下へ渡されるクロージャ）では、D2 のロンダリング穴はまだ開いていました。次のように閉じます。この ADR が規定していない部分では、以下の保守的な選択をしています。

1. **effect fn を引数として名指すことは effect 呼び出しです**（checker における D2 の規則）。裸の名前は pure fn からなら既に E006 でしたが、モジュールパス（`fs.read_text`）も同様になりました。明示的な `effect` スロット（D3）は例外です。
2. **ビットは `Ty::Fn` ではなく値に乗ります。** D5 は型ビットを計画していました。代わりに、既に ADR-0026 の category 集合を運ぶ値フロー解析（`crates/almide-codegen/src/effect_flow`）が、同じマスクのもう 1 ビットとして運びます。`effect fn` 自身のノードと、未走査の stdlib `effect fn` 呼び出しすべてがこれを立てます。レコードフィールド、リスト、option、クロージャを通って値を追います（ADR-0026 D1: 保存された値は集合を保持する）。新しい表面構文も、モノモーフ化の態数を増やすものもありません。
3. **E109。** pure fn の解決済み集合が具体的にそのビットを持つ場合、経路を添えて拒否します。これは読み取る値を通じて持つ場合であり、自身の fn 型パラメータを通じた場合ではありません（後者は D2 の透過性であり、その呼び出し地点で計上します）。素の fn 型のレコードフィールドは*透過的ではありません*。拒否されるのはそれを実行する pure fn であり、それを保存した effect fn ではありません。
4. **精度より健全性を優先します。** 追跡対象の位置を離れた値は fn シグネチャごとにプールされ、プールされた値の呼び出しにはプール全体を計上します。同じシグネチャで effect fn を実行する fn がどこかでリストに格納されていれば、リストから fn を読む pure fn は拒否されます。リポジトリでの実測（spec/、stdlib/、examples/、tests/: 3,459 ファイル、build 経路；全 5,102 `.almd` ファイル、`check`）では、正当なプログラムの拒否はありませんでした。pure `main` から `process.exec` を渡していた 2 つの fixture は `effect` に変更しました。
5. **Abort は effect ではありません。** `process.exit`（本体なし、`-> Never`）はそのビットを持ちません。これは ADR-0022 の abort で、lower 済み `assert` の arm が呼び出します。名前リストではなく宣言から読み取ります。
6. **変更していない点:** `effect (A) -> B` の値を素の `(A) -> B` パラメータに渡すと、依然として check は通り（unify は `is_effect` を無視）、その後 2 種類の返り値表現により rustc で失敗します（E0271）。これは独立した型付けの穴であり、ロンダリングではありません。解析は呼び出し地点で計上しています。

## 原本に関する注意

当初の 4 象限と移行破壊ゼロの主張は、その歴史的段階の決定を記述しています。保持したコードコメントのとおり、D1 の total-effect 象限は 2026-09-24 に撤回されています（ADR-0002 §D6）。2026-10-25 の #4151 追記は D5 の型ビット計画を値フロー追跡で置き換え、モノモーフ化の態数を増やしません。プール解析は保守的で、正当なプログラムの拒否がなかったとの実測報告は測定対象のリポジトリコーパスに限られ、偽陽性が普遍的にないという意味ではありません。E0271 の型付けの穴は明示的に未解決のままであり、Abort は Effect ではありません。この読者版は各段階を、より強い現在の保証へと統合しません。
