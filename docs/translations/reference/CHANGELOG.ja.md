<a id="changelog"></a>

# 変更履歴

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](CHANGELOG.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](CHANGELOG.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](CHANGELOG.zh-CN.md)

> 非規範の原文固定・歴史記録用読者スナップショット。変更していない原本を正本とする。自動同期、この翻訳のための独立実行・再計測・証明検証は行っていない。先頭は2026-07-26の0.36.0であり、最新版の主張ではない。「現在」、機能対応・削除、CI結果、等価性や性能・モデル比較は各記載時点に属し、現行実装や普遍的保証ではない。原文の certified は記載されたコンパイラの証明書検査を指し、第三者認証ではない。原文の不整合も保持する：0.15.0は11カテゴリー・12モジュールとする一方、各一覧は13項目ある。S2の厳格な契約・非Unknownの主張と、リリースビルドが診断後も続行する説明が併存する。0.14.6の既知の不足と後段の追加済み説明、E001..E099の主張とE420も併存する。有限のdojo結果と著者のSOTA・UFCS判断は当時の観測と解釈であり、全モデル・全タスクの証明ではない。過去のパスやコマンドを含むコード表記は原文のまま保持する。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/CHANGELOG.md)

このプロジェクトの重要な変更をここに記録する。本ファイルは `0.14.6` から開始した。それ以前の版は末尾の「このファイルが存在する前」で遡及的にまとめる。

形式はおおむね [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) に従う。各項目は診断・ツール・言語・stdlib に対する意図で分類する。下流の利用者（LLM harness、editor、利用者）が気にするのはその点だからである。

## [0.36.0] — 2026-07-26

10 個の新たな動作契約（C-165…C-174）、wasm renderer の暗黙 defer 機構の廃止、契約台帳から生成する適合性レポート、そして `cargo test` のたびにターゲット間でバイト比較する 2 つの代表例。

<a id="fixed--cross-target-equivalence"></a>

### 修正 — ターゲット間等価性

- **TCO Map/Set accumulator の初期値**（C-174）: 末尾再帰 accumulator の `acc + []` は以前 EMPTY deferred block を束縛しており、空でない初期値が wasm で黙って消えた。#810 census が発見した実在の誤コンパイルである。初期値は現在、本物の `Op::Dup` で accumulator に共有される。0 回反復、Set accumulator、`_str` map layout は fixture で固定。（#810）
- **整数リテラル**（C-173）: 絶対値だけでなく、符号と i64 carrier に対して範囲検査する。`let x: UInt32 = -5` と carrier を超える hex literal には、それぞれ型範囲を述べる E024 hint が付き、書いたとおりに表示する（`5` ではなく `-5`）。検査は aggregate collection の要素や record field / tuple 位置にも再帰する（fuzzer が発見した check と build の差）。`i64::MAX` を超える UInt64 経路は #872 で追跡する。
- **型消去した `unwrap_or`**（C-172）: 全 heap payload を self-host 化し、最後の unlinked-call 拒否分類を解消。
- **バイト offset の境界**（C-171）: 減算形式で記述し、`i64::MAX` 付近の位置が wrap して通り抜けないようにする。
- **整数算術は wrapping op を出力**（C-170）: module-level `let` および `overflow-checks = true` でも overflow 則が成り立つ。
- **`list.repeat`**（C-169）: `string.repeat` と同じサイズ上限。過大な回数は両ターゲットで同じように abort する。native runner の `string.repeat` shim も負の回数を clamp し、共有の 2^31 バイト上限超では panic でなく abort する。
- **`list.flatten`**（C-168）: 引数を借用するため、同じ式内で別の借用と合成できる。
- **`float.clamp`**（C-167）: `f64::clamp` と同じ比較チェーンを使い、範囲内の負のゼロが両ターゲットで符号を保持する。
- **`map` の `to_string` ファミリー**（C-166）: 接尾辞なしで repr router を経由し、Float 値の表示を self-host 化。
- **heap accumulator の fold**（C-165）: skv map layout、String list、String set を self-host 化。
- **`some(...)` 内の heap 型呼び出し**を scalar として格納できなくなった。Some payload gate が payload 型を読むため、#871 の誤値形式は修正設計中は拒否する。
- **heap 要素 tuple のキャプチャ**を clone で包み、`(String, …)` tuple をキャプチャする closure と元の値の後続使用を両立させる。nightly fuzz が発見した 2 つ目の実在問題。

<a id="hardened--the-deferred-bind-terminal-810-777"></a>

### 強化 — deferred-bind の終端（#810、#777）

- `Init::Opaque` は 1 型に 3 つの意味を持っていた。本当に空の場合は独立した `Init::Empty` に、TCO accumulator 共有は本物の `Op::Dup` に、構築不能の場合は拒否にする。bind 終端は 3 形状の許可リスト（ClosureCreate / Lambda / Range）の背後で既定拒否となり、C-174 クラスが黙って再発できないようにする。
- effect-unwrap 脱糖の到達範囲より深く入れ子になった error 演算子は、deferred Const の危険を冒さず名前付きで拒否する。direction-proof テストにより、将来の脱糖拡張も native と byte-match しなければならない。
- lower 済みの全関数に対し、op を網羅的に defines/reads に分けた def-before-use 整形式 gate を通す。新しい Op variant は分類されるまで compile error となる。（#777）

<a id="added--evidence--conformance"></a>

### 追加 — 証拠と適合性

- ALS 適合性レポートを契約台帳から生成する（`docs/contracts/conformance.md`、各台帳節に fixture の裏付け）。CI の鮮度 gate を設け、契約 index にも同じ gate を追加。（#811）
- `docs/contracts/` に証明済み対信頼依存の境界 map を追加。
- v0 時代の wasm test skip 2 件を削除。台帳 gate が、残る全 skip を明示的な platform limit として維持する。
- 拒否される `main` は、単に subset 外だというだけでなく理由を報告する。

<a id="added--examples"></a>

### 追加 — 例

- `examples/raytracer.almd`: ターミナルに ANSI 24-bit half-block art を直接描く ray tracer。鏡面球 3 つ、市松模様の床、硬い影、鏡面ハイライト、3 回の鏡面反射。ターゲット間で byte-identical。
- `examples/lisp.almd`: closure、lexical scope、遅延束縛の top-level 定義を備える mini-Scheme。`fact`、`fib`、`map`、quicksort が解釈される Lisp の中で動く。ターゲット間で byte-identical。
- `tests/examples_parity_test.rs`: 各 example は PARITY（`cargo test` ごとに native と wasm をバイト比較）か、比較できない理由付きの LEDGERED のどちらか。どちらにもない example は suite を失敗させる。

<a id="internal--code-health-codopsy-round-2"></a>

### 内部 — コード健全性（codopsy 第 2 ラウンド）

- almide-frontend、almide-tools、almide-interp、almide-dialect を、name-router 抽出と完全性テスト付きの table-driven lookup によって A 評価へ。E015 reimpl-lint の提案は現在、決定的になる（ソート済み fn 一覧に対する edit distance）。
- almide-mir の最悪 4 関数（循環的複雑度 103–116）を phase helper に分割し、986 ファイルのコーパスで両 renderer を byte-verify（emit と wasm の hash は不変）。#852 は継続。
- MIR debug channel を 1 つの文書化した trace module に通し、roster test で全 channel の記載を保つ。

## [0.35.0] — 2026-07-25

差分 fuzzer と issue 台帳に基づく正しさのリリース。ターゲット間不一致 10 件を修正、v1 subset の拒否 3 件を解消し、`almide check` は通るが生成コードが build できない受け入れ差 2 件を修正した。新たな動作契約 4 件（C-161…C-164）と cross-target fixture 8 件が結果を固定する。

<a id="fixed--cross-target-equivalence-1"></a>

### 修正 — ターゲット間等価性

- **Matrix constructor の次元**（C-161）: 負の次元は 0（空行列）へ clamp し、共有の 2^28 上限を超える要素数は両ターゲットで `Error: matrix dimensions too large` と exit 1 で abort する。`seq = -2147483648` で構築した transformer 形状プログラムは以前、native では raw-vec capacity-overflow panic（exit 101）、wasm では境界外 trap（exit 134）という異なる形で両経路が crash していた。T6 規約はどちらも許容しない。（#849）
- **`io.write` / `io.write_bytes`**（C-162）: 両方を wasm 上で単一 iovec の `prim.fd_write` 基盤上に self-host 化し、`io.print` と同じ `Capability::Stdout` を持たせた。またプログラム順で出力する。native はこれらを終了時しか flush しない 64 KiB `BufWriter` に通し、一方 `println!` は Rust 自身の `Stdout` へ書いていたため、交互の出力が並べ替わっていた。native 単独でも誤りだった。（#805）
- **let/var に束縛した heap 結果の `if`/`match`**（C-163）は、両経路で選択 arm を実行する。欠けていたのは `some(...)` 内の String リテラル payload で、保存された café fuzz 発見例そのもの。（#860）
- **tuple / record / nested-list 要素の List 変更操作と末尾コピー**（C-164）: `set`/`update`/`insert`/`remove_at`/`swap` と `take_end`/`tail`/`drop_end` に rc が正しい対応実装を設け、結果が各コピー要素を共同所有し、元のリストも読めるようにした。以前は未登録名に送られて拒否されていた。（#850、#808）
- **負の offset と幅は全域的**（C-034）: 各 `bytes.read_*` の offset guard を `checked_add` にする（負の `pos` が巨大な `usize` となり、加算で範囲内へ wrap して slice index で panic していた）。`bytes.read_f16_le` の wasm self-host に兄弟関数と同じ負値 guard を追加。`string.pad_start`/`pad_end` は符号付きで幅を clamp、`bytes.copy_to_ptr` は符号なしで容量を clamp、共有 2^31 バイト上限を超える `repeat` は両経路で同様に abort する。
- **高精度の `float.to_fixed`**（C-155）: 桁用作業領域は f64 の約 1074 桁の厳密展開に合わせたサイズだったが、`nd` は 4096 に達するため、境界を超えると wasm で隣接 heap を読んでいた。既存 fixture は展開を 6 文字へ切り詰めていたので検出できなかった。
- **空 needle の `string.replace_first`**: not-found sentinel は `send` だったが、空 needle が `send` に正当に一致する場合と衝突し、wasm では `replace_first("", "", to)` が haystack を変更せず返していた。
- **空の重み行列に対する `matrix.swiglu_gate`**は、native oracle の空 operand guard に合わせて両経路で空行列を返す。

<a id="fixed--check-accepted-must-build"></a>

### 修正 — check で受け入れたものは build できる

- **intrinsic の borrow mode を runtime 自身のシグネチャから取得**: `build.rs` が `runtime/rs/src/` 内の各 `pub fn almide_rt_*` のパラメータごとの `&mut` 性を抽出し、read-only helper に `&mut` を渡さないようにする。旧 heuristic（「Unit を返す ⇒ arg 0 を変更」）は `&Vec<u8>` パラメータに対して `almide_rt_io_write(&mut chunk)` を出力し、mandelbrot bench で E0596、さらに `io.write_bytes` と `testing.assert_contains` にも潜在していた。（#803）
- **書き込み専用の `var` が dead-binding 除去後も残る**: index assign は書き込みなので `use_count` は数えず、DCE が `let` を削除した一方、write-back はその名前を残した（E0425）。現在は DCE と未使用変数警告の両方が mutation target を参照する。（#857）
- **サイズ付き整数 collection literal を範囲検査**: `let bs: List[Int8] =
  [1, 256]` がコンパイルされ、その後 rustc が `256i8` を拒否していた。要素リテラルを注釈付き要素型へ固定し、既存 E024 検査を発火させる。
- **`mem.save` / `mem.restore` に native 本体を追加**: runtime が一度も定義しない `almide_rt_mem_*` symbol を宣言していたため、呼び出すプログラムはすべて無効 Rust を出力していた。

<a id="fixed--diagnostics"></a>

### 修正 — 診断

- **import した module の型エラーが import 元へ届く**（#862）: check 経路で module 本体を推論せず、build driver は `infer_module` の診断を誰も読まない一覧へ追記していた。`almide check`、`build`、`test` は現在、module 自身のファイルを示してすべて失敗する。`base64` の E006 はこの背後で数週間残存していた。これを表面化させることで、隠れていた誤検出も 2 件修正した。架空の E019（「ambiguous constructor 'Wrap': declared in Wrapper and Wrapper」）と、package をまたいで合法な同名型に対する架空の E020。

<a id="changed--wasm-subset-coverage"></a>

### 変更 — wasm subset の範囲

- **`main` も宣言するファイルで、`almide test` が wasm レッグの test block を実行**。native test mode（`main` をコンパイルするが呼ばない）に合わせる。これだけで spec 17 ファイルが native fallback から外れた。（#813）
- **同梱 stdlib module を関数単位でリンク**: intrinsic を含む module も、pure Almide 拡張（`list.split_at`、`iterate`、`bundled_probe`）を通常のリンク済み兄弟として提供し、intrinsic-backed fn は registry dispatch を維持する。（#812）
- **名前キーの stdlib 判断で単相化接尾辞を取り除く**: `option.collect__Int` が materialized-variant read-shape 検査に入らず、結果への `match` が追跡されず拒否されていた。（#812）
- `spec/` の native fallback は 32 から 11 へ減少。そのうち 8 は本当に host 依存の `// wasm:skip` ファイル。
- **walled-real ratchet が 0 に復帰**。同梱 stdlib リンク作業以降、Trust Spine gate は赤だった。原因は 2 つ。`classify_corpus` が import 解決をハードコード stdlib 一覧で制限し、`import args` が届かず、`args.flag` が未検証 caps の stdlib 呼び出しとして拒否されたこと。さらに linkable-module 規則の独自コピーを持ち、関数単位リンクを知らなかったこと。現在は両方を `pipeline::linkable_module_fns` に通し、拒否レポートが実パイプラインの実際にコンパイルするプログラムを記述する。（#848）

<a id="added--tooling"></a>

### 追加 — ツール

- **`ALMIDE_WALL_REASON=1`** が wasm render のどの stage（render / wat assemble / validate）が拒否したかを示し、関数ごとの理由も渡す。fallback ファイルのために `render_program` を手で追う必要がなくなる。
- **`almide compile <module> --json` が同梱専用 module に対応**: `path`、`args`、`html`、`mem` は、バイナリがソースを内蔵する module に「module not found」を報告していた。`html` には `bundled_source` arm が全くなく、現在は unit test が 2 つの一覧の一致を維持する。（#863）

<a id="added--documentation"></a>

### 追加 — 文書

- **stdlib ページを 16 追加**。同梱の全 module にページができた（23 から 39/39 へ）。`path`、`args`、`html`、`mem`、`net`、`zlib`、`base64`、`hex`、およびサイズ付き数値変換 module 8 つ。すべて機械管理の signature index を持ち、worked example は実行可能（`spec/stdlib/bundled_module_docs_test.almd`、`spec/stdlib/sized_numeric_docs_test.almd`）。これによって本リリースの欠陥 3 件を発見した。（#863）
- **module interface がサイズ付き数値を命名できる**: `Int8`…`UInt64` と `Float32`/`Float64` は `Named` ではなく独自の `Ty` variant であるため、これらを含むすべての `almide compile <mod> --json` signature が `unknown` に serialize され、`int.to_int8` が `(n: Int) -> ?` と読めていた。
- **`docs/wasm/` 設計 memo を archive 化**: v1 trust spine より前の 2026-04 の 6 文書を、理由を示す banner とともに `docs/wasm/archive/` へ移動。`memory-model.md` は 2 つの linear memory と free のない bump allocator を記述していたが、出荷コンパイラはそれを実装していない。（#863）
- **roadmap index 完備**: 不足していた `<!-- description: -->` 34 件と `<!-- done: -->` 21 件を補完。description は各ファイル自身の題名から書き、日付は git 履歴から取得した。（#858）

## [0.29.0] — 2026-07-15

<a id="changed--compilation"></a>

### 変更 — コンパイル

- **v1 の検証付きレンダラーが wasm のデフォルト経路になった**（97e1cd8c）：すべての `almide run` / `almide build --target wasm` が認証付き MIR パイプラインを通る。所有権・名前の全域性・ケイパビリティの証明書を、健全性が Coq で機械証明されたチェッカーがビルドごとに再検証する。`--no-verified` で v0 エミッターに戻せる。組織全体のバイト検証で判明した、検証付き経路をデフォルトにした場合の出力差異6件は、切り替え前に修正した（モジュール間 ABI レジストリ、void クロージャのディスパッチ、入れ子ラムダの命名、キーをキャッシュする `sort_by`、幾何的に増やす連結容量、C-129 ガード）。

<a id="added--language"></a>

### 追加 — 言語

- **検証付き経路での可変モジュールレベル `var`**（C-033）：モジュールグローバルは、認証された take/drop 遷移を持つ線形メモリのスロットに置かれ、エイリアスされたスナップショットはコピーオンライトとなる。`var g = [...]; let s = g; g[0] = 9` はスナップショットを観測し、ネイティブとバイト単位で一致する。これにより、デフォルト経路で既知の最後の出力差異を解消し、`spec/wasm_cross/module_var_alias_cow.almd` で固定した。
- **`env.get`**（C-133、ALS-R5）：両 wasm エンジンで WASI environ を通じてホスト環境を読む（すべての wasmtime 起動に `-S inherit-env` を接続）。ネイティブはプロセス環境を読む。他のすべての effect を持つ公開操作と同様にケイパビリティで制限する。

<a id="added--tooling-1"></a>

### 追加 — ツール

- **ネイティブ trust-spine の段階1〜4**（`--target rust --verified`）：スカラー・String・リストのプログラムについて、wasm 経路と同じ認証付き MIR からネイティブ Rust を生成する（Dup→clone、Drop→スコープ末尾での消去、ListLit/ListGet/ListSet 操作の共有）。v0 との出力差分で検査する。部分集合の外のプログラムは正直に拒否し、v0 にフォールバックする。

<a id="removed--language"></a>

### 削除 — 言語

- **`fan.timeout` を削除**（C-006、#765）：実時間タイムアウトには移植可能なクロスターゲットの意味がなく（wasm には時計・スケジューラー・スレッドがない）、結果がプログラムとその入力の関数にならない唯一の標準ライブラリ操作だった。期限が発火するかは、ネイティブで2回実行する間でもマシン負荷に依存していた。参照すると、移行ヒント付きの削除済み機能エラー（**E027**）を check 時に出す。期限はプログラムを起動するホスト境界で扱う（`timeout 5 ./app`）。組織全体の調査で利用箇所は0件だった。台帳最後の `flagged-for-revision` 契約を解消し、flagged のラチェット値は0、README の等価性の主張は「例外：なし」となった。

<a id="fixed--wasm-codegen"></a>

### 修正 — WASM コード生成

- **Perceus RC：`guard`-`else` のヒープ一時値**（#755）：`guard … else { … }` ブロック内で束縛されたヒープローカル値に、WASM ターゲットで参照カウント処理が行われていなかった。`Guard` は `IrStmtKind` なので、`block_to_fnbody` が perceus の素通し分岐 `FnBody::Stmt` に渡し、その分岐は内部を再帰処理していなかった。そのため else ブロックの一時値にスコープ末尾の `Dec` がなく、WASM RC 検証がビルドを拒否した（`[perceus-belt] LEAK: no RcDec`）。Stmt 分岐は今では Guard の `cond` と `else_` を `perceus_expr` で処理するため、発散する else ブロックも他のブロック同様に往復処理され、Dec の釣り合いが取れる。

<a id="fixed--rust-codegen"></a>

### 修正 — Rust コード生成

- **末尾位置で素の値を返す `@intrinsic` effect 関数**（#758）：ランタイムが素の値を返す組み込み effect 関数への末尾呼び出し（`io.print →
  Unit`、`io.read_line → String`、`fs.exists → Bool`、`env.get → Option`）を、誤って Result の末尾ラップ対象から除いていた。除外集合にはランタイムで `Result` を返すものだけでなく、すべての `@intrinsic` effect 関数が入っていたため、自動で `Result` にラップされた関数の末尾に素の `()` / 値が残った（不正な Rust、E0308）。集合は実際に `Result` を返す組み込み関数から構成するようになった（宣言が `-> Result[...]` のものに加え、`-> Unit` 宣言だがランタイムで `Result<(),
  String>` を返す `http.serve`）。したがって、素の値を返す組み込み関数の末尾にも、他の末尾と同じく `Ok(...)` が付く。

## [0.23.3] — 2026-05-24

<a id="performance--wasm-parity-with-rust"></a>

### 性能 — WASM と Rust の同等性

- **ハッシュテーブル map**：FNV-1a/splitmix ハッシュを使うオープンアドレス法で、負荷率75%でリサイズ。`map.insert` 480ms → 0.5ms（960倍）、`map.get` 1041ms → 0.6ms（1700倍）。
- **二分再帰の変換**：`f(n-1) + f(n-2)` パターン → アキュムレーターループ。`fib(35)` 63ms → 35ms、Rust WASM と同等。
- **ラムダのインライン化**：キャプチャのないラムダはクロージャ変換を省き、`list.map`/`filter`/`fold` ループ内に本体をインライン化する。単純なラムダはゼロコストのクロージャとなる。
- **ソートの連続区間検出**：O(n) の昇順・降順事前走査と直接 swap による反転。逆順100k：5.6ms → 0.5ms。
- **WASM パイプラインの TCO**：末尾呼び出し最適化パスを WASM ターゲットでも実行する（従来は Rust のみ）。
- **適応的な作業用ローカル**：単純な関数の作業用ローカルを48/16/16/8ではなく4/2/2/0とする。`fib` は88ローカル → 8。
- **文字列レイアウトの移行**：int_to_string、float_to_string、println_str、JSON パーサー、文字列述語、および他10以上のファイルでデータオフセットを修正（4→8）。
- **WASM `datetime.monotonic_ns`**：新しいディスパッチと `clock_time_get` の8バイトアライメント修正。

<a id="result"></a>

### 結果

- Rust WASM に対して3勝：`int_tostring`（0.63倍）、`int_parse`（0.83倍）、`fib35`（≈1.0倍）。
- 7件同等：`list_map`、`list_fold`、`str_concat`、`math_sqrt`、`sort`、`map_insert`、`map_get`。
- 残る差は1件：`list_filter`（3倍、WASM の構造的分岐と LLVM の cmov の違い）。

## [0.23.2] — 2026-05-23

<a id="performance"></a>

### 性能

- **WASM ソート**：挿入ソート O(n²) をボトムアップ・マージソート O(n log n) に置き換え、100k 要素で222倍高速化。
- **WASM 文字列連結**：容量を持つ文字列レイアウト `[len][cap][data]` と、`__string_append` ランタイムによる償却 O(1) の追記。ピープホール変換 `s = s + "x"` → インプレース追記。
- **文字列リテラル最適化**：`&str` 借用コンテキストの文字列リテラルで `.to_string()` を省略。`string.contains` 572ms → 45ms。

<a id="added"></a>

### 追加

- インプレース文字列変更用の `string.push(mut s, suffix)` と `string.clear(mut s)`。
- 標準ライブラリ・ベンチマークスイート：18モジュールにわたる43ベンチマーク（`research/benchmark/stdlib/`）。

<a id="result-1"></a>

### 結果

- 複合ベンチマーク（fib + sort + map + concat + bytes）で Almide WASM は Rust WASM の0.81倍（高速）。

## [0.16.1] — 2026-05-12

<a id="added-1"></a>

### 追加

- **almide-dialect クレート**：SSA lowering、dump、Rust エミッター、検証器を備えた純 Rust の MLIR 方言スキーマ。方言パイプラインは既存 Rust コード生成との等価性テスト7/7件に合格。
- **コンパイル時の値パラメーター**：Mojo 形式の `fn f[N: Int]()`。スカラー型境界を const パラメーターとして自動検出する。`zeros[5]()`、`dot[3](a, b)` がエンドツーエンドで動く。
- **Mojo 同等性評価**：`docs/roadmap/active/mojo-parity-assessment.md` に16軸の重み付き採点。

## [0.16.0] — 2026-05-11

<a id="added-2"></a>

### 追加

- **ビットのイントロスペクション**：`int` 標準ライブラリに10関数追加。`count_leading_zeros`、`count_trailing_zeros`、`pop_count`、`bit_reverse`、`byte_swap`、`bit_width`、`log2_floor`、`log2_ceil`、`next_power_of_two`、`prev_power_of_two`。Rust ランタイムとネイティブ WASM コード生成（`i64.clz`/`i64.ctz`/`i64.popcnt`）。
- **Hashable プロトコル**：Map キー型をコンパイル時に検証する。`Map[Float, V]` と `Map[Fn, V]` はエラーとなる。組み込み型（Int、String、Bool、定幅整数）は Hash 適合として自動登録される。`deriving Hash` を持つユーザー型は `#[derive(Eq, Hash)]` を出力し、Map キーとして使用できる。

## [0.15.10] — 2026-05-10

<a id="fixed"></a>

### 修正

- WASM JSON パーサー：科学記数法（`1.5e-7`、`2E10`）を正しく解析する。

## [0.15.9] — 2026-05-10

<a id="fixed-1"></a>

### 修正

- WASM ライブラリモード：`main` のない `almide build --target wasm` はテスト関数を省き、`_start` エクスポートを出力せず、ブラウザー / JS 組み込み用のクリーンなライブラリモジュールを生成する。

## [0.15.8] — 2026-05-10

バグ修正パッチ。ResultPropagation の書き直しと WASM の guard-else 修正。

<a id="fixed-2"></a>

### 修正

- match 分岐の `err()`：match の型が Result でない場合は早期 return、それ以外は値となる。
- モジュール間のレコードリテラル（`module.Type { ... }`）が解決できなかった問題。
- `fs` ランタイムを含む場合、`FileStat` 構造体を自動出力する。
- effect 関数の `guard ... else { println(...); () }` を `Ok(())` でラップする。
- WASM の `guard ... else { err("msg")! }`：Block/Unwrap/Try ラッパーを剥がす。
- 変更される `Bytes`、`Vec`、`HashMap` の束縛が `let mut` に昇格しなかった問題。

<a id="changed"></a>

### 変更

- ResultPropagation パスを書き直し、デッドコード700行を削除、`resolve_err_types` を追加。
- ConcretizeTypes の `kind_name()`：すべての IrExprKind 変体を列挙（以前は26個が `(other)` に隠れていた）。
- ConcretizeTypes の `infer_err_ty_from_enclosing`：lift 前の `enclosing_ret` に対応。

## [0.15.0] — 2026-04-20

モジュール間 `let` が Swift/TypeScript と同等になり、WASM ディスパッチの欠落（既存バグと最後の行列演算ブロッカー3件）を解消し、今後のディスパッチの乖離を防ぐ CI ゲートを追加した。変更は追加的であり、破壊的 API 変更や公開機能の削除はないが、モジュールシステムの公開機能変更が十分大きいためマイナーバージョンを上げる。

<a id="cross-module-let-now-works-for-every-type"></a>

### モジュール間 `let` がすべての型で動作

モジュールスコープの `let` 束縛（`util.almd` の `let FOO = ... `、`main.almd` の `util.FOO`）は、従来いくつかの形で壊れていた。

- スカラー定数（`let MAGIC: Int = 42`）は `(*ALMIDE_RT_UTIL_MAGIC)` としてコンパイルされたが、生成 Rust は `const ALMIDE_RT_UTIL_MAGIC: i64 = 42i64;` だった。`i64` は参照ではないため、rustc が参照外しを拒否した。
- 複合値への参照（`let RES: Result[Int, String] = ok(7)`）はモジュール lowering で型注釈を失った。生成 Rust は `LazyLock<Result<i64, _>>` となり、rustc が `E0121` で拒否した。注釈を復元しても `*LAZY_VAR` は参照外しから値をムーブするため、消費する呼び出し側（`result.unwrap_or((*LAZY_VAR), ...)`）で `E0507` が起きた。clone 挿入パスが合成されたモジュール間 Var を見落としていたためである。
- 注釈のないリテラル（`let ANON = { a: 1, b: "hi" }` / `let MAP = ["a": 1]`）は `main` の利用箇所で型検査すると `Unknown` となり、`LazyLock<_>`（rustc）または `ConcretizeTypes` 事後条件違反（WASM）を生んだ。

独立した4修正を順に導入した。

- `walker/expressions.rs`：合成 `ALMIDE_RT_<MOD>_<NAME>` Var を常に `LazyLock` ベースと仮定するのをやめた。`CodegenAnnotations` に `lazy_top_let_names: HashSet<String>` を追加し、各モジュールの `TopLetKind::Lazy` 項目から事前に索引化する。スカラー `Const` の top_lets は素の名前で出力する。
- `frontend/src/lower/mod.rs`：`TopLet` 分岐の型検索は `{prefixed, unprefixed, expr_ty}` の順でフォールバックする。モジュールスコープ束縛が、ジェネリック情報を欠いた `Result[Int, ?]` を黙って再推論するのではなく、登録時に設定された注釈型を見つけられる。
- `pass_clone.rs::split_clone_ids`：`ALMIDE_RT_` 接頭辞の名前を常に clone 対象とした。合成モジュール間 Var は `.clone()` で実体化され、LazyLock からムーブしない。
- `canonicalize/registration.rs::infer_literal_type` は `Record` / `List` / `Tuple` / `MapLiteral` / `Some` / `None` / `Ok` / `Err` / `Paren` を再帰処理し、登録時に具体型を返す。`Checker` には `current_module_prefix` も追加した。`infer_module` がこれを設定し、モジュール検査順にかかわらず `TopLet` の書き戻しが接頭辞付き（`util.ANON`）と接頭辞なし（`ANON`）の両キーに届くようにする。

テストした11カテゴリーはすべて Rust と WASM の両方で動作する。Int、Float、Bool、String、List、Map、Set、Tuple、Option、Result、名前付きレコード、匿名レコード、バリアントについて、注釈あり・なしの両方である。

<a id="wasm-dispatch-hardening"></a>

### WASM ディスパッチの強化

- **Mono の `Discover` におけるモジュール名ガード**：同じ関数名が複数モジュールにある場合（`option.filter[A]`、`list.filter[A]`、`result.filter[A, E]`）、最初の名前一致が特殊化を黙って横取りし、誤った `(module, fn, suffix)` キーで登録していた。続く `Rewriter` が検索に失敗し、呼び出し箇所が `Module { m, f }` のまま残り、WASM ICE を起こした。`Discover` は `Rewriter` と同じくモジュール名で絞り込む。`stdlib/result.almd` の thunk 回避策 `filter_with(r, pred, () -> E)` は、仕様どおりの `filter[A, E](r, pred, err_val: E)` に戻した。
- **`emit_<module>_call` の Named ディスパッチ・フォールバック**：`emit_wasm/calls.rs` の Module 分岐は、panic 前に `func_map["almide_rt_<m>_<f>"]` を試す。未知モジュールの `_ =>` 分岐のフォールバック連鎖と同様である。`int / float / string / option / result / list / bytes /
  matrix / base64 / hex / map / math / set`（12モジュール）に適用した。
- **`error.message` / `error.context` の ICE**：`dispatch_runtime_fallback` に `"error"` 分岐を追加し、`CallTarget::Module` 経由で `emit_call` に戻す。インライン出力は Module 側に存在しており、RuntimeCall 経路にリダイレクトが必要だっただけである。

<a id="matrix-wasm-ops"></a>

### Matrix の WASM 演算

Rust ランタイムのみで、WASM ターゲットでは `[ICE] emit_wasm: RuntimeCall almide_rt_matrix_<op>` を起こしていた3関数に、インライン WASM 出力分岐を追加した。

- `rms_norm_rows(m, gamma, eps)`：mean/beta を使わない layer-norm 変種。
- `attention_weights(q, kt, scale)`：融合した `softmax_rows(scale * q×kt)`。第1段階の作業用ローカル（`q`、`kt`、`scale`、`k`、`acc`、`inner`）を段階間で解放し、第2段階（`row_off`、`maxv`、`sumv`、`v`）で同じ WASM ローカルスロットを再利用する。
- `swiglu_gate(x, w_gate, w_up)`：`out[i,j] = g · σ(g) · u`。両内積が同じ `k` ループを共有する。

`ScratchAllocator` の関数ごとの i32 上限を32 → 48に引き上げ、入れ子の行列パイプライン（SDPA、Llama 1-block）が収まるようにした。

<a id="new-ci-gate-wasm-dispatch-coverage"></a>

### 新 CI ゲート：WASM ディスパッチ網羅率

`tests/wasm_dispatch_coverage_test.rs` は `almide test spec/stdlib/ --target wasm` を実行し、要約を解析して `failed == 0 && skipped <= SKIP_BASELINE` を検査する。基準値は3で、正当な `// wasm:skip` ファイルのみ（ネイティブ限定 `process` API、および panic 捕捉を要する `testing.assert_throws`）である。

<a id="numbers"></a>

### 数値

| 対象 | 変更前 | 変更後 |
|---|---|---|
| WASM spec | 214成功 / 7スキップ | **218成功 / 3スキップ** |
| Rust spec | 221/221 | 221/221（変更なし） |
| モジュール間 `let` | スカラーのみ、参照で不具合 | Swift/TS と完全に同等 |
| `stdlib/` WASM 全体検査の ICE 件数 | 4 | **0** |

<a id="s2-flip--concretizetypes-postcondition-is-now-a-hard-contract"></a>

### S2 切り替え — ConcretizeTypes 事後条件を厳格な契約に

契約上、`expr.ty` は信頼できる。Pipeline ハーネス（`crates/almide-codegen/src/pass.rs`）は `ALMIDE_CHECK_IR` / `ALMIDE_VERIFY_IR` を読まなくなり、パス間 IR 検証と各パスの `Postcondition` 一覧を毎ビルドで実行する。デバッグビルドは違反時に panic する（CI やローカル `cargo test` が未解決型のプログラムを通さないため）。リリースビルドは同じ `[POSTCONDITION VIOLATION]` / `[IR CHECK]` 診断を stderr に出してコンパイルを続けるため、エンドユーザーの `almide build` はコンパイラバグでクラッシュしない。

この切り替え後、下流パス（クロージャ変換、WASM 出力、標準ライブラリ・ディスパッチ、`resolve_list_elem` など）は、`IrExpr.ty` が非 Unknown であることを無条件に前提にできる。v0.14.6–0.14.7で追加した「型がまだ Unknown なら出力時フォールバック X を試す」という防御的な適応処理は、後続作業で削除可能になる。

`ClosureConversion` が生成する WASM ターゲットの lift 済みラムダに残る TypeVar は、既存のパス境界問題として S3（`pass_resolve_calls` 第1b-c段階）で追跡している。デフォルトの spec/ 全体検査では表面化しない。

削除した公開機能：
- `ALMIDE_CHECK_IR` / `ALMIDE_VERIFY_IR` 環境変数（代替なし。契約は常に適用）。

<a id="s3-phase-1d--delete-emit_stub_call--emit_stub_call_named-helpers"></a>

### S3 第1d段階 — `emit_stub_call` / `emit_stub_call_named` ヘルパーを削除

`emit_wasm/calls.rs` の2ヘルパーは、標準ライブラリ・ディスパッチャーの `_` 分岐に到達したとき `[ICE]` メッセージ付きで `panic!` するためだけに存在していた。全呼び出し箇所を、個別のモジュール / ディスパッチャー名を含むインラインの `panic!("[ICE] emit_wasm: no WASM dispatch for `<m>.<func>` — ...")` に置き換え、不足分岐を診断から直接特定できるようにした。呼び出し元がなくなったためヘルパー本体も削除し、`calls.rs`、`calls_io.rs`、`calls_value.rs`、`calls_http.rs`、`calls_datetime.rs`、`calls_process.rs`、`calls_fs.rs`、`calls_env.rs`、`calls_random.rs`、`calls_regex.rs`、`calls_list_helpers.rs` はすべてインライン panic を使う。

挙動は変わらない。ディスパッチャーのフォールバック到達は S2 切り替え時点ですでにコンパイル時 ICE だった。この整理は間接層を1つ除き、エラーメッセージに正しいモジュール / ディスパッチャーを示す。

## [0.14.8] — 2026-04-17

v0.14.7 のリグレッションに対するホットフィックス。外部パッケージのディスパッチ（例：`almide.toml` の `[dependencies]` から読み込む `almai`）がリンクできなかった。第1b段階の `ResolveCallsPass` が、ユーザーパッケージ・モジュールを含むすべての `CallTarget::Module` を、バージョンなしのマングル名に書き換えていたためである。walker はこれらの関数をバージョン付き識別子（`almide_rt_<pkg>_v<major>_<fn>`）で出力する一方、呼び出し箇所はバージョンなし形式（`almide_rt_<pkg>_<fn>`）を参照し、コンパイル時に Rust E0425 を起こした。

<a id="fix"></a>

### 修正

`ResolveCallsPass` は Module → Named 書き換えを `stdlib_info::is_any_stdlib(m)` で制限するようになった。`versioned_name` を持たない同梱 Almide 標準ライブラリ・モジュール（`args`、`path`、`list`、…）のみを書き換える。ユーザーパッケージ呼び出しは `CallTarget::Module` に留まり、`StdlibLoweringPass::rewrite_module_names`（Rust）または WASM の `func_map` の素の名前へのフォールバックを通る。どちらもバージョン付きシンボルの検索方法を知っている。

公開機能の変更なし。`almide test`（206ファイル）と `cargo test` は合格し、下流の `almide-dojo`（`almai.defaults` / `almai.with_system` / `almai.call_with` を使う13テスト）も再びコンパイル・実行できる。

## [0.14.7] — 2026-04-17

第3段階「理想形への移行」の一連の作業。6つの出荷ポイント（`-phase3.1` から `-phase3.5`、および途中の B 修正）をこのリリースに統合した。共通目標は、同梱 Almide / コード生成ディスパッチのパッチ層にある特例をすべて0にすること。このリリース以降、すべての `CallTarget::Module` は TOML 標準ライブラリ関数（ターゲットごとのインライン出力）に解決されるか、`Named`（同梱 Almide 経路）に書き換えられる。未解決の標準ライブラリ呼び出しはコンパイル時 ICE となり、単相化はパス後にすべてのジェネリック原本を削除し、監査は関数の場所を示して残留を検出する。計画と完了の説明は `docs/roadmap/done/bundled-almide-ideal-form.md` と `docs/roadmap/active/codegen-ideal-form.md §Phase 3 Arc` を参照。

<a id="patch-layer-status-at-release"></a>

### リリース時のパッチ層の状態

- 同梱 `option.almd` / `result.almd`（シグネチャ上書き）：**撤去済み**（S1）
- 同梱関数向け WASM `func_map` のモジュール別フォールバック：**撤去済み**（A）
- mono の prune 段階における `is_bundled_module` フィルター：**撤去済み**（S4）
- prune を飛ばす `monomorphize_module_fns` の早期 return：**撤去済み**（B）
- `emit_stub_call*` ランタイム trap：**撤去済み**（S3、現在はコンパイル時 ICE）

<a id="s1--optionresult-signature-normalization"></a>

### S1 — Option/Result シグネチャの正規化

`option.unwrap_or_else` と `option.or_else` の TOML シグネチャを黙って上書きしていた、同梱 `stdlib/option.almd` / `stdlib/result.almd` を削除した。TOML が `Fn[Unit] -> X` を宣言するのに呼び出し側が `() => x` と書くという根本原因を発生源で修正した。TOML は `Fn[] -> X` を使い、`stdlib_codegen.rs` の TOML パーサーが空パラメーターを扱う。

公開機能の変更：

- `stdlib/defs/option.toml` の `unwrap_or_else.f` / `or_else.f`：`Fn[Unit] -> X` → `Fn[] -> X`
- `stdlib/option.almd` / `stdlib/result.almd`：削除（シグネチャ上書きが不要になったため。ランタイム・ディスパッチは元々 TOML ベース）
- `BUNDLED_MODULES` / `AUTO_IMPORT_BUNDLED` / `get_bundled_source`：`option` / `result` の項目を削除。Tier-1 自動インポートは `import_table.rs` のハードコード一覧経由で継続する。

呼び出し側から見える破壊的変更はない。`option.or_else(o, () => ...)` は同梱上書きを経由せず、TOML シグネチャに対して直接型検査される。この相互依存のゲートである `spec/stdlib/coverage_misc_test.almd` は変更なしで合格する。

<a id="s2--concretizetypes-audit-always-on-bundled-stdlib-generic-cleanup"></a>

### S2 — ConcretizeTypes 監査を常時有効化し、同梱標準ライブラリのジェネリックを整理

`ConcretizeTypesPass::postconditions` は `ALMIDE_AUDIT_TYPES=1` による監査の制限をやめ、`Custom(audit_remaining_unresolved)` を毎ビルドで実行する。違反は `[POSTCONDITION VIOLATION] [ConcretizeTypes] N expressions remain ...` と出力し、`ALMIDE_CHECK_IR=1` の下では `panic!` に昇格する。

Rust ターゲットの `spec/` は `ALMIDE_CHECK_IR=1` で問題なし。WASM ターゲットでは `ALMIDE_CHECK_IR=1` の下で、`ClosureConversion` が生む lift 済みラムダの TypeVar 残留にまだ引っ掛かる。この穴を塞ぐのは S3 の作業である。デフォルト挙動（`ALMIDE_CHECK_IR` なし）は変わらず、両ターゲットとも従来どおり spec/ に合格する。

同梱標準ライブラリの mono 整理：`monomorphize_module_fns` は従来、未使用のジェネリック原本関数を `program.modules` に残し、それらが TypeVar を保ったまま WASM エミッターに届いていた。特殊化の後は `is_bundled_module(name)` のジェネリック関数をすべて削除する。特殊化インスタンスは同じ `module.functions` に存在するため、ジェネリック原本は不要である。

<a id="s3--wasm-emit_stub_call-panics-at-compile-time"></a>

### S3 — WASM `emit_stub_call*` をコンパイル時 panic に

`emit_stub_call_named` と `emit_stub_call` は引数を捨てて実行時 `unreachable` 命令を出すのをやめ、`[ICE]` 接頭辞付きの `panic!()` を起こす。WASM 出力時に到達するのは、`module.func` 呼び出しが TOML または同梱 IR の呼び出し先を持たないまま `pass_resolve_calls` を通過したことを意味する。これはランタイム trap としてデバッグする問題でなく、resolver で修正すべきコンパイラバグである。

`ALMIDE_WASM_STUB_PANIC` / `ALMIDE_WASM_STUB_VERBOSE` / `ALMIDE_WASM_STUB_TRACE` 環境変数を削除した。v0.14.6 の stub-panic 全体検査ですでに spec/ + nn が stub に到達しないと確認した。この手順で codegen-ideal-form ロードマップの第1c段階を閉じる。

<a id="s4--mono-drops-generic-source-fns-from-every-module-not-just-bundled"></a>

### S4 — 同梱だけでなく全モジュールから mono のジェネリック原本関数を削除

`monomorphize_module_fns` は元々、`program.modules` の全モジュールにわたってジェネリック関数を発見・特殊化していた（v0.14.6 で導入した同梱限定フィルターは特殊化後の prune にのみ適用）。prune を全モジュールで統一し、特殊化後に `is_bundled_module(...)` 内だけでなく、すべてのジェネリック原本関数を削除する。ジェネリック関数を持つユーザーパッケージ・モジュール（例：`pkg.helper[T](x: T) -> List[T]`）も同梱標準ライブラリと同じ mono 後の不変条件を得る。

<a id="b--mono-prune-always-runs-concretizetypes-audit-is-more-locatable"></a>

### B — mono の prune を常時実行し、ConcretizeTypes 監査の場所を分かりやすく

`monomorphize_module_fns` は従来、ジェネリック特殊化を発見しなければ早期 return していた。そのため、同梱標準ライブラリ・モジュールをインポートするが、そのジェネリック関数を1つも呼ばないという、特に重要な場合にループ後の prune を飛ばしていた。未使用のジェネリック原本がコード生成まで残り、本体に `TypeVar(T)` を持って WASM の `ConcretizeTypes` 監査を失敗させた。修正では prune を常に実行し、書き換えループ（`rename` が空なら何もしない）のみ条件付きで省く。

`audit_remaining_unresolved`（`Custom` 事後条件）は、不透明な `Discriminant(NN)` 番号の代わりに、違反する各式を囲む関数名と短い `kind` ラベル（「[list::iterate] List ty=...」）を報告する。

修正後、WASM の spec/ は `ALMIDE_CHECK_IR=1` で **191/206成功、15スキップ**。残る15件は独立した型推論の穴（空リストの `Applied(List, [Unknown])`、OpenRecord の伝播、codec 導出リストフィールド、ジェネリック chain-b 引数など）で、`codegen-ideal-form.md §#4` で追跡する。デフォルト挙動（`ALMIDE_CHECK_IR` なし）は変わらず、両ターゲットとも従来どおり spec/ に合格する。

<a id="a--phase-1b-resolvecalls-rewrites-bundled-stdlib-calls-to-named"></a>

### A — 第1b段階：ResolveCalls が同梱標準ライブラリ呼び出しを `Named` に書き換え

`pass_resolve_calls.rs` は検証専用ではなくなった。各 `CallTarget::Module { module, func }` に対して次を行う。

- TOML 標準ライブラリ（例：`list.map`、`option.unwrap_or_else`）：`Module { module, func }` のまま残し、ターゲット別ディスパッチャーが引数の装飾 / インライン出力（Rust の `pass_stdlib_lowering`、WASM の `emit_call`）を適用できるようにする。
- 同梱 Almide 標準ライブラリ（例：`stdlib/list.almd` に定義され mono で特殊化された `list.split_at`、`list.iterate`）：コード生成が登録したマングル済みシンボル `CallTarget::Named { name: "almide_rt_<m>_<f>" }` に書き換える。両バックエンドは元々その名前で同梱関数を登録しており、追加ディスパッチ論理は不要。
- TOML にも同梱 IR 関数にも該当しない：事後条件違反。以前は実行時 trap に先送りした未解決標準ライブラリの穴が、`ALMIDE_CHECK_IR=1` の下ではコンパイル時 ICE となる。

v0.14.6 でパッチとして追加した WASM の `_ if module == "list"` 分岐の同梱関数フォールバック（`func_map.get("almide_rt_list_*")`）を削除した。上記書き換えにより、同梱関数はそもそも Module ディスパッチ分岐に届かず、フォールバックは不要になった。

`bundled-almide-ideal-form.md` を `done/` に移動し、一覧の全5項目が完了した。

## [0.14.6] — 2026-04-16

「LLM-first 言語」ロードマップの第2段階。**重点**：診断でコピー＆ペースト可能な修正コードを出し、LLM の再試行をより速く収束させる。[almide-dojo] の30タスク・ベンチマークで測定した。

<a id="llm-writability-dojo-msr-2026-04-16"></a>

### LLM の書きやすさ（dojo MSR、2026-04-16）

| モデル | v0.14.5 基準 | 0.14.6 | Δ |
|---|---|---|---|
| Sonnet 4.6 | — | **30/30 (100%)** | — |
| llama-3.3-70b | 17/30 (57%) | **23/30 (77%)** | **+6 (+20pt)** |
| llama-3.1-8b | 13/30 (43%) | 10/30 (33%) | -3（ノイズ帯内、σ≈2） |

Sonnet の30/30は中核設計を裏付ける。SOTA に達するための言語側の譲歩（UFCS、命令的ループ）は不要だった。全分析は [docs/roadmap/active/llm-first-language.md](../../roadmap/active/llm-first-language.md) を参照。

<a id="added--diagnostics"></a>

### 追加 — 診断

以下の各項目には、コピー＆ペースト可能なコードを含む `try:` ブロックを添える。

- **E001 関数本体の Unit 漏れ**：AST から末尾 `let`/`var` の名前を抽出し、コード例を `<computation>` プレースホルダーではなく `let <real_name> = ... / <real_name>` とする。
- **E001 if 分岐の Unit 漏れ**：一方の分岐が裸の代入なら、実際の変数をコード例に記す（`let new_val = if cond then … else val`）。
- **E002 メソッド呼び出し構文**：`x.to_upper()` → `string.to_upper(x)`。あいまい検索の提案がある場合のみコード例を出す（候補が見つからない場合は出さない）。
- **E002 未定義関数**：あいまい一致の提案と、正しい名前への修正をコード例に示す。自由形式の別名（例：`xs + [x]`）ではコード例を正しく抑制し、呼び出し位置へ呼び出しでない断片を貼り付けるのを避ける。
- **E003 未定義変数**：インポートを提案できる標準ライブラリ・モジュールなら `import json` を、そうでなければあいまい検索による名前変更を提案する。
- **E004 引数数の不一致**：`<name: Type>` プレースホルダー付きの全シグネチャを示す（`string.join(<list: List[String]>, <sep: String>)`）。
- **E009 let への再代入**：実際の束縛名を使い `var <name> = ...` を提案する。
- **ハルシネーションに特化したコード例**：
  - `int.sqrt(n)` → 変換・平方根・変換（`float.sqrt(int.to_float(n))`）。
  - `int.gt(a, b)` / `int.lt` / `int.gte` / `int.lte` / `int.eq` / `int.neq`（および float/string/bool の変種）→ 演算子対応表。
- **位置が不適切な `test` キーワード**：考えられる2原因を両方示す（先行宣言が閉じていない、またはハーネスが送ったコードにテストを含めるべきでない）。
- **rest/cons パターン**：`[head, ...tail]` / `[h, ..t]` / `head :: tail` に対し、Almide で唯一の慣用形である `list.first` / `list.drop` の再帰を示す専用ヒントを出す。
- **`while cond do ... done`**（Pascal/Ruby/OCaml 形式のループ）検出は、再帰形式（pure/effect 関数向けに推奨）と Almide の `while cond { ... }` 形式（`var` アキュムレーター用）の両方を示す、より充実した `try:` コード例を出す。dojo の二分探索 / 行列演算で `do ... done` が一貫して初回試行に現れ、再試行では再帰形式が成功したことが動機。

<a id="added--tooling-2"></a>

### 追加 — ツール

- **`almide ide outline <file|@stdlib/<module>>`**：宣言ごとに1行の要約（fn / type / let）。LLM による API 探索で `grep` の代わりに使うことを狙う。
- **`almide ide doc <symbol> [--file <f>]`**：標準ライブラリまたはユーザーシンボルのシグネチャと docstring。`string.to_upper` / `greet` が同じように動く。
- **`almide ide stdlib-snapshot [--modules m1,m2,...] [--json]`**：中核標準ライブラリの outline を連結したテキスト / JSON。N 回ではなく1回のサブプロセスで済む。ハーネスが SYSTEM_PROMPT に埋め込むための設計で、デフォルト7モジュールはテキスト約3.5Kトークン（JSON は14.5K）と測定。
- ダッシュボード / 自動化向けに `almide ide outline` の **`--json` フラグ**。
- **スナップショットテスト**：`almide ide outline @stdlib/*` と `stdlib-snapshot` の出力形式を固定し、標準ライブラリ変更で下流ハーネスの SYSTEM_PROMPT 埋め込みが黙って劣化するのを防ぐ。

<a id="fixed--parser"></a>

### 修正 — パーサー

- **改行をまたぐ let-in 検出**：`let x = expr\n  in <body>` で「Expected expression (got In 'in')」へ連鎖するのではなく、OCaml/Haskell のヒントを出す。部分的な `Stmt::Let` は AST に保持し、下流診断（E001）が実際の束縛名を引き続き参照できるようにする。
- **rustc エラーコード漏出時のラップ**：rustc の4桁 `error[E\d{4}]` コード（Almide 自身は出さず、Almide は3桁 E001..E099 まで）を、出力に `src/main.rs` がなくても常にバグ報告バナーで包む。ハーネスの分類器がコード生成バグをユーザー向け言語エラーと誤認するのを防ぐ。

<a id="changed-1"></a>

### 変更

- `almide ide outline` の `--filter` は部分文字列に一致する（文書では「接頭辞」とされていたが、実装は元々部分文字列だった。文書を挙動に合わせた）。

<a id="added--phase-3-mvp"></a>

### 追加 — 第3段階 MVP

- **`almide ide doc`** は `@stdlib/<module>.<fn>` 接頭辞を `<module>.<fn>` の別名として受け付け、`almide ide outline @stdlib/<module>` と使い勝手を揃える。
- **`almide fix` の終了コード**：自動修正後にファイルが問題なければ0、`manual_pending` が空でなければ1を返す。ハーネスは出力を解析せず終了コードで再試行を制御できる。
- **診断説明文書を充実**（`docs/diagnostics/E001.md` から `E013.md` までの全件）：コンパイラが実際に出す `try:` コード例の形、よくある型不一致の変換表、`llms.txt` / `CHEATSHEET.md` への相互参照をそれぞれ追加した。一部文書は内容が不一致だった（E010 は「scope error」と説明していたがコードの意味は**非網羅的 match**、E011 は「init issue」と説明していたが意味は **pure 関数内のクロージャの可変 var**）。これらを訂正した。チェッカーが出していたが未文書化だった E012（重複定義）と E013（非レコードへのフィールドアクセス）も追加した。
- **`almide fix <file> [--dry-run] [--json]`**：`auto_imports` を適用（不足した `import json` / `import fs` などを追加）し、OCaml 形式の `let x = expr in <body>` のキーワードを除去（本体は残す）、比較関数呼び出しを書き換え（`int.gt(n, 0)` → `n > 0`、int / float / string / bool の lt / gte / lte / eq / neq も同様）、**`return` キーワードを除去**する（Go/Rust/JS の習慣。Almide は末尾式を使い、複数箇所は不動点まで反復修正）。残った `try:` コード例付き診断は手動修正箇所として報告する。`--json` は LLM ハーネスの再試行ループ統合用に、安定スキーマの報告（`schema_version`、imports_added、letin_removed、operator_rewrites、**return_removed**、manual_pending、changed、dry_run）を出す。cons パターンの書き換えはまだ手動（AST レベルのパターン変換と、失われた本体のパーサー回復が必要）。
- **`list.binary_search(List[Int], Int) -> Option[Int]`**：ソート済みリストの二分探索。dojo の二分探索タスクは従来70bで失敗していたが、API 呼び出しに縮められる。
- **`string.run_length_encode(String) -> List[(String, Int)]`**：RLE のペア。同じ動機。
- リポジトリルートの **`llms.txt`**：ミッション、CLI リファレンス、中核的慣用形、標準ライブラリ参照先、診断コード、アンチパターン。LLM ツールが1 URL で取得できる入口。

<a id="known-gaps-documented-not-blockers"></a>

### 既知の不足（文書化済み、ブロッカーではない）

- `stdlib/<m>.almd` の同梱ソースで既存 `list.*` / `string.*` モジュールを拡張するのは現時点では動かない。`list.*` lowering は関数が TOML / Almide のどちら由来でも `almide_rt_list_<fn>` を出すようハードコードされている。回避策は TOML + ランタイム（`stdlib/defs/<m>.toml` + `runtime/rs/src/<m>.rs`）として関数を追加すること。完全な Almide ソースのディスパッチは第3-2.2段階。
- `almide fix` はまだ `let-in` / `head :: tail` / 演算子形式の書き換えを機械適用しない。try: コード例を示すが手編集が必要。第3-1.2段階。
- `llms.txt` は手書きで、正本文書（SPEC / cheatsheet）からの自動生成は未対応。第3-3.2段階。

<a id="added--almide-docs-gen---check-doc-drift-guard"></a>

### 追加 — `almide docs-gen --check`（文書の乖離防止）

`llms.txt` と `docs/diagnostics/` が正本ソースに追随していることを検証する整合性チェック。MVP は4軸を扱う。

- **バージョン**：`Cargo.toml` のバージョン文字列が `llms.txt` に存在しなければならない。
- **llms.txt が参照する診断コード**：`docs/diagnostics/` 配下の全 `EXXX` ファイルが `llms.txt` に記載されなければならない。
- **自動インポートされる標準ライブラリ**：`almide_lang::stdlib_info::AUTO_IMPORT_BUNDLED` の全モジュールが、`llms.txt` の「Fast facts」に記載されなければならない。
- **診断レジストリの全単射**：コンパイラソース内の全 `with_code("EXXX")` に対応する `docs/diagnostics/EXXX.md` が必要であり、各文書も実際に出されるコードに対応しなければならない。

失敗時は乖離を箇条書きで報告し終了コード1を返す。`cargo test` の結合テスト `docs_gen_check_passes_on_clean_checkout` により、正本を変更して文書を忘れた PR はすべて CI で失敗する。

完全生成（乖離検査だけでなく）は `docs/roadmap/active/llms-txt-autogen.md`、registry と emit の統一方針は `docs/roadmap/active/diagnostic-emit-doc-unification.md` に範囲を定める。

初回実行で発見・修正した実際の乖離：
- `llms.txt` の `E010-E013` 範囲行は部分文字列として `E011` / `E012` を含んでいなかった（範囲圧縮が内容の代わりになっていた）。展開した。
- `E420`（関数可視性違反）はコンパイラが出していたが文書がなかった。`docs/diagnostics/E420.md` を追加し、コード番号が連番外で将来リリースの番号変更候補であると記した。

<a id="added--bundled-almide-stdlib-dispatch-infrastructure"></a>

### 追加 — 同梱 Almide 標準ライブラリのディスパッチ（基盤）

`stdlib/<module>.almd` ファイルは、Almide で書いた新関数により TOML ベースの標準ライブラリ・モジュールを拡張できるようになった。従来は黙って失敗し、コード生成が全 `module.func` 呼び出しに `almide_rt_<m>_<f>` を出していたため、同梱の `fn binary_search_v2` が存在しないランタイム関数を呼んでいた。

修正は4層から成る。

1. **フロントエンドの解決**（`src/main.rs`）：`is_stdlib_module(name) == true` でも同梱標準ライブラリ・モジュールを IR に lowering する。TOML のみの標準ライブラリは引き続き早期終了する。
2. **TOML 重複の除去**（`src/main.rs`）：TOML ランタイムと同名の同梱関数を削除する。それらは rt_ 経路を通り、同梱専用関数は残る。
3. **IR 検証**（`almide-ir`）：`known_module_functions` で同梱標準ライブラリ・モジュールを省くため、同梱本体から TOML 関数を呼ぶ場合（例：`result.collect` から `list.is_empty`）に「unknown function」エラーにならない。
4. **コード生成ディスパッチ**（`pass_stdlib_lowering`）：同梱専用 `(module, func)` ペアのレジストリをパスごとに構築する。それらについては `Module → almide_rt_*` 書き換えを抑制し、呼び出しを `Module` のまま保って、walker が通常のユーザー関数呼び出しを出力する。

既存の同梱 `option`/`result` の `.almd` ソースには隠れた役割があった。コード生成はそれらを除く（ランタイム・ディスパッチは TOML 優先）が、型チェッカーは読み込み、**TOML よりもそのシグネチャを優先する**。そのため `option.or_else(o, () => ...)` は TOML の `Fn[Unit] -> X` ではなく、同梱の `fn() -> X` に対して型検査される。削除するとテストが壊れる。統一への道筋は `roadmap/active/option-result-bundled-cleanup.md` を参照。

<a id="added--bundled-list-fns-real-users-of-the-dispatch-path"></a>

### 追加 — 同梱 `list.*` 関数（ディスパッチ経路の実利用）

`stdlib/list.almd` は3関数を提供し、Rust と WASM の両ターゲットで `spec/stdlib/list_bundled_test.almd` がカバーする。

- `list.bundled_probe(n)`：スモーク回帰防止。
- `list.split_at(xs, n) -> (List[T], List[T])`：インデックス `n` でリストを分割する。同梱関数から既存 TOML 関数（`list.take`、`list.drop`）を呼ぶ例。
- `list.iterate(seed, f, n) -> List[T]`：長さ `n` の `[seed, f(seed), f(f(seed)), ...]` を構築する。同梱ディスパッチ経路を端から端まで使う純 Almide 再帰。

<a id="changed--monomorphize-extended-to-module-defined-generics"></a>

### 変更 — `monomorphize` をモジュール定義のジェネリックへ拡張

IR モジュール内に宣言されたジェネリック関数（例：`stdlib/list.almd` の `list.split_at[T]`）も、トップレベル `program.functions` に加えて単相化パスで特殊化する。呼び出し先は `CallTarget::Module { module, func }` のままなので、全バックエンドでコード生成は同じ標準ライブラリ・ディスパッチを通る。`list.split_at([1, 2], 2)` が `TypeVar` を残したままエミッターに届き、要素サイズが i32 にフォールバックする WASM の穴を閉じる。特殊化本体内の自己再帰 `Named` 呼び出しは `specialize_function` 自身が書き換え、トップレベル関数については引き続きトップレベル rewrite_calls が正本となる。

ロードマップ：残る負債（ディスパッチ入口の統一、`stub_call → unreachable` の廃止、`ConcretizeTypes` の厳格な事後条件、option/result シグネチャの正規化）は `active/bundled-almide-ideal-form.md` を参照。

基盤のみのため MSR の変化は見込まない。下流作業（`diagnostic-snippet-externalization`、Almide による自動書き換え規則）の障害を解消する。

<a id="internal-refactors-no-behavior-change-no-msr-effect"></a>

### 内部リファクタリング（挙動変更なし、MSR への影響なし）

- **`almide fix` のキーワード除去規則**を1つの `KeywordRemoval { keyword, diag_matches, max_iter }` エンジンに統合した。`let-in` と `return` の規則はデータ駆動となり、第3の削除規則は const 項目1つと呼び出し箇所の追加で済む。従来2箇所にあった `word_boundary_ok` を抽出した。
- **比較演算子表**（`int.gt` / `.lt` / `.eq` など → `>` / `<` / `==`）を `almide::stdlib::comparison_operator_of` に集約した。従来同じ対応が `suggest_alias`、`try_snippet_for_alias`、`cli/fix.rs::comparison_fn_to_operator` に重複していたが、各箇所が1つの正本関数から導出するようになった。副次的に、`string.eq` と `bool.eq` にも「Did you mean」ヒントが付く（従来 `suggest_alias` に欠けており、統合時に発見した）。
- **while-do の `try:` コード例**を、従来の Option A/B ブロック（≈15行）から7行へ短縮した。簡潔な `while` 形式1つと `fn loop` 再帰のひな型1つにする。振り返りでは、長い形式は選択肢の逆説を招き、再試行コンテキストを膨らませる恐れがあるとされた。

<a id="deferred-evidence-based"></a>

### 延期（証拠に基づく）

- `almide ide peek-def` / `find-refs`：dojo のコンテキストは「既存本体を調べる」ワークフローを使わないため MSR 改善は見込まない。タスク集にリファクタリング形式のタスクが増えた時点で再検討する。
- UFCS 採用：dojo データでは UFCS の利点は弱いモデルのみ（8b parse-err 9 に対し70b parse-err 0）。現構文での Sonnet 30/30は強いモデルには不要だと示す。[docs/roadmap/active/llm-first-language.md](../../roadmap/active/llm-first-language.md) の `Path A` を正式に却下した。

---

<a id="before-this-file-existed"></a>

## このファイルができる前

- **v0.14.5**：LLM-first ロードマップの基準（dojo 70b 17/30、8b 13/30）。
- **v0.14.x 以前**：git 履歴を参照。タグ付きバージョンのリリースノートは GitHub Releases にある。

[almide-dojo]: https://github.com/almide/almide-dojo
