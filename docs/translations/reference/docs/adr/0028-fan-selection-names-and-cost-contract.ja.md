<a id="adr-0028-replace-fanany-with---listfind_map-rename-fanrace-to-fanmin-with-a-required-clock-admit-the-real-time-clock-and-scope-the-cost-contract-to-a-meter-version"></a>

# ADR-0028: `fan.any` を `??` / `list.find_map` で置き換え、`fan.race` を clock 必須の `fan.min` に改名し、実時間 clock を受け入れ、コスト契約を meter version に限定する

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](0028-fan-selection-names-and-cost-contract.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](0028-fan-selection-names-and-cost-contract.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](0028-fan-selection-names-and-cost-contract.zh-CN.md)

> 固定原文の非規範的な歴史読者スナップショット。変更していない原本が正本。自動同期・独立ベンチマーク・現行安全性の証明・過去のコマンド実行は行っていない。設計採択は後続実装すべての完了証明ではなく、meter-version gate、順序付きprocess、#3788待ちのDuration clockを含めテスト計画の状態を保持する。証拠監査のpin 0746eb689はこの読者版の原文pinとは異なる。協調cancelは境界で敗者を止めて終了を待つもので、実行済みeffectは残り、wasm逐次scheduleは合法であってnative同時実行と同一ではない。他言語を一般化した文言とコスト同等性は原文当時の主張で、ここで独立再検証していない。同じmeter version内の限定は版をまたぐ安定性を意味しない。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0028-fan-selection-names-and-cost-contract.md)

- **状態**: Accepted（2026-10-18）。Q1 は2026-10-17に一つの修正（非推奨期間なし）を加えて批准。
  他のすべての問いは2026-10-18に他言語の設計との比較で決定した（§決定）。未決の問いはない。
- **日付**: 2026-10-17（提案。同日の言語横断の命名比較後に改訂）、2026-10-18（採択。
  最初の改訂での暫定名 `fan.first_ok` / `fan.min_cost` は置き換えた — §却下した代替案）。
- **範囲**: `fan` の選択形式（`fan.any`、`fan.race`）、その診断とドキュメント、
  言語に欠けていた実時間選択、`fan.min` と `fan.bounded` が選択に用いる `Compute` meter の契約、
  2026-10-17の監査で見つかった意味論上の問い（#3778–#3783）。
- **証拠**: [fan-semantics-audit.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/design/fan-semantics-audit.md)
  （0746eb689固定）、[fan-cross-language.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/design/fan-cross-language.md)、
  `tests/fan_acceptance_test.rs`、issue #3778–#3784。
- **導入先**: dialect epoch 14（`fan-selection-names`、
  `proofs/dialect-epochs.toml`）。`fan.min` の実時間 clock と
  `fan.timeout` の I/O 本体は後続 #3788。

<a id="context"></a>

## 背景

1. **Almide は5種類の選択のうち3種類を提供していた。**

   | 種類 | 選ぶもの | 本ADR以前のAlmide |
   |---|---|---|
   | K1 | 入力順で最初の成功 | `fan.any` |
   | K2 | 実時間で最初の成功 | – |
   | K3 | 成功・失敗を問わず最初の完了 | – |
   | K4 | 最小論理コスト（同値なら入力順） | `fan.race` |
   | K5 | すべて収集 | `fan.map` / `fan.settle` / `fan { }` |

   K2とK3は意図的に欠けていた。答えがスケジューリングに依存し、
   ADR-0024 N7 と SPEC §13.5（定理T9）はそれを決定的tierから除外している。
2. **どちらの名前も、誤った読み方のまま黙ってコンパイルされた。** JS `Promise.race`（K3）を
   期待する書き手は、pure armの `fan.race { a(), b() }` からK4を得た。
   `Promise.any`（K2）を期待する書き手は `fan.any` からK1を得た。
   どちらにも診断は出なかった。I/O armの `race` だけがE006になった。
3. **コスト値はコンパイラに依存する。** 消費量はinlinerとloweringが決めるIR/MIRの
   charge siteで数える（監査§8）。同じbuildの2本のnative経路が相互に、またwasmとも
   異なる（#3778）。本ADR以前の「すべてのtargetとmachineで同じ勝者・判定」という
   約束（CHEATSHEET、C-204/C-205）は、コンパイラ版について何も述べていなかった。
4. **最も一般的な実用形に綴りがなかった。** 「2台のserverに問い合わせ、先に答えた方を使う」
   （K2）は他言語の `race` / `any` の用途であり、横断比較表はJS、Rust、Python、Go、
   Haskell、Kotlin、Swiftにそれを示す。Almideの答えは「表現不能」だった。

<a id="naming-in-other-languages"></a>

## 他言語での命名

各行はライブラリが文書化した意味で、選択種類順に並べている。

| 名前 | 場所 | 選択の種類 | 順序保証 |
|---|---|---|---|
| `race` | JS `Promise.race` | K3 最初の完了、rejectを含む | なし（wall time） |
| `race` | Haskell `async` `race :: IO a -> IO b -> IO (Either a b)` | K3 最初の完了、**敗者はcancelされる** | なし |
| `select` / `select!` | Rust `futures`、`tokio`; Kotlin `select`; Go `select`（仕様上、ready caseからランダム選択） | K3 | なし、または設計上ランダム |
| `firstCompletedOf` / `anyOf` / `WhenAny` | Scala `Future`; Java `CompletableFuture.anyOf`; C# `Task.WhenAny` | K3 | なし |
| `any` | JS `Promise.any`; Rust `futures::select_ok`（同じ種類） | K2 | なし |
| `findAny` / `find_any` | Java `Stream.findAny`; Rayon `find_any` | 任意の一致、**明示的に非決定的** | なし |
| `any` | Python `any()`、Rust `Iterator::any`、Haskell `any` | 選択ではなくBool | – |
| `find_map` / `find_value` / `firstNotNullOfOrNull` / `collectFirst` | Rust `Iterator::find_map`、OCaml `List.find_map`; Elixir `Enum.find_value`; Kotlin; Scala | **K1** — すべてLIST/iterator関数で、並行処理primitiveではない | 入力順 |
| `findFirst` / `find_first` / `find_map_first` | Java `Stream.findFirst`; Rayon `find_first`、`find_map_first` | **K1** | 入力順 |
| `asum` / `<\|>` / `or_else` / `.catch(() => b())` | Haskell `Alternative`; Rust `Result::or_else`; JS | **2〜3候補のK1**: 遅延評価するfallback chain | 入力順 |
| `min_by_key` | Rust `Iterator::min_by_key`: 「複数要素が同じ最小値なら、最初の要素を返す」 | **K4そのものの規則**（最小key、同値なら最初） | 同値なら入力順 |

結果:

- **`race` という語がある全ecosystemで「最初に完了」を意味する**。
  Haskellでは敗者もcancelする。Almideの `race` は最初の完了でもcancelでもなかった。
- **`any` は順序付き選択を意味しない。** 「順序保証なし」
  （`findAny`、`find_any`、`Promise.any`）かBoolを意味する。
- **順序付き最初の成功はどこでもlist関数**であり、候補が少数なら遅延fallback chainである。
  定義上逐次的なので、他言語は並行処理combinatorにしていない。
- K4の規則（最小key、同値なら最初）は `min_by_key` の文書化された契約。
  その定着した語が **`min`** である。
- Almideは以前にも同じことをしていた。0.42.0で `fan.race(thunks)` を削除した理由は
  「名前が、言語が意図的に持たないtiming動作を約束していた」からだった（SPEC §fan、C-004）。
  現役の `fan.race` headは同じ名前で同じ欠陥を繰り返していた。

<a id="how-other-compilers-rename-a-public-name"></a>

## 他のコンパイラは公開名をどう改名するか

| コンパイラ | 仕組み | ユーザーに見えるもの |
|---|---|---|
| Swift | `@available(*, deprecated, renamed: "new")` | 新名へのワンクリックfix-it付き警告 |
| Kotlin | `@Deprecated(message, ReplaceWith("new(...)"), level)`、releaseごとにWARNING → ERROR → HIDDEN | IDEが全使用箇所を置換。旧名は消える前に明確なerrorとして残る |
| Rust | `#[deprecated(note)]`、`cargo fix` が適用するmachine-applicable suggestions、破壊的綴り変更にはedition（`cargo fix --edition`） | `rustc` が置換先を表示し、1コマンドでcrateを移行 |
| Go | API移動用 `go fix` rewriter | 1コマンドでmoduleを移行 |
| Almide | stdlib関数の `@deprecated(since=N, use="…")`、`proofs/dialect-epochs.toml` の `[[deprecation]]`、診断の `try:` 置換、E027 tombstone | 同じ3要素 |

他言語はすべて、旧名を最後には*置換先をなお示す*errorにするのであって、未知の識別子にはしない。
決定（2026-10-17）: Almideは警告期間を省き直接その最終状態へ進む。改名が1 tokenの機械的変更で、
corpusが小さく、黙った期間を設けると誤読がもう1 epochコンパイルされ続けるためである。

<a id="logical-cost-the-contract-to-state"></a>

## 論理コスト: 明記する契約

- **単位。** 1 charge unit。版付き校正常数CM-1（v0.3 = 3 ns/unit、
  `crates/almide-types/src/time_units.rs`）により宣言時間から変換する。
- **Charge site。** `docs/specs/als/deterministic-time.md` の表そのもの:
  - fn entry。ただしloopなし・非再帰calleeを除く（C-294）。
  - 各loop-head評価。
  - bulk string連結を `1 + len/16` で。
  - それ以外はなし。
  #3778はnativeとwasmがそれぞれ別の行に当時違反していることを示す。
  修正は両legを表に従わせることであり、表を変えることではない（後述C）。
- **Budget範囲。**
  - `fan.bounded`: 本体。
  - `fan.min(compute…) { }`: 各armに別々に全budget。
  - `fan.min(compute…, xs, f)`: 各要素に別々。
  - 外側regionがmin-capする。当時sourceはregionを入れ子にできない。
- **引数。** `{ f(g(x)) }` のような単一call本体では、meter開始前に `g(x)` が実行された（監査§6）。
  後述Eはこれをregion内へ置く。
- **版管理。** 判定と勝者は（program、inputs、**meter version**）の関数である。
  同じmeter version内ならすべてのtargetとmachineで同じになる。
  charge表、その除外規則、校正常数が変わる場合にのみmeter versionが変わる。
  変更はspend ledger差分とともにrelease notesで知らせる。spend ledgerをgateとする:
  regionごとのspendを固定したfixture corpus（`fuel_bounded_boundary.almd`、
  `fuel_dyn_charge.almd`、acceptance testの `heavy(1000)` = 1002 units）で、
  固定spendに変更があれば、同じPRでmeter versionを上げない限りCIが失敗する。

<a id="decision"></a>

## 決定

すべての項目の背後の規則: **名前が選ぶものを示し、clockの型が何で測るか・どのtierで動くかを示す。**
`compute.*` はpure本体と全machineで同じ答え、`duration.*` はoracle tier、
つまりeffectが許され、判定は記録されたhost inputとなる。

1. **Q1（2026-10-17）: 旧名を即座に退役させ、非推奨期間は置かない。**
   dialect epoch 14から `fan.race` と `fan.any` は置換先を示すE027 tombstoneとなる。
   parserはblock headを報告し（`crates/almide-syntax/src/parser/fan.rs`、`renamed_fan_head_text`）、
   checkerはcall形式を同じ文面で報告する（`static_dispatch.rs`）。
2. **A（2026-10-18）: `fan.any` は改名ではなく削除。** source順の最初のOkは
   `a() ?? b() ?? fallback`、list上では `list.find_map(xs, (x) => f(x)?) ?? fallback`。
   両方既存であり、`??` は左辺失敗時にのみ右辺を評価する。
   `list.find_map` はeffect callbackを受け入れる — 2026-10-18にnative/wasmで実測。
   理由: 他言語（Rust `find_map`、OCaml `List.find_map`、Elixir `find_value`、Kotlin
   `firstNotNullOfOrNull`、Scala `collectFirst`）はlist libraryに置き、2候補形式は
   どこでも遅延fallback chain（`or_else`、`.catch(() => b())`、`<|>`）である。
   逐次選択にfan形式を持つのは1意味2綴りだった。
3. **修正後Q1（2026-10-18）: `fan.race` は `fan.min(clock)` になり、clock引数は必須。**
   `fan.min(compute.ms(5)) { a(), b() }` と `fan.min(compute.ms(5), xs, f)`。
   `fan.min { … }` と `fan.min(xs, f)` はparse error。
   理由: `min` はこの規則に対する `min_by_key` の語で、必須clockにより
   `fan.min { 3, 5 }` を最小値と読むことを防ぐ。暫定 `min_cost` は、何を最小化するかを
   clock型が既に示すため却下した。
4. **H（2026-10-18）: 実時間clockを受け入れる。**
   `fan.min(duration.ms(n)) { … }` / `(duration…, xs, f)` はarmを一緒に開始し、
   最初にOkで完了したarmが勝ち、armはI/O可能、**入口から1つのdeadlineを共有**する
   （armごとではない）。敗者を次のcharge siteまたはI/O境界でcancelし、形式から戻る前に
   終了を待つ。敗者の実行済みeffectは残り、stdoutは破棄し、勝者の値だけを返す。
   nativeはarmを並列実行し、wasmはasyncを得るまで順番に実行する
   （ADR-0024 D8。arm 0が成功時に勝つのも合法なschedule）。ω record/replayは勝者indexを固定する。
   理由: 他言語の `race` / `any` が扱う形であり、実行ごとに勝者が変わりうるtrade-offも
   各言語が受け入れるものだから。構造化停止はHaskell `race`、Python `TaskGroup`、Kotlin、Swiftに従う。
   Almideには非構造化spawnがないので、JS式の放置継続は却下する。
   共有deadlineは `Promise.race([a, b, timer])`、tokio `timeout(dur, select!)`、
   `asyncio.wait(tasks, timeout=…)`、単一Go `ctx` の方式である。
   armごとのdeadlineでは逐次wasm legのwall timeがarm数に応じて増える。
   `fan.timeout(duration…)` も同じcancel規則でI/O本体を受け入れる。
   初稿の「K2とK3を除外」を部分的に逆転し、K2を入れる。K3は入れず、失敗armは飛ばして選ばない。
   後続: #3788。導入されるまで `Duration` clockは `fan.min` で型エラーとなる。
5. **G（2026-10-18、×）: `fan.bounded` は `fan.timeout` と別名のまま。**
   clock型で決める規則はcut形式に拡張しない。
   理由: runtimeはfuel/gas制限とwall deadlineを別名にする — wasmtimeの `set_fuel` と
   `set_epoch_deadline`、Ethereumの「gas limit」とtimeout、tokioの `timeout` と `timeout_at`。
   異なる種類の制限を別名にするのが他言語の設計で、「timeout」はどこでもwall timeを意味する。
6. **B（2026-10-18）: コスト契約をmeter versionに限定する。** §論理コストのとおり。
   理由: Ethereumはhard fork（EIP-150、EIP-2929）ごとにfork idの下でgas価格を改定し、
   wasmtimeはinstructionごとのfuelが版で変わりうると文書化する。
   版をまたぐ約束はinlinerのあらゆる変更を禁じてしまう。
7. **C（2026-10-18、#3778）: nativeとwasmはcharge表に従う。**
   nativeはloopなしwrapper entryへのchargeをやめ、wasmはstring連結にchargeする。
   既存の勝者・判定はmeter versionの下で変わりうる。
   理由: 同じ表の2実装での不一致はconsensus bug（Ethereum clientでは最優先）であり、許容差ではない。
8. **D（2026-10-18、#3780）: armが順序付きprocess操作に到達する `fan { }` はnativeでも逐次実行する。**
   対象は `env.set`（ADR-0024 D3の集合へ追加）。
   理由: Rustはedition 2024で `env::set_var` を `unsafe` にし、Goの `t.Setenv` は
   `t.Parallel` 下でpanicする。他言語は組合せを拒否するが、Almideは直列化で決定性を保ち、
   nativeの観測をwasmに合わせる。check時拒否は将来の強化案として残す。
9. **E（2026-10-18）: `fan.bounded` / `fan.timeout` / `fan.min` の単一call本体の引数を計測region内へ置く。**
   理由: gasとwasmtime fuelは実行instructionすべてを数える。引数の仕事を飛ばす制限は制限ではない。
10. **F（2026-10-18）: `fan.min` の全失敗文面は `fan.min: no branch
    succeeded within budget`。** clock必須なのでbudgetは必ず存在し、1文面で
    「全armがErr」と「全armがcut」の両方を扱う。`succeeded` は前者も表す。
    （JS `Promise.any` は「All promises were rejected」と報告する。文面は失敗を示すのであり、
    書き手が設定していないbudgetを示すのではない。）

<a id="alternatives-rejected"></a>

## 却下した代替案

- **A、両方の名前を残す。** 他言語は `race` を「最初の完了」、`any` を「順序なし」と読む。
  残せば両誤読が黙って通る。（これは初稿の推奨だった。JSだけに対して改名costを比べ、
  `race` と `any` の共通意味がecosystem横断でどれほど広いか調べていなかった。）
- **暫定名 `fan.first_ok` / `fan.min_cost`**（最初の改訂、2026-10-17）。
  `first_ok` は他言語が順番に試す選択を並行libraryへ置かず、逐次選択は既に `??` と
  `list.find_map` で表せるため却下。`min_cost` はclock型が最小化対象を示すのでsuffixが冗長で、
  `Duration` clockへの道を閉ざすため却下。
- **clock省略可能な `fan.min`。** `fan.min { 3, 5 }` は最小値に読める。
  引数こそが `min` をarmの選択にする。
- **`fan.min(duration…)` のarm別deadline。** 他言語になく、逐次legで総wall timeがarm数倍になる。
- **clock型で `fan.bounded` を `fan.timeout` へ統合（G）。** 決定5により却下。
- **K2を言語に入れない**（初稿Q2）。他言語でraceの最も一般的な用途であり、trade-offも共通に受け入れ、
  oracle tier（ω record/replay）は `fan.timeout` のために既にある。
- **JS式に敗者を放置。** Almideには非構造化spawnがなく、形式から戻った後も敗者が動けばそれに該当する。
- **版をまたいでmeterを固定。** regionを持つあらゆるprogramでinlinerとloweringの変更を禁じることになる。

<a id="consequences"></a>

## 帰結

- **得るもの**:
  - 各選択名が他ecosystemの語で選択対象を示し、他言語から持ち込む習慣が正しい形式を示す診断に変わる。
  - 最も一般的な実用形（「先に答えたserver」）に構造化cancel付きの綴りができる。
  - 「clock型がtierを決める」という1規則が `fan.min` と `fan.timeout` を覆う。
  - コンパイラ更新時、meter versionから勝者が変わりうるか分かる。
- **払うもの**:
  - 3つのbreakを伴うepoch entry（14）を1つ追加し、例、CHEATSHEET、llms.txt、Dojo taskを書き換える。
  - `fan.any` runtime（self-hostの `any_map` 9 cell、chain desugar）は削除が入るまでdead code。
  - spend ledgerというgateと、release notesのmeter versionという数字が1つずつ増える。
  - `fan.min(duration…)` と `fan.timeout` のI/O本体にはnativeのcancel機構
    （charge siteとI/O境界で確認する協調flag）とwasmの逐次scheduleが必要。

<a id="falsifier"></a>

## 反証条件

usability評価（`docs/design/fan-usability-eval.md`）は、D-a/D-c付き旧名のC1と採択表層のC2を比較する。
C2がF1 + F2 + F5の誤読率をC1比で少なくとも3分の1減らさなければ、改名は本ADRの主張する
便益を得ておらず、まず診断文面を見直す。

<a id="test-plan"></a>

## テスト計画

| 変更 | テスト | leg | 状態 |
|---|---|---|---|
| Q1 / A: 旧名が置換先付きE027 | `tests/diagnostics/e027-fan-race-renamed-head`、`e027-fan-any-removed`、thunk-list case; `scripts/check-dialect-epochs.sh` がepoch 14の `breaks` を見る | check | epochとともに完了 |
| A: 置換先が逐次・遅延 | `spec/wasm_cross/fan_any_early_winner.almd`（`??` chain: 後続候補は実行しない）、`fan_any_allfail.almd`（全候補失敗→fallback）、mapper fixtureのeffect callback付き `list.find_map` | native, wasm | epochとともに完了 |
| Q1: `fan.min` がclock必須 | `fan.min { }` と `fan.min(xs, f)` はparse error; `tests/fan_surface_matrix_test.rs` | check | epochとともに完了 |
| F: 全失敗文面 | `fan.min(compute.ms(1)) { err("a"), err("b") }` → `fan.min: no branch succeeded within budget` | 4 legs | epochとともに完了 |
| C / #3778: meterが表に従う | `tests/fan_acceptance_test.rs`: `a_loop_free_wrapper_costs_the_same_on_every_leg` と `a_cut_body_that_interpolates_stops_at_the_same_point_on_every_leg` のignore解除 | 4 legs | 完了（#3778: native既定経路が `meter_plan` を読み、interpolationでなく `+` にcharge; `fuel_wrapper_entry_exempt.almd`、`fuel_interp_uncharged.almd`） |
| B: meter version gate | 固定spendをすべて再計算し、同時に `METER_VERSION` が動かなければ失敗するscript | CI | 後続 |
| #3779: `fan { list.map }` のtrap順序 | `a_trap_in_a_parallel_list_map_is_the_lowest_index_one` のignore解除、20回実行 | native par / seq, wasm | 2026-10-18完了（配分blockごとに1 timeline; `fan_list_map_trap_order.almd`、acceptance testのignore解除; #3784も同変更でclose） |
| D / #3780: 順序付き操作がfanを逐次化 | `fan_arms_that_share_process_state_agree_across_targets` のignore解除; `env.set` 付きblockが逐次生成され、なしではそうならないcodegen test | 4 legs | 後続 |
| E: 単一call引数を計測 | `fan.bounded(compute.ns(30)) { id(heavy(100000)) }` が全legでErr; `fan.timeout` と `fan.min` も同様（`tests/fan_acceptance_test.rs` `a_single_call_bodys_arguments_are_inside_the_region`、`spec/wasm_cross/fuel_single_call_args.almd`） | 4 legs | 完了（#3790） |
| H: `fan.min(duration…)` | 実時間で最初のOkが勝者; 敗者println破棄; 敗者effect（書いたfile）は残る; 敗者停止後のみreturn; `ALMIDE_OMEGA` が両targetで勝者indexをreplay; wasmはarm 0 | native, wasm | #3788 |
| H: 共有deadline1つ | 各々deadlineの60%を要する2 arm: 逐次legで2番目がcut | native, wasm | #3788 |
| H: I/O本体付き `fan.timeout(duration…)` | 本体のrequestを送信し、次境界でcut、Errはdeadlineのもの | native, wasm | #3788 |
| #3783（空本体）、#3781（wasm ω replay）、#3782（Float勝者） | #3786で導入 | 4 legs | 完了 |

<a id="ratification-record"></a>

## 批准記録

| 問い | 判定 | 日付 |
|---|---|---|
| Q1 改名、非推奨期間なし | ○ | 2026-10-17 |
| 暫定名 `first_ok` / `min_cost` | × — Aと `fan.min` で置換 | 2026-10-18 |
| ① `fan.any` のlist形式 → `list.find_map` | ○ | 2026-10-18 |
| ② `fan.race` → `fan.min`、clock必須 | ○ | 2026-10-18 |
| ②b `fan.min(duration…)` を受け入れる | ○ | 2026-10-18 |
| ②c 次境界で敗者cancel、終了待機; I/O許可 | ○ | 2026-10-18 |
| A `fan.any` block形式 → `??` chain | ○ | 2026-10-18 |
| B コスト契約をmeter versionに限定 | ○ | 2026-10-18 |
| C meterが表に従う（#3778） | ○ | 2026-10-18 |
| D `env.set` armはnativeで逐次（#3780） | ○ | 2026-10-18 |
| E 単一call引数を計測 | ○ | 2026-10-18 |
| F 全失敗文面 | ○（`no branch succeeded within budget`） | 2026-10-18 |
| G `fan.bounded` を `fan.timeout` に統合 | × | 2026-10-18 |
| H 実時間clockに共有deadline1つ | ○ | 2026-10-18 |
| Q7 空本体、Q8 wasm ω replay | ○ — #3786で導入 | 2026-10-18 |

<a id="references"></a>

## 参考資料

- ADR-0001 (deterministic time units), ADR-0011, ADR-0020, ADR-0022, ADR-0024
- C-004, C-005, C-185, C-199, C-200, C-204, C-205, C-207, C-208, C-294, C-320
- Haskell `async`: [race](https://hackage.haskell.org/package/async/docs/Control-Concurrent-Async.html#v:race) · Java [Stream.findFirst / findAny](https://docs.oracle.com/en/java/javase/21/docs/api/java.base/java/util/stream/Stream.html)
- Rayon [find_first / find_any / find_map_first](https://docs.rs/rayon/1.10.0/rayon/iter/trait.ParallelIterator.html) · Rust [Iterator::min_by_key](https://doc.rust-lang.org/std/iter/trait.Iterator.html#method.min_by_key), [Iterator::find_map](https://doc.rust-lang.org/std/iter/trait.Iterator.html#method.find_map), [Result::or_else](https://doc.rust-lang.org/std/result/enum.Result.html#method.or_else) · OCaml [List.find_map](https://ocaml.org/manual/5.2/api/List.html) · Elixir [Enum.find_value](https://hexdocs.pm/elixir/Enum.html#find_value/3)
- 種類別の制限: wasmtime [Store::set_fuel](https://docs.rs/wasmtime/latest/wasmtime/struct.Store.html#method.set_fuel) / [set_epoch_deadline](https://docs.rs/wasmtime/latest/wasmtime/struct.Store.html#method.set_epoch_deadline) · tokio [timeout](https://docs.rs/tokio/latest/tokio/time/fn.timeout.html) / [timeout_at](https://docs.rs/tokio/latest/tokio/time/fn.timeout_at.html) · Ethereum gas repricing [EIP-150](https://eips.ethereum.org/EIPS/eip-150), [EIP-2929](https://eips.ethereum.org/EIPS/eip-2929)
- 環境と並列処理: Rust [std::env::set_var (unsafe since edition 2024)](https://doc.rust-lang.org/std/env/fn.set_var.html) · Go [testing.T.Setenv](https://pkg.go.dev/testing#T.Setenv)（「並列テストでは使えない」）
- 共有deadline: tokio `timeout(dur, select!{…})` · Python [asyncio.wait](https://docs.python.org/3/library/asyncio-task.html#asyncio.wait) (`timeout=`) · Go [context.WithTimeout](https://pkg.go.dev/context#WithTimeout)
- 改名: Swift [`@available(renamed:)`](https://docs.swift.org/swift-book/documentation/the-swift-programming-language/attributes/#available) · Kotlin [`@Deprecated` / `ReplaceWith`](https://kotlinlang.org/api/core/kotlin-stdlib/kotlin/-deprecated/) · Rust [`cargo fix`](https://doc.rust-lang.org/cargo/commands/cargo-fix.html)
- MDN [Promise.any](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/Promise/any) / [Promise.race](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/Promise/race)
