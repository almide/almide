<a id="effect-inspection-harness"></a>

# effect 検査ハーネス

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](effect-inspection-harness.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](effect-inspection-harness.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](effect-inspection-harness.zh-CN.md)

> [英語の正本](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/effect-inspection-harness.md)（固定コミット `35297a612fb29e3f78f6587aba253aa91d51f466`）の非規範的な読者用スナップショット。元の文書が正本であり、この写しは自動同期されず、生成器・チェッカーの入力でもない。以下の日付、計画、完了状態、測定値、比較主張は当時の原文の記述を保持したもので、独立した再現や現行版についての検証ではない。コードブロックとコメントは原文のまま保持する。過去の収集用メタデータは、動作しない読者向け注記として表示する。

> 原文の日付付き設計・採択状態を以下に再現する。

> 原文の注意点：保存された 10 月 12 日の設計は、採択・更新を示す冒頭注記に従って読む。133/36 の件数、全面的スキップ、末尾の未決の問いは当時の記述であり、現状ではない。方針は ADR-0027 が解決しており、対応シグネチャの受入れはすべての effect の合格証明ではない。性質比較はホストが選ぶ値の一致を意味せず、ループバック・キー不要という隔離は fixture に限る。

<a id="adopted-design-and-implemented-harness"></a>

## 採択された設計と実装済みハーネス

以下の 10 月 12 日の問題件数と作業順序は、設計時のスナップショットである。[ADR-0027](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0027-effect-functions-are-inspected-in-a-fixed-host-world.md) が 10 月 17 日に検査規則を採択したため、末尾の owner への問いは未決の方針依頼ではない。現在の [catalogue](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/tools/xtarget-fuzz/src/generator/catalogue.rs) は、サンドボックス化した fs/env/io/args/process/net/http モジュール、ホスト不要の zlib 関数、性質判定関数から、対応済みのシグネチャを受け入れる。すべての effect 関数をスキップするわけではない。受入れには引き続きシグネチャとターゲットのフィルターがかかり、すべての effect 関数が検査に合格したという主張ではない。関数ごとの状態には、古い 133/36 の件数ではなく、[検査台帳](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/proofs/stdlib-inspection.toml)と[ターゲット対応状況](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/proofs/target-availability.toml)を用いる。

**状態：** 設計、2026-10-12。[ADR-0027](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0027-effect-functions-are-inspected-in-a-fixed-host-world.md)（2026-10-17）で決定。
**追跡対象：** #3552（stdlib 検査活動）。

<a id="problem"></a>

## 問題

stdlib 検査台帳（`proofs/stdlib-inspection.toml`）は、すべての公開関数を五つの基準で検査することを求める。その一つは native と wasm で関数の振る舞いが同じであること。基準と行の `class = "effect"` フィールドはすでに存在し、effect 行には組込みホストおよび標準 WASI ランタイムでの parity を要求する。

プログラム生成器（`tools/xtarget-fuzz`）は effect 関数をまるごとスキップする（`signature_from_decl`）。そのため、**未検査 458 関数のうち 133 関数**は、品質に関係なく検査できない。

| モジュール | 未検査の effect fn |
|---|---|
| fs | 34 |
| http | 28 |
| net | 24 |
| process | 13 |
| env | 8 |
| zlib（ファイル操作版） | 8 |
| args | 6 |
| io | 6 |
| random | 4 |
| datetime | 2 |

<a id="what-was-consulted"></a>

## 参照したもの

- **`../almide-references`。** `RESEARCH-verification.md` は差分 fuzzing（V-3、V-4）を扱うが、純粋なプログラムだけである。effect のテスト、サンドボックス化、WASI 適合性については何も見つからなかった。**そこにはこの設計の資料がなく、設計は以下のパターンに基づく。**
- **このリポジトリの既存クロスターゲット effect テスト。** `tests/wasm_runtime_test_parts/common.rs` と `corpus.rs` は、すべての effect fixture を新しい `tempfile::tempdir()` 内で、`wasmtime --dir=/` または preopen したディレクトリと `-S inherit-env=y` を使って実行する。native はそのディレクトリを cwd にする。インタープリターには独自のサンドボックス化された fs がある（`crates/almide-interp/src/dispatch_vfs.rs`）。
- **`proofs/target-availability.toml`。** 各 wasm 実行系が対応する effect 関数を列挙する。133 のうち 36（http streaming と `*_response`、`net` のすべて、`process.stdin_lines`）は組込みホストで利用できず、標準 p1 では 62 が利用できない。

<a id="design-four-tiers-by-what-makes-a-run-reproducible"></a>

## 設計：実行を再現可能にする要因に応じた四つの層

<a id="e1-sandboxable-and-deterministic-about-62-functions"></a>

### E1: サンドボックス化でき、決定的（約 62 関数）

fs、env（`get`、`set`、`cwd`、`temp_dir`、`args`）、args、io（stdin）、zlib のファイル操作版を対象にする。

- **実行ごとに固定した世界。** 各プログラムを各実行系で、固定ツリーを初期状態として用意した新しい作業ディレクトリで実行する（既知のバイト列を持つファイルを数個、サブディレクトリ、ホストが許せば symlink）。env、argv、stdin も固定する。
  - native：cwd = 作業ディレクトリ；
  - 組込みホスト：cwd = 作業ディレクトリで `almide run --target wasm`；
  - 標準 WASI：`wasmtime --dir=.` と明示的な `--env`、args。
- **parity が比較するもの：** stdout、stderr、終了コード、および実行後の作業ツリー snapshot（パス、種類、サイズ、バイト列）。何も出力しなくても、書込み結果が異なれば乖離である。
- **生成器：** `effect fn main`。fs 関数のパス引数は初期ツリーから選ぶ（存在するものとしないもの）。成功だけでなくエラー経路も通す。
- **Complexity：** サイズパラメーターはファイル長（`read_text`、`read_bytes`、`fold_lines`）または行数。判定器は変更しない。コストがホスト側に属する関数（`mkdir_p`、`exists`）は 1 と宣言する。
- **Diamond：** 実行ごとに同じサンドボックス設定。

<a id="e2-non-deterministic-random-datetimenow--monotonic_ns-envmillis--unix_timestamp--sleep_ms"></a>

### E2: 非決定的（random、`datetime.now` / `monotonic_ns`、`env.millis` / `unix_timestamp` / `sleep_ms`）

値は実行系間で一致しない。parity は**性質**で判定する：終了コード、型、宣言された範囲（`random.int(lo, hi)` が範囲内、shuffle が順列である、2 回の `monotonic_ns` 読取りが非減少、`sleep_ms(k)` が少なくとも k ms かかる）。台帳の parity フィールドに検査した性質を明記し、バイト等価より弱いことが読者に分かるようにする。

<a id="e3-process-13"></a>

### E3: process（13）

起動するのは固定の副作用なしコマンドだけ：`/bin/echo`、`/usr/bin/true`、stdin を与えた `/bin/cat`。parity は捕捉した出力とステータスを比較する。`process.exit` は終了コードだけで判定する。`process.kill` と `spawn` には、ハーネスが回収する子プロセスを与える。

<a id="e4-network-http-28-net-24"></a>

### E4: network（http 28、net 24）

実行ごとに起動するループバック fixture サーバー：事前に決めた応答（streaming と chunked 応答を含む）の HTTP サーバー、および TCP / Unix エコーサーバー。外部ホストはなく、API キーもない（`anthropic_` / `openai_` の streaming 呼出しは、base URL を通じて fixture を向く）。

**36 関数は wasm でブロックされている**（`target-availability.toml` の組込み実行系）。wasi:http@0.3 の移植（#1710）が入るまで、その関数で基準 1 は成立しない。未検査のままにする。基準を「native のみ」に狭めない。

<a id="alternatives-considered"></a>

## 検討した代替案

- **テストのため stdlib 内の effect を mock 化する。** 却下：検査が判定するのは mock となり、クロスターゲットバグがまさに存在する各実行系の実際のホストバインディングではなくなる。
- **ホスト呼出しを記録・再生する。** サンドボックスで実際に実行できる E1 と E3 では却下。E4 ではループバック fixture が不安定と判明した場合に限り有用である。
- **effect は手書きの `spec/wasm_cross` fixture に任せる。** 却下：それらは関数ごとの例である。基準 4 は毎回新しく生成した組合せを求め、それによって今日、純粋関数の一群で九つの欠陥が見つかった。

<a id="compatibility"></a>

## 互換性

- ツールのみ：`tools/xtarget-fuzz` と検査スクリプト。言語、stdlib、codegen の変更はない。
- E2 行の台帳の `parity` フィールドには、検査する性質を記す。

<a id="order-of-work"></a>

## 作業順序

1. **E1：** 三つの runner のサンドボックス設定、比較へのツリー snapshot 追加、生成器の effect `main`、catalogue の fs、env、args、io。
2. **E2：** 性質の判定器。
3. **E3：** 固定コマンド集合を使う process。
4. **E4：** wasm 対応の http 部分集合から始めるループバック fixture。

<a id="open-question-for-the-owner"></a>

## owner への未決の問い

組込み wasm に未対応の 36 関数は、#1710 まで未検査のままにするか（上記設計）、それとも明示的に native 専用とした別の検査済み印を付けるか。ここでは決めない。
