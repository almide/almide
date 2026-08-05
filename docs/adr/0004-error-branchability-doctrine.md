# ADR-0004: Error branchability — String stays terminal; doctrine, lint, and context instead of tags

- **Status**: Accepted
- **Date**: 2026-08-05
- **決定範囲**: エラー値の表現ドクトリン — String 収束を維持するか、
  「内容で分岐できる構造」(タグ・error set)を足すか
- **関連**: [ADR-0002](./0002-fallibility-effect-orthogonal.md)(可謬性の軸)、
  [ADR-0003](./0003-error-type-conversion-at-propagation.md)(無損失原理・variant E の公式ルート)、
  C-029 系(メッセージ byte 契約 — 本 ADR により全文固定のまま維持)
- **経緯**: 2026-08-05 のエラー表面 matrix 討議の続き。一問一具体物の○×で批准。
  ドクトリン・lint・context の 3 点を採択し、タグ入り String(バーコード)と
  error set 型は**条件付き予約案まで含めて却下**した。初稿にあった段階ゲート構想
  (Stage 1 タグ → Stage 2 error set)は Alternatives に却下理由ごと保存する。

## Context — 問題の定式化

almide のエラーは String に収束している: stdlib の E は String 単一文化、
`-> T!` は E=String(ADR-0002)、main の失敗チャネルは String。これは報告
(人間/LLM が読む)には十分だが、**分岐**(プログラムがエラー内容で挙動を変える)
には構造がない。現状の分岐手段は string-match のみで、これは Go 〜1.12 が
10年かけて有害と実証したパターンである。

```almide
// 今日「なければデフォルト、それ以外は失敗」を書くとこうなる — Go 〜1.12 の再演
fn load_config(path: String) -> Result[String, String] =
  match fs.read_text(path) {
    ok(t)  => ok(t),
    err(e) =>
      if string.contains(e, "No such file")   // ✗ メッセージ本文に依存した分岐
      then ok(default_config())
      else err(e),
  }
```

緩和要因:

1. `??` が「内容を見ない fallback」を第一級で提供し、分岐需要の大半を吸収している
2. 事前述語(`fs.exists` 等)で分岐の多くはエラー経路に入る前に判定できる
3. 自作ドメインでは variant E(ADR-0003 D1 の公式ルート)で構造分岐できる

### 各言語が証明した定理

| 言語 | 証明したこと |
|---|---|
| Go 〜1.12 → 1.13 | String 収束は規模が出ると必ず string-match 分岐に至る。後付け修復(`%w`/`errors.As`)は可能だが不格好 |
| Rust(thiserror/anyhow 分裂) | 人間ですら per-type 儀式を拒否した。アプリ側は型消去へ収束。anyhow の本体は型消去でなく context チェーン |
| Zig(error sets) | 儀式ゼロ × match 可能は両立する: タグ宣言不要、set 推論、上位集合へ暗黙 coerce(無損失) |
| Swift(SE-0413) | 業界の到達点は「消去がデフォルト、型付きは opt-in の精密化」という二層 |
| Roc / OCaml 多相バリアント | 構造的タグ union は宣言なしで自動合成できる。対価は推論エラーの難解さ |
| Erlang/Elixir | 裸のタグで分岐需要の大半は足りる。型なしでも規約が機能する |
| Gleam(snag) | 型クラスなしの小言語では「String + cause チェーン」が現実的な中間解 |

## Decision

**String を既定にして終着点とする。エラー値に機械可読構造(先頭タグ・error set)は
導入しない。string-match 分岐はドクトリンと lint で封じ、報告品質は context
チェーンで確保する。stdlib 由来エラーの内容分岐は提供しない — `??` による fallback、
事前述語、および自作ドメインの variant E で賄う。**

### D1. ドクトリン行

spec / CHEATSHEET に一行で明文化する:
**「エラーの*内容*で分岐したくなったら、それが variant E(ADR-0003 D1 の公式ルート)
に切り替える合図である。エラー文字列への string-match で分岐してはならない。」**

```almide
// ✓ 自分の定義域では E を variant で設計する(0003 D1 の公式ルート)
type ConfigError = | Missing(String) | BadValue(String)

fn get_port(cfg: Map[String, String]) -> Result[Int, ConfigError] = ...

match get_port(cfg) {
  ok(p)            => p,
  err(Missing(_))  => 8080,                 // ← 構造で分岐。メッセージ変更に不感
  err(BadValue(m)) => process.exit(1),
}
```

### D2. context チェーンの正準形

`result.context(r, "loading config")` を stdlib に置く
(`result.map_err((e) => "loading config: ${e}")` の命名にすぎない糖衣)。
anyhow が実証した「アプリのエラーに本当に必要なのは型でなく文脈の連鎖」を、
型システム変更ゼロで取り込む。実装: #1104。

```almide
// 今も書ける正準形:
let cfg = fs.read_text(path) |> result.map_err((e) => "loading config: ${e}")!

// D2 の糖衣(意味は上と同一):
let cfg = fs.read_text(path) |> result.context("loading config")!

// 呼び出しが連なると anyhow 流の文脈チェーンになる:
//   Error: starting server: loading config: No such file or directory
```

### D3. string-match 分岐への lint

err 値由来の String への `string.contains` / 等値比較による分岐に警告を出す。
Go の失敗パターンを診断で先回りする。実装: #1105。

```
warning[W0xx]: エラー文字列の本文で分岐しています
  --> app.almd:12
   |   if string.contains(e, "No such file") then ok(default_config())
   = help: エラーの内容で分岐するなら variant E を定義してください(ADR-0004 D1)。
           内容を見ない fallback なら ?? を検討
```

### D4. stdlib エラーの内容分岐は提供しない(明示的な非目標)

stdlib 由来のエラー(E=String)を種類で分岐する公式手段は置かない。公式の手段は:

1. **`??` fallback** — 種類を問わず既定値で継続(需要の大半)
2. **事前述語** — `fs.exists(path)` 等でエラー経路に入る前に判定
3. **variant E** — 分岐が本質的なドメインは自作の E で設計(0003 D1)

分岐需要が本物である場所は、エラー構造の追加ではなく **API 形状**で個別に受ける:
「不在」を成功チャネルの Option で返す変種(`map.get` が err でなく Option を返す
のと同じ原理)。初適用として fs の content-reader family
(`read_text_if_exists` 等 4 セル)を批准済み — #1106。

```almide
let cfg = fs.read_text_if_exists(path)! ?? default_config()
//        none = 不在(正常系) / err = 権限・IO 等の本物の失敗
```

## Rationale

### 分岐需要の実態が「構造」を正当化しなかった

`??` と事前述語が吸収した後に残る「エラー種で分岐したい」場面は、fs の
not_found 程度に局在しており、そのために全 stdlib メッセージの改稿(タグ化)や
型システム拡張(error set)を払う比率が合わない。局在した需要は API 形状
(D4 の Option 変種)という既存型で受けられる。

### 即時 3 点(D1〜D3)は安くて可逆

ドクトリン行は文書のみ、context は既存 map_err の命名、lint は警告のみで
既存コードを壊さない。string-match の本番流入への先回りとして、構造の導入と
独立に今払う価値がある。

### 契約の単純さを守る

タグ化(却下案 A)は契約面を「全文 byte」から「タグのみ」へ移す再設計を伴い、
再現性保証の複雑化を招く。全文 byte 固定は Hyrum 凍結の裏面を持つが、
「メッセージで分岐するな」(D1)が守られる限り凍結の実害は診断改善の摩擦に
限られ、それは契約更新の通常手続きで払える。

## Alternatives — 検討して却下した案

1. **タグ入り String(バーコード)**: stdlib エラーを「`タグ: 詳細文`」に統一し
   `error.is(e, "not_found")` で判定、契約はタグのみ固定(Node の `err.code === 'ENOENT'`、
   C の errno と同系)。**却下** — 分岐需要の局在(Rationale 1)に対して全 stdlib
   改稿 + 契約再設計が過大。
   ```almide
   fs.read_text("x.toml")        // => err("not_found: x.toml")(却下案)
   ```
2. **error set 型(Zig 系統)**: `#not_found` の第一級タグ + 集合推論 + 網羅性検査。
   ADR-0003 の無損失原理と整合する唯一の暗黙合成可能な構造であり、理論的最有力
   だったが、**「A/B で効果実証されたら」の条件付き予約案まで含めて却下** —
   型システム拡張(S コスト)を正当化する見込みが現在の需要にない。将来復活させる
   場合は本節の分析(nominal enum ではなく集合意味論を選ぶこと)を出発点にする。
   ```almide
   fn load(p: String) -> Result[Config, #not_found | #parse] = ...   // 却下案
   ```
3. **stdlib を nominal typed E 化(Rust 型)**: Rust の歴史(アプリ側が儀式を拒否し
   anyhow へ逃げた)と ADR-0003(全境界で map_err 儀式)が二重に反証。**却下**。
4. **Roc / OCaml 流の構造的タグ union**: row 多相の推論エラーは難解で S コスト最大。**却下**。
5. **タグ語彙を標準タクソノミー(errno/HTTP)から借りる規則**: 語彙は都度の設計判断
   とし規則では縛らない(タグ自体が却下された今、規則の対象も存在しない)。**却下**。
6. **何もしない(D1〜D3 も見送り)**: string-match の本番流入(Go 〜1.12 の再演)を
   無防備に待つことになる。**却下**。

## Consequences

- 得るもの: エラー仕様の終着点が確定する(String + variant E、構造追加なし)。
  仕様・契約・実装のどれも新機構を持たない。string-match 流入は D1/D3 が封じる
- 払うもの: stdlib エラーの種類分岐は公式に不可能のまま。fs 系で本物の需要が
  出た場合は D4 の API 形状(Option 変種)で個別に受ける
- 実装は #1104(context)と #1105(lint + ドクトリン行)のみ。契約・型システム・
  stdlib メッセージは無変更

## Falsifier

1. **D3 の lint と `??` があっても string-match 分岐が書かれ続ける**と dojo で
   計測された場合 — 「分岐需要は吸収できる」という本 ADR の前提が誤り。
   Alternatives 1(タグ)/ 2(error set)の再評価から議論をやり直す。
2. **D4 の Option 変種 API が増殖し始めた場合**(`*_if_exists` 系 family が
   fs 以外の 3 モジュール以上に波及等)— 需要が局在でなく一般だった証拠であり、
   構造導入(Alternatives 2)を再検討する。
3. **D2 の context 連鎖が実コーパスでほぼ使われない**と計測された場合 — 報告品質の
   問題認識が誤りだった証拠として、D2 を deprecated にする改訂を行う。

## References

- Go — [Working with Errors in Go 1.13](https://go.dev/blog/go1.13-errors)、
  [Errors are values](https://go.dev/blog/errors-are-values)
- Rust — [anyhow](https://docs.rs/anyhow)、[thiserror](https://docs.rs/thiserror)、
  [`io::ErrorKind`](https://doc.rust-lang.org/std/io/enum.ErrorKind.html)(却下案 1 の同系)
- Node.js — [`err.code`(errno)](https://nodejs.org/api/errors.html#errorcode)(却下案 1 の同系)
- Zig — [Errors](https://ziglang.org/documentation/master/#Errors)(却下案 2 の同系)
- Swift Evolution SE-0413 — [Typed throws](https://github.com/swiftlang/swift-evolution/blob/main/proposals/0413-typed-throws.md)
- Gleam — [snag](https://hexdocs.pm/snag/)(String + cause チェーンの中間解)
- 内部: ADR-0002 / ADR-0003、C-029 系、2026-08-05 matrix、#1104、#1105
