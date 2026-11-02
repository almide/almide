<a id="adr-0025-wasm-process-is-a-private-host-capability--almideprocessspawn-shipped-only-when-the-op-set-names-it-refused-at-load-by-a-stock-host-bounded-by-permissions-proc"></a>

# ADR-0025: Wasm の `process` は private なホスト能力 — `almide:process/spawn` は op 集合に現れる場合だけ組み込まれ、標準ホストはロード時に拒否し、`[permissions] proc` で制限される

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](0025-wasm-process-is-a-private-host-capability.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](0025-wasm-process-is-a-private-host-capability.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](0025-wasm-process-is-a-private-host-capability.zh-CN.md)

> [英語の正本](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0025-wasm-process-is-a-private-host-capability.md)（固定コミット 35297a612fb29e3f78f6587aba253aa91d51f466）の非規範的な履歴参照用スナップショット。正本が優先され、この資料は自動同期されない。以下の Accepted 状態、測定、実装に関する主張は原文の記録であり、現時点で独立して検証した結果ではない。記載のコマンドは実行していない。

- **状態**：Accepted。所有者は 2026-10-01（#2589）に **A** と裁定した。wasm では `process` を private な `almide:process/spawn` WIT import 経由で利用可能とする。
- **日付**：2026-10-03
- **範囲**：`--target wasm` における `process` のサブプロセス群（`exec`、`exec_in`、`exec_with_stdin`、`exec_status`、`exec_status_timeout`、`run`（#3379 までは `exec_attached`）、`run_in`、`spawn`、`kill`、`is_alive`、`pid`）。組み込みホスト（`almide run --target wasm`、`almide test` の wasm 経路）、p1 ビルド成果物（`almide build --target wasm`）、`almide.toml` の `[permissions] proc`、`proofs/target-availability.toml`。
- **例外の対象**：[ADR-0023](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0023-wasm-http-over-wasi-http.md) §2.6（「標準ランタイムを対象とする者は private なホスト ABI を出荷しない」）および §3.1 の項目 1（「新しい op をホストが提供することはない」）。この記録がその例外を設ける唯一の場所であり、§3 がその範囲を制限する。
- **関連**：#2589（本決定）、#2587（ハンドル形式の process API）、#1423（ターゲット可用性）、#1628（コンポーネントモデル）、#2588（`wasi:http/service`。同じ「ホストが world を満たす」構造）。

<a id="1-context"></a>

## 1. 背景

<a id="11-two-models-of-process-lived-side-by-side"></a>

### 1.1 `process` の 2 つのモデルが併存していた

- `proofs/target-availability.toml` はサブプロセス群を **native-only-forever** に分類していた。「標準 WASI（0.2 と 0.3）にはサブプロセス API がない」（#1423 の bucket C、2026-08-31）。これらの fn はすべて structural、stock-p1、embedded の各経路で壁となり、`spec/stdlib/process_exec_status_test.almd`、`process_ext_test.almd`、`process_timeout_test.almd` は `proofs/wasm-fallback-baseline.txt` に HOST_CAPABILITY として記載されていた。
- `docs/wasm/capability-system.md`（2026-07-31）は `Proc` をホストが付与する能力として扱い、`almide_host` モジュールの `almide_host_proc` import を挙げていた。この import は存在したことがない。`git grep almide_host_proc` では、その文書内だけに見つかった。

<a id="12-what-the-platform-offers-2026"></a>

### 1.2 プラットフォームが提供するもの（2026）

- WASI 0.3.0（2026-06-11）は cli、clocks、filesystem、http、random、sockets を標準化する。サブプロセスインターフェースはなく、WASI の提案一覧にも候補はない。子プロセスの起動はサンドボックスの外に出るため、標準化される可能性は低い。
- WASIX（Wasmer）には非標準の `proc_spawn` があるが、wasmtime はこれを実行しない。
- wasm ランタイムは最初の命令が実行される前にすべての import を解決する。ホストが定義しないものを import するモジュールはインスタンス化時に拒否される。そのため「この環境は子プロセスを起動できない」は、実行時の予期せぬ事態ではなく、ロード時の判定にできる。

<a id="13-the-goal-behind-the-ruling"></a>

### 1.3 裁定の背景にある目標

wasm 向けにビルドできるプログラムは、すべてそうできるべきである。`almide test` の wasm 経路で成功しなかった 11 ファイルのうち、3 個がこれらのサブプロセスのファイルだった。

<a id="2-decision"></a>

## 2. 決定

<a id="21-one-interface-written-in-wit"></a>

### 2.1 WIT で記述する 1 つのインターフェース

`crates/almide-wasm-run/wit/process/spawn.wit`：

```wit
package almide:process;

interface spawn {
  enum op { exec, exec-in, exec-with-stdin, exec-status, exec-status-timeout,
            run, spawn, kill, is-alive, pid, run-in }
  call: func(op: op, a: string, b: string) -> result<string, string>;
}
```

すべての操作は 2 つのテキストオペランドを受け取り、ok(text) または err(text) を返す。引数リストは十進の CHAR 長セル（`<chars>\n<payload>`、http_framed のセル形式）で渡され、3 つ目のオペランドが必要な操作は最初のセルにそれを入れる。WIT コメントが各オペランドと応答を定義し、`crates/almide-wasm-run/src/tests/host_process_test.rs` の WIT テストが、ケース順と `call` の形をホストの op 番号に対して固定する。（#3379 はケース 5 の名称を `exec-attached` から `run` に変更し、`run-in` を op 90 として追加したため、それ以前のケースは番号を維持する。）

ケース番号はホスト op から 80 を引いたもの。生成コードとホストの境界にある op 80..=90 である（`crates/almide-wasm/src/fs_meta.rs`、`PROC_LEAVES`）。ゲスト側は Almide（`stdlib/process_wasm.almd`、self-host registry 経由で wasm の各経路にリンク）で、セルを構築し、`exec_status` の応答 `<code>\n<stdout chars>\n<stdout><stderr>` を `ProcessStatus` にデコードする。

<a id="22-who-serves-it"></a>

### 2.2 提供する側

- **組み込みホスト**は、他の組み込みサービスと同様に、`almide.fs_call` 経由で生のモジュールに op 80..=90 を提供し、さらに自身のリンカー上で import `almide:process/spawn.call` 自体を定義する（`almide_wasm_run::link_spawn_import`）。どちらも `crates/almide-rt-core/src/process_core.rs` を実行し、`runtime/rs/src/process.rs` はこれをそのまま `include!` する。したがって native と組み込みホストは、取得したテキスト、終了コード、すべての err 文字列を同じコードから生成する。native 専用なのは、端末上で停止した子プロセスを特定する `waitpid` probe だけ（#2540）。これにはワークスペースで禁止されている `unsafe` が必要なため、core は poll を引数として受け取る。組み込みホストでは、そのような子プロセスは代わりに期限に達する。
- **p1 成果物**（`almide build --target wasm`）。op 集合に process op があると、`to_wasi` は import `almide:process/spawn.call` を追加する。これは `call` を canonical ABI に lowering したもので、形は `(op, a_ptr, a_len, b_ptr, b_len, retptr)`、応答のバイト列はエクスポートされた `cabi_realloc` を通じて配置される。さらに `fs_call` の背後に、staging pair を応答へ向ける forwarder を追加する（`crates/almide-wasi/src/proc_service.rs`）。process op を含まないプログラムには、これらは一切追加されない。この選択は、environ pair が op 26 から受けるもの、および `wants_http` が p3 world に対して行うものと同じである。
- **コンポーネントビルド**（`--component`、p2 または p3）は、`almide:process/spawn` を示す E081 でビルド時に拒否される。まだこのインターフェースを宣言する component world はない（§5）。

<a id="23-what-a-stock-host-does"></a>

### 2.3 標準ホストの動作

標準ホストは `_start` の実行前、ロード時にモジュールを拒否する。2026-10-03、wasmtime 47.0.2 で、3 行の `exec_status` プログラムの成果物について測定した：

```
$ almide build p.almd --target wasm -o p.wasm
note: p.wasm imports almide:process/spawn (process.*, ADR-0025): a stock WASI runtime refuses it at load; it runs on a host that implements that import — `almide run p.almd --target wasm` is one
Built p.wasm (6644 bytes, structural leg, …)
$ wasmtime run p.wasm
Error: failed to run main module `p.wasm`
Caused by:
    0: failed to instantiate "p.wasm"
    1: unknown import: `almide:process/spawn::call` has not been defined
$ almide run p.almd --target wasm
start
code=0 out=hi
```

ビルドがその点を伝え、ランタイムはロード時に拒否するため、プログラムが実行時に誤った答えを返すことはない。

<a id="24-permissions-proc"></a>

### 2.4 `[permissions] proc`

`almide.toml` には、サブプロセス群が起動できるコマンドを列挙できる：

```toml
[permissions]
proc = ["git", "cargo"]
```

- **静的に**（`cli::check_proc_allowlist`。両ターゲットで `run`、`build`、`test` がリンク済み IR に対して実行し、`almide check` は直接呼び出しが見えるリンク前 IR に対して実行する）：子プロセスを起動するすべての呼び出しは、コマンドをリスト内の文字列リテラルで指定しなければならない。リスト外のリテラルは `process.exec("cargo") (line N): `cargo` is not in [permissions] proc` で拒否される。実行時に計算されるコマンドや、値として渡される起動用 fn はチェックできないため、これらも拒否される。したがってコンパイルできるプログラムは、どのターゲットでもリスト外のコマンドを起動しない。
- **組み込みホストの実行時に**（`almide_wasm_run::set_proc_allowlist`）：同じリストが、子プロセスを起動するすべての操作を制限する。リスト外のコマンドには、何かを実行する前に、そのコマンド名を示す err を返す。これにより、静的ゲートが検査していない成果物も制約される。
- キーがなければ、従来どおり何も制限しない。`proc = []` はどのコマンドも許可しない。

`Proc` はまだチェッカー内で独立した effect ではない。`pass_effect_inference` は `process` を `Env` に対応付けるため、現在 process 呼び出しを許可するのは `allow = ["Env"]`。`proc` リストは、その許可の内側で範囲を狭める。`Proc` を `Env` から分離すると、既存のどのマニフェストがコンパイルできるかが変わるため、本決定の対象外とする。

<a id="25-the-availability-ledger"></a>

### 2.5 可用性台帳

`proofs/target-availability.toml` に **host-capability** というクラスを追加する。この fn は、標準 WASI が定義しない能力をホストが付与する場所でのみ動作する。native と組み込みホストでは実行でき、標準向け成果物もビルドできるが、private な `almide:*` import を持つため標準ランタイムはロード時に拒否する。可用性 probe は成果物から import を読み取るため、stock-p1 の壁は宣言ではなく測定される。サブプロセスの 10 行を、stock-p1 経路だけを指定したこのクラスへ移す。`process.env` と `process.sleep` の行は削除する。これらは、すべてのホストが提供する host op 26 と 36 に lowering されるためである。

<a id="3-the-exception-and-its-bounds"></a>

## 3. 例外とその境界

ADR-0023 §2.6 が private なホスト ABI を拒否するのは、ツールチェーンが前提としたホストを持たない標準ランタイムが、そのホスト向け成果物を実行することになるからである。その理由は、lowering 先の標準が存在する場合に成り立つ。HTTP には `wasi:http` がある。サブプロセスには標準も提案もないため、lowering 先がない。wasm 上でこの群全体を拒否すると、ホストなら実行できるプログラムもビルド不能になる。本 ADR は 4 条件のもとで private インターフェースを許可する。どれか 1 つでも外せば、例外は成立しない。

1. **op 集合による選択。** import が組み込まれるのは、生成された op 集合が process op を含む成果物だけ。process を使わないプログラムの成果物は、本決定以前の成果物とバイト単位で同一であり、サイズと割り当ての台帳が証拠となる。
2. **ロード時に明確に拒否。** import を持たないホストは、実行前にモジュールを拒否する（§2.3）。黙って失敗したり、誤った答えで失敗したりしない。
3. **マニフェストで制限。** `[permissions] proc` は静的にも組み込みホスト上でも成立する（§2.4）。
4. **単一の閉じたインターフェース。** 例外は `almide:process/spawn` だけで、それ以外は含まない。別の private な `almide:*` インターフェースには独自の記録が必要。このインターフェース内の新操作は `enum op` の新しいケースとし、WIT テスト、ホスト側の分岐、cross fixture と一緒に 1 つの変更で導入する。

<a id="4-evidence"></a>

## 4. 証拠

- `spec/stdlib/process_exec_status_test.almd`、`process_ext_test.almd`、`process_timeout_test.almd` が組み込みホスト上の `almide test` の wasm 経路で成功し、`proofs/wasm-fallback-baseline.txt` から外れる。
- `spec/embedded_cross/process_spawn_family.almd` は、成功、非ゼロ終了状態、起動失敗、期限到達、大量出力する子プロセス、`spawn` / `is_alive` / `kill`、CHAR セルを通る非 ASCII 出力という、すべての操作を試す。`almide run` と `almide run --target wasm` はその出力がバイト単位で一致する（`tests/embedded_cross_test.rs`）。
- `crates/almide-wasm-run/src/tests/host_process_test.rs`：canonical import は `cabi_realloc` を通じて応答を配置する。op 83 を持つ `to_wasi` 成果物には import が含まれ、それと 2 つの preview-1 呼び出しを定義するホスト上で正しい出力を得る。import のないホストはモジュールを拒否する。process op のない op 集合には import が付かない。WIT のケース順はホスト op と一致する。
- `tests/proc_allowlist_test.rs`：`check`、native の `run`、wasm の `run`、wasm の `build` における静的ゲートを、リスト外リテラル、計算されたコマンド、値として渡される起動用 fn について確認する。

<a id="5-alternatives"></a>

## 5. 代案

- **native-only-forever を維持する。** ホストが提供できるにもかかわらず、3 個の spec ファイルと、外部コマンドを呼ぶすべてのプログラムが wasm 向けにビルド不能のままとなる。裁定はこれを却下した。
- **WASIX の `proc_spawn`。** 別ベンダー独自の意味論を持つ private ABI であり、wasmtime は実行しない。採用しても最初の private ABI はなくならず、2 つ目が増える。
- **ロード時ではなくビルド時（E081）に拒否する。** 以前の動作である。明確な拒否ではあるが、提供可能なホスト向けにも成果物を作れなくする。裁定は標準ホストでのロード時拒否を求めている。成果物の形がまだ存在しないコンポーネントビルドでは E081 を維持する。
- **今すぐ component world import を導入する。** p2/p3 shim はホストが生成したすべての値を、予約規律に従う `cabi_realloc` 経由で配置する（#2119）。これは shim が呼び出し前に上限を選ぶために成り立つ。サブプロセスの応答にはゲストが選べる上限がない。`almide:process/spawn` をコンポーネントに持ち込むには、先にストリーミング応答（`stream<u8>`、#2587 のハンドル形式）が必要。それまではコンポーネント経路を E081 で拒否する。
- **今すぐリソース形式のインターフェースを導入する**（`resource child { wait, stdout, stop }`、#2589 のスケッチ）。現在の API は同期式で、ハンドル API とその `Exit` 型は #2587 が決める。`enum op` を持つ `call` が現在の API をカバーする。ハンドルは `call` を壊すことなく、同じパッケージの新しいケース、または新しい `resource` として追加できる。

<a id="6-consequences"></a>

## 6. 結果

利点：

- すべてのサブプロセス fn が組み込み wasm ホスト上でビルド・実行でき、native と同じ観測結果を持つ。wasm fallback 一覧から 3 ファイルが外れる。
- process を使わない成果物は変わらない。
- 能力が成果物内に見える。import 一覧が子プロセスを起動できるかを示し、許可しないホストはロード時に拒否する。

コスト：

- 組み込みホストが 10 個の新しい op を提供する。これは本来 ADR-0023 §3.1 の項目 1 が禁止している（§3 で範囲を限定する）。
- process を使うプログラムの p1 成果物は、`almide:process/spawn` を実装するホストでのみ動作する。標準ランタイムは設計どおり拒否する。
- p1 成果物の `cabi_realloc` はヒープ境界を進め、解放しない。各応答はインスタンスの生存期間を通して、その長さ分のコストを負う。
- 端末上で停止した子プロセスは native ではその状態が特定され、組み込みホストでは期限に達する（§2.2）。

<a id="7-falsifiers"></a>

## 7. 反証条件

次のいずれかが観測されたら、本決定を撤回または改訂する：

1. WASI 標準（または批准された提案）がサブプロセスインターフェースを定義する。その場合、この群をそこへ lowering し、採用する ADR によって本 private インターフェースを廃止する。
2. 標準ランタイムが、`almide:process/spawn` を持つ成果物をその定義なしにインスタンス化する、または拒否する代わりに実行して誤答を返す。この場合、ロード時の保証は偽となる。
3. op 集合に process op がないプログラムに import が付く、またはこの機構によって成果物のバイト列が変わる。この場合、選択が壊れている（サイズのラチェットが測定する）。
4. `spec/embedded_cross/process_spawn_family.almd` が native と組み込みホストで異なる結果になる。この場合、shared-core の主張は偽となる。
5. `[permissions] proc` のもとでビルドしたプログラムが、いずれかのターゲットでリスト外のコマンドを起動する。

<a id="8-references"></a>

## 8. 参考資料

- WASI 0.3：https://wasi.dev/releases/wasi-p3、https://bytecodealliance.org/articles/WASI-0.3
- WASI 提案：https://github.com/WebAssembly/WASI/blob/main/docs/Proposals.md
- wasmtime の Component Model：https://component-model.bytecodealliance.org/running-components/wasmtime.html
- Canonical ABI（平坦化、`cabi_realloc`、`MAX_FLAT_RESULTS`）：https://github.com/WebAssembly/component-model/blob/main/design/mvp/CanonicalABI.md
- Deno `--allow-run`：https://docs.deno.com/runtime/fundamentals/security/

---

原文は 11 個の op 番号（80..=90）を記しつつ、古い「10 個」の行数／op 数も残している。この履歴上の記述を黙って整合させていない。「提案なし／可能性が低い」は当時のプラットフォーム評価で、現在の標準調査ではない。proc キーがなければ制限されず、private host import は一般的なサンドボックス保証ではない。
