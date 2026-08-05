# ADR-0004: Error branchability — evidence-gated stages from String to tagged errors

- **Status**: Proposed
- **Date**: 2026-08-05
- **決定範囲**: エラー値の表現ドクトリン — String 収束をどこまで維持し、
  「内容で分岐できる構造」をいつ・どの形で足すか。および語彙・契約面の設計基準
- **関連**: [ADR-0002](./0002-fallibility-effect-orthogonal.md)(可謬性の軸)、
  [ADR-0003](./0003-error-type-conversion-at-propagation.md)(無損失原理 — 本 ADR の
  Stage 2 の形を選択する)、C-029 系(メッセージ byte 契約)
- **経緯**: 2026-08-05 のエラー表面 matrix 討議の続き。「String 収束は正しいか」を
  他言語の歴史と照合した結果、収束自体ではなく**収束から出る合図と出口の欠如**が
  問題であると同定した。

## Context — 問題の定式化

almide のエラーは String に収束している: stdlib の E は String 単一文化、
`-> T!` は E=String(ADR-0002)、main の失敗チャネルは String。これは報告
(人間/LLM が読む)には十分だが、**分岐**(プログラムがエラー内容で挙動を変える)
には構造がない。現状の分岐手段は `string.contains(e, "No such file")` 型の
string-match のみで、これは Go 〜1.12 が10年かけて有害と実証したパターンである。

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

現時点の緩和要因:

1. `??` が「内容を見ない fallback」を第一級で提供し、分岐需要の大半を吸収している
2. メッセージが契約(C-029 系)で byte 固定されており、string-match が他言語より
   「壊れにくい」— ただしユーザーが match し始めた瞬間、メッセージ改善が永久に
   不可能になる(Hyrum 凍結)という両刃
3. 分岐が正当に必要なドメイン(fs の not_found / permission 区別等)は既に見えている

評価軸(mission = MSR 最大化から導出):

| 軸 | 内容 |
|---|---|
| W | 記述儀式の少なさ(LLM の失敗表面) |
| B | 分岐可能性(string-match なしで内容分岐できるか) |
| X | 網羅性検査(E に新ケースが増えたとき既存分岐箇所が check で炙り出されるか) |
| R | 報告品質(context チェーン) |
| S | 仕様サイズ(型機構の複雑さ・推論の予測可能性) |
| P | **事前学習事前確率** — 構文・語彙がモデルの事前分布で高確率か。2026 の言語だけが使える基準 |

### 各言語が証明した定理

| 言語 | 証明したこと |
|---|---|
| Go 〜1.12 → 1.13 | String 収束は規模が出ると必ず string-match 分岐に至る。後付け修復(`%w`/`errors.As`)は可能だが不格好 |
| Rust(thiserror/anyhow 分裂) | 人間ですら per-type 儀式を拒否した。アプリ側は型消去へ収束。anyhow の本体は型消去でなく context チェーン |
| Zig(error sets) | 儀式ゼロ × match 可能は両立する: タグ宣言不要、set 推論、上位集合へ暗黙 coerce(無損失) |
| Swift(SE-0413) | 業界の到達点は「消去がデフォルト、型付きは opt-in の精密化」という二層 |
| Roc / OCaml 多相バリアント | 構造的タグ union は宣言なしで自動合成できる。対価は推論エラーの難解さ(P が低い) |
| Erlang/Elixir | 裸のタグで分岐需要の大半は足りる。型なしでも規約が機能する |
| Gleam(snag) | 型クラスなしの小言語では「String + cause チェーン」が現実的な中間解 |

W と X のトレードオフを解いたのは**集合意味論のタグ**(Zig/Erlang/Roc 系)だけであり、
nominal enum(Rust 系)は解いていない。

## Decision

**String は既定のエラー通貨として維持する。分岐可能な構造は「証拠ゲート」を通過した
段階でのみ追加し、その形は集合意味論のタグ(nominal enum ではない)とする。
語彙は既存の標準タクソノミーから借り、契約面はメッセージ byte からタグへ移す方向とする。**

### D1. ドクトリン行(Stage 0・即時)

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

(stdlib 由来のエラー(E=String)は自分では variant 化できない — その圧力点を
受けるのが Stage 1(D4)である。)

### D2. context チェーンの正準形(Stage 0・即時)

`result.context(r, "loading config")` を stdlib に置く
(`result.map_err((e) => "loading config: ${e}")` の命名にすぎない糖衣)。
anyhow が実証した「アプリのエラーに本当に必要なのは型でなく文脈の連鎖」を、
型システム変更ゼロで取り込む。

```almide
// 今も書ける正準形:
let cfg = fs.read_text(path) |> result.map_err((e) => "loading config: ${e}")!

// D2 の糖衣(意味は上と同一):
let cfg = fs.read_text(path) |> result.context("loading config")!

// 呼び出しが連なると anyhow 流の文脈チェーンになる:
//   Error: starting server: loading config: No such file or directory
```

### D3. string-match 分岐への lint(Stage 0・即時)

err 値由来の String への `string.contains` / 等値比較による分岐に警告を出す。
Go の失敗パターンを診断で先回りする。

```
warning[W0xx]: エラー文字列の本文で分岐しています
  --> app.almd:12
   |   if string.contains(e, "No such file") then ok(default_config())
   = help: エラーの内容で分岐するなら variant E を定義してください(ADR-0004 D1)。
           stdlib のエラーはタグ判定 API を待つか、?? での fallback を検討
```

### D4. Stage 1 — 契約付きタグ入り String(条件付き)

**発動条件**: dojo の計測で「エラー内容分岐」の需要が実証されること
(タスクが要求する頻度、または lint 警告の発生率)。

内容: stdlib のエラーメッセージを「`タグ: 詳細文`」形式に統一する。タグは
errno / HTTP status 系から借りた**閉じた小タクソノミー**(`not_found`,
`permission_denied`, `timeout`, `parse`, …)として registry で管理し、
契約台帳に載せる。判定 API(形状は実装時に設計)を stdlib に置き、
生の string-match は D3 の lint が塞ぐ。**契約面はタグに移り、詳細文は
byte 契約から解放される**(Hyrum 凍結の解消)。

```almide
// スケッチ — API 形状は実装時に設計
fs.read_text("config.toml")
// => err("not_found: config.toml")
//         ^^^^^^^^^ タグ = 契約固定・registry 管理    ^^^ 詳細文 = 自由に改善可

fn load_config(path: String) -> Result[String, String] =
  match fs.read_text(path) {
    ok(t)  => ok(t),
    err(e) =>
      if error.is(e, "not_found")            // ✓ 契約されたタグへの判定
      then ok(default_config())
      else err(e),
  }
```

```toml
# contracts.toml の契約面の移行(概念図)
# 旧: fs.read_text の err は byte 列 "No such file or directory (os error 2)" に固定
# 新: fs.read_text の err は tag = "not_found" に固定。詳細文は契約外
```

### D5. Stage 2 — error set 型(条件付き・Zig 系統)

**発動条件**: grammar-lab の A/B で「エラー分岐箇所の網羅性検査(X)が MSR を
有意に改善する」ことが実証されること。

内容: `#not_found` 型の第一級タグと集合意味論(set は推論、上位集合への拡大は
無損失なので ADR-0003 D3 により暗黙で合法)。X を機械化する唯一の案だが、
S のコスト(新型機構、`-> T!` との整合、wasm byte 一致)が大きいため、
証拠なしには動かない。**Stage 1 の実測が X の不在を問題として示さなければ、
Stage 1 が終点である。**

```almide
// スケッチ — 構文は実装時に設計。タグは宣言不要、集合は推論
fn load(path: String) -> Result[Config, #not_found | #parse] = ...

match load(path) {
  ok(c)           => c,
  err(#not_found) => default_config(),
  err(#parse)     => process.exit(1),
  // ← load に #permission が増えると、この match が非網羅として check エラーに
  //    なる = 修正必要箇所を機械が炙り出す(X)。Stage 1 ではこれができない
}

// 上位集合への拡大は無損失 → 暗黙で合成(ADR-0003 D3 に整合):
fn boot(p: String) -> Result[Config, #not_found | #parse | #permission] =
  load(p)                          // ✓ map_err 儀式なし
// 対して nominal enum(Rust 型)は ConfigError → AppError が損失ありの変換に
// なるため、0003 の下では永遠に明示 map_err を要する — 集合タグだけが両立点
```

### D6. 語彙の設計基準は事前学習事前確率(P)

分岐タグの語彙は、事前学習コーパスに大量に存在する標準タクソノミー
(errno・HTTP status・Go sentinel)から借りる。bespoke な階層命名
(`ConfigLoadFailureKind` 型)は P が低く採らない。**構文・語彙の選択は
「モデルの事前分布で高確率な形か」で評価する** — 本基準は本 ADR 以降の
表面設計全般に適用する。

```
P が高い(事前学習コーパスに大量に存在し、LLM が初見で正しく書ける):
  not_found  permission_denied  timeout  parse  invalid_input  conflict
P が低い(このリポジトリでしか通用しない):
  ConfigLoadFailureKind::MissingKeyInSection  E_CFG_04  AlmideIoFault
```

## Rationale

### ADR-0003 の無損失原理が Stage 2 の形を選択している

「暗黙変換は無損失に限る」(0003 D3)を将来のエラー表現に適用すると:
nominal enum の合成(`ConfigError` → `AppError`)は損失ありで永遠に map_err
儀式を要するが、**タグ集合の上位集合への拡大は無損失であり暗黙合成が原理と
整合する**。儀式ゼロで合成でき、かつ 0003 を破らない表現は集合意味論のタグ
だけである。Zig が同じ結論に先着していたのは偶然ではない。

### 契約資産の反転

byte 固定されたメッセージは再現性の資産だが、分岐に使われた瞬間 Hyrum 凍結に
変わる。機械が読む部分(タグ)を契約で凍結し、人間が読む部分(詳細文)を
自由化するのが正しい分割であり、Go が `errors.Is` で後付けしたものを
almide は契約台帳ネイティブで最初から持てる。

### 段階ゲートは repo の流儀そのもの

falsifier・ratchet・dojo 計測で言語を進める文化に対し、「理論的に美しいから
error set を今入れる」は整合しない。分岐需要(D4 条件)と網羅性の MSR 効果
(D5 条件)はどちらも計測可能であり、計測が裁く。

## Alternatives — 検討して却下した案

1. **stdlib を nominal typed E 化(Rust 型)**: Rust の歴史(アプリ側が儀式を
   拒否し anyhow へ逃げた)と ADR-0003(全境界で map_err 儀式が発生)が二重に
   反証する。**却下**。
2. **Roc / OCaml 流の構造的タグ union を直接導入**: 理論的には W・B・X を同時に
   解くが、row 多相の推論エラーは out-of-distribution(P が低い)であり、
   S のコストも最大。**却下**(Stage 2 のタグ集合は同系だが閉じた語彙 + 単純な
   集合包含に限定する点で異なる)。
3. **証拠なしで Stage 2 まで一気に実装**: 計測文化に反する。分岐需要そのものが
   `??` で吸収されて出ない可能性が現実にある。**却下**。
4. **何もしない(現状維持)**: fs の not_found 分岐という圧力点が既に見えており、
   放置は string-match の本番流入(Go 〜1.12 の再演)を待つことになる。
   最低限 D1〜D3 は先回りとして必要。**却下**。
5. **メッセージ byte 契約の維持(タグへ移さない)**: ユーザー分岐が始まった後では
   詳細文の改善が破壊的変更になり、診断品質(mission の中核)を凍結する。**却下**。

## Consequences

- 得るもの: string-match 流入の先回り(D1/D3)、anyhow 相当の報告品質(D2)、
  分岐構造を「需要の証拠が出た分だけ」払う段階設計、語彙・契約の将来互換
  (Stage 1 のタグは Stage 2 の `#タグ` にそのまま持ち上がる)
- 払うもの: Stage 0 の実装(糖衣 + lint)、dojo 側に計測項目の追加、
  Stage 1 発動時は stdlib メッセージの一斉改稿 + 契約移行という大きめの PR
- 明示的な非目標: ユーザー定義の開いたタグ空間(Stage 1 は stdlib 管理の
  閉じた語彙)。ユーザーの分岐需要は variant E ルート(0003 D1)が受ける

## Falsifier

1. **D3 の lint と `??` があっても string-match 分岐が書かれ続け、かつ Stage 1 の
   タグ判定 API が使われない**と dojo で計測された場合 — 段階モデル自体が誤り。
   表現(タグ)でなく別の何かが問題であり、本 ADR を撤回して原因分析からやり直す。
2. **タグ語彙が閉じない**(stdlib の実エラーを覆うのにタクソノミーが際限なく
   増殖する)ことが Stage 1 の設計中に判明した場合 — D4 の前提が崩れる。
3. **grammar-lab の A/B で網羅性検査が MSR を改善しない**と出た場合 — D5 は
   永久に閉じ、Stage 1 を終点として本 ADR を改訂する(これはゲートの正常動作
   であり撤回ではないが、結果を ADR に記録する)。
4. **P 基準(D6)に従った語彙選択が、dojo で bespoke 命名と差がない**と出た場合 —
   D6 を設計基準から外す。

## References

- Go — [Working with Errors in Go 1.13](https://go.dev/blog/go1.13-errors)、
  [Errors are values](https://go.dev/blog/errors-are-values)(string-match 問題と
  その修復の一次記録)
- Rust — [anyhow](https://docs.rs/anyhow)(context チェーン)、
  [thiserror](https://docs.rs/thiserror)(library 側の typed 慣行)
- Zig — [Errors](https://ziglang.org/documentation/master/#Errors)・
  [The Global Error Set](https://ziglang.org/documentation/master/#The-Global-Error-Set)
  (儀式ゼロ × match 可能 × 無損失 coercion)
- Swift Evolution SE-0413 — [Typed throws](https://github.com/swiftlang/swift-evolution/blob/main/proposals/0413-typed-throws.md)
  (消去デフォルト + opt-in 精密化の二層)
- Roc — [roc-lang.org](https://www.roc-lang.org)(構造的タグ union)、
  OCaml manual — polymorphic variants(同系の先行技術と推論エラー問題)
- Gleam — [snag](https://hexdocs.pm/snag/)(String + cause チェーンの中間解)
- 内部: ADR-0002 / ADR-0003(無損失原理)、C-029 系(byte 契約 → タグ契約への
  移行対象)、2026-08-05 matrix(`string.contains` 分岐の現状証拠)
