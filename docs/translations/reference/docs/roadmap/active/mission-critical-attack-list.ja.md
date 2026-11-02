原文の description メタデータ：Almide をミッションクリティカル分野で根拠をもって利用可能にするための階層別リスト
<a id="mission-critical-attack-list"></a>

# ミッションクリティカル対応の攻略リスト

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](mission-critical-attack-list.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](mission-critical-attack-list.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](mission-critical-attack-list.zh-CN.md)

> 固定原文の非規範読者用スナップショット。正本は原文であり、自動同期しない。独立した実装検証や引用指示の実行は行わない。歴史的状態、計画、比較主張は原文の範囲を保持する。
> [Original source (English) / 原文（英語）](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/mission-critical-attack-list.md)

> 原文注記：mission-critical 利用可能性は目標であり、適格性認定や安全認証ではない。チェック済み項目は原文の当時の結果であり、独立再検証ではない。末尾表は後の完了更新があっても 2026-08-18 の基準値のまま。A1-1 の 27 行と残り 23 棄権という異なる数、二つの挙動変更についての不完全な文句も、推測で整合させず残す。連続記録・qualification・本番導入の未チェック条件は未チェックのままである。


deep-wash campaign 終了時の 2026-08-18 に記述（compiler bug 7 系統を発見・修正、501 fixture corpus、wall を 16 に固定、`8be514bb4` で CI green）。目標状態：**ミッションクリティカル分野で Almide を使えると根拠をもって説明できること。** 各項目に測定可能な終了条件を設け、それがない項目は載せない。

規模：S（≤1 日）/ M（≤1 週）/ L（≤1 か月）/ XL（四半期以上）。

<a id="tier-0--semantics-must-stop-moving-everything-else-builds-on-this"></a>

## Tier 0 — 意味論の変更を止める（ほかのすべての土台）

- [x] **A0-1 (#530 + #1485 + #1483) (L) 仕様凍結 + 互換性契約。**
  DECLARED 2026-08-20（4997c7535、owner 指定順序の ADR-0012 D2/D3 完了後）：docs/STABILITY.md — 凍結 surface（LLM 向け = stable）、ALS 適合条項（「実装が間違っている」）、破壊的変更方針（major + @dialect 引上げ + migration note）。proofs/stability-closure.toml — 数値を見る前に N を固定した六条件の stability 基準（#1485 CLOSED）を毎 push 報告する。残件の消化は担当項目に残す：#530（文章の矛盾をゼロに）、#1483（双方向 surface 全点検 + experimental marker）。0.57.2 自体には次の記録がある。DECIDED 2026-08-20（owner）：凍結は GO、ただし ADR-0012 の error surface 最終形が導入された後に順序づける。まず #1193（D2：`-> T!E` が fallibility marker に型付き error を保持）、次に #1194（D3：fmt が戻り値位置 Result を `T!E`/`T!` に正規化）。fmt の標準形が凍結対象 surface を変えるためである。順序：#1193 -> #1194 -> ALS 凍結宣言 + 破壊的変更方針（変更 = 明示的 major）+ #530/#1485/#1483 の完了 draft。
  二つの挙動変更がある。patch レベルの意味変更は、この市場に対する言語の適格性を失わせる。終了条件：ALS 凍結文書、破壊的変更方針（廃止までの周期を設け、決して黙って変えない）、release gate が強制する 1.0 条件 checklist。

<a id="tier-1--close-the-verification-holes-compiler-correctness"></a>

## Tier 1 — 検証の穴を塞ぐ（compiler の正しさ）

- [x] **A1-1 (#1226) (L) Interp の tag 付き heap 範囲（#1226 slice 2）。** HEAP_BOUNDARY による棄権 = 115/501 fixture。各棄権は共通 frontend bug が 2-0 の投票で通る場所である（E0004 系統がまさにこの形）。終了条件：棄権率 < 5%（投票 ≥ 476/501）。
  DONE 2026-08-19（#1226 closed）：棄権 135→27 行 = corpus の 4.6%、実際の三者投票 566/589（96.1%）、各増分で異議ゼロを維持。残り 23 棄権は interp-abstain-classes.toml に分類済み。
- [x] **A1-2 (XL, #1527) fuzz 頻度順の wall 解消。** subset wall は日常的な code に存在する。観測頻度順（30 分 fuzz run ごと）に解消：List 引数実体化（約98×）、呼出しを含む分岐を持つ解決不能な `if`（約59×）、registry 不足 `result.flatten/to_list/filter`、`list.zip_with`（約78×）、scalar-binding subset（約25×）、次に固定した 16 specimen（3 段 nesting、Map 値の map、tuple-list equality、heap-acc fold）。終了条件：30 分 fuzz run の wall < 100、かつ各対応完了で specimen を同じ PR に契約下の投票 fixture として導入する（縮小のみの規則は既存）。
  CLOSED 2026-09-06（#1527）：2026-09-06 nightly で **30 分あたり 64 wall**（8 × 295 s、11,129 program 中 84 wall）、finding 0。walled-real ratchet は恒久的な 0 を維持し、すべての対応完了が契約下に入る。
- [x] **A1-3 (L, #1528) negative test を 10× に（test-surface-25x tier 1）。** diagnostic pair 99 に対する Rust の約15k が成熟度の最大の差。終了条件：全 E-code に各 hint variant と fix-it verdict をカバーする fixture 系統、≥1,000 pair、fixture が < 3 の E-code を coverage gate が列挙する。
  PROGRESS 2026-08-18：**749 pair**（wave-2 の 730 + #1518/#1521/#1486/#1515 系統）。新 E-code 三つ（E058/E059/E060）を系統とともに導入。
  CLOSED 2026-09-02（#1784）：**ちょうど 1,000 pair**。複数 file fixture（`almide.toml` + `src/*.almd` の兄弟 file）で E033/E420 の例外を廃止し、全 code を coverage gate が強制する ≥3 系統基準の対象とした。
- [x] **A1-4 (M, #1529) RC 配置 snapshot（koka parc 方式）。** RC が重要な corpus（koka_parc* 系統を種にする）について、RC 挿入後の drop/dup 配置を runtime 結果と並ぶ期待出力として commit し、今日は無害な配置移動も目立つようにする。終了条件：≥ 20 形状の CI snapshot gate。
  DONE 2026-08-19（#1529 closed）：21 形状 snapshot gate（tests/rc_placement_snapshot_test.rs）、drop 順序 mutant の検出証拠。
- [x] **A1-5 (M, #1530) heap 上限による leak harness（grain makeGcProgram 方式）。** 両 target に厳格な heap 予算設定を設け、各 RC fixture を N 単位と N-1 で動かす。黙った leak が決定的 OOM になる。終了条件：CI で churn corpus を上限下で実行し、意図的に leak する control 一つで harness の検出力を示す。
  DONE 2026-08-20（#1530）：両 target に `almide build --heap-cap`（wasm は $alloc の frontier ceiling、native は live-bytes global allocator、両者とも規定 C-197 abort 形状）。harness tests/heap_cap_test.rs — churn corpus は上限下でも影響なし、rc_dec 除去 mutant 4/9 が決定的 OOM、native cap=1 強制が稼働。
- [x] **A1-6 (M, #1531) allocation 数 assertion（roc alloc-count 方式）。** allocation counter surface + 正確な loop body 内の allocation 数を、assertion ごとの control program とともに設ける。終了条件：≥ 10 loop 形状で反復ごとの allocation がゼロと assert する gate。
  DONE 2026-08-19（#1531 closed）：出荷 WAT に対する static-zero gate（tests/alloc_count_gate_test.rs）、10 形状 + allocation する control 2 個。
- [x] **A1-7 (S) 登録済み diagnostic 三件。** #1509（`not (expr)` guard parse）、#1510（`t.0.1` float lexing hint）、#1511（fmt Option 正規化 E054 — fmt は合法 program 全体で全域でなければならない）。終了条件：三件すべて fixture 付きで closed。DONE 2026-08-18、v0.57.2 で出荷。

<a id="tier-2--prove-the-findings-have-dried-up-time-is-the-ingredient"></a>

## Tier 2 — 発見が枯れたことを示す（必要なのは時間）

- [ ] **A2-1 (M, #1532 + #924) fuzz nightly 強化 + 90 日連続 green。** まず drain の停止を修正する（停止した native cargo build が case ごとの timeout を生き延びる。runner は BUILD 段階を kill する必要があり、今週二 campaign が止まった）。次に seed を回す nightly run、finding 自動起票、連続記録 meter。終了条件：新たな correctness finding がゼロで 90 夜連続 green。
  PROGRESS：停止問題は 2026-08-18 に closed（#1532）。meter は #924。計数条件は 2026-09-21 に承認され、計画 fuzz 時間の ≥ 75% を実行した夜を数える（`shards=k/N`）。連続値を信頼するには先に #2390 の導入が必要。finding はもはや #924 へ入らない（#2379 が `fuzz-findings` label へ振り分ける）。
- [x] **A2-2 (M, #1533) 実 code acceptance tier。** 実際の下流 project（dfa、parsegen、そのほかの consumer）を CI の acceptance ring としてコンパイル・テストする。E0004 と #1501 の両系統は生成 code でなく実 code で発見された。終了条件：毎 develop push で CI が green の実 project ≥ 3。
  DONE 2026-08-20（#1533 closed）：Acceptance Ring workflow — 固定 ref の dfa/parsegen/svg を develop から build した compiler に対し毎 push/PR で実行し、test-file 下限と native-fallback 上限を設ける。両方の kill 証拠があり、初回から green。teastia（private）は read token が用意できた時点で加える。
- [x] **A2-3 (S) 新しい検出観点を加える周期。** 四半期ごとに一つの新観点を加える（今回：reference-suite 移植、panic-wash、nested-type matrix）。#1508 backlog（or-pattern、string pattern、NaN-bits）を feature 導入時に使う。終了条件：四半期ごとの常設 roadmap 行。新観点で finding ゼロの四半期が、この市場で求める「枯れた」証拠になる。2026-Q3 割当超過（2026-08-18）：pass 分離 semantic-preservation gate（#1487、mutant 検出検証済み）、perf ablation 部分（#1466）、cap-effect consistency gate（#1515）、seed 付き regex 差分（同日に初の CI kill を確認）。2026-Q4 に繰り返す。

<a id="tier-3--outside-the-compiler-no-issue-fixing-moves-these"></a>

## Tier 3 — compiler の外側（issue 修正だけでは進まない）

- [x] **A3-1 (#1534) (S) security 態勢の最低水準。** disclosure 窓口のある SECURITY.md、CI の dependency lock audit（MVS+lock 作業を種にする）、release signing 方針。終了条件：文書化して README から link。
  CLOSED 2026-08-31（#1534）：`SECURITY.md` を merge（#1636）。演習で既知脆弱 pin（RUSTSEC-2020-0071）に dependency audit が red となり、全 release asset は Sigstore attestation 付き（`release.yml`）。
- [x] **A3-2 (#1535) (M) support 契約。** LTS 方針（修正対象 version と期間）、versioning 保証、maintainer 体制について正直な bus-factor 説明。終了条件：SUPPORT.md 承認。
  CLOSED 2026-08-27（#1535）：`SUPPORT.md` — 各保証は、それを強制する仕組みを明示する。
- [ ] **A3-3 (XL) qualification 資料一式（認証対象業界を狙う場合のみ）。** 既存証拠（三者差分 oracle、295 contract 台帳、gate 検証の列挙、wall の正直な扱い）を tool-confidence の論証へ対応づける（IEC 61508 / ISO 26262 TCL の語彙）。基盤は非常に強い素材であり、資料作成は新 code ではなく執筆と監査になる。終了条件：review 済み qualification dossier 骨子。
- [ ] **A3-4 (XL) 本番導入の段階。** 非 critical な内部 tool → 監督付き pilot → mission-critical。各段階に incident SLO を設ける。終了条件：最初の外部本番導入で post-mortem のない四半期。

<a id="sequencing"></a>

## 順序

A0-1、A1-1、A1-2 が critical path であり、「verified」の意味そのものを決める。A1-3..7 と A2-1..2 は自由に並列化できる。Tier 3 は今始め（A3-1 は一日）、ほかと並行する。正直な予定は Tier 0–2 が集中作業で一〜二四半期、A3-4 の段階導入は市場次第、A3-3 は対象業界が認証を要求する場合のみ。

<a id="standing-metrics-check-each-quarter"></a>

## 常設指標（毎四半期確認）

| 指標 | 現在（2026-08-18）| mission-critical 基準 |
|---|---|---|
| 三者投票率 | 76.6% | ≥ 95% |
| 30 分 fuzz run あたり wall | ~500 | < 100 |
| diagnostic fixture pair | 99 | ≥ 1,000 |
| fuzz green 連続記録 | 2 rounds | 90 nights |
| CI 内の実 project | 0 | ≥ 3 |
| 未解決 miscompile 系 bug | 0 | 0（維持）|
