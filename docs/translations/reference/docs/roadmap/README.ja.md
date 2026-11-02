<a id="almide-roadmap"></a>

# Almide ロードマップ

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](README.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](README.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](README.zh-CN.md)

> 固定された生成済み索引を手動で組み立てた非規範読者用スナップショット。自動生成・同期される索引ではなく、正本は原文。生成command未実行で、状態・証明・性能・認証の独立再検証もしていない。99/32/15件と日付は原文時点。active/on-hold/doneの配置や短い主張は現行完了・普遍的正しさ・世界一・外部資格化を認定しない。各原文とreader注記で範囲を確認。リンクは固定原文を指す。
> [Original source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/README.md)

> 原文の生成履歴：元索引はディレクトリ構造から自動生成された。当初の更新commandは `bash docs/roadmap/generate-readme.sh > docs/roadmap/README.md`。このreaderは手動翻訳snapshotで、その生成物ではない。
>
> [GRAND_PLAN.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/GRAND_PLAN.md) — 5段階戦略
> [ROAD_TO_1_0.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/ROAD_TO_1_0.md) — 0.41 → 0.99 version 段階表

<a id="active"></a>

## 進行中

99 件

| 項目 | 説明 |
|------|-------------|
| [ADR-0013：native String を共有bufferにする — 実装](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/adr-0013-string-sharing.md) |  |
| [AlmidePerceusBelt](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/almide-perceus-belt.md) | 歴史的 Perceus belt 設計、廃止済み emit 型状態、範囲を限定した証明の根拠 |
| [全軸制覇への突破点 — アリーナと近隣比較から導いた設計指針](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/arena-breakthroughs.md) | 全軸制覇への突破点台帳 — 近隣アリーナの実測と 18 近隣コンパイラの機構読みから、どの軸で負けていて何を突破すれば世界一かを、勝利条件・ゲート・順序つきで固定する |
| [Async Inception — 論理時間を言語の時間基盤にする](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/async-inception.md) | 非同期の起点：主張、文法、意味論、証明、対外主張と計画を一つの憲章へ |
| [2026 年 async の主張 — 監査と引き離すための計画](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/async-world-claim.md) | 2026 年最高の async という主張の監査：軸、競合、引き離すための五手。 |
| [Behavioral Contract — 改変保存と批准性質を区別する設計案](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/behavioral-contract.md) | 挙動の証拠に関する提案。前版の保存と人間が批准した性質を、範囲付きのコンパイラ／ランタイムの主張から区別する。 |
| [ビルド速度：ランタイム rlib とホット関数のインライン化](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/build-speed-runtime-rlib.md) | Runtime rlib ビルド経路：現行 fat-LTO リリースリンクと歴史的インライン化実験 |
| [Camp-4 の一般化：heap-result `match` を認識型ではなく全域にする](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/camp4-general-heap-match.md) | lowering のみで heap-result match を認識型ではなく全域にする |
| [証明書形式 v1 — 設計](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/certificate-format-v1.md) | 関数ごとの所有権証明書形式 — i/a/d/m のアルファベット、呼び出し・分岐・閉包の拡張、および出荷済みの構成要素。 |
| [認証級への硬化](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/certification-grade.md) | 認証級への硬化 — 機械筆記ソフトウェアの信頼層に DO-178C / ISO 26262 / IEC 61508 の機構（仕様、トレーサビリティ、カバレッジ、ツール資格、dossier）を採用する |
| [CI クリティカルパス：測定基準、近隣調査、優先順計画（2026-09-21）](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/ci-critical-path.md) |  |
| [CI 警告の整理](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/ci-warnings-cleanup.md) | CI が容認するコンパイラ警告の継続台帳と、各分類をゼロにする計画 |
| [Closure のクロスターゲット完全性](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/closure-cross-target-completeness.md) | 歴史的な closure sweep、その後のアーキテクチャ文脈、証拠の限界 |
| [Codegen の走査完全性](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/codegen-traversal-totality.md) | 実装済み fold/lint の基盤と、その限定された coverage を伴う traversal-totality 設計。 |
| [構築による完全性](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/completeness-by-construction.md) | Perceus / borrow checker 型の機械的な完全性保証を Almide 自身のパスへ取り入れる設計ノート。 |
| [並行性の立場：決定的データ並列](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/concurrency-stance.md) | 並行性モデルの決定と、それによって確定する fan 系の修正 |
| [継続改善台帳](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/continuous-improvement.md) |  |
| [正しさ保証の欠落](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/correctness-guarantee-gaps.md) | 型が正しい source から正しい binary までの鎖で、機械的または数学的な証明がまだ欠けている層 |
| [クロスターゲット完全性（蓋）](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/cross-target-completeness.md) | 歴史的な複数ターゲット残件解消戦略、日付付き一覧と差分証拠の限界 |
| [決定性／純粋性 Belt](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/determinism-belt.md) | 決定性 belt の歴史的設計、廃止した type-state 境界、現行証拠の限界 |
| [決定的な上限 — byte-identity の下で計算の上限を合法化する](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/deterministic-bounds.md) | byte-identity 下で fan.timeout を合法化する — fuel、決定的配分、勝者順序、effect 隔離 |
| [edit-locality 理論 — Almide が証明するために存在する定理](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/edit-locality-theory.md) | 編集の影響範囲に上限を設ける設計直感を、証明済み定理に変える |
| [effect 検査ハーネス](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/effect-inspection-harness.md) |  |
| [capability ベースの effect system](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/effect-system-capability.md) | sandbox 化した AI agent container の capability ベース effect system |
| [青写真 — 約27個の effectful / raw-pointer stdlib 関数を v1 で FUNCTIONAL にする](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/effectful-27-blueprint.md) | effect/raw-pointer の青写真。byte テスト済み tier は全入力の意味論的証明ではない。 |
| [Execution Inception — as-if 規則を、並行と target の向こうまで完成させる](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/execution-inception.md) | 8月の実行基体調査と当初計画。現行 ADR と component host への参照を付す。 |
| [Fan v2 — 参照例](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/fan-v2-examples.md) | fan v2 表面の参照例：すべての head、境界意味論、診断 |
| [Fan v2 — 実行ポリシー設計・移行記録](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/fan-v2.md) | Fan v2 設計・移行履歴。現行選択表面は ADR-0028 と SPEC に従う |
| [Flight Evidence Gaps — 実地監査所見台帳](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/flight-evidence-gaps.md) | 実地内部監査所見（2026-07-03）：DAL-A の哲学と DAL-A の証拠との測定された距離を、是正作業と受入基準付きの7所見として記録 |
| [Flight-grade：組織と実績の段階](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/flight-organization.md) | Flight-grade のセクション B と C：法人、対外折衝、資金、顧客 |
| [飛行プロファイル — 航空品質への接地分析と2つのキーストーン](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/flight-profile.md) | DO-178C 飛行品質のギャップ分析。6本柱の状態と2つの工学的キーストーン。 |
| [飛行資格化 — DO-178C/DO-330/DO-333 マッピング + 資格化キット(G-F5 + G-F6)](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/flight-qualification.md) | 資格化証拠マッピングとキット設計の提案。本書は当局に受理された credit や資格化済みツールを確立しない |
| [飛行リファレンスアプリ — PID 制御則カーネル + make verify + receipt(G-F4)](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/flight-reference-app.md) | 飛行 gate G-F4 — `make verify` を端から端まで通る参照アプリ（counted sim loop 上の固定小数点 Q16.16 PID 制御則 kernel）、7段検証 pipeline（既存と keystone 待ち）、出す receipt（C-SAFE/C-PROVEN は緑、C-WCET/C-FAITHFUL は未了）、リスク低減順（Slice 0 scalar-no-print は現時点で緑 → Slice 1 print=G-F0 frontier → Slice 2 keystone あ が C-WCET を解放 → Slice 3 keystone い が Ferrocene を解放）。 |
| [飛行キーストーン (い) — v1 MIR → Rust → Ferrocene(忠実性束縛)](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/flight-rust-ferrocene.md) | native renderer と Ferrocene 資格化経路の提案。抽象 RC 補題は Almide→Rust 翻訳を証明しない。 |
| [Almide 飛行プロファイル — サブセットの原提案](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/flight-subset-spec.md) | 飛行サブセットの原提案。規範的な有界プロファイルの正本は ALS §B であり、歴史的な @flight 草案ではない。 |
| [飛行キーストーン (あ) — counted loop と確保上限の提案](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/flight-wcet-loops.md) | counted-loop witness と確保回数上限の提案。target 固有の物理 WCET は別義務のまま。 |
| [Fuzz 所見の選別：夜間差分 gate を再び成功へ](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/fuzz-findings-triage.md) | 歴史的な夜間差分 fuzz の選別。所見、修正、撤回された仮説を保持 |
| [整数リテラルの域 — 言語横断調査と Almide の目標](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/integer-literal-domain.md) | 他言語での整数リテラル範囲検査と Almide の着地点 |
| [StringInterpは特別な構文ではない — `concat + to_string(part)`へdesugarする](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/interp-is-desugar-to-tostring.md) | 歴史的な補間の脱糖判断と、範囲を限定した正しさの根拠 |
| [未解決issue一覧、2026-09-21 — 共通headerを付けた78件](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/issue-inventory-2026-09-21.md) |  |
| [Issue Ledger Burn-down — 完全性キャンペーンの残量計](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/issue-ledger-burndown.md) | issue 台帳の残量計：未解決 issue 数を残作業の正直な指標に保つ |
| [#1437: lift された carrier の ABI — 宣言駆動の、読み取れる単一レイアウト](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/lifted-carrier-abi.md) | #1437：生成側のずれに代わる、宣言駆動の単一 carrier layout |
| [LLM-first 言語](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/llm-first-language.md) | dojo MSR で測り、LLM が最も正確に書ける言語を目指す計画 |
| [論理時間 Async — async 文法設計](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/logical-time-async.md) | async 文法。fuel を論理時計とし、決定的 race と oracle 層を持つ。 |
| [論理時間 Async — 実装の青写真](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/logical-time-implementation.md) | 実装の青写真：Op::Charge、fuel ABI、計量クローン、race lowering、ゲート。 |
| [論理時間非同期 — 証明台帳](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/logical-time-proofs.md) | 論理時間非同期意味論の証明台帳：定理、Lean の核、モデル gate |
| [Map / Set のデータ構造 roadmap](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/map-data-structure-roadmap.md) | Map / Set データ構造ロードマップ。却下した seq-in-entry 設計と誤りの理由を含む |
| [monkey 回帰テスト群の復活：`spec/` の実態](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/mir-caps-call-count-breach.md) | 2026年8月の monkey-suite caps 計数診断、回復した公開由来、未了の現行再現 |
| [ミッションクリティカル対応の攻略リスト](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/mission-critical-attack-list.md) | Almide をミッションクリティカル分野で根拠をもって利用可能にするための階層別リスト |
| [native：Box 化された（再帰 variant）フィールドでの入れ子 ctor／リテラルパターン](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/native-boxed-pattern-lowering.md) | 歴史的なネストした boxed-pattern 診断。実装済み native lowering と回帰への参照 |
| [Native Trust Spine — 単一 memory model としての Perceus（#764）](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/native-trust-spine.md) | almide build --target rust を wasm と同じ v1 Perceus MIR 経由にし、単一 memory model を両経路に使う |
| [embedded wasm レーンの net Unix ソケットと shm（暫定 ops 104..=116）](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/net-unix-shm-embedded.md) | net の Unix ドメインソケットと共有メモリファイルのための暫定 embedded host ops 104..=116、および ADR-0023 step 7 での廃止。 |
| [外部視点監査 2026-07 — 証拠層の負債バーンダウン](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/outside-review-audit-2026-07.md) | 2026-07-27 の5視点外部レビュアー監査：証拠層が v0→v1 移行に遅れ、正直さが逆転（内部文書は正直、外向け主張は誤り）。「誤主張ゼロ、全 gate が実在」への層別解消、issue #913–#932 |
| [Playground UI刷新](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/playground-revamp.md) | Playground UI刷新：ファイルタブ、共有URL、TS方式サンプルギャラリー |
| [0.29 後の改善棚卸し](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/post-0.29-improvement-sweep.md) | 0.29.0 後の改善走査：表面の修正から v0 wasm 退役まで、全未解決項目を issue 化して順序付ける |
| [Protocol：宣言された適合 + opt-in の `any P`](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/protocol-any-existentials.md) | 宣言された適合 + opt-in の `any P` 存在型 — Go の interface 値の使いやすさを、暗黙の充足や nil-interface の落とし穴なしに取り入れる。取り入れる価値のある Swift のアイデアはこの 1 つだけで、それ以外は取り入れない。 |
| [Receipt Logic — 受領書の論理](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/receipt-logic.md) | 信頼層の形式的基盤 — 受領書の論理：主張の型、脅威モデル、信頼基底、反証手続き、用途相対の完全性。 |
| [reconciliation follow-up — v0.28.0 で見送った develop 側の残件](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/reconciliation-followup.md) | v0.28.0 統合の追跡：0.28.1 向けに延期した develop コミット |
| [result-family-from-type — layoutは型の関数であり、名前はコードだけを選ぶ](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/result-family-from-type.md) | 呼出結果の layout を module/func 名テーブルでなく型から導出する |
| [Return-op の根絶：`!` の単一脱糖、位置別の群を削除](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/return-op-eradication.md) | ! 演算子を単一の脱糖にし、位置ごとの return-op 群を削除する |
| [rot-eradication — 劣化の分布と参照資料に裏付けられた対策](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/rot-eradication.md) | コンパイラの劣化の分布と、参照資料に裏付けられた対策 |
| [構築によるセキュリティ](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/security-by-construction.md) | セキュリティ分類台帳と記録された対策。残る欠落と主張範囲を明示 |
| [Self-host linking v2 — mono instance をリンクし twin matrix を廃止する](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/selfhost-link-v2.md) | Self-host linking v2 — renderer がすでに生成する単相化 generic 本体をリンクし、shim registry の型別 twin を廃止する |
| [シェルスクリプトの立場：構造化パイプライン、対話シェルは作らない](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/shell-scripting-surface.md) | スクリプトには Nushell のデータモデルを取り、対話シェルは構築しない |
| [標準ライブラリの再設計（3層）](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/stdlib-redesign.md) | 9言語と対照した標準ライブラリの3層再設計：規則、分類一覧、module ごとの役割表、全公開関数の判断、採用提案から生まれた issue |
| [テスト面積 25× — リファレンスコンパイラ規模への道](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/test-surface-25x.md) | 登録済みテスト表面を25倍に増やし、参照コンパイラの規模へ近づける |
| [Ticks インターフェース監査 — 求められる API の形が違うのか？](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/ticks-interface-audit.md) | timeout 型 API に対する ticks の監査。書けない数字の問題と 3 つの修正。 |
| [Trust Layer — 機械が書くソフトウェアの信頼層](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/trust-layer.md) | カテゴリ戦略：「機械が書くソフトウェアの信頼層」を勝ち取る。MWS Trust Levels、受領書、クリティカルパス |
| [型の Where 制約](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/type-where-constraints.md) | 既存 inline generic bound を超える where 節構文と型定義制約の提案 |
| [v0 wasm codegen：Try/Unwrap/Fanの早期returnによるheap leak](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v0-unwrap-early-return-leak.md) | 廃止済み v0 wasm emitter の Try/Unwrap/Fan 早期 return heap leak。同分類の歴史記録として保持 |
| [目標PROMPT — ADT brick 5（heap-field bind + recursive drop）を仕上げ、第1の足がかりを実現する](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-adt-brick5-goal.md) | 履歴ADT brick-5目標prompt、完了済みrecursive dropと却下された試行の記録 |
| [v1 ① — カスタム ADT（variant）を第一級の値にする](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-adt-value-model.md) | v1 値モデルでカスタム ADT（variant）を第一級の値にする |
| [V1 残件 — trust spine の残件一覧（優先度付き）](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-backlog.md) | v1 trust-spine の残件：健全性、drop 完全性、self-host 表面、wall |
| [v1 Bolt Backlog(AI-DLC 管理)](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-bolt-backlog.md) | v1 登攀向け AI-DLC Bolt 残件。camp/step を意図駆動・時間枠付き Bolt（意図/完了定義/gate/依存/状態）で表す。構築の防護策は goal-prompt 規律。各 Bolt の出口 gate は独立 review（reviewer agent + Trust Spine CI + 偏りのない二重 oracle corpus）。明示分岐は人間 Mob が決め、各頂上までの「あと何 Bolt」を追跡 |
| [v1 → develop 還流戦略](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-develop-reflow.md) | develop-v1 統合の2026年7月の歴史的提案。現在の実行待ち依頼ではない |
| [v1 ヒープ結果の `if`/`match` 実行 — 設計（リスク低減済み: Coq 変更なし）](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-heap-result-control-flow.md) | 最初の heap-result 制御フロー断片の履歴と、その後の lowering 文脈 |
| [v1 join の完全性と線形 checker](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-join-completeness.md) | bind 位置の join で継続の複製をなくし、証明済み checker を witness サイズに対して線形にする。 |
| [v1 KGI / KPI スコアボード](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-kgi-kpi.md) | 2026年6月の歴史的 KGI/KPI スコアボード。目標と測定済み・証明済み結果を区別。 |
| [v1 ループをまたぐ heap 所有権 — 案 C（cert-spine 拡張）、完全性の修正](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-loop-ownership-cert.md) | ループをまたぐ heap 所有権を証明書 spine の拡張にする。特殊事例でなく完全性の修正 |
| [Almide v1: MIR を唯一の真とする単一意味論アーキテクチャ](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-mir-architecture.md) | 単一意味論アーキテクチャ。所有権とレイアウトを MIR で一度決め、renderer はその決定を再現する。 |
| [組織のバイト検証 — 各リポジトリ自身のベクトルを両ターゲットで](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-org-byte-verification.md) | 組織全体の v0==wasm バイト検証走査と、それで見つかった wasm のバグ分類 |
| [v1 — parser-TCO の要点（真のリポジトリ横断 heap-result-expr の要点）](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-parser-tco-lever.md) | parser-TCO の要点：リポジトリをまたぐ heap-result 式の解放 |
| [Almide v1 Phase 1: MIR コア + 二レンダラ — 実装設計](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-phase1-mir-core.md) | 歴史的 MIR Phase 1 実装設計と、IR / MIR オラクルの区別。 |
| [v1 証明アーキテクチャ — 構想（着地形）](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-proof-architecture.md) | 提案された最終形の証明アーキテクチャ。現行実装と信頼基底は別途範囲を限定 |
| [v1 Proof Spine — #31 の進捗（全V / leak-freedom / 推移的caps / 抽出穴）](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-proof-spine-progress.md) | v1 proof-spine の進捗 — task #31（全V / leak-freedom / 推移的caps / 抽出穴）の証明済み範囲と正直な残作業。CapabilityReach.v の推移的caps定理を記録（2026-06-21）。 |
| [v1 — records 機能: svg 完全攻略（目標プロンプト）](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-records-svg.md) | records/svg の歴史的な取り組み、最終的な測定結果、およびネイティブフォールバックの例外 |
| [v1 リリース路線 — 段階リリース計画](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-release-path.md) | 2026年7月の歴史的 release 路線判断。zero-risk と資格化の過剰主張を訂正 |
| [v1 stdlib self-host — 機構の段階（Option / List構築 / closures）](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-selfhost-machinery.md) | 初期self-host機構キャンペーンの履歴。後続closure/drop作業と残るlinking提案 |
| [v1 セルフホスト print 基盤 — ③ 観測可能性の要石](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-selfhost-print-floor.md) | セルフホスト print 基盤 — セルフホストの残り全体を支える観測可能性の要石 |
| [v1 trust-spineの正しさの穴（敵対的掃引2026-06-27）](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-spine-correctness-holes.md) | v1 trust spineへの敵対的掃引（2026-06-27）と、そこで露呈した正しさの穴。 |
| [v1 System Map — 全体像(mermaid)](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-system-map.md) | 意図した v1 保証アーキテクチャと歴史的成熟度図。完成済み信頼鎖証明書ではない |
| [v1 TCO — 自己再帰末尾呼出し → スカラー状態ループ（yaml parser の要石）](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-tco-self-recursion.md) | 自己再帰末尾呼出しをスカラー状態ループへ — yaml parser の要石 |
| [V1 → V0 対等性 — 完成計画](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-v0-parity.md) | テストと witness による保証の制限を明示した、歴史的な incumbent-v0 対等計画 |
| [v1 動的 Value モデル — yaml の要石（経路 A：self-host + 1つの信頼された再帰 drop routine）](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-value-model.md) | 動的 Value モデル：self-host と1つの信頼された再帰 drop routine |
| [柱 C の拡張：Value RC を証明書の対象領域に取り込む](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/value-rc-cert.md) | Value の参照カウントを証明書の対象領域に取り込む |
| [WASM 参照カウント解放：所有権規律の問題解消](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/wasm-frees-ownership-discipline.md) | 所有権規律を機械的に扱える形にし、参照カウントによる解放の backlog を解消する |
| [WASM 所有権 emit 層の機械化 (Perceus drift の構造的封じ込め)](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/wasm-ownership-emit-mechanization.md) | 所有権 emit の事故履歴と実装済み測定基盤 |
| [WASMプラットフォームのフロンティア — core Wasm 3.0の外側](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/wasm-platform-frontier.md) | 歴史的 platform 調査と現行 core/component/embedded 経路への参照 |
| [ゲートの言語境界](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/zero-committed-shell.md) | Almide に属するゲートと bash に残すゲート、およびその境界の維持 |

<a id="on-hold"></a>

## 保留

32 件

| 項目 | 説明 |
|------|-------------|
| [almide-lander 経由の Almide 間 FFI](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/almide-to-almide-ffi.md) | almide-lander を使い、共有ライブラリ FFI 経由でコンパイル済み Almide ライブラリを Almide から呼び出す |
| [Almide UI — AlmideライブラリとしてのリアクティブWebフレームワーク](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/almide-ui.md) | 純 Almide ライブラリとして作る SolidJS 風 reactive UI framework |
| [API 差分と自動バージョン管理](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/api-diff-auto-versioning.md) | 出荷済み stdlib interface gate を超える、package 向け API 差分と version 助言 |
| [LLM ベンチマーク：次のフェーズ](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/benchmark-next-phase.md) | LLM benchmark Phase 2–3：言語間比較、難しい問題、公開 |
| [コンパイル時契約](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/compile-time-contracts.md) | where 節によるコンパイル時事前条件と型不変条件 |
| [`try:` スニペットを Rust リテラルから外部化する](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/diagnostic-snippet-externalization.md) | 診断の修復 snippet を Rust literal から検査済み source template へ移す残作業 |
| [Flow[T] — 遅延ストリーミング列](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/flow-design.md) | Flow[T] の遅延ストリーミング列。flow.* 名前空間の動詞を list.* と揃える。 |
| [Flow[T] — 利用者向け仕様（草案）](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/flow-spec-draft.md) | Flow[T] の利用者向け仕様案 — Phase 1 後に docs/specs/flow.md へ昇格予定 |
| [GPU 計算 — Matrix 型とコンパイラ駆動の GPU 実行](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/gpu-compute.md) | コンパイラ駆動の CPU/GPU 実行を持つ Matrix primitive 型 |
| [IR 最適化 Tier 2](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/ir-optimization-tier2.md) | 複数ターゲット IR 最適化向け CSE と inline pass |
| [law ブロック実験 — C-ASSERTED を測ってみた記録 (2026-09-27)](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/law-blocks-experiment.md) | C-ASSERTED の最初の実地測定。law ブロック(全入力で成り立つ性質)を LLM に読ませた MSR 実験、同じ law を Lean の自動タクティクだけで何本閉じられるかの実験、関数ごとに law を書けば数学の作法なしで証明が組み上がるかの実験。数字・発見・Lean 相当に足りないもの・次の一手。 |
| [law 証明層 — 設計 (draft, 2026-09-28)](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/law-proofs-design.md) | 関数ごとの law を Lean をカーネルにして証明する層の設計。純粋な断片の自動翻訳、proved / tested / refuted の算出、Int の折り返しと剰余の意味、停止性、翻訳の信頼の鎖、Lean の利用条件(ライセンス・商標)、段階計画。 |
| [LLM 統合](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/llm-integration.md) | ライブラリ生成、自動修復、コード説明の組込み LLM command |
| [LSP コードアクション — 残るリファクタリング](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/lsp-code-actions.md) | 出荷済み診断 quickfix を超える生成 refactoring と import 管理の残被覆 |
| [小型 ML 推論ランタイム](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/ml-inference.md) | コンパイル時のモデル特殊化を使う小型 ML 推論ランタイム。 |
| [MLIR バックエンドと Egg 書き換えエンジン](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/mlir-backend-adoption.md) | Stage 2 の段階的 lowering：dialect walker が全227 spec test に成功 |
| [中央パッケージレジストリ](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/package-registry.md) | 既存 git ベース package system を超える、中央 package 公開、所有、来歴 |
| [依存関係のバージョン制約](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/package-version-resolution.md) | 出荷済み MVS resolver を超える依存範囲の意味論と利用可能 version 選択の残件 |
| [性能研究：世界 #1 への道](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/performance-research.md) | 意味情報を利用した最適化で手書き Rust を超える研究計画 |
| [Porta Embedded — WASI host 上の 10KB 未満の Almide IoT agent](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/porta-embedded.md) | 組込み WASI agent runtime の提案。サイズと cross-host 目標には hardware 上の証拠が必要 |
| [プロセス合成：パイプラインは値、実行は別の手順](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/process-composition.md) | コマンドパイプラインを値として構築し、実行を一つの明示的な手順にする。実際の需要が出るまで保留 |
| [Rainbow Bridge — 外部コードを Almide package として包む](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/rainbow-bridge.md) | @extern を介し外部 Rust/TS/Python コードを native Almide package として包む |
| [リージョンに基づくメモリ管理](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/region-inference.md) | v0.23.4 の歴史的 region 作業と、未実装の全面 region 推論提案 |
| [研究：変更生存率の論文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/research-modification-survival-rate-paper.md) | 言語間の LLM コード改変生存率を測る学術論文 |
| [The Rumbling — Almide OSS 書き直しキャンペーン](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/rumbling.md) | WASM サイズと LLM 精度を実証するため、OSS tool を Almide で書き直す取り組み |
| [設計による安全性](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/secure-by-design.md) | 層別 security model の提案、実装済み基盤、残る信頼境界 |
| [シェル補完](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/shell-completions.md) | bash/zsh/fish の自動補完用 almide completions subcommand |
| [ファイルベースのスナップショットテスト — 保留](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/snapshot-testing.md) | 出荷済み source-literal snapshot assertion を超える任意の file-backed snapshot |
| [監督とアクター](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/supervision-and-actors.md) | Erlang 風 actor、supervisor、型付き channel を stdlib module にする |
| [WASM の例外処理](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/wasm-exception-handling.md) | Wasm 例外 lowering 提案。upstream default は version と engine に依存 |
| [WASM HTTP クライアント](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/wasm-http-client.md) | 出荷済み wasm HTTP client 経路と、default component 移行の残件 |
| [Web フレームワーク](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/web-framework.md) | stdlib http 上の外部 package として、template と Codec を統合する Hono 風 web framework |

<a id="done"></a>

## 完了

15 件

<details>
<summary>保管済み全15項目を表示</summary>

| 完了 | 項目 | 説明 |
|------|------|-------------|
| 2026-07-23 | [コード健全性：Codopsy 駆動のファイル分割と関数分解](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/done/code-health-codopsy.md) | Codopsy 駆動のコード健全化：1000行超 file を分割し、cog>100 関数を分解 |
| 2026-07-15 | [主張の表現：Perceus の言い回しとバイト一致保証の範囲](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/done/claim-wording-perceus-byte-identity.md) | 公開説明を100%測定で裏付けるための、主張表現2箇所の修正 |
| 2026-06-02 | [クロージャー codegen のクロスターゲット差異](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/done/closure-codegen-cross-target-gaps.md) | 敵対的差分走査で見つかった native/wasm の closure-codegen 乖離。全8件修正 |
| 2026-06-02 | [クロージャアーキテクチャ v2](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/done/closure-architecture-v2.md) | Closure Architecture v2 — 1 つの識別子、1 つのキャプチャ集合、リフティングは低水準化。クロージャの表現とインライン化の最適化を分離する |
| 2026-04-20 | [Variant 網羅性の改善](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/done/variant-exhaustiveness-refinement.md) | 非網羅 match に欠けた arm コードを提示し、到達不能 arm は hard error にする |
| 2026-04-20 | [再実装 lint：シグネチャ一致による stdlib 再実装の検出](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/done/reimpl-lint.md) | stdlib 関数と signature が一致する user 関数を検出し、委譲を勧める |
| 2026-04-19 | [Stdlib の宣言的統一 — 単一の正本へ](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/done/stdlib-declarative-unification.md) | stdlib を単一の正本へ近づける：`.almd` + 複数 target ABI attribute |
| 2026-04-19 | [Codegen の理想形](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/done/codegen-ideal-form.md) | 宣言的 dispatch と明示的 symbol 解決へ向けた WASM codegen 再設計 |
| 2026-04-17 | [同梱 Almide Stdlib — 理想形](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/done/bundled-almide-ideal-form.md) | 同梱 Almide stdlib の理想形：dispatch 経路1つ、patch-layer 特例なし |
| 2026-04-16 | [標準ライブラリ module の同梱 Almide dispatch](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/done/bundled-almide-dispatch.md) | stdlib/&lt;module&gt;.almd による TOML module の拡張を可能にする（codegen dispatch 修正） |
| 2026-03-19 | [HKT 基盤 — フェーズ 1–4 + ストリーム融合（全 6 法則）](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/done/hkt-foundation-phase1.md) | 型構築子と代数法則を持つ HKT 基盤 Phase 1–4 |
| 2026-03-19 | [エフェクトシステム — フェーズ1〜2](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/done/effect-system-phase1-2.md) | 7分類の effect 推論 engine と checker 統合 |
| 2026-03-11 | [Playground の修復ターン](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/done/playground-repair.md) | Playground の AI エラー修復と型 checker 統合 |
| 記録なし | [WASM最適化ロードマップ](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/done/wasm-optimization-roadmap.md) |  |
| 記録なし | [Almide WASM Engine — 全面再設計](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/done/wasm-engine-redesign.md) | LayoutRegistry、WasmIR、WasmBuilder を中心とする WASM engine の全面再設計 |

</details>

