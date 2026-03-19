# Direct WASM Emission via wasm-gc [ACTIVE]

Branch: `feature/wasm-direct`

## Core Insight

**ランタイムを自分で持たない。ホストに委譲する。**

Rust 経由 WASM はアロケーター(16KB) + fmt(40KB) + panic handler(10KB) が不可避。MoonBit が 30 bytes で Hello World を出せるのは、メモリ管理をブラウザ/wasmtime の GC に任せているから。

Almide も同じ戦略を取る。バイナリに入れるのはロジックだけ。

## Ideal Architecture (最終理想形)

```
.almd source
     │
     ▼
  Lexer → Parser → AST → Checker → Lowering → IR
                                                 │
                                    ┌────────────┼────────────┐
                                    ▼            ▼            ▼
                              Target::Rust  Target::TS   Target::WasmGc
                                    │            │            │
                              nanopass (7)  nanopass (4)  nanopass (3)
                                    │            │            │
                              TOML walker  TOML walker  wasm emitter
                                    │            │            │
                                  .rs          .ts/.js     .wasm
                                    │                        │
                                  rustc                   done.
                                    │                     (no second compiler)
                                  binary
```

**WasmGc は既存の Target の仕組みにそのまま乗る。** 新しいアーキテクチャは不要。`target.rs` に 1 エントリ追加し、出力層だけテンプレート walker → バイナリ emitter に差し替える。

### なぜ codegen v3 にそのまま乗るか

nanopass 層は共有。ターゲット固有のパスだけが違う：

```rust
Target::Rust => Pipeline::new()
    .add(TypeConcretizationPass)
    .add(BorrowInsertionPass)       // Rust only
    .add(CloneInsertionPass)        // Rust only
    .add(StdlibLoweringPass)
    .add(ResultPropagationPass)     // Rust only (?)
    .add(BuiltinLoweringPass)       // Rust only (macros)
    .add(FanLoweringPass),

Target::WasmGc => Pipeline::new()
    .add(TypeConcretizationPass)
    .add(StdlibLoweringPass)
    .add(FanLoweringPass),
    // 5 パスが不要。GC がコピー/所有権/Box を全部処理する。
```

**コンパイラの複雑度が半分になる。** Borrow analysis、clone insertion、Box deref — 全部 GC に委譲。

### 最終的な出力パス

```
codegen::emit(ir, Target::WasmGc) → Vec<u8>
  1. nanopass pipeline (3 passes)
  2. type layout: Almide types → wasm-gc types (struct/array/ref)
  3. function emit: IrFunction → WASM function (wasm-encoder)
  4. module assembly: types + imports + functions + exports → .wasm
```

テンプレートは使わない。IR ノードを直接 `wasm_encoder::Instruction` に変換する。

## Type Mapping (理想形)

| Almide | wasm-gc | Allocation |
|--------|---------|------------|
| Int | `i64` | スタック (0 cost) |
| Float | `f64` | スタック (0 cost) |
| Bool | `i32` | スタック (0 cost) |
| Unit | (omitted) | なし |
| String | `(ref $str)` = `(array (mut i8))` | **GC** |
| List[T] | `(ref $list)` = `(array (mut $elem))` | **GC** |
| Record | `(ref $Rec)` = `(struct field...)` | **GC** |
| Variant | `(ref $Var)` = `(struct (field $tag i32) ...)` | **GC** |
| Option[T] | `(ref null $T)` | **GC** (null = none) |
| Result[T,E] | `(struct (field $tag i32) (field $ok $T) (field $err $E))` | **GC** |
| fn(A)->B | `(struct (field $fn funcref) (field $env (ref $env_struct)))` | **GC** |
| Map[K,V] | ホスト import or ツリーマップ | **GC** |

**全ヒープオブジェクトは GC が管理。** ref count なし、free なし、デストラクタなし。

## Phase 0: PoC — DONE

| Mode | Size | Validated |
|------|------|-----------|
| WASI | 143 bytes | wasmtime run ✓ |
| Embed | 111 bytes | wasm-tools validate ✓ |
| **wasm-gc** | **77 bytes** | **wasm-tools validate ✓** |

## Phase 1: IR → wasm-gc (next)

Goal: FizzBuzz → wasm-gc binary, <500 bytes.

```
IR nodes:  LitInt, LitFloat, LitBool, BinOp, UnaryOp,
           If, Block, Bind(let), Var, Call, Function
```

これが動けば「Almide コンパイラが wasm-gc バイナリを直接出力する」が証明される。

## Phase 2: Strings + Records + Collections

Goal: string 操作、レコード生成、リスト操作が動く。

- String → GC array i8 + ホスト import (print, concat, slice)
- Record → GC struct (フィールドレイアウトはコンパイル時確定)
- List → GC array + ホスト import (map, filter は WASM 内で実装可能)
- for...in → loop + array.get

## Phase 3: Variant + match + Option/Result

Goal: パターンマッチ、エラー処理が動く。

- Variant → tagged struct (tag field + payload fields)
- match → tag 読み出し + br_table 分岐
- Option → nullable ref (ref.is_null で判定)
- Result → tagged struct (tag=0: ok, tag=1: err)
- effect fn → Result を返す関数 (auto-unwrap は呼び出し側で tag チェック)

## Phase 4: Closures + stdlib + 全テスト通過

Goal: 142/142 テストが wasm-gc ターゲットで通る。

- Lambda → closure struct (funcref + env)
- call_indirect で closure 呼び出し
- stdlib core (string, list, map, int, float, math) の WASM 実装
- ホスト import で補えない部分のみ WASM 内に実装

## Phase 5: Playground 統合

Goal: Playground が JS emit → eval ではなく wasm-gc emit → WebAssembly.instantiate に。

- `compile_to_wasm()` が `Vec<u8>` を返す
- JS 側: `WebAssembly.instantiate(bytes, { env: { print, ... } })`
- `__almd_list` 等の JS ランタイムが不要になる
- `patchRuntimeForBrowser` ハック廃止

## Size Targets (MoonBit 同等以上)

| Program | Target | MoonBit (ref.) | 現行 Rust 経由 |
|---------|--------|----------------|---------------|
| return 42 | ~30 bytes | ~30 bytes | 433 KB |
| FizzBuzz | <500 bytes | ~500 bytes | 433 KB |
| Quicksort | <5 KB | ~5 KB | ~500 KB |
| Real app | <50 KB | 30-100 KB | ~1 MB |

## Why This Matters

1. **LLM が生成したコードが即 WASM になる** — rustc 不要、数 ms でコンパイル完了
2. **Playground が本物の native 速度で動く** — JS eval のオーバーヘッドなし
3. **Edge deploy** — CloudFlare Workers 等で Almide バイナリが直接動く
4. **コンパイラが単純になる** — borrow/clone/Box の複雑さが GC に吸収される
5. **バイナリサイズで MoonBit と同等** — Almide の売りが「LLM 精度 + 小さいバイナリ」になる

## References

- [wasm-gc proposal](https://github.com/nickmain/nickmain.github.io/wiki/WebAssembly-GC-Proposal)
- [V8 wasm-gc blog](https://v8.dev/blog/wasm-gc)
- [MoonBit](https://www.moonbitlang.com/) — wasm-gc 直接 emit の先行者
- [wasm-encoder](https://docs.rs/wasm-encoder/) — Almide が使う WASM バイナリ生成ライブラリ
- [Chez Scheme nanopass](https://nanopass.org/) — Almide の codegen 設計の源流
