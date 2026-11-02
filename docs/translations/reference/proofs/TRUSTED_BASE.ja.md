<a id="trusted-base-ledger-config-management--tier-1-layer-7"></a>

# 信頼基盤台帳（構成管理 — tier-1 第7層）

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](TRUSTED_BASE.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](TRUSTED_BASE.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](TRUSTED_BASE.zh-CN.md)

> 固定原文の非規範的な参照資料。正本の台帳は変更していない。日付・測定・証明・CI の主張は原文の履歴であり、新たに再現した検証、外部認証、普遍的なサンドボックスやメモリ安全性の保証ではない。新旧の進捗層（CI 版、高階処理・制御フローの受入れ、println・バイト列結合の範囲）が併記されるため、整合したものに書き換えず保持した。モデル上の保証・成果物バイト列の主張・残る信頼前提はそれぞれの範囲とともに読む。自動同期しない。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/proofs/TRUSTED_BASE.md)

航空級の規律：**何を信頼するかを正確に名指すことが、残りを「証明済み」と呼ぶための条件である。** レビュー担当者はこのページで境界を知る。receipt の `C-PROVEN` 主張に対する、率直な裏側の説明である。

<a id="toolchain-pin"></a>

## ツールチェーンの固定

| 要素 | バージョン | 役割 |
|---|---|---|
| Rocq / Coq | **9.1.1** | カーネルと `coqchk` による独立再検査（正本、ローカルのソースビルド） |
| OCaml | **5.4.1** | 抽出先（CertiCoq まで） |

すべての主張を再現する：`make verify-trust`（または `proofs/check.sh` + `proofs/gate.sh`）。

**CI の異なるバージョンについての率直な注記。** `Trust Spine` GitHub Actions workflow は、opam の Rocq **9.3** で spine 全体を再導出する。rocq-core は9.3.0に固定（`ROCQ_CORE_VERSION`、キャッシュキーも同じ）、rocq-stdlib は9.2.0である（opam に9.1.1はなく、上記正本の固定版はローカルのソースビルド）。証明は **9.1.1、9.2、9.3でカーネル検査済みかつ追加公理なし**。単一バージョン成果物ではなく、複数バージョンでの再導出であり、不足でなく強みである。9.3は `is` を予約するため、証明では束縛しない（#3368）。Rocq 9.2以降は `rocq` ドライバーのみを配布するため、CI は `rocq compile` / `rocq check` の薄いラッパーとして `coqc`/`coqchk` を提供する（後者は Rocq Proof Checker そのもので、独立した De Bruijn 再検査は実際に行われる）。

<a id="build-and-run-toolchain-4010"></a>

## ビルド・実行ツールチェーン（#4010）

下記の証明が扱うのは Almide のチェッカーとモデルである。実際に動くバイト列は、ここでの証明がカバーしないツールが生成・実行するため、それぞれ名前とバージョンを挙げて信頼する。`scripts/check-toolchain-pins.sh` はこの一覧を workflow と `Cargo.toml` が実際に使うものと一致させる（機械可読の写しは `scripts/toolchain-pins.txt`）。使っているのに未掲載の版も、掲載しているのにもう使わない版も失敗となる。

| 要素 | バージョン | 動作 | 独立に検査するもの |
|---|---|---|---|
| rustc | **1.97.0**（`RUST_TOOLCHAIN`） | コンパイラをビルドし、ネイティブ経路が生成する Rust をコンパイル | これを証明するものはない。契約コーパス上でネイティブ経路を wasm 経路とバイト単位で比較する |
| LLVM | その rustc 内の版（1.97では22.1系。`rustc -vV` が表示） | ネイティブコード生成 | rustc と同じ |
| rustc（Ferrocene） | **1.95.0** | 記録済み Ferrocene upstream pin（#573）の通常版 rustc で run-manifest コーパスの生成 Rust をビルド | 互換性証拠のみ。この経路は資格認定済み Ferrocene バイナリを使わない |
| rustc（stable、perf） | **1.99.1**（`RUST_STABLE_TOOLCHAIN`） | `perf-ratchet` と `perf-profile-arm` の性能比で手書き Rust 側をビルド。出荷物はこれでビルドしない | 性能比ゲート自身。固定により基準値を静止させる |
| rustc nightly | **nightly-2026-08-01**、**nightly-2025-06-01** | カバレッジ計装（分岐カバレッジ、MC/DC）と `almide-str` の Miri。出荷物はこれらでビルドしない | — |
| wasmtime（組み込みクレート） | **50.0.1** | wasm の `almide run` / `almide test`、fan worker | 下記の通常版 CLI 実行 |
| wasmtime（CI の通常版 CLI） | **50.0.1** | CI ゲートでビルド済み `.wasm` 成果物を実行。全箇所で1つの版 | ランタイムの `tests/wasm_vm_parity_test.rs`（REQ-VM-9）が `almide-wasm-vm` をこれと比較する |
| almide-wasm-vm | このツリー | Critical プロファイルの機能範囲向けの、2つ目の独立 wasm インタープリター（標準ライブラリのみ、`unsafe` なし） | REQ-VM-9：5つの WASI import の範囲内にある全 `spec/wasm_cross` fixture が通常ランタイムと同じ実行結果となり、`--profile critical` を通る場合はネイティブとも一致 |
| OS、libc、CPU | CI ランナーとユーザーのもの | すべて | 受け入れるが検査しない |

**CI が取得するもの（セキュリティ行16）。** workflow や CI スクリプトがダウンロードする全バイナリ・アーカイブを、`.github/pinned-downloads.txt` 記載の sha256 で確認する（`scripts/fetch-pinned.sh`）。wasmtime、cargo-nextest、wasm-pack、elan、Lean ツールチェーン、Node が対象。自プロジェクトの `latest` Almide リリースは、代わりに release attestation を検証する。すべての `git clone` は40桁16進コミットを checkout（codopsy）、opam リポジトリは各ソースアーカイブのチェックサムを opam ファイルが持つコミットに固定し、全サードパーティー action も取得内容とともにその一覧に宣言する。`scripts/check-action-pins.sh` はそれ以外を拒否する。以下4経路はここで固定するのでなく信頼する。
- **rustup。** `dtolnay/rust-toolchain` が正確なツールチェーンを指定して実行する。rustup は static.rust-lang.org が配信する、そのリリースの channel manifest に記された sha256 で各コンポーネントを確認する。rustc 自体と同じく Rust のリリース基盤を信頼する。
- **apt と Homebrew のパッケージ**（binaryen、musl-tools、opam、wabt、valgrind、Vulkan ドライバー）。ここに記すダイジェストでなく、ディストリビューションの署名付きパッケージ索引で確認する。
- **`actions/cache` 項目。** 書き込むのはこのリポジトリ自身の実行のみで、PR のキャッシュは自身の ref に限定する。キャッシュされたリリース tarball は使用前に `fetch-pinned.sh` が再ハッシュする。展開済みツリー（`~/.elan` の Lean ツールチェーン、固定コミットをキーにする codopsy バイナリ、opam root）は再ハッシュしない。
- **`# supply: head-tracked` と記した clone。** 動く head 自体が測定対象。下流 canary は各プロジェクトのデフォルトブランチ HEAD をテストし、dispatch 専用、token は読み取り専用、経路は秘密を持たない。文法乖離チェックは `almide-grammar` をデータとして読み、ビルド・実行しない。

**2つのエンジン。** wasm 実行は wasmtime だけには依存しない。`almide-wasm-vm` は wasmtime とコードを共有せず、REQ-VM-9 が `spec/wasm_cross` 全コーパスで通常ランタイムとの差分を取る。範囲は Critical プロファイルの機能である。5つの WASI 呼び出し（fs、env、args）より多くを import する成果物はロード時に拒否するため、そのような成果物では wasmtime が唯一のエンジンとなる。

**組み込みホストはサンドボックスではない。** core-module 経路のファイルシステム操作は `[permissions] fs` の許可に従うが、キーがなければホストのファイルシステムへのアクセスを維持する。p3 component 経路（`run_component`、`crates/almide-wasm-run/src/component_host.rs`）は許可されたルートを preopen し、キーがなければ現在の作業ディレクトリと一時ディレクトリを preopen し、環境を継承する。Wasm はゲストのメモリアクセスを制限し、ホストはゲストがホストに確保させる量を制限する。1回の実行につき全インスタンス共通の線形メモリ予算を1つ設け、ホストが読む前に全ゲスト span をメモリと照合する（#4135）。ゲストがホストに依頼できる行為は制限しない。`docs/wasm/README.md`（「What the embedded host grants」）に経路別の表がある。ファイルシステムの範囲制限（#4022）は実装済みだが、全ホスト能力に対する一般的なサンドボックスではない。

**未固定のステップ（負債）。** バージョン未指定の stable ステップとして残るのは `scripts/toolchain-pins.txt` 記載の `fuzz-nightly.yml:coverage-ratchet`。固定するには測定基準値を取り直し、台帳項目を削除する必要がある。2つの perf ジョブは `RUST_STABLE_TOOLCHAIN` を使う。上の表を参照。

**ツールチェーン更新時。** `RUST_TOOLCHAIN` や wasmtime の版を上げると信頼する要素が変わるため、導入前に次を行う。

1. 2版の間に修正または導入された、upstream の `I-unsound` ラベルの issue（rust-lang/rust）と wasmtime のセキュリティアドバイザリーを読む。生成 Rust または出力 wasm が到達し得るものを PR に記す。生成 Rust は `unsafe` を `runtime/rs` とコンパイラ出力の2つの prelude（region arena と SIGPIPE 設定、#4007）に限定しているため、safe Rust 検査の健全性バグが最も重要なケースである。
2. 新版でクロスターゲットのコーパスと REQ-VM-9 を実行する。差はノイズでなく発見事項である。
3. 同じ PR で `scripts/toolchain-pins.txt` とこの表を更新する。さもなければゲートが PR を拒否する。

**ユーザーの rustc（#4110）。** CI の固定版はユーザー端末のツールチェーンについて何も意味せず、標準ライブラリのセキュリティ修正は、そのツールチェーンに含まれる場合だけネイティブ Almide バイナリへ届く（CVE-2024-24576：Windows で `.bat` / `.cmd` を実行する `Command` は1.77.2より前、引数を未エスケープのまま cmd.exe に渡し、`process.exec` 経由のシェルインジェクションを再び許し得た）。このため全ネイティブビルド（`almide run` / `build` / `test` / `bench`、cdylib、install 経路）は rustc または cargo の実行前に `rustc --version` を読み、rustc 1.97.0未満を拒否する。唯一の定義箇所は `src/cli/rustc_floor.rs` の `MIN_RUSTC`、すなわち max(1.77.2, 生成コードを検証する `RUST_TOOLCHAIN`)。Almide が依存する標準ライブラリ修正が入れば引き上げ、定数のコメントに各更新がカバーするアドバイザリーを列挙する。`tests/rustc_floor_test.rs` は1.77.1と下限より1パッチ下を報告する偽 rustc（拒否）および下限（ビルド・実行）を動かし、この段落と docs/specs/cli.md を定数と照合する。対象外は、出力 Rust を自身の rustc で直接コンパイルする Ferrocene 経路と、再検査なしで再利用するビルドキャッシュ済みバイナリである。

<a id="the-irreducible-base-cannot-be-discharged-by-proof--消えない底"></a>

## 還元できない基盤（証明で取り除けない「消えない底」）

以下4つは必然的に信頼する。それ以外はすべて、これらを前提に証明する。

1. **Coq/Rocq カーネル。** 数十年の敵対的な精査。`coqchk` が各 `.vo` を独立に再検査する（De Bruijn 基準）。新しい論理体系には蓄積された精査がないため、発明せず借りる。
2. **OCaml 抽出と OCaml コンパイラ：ゲート判定の信頼の根から除外済み（brick 6b、2026-07-09）。** 証明済みチェッカーは高速経路として引き続き OCaml（`ocamlopt`）へ抽出するが、`gate.sh` の各行の判定と全コーパスの証拠集合は Rocq カーネル自身が独立に再検証する。生成した assertion ファイルに証拠バイト列を原文どおり埋め込み、`vm_compute` がカーネル検査済み論理内で証明済みチェッカーを評価する（gate.sh の `kernel_verify`、corpus-wall.sh の batch oracle。27k オブジェクト全コーパスで約4分と測定）。バイナリとカーネルの差異はビルドを失敗させ、改ざん演習（破損した証拠と模擬の異なる判定を両方検出）を毎ビルドで行う。そのため Thompson の穴はもはや判定へ届かず、カーネル（項目1）が担う。残る前提も明記する。抽出パイプラインは便利なバイナリの高速経路としての正しさについてのみ信頼する（相互検査あり）。`vm_compute` のバイトコード評価器は従来どおりカーネル自身の trusted computing base の一部。upstream には検証済み抽出（MetaRocq、Rocq 9.1）と CertiRocq（CertiCoq → Rocq 9.1 の移植、2026）がすでに存在する。ローカル（ソースビルド9.1.1、opam なし）と CI（opam 9.2）のツールチェーンが揃った後の将来の高速経路ラチェットとして記録しており、どちらも判定には不要。
3. **ハードウェア。** CPU が機械語を忠実に実行すること。
4. **ALS の妥当性**：形式意味論が意図した意味を捉えていること。これは経験的（interp + dojo + 利用）に検査する唯一の項目で、証明しない。

<a id="axiom-ledger-the-print-assumptions--standard-gate"></a>

## 公理台帳（「Print Assumptions ⊆ standard」ゲート）

すべての定理が依存するのは**カーネルだけ**。全定理について `Print Assumptions` が *Closed under the global context* を報告する（`Admitted` も追加公理もなし）。`proofs/check.sh` で検証する。表そのものも**主張の乖離ゲートで検査する**（`check.sh` の claim-drift ゲート、#34 / 指標⑤）。以下に名指す各定理が、カーネル再検査器（`coqchk`）が実際に検証した定数であることを機械的に確認し、台帳の公開主張が証明範囲を越えないようにする（捏造行はゲートで失敗する）。この表は spine の代表的な抜粋である。

| 定理 | ファイル | 仮定 |
|---|---|---|
| `check_sound` | OwnershipChecker.v | グローバルコンテキストの下で閉じている |
| `check_all_sound` | OwnershipChecker.v | グローバルコンテキストの下で閉じている |
| `check_cert_sound` | OwnershipChecker.v | グローバルコンテキストの下で閉じている |
| `check_reuse_sound` | OwnershipChecker.v | グローバルコンテキストの下で閉じている |
| `check_clc_unroll_sound` | OwnershipChecker.v | グローバルコンテキストの下で閉じている |
| `eager_copy_refines_safety` | ALS.v | グローバルコンテキストの下で閉じている |
| `mrun_tracks_exec` | RuntimeModel.v | グローバルコンテキストの下で閉じている |
| `alloc_not_live` | FreeList.v | グローバルコンテキストの下で閉じている |
| `region_window_reuse_safe` | FreeList.v | グローバルコンテキストの下で閉じている |
| `region_reset_leaves_no_free_into_the_region` | FreeList.v | グローバルコンテキストの下で閉じている |
| `pinned_stays_immortal_forever` | FreeList.v | グローバルコンテキストの下で閉じている |
| `p_region_reset_preserves_PINV` | FreeList.v | グローバルコンテキストの下で閉じている |
| `reuse_restores_rc_1` | FreeListRc.v | グローバルコンテキストの下で閉じている |
| `reuse_hands_back_a_zero_count_block` | FreeListRc.v | グローバルコンテキストの下で閉じている |
| `double_release_traps` | FreeListRc.v | グローバルコンテキストの下で閉じている |
| `r_region_reset_preserves_RINV` | FreeListRc.v | グローバルコンテキストの下で閉じている |
| `rc_dec_prog_realizes_rt_dec` | WasmRcDec.v | グローバルコンテキストの下で閉じている |
| `rc_inc_bytes_encode_the_instruction_tree` | WasmEncode.v | グローバルコンテキストの下で閉じている |
| `rc_inc_bytes_execute_to_rt_inc` | WasmExec.v | グローバルコンテキストの下で閉じている |
| `rc_dec_bytes_trap_on_zero` | WasmExec.v | グローバルコンテキストの下で閉じている |
| `rc_dec_bytes_frees_when_one` | WasmExec.v | グローバルコンテキストの下で閉じている |
| `make_unique_yields_unique` | CowSafety.v | グローバルコンテキストの下で閉じている |
| `check_fill_sound` | CallModes.v | グローバルコンテキストの下で閉じている |
| `check_modes_cert_sound` | CallModes.v | グローバルコンテキストの下で閉じている |
| `check_bc_unroll_sound` | OwnershipChecker.v | グローバルコンテキストの下で閉じている |
| `check_xc_unroll_sound` | OwnershipChecker.v | グローバルコンテキストの下で閉じている |
| `check_line_abort_sound` | OwnershipChecker.v | グローバルコンテキストの下で閉じている |
| `check_xc_abort_sound` | OwnershipChecker.v | グローバルコンテキストの下で閉じている |
| `check_xc_no_resurrection` | OwnershipChecker.v | グローバルコンテキストの下で閉じている |
| `check_line_prefix_safe` | OwnershipChecker.v | グローバルコンテキストの下で閉じている |
| `merge_subset_sound` | Subset.v | グローバルコンテキストの下で閉じている |
| `subset_check_fast_sound` | Subset.v | グローバルコンテキストの下で閉じている |
| `rc_inc_bytes_isa_effect` | WasmDecode.v | グローバルコンテキストの下で閉じている |
| `rc_dec_bytes_isa_traps` | WasmDecode.v | グローバルコンテキストの下で閉じている |
| `rc_dec_bytes_isa_frees` | WasmDecode.v | グローバルコンテキストの下で閉じている |
| `memory_safe_iff_clauses` | MemorySafe.v | グローバルコンテキストの下で閉じている |
| `check_trace_sound` | MemorySafe.v | グローバルコンテキストの下で閉じている |
| `check_trace_complete` | MemorySafe.v | グローバルコンテキストの下で閉じている |
| `rc_check_gives_temporal` | MemorySafe.v | グローバルコンテキストの下で閉じている |
| `unguarded_rc_check_misses_resurrection` | MemorySafe.v | グローバルコンテキストの下で閉じている |
| `unsigned_ge_excludes` | SpatialCert.v | グローバルコンテキストの下で閉じている |
| `run_sound` | SpatialCert.v | グローバルコンテキストの下で閉じている |
| `accepts_spatial` | SpatialCert.v | グローバルコンテキストの下で閉じている |
| `unchecked_run_not_spatial` | SpatialCert.v | グローバルコンテキストの下で閉じている |
| `check_bounds_cert_sound` | BoundsCert.v | グローバルコンテキストの下で閉じている |
| `typed_core_memory_safe` | CoreSafe.v | グローバルコンテキストの下で閉じている |
| `unchecked_index_not_safe` | CoreSafe.v | グローバルコンテキストの下で閉じている |
| `certified_spatial_temporal` | Certified.v | グローバルコンテキストの下で閉じている |
| `certified_with_bounds_cert` | Certified.v | グローバルコンテキストの下で閉じている |
| `certified_memory_safe` | Certified.v | グローバルコンテキストの下で閉じている |
| `shared_list_survives_mutation` | CoreSafe.v | グローバルコンテキストの下で閉じている |
| `missing_inc_is_use_after_free` | CoreSafe.v | グローバルコンテキストの下で閉じている |
| `box_frees_and_keeps_content` | CoreSafe.v | グローバルコンテキストの下で閉じている |
| `box_store_without_share_is_use_after_free` | CoreSafe.v | グローバルコンテキストの下で閉じている |
| `site_window` | InitCert.v | グローバルコンテキストの下で閉じている |
| `sited_init` | InitCert.v | グローバルコンテキストの下で閉じている |
| `sited_alloc_ok` | InitCert.v | グローバルコンテキストの下で閉じている |
| `certified_sited_memory_safe` | InitCert.v | グローバルコンテキストの下で閉じている |
| `reorder_memory_safe` | InitCert.v | グローバルコンテキストの下で閉じている |
| `check_init_cert_sound` | InitCert.v | グローバルコンテキストの下で閉じている |
| `fill_window` | InitCert.v | グローバルコンテキストの下で閉じている |
| `rsite_window` | FieldCert.v | グローバルコンテキストの下で閉じている |
| `ftrace_safe` | FieldCert.v | グローバルコンテキストの下で閉じている |
| `fields_memory_safe` | FieldCert.v | グローバルコンテキストの下で閉じている |
| `check_fields_cert_sound` | FieldCert.v | グローバルコンテキストの下で閉じている |

<a id="known-limitations-what-is-not-yet-proven--recorded-not-hidden"></a>

## 既知の制限（まだ証明されていないこと — 隠さず記録）

**ここでの「メモリ安全」の意味（#4013）。** 定義は `MemorySafe.v` に一本化されている。ヒープトレース（alloc / store / load / inc / dec、それぞれがブロックを指定する）上で、すべての load と store が生きている自身のブロック内にあること（SPATIAL）、解放済みブロックに何も触れず、割り当てが生きているブロックを払い出さないこと（TEMPORAL）、読み取る各バイトが割り当て後に書かれていること（INITIALIZATION）、各アクセスがそのブロックの表現を使うこと（TYPE）を意味する。決定可能なトレースチェッカーは、この定義に対して健全かつ完全である（`check_trace_sound` / `check_trace_complete`）。4 条項のうち、spine の従来の定理が届くのは TEMPORAL だけである。`rc_check_gives_temporal` は、ガード付き RC チェックとアロケータの freshness（`FreeList.alloc_not_live`）からこれを導く。SPATIAL、INITIALIZATION、TYPE に対応するものは他のファイルにはない。ただし `SpatialCert.v`（#4005）は、固定長リストブロックの要素読み書きについて、静的な境界チェック証明書のチェッカーが SPATIAL（および TYPE）に対して健全であると証明する。`accepts_spatial` により、受理された証明書のあらゆる実行は、すべてのループ回数、分岐、再代入された i64 インデックスに対して `Spatial` となる。emitter の 1 回の符号なし比較が負のインデックスを除外する（`unsigned_ge_excludes`）。`BoundsCert.v` は証明書のバイトパーサーを証明内に入れ、`check_bounds_cert` を抽出する（driver mode `bounds`、`check_bounds_cert_sound`）。structural wasm 経路は、すべてのインデックス演算子アクセスについて、ビルドごとにこれを `bounds` witness として出力する（crates/almide-wasm/src/witness_bounds.rs）。リストの各 `xs[i]` の読み取りと格納、Bytes の各 `b[i]` の読み取りと格納が対象で、#3345 の fact 候補はその区間内で、他のすべてのアクセスはそれぞれ専用の新しい slot のもとで記録する。チェックは比較を出力する箇所に、アクセスは load または store を出力する箇所に記録する。almide-verify と proofs/structural-wall.sh が抽出チェッカーとカーネルで判定し、scripts/witness-reinject.sh bounds はチェックが失われる 3 つのバグを emitter に再注入し、各 witness が拒否されることを要求する。certified corpus 上のカバレッジ（trust-spine run 40085282713、2026-10-24）：954/954 の bounds witness を受理。93 個の空でない witness に、生成された 434 チェックに対する 365 読み取りと 82 書き込みを記録した。この全アクセス記録以前は、空でない witness は 5 個だった。#4011 (d) 以降、各 bounds / init event は出荷モジュールへの ANCHOR も持つ。`almide-anchor-check`（crates/almide-anchor-check。wasmparser の decoder と validator を使用し、コンパイラコードは使わない。それ自体は信頼対象で、18 個の受理／拒否用単体モジュールと 3 つのバイトレベル訓練 `scripts/witness-reinject.sh anchors` がある）は、すべての structural-wall 成果物で、位置を特定した各 event が指定した命令であることを要求する。チェックは中断する `if` を開く符号なし比較、アクセスは load または store、site は割り当てサイズと各 store の幅・offset・アドレス local（外へ抜ける分岐も未記録 store もない）、fill は正確な countdown である。sec-rows-2-3-4（2026-10-27）以降は比較のオペランドも検査する（crates/almide-anchor-check/src/operands.rs）。位置を特定した各アクセスを、出荷命令上でその slot のチェックに対応させる。同じ index、アドレス指定するブロックの自身の slot サイズでの `len / S`、1 つの slot 内、チェックが支配すること、その間に index や list が変わらないことを求める（書き込みはリスト自身の copy-on-write judge だけ。その呼び出し位置は anchor ファイルの `judges` 行にある。リストを渡された他の呼び出しはなく、count word への store もない）。巻き上げた count と payload pointer も含めて検査し、kills が表していた内容も検査する。995-fixture corpus の位置特定済みアクセス 578 件中 578 件と、`scripts/witness-reinject.sh anchors` の 3 つの operand 訓練が対象である。信頼する事項：位置を特定した judge 呼び出しが copy-on-write judge（引数と同じ count のブロックを返す）であること、ブロックを渡された呼び出しだけがその場でブロックを縮めること、表現、anchor ファイルの `alloc` index が `$alloc` であること、そしてインデックス演算子と下記の文字列読み取り（#4281）以外の、witness が記録しないすべてのヒープアクセス。

またソースレベルでは、`CoreSafe.v`（#4014）が、Almide の型付き core（int、bool、値意味論を持つ int リスト、`let`、代入、要素 store、`if`、`while`、境界チェック付きインデックス、`if` 式、再帰的な 1 引数／2 引数関数。2 つのパラメーターは異なり、呼び出しが戻る際に各引数を順に返却する）に、ヒープトレースを出力する big-step 意味論を与える。参照カウントを伴い、共有読み取りは increment、消費されたオペランド・上書きされた変数・戻る呼び出しのパラメーターは decrement、count がゼロになったブロックは解放される。box（1 slot が任意の値を持つブロック）の解放は、その slot が保持していたものを返却する。そして `typed_core_memory_safe` を証明する。関数本体が正しく型付けされた任意の関数表について、正しく型付けされた core プログラムの終了する実行またはチェックで中断される実行は、4 条項すべてについて `MemorySafe` である。これは core に限られる（クロージャ、レコード、文字列、多引数呼び出しはない。束縛はスコープ末尾ではなく上書き時に解放されるため、残りはリークする。終了・中断する実行のみ）。コンパイラの lowering に結び付けるものはない。`Certified.v` は 2 つのビルドごとの証明書を 1 つのトレース上で合成する。`certified_with_bounds_cert` は、すべてのブロックの stream をガード付き RC チェックが受理し、fresh なアロケータ、layout に忠実な割り当て、トレースのアクセスが受理済み bounds 証明書の実行であることから、`Spatial`、`Temporal`、`WellTyped` を与える。`certified_memory_safe` は、さらに constructor 規律（各割り当ての直後に、その全範囲を初期化する store が続く）という仮定から INITIALIZATION を加える。

`InitCert.v`（#4013）は、この仮定と layout の仮定（`alloc_ok`）をビルドごとの証明書に置き換える。construction site（割り当ての header、slot 幅、slot 数、representation、続いて格納する slot）は、すべての slot が格納され、末尾を超える格納がない場合に受理される。`site_window` は受理済み site の window が MemorySafe で、ブロックの全範囲が初期化済みとなることを証明する。`sited_init` と `sited_alloc_ok` は、受理済み site で構成するトレースの全割り当てについて INITIALIZATION と layout を与える。`certified_sited_memory_safe` は RC 証明書、bounds 証明書、site を `MemorySafe` の 4 条項すべてへ合成し、初期化や layout の仮定を残さない。`reorder_memory_safe` は異なるブロックの隣接 event を交換しても MemorySafe が不変であると証明するため、site の window はその要素自身の event と交錯できる。`check_init_cert_sound` はバイトパーサーを証明に入れる（driver mode `init`）。`fill_window` は LOOP FILL、すなわち実行時 count からサイズを決め、countdown loop で埋めるブロックへ拡張する。ループが、ブロックのサイズ算出に使った同じ local を減算し、cursor が 1 slot ずつ進むときに受理し、そのときあらゆる count に対して MemorySafe かつ初期化を行う。structural 経路は各リストリテラルと各 range リスト（loop fill）について、ビルドごとに `init` witness を出力する（crates/almide-wasm/src/witness_init.rs）。proofs/structural-wall.sh は抽出チェッカー、カーネル、almide-verify で判定する（certified corpus、trust-spine run 40094443633：554 個の空でない witness に 2973 site、すべて受理）。他の割り当て site（レコード、variant、文字列、ランタイムの routine）はまだ何も記録しない。そのため `sited`（全割り当てが記録済み site）が実行について成立するのは、そのすべての割り当てがリストリテラルまたは range のときだけである。信頼する事項：記録規律、および site のブロックが最後の store まで private のままであること。まだトレースを持つ証明書はないので、この定義が目指す定理、出荷成果物 `a` に対する `K(c, a) accepts ⟹ MemorySafe a` は未解決である（#4005、#4006、#1696）。証明されるまでは、「メモリ安全」はトレースモデルについての主張であり、バイト列についてではない。

**レコード、タプル、variant のフィールド（#4013 TYPE 条項、#4279）。** フィールドはチェックなしの定数 offset で読み書きされる。そのため、アクセスが仮定する layout でブロックが構築されたとき、かつそのときに限り、境界内で初期化済みとなる。`FieldCert.v` は layout で型付けした construction site と field access を、第 3 のビルドごとの証明書 `fields` として追加する。layout table（kind ごと、すなわちレコード・タプル・variant の 1 case ごとの payload サイズと各 field の offset／幅。variant case の field 0 は tag word）、自身の kind のすべての field を格納しなければならない site、宣言された field を指定しなければならない access、すべての case の field 0 でなければならない tag read から成る。`rsite_window` は受理された site の window が MemorySafe で、その layout のブロックの全 field が初期化済みとなることを証明する。`ftrace_safe` は、記録した kind のブロックの割り当てが受理済み site で、そこへの load/store がその kind の宣言済み field を指定するトレースにおいて、TEMPORAL を仮定すると、そうしたブロックの全 event が MemorySafe であると証明する。`fields_memory_safe` はこれを、それ以外の全ブロックに対する他の証明書の条項と合成して `MemorySafe` にする。`check_fields_cert_sound` はバイトパーサーを証明に入れる（driver mode `fields`）。structural 経路は、レコード・タプル・variant のリテラルと constructor、member／位置による読み取り、dying read の slot take、pattern test と bind、spread 上書きについて、ビルドごとに witness を出す（crates/almide-wasm/src/witness_fields.rs）。`almide-anchor-check` は、同じ layout table に基づき、位置を特定した全 event を出荷バイト列と対応付ける。割り当ての payload サイズ、各 store と access の幅／offset、variant site における自身の case の tag store、および各 variant case field の読み取りをガードする、自身のアドレスの tag test を検査する（条件が `tag == C` を含意する外側の `if`。アドレスは同じ変数、または同じ field を再ロードしたもの。その間にメモリを書き得るものはない。呼び出しを許すのは callee 本体が何も書かないときだけで、モジュール全体の不動点で求める）。1000-fixture certified corpus（GCE、ba31c7646）では、1000/1000 の `fields` witness が 3 つすべての判定で受理され、全成果物で FIELDS OK。3998 site／8636 store、5390 field access（うち 552 variant case field を tag test に対応付け）、614 tag read。信頼する事項：ブロックの静的型がその layout であること（representation。bounds と init witness でもすでに信頼している。IR は cast-free で FieldAssign は拒否される）、および #4279 が追加する 1 行（所有者の裁定、2026-10-30）：

> **信頼する事項（#4279）：construction window の後に variant ブロックの tag word を書き換えるものはない。**
> 範囲：variant case-field の読み取りが依存する case に限る。両端はバイト列で検査する。construction site は tag store を field 0 として記録し、`almide-anchor-check` はその store が site 自身の case 定数を格納することを要求する。記録済み case-field read はすべて、その case に対する自身のアドレスの tag test の下になければならない（`operands.rs` の `case_guarded`）。信頼するのはその間だけ、すなわち途中で tag word に store する命令がないことである。ヒープモデル（`MemorySafe.v`）は値を保持しないためである。tag read 自体にこのような行は不要。field 0 はすべての case で tag word だからである（`ITag`）。訓練 `case-field-unguarded`（scripts/witness-reinject.sh anchors）は case field の read を test より前へ移動し、ANCHORS REJECT となることを要求する。

これらのブロックを扱うランタイム自身の routine（typed drop、copy、等値性、比較、display 本体）はまだ何も記録しない（#4278）。引数なしの case は、割り当てモデルの外にある静的 pool ブロックである。

**文字列のバイト（#4281）。** structural 経路の文字列 API がユーザーコード内で読む各バイト（`get`、`first` / `last`、`capitalize`、`to_codepoint`、`to_list`、`reverse`、`lines`、`pad_*`、`starts_with` / `ends_with`、`strip_prefix` / `strip_suffix`、`io.read_line`）を、1 バイトの slot として、その API がすでに出力している test とともに `bounds` witness に記録する。記録のために test を追加することはなく、1030 件の corpus／perf 成果物の出荷バイト列は merge base と同じか小さい（1021 件は同一、4 件は合計 27 B 縮小、5 件は local index だけが異なる）。新しい event も Rocq 条項も追加しない。各 test は `BoundsCert.v` の `c` または `e` であり、読み取りをカバーする根拠は `almide-anchor-check`（operands.rs の `covers`）がバイト列上で検査する。fact ごとに 1 つのルールを持つ：
- API 自身の `idx >=u len(s)` / `len(s) == 0` が `if` を開き、読み取り（`s + idx` / byte 0）がその `else` 内にある。`c` は `if` に anchor される。
- byte loop の終了 test `k >=u n; br_if` は `n = len(s)` に対して、または外側の test がそれ以下に制限する `n` に対して行う（`n >u len(s)` または `len(s) != n` の `else` 内に loop がある）。すなわち `k < n <= len(s)`。
- 同じ 2 つの fact のもとで `(len(s) - n) + k` にある suffix read：`len(s) - n + k < len(s)`。
- 終了 test と `k` を正にする test（`k >u x` の then 分岐）のもとで、cursor の 1 バイト前 `s + k - 1`：`0 <= k - 1 < len(s)`。
- 減少方向に進む cursor、終了条件 `p == 0; br_if`（loop に関する帰納法）。入口では `p = len(s)`、または `len(s) == 0` の `else` 内では `len(s) - 1`。loop 内の `p` への全書き込みは、終了 test 後の `p - 1`。読み取りは `s + p` または `s + p - 1` で、2 つの offset の和は少なくとも 1。
- 新しいブロック自身のサイズ：`i32.const N; call $alloc`（呼び出しに anchor された `c`）。読み取りはその N バイト内にある（生きているブロックの割り当てサイズは変わらない。生存性は所有権 witness が担う）。
各ルールは、すべての `len` 読み取りからアクセスまで `s` の count が不変であることを要求する（既存の count ルール）。loop がガードする read は、その loop 内になければならない。1001-fixture certified corpus（GCE、e23e4ea28）では、1001/1001 の bounds witness が 3 つすべての判定で受理され、1083 個の loop 終了 test（従来 429 個）と 850 個の生成チェックを記録した。全成果物で ANCHORS OK、1899 件の位置特定済みアクセスを guard に対応付けた（従来 1018 件）。訓練 `string-test-one-byte-loose`、`string-loop-exit-loosened`、`string-prefix-length-test-loosened`（`scripts/witness-reinject.sh
anchors`）はそれぞれ ANCHORS REJECT となり、`crates/almide-anchor-check/tests/string_bytes.rs` には受理／拒否の 33 モジュールがある。未記録なのは文字列 routine 自身の本体、#4278 の helper ごとの証明書に残されているランタイム routine である。emitter の helper 内の 25 byte read（runtime.rs 6、runtime_str.rs 6、utf8_helpers.rs 6、kmp.rs 4、value_helpers.rs、json_helpers.rs、map_index.rs が各 1）と、24 個の self-hosted `stdlib/string_*.almd` 本体内の 120 個の `prim.load8` が該当する。

**native 経路（#4006）。** verified native renderer が描画するプログラム（`proofs/native-verified-floor.txt`、958 個の `spec/wasm_cross` プログラム中 106 個、増加のみ）について、`proofs/native-wall.sh` は、描画された各関数の所有権証明書をビルドごとに判定する。証明書は renderer が実際に描画した MIR（`try_render_rust_source_certified`）から取得するため、probe charge、result-window 書き換え、make-unique 省略も証明対象内に入る。抽出チェッカー、Rocq カーネル、almide-verify で判定し、生成した Rust ソースの SHA-256 を固定する。その poison 経路は、Rust 側の鏡のチェックをバイパスして release を 1 つ失った render の witness を拒否する（`scripts/witness-reinject.sh native`）。信頼する事項：rustc が生成ソースをその収支を保つバイナリへコンパイルすること、および floor 外のすべてのプログラム。後者は証明書なしの標準 codegen から出荷される。

<a id="historical-implementation-progression-retained-evidence"></a>

## 実装の歴史的な進展（保持する証拠）

以下の積み上げ式の記述は、初期の MIR／旧 renderer 作業を記録する。コーパス件数、未完作業、削除された `check-wasm-bytes.sh` への参照は歴史的なものであり、現在の出荷物の保証ではない。旧 renderer は #2761 で廃止され、`Wasm*` 定理はモデルの定理として残る。現在の structural-runtime のバイト根拠付けは `check-structural-bytes.sh`、artifact/witness 検査は `structural-wall.sh`、native 部分集合の検査は `native-wall.sh` が担う。現在のトレースモデルの限界は上で述べた。正確な certified 集合と信頼境界は receipt を参照する。

初期の進展では、その範囲を次のように記述していた：

- **飛行品質の性質の集合は、値意味論の部分集合上で完全である**：RC バランス（`MemorySafe` の TEMPORAL 条件、後述）、名前の全域性、capability 上限（推移的なものを含む）、型の具体化、メモリモデルのリーク不在、再利用の健全性（`Reuse` は一意に所有されたオブジェクトだけを対象とする。別名参照のあるインプレース再利用はない）、free-list の再利用安全性（有効な割当てが現在生存するブロックを返すことはない。解放後再利用はない。`FreeList.alloc_not_live`）、region reset の安全性（`RegionSave`/`RegionRestore` の区間は、任意の本体をまたいで allocator 不変条件を保持する。したがって frontier reset 後、回収済み領域を指す free-list 項目は残らず、reset を越えて再利用安全性が保たれる。`FreeList.region_window_reuse_safe` / `FreeList.region_reset_leaves_no_free_into_the_region`）、PINNED_RC の不滅性（`$alloc8` / `__alloc_pinned` で割り当てたブロック、すなわちホストが書き込むすべての WASI buffer と preopen table は、任意の実行にわたって解放されず、free-list に入らず、`$alloc` から返されず、region reset を越えて生存する：`FreeList.pinned_stays_immortal_forever` / `FreeList.p_region_reset_preserves_PINV`。これは C-042 が違反した不変条件）、再利用のカウント側（free-list から返されるブロックに古い参照カウントは残らず、正確に 0 で渡される。`FreeListRc.reuse_hands_back_a_zero_count_block`。constructor の store により正確に `RC_INITIAL` = 1 になる。`FreeListRc.reuse_restores_rc_1`。任意の実行と region reset を通じ、free-list はカウント 0 のブロックのみ、生存集合は正のカウントのブロックのみを保持するため、二重 release は `$rc_dec` の sentinel で必ず TRAP する：`FreeListRc.double_release_traps` / `r_region_reset_preserves_RINV`）、copy-on-write の別名安全性（`MakeUnique` は一意に所有されたブロックを返す。別名参照のあるインプレース変更はない。`CowSafety.make_unique_yields_unique`）、バイト対応表と `rt_dec` / `rt_inc` を実現する `$rc_dec` / `$rc_inc` 命令木（`WasmRcDec`）、rc_inc 命令木から実際の wasm バイト列へのエンコード（`WasmEncode`、wat2wasm と対応付け済み）、その実際のバイト列が wasm stack machine 上で `rt_inc` として実行されること、汎用インタープリター（`WasmExec`）によって renderer の実際のバイト列上で実行される `$rc_dec` 全バイト列の安全性（二重解放なしとリーク不在）、オペランドスタックの均衡、ループのない部分の停止性。すべてカーネル検査済みで追加公理なし（監査済み 137 定理）。残るのは深さ（バイト対応 ISA 層、および free-list / `rc_inc` を実現する RENDERER。その安全性モデルは証明済みなので、この部分は信頼する runtime を増やすのではなく証明を精緻化する）と広さ（部分集合を超える lowering：制御フロー、クロージャ、stdlib）であり、部分集合上の新たな性質ではない。
- **MIR-lowering の WALL は v0 コーパス全体で実証的に検証されている**（`proofs/corpus-wall.sh`、`make verify-trust` → CI に含む）。v0 spec コーパス全体（465 個の `.almd` ファイル、MIR に到達する 4195 関数）を実際の frontend → `lower_function` に通し、手作り MIR だけでなく実際のソース上で二つの健全性不変条件を検査する。(1) **wall** — `lower_function` はコーパス全体で全域的であり、すべての関数は `Ok`（profile 内）か明示的 `Unsupported`（wall で遮断）になる。**panic ゼロ、検知されない拒否ゼロ**（値意味論の部分集合外のプログラムは理由を伴って拒否され、黙って誤 lowering されない）。(2) **accept ⟹ 証明済みの三つの性質すべてについて安全** — カーネルで証明されたチェッカーが、profile 内のすべての関数の witness を再検証し、所有権（二重解放／リークなし、`check_sound`）、名前の全域性（宙に浮いた MIR 参照なし、`check_names_cert_sound`）、capability 上限（未宣言のホスト effect なし、`check_caps_cert_sound`）について ACCEPT する。witness の粒度は性質ごとに異なり、ゲートもそれに従う。所有権チェッカーは heap object にわたり FOLD する（ストリーム全体を一度 fold）一方、名前／capability チェッカーは単一の `<superset>|<subset>` witness を parse するため、一関数ずつ再検証する（まとめたファイルでは全関数の id を一つの superset に誤って fold する。このゲートの構築中に発見し、取り込んだ）。これは step 4 の「継続的なコーパス検証 = parity の定義」の、誠実な最初の形である。まだ完了の定義（証明済み profile が全コーパスを受理すること）を主張せず、そこへの進捗を測る仕組みを確立し、境界が穴でなく wall であることを証明する。**現在の正直なカバレッジは、所有権＋名前について 4083/4195 関数が profile 内（97%）。caps-VERIFIED はより少ない、parity を拘束する 3528（caps の注記を参照）**。対象は値意味論の部分集合に加え、**`Range` 値、CLOSURE 値、解決不能な `Method`/`Computed` ターゲット呼出し**（`f(0..n)`、`var g = (x) => …`、`obj.method()`、`(g)()`）。`Range` と CLOSURE 値（新しい heap env）は新しい値であり、解決不能な呼出し先（ここでは dispatch／closure 値が不明）は遅延された新しい値（heap の `Alloc{Opaque}` / scalar の `Const`）としてモデル化する。receiver／引数の呼出しは捕捉するが、method／computed 呼出し自体は省略するため、`ir_calls > mir_calls` ゲートが関数を caps-unverified として taint する。呼出し先の capability が不明なので正直な扱いである。さらに**エラー演算子**（`e!`/`e?`/`e ?? d`/`e?.field` は新しい値、すなわち unwrap／default／map された結果を生成し、すべての Opaque と同様に遅延する。オペランドの呼出しは捕捉する）。Almide に `try`/`catch` はない。`e ?? d`（unwrap-or-default）、`ToOption` としての `e?`（`Result → Option`）、`e?.f`（optional chaining）は制御フローのない全域的な値写像なので、常に安全に遅延できる。**`e!`（`Unwrap`）と effect-fn の自動 `?`（`Try`）は `Err` を早期 return する**。この早期 return は v0 WASM codegen のリークだった。Err 経路の素の `return_` が関数の scope-end heap free を飛び越す（Perceus の rc_dec は終端 `Ret` にしかなかった）ため、早期 return 時に生存する heap local がリークした（Rust は常にリークなし：`e!`→`?`→scope-exit `Drop`）。遅延して続行する証明書は balanced（チェッカーは no_leak を証明する）なので、その形を証明書で認めれば **accept-but-unsafe** になった。**修正済み**（単に wall で遮断したのではなく、根本の v0 バグを修正）。wasm emitter は各 `Try`/`Unwrap`/`Fan` の Err 経路の `return_` で、その時点で生存する heap local を伝播前に解放する。`emit_early_return_decs` は Perceus 自身の rc_dec liveness を読む（所有 heap の稼働中集合。heap Bind で追加、RcDec で削除し、env-borrow と donate-only の `__*` temp は除外。返す Err ptr は scratch temp で決して解放しない）。リークなし（100k 回 ×100KB の err-loop が OOM でなく完走）と二重解放なし（wasm コーパス全 260 ファイルとクロスターゲットのバイトゲートが成功）を実証的に検証した。したがって早期 return の形は両ターゲットで忠実に lowering される。move/consume モデルは Dec なしにユーザーの heap local を move しない（RcInc による alias、検証済み）ので、稼働中集合は生存集合と正確に一致する。docs/roadmap/active/v0-unwrap-early-return-leak.md を参照。さらに**分解パターン**（`match` 分岐の `Some(x)`/`Ok(v)`/`Foo(a,b)` と、`let Foo{..}=`/`let (a,b)=` は payload をコンテナ粒度で束縛する。heap 束縛は subject 全体を alias する（`Op::Dup`、証明済み `a` event を再利用。要素／payload 単位の厳密な同一性には layout brick が必要で、ほかの Opaque と同様に遅延）。scalar 束縛は `Const`。record の shorthand field は wall で遮断する）。
  さらに**高階の純粋 combinator**（closure を伴う `list.map`/`filter`/`fold`/…、値位置と EFFECT／文位置の両方）。純粋 combinator は呼出し中に closure を実行して破棄するため escape しない。closure 引数は capability によって扱う（`Lambda` 本体の呼出し、`ClosureCreate`/`FnRef` の呼出し先 → effect marker。closure の caps が witness に届き、`mir<=ir` ゲートが入れ子の高階／解析不能な本体を taint する）。値は遅延し、capture は借用する。OPAQUE な関数値（caps を解析できない）は wall で遮断する。値の結果は新規所有、Unit／scalar の effect 結果は所有権を持たず、捨てる heap 結果は割当て後に drop する。
  さらに **`for`/`while` ループ**。反復ごとの scope frame により、モデル化した 1 回の反復が内部で balanced ⟹ 任意の N に対し N 回の実行時反復はリークなし。loop op はない。heap iterable は借用、または `lower_call_args` で実体化する。ループ変数は反復ごとにコンテナを alias する（`Op::Dup`、コンテナ粒度）か scalar の `Const`。SCALAR のみの frame に対する `break`/`continue`（引数なし、値なし、ラベルなし。Almide には `break x`/`return` がない）は no-op として許可する（frame に heap handle がなく、実際の早期脱出が Drop を飛ばさず、どちらのターゲットでもリークしない）。しかし HEAP frame（heap loop variable の `Op::Dup`、heap body local）では wall で遮断する。v0 wasm backend は break の分岐先の後で解放するため、実際の早期脱出は反復ごとの heap handle をリークする（敵対的検証で捕捉し、出荷していない accept-but-unsafe）。heap 再代入、つまりループのアキュムレーター `acc = acc + [x]` は再束縛せず遅延する。`acc` は反復間で生存しているループ前の handle を維持し、後続反復が読む handle を先行反復が drop することはない（メモリ安全。蓄積自体は Opaque と同様に遅延）。また frame handle ではないため、scalar loop＋heap accumulator＋break は許可する。scalar 再代入 `i = i+1` も許可する。さらに **`if`/`match` 制御フロー**（文／scalar／Unit／HEAP の末尾位置と heap 束縛位置）。分岐を分岐ごとの scope frame とともに平坦な op stream へ線形化し、branch op はない。brick 5a 以降、分岐ごとの規律は信頼するのでなく検査する。各 object の退出時カウントについて分岐が一致しなければならない（分岐でグループ化した証明書 `{…|…}` と `verify_ownership` の branch join。一方の分岐が外側 handle を外へ move すると、release parity が兄弟分岐へ補償 Drop を挿入）。結果は呼出し側が出力する単一の合流 slot で、破棄（Unit／文）、`Const`（scalar）、または新しい `Alloc{Opaque}`（heap。構成上メモリ安全で、内容値は Opaque 同様に遅延）になる。heap の `match` subject は実体化する（新しい値は scope end に drop する所有 temp へ、追跡済み変数は借用）。payload 束縛パターンはコンテナ粒度で束縛する（後述）。分岐 guard は wall で遮断する。HEAP 変数を再代入する分岐は遅延する。変数は分岐前の handle を保持するため、分岐後の読出しで、その分岐が drop した handle を参照しない（経路依存の UAF なし）。
  さらに**タプル分解**（`let (a,b) = (x,y)` は要素ごと、`(a,b) = t` は要素ごとにコンテナを alias）、**field／element 代入**（xs[i]=v / r.field=v → MakeUnique copy-on-write）、**任意の heap 引数の println**（`println("x")`/`f()`/`a++b` を実体化して借用。Stdout に到達するので caps-unverified という正直な扱い）、および**再代入**。`x = v` は `x` を再束縛する。古い束縛は scope end まで残り、正確に一度 drop する（保守的な寿命延長であり、二重解放はない）。`v` 内の古い `x` の読出しは、まだ生存する古い handle を借用する（再束縛前に lowering する）ため、UAF はない。加えて **field／element 抽出** — `xs[i]` / `r.field` / `t.0` / `m[k]`。scalar 結果は明確なコピー → `Const`。HEAP 結果は既存の `Op::Dup`（v1 のコンテナ粒度 field access）によってコンテナを alias する。抽出値はコンテナ全体への第 2 の handle となり、その値の寿命中コンテナを生存させる（保守的な寿命延長であり、UAF はない）。証明済み `a`/alias event を再利用するため Coq checker と backing gate は不変で、field 単位の厳密な alias（値自身の object）は正直に layout brick へ遅延する（LayoutId は現在 placeholder）。入れ子のコンテナ抽出（`a.b.c`、単一の追跡済み `src` がない）は wall で遮断したまま。さらに式本体関数、直接の heap literal return、名前付き呼出し結果の直接 return、**借用 heap parameter** を取る関数、**一階の純粋 stdlib `Module` 呼出し**、**入れ子の CALL 引数**（`f(g(x))` / `assert_eq(g(x), …)` を所有 temp に実体化し、外側呼出しへ借用、scope end に drop）、**literal の CALL 引数**（`f("x")` / `f([1,2,3])` / `f(3.14)`。heap literal は `Alloc`、scalar literal は `Const`）、**Option・Result constructor**（`Some(x)` / `Ok(e)` / `None` / `Err(e)`。heap variant はコンテナ literal と同様に実体化）、**BinOp / UnOp**（`a+b` / `s1++s2` / `-n`。新しく計算した値。heap concat は `Alloc`、scalar 算術／論理は `Const`。オペランドは自身の所有権を持ち、値意味論なので結果は決して alias ではない）も含む。残りは機能ごとの `Unsupported` histogram で wall に遮断し、次に許可する領域を示す（現在最大の分類は正確な stdlib 関数名を示す：closure 引数付き `list.map`/`filter`/`fold`、すなわち高階 brick と、呼出しを引数とする実体化）。
  **stdlib の `Module` 呼出しは純粋なものだけを許可する**。`<module>.<func>` 呼出しを `Op::CallFn` へ lowering するのは、一階（closure 引数なし）かつ呼出し先が証明可能に純粋（host capability に到達しない）な場合だけ。これは capability の健全性による制約である。証明済み checker は `Op::Call` の型付き `RtFn` だけから `used` capability を導出するので、effectful stdlib 名への `CallFn` は capability を `used` から黙って落とし、accept-but-unsafe になる。純粋な呼出し先は空の capability 集合へ到達し、空の `used` witness が忠実に表す。effectful（`effect fn` キーワードによる `fs`/`http`/`net`/`io`/`env`/`process`/`random`/`zlib`）、impure-plain（`datetime`/`args`/`mem`/`testing`。キーワードなしで host に到達）、高階呼出しは wall で遮断し、決して lowering しない。純粋性レジストリ（`crates/almide-mir/src/purity.rs::PURE_MODULES`）は全面的に信頼しない emitter 内に置く。**乖離ゲート**（`corpus-wall.sh` 内の `proofs/check-stdlib-purity-registry.sh`）が `stdlib/*.almd` から effectful 集合を再導出し、許可した module が `effect fn` を獲得する、または stdlib module が未分類のまま残ると失敗するため、pure→effectful の乖離は黙って出荷できない。**capability の性質は推移的に検査し、範囲は正直に Stdout に限定する。** 空の capability witness（caps-safe という主張）を出力するのは、保守的 fold（`certificate::reaches_capability_or_unknown`）がすべての `Op::CallFn` 辺を通じて Stdout に到達しないと証明する場合だけ。profile 内の呼出し先は fold し、純粋 stdlib `Module` 呼出し、variant constructor、既知の Stdout-free builtin（`assert*`/`eprintln`/`panic`/`to_string`。stderr／abort へ到達するが Stdout には到達しない）は問題なし。それ以外の不明な呼出し先（wall で遮断された、または別ファイルのユーザー関数）は必ず taint するので、誤って受理せず `caps-unverified` と報告する（3528/4083 を検証済み、555 は未検証）。**ゲートが検証するのは実際の capability-bound 性質 `reachable ⊆ declared`**（まさに `proofs/CapabilityBound.v` が証明するもの）であり、退化した「どんな capability にも到達しない」ではない。`lower_function` は各関数の effect signature を `declared_caps` 上限へ lowering する。`effect fn` は `{Stdout}`（唯一モデル化した cap）、純粋な `fn` は ∅ を宣言する。classifier は推移的な到達可能 cap 集合を fold する（`certificate::reachable_caps_or_tainted`。taint があれば `None`、すべての辺が解析可能なときだけ `Some(set)` を返す）。その後 `<declared>|<reachable>` を出力し、証明済み `check_caps_cert` が `reachable ⊆ declared` を再検証する。つまり effectful な関数は自身の宣言上限に対して検証される（出力する `effect fn` は、使用する Stdout を宣言済みなので受理される）。単に capability に触れるため除外されるのではない。未宣言の capability に到達する関数、例えば出力する非 `effect fn`（frontend の `is_effect` flag はすべての Stdout 到達を覆わない）は `reachable ⊆ declared` に違反し、保守的に caps-unverified となり、誤って受理されない。
  **Opaque lowering で省略された呼出し**（list element、ctor payload、BinOp operand、scalar 値。その部分式は lowering されない）は、fold から見えない第 2 の caps 死角である。`lower::record_elided_calls` が各呼出しを素の EFFECT MARKER `Op::CallFn{dst:None, args:[], result:None}` として表面化する。既存 handler は結果も dst もない呼出しを、値を伴わない EFFECT のみとして扱い、所有権 event も値参照も出さない（所有権／名前の witness と `+1` backing gate は不変）が、caps fold は名前で照合して呼出し先を推移的に fold する。健全にモデル化した呼出しだけを表面化する。一階の `Named` 呼出し（fold が開くか、正直に taint）と一階の純粋な `Module` 呼出しである。高階／effectful な `Module`／`Method`／`Computed` 呼出しは省略したままにして、ゲートが関数を taint した状態に保つ（誤った taint 解除なし）。健全性の安全網：marker は位置全体が省略された箇所でのみ記録するため、MIR 呼出し数は IR の数へ向かって増えることしかない。corpus gate は `mir_calls <= ir_calls` を検査し、二重計上（marker が本当の省略を隠し、誤って taint 解除できる唯一の方法）を WALL BREACH とし、構造的に出荷不能にする。ソースに MIR より多くの呼出し node がなお残る関数（実体化できない省略呼出し）、およびその推移的呼出し元は保守的に taint したままなので、caps-verified 数は正直で、過大主張しない。これで直接 witness の穴（`reachable_caps` の honest-scope：不明呼出し先が ∅ を寄与していた）を閉じる。正直な範囲：モデル化するのは `Capability::Stdout` のみであり、性質は「未宣言の **Stdout** effect がない」である。stderr、abort、fs、net は実在する host effect だが、まだ名前を付けていない（より広い `Capability` 集合と frontend で lowering した `declared_caps` は後の brick）。
  **heap parameter はデフォルト借用の呼出し規約を使う**（呼出し側が参照を所有し、parameter は所有の `+1` を証明書へ寄与しない）。現在の runtime で健全なのはこの規約だけである。所有 parameter の `+1` は合成されたもので、実際の runtime `Alloc`/`rc_inc` に裏付けられず、ゲートから見えない use-after-free のクラスになる。借用 parameter が自身の参照を先に獲得せず（`Dup`）return／release することは明示的に wall で遮断する（`returning a borrowed param directly`）。その証明書は rc 0 での release となり、証明済み checker が誤りとして検出する。**再発防止 backing gate**（`corpus-wall.sh` 内、unit test にも反映）が、証明書のすべての `+1` に実際の `Alloc`/`Dup`/heap-result の裏付けがあることを検査するので、合成 parameter 所有権の再導入は構造的に出荷不能である。注：この brick で証明済み Coq checker は不変（証明書の出力だけが裏付けのない parameter `i` を除いた）なので、checker サイズの不変条件は保たれる。カバレッジは報告し、脆い厳密件数でゲートしない。強制するのは健全性不変条件と、崩壊防止の下限（profile 内の witness が 1 つ以上 checker に到達すること。カバレッジが黙ってゼロに崩壊するとゲート失敗）だけである。
- **添字境界のメモリ安全性 — 発見し wall で封じた穴。** 所有権チェッカーは RC の性質を証明するが、リスト添字の境界は検査しない。renderer の `$list_set`/`$list_get`/`$elem_addr` にも境界検査がなかったため、範囲外の添字はブロック外のアドレスを計算し、そこへの store はメモリを破壊する。受理されるのに安全でない穴であり、RC とは別のメモリ安全性の軸である。これは（黙って処理するのではなく）wall で封鎖した。現在の `$elem_addr` は `idx < 0 ∨ idx ≥ cap` で trap（`unreachable`）するため、すべての要素アクセスは境界検査され、OOB はメモリ破壊ではなく制御された停止になる（wasmtime で検証：OOB は trap し、境界内は trap しない。値意味論の出力はバイト単位で不変 — 実際のアクセスはすべて境界内）。同じ監査で、もう 1 つの無制限書込みの欠落も塞いだ。`$print_list` は固定バッファー `[SCRATCH_ADDR, HEAP_BASE)` に上限なしで出力行を構築していたため、非常に長いリストは行をヒープへあふれさせた。現在は `HEAP_BASE` を越える要素を追加する前に trap する（print-buffer wall）。より根本的な修正、つまりチェッカーが OOB を静的に拒否すること（実行時ではなく検査時の wall）は後の brick であり、現在の wall は実行時 trap である。
- **wasm renderer は RC 方式にある（A1.1b）：drop ごとに release を出力する。** `Drop` は現在 `call $rc_dec` として描画され、参照カウントセル（A1.1a の再配置でヒープオフセット 0 = `RuntimeModel.RC_OFFSET` に配置）を 0 へ減らす。つまりバイナリはセル単位で実際に解放する。それに応じて安全性の基礎も移った。もはや `eager_copy_refines_safety` ではなく（成果物は Dec-free ではない）、`RuntimeModel.balanced_cert_no_memory_fault` — 受理された（バランスした）証明書はメモリ機械で二重解放しない — と `balanced_cert_frees_in_memory` — セルが最終的に解放済み（rc 0）になる — を使う。両方ともすでにカーネル証明済みで追加公理なし（この区分は純粋な証明再利用であり、`.v` の変更はない）。ビルドごとの V である `validate_translation_perceus` は、witness の各 drop を `call $rc_dec` のバイト列へ結び付ける（drop ごとに release が 1 つ、それ未満ではない）。これにより証明が実際のバイト列へ移り、`$rc_dec` の実行時 sentinel が二重解放を trap する（すでに 0 のセルで `unreachable` — wasmtime で発火を確認）。したがって `C-SAFE` の二重解放不在とセル単位のリーク不在を、モデルだけでなく出力成果物について主張できるようになった。物理的な回収も現在は実現済み（A1.2-render）：rc→0 の `$rc_dec` はブロックを free-list に戻し、`$alloc` はサイズが完全一致する先頭を再利用する。これは証明済み `FreeList.v` モデルの詳細化である（`alloc_not_live`：有効な割当てが現在生存中のブロックを返すことはない — reuse-after-free がない）。二重解放 sentinel は保持される。free-list のリンクは死んだ LEN フィールドにあり、rc セルにはないため、解放済みブロックを再度 release すると引き続き trap する（検証済み：二重解放 trap テストと、alloc/free/realloc で `p1==p2` となる再利用テストの両方が wasmtime で通過。値意味論の出力はバイト単位で不変）。#909 の第 3 sentinel 以降、この文は単に検証しただけでなく証明されている。`FreeListRc.v` は allocator とともに参照カウント（`RuntimeModel.read_rc`、バイト定理が書き込むものと同じセル）を持ち、`double_release_traps` — free-list のブロックを読むと 0 なので sentinel が発火する — と、alloc/store の組である `reuse_restores_rc_1` を証明する。信頼に残る部分は以前より狭く、今は単なる断言ではなく実行可能なゲートで検査する。emitter が rc 管理対象の各 `call $alloc` 箇所でその `RC_INITIAL` store を書くことを、`almide-mir` の `every_rc_managed_alloc_site_initializes_the_rc_cell` が検査する。この検査は renderer 内の全 16 個の `$alloc` 箇所を列挙し、各箇所が rc セルを初期化するか、名前を明示した raw-scratch 箇所（証明済みの `alloc_raw` カテゴリー）であることを要求し、定数が乖離しないよう `.v` ファイルから `RC_INITIAL` を読む。共有についての正直な範囲も実現済み（A1.3-render）：`Dup` は `rc_inc` で共有し（コピーなし）、`MakeUnique` は copy-on-write（共有時に clone — まず `rc_dec` を行うので別名参照が元の内容を生存させ、一時変数は不要。その後 `list_copy`）となり、`CowSafety.make_unique_yields_unique` を詳細化する。rc セルは現在、抽象参照カウント（1→2→1→0）を実際に追跡し、証明済み RC 機械（`WasmRcDec`/`RuntimeModel`）を動作させる。値意味論の出力はバイト単位で不変である。したがって A1 の renderer は、共有／cow／free-list／二重解放 sentinel のすべてが実際の RC となり、各部分が証明を詳細化し、信頼するランタイムの追加はゼロ。未完了（安全性ではなく性能／エンコード）：free-list はサイズ完全一致の先頭のみを照合する（サイズの違う解放済みブロックはまだ再利用しない — 再利用を逃すだけで、安全性を損なうことはない。サイズクラス／走査は後の区分）；命令木の生バイトエンコード（A2 で延期した重い半分）。
- **バイト列との結び付けは部分的である。** op→wasm 命令の表は形式的な Coq オブジェクト（`Translation.v`）であり、ランタイムのヒープは、その rc セルが抽象参照カウントを追跡すると証明されたメモリ状態機械としてモデル化される（`RuntimeModel.mrun_tracks_exec`）。`validate_translation` はビルドごとに各 op のパターンが出力されたこと（drop のパターンは `call $rc_dec`）を再検査し、`validate_translation_perceus` は drop ごとに release が 1 つ出力されたことを再検査する。モデルの `RC_OFFSET = 0` は renderer の物理 rc セルのオフセット（A1.1a 再配置）と一致し、`call $rc_dec` はそのセルに書き込む。**A2 最初の区分は完了（命令木レベル）、`WasmRcDec.rc_dec_prog_realizes_rt_dec`**：renderer が出力する正確な `$rc_dec` 命令木（load/add/sub/store/trap 断片の小さな操作的意味論とともにデータとしてモデル化）が `RuntimeModel.rt_dec` を計算することを証明した。同じ trap（セル 0）、同じ減算である。したがってリーク／二重解放不在の証明が使う抽象 release は、単なる token ではなく、出力命令が計算するものに一致する。バイトエンコードも現在は結び付いている（A2 バイト区分、`WasmEncode.rc_inc_bytes_encode_the_instruction_tree`）。Coq の wasm バイナリエンコーダーは、renderer の `$rc_inc` に対して `wat2wasm` が出力するバイト列と完全に同じものを生成する。`proofs/check-wasm-bytes.sh` がビルドごとに対応付けを確認する。#932 以降、ゲートは手でコピーした定数を持たない閉じた三角形である。アセンブルする WAT は renderer 自身の描画出力から抽出し、比較先のバイトリストは `.v` ソースから解析するので、証明から乖離する renderer の編集は定理を孤立させず、ゲートを失敗させる（非循環かつ renderer に接地）。`rc_inc_prog_realizes_rt_inc` と合成すると、出力バイト列は `rt_inc` を計算する命令木をエンコードする。さらに実行も結び付いた（`WasmExec.rc_inc_bytes_execute_to_rt_inc`）。最小の wasm スタック機械が実際の rc_inc バイト列を実行し、メモリ効果は厳密に `rt_inc` となる。つまり実行されたバイト列が抽象 acquire を計算するのであり、計算するはずの命令をエンコードするだけではない。したがって `rc_inc` は端から端まで結び付いた。命令木 ↔ 実際のバイト列 ↔ 実行 ↔ rt_inc と、信頼の連鎖が実際の wasm バイト列に届く。インタープリターは二重解放 trap の制御フローも実行する（`WasmExec.trap_bytes_trap_on_zero` / `_pass_on_nonzero`：`(if (i32.eqz cell) (then unreachable))` のバイト列はセルが 0 のとき、かつそのときに限り trap する。実際に対応付けられたバイト列上の sentinel である）。したがって直線的コードと安全上重要な trap の両方を扱う。一般的な構造化制御フローの基礎も構築済み（`WasmExec.skip_block` + `imm_len`）。opcode と衝突する即値（`i32.const 4` = `41 04`、0x04 は `if`；`i32.const 11` = `41 0b`、0x0b は `end`）に惑わされず、ブロックに対応する `end` を見つける、即値を認識する構造探索器である。まさにこれらの衝突例について証明済み。これは一般制御フローに必要なのが opcode ごとの小さな即値長表だけであり、完全な WasmCert-Coq パーサーではないことを示す。そして現在は一般的な `if` 実行器（`WasmExec.run_g` + `split_block`）に接続済み。fuel に上限を設けたインタープリターが一般的な構造化 `if … end` を実行し、条件が非ゼロなら then 本体を実行し、それ以外なら飛ばす（`if (cond) (then store 0:=42)` で本体を実行／スキップすることを証明）。固定 trap パターンを超える。さらに `global.get`/`set`（予約セルとしての `$freelist` global、往復を証明済み）と、添字付き locals + `local.set`（`$rc` 一時値用の locals 環境）も備える。これにより、free-list と `global.get`/`set` を含む完全な `$rc_dec` バイト列を wat2wasm と対応付け（check-wasm-bytes.sh）、rc_dec の両方の安全性を、それらの実バイト列上で一般インタープリターによる実行を通じて証明した。`rc_dec_bytes_trap_on_zero`（二重解放なし — すでに 0 のセルを release すると trap）と `rc_dec_bytes_frees_when_one`（リーク不在 — 有効な release は rc セルを 0 としてブロックを解放）の両方を、renderer の実際の `$rc_dec` バイト列で実行する。信頼モデル（「保護するのは安全性であり、機能的正しさではない」）に従えば、重要な rc_dec バイト結合はこの 2 つである。free-list push の構成（解放したブロックがどのリストへ入るか）は機能の問題であり、安全性の性質ではない。インタープリターの実行は現在、両側から接地される。バイト列は wat2wasm と、実行は実運用エンジンと対応付ける。wasmtime 差分テスト（`rc_cell_values_match_the_interpreter_on_-
  wasmtime`）が、実エンジンは run_g が予測するのと同じ rc セル値を計算することを確認する（rc_inc 1→2、rc_dec 1→0）。したがって残る部分は「run_g = wasm 仕様を信頼する」から「wat2wasm/wasmtime = 仕様」へ縮小する。実運用ツールはツールチェーンの他の部分と同じ信頼レベルである。未完了：機能面のリスト操作ランタイム（bootstrap-runtime の負債。手で結合するのではなく、証明済み経路を通じて Almide で self-host する、#30）；wat2wasm/wasmtime への対応付けにとどまらず、インタープリター↔仕様の残存部分を完全に閉じる Coq 内の完全な WasmCert-Coq ISA。
- **実際の `.almd` が 1 つ、端から端まで流れるようになった**（`proofs/fixtures/return_list.almd` → 実際の frontend → MIR → 証明済みチェッカー、所有権と名前について — 週次指標 ① 0→1）。lowering が扱うのは値意味論の部分集合のみ（ヒープリテラル、別名参照、添字代入の copy-on-write、scalar/heap-move-out return — 呼出しや制御フローはまだない、#29）。したがって広範な拒否例と capability witness はまだ代表的な MIR 形状（emit_cert.rs）である。
- **抽出は照合される高速経路であり、判定の根ではない**（上記項目 2、brick 6b）。カーネル oracle はすべてのゲートとコーパスの判定を再検証する。検証済み抽出／CertiRocq 採用は、高速経路自体について記録された将来のラチェットである。
- **独立チェッカーは 1 つ。** 多様性（独立チェッカー ≥2）は brick 6 である。

<a id="proven-vs-trusted-boundary-map-flight-evidence-gaps-f3-1"></a>

## 証明済みと信頼に置く部分の境界図（flight-evidence-gaps F3-1）

監査者の最初の質問、**厳密には何が証明済みで、何が信頼するエンジニアリングなのか**への 1 ページの回答。（2026-07-03 — 実地の点検で出力を壊すバグを 5 件発見した後に記載。すべて信頼に置く領域内だった。）

```
                         ┌──────────────────────────────────────────────┐
  .almd source ──parse──▶│ FRONTEND (trusted Rust)                      │
                         │  check / lower / optimize / mono / ir_link   │
                         └──────────────┬───────────────────────────────┘
                                        │ IR
                         ┌──────────────▼───────────────────────────────┐
                         │ MIR LOWERING (trusted Rust, WALLED)          │  ← the five 2026-07-03
                         │  lower_function + pre-desugars               │    bugs lived HERE
                         │  emits per-function WITNESSES                │
                         └──────────────┬───────────────────────────────┘
                                        │ MIR + certificates
                         ┌──────────────▼───────────────────────────────┐
                         │ CHECKER (Coq-PROVEN kernel)                  │  ← accept ⟹ ownership ∧
                         │  check_all_sound / names / caps-transitive   │    names ∧ caps (37 thms)
                         └──────────────┬───────────────────────────────┘
                                        │ accepted MIR
                         ┌──────────────▼───────────────────────────────┐
                         │ RENDERER render_wasm (trusted Rust;          │
                         │  rc_dec/rc_inc byte trees PROVEN — WasmExec) │
                         └──────────────┬───────────────────────────────┘
                                        │ wat
                              wasmtime (UNQUALIFIED tool)
```

図の説明訳：.almd ソースを解析し、信頼する Rust の frontend で check / lower / optimize / mono / ir_link を行って IR を得る。MIR lowering は信頼する Rust で、wall が設けられ、lower_function と事前脱糖を行い、関数ごとの witness を出力する。2026-07-03 の 5 件のバグはここにあった。MIR と証明書を Coq で証明済みのカーネルチェッカーへ渡す。check_all_sound / names / caps-transitive により、受理は所有権・名前・capability を含意する（37 定理）。受理済み MIR を renderer render_wasm（信頼する Rust。rc_dec/rc_inc のバイト木は WasmExec で証明済み）へ渡し、wat を得る。最後は wasmtime（適格性確認されていないツール）。

証明が与えるもの：受理された関数は、二重解放、リーク、名前の未解決参照、未宣言 capability への到達を起こさない。証明が与えないもの：lowering が正しい意味論を選んだこと。証明書で健全と判定された関数でも、誤った文字列を表示し得る。その欠落を覆うのは差分証拠（出力同等性 baseline + org テスト群）だけであり、その到達範囲を `proofs/coverage.sh` が測定する。

<a id="ffi-an-externs-declared-purity-is-the-authors-claim"></a>

### FFI：`@extern` に宣言された純粋性は作者の主張

`@extern(wasm, ..)` import はプログラム外のコードであり、capability 証明書は内部を見ることができない。その各呼出しは FOREIGN（`cert_project::cap::FOREIGN`）を記録するので、報告には常に外部コードへの到達が示される。そのコードにホストへの effect があるかどうかは extern の宣言が述べる。通常の `fn` extern は FOREIGN だけを記録し、チェッカーと同様に、通常の `fn` の上限はこれを許容する。`effect fn` extern はさらに FOREIGN_EFFECT を記録し、これは `effect fn` だけが許容する。通常の `fn` extern の純粋性は検査されず、信頼に置く。これは作者の宣言であり、すべての FFI 境界で置くのと同じ信頼である。`witness_decls::tests::a_plain_caller_of_an_effect_extern_is_refused` および `tests/verify_shim_test.rs::a_plain_fn_calling_a_pure_extern_builds_under_the_verifier_and_reports_foreign` によって固定されている。

<a id="the-five-2026-07-03-trusted-zone-bug-classes-and-their-regression-pins"></a>

### 2026-07-03 の信頼領域の 5 つのバグ分類と回帰防止箇所

| 分類 | 壊れたもの | 固定するゲートまたは fixture |
|---|---|---|
| match の線形化が effect を持つ両方の分岐を実行 | wall が成功していても出力が誤る | 呼出しを含む分岐の WALL guard（`lower_branch`）+ parity baseline の `mutual_recursive_types` / grammar dispatch fixture |
| never-err strip と実際の Result ブロックの不一致 | Result handle からレコードフィールドを読み取る | parity baseline の `effect_assign_unwrap`（全 5 経路）+ lifted/self strip ゲート |
| scalar module-globals を Const-0 へ lowering | トップレベル `let` の使用が毎回 0 を読む | const-init の実体化 + 呼出しを含む初期化への WALL；baseline の `top_let_test` |
| lifted lambda が variant/global registry を失う | `filter` がすべての要素を落とす | sub-ctx の継承（binds.rs）；baseline の `closures_and_variants` |
| `$_start` が明示的 Result の main をスタックに残す | すべての Result-main CLI で無効な wasm | `$_start` tag-read/drop；grammar CLI matrix をバイト検証 |

| `prim.handle(<literal>)` が遅延 Const **0** に解決 | 生成された case table がアドレス 0 を読み、すべての lookup が黙って失敗（導入中の 2026-07-03 に発見） | prim 引数ループでの literal 実体化（calls_p4）+ parity baseline の `string_case_unicode` |

6 件すべてに共通するパターン：信頼領域内で、REGISTRY または規約（追跡集合、レイアウト表、呼出し規約、scalar の遅延 Const fallback）が producer と consumer の間で乖離した。遅延 Const fallback（部分集合外の scalar 末尾は `Op::Const` のゼロへ lowering され、呼出しは省略される）は、このパターンが体系的に残る例である。設計上、出力の正しさと引き換えに caps のカバレッジを得ている。scalar 関数の出力が一度も観測されなかったときには健全だったが、現在は健全でない。その廃止（値が観測される経路では Const ではなく wall）を flight-evidence-gaps F2 に記録している。構造的な修正の方向は certificate-format-v1 / value-rc-cert（信頼領域を縮小すること）。戦術的な方向は wall の規律（未追跡／未知の形状を扱うすべての consumer は推測せず wall にすること）であり、現在は match 線形化と Map 表現のルーティング箇所で強制されている。

<a id="use-relativized-completeness"></a>

## 用途に相対化した完全性

完全性は絶対的にではなく、モデルと用途ごとに宣言する。歴史的な eager-copy の結果は、その所有権断片についてのものであり、出荷する structural runtime の現在の完全性の境界ではない。トレースモデルの定理、ランタイムの詳細化、成果物ごとの証明書検査には、それぞれ異なる仮定とカバレッジがある。どれもソースから成果物までの意味論全体の証明でも、出荷されるすべてのプログラムが `MemorySafe` であることの証明でもない。receipt は、その証明書対象の成果物についての主張と、信頼に残る仮定を明記する。
