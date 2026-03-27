<!-- description: Export Almide code as native-speed libraries callable from any language -->
# Rainbow FFI Gate [ON HOLD]

## Thesis

Almide で書いたコードを、どの言語からでもネイティブ速度で呼べるライブラリとして出力する。トランスパイラではなく **ライブラリコンパイラ**。

Almide の強み（row polymorphism + monomorphization + borrow analysis）はすべて Rust コード生成時に発揮される。Python/Ruby に直接トランスパイルすると、これらの強みがすべて消える。だからターゲット言語を増やすのではなく、Almide → Rust → 共有ライブラリ → 各言語バインディング、という経路で各言語に速度を届ける。

```
almide source
    ↓
  Rust codegen（borrow analysis, monomorphization — ゼロコスト）
    ↓
  cdylib (.so / .dylib / .dll)
    ↓
  ┌─ Python ──→ PyO3 バインディング（pip install 可能）
  ├─ Ruby ────→ Magnus バインディング（gem 可能）
  ├─ Node.js ─→ napi-rs バインディング（npm 可能）
  ├─ Swift ───→ C ABI ブリッジ
  ├─ Kotlin ──→ JNI / Panama FFI
  ├─ Erlang ──→ Rustler NIF
  └─ WASM ────→ wasmtime / wasmer（全言語共通）
```

## Syntax

`export` マークされた関数が FFI 境界に公開される:

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
# 共有ライブラリを生成
almide build lib.almd --lib

# 特定言語のバインディング付きで生成
almide build lib.almd --lib --bind python
almide build lib.almd --lib --bind ruby
almide build lib.almd --lib --bind node
almide build lib.almd --lib --bind wasm
```

### 利用側

```python
# Python
from almide_lib import fibonacci, greet, Color

print(fibonacci(40))                    # ← ネイティブ速度
print(greet({"name": "Alice", "age": 30}))
c = Color.Custom(r=255, g=0, b=128)
```

```ruby
# Ruby
require 'almide_lib'

puts AlmideLib.fibonacci(40)           # ← ネイティブ速度
puts AlmideLib.greet({ name: "Alice", age: 30 })
c = AlmideLib::Color.custom(r: 255, g: 0, b: 128)
```

```javascript
// Node.js
const { fibonacci, greet, Color } = require('almide-lib')

console.log(fibonacci(40))            // ← ネイティブ速度
console.log(greet({ name: "Alice", age: 30 }))
const c = Color.Custom({ r: 255, g: 0, b: 128 })
```

## Type Mapping

Almide の型情報がそのままバインディングの型ヒントになる:

| Almide | Rust (内部) | Python | Ruby | Node.js |
|--------|------------|--------|------|---------|
| `Int` | `i64` | `int` | `Integer` | `number` / `bigint` |
| `Float` | `f64` | `float` | `Float` | `number` |
| `String` | `String` | `str` | `String` | `string` |
| `Bool` | `bool` | `bool` | `TrueClass/FalseClass` | `boolean` |
| `List[T]` | `Vec<T>` | `list[T]` | `Array` | `T[]` |
| `Map[K, V]` | `HashMap<K, V>` | `dict[K, V]` | `Hash` | `Map<K, V>` |
| `Option[T]` | `Option<T>` | `T \| None` | `T \| nil` | `T \| undefined` |
| `Result[T, E]` | `Result<T, E>` | 例外に変換 | 例外に変換 | 例外に変換 |
| `{ x: Int, y: Int }` | `struct` | `TypedDict` / `dataclass` | `Struct` / `Hash` | `interface` |
| `\| A(T) \| B` | `enum` | `class` (tagged union) | `class` (tagged union) | `class` (tagged union) |

### Record → 構造体

`export` された関数の open record パラメータは、monomorphization 後の具体型が公開される:

```almide
export fn area(shape: { width: Float, height: Float, .. }) -> Float =
  shape.width * shape.height
```

```python
# Python 側: dict でも dataclass でもOK
area({"width": 3.0, "height": 4.0})
area({"width": 3.0, "height": 4.0, "color": "red"})  # 余分なフィールドは無視
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

`almide build lib.almd --lib --bind python` が生成するもの:

```
dist/
├── src/
│   └── lib.rs              # Almide → Rust 生成コード
├── bindings/
│   └── python/
│       ├── almide_lib/
│       │   ├── __init__.py  # Python API（PyO3 ラッパー）
│       │   └── __init__.pyi # 型スタブ（IDE 補完用）
│       ├── Cargo.toml       # PyO3 依存
│       ├── pyproject.toml   # pip install 用
│       └── src/
│           └── lib.rs       # #[pyfunction] / #[pyclass] 自動生成
├── Cargo.toml
└── build.sh                 # maturin build のラッパー
```

### 生成される Rust FFI コード（Python 向け）

```rust
// bindings/python/src/lib.rs（自動生成）
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

Almide コンパイラが型情報を使って `#[pyfunction]` / `#[pyclass]` を自動生成する。Rust ユーザーが手で書くボイラープレートがゼロになる。

## WASM Path

WASM 経由なら全言語で即座に使える。バインディング生成不要:

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

WASM は言語間ポータビリティが最大だが、GC型（String, List, Map）の受け渡しに component model が必要。primitive 型（Int, Float, Bool）なら今すぐ動く。

## Implementation Phases

### Phase 1: `--lib` 基盤 (P0)

`almide build --lib` で cdylib を出力する:

- [ ] `export` キーワードの構文追加（parser）
- [ ] `export` 関数の型制約チェック（FFI 安全な型のみ許可）
- [ ] `--lib` フラグで `fn main` なしのコード生成
- [ ] Cargo.toml に `crate-type = ["cdylib", "rlib"]` を出力
- [ ] C ABI ヘッダー（`.h`）の自動生成

### Phase 2: Python バインディング (P0)

最も需要が大きい。PyO3 + maturin:

- [ ] `--bind python` で PyO3 ラッパー自動生成
- [ ] 型マッピング: Almide 型 → PyO3 型変換コード
- [ ] Record → PyDict 変換
- [ ] Variant → Python class 生成
- [ ] `.pyi` スタブ自動生成（IDE 補完）
- [ ] `pyproject.toml` 生成（pip install 対応）

### Phase 3: Node.js バインディング (P1)

napi-rs 経由:

- [ ] `--bind node` で napi-rs ラッパー自動生成
- [ ] `.d.ts` 型定義生成
- [ ] `package.json` 生成（npm publish 対応）

### Phase 4: Ruby バインディング (P1)

Magnus 経由:

- [ ] `--bind ruby` で Magnus ラッパー自動生成
- [ ] `.gemspec` 生成

### Phase 5: WASM バインディング (P2)

wasm-bindgen / component model:

- [ ] `--bind wasm` で WASM モジュール生成
- [ ] component model 対応（String, List の受け渡し）
- [ ] WAI / WIT 定義ファイル生成

## Why Not Transpile?

Python/Ruby に直接トランスパイルしない理由:

| | トランスパイル | ライブラリ FFI |
|---|---|---|
| 実行速度 | Python/Ruby の速度 | Rust ネイティブ速度 |
| borrow analysis | 無意味（GC 言語） | 完全に活きる |
| monomorphization | 無意味（動的型付け） | 完全に活きる |
| ランタイム実装 | 282関数 × 各言語 | 不要（Rust 実装を共有） |
| 追加コード量 | ~31,000行 / 言語 | ~2,000行 / 言語 |
| エコシステム統合 | 中途半端な互換 | pip/gem/npm にそのまま乗る |
| 型安全性 | 消える | バインディング側で型ヒント提供 |

**Almide の価値は「書きやすさ × 速さ」。** トランスパイルは速さを捨てる。FFI は両方残す。

## Priority

`--lib` 基盤 > Python バインディング > Node.js > Ruby > WASM component model
