# option

Option type operations. auto-imported.

### `option.map(o: Option[A], f: Fn[A] -> B) -> Option[B]`

Transform the inner value using a function. If none, returns none.

```almd run
fn show(o: Option[Int]) -> String = match o {
  some(n) => "some(${n})",
  none => "none",
}

fn main() -> Unit = {
  println(show(option.map(some(2), (x) => x * 10)))
}
```
```output
some(20)
```

### `option.flat_map(o: Option[A], f: Fn[A] -> Option[B]) -> Option[B]`

Chain an Option-returning function on the inner value. Flattens nested Options.

```almd run
fn show(o: Option[Int]) -> String = match o {
  some(n) => "some(${n})",
  none => "none",
}

fn main() -> Unit = {
  println(show(option.flat_map(some(5), (x) => if x > 0 then some(x) else none)))
  println(show(option.flat_map(some(-5), (x) => if x > 0 then some(x) else none)))
}
```
```output
some(5)
none
```

### `option.flatten(o: Option[Option[A]]) -> Option[A]`

Flatten a nested Option. some(some(x)) becomes some(x), some(none) becomes none.

```almd run
fn show(o: Option[Int]) -> String = match o {
  some(n) => "some(${n})",
  none => "none",
}

fn main() -> Unit = {
  println(show(option.flatten(some(some(42)))))
  println(show(option.flatten(some(none))))
}
```
```output
some(42)
none
```

### `option.unwrap_or(o: Option[A], default: A) -> A`

Get the inner value, or return a default if none.

```almd run
fn main() -> Unit = {
  println("${option.unwrap_or(none, 0)}")
  println("${option.unwrap_or(some(7), 0)}")
}
```
```output
0
7
```

### `option.unwrap_or_else(o: Option[A], f: Fn[Unit] -> A) -> A`

Get the inner value, or compute a default using a function.

```almd run
fn main() -> Unit = {
  println("${option.unwrap_or_else(none, () => 42)}")
}
```
```output
42
```

### `option.is_some(o: Option[A]) -> Bool`

Check if the Option contains a value.

```almd run
fn main() -> Unit = {
  println("${option.is_some(some(42))}")
}
```
```output
true
```

### `option.is_none(o: Option[A]) -> Bool`

Check if the Option is none.

```almd run
fn main() -> Unit = {
  println("${option.is_none(none: Option[Int])}")
}
```
```output
true
```

### `option.to_result(o: Option[A], err: String) -> Result[A, String]`

Convert some to ok, none to err with the given error message.

```almd run
fn show(r: Result[Int, String]) -> String = match r {
  ok(n) => "ok(${n})",
  err(e) => "err(\"${e}\")",
}

fn main() -> Unit = {
  println(show(option.to_result(some(42), "missing")))
  println(show(option.to_result(none, "missing")))
}
```
```output
ok(42)
err("missing")
```

### `option.filter(o: Option[A], f: Fn[A] -> Bool) -> Option[A]`

Keep the value if it satisfies the predicate, otherwise return none.

```almd run
fn show(o: Option[Int]) -> String = match o {
  some(n) => "some(${n})",
  none => "none",
}

fn main() -> Unit = {
  println(show(option.filter(some(5), (x) => x > 3)))
  println(show(option.filter(some(2), (x) => x > 3)))
}
```
```output
some(5)
none
```

### `option.zip(a: Option[A], b: Option[B]) -> Option[(A, B)]`

Combine two Options into an Option of a tuple. None if either is none.

```almd run
fn show(o: Option[(Int, Int)]) -> String = match o {
  some((a, b)) => "some((${a}, ${b}))",
  none => "none",
}

fn main() -> Unit = {
  println(show(option.zip(some(1), some(2))))
  println(show(option.zip(some(1), none)))
}
```
```output
some((1, 2))
none
```

### `option.or_else(o: Option[A], f: Fn[Unit] -> Option[A]) -> Option[A]`

Return the Option if some, otherwise call the function to produce an alternative.

```almd run
fn show(o: Option[Int]) -> String = match o {
  some(n) => "some(${n})",
  none => "none",
}

fn main() -> Unit = {
  println(show(option.or_else(none, () => some(42))))
  println(show(option.or_else(some(1), () => some(42))))
}
```
```output
some(42)
some(1)
```

### `option.to_list(o: Option[A]) -> List[A]`

Convert some(x) to [x], none to [].

```almd run
fn main() -> Unit = {
  println("${option.to_list(some(42))}")
  println("${option.to_list(none: Option[Int])}")
}
```
```output
[42]
[]
```

<!-- BEGIN GENERATED SIGNATURE INDEX (make stdlib-docs) — do not edit by hand -->

## Signature index (14 functions)

```
option.map(o: Option[A], f: (A) -> B) -> Option[B]
option.flat_map(o: Option[A], f: (A) -> Option[B]) -> Option[B]
option.flatten(o: Option[Option[A]]) -> Option[A]
option.unwrap_or(o: Option[A], default: A) -> A
option.unwrap_or_else(o: Option[A], f: () -> A) -> A
option.is_some(o: Option[A]) -> Bool
option.is_none(o: Option[A]) -> Bool
option.to_result(o: Option[A], e: E) -> Result[A, E]
option.filter(o: Option[A], f: (A) -> Bool) -> Option[A]
option.zip(a: Option[A], b: Option[B]) -> Option[()]
option.or_else(o: Option[A], f: () -> Option[A]) -> Option[A]
option.to_list(o: Option[A]) -> List[A]
option.collect(xs: List[Option[T]]) -> Option[List[T]]
option.collect_map(xs: List[T], f: (T) -> Option[U]) -> Option[List[U]]
```

<!-- END GENERATED SIGNATURE INDEX -->
