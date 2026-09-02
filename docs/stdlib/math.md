# math

Mathematical functions. import math.

### `math.min(a: Int, b: Int) -> Int`

Return the smaller of two integers.

```almd run
fn main() -> Unit = {
  println("${math.min(3, 7)}")
}
```
```output
3
```

### `math.max(a: Int, b: Int) -> Int`

Return the larger of two integers.

```almd run
fn main() -> Unit = {
  println("${math.max(3, 7)}")
}
```
```output
7
```

### `math.abs(n: Int) -> Int`

Return the absolute value of an integer.

```almd run
fn main() -> Unit = {
  println("${math.abs(-5)}")
}
```
```output
5
```

### `math.pow(base: Int, exp: Int) -> Int`

Raise an integer base to an integer exponent.

```almd run
fn main() -> Unit = {
  println("${math.pow(2, 10)}")
}
```
```output
1024
```

### `math.pi() -> Float`

Return the mathematical constant pi (3.14159...).

```almd run
fn main() -> Unit = {
  println(float.to_string(math.pi()))
}
```
```output
3.141592653589793
```

### `math.e() -> Float`

Return Euler's number e (2.71828...).

```almd run
fn main() -> Unit = {
  println(float.to_string(math.e()))
}
```
```output
2.718281828459045
```

### `math.sin(x: Float) -> Float`

Return the sine of an angle in radians.

```almd run
fn main() -> Unit = {
  println(float.to_string(math.sin(0.0)))
}
```
```output
0.0
```

### `math.cos(x: Float) -> Float`

Return the cosine of an angle in radians.

```almd run
fn main() -> Unit = {
  println(float.to_string(math.cos(0.0)))
}
```
```output
1.0
```

### `math.tan(x: Float) -> Float`

Return the tangent of an angle in radians.

```almd run
fn main() -> Unit = {
  println(float.to_string(math.tan(0.0)))
}
```
```output
0.0
```

### `math.log(x: Float) -> Float`

Return the natural logarithm (base e) of a float.

```almd run
fn main() -> Unit = {
  println(float.to_string(math.log(1.0)))
}
```
```output
0.0
```

### `math.exp(x: Float) -> Float`

Return e raised to the given power.

```almd
math.exp(1.0) // => 2.718281828459045
```

### `math.sqrt(x: Float) -> Float`

Return the square root of a float.

```almd run
fn main() -> Unit = {
  println(float.to_string(math.sqrt(16.0)))
}
```
```output
4.0
```

### `math.log10(x: Float) -> Float`

Return the base-10 logarithm of a float.

```almd run
fn main() -> Unit = {
  println(float.to_string(math.log10(100.0)))
}
```
```output
2.0
```

### `math.log2(x: Float) -> Float`

Return the base-2 logarithm of a float.

```almd run
fn main() -> Unit = {
  println(float.to_string(math.log2(8.0)))
}
```
```output
3.0
```

### `math.sign(n: Int) -> Int`

Return the sign of an integer: -1, 0, or 1.

```almd run
fn main() -> Unit = {
  println("${math.sign(-42)}")
}
```
```output
-1
```

### `math.fmin(a: Float, b: Float) -> Float`

Return the smaller of two floats.

```almd run
fn main() -> Unit = {
  println(float.to_string(math.fmin(1.5, 2.5)))
}
```
```output
1.5
```

### `math.fmax(a: Float, b: Float) -> Float`

Return the larger of two floats.

```almd run
fn main() -> Unit = {
  println(float.to_string(math.fmax(1.5, 2.5)))
}
```
```output
2.5
```

### `math.fpow(base: Float, exp: Float) -> Float`

Raise a float base to a float exponent.

```almd run
fn main() -> Unit = {
  println(float.to_string(math.fpow(2.0, 0.5)))
}
```
```output
1.4142135623730951
```

### `math.factorial(n: Int) -> Int`

Return the factorial of a non-negative integer.

```almd run
fn main() -> Unit = {
  println("${math.factorial(5)}")
}
```
```output
120
```

### `math.choose(n: Int, k: Int) -> Int`

Return the binomial coefficient C(n, k) = n! / (k! * (n-k)!).

```almd run
fn main() -> Unit = {
  println("${math.choose(5, 2)}")
}
```
```output
10
```

### `math.log_gamma(x: Float) -> Float`

Return the natural logarithm of the gamma function at x.

```almd run
fn main() -> Unit = {
  println(float.to_fixed(math.log_gamma(5.0), 3))
}
```
```output
3.178
```

<!-- BEGIN GENERATED SIGNATURE INDEX (make stdlib-docs) — do not edit by hand -->

## Signature index (23 functions)

```
math.abs(n: Int) -> Int
math.atan(x: Float) -> Float
math.choose(n: Int, k: Int) -> Int
math.cos(x: Float) -> Float
math.e() -> Float
math.exp(x: Float) -> Float
math.factorial(n: Int) -> Int
math.fmax(a: Float, b: Float) -> Float
math.fmin(a: Float, b: Float) -> Float
math.fpow(base: Float, exp: Float) -> Float
math.log(x: Float) -> Float
math.log10(x: Float) -> Float
math.log2(x: Float) -> Float
math.log_gamma(x: Float) -> Float
math.max(a: Int, b: Int) -> Int
math.min(a: Int, b: Int) -> Int
math.pi() -> Float
math.pow(base: Int, exp: Int) -> Int
math.sign(n: Int) -> Int
math.sin(x: Float) -> Float
math.sqrt(x: Float) -> Float
math.tan(x: Float) -> Float
math.tanh(x: Float) -> Float
```

<!-- END GENERATED SIGNATURE INDEX -->
