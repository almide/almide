# unix

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](unix.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](unix.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](unix.zh-CN.md)



> 原文：[unix.md @ 35297a6](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/unix.md)。読者向けの非規範的な固定スナップショットです。自動生成・自動同期の対象ではありません。署名・型・`@since` とコード（コメントを含む）は原文のまま保持し、API 説明の訳を末尾に併記しています。コード例・性能・安全性・ターゲット間の一致は、この翻訳作業では独立実行・検証していません。

> プラットフォーム対応と WASI の説明は固定原文時点のものです。本文の Result 表記と索引の effect 署名表記を統一せず保持しています。不透明な記述子や即時可視性の説明は独立した隔離・同期・安全性の保証ではありません。

Unix ドメインソケット、そこで受け渡すファイル記述子、共有メモリファイル。Wayland、PipeWire、D-Bus の基礎となる記述子受け渡しの層です。`import unix`、`effect fn`。

記述子は不透明な `unix.Fd` です。このモジュールだけが生成するため（`connect`、`listen`、`accept`、`take_fds`、`shm_create`）、TCP ハンドルや裸の `Int` がここでの呼び出しに渡ることはありません。`Fd` は `Read`、`Write`、`Close` なので、他のハンドルと同様、バイトの入出力には [io](../../../../stdlib/io.md) を使います。

- `io.read(fd, max)` で読み取ります。バイトが届くまでブロックし、空の結果は相手が閉じたことを示します。
- `io.write(fd, data)` で書き込むか、バイトとともに記述子を渡すなら `unix.write_fds` を使います。
- `io.close(fd)` で閉じます。ソケット、リスナー、共有メモリファイル、受信した記述子のいずれも同じです。ソケットを閉じると、受信済みでまだ取り出していない記述子も閉じます。

すべての呼び出しは `Result` を返します。`!` を書くか `err` を処理してください（ADR-0008）。`take_fds` の戻り値は通常のリストですが、`effect fn` としての呼び出しは `Result` になるため、`unix.take_fds(sock)!` とします。

**ターゲット。** ネイティブでは Unix プラットフォーム（Linux、macOS、BSD 群）ですべての関数を提供し、それ以外では `err` を返します。`almide run --target wasm`（組み込みホスト）は暫定ホスト操作経由で同じコードを提供します（#3620。ADR-0023 の手順7でこの経路を WASI 0.3 コンポーネントとして動かすときに廃止）。原文では WASI は Unix ドメインソケットも共有メモリも持たないため、標準成果物（`almide build --target wasm`）ではビルド時にこのモジュールを拒否するとしています（E081）。

## ソケット

### `unix.connect(path: String) -> Result[Fd, String]`

`path` のリスナーへ接続したストリームソケット。

### `unix.listen(path: String) -> Result[Fd, String]`

`path` にバインドしたリスナー。パスはまだ存在していてはいけません。

### `unix.accept(listener: Fd) -> Result[Fd, String]`

`listener` への次の接続をブロックして待ち、そのソケットを返します。

### `unix.write_fds(sock: Fd, data: Bytes, fds: List[Fd]) -> Result[Unit, String]`

`data` 全体を送り、最初のバイトに記述子 `fds`（最大28個）を載せます（SCM_RIGHTS）。相手は同じ開いているファイルを指す新しい記述子として受信します。

### `unix.take_fds(sock: Fd) -> List[Fd]`

`sock` で受信済みでまだ取り出していない記述子を、古い順で返します。記述子が載っていたバイトを `io.read` が受信すると、ソケットごとのキューに入ります。

### `unix.select(xs: List[Fd], timeout_ms: Int) -> Result[List[Fd], String]`

`xs` のうち `timeout_ms` 以内に読み取り可能（データ、接続、相手の切断）になる記述子を、渡された順で返します。Wayland ソケットとコンポジターのイベントソケットなど、複数の入力元を一度に待てます。負のタイムアウトはどれかが読み取り可能になるまで待ち、`0` は待たずに確認だけをするため、ブロックせずイベントループを進めるのに使えます。読み取り可能になったものがなければ空です。

```almd check
import io
import unix

effect fn main() -> Unit = {
  let l = unix.listen("/tmp/demo.sock")!
  let a = unix.connect("/tmp/demo.sock")!
  let b = unix.accept(l)!
  io.write(a, bytes.from_string("hello"))!
  let ready = unix.select([b], 1000)!
  println("${list.len(ready)} ready")
  let got = io.read(b, 64)!
  println(bytes.to_string_lossy(got))
  io.close(a)!
  io.close(b)!
  io.close(l)!
}
```

## 共有メモリファイル

内容を埋めて相手に渡し、相手がマップする無名ファイルです。Wayland バッファのピクセルやオーディオリングなどに使います。書き込みは、マップの有無にかかわらず、そのファイルを保持するすべてのプロセスに即座に見えます。

### `unix.shm_create(size: Int) -> Result[Fd, String]`

`size` バイトの新しいゼロ埋め無名ファイル。

### `unix.shm_resize(fd: Fd, size: Int) -> Result[Unit, String]`

ファイルを `size` バイトに拡大または縮小します。

### `unix.shm_write(fd: Fd, offset: Int, data: Bytes) -> Result[Unit, String]`

ファイルのバイト `offset` に `data` を書き込みます。負のオフセットやファイル末尾を超える書き込みはプログラムを中断します。先に `shm_resize` してください。

### `unix.shm_read(fd: Fd, offset: Int, len: Int) -> Result[Bytes, String]`

ファイルの `offset` から最大 `len` バイト。末尾ではそれより少なくなります。

```almd check
import io
import unix

effect fn main() -> Unit = {
  let l = unix.listen("/tmp/demo.sock")!
  let a = unix.connect("/tmp/demo.sock")!
  let b = unix.accept(l)!
  let file = unix.shm_create(4096)!
  unix.shm_write(file, 0, bytes.from_string("pixels"))!
  unix.write_fds(a, bytes.from_string("take this"), [file])!
  let _ = io.read(b, 64)!
  let got = unix.take_fds(b)!
  match list.get(got, 0) {
    some(f) => println(bytes.to_string_lossy(unix.shm_read(f, 0, 6)!)),
    none => println("no descriptor arrived"),
  }
}
```


## 原文固定スナップショットの API 索引（10関数）

```
// Stream socket connected to the listener at path.
// @since unreleased
effect unix.connect(path: String) -> Fd

// Listener bound at path, which must not exist yet.
// @since unreleased
effect unix.listen(path: String) -> Fd

// Blocks for the next connection on listener.
// @since unreleased
effect unix.accept(listener: Fd) -> Fd

// Sends all of data, the descriptors fds riding with its first byte.
// @since unreleased
effect unix.write_fds(sock: Fd, data: Bytes, fds: List[Fd]) -> Unit

// Descriptors received on sock and not yet taken, oldest first.
// @since unreleased
effect unix.take_fds(sock: Fd) -> List[Fd]

// The descriptors of xs readable within timeout_ms, in order; < 0 waits for one, 0 only checks.
// @since unreleased
effect unix.select(xs: List[Fd], timeout_ms: Int) -> List[Fd]

// New zero-filled unnamed file of size bytes.
// @since unreleased
effect unix.shm_create(size: Int) -> Fd

// Grows or shrinks the file fd to size bytes.
// @since unreleased
effect unix.shm_resize(fd: Fd, size: Int) -> Unit

// Writes data into fd at offset; a negative offset or a write past the end aborts (shm_resize first).
// @since unreleased
effect unix.shm_write(fd: Fd, offset: Int, data: Bytes) -> Unit

// Up to len bytes of fd from offset; fewer at its end.
// @since unreleased
effect unix.shm_read(fd: Fd, offset: Int, len: Int) -> Bytes
```

## API 説明の訳（上記署名順）

- `effect unix.connect(path: String) -> Fd`：path のリスナーに接続したストリームソケット。
- `effect unix.listen(path: String) -> Fd`：path にバインドしたリスナー。path はまだ存在していてはいけません。
- `effect unix.accept(listener: Fd) -> Fd`：listener の次の接続をブロックして待ちます。
- `effect unix.write_fds(sock: Fd, data: Bytes, fds: List[Fd]) -> Unit`：data 全体を送り、最初のバイトに記述子 fds を載せます。
- `effect unix.take_fds(sock: Fd) -> List[Fd]`：sock で受信済みでまだ取り出していない記述子。古い順です。
- `effect unix.select(xs: List[Fd], timeout_ms: Int) -> List[Fd]`：xs のうち timeout_ms 以内に読み取り可能な記述子を順に返します。< 0 はどれかを待ち、0 は確認のみ。
- `effect unix.shm_create(size: Int) -> Fd`：size バイトの新しいゼロ埋め無名ファイル。
- `effect unix.shm_resize(fd: Fd, size: Int) -> Unit`：ファイル fd を size バイトに拡大または縮小します。
- `effect unix.shm_write(fd: Fd, offset: Int, data: Bytes) -> Unit`：fd の offset に data を書き込みます。負の offset や末尾を超える書き込みは中断します（先に shm_resize）。
- `effect unix.shm_read(fd: Fd, offset: Int, len: Int) -> Bytes`：fd の offset から最大 len バイト。末尾ではより少なくなります。
