<!-- description: Stdlib redesign L2/L3 — random: add a seeded, pure Rng with fixed SplitMix64 sequences, and random.bytes as the cryptographic source over wasi:random; random.shuffle is the one shuffle -->
# L2/L3: random

> Last updated: 2026-10-21
> 前提: [L0](L0-conventions.md)・[L1](L1-categories.md)（R-L1-04）
> 出典: Rust https://doc.rust-lang.org/std/random/index.html（unstable） ・ Go https://pkg.go.dev/math/rand/v2 (F) と https://pkg.go.dev/crypto/rand ・ Python https://docs.python.org/3/library/random.html (F) と https://docs.python.org/3/library/secrets.html ・ TS/JS https://developer.mozilla.org/en-US/docs/Web/API/Crypto/getRandomValues ・ Gleam https://gleam-stdlib.hexdocs.pm/gleam/int.html (F) ・ Elixir https://www.erlang.org/doc/apps/stdlib/rand.html ・ OCaml https://ocaml.org/manual/latest/api/Random.html ・ Kotlin https://kotlinlang.org/api/core/kotlin-stdlib/kotlin.random/-random/ ・ Swift https://developer.apple.com/documentation/swift/systemrandomnumbergenerator

(F) = 2026-10-21 に取り寄せて確かめたもの: Go `*Rand` のメソッド（`New` `NewPCG` `IntN` `Float64` `Shuffle` `Perm` ほか）、Python `random` の関数と `Random([seed])`、および「このモジュールの擬似乱数は安全のために使ってはならない、`secrets` を使え」という注意書き、Gleam の `int.random`。

いまの実装（`stdlib/random*.almd`、`runtime/rs/src/random.rs`）: native は時刻を種にした xorshift、wasm は WASI のエントロピー（`prim.random_get`）を読みます。どちらもシードを渡す手段がなく、テストで同じ列を再現できません。`random.int` は剰余でわずかに偏ることを、ソースのコメントが認めています（`random_int.almd:8-9`）。

## L2: 役割の比較

| 役割 | Almide | Rust | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin | Swift |
|---|---|---|---|---|---|---|---|---|---|---|
| 範囲の整数 | `int(min, max)`（両端を含む） | —（unstable） | `IntN(n)` (F) | `randint(a, b)` (F) | — | `int.random(max)` (F) | `Enum.random(a..b)` | `Random.int` | `nextInt(from, until)` | `Int.random(in:)` |
| [0, 1) の Float | `float()` | — | `Float64()` (F) | `random()` (F) | `Math.random()` | `float.random()` | `:rand.uniform()` | `Random.float` | `nextDouble()` | `Double.random(in:)` |
| 1 つ選ぶ | `choice(xs)` | — | — | `choice` (F) | — | — | `Enum.random` | — | `random()` | `randomElement()` |
| 並べ替え | `shuffle(xs)`（effect） | — | `Shuffle` (F) | `shuffle`（その場） (F) | — | `list.shuffle` | `Enum.shuffle` | — | `shuffled()` | `shuffled()` |
| シード付きの生成器 | — | — | `New(NewPCG(s1, s2))` (F) | `Random(seed)` (F) | — | — | `:rand.seed` | `Random.State.make` | `Random(seed)` | — |
| 暗号用の乱数バイト | — | — | `crypto/rand.Read` | `secrets.token_bytes`、`randbytes`（暗号用ではない）(F) | `crypto.getRandomValues` | — | `:crypto.strong_rand_bytes` | — | `SecureRandom`（JVM） | `SystemRandomNumberGenerator` |

### 完全性の規則

1. **2 つの源、同じ役割の表**: 環境の乱数（effect、いまの 4 関数）と、シード付きの生成器 `Rng`（純粋）は、同じ 4 つの役割を持ちます: 範囲の整数、[0, 1) の Float、1 つ選ぶ、並べ替え。シード付きのセルの名前は「環境の乱数の名前の前に `next_` を付け、第 1 引数に `Rng` を取り、`(結果, 次の Rng)` を返す」と機械的に決めます。
2. **列は版をまたいで固定**: `Rng` のアルゴリズムは SplitMix64 で、同じシードからの列は native と wasm で 1 ビットも違わず、Almide の版が変わっても変わりません。変えるときは dialect epoch の `breaks` に入れます（ADR-0016 の数値の決定性に入れる）。
3. **暗号用はバイト列だけ**: `random.bytes(n)` だけを暗号用（CSPRNG）として約束します。`int`・`float`・`choice`・`shuffle` は暗号用ではないと doc に書きます（Python と同じ注意）。
4. **`shuffle` は 1 つ**: 並べ替えは `random.shuffle`（effect）だけです。`list.shuffle`（純粋な fn なのに結果が毎回違う）は L2-list で撤去が決まっています。

### L2 の判定

- 抜けている役割: シード付きの生成器（6/9）、暗号用の乱数（6/9）→ R-random-01、R-random-02（R-L1-04）。
- 重複: `list.shuffle` と `random.shuffle` → 残すのは `random.shuffle`（R-random-03）。
- 端: `random.int(5, 1)` は `5` を返す（`max <= min` なら `min`）。失敗を表す値ではなく「範囲が 1 点に潰れた」と読める規則なので据え置きます（R-random-04 ×）。

## 案

### R-random-01 シード付きの生成器 `Rng` を足す — ○ 採用

- 内容: 型 `Rng`（不透明。中身は 64 bit の状態 1 つ）と、次の 5 関数を足します。すべて純粋な `fn` です（環境を読まないので、ADR-0022 のとおり effect ではありません）。
  - `random.seeded(seed: Int) -> Rng`
  - `random.next_int(rng: Rng, min: Int, max: Int) -> (Int, Rng)`（両端を含む。`random.int` と同じ範囲の規則。偏りのない棄却法で引く）
  - `random.next_float(rng: Rng) -> (Float, Rng)`（[0, 1)、上位 53 bit から作る）
  - `random.next_choice[T](rng: Rng, xs: List[T]) -> (Option[T], Rng)`
  - `random.next_shuffle[T](rng: Rng, xs: List[T]) -> (List[T], Rng)`（Fisher–Yates）
- 根拠の L0 規則: R-L0-04（コピーが既定。`mut rng` は閉じた動詞集合に入らないので、次の状態を返す形にする）、R-L0-03（主語の `Rng` を先頭に）、R-L0-02（返す型 `Rng` のモジュール）、L1 R-L1-04。
- (a) Go `rand.New(rand.NewPCG(..))` (F)、Python `random.Random(seed)` (F)、Kotlin `Random(seed)`、OCaml `Random.State`、Erlang `:rand.seed` の 6 言語が持っています。どれも生成器を書き換える流儀ですが、Almide は値の意味論なので、Gleam・Elixir（`:rand.uniform_s` が `{値, 状態}` を返す）と同じ「次の状態を返す」形にします。
- (b) 純粋な計算です（wasm でも host の機能は要りません）。SplitMix64 は加算・XOR・論理右シフト（R-int-06 の `int.bshr_logical`）・掛け算（`Int` は折り返す）だけで書けるので、Almide で書いて両 target で同じ列になります。host の機能は要りません。
- (c) 「テストで再現できる乱数」を書くとき、LLM は `Random(seed)` 形を書きます。返す形 `let (x, rng2) = random.next_int(rng, 1, 6)` は、Almide のタプル分解の定番の形です。古い `rng` を使い回す誤り（同じ値が続く）は、R-L0-05（Unit 以外の値を捨てたらエラー）で、`random.next_int(rng, 1, 6)` を文として書いた形だけは止まります。
- (d) 追加（型 1、関数 5）。使用数 0。
- ゲート案: シード 0・1・`int.max_value()` から最初の 8 個の値を固定する spec と、wasm_cross fixture（両 target の列の一致）。列の固定値は als の契約に入れます（新しい C 番号を als に先に置く）。完全性の規則 1 の「`next_` + 環境の乱数の名前」の対応を、シグネチャから検査するゲートを置きます。
- issue: #3939

### R-random-02 暗号用の乱数 `random.bytes` を足す — ○ 採用

- 内容: `effect fn random.bytes(n: Int) -> Bytes`。OS の暗号用の乱数源から `n` バイト。`n < 0` は abort（契約違反）。
- 根拠の L0 規則: R-L0-13（環境を読むので effect）、R-L0-15（両 target で動く）、L1 R-L1-04。
- (a) Go `crypto/rand`、Python `secrets`、JS `crypto.getRandomValues`、Erlang `:crypto.strong_rand_bytes`、Kotlin（JVM）`SecureRandom`、Swift `SystemRandomNumberGenerator` の 6 言語。
- (b) wasm: `wasi:random` の `get-random-bytes`（いまの `prim.random_get` が呼んでいるもの）。native: OS の `getrandom`（Linux）/ `getentropy`（macOS）で読み、いまの xorshift は使いません。新しい host op は足しません。ADR-0027 の effect 検品は、決定的なホストで「長さが `n` であること」と「2 回の呼び出しが違うこと」を見ます。
- (c) トークンや鍵を作るとき、LLM は `random.int` を並べて書きがちです。名前のある暗号用の入口があれば、CHEATSHEET に「秘密には `random.bytes`」と 1 行で書けます。R-L1-07 の `uuid.v4` もこれを使います。
- (d) 追加。使用数 0。
- ゲート案: `spec/stdlib/random_bytes_test.almd`（長さ、`n = 0`、`n < 0` の abort）と effect 検品の verdict 行。
- issue: #3938

### R-random-03 並べ替えは `random.shuffle` に一本化する — ○ 採用

- 内容: `random.shuffle` を据え置き、`effect fn` のままにします。`list.shuffle` の撤去は [L2-list](L2-list.md) の案で行い、`removed_spellings.rs` のヒントは `random.shuffle(xs)` を名指しします。
- 根拠の L0 規則: R-L0-01、R-L0-13（結果が毎回違う関数は環境を読んでいるので effect）。
- (d) `random.shuffle` は据え置き。`list.shuffle` は L2-list の R-list-05 で `@deprecated(since=15, use="random.shuffle")` を付け、`removal_epoch = 16` で撤去します。`almide fix` は `list.shuffle(xs)` → `random.shuffle(xs)` に書き換えます（`random` は import が要るので足します）。純粋な fn の中で呼んでいた箇所は、書き換えたあと E006 になるので、ヒントで「呼び出し側を effect fn にするか、`random.next_shuffle` でシードを渡す」と示します。`random.shuffle` の使用数 12/0/7/1。
- issue: L2-list の案と同じ issue
- issue: #3943

### R-random-04 `random.int` の `max < min` を abort にする — × 不採用

- 内容: 変えません。`max <= min` で `min` を返す規則は、失敗を隠す値ではなく、範囲が 1 点に潰れたときの決まった答えで、R-L0-06 の 4（失敗しうる構築・変換は番兵を返さない）の対象でもありません。同じ規則を `next_int` にも使って、2 つの源の端をそろえます。
- issue: なし

### R-random-05 シード付きの族は `bytes` のセルを持たない、と宣言する — ○ 採用（ゲートの宣言）

- いま: 完全性の規則 1 は「シード付きは `next_` + 環境の乱数の名前」と決めていますが、`random.bytes` は暗号用（規則 3）なので `next_bytes` がありません。このままだと規則 1 のゲートが落ちます。
- 案: 規則 1 のゲートに「`bytes` は暗号用なので `next_` のセルを持たない」を宣言します。`random.next_bytes` と書いたときの案内で、`next_int(rng, 0, 255)` の繰り返しを示します。
- (a) シード付きのバイト列を持つのは Python・Kotlin・Erlang の 3 言語で、多数派ではありません。
- issue: #3974

### R-random-06 `random.int(min, max)` の上端を含むことを変える — × 不採用（文書だけ）

- 理由: 上端を含まない言語が多い（Go・Kotlin・OCaml・Gleam・Rust）ものの、同じ名前で意味を変えることは R-L0-14.4 が許しません。改名しても、2 引数の `randint` 形（Python）が LLM の最も強い連想なので利益は小さく、添字の用途は `random.choice` で避けられます。CHEATSHEET の `random.int` の行に「両端を含む」と書くことだけをします。
- issue: なし

## L3: 関数表

`random` の現行 4 関数と、追加する 6 関数です。

| 関数 | シグネチャ(現行, mut 表示) | 他言語の主な綴り | 判定 | 案 |
|---|---|---|---|---|
| `int` | `effect random.int(min: Int, max: Int) -> Int` | Go `IntN` (F) / Python `randint` (F) / Gleam `int.random` (F) / Kotlin `nextInt` / Swift `Int.random(in:)` | 据え置き | R-random-04 |
| `float` | `effect random.float() -> Float` | Go `Float64` (F) / Python `random` (F) / JS `Math.random` / Kotlin `nextDouble` | 据え置き | — |
| `choice` | `effect random.choice(xs: List[T]) -> Option[T]` | Python `choice` (F) / Elixir `Enum.random` / Kotlin `random()` / Swift `randomElement()` | 据え置き | — |
| `shuffle` | `effect random.shuffle(xs: List[T]) -> List[T]` | Go `Shuffle` (F) / Python `shuffle` (F) / Elixir `Enum.shuffle` / Kotlin `shuffled()` / Swift `shuffled()` | 据え置き | R-random-03 |
| `seeded` | （追加）`random.seeded(seed: Int) -> Rng` | Go `New(NewPCG(..))` (F) / Python `Random(seed)` (F) / Kotlin `Random(seed)` / OCaml `Random.State.make` | 追加 | R-random-01 |
| `next_int` | （追加）`random.next_int(rng: Rng, min: Int, max: Int) -> (Int, Rng)` | Go `r.IntN` (F) / Python `r.randint` (F) / Kotlin `r.nextInt` / Erlang `:rand.uniform_s` | 追加 | R-random-01 |
| `next_float` | （追加）`random.next_float(rng: Rng) -> (Float, Rng)` | Go `r.Float64` (F) / Python `r.random` (F) / Kotlin `r.nextDouble` | 追加 | R-random-01 |
| `next_choice` | （追加）`random.next_choice[T](rng: Rng, xs: List[T]) -> (Option[T], Rng)` | Python `r.choice` (F) / Kotlin `xs.random(r)` | 追加 | R-random-01 |
| `next_shuffle` | （追加）`random.next_shuffle[T](rng: Rng, xs: List[T]) -> (List[T], Rng)` | Go `r.Shuffle` (F) / Python `r.shuffle` (F) / Kotlin `xs.shuffled(r)` | 追加 | R-random-01 |
| `bytes` | （追加）`effect random.bytes(n: Int) -> Bytes` | Go `crypto/rand.Read` / Python `secrets.token_bytes` / JS `crypto.getRandomValues` / Swift `SystemRandomNumberGenerator` | 追加 | R-random-02 |
