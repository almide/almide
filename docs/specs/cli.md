# CLI Specification

> Last updated: 2026-09-23

## Overview

```
almide <command> [options] [arguments]
```

プロジェクトルートに `almide.toml` + `src/main.almd` があれば、ファイル引数は省略可能。自動的に `src/main.almd` が使われる。

---

## Commands

### `almide run`

コンパイルして即実行。内部で Rust ソースを生成 → `cargo build` → バイナリ実行。

```bash
almide run                              # src/main.almd を実行
almide run app.almd                     # 指定ファイルを実行 (native)
almide run app.almd --target wasm       # wasm をビルドして wasmtime で実行
almide run -- --flag value              # -- 以降はプログラムの引数
almide run app.almd -- arg1 arg2        # ファイル指定 + プログラム引数
```

| オプション | 説明 |
|---|---|
| `--target rust\|wasm` | 実行ターゲット。`rust`（既定、ネイティブバイナリ）または `wasm`（v1 trust-spine が直接 WASM を生成し `wasmtime` CLI で実行。rustc 不要）。両ターゲットは同一の観測可能挙動（stdout/stderr/exit）を出す（クロスターゲット等価性保証）。`wasm` には PATH に `wasmtime` が必要 |
| `--no-check` | 型チェックをスキップ |
| `--release` | 最適化ビルド (cargo --release) |

`almide` 自身のフラグ（`--target` / `--no-check` / `--release`）は `--` の前で解釈され、`--` 以降はそのままプログラムに渡る（`cargo run` と同じ規約）。プログラム内で `env.args()` を呼ぶと `--` 以降の引数が `List[String]` で返る。

テスト: `tests/run_target_flag_test.rs`

**native のビルドキャッシュ**(#2500): native ターゲットは生成 Rust を共有スクラッチ dir
（`ALMIDE_RUN_PROJECT_DIR`、既定 `<temp>/almide-run`）でビルドし、生成コードの内容ハッシュを名前にした
`target/<profile>/almide-<hash>` にバイナリを置く。同じ生成コードは `almide run` / `almide build`
のどちらから来てもこの 1 本を再利用し、cargo を呼ばない。dir 内の書き込み（ビルド・退避・掃除）は
すべて `.almide-build.lock` の下で直列化され、キャッシュヒットだけがロック無しで走る。

- **退避**: ヒットのたびにバイナリの mtime を更新し、7 日間使われなかった `almide-<hash>`
  （と `deps/` に残る同じ世代の `almide_out-*` オブジェクト）を、次のミス時にロックの下で削除する。
  この掃引は 1 日 1 回（`.almide-evict-stamp`）。incremental セッションには触れない。
- **rustc ICE からの復旧**: 中断されたビルド（ENOSPC、kill）が rustc の incremental セッションを
  壊すと、以後その形のビルドは `the compiler unexpectedly panicked` で毎回落ちる。失敗した
  ビルドの出力にこの banner があれば、同じロックの下で `target/*/incremental` を消して 1 回だけ
  再ビルドし、成功したら stderr に
  `note: rustc crashed on a stale incremental session; cleared … and rebuilt successfully` を
  1 行出す。再ビルドも失敗したら元のエラーをそのまま報告する（ループしない）。
- **`almide clean`** がこの dir を空にする（下記）。

テスト: `tests/run_cache_recovery_test.rs`

**stdout のバッファリング**(#2245): native バイナリの `println` / `io.print` / `io.write` /
`io.write_bytes` は 1 つの 64 KiB バッファを通る(順序はプログラム順)。stdout が端末なら書き込み
ごとに flush、パイプやファイルならブロック単位。flush 点は終了時・panic 時・`main` が err を返した
時・子プロセス起動前・stdin 読み取り前、そして常に flush する `io.print`(`io.print("")` が明示
flush)。stderr(`eprintln`)は無バッファなので、端末以外では stdout との相対順序は保たれない。
詳細は [docs/stdlib/io.md](../stdlib/io.md)。

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

### `almide build`

コンパイルしてバイナリを生成。

```bash
almide build                            # src/main.almd → パッケージ名のバイナリ
almide build app.almd -o myapp          # 出力ファイル名指定
almide build app.almd --target wasm     # WASM バイナリ（直接 emit、rustc 不要）
almide build app.almd --target wasm --host js -o dist/app.wasm  # + dist/app.js, dist/app.d.ts (JS ホスト、#2265)
almide build --release                  # 最適化ビルド (opt-level=2)
almide build --fast                     # 最大性能 (opt-level=3, LTO, native CPU)
```

| オプション | 説明 |
|---|---|
| `-o <name>` | 出力ファイル名 |
| `--target wasm` | WASM バイナリを生成（直接 emit） |
| `--host js` | `--target wasm` 専用: モジュールの隣に JS ホスト `<mod>.js`（依存なしの ES module）と `<mod>.d.ts` を書く（#2265）。`init(source?, hooks?)` がインスタンス化、`run()` が `main`、`pub fn` ごとに 1 つのラッパ。`@extern(wasm, "js", "name")` は `init({ js: { name } })` で結線。マーシャルは Int（`number`、±2^53 の範囲検査）/ Float / Bool / String / Unit — それ以外の型を境界に持つ `pub fn` はビルド時に型名を挙げて拒否。出荷物はプログラムが使う分だけ（#2276）: `__alloc`/`__release` の export と glue の String ヘルパは境界に `String` がある時だけ、WASI shim は出荷モジュール（`--wasm-opt` 後）が import する名前だけ。ゲート: `scripts/check-js-host.sh`（`spec/wasm_host_js/` を node で実行し、期待出力と native 出力に一致させ、モジュールのバイト同一性・shim 集合・`glue-ceiling.txt` の上限を検査）。仕様: docs/wasm/WASM-OUTPUT.md「JS host」節 |
| `--release` | 最適化ビルド |
| `--fast` | 最大性能（`--release` を含む + LTO + native CPU） |
| `--unchecked-index` | 配列の境界チェックを無効化（unsafe） |
| `--no-check` | 型チェックをスキップ |
| `--repr-c` | struct/enum に `#[repr(C)]` を付与（C ABI 互換） |

出力ファイル名のデフォルト:
- `almide.toml` があれば `[package] name`
- なければソースファイル名から `.almd` を除いた名前

---

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
| `--json` | JSON 形式で結果出力 |
| `--target wasm` | wasmtime で実行 |
| `--update-snapshots` | `testing.assert_snapshot` の不一致を受理し、呼び出し側の期待値リテラルをソース内で書き換える(`ALMIDE_UPDATE_SNAPSHOTS=1` でも同じ) |
| `--ci` | CI モード: スナップショットを一切書かない(`CI=true` でも同じ)。新規・乖離はどちらも失敗 |
| `--allow-no-tests` | 実行すべきテストが 0 件でも 0 で終了する(既定は 5) |
| `--show-output` | 通ったテストも含め、全テストが stdout / stderr に書いたものを表示する |

実行のたびに **実行テスト数**を報告する: `2 tests in 1 file`。`--run` が何かを除外した
ときは `0 tests in 1 file (2 filtered out)` のように除外数も付く。`--json` は各行に
`"tests"` と `"filtered_out"` を持つ。

**ゼロの扱いは二種類ある**(#2084):

| 状況 | 終了コード | 理由 |
|---|---|---|
| `--run` が 1 件も一致しなかった | 0 | 絞り込んだのは呼び出し側。件数表示で自明になる |
| 実行すべきテストが 1 件も無かった | **5** | 事故。`--allow-no-tests` で opt-out |

「テストが無かった」は**テストファイルが見つからなかった場合と、名指ししたファイルに
`test` ブロックが無かった場合の両方**に適用される。5 は 1(テスト失敗)と区別するための
専用コードで、呼び出し側が出力を読まずに判別できる。

この表は**ターゲットを問わない**: 同じファイル・同じ引数なら `--target wasm` も同じ終了
コードと同じ件数行(`0 tests in 1 file`)を返す。`main` も `test` ブロックも無いファイルは
レンダラの壁(WALL、そのレグが辞退した)ではなく「走らせるものが無かった」であり、両レグで
5 になる(#2204)。

テスト: `tests/test_zero_outcomes_test.rs`、両ターゲット一致は `tests/test_zero_exit_parity_test.rs`

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

`--run <pattern>` は **生成された関数名に対する大文字小文字を区別する部分文字列一致**で、
`test "…"` のラベルそのものではない。ラベルは `__test_almd_` を前置し、空白・記号を `_` に
畳んだ綴りになる（`crates/almide-base/src/names.rs`）。したがって `test "beta fails"` は
`--run beta` と `--run beta_fails` では選ばれるが、`--run "beta fails"`（空白のまま）では
選ばれない。`--run __test_almd_` は全件を選ぶ。

この規則は **native レグと wasm レグで同一**である（#2085）。native はパターンを Rust の
テストハーネスに渡し、wasm はランナー合成の時点で同じ述語で絞る。どちらのレグでも、
1件も一致しないパターンは 0 件を実行して成功終了する。

テスト: `tests/wasm_test_filter_parity_test.rs`

スクラッチ成果物（wasm レグが wasmtime に渡す `.wasm` モジュール）は実行ごとに固有の
`$TMPDIR/almide-test-<pid>-<nonce>/` 配下に、ファイルの**絶対パス**のハッシュで命名して
置かれ、終了時に削除される（`ALMIDE_KEEP_SCRATCH=1` で残し、場所を stderr に出す）。
同名ファイルの並列実行や別ディレクトリの同名ファイルがパスを共有することはない（#1877）。
ネイティブ fallback のビルドキャッシュは `$TMPDIR/almide-test/native/` に永続（同じく絶対パス鍵）。

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

同じ規則が**ランタイム rlib キャッシュ** `<temp>/almide-rtlib-<key>/` にも効く(#2504)。
この dir はランタイムソース × rustc バージョン × opt レベルごとに 1 つ作られ、コンパイラを
ビルドし直すたび・ツールチェーンを上げるたびに新しい鍵になって古い方は二度とリンクされない
（2026-09-22 に 31 dir / 102 MB を実測）。リンクのたびに rlib の mtime を更新し、7 日リンク
されなかった dir を（自分が今使っている dir を除いて）空にする。掃引は 1 日 1 回、
stamp は `<temp>/.almide-evict-stamp`。

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
```

| オプション | 説明 |
|---|---|
| `--deny-warnings` | 警告をエラー扱い |
| `--json` | 診断を JSON で出力（1 行 1 診断、エディタ/エージェント統合用） |
| `--explain <code>` | エラーコード (E001〜E030, E420) の説明 |
| `--effects` | 各関数のエフェクト/ケイパビリティ分析 |
| `--timings` | lex / parse / check の phase 別 wall time（#1311） |

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
`{level, code, message, hint, here, try, try_replace, applicability, suggestions, context, file, line, col, end_col, secondary}`。
`try` は貼り付け可能な修正スニペット、`try_replace` はそれが置換するスパン、
`suggestions[]` は `{line, col, end_col, replacement, applicability}` の構造化された同じ修正。

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

E001〜E030 + E420 の全コード解説は [../diagnostics/](../diagnostics/) を参照（上表は代表例）。

---

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

### `almide mcp`

Model Context Protocol サーバを stdio で起動する。エージェント（Claude Code 等）が
コンパイラを **型付きツール呼び出し** として使うための口。人間向け出力を
モデルに読み解かせる工程を挟まないことが目的で、この工程こそが精度の漏れ口。

```bash
almide mcp                              # stdio で JSON-RPC 2.0（改行区切り）を待つ
```

対応メソッド: `initialize` / `tools/list` / `tools/call` / `ping`。
それ以外は JSON-RPC `-32601`（`capabilities` は `tools` のみを宣言する）。

ツール（5 つ。すべて CLI の既存の機械可読出力を経由する）:

| ツール | 実体 | 返すもの |
|---|---|---|
| `almide_check` | `almide check --json` | 構造化診断の配列（`try`/`try_replace` 込み） |
| `almide_test` | `almide test --json` | ファイル単位の pass/fail + ランナー生出力 |
| `almide_api` | `almide ide outline --json` / `stdlib-snapshot --json` | 公開宣言のシグネチャ一覧 |
| `almide_explain` | `almide explain <CODE>` | 診断コードの解説（markdown） |
| `almide_fmt_check` | `almide fmt --check --json` | 未整形ファイル一覧（書き込みなし） |

設計上の制約:

- **コンパイラへの入口は 1 本**。各ツールは `current_exe()`（= 自分自身）を
  サブプロセスとして起動し、CLI が既に出している JSON を読む。MCP 専用の
  出力経路を作らない（作れば必ず CLI とドリフトし、しかも誰も目で読まないので
  ドリフトが見えない）
- **人間向けテキストは決してパースしない**。機械可読な形が無い箇所
  （テストの個別失敗詳細 = #1313）は `*_unstructured` という名前のフィールドに
  そのまま入れて返す
- **書き込みツールは無い**。`fmt` は `--check` 形のみ。適用は CLI（`almide fmt` /
  `almide fix`）で行う — エージェント自身のトランスクリプトに編集が残る

Claude Code プラグイン定義（MCP + LSP）: `tools/claude-plugin/`、
マーケットプレイス: `.claude-plugin/marketplace.json`、導入手順: [../mcp.md](../mcp.md)

テスト: `tests/mcp_test.rs`

---

### `almide init`

新しいプロジェクトを作成。

```bash
almide init
```

生成���:

```
almide.toml               [package] name, version, edition
src/
  main.almd               effect fn main テンプレート
tests/                    テスト用ディレクトリ
CLAUDE.md                 AI 向けプロジェクト説明
```

`name` はカレントディレクトリ名から自動生成。

---

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

出力は `name <old12> -> <new12>`(初回ロックは `name -> <new12>`)。

### `almide add`

依存パッケージを追加。`almide.toml` の `[dependencies]` に書き込み、即フェッチ。

```bash
almide add bindgen                      # github.com/almide/bindgen
almide add almide/almide-bindgen        # github.com/almide/almide-bindgen
almide add user/repo@v0.1.0             # バージョン指定
almide add --git https://example.com/repo.git --tag v1.0 mylib
```

短縮記法:
- `almide add name` → `https://github.com/almide/{name}`
- `almide add user/repo` → `https://github.com/{user}/{repo}`
- `@v0.1.0` → `tag = "v0.1.0"`

---

### `almide deps`

依存パッケージの一覧を表示。

```bash
almide deps
# bindgen = https://github.com/almide/almide-bindgen (v0.1.0)
# json = https://github.com/almide/json (main)
```

---

### `almide dep-path`

依存パッケージのローカルキャッシュディレクトリを出力。

```bash
almide dep-path bindgen
# /Users/you/.almide/cache/bindgen/.src-2080cb5159116353/a629eded8d20/src
```

用途: 依存パッケージの `.almd` ファイルを `process.exec("almide", ["run", path])` で実行する場合のパス取得。

---

### `almide clean`

キャッシュをクリアする。対象は 6 つ:

| 対象 | 場所 |
|---|---|
| 依存キャッシュ | `~/.almide/cache/` |
| インクリメンタルキャッシュ | `./.almide/cache/` |
| コンパイルキャッシュ | `./target/compile/` |
| native ビルドスクラッチ(#2500) | `ALMIDE_RUN_PROJECT_DIR`（既定 `<temp>/almide-run`）と `<temp>/almide-build-cdylib` |
| `almide test` の worker dir(#2504) | `<temp>/almide-test/native/<key>/` を 1 つずつ |
| ランタイム rlib(#2504) | `<temp>/almide-rtlib-<key>/` を 1 つずつ |

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

### `almide verify`

プログラムの flight-grade 証明書（ownership / names / caps / call-modes の witness）を、**独立版数の別バイナリ `almide-verify`** に再検査させる (#2152)。`almide verify` 自身は検査器を持たない subprocess shim で、`almide-verify` を **almide 実行ファイルの隣 → PATH** の順に探して起動し、標準入出力と終了コードをそのまま返す。見つからなければ **linked fallback は無く**、名前付きエラー `error[verifier-missing]` で終了コード 127。

```bash
almide verify app.almd                    # 証明書 bundle を生成し almide-verify bundle に渡す
almide verify app.almd --emit app.bundle  # bundle をファイルに残す（almide-verify が無くても書く）
almide verify ownership w.cert            # .almd 以外の引数は almide-verify にそのまま渡る
almide-verify --version                   # 検査器自身の版数（コンパイラとは独立）
```

- `almide` 側で走るのは **untrusted な producer** だけ（`almide_mir::pipeline::program_witnesses` がプログラムを MIR に下ろし、`crate::certificate` の witness を bundle に書く）。判定は常に `almide-verify`。
- `almide-verify` はコンパイラの crate を一切リンクしない（`crates/almide-verify`、依存ゼロ）。各性質の判定は `proofs/` の Coq 検査器（`check_xc` / `check_names_cert` / `check_caps_cert` / `check_prog_cert` / `check_modes_cert`）の転写で、定理は持たない。抽出版検査器との一致は `proofs/gate.sh`（全行 + seeded ランダム差分）と `proofs/corpus-wall.sh`（コーパス全 witness）がゲートする。
- lowering subset の外の関数は bundle に `uncertified` として名前つきで載る（黙って飛ばさない）。
- `./almide.toml` の `[permissions].allow` があれば effect fn の宣言 capability をそれに絞る（caps witness が reject できるようになる）。

`almide-verify` の終了コード: 0 = 全 witness ACCEPT（CERTIFIED）、1 = REJECT あり、または witness が 0 件、3 = 全 ACCEPT だが uncertified な関数あり（INCOMPLETE）、2 = 使い方の誤り・読めない入力・不正な bundle。

テスト: `tests/verify_shim_test.rs`（shim の委譲・不在時の名前付きエラー・終了コード転送）、`crates/almide-verify/tests/coq_examples.rs`（Coq の全 `Example` と `build-checker.sh` の固定行）、`crates/almide-verify/tests/cli.rs`

---

## Legacy Mode

ファイル名が `.almd` で終わる引数を最初に指定すると、`emit` コマンドとして扱われる:

```bash
almide app.almd --target rust           # → almide emit app.almd --target rust
almide app.almd --target rust --repr-c  # #[repr(C)] 付き Rust 出力
almide app.almd --emit-ast              # AST を JSON で出力
almide app.almd --emit-ir               # 型付き IR を JSON で出力
```

---

## Exit Codes

| コード | 意味 |
|---|---|
| 0 | 成功 |
| 1 | コンパイルエラー、テスト失敗、依存解決失敗 |
| 127 | `almide verify`: 独立検査器 `almide-verify` が見つからない（`error[verifier-missing]`） |

---

## Global Options

| オプション | 説明 |
|---|---|
| `-h, --help` | ヘルプ表示（各コマンドにも付く） |
| `-V, --version` | バージョン表示（例: `almide 0.34.2`） |

---

## 環境変数

`ALMIDE_*` の環境変数は `almide_base::env::SWITCHES`（`crates/almide-base/src/env.rs`）が
唯一の台帳で、`almide switches` で列挙できる（#2205）。下表は `almide switches --md` の出力
そのもので、`scripts/check-env-switches.sh`（CI `checks`）が台帳に無い名前・コンパイラ側の直接の
`std::env::var("ALMIDE_…")` 読みを、`tools/almide-gates env-switches-doc` がこの表と台帳の差分を拒否する。

- 真偽値の意味は一つ: 未設定・空・`0`・`false`・`off`・`no` が OFF、それ以外は ON。
- 種別 `route`（実行するレッグを強制する）と `gate`（既定の検査を迂回する）のスイッチが ON のときは、
  そのプロセスで最初に読まれた時点で stderr に 1 行（`[almide] ALMIDE_X is set: …`）が出る。
  強制されたルートで出た verdict は、そう名乗る。
- `harness` はテストハーネス（`tests/`, `crates/*/tests`, `tools/`）が読むフック、`ci` はワークフローと
  `scripts/` だけが使うもの、`runtime` はコンパイルされたプログラム自身が実行時に読むもの。

<!-- almide switches --md: begin -->
| 変数 | 種別 | 説明 |
|---|---|---|
| `ALMIDE_ABI_PROBE` | debug | print the lifted-effect-fn ABI decision per function (v1 lowering) |
| `ALMIDE_ALLOC_COUNT` | harness | build the native program with a counting allocator that prints `__ALMD_ALLOC allocs=N deallocs=N reallocs=N peak=N` on stderr when `__almide_main` returns (the native borrow oracle's allocation lane, tests/native_borrow_oracle_test.rs) |
| `ALMIDE_BANG_RETURN` | ablation | turn OFF the per-position `!` desugars of the v1 lowering so every `!` reaches the bind-position rule or walls loudly (the decline-matrix probe) |
| `ALMIDE_BENCH_DIR=value` | harness | the fixture directory of the structural leg's perf probe (default `crates/almide-wasm/tests/perf`) |
| `ALMIDE_BIN=value` | harness | path of the `almide` binary the test harnesses, scripts and workflows drive (default: `target/release/almide`, then PATH) |
| `ALMIDE_BORROW_OWN_ALL` | ablation | make BorrowInsertion own every borrow-eligible param, as before inference existed — the ablation the ownership certifier's C4 sensitivity test drives, and the borrow-inference perf knob |
| `ALMIDE_BOUNDED_DEBUG` | debug | print why a bounded-loop bind declined (v1 lowering) |
| `ALMIDE_BUILD_PROVENANCE=value` | ci | read by `build.rs` at BUILD time: `release` makes `almide --version` say `(release)`, anything else (including unset) says `(dev)`. Set only by `.github/workflows/release.yml`, the one thing that builds from a tag, so a binary claiming to be a release had to come from there (#2384) |
| `ALMIDE_BUILD_SHA=value` | ci | read by `build.rs` at BUILD time: the commit `almide --version` names beside the build kind, truncated to 9 characters. Passed in by `make install` and the release workflow rather than read from git in the build script, which would rebuild the root crate after every commit (#2384) |
| `ALMIDE_CAPTURE_MOVE_OFF` | ablation | make CaptureClone clone every capture again, as before #2231, instead of moving a value whose sole user is the closure — the ablation the ownership certifier's sensitivity test drives |
| `ALMIDE_CERTIFY_OWNERSHIP=value` | debug | run the native ownership certifier after the pass pipeline (#2231): `report` prints every violation, `fail` aborts the build on one, `off` skips it; unset = `fail` in a debug build, `off` in a release build |
| `ALMIDE_COMPILER_STACK=value` | tool | stack size in bytes of the compiler driver thread (default 256 MiB); a deep input that overflows it is the regression test's subject |
| `ALMIDE_COMPONENT_ADAPTER` | route | route `--component` through the preview1 adapter instead of the direct component emission |
| `ALMIDE_COMPONENT_P3` | route | emit a WASI 0.3 component (stdio over component-model streams, the async canonical ABI) under `--component`; needs a p3-capable wasmtime |
| `ALMIDE_CORPUS_FILTER=value` | harness | substring filter over the fixture paths the 3-way oracle test evaluates |
| `ALMIDE_CORPUS_SHARD=value` | harness | `k/N` (1-based) walks the k-th modulo slice of the SORTED spec/wasm_cross corpus in the six corpus giants (#2381), read after the sort; `merge/N` reads the N shards' partials from `ALMIDE_CORPUS_SHARD_DIR` and judges the whole-corpus ceilings and the name-keyed bridge ledger; unset = the unsharded gate |
| `ALMIDE_CORPUS_SHARD_DIR=value` | harness | directory where a sharded corpus gate writes its walked-fixture list and its partial counts / bridge names, and where `merge/N` reads them; unset = a local slice that writes nothing |
| `ALMIDE_CORPUS_WEIGHTS_DIR=value` | harness | directory where a corpus gate records the wall it measured per fixture (`weights/<column>.<gate>[.<k>-of-<N>].txt`, `stem<TAB>ms`) for scripts/gen-corpus-weights.sh to render into proofs/corpus-weights.txt, the table the balanced `k/N` slices read (#2457); CI points it at the shard-partials dir so the committed table is rendered from the runner's own ratios (#2502); unset = nothing recorded |
| `ALMIDE_COVERAGE_CONDITION=value` | ci | the coverage ratchet's condition tag (which baseline row a push is judged against) |
| `ALMIDE_CWD=value` | runtime | the writer's working directory, set by `almide run` for the wasm host so relative fs paths resolve as on native (C-137); never set by hand |
| `ALMIDE_DBG_ANF` | debug | print why a lambda lift or statement inline declined (v1 lowering) |
| `ALMIDE_DBG_BANG` | debug | print the `!` unwrap decisions of the v1 bind lowering |
| `ALMIDE_DBG_BORROW=value` | debug | dump every native borrow-inference signature key containing the value, per fixed-point iteration, and the keys each call site consults |
| `ALMIDE_DBG_CELLS` | debug | print the captured / mutated / celled variable sets (v1 lowering) |
| `ALMIDE_DBG_CONTLIFT` | debug | print why a poison-oracle continuation lift rolled back (v1 lowering) |
| `ALMIDE_DBG_DESUGAR_FN=value` | debug | print the fully desugared body of the fn named by the value (v1 lowering; was `DBG_DESUGAR_FN` before #2205) |
| `ALMIDE_DBG_DESUGAR_RAW` | debug | with `ALMIDE_DBG_DESUGAR_FN`, print the raw pre-desugar body too (was `DBG_DESUGAR_RAW`) |
| `ALMIDE_DBG_ELEM` | debug | print why a list-literal Block element declined (v1 lowering) |
| `ALMIDE_DBG_FAN` | debug | print the fan lowering's prefetch and pattern decisions (structural leg) |
| `ALMIDE_DBG_GINIT` | debug | print the eager top-let init runner's admission set and why an extended runner declined (v1 lowering, C-007) |
| `ALMIDE_DBG_LINK` | debug | dump the wasm link demand set and what each key resolves to |
| `ALMIDE_DBG_LOWER_FN=value` | debug | print the fully desugared body the v1 lowering actually lowers, for the fn named by the value (was `DBG_LOWER_FN`) |
| `ALMIDE_DBG_NEMATCH` | debug | print the never-err match analysis per function (v1 lowering) |
| `ALMIDE_DBG_NESTED_MATCH` | debug | print why a nested-match chain was refused (v1 lowering) |
| `ALMIDE_DBG_QQ` | debug | print which path lowered each `??` (match-first vs route fallback, v1 lowering) |
| `ALMIDE_DBG_ROUTER` | debug | print a stdlib call name refused for its registered signature, with the mismatch and the argument types (v1 lowering) |
| `ALMIDE_DBG_SWITCH` | debug | print the `br_table` switch rendering decisions (v1 wasm render) |
| `ALMIDE_DBG_TCO` | debug | print the tail-call-to-loop admission decisions (v1 lowering) |
| `ALMIDE_DBG_TRAP` | debug | print the wasm trap's backtrace and host state when the embedded host catches one |
| `ALMIDE_DBG_UNLINKED` | debug | print wasm references with no resolvable definition |
| `ALMIDE_DBG_WAT=value` | debug | print the rendered WAT of every function whose name contains the value (v1 wasm render) |
| `ALMIDE_DBG_WHILE` | debug | print the while-loop lowering decisions (v1 lowering) |
| `ALMIDE_DEBUG_CALL_OPS` | harness | print the call ops of every lowered fn (the classify_corpus example) |
| `ALMIDE_DEBUG_EFFECTS` | debug | print the effect inference pass's per-function results (native codegen) |
| `ALMIDE_DEBUG_MIR_OPS` | harness | print every MIR op with its index (the classify_corpus example — the certificate-bisection instrument) |
| `ALMIDE_DEFAULTS_DEBUG` | debug | print the record-default resolution when no default keys were found (native codegen) |
| `ALMIDE_DISABLE_OPT` | ablation | run the optimiser pipeline with every optional pass off (ablation; the always-on enabler passes still run) |
| `ALMIDE_DUMP_DROPS` | debug | print the computed drop set (v1 lowering) |
| `ALMIDE_DUMP_IR=value` | debug | dump the IR after the named passes (comma-separated, or `all`) on the native pipeline, and the post-chain body of every fn whose name contains the value on the v1 pipeline |
| `ALMIDE_DUMP_MIR` | debug | print every lowered fn's op stream before the native render runs |
| `ALMIDE_DUMP_VERIFY` | debug | print the native render's verification transcript |
| `ALMIDE_DUMP_WMIR=value` | debug | print the lowered wasm-leg op stream of every fn whose name contains the value |
| `ALMIDE_EXPECT_TOOLS` | harness | make a harness test FAIL instead of skipping when an external tool (wasmtime, wasm-tools) is missing; CI sets it |
| `ALMIDE_FALLBACK_NAMES` | tool | make `almide test` print one `FALLBACK <file>` line per file the wasm leg did not pass — the wasm coverage ratchet's data feed |
| `ALMIDE_FAN_SEQUENTIAL` | runtime | run `fan.*` sequentially in the native runtime (a determinism lever for measurement; the observable result is the same by contract) |
| `ALMIDE_FN_ESCAPE_OFF` | ablation | make BorrowInsertion borrow EVERY fn-typed param as `&dyn Fn`, escaping or not (#2288) — the ablation the ownership certifier's C5 sensitivity test drives |
| `ALMIDE_FUEL_PROBE` | route | insert fuel charges and force the INCUMBENT wasm leg (the charge probe); `almide run --time-report` sets it internally |
| `ALMIDE_FUZZ_BASE=value` | harness | the first seed of the differential fuzz's fixed seed range (default 0) |
| `ALMIDE_FUZZ_HOST_ORACLE` | harness | run the differential fuzz in host-oracle mode (arm selection is deterministic per seed and mode) |
| `ALMIDE_FUZZ_ITERS=value` | harness | how many seeds the differential fuzz's fixed range covers (default 200) |
| `ALMIDE_HEAP_TRACE` | debug | print the interpreter's heap-block allocations and frees |
| `ALMIDE_HTTP_TIMEOUT_SECS=value` | runtime | the http client's request timeout in seconds, read by the compiled program (default 30) |
| `ALMIDE_INSTALL=value` | tool | the directory `almide install` installs binaries into (overrides the default `~/.local/bin`) |
| `ALMIDE_INTERP_SWEEP_THREADS=value` | harness | interp sweep thread count; 1 = serial (#2381) |
| `ALMIDE_IR_FAULT=value` | harness | inject an IR violation after the named optimiser pass, so the per-pass verifier can be watched turning red in the release binary |
| `ALMIDE_KEEP_SCRATCH` | tool | keep the `almide test` scratch build directory instead of deleting it |
| `ALMIDE_LOCAL_REUSE_THRESHOLD=value` | route | the distinct-local count above which the v1 wasm render reuses locals (default 8000); a test knob that forces the transform on across the corpus |
| `ALMIDE_LSP_TRACE` | debug | print every LSP request and response the language server handles |
| `ALMIDE_MANIFEST_TREE_CHECK=value` | ci | the parity-manifest generators' stale-tree refusal (#2405, scripts/lib/oracle-header.sh): `strict` (default) refuses an ORACLE that is not `<Cargo.toml version> (dev…)`, is stamped with a commit other than HEAD, or is unstamped and older than the sources; a worktree behind its upstream; and an untracked spec/ fixture. `gate` keeps only the untracked-fixture check (scripts/check-parity-goldens.sh vouches for CI's artifact). `off` is the deliberate override |
| `ALMIDE_MG_DEBUG` | debug | print the mutable-global slot assignment and cross-module name-bridge decisions (v1 lowering) |
| `ALMIDE_MONO_DEBUG` | debug | print the monomorphisation discovery and instantiation decisions |
| `ALMIDE_MP_PROBE` | debug | print the mut-param analysis decisions (IR) |
| `ALMIDE_MUTATION_BASE=value` | ci | the base ref the mutation gate diffs against |
| `ALMIDE_MUTATION_SCOPE=value` | ci | which mutation set the mutation gate runs |
| `ALMIDE_MUTATION_SHARD=value` | ci | this job's shard index of the mutation gate |
| `ALMIDE_MUTATION_SHARDS=value` | ci | how many shards the mutation gate is split into |
| `ALMIDE_NAMES_DEBUG` | debug | print the native name-verification map and its scoped shadowing decisions |
| `ALMIDE_NO_AVAIL_CHECK` | gate | bypass the E081 stdlib availability check (the measurement escape the availability probe builds through) |
| `ALMIDE_NO_BR_TABLE` | route | render every switch as an if-chain instead of `br_table` (v1 wasm render) |
| `ALMIDE_NO_RTLIB` | route | build the native runtime inline instead of linking the prebuilt runtime crate (the self-contained cargo path; `almide test` sets it for the harness build) |
| `ALMIDE_NO_VERIFIED_OK` | gate | re-enable the retired `--no-verified` legs (the v0 fallback) instead of refusing the flag |
| `ALMIDE_OMEGA=value` | route | the baked ω ordinal for deterministic wall-deadline replay: the artifact cuts at the n-th wall check without reading the clock (`-1` / unset = live) |
| `ALMIDE_OMEGA_RECORD` | route | make the native artifact print `__ALMD_OMEGA <ord>` at each region exit whose deadline fired (record on native, replay anywhere) |
| `ALMIDE_ONLY_PASS=value` | ablation | run the optimiser with ONLY the named optional pass (plus the always-on enablers); an unknown name is a hard error |
| `ALMIDE_ORACLE_KEEP` | harness | keep the programs the native borrow-mode oracle generates (tests/native_borrow_oracle_test.rs) instead of deleting them after the run |
| `ALMIDE_ORG_DIR=value` | ci | the checkout directory of the org repos the cross-repo verification scripts walk |
| `ALMIDE_P3_HTTP_STOP=value` | ablation | make the p3 http shim answer its static error right after stage N of the request build (1..=5), to localise a hang |
| `ALMIDE_PASS_EDGES=value` | harness | extra `A<B` pass-order edges (comma-separated) the shuffle honours — the bisection instrument that names the pair a shuffle divergence needs declared |
| `ALMIDE_PROBE_DUMP=value` | harness | the path the heap probe writes its emitted wasm to |
| `ALMIDE_PROBE_IR=value` | harness | the path the heap probe writes its lowered IR to |
| `ALMIDE_PROBE_SRC=value` | harness | the source file the heap probe compiles (unset = the probe is skipped) |
| `ALMIDE_PROFILE` | debug | print per-pass and per-phase timings of the native pipeline |
| `ALMIDE_RC_TRAP_DOUBLE_FREE` | trap | arm the structural leg's double-free trap: releasing a block already at rc 0 traps instead of wrapping |
| `ALMIDE_REGION_DEBUG` | debug | print the region-window pass's decisions (native and structural leg) |
| `ALMIDE_REGION_OFF` | ablation | turn the region-window allocation pass off (native and structural leg) |
| `ALMIDE_REGION_TRAP_STALE` | trap | arm the native region prelude's stale-reference trap (#2200) |
| `ALMIDE_RENDER=value` | ci | the render_program example binary the prelude audit re-renders fixtures with |
| `ALMIDE_REPO=value` | ci | the repository slug a release script targets |
| `ALMIDE_RUN_PROJECT_DIR=value` | tool | the scratch dir `almide run` / `almide build` compile native binaries in, instead of `<temp>/almide-run` (the content-keyed binary cache, its cargo target, its rustc incremental sessions); `almide clean` empties it |
| `ALMIDE_SEMLAW_CASES=value` | harness | how many cases the semantic-laws property test draws |
| `ALMIDE_SHUFFLE_PASSES=value` | gate | run the native passes in the seeded random order the declared dependency edges permit — a pass-dependency probe: the emitted Rust must not change (#2186) |
| `ALMIDE_SIZE_ALONE=value` | harness | the one fixture a child process of the size ratchet measures alone, for its isolation check (#2309); the ratchet sets it on the processes it spawns |
| `ALMIDE_SKIP_PASS=value` | ablation | skip the named optional passes (comma-separated) — a pass-dependency probe: output must not change |
| `ALMIDE_SKIP_VERSION_CHECK` | gate | skip the project's `almide` version requirement check |
| `ALMIDE_STREAM_FUSION_OFF` | ablation | turn the stream-fusion pass off |
| `ALMIDE_TCO_DEBUG` | debug | print the native tail-call loop rewrite decisions |
| `ALMIDE_TEST_LAX_WASM` | gate | let the default `almide test` lane (wasm first, native fallback) PASS a file whose wasm leg diverged — trapped where the native re-run passed; without it a diverged leg fails the run |
| `ALMIDE_TEST_VERBOSE` | tool | show the full cargo / rustc output of the `almide test` harness build |
| `ALMIDE_TIME_PHASES` | debug | print the wall-clock time of each `almide run` phase |
| `ALMIDE_TMPDBG` | debug | print the temporary-drop decisions of the v1 lowering |
| `ALMIDE_TOPLET_DEBUG` | debug | print the cross-module top-let type writes and reads of the checker |
| `ALMIDE_TRACE_PASSES` | debug | name each optimiser pass BEFORE it runs, so a pass that never returns is identifiable |
| `ALMIDE_UPDATE_ALLOC` | harness | regenerate the structural leg's allocation baseline |
| `ALMIDE_UPDATE_DUMPS` | harness | regenerate the structural leg's section-dump goldens |
| `ALMIDE_UPDATE_GAUNTLET` | harness | regenerate the gauntlet manifest |
| `ALMIDE_UPDATE_INTERP_LEDGER` | harness | regenerate the interpreter abstain and bridge-fallback ledgers |
| `ALMIDE_UPDATE_NATIVE_OWN` | harness | regenerate the native result-ownership ledger |
| `ALMIDE_UPDATE_RC_SNAPSHOTS` | harness | regenerate the rc-placement snapshots |
| `ALMIDE_UPDATE_SIZES` | harness | regenerate the structural leg's size baselines |
| `ALMIDE_UPDATE_SIZE_LADDER` | harness | regenerate the stdlib-linking size ladder ledger (#2141) |
| `ALMIDE_UPDATE_SNAPSHOTS` | tool | same as `almide test --update-snapshots` |
| `ALMIDE_UPDATE_SURFACE` | harness | regenerate the exercised-surface golden |
| `ALMIDE_UPDATE_WITNESS_FLOOR` | harness | regenerate the certificate witness floor |
| `ALMIDE_VERBOSE` | debug | same as `almide -v`: surface the native wall-and-fallback notes that a quiet run hides |
| `ALMIDE_VERIFIED_DEBUG` | debug | name the wasm leg that rendered, and why the other declined (the route oracle) |
| `ALMIDE_VERSION_LINE=value` | ci | NOT read from the environment at run time: `build.rs` EMITS it as `cargo:rustc-env`, and `src/main.rs` reads it with `env!` at compile time. It is the string `almide --version` prints — `<version> (<kind>[, <sha>])` — assembled from ALMIDE_BUILD_PROVENANCE and ALMIDE_BUILD_SHA (#2384) |
| `ALMIDE_WALL_REASON` | debug | make `almide test` say WHICH stage of the wasm leg declined a fallback file, not just `v1 wall` |
| `ALMIDE_WASM_FREES` | ci | the frees-churn gate's switch; its compiler reader retired with the v0 emitter (#782), the gate that still sets it is #2207's |
| `ALMIDE_WASM_INCUMBENT` | route | force the INCUMBENT wasm leg (the v1 MIR renderer) instead of the structural-first route |
| `ALMIDE_WASM_STRUCTURAL` | route | force the STRUCTURAL wasm leg for a shape the router would send to the incumbent (the route-flip probe) |
| `ALMIDE_WAT_PRELUDE_REACH` | ci | make the prelude audit re-render every named fixture to measure reachability (CI sets it) |
| `ALMIDE_WITNESS_DUMP` | harness | print every fixture's certificate witness in the witness-floor test |
| `ALMIDE_WRITE_FUZZ_CORPUS` | harness | write the generated fuzz programs to disk |
<!-- almide switches --md: end -->

`ALMIDE_*` 以外:

| 変数 | 説明 |
|---|---|
| `CI` | `true` で `almide test --ci` と同じ(スナップショットを書かない) |
