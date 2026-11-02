<a id="ci-critical-path-measured-baseline-peer-survey-and-the-ranked-plan-2026-09-21"></a>

# CI クリティカルパス：測定基準、他リポジトリ調査、優先順位付き計画（2026-09-21）

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](ci-critical-path.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](ci-critical-path.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](ci-critical-path.zh-CN.md)

> 固定原文の非規範読者用スナップショット。正本は原文であり、自動同期しない。独立した実装検証や引用指示の実行は行わない。歴史的状態、計画、比較主張は原文の範囲を保持する。
> [Original source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/ci-critical-path.md)

> 原文履歴注記：日付付きの読取り専用CI調査であり、現在のGitHub料金・repo権限・必須check・planアクセス・測定を示さない。#2444後の初期下限は推定で、その後66→43.4→25.1分の実測とlaptop由来重みの後続訂正が続く。他repo調査の一般論と例外を併存させ、rust/lean4のみとの記述でhexaや100% self-hostedのZig行を消さない。後のC step削除（#4023）と旧A+C計算もそのままで、時間を推測再計算しない。gate・権限・有料runner・workflow変更の歴史的提案は実行許可ではない。command実行・remote設定変更・課金・CI変更はしていない。


#2381 の背景調査（PR #2440、#2444 と後続）。../almide-references の読取り専用調査と ci.yml のジョブ別棚卸し。削減量はすべて §0 で指定した測定ジョブ時間からの計算である。各段階の着地後の測定結果は、得られ次第 §4 の後ろへ追記する。

読取り専用調査。基準はブランチ `ci-2381-solo-giants`（worktree `.claude/worktrees/ci-2381-critical-path`）。PR #2440（ledger を逐次 nextest group から外し、同族を shard ごとに1つへ分散）と PR #2444（5つの巨大 target を6つの `Test Rust (solo …)` job に配置）を含む。以下の数値は、指定 run の GitHub Jobs API、その worktree 内ファイル、または推定と明示した値のいずれか。

<a id="0-measured-baseline-what-the-plan-is-arithmetic-against"></a>

## 0. 測定基準（計画の計算の基になるもの）

develop run 35569545602（今夜以前の最後の green develop、端から端まで66分、06:41:34 → 07:46:59）のジョブ別時間（開始→完了）：

| job | 実経過時間 | 注記（20秒以上のstep時間） |
|---|---|---|
| Test Rust (shard 0/4) | 60.0 min | `Cargo tests` 3531 s = interp_abstain_ledger 1666 s の後に interp_bridge_fallback_ledger 1586 s、逐次化 |
| Test Rust (shard 2/4) | 41.1 min | interp_oracle 1223 + opt_parity 1075 を逐次化（family group） |
| Test Rust (shard 3/4) | 27.9 min | |
| Commissioned wasm gates | 27.8 min | carried gates `cargo test --release` 679 s → release専用root tests 436 s → double-free sweep 76 s → target-availability 426 s → ratio 25 s（1668 s 中1642 s、完全逐次） |
| Test Rust (shard 1/4) | 25.6 min | |
| Almide gates | 15.4 min | reachability ratchet 485 s、parity goldens 140 s、spec tests 135 s、WAT audit 50 s、他は各30 s未満 |
| WASM browser-ABI determinism | 11.4 min | 654 sのstepが1つ、compile律速（harness crateのwasm-pack build、`actions/cache`はemitter sourceをkeyにする） |
| WASM host-arch determinism | 8.6 min | 450 sのstepが1つ、同じ形（native + wasm32-wasip1 harness build） |
| Emit & Format | 7.6 min | pass-order shuffle 183 s、stability closure 93 s、約40の1分未満gate |
| Commissioned aviation quality | 7.4 min | codopsy build（cached）+ score |
| Test WASM (Windows host) | 6.0 min | |
| Perf ratchet | 6.0 min | |
| Build (Linux) | 5.4 min | cargo build 138 s、nextest archive 86 s、rustc ratchet 27 s、clippy ratchet 37 s、+ checkout/toolchain/cache/upload |
| Test shards cover every target | 2.4 min | |
| Lean / Test WASM / Test ARM / ratchet-separation / coverage / changes | 各 ≤ 0.7 min | |

この表の shard 名は当該 run の0始まりのもの。2026-09-22 に `Test Rust (shard 1/4)` … `(shard 4/4)` へ改名したため、ここでの `shard 0/4` は現在の `shard 1/4` に相当する。

PR #2440 run 35581361256：run 作成09:05:52、最初のjob `Change class` 開始09:34:44。作業前に**runner容量待ち29分**。`build` 完了09:40:51、shard引受09:42:38（負荷時は `needs` を1段進むごとにさらに約2分のqueue遅延）。shard は33–44分で、各巨大targetは単独より1.4–1.9倍遅かった（競合）。

巨大targetの単独時間（計画に使用）：interp_abstain_ledger 1666 s、interp_bridge_fallback_ledger 1586 s、wasm_cross_target_spec 1377 s、run_parity 1310 s、interp_cross_target_spec 1223 s、wasm_opt_parity_spec 1075 s。他の合計は約6.5 ks（`scripts/ci-test-weights.txt` は巨大target込み15,964 s。次に大きい `diagnostic_harness_test` は1501 s、7 testのfixture loop。§2.5参照）。

**#2444 後の予想下限（未測定、ブランチ上にrunなし）**：build 5.5 + solo準備約2 + 1666 s（27.8分）≈ **35–36分**。Commissioned wasm gates はその直後の5.5 + 27.8 ≈33分で完了する。ledgerの1666 sは競合するshardで測った値で、真の単独時間は#2444実行まで不明（interp側はoracleが合計1223 s内で計算する同じ関数なので、隔離したledger単独は1666 sをかなり下回るはず）。

本日時点のdevelop必須check（API）：Build (Linux)、Test WASM、Test ARM、Emit & Format、WASM host-arch determinism、Lean Proofs、Test Rust (shard 1–4/4)、Test shards cover every target、Almide gates、WASM browser-ABI determinism、acceptance-ringの名称。**必須でない：Commissioned wasm gates、Commissioned aviation quality、Perf ratchet、Test WASM (Windows host)、Coverage ratchet、Ratchet-commit separation、Test Rust (solo …)**（solo側はcoverage jobのproxy stepを通じてのみ対象）。develop merge queue ruleset：`REBASE`、`ALLGREEN`、build/merge最大5entries、check応答timeout120分。develop branch protectionにはPR review要件、push制限、admin強制がなく、直接pushは技術的に可能。

<a id="1-peer-survey-what-the-reference-repos-actually-do"></a>

## 1. 他リポジトリ調査（参照リポジトリが実際に行うこと）

出典：`almide-references/` 配下の各repoの `.github/workflows/*.yml`。`RESEARCH-neighbours.md` にはwadoとの1行topology比較（60行）以外CI記述なし。swiftとmoonbit-compilerにはCI設定なし。zigのCIは `.forgejo/workflows/ci.yaml`。

| repo | 言語／suite | runner規模 | cache | test並列化 | PR対merge/nightly | merge_group | 時間／timeout |
|---|---|---|---|---|---|---|---|
| **aver** | Rust、15 integration targets ≈ 実測2,066 s、33 cert lanes | ubuntu-latest ×11 + windows 1 | 各job rust-cache、binaryen/wasm-tools/elan/prelude に `actions/cache`、archive引渡なし | 実測秒数でLPT詰合せ（`tools/ci_test_shard.py i 4`）、さらに**分割2target内で `cargo nextest --partition slice:i/4`**。cert lanesはJSON由来 `-E 'test(/…/)' --partition slice:`、`decide` jobが分割の排他性・完全性を証明 | PR = ci + 5 smoke cert lanes（約5分）、全33 lanesは6時間ごと+ release/**、fuzz/rust-codegenはnightly限定、proptest casesはPR256対nightly2000 | なし | 「30 lanesに330分のkernel作業、各PRが約40分待ち」→PRから移動。ci.ymlにtimeoutなし |
| **vera** | Python、12,290 pytest tests | ubuntu + macos-15/26 + windows + arm（13 cells） | なし | pytest-xdist `-n auto` | stress suiteはnightlyとpath-filtered PR、cancel-in-progressはPR限定 | なし | suite3–4分、coverage cell8→3分 |
| **jacquard** | OCaml/dune | ubuntu-latest | なし | `cc: [clang, gcc]` | PR + mainですべて、main pushはcancelしない | なし | `docs/ci-cd.md`に60/90/30/240分timeoutを記録 |
| **hexa** | self-hosted言語、約32 bash gate workflows | hosted混合 + **self-hosted pool**、hosted fallback action付き | source hash keyのccache | workflow間fan-out、ファイル別上限付きxargs | PRは厳密な `paths:` filter、main無filter、cancelはPRだけ | なし（aggregator `selfhost-gates-summary`） | darwin self-hosted queueは**2.7分slotに中央値168.8分（p90 230.7）** |
| **sui** | Python | 3×3 matrix | なし | なし | PR + main | なし | なし |
| **vibe** | selfhost compiler、495-file unit battery | ubuntu-latest ×29 | 約25の `actions/cache` key、**compilerを1回buildして16 MB artifactを14 jobsへ配布** | **8 shards、LPT重み付き（`scripts/unit_test_weights.tsv`）、`VIBE_UNIT_TEST_SHARD=i/8`** | coverage main限定、concurrency blockなし | なし（aggregator `ci-required`） | 予算「CI5分未満」、cold battery5m20s→4m11s。「一度buildでjob時間7879→4971 sだがwall時間は悪化」、job開始中央値365 s対3 s |
| **wado** | Rust、4,671 fixtures | ubuntu-latest ×20 | rust-cache `shared-key: ci, save-if: false`、lockfile変更時に**別 `cargo-cache.yml` がcache公開** | `level: [O0..Os]` matrix + sibling3jobs、EMI nightly `WADO_EMI_SHARD=i/8` | `changes` docs-only skip job（必須check停滞回避で `paths-ignore` は使わない）、EMI nightly限定 | なし（aggregator `test`、`if: always()`） | 各job `timeout-minutes: 60` |
| **koka** | Haskell、約718 `.kk` cases | 5-OS matrix | stack-action、vcpkg | なし | PR/dev同じ、cancel-in-progress | なし | なし |
| **lean4** | C++/Lean、ctest経由約4.4k test files | ubuntu-latest + **self-hosted `chonk`、`runner-fallback-action` → nscloud 8×16 / 32×64 / macOS 6×14 / Windows 4×16** | ccache + sha keyのstage1 olean cache、外部Lake cache | ctest `-j$NPROC`、regex `-E/-R` subset、primary対 `matrix-secondary` | **label駆動check-level**：PRはLinuxのみ、macOS/Windows/sanitizerはmerge_group/nightly/tag限定 | **あり**（aggregator `all-done`） | 明記なし、cache networkに20分step timeout |
| **roc** | Zig、独自MiniCI | hosted、benchmark self-hosted、arm shards無効（network切断） | Zig deps cacheのみ機能（`.zig-cache` は2 GiB上限超）、**`minici-build.tar` を1回buildして7shardsで使用** | **名前付き境界によるrange分割**（`--minici-from/-to/-after/-before`） | **PRはOS別minici、全suiteはnightly**、green時に `nightly` branchをfast-forward | なし（aggregator `finish`） | shard timeout120/180 |
| **rust** | Rust+LLVM | ubuntu4c無料、**hosted4/8/16-core**、arm8-core、windows8-core、**AWS CodeBuild8c/36c、EC2 c8a.8xlarge** | sccache→S3、ghcr Docker cache | 約90-job matrix、手管理番号shard（`x86_64-gnu-llvm-22-{1,2,3}`） | **bors経由でPR12jobs（約40分）対auto約90jobs（約2時間）** | bors（merge_groupでない） | PR約40分、auto約2時間、1日約10merge、rollup |
| **gleam** | Rust | hosted | rust-cache `key: v1-<target>`、binary artifact→e2e job | 7-way OS matrix | PR/main同じ、docs `paths-ignore` | なし | 30/10 |
| **grain** | OCaml | hosted | `lookup-only` warming jobのesy cache（1つのcache buildが全targetを塞がないため） | 3 OS × {native, js} | PR/main/queue同じ | **あり** | なし |
| **wasmi** | Rust | hosted | rust-cache、cache維持のためnightly固定 | Miri配下だけnextest | 全PRですべて、Miri specはnightly | なし | なし |
| **spacewasm** | Rust | arm含むhosted | rust-cache **`save-if: main`** | matrix | Miri integrationはdaily限定 | なし | なし |
| **plumbum / xonsh / zod / sh** | Python / TS | hosted | uv cache／なし | matrix | PR/main同じ | なし | なし、xonshはtest別 `--timeout=600` |
| **zig** | Zig | **100% self-hosted**特殊target | 意図的になし | 16並列jobs | riscv64 push限定、cancelはPRだけ | なし | 120–600分timeout |

Almideに重要な横断的事実：

- **nextest `--partition` を使う比較先はaverだけ**。しかも単一test *case* が支配する場面だけで、Almideの巨大targetと同型。`test-groups`、`slow-timeout`、archiveはAlmide自身以外使わない。
- **「一度build、artifactで配布」が共通pattern**（vibe、roc、gleam、grain、Almideの `almide-linux` + nextest archive）。vibeが落とし穴を測定：job時間37%減だが、各consumerがartifact downloadとrunner queue遅延下の `needs` hopを払うためwall時間は増加（job開始中央値365 s）。
- **長いsuiteを持つ比較先が実際に動かした主レバーは段階gate**：aver（cert330分→PR5-lane smoke）、lean4（PR check-level0）、roc（PR minici、nightly全suite）、rust（PR12jobs対auto90）。全て、ある種の失敗がPR後（queue/nightly/bors）にしか見つからないことを受容。suite2–5分のvera、vibe、zod、suiは全場所ですべて動かすが、その方法は転用できない。
- **大型／self-hosted runner**を使うのはrustとlean4のみで、両者hosted fallback付き。hexaのself-hosted darwin poolは警告例（2.7分jobにqueue中央値168分）。
- **必須aggregator job**（vibe `ci-required`、wado `test`、hexa `selfhost-gates-summary`、roc `finish`、lean4 `all-done`）は「matrix改名で黙って必須checkから外れる」問題へのほぼ共通解。
- **全jobのtimeout**（wado全60、jacquard job別文書化、roc shard別）。Almide ci.ymlの `timeout-minutes` はWindows/cross/coverageだけ。巨大targetのhangは既定360分とrunner slotを消費。
- CI時間を記録する比較先はrust（PR約40分、auto約2時間）とvibe（5分予算）以外ない。rustはcompilerの40分PR laneが普通とされる例。vibeは各fixture compileが1–6 sの495-test compile-per-test batteryのコストを示し、Almide corpusとfixture単価が近い。

<a id="2-almides-ciyml--job-inventory-duplicates-fan-out-candidates-shardability"></a>

## 2. Almide ci.yml — job棚卸し、重複、fan-out候補、分割可能性

<a id="21-jobs-name--needs--what-it-does"></a>

### 2.1 Job（名前→needs→処理）

| job | needs | 処理 |
|---|---|---|
| changes | — | docs-only分類（0.1 min） |
| ratchet-separation | — | commit単位ratchet/impl分離（0.2） |
| build | — | `cargo build --release`、nextest archive（`ci-test` profile）、rustc + clippy ratchet、`almide-linux` + `almide-tests-archive` upload |
| shard-coverage | changes, build, test-rust-solo | shards ∪ solos == 全targetを証明、ledger側はbinaryを分割、「全solo合格」のproxy |
| test-rust ×4 | changes, build | 当該shardのbinary id上でarchive再生（`scripts/ci-test-shard.sh` のpacker、重みfile） |
| test-rust-solo ×6 | changes, build | 各巨大target1つ、同じ準備（toolchain、nextest、archive、wasmtime v47、apt binaryen） |
| almide-gates | changes, build | registry再生成（`cargo build -p almide-codegen`）、parity goldens、`almide test spec/ --target rust`、examples、fallback ratchet、wall corpus、reachability ratchet（485 s）、WAT audit（`cargo build --example`）、dogfood、README block、frees churn |
| test-arm | changes | macos-14 NEON test（0.3） |
| test-wasm | changes, build | `almide test spec/ --target wasm`（0.5） |
| test-wasm-windows | changes | Windows parity subset（6） |
| wasm-host-determinism | changes | wasm32 libの `cargo check` + native/wasm32-wasip1 harness build + fixture loop（8.6） |
| wasm-browser-determinism | changes | wasm-pack harness + node fixture loop（11.4） |
| lean-proofs | — | sorry/axiom grep、`lake build` 3回、conformance再生成（0.7、cached） |
| coverage-ratchet | changes, build | crates/src/runtime/spec/stdlib変更以外は関連性判定でskip。それ以外は約14分instrumented rebuild |
| checks (Emit & Format) | build | binary/repo上の約45 shell gates（7.6） |
| perf-ratchet | changes, build | valgrind + wasmtime、ratio gates3本（6） |
| build-cross / corpus-cross / test-rust-cross | main向けPRのみ | macOS/Windows |
| commissioned-wasm-gates | changes, build | `cargo test --release` compile2回 + script gate3本、逐次（27.8） |
| commissioned-aviation-quality | changes | codopsy A + 800行上限（7.4） |
| commissioned-mutation-gate | changes | PR限定：増分scope、function-overlap mutant優先、記録済みkiller suite優先、延期した残りを明記する28分予算（#2814）。対象mutantなしなら0.5。全sweepはdevelopの `mutation-sweep.yml` |
| trigger-playground | main pushのみ | |

全runのrunner slot数：4 shards + 6 solos + 他15jobs ≈ **25 jobs**。queueの `max_entries_to_build: 5` は最大5並行queue runsにPR runsとdevelop push runを加えるため、標準runner同時実行上限（Free org20、Team60、当tokenではorg plan読取不可）に対し150以上のjob slotとなる。29–37分待ちはこれであり、個々のjobではない。

<a id="22-work-done-more-than-once"></a>

### 2.2 複数回行われる処理

| 重複処理 | 場所 | コスト |
|---|---|---|
| **queue mergeごとにdevelop上の全 `push` runも発火**（本日10:06のrun 4a3d9dd4bも1例） | `on: push: branches: [main, develop]` | mergeしたPRごとに25-job run全体が追加。runner slot圧迫の最大単独要因。queueの `merge_group` runは同じtreeを試験済み（REBASE、group merge時のtree==develop tip）。§4の注意参照 |
| **corpus tableをrunあたり3回構築**：`corpus.rs::build_corpus` は全722 fixtureのnative + wasm + wasm-opt + interpを**無条件**buildし、`wasm_runtime_cross_target`、`_opt_parity`、`_interp_oracle` がそれぞれ独自copyを作る。opt-parityはnative/interpを読まず、cross-targetはwasm-opt/interpを読まず、oracleはwasm-optを読まない | `tests/wasm_runtime_test_parts/corpus.rs:51-125` | runner時間約3 × 1100–1400 s、その約半分は読むgateが捨てる側（側別未測定。nativeはfixtureごとのrustc compileで高コスト） |
| **interp側をさらに3回計算**：両ledger tests（`interp_abstain_ledger`、`interp_bridge_fallback_ledger`）が全corpusに `run_interp_capture(_with_fallbacks)` を呼び、oracleもtable内で同じ側を計算 | `tests/wasm_runtime_interp_ledger.rs:76-84, 273-284` | 1 sweepのconsumer2つに1666 + 1586 s |
| wasmtime install | 14jobs（v47.0.3 cached tarball）+ **別pin v27.0.0** の2jobs（almide-gates、commissioned） | 各約10 s、pin分裂はコストより正しさの懸念 |
| `apt-get update && apt-get install binaryen` | 4 shards + 6 solos + almide-gates + checks = 12 jobs | 各約30 s、runあたり約6 runner-min、flakyなのは `apt-get update` 側 |
| `get.nexte.st/latest` からの `cargo-nextest` install（**未固定**） | build、4 shards、6 solos、shard-coverage = 12 jobs | 各約5 s、run途中のnextest releaseで `build` とconsumerのarchive形式が不一致になり得る |
| `almide-tests-archive` download | 11 jobs | 各約35 s |
| rust toolchain install | 12 jobs（archive consumerには `cargo nextest` shimのためだけ必要） | 各約20 s |
| release-profile workspace compile | commissionedは6 cratesのtest compile（実行込み679 s）、続いてrootのrelease-only3tests（実行込み436 s）。coverage-ratchetはinstrumented compile、determinismはharness crate compile（cache keyがemitter hashでMIR PRごとmiss） | critical path上にあるのはcommissioned compileだけ |

<a id="23-jobs-that-could-run-on-merge_group-only"></a>

### 2.3 `merge_group` 限定にできるjob

「PR作者はqueue前には結果を活かせない」という基準の候補：`commissioned-wasm-gates`（27.8）、`wasm-browser-determinism`（11.4）、`wasm-host-determinism`（8.6）、`perf-ratchet`（6.0）、`test-wasm-windows`（6.0）、`commissioned-aviation-quality`（7.4）。ただし§4項目6参照。GitHubはPRで必須でないcheckをqueueだけ必須にできないため、queue-only laneにはaggregatorが必要。またrepoの記録済み選好（commit eb48f630b「本日それぞれCI1巡を費やす2つのgate」）はPR laneのgateを減らすのではなく追加すること。網羅のため掲載するが順位は低い。

<a id="24-independent-steps-inside-commissioned-wasm-gates"></a>

### 2.4 `Commissioned wasm gates` 内の独立step

5 stepが共有するのはrelease target dirだけ：

- A. `cargo test --release -p almide-wasm -p almide-wasm-run -p almide-spine -p almide-syntax -p almide-corpus -p almide-layout` — 679 s。
- B. `cargo test --release --test embedded_cross_test --test module_type_repr_test --test wasm_vm_parity_test` — 436 s（root crate release-test compile + run）。
- C.（#4023以降削除）double-free trap sweepはtrap有効で `backend_parity` と `gauntlet` を再実行していた。76 s。現在は全wasm buildでtrapが入り、Aもその下で走る。
- D. `scripts/check-target-availability.sh` — 426 s、download済みbinaryのみ必要、cargo不要。
- E. `scripts/check-wasm-runtime-ratio.sh` — 25 s、binaryのみ。

D+Eはcompile依存がなく、今すぐ別jobまたはbinary/wasmtimeを既に持つ `almide-gates` へ移せる。A+CとBは各自release compileを払う2jobsに分割可能（rust-cacheがdepsを戻し、workspace cratesは再compile）。wall：27.8 → max(A+C≈13.5, B≈8.5, D+E≈8.5) ≈ **13.5分**。runnerコストはrunあたり約8分増。

<a id="25-are-the-six-giants-fixture-range-shardable"></a>

### 2.5 6つの巨大targetはfixture-range分割できるか

6つともcorpus列挙は同じ：`read_dir(spec/wasm_cross)`（722 `.almd`、`spec/wasm_fail` 4）、`sort_by_key(path)`、1つの `for` loop、結果を `Vec` に集め末尾でassert。threadをspawnするものはない。現在のpartition hook：

- `corpus.rs:78` は `ALMIDE_CORPUS_FILTER=<substring>` を尊重（stemでretain）。開発用loopで、明示的に「CIでは設定しない」。
- `wasm_runtime_interp_ledger.rs` と `crates/almide-spine/tests/run_parity.rs` にはfilterなし。

range/modulo partition（sort後に `ALMIDE_CORPUS_SHARD=k/N`：`entries.retain(|(i, _)| i % N == k)`）は3箇所に約10行の変更（corpus.rs、2つのledger loop、run_parityのmanifest loop）。**部分corpusのgateごとの健全性**：

| gate | fixture単位assert? | partitionで壊れるcorpus全体assert |
|---|---|---|
| wasm_cross_target_spec | あり（fixtureごとequal/allowed/stale/failed） | なし |
| wasm_opt_parity_spec | あり | 「全て `wasm_opt.is_none()` → skip」guard、shardごとでも問題なし |
| interp_cross_target_spec | あり | なし |
| run_parity | あり | **`n_unsupported <= ceiling`、`n_fuel <= ceiling`**（manifest全体のshrink-only件数）と `manifest.len() > 550`。shardでは「shard合計≤ceiling」に変える必要（count artifact出力、coverage jobで合算）。さもなくばN個の部分countにceilingをN回適用し、黙った緩和になる |
| interp_abstain_ledger | observed↔ledgerの集合一致、**双方向** | **stale検出**：ledger掲載fixtureが当shardにないと「もう棄権しない」扱いで偽の赤（corpus.rs:74-77のコメントもfilterについて警告済み）。修正：ledgerをshard fixture集合に制限して比較。新規棄権検出はshardごとに健全 |
| interp_bridge_fallback_ledger | fixtureでなく*名前*（`module.func`）の集合一致 | 当shard外のfixtureだけが到達する名前はstale扱いで偽の赤。shard単独で判別不能。比較前に全shardの観測名集合を集約（artifact + combine job）するか、分割しない（1586 sが新下限）か、§4項目2で高速化する |

coverage証明（repo規則：fixtureを落とすpartitionは高速でなく赤であるべき）：shardのstemを出す `--list-corpus k/N` modeを設け、coverage jobで `∪ shards == ls spec/wasm_cross` と重複なしをassert。`Test shards cover every target` と同型。このstepなしのrange分割はまさに「試験量を減らして速いgreen」の失敗であり、出荷してはならない。

**job別推定**（準備S≈120 sはshard job開始→`Cargo
tests`開始の実測。native warm-up W≈60–120 sは*推定*で、fresh `/tmp/almide-run` の最初の `almide
build` がruntime depsをcompile。ledgerとrun_parityにはWなし）：

| 巨大target | solo (s) | N=2、jobあたり | N=4、jobあたり |
|---|---|---|---|
| interp_abstain_ledger | 1666 | 833 + 120 ≈ **16 min** | 417 + 120 ≈ **9 min** |
| interp_bridge_fallback_ledger | 1586 | 793 + 120 ≈ 15 min（集約比較がある場合のみ） | 397 + 120 ≈ 8.6 min |
| wasm_cross_target_spec | 1377 | 689 + 210 ≈ 15 min | 344 + 210 ≈ 9.2 min |
| run_parity | 1310 | 655 + 120 ≈ 13 min | 328 + 120 ≈ 7.5 min |
| interp_cross_target_spec | 1223 | 612 + 210 ≈ 13.7 min | 306 + 210 ≈ 8.6 min |
| wasm_opt_parity_spec | 1075 | 538 + 210 ≈ 12.5 min | 269 + 210 ≈ 8 min |

巨大targetのcritical pathは5.5（build）+約16（N=2）≈ **21.5分**、または5.5+約9.2（N=4）≈ **15分**。ただし `Commissioned wasm gates`（33分で終了）と小targetのshardにも対処しなければ、それらが下限。solo job数は6→12（N=2）または24（N=4）。N=4ではrun約43jobsとなり、§4項目5/7を先に入れないと§0の容量待ちを悪化させる。最初はN=2が妥当。

**6つの次に大きいtarget**：`diagnostic_harness_test`（合計1501 s、7つの `#[test]`、うち2つは `collect_cases()` のfixture loop）。test別内訳はweights fileになく、1testが約1000 sなら6巨人を外しても担当shard単独でwall約20分。小target shardが約16分と仮定する前に測定する。

<a id="3-github-side-levers"></a>

## 3. GitHub側のレバー

- **大型hosted runner**：Linux4-core $0.016/min、8-core $0.032、16-core $0.064（public repoでも課金、ここでは標準runner無料）。巨大targetは1processでfixtureを1つずつ走らせるため16-coreでも1666 sは短縮せず、#2444で既に競合除去済み。小target shardは `BuildDirLock`（1machineで `almide` native compileは同時1つ）に制限される。大型runnerで得るのは**別の同時実行pool**。移したjobの29分容量待ちが消えるが、4-core大型で約25jobs×約8min×$0.016 ≈ $3–4/run。安い策はjob数削減（項目5、7）。
- **Self-hosted**：hexa darwin pool（queue中央値168分）とrocの無効arm shardsは小poolに否定的な比較証拠。lean4はhosted fallback action付きでのみ採用。推奨しない。
- **`concurrency`**：既に `${{ github.workflow }}-${{ github.ref }}` とcancel-in-progressで、PR pushには適切。ただし次のqueue mergeでdevelop push runもcancel（35565762632は次pushで06:41にcancel）。queue runがtreeを審査した場合に限り問題ない。vera、jacquard、hexa、wadoの4者はcommit別記録維持のためmain runを意図的にcancelしない。Almideの `save-if: develop` rust-cache書込みもcancelで死ぬため、dep cache更新は見かけより少ない。
- **`merge_group` 限定heavy lane**：aggregator経由でのみ可能（項目6）。
- **必須check再設計**：13個の名前付きcontextを1つのaggregator jobに置換（`if: always()`、全gateを `needs:` に指定、非successがあれば失敗。docs-only runでの `skipped` は `changes` でguardされたjobのみ許可）。ci.yml headerが警告するmatrix改名危険、`shard-coverage` のsolo proxy step、実質gate7本が必須でない現状を解消。高速化ではなく強化だが、lane分離の前提。
- **nextest**：全archiveの `--partition hash:k/N` は*test*単位分割（`diagnostic_harness_test` の7testも分散）で構造上完全（`nextest list --partition` のunionでも証明可能）。ただし重みでなく件数で均し、単一testの巨人は分割できないため小targetのpackerだけ置換。`.config/nextest.toml` の `slow-timeout = { period = "…", terminate-after = N }` と全jobの `timeout-minutes` で、現在slotを6時間占有し得る4h20m hangクラス（#1008）を閉じる。archive/test-groupsは使用済みで、jobごとの `--extract-to` も既に最安形。

<a id="4-ranked-plan"></a>

## 4. 優先順位付き計画

削減量は#2444後の基準（約36分＋容量待ち）からのcritical path上のもの。「risk」は変更によってgreenの意味が弱まる可能性を指す。

| # | 項目 | 仕組み | critical-path削減 | risk | 工数 |
|---|---|---|---|---|---|
| 1 | **まず#2444測定、次にcorpus coverage gate付きN=2 range分割** | corpus.rs/ledger/run_parityのsort後に `ALMIDE_CORPUS_SHARD=k/N`、matrix `leg × shard`、`--list-corpus` mode、coverage jobで∪ == `ls spec/wasm_cross` と重複なしassert。run_parityはcount出力、coverageがceilingに対して合算。abstainはledger ∩ shardと比較。bridge-fallbackは**非分割維持または名前を集約** | 巨人job27.8→約16分、**約12分**（36→約24）、#3と同時にのみ実現 | 黙って少ないfixtureを見るshardがまさに失敗。同じPRにcoverage step必須。2つのratchet/ledger gateは集約形がなければ緩む | M（test3file+shard script+coverage job） |
| 2 | **interp sweepをprocess内並列化**（ledger2gateとoracle interp）：sort済みfixtureをscoped thread poolで走らせindexで収集 | backend不要、spawn不要、flockなし、fixture共有なし。`Interpreter` は `Rc` ベースだがthread内に閉じる。staticは `OnceLock`/`Mutex`/`RwLock`（`stdlib_pool.rs:66`、`bundled_sigs.rs:47,107,230`）のみで現在 `Sync` としてcompile。同じ関数を呼ぶ2ledgerも1sweepで計算 | job分割なしの4vCPUで1666→約1666/3≈9–10分。#1 N=2併用でjob約5分。2ledgerを1sweepにするのでrunner分数も減る | `parse_cached`/`bundled_sigs` 内lockが逐次化し速度上限となる（可視、無言でない）。非決定順序でledger textを変えない（index収集、writerも既にsort） | S–M |
| 3 | **`Commissioned wasm gates` をA+C/B/D+Eへfan-out**（commandそのまま） | 3jobs、`needs: [changes, build]` | 27.8→約13.5分。#1併用で下限33→約24 | commandをbyte単位で写し3つ全て必須にすればgate強度riskなし（現job自体必須でない） | S |
| 4 | **各gateが実際に読むcorpus側だけ構築** | binary別const `NEEDED_LEGS`、`build_corpus` が他をskip（opt_parity:native/interp不要、cross_target:wasm-opt/interp不要、oracle:wasm-opt不要） | 側別未測定。nativeはfixtureごとのrustc compileなのでopt_parityはおそらく半減（1075→約500 s）、cross_targetはinterp約1/4減。#1全shardも安くなる | 読まないとする側を本当に比較していない必要。3testの本文が証拠（cross_targetの `l.wasm_opt` 未使用等）。未使用fieldへのcompile-time `let _ = ` 追加だけで不十分、binary別leg集合をassertするtest維持 | S |
| 5 | **queue実行済みjobのdevelop重複 `push` runを止める** | heavy jobに `if: github.event_name != 'push' || github.ref == 'refs/heads/main'`。pushで `build`（rust-cache `save-if: develop` writer）、`changes`、`checks`、leanを維持 | 単runのcritical pathは0、**merge PRごと約25 job slot削除**。29–37分容量待ち最大要因 | developにpush制限なし（API `restrictions: null`、`enforce_admins: false`）なので直接push未試験となる。queue強制のpush制限/rulesetと同時着地必須。`github.event.before` に基づくdocs-only経路も無意味となる | S（yml）+ repo設定 |
| 6 | **必須aggregator check**（`CI required`、`if: always()`）へsolo、Commissioned×2、perf、Windows、coverage追加 | vibe/wado/lean4 pattern | 0分。#1 matrix増大やlane分離の前提で、現非必須gate7本を直す | `skipped` を合格とするなら `changes` guard jobだけ厳密whitelist。`needs`漏れは黙ってgateを落とす。全job名を列挙する `scripts/check-workflow-parse` 型負例control維持 | S |
| 7 | **timeout＋tool固定＋共通setup composite** | 全job `timeout-minutes`（wado全60、巨人45、他20）、nextest `slow-timeout`/`terminate-after`、`cargo-nextest` version固定、12jobの `apt-get update` をbinaryen tarball cacheへ、toolchain+nextest+archive+wasmtime+binaryen共通action、wasmtime pin統一（v27/v47） | setup各job約0.5–1分減、hangは360でなく45分 | なし、pinningは再現性強化 | S |
| 8 | **`check-target-availability`（426 s）と `reachability ratchet`（485 s）を独立jobへ**、#1–#3で下限約20分後 | binary-only scripts | Almide gates15.4→約7.5、下限になる時のみ重要 | 原形ならなし | S |
| 9 | **merge-group-only heavy lane**（Commissioned、determinism×2、perf、Windows、aviation） | job条件 `if: github.event_name != 'pull_request'`、#6 aggregatorが `event_name == merge_group` で `skipped` なら失敗 | #1–#3後はPR lane−0分（既に下限でない）、PR pushあたり−6slots。queue不変 | PRで隠す失敗がqueue1巡（約25分）後に見つかりdequeuing。eb48f630bが先払いしたコストそのもの。wallでなく容量が不満の場合のみ価値 | S、ただしpolicy変更（○×） |
| 10 | **大型hosted runner** | 巨人に `runs-on: ubuntu-4-core`/8-core | 巨人あたり約0分（single process）、別poolでのみ約29分待ち回避、$3–4/run | なし | S、継続費用あり、#5先行 |
| 11 | **shardごとcorpus tableを1回buildし3gate共有**（binary sha+fixture hash keyのartifact、gateがkey確認） | shard3buildを1へ | #4後は捨てる側既に除去、残りnative+wasmの2回→1回（shard約5runner分、table jobがcritical pathでなければ同path削減0） | stale/不一致tableは黙って旧outputを審査。key assertがgateの全て。repo履歴（#2090「怖いwasm破損は無言」）もleg/gate同process維持を支持 | M、最下位 |

**1–4+7後のend-to-end予測**（計算、未実測）：queue引受→changes0.1→build5.5→max(巨人shard≈9–16分、#2でledger≈5–9、Commissioned A≈13.5、Almide gates15.4、小target shard≈16–20で `diagnostic_harness_test` 次第) ≈ **5.5+約16–20 = 22–26分**。今夜66分、#2444後約36分からの変化。さらに下では `Almide gates` と `diagnostic_harness_test` を持つshardが下限となり、#8とtest別測定が必要。burst時29–37分の容量待ちは全てから独立し、#5（run減）、#7/#9（run内job減）、#10（別pool）だけが対処する。

**比較先から転用できないもの**：5分suiteのvibe/vera/zodは作業が小さいためcache warmthやjob開始遅延を最適化。Almideは13 ksの実実行。averの `--partition slice:` が転用できるのは複数test binaryだけ（ledger binary2test、他1）。rust bors/auto段階化とlean4 label段階だけがPR/queue分離の先例で、両者後段検出を受容する。これは本repoがこれまで拒んだtrade-off。

<a id="5-measured-after-the-wave-landed-2026-09-21-develop-run-35633436273-on-83f922680"></a>

## 5. 一連の変更着地後の実測（2026-09-21、develop run 35633436273、83f922680）

新構成で最初のcancelされなかったdevelop push run（batch #2451経由で#2440/#2444/#2445/#2446/#2449全て含む）。1run、1組の数値、予測ではない：

| | 旧develop（35569545602） | 今回 |
|---|---|---|
| wall、push→最後の必須job green | 66 min | **43.4 min**（17:39:49 → 18:23:11） |
| critical path | shard0（59分、ledger2test逐次） | build4.5 → `Test Rust (solo run_parity)`35 → coverage gate3 |
| 他の長いjob | shard1–3≈44/41/35 | coverage ratchet21、browser-ABI determinism15、solo opt_parity/cross_target/interp_ledger各14、shard0 13 |
| shard（小targetのみ） | 44 / 41 / 35 / 33 | **13 / 11 / 7 / 9** |
| Commissioned wasm gates | 27.8（1job） | 13 / 12 / 8（3jobs、#2445） |
| interp ledger | 1666 s + 1586 s逐次 | 1solo、14分（pool上1sweep、#2446） |

同treeのPR run（35625736101）は非必須mutation gateが末尾（44分）となり45.5分、必須greenは38分。「PRをqueueへ入れられるまで」の引用値は**約38–43分**で、末尾はrunner単独のtest `run_parity` となった。#2454（1sweep interp上のfixture-range N=2）はその側を半分にする（local実測303/799 s対非分割1107 s）。計算上約4.5+18+3≈ **26分**、次の下限はcoverage ratchet21とbrowser-ABI determinism15。

どのjobより高くついたため同日に測れたこと：merge-queue REBUILDは置換前refのrunを生かしたままにする（15:25Zに18run、queue105jobs/running44）。queue先頭の必須jobで再試行しない `curl` がGitHub504に遭うと、その後ろ全entryが再開始。#2449はworkflow内から旧runをcancelしnextest/wasmtime/elan downloadをretry。当日の滞留解消はbatch PR1つを1回審査する方式だった。

<a id="51-after-2454-develop-run-35645124173-on-6d38f0164-2026-09-21-19291954z"></a>

### 5.1 #2454後（develop run 35645124173、6d38f0164、2026-09-21 19:29→19:54Z）

| | 今回 |
|---|---|
| wall、push→最後の必須job green | **25.1 min** |
| critical path | build4.5 → `Test Rust (solo run_parity 2/2)`16 → coverage gate2 |
| 巨人、半分ごと（1/2 · 2/2） | run_parity6 · 16、interp_oracle4 · 12、interp_ledger3 · 11、cross_target3 · 3、opt_parity<2 · 2 |
| 次の下限 | `Almide gates`15、browser-ABI determinism14、structural側13 |

1日で3構成、3数値：66→43.4→25.1。分割で各巨人のコストは半減したが剰余類sliceが不均等（run_parity6対16）なのでwallは半減せず。#2457が半分別表とともに引き受ける。#2381はこの測定でclose。

<a id="52-why-the-halves-were-uneven-and-the-balanced-slice-2457"></a>

### 5.2 半分が不均等だった理由と均衡slice（#2457）

全corpus fixtureのfixture別wallをlocalで1回測定（Darwin/arm64、14cpus、`ALMIDE_CORPUS_WEIGHTS_DIR`、非分割。同switch下で各gateが記録するwallから `proofs/corpus-weights.txt` を生成）：

| leg | fixture数 | 合計 | 中央値 | 最大 | 上位20の割合 | modulo半分（1/2 · 2/2） | LPT半分 |
|---|---|---|---|---|---|---|---|
| interp sweep（ledger、oracle） | 727 | 460 s | 2 ms | 65.6 s | **94.6 %** | 110 · **349 s** | 230 · 230 s |
| run_parity（逐次走査） | 729 | 186 s | 25 ms | 56.9 s | 85.0 % | 98 · 88 s | 93 · 93 s |
| builds（cross_target、opt_parity） | 727 | 281 s | 375 ms | 1.4 s | 4.8 % | 142 · 139 s | 141 · 141 s |

つまり剰余の偏りでなくheavy tail。interp側は20fixtureに95%の時間を使う（先頭7つは `string_position_large_offset`、`regex_repetition_depth`、`float_parse`、`matrix_q1_full_loader_cols_beyond_buffer`、`string_interp_large_hole`、`bytes_temp_receiver`、`float_subnormal_preserved`。全てsort後奇数indexで2/2側、半分間3.2倍がCIの3対11分）。build-only側は平坦なのでcross_target/opt_parityは件数で均等。run_parityのtailも同fixtureだがlocalの半分は均等（98/88 s）、CIは6/16分。重いfixtureとその他の比はprogramでなくrunnerのもの（2/2巨人はallocation律速、1/2巨人はCPU律速）。ゆえにcommitする表はCI runner自身の測定から生成する（各solo shardのwallを `corpus-shard-*` artifactへ記録、downloadに `scripts/gen-corpus-weights.sh --render`）。

sliceは記録済み列のLPT partitionとなった（均一重みならmodulo sliceを厳密再現。未記録stemは中央値を使うので表がfixtureを除外できず、coverage stepもunion証明維持）。表からの予測はinterp230/230 s、run_parity93/93 s。着地後のdevelop runをここに記録する。

<a id="53-the-prediction-was-a-laptops-and-the-runner-did-not-honour-it-2502"></a>

### 5.3 予測はlaptop由来でrunnerは従わなかった（#2502）

上表はlocal測定。着地後のdevelopも以前の形を保った：35692820397で `Test Rust (solo run_parity 1/2)`479 s対 `(2/2)`1096 s、35694867589で841/1168 s。予測1:1に対し1.4–2.3倍の偏り。記録比率は単に別machineのものだった。fixture別runner対Mac：

| fixture | Mac | ubuntu-latest | × |
|---|---|---|---|
| `string_position_large_offset` | 38.9 s | 520.5 s | 13.4 |
| `regex_repetition_depth` | 56.9 s | 426.7 s | 7.5 |
| `float_parse` | 6.0 s | 126.5 s | 21.1 |
| `bytes_temp_receiver` | 2.4 s | 57.0 s | 24.1 |
| run_parity列全体 | 186 s | 2008 s | 10.8 |

一定倍率ではない。7 GB runner（#2387）ではallocation律速fixture13–24倍、CPU律速7倍。この比率こそLPT partitionの材料。現在commit表はCI run自身の記録から生成する。全solo jobは既に `ALMIDE_CORPUS_WEIGHTS_DIR` 下の `corpus-shard-*` artifactへ記録（`weights/` subdir内。flat配置だとrun_parity gateの `run_parity` 列が自身の `fixtures` / `counts` partialと名前衝突し、CI downloadからの生成でjunk743行が表へ混入した）：

```
bash scripts/gen-corpus-weights.sh --from-ci [run-id]   # default: latest green develop
```

保持したコードのコメント訳：run-id省略時は最新のgreen develop。

表の `# measured-on:` は元runを示す。35694867589から生成すると同wallを **1004/1004 s** に再分割でき、*別run*の測定wallへ表を再生すると844/730 s（1.16倍）。同じrunnerのrun間なら比率が持ち越せる。Mac表にはなかった性質。

週次ratchet（`shard-balance.yml`、`gen-corpus-weights.sh --check`）は10solo jobsを自身のlogから測定し偏りで失敗させる。test-shard側と同じ。sliceが制御するものに基づき各legを判定：

- **逐次leg**（`run_parity`、`cross_target`、`opt_parity`）：列の均等分割はwall均等分割なのでmax/min≤1.4。
- **thread-pool leg**（`interp_ledger`、`interp_oracle`）：sweepは `available_parallelism` で走るためwallは全作業完了時間。1fixture（`string_position_large_offset`、leg1657 s中492 s）がどんなpartitionでも動かせない下限。LPTは既に2巨人を分け、半分は約1.8倍差（予測492/272 s、実測593/286 s）。ここに比率目標を課すと再sliceで直せない理由で永久赤になるため、代わりにmax wall≤1.6×下限で判定。巨人を隠せなくなったsliceを捕まえ、レバーはfixtureを安くすることだと示す。

また `# measured-on:` が `ci` を示さない間は判定そのものを拒む。「表がlaptop由来」としか言わない赤は誰も対処できないため、偏りと更新commandを表示し、CI測定表がcommitされるまでgreenを保つ。
