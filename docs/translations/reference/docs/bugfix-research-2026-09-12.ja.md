<a id="issue-fixes-research-and-evidence-2026-09-12"></a>

# Issue 修正：調査と証拠（2026-09-12）

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](bugfix-research-2026-09-12.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](bugfix-research-2026-09-12.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](bugfix-research-2026-09-12.zh-CN.md)

> 固定原文の非規範的な履歴参照資料。原本不変更、自動同期なし。記載のテスト・測定・CI・コマンド・外部参照確認を新たに実行していない。初期のローカル限定／未解決という記述と後の統合・修正が併存し、当時の結果は現行ソフトウェアの認証ではない。終盤の本文は dialect 3分類・欠陥2分類とするが、表は dialect 2行・欠陥3行（うち1行は同じ箇所）であり、数え方の差を黙って整合させず保持した。予約・隔離・証明の主張には原文の除外範囲がある。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/bugfix-research-2026-09-12.md)

baseline：`edbd7b751e4902e7ea3daff622757d35c71ded13`。
実装変更はまだローカルにあり、issue の close や release の公開は行っていない。
前提となる C-214 judge 改訂は、[almide/als#65](https://github.com/almide/als/pull/65) として別途提出している。

<a id="changes"></a>

## 変更

| Issue | 変更 | 回帰の証拠 |
| --- | --- | --- |
| #2114 | 既存の owned-string append helper で stdin 行を伸ばす。LF/EOF で止め、共有 cursor を保ち、native の CR 除去と一致させる。 | `tests/io_read_line_boundary_test.rs`：native、core WASI、direct component。長さ 4094–4097、8193、65537。旧境界をまたぐ UTF-8、CRLF、EOF、後続の line/byte/drain read。 |
| #2112 | 両方の Value collection constructor で borrowed slice を受理し、owned Value payload へコピーする。 | `spec/wasm_cross/value_borrowed_constructors.almd`、既存 C-103 に関連付け。引数、空入力、nested object、snapshot 独立性。native/wasm/interpreter の一致。 |
| #2107 | named-record lookahead で comment を無視し、末尾 comma なしで record pattern が閉じる前の newline/comment trivia を受理する。 | `tests/record_comment_parse_test.rs`。 |
| #2106 | case comment を意味を持たない variant metadata として保存し、expression-member comment を既存 ExprId table で保存する。comment 付き container を物理行に出力する。named/anonymous record は field parser を共有する。 | `tests/fmt_container_comments_test.rs`：variant 表記、record、spread record、call、tuple、保存と冪等性。既存 formatter/parser suite。 |
| #2113（一部） | E015 は名前の完全一致を要求し、host/effect 候補を除外し、本体は比較していないと明示し、置換 snippet を提示しない。outline は bundled `args` を認識する。direct component build は利用不能な生成 host operation を E081 で拒否し、出力を書く前に operation と p2/p3 target を名指しする。 | `tests/reimpl_lint_best_match_test.rs`、更新した E015 diagnostic fixture、`tests/component_availability_test.rs`（p2/p3 argv 拒否、既存出力保存、preview1 argv 受理）。 |
| #2108 | CHEATSHEET の廃止済み `0..n` を `0..<n` に置換する。 | 周辺 range 例との source review。 |
| #2109 | 拒否する range-pattern、range-type、prefix `..` / `..=` 形式を E031 と grammar 固有の案内で診断し、機械的な書き換えはしない。有効な rest marker を保つ。 | `tests/retired_range_context_test.rs`：issue 再現、operator span、安全でない fix-it の不在、合法な list/record rest と現行 range 表記。 |
| #2110 | 初期化専用 `_start` を持つ structural library emission を追加し、全 public export と到達可能な callee の lowering を要求する。main なし build はそこへ経路を向け、通常 program emission は引き続き main を要求する。 | `tests/wasm_library_route_test.rs`：board-parser 再現全体、既定／強制 structural routing、export 存在、Wasmtime 呼び出し、lowering できない public 本体の厳格な拒否。既存 export DCE 回帰も合格。 |
| #2111 | 古い HTTP stub 案を stock/embedded/component の区別と関数単位 availability の正本に置き換える。 | `proofs/target-availability.toml` と既存 component/embedded 実装。 |

変更前後の再現：変更前 release binary は 4097 ASCII bytes に **4096** を返し、変更後 binary は **4097** を返す。Value fixture は変更前 binary で E0308 に失敗し、変更後実装で合格する。

<a id="design-references"></a>

## 設計の参照資料

指定された `../almide-references` checkout から読んだもの：

- Koka `3ac4f001ab7277b484d661fdbada1aaf8d01ecbf`、`kklib/src/os.c`、`kk_os_read_line`：固定 scratch buffer を newline/EOF まで繰り返し append する。buffer size は行長上限ではない。Almide は Koka の error surface や C-string 仮定を採用せず、自身の EOF/CR 規約と既存の幾何級数的 append allocator を維持する。
- Go `e51216de8e26247ee0f3d2cfa576233b0d29f542`、`src/go/ast/commentmap.go`：comment を source 順に node と対応させる。Almide も trivia を意味的 serialization から除き、同一行 trailing comment と leading comment を区別する。同じく `src/go/parser/parser.go` の `errorExpected` は拒否構文を現在の token に結び付ける。E031 拡張は外側の Almide grammar と実 token span を使い、未対応形式に expression operator の機械適用可能な rewrite を与えない。
- Rust `0d31508599a7814a7044e9a7a871e3dc5f037753`、`compiler/rustc_mir_build/src/builder/expr/as_place.rs`：borrowed projection を owned storage に変換するとき `to_vec()` を使う。Value 境界はその所有権区別に従い、borrowed Vec を要求しない。
- Zig `lib/std/os/wasi.zig` は argv/environment を `wasi_snapshot_preview1` に明示的に結び付ける。component world にこれらの service があるとは推定できない。component audit は各 direct shim が実装する operation を使い、p3 filesystem と HTTP capability を維持する。`lib/std/wasm.zig` は function/memory export を host entry point とは独立にモデル化する。structural library mode は `almide-mir/src/pipeline.rs` に記載した Almide 既存 #881 ABI に従う。`_start` は global を初期化し、public function は host が呼べる export root として残る。
- Rust `library/std/src/thread/scoped.rs` は、全子を scope exit 前に join することで bounded な lifetime に borrowed capture を結び付ける。これは capture-clone 除去に非脱出／lifetime 境界の確立が必要な理由を示す。返される closure に対して read-only access を観察するだけでは不十分。

<a id="als-and-quality"></a>

## ALS と品質

judge の CONTRIBUTING 指示と ledger を既存 pin `52af321db00a452aa41d591a3ec4591354c8dbd8` で読んだ。contract statement、contract ID、ALS pin は変更していない。C-103 に実装証拠を加え、双方向 fixture link と生成文書を更新した。contract gate、pinned-ID gate、syntax-element coverage gate が合格する。

既存設定の Codopsy **2.2.0** で commissioned wasm crate は **A / 91** を維持する。syntax、frontend、tools は A のまま。これを repository 全体の A と解釈してはならない。隔離した HEAD archive でも、`crates` は **B / 82**、`src` は **B / 86** であり、codegen cleanup 前はその集計値が変わらなかった。最新の `crates` 集計は B / 83。しきい値と除外は緩めていない。codegen crate は B / 89 から始まった（隔離 baseline で確認）。部分的な variable-reference traversal を共通の free-variable analysis に置換し、runtime-registry I/O error を伝播させた後は A / 91。欠けた include file と宣言済み kernel module は、文脈なし panic や無言の省略ではなく source path 付きで失敗する。`tests/runtime_registry_io_test.rs` は両方の source 欠落ケースを扱う。repository 全体の集計 A 要件は未達のまま。この cleanup 後、codegen snapshot 全 13 件、native fold/index-block 回帰、registry I/O 回帰 2 件、Almide file 全 438 件が合格する。

<a id="validation"></a>

## 検証

- 最終 release build：`almide test` は **438** file 全件合格、**425** は wasm、**13** は native fallback。loopback TCP test は filesystem/network sandbox 外での実行が必要で、そこで合格する。
- formatter/parser 回帰 suite：既存 single-file corpus のコピーを format して出力を検査するものを含め、Rust test **156** 件が合格。
- diagnostic-harness check 6 件が合格（変更した hint の期待値を更新後、失敗した check を再実行）。E015/outline は focused test 5 件。
- stdin 境界 matrix は native、core WASI、direct component で合格。新 Value fixture は cross-target と interpreter-oracle の両 gate で合格。
- contract ledger、ALS pin、ALS syntax coverage、wasm file discipline、`git diff --check` が合格。repository の全 CI workflow を実行したわけではない。
- #2109 後：parser/formatter/context suite 156 test、既存 range-spelling CLI check 2 件に合格し、context test も正確な diagnostic-span assertion 付きで合格。再 build release は Almide test file 全 438 件に再び合格。syntax Codopsy は A / 95 のまま。
- #2113 component build audit 後：新しい拒否／出力保存 test が合格し、既存 p2/p3 component test 全 12 件を実行して合格。再 build release はまた全 438 Almide file に合格。変更した wasm-run crate は Codopsy 2.2.0 で A / 96。

<a id="remaining-work"></a>

## 残作業

<a id="wasi-version-verification"></a>

### WASI 版の検証

2026-09-12 時点の公式 [WASI release](https://github.com/WebAssembly/WASI/releases/tag/v0.3.1) は **0.3.1**、2026-08-11 公開。0.3.0 の async 機能に加えて Component Model の maps、implements、external-id 対応を要求する。check-in 済み `crates/almide-wasm-run/wit/p3/world.wit` は明示的に **0.3.0** を対象とする。版文字列だけでは新しい host との互換性を証明も反証もできず、完全な 0.3.1 適合は実証されていない。

既存 p3 実装は稼働する。`cargo test --test
component_p3_test -- --nocapture` は Wasmtime **47.0.3** で **11** test 全件合格、実行 skip なし。coverage は stdout/stderr/exit 一致、stdin、deterministic fan、filesystem read/write、並行 prefetch と cancellation、structural routing、HTTP と framed HTTP、transport error 後の復旧を含む。test は `ALMIDE_COMPONENT_P3=1` と `--component` を使う。既定の direct component は依然 p2 を選ぶ。p3 filesystem write は未完とする古い comment は訂正した。

<a id="open-issues"></a>

### 未解決 issue

- #2103：以前の judge の C-214 は `exec failed:` を明示的に約束していた。規範改訂は [almide/als#65](https://github.com/almide/als/pull/65)、commit `2490704`、`/tmp/als-issue2103` で準備。ALS-R5、C-214、missing-command fixture を同時に変更し、正確な timeout-fire message は保持した。reference independence/kernel/totality と runner self-test を含め、全 local judge gate が合格。author/verifier 記録は独立性なしと明示する。required CI が合格し、改訂は `4fa62a2211d71823b198dbd4076f881503bf4729` として merge された。別 commit `9e29ef1af` で実装 pin を進め、runtime status twins は改訂された command を識別する表記を実装する。流入する develop 変更にはすでに C-348 の検査付き byte slice がある。
- #2098：closure 構築時の capture copy と enumerate 具体化には、なお広範な最適化作業が必要。scalar enumerate/fold は、tuple が脱出できない場合に tuple list と要素ごとの tuple 割り当てを除去するようになり、scalar snapshot で eager-source semantics を保持する。`research/benchmark/perf/spectralnorm/compare.py` は両 target 上の 3 表記を、出力一致、warmup、round-robin sample 9 回、任意の強制 ratio budget 付きで比較する。n=1200 では、check-in 済み命令型 corpus baseline 比で native indexed fold は 2.36×、wasm enumerate fold は 3.21×。中央値と限界は README 参照。最適化完了の主張ではない。最初の最適化後、wasm enumerate は命令型 baseline 比 129.72 ms / 1.17× となり、365.81 ms / 3.21× から下がった。empty/scalar と escaping-tuple/heap-element fallback ケースは `tests/enumerate_fold_test.rs` で native と比較テストする。全 438 Almide file が合格、wasm Codopsy は A / 91。native の同期 scalar fold は、証明された即時 iterator call を通して read-only capture を借用するようになった。脱出する capture と共有可変 capture は既存所有権を保つ。iterator collector closure は Rust iterator ABI 向けに unbox する。indexed benchmark は native 上で現在 58.28 ms / 1.42×、以前は 97.39 ms / 2.36×。`tests/fold_capture_borrow_test.rs` は出力、脱出／可変挙動、空入力、生成 capture-copy binding の不在を検査する。既存 capture/borrow 3 suite の追加 6 test、全 13 codegen snapshot（direct-fold snapshot は review して更新）、native differential check 4 件、全 438 Almide file が再び合格。より広い capture borrowing は残る。
- #2113：通常の direct component build は、未対応 host op を名前付き diagnostic で拒否するようになった。low-level transform を直接使う caller には generic runtime refusal が残り、その stderr/exit 挙動には別途 runtime 作業が必要。E015 と outline は修正済み。
- #2110 の board parser は、main なしで check/build できるようになった（手動再現では 31,517-byte stock-WASI artifact）。export inventory と RateLimited/BoardNotFound を試す呼び出しの両方が Wasmtime で合格。

<a id="integration-with-develop-at-dc2f934df"></a>

### dc2f934df の develop との統合

初期修正と別の ALS pin commit を `fix/issue-regressions-20260912` に公開した後、流入する develop 変更を統合した。その検査付き byte slice、所有権修正、process/fs diagnostic、stream fusion を保持する。完全な free-variable check を、新たに切り出された statement-lowering module へ移す。capture-free fold は既存 fusion 経路に残す。早く昇格させると map/filter chain と fold の間に collection が入り、pipe-chain snapshot に捕捉される。

統合後、Almide test file 全 440 件に合格（4,081 tests、WASM 428、native fallback 12）。focused Rust integration/snapshot test 36 件も合格。contract traceability は 348 contract と 704 symmetric fixture を覆い、ALS pin、pass roster、file discipline、layout check が合格。Codopsy v2.2.0 はしきい値変更なしで codegen A/91、wasm A/90。これら module score は repository 全体の rank A を証明しない。

<a id="process-status-diagnostics-2103"></a>

### Process status diagnostic（#2103）

merge された ALS-R5/C-214 改訂と別途 pin 更新に従い、両 status function は呼び出しと二重引用符付き command を識別し、埋め込まれた quote/backslash/control character を escape し、引数一覧を省略する。timeout twin は指定 millisecond bound を含む。host error text はそのまま、deadline error は `exec timed out after <ms>ms` を保持する。local Rust 参照（`library/std/src/sys/process/unix/unix.rs`、spawn は 64–68 行と 148–150 行）は InvalidInput と host errno error を区別して保持する。本実装はその error を再構築・分類せず、装飾する。通常の process function は既存 quoting contract を保ち、この改訂は status twins を規定する。

process matrix は両 status function を含む。compile 済み fixture は quote、backslash、CR/LF/tab、Unicode、private argument の省略、実際の host error suffix を確認する。orphaned-pipe deadline 回帰を含む focused test 7 件に合格。全 440 Almide file 合格（4,082 tests）。変更した timeout fixture の AST hash だけ更新する。以前の Value constructor fixture にも、不足していた AST/check/run manifest entry を追加し、生成 AST、成功した check、WASM 出力から測定する。

最初の PR CI run は既存 fixture 14 件の古い E015 diagnostic hash を検出した。すべて exit status 0 を維持する。10 件には改訂された signature-only warning が入り、4 件では曖昧な名前一致（`c_sqrt`/`c_floor`/`c_ceil`、`parse1`、UFCS helper 名、`bo`）がなくなる。source 宣言と以前の substring/edit-distance rule を review した。測定したこれらの行だけ更新し、diagnostic 除外や比較 floor は緩めない。

<a id="second-pass-the-ci-runs-own-findings-same-day"></a>

## 第2巡：CI run 自身が見つけた問題（同日）

最初の PR run 2 回で赤になった gate 5 件は、それぞれ実際の欠陥であり flake ではなかった。

| Gate | 欠陥 | 変更 |
| --- | --- | --- |
| Commissioned wasm gates | 新しい `value_borrowed_constructors` fixture は spine/AST manifest にあったが、size ledger にも allocation ledger にもなかった | 測定した 3 行を追加し、他の全行はそのまま（全再生成すると同 commit で無関係な drift まで主張することになる） |
| Test Rust shard 3 | `pass_capture_borrow.rs` の `Reads` visitor が `expr.kind` を `_ => {}` で match していた。traversal totality lint は後続 `walk_expr` でなくその形を読む | 意味を変えず guard に書き換えた |
| Test Rust shard 0 | #2106 は member にどんな leading comment があっても物理行を強制し、inline `/* */` は node と同じ行に残すという #1404 の規則を壊した | trigger を「同じ行を共有できない comment」（`//` または複数行 block comment）に変更。inline のものは `fmt_expr_sans_leading` が出力する |
| Coq / TCB | 抽出 checker は Rocq 9.1.1 で 1,348 行、`develop` の cache 済み opam root にある旧 Rocq で 1,196 行と測定される | CI の値を再現する local 抽出（`proofs/build-checker.sh`、Rocq 9.1.1）から block を再生成した |
| Emit & Format | op-31 shim が transform から抜け、Hello, world が 7 bytes 小さくなった | `scripts/gen-readme-stats.sh --measure` |

<a id="2116--ioread_all-was-bounded-by-the-park-not-the-heap"></a>

## #2116 — `io.read_all` を制限していたのは heap でなく park

変更前の測定：native は任意サイズを読む。stock WASI は **257,024 B** で `Error: host op unsupported in the WASI build` として拒否する。p2 component は **326,657 B** で stdout/stderr とも空、exit 1 で終了する。Cloudflare Worker で #2113 が報告した症状を、通常の `wasmtime run` で再現した。

`io.read_all` は今、共有 cursor から 4096-byte chunk で stream を取り（全3 shim が提供する op 35）、#2114 の `io.read_line` lowering と同じく owned-string accumulator に append する。全3 leg は測定した全サイズで一致し、`tests/io_read_all_capacity_test.rs` では 1 MB、手動では 100 MB まで確認した。**host op 31 を廃止**し、guest、p1/p2/p3 shim、embedded host、`P1_SERVED_OPS` から除いた。同時に unsupported-op message を共有していた3箇所のうち2箇所と `g_pcap` global もなくなった。残る3箇所目の env.set overlay の拒否は、自分の名前を示すようになった。

読んだ参照：Zig `e4cbd752c`、`lib/std/heap/arena_allocator.zig` の `resize`。buffer の末端が arena の frontier と一致するときだけ in-place に拡張し、それ以外は拒否する。この先例が関係するのは本変更ではなく #2117 の残る半分であり、issue に記録した。

<a id="2117--the-accumulator-above-the-largest-size-class"></a>

## #2117 — 最大 size class を超える accumulator

`$alloc` が class へ丸めるのは `16 << 15` 未満だけであり、`$free` は合計 ≥ 2^20 の block を放棄する。そのため 512 KB を超える string は append のたび再割り当てし、サイズ不足となった各 copy を leak した。5.5 MB の accumulator が address space を使い切り C-197 に至った。`$str_append` はその ceiling を超えると capacity の2倍を要求するようになった。`$list_push` と `$map_reserve` 自身と同じ方針で、未満の領域はすべて byte-identical のまま。`io.read_all` は 100 MB に 0.08 s で達し、`acc = acc + s` は 8 MB を 0.01 s で作る。`tests/string_accumulator_growth_test.rs` は両 leg が 32 MiB `--heap-cap` 内で終了することを要求し、allocator 変更だけを戻すと wasm leg は再び `Error: out of memory` になる。

size ledger は専用 commit で再申告する。197 emitted module が幾何級数的 append のコスト（集計 +0.90 %）を払い、509 shipped module から op-31 shim がなくなる（各 −149 B、codec fixture の増大後の集計 +0.59 %）。

<a id="2098--the-spelling-tax-is-now-gated"></a>

## #2098 — 表記による追加コストに gate を設定

`scripts/check-spelling-ratio.sh` は、1 leg の1回の round-robin run で同じ program の表記を比較するため、native/Rust ratio gate と同様に runner の影響が比率で打ち消される。n=1200、5 sample、6 artifact の stdout が byte-identical の測定：native `1.00 / 1.39 / 1.70`、wasm `1.00 / 0.89 /
0.72`。issue 起票時の 4.25× は今 **0.72×**。文書化された表記が structural leg の3表記中で最速である。gate は perf-ratchet job で 2.5× を使い、native 値より上に雑音の余裕を置く。

<a id="filed-not-fixed"></a>

## 起票済み、未修正

同系統の3欠陥は急いで本変更へ入れず、測定・再現して起票した。#2118（op 32 が境界なしに `b_len` bytes を park に書く。現時点で source から到達不能、全 stdlib caller は8を渡す）、#2119（direct component の `cabi_realloc` が grow 失敗で無言終了し、C-197 形式に反する）、#2120（park の 261,120-byte staging room を超えると `env.get` が `none`、`env.args` が `[]` を返すが、native は値を返す。無言の誤答で、同系統最悪のもの）。

<a id="2120--the-park-stops-being-where-a-result-has-to-fit"></a>

## #2120 — 結果が収まる必要のある場所が park でなくなる

environ block または argv が park の data page（1 byte 単位で測定し 261,120 bytes）を超えると、native が値を返すのに `env.get` は `none`、`env.args` は `[]` を返していた。どちらも普通で有効に見える答えなので、program は stderr なし・exit 0 のまま `??` fallback または引数なし経路へ進んだ。

shim の `host_read` は固定 address ではなく `g_ppos` global から copy するようになった。p2 shim がすでに持つ間接化と同じである。結果は3つ：

- page より大きい結果は新しく grow した memory の最上部へ stage する。`stage_for` は結果に必要な page の2倍を grow し、上端の `need` bytes を8整列で使う（alignment を検査する host は奇数の pointer array を即拒否する。測定：`Pointer not aligned to 4`）。`fs_call` と `host_read` の間の guest 自身の割り当ては bump head、つまり旧 memory end 以下から始まり、最大 `need` bytes なので staging bytes に届かない。
- overlay hit と environ hit は in-place に返す。値はすでに memory 内にあるため、page への copy がなくなり、`env.args` が frame payload 全体を `park + DATA` へ下げる操作もなくなる。
- machine に拒否された grow は誤答でなく C-197 形式（`Error: out of memory`、exit 1）になる。

参照した他実装は同じ問題に逆向きから答える。Zig（`../almide-references/zig` @ `e4cbd752c8c0`、`lib/std/process.zig` の `getEnvMap` の WASI arm）は `environ_sizes_get` を呼び、pointer array と buffer の両方を caller の allocator から割り当て、退出時に解放する。wasi-libc も `malloc` で同じことをする。どの場合も宛先は guest memory だが、所有者が異なる。shim はその経路を取れない。guest の `host_read` が `fs_call` の return 後に走るため、shim が割り当てた block は call を超えて生き残り、解放する者がいなくなる。memory を grow して frontier より上へ stage するのは、shim が守れる位置に所有権を残した同じ答えである。bytes は構築上 transient であり、bump allocator が後で page を再利用する。

`tests/env_staging_capacity_test.rs` は、両 service について 16、260,000（旧 page 内）、262,000（境界を少し超えた点）、900,000 bytes で native と stock WASI を比較固定し、小さな3引数の順序と framing も対象とする。

間接化を出荷するのは利用する service だけ。corpus module 697個のうち688個は byte-identical で、env/args に到達する9個が96–101 bytes を払う。`env.set` の64 KiB overlay は自身の境界を保つ。#2116 以降は無言でなく大きく名前を示す拒否であり、残る最後の park-size 制限である。

<a id="2118--the-one-writer-into-the-span-with-no-bound"></a>

## #2118 — span に境界なしで書く唯一の writer

op 32 は staging address に `b_len` bytes を書き、何も検査していなかった。span を超える長さは env.set overlay、その後は生存中 guest block に到達していたはずである。source からは到達できない。stdlib 内の全 `prim.random_get` call site は8を要求する。だからこそ境界は各 artifact の shim でなく emitter に置く。`prim.rs` は `ENTROPY_MAX` 内の literal でない長さを拒否するため、検査は byte も runtime branch も増やさない。target 選択前の拒否なので p2/p3 arm も継承する。`entropy_length_ok` は境界の両側と計算された長さの形で単体テストし、corpus は byte-identical に lowering される。

<a id="2119--the-canonical-abis-mute-corner"></a>

## #2119 — canonical ABI の無言の隅

issue は `cabi_realloc` から C-197 行を書けるかを問い、そこに書く案を推奨していた。答えは不可で、証拠は spec の文章でなく host にある。wasmtime 47.0.4 の `src/runtime/component/func.rs` は全 lowering を `set_may_leave(false)` で囲み、「この実行中 component は import を呼ぶことを禁じられる」と comment する。`func/host.rs` は `lower_result_and_exit_call`（host が生成した値のため realloc を呼ぶ window）を同様に囲む。`wasmtime-internal-cranelift` の `compiler/component.rs` は `Trampoline::LowerImport` に対して `trapz may_leave, TRAP_CANNOT_LEAVE_COMPONENT` で強制する。したがって realloc からの host call は単に未証明なのではなく trap する。

shim はまさにそれを行っていた。`--heap-cap 1048576` 付き p2 component が 1.2 MB の stdin を drain する場合の測定：

```
2: wasm trap: cannot leave component instance
EXIT=134
```

欠陥は issue が述べた無言終了ではなく、T6 が禁じる形式の raw trap であり、memory 不足なのに component 構造を責めていた。issue の option 1 は host 自身の source によって閉じられ、option 3（無言終了を ledger に記す）は誤ったものを記録することになっていた。

**予約の規律（option 2、shim が提供する op に対して全域化）。** 全 landing を1段前、abort が認められる通常 guest code で予約する。

- `$reserve(need)` は memory を grow し、拒否されれば `$alloc` と同じ行 `Error: out of memory` を出力して exit 1。
- 結果が `cabi_realloc` 経由で着地する import の前に、shim は自ら選んだ call の境界を予約する。op 35 blocking-read は `a_len`、op 32 entropy は `b_len`。host は要求以上を返せないため予約は正確であり、realloc 自身の grow には到達しなくなる。
- guest 側の割り当て（p3 read-to-end buffer、fan slot table、http body buffer）は `$alloc`、すなわち `$reserve` と同じ bump の経路を通る。size 式の重複ではなく、各 call site の1 index 変更で済む。
- `cabi_realloc` の grow 失敗分岐は裸の `unreachable` になり、channel のない隅として記録する。memory に裏付けられない pointer を返すと guest を無言で破壊し、終了より悪い。

他実装も同じ終わり方をする。wit-bindgen の `cabi_realloc` は Rust の `handle_alloc_error` を呼び abort し、wasi-libc も abort する。ecosystem のどの guest もその位置から報告しない。誰にもできないからである。

証明外に残るのは、size を host だけが選ぶ一つの landing、宣言した64 KiB（`PREOPEN_RESERVE`）を予約する p3 preopen table である。heap cap の下で preopen path がそれを超える host は依然 trap で終わる。この残余は、発見されるままにせず C-197 に記載する。

`tests/component_realloc_oom_test.rs` は双方向を固定する。行と exit 1 が出なければならず、`cannot leave component instance` と `wasm trap` は出てはならない。op-35 予約を除くと失敗することを実証した。

<a id="2098--two-measurements-one-of-them-of-the-build-profile"></a>

## #2098 — 2つの測定、その一つはビルドプロファイルの測定

issue の native 値（以前の修正後、添字方式2.36倍、enumerate方式1.68倍）は、`--release` なしの `almide build`、つまりプロジェクトの既定プロファイル `opt-level = 1`、LTOなしで測定していた。この水準は意図的で不可欠である。LLVM の末尾呼び出し最適化はそれ未満では動かず、これがないと `spec/wasm_cross/mutual_tail_recursion.almd` が native stack をあふれさせる（`src/cli/cargo_build.rs` にその発見日を記録）。一方、LLVM が iterator chain を平坦化しない水準でもある。

同じツリー・機械・ラウンドロビンのハーネスで測定した。

| 書き方 | 既定プロファイル | `--release` |
|---|---:|---:|
| 添字による fold | 1.42x | **1.01x** |
| enumerate fold（修正前） | 1.68x | 1.37x |

したがって添字方式に残る差はコンパイラの出力内容ではない。opt-level 3 では同じ生成ソースが命令型の形と正確に一致する。enumerate 方式の差は最適化後も残り、これこそが本当の発見だった。

次に考えがちな「`almide build` の既定を `--release` にすべき」という推論は、それ自体の測定に耐えない。両プロファイルをラウンドロビンで測ると、`--release` は opt-level 1 がすでに得意とする2方式では遅い（命令型47.95対41.93 ms、enumerate 48.14対41.77 ms）。速いのは添字方式だけ（48.23対59.65）。opt-level 3 は3方式とも約48 ms、opt-level 1 は2方式が約42、1方式が約60となる。守る価値があるのは書き方による費用が同じという性質であり、一方のプロファイルの勝利ではない。

**enumerate の余分な費用が実際にあった場所。** 生成コードを手で編集して再ビルドし切り分けた（n=1200、9標本、ラウンドロビン、出力同一）。

| 生成コード | 比率 |
|---|---:|
| 元の出力 | 1.38x |
| `.enumerate()` を融合、capture の clone は残る | 1.14x |
| 融合し、さらに source を借用 | **1.01x** |

仕組みは2つで、どちらも行ごと、したがって実行ごとに48,000回起きる。`almide_rt_list_enumerate` が構築し、fold が一度走査して破棄する `Vec<(i64, f64)>` と、そこへ渡すためにキャプチャした1,200要素ベクトルを丸ごとコピーする処理である。

**修正。** SOURCE 位置の `list.enumerate` は source ではなく、その上の ADAPTER である。`IterStep::Enumerate` はプログラムのテキストを持たず、位置は iterator から来る。したがって純粋・全域的・順序保存であり、merge rule の純粋性と abort の計上に新たな段階を加えない。変換した source の前にある clone は構造上不要になる。CloneInsertion は、消費する runtime call が呼び出し元の値を奪わないよう置いたが、その呼び出しを融合で消したことで消費側がなくなった。clone を除く際の例外として残るのは、同じ変数を変更する callback であり、仮定せず検査する。実際にはそのような変数はすでに読み取りで snapshot を作る shared cell で、mutation fixture が両経路でそれを固定している。

結果：既定プロファイルで native enumerate は1.68倍から **1.01倍**へ。wasm 経路は変更していない（`IterChain` は Rust 専用）。`tests/enumerate_source_fusion_test.rs` は借用する形、snapshot の形、chain 内の4位置、native/wasm の一致を固定する。両 shape assertion は source-adapter rule を無効化すると失敗することを実証した。

perf コーパスは両プロファイルとそれぞれが答える問いを記録するようになり、CI 予算は2.5倍から2.0倍へ下がる。

<a id="2128--a-boundary-that-is-a-check-because-a-rule-alone-already-failed-once"></a>

## #2128 — 規則だけでは一度失敗したため、検査としての境界

ロードマップの決定は、構造化データ（TOML、markdown、台帳、生成出力）を読むゲートは Almide で書き、grep して終了するゲートは bash のままというものだ。好みではなく費用の非対称性による。60余りの `check-*.sh` はそれぞれ grep/awk/sed を作り直すが、Almide 側は型付き TOML/markdown reader をすでに用意しており、次の構造化ゲートは無料で使える。

規則だけでは守られないことを、プロジェクト自身の測定が示す。以前の「`.sh` を commit しない」という目標は、「件数が増えたらゲートが CI を失敗させる」という完了条件付きで出たが、実装されなかった。commit 済み shell は6週間で50から124ファイルへ増え、追加81件のうち50件は `scripts/check-*.sh` だった。何も測定していなかったため、誰も気づかなかった。

そこで境界を既存の台帳に加える。各 `[[gate]]` 行は第2軸 `reads = structured | scalar | UNCLASSIFIED` を持ち、`structured` を宣言しながらパスが `.sh` の行は検査に失敗する。`UNCLASSIFIED` は台帳の `UNVERIFIED` と同じ、負債を正直に示す表現で、双方向で減少のみを認める。既存80行の分類を終える前に仕組みを導入できる。

**Almide のゲートは台帳のどこにも存在しなかった。** 列挙対象が `scripts/check-*.sh` と名前付き2スクリプトだけだったため、Almide のゲートは検証クラスを持ち得なかった。今は `tools/almide-gates/src/main.almd` のサブコマンド dispatch も読み、`tools/almide-gates:<subcommand>` として扱う。dispatch 自体から読むので両者は乖離せず、parse が見失ったら失敗する下限もある。

見えるようにした代償は上限だった。`output-parity` と `fuzz-track-record` が、元の `.sh` との受入検査なしで見えるようになったため、`unverified_ceiling` は0から2へ動いた。負債自体は新しくない。それを扱うのが未解決の #2130 であり、台帳から見えないゲートより未検証と記録されたゲートの方がよい。

導入前に3つの失敗方向を実証した。`.sh` パスの `structured` 行、行のない Almide ゲート、`reads` フィールドのない行である。ラチェットは双方向に働くことも示した。1行を分類して件数が上限未満になると、ゲートはヘッダーの上限も引き下げるよう要求した。

この検査自身の行も80行の一つである。bash で TOML 台帳を読むので、自らの境界に従えば Almide に属する。それを認めるために正直な負債の分類がある。
<a id="2134--one-mistake-two-codes-and-only-the-indirect-spelling-readable"></a>

## #2134 — 一つの間違い、二つのコード、読めるのは間接的な書き方だけ

`none()` は次を報告していた。

```
error[E001]: type mismatch in function call: expected Option[?0] but got fn() -> Option[Int]
  hint: Fix the expression type or change the expected type
```

同じ間違いを束縛経由で書くと、すでに役立つ内容を報告していた。

```
error[E002]: `f` is not a function — it has type Option[?0]
  hint: `f` is a value; only functions and closures can be called
```

つまり診断自体は存在したが、直接的な書き方はそこに到達しなかった。`none` は `ExprKind::None` であり、call dispatch の計算済み callee 分岐へ進む。その分岐が行うのは `constrain(callee_ty, fn(args) -> ?ret)` だけだ。読者が見たメッセージはこの単一化の両側であり、期待型が推論スロットだと、未解決の `?ret` が型注釈を追加せよという第2の、見当違いな E025 を生んだ。一つのタイプミスに二つのエラーが出て、後者は修正から遠ざけた。

callee 位置の値は、どこにあっても E002 になる。`none` には専用の修正文と `try:` を付ける（間違いは括弧だけで、引数を取る `some` があるからこそ対称的な書き方が正しく見える）。E025 の連鎖もなくなる。制約を加えないので、未確定なまま残るものがない。

逆方向も同じ形の裏返しであり、どちらも名前の距離による候補では届かない。名前を間違えてはいないためだ。

| 実際 | 期待 | 起きたこと |
|---|---|---|
| `fn(A) -> T` | `T` | 引数が渡されなかった（`some(x)` の代わりに `some`） |
| `fn() -> T` | `T` | 呼び出しが行われなかった |

両方とも型だけで決まる。実際の型が関数で、その結果の型の先頭が期待型の先頭と一致する。このため `arity_shape_hint` が、既存の fallible-callback shape hint と並んで E001 hint chain に加わる。

C-237 は契約文自体で旧コード（`none()` は E001）を名指していた一方、括弧内の `(none is a value, not a function)` は E001 でなく E002 のメッセージを説明していた。台帳も同じ変更で訂正する。

<a id="2130-step-1--the-diff-that-was-specified-and-never-built-and-what-it-found"></a>

## #2130 step 1 — 指定されながら作られなかった diff と、その発見

Unit 0.46 は 7 つのゲートを `tools/almide-gates/` へ移植し、元の `.sh` とのバイト同一性を受入れ検査と定めていた。CI が実行するのは `.sh` 側だけ。対になる実装は `almide test tools/almide-gates/` で試されるが、両者を比較するものはなかった。ツール自身のコメントが代償を示す。「鮮度ゲートは bash とコミット済み文書を比較するが、移植版をそのどちらとも比較しない」。

**diff は初回実行で乖離を発見した。** これこそ何かを正式採用する前に diff を作るべき理由のすべてである。`stamp` の FATAL 経路 — PATH のバイナリが workspace build と一致しない場合 — には 2 つの乖離があった：

- bash は原因（最後の `make install` 後に `cargo test` が `target/release` を再リンクしたこと）と修正方法を示すようになったが、移植版は当初の 1 行のままだった；
- bash はそこで戻るので、toolchain 行と末尾の罫線はその経路では表示されない。一方、移植版は続けてすべて表示した。

どちらも、対になる実装を正式採用した日に異なる証拠テキストを出荷することになっていた。FATAL 経路こそ開発者が実際に遭遇するものだ。移植版は現在、両経路（不一致分岐と通常の一覧）でバイト単位で一致する。そのためには早期復帰を `return` ではなく構造で表す必要があった — Almide に return はない。

`check-contracts` は exit code を含めすでにバイト同一だった。

2 つの対になる実装は比較の外に残り、ごまかさず名前を挙げている。`output-parity` は全コーパスを走査し、`fuzz-track-record` は GitHub API を照会するため、それぞれ専用の fixture 設計がなければ diff に意味がない。正式採用は何も行わず、`.sh` も削除しない。それは soak 後の step 3 である。
<a id="2133--not-an-oom-a-case-payload-that-never-took-ownership"></a>

## #2133 — OOM ではなく、所有権を獲得しなかった case payload

273 バイトの入力で `Error: out of memory` として報告された。develop ではすでに `wasm trap: out of bounds memory access` へ変わっており、障害アドレスは `0x30384238` — 表示可能な ASCII 4 バイト。文字列 payload がブロック handle として `$rc_inc` に届くときの姿である。どちらの形も不具合そのものではなく、その結果だった。

**縮約**は推測でなく報告者のツリーから削って行った。`report.render` → `match j.verdict` → `Supported` 分岐 → 1 つの `probe_rows(matched)` → その中の `list.map` → 最後にはツリーを完全に離れ、13 行になった：

```almide
type Probe = { kind: String }
type Verdict = | Supported{ matched: List[Probe] } | Nothing

fn of(xs: List[String]) -> List[Probe] = list.map(xs, (f) => Probe { kind: f })

fn verdict_of(xs: List[String]) -> Verdict = {
  let checked = of(xs)
  Supported { matched: checked }
}

fn tags(v: Verdict) -> List[String] =
  match v { Supported{ matched } => list.map(matched, (p) => p.kind), Nothing => [] }

effect fn main() -> Unit = println(list.join(tags(verdict_of(["a", "b"])), ","))
```

Native は `a,b` を表示する。wasm 経路は解放済みメモリのバイトを表示し、**両方とも exit 0**。ユーザーが trap として遭遇した形は、もう一段小さくすると静かに誤答する。差分コーパスはこの種のためにあるが、そのコーパスに対応するセルはなかった。

**出力が明白に示していた。** `verdict_of` は次のように描画された：

```wat
local.get 7
local.get 1            ;; checked
i32.store offset=20    ;; the payload store — no rc_inc anywhere near it
local.get 7
local.get 1
call 48                ;; the epilogue releases checked
```

コードコメントの訳：local.get 1 は checked。payload の store の近くには rc_inc が全くない。epilogue が checked を release する。

**原因**は `lower_named_record` の variant 分岐で呼出しが 1 つ欠けていたこと。10 行下の record 分岐は全 field を `self.rc_share_guard(fexpr, fty)` 経由で格納するが、record 形状の CASE 分岐はそうしていなかった。明記した field も宣言された default も同様である。そのため、`let` 束縛のヒープ値を payload へ移すと、frame がなお束縛を所有するのに共同所有の `+1` なしで格納し、epilogue が case の保持するブロックを解放していた。

この問題があったのは `let` の書き方だけ。`Supported { matched: of(xs) }` は producer を inline にし、新しい値が credit とともに移るので、最初から正しかった。TUPLE 形状 constructor（`lower_variant_ctor`）には作成時から guard があった。機能ではなく書き方の背後に隠れる不具合は、機能別 fixture では見つからない。そのため新しい corpus fixture は `let` 形式、default-field 形式、すべての producer frame が復帰した後に読み戻す形を試す。

**証拠は corpus fixture ではなく差分テストであり、その理由はゲートにある。** これには heap variant case を返す関数が必要だが、旧来の brick はそれを全く lowering できない。export でも `local` でも、`match` があってもなくても同じ。したがって `spec/wasm_cross/` に置くには、縮小のみの ratchet を一度に 3 つ動かすことになった。host/browser determinism の wall 上限（各 28 -> 29）と `proofs/walled-real-baseline.txt` の新規 2 項目であり、後者のゲート自身が項目追加は「レビューを要する後退であり、修正ではない」と言う。`tests/variant_case_payload_ownership_test.rs` がすでに提供するカバレッジ — 両経路をビルド・実行して比較する — のために 3 ratchet を緩めるのは悪い交換であり、ゲートの拒否は正しかった。fixture を撤回し、上限を元に戻した。
<a id="2129--the-three-mis-reads-were-the-bash-regexes-and-two-real-ones-were-elsewhere"></a>

## #2129 — 3 回の読み違いは BASH の正規表現で、実際の 2 件は別の場所にあった

issue が引用するコメント —

> 正規表現ではなく scanner として書く。この Unit ですでに入力を 3 回黙って読み違えた部分が正規表現だからだ。

— は、bash ゲートを移植する際に書いた `tools/almide-gates/src/ledger_coverage.almd` にある。Unit 0.46 のログが指す 3 件はすべて移植版が再現しなければならなかった bash pattern であり、`stdlib/regex` の呼出しではない。空白に敏感な pattern で `spec = "..."` key を照合し、空白の異なる行を黙って落としたこと；境界の選択肢を含めない `grep -qE "^## $sec"` により `ALS-T1` が `## ALS-T14` に解決され、不正な key が通ったこと；除去後の行順を rank にしたため、コメント行を class と数えたこと。

したがって issue に書かれた step 1 には対象がない。正しい指摘は欠落の方である。`regex` はここでは誰も信頼しないモジュールで、その不信が妥当かを測ったものはなかった。そこで測定を先に行った。

**外部エンジンとの 5,922 ケースの比較**（Python の `re`、参照エンジン同士が一致するよう選んだ部分集合）で、**5 分類の 114 件の不一致**を見つけた。3 つは方言、2 つは不具合だった：

| 分類 | 判定 |
|---|---|
| `split` が capture group を返す | 方言 — Python は含めるが Rust/Go/Almide は含めない |
| 空の選択肢（`\|ab`） | 方言 — Python 3.7+ は空マッチの位置で非空マッチを再試行する；**ripgrep と照合し Rust が Almide と一致すると確認** |
| `[]]` が決してマッチしない | **不具合** |
| 空文字列にマッチする pattern 上の `split` | **不具合** |
| `split` が末尾の空 field を落とす | **不具合**（同じ箇所） |

**`[]]`。** class の最初の位置の `]` は literal のメンバーである。POSIX、および PCRE、Python、Rust、Go、JS はすべてこれに従う。終端記号と読むと pattern は空 class と余った `]` になり、エラーもマッチもなく、外からは何も見えない。これはまさにコメントが疑っていた形である。

**ゼロ幅マッチに対する `split`。** ループは「次の field の開始位置」と「次の検索の開始位置」という異なる 2 つの意味に 1 つの cursor を使っていた。そのためゼロ幅マッチが 1 文字を食い、`split("^", "abc")` は `["a", "bc"]` と答えて、`a` を本来属する field から移した。Python、Rust、Go はいずれも `["", "abc"]` と答える。cursor を分け、末尾 field を常に出力する（`split("$", "a")` = `["a", ""]`）ことで、114 件の不一致が 6 件へ減り、残りはすべて確認済みの 1 つの方言分類だった。

両エンジン — `runtime/rs/src/regex.rs` と self-hosted の対応実装 `stdlib/regex_engine.almd` — に修正が入っている。C-032 が両経路のバイト同一性を要求するためで、native だけを直すと直ちにその fuzz が失敗した。これはゲートが機能しているということだ。


<a id="the-exclusion-that-removed-more-than-it-meant-to"></a>

### 意図以上を取り除いた除外

regex の 2 fixture を run-parity manifest から外すのは正しく見えた。固定 oracle は修正前の答えを記録しているので、それらを判定できない。しかし、ゲートだけが見つけられる形で誤っていた。run manifest は exercised-surface、allocation、size の各走査の corpus list でもあるため、行を引くとその fixture がすべてから消える。`surface_matrix` は失敗し、wasm 経路で実際に試される surface から消えた 8 構文を名指しした。`regex.captures`、`find`、`find_all`、`full_match`、`is_match`、`replace`、`replace_first`、`split`。その 2 fixture だけがそこでモジュールを試していたからである。除外すれば、修正を出荷すると同時に修正対象のカバレッジを黙ってなくしていた。

`scripts/lib/run-oracle-stale.txt` は既存の register にはなかった精密な機構である。行は manifest に残り、他のすべての走査は引き続き fixture を通り、oracle の答えとの比較だけがスキップする。その比較は 1 つでなく 3 つあり、`run_parity`、`backend_parity`、WASI ゲートが同じ hash を比較する。最初を修正すると、2 番目、3 番目も順に失敗した。reader は `almide_corpus` に置き、第 4 の比較が黙って忘れられないようにする。manifest を corpus list としてのみ使う 4 走査（exercised surface、allocation、size、witness floor）は設計上変更しない。両方向に縮小のみであり、登録が再び oracle と一致し始めたら失敗する。実際に一致する fixture を指す偽の行で、3 比較すべてについてそれを実証した。それらの allocation watermark は低下した（88,144 から 87,824、291,248 から 290,608 — split 修正は割り当てる field が少ない）。この変更で新しい値を承認する。

**恒常的なゲートが持続する側である。** C-032 の oracle は我々自身の native エンジンであり、両経路の一致を確認するが、両者が同じように誤読する規則は見えない。まさにそのため両不具合は生き残った。新しい `scripts/check-regex-reference.sh` は、外部エンジンがすでに答えた 6,624 問を両経路へ 0.15 s で問い、その部分集合と意図的除外を `proofs/regex/README.md` に明記する。登録前に、偽造した参照行が両経路を失敗させることを実証した。
