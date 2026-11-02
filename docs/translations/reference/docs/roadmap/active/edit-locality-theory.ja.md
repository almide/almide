<a id="edit-locality-theory--the-theorem-almide-exists-to-prove"></a>

# edit-locality 理論 — Almide が証明するために存在する定理

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](edit-locality-theory.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](edit-locality-theory.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](edit-locality-theory.zh-CN.md)

> 非規範の原文固定読者用スナップショット。変更しない原文を読取り側にとっての正本とする。自動同期や独立実行はしない。歴史的主張・採用案・実装状態・測定証拠の元の範囲を保持し、本翻訳が独立検証したとはしない。インラインコードは元言語の文字列も含め原文どおり保持する。 段階完了は明示的に断片範囲・8月15日時点に限定する。型なしframe、型付きmodularity、評価器健全性、未証明の完全性、信頼するrender接続点を区別する。初期5予測は歴史的本文で待機中のままで、新たに確認したMSR改善ではない。Koka・調査コンパイラ・唯一性の主張は当時の原文調査に帰属し、ここで独立再確認しない。メモリ・時間・コンパイラ全体の証明を主張しない。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/edit-locality-theory.md)

> 元metadataの非活性注記：限定された編集影響範囲を設計直感から証明済み定理へ変える。

**状態**: active（基本文書を 2026-08-15 に記述。Stage 1 は `docs/specs/edit-locality.md` として着地）。
**使命との関係**: MSR（modification survival rate）は、ある意味論的性質の経験的な影である。それは、ソース編集の観測可能な影響範囲が、編集されたコードを通る実行に限定されるという性質。本書は、それを設計上の直感から、動く実験ループを備えた証明済み定理へ変える計画である。

<a id="the-claim"></a>

## 主張

> **Edit Frame（L1）。** 定義のシグネチャを保持する編集は、編集された定義を通らない実行の観測（stdout、stderr、exit code — `docs/contracts/contracts.toml` が固定する集合）を変えられない。

これを満たす実装済み言語はほとんどない。オーバーロード解決、暗黙 instance、マクロ、プログラム全体の推論、動的スコープの effect handler は、それぞれこれを破る。離れた場所の編集によって、未編集コードの意味を変えられるためである。Almide の既存規律 — シグネチャ中の `effect`、明示的伝播のみ（ADR-0008、E041/E042）、修飾付きのモジュール横断アクセス — は、まさに L1 が必要とする前提集合である。各規則は MSR のため一つずつ採用された。L1 は、そのすべてが支えていた単一の文である。

<a id="why-this-is-the-anti-koka-position-not-the-diet-koka-position"></a>

## なぜこれは簡略版 Koka ではなく、Koka と対極の立場なのか

Koka の至宝である動的スコープの代数的 effect handler は、構成上 frame 違反を作る機構である。`perform op` の意味は、呼び出し文脈のどこか、場合によっては別ファイルに配置された handler に依存する。handler の追加や並べ替えは、未編集コードの動作を変える遠隔編集である。**Koka は handler を切り離さずに L1 を証明できない。** したがって両言語は一つの軸の対極にある。Koka は effect の表現力を最大化し、Almide は編集局所性を最大化する。将来の機能提案への gate の問いは一つ、*L1 を保持するか？* である。（handler、暗黙 instance、glob 再 export はすべて gate に失敗する。それが gate の仕事である。）

<a id="stages"></a>

## 段階

1. **定式化と探索**（DONE 2026-08-15）: `docs/specs/edit-locality.md` は現言語に対して L1–L3 を述べ、各既存規則が果たす局所性の役割を対応付け、実装済みコンパイラに対する反例探索を記録する。見つかった違反はすべて言語修正か明示的な付帯条件にし、未記述の例外には決してしない。初日の成果: almide 0.57.0 で**現存するターゲット間不一致**を再現した（V1 — LICM が、本体推論で「pure」とされた部分演算を実行回数ゼロのループから投機的に移動し、native は trap、wasm は出力する）。421 個の `wasm_cross` fixture と差分 fuzz はこれを検出していなかった。証明を 1 行も書く前に、定理が実際の契約違反を発見した。
2. **機械化した核計算体系**（DONE 2026-08-15）: λ_almd は `crates/almide-edit-belt` にある。3 つ目の 0-sorry Lean belt であり、perceus・race とともに CI `lean-proofs` job で kernel-check する。計算体系: 宣言されたシグネチャ（`effect` flag を含む）を持つトップレベル定義、`let` / 呼び出し / `Result` 構築と `match` / 明示的 `!`（ADR-0008、`norm`/`abrupt` 結果の分離として）/ `??` / `print`（観測）、trace と入った全定義の台帳を備えた big-step 意味論。すべて sorry-free の定理:
   - `ev_agree` — 評価は実際に入った定義だけに依存する。一つの補題で L1 全体を述べる。call 規則が呼び出した名前の位置だけで表を読み、それ以外を読まないため、1 ページで証明できる。
   - `edit_frame` / `edit_frame_observables` — L1 の移送、および（決定性 `ev_det` と組み合わせた）観測同一の形式。記録すべき発見: 型なしの frame はシグネチャ保持を全く必要としない。`f` を何に置き換えても、`f` を避ける実行は保たれる。シグネチャ保持は型付き部分だけの仮定である。
   - `typing_modular` — 同じシグネチャで本体を入れ替えてもプログラムは well-typed のままで、他の各定義の導出は文字どおり同じ proof object になる（型付けはシグネチャだけを見る）。
   - `pure_silent` — `eff = false` と型付けされたコードは空 trace で評価される。「pure `fn` は観測を生成できない」の意味論的内容（E006 の境界、機械検査済み）。
   正直な範囲: 単項呼び出し、`Result` のみのデータ、1 effect。must-use（E041/E042）は実装レベルの診断のままで、核は ADR-0008 の実行時意味をモデル化し、lint 表面はモデル化しない。
3. **バックエンドを refinement として扱う**（断片範囲で DONE、2026-08-15）: 核は実行可能な仕様である。`Evaluator.lean` は fuel を添字とする `evalE` と `eval_sound` を追加する（返すものはすべて関係で導出でき、`ev_det` と合わせて一意。完全性は意図的に未証明で、`some` 出力のみ利用する）。強制の対象は手選びではなく生成器全域。`Corpus.lean` が λ_almd で表現できる断片上の型付き 48 プログラムのコーパスを決定的に生成し、全プログラムが観測に評価されることを kernel-check する（定理 `corpus_total`、`:= by rfl`）。`conformancegen` がコミット済みコーパスを出力・固定する（`proofs/kernel-conformance/`、CI の drift gate で管理。`.expected` trace 自体も評価器で固定され、`trustCompiler` クラスの接続点となる）。`tests/kernel_conformance_test.rs` はその全体を native と wasm の両方で期待 trace に照合する。着地時は両ターゲット 48/48、加えて契約 C-280 下の手書き 7 プログラム族がある。信頼の接続点は一度レビューした約 80 行の `erase`/`render` 対であり、3 層の位置づけは `docs/contracts/proven-vs-trusted.md` にある。初日の成果: 初回実行でコーパスは実際のコンパイラ bug を 2 つ捉えた。almide#1428（checker が裸の `err(..)` match subject を受け入れ、codegen が無効 Rust を出力）と almide#1429（裸のパラメータ末尾で、v1 renderer が effect fn のシグネチャと本体を分離）。残るのは、証明器内に構築したコンパイラなしでは手の届かない研究級作業、すなわち検証済み surface-core→λ_almd 変換と Rust 実装のパスごとの simulation 証明である。それまでは断片 gate が ratchet となる。`docs/specs/edit-locality.md` §3 の R-group 義務は独自台帳に残す。
4. **予測ループ**（機構稼働 2026-08-15、着地後の実行蓄積により閉じる）: `proofs/l1-verdicts.toml` は実験ノートである。意味論に触れるすべての変更は、測定より前に L1 verdict とコミットした MSR 予測（因果機構付きの up/neutral/down）を記録する。`scripts/check-l1-verdicts.sh` が CI で schema を検査する。Dojo 側は `src/l1_loop.almd`（dogfood 規則に従い Almide で記述。自己利用でただちに `regex.captures` の文書と実装のずれ、almide#1432 を発見）。台帳と `runs/*/summary.md` の見出し MSR 数値を結合し、各予測の状態 — confirmed / refuted / inconclusive / awaiting — を `dashboards/l1-loop.md` に報告する。反証された予測も理論に関する所見として台帳に残す。初期登録は LV-001..005（この取り組みの 5 着地、すべて `up` 予測）。すべて着地後の実行待ちであり、最後に採点した実行はこの取り組みより前なので、最初の実データ点は次回 Dojo で到来する。測定を予測する理論は他の言語チームが持たない層である。正確な Koka 比較（Survey 4、2026-08-15 に koka ソースで確認）: 証明は論文上のみで repo 内のどこにも機械化はない。812 行の FIP/FBIP verifier はコンパイルのたびに動くが、Core IR typechecker は三重に無効化されている（既定 OFF、test suite からコメントアウト、ON でも warning のみ）。クロスバックエンド差分 harness は存在するが CI では一度も有効化しない。機械検査した定理を動く測定ループへつなぐ組み合わせは、調査した 9 コンパイラのどれも持たない。

<a id="non-goals"></a>

## 対象外

- Koka と effect 表現力で競うこと。その競争では 14 年の後れがあり、勝とうとすれば L1 を失う。
- メモリや時間について何かを証明すること。観測集合は stdout / stderr / exit code であり、既存契約台帳が固定するものと厳密に同じである。
