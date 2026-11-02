説明：Closure Architecture v2 — 1 つの識別子、1 つのキャプチャ集合、リフティングは低水準化。クロージャの表現とインライン化の最適化を分離する
歴史的完了メタデータ：2026-06-02
<a id="closure-architecture-v2"></a>

# クロージャアーキテクチャ v2

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](closure-architecture-v2.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](closure-architecture-v2.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](closure-architecture-v2.zh-CN.md)

> 原文コミット固定の非規範閲覧用スナップショット。原文を正本とし、自動同期しない。独立した実行、実装検証、証明検査、測定は行っていない。歴史的コマンドや設計指示は資料であり、実行した指示ではない。
> [Original source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/done/closure-architecture-v2.md)

> 原文上の注意：これは完了した歴史的な設計・実装ログであり、現行コンパイラの監査でも branch 再開指示でもない。初期要件と後段の訂正は併存する。local lambda の SCC 再帰対応は不要となり、auto-? の失敗は Perceus でなく frontend の呼出解決と診断された。初期の廃止目標に対して最終的には clone predicate が残り、全件 lift の理想に対して mutable capture は raw Lambda を維持する。P0–P6 完了は別記の型付け・構文解析・格納の欠落まで解消した意味ではない。Perceus から binary への証明の鎖を閉じ、出力コード全体を証明したという原文の表現は歴史的主張として保持する。モデルのカーネル検査済み RC 定理、手作業の実装対応、有限の複数ターゲットテスト、任意の実行物の正しさは別物である。runtime allowlist 注釈の信頼境界と、後段の user HOF 推論による限定も保持する。テスト件数や日付は独立に再実行・更新していない。

> **完了した設計の記録。** P0–P6 は完了している。最終実装ブランチは 2026-06-02 に [#332](https://github.com/almide/almide/pull/332) でマージされた。2026-10-31 に `done/` へ移動した。以下の問題の説明、フェーズ表、コマンド、テスト件数、証明に関する主張は、当時の実装を記録したものであり、現在のコンパイラの挙動や再開すべき作業を示すものではない。

> **1 つの識別子、1 つのキャプチャ集合、リフティングは低水準化。** クロージャは、プログラム全体で一意の識別子と、一度だけ計算される明示的なキャプチャ集合を持つ、単一の正規値である。*インライン化*はエスケープの証明に基づく別の書き換えであり、表現上の近道では決してない。これは `Verified` / `Canonical` が出力を制御する方式のクロージャ版である。推測ではなく、構築による正しさ。

<a id="original-problem-historical"></a>

## 当初の問題（歴史的記録）

設計時点では、1 つのソースラムダ `(x) => f(x)` が、使用箇所の構文、キャプチャ解析、可変性、ターゲットに応じて最大 **5 通り**に表現されていた:

1. インラインのイテレーターアダプターコールバック（`IterStep`/`IterCollector`、Rust）。
2. 残存する生の `IrExprKind::Lambda` 値。
3. リフトされた `__closure_N` 関数 + `ClosureCreate` + `EnvLoad` の 3 点セット（WASM、不変キャプチャ）。
4. ヒープセルによる可変キャプチャを持つ生の `Lambda`（WASM）。
5. η 展開されたラッパー / `FnRef`（名前付き関数）。

キャプチャは `Lambda` ノードに保存されて**いない**。**3 つ**の独立した自由変数解析（クロージャ変換の `FreeVarCollector`、`emit_wasm` の `ClosureScanner`、Rust の暗黙的な `move`）によって再導出され、ほぼ重複した **2 つ**の WASM 環境ビルダー（`emit_lambda_closure` と `emit_closure_create`）に渡される。`Ty::Fn { params, ret }` が唯一の呼び出し可能型であり、クロージャ／キャプチャ／呼び出し規約のタグを持たない。そのため、すべての操作上の違いは、*どのノード種別*と*どのパイプライン分岐*が選ばれたかに暗黙的に埋め込まれている。

概念的一貫性の欠陥: **正規の「関数値」が存在しない。** 表現はラムダの形から*推測*される（`free.is_empty()` ⟹ 「これは HOF 呼び出し箇所でインライン化される」）。だが、その特性は実際にはこのパスからは見えない。

<a id="the-verified-bug-this-produces"></a>

### これによって生じる検証済みのバグ

`ClosureConversionPass` はキャプチャのないラムダを生のまま残す（`pass_closure_conversion.rs` の `free.is_empty()` 分岐）。出力時には、生のラムダを事前走査済みの `LambdaInfo` と `lambda_id` で対応付ける。しかし `lambda_id` は**モジュールごとに 0 にリセット**され（`LowerCtx::new`）、モジュールのラムダは**一度も登録されない**（WASM の事前走査は `program.functions` しかたどらない）。結果として、0.23.14 の 8 行・2 モジュールのプログラムで再現した:

```almide
// src/lib.almd
pub fn neg() -> (Int) -> Int = (n) => 0 - n
// main.almd
import self.lib
fn apply(f: (Int) -> Int, x: Int) -> Int = f(x)
fn add() -> (Int) -> Int = (n) => n + 1000
effect fn main() -> Unit = {
  println(int.to_string(apply(add(), 5)))     // 1005  (correct on both)
  println(int.to_string(apply(lib.neg(), 5))) // want -5; WASM printed 1005
}
```

`lib.neg()(5)` が WASM で `1005`（main の `add` 本体）を返す — 無言の誤った結果。main プログラムにラムダがない派生版では、不正な WASM（`unknown table 0: table index out of bounds`）を出力する。ネイティブでは正しい。Rust ターゲットはネイティブクロージャを使い、WASM の ID 照合経路を実行しないためである。

<a id="the-principle-separate-three-conflated-concerns"></a>

## 原則: 混同された 3 つの関心事を分離する

| 関心事 | 現状（破綻） | v2 |
|---|---|---|
| **識別** — *どの*クロージャか | モジュールごとの `lambda_id`。モジュール間で衝突する | プログラム全体で一意の `ClosureId` |
| **キャプチャ集合** — *何を*取り込むか | 3 つの解析が再導出し、食い違い得る | 1 つの共有解析をノードに付加し、明示的な `mode` + 正確な `Ty` を持つ |
| **表現** — *どのように*配置し呼び出すか | キャプチャが空かどうかから推測 → 5 つの形 | `Lambda` 値ノードが共有パイプライン全体で正規形となる。*リフティング*はターゲットの低水準化。*インライン化*は、リフティングが不要であることの証明 |

発想の逆転: **現状ではラムダの形から表現を推測する。v2 では使用方法から証明する**（エスケープ解析が呼び出し箇所ごとに判断する）。一方、識別子とキャプチャは一度だけ計算し、すべてのターゲットで共有する。

<a id="the-boundary-shared-semantic-core-vs-target-lowering"></a>

## 境界: 共有の意味論的コアとターゲット低水準化

この再設計で最も重要な判断（統一的な*リフト済み* `Closure{code,env}` ノードは抽象度が適切でない — これは WASM の低水準化形である。Rust に押し通すと、ネイティブクロージャを逆向きに再構築する必要が生じ、`rustc` を活かせなくなる。WGSL ではそもそも表現できない）:

**共有部分（`almide-ir` 内で、パイプラインが分岐する前に関数*と*モジュール全体について一度だけ計算）:**

- `ClosureId(u32)` — プログラム全体で一意の識別子。`lambda_id: Option<u32>` を置き換える。
- ノードに付加するキャプチャ集合:
  `Lambda { params, body, id: ClosureId, captures: Vec<Capture>, escape: EscapeVerdict }`、
  `Capture { var, ty: Ty /* Almide owned type */, mode: ByVal | ByMutCell }`。
  キャプチャなし ⟹ `captures: []`。`free.is_empty()` による迂回は設けない。
- `ClosureId` をキーとするプログラム全体の `ClosureTable`（関数 + モジュール）。
- 1 つの共有自由変数 + エスケープ解析が上記をすべて埋める。

`Lambda`（id + captures + escape を保持）は、**共有パイプライン全体を通じて正規のクロージャ値の形を維持する。**共有のリフト済み `Closure{code,env}` ノードは存在しない。

**ターゲット低水準化（それぞれ共有キャプチャ集合 + エスケープ判定を読む）:**

- **Rust** — リフトしない。ネイティブの `move |params| body` を出力し、`captures` から決定的に `CaptureClonePass` を駆動する（`needs_clone_type` 許可リストを削除）。`ParamBorrow` の実体化（`to_vec`/`to_owned`）は Rust 表現の導出である。エスケープにより必要となる格納形 → `Rc<dyn Fn>`。`rustc` に最適化を任せる。
- **WASM** — WASM 専用の `ClosureLowering` パスが **`InlineClosures` の後に残ったクロージャのみ**（エスケープするもの）を `__closure_N` + `Ty::EnvPtr` 環境 + `EnvLoad` にリフトし、`ClosureTable` → 関数テーブルに**名前で**登録する（順序に依存せず、モジュールに対して安全）。環境ビルダーは 1 つ。`valtype_and_load(ty, mode)` ヘルパーを 1 つ設け、ValType 選択、可変セルの i32 扱い、狭い整数の符号拡張を集約する。`call_indirect` の型インデックスは `ClosureId` 経由でリフト済み関数に登録された `closure_type_idx` から得る — **唯一の正となる情報源**。
- **WGSL** — *クロージャなし*のターゲット。`@gpu` 本体内では `InlineClosures` を無条件に実行し、残ったエスケープクロージャはすべてコンパイルエラーにする。（これにより逆側から主張を検証できる。WGSL では「インライン化かエラーか」が任意ではなく必須となる。）

この設計の利点: **共有部分 = 識別子 + キャプチャ集合 + エスケープ判定。リフティング、符号拡張、`to_vec`/`to_owned`、`Rc<dyn Fn>`、`call_indirect`、「インライン化かエラーか」は、すべてその単一の共有情報からターゲットごとに導出される。**どちらのバックエンドにも無理を強いない。

<a id="inlineclosures-prove-the-use"></a>

## InlineClosures: 「使用方法を証明する」

リフティングの**前**に走る認識パス。**両側**の証明が成り立つ場合に限り、クロージャをインライン化できる:

- **(a) 使用箇所**: クロージャが*匿名*（どの `Bind` の右辺でもない）で、*単一使用*であり、スコープからエスケープしないこと。**かつ**
- **(b) 受け手**: 呼び出し先が、stdlib の `.almd` ソースに `@fn_arg_consumed(idx)` を持つ*消費のみ*のコンビネーターの精選許可リスト上にあること（stdlib/defs の TOML パイプラインは廃止済み）。ランタイム実装に対して一度検証されるため、クロージャは呼び出し*経由*でもエスケープしない（そうしなければ、reducer が関数をアキュムレーターに取り込む `fold` ではエスケープしてしまう）。

両方が成り立つとき、`Call(combinator, [.., literal-closure])` を `InlinedHOF` ノードへ書き換える（WASM: 本体を埋め込み、確保なし / `call_indirect` なし。Rust: `IterChain` アダプター）。Egg 融合は `InlinedHOF` を合成する。**束縛されたクロージャは決してインライン化しない** ⟹ 「一度だけ計算する」が文字どおり成立する。WASM エミッターの高速経路は生の `Lambda` の形ではなく、`InlinedHOF` をキーとする。

<a id="the-hard-correctness-pieces-designed-in-not-glossed"></a>

## 正しさの難所（曖昧にせず設計に組み込む）

- **再帰。** 自己／相互再帰する*ローカル*クロージャは、自分自身を自身の環境へキャプチャできない（循環／未確保の値）。前処理パスが `let` 束縛ラムダ群の SCC を計算する。自己／兄弟への参照を `ClosureId` のコードシンボルへ書き換え、リフト済み関数自身の `__env` パラメーターを再帰呼び出しへ渡す（標準的な「再帰クロージャは自身の環境を渡す」方式）。事後条件: 構築中のクロージャをキャプチャする `env` がないこと。
- **可変キャプチャは表現のバグではなく、*生存期間*のバグ。** `ByMutCell` は必要だが、それだけでは不十分（現在のコードは、キャプチャ対象のローカル変数が `var_map` の外にあると、黙って環境に `0` を格納する）。修正: 変数の*束縛*箇所に `CellAlloc` ノードを置く。その変数の**すべての**読み書き — リフト済み本体だけでなく、外側の関数も — を `CellRead`/`CellWrite` に向け直す。セル（`Ty::Cell(T)`、1 スロットのヒープレコード）は RC 追跡され、Perceus がクロージャのエスケープ後も生存させる。ゼロ格納のフォールバックを削除し、事後条件エラーにする。
- **環境を通した Perceus RC。** 現状では、`ClosureCreate` が `VDecl` の値である場合にしかキャプチャの inc が発生しない。そのため呼び出し引数や末尾として作られたクロージャ（まさに検証済みバグの形）には inc が**なく**、過少カウントになる。修正: inc/dec を、*すべての*構文位置（宣言値、呼び出し引数、末尾、リスト要素）で **Closure ノード**に結び付ける。値の実体化箇所で各ヒープ環境キャプチャを inc し、Closure 自身の drop で各々を dec する。`Ty::EnvPtr` はヒープで**あり**、所有ではなく借用。EnvLoad で束縛されたキャプチャは構造的借用（inc/dec なし）。これにより L5 Lean 証明に明確な対象を与える。両端が 1 つのノードに結び付くため、すべての経路で `inc_count(cap) == dec_count(cap)` が成立し、**「Perceus → バイナリ」の証明の連鎖が閉じる。**
- **型の同一性、唯一の正となる情報源。** `Ty::EnvPtr` は*実体のある封印されたスカラーバリアント*（`Ty::String` のエイリアスではない。そうすると Perceus が環境ブロックを `RcDec` し、ヒープを破壊する）。現状は `ty_to_valtype = I32` で、将来の GC バックエンド（`(ref $env)` + `call_ref`）に備えてターゲットでパラメーター化する。`call_indirect` の型は `ClosureId` 経由の `closure_type_idx` から得て、箇所ごとに `callee.ty` から再導出しない（構造的な `register_type` の重複排除には、潜在的な 2 つ目の衝突がある — main にラムダがない「table OOB」版）。**`Ty::Fn` は 1 つに保つ** — 呼び出し可能型を二分すると、単一化とすべての `Type[Fn]` コンテナーに問題が波及する。規約はノードに持たせ、「変換後はすべての呼び出し可能値が Closure であり、各箇所は常にクロージャ ABI を出力し、`.ty` を探らない」ことで強制する。

<a id="phasing-every-boundary-perf-neutral-and-invariant-satisfiable"></a>

## フェーズ分割（各境界で性能中立かつ不変条件を充足可能）

レッドチームの規則: **完全性は、その依存先と不可分に導入する。** Egg 融合（変換前に走り、生のラムダを再出力）と WASM のインライン高速経路（`is_inline_lambda` は生の `Lambda` に一致）がまだ生のラムダを必要とする間は、「変換後に生の Lambda がゼロ」は*充足不能*。これを複数リリースに分割してはならない。

| フェーズ | 内容 | 特性 |
|---|---|---|
| **P0** | プログラム全体で一意のクロージャ識別子（`GlobalizeClosureIdsPass`）+ WASM 走査へのモジュールラムダ登録 | **検証済みバグを修正、性能中立**（高速経路は変更なし）。8 行の再現例を native==wasm リグレッションテストにする |
| **P1** | 共有コア: `ClosureId` + 関数とモジュール全体にわたる単一の自由変数／エスケープ解析。ノードに `captures`/`escape` を付加。`ClosureTable` | 表現は不変。Rust/WASM が共有集合を*読む*。旧解析との一致をアサート |
| **P2** | `InlineClosures`（`InlinedHOF`）+ **不可分な完全性**: Egg を識別子付きクロージャへ移行。WASM の 3 つの高速経路を `InlinedHOF` に基づくよう書き換える。残存するものすべてに WASM `ClosureLowering` を実施。`lambda_id`/`param_id`/カウンターの解決器と環境ビルダー 1 つを削除。`Ty::EnvPtr`。`emit_load_at` 経由の `EnvLoad`。`ClosureId` 経由の `closure_type_idx` | 「生の Lambda ゼロ」がようやく充足可能。`unreachable` は事後条件エラーになる |
| **P3** | `ByMutCell`（`CellAlloc`/`CellRead`/`CellWrite`）+ SCC 再帰 + 環境を通した Perceus RC | 3 つの正しさの阻害要因を `PerceusVerify` で検証 |
| **P4** | Rust が共有キャプチャ集合を完全に読む。`needs_clone_type` + 3 番目の解析（暗黙的 move）を廃止 | 1 つの解析、両ターゲット |
| **P5** | Lean: 環境について `inc == dec` を証明検査 | 証明の連鎖が閉じる |

<a id="p3-ground-truth-measured-on-develop-post-p2ba"></a>

## P3 の実態（P2b/A 後の develop で測定）

レッドチームの 3 つの「阻害要因」を実際の言語に対して挙動検証した結果、**捉え方を修正した** — 現存するバグは 1 つだけ:

- **再帰は論点にならない。** 自己参照する*ローカル*ラムダ（`let fact = (n) => ... fact(n-1)`）はチェッカーに拒否される（`E002 undefined function 'fact'`）。再帰にはトップレベル `fn` を使う（名前付き `FnRef` であり、クロージャ値ではない）。修正すべきものはなく、SCC パスは不要。
- **不変キャプチャはすでに動作する。** ヒープのものも含む。`let prefix = "hi-"; (n) => prefix + n` は**両方**のターゲットで正しい RC とともに正しい答えを返す。「環境を通した RC」の懸念は、実際には不変の場合にすでに処理されている。
- **可変キャプチャだけが実際のバグ**であり、深刻である:
  - **Rust ターゲット — 無言の誤った結果。** `var total = 0; let add = (x) => { total = total + x }; add(5); add(10); total` が **`0`** を返す（`15` であるべき）。エミッターは `let add = move |x| { total = total + x }` を生成する。`move` クロージャは `total` の**コピー**をキャプチャする（`Copy` なので）。そのため外側の `total` は変化しない。（*無言の*誤答 — 記述の正確さを重視する言語にとって最悪の種類。）*エスケープする*可変クロージャ（`make_counter`）はコンパイル自体が失敗する（`rustc E0525`、FnMut を Fn として扱う）。
  - **WASM ターゲット — エスケープ時にトラップ。** ヒープセル機構は*エスケープしない*可変キャプチャでは正しく動く（`15` を返す）が、フレームから**エスケープする**可変クロージャ（`make_counter`）は**実行時にトラップする**。

<a id="the-fix-feature-sized-not-a-patch"></a>

### 修正（パッチではなく機能規模）

変更され、かつキャプチャされる変数は、move／clone されたコピーではなく、**共有可変状態**でなければならない。これは WASM のセル機構がすでに実装している結論と同じであり、それを Rust に適用し、エスケープについて修正する:

- **Rust**: キャプチャされ、かつ変更される `Mutability::Var` の変数を `Rc<Cell<T>>`（`Copy`）/ `Rc<RefCell<T>>`（`!Copy`）へ低水準化する。宣言は `Rc::new(Cell::new(init))`、各読み取りは `→ .get()`、各書き込みは `→ .set(v)`、クロージャは `Rc::clone` をキャプチャする（セルを共有）。検出集合（`shared_mut_vars`）を walker（Bind/Var/Assign）と `CaptureClonePass` に渡す必要がある（値ではなく `Rc` を clone する）。
- **WASM**: ヒープセルは維持するが、エスケープを修正する。クロージャ構築箇所でセル変数が直接のローカル変数でない場合でも、キャプチャしたセルポインターを環境に格納しなければならない（`var_map` に見つからず `0` を格納してしまう箇所）。

これが「ByMutCell」の本当の内容である。両バックエンドを貫く新しい共有セル表現であり、1 行の変更ではない。下の行にある SCC 再帰と広範な環境を通した RC 作業は*不要*（再帰は論点にならず、不変 RC はすでに動く）。

**アーキテクチャ上の複雑さ（実装中に判明）。** Rust の修正は walker のコード生成だけではない。パイプラインの順序変更が必要である。キャプチャ分類（どの変数が変更され、かつキャプチャされるか → `shared_mut`）は、現在、*最後*の段階である walker のアノテーションフェーズにある。しかし（はるかに早い段階の）`CaptureClonePass` が、キャプチャするラムダを `{ let __cap = v.clone(); move |..| ..__cap.. }` で包む役割を持ち、`Copy` のキャプチャを*スキップ*する（`Int` の `move` には事前 clone が不要）。`Int` のカウンターが `Rc<Cell<i64>>` になると、**もはや `Copy` ではない**ため、その clone ラップが*必要になる*。さもなければ `move` が唯一の `Rc` を消費し、クロージャの後で変数を使えない。したがって `CaptureClonePass` は `shared_mut` キャプチャを包む必要があるが、それを識別する分類より*前*に走る — 循環依存である。これをきれいに解消するには、**変更され、かつキャプチャされる集合を早期（`CaptureClonePass` より前）に一度だけ計算**し、`CaptureClonePass`（セルをラップ + `Rc::clone` し、`__cap` も `shared_mut` とマーク）と walker（Bind → `Rc::new(Cell::new())`、読み取り → `.get()`、書き込み → `.set()`、所有文脈 → `.get()`）の両方で使う。この単一の共有解析が正しい構造である（P1 の「1 つの解析、両消費者」に対応する）。また、これこそがセッション末尾のパッチではなく、集中して取り組む機能である理由でもある。一部にしか情報を通さない実装はコンパイルされても、ある文脈では無言で誤動作する。まさに今修正しようとしている障害形態である。

基礎作業は `closure-v2-p3` ブランチに入った（未マージ）。`shared_mut_vars` のアノテーション + ヘルパーと、キャプチャ分類器の Copy／非 Copy 分割。残る walker コード生成 + `CaptureClonePass` の順序変更で完成する。

<a id="honest-limits"></a>

## 正直な限界

- **P0 は対象を絞った修正であり、再設計ではない。** `lambda_id` をプログラム全体で一意にし、モジュールラムダを登録する。`free.is_empty()` による表現上の迂回と、並行する生 Lambda の出力経路は P2 まで残る。可変キャプチャの生存期間（P3）や重複した解析（P1/P4）は改善しない。
- **エスケープ許可リスト（`@fn_arg_consumed`）は手作業で選定され、各ランタイム実装に対して一度検証される** — 推論された特性ではなく、保守義務である。誤ったアノテーションはエスケープするクロージャをインライン化し得る。信頼基盤として扱うこと。*ユーザー定義の高階関数については #2288 により置き換えられた*: 関数型パラメーターのエスケープは、`&T` を決めるのと同じ不動点で、その出現箇所から推論する（`fn_param_borrow`、docs/specs/codegen.md「呼び出し可能値がエスケープしない限り関数型パラメーターは借用される」）。エスケープしないスロットは `&dyn Fn` となり、証明書検査器の C5 が判定を検査する。許可リストが現在記述するのはランタイム側の対応実装自身のスロットのみ（#2289 ではそれらもアブレーションへ移す）。
- **GC への移行は局所的だが、無償ではない**: `Ty::EnvPtr` と Closure のレイアウトはターゲットでパラメーター化されるため、`gc` バックエンドは `emit_wasm` + `ty_to_valtype` のみを差し替える。しかし `call_ref`/型付き funcref には、別途設計すべき独自の型同一性モデルがある。
- **L3 決定性との相互作用**: `GlobalizeClosureIdsPass` は `CanonicalizePass` の前に走り、変更するのは `lambda_id` のみ（関数順序ではない）。そのため `Canonical` 証明書に影響しない。クロージャテーブルのスロットはこれらの ID と独立に走査順で割り当てられるので、出力バイト列のホスト間決定性は維持される。

<a id="implementation-progress-log-historical"></a>

## 実装進捗ログ（歴史的記録）

<a id="done--in-flight"></a>

### 完了／進行中

| フェーズ | 内容 | 状態 |
|---|---|---|
| P0 | プログラム全体で一意のクロージャ ID + モジュールラムダ登録 | **マージ済み**（`#329`） |
| P1 | `almide-ir::free_vars` 内の単一の共有自由変数解析 | **マージ済み**（`#330`） |
| P2b/A | 値ラムダ → `ClosureCreate`。`map/filter/fold` のインライン引数だけを生のまま残す。キャプチャするコンビネーターラムダもインライン化 | **マージ済み**（`#331`） |
| P3（Rust） | 変更され、かつキャプチャされる Copy ローカル変数 → 共有 `Rc<Cell<T>>`（`15` の代わりに黙って `0` になる問題を修正） | **PR `#332`**、ブランチ `closure-v2-p3`、検証済み（cargo test + spec 240/240） |
| P3（WASM） | auto-`?` のクロージャ束縛を名前で呼び出す → `lower_call_target` の誤解決（wasm でトラップ） | **修正済み**、ブランチ `closure-v2-p3`、検証済み（spec 240/240 + cargo test + リグレッション 2 件） |
| P4 | Rust 独自の 2 つの自由変数走査（`pass_capture_clone`、walker の `CaptureCollector`）→ 共有 `almide_ir::free_vars` | **完了**（`7bdc31f3`）、ブランチ `closure-v2-p3`、検証済み（spec 240/240 + cargo test）。−218 LOC |
| P5 | Lean — 環境 RC 契約（キャプチャについて `inc == dec`）を証明検査し、クロージャを無リーク証明に組み込む | **完了**（`f6c55727`）、`AlmidePerceusBelt/ClosureRc.lean`、`lake build` 成功、`sorry` なし |
| P6 | 非 Copy の可変キャプチャ → 共有セル（`SharedMut`/ヒープセル）により、クロージャの変更を伝播 — 以前は両ターゲットで無言の誤動作 | **完了**（`f680b807`）、ブランチ `closure-v2-p3`、検証済み（spec 240/240 + cargo test + クロスターゲットのリグレッション 2 件） |

P3 はブランチ `closure-v2-p3` 上にある（Rust: `22a7d87a` の基礎作業 + `232a4fac` の修正。WASM: 以下の呼び出し先修正）。実装では、メインのチェックアウトにある無関係な未コミット変更を避けるため、`origin/develop`（または `origin/closure-v2-p3`）から新しい worktree を使った。これは歴史的な作業手順の注記であり、再開の指示ではない。

<a id="fixed--p3-wasm-auto--closure-binding-called-by-name-was-drops-the-block"></a>

### 修正済み — P3（WASM）: auto-`?` のクロージャ束縛を名前で呼び出す（以前: 「ブロックを drop する」）

クロージャを返す `effect fn` は、束縛に auto-`?` を使うと WASM でトラップした:

```almide
effect fn make_adder_e(n: Int) -> (Int) -> Int = (x) => x + n
effect fn main() -> Unit = {
  let add5 = make_adder_e(5)        // auto-`?` → TRAPPED on wasm
  // let add5 = make_adder_e(5)!    // explicit `!` → always WORKED (15)
  println(int.to_string(add5(10)))
}
```

**真の根本原因（フロントエンドであり、Perceus や `Try` の出力では*ない* — 本文書の以前の仮説は誤りだった）。** `lower_call_target`（`crates/almide-frontend/src/lower/calls_target.rs`）は、`add5` の **`var_table` スナップショットの型**を読み、`matches!(.., Ty::Fn)` を調べて、`add5(10)` を `Named` と `Computed` のどちらにするか決めていた。effect fn では effect-`?` の unwrap 書き換え（`auto_try`）が低水準化の*後*に走るため、呼び出し箇所で `add5` の保存済み型はまだ `Result[(Int)->Int, String]` のままで、`Ty::Fn` ではない。検査が失敗し、呼び出しは存在しない関数 `Named { add5 }` にフォールスルーした。

この 1 つの誤解決が、それまで誤った原因に帰していた*両方*の症状を生んだ:
- **WASM が `unreachable` を出力**: `Named { add5 }` 呼び出しには対象がない → 未解決呼び出しスタブ。
- **Perceus の早すぎる `rc_dec`**: `Named` 呼び出しは変数参照ではないため、`add5` は束縛後に未使用に見えた。Perceus は*正しく*それを解放した。Perceus は一度も間違っていなかった — 変数に言及しない呼び出しを与えられていた。（`?` と `!` の形式のパスごとの IR ダンプを比較して確認。`StackBalance` まで同一で、差は `lower_call_target` の出力 — `Named{add5}` と `Computed{var2}` — のみにあり、最初のダンプから、つまりフロントエンドから存在した。）

**修正。** `lower_call_target` は現在、**呼び出し先の使用箇所の型**（`ctx.expr_ty(callee)` — チェッカーがすでに auto-`?` で unwrap し、関数型にしている）から呼び出し可能性を判断し、保存済み型とそこから `Result` を除いた形をフォールバックとする。束縛に解決されるローカル名は、常に*変数経由*で呼び出す（`Computed`）。その後、出力処理がクロージャをディスパッチし、Perceus は使用を認識して、その後に解放する。Perceus、`Try` の出力、コード生成パスには変更なし。`var_table` の型は依然として遅れているが、使用箇所の型が正であり、それを迂回する。値型の auto-`?` 束縛も同じ理由ですでに動いていた（その出力は、`auto_try` が修正する `Bind.ty` を読む）。

検証済み: `?` と `!` の再現例 → wasm + ネイティブで `15`。修正後の `main` IR は `!` 形式と一致（末尾リフトされた `__perceus_ret`、使用後の `rc_dec`）。境界ケース（2 つのクロージャ、1 式内の複数呼び出し、HOF に渡すクロージャ）もターゲット間で一致。spec 240/240。完全な `cargo test`。`tests/wasm_runtime_test_parts/p2.rs` に新しいリグレッション 2 件（`wasm_effect_fn_returns_closure_auto_try_binding`、`_used_twice`）。

<a id="done--p4-one-free-var-analysis-for-both-targets"></a>

### 完了 — P4: 両ターゲットに 1 つの自由変数解析

キャプチャ集合は現在、すべての消費者に対して、正確に 1 つの関数 `almide_ir::free_vars::free_vars` で計算される。P4 より前には、Rust ターゲットに、これと微妙に異なる **2 つ**の独自再実装があった:
- `pass_capture_clone::collect_free_vars`（`__cap` の clone ラップに供給）。
- walker の `CaptureCollector`（ラムダ深度／ローカル変数スタックの走査。`RcCow` の格納分類に供給）。

両方を削除し、両呼び出し箇所が現在は `free_vars` を使う。共有解析は厳密により正確である。*すべての*束縛子（分解を含むブロック `let`、match 分岐パターン、for-in 変数、ネストしたラムダ）を追跡し、`walk_expr` により新しい IR ノードに対してデフォルトで安全になる。手書きの走査では一部を黙って落としていた。また VarId 順の集合を返すため、`__cap` 束縛は今では決定的に出力される。差し引き −218 LOC。

ターゲット固有のまま残るのは、その 1 つの集合の*射影*であり、重複ではなく正当なもの。Rust は clone すべきキャプチャに絞り（`needs_clone_type` は `Copy` を除外し、`SHARED_MUT` は `Rc<Cell>` のものを強制的に含める）、`RcCow` を分類する。WASM はすべてのキャプチャを環境に格納する。「`needs_clone_type` を廃止する」は、それに入力していた重複*走査*を廃止することとして実現された。述語自体は 1 行の Rust の clone 方針であり、解析ではない。検証済み: spec 240/240、完全な `cargo test`、難しいケース（可変 `Copy` キャプチャ → 共有セル、2 つのクロージャがキャプチャする変数、さらに外側の変数をキャプチャするネストしたクロージャ）で native==wasm。

<a id="done--p5-the-env-rc-contract-certified-in-lean"></a>

### 完了 — P5: Lean で証明検査された環境 RC 契約

`AlmidePerceusBelt/ClosureRc.lean`（`lake build` によりカーネル検証済み、`sorry`/`axiom` なし）。コンパイラはクロージャのライフサイクルを通常の `RcInc`/`RcDec` IR ノードに低水準化する（`PerceusPass` + `pass_perceus.rs` の規則 6）。作成時にはキャプチャした各ヒープ変数を inc してクロージャオブジェクトを確保し、drop 時にはオブジェクトを dec してから各キャプチャを dec する。`closureScope` は既存の `inc`/`dec`/`vdecl` の `FnBody` ノードのみで、まさにその低水準化をモデル化する。公理的なものはなく、契約は仮定ではなく*導出*される:

- `closure_env_rc_balanced` — キャプチャ集合について、クロージャは各変数を同じ回数 inc と dec する（`inc == dec`）。各キャプチャの参照カウントへの純寄与は 0。
- `closure_preserves_isFreed` — クロージャは借用した各キャプチャの解放収支を維持する（キャプチャ値のリークも二重解放もない）。
- `closure_obj_freed` — クロージャオブジェクト自体が解放される（drop 時の `dec cv`）。
- `closureScope_allHeapFreed` — 継続部分が `allHeapFreed` を満たすならクロージャも満たし、通常のヒープ変数と同じエンドツーエンドの無リーク定理（`perceus_all_heap_freed`）に組み込まれる。

（公理的な Closure コンストラクターを追加するのではなく）*実際の* inc/dec 低水準化をモデル化する方が強い結果になる。コンパイラが実際に出力するコードの RC が正しいことを証明するためである。これにより、以前は証明の外にあった唯一の経路、クロージャの Perceus→バイナリの証明の連鎖が閉じる。

<a id="done--p6-non-copy-mutable-captures-the-silent-wrong-gap-closed"></a>

### 完了 — P6: 非 Copy の可変キャプチャ（無言の誤動作の穴を解消）

完全性をレッドチーム検証する中で発見: `var acc: List = []; let f = () => list.push(acc, 1);
f(); f(); list.len(acc)` が両方のターゲットで **0** を返していた。クロージャの変更が失われていた。P3 は Copy スカラー（`Rc<Cell>` / ヒープセル）しかカバーしていなかった。非 Copy キャプチャは Rust の `RcCow`（copy-on-write — 共有された書き込みでは clone され、変更が失われる）を通り、WASM はリストを値でキャプチャしていた。修正は、キャプチャされ、かつ変更される非 Copy 変数を*共有*セルにする（Rust では `SharedMut<T>` = `Rc<RefCell<T>>`、WASM では既存のヒープセル）:

- **検出**は IR の `Mutability` フラグではなく、*クロージャを通した変更*に基づく。メソッド（`list.push`）経由でのみ変更される `var` は `Let` と記録される（再代入はされない）。Rust の `CaptureClonePass` と WASM の `ClosureConversionPass` は、どちらもキャプチャ変数への代入 / `&mut` 借用 / インプレース変更操作（`list.push` など）を走査する。
- **Rust**: `SharedMut` は `Cell` の `.get()/.set()` に倣うため、読み取り／代入の低水準化は変わらない。コンストラクター（`SharedMut::new`）とインプレース変更（`borrow_mut()`）だけが型で分岐する。これらの変数について RcCow を廃止。共有読み取りは所有されたスナップショットを借用する（`&x.get()`）ので、セルがブロックローカルである末尾位置でも安全。
- **WASM**: 変更されるキャプチャは、セルを失う環境を持つ `ClosureCreate` にリフトするのではなく、*生の*ラムダ（ヒープセル経路）のまま残さなければならない。`shared_mut_vars`（本体がまだキャプチャ名を参照している変換前に計算）が `mutable_captures` を初期登録し、セルが環境を通過し、`list.push` の再確保が新しいポインターをセルへ書き戻すようにする。

native==wasm を検証済み: キャプチャ経由の変更（`2`）、ネストしたクロージャ（`3`）、Copy は引き続き動作（`20`）、読み取り専用キャプチャは不変（`40`/`104`）。spec 240/240。完全な `cargo test`。新しいクロスターゲットのリグレッション 2 件。Lean の P5 証明は変更なしでこれらのセルもすでにカバーする。共有セルは通常のキャプチャされたヒープオブジェクト（キャプチャ時に `Rc::clone`、drop 時に `dec`）なので、`closure_env_rc_balanced` がその `inc == dec` を証明する。内容の変更は RC と直交する。

<a id="status-closure-architecture-v2-complete"></a>

### 状態: クロージャアーキテクチャ v2 は完了

P0–P6 はすべてブランチ `closure-v2-p3` に入った（P0/P1/P2b は #329/#330/#331 でマージ。P3-Rust、P3-WASM、P4、P5、P6 はその後のブランチ上のコミット）。残るフェーズはない。クロージャは正しく（全型の可変キャプチャ、両ターゲットを含む）、1 つのキャプチャ解析を使い、RC は Lean で証明されている。

キャプチャ*セル*のアーキテクチャは完了している。その後の敵対的なクロスターゲットの網羅検証により、クロージャの*型付け／解析／格納*方法に隣接する差異が見つかった（WASM の文字列変更操作、タプル内クロージャ、リスト添字呼び出し、型付きパラメーター、Map 内クロージャ、非 Copy 要素の IndexAssign）。これらはキャプチャセルと直交する。[クロージャのコード生成におけるターゲット間の未対応箇所](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/done/closure-codegen-cross-target-gaps.md) で別途追跡している（5 件は #334 で修正済み。深い 2 件は根本原因を特定し、未解決）。

保持したコードと診断の説明：最初の例のファイルは src/lib.almd と main.almd。1005 は両ターゲットで正しく、次の呼出しは -5 が期待値なのに WASM は 1005 を出した。診断 unknown table 0 は、table index が範囲外であることを示す。2 番目の例では自動 ? が wasm で trap し、コメントアウトした明示的 ! は常に成功して 15 となった。キャプチャ型の inline コメント Almide owned type は Almide の所有型を意味する。
