<!-- description: Export Almide code as native-speed libraries callable from any language -->
<!-- done: 2026-03-28 -->
# Rainbow FFI Gate

> Realized via almide-lander: Almide → Rust codegen → cdylib + language bindings

## Thesis

Output code written in Almide as libraries callable from any language at native speed. Not a transpiler — a **library compiler**.

Almide's strengths (row polymorphism + monomorphization + borrow analysis) are all realized during Rust code generation. Transpiling directly to Python/Ruby would erase all of these strengths. So instead of adding target languages, we deliver speed to each language via the path: Almide → Rust → shared library → language bindings.

```
almide source
    ↓
  Rust codegen (borrow analysis, monomorphization — zero cost)
    ↓
  cdylib (.so / .dylib / .dll)
    ↓
  ┌─ Python ──→ PyO3 bindings (pip installable)
  ├─ Ruby ────→ Magnus bindings (gem installable)
  ├─ Node.js ─→ napi-rs bindings (npm installable)
  ├─ Swift ───→ C ABI bridge
  ├─ Kotlin ──→ JNI / Panama FFI
  ├─ Erlang ──→ Rustler NIF
  └─ WASM ────→ wasmtime / wasmer (universal across languages)
```

## Syntax

Functions marked with `export` are exposed at the FFI boundary:

```almide
// lib.almd
export fn fibonacci(n: Int) -> Int =
  if n <= 1 then n
  else fibonacci(n - 1) + fibonacci(n - 2)

export fn greet(user: { name: String, age: Int }) -> String =
  "Hello, ${user.name} (${int.to_string(user.age)})"

export type Color =
  | Red
  | Green
  | Blue
  | Custom({ r: Int, g: Int, b: Int })
```

```bash
# Generate shared library
almide build lib.almd --lib

# Generate with bindings for a specific language
almide build lib.almd --lib --bind python
almide build lib.almd --lib --bind ruby
almide build lib.almd --lib --bind node
almide build lib.almd --lib --bind wasm
```

### Consumer Side

```python
# Python
from almide_lib import fibonacci, greet, Color

print(fibonacci(40))                    # ← native speed
print(greet({"name": "Alice", "age": 30}))
c = Color.Custom(r=255, g=0, b=128)
```

```ruby
# Ruby
require 'almide_lib'

puts AlmideLib.fibonacci(40)           # ← native speed
puts AlmideLib.greet({ name: "Alice", age: 30 })
c = AlmideLib::Color.custom(r: 255, g: 0, b: 128)
```

```javascript
// Node.js
const { fibonacci, greet, Color } = require('almide-lib')

console.log(fibonacci(40))            // ← native speed
console.log(greet({ name: "Alice", age: 30 }))
const c = Color.Custom({ r: 255, g: 0, b: 128 })
```

## Type Mapping

Almide's type information directly becomes type hints for bindings:

| Almide | Rust (internal) | Python | Ruby | Node.js |
|--------|------------|--------|------|---------|
| `Int` | `i64` | `int` | `Integer` | `number` / `bigint` |
| `Float` | `f64` | `float` | `Float` | `number` |
| `String` | `String` | `str` | `String` | `string` |
| `Bool` | `bool` | `bool` | `TrueClass/FalseClass` | `boolean` |
| `List[T]` | `Vec<T>` | `list[T]` | `Array` | `T[]` |
| `Map[K, V]` | `HashMap<K, V>` | `dict[K, V]` | `Hash` | `Map<K, V>` |
| `Option[T]` | `Option<T>` | `T \| None` | `T \| nil` | `T \| undefined` |
| `Result[T, E]` | `Result<T, E>` | Converted to exception | Converted to exception | Converted to exception |
| `{ x: Int, y: Int }` | `struct` | `TypedDict` / `dataclass` | `Struct` / `Hash` | `interface` |
| `\| A(T) \| B` | `enum` | `class` (tagged union) | `class` (tagged union) | `class` (tagged union) |

### Record → Struct

For `export` functions with open record parameters, the concrete type after monomorphization is exposed:

```almide
export fn area(shape: { width: Float, height: Float, .. }) -> Float =
  shape.width * shape.height
```

```python
# Python side: dict or dataclass both work
area({"width": 3.0, "height": 4.0})
area({"width": 3.0, "height": 4.0, "color": "red"})  # Extra fields are ignored
```

### Variant → Tagged Union

```almide
export type Shape =
  | Circle(Float)
  | Rect({ width: Float, height: Float })
```

```python
# Python
s = Shape.Circle(5.0)
s = Shape.Rect(width=3.0, height=4.0)

match s:
    case Shape.Circle(r): print(f"circle r={r}")
    case Shape.Rect(w, h): print(f"rect {w}x{h}")
```

## Generated Output

`almide build lib.almd --lib --bind python` generates:

```
dist/
├── src/
│   └── lib.rs              # Almide → Rust generated code
├── bindings/
│   └── python/
│       ├── almide_lib/
│       │   ├── __init__.py  # Python API (PyO3 wrapper)
│       │   └── __init__.pyi # Type stubs (for IDE completion)
│       ├── Cargo.toml       # PyO3 dependency
│       ├── pyproject.toml   # For pip install
│       └── src/
│           └── lib.rs       # Auto-generated #[pyfunction] / #[pyclass]
├── Cargo.toml
└── build.sh                 # Wrapper for maturin build
```

### Generated Rust FFI Code (for Python)

```rust
// bindings/python/src/lib.rs (auto-generated)
use pyo3::prelude::*;
use almide_lib;

#[pyfunction]
fn fibonacci(n: i64) -> i64 {
    almide_lib::fibonacci(n)
}

#[pyfunction]
fn greet(user: &PyDict) -> PyResult<String> {
    let name: String = user.get_item("name")?.extract()?;
    let age: i64 = user.get_item("age")?.extract()?;
    Ok(almide_lib::greet(&AlmideUser { name, age }))
}

#[pymodule]
fn almide_lib(_py: Python, m: &PyModule) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(fibonacci, m)?)?;
    m.add_function(wrap_pyfunction!(greet, m)?)?;
    Ok(())
}
```

The Almide compiler uses type information to auto-generate `#[pyfunction]` / `#[pyclass]`. The boilerplate that Rust users write by hand drops to zero.

## WASM Path

Via WASM, immediately usable from all languages. No binding generation needed:

```bash
almide build lib.almd --lib --bind wasm    # → lib.wasm + lib.d.ts
```

```python
# Python (wasmtime)
from wasmtime import Store, Module, Instance
store = Store()
module = Module.from_file(store.engine, "lib.wasm")
instance = Instance(store, module, [])
print(instance.exports(store)["fibonacci"](40))
```

WASM has maximum cross-language portability, but passing GC types (String, List, Map) requires the component model. Primitive types (Int, Float, Bool) work immediately.

## Implementation Phases

### Phase 1: `--lib` Foundation (P0)

Output cdylib with `almide build --lib`:

- [ ] Add `export` keyword syntax (parser)
- [ ] Type constraint check for `export` functions (only FFI-safe types allowed)
- [ ] Code generation without `fn main` via `--lib` flag
- [ ] Output `crate-type = ["cdylib", "rlib"]` in Cargo.toml
- [ ] Auto-generate C ABI header (`.h`)

### Phase 2: Python Bindings (P0)

Highest demand. PyO3 + maturin:

- [ ] Auto-generate PyO3 wrapper with `--bind python`
- [ ] Type mapping: Almide type → PyO3 type conversion code
- [ ] Record → PyDict conversion
- [ ] Variant → Python class generation
- [ ] Auto-generate `.pyi` stubs (IDE completion)
- [ ] Generate `pyproject.toml` (pip install support)

### Phase 3: Node.js Bindings (P1)

Via napi-rs:

- [ ] Auto-generate napi-rs wrapper with `--bind node`
- [ ] Generate `.d.ts` type definitions
- [ ] Generate `package.json` (npm publish support)

### Phase 4: Ruby Bindings (P1)

Via Magnus:

- [ ] Auto-generate Magnus wrapper with `--bind ruby`
- [ ] Generate `.gemspec`

### Phase 5: WASM Bindings (P2)

wasm-bindgen / component model:

- [ ] Generate WASM module with `--bind wasm`
- [ ] Component model support (String, List passing)
- [ ] Generate WAI / WIT definition files

## Why Not Transpile?

Reasons for not transpiling directly to Python/Ruby:

| | Transpile | Library FFI |
|---|---|---|
| Execution speed | Python/Ruby speed | Rust native speed |
| Borrow analysis | Meaningless (GC language) | Fully utilized |
| Monomorphization | Meaningless (dynamic typing) | Fully utilized |
| Runtime implementation | 282 functions × each language | Not needed (shared Rust implementation) |
| Additional code | ~31,000 lines / language | ~2,000 lines / language |
| Ecosystem integration | Half-baked compatibility | Rides directly on pip/gem/npm |
| Type safety | Lost | Type hints provided by bindings |

**Almide's value is "ease of writing x speed."** Transpiling sacrifices speed. FFI preserves both.

## Priority

`--lib` foundation > Python bindings > Node.js > Ruby > WASM component model
