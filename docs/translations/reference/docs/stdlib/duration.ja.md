# duration

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](duration.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](duration.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](duration.zh-CN.md)



> 原文：[duration.md @ 35297a6](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/duration.md)。読者向けの非規範的な固定スナップショットです。API 索引は原文固定時点の内容であり、この訳は自動生成・自動同期の対象ではありません。署名・型・`@since` とコード例（コメントを含む）は原文のまま保持し、API 説明の訳を末尾に併記しています。コード例はこの翻訳作業では独立実行していません。

実時間のコンストラクター。自動的に利用できます（検査器の公開機能であり、import 不要、標準ライブラリモジュールではありません）。規範的な規則：[ADR-0001](../../../../adr/0001-deterministic-time-units.md)。

`Duration` 値は**実時間**の量です。プログラムが実行されるマシン上でストップウォッチが測る時間を表します。これは oracle 層の時計であり、`fan.timeout(duration.ms(n)) { body }` が資源使用量を計上する箇所で協調的に検査します（エフェクト面の期限にも同じ型を予約していますが、将来の機能です）。決定的な層は別の時計 [compute](../../../../stdlib/compute.md) を読みます。2つの型は混ざらず、相互変換もありません。この隔壁により、両者は馴染みのある `ms` という語彙を安全に共有できます。

単位の集合は `ns / us / ms / s / min / h` に限定されています。時間リテラルはなく（`5s` は解析できません）、単なる `Int` は時間ではありません。

### `duration.ns(n: Int) -> Duration` … `duration.h(n: Int) -> Duration`

```almd
duration.ms(5000)  // the conventional timeout spelling
duration.s(30)
duration.min(10)
```

負の引数は決定的な実行時中止になります。生成時にオーバーフローすると、表現可能な最大時間に飽和します。

<a id="algebra-adr-0001-s3"></a>

### 代数演算（ADR-0001 S3）

同じ時計の範囲内では `Compute` と同じ操作が使えます。

```almd
duration.s(1) + duration.ms(500)   // Duration — saturating add
duration.s(2) - duration.s(3)      // Duration — saturates at 0
duration.ms(100) * 3               // Duration — scale by Int
duration.ms(1) < duration.s(1)     // Bool
```

どの演算子でも `Duration ⊕ Compute` は型エラーです。実時間の期限が決定的な予算へ入り込むことはありません（`fan.bounded(duration.ms(5))` は拒否され、`compute.ms(...)` / `fan.timeout` が案内されます）。



<a id="signature-index-7-functions"></a>

## 固定スナップショットの API 索引（7関数）

```
// n nanoseconds of wall-clock time; negative n aborts.
// @since 0.51.0 or earlier
duration.ns(n: Int) -> Duration

// n microseconds of wall-clock time; negative n aborts.
// @since 0.51.0 or earlier
duration.us(n: Int) -> Duration

// n milliseconds of wall-clock time; negative n aborts.
// @since 0.51.0 or earlier
duration.ms(n: Int) -> Duration

// n seconds of wall-clock time; negative n aborts.
// @since 0.51.0 or earlier
duration.s(n: Int) -> Duration

// n minutes of wall-clock time; negative n aborts.
// @since 0.51.0 or earlier
duration.min(n: Int) -> Duration

// n hours of wall-clock time; negative n aborts.
// @since 0.51.0 or earlier
duration.h(n: Int) -> Duration

// Negative if a < b, 0 if equal, positive if a > b.
// @since unreleased
duration.compare(a: Duration, b: Duration) -> Int
```




## API 説明の訳（上記署名順）

- `duration.ns(n: Int) -> Duration` — 実時間で n ナノ秒。n が負なら中止。
- `duration.us(n: Int) -> Duration` — 実時間で n マイクロ秒。n が負なら中止。
- `duration.ms(n: Int) -> Duration` — 実時間で n ミリ秒。n が負なら中止。
- `duration.s(n: Int) -> Duration` — 実時間で n 秒。n が負なら中止。
- `duration.min(n: Int) -> Duration` — 実時間で n 分。n が負なら中止。
- `duration.h(n: Int) -> Duration` — 実時間で n 時間。n が負なら中止。
- `duration.compare(a: Duration, b: Duration) -> Int` — a < b なら負、等しければ0、a > b なら正。
