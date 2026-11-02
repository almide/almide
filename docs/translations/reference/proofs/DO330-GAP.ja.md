<a id="do-330-gap-analysis--the-consolidated-stage-5-document"></a>

# DO-330 ギャップ分析 — Stage 5 の統合文書

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](DO330-GAP.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](DO330-GAP.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](DO330-GAP.zh-CN.md)

> [正本](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/proofs/DO330-GAP.md)（固定コミット 35297a612fb29e3f78f6587aba253aa91d51f466）の非規範的な履歴参照用スナップショット。正本が優先され、この写しは自動同期されず、生成器や証明の入力ではない。コマンド、証明、測定、資格認定評価を独立に実行していない。出典参照、生成マーカー、日付付き数値は履歴資料として保持し、認証・資格認定・当局の受理・公開の証拠とはしない。

段階計画が求める 1 ページ：既存のゲートをツール資格認定の目標へ対応付け、形式化した 3 つの成果物と、残る不足を列挙する。これは検証手段に裏付けられた出典をまとめた索引であり、各主張には別の場所にゲートがある。以下のすべての `source:` 行は解決できなければならない（`scripts/check-tor-refs.sh` はこのファイルも走査する）。全体を通して Ferrocene（Rust の ISO 26262/DO-178C 資格認定）を先行事例のモデルとする。

<a id="1-the-qualification-argument-mapping"></a>

## 1. 資格認定の論証（対応付け）

DO-178C Table-A の目標対応表と DO-330 TQL の論証は G-F5 の設計にある。提案する論証は、独立した出力検証によって信頼しないコンパイラの資格認定負担を減らし、チェッカーに焦点を当てる。これは評価すべき資格認定の論証であり、割り当て済み TQL や形式手法クレジットの付与ではない。適用される目標と受理される証拠は、申請者と認定当局が判断する。
source: docs/roadmap/active/flight-qualification.md

主張の強さの概観（測定済み、乖離ゲートあり）：
source: proofs/STAGE-STATUS.md

<a id="2-the-three-formalized-artifacts-goal-directed-trio--all-instrument-backed"></a>

## 2. 形式化した 3 成果物（目標に沿う 3 点 — すべて検証手段に裏付けられる）

| 成果物 | 所在 | 強制手段 |
|---|---|---|
| ツール運用要求 | proofs/TOR.md（現行 TOR 行） | check-tor-refs.sh：各要求がそれを強制する検証手段を維持する |
| 検証ツールの検証 | proofs/gate-verification.toml | check-gate-verification.sh：各ゲートを失敗の仕方で分類。UNVERIFIED は減少のみ |
| 既知問題の形式化 | proofs/releases/（リリースごとの seal） | release-seal.sh：`known_problems` は必須、派生フィールドはタグに対して永続的に再測定される |

source: proofs/TOR.md
source: proofs/gate-verification.toml
source: scripts/release-seal.sh

<a id="3-remaining-gaps-enumerated-owned"></a>

## 3. 残る不足（列挙済み、担当あり）

| 不足 | 状態 | 担当／参照 |
|---|---|---|
| MC/DC と分岐カバレッジ | ラチェット管理する行／条件カバレッジと safety-set の判断ベクトルは存在するが、分野レベルのカバレッジ・クレジットは未評価 | flight-evidence-gaps F2 残項目 |
| C-WCET（counted-loop の要石 あ） | receipt 設計では PENDING | flight-reference-app Slice 2 |
| C-FAITHFUL と Ferrocene 経路（要石 い） | structural-runtime の部分的な精緻化。チェーン内に資格認定済み native バイナリ・ツールチェーンはない | flight-reference-app Slice 3。Ferrocene へのアクセスが必要 |
| wasmtime の資格認定上の来歴 | 未認定の COTS、手続的な TOR-9 | #865 |
| ALS 構文要素の記述 | 減少のみのラチェット。現在の件数は台帳の `unwritten_ceiling` と proofs/STAGE-STATUS.md | proofs/als-element-coverage.toml（凍結の前提条件） |
| 持続性の証拠 | ledger-counts.yml が毎夜 streak を再測定。現在の件数は proofs/STAGE-STATUS.md | research/benchmark/fuzz-green/（90 日のマイルストーン） |
| プロセスの独立性 | ratchet-separation ゲートは存在する。人間／組織のレビュー分離は組織レベルの作業 | flight-evidence-gaps F5 の完了注記 |
| PSAC／認証計画 | 計画文書はまだない | flight-qualification §G-F6（dossier テンプレート） |
| 外部評価そのもの | 台帳上の事実ではなく、市場／認定当局に関する事実 | 定義上、リポジトリの範囲外 |

source: docs/roadmap/active/flight-evidence-gaps.md
source: proofs/als-element-coverage.toml

<a id="4-what-this-page-is-not"></a>

## 4. このページが主張しないこと

資格認定、採用、完全性の主張ではない。評価者が出発点にする率直な棚卸しである。再生成できる数値はコピーではなく参照先として含めることで、黙って乖離しないようにする。
