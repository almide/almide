<a id="adr-0008-propagation-is-always-explicit--auto--is-abolished--is-the-only-marker-result-is-must-use"></a>

# ADR-0008: 伝搬は常に明示 — auto-? を廃止し、`!` を唯一のマーカーとし、Result を must-use にする

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](0008-explicit-propagation-only.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](0008-explicit-propagation-only.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](0008-explicit-propagation-only.zh-CN.md)

> [日本語の正本](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0008-explicit-propagation-only.md)（固定コミット `35297a612fb29e3f78f6587aba253aa91d51f466`）の非規範的な読者用スナップショット。元の ADR が正本であり、変更しない。この写しは自動同期されず、チェッカーや生成器の入力ではない。以下の決定、日付、測定値、実装状態、主張は固定した当時の原文を伝えるもので、独立検証や現行リリースの保証ではない。コードブロックとそのコメントは原文のまま保持する。

> 原文の注意点：v0.53.6 の matrix、他言語の先例に関する断言、移行日程はこの ADR の歴史的記述であり、ここで独立に再現していない。E0xx は原文の仮記号であり、新たに割り当てた診断ではない。dojo MSR の反証条件は当時の基準として残し、新たな測定結果とはしない。

- **状態**: Accepted
- **日付**: 2026-08-05
- **決定範囲**: effect fn / 可謬 fn 内での失敗伝搬の機構 — 暗黙伝搬(auto-`?`)の存廃、
  伝搬マーカーの一本化、捨てられた Result の扱い
- **関連**: [ADR-0002](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0002-fallibility-effect-orthogonal.md)(D4: `!` = 伝搬専用。
  D3 の lift 対等性は本 ADR で自明化)、[ADR-0005](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0005-operators-desugar-to-stdlib.md)
  (`?`・`??` の糖衣定義 — 本 ADR で特例なしに合成可能になる)、
  [ADR-0006](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0006-fallibility-polymorphic-hofs.md)(明示 `!` の標準形)、#1103(Phase 1)
- **経緯**: 2026-08-05、エラー表面 matrix の B トラック 21 セル(auto-`?` の位置まだら)
  の深掘りから。○×裁定で「全明示 + must-use はエラー」に決定。

<a id="context--実測したまだらの全景v053621-セル"></a>

## 背景 — 実測した「まだら」の全景(v0.53.6、21 セル)

auto-`?`(effect fn 内で失敗しうる呼び出しを暗黙に伝搬する機構)は、
**効く位置と効かない位置がまだら**である:

| 暗黙伝搬が効く(5 位置) | 効かない — 明示 `!` が必要(5 位置) |
|---|---|
| 文の位置(`fail()` 単独) | 関数の引数(`double(get())` → E005) |
| 注釈なし let(`let r = int.parse(s)`) | パイプ(`get() \|> double` → E005) |
| if 条件 | record フィールド(→ E001) |
| match(値パターンのとき) | リスト要素(→ E001) |
| 文字列補間(警告ゼロで) | タプル成分(→ 番号なしエラー) |

さらに悪い実測が 2 つ:

```almide
// 1. パターンの形が実行時の生死を決める:
match get() { ok(v) => ..., err(e) => ... }   // Result のまま受ける(err は自分で捌く)
match get() { 42 => ..., _ => ... }           // auto-? が差し込まれ、err ならプログラム停止

// 2. 注釈の有無が挙動を反転させる:
let r = int.parse("zz")                        // 暗黙伝搬 — err で停止
let xs: List[Result[Int, String]] = [get()]    // 保持 — 停止しない
```

先例調査: **「一部の位置だけ暗黙」の言語は存在しない**。全員がどちらかの極にいる —
全明示(Rust `?` / Swift `try` / Zig `try`: 伝搬箇所に必ずマーカー、位置差ゼロ)か、
全暗黙(Java/Python の例外、OCaml 5 / Koka の効果: マーカーなし、全位置一様)。
全暗黙の言語には Result という**値**が存在しない(例外は値ではない)ことが本質で、
Result を値として持つ almide が全暗黙に寄ると「値として持つのか伝搬するのか」の
曖昧さが全位置に広がる。

<a id="decision"></a>

## 決定

<a id="d1-auto--は廃止--伝搬の綴りは--一本"></a>

### D1. auto-`?` は廃止 — 伝搬の綴りは `!` 一本

effect fn / 可謬 fn 内で、失敗しうる呼び出しは**どの位置でも Result 値を生む**。
伝搬したければ `!` を書く。位置による差は存在しない:

```almide
effect fn main() -> Unit = {
  let cfg  = fs.read_text(p)!            // 伝搬は常に明示
  let port = int.parse(s)!
  double(get()!)                          // 引数位置も同じ綴り
  println("v=${get()!}")                  // 補間も同じ綴り
  match get() { ok(v) => ..., err(e) => ... }   // Result は普通の値 — 普通に match
}
```

<a id="d2-捨てられた-result-は-check-エラーmust-use"></a>

### D2. 捨てられた Result は check エラー(must-use)

文の位置で Result を黙って捨てることはできない(auto-`?` 廃止で「黙って握り潰し」に
劣化する穴を塞ぐ)。逃げ道は 2 綴り、hint に明記:

```
error[E0xx]: この Result は使われていません — エラーが黙って捨てられます
  --> app.almd:3
   |   fail()
   = help: 伝搬するなら fail()!、意図的に捨てるなら let _ = fail()
```

`let _ =` は ADR-0004 D3-(b) の `(_)` と同じ「意図的破棄の明示」の系譜。

<a id="d3----の特例則は消滅しadr-0005-の定義だけが残る"></a>

### D3. `?` / `??` の特例則は消滅し、ADR-0005 の定義だけが残る

現状の「`e()?` は auto-? より先に効く」「`??` は auto-? を抑止する」という
暗黙機構との相互作用ルールは、機構ごと消える。effect 呼び出しは常に Result 値なので:

```almide
e()?          // ≡ result.to_option(e()) — ADR-0005 D1 そのまま
e() ?? fb     // ≡ unwrap_or_else — 同上。特例なしの普通の値演算
```

<a id="d4-移行は-e040-窓方式1-リリースの警告期間"></a>

### D4. 移行は E040 窓方式(1 リリースの警告期間)

1. リリース N: 暗黙伝搬が挿入されている全箇所(検出対象 = Context の 5 位置クラス、
   matrix の probe が再現テスト)に deprecation 警告 + 正確な `!` 挿入 hint
2. リリース N+1: 切替(警告 → エラー)+ D2 の must-use 有効化
3. リポジトリ内の全 .almd(stdlib・tools・spec)を機械移行。下流 6 リポジトリの
   sweep も同時(既知の手順)
4. CLAUDE.md の codegen テスト規則(「effect fn 内の fs.read_text() は手書き ?
   なしでコンパイルできること」)は切替時に本 ADR 準拠へ書き換える

<a id="d5-adr-0002-d3lift-対等性は自明化"></a>

### D5. ADR-0002 D3(lift 対等性)は自明化

`-> T!`(pure 可謬)と effect fn の伝搬エルゴノミクスは「どちらも明示 `!`」で
完全に一致する。#1103 の D3 パリティ受け入れ条件は本 ADR 準拠で読み替える。

<a id="rationale"></a>

## 根拠

- **まだら 21 セルが構造ごと消滅する**: 「パターンの形で生死が変わる」
  「注釈で挙動が反転する」は、修理ではなく機構の廃止でしか消えない
- **伝搬の綴りが 1 つになる**: ADR-0002 D4 が `!` を伝搬専用と定めた以上、
  第二の伝搬機構(auto-`?`)は綴り一本化原理(ADR-0004/0005)への違反だった
- **事前分布**: Rust `?` / Swift `try` / Zig `try` — 全明示側が圧倒的。
  Swift は throwing 呼び出し全部に `try` を書かせて成立している
- **移行が機械的**: 警告 hint 通りに `!` を貼るだけ — LLM の最得意クラスの修正
- **Result-as-value の一貫性**: `?`・`??`・match・`|>` が特例なしに合成できる
  (ADR-0005 の定義表がそのまま全位置で真になる)

<a id="alternatives--検討して却下した案"></a>

## 代替案 — 検討して却下した案

1. **全暗黙へ完成させる**(例外言語型): Result が値として在る言語では
   「値として持つ/伝搬する」の曖昧さが全位置に拡大し、注釈依存の挙動が仕様の
   中心になる。**却下**。
2. **現状の位置リストを仕様化して驚きセルだけ修理**: パターン形状依存の生死は
   文書化で良性にならない。二機構(auto-? と `!`)の併存も残る。**却下**。
3. **文の位置だけ auto-? を残す**(最小暗黙): 機構が 2 つ残る点で同罪。
   文の位置は D2(must-use)+ `!` で 1 文字の差しかない。**却下**。
4. **must-use を警告に留める**(Rust 型): エラー握り潰しは silent-loss 族の
   最後の穴であり、逃げ道(`let _ =`)が 1 綴りで在る以上、エラーにしても
   書き手を困らせない。**却下**(Falsifier 3 で再考条項あり)。

<a id="consequences"></a>

## 帰結

- 既存の全 effect fn コード(stdlib・tools・spec・下流)に `!` が機械追加される —
  大きいが単純な diff。警告期間があるため一斉切替ではない
- 冗長さは増える(Swift の `try` 相当)。1 呼び出し 1 文字
- B トラック 21 セルは「effect/可謬呼び出しは常に Result 値」という 1 行の仕様に
  畳まれ、`e()?` 迎撃・`??` 抑止などの未文書特例はすべて ADR-0005 定義の
  通常合成として文書化される
- #1103(Phase 1)の設計依存が解消され、完全に着手可能になる

<a id="falsifier"></a>

## 反証条件

1. **移行後、dojo の MSR が有意に劣化した場合**(`!` の増加がノイズとして効く)—
   最小暗黙(Alternatives 3)を再評価する。
2. **警告期間で、hint が機械的に直せない曖昧サイトが大量に見つかった場合** —
   切替を延期し、曖昧クラスを個別設計してから再開する。
3. **must-use エラーの偽陽性**(正当な fire-and-forget が実在する)**が計測された
   場合** — D2 を警告に軟化する。

<a id="references"></a>

## 参照

- Swift — [エラー処理（`try` の全呼び出し明示）](https://docs.swift.org/swift-book/documentation/the-swift-programming-language/errorhandling/)、SE-0413
- Rust — [`?` 演算子](https://doc.rust-lang.org/reference/expressions/operator-expr.html#the-question-mark-operator)、
  [`#[must_use]` / unused_must_use](https://doc.rust-lang.org/std/result/#results-must-be-used)
- Zig — [try](https://ziglang.org/documentation/master/#try)
- 内部: 2026-08-05 matrix の `autoq/*`・`qop/effect-*`・`qq/effect-*` 21 セル
  (probe 保存済み)、ADR-0002 / 0004 / 0005 / 0006
