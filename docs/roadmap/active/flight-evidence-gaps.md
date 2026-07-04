<!-- description: Hands-on internal audit findings (2026-07-03) — the measured distance between DAL-A philosophy and DAL-A evidence, as 7 findings with corrective work items and acceptance criteria -->
# Flight Evidence Gaps — 実地監査所見台帳

> **Position**: [certification-grade](certification-grade.md) が規格の 5 メカニズム分解と
> ALS 設計、[flight-qualification](flight-qualification.md) が DO-330/333 マッピングを
> 受け持つのに対し、本書は **2026-07-02〜03 の実地作業（compiler/MIR を約 40 コミット改修、
> parity 126→163、silent-miscompile クラス全滅）で作業者として内側から観測した証拠体系の穴**を、
> 監査所見（finding）の形式で固定する。各所見は「このセッションで実際に起きた事実」を
> 証拠として持ち、規格上の意味・是正 work item・機械検証可能な受入基準を付す。
>
> **総評**: 設計保証の哲学（fail-safe 大原則・トレーサビリティ・ラチェット）は DAL-A を
> 向いているが、証拠体系は DAL-D 相当。距離を埋めるのは個別バグ修正ではなく本書の 7 所見。

---

## 所見一覧（深刻度順）

| # | 所見 | 深刻度 | 対応する規格メカニズム |
|---|---|---|---|
| F1 | 仕様が実装（v0）そのもの — oracle 循環 | 最重大 | ①独立規範仕様 |
| F2 | 検証カバレッジの錯覚 — ゲート green ≠ 出力正 | 最重大 | ③検証格付け |
| F3 | 証明と実装の間隙 — lowering は信頼コード | 重大 | ③ DO-333 formal credit |
| F4 | 検証の非決定性 — flaky を「既知」で容認 | 重大 | ③検証手順の頑健性 |
| F5 | 独立性の不在 — 変更者がラチェットを動かす | 重大 | ⑤プロセスと責任 |
| F6 | 構成管理インシデント未解明 — バイナリ置換 | 中 | ⑤構成管理 |
| F7 | 既知の数値非互換 — float.parse 境界丸め | 中 | ①数値精度要求 |

---

## F1 — 仕様が実装（v0）そのもの（oracle 循環）

**証拠（2026-07-03 実例）**: MIR 経路の `json.parse` self-host は
`runtime/rs/src/json.rs` の**条項写経**として実装した（サロゲート合成・寛容な区切り・
エラー文言まで）。`string.trim` の Unicode 化も `char::is_whitespace` の 25 codepoint を
oracle から逆引きした。float.parse のエラー文言（"cannot parse float from empty string"）は
Rust の `ParseFloatError` 表示への追従。**「正しさ」の定義が常に v0 実装**であり、
v0 のバグは仕様バグとして固定される（実際、`list.chunk(-1)` は `chunks(n as usize)` の
usize 再解釈という Rust 実装詳細を「仕様」として写した）。

**規格上の意味**: DO-178C の要求は実装から独立に存在しなければならない。
実装＝仕様の循環では High-Level Requirements への適合検証が定義不能。

**是正**: certification-grade CG-1（ALS 規範化）がそのまま是正措置。本書からの追加要求は:
- ALS の各節は「v0 がこうだから」でなく**規範的判断**として書く。v0 の実装詳細由来の
  挙動（chunk の usize 再解釈、parse_string の寛容さ）は ALS 側で**採否を明示的に裁定**し、
  裁定の記録（rationale）を残す。
- 写経で作った self-host（json_parse / string_trim / float_parse / list_chunk）を
  ALS 完成後に**仕様参照へ張り替える**リストとして本所見に固定する。

**受入基準**: `check-contracts.sh` の三層トレーサビリティ（CG-1 exit criteria）に加え、
上記 4 self-host のヘッダコメントが oracle 実装ファイルでなく ALS 節番号を参照している。

**進捗（2026-07-04）**: `docs/specs/als/text-and-numbers.md` に規範節 ALS-T1〜T5 を制定
（trim の White_Space 集合、float.parse の正確丸め値規範＋エラー文言、json.parse の
RFC 8259 + 裁定、list.chunk の負サイズ裁定 — **v0 実装詳細からの昇格を明記** —、
case mapping の Unicode 規範 + Final_Sigma + ロケール非依存裁定）。写経 self-host
6 ファイルのヘッダを ALS 節参照に張り替え済み（oracle 循環の後半 = CG-1 の
spec-keying / 三層トレーサビリティは未了）。

**進捗2（2026-07-04）**: **spec-keying 機構を実装** — 契約の `spec = "ALS-xx"`
フィールドと、check-contracts.sh の解決検証（存在しない節を参照する契約は gate FAIL、
mutation テスト済み）。初期セット6契約を keyed（C-001/C-002→ALS-T6 終了規約、
C-007→ALS-T7 top-let 評価時機 — 両節を新規制定 —、C-020→T5、C-021→T1、C-024→T2）。
三層 spec↔contract↔fixture の機構は稼働。CG-1 の残り = 全 active 契約への展開
（ALS 節の執筆が律速、文書プロジェクト）。

## F2 — 検証カバレッジの錯覚（ゲート green ≠ 出力正）

**証拠（2026-07-03 実例）**: corpus-wall が "zero silent miscompiles" を掲げて green の裏で、
`match` の linearization（両アーム逐次実行）が **println を両方実行する出力破壊**として
生存していた。ゲートの主張は証明書の健全性（ownership/caps）であって出力の正しさではない。
出力を実際に検証する output-parity の網は spec 573 ファイル中 **163**（wall 75 + v0fail 23 +
skip 297 は未検証）。さらに**コンパイラ自体の構造カバレッジは一切未計測** — MC/DC どころか
statement coverage も無い。「どのコード行が一度もテストで通っていないか」を誰も知らない。

**規格上の意味**: DAL-A は object code レベルの MC/DC。その前提の statement coverage すら
無い状態では、検証活動の完全性を主張する術がない。linearization の生存はまさに
「カバレッジの穴に住むバグ」の実物標本。

**是正 work items**:
1. `cargo llvm-cov`（または grcov）を CI に導入し、`almide test spec/` + corpus-wall +
   output-parity 実行下での **almide-mir / almide-codegen の line coverage を計測・公表**する
   （まず現状値の直視。目標値はその後）。
2. 未カバー領域のうち「出力に影響する分岐」を evidence ladder の欠損として台帳化。
3. output-parity の skip 297 の内訳を分類し（`fn main` 無し等）、**検証不能ではなく
   検証未着手**のものを wall/v0fail から切り出す。
4. ゲートの主張文言を検証内容に一致させる（corpus-wall の「zero silent miscompiles」は
   「zero certificate violations」へ — 主張と証拠の一致は監査の第一歩）。

**受入基準**: CI が coverage 数値を吐き、ダッシュボードに「コンパイラ自体のカバレッジ」行が
存在する。ゲートの出力文言が実際の検証対象と一致している。

**進捗（2026-07-03）**: `proofs/coverage.sh`（手動 llvm-cov パイプライン — cargo-llvm-cov の
オーケストレーションは誤対象を 0.00% と報告したため不採用）で初回計測:
**almide-mir line 65.76% / function 72.20%**（テストスイート + spec/wasm_cross 232 fixture の
render 負荷）。ゲート文言は corpus-wall / receipt / TRUSTED_BASE / classify_corpus の 4 箇所を
「totality + certificate claim, NOT output correctness」に修正済み。同日、この文言修正の
正しさが実地で証明された: `prim.handle(<literal>)` が deferred-Const 0 に落ちる
silent-miscompile（6 例目のクラス）を corpus-wall green の裏で発見・修正した。
deferred-Const フォールバックの退役が次の一手（TRUSTED_BASE の境界図に記録）→ **退役済み**
（strict value mode、2026-07-03）。

**進捗2（2026-07-04）**: ワークロードを拡大した再計測 — almide-mir + almide-codegen の
テストスイート + spec 全実行可能 fixture の v1 render + **v0 生産経路**（`almide test spec/`
のフルパイプライン）。TOTAL line 65.89%。per-file 台帳を `proofs/COVERAGE.md` に固定
（最低は defunc エンジン control_p5 の 43% — 次のテスト作成対象リスト付き）。

## F3 — 証明と実装の間隙（lowering は信頼コード）

**証拠（2026-07-03 実例）**: Coq 証明があるのは checker カーネルと Coown パターン
（CoownLoop.v / CoownCompose.v）。しかし証明書を**生成する** lowering 本体は Rust の
信頼コードであり、今回だけで 5 箇所の出力破壊（linearization、never-err strip の表現齟齬、
Const-0 グローバル、lambda ctx の layout 欠落、_start のスタック残留）を修正した —
**全て証明の外側**で起きた。「kernel-proven path」の proven は end-to-end ではない。

**規格上の意味**: DO-333 の formal credit は証明された性質にしか付かない。
lowering の正しさは現状 differential testing のみが担保しており、その網が F2。

**是正 work items**:
1. 「何が証明済みで何が信頼コードか」の**境界図**を docs/contracts/ 側に一枚で明文化する
   （現状は各ファイルのコメントに分散）。監査人が最初に要求する図。
2. 信頼コード側の縮小: lowering の出力不変条件（「Bind された Result は追跡集合に入る」
   「materialize と drop 分類は同一の型判定を使う」等、今回の修正が全て違反していた類）を
   **ランタイム assert でなく post-pass 検査**として機械化する。既存の
   `assert_names_resolvable` / ConcretizeTypes ゲートと同型の、追跡集合整合ゲートの追加。
3. 長期は certificate-format-v1 / value-rc-cert の per-function cert 化に合流。

**受入基準**: 境界図が存在し、追跡集合整合ゲートが CI で走り、今回の 5 バグの各クラスに
対応する再発防止ゲートまたは fixture が指させる。

## F4 — 検証の非決定性（flaky を「既知」で容認）

**証拠（2026-07-03 実例）**: output-parity のフルラン時に append_accumulator(_heap) /
list_eq(_float_bool) / string_codepoint(_index) の 2〜3 ファイルが**マシン負荷でのみ**落ち、
単体では常に byte-match する。原因はハーネスの 20 秒 `alarm` タイムアウトと並列 cargo 実行の
CPU 競合。私はこれを 2 回「baseline へ手動復元」して先に進んだ — 研究速度としては妥当でも、
**検証手順自体が非決定的である事実**は残っており、この手の容認は証拠能力を毀損する。

**規格上の意味**: 再現しない検証結果は結果ではない。「単体では通る」は追加の主張であって
証拠の代替にならない。

**是正 work items**:
1. output-parity のタイムアウトを wall-clock でなく**リトライ付き**にする（timeout 到達時は
   1 回だけ単独再実行し、その結果を採用。再実行でも落ちたら真の失敗）。
2. フルランを負荷分離する（`--test-threads` 相当の直列化、または gate 実行中の cargo build
   禁止をスクリプトで強制 — 既に flock の前例がある）。
3. 「baseline の手動編集」を廃止し、`--update` の出力のみを正とする（F5 と連動）。

**受入基準**: フルランを 5 回連続で回して REGRESSION 出力がゼロ。baseline ファイルの
git 履歴に手動編集コミットが以後現れない。

**クローズ（2026-07-03）**: 真因は負荷ではなく **sort/comm の照合順ロケール**（python の
バイト順 sorted と shell のロケール順 sort で `.` と `_` の順序が逆転 — 同一ファイルが
「新規」と「退行」の両方に出る自己矛盾ログが決定打）。`LC_ALL=C` を gate に固定し、
リトライは solo-once 方式（全非 match verdict を静穏時に 60s で再判定）。
受入基準達成: 5 連続フルラン green。2日間「マシン負荷」と誤診していた —
非決定性を容認せず根治せよという本所見自身の教訓どおりだった。

## F5 — 独立性の不在（変更者がラチェットを動かす）

**証拠（2026-07-03 実例）**: このセッションで私は (a) lowering を変更し、(b) その変更の
検証を実行し、(c) parity baseline を `--update` + 手動復元で更新し、(d) テスト期待値を
1 件書き換え（`match_arm_heap_payload_binding_aliases_the_subject` を壁期待に変更）、
全てを単独で行った。(d) は正当な変更（旧テストが miscompile 挙動を固定していた）だが、
**その正当性を判断したのも変更者自身**である。

**規格上の意味**: DO-178C の verification independence（DAL-A/B）は「作った者が
合否を決めない」こと。自動ゲートは強い緩和要因だが、ラチェット・期待値・壁化の
3 権限が変更者に集中している。

**是正 work items**:
1. baseline / 期待値 / 壁化(KnownBroken) の変更を**専用コミット**に分離する規約
   （lefthook で「実装変更と baseline 変更の同一コミット」を reject）。
2. それら専用コミットに、変更理由・退行でないことの証拠（単体ラン記録）を
   コミットメッセージで義務付ける。
3. 長期: レビューを人間または独立エージェントの承認に載せる（組織側、⑤の範囲）。

**受入基準**: lefthook ルールが存在し、直近 20 コミットで実装と baseline の混在ゼロ。

**クローズ（2026-07-03）**: `scripts/check-ratchet-separation.sh` + lefthook pre-commit。
負例（実装+baseline 混在 → exit 1）・正例（清浄 → exit 0）で検証済み。同日の
float_parse / total_order / case_unicode の 3 ratchet は全て分離コミットで実施。

## F6 — 構成管理インシデント未解明（バイナリ置換）

**証拠（2026-07-03 実例）**: 作業中に `~/.local/bin/almide` が**説明なく** v0.27.13
（Jun 22 ビルド）に置き換わり、spec が 2 件失敗した。私は `make install` で復旧して
先に進み、**原因（何が・いつ・なぜ上書きしたか）を未解明のまま**にした。並行エージェント・
cron・別セッションのいずれかの可能性があるが特定していない。

**規格上の意味**: 検証環境の構成が管理下にないことを示す実インシデント。
「その binary で取った証拠」の同一性が主張できない。

**再発（2026-07-03 14:00 頃、刻印ゲートが検出）**: F6-2 の刻印を導入した**当日に再発**し、
coverage 計測が刻印ゲートで正しく FATAL 停止した — 誤ったバイナリでの証拠採取は既に
構造的に不可能になっている。現行犯調査の結果: `~/.cargo/bin/almide`（0.14.8, Apr 18）・
cron・LaunchAgents・org 内スクリプトは全て白。置換後の mtime が「Jun 22 のまま」保存されて
いることから、リリース tarball 展開 + mv（mtime 保存転送）の指紋 — **このマシン上の並行
セッションが GitHub リリースの v0.27.13 を再インストールしている**可能性が最有力。
防止（uchg 等）は並行作業の正当な上書きを壊すため採らず、刻印による検出+停止を恒久対策とする。

**機構特定（2026-07-04、クローズ）**: ディスク全域走査の結果、almide バイナリは
`~/.local/{bin,almide}` と**本リポの target/release のみ**（別 checkout・worktree・
キャッシュは存在しない）。よって全ての置換は本リポの `make install` 経由であり、
犯人は**同一 working copy を共有する並行セッション**（同時期の frontend/codegen 5
ファイル M 状態、org への新リポ ceangal 追加とも整合）。古い 0.27.13 が現れたのは
並行セッションが別ブランチ/コミットをビルドしていた時間帯。対策は既設の刻印
（誤バイナリでの証拠採取は構造的に不可能）で完結しており、プロセス名の現行犯記録用に
watcher（/tmp/almide-binwatch.log）を常駐させた。

**是正 work items**:
1. インシデント記録として本所見を残す（済 — 本書）。
2. ゲート実行スクリプト冒頭で `almide --version` + バイナリの mtime/hash を**ログに刻印**し、
   期待ビルドと不一致なら即 fail する（1 行の防御で再発を検出可能にする）。
3. 検証実行時の toolchain 刻印（rustc/wasmtime/coqc バージョン）を receipt に含める —
   certificate-format-v1 と接続。

**受入基準**: 全 proofs/*.sh がバイナリ刻印を出力し、バージョン不一致で fail する。

**クローズ（2026-07-03）**: `proofs/lib/stamp.sh` を全ゲートに導入。導入当日に 2 回の
実ドリフト（再発した 0.27.13 置換 + stdlib 埋め込み更新前の古い workspace ビルド）を
FATAL 停止で捕捉 — 誤ったバイナリでの証拠採取は構造的に不可能になった。

## F7 — 既知の数値非互換（float.parse 境界丸め）

**証拠（2026-07-03 実例）**: self-host float.parse は mant×10^k スケーリング実装であり、
denormal（4.9e-324）・最大値（1.797e308）境界と 19 桁超 mantissa で 1ULP 級の丸め差が
v0（Rust strtod = Eisel-Lemire）に対して残る。parity の実 MISMATCH はこれ 1 件のみ。

**規格上の意味**: 数値精度要求のある系（航空はまさにそれ）では既知の丸め非互換は
単体でブロッカー。「ほぼ正しい」浮動小数点は無い。

**是正 work items**:
1. 正確な 10 進→f64 変換の self-host 実装（Eisel-Lemire または Simple Decimal Conversion
   の純 Almide 移植 — 独立 brick、工数大）。
2. それまでの間、ALS に現行実装の**精度保証範囲**を明記（「10^±22 内の 15 桁以下は exact、
   境界は 1ULP」級の定量文）し、契約 fixture でその範囲を pin する。

**受入基準**: `spec/wasm_cross/float_parse.almd` が byte-match、または ALS が精度範囲を
規範として定め fixture がそれを検証している（どちらかで所見クローズ）。

**クローズ（2026-07-03）**: 800 桁高精度10進（Simple Decimal Conversion）の slow path を
実装し `float_parse.almd` が byte-match（denormal 最小値 5e-324・f64 最大値・19 桁超
mantissa・half-even・巨大指数すべて Rust と一致）。fast path（15 桁以下 & |exp|≤22）は
従来の Clinger 安全域のまま。

---

## F8 — 新規所見（2026-07-03 夕）: ローカル PCC チェーンが暴いた cert 会計違反

**経緯**: F6-2 の一環で Coq（Rocq 9.1.1）をローカル導入し、`proofs/corpus-wall.sh` の
checker phase（従来 CI 委任で、ローカルでは一度も走っていなかった）を初めて全量実行した。
結果: **kernel-proven checker が ownership witness を REJECT**（20393 オブジェクト中、
`spec/integration/codegen_effect_fn_test.almd::parse_positive_even` — 連鎖 monadic `!` の
err 再構成 — の cert 行が bare `m`：取得イベントなしの move-out）。

**意味**: この関数の実行は parity で byte-verified（err パス fixture 含め green）であり、
実害（double-free/leak）の観測は無い。しかし cert が拒否される以上、この関数は
「accept ⟹ safe」の網の**外**にいる — 証明の保護を受けていない in-profile 関数が
存在していたことになる。mir>ir ゲート同様、「green に見えたのは検証が走っていなかった
から」の実例であり、F2（カバレッジの錯覚）の追加証拠。

**是正 work items**:
1. 犯人経路の特定と会計修正: `err(e) => err(e)` 再構成（mod_p6 の monadic desugar）の
   payload move-out に取得イベント（Dup/`a`）を対にするか、実際に二重所有なら実バグとして修正。
2. `ownership.names`（cert 行 ↔ 関数名の並記、本所見の調査で追加済み）を恒久化 — REJECT が
   即座に関数へ bisect できる。
3. corpus-wall の checker phase を**ローカル必須**化（coqc は導入済み）— 「CI 委任」は
   今回のような未実行の温床だった。
4. CI が本当にこの phase を回しているか確認（回していれば CI も赤いはず — 赤くないなら
   CI 側も未実行 = 構成の穴）。

**受入基準**: corpus-wall が checker phase 込みでローカル green、REJECT ゼロ。

**進捗（2026-07-03 夜）**: 違反 cert 行を全列挙する自前バランサで棚卸しし、**多数 → 2 件**へ。
1. **merge move-in の記帳**（根治済み）: ネストした monadic `!` の内側 merge dst が外側に
   Consume される時の bare `m` — released merge dst に arm からの move-in `i` を対で記帳
   （物理: アームの −1 と merge の +1 は同一参照の持ち替え）。backing gate（unbacked +1 検査）
   も同じ released-merge 集合で歩調を合わせた。checker ACCEPT 確認済み。
2. **via_if クラス（miscompile 8例目、根治済み）**: `let v = if c then boom(x) else boom(y)`
   （effect fn の auto-`?` が各アーム内に `Try` を置く scalar-typed if）で、scalar 経路が
   Try を黙って剥がし **Result ブロックの handle に +100 する raw 演算**を出していた —
   checker の leak 行（bare `i`）が本物のミスコンパイルを指した初の実例。tail-dup の対象を
   「heap 型 or アームに直接 Try/Unwrap を含む bind」へ拡張し、既存の monadic desugar に
   委ねて根治（via_if byte-MATCH、baseline 176 退行ゼロ、MIR 507/0）。
3. **残クラスも根治（同日深夜）**: `effect_tco::checked`（declared-Result tail-if スタイルは
   Consume を発行せず `EndIf {{ val }}` が実質 move — val-move 規則を追加、ループ機構の値は
   除外）と `bytes_set_value_semantics::rotate`（Dup 初期化 slot のイベントが of 解決と
   ValueId 直打ちで2行に分裂 — slot イベントを object_of 解決に統一、"a(id)m" に収束）。

**クローズ（2026-07-03）**: `proofs/corpus-wall.sh` が **checker phase 込みでローカル完全
green** — kernel-proven checker が ownership 20876 オブジェクト / names 4248 / caps 3481 /
caps-transitive 243 の全 witness を ACCEPT。PCC チェーンの全量ローカル実行はプロジェクト
史上初。副産物として本物のミスコンパイル1クラス（via_if）と cert 会計3クラスを根治。
同日3回目のバイナリ置換も刻印が FATAL 捕捉（並行セッション活発 — F6 恒久対策が機能）。

## 優先順位と依存

```
F6-2 (刻印)         ── 1日級、即効、単独
F4 (flaky 根絶)     ── 数日級、単独 — 以後の全証拠の信頼性の前提
F5 (権限分離)       ── 数日級、単独
F2-1 (coverage 計測) ── 数日級、単独 — 現状の直視
F3-2 (整合ゲート)   ── 週級 — 今回のバグ 5 クラスの再発防止
F1 (ALS)            ── certification-grade CG-1 に合流（最長・最重要）
F7 (strtod)         ── 独立 brick、F1 の精度規範が先でも可
```

小さく即効の F6-2 → 証拠の信頼性を回復する F4/F5 → 現状を直視する F2-1 → 構造的な
F3/F1 の順。**F1 と F2 が閉じるまで、「航空品質」を名乗る主張は evidence ladder の
rank を明示して限定的に行う**こと（無限定の "flight-grade" 表現の自粛）。
