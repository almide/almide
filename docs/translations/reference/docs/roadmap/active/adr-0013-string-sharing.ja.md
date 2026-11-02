<a id="adr-0013-native-string-as-a-shared-buffer--implementation"></a>

# ADR-0013：native String を共有bufferにする — 実装

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](adr-0013-string-sharing.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](adr-0013-string-sharing.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](adr-0013-string-sharing.zh-CN.md)

> 非規範的な固定原文の歴史的実装読者版。原本不変更、自動同期やbenchmark/corpus/Miri/検査stampの独立再実行なし。stage 1着地、stage 2 branch内完了、旧次段階/未完節、10月17日の最終flipを別の履歴層として保持し、現在のmerge/対応状況を主張しない。計測は機械・版・workload・除外file・harnessの限定付きで、変更したharness間の数値は明示通り比較不能。丸められたゼロ時間は計測報告のままで実行コストゼロを保証しない。thread安全性は所述marking規律に依存し独立証明ではなく、旧Miri/被覆未確認を後の完了文で黙って解消しない。コマンド/GCE測定/owner裁定は歴史記録で実行や支出の新許可ではない。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/adr-0013-string-sharing.md)

**状態：** stage 1着地（#3715、2026-10-15）。stage 2は `a13-flip-final` branchで完了（2026-10-17）：
- 着地済み：step 1（alias、#3716）、型（#3717）、literal（#3719）。
- branch上：step 3〜6、thread境界、step 7。共有bufferが唯一のnative Almide Stringとなり、`ALMIDE_SHARED_STR` switchは削除（末尾「Step 7」参照）。
**決定：** [ADR-0013](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0013-string-shared-immutable-buffer.md)（2026-09-04受理）。
**owner裁定2026-10-15：** stdlib検査の個別修正直後に実装する。代わりに条件付きで消費されるparameterのborrow推論を変更しない。

<a id="why-now"></a>

## なぜ今か

stdlib検査で `result.filter` はnativeでcallごとO(n)、wasmでO(1)と計測された。
失敗branchだけが消費するparameterへlive `String` を渡し、nativeはcall箇所に `x.clone()` をrenderする。

同じ形を6言語で実行（macOS arm64、n=20,000→320,000時のcallごとのµs）：

| モデル | callごと | nに伴い増加 |
|---|---|---|
| Rust、owned parameter + caller clone（当時のAlmide native） | 0.22 → 4.25 | はい（19倍） |
| Rust、borrowed + 消費branchでclone | 0.000 → 0.000 | そのbranchのみ |
| Rust `Rc<str>`、Swift、Go、Python、Node、Ruby | 一定 | いいえ |

owned-copyモデルだけが引数サイズのコストを払う。wasm側は既に共有（rc block）。
ADR-0013の発見に別方向から再び出会った。

<a id="stage-1-the-ratchet-landed"></a>

## Stage 1：ratchet（着地済み）

反証条件：strchurnかfastaが**5%以上**遅くなり、thread-shared variantでも回復しなければstage 2を戻す。
ノイズが5%を十分下回る測定でしか判定できない。

CIの `check-perf-ratio.sh` はstrchurnとdecodeを報告するが固定基準にしない。ratioがarchitectureに依存するため。
そこで反証条件は固定1機で判定：

- `research/benchmark/perf/gce_ab.sh <ref-a> <ref-b> <out>` は使い捨てGCE VM1台で両refをbuild。
  同じbench sourceで `bench.py`（full workloads、native + rust）をA、B、A、Bと交互実行。
- `research/benchmark/perf/ab_summary.py <out>/res` が集計。benchごとにnative時間中央値、各側almide/rust比中央値、
  B対A、全roundにわたるratioのばらつきを示す。

**ノイズ下限：** develop 0d93660eaをA/B両方。GCE c4-standard-4
（Intel Xeon Platinum 8581C、4 vCPU）、Ubuntu 24.04、Linux 7.0.0-1014-gcp、rustc 1.99.0。
3 round ×5 run、中央値、2026-10-15。

| Bench | 手書きRustとの比 | B対A（同build） | 6 roundのばらつき |
|---|---:|---:|---:|
| fasta | 1.164 | +0.07% | 1.18% |
| strchurn（同形参照） | 1.126 | −0.67% | 1.38% |
| decode | 0.783 | +2.03% | 3.11% |

- 反証行fasta/strchurnは約1.4%以内で再現し、5%回帰は明瞭に見える。
- decodeはノイズが約3%と大きい。ADR通り、報告するが反証条件に使わない。
- これらがstage 2 baseline数値。

<a id="stage-2-the-representation-next"></a>

## Stage 2：表現（次）

develop 0d93660ea時点の一覧：

| 表面 | 件数 |
|---|---:|
| `runtime/rs/src` 内の `pub fn almide_rt_*` | 809 |
| … `String`（または `Result<String, …>`）を返す | 61 |
| … `String` を値で受ける | 17 |
| runtime内の `Vec<String>` 言及 | 32 |
| `String` 型を記すcodegen / MIR箇所 | 17 |

コード前に解決する未決設計点：

1. **型。** 最初の測定：Rust microbenchmark、macOS arm64、5回中最良、2026-10-15。
   prototypeはsession scratchpad。選択前にGCEで再測定する。

   | 操作 | `String`（当時） | `Rc<str>` | `Rc<String>` | 単一block `[rc, len, cap \| bytes]` |
   |---|---:|---:|---:|---:|
   | 短い文字列2M個構築 | 28.1 ms | 34.5（+21%） | 56.7（+100%） | 31.0（+10%） |
   | 100 KB文字列を20k回共有 | 25.3 ms | 0.00 | 0.00 | 0.00 |
   | unique文字列に2M文字push | 0.72 ms | 二次（capacityなし） | 1.47（2倍） | 3.18（4.4倍） |
   | 2M回の等値scan | 0.57 ms | 0.51 | 1.05 | 0.73 |

   - **`Rc<str>`** はcapacityがなく、`string.push` が毎回文字列を再構築する。却下。
   - **`Rc<String>`** は2回確保と追加間接参照を払う。
   - **単一block** はwasm側layout。構築と共有で勝つ。未最適化prototypeではunique pushが4.4倍遅い。
     毎pushでcount検査しinline fast pathがないため。stage 2着地前にstrbuild ratchet内へ下げる必要がある。
   - **push差はcount検査でなくaliasing。** byte storeのpointerをcompilerがheaderから分離できず、
     毎pushで `rc`、`len`、`cap` をreload。fast path inline化でも改善せず（3.21 ms）。
   - **unique view** で解決：countを1回検査後、len/capをlocalに保ちview終了時writeback。
     **0.76 ms**に到達、Stringは0.72。
   - よって同じ `var` への連続pushをcodegenが1つのunique viewにすれば、単一blockは全箇所で
     `String` に並べる。`var s` のin-place append windowが既にその連続区間。
2. **`mut` 表面。** `string.push` / `string.clear` はRcCow（`Rc::make_mut` 意味論）：
   uniqueならその場、共有ならcopy。List / Mapと同規則。
3. **fan境界。** Kokaのthread-shared flipを使い、scoped threadへcaptureされた値だけatomicへ。
   stage 2 ratchetが判定。flipによるstrchurn回帰なら停止。
4. **ABI。** runtime関数は `Deref` 経由で働く `&str` を引き続き受ける。
   `String` を返す関数は共有型を返し、境界で1回変換。

Stage 3（`split` / `slice` / `trim` / `lines` のseamless slice）はstage 2後。
Stage 4（inline small strings）は本書範囲外。

<a id="stage-2-the-migration-plan"></a>

## Stage 2：移行計画

develop 0d93660eaで表面を調査。file:lineはそのcommit由来。

**native rendererは2つ、programごと選択。**
- `src/cli/mod.rs:240`（`render_v1_native_or_fallback`）は全programでMIR v1を試し、壁が1つでもあればv0 walkerへfallback。
  - v1はmulti-module、top-level let、heap tail recursion、Codec / `Value` programでwallするので、多くの実programはwalker。
  - `almide test` と `install` は常にv0。
- metered programはv0 codeへv1 regionを**splice**可能（`mod.rs:323-328`）。両rendererのString表現は同型でなければならない。
- v1出力は `runtime/rs` やcodegen preludeを含まず、独自header（`render_native.rs:229`）とshim（`render_native_shims.rs`）を持つ。

**手順。** 各々単独で着地し、全gate greenを維持。

1. **alias、動作変更なし。**
   - `type AlmideString = String;` を `rust_runtime_prelude`（`crates/almide-codegen/src/lib.rs:343`）とv1 headerに各1回。
     metered spliceでは定義がちょうど1つになる必要。
   - Almide String型を記す全箇所がaliasを出力：
     - walker：`walker/types.rs:138`、`templates/rust.toml:242`、error型default（`types.rs:172`、`expressions_calls_binops.rs:549`）。
     - v1：`render_native*.rs` 内の `NTy::Str` render。
   - aliasは透過なのでruntime無変更でcompile。
   - gate：spec corpus生成Rustの型位置にbare `String` がないことをgrep検査。
2. **flag背後の型。**
   - `AlmideStr`：単一block `[rc, len, cap | bytes]`、`Deref<Target = str>`、`From<String>`、`From<&str>`、
     `Display`、byte基準 `PartialEq` / `Ord` / `Hash`、`Clone` = count+1、上で計測したviewを返す `unique()`。
   - 単体testと型単独Miri run。
   - flag `ALMIDE_SHARED_STR=1` でaliasをflip。既定off。
3. **構築箇所。**
   - literalは `AlmideStr::from("..")`、またはSwift literal同様のstatic immortal blockがさらによい。
   - concatは `AlmideStr` 向け `AlmideConcat` を通し、左辺がuniqueならunique view。
   - `format!` 結果は `From<String>` で境界変換。
   - error payload（`Result<T, AlmideStr>`）。
4. **runtime ABI。**
   - `&str` parameter（140）は `Deref` 経由で継続。戻り値（`-> String` 51、`Vec<String>` 16、
     `Option<String>` 13、`Result<_, String>` 124）は関数境界で変換。
   - `&mut String` 4つ（`string.push` / `clear`、`value.rs:411`）は `&mut AlmideStr` を受け `unique()` を使う。
   - `buildscript/runtime_registry.rs:314` はruntime source textからmodeをparseするので、新表記をpatternに教える。
5. **in-place window。速度でなく正しさの手順。**
   - MIR v1 append window（`render_native_append.rs:28-45`）は `s = __str_concat(s, x)` を `s.push_str(x)` とrender。
     健全性説明は「すべての `Dup` はdeep `.clone()`」（コメント15〜20行）。共有bufferでは偽なので `unique()` を通す必要。
   - walkerの `AlmideConcat<String> for String`（`lib.rs:348-353`）も同じ義務。
   - gate：両側で `let t = s; s = s + "x"` が `t` を変えないalias fixture。
6. **`AlmideValue::Str`**（`value.rs:18`）を `Str(AlmideStr)` へ。ADR-0018 / #1679のstage。
7. **既定をflip。** stage 1 baselineに対し `gce_ab.sh` を実行。strchurn/fastaが5%以上遅くないこと。
   その後alias flagを除く。

**観測動作を不変にするもの：**
- 各手順で全spec corpusを両側実行しcross-target契約も実行。
- step 2〜6は7までflag背後で、回帰ならflag flipで旧表現へ戻れる。

**step 3〜4 ratchet（2026-10-15計測）。**
- **全数調査：** aliasを `AlmideStr` に向け（probe flag）、`spec/lang` と `spec/stdlib` 全file（463）をemitし、
  `rustc --test` で各々compileしてerror計数。
  - flag offでは同harnessで0 error。ただしstandalone rustcでrustlsをlinkできないhttp10fileを除く。
    環境由来なので両計数から除外。
- **移行規則：** 各構築箇所を、aliasが `String` なら恒等、`AlmideStr` なら変換となる
  `AlmideString::from(..)` / `.into()` へ。flag offの動作は変わらず、全数調査件数は減少のみ。

| 状態 | flag onのerror |
|---|---:|
| aliasのみ（#3716） | 8,924 |
| + literalを `AlmideString::from("..")` へ | 7,326 |
| + `int.to_string` / `float.to_string` が `AlmideString` を返す（probe） | 3,657 |

ここから**distinct site**も計数（runtime内errorは463fileすべてで繰返す）：

| 状態（probe、flag on） | error | distinct site |
|---|---:|---:|
| literal + `AlmideStr` 向け `AlmideConcat` impl | 6,664 | 1,421 |
| + runtime `-> String` 46関数が `AlmideString` を返す | 2,631 | 1,193 |
| + runtime `Result<_, String>` 137シグネチャを `AlmideString` 表記へ | 9,501 | 1,137 |

最後にraw countが増えたのはerrorの一部がruntime自身へ移り、各fileで反復するため。
distinct siteは減少。flag offでは各段階0を維持。

**ここからflagが実在（2026-10-17）。** `ALMIDE_SHARED_STR=1` を登録switch化。
prelude（`crates/almide-codegen/src/prelude_string.rs`）が `almide-str` sourceを `mod almide_str_block` としてspliceし、
aliasを `AlmideStr` に向ける。probe編集が不要になりharnessも変更：
- `almide <file> --target rust` でemit、`rustc --test --emit=metadata` で型検査。
  463からrustls / flate2が必要な10を除く453file。
- distinct siteは（error code、message、問題source行）。
- flag-on preludeは `AlmideStr` 向け `AlmideConcat` impl
  （unique時のみその場appendする `AlmideStr::concat` 経由）と構造的 `into_almide` を持つ。
  Stringは変換、`Vec` / `Option` / `Result` / tuple / `HashMap` / `AlmideMap` は要素map、scalarと `AlmideValue` は恒等。
- 新harnessで起点を再測定するため、以下の行と上の行は比較不能。

| 状態（flag on、このharness） | error | distinct site | errorのあるfile |
|---|---:|---:|---:|
| 起点：着地済alias、literal、runtime return、`Result` error | 15,643 | 724 | 453 |
| + runtime内部を境界まで（`fan`、`fs`、`value`、`json`、`map`、`list`、`error`、`process_core`） | 959 | 681 | 187 |
| + 構築箇所：String copyに `to_almide()`、補間に `AlmideString::from(format!(..))`、error payload、`ok_or`、decode frame key | 115 | 96 | 18 |
| + 接頭辞なしruntime return（`almide_regex_*`、`almide_json_*`、`ProcessStatus` field） | 61 | 44 | 14 |

残り61errorすべてthread境界。`AlmideStr` は `Send`/`Sync` でなく、`fan` block、`fan.map_par`、
`fs.fold_lines_chunked`、top-level `let`（`static LazyLock`）に達する。
構築箇所でなく設計点3（Koka thread-shared flip）。

| + 設計点3：全thread境界でthread-shared mark | 0 | 0 | 0 |

**設計点3（thread境界）。** Kokaのthread-shared flip
（`kklib.h`：負countはatomic、task開始前に `kk_block_mark_shared`）。
- `AlmideStr` countは `ALMIDE_STR_SHARED` bitを持つ。marked blockはatomic read-modify-writeで計数し、
  最後のdropは `Release` と `Acquire` fenceを組み合わせる。unmarked blockはplain moveに相当する
  `Relaxed` load/store。unique検査は `Acquire` load。
- `almide_str_mark_reachable(&v)` が `v` の到達先全部をmark。
  marking window（各cloneがbitを立てるglobal counter）内で `v` をcloneする。
  型別walkerは不要：Koka walkが全blockを覆うようにAlmide全値は `Clone`。
- compilerがemitするすべての境界だけでmark：
  - threaded `fan` blockのarm自由変数。
  - シグネチャが `Send` をboundするruntime helperへの引数自由変数
    （シグネチャ生成の `runtime_fn_modes::crosses_threads`）。
  - helperがworkerへ渡すdata（`fan.map_par` item、`list.par_*`、`fold_lines_chunked` init）。
  - `static LazyLock` が全threadに渡すtop-level `let` 値。
- `unsafe impl Send / Sync for AlmideStr` はこのmarkingに依存。unmarked blockは2threadから到達しない。
- 境界を越える値だけatomic化。marking window中に他threadがcloneしてもそのblockをmarkし、速度コストだけ。
- flag offでは `almide_share()` は空default method。runtime callは無になり、compilerはmarkをemitしない。

**flag onでcorpus実行。** 同じ463fileをopt-level 0で `rustc --test` buildしnative実行：
flag on438pass、off452pass。差は14thread境界file。
設計点3後（`spec/stdlib/thread_shared_string_test.almd` 追加で464file）、両方453pass、非pass11fileも同じ。
rustls / flate2の10fileはstandalone rustcでlink不可。
残る深再帰testはこのharnessのopt-level 0既定test-thread stackを両側でoverflow。

**Step 5（`let t = s; s = s + "x"`）。** `spec/lang/alias_cow_test.almd` が直線とloopで固定し、
wasmとnative flag on/offでpass。共有buffer下のwalker `+` は `AlmideStr::concat`、
v1 append windowの `s.push_str(x)` は `AlmideStr::push_str` を通る。
両者unique viewを取り、共有blockは書込前にcopy。

**v1 rendererとhttpが後続。**
- v1 headerもflag下で同blockをsplice（`crates/almide-mir/src/render_native_str.rs`）。string shimとliteralは `AlmideString` を構築。
- `http.rs` とSSE clientは `AlmideString` を返し、header map/chunk callbackも同型で受ける。
- 全数調査では両者を見られない：v1は `--target rust` 経路でなく、httpはrustlsが必要。
  - http：除外10fileをflag onで `almide build`（rustls/flate2付きcargo経路）し、10件全compile。
  - v1：まだcorpus測定なし。append windowと全string shimを通る1programはflag onでv1 renderし、offと同byte出力。
  - **v1全数調査。** `probe_native`（example binaryのv1 renderer）を543programへ実行：
    `spec/`、`examples/`、`research/benchmark/perf`、`exercises`。
    - v1 renderは7件のみ。他536は `env.args` 等でwalkerへwall。
    - flag onで7件全て型検査と実行を通り、stdout/exit codeはoffと同じ。

**未被覆：**
- 新atomic経路（`crates/almide-str/tests/almide_str_threads_test.rs`）のMiriはCI（`miri-almide-str`）のみ。
  この機械にMiri用rustup toolchainなし。
- 既定flip（step 7）はGCE A/B待ち。
- flag下の速度：`&str + AlmideStr` とv1 shim resultが必要より1回多くcopy、`to_almide()` もcount増分で足りる箇所でcopy。
  step 7の `gce_ab.sh` で判定。

**Step 6（`AlmideValue::Str`）。** payloadを `Str(AlmideString)` にしたため、flag下のValueは
構築元string blockを共有。ADR-0018は `AlmideStr` に○を裁定。
- object keyは `AlmideKey`（intern済み `Cow<'static, str>`）のまま。
- runtime9行の表記変更、aliasが `String` の間は各々恒等。
- flag offで生成user codeは全463調査fileでbyte同一（`RT_BOUNDARY_MARKER` 後を比較）。on/offともerror0維持。
- `spec/stdlib/value_string_payload_test.almd` がpayload経路を固定：`value.str`、sourceへのpush、parse/stringify、
  derive Codec round-trip、多数Valueが1string共有。wasmとnative on/offでpass。
- native corpus（465file）：on/offとも454pass、同じ11非pass。

**Step 7（flip、2026-10-17）。** 反証条件は最初失敗：GCE
（c4-standard-4、Xeon 8581C、`gce-perf-ab.sh`、3交互round×5run、中央値）でstrchurnは
`String` 0.478s、共有buffer0.675s、41%遅化。fasta不変。原因と修正：
- **全string resultを二度構築。** runtimeが `String` を作り変換（`.into()` / `into_almide()`）し、
  文字列ごと追加確保/copy/free。`int.to_string`、`string.join`（30 MB copy）、
  `string.split`（4M piecesと追加 `Vec`）は今やexact-size blockを一度構築
  （`AlmideStr::from_i64`、`AlmideStr::join`、pieceごとの `AlmideString::from`）。
  localでは命令数が `String` 比1.72倍→1.04倍。
- **24byte headerが短いstringを1 allocator size class上へ。** glibcで7byte stringは48byte chunk、
  `String` bytesなら32byte。headerを `[rc: u32, spare: u32, len: usize]` の16byteへ。
  capacityは `len + spare`、制限されるのはspareだけ（`u32::MAX` まで。
  確保/成長をclampし、超えて終わるunique viewはblockを縮小）。GCE3-way profile：
  `String` 0.4545s、24byte header0.4855s（+6.8%）、16byte0.3925s（−13.6%）。
- release runtime rlibが黙ってbuild失敗（`pub #[inline] fn`）し、全共有buffer release buildがcargoへfallback。
  attribute順を今はunit testで固定。
- unique blockはdrop fast pathで、他の検査より先にfree。

最終反証判定（GCE A=`adr0013-stage2` の `String`、B=`a13-perf`）：
strchurn0.4565s→0.3925s（**−14.0%**、almide/rust比1.117→0.953）、
fasta3.177s→3.186s（+0.27%、ばらつき0.77%）。双方5%未満。

その後switch削除（`String` 経路は残らない）：
- `prelude_string.rs` は共有preludeのみ。`almide_into_str_impls!` gate、env-switch行、その `docs/specs/cli.md` 行を削除。
  thread境界markとlazy-static markは無条件、v1 headerは常にblockをsplice。
- **`@extern(rust)` hostは文書通り `String` シグネチャを維持**（docs/specs/module-system.md §11.1）。
  wrapperが境界変換：List / Set / Map / Option / Result / tuple内にStringを持つ値は入口で `String` 形へcopy
  （`mut` ならcall後copy back）、戻り値は `AlmideString` 形へcopy。
  hostが `String` を返しても既に `AlmideString` でも同じ。plain `String` paramは `&str` のまま。
  `ret_string`、`ret_list_str`、`ret_opt_str`、`mut_str`、`mut_list_str` 行が固定。
- **`String` keyのMap / set hash。** `almide_rt_map_key_hash` は `TypeId` で `String`
  （および `(String, _)`、`Vec<String>`）keyを認識するので、`AlmideString` keyは線形経路へ落ちた。
  500,000stringの `list.unique` は27s、`map.update` rebindは各stepでcopy。
  downcastは今や `AlmideString` を指す。
- v1 rendererはborrowed stringから `.to_string()`（`String`）でowned copyを作っていたが、
  今は `AlmideString::from(..)`。`int.to_string` / concat shimも1block構築。
- `UniqueStr` を `AlmideUniqueStr`（bare top-level item名）へ、`FromIterator` はfull path表記
  （v1 renderはedition 2015でもcompile）。
- 補間は `AlmideString::from_fmt(format_args!(..))`（stack buffer、exact block一つ）。
  `+` 左のliteralはowned右辺へprepend（`AlmideStr::prepend`）。native確保台帳は減少のみ：
  str389→349、strs822→708、map569→524、rec473→378、recs766→686。list/set/bytes不変。

未完：多くのruntime関数はまだ `let __almide_out: String = ..; __almide_out.into()` を返し、
結果ごと2回目の確保がある。各々 `int.to_string` 同種の局所修正。

**runtime ABI手順は検査stampを失効させる。** runtime関数textが変われば実装hashが変わり、
`tests/stdlib_inspection_test.rs` が再検査を要求。`-> String` 手順だけで38stamp関数が失効。
そのためABI手順（return、`Result`、`Vec`、`Option`）を1branchに集め、提案前にGCEで一度再検査する。
