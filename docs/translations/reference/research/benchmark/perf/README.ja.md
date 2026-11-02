# Perf Suite — Native と WASM のランタイムスコアボード（#917）

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](README.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](README.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](README.zh-CN.md)

> [固定原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/research/benchmark/perf/README.md)の非規範的な読者用翻訳です。原文を正本とし、自動同期は行いません。




> この翻訳はソース固定の非規範的スナップショットです（リビジョン：35297a612fb29e3f78f6587aba253aa91d51f466）。自動同期されません。測定値および主張は独立に再検証していません。英語原文を正とします。
>
> 原文上の注意: 本文には過去の計測、未再計測の原因推定、検証・ゲートの未対応範囲が併記されています。数値を全環境や現在の実装へ一般化しないでください。

`bench.py` のスイートには、計算カーネル、縮小版 One Billion Row Challenge、リスト構築の形、文字列の生成・処理などのワークロード行が含まれます。各行は実行する系統と検証モードを宣言します。すべてのワークロードがすべてのターゲットで走るわけではありません。定義されている場合には手書きの Rust 参照実装も含みます。このディレクトリは [docs/project/BENCHMARKS.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/project/BENCHMARKS.md) の数値の出典です。`bench.py` が生成していない数値は公表しません。

## 構成

| パス | 目的 |
|---|---|
| `<bench>/<bench>.almd` | ベンチマークのソース（Almide） |
| `rust-ref/*.rs` | 手書きの Rust 参照実装。native 系統とまったく同じフラグでコンパイル |
| `bench.py` | ビルド + 検証 + 計時ハーネス。`results/` に書き込む |
| `results/<date>-<label>.json` | 日付付きの生の結果（コミット済み） |
| `native/` | README のバイナリサイズ・CLI 測定（`measure.sh`、スコアボードとは別） |
| `string-gap-1004.md` | #1004 の要因分析：文字列の行の差が実際にどこで生じるか |
| `strchurn/ladder.py` | その分析を生成した段階的な差分比較（再実行可能） |

## 方法

- **実行系統**：`native` = `almide build --release`（cargo release プロファイル：opt-level=3、LTO、1 CGU）。`wasm` = `almide build --target wasm` を wasmtime CLI で実行。`rust` = `rust-ref/*.rs` を `rustc -C opt-level=3 -C lto=yes -C codegen-units=1 -C overflow-checks=no` でコンパイル。
- **速度より先に正しさ**：計時の前に、ベンチマークのすべての変種が小さなワークロードでバイト単位で同一の stdout を出す必要があります（fft は 1 行目のみ比較。2 行目は自己申告の時間です）。誤った答えを計算する項目はスコアボード項目とは認めません。
- **計時**：プロセス全体の実時間を測り、stdout は /dev/null へ。ウォームアップ 1 回の後、N 回のラウンドを交互に実行します（変種 A、B、C、A、B、C…）。これによりドリフトが各変種に均等に作用します。代表値は中央値で、JSON には全実行を保持します。
- **静かなマシン**：ブラウザを閉じ、並行してコンパイラをビルドせず、重い処理が同居する間の実行を信用しないでください。Apple Silicon では静かなマシンの実時間測定は P コア上に載ります。ピン留めは試みません。
- **主張に必要なら参照実装も同じ形**：`nbody_unrolled.rs` は nbody.almd の完全展開されたスカラー局所変数を再現し、コード生成のオーバーヘッドを切り分けます。`nbody.rs` は実際によく書かれる天体配列の標準的な形です。Almide は現在、後者を上回ります（境界チェック）。この比較は報告しますが、ゲートにはしません。
- `fannkuchredux` と `mandelbrot` は `fan` で書かれており、#1330 以降は通常の逐次 Rust 参照実装を持ちます（`rust-ref/fannkuchredux.rs`、`rust-ref/mandelbrot.rs`：1 スレッド、`unsafe` なし、SIMD なし）。これは普遍的に同じ並列度での比較ではありません。native の `fan` は条件を満たすコールバックをスレッド上で実行できます（`pass_rust_lowering_fan.rs`）。`fannkuchredux` は現在 VICTORY 行で、`ALMIDE_FAN_SEQUENTIAL` による無効化比較で検査します。`mandelbrot` は REPORTED のままです。Bytes を返すコールバックは、その native の Send-safe な経路の条件を満たしません。以前の ~1.0 という測定値は以前の実装を表し、現在 native fan の並列性がないことを意味しません。
  `binarytrees`（#1991）と `treealloc`（#2028）は同じ形の `Box` 参照実装を持ちます（`rust-ref/binarytrees.rs`、`rust-ref/treealloc.rs`：1 スレッド、ノードごとに `Box` と解放、arena なし、`unsafe` なし）。native では `check(make(d))` がリージョンウィンドウ内で実行されるため、どちらも 1 を下回ります。これらは `check-perf-ratio.sh`（#1330）の VICTORY 行で、アロケータ依存の数値の周囲の ±帯域ではなく、主張そのものと `ALMIDE_REGION_OFF=1` による無効化比較をゲートにします（M4 Pro では 0.32、CI ランナーでは 0.61。listbuild と同様です）。無効化比較と両サイズを含む宣言は `docs/project/BENCHMARKS.md` にあります。
- `onebrc` は縮小版 One Billion Row Challenge（`station;temp` 行 → 観測所ごとのソート済み min/mean/max）です。ホットループが算術ではなくファイル I/O、`string.split`、マップ更新である唯一の行です。温度は終始 10 分の 1 度単位の整数なので、浮動小数点の書式化なしで出力がバイト単位で一致します。wasm 系統は除外します。`wasmtime run` はディレクトリを事前オープンしないため、この系統はファイルに触れません。この行はストリーミング行処理ファミリー（`fs.for_each_line` / `fs.fold_lines`、C-220）の誕生の地であり、監視役です。
  元の eager な `fs.read_lines` の形では RSS がファイルサイズの ~4× でした（2026-08-08、M4 Pro：126 MB / 10 M 行のファイルで 505 MB、632 MB / 50 M で 2.2 GB。同形の Rust 参照実装でも同一なので、壁はコード生成ではなく API の形でした）。公式の 1 B 行ファイルに外挿すると ~50 GB です。現在、集計段階は両系統でストリーミングし、RSS はあらゆる規模で 1 MB に保たれます。最初のストリーミング版には時間の退行がありました（50 M 行で 33.4 s、eager は 15.6 s）。原因は fold のアキュムレータの Map が各行でクローンされることでした。clone パスが相互排他的な match アームにまたがる構文上の使用を数えていたため、分岐末尾が最後の使用になれなかったのです（#1143）。性能改善の全記録（2026-08-08、50 M 行 / 632 MB、M4 Pro）は以下のとおりです。各段階は前段階とバイト単位で同一です。
  eager 15.6 s / 2.2 GB → 最初のストリーミング版 33.4 s / 1 MB（fold アキュムレータの Map を毎行クローン。clone パスが相互排他的な match アームの構文上の使用を数えるため分岐末尾が最後の使用になれない；#1143）→ 兄弟分岐の差し引き修正 + 消費型 `map.set` 15.2 s → `string.split_once` + `map.upsert`（検索 1 回、Vec なし、キークローンなし）8.2 s → `fs.fold_lines_chunked`（ランタイムスレッド上の範囲ワーカー、呼び出し元が部分結果をマージ）**8 ワーカーで 1.27 s / 12 で 0.95 s。同形の単一スレッド Rust 参照実装は 2.58 s**。素朴な fold の記述が構造によって手書きの逐次 Rust を上回り、binarytrees の手法を native で再現しています。スケーリングは実在し（1w 8.5 s → 2w 4.6 s → 12w 0.95 s）、RSS は 2 MB にとどまります。
  正直な注意点：参照実装は意図的に単一スレッドです（手で並列化した Rust なら再び勝ちます）。コアあたりの差（~3.3×）は、行ごとの割り当てを伴う操作群（split_once ×2 + strip_prefix + 行ごとの upsert クロージャ）と `AlmideMap` の検索に帰属されましたが、**この検索は測定当時は線形走査で、現在は違います**。#2150 以降、ハッシュ可能なキーを持ち `ALMIDE_MAP_INDEX_THRESHOLD` = 16 を超えるマップはスロットテーブルのインデックスを持ちます（`runtime/rs/src/map.rs:12`-`31`）。この行は以降再測定しておらず、この要因分析の検索部分は未検証です。また、クロージャがキャプチャする Map（`for_each_line` + `var stats`）は、今も `SharedMut` セルを通じた読み取りのたびにクローンされます。集計は CHEATSHEET が教えるとおり `fold_lines` で行うべきです。改善が続く間、ここの実時間比率は報告対象で、ゲートにはしません。
- `fft-wasm` 行があるのは、wasm 系統がかつて頻繁な `data[i] = x` リスト書き込みで崩壊していたためです（2^18 で ~3 桁）。各格納がブロック全体をコピーしていました（書き込みごとに `$block_copy`、格納 1 回あたり O(n)）。RC-5 の copy-on-write 判定（#1729）が格納を O(1) にし、#2150 が再測定する前に崖は解消していました。develop の埋め込みホストで 2^18 時 4.1× / 2^22 時 4.7×（2026-09-24）。残ったのは毎回の格納で走る判定そのもので、呼び出し、ヒープ境界比較、参照カウント読み取りがバタフライごとに 4 回発生していました。#2150 は、ループが要素単位でしか読み書きしないリストについて、これをループへの進入時に 1 回だけ行います（`crates/almide-wasm/src/cow_hoist.rs`）。結果は 2^18 時 2.7× / 2^22 時 2.9× です。残るのはアクセスごとの処理、つまり中断フレームを伴う i64 の境界比較と、各ロード・ストアで i32 に折り返される i64 のインデックス演算です。`perf_ratchet.rs` の R5（格納の漸近的クラス）と R6（格納・読み取りの同等性）が、両方の形の再発を防ぎます。
- **mandelbrot の wasm の「クレーター」（4000 で ~130×）は再現しませんでした**。#2150 が develop で再測定したところ、`wasmtime run`（このハーネスの wasm 系統）では 4000 で native の 1.13× でした。`almide bench --target wasm` では 2.05× で、差のすべては埋め込みホストの 30 s エポックウォッチドッグによるものでした。エポック割り込みは wasmtime に各ループ先頭でエポックを検査させ、脱出時間ループは 1 反復あたりわずか数命令です（同じモジュールを `wasmtime run -W
  timeout=60s` で実行：636 → 1,199 ms）。ウォッチドッグはテストハーネスの設備なので、ベンチランナーは現在それなし（`run_wasm_unbounded`）で実行し、1.13× となります。`perf_ratchet.rs` R7 はこのランナーの追加負担がない状態を固定します。
- **`fft` は入力ではなく変換を測ります**（#1338）。以前はループ内の `data = data + [x]` で 8.4M 要素の入力を作っていたため、行の実時間の ~8% がリスト構築に費やされ、それをコード生成として公表していました。報告値 1.27x の一部は準備でした。現在は事前割り当て後にインデックスで書き込みます。これは参照実装の `Vec::with_capacity` + push と同じ形で、上記 `nbody_unrolled.rs` の公平性ルールです。2026-08-13 に 1.18x で基準を再設定しました。変換は変更前後でビット同一で、配列全体の位置重み付きチェックサムにより 2^12 / 2^16 / 2^20 で別途確認しています。行自身の `line1` 検証モードが比較するのは `size: 2^k = n` だけで、計算値を含まないため、ハーネスはこの行の壊れた変換を見つけられません。この穴は埋める価値がありますが、ここでは埋めません。計時パス内のチェックサム処理は、#1338 の発端となった「ベンチマークがカーネル以外のものを測る」問題をまさに再導入するからです。
- **`listbuild` は 1 つのワークロードを 3 行で表します**（#1337）。同じ 2^23 要素のインターリーブされた `Float` 配列を、(a) 事前割り当て + インデックス書き込み、(b) `var` + `for` + `data = data + [x]`、(c) `list.range |> list.flat_map` で構築します。3 つのソースは構築ループだけが異なり、算術とチェックサム消費処理は同一です。したがって行間の差こそ形のコストであり、計時前に 3 つすべてのバイト同一性を検証します。このファミリーが存在するのは、(c) が CLAUDE.md と docs/CHEATSHEET.md の推奨形でありながら最も遅かったからです。置き換えるべきものとして文書化された append ループの 1.67x で、原因はすべて `flat_map` の中間リストに対する要素ごとのヒープ割り当てでした。これは性能問題の前に MSR の問題です。イディオム文書は生成コードを誘導するコンテキスト内の資料なので、「推奨」と「高速」は同じ形を指す必要があります。現在 3 つは互いに 2% 以内です。
  このファミリーで `check-perf-ratio.sh` がゲートにするのは、各行の対 Rust 比ではなく行間の関係（`IDIOM_CEILING`）です。絶対比率は強くアーキテクチャに依存することが分かりました。同じコミットでも M4 Pro では 1.58、ubuntu-latest の CI ランナーでは 0.91 です。ワークロードのコストは算術ではなく割り当てにあり、2 つのアロケータの挙動が異なるためです。したがってこのファミリーでは「比率がマシン差を相殺する」が成り立ちません。各行は onebrc と同様に報告し、関係性（ローカル 1.018x、CI 1.045x）をゲートにします。
  **残る ~1.6x の正体を 2026-08-13 に測定しました。それは具体化でも #1004 の表現の話でもなく、決定論的ソフトウェア libm です。** 出力 Rust で、クロスターゲットのバイト同一性契約が要求する `almide_rt_libm_sin`/`_cos` だけをプラットフォーム版に入れ替えると、この行は 194.5 ms から 105.5 ms へ変わります。参照実装は 123.9 ms です。超越関数の決定性なしでは、出力コードは手書き参照実装を 1.17x 上回ります。構築の形に罪はありません。`list.repeat` + 境界チェック付きインデックス書き込みは 75.5 ms、`Vec::with_capacity` + push は 90.6 ms です。証拠：[string-gap-1004.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/research/benchmark/perf/string-gap-1004.md)。
- **`strchurn` は文字列の行です**（#1004）：N 個の整数 → `int.to_string` → `list.join` → `string.split` → `string.len` → `list.sum`。対 Rust 1.7x がその issue を生んだワークロードです。参照実装を 2 つ持ちます。意味がある比較は `rust-ref/strchurn.rs` との比較です。Almide プログラムが避けられない標準ライブラリの 2 つの義務を守ります。`split` は所有された `String` を収集し（`List[String]` の要素型は所有型）、長さは `chars().count()` です（`string.len` は文字数）。M4 Pro では 1.12x です。`rust-ref/strchurn_idiomatic.rs` は同じプログラムを借用した `&str` 断片とバイト単位の `len()` で書き、1.9x 速く動きます。その差のすべては API 契約であってコード生成ではありません。
  listbuild ファミリー同様、この行は基準固定ではなく REPORTED です。差の 75% は N 個の所有された `String` の malloc/memcpy/free なので、コード生成より先にアロケータを比較しており、1 アーキテクチャの数値をゲートにすべきではありません（ただし listbuild と違い、こちらは両側が同じ割り当てを行うため、2 つ目のアーキテクチャでの測定によって昇格できる可能性はあります）。完全な要因分析 — 75% が `split` の戻り型、11% が `string.len` の意味論、9% が rlib 境界、5% がリスト中間値で、issue タイトルが疑った `RcCow` は経路にまったく存在しない — は [string-gap-1004.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/research/benchmark/perf/string-gap-1004.md) にあります。`strchurn/ladder.py` で最初から再構築できます。
- **`wordfreq` は Map の行です**（#2150、#2157）：5 000 語の語彙から N 回抽出し `Map[String, Int]` で数え、回数降順 / 単語昇順で上位 10 件を出します。単語カウント、group-by、ヒストグラムの各プログラムにあるキー付き集計の形です。listbuild 同様、1 つのワークロードに 2 つの書き方があります。`wordfreq` は命令的な `var` Map + `m[w] = (map.get(m, w) ?? 0) + 1` ループ、`wordfreq-group` は CHEATSHEET の `list.group_by` + `map.map` です。語彙、抽出、ソート、出力が同一なので、その差が推奨イディオムのコストです。参照実装（`rust-ref/wordfreq.rs`）は同じ形・同じ意味論で、`HashMap<String, i64>`（hashbrown + SipHash）、抽出ごとに語彙からクローンする所有キー（Almide の `let w = vocab[i]` は所有）、抽出ごとに 1 回の `entry` を使います。
  両行とも wasm 系統も走り、`check-perf-ratio.sh` はそれらの wasm での関係をゲートにします（`wordfreq-wasm-idiom`、上限 1.15×）。#2156 より前、構造的な系統の `group_by` は新しいキーごとにアキュムレータをコピーして拡張し、検索を線形の `$scan_str` に固定していました。全要素が全グループキーを走査し、5 000 語の語彙では要素あたり ~11 µs、命令的な記述の 110× でした（2M 回抽出：201 ms に対して 22 s）。現在は `$map_reserve` を通じてその場で拡張し、`m[w] = …` ウィンドウと同様にインデックス経路に乗り、wasm ではイディオムの行が命令的な行の ~0.8× です。
  `wordfreq` の native/Rust 比は #2150 以降、`check-perf-ratio.sh` の PAIRS に基準固定されています（4M の quick ワークロードでベースライン 1.90）。コード生成より先にアロケータとハッシュ関数を比較するため、2 種類のマシンの測定が得られるまでは報告のみでした。それらは一致しており、2M で成功したランナー実行 5 回は 1.66-1.75、M4 Pro の 2M では 1.77、4M では 1.97 です。`wordfreq-group` は報告のみのままです。
  #2150 以降の native の `AlmideMap` はコンパクトな順序付き辞書です（挿入順のエントリベクタ + 位置を保持するオープンアドレス法のスロットテーブル。エントリごとに完全なハッシュをキャッシュし、演算ごとに multiply-fold ハッシュを 1 回行う）。読み取り側（`map.get` / `get_or` / `contains`、`set.contains`）はキーを借用し、単純変数キーの書き込み `m[w] = …` はキーをムーブします。2M 回抽出ループの Map コストは M4 Pro で ~93 ms から ~55 ms に下がり、同じプログラムの構造的 wasm 系統の ~73 ms を下回りました。
- **`jsonround` は Value の行です**（#2157）：N 個の 4 フィールドのレコード（ネストしたオブジェクト、2 文字列の配列）を 1 つの `Value` に構築し、`json.stringify`、`json.parse` で往復してネストした `meta.score` を合計します。native と wasm のみで、ゲートにするのは Rust 比ではなく両系統の関係です。#2157 では wasmtime より native バイナリの方が 1.3–2.5× 遅いことが分かりました。`value.array` / `value.object` は渡されたリストから各要素をクローンし（各要素が部分木全体なので、文書はネストの各階層でコピー）、`value.as_array` は解析済み文書から全レコードをクローンし、`json.stringify` はノードごとの `String` とコンテナごとの `format!` + `join` でテキストをボトムアップに組み立てていました。現在 3 つの操作は `_owned` ランタイム対応版を持つ `@reuse` として宣言され、最後の使用となるソースは結果にムーブされ、テキストは 1 つのバッファに書かれます。`check-perf-ratio.sh` はこの行と wordfreq の両行で native ≤ wasm を維持します（NATIVE <= WASM ブロック）。
- **`regexcycle-8` / `regexcycle-32` はルールテーブルの行です**（#4392）：N 個の `Regex` 値を 4 行のテキストの各行に順番に適用し、どちらの N でもマッチ数は同じです。Rust 参照実装はなく、行間の関係をゲートにします（`check-perf-ratio.sh`、REGEX CYCLE、両系統で上限 1.5x）。0.69.0-rc1 はコンパイル済みパターンを 16 エントリの LRU リストに保持していたため、16 を超える周期では各マッチで全パターンを再コンパイルしました。修正を戻すと 3.27x、修正ありでは 1.03x（GCE n2-standard-4）です。native キャッシュは現在、ハッシュインデックスの下に 1024 パターンを保持し、満杯ならランダムに追い出します。そのため、それより長い周期でも大半はコンパイル済みのままです。

## 実行

```bash
python3 research/benchmark/perf/bench.py                 # full suite, all legs
python3 research/benchmark/perf/bench.py --runs 7 --label m4pro
python3 research/benchmark/perf/bench.py --legs native,rust --bench nbody
ALMIDE_BIN=target/release/almide python3 research/benchmark/perf/bench.py
```

`almide`（または `ALMIDE_BIN`）、`rustc`、wasm 系統では `wasmtime` CLI が必要です。

## CI ラチェット

`scripts/check-perf-ratio.sh`（CI の `perf-ratchet` ジョブ）は、`--quick` ワークロードで、各ベンチマークの almide-native / 手書き Rust 比率を `scripts/perf-ratio-baseline.txt` に対してゲートします。絶対時間ではなく比率です。比率がランナー差を相殺します。持続的な退行・改善があれば、同じ変更内でベースラインを移動します。完全な方針はスクリプトのヘッダーを参照してください。行は 3 クラスです：PAIRS（±帯域に基準固定）、REPORTED（表示のみ、アロケータ依存）、VICTORY（#1330：Almide が通常の Rust より速い行で、`ratio < 1.0` と、その行の効果の要因とされる最適化を同一ソースで無効化した比較をゲートにする）。

## まだカバーしていないもの

- **リストコンビネータの遅延性：もはや未解決ではないが、行はまだない。** 2026-08-13（[string-gap-1004.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/research/benchmark/perf/string-gap-1004.md) の発見 (a)）には、`list.range` → `map` → `fold` は 30M 要素で実際に 240 MB の `Vec` を作り、融合された Rust イテレータチェーンの 6.6x で動いていました。現在このチェーンは native 系統でカウント付きイテレータへ低水準化されます。`(0i64..n).map(…).filter(…)` となり、範囲の `Vec` はありません（#2045 の融合と #3824 のカウント付き fold ヘッドの作業）。2026-10-20、M4 Pro で再測定しました（develop 5d52899e2、`--release`、`n` は argv から読み取り、LLVM がループを閉じた式に変換できないよう `% 1000` の本体、交互に 9 回実行）。Almide は中央値 18.0 ms、最大 RSS 1.5 MB、手書き Rust イテレータチェーンは 14.0 ms で、6.6x ではなく 1.29x です。この形をゲートにするベンチマーク行はまだありません。
