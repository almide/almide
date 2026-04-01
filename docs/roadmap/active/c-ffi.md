<!-- description: Call C libraries from Almide via @extern(c, ...) and extern "C" codegen -->
# C FFI — Call C Libraries from Almide

## Goal

Almide から C ライブラリを直接呼べるようにする。SQLite、OpenSSL、zlib 等の既存 C エコシステムを利用可能にする。

```almide
@extern(c, "sqlite3", "sqlite3_open")
fn sqlite3_open(filename: CString, db: Ptr[Ptr[Opaque]]) -> CInt

@extern(c, "sqlite3", "sqlite3_close")
fn sqlite3_close(db: Ptr[Opaque]) -> CInt
```

生成される Rust:
```rust
#[link(name = "sqlite3")]
extern "C" {
    fn sqlite3_open(filename: *const c_char, db: *mut *mut c_void) -> c_int;
    fn sqlite3_close(db: *mut c_void) -> c_int;
}
```

## What Already Works

| Component | Status | Detail |
|-----------|--------|--------|
| Parser | ✅ ready | `@extern(c, "lib", "func")` は今すぐパースできる。target は自由な識別子 |
| AST | ✅ ready | `ExternAttr { target, module, function }` — 変更不要 |
| IR pipeline | ✅ ready | extern_attrs が IrFunction まで伝搬される |
| `--repr-c` | ✅ ready | struct/enum に `#[repr(C)]` を付ける。C 構造体の受け渡しに必要 |
| Monomorphization | ✅ ready | extern attrs は specialization を通過する |
| Existing test | ✅ ready | `spec/integration/extern/extern_test.almd` が雛形として使える |

## What Needs to Be Built

### 1. Codegen Template for `c` target

現在の `extern_fn` テンプレートは Rust の `use` 文を生成する:
```toml
[extern_fn]
template = "use {module}::{function} as {name};"
```

`c` target 用に `extern "C"` ブロックを生成するテンプレートが必要:
```toml
[extern_c_fn]
template = """
#[link(name = "{module}")]
extern "C" {{ fn {function}({params_c}) -> {return_c}; }}
pub fn {name}({params_almide}) -> {return_almide} {{ unsafe {{ {function}({marshaling}) }} }}
"""
```

2層構造: `extern "C"` 宣言 + safe ラッパー関数（型変換を含む）。

### 2. C FFI Type System

Almide の型と C の型のマッピング。新しい型を追加する。

```
Almide Type     C Type              Rust Type
─────────────   ─────────────       ─────────────
CInt            int                 c_int
CLong           long                c_long
CSize           size_t              usize
CString         const char*         *const c_char
Ptr[T]          T*                  *mut T
Ptr[Opaque]     void*               *mut c_void
Bool            int (0/1)           c_int → bool
Float           double              f64
Int             int64_t             i64
```

設計判断:
- **Almide のプリミティブ型 (Int, Float, String, Bool) は自動変換**。String → CString 変換は safe ラッパーが CString::new() を呼ぶ。
- **FFI 専用型 (CInt, CString, Ptr) は @extern(c) 関数でのみ使用可能**。通常の Almide コードには出現しない。
- **Opaque は不透明ポインタ**。型の中身を知らないまま受け渡すための型。sqlite3* のような C ハンドルに使う。

### 3. Link Directive

`@extern(c, "sqlite3", ...)` の `"sqlite3"` から `#[link(name = "sqlite3")]` を生成する。

同一ライブラリの複数関数を1つの `extern "C"` ブロックにまとめる最適化も必要:
```rust
// 個別に出すと冗長
#[link(name = "sqlite3")] extern "C" { fn sqlite3_open(...); }
#[link(name = "sqlite3")] extern "C" { fn sqlite3_close(...); }

// まとめる
#[link(name = "sqlite3")]
extern "C" {
    fn sqlite3_open(...);
    fn sqlite3_close(...);
}
```

### 4. Safe Wrapper Generation

Almide ユーザーが unsafe を意識しなくていいように、safe ラッパーを自動生成する。

```almide
// ユーザーが書くもの
@extern(c, "math", "sqrt")
fn c_sqrt(x: Float) -> Float
```

生成される Rust:
```rust
#[link(name = "m")]
extern "C" { fn sqrt(x: f64) -> f64; }
pub fn c_sqrt(x: f64) -> f64 { unsafe { sqrt(x) } }
```

String の場合はマーシャリングが入る:
```almide
@extern(c, "mylib", "greet")
fn greet(name: String) -> String
```

```rust
extern "C" { fn greet(name: *const c_char) -> *const c_char; }
pub fn almide_greet(name: String) -> String {
    let c_name = std::ffi::CString::new(name).unwrap();
    let result = unsafe { greet(c_name.as_ptr()) };
    unsafe { std::ffi::CStr::from_ptr(result).to_string_lossy().into_owned() }
}
```

## Implementation Plan

### Phase 0: Lander Bridge (Almide → compiled Almide .dylib)

**Almide に C FFI 型を追加しない**。Rust crate を中間層にして既存の `@extern(rs, ...)` で接続する。

```
lander --lang almide mylib.almd
    ↓
    ├── libalmide_mylib.dylib           ← bridge_* byte-buffer 関数
    ├── bridge/                         ← Rust crate (lander が自動生成)
    │   ├── Cargo.toml                     [dependencies] に libloading 等
    │   └── src/lib.rs                     extern "C" + pack/unpack → safe wrapper
    └── almide-mylib/                   ← Almide package
        ├── almide.toml
        │   [rust-dependencies]
        │   bridge = { path = "../bridge" }
        └── src/mod.almd
            @extern(rs, "bridge", "distance")
            fn distance(a: Point, b: Point) -> Float
```

**コンパイラ側の変更 (1つだけ):**
- `almide.toml` の `[rust-dependencies]` を読み、生成する `Cargo.toml` に注入する
- `src/cli/mod.rs` の `GENERATED_CARGO_TOML` テンプレート生成を動的にする

**lander 側の変更:**
- `--lang almide` で Rust bridge crate + Almide package を出力する
- `almide-bindgen/src/bindings/almide.almd` を Rust bridge 生成に書き換え

**必要ないもの:**
- CString, Ptr, Opaque 等の FFI 型
- `@extern(c, ...)` codegen
- bytes pack/unpack の Almide stdlib
- dlopen ランタイム

### Phase 1–4: General C FFI (将来)

```
Phase 1: Primitive types only (Int, Float, Bool)
  - @extern(c, ...) codegen template
  - #[link] directive grouping
  - safe wrapper generation
  - テスト: libm の sqrt, floor, ceil を呼ぶ

Phase 2: String / CString
  - CString::new() / CStr::from_ptr マーシャリング
  - null-terminated string の安全なハンドリング
  - テスト: 自作 C ライブラリの文字列関数を呼ぶ

Phase 3: Pointer types
  - Ptr[T], Ptr[Opaque] 型の追加
  - opaque handle パターン (sqlite3*, FILE* 等)
  - テスト: SQLite の open/close/exec サイクル

Phase 4: Struct passing (--repr-c 連携)
  - #[repr(C)] struct の C ↔ Almide 受け渡し
  - by-value と by-pointer の使い分け
  - テスト: 座標構造体を C 関数に渡す
```

## Scope Boundary

**やること:**
- Almide から C 関数を呼ぶ（caller 側）
- 型マッピングと safe ラッパー自動生成
- static link (`#[link]`) によるリンク

**やらないこと（将来の別 roadmap）:**
- C から Almide を呼ぶ（callee 側 = lander の領域）
- dlopen / 動的リンク
- C ヘッダーの自動パース（Rust の bindgen 的なもの）
- C++ 対応

## Files to Modify

- `codegen/templates/rust.toml` — `extern_c_fn` テンプレート追加
- `crates/almide-codegen/src/walker/mod.rs` — target "c" の分岐 (L105-117)
- `crates/almide-codegen/src/walker/declarations.rs` — extern "C" ブロックのグルーピング
- `crates/almide-types/src/lib.rs` or new file — FFI 型定義 (CInt, CString, Ptr)
- `crates/almide-frontend/src/lower/mod.rs` — FFI 型の lowering
- `spec/integration/extern_c/` — テスト
