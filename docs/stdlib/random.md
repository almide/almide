# random

Random number generation. import random, effect.

### `random.int(min: Int, max: Int) -> Int`

Generate a random integer between min and max (inclusive).

```almd check
import random

effect fn main() -> Unit = {
  println(int.to_string(random.int(1, 100))) // => 42
}
```

### `random.float() -> Float`

Generate a random float between 0.0 and 1.0.

```almd check
import random

effect fn main() -> Unit = {
  println(float.to_string(random.float())) // => 0.7321
}
```

### `random.choice(xs: List[T]) -> Option[T]`

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

**wasm element coverage** (#1169): scalar (`Int`/`Float`/`Bool`), `String`, and
`(String, String)` elements run on the wasm leg; any other element type walls
honestly (`random.choice_x` unlinked) and runs via the native leg.

### `random.shuffle(xs: List[T]) -> List[T]`

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

<!-- BEGIN GENERATED SIGNATURE INDEX (make stdlib-docs) — do not edit by hand -->

## Signature index (4 functions)

```
effect random.int(min: Int, max: Int) -> Int
effect random.float() -> Float
effect random.choice(xs: List[T]) -> Option[T]
effect random.shuffle(xs: List[T]) -> List[T]
```

<!-- END GENERATED SIGNATURE INDEX -->
