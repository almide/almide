<!-- description: Stdlib redesign L2/L3 — random: add a seeded, pure Rng with fixed SplitMix64 sequences, and random.bytes as the cryptographic source over wasi:random; random.shuffle is the one shuffle -->
<a id="l2l3-random"></a>

# L2/L3: random

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](L2-random.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](L2-random.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](L2-random.zh-CN.md)

> Non-normative dated redesign snapshot. The pinned original remains authoritative and unchanged; no automatic synchronization or fresh implementation, randomness or security audit. Accepted proposals are not claims of current implementation. Cryptographic-source guarantees, version-stable sequences and host-interface descriptions are retained as design claims, not newly verified guarantees. The six-of-nine counts and the accompanying enumeration are retained without silently reconciling their scope. Deterministic-host checks do not establish cryptographic quality. [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-random.md)

> Last updated: 2026-10-21
> Prerequisites: [L0](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L0-conventions.md), [L1](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L1-categories.md) (R-L1-04)
> Sources: Rust https://doc.rust-lang.org/std/random/index.html (unstable); Go https://pkg.go.dev/math/rand/v2 (F) and https://pkg.go.dev/crypto/rand ; Python https://docs.python.org/3/library/random.html (F) and https://docs.python.org/3/library/secrets.html ; TS/JS https://developer.mozilla.org/en-US/docs/Web/API/Crypto/getRandomValues ; Gleam https://gleam-stdlib.hexdocs.pm/gleam/int.html (F); Elixir https://www.erlang.org/doc/apps/stdlib/rand.html ; OCaml https://ocaml.org/manual/latest/api/Random.html ; Kotlin https://kotlinlang.org/api/core/kotlin-stdlib/kotlin.random/-random/ ; Swift https://developer.apple.com/documentation/swift/systemrandomnumbergenerator

(F) = fetched and checked on 2026-10-21: Go `*Rand` methods (`New` `NewPCG` `IntN` `Float64` `Shuffle` `Perm`, and others); Python `random` functions and `Random([seed])`, including the warning that this module's pseudorandom numbers must not be used for security and that `secrets` should be used instead; Gleam `int.random`.

The implementation at that time (`stdlib/random*.almd`, `runtime/rs/src/random.rs`): native uses time-seeded xorshift; wasm reads WASI entropy (`prim.random_get`). Neither accepts a seed, so tests cannot reproduce a sequence. Source comments acknowledge slight modulo bias in `random.int` (`random_int.almd:8-9`).

<a id="l2-役割の比較"></a>

## L2: Comparing Roles

| Role | Almide | Rust | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin | Swift |
|---|---|---|---|---|---|---|---|---|---|---|
| Ranged integer | `int(min, max)` (inclusive endpoints) | —（unstable） | `IntN(n)` (F) | `randint(a, b)` (F) | — | `int.random(max)` (F) | `Enum.random(a..b)` | `Random.int` | `nextInt(from, until)` | `Int.random(in:)` |
| Float in [0, 1) | `float()` | — | `Float64()` (F) | `random()` (F) | `Math.random()` | `float.random()` | `:rand.uniform()` | `Random.float` | `nextDouble()` | `Double.random(in:)` |
| Choose one | `choice(xs)` | — | — | `choice` (F) | — | — | `Enum.random` | — | `random()` | `randomElement()` |
| Shuffle | `shuffle(xs)` (effect) | — | `Shuffle` (F) | `shuffle` (in place) (F) | — | `list.shuffle` | `Enum.shuffle` | — | `shuffled()` | `shuffled()` |
| Seeded generator | — | — | `New(NewPCG(s1, s2))` (F) | `Random(seed)` (F) | — | — | `:rand.seed` | `Random.State.make` | `Random(seed)` | — |
| Cryptographic random bytes | — | — | `crypto/rand.Read` | `secrets.token_bytes`, `randbytes` (not cryptographic)(F) | `crypto.getRandomValues` | — | `:crypto.strong_rand_bytes` | — | `SecureRandom` (JVM) | `SystemRandomNumberGenerator` |

<a id="完全性の規則"></a>

### Completeness Rules

1. **Two sources, the same role matrix**: environmental randomness (effect, the existing four functions) and seeded generator `Rng` (pure) have the same four roles: ranged integers, Float in [0, 1), selecting one item, and shuffling. Seeded cells are named mechanically: prefix the environmental function name with `next_`, take `Rng` as the first argument, and return `(結果, 次の Rng)` (result, next Rng).
2. **Sequences are fixed across versions**: `Rng` uses SplitMix64. A seed produces bit-identical sequences on native and wasm, unchanged across Almide versions. Changes belong in dialect epoch `breaks`, within ADR-0016's numeric determinism.
3. **Only byte sequences are cryptographic**: only `random.bytes(n)` is promised as cryptographic (CSPRNG). Document that `int`, `float`, `choice`, and `shuffle` are not cryptographic, with the same warning as Python.
4. **One `shuffle`**: shuffling uses only `random.shuffle` (effect). L2-list already decided to remove `list.shuffle`, a pure fn whose result nevertheless changes each time.

<a id="l2-の判定"></a>

### L2 Judgments

- Missing roles: seeded generators (6/9), cryptographic randomness (6/9) → R-random-01 and R-random-02, R-L1-04.
- Duplication: `list.shuffle` and `random.shuffle` → retain `random.shuffle`, R-random-03.
- Boundary: `random.int(5, 1)` returns `5`; if `max <= min`, return `min`. This can be read as a range collapsed to one point, not a failure sentinel, so retain it, R-random-04 ×.

<a id="案"></a>

## Proposals

<a id="r-random-01-シード付きの生成器-rng-を足す---採用"></a>

### R-random-01 Add Seeded Generator `Rng` — ○ Accepted

- Contents: add opaque type `Rng`, containing one 64-bit state, and the following five functions. All are pure `fn`: they do not read the environment and therefore are not effects under ADR-0022.
  - `random.seeded(seed: Int) -> Rng`
  - `random.next_int(rng: Rng, min: Int, max: Int) -> (Int, Rng)` (inclusive endpoints, same range rule as `random.int`, unbiased rejection sampling).
  - `random.next_float(rng: Rng) -> (Float, Rng)` ([0, 1), constructed from the upper 53 bits).
  - `random.next_choice[T](rng: Rng, xs: List[T]) -> (Option[T], Rng)`
  - `random.next_shuffle[T](rng: Rng, xs: List[T]) -> (List[T], Rng)` (Fisher–Yates).
- L0 basis: R-L0-04, copying is the default and `mut rng` is outside the closed verb set, so return the next state; R-L0-03, subject `Rng` first; R-L0-02, module of returned type `Rng`; L1 R-L1-04.
- (a) Six languages have it: Go `rand.New(rand.NewPCG(..))` (F), Python `random.Random(seed)` (F), Kotlin `Random(seed)`, OCaml `Random.State`, and Erlang `:rand.seed`. These use mutable generators, but Almide has value semantics, so return the next state like Gleam and Elixir (`:rand.uniform_s` returns `{値, 状態}`, meaning value and state).
- (b) Pure computation, requiring no host capability even on wasm. SplitMix64 needs only addition, XOR, logical right shift (R-int-06's `int.bshr_logical`), and multiplication (`Int` wraps), so it can be written in Almide and produce the same sequence on both targets. No host capability is needed.
- (c) For reproducible test randomness, LLMs write the `Random(seed)` shape. Returning `let (x, rng2) = random.next_int(rng, 1, 6)` uses Almide's standard tuple destructuring. R-L0-05, which errors on discarding non-Unit values, stops only the statement form `random.next_int(rng, 1, 6)` among mistakes that reuse old `rng` and repeat the same value.
- (d) Addition: one type, five functions. Usage count 0.
- Proposed gate: specs fixing the first eight values from seeds 0, 1, and `int.max_value()`, plus a wasm_cross fixture checking sequences match across targets. Put fixed sequence values in als contracts, placing a new C number in als first. Check the completeness-rule-1 mapping between `next_` names and environmental names from signatures.
- Issue: #3939.

<a id="r-random-02-暗号用の乱数-randombytes-を足す---採用"></a>

### R-random-02 Add Cryptographic Randomness `random.bytes` — ○ Accepted

- Contents: `effect fn random.bytes(n: Int) -> Bytes`, returning `n` bytes from the OS cryptographic random source. `n < 0` aborts as a contract violation.
- L0 basis: R-L0-13, reading the environment makes it an effect; R-L0-15, both targets; L1 R-L1-04.
- (a) Six languages: Go `crypto/rand`, Python `secrets`, JS `crypto.getRandomValues`, Erlang `:crypto.strong_rand_bytes`, Kotlin/JVM `SecureRandom`, Swift `SystemRandomNumberGenerator`.
- (b) wasm: `get-random-bytes` in `wasi:random`, described here as what the existing `prim.random_get` calls. Native: OS `getrandom` on Linux / `getentropy` on macOS, not the current xorshift. No new host op. ADR-0027 effect inspection uses a deterministic host to check that length is `n` and two calls differ.
- (c) LLMs tend to concatenate `random.int` values when creating tokens or keys. A named cryptographic entry point allows a CHEATSHEET line, “use `random.bytes` for secrets.” R-L1-07's `uuid.v4` also uses it.
- (d) Addition. Usage count 0.
- Proposed gate: `spec/stdlib/random_bytes_test.almd`, checking length, `n = 0`, and abort for `n < 0`, plus an effect-inspection verdict row.
- Issue: #3938.

<a id="r-random-03-並べ替えは-randomshuffle-に一本化する---採用"></a>

### R-random-03 Consolidate Shuffling into `random.shuffle` — ○ Accepted

- Contents: keep `random.shuffle` as `effect fn`. Remove `list.shuffle` under the [L2-list](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-list.md) proposal; `removed_spellings.rs` hints explicitly name `random.shuffle(xs)`.
- L0 basis: R-L0-01 and R-L0-13, a function whose result changes each time reads the environment and is an effect.
- (d) Keep `random.shuffle`. L2-list R-list-05 marks `list.shuffle` with `@deprecated(since=15, use="random.shuffle")` and removes it at `removal_epoch = 16`. `almide fix` rewrites `list.shuffle(xs)` → `random.shuffle(xs)` and adds the required `random` import. Calls inside pure fn become E006 after rewriting; hints say to make the caller effect fn or provide a seed through `random.next_shuffle`. `random.shuffle` usage: 12/0/7/1.
- Issue: same issue as the L2-list proposal.
- Issue: #3943.

<a id="r-random-04-randomint-の-max--min-を-abort-にする---不採用"></a>

### R-random-04 Abort for `random.int` with `max < min` — × Rejected

- Contents: no change. Returning `min` for `max <= min` is a defined answer for a range collapsed to one point, not a value hiding failure, and is outside R-L0-06 item 4's ban on sentinels for fallible construction/conversion. Apply the same rule to `next_int` so boundaries align across both sources.
- Issue: none.

<a id="r-random-05-シード付きの族は-bytes-のセルを持たないと宣言する---採用ゲートの宣言"></a>

### R-random-05 Declare No `bytes` Cell in the Seeded Family — ○ Accepted, Gate Declaration

- Current: completeness rule 1 names seeded cells as `next_` plus the environmental name, but cryptographic `random.bytes` (rule 3) has no `next_bytes`. Without an exception, rule 1's gate fails.
- Proposal: declare in rule 1's gate that cryptographic `bytes` has no `next_` cell. A hint for `random.next_bytes` should show repeated `next_int(rng, 0, 255)` calls.
- (a) Python, Kotlin, and Erlang have seeded byte sequences: three languages, not a majority.
- Issue: #3974.

<a id="r-random-06-randomintmin-max-の上端を含むことを変える---不採用文書だけ"></a>

### R-random-06 Change the Inclusive Upper Bound of `random.int(min, max)` — × Rejected, Documentation Only

- Reason: many languages exclude the upper bound (Go, Kotlin, OCaml, Gleam, Rust), but R-L0-14.4 prohibits changing meaning under the same name. Even renaming gains little because the two-argument `randint` shape from Python is the strongest LLM association; indexing use cases can instead use `random.choice`. Only add “inclusive endpoints” to the CHEATSHEET `random.int` row.
- Issue: none.

<a id="l3-関数表"></a>

## L3: Function Table

The four existing `random` functions and six additions.

| Function | Signature (current, mut shown) | Main spellings in other languages | Judgment | Proposal |
|---|---|---|---|---|
| `int` | `effect random.int(min: Int, max: Int) -> Int` | Go `IntN` (F) / Python `randint` (F) / Gleam `int.random` (F) / Kotlin `nextInt` / Swift `Int.random(in:)` | Keep | R-random-04 |
| `float` | `effect random.float() -> Float` | Go `Float64` (F) / Python `random` (F) / JS `Math.random` / Kotlin `nextDouble` | Keep | — |
| `choice` | `effect random.choice(xs: List[T]) -> Option[T]` | Python `choice` (F) / Elixir `Enum.random` / Kotlin `random()` / Swift `randomElement()` | Keep | — |
| `shuffle` | `effect random.shuffle(xs: List[T]) -> List[T]` | Go `Shuffle` (F) / Python `shuffle` (F) / Elixir `Enum.shuffle` / Kotlin `shuffled()` / Swift `shuffled()` | Keep | R-random-03 |
| `seeded` | (addition) `random.seeded(seed: Int) -> Rng` | Go `New(NewPCG(..))` (F) / Python `Random(seed)` (F) / Kotlin `Random(seed)` / OCaml `Random.State.make` | Add | R-random-01 |
| `next_int` | (addition) `random.next_int(rng: Rng, min: Int, max: Int) -> (Int, Rng)` | Go `r.IntN` (F) / Python `r.randint` (F) / Kotlin `r.nextInt` / Erlang `:rand.uniform_s` | Add | R-random-01 |
| `next_float` | (addition) `random.next_float(rng: Rng) -> (Float, Rng)` | Go `r.Float64` (F) / Python `r.random` (F) / Kotlin `r.nextDouble` | Add | R-random-01 |
| `next_choice` | (addition) `random.next_choice[T](rng: Rng, xs: List[T]) -> (Option[T], Rng)` | Python `r.choice` (F) / Kotlin `xs.random(r)` | Add | R-random-01 |
| `next_shuffle` | (addition) `random.next_shuffle[T](rng: Rng, xs: List[T]) -> (List[T], Rng)` | Go `r.Shuffle` (F) / Python `r.shuffle` (F) / Kotlin `xs.shuffled(r)` | Add | R-random-01 |
| `bytes` | (addition) `effect random.bytes(n: Int) -> Bytes` | Go `crypto/rand.Read` / Python `secrets.token_bytes` / JS `crypto.getRandomValues` / Swift `SystemRandomNumberGenerator` | Add | R-random-02 |
