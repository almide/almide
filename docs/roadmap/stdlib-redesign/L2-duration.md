<!-- description: Stdlib redesign L2/L3 — duration: the six wall-clock constructors stay as ADR-0001 fixed them; add compare as the function form of its Ord, keep readers and Float constructors omitted -->
# L2/L3: duration

> Last updated: 2026-10-21
> 前提: [L0](L0-conventions.md)・[L1](L1-categories.md)（R-L1-21: `compute` と統合しない）・[ADR-0001](../../adr/0001-deterministic-time-units.md)
> 関連: [L2-compute](L2-compute.md)・[L2-datetime](L2-datetime.md)
> 出典: Rust https://doc.rust-lang.org/std/time/struct.Duration.html ・ Go https://pkg.go.dev/time#Duration ・ Python https://docs.python.org/3/library/datetime.html#timedelta-objects ・ TS/JS https://tc39.es/proposal-temporal/docs/duration.html ・ Gleam https://hexdocs.pm/gleam_time/gleam/time/duration.html（公式 package） ・ Elixir https://hexdocs.pm/elixir/Duration.html ・ OCaml https://ocaml.org/manual/latest/api/Unix.html（標準に Duration の型はない） ・ Kotlin https://kotlinlang.org/api/core/kotlin-stdlib/kotlin.time/-duration/ ・ Swift https://developer.apple.com/documentation/swift/duration

`duration` は ADR-0001 の規範どおりの閉じた面です: `ns` `us` `ms` `s` `min` `h` の 6 つの構築子、負の値は決定的な abort、構築時の桁あふれは飽和、演算は `+`・`-`（0 で飽和）・`* Int`・同じ型どうしの比較だけ（S3）。数値を読み出す関数（`in_ms` など）、`Float` の構築子、日以上の単位は、S7 で再開条件つきの意図的な省略です。2026-10-16 の裁定で、読み出し関数を足す案は撤回されています（表示は `Repr`）。

## L2: 役割の比較

| 役割 | Almide | Rust | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin | Swift |
|---|---|---|---|---|---|---|---|---|---|---|
| 単位から作る | `ns` `us` `ms` `s` `min` `h` | `from_nanos` `from_millis` `from_secs` 等 | `n * time.Millisecond` | `timedelta(milliseconds=n)` | `Temporal.Duration.from` | pkg `duration.milliseconds` | `Duration.new!` | — | `n.milliseconds` | `.milliseconds(n)` |
| 日以上の単位 | —（S7） | — | — | `days=` | `days` | — | `day:` | — | `n.days` | — |
| 小数から作る | —（S7） | `from_secs_f64` | — | `seconds=1.5` | — | — | — | — | `1.5.seconds` | `.seconds(1.5)` |
| 足す・引く・整数倍 | `+` `-` `* Int` | `+` `-` `*` | `+` `-` `*` | `+` `-` `*` | `add` | pkg `add` | `Duration.add` | — | `+` `-` `*` | `+` `-` `*` |
| 比較 | `<` `==` など（`Ord`） | `<` `cmp` | `<` | `<` | `Duration.compare` | pkg `compare` | `Duration` の比較関数なし | — | `<` `compareTo` | `<` |
| 数値で読み出す | —（S7、表示は `Repr`） | `as_millis` | `Milliseconds()` | `total_seconds()` | `total` | pkg `to_seconds` | `to_timeout` | — | `inWholeMilliseconds` | `components` |
| 時計が違う量との分離 | `Compute` と混ぜると型エラー | — | — | — | — | — | — | — | — | `Clock` ごとの `Instant` |

### 完全性の規則

1. ADR-0001 S2: 2 つの時計 × 6 単位の 12 セル（`duration` の 6 と [compute](L2-compute.md) の 6）がそろって存在します。単位は閉じた集合です。
2. ADR-0001 S3: 演算は演算子で書き、`Duration` と `Compute` は混ぜられず、変換する関数はありません。
3. R-L0-16（順序のある型は `compare` を持つ）: `Duration` は `Ord` を導出しているので、`duration.compare(a, b) -> Int` を置きます。`compute.compare` と同じ形です。これは `<` の関数の形で、量を数値として読み出す関数ではないので、S7 の省略と両立します。
4. 意図的な省略（S7 の台帳）: 読み出し関数、`Float` の構築子、日以上の単位、`T / Int`、`min`・`max`。

### L2 の判定

- 抜けている役割: 読み出し（7/9）、小数の構築子（4/9）、日の単位（5/9）は、ADR-0001 S7 の再開条件が満たされていないので足しません（R-duration-02 ×）。
- 抜けているセル: `compare`（R-L0-16）→ R-duration-01。
- 重複: なし。`duration.ms` と `compute.ms` は型が違う別の値です（#3851 の「Dropped」のとおり）。

## 案

### R-duration-01 `duration.compare` を足す — ○ 採用

- 内容: `duration.compare(a: Duration, b: Duration) -> Int`（`a < b` で負、等しければ 0、`a > b` で正）。
- 根拠の L0 規則: R-L0-16（順序のある型は `<mod>.compare(a, b) -> Int` を持つ）、R-L0-02。
- (a) Rust `Ord::cmp`、Kotlin `compareTo`、Temporal `Duration.compare`。
- (b) 純粋な計算です。ナノ秒の `Int` 1 本どうしの比較なので、両 target で同じです。
- (c) `list.sort_with(ds, duration.compare)` のように、比較関数を受け取る関数にそのまま渡せます。`Int` の `int.compare` と同じ綴りなので、規則から当たります。
- (d) 追加。使用数 0。ADR-0001 S2 の 12 セルのゲート（S6-1）は構築子だけを数えるので、変わりません。
- ゲート案: すべての `Ord` を導出する標準の型について `compare` の存在を検査するゲート（R-L0-16のゲート）に、`Duration` と `Compute` を入れます。
- issue: #3902

### R-duration-02 読み出し関数・小数の構築子・日の単位を足す — × 不採用

- 内容: 足しません。ADR-0001 S7 の台帳のとおりで、2026-10-16 の裁定（読み出し関数を足す案の撤回）とも同じです。
- 根拠: ADR-0001 S7 の再開条件（「予算を計算に使う実例」「Dojo で整数換算の書き誤りが観測されたら」「実需要の記録」）が、まだ満たされていません。L1 の追加基準は「既存の裁定でまかなえない」ことを求めます。
- issue: なし

### R-duration-03 `Duration` と `Compute` を 1 つのプロトコル `TimeAmount` に従わせる — ○ 採用

- いま: `Duration`（壁時計の時間）と `Compute`（決定的な計算時間）は、同じ 6 単位の構築子・`compare`・演算子を持っています。ただし「同じ面を持つ」ことは ADR-0001 S1 の文章と、型ごとに複製した行列のゲートで守っているだけで、言語の宣言にはなっていません。
- 案: 標準にプロトコルを 1 つ置き、2 つの型をそれに従わせます。
  ```almide
  protocol TimeAmount {
    fn ns(n: Int) -> Self
    fn us(n: Int) -> Self
    fn ms(n: Int) -> Self
    fn s(n: Int) -> Self
    fn min(n: Int) -> Self
    fn h(n: Int) -> Self
    fn compare(a: Self, b: Self) -> Int
  }
  type Duration: Eq, Ord, Repr, TimeAmount
  type Compute: Eq, Ord, Repr, TimeAmount
  ```
  - 2 つの型の違いは「どの時計の量か」だけで、使い方は同じ、という関係が宣言 1 か所で読めます。
  - 型は別のままです（R-L1-21・ADR-0001 S3: 混ぜると型エラー、変換関数なし）。プロトコルは「同じ形をしている」ことを表すだけで、片方をもう片方として渡せるようにはしません。
  - 構築子と `compare` は、いまのモジュール関数（`duration.ms`、`compute.compare` など）がそのままプロトコルのメソッドになります。同じ計算の転送の関数を別に置きません（R-L0-01）。
  - 演算子（`+`・`-`・`* Int`・`<`）は、ADR-0001 S3 の行列のまま両方の型に置きます。プロトコルはメソッドしか持てないので、ここには入りません。
  - 時計を選ばない関数を、利用者が 1 つ書けば両方に使えます（`fn half_or_less[T: TimeAmount](a: T, limit: T) -> T`）。
- 根拠の L0 規則: R-L0-01（同じ役割を 2 か所で手書きしない）。CLAUDE.md の「性質は宣言 1 か所から導出する」。
- 用語の注意: `Compute` は OS が測る CPU 時間ではありません。凍結した抽象機械の charge の数（関数の入口とループの頭で数える）で、どのホストでも同じ値になる決定的な量です（ADR-0001）。壁時計と対になる「もう 1 つの時計」です。
- (a) Swift の `Clock` プロトコルは、時計ごとに「`DurationProtocol` に従う量の型」を決められる形です（[DurationProtocol](https://developer.apple.com/documentation/swift/durationprotocol)。標準の `ContinuousClock` と `SuspendingClock` は、どちらも同じ `Duration` 型を使います）。Java の `TemporalAmount`（`Duration` と `Period` が従う）も、量の型を 1 つのインターフェースでまとめる考え方です。ほかの言語は、時計の違いを型で分けていません。
- (b) 検査器だけの変更です。値の表現（ナノ秒の Int 1 本）は変わりません。
- (c) `duration.ms(5)` を覚えれば `compute.ms(5)` も同じ形で書ける、という対応が、文書の約束ではなく型の宣言として見えます。片方にだけ関数が足される、というずれも、プロトコルの検査で起きなくなります。
- (d) 追加です（新しいプロトコル 1 つと、2 つの型の宣言）。既存のコードは変わりません。名前の `TimeAmount` は、どの言語の綴りとも衝突せず、「時間の量」と読めることで選びました。
- 完全性の規則: `TimeAmount` に従う型は、6 単位の構築子と `compare` をすべて持ちます（プロトコルの検査が強制します）。ADR-0001 S2 の 12 セルの行列ゲートは、このプロトコルの検査に置き換わります。
- ゲート案: 2 つの型が `TimeAmount` に従うことを、`spec/stdlib/` の総称関数のテスト（同じ総称関数を `Duration` と `Compute` の両方で呼ぶ）で確かめます。
- issue: #3964

### R-duration-04 `min`（分）を `minutes` などの完全な綴りにする — × 不採用

- 理由: 6 つの単位の綴りは ADR-0001 S2 が決めています。`duration.min(a, b)` と最小値のつもりで書くと引数の数が合わず E004 になるので、黙って違う値にはなりません。
- issue: なし

## L3: 関数表

`duration` の現行 6 関数と、追加する 1 関数です。

| 関数 | シグネチャ(現行, mut 表示) | 他言語の主な綴り | 判定 | 案 |
|---|---|---|---|---|
| `ns` | `duration.ns(n: Int) -> Duration` | Rust `from_nanos` / Go `time.Nanosecond` / Kotlin `nanoseconds` / Swift `.nanoseconds` | 据え置き | — |
| `us` | `duration.us(n: Int) -> Duration` | Rust `from_micros` / Go `time.Microsecond` / Python `microseconds=` / Kotlin `microseconds` | 据え置き | — |
| `ms` | `duration.ms(n: Int) -> Duration` | Rust `from_millis` / Go `time.Millisecond` / Python `milliseconds=` / Kotlin `milliseconds` / Swift `.milliseconds` | 据え置き | — |
| `s` | `duration.s(n: Int) -> Duration` | Rust `from_secs` / Go `time.Second` / Python `seconds=` / Kotlin `seconds` / Swift `.seconds` | 据え置き | — |
| `min` | `duration.min(n: Int) -> Duration` | Rust `from_mins` / Go `time.Minute` / Python `minutes=` / Kotlin `minutes` | 据え置き | — |
| `h` | `duration.h(n: Int) -> Duration` | Rust `from_hours` / Go `time.Hour` / Python `hours=` / Kotlin `hours` | 据え置き | — |
| `compare` | （追加）`duration.compare(a: Duration, b: Duration) -> Int` | Rust `cmp` / Kotlin `compareTo` / Temporal `Duration.compare` | 追加 | R-duration-01 |
