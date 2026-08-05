# ADR-0004: Error branchability — evidence-gated stages from String to tagged errors

- **Status**: Accepted(部分批准 — Stage 1 の 3 論点は **Open questions** として意図的に未決。
  追補または ADR-0005 で決める)
- **Date**: 2026-08-05
- **決定範囲**: エラー値の表現ドクトリン — String 収束をどこまで維持し、
  「内容で分岐できる構造」をいつ・どの形で足すか
- **関連**: [ADR-0002](./0002-fallibility-effect-orthogonal.md)(可謬性の軸)、
  [ADR-0003](./0003-error-type-conversion-at-propagation.md)(無損失原理 — 本 ADR の
  Stage 2 の形を選択する)、C-029 系(メッセージ byte 契約)
- **経緯**: 2026-08-05 のエラー表面 matrix 討議の続き。「String 収束は正しいか」を
  他言語の歴史と照合した結果、収束自体ではなく**収束から出る合図と出口の欠如**が
  問題であると同定した。同日、一問一具体物の○×で D1/D2/D3/D4 を批准、
  語彙規則案を却下、Stage 1 の 3 論点を保留とした。

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

W と X のトレードオフを解いたのは**集合意味論のタグ**(Zig/Erlang/Roc 系)だけであり、
nominal enum(Rust 系)は解いていない。

## Decision

**String は既定のエラー通貨として維持する。string-match 分岐をドクトリンと lint で
封じ、context チェーンを正準化する(即時)。error set 型への進路は
「網羅性検査の MSR 効果が A/B で実証されたら」の条件付きで予約する。
タグ入り String(Stage 1)の採否・契約移行・タイミングは未決とし Open questions に置く。**

### D1. ドクトリン行(即時)

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

(stdlib 由来のエラー(E=String)は自分では variant 化できない — その圧力点の
扱いが Open questions 1 である。)

### D2. context チェーンの正準形(即時)

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

### D3. string-match 分岐への lint(即時)

err 値由来の String への `string.contains` / 等値比較による分岐に警告を出す。
Go の失敗パターンを診断で先回りする。

```
warning[W0xx]: エラー文字列の本文で分岐しています
  --> app.almd:12
   |   if string.contains(e, "No such file") then ok(default_config())
   = help: エラーの内容で分岐するなら variant E を定義してください(ADR-0004 D1)。
           内容を見ない fallback なら ?? を検討
```

### D4. error set 型への進路を条件付きで予約(Zig 系統)

**発動条件**: grammar-lab の A/B で「エラー分岐箇所の網羅性検査(X)が MSR を
有意に改善する」ことが実証されること。この予約は Stage 1(Open questions 1)の
採否と独立である。

内容: `#not_found` 型の第一級タグと集合意味論(set は推論、上位集合への拡大は
無損失なので ADR-0003 D3 により暗黙で合法)。X を機械化する唯一の案だが、
S のコスト(新型機構、`-> T!` との整合、wasm byte 一致)が大きいため、
証拠なしには動かない。

```almide
// スケッチ — 構文は実装時に設計。タグは宣言不要、集合は推論
fn load(path: String) -> Result[Config, #not_found | #parse] = ...

match load(path) {
  ok(c)           => c,
  err(#not_found) => default_config(),
  err(#parse)     => process.exit(1),
  // ← load に #permission が増えると、この match が非網羅として check エラーに
  //    なる = 修正必要箇所を機械が炙り出す(X)
}

// 上位集合への拡大は無損失 → 暗黙で合成(ADR-0003 D3 に整合):
fn boot(p: String) -> Result[Config, #not_found | #parse | #permission] =
  load(p)                          // ✓ map_err 儀式なし
// 対して nominal enum(Rust 型)は ConfigError → AppError が損失ありの変換に
// なるため、0003 の下では永遠に明示 map_err を要する — 集合タグだけが両立点
```

### D5. タグ語彙は規則で縛らない

タグの命名は都度の設計判断とする(errno/HTTP 系標準語彙への強制は**しない** —
Alternatives 6 参照)。stdlib が自身のエラー語彙を慣行として統一することは妨げない。

## Open questions — 意図的に未決(追補または ADR-0005 で決める)

批准時に「これから決めたい」として保留された 3 論点。討議素材としてスケッチを残す:

**OQ1. タグ入り String の採否** — stdlib エラーを「`タグ: 詳細文`」形式に統一し、
判定 API を置くか:

```almide
fs.read_text("x.toml")        // => err("not_found: x.toml")

err(e) => if error.is(e, "not_found") then ok(default_config()) else err(e)
```

**OQ2. 契約面の移行** — OQ1 をやる場合、byte 固定をタグだけにして詳細文を
自由化するか(Hyrum 凍結の解消)。現在はメッセージ全文が byte 固定:

```toml
# contracts.toml の契約面の移行(概念図)
# 旧: fs.read_text の err は byte 列 "No such file or directory (os error 2)" に固定
# 新: fs.read_text の err は tag = "not_found" に固定。詳細文は契約外
```

**OQ3. 着手タイミング** — OQ1〜2 は「dojo で分岐需要が実証されてから」の
証拠ゲートにするか、圧力点(fs の not_found)が既知である以上すぐ着手するか。

## Rationale

### ADR-0003 の無損失原理が D4 の形を選択している

「暗黙変換は無損失に限る」(0003 D3)を将来のエラー表現に適用すると:
nominal enum の合成(`ConfigError` → `AppError`)は損失ありで永遠に map_err
儀式を要するが、**タグ集合の上位集合への拡大は無損失であり暗黙合成が原理と
整合する**。儀式ゼロで合成でき、かつ 0003 を破らない表現は集合意味論のタグ
だけである。Zig が同じ結論に先着していたのは偶然ではない。

### 即時 3 点(D1〜D3)は安くて可逆

ドクトリン行は文書のみ、context は既存 map_err の命名、lint は警告のみで
既存コードを壊さない。string-match の本番流入(Go 〜1.12 の再演)への先回りと
して、Stage 1 以降の判断と独立に今払う価値がある。

### 段階ゲートは repo の流儀そのもの

falsifier・ratchet・dojo 計測で言語を進める文化に対し、「理論的に美しいから
error set を今入れる」は整合しない。網羅性の MSR 効果(D4 条件)は計測可能であり、
計測が裁く。

## Alternatives — 検討して却下した案

1. **stdlib を nominal typed E 化(Rust 型)**: Rust の歴史(アプリ側が儀式を
   拒否し anyhow へ逃げた)と ADR-0003(全境界で map_err 儀式が発生)が二重に
   反証する。**却下**。
2. **Roc / OCaml 流の構造的タグ union を直接導入**: 理論的には W・B・X を同時に
   解くが、row 多相の推論エラーは難解で S のコストも最大。**却下**(D4 のタグ集合は
   同系だが、閉じた語彙 + 単純な集合包含に限定する点で異なる)。
3. **証拠なしで error set を今実装**: 計測文化に反する。分岐需要そのものが
   `??` で吸収されて出ない可能性が現実にある。**却下**(D4 は条件付き予約)。
4. **何もしない(現状維持)**: fs の not_found 分岐という圧力点が既に見えており、
   放置は string-match の本番流入を待つことになる。最低限 D1〜D3 は先回りとして
   必要。**却下**。
5. **error set の進路を恒久的に閉じる**: X(網羅性 = 修正箇所の機械的炙り出し)は
   MSR に直結しうる性質であり、証拠で裁く前に閉じる理由がない。**却下**(D4 で
   条件付き予約)。
6. **タグ語彙を標準タクソノミー(errno/HTTP)から借りる規則にする**(初稿 D6、
   「事前学習事前確率」基準): 批准討議で**却下** — 語彙は都度の設計判断とし、
   規則では縛らない。stdlib 内の語彙統一は慣行としては可。事前分布の観点は
   語彙選択の一考慮要素として残るが、公式基準には昇格させない。

## Consequences

- 得るもの: string-match 流入の先回り(D1/D3)、anyhow 相当の報告品質(D2)、
  error set への進路が falsifier 付きで固定される(D4)— 将来の再演を防ぐ
- 払うもの: D2 の stdlib 実装 + テスト、D3 の lint 実装(err 由来値の追跡)、
  dojo/grammar-lab 側に D4 の計測項目
- 未決のまま残るもの: stdlib エラーの分岐手段(OQ1〜3)。それまで stdlib 由来
  エラーの内容分岐は公式には不可能(`??` で吸収するか、variant E の自作ドメインに
  閉じる)

## Falsifier

1. **D3 の lint と `??` があっても string-match 分岐が書かれ続ける**と dojo で
   計測された場合 — ドクトリン+lint では封じられない証拠であり、OQ1(タグ判定 API)
   の優先度を繰り上げるか、本 ADR の段階モデル自体を見直す。
2. **grammar-lab の A/B で網羅性検査が MSR を改善しない**と出た場合 — D4 は
   永久に閉じ、その結果を本 ADR に記録する(ゲートの正常動作であり撤回ではない)。
3. **D2 の context 連鎖が実コーパスでほぼ使われない**と計測された場合 — 報告品質の
   問題認識(R 軸)が誤りだった証拠として、D2 を deprecated にする改訂を行う。

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
- 内部: ADR-0002 / ADR-0003(無損失原理)、C-029 系(OQ2 の移行対象)、
  2026-08-05 matrix(string-match 分岐の現状証拠)
