# Road to 1.0 — 0.41 → 0.99 バージョンラダー

> 現在 0.40.2。ここから 1.0 までの全 minor を decade（0.4x / 0.5x / …）単位のアークに割り、
> 各 minor に「顔となる大機能」を 1 つずつ載せる台帳。open issue 40 件は全てこの台帳のどこかに割付済み。

## 読み方と運用ルール

- **decade = アーク**。テーマと出口ゲートを持つ。decade 境界（0.50, 0.60, …）はゲート通過のチェックポイントリリース。
- **各 minor = 大機能 1 つ**。パッチ（0.41.x）は自由。decade 内で番号が前後にずれるのは構わない — 不変条件は decade ゲートであってバージョン番号ではない。
- **順序の根拠**は依存関係:
  1. **計測器が先** — fuzz が死んだまま codegen を触らない（0.4x）
  2. **編集ループが規模非依存になってから** dogfood でスケール実測（0.4x）
  3. **wasm 対等性と表現コスト**を、生きた oracle の上で直す（0.5x）
  4. **cranelift（rustc 独立）**は差分 oracle が常時緑になってから（0.6x）
  5. **証明と Critical プロファイル**は codegen が安定してから（0.7x）
  6. **資格化キット**は証明の上に積む（0.8x）
  7. **仕様凍結は最後** — 全部が動いてから規範化する（0.9x）
- 新 issue を立てたら、この台帳のどこかの decade に割付ける（同 PR で）。載らない issue を作らない。

---

## 0.4x — 計測器と編集ループ

silent-wrong-code の計測器（fuzz nightly）を復活させ、編集ループをプロジェクト規模に依存しない形にする。

| Version | 大機能 | Issue |
|---|---|---|
| 0.41 | fuzz-nightly を毎夜使える計測器に戻す（20 夜中 1 夜の根治）。perf scoreboard/ratchet（#917）の close-out | [#924](https://github.com/almide/almide/issues/924), [#917](https://github.com/almide/almide/issues/917) |
| 0.42 | fuzz true green — 残 findings 0 + 連続緑 2 夜 | [#796](https://github.com/almide/almide/issues/796) |
| 0.43 | 単一ドライバ — フロントエンド 1 回実行、手同期ドライバシーケンス 6 → 1 | [#925](https://github.com/almide/almide/issues/925) |
| 0.44 | feature-gated runtime（http/zlib）の rtlib 化 — fresh dir の 8.4s 初回ビルド解消 | [#1002](https://github.com/almide/almide/issues/1002) |
| 0.45 | 10k 行 dogfood プロジェクト着工 — スケール主張を実測に変える | [#1001](https://github.com/almide/almide/issues/1001) |
| 0.46 | モジュール単位コンパイルキャッシュ（module rlib + typed-IR cache） — dogfood がトリガー（フルビルド 2-3s 超）を踏んだら | [#1003](https://github.com/almide/almide/issues/1003) |
| 0.47 | クエリ/インクリメンタル基盤 phase 1 — LSP を per-keystroke 全再解析から解放 | [#928](https://github.com/almide/almide/issues/928) |
| 0.48 | クエリ基盤 phase 2 — ビルドパイプライン本体をクエリ上に | [#928](https://github.com/almide/almide/issues/928) |
| 0.49 | build-speed / runtime-perf / safety 三点セットの実測数字を README に載せ切る | [#999](https://github.com/almide/almide/issues/999) |

**Gate 0.50**: fuzz 連続緑が常態 / dogfood フルビルドがキャッシュ効きで 2-3s 未満 / 三点の数字が public かつラチェット管理。

## 0.5x — クロスターゲット対等性

wasm leg を native と同格に。最適化品質の乖離（#929）は v0 退役時に構造的に生まれた負債であり、ここで返す。

| Version | 大機能 | Issue |
|---|---|---|
| 0.51 | QualifiedRef newtype — v1 MIR 上で bare type identity を表現不能にする（#433 クラスの型による根絶） | [#908](https://github.com/almide/almide/issues/908) |
| 0.52 | concurrency モデルの立場決定と文書化 — structured concurrency vs data-parallel-only、cross-target 契約への含意 | [#1000](https://github.com/almide/almide/issues/1000) |
| 0.53 | wasm leg に nanopass optimizer 群を接続 | [#929](https://github.com/almide/almide/issues/929) |
| 0.54 | wasm SIMD | [#929](https://github.com/almide/almide/issues/929) |
| 0.55 | hole-hunt レンズ — pass-ordering / checker-accepts-but-lowering-reinterprets / 診断乖離 / host-env 依存 | [#912](https://github.com/almide/almide/issues/912) |
| 0.56 | RcCow 表現コスト phase 1 — allocation-heavy 文字列ワークロードの対 Rust ~1.7x を解剖・縮小 | [#1004](https://github.com/almide/almide/issues/1004) |
| 0.57 | RcCow phase 2 — 対 Rust ギャップをラチェット下に | [#1004](https://github.com/almide/almide/issues/1004) |
| 0.58–0.59 | バッファ — dogfood と hole-hunt の findings 焼却 | — |

**Gate 0.60**: 両ターゲットの最適化品質が同格 / hole-hunt findings 0 / 対 Rust perf ギャップが計測・ラチェット管理下。

## 0.6x — rustc からの独立（debug ビルド）

cranelift direct native emit のエンドゲーム（#1005）。0.4x で復活させた fuzz oracle と、新設する cranelift-vs-rustc 差分ゲートが安全網。

| Version | 大機能 | Issue |
|---|---|---|
| 0.61 | cranelift spike — scalar core の MIR → CLIF | [#1005](https://github.com/almide/almide/issues/1005) |
| 0.62 | heap / RC 演算 + closure | [#1005](https://github.com/almide/almide/issues/1005) |
| 0.63 | rtlib リンクと stdlib 全面カバー | [#1005](https://github.com/almide/almide/issues/1005) |
| 0.64 | 差分ゲート — cranelift leg vs rustc leg の挙動 oracle を CI 常設 | [#1005](https://github.com/almide/almide/issues/1005) |
| 0.65 | debug ビルドのデフォルトを cranelift に切替（release は rustc 継続） | [#1005](https://github.com/almide/almide/issues/1005) |
| 0.66–0.69 | バッファ — 編集ループ最終形（check → run 体感即時）の磨き込み | — |

**Gate 0.70**: `almide run` の debug パスから rustc が消滅、フル oracle 緑のまま。

## 0.7x — 証明の深化と Critical プロファイル

codegen が安定した上に、証明のカバレッジを runtime まで広げ、認証可能サブセットを機械検査にする。

| Version | 大機能 | Issue |
|---|---|---|
| 0.71 | Coq allocator 証明の残り sentinel 不変条件 — region reset + PINNED_RC | [#909](https://github.com/almide/almide/issues/909) |
| 0.72 | Lean 証明カバレッジを runtime へ — allocator / free-list / RC 演算 | [#576](https://github.com/almide/almide/issues/576) |
| 0.73 | ビルド毎 translation-validation 証明書 + verified checker | [#570](https://github.com/almide/almide/issues/570) |
| 0.74 | `almide check --profile critical` — 認証可能サブセットの機械検査 | [#567](https://github.com/almide/almide/issues/567) |
| 0.75 | static memory mode + partitioned-runtime 互換 | [#568](https://github.com/almide/almide/issues/568) |
| 0.76 | WCET 解析可能な codegen（Critical プロファイル） | [#569](https://github.com/almide/almide/issues/569) |
| 0.77 | コンパイラ構造カバレッジの計測とラチェット、safety pass の MC/DC | [#566](https://github.com/almide/almide/issues/566) |
| 0.78 | wasm Critical 出力の qualified 実行環境 | [#865](https://github.com/almide/almide/issues/865) |
| 0.79 | DO-333 formal-credit マッピング — 証明済み性質ごとの認証クレジット | [#575](https://github.com/almide/almide/issues/575) |

**Gate 0.80**: 「critical profile でコンパイルが通る = 証明書が付く」が全て機械検査で成立。

## 0.8x — 資格化キットの製品化

証明の山を、第三者に渡せる製品（qualification kit）に変える。

| Version | 大機能 | Issue |
|---|---|---|
| 0.81 | 生成 Rust の行レベル source traceability | [#572](https://github.com/almide/almide/issues/572) |
| 0.82 | tool-qualification データパッケージ（Almide as a code generator） | [#574](https://github.com/almide/almide/issues/574) |
| 0.83 | Ferrocene を Critical プロファイルのネイティブバックエンドに | [#573](https://github.com/almide/almide/issues/573) |
| 0.84 | リリース毎の署名付き qualification dossier 自動生成 | [#571](https://github.com/almide/almide/issues/571) |
| 0.85 | service-history / problem-reporting 記録基盤 | [#584](https://github.com/almide/almide/issues/584) |
| 0.86 | flight reference app — PID 制御則カーネルが `make verify` を end-to-end で通過（G-F4） | [#776](https://github.com/almide/almide/issues/776) |
| 0.87–0.89 | バッファ — dossier の実戦投入フィードバック反映 | — |

**Gate 0.90**: reference app + dossier のセットを第三者にそのまま渡せる。

## 0.9x — 規範仕様と 1.0 エンドゲーム

全部が動いてから規範化する。仕様凍結が最後なのは意図的 — 凍結は完成の宣言であって願望ではない。

| Version | 大機能 | Issue |
|---|---|---|
| 0.91 | almide-interp を第三審から規範意味論（normative semantics）へ昇格 | [#564](https://github.com/almide/almide/issues/564) |
| 0.92–0.95 | ALS 規範言語仕様（CG-1）章別完成 — 文法 / 型システム / 意味論 / stdlib 契約 | [#530](https://github.com/almide/almide/issues/530) |
| 0.96 | 仕様凍結 — 構文・演算子・stdlib 境界の最終監査、edition 方針発効 | — |
| 0.97 | LTS / versioning コミットメント発効（multi-decade support policy） | [#578](https://github.com/almide/almide/issues/578) |
| 0.98 | 既知乖離ゼロ監査 — contract ledger `flagged-for-revision` 0 / wall 0 / claim-drift 0 の最終確認 | — |
| 0.99 | RC 硬化 — 全ゲート緑のままフリーズ。残タスクは 1.0 リリースのみ | — |

**Gate 1.0**: 下の「1.0 の定義」が全項目成立。

---

## プログラムトラック（バージョン非拘束）

組織・事業側の里程標。コンパイラのバージョンには載らないが、0.8x の資格化キット製品化と並走する。

| Issue | 内容 | 目安 |
|---|---|---|
| [#586](https://github.com/almide/almide/issues/586) | flight-grade program tracking | 全期間 |
| [#585](https://github.com/almide/almide/issues/585) | 機械生成コードの認証に関するポジションペーパー公開 | 0.5x–0.6x（早いほど良い） |
| [#583](https://github.com/almide/almide/issues/583) | deployment ladder の定義（ground → space/UAS → 上位保証） | 0.7x |
| [#579](https://github.com/almide/almide/issues/579) | 認証当局リレーション（DER / TÜV エンゲージメント） | 0.7x–0.8x |
| [#577](https://github.com/almide/almide/issues/577) | dossier に署名し責任を負える法人の設立 | 0.8x（1.0 までに必須） |
| [#580](https://github.com/almide/almide/issues/580) | solo-plus-agents を超える qualification エンジニアリング体制 | 0.8x |
| [#581](https://github.com/almide/almide/issues/581) | trust-market 収益による qualification climb の資金化 | 0.8x–0.9x |
| [#582](https://github.com/almide/almide/issues/582) | 最初の regulated-adjacent デプロイ（service-history クロック開始） | 0.9x（1.0 までに必須） |

## 1.0 の定義

- **仕様**: ALS が規範（#530）、almide-interp が規範意味論（#564）、仕様は凍結済み
- **正しさ**: fuzz 常時緑、hole-hunt findings 0、contract ledger flagged 0、全ビルドに translation-validation 証明書
- **速度**: 編集ループが規模非依存、debug パスに rustc なし、対 Rust ギャップは実測・ラチェット管理
- **信頼**: critical profile + qualification dossier が製品として渡せる、reference app が証拠
- **数字**: build-speed / runtime-perf / safety の三点が README で実測公開

## Issue 割付の完全性

open issue 40 / 40 をこの台帳に割付済み（バージョン行 32 + プログラムトラック 8）。
この台帳と issue リストの乖離は負債 — 新 issue は同 PR でここに割付け、クローズしたら行を消す代わりに issue リンクが closed になることで進捗が見える。
