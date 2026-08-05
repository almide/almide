# ADR-0003: Error-type conversion at propagation points — String is the terminal error type

- **Status**: Proposed(討議用ドラフト。D2 は ADR-0002 D4 の一部修正を含む — 批准時に 0002 側へ追記する)
- **Date**: 2026-08-05
- **決定範囲**: 伝搬点(明示 `!`・effect/fallible 文脈の auto-`?`)における
  エラー型 E の変換規則 — 一致・不一致(両方向)・変換フックの是非・変換時の文字列形式
- **関連**: [ADR-0002](./0002-fallibility-effect-orthogonal.md)(D4 が本件を仮決め、
  変換フックを open question として残した)、#1103(Phase 1)、
  C-029 / C-210 系(エラー文字列の byte 一致契約)
- **経緯**: 2026-08-05 の実測 matrix で `!` の E 型 3 セルが「一致=完動 /
  String→CustomE=ICE / CustomE→String=暗黙 Debug 文字列化」と三者三様に割れている
  ことが判明。一致セルの完動が本 ADR の出発点。

## Context — 実測が示した現状

```almide
type AppError = | NotFound(String) | Io(String)
```

| operand の E | 囲む宣言の E | 実測(v0.53.6) |
|---|---|---|
| AppError | AppError | ✅ 無変換で構造のまま伝搬。`err(NotFound(m))` で match 可能 |
| String(`int.parse` 等) | AppError | ❌ check 素通り → codegen で「Almide bug」ICE(rustc E0277) |
| AppError | String | ⚠ 黙って `map_err(\|e\| format!("{:?}", e))` 挿入 — Rust **Debug** 形式の文字列に劣化 |

effect fn 内の auto-`?` も同一機構なので同じ 3 セル構造を持つ(実測済み:
`effect fn -> Int` 内の CustomE operand は同じ暗黙 Debug 変換)。

背景事実:

1. **stdlib の E は String 単一文化**。`int.parse`・`value.field`・`fs.*`・
   `option.to_result`・`!` の暗黙 `err("none")`・`-> T!`(ADR-0002)— すべて String。
2. **main の失敗チャネルは String**。未処理 err は最終的に `Error: <msg>`(String)
   + exit 1 に落ちる。つまり**どのエラーも最外周では必ず String に到達する**。
3. `!` には既に「String チャネルへ向かう暗黙の正準変換」の前例がある:
   Option operand の none → `err("none")`。

## Decision — 何を決めるか(提案)

**エラー型の半順序において String を ⊤(terminal error type)と定める。
伝搬点での変換は「⊤ への上方変換のみ暗黙、それ以外は check エラー + 明示変換 hint」とする。**

### D1. E 一致は無変換(現状の一致セルを公式化)

operand E = 宣言 E なら値をそのまま伝搬する。構造化エラー(variant E)は
一致経路で match 可能性を完全に保つ。spec に「typed error の公式ルート」として明文化。

### D2. E → String は暗黙に許す — ただし正準 repr で(ADR-0002 D4 の修正)

CustomE operand を E=String の宣言(明示 `-> Result[T, String]`・`-> T!`・
現行 effect fn)の中で `!` / auto-`?` すると、**正準 repr**(`"${e}"` 補間と
同一バイト)への変換が暗黙に挿入される。

- 現行の Rust **Debug** 形式(`NotFound("nope")`)は廃止し、repr 形式に統一する
  (現挙動は §8-5 のバグとして修正 — 観測可能な出力変更なので契約 fixture を同 PR で)。
- ADR-0002 D4 は本方向を「廃止して map_err hint」と仮決めしていたが、本 ADR で
  **上方変換に限り暗黙を維持**へ改める。理由は Rationale。0002 批准時にこの分析は
  行っておらず、両方向を対称に扱ったのが仮決めの実体だった。

### D3. それ以外の不一致は check 時エラー(新 E-code)+ machine-hint

String → CustomE、および CustomE1 → CustomE2 は check エラー。ICE(§8-4)はこの
検査の実装で消える。hint は変換の雛形まで出す:

```
error[E0xx]: `!` は operand の err(String)を f の失敗チャネル(AppError)へ
             変換できない — 上方変換(→ String)以外は明示が必要
  try: result.map_err(int.parse(s), (e) => Io(e))!
```

(候補 ctor が 1 つに定まるとき(String を運ぶ case が唯一)は ctor 名まで埋める。
複数あれば列挙して選ばせる。)

### D4. From 型の暗黙変換フックは導入しない

Rust の `From`/`?` 相当(ユーザー定義変換の自動適用)は**導入しない**。
遠くの impl の有無で同じ `!` の意味が変わるのは予測可能性(LLM writability)を
下げる。dojo の MSR 計測で map_err ノイズが実害と示されたら、その証拠を持って
別 ADR で再考する(Falsifier 参照)。

### D5. 適用範囲は「伝搬点」で統一

明示 `!` と auto-`?` は同一機構として同一規則に従う。`?`(to-Option)と `??` は
E を破棄するので本 ADR の対象外。`try_*` family は E 多相なので対象外
(callback と戻りの E は同一型変数)。

## Rationale — なぜそれか

### String = ⊤ は既に言語の事実である

Context の背景 1〜3 の通り、String は「全エラーが最終的に到達する型」として
既に機能している。上方変換(E → ⊤)は全域的・一意・決定的で、失われるのは
構造だけ — そして**構造の喪失は署名に見えている**(中間 fn が E=String と宣言
している)。暗黙で失われるのは何もない。

### Swift の先例 — typed throws は ⊤ へ暗黙に広がる

Swift 6 の typed throws では `throws(E)` の関数を `throws`(= `throws(any Error)`)
文脈で呼ぶとき、E → any Error の上方変換は暗黙である。逆方向は明示キャストが要る。
本提案はこの非対称と同型で、almide では `any Error` の役を String が担う。

### Rust の対称形を採らない理由

Rust は方向を問わず `From` impl の有無で決める。強力だが、(a) エラー型ごとに
変換 impl を書く文化(thiserror 等のボイラープレート生態系)を要求し、
(b) `?` の挙動が非局所的な impl に依存する。almide は「読んだまま」を優先し、
上方 1 方向だけを言語規則(impl 不要・常に同じ)として固定する方を採る。

### none → err("none") との整合

`!` は既に Option operand を String チャネルへ暗黙変換している。D2 はこの前例の
一般化であり、新しい種類の暗黙ではない。逆に D4(0002)のまま CustomE → String を
禁止すると、「none は暗黙で String になるのに、NotFound は明示が要る」という
非一貫が生まれる。

## Alternatives — 検討して却下した案

1. **両方向とも strict(ADR-0002 D4 の仮決め)**: 規則は 1 行で対称だが、
   上方変換ですら `map_err(x, (e) => "${e}")` の儀式を課す。none→err("none") の
   既存暗黙と矛盾し、CustomE を使い始めた途端に main 近傍の糊コードが map_err
   まみれになる — typed error の採用を阻害する向きの摩擦。**却下**(D2 で修正)。
2. **Rust 流 From フック**: Rationale の通り予測可能性を優先して**却下**
   (証拠が出たら再考、D4)。
3. **指定 variant への自動 wrap**(String を運ぶ case へ自動注入): 候補が複数ある
   型で曖昧、型定義の変更で挙動が変わる。hint で ctor を提案する(D3)に留める。**却下**。
4. **変換子付き演算子**(`x!(Io)` 等): 新構文の割に map_err で書けるものの糖衣。
   グリフ予算に見合わない。**却下**。
5. **Zig 流 error set union**(E1 ∪ E2 を推論): エラー型が集合として合成される
   世界観ごと持ち込む必要があり、Result[T, E] の単純さを失う。**却下**。

## Consequences

- 一致セル(完動)が公式ルートになり、typed error のガイドが書ける:
  「ドメイン内は同一 E で貫き、境界で `!` に ⊤ へ落とさせるか、match で構造を読む」。
- §8-4(ICE)と §8-5(Debug 劣化)が同じ検査/変換の実装で消える。
- **観測可能な変更**: CustomE → String の文字列が Debug 形式から repr 形式に変わる
  (`NotFound("nope")` → repr 形)。契約 fixture + 両ターゲット byte 一致検証を同 PR で。
- 残る摩擦は「String operand を CustomE fn で使う」方向の map_err 明示のみ。
  緩和は D3 の ctor 埋め込み hint と、イディオム文書
  (「stdlib を触る下請けは E=String で書き、境界 fn で変換する」)。

## Falsifier

1. **dojo の MSR 計測で、暗黙の上方変換に起因する誤修正**(構造 match を期待した
   箇所が String 化していた等)**が有意に検出された場合** — D2 を strict へ戻す
   ADR で supersede(型検査が原理上防ぐはずなので、これが起きるなら検査の穴)。
2. **map_err ノイズが実コーパスで支配的**(境界 fn あたり複数回・全可謬 fn の
   相当割合)**と計測された場合** — D4 を撤回し From 型フックの ADR を起こす。
3. **repr 形式への変更が既存契約(C-029 系)と衝突し、両立不能と判明した場合** —
   D2 の形式選択を再考する。

## References

- 実測: 2026-08-05 matrix の `bang/pure-Result-CustomE-fn/*` ・
  `bang/effect-fn->T/CustomE-operand` 各セル(v0.53.6、probe 保存済み)
- Swift Evolution SE-0413 — [Typed throws](https://github.com/swiftlang/swift-evolution/blob/main/proposals/0413-typed-throws.md)
  (typed → untyped の暗黙上方変換)
- Rust — [`std::convert::From` and `?`](https://doc.rust-lang.org/std/convert/trait.From.html)、
  [The question mark operator](https://doc.rust-lang.org/reference/expressions/operator-expr.html#the-question-mark-operator)
- Zig — [Error Set Type](https://ziglang.org/documentation/master/#Error-Set-Type)(合併の対比)
- 内部: ADR-0002 D4(仮決めの修正対象)、C-211、`!` の none→err("none") 前例
