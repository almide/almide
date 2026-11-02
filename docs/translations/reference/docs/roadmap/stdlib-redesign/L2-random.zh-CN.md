<!-- description: 标准库重设计L2/L3 — random：新增采用固定SplitMix64序列的纯seeded Rng，以及通过wasi:random提供加密随机源的random.bytes；仅保留random.shuffle作为shuffle入口 -->
<a id="l2l3-random"></a>

# L2/L3: random

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](L2-random.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](L2-random.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](L2-random.zh-CN.md)

> 非规范性带日期重设计参照版。固定原文不变且为权威，不自动同步，未重新审计实现、随机数或安全性。采纳提案不表示当前已实现。加密随机源保证、跨版本固定序列及host接口说明仅保留为设计主张，并非新验证保证。6/9计数及所附列举的范围不作静默调和。确定性host检查不能证明密码学质量。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-random.md)

> 最后更新：2026-10-21
> 前提：[L0](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L0-conventions.md)、[L1](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L1-categories.md)（R-L1-04）
> 来源：Rust https://doc.rust-lang.org/std/random/index.html（unstable）；Go https://pkg.go.dev/math/rand/v2 (F) 和 https://pkg.go.dev/crypto/rand ；Python https://docs.python.org/3/library/random.html (F) 和 https://docs.python.org/3/library/secrets.html ；TS/JS https://developer.mozilla.org/en-US/docs/Web/API/Crypto/getRandomValues ；Gleam https://gleam-stdlib.hexdocs.pm/gleam/int.html (F)；Elixir https://www.erlang.org/doc/apps/stdlib/rand.html ；OCaml https://ocaml.org/manual/latest/api/Random.html ；Kotlin https://kotlinlang.org/api/core/kotlin-stdlib/kotlin.random/-random/ ；Swift https://developer.apple.com/documentation/swift/systemrandomnumbergenerator

(F) = 于2026-10-21获取并确认的内容：Go `*Rand`方法（`New` `NewPCG` `IntN` `Float64` `Shuffle` `Perm`等）、Python `random`函数与`Random([seed])`，以及“本模块的伪随机数不得用于安全用途，应使用`secrets`”的警告；Gleam的`int.random`。

当时实现（`stdlib/random*.almd`、`runtime/rs/src/random.rs`）：native使用以时间为种子的xorshift，wasm读取WASI熵（`prim.random_get`）。两者均无法传入seed，测试无法重现相同序列。源码注释承认`random.int`存在轻微取模偏差（`random_int.almd:8-9`）。

<a id="l2-役割の比較"></a>

## L2：角色比较

| 角色 | Almide | Rust | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin | Swift |
|---|---|---|---|---|---|---|---|---|---|---|
| 范围整数 | `int(min, max)`（包含两端） | —（unstable） | `IntN(n)` (F) | `randint(a, b)` (F) | — | `int.random(max)` (F) | `Enum.random(a..b)` | `Random.int` | `nextInt(from, until)` | `Int.random(in:)` |
| [0, 1) Float | `float()` | — | `Float64()` (F) | `random()` (F) | `Math.random()` | `float.random()` | `:rand.uniform()` | `Random.float` | `nextDouble()` | `Double.random(in:)` |
| 选择一个 | `choice(xs)` | — | — | `choice` (F) | — | — | `Enum.random` | — | `random()` | `randomElement()` |
| 打乱顺序 | `shuffle(xs)`（effect） | — | `Shuffle` (F) | `shuffle`（原地） (F) | — | `list.shuffle` | `Enum.shuffle` | — | `shuffled()` | `shuffled()` |
| seeded生成器 | — | — | `New(NewPCG(s1, s2))` (F) | `Random(seed)` (F) | — | — | `:rand.seed` | `Random.State.make` | `Random(seed)` | — |
| 加密随机字节 | — | — | `crypto/rand.Read` | `secrets.token_bytes`、`randbytes`（非加密用途）(F) | `crypto.getRandomValues` | — | `:crypto.strong_rand_bytes` | — | `SecureRandom`（JVM） | `SystemRandomNumberGenerator` |

<a id="完全性の規則"></a>

### 完整性规则

1. **两个来源，同一角色矩阵**：环境随机数（effect，当时四个函数）与seeded生成器`Rng`（纯）具有相同四种角色：范围整数、[0, 1) Float、选择一个、打乱顺序。seeded单元格名称机械地确定：环境函数名前加`next_`，第一个参数接收`Rng`，返回`(結果, 次の Rng)`（结果及下一个Rng）。
2. **跨版本固定序列**：`Rng`采用SplitMix64；同一seed在native与wasm产生逐bit一致序列，Almide换版本也不改变。变更时列入dialect epoch `breaks`，纳入ADR-0016数值确定性。
3. **仅字节序列用于加密**：仅`random.bytes(n)`承诺为加密用途（CSPRNG）。文档注明`int`、`float`、`choice`、`shuffle`不用于加密，与Python警告相同。
4. **只有一个`shuffle`**：打乱顺序仅用`random.shuffle`（effect）。`list.shuffle`虽为纯fn却每次结果不同，已在L2-list决定撤去。

<a id="l2-の判定"></a>

### L2判断

- 缺少角色：seeded生成器（6/9）、加密随机数（6/9）→ R-random-01、R-random-02（R-L1-04）。
- 重复：`list.shuffle`与`random.shuffle` → 保留`random.shuffle`，R-random-03。
- 边界：`random.int(5, 1)`返回`5`；若`max <= min`则返回`min`。这可理解为范围压缩为一个点，而非失败哨兵，故保留，R-random-04 ×。

<a id="案"></a>

## 提案

<a id="r-random-01-シード付きの生成器-rng-を足す---採用"></a>

### R-random-01 新增seeded生成器`Rng` — ○ 采纳

- 内容：新增不透明类型`Rng`，内部仅一个64-bit状态，及以下五个函数。均为纯`fn`，因为不读环境，按ADR-0022不是effect。
  - `random.seeded(seed: Int) -> Rng`
  - `random.next_int(rng: Rng, min: Int, max: Int) -> (Int, Rng)`（含两端，范围规则同`random.int`，用无偏拒绝采样）。
  - `random.next_float(rng: Rng) -> (Float, Rng)`（[0, 1)，从高53 bit生成）。
  - `random.next_choice[T](rng: Rng, xs: List[T]) -> (Option[T], Rng)`
  - `random.next_shuffle[T](rng: Rng, xs: List[T]) -> (List[T], Rng)`（Fisher–Yates）。
- L0依据：R-L0-04，默认复制，`mut rng`不属于封闭动词集合，故返回下一状态；R-L0-03，主语`Rng`置首；R-L0-02，返回类型`Rng`所在模块；L1 R-L1-04。
- (a) 六种语言具有该能力：Go `rand.New(rand.NewPCG(..))` (F)、Python `random.Random(seed)` (F)、Kotlin `Random(seed)`、OCaml `Random.State`、Erlang `:rand.seed`。它们采用修改生成器的方式，但Almide是值语义，因此像Gleam和Elixir（`:rand.uniform_s`返回`{値, 状態}`，即值及状态）一样返回下一状态。
- (b) 纯计算，wasm也无需host能力。SplitMix64仅需加法、XOR、逻辑右移（R-int-06的`int.bshr_logical`）、乘法（`Int`回绕），可用Almide编写，使两target序列相同。不需要host能力。
- (c) 编写“测试可复现随机数”时，LLM会写`Random(seed)`形态。返回形式`let (x, rng2) = random.next_int(rng, 1, 6)`是Almide惯用tuple解构。复用旧`rng`导致相同值重复的错误中，R-L0-05（丢弃非Unit值报错）只会阻止将`random.next_int(rng, 1, 6)`作为语句的形式。
- (d) 新增一个类型、五个函数。使用数0。
- Gate提案：spec固定seed 0、1、`int.max_value()`各自前八个值，另设wasm_cross fixture检查两target序列一致。固定序列值纳入als契约，先在als添加新C编号。由签名检查完整性规则1的`next_`与环境函数名对应关系。
- Issue：#3939。

<a id="r-random-02-暗号用の乱数-randombytes-を足す---採用"></a>

### R-random-02 新增加密随机数`random.bytes` — ○ 采纳

- 内容：`effect fn random.bytes(n: Int) -> Bytes`，从OS加密随机源读取`n`字节。`n < 0`因违反契约而abort。
- L0依据：R-L0-13，读取环境所以是effect；R-L0-15，两target可用；L1 R-L1-04。
- (a) 六种语言：Go `crypto/rand`、Python `secrets`、JS `crypto.getRandomValues`、Erlang `:crypto.strong_rand_bytes`、Kotlin/JVM `SecureRandom`、Swift `SystemRandomNumberGenerator`。
- (b) wasm：`wasi:random`的`get-random-bytes`，原文称其为当时`prim.random_get`调用的接口。native：OS `getrandom`（Linux）/ `getentropy`（macOS），不用当时xorshift。不添加新host op。ADR-0027 effect检验在确定性host检查长度为`n`及两次调用不同。
- (c) 创建token或key时，LLM倾向拼接`random.int`。有命名加密入口，就能在CHEATSHEET写一行“秘密使用`random.bytes`”。R-L1-07的`uuid.v4`也使用它。
- (d) 新增，使用数0。
- Gate提案：`spec/stdlib/random_bytes_test.almd`（长度、`n = 0`、`n < 0`的abort）及effect检验verdict行。
- Issue：#3938。

<a id="r-random-03-並べ替えは-randomshuffle-に一本化する---採用"></a>

### R-random-03 将打乱顺序统一为`random.shuffle` — ○ 采纳

- 内容：保留`random.shuffle`及其`effect fn`性质。`list.shuffle`按[L2-list](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-list.md)提案撤去，`removed_spellings.rs`提示明确指向`random.shuffle(xs)`。
- L0依据：R-L0-01、R-L0-13，每次结果不同的函数读取环境，所以是effect。
- (d) `random.shuffle`不变。L2-list R-list-05为`list.shuffle`加`@deprecated(since=15, use="random.shuffle")`，于`removal_epoch = 16`撤去。`almide fix`将`list.shuffle(xs)`改为`random.shuffle(xs)`，补上需要的`random` import。原先纯fn内的调用改写后变为E006，提示“把调用者改为effect fn，或用`random.next_shuffle`传seed”。`random.shuffle`使用数12/0/7/1。
- Issue：同L2-list提案。
- Issue：#3943。

<a id="r-random-04-randomint-の-max--min-を-abort-にする---不採用"></a>

### R-random-04 当`random.int`的`max < min`时abort — × 不采纳

- 内容：不变。`max <= min`时返回`min`是范围压缩为一点的确定答案，不是隐藏失败的值，也不属于R-L0-06第4项“可失败构造/转换不返回哨兵”的范围。`next_int`采用相同规则，使两来源边界一致。
- Issue：无。

<a id="r-random-05-シード付きの族は-bytes-のセルを持たないと宣言する---採用ゲートの宣言"></a>

### R-random-05 声明seeded族没有`bytes`单元格 — ○ 采纳，gate声明

- 当时：完整性规则1规定seeded为`next_`加环境函数名，但`random.bytes`用于加密（规则3），无`next_bytes`。这样规则1 gate会失败。
- 提案：在规则1 gate声明“`bytes`用于加密，所以没有`next_`单元格”。写出`random.next_bytes`时的提示展示重复调用`next_int(rng, 0, 255)`。
- (a) Python、Kotlin、Erlang提供seeded字节序列，共三种，非多数。
- Issue：#3974。

<a id="r-random-06-randomintmin-max-の上端を含むことを変える---不採用文書だけ"></a>

### R-random-06 改变`random.int(min, max)`包含上端点的规则 — × 不采纳，仅文档

- 原因：虽许多语言不含上端点（Go、Kotlin、OCaml、Gleam、Rust），R-L0-14.4不允许同名改义。即使改名，双参数`randint`形式（Python）是LLM最强联想，收益小；索引用途可由`random.choice`避免。只在CHEATSHEET的`random.int`行注明“包含两端”。
- Issue：无。

<a id="l3-関数表"></a>

## L3：函数表

`random`当时四个函数，加六个新增函数。

| 函数 | 签名（当时，显示mut） | 其他语言主要写法 | 判断 | 提案 |
|---|---|---|---|---|
| `int` | `effect random.int(min: Int, max: Int) -> Int` | Go `IntN` (F) / Python `randint` (F) / Gleam `int.random` (F) / Kotlin `nextInt` / Swift `Int.random(in:)` | 保留 | R-random-04 |
| `float` | `effect random.float() -> Float` | Go `Float64` (F) / Python `random` (F) / JS `Math.random` / Kotlin `nextDouble` | 保留 | — |
| `choice` | `effect random.choice(xs: List[T]) -> Option[T]` | Python `choice` (F) / Elixir `Enum.random` / Kotlin `random()` / Swift `randomElement()` | 保留 | — |
| `shuffle` | `effect random.shuffle(xs: List[T]) -> List[T]` | Go `Shuffle` (F) / Python `shuffle` (F) / Elixir `Enum.shuffle` / Kotlin `shuffled()` / Swift `shuffled()` | 保留 | R-random-03 |
| `seeded` | （新增）`random.seeded(seed: Int) -> Rng` | Go `New(NewPCG(..))` (F) / Python `Random(seed)` (F) / Kotlin `Random(seed)` / OCaml `Random.State.make` | 新增 | R-random-01 |
| `next_int` | （新增）`random.next_int(rng: Rng, min: Int, max: Int) -> (Int, Rng)` | Go `r.IntN` (F) / Python `r.randint` (F) / Kotlin `r.nextInt` / Erlang `:rand.uniform_s` | 新增 | R-random-01 |
| `next_float` | （新增）`random.next_float(rng: Rng) -> (Float, Rng)` | Go `r.Float64` (F) / Python `r.random` (F) / Kotlin `r.nextDouble` | 新增 | R-random-01 |
| `next_choice` | （新增）`random.next_choice[T](rng: Rng, xs: List[T]) -> (Option[T], Rng)` | Python `r.choice` (F) / Kotlin `xs.random(r)` | 新增 | R-random-01 |
| `next_shuffle` | （新增）`random.next_shuffle[T](rng: Rng, xs: List[T]) -> (List[T], Rng)` | Go `r.Shuffle` (F) / Python `r.shuffle` (F) / Kotlin `xs.shuffled(r)` | 新增 | R-random-01 |
| `bytes` | （新增）`effect random.bytes(n: Int) -> Bytes` | Go `crypto/rand.Read` / Python `secrets.token_bytes` / JS `crypto.getRandomValues` / Swift `SystemRandomNumberGenerator` | 新增 | R-random-02 |
