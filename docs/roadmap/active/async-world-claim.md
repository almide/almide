<!-- description: Audit of the best-async-of-2026 claim: the axis, the rivals, the five runaway moves -->
# The 2026 Async Claim — audit and the runaway plan

> 「2026 年世界最高の非同期文法」と**言えるか**の監査と、**ぶっちぎりにする**ための五手。
> 設計は [logical-time-async.md](./logical-time-async.md) / [fan-v2.md](./fan-v2.md)、
> 証明は [logical-time-proofs.md](./logical-time-proofs.md)。本文書は主張の側を扱う —
> どの一文なら審査に耐え、どの一文はまだ嘘になるか。

## 判定（先に結論）

**今日、無条件の「世界最高の async」は言えない。今日でも言い切れる唯一性の軸は 4 本ある。
五手が揃えば、無条件版に実質等しい claim が審査に耐える。**

- 言える（証拠つき）: 「**決定的・クロスターゲット・機械証明済みの async 意味論**を持つ
  言語は、代表的な言語には他に見当たらない」
- まだ言えない: 「async 全般で最高」— 実運用 async（耐障害サーバ、成熟エコシステム）で
  Erlang/OTP と Go に現時点で負けており、oracle 層（I/O レース・タイムアウト）は未実装。
- 言ってはいけない: 実装が Stage 1–3 を終えるまで、race/bounded を「ある」と語ること。
  現状は設計+証明であり、出荷済みなのは fan{} / map / any / settle まで。

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

## 反証条件

- 軸 1 の反例: 命令粒度の決定的 race を持つ言語・処理系が見つかる（研究言語含む）。
  → 表を訂正し、claim 3 を取り下げる。探索は D の比較作業と同時に行う。
- Verse の純粋計算 race に関する推論が誤り（実機で分解能がある）。→ 軸 1 の Verse 行を
  訂正。推論であることは表に明記済み。
- B の redaction が実用にならない（ω が本質的に機密で共有不能）。→ replay は
  ローカル・CI 内デバッグ機能に格下げし、claim 4 の射程を狭める。
- D の測定で Almide の async MSR が競合に勝てない。→ 文法の問題なら fan v2 の反証条件
  （block/mapper 2 択の再設計）へ、診断の問題なら診断改善レーンへ。measurement wins。
