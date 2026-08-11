# ADR-0012: Error-surface end state — refinement stays in the marker (`T!E`), erasure stays the default

- **Status**: Accepted(設計批准。実装は未着手)
- **Date**: 2026-08-11
- **決定範囲**: エラー型仕様の**終着形**。二層(消去層 = String / 精密層 = variant E)の
  層割当規準、精密層の表記 `T!E`、fmt 正準形、および非目標の固定(lambda / main /
  変換フック / union 系構造 / 暗黙伝搬)。
- **関連**: [ADR-0002](./0002-fallibility-effect-orthogonal.md)(**D2 を本 ADR が改訂** —
  「E は String 固定」を「E は String 既定・マーカー内で精密化可」へ)、
  [ADR-0003](./0003-error-type-conversion-at-propagation.md)(不変 — 伝搬点の無変換原理は
  そのまま `T!E` に適用される)、[ADR-0004](./0004-error-branchability-doctrine.md)(不変 —
  stdlib の String 終着・タグ/error set 却下は動かさない)、
  [ADR-0008](./0008-explicit-propagation-only.md)(不変・証拠追加)、
  ADR-0006 / 0009 / 0010(表記の整合)
- **経緯**: 2026-08-11、「2026 年時点で最高のエラー型仕様とは何か」を固めるための
  他言語サーベイ(MoonBit / Swift / Zig / Rust / Gleam / Roc / Unison / Inko / Koka /
  Flix / Effekt)を実施。各言語の 2024–2026 の一次資料(提案文書・リリースノート・
  issue 裁定・実運用報告)に基づく。同日、D1〜D4 を一問一具体物の○×で批准
  (D2 → D3 → D1 → D4 の順、全て○)。

## Context — 現状の非対称

Almide のエラー面は 0.54–0.56 のアークで二層構造に到達している:

- **消去層(既定)**: E = String。`T!` ≡ `Result[T, String]`(ADR-0002 D2)、
  effect fn の暗黙 lift、stdlib 全域、main の失敗チャネル。context は `map_err`
  イディオム + E036、string-match 分岐は E035 が封じる(ADR-0004)。
- **精密層(opt-in)**: variant E。`Result[T, MyError]` を明示で綴れば、E 一致の
  伝搬点で `!` が**構造のまま**伝搬し match 可能(ADR-0003 D1「typed error の公式ルート」、
  #1103 で ICE / 暗黙 Debug 劣化も消滅済み)。

意味論は完成している。**欠けているのは精密層の表記だけ**であり、その欠落は
2 つの実害を持つ:

1. **モデル跳躍**: 消去層の可謬は「関数の属性」(後置 `!`)で綴るのに、精密化した
   瞬間に「総称型」(`Result[T, E]`)へ表記モデルが変わる。Swift(`throws` →
   `throws(E)`)も MoonBit(`raise` → `raise E`)も精密化はマーカー内で完結する —
   モデルを跨ぐのは Almide だけ。
2. **精密化が本体書き換えを強制する**: pure fn の明示 `-> Result[T, E]` は tail lift を
   持たない(lift は `!` マーカーと effect fn の持ち物 —
   `crates/almide-syntax/src/parser/declarations.rs` の分岐)。よって
   `-> Config!` の関数を `-> Result[Config, ConfigError]` へ精密化すると、
   署名だけでなく**値 tail を `ok(...)` で包み直す本体編集**が要る。
   「エラー型を 1 段精密にする」という最頻クラスの修正が最小差分で済まない —
   MSR(modification survival rate)に直撃する非対称である。

## Survey — 2026 年の他言語実測(一次資料)

### 収斂 1: 「消去が既定・型付きは閉域の opt-in」は業界横断の到達点

| 言語 | 既定 | 精密層 | 証拠 |
|---|---|---|---|
| Swift | untyped `throws` | `throws(E)`(SE-0413) | 提案自身が逐語で「the existing (untyped) `throws` **remains the better default** error-handling mechanism for most Swift code」。出荷 2 年後の実利用は Embedded Swift とジェネリック機構(`AsyncSequence.Failure`)にほぼ限定 |
| Rust | `anyhow`(アプリ) | `thiserror`(ライブラリ) | 二極は 2026 年も継続(累計 DL ~8.6億 / ~13億)。std のエラー改善は事実上停止、provider API は 2026-02 に放棄方向(RFC 3885 へ転回) |
| MoonBit | 素の `raise`(≡ `raise Error`) | `raise E`(suberror) | 最新の blessed な `@string.parse_int` は**素の raise + メッセージ文字列**。core 内訳: `raise?` 329 / 素 `raise` 68 / 型付き 29。stringly か構造化かは core PR #3997 で**現在も係争中** |
| Unison | — | `Throw e` あり | 生態系は catch-all の `Exception`/`Failure` に収斂 — 精密層だけでは外周が回らないことの実証 |
| Zig | 型付き set が既定(逆張り) | — | 大規模では「境界は明示 set、内部は推論」の規律 + `anyerror` への侵食圧。推論 set の再帰非対応(#2971)は 2026 年も open |

### 収斂 2: エラーはペイロードを運べなければならない

Zig はペイロード提案 #2647 を 2024-12 に**恒久却下**(Andrew Kelley の閉鎖コメントは
一文 — 「**Error codes are for control flow.**」)。公認回避策の diagnostics
out-parameter は実務者から「extremely unergonomic」と評され、stdlib 内でも適用が
不整合(std.json は非対応)。Almide の variant E はコンストラクタにペイロードを
持てるため、この欠落を最初から持たない。

### 収斂 3: 境界の自動合成(union / row / set)は対価が高い

- **Roc**(構造的 open tag union、合成の極点): 境界儀式ゼロ。対価は open/closed
  推論が言語最難関概念になること。
- **Koka**(effect row): 自動マージ。対価は row 推論のエラーメッセージ難
  (2025 年の実務者報告で確認 — 内部型変数名がユーザーに漏れる)。
- **Zig**(error set): 合成は成立するが**閉世界コンパイルへの依存**が前提
  (matklad の分析)— 他言語へ輸出できない。
- **Inko**(決定的データ点): 「署名に単一の名義 throw 型」という最も単純な設計を
  実際に出荷し、**0.11.0 で撤回**。撤回理由の逐語:
  「This makes composing errors much easier」— 単一名義型の強制は境界で破綻した。
  ※ Almide は variant E を**強制しない**(既定は String)ので Inko の失敗形とは
  異なるが、「精密層を既定にしてはならない」ことの実証として引く。
- **Gleam**(名義 + 明示変換のみ、Almide と同形): `map_error` 儀式は残るが、
  コア判断は「変えない」(2024-11、`?` 演算子提案を却下)。生態系の逃げ道 snag
  (単一不透明型)は作者自身が「ライブラリでは使うな」と明記 — 消去層の需要の実在証明。

### 収斂 4(matklad, 2025-12): 現代エラーモデルの三点合意と「未提供の中間点」

Go/Rust/Swift/Zig は (1) **呼び出し点の可謬マーカー**、(2) **panic の別チャネル**、
(3) **値としてのエラー** に収斂した("The Second Great Error Model Convergence")。
Java の checked exceptions が失敗したのは E の変化が**全署名連鎖に波及**したため —
現代設計は 0→1(総→可謬)でだけ課金し、N→N+1(variant 追加)では課金しない。
そして「網羅的列挙と全消去の**中間点はどの言語でも十分に提供されていない**」。

Almide の現行意味論はこの三点をすべて満たし、variant E は名義型なので
**N→N+1 テストも通る**(ConfigError に variant を足しても署名は 1 本も変わらない)。
中間点(消去既定 + ペイロード付き精密層 + 損失変換の強制可視化)は既に占位している —
残っていたのは表記だけ、が本サーベイの結論である。

### MoonBit の `T!E` 放棄の解剖 — 記法ではなく呼び出し点モデルの問題だった

MoonBit は 2024–2025 に `fn f() -> Int!String` 表記を捨て `-> Int raise E` へ移行した。
経緯を分解すると、痛点はすべて**呼び出し点**と**グリフ過積載**にある:

- 呼び出し点マークが 2 年で 3 転(`f(x)!` → `f!(x)`〔IDE 補完都合と明記〕→ 廃止)
- `!` が `type!` / `f!` / `!!` / `catch!` で各々別意味という過積載
- `f?(g!(..))` の意味論トラップ(機械移行不能と自ら注記)

**戻り型表記 `T!E` 自体の欠陥を示す一次資料は存在しない**(専用の撤回文書なし)。
そして移行先の MoonBit は伝搬を**暗黙化**し、可視性を「IDE が可謬呼び出しに下線を
引く」ことで補償した(beta-release 逐語: "The IDE will automatically _underline_
functions that may throw")。この補償は **raw text をインターフェースとする書き手
(LLM・diff レビュー)には不可視**であり、Almide が ADR-0008 で明示伝搬に張った
理由そのものである。Almide の `!` は伝搬演算子と戻りマーカーの 2 義に留まっており
(trap は `!` に載せていない — ADR-0002 D4)、MoonBit の過積載条件は再現しない。

### 実証の穴(機会)

型付き E と消去 E が LLM の生成精度・修正生存率に与える影響を測った研究は
**存在しない**(2026-08 時点、LMPL 2025 にも該当論文なし)。MoonBit 以外に
「LLM 書きやすさ」を理由にエラー設計を公表した言語もない。MSR 計測基盤(Dojo)を
持つ Almide は、この空白を Falsifier 付きの実測で埋められる唯一の位置にいる。

## Decision

**二層エラーモデルを終着形として宣言し、精密層の表記をマーカー内で完結させる。
`-> T!E` ≡ `Result[T, E]` を fn 宣言の戻り位置と fn 型 slot に導入する。
既定は String のまま(`T!` ≡ `T!String`)、stdlib は String のまま(ADR-0004 不変)、
伝搬点の無変換原理はそのまま(ADR-0003 不変)。**

### D1. 二層ドクトリンの終着宣言(層割当規準の明文化)

spec に層割当を一段落で明文化する:

- **消去層(既定)**: E = String は**報告チャネル** — 人間/LLM が読む。context は
  `map_err` で前置(ADR-0004 D2)。
- **精密層(opt-in)**: variant E は**分岐チャネル** — プログラムが内容で挙動を変える。
  切替の合図は ADR-0004 D1 のドクトリン行そのまま(「内容で分岐したくなったら
  variant E」)。適用域は**閉域**(モジュール/パッケージ内でエラーを取り切る場所 —
  SE-0413 の 3 条件と同じ線引き)。
- 層間の降格(variant E → String)は境界で必ず `map_err` として可視(ADR-0003)。

この二層 + 無変換 + ペイロード付き精密層の組が、matklad の言う「未提供の中間点」の
充足であり、本 ADR が「2026 年のあるべき姿」として主張する形である。

### D2. `T!E` — 精密化はマーカー内で完結する

```almide
type ConfigError = | Missing(String) | BadValue(String)

// read_cache: (String) -> String!ConfigError
fn load(path: String) -> Config!ConfigError = {
  let text = read_cache(path)!          // E 一致 → 構造のまま伝搬(ADR-0003 D1)
  guard valid(text) else err(BadValue(path))
  parse_config(text)                    // 値 tail → ok(...) に lift(T! と同一の人間工学)
}

effect fn main() -> Unit = {
  let cfg = load(p) |> result.map_err((e) => "loading config: ${e}")!  // 降格は可視
  ...
}
```

- **脱糖**: `-> T!E` ≡ `-> Result[T, E]`。`T!` は `T!String` の省略形として再定義される
  (ADR-0002 D2 の改訂 — 「String 固定」から「String 既定」へ)。
- **位置**: `!` マーカーが今日合法な位置と完全に同じ — fn 宣言の戻り位置、
  fn 型 slot の戻り位置(`op: (Int) -> Int!ParseError`)。型構成子ではない
  (`List[Int!E]` は不可 — ADR-0002 D2 の属性モデル維持)。
- **E の文法**: `!` の直後・同一行に、**名前付き型アトム**(修飾名
  `mod.Error`・ジェネリクス可)。E 自身への `?` / `!` 後置は不可
  (Option の E が要るなら明示 Result で綴る — 需要は観測されていない)。
  現行文法では `!` の直後に型名が現れる余地が空いている(現状ここに来うるのは
  `=` `{` `,` `)` と改行のみ)ため、1 トークン先読みで曖昧性なくパースできる。
- **本体人間工学**: `T!` と**同一**(値 tail の ok-lift、`!` 伝搬、`err(...)` 合法)。
  これにより `T!` → `T!E` の精密化は**署名 1 箇所の編集で完結**し、Context 2 の
  本体書き換え強制が消える。
- **effect fn との合成**: `effect fn f() -> T!E` は effect・可謬(E 付き)。
  既存の「明示 Result は二重包装しない」規則の糖衣側であり、新規則は増えない。
- **伝搬**: 規則は不変 — 「`!` は E を変換しない」(ADR-0003)。`T!E` は既存規則の
  適用対象が増えるだけで、変換・推論・特例は一切足さない。

### D3. fmt 正準形 — 戻り位置の Result はマーカーへ

`almide fmt` は fn 宣言・fn 型 slot の**戻り位置**に現れた `Result[T, String]` を
`T!` へ、`Result[T, E]` を `T!E` へ正規化する(`T!String` と綴られたものも `T!` へ)。
戻り位置**以外**(let 注釈、フィールド型、型引数)の `Result` はそのまま —
`!` は arrow の属性であり値の型ではない、という ADR-0002/0010 の線をなぞる。

これは ADR-0002 が open question として残した「fmt がどちらへ正規化するか」の
クローズであり、ADR-0010 D3(`Option[T]` → `T?` 正規化)と同じ解法で
「1 型 2 記法」の緊張を潰す。正規化は意味保存(明示 Result の本体で合法なものは
lift 下でもすべて同型 — lift は受理を増やすだけで、既存の Result 型 tail の意味は変えない)。

### D4. 非目標の固定(再確認 + 新証拠)

1. **stdlib は String のまま**(ADR-0004 不変)。MoonBit core の実態
   (blessed パーサが素 raise + メッセージ、構造化は係争中)と Unison の
   catch-all 収斂が、消去層 stdlib の正しさを追認する。再考条件は
   ADR-0004 の Falsifier がそのまま生きる。
2. **変換フック(From 相当)・union / row / error set は導入しない**
   (ADR-0003 D4 / 0004 不変)。新証拠: Koka の row 推論メッセージ難(2025 実測)、
   Roc の open/closed 難、Zig set の閉世界依存、Rust provider API の放棄。
3. **伝搬の明示は不変**(ADR-0008)。MoonBit 型の「暗黙化 + IDE 補償」は
   raw-text インターフェースでは可視性がゼロになるため採らない — 本 ADR の
   サーベイでこの判断は**証拠を得た**(収斂 4 の呼び出し点マーカー合意も同方向)。
4. **lambda の失敗チャネルは String のまま**(ADR-0009 L3)。使用駆動で E を
   推論すると Koka 型の推論エラー難を輸入する。typed E の lambda は明示注釈の
   Result で綴る(普通の値として完動する)。
5. **main の失敗チャネルは String のまま**。境界降格の `map_err` が可視化点
   (ADR-0003 Consequences のとおり)。

## Rationale

### 精密化は表記モデルを跨いではならない

Swift は `throws` → `throws(E)`、MoonBit は `raise` → `raise E`、Zig は `!T` → `E!T` —
**精密化がマーカー内で完結しない言語は存在しない**。Almide だけが「属性 → 総称型」の
モデル跳躍を強制していた。跳躍は覚えるべき表記モデルを二重化し、さらに tail lift の
喪失(Context の実害 2)による本体書き換えまで連鎖させる。`T!E` はこの跳躍を消す
最小の一手である。

### 最小差分こそ MSR

「エラー型を精密にする」は閉域設計の最頻修正クラスである。D2 後、この修正は
署名 1 トークンの追加で完結し、伝搬点の E 不一致はすべて check エラー + 正準形 hint
(#1103 実装済み)が受ける。「エラー文言を読んで正準形を貼る」は LLM の最高成功率
クラスの修正(ADR-0003 Rationale)— 精密化の全経路が既存の安全網の上に乗る。

### String 単一文化の希釈リスクには表記ではなくドクトリンで答える

`T!E` が安くなると variant E が乱造される — この懸念は SE-0413 自身が逐語で警告した
ものと同型である("Resist the temptation to use typed throws because there is only
a single kind of error")。Almide の答えはドクトリン側にある: variant E へ切り替える
合図は**分岐需要**(ADR-0004 D1 のドクトリン行 + E035 lint)であって、綴りの安さでは
ない。仮に分岐しない variant E が書かれても実害は小さく、String へ降格する境界では
必ず `map_err` として可視化される。stdlib 面が String で固定されている限り(D4-1)、
単一文化の骨格は揺らがない。

### 儀式コストは Gleam 水準に留まり、Inko の失敗形にはならない

名義 E + 明示変換の対価(境界 `map_err`)は Gleam が 2024-11 に「許容」と裁定した
のと同じコストであり、Almide は hint 供給(#1103)で Gleam より軽い。
Inko が撤回した「単一名義型の強制」とは違い、Almide の精密層は opt-in で
既定が消去層なので、合成困難は閉域の外へ漏れない。

## Alternatives — 検討して却下した案

1. **keyword 方式(`raise E` / `throws(E)` スタイルの新キーワード)**: MoonBit の
   移行先と同形。却下 — Almide には既に後置 `!` マーカーが出荷済みで(ADR-0002)、
   キーワード追加は 1 意味 2 綴りを**言語コア**に作る。MoonBit が `!` を捨てた
   痛点(グリフ過積載・呼び出し点マーク)は Almide に存在しない。
2. **暗黙伝搬 + IDE 補償(MoonBit 2025 型)**: 却下 — ADR-0008 と正面衝突。
   IDE 下線は raw text に存在せず、LLM とレビュー diff には見えない。
   MSR の読者は将来の修正者であり、その修正者はテキストしか読まない。
3. **error set / 構造的 union / row(Zig / Roc / Koka 型)**: 却下再確認
   (ADR-0004 Alternatives 2/4)。新証拠は Survey 収斂 3 のとおり — 合成側の対価
   (推論メッセージ難・閉世界依存)は 2025–2026 の実測で裏書きされた。
4. **From 型変換フック**: 却下再確認(ADR-0003 D4)。Rust 自身の provider API 放棄
   (2026-02)が「遠くの impl に依存する機構」の整備コストを傍証する。
5. **現状維持(精密層は明示 Result のみ)**: 却下 — 表記モデルの跳躍と本体書き換えの
   強制(Context の実害 1・2)が残る。「動く」は真だが「最小差分で動く」が偽。
6. **`T!E` と同時に stdlib の一部を typed 化**: 却下 — ADR-0004 の決定範囲を
   侵食する。stdlib 側の再考は 0004 の Falsifier 経由でのみ。

## Consequences

**得るもの**: 精密化が署名 1 箇所の編集になる。エラー仕様の全 D 点が
「マーカー = 可謬の属性、E は String 既定・アトム 1 個で精密化、変換は必ず可視」の
3 句で説明できる。ADR-0002 の open question(fmt 正準形)がクローズする。

**払うもの**:
- 実装: パーサ 2 箇所(宣言戻り位置・fn 型 slot の `Bang` 後に型アトムを許す)、
  resolver 1 箇所(疑似ジェネリクス `!` の args.len()==2)、fmt の印字 + 正規化、
  spec/lang テストと CHEATSHEET / result-option-effect.md / llms.txt の更新。
  `!` マーカーの既存 matcher(module_inference / lower / check)は名前照合のみで
  arg 数を見ないため無変更。
- D3 の正規化は stdlib / spec 全域に一度だけ差分を出す(splice-context ソースは
  `--no-import-edit`、negative diagnostics fixture は対象外 — 既存の fmt 運用規約どおり)。
- 観測可能挙動は不変(表記のみ)— **契約台帳の更新は不要**。`T!E` の意味論は
  既存の explicit-Result 経路と同一であり、既存フィクスチャがそのまま証拠になる。
  新規に足すのは `spec/lang/` の表記テスト(パース・fmt・lift の同型性)。

## Falsifier — 何が起きたらこの決定を撤回するか

1. **Dojo の MSR 計測で、`T!E` 導入後に E 不一致 check エラー起因の修正失敗が
   有意に増えた場合**(綴りの安さが variant E を乱造させ、String 単一文化の希釈が
   hint で回収できないと実証されたら)— `T!E` を宣言位置から退役させ、精密層を
   明示 Result のみに戻す ADR で supersede。
2. **D3 の正規化が実害を出した場合**(戻り位置 Result の意図的使い分けが実在した
   証拠が出たら)— D3 のみ撤回し、正規化を `T!String` → `T!` に縮小する。
3. **ADR-0004 の Falsifier が先に発火した場合**(stdlib への構造導入が再検討される
   なら)— 本 ADR の D4-1 は 0004 の再裁定に従属する。層構造(D1)と表記(D2)は
   その場合も独立に生存する。

## References

- Swift SE-0413 — [Typed throws](https://github.com/swiftlang/swift-evolution/blob/main/proposals/0413-typed-throws.md)
  (「untyped `throws` remains the better default」「Resist the temptation…」の逐語出典)、
  [FullTypedThrows の未出荷](https://forums.swift.org/t/where-is-fulltypedthrows/72346)、
  [SE-0421 AsyncSequence.Failure](https://github.com/swiftlang/swift-evolution/blob/main/proposals/0421-generalize-async-sequence.md)
- Zig — [#2647 payload 恒久却下「Error codes are for control flow.」](https://github.com/ziglang/zig/issues/2647)、
  [#2971 推論 set の再帰(open)](https://github.com/ziglang/zig/issues/2971)、
  [diagnostics パターン実務報告](https://srcreigh.ca/posts/error-payloads-in-zig/)
- Rust — [thiserror](https://docs.rs/thiserror) / [anyhow](https://docs.rs/anyhow)(DL 実測 2026-08)、
  [provider API 放棄方向(#99301)](https://github.com/rust-lang/rust/issues/99301)
- MoonBit — [error-handling docs](https://docs.moonbitlang.com/en/latest/language/error-handling.html)、
  [2025-06-03](https://www.moonbitlang.com/updates/2025/06/03/index) /
  [2025-06-16 移行表](https://www.moonbitlang.com/updates/2025/06/16/index)、
  [beta-release(IDE 下線補償の逐語)](https://www.moonbitlang.com/blog/beta-release)、
  [core PR #3997(stringly vs 構造化の係争)](https://github.com/moonbitlang/core/pull/3997)、
  旧記法: [0722](https://discuss.moonbitlang.com/t/the-moonbit-update-0722/271) /
  [0729](https://discuss.moonbitlang.com/t/the-moonbit-update-0729/272)
- matklad — [The Second Great Error Model Convergence(2025-12-29)](https://matklad.github.io/2025/12/29/second-error-model-convergence.html)
- Inko — [0.11.0 撤回](https://inko-lang.org/news/inko-0-11-0-released/)
- Gleam — [`?` 演算子却下(2024-11)](https://github.com/gleam-lang/gleam/discussions/3908)、
  [snag](https://github.com/lpil/snag)
- Roc — [FAQ(tag union rationale)](https://www.roc-lang.org/faq)、
  [open/closed union 分析](https://gist.github.com/j-maas/ed3d2811d808d0fa1386478575df928d)
- Unison — [abilities error handling](https://www.unison-lang.org/docs/fundamentals/abilities/error-handling/)
- Koka — [row-polymorphic effects](https://arxiv.org/abs/1406.2061)、
  [2025 実務者報告(row 推論の難)](https://gfxmonk.net/2025/04/13/im-excited-about-koka.html)
- 内部 — ADR-0002/0003/0004/0006/0008/0009/0010、#1103、C-211、
  `crates/almide-syntax/src/parser/declarations.rs`(lift 分岐)、
  `crates/almide-frontend/src/canonicalize/resolve.rs`(`!` 脱糖)
