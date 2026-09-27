<!-- description: AI-DLC Bolt backlog for the v1 climb — the camps/steps roadmap expressed as intent-driven, time-boxed Bolts (each with intent / Definition-of-Done / gate / deps / status). The construction guardrails are the goal-prompt discipline; each Bolt's exit gate is independent review (reviewer agent + Trust Spine CI + unbiased dual-oracle corpus); humans (Mob) decide at the marked forks. Tracks "あと何 Bolt" to each summit. -->
# v1 Bolt Backlog(AI-DLC 管理)

> **これは何か**: 頂上までの camp/step ロードマップを、**AI-DLC の Bolt**(intent 駆動・時間箱・
> 数時間スケールの作業単位)として構造化したもの。**1 Bolt ≒ 1 commit/brick**。進捗を「あと
> 何 Bolt」で測り、自走ループで消化する。
> **関連**: [v1-kgi-kpi](v1-kgi-kpi.md) / [flight-profile](flight-profile.md) / [effectful-27-blueprint](effectful-27-blueprint.md) / [interp-is-desugar-to-tostring](interp-is-desugar-to-tostring.md) / [arena-breakthroughs](arena-breakthroughs.md)。
>
> **最終更新: 2026-09-27(v0.64.0 出荷日、origin/develop `d027bbb77`)**。前回 2026-06-21 からの
> 差分は下の「3 か月で何が変わったか」にまとめた。状態は issue/PR/commit で裏を取ったもの。

## 3 か月で何が変わったか(2026-06-21 → 09-27)

- **名前が変わった**: 6 月の「v1」は今 **incumbent leg**(`crates/almide-mir`、証明書を持つ側)。
  既定で出荷しているのは greenfield で新造した **structural leg**(`crates/almide-wasm`)。
  Stage 2 着地 #1599-#1601(2026-08-26)、v0.60.0 で既定化。出典 `docs/wasm/README.md`、`docs/ARCHITECTURE.md`。
- **oracle が変わった**: 「v0 との byte 一致」ではなく、native ⇄ wasm の byte 一致(`spec/wasm_cross`、契約台帳 C-NNN)
  + interp の 3-way。
- **①(run 率)は床を越えた**: unbiased corpus が 6 月の 0/15 から 666+15 / 684(約 99.6%)に(下記 C1-B5)。
- **構造の逆転**: **出荷される既定 leg に証明がない**(trusted であって proven ではない)。
  certificate は incumbent 側だけ。structural の witness は wasm_cross 789 本中 **1 本**(`d9945cd57`)。
  → Camp 4 の本線は「証明を増やす」から「**証明を既定 leg に載せ替える**」に変わった。
- **② の道具が出荷された**: v0.64.0 で `almide survive` / `apply --if-survives`(#2147)と構造化 `repair`
  診断(#2149)が出荷された。ただし A/B の実数はまだない。

## AI-DLC マッピング

| AI-DLC | 本プロジェクトでの実体 |
|---|---|
| **Intent**(何を作るか) | Camp(execution / MSR / WASI / proof / flight) |
| **Bolt**(時間箱の作業単位) | 1 commit = 1 brick(intent + DoD + gate を持つ) |
| **Construction 規律** | goal-prompt(穴でなく壁・push+CI・dual-oracle・false-green・root-fix-not-revert) |
| **Bolt の done-gate** | 独立 CI(Coq+公理+PCC)∧ native⇄wasm byte一致 + 3-way oracle ∧ 契約台帳 ∧ 規律。**self-run は完了でない** |
| **Mob checkpoint**(人間) | 号令 / 優先順位 / スコープ / 「このゴールで正しいか」/ MSR 結果→GTM 軸。GitHub では `mob` ラベル |

## Bolt テンプレ

```
Bolt <ID>: <一行 intent>
  DoD  : 機械でチェックできる完了条件
  gate : 検証手段(CI / dual-oracle / unbiased corpus)
  deps : 依存 Bolt
  状態 : ⬜未 / 🔄進行 / ✅完了 / 🧱壁(部分) / 📐設計済 / 🪦前提が変わり置換
```

---

## 🏕 Camp 1 — 実行パリティ【ほぼ完了・残りは incumbent 退役へ合流】

- **C1-B1: heap-result variant match を実行** — ✅
  #1492 アーク(2026-08-17)で **lowering だけで通った**。Coq・checker は無変更で、6 月の
  「Camp 4 gated」は誤診だった。wall corpus は 7/10。残り 3 は Map/Set の heap-key typed twin(→ N-B4)。
  出典 [camp4-general-heap-match](camp4-general-heap-match.md)。structural leg は任意形状を下ろせる。
- **C1-B2: 残り heap 値配管** — ✅(🪦 旧ヒストグラムは incumbent 前提なので置換)
  現在の実測(#2739): spec build lane 919 本 → structural 881 / incumbent のみ 24 / 両 leg 拒否 5。
  test lane 462 本 → structural 392 / incumbent 61。
- **C1-B3: closures C2(first-class)** — ✅
  Closure Architecture v2 P0-P6 完了([closure-architecture-v2](closure-architecture-v2.md))。
  structural で env drop-glue(`3f60174d3`)、route handler closure(`cf90ab9d7`)。
  「閉包に ABI なし」系列は #2287 ✅ / #2288 ✅ / **#2289 🔄**(→ N-B3)。
- **C1-B4: stdlib の幅** — 🔄(ほぼ完了)
  公開 fn 1,003 のうち structural 942、incumbent のみ 19、両 leg wall 42。関数単位の reachability
  frontier は 0(PR #1224)。残りは host capability。`proofs/wasm-reachability-baseline.txt`、
  `proofs/target-availability.toml`。
- **C1-B5: ⭐ *unbiased* corpus で honest 距離を測る** — ✅ **再測定済み(2026-09)**
  almide-dojo の LLM 解(別プロセス生成)892 本 → structural 666 / incumbent 15 / 両 leg 拒否 3 /
  旧構文 208(対象外)。**有効 684 本中 約 99.6% が wasm で通る**(6 月は 0/15)。#2739、`dc832c1b9`。
  ⚠ これは「wasm で test が通る」率。native byte 一致率を別に算出した数字ではない。
- **C1-B6: 実 repo を通す(cross-repo conquest)** — 🔄
  csv ✅ / svg ✅(6 月)/ almide/dfa は wasm で 50 test byte 一致。downstream 4 repo
  (almai / comide / golemide / gramide-cli)55 test files → structural 37 / incumbent 依存 0 /
  両 leg 拒否 17 / E081(http/process)3。
- **Camp 1 Exit**: 実 agent プログラムが走って byte 一致 → **実質達成**。残りは「両 leg 拒否」の
  尻尾と incumbent 退役(N-B1)。

## 🏕 Camp 2 — MSR(存亡の賭け)【道具は揃い、実測待ち】

MSR の作業は [almide/almide-dojo](https://github.com/almide/almide-dojo) に移った(repo 境界は CLAUDE.md)。
軸は「二値の生存率」から「**契約を保つ変更バンク + survive による回復の A/B**」へ移った。

- **C2-B1: MSR 統制実験を組む** — ✅(🪦 6 月の `msr-v2` workflow は置換)
  Dojo の日次レーン(`src/main.almd`、ハーネス自体が Almide 製)と言語横断レーン(`src/msr/`、7 言語 21 タスク)。
  各走行に verdict `comparable` / `inconclusive-saturated` を刻印。飽和対策として契約保存変更バンク
  (dojo#3、400 タスク・8 family・hidden oracle・wasm leg・resource oracle)を投入済み。
- **C2-B2: 回して数字を出す** — 🔄
  - 日次(2026-09-27、v0.63.1、38 タスク、comparable): Llama 3.3 70B 28/38(73%)、Llama 3.1 8B 13/38(34%)。
  - 言語横断(2026-09-22、Llama 70B、21 タスク): **Almide 80%** / Rust 95% / Go 100% / TS 100% /
    Zig 61% / MoonBit 47% / Gleam 14%。**Almide は Rust/TS/Go に負けている**。
  - Sonnet 5 の行はない(#1617 は閉。「CI に API 鍵を置かない」裁定なので手動計測が要る)。
  - バンク pilot(400×3×3)は 2026-09-25 に 6h43m でキャンセル。飽和 verdict は未取得。
  - #1963(P-critical)「thesis に現在の計測がない」が OPEN。
- **C2-B3: ⭐ MSR 再定義(失敗の明確性+回復可能性)で測り直す** — 🔄(道具出荷済・A/B 待ち)
  v0.64.0 で `almide survive` / `apply --if-survives`(#2147、CLI・MCP・LSP)と構造化 `repair`
  フィールド + 双方向ラチェット + `explain --list`(#2149 ✅)が出荷された。dojo#58 で survive A/B を実装済み
  (control vs survive、同一予算 4 回、勝利条件 = 平均試行回数 ≤ 0.8×control)。偽モデルでの配線実証は
  3.00→2.00 だが、これは効果の証拠ではない。
  残り: dojo#59(pin を v0.64.0 へ)→ Llama 70B で dispatch(Workers AI 呼び出し 約 2,200-2,500 回)。
  #2147 は数字が出るまで OPEN。6 月の inject-mistake catch 率は survive A/B に吸収された(推定)。
- **契約相対生存アーク #1998**(2026-09-07 批准): #1994 / #1995 / #1996 / #1991 / #1997 / #2009(L2/L3)✅、
  残りは dojo#3(バンク)と #1589(generic protocol)。edit-locality L1 の予測ループ
  (`proofs/l1-verdicts.toml` + dojo `src/l1_loop.almd`)が稼働中([edit-locality-theory](edit-locality-theory.md))。
- **Camp 2 Exit(Mob 判断)**: GTM 主軸の決定。**言語横断で Almide 80% < Rust 95% という数字は、
  6 月の判断「二値 MSR 単独では勝てない、② 検証+回復が頑健」を裏づけている**。survive A/B の数字が
  最終判断の材料。

## 🏕 Camp 3 — 検証済み WASM/WASI component producer【大半は済・「検証」部分が残る】

- **C3-B1: effectful WASI floor** — 🔄(大半は済)
  clock は host op 60 で embedded host / p1 shim / p2・p3 component の全部に届く(`124d80cfb`)。fs と
  env.os/temp_dir/cwd は p1 shim が提供(`7782e66d0`)。p3 には fs の read / write(#1662 / #1666)。
  `declared ⊇ used` は Coq で証明済み(`proofs/CapabilityBound.v` は関数単位、`CapabilityReach.v` は推移閉包)。
  `--profile critical` は deny-all + `--allow`(#567 ✅)。
  残り: env/process の p3 host surface(#1628 の残件、#2589)。
  ⚠ 能力語彙は nat 添字の汎用 allowlist で、Fs/Net/Clock/Random/Env を個別の Coq 型にはしていない。
  6 月の DoD を満たしたと見なすかは Mob 判断。
- **C3-B2: Component Model 境界の検証/最小化** — 🔄(産出は済・検証は ⬜)
  stage 0 adapter(#1631)→ stage 1 direct p2(#1632、アダプタ形の 1/3.2 のサイズ)→ stage 2 direct p3
  (#1646)+ fan.any cancel(#1664)+ fs → stage 4 WASI pin ledger(`proofs/wasi-pin-policy.toml`)。
  canonical ABI(lift/lower)の証明も、最小化の論証も**まだない**。追跡 #1628 は OPEN。
- **関連の着地**: wasm VM #865 ✅(PR #2304、parity 681 一致 / 0 不一致)。http layer(v0.64.0:
  `http.serve` が embedded wasm lane で動く C-367、call handle は C-366)。後続は #1710(wasi:http@0.3 client)
  と #2702(P3 handler export)。
- **Camp 3 Exit**: 検証済み component を産出 → **産出は達成、「検証済み」は Camp 4 の載せ替え(N-B2)待ち**。

## 🏕 Camp 4 — 証明の完全性【incumbent 上は概ね達成 → 既定 leg への載せ替えが本線】

- **C4-B1: leak-freedom / reuse / call-mode** — 🔄(**incumbent 上は ✅、structural 上は ⬜**)
  leak-freedom: `OwnershipChecker.check_sound` + OwnershipLoop。reuse: FreeList.v / FreeListRc.v(#909 ✅)/
  CowSafety.v。call-mode: `CallModes.v`(`c485bef9a`)。runtime: StructuralRuntime/Alloc/Decode/Run.v
  (#576 ✅)。native の ownership certifier #2231 ✅(台帳 752→0、残件 #2239 ✅)。
  ⚠ 既定の structural leg で certified なのは wasm_cross **789 本中 1 本**(grow-only ラチェット、`d9945cd57`)。
  残りは #2755-#2759(branch / loop / closure / call-modes / names / caps)。
  ⚠ [v1-kgi-kpi](v1-kgi-kpi.md) の「4/8」は 07-30 時点のままで古い。
- **C4-B2: byte 束縛(Gap 1・最難関)** — 🔄(部分的)
  rc プリミティブ(`$rc_inc` / `$rc_dec`)は Coq 内で bytes→ISA まで定理になっている
  (WasmEncode/Exec/Isa/Decode.v、StructuralDecode.v)。任意プログラムの witness→bytes は未達。
  近い一歩は **#2760**: receipt を structural build から再生成し、各 `.wasm` の SHA-256 を pin する。
- **Camp 4 Exit**: 安全性束を**既定 leg で** 8/8 → 信頼主張が飛行級へ。

## 🏕 Camp 5 — 飛行級(cert スパイン、③④市場)【📐 → 🔄、kit はほぼ揃った】

- **C5-B1: WCET / counted-loop を Coq へ** — 🔄
  #569 ✅(WCET 三層ストーリー + gated PID カーネル、PR #1642、`docs/project/WCET-STORY.md`)。ループ所有権は
  `proofs/OwnershipLoop.v` / `CoownLoop.v`。G-F2(ループを Coq に持ち上げ確保回数の上限を証明)は ⬜。
  structural witness の loop frame は #2757。target の calibration は外部に残る。
- **C5-B2: 本番 MIR→Rust + Ferrocene** — 🔄
  Ferrocene レーンは実 1.95.0 で 633 件全緑(#573 ✅)、trace-map #572 ✅。
  **native Rust の生産者が 2 本ある問題 #1480 は Mob 待ち**。
- **C5-B3: リファレンスアプリ + 資格化キット** — 🔄(✅ 寄り)
  参照アプリ PID は C-230、G-F4 は #776 ✅。キットは #566 / #567 / #568 / #574 / #575 / #576(Coq 45 定理)/
  #571 dossier / #1534 署名がすべて ✅、追跡 #586 は 2026-09-21 に閉。
  残り: ALS(CG-1)は PARTIAL(als 側で参照評価器が進行中)。
  ⚠ `proofs/TOOL-QUALIFICATION.md` の Gaps 表と [flight-qualification](flight-qualification.md) のキット表 10-13 は古い。
- **Camp 5 Exit**: 飛行ラダー G-F0..G-F6 → 安全臨界/航空(提携)。**kit 側は揃い、証明を既定 leg に載せる N-B2 が律速**。

## 🆕 新 Bolt(2026-09 に発生した本線)

- **N-B1: incumbent wasm emitter の退役**(#1696 step 4-5 → 追跡 #2739)— 🔄
  DoD: 両 leg の route census(91 行、shrink-only、#2741)が 0 → WAT emitter 約 2.6 万行を削除(#2761)。
  最大の原因は p1 の `to_wasi` に fs 操作がないこと(25 本)。個別は #2742-#2753、#2703。
  deps: N-B2(証明書を先に structural へ移さないと、退役で証明が消える)。
- **N-B2: ⭐ 既定 leg に証明書を載せる(structural witness ラダー)** — 🔄
  #2754 ✅ → #2755-#2759(branch / loop / closure / call-modes / names / caps)→ #2760(receipt 再生成 + SHA pin)。
  DoD: C-SAFE / C-PROVEN receipt が structural build から出る。certified wasm_cross を 1/789 から伸ばす。
  **Camp 3 / 4 / 5 の Exit がすべてここを通る**。
- **N-B3: fusion を self-hosted HOF 本体の特殊化にする**(#2289)— 🔄
- **N-B4: Map/Set heap-key typed twin**(C1-B1 wall corpus の残り 3、ルート分割 `4d75df7ce`)— 🔄
- **N-B5: survive A/B を実測する**(dojo#59 → dispatch → #2147 の勝利条件判定)— ⬜ **Mob 承認待ち(Workers AI 予算)**
- **N-B6: バンク pilot を再走して飽和 verdict を得る**(dojo#3、9/25 はキャンセル)— ⬜
- **N-B7: make-verify を survive / repair 版に作り直す**(S-B1 の再起動)— ⬜
- **N-B8: Stage 4 durability — fuzz true-green streak**(`proofs/STAGE-STATUS.md` では **0 日**)— 🔄
- **N-B9: #1628 残件**(host 境界 determinism の C 行、env/process p3)と http の p3 化(#1710 / #2702)— 🔄

## 🏔 頂上 Bolt — GTM

- **S-B1: make-verify キラーデモ** — 🔄(**停滞**)
  `demo/make-verify/` は 2026-06-18(`cd3c4e248`)以来更新がない。v0.64.0 の `survive` / `repair` を
  使えば「誤修正を適用前に止めて直し方を示す」を実物で見せられる状態(→ N-B7)。

---

## Mob checkpoint(人間=あなたが決める分岐)

**2026-09-27 時点で判断待ちのもの**(GitHub の `mob` ラベルは 8 件):

- **survive A/B の dispatch 承認**(N-B5): Workers AI 予算。言語横断レーンは手動実行のみ(dojo#55)。
- **frontier モデル(Sonnet 5 等)の行**: 鍵なし CI の方針の下で、誰がいつ手動で測るか。
- **言語横断 80% < Rust/TS/Go の扱い**: GTM 上で verification 軸に寄せる根拠として使うか。
- **#1480 native Rust の生産者 2 本の一本化**: C5-B2 の律速。
- **C3-B1 の能力語彙**: nat allowlist で DoD 達成と見なすか、Coq 型に分けるか(soundness-critical)。
- **C4-B1 の完了定義**: 「どの leg の証明か」。本書は「既定 leg で」と定義した(N-B2)。

**恒常的な分岐**(6 月から不変): 次に登る Camp/Bolt / 号令レベル(着手・スコープ変更)/
proof・Coq 信頼語彙の拡張(必ず人間判断)。

## 自走ループ(Mob 以外)

```
Bolt を1つ取る(優先 = 既定 leg の証明被覆を最も上げる / release blocker が先)
  → goal-prompt 規律で construct
  → done-gate(独立 CI ∧ native⇄wasm byte一致 ∧ 契約台帳 ∧ false-green ∧ 壁)
  → 通れば次 Bolt / 落ちれば root-fix
  → self-run で完了にするな・危険1タスク延期≠セッション終了
```

## 進捗ビュー(あと何 Bolt)

- **🚩 最初の商売頂上(①agent市場)** = Camp 1 **実質済** + C3-B1(残 env/process)+ S-B1(N-B7)
  ── **近い**。技術は揃い、足りないのはデモと回復の数字(N-B5)。
- **🏔 真の頂上(全市場・飛行)** = N-B2(既定 leg の証明)→ N-B1(退役)+ C4-B2(byte 束縛)+
  Camp 5 の G-F2 ── **律速は N-B2**。6 月は「Camp 4 escalation」が律速だったが、今は「証明はあるが、
  出荷物に乗っていない」が律速。

## リリース / issue スナップショット(2026-09-27)

- 最新: **v0.64.0**(2026-09-27、「check an edit before applying it, repairs in diagnostics, and an http layer」)。
  v0.64.1-rc1 が同日 pre-release。
- open issue 88 件(enhancement 52 / A-wasm 29 / A-codegen 12 / A-stdlib 12 / proof 10 / mob 8 / bug 5 …)。
- **release blocker 3 件**(`scripts/count-release-blockers.sh`): #2780(regression、0.64 から `process.exit` が
  126..255 を拒否し、wrapper が子プロセスの exit status を返せない)/ #2782(I-divergence、wasm `bytes.repeat` が
  C-197 の上限検査をすり抜けて trap)/ #2783(I-divergence、native `matrix.linear_f32_row_no_bias` が C-161 を飛ばして panic)。
- バージョン順の計画は [ROAD_TO_1_0](../ROAD_TO_1_0.md)(2026-09-27 に 0.65 以降を引き直した)。本書の N-B1 / N-B2 が
  その 0.6x 残り、C2 系が 0.8x に当たる。

---

## 履歴: Mob 判断「② を勝ち筋として active 昇格」(2026-06-18)

north-star に照らすと ②(検証+回復)が勝ち筋、①(run 率)は床、と判断して優先 fork を組み替えた。
当時の active は S-B1 / C2-B3、Mob-gated は C4-B1 + C1-B1 の cert-precision、fade は C1-B2 / C1-B4。

**2026-09-27 の再評価**: 判断は**維持・強化**。①は Camp 4 escalation なしで structural leg が床を越えた
(C1-B1 の gated は誤診だった)。② は v0.64.0 で道具として出荷され、言語横断の数字(Almide 80% < Rust 95%)
も「二値 MSR 単独では勝てない」を裏づけた。残る問いは、② の効果を数字で示すこと(N-B5)と、検証を出荷物に
乗せること(N-B2)の二つ。
