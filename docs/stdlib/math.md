# math

Mathematical functions. import math.

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

### `math.factorial(n: Int) -> Int`

Return the factorial of a non-negative integer (1 for n <= 1). The product wraps modulo 2^64
like every Int product, so it is wrong past 20 and 0 for every n >= 66: from there on n!
holds at least 64 factors of two (v2(66!) = 66 - popcount(66) = 64), and the answer comes
back at once instead of after n multiplications.

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

Return `ln|Γ(x)|`, the natural logarithm of the absolute gamma function, on the whole real
line (negative non-integers included). The non-positive integers are poles and give `inf`.

```almd run
fn main() -> Unit = {
  println(float.to_fixed(math.log_gamma(5.0), 3))
}
```
```output
3.178
```

<!-- BEGIN GENERATED SIGNATURE INDEX (make stdlib-docs) — do not edit by hand -->

## Signature index (27 functions)

```
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

// Product 1..n; 1 for n <= 0; wraps past 20, 0 from 66 on.
// @since 0.5.13 or earlier
math.factorial(n: Int) -> Int

// Natural log; -inf at 0, NaN for negative x.
// @since 0.5.0 or earlier
math.log(x: Float) -> Float

// Base-10 log; -inf at 0, NaN for negative x.
// @since 0.5.13 or earlier
math.log10(x: Float) -> Float

// Base-2 log; -inf at 0, NaN for negative x.
// @since 0.5.13 or earlier
math.log2(x: Float) -> Float

// ln|Gamma(x)| on the whole line; inf at the poles.
// @since 0.5.13 or earlier
math.log_gamma(x: Float) -> Float

// The circle constant, 3.141592653589793.
// @since 0.5.0 or earlier
math.pi() -> Float

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

// Arcsine in radians, within -pi/2..pi/2; NaN outside -1..1.
// @since unreleased
math.asin(x: Float) -> Float

// Arccosine in radians, within 0..pi; NaN outside -1..1.
// @since unreleased
math.acos(x: Float) -> Float

// Angle of the point (x, y) in radians, within -pi..pi.
// @since unreleased
math.atan2(y: Float, x: Float) -> Float

// e to the x, minus 1, accurate near 0; -1.0 below about -38.
// @since unreleased
math.expm1(x: Float) -> Float

// Natural log of 1 + x, accurate near 0; -inf at -1, NaN below.
// @since unreleased
math.log1p(x: Float) -> Float

// Hyperbolic sine; overflows to +-inf past about 710.
// @since unreleased
math.sinh(x: Float) -> Float

// Hyperbolic cosine, at least 1.0; overflows to inf past about 710.
// @since unreleased
math.cosh(x: Float) -> Float

// sqrt(a*a + b*b) without overflow or underflow on the way.
// @since unreleased
math.hypot(a: Float, b: Float) -> Float

// Real cube root; negative x gives a negative root.
// @since unreleased
math.cbrt(x: Float) -> Float

// Inverse hyperbolic sine.
// @since unreleased
math.asinh(x: Float) -> Float

// Inverse hyperbolic cosine, at least 0.0; NaN below 1.0.
// @since unreleased
math.acosh(x: Float) -> Float

// Inverse hyperbolic tangent; +-inf at +-1, NaN outside -1..1.
// @since unreleased
math.atanh(x: Float) -> Float
```

<!-- END GENERATED SIGNATURE INDEX -->
