# random

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](random.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](random.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](random.zh-CN.md)



> 原文：[random.md @ 35297a6](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/random.md)。这是供读者参考的非规范性固定快照，不参与自动生成或自动同步。签名、代码示例和 API 索引注释保持原文，说明译文附于文末。本次翻译未独立复核测量、行为或示例。
>
> 原文存在不一致：Wasm 元素覆盖一段称包括 record/tuple，后续段落却限制为 scalar/String/字符串对。译文未订正而是保留两者，不代表独立确认了当前覆盖范围。

随机数生成。import random，effect。

`random.int`、`random.float`、`random.choice` 和 `random.shuffle` 不具备密码学安全性：不要用于秘密（token、key、password、nonce）。秘密应使用 `random.bytes`，这是唯一取自操作系统密码学随机生成器的来源。

### `random.int(min: Int, max: Int) -> Int`

生成 min 到 max 之间（含两端）的随机整数。

```almd check
import random

effect fn main() -> Unit = {
  println(int.to_string(random.int(1, 100))) // => 42
}
```

### `random.float() -> Float`

生成 `[0, 1)` 内的随机 float：53 个随机位乘以 2^-53，因此在所有目标上 `0.0` 可能出现，`1.0` 不会。

```almd check
import random

effect fn main() -> Unit = {
  println(float.to_string(random.float())) // => 0.7321
}
```

### `random.choice(xs: List[A]) -> Option[A]`

从列表随机选择元素；空时为 none。

```almd check
import random

fn show(o: Option[String]) -> String = match o {
  some(s) => "some(\"${s}\")",
  none => "none",
}

fn show_pair(o: Option[(String, String)]) -> String = match o {
  some((omen, advice)) => "some((\"${omen}\", \"${advice}\"))",
  none => "none",
}

effect fn main() -> Unit = {
  println(show(random.choice(["a", "b", "c"]))) // => some("b")
  println(show_pair(random.choice([("大吉", "Ship it."), ("凶", "Wait.")]))) // => some(("凶", "Wait."))
}
```

**Wasm 元素覆盖：** `choice` 和 `shuffle` 使用结构化列表元素表示，包括 record 和 tuple；不限于 scalar、string 或 string pair。

### `random.shuffle(xs: List[A]) -> List[A]`

返回随机打乱后的列表副本。

```almd check
import random

effect fn main() -> Unit = {
  println("${random.shuffle([1, 2, 3])}") // => [3, 1, 2]
}
```

**wasm 元素覆盖**（#1169）：scalar、`String`、`(String, String)` 元素在 wasm 路径运行（pair 版本通过 `list.swap` 交换 COW 副本内的 raw slot）；其他元素类型会如实以 wall 拒绝，并通过 native 路径运行。

### `random.bytes(n: Int) -> Bytes`

来自平台密码学安全随机源（CSPRNG）的 `n` 字节：native 使用 OS（Linux 的 `getrandom`、macOS 的 `getentropy`），wasm 使用 `wasi:random`。所有秘密都应使用它。`n = 0` 得到空 `Bytes`；负 `n` 以 `Error: random.bytes: negative length <n>` abort（exit 1）。

```almd check
import random
import hex

effect fn main() -> Unit = {
  let key = random.bytes(32)
  println(int.to_string(bytes.len(key))) // => 32
  println(hex.encode(random.bytes(4))) // => 9f3a0c7e
}
```



## 固定快照 API 索引 (10 个函数)

```
// Random Int in [min, max]; min if max <= min. Not for secrets: use random.bytes.
// @since 0.5.0 or earlier
effect random.int(min: Int, max: Int) -> Int

// Random Float in [0, 1). Not for secrets: use random.bytes.
// @since 0.5.0 or earlier
effect random.float() -> Float

// Random element of xs; none when empty. Not for secrets: use random.bytes.
// @since 0.5.0 or earlier
effect random.choice(xs: List[A]) -> Option[A]

// Random permutation of xs as a new list. Not for secrets: use random.bytes.
// @since 0.5.0 or earlier
effect random.shuffle(xs: List[A]) -> List[A]

// n bytes from the OS cryptographic random source (CSPRNG); n < 0 aborts.
// @since unreleased
effect random.bytes(n: Int) -> Bytes

// A generator whose stream is fixed by seed.
// @since unreleased
random.seeded(seed: Int) -> Rng

// An Int in [min, max] (both ends) and the next state; min, state unchanged, if max <= min.
// @since unreleased
random.next_int(rng: Rng, min: Int, max: Int) -> (Int, Rng)

// A Float in [0, 1) from the top 53 bits, and the next state.
// @since unreleased
random.next_float(rng: Rng) -> (Float, Rng)

// A uniformly chosen element (none, state unchanged, when xs is empty) and the next state.
// @since unreleased
random.next_choice(rng: Rng, xs: List[A]) -> (Option[A], Rng)

// A uniform permutation of xs (Fisher-Yates) and the next state.
// @since unreleased
random.next_shuffle(rng: Rng, xs: List[A]) -> (List[A], Rng)
```



## API 说明译文（按上述签名顺序）

- `effect random.int(min: Int, max: Int) -> Int` — [min, max] 内的随机 Int；max <= min 时为 min。不可用于秘密，请使用 random.bytes。
- `effect random.float() -> Float` — [0, 1) 内的随机 Float。不可用于秘密，请使用 random.bytes。
- `effect random.choice(xs: List[A]) -> Option[A]` — xs 的随机元素；空时为 none。不可用于秘密，请使用 random.bytes。
- `effect random.shuffle(xs: List[A]) -> List[A]` — xs 的随机排列，作为新列表返回。不可用于秘密，请使用 random.bytes。
- `effect random.bytes(n: Int) -> Bytes` — 来自 OS 密码学随机源（CSPRNG）的 n 字节；n < 0 则 abort。
- `random.seeded(seed: Int) -> Rng` — 随机流由 seed 固定的 generator。
- `random.next_int(rng: Rng, min: Int, max: Int) -> (Int, Rng)` — [min, max]（含两端）内的 Int 与下一状态；max <= min 时返回 min，状态不变。
- `random.next_float(rng: Rng) -> (Float, Rng)` — 由高 53 位生成的 [0, 1) 内 Float 与下一状态。
- `random.next_choice(rng: Rng, xs: List[A]) -> (Option[A], Rng)` — 均匀选择的元素及下一状态（xs 为空时为 none，状态不变）。
- `random.next_shuffle(rng: Rng, xs: List[A]) -> (List[A], Rng)` — xs 的均匀排列（Fisher-Yates）与下一状态。
