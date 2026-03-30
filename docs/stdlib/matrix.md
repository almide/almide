# matrix

2D matrix operations. import matrix.

### `matrix.zeros(rows: Int, cols: Int) -> Matrix`

Create a zero-filled matrix.

```almd
matrix.zeros(3, 4)
```

### `matrix.ones(rows: Int, cols: Int) -> Matrix`

Create a matrix filled with ones.

```almd
matrix.ones(3, 4)
```

### `matrix.shape(m: Matrix) -> (Int, Int)`

Get the (rows, cols) shape of a matrix.

```almd
matrix.shape(m) // => (3, 4)
```

### `matrix.transpose(m: Matrix) -> Matrix`

Transpose a matrix.

```almd
matrix.transpose(m)
```

### `matrix.from_lists(rows: List[List[Float]]) -> Matrix`

Create a matrix from a list of row lists.

```almd
matrix.from_lists([[1.0, 2.0], [3.0, 4.0]])
```

### `matrix.to_lists(m: Matrix) -> List[List[Float]]`

Convert a matrix to a list of row lists.

```almd
matrix.to_lists(m) // => [[1.0, 2.0], [3.0, 4.0]]
```

### `matrix.get(m: Matrix, row: Int, col: Int) -> Float`

Get element at (row, col).

```almd
matrix.get(m, 0, 1) // => 2.0
```

### `matrix.rows(m: Matrix) -> Int`

Get the number of rows.

```almd
matrix.rows(m) // => 3
```

### `matrix.cols(m: Matrix) -> Int`

Get the number of columns.

```almd
matrix.cols(m) // => 4
```

### `matrix.add(a: Matrix, b: Matrix) -> Matrix`

Element-wise addition of two matrices.

```almd
matrix.add(a, b)
```

### `matrix.mul(a: Matrix, b: Matrix) -> Matrix`

Matrix multiplication.

```almd
matrix.mul(a, b)
```

### `matrix.scale(m: Matrix, s: Float) -> Matrix`

Multiply all elements by a scalar.

```almd
matrix.scale(m, 2.0)
```

### `matrix.map(m: Matrix, f: fn(Float) -> Float) -> Matrix`

Apply a function to every element.

```almd
matrix.map(m, (x) => x * x)
```
