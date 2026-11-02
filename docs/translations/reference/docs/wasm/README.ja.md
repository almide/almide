<a id="almide-wasm-documentation"></a>

# Almide WASM ドキュメント

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](README.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](README.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](README.zh-CN.md)

> 非規範の原文固定読者スナップショット。変更していない原本が正本。自動同期、独立実行、再測定、安全性検証、第三者認証は行っていない。以下の「現行」「実装済み」や断定は固定原文の記述であり、今回の再評価ではない。組み込みホストは一般的なサンドボックスではなく、権限キー省略時と core／component の既定値は異なる。証明書の対象からコンポーネントを除くという原文の限定を維持する。依存関係別権限や名前付き権限プロファイルは将来案のまま。コードとコメントは原文どおり。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/wasm/README.md)

Almide は**単一の wasm 経路**、構造的経路（`src/cli/build.rs` の `render_wasm_module_routed` から入る）を通じて core Wasm モジュールと WASI コンポーネントを出力する。未検証の v0 emitter は #782 で廃止され、従来の v1 経路（`crates/almide-mir` の MIR→WAT 信頼スパイン）は #2752 で最後のルートを失い、#2761 で削除された。構造的経路が lowering できない形は、明示的な壁となる。つまり `wall: <reason>` 行と `--> in <fn>` の位置行を伴う `error[E082]` であり、黙ってフォールバックすることはない。

| 経路 | エンジン | この経路に送られる対象 |
|---|---|---|
| **structural**（唯一の経路） | 稼働に入った新規構築エンジン：`almide::wasm_leg` フロントと `crates/almide-wasm` 直接 emitter。core モジュールは標準 WASI または対応コンポーネントルート向けにパッケージ化される。所有権は、コンパイラが配置する参照カウントと copy-on-write による。emitter は証明書バンドルを記録し、`almide verify` と certified-corpus ゲートは、対応する core 成果物の対象特性を検査する。コンポーネントは対象ではない。差分ゲートと変異ゲートは別の動作証拠を提供する | 構造的 emitter が対応する形と、選択したホストルートの能力の範囲内にあるプログラムおよび公開ライブラリ |

従来経路を強制するスイッチは、そのルートとともに削除された（#2752）。現在 `ALMIDE_WASM_SKIP_STOCK_AUDIT=1` が省略するのは、ビルド時の標準 WASI ホスト操作監査だけである（対応範囲の前線を開発するためのプローブ）。

<a id="what-ships-today"></a>

## 現在提供されるもの

| 特性 | 提供される動作 |
|---|---|
| ターゲット | core `wasm32-wasip1` コマンドと対応 WASI コンポーネントルート。HTTP serve 形のプログラムは WASI 0.3 ハンドラコンポーネントとしてビルドされる |
| メモリ | バンプ割り当て + **Perceus 型参照カウント**。対応する core 成果物（コンポーネントは除く）の構造的証明書は、[明示的なカバレッジと信頼上の限界](../../../../design/MEMORY-SAFETY.md)のもとで `almide verify` と certified-corpus ゲートにより検査される。ランタイムのバイト列は `proofs/check-structural-bytes.sh` により Coq デコーダモデルと照合される |
| 等価性 | 観測可能な出力を契約ごとに比較する。宣言されたホスト観測、リソース制限、オラクル相対の動作には、それぞれ文書化された適用範囲がある |
| 壁 | lowering 部分集合の外 ⇒ 壁と関数を名指しする `error[E082]` をユーザーに示す。`almide test` では `ALMIDE_WALL_REASON=1` がどの段階で拒否したかを示す |

<a id="running-a-server-on-wasm"></a>

## wasm でサーバーを動かす

`almide run app.almd --target wasm` は、構造的経路のモジュールを**組み込みホスト**（`almide` バイナリ内の wasmtime）で動かし、このホストが `http.serve` を提供する（C-367、#2650）。serve ループはゲストが所有する。`main` は一度実行され、`http.serve` を呼び、その後同じインスタンス内でホストからリクエストを1件ずつ受け取り、レスポンスを1件返す。したがって、`main` が `http.serve` より前に計算した値は、ネイティブの場合と同じく、すべてのリクエストで同じ値となる。ホストはネイティブランタイムが使うサーバーコード（`crates/almide-rt-core/src/http_server_core.rs`）を通じてバイト列を移動するだけ（fs_call ops 70..=72）なので、レスポンスのバイト列はネイティブと同一になる。実行中、stderr はバッファリングされず、stdout はネイティブの規則に従う（端末への書き込みは毎回フラッシュし、それ以外は64 KiBバッファ）。そのためサーバーの出力は実行中に表示される。

上記の組み込みルートと、標準サーバー成果物は異なる。対応する serve 形のプログラムを `almide build --target wasm` でビルドすると、`wasmtime serve` 向けの `wasi:http/handler@0.3.0` コンポーネントになる（C-375、#2659）。ビルドはその形とコンポーネントが利用できる操作を検証する。`--host js` はこのサーバー export に対応しない。すべての Wasm ルートを同じホストとして扱うのではなく、ルート選択は `src/cli/build_wasm.rs` を参照する。

<a id="what-the-embedded-host-grants-it-is-not-a-sandbox"></a>

## 組み込みホストが与える権限（サンドボックスではない）

`almide run --target wasm` は、ネイティブバイナリと同じく、実行ユーザーの権限でプログラムを動かす。Wasm はゲスト自身のメモリアクセスを境界内に制限する。マニフェストの権限設定は宣言された能力とファイルシステムルートを制限できるが、これらの制御は一般的なプロセス封じ込めの保証ではない。任意の信頼できないコードを隔離するものとして組み込みホストを扱ってはならない。

| レーン | 使用時 | ファイル | 環境 | その他のホストアクセス | メモリ |
|---|---|---|---|---|---|
| core モジュール（`almide.*` imports、`crates/almide-wasm-run/src/host.rs`） | ほとんどのプログラム | 設定されていれば `[permissions] fs` ルートを、スコープ付きファイルシステム操作で強制する。このキーがなければ、この制御によるパス制限はない | プロセス環境（読み取り。`env.set` はプロセスではなく実行ごとのオーバーレイに書く） | プロセス起動、tcp / unix ソケット、共有メモリ、http クライアントとサーバー、stdin、時計。いずれもネイティブと同様 | 実行のインスタンス、その `fan` worker インスタンス、および `testing.assert_aborts` 子にまたがる、実行ごとに単一の予算（#4135）。テストハーネスが上限を指定した場合はその上限。それ以外は、環境から読み取った4 GiB（wasm32アドレス空間）、ホストの cgroup メモリ上限、物理メモリのうち最小のもの（C-197） |
| p3 コンポーネント（`crates/almide-wasm-run/src/component_host.rs`） | core レーンでは WASI 0.3 経由でのみホスト操作を提供するプログラム（http クライアント群、ADR-0023 step 7） | 設定された `[permissions] fs` ルートを読み書き可能として事前オープン。このキーがなければ作業ディレクトリと一時ディレクトリ（`/` ではない） | 全体を継承し、さらに `ALMIDE_CWD` | stdio を継承。wasi:http はネイティブのクライアント経由。wasi:sockets は wasmtime-wasi の提供どおり | メモリごとの wasm32 の4 GiB以外に store 制限はない |

ファイルシステムのスコープ制限は実装済み（#4022）。core ホストは64-bit Linux/macOSで、設定ルート配下におけるファイルシステム操作を検査して実行する。他のプラットフォームは、検査と使用の競合の影響を受けるパスベースの検査を維持する（`crates/almide-rt-core/src/fs_beneath_sys.rs`）。コンポーネントホストはそれらのルートを、ルート設定がなければ cwd/temp を事前オープンする。レーンによって既定値が異なり、いずれも一般的なプロセス封じ込めを与えない。標準成果物は、例えば `wasmtime run --dir=<dir>` のように、外部ランタイムが与える権限を受け取る。権限の実装は `crates/almide-wasm-run/src/host_fs_scope.rs` と `component_host.rs` にある。

core モジュールレーンでは、ゲストのメモリ外に出るゲストポインタや長さは、そのゲストのトラップとなる（`Error: wasm trap: out of bounds memory
access`、終了値1）。ホストの panic になることはなく、ホストは偽造された長さに合わせたバッファを割り当てない（#4135）。コンポーネントレーンでは wasmtime の canonical ABI が同じ範囲を検査する。ホスト出力はゲストのバイト列として書き込まれ、再エンコードされない（C-399）。

正本となる参照先：

- **アーキテクチャ** — [docs/ARCHITECTURE.md](../../../../ARCHITECTURE.md)（wasm ルートを含むコンパイラパイプライン）と [docs/roadmap/active/v1-mir-architecture.md](../../../../roadmap/active/v1-mir-architecture.md)（所有権とレイアウトを MIR で一度だけ決める理由。#2761 で廃止された従来経路の設計であり、現在 MIR はネイティブ経路に使われる）。
- **ターゲット間の等価性** — [docs/contracts/](../../../../contracts)：観測可能な native ⇄ wasm の約束にはすべて、実行可能な fixture まで追跡できる名前付き契約がある。
- **所有権証明書** — [docs/roadmap/active/certificate-format-v1.md](../../../../roadmap/active/certificate-format-v1.md)。

<a id="documents-here"></a>

## このディレクトリの文書

| 文書 | 内容 |
|-----|------|
| [能力システム](../../../../wasm/capability-system.md) | コンパイル時の最小権限の強制 — 現行 |
| [JS ホストの非同期 imports](../../../../wasm/JS-HOST-ASYNC-IMPORTS.md) | `--host js` における `@extern(wasm, "js", ..., returns: promise)`：JSPI による中断、非同期になる exports、直列化、JSPI がない場合の拒否、未マークの Promise の拒否（#3353、#3371）— 現行 |

<a id="archive"></a>

## アーカイブ

v1 信頼スパインが唯一の wasm 経路になる**前**に書かれた2026-04の設計メモ（メモリモデル、WASM 3.0機能、agent container、hatch、bubblewrap、エコシステム調査）はツリーから削除された。これらは agent-container 製品の方向性と、出荷コンパイラが実装していない v0 時代のメモリモデル（2つの線形メモリ、解放なしのバンプ割り当て）を記述しており、`docs/` に残すと出荷済みの内容を説明する文書として見つかってしまったためである。git 履歴には残っている。`git log --diff-filter=D -- docs/wasm/archive/` で削除コミットを探せる。
