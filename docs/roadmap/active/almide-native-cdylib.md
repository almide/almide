<!-- description: Build .so/.dylib from pure Almide, eliminating Rust scaffolding from lander -->
# Almide Native cdylib — Scaffold in Almide, Not Rust

## Goal

lander の scaffolding (lib.rs) を Almide で書けるようにする。Rust コード生成を排除し、almide-bindgen/lander を **100% Almide** で閉じる。

```
現状:
  mylib.almd → bindgen → scaffolding (Rust) → cargo build --lib → .dylib

目標:
  mylib.almd → bindgen → scaffolding (Almide) → almide build --cdylib → .dylib
```

## What's Needed

### 1. `@export(c, "symbol")` — callee 側 FFI 属性

`@extern(c)` の逆。Almide 関数を C ABI で公開する。

```almide
@export(c, "bridge_add")
fn bridge_add(args: RawPtr, args_len: Int, out: RawPtr, out_cap: Int) -> Int = {
  var pos = 0
  let a = bytes.read_i64_be(args_as_bytes, pos)
  pos = pos + 8
  let b = bytes.read_i64_be(args_as_bytes, pos)
  let result = add(a, b)
  var out_buf = bytes.new(0)
  bytes.write_i64_be(out_buf, result)
  bytes.copy_to_ptr(out_buf, out, out_cap)
}
```

生成される Rust:
```rust
#[no_mangle]
pub extern "C" fn bridge_add(args: *mut u8, args_len: i32, out: *mut u8, out_cap: i32) -> i32 {
    // ... Almide body compiled to Rust ...
}
```

実装: `@extern(c)` とほぼ対称。パーサーは `@export` を新しいキーワードとして認識、codegen が `#[no_mangle] pub extern "C"` を付与。

### 2. `almide build --cdylib` — shared library 出力

現在の `almide build` は実行バイナリ (bin) を出力する。`--cdylib` で動的ライブラリを出力する。

変更箇所: `src/cli/mod.rs` の `GENERATED_CARGO_TOML` テンプレート:
```toml
# --cdylib 時
[lib]
name = "almide_mylib"
crate-type = ["cdylib"]
```

加えて `src/main.rs` → `src/lib.rs` への出力先変更（main 関数なし）。

### 3. `bytes` のポインタ操作拡充

bridge の callee 側は RawPtr からの読み込みが必要:

```
bytes.from_raw_ptr(ptr: RawPtr, len: Int) -> Bytes   ← unsafe slice 作成
bytes.copy_to_ptr(buf: Bytes, ptr: RawPtr, cap: Int) -> Int  ← 結果書き戻し
```

2 関数追加。`@export(c)` 関数内でのみ使うことを想定。

### 4. scaffolding.almd を Almide に書き換え

現在の `almide-bindgen/src/scaffolding.almd` は Rust ソースコードを文字列生成している。これを:
- bridge 関数の Almide ソースコードを生成するように変更
- `@export(c, "bridge_*")` + bytes pack/unpack の Almide コード

`bridge_alloc` / `bridge_free` も Almide で書ける:
```almide
@export(c, "bridge_alloc")
fn bridge_alloc(len: Int) -> RawPtr = bytes.as_mut_ptr(bytes.new(len))

@export(c, "bridge_free")
fn bridge_free(ptr: RawPtr, len: Int) -> Unit = bytes.free_raw(ptr, len)
```

## Implementation Plan

```
Step 1: @export(c) 属性
  - パーサー: @export(c, "symbol") の認識
  - AST: ExportAttr { target, symbol }
  - codegen: #[no_mangle] pub extern "C" fn + 型マッピング (Int→i32 等)
  - テスト: Almide 関数を C ABI で公開、別プロセスから dlopen で呼ぶ

Step 2: almide build --cdylib
  - CLI: --cdylib フラグ追加
  - Cargo.toml テンプレート: crate-type = ["cdylib"]
  - 出力: src/lib.rs (main なし)
  - テスト: .dylib が生成されること、nm でシンボル確認

Step 3: bytes ポインタ操作
  - bytes.from_raw_ptr, bytes.copy_to_ptr 追加
  - Rust ランタイム実装 (unsafe)
  - テスト: RawPtr → Bytes 変換の往復

Step 4: scaffolding.almd 書き換え
  - Rust コード生成 → Almide コード生成に変更
  - lander のワークフロー更新: cargo build → almide build --cdylib
  - E2E: mylib.almd → almide-only scaffolding → .dylib → consumer
```

## Architecture After

```
almide-bindgen (100% Almide)
├── src/scaffolding.almd     ← generates Almide @export(c) code (not Rust)
└── src/bindings/*.almd      ← generates caller code for each language

almide-lander (100% Almide)
├── src/main.almd
└── workflow:
    1. almide compile mylib.almd --json → interface
    2. scaffolding.generate(interface) → scaffold.almd
    3. almide build scaffold.almd --cdylib → .dylib
    4. bindings/<lang>.generate(interface) → binding.<ext>
```

Rust は Almide コンパイラの内部実装としてのみ存在し、ユーザー/ツーリングからは見えなくなる。

## Dependencies

- `@extern(c)` — ✅ done (this session)
- `RawPtr` 型 — ✅ done (this session)
- `bytes` pack/unpack — ✅ done (this session)
