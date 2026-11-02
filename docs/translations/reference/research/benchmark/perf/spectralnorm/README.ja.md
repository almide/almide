<a id="spectralnorm-spelling-comparison-2098"></a>

# Spectralnorm の書き方比較（#2098）

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](README.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](README.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](README.zh-CN.md)

> 固定原文の非規範読者用スナップショット。原文を正本とし、自動同期しない。benchmark の独立再実行、現 CI 確認、実装検証はしない。歴史的測定・提案・指示は元の範囲を保持し、引用コマンドやプロンプトは実行しない。
> [Original source (English) / 原文（英語）](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/research/benchmark/perf/spectralnorm/README.md)


コンパイラをビルド後、リポジトリルートで実行する：

```sh
ctxgate exec python3 research/benchmark/perf/spectralnorm/compare.py --n 1200 --runs 9
```

既存の命令的プログラムを、添字付き fold と enumerate-fold の変種と比較する。異なるのは二つの行列・ベクトル積関数だけ。native/wasm の全 6 成果物で stdout と stderr が一致しなければならない。各成果物を 1 回 warmup し、ラウンドロビンでサンプルを取る。JSON は全サンプル、中央値、命令的形式に対する比、成果物サイズを含む。測定にはプロセス起動、wasm では runtime compilation も含む。小さな入力は smoke test であり、定常性能の証拠ではない。

`--max-ratio R` を指定すると、いずれかの比率が指定予算を超えたとき失敗する。`scripts/check-spelling-ratio.sh` はまさにその CI 版（既定 2.0x、perf-ratchet job）。比率は一つの経路の書き方を一回のラウンドロビン実行で比較するため、共有 runner が遅くても数値が動くだけで判定は変わらない。native/Rust 比率ゲートと同じ規律である。native capture copy の対応が入ったら予算を下げること。失敗ビルドを通すために上げてはならない。このコーパスの baseline は `for` と成長する出力リストを使い、issue の外部実験の事前割当て `while` baseline ではないので、その比率を当該実験の正確な再現として報告してはならない。変種は意図的に同じ反復順と浮動小数の加算順を保持する。

同期 scalar fold は、添字アクセス、明示的借用、clone を通じて読取り専用と証明された capture を借用する。escape する入れ子 closure はない。wasm 経路の scalar enumerate/fold はソースを scalar list にスナップショット化し、callback が tuple を外へ出せない場合、一つの private tuple slot を再利用する。native 経路では SOURCE 位置の `list.enumerate` は現在、次段が走査して drop する `Vec<(Int, T)>` でなく chain adapter（`.iter().cloned().enumerate()`）であり、消費される runtime call に渡していた clone も消える。

`--release` は opt-level 3 と LTO でビルドする。指定しない `almide build` はプロジェクト既定の opt-level 1（正しさに不可欠。`src/cli/cargo_build.rs` を参照）を使う。二つの profile は別の問いに答えるため両方を以下に記録する。既定 profile は通常の `almide build` が出荷するもの、release profile は LLVM がいずれにせよ片付ける部分から lowering を切り分ける。

2026-09-13、ローカル v0.62.0 release build と Wasmtime 47.0.3、n=1200、warmup 後 5 サンプルで測定。全 6 成果物の各実行は `1.274224150` と改行を出力し、stderr は空だった。

既定 profile（`almide build`、opt-level 1）：

| ターゲット | 命令的 | 添字付き fold | enumerate fold |
| --- | ---: | ---: | ---: |
| Native | 41.09 ms | 58.29 ms (1.42x) | 41.66 ms (1.01x) |
| Wasm | 93.20 ms | 118.51 ms (1.27x) | 96.12 ms (1.03x) |

Release profile（`almide build --release`、opt-level 3 + LTO）：

| ターゲット | 命令的 | 添字付き fold | enumerate fold |
| --- | ---: | ---: | ---: |
| Native | 47.95 ms | 48.23 ms (1.01x) | 48.14 ms (1.01x) |
| Wasm | — | 1.13x | 0.91x |

何が変わり、何を意味するか：

- **Native enumerate の 1.68x → 1.01x** は source-adapter fusion による。旧出力は capture した 1,200 要素ベクトルをコピーし、行ごとに `Vec<(i64, f64)>` を作っていた。各々 1 実行 48,000 回で、両方なくなった。
- **Native indexed は既定 profile で 1.42x、release で 1.01x**。理由を示すのは release 列であり、opt-level 3 では三つの書き方が約 48 ms に収束する。indexed に残る差は opt-level 1 で LLVM が iterator chain に対して行わない処理であり、lowering が出力する負担ではない。
- release 列は推奨ではない。同じソースでラウンドロビン測定すると、`--release` は、既定ですでに良好な二つの書き方では遅い（命令的 47.95 対 41.93 ms、enumerate 48.14 対 41.77 ms）。速いのは indexed のみ（48.23 対 59.65 ms）。どの profile で出荷しても書き方のコストは同じであるべきで、このコーパスがゲートする性質はそれである。opt-level 1 の indexed を除き、現在は両 profile で測定ノイズ内に収まる。
- Wasm のサンプルは依然ノイズが多い。ローカルな比較測定であり、移植可能な時間保証ではない。

2026-10-12 に Linux、develop `1df5391`、GCE c4-standard-8（x86_64）と c4a-standard-8（aarch64、Neoverse-V2）、n=1200、9 サンプルで再測定。表は native 経路について、各書き方の release 中央値 ÷ 既定中央値を示す。両 profile を `compare.py --profile-pair` でラウンドロビン計測した：

| アーキテクチャ | 命令的 | 添字付き fold | enumerate fold |
| --- | ---: | ---: | ---: |
| x86_64 | 76.0 / 76.2 ms (1.00x) | 85.3 / 93.3 ms (0.91x) | 76.0 / 76.1 ms (1.00x) |
| aarch64 | 76.9 / 53.8 ms (**1.43x**) | 73.6 / 90.8 ms (0.81x) | 76.9 / 63.0 ms (**1.22x**) |

この aarch64 測定では `--release` は命令的と enumerate で遅く、indexed fold で速い（0.81x）。命令的／enumerate の低速化は書き方の負担ではない。generic aarch64 core 向けにコスト算定された LLVM loop vectorizer が、`eval_a` の i64 乗算を NEON lane に分割する。`-C llvm-args=-vectorize-loops=false` または `-C target-cpu=native` では低速化が消える（#3604）。書き方同士の比率では全書き方が等しく負う負担を検知できない。ゲートの profile 経路は各書き方を二つの profile 間で自分自身と比較する。`perf-profile-arm` CI job が `ubuntu-24.04-arm` 上でその経路を実行する。

2026-10-19 に修正（#3604）。原因は二つで、どちらも一般的なもの：

- **vectorizer が順序付き浮動小数 reduction を取り込んだ。** Almide の `Float` の `+` は strict なので、`sum + eval_a(i, j) * v[j]` は順序付き reduction である。x86 の cost model はこれを vectorize しないが、AArch64 は opt-in し、generic tuning が `eval_a` の除算を NEON lane にまとめる一方、i64 乗算（NEON に 64-bit vector multiply はない）と順序付き加算はその周囲で lane ごとに実行された（hot loop 内の `fdiv v.2d`）。release build は現在、全ターゲットで `-C llvm-args=-force-ordered-reductions=false` を渡す（`src/cli/native_target.rs` の `RELEASE_LLVM_ARGS`）。これらのループは scalar のまま、整数 reduction と reduction のないループは従来どおり vectorize する。x86 では cost model が元から行っていた動作である。
- **fold の先頭の range literal が構築されていた。** `list.fold(0..<n, …)` は行ごとに `Vec<i64>` を実体化し、`Vec` から読んだ添字は範囲を LLVM に伝えないため、`v[j]` の境界検査がループ内に残った。現在 literal は両経路で `list.range(a, b)` と同じように数える（#3810）。収集する chain（`list.map(0..<n, f)`）は構築した head を保持する。どのマシンにも載らない span での失敗は C-197 の out-of-memory abort のまま。

変更前 → 後、GCE、n=1200、21 サンプル（hyperfine 中央値、ms）、全成果物が `1.274224150` を出力：

| アーキテクチャ | Profile | 命令的 | 添字付き fold | enumerate fold |
| --- | --- | ---: | ---: | ---: |
| aarch64 (Neoverse-V2) | release | 76.91 → 53.26 | 69.59 → 57.65 | 76.90 → 54.22 |
| aarch64 (Neoverse-V2) | default | 53.76 → 53.76 | 90.65 → 53.47 | 62.87 → 62.83 |
| x86_64 (Xeon 8581C) | release | 48.17 → 48.13 | 63.47 → 47.52 | 49.88 → 52.30 |
| x86_64 (Xeon 8581C) | default | 60.04 → 60.31 | 70.81 → 59.53 | 59.32 → 58.46 |

x86 enumerate release の機械語は変更前後で同じ（そこで option は無効で、ソースも変わらない）なので、二つの値はこの VM の実行間のばらつきである。aarch64 に残るのは indexed fold が命令的版の 1.08x であること。外側添字を構築済み list から読み、内側を数えると、LLVM は範囲が分かる一つの添字を中心に `eval_a` の `ij * (ij + 1) / 2` を再結合する（外側 head を数えた enumerate でも鏡像的な同じ効果を測定。他の build が一つの `madd` を持つところに `mul` + `sub` の組）。外側 head も数えると indexed は 53.45 ms、enumerate は 62.0 ms となり、収集 chain の失敗形式も変えるため採用しない。

外側 head と enumerate index を数える案は 2026-10-20 に測定し不採用（#3604 の方向 1）。二つの候補 lowering を測定した：

- 収集する range-literal head（`list.map(0..<n, f)`）を、span 分 reserve した単一 vector へ数えながら入れ、C-197 abort を維持する。
- enumerate adapter の index を `.enumerate()` の `usize` から cast せず、`0i64..i64::MAX` と zip する。

Apple M4 Pro（aarch64-apple-darwin、host 既定の `apple-m1` tuning）、`--release`、n=5500、11 ラウンドロビンサンプル、中央値 ms、全成果物が `1.274224153` を出力。「range」は、head がすでに数えられていた `list.map(list.range(0, n), (i) => list.fold(list.range(0, n), …))`：

| Profile | 命令的 | 添字付き fold | enumerate fold | range fold |
| --- | ---: | ---: | ---: | ---: |
| release, develop | 727.8 | 694.1 (0.95x) | 710.6 (0.98x) | 712.0 (0.98x) |
| release, 両 lowering | 730.7 | 709.6 (0.97x) | 707.7 (0.97x) | 714.9 (0.98x) |
| default, develop | 749.1 | 786.1 (1.05x) | 725.7 (0.97x) | 747.0 (1.00x) |
| default, 両 lowering | 748.3 | 751.2 (1.00x) | 709.6 (0.95x) | 747.7 (1.00x) |

Linux aarch64 の `--release` build が使う LLVM の GENERIC aarch64 tuning では、外側を数えるとコストが増える。同じ出力 Rust を rustc の `-C opt-level=3 -C codegen-units=1 -C panic=abort -C target-cpu=generic
-C llvm-args=-force-ordered-reductions=false` でビルドして M4 で実行した（n=3000、9 サンプル、ms）：

- 命令的：238.3。
- enumerate：develop では 189.0、両 lowering では 234.5。
- indexed：develop では 210.4、外側を数える案では 237.8。

enumerate の変更を一つずつ適用すると、zip した index だけでは 188.9（中立）、外側 head を数えるだけでは 215.2 だった。外側ループだけの Rust microbenchmark は、generic tuning で構築済み `Vec<i64>` が 205 ms、`0..n` 上の `extend`、`push`、`collect` は 228 ms、`apple-m1` では 4 形式すべてが 207–212 ms だった。外側 index の範囲が分かると LLVM は内側ループのスケジュールを変え、強度削減した `ij` の `mul` でなく `i + j` の `madd` を使う。

したがって generic tuning で外側 head を数えると、命令的版より 12–20% 速い慣用的書き方をその速度まで下げ、host tuning では何も得ない。zip した index はどこでも中立。どちらも採用しない。1.53x 自体は #3824 で修正された。導入したのは、すでに数えていた chain の失敗形式である。`list.range(a, b) |> list.map(f)` は `.collect()` で収集し、どのマシンにも載らない span では `capacity overflow` で panic していた。現在は `almide_rt_list_range` や wasm 経路と同じく、C-197 abort を備えて reserve した vector に詰める（`tests/counted_index_chain_test.rs`）。Spectralnorm には影響しない。

記録として残す以前の値：wasm scalar enumerate/fold lowering 前、wasm enumerate は 365.81 ms（3.21x）、後は 129.72 ms（1.17x）。native scalar-fold の capture 借用前、native indexed は 97.39 ms（2.36x）、後は 58.28 ms（1.42x）。

> 原文注記：測定は異なる日付・profile・機種にまたがる。初期の既定 profile 表と近くのラウンドロビン説明には異なる値（41.09/41.66/58.29 対 41.93/41.77/59.65 ms）があり、黙って整合させず両方保持する。後続更新や不採用候補の記述は、すべての過去の観測が現在も成立する意味ではない。
