# ADR-0021: A lambda's failure channel carries the error type its `!`s agree on — contextual type first, then the join, String as the top

- **Status**: Accepted(設計裁定。実装は未着手 — 段階は D7)
- **Date**: 2026-09-27
- **決定範囲**: lambda 本体の `!` が落ちる「lambda 自身の失敗チャネル」の E をどう決めるか。
  型付け規則・既定・診断/fix-it・`__fallible_*` 正準形書き換えとの関係・移行影響・実装段階。
- **関連**: [ADR-0006](./0006-fallibility-polymorphic-hofs.md)(**D1 の「E は String 固定」と
  D4 を本 ADR が改訂**)、[ADR-0009](./0009-fn-type-quadrant-transparency.md)(D5 の
  #489 不変条件は不変)、[ADR-0012](./0012-typed-error-refinement-in-the-marker.md)
  (**D4-4 を本 ADR が置き換える**)、[ADR-0003](./0003-error-type-conversion-at-propagation.md)
  (不変 — 伝搬点の規則を lambda チャネルにそのまま適用する)、
  `docs/specs/result-option-effect.md` の L3(「E は String 固定」— 本 ADR が改訂)、
  #2601(発端)、PR #2714(E022 の callback 名指し)
- **経緯**: 2026-09-27、#2601 の裁定。実装側が A(現状 + 良い E022)/ B(正準形の
  tail 葉への拡張)/ C(チャネル E の推論)を並べた。他言語 8 種の実走 probe(Rust・Swift・Go・TypeScript・OCaml・Zig・Lean・MoonBit)と一次資料、
  Almide 自身の全コーパス掃引、dojo bank pilot の生成物で測って決めた。調査ノートは
  `almide-references/RESEARCH-typed-errors-in-closures.md`。

## Context

lambda は「ミニ可謬 fn」であり、本体の `!` は lambda 自身の失敗チャネルに落ちる
(ADR-0006 D1、#489 — クロージャ境界は越えない)。そのチャネルは今日、
**`Result[_, String]` に固定**されている(`infer_expr_g3_lambda` が
`Ty::result(fresh, Ty::String)` を作る)。variant E の `!` operand はこのチャネルに
Debug 文字列として黙って落ちる。

例外は 1 つだけある。`list.*` / `fs.*` の fallible HOF に渡した callback の本体が
**丸ごと** `call(..)!` のとき、`normalize_fallible_hof_callback` がマーカーを剥がして
`__fallible_*` に回し、call 自身の `Result[_, E]` がそのまま callback 型になる。
E が残るのはこの綴りのときだけである。

### 実測: 現状の型は綴りで変わる(almide 0.64.0、`almide check`)

`step: (Int, Int) -> Int!E`、囲む fn は `-> Int!E`、`xs |> list.fold(0, CB)!`:

| CB の綴り | 結果 |
|---|---|
| `(acc, x) => step(acc, x)!` | ok(E が残る) |
| `(acc, x) => { step(acc, x)! }`(波括弧を足しただけ) | **E022** |
| `(acc, x) => x \|> step(acc)!`(pipe に書き換えただけ) | **E022** |
| `(acc, x) => if x == 100 then acc else step(acc, x)!` | **E022** |
| `(acc, x) => { let v = step(acc, x)!; v + 1 }` | **E022** |
| user HOF の型付き slot `f: (Int) -> Int!E` に `(n) => step(n)!` | **E005**(正準形でも不可) |
| `let f: (Int) -> Int!E = (n) => step(n)!` | **E001** |

つまり、意味を変えない編集(波括弧・pipe・`let` 1 行)が HOF 呼び出しの型を
`Result[_, E]` から `Result[_, String]` に変え、エラーは遠くの `)!` に出る。
ADR-0012 D2 が `T!E` を fn 型 slot に置けると決めたのに、その slot に入る `!` 付き
lambda は 1 つも書けない。これが MSR の欠陥の形そのものである。

加えて 2 つの実害を測った:

- **check が通り rustc が落ちる穴が残っている(0.64.0)**。map の結果を `)!` ではなく
  `match` で消費すると、`(x) => match x { 0 => 0, _ => step(0, x)! }` は
  `almide check` を通り、rustc で E0277 + E0308、wasm では E082
  (`ty-mismatch:Scalar(Str)-vs-Named(0)`)。#2601 の `)!` 形だけが #2660 以降 E022 に
  なっている。
- **消去は native だけが受理する**。`-> Int!` fn で typed `!` を lambda に入れた形は
  native では `err Neg(-2)` を印字して通るが、wasm の両レグは
  `unwrap-err-ty-mismatch` で拒否する(E082)。

### 実測: LLM はこの形を書き、全部落ちる

dojo の bank pilot(run 36083949429、2026-09-25、compiler 0.63.1)を集計した。
typed-error タスクでは 66 attempt が variant E の call に `!` を付けて callback に
入れている(全て llama-3.3-70b):

| 位置 | 出現 | 失敗 |
|---|---:|---:|
| 正準形(本体全体が `f(..)!`) | 36 | 30 |
| ブロック途中の `let v = f(x)!` | 26 | 26 |
| if/match の tail 葉 | 16 | 16 |

**非正準形の typed `!` を 1 つでも含む attempt は 37 件で、37 件とも失敗した。**
そのうち 14 件はモデル自身が持ち込んだ形である(label-orders 6、ignore-expired-coupons 4、
restore-isbn 3、retired-signing-key 1)。典型は、baseline の正準形
`(cart, code) => apply_coupon(book, cart, code, today)!` を、修正のために
`{ let c = find(book, code)!; ... }` へ展開する編集である。これはまさに MSR が測る
「編集で壊れる」経路にあたる。
bank の人手 baseline も 3 本この形を含んでいて、0.64.0 では compile しない
(`early-recovery/{drop-exact-repeats-of-a-reading, reorder-events-with-small-clock-skew,
merge-overlapping-shifts-in-the-timesheet}`)。#2601 はこの bank の作問中に見つかった。

### 実測: 既存コードへの露出(`almide --emit-ast` + 解析器)

本体(ネストした lambda を除く)に `!` を持つ lambda を、test ブロック外で数えた:

| コーパス | 件数 | 正準形 | tail 葉のみ(B が覆う) | ブロック途中(C のみ) | variant E の `!` |
|---|---:|---:|---:|---:|---:|
| `spec/` | 109 | 79 | 11 | 19 | 0 |
| `stdlib/` | 2 | 0 | 2 | 0 | 0 |
| `tests/` + `examples/` | 18 | 14 | 0 | 4 | 0 |
| `research/benchmark/exercises/` | 0 | – | – | – | – |
| almide-dojo(main: tasks/msr/src/bank) | 6 | 6 | 0 | 0 | 0 |
| almai / comide / golemide / gramide-cli | 0 | – | – | – | – |
| **計** | **135** | **99** | **13** | **23** | **0** |

構文だけでは型が決まらない operand が 11 件あり、手で引いて全て String 系と確認した
(`Handler` / `HttpHandler` 型の閉包パラメータ、`Order.total -> Money!`)。
非正準形 36 件のうち、tail 葉だけで済むのは 13 件(36%)で、23 件(64%)は
ブロック途中にある。bank の生成物でも、非正準形 42 件中 26 件(62%)がブロック途中だった。

## Survey — 他言語は closure 内の伝搬演算子の E をどう型付けるか

実走 probe(Rust 1.96.1 / Swift 6.3.3 / Go 1.27rc2 / TypeScript 5.9.3)と
一次資料による。詳細・probe 全文・生出力は調査ノートにある。

| 言語 | closure 内の E | 何も縛らないとき | 注釈が要る場面 | 失敗時の診断(逐語) |
|---|---|---|---|---|
| Swift 6.3.3 | 投げるかどうかは推論、**型は注釈か context だけ** | `any Error`(top) | 具体 E が欲しく context がないとき(毎回) | `thrown expression type 'any Error' cannot be converted to error type 'E'`(外側の `try` に出る — probe) |
| Rust 1.96 | context からの推論(`?` は `From` 経由) | なし(E0282) | 戻り型の context がない `let` closure | `error[E0282]: type annotations needed … cannot infer type of the type parameter `E`` + `Ok::<i64, E>` の提案(probe) |
| MoonBit | arrow `x => …` は**本体から精密に推論**(`raise Bad`)。`fn` リテラルの推論は deprecated | 素の `raise` = `Error` | arrow では不要 | `[4122] Function with error can only be used inside a function with error types…`(probe) |
| OCaml 5.5 | `let*` は HM で推論(同一 E のみ)。例外/effect は型に出ない | 型変数 | なし | `The constant 42 has type int but an expression was expected of type string`(probe) |
| Gleam | `use` の匿名 fn は HM で推論(同一 E のみ) | 型変数 | なし | `Type mismatch … Expected type: Result(Int, Bool)` |
| Lean 4.29 | lambda の monad は期待型か本体の action から | なし(metavariable) | `throw` だけで context なし | `typeclass instance problem is stuck MonadExcept String (?m.18 s)`(probe) |
| Koka / Roc / jacquard | open row / open tag union で推論・合成 | open row / union | ほぼなし | `effects do not match … inferred effect: <…\|_e>`(Koka) |
| Zig 0.16 | closure なし。nested fn の error set は推論、fn pointer 型は明示 set | 推論 set | fn pointer 引数では常に | `function type cannot have an inferred error set`(probe) |
| Go 1.27 / TypeScript 5.9 | 型に error チャネルがない(`error` / `unknown` 固定) | top | – | `TS2322: Type 'unknown' is not assignable to type 'Bad'`(catch 側 — probe) |
| aver / vera / vibe / wado | aver・sui は lambda なし、vera は effects 注釈必須、vibe・wado は本体から推論 | – | vera は常に | – |

Swift の SE-0413 は Future directions に、まさに本 ADR の規則を書いている。
「closure の thrown error type を、catch されない全 throw site の `errorUnion(E1, …, EN)`
として推論する」。ただし既存コードが closure の `any Error` に依存しているので、
source 互換のため upcoming feature flag(`FullTypedThrows`)の後ろに置く、としている。
Revision 6 は「Closure type inference did not get implemented in Swift 6.0」と記録し、
コンパイラのテスト `test/expr/closure/typed_throws.swift` は
「We do not infer thrown error types from the body, because doing so would break existing code」
と書いている。**Swift が推論を出荷しない理由は規則の欠陥ではなく、既存コードの型が
変わることにある。** Almide ではその既存コードが 0 件と測れている(Context の露出表)。

読み取れることは 4 つある:

1. **HM 系・効果系(OCaml / Haskell / Koka / Lean / Roc / Gleam)は closure の E を
   本体から推論する**。注釈は要らない。ADR-0012 D4-4 が恐れた「推論エラー難」の実体は
   Koka の row 変数と Roc の open/closed union であり、E そのものの推論ではない。
2. **E を context から決める言語(Rust)は、context がないときに失敗する**。
   Rust の E0282 は、`?` が `From` を経由するので operand から E が決まらないために出る。
   Almide には `From` がない(ADR-0003 D4)。そのため operand の E がそのまま候補になり、
   この失敗類は構造的に生じない。
3. **Swift は Almide の現状と同型の場所に立っていて、それを自分でも未完成扱いにしている**。
   closure の thrown type は、注釈がなければ `any Error`(top)に落ちる。typed の
   `try` を closure に入れて `throws(E)` fn から `try xs.map { ... }` すると、
   `thrown expression type 'any Error' cannot be converted to error type 'E'` が
   **外側の `try` に**出る。これは PR #2714 以前の Almide の E022 と同じ位置・同じ構造の
   失敗である。本体からの推論(FullTypedThrows)は 2026 年の 6.3.3 でも
   experimental で、production compiler では有効化できない。Swift には逃げ道として
   closure の `throws(E)` 注釈がある。Almide には lambda の戻り型注釈がないので、
   逃げ道がない。
4. **いちばん新しい LLM 時代の言語は、本体からの推論に寄っている**。MoonBit は
   arrow lambda の E を本体から精密に推論し(`(String) -> Int raise Bad`)、推論しない側の
   `fn` リテラルの綴りを deprecated にした。vibe と wado も closure の効果を本体から推論する。

## Decision

**lambda の失敗チャネルの E を固定の String から「決まる型」に変える。
contextual な期待型があればそれを使い、なければ本体の `!` operand の E の join を使う。
既定と top は String とする。`__fallible_*` の正準形剥がしは型付けの役目を失い、
lowering の最適化に格下げする。**

### D1. 型付け規則(L3 の改訂)

lambda λ の失敗チャネルを `Result[T, ε]` とする。ε は次の順で決める:

1. **検査モード(期待型が E を持つ)**: λ が `(A) -> B!E` / `(A) -> Result[B, E]` の
   期待型に対して検査されるとき、ε := E とする。期待型の出どころは、user HOF の
   型付き slot、`let f: (A) -> B!E = ...` の注釈、`-> (A) -> B!E` の fn の値 tail の 3 つ。
   本体の各 `!` は fn 本体とまったく同じ規則(`bang_error_channel`、ADR-0003 D1/D2)で
   ε に照合する。一致すれば無変換で伝搬し、String・Option・非 Result の effect call・
   別の E' が来れば、その `!` の位置で E022 にする。
2. **合成モード(期待型が E を持たない — `list.*` / `fs.*` の組み込み HOF、
   注釈のない `let`)**: ε := ⊔ { err(o) | o は λ 自身の本体の `!` operand }。
   ここで err(`Result[_, E]`) = E、err(`Option`) = String(none → err("none")、ADR-0003 D3)、
   err(Result を返さない effect call) = String とする。⊔ は、現行の暗黙変換の順序
   「任意の E ≤ String」の上の最小上界で、**全 operand が同じ E なら E、1 つでも食い違えば
   String** になる。
3. **既定**: `!` operand を持たないチャネル(effect slot の lambda など)と、λ を抜ける
   時点で E が未解決の operand を含むチャネルは String とする。どちらも今日の挙動である。

`!` は最内の lambda のチャネルに落ちる(#489 / ADR-0009 D5 は不変)。test ブロック内の
`!` は unwrap のままで、チャネルを持たない(L9 は不変)。

この規則には、ADR-0012 D4-4 の懸念を外す性質が 3 つある:

- **⊔ は全域**。順序に top(String)があるので、合成モードで型付けが「失敗」する
  ことはない。row 変数も汎化もなく、推論器が作る新しいエラーメッセージは 0 種類である。
- **受理を保存する**。今日 check を通る lambda はすべて通り、ε が String から E に
  変わるのは「全 `!` が同じ E に一致した」ときだけである。
- **綴りに依存しない**。波括弧・pipe・`let` の追加・if/match への移動は operand の
  集合を変えないので、ε も変わらない。

### D2. 観測可能な変化は 1 種類だけ

ε が String から E に変わった lambda を渡した HOF 呼び出しの型は
`Result[_, String]` から `Result[_, E]` になる。その先の消費は次の 3 通りになる:

- `-> T!E` fn での `)!`: **受理される**(#2601 が求める修正)。
- `-> T!` fn での `)!`: fn レベルの E → String の Debug 化で受理され、
  **印字は今日と 1 バイトも変わらない**。同じ E 値の Debug を、内側の `!` ではなく
  外側の `!` で取るだけの違いである。
- 値位置での消費(`match r { err(e) => string.len(e) }` など): ここだけが型を変える。
  **既存の全コーパスでの該当は 0 件**(Context の露出表で variant E の `!` を持つ
  lambda が 0 件だから)。

### D3. 診断と fix-it

ε そのものの推論は失敗しない(D1)。代わりに次の 4 つの場面で診断を出す:

1. **検査モードで operand が ε と食い違う**: その `!` の位置で E022 を出す。
   文言と hint は fn 本体のもの(`bang_error_channel`)をそのまま使い、主語を
   「the callback's error type is `E` (from the slot `f: (Int) -> Int!E`)」にする。
   fix-it は ADR-0003 D2 の方向別正準形で、`int.parse(s) |> result.map_err((e) => BadValue(e))!`
   のように String を運ぶ ctor が 1 つなら名前まで埋める。今日の E005/E001
   (「fn(Int) -> Result[Int, String] を渡した」)は出なくなる。
2. **合成モードで ⊔ が String に落ち、それが `-> T!E` fn の `)!` で E022 になる**:
   PR #2714 の E022 を拡張し、**食い違った operand を 2 つとも名指しする**:
   ```
   error[E022]: operator '!' cannot propagate this error: the fn's error type is `ShiftErr`,
     but the callback's `!`s fail with different types — `ShiftErr` (line 43, col 20)
     and `String` (line 44, col 31) — so its channel is `String`
     hint: convert the odd one at its `!` so every `!` in the callback fails with `ShiftErr`:
       int.parse(raw) |> result.map_err((e) => BadNumber(e))!
   ```
3. **未解決 operand で String に落ちた**: hint で「その callback を型付き `let` に
   束縛する」ことを提案する(`let step_cb: (Acc, Line) -> Acc!ShiftErr = (s, l) => ...`)。
   これで検査モードに入る。lambda の戻り型注釈という新構文は足さない。
   Swift の `throws(E)` 注釈の役は、既存の fn 型注釈が担う。
4. **check が通り rustc が落ちる穴(Context)**: D1 の後は ε と HOF 結果の型が
   一致するので、構造的に閉じる。D7 の step 1(#2722)で先に E022 として塞ぐ。

### D4. `__fallible_*` と正準形剥がしの格下げ

`normalize_fallible_hof_callback` は今日 2 つの仕事をしている。(a) `list.map` などを
`__fallible_map` などへ振り替えること、(b) 正準形のマーカーを剥がして E を残すこと。
D1 の後、(b) は型付けに不要になる。合成モードの ε が同じ E を出し、`__fallible_*` の
carrier は既に E ジェネリックだからである。正準形の typed fold は今日 native と wasm の
両方で `ok 3` / `neg -2` を出しており、これがその実証になっている。(a) は残す。(b) は
alloc 台帳・size ratchet・perf で A/B を取り、差がなければ撤去し、差があれば
lowering 側の最適化として残す。型の上ではどちらでも同じになる。

### D5. 他 ADR との関係(明示の改訂)

- **ADR-0012 D4-4 を置き換える。** 旧文「lambda の失敗チャネルは String のまま
  (ADR-0009 L3)。使用駆動で E を推論すると Koka 型の推論エラー難を輸入する」は
  撤回する。撤回の根拠は、D1 の ⊔ が全域で row 変数を持たないこと(Survey の 1・2)と、
  「明示注釈の Result で綴る」という代替の道が実在しないこと(Context の表の最後の 2 行)
  である。なお、引用元の「ADR-0009 L3」は ADR-0009 には存在しない。L3 は
  `docs/specs/result-option-effect.md` の L1〜L9(2026-08-07 批准)の 1 項目である。
  改訂するのはそちらの L3 になる。
- **ADR-0006 D1 の「E は ADR-0002 D2 に従い String 固定」を改訂し、D4 を撤回する。**
  HOF の E は callback の ε から流れる(可謬性ビットと同じ透過)。D4(E ジェネリック
  traverse は非サポート)は、正準形の経路で既に事実上破れている。加えて Falsifier 3
  (「カスタム E traverse の実需要 ≥3 箇所」)が発火した。dojo bank の人手 baseline 3 本、
  モデル自身が持ち込んだ 14 attempt、#2601 本体とそのコメントの 2 形がその実需要である。
- **ADR-0009 は不変。** D5 の「lambda 内の `!` は lambda 自身のチャネルに落ちる」は
  そのまま残り、本 ADR はそのチャネルの E を決めるだけである。D2 の透過も同じ形で E に
  延びる。
- **ADR-0003 は不変。** lambda 内の各 `!` は、fn 本体と同じ「`!` は E を変換しない」規則で
  ε に照合される。ただし実装上の注記が 1 つある。ADR-0003 D2 は CustomE → String を
  check エラーと書いているが、実装(#2635、`bang_error_channel.rs` 冒頭の doc comment)は
  fn レベルで Debug 化して受理している。D1 の ⊔ はこの**実装された**順序
  (E ≤ String)の上で定義した。将来 D2 が文字どおり施行されれば、順序は top を失い、
  合成モードの食い違いは D3-2 の E022 + map_err fix-it で拒否される形に変わる。
  規則の骨格(contextual → 一致 → 既定)は、その場合も変わらない。

### D6. 移行影響

- 受理集合の縮小: **0**(D1 の受理保存)。
- 型が変わる HOF 呼び出し: 全コーパス(spec / stdlib / tests / examples / exercises /
  dojo main / almai / comide / golemide / gramide-cli)で **0**。
- 新たに受理される形: #2601 の 4 形とそのコメントの 2 形、dojo bank の人手 baseline 3 本、
  bank pilot で落ちた非正準 typed attempt 37 件の型エラー部分(他の誤りは別)、
  型付き slot / 型付き `let` への `!` 付き lambda。
- 契約台帳: native で観測できる出力は変わらない(D2)。wasm では今日 E082 で拒否している
  形が走るようになるので、新しい cross-target 契約 1 本と fixture で固定する
  (C-NNN は almide/als で先に採番する — CLAUDE.md の順序)。

### D7. 実装段階(1 段階 1 PR、各段階に issue — すべて #2601 配下)

1. **穴を先に塞ぐ**(#2722): `match` で消費する map の形(check 通過 → rustc E0277)を、
   現行規則の下で E022 にする。#2714 の `lambda_err_erasures` を、`)!` だけでなく
   HOF 結果の値消費にも効かせる。バグ修正なので D1 を待たない。
2. **checker: ε の合成**(#2723): `lambda_ret` の E を fresh にし、operand の E を記録して
   λ 出口で ⊔ を取る(既定 String)。検査モードでは期待型の E を ε に入れる。
   E005/E001 の typed-slot 拒否を解消する。spec の L3 改訂と
   `spec/lang/fallible_lambda_test.almd` のピン追加もこの段階に含める。
3. **Rust lowering**(#2724): ε ≠ String の lambda を `Result<_, E>` の closure として出し、
   `?` を無変換にする。⊔ が String に落ちた lambda には今日の Debug `map_err` を残す
   (`pass_lambda_type_resolve.rs` / `walker/expressions_control.rs`)。
   生成 Rust が警告なしで compile することを確認する。
4. **wasm 両レグと interp**(#2725): structural leg の `lower_try_unwrap` の
   `unwrap-err-ty-mismatch` 壁を ε 一致の経路に通す。incumbent と almide-interp も
   ε を読む。3-way oracle の fixture と新しい C-NNN(als 先行)をこの段階で入れる。
5. **診断**(#2726): D3-2 の「食い違った operand を 2 つ名指し」と D3-3 の型付き `let` hint、
   `tests/diagnostics/` の broken/fixed 組。
6. **正準形剥がしの A/B と格下げ**(#2727): D4(b) を alloc 台帳・size ratchet・perf で比較し、
   撤去するか lowering 最適化として残すかを決める。
7. **docs**(#2728): `docs/specs/result-option-effect.md`(L3 と非目標 4)を改訂する。
   CHEATSHEET / llms.txt の callback 例は、実装が出荷されてから `almide check` fence で
   足す(#1483 の規則)。

## Rationale

- **MSR の定義に直結する**: 今日の規則は、意味を変えない編集(波括弧・pipe・`let`・if)で
  型を変える。bank pilot で非正準 typed attempt が 37/37 落ちたのは、その編集経路を
  モデルが実際に踏むからである。D1 の ε は operand の集合だけで決まり、綴りに依存しない。
- **B では足りない**: tail 葉への拡張(B)が覆うのは、既存コーパスの非正準形の 36%、
  bank の非正準 typed 出現の 38%(16/42)にとどまる。残りのブロック途中の形は、
  モデルが正準形を修正のために展開するときに**必ず**通る形である
  (ignore-expired-coupons の例)。B は構文の正準形を 1 段広げるだけで、
  「綴りで型が変わる」欠陥の形は残る。
- **C の費用は ADR-0012 が見積もったより小さい**: 恐れた推論エラー難は row / union の
  性質で、2 段の順序上の ⊔ にはない。移行影響は 0 件である。
- **Swift の位置は反面教師である**: closure 推論なし・top 既定という Almide の現状と
  同型の設計に、Swift は注釈という逃げ道を足して、推論は experimental に留めている。
  Almide には注釈の逃げ道がないので、同じ位置に留まれば「書けない」になる。
  Swift が推論を出荷しない理由は既存 closure の型が変わる source break
  (Survey 参照)であり、Almide ではその該当が 0 件と測れている。D1-2 の ⊔ は、SE-0413 が
  Future directions に書いた `errorUnion` を、Almide の 2 段順序(E ≤ String)に落とした形である。

## Alternatives — 検討して却下した案

1. **A: 現状維持 + #2714 の E022**: 診断は良くなるが、Context の表の 7 形中 6 形は
   書けないままになる。型付き slot・型付き `let` への `!` lambda は正準形でも不可で、
   ADR-0012 D2 の約束が空になる。bank pilot の 37/37 失敗は変わらない。**却下**。
2. **B: 正準形の tail 葉への拡張**: 局所的で推論は要らないが、覆うのは 36〜38% である。
   「綴りで型が変わる」を 1 段押し戻すだけで、境界(ブロック途中の `let`)は、修正の
   編集が最も踏む場所に残る。**却下**。
3. **C-strict: 食い違う operand を常にエラーにする(String への暗黙 ⊔ を持たない)**:
   ADR-0003 D2 の字面には合う。しかし fn レベルの実装が E → String を受理している以上、
   lambda だけ厳しくすると「fn と lambda で `!` の規則が違う」非対称が新しく生まれる。
   今日の受理も縮む。D5 の注記どおり、D2 が施行されたときに一緒に移る。**保留**。
4. **lambda の戻り型注釈構文を足す(Swift の `throws(E)` 型)**: 検査モードの入口は
   既存の fn 型注釈(`let` / slot)で足りる。新構文は 1 意味 2 綴りになる。**却下**。
5. **HOF 側の E ジェネリック化だけを行い、lambda は String のまま**: carrier は
   既に E ジェネリックなので、欠けているのは lambda 側だけである。**不成立**。

## Consequences

**得るもの**: 綴りに依存しない callback 型。型付き slot の実用化(ADR-0012 D2 の完結)。
check 通過・rustc 失敗の穴の構造的な閉鎖。bank の typed-error 群の人手 baseline が
そのまま compile するようになる。native ⇄ wasm の受理の一致。

**払うもの**: checker(チャネルの E を fresh にする、operand の記録、λ 出口の ⊔)、
Rust closure 型、wasm 両レグ、interp の 4 箇所。D7 の 7 段階。型が変わる既存コードは 0 件で、
移行の手作業はない。

## Falsifier

1. **D7 step 2 の着地後、spec / dojo のどこかで「ε が E に変わったことで値消費が
   型エラーになった」実例が 3 件以上出た場合**: 合成モードを撤回して検査モードだけ残し、
   合成は String 既定に戻す。
2. **λ 出口で未解決の operand が String 既定に落ち、それが原因の E022 が dojo の
   失敗コーパスで typed-error 失敗の主因になった場合**: ⊔ を λ 出口ではなく fn 単位の
   解決後に遅延させる(実装の変更で、規則は不変)。
3. **ADR-0003 D2(CustomE → String の check エラー)が施行された場合**: D1-2 の
   「食い違い → String」を「食い違い → E022 + map_err fix-it」に置き換える(C-strict へ移る)。

## References

- 調査ノート: `almide-references/RESEARCH-typed-errors-in-closures.md`
  (probe 全文・生出力・解析器・bank pilot 集計)
- Swift SE-0413 — [Typed throws](https://github.com/swiftlang/swift-evolution/blob/main/proposals/0413-typed-throws.md)
- Rust — [E0282](https://doc.rust-lang.org/error_codes/E0282.html)、
  [`?` と `From`](https://doc.rust-lang.org/reference/expressions/operator-expr.html#the-question-mark-operator)
- 内部 — #2601、PR #2714、#2635(`bang_error_channel.rs`)、#2660、
  `crates/almide-frontend/src/check/infer_calls_closures.rs`
  (`infer_expr_g3_lambda` / `accept_lambda_channel_prop` /
  `normalize_fallible_hof_callback`)、`crates/almide-wasm/src/data.rs`
  (`unwrap-err-ty-mismatch`)、`docs/specs/result-option-effect.md`(L1〜L9)、
  almide-dojo bank pilot run 36083949429
