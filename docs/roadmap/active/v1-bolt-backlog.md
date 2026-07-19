<!-- description: AI-DLC Bolt backlog for the v1 climb — the camps/steps roadmap expressed as intent-driven, time-boxed Bolts (each with intent / Definition-of-Done / gate / deps / status). The construction guardrails are the goal-prompt discipline; each Bolt's exit gate is independent review (reviewer agent + Trust Spine CI + unbiased dual-oracle corpus); humans (Mob) decide at the marked forks. Tracks "あと何 Bolt" to each summit. -->
# v1 Bolt Backlog(AI-DLC 管理)

> **STALE (last touched 2026-06-21)** — see [v1-loop-ownership-cert.md](v1-loop-ownership-cert.md) and
> [v1-org-byte-verification.md](v1-org-byte-verification.md) for the current wall status; the Camp 1
> status table below likely no longer reflects reality.

> **これは何か**: 頂上までの camp/step ロードマップを、**AI-DLC の Bolt**(intent 駆動・時間箱・
> 数時間スケールの作業単位)として構造化したもの。**1 Bolt ≒ 1 commit/brick**。進捗を「あと
> 何 Bolt」で測り、自走ループで消化する。
> **関連**: [v1-kgi-kpi](v1-kgi-kpi.md) / [flight-profile](flight-profile.md) / [effectful-27-blueprint](effectful-27-blueprint.md) / [interp-is-desugar-to-tostring](interp-is-desugar-to-tostring.md)。

## AI-DLC マッピング

| AI-DLC | 本プロジェクトでの実体 |
|---|---|
| **Intent**(何を作るか) | Camp(execution / MSR / WASI / proof / flight) |
| **Bolt**(時間箱の作業単位) | 1 commit = 1 brick(intent + DoD + gate を持つ) |
| **Construction 規律** | goal-prompt(穴でなく壁・push+CI・dual-oracle・false-green・root-fix-not-revert) |
| **Bolt の done-gate** | 独立 CI(Coq+公理+PCC)∧ dual-oracle byte一致 ∧ 規律。**self-run は完了でない** |
| **Mob checkpoint**(人間) | 号令 / 優先順位 / スコープ / 「このゴールで正しいか」/ MSR 結果→GTM 軸 |

## Bolt テンプレ

```
Bolt <ID>: <一行 intent>
  DoD  : 機械でチェックできる完了条件
  gate : 検証手段(CI / dual-oracle / unbiased corpus)
  deps : 依存 Bolt
  状態 : ⬜未 / 🔄進行 / ✅完了 / 🧱壁(部分)
```

---

## 🏕 Camp 1 — 実行パリティ(実プログラムが v0 byte一致)【現在地・honest ~10%】

- **C1-B1: heap-result variant match を実行**
  DoD: `match opt { Some(x) => "str", None => "str" }`(match が heap 値を返す)が v0 byte一致、非実行は WALL
  gate: dual-oracle byte一致 + CI緑 + false-green。real-program corpus の 3 WALL 中2本(safe_div_chain/validate_age)が flip
  deps: Option/Result value-match(✅済) / 状態: 🧱 **Camp 4 gated(2026-06-18)** — byte一致するが proven ownership checker が owned/param 両方で REJECT(merged-dst の branch-join が現 Coq trust 語彙で証明不可)。trust spine が2回拒否→walled。これは Camp 4 cert-precision の ESCALATION であって単純 Camp 1 Bolt でない。SCALAR-result の variant match(call-arg/let/operand/tail)は ✅済(commits 63afd081/90ac820e/4154ba5b/6eb21c52)
- **C1-B2: 残り heap 値配管(~650、histogram)**
  DoD: List-arg(253)/ OptionSome・ResultOk-arg(271)/ heap-result match(103)/ Record・Result 返り(~90)/ List[heap] literal(62)を各々 byte一致 or WALL
  gate: 各サブクラスごと dual-oracle + CI / 状態: ⬜(列挙済)
- **C1-B3: closures C2(first-class)**
  DoD: `let f = (x)=>...; list.map(f)` / 格納・受け渡し・返す closure が byte一致(capability 付き repr)。defunc 不可は今まで WALL だったのを実行化
  gate: dual-oracle + false-green(closure の caps)+ CI / deps: C1(✅) / 状態: 🧱(C1 のみ済、C2 壁)
- **C1-B4: stdlib の幅(~500)**
  DoD: matrix / regex / json / string大小判定 / bytes / float.parse を各々 self-host or runtime brick、byte一致
  gate: per-fn dual-oracle + CI / 状態: 🔄(175 module self-host 済、継続)
- **C1-B5: ⭐ *unbiased* corpus で honest 距離を測る**
  DoD: dojo task-bank or LLM が別プロセス生成した(自作でない)プログラム batch で byte-match % を測定 → 偏りのない実数
  gate: そのもの(これが計測器)/ 状態: ✅ **実施済(2026-06-18)** — 別プロセスの 15 agent が生成した(自作でない)プログラム → v1 byte-match **0/15**(自作 57 は 7-9/10 = biased-upward を実証)。silent miscompile 0(soundness 無傷)。支配的 gap = Map/Set/records/guards。詳細 [[project_v1_output_parity_gap]]
  - 🔄 **gap 消化(2026-06-21)**: **records 全域 + `Map[String,String]` を攻略済**(svg full conquest, [[v1-records-svg]]) — record 構築/field/spread/再帰drop/List[Record] literal+concat/`map.entries`(新 `(String,String)` tuple-list)。横断修正 `not <bool-call>` の let arm も。**残 gap = Set / guards / 一般 Map(非 String 値) / path-mini-lang(svg path.almd は v0-wasm 制限で native fallback)**。
- **C1-B6: 実 repo を v1 で通す(cross-repo conquest)** — csv ✅(4/4) / svg ✅(records render byte一致+leak-free, [[v1-parser-tco-lever]] scoreboard)。次は Map/Set 依存の薄い実 repo を選ぶ。
- **Camp 1 Exit**: 実 agent プログラムが走って v0 byte一致 → ①市場 + make-verify デモが立つ
  - ✅ **make-verify デモ着手(2026-06-18, commit bed15165)**: `demo/make-verify/` — 誤修正は Almide で明確+回復可能(E010+修正提示)・Python で silent。MSR 再定義(失敗の明確性+回復可能性)を実物で具現、model 非依存(inject-mistake)。種は育成中。

## 🏕 Camp 2 — MSR(存亡の賭け)【0%・並行で*今すぐ*起動】

- **C2-B1: MSR 統制実験を組む**
  DoD: Dojo に Almide vs Python/TS/Rust/SPARK の修正タスク統制群、回せる状態
  gate: 実験デザインのレビュー / 状態: ✅ **ハーネス構築済(2026-06-18)** — seed→modify(blind)→judge を Almide/Python/Rust で(SPARK/TS は後 round)。fairness control + language-neutral pinned oracle(v1 交絡を修正済)。workflow script `msr-v2`
- **C2-B2: 回して数字を出す**
  DoD: MSR の実数 + 対照群との差
  gate: 統制・対照群つき測定 / deps: C2-B1 / 状態: ✅ **3実験実施(v1→v2→v3-haiku)、ただし★天井効果★** — 中難度では Opus も Haiku も全言語 5/5・0 retry(差ゼロ)。binding lever は model 強度でなく**タスク難度**。**MSR は regime 依存**。GTM 含意 → C2-B3。詳細 [[project_summit_map]]
- **C2-B3: ⭐ MSR 再定義(失敗の明確性+回復可能性)で測り直す**(Mob 確定)
  DoD: 「修正が*間違ってる*時、言語が明確に捕捉+回復可能か」を inject-mistake(model 非依存)で測定 → Almide/Rust(compile catch+diagnostic)vs Python(silent)。難タスク or 弱/多様モデルで regime を定量化
  gate: inject-mistake catch-rate + recovery / 状態: 🔄 demo で原理実証済(make-verify)、実験化は未
- **Camp 2 Exit(Mob 判断)**: MSR で GTM 主軸決定。**3実験の示唆: 二値生存は frontier で蒸発(天井)、頑健な差別化は「検証(証明付き安全)」+「失敗が明確・回復可能」。GTM 軸は MSR 単独より verification 寄りが頑健**(人間判断)

## 🏕 Camp 3 — 検証済み WASM/WASI component producer(賭け)

- **C3-B1: effectful WASI floor**
  DoD: clock/random/fs/env を WASI 0.2 host import + Coq 能力語彙拡張(Fs/Net/Clock/Random/Env)で実行、`declared ⊇ used` 検証
  gate: 走る + caps-safe(byte-match 不可 tier、honest に)+ CI / deps: Coq caps 拡張(人間 escalate)/ 状態: ⬜(blueprint 済)
- **C3-B2: Component Model 境界の検証/最小化**
  DoD: canonical ABI(lift/lower)を検証 or 最小信頼面に。verified component producer の差別化
  gate: 境界の trust-base 台帳 + 証明 or 最小化論証 / 状態: ⬜
- **Camp 3 Exit**: 検証済み WASM/WASI component を産出 → ①agent サンドボックス満額 + WASM 未来ポジション

## 🏕 Camp 4 — 証明の完全性(性質 4/8 → 8/8)

- **C4-B1: leak-freedom / reuse / call-mode**
  DoD: 残り3性質を Coq 証明 + 公理クリーン / gate: coqchk + Print Assumptions / 状態: ⬜
- **C4-B2: byte束縛(Gap 1・最難関)**
  DoD: witness → 実 wasm バイト を契約から証明済みへ(WasmCert-Coq ISA 層)
  gate: 証明 + wasmtime grounding / 状態: ⬜(deferred heavy track)
- **Camp 4 Exit**: 安全性束 8/8 → 信頼主張が飛行級へ

## 🏕 Camp 5 — 飛行級(cert スパイン、③④市場)【設計済み・最長】

- **C5-B1: 飛行キーストーン(あ)WCET/counted-loop を Coq へ**(設計=[flight-wcet-loops](flight-wcet-loops.md))/ 状態: 📐
- **C5-B2: 飛行キーストーン(い)本番 MIR→Rust + Ferrocene**(設計=[flight-rust-ferrocene](flight-rust-ferrocene.md))/ 状態: 📐
- **C5-B3: リファレンスアプリ + 資格化キット**(設計=[flight-reference-app](flight-reference-app.md) / [flight-qualification](flight-qualification.md))/ 状態: 📐
- **Camp 5 Exit**: 飛行ラダー「近い」(G-F0..G-F6)→ 安全臨界/航空(提携)

## 🏔 頂上 Bolt — GTM

- **S-B1: make-verify キラーデモ**
  DoD: real-program corpus が v1 を byte一致 + per-build 証明で通り、「テストが見逃す silent bug を捕まえる」を実証する demoable artifact
  gate: 走る demo + receipt / 状態: 🔄(種=real-program corpus が既に育ってる)**← v1 完成を待たず今建てる**

---

## Mob checkpoint(人間=あなたが決める分岐)

- **優先 fork**: 次に登る Camp/Bolt(デフォルト = byte-match を最も上げる Camp 1)
- **号令レベル**: closures C2 着手 / ターゲット選択 / スコープ変更
- **MSR 結果**: GTM 主軸の決定(C2-B2 後)
- **WASM/WASI 戦略**: Component Model 採用タイミング(C3)
- **proof/Coq 信頼語彙拡張**: soundness-critical なので必ず人間判断

## 自走ループ(Mob 以外)

```
Bolt を1つ取る(優先 = byte-match を最も上げる)
  → goal-prompt 規律で construct
  → done-gate(独立 CI ∧ dual-oracle ∧ false-green ∧ 壁)
  → 通れば次 Bolt / 落ちれば root-fix
  → self-run で完了にするな・危険1タスク延期≠セッション終了
```

## 進捗ビュー(あと何 Bolt)

- **🚩 最初の商売頂上(①agent市場)** = Camp 1(残 C1-B1〜B5)+ C3-B1 + S-B1 ── **近い・列挙可能**
- **🏔 真の頂上(全市場・飛行)** = + C2(MSR)+ C4-B2(byte束縛)+ Camp 5(飛行)── **遠い(長丁場 = byte束縛と飛行)**

## Mob 判断:② を勝ち筋として active 昇格(2026-06-18)

north-star に照らすと **②(検証+回復)が勝ち筋(既に 6/8、Python 0/8 silent)、①(run率)は床で
残りは Camp 4 escalation に gated**(unbiased 0/15、最難関、soundness-critical)。MSR-survival は
天井で蒸発、verification が生き残った ── よって優先 fork を組み替える:

- **🟢 active(② = 勝ち筋を育てる)**
  - **S-B1**: make-verify デモ拡張(②の実証・model-zoo に効く・GTM 種)
  - **C2-B3**: MSR-as-recoverability 実験(②の測定・regime 定量化)
  - 通常作業の compiler-②(silent を出荷しない壁規律)は継続
- **🟡 Mob-gated(① = 床、あなたの go 待ち)**
  - **C4-B1 + C1-B1 の cert-precision**(heap-result merged-dst / 2段再帰 drop = Coq trust base 拡張)。
    soundness-critical・rush 禁止・fresh+adversarial で「usable にする」と腹を括った時に着手
- **🔴 fade(diminishing)**
  - **C1-B2 / C1-B4**(cert-clean ① の残り plumbing/stdlib)。easy 分収穫済み、default で長く掘らない
