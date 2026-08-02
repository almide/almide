<!-- description: Audit of the best-async-of-2026 claim: the axis, the rivals, the five runaway moves -->
# The 2026 Async Claim — audit and the runaway plan

> 憲章: [async-inception.md](./async-inception.md)。本文書はその主張監査である。
>
> 「2026 年世界最高の非同期文法」と**言えるか**の監査と、**ぶっちぎりにする**ための五手。
> 設計は [logical-time-async.md](./logical-time-async.md) / [fan-v2.md](./fan-v2.md)、
> 証明は [logical-time-proofs.md](./logical-time-proofs.md)。本文書は主張の側を扱う —
> どの一文なら審査に耐え、どの一文はまだ嘘になるか。

> **実装状況（2026-08-03、branch `worktree-stage1-charge-probe` — 食い違いは
> BURNDOWN/契約台帳が正）**: 下の実行順節は起草時の「これから」の体のまま残す。
> 現在地は以下のとおり。
>
> - **Lane 1 完了**: Wave 1（b817e3dc）→ Stage 1（8ece4153 + 4db109aa）→
>   Stage 2（4152da7b）→ Stage 3（113feee2）。CM-1 v0.3 = 3ns/unit、境界は
>   ns 精度 exact flip、契約 C-202..C-207 + 3-way corpus 常設 —
>   **claim 1–3 の解禁条件は成立**。
> - **Lane 2 B2 完了**: `fan.timeout(duration.ms(n)) { body }`（f8d760ba、
>   C-208 + ALS-D5）。
> - **Lane 2 B1 は timeout-ω スライスのみ**: ω = 期限が切れた wall check の序数。
>   `ALMIDE_OMEGA_RECORD=1` 採録 → `ALMIDE_OMEGA=<n>` コンパイル時 bake 再生、
>   record(native) → replay(native + wasm) byte 一致を常設 gate で検査。
>   **効果応答テープ（fs/http/random/process/env の ω スキーマ + `--record` /
>   `--replay` + redaction）は未着手** — claim 4 は「timeout 事象の決定的再現」に
>   射程を限定すれば今日言える。全文の解禁は B1 完了後（下のスケッチが仕様）。
> - **Lane 3 D は Almide 側完了**: dojo async バンク 7 タスク **7/7 pass**
>   （cli:claude、retry 込み — dojo runs/2026-08-02/summary.md）。
>   **claim 5 の全文解禁には同一モデル比較 lang-bench(async) が残る**。
> - **追補**: race の mapper 形も実装済み（T7-1）— fan v2 matrix は全セル確定。
> - **C（KPN）**: 設計どおり未着手（地平のまま）。

## 判定（先に結論）

**今日、無条件の「世界最高の async」は言えない。今日でも言い切れる唯一性の軸は 4 本ある。
五手が揃えば、無条件版に実質等しい claim が審査に耐える。**

- 言える（証拠つき）: 「**決定的・クロスターゲット・機械証明済みの async 意味論**を持つ
  言語は、代表的な言語には他に見当たらない」
- まだ言えない: 「async 全般で最高」— 実運用 async（耐障害サーバ、成熟エコシステム）で
  Erlang/OTP と Go に現時点で負けており、oracle 層（I/O レース・タイムアウト）は未実装。
- 言ってはいけない: 実装が Stage 1–3 を終えるまで、race/bounded を「ある」と語ること。
  現状は設計+証明であり、出荷済みなのは fan{} / map / any / settle まで。

## 戦場の名指しと勝算

5 文は武器のリストであって、戦場ではない。戦場は一つに名指しする:

> **コードの大半を LLM が書く時代の並行性。** エージェントが書いた並行コードが走る
> たび同じ結果を返し、壊れたら必ず再現できる、という性質が資産になる場所。

耐障害サーバを人間が運用する世界（Erlang/Go の戦場）では**戦わない** — 負けの表の
うち Erlang 行は譲った負けである。5 文の持ち場: 1〜3 = 検証可能の武器、4 = 再現可能の
武器、5 = スコアボード。

| claim | 勝算 | 根拠 | 残リスク |
|---|---|---|---|
| 1 機械証明 | 勝ち済み | 7 定理 0 sorry | 単独では市場を動かさない（信頼の担保であって需要ではない） |
| 2 観測等価 | 高 | 199+ 契約とゲートの既存基盤 | Stage 1 の charge-trace 保存（一点に特定済み） |
| 3 計算コスト race | デフォルト勝ち | 作っている言語が見当たらない | 先行例の発見（反証条件）。学術的な旗であり日常需要は薄い |
| 4 record/replay | **最大の勝ち筋** | 需要は Temporal の商用成功が実証。moat は構造的（非決定性の通り道が registered surface 一箇所） | 実装スコープ（ω スキーマ、redaction） |
| 5 LLM 実測 | 有利だが本物のリスクあり | lang-bench 先行データ（2026-07-15、Almide 100% vs Gleam 19/20） | LLM は Go/TS async を大量に訓練済み。誤りクラスの構造的消去で上回れるかは測定が決める |

この戦場での本当の対抗馬は表の五人ではなく**「ランタイムで足りる」論**（Temporal 型
replay の普及）である。反論は構造で立つ: Temporal は決定性を規約で要求し、違反は
実行時に発覚する。**規約は LLM が破る。構造は破れない** — MSR の一行目と同じ論理。
戦場の時価は上がっている: AI コーディングは成長側の市場で、deterministic simulation
（Antithesis / FoundationDB 流）への注目も追い風。今は小さいが値上がりする土地に、
構造的 moat 付きで最初に立つ、が勝算の要約である。

## 唯一性の軸 — 今日すでに世界で一人の場所

### 軸 1: 命令粒度の論理時計

| 系 | 論理時計の粒度 | 純粋計算同士を race できるか |
|---|---|---|
| Verse | event / simulation tick | **不可のはず** — suspension を含まない式は同一 tick 内で完了するため、純粋計算の race は常にソース順先頭に潰れる（docs からの推論、実機未検証） |
| Lingua Franca | timestamp（イベントのタグ） | 不可 — 時間はイベント由来で、計算コストは時計に乗らない |
| Esterel 系 | 同期 tick | 不可（同上） |
| EVM | **命令粒度（gas）** | そもそも並行構文がない |
| **Almide（設計）** | **命令粒度（fuel）** | **可 — fan.race(fuel:) の存在理由そのもの** |

「Verse の race を EVM の gas の上で走らせる」は、両者のどちらも単独では持たない分解能を
作る。**計算コストで勝者を決める race は、この表の誰も持っていない。**

### 軸 2: クロスターゲット観測等価の内側での async

Go/Rust/Kotlin/Swift の async はシングルターゲットの意味論で、決定性は外部ツール
（loom、race detector、rr）がテスト時に近似する。Almide は native ⇄ wasm の観測等価
契約（199+ 契約、ratchet 付き）の**内側**に async を置き、しかも中断点（charge site）
まで両ターゲットで一致させる設計にした。Swift 6 の strict concurrency が消すのは
data race（型レベル）であり、非決定性ではない。

### 軸 3: 実装に**先行**する機械証明 + 全数合流ゲート

- Lean kernel-check 7 定理・0 sorry（選択の一意性、cap の可視窓保存、合流）
- 74,898 構成 × 全物理スケジュール（遅延 fuel 検査の overrun 込み）の confluence 検査
- 証明作業が設計バグ 3 件を実装前に検出・修正した記録

loom（Rust）や Coyote（.NET）、P 言語（AWS で実運用）は**テスト時にスケジュールを
探索する**装置であり、言語の意味論として合流を証明して出荷前に固定した例は、代表的な
ものには見当たらない。Verse には core calculus の形式化（Verse calculus, ICFP 2023）が
あるが、concurrency/transactional 層の機械証明が公開されているとは確認できていない。

### 軸 4: 「最高」を外部指標で測る文化

MSR（modification survival rate）と lang-bench は同一モデル・同一タスクでの実測であり、
「LLM が最も正確に書ける」を主張ではなく測定にする。async 版タスクバンクが D 手（下記）。

## 負けている軸 — 正面から認める表

| 相手 | 勝っている点 | Almide の現状 |
|---|---|---|
| Erlang/OTP | 耐障害性（supervision、let-it-crash）、30 年の実運用 | 構造的リカバリなし。trap は決定的に死ぬだけ |
| Go | 実運用 async の成熟度、goroutine の使い勝手 | oracle 層が未実装で「本物の並行 I/O レース」が書けない |
| Verse | transactional effects が出荷済み、Fortnite で実運用 | 効果隔離は Rung 0（pure）設計のみ |
| Lingua Franca | 分散（federated）決定性、学術的蓄積 | 分散は視野外 |
| Temporal 等 durable execution | replay による永続実行が商用で実在（ただし言語ではなくランタイム） | **ここが五手 B の取りどころ** |

この表を隠して「最高」と言えば、audit（この repo の academic-rigor 規範）で落ちる。

Lingua Franca の行には、審査で必ず来る問いが畳まれている — 「logical time なら LF が
先では？」。答えは一行で立つ: **LF の時計は事象の間を並べ、Almide の時計は計算の内側で
刻む**。LF は C / Python / TS / Rust を束ねる調整層であり、reaction の中身（target 言語の
コード）を所有しないから、計算を可搬に計量できない — 計量は codegen の所有を要求する
（record/replay の moat と同じ構造）。逆に分散の決定的調整は LF が正しく先行しており、
Almide が地平の先（KPN チャネル → 分散）へ踏み出す日が来れば、そこでの LF は競合では
なく借用元である。負けの性格は「地平の負け」— 追わない Erlang とも、時間の問題の
Go/Verse とも、奪い返す Temporal とも違う第四の型として記録する。

## ぶっちぎりの五手

### A. Stage 1–3 を実装して出荷する

紙は競合しない。fuel・bounded・race が両バックエンドでゲート付きで動いて初めて、
軸 1–3 は「設計の話」から「言語の性質」になる。**五手の中で唯一の前提条件。**

### B. oracle 層を「record/replay」として設計する — 決定打

Stage 4 を「timeout の再導入」ではなく **言語内蔵の決定的 record/replay** として設計する。

**主張の形**: 決定層は `observe = f(program, input)` を確立した。oracle 層は
`observe = f(program, input, ω)` — 環境の応答列 ω を**明示的な入力**にする。ならば:

    almide run app.almd --record trace.ω     # ω を採録
    almide run app.almd --replay trace.ω     # 同一実行を再現 — 別ターゲットでも

replay の正しさは新しい定理を要らない — 関数の再適用である（T9 の系）。**flaky な
async バグは、再現可能な成果物になる。**

**なぜ Almide だけが安く手に入るか（moat の構造）**: 競合は非決定性が言語中に散在する
（Go は goroutine スケジューリング自体が非決定で、rr は syscall+スケジューリングまで
採録する羽目になる。Temporal は workflow コードに決定性を**規約で**要求し、違反は
実行時に発覚する）。Almide は決定層が既に全てを固定しており、非決定性は oracle 効果の
**狭い registered surface**（fs/http/process/random/env — self-host registry 経由）しか
通らない。テープを貼る場所が構造的に一箇所しかない。**B は Almide には配線工事、
競合にはアーキテクチャ変更**であり、これが「ぶっちぎり」の実体である。

**設計スケッチ**（Stage 4 の設計文書で本化する）:

- ω のスキーマは効果モジュールごとに定義（http: status/headers/body、fs: 内容/エラー、
  random: 値列、datetime.now: 値列、process: exit/stdout/stderr）。採録するのは
  **応答値であって時刻ではない** — 時間が意味論に入るのは timeout の裁定だけで、
  それは「どの charge site で切れたか」という**離散事象**として採録する。
- 消費点は charge site（中断点統一原理）なので、ω の消費順は決定層が既に固定している。
  ゆえに replay はターゲット非依存 — **native で採録し wasm で replay** が定義から成立し、
  これ自体が R_Ω 契約の実行可能形（executable oracle contract）になる。
- `fan.timeout(ms:)` は「ω に timeout 事象がどの site で現れたか」を読む構文になる。
  record 時は実時計が事象を生み、replay 時はテープが再生する。
- fan{} の並行 I/O は join-all + arm 出力バッファリング（#1026 系）と合成 — ω は
  arm ごとの応答列で、interleaving は観測に入らないから採録不要。
- 契約: 「record した ω での replay は観測を byte 一致で再現する」を C-NNN + fixture 化。
  これは R_Ω の存在証明を CI に常駐させることに等しい。
- 境界も明記: ω にはネットワーク応答等が入るため、テープは機密になりうる。redaction は
  スキーマ側の責務として設計する（値の型は保ち内容を伏せる）。

### C. 決定的チャネルの地平を名指しでロードマップに載せる

論理時計の上の KPN（Kahn process network）型チャネル + tick 順 select は、#1000 が
「structured concurrency は契約が先」と留保した再訪条件を満たす具体的経路。Erlang の
耐障害性そのものは取りに行かない（別の製品）が、「チャネルがない」という評価軸の穴は
これで**設計上の空白から時期の問題**に変わる。

### D. async 版の実測を dojo に積む

fan v2 タスクバンク（fan-out、フォールバック、投機、bounded 全域化）で MSR を測り、
Verse / Go / Rust / TS と同一モデル・同一タスクの lang-bench 比較を出す。lang-bench の
前例（2026-07-15、Almide 100% vs Gleam 19/20）と同じ流儀。**「LLM に最高の async」は
測定値として言う。**

### E. claim の文言を evidence つきで固定する

外に出す一文はこれらに限る（各行が evidence に 1:1 で対応）:

1. 「Almide の並行構文は、実行スケジュールがどれでも観測が変わらないことが
   **Lean で機械証明**されている」 — race belt 7 定理
2. 「async の意味論は **native と wasm で観測等価**であり、契約台帳とゲートが
   それを常時検査している」 — 契約 + fixture（Stage 1–3 出荷後）
3. 「計算コストで勝者が決まる race を持つ言語は他にない」 — 軸 1 の表
   （反例が出たら取り下げる、と添える）
4. 「async のバグは record/replay で**必ず再現できる**」 — B 出荷後のみ
5. 「LLM が最も正確に async を書ける言語である」 — D の測定値が出てからのみ

「世界最高」という語そのものは使わない。上の 5 文が揃った状態は、その語より強い。

## 実行順 — 2 レーン並列 + 地平 1 本

依存を解くと A→B→C の直列ではない。B の大半（B1）は fuel に依存しないので並走できる。

**Lane 1（本線、直列 — worktree: 主）**

1. **fan v2 Wave 1** — any/settle の block/mapper 形 + thunk tombstone + spec 8 ファイル
   移行 + matrix gate。fuel 不要で今すぐ着手可。race を最初から v2 の形で迎えるための
   先行工事。
2. **Stage 1: CM-1 + charge-trace 保存** — プログラム全体の falsifier なので Wave 1 の
   直後に置く。**ここでは region 特殊化を作らない**: 隠しフラグ（例 `--fuel-probe`）で
   全体計測ビルドを作り、fixture 三点比較（result / consumed / trace）と validator を
   先に立てる。特殊化機構は表面が来る Stage 2 まで遅延。
3. **Stage 2: fan.bounded** — v2 block form + region 特殊化 + CM-1 定数の確定と台帳
   登録（同一 PR）。
4. **Stage 3: fan.race** — 枝刈り cap、trap 遅延判定、入れ子 streaming、E027 改訂。
   race belt / spike のゲートを CI の受理条件として配線。→ **claim 1–3 が解禁**。

**Lane 2（並走可 — worktree 分離）**

- **B1: record/replay 基盤** — fuel 非依存。現行言語は oracle 効果以外すべて決定的
  なので、ω の採録・再生は今日の表面で既に意味を持つ。効果 surface の棚卸し
  （registered surface の oracle 効果列挙）→ ω スキーマ → tape 形式 → `--record` /
  `--replay` → replay 等価契約 + fixture。→ **claim 4 が解禁**（B2 前でも
  「effectful プログラムの決定的再現」として主張可能）。
- **B2: fan.timeout(ms:)** — ω 事象を charge site で読む形。これだけ Stage 1 に依存
  するので、Lane 1 の 2 完了後に Lane 2 へ合流。

**Lane 3（dojo 側）**

- **D: async タスクバンク** — v0 は Wave 1 後の表面（fan{} / map / any / settle）で
  作れる。race / bounded タスクは Stage 3 後に追加。→ **claim 5 が解禁**。

**地平（着手しない）**

- **C: KPN チャネル** — Stage 3 完了後に設計文書のみ起こす。実装は #1000 の再訪条件
  （構文ごとの契約が先）を満たしてから。

## 反証条件

- 軸 1 の反例: 命令粒度の決定的 race を持つ言語・処理系が見つかる（研究言語含む）。
  → 表を訂正し、claim 3 を取り下げる。探索は D の比較作業と同時に行う。
- Verse の純粋計算 race に関する推論が誤り（実機で分解能がある）。→ 軸 1 の Verse 行を
  訂正。推論であることは表に明記済み。
- B の redaction が実用にならない（ω が本質的に機密で共有不能）。→ replay は
  ローカル・CI 内デバッグ機能に格下げし、claim 4 の射程を狭める。
- D の測定で Almide の async MSR が競合に勝てない。→ 文法の問題なら fan v2 の反証条件
  （block/mapper 2 択の再設計）へ、診断の問題なら診断改善レーンへ。measurement wins。
