# Direct WASM Emission [PLANNED]

## Motivation

現在のWASMパス: `.almd → AST → Rust code → rustc → WASM` (433KB hello world)
目標: `.almd → AST → WASM bytecode` (1-5KB hello world, 100x改善)

MoonBitは直接WASM emitで数十バイト〜数KBを実現。Almideも同じアーキテクチャに移行し、Playgroundの高速化とEdge対応を可能にする。

## Architecture

```
src/
├── emit_rust/      ← 既存。native CLI / cargo統合用。維持する
├── emit_ts/        ← 既存。Web (Deno/Node) 用。維持する
└── emit_wasm/      ← 新規。WASM直接emit
    ├── mod.rs           エントリ: AST → Vec<u8> (WASM binary)
    ├── runtime.rs       線形メモリ管理 (bump allocator → 後でRC追加)
    ├── strings.rs       UTF-8 string on linear memory
    ├── values.rs        Int(i64), Float(f64), Bool(i32), Unit の変換
    ├── expressions.rs   式 → WASM instructions
    ├── statements.rs    let/var/assign/for → locals + instructions
    ├── functions.rs     fn定義 → WASM function, クロージャ → funcref + env
    ├── collections.rs   List (dynamic array), Map (hash table)
    └── patterns.rs      match → br_table / if-else chain
```

### 既存targetとの関係

- `--target rust` → `emit_rust/` (変更なし)
- `--target ts` / `--target js` → `emit_ts/` (変更なし)
- `--target wasm` → **`emit_wasm/`** (新規。現在の rustc 経由パスを置き換え)
- `almide run` → デフォルトは引き続き Rust target

### Crate依存

```toml
[dependencies]
wasm-encoder = "0.245"  # bytecodealliance の低レベルWASMエンコーダ
```

`wasm-encoder` は WASM module を Rust の構造体として組み立て、`Vec<u8>` にシリアライズする。WAT テキストは経由しない。

## WASM の基礎知識（次のセッション向け）

### WASM のメモリモデル
- **線形メモリ**: 1つのフラットな `Vec<u8>` に全データを格納。`memory.grow` で拡張
- **ローカル変数**: 関数ごとに `i32`/`i64`/`f32`/`f64` のローカルを持てる
- **スタック**: 各命令が値スタック上で動作。`i64.add` は2つpopして1つpush
- **関数テーブル**: 間接呼び出し用。クロージャの実装に使う
- **型**: `i32`, `i64`, `f32`, `f64`, `funcref`, `externref` のみ。構造体やポインタは `i32` (メモリオフセット) で表現

### Almide 型 → WASM 型マッピング

| Almide型 | WASM型 | メモリレイアウト |
|----------|--------|-----------------|
| Int | `i64` | ローカル変数 or スタック |
| Float | `f64` | ローカル変数 or スタック |
| Bool | `i32` (0/1) | ローカル変数 or スタック |
| Unit | なし（省略） | — |
| String | `i32` (ポインタ) | メモリ上: [len: i32][data: u8...] |
| List[T] | `i32` (ポインタ) | メモリ上: [len: i32][cap: i32][data: T...] |
| Tuple(A,B) | `i32` (ポインタ) | メモリ上: [A][B] 隣接配置 |
| Record | `i32` (ポインタ) | メモリ上: フィールド順に隣接配置 |
| Variant | `i32` (ポインタ) | メモリ上: [tag: i32][payload...] |
| Map | `i32` (ポインタ) | メモリ上: ハッシュテーブル構造 |
| Option[T] | `i32` (ポインタ or 0) | None = 0, Some = ポインタ |
| fn(A)->B | `i32` (ポインタ) | メモリ上: [funcidx: i32][env_ptr: i32] |

### 値型 vs ヒープ型
- **値型** (Int, Float, Bool): WASMローカル変数に直接格納。コピーコスト0
- **ヒープ型** (String, List, Record, Variant): 線形メモリに確保。i32ポインタで参照
- タプルは要素が全て値型なら多値返却 (`(i64, i64)`) にできる最適化余地あり

## Implementation Phases

### Phase 0: PoC — Hello World (1日)

目標: `wasm-encoder` で手書きの WASM を生成し、WASI (wasmtime) で実行。サイズ計測。

```rust
// src/emit_wasm/mod.rs (最初のPoC)
use wasm_encoder::*;

pub fn emit_hello_world() -> Vec<u8> {
    let mut module = Module::new();

    // Import fd_write from WASI
    let mut imports = ImportSection::new();
    imports.import("wasi_snapshot_preview1", "fd_write",
        EntityType::Function(/* type index */));

    // Memory: 1 page (64KB)
    let mut memories = MemorySection::new();
    memories.memory(MemoryType { minimum: 1, maximum: None, ... });

    // Data: "Hello\n" at offset 0
    let mut data = DataSection::new();
    data.active(0, &ConstExpr::i32_const(0), b"Hello\n");

    // Function: _start → call fd_write(stdout, iov, 1, &nwritten)
    // ... (WASMの命令列を組み立て)

    module.finish()
}
```

確認事項:
- `wasmtime` で実行できるか
- バイナリサイズ (目標: 100B以下)
- `wasm-encoder` の API の使い勝手

### Phase 1: 最小言語サブセット (1-2週)

対応する言語機能:
- [x] Int/Float リテラル、四則演算、比較
- [x] Bool (`true`/`false`), `and`/`or`/`not`
- [x] let/var バインディング
- [x] if/then/else
- [x] fn 定義、関数呼び出し
- [x] println (WASI fd_write 経由)
- [x] String リテラル (data section に埋め込み)

対応しない:
- クロージャ (lambda)
- List, Map, Record
- パターンマッチ
- stdlib のほとんど

ゴール: FizzBuzz が動く

```
fn fizzbuzz(n: Int) -> String =
  if n % 15 == 0 then "FizzBuzz"
  else if n % 3 == 0 then "Fizz"
  else if n % 5 == 0 then "Buzz"
  else int.to_string(n)
```

### メモリ管理 (Phase 1 内で実装)

最初は **bump allocator** で十分。解放しない。

```
┌──────────────────────────────────────┐
│ Linear Memory (64KB page)            │
├──────────┬───────────┬───────────────┤
│ Static   │ Stack     │ Heap →→→      │
│ data     │ (iov等)   │ (bump alloc)  │
│ 0..1024  │ 1024..4096│ 4096..        │
└──────────┴───────────┴───────────────┘
```

```rust
// Runtime: bump allocator
// global $heap_ptr: i32 = 4096
// fn alloc(size: i32) -> i32:
//   local ptr = global.get $heap_ptr
//   global.set $heap_ptr (i32.add ptr size)
//   ptr
```

### Phase 2: コレクション + クロージャ (2-3週)

**List:**
```
メモリレイアウト: [ref_count: i32][len: i32][cap: i32][elem_0][elem_1]...
```
- 要素サイズは型に依存 (Int=8bytes, String=4bytes pointer)
- `list.len` → メモリ読み出し
- `list.get` → bounds check + メモリ読み出し
- `list.set` → 新リスト確保 + コピー + 書き込み (immutable)
- `list.swap` → 新リスト確保 + コピー + swap

**クロージャ:**
```
メモリレイアウト: [func_index: i32][env_ptr: i32]
環境: [captured_var_0][captured_var_1]...
```
- `fn(x) => x + y` で `y` がキャプチャされる
- 環境はヒープに確保、ポインタを持つ
- 呼び出し: `call_indirect` でfunc_indexを呼び、env_ptrを第1引数に渡す
- **重要**: `list.map(xs, fn(x) => x + 1)` が動かないとAlmideとして使い物にならない

**参照カウント:**
```
// 各ヒープオブジェクトの先頭4バイトが ref_count
// fn rc_incr(ptr: i32): memory[ptr] += 1
// fn rc_decr(ptr: i32): memory[ptr] -= 1; if 0 then free(ptr)
```
- bump allocatorからfree listベースに移行
- 循環参照はAlmideでは発生しにくい（immutableデータ + ループはforのみ）

### Phase 3: 完全対応 (3-4週)

- Record: フィールドオフセット計算、名前解決はコンパイル時
- Variant: tag + payload、match → tag読み出し + br_table分岐
- Map: 簡易ハッシュテーブル (open addressing)
- Tuple: 小さいタプル (2-3要素) は多値返却最適化
- String interpolation: 文字列結合のWASM実装
- for...in: List走査のインライン化
- effect fn: Result型の表現 (tag: 0=ok, 1=err + payload)
- do block: エラー伝搬のWASM実装

### Phase 4: 最適化 (継続)

- dead code elimination (使われていない関数をemitしない)
- 定数畳み込み (コンパイル時に計算可能な式を事前評価)
- インライン化 (小さい関数をcall siteに展開)
- List操作の融合 (map+filter → 1パス)
- String の small string optimization (短い文字列はポインタに直接埋め込み)

## 予想サイズ比較

| プログラム | Rust経由（現在） | 直接emit（推定） | MoonBit（参考） |
|-----------|-----------------|-----------------|----------------|
| Hello World | 433KB | 100-500B | ~30B |
| FizzBuzz | 433KB | 1-3KB | ~500B |
| Fibonacci | 433KB | 1-3KB | ~1KB |
| Quicksort | ~500KB | 10-30KB | ~5KB |
| JSON parser | ~600KB | 30-80KB | ~20KB |
| 実用アプリ | ~1MB | 50-200KB | 30-100KB |

MoonBitの数十バイトには届かないが、**100-1000x の改善**。実用上十分。

## Playgroundへの影響

直接emit最大のメリットは**コンパイル速度**:

| | 現在 (Rust経由) | 直接emit |
|---|---|---|
| WASM生成 | rustc 呼び出し: 数秒〜数十秒 | インメモリ: 数ms |
| Playground | WASM crate で JS emit | **WASM直接emit → ブラウザで即実行** |

Playground を `compile_to_js` → `eval` から `compile_to_wasm` → `WebAssembly.instantiate` に変えられる。これにより:
- JS runtime の __almd_list 等が不要になる
- ランタイムがWASMバイナリ内に含まれる
- `patchRuntimeForBrowser` のハック不要

## リスク・判断ポイント

### やる価値がある場合
- Playground の高速化が最優先の場合
- Edge/WASM ターゲットを本気で狙う場合
- 「Almide はコンパイルが速い」を売りにしたい場合

### やる価値が薄い場合
- LLM-first の使命に直接貢献しない（LLMはバイナリサイズを気にしない）
- Rust/TS target で十分な場合
- 工数が他の機能（LSP、trait system、stdlib充実）に使える場合

### 妥協案
- Phase 1 (FizzBuzzレベル) だけ実装してサイズと速度を計測
- 計測結果を見て Phase 2 以降を判断
- Playground は引き続き JS emit で運用し、将来的に切り替え

## 参考

- [wasm-encoder crate](https://docs.rs/wasm-encoder/) — bytecodealliance のWASMバイナリ生成ライブラリ
- [WASM spec](https://webassembly.github.io/spec/) — 公式仕様
- [WASI preview1](https://github.com/WebAssembly/WASI/blob/main/legacy/preview1/docs.md) — fd_write 等のシステムコール
- [MoonBit](https://www.moonbitlang.com/) — 競合。直接WASM emit の参考
- [Virgil](https://github.com/nickmain/virgil) — 直接WASM emit する研究言語
- [AssemblyScript](https://www.assemblyscript.org/) — TypeScript→WASM。参考になるメモリ管理

## 実装メモ（次のセッション用）

### wasm-encoder の基本パターン

```rust
use wasm_encoder::{
    Module, TypeSection, FunctionSection, CodeSection,
    ImportSection, MemorySection, DataSection, ExportSection,
    Function, Instruction, ValType, MemArg,
};

let mut module = Module::new();

// 1. Type section: 関数シグネチャを定義
let mut types = TypeSection::new();
types.function(vec![ValType::I64], vec![ValType::I64]); // fn(i64) -> i64

// 2. Function section: 関数とtype indexの対応
let mut functions = FunctionSection::new();
functions.function(0); // type index 0

// 3. Code section: 関数本体
let mut codes = CodeSection::new();
let mut f = Function::new(vec![]); // ローカル変数なし
f.instruction(&Instruction::LocalGet(0));
f.instruction(&Instruction::I64Const(1));
f.instruction(&Instruction::I64Add);
f.instruction(&Instruction::End);
codes.function(&f);

// 4. Export section
let mut exports = ExportSection::new();
exports.export("add_one", ExportKind::Function, 0);

// 5. 組み立て
module.section(&types);
module.section(&functions);
module.section(&codes);
module.section(&exports);

let bytes: Vec<u8> = module.finish();
```

### Almide AST → WASM 変換の起点

```rust
// src/emit_wasm/mod.rs
pub fn emit(program: &ast::Program) -> Vec<u8> {
    let mut emitter = WasmEmitter::new();
    for decl in &program.declarations {
        match decl {
            ast::Decl::Fn { name, params, body, .. } => {
                emitter.emit_function(name, params, body);
            }
            ast::Decl::Type { .. } => {
                emitter.register_type(decl);
            }
            _ => {}
        }
    }
    emitter.finish()
}
```

### WASI の println 実装

```
;; println(s: i32_ptr) の WASM 実装
;; s は [len: i32, data: u8...] へのポインタ
;; 1. iov 構造体を組み立て (buf_ptr, buf_len)
;; 2. fd_write(fd=1, iovs, iovs_len=1, &nwritten) を呼ぶ
;; 3. '\n' も書き出す
```

これは Phase 0 の PoC で最初に実装するもの。
