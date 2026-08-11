# Road to 1.0 — 0.41 → 0.99 バージョンラダー

> 現在 0.40.2。ここから 1.0 までの全 minor を decade（0.4x / 0.5x / …）単位のアークに割り、
> 各 minor に「顔となる大機能」を 1 つずつ載せる台帳。open issue 40 件は全てこの台帳のどこかに割付済み。

## 読み方と運用ルール

- **decade = アーク**。テーマと出口ゲートを持つ。decade 境界（0.50, 0.60, …）はゲートリリース — その decade の出口監査を固定する 1 リリース。
- **各 minor = 行 1 つ**。0.41–0.99 の 59 バージョン全てに行がある（大機能 54 + ゲートリリース 5）。パッチ（0.41.x）は自由。decade 内で番号が前後にずれるのは構わない — 不変条件は decade ゲートであってバージョン番号ではない。fuzz / hole-hunt / dogfood の findings が割り込むときは既存行を decade 内で後ろへスライドし、ゲートリリースで帳尻を監査する。
- **順序の根拠**は依存関係:
  1. **計測器が手術より先** — fuzz（0.41–0.42）が死んだまま codegen を触らない。hole-hunt レンズ（0.52）は wasm optimizer 接続より先、cranelift 差分ゲート（0.62）は heap/RC 実装より先
  2. **決定文書は最上流** — concurrency の立場（0.44）は cross-target 契約・cranelift 実行モデル・critical profile・仕様凍結すべての上流なので 0.4x で決める
  3. **基盤 → 永続層** — 単一ドライバ（0.43）→ クエリ基盤（0.47–0.48）→ その fingerprint を鍵にしたキャッシュ（0.49）。逆順は作り直しになる
  4. **表現変更はバックエンド追加より先** — RcCow（0.55–0.56）は cranelift がその表現を対象コードにする前に終える
  5. **デバッグ可能性はデフォルト切替より先** — DWARF（0.65）→ cranelift デフォルト化（0.66）
  6. **証明と Critical プロファイルは codegen 安定後**（0.7x）、**資格化キットは証明の上**（0.8x）
  7. **仕様凍結は最後** — 全部が動いてから規範化する（0.9x）
- 新 issue を立てたら、この台帳のどこかの decade に割付ける（同 PR で）。載らない issue を作らない。
- **実行は AI-DLC で回す** — Intent=decade / Unit=行 / Bolt=作業サイクル、人間は Mob ポイントのみ。運用モデルとループ手順の文書（`docs/AI_DLC*.md`、`docs/ai-dlc/units/`）はツリーから外した（git 履歴に残る）。

---

## 0.4x — 計測器と編集ループ

silent-wrong-code の計測器（fuzz nightly）を復活させ、編集ループをプロジェクト規模に依存しない形にする。

| Version | 大機能 | Issue |
|---|---|---|
| 0.41 | fuzz-nightly を毎夜使える計測器に戻す（20 夜中 1 夜の根治）。perf scoreboard/ratchet（#917）の close-out | [#924](https://github.com/almide/almide/issues/924), [#917](https://github.com/almide/almide/issues/917) |
| 0.42 | 単一ドライバ — フロントエンド 1 回実行、手書き段順序 9 → 1。**出荷済** | [#925](https://github.com/almide/almide/issues/925) |
| 0.43 | **v0.42.0 で出荷済**（0.42 と同一タグ — 下の注記参照）。concurrency モデルの立場決定と、それに基づく fan 族の根治。立場は**決定的データ並列**に確定（[concurrency-stance.md](./active/concurrency-stance.md)）— `fan` はスケジューリングの構文であって意味論の構文ではない。実装: E008 の引数拡張(#1025) → `fan.race` トンボストーン(#1024) → キャンセル記述の削除(#1023) → arm 出力のリスト順フラッシュと trap 契約(#1026、C-004 の EXCEPTION 退役) | [#1000](https://github.com/almide/almide/issues/1000), [#1025](https://github.com/almide/almide/issues/1025), [#1024](https://github.com/almide/almide/issues/1024), [#1023](https://github.com/almide/almide/issues/1023), [#1026](https://github.com/almide/almide/issues/1026) |
| 0.44 | fuzz true green — 残 findings 0 + 連続緑 2 夜 | [#796](https://github.com/almide/almide/issues/796) |
| 0.45 | **測定して着手しないと決定（2026-07-31）**。feature-gated runtime（http/zlib）の rtlib 化 — 8.4s は再現したが（全 4 キャッシュ層クリアで 9s）、rlib cache が編集ループでは吸収し、CI では container あたり 1 回 ~9s＝ジョブの約 2%。issue 自身の「それ以前ではない」が正しい。再武装条件を鋭利化して #1002 に記録 | [#1002](https://github.com/almide/almide/issues/1002) |
| 0.46 | **進行中（v0.43.0 で途中成果を出荷）**。10k 行 dogfood プロジェクト着工 — スケール主張を実測に変える。`tools/almide-gates` がサブコマンド 3/6 でバイト一致、~490 行で欠陥 6 件を検出 | [#1001](https://github.com/almide/almide/issues/1001) |
| 0.47 | クエリ/インクリメンタル基盤 phase 1 — LSP を per-keystroke 全再解析から解放 | [#928](https://github.com/almide/almide/issues/928) |
| 0.48 | クエリ基盤 phase 2 — ビルドパイプライン本体をクエリ上に | [#928](https://github.com/almide/almide/issues/928) |
| 0.49 | モジュール単位コンパイルキャッシュ — クエリ fingerprint を鍵にした永続層（module rlib + typed-IR）。dogfood のフルビルド 2-3s 超がトリガー | [#1003](https://github.com/almide/almide/issues/1003) |
| 0.50 | ゲートリリース — build-speed / runtime-perf / safety 三点セットの実測数字を README に載せ切り、0.4x 出口監査をラチェットとして発効 | [#999](https://github.com/almide/almide/issues/999) |


> **0.42–0.44 の順序を入れ替えた（2026-07-31）。** 元は 0.42=fuzz true green /
> 0.43=単一ドライバ / 0.44=concurrency。fuzz true green の DoD は「連続緑 2 夜」で、
> これは作業量ではなく**観測期間**に律速される — どれだけ手を動かしても今日は閉じない。
> 一方 単一ドライバと concurrency は完成した。ラダー自身の運用ルール
> 「decade 内で番号が前後にずれるのは構わない — 不変条件は decade ゲートであって
> バージョン番号ではない」に従い、完成した 2 つを先に出荷し、観測待ちの行を 0.44 へ
> スライドした。Gate 0.50 の条件は変わっていない。


> **0.42 と 0.43 は同じタグ（v0.42.0）で出荷した。** 両 Unit が develop 上で完成してから
> 最初のリリースを切ったため、1 タグが 2 行分を運んだ。リリースノートは両方を記載している。
> 直後に空の v0.43.0 を切るのは「既に出荷済みの内容を主張するバージョン番号」になるので
> しない。以降の版番号は行番号より 1 つ後ろにずれる — ラダーの不変条件は decade ゲートで
> あってバージョン番号ではない、というルールの範囲内。**教訓は「Unit が閉じた時点で出荷
> する」**こと。まとめて出すこと自体は誤りではないが、行↔版の対応が静かに崩れる。


### 0.4x 出荷状況（2026-08-01 時点）

行番号と版番号は 1:1 ではない。0.42–0.44 は観測律速の行を後ろへスライドさせ、完成した 2 行を
1 タグでまとめて出したため。**どの行がどの版で出たかはこの表が正**：

| 行 | 内容 | 出荷 |
|---|---|---|
| 0.42 | 単一ドライバ（#925） | **v0.42.0** |
| 0.43 | concurrency 立場決定 + fan 族 4 件（#1000 #1023 #1024 #1025 #1026、全てクローズ済） | **v0.42.0**（0.42 と同一タグ） |
| 0.44 | fuzz true green（#796） | 未出荷 — nightly 連続緑 2 夜が条件、観測律速 |
| 0.45 | rtlib 化（#1002） | 出荷なし — 測定して発動条件を満たさないと決定。コードを書かないことが正解 |
| 0.46 | 10k 行 dogfood（#1001） | 途中成果を **v0.43.0** で出荷（サブコマンド 3/6 バイト一致） |
| 0.47 | クエリ基盤 phase 1（#928） | 未着手 — 指標が現存規模では測定不能、0.46 待ち |
| 0.48 | クエリ基盤 phase 2（#928） | 未着手 — 0.47 の答え待ち |
| 0.49 | モジュールキャッシュ（#1003） | 未着手 — issue が「実プロジェクトのフルビルド 2–3s 超まで着手するな」と明記、0.46 待ち |
| 0.50 | ゲートリリース（#999） | B1 のみ完了（ビルド速度を README に公開） |

**0.45 / 0.47 / 0.49 の 3 行は、issue 自身が発動条件を書いており、いずれも 0.46 に依存する。**
0.4x 後半は独立した 4 行ではなく、0.46 を根とする 1 本の依存鎖である。

**Gate 0.50**: fuzz 連続緑が常態 / dogfood フルビルドがキャッシュ効きで 2-3s 未満 / 三点の数字が public かつラチェット管理。

## 0.5x — クロスターゲット対等性

wasm leg を native と同格に。最適化品質の乖離（#929）は v0 退役時に構造的に生まれた負債であり、ここで返す。

| Version | 大機能 | Issue |
|---|---|---|
| 0.51 | effect 表面規則の統一と診断修復（OTel dogfooding 発の #1049–#1054 一括）— `!` は effect call 上で常に可（never-err は無警告 no-op）、二項演算子オペランドの implicit unwrap（lowering の A-正規化込み）、unresolved import の wall taxonomy 修復、runtime-backed 型のユーザー注釈（HttpRequest/HttpResponse/JsonPath + 完備性 matrix gate）、E025 shape 導出。**出荷済 v0.51.0**。派生: [#1055](https://github.com/almide/almide/issues/1055)（effect-typed fn params）, [#1056](https://github.com/almide/almide/issues/1056) | [#1049](https://github.com/almide/almide/issues/1049)–[#1054](https://github.com/almide/almide/issues/1054) |
| 0.52 | JSON interop の設計決定（#1062）— 追加機構ゼロで決着: wire 型は wire の名前をそのまま鏡映、`as "wire"` は識別子になれないキー限定、外来 tag 形は手書き codec。C-209 = encode は none 省略 / decode は missing・null を none に畳む / `Option[Value]` が 3 状態の脱出口 / underivable shape は宣言時 E023。#1061 の 13 セル全消化 + コンテナ任意ネスト（#1065）。同梱: temp-dir 表面の TMPDIR 規約（C-189 改訂）、wasmtime 47。**出荷済 v0.52.0** | [#1062](https://github.com/almide/almide/issues/1062), [#1064](https://github.com/almide/almide/issues/1064), [#1065](https://github.com/almide/almide/issues/1065) |
| 0.53 | プラットフォーム conformance（Wasm 3.0 / WASI 監査残件の全消化）— NaN 観測の canonical 化 = deterministic profile 準拠（C-210 + relaxed SIMD/atomics/shared 不使用の命令 gate、native 側含め全ホストアーキで成立）。self-host リンク同名異署名衝突 = invalid-wasm 脱出の wall 化（#1068）。return_call_indirect（C-178 の indirect twin、深度主張の正直化込み）。**Windows ホストの wasm レッグ開通 + push ごとの常設 CI gate**（#1066、fs パス契約 = Go 互換を Go/Rust/wasi-libc 比較つきで明文化）。**出荷済 v0.53.0** | [#1066](https://github.com/almide/almide/issues/1066), [#1068](https://github.com/almide/almide/issues/1068) |
| 0.54 | pre-optimizer 安全化 → wasm optimizer 接続 + SIMD — QualifiedRef newtype（v1 MIR 上で bare type identity を表現不能に、#433 クラスの型による根絶）と hole-hunt レンズ（実例棚 [#1018](https://github.com/almide/almide/issues/1018)）を前段に据え、その上で nanopass optimizer 群と v128 SIMD を wasm leg へ接続する（optimizer と SIMD はどちらも #929 の同一アーク）。「optimizer より前」という順序制約は Unit 内順序として保存 | [#908](https://github.com/almide/almide/issues/908), [#912](https://github.com/almide/almide/issues/912), [#929](https://github.com/almide/almide/issues/929) |
| 0.55 | RcCow 表現コスト phase 1 — allocation-heavy 文字列ワークロードの対 Rust ~1.7x を解剖・縮小 | [#1004](https://github.com/almide/almide/issues/1004) |
| 0.56 | RcCow phase 2 — 対 Rust ギャップをラチェット下に。表現変更は cranelift（0.6x）が対象コードを生成し始める前に完了 | [#1004](https://github.com/almide/almide/issues/1004) |
| 0.57 | 10k 行 dogfood プロジェクト完成・公開 — 0.46 着工分の完了、スケール数字（LOC・モジュール数・ビルド時間）を README に | [#1001](https://github.com/almide/almide/issues/1001) |
| 0.58 | hole-hunt findings 焼却完了 — 0.52 のレンズ群が出した findings を 0 に | [#912](https://github.com/almide/almide/issues/912) |
| 0.59 | MSR の第三者再現性 — dojo ブリッジ CI（タスクサブセットを本 repo の PR ゲートで実行）+ 他言語でも同条件で走らせられる公開ハーネス。指標が土俵になる条件 | — |
| 0.60 | ゲートリリース — クロスターゲット対等性監査を固定 | — |

**Gate 0.60**: 両ターゲットの最適化品質が同格 / hole-hunt findings 0 / 対 Rust perf ギャップが計測・ラチェット管理下。

> **0.51 の差し替え（2026-08-03）**: 計画上の 0.51（QualifiedRef #908）は、OTel dogfooding が出した
> #1049–#1054（effect 表面規則の統一）に席を譲った。表面規則の穴は書き手が今日踏むもので、
> optimizer 前提の内部安全化より先に返すべき負債だからである。QualifiedRef は消えたのではなく
> 0.52 に統合され、「0.53 の optimizer 接続より前」という順序制約ごと保存されている。
> 静かな番号の付け替えではなく、この注記が記録である。
>
> **0.52 / 0.53 の差し替え（2026-08-03、連続 2 件）**: 0.52 の計画枠（QualifiedRef + hole-hunt）は
> #1062 の JSON interop 設計決定（並行レーンで同日出荷）に、0.53 の計画枠（optimizer 接続）は
> Wasm 3.0 / WASI 監査残件の全消化（プラットフォーム conformance）に、それぞれ席を譲った。
> QualifiedRef + hole-hunt は 0.54 の**前段**に統合 — 0.54 の本体（optimizer 接続 + SIMD、
> どちらも #929）はまさにこの 2 つを前提とするので、順序制約は Unit 内順序として保存される。
> 3 連続の差し替えが示す実態 — 「計画 Unit より、dogfooding と監査が吐く負債の方が先に
> 返済期日を迎える」— もここに記録しておく。

## 0.6x — rustc からの独立（debug ビルド）

cranelift direct native emit のエンドゲーム（#1005）。0.4x で復活させた fuzz oracle と、新設する cranelift-vs-rustc 差分ゲートが安全網。

| Version | 大機能 | Issue |
|---|---|---|
| 0.61 | cranelift spike — scalar core の MIR → CLIF | [#1005](https://github.com/almide/almide/issues/1005) |
| 0.62 | 差分ゲート — cranelift leg vs rustc leg の挙動 oracle を scalar サブセットの時点で CI 常設し、以後のカバレッジ拡大と共に育てる | [#1005](https://github.com/almide/almide/issues/1005) |
| 0.63 | heap / RC 演算 + closure | [#1005](https://github.com/almide/almide/issues/1005) |
| 0.64 | rtlib リンクと stdlib 全面カバー | [#1005](https://github.com/almide/almide/issues/1005) |
| 0.65 | cranelift debug info — DWARF 行情報と backtrace。デフォルト切替の前にデバッグ可能性を確保する | [#1005](https://github.com/almide/almide/issues/1005) |
| 0.66 | debug ビルドのデフォルトを cranelift に切替（release は rustc 継続） | [#1005](https://github.com/almide/almide/issues/1005) |
| 0.67 | in-process JIT 実行 — `almide run` / `almide test` の debug パスからリンカも消す | [#1005](https://github.com/almide/almide/issues/1005) |
| 0.68 | 関数単位インクリメンタル再コンパイル — cranelift をクエリ基盤（0.47–0.48）に接続 | [#928](https://github.com/almide/almide/issues/928), [#1005](https://github.com/almide/almide/issues/1005) |
| 0.69 | 編集ループ総仕上げ — check → run の p50/p95 を計測して README 数字に追加 | [#999](https://github.com/almide/almide/issues/999) |
| 0.70 | ゲートリリース — rustc-free debug 監査を固定 | — |

**Gate 0.70**: `almide run` の debug パスから rustc とリンカが消滅、フル oracle 緑のまま。

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
| 0.80 | ゲートリリース — Critical プロファイル監査を固定 | — |

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
| 0.87 | dossier 外部レビュー round 1 — reference app の dossier を DER/TÜV 文脈の外部レビューに渡し、所見を反映 | [#571](https://github.com/almide/almide/issues/571), [#579](https://github.com/almide/almide/issues/579) |
| 0.88 | qualification kit の顧客統合ストーリー — kit が提供する範囲と顧客側ドメインプロセスの境界を確立（G-F6） | [#574](https://github.com/almide/almide/issues/574) |
| 0.89 | qualification kit v1 — 外部所見反映後の署名付き完成版 | [#571](https://github.com/almide/almide/issues/571), [#574](https://github.com/almide/almide/issues/574) |
| 0.90 | ゲートリリース — 引き渡し可能性監査を固定 | — |

**Gate 0.90**: reference app + dossier + kit のセットを第三者にそのまま渡せる。

## 0.9x — 規範仕様と 1.0 エンドゲーム

全部が動いてから規範化する。仕様凍結が最後なのは意図的 — 凍結は完成の宣言であって願望ではない。

| Version | 大機能 | Issue |
|---|---|---|
| 0.91 | almide-interp を第三審から規範意味論（normative semantics）へ昇格。前提は abstain 台帳を 0 に寄せること — 残る in-place 系の穴が [#1021](https://github.com/almide/almide/issues/1021)（bytes バイト単位ライタ）と [#1022](https://github.com/almide/almide/issues/1022)（mut パラメータの copy-out） | [#564](https://github.com/almide/almide/issues/564), [#1021](https://github.com/almide/almide/issues/1021), [#1022](https://github.com/almide/almide/issues/1022) |
| 0.92 | ALS 文法・構文章 — grammar の規範化 | [#530](https://github.com/almide/almide/issues/530) |
| 0.93 | ALS 型システム章 — 推論・単一化・protocol 制約の規範化 | [#530](https://github.com/almide/almide/issues/530) |
| 0.94 | ALS 動的意味論章 — 0.91 で昇格した interp 準拠で記述 | [#530](https://github.com/almide/almide/issues/530) |
| 0.95 | ALS stdlib・cross-target 契約章 — contract ledger を仕様へ昇格 | [#530](https://github.com/almide/almide/issues/530) |
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
- **指標**: MSR が第三者再現可能 — dojo ハーネスが公開され、他言語でも同条件で測れる（0.59）

## 台帳の完全性

- **バージョン行 59 / 59** — 0.41–0.99 の全 minor に行がある（大機能 54 + ゲートリリース 5）
- **open issue 40 / 40 割付済み** — バージョン行に 32、プログラムトラックに 8
- Issue 欄が「—」の行（0.59 / ゲートリリース 0.60–0.90 / 0.96 / 0.98 / 0.99）は台帳新設の作業。着工時に issue を立てて同 PR でリンクを埋める。0.50 のゲートリリースは #999 を成果物として持つ
- この台帳と issue リストの乖離は負債 — 新 issue は同 PR でここに割付け、クローズしたら issue リンクが closed になることで進捗が見える

## Release-order deviation, 2026-08-01 — recorded so the ladder can be read honestly

**The ladder's rule is one Unit, one release, in order.** It was broken during the 0.4x decade
and this note is the correction rather than a quiet renumber.

What happened: v0.45.0 shipped, `Cargo.toml` was bumped to 0.46.0, and then the work and
records for Units **0.47, 0.48, 0.49 and 0.50 all landed on `develop` before v0.46.0 was
tagged**. The tree therefore carried five Units' worth of change under one unreleased version
number.

Why it happened, plainly: Units 0.45, 0.47 and 0.48 resolved by *measurement* rather than by
implementation — each concluded "the trigger does not fire, here are the numbers" — so they
produced documents rather than artifacts, and a document feels like it does not need a release.
It does. A release is what makes a row's conclusion citable and dated, and a measurement whose
conclusion is "we are not building this" is exactly the kind of decision that needs a fixed
point someone can point at later.

**The correction**: releases resume in order from **v0.46.0**, one per row, and no row is
described as shipped before its tag exists. Where a release carries records that landed early,
its notes say so instead of pretending the ordering held.

**The rule, sharpened for next time**: a Unit is not done when its `construction.md` is
written. It is done when the tag exists. Starting the next Unit before that is what produced
this, and the ladder is only auditable if the two stay coupled.

## Merged past unverified CI, 2026-08-01 — the mechanism that should have stopped it

**v0.47.0, v0.48.0 and v0.49.0 were tagged on commits whose CI never completed.** Not failed —
**CANCELLED**, each superseded by the next merge while still in flight. Recorded here because
the cause is a missing mechanism, not a missing intention.

**What happened.** The first two release PRs were merged after explicitly polling their checks
to green. Polling took ~50 minutes per release, so the remaining three switched to
`gh pr merge --merge --auto`, expecting auto-merge to hold until checks passed. `main` requires
a pull request but has **no required status checks**, so `--merge` executed immediately and
`--auto` was a no-op. The command returned `MERGED` and the release proceeded.

**A flag was substituted for a verification.** That is the whole failure. The intention was
identical in all five releases; only the enforcement differed.

**The damage, measured rather than assumed**: `git diff v0.49.0 v0.50.0` restricted to
`crates/ src/ stdlib/ runtime/ spec/ tests/ tools/ .github/` is **empty** — every difference is
documentation — and v0.50.0 (`c75f2ee8`) is green on `main`. So the three tags are verified
transitively and nothing shipped is unverified in substance. Each release note now says so
rather than leaving the gap to be discovered.

**Not retracted, and the reason matters.** The release-deletion procedure in CLAUDE.md is for a
BROKEN release. These are not broken; they lack a completed run on their own commit, which is a
process defect. Deleting them would break anyone who pinned a tag, and removing 0.49 alone
would restore the 0.48 → 0.50 gap that this ladder exists to prevent.

### The fix is a mechanism, not a resolution

**Required status checks on `main` are NOT configured and should be.** With them, `--merge`
would have been refused by GitHub regardless of what the operator intended:

```bash
gh api -X PUT repos/almide/almide/branches/main/protection --input - <<'JSON'
{
  "required_status_checks": {
    "strict": true,
    "contexts": [
      "Test Rust", "Test WASM", "Emit & Format",
      "Cross-Target (Rust vs WASM)",
      "Coq proofs + axiom audit + PCC gate",
      "WASM host-arch determinism"
    ]
  },
  "enforce_admins": false,
  "required_pull_request_reviews": null,
  "restrictions": null,
  "required_linear_history": false,
  "allow_force_pushes": false,
  "allow_deletions": false
}
JSON
```

`"strict": true` also forces the branch to be up to date before merging, which would have
caught the second half of this: `develop` moved under two of these PRs while they were open, so
the checks that were cancelled were cancelled for a reason worth surfacing.

This is the same discipline the repository already applies to everything else — the contract
ledger, the ratchets, the down-only counts. **A rule that depends on the operator remembering
is not a rule.** Until it is configured, the release procedure below is the fallback, and it is
strictly weaker.

### Until then: the release procedure has one added step

Between "merge" and "tag": **confirm every check on the PR reached `SUCCESS`**, by reading the
conclusions, not by trusting a merge flag.

```bash
gh pr view <N> --json statusCheckRollup \
  --jq '[.statusCheckRollup[]|select(.conclusion!="SUCCESS" and .conclusion!="SKIPPED")|{name,conclusion,status}]'
```

Empty output, and only empty output, is permission to tag. `CANCELLED` counts as not-verified.
