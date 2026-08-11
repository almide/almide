# ADR-0002: Fallibility and effect are orthogonal axes; `-> T!` marks pure-fallible

- **Status**: Accepted(設計批准。実装は未着手 — Phase 計画は本文 §D5)
- **Date**: 2026-08-05
- **決定範囲**: 関数宣言の失敗チャネル表記(`-> T!`)、`effect fn` の意味論の再分解、
  `!` 演算子の意味の固定、失敗を「値」で表すか「⊥(統制停止)」で表すかの規準
- **関連**: [#1055](https://github.com/almide/almide/issues/1055)(effect-typed fn parameters — 本 ADR が語彙を供給)、
  C-211 / C-029([contracts.toml](../contracts/contracts.toml))、
  [result-option-effect.md](../specs/result-option-effect.md)(本 ADR に伴い要改訂)、
  #489(lambda 境界)、#840/#841(never-err elision)
- **批准**: 2026-08-05 の設計討議。起点は「`fn f(s: String) -> Int = int.parse(s)!` が
  E022 になるが、どうあるべきか」。同日実施した全演算子×全文脈の実測
  (v0.53.6、287 セル、7 スライス並列 probe)を証拠として使用。

## Context — 何が問題だったか

### 1. `effect fn` は 3 つの性質の縮退である

現行の `effect fn` は 1 キーワードに次を融合している:

1. **capability** — host effect(fs / http / process / random …)を呼べる
2. **fallibility** — 宣言 `-> T` が `Result[T, String]` に lift される
3. **人間工学** — 本体内で auto-`?` が走り、`!` が合法になる

このため「失敗はするが世界には触れない関数」を軽く書く綴りがない。
`int.parse` のような関数は `-> Result[Int, String]` と手で書く(19 文字)しかなく、
E022 の hint も「Result と書け / effect fn にせよ / `??` を使え」と、
**意味の違う 3 つの逃げ道を並べる**ことしかできていなかった。

### 2. 実測が示した縮退の歪み(2026-08-05 matrix、抜粋)

- `effect fn` を error-lift 目的だけで使うと capability が付いてきて pure から呼べない
  (E006)。逆に明示 `-> Result` で宣言し直すと **pure fn からも渡せてしまう**
  (effect の洗浄)— capability と fallibility が別物である証拠。
- codegen には never-err elision(#840/#841)が既にあり、
  「失敗しない effect fn」(`env.args`、`random.int`)を**最適化として裏で判別している**。
  表面に型が無いだけで、実装は 2 軸を知っている。
- `try_*` family の callback(`(A) -> Result[B, E]`)で E が未確定になる摩擦
  (E025 クラスタ)が多発 — callback slot に fallibility を書く語彙がないため。
- C-211(0.53.5)は pure fn `-> Result/Option` での `!` を既に解禁しており、
  **pure-fallible 象限は意味論として実在する**。欠けているのは表記だけ。

### 3. 「pure が Result を返してよいのか」への回答が用語として無かった

spec に purity / fallibility / totality の区別が書かれておらず、
「pure なのに失敗するのは変では」という混乱が設計討議のたびに再演されうる。

## Decision — 何を決めたか

**可謬性(fallibility)と効果(effect)を直交する 2 軸として分離する。
`effect` キーワードは capability のみを、戻り型位置の後置 `!`(`-> T!`)は
fallibility のみを意味する。** 最終文法は 4 象限:

```almide
fn        f() -> Int      // pure  ・総(失敗しない)
fn        f() -> Int!     // pure  ・可謬     ← 新設(Phase 1)
effect fn f() -> Int      // effect・総       ← Phase 3 で表現可能になる
effect fn f() -> Int!     // effect・可謬     ← 今の effect fn
```

### D1. 用語の規準(spec に定義段落として置く)

- **pure** = 本体が世界に触れない(host effect なし)。参照透過・再現可能。
- **fallible** = 戻り値が `Result[T, String]` に lift される。**codomain の形の話であり、
  純粋性とは無関係**。`int.parse : String -> Result[Int, String]` は完全に純粋である
  (同じ入力→同じ Result。部分関数の全域化)。
- **失敗の表現規準**: 結果が**引数だけから決まる**失敗は値(Result)で表す。
  **世界の状態に依存する**失敗は effect 圏で表す。
  **⊥(T6 統制停止)で表す失敗は閉じた列挙**(算術の定義域エラー、添字範囲外)であり、
  安易に増やさない。プロセスとしての失敗(exit 1)への変換は
  main / effect 圏 / 明示演算子の境界でのみ起こる。

### D2. `-> T!` の表面

- `!` は **fn 宣言の戻り型位置**と **fn 型のパラメータ slot**(`f: (A) -> B!`)にのみ
  書ける。**型構成子ではない**(`List[Int!]` は不可)— Swift の `throws` と同じ
  「関数の属性」モデル。
- デノテーション: `-> T!` の関数を呼ぶと `Result[T, String]` が返る。それだけ。
- **E は String 固定**。カスタム E は従来どおり明示 `-> Result[T, MyErr]`。
  (`!` は「共通 E の可謬」専用 — stdlib の E が String 一色である現実に合わせる。)

### D3. 宣言人間工学は fallibility 軸の持ち物

`-> T!`(および `-> Result[T, String]` 宣言)の本体は、現行 effect fn と**同一の**
lift 人間工学を得る: 末尾 T の auto-ok、可謬呼び出しへの auto-`?`、`!` と `err(...)` 合法。
capability(E006 系)は一切付かない。
これにより「effect fn の人間工学」は「fallibility の人間工学 + capability 検査」に分解される。

### D4. `!` 演算子の意味は「伝搬」のみに固定する

`x!` = 「x の失敗を、囲む宣言の失敗チャネルへ変換して伝搬する」。チャネルが無ければ
E022。**trap(unwrap-or-abort)意味論を `!` に載せない。署名を黙って lift しない**
(Alternatives 参照)。E022 は「失敗チャネル不在」ファミリとして再定式化し、
operand 型を見て hint を分岐、末尾形には machine-applicable な
`try: fn f(s: String) -> Int! = int.parse(s)` を出す。
併せて `!` の E 型不一致(String operand → CustomE fn)は **check 時エラー**にする
(現状は check 素通り→ICE。逆方向の暗黙 Debug 文字列化も廃止し、
`result.map_err` を挟む旨の hint を出す)。

### D5. 移行(E040 deprecation 窓の前例を踏襲)

- **Phase 1(追加のみ・非破壊)**: pure に `-> T!` 導入。E022 hint 更新。
  `try_*` family 等の callback slot を `(A) -> B!` 表記に更新(意味不変)。
- **Phase 2(警告窓)**: `effect fn f() -> T!` を現行 effect fn の同義として許可。
  本体が可謬なのに `-> T` な effect fn へ deprecation 警告 + `almide fix` が `!` を機械挿入。
- **Phase 3(意味反転)**: `effect fn f() -> T` を**総**(lift なし)と再定義。
  `env.args` / `random.int` らの署名が正直になり、never-err elision が型に裏打ちされる。
  stdlib 宣言を一括更新。**Phase 3 の実施時期は未定**(Falsifier 条件を先に監視)。

## Rationale — なぜそれか

### 純粋性の理論

失敗値(Result)は参照透過性を一切損なわない。損なうのは ⊥ の方であり、
Haskell(`Either`)/ Elm(`Result`)/ Gleam など純粋性の強い言語ほど
失敗は**値でしか**表せない。`-> Int!` は purity を保つ側の解、
trap は既にある妥協(T6)を広げる側の解である。

### 他言語の実測比較(2026-08-05 調査)

型付きエラー勢は全員「失敗チャネルの無い関数での unwrap は**拒否**」かつ
「trap は**別綴り**」:

| 言語 | 伝搬 | チャネル無しでの伝搬 | trap 綴り |
|---|---|---|---|
| Rust | `expr?` | E0277 で拒否("the `?` operator can only be used in a function that returns `Result` or `Option` (or another type that implements `FromResidual`)") | `.unwrap()` / `.expect()` |
| Swift | `try` + `throws` | 拒否("errors thrown from here are not handled") | `try!` |
| Zig | `try expr` | 拒否(戻り型が error union でない)。**`fn f() !i32` の 1 グリフで宣言でき、error set は推論**("An error set type and normal type can be combined with the `!` binary operator to form an error union type" — Zig Language Reference) | `catch unreachable` |
| Gleam | `use` 糖衣 | 構造的に不可 | `let assert Ok(x)` |
| Go | 演算子なし | — (`try` 提案 [golang/go#32437](https://github.com/golang/go/issues/32437) は明示性への懸念で**否決** — 暗黙伝搬への警句) | なし |

`-> T!` は Zig の「宣言 1 グリフ」を Swift の「関数属性」モデルで取り込む形。
なお Swift `try!` / Kotlin `!!` では **`!` 系こそ trap** であり、almide の `!`(伝搬)は
世間の glyph 直感と逆 — CHEATSHEET / llms.txt に「almide の `!` は Swift の `try` であって
`try!` ではない」と明記する(D4 の系)。

### E 変換

Rust は `?` に `From::from(e)` を自動挿入し、**From が無ければ check エラー**。
almide の現状(一方向 ICE・逆方向暗黙 Debug)はこのモデルの劣化形であり、
D4 の check 時エラー化はその是正。変換フック(From 相当)の導入は本 ADR では
決めない(open question)。

### 期待できる波及(matrix の実測に基づく)

- `!` の文脈依存 4 義が 1 原理に畳まれ、E022 の修正が「1 文字」になる
- `try_map` の署名が `fn try_map[A, B](xs: List[A], f: (A) -> B!) -> List[B]!` と読め、
  slot の `!` が E を String に確定させるため **E025 摩擦クラスタが構造的に消える**
- effect 洗浄・「auto-lift された effect fn は callback に渡せない」問題に
  2 軸の型語彙で答えられる(#1055 の capability 軸 × 本 ADR の fallibility 軸)
- `http.serve` のハンドラ slot は `effect (HttpRequest) -> HttpResponse!` と書ける

## Alternatives — 検討して却下した案

1. **黙った lift**(`-> Int` のまま実体 Result): 署名が嘘になる。宣言上区別のつかない
   2 種類の pure fn が生まれる。effect fn が許されるのは `effect` キーワードという
   マークがあるから。**却下**。
2. **`!` に trap 意味論**(err なら T6 halt): 呼び出し側の迎撃(`??` / match)が不可能になり、
   effect fn から pure helper への切り出しで「伝搬→即死」に**静かに**意味が変わる。
   MSR(modification survival rate)の看板と正面衝突。**却下**。
3. **trap 専用綴り `!!` / `assert` 系の新設**: 型付きエラー勢は全員持っている
   (Rust `.unwrap()` 等)ため筋は悪くないが、`-> T!` により「Int が欲しい」需要の大半が
   可謬文脈内の伝搬で満たされ、必要性が薄まる。**保留**(却下ではない)。
   **2026-08-07 再確認: 据え置き継続**(#1133 で裁定・クローズ)。v0.54〜v0.56 が
   「綴りを減らす」アーク(auto-? 削除・try_ 7 関数削除・collect 削除・`Option[T]`→`T?`)
   だった直後に新グリフを足すのは方向が逆、というのが理由。解禁条件は #1133 に 3 つ明記:
   (1) sentinel が誤値を流した実例 or Dojo MSR での詰まりの計測が 1 件以上、
   (2) 後置族(`!`/`!!`/`?`/`??`/`?.`)の優先順位表を導入と同時に確定、
   (3) L9 の test フォーク解消を同時に裁定(分離すると 2 度手間)。
4. **Zig 流の E 推論**(`!T` の error set を本体から推論): E の多相化は stdlib の
   「共通 E = String」という現実と合わず、型表示の予測可能性(LLM writability)を下げる。
   **却下**(String 固定 + カスタム E は明示 Result)。
5. **`T?` = Option sugar**(Kotlin 流): fallibility 軸ではなく型表記一般の話。
   スコープ外として**分離**(採否は別議論)。
6. **現状維持**(明示 `Result[T, String]` のみ): 動作はする(C-211)。しかし E022 hint の
   罠(Option 提案が Result operand で不成立)、E025 摩擦、effect fn の縮退は残る。
   19 文字の摩擦は「失敗を型に書く」文化の普及速度に直結する。**却下**。

## Consequences — 何が良くなり、何を払うか

**得るもの**: 上記 Rationale の波及 4 点。加えて `effect` / `!` が 1 キーワード 1 意味に
戻り、stdlib の署名が正直になる(Phase 3)。

**払うもの**:
- 文法表面の追加(戻り位置の `!`、fn 型 slot の `!`)とその教育コスト。
- **表記の二重化**: `(A) -> B!` と `(A) -> Result[B, String]` は同一デノテーション。
  0.53.5 の one-name-one-meaning 運動(#1078)と緊張するが、あれは「1 関数に 2 名前」、
  これは「1 型に 2 記法」(`16` と `0x10` の関係)であり、デノテーション規則 1 行で固定する。
  **fmt がどちらへ正規化するかは未決**(open question)。
- Phase 3 は全 effect fn の署名に触れる移行(機械書き換え可能だが、外部パッケージへの
  波及は E040 のときと同様の下流走査が要る)。
- pure-fallible に auto-`?` を入れることで、auto-`?` の位置非対称
  (matrix B 論点: 位置・注釈・パターン形で挙動が変わる)を pure 圏にも輸入する。
  位置マトリクス自体の改革は**別トラック**(本 ADR の従属 open question)。

## Falsifier — 何が起きたらこの決定を撤回するか

1. **dojo の MSR 計測で、`T!` / `Result[T, String]` の 2 記法併存が単一記法より
   有意に生存率を下げた場合**(記法の二重化が 19 文字の摩擦より害が大きいと実証されたら、
   `!` 表記を捨てるか Result 表記を fn 宣言位置から退役させる ADR で supersede)。
2. **Phase 2 の警告窓で、可謬/総を静的に分類できない effect fn が大量に見つかった場合**
   (Phase 3 の意味反転は成立しない — effect fn の lift 強制を維持する形へ縮退)。
3. **#1055 の設計が「fallibility は関数属性ではなく型構成子であるべき」という結論に
   達した場合**(D2 の属性モデルと矛盾するため、統合 ADR で supersede)。

## References

- 実測データ: 2026-08-05 の演算子×文脈マトリクス(v0.53.6、287 セル。
  probe: セッション scratchpad `matrix/`、集計 `matrix_results.json`)
- Rust: The Rust Reference — [The question mark operator](https://doc.rust-lang.org/reference/expressions/operator-expr.html#the-question-mark-operator)、
  [`std::ops::FromResidual`](https://doc.rust-lang.org/std/ops/trait.FromResidual.html)
- Swift: [The Swift Programming Language — Error Handling](https://docs.swift.org/swift-book/documentation/the-swift-programming-language/errorhandling/)
- Zig: [Zig Language Reference — Errors](https://ziglang.org/documentation/master/#Errors)
- Go: [proposal: Go 2: error handling: try statement](https://github.com/golang/go/issues/32437)(declined)
- Gleam: [Gleam Tour — let assert](https://tour.gleam.run/advanced-features/let-assert/)
- 内部: C-211 / C-029(docs/contracts/contracts.toml)、#489、#840/#841、#1051、#1055、#1078
