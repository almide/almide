<!-- description: Fan v2: the execution-policy grammar — heads x forms, thunk-free, labeled fuel -->
# Fan v2 — one execution-policy grammar

> 憲章: [async-inception.md](./async-inception.md)。本文書はその表面詳細である。
>
> [logical-time-async.md](./logical-time-async.md)（意味論）の上に、fan の**表面全体**を
> 一つの文法に畳む設計。[concurrency-stance.md](./concurrency-stance.md) の決定
> （決定的データ並列）は前提であり、変更しない。

## 統一テーゼ

**fan は「実行ポリシー」の文法である。** すべての fan 構文は

    head（ポリシー） × form（block = 静的 arm 列 | mapper = 動的リスト）

の 1 セルであり、それ以外の形は存在しない。thunk（`() => …`）は表面から**全廃**する。

現行の fan 表面は 3 つの呼び出し規約が混在している — block は式の列、`fan.map` は
(list, mapper)、`fan.any`/`fan.settle` は thunk リスト。しかも auto-wrap 契約が
コンビネータごとに違う（「race/any/settle thunks auto-wrap, map mappers do not」）。
v2 はこれを **head × form の直積 1 枚**に置き換える。

## v2 マトリクス（これが文法仕様の全体）

| head | 選択規則 | block form（静的 arm） | mapper form（動的） | arm 効果上限 | 並列化 |
|---|---|---|---|---|---|
| （無印）all | 全部。リスト順先頭 Err | `fan { a; b }` → `(A, B)` | `fan.map(xs, f)` → `List[B]` | effect 可 | 可（観測はリスト順） |
| settle | 選択しない。全収集 | `fan.settle { a; b }` → `(Result[A], Result[B])` | `fan.settle(xs, f)` → `List[Result[B]]` | effect 可 | 可 |
| any | **index 最小**の成功 | `fan.any { a; b }` → `T` | `fan.any(xs, f)` → `T` | effect 可 | 逐次（意味論ごと逐次） |
| race | **(spend, index) 最小**の成功 | `fan.race(fuel: n) { a; b }` → `T` | `fan.race(fuel: n, xs, f)` → `T` | **pure**（Rung 0） | 可（枝刈り付き） |
| bounded | 単一 body の計量 | `fan.bounded(fuel: n) { body }` → `T` | —（map と合成） | **pure** | — |
| timeout | 環境が切る（oracle 層） | `fan.timeout(ms: n) { body }` → `T` | — | oracle 可 | ホスト相対 |

- block form の arm は **式**（`fan {}` と同じ arm 契約: Result auto-unwrap、外側 `var`
  キャプチャ禁止）。any/race の arm は同一の `T` に unify する。all/settle は tuple なので
  異型でよい。
- mapper form の mapper は **Result を返す**（`fan.map` の既存契約に統一。pure mapper は
  今までどおり `list.map` へ誘導する #547 の診断を全 head に広げる）。
- bounded / timeout は arm 列ではなく **単一 body**（通常のブロック。`let` も書ける）を
  取る。「N-arm head」と「body head」の 2 種があることは文法表に明記する。
- 空の arm 列・空 body はコンパイルエラー（旧 race の空リスト規則を継承）。

**auto-wrap の不統一は form 単位の規則に置き換わる**: block form の arm は常に
`fan {}` 流（auto-unwrap）、mapper form の mapper は常に `fan.map` 流（Result 必須）。
head ごとの例外はゼロになる。

## thunk 全廃が買うもの

### 1. 表面から `() =>` が消える

```almide
// v1（現行）
fan.any([() => http_fetch(a), () => http_fetch(b)])
fan.any(mirrors |> list.map((m) => () => fetch(m)))

// v2
fan.any { http_fetch(a); http_fetch(b) }
fan.any(mirrors, (m) => fetch(m))
```

fan-concurrency.md §3.4「Why Thunks Are Needed」は、fan.* が**関数だから**遅延に thunk が
要るという説明だった。v2 では全 head が構文なので、この節ごと言語から削除される。
動的形も「thunk リストを組み立ててから渡す」2 段が「リスト + mapper」の 1 段になり、
コードは厳密に短くなる。

### 2. wasm の wall クラスが 1 つ死ぬ

現行の動的 thunk リスト（非リテラル）は v1 で `List[funcref]` が表現できず、
`desugar_fan.rs` はリテラル・let 束縛リテラルだけを inline し、真に動的なリストは
purity wall に落としている。mapper form は「データのリスト + 閉包 1 個」— `fan.map` と
同じ形 — なので、**この wall クラスは構文の変更だけで消滅する**。意味論の犠牲はゼロ。

### 3. `fuel:` ラベルの供給源になる

言語にラベル引数構文は**存在しない**（logical-time-async.md 初稿はここを `limit:` の
前例ありと誤認していた — 訂正済み）。v2 では fan head が関数呼び出しではなく構文なので、
`fan.race(fuel: n)` の `fuel:` は **fan 文法自身の要素**として実装できる。汎用ラベル引数
機構は導入しない。単位を持つ head 引数は必ずラベルを持つ（`fuel:` / `ms:`）— 単位の
誤読クラスを構文で殺す原則はここで統一的に効く。

## 各 head の意味論（確定事項の整理）

- **all / settle**: 現行のまま。C-004 / C-199 / C-200 は statement ごと生存する。
  arm 出力のバッファリング（リスト順 flush、C-004 EXCEPTION 節の退役）は
  logical-time-async.md の Rung 1 と部品を共有する別ワークストリーム。
- **any**: 現行の脱糖（match チェーン）が既に定義している**逐次フォールバック**を正式な
  意味論に昇格する。「リスト順に試し、最初の Ok で**打ち切る**。先行候補の効果は起こり、
  後続候補の効果は起こらない」。any は並行構文ではなく**フォールバック構文**である —
  これを仕様の文として明記する（pure arm に限り投機実行は不可観測な最適化として許す）。
  全滅時の Err は台帳定数（現行どおり）。
- **race / bounded**: 意味論は logical-time-async.md の全項（lockstep ≡ least-spend、
  per-branch 予算、trap 可視窓、EVM 式 min-cap、CM-1）。v2 はその表面を block form に
  確定させる。**bounded + `??` が主要イディオムになる**:

  ```almide
  let plan = fan.bounded(fuel: 100_000) { optimal_plan(g) } ?? greedy_plan(g)

  let ans = fan.race(fuel: 1_000_000) {
    exact_solve(input)
    heuristic_solve(input)
  } ?? default_answer
  ```

- **timeout**: Stage 4（Path C / R_Ω）まで tombstone 維持。再導入時の形だけここで
  確定する — `fan.timeout(ms: n) { body }`、中断点は charge site（logical-time-async の
  中断点統一原理）、契約クラスは oracle-relative。

## 完備性規則（matrix gate に載せる文）

1. すべての head は block form を持つ。
2. mapper form を持つのは「同型の動的データに対して arm を量産できる head」
   （all / settle / any / race）に限る。bounded / timeout は単一 body head なので持たない
   — これは意図的省略である。
3. `fuel:` はメトリクスを消費する head（race）と定義する head（bounded）に、`ms:` は
   oracle head（timeout）に、**ちょうど**現れる。無印 fan / map / any / settle に予算
   引数はない — fuel 次元は `fan.bounded` の合成で届くため（logical-time-async の
   完備性規則そのまま）。
4. 新しい head を足すときは、この表に行を足し、全列（選択規則・両 form の型・効果上限・
   並列化可能性）を埋めることが PR の受理条件（API 族の matrix 原則）。

## 移行 — 何が壊れて、どれだけか

**不動**: `fan { }` と `fan.map(xs, f)` — 最多使用の 2 表面は文字も変わらない。
契約（C-004/C-005/C-199/C-200）も statement ごと生存。

**破壊的**: `fan.any(thunks)` / `fan.settle(thunks)` の thunk リスト形。E0xx 移行
tombstone（check 時、書き換え例つき）にする。共存させない — 同じ意味の 2 綴りは
この repo が繰り返し払ってきた種類の負債である。

爆風半径は測定済み: リポジトリ内の使用は **spec 8 ファイル ~60 行**（stdlib はコメント
1 件、CHEATSHEET は 0 件）。各 fixture が何を pin しているかを読んで移す —
`fan.race` 撤去（#1024）で学んだとおり、機械置換は「通るが誤ったことを主張するテスト」
を作る。契約 C-005 系の statement は新形式の綴りに更新する（振る舞いは同一なので
契約の主張自体は不変）。

## 却下したもの（v2 の輪郭は否定側で確定する）

| 候補 | 判定 | 理由 |
|---|---|---|
| `fan.rush`（最速を返し敗者は走り続ける） | **恒久却下** | 敗者の効果が採用後も漏れ続ける構文は、決定的モデルで意味を与えられない。「リスト順の意味を与えられないものは言語に入れない」の適用例 |
| `fan.spawn` / `fan.link` | 地平送り | 非構造化・チャネルは logical-time-async の「将来の地平」（決定的並行性）が土台になってから。v2 には入れない |
| 汎用ラベル引数 | 導入しない | 言語全体の呼び出し規約の問いであり、fan 経由で密輸しない。`fuel:`/`ms:` は fan 構文の要素 |
| trailing lambda（`fan.map(xs) { x => … }`） | 導入しない | 綴りが 2 通りになるだけで意味が増えない |
| `fan.map(xs, limit: n, f)` | 入れない | 実装されていなかった（fan-concurrency-next の ✅ は stale、checker は 2 引数固定）。観測を変えないスケジューリングヒントは必要が実証されてから、別の機構（pragma 類）で |
| `fan.all { }`（無印の別名） | 導入しない | 最頻の形が最短であるべき。別名は共存負債 |

## MSR — v2 が生存率に効く形

- **決定木が浅くなる**: 「静的に並べるか（block）、データで量産するか（mapper）」×
  「全部要るか（無印/settle）、最初の成功か（any）、最安の成功か（race）、上限だけか
  （bounded）」。LLM が選ぶのはこの 2 択 × 4 択だけで、全セルの型が表から読める。
- **thunk 消滅**は「`() =>` を忘れる／余計に付ける」という編集破壊クラスをゼロにする。
- **arm 契約の単一化**（block は auto-unwrap、mapper は Result 必須）で、head を
  入れ替える編集（any → race 等）が型契約の再学習なしに生存する。
- 走るたび同じ、はそのまま: v2 に非決定的なセルは 1 つもない（timeout は oracle 層で
  契約クラスごと分離）。

## 実装 Wave（logical-time-async のステージと交差する）

1. **Wave 1 — 表面統一（fuel 不要、今すぐ可能）**: any/settle の block form + mapper
   form、thunk リスト形の E0xx tombstone、spec 8 ファイル移行、SPEC.md §13 再記述、
   CHEATSHEET、formatter、interp ブリッジ、matrix gate 新設。parser は
   `fan.IDENT` の後の `(head-args)` と `{`/`(` の分岐を追加。
   あわせて **async/await の死骸を AST/IR から撤去する**: lexer にキーワードは既に
   存在せず parser の `async_` は false 固定（2026-08-01 確認）だが、
   `ExprKind::Await` / `IrExprKind::Await` / `Decl::Fn` の `r#async` フィールドと
   fmt / interp / optimize の形骸アームが残っている。「文法から書けない」を
   「表現できない」に格上げする。
2. **Wave 2 — 決定層の完成**: `fan.bounded`（logical-time Stage 2）→ `fan.race`
   （Stage 3）を v2 の形で。E027 改訂もここ。
3. **Wave 3 — oracle 層**: `fan.timeout(ms: n) { }`（Stage 4）。

Wave 1 が独立に着手可能であることが v2 の要点の一つ — 表面の統一は fuel 意味論の
実装を待たない。

## 反証条件

- parser の `fan.IDENT ( … ) { … }` 分岐が既存の式文法と衝突する（member call +
  後続 block の曖昧性）。→ fan が予約語で primary から専用パースである限り起きない
  はずだが、Wave 1 の最初に fixture で確定させる。
- any/settle の移行で pin が壊れる fixture が出る。→ #1024 の手順（1 ファイルずつ
  pin を読む）を踏む。8 ファイルは半日仕事の規模。
- mapper form の race に実ユースが出ない。→ セルは宣言のまま Wave 2 では
  block form だけ実装し、matrix gate に「宣言済み・未実装」を明示させる。
- 「block か mapper か」の 2 択が LLM に不利に働く証拠が dojo の計測で出る。→
  MSR タスクバンクに fan v2 セットを足して測る。設計の当否は計測が決める。
