# compute

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](compute.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](compute.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](compute.zh-CN.md)



> 原文：[compute.md @ 35297a6](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/compute.md)。非規範的な読者向け固定スナップショットです。自動生成・自動同期されません。署名・コード例・API 索引のコメントは原文を保持し、説明の訳を末尾に併記しています。計測・動作・例をこの翻訳作業で独立に再検証していません。
>
> 原文上の注意: 決定的計算時間は実時間・期限の保証ではありません。規範的定義はリンク先 ADR の原文であり、この固定訳自体を新たな規範とするものではありません。

決定的な計算時間のコンストラクタ。自動利用可能です（checker の表層機能。import 不要で、stdlib module ではありません）。規範的な規則は [ADR-0001](../../../../adr/0001-deterministic-time-units.md) を参照してください。

`Compute` 値は**決定的な計算時間**の量です。固定された抽象機械上で、プログラム自身の仕事を計測します（関数入口とループ先頭で単位を課金）。そのため予算判定はプログラムと入力だけの関数となり、全ターゲット・全ホストで同じです。これが `fan.bounded` の読む時計であり、`fan.min(compute…)` が選択に使う時計です（`fan.min` の時計引数は必須。その型が何に対する「最小」かを示します）。実時間は別の時計です。[duration](../../../../stdlib/duration.md) を参照してください。

単位集合は `ns / us / ms / s / min / h` に限定されます。時間リテラルはなく（`100ms` は解析できません）、裸の `Int` が時間になることもありません。

### `compute.ns(n: Int) -> Compute` … `compute.h(n: Int) -> Compute`

```almd run
fn spin(n: Int) -> Int = {
  var i = 0
  while i < n {
    i = i + 1
  }
  42
}

effect fn main() -> Unit = {
  let a = compute.ns(1500)   // nanoseconds
  let b = compute.us(50)     // microseconds
  let c = compute.ms(100)    // milliseconds
  let d = compute.s(2)       // seconds
  let e = compute.min(5)     // minutes
  let f = compute.h(1)       // hours
  // every unit is the same clock: the sum is a budget fan.bounded reads
  println(int.to_string(fan.bounded(a + b + c + d + e + f) { spin(1000) } ?? -1))
}
```
```output
42
```

負の引数は決定的な実行時 abort になります（`Error: negative time: compute.us(-5)`、exit 1。両ターゲットで同一）。overflow する構築は表現可能な最大時間へ飽和します。

### 代数（ADR-0001 S3）

```almd run
fn spin(n: Int) -> Int = {
  var i = 0
  while i < n {
    i = i + 1
  }
  42
}

effect fn main() -> Unit = {
  let sum = compute.ms(2) + compute.ms(3)      // Compute — saturating add
  let diff = compute.ms(5) - compute.ms(2)     // Compute — saturates at 0 (never negative)
  let scaled = compute.ms(2) * 3               // Compute — scale by Int (either order);
                                               // a negative factor aborts
  let less = compute.ms(2) < compute.ms(3)     // Bool — same-clock comparison
  let floor = compute.ms(1) - compute.ms(2)    // saturated to 0: admits no loop
  println(int.to_string(fan.bounded(sum) { spin(1000) } ?? -1))
  println(int.to_string(fan.bounded(diff) { spin(1000) } ?? -1))
  println(int.to_string(fan.bounded(scaled) { spin(1000) } ?? -1))
  println(int.to_string(fan.bounded(3 * compute.ms(2)) { spin(1000) } ?? -1))
  println(int.to_string(fan.bounded(floor) { spin(1000) } ?? -1))
  println(if less then "less" else "not less")
}
```
```output
42
42
42
42
-1
less
```

`T * T`（時間 × 時間）、`Compute` を `Duration` または裸の `Int` と混ぜる操作、`/` は型エラーです。各セルを閉じている理由は ADR を参照してください。

### 利用側

```almd run
fn work(n: Int) -> Int = n * 2

fn fast() -> Int = 1

fn slow() -> Int = {
  var i = 0
  while i < 100000 {
    i = i + 1
  }
  2
}

effect fn main() -> Unit = {
  let input = 21
  let fallback = -1
  let r = fan.bounded(compute.ms(100)) { work(input) } ?? fallback
  let w = fan.min(compute.us(50)) { fast(), slow() } ?? fallback
  println(int.to_string(r))
  println(int.to_string(w))
}
```
```output
42
1
```

`almide run --time-report` は計測した実時間の横にプログラムの決定的時間を表示します（`time: 0.15ms deterministic (≈38.7ms wall here)`）。二つは異なる時計で、同じ量であるとは主張しません。



## 固定スナップショットの API 索引 (7 関数)

```
// n nanoseconds of deterministic compute time; negative n aborts.
// @since 0.51.0 or earlier
compute.ns(n: Int) -> Compute

// n microseconds of deterministic compute time; negative n aborts.
// @since 0.51.0 or earlier
compute.us(n: Int) -> Compute

// n milliseconds of deterministic compute time; negative n aborts.
// @since 0.51.0 or earlier
compute.ms(n: Int) -> Compute

// n seconds of deterministic compute time; negative n aborts.
// @since 0.51.0 or earlier
compute.s(n: Int) -> Compute

// n minutes of deterministic compute time; negative n aborts.
// @since 0.51.0 or earlier
compute.min(n: Int) -> Compute

// n hours of deterministic compute time; negative n aborts.
// @since 0.51.0 or earlier
compute.h(n: Int) -> Compute

// Negative if a < b, 0 if equal, positive if a > b.
// @since unreleased
compute.compare(a: Compute, b: Compute) -> Int
```



## API 説明の訳（上記署名順）

- `compute.ns(n: Int) -> Compute` — 決定的な計算時間の n ナノ秒。負の n は abort。
- `compute.us(n: Int) -> Compute` — 決定的な計算時間の n マイクロ秒。負の n は abort。
- `compute.ms(n: Int) -> Compute` — 決定的な計算時間の n ミリ秒。負の n は abort。
- `compute.s(n: Int) -> Compute` — 決定的な計算時間の n 秒。負の n は abort。
- `compute.min(n: Int) -> Compute` — 決定的な計算時間の n 分。負の n は abort。
- `compute.h(n: Int) -> Compute` — 決定的な計算時間の n 時間。負の n は abort。
- `compute.compare(a: Compute, b: Compute) -> Int` — a < b なら負、等しければ 0、a > b なら正。
