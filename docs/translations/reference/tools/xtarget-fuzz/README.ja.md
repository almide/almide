<a id="xtarget-fuzz--almide-generative-fuzzer"></a>

# xtarget-fuzz — Almide 生成型 fuzzer

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](README.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](README.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](README.zh-CN.md)



> 原文: [tools/xtarget-fuzz/README.md @ 35297a](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/tools/xtarget-fuzz/README.md)。非規範的な読者向けスナップショット翻訳です。自動同期されません。計測値・主張の独立した再検証は行っていません。原文を正本とします。

completeness roadmap の Stage 3。合成空間を継続的に探索し、**観測可能な相違**とコンパイラ障害を探す機械です。

synthesis、corpus mutation、identity、composition、multi-module shape 系列で Almide プログラムを生成し、両ターゲットでコンパイル・実行する oracle ladder に通して結果を判定します。全プログラムは `(seed, index, family)` の三つ組から再現可能です。発見は `findings/` に保存します。最小化は系列ごとに異なり、shape の発見は最小再現例を主張せずプロジェクト全体を保存します。

<a id="which-oracle-judges-what"></a>

## どの oracle が何を判定するか

*差分* fuzzer には、両経路が共有する bug の oracle がありません。native と構造的 Wasm は frontend と linked-IR 機構を共有します（MIR/WAT renderer の共有ではありません）。そこに誤コンパイルがあると、両経路が同じように間違い、票は全会一致になります。#1322 がまさにその例です。scalar let-alias の誤コンパイルは v0.57.0 から存在し、`spec/wasm_cross` の同等性フィクスチャ全 394 件は green のままでした。

| oracle | 検出できる問題 | カバレッジ |
|---|---|---|
| native ⇄ wasm 差分 | 二つの backend 間の相違 | 全プログラム |
| `almide-interp`（第三の判定者、#516） | interpreter が実行できる場合、一方の backend | 自由に棄権 — 判決ではなく一票 |
| **構成によるもの** | **片方、または両方同時** | identity の scalar/control flow、composition の operator/payload セル、期待出力を持つ shape project |

第三行は他の二つでは提供できません。identity 系列の期待 stdout は自身のソース内のリテラルなので、各経路を**単独**で判定し、両者の一致は何も証明しません。

これは独立クレートです（`tools/wasmgen-harness` 同様、独自の `[workspace]`）。AST / parser / formatter のため親の `almide` crate に path 依存し、oracle ladder では新しくビルドした `almide` バイナリを**外部プロセスとして実行**します。そのため compiler ICE が落とすのは fuzzer ではなく、観測可能な子プロセスです。

<a id="build--run"></a>

## ビルドと実行

```bash
# 1. Build the compiler (the fuzzer drives this binary).
cargo build --release --bin almide          # from the repo root

# 2. Build the fuzzer.
cd tools/xtarget-fuzz && cargo build --release

# 3. Run a campaign (time-budgeted or fixed count).
./target/release/xtarget-fuzz run --minutes 60          # 60-minute hunt
./target/release/xtarget-fuzz run --count 200 --jobs 8  # 200 programs, 8 workers

# A campaign run ENTIRELY under the by-construction oracle (#1332).
./target/release/xtarget-fuzz run --minutes 30 --family identity

# Inspect / reproduce one program deterministically.
./target/release/xtarget-fuzz gen    --seed 100 --index 42   # print source
./target/release/xtarget-fuzz replay --seed 100 --index 42   # re-run the ladder

# Re-judge a saved repro. An identity repro carries its own oracle in
# `// @expect` header lines, so this needs no seed and no family.
./target/release/xtarget-fuzz ladder findings/<dir>/repro.almd

# Catalogue / corpus sizes.
./target/release/xtarget-fuzz stats
```

`--family all|identity|synthesis|composition|shape`（デフォルト `all`）は campaign が使う系列を選びます。**`(seed, index)` がプログラムを再現するのは同じ `--family` の場合だけです。** `all` は生成前の系列選択に乱数一回を使いますが `identity` は使わないため、同じ index でも最初のバイトから異なります。全発見の `meta.txt` に系列を記録し、`reproduce` 行にも明示します。

WASM 実行段には `PATH` 上の `wasmtime` が必要です。repo root と `almide` バイナリは自動検出します。`--repo` / `--almide` で上書きできます。

<a id="the-generator"></a>

## generator

五系列を段階的に選びます（`src/generator/mod.rs` の `Engine::generate`）。Identity は `all` の 3/13、composition は残りの抽選の 3/13、shape は両者の残りの 1/6 です。最終的な残りを synthesis と mutation で 7:3 に分けます。以前の synthesis 54% / mutation 23% は composition と shape の追加前の数値です。

identity の選択は、プログラムの主乱数列ではなく**独立した** RNG sub-stream（`IDENTITY_STREAM_SALT`）で行います。主乱数列に抽選を追加すると乱数の対応が変わり、保存した発見の `(seed, index)` が最小化の元になったプログラムを再生成できなくなります。選択を分離することで、#1332 前の synthesis と mutation の乱数列をバイト同一に保ちます。

<a id="the-identity-family-23--the-by-construction-oracle-1332"></a>

### identity 系列（約 23%）— 構成による oracle（#1332）

`src/generator/identity.rs`。Rustlantis の手法で、各プログラムを**答えから逆向きに**構築します。各 accumulator はリテラル `K` で始まり、初期化と `println` の間の各文群は**恒等変換**です。したがってプログラムは自身のソースに見えるリテラル `K` を出力しなければならず、判定に第二の実行は不要です。

```almide
// @expect a0=-3728
fn main() -> Unit = {
  var a0 = -3728
  let snap2 = a0          // ← #1322's shape: sound only if this COPIES
  a0 = a0 + 1796
  var it8 = 0
  while it8 < 3 {         // ← loop-carried scalar state
    let cur9 = a0
    a0 = cur9 + 896
    it8 = it8 + 1
  }
  a0 = a0 - 2688
  a0 = snap2
  println("a0=${a0}")
}
```

健全性は計測ではなく構造によります。

1. **各ブロックは代数的に恒等です。** 自身の逆操作（`+n`/`-n`、`*m`/`/m`、`xor n`/`xor n`、swap/swap、snapshot/restore）を持つか、生成時に計算した定数（`while` の反復数 × step、`0..<t` の三角数、要素和ゼロのリスト）で補償するか、分岐の**両方**の arm で釣り合わせ、どちらを通っても変わらないようにします。ブロック本体自体が恒等ブロックのリストなので、入れ子も合成できます。
2. **どの算術も overflow や切捨てを起こしません。** generator は `|acc|` の保守的上限を持ち、`2^40` を超えるブロックを拒否します。整数除算は `*m` / `/m` 対の閉じる側だけに現れ、被除数は開始値のちょうど `m ×` です。（Almide の `+`/`-` は `wrapping_*` に lower するため加法の逆操作は上限なしでも成立しますが、`/` と補償ループを正確にするのはこの上限です。）

どちらの性質もコンパイラの正しさには依存しません。それが要点です。ブロックの重みは #1332 の三形状（ループをまたぐ scalar 状態、let-of-var aliasing、branch-arm assignment）へ偏らせています。

**意図的に対象外**: strings、floats、Unicode、データとしての collections、effects、generics。この identity 系列では、構成による安価な逆操作を持ちません。追加の形状は他系列が扱います。composition と shape は独自の構成による期待出力を持ち、synthesis と mutation は主に差分証拠を使います。

**最小化は構造的**です（`minimize::minimize_plan`）。identity の発見はテキストでなく*plan* を縮小します。ほぼ各行が逆操作対の片側なので、一行を消すとプログラムが出力すべき値が変わります。テキストレベルの縮小では bug と無関係な理由で「まだ再現する」ことになり、誤コンパイルを黙って generator の人工的な問題へ変えてしまいます。

<a id="the-composition-family--operatorpayload-interactions"></a>

### composition 系列 — operator/payload の相互作用

`--family composition` は、構成から期待出力が分かる operator × payload × position のセルを標本化します（`src/generator/composition.rs`）。Int/String/tuple/record payload の tail、let、argument 位置で、`??`、`!`、`?`、pipe、match、tuple extraction を扱います。期待答えの決定を backend の一致には頼りません。

<a id="the-shape-family--multi-module-projects-3309"></a>

### shape 系列 — multi-module project（#3309）

`--family shape`（`all` では identity と composition が残した抽選の六回に一回）。二日間で、受理されたのに一方の経路でビルドできない十一プログラムがユーザーから届きました（#3283–#3307）。単一ファイルのものはなく、固定 pool の名前も使っていませんでした。この系列は**プロジェクト**を生成します。宣言モジュールは一つから四つで、各々 sibling module または path-dependency package（entry 自身に置く部分は最大一つ）。全 value kind の top-level let（Rc-backed `Bytes`/`Matrix`、fn 値、closure の record）、closure・nested closure・HOF callback・他モジュールから変更される module `var`s を含めます。名前は敵対的 pool から選びます。大小文字だけ異なる対、native 側の生成 binder（`c`、`_fn_arg0`、`__cap_*`、`__almide_*`）、mangle された static の表記（`almide_rt_<module>_<name>`）です。

語彙は `tests/shape_matrix/gen.rs` をそのまま取り込みます（`#[path]`）。決定的な shape-matrix gate が毎 PR で有限の直積を列挙し、この系列はその境界を越えて標本化します（モジュールごとに複数 let、同時に複数モジュール、深さ四までの closure 入れ子）。各 project は構成によって自身の stdout を知ります。他系列と異なる規則が二つあります。**wasm wall は発見です**（check が受理した project なので、カバレッジの負債ではなく #3286 / #3296 のクラス）。また、発見は丸ごと記録します。`repro.almd` は `// ==== <path> ====` ヘッダーで全ファイルを束ねたものです。最小化しません（`replay --family shape` は scratch dir に project を再作成）。

<a id="the-other-two-families"></a>

### 他の二系列

- **型主導 synthesis** — `src/generator/{term,program}.rs`。型付き項文法から*構成によって*型の正しいプログラムを作ります。目標型を選び、literal、スコープ内変数、戻り型が目標と単一化する stdlib call、HOF 引数の inline lambda、`if` arm からその型の式を生成します。停止のため fuel を制限します。曖昧な literal（`[]`、`none`、`ok`/`err`）は型注釈付き `let` 束縛へ引き上げ、型検査を通します。
- **Stdlib catalogue** — `src/generator/catalogue.rs`。シグネチャは**機械用ソース**、同梱の `stdlib/*.almd` 宣言ファイルを実際の Almide parser で解析して抽出します。選別した*重み表*を解析済み表層に重ね、過去の相違集中領域（string/Unicode、float formatting、closures/HOFs）へ選択を偏らせます。
- **Value pool** — `src/generator/pools.rs`。相違を起こしやすい literal の、名前とコメント付きの表です。multibyte string（`日本語`、emoji、`é`、結合文字、`ß`）、float 境界（`-0.0`、`5e-324`、`1e300`、`0.1+0.2` 形状）、int 極値（`i64::MIN/MAX`、幅の境界）。
- **Mutation** — `src/generator/mutate.rs`。`main` を持つコーパス（`spec/wasm_cross`、`examples` 等）を解析し、`test` ブロックを除去して、型を保持する AST mutation（pool からの literal 摂動、同種の部分式交換、文複製）を適用します。`// wasm:skip` ファイルは既知の相違として除外します。

決定性: プログラム本体には `for_program(seed, index)` で seed を与えた `SplitMix64`（`src/rng.rs`）、系列選択には前述の salt 付き sub-stream を使います。Effect は出力だけに限りません。選別 catalogue は fs/env/io/args/process/net/http から、seed 付きファイル、固定入力、loopback fixture を使った再現可能な sandbox 操作を許します。host が決める結果には、宣言されていれば property verdict を使います。境界は `SANDBOXED_EFFECT_MODULES`、`generator/verdicts.rs`、`oracle/sandbox.rs`、`oracle/fixture.rs` を参照してください。任意の非 sandbox effect を意味しません。

<a id="the-oracle-ladder"></a>

## oracle ladder

`src/oracle/` — 安価なものから高価なものへ進み、最初の失敗で分類します。

| 段 | 検査 | 失敗の意味 |
|------|-------|---------------|
| a | `almide check` が受理 | **generator bug**（型が正しいと約束した）— 計数するが発見ではない |
| b | `parse∘fmt` が冪等 | formatter の不安定性 |
| c | native build + run（ICE なし） | native codegen 障害 |
| d | wasm build + validate | wasm codegen 障害 |
| e | **self-check** — 各経路と `@expect` 出力 | **誤コンパイル**。`native` / `wasm` / **`both legs`** に帰属 |
| f | 両方実行し stdout/exit をバイト比較 | **相違**（または hang） |
| g | 両方と参照 interpreter を比較 | interpreter が投票すれば、一方の backend が誤り |

(e) 段は構成による oracle を持つプログラム（identity、composition、shape 系列）だけで働きます。両経路を同時に有罪判定できる唯一の段なので差分比較の**前**に実行します。ただし resource-limit skip（C-196 stack、C-197 wasm32 memory）の**後**です。そのため wasm OOM は偽の誤コンパイルでなく skip のままです。

resource-limit skip（`ladder.rs` の `resource_class`）は各経路固有の abort signature を読みます。wasm 側は `call stack exhausted`（標準 wasmtime の trap）/ `Error: out of memory`、native 側は `stack overflow`（C-196 の `Error:
stack overflow`、または未登録 thread の Rust 自身のメッセージ）/ `memory allocation of … bytes failed` です。制限に達した経路が死ぬ前に出した全出力が、もう一方の stdout の prefix である場合だけ、その実行を捨てます。もう一方の結末は条件に含みません（#2382）。「wasm が最初の割当で OOM、native は進んで意図的 panic」は resource skip、「wasm が native の出さない行を出してから OOM」は制限で途中終了した発見です。resource signature のない abort は常に意味論の verdict（`RunFailureDivergence` または `OutputDivergence`）です。

## Minimizer

`src/minimize.rs` — delta-debugging。文を削除してから式を単純化し、ladder を再実行して、同じ種類の発見が再現する場合だけ縮小を保持します。identity は代わりに構造的経路（`minimize_plan`、上記）を使います。出力先は `findings/<kind>/` です。`repro.almd`、`original.almd`、`meta.txt`（seed/index/family/replay command）、`native.out`、`wasm.out`。`(kind, summary)` で重複を除きます。

## Throughput

WASM build と native cargo build がボトルネックです。worker ごとに**独立した** build scratch dir（`ALMIDE_RUN_PROJECT_DIR`）を持つため、共有 `/tmp` の build flock に直列化されず、コア数とともに throughput が伸びます。ローカル計測（Apple M-series、campaign summary 行参照）では、`--jobs` と native-build cache の温まり方に応じて約 **40–120 programs/min** です（プログラムごとのコストは native の `cargo` 再ビルドが支配し、warm cache では約 0.25 s/program）。

## Nightly CI

`.github/workflows/fuzz-nightly.yml` は 03:00 UTC（および manual dispatch）に時間予算付き campaign を実行し、`findings/` を artifact に upload、発見 > 0 なら `fuzz` label の tracking issue を作成・更新します。PR CI は意図的に変更しません。fuzzer は nightly 専用です。

identity 系列を取り込むための nightly 変更は不要です。デフォルトの `--family all` は identity、composition、shape の構成による oracle を含みます。実際の割合は古い identity だけの割合でなく、段階的選択と完了プログラム数によります。campaign summary は正確な割合を `self-checked = N (P%)` と表示します。`SelfCheckFailure` は正しさのクラスなので、見つかった夜は他の誤コンパイルと同じく red になり tracking issue を作成します。

将来の改良（未接続）: 一つの matrix shard に `--family identity` を指定し、重みの再調整にかかわらず夜間カバレッジの固定割合を oracle 付きにします。
