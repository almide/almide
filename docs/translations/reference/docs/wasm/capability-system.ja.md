<a id="capability-system"></a>

# 能力システム

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](capability-system.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](capability-system.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](capability-system.zh-CN.md)

> 非規範の原文固定読者スナップショット。変更していない原本が正本。自動同期、独立実行、再測定、安全性検証、第三者認証は行っていない。以下の「現行」「実装済み」や断定は固定原文の記述であり、今回の再評価ではない。組み込みホストは一般的なサンドボックスではなく、権限キー省略時と core／component の既定値は異なる。証明書の対象からコンポーネントを除くという原文の限定を維持する。依存関係別権限や名前付き権限プロファイルは将来案のまま。コードとコメントは原文どおり。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/wasm/capability-system.md)

コンパイル時の能力チェックとホストの権限は別の制御である。`[permissions].allow` はプログラムから推論されたカテゴリを制限し、ホストと選択した Wasm ルートは成果物が実行できる操作を決める。カテゴリチェックも import の削除も、任意のネイティブコードや子プロセスに対する一般的なサンドボックスではない。

<a id="capability-categories"></a>

## 能力カテゴリ

現行の語彙は `crates/almide-ir/src/effect.rs`（`Effect::ALL`）の14個の葉である。

| 親 | 葉 |
|--------|--------|
| `FS` | `FS.read`, `FS.write` |
| `Net` | `Net.fetch`, `Net.listen` |
| `IO` | `IO.stdin`, `IO.stdout`, `IO.stderr` |
| `Env` | `Env.read`, `Env.write` |
| — | `Proc`, `Time`, `Rand`, `Fan`, `Abort` |

親は記載された葉を与える。`IO` はファイルシステムアクセスを**与えない**。`Proc` に親はない。`All` は権限名ではない。不明な名前は `src/project.rs::allowed_effects` が拒否する。推論とマニフェストの検査には `Effect` の集合を使い、以前提案された13-bitの `u16` 表現は使わない。

カテゴリは操作ごとに割り当てられる。レジストリと標準ライブラリの `@category` 注釈が正本である。`crates/almide-ir/src/effect.rs` の `STDLIB_MODULE_EFFECTS` と `stdlib_call_effect` を参照する。現行の関数シグネチャには、ここで別の API 一覧を用意するのではなく、[生成された標準ライブラリリファレンス](../../../../stdlib/README.md)を使う。

出力と中断の組み込み機能は通常の `fn` 内にも現れるが、それでもカテゴリを持つ。`println` は `IO.stdout`、`eprintln` は `IO.stderr`、`panic` と失敗した assertion は `Abort` と `IO.stderr` の両方を使う。`err(...)` を返すことは `Abort` ではない。effect ビット、失敗可能性、カテゴリ集合は別のものだ。[effect 仕様](../../../../specs/effect-system.md)と [ADR-0026](../../../../adr/0026-effect-categories-ride-the-transparent-fn-type.md)を参照する。

<a id="declaration"></a>

## 宣言

```toml
# almide.toml
[permissions]
allow = ["FS.read", "IO.stdout"]
```

ドット付きの葉と上記4つの親が受け入れられる。`allow` キーがなければ、カテゴリ制限は適用されない（後方互換性）。明示的な `allow = []` はカテゴリを一切与えず、実装済みの `pure` プロファイルの別名ではない。時計や乱数の能力がないだけでは、決定性を証明したり、タイミングサイドチャネルを排除したりできない。

その他の権限キーには別の目的がある。

- `proc` はプロセスのコマンドを制限する。下記を参照。
- `fs` はファイルシステムルートを制限する。相対指定ならプロジェクトルートからの相対となる。空リストはルートを一切与えない。組み込みホストの既定値は下記で説明する。
- `native` は依存パッケージにネイティブコードを持ち込む許可を与える。カテゴリ名でも一般的な OS サンドボックスでもない。`src/native_grant.rs` を参照する。

<a id="inference-and-diagnostics"></a>

## 推論と診断

`EffectInferencePass` は `IrProgram.effect_map` を記録する。その `effect_flow` ソルバーは、高階関数へ渡すコールバックを含む、直接呼び出しと関数値を通じて集合を伝播する。これは実装済みであり、将来の Phase 4 ではない。コンパイラのカテゴリ入力は、操作ごとの標準ライブラリメタデータと組み込みメタデータであり、モジュール内のすべての関数を1つの effect に一律対応させるものではない。

外部の `@extern` 関数と標準ライブラリ以外の `@inline_rust` テンプレートは推論から見て不透明で、すべての葉（top）を持つ。明示的なカテゴリ上限を通過するのは、その上限が全葉を与える場合のみである。これは外部の本体が何をするかを証明するものではない。`crates/almide-codegen/src/pass_effect_inference.rs` と `effect_flow/mod.rs` を参照する。

推論された集合の確認には `almide check --effects` を使う。権限検査は推移的な要求をマニフェストと比較し、不足する葉を、利用可能なら由来の経路付きで報告する。診断は `error: capability violation in ...` で始まる。**E010 ではない**（E010 はパターンマッチの診断）。表示処理は `src/cli/check.rs` と `src/cli/mod.rs` にある。旧モック診断や固定20ラウンドの推論制限を前提にしてはならない。

<a id="process-commands"></a>

## プロセスのコマンド

`Proc` の許可はプロセス操作を許し、子プログラムを通じて他のリソースへ到達し得る。コマンドの許可リストは子の effect に対するサンドボックスではない。`cargo` やシェルを許可すると、大きな権限を与え得る。

<a id="what-the-checker-enforces-today"></a>

### 現在チェッカーが強制すること

`Proc` は独立した葉（ADR-0026）である。`[permissions] allow = ["Proc"]` はプロセス操作を許し、これを暗黙に含む親はない。`[permissions] proc` は別にコマンドを制限する（ADR-0025）。

```toml
[permissions]
allow = ["IO", "Proc"]
proc = ["git", "cargo"]
```

- `exec` はリストにあるコマンドのリテラルを指定しなければならない。`output`、`run`、`start` は `ProcessCommand` レコードを受け取り、その `cmd` も同様のリテラルでなければならない。計算で求めるコマンドや、値として渡す起動関数は、検査できなければ拒否する。現行 API は、廃止されたヘルパー名ではなく `src/cli/mod.rs::PROC_SPAWNING` と `proc_command_literal` を参照する。
- `process.kill` は子を起動しないが、プロセスへシグナルを送る。その `@signals_process` 注釈はこれをコマンド `kill` として制限する。組み込みホストも検査する（`stdlib/process.almd`、`host_process.rs`）。
- `proc` キーがなければコマンドは制限されず、`proc = []` は何も与えない。コマンド一覧なしで `Proc` を許可すると警告し、`--profile critical` では拒否する。

<a id="on-wasm-the-private-almideprocessspawn-import"></a>

### wasm 上：非公開の `almide:process/spawn` import

非公開のサブプロセスインターフェースは [`spawn.wit`](../../../../../crates/almide-wasm-run/wit/process/spawn.wit) に定義されている。操作 case には旧 ABI の slot も含まれ、すべてが現行のソースレベル関数ではない。ソース API は[プロセスリファレンス](../../../../stdlib/README.md)と `stdlib/process.almd` を参照する。

- 組み込み core ホストは、ネイティブランタイムの操作を共有する process host dispatch を通じて、対応する同期操作を提供する。
- プロセスのホスト操作が必要な標準 p1 成果物は、`almide:process/spawn::call` を import し、`cabi_realloc` を export する。この非公開 import を実装しないランタイムは、インスタンス化時に成果物を拒否する。
- 子ハンドル群（`start`、`poll`、`wait`、`stop`、`read_new_stdout`、`read_new_stderr`、`child_pid`）は、Wasm の子リソース対応（#4308）を待つ間、ネイティブ専用である。「組み込みホストはプロセス I/O を提供する」という説明の対象には入らない。
- コンポーネントルートはこの非公開サブプロセスインターフェースを宣言しない。

関数ごと・ルートごとの利用可否は [`target-availability.toml`](../../../../../proofs/target-availability.toml) に記録されており、WIT case の存在から推論するものではない。ADR-0025 が非公開ホストの決定を記録する。

<a id="wasm-imports-and-host-scoping"></a>

## Wasm imports とホストのスコープ制限

imports は、許可リストの文字列だけでなく、到達可能な操作と選択した成果物／ホストルートによって決まる。1つの import が複数の操作を提供し得る。例えば組み込み core ホストは `almide.fs_call` を通じてファイルシステム呼び出しを dispatch する。したがって import の有無はカテゴリ集合と一対一に対応する証明ではない。以前の能力別 import 表とバイナリサイズ表は設計スケッチであり、現行成果物の保証ではなかった。[Wasm 出力](../../../../wasm/WASM-OUTPUT.md)、[ホストの概要](../../../../wasm/README.md)、[操作能力レジストリ](../../../../../proofs/target-availability.toml)を参照する。

外部ランタイムは成果物が宣言する imports を提供する必要があり、未解決 imports はインスタンス化に失敗する。標準 WASI 成果物は、事前オープンしたディレクトリなど、ランタイムが与える権限を受け取る。これにより `FS.read` だけの許可が、ランタイムの読み取り専用 descriptor ポリシーと同等になるわけではない。

組み込みホストでは `[permissions] fs` が実装されている。

- core モジュールのファイルシステム操作は、設定ルート配下で検査・実行される。descriptor 相対の封じ込めは64-bit LinuxとmacOSに実装済みで、他のプラットフォームは検査／使用の競合の影響が残るパスベースの検査を使う（`fs_beneath_sys.rs`）。`fs` キーがなければ、この制御によるパス制限はない。
- コンポーネントホストは設定ルートを読み書き可能として事前オープンする。キーがなければ作業ディレクトリと一時ディレクトリを事前オープンし、`/` は開かない。

実装は `crates/almide-wasm-run/src/host_fs_scope.rs` と `component_host.rs` にある。これらの制御は、組み込みホストを任意の信頼できないコードに対する一般的な隔離環境へ変えるものではない。特に、プロセス権限と外部ネイティブコードは別に検討する必要がある。

<a id="per-dependency-restriction-future"></a>

## 依存関係ごとの制限（将来）

```toml
[dependencies.json_parser]
version = "1.0"
allow = []              # pure only -- no I/O, no net, nothing

[dependencies.http_client]
version = "2.0"
allow = ["Net.fetch"]   # can fetch, nothing else
deny = ["Proc"]         # explicitly deny even if parent allows

[dependencies.code_formatter]
version = "0.5"
allow = ["FS.read"]     # read-only access to source files
```

提案された動作：推移的な呼び出しグラフが、依存関係ごとの許可を超える依存関係を拒否する。この構文と強制処理は、現行のトップレベル `[permissions].allow` 検査では実装されていない。

この提案は信頼の階層を作るものとなる。トップレベルプロジェクトが自身の能力を宣言し、各依存関係はその部分集合に制限される。現在の封じ込め保証ではない。

<a id="proposed-profile-names-not-implemented"></a>

## 提案されたプロファイル名（未実装）

以前の設計では、一般的な権限集合を `pure`、`readonly-agent`、`readwrite-agent`、`network-agent`、`full-agent` と名付けた。`[permissions] profile` は実装済みのマニフェストキーではないため、制限の要求に使ってはならない。CLI の `--profile critical` は別機能であり、これらの別名の1つではない。

代わりに明示的な `allow` リストを書く。例えば：

```toml
[permissions]
allow = ["FS.read", "IO.stdin", "IO.stdout"]
fs = ["workspace"]
```

これは列挙したカテゴリとファイルシステムルートを与える。すべての観測可能な動作が決定的であることや、すべてのホストリソースが封じ込められることの証明ではない。別の操作が必要なら、明示的にリストを拡張する。
