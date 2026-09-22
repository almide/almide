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

Return e raised to the given power. The vendored libm guarantees the result within 1 ulp of the true value and byte-identical across targets (C-305) — not correct rounding: `math.exp(1.0)` is one ulp above `math.e()`, which is the correctly rounded constant.

```almd run
fn main() -> Unit = {
  println(float.to_string(math.exp(0.0)))
  println(float.to_string(math.exp(1.0)))
  println(float.to_string(math.e()))
}
```
```output
1.0
2.7182818284590455
2.718281828459045
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

Return the binomial coefficient C(n, k) = n! / (k! * (n-k)!), exactly whenever it fits in an
Int (`choose(62, 31)` = 465428353255261088); a larger result wraps modulo 2^64 like every Int
product. `k < 0` or `k > n` gives 0.

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
// Magnitude of n; min_value wraps to itself.
// @since 0.5.0 or earlier
math.abs(n: Int) -> Int

// Arctangent in radians, within -pi/2..pi/2.
// @since 0.30.0 or earlier
math.atan(x: Float) -> Float

// Binomial n over k; 0 if k < 0 or k > n; overflow wraps.
// @since 0.5.13 or earlier
math.choose(n: Int, k: Int) -> Int

// Cosine of x radians; NaN for infinite x.
// @since 0.5.0 or earlier
math.cos(x: Float) -> Float

// Euler's number, 2.718281828459045.
// @since 0.5.0 or earlier
math.e() -> Float

// e to the x; overflows to inf above ~709.78.
// @since 0.5.0 or earlier
math.exp(x: Float) -> Float

// Product 1..n; 1 for n <= 0; wraps past 20.
// @since 0.5.13 or earlier
math.factorial(n: Int) -> Int

// Greater; a NaN operand is ignored; 0.0 > -0.0.
// @since 0.5.13 or earlier
math.fmax(a: Float, b: Float) -> Float

// Lesser; a NaN operand is ignored; -0.0 < 0.0.
// @since 0.5.13 or earlier
math.fmin(a: Float, b: Float) -> Float

// base to the exp; NaN for negative base, fractional exp.
// @since 0.5.13 or earlier
math.fpow(base: Float, exp: Float) -> Float

// Natural log; -inf at 0, NaN for negative x.
// @since 0.5.0 or earlier
math.log(x: Float) -> Float

// Base-10 log; -inf at 0, NaN for negative x.
// @since 0.5.13 or earlier
math.log10(x: Float) -> Float

// Base-2 log; -inf at 0, NaN for negative x.
// @since 0.5.13 or earlier
math.log2(x: Float) -> Float

// ln Gamma(x), Lanczos; inf at 0, unreliable for x < 0.
// @since 0.5.13 or earlier
math.log_gamma(x: Float) -> Float

// Greater of a and b.
// @since 0.5.0 or earlier
math.max(a: Int, b: Int) -> Int

// Lesser of a and b.
// @since 0.5.0 or earlier
math.min(a: Int, b: Int) -> Int

// The circle constant, 3.141592653589793.
// @since 0.5.0 or earlier
math.pi() -> Float

// base to the exp, wrapping; negative exp aborts.
// @since 0.5.0 or earlier
math.pow(base: Int, exp: Int) -> Int

// -1, 0 or 1 by the sign of n.
// @since 0.5.13 or earlier
math.sign(n: Int) -> Int

// Sine of x radians; NaN for infinite x.
// @since 0.5.0 or earlier
math.sin(x: Float) -> Float

// Square root; NaN below 0, -0.0 stays -0.0.
// @since 0.5.0 or earlier
math.sqrt(x: Float) -> Float

// Tangent of x radians; NaN for infinite x.
// @since 0.5.0 or earlier
math.tan(x: Float) -> Float

// Hyperbolic tangent in -1..1; inf gives 1.0.
// @since 0.30.0 or earlier
math.tanh(x: Float) -> Float
```

<!-- END GENERATED SIGNATURE INDEX -->
