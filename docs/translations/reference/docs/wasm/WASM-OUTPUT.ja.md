<a id="wasm-output--whats-in-the-binary-and-why"></a>

# WASM 出力 — バイナリに何が入るのか、その理由

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](WASM-OUTPUT.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](WASM-OUTPUT.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](WASM-OUTPUT.zh-CN.md)

> [正本](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/wasm/WASM-OUTPUT.md)の非規範な履歴参照用スナップショット。固定コミット：35297a612fb29e3f78f6587aba253aa91d51f466。自動同期されない。安全性・証明・測定・実装の主張はここで独立に再検証しておらず、検証コマンドも実行していない。正本を優先する。本文の証明書検査は記載された証明書とチェッカーの機構を指し、第三者による安全認証を意味しない。

Almide は WebAssembly を**直接**出力する。LLVM、Cranelift、wasm-bindgen は使わず、モジュール内にコンパイル済み標準ライブラリのオブジェクトコードも含めない。renderer は **structural 経路**（crates/almide-wasm、wasm-encoder、`--target wasm` 経路）だけである。**旧来の v1 経路**（証明書付き MIR→WAT renderer）は #2752 で最後の経路を失い、#2761 で削除された。以下でこれに言及する測定は歴史的なもので、現在のコンパイラでは再現できない。この文書は実際のモジュールをバイトごとに分析し、サイズの主張が何を意味するか正確に述べる。

代表値である Hello, world のバイト数は CI 由来：`docs/benchmarks/wasm-size.txt`（`scripts/gen-readme-stats.sh` が再測定、#1605 以降ゲート化）。以下のその他の値は、2026-08-27 に当時の `develop` コンパイラ、`wasm-opt`（Binaryen）132、`wasm-objdump`（WABT）1.0.41 で測定した。Rust 比較行は 2026-07-23 の測定（`rustc 1.94.1`）を保持する。末尾のコマンドで各値を再現できる。

<a id="the-headline-numbers-measured-2026-08-27-the-incumbent-columns-are-historical-retired-by-2761"></a>

## 代表的な数値（2026-08-27 測定。旧来経路の列は歴史的なもので、#2761 で廃止）

| プログラム | structural（出荷） | structural + `-Oz` | 旧来 v1（廃止） | 旧来 + `-Oz`（廃止） |
|---|---:|---:|---:|---:|
| Hello, world | 4,459 B | **364 B** | 1,096 B | 788 B |
| FizzBuzz 1–100 | 4,716 B | **1,162 B** | 2,168 B | 1,346 B |
| Fibonacci（再帰） | 4,490 B | **894 B** | 1,791 B | 1,035 B |
| 再帰ジェネリック ADT repr¹ | **14,088 B** | **9,320 B** | 34,723 B | 21,883 B |

¹ `spec/wasm_cross/compound_repr_recursive_interp.almd` — 再帰 ADT、相互再帰レコード、ジェネリックの具体化、およびそれらの完全な `${…}` repr 機構。

**逆転を 1 段落で説明すると。** structural 経路は約 3.5 KB の固定ランタイム前置部（allocator 群、COW ゲート、itoa scratch、OOM 経路 — Hello, world モジュールで 38 個の補助関数）を持つため、ほぼ空のプログラムでは生モジュールが旧来経路より大きい。実際のプログラムでは逆転する。ADT-repr 行は structural が 14.1 KB、旧来が 34.7 KB。600 以上の fixture コーパス全体では greenfield VERDICT が **structural 合計 4.11 MB、旧来 10.54 MB** を記録する。さらに `wasm-opt -Oz` 後は、測定した全行で structural モジュールが 4 列中最小になる。前置部は静的には到達可能（モジュール自身の呼出しグラフが大半には入らない表駆動 allocator）であり、`-Oz` のモジュール全体の DCE が、renderer 内の到達可能性パスでは保守的に残す必要のある部分を削除する。Hello, world は 4,459 → 364 バイトへ減少し、関数は **1 つ**だけ残る。

誤解を避けるための 2 つの位置付け：

- 7 月の Rust 比較は文脈として有効である。Rust の `wasm32-wasip1` Hello, world は既定 64,430 B、完全なサイズ最適化 profile（`opt-level="z"`、`lto`、`strip`、`panic="abort"`、`codegen-units=1`、2026-07-23 測定）で 40,754 B。この差はツールチェーンの最低限の負担の違い — Rust は各 `println!` に `std` の書式化機構をリンクする — であり、言語の品質を評価するものではない。
- **出荷される** Almide バイナリは検証されたものである（後述）。`wasm-opt` 列には明示的な `--wasm-opt` opt-in が必要であり、それを使うとモジュールは検証範囲の外へ出る。

<a id="section-anatomy--hello-world-structural-incumbent-row-historical"></a>

## セクション構成 — Hello, world（structural。旧来経路の行は歴史的なもの）

出荷形態の structural 経路（4,459 B、`wasm-objdump -h` による）：

| セクション | サイズ | 内容 |
|---|---:|---|
| Type | 113 B | 19 個の signature |
| Import | 179 B | 5 個の WASI import（`fd_write`、`proc_exit`、`random_get`、`clock_time_get`、`fd_read`） |
| Function | 40 B | 39 関数の index table |
| Table + Elem | 6 B | funcref table（ここではクロージャなしのため空） |
| Memory | 3 B | 線形メモリ 1 つ |
| Global | 97 B | 14 global（heap cursor、free-list head、park-buffer 状態など） |
| Export | 35 B | `_start` + memory |
| Code | 3,694 B | 39 本体：`main` + 38 関数の前置部（サイズクラス付き allocator 群、RC/COW ゲート、itoa、文字列等価性、WASI park-buffer の接着処理） |
| Data | 261 B | 文字列 pool（`"Hello, world!"` + OOM/trap メッセージ） |

同じモジュールの `wasm-opt -Oz` 後（364 B）：Type 12 B、Import 35 B（`fd_write` のみ残る）、Function 2 B、Memory 3 B、Global 8 B、Export 35 B、Code 92 B（関数 **1 つ**）、Data 149 B。これが structural 前置部の全貌である。リンクされた補助コードであり、これほど小さなプログラムでは、モジュール全体の optimizer がほぼすべてを dead と証明する。（「tier-0 preamble」案 — 動的割当てに到達しないことを DCE が証明したとき renderer 内で allocator 群を落とす — なら、検証範囲を外れずに出荷サイズ 4.4 KB を 364 B に大きく近づけられる。導入されれば、この文書で測定する。）

旧来の v1 経路（歴史的 — #2761 で廃止、1,096 B）：Type 26 B、Import 70 B（WASI import 2 個）、Function 9 B、Memory 3 B、Global 38 B、Export 40 B、Code 739 B（8 関数 — `alloc`、`rc_dec`、`main`、`print_str`、`_start` と helper 3 個）、Data 46 B。さらに 98 バイトの `name` custom section（関数名のみ、local は除去。wasmtime trap の backtrace に表示されるので、そのバイト数に値する）。7 月測定からの旧来経路の +393 B（703 → 1,096 B）は、C-320 の一連の変更で導入した決定論的 metering と stdin 処理である。

<a id="where-the-stdlib-went"></a>

### stdlib はどこへ行ったか

Almide の stdlib は 49 モジュール、1310 関数だが、**Almide 自身で実装され**、要求に応じてリンクされる。コンパイラは lowering 済みプログラムを走査して呼び出す dispatch 名（`string.len`、`map.set`、`list.sort_by` など）を調べ、一致する self-host ソースだけをリンクし、リンクした関数の呼出し先も含まれるよう不動点まで反復する。Hello, world がリンクする stdlib 関数は **ゼロ**、FizzBuzz は `int.to_string` の背後にある少数だけをリンクする。未使用モジュールは何も追加しない。取り込んだものでも到達不能と分かれば、アセンブル前に到達可能性 DCE が除去する。

<a id="why-the-value-model-stays-small"></a>

### 値モデルが小さく保たれる理由

- **i64 に統一した slot。** すべての scalar は i64、すべてのヒープ値は i32 handle が指す長さ接頭辞付きブロック。型ごとのレイアウトも metadata もない。
- **variant は `tag @ slot 0`。** `match` は整数比較へコンパイルする。vtable も型記述子もない。
- **単相化 → 直接呼出し。** ジェネリックはコンパイル時に特殊化する。間接呼出しは、単一の funcref table を通すクロージャだけ。
- **2 層の到達可能性 DCE。** 要求駆動 linker が、どの self-hosted stdlib ソースをプログラムへ入れるか決定し、その後 renderer の到達可能性パスが、モジュールをアセンブルする前に未到達 helper、import、data segment を落とす。

<a id="whats-still-not-the-smallest-possible-module-and-why"></a>

## まだ最小限のモジュールではない部分と、その理由

検証パイプラインは**自身の描画処理が生成したバイト列を出荷する**。既定経路でビルドする各モジュールは出力時に検証される。structural 経路は証明書 bundle も記録する。`almide verify` と証明書検査対象コーパスのゲートが検査するのは、対応する core 成果物であり、component ではない。witness のカバレッジと記録への信頼には制限があり、成果物全体のメモリ安全性の証明ではない（[メモリ安全性](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/design/MEMORY-SAFETY.md) を参照）。`wasm-opt` は種類が異なる。**renderer の完成出力に適用する、外部の未検証変換**であり、実行すれば、パイプラインが生成したバイト列を、別の未認証ツールが書き換えたバイト列で置き換える。この境界ゆえに opt-in のままである：

```bash
almide build app.almd --target wasm --wasm-opt   # runs: wasm-opt -Oz
#   --enable-nontrapping-float-to-int --enable-tail-call --enable-bulk-memory
#   --enable-mutable-globals
#   (the features the structural leg actually emits — no SIMD in the default output;
#    bulk-memory is the structural leg's memory.copy and mutable-globals its
#    exported globals, #1616)
```

コードコメントの説明：wasm-opt -Oz を実行する。指定する feature は structural 経路が実際に出力するもの。既定出力に SIMD はない。bulk-memory は structural 経路の memory.copy、mutable-globals はその exported global に対応する（#1616）。

**`-Oz` はそのバイト数と引き換えに速度を落とす。** 境界検査を先に済ませた guard 付き高速経路へ hot loop を複製していた旧来 renderer（現在は #2761 で廃止）では、`-Oz` のコード統合がほぼ同じ複製を検査付きループ 1 つへ戻し、spectralnorm で約 3 倍遅いと測定された。サイズが重要な cold code に `-Oz` を使い、計算 kernel へ適用する前に benchmark を行うこと。

<a id="determinism-and-the-cross-target-contract"></a>

## 決定性とクロスターゲット契約

出力バイト列は**ホストアーキテクチャ間で決定的**である。native（x86-64/aarch64）としてビルドしたコンパイラと wasm32（playground）としてビルドしたコンパイラは、クロスターゲット fixture コーパスについてバイト同一のモジュールを生成する。これは目標ではなく CI ゲート（`scripts/check-host-determinism.sh`）である。また両ターゲットでコンパイルできる各プログラムは native ⇄ wasm で **stdout/stderr/exit code がバイト同一**となり、[docs/contracts/](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/contracts) に契約ごとに記録する。

<a id="the-embedded-hosts-call-stack-budget-3435"></a>

### 組込みホストのコールスタック予算（#3435）

ターゲットのコールスタック資源を超える再帰は、native と組込みホストの両方で、定義された abort `Error: stack overflow` + exit 1 になる（C-196、ALS-T6）。発生する深さは各ターゲット固有であり、等しく揃えず宣言する。組込み上限の設定は、出荷する経路での選択である。`almide run --target wasm`、`almide
bench --target wasm`、`almide test` の wasm 経路の背後にある組込みホストは、wasmtime の `max_wasm_stack` を **8 MiB**（`crates/almide-wasm-run/src/host.rs` の `EMBEDDED_WASM_STACK`）に設定する。これは native の main thread スタックのサイズである。各 guest と各 `fan` worker instance は native スタック 64 MiB の host thread 上で動かす。wasmtime は wasm 予算と host frame の合計より大きい host スタックを必要とするためである。wasmtime 自身の既定は 512 KiB で、ホストも #3435 まではそれを使っていた。

2026-10-06 測定（macOS aarch64、release build。まだ応答できる最も深い引数）：

| 再帰の形 | native | 組込み、512 KiB | 組込み、8 MiB |
|---|---:|---:|---:|
| `T!` を返す非末尾の木走査（#3434 の `infer`） | ~43,000 | ~5,400 | ~87,000 |
| frame ごとに heap local が 4 個ある再帰 | ~32,500 | ~5,400 | ~87,000 |
| 木上の `1 + depth(f)` | 上限なし（LLVM がループ化） | ~32,500 | ~523,000 |

stock runtime は独自の上限と失敗形式を保つ。残存差分 C-196 は次を宣言する。`wasmtime run` は既定 512 KiB（`-W
max-wasm-stack=N` で増やせる）で、枯渇は独自の `wasm trap: call
stack exhausted`、exit 134 として報告する。browser はそれぞれ独自に設定する。

<a id="measuring-allocation-the-watermark-and-the-counter-2407"></a>

## 割当ての測定：watermark と counter（#2407）

structural 経路のすべてのモジュールは bump-heap pointer を `__heap` global として export する（region window が巻き戻す場合は `__heap_high` も）。組込みホストは実行後にこれを読み、`crates/almide-wasm/tests/alloc_ledger.rs` が fixture ごとに `crates/almide-wasm/tests/golden/alloc-baseline.txt` へ固定する。これが **watermark** である。元からモジュールが維持する global なので追加費用はなく、**ピーク値**である。行の値は `pool size + the highest the heap ever reached`。ピークでは churn（割当てと解放の繰返し）は分からない。allocator はサイズクラスの free-list を持つので、反復ごとに新しいブロックを割り当て解放するループでも、watermark は単一割当てとほぼ同じになる。

そのための計測器が **counter** であり、native の `ALMIDE_ALLOC_COUNT`（#2228）の wasm 版である。出力時の switch：

```bash
ALMIDE_WASM_ALLOC_COUNT=1 almide run app.almd --target wasm
# stderr, after the program's own output:
# __ALMD_WASM_ALLOC allocs=2000 reused=1996 bytes=9780 frees=2000 heap_end=65872
```

コードコメントの説明：プログラム自身の出力の後、stderr に counter 行を出力する。

| フィールド | 数えるもの |
|---|---|
| `allocs` | すべての `$alloc` 呼出し（free-list hit または bump） |
| `reused` | サイズクラス free-list の pop が処理した `$alloc` 呼出し（残りは bump head を進めた） |
| `bytes` | 要求した payload 長の合計 |
| `frees` | すべての `$free` 呼出し（free-list に格納、または小さすぎる／大きすぎるため放棄） |
| `reclaimed` | region window の restore が `$free` なしで一括回収したブロック（#1961） |
| `live` | `allocs − frees − reclaimed`：解放されなかったヒープブロック（後述） |
| `heap_end` | 同じ行の watermark。2 つを併読できる |

switch の有効時、emitter は top-let global の後ろに 5 個の `i64` global を追加する（既存 index は動かず、ヒープレイアウトは 1 バイトも変わらない）。`$alloc` / `$free` / region window の restore がそれらを増やす。ホストは `__alloc_count`、`__alloc_reused`、`__alloc_bytes`、`__free_count`、`__region_reclaimed` として読む。**既定の off では何も出力しない**。モジュールはその switch を知らないビルドとバイト同一であり、サイズ ratchet も alloc 台帳も動かず、証明を転記した runtime tree（`proofs/StructuralRuntime.v`）は出荷形態のまま。計測を有効にしていないモジュールは counter をゼロではなく不在として報告する（`RunResult::alloc_count` は `None`）。

counter は watermark の隣で台帳化する。同じ `ALMIDE_UPDATE_ALLOC=1` 再生成が `crates/almide-wasm/tests/golden/alloc-count-baseline.txt` を書く（fixture ごとに `allocs reused
bytes frees`、`~` は較正で除外、`!` は拒否）。検査実行は、計測ありモジュールを計測なしと照合して同じ stdout、同じ watermark を要求するので、計測器が測定対象を乱さないことを証明する。

**終了時の live。** Native は値がスコープ外へ出るとすべて drop する。wasm 経路は手動 release なので、release の欠落は出力 parity では見えない wasm 固有の不具合となる（#2932、#2944）。`live` は解放されなかったブロック数。top-let global は設計上終了まで生存するので、計測ありの `main` は epilogue の直前に drop 可能な top-let をすべて release する。出荷モジュールは決してこれを行わない。そのため正常終了後の `live` は正確にリークしたブロック数となる。同じ更新実行が `crates/almide-wasm/tests/golden/live-at-exit-baseline.txt` を書く。`live` が 0 でない fixture だけを載せ、それぞれの機構を担当する issue を明記し、縮小のみを許す（未掲載 fixture は 0 が必要。行は下がることしかできず、下がれば更新しなければならない。新規行は issue が指定されるまで `#?` を持ち、検査はそれを拒否する）。exit 0 でない実行は判定しない。生成型 fuzzer は、問題なく実行される各プログラムに同じ測定を適用する（`LeakAtExit` finding kind、`fuzz-leak` ラベル下の独自 nightly 分類）。

**2 つの読み方。** watermark 行が変わったとき、count 行を見ればどちらが動いたか分かる。`allocs` / `bytes` の増加は割当て、count が同じで watermark が高いのは生存期間（長く保持され、free-list が返すものを持たなかった）。watermark が全く動かなくても count 行は動き得る。それが churn である。両者が一致しない 3 プログラム：

| プログラム | `allocs` | `reused` | `bytes` | watermark 差分 |
|---|---:|---:|---:|---:|
| リストリテラル 1 つ（`let xs = [1, 2, 3]`） | 1 | 0 | 24 | +0（基準） |
| 同じものに第 2 のリテラルを追加 | 2 | 0 | 48 | +64 |
| 1000 ×（`"row " + int.to_string(i)`、反復ごとに drop） | 2000 | 1996 | 9780 | +48 |

要点は第 3 行である。合計 9,780 バイトの 2000 回の割当てでも、1,996 回を free-list が処理したため、watermark は 48 バイトしか動かない。watermark だけならこのループを 2 回の割当てと区別できない。コーパスでも規模を大きくすると同じ形が見られる（2 台帳の行、2026-09-23）：

| fixture | `allocs` | `reused` | 要求 `bytes` | `__heap` 行 |
|---|---:|---:|---:|---:|
| `variant_result_one_credit_churn` | 1,638,000 | 1,629,810 | 104,832,000 | 1,114,096 |
| `mut_param_alias_cow` | 8,170 | 8,049 | 72,057,178 | 209,056 |
| `grain_gc_shapes` | 5,059 | 5,035 | 16,030,437 | 131,360 |

それぞれ watermark が示す量の 100〜1000 倍のバイトを要求し、churn が倍になっても watermark 行は動かない。計測器は structural 経路を対象とし、#2761 以降これはすべての wasm プログラムを意味する。

<a id="js-host---host-js-2265"></a>

## JS ホスト（`--host js`、#2265）

`almide build app.almd --target wasm --host js -o dist/app.wasm` はモジュールの隣に `dist/app.js` と `dist/app.d.ts` を書く。コンパイラはこの host glue を書くための全情報をすでに持つため、手書きの WASI stub、import object、String marshalling なしに成果物をページまたは node で実行できる。`app.js` は依存なしの ES module：

- `init(source?, hooks?)` はコンパイルとインスタンス化を行う。`source` は省略できる（`.js` の隣の `.wasm` を読み込む。node では `fs.readFile`、ページでは `fetch`）。また bytes、`Response`、そのいずれかの promise、コンパイル済み `WebAssembly.Module` でもよい。`hooks.stdout` / `hooks.stderr` は生の `Uint8Array` chunk を受ける（既定：node では `process.stdout`、ページでは行ごとに `console.log`）。
- 出荷モジュールが名前を挙げる `wasi_snapshot_preview1` import だけを shim 化する（`fd_write`、`proc_exit` → `AlmideExit` を throw、clock/random/read の基礎機能）。ほかをリンクしないので、ほかを出力しない（#2276）。glue は任意の `--wasm-opt` 書換え後のバイト列から導出するため、`println` のみのプログラムを `--host js --wasm-opt` でビルドすると、正確に `fd_write` と `proc_exit` を持つ。shim 表外の import は、ページ内の `LinkError` ではなく、名前を示したビルド時拒否となる。
- `@extern(wasm, "js", "name")` import は `init(source, { js: { name } })` で接続する。`String` 引数はユーザー関数実行前にブロック header からデコードし、`String` 戻り値は guest が所有する新規ブロックへエンコードする。structural 経路は各宣言を宣言済み import へ lowering する（#2275）。関数 slot は明示的に失敗する stub として出力し、完成バイト列への後処理（`imports.rs`）が、5 個の `almide.*` import の後ろで各 stub を `(import module name (func ...))` に変換し、1 つの map を通して call、export、table entry を再採番する。ABI は export と同じ（`Int` → i64、`Float` → f64、`Bool` → i32、`String` → i32 block、`Unit` → 結果なし）。`String` 引数は呼出し中借用され（host は何も release しない）、`String` 結果は guest が所有する新規ブロック。`rs`/`rust` extern には wasm host がなく、wall のまま（E082）。`spec/wasm_host_js/extern_js.almd` の `// @leg: structural` 行は経路変更を可視化する ratchet 行である。`almide run --target wasm` はそのようなプログラムを名前付きで拒否し（import の host がない）、`--host js` を案内する。
- 各 `pub fn` に wrapper が付く：`Int` ↔ `number`（±2^53 の外では黙って切り詰めず `RangeError`。より広い値を正確に保つには `BigInt` 対応 hook を渡す）、`Float`、`Bool`、`String`、`Unit`、および（#3354）引数・戻り値としてのブロック形状。`Bytes` ↔ `Uint8Array`（コピー）、`List[T]` ↔ `Array<T>`（ここで対応する任意の要素型、入れ子のリストを含む）、`Option[T]` ↔ `T | undefined`（`none` は `undefined`、入力は `null` も受理）、record ↔ 宣言順のフィールドを持つ通常 object（フィールド欠落は呼出し前に `TypeError`）。各ブロックは emitter が export ごとに記録するレイアウト（`almide_wasm::host_exports::export_params_noted` / `export_rets`：element stride、field offset、record size）で構築・読取りし、host が再導出することはない。ソース型と一致しない記録はビルド時拒否。所有権は記録された引数所有権に従い、callee 所有ブロックは callee が release、借用ブロックは呼出し後に host が release する。モジュールの `__release` は flat なので、host はブロックの最後の credit を落とすときに形状を走査し、そのブロックが保持していた子を release する。variant、`Map`、`Set`、tuple、関数はまだ関数名と型を示すビルド時拒否。ブロック helper とともに出荷する `memoryBytes()` は線形メモリサイズを報告するため、host がリークを確認できる。
- `effect fn` export（および `Result[T, String]` を宣言する関数）は、宣言された `T` にかかわらず 1 つの `Result` ブロック（tag は payload+0、0 = ok、value slot は payload+8）を返す（#3352）。wrapper はこれを unwrap する。ok は上記のように marshalling した `T`、err は err String を `message` とする `AlmideError`（`Error`）を throw する。ブロックは `__release` で解放し、Result が最後の参照を保持していた場合は slot 自身の String ブロックも解放する。wrapper はソース型ではなく emitter が export ごとに記録する戻り値 ABI（`almide_wasm::host_exports::export_rets`）から選ぶ。ソース型と異なる記録（または String でない err）は関数名を示してビルド時拒否する。`.d.ts` は `T` を示し、`AlmideError` を宣言する。後者は何らかの export が Result を unwrap する場合にだけ出荷する。ゲート：`spec/wasm_host_js/effect_exports.almd` は node 上で marshalling する全型 × {fn, effect fn} × {ok, err} を実行する。
- throw する hook（または `returns: promise` と印されたものが reject する場合）（#3356）。wasm を巻き戻して伝播する例外は、巻き戻された frame が実行するはずだったすべての release を飛ばすため、glue は決して通過させない：
  - **失敗可能**な extern は `effect fn`、または `Result[T, String]` を宣言するもの（`T` は scalar、`String`、`Unit`）。host が構築する i32 の `Result` ブロックとして import する。hook の値は ok、throw は err（メッセージ）になる。Almide 呼出し元は自身の release 経路を通じて `!` で err を伝播し、それを伝播する export は `AlmideError` を reject または throw する。失敗呼出しでもメモリは増えない。`spec/wasm_host_js/hook_errors{,_async}.almd` が 3,000 回と 600 回で固定する。代わりに unwind すると、同じ 3,000 回で線形メモリが 0.9 MB から 7.3 MB へ増えた。
  - **失敗不能**な extern（`T` を返す通常の `fn`）は宣言上失敗できないので、そこでの throw は trap である。glue は import 名と修正方法（`effect fn` と宣言する）を示すエラーを再 throw し、instance を放棄する。その後のすべての呼出しは、巻き戻した frame がブロックを保持したヒープ上で実行せず、`… abandoned … call init() again` を throw する。`init()` は新しい instance を開始する。
- Promise を返す hook は宣言に `@extern(wasm, "js", "name", returns: promise)` と記す（#3353、#3371）。glue は JSPI を通じてそこで中断する。印のない hook が thenable を返すと `almide: hooks.js.<name> returned a Promise; mark its @extern with
  returns: promise` を throw し、失敗可能 extern であっても instance を放棄する。設計：[JS-HOST-ASYNC-IMPORTS.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/wasm/JS-HOST-ASYNC-IMPORTS.md)。
- `main` は `run()` であり、`init` は `_start` を呼ばない。

String marshalling はブロックレイアウト（`almide-layout`：rc @0、len @4、cap @8、payload @12）と、各 slot の valtype のためにモジュール自身の signature を読む（structural 経路の `Bool` は `i32`）。host が構築するブロックはモジュールの exported allocator（`__alloc`）を通り、host が取り出すブロックは exported release（`__release`）で解放する。両 export は `--host js` 下で host surface がブロック marshalling を必要とする場合にのみ存在する：String、Bytes、List、Option、record、unwrap する Result のエラーメッセージ（`src/cli/js_host.rs` の `HostSurface::needs_string_abi`）。どれも不要な scalar-only surface は switch なしのビルドとモジュールがバイト同一で、ブロック marshalling helper を持たない。structural 経路は export されたどの引数を callee が所有するかも記録するため、host は自分がまだ保持する credit だけを正確に release する。

ゲート：`scripts/check-js-host.sh` は各 `spec/wasm_host_js/*.almd` を `--host js` でビルドし、node で実行し、stdout を `<fixture>.expected` とバイト比較し、fixture の `<fixture>.host.mjs`（`js` hook と wrapper を動かす `after(module)`）を実行する。`@extern` のない fixture については native バイナリの stdout とも比較する。出荷内容も検査する（#2276）：scalar-only の場合は `--host js` なしとモジュールがバイト同一であること、marshalling surface がブロックを必要とするとき allocator/release export があること、glue の `wasi.<name>` shim 集合が出荷モジュールの `wasi_snapshot_preview1` import と等しいこと（`wasm-opt` 前後）、各 glue のバイトサイズが `spec/wasm_host_js/glue-ceiling.txt` の行以下であること（縮小は黙って通り、増加は台帳編集となる）。CI は `checks` ジョブで実行する。

<a id="reproducing-the-measurements"></a>

## 測定の再現

```bash
# Almide — the structural leg, plain and -Oz (the incumbent columns cannot be reproduced since #2761)
printf 'fn main() -> Unit = {\n  println("Hello, world!")\n}\n' > hello.almd
almide build hello.almd --target wasm -o hello.wasm                    # structural, as shipped
almide build hello.almd --target wasm --wasm-opt -o hello.min.wasm     # structural, -Oz
wasm-objdump -h hello.wasm                                             # the section tables above

# Rust (same target, full size profile — 2026-07-23 numbers)
cargo new rhello && cd rhello
# [profile.release] opt-level="z", lto=true, strip=true, panic="abort", codegen-units=1
cargo build --release --target wasm32-wasip1
```

コードコメントの説明：Almide は structural 経路の通常版と -Oz 版。旧来経路の列は #2761 以降再現不能。各 build は出荷形態と -Oz 形態を作り、wasm-objdump は上記セクション表を示す。Rust は同じターゲットの完全なサイズ最適化 profile で、数値は 2026-07-23 のもの。

---

原文に関する注意：旧来経路のサイズ・速度測定は歴史的なもので、廃止後には再現できない。各数値を再現するとの一般的な案内は、この制限を取り消さない。日付付き測定、CI 由来の代表値、後の JS-host 機能は異なる記述層であり、新たな測定ではない。セクション表は最適化後の Hello world に fd_write のみ残ると述べるが、後の JS-host 段落は println のみの最適化ビルドが fd_write と proc_exit を持つと述べる。範囲を勝手に統一せず両方保持した。counter の例は後述する一部フィールドを含まない。「検証済み」は記載されたコーパス、witness のカバレッジ、信頼する記録に制限され、外部 wasm-opt はその範囲を外れる。host ABI、例外封じ込め、測定された割当て／リークの挙動は、普遍的なメモリ安全性証明や第三者認証ではない。
