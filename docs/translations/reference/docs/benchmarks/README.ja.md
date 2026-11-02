<a id="what-is-measured"></a>

# 測定しているもの

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](README.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](README.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](README.zh-CN.md)

> 固定原文の非規範読者用スナップショット。原文を正本とし、自動同期・自動生成しない。機械処理への入力ではない。benchmark、proof check、CI の独立再実行は行わない。歴史的生成 marker は可視の metadata としてのみ表示し、そのコマンドは実行しない。
> [Original source (English) / 原文（英語）](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/benchmarks/README.md)

> 原文注記：日付、件数、機種・モデル条件、実装や CI 状態は歴史的な原文の主張であり、現在の保証ではない。原文自身が未解消の contract-ledger 不一致を記録する。core 成果物の証明書検査は完全なメモリ安全性や native 機械語の証明ではない。自己完結 Wasm も import に対応する host を必要とし、GC なしという記述は参照 counting 不在を意味しない。過去の999関数/44moduleと9月38課題スコアカードは、他ページや後続 run と整合させず保持する。下の STATUS taxonomy への言及は、本抜粋が分類を列挙していなくても原文どおり残す。


このページの主張はすべて、script から導出するか測定日を付す。`scripts/check-readme-numbers.sh` は CI で裸の数値を拒み、90 日より古い、または取得元の almide-dojo run を示さない LLM-writability スコアカードも拒む。`generated` marker 間の表は、この directory の commit 済み ledger（`*.txt`）と `proofs/` から生成する。生成 block と ledger が一致しなければ CI は失敗する。手で編集してはならない。marker 外の prose の変更は CI の文書 check のみを実行する。内側や marker 自体の変更は full lane を実行する。方法と benchmark ごとのスコアボードは [docs/project/BENCHMARKS.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/project/BENCHMARKS.md) にある。

<a id="llm-writability"></a>

## LLM による書きやすさ

[almide-dojo](https://github.com/almide/almide-dojo) が 2026-09-22 に 38 課題（basic / intermediate / advanced）を、固定コンパイラ `almide 0.62.0` と同 repo の CI lane で測定した。両 run は harness によって **`comparable`** と記録されている。予定したすべての課題がモデルに到達したので、率は区間ではなく点である。両方とも固定 seed（`20260922`）と temperature 0 で sample し、provider が実際に送信した値として run の manifest に記録した。run は commit 済み — [`almide-dojo@8af34bc`](https://github.com/almide/almide-dojo/commit/8af34bc3) — なので、以下の表は信じるのではなくその `summary.md` から再計算できる。後続 run は[ライブ dashboard](https://almide.github.io/almide-dojo/)にある。方針として CI に Anthropic や OpenAI の key は入れないため、ここにあるのは key なしで lane が到達できるモデルである：

| モデル | 合格率 | 1-shot（初回）合格率 |
|---|---|---|
| Llama 3.3 70B (fp8-fast) | 65% (25/38) | 39% (15/38) |
| Llama 3.1 8B | 44% (17/38) | 34% (13/38) |

最新の同一モデル比較は MiniGit bench である。2026-07-15 に almide 0.29.0 で Sonnet 5 × 20 試行（後のコンパイラでは再実行していない）、合格 100%、5 言語中最も簡潔（233 LOC）。agent の実時間は Gleam と MoonBit より速いが Rust と TypeScript より遅い（573 s 対 297 s）。これは 6–9× の自己並列実行下で測った LLM-writability の数値であり、生成 code の速度では**ない**（[chart](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/figures/lang-bench-snapshot-2026-07.png) · [方法](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/research/benchmark/lang-bench/README.md) · [upstream](https://github.com/mame/ai-coding-lang-bench)）。

<a id="byte-identical-across-targets"></a>

## Target 間でバイト単位に同一

**Cross-target の観測は各契約の範囲内で比較する。** バイト比較は入力と host の仮定が一致する条件での実行出力を対象とし、成果物のバイトではない。乱数、platform 報告、圧縮出力、oracle 相対 deadline には、無制限の等価性ではなく宣言された不変条件や比較条件がある。非対応操作の拒否は parity run の成功ではない。[範囲](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/design/EQUIVALENCE.md)を参照。

この主張は prose だけではない。観測可能な約束はすべて[挙動契約 ledger](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/contracts)内の名前付き契約であり、実行可能な証拠へ追跡できる。以下の数値は ledger から再生成する（`scripts/gen-claims.sh`、CI の `scripts/check-contracts.sh` で強制）：

原文の claims 生成 block 開始：docs/contracts/contracts.toml から scripts/gen-claims.sh が導出。原文の指示：marker 間を編集しない。

> 原文の counts 生成 block 開始（2026-10-21 時点）：proofs/ledger-counts.toml の stamp 済み合計。更新は scripts/gen-ledger-counts.sh のみで、fixture/contract PR では行わない。原文の指示：marker 間を編集しない。

> **Ledger：388 契約 — active 388、改訂対象フラグ 0。**

> 原文の counts 生成 block 終了。

>
> **改訂対象の契約：なし。** 全契約は `active` と記録され、ledger gate は class >= `fixture` の実行可能な証拠を要求する。
> この status は範囲付きの比較条件を取り除かず、すべてのプログラムの等価性も証明しない。
> [C-189](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/contracts/contracts.toml)（platform 報告）、[C-208](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/contracts/contracts.toml)（oracle 相対 deadline）、[C-331](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/contracts/contracts.toml)（leg 依存の圧縮バイト）参照。

原文の claims 生成 block 終了。


**Ledger 整合性注記：** `active` status は全契約の記述が一致する証拠ではない。C-189 は依然として platform 依存の例外を 2 つだけとし、`env.cwd` に Wasm wall があるとする一方、C-227 は `env.cwd` を提供済み family に含め、C-331 は leg 依存の圧縮バイトを宣言する。これらの記述は[固定 contract ledger](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/contracts/contracts.toml)に残る。本ページはその規範上の不一致を解消しない。整合化は契約、pin、生成 count を黙って書き換えず、[judge-first 手順](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/scripts/check-als-pin.sh)に従わなければならない。

範囲、ledger の仕組み、証拠 stack（contract ledger、cross-target fixture gate、differential fuzz、emit-time Σ-probe、Lean belt、組織全体の byte-verify sweep）：**[docs/design/EQUIVALENCE.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/design/EQUIVALENCE.md)**。

<a id="memory-safety--proven-where-it-is-proven-trusted-where-it-is-trusted"></a>

## メモリ安全性 — 証明した部分は証明、信頼する部分は信頼

ownership 注釈、lifetime、`free` を書く必要はない。コンパイラが Perceus 方式の ownership 操作を配置する。Rocq/Coq checker が証明するのはモデル化された witness の性質であり、完全な成果物のメモリ安全性や意味上の正しさではない。proof spine は監査済み定理を 137 本持ち、`proofs/check.sh` はこの count を `Print Assumptions` の audit と照合し、spine を独立に再検査する。`almide verify <file.almd> --emit <bundle>` は今は構造化 Wasm bundle を記録し、独立した `almide-verify` がそれを検査する。対応する **core 成果物**は指定するか既定 path に見つかれば SHA-256 で結び付けられる。component 成果物はこの証明書の経路では対応しない。native MIR witness は別の proof/gate 入力のままであり、全 native 機械語を検証した証拠ではない。

記録側への信頼、モデル化していない access、coverage の限界は残る。differential corpus と semantic-mutation gate は補完的な挙動証拠を提供する。`proofs/check-structural-bytes.sh` は構造化 runtime と Coq decoder model を照合する。[証明済みと信頼の区別](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/contracts/proven-vs-trusted.md)と[メモリ安全性 gap 表](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/design/MEMORY-SAFETY.md)参照。

<a id="performance"></a>

## 性能

VM なし、GC なし、interpreter なし。native は Rust を通じて機械語へコンパイルされ、WASM は自己完結した module として直接生成される（プログラムが到達する runtime support をリンクし、残りは除去する）。

原文の wasm-size 生成 block 開始：docs/benchmarks/wasm-size.txt から scripts/gen-readme-stats.sh が生成。原文の指示：marker 間を編集しない。

| プログラム (`almide build --target wasm`, 出荷状態) | develop | release 済み v0.68.0-rc2 |
|---|---:|---:|
| Hello, world | **325 B** | **325 B** |

develop 列は v0.68.0 tag 前の 437b8f127 の develop build（`almide --version`：`almide 0.68.0 (dev, 437b8f127)`）で、2026-10-20 測定。CI は push ごとに再 build し、`docs/benchmarks/wasm-size.txt` の stamp 更新なしにバイトが変わると失敗する。released 列は v0.68.0-rc2 release asset のコンパイラ（`almide 0.68.0 (release, fcba2bcd5)`、`gh release download v0.68.0-rc2 -R almide/almide`）で、2026-10-20 測定。出荷バイトは後処理 optimizer に触れない（`--wasm-opt` は opt-in で、その出力は renderer 自身の module ではない）。

原文の wasm-size 生成 block 終了。


同じ wasm target の Rust は、サイズを最大限調整しても Hello, world で 37 KB 以上となる。`rustc 1.99.1`、`wasm32-wasip1`、`opt-level="z"`、`lto`、`strip`、`panic="abort"`、`codegen-units=1` で 37,541 B（既定 release profile では 62,405 B）。2026-10-20 に GCE c4-standard-8 上で [WASM-OUTPUT.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/wasm/WASM-OUTPUT.md) 末尾の手順により測定した。native の CLI 型 benchmark（`research/benchmark/perf/native/cli_app.almd`、`research/benchmark/perf/native/measure.sh` が行う build）は、x86_64 Linux（GCE c4-standard-8）で strip 済み 507,960 B、aarch64 Linux（GCE c4a-standard-8）で 463,720 B。2026-10-20 に v0.68.0-rc2 release binary で build し、crate 依存はない（C library — `libc`、`libgcc_s` — だけをリンク）。バイトごとの分析：**[docs/wasm/WASM-OUTPUT.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/wasm/WASM-OUTPUT.md)**。

Almide 側と同じ flag — release profile と native `--release` build が使う target CPU（x86_64 は `x86-64-v3`、aarch64 は generic CPU。`almide build --print-release-rustflags` で表示）— でコンパイルした手書き Rust に対し、`bench.py`、full workload、交互実行 5 回の中央値を比較する。2026-10-20 に v0.68.0-rc2 release binary（`almide 0.68.0 (release, fcba2bcd5)`）で GCE c4-standard-8（x86）/ c4a-standard-8（arm）上で測定した結果は、n-body **0.94×** / **1.02×**、spectral-norm **1.00×** / **0.99×**、fasta **1.16×** / **1.10×**、FFT **0.88×** / **0.94×**、wordfreq **0.73×** / **0.59×**（ratchet 用の 8M 回抽出。2M では起動ノイズ下限 0.08 s を下回る）、listbuild **0.87–0.89×** / **0.93–0.95×**（[スコアボード](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/project/BENCHMARKS.md#runtime-scoreboard)）。effect system が証明する、全寿命が 1 つの `check(make(depth))` 式である木のように、Rust が持たない情報を Almide が持つ場合は、同じプログラムの通常の Rust より速い：

原文の native-victory 生成 block 開始：docs/benchmarks/native-victory.txt から scripts/gen-readme-stats.sh が生成。原文の指示：marker 間を編集しない。

| Workload（処理） (`bench.py` 方式、交互実行9回中央値) | 最適化 | Almide / 通常の Rust, x86 | Almide / 通常の Rust, arm | 無効時 (`ALMIDE_REGION_OFF=1` / `ALMIDE_FAN_SEQUENTIAL=1`), x86 / arm |
|---|---|---:|---:|---:|
| binarytrees | region window（領域単位の処理区間） (#1991) | **0.28 (d17)** / **0.27 (d19)** | **0.33 (d17)** / **0.32 (d19)** | 0.89 / 1.04 |
| treealloc | region window（領域単位の処理区間） (#1991) | **0.22 (d20)** / **0.24 (d21)** | **0.27 (d20)** / **0.28 (d21)** | 0.91 / 1.03 |
| fannkuchredux | 並列 fan (#2044) | **0.25 (n10)** / **0.23 (n11)** | **0.21 (n10)** / **0.18 (n11)** | 0.97 / 1.02 |

2026-10-20 に、main fcba2bcd5 から build した v0.68.0-rc2 release asset（almide-linux-x86_64 / almide-linux-aarch64、`almide 0.68.0 (release, fcba2bcd5)`）で測定。x86 = GCE c4-standard-8（Intel Xeon Platinum 8581C、8 vCPU）、arm = GCE c4a-standard-8（Neoverse-V2、8 vCPU）。参照実装は rustc 1.99.1、release flag は `almide build --print-release-rustflags` の報告どおり（x86 は -C target-cpu=x86-64-v3、両方 -C llvm-args=-force-ordered-reductions=false）。セル内の 2 比率は 2 入力サイズに対応する（サイズも各セル内に記載）。Rust 側は人が書く通常のプログラム — ノードごとの `Box`、1 thread、arena なし、`unsafe` なし、SIMD なし — を Almide 側と同じ `rustc` flag でコンパイルしたもの。「無効時」列は同じ Almide source の当該最適化だけを無効にし、大きい方のサイズで測定した値。`perf-ratchet` job は行が 1.0 に達するか ablation の効果がなくなると失敗する。宣言と方法：[docs/project/BENCHMARKS.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/project/BENCHMARKS.md#faster-than-ordinary-rust-1330)。Ledger：`docs/benchmarks/native-victory.txt`。

原文の native-victory 生成 block 終了。


原文の build-speed 生成 block 開始：docs/benchmarks/build-speed.txt から almide-gates の `bench` subcommand が導出。原文の指示：marker 間を編集しない。

almide 0.68.0 (dev)、x86_64 Linux、INTEL(R) XEON(R) PLATINUM 8581C CPU @ 2.30GHz [GCE c4-standard-8]、`examples/lisp.almd`（297 行）、2026-10-19 で測定。全行は N 回の**平均**である。30ms process の単発実行は scheduler noise となる。cold は毎回 `$TMPDIR/almide-run` と dependency cache の**両方**を消去する。後者だけを消すと warm build を測ることになる。`almide run tools/almide-gates/src/main.almd -- bench` で再生成する。ratchet（`-- bench docs/benchmarks --check`）は上記機種 class で 1.5x に達すると失敗し、それ以外では明示して skip する。

| 条件 | 時間 | 回数 |
|---|---|---|
| `almide check` | **25.2 ms** | 20 |
| build、warm（content-cache hit） | **92.9 ms** | 5 |
| build、cold | **4846.1 ms** | 3 |
| build、cold, `--target wasm` | **148.7 ms** | 3 |

原文の build-speed 生成 block 終了。


`almide check` はほぼ線形に伸びる。この repo 自身の stdlib の 2k → 33k 行の階段で、project 行数に対する check 時間の log-log 傾きは両機種 **1.10**（1.0 が線形、2.0 が二次）であり、10.7k 行段階は空 project 下限の **4.2×**（x86）/ **4.3×**（arm）のコストとなる。2026-10-20 に v0.68.0-rc2 release binary を GCE c4-standard-8 / c4a-standard-8 上で測定（`research/benchmark/editloop/scale.py --runs 40`）し、`scripts/check-edit-loop-scale.sh` で維持。表は [BENCHMARKS.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/project/BENCHMARKS.md#edit-loop-scale-1334)。`perf-ratchet` job（`scripts/check-perf-ratio.sh`）は develop CI ごとに全 native 行を再測定し、anchored 行（n-body、spectral-norm、fasta、FFT、wordfreq）が commit 済み baseline を 40% 超えて悪化すると失敗する。測定・gate 済みの Wasm runtime（#1701）：

原文の wasm-runtime 生成 block 開始：docs/benchmarks/wasm-runtime.txt から scripts/gen-readme-stats.sh が生成。原文の指示：marker 間を編集しない。

| Benchmark (`almide bench`, 検証後に計時、交互実行2×5回の最小値) | wasm/native, `main` のみ | cold start（起動 対 compile + instantiate） |
|---|---:|---:|
| nbody | **1.21×** | 1.41× |
| spectralnorm | **1.18×** | 1.27× |
| binarytrees | **1.20×** | 1.34× |
| treealloc | **1.08×** | 1.13× |
| fasta | **0.90×** | 1.00× |
| fannkuchredux | **1.37×** | 1.48× |
| mandelbrot | **1.21×** | 1.30× |
| onebrc | **1.90×** | 2.56× |
| fft | **1.42×** | 1.52× |
| strchurn | **0.92×** | 0.95× |
| listbuild_append | **1.77×** | 1.98× |
| listbuild_combinator | **1.55×** | 1.74× |
| listbuild_prealloc | **1.36×** | 1.53× |
| mapbuild | **0.92×** | 1.08× |

embedded wasm host（linear memory の Perceus RC）と native binary を、同じ機種・同じ実行で比較する。比は両側のプログラム自身の `main` の入口から復帰までを計る（native は process 内、wasm は host call の前後）。process 起動と module compile/instantiate はその外であり、cold-start 列がそれらを示す（#2980）。小さい workload は `main` が測定に十分長くなるよう ledger 固定サイズ（`args=`）で実行する。cross-engine 比では hardware は相殺**されない**（2-core CI runner は nbody が約 10x 悪化）ので、stamp 済み比率の判定は stamp 元機種 class で行う。CI は下の STATUS taxonomy を gate し、wasm 側を同じ runner 上で最新 release binary と A/B 比較する（交互実行、各 run の最小値、ledger の `ab_band` — #2143）（`scripts/check-wasm-runtime-ratio.sh`）。これらの行で `fan` を並列に実行するのは fannkuchredux だけである。native は thread（#2044）、embedded wasm host は module の別 instance（#3003、ADR-0011 §D2a）を使い、それ以外のすべての wasm host ではバイト同一な逐次実行となる。binarytrees と mandelbrot の `fan.map` callback は両側とも逐次実行（両側で user time と wall time が同じ、2026-10-03 測定）。未測定 corpus cell は推測せず正直に残す。wasm build path の wall 0、embedded heap を使い尽くすもの 0（#1729）。gate 実行ごとに再測定し、計時可能になった cell は行を昇格するまで gate を失敗させる。Ledger：`docs/benchmarks/wasm-runtime.txt`（version 0.68.0 を持つ develop build であり 0.68.0 release ではない。2026-10-19、x86_64 Linux、INTEL(R) XEON(R) PLATINUM 8581C CPU @ 2.30GHz [GCE c4-standard-8] で測定）。

原文の wasm-runtime 生成 block 終了。


<a id="derived-counts"></a>

## 導出された件数

[Project status](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/status.md) の根拠となる stamp 済み合計と、gate net の mutation score。

原文の stats 生成 block 開始：docs/stdlib/*.md、spec/、docs/contracts/contracts.toml から scripts/gen-readme-stats.sh が導出。原文の指示：marker 間を編集しない。

原文の counts 生成 block 開始（2026-10-21 時点）：proofs/ledger-counts.toml の stamp 済み合計。更新は scripts/gen-ledger-counts.sh のみで、fixture/contract PR では行わない。原文の指示：marker 間を編集しない。

| 導出件数 | 値 |
|---|---|
| Stdlib | 999 関数、44 module — self-hosted `.almd`, signature index は次により compiler から再生成： `tools/gen-stdlib-doc-index.py` |
| テスト | 514 `.almd` test file、配置先 `spec/` (`almide test spec/`) + 388契約の cross-target ledger |

原文の counts 生成 block 終了。

原文の stats 生成 block 終了。


原文の mutation-score 生成 block 開始（2026-09-22 時点）：proofs/mutation-score.toml から scripts/gen-mutation-score.sh が stamp。各 mutation-sweep run で再測定し、更新は `--from-run` のみ。原文の指示：marker 間を編集しない。

**Mutation score** — 41/41 mutant を検出（100.0 %）、生存 0、stale 0。`ci/mutations/` の完全な release-shape net sweep（[`scripts/check-mutation-gate.sh`](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/scripts/check-mutation-gate.sh)）を、`3b02f7dc4` の mutation-sweep run [35650396971](https://github.com/almide/almide/actions/runs/35650396971) から 2026-09-22 に stamp した。

原文の mutation-score 生成 block 終了。

