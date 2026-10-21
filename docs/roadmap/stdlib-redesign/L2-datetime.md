<!-- description: Stdlib redesign L2/L3 — datetime: Int Unix seconds stay the representation; add now_ms as the one millisecond clock, retire the identity and operator wrappers -->
# L2/L3: datetime

> Last updated: 2026-10-21
> 前提: [L0](L0-conventions.md)・[L1](L1-categories.md)（R-L1-25: 時計は `datetime` が持つ。タイムゾーンは標準に入れない）
> 関連: [L2-duration](L2-duration.md)・[L2-compute](L2-compute.md)・L2-env
> 出典: Rust https://doc.rust-lang.org/std/time/struct.SystemTime.html ・ Go https://pkg.go.dev/time ・ Python https://docs.python.org/3/library/datetime.html と https://docs.python.org/3/library/time.html ・ TS/JS https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/Date ・ Gleam https://hexdocs.pm/gleam_time/（公式 package。標準にはない） ・ Elixir https://hexdocs.pm/elixir/DateTime.html ・ OCaml https://ocaml.org/manual/latest/api/Unix.html ・ Kotlin https://kotlinlang.org/api/core/kotlin-stdlib/kotlin.time/-instant/ ・ Swift https://developer.apple.com/documentation/foundation/date

各言語の時間の置き場所は [facts/categories.md](facts/categories.md) 1.G・3.3 節にあります（Kotlin `kotlin.time.Instant` と WASI の clocks は (F) 済み）。

実測（`almide 0.68.0 dev 437b8f127`、native と wasm で同じ）: `datetime.weekday(0)` は `"Thursday"`。`datetime.from_parts(2024, 13, 1, 0, 0, 0)` は `2025-01-01T00:00:00Z`（はみ出した月は繰り上がる）。`datetime.format(0, "%Y %%")` は `"1970 %%"`（`%%` を `%` にしない）。`parse_iso("2024-01-01T09:00:00+09:00")` は `1704067200`（オフセットを正しく引く）、小数秒は err。`datetime.from_unix(5)` は `5`（何もしない関数）。

## L2: 役割の比較

| 役割 | Almide | Rust | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin | Swift |
|---|---|---|---|---|---|---|---|---|---|---|
| 時刻の表現 | `Int`（Unix 秒） | `SystemTime` | `time.Time` | `datetime` / `float` 秒 | `Date`（ms） | pkg `Timestamp` | `DateTime` | `float`（`Unix.time`） | `Instant` | `Date` |
| 現在時刻（秒） | `now()` と `env.unix_timestamp()` | `SystemTime::now()` | `time.Now().Unix()` | `time.time()` | — | pkg | `System.os_time(:second)` | `Unix.time` | `Clock.System.now()` | `Date()` |
| 現在時刻（ms） | `env.millis()` | `duration_since(UNIX_EPOCH)` | `UnixMilli()` | `time.time_ns()` | `Date.now()` | pkg | `System.os_time(:millisecond)` | — | `toEpochMilliseconds()` | `timeIntervalSince1970` |
| 単調時計 | `monotonic_ns()` | `Instant::now()` | `time.Now()`（単調つき） | `time.monotonic_ns()` | `performance.now()` | pkg | `System.monotonic_time` | — | `TimeSource.Monotonic` | `ContinuousClock` |
| 部品から組み立てる | `from_parts` | — | `time.Date` | `datetime(...)` | `Date.UTC` | pkg | `DateTime.new` | `Unix.mktime` | `LocalDateTime(...)` | `DateComponents` |
| 部品を取り出す | `year` `month` `day` `hour` `minute` `second` `weekday` | — | `Year()` 等 | `.year` 等 | `getUTCFullYear()` 等 | pkg | `.year` 等 | `Unix.gmtime` | `.year` 等 | `Calendar.component` |
| 曜日 | `weekday`（英語の名前の String） | — | `Weekday()`（数、`String()` で名前） | `weekday()`（0 = 月） | `getUTCDay()`（0 = 日） | pkg | `Date.day_of_week`（1 = 月） | `tm_wday`（0 = 日） | `dayOfWeek`（列挙） | 数（1 = 日） |
| ISO 8601 の読み書き | `parse_iso` `to_iso` | — | `Parse(RFC3339)` `Format` | `fromisoformat` `isoformat` | `toISOString` `Date.parse` | pkg | `from_iso8601` `to_iso8601` | — | `Instant.parse` `toString` | `ISO8601DateFormatter` |
| 書式 | `format(ts, "%Y-%m-%d")` | — | `Format("2006-01-02")` | `strftime` | `Intl` | pkg | `Calendar.strftime` | — | `format` | `DateFormatter` |
| 時間を足す・引く | `add_days` `add_hours` `add_minutes` `add_seconds`、`diff_seconds` | `+ Duration` | `Add` `Sub` | `+ timedelta` | 算術 | pkg | `DateTime.add` `diff` | 算術 | `+ Duration` | `addingTimeInterval` |
| 前後の比較 | `is_after` `is_before` | `<` | `After` `Before` | `<` | `<` | pkg | `DateTime.compare` | `<` | `<` | `<` |
| Unix 秒との変換 | `from_unix` `to_unix`（何もしない） | `UNIX_EPOCH +` | `time.Unix` `.Unix()` | `fromtimestamp` `timestamp` | `new Date(ms)` `getTime` | pkg | `from_unix` `to_unix` | — | `fromEpochSeconds` `epochSeconds` | `Date(timeIntervalSince1970:)` |
| 月を足す（暦の計算） | — | — | `AddDate` | — | `setUTCMonth` | pkg | `Date.shift` | — | `plus(DateTimePeriod)` | `Calendar.date(byAdding:)` |
| タイムゾーン | —（UTC と固定オフセットだけ） | — | `LoadLocation` | `zoneinfo` | `Intl` | — | 外部 db | — | pkg | `TimeZone` |

### 完全性の規則

1. **表現**: 時刻は `Int` の Unix 秒（UTC）です。ミリ秒の時刻は `now_ms()` だけが返し、`datetime` のほかの関数はすべて秒を取ります。
2. **時計の持ち分**（R-L1-25）: 実時間（秒・ミリ秒）と単調時計は `datetime` だけが持ちます。`now`（秒）、`now_ms`（ミリ秒）、`monotonic_ns`（ナノ秒、差だけに意味がある）の 3 つで、名前の接尾辞が単位を表します。
3. **部品の対**: `from_parts` で組み立てられる 6 つの部品（年・月・日・時・分・秒）には、それぞれ取り出す関数があります（`year` … `second`）。
4. **足し算の族**: `add_<単位>` は `days`・`hours`・`minutes`・`seconds` の 4 つで閉じます。週・月・年は置きません（月と年は暦の規則が要り、L1 で「暦と tz は package」と決めた範囲に入るため）。
5. 演算子と同じ計算の関数、何もしない関数は置きません（R-L0-01。別の公開関数を 1 回呼ぶだけの関数も同義語に数える）。
6. **比較**: 時刻は `Int` なので、`datetime.compare` は置きません。3 値の比較関数は `int.compare`（R-L0-16）をそのまま渡します（`list.sort_with(ts, int.compare)`）。置くと `int.compare` の転送になり、R-L0-01 に反します。

### L2 の判定

- 重複: `env.unix_timestamp` ≡ `datetime.now`（#3851 B）、`env.millis` は `datetime.now_ms` に移す（R-L1-25）→ R-datetime-01（`env` 側の撤去は L2-env）。`from_unix` `to_unix`（何もしない）、`is_after` `is_before` `diff_seconds`（`>` `<` `-` と同じ）→ R-datetime-03。
- 表現: 時刻専用の型を作るか → R-datetime-02（×）。
- 抜けている役割: 月の加算（5/9 だが暦の規則が要る）・タイムゾーン（L1 で ×）→ 足しません。
- 曜日の戻り型 → R-datetime-04（×）。

## 案

### R-datetime-01 ミリ秒の現在時刻 `datetime.now_ms` を足す — ○ 採用

- 内容: `effect fn datetime.now_ms() -> Int`（Unix エポックからのミリ秒、実時間）。`env.millis` と同じ本体です。`env.millis` と `env.unix_timestamp` の撤去は L2-env の案で行い、ヒントはそれぞれ `datetime.now_ms()`、`datetime.now()` を名指しします。
- 根拠の L0 規則: R-L0-01、R-L0-02（時計は `datetime` の持ち分）、L1 R-L1-25。
- (a) 時計を 1 か所に集める言語が多数派です（Rust `std::time`、Go `time`、Kotlin `kotlin.time`）。
- (b) wasm は `wasi:clocks` の `wall-clock`、native は OS の時計。いまの `env.millis` の経路のままです。
- (c) 経過時間を測るとき、LLM は `datetime.now` の差を秒で取って 0 になる誤りをします。ミリ秒の時計が同じモジュールにあれば名前から探せます（`monotonic_ns` を使うべき場面であることは doc に書きます）。
- (d) 追加。使用数 0（`env.millis` 11/7/21/3、llms.txt:388 が移ってきます）。
- ゲート案: ADR-0027 の effect 検品に verdict 行を足します。#3851 F の同義語ゲート規則 5 が `env.millis` との双子を撤去まで許容リストで数えます。
- issue: #3851 B（既存）

### R-datetime-02 時刻専用の型（`Instant` など）を作る — × 不採用

- 内容: 作りません。時刻は `Int` の Unix 秒のままにします。
- 根拠の L0 規則: R-L0-00。R-L0-14 の 4 により、旧い書き方が型エラーになる変更なら同じ名前のまま型を変えられるようになりました。それでも型を作ると 22 関数すべての引数と戻り型が変わり、`Int` で時刻を扱うすべての利用者のコードが書き換えになります。下の (b)(c) の利点を失ってまで払う費用ではありません。
- (a) 時刻を専用の型にする言語は多数派です（Rust、Go、Python、JS、Elixir、Kotlin、Swift）。それでも Almide は、この差を名前と検査で埋めます: 単位は関数名の接尾辞（`now`・`now_ms`・`monotonic_ns`）が表し、ADR-0001 が型で分けたのは時間の「量」（`Compute`・`Duration`）で、時刻（ある瞬間）ではありません。
- (b) `Int` の秒は ADR-0016 の決定性の約束にそのまま乗り、JSON にもそのまま出せます。
- (c) 型を作った場合、`Duration` から数値を読み出す関数がない（ADR-0001 S7）ので、`diff` の結果を秒で表示する道がなくなります。いまの `Int` なら 1 通りに書けます。
- 再開の条件: ミリ秒と秒の取り違え（`datetime.year(datetime.now_ms())`）が Dojo の計測で観測されたら、型を作る案を新しい名前の族として出し直します。
- issue: なし

### R-datetime-03 何もしない関数と演算子の双子を撤去する — ○ 採用

- 内容: `from_unix`（引数をそのまま返す）、`to_unix`（同じ）、`is_after(a, b)`（= `a > b`）、`is_before(a, b)`（= `a < b`）、`diff_seconds(a, b)`（= `a - b`）を撤去します。
- 根拠の L0 規則: R-L0-01（同じ役割に綴りは 1 つ。別の公開関数を 1 回呼ぶだけの関数も同義語に数える。演算子は ADR-0005 で標準の関数に脱糖されるので、演算子 1 つの関数も同じ扱い）。
- (a) Elixir の `from_unix`、Kotlin の `isAfter` のように、時刻が専用の型である言語では意味のある関数です。時刻が `Int` の Almide では、何も変換していません。
- (c) `datetime.from_unix(ms)` と書くと、LLM は「単位を変換してくれる」と読み、ミリ秒をそのまま秒として扱う誤りが隠れます。関数がなければ、`Int` であることが書いた形から見えます。
- (d) 撤去 5。`@deprecated(since=15, use="x")`（`from_unix`・`to_unix`。ヒント「引数をそのまま使う」）、`use="a > b"`（`is_after`）、`use="a < b"`（`is_before`）、`use="a - b"`（`diff_seconds`）、どれも `removal_epoch = 16`。`almide fix`: `datetime.from_unix(x)` → `x`、`datetime.to_unix(x)` → `x`、`datetime.is_after(a, b)` → `a > b`、`datetime.is_before(a, b)` → `a < b`、`datetime.diff_seconds(a, b)` → `a - b`。使用数（spec/tests/docs/stdlib）: `from_unix` 12/0/6/0、`to_unix` 7/0/6/0（+1 research）、`is_after` 3/0/6/0、`is_before` 5/0/6/0、`diff_seconds` 4/0/6/0。CHEATSHEET・llms.txt には出ていません。
- ゲート案: 同義語ゲートに「本体が引数をそのまま返す公開関数」「本体が演算子 1 つの公開関数」を検出する規則を足します（許容リストは減らす方向だけ）。
- issue: #3903

### R-datetime-04 `weekday` を数を返す関数にする — × 不採用

- 内容: 変えません。`weekday` は英語の曜日名（`"Monday"` …）の `String` を返したままにします。
- 根拠の L0 規則: R-L0-00。戻り型を `Int` に変えると旧い書き方（`== "Monday"`）は型エラーになるので、R-L0-14 の 4 では許される変更です。それでも、下の (a) のとおり数にする利益がないので変えません。
- (a) 数を返す言語が多数ですが、0 と 1 のどちらから、日曜と月曜のどちらから数えるかが言語ごとに違い（上の表）、数にするとかえって当たりません。
- (c) 文字列と数を取り違えると型検査で止まるので、静かに外れることはありません。
- issue: なし

### R-datetime-05 `format` の `%%` を `%` にする — × 不採用

- 内容: 変えません。doc の「`%%` の escape はない」をそのまま残し、CHEATSHEET の `format` の行にも書きます。
- 根拠の L0 規則: R-L0-14 の 4（同じ名前のまま、旧い書き方が型の検査を通って別の結果になる変更はしない。`%%` の出力を変えると、まさにこの形になる）。`%%` を含む書式は spec に 0 件で、書き方を変える利益より、学習データの旧い意味と食い違う損のほうが大きいためです。
- issue: なし

## L3: 関数表

`datetime` の現行 22 関数と、追加する 1 関数です。

| 関数 | シグネチャ(現行, mut 表示) | 他言語の主な綴り | 判定 | 案 |
|---|---|---|---|---|
| `add_days` | `datetime.add_days(ts: Int, n: Int) -> Int` | Go `AddDate(0,0,n)` / Python `+ timedelta(days=n)` / Elixir `DateTime.add(dt, n, :day)` | 据え置き | — |
| `add_hours` | `datetime.add_hours(ts: Int, n: Int) -> Int` | Go `Add(n*time.Hour)` / Python `timedelta(hours=n)` / Kotlin `+ n.hours` | 据え置き | — |
| `add_minutes` | `datetime.add_minutes(ts: Int, n: Int) -> Int` | Go `Add(n*time.Minute)` / Python `timedelta(minutes=n)` | 据え置き | — |
| `add_seconds` | `datetime.add_seconds(ts: Int, n: Int) -> Int` | Go `Add(n*time.Second)` / Elixir `DateTime.add(dt, n)` / Swift `addingTimeInterval` | 据え置き | — |
| `day` | `datetime.day(ts: Int) -> Int` | Go `Day()` / Python `.day` / JS `getUTCDate()` / Elixir `.day` | 据え置き | — |
| `diff_seconds` | `datetime.diff_seconds(a: Int, b: Int) -> Int` | Go `Sub` / Elixir `DateTime.diff` / 他は `-` | 撤去 | R-datetime-03（`a - b`） |
| `format` | `datetime.format(ts: Int, pattern: String) -> String` | Python `strftime` / Elixir `Calendar.strftime` / Go `Format`（見本の日付） | 据え置き | R-datetime-05 |
| `from_parts` | `datetime.from_parts(y: Int, m: Int, d: Int, h: Int, min: Int, s: Int) -> Int` | Go `time.Date` / Python `datetime(...)` / JS `Date.UTC` / Elixir `DateTime.new` | 据え置き | — |
| `from_unix` | `datetime.from_unix(seconds: Int) -> Int` | Elixir `DateTime.from_unix` / Go `time.Unix` / Kotlin `fromEpochSeconds` | 撤去 | R-datetime-03（引数そのもの） |
| `hour` | `datetime.hour(ts: Int) -> Int` | Go `Hour()` / Python `.hour` / JS `getUTCHours()` | 据え置き | — |
| `is_after` | `datetime.is_after(a: Int, b: Int) -> Bool` | Go `After` / Kotlin `>`（Java `isAfter`） / 他は `>` | 撤去 | R-datetime-03（`a > b`） |
| `is_before` | `datetime.is_before(a: Int, b: Int) -> Bool` | Go `Before` / 他は `<` | 撤去 | R-datetime-03（`a < b`） |
| `minute` | `datetime.minute(ts: Int) -> Int` | Go `Minute()` / Python `.minute` / JS `getUTCMinutes()` | 据え置き | — |
| `month` | `datetime.month(ts: Int) -> Int` | Go `Month()` / Python `.month` / JS `getUTCMonth()`（0 始まり） | 据え置き | — |
| `now` | `effect datetime.now() -> Int` | Python `time.time()` / Go `time.Now().Unix()` / Elixir `System.os_time(:second)` | 据え置き | R-datetime-01（`env.unix_timestamp` の残る側） |
| `monotonic_ns` | `effect datetime.monotonic_ns() -> Int` | Python `time.monotonic_ns()` / Rust `Instant::now()` / Elixir `System.monotonic_time` | 据え置き | — |
| `parse_iso` | `datetime.parse_iso(s: String) -> Result[Int, String]` | Python `fromisoformat` / Elixir `DateTime.from_iso8601` / Go `Parse(time.RFC3339, s)` / Kotlin `Instant.parse` | 据え置き | — |
| `second` | `datetime.second(ts: Int) -> Int` | Go `Second()` / Python `.second` / JS `getUTCSeconds()` | 据え置き | — |
| `to_iso` | `datetime.to_iso(ts: Int) -> String` | Python `isoformat` / JS `toISOString` / Elixir `to_iso8601` | 据え置き | — |
| `to_unix` | `datetime.to_unix(ts: Int) -> Int` | Elixir `DateTime.to_unix` / Go `.Unix()` / Python `timestamp()` | 撤去 | R-datetime-03（引数そのもの） |
| `weekday` | `datetime.weekday(ts: Int) -> String` | Go `Weekday()` / Python `weekday()`（数） / Elixir `Date.day_of_week`（数） | 据え置き | R-datetime-04 |
| `year` | `datetime.year(ts: Int) -> Int` | Go `Year()` / Python `.year` / JS `getUTCFullYear()` | 据え置き | — |
| `now_ms` | （追加）`effect datetime.now_ms() -> Int` | JS `Date.now()` / Go `time.Now().UnixMilli()` / Elixir `System.os_time(:millisecond)` / Kotlin `toEpochMilliseconds()` | 追加 | R-datetime-01 |

## L0 への差し戻し（処理済み）

- 時間の量を受け取る関数の型: `Int` のまま据え置きます（× 不採用）。時刻を Int の Unix 秒で表すと決めたので（R-datetime-02）、量も単位を名前に含めた `Int`（`add_seconds`、`sleep_ms`）で揃えます。`Duration` は ADR-0001 のとおり、`fan` の予算と期限に使う型に留めます。`Duration` に読み出す関数がない（S7）ので、時刻の計算に混ぜると、読み出す手段を足すか、型ごとに 2 通りの関数を持つことになります。どちらも R-L0-01 に反します。
