歴史的説明：関数ごとの所有権証明書形式 — i/a/d/m のアルファベット、呼び出し・分岐・閉包の拡張、および出荷済みの構成要素。
<a id="certificate-format-v1--design"></a>

# 証明書形式 v1 — 設計

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](certificate-format-v1.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](certificate-format-v1.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](certificate-format-v1.zh-CN.md)

> 非規範的な固定原文の歴史的設計・実装台帳読者版。原本不変、自動同期・独立した証明検査/corpus実行/benchmarkなし。冒頭のOCaml抽出構成、退役MODE案、初期subset一意性案、guard付きreuseへの精緻化、7月追記を履歴の別層として保持し、一つの現行設計に書き換えない。brick完了は任意の生成関数の証明ではない。原文のrenderer/生成drop/assembler・engine/kernelへの信頼境界は残り、kernel-as-oracleはwitnessの性質を検証するが関数全体のbyte対応を閉じない。定理・テスト件数と性能は各段階当時の報告。証明書や資格化対象の記述は第三者認証の主張ではない。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/certificate-format-v1.md)

> 状態：**設計承認済み、brick 1–2 と 5 は出荷済み**（所有権アルファベット i/a/d/m、呼び出し 2a/2b/2c — effect 呼び出し、呼び出し位置ごとの cap、param-mode シグネチャと manifest 宣言 cap。full mode 5a/5b/5c — 分岐一致 `{…|…}`、`b` 文字、閉包 dispatch シグネチャとキャプチャ閉包）。3a/3b/3c + 4a/4b も各 brick 内で出荷済み（3c = ツリー内 raw-byte ⟶ ISA decoder、WasmDecode.v）。**brick 6 も出荷済み**（kernel-as-oracle gate — verdict の信頼根から extraction を退役）。**v1 brick ラダーは完了した。** heap capture を持つ閉包 env は 2026-07-10 に出荷済み（5c 参照）。その借用 env load は `b` ではなく call-mode/capture の会計で被覆する。残るラチェット（記録済み、brick ではない）：rc 断片を越える A2（関数全体の本体は renderer 契約のまま）、toolchain が揃った時点で fast path に verified-extraction/CertiRocq を採用すること。暗黙の「オブジェクトごとの i/d」形式を置き換える。先行研究と敵対的設計検討（Perceus/Koka reuse、linear/foundational PCC、WasmCert-Coq の byte 意味論、v0 の所有権分類）に基づく。

<a id="why-this-exists"></a>

## なぜ必要か

v1 の信頼に関する論旨：**コンパイラではなく CHECKER を証明する**。信頼しないコンパイラがビルドごとの *witness* を出し、kernel で証明された小さな Rocq checker を OCaml に抽出して再検証する — `accept(witness, artifact) ⟹ P(artifact)`。中核の訴求点は、資格化の対象（checker）が監査者の読める**数百行**であること。

v0 witness（`ownership_certificate`：オブジェクトごとの `i`/`d` ストリームを `check_all` で釣り合わせる）は「括弧が釣り合う」としか言えない。先にある 3 つの gap は、それ以上を必要とする。

- **G1 witness ⟹ wasm bytes** — witness を実際に出力した bytes と runtime heap machine（`__alloc`/`__rc_dec`/free-list）に結びつける。
- **G2 frees（real-RC）renderer** — SHARING（fresh / alias / move）、reuse、leak-freedom を表す。i/d ストリームは粗すぎる。
- **G3 slice → 言語全体** — 呼び出し、制御フロー、閉包、入れ子/再帰的 heap。

<a id="the-central-tension-and-the-principle-that-resolves-it"></a>

## 中心的な緊張と、それを解く原則

witness が 3 つすべてを表せるほど豊かになると、素朴な checker も一緒に大きくなり、数百行の core は崩れる。解決は次のとおり。

> **witness の表現力を制限するのではなく、checker が決定する事柄を制限して小さく保つ。** witness を「検査可能な形へ詳細化」する。すべての大域的決定（所有権推論、分岐の調停、reuse 安全性、byte layout）を、信頼しないコンパイラがあらかじめ局所的な具体的事実へ解決し、checker は固定規則で検査する。checker は推論しない、CFG を辿らない、callee を開かない。

したがって checker のサイズは**規則数**（イベント文字 + subset のインスタンス + op→wasm パターン表の項目数）に比例し、プログラムサイズ、言語の複雑さ、コンパイラの複雑さには比例しない。

<a id="the-format-flat-stream--side-table-ground-facts-only"></a>

## 形式：flat-stream + side-table、具体的事実のみ

ビルドごとの witness は、プログラム全体の 1 つの bundle であり、改行/`|` 区切りの平坦なテキスト節からなる。すべて具体的事実（自然数、`|` リスト、固定アルファベット）で、**Coq 項、高階構造、「X を推論せよ」という指示を埋め込む節はない**。各節は Rocq で証明済みの 2 つの parser 形（`parse_go` 行 fold、`parse_pair` の `|` 分割）で解析する。checker は各節 verdict の AND と、少数の節間*一致*検査からなる。

<a id="1-ownership-stream--the-v0-core-with-a-richer-alphabet"></a>

### 1. 所有権ストリーム — アルファベットを豊かにした v0 core

参照カウントされる OBJECT ごとに 1 行、イベントごとに 1 文字。各文字は既存 fold が処理する符号付き delta を持ち、*文字自体*がコンパイラの決定済み具体的事実である。

| 文字 | δ | 具体的事実 | gap |
|---|---|---|---|
| `i` | +1 | FRESH 取得（Alloc / fresh Dup / owned Call-result） | — |
| `a` | +1 | ALIAS 取得（既存 SHARED ref を inc）— share-vs-move | G2.1 |
| `d` | −1 | 通常の解放（Drop） | — |
| `m` | −1 | MOVE-OUT（Consume → container / return / consuming callee） | move≠drop |
| `r` | −1 | REUSE 可能な解放（一意性を証明した drop） | G2.2 |
| `b` | 0 | 生存する所有参照の借用（カウント不変、`rc > 0` が必要） | G2.3 |

checker の行ごとの規則は `proofs/OwnershipChecker.v` のガード付き fold：`i|a ⇒ +1`、`d|m ⇒ (if rc≤0 reject else −1)`、`r ⇒ (if rc=1 then 0 else reject)`、`b ⇒ (if rc≤0 reject else rc)`。`0` から始まり、fault なしで `0` に終わる実行だけを受理する。Alias と MoveOut は増減の釣り合い規則を再利用する。Reuse はさらに一意性（`check_reuse_sound`）、Borrow は生存性を検査する。借用 param/env load はこのイベントではなく call mode で会計する。どちらも無制約の delta-only イベントではない。**v0 の `i`/`d` は退化ケースである。**

~~bundle の先頭に `MODE` byte（`eager` | `perceus` | `full`）を付け、合法な文字を選択する~~ — **退役（brick 5b の決定、2026-07-08）**。アルファベット自体が自己ガードを持つと分かった。`r` の義務は fold 自身が果たす（rc = 1 のときに限り有効 — `check_reuse_sound`）。`b` は自身の生存性ガードを持つ（rc = 0 で fault）。よって witness は構造上「義務が存在する前に reuse を主張」できない。義務は bundle ごとでなくイベントごとに強制する。MODE 接頭辞は checker が再導出できないコンパイラの自己申告となり、具体的事実の原則に反する。また配備レベルの文字ポリシー（「この運用者は perceus mode を禁止する」）は運用者の関心であり、witness bytes でなく manifest 層（`[permissions]` 風）に属する。

<a id="2-side-table-sections--all-the-one-subset-law-subsetv"></a>

### 2. Side-table 節 — すべて同じ 1 つの subset 法則（Subset.v）

`subset_check sup sub = forallb (fun x => mem x sup) sub` をいくつかの形で具体化する。**1 つの証明（`subset_check_sound`）が全体を被覆し、性質の追加は名前付けの追加であって証明の追加ではない**。

- `used ⊆ defined` — 名前の全域性。
- `used-caps ⊆ allowed` — capability の上限。
- **`actual-call-modes ⊆ declared-param-modes`** — 呼び出し位置とシグネチャ（下記）。
- `r-objects ⊆ proven-unique-at-drop` — Perceus reuse の健全性（G2.2）。
- `used-field-offsets ⊆ declared-heap-offsets` — byte layout（G1）。

<a id="3-call-signatures--compositionality-the-whole-language-lever"></a>

### 3. 呼び出しシグネチャ — 合成性（言語全体へのてこ）

各関数は SIGNATURE 行を持つ。param ごとの mode（move/borrow）、return mode（move-out/borrow）、宣言 effects/caps。呼び出し位置 `call fn_id : actuals` では checker が `fn_id` のシグネチャを検索し、subset 規則で `actuals` を検査する。**callee は決して開かない。** N 回の呼び出し検査は O(N) の membership で、callee サイズに依存しない。閉包はキャプチャ env の所有権をシグネチャに含む値である。これにより call / closure / recursion を「1 規則 + 1 種類のシグネチャ」で載せ、checker を平坦に保てる。→ G3.1/G3.3。

<a id="4-byte-binding--table-now-semantics-once-g1"></a>

### 4. Byte-binding — 今は表、意味論は一度だけ（G1）

op ごとに固定 `op → wasm-instruction-pattern` 変換表への tag を持ち、layout 定数節（`rc_offset`、`free_list_offset`、`heap_ptr_global`）を添える。ビルドごとの検査は出力 bytes と表の**構文的**照合（小さな checker 内で安価）。**意味論的**な半分 — runtime 関数（`__alloc`/`__rc_dec`/free-list）が抽象 ±1 op を精緻化すること — は wasm memory-model library に対して一度だけ証明する（重いが償却でき、WasmCert-Coq が必要）。`R(M,w)` = 「M の bytes が w の op の表に一致する」、`V'` がビルドごとの表照合器。→ 毎回 runtime を再証明せず、G1.2 の「Dec は抽象 token」を閉じる。

<a id="the-checker-size-invariant--and-the-tripwire-to-guard"></a>

## Checker サイズ不変条件 — 守るべき警戒線

> **不変条件。** checker の全規則は、(a) 1 行の符号付き delta の left-fold、(b) 具体的事実の `|` リスト上の `mem`/`subset` 検索、(c) 固定変換表との構文的 pattern-match、のいずれか。サイズ ∝ 文字数 + subset 具体化数 + 表項目数。

> **警戒線（core が死ぬ瞬間）。** checker が callee を開く、CFG/制御フローを辿る、meta-variable を解く/推論することを禁止する。規則がそのどれかを必要とした瞬間、大域的決定が trusted base に漏れたことになる。コンパイラ出力の具体的事実へ押し戻す。

これが「整える勝負」の正確な形である。表現力は節を追加することで得る（各節が固有の内部化 parser と、共有 Subset/Balance 法則を再利用する健全性補題を持つ）。規則を賢くすることでは得ない。

<a id="migration-from-id"></a>

## i/d からの移行

`i`/`d` は v1 アルファベットの**退化したインスタンス**。`OwnershipChecker.v` は balance 検査アーキテクチャを保ち、新イベントで `exec` と parser を拡張する。Alias/MoveOut は旧 delta 意味論を再利用し、Reuse/Borrow は一意性/生存性ガードと対応する証明ケースを追加する（下記 brick 4 と 5b）。元の `i`/`d` ストリームは拡張アルファベットでも有効なまま。

<a id="build-order-each-step-gate-green-dual-oracle-ratchet-holds"></a>

## 構築順（各段階 gate-green、dual-oracle ラチェット維持）

1. **所有権アルファベット（eager: i/a/d/m）。** ✅ **出荷済み。** `a`（alias）と `m`（move-out）は具体的事実。`exec` は 4 分岐 fold。`eager_copy_refines_safety` を「Dec-free」から「increment-only」へ一般化（MoveOut も −1）。`return_list.almd` witness は `id → im`。proof spine + gate + 31 テスト + CI green。
2. **呼び出し。**
   - **2a：effect 呼び出し。** ✅ **出荷済み。** `lower.rs` は `println(s)` を生存する文字列を借用する `Op::Call{PrintStr}` へ lower する。本物の出力プログラムが PCC 連鎖を通り（所有権 `id` ACCEPT）、**capability witness は実ソースから来る**（`used=[Stdout]`）。未宣言なので cap bound が REJECT（`|0`）する。sandbox の約束が実際の host effect を捕捉する。
   - **2b：呼び出し位置ごとの capability subset 規則。** ✅ **出荷済み。** `lower.rs` がユーザー呼び出し `beep()` を `Op::CallFn` へ lower。コンパイラは各 callee の到達可能 cap を caller に fold する（`reachable_caps`、call graph を推移的に辿る）。証明済み `check_caps_cert` が `reachable ⊆ declared` を再検査するので、直接効果を持たない `main` でも `beep` 経由で到達する Stdout により REJECT される（`tcaps` witness `|0`）。checker は callee を開かず subset だけを行う（Subset.v のてこ、新 Coq はゼロ）。direct-only cap の gap を閉じる。正直な範囲：コンパイラの reachability fold はビルドごとに信頼している（edge ごとの検証と、未知 callee を保守的に reject することが強化策）。
   - **2c：所有権 param-mode と manifest 宣言 cap。** ✅ **出荷済み。** (i) `proofs/CallModes.v` — SIGNATURE 節（§3）。関数ごとの宣言 heap-param mode（borrow | move を自然数で表す）と呼び出し位置ごとの actual mode。`check_modes_cert` は各位置の位置対応を検査する（未知 callee は保守的に reject、各位置で自然数リストの等価検査を 1 回行い、checker サイズ不変条件を保つ）。合成法則 `check_fill_sound` が健全性の根拠。受理された caller ストリーム（call site を宣言 mode marker に縮約：borrow = イベントなし、move = `m`）は、宣言 mode を満たす callee ストリーム（`param_stream_ok`、Reuse-free + shift-invariant）をどうインライン展開しても double-free/leak-free のまま。したがって checker が callee を開かずに関数ごとの cert が合成する。空虚でないこと：caller は borrow、callee は move と考える組合せ（各々単独では balanced、inline 後 double-free）こそ、一致検査が禁じるもの（`disagreement_double_frees`）。Emitter は `call_modes_witness`（certificate.rs。ドット付き self-host callee は renderer 契約に従って省略し、caps graph と同じ既知クラス方針）。gate 行は手構築の agree/mismatch と実ソース `two_functions` / `heap_arg_call`（実際に heap 引数を渡す）。両定理は axiom-clean、coqchk 済み、台帳登録済み。当時の lowering は borrow-only なので出力 mode はすべて `0`。消費する move 規約が入るときはシグネチャと site を同時に切り替えないと build を reject する。その一致は現在 machine-check されている。(ii) Manifest 宣言 cap — ACCEPT ケース：`apply_manifest_caps` は `effect fn` の宣言上限を運用者の `almide.toml [permissions].allow` に精緻化する（`manifest_caps` で Capability registry に射影、pure fn は ∅ を保つ。manifest は effect system が拒否する権限を与えられない）。gate：同じ実出力プログラムが `allow=["IO"]` では ACCEPT、`allow=["Rand"]` では REJECT。初の空虚でない cap bound（旧 all-caps effect 既定値は effect fn を reject できなかった）。2c より後の精緻化として残るもの：シグネチャの return-mode 文字（当時は owned-heap-result 規約が統一）、閉包シグネチャ（brick 5）、細粒度 `FS.read` 風 manifest 語彙（effect-system-capability Phase 1）。
3. **Byte-binding：op→wasm パターン表。**
   - **3a：表 + ビルドごとの照合器。** ✅ **出荷済み。** `proofs/Translation.v` は op→wasm-instruction 表を Coq object として形式化する（形式的 byte-binding `R(M,w)` — G1.1/G1.3/G1.4 を閉じる）。ALS を再利用し eager-mode 安全性インスタンスを証明。`translation_validation.rs::validate_translation` は実 WAT で、すべての op の pattern が存在し、かつ `rc_dec` がないことを build ごとに再検査する。単なる Dec-free scan より厳しく、op を落とす renderer を捕まえる。正直な範囲：存在検査であり、op ごとの正確な byte-window 全単射ではない。意味論的実現（runtime memory machine：`call $rc_dec` が抽象 −1 として free-list を変える）は runtime-memory-model + WasmCert-Coq library、つまり**最難関の G1.2**（3b）。
   - **3b：runtime memory model（抽象 memory 側）。** ✅ **出荷済み。** `proofs/RuntimeModel.v` は runtime heap を linear-memory 状態機械としてモデル化する。refcount は `base + RC_OFFSET` の CELL にあり、`rt_inc`/`rt_dec` は具体的 memory write。`mrun_tracks_exec` を証明し、cell は抽象 refcount（`OwnershipChecker.exec`）と全く同様に進み、fault も一致する。系 `balanced_cert_no_memory_fault`：受理 cert（rc 0 から balanced）は memory で double-free しない機械により実現される。抽象 Dec は浮遊 token でなく具体的 memory 操作に結びついた（両定理 axiom-clean、coqchk 検証済み）。
   - **3c：raw-byte ⟶ ISA 結合（G1.2 最後の区間、ツリー内）。** ✅ **出荷済み（2026-07-09、`proofs/WasmDecode.v`）。** rc opcode 部分集合の証明済み DECODER `decode : bytes → option (list instr)`（未知 opcode / malformed memarg / 非ゼロ global index / `end` 欠落 ⇒ None、保守的 reject）が実 assembler bytes を WasmIsa の関係的 small-step 意味論に結びつける。`decode rc_inc_bytes` と `decode rc_dec_real_bytes`（両方とも check-wasm-bytes.sh が build ごとに wat2wasm に接地し、renderer の実形状、`p + 0` 加算も含む）は、すべての `irun` 簡約が証明済み効果を持つプログラムを返す。`rc_inc_bytes_isa_effect`（cell +1）、`rc_dec_bytes_isa_traps`（double-free は byte レベルの仕様で STUCK）、`rc_dec_bytes_isa_frees`（一意な解放は cell をゼロにし $freelist につなぐ）。連鎖：実 bytes ⟹（証明済み decode）ISA program ⟹（証明済み irun）memory effect。正直な残余（記録済み、WasmCert-Coq 自身と同類）：wasm ENGINE の ISA 仕様への忠実性（check-wasm-exec.sh 内 wasmtime で build ごとに照合）、assembler への接地、kernel。rc primitive 以外の関数全体は §3 renderer 契約のまま。WasmCert-Coq 全体 IMPORT はツリー内では依然不可能（Rocq 9 ではなく旧 Coq 向け）。ツリー内 ISA + decoder が承認された構成（WasmIsa.v header）。
4. **perceus mode（`r`）+ leak-freedom。**
   - **4a：`r` イベント + memory レベルの leak-freedom。** ✅ **出荷済み。** `OwnershipChecker.v` に `Reuse`（`r`）— reuse 可能な解放（perceus mode）— を追加。`−1` として fold するため `check_sound` をそのまま再利用（`a`/`m` 同様、新証明ゼロ）。`RuntimeModel.v` の `balanced_cert_frees_in_memory` は、受理 cert が runtime cell を **0 — leak でなく FREED** にすることを証明。したがって witness レベルで `check_sound` の `no_leak` が証明済みの leak-freedom（`check` の「0 に終わる」側）が実際の解放 MEMORY に結びついた。これは eager-copy renderer（release を出さない）が達成できず、release を出す renderer が実現する性質。抽出 checker は `r` を端から端まで処理する（`build-checker.sh` perceus demo）。**完了（4b）reuse の健全性 — `check_reuse_sound`**：subset 節ではなく（それでは checker が再導出できないコンパイラ自己申告「proven-unique」集合を信頼してしまう）、一意性を FOLD で果たす。`exec` の `Reuse` 分岐を `rc = 1` のときのみ有効とし、SHARED object（rc > 1）の Reuse は FAULT。checker は自身の count から一意性を導く。より単純で、厳密に健全。`RuntimeModel.step_mem`/`Termination.fuel_exec` を lockstep に保つ（`rt_reuse`）。閉じた穴 `iard`（shared object を reuse する balanced cert）は Coq（`cert_shared_reuse_rejects`）と抽出 checker（`build-checker.sh shared_reuse.cert`）の両方で REJECT。**完了（A1.2 証明基盤）— `FreeList.alloc_not_live`**：free-list allocator（bump + free-set + ghost live-set）をモデル化し REUSE-SAFE と証明。有効な割当（fresh frontier、または free-list の block）は現在 LIVE の block を返さない（no reuse-after-free）。alloc/free を通る INV 保存で全実行に持ち上げる。これで A1.2 の分岐は PROVE へ解決。physical free-list を出す renderer slice は trusted runtime を追加せず、このモデルを精緻化する。**完了（A2 最初の slice）— `WasmRcDec.rc_dec_prog_realizes_rt_dec`**：renderer が出す正確な `$rc_dec` 命令木を RuntimeModel の `Mem` 上の小さな op 意味論（load/add/sub/store/trap）付きデータとしてモデル化し、`rt_dec` を計算することを証明。同じ trap、同じ減算。解放の意味論は命令木レベルで証明済み（残る A2 gap は raw-byte ENCODING、assembler / full WasmCert-Coq ISA のみ）。proof spine = **25 定理 axiom-clean**。**完了（A1.3 cow 安全性）— `CowSafety.make_unique_yields_unique`**：clone-on-shared 規律をモデル化し一意所有 block を生むと証明するため、その場変更は alias を壊さない（aliased-mutation クラスが生じない）。sharing renderer の cow はこれを精緻化。proof spine = **27 定理 axiom-clean**、A1 の安全性クラス（leak / double-free / reuse-soundness / reuse-after-free / aliased-mutation）はすべて証明済み。**完了（A1.2-render）— 物理回収を実現**：rc→0 で `$rc_dec` が block を free-list に返し、`$alloc` は exact-size head を再利用して `FreeList` を精緻化。double-free sentinel は保持（link は rc cell でなく dead len field）、wasmtime で検証（reuse `p1==p2` + double-free trap + value-semantics bytes 不変）。**完了（A1.3-render）— SHARING + cow を実現**：`Dup` は `rc_inc` で share（copy なし）、`MakeUnique` は cow（clone-on-shared、`rc_dec` を先にして temp なし）で `CowSafety` を精緻化。rc cell が抽象 refcount（1→2→1→0）を実際に追跡し、証明済み rc machine を使い、出力 bytes 不変。**A1 は real-RC sharing、cow、free-list 回収、sentinel を実装する。** モデル化した RC routine には refinement proof があるが、生成 drop routine、関数全体 lowering、renderer への信頼は消えない。現行 C-PROVEN/C-FAITHFUL 境界は `proofs/receipt.sh` を参照。残り（性能/encoding、安全性ではない）：A1.2 size-class/走査（当時は exact-size head-match）、A2 raw-byte encoding。`rc_dec`/`rc_inc` は完了（A1.1b / A1.3-render）。
5. **full mode：分岐一致 + `b` + 閉包シグネチャ。** ✅ **出荷済み（2026-07-08）。**
   - **5a：分岐の資源状態一致（`CBranch`、format v4）。** `OwnershipChecker.v`：`CertItem` に `CBranch thenb elseb` を追加。両 flat arm 本体が入口 count から同じ結果へ fault なしで進む場合に限り受理（一致であって保存ではない。net +1 arm は heap-result 分岐）。`UL_branch` は runtime が実行する片方の arm に展開され、主定理 `check_line_unroll_sound` は不変。parser `parse_bc`（format v4、`{ then | else }`）は `parse_clc` の上位集合で、全 flat/CLoop/CCondLoop cert が byte-identical に解析される。Emitter は `IfThen`/`Else`/`EndIf` region ごとに arm イベントを buffer。各 arm が自己釣り合いするものは FLAT 出力（byte-identical、全既存 cert に変更なし）、そうでなければ `{then|else}` にまとめる。これにより CROSS-ARM COMPENSATION（片方の `i` を他方の `d` で「釣り合わせる」、経路依存 leak/double-free、旧 flat は ACCEPT）が構造的に拒否され、lowering の arm ごとの balance は信頼する慣行でなくなった。`verify_ownership` に同じ分岐 JOIN（arm ごとの入口状態 snapshot + object ごとの一致 + `BranchDisagreement`）を追加し、両 arm を平坦に fold する処理を退役。**グルーピングの初回適用で実際の leak が見つかった**：effect-fn 末尾 `if err then err(msg) else ok(total)` は error path で error accumulator を Err block に move するが、ok path では一切解放しなかった（`{m|}`、happy-path 呼び出しごとに空文字列 block が 1 つ leak、flat cert は `m` を無条件に数えていた）。根本修正：`lower_heap_result_if_inner` は RELEASE PARITY を強制し、一方が move-out する外側 handle を sibling arm で補償 `Drop`（`{m|d}`、両 arm 一致）。spec 280/280、corpus 26,976 objects ACCEPT。
   - **5b：`b`（borrow、+0）文字。** `exec` に `Borrow` を追加。delta なし、生存性ガード（rc ≤ 0 で fault）。所有 object の use-after-free を witness で表せる（`idb` は REJECT。以前は `Op::Borrow`/`MakeUnique` が cert から見えなかった）。op ごとの全補題を拡張（`exec_app`、`reuses_unique`、Termination の fuel interpreter、ALS increments-only、RuntimeModel memory machine の `rt_borrow` — live-read が抽象意味論と同じように fault —、CallModes の `exec_shift`/`exec_fill`。`b` は shift-safe なので call 境界で合成する）。Emitter：所有権を持つ（+1 がある）stream の object に対する `Op::Borrow`/`Op::MakeUnique` で `b` を出す。借用 param の直接利用はイベントなしのまま。その生存性は CALLER の義務であり mode 一致が果たす（理由は `bare_borrow_body_needs_own_ref` に記録。ゼロ初期化 param stream の `b` は fault）。500-seed differential floor が実際に `b` を試す。MODE-byte 問題は解決、退役（§1。アルファベットは自己ガードし、ポリシーは manifest 層）。
   - **5c：閉包シグネチャ（G3.1/G3.3、検査可能 slice）。** `call_modes_witness` は `Op::CallIndirect` を飛ばさなくなった。funcref 値は `Op::FuncRef` からしか来ないため、site の POSSIBLE-CALLEE 集合は dispatch の param 形（arity + 位置ごとの heapness）に合う program FuncRef table の target。それ以外は `call_indirect` 型 gate で trap し fail-stop。witness は possible callee ごとに一致行を 1 本出すので、証明済み `forallb site_ok`（`check_modes_cert_sound`）がそのまま集合上の Forall lift となり、新 Coq 表面はゼロ。空/不可視集合（lower されなかった FuncRef target）は範囲外 sentinel で保守的 REJECT。実ソース gate 行：`funcref_call.almd`（返された lifted funcref を CallIndirect で dispatch。lifted `__lambda_*` 補助関数も witness program に含む）。**後続出荷（2026-07-09）：キャプチャ閉包。** 第一級関数値は一律 CLOSURE BLOCK — heap 上の `[rc][len][cap][fnidx][captured…]` DynList。call は slot 0 の fnidx を load し、block を先頭の借用 env 引数で渡す。lifted lambda の prologue が capture を読み出す（`fn adder(n) =
     (x) => x + n` を返す/束縛/dispatch。v0-byte-identical、closure_capturing_wasm.almd。corpus in-profile 4,693 → 4,745、walls 317 → 306）。新 op / render 規則 / checker 規則はゼロ。block は通常の owned heap object（`i`/`d`/`m` cert）、env は通常の借用 heap param（CallModes 一致が閉包境界を被覆し、possible-callee 展開は env+args 形に照合）。**CLOSURE ENV FULL MODE は 2026-07-10 に出荷**：capture に HEAP 値（1 階層を正確に扱う種類 — String / List[Int] / List[Float]）と Fn 値（閉包を捕える閉包 — `compose`）を追加。heap capture は共同所有：作成時に `Dup` + move-in（caller object の cert `a`+`m`。CowSafety が share の値意味論安全性を支える）、prologue は借用 handle として読み出す（`LoadHandle`、param 規律）。block は自己記述的 — slot 0 fnidx、slot 1 = n_heap | n_closure<<16。統一された GENERATED-ALMIDE `$__drop_closure`（render pipeline が挿入し、他の生成 `$__drop_*` と同様 witness 表面外）がどの drop site でも閉包を解放する。capture した閉包は自己再帰、heap capture は各 1 回 rc_dec、fnidx slot は触れない。heap-result 閉包 call は call の引数位置でも lower する（`println(hi("world"))`）。v0-byte-identical（closure_capturing_heap_wasm）。gate 行 `closure_heap_capture.almd`（greeter `im|am` + 2-heap-arg dispatch mode 行）、kernel 一致。**`b` の消費者問題は解決**：lambda 内 env read はゼロ初期化借用 stream 経由の load で、5b param 規律によりイベントなし。したがって `b` の本質的 cert 消費者は OWNED stream の `Op::MakeUnique`/`Op::Borrow`（5b 出荷）であり、旧「closure-env borrow letter」という捉え方は退役。env 境界は `b` ではなく CallModes 一致 + 共同所有 capture 会計が担う。残るラチェット：入れ子 heap capture（List[String]/Value/variant env、型付き再帰 slot free が必要）、Float capture（f64↔i64 reinterpret prim）。
   - Gate：build-checker byte demo ×6（borrow live/uaf/nothing、branch agree/disagree/cross-arm）、gate.sh 29 行（新 8 行：branch A/R、borrow A/R、closure A/R、実 `heap_result_if.almd` + `funcref_call.almd`）、`check_bc_unroll_sound` axiom-clean + coqchk 済み + 台帳登録、corpus-wall 被覆不変（in-profile 4,693 / walled 317、黙って除外したものなし）、almide-mir 579、workspace + spec suite 緑。
6. **抽出への信頼を退役：KERNEL-AS-ORACLE gate。** ✅ **出荷済み（2026-07-09）。** build ごとの gate の VERDICT は未検証 OCaml extraction + `ocamlopt`（TRUSTED_BASE 項目 2、Thompson hole）に依存しなくなった。`gate.sh` の `kernel_verify` は各 witness の bytes をそのまま生成 assertion file に埋め込み、`coqc` にかける。Rocq KERNEL 自身が正確な bytes 上で証明済み checker（`vm_compute`）を行ごとに評価する（全 31 行 + manifest 2 行）。binary/kernel が乖離すれば build を落とす。`corpus-wall.sh` は corpus witness 全集合に batch oracle を実行（27k-object ownership cert に 1 回の `check_bc` fold + 4.7k names / 3.9k caps / 281 tcaps witness に `forallb`）。`vm_compute` 実測約 4 分、無条件接続（sampling なし）。改ざん訓練を毎 build 実施：(i) 壊した witness を binary と kernel がともに拒否、(ii) 模擬的な verdict 不一致を捕捉（oracle が実効性を持つ）。抽出 binary は fast path として残るが、照合される対象であり verdict の根ではない。SCOUT 記録（6a）：`rocq-verified-extraction`（MetaRocq、Rocq 9.1 package あり）と CertiRocq（CertiCoq → Rocq 9.1 port、2026）は upstream に存在する。採用は延期（local は opam なし source build 9.1.1、CI は opam 9.2。また両方とも fast path 基盤に compiler が残る：Malfunction→ocamlopt / C→CompCert）し fast-path ラチェットとして記録。verdict は kernel が担うため、どちらも verdict に必須でない。正直な残余：`vm_compute` の bytecode evaluator は kernel 自身の TCB の一部（従来どおり）。oracle が認証するのは WITNESS レベルの性質であり、rc primitive を越える wasm-byte 結合は §3 renderer 契約のまま。

<a id="open-risks-honest"></a>

## 未解決リスク（正直に）

- Witness SIZE は増える（build ごとの節が増える）。信頼性ではなく throughput の問題で、checker は小さいまま。`gate.sh` を遅くしないよう監視する。
- Brick 3b/3c は、記録された runtime-model/ISA と decoder slice をすでに供給しており、未構築の層ではない。未解決境界は、証明済み RC routine 以外の関数全体についての witness-to-byte 対応。元の WasmCert-Coq 統合は提案された道であり、任意の生成コードが現在意味論的に検証された証拠ではない。structural backend の現行測定/証明範囲は `proofs/receipt.sh` を参照。
- ~~分岐の資源状態一致（brick 5）は CFG 走査へ最も誘惑される箇所~~ — 警戒線内で出荷。`CBranch` 規則は平坦な arm fold 2 回 + 等価検査 1 回。CFG 走査も経路列挙もない（emitter が object ごとの arm イベントを具体的事実としてまとめ、入れ子 region arm は賢い規則でなく常に拒否する poison `{i|}` にフォールバックする）。
