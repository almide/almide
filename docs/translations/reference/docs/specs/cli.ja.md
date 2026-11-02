<a id="cli-specification"></a>

# CLI 仕様

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](cli.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](cli.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](cli.zh-CN.md)

> [日英混在の正本](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/cli.md)、固定コミット `35297a612fb29e3f78f6587aba253aa91d51f466` の非規範的な読者用翻訳。自動同期されず、生成器・検査器への入力でもない。正本の権威を置き換えない。以下のコマンド・実行時フラグ・計測・日付・実装状態・安全性の主張は、固定原文の記述を保持したもので、現行リリースで独立に実行・検証していない。コードブロックとそのコメントは原文のまま。生成表マーカーは無効な読者用注記にしている。コンパイル予算の超過処理は、コンパイラの全操作がクラッシュ・OOM・ハングを起こさないという包括保証ではない。profile・能力・プラットフォーム・成果物の境界は原文の適用範囲を保持する。


> 最終更新： 2026-10-31

<a id="overview"></a>

## 概要

```
almide <command> [options] [arguments]
```

プロジェクトルートに `almide.toml` + `src/main.almd` があれば、ファイル引数は省略可能。自動的に `src/main.almd` が使われる。

---

<a id="commands"></a>

## コマンド

<a id="almide-run"></a>

### `almide run`

コンパイルして即実行。既定の native ターゲットでは Rust ソースを生成 → `cargo build` → バイナリ実行。

```bash
almide run                              # src/main.almd を実行
almide run app.almd                     # 指定ファイルを実行 (native)
almide run app.almd --target wasm       # wasm をビルドして埋め込み wasmtime ホストで実行
almide run -- --flag value              # -- 以降はプログラムの引数
almide run app.almd -- arg1 arg2        # ファイル指定 + プログラム引数
```

| オプション | 説明 |
|---|---|
| `--target rust\|wasm` | 実行ターゲット。`rust`（既定、ネイティブバイナリ）または `wasm`（構造的エミッタが WASM を生成し、埋め込み wasmtime ホストで実行。外部の `wasmtime` CLI・rustc のインストールは不要）。未対応のプログラムは診断付き wall になり、別レンダラへフォールバックしない。クロスターゲット等価性の範囲と制約は [WASM-OUTPUT](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/wasm/WASM-OUTPUT.md) を参照 |
| `--no-check` | 型チェックをスキップ |
| `--locked` | `almide.lock` に無い依存のフェッチと、ロックを変える解決を拒否し、ロックを書かない(#4093)。CI 向け |
| `--release` | 最適化ビルド。opt-level=3、fat LTO、codegen-units=1。ターゲットが x86_64 なら **`x86-64-v3`（AVX2・FMA。Intel は 2013 年の Haswell 以降、AMD は 2017 年の Zen 以降）に合わせて調整する**（#3604）。実測（spectralnorm、実行時の `n`）では汎用 → v3 が AMD Zen3 で 87.8 → 67.2 ms、Intel Xeon 8581C で 57.1 → 45.4 ms。`native` は Zen3 で 109.8 ms と逆に遅く、一度既定にして取り消した（#3727 → #3734）。AVX2 を持たない CPU では `SIGILL` で落ちるので、そこに配るなら `--target-cpu portable`。aarch64 などほかのターゲットは汎用のまま |

`almide` 自身のフラグ（`--target` / `--no-check` / `--release`）は `--` の前で解釈され、`--` 以降はそのままプログラムに渡る（`cargo run` と同じ規約）。プログラム内で `env.args()` を呼ぶと `--` 以降の引数が `List[String]` で返る。

テスト: `tests/run_target_flag_test.rs`

**実行時間の上限は無い**(#2615): `almide run` はどちらのターゲットでもプログラムを最後まで走らせる。
native バイナリに時間制限が無いのと同じく、`--target wasm` の埋め込みホストも watchdog を張らない
（以前は 30 秒で `wasm trap: interrupt` になり、ループ先頭ごとに epoch 検査の費用も払っていた）。
時間制限が要るのはハーネスの側で、ハーネスが自分で張る: in-process のテストランナー
（`run_wasm` / `run_wasm_with` / `run_wasm_capped`）は 30 秒の watchdog を持ち、
`ALMIDE_WASM_WATCHDOG_SECS` で変えられる。fuzz（`tools/xtarget-fuzz` の `--timeout`）や掃引スクリプトは
プロセス単位の制限を自前で持つ。テスト: `tests/wasm_run_no_time_limit_test.rs`

**native のビルドキャッシュ**(#2500): native ターゲットは生成 Rust を共有スクラッチ dir
（`ALMIDE_RUN_PROJECT_DIR`、既定 `<cache base>/run`）でビルドし、生成コードの内容ハッシュを名前にした
`target/<profile>/almide-<hash>` にバイナリを置く。同じ生成コードは `almide run` / `almide build`
のどちらから来てもこの 1 本を再利用し、cargo を呼ばない。dir 内の書き込み（ビルド・退避・掃除）は
すべて `.almide-build.lock` の下で直列化され、キャッシュヒットだけがロック無しで走る。

- **置き場所**（#4180）: native のビルドキャッシュ（`run`、`build-cdylib`、ランタイムの rlib
  `almide-rtlib-<key>`、`almide test` のワーカー `test/native`）は、すべて利用者ごとの非公開の土台
  `<temp>/almide-<euid>` の下に置く。土台は mode 0700 で排他的に作る。すでにある場合は、実体の
  ディレクトリで、実行中の利用者の持ち物で、グループと他人に権限が無いときだけ使う。それ以外は直さずに
  拒否する（exit 1）。中身を他人が書いたかもしれないため。Linux の `/tmp` のような共有の temp で、
  他人が先に作った dir に置いたバイナリが自分の権限で実行される穴を塞ぐ。
- **ヒットの条件**（#4180）: ビルド時にバイナリの SHA-256 を `almide-<hash>.sha256` に記録し、
  ヒットはその値とバイナリの中身が一致するときだけ。記録が無いか一致しなければ作り直す。
  テスト: `tests/build_cache_private_test.rs`

- **退避**: ヒットのたびにバイナリの mtime を更新し、7 日間使われなかった `almide-<hash>`
  （と `deps/` に残る同じ世代の `almide_out-*` オブジェクト）を、次のミス時にロックの下で削除する。
  この掃引は 1 日 1 回（`.almide-evict-stamp`）。incremental セッションには触れない。
- **サイズ上限**(#2608): 同じ `almide-<hash>` と `almide_out-*` の合計が `ALMIDE_CACHE_MAX_BYTES`
  （既定 `4G`、`0`/`off` で無効）を超えたら、mtime の古い順に同じロックの下で削除する。直近 2 分以内に
  使われたものは残す。この掃引は 1 分に 1 回（`.almide-size-stamp`）。依存 crate の rlib と
  incremental セッションは対象外。既定値は開発機での実測（プログラムごとの成果物 2.0 GB）の 2 倍。
- **rustc ICE からの復旧**: 中断されたビルド（ENOSPC、kill）が rustc の incremental セッションを
  壊すと、以後その形のビルドは `the compiler unexpectedly panicked` で毎回落ちる。失敗した
  ビルドの出力にこの banner があれば、同じロックの下で `target/*/incremental` を消して 1 回だけ
  再ビルドし、成功したら stderr に
  `note: rustc crashed on a stale incremental session; cleared … and rebuilt successfully` を
  1 行出す。再ビルドも失敗したら元のエラーをそのまま報告する（ループしない）。
- **`almide clean`** がこの dir を空にする（下記）。

テスト: `tests/run_cache_recovery_test.rs`

**wasm のビルドキャッシュ**: `--target wasm`（と `almide bench`）は、描画した wasm モジュールを
同じ dir の `target/wasm/almide-<key>` に置き、変わっていないプログラムの再実行ではモジュールを描画し直さない。
key は構造レグが読むもの全部: 入口ファイルのパスとソース、リゾルバがディスクから読んだ全モジュール
（自分のモジュールと依存パッケージ）のパスとソース、依存 dir、作業 dir、コンパイラ自身（バージョン行と
実行ファイルのパス・サイズ・mtime。同梱 stdlib はこの中にある）、環境の `ALMIDE_*` 変数すべて。
パース・型検査（警告も）・`[permissions]`・IR 検証・E081 は毎回ヒットの前に走るので、ヒットでも
ミスと同じ診断が出て、同じものを拒否する。飛ばすのは描画（構造レグ自身の front・emit・検証）だけ。
壁で止まった描画はキャッシュしない。ヒットの条件（SHA-256 の記録と一致）・7 日の退避・
`ALMIDE_CACHE_MAX_BYTES` の上限・`almide clean` は native の成果物と同じ。
`ALMIDE_TIME_PHASES=1` の `[phase] render (cache hit)` 行でヒットが見える。
テスト: `tests/wasm_run_cache_test.rs`

**`main` の無いファイル**: どちらのターゲットでも、そのファイルの `test` ブロックを
`almide test <file> --target <leg>` と同じ判定で走らせる（stderr に
``note: <file> declares no `main`; running its test blocks …`` を 1 行出す）。全部通れば 0、
1 件でも落ちれば 1、`test` ブロックも無ければ「走らせるものが無かった」の 5（`almide test` の
ゼロの扱い、#2204）。以前は native が libtest のハーネスをそのまま実行し、wasm は拒否していた。
`--` 以降のプログラム引数は使わない。テスト: `tests/wasm_run_cache_test.rs`

**stdout のバッファリング**(#2245): native バイナリの `println` / `io.print` /
`io.write(io.stdout()!, …)` は 1 つの 64 KiB バッファを通る(順序はプログラム順)。stdout が端末なら書き込み
ごとに flush。パイプやファイルでも改行を含む書き込みは flush する。その他の flush 点は終了時・panic 時・`main` が err を返した
時・子プロセス起動前・stdin 読み取り前、そして常に flush する `io.print`(`io.print("")` が明示
flush)。stderr(`eprintln`)は無バッファなので、端末以外では stdout との相対順序は保たれない。
詳細は [docs/stdlib/io.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/io.md)。

<a id="実行可能なスクリプト"></a>

### 実行可能なスクリプト

ファイル先頭に shebang を書き、実行権限を付ければ直接実行できる。
`/usr/bin/env -S` に対応した環境で、`almide` が PATH に必要。

`-S` は飾りではない。Linux のカーネルは shebang の残りを**一つの引数**として
`env` に渡すので、`-S` 無しの `#!/usr/bin/env almide run` は `almide run` という
名前のプログラムを探して死ぬ。macOS は空白で分割するので通ってしまう。
`almide check` は `-S` の無い二語以上の `env` shebang を E062 として警告する。

```almd
#!/usr/bin/env -S almide run

fn main() -> Unit = {
  println("shebang ok")
}
```

```bash
chmod +x script.almd
./script.almd
```

shebang は先頭（任意のUTF-8 BOMの直後を含む）だけで認識される。
行番号を保ち、`fmt` は shebang を imports や dialect stamp より前に保持する。
ほかの位置の `#` は従来どおりエラー。

テスト: `spec/cli/shebang.almd`, `tests/hot_native_regressions_test.rs`


---

<a id="almide-build"></a>

### `almide build`

コンパイルしてバイナリを生成。

```bash
almide build                            # src/main.almd → パッケージ名のバイナリ
almide build app.almd -o myapp          # 出力ファイル名指定
almide build app.almd --target wasm     # WASM バイナリ（直接 emit、rustc 不要）
almide build app.almd --target linux-musl -o app  # Linux の静的 musl バイナリ（#2772）
almide build app.almd --target x86_64-unknown-linux-musl  # 任意の rustc ターゲット三つ組
almide build app.almd --target wasm --host js -o dist/app.wasm  # + dist/app.js, dist/app.d.ts (JS ホスト、#2265)
almide build app.almd --target wasm --debug   # + DWARF 行テーブル（.debug_* カスタムセクション、#1315）
almide build --release                  # 最適化ビルド (opt-level=3, fat LTO, x86_64 は x86-64-v3 向け)
almide build --release --target-cpu portable -o app  # AVX2 の無い CPU にも配るバイナリ（rustc の汎用 CPU）
```

| オプション | 説明 |
|---|---|
| `-o <path>` | 出力ファイルのパス。どのネイティブ経路（バイナリ / `--cdylib` / `--repr-c` / その組み合わせ）でも同じ意味で、書いたパスそのものに出力し、親ディレクトリが無ければ作る（#3349）。拡張子や `lib` 接頭辞は付け足さない |
| `--target wasm` | WASM バイナリを生成（直接 emit） |
| `--target <triple>` | ネイティブバイナリのターゲット（#2772）。`rust` / `native`（既定、このホスト）、`linux-musl`（ホストのアーキテクチャの `<arch>-unknown-linux-musl`。x86_64 と aarch64）、または rustc のターゲット三つ組。cargo に `--target` を渡し、`target/<triple>/<profile>/` から成果物を拾う。ターゲットの標準ライブラリは `rustup target add <triple>` で入れる。musl ターゲットは Rust の既定（`crt-static`）で**静的リンク**になり、almide は追加のリンクフラグを付けない（glibc ターゲットは従来どおり動的）。依存の無いプログラムは rustc 同梱の musl crt で x86_64 ホスト上なら追加ツール無しにリンクできる。別アーキテクチャ向けや C を含む `[native-deps]` には、そのターゲットのリンカ / C コンパイラ（`musl-tools` 等）が要る。未知の値（三つ組の形をしていないもの）と `wasm32-*` は、ホスト向けに黙ってビルドせず終了コード 2 で拒否する |
| `--host js` | `--target wasm` 専用: モジュールの隣に JS ホスト `<mod>.js`（依存なしの ES module）と `<mod>.d.ts` を書く（#2265）。`init(source?, hooks?)` がインスタンス化、`run()` が `main`、`pub fn` ごとに 1 つのラッパ。`@extern(wasm, "js", "name")` は `init({ js: { name } })` で結線（`effect fn` か `Result[T, String]` で宣言した extern は hook の throw / reject を err として受け取り、Almide 側で `!` 伝搬できる。それ以外の extern で hook が throw するとインスタンスは放棄され、`init()` し直すまで呼べない — #3356）。Promise を返す hook は extern 側に `@extern(wasm, "js", "name", returns: promise)` と書く（#3353 / #3371）: glue がそれを `WebAssembly.Suspending` で包み、そこへ到達しうる export（と `run()`）だけを呼び出しグラフから推論して `WebAssembly.promising` 経由の async 関数（`.d.ts` は `Promise<T>`）にする。インスタンスへの呼び出しは 1 つずつ呼んだ順に直列化され、async 呼び出しが中断中の同期 export は入らずに例外。JSPI の無い実行系では `init()` が理由を挙げて拒否。言語の型・効果・呼び出し側は変わらず、native は無視する。印の無い hook が thenable を返すと `almide: hooks.js.<name> returned a Promise; mark its @extern with returns: promise` を throw してインスタンスを放棄する（fallible extern でも err にはならない）。develop にあった暫定フラグ `--async-import` は v0.67.0-rc1 より前に削除され、どのリリースにも入っていない。設計: docs/wasm/JS-HOST-ASYNC-IMPORTS.md。マーシャルは Int（`number`、±2^53 の範囲検査）/ Float / Bool / String / Unit、export ではさらに Bytes（`Uint8Array`）/ List[T]（配列、入れ子可）/ Option[T]（`T | undefined`）/ レコード（宣言順のフィールドを持つ plain object）を引数・戻り値の両方で（#3354、レイアウトとビルド側の所有権はエミッタが export ごとに記録したものに従う）— それ以外の型（バリアント / Map / Set / タプル / 関数）を境界に持つ `pub fn` はビルド時に型名を挙げて拒否。`effect fn`（と `Result[T, String]` を返す fn）の戻り値はモジュール上では Result ブロックで、ラッパはエミッタが記録した戻り ABI から作る: ok は `T` の値、err は message を持つ `AlmideError`（`Error` の派生）を throw する。記録とソースの型が合わない export もビルド時に関数名を挙げて拒否（#3352）。出荷物はプログラムが使う分だけ（#2276）: `__alloc`/`__release` の export と glue の String ヘルパは境界に `String` がある時だけ、WASI shim は出荷モジュール（`--wasm-opt` 後）が import する名前だけ。ゲート: `scripts/check-js-host.sh`（`spec/wasm_host_js/` を node で実行し、期待出力と native 出力に一致させ、モジュールのバイト同一性・shim 集合・`glue-ceiling.txt` の上限を検査）。仕様: docs/wasm/WASM-OUTPUT.md「JS host」節 |
| `--debug` | `--target wasm` 専用: 出荷するモジュール（`to_wasi` 後）の末尾に DWARF 4 の `.debug_info` / `.debug_abbrev` / `.debug_line` / `.debug_str` / `.debug_ranges` をカスタムセクションとして足し、コードオフセット（Code セクション内容先頭からのバイト位置、LLVM/Emscripten と同じ "DWARF for WebAssembly" 規約）から `.almd` の file:line:col を引けるようにする（#1315）。ソースファイルごとに compile unit 1 つ、関数ごとに `DW_TAG_subprogram`（名前・`low_pc`/`high_pc`）1 つ。Chrome DevTools と wasmtime + lldb がフラグ無しで読む。行はエミッタが命令ごとに IR の span から記録し（span を持たない合成ノードは最も近い祖先の span を継ぐ、祖先にも無い命令 — 関数の出口の解放など — は line 0）、コード自体は変えない: 既定（`--debug` 無し）の出力はバイト同一で、`size-baseline*.txt` の対象外。`--component` / `--wasm-opt` とは併用不可（どちらも出荷バイトを書き換え、表が指すアドレスが変わるため、終了コード 2 で拒否）。ネイティブターゲットでも拒否 |
| `--release` | 最適化ビルド。opt-level=3、fat LTO、codegen-units=1。ターゲットが x86_64 なら **`x86-64-v3`（AVX2・FMA。Intel は 2013 年の Haswell 以降、AMD は 2017 年の Zen 以降）に合わせて調整する**（#3604）。実測（spectralnorm、実行時の `n`）では汎用 → v3 が AMD Zen3 で 87.8 → 67.2 ms、Intel Xeon 8581C で 57.1 → 45.4 ms。`native` は Zen3 で 109.8 ms と逆に遅く、一度既定にして取り消した（#3727 → #3734）。AVX2 を持たない CPU では `SIGILL` で落ちるので、そこに配るなら `--target-cpu portable`。aarch64 などほかのターゲットは汎用のまま |
| `--target-cpu <cpu>` | `--release` のネイティブビルドを調整する CPU（#3604）。既定はターゲットの系統で決まり、x86_64 なら `x86-64-v3`、それ以外は rustc の汎用 CPU（クロスビルドでもターゲットの三つ組で決まる）。`portable` は汎用 CPU、それ以外の値は rustc の CPU 名としてそのまま渡す（`native`、`x86-64-v4`、`neoverse-n1` など）。クロスビルドに `native` を指定するとエラー、`--target wasm` に指定してもエラー（wasm は CPU に依存しない）。ランタイムの rlib も同じ CPU でビルドし、ビルドキャッシュの鍵に CPU が入る |
| `--fast` | `--release` と同じ（互換のために残す） |
| `--unchecked-index` | 配列の境界チェックを無効化（unsafe） |
| `--no-check` | 型チェックをスキップ |
| `--locked` | `almide.lock` に無い依存のフェッチと、ロックを変える解決を拒否し、ロックを書かない(#4093)。CI 向け |
| `--repr-c` | struct/enum に `#[repr(C)]` を付与（C ABI 互換） |
| `--cdylib` | 共有ライブラリ（`.so` / `.dylib` / `.dll`）を生成。`-o` 省略時はカレントディレクトリの `lib<名前>.<拡張子>`（Windows は `<名前>.dll`）。Cargo の crate 名は `-o` ではなくパッケージ名（`almide.toml` の `[package].name`、無ければエントリファイル名）から作る（#3349） |

**`CARGO_BUILD_TARGET`**(#2772): `--target` が無いとき、環境変数 `CARGO_BUILD_TARGET` はターゲットの指定として
`--target <triple>` と同じに扱う（`--target rust` はそれを打ち消してホスト向けにする）。どちらの場合も almide は cargo に
ターゲットを明示し、子プロセスの環境からは `CARGO_BUILD_TARGET` を除く。以前はこの変数が cargo に漏れて成果物が
`target/<triple>/` に出る一方、almide は `target/<profile>/almide-out` を拾っていたため、前回のホスト向けバイナリを
黙って出力するか `expected binary not found` で落ちていた。ビルド前にその場所の古い成果物を消すので、cargo 設定の
`build.target` などで出力先がずれても古いバイナリは出ず、`expected binary not found` で止まる。ビルドキャッシュの鍵には
三つ組が入る。`almide run` / `almide test` はバイナリをこのマシンで実行するので、常にホスト向けにビルドする
（`CARGO_BUILD_TARGET` は子の cargo に渡さない）。

出力ファイル名のデフォルト:
- `almide.toml` があれば `[package] name`
- なければソースファイル名から `.almd` を除いた名前

<a id="rustc-の下限4110"></a>

#### rustc の下限（#4110）

ネイティブビルド（`almide run` / `build` / `test` / `bench`、cdylib と install の経路）は、
rustc や cargo を起動する前に `rustc --version`（PATH の rustc と、設定されていれば `$RUSTC`）を読み、
**rustc 1.97.0** 未満なら何もビルドせず終了コード 1 で拒否する。エラーには見つかった版、必要な版、
直し方（`rustup update`）が出る。下限は `src/cli/rustc_floor.rs` の `MIN_RUSTC` 一か所に書かれ、
max(1.77.2, CI が生成コードを検証する `RUST_TOOLCHAIN`) である。1.77.2 は CVE-2024-24576
（Windows の `Command` が `.bat` / `.cmd` への引数を cmd.exe で無害化しなかった）の修正版で、
古い rustc で作ったバイナリでは `process.exec` 経由のシェル注入が再び開く。
`--target wasm`（直接 emit）は rustc を使わないので対象外。

---

<a id="almide-test"></a>

### `almide test`

`.almd` ファイル内の `test "name" { ... }` ブロックを検出・実行。

```bash
almide test                             # カレントディレクトリ以下を再帰スキャン
almide test spec/lang/                  # ディレクトリ指定
almide test spec/lang/expr_test.almd    # ファイル指定
almide test --run "pattern"             # テスト名でフィルタ
almide test --target wasm               # WASM ターゲットでテスト
almide test --json                      # 結果を JSONL (1行1ファイル) で出力
almide test --update-snapshots x_test.almd  # スナップショットの受理(期待値リテラルを書き換え)
```

| オプション | 説明 |
|---|---|
| `-r, --run <pattern>` | テスト名のパターンフィルタ(下記) |
| `--no-check` | 型チェックをスキップ |
| `--locked` | `almide.lock` に無い依存のフェッチと、ロックを変える解決を拒否し、ロックを書かない(#4093)。CI 向け |
| `--json` | JSON 形式で結果出力 |
| `--target wasm` | wasm レグだけで実行（native fallback なし） |
| `--update-snapshots` | `testing.assert_snapshot` の不一致を受理し、呼び出し側の期待値リテラルをソース内で書き換える(`ALMIDE_UPDATE_SNAPSHOTS=1` でも同じ) |
| `--ci` | CI モード: スナップショットを一切書かない(`CI=true` でも同じ)。新規・乖離はどちらも失敗 |
| `--allow-no-tests` | 実行すべきテストが 0 件でも 0 で終了する(既定は 5) |
| `--show-output` | 通ったテストも含め、全テストが stdout / stderr に書いたものを表示する |

実行のたびに **実行テスト数**を報告する: `2 tests in 1 file`。`--run` が何かを除外した
ときは `0 tests in 1 file (2 filtered out)` のように除外数も付く。`--json` は各行に
`"tests"` と `"filtered_out"` を持つ。

**ゼロの扱い**(#2084, #4264):

| 状況 | 終了コード | 理由 |
|---|---|---|
| `--run` が 1 件も一致しなかった | **5** | 何も走っていない。CI のフィルタの打ち間違いが緑に見えないように。メッセージはパターンが一致しなかったと言う。`--allow-no-tests` で opt-out |
| 実行すべきテストが 1 件も無かった | **5** | 事故。`--allow-no-tests` で opt-out |
| テストはあったが、全ファイルがスキップされ 1 件も走らなかった | **5** | 何も走っていない（#4325）。メッセージは `no test ran: N file(s) were skipped` と言う。`--allow-no-tests` で opt-out |

「テストが無かった」は**テストファイルが見つからなかった場合と、名指ししたファイルに
`test` ブロックが無かった場合の両方**に適用される。5 は 1(テスト失敗)と区別するための
専用コードで、呼び出し側が出力を読まずに判別できる。

この表は**ターゲットを問わない**: 同じファイル・同じ引数なら `--target wasm` も同じ終了
コードと同じ件数行(`0 tests in 1 file`)を返す。`main` も `test` ブロックも無いファイルは
レンダラの壁(WALL、そのレグが辞退した)ではなく「走らせるものが無かった」であり、両レグで
5 になる(#2204)。

**スキップは、同じ実行で 1 件以上のテストが走ったときだけ緑**(#4325)。`--target wasm`
のスキップは 3 種あり、要約行に種類ごとの数が出る（`1 passed, 0 failed, 2 skipped: 1 walled,
1 declared `// wasm:skip` (of 3 files)`、件数行は `1 test in 3 files, 2 files skipped`）:

| スキップ | 意味 |
|---|---|
| declared（先頭 3 行の `// wasm:skip`） | 作者が「wasm ではこのファイルを走らせられない」と理由付きで宣言した |
| walled（`WALL` 行） | wasm レグがそのプログラムを lower できなかった（E082 と同じ判定） |
| host unavailable | レーンを起動できない環境（`ALMIDE_TEST_WASM_RUNNER=wasmtime` で CLI が無い等） |

どれも「テストはあるが、このレグでは走らなかった」であって合格ではない。そこで規則は
1 つ: **実行全体で走ったテストが 0 件なら、スキップの有無にかかわらず 5**。宣言済みの
`// wasm:skip` ファイルも、そのファイル単独で `--target wasm` にかければ 5 になる（走らせた
ものが無い）。ディレクトリ実行で他のファイルのテストが 1 件でも走れば、宣言スキップは
緑のまま。以前はスキップがあると 0 件でも 0 で終わり、wasm で lower できない負の対照が
`0 passed, 0 failed, 1 skipped` で通っていた。

**組み込みホストに無い `@extern(wasm)` import はミスコンパイルではない**(#4324)。
`@extern(wasm, "m", "f")` を import するプログラムは、`almide run --target wasm` と同じ
文言（`this program imports `m.f` … has no host for it` と `--host js` の hint）で断られる:

- 既定の `almide test`（wasm 先行 + native fallback）: そのファイルは native で走り、
  `note: <file>: the wasm leg cannot run this file; its tests run on the native fallback`
  と理由が出る。「wasm-only miscompile」の WASM TRAP にはならない
- `--target wasm`: `almide run --target wasm` と同じく **FAIL**（終了 1）。そのホストを
  前提とするファイルは `// wasm:skip <理由>` で宣言する

テスト: `tests/test_zero_outcomes_test.rs`、両ターゲット一致は `tests/test_zero_exit_parity_test.rs`、
スキップと無ホストの判定は `tests/wasm_test_skip_verdict_test.rs`

**失敗したテストの出力は失敗報告の下に付く**(#2538)。`println` / `eprintln` で途中の値を
出したテストが落ちたとき、その出力が捨てられないようにするため:

```
FAILED: e.almd
  test: eprintln inside a failing test
  at:   e.almd:4
  expected: 2
  found:    1
  stdout:
    STDOUT: value is 42
  stderr (whole file — stderr carries no per-test boundary):
    DEBUG: value is 42
```

- **stdout はテスト単位**。native(libtest を `--test-threads=1` で実行)も wasm のランナーも
  テストの前後を stdout に印字するので、その間がそのテストの出力になる。
- **stderr はファイル単位**。どちらのハーネスも stderr にはテストの境目を書かないため分割
  できず、同じファイルの通ったテストの stderr も含む。ラベルがそう言う。
- 通ったテストの出力は既定で黙る。`--show-output` で全テスト分を表示する。
- native・wasm・既定レーン(wasm 先行、失敗は native で再実行)のどれでも同じ。

テスト: `tests/test_failure_shows_output_test.rs`

**ファイルのテスト実行は、そのファイル自身の `test` だけを走らせる**(#2550)。import した
モジュールの `test` は import 元のビルドには入らず、そのモジュールのファイル自身の実行で走る。
したがってディレクトリを渡した `almide test` では、どのテストもちょうど 1 回走り、落ちた
テストはそれを書いたファイルのパスとソース上の名前で報告される。native・wasm・既定レーンの
どれでも同じ。

テスト: `tests/imported_module_tests_not_rerun_test.rs`

`--run <pattern>` は **テスト自身の名前（`test "…"` に書いた文字列、`where` の case は
`名前 / case`）に対する大文字小文字を区別する部分文字列一致**である（#4264）。失敗報告に出る
名前をそのまま渡せば選ばれる: `--run "beta fails"`（空白のまま）も、`--run "値が無ければ"` の
ような ASCII 以外の名前も一致する。生成された関数名（`__test_almd_0001_beta_fails`、
`almide_ir::test_fn_name`）には一致させない — 以前はそちらに一致させていたため、空白や記号を
含む名前、ASCII 以外の文字で書いた名前（関数名では畳まれてハッシュが付く）を自分の名前で
選べなかった。

この規則は **native レグと wasm レグで同一**である（#2085）。述語は
`almide_base::names::test_name_matches_filter` の一つだけで、wasm はランナー合成の時点で
これで絞り、native は同じ述語で選んだテストの完全なパスを `--exact` で Rust のテスト
ハーネスに渡す。1 件も一致しないパターンは上の表のとおり 5 で終わる。

**ファイルの判定は終了コードだけでは決めない**(#4255)。テストの中で `process.exit(0)` を
呼ぶとプロセスは 0 で終わるが、残りのテストは走っていない。native は各テストが本体の
最後まで到達したときに `ALMIDE_TEST_LEDGER` の指すファイルへ自分の名前を追記し
（`test_module` テンプレート。プログラムの stdout ではないハーネス自身の記録）、選んだ
全テストの完了記録があるときだけ通し、wasm は組み込みホストがランナーの `main` が最後まで戻ったことを確かめる。
足りなければそのファイルは失敗し、最初に終わらなかったテストの名前を出す。

テスト: `tests/wasm_test_filter_parity_test.rs`

wasm レグは `almide` バイナリに組み込まれた wasmtime ホスト（`almide run --target wasm` と同じもの）で動くので、PATH に `wasmtime` は要らない（#3046）。`ALMIDE_TEST_WASM_RUNNER=wasmtime` のときだけ、`almide build --target wasm` が出す stock 成果物（`to_wasi` 済み）を `wasmtime` CLI で動かす（比較用の経路）。どちらの経路でも、wasm レグが起動できなかったファイルがあれば、原因ごとに一度 `note: the wasm leg could not start for N file(s) (<原因>); they run on the native fallback` を出す。件数だけが痕跡になることはない。

テスト: `tests/test_wasm_leg_runner_test.rs`

スクラッチ成果物（CLI 経路で wasm レグが wasmtime に渡す `.wasm` モジュール）は実行ごとに固有の
`<cache base>/test-run-<pid>-<nonce>/` 配下に、ファイルの**絶対パス**のハッシュで命名して
置かれ、終了時に削除される（`ALMIDE_KEEP_SCRATCH=1` で残し、場所を stderr に出す）。
同名ファイルの並列実行や別ディレクトリの同名ファイルがパスを共有することはない（#1877）。
ネイティブ fallback のビルドキャッシュは `<cache base>/test/native/` に永続（同じく絶対パス鍵）。

この worker dir は**テストファイルの絶対パス 1 本につき 1 つ**で、それぞれが自分の `target/` を持つ。
放置すると消えるものが無い（名前を変えたテスト、消した worktree、消えたブランチの分が残り続ける:
2026-09-22 に 4,510 dir / 39 GB を実測）ので、`almide test` の開始時に**7 日間誰も使っていない
worker dir を空にする**(#2504)。判定は「その dir 直下と `target/<profile>/` のファイルの mtime」
— キャッシュヒットが実行するバイナリの mtime を更新する(#2500)ので、ビルドしていなくても
「使った」dir は残る。掃引は 1 日 1 回（`native/.almide-evict-stamp`）、各 dir の
`.almide-build.lock` を**待たずに**取り、取れなければ（他プロセスがビルド中）その dir は飛ばす。
lockfile は残すので、掃引後の dir は lockfile だけの空ディレクトリになる（理由は
[`almide clean`](#almide-clean) の節）。`ALMIDE_KEEP_SCRATCH=1` のときは掃引しない。
`almide clean` は年齢に関係なく全 worker dir を空にする。

年齢の規則は**別々のファイルを大量にテストする突発負荷**の中では発火しない（2026-09-24 に 2 時間で
7,144 dir / 84.9 GB に達してディスクを埋めた、#2608）。そこで worker キャッシュ全体に**サイズ上限**を
置く: 合計が `ALMIDE_CACHE_MAX_BYTES`（既定 `4G`、`0`/`off` で無効）を超えたら、最後に使われたのが
最も古い worker dir から順に、上と同じ手順（各 dir の lock を待たずに取る・lockfile は残す・
lock の下で「最後の使用が変わっていない」ことを再確認）で空にする。直近 2 分以内に使われた dir は
上限を超えていても空にしない（ロック無しのキャッシュヒットが存在確認から exec するまでの窓を守る）。
この掃引は 1 分に 1 回（`native/.almide-size-stamp`）。既定の 4 GiB は、worktree 複数と並走
エージェント 5 本の開発機で実測した 1 週間分の作業集合（worker 65 dir / 2.0 GB）の 2 倍。

同じ規則が**ランタイム rlib キャッシュ** `<cache base>/almide-rtlib-<key>/` にも効く(#2504)。
この dir はランタイムソース × rustc バージョン × opt レベルごとに 1 つ作られ、コンパイラを
ビルドし直すたび・ツールチェーンを上げるたびに新しい鍵になって古い方は二度とリンクされない
（2026-09-22 に 31 dir / 102 MB を実測）。リンクのたびに rlib の mtime を更新し、7 日リンク
されなかった dir を（自分が今使っている dir を除いて）空にする。掃引は 1 日 1 回、
stamp は `<cache base>/.almide-evict-stamp`。

テスト: `tests/test_scratch_race_test.rs`, `tests/run_cache_recovery_test.rs`

失敗の報告は**構造化ブロック**（`FAILED: <file>` に続けて `test:` / `at:` /
`hint:` / `diff:` または `expected:` `found:`）。複数行文字列・リスト・レコードは
単位ごとの実差分になる。ファイル内の失敗は**ソース行順**に並ぶので、2回の実行を
そのまま diff できる。

`--json` は1ファイル1行の JSONL:

```json
{"file":"spec/lang/x_test.almd","status":"fail","exit_code":101,
 "failures":[{"name":"string mismatch","file":"spec/lang/x_test.almd","line":4,
              "op":"assert_eq","expected":"\"a\\nB\"","found":"\"a\\nb\"",
              "diff":"      a\n    - B\n    + b\n","message":"…"}]}
```

テストの書き方:

```almide
test "addition" {
  assert_eq(1 + 2, 3)
}

test "string concat" {
  assert_eq("a" + "b", "ab")
}
```

- `test` ブロックは任意の `.almd` ファイルに書ける
- `*_test.almd` サフィックスは慣習（強制ではない）
- `test` ブロック内は暗黙の effect context（I/O 呼び出し可能）

<a id="スナップショット-testingassert_snapshot"></a>

#### スナップショット (`testing.assert_snapshot`)

期待値は**ソース内のリテラル**(第 2 引数)であり、sidecar ファイルは持たない
(expect-test 型)。`""` で書き始めて `almide test --update-snapshots <file>` を
実行すると、実測値がリテラルとして書き戻される(複数行なら heredoc)。
書き換えは呼び出し行のリテラルだけで、ファイルの他の部分はバイト単位で不変。

```almide
import testing

test "render" {
  testing.assert_snapshot(render(x), "")   // → --update-snapshots で実測値に書き換わる
}
```

- 不一致は通常の失敗として報告される(`expected:` / `found:` または `diff:` と、
  `accept:` 行に受理コマンド)。実行時の停止ブロックは両ターゲットで同一
  (`Error: snapshot mismatch` / `at: line N` / `expected:` / `found:`、exit 1、契約 C-336)
- `--update-snapshots` は 1 ファイルにつき「実行 → 書き換え → 再実行」を収束まで
  繰り返す(停止は最初の不一致で起きるため)。第 2 引数がリテラルでない
  (変数・補間文字列)場合は書き換えず失敗する
- CI モード(`--ci` / `CI=true`)では `--update-snapshots` は何も書かず、新規
  (`""`)・乖離とも失敗する。スナップショットはコードと同様にコミットして
  レビューする

---

<a id="almide-check"></a>

### `almide check`

型チェックのみ実行。バイナリ生成なし。CI やエディタ統合用。

```bash
almide check                            # パッケージ内: src/ 配下の .almd を全部チェック (#2165)
almide check app.almd                   # 指定ファイルをチェック
almide check --deny-warnings            # 警告をエラーとして扱う
almide check --json                     # 診断を JSON で出力(パッケージ内なら src/ 全部、#2253)
almide check --explain E001             # エラーコードの説明
almide check --effects                  # 各関数のエフェクト分析を表示
almide check --timings                  # フロントエンドの phase 別内訳
almide check app.almd --profile critical --allow IO  # critical profile (#567)
```

| オプション | 説明 |
|---|---|
| `--deny-warnings` | 警告をエラー扱い |
| `--profile critical` | 全関数に bounded profile を適用、capability は deny-all から(下記) |
| `--allow <CAP>` | `--profile critical` で capability を 1 つ許可(複数回指定可) |
| `--json` | 診断を JSON で出力（1 行 1 診断、エディタ/エージェント統合用） |
| `--explain <code>` | エラーコードの説明(`almide explain <code>` と同じ) |
| `--budget NAME=N` | コンパイルコストの予算を 1 つ設定(下記、複数回指定可。`--profile critical` では拒否) |

`./almide.toml` にどのリーダーも読まないキー(`brnach = "main"` など)があると、
`check` / `build` / `run` / `test` を含む全コマンドが一度だけ警告する(#3382):
行(`almide.toml:LINE`)・キー・その表が受け付けるキー・1 編集距離の候補を出す。
ビルドはキーが無い場合と同じに走り、終了コードは変わらない(`--deny-warnings`
も数えない)。`--json` では `"level":"warning"` の行として stdout に、それ以外は
stderr に出る。表ごとの受理キーは [package-system.md §6](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/package-system.md#6-dependency-declaration)。

`almide explain --list [--json]`(#2149)は全コードを 1 行ずつ出す:
`{code, mnemonic, severity, since, verdict}`。`mnemonic` は `docs/diagnostics/<CODE>.md`
の題、`verdict` はその `## Fix-it verdict`(mechanical / conditional / not-fixable)、
`severity`(error / warning / error|warning)と `since`(そのコードを初めて出したリリース)は
`docs/diagnostics/codes.toml`。行の集合はコンパイラが出すコードの集合と一致し
(`with_code` の 77 + ビルド経路が `error[EXXX]` を直接出す E081–E083)、
severity はフィクスチャ全件で実際の level と照合される(`tests/explain_list_test.rs`)。
| `--effects` | 各関数のエフェクト/ケイパビリティ分析。受け取ったクロージャを呼ぶ関数は `{} + whatever f (arg 1) does` と表示され pure に数えない(#3268)。クロージャのカテゴリ集合は値と一緒に流れ、呼ぶ側の呼び出し地点に課される(ADR-0026 D1、`docs/specs/effect-system.md`)。`[permissions]` 違反は `path:` 行で経路を示す(D4) |
| `--timings` | lex / parse / check の phase 別 wall time（#1311） |

<a id="コンパイルコストの予算adr-0030-4136"></a>

#### コンパイルコストの予算（ADR-0030, #4136）

コンパイラは信頼できない入力（PR の CI、ファイルを開いた瞬間の LSP、採点器、playground）を処理するので、
入力に比例して仕事が増えるフェーズにはそれぞれ名前付きの**予算**がある。予算は数える（ノード数・インスタンス数・
バイト数・段数）もので、秒や空きメモリでは測らない。だから同じ入力は、どの機械でも、native でも wasm 上の
コンパイラでも、CLI でも LSP でも同じ判定になる。超えたら定義済みの診断で終わり、クラッシュ・OOM・ハングにはならない。

- **ソースは予算を上げられない。** 属性・`almide.toml`・`almide.lock`・環境変数のどれも予算を読まない。
- **上げられるのはコマンドラインだけ**: `--budget NAME=N`（複数回指定可、全サブコマンド共通）。
  `--profile critical` は `--budget` を拒否する（既定の予算で判定する）。
- `almide budgets` が一覧、`almide budgets --md` が下表そのもの（`tests/compile_budget_test.rs` が一致を検査）。
- 隠しフラグ `--budget-report` は、終了時に各予算の観測最大値と数えた仕事量を stderr に出す
  （`scripts/measure-compile-budgets.sh` と上記ゲートが読む）。

> 原文の生成マーカー（この読者用スナップショットでは無効）：almide budgets --md: begin

| 予算 | 既定値 | 数えるもの | 超過時 |
|---|---|---|---|
| `nesting` | 500 段 | 一つの式またはパターンの構文上の入れ子。パーサが下る際に数える | 構文エラー `expression nesting too deep (max N)` |
| `type_size` | 256 ノード | 一つの推論型のノード。検査器が式の型を作る際、および推論変数を解決する際に数える。`let t1 = (t0, t0)` を 22 回倍増すると 3.7 GB になっていた（#4114） | E111 |
| `generic_instances` | 1000 インスタンス | 一つのプログラムが単相化する protocol 制約付きジェネリック fn の特殊化インスタンス。各インスタンスの型引数も `type_size` に制限される。多相再帰では、毎回より大きなものが際限なく追加される | E112 |
| `manifest_bytes` | 1048576 バイト | 読み取る一つの `almide.toml` または `almide.lock` の大きさ。自プロジェクト・依存先とも、読む前と読み取り中に検査する（#4194） | エラー `<file> is larger than N bytes` |
| `dependency_count` | 1000 パッケージ | 一度の依存解決が訪ねる異なるパッケージ。推移閉包を含め、各取得の前に数える | エラー `too many dependencies (N, max M)` |
| `dependency_depth` | 64 段 | ルートパッケージから解決中のパッケージまでの依存鎖の長さ。取得前に数える | エラー `dependency chain too deep (N, max M)` |
| `exhaustiveness_steps` | 400 ステップ | 一つの `match` の網羅性・到達可能性検査の再帰ステップ（パターン行列の usefulness は最悪時に指数的） | E113 |

> 原文の生成マーカー（この読者用スナップショットでは無効）：almide budgets --md: end

<a id="--profile-critical"></a>

#### `--profile critical`

`--profile critical`(#567)は bounded profile(ALS §B、E070–E078)を **全関数** に
適用する。`@bounded` 属性は要らない。critical で通るコードは通常モードでも必ず通る
(部分集合であって方言ではない)。高階呼び出しやクロージャ生成は E074 になる。

capability は deny-all から始まる。host に触れる stdlib 呼び出しは E076 になり、
`--allow` で許可した分だけ通る。`--allow` の語彙と、それぞれが許可する module は
`crates/almide-frontend/src/check/bounded.rs` の `CAPABILITY_GRANTS`:

| `--allow` | 許可する module |
|---|---|
| `IO` | `io`, `fs` |
| `Net` | `http`, `net`, `unix` |
| `Env` | `env`, `args` |
| `Time` | `datetime`, `duration` |
| `Rand` | `random`, `uuid` |
| `Process` | `process` |

`Fan` は無い。`fan.*` は critical profile の外にある(#1628)。この語彙は
`almide.toml` の `[permissions].allow` の leaf / parent capability 語彙とは別物。
たとえば critical の `Process` に対し manifest は `Proc`、manifest には `Fan` もある。
正確な leaf と parent 展開は `docs/specs/effect-system.md` §8 を参照。

```
$ almide check rd.almd --profile critical
error[E076]: an effect outside the declared capability is not admissible under `--profile critical`
  hint: capabilities start deny-all — grant one with --allow IO|Net|Env|Time|Rand|Process
$ almide check rd.almd --profile critical --allow IO
No errors found
```

`--profile critical` は `ffi.*` の memory-unsafe 呼び出しと `@extern` fn の
宣言を E100 で拒否し、`--allow` では許可できない。通常モードでも `ffi` の
使用は明示 import を要する。現在の surface は不透明な `Handle` と長さ・所有者を
持つ `Region` であり、削除された `RawPtr` や `bytes.from_raw_ptr` を使う例ではない。
詳細は [ffi](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/ffi.md) と `stdlib/ffi.almd`（ADR-0031）。

profile に関係なく、`almide build` は `@extern` や foreign pointer の使用を
`Built` 行に列挙する。これは保証境界の表示であって、その表示が無いだけで
完全なメモリ安全性の証明が得られるという意味ではない。

テスト: `tests/build_outside_guarantee_test.rs`

拒否されるもの(いずれも終了コード 1):

- 語彙に無い名前: ``error: unknown capability `io` in --allow — grantable capabilities are IO, Net, Env, Time, Rand, Process``
  と ``hint: did you mean `IO`?``。文面は `[permissions].allow` の拒否と同じ(#3247)
- `--profile` 無しの `--allow`: `error: --allow requires --profile critical`
- `critical` 以外の profile 名: ``error: unknown profile `strict` — the only profile is `critical` ``
- `--effects` との併用: `error: --profile is not supported with --effects`

`--json` とパッケージ全体の形(FILE 省略)も同じ profile で判定する。

テスト: `tests/critical_profile_test.rs`、`tests/manifest_permissions_test.rs`
(`critical_allow_suggests_the_nearest_capability`)

<a id="--timings"></a>

#### `--timings`

フロントエンドの時間を lex / parse / check に分解して stderr に 2 行出す。
1 行目は人間向け、2 行目は機械可読（キー名は API — `scripts/check-edit-loop-scale.sh`
の per-phase ratchet が読む）:

```
$ almide check --timings spec/lang/expr_test.almd
timings: lex 1.8ms (16.9%, 2443k lines/s) parse 1.6ms (15.3%, 2692k lines/s) check 3.9ms (35.9%, 1149k lines/s) | other 3.4ms | total 10.7ms over 4426 lines in 43 sources
almide-timings {"lex_ns":1812250,"parse_ns":1644211,"check_ns":3851626,"total_ns":10717583,"lines":4426,"bytes":109057,"sources":43}
```

- 計上は `--timings` を付けた時だけ有効。付けない実行は clock を一切読まない
  （計測が計測対象を動かさないため — 実測 A/B は `research/benchmark/editloop/scale.py` の冒頭）。
- `lines` / `sources` は **実際に lex したもの全部**。エントリと自プロジェクトの
  モジュールに加え、どのチェックも必ず払う auto-import 済み bundled stdlib を含む。
  lines/sec の分母はこれ。
- `other` は 3 phase 以外の残り（ファイル I/O、import 解決、canonicalize、
  unused 警告用の lowering）。残余に名前を与えないと任意の回帰を吸ってしまう。
- エラーで終了した check では出さない。時間が診断レンダラに行っているため。

テスト: `tests/check_timings_test.rs`（キー一式、各 phase 非ゼロ、二重計上なし、
`--timings` なしでは無出力）。ratchet 側は `scripts/check-edit-loop-scale.sh`。

`--json` の 1 行は
`{level, code, message, hint, here, try, try_replace, applicability, suggestions, repair?, context, file, line, col, end_col, secondary}`。
`try` は貼り付け可能な修正スニペット、`try_replace` はそれが置換するスパン、
`suggestions[]` は `{line, col, end_col, replacement, applicability}` の構造化された同じ修正。

**`repair`**(#2149): 修復の構造化フィールド。修正を持つ診断にだけ出る(持たない診断の
バイト列は従来どおり)。形は
`{"primary": Edit|null, "alternatives": [Edit], "example": "…"|null}`、
`Edit` は `suggestions[]` の要素と同じ `{line, col, end_col, replacement, applicability}`。

- `primary` — 診断が責任を持つ 1 つのスパン厳密な編集。`applicability` が
  `"machine-applicable"` のものだけを `almide fix` は無人で適用する。
  `docs/diagnostics/<CODE>.md` の `## Fix-it verdict` が **mechanical** のコードは、
  必ずこれを machine-applicable で出す(`tests/diagnostic_coverage_test.rs` が
  バイナリで判定し、`almide fix` 後に `almide check` が通ることまで確かめる)。逆に
  mechanical でないコードが machine-applicable を出すのも失敗(`diagnostic_harness_test.rs`)。
- `alternatives` — 別の読みに基づく編集。常に `"maybe-incorrect"`、決して自動適用しない。
- `example` — スパン厳密な編集が無いときの表示専用スニペット(プレースホルダ可)。

`suggestions[]` は `primary` と `alternatives` を順に並べたもの。`almide fix --json` の
`suggestions` と LSP の quickfix も同じ `repair` から作られる(machine-applicable な
`primary` は LSP で preferred になる)。

**位置の単位**(#2250): `line` は 1 始まりの行、`col` / `end_col` は **1 始まりの文字数**
(Unicode スカラー値の個数)。バイトでも表示幅でもない — CJK を含む 66 文字(98 バイト)の行の
末尾挿入は `col = 67`。`end_col` は排他的(その列の直前まで)で、`col == end_col` は
ゼロ幅の挿入点。バイトで数えるハーネスは UTF-8 を壊す。

**位置は必ずファイル上の実在する場所を指す**: heredoc の 3 行目にある `${…}` はその行・その列で
報告される(かつては文字列の先頭行に、デコード済みテンプレート全体を数えた列で出ていた)。
`)` が後続行にある呼び出しは `Span` が終端行を持たないため `(` のスパンだけを運ぶ —
その場合 E041/E042 の `!` 修正は `try` にだけ出て、`try_replace` / `suggestions` は付かない
(式の途中に `!` を挿す位置を渡すより、位置を出さない)。
テスト: `tests/fixit_span_on_line_test.rs`、`tests/interpolation_span_parity_test.rs`。

**構文エラーも JSON で出る**: トップレベル宣言が 1 つも成立しないファイルは
`Parser::parse` が `Err` を返し、共有の `parse_file` は人間向けテキストを stderr に出して
終了する。`--json` はパーサから診断を直接取り出してこの経路でも JSON を出す
（LLM が最も多く出すエラー種別が唯一テキストのままだった）。

**終了コードは素の `almide check` と一致する**(#2350): パースエラーまたは
`level` が `error` の診断が 1 つでもあれば `1`、警告だけなら `0`。かつては
「パースできたファイルは一律 `0`」で、型エラーは JSON 行に出したうえで終了
コードからは捨てていた。`--json` を読むのは人間ではなくエディタ・CI・ハーネス
であり、その消費者は終了コードで分岐して失敗時だけ payload を見る — つまり
拒否したプログラムが全部「合格」として通っていた。`fmt --json` が最初から
`--check` と同じ終了コードを返しているのと同じ規則で、`check --json` だけが
例外だった。

テスト: `tests/mcp_test.rs`（`json_check_reports_a_total_parse_failure_as_json`）

**パッケージ全体の JSON**(#2253): FILE を省いた `almide check --json` は、素の `almide check`
と同じ `src/` 配下の全エントリを同じ順で判定し、全診断を 1 行 1 診断で出す。各行の `file`
がどのエントリの診断かを言うので、それ以外の複数ファイル向けの形はない(エントリごとの
`: ok` 行も出ない — 行は診断だけ)。終了コードは単一ファイル形の集約: clean でないエントリが
1 つでもあれば `1`。判定は短絡させない — 最初の不合格エントリで止めると、その後ろの診断が
レポートから丸ごと消えるため、全エントリを判定してから集約する。
`--effects` は関数ごとのレポートなので従来どおり単一エントリ解決のまま。

テスト: `tests/check_package_entries_test.rs`（`json_judges_every_entry_and_each_row_names_its_file`、
`json_exit_code_is_one_when_an_entry_does_not_parse`）、
`tests/check_json_exit_code_test.rs`（両モードの終了コード一致、および
「終了コードが赤 ⇔ error 行を出した」の同値）

エラーコード:

| コード | 説明 |
|---|---|
| E001 | 型の不一致 |
| E002 | 未定義の関数 |
| E003 | 未定義の変数 |
| E004 | 引数の数が違う |
| E005 | 引数の型が違う |
| E006 | 純粋関数から effect 関数を呼んでいる |
| E007 | 純粋関数内の fan ブロック |
| E008 | fan 内での var キャプチャ |
| E009 | let/パラメータへの代入 |
| E010 | 非網羅的 match |

診断コードの解説は [../diagnostics/](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/diagnostics) を参照（上表は代表例）。

---

<a id="almide-fmt"></a>

### `almide fmt`

ソースファイルのフォーマット。

行長の上限は **100 字** (#2161)。呼び出しの引数リストと record リテラルは、
次のどちらかなら 1 メンバー 1 行 (末尾カンマ付き、閉じ括弧は元のインデント) に
並べる:

- 書き手が既に行を分けている (メンバーがソース上で別々の行にある)
- 1 行にした形が、その開始位置から 100 字を越える

どちらでもなければ 1 行のまま。メンバーが 1 つの容器は積まない。どちらの
配置も不動点 (整形結果をもう一度整形しても変わらない)。

```bash
almide fmt                              # src/**/*.almd を整形
almide fmt app.almd                     # 指定ファイルを整形
almide fmt --check                      # 差分があれば非ゼロで終了（CI 用）
almide fmt --check --json               # 同じゲートを JSON 1 オブジェクトで（機械可読）
almide fmt --dry-run                    # 書き込みせず差分表示
almide fmt --no-import-edit stdlib/     # import 行を一切触らず整形(splice-context ソース用)
```

| オプション | 説明 |
|---|---|
| `--check` | 比較のみ。未整形ファイルを stderr に列挙し、あれば終了コード 1 |
| `--json` | `--check` の機械可読版（`--check` を含意）。stdout に 1 オブジェクト、終了コードは同じ |
| `--dry-run` | 整形結果を stdout に出すだけ。書き込まない |
| `--no-import-edit` | import 行を一切編集しない |

`--json` の出力:

```json
{"checked":2,"unformatted":["src/a.almd"],"unreadable":[],"verify_failed":false,"ok":false}
```

`--json` も `--check` も書き込みは一切しない。整形の適用は `almide fmt <path>`。

テスト: `tests/mcp_test.rs`（`fmt_check_json_reports_drift_and_keeps_the_gate_exit_code`）

<a id="コメントの付け先"></a>

#### コメントの付け先

fmt は整形後に再パースして AST 同一性とコメント数を検証し（E054、#1309）、置き場のないコメントがあればファイルを触らず拒否する。置き場は次の 4 つ（`ExprComments`、#1404 / #1714 / #1326）:

| 書かれた位置 | 付け先 | 再出力 |
|---|---|---|
| `f(/* c */ a)` — ノードの前、同一行 | 後続ノードの leading | `/* c */ a`（ノードが動けば随伴） |
| `f(a /* c */, b)` / `1 /* c */ + 2` — ノードの後、区切り・閉じ括弧・演算子の前 | 直前ノードの trailing | `a /* c */`（カンマを越えない） |
| `1 + // c` ↵ `2` / `xs // c` ↵ `\|> f` / `x // c` ↵ `.m()` — 行末、式は次行に継続 | **行を終えるオペランド**の line_trailing | `1 // c` ↵ `  + 2` — オペランド直後で改行し、演算子・`\|>`・`.` が継続行を先導（`...` だけは行末に残す） |
| `xs` ↵ `  // c` ↵ `  \|> f` — 継続の合間の独立行 | 直前オペランドの line_between | 継続インデントで独立行のまま |

行末 `//` を演算子より前に書いても後に書いても同じ出力になる（正規形は演算子先導）。キーワードや区切りで終わる行の `//`（`then // c`、末尾 `.`、`match { // c`）には付け先がなく、引き続き拒否される。

---

<a id="almide-compile"></a>

### `almide compile`

Module Interface を生成。外部ツール（binding generator 等）が型情報を読むための JSON / `.almdi` アーティファクト。

```bash
almide compile                          # プロジェクト全体
almide compile parser                   # モジュール名指定
almide compile app.almd --json          # JSON 出力（stdout）
almide compile --dry-run                # 人間向け表示
almide compile -o target/compile        # 出力ディレクトリ指定
```

JSON 出力の構造:

```json
{
  "module": "mathlib",
  "types": [{
    "name": "Point",
    "kind": { "kind": "record", "fields": [{"name": "x", "type": {"kind": "float"}}] },
    "abi": { "size": 16, "align": 8, "fields": [{"name": "x", "offset": 0, "size": 8}] }
  }],
  "functions": [{
    "name": "distance",
    "params": [{"name": "a", "type": {"kind": "named", "name": "Point"}}],
    "return": {"kind": "float"},
    "effect": false
  }],
  "constants": [],
  "dependencies": []
}
```

`abi` フィールドは具象型（ジェネリックでない）にのみ付与。C ABI のレイアウト（size, align, field offset）。

---

<a id="almide-mcp"></a>

### `almide mcp`

Model Context Protocol サーバを stdio で起動する。エージェント（Claude Code 等）が
コンパイラを **型付きツール呼び出し** として使うための口。人間向け出力を
モデルに読み解かせる工程を挟まないことが目的で、この工程こそが精度の漏れ口。

```bash
almide mcp                              # stdio で JSON-RPC 2.0（改行区切り）を待つ
```

対応メソッド: `initialize` / `tools/list` / `tools/call` / `ping`。
それ以外は JSON-RPC `-32601`（`capabilities` は `tools` のみを宣言する）。

ツール（6 つ。すべて CLI の既存の機械可読出力を経由する）:

| ツール | 実体 | 返すもの |
|---|---|---|
| `almide_check` | `almide check --json` | 構造化診断の配列（`try`/`try_replace` 込み） |
| `almide_test` | `almide test --json` | ファイル単位の pass/fail + ランナー生出力 |
| `almide_api` | `almide ide outline --json` / `stdlib-snapshot --json` | 公開宣言のシグネチャ一覧 |
| `almide_explain` | `almide explain <CODE>` | 診断コードの解説（markdown） |
| `almide_fmt_check` | `almide fmt --check --json` | 未整形ファイル一覧（書き込みなし） |
| `almide_survive` | `almide survive --with - --json` | 編集を書き込む前の生存デルタ（下の `almide survive`） |

設計上の制約:

- **コンパイラへの入口は 1 本**。各ツールは `current_exe()`（= 自分自身）を
  サブプロセスとして起動し、CLI が既に出している JSON を読む。MCP 専用の
  出力経路を作らない（作れば必ず CLI とドリフトし、しかも誰も目で読まないので
  ドリフトが見えない）
- **人間向けテキストは決してパースしない**。機械可読な形が無い箇所
  （テストの個別失敗詳細 = #1313）は `*_unstructured` という名前のフィールドに
  そのまま入れて返す
- **書き込みツールは無い**。`fmt` は `--check` 形のみ。`almide_survive` は編集を
  判定するだけで書かない。適用は CLI（`almide apply --if-survives` / `almide fmt` /
  `almide fix`）で行う — エージェント自身のトランスクリプトに編集が残る

Claude Code プラグイン定義（MCP + LSP）: `tools/claude-plugin/`、
マーケットプレイス: `.claude-plugin/marketplace.json`、導入手順: [../mcp.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/mcp.md)

テスト: `tests/mcp_test.rs`

---

<a id="almide-survive"></a>

### `almide survive`

提案された編集を **書き込む前に** 判定する（#2147）。編集をメモリ上で当て、
check・その編集に届くテスト・契約 fixture を編集の前後で走らせ、診断 / テスト /
契約ごとに `{unchanged, newly_broken, newly_fixed, removed}` を返す。ファイルは
決して書かない。

```bash
almide survive src/calc.almd --with fix.patch --json   # unified diff
almide survive src/calc.almd --with new.txt  --json    # ファイル全文
git diff src/calc.almd | almide survive src/calc.almd --with - --json
```

- `--with <PATH|->` — 編集。unified diff（`--- ` / `@@ ` で始まる）か、ファイルの
  新しい全文。`--as auto|patch|text` で読み方を固定できる（既定 `auto`）。
  diff の hunk は文脈行と削除行が完全一致しなければ適用しない（あいまい適用は
  別の編集を判定することになるので、exit 2 で拒否）
- **到達集合**: 編集したファイル自身と、プロジェクト根（`./almide.toml` があれば `.`、
  無ければファイルのディレクトリ）以下の `.almd` のうち import の閉包がそれを読むもの。
  check は全到達ファイル、テストは `test` ブロックを持つ到達ファイル、契約は
  `// @contract: C-NNN` を持つ到達ファイル（native と `--target wasm` の stdout と
  exit code のバイト一致）
- **メモリ上で当てる仕組み**: 各レッグは子 `almide` プロセスで、編集後側だけ
  隠し引数 `--survive-overlay <PATH> --survive-overlay-text <FILE>` を渡す（#4129 で
  環境変数から引数に移した。環境変数だと、開発者が管理していない環境がどのビルドにも
  ディスク上と違うテキストをコンパイルさせられたため）。コンパイラの
  `.almd` 読み取りはすべて `almide::source_overlay::read_to_string` を通り、対象
  ファイルの読み取りだけが提案テキストを返す。提案テキストはツリーの外（システムの
  一時ディレクトリ）に置く
- `--timeout <秒>`（既定 600）— 子プロセス 1 本ごとの上限。超えたらプロセス
  グループごと kill し、テストは `timeout`、契約は `timeout` と報告する
- exit: `0` = 生き残る、`1` = 生き残らない、`2` = 判定できない（読めない / hunk が
  当たらない）。`--json` 時は `2` も `{"schema_version":1,"error":"…"}` を stdout に出す

**生き残る** = error の診断・テスト・契約のどれも `newly_broken` に無く、
テスト・契約のどれも `removed` に無いこと（#4267）。テストを消せば何も
`newly_broken` にならないので、消すことは生き残ることではない。意図した削除は
`--allow-removed`（`survive` と `apply` の両方）で通す。MCP の `almide_survive` は
これを渡さない。名前は残ったまま、値を読むアサーション（`assert` / `assert_eq` /
`assert_ne` / `testing.assert_*` で、引数のどれかがリテラルでないもの）を全部失った
テスト（`assert(true)` や空の本体）は `newly_broken`（`after: "hollowed"`）になる。

warning の `newly_broken` は報告するが判定には入れない（`almide check` 自体が
warning で失敗しない。また未使用変数 lint は error の無いファイルでしか走らない
ので、最後の error を直すと元からあった warning が表に出る — これで拒否すると
直す編集ほど拒否される）。

**診断の同一性**（編集で行がずれる — ここが要）: 編集ファイルの前後テキストの
行 diff（LCS）で、変わらなかった行を編集前の行番号から編集後の行番号へ写す。
編集された行はどこにも写らない。編集ファイル以外は恒等写像。

1. `position` — 同じ check 対象・同じ `file`・同じ level / code / message で、
   編集前の位置を写した先が編集後の位置（行も列も）と一致
2. `position_renumbered` — 1 と同じだが、message は数字列を消すと一致
   （「first defined at line 12」のように行番号を引用する message は、同じ挿入で
   番号がずれる）
3. `content` — 前後どちらかが編集された行の上にあるとき、同じ対象・file・
   level / code / message なら同一（行を書き換えても error が残ったなら直っていない）

対にならなかった編集前の診断は `newly_fixed`、編集後の診断は `newly_broken`。
診断はそれ自体が悪い状態なので、check の `removed` は常に空。

**テストと契約の分類**（同一性はファイル + テスト名 / fixture + 契約 ID）:

| 編集前 | 編集後 | 分類 |
|---|---|---|
| あり | なし | `removed` |
| なし または 不良 | 良 | `newly_fixed` |
| なし または 良 | 不良 | `newly_broken` |
| 良 | 良 | `unchanged` |
| 不良 | 不良 | `unchanged` |

テストの状態は `pass` / `fail` / `ignored` / `not_run`（バイナリがそこまで
走らなかった）/ `compile_error` / `timeout`（良 = `pass` と `ignored`）。
契約は `holds` / `diverges` / `timeout`（良 = `holds`）。

**JSON（`schema_version: 1`）**:

```json
{
  "schema_version": 1,
  "file": "calc.almd",
  "edit": { "kind": "patch", "lines_before": 3, "lines_after": 3 },
  "written": false,
  "survives": false,
  "reach": { "root": ".", "checked": ["calc.almd", "calc_test.almd"], "tests": ["calc_test.almd"], "contracts": [] },
  "summary": { "check": {"unchanged":0,"newly_broken":0,"newly_fixed":0,"removed":0}, "tests": {"…": 0}, "contracts": {"…": 0} },
  "check": {
    "unchanged":    [ { "checked": "…", "matched_by": "position", "before": {…}, "after": {…} } ],
    "newly_broken": [ { "checked": "…", "before": null, "after": {…} } ],
    "newly_fixed":  [ { "checked": "…", "before": {…}, "after": null } ],
    "removed": []
  },
  "tests":     { "newly_broken": [ { "file": "calc_test.almd", "name": "add adds", "before": "pass", "after": "fail", "failure": {…} } ], "…": [] },
  "contracts": { "unchanged": [ { "fixture": "…", "contract": "C-NNN", "before": "holds", "after": "holds", "detail": {…} } ], "…": [] }
}
```

check の `before` / `after` は `almide check --json` が出した診断オブジェクトを
**そのまま** 運ぶ（`almide check --json` に増えるフィールドは変更なしで届く）。
`failure` は `almide test --json` の失敗レコードと同じ形。JSON 化されずに失敗した
check 実行は `check.unstructured` に stderr ごと残す（黙って clean 扱いにしない）。
形を変える（名前・型・分類規則の変更）ときは `schema_version` を上げる。

LSP からは独自リクエスト `almide/survive`
（params `{textDocument: {uri}, text?, patch?}`、どちらも無ければ開いているバッファを
提案とみなす）、MCP からは `almide_survive` で同じ JSON が返る。どちらも CLI を
子プロセスで走らせる。

テスト: `tests/survive_test.rs`（`tests/survive/` の golden delta 4 本 — テストを
壊す patch / 行挿入をまたいで残る error と直る error の全文編集 / 注釈だけの中立
patch / `repair`（#2149）付きの E031 をそのまま `newly_broken` に運ぶ patch — と、
書き込まないこと・apply の拒否と受理）、`tests/mcp_test.rs`、
`tests/lsp_test.rs`

---

<a id="almide-apply"></a>

### `almide apply`

`survive` の判定と書き込みを 1 手にする（verify-then-write）。

```bash
almide apply src/calc.almd --with fix.patch --if-survives        # 生き残るときだけ書く
almide apply src/calc.almd --with fix.patch --force               # 判定に関わらず書く（判定は報告する）
```

- `--if-survives[=true|false]` — 値は `true` / `false` のみ。それ以外
  （`maybe`、`1` など）は評価もせず書かずに exit 2（fail closed）。
  `--if-survives=false` は `--force` と併用したときだけ意味を持つ
- `--if-survives` も `--force` も無い `apply` は exit 2（書き込みには明示の門が要る）
- `--force` は値を取らないフラグ（`--force=yes` は usage error）
- 書き込みは原子的（同じディレクトリの一時ファイルに書いて fsync、元の権限を
  写して rename）。判定中にファイルが変わっていたら書かずに exit 2
- exit: `0` = 書いた、`1` = 生き残らないので書かなかった、`2` = 判定前に拒否
- `--json` の出力は `survive` の JSON に `written` と `forced` を足したもの

テスト: `tests/survive_test.rs`

---

<a id="almide-init"></a>

### `almide init`

新しいプロジェクトを作成。

```bash
almide init
```

生成物:

```
almide.toml               [package] name, version, edition
src/
  main.almd               effect fn main テンプレート
tests/                    テスト用ディレクトリ
CLAUDE.md                 AI 向けプロジェクト説明
```

`name` はカレントディレクトリ名から自動生成。

---

<a id="almide-update"></a>

### `almide update`

ロック済み git 依存を、その ref の**現在の remote head** へ前進させる(#1131)。

```bash
almide update almai      # 指定の依存だけ
almide update            # tag 固定でない全依存
```

- `almide.lock` は意図的に sticky(`fetch_all_deps` が pin を再利用して再現性を守り、
  既存依存への `almide add` も同じ pin を書き戻す)。前進の唯一の正規手段が本コマンド。
- **tag 固定の依存は動かさない**(マニフェストの要求そのものが変わるため。skip を報告)。
- 変更した entry だけを書き換え、他の pin はバイト単位で不変。
- 前進時は当該 ref のキャッシュディレクトリを破棄し、次のフェッチで再取得させる。
- `git ls-remote` 1 回で解決(clone しない)。
- ネットワークに触れる前に所有者を `[trust] owners` と照合する(#4093)。
- `almide.toml` の `git` がロックと違う依存(ソース変更)は、tag 固定でも古い pin を捨てて新しいソースから
  ロックし直す。ビルドはソース変更を拒否し、本コマンドを案内する([package-system.md §6.3](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/package-system.md))。

出力は `name <old12> -> <new12>`(初回ロックは `name -> <new12>`)。

<a id="almide-add"></a>

### `almide add`

依存パッケージを追加する。順序は「解決 → 所有者の信頼確認 → 存在確認(`git ls-remote`)→ フェッチ →
`almide.toml` と `almide.lock` の書き込み」(#4093)。拒否・失敗はすべて終了コード 1 で、両ファイルを一切変えない。

```bash
almide add bindgen                      # github.com/almide/almide-bindgen(パッケージ一覧のスナップショットで解決)
almide add almide/almide-bindgen        # 同上(パッケージ名 bindgen)
almide add user/repo@v0.1.0 --trust github.com/user   # github.com/almide 以外の所有者は --trust が要る
almide add --git https://example.com/team/repo.git --tag v1.0 --trust example.com/team mylib
almide add almide-graphics/ceangal2@v0.1.0 --subdir ceangal --trust almide-graphics  # サブディレクトリのパッケージ
almide add user/repo --dry-run          # 解決・信頼判定・存在確認だけを表示し、何も書かない
```

| フラグ | 意味 |
|---|---|
| `--trust <owner>` | ソースの所有者(`host/owner`、github.com なら `owner` だけでも可)を信頼し `[trust] owners` に書く。解決した所有者と一致しなければエラー |
| `--dry-run` | 解決結果・信頼判定・存在確認を表示して終了。フェッチも書き込みもしない(拒否される追加なら終了コード 1) |

`--yes` / `--force` や、すべてを信頼する環境変数は存在しない。所有者を書いた許可だけが通る
([package-system.md §6.3](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/package-system.md))。存在しないリポジトリは not found として報告し、近い
almide パッケージがあれば実行できる `did you mean \`almide add <x>\`` を添える(提案だけで置き換えない)。

短縮記法:
- `almide add name` → github.com/almide の `name` パッケージ(パッケージ名とリポジトリ名が違う場合は
  埋め込みスナップショット `src/almide_packages.txt` で引く。無ければ `https://github.com/almide/{name}`)
- `almide add user/repo` → `https://github.com/{user}/{repo}`
- `@v0.1.0` → `tag = "v0.1.0"`
- `--subdir <dir>` → `subdir = "<dir>"`(#3381、[package-system.md §6.1](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/package-system.md))。パッケージ名は
  subdir の最後の要素(`--git` 指定時は第 1 引数)。

フェッチと検査(subdir の存在・`almide.toml` の package name が依存名と一致・推移的依存の所有者)が
通ってから `almide.toml` と `almide.lock` に書き込む。失敗時は両方を元に戻す。

---

<a id="almide-trust"></a>

### `almide trust`

所有者を `almide.toml` の `[trust] owners` に加える(#4093)。`almide add` を経ない依存(推移的依存、
手書きのマニフェスト行)が拒否されたとき、その拒否メッセージが示す 1 手。

```bash
almide trust github.com/bob     # [trust] owners = ["github.com/bob"]
almide trust bob                # github.com/bob と同じ
```

---

<a id="almide-install"></a>

### `almide install`

```bash
almide install github.com/almide/<repo>          # 既定のブランチの最新コミット
almide install <spec> --tag v1.2.0               # タグ
almide install ./path/to/project                 # ローカルのディレクトリ
almide install <spec> --name tool --bin-dir ~/bin
```

パッケージを取得し、そのパッケージをルートプロジェクトとして `--release` でビルドして、bin ディレクトリ
（`--bin-dir`、`ALMIDE_INSTALL`、既定 `~/.local/bin` の順）に置く。

- **信頼**（#4184）: 取得元の持ち主（`host/owner`）は、既定で信頼される `github.com/almide` か、
  `--trust <owner>` で名指ししたものに限る。それ以外は**取得する前に**拒否する。`almide add` と同じ規則。
  ローカルのディレクトリには信頼は要らない。
- **ネイティブコード**（#4184）: パッケージ自身のネイティブコード（`native/` の Rust、`[native-deps]`
  のクレートとその build script、`@extern(rust|c)`）は、ビルド時に利用者の権限で動く。
  `[permissions] native` が見るのは依存だけなので、install はそれを列挙し、`--allow-native`
  が無ければ拒否する。純 Almide のパッケージはフラグ無しで入る。
- **置き場所**（#4183）: 名前（`[package] name` か `--name`）は 1 つの素の識別子
  （英字・数字・`_`、数字で始まらない）でなければならない。出力は bin ディレクトリ内の一時名に作ってから
  rename で置くので、宛先の symlink の先には決して書かない。almide が入れていない既存のファイル
  （bin ディレクトリの `.almide-installed` に記録が無いもの）や symlink は、`--force` が無ければ置き換えない。

テスト: `tests/install_destination_test.rs`、`tests/install_native_gate_test.rs`

<a id="almide-deps"></a>

### `almide deps`

依存パッケージの一覧を表示。依存を取得し、各パッケージ(直接依存の後に、それらを通じて
入る依存)について version、ケイパビリティ集合(`[permissions].allow` が検査するカテゴリ。
`@extern` fn は全カテゴリ)、持ち込むネイティブコード(`native/`、`[native-deps]`、
`@extern(rust|c)` fn)と、それが `[permissions].native` で許可されているかを表示する(#4093)。
許可の無いネイティブコードを持つ依存は `build` / `run` / `test` / `bench` が拒否する
(`docs/specs/package-system.md` §6.4)。

```bash
almide deps
# bindgen = https://github.com/almide/almide-bindgen (v0.1.0)
#   version: 0.1.0
#   capabilities: none
#   native code: none
# nat = path ../nat
#   version: 0.1.0
#   capabilities: Abort, Env.read, Env.write, FS.read, FS.write, Fan, IO.stderr, IO.stdin, IO.stdout, Net.fetch, Net.listen, Proc, Rand, Time
#   native code: native/natval.rs; @extern fn `number` (src/mod.almd:2) (granted by [permissions].native)
# ceangal = https://github.com/almide-graphics/ceangal2 (v0.1.0) subdir ceangal
#   ...
```

---

<a id="almide-dep-path"></a>

### `almide dep-path`

依存パッケージのローカルキャッシュディレクトリを出力。

```bash
almide dep-path bindgen
# ~/.almide/cache/.sources/<encoded-source>/@/<commit>/src
almide dep-path ceangal     # subdir 依存: 共有クローン内のパッケージディレクトリ
# ~/.almide/cache/.sources/<encoded-source>/@/<commit>/ceangal/src
```

用途: 依存パッケージの `.almd` ファイルを `process.exec("almide", ["run", path])` で実行する場合のパス取得。

---

<a id="almide-clean"></a>

### `almide clean`

キャッシュをクリアする。対象は 6 つ:

| 対象 | 場所 |
|---|---|
| 依存キャッシュ | `~/.almide/cache/` |
| インクリメンタルキャッシュ | `./.almide/cache/` |
| コンパイルキャッシュ | `./target/compile/` |
| native ビルドスクラッチ(#2500) | `ALMIDE_RUN_PROJECT_DIR`（既定 `<cache base>/run`）と `<cache base>/build-cdylib` |
| `almide test` の worker dir(#2504) | `<cache base>/test/native/<key>/` を 1 つずつ |
| ランタイム rlib(#2504) | `<cache base>/almide-rtlib-<key>/` を 1 つずつ |

**ビルドスクラッチの「空にする」は完了形の振る舞いであって、やり残しではない。** 各 dir の
`.almide-build.lock` を取ってから中身を消す（進行中のビルドは完了してから消える）ので、
dir は lockfile だけを持つ空ディレクトリとして残る。lockfile を消さないのは意図的で、消すと
「その lockfile を開いて待っている builder」と「次に来て新しい lockfile を作る builder」が
**別 inode をロックして排他が壊れる** — 同じ dir で 2 つのビルドが同時に走り、片方のバイナリが
もう片方のものになる(#1877 と同じ壊れ方)。消えるのは容量（worker dir なら 1 本あたり数 MB）、
残るのは 0 バイトのファイル 1 つ。

空にした dir ごとに `Cleaned <path>` を stderr に 1 行出す（worker dir と rlib dir はそれぞれ
`Cleaned <native> (N test worker dir(s))` / `Cleaned <temp>/almide-rtlib-* (N runtime rlib dir(s))`
と 1 行にまとめる）。何も無ければ `No cache to clean`。

```bash
almide clean
```

テスト: `tests/run_cache_recovery_test.rs`

---

<a id="almide-verify"></a>

### `almide verify`

`almide build --target wasm` が出荷する wasm モジュールの証明書（ownership / call-modes / bounds / names / caps / caps-transitive の witness）を、**独立版数の別バイナリ `almide-verify`** に再検査させる (#2152, #4011)。`almide verify` 自身は検査器を持たない subprocess shim で、`almide-verify` を **almide 実行ファイルの隣 → PATH** の順に探して起動し、標準入出力と終了コードをそのまま返す。見つからなければ **linked fallback は無く**、名前付きエラー `error[verifier-missing]` で終了コード 127。

```bash
almide build app.almd --target wasm               # app.wasm を書く
almide verify app.almd                            # 証明書 bundle を生成し、app.wasm の SHA-256 に結びつけて almide-verify bundle に渡す
almide verify app.almd --artifact out/app.wasm    # 結びつける成果物を明示する（無ければ・一致しなければエラー）
almide verify app.almd --emit app.bundle          # bundle をファイルに残す（almide-verify が無くても書く）
almide verify --mir app.almd --emit m.bundle      # MIR lowering の witness（coverage.sh 用、ビルドとは無関係）
almide verify ownership w.cert                    # .almd 以外の引数は almide-verify にそのまま渡る
almide-verify --version                           # 検査器自身の版数（コンパイラとは独立）
```

- `almide` 側で走るのは **untrusted な producer** だけ。`almide build --target wasm` と同じ経路（structural leg）でプログラムをビルドし、そのビルドが emit 時に記録した witness を bundle に書く（`crates/almide-wasm/src/witness_bundle.rs`。CI の `proofs/structural-wall.sh` が判定する bundle と同じ producer）。判定は常に `almide-verify`。
- **成果物への結びつけ**: `--artifact` の名指し、または `almide build` の既定出力（`app.almd` なら `app.wasm`）が在れば、そのバイト列がこのビルドの出すモジュールと一致することを確かめ、SHA-256 を bundle（version 2 の `artifact` 行）に書く。`almide-verify` はファイルから digest を計算し直し、不一致なら REJECT。一致しない（古い）ファイルは producer が拒否する（終了コード 1）。ファイルが無ければ version 1 の bundle（ビルドが出すモジュールについての判定）になり、その旨を note で出す。
- `http.serve` などで component として出荷されるプログラムは対象外（終了コード 1、理由を表示）。対象はプレーンな `almide build --target wasm` が書く core module。
- `almide-verify` はコンパイラの crate を一切リンクしない（`crates/almide-verify`、依存ゼロ）。各性質の判定は `proofs/` の Coq 検査器（`check_xc` / `check_names_cert` / `check_caps_cert` / `check_prog_cert` / `check_modes_cert` / `check_bounds_cert`）の転写で、定理は Rocq kernel が検査する。
- witness を記録できなかった frame は bundle に `uncertified` として名前つきで載る（黙って飛ばさない）。
- `./almide.toml` の `[permissions].allow` があれば `effect fn` の宣言 capability をそれに絞る（caps witness が reject できるようになる）。plain `fn` の上限は console 出力（ADR-0022）で、manifest では広がらない。
- `almide verify --mir app.almd [--emit b]` は #4011 以前の挙動: MIR lowering の witness（関数ごとの ownership / names / caps と call-modes）を書く。#2761 以降 MIR から出荷される wasm バイトは無いので、これはビルドではなく MIR についての判定。残している理由は `proofs/coverage.sh` がこの経路で MIR の証明書 producer を駆動し、その coverage floor が ratchet だから（`--artifact` とは併用できない）。MIR の witness を判定する gate は `proofs/gate.sh` / `proofs/corpus-wall.sh`。

`almide-verify` の終了コード: 0 = 全 witness ACCEPT（CERTIFIED）、1 = REJECT あり、または witness が 0 件、3 = 全 ACCEPT だが uncertified な関数あり（INCOMPLETE）、2 = 使い方の誤り・読めない入力・不正な bundle。

テスト: `tests/verify_shim_test.rs`（shim の委譲・不在時の名前付きエラー・終了コード転送・成果物への結びつけと古いファイルの拒否・manifest を超える capability の REJECT）、`crates/almide-verify/tests/coq_examples.rs`（Coq の全 `Example` と `build-checker.sh` の固定行）、`crates/almide-verify/tests/cli.rs`

---

<a id="legacy-mode"></a>

## 旧来のモード

ファイル名が `.almd` で終わる引数を最初に指定すると、`emit` コマンドとして扱われる:

```bash
almide app.almd --target rust           # → almide emit app.almd --target rust
almide app.almd --target rust --repr-c  # #[repr(C)] 付き Rust 出力
almide app.almd --emit-ast              # AST を JSON で出力
almide app.almd --emit-ir               # 型付き IR を JSON で出力
```

---

<a id="exit-codes"></a>

## 終了コード

| コード | 意味 |
|---|---|
| 0 | 成功 |
| 1 | コンパイルエラー、テスト失敗、依存解決失敗、fmt --check の差分など |
| 2 | コマンド固有の使い方・入力・判定不能エラー（各節参照） |
| 3 | almide verify: witness は ACCEPT だが未認証関数が残る INCOMPLETE |
| 5 | almide test: 実行対象が 0 件（--allow-no-tests で変更可） |
| 127 | `almide verify`: 独立検査器 `almide-verify` が見つからない（`error[verifier-missing]`） |

---

<a id="global-options"></a>

## 共通オプション

| オプション | 説明 |
|---|---|
| `-h, --help` | ヘルプ表示（各コマンドにも付く） |
| `-V, --version` | バージョン表示（例: `almide 0.34.2`） |

---

<a id="環境変数"></a>

## 環境変数

`ALMIDE_*` の環境変数は `almide_base::env::SWITCHES`（`crates/almide-base/src/env.rs`）が
唯一の台帳で、`almide switches` で列挙できる（#2205）。下表は `almide switches --md` の出力
そのもので、`scripts/check-env-switches.sh`（CI `checks`）が台帳に無い名前・コンパイラ側の直接の
`std::env::var("ALMIDE_…")` 読みを、`tools/almide-gates env-switches-doc` がこの表と台帳の差分を拒否する。

- 真偽値の意味は一つ: 未設定・空・`0`・`false`・`off`・`no` が OFF、それ以外は ON。
- 種別 `route`（実行するレッグを強制する）と `gate`（既定の検査を迂回する）のスイッチが ON のときは、
  そのプロセスで最初に読まれた時点で stderr に 1 行（`[almide] ALMIDE_X is set: …`）が出る。
  強制されたルートで出た verdict は、そう名乗る。
- **スイッチ錠**（#4129）: 種別 `route` / `gate` / `ablation` / `trap` / `instrument` は「何をコンパイルするか・どの検査を走らせるか」を
  変える。リリースビルド（`almide --version` が `(release…)` と名乗るもの）は、こうしたスイッチを環境から
  受け継いでも無視し、stderr に `[almide] ALMIDE_X is set but IGNORED: …` と 1 行出す。効かせるには
  コマンドラインで `--allow-switch ALMIDE_X` と名指しする（複数回可）。CI、`.envrc`、devcontainer、
  エディタのタスクなど、開発者が管理していない環境が、レビューしたものと別のソースをコンパイルさせたり
  検査を飛ばさせたりできないようにするため。ツールが自分で立てるスイッチ（`almide test` の
  `ALMIDE_NO_RTLIB`、`--time-report` の `ALMIDE_FUEL_PROBE`）は錠の対象外。開発ビルドは従来どおり
  環境を読み、隠しフラグ `--lock-switches` でリリースと同じ錠をかけられる。
  錠が何を抑えるかは各スイッチが宣言した種別で決まるので、その宣言を検査する: 錠の外の種別
  （`debug` / `tool` / `runtime` / `harness` / `ci`）のスイッチは、どれを ON にしても生成される
  Rust・wasm・ネイティブプログラムの出力が変わってはならない（`tests/switch_scope_test.rs`）。
  リリースビルドが既定で錠をかけることは `almide_base::env::lock_engaged_by_default` の単体テストと、
  リリースワークフローが成果物そのものに走らせる `scripts/check-release-switch-lock.sh` で確かめる。
- `--profile critical` は、こうしたスイッチが環境から 1 つでも ON で渡ると、`--allow-switch` の有無に
  関係なく拒否する（exit 1）。
- `instrument` は生成物に計測（割り当てカウンタ、固定・毒入れアロケータ）を足すもの: プログラムの
  計算は同じだが出荷物とは別物なので錠の対象。
- `harness` はテストハーネス（`tests/`, `crates/*/tests`, `tools/`）が読むフック、`ci` はワークフローと
  `scripts/` だけが使うもの、`runtime` はコンパイルされたプログラム自身が実行時に読むもの。

> 原文固定スナップショットの表。この読者向け写しは生成・自動同期の対象ではない。上記は正本とその検査の説明であり、この翻訳表への検査を示すものではない。

| 変数 | 種別 | 説明 |
|---|---|---|
| `ALMIDE_ABORT_SITE=value` | runtime | 親がテストバイナリを再実行するときに設定する、ネイティブテスト子プロセスが実行する testing.assert_aborts の箇所（ADR-0029）。手動では決して設定しない |
| `ALMIDE_ALLOC_COUNT` | instrument | `__almide_main` が戻るとき stderr に `__ALMD_ALLOC allocs=N deallocs=N reallocs=N peak=N` を出力する計数アロケータ付きでネイティブプログラムをビルドする（ネイティブ borrow オラクルの割り当てレーン、tests/native_borrow_oracle_test.rs） |
| `ALMIDE_BANG_RETURN` | ablation | v1 lowering の位置ごとの `!` 脱糖を OFF にし、すべての `!` が bind 位置の規則に到達するか、明確に拒否されるようにする（decline-matrix プローブ） |
| `ALMIDE_BASELINE_BIN=value` | harness | ソケット経路が導入される前のコンパイラ。tests/net_wasm_default_route_test.rs は、ソケットを使わないプログラムの preview 1 モジュールをこれとバイト比較する |
| `ALMIDE_BENCH_DIR=value` | harness | 構造的レッグの性能プローブの fixture ディレクトリ（既定は `crates/almide-wasm/tests/perf`） |
| `ALMIDE_BENCH_MAIN_NS=value` | harness | `almide bench` が測定対象のネイティブバイナリに指定するファイル。差し込まれた `main` のタイマーが経過ナノ秒を書き込む（#2980、main のみを測る境界） |
| `ALMIDE_BIN=value` | harness | テストハーネス・スクリプト・ワークフローが駆動する `almide` バイナリのパス（既定は `target/release/almide`、次に PATH） |
| `ALMIDE_BORROW_OWN_ALL` | ablation | 推論が存在する前と同様、BorrowInsertion が borrow 可能なすべてのパラメータを所有するようにする。所有権認証器の C4 感度テストが使うアブレーションであり、borrow 推論の性能調整スイッチ |
| `ALMIDE_BOUNDED_DEBUG` | debug | bounded-loop の bind が辞退した理由を出力する（v1 lowering） |
| `ALMIDE_BUILD_PROVENANCE=value` | ci | `build.rs` がビルド時に読む。`release` なら `almide --version` は `(release)`、それ以外（未設定を含む）なら `(dev)` と表示する。タグからビルドする唯一のもの、`.github/workflows/release.yml` だけが設定するため、release を名乗るバイナリはそこから来たはず、という位置づけ（#2384） |
| `ALMIDE_BUILD_SHA=value` | ci | `build.rs` がビルド時に読む。`almide --version` がビルド種別の隣に表示するコミットで、9 文字に切り詰める。コミットごとにルート crate が再ビルドされるのを避けるため、ビルドスクリプトで git から読むのではなく `make install` とリリースワークフローが渡す（#2384） |
| `ALMIDE_CACHE_MAX_BYTES=value` | tool | ユーザー専用の非公開キャッシュ基点にある各ネイティブビルドキャッシュのサイズ上限。`almide test` のワーカーキャッシュ（`<cache base>/test/native`、最も長く使われていないワーカーディレクトリから空にする）と、ビルド用作業ディレクトリのプログラム別バイナリ・オブジェクト（`<cache base>/run`）が対象。`4G`（既定）、`512M`、バイト数、または上限なしを表す `0`/`off`（#2608） |
| `ALMIDE_CAPTURE_MOVE_OFF` | ablation | クロージャだけが使う値を move する代わりに、#2231 より前のように CaptureClone がすべてのキャプチャを再び clone するようにする。所有権認証器の感度テストが使うアブレーション |
| `ALMIDE_CERTIFY_OWNERSHIP=value` | debug | パスパイプライン後にネイティブ所有権認証器を実行する（#2231）。`report` は全違反を出力、`fail` は 1 件でもあればビルドを中止、`off` は省略する。未設定なら debug ビルドで `fail`、release ビルドで `off` |
| `ALMIDE_CI_BASE=value` | harness | 分割 CI 実行が変更自身のファイルを見つけるため HEAD と比較するコミット（PR の base sha、merge group の base_sha）。変更が触れるすべての fixture を `ALMIDE_CI_SLICE` の分割対象に加える。scripts/ci-slice-prepare.sh がこれを `ALMIDE_CI_TOUCHED` に解決する |
| `ALMIDE_CI_SEED=value` | harness | 先頭 7 桁の 16 進数で `ALMIDE_CI_SLICE` の剰余クラスを回転させるコミット sha。連続するコミットで異なる分割を判定するためのもの（CI は `github.sha` を渡す）。未設定ならオフセット 0 |
| `ALMIDE_CI_SLICE=value` | harness | `k` > 1 のとき、コーパスを走査する各テスト・スクリプトは、コミットで回転する fixture の 1-in-k 分割と変更が触れる fixture を判定し、コーパス全体の鮮度・完全性検査は省略し、台帳を一切書き換えない（crates/almide-corpus/src/ci_slice.rs、scripts/lib/ci-slice.sh）。CI は基点が main でない PR と merge group で設定する。未設定または 1 なら従来どおり全実行 |
| `ALMIDE_CI_TOUCHED=value` | harness | 変更が触れるリポジトリ相対パスを 1 行ずつ列挙するファイル（scripts/ci-slice-prepare.sh が書く）。`ALMIDE_CI_BASE` に対して `git diff` を実行する代わりに読む |
| `ALMIDE_COMPILER_STACK=value` | tool | コンパイラドライバースレッドのスタックサイズ（バイト単位、既定 256 MiB）。これをオーバーフローさせる深い入力が回帰テストの対象 |
| `ALMIDE_COMPONENT_ADAPTER` | route | `--component` を直接の component 出力ではなく preview1 アダプタ経由にする |
| `ALMIDE_COMPONENT_P3` | route | `--component` で WASI 0.3 component を出力する（component-model streams 経由の標準入出力、非同期 canonical ABI）。op 集合に tcp / udp / name-lookup op が含まれるプログラムは、preview 1 にソケットがないため、このスイッチなしでもこの component としてビルドされる。機能フラグなしの標準 wasmtime 46+ で動く（`wasmtime run app.wasm`、http クライアントなら `-S http` — ADR-0023 step 1） |
| `ALMIDE_CONTROL_ADMIT_CODEC_AT_INPUT` | ablation | `@input_sink` パラメータ（`http.decode_json[T]`）で、`Encode` でもある型（`Codec` 型）を E110 にせず受け入れる。row-22 入力型ゲート（tests/input_sink_test.rs）の負の対照で、over-post セルが E110 から `is_admin=true` に反転しなければならない。それ以外では決して設定しない |
| `ALMIDE_CONTROL_ADMIT_HAND_DECODE` | ablation | リクエスト decode フローゲート（E110 のフロー側、#4232）が、送出にも使う型へ手動 decode したリクエストデータ（`User.decode(json.parse(http.body(req))!)` で `Codec` 型へ）を受け入れるようにする。tests/input_sink_test.rs の負の対照で、hand-decode プログラムが E110 から `is_admin=true` に反転しなければならない。それ以外では決して設定しない |
| `ALMIDE_CONTROL_BIN=value` | harness | セキュリティ経路テスト（`tests/effect_value_laundering_test.rs`、`proc_category_test.rs`、`permissions_routes_test.rs`）が検査対象の代わりに実行する `almide` バイナリ。負の対照であり、検査を欠くビルドではそのアサーションが失敗する（#4151、#4155） |
| `ALMIDE_CONTROL_IGNORE_SKIP_DECODE` | ablation | Codec derive が入力から `@skip_decode` フィールドを再び読むようにする。#4108 フィールド省略行列ゲート（tests/codec_field_skip_test.rs）の負の対照で、`admin=false` セルが反転しなければならない。それ以外では決して設定しない |
| `ALMIDE_CONTROL_NO_TYPE_SHARING` | ablation | 型を clone するたび、要素リストを共有せず木全体をコピーするようにする（ADR-0030 D5）。型共有ゲート（tests/compile_budget_test.rs）の負の対照で、型サイズ予算を解除した倍増チェーンでは、その結果、指数的個数の型リストを構築しなければならない。それ以外では決して設定しない |
| `ALMIDE_CORPUS_FILTER=value` | harness | 3-way オラクルテストが評価する fixture パスに対する部分文字列フィルタ |
| `ALMIDE_CORPUS_SHARD=value` | harness | `k/N`（1 始まり）は 6 つの大規模コーパステスト（#2381）で、ソート済み spec/wasm_cross コーパスの k 番目の剰余分割を走査する。ソート後に読む。`merge/N` は `ALMIDE_CORPUS_SHARD_DIR` から N 分割の部分結果を読み、コーパス全体の上限と名前キーの bridge 台帳を判定する。未設定なら未分割ゲート |
| `ALMIDE_CORPUS_SHARD_DIR=value` | harness | 分割コーパスゲートが走査済み fixture 一覧と部分集計・bridge 名を書き、`merge/N` がそれらを読むディレクトリ。未設定なら何も書かないローカル分割 |
| `ALMIDE_CORPUS_WEIGHTS_DIR=value` | harness | コーパスゲートが fixture ごとの実測経過時間を記録するディレクトリ（`weights/<column>.<gate>[.<k>-of-<N>].txt`、`stem<TAB>ms`）。scripts/gen-corpus-weights.sh が proofs/corpus-weights.txt に整形し、均衡化した `k/N` 分割がその表を読む（#2457）。CI は shard-partials ディレクトリを指定し、コミットする表をランナー自身の比率から生成する（#2502）。未設定なら記録しない |
| `ALMIDE_COVERAGE_CONDITION=value` | ci | coverage ratchet の条件タグ（push をどのベースライン行に対して判定するか） |
| `ALMIDE_COVERAGE_NATIVE_LEG=value` | ci | `1` なら proofs/coverage.sh がネイティブ codegen ワークロードの `almide test spec/ --target rust` も実行する（#3195）。既定は ALMIDE_COVERAGE_CONDITION |
| `ALMIDE_CWD=value` | runtime | 相対 fs パスが native と同じように解決されるよう、`almide run` が wasm ホストに設定する書き手の作業ディレクトリ（C-137）。手動では決して設定しない |
| `ALMIDE_DBG_ANF` | debug | lambda lift または文 inline が辞退した理由を出力する（v1 lowering） |
| `ALMIDE_DBG_BANG` | debug | v1 bind lowering の `!` unwrap 判断を出力する |
| `ALMIDE_DBG_BORROW=value` | debug | 値を含むネイティブ borrow 推論の署名キーを不動点反復ごとにすべてダンプし、各呼び出し箇所が参照するキーもダンプする |
| `ALMIDE_DBG_CELLS` | debug | キャプチャされた変数・変更された変数・cell 化された変数の集合を出力する（v1 lowering） |
| `ALMIDE_DBG_CONTLIFT` | debug | poison-oracle continuation lift がロールバックした理由を出力する（v1 lowering） |
| `ALMIDE_DBG_DESUGAR_FN=value` | debug | 値で指定された fn の完全に脱糖済みの本体を出力する（v1 lowering。#2205 より前は `DBG_DESUGAR_FN`） |
| `ALMIDE_DBG_DESUGAR_RAW` | debug | `ALMIDE_DBG_DESUGAR_FN` と併用して、脱糖前の生の本体も出力する（以前は `DBG_DESUGAR_RAW`） |
| `ALMIDE_DBG_ELEM` | debug | リストリテラルの Block 要素が辞退した理由を出力する（v1 lowering） |
| `ALMIDE_DBG_FAN` | debug | fan lowering の prefetch・パターン・インスタンス並列化の判断を出力する（構造的レッグと組み込みホスト） |
| `ALMIDE_DBG_GINIT` | debug | 先行実行する top-let 初期化ランナーの受け入れ集合と、拡張ランナーが辞退した理由を出力する（v1 lowering、C-007） |
| `ALMIDE_DBG_LINK` | debug | wasm リンクの要求集合と、各キーの解決先をダンプする |
| `ALMIDE_DBG_LOWER_FN=value` | debug | 値で指定された fn について、v1 lowering が実際に lower する完全に脱糖済みの本体を出力する（以前は `DBG_LOWER_FN`） |
| `ALMIDE_DBG_NESTED_MATCH` | debug | 入れ子 match チェーンが拒否された理由を出力する（v1 lowering） |
| `ALMIDE_DBG_QQ` | debug | 各 `??` を lower した経路を出力する（match 優先か経路 fallback か、v1 lowering） |
| `ALMIDE_DBG_ROUTER` | debug | 登録署名に対して拒否された stdlib 呼び出し名を、不一致と引数の型とともに出力する（v1 lowering） |
| `ALMIDE_DBG_TCO` | debug | 末尾呼び出しからループへの変換の受け入れ判断を出力する（v1 lowering） |
| `ALMIDE_DBG_TRAP` | debug | 組み込みホストが wasm trap を捕捉したとき、その backtrace とホスト状態を出力する |
| `ALMIDE_DBG_WHILE` | debug | while ループ lowering の判断を出力する（v1 lowering） |
| `ALMIDE_DEBUG_CALL_OPS` | harness | lower 済みの各 fn の call op を出力する（classify_corpus example） |
| `ALMIDE_DEBUG_EFFECTS` | debug | effect 推論パスの関数ごとの結果を出力する（ネイティブ codegen） |
| `ALMIDE_DEBUG_MIR_OPS` | harness | 全 MIR op をインデックスとともに出力する（classify_corpus example — 証明書の二分探索用計測器） |
| `ALMIDE_DEFAULTS_DEBUG` | debug | default キーが見つからなかったとき、レコードの default 解決を出力する（ネイティブ codegen） |
| `ALMIDE_DEPS_CACHE_ONLY` | tool | ディスクに既にあるものだけで依存関係を解決する。キャッシュにない依存関係はエラーとなり、clone は一切せず、`almide.lock` も決して書かない。`almide mcp` がすべてのツール子プロセスに設定する（#4179） |
| `ALMIDE_DISABLE_OPT` | ablation | 最適化パイプラインで任意のパスをすべて OFF にする（アブレーション。常時有効の前提パスは実行される） |
| `ALMIDE_DUMP_IR=value` | debug | ネイティブパイプラインでは指定パス後の IR をダンプする（カンマ区切り、または `all`）。v1 パイプラインでは名前に値を含む各 fn のチェーン処理後の本体をダンプする |
| `ALMIDE_DUMP_MIR` | debug | ネイティブ render の前に、lower 済みの各 fn の op 列を出力する |
| `ALMIDE_DUMP_VERIFY` | debug | ネイティブ render の検証記録を出力する |
| `ALMIDE_DUMP_WMIR=value` | debug | 名前に値を含む各 fn の、lower 済み wasm レッグ op 列を出力する |
| `ALMIDE_DWARF_FIXTURES=value` | harness | DWARF 行テーブルの span 解決測定（`crates/almide-wasm/tests/dwarf_lines.rs`、ignored テスト）で測定する `.almd` ファイルをカンマ区切りで指定する（#1315） |
| `ALMIDE_EXPECT_BASELINE` | harness | ALMIDE_BASELINE_BIN が未設定のとき、tests/net_wasm_default_route_test.rs がバイト比較を省略するのではなく失敗するようにする |
| `ALMIDE_EXPECT_TOOLS` | harness | 外部ツール（wasmtime、wasm-tools）がないとき、ハーネステストを省略せず失敗させる。CI が設定する |
| `ALMIDE_F32_SWEEP_N=value` | harness | Float32 printer sweep が出力（`${x}` と `float32.to_string`）し、各レッグで Rust f32 Display と比較する xorshift32 ビットパターンの個数（既定 100000、tests/float32_to_string_cross_target_test.rs） |
| `ALMIDE_FAKE_RUSTC_VERSION=value` | harness | tests/rustc_floor_test.rs の偽 `rustc` が報告するバージョン。それ以外の呼び出しはすべて `ALMIDE_REAL_RUSTC` に転送する（rustc 下限による拒否のプローブ） |
| `ALMIDE_FALLBACK_NAMES` | tool | `almide test` が、wasm レッグで合格しなかったファイルごとに `FALLBACK <file>` を 1 行出力するようにする。wasm coverage ratchet へのデータ供給 |
| `ALMIDE_FAN_COST_OFF` | ablation | コストモデル（#3341）が逐次のままにするすべての `fan` 提案を並列化する。両レッグ（native スレッドと組み込み wasm ホストのインスタンス）で #3341 より前の動作にする。クロスオーバー測定と並列経路テストが使うアブレーション |
| `ALMIDE_FAN_PAR_OFF` | ablation | 構造的レッグのインスタンス並列化の提案から `fan` チャンク map を除外する（#3003）。すべてのチャンクはその実行自身のインスタンスで逐次実行する |
| `ALMIDE_FAN_SEQUENTIAL` | runtime | ネイティブランタイムで `fan.*` を逐次実行し、`--host js` の fan は overlap プロトコルの代わりに要素ごとに 1 回中断する非同期フック上に構築する（#3383）。測定用の決定性調整スイッチで、観測可能な結果は契約上同一 |
| `ALMIDE_FAN_THREADS=value` | runtime | 両レッグ（native スレッドと組み込み wasm ホストのチャンクインスタンス）で `fan` の並列 map のワーカー数を制限する（#3003。スレッド数スケーリング曲線の測定用調整スイッチ。観測可能な結果は契約上同一。既定は利用可能な並列度） |
| `ALMIDE_FLAGS_BIN=value` | tool | 測定対象がそのフラグ導入前の場合（#3883）、research/benchmark/perf/bench.py が native の `--release` rustflags（`almide build --print-release-rustflags`）を問い合わせるコンパイラ。測定対象ビルドと release フラグが一致しなければならない。未設定なら測定対象コンパイラに問い合わせる |
| `ALMIDE_FLOAT_SWEEP_N=value` | harness | float printer sweep が出力し、各レッグで Rust `format!` と比較する xorshift64 ビットパターンの個数（既定 100000、tests/float_to_string_cross_target_test.rs） |
| `ALMIDE_FMOD_SWEEP_N=value` | harness | float `%` sweep が Float と Float32 として各レッグで Rust `%` と比較する xorshift64 ビットパターン対の個数（既定 20000、tests/float_fmod_cross_target_test.rs） |
| `ALMIDE_FN_ESCAPE_OFF` | ablation | escape するかどうかにかかわらず、BorrowInsertion がすべての fn 型パラメータを `&dyn Fn` として借用するようにする（#2288）。所有権認証器の C5 感度テストが使うアブレーション |
| `ALMIDE_FORCE_METER_CODEGEN` | route | プログラム全体を v1 で render できる場合や領域を splice できる場合でも、計測付き（fan.bounded / fan.min / fan.timeout）ネイティブプログラムを、独自の決定的 meter を持つ標準 codegen 経由でビルドする。codegen meter を v1 render と照合する差分検査（#3673） |
| `ALMIDE_FORCE_METER_SPLICE` | route | プログラム全体を v1 で render できる場合でも、計測付き（fan.bounded / fan.min / fan.timeout）ネイティブプログラムを、v1 で render した領域をリンクする標準 codegen 経由でビルドする。両経路の消費量が等しいことを保つ差分検査（#3669） |
| `ALMIDE_FUEL_PROBE` | route | ネイティブ render に fuel 消費計上を挿入する（charge プローブ）。`almide run --time-report` が内部設定する。native 専用で、wasm レッグには消費計上 trace がない（#2752） |
| `ALMIDE_FUZZ_BASE=value` | harness | 差分 fuzz の固定 seed 範囲の最初の seed（既定 0） |
| `ALMIDE_FUZZ_HOST_ORACLE` | harness | 差分 fuzz を host-oracle モードで実行する（arm 選択は seed とモードごとに決定的） |
| `ALMIDE_FUZZ_ITERS=value` | harness | 差分 fuzz の固定範囲に含める seed の個数（既定 200） |
| `ALMIDE_GPU_REQUIRED` | harness | wgpu adapter が見つからないとき、SKIP を出力せず GPU レッグの parity テストを失敗させる（#4236。`gpu-parity` CI ジョブが lavapipe 上で設定する。crates/almide-gpu-run/tests/int_map_parity.rs） |
| `ALMIDE_HEAP_TRACE` | debug | インタープリタのヒープブロック割り当てと解放を出力する |
| `ALMIDE_HTTP_MAX_RESPONSE_BYTES=value` | runtime | コンパイル済みプログラムが読む、http クライアントのバッファ付きレスポンスのバイト上限（既定 1 GiB、0 は上限なし、#2825） |
| `ALMIDE_HTTP_TIMEOUT_SECS=value` | runtime | コンパイル済みプログラムが読む、http クライアントの接続・読み取りタイムアウト秒数（既定 30、0 はなし） |
| `ALMIDE_INLINE_OFF` | ablation | 構造的レッグの小さな scalar-fn inliner を OFF にする（#2980）。A/B 比較用にすべての呼び出しを残す |
| `ALMIDE_INSTALL=value` | tool | `almide install` がバイナリをインストールするディレクトリ（既定の `~/.local/bin` を上書き） |
| `ALMIDE_INTERP_SWEEP_THREADS=value` | harness | interp sweep のスレッド数。1 は逐次（#2381） |
| `ALMIDE_IR_FAULT=value` | route | 指定した最適化パス後に IR 違反を注入し、release バイナリでパスごとの verifier が失敗になる様子を観察できるようにする（release ビルドがこれを有効にするのは `--allow-switch ALMIDE_IR_FAULT` 指定時のみ、#4129） |
| `ALMIDE_KEEP_SCRATCH` | tool | `almide test` のビルド作業ディレクトリを削除せず残す |
| `ALMIDE_LSP_TRACE` | debug | 言語サーバーが扱う全 LSP リクエスト・レスポンスを出力する |
| `ALMIDE_MANIFEST_TREE_CHECK=value` | ci | parity manifest 生成器の古いツリーの拒否（#2405、scripts/lib/oracle-header.sh）。`strict`（既定）は、`<Cargo.toml version> (dev…)` でない ORACLE、HEAD 以外のコミットが刻印された ORACLE、または刻印がなくソースより古い ORACLE、upstream より遅れた worktree、未追跡の spec/ fixture を拒否する。`gate` は未追跡 fixture の検査だけを残す（scripts/check-parity-goldens.sh が CI 成果物を保証する）。`off` は意図的な上書き |
| `ALMIDE_MONO_DEBUG` | debug | 単相化の発見とインスタンス化の判断を出力する |
| `ALMIDE_MP_PROBE` | debug | mut-param 解析の判断を出力する（IR） |
| `ALMIDE_MUTATION_BASE=value` | ci | mutation ゲートが差分比較する基点 ref |
| `ALMIDE_MUTATION_BUDGET_SECS=value` | ci | mutation ゲートの時間予算（秒）。超過後は新たな mutant を開始せず、残りは延期として名前を示す（0 または未設定は予算なし） |
| `ALMIDE_MUTATION_SCOPE=value` | ci | mutation ゲートが実行する mutation 集合 |
| `ALMIDE_MUTATION_SHARD=value` | ci | mutation ゲートにおける、このジョブの shard インデックス |
| `ALMIDE_MUTATION_SHARDS=value` | ci | mutation ゲートを分割する shard 数 |
| `ALMIDE_NAMES_DEBUG` | debug | ネイティブ名前検証 map と、そのスコープ付き shadowing 判断を出力する |
| `ALMIDE_NO_AVAIL_CHECK` | gate | E081 stdlib availability 検査を迂回する（availability プローブがビルド時に使う測定用の抜け道） |
| `ALMIDE_NO_RTLIB` | route | ビルド済みランタイム crate をリンクせず、ネイティブランタイムを inline でビルドする（自己完結した cargo 経路。`almide test` がハーネスビルドに設定する） |
| `ALMIDE_NO_VERIFIED_OK` | gate | フラグを拒否する代わりに、廃止した `--no-verified` レッグ（v0 fallback）を再有効化する |
| `ALMIDE_OMEGA=value` | route | 決定的な実時間 deadline リプレイ用に焼き込む ω 序数。成果物は時計を読まず n 番目の実時間検査で打ち切る（`-1` または未設定は live） |
| `ALMIDE_OMEGA_RECORD` | route | deadline が発火した領域を出るたび、native 成果物が `__ALMD_OMEGA <ord>` を出力するようにする（native で記録し、どこでも再生する） |
| `ALMIDE_ONLY_PASS=value` | ablation | 指定した任意パスだけ（および常時有効の前提パス）を使って最適化器を実行する。未知の名前はハードエラー |
| `ALMIDE_ORACLE_KEEP` | harness | ネイティブ borrow-mode オラクルが生成するプログラム（tests/native_borrow_oracle_test.rs）を、実行後に削除せず残す |
| `ALMIDE_ORG_DIR=value` | ci | リポジトリ横断検証スクリプトが走査する org リポジトリの checkout ディレクトリ |
| `ALMIDE_P3_HTTP_STOP=value` | ablation | ハング箇所を絞り込むため、リクエスト構築の stage N（1..=5）の直後に p3 http shim が固定エラーを返すようにする |
| `ALMIDE_PASS_EDGES=value` | harness | shuffle が従う追加の `A<B` パス順序 edge（カンマ区切り）。shuffle の不一致を防ぐため宣言が必要なペアを特定する二分探索用計測器 |
| `ALMIDE_PROBE_DUMP=value` | harness | heap プローブが出力 wasm を書くパス |
| `ALMIDE_PROBE_IR=value` | harness | heap プローブが lower 済み IR を書くパス |
| `ALMIDE_PROBE_OUT=value` | harness | 消費計上プローブ付きネイティブプログラムが `__ALMD_PROBE` meter 行を書き込むファイル。`almide run --time-report` は新しい一時パスを設定し、プログラム自身の stderr を測定値として読まないようにする（#3489）。未設定なら行は stderr に出る |
| `ALMIDE_PROBE_SRC=value` | harness | heap プローブがコンパイルするソースファイル（未設定ならプローブを省略） |
| `ALMIDE_PROFILE` | debug | ネイティブパイプラインのパスごと・phase ごとの時間を出力する |
| `ALMIDE_REAL_RUSTC=value` | harness | 偽 rustc が `--version` 以外について実行する、本物の `rustc`（tests/rustc_floor_test.rs） |
| `ALMIDE_REGION_DEBUG` | debug | region-window パスの判断を出力する（native と構造的レッグ） |
| `ALMIDE_REGION_OFF` | ablation | region-window 割り当てパスを OFF にする（native と構造的レッグ） |
| `ALMIDE_REGION_TRAP_STALE` | trap | ネイティブ region prelude の古い参照を検出する trap を有効化する（#2200） |
| `ALMIDE_REPO=value` | ci | リリーススクリプトが対象とするリポジトリ slug |
| `ALMIDE_RUN_PROJECT_DIR=value` | tool | `almide run` / `almide build` が `<temp>/almide-run` の代わりにネイティブバイナリをコンパイルする作業ディレクトリ（内容キーのバイナリキャッシュ、その cargo target、その rustc incremental session）。`almide clean` が空にする |
| `ALMIDE_SCOPED_CLOSURE_OFF` | ablation | BorrowInsertion が、すべての `let` 束縛クロージャを非 escape スコープではなくクロージャ値として扱うようにする（#3455）。スコープ規則導入前と同様、キャプチャは再び所有される |
| `ALMIDE_SEMLAW_CASES=value` | harness | semantic-laws property test が生成するケース数 |
| `ALMIDE_SHUFFLE_PASSES=value` | gate | 宣言された依存 edge が許す seed 付きランダム順序で native パスを実行する。パス依存関係のプローブであり、出力 Rust は変わってはならない（#2186） |
| `ALMIDE_SIZE_ALONE=value` | harness | size ratchet の子プロセスが分離検査のため単独測定する 1 つの fixture（#2309）。ratchet が起動するプロセスに設定する |
| `ALMIDE_SIZE_PRESSURE=value` | harness | size-ratchet の子が決定性検査のため 1 つの fixture をビルドするときの割り当て圧力（`plain` / `quarantine` / `ballast`、#3143）。ratchet が起動するプロセスに設定する |
| `ALMIDE_SKIP_PASS=value` | ablation | 指定した任意パスを省略する（カンマ区切り）。パス依存関係のプローブであり、出力は変わってはならない |
| `ALMIDE_SKIP_VERSION_CHECK` | gate | プロジェクトの `almide` バージョン要件検査を省略する |
| `ALMIDE_STDLIB_COVERAGE_PROBE=value` | harness | 単一 fixture のプローブ用に、tests/stdlib_test_coverage_test.rs が測定する 1 つのプログラム（リポジトリ相対パス） |
| `ALMIDE_STDLIB_COVERAGE_REPORT=value` | harness | tests/stdlib_test_coverage_test.rs が関数ごとの coverage 表を書き込む場所（scripts/check-stdlib-test-coverage.sh） |
| `ALMIDE_STREAM_FUSION_OFF` | ablation | stream-fusion パスを OFF にする |
| `ALMIDE_SURVIVE_OVERLAY=value` | harness | 廃止済み（#4129）。もう読まれない。`almide survive` は overlay を子実行に `--survive-overlay` / `--survive-overlay-text` 引数として渡すため、環境によってコンパイラがディスクファイルとは別のテキストを読まされることはない。回帰テストが設定して何も変わらないことを確認できるよう、登録は残している |
| `ALMIDE_SURVIVE_OVERLAY_TEXT=value` | harness | `ALMIDE_SURVIVE_OVERLAY` とともに廃止済み（#4129）。もう読まれない |
| `ALMIDE_TCO_DEBUG` | debug | ネイティブ末尾呼び出しループ書き換えの判断を出力する |
| `ALMIDE_TEST_ABORT_PROBE` | runtime | ネイティブ testing.assert_aborts の子プロセス（ADR-0029）を識別し、そこで `panic` が C-219 abort 経路を使うようにする。親が設定し、手動では決して設定しない |
| `ALMIDE_TEST_LAX_WASM` | gate | 既定の `almide test` レーン（wasm 優先、native fallback）で、wasm レッグが native 再実行は合格するのに trap した不一致ファイルを合格させる。この設定がなければ不一致レッグは実行を失敗させる |
| `ALMIDE_TEST_LEDGER=value` | runtime | ネイティブ `almide test` バイナリが完了したテスト名を追記する完了台帳ファイル（#4255）。runner が設定し、手動では決して設定しない |
| `ALMIDE_TEST_SIGNALLED_CALL_URL=value` | harness | almide-rt-core http テストの自己再実行を signalled-call 子プロセスに変える loopback URL（#3857）。その URL への call が待機する間に、存在しないコマンドを spawn する。親テストが設定し、手動では決して設定しない |
| `ALMIDE_TEST_VERBOSE` | tool | `almide test` ハーネスビルドの cargo / rustc 出力をすべて表示する |
| `ALMIDE_TEST_WASM_RUNNER=value` | tool | `wasmtime` は、すべての `almide` バイナリに含まれる組み込みホストの代わりに、`almide test` の wasm レッグを `wasmtime` CLI で実行する（標準 `to_wasi` 成果物、#3046）。比較用経路であり、未設定なら組み込みホスト |
| `ALMIDE_TIME_PHASES` | debug | 各 `almide run` phase の実時間を出力する |
| `ALMIDE_TMPDBG` | debug | v1 lowering の一時値 drop 判断を出力する |
| `ALMIDE_TOPLET_DEBUG` | debug | checker がモジュールをまたいで行う top-let 型の書き込み・読み取りを出力する |
| `ALMIDE_TRACE_PASSES` | debug | 各最適化パスの実行前に名前を出力し、戻らないパスを特定できるようにする |
| `ALMIDE_UPDATE_ALLOC` | harness | 構造的レッグの割り当てベースラインを再生成する |
| `ALMIDE_UPDATE_DUMPS` | harness | 構造的レッグの section-dump golden を再生成する |
| `ALMIDE_UPDATE_GAUNTLET` | harness | gauntlet manifest を再生成する |
| `ALMIDE_UPDATE_INSPECTION` | harness | stdlib inspection 台帳に不足行を加え、古くなった行をリセットする |
| `ALMIDE_UPDATE_INTERP_LEDGER` | harness | インタープリタの abstain 台帳と bridge-fallback 台帳を再生成する |
| `ALMIDE_UPDATE_NAME_SITES` | harness | 名前キーの property 台帳から古い行を除き、新しい箇所を追記する（各箇所には引き続き `why` が必要） |
| `ALMIDE_UPDATE_NATIVE_OWN` | harness | ネイティブ result 所有権台帳を再生成する |
| `ALMIDE_UPDATE_NATIVE_VERIFIED_FLOOR` | harness | 検証付きネイティブ renderer がカバーする spec/wasm_cross fixture の proofs/native-verified-floor.txt を再生成する（#4006、tests/native_verified_floor_test.rs） |
| `ALMIDE_UPDATE_SIZES` | harness | 構造的レッグのサイズベースラインを再生成する |
| `ALMIDE_UPDATE_SIZE_LADDER` | harness | stdlib リンクの size ladder 台帳を再生成する（#2141） |
| `ALMIDE_UPDATE_SNAPSHOTS` | tool | `almide test --update-snapshots` と同じ |
| `ALMIDE_UPDATE_STDLIB_COVERAGE` | harness | 実測実行から stdlib テスト coverage 台帳の行を書き換える |
| `ALMIDE_UPDATE_SURFACE` | harness | 実行済み surface の golden を再生成する |
| `ALMIDE_UPDATE_WITNESS_FLOOR` | harness | 証明書 witness floor を再生成する |
| `ALMIDE_VERBOSE` | debug | `almide -v` と同じ。通常の静かな実行では隠れる、native の拒否・fallback 注記を表示する |
| `ALMIDE_VERIFIED_DEBUG` | debug | render した wasm レッグと、もう一方が辞退した理由を示す（route オラクル） |
| `ALMIDE_VERSION_LINE=value` | ci | 実行時に環境からは読まない。`build.rs` が `cargo:rustc-env` として出力し、`src/main.rs` がコンパイル時に `env!` で読む。ALMIDE_BUILD_PROVENANCE と ALMIDE_BUILD_SHA から組み立てる、`almide --version` の表示文字列 — `<version> (<kind>[, <sha>])`（#2384） |
| `ALMIDE_WALL_REASON` | debug | `almide test` が fallback ファイルについて、単に `v1 wall` ではなく wasm レッグのどの stage が辞退したかを示すようにする |
| `ALMIDE_WASM_ALLOC_COUNT` | instrument | 構造的 wasm レッグの割り当てカウンタを出力する（#2407。5 つの i64 global、`$alloc`/`$free`/region restore bump。`__alloc_count` / `__alloc_reused` / `__alloc_bytes` / `__free_count` / `__region_reclaimed` として export する。`main` は戻る前に自身の top-let global も解放する）。また `almide run --target wasm` が実行後 stderr に `__ALMD_WASM_ALLOC allocs=N reused=N bytes=N frees=N reclaimed=N live=N heap_end=N` を出力するようにする。`live` は一度も解放されなかったヒープブロック数。OFF ならモジュールはこのスイッチなしのビルドとバイト単位で同一（`ALMIDE_ALLOC_COUNT` の wasm 版。台帳は crates/almide-wasm/tests/golden/alloc-count-baseline.txt と live-at-exit-baseline.txt） |
| `ALMIDE_WASM_CANONICAL_ALLOC` | instrument | 構造的 wasm レッグの `$alloc` を free-list 経路なしで出力し、すべての要求を bump にする（stdlib inspection meter の固定アロケータ、owner の 2026-10-15 判断）。これにより実測 bytes-moved 系列はヒープにある解放済みブロックではなくプログラムの要求だけに依存し、出荷モジュールのコピー量の上限となる。OFF ならモジュールはこのスイッチなしのビルドとバイト単位で同一（crates/almide-wasm/src/canonical_alloc.rs） |
| `ALMIDE_WASM_CHECKED_ALLOC` | instrument | 構造的 wasm レッグの `$free` が解放済み size-class ブロックを 0xA5 で poison し、`$alloc` が再利用前にその poison を検査するように出力する。解放中に書き込みが起きていれば trap する（#4009。wasm レッグの use-after-free と迷い込み書き込み検出器。scripts/wasm-checked-alloc.sh がこれを使いコーパスを実行する）。OFF ならモジュールはこのスイッチなしのビルドとバイト単位で同一（crates/almide-wasm/src/checked_alloc.rs） |
| `ALMIDE_WASM_FREES` | ci | frees-churn ゲートのスイッチ。コンパイラ側の読み手は v0 emitter とともに廃止された（#782）。今も設定するゲートは #2207 のもの |
| `ALMIDE_WASM_SKIP_STOCK_AUDIT` | route | `almide build --target wasm` の標準 WASI host-op 監査を省略する（parity、diff-fuzz、availability ゲートがビルド時に使う emitter-frontier プローブ）。#2934 までは ALMIDE_WASM_STRUCTURAL という名前だった |
| `ALMIDE_WASM_WATCHDOG_SECS=value` | harness | プロセス内テストランナーの epoch watchdog（almide-wasm-run の `run_wasm` / `run_wasm_with` / `run_wasm_capped`）が、`interrupt` で trap するまでモジュールに許す実時間の秒数（既定 30）。native に watchdog がないのと同様、`almide run --target wasm` と `almide bench --target wasm` はどの設定でも watchdog を有効化しない（#2615） |
| `ALMIDE_WITNESS_DUMP` | harness | witness-floor テストで各 fixture の証明書 witness を出力する |
| `ALMIDE_WRITE_FUZZ_CORPUS` | harness | 生成した fuzz プログラムをディスクに書く |

`ALMIDE_*` 以外:

| 変数 | 説明 |
|---|---|
| `CI` | `true` で `almide test --ci` と同じ(スナップショットを書かない) |
