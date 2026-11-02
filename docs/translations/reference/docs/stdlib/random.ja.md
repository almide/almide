# random

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](random.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](random.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](random.zh-CN.md)



> 原文：[random.md @ 35297a6](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/random.md)。非規範的な読者向け固定スナップショットです。自動生成・自動同期されません。署名・コード例・API 索引のコメントは原文を保持し、説明の訳を末尾に併記しています。計測・動作・例をこの翻訳作業で独立に再検証していません。
>
> 原文上の不整合: Wasm の要素対応範囲について、record/tuple を含むとする段落と、scalar/String/文字列ペアに限定する後続段落が併存します。訳では訂正せず保持しており、現在の対応範囲を独立確認したものではありません。

乱数生成。import random、effect。

`random.int`、`random.float`、`random.choice`、`random.shuffle` は暗号学的に安全ではありません。秘密（token、key、password、nonce）には使わないでください。秘密には、OS の暗号学的乱数生成器から取得する唯一のソースである `random.bytes` を使います。

### `random.int(min: Int, max: Int) -> Int`

min から max（両端を含む）のランダムな整数を生成します。

```almd check
import random

effect fn main() -> Unit = {
  println(int.to_string(random.int(1, 100))) // => 42
}
```

### `random.float() -> Float`

`[0, 1)` のランダムな float を生成します。53 ランダムビットを 2^-53 倍するため、全ターゲットで `0.0` はあり得ますが `1.0` はありません。

```almd check
import random

effect fn main() -> Unit = {
  println(float.to_string(random.float())) // => 0.7321
}
```

### `random.choice(xs: List[A]) -> Option[A]`

リストからランダムな要素を選びます。空なら none。

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

**Wasm の要素対応範囲:** `choice` と `shuffle` は record と tuple を含む構造的な list 要素表現を使います。scalar、string、string pair だけに限定されません。

### `random.shuffle(xs: List[A]) -> List[A]`

リストをランダムに並べ替えたコピーを返します。

```almd check
import random

effect fn main() -> Unit = {
  println("${random.shuffle([1, 2, 3])}") // => [3, 1, 2]
}
```

**wasm の要素対応範囲**（#1169）: scalar、`String`、`(String, String)` 要素は wasm 側で動きます（pair 版は `list.swap` で COW コピー内の raw slot を交換）。ほかの要素型は正直に wall として拒否し、native 側で動きます。

### `random.bytes(n: Int) -> Bytes`

プラットフォームの暗号学的に安全な乱数源（CSPRNG）からの `n` バイト。native では OS（Linux は `getrandom`、macOS は `getentropy`）、wasm では `wasi:random` を使います。すべての秘密に使ってください。`n = 0` は空の `Bytes`、負の `n` は `Error: random.bytes: negative length <n>` で abort（exit 1）します。

```almd check
import random
import hex

effect fn main() -> Unit = {
  let key = random.bytes(32)
  println(int.to_string(bytes.len(key))) // => 32
  println(hex.encode(random.bytes(4))) // => 9f3a0c7e
}
```



<a id="signature-index-10-functions"></a>

## 固定スナップショットの API 索引 (10 関数)

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



## API 説明の訳（上記署名順）

- `effect random.int(min: Int, max: Int) -> Int` — [min, max] のランダムな Int。max <= min なら min。秘密には使わず random.bytes を使います。
- `effect random.float() -> Float` — [0, 1) のランダムな Float。秘密には使わず random.bytes を使います。
- `effect random.choice(xs: List[A]) -> Option[A]` — xs のランダムな要素。空なら none。秘密には使わず random.bytes を使います。
- `effect random.shuffle(xs: List[A]) -> List[A]` — xs のランダム順列を新しいリストで返します。秘密には使わず random.bytes を使います。
- `effect random.bytes(n: Int) -> Bytes` — OS の暗号学的乱数源（CSPRNG）から n バイト。n < 0 は abort。
- `random.seeded(seed: Int) -> Rng` — seed により乱数列が固定される generator。
- `random.next_int(rng: Rng, min: Int, max: Int) -> (Int, Rng)` — [min, max]（両端を含む）の Int と次の状態。max <= min なら min を返し、状態は変更しません。
- `random.next_float(rng: Rng) -> (Float, Rng)` — 上位 53 ビットからの [0, 1) の Float と次の状態。
- `random.next_choice(rng: Rng, xs: List[A]) -> (Option[A], Rng)` — 一様に選んだ要素と次の状態。xs が空なら none で状態変更なし。
- `random.next_shuffle(rng: Rng, xs: List[A]) -> (List[A], Rng)` — xs の一様な順列（Fisher-Yates）と次の状態。
