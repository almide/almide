<!-- description: Audit of ticks against timeout-shaped APIs: the unwritable-number problem, three fixes -->
# Ticks interface audit — is the demanded API shape different?

> 憲章: [async-inception.md](./async-inception.md)。`ticks:` 確定（be4e7d4c）を受けて、
> 他言語の timeout 系インターフェースとの違和感と、「求められている API の形が実は
> 違うのではないか」を精査した記録。結論は設計修正 3 点で、各文書に反映済み。

## 他言語の形と、違和感の正体

| 系 | 形 | 単位 | 数字の供給者 |
|---|---|---|---|
| Go | `context.WithTimeout(ctx, 5*time.Second)` | 時間 | 人間の直感。deadline は context で**伝播** |
| JS | `AbortSignal.timeout(5000)` | 時間 | 人間の直感 |
| Rust/tokio | `timeout(Duration::from_secs(5), fut)` | 時間 | 人間の直感 |
| Python | `asyncio.wait_for(coro, timeout=5.0)` | 時間 | 人間の直感 |
| Erlang | `receive after 5000` | 時間 | 人間の直感 |
| Verse | race の枝に `Sleep(5.0)` | 時間 | 人間の直感（timeout = race の一枝） |
| EVM | gas limit | 計算量 | **ツール**（eth_estimateGas）+ マージン |
| Wasmtime | fuel | 計算量 | **ホスト運用者**が経験的に校正 |

上 6 行の共通点は「単位が時間」かつ「数字に直感が効く」。下 2 行だけが計算量で、
どちらも**数字を人間の直感で書かせていない** — ツールか運用が校正する。

違和感の正体はここにある。`ticks:` は timeout の代替綴りではなく**別の量**であり、
時間の予算（SLA・応答性・外部 I/O の見切り）は oracle 層の `ms:`（Stage 4）と
ホスト境界が担う。この住み分けは正しい。**問題は単位ではなく数字である**:

> `ticks: 100_000` は誰にも見積もれない。CM-1 は versioned な抽象コスト表で、人間にも
> LLM にも事前直感がない。しかも予算は body の実装に結合する — `optimal_plan` を
> リファクタすれば 100_000 は腐る。**編集で腐る魔法数を必須引数にするのは、
> modification survival を掲げる言語の自己矛盾に近い。**

## 修正 1 — race の `ticks:` を任意化する

race の選択規則（(spend, index) 最小の成功）は予算を**必要としない**。予算の役割は
発散ガードだけである。予算なしの race は well-defined:

```almide
fan.race { exact(p); heuristic(p) }   // 予算なし — 発明すべき数字がない
```

- 意味論は既存定義の n = ∞ 特殊化。枝 0 を走らせ、その spend が以後の cap になる
  （最初の完了が現れるまでは cap なし）。
- 停止性: 完了枝より先（リスト順）に発散枝があればハング。これは `fan {}` の既存仕様
  （「停止しない兄弟がいればハングするのは仕様」、#1023）と**同じ危険クラス**であり、
  新しい穴ではない。ガードが欲しい枝構成（探索の暴走など）では `ticks:` を付ける:

```almide
fan.race(ticks: 1_000_000) { search_a(p); search_b(p) }   // ガード付き
```

- `fan.bounded` の `ticks:` は**必須のまま** — bound することが構文の存在理由だから。
- Lean / モデルの定理は予算をパラメータに取っており、n = ∞ でもそのまま成立する
  （発散を含むケースは有限観測に到達しないだけで、合流主張は停止ケース上の主張）。

MSR への効き: 最頻ケース（ポートフォリオ 2〜3 枝）から発明定数が消える。LLM は
`fan.race { a; b }` とだけ書けばよく、「timeout のつもりで ticks に時間を書く」誤用の
圧力も、そもそも数字を書かないことで消える。

## 修正 2 — 数字の供給者はツールとホスト境界

数字が要る場面で、書くのは直感ではなく計測にする。

1. **校正ループ**: Stage 1 の `--fuel-probe` はそのままユーザー向け計測器になる —
   `--ticks-report` で region / プログラムごとの消費 tick を実測し、マージンを掛けて
   予算にする。EVM の estimateGas と同じ運用形。CM-1 改版（semver-major）で再校正。
2. **ホスト境界の決定的予算**: `almide run --ticks n` / `almide test --ticks n` —
   プログラム全体を bounded main で包む。数字の持ち主が **harness / CI** になり、
   ソースに定数が入らない。壁時計の CI タイムアウトと違い、**flaky にならない
   hang killer**（同じ入力なら同じ tick で切れる）。エージェントが書いたコードを
   決定的に bound する、という戦場（async-world-claim の名指しした戦場）への最短の
   製品はむしろこれである。実装は「main を bounded で包む」だけで新意味論ゼロ。

## 修正 3 — 「timeout ではない」を明文化する

- ticks は計算量の予算であり、時間の予算ではない。時間が欲しい読者は oracle 層
  `fan.timeout(ms:)`（Stage 4）か、ホスト境界（`timeout 5 ./app`）へ。fan-v2 の
  診断にもこの誘導を含める（既存の pure 制約ヒントと同型）。
- Go の context が**値で**運ぶ deadline 伝播を、Almide は入れ子の min-cap が
  **意味論で**運ぶ — 内側の region は外側の残量を暗黙に継承する（EIP-150 式）。
  cancellation token / AbortSignal に相当するユーザー可視の取り消し API は存在しない
  （キャンセルは構造的・最適化であり、観測に現れないため渡す対象がない）。

## 「単位を時間にしないとだめでは？」への回答

比較表の上 6 行が全部時間である以上、この問いは必ず来る。答えは二段である。

**決定層を時間にはできない — 好みではなく定理（T9）。** 壁時計は host 速度の関数で
あり「観測 = (プログラム, 入力) の関数」と矛盾する。時間にした瞬間、native ⇄ wasm の
観測等価・record/replay・flaky ゼロが同時に崩れる — `fan.timeout` を 0.29.0 で撤去した
理由そのものに戻る。ここは譲れないのではなく、譲る対象が存在しない。

**時間を求める需要には、時間で答える。** 主流の需要（SLA・応答性・見切り）が時間形で
あることは比較表が示す事実で、その受け皿は oracle 層 `fan.timeout(ms:)` である。
本監査の帰結としてその**優先度を引き上げる**: B2 は Lane 2 の「おまけ」ではなく、
時間形需要への正式回答として B1 と同格の deliverable に置く。ticks は時間の代替では
なく、**時間では買えないもの**（決定性・replay・sandbox・flaky ゼロの CI）を買う人の
ための別商品である — 二層に分かれていること自体が、この問いへの設計上の回答になる。

**検討して却下した中間案 — 時間風単位の tick 換算。** 参照レート（例: 参照機で
1ms ≈ N tick）を CM-1 に pin し、ソースに `budget: 100.ms` と時間風に書かせて
コンパイル時に tick へ定数変換する案。決定性は保てるが、`100.ms` と書けて壁時計と
一致しない API は、`fan.timeout(1000)` の誤読事故を**逆向きに再演**する（時間に見える
ものは時間として読まれる — 0.29.0 の教訓）。却下。ただし直感の接続はツール側で行う:
`--ticks-report` は tick 数に**この機械での実測時間を併記**する
（`52,000 ticks (≈0.4ms here)`）— gas エコシステムが estimateGas と gas/sec 相場で
直感を接続しているのと同じ解法である。

## 反証条件

- 予算なし race のハングが dojo / 実地で事故クラスとして観測される → race の
  `ticks:` を必須に戻し、本文書のこの節を訂正マーク付きで反転する。
- `--ticks-report` の実測が CM-1 改版のたびに大きく揺れて校正が実用にならない →
  CM-1 の安定性契約（定数変更の頻度制限）を台帳側で強める。
- ホスト境界 `--ticks` に需要がない（誰も使わない）→ ラダーから落とす。
  逆に需要が集中したら、fan.bounded の言語内表面の優先度を下げて再配分する。
