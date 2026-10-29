<!-- description: Stdlib redesign L2/L3 — ffi (new, #3853): the five raw-address functions leave the always-visible bytes module for an explicitly imported ffi with one address op that is null on empty on both targets -->
# L2/L3: ffi（新しいモジュール）

> Last updated: 2026-10-21
> 前提: [L0](L0-conventions.md)・[L1](L1-categories.md)（R-L1-26: 生ポインタを `ffi` に移す、#3853）
> 出典: Rust https://doc.rust-lang.org/std/primitive.slice.html#method.as_ptr ・ https://doc.rust-lang.org/std/slice/fn.from_raw_parts.html ・ Go https://pkg.go.dev/unsafe ・ Python https://docs.python.org/3/library/ctypes.html ・ TS/JS https://nodejs.org/api/n-api.html （JS 側は `WebAssembly.Memory.buffer`）・ Gleam —（無い。外部関数は `@external`）・ Elixir —（NIF は C 側）・ OCaml https://ocaml.org/manual/latest/intfc.html ・ Kotlin https://kotlinlang.org/api/core/kotlin-stdlib/kotlinx.cinterop/ （Native）・ Swift https://developer.apple.com/documentation/swift/unsafemutablerawpointer

> **2026-10-27 追記**: この文書が述べる `RawPtr` の面（`ffi.address`・`ffi.to_int`・番地と長さを渡す `ffi.read` / `ffi.write`、`bytes.as_ptr` などの旧綴り）は [ADR-0031](../../adr/0031-raw-pointers-carry-provenance.md)（Accepted 2026-10-27、dialect epoch 15）で置き換わりました。いまの面: `RawPtr` は不透明な外部トークン `Handle` に改名（stdlib の関数は受け取りも返しもしない）。読み書きは長さつきの `Region` を通す — `ffi.region(b)`（読み取り専用）・`ffi.alloc(n)`（effect fn、書き込み可、ゼロ埋め）・`ffi.len`・`ffi.is_writable`・`ffi.read(r, off, n)`・`ffi.write(r, off, b)`・`ffi.to_bytes(r)`。範囲外は `err`。`ffi.address`・`ffi.to_int`・`bytes.as_ptr`・`as_mut_ptr`・`data_ptr`・`from_raw_ptr`・`copy_to_ptr` は削除。`@extern(c)` の `Region` 引数は (ptr, size_t len) の 2 引数で渡り、`Region` を返すには `@region(len = …, release = "…")` が要ります（無ければ E090）。以下は 2026-10-21 時点の記録として残します。

## L2: 役割の比較

| 役割 | Almide（いま → 案） | Rust | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin | Swift |
|---|---|---|---|---|---|---|---|---|---|---|
| バイト列の先頭の番地 | `bytes.as_ptr`・`as_mut_ptr`（空なら null）・`data_ptr`（`Int`、空でも null でない）→ `ffi.address` | `as_ptr`・`as_mut_ptr` | `unsafe.SliceData` | `ctypes.addressof` | `Memory.buffer` の offset | — | — | `Bigarray` | `addressOf`・`refTo` | `withUnsafeBytes` |
| 番地を整数で | `bytes.data_ptr` → `ffi.to_int` | `as usize` | `uintptr` | `ctypes.cast(…, c_void_p).value` | — | — | — | — | `toLong` | `Int(bitPattern:)` |
| 番地から読む（コピー） | `bytes.from_raw_ptr` → `ffi.read` | `slice::from_raw_parts` | `unsafe.Slice` | `ctypes.string_at` | `new Uint8Array(buf, off, len)` | — | — | — | `readBytes` | `UnsafeRawBufferPointer` |
| 番地へ書く | `bytes.copy_to_ptr` → `ffi.write` | `ptr::copy_nonoverlapping` | `copy(unsafe.Slice…)` | `ctypes.memmove` | `Uint8Array.set` | — | — | — | `memcpy` | `copyMemory` |

### 完全性の規則

- 役割は 4 つに閉じます: {番地を取る, 番地を整数にする, 番地から読む, 番地へ書く}。
- 番地を取る関数は 1 つだけで、空のバイト列には null を返します（native と wasm で同じ答え）。読み取り用と書き込み用の 2 つの綴りは置きません（native の `as_ptr` も `*mut u8` を返しており、区別は名前だけでした。#3853）。
- `import ffi` が要ります。`bytes` は全ファイルから import なしで見えるので、メモリ安全でない関数を置きません。
- 番地を取る・整数にする関数は純粋です（値は変わるが、外の世界を読まない）。ADR-0027 D3 の「往復」の verdict で検品します。

### L2 の判定

- 重複: 番地を返す綴りが 3 つ（`as_ptr`・`as_mut_ptr`・`data_ptr`）で、空のときの答えが違う → R-ffi-01
- 置き場所違い: 5 関数が自動で見える `bytes` にある → R-ffi-01

## 案

### R-ffi-01 `ffi` を作り、`bytes` の生ポインタ 5 関数を移す — ○ 採用

- 内容:
  ```almide
  fn address(b: Bytes) -> RawPtr                     // 先頭の番地。空なら null
  fn to_int(p: RawPtr) -> Int                        // 番地の整数（JS host に渡す用。null は 0）
  fn read(ptr: RawPtr, len: Int) -> Bytes            // len バイトのコピー。null か len <= 0 なら空
  fn write(b: Bytes, ptr: RawPtr, cap: Int) -> Int   // 最大 cap バイトを書き、書いた数。null なら 0
  ```
  書き換え: `bytes.as_ptr(b)`・`bytes.as_mut_ptr(b)` → `ffi.address(b)`、`bytes.data_ptr(b)` → `ffi.to_int(ffi.address(b))`、`bytes.from_raw_ptr(p, n)` → `ffi.read(p, n)`、`bytes.copy_to_ptr(b, p, c)` → `ffi.write(b, p, c)`。
- 意味の違い: `data_ptr` は空のバイト列でも `handle + 12` を返していました。`ffi.to_int(ffi.address(b))` は 0 です。長さ 0 の読み取りしか起きないので、JS host の経路（#907、#3420 の寿命の規則 `spec/wasm_host_js/extern_data_ptr_lifetime.almd`）では差が出ません。新しい名前の側だけで起きる変化です（R-L0-14 の 4）。
- 綴り: `to_int` は R-L0-07（元の型のモジュールに `to_<先>`）。`RawPtr` の持ち主は `ffi` です（R-L0-02）。
- 呼び出せるファイルを絞る権限（`prim` の E085 と同じ形で、`@extern` の無いファイルからの `ffi.*` を拒む案）は、この案に含めません。`[permissions]` の鍵の設計は `proc`（ADR-0025）と一緒に決めるもので、#3853 で追跡します。この案は、権限がなくても成り立ちます（`import ffi` が要るだけで、ふつうのコードの補完候補からは消えます）。
- 根拠の L0 規則: R-L0-01（番地の綴りを 1 つ）、R-L0-02、R-L0-07、R-L1-26。
- (a) 9 言語とも、生の番地は `unsafe`・`ctypes`・`cinterop` などの名前のはっきりした場所にあります。ふつうのバイト列の型の隣に置く言語は Rust（`as_ptr` は安全、使う側が `unsafe`）だけです。
- (b) native: `*mut u8`。wasm: 線形メモリの番地（#440 / C-062、`stdlib/bytes_rawptr.almd`）。両方で同じ本体を使い、空のときの答えをそろえます。host の機能は要りません。
- (c) LLM がふつうのコードで `bytes.as_mut_ptr` を補完候補に見ることがなくなります。
- (d) 追加と、`bytes` の側の撤去（L2-bytes で同じ判定）。`@deprecated(since=15, use="ffi.…")`、`removal_epoch = 16`。`almide fix` は 1 対 1 で書き換えます（`import ffi` も足す）。使用数（spec/tests/docs、#3853）: `as_ptr` 10/1/7、`as_mut_ptr` 9/0/5、`data_ptr` 12/0/10、`from_raw_ptr` 6/0/4、`copy_to_ptr` 11/0/5。
- ゲート案: `bytes` に `RawPtr` を引数か戻り値に持つ公開関数が無いこと。`ffi.address` の空のときの答えを native と wasm で比べる cross fixture。
- issue: #3853（既存）

### R-ffi-02 `ffi.read` と `ffi.write` を effect fn にする — ○ 採用

- いま: `fn ffi.read(ptr: RawPtr, len: Int) -> Bytes` と `fn ffi.write(b: Bytes, ptr: RawPtr, cap: Int) -> Int` は純粋な fn です。
- 案: `effect fn ffi.read(…)`、`effect fn ffi.write(…)`。`address`・`to_int` は純粋のままです。
- 根拠の L0 規則: R-L0-13（環境を読む関数は effect）、ADR-0022（純粋な fn ができるのは書き出しと abort だけ）。`write` は Almide の値の外のメモリを書き換え、`read` は外のコードが書いたメモリを読みます。
- (a) 9 言語とも、生の番地の読み書きは `unsafe`・`ctypes` など、ふつうの関数と区別された場所にあります。
- (b) 実測で両 target が食い違います。純粋な fn の中で `let` の `Bytes` の番地へ書くと、native では呼び出し元の不変の値が書き換わり、wasm では書き換わりません（いまの `bytes.as_mut_ptr` と `copy_to_ptr` で再現。#3966、リリースを止める不具合として別に直します）。純粋な fn として扱うかぎり、最適化でさらに変わりえます。
- (d) 新しい関数の宣言だけです。移行元の `bytes.from_raw_ptr`・`copy_to_ptr` を使う純粋な fn は、書き換えたあと E006 になるので、ヒント「呼び出し元を effect fn にする」を出します。
- 完全性の規則: Almide の値の外にある状態を読み書きする関数は effect です。`ffi` では {address, to_int} が純粋、{read, write} が effect です。
- issue: #3974

### R-ffi-03 `ffi.write` の引数を「書き込む先が先」にする — ○ 採用

- いま（案の段階）: `fn ffi.write(b: Bytes, ptr: RawPtr, cap: Int) -> Int`。標準のほかの書き込みは、書き込む先が先です（`fs.write_bytes(path, data)`、`net.tcp_write(stream, data)`、`unix.write(sock, data, fds)`、`bytes.put_<型>(mut b, pos, v)`）。
- 案: `fn ffi.write(ptr: RawPtr, b: Bytes, cap: Int) -> Int`。`ffi.read(ptr, len)` とも、先頭がポインタで揃います。
- 根拠の L0 規則: R-L0-03（主語の引数が先）。
- (d) まだ出ていない関数の形を変えるだけです。`almide fix` の `bytes.copy_to_ptr(b, p, n)` からの書き換えは、引数の順を入れ替えます。
- issue: #3979

## L3: 関数表

| 関数 | シグネチャ(案) | 他言語の主な綴り | 判定 | 案 |
|---|---|---|---|---|
| `ffi.address` | `fn address(b: Bytes) -> RawPtr` | Rust `as_ptr` / Go `unsafe.SliceData` / Python `addressof` | 追加（`bytes.as_ptr`・`as_mut_ptr` から） | R-ffi-01 |
| `ffi.to_int` | `fn to_int(p: RawPtr) -> Int` | Go `uintptr` / Swift `Int(bitPattern:)` | 追加（`bytes.data_ptr` の役割） | R-ffi-01 |
| `ffi.read` | `effect fn read(ptr: RawPtr, len: Int) -> Bytes` | Rust `from_raw_parts` / Python `string_at` / Go `unsafe.Slice` | 追加（`bytes.from_raw_ptr` から） | R-ffi-01 |
| `ffi.write` | `effect fn write(ptr: RawPtr, b: Bytes, cap: Int) -> Int` | Rust `copy_nonoverlapping` / Python `memmove` | 追加（`bytes.copy_to_ptr` から。書き込む先を先に） | R-ffi-01、R-ffi-03 |
