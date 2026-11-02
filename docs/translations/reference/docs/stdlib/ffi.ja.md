# ffi

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](ffi.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](ffi.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](ffi.zh-CN.md)



> 原文：[docs/stdlib/ffi.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/ffi.md)（固定コミット `35297a612fb29e3f78f6587aba253aa91d51f466`）。非規範的な読者向けスナップショット訳。自動同期されず、独立した実測・実装検証は行っていない。API シグネチャとコード内コメントは原文を保持し、説明訳をコード外へ添える。生成索引の固定コピーであり、この訳は生成対象ではない。 境界チェックはバインディングが宣言する長さと外部コードを信頼する前提であり、外部コード全体の安全性保証ではない。原文冒頭は Region の生成箇所を「3 つ」とするが、後段には export へ貸し出す領域も記載される。この構成を勝手に統一せず保持する。

外部関数との境界にあるメモリ。`import ffi`。

プログラムが読み取れるポインタには出自が付く（[ADR-0031](../../../../adr/0031-raw-pointers-carry-provenance.md)）。

- **`Handle`** は不透明な外部トークン（`FILE*`、`sqlite3*`）。`@extern` 関数から返され、外部関数へ渡し返すことだけができる。これを通して読み書きするものはなく、`Int` 形式もない。
- **`Region`** はベース、長さ、権限、所有者を持つビューで、フィールドは不透明。`ffi.read` と `ffi.write` は、両ターゲットでメモリに触れる前に `0 <= off` と `off + n <= ffi.len(r)` を調べる。領域外のアクセスは `err` になり、別のメモリを読み書きしない。読取り専用領域への書込みも `err`。

Region の生成箇所は厳密に 3 つで、どれも長さを把握している。

| 生成元 | 長さ | 書込み可否 |
|---|---|---|
| `ffi.region(b)` | `bytes.len(b)`。領域が `b` を保持 | 不可 |
| `ffi.alloc(n)!` | `n`。新規・ゼロ初期化で、Almide のどの値ともエイリアスしない | 可 |
| `@region(len = …, release = "…")` を持つ `@extern(c)` 関数 | 宣言した長さ | 不可 |

```almide
import ffi

@extern(c, "z", "zbuf_new")
@region(len = n, release = "zbuf_free")
fn zbuf_new(n: Int) -> Region

@extern(c, "z", "zfill")
fn zfill(buf: Region) -> Unit
```

`@extern(c)` 関数の `Region` 引数は C の 2 引数、ポインタと `size_t` の長さ（`void zfill(uint8_t *buf, size_t len)`）になる。`@extern(rust)` 関数では同じ組 `(base: *mut u8, len: usize)`、`@extern(wasm)` 関数ではベースと長さの 2 数値。呼出し先は常にランタイムが保持する長さを受け取り、ホストはランタイムの Region 型を名前で指定しない。したがって `@extern(wasm)` と `@extern(rust)` の両方を持つバインディングは両ホストで同じ引数形を持つ（#4353）。

```almide
import ffi

@extern(wasm, "sys", "log")
@extern(rust, "crate::sys", "log")
fn log(buf: Region) -> Unit = _
```

```rust
// native/sys.rs
pub fn log(base: *mut u8, len: usize) {
    let bytes = unsafe { std::slice::from_raw_parts(base, len) };
    println!("{}", String::from_utf8_lossy(bytes));
}
```

ポインタが有効なのはその呼出し中だけで、ホストが書き込めるのは Region が書込み可能（`ffi.alloc`）な場合だけ。外部領域の解放関数は、その領域の最後のコピーが消えたときに 1 回実行される。Almide コードから解放はしない。

`import ffi` が明示的な有効化になる。stdlib 外では、`import ffi` を書いていないファイルで `ffi` 関数を呼ぶと E100（#4043）。`almide check
--profile critical` はインポート済みでもすべての `ffi.*` 呼出しを拒否する（E100、#4008）。`Built` 行には全 `@extern` 関数と全 `@region` 長さ節を示す。バインディングが述べる長さも外部コードも、信頼する対象である。

**両ターゲット。** ベースはネイティブでは `*mut u8`、wasm では線形メモリのオフセットで、観測されることはない。移動するバイトと `err` テキストは両方で同じ（`spec/wasm_cross/bytes_rawptr.almd`、C-062）。

```almide check
import ffi

effect fn main() -> Unit = {
  let r = ffi.alloc(4)!
  ffi.write(r, 0, bytes.from_string("hi"))!
  let back = ffi.read(r, 0, 2)!
  println(bytes.to_string(back) ?? "")
  let past = match ffi.read(r, 2, 4) {
    ok(_) => "read",
    err(e) => e,
  }
  println(past)
}
```

**スライス。** `ffi.slice(r, off, n)` は `r` 内の `off` から `n` バイトのビュー。`0 <= off` かつ `off + n <= ffi.len(r)` でなければ `err` となるので、親の外側には届かない。親が書込み可能でもスライスは読取り専用。親のメモリを読むため、後から親に書くとビューにも反映される。親のブロックを保持するので、メモリの寿命はビューより長い。スライスを `@extern` に渡すと、コピーなしで部分範囲を渡す。

```almide check
import ffi

effect fn main() -> Unit = {
  let r = ffi.alloc(4)!
  let tail = ffi.slice(r, 2, 2)!
  ffi.write(r, 2, bytes.from_list([7, 8]))!
  println(bytes.to_list(ffi.read(tail, 0, 2)!) |> list.map((x) => int.to_string(x)) |> list.join(","))
  let far = match ffi.slice(r, 3, 2) {
    ok(_) => "sliced",
    err(e) => e,
  }
  println(far)
}
```

**コピーなしのホストバッファ**（ADR-0031 改訂、2026-10-29）。長さは常に Region から得る。

- *Push、wasm。* `@export(wasm)` 関数は Region を返せる。複数値の戻り値 `(ptr: i32, len: i32)` として境界を越える。ホストは Region のブロックの持分を 1 つ保持し、プログラムが自分のコピーをどう扱っても、ホストがモジュールの `__region_release` export に `ptr` を渡すまではバイトの位置が変わらない。ホストは export が返した `ffi.alloc` Region に直接書き込み、次の export を呼んでそれを読ませられる（プログラム側のコピーはトップレベルの `var` が保持）。空の Region は `(0, 0)` で渡る。スライスはコピーせず自分自身のアドレスで渡り、その持分がホストの解放まで親を生かす。同じアドレスを 2 回解放するとトラップする。`effect fn` export の `err` は表示してトラップする。この組はビューのデータアドレスと Region 自身の長さであり、Region レコードのアドレスではない（#4354）。`--host js` ではグルーが export を `f(…) → [ptr, len]` として包み、`regionRelease(ptr)` を追加する。
- *wasm export への貸出し。* `@export(wasm)` 関数は Region 引数を取れる。ホストはインポートが受け取るのと同じ `(base, len)` を、メモリのアドレス幅で渡す。関数はモジュールメモリのその範囲に対する読取り専用 Region を 1 回の呼出し用に借りる。何も所有せず、各アクセスはホストの長さに対して検査する。本体がコピーをトップレベルの `var` に保存する、返す、クロージャに捕捉する場合は E119。通常ホストは push export から得た範囲を返し、その持分が生存を維持する。
- *Pull。* `@extern(wasm)` または `@extern(c)` 関数に渡す書込み可能な `ffi.alloc` Region は、その呼出し中だけ貸し出される。メモリアドレス幅（wasm32 では `i32`）の `(base, len)`、または `(uint8_t*, size_t)` として渡り、外部コードがその場で埋める。
- *C 呼出し元。* `@export(c)` 関数は Region 引数を取れる。呼出し元の `(const uint8_t*, size_t)` なら読取り専用。宣言が `@region(out = p)` で名指す引数なら `(uint8_t*, size_t)` で書込み可能。どちらも 1 回の呼出し中だけ借りる。各アクセスは引き続き呼出し元が渡す長さに対して検査する。本体がコピーをトップレベルの `var` に保存する、返す、クロージャに捕捉する場合は検査時に E119。別の方法でコピーを保持すると、呼出し元バッファより長く生存させる代わりに、呼出しから戻るときプロセスを停止する。

```almide
import ffi

var inbox: Region = ffi.region(bytes.new())

@export(wasm, "inbox_open")
effect fn inbox_open(n: Int) -> Region = {
  inbox = ffi.alloc(n)!
  inbox
}

@export(wasm, "inbox_sum")
effect fn inbox_sum(part: Region) -> Int =
  bytes.to_list(ffi.read(part, 0, ffi.len(part))!) |> list.fold(0, (a, x) => a + x)

@export(c, "fill")
@region(out = out)
effect fn fill(out: Region, byte: Int) -> Int = {
  ffi.write(out, 0, bytes.from_list(list.repeat(byte, ffi.len(out))))!
  ffi.len(out)
}
```

削除済みの表記（dialect epoch 15。`almide fix` は最初の 3 つを改名する）。

| 削除済み | 代わりに書くもの |
|---|---|
| `ffi.address(b)`, `bytes.as_ptr(b)` | `ffi.region(b)` |
| wasm `@extern` に渡す `bytes.data_ptr(b)` | `@extern` が `Region` を取る：`ffi.region(b)` |
| `ffi.to_int(p)` | なし：アドレスに `Int` 形式はない |
| `bytes.as_mut_ptr(b)` + `bytes.copy_to_ptr` | `ffi.alloc(n)!` + `ffi.write(r, off, b)!` |
| `bytes.from_raw_ptr(p, n)`, `ffi.read(p, n)` | `ffi.read(r, 0, n)!` |
| 型 `RawPtr` | `Handle`、または `@region(…)` 付き `Region` |



<a id="signature-index-8-functions"></a>

## シグネチャ索引（8 関数）

```
// A read-only view of b; b lives as long as the view.
// @since unreleased
ffi.region(b: Bytes) -> Region

// A fresh writable region of n zero bytes; err if n is negative.
// @since unreleased
effect ffi.alloc(n: Int) -> Region

// The region's length in bytes.
// @since unreleased
ffi.len(r: Region) -> Int

// True if write may change the region (ffi.alloc and writable foreign regions).
// @since unreleased
ffi.is_writable(r: Region) -> Bool

// Copy of n bytes at off; err unless 0 <= off and off + n <= len(r).
// @since unreleased
effect ffi.read(r: Region, off: Int, n: Int) -> Bytes

// Writes b at off; err if the region is read-only or b does not fit.
// @since unreleased
effect ffi.write(r: Region, off: Int, b: Bytes) -> Unit

// A copy of the whole region as Bytes.
// @since unreleased
effect ffi.to_bytes(r: Region) -> Bytes

// A read-only view of n bytes at off inside r; err unless 0 <= off and off + n <= len(r).
// @since unreleased
ffi.slice(r: Region, off: Int, n: Int) -> Result[Region, String]
```



## 索引コメントの説明訳

- `ffi.region(b: Bytes) -> Region` — b の読取り専用ビュー。b は少なくともビューが存在する間は生存する。 (@since unreleased)
- `effect ffi.alloc(n: Int) -> Region` — n 個のゼロバイトからなる新しい書込み可能領域。n が負なら err。 (@since unreleased)
- `ffi.len(r: Region) -> Int` — 領域のバイト長。 (@since unreleased)
- `ffi.is_writable(r: Region) -> Bool` — write が領域を変更できるなら true（ffi.alloc と書込み可能な外部領域）。 (@since unreleased)
- `effect ffi.read(r: Region, off: Int, n: Int) -> Bytes` — off から n バイトのコピー。0 <= off かつ off + n <= len(r) でなければ err。 (@since unreleased)
- `effect ffi.write(r: Region, off: Int, b: Bytes) -> Unit` — off に b を書く。読取り専用または b が収まらなければ err。 (@since unreleased)
- `effect ffi.to_bytes(r: Region) -> Bytes` — 領域全体を Bytes としてコピー。 (@since unreleased)
- `ffi.slice(r: Region, off: Int, n: Int) -> Result[Region, String]` — r 内の off から n バイトの読取り専用ビュー。0 <= off かつ off + n <= len(r) でなければ err。 (@since unreleased)
