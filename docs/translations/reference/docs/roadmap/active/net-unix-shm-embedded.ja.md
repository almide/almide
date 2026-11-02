原文の説明メタデータ：net の Unix ドメインソケットと共有メモリファイルのための暫定 embedded host ops 104..=116、および ADR-0023 step 7 での廃止。

<a id="net-unix-sockets-and-shm-on-the-embedded-wasm-lane-interim-ops-104116"></a>

# embedded wasm レーンの net Unix ソケットと shm（暫定 ops 104..=116）

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](net-unix-shm-embedded.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](net-unix-shm-embedded.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](net-unix-shm-embedded.zh-CN.md)

> 原文コミット固定の非規範閲覧用スナップショット。原文を正本とし、自動同期しない。独立した実行、実装検証、証明検査、測定は行っていない。歴史的コマンドや設計指示は資料であり、実行した指示ではない。
> [Original source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/net-unix-shm-embedded.md)

> 原文注記：platform 対応、実装、テスト合格、一致の主張は原文時点の暫定設計を述べ、現在の保証や全実行の証明ではない。生の記述子番号は process 間一致から明示的に除外され、記述子の保持検査と失敗しない take 操作の異なる規則を保持する。step 7 の廃止は約束であって完了の証拠ではなく、提案 component interface の存在も主張しない。WASI の広い表現はこの Unix ソケット／共有メモリファイル群の文脈で保持し、WebAssembly の全メモリ機能に関する主張へ広げない。

Issue [#3620](https://github.com/almide/almide/issues/3620)、route 4（「unix / fd passing / shm」）。net/tcp 設計（TCP steps 1 と 2：共有 `net_core`、ops 91..=103）の兄弟であり、同じ形で構築する。Owner の裁定 2026-10-17：○。

<a id="decision"></a>

## 決定

13 関数 `net.unix_connect`、`unix_listen`、`unix_accept`、`unix_send`、`unix_recv`、`unix_take_fds`、`unix_poll`、`unix_wait`、`unix_close`、`shm_create`、`shm_resize`、`shm_write`、`shm_read` に、TCP とまったく同じく**暫定 embedded host ops** を与える：

| 側 | 変更前 | 変更後 |
|---|---|---|
| native | `runtime/rs/src/net.rs`、生の記述子 | 共有コア、thread ごとに 1 表 |
| embedded（`almide run --target wasm`） | E081 | 同じコア上の ops 104..=116、run ごとに 1 表 |
| structural（`ALMIDE_WASM_SKIP_STOCK_AUDIT`） | 壁 | `stdlib/net_wasm.almd` の本体を lowering |
| stock-p1（`almide build --target wasm`） | E081 | E081、不変 |

stock 成果物は E081 のままである。**WASI には Unix ドメインソケットも共有メモリもない**からだ。どの preview（1、0.2、0.3）も定義しておらず、stock runtime はこれらの呼出しを提供できない。`proofs/target-availability.toml` は、その理由付きで各行の `stock-p1` 側だけを保持する。

<a id="one-implementation"></a>

## 実装は 1 つ

`crates/almide-rt-core/src/net_unix_core.rs` がこの一群の全体を保持する。Native はこれを `include!` し（`runtime/rs/src/net.rs`、薄い `almide_rt_net_unix_*` / `almide_rt_net_shm_*` wrapper）、`almide-wasm-run` はリンクする（`net_unix_host.rs`）。したがってバイト、受信した記述子、すべての err テキストは構成上一致し、`tests/net_unix_embedded_cross_test.rs` がそれを観測する。

**`unsafe`。** 4 つの syscall には stable Rust の安全な std 形式がない。SCM_RIGHTS を伴う `sendmsg` / `recvmsg`、`poll`、`fcntl(F_SETFD, FD_CLOEXEC)` である。workspace は `unsafe` を forbid し、`almide-wasm-run` もその lint を採用するため、embedded host にコピーを置けなかった。2 つのコピーは 2 つの挙動にもなる。代わりに：

- `almide-rt-core` は独自 lint を持つ：`unsafe_code = "deny"`（`forbid` ではない）。
- ただ 1 つの module、`src/net_unix_sys.rs` だけを `#[allow(unsafe_code)]` とする。各 block は SAFETY 行を持ち、受け取る生の記述子はすべて、呼出し全体を通して表が保持する `OwnedFd` から借用する。
- `crates/almide-rt-core/tests/net_unix_core.rs` は、crate の他ファイルに `unsafe` が現れるか、その module の block に SAFETY 行がなければ失敗する。

コアの残り（connect、listen、accept、ファイルの読み書き、close）は安全な std コードである。`rustix` のような crate を追加すれば `unsafe` は除去できただろうが、`net` を使うすべての生成 native project に crates.io 依存を加え、事前ビルド runtime ではなく cargo 経路を使わせることにもなる。

<a id="descriptors"></a>

## 記述子

guest は native が返すのと同じく**生の OS 記述子番号**を受け取る。native process と embedded host の process では番号が異なるため、fixture は番号ではなく記述子の性質を印字する。

コアはプログラムが保持する記述子の表を維持する。自分で開いたもの（`unix_listen`、`unix_connect`、`unix_accept`、`shm_create`）とソケット経由で届いたもの（`unix_recv`）を、`unix_close` まで保持する。それ以外の番号を指定する呼出しは、どのシステムコールよりも前に拒否する：

- 負または `i32::MAX` より大きい：`<fn>: not a file descriptor: <n>`（この変更前の native のテキスト）。
- それ以外：`<fn>: descriptor <n> is not open in this program`。

embedded host では、これにより guest が host 自身の記述子（wasmtime のものや host の stdio）にアクセスしたり閉じたりするのを防ぐ。Native も同じ応答になるため、これは **native の挙動変更**である。以前は `net.unix_close(0)` が process の stdin を閉じ、`net.unix_poll(2, 0)` は stderr を poll していた。現在はソケットを閉じると、そのソケットが受信してまだ取り出されていない記述子も閉じる（native はそれらをリークしていた）。

<a id="operand-encoding-ops-104116"></a>

## オペランド符号化（ops 104..=116）

TCP と同様に op ごとに 2 オペランド：`a` はテキスト、`b` はテキストまたは生バイト。応答は ok / err。

| op | 関数 | a | b | ok 応答 |
|---|---|---|---|---|
| 104 | `unix_connect` | path | — | 記述子 |
| 105 | `unix_listen` | path | — | 記述子 |
| 106 | `unix_accept` | listener | — | 記述子 |
| 107 | `unix_send` | socket、続いて送信する各記述子を 1 行に 1 つ | 生バイト | — |
| 108 | `unix_recv` | socket | max | 生バイト |
| 109 | `unix_take_fds` | socket | — | 記述子、1 行に 1 つ |
| 110 | `unix_poll` | 記述子 | ms | `1` / `0` |
| 111 | `unix_wait` | 記述子、1 行に 1 つ | ms | 準備済み記述子、1 行に 1 つ |
| 112 | `unix_close` | 記述子 | — | — |
| 113 | `shm_create` | size | — | 記述子 |
| 114 | `shm_resize` | 記述子 | size | — |
| 115 | `shm_write` | `<fd>\n<offset>` | 生バイト | — |
| 116 | `shm_read` | 記述子 | `<offset>\n<len>` | 生バイト |

番号は tcp 一群（91..=103）に続く。他の表は 104..=116 を使わない。p1 op 集合（`almide-wasi` の `P1_SERVED_OPS`、`FS_SERVICE_OPS`）、http 43..=65、serve 70..=72、73 / 74、process 80..=90、`component_availability::operation_name` を確認した。

<a id="retirement-commitment"></a>

## 廃止の約束

**ADR-0023 step 7 で embedded レーンを p3 component として実行するとき、ops 104..=116 を廃止する。** 同じ step で tcp 一群の ops 91..=103 も廃止し、以後は `wasi:sockets` が提供する。この一群には WASI の対応物がないため、step 7 では component world の Almide 独自 interface（#3620 の計画）を通して提供するか、それが存在するまで wasm では利用不可にする。どちらの場合も、この core-module ops を step 7 後に残さない。この約束は op 表にも記載する：`crates/almide-wasm-run/src/net_unix_host.rs` と `crates/almide-wasm/src/net_call.rs`。

<a id="evidence"></a>

## 証拠

- `tests/net_unix_embedded_cross_test.rs` と `spec/embedded_cross` の sweep：`spec/embedded_cross/net_unix_loopback.almd`（C-382、全関数の成功経路、双方向の記述子受渡し）と `net_unix_errors.almd`（C-383、全関数のエラー経路）は native と `almide run --target wasm` で同じ行を印字し、どちらの側も最後の `done` 行まで到達する。第 3 のテストが native のエラーテキストを固定する。
- `crates/almide-rt-core/tests/net_unix_core.rs`：表による拒否、取り出していない記述子の close、`unsafe` の許可範囲。
- `tests/net_unix_test.rs`（native、変更なし）は引き続き通る。

<a id="contracts"></a>

## 契約

ALS-R5 の C-382（一致）と C-383（エラー）は、2 fixtures とともに先に judge に入った（almide/als#145）。この tree はそれらを `spec/embedded_cross/` にそのまま保持する。各 fixture は temp dir の新しいランダム path で listen し、最後の行の前に削除するので、sweep は引数なしで実行できる。記述は次のとおり：

1. **一致。** ALS-R5 (#3620)：Unix host の native 側と EMBEDDED wasm レーン（`almide run --target wasm`）で、`net.unix_*` / `net.shm_*` 一群（`unix_connect`、`unix_listen`、`unix_accept`、`unix_send`、`unix_recv`、`unix_take_fds`、`unix_poll`、`unix_wait`、`unix_close`、`shm_create`、`shm_resize`、`shm_write`、`shm_read`）は同一に応答する。受信バイトは一致する。受信記述子は数と送信順序が一致し、各々同じ開かれたファイルの新しい記述子である（一方で書いたものを他方で読む）。`unix_poll` / `unix_wait` の readiness は一致する（データ、保留中の接続、または peer の close。`unix_wait` は与えた順に準備済み記述子を返す）。`shm_read` は同じバイトを返す（作成・拡張時はゼロ埋め、ファイル末尾では少なくなる）。記述子は生の OS 番号であり、側によって異なり得る。性質（非負、開いている間は相互に異なる）は一致する。stock 成果物（`almide build --target wasm`）は、WASI に Unix ドメインソケットも共有メモリもないため、すべてを build 時に拒否する（E081）。
2. **エラー。** ALS-R5 (#3620)：`net.unix_*` / `net.shm_*` 呼出しの失敗はすべて `err` で、trap ではなく、native と embedded wasm レーンでテキストが同じである。呼出しはプログラムが保持する記述子だけに作用する（`unix_listen`、`unix_connect`、`unix_accept`、`shm_create` が返したもの、または `unix_recv` で受信したものを `unix_close` まで）。それ以外はどのシステムコールより前にも拒否する。n < 0 または n > 2^31-1 なら `<fn>: not a file descriptor: <n>`、それ以外なら `<fn>: descriptor <n> is not open in this program`。標準ストリームと閉じた記述子も含む。引数の定義域には Almide 独自のテキストがあり、記述子の後で検査する：`shm_create: negative size <n>`、`shm_resize: negative size <n>`、`shm_write: negative offset <n>`、`shm_read: negative offset or length (<o>, <n>)`、`unix_send: <k> descriptors in one message (at most 28)`。OS の拒否は `<fn>: <the platform's error text>`、`unix_connect(<path>)` と `unix_listen(<path>)` には path が付き、両側は同じ host から取得する。`unix_take_fds` は決して失敗しない。受信物のないソケットや保持していないソケットには空 list を返す。ソケットを閉じると、受信したが取り出していない記述子も閉じる。
