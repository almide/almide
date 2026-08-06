# ADR-0003: Error-type conversion at propagation points — lossy conversion must be spelled

- **Status**: Accepted
- **Date**: 2026-08-05
- **決定範囲**: 伝搬点(明示 `!`・effect/fallible 文脈の auto-`?`)における
  エラー型 E の変換規則 — 一致・不一致(両方向)・変換フックの是非・既存暗黙(none→err)の位置づけ
- **関連**: [ADR-0002](./0002-fallibility-effect-orthogonal.md)(D4 の仮決めを本 ADR が
  確認・詳細化する。修正ではない)、#1103(Phase 1)
- **経緯**: 2026-08-05 の実測 matrix で `!` の E 型 3 セルが「一致=完動 /
  String→CustomE=ICE / CustomE→String=暗黙 Debug 文字列化」と三者三様に割れている
  ことが判明。初稿は Swift 流「String=⊤ への暗黙上方変換」を提案したが、討議で
  strict に倒した(Alternatives 1 参照 — 却下理由ごと保存)。

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
   `option.to_result`・`-> T!`(ADR-0002)— すべて String。
2. **main の失敗チャネルは String**。未処理 err は最終的に `Error: <msg>` + exit 1 に落ちる。
3. `!` は Option operand の none を `err("none")` に暗黙変換する(既存仕様)。

## Decision

**伝搬点での E 変換は「無損失なら暗黙、損失があるなら必ずコードに見える」を原理とする。
E 不一致は方向を問わず check 時エラーであり、compiler が変換の正準形を hint で供給する。**

### D1. E 一致は無変換(現状の一致セルを公式化)

operand E = 宣言 E なら値をそのまま伝搬する。構造化エラー(variant E)は
一致経路で match 可能性を完全に保つ。spec に「typed error の公式ルート」として明文化する。

### D2. E 不一致は方向を問わず check 時エラー + machine-hint(ADR-0002 D4 の確認)

規則は一文で尽きる: **`!` / auto-`?` は E を変換しない。** 不一致は新 E-code の
check エラーとし、hint が方向別の正準形を提示する:

```
// CustomE → String(上方向・損失あり)
error[E0xx]: `!` は err(ConfigError) を main の失敗チャネル(String)へ変換できない
  try: get_port(cfg) |> result.map_err((e) => "${e}")!

// String → CustomE(下方向)
error[E0xx]: `!` は err(String) を f の失敗チャネル(ConfigError)へ変換できない
  try: int.parse(s) |> result.map_err((e) => BadValue(e))!
```

下方向は String を運ぶ ctor が唯一なら ctor 名まで埋める。複数あれば列挙して選ばせる。
現行の暗黙 Debug 文字列化(§8-5)と check 素通り ICE(§8-4)は、この検査の実装で
両方とも消える。

### D3. 暗黙変換の唯一の許容条件は「無損失」— none→err("none") は維持

none はペイロードを持たないため、`err("none")` への埋め込みで**捨てられる情報が
存在しない**(無損失)。よって既存の暗黙は原理の例外ではなく適用例であり、維持する。
対して `NotFound("alice")` → `"NotFound alice"` は match 可能性を破壊する(損失あり)
ため、明示 `map_err` を要求する。この線引きが本 ADR の原理である。

### D4. From 型の暗黙変換フックは導入しない

Rust の `From`/`?` 相当(ユーザー定義変換の自動適用)は導入しない。
遠くの impl の有無で同じ `!` の意味が変わるのは予測可能性(LLM writability)を
下げる。dojo の MSR 計測で map_err ノイズが実害と示されたら、その証拠を持って
別 ADR で再考する(Falsifier 参照)。

### D5. 適用範囲は「伝搬点」で統一

明示 `!` と auto-`?` は同一機構として同一規則に従う。`?`(to-Option)と `??` は
E を破棄するので本 ADR の対象外。`try_*` family は E 多相(callback と戻りの E は
同一型変数)なので対象外。

## Rationale

### 損失のある変換は、次の修正者への情報である

MSR は**次の修正が生き残るか**の指標であり、読み手は将来の修正者(主に LLM)。
`|> result.map_err((e) => "${e}")` の 1 行は「この境界で構造が String に落ちる」を
その場に可視化する — 構造化エラーに依存する修正をこの下流に書いてはいけない、
という事実が grep 可能・レビュー可能になる。暗黙化はこの情報を型シグネチャの
突き合わせ(非局所)へ追いやる。

### 儀式の書き手コストは hint がほぼゼロにする

上方向の正準形は `|> result.map_err((e) => "${e}")` の**一綴りだけ**であり、
check エラーの hint がそれをそのまま供給する。LLM にとって「エラー文言を読んで
正準形を貼る」は最も成功率の高いクラスの修正で、変換方向を推測させる余地がない。
初稿はこれを「情報を運ばない儀式」と評価したが、運んでいるのは書き手への情報では
なく**読み手への情報**である、が討議の結論。

### 規則が一文になる

「`!` は E を変換しない」。方向別の場合分け(上は暗黙・下はエラー)より、
仕様・診断・教材・LLM プロンプトのすべてが短くなる。例外は D3 の無損失条件
1 つで、これは「変換」ではなく「無からの埋め込み」と説明できる。

### 先例調査 — 暗黙の広げ先が lossy な言語は存在しない

| 言語 | E 不一致時の伝搬 | ⊤ 相当 | ⊤ は構造を保つか |
|---|---|---|---|
| Rust | `From` impl があれば暗黙変換、なければ E0277 | `Box<dyn Error>` / `anyhow::Error` | ✅ downcast 可 |
| Swift (SE-0413) | `throws(E)` → `throws` は暗黙上方 | `any Error`(存在型) | ✅ `catch let e as E` で match 可 |
| Zig | superset の error set へ暗黙 coerce | `anyerror` | ✅ タグ保存・`switch` 可 |
| Go | 明示 wrap(`fmt.Errorf("…: %w", err)`) | `error` interface | ✅ `errors.As` / `errors.Is` |
| Kotlin/Java | throw は subtype 上方(例外機構) | `Throwable` | ✅ catch by type |
| Gleam / Elm / Haskell `Either` | 暗黙変換なし — 明示 `map_error` | 作らない | — |

共通線: **暗黙が許される変換は例外なく無損失**(From の全単射、存在型への上方、
superset coercion、subtype)であり、変換後も元の構造へ戻れる。「暗黙で文字列化」に
相当する先例は見つからなかった — Alternatives 1 は先例なき発明だった。⊤ を持たない
Gleam/Elm は明示 map_error 一択で、本 ADR の決定と同形(Gleam は型クラス不採用という
設計制約まで共通)。D3 の無損失原理は、各言語の暗黙変換が全て満たしている条件を
言語化したものである。

### 一致セルの完動が typed error の公式ルートを保証する

strict にしても失うものはない: ドメイン内は E を揃えて貫通(D1・実測で完動)、
境界でだけ明示変換。「stdlib を触る下請けは E=String で書き、境界 fn で変換する」
というイディオムが自然に導かれる。

## Alternatives — 検討して却下した案

1. **String を ⊤ とする暗黙上方変換(本 ADR 初稿・Swift SE-0413 型)**:
   `throws(E)` → `throws(any Error)` の暗黙上方変換と同型に、CustomE → String を
   暗黙許可し正準 repr で文字列化する案。魅力は main 近傍の糊コードから儀式が
   消えること。却下理由: (a) 損失のある変換が、まさに次の修正者が見るべき場所で
   不可視になる(Rationale 1 の裏返し)。(b) E=String 戻りが最小抵抗経路になり、
   typed error を貫通させ損ねた fn(`-> Result[User, String]` のうっかり)が黙って
   通る — strict なら check エラーが「E を決めろ」と迫る。(c) 書き手コストの差は
   hint 供給でほぼ消えるため、可視性を売る対価が小さい。Swift の ⊤ は match 可能な
   `any Error`(存在型)であり構造を失わないが、almide の String 化は非可逆 —
   同型ではなかった、が決定的な差。
2. **Rust 流 From フック**: 予測可能性を優先して却下(証拠が出たら再考、D4)。
3. **指定 variant への自動 wrap**(String を運ぶ case へ自動注入): 候補が複数ある
   型で曖昧、型定義の変更で挙動が変わる。hint で ctor を提案する(D2)に留める。却下。
4. **変換子付き演算子**(`x!(Io)` 等): map_err で書けるものの糖衣に新構文を払う
   価値がない。却下。
5. **Zig 流 error set union**(E1 ∪ E2 を推論): エラー型が集合として合成される
   世界観ごと持ち込む必要があり、Result[T, E] の単純さを失う。却下。

## Consequences

- 仕様が一文になり、E 型マトリクスの 3 セルが「一致=公式ルート / 不一致=エラー+hint」
  の 2 状態に畳まれる。§8-4(ICE)・§8-5(暗黙 Debug 劣化)は同じ検査で消える。
- main 近傍の境界越え(CLI で 5〜15 箇所程度)に `|> result.map_err((e) => "${e}")`
  が明示される。これはコストではなく境界の可視化として払う。
- **互換性**: 現行の暗黙 Debug 変換に依存していたコードは check エラー化する
  (挙動が SURPRISE 級だったため実害はほぼ無いと推定するが、リリースノートに
  移行 hint を明記する)。出力形式の契約変更は発生しない — 暗黙変換自体が消えるため。
- none→err("none") は無損失原理の適用例として現状維持(D3)。文言の再考
  (「none」より情報のあるメッセージ)は本 ADR の範囲外。

## Falsifier

1. **dojo の MSR 計測で、map_err 儀式そのものが有意な修正失敗源**(方向の取り違え、
   `"${e}"` の書き損じ等)**と示された場合** — hint 供給で防げていない証拠なので、
   Alternatives 1(暗黙上方変換)を新 ADR で再考する。
2. **実コーパスで境界儀式が支配的**(可謬 fn の相当割合が map_err 行を持つ等)
   **と計測された場合** — D4 を撤回し From 型フック、または上方変換の ADR を起こす。
3. **無損失判定が曖昧な変換が現れた場合**(D3 の線引きで分類できない第三のケース)—
   原理の定義から改訂する。

## References

- 実測: 2026-08-05 matrix の `bang/pure-Result-CustomE-fn/*` ・
  `bang/effect-fn->T/CustomE-operand` 各セル(v0.53.6、probe 保存済み)
- Swift Evolution SE-0413 — [Typed throws](https://github.com/swiftlang/swift-evolution/blob/main/proposals/0413-typed-throws.md)
  (Alternatives 1 の比較対象 — `any Error` は match 可能であり almide の String 化とは非同型)
- Rust — [`std::convert::From` and `?`](https://doc.rust-lang.org/std/convert/trait.From.html)、
  [The question mark operator](https://doc.rust-lang.org/reference/expressions/operator-expr.html#the-question-mark-operator)
- Zig — [Error Set Type](https://ziglang.org/documentation/master/#Error-Set-Type)・
  [anyerror](https://ziglang.org/documentation/master/#The-Global-Error-Set)(無損失 coercion の例)
- Go — [Error wrapping: `%w`, `errors.Is` / `errors.As`](https://go.dev/blog/go1.13-errors)(明示 wrap + 構造保持)
- Gleam — [`gleam/result.map_error`](https://hexdocs.pm/gleam_stdlib/gleam/result.html#map_error)
  (⊤ なし・明示変換一択の同形先例)
- 内部: ADR-0002 D4(本 ADR が確認・詳細化)、C-211、`!` の none→err("none")(D3 の適用例)
