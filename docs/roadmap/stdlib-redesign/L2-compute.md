<!-- description: Stdlib redesign L2/L3 — compute: the six deterministic compute-time constructors stay as ADR-0001 fixed them; add compare; no counterpart exists in the nine languages -->
# L2/L3: compute

> Last updated: 2026-10-21
> 前提: [L0](L0-conventions.md)・[L1](L1-categories.md)（R-L1-21: `duration` と統合しない）・[ADR-0001](../../adr/0001-deterministic-time-units.md)・[ADR-0028](../../adr/0028-fan-selection-names-and-cost-contract.md)
> 関連: [L2-duration](L2-duration.md)
> 出典: Rust https://doc.rust-lang.org/std/time/struct.Duration.html ・ Go https://pkg.go.dev/time#Duration ・ Python https://docs.python.org/3/library/time.html#time.process_time ・ TS/JS https://tc39.es/proposal-temporal/docs/duration.html ・ Gleam https://hexdocs.pm/gleam_time/ ・ Elixir https://hexdocs.pm/elixir/Process.html#info/2（reductions） ・ OCaml https://ocaml.org/manual/latest/api/Sys.html（`Sys.time` は CPU 時間） ・ Kotlin https://kotlinlang.org/api/core/kotlin-stdlib/kotlin.time/-duration/ ・ Swift https://developer.apple.com/documentation/swift/clock

`Compute` は「決定的な計算時間」の量で、凍結した抽象機械の charge（関数の入口とループの頭）で数えます。予算の判定は、プログラムと入力だけで決まり、どの target・どのホストでも同じです。`fan.bounded` が読み、`fan.min(compute…)` が選ぶ時計です。9 言語に同じものはありません（[facts/categories.md](facts/categories.md) 3.3 節: 近いのは BEAM の reductions 数と CPU 時間ですが、どちらも予算の型ではありません）。

## L2: 役割の比較

| 役割 | Almide | Rust | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin | Swift |
|---|---|---|---|---|---|---|---|---|---|---|
| 決定的な計算時間の量 | `ns` `us` `ms` `s` `min` `h` | — | — | —（`process_time` は CPU 時間で非決定） | — | — | —（reductions は数で、型ではない） | —（`Sys.time` は CPU 時間） | — | — |
| 足す・引く・整数倍・比較 | `+` `-` `* Int` `<`（`Ord`） | — | — | — | — | — | — | — | — | — |
| 予算として使う | `fan.bounded(c)`、`fan.min(c, …)` | — | — | — | — | — | — | — | — | — |
| 実時間との混用 | 型エラー（S3） | — | — | — | — | — | — | — | — | `Clock` ごとに `Instant` を分ける |
| 数値で読み出す | —（S7） | — | — | — | — | — | — | — | — | — |

### 完全性の規則

[L2-duration](L2-duration.md) の規則 1〜4 と同じです（ADR-0001 の S2・S3・S7 は、2 つの時計に同じ面を求めています）。`compare` も `duration` と同時に置きます。

### L2 の判定

- 9 言語に対応がないことは、L1 R-L1-21 で「Almide 固有の理由（決定性の約束）があるので多数派に合わせない」と判定済みです。
- 抜けているセル: `compare`（R-L0-16）→ R-compute-01。
- 重複: なし。

## 案

### R-compute-01 `compute.compare` を足す — ○ 採用

- 内容: `compute.compare(a: Compute, b: Compute) -> Int`。
- 根拠の L0 規則: R-L0-16（順序のある型は `compare` を持つ）。ADR-0001 S1 の「両型は同じ演算面を持つ」により、`duration.compare` と対で足します。
- (b) 純粋な計算です。charge の数は決定的なので、比較の結果も両 target で同じです。
- (c) 予算の一覧を並べるときに `list.sort_with(cs, compute.compare)` と書けます。
- (d) 追加。使用数 0。
- ゲート案: R-duration-01 と同じゲート（`Ord` を導出する標準の型に `compare`）。
- issue: #3902

### R-compute-02 `duration` と統合する、または読み出し関数を足す — × 不採用

- 内容: どちらもしません。ADR-0001 の決定（2 つの時計は互いに変換できない別の型、読み出しは S7 で省略）と、L1 R-L1-21 のとおりです。UFCS の `n.ms()` を曖昧エラーにしているのも、この分離を守るためです（S2）。
- issue: なし

## L3: 関数表

`compute` の現行 6 関数と、追加する 1 関数です。

| 関数 | シグネチャ(現行, mut 表示) | 他言語の主な綴り | 判定 | 案 |
|---|---|---|---|---|
| `ns` | `compute.ns(n: Int) -> Compute` | —（9 言語に対応なし。実時間なら Rust `Duration::from_nanos`） | 据え置き | — |
| `us` | `compute.us(n: Int) -> Compute` | —（実時間なら Rust `from_micros`） | 据え置き | — |
| `ms` | `compute.ms(n: Int) -> Compute` | —（実時間なら Rust `from_millis`、Go `time.Millisecond`） | 据え置き | — |
| `s` | `compute.s(n: Int) -> Compute` | —（実時間なら Rust `from_secs`） | 据え置き | — |
| `min` | `compute.min(n: Int) -> Compute` | —（実時間なら Rust `from_mins`） | 据え置き | — |
| `h` | `compute.h(n: Int) -> Compute` | —（実時間なら Rust `from_hours`） | 据え置き | — |
| `compare` | （追加）`compute.compare(a: Compute, b: Compute) -> Int` | Rust `cmp` / Kotlin `compareTo` | 追加 | R-compute-01 |
