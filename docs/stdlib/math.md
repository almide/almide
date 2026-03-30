# math

Mathematical functions. import math.

### `math.min(a: Int, b: Int) -> Int`

Return the smaller of two integers.

```almd
math.min(3, 7) // => 3
```

### `math.max(a: Int, b: Int) -> Int`

Return the larger of two integers.

```almd
math.max(3, 7) // => 7
```

### `math.abs(n: Int) -> Int`

Return the absolute value of an integer.

```almd
math.abs(-5) // => 5
```

### `math.pow(base: Int, exp: Int) -> Int`

Raise an integer base to an integer exponent.

```almd
math.pow(2, 10) // => 1024
```

### `math.pi() -> Float`

Return the mathematical constant pi (3.14159...).

```almd
math.pi() // => 3.141592653589793
```

### `math.e() -> Float`

Return Euler's number e (2.71828...).

```almd
math.e() // => 2.718281828459045
```

### `math.sin(x: Float) -> Float`

Return the sine of an angle in radians.

```almd
math.sin(0.0) // => 0.0
```

### `math.cos(x: Float) -> Float`

Return the cosine of an angle in radians.

```almd
math.cos(0.0) // => 1.0
```

### `math.tan(x: Float) -> Float`

Return the tangent of an angle in radians.

```almd
math.tan(0.0) // => 0.0
```

### `math.log(x: Float) -> Float`

Return the natural logarithm (base e) of a float.

```almd
math.log(1.0) // => 0.0
```

### `math.exp(x: Float) -> Float`

Return e raised to the given power.

```almd
math.exp(1.0) // => 2.718281828459045
```

### `math.sqrt(x: Float) -> Float`

Return the square root of a float.

```almd
math.sqrt(16.0) // => 4.0
```

### `math.log10(x: Float) -> Float`

Return the base-10 logarithm of a float.

```almd
math.log10(100.0) // => 2.0
```

### `math.log2(x: Float) -> Float`

Return the base-2 logarithm of a float.

```almd
math.log2(8.0) // => 3.0
```

### `math.sign(n: Int) -> Int`

Return the sign of an integer: -1, 0, or 1.

```almd
math.sign(-42) // => -1
```

### `math.fmin(a: Float, b: Float) -> Float`

Return the smaller of two floats.

```almd
math.fmin(1.5, 2.5) // => 1.5
```

### `math.fmax(a: Float, b: Float) -> Float`

Return the larger of two floats.

```almd
math.fmax(1.5, 2.5) // => 2.5
```

### `math.fpow(base: Float, exp: Float) -> Float`

Raise a float base to a float exponent.

```almd
math.fpow(2.0, 0.5) // => 1.4142135623730951
```

### `math.factorial(n: Int) -> Int`

Return the factorial of a non-negative integer.

```almd
math.factorial(5) // => 120
```

### `math.choose(n: Int, k: Int) -> Int`

Return the binomial coefficient C(n, k) = n! / (k! * (n-k)!).

```almd
math.choose(5, 2) // => 10
```

### `math.log_gamma(x: Float) -> Float`

Return the natural logarithm of the gamma function at x.

```almd
math.log_gamma(5.0) // => 3.178...
```
