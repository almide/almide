<!-- description: The async inception: thesis, grammar, semantics, proofs, claim and plan in one charter -->
# Async Inception — logical time as the language's time base

> 非同期設計の憲章。この一枚で全体が読め、深掘りは末尾の文書地図から辿れる。
> 個別文書と食い違ったら**本文書が正**（食い違い自体が修正対象のバグ）。

## 0. 一文

**壁時計は観測を決めない。言語の時間基底は論理時間（fuel）であり、環境の時間は宣言された
入力としてのみ入る。**

## 1. 物語 — 二度削除した言語だけが、三度目に正しく作れる

Almide の非同期は、足し算ではなく引き算の歴史でできている。

- **第一の削除（2026-03）**: `async` / `await` / `Future` を言語から消した。`effect fn` が
  非同期境界であり、待つことは書くものではなくコンパイラが挿すもの。LLM が await を
  忘れる・重ねるという誤りのクラスが構文ごと消滅した。
- **第二の削除（0.29.0 / 0.42.0）**: `fan.timeout` と `fan.race` を消した。壁時計の
  タイムアウトは「プログラムと入力の関数」でない唯一の表面であり、race は決定的モデルの
  下で意味を与えられない名前だった。SPEC の§13.1「どちらで実行しても結果は同一」と
  §13.2「速い方が勝つ」は両立せず、**捨てたのは後者**だった（#1000、決定的データ並列の
  stance）。
- **第三幕（本設計）**: 削除の理由そのものが復活の設計図になる。race が死んだのは
  メトリクスがなかったからだ。**fuel が枝間で比較可能な決定的メトリクスを与えたとき、
  そしてそのときに限り、race は逐次評価と厳密同一の観測を持つ**。stance の原理
  「リスト順の意味を与えられないものは言語に入れない」は破られない — 適用されるのだ。

この弁証法が設計全体の背骨である: 妥協して残すのではなく、正しく消し、正しく取り返す。

## 2. 合成 — 何を借り、何が世界で初めてか

| 系譜 | 借りたもの | 借りられなかったもの |
|---|---|---|
| Verse `race` | 勝者選択の UX、「同時ならソース順」、構造化キャンセル | 論理時計が event 粒度 — 純粋計算同士をランク付けする分解能がない |
| EVM gas / EIP-150 | 決定的 out-of-gas、入れ子の min-cap、コスト表=意味論 | 並行構文そのものがない |
| Wasmtime fuel | 決定的中断 vs 壁時計中断という区別の命名 | wasm op 粒度 — 最適化不変はソース意味論粒度でしか得られない |
| Lingua Franca / Esterel | 論理時間、同時刻の宣言順解決 | 時計がイベント由来で計算量が乗らない |
| Par monad / LVars | 制限による決定性、quasi-determinism という退却先の命名 | fuel・選択・キャンセルの統合 |

**新しいのは合成である**: 命令粒度の論理時計をメトリクスとする race を、敗者の非観測性・
入れ子予算・trap 規則まで込みで、native ⇄ wasm の観測等価契約とその検証装置ごと一体で
定義したこと。これができるのは `fan` がライブラリではなくコンパイラ既知の構文であり、
**予算配分・勝者選択・効果可視性の三点をコンパイラが所有できる**からだ — 言語の形に
関する主張であって、一機能の主張ではない。

## 3. 文法 — fan v2: head × form の一枚

fan は「実行ポリシー」の文法である。全構文は head（ポリシー）× form（block = 静的 arm 列
| mapper = 動的リスト）の 1 セルで、thunk（`() =>`）は表面から全廃される。

| head | 選択規則 | block form | mapper form | arm 効果上限 |
|---|---|---|---|---|
| （無印）all | 全部。リスト順先頭 Err | `fan { a; b }` → `(A, B)` | `fan.map(xs, f)` → `List[B]` | effect 可 |
| settle | 全収集 | `fan.settle { a; b }` → `(Result[A], Result[B])` | `fan.settle(xs, f)` | effect 可 |
| any | index 最小の成功（逐次フォールバック） | `fan.any { a; b }` → `T` | `fan.any(xs, f)` | effect 可 |
| race | (spend, index) 最小の成功 | `fan.race(fuel: n) { a; b }` → `T` | `fan.race(fuel: n, xs, f)` | **pure**（Rung 0） |
| bounded | 単一 body の計量 | `fan.bounded(fuel: n) { body }` → `T` | —（map と合成） | **pure** |
| timeout | 環境が切る | `fan.timeout(ms: n) { body }` | — | oracle 可（Stage 4） |

- 族の軸は「何を最小化するか」一本: any は index、race は fuel。all/settle は選択しない。
- auto-wrap の頭ごとの例外は消え、**form 単位の規則**になる: block arm は `fan {}` 流
  （auto-unwrap）、mapper は `fan.map` 流（Result 必須）。
- `fuel:` / `ms:` はラベル必須。ラベル引数機構は言語にないが、fan head は関数でなく
  構文なので fan 文法自身が供給する（汎用ラベル引数は導入しない）。
- 主要イディオムは 1 行に畳める: `fan.bounded(fuel: n) { optimal() } ?? greedy()`。
- 却下も文法の一部: `rush`（敗者効果が漏れる — 恒久却下）、trailing lambda・`fan.all`
  別名（共存負債）、`limit:`（未実装だったと判明、必要が実証されてから）。
- LLM の決定木は 2 つだけ: 「静的に並べるか、データで量産するか」×「全部 / 最初の成功 /
  最安の成功 / 上限だけ」。async/await/Future/task handle は引き続き存在しない。

## 4. 意味論 — 五本の柱

### 柱 1: 論理時計（CM-1）

charge site は共有 MIR の basic block 入口。**charge は最適化前の MIR 誕生時に付与し、
全パスが注釈を保存する**。ゆえに `-O` 不変は検査項目ではなく構成的に成立し、RC・
MakeUnique・move は無料（ソース意味論のコストではない — EVM gas が JIT と無関係なのと
同型）。コスト表は versioned な意味論的オブジェクトとして契約台帳に載り、定数変更は
semantic change。well-formedness（W1: 全サイクル・全再帰は charge site を通る、W2: site
間の仕事は有限）が実装の受理条件。**charge site は言語の唯一の中断点**であり、fuel 系は
そこでカウンタを、oracle 系は同じ場所で環境を読む。

### 柱 2: 決定的事象規則（race の全て）

各枝は決定的 charge トレース。事象 = (累積 fuel, 枝 index)、merge 順はその辞書式 —
「1 tick に 1 fuel、同時はソース順」の lockstep をスケジューラなしで言い切った形。

> **Complete / Trap の終端事象を merge 順に並べ、最初の決定的事象が唯一の裁定者。**
> Complete ならその枝が勝者（= 完了枝の (spend, index) 辞書式最小と一致）、Trap なら
> プログラムがその trap で落ちる、存在しなければ `Err(exhausted)`。

### 柱 3: 敗者の非観測性 — キャンセルは最適化

観測は「採用値・trap の可視窓・（入れ子時の）消費量」のみ。枝刈り cap（記録済み候補
d に対し枝 k を `d.time − (k > d.idx ? 1 : 0)` で切る）は決定的事象とその可視窓を
**絶対に隠せない**（Lean 定理）。ゆえに native がいつ敗者を止めても、fuel 検査が
何ステップ遅延しても、観測は不変。予算は **per-branch**（枝の追加が他枝の意味を
変えない — modification survival の直接の論拠）。

### 柱 4: 予算の入れ子 — streaming merge

入れ子の実効予算は `min(自身, 外側残量)`（EIP-150 と同構図）。race の消費は
**occurred stream**（決定的事象に merge 順で先行する charge 列）として外側が streaming
で観測し、途中で尽きればその点で外側 Exhausted。意味論的消費量をカウンタから引く —
実装が実際に費やした仕事ではなく。

### 柱 5: 効果の三層と不可能性定理

Almide の効果規律（E007/E008、単一エラーチャネル）により、効果は {pure | 出力 | oracle}
に最初から三分されている。race/bounded の枝は v1 で pure（Rung 0）、出力は勝者のみ
決着後 flush する transactional 層（Rung 1）へ。そして **T9**: 環境の壁時計で勝者を選ぶ
構文は「観測が (プログラム, 入力) の関数」という決定層の定義と矛盾する — **I/O レースと
タイムアウトが oracle 層（ω を宣言された入力とし R_Ω で関係付ける契約クラス）にあるのは
選好ではなく定理**である。

## 5. 証明 — 三層の証拠と、証明が直した設計バグ

| 層 | 中身 | 所在 |
|---|---|---|
| Lean kernel-check | 選択代数 7 定理・0 sorry: 一意性、部分集合安定性、cap の決定事象・可視窓保存、合流 | `crates/almide-race-belt/`（CI `lean-proofs`） |
| 全数モデル検査 | 74,898 構成 × 全物理スケジュール（overrun 込み）で参照意味論・逐次 scan・敵対的並列・入れ子 streaming が outcome + consumed + occurred stream まで一致 | `research/spike/logical-time-race/`（`run-gate.sh`） |
| 紙の定理 | T1 全域性 〜 T9 不可能性、有界断片の有限観測性（2-safety の成立条件を閉じる） | `logical-time-proofs.md` |

証明は儀式ではなかった — **設計バグを 3 件検出した**: ①勝者不在時の trap 規則が未定義
→ 決定的事象規則に統一 ②消費量 Σ min(end_j, s\*) が境界で過大 → occurred stream に置換
③入れ子の「原子的 site charge」は裁定点が定義不能 → streaming に修正。加えて事実誤認
1 件（`limit:` ラベル引数の実装済みという stale 記録への依拠）も訂正マーク付きで修正済み。
**証明しないもの**も明記する: 実装（renderer）の意味論適合は charge-trace validator +
fixture の領分（proven-vs-trusted の境界）。

## 6. 主張 — 外に出してよい 5 文と解禁条件

「世界最高」という語は使わない。使うのは evidence と 1:1 の 5 文で、揃った状態はその語
より強い:

1. 「並行構文はどのスケジュールでも観測が変わらないことが **Lean で機械証明**されている」
   — 今日から可
2. 「async の意味論は **native と wasm で観測等価**、契約台帳が常時検査」 — Stage 3 後
3. 「**計算コストで勝者が決まる race** を持つ言語は他にない」 — Stage 3 後（反例が
   出たら取り下げると添える）
4. 「async のバグは record/replay で**必ず再現できる**」 — B1 後
5. 「LLM が最も正確に async を書ける言語である」 — dojo 実測後のみ

負けている軸も憲章に残す: Erlang の耐障害性、Go の成熟度、Verse の transactional 出荷。
これを隠した瞬間、この文書は広告になり憲章でなくなる。

## 7. 計画 — 2 レーン並列 + 地平 1 本

```
Lane 1（本線・直列）        Lane 2（並走・worktree 分離）      Lane 3（dojo）
─────────────────           ──────────────────────            ─────────────
1. fan v2 Wave 1            B1: record/replay 基盤             D: async タスクバンク v0
   (any/settle 統一、        (fuel 非依存 — 今日の表面で        (Wave 1 の表面で)
    thunk 全廃)               ω 採録・再生が成立)
2. Stage 1: CM-1 +               │                                 │
   charge-trace 保存             │                                 │
   (--fuel-probe で              │                                 │
    特殊化なしに falsify)        │                                 │
3. Stage 2: fan.bounded          │                                 │
   (特殊化 + CM-1 定数)          │                                 │
4. Stage 3: fan.race        B2: fan.timeout(ms:)               race/bounded タスク追加
   → claim 1–3 解禁          (Stage 1 の site に合流)            → claim 5 解禁
                             → claim 4 解禁
地平: C = KPN チャネル（Stage 3 後に設計文書のみ。#1000 の再訪条件を満たしてから）
```

B が決定打である理由: 非決定性の通り道が registered surface 一箇所しかない Almide には
record/replay は配線工事であり、スケジューラ自体が非決定な競合にはアーキテクチャ変更に
なる。「native で採録し wasm で replay」は R_Ω 契約の実行可能形として定義から成立する。

## 8. 反証条件（統合）

- **charge trace が lowering で保存できない**（renderer peephole が最有力容疑）—
  Stage 1 の三点 fixture が検出し、Stage 2 以降を無効化する。これが最大の技術リスク
  なので Lane 1 の先頭に置いてある。
- **W1/W2 を満たせない配置が必要になる** — T1（全域性）が崩れ、主張を取り下げる。
- **計測オーバーヘッドが region 限定でも許容外** — charge 粒度の粗化か AARA 前倒し。
- **pure race に実ユースが薄い** — Rung 1 前倒し、それでも薄ければ race の価値再査定。
- **命令粒度 race の先行例が見つかる** — claim 3 を取り下げ、表を訂正（探索は D と同時）。
- **dojo 実測で async MSR が競合に勝てない** — measurement wins。文法なら fan v2 の
  再設計、診断なら診断レーンへ。

## 9. 文書地図

| 文書 | 役割 |
|---|---|
| 本文書 | 憲章 — 全体の正 |
| [concurrency-stance.md](./concurrency-stance.md) | 前提の決定: 決定的データ並列（#1000） |
| [deterministic-bounds.md](./deterministic-bounds.md) | 問題設定と正しさの議論（4 設問） |
| [logical-time-async.md](./logical-time-async.md) | 意味論の詳細（CM-1、決定的事象、入れ子、効果梯子） |
| [fan-v2.md](./fan-v2.md) | 表面の詳細（head × form、移行、却下表、Wave） |
| [logical-time-proofs.md](./logical-time-proofs.md) | 証明台帳（T1–T9、訂正記録、小スコープ但し書き） |
| [async-world-claim.md](./async-world-claim.md) | 主張の監査（競合表、五手、実行順の原本） |
| `crates/almide-race-belt/` | Lean 機械証明（0 sorry） |
| `research/spike/logical-time-race/` | 全数合流ゲート（GATE.md + run-gate.sh） |
