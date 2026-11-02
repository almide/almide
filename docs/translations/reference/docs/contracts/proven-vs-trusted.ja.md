<a id="proven-vs-trusted--the-boundary-map"></a>

# 証明済みと信頼に委ねる部分 — 境界図

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](proven-vs-trusted.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](proven-vs-trusted.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](proven-vs-trusted.zh-CN.md)

> 非規範的な固定原文読者スナップショット。変更していない固定原本が正本で、自動同期や証明/gate/benchmarkの独立再実行ではない。model性質の証明、有限fixture検査、実装の信頼接続を区別する。formal-credit/DO333形式の記述は認証やtool qualification取得の主張ではない。廃止MIR pipelineと出荷structural byte pipelineは同一視せず、記録規律・未model化heap access・Rust/Lean compile・host動作の限界を保持する。時点付き数値/実装記述を今回独立に更新していない。原文は橋を3層と呼びながら4つの位置づけを列挙するが、黙って再番号付けせず保持。コマンドは再現方法の記録で、この翻訳で実行する指示ではない。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/contracts/proven-vs-trusted.md)

証明を付属するコンパイラについて監査者が最初に問うのは、*どの部分が証明済みか*である。本ページはそれに答え、各答えの証拠を示す。

toolchain pin、削減不能な信頼root、改ざん訓練、bug classごとのregression pinを含む全台帳は [`proofs/TRUSTED_BASE.md`](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/proofs/TRUSTED_BASE.md)。本ページが地図、そちらが実地である。

同境界をobjectiveに沿って記述したもの、すなわち各proof familyを仮定/coverage限界付きformal-credit主張として示したもの（#575、DO-333形式）は [`proofs/FORMAL-CREDIT.md`](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/proofs/FORMAL-CREDIT.md)。

<a id="the-one-sentence-version"></a>

## 一文でいうと

**受理されたcertificateは、そのcheckerがmodel化した性質を確立する**。witnessのcoverageと仮定の範囲内で、例えばRC balance、name totality、capability boundである。artifact全体のmemory safety証明ではなく、記録と未model化accessは信頼境界に残る。**loweringが正しい意味論を選んだかは何も証明しない**。certified functionでも間違った文字列をprintしうる。

<a id="which-leg-is-this-page-about"></a>

## どの経路についてのページか

v0.60.0（#1599）以降wasm経路は2つあった。下の境界図は旧v1経路が出荷していた**MIR certificate pipeline**を説明する。#2752以降その経路へ到達するrouteはなく、#2761がrendererを削除したため、**出荷wasm byteはこのpipeline由来ではない**。各行はいまproofs/gate.shがMIR上でwitnessする内容であり（#4011以降 `almide verify` は代わりにstructural bundleを判定）、実行するbyte上で再検査された性質ではない。Coq定理 `WasmEncode.v` / `WasmExec.v` / `WasmRcDec.v` は*model化された*runtime（廃止旧経路）の定理である。

`--target wasm` 経路、唯一のwasm rendererである `crates/almide-wasm` のstructural emitterは、emit時に独自certificateを記録する（#1696、完了）。出荷stock-WASI byteについてframeごとのownership witness、call mode、name totality、capability、および全 `xs[i]` / `b[i]` read/storeの `bounds` witness（#4005）を持つ。`proofs/structural-wall.sh` は各certified fixture bundleをextracted checker、Rocq kernel、`almide-verify` の3者一致で判定し、`almide build --target wasm` が書いたfileのSHA-256をpinする。下の行はこれには適用されない。信頼するのは記録eventがbyte内instructionであること（記録規律）とindex operator外の全heap access。`MemorySafe` の各節に対するproven / buildごとのchecked / trustedの分割は [docs/design/MEMORY-SAFETY.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/design/MEMORY-SAFETY.md#the-gap-table-2026-10-24-4011) のgap表にある。他の証拠は差分検査とCI gate: grow-onlyな全 `spec/wasm_cross` corpus上のnative/interpreterとのbyte同一性、emitterのsemantic mutation net、fixtureごとのalloc/size台帳、拒否ratchet（拒否形状は拒否のまま。黙って再emitするのは#1688 regression class）、structural runtime byteをCoq decoder model `StructuralDecode.v` に結び付ける `proofs/check-structural-bytes.sh`。

<a id="where-the-boundary-runs-the-mir-certificate-pipeline"></a>

## 境界の位置（MIR certificate pipeline）

| Stage | 状態 | 裏付け |
|---|---|---|
| lexer、parser | trusted | 差分fuzz、spec corpus |
| MIR witness checker（ownership、型具象化、name） | **model化性質について証明済み** | `proofs/OwnershipChecker.v`、`TypeConcretization.v`、`NameTotality.v`。frontend型checker全体の証明ではない |
| coown pattern | **証明済み** | `proofs/CoownCompose.v`、`CoownLoop.v`、`CowSafety.v` |
| AST → IR lowering | trusted | checkerの `TypeMap` が正本、974-file emit baseline |
| IR → MIR lowering | **trusted** | ← *F3（#777）が扱うgap* |
| MIR ownership witness | **再検査可能性が証明済み** | `proofs/gate.sh`: 非信頼producerがwitnessを出し、kernelで証明したcheckerが再検証 |
| `almide-verify`（binary配布で動くchecker、#2152） | trusted — 一致gate付きだが未証明 | compiler crateにlinkしない、独立version管理の5 Coq checkerのRust転写。`proofs/gate.sh` が全行とseed付きrandom差分でextracted checker判定に一致させ、`proofs/corpus-wall.sh` が全corpus witnessを検査 |
| MIR → wasm byte | 廃止（#2761） | 旧renderer削除。`WasmEncode.v` / `WasmExec.v` / `WasmRcDec.v` はそのmodel runtimeの定理。出荷structural byteは `proofs/check-structural-bytes.sh` と `StructuralDecode.v` で検査 |
| wasmtime | 未適格化tool | 構造上範囲外 |

<a id="why-the-ir--mir-row-is-the-one-that-matters"></a>

## IR → MIR行が重要な理由

2026-07-03監査で見つかった出力を壊す5bugは全てこの行にあり、5つとも*certificateに受理された*。MIRはRC balance/name解決可能性を満たしたが、別の計算をしていた。

trusted-base台帳から引用する共通形:

> 信頼領域内でproducerとconsumerの間のREGISTRYまたはCONVENTION（追跡集合、layout表、呼出規約、scalar deferred-Const fallback）がずれた。

ずれは*valid*なMIRを作るためcertificateでは検出できない。検出するのはlowering出力自身へのpost-passで、loweringが保つべき不変条件を検査する。`assert_names_resolvable` や `ConcretizeTypes` gateと同型である。そのgateは今ある（`crates/almide-mir/src/mir_wellformed.rs`、#777項目2）。emitした全関数のop streamでdef-before-useを検査し、全実opでdefines/readsの分割が `op_values` を分割することをassert（三種の出現関数のずれを防ぐ）。違反はrenderでなくnamed wallになる。`lower_function_all` 内で動くため、loweringする全CI経路（Test WASM、Cross-Target、Trust Spine）がbuildごとに通る。

5分類のregression pinは [`proofs/TRUSTED_BASE.md`](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/proofs/TRUSTED_BASE.md#the-five-2026-07-03-trusted-zone-bug-classes-and-their-regression-pins)。

gate出荷後もこの行は問題を出した。#1537（2026-08、実下流project報告）も同例で、`desugar_guard` のネストrewriteがUnit型statement-`if` 下にResult型armを残し、branch loweringのdeferred-value fallthroughがearly returnを黙って落とした。guardの `err` は伝播せず、guardを飛ばした状態で `ok` を返した。MIRはまたvalid、RC-balanced、name-total、certifiedだった。未解決値はなく整形式の誤値が流れたためdef-before-useでは見えない。修正は同post-pass型のnamed invariant追加（unit型arm内Result/Option tailにstrict-mode wall、`crates/almide-mir/src/lower/control.rs`）。完全対応は#1527 wall burndownで追跡。この行を正直に読むなら数か月ごとに新たな「受理されるが誤り」を生み、各々追加invariantで退役させている。したがって図では**trusted**のまま。分類を動かすのはgate数でなく静かな四半期である。

<a id="the-edit-locality-kernel-seam-stage-3"></a>

## edit-locality kernelとの接続部（Stage 3）

λ_almd kernel（`crates/almide-edit-belt`、3つ目の0-sorry Lean belt）はcoreの機械検査済み参照意味論を与える。`ev_agree`/`edit_frame`/`ev_det`（決定性込みL1）、`typing_modular`、`pure_silent`、`eval_sound`（実行可能評価器と関係の一致）。出荷compilerへの橋には位置づけの異なる3層がある。

- **kernel検査済み（proven）** — 手書きfamilyの観測。`k1_obs`…`kAll_obs`、`corpus_total` は `:= by rfl`、つまりLean KERNEL reductionで証明した定理。公理集合は `#guard_msgs in #print axioms` でpin（`propext` のみ）。`eval_sound` + `ev_det` はpinした `evalE` 出力を唯一の `Ev` 導出（`kAll_ev`）へ持ち上げる。生成corpusでkernelが検査するのは**全域性**: 48program全てが `some` 観測へ評価される（`corpus_total`）。
- **評価器pin（trusted）** — `proofs/kernel-conformance/*.expected` の**値**。COMPILED Lean評価器（`lake exe conformancegen --write`）が出すため、信頼にはLean compilerへの信頼が必要。lean4自身が `trustCompiler` 公理で名指す境界である。`evalE` の未証明complete方向もdocコメントでなく列挙可能な対象: `EvalCompleteness` が記述し、`True` に保つmarker公理 `trustEvalCompleteness` が必要な議論をtag付けし、CI axiom ratchetがbelt内全 `axiom` を列挙する。
- **trusted（レビュー済み接続）** — `Corpus.lean` の `eraseE`/`eraseChain` + `render*` 対。同surface grammarを走査する約40行ずつの関数で、一方は評価器の採点するλ_almd項、他方はcompilerに渡すalmide textを作る。constructorごとに一致をレビューすること**そのもの**が信頼義務で、programごとでなく1度レビューする。（手書きfamilyも同じ接続を手で加える: `spec/wasm_cross/kernel_conformance.almd` ↔ `kAll`、Rust literal ↔ Lean literal。）
- **gated** — 両backendがその観測を出すこと。`tests/kernel_conformance_test.rs` が全corpusをnativeとwasm runtime存在時のwasmで実行。`wasm_cross` harnessにfamilyを含め、`conformancegen --check` CIがcommit corpusを `Corpus.lean` にbyte同一でpin。全て契約C-280下。

上の分類が採るLean自身の3段階語彙（lean4ソース読解、Survey 4による修正）:

| Lean idiom | 信頼水準 | 理由 |
|---|---|---|
| `decide` / `:= by rfl` | kernel証明 | kernel自身が項を再reduction |
| `#guard` | 非信頼pin、**証明ではない** | 非信頼elaborator評価器で実行（lean4 `src/Init/Guard.lean`） |
| `native_decide` | **名指し公理を仮定した**compiled評価器の証明 | `Lean.ofReduceBool`/`trustCompiler` を実体化し `#print axioms` に接続を示す |

beltの全conformance主張はtier 1。`.expected` fileはtier 3の信頼水準（compiled評価）で、公理でなくここに接続を記録する。consumerがLean証明でなくshell gateだからである。

従って「backendはkernelのrefinement」は**生成されたλ_almd表現可能fragment**上で強制される。機械生成program、kernel検査でないcompiled評価器の期待値、backend機械diff、レビュー済み約80行の接続がある。compilerをprover内で書き直さない限り届かないのは、検証済みsurface-core→λ_almd翻訳とRust実装自体のpassごとのsimulation証明である。研究級の残余として `docs/roadmap/active/edit-locality-theory.md` に記録し、それまではfragment gateをratchetとする。初日で実compiler bug2つを検出した（almide#1428: checker受理programがcodegenで死ぬ、almide#1429: v1 rendererがbare-parameter tailでeffect fnのsignature/bodyを分断）。gateは実効性がある。

<a id="above-l1-l4-and-l5-edit-localitymd-1a"></a>

### L1より上: L4/L5（`edit-locality.md` §1a）

各義務を主張可能な内容ごとに分ける。半分ごとに位置づけが違い「L4成立」だけでは何を検査したか隠れるためである。以下の全定理は `crates/almide-edit-belt/AlmideEditBelt/Contract.lean` にあり、0 `sorry`、新公理なし。

| 義務 | 位置づけ | 主張 | 主張しないこと |
|---|---|---|---|
| L4、effect frame | **proven**（`l4_pure_replacement_silent`） | pure宣言定義を不変signatureに適合するものへ置換すると、編集後λ_almd programの任意環境で空traceになる | replacementの戻り値やcallerの利用方法。callerは次に別内容をprintしうる |
| L4、非空虚性 | **proven**（`l4_witness`、`l4_loud_replacement_prints`） | 4仮定は同時成立し、purityなしなら結論は偽（effectful置換は `"hi"` をprint） | 仮定が最弱であること |
| L4、resource frame（`scoped`、#1997） | **gated**、未証明 — C-362 / C-363 / C-364 fixture | `scoped` blockの観測はbodyのもの、`scoped fn` はregion内外で同応答、fragment外形状は両targetのcheck時拒否 | λ_almdの性質。kernelにheap/regionはなく、region component追加まで述べる対象がない |
| L5、合成 | **proven**（`l5_edits_compose`、`l5_pure_silent_after_edits`） | signature維持置換の任意列でwell-typedを維持し、結果の全pure宣言定義はsilent | pure codeの空trace以外の観測保存 |
| L5、両targetでの実現 | **trusted** — backend refinement | — | checked lowering（#1995 ExitPlan、#1996 E-OWN-LOWERING）がnative/wasmで上記を保つこと。この節の他行と同じkernel/backend接続で、`spec/wasm_cross` と契約台帳でgateするが未証明 |

<a id="what-each-gate-actually-claims"></a>

## 各gateが実際に主張すること

| Gate | 主張 | 主張しないこと |
|---|---|---|
| `proofs/gate.sh` | witnessed MIRはRC-safe、name-total、capability-bounded。`almide-verify` は全行とseed付きrandom witness1,000件でextracted checkerと同判定 | wasm byteとwitnessの一致、行外入力での `almide-verify` 一致（定理を持たない） |
| `proofs/structural-wall.sh` | `witness-fixtures.txt` 全fixtureで出荷file SHA-256がbundleのpinと一致し、structural buildの全ownership/call-mode/name/capability/bounds/init witnessをextracted checker、kernel、`almide-verify` が共に受理。`accepts_spatial` により受理bounds witnessの全runで記録index accessがblock内。`site_window` により受理init witnessの全記録construction siteがblock全域を初期化。また出荷fileへの `almide-anchor-check`（#4011 (d)）により、全located bounds/init eventは名指すinstructionそのもの（site alloc size、storeのwidth/offset/address local、fill countdown、check compare-and-abort、access load/store）。全located accessはcheckのoperandに拘束される（同index、slot sizeで数えたaddress先blockのcount、1slot内、間にどちらも変更なし） | ownership eventがbyte内RC instructionであること、located judge callがcopy-on-write judgeでblockを渡されたcallだけがin-place縮小すること、representation、bounds witness未記録heap access（record field、string byte、map slot、`for` loop element read、runtime routine） |
| `proofs/native-wall.sh` | `proofs/native-verified-floor.txt` の全programで、verified native rendererがrenderした各関数について、実際にrenderしたMIR由来のownership witnessをextracted checker/kernel/`almide-verify` が受理し、bundleが生成Rust source SHA-256をpin。releaseを落としたrenderのwitnessは拒否 | rustcがbinaryでMIR balanceを維持すること、floor外program（standard-codegen fallback） |
| `scripts/witness-reinject.sh` | lost bounds check再注入（store経路、read経路、古い#3345 fact）ごとに `almide-verify` が拒否するbounds witnessとなり、list-literal store欠落再注入も拒否init witnessになる。同programの未patch emitter witnessは受理。`anchors`: 記録callが残るinstruction変更再注入（guard無効化、slot store位置ずれ、fill cursor倍化）は証明済みcheckerに受理されるwitnessと `almide-anchor-check` が拒否するbyteになる。operand bug再注入（byte単位count、記録も適用もされないkill、zero-width Bytes room）も同じ | anchor検査対象外のinstruction変更（representation、RC instruction）の検出 |
| `scripts/check-build-certifies.sh` | compiler横に `almide-verify` があれば、`almide build --target wasm` は自身のcertificateを拒否されたartifactを書かない。各bounds check欠落再注入でbuildはfileなしexit 1、未patchならfileを書き、verifierを移せばpatch済buildも書く | verifier未設置buildの検査（されない）、certificate未記録の内容（上行） |
| `proofs/corpus-wall.sh` | corpus上で `lower_function` が全域的: 全関数 `Ok` または明示 `Unsupported` | `Ok` 関数の出力が正しいこと |
| `proofs/output-parity.sh` | baseline集合でnative/structural wasm buildが一致。その中のstructural wallは正確に `proofs/output-parity-walled-baseline.txt` の行（#2793） | 集合外の内容 |
| `scripts/check-contracts.sh` | 全observable cross-target promiseに実行可能証拠あり | promise自体の正しさ |
| greenfield parity gate（`ast_parity`、`check_parity`、`run_parity`、`backend_parity`、WASI gate） | 移植parser/checker/interpreter/structural wasmが、**同commit**からbuildしたCLIのcorpus出力（`--emit-ast`、`check --json`、`run --target wasm`）をbyte単位再現。`scripts/check-parity-goldens.sh` はCIでそのbinaryからgolden再生成し全行diffで失敗するため、手編集や古いbuild由来記録を防ぐ（#2183） | CLI自身の出力の正しさ。oracleは独立参照でなく試験対象tree。CLI正しさを主張するのは契約台帳/kernel pin/cross-target fuzzで、経路一致は移植忠実性の主張のみ |
| cross-target fuzz | N programでdivergence未発見 | divergenceが存在しないこと |
| E083 exit validator（`exit_plan.rs`、#1996） | 全関数のemit exit windowがchecked ExitPlanを実装（各解放creditそこでdecrement、carry/foreignはdecrementなし、planが名指すtransfer instruction、frame置換edgeをまたぐ未処理creditなし）。wasm byteから読み戻す。違えばcompiler-defect診断でbuild停止（wallでもrerouteでもない） | plan**内容**の正しさ（credit harness/witnessの仕事）、byteより下流（wasmtime/host） |
| kernel-conformance pin（C-280） | compiled familyの観測は両targetでkernel検査済λ_almd traceと一致。生成corpusはcompiled評価器traceと一致 | 全almide programがkernelをrefineすること（fragmentの像だけpin）。corpus `.expected` 値は評価器pin（compiled Lean）でkernel検査ではない |

全再現: `make verify-trust`。
