# random

Random number generation. import random, effect.

`random.int`, `random.float`, `random.choice` and `random.shuffle` are not
cryptographically secure: do not use them for secrets (tokens, keys,
passwords, nonces). For secrets use `random.bytes`, the one source drawn from
the operating system's cryptographic random generator.

### `random.int(min: Int, max: Int) -> Int`

Generate a random integer between min and max (inclusive).

```almd check
import random

effect fn main() -> Unit = {
  println(int.to_string(random.int(1, 100))) // => 42
}
```

### `random.float() -> Float`

Generate a random float in `[0, 1)` — 53 random bits scaled by 2^-53, so
`0.0` is possible and `1.0` is not, on every target.

```almd check
import random

effect fn main() -> Unit = {
  println(float.to_string(random.float())) // => 0.7321
}
```

### `random.choice(xs: List[A]) -> Option[A]`

Pick a random element from a list, or none if empty.

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

**Wasm element coverage:** `choice` and `shuffle` use the structural list
element representation, including records and tuples; they are not limited
to scalar values, strings, or string pairs.

### `random.shuffle(xs: List[A]) -> List[A]`

Return a randomly shuffled copy of a list.

```almd check
import random

effect fn main() -> Unit = {
  println("${random.shuffle([1, 2, 3])}") // => [3, 1, 2]
}
```

**wasm element coverage** (#1169): scalar, `String`, and `(String, String)`
elements run on the wasm leg (the pair variant swaps raw slots within the COW
copy via `list.swap`); any other element type walls honestly and runs via the
native leg.

### `random.bytes(n: Int) -> Bytes`

`n` bytes from the platform's cryptographically secure random source (CSPRNG):
the operating system's (`getrandom` on Linux, `getentropy` on macOS) on native,
`wasi:random` on wasm. Use it for every secret. `n = 0` gives empty `Bytes`; a
negative `n` aborts with `Error: random.bytes: negative length <n>` (exit 1).

```almd check
import random
import hex

effect fn main() -> Unit = {
  let key = random.bytes(32)
  println(int.to_string(bytes.len(key))) // => 32
  println(hex.encode(random.bytes(4))) // => 9f3a0c7e
}
```

<!-- BEGIN GENERATED SIGNATURE INDEX (make stdlib-docs) — do not edit by hand -->

## Signature index (10 functions)

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

<!-- END GENERATED SIGNATURE INDEX -->
