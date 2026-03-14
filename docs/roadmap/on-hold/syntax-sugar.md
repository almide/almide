# Syntax Sugar [ON HOLD]

## Range Literals ✅
```almide
let xs = 0..10        // [0, 1, 2, ..., 9]
let ys = 0..=10       // [0, 1, 2, ..., 10]
let zs = 10..0..-1    // [10, 9, 8, ..., 1]
```

## List Comprehensions
```almide
let evens = [x for x in 0..100 if x % 2 == 0]
let pairs = [(x, y) for x in xs for y in ys]
```

## Default Arguments
```almide
fn greet(name: String, greeting: String = "Hello") -> String =
  "${greeting}, ${name}!"
```

## Named Arguments
```almide
http.response(status: 200, body: "OK")
```

## Exhaustiveness Checking for Pattern Match ✅
Detects at compile time when a match on a variant type does not cover all cases. Implemented in `src/check/mod.rs`.

```almide
type Color = Red | Green | Blue

fn name(c: Color) -> String = match c {
  Red => "red",
  Green => "green",
  // warning: non-exhaustive match, missing: Blue
}
```

## Raw String Literals
```almide
let regex_pattern = r"^\d{3}-\d{4}$"
let path = r"C:\Users\test"
```

## Block Comments
```almide
/*
  multi-line comment
  currently only // is supported
*/
```

## Priority
~~Range literals~~ ✅ > ~~exhaustiveness checking~~ ✅ > block comments > list comprehensions > default arguments > raw strings
