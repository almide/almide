<a id="adr-0027-effect-functions-are-inspected-in-a-fixed-host-world--a-seeded-sandbox-property-verdicts-for-what-the-host-decides-and-loopback-fixtures-for-the-network"></a>

# ADR-0027: effect 関数は固定したホスト世界で検査する — 初期状態を用意したサンドボックス、ホストが決める値の性質判定、ネットワーク用ループバック fixture

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](0027-effect-functions-are-inspected-in-a-fixed-host-world.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](0027-effect-functions-are-inspected-in-a-fixed-host-world.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](0027-effect-functions-are-inspected-in-a-fixed-host-world.zh-CN.md)

> [英語（日本語の引用を含む）の正本](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0027-effect-functions-are-inspected-in-a-fixed-host-world.md)（固定コミット `35297a612fb29e3f78f6587aba253aa91d51f466`）の非規範的な読者用スナップショット。元の ADR が正本であり、変更しない。この写しは自動同期されず、チェッカーや生成器の入力ではない。以下の決定、日付、測定値、実装状態、主張は固定した当時の原文を伝えるもので、独立検証や現行リリースの保証ではない。コードブロックとそのコメントは原文のまま保持する。

> 原文の注意点：行数、検査状態、作業順序は日付付きの歴史的記述であり、検査活動の完了証拠ではない。性質の parity 判定はホストが選んだ値の一致でも、普遍的な統計保証でもない。ループバック・キー不要という隔離の説明はこの fixture に限り、任意の本番ネットワーク操作には拡張しない。

- **状態**: 採択（D1–D8、2026-10-17）。
- **日付**: 2026-10-17
- **範囲**: 結果がホストに依存する stdlib 関数に対して、`xtarget-fuzz inspect` が五つの検査基準（parity、complexity、alloc、fuzz、diamond）をどう適用するか。対象は fs、env、args、io、process、random、datetime、http、net、`bytes` と `mem` のホストアドレス関数、および `list.shuffle`。
- **基礎とする設計**: [effect-inspection-harness.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/effect-inspection-harness.md)（2026-10-12 の設計、層 E1–E4）。本 ADR はこれを検査ハーネスの決定にし、設計に残っていた未決部分 — 実行系、非決定的な値の比較方法、ホストアドレス関数、effect 行に対する各基準の意味 — を埋める。
- **関連**: #3552（検査活動）、#1710（wasi:http@0.3）、#1827（wasm 未対応関数）、[ADR-0023](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0023-wasm-http-over-wasi-http.md)、[ADR-0025](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0025-wasm-process-is-a-private-host-capability.md)。

<a id="context"></a>

## 背景

2026-10-16 の作業波の後、`proofs/stdlib-inspection.toml` には未検査の行が **150** ある。そのうち 140 はホストに依存する：

| モジュール | 行数 | 結果を決めるもの |
|---|---|---|
| fs | 34 | ファイルツリー |
| http | 28 | サーバー |
| net | 26 | 相手、ソケット、共有メモリ |
| process | 17 | 子プロセス、自身の pid、終了 |
| env | 9 | 環境変数、argv、cwd、時計、OS 名 |
| args | 6 | argv |
| io | 6 | stdin、stdout |
| bytes | 6 | ヒープアドレス |
| random | 4 | エントロピー源 |
| datetime | 2 | 時計 |
| mem | 2 | ヒープのマーク |

`list.shuffle`（「シードなし、実行ごとに異なる」と文書化された純粋な `fn`）が 141 個目となる。

現時点の検査ハーネス（`tools/xtarget-fuzz/src/inspect.rs`）は各 probe プログラムをビルドし、そのままのプロセスとして実行する。native は `./bin`、wasm は `wasmtime run`。専用の cwd も argv、env、stdin もなく、stdout をバイト単位で比較する。そのため effect の probe は、wasm でまったく実行できない（preopen がない）か、実行ごとに異なる値（pid やタイムスタンプ）を出力してしまう。

プログラム fuzzer には仕組みの半分がすでにある。`oracle/sandbox.rs` は固定した argv、env、stdin と初期ファイルツリーを持つ新しい環境を作り、`oracle/runner.rs` は各実行系をそこで走らせ、終了後のツリーの snapshot を比較する（層 E1）。検査ハーネスはまだこれを使っていない。

<a id="decision"></a>

## 決定

<a id="d1-the-legs"></a>

### D1. 実行系

effect 行は native と**組込み wasm ホスト**（`almide run --target wasm`、`almide-wasm-run`）を比較する。また `proofs/target-availability.toml` が標準ランタイムによる対応を示す関数については、**標準ランタイム**（`wasmtime run`）とも比較する。

行の `parity` フィールドに、対象とする実行系を記す。組込みホストが対応していない関数は、まず wasm で動くようにし、その後両実行系で検査する（D8）。

<a id="d2-a-fixed-world-per-run-tier-e1-fs-env-args-io-processargs--env--stdin_lines"></a>

### D2. 実行ごとに固定した世界（層 E1: fs、env、args、io、process.args / env / stdin_lines）

すべての probe 実行は、どの実行系でも、どのサイズでも、`oracle/sandbox.rs` から新しい専用サンドボックスを得る：
- cwd として使う初期ファイルツリー；
- 固定した argv、env、stdin。

検査ハーネスは別コピーを作らず、fuzzer の runner を再利用する。

parity は stdout、stderr、終了コード、**実行後のツリー snapshot** を比較する。何も出力しなくても、書込み結果が異なれば乖離である。

レシピはツリー内のパスを指定するので、測定する呼出しは常に成功経路を通る。これは #3671 の規則である：err または none の経路で付けた検査済み印は無効である。

<a id="d3-what-the-host-decides-is-compared-as-a-verdict-not-as-a-value-tier-e2"></a>

### D3. ホストが決めるものは、値ではなく判定として比較する（層 E2）

ホストが選ぶ値（乱数、タイムスタンプ、pid、アドレス）は、実行系間や実行間で一致しない。その probe は値を出力せず、**判定行**を出力する。つまり、すでに検査済みの純粋関数から probe 自体が計算した、その関数の宣言された性質である。

両実行系は同じ判定を出力し、すべての判定は `true` でなければならない。比較自体はバイト単位の等価比較のままで、対象が判定になる。

| 関数 | 性質 |
|---|---|
| `random.int(lo, hi)` | m 回の抽出で lo ≤ r ≤ hi。hi − lo が小さいときは両端に到達する |
| `random.float()` | m 回の抽出で 0 ≤ r < 1 |
| `random.choice(xs)` | 結果は xs の要素。`none` は `[]` の場合だけ |
| `random.shuffle(xs)`, `list.shuffle(xs)` | xs の順列（ソートしたコピーが等しい）。さらに頻度を検査する：m 回のシャッフルで、各位置に各要素が現れる回数が m/n の 6σ 以内。レシピで m を固定した場合、正しいシャッフルでも約 10⁹ 回に 1 回はこれに不合格となる |
| `datetime.now()`, `env.unix_timestamp()`, `env.millis()` | ハーネスが実行直前と直後に読み取って渡す時間窓 `[t₀, t₁]` 内にある |
| `datetime.monotonic_ns()` | 2 回の読取りが非減少である |
| `env.sleep_ms(k)`, `process.sleep(k)` | 呼出し前後の単調時計の差が ≥ k ms |
| `process.pid()` | > 0、かつ `process.is_alive(process.pid())` |
| `process.is_alive(p)` | 実行中の子では true、回収後は false |
| `env.os()` | その実行系の contract が、実行するホストについて指定する値 |
| `bytes.as_ptr` / `as_mut_ptr` / `data_ptr` / `copy_to_ptr` / `from_raw_ptr` / `heap_restore` | 往復：ポインターを通してコピーし、読み戻したバイト列が等しい。また、値が生存している間はポインターが安定している |
| `mem.save` / `mem.restore` | その間の確保が解放される（生存数がマークに戻る） |
| `net.tcp_is_open(c)` | fixture（D5）への接続中は true、`tcp_close` 後は false |

この種の行の `parity` フィールドは `PASS (properties: …)` で始め、値の一致と誤読されないようにする。

<a id="d4-processes-run-a-fixed-command-set-tier-e3"></a>

### D4. プロセスは固定したコマンド集合を実行する（層 E3）

probe が起動するのは次だけである：
- `/bin/echo`；
- stdin を入力する `/bin/cat`；
- `/usr/bin/true`；
- 失敗終了ステータス用の `/bin/sh -c 'exit 3'`；
- timeout 形式用の `/bin/sleep`。

ハーネスはすべての子プロセスを回収する。`process.exit(c)` は終了コードだけで判定する。

<a id="d5-the-network-is-a-loopback-fixture-tier-e4"></a>

### D5. ネットワークはループバック fixture（層 E4）

実行ごとにハーネスは 127.0.0.1 とサンドボックス内の Unix ソケットに fixture を起動し、実行後に停止する。fixture は次を提供する：
- method、path、headers、body をエコーし、あらかじめ決めたステータスコード、chunked 応答、SSE ストリームを提供する HTTP サーバー；
- TCP エコーサーバー；
- Unix エコーサーバー。

プログラムは固定した環境変数からアドレスを読む。`anthropic_` / `openai_` のストリーミング呼出しは、base URL を通して SSE endpoint に到達する。外部ホストには接続せず、API キーも使わない。

<a id="d6-how-each-criterion-reads-for-an-effect-row"></a>

### D6. effect 行における各基準の意味

- **Complexity。** サイズパラメーターは呼出しを通るデータ、すなわちファイル長、行数、payload のバイト数、リスト長である。固定したホスト操作（`exists`、`mkdir_p`、`tcp_close`）を行う関数は order 1 と宣言する。カウント型メーター（`instructions:u` と移動バイト数）はプログラム自体の作業を測る。カーネル時間は対象外であり、検査済み印にも明記する。
- **Alloc。** 変更なし：呼出しは生存したものを残さない。例外は返したリソースハンドルであり、probe がカウント前に解放する。
- **Fuzz。** 生成器は世界から引数を取り、これらの関数を選ぶ：ツリー内パス、fixture のアドレス、`lo..hi` 範囲。生成したプログラムが E2 の値に到達した場合、値ではなくその値の判定を出力する。
- **Diamond。** 各経路を専用の新しいサンドボックス（または fixture）で実行し、三つの経路で同じ条件にする。

<a id="d7-the-table-is-one-place"></a>

### D7. 表は一か所に置く

各関数の層と D3 の性質を、`tools/xtarget-fuzz` 内の一つの表に置く。テストは、その表で分類されていない台帳の effect 行を拒否する。同じ規則を純粋関数にも適用し、宣言の外で名前による特別扱いをしてはならない。

<a id="d8-nothing-is-stamped-native-only"></a>

### D8. native 専用の検査済み印は付けない

現時点でどの wasm 実行系も対応していない関数は、先に移植し、その後両実行系で検査する。http streaming family、`http.*_response`、`net.unix_*`、`net.shm_*`、量子化行列カーネル、test ブロック内の `testing.assert_throws` がこれに含まれる。2026-10-12 に裁定した native 専用の印（plan B）は、「未検査 = 0」のためには使わない（owner、2026-10-17：「未対応は当然wasmでも動くようにした上で両方検品です」）。

<a id="alternatives-considered"></a>

## 検討した代替案

- **検査のため stdlib の effect を mock 化する。** 却下（2026-10-12 の設計と同じ）：検査対象が mock になり、クロスターゲットのバグが存在する実行系の実際のホストバインディングではなくなる。
- **乱数実行をバイト単位で再現する seed スイッチ（`ALMIDE_RANDOM_SEED`）。** 却下。通常のプログラムが依存しかねない隠れたランタイムモードを増やす。同じ seed を持つ二つの実行系は一つの PRNG も共有しなければならず、ハーネスの都合でランタイムを制約する。D3 の判定は、ランタイムを変えずに API の約束（範囲、順列、一様性）を検査する。
- **ホスト呼出しを記録して再生する。** 却下：記録を判定してしまう。ループバック fixture が不安定と判明した場合に限り、E4 の予備案として残す。
- **ホストアドレス関数を「検査不能」と記す。** 不要：D3 は 14 個それぞれに観測可能な性質を与える。ないと判明した関数があれば、別の問いとして戻す。

<a id="compatibility"></a>

## 互換性

- ツールのみ：`tools/xtarget-fuzz` と検査スクリプト。言語、stdlib、codegen の変更はない。
- ハーネスが見つけた欠陥は、すべての検査作業波と同様、テストを先に書いて修正する。

<a id="order-of-work"></a>

## 作業順序

1. 検査ハーネスに E1（サンドボックス実行、parity にツリー snapshot）を実装し、fs、env、args、io に印を付ける。
2. E2 の判定 probe を実装し、random、時計、shuffle、アドレスとヒープマークの関数に印を付ける。
3. E3 を実装し、process に印を付ける。
4. E4 fixture を実装し、組込みホストが対応する http と net の関数に印を付ける。

<a id="open"></a>

## 未決事項

1. ~~D1–D7 を一括採択すること。~~ 2026-10-17 に採択（○）。
2. ~~組込みホストが未対応の 24 行。~~ 2026-10-17 に決定：移植してから両実行系を検査する（D8）。
3. ~~test ブロック内の `testing.assert_throws`。~~ 同じ裁定（D8）により決定：wasm で実行できるようにしてから両実行系を検査する。
