<!-- description: v1 リリース路線 — opt-in 検証 codegen(v0 fallback) を beachhead に、カバレッジ→証明書 emit→flight-grade へ段階リリースする計画。4案のメリデメと選定理由、各段の受入基準。 -->
# v1 リリース路線 — 段階リリース計画

> 2026-07-05 策定。「v1 をはやくリリースしたい」に対し、**正しさ(honest-wall + PCC)は
> 既に保証済み**という事実を土台に、最速で「実体のある v1」を出す路線を選定した記録。
> **関連**: [v1-kgi-kpi](v1-kgi-kpi.md)（終局指標）/ [v1-v0-parity](v1-v0-parity.md)（③被覆）/
> [certificate-format-v1](certificate-format-v1.md)（③証明書）/ [flight-profile](flight-profile.md)（Gap 1 先）。

## 前提（これが全ての土台）

- **v1 trust-spine は「間違ったバイトを絶対に出さない」**（honest-wall）。lower できれば
  v0 と byte 一致、できなければ `try_render_wasm_source` が `Err` を返して wall する。
  MISMATCH=0 / RUNERR=0 を常時維持。PCC(ownership/caps)ACCEPT。
- したがって **どのリリース路線でも miscompile リスクは無い**。差は「何が出せるか / 工数 /
  ユーザーに届く価値」だけ。

## 選定: ① opt-in 検証 codegen(v0 fallback) を beachhead に

**採用理由**: 今日出せて・リスク0・v1 が実在の製品機能になり、かつ ②③ の成果が同じ器の上で
自動的に価値化される。④(version tag) は ① を含む版を切ることで併用する。

## 4案のメリデメ（意思決定の記録）

### ① opt-in 検証 codegen（v0 fallback）★採用・進行中
`almide build/run --target wasm --verified` で v1 を試行 → 通れば PCC 検証済み v1 出力、
壁なら v0 に自動 fallback。default(flag 無し)は v0 のまま。

- **メリット**: 今すぐ出せる（統合＋flag、研究不要）／リスク0（壁は v0＝今日と同じ）／
  v1 が実際に使える製品機能になる／opt-in で既存ユーザー無影響／byte-binding 証明(Gap 1)が
  来たら同じ経路が flight-grade に昇格。
- **デメリット**: 全関数が subset 内の時だけ v1 が発動（whole-program dispatch）→ 実プログラムは
  当初 v0 fallback が多く**発動機会が少なく地味**／関数単位混在は ABI/runtime 共有で難しい／
  **信頼の本体(証明書＋checker)はまだユーザーに露出しない**（実行側＝byte一致 wasm は出るが
  検証ストーリーは③まで未露出）。
- **工数/リスク**: 中／正しさリスク低・価値リスク中。

### ② 実プログラムを端まで通す（カバレッジ拡大）— 次段
壁を開け続け、実 repo が v1 単独で lower するまでカバレッジを上げる（fallback 不要域の拡大）。

- **メリット**: 完成域では「v1 が実プログラムを単独コンパイル」＝強く正直なマイルストーン／
  Gap 3(実プログラム被覆)を直接前進／① の器の上で自動的に「v1 発動域の拡大」として価値化。
- **デメリット**: 遅い／終端が見えにくい（1関数でも壁だと whole-program は v0 fallback）／
  残件に **capturing closure = 証明モデルの根本制約**（[closure-architecture-v2](closure-architecture-v2.md)、
  証明拡張＝研究）が混ざる／これ単体では何もリリースされない（①③で製品化が要る）。
- **受入基準の例**: 対象 repo の全 fn が v1 で lower（壁ゼロ）→ `--verified` で v0 fallback が
  一度も発火しない。現状の残壁: Set（`Option/Result[Set]` は self-host 化可、`some(set)` は
  construction 確認済み）／guards ／一般 Map（非String値の top-level・`Map[Int,Int]`/`Map[String,String]`
  は self-host 化可）／capturing closure（根本制約）／tuple-inner・record-inner interp（open-shape
  aggregate = per-shape reader 要）。**進捗は [v1-kgi-kpi](v1-kgi-kpi.md) の「攻める KPI: 言語面被覆」に集約**。

### ③ 証明書 emit を製品化 — その次
`almide build --certify` で wasm＋証明書を出し、数百行 checker が再検証（README の v1 核心＝
「building is hard, checking is cheap」）。`emit_cert`（検証側の機械）は既に一部存在。

- **メリット**: これが v1 の唯一無二の価値（trust model の本体）／第三者が checker を回して
  「受理⟹安全性束」を体験できる＝信頼が可触化／① の verified 出力にそのまま証明書を添付できる。
- **デメリット**: 現状の証明書は**安全性束 4/8**（mem/name/cap/type ✅ ／ leak・reuse・call-mode・
  byte 残）で、**byte-binding(Gap 1「本丸」)はまだ「信頼のまま」**→証明書は正直だが不完全
  （「bytes refine source」= C-FAITHFUL は未証明）／完全な KGI-T は Coq byte-binding 証明＝深い研究／
  「4/8 を証明」と誤解なく打ち出す設計が要る（過大主張リスク）。
- **受入基準の例**: `almide build --certify` が `app.wasm` + `app.cert` を出力し、同梱 checker
  (`Print Assumptions` ⊆ 信頼底、0 sorry/0 Axiom)が受理⟹宣言した安全性束（当面 4/8、正直に明示）を
  再導出。**証明書形式は [certificate-format-v1](certificate-format-v1.md)**。

### ④ 現状をバージョンリリース — ① と併用
trust-spine は内部品質改善のまま、`Cargo.toml` を bump して tag（release workflow が release 生成）。

- **メリット**: 最速（数分）／直近の品質・被覆改善が出荷／リスク0。
- **デメリット**: 単体ではユーザーから見て「v1」が新たに使えるわけではない（純保守リリース）。
- **運用**: ① の統合を含む版を切ることで「v1 opt-in が使える最初のリリース」にする。以後 ②③ の
  各段でカバレッジ/証明書が伸びるたびに bump。

## 横断する正直な限界

- どの統合路線でも **v1 は「lower できる範囲」でしか発動しない**（②カバレッジが効く）。
- **完全な信頼主張(KGI-T)には Gap 1＝Coq byte-binding 証明が必須**で、これは最速路線でも研究工程。
  ①③ はその手前で「正直にスコープを切った」価値を先に出す設計。
- 航空(flight-grade)への接地は [flight-profile](flight-profile.md) / [flight-rust-ferrocene](flight-rust-ferrocene.md)
  （Gap 1 の wasm byte-binding を回避して Rust→Ferrocene で束縛する keystone (い)）を参照。

## 実行順（このドキュメントの結論）

1. **① を beachhead として即出し**（`--verified` opt-in、v0 fallback）＝進行中。
2. **④ で ① を含む版を切る**（「v1 opt-in 初リリース」）。
3. **② でカバレッジを上げる**（v1 発動域の拡大。残壁を一つずつ、根本制約は明示して回避 or 研究へ）。
4. **③ で証明書を露出**（安全性束 4/8 を正直に。trust model が可触化）。
5. **Gap 1(byte-binding 証明) / flight keystone** で flight-grade へ（研究工程、別台帳）。
