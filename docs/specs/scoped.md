# scoped — 宣言できるリクレイム境界

> Last updated: 2026-09-22

`scoped` は修飾子であり、**二箇所**に書ける。ブロック位置の `scoped { … }` は
記憶がいつ終わるかを宣言し、宣言位置の `scoped fn` は「領域の中で走れる」を
**シグネチャの一部**にする(本体を編集しても、モジュールを跨いでも、資格が
黙って失われない)。規範は [docs/specs/als/expressions.md](./als/expressions.md)
の ALS-E31、契約は C-362 / C-363 / C-364。

```almide
type Chain = Nil | Cons(Int, Chain)

scoped fn build(n: Int, acc: Chain) -> Chain =
  if n == 0 then acc else build(n - 1, Cons(n, acc))

scoped fn total(c: Chain, acc: Int) -> Int =
  match c {
    Nil => acc,
    Cons(h, t) => total(t, acc + h),
  }

fn sum_to(n: Int) -> Int = scoped { total(build(n, Nil), 0) }
```

テスト: `spec/wasm_cross/scoped_region_value.almd`(C-362)、
`spec/wasm_cross/scoped_worker_in_and_out.almd`(C-363)、
`spec/lang/scoped_test.almd`、`tests/scoped_region_test.rs`、
`tests/diagnostics/e08{6,7,8}-scope-*`(C-364)。

## 構文 — `scoped` は文脈キーワード

```ebnf
scoped_block := "scoped" block
fn_decl      := [visibility] ["scoped"] ["effect"] "fn" name …
```

`scoped` が修飾子として読まれるのは二つの形だけである。

- **式位置**: `scoped` の直後に**同じ行の** `{` が来たとき。
- **宣言位置**: 直後が `fn` または `effect` のとき。

それ以外の `scoped` は識別子のままである。`match` / `while` / `for … in` の
**ヘッド**では、直後の `{` が構文自身の本体を開くので `scoped` は常に識別子と
して読まれる — `match scoped { … }` は #1997 の前後で同じ意味を持つ。したがって
`scoped` という名前の変数・関数・モジュールを持つ既存プログラムは一つも壊れず、
方言エポックは上がらない(`proofs/dialect-epochs.toml` は 4 のまま)。ヘッドの
中で本当にスコープブロックを書きたいときは括弧で囲む:
`while (scoped { step(i) }) > 0 { … }`。

`almide fmt` は `[visibility] scoped [effect] fn` の順で往復する。

## What a scoped block promises

`scoped { body }` の**値は `body` の値**であり、`scoped` を消しても stdout・
stderr・終了コードは変わらない(C-362)。修飾子が宣言するのは**記憶の回収境界**
である: ブロックのために確保された記憶はその境界までに回収され、領域への参照は
境界を越えない。

実現はレグごとに異なる。

| レグ | 実現 |
|---|---|
| 構造 wasm | ブロック入口で `RegionSave`(バンプポインタと 16 本のサイズクラス頭をひとつの退避ブロックへ、頭はゼロ化)、出口で `RegionRestore` が巻き戻す。ブロック内で確保された全ブロックが一度に消える(`crates/almide-wasm/src/region.rs`) |
| native | 呼び出し点が `almide_region_window(\|\| …)` になり、`__rgn_` 双子(`Copy` ハンドル `AlmideRgn<T>` を持つ双子 enum)がプレリュードのアリーナで走る。双子化できない閉包では**同じ閉じ点**で所有権の解放が起こる(`crates/almide-codegen/src/pass_region_window.rs`) |

**主張しないこと**: 窓の中のピーク記憶量の上界、停止性、境界までの確保量の
上界。これらは別の契約とベンチマークのオラクルを要する。

## What a scoped fn promises

`scoped fn` は**領域の外では通常の関数**である。直接呼んでも、`scoped { … }`
の中から呼んでも、答えは同じで両ターゲットで同一である(C-363)。修飾子が課すのは
**本体に対する検査**だけ — つまり「この関数は領域の中で走れる」という約束を、
発見ではなく**義務**にする。

stage 1 の admitted fragment:

- **値**: スカラ(`Int` / `Float` / `Bool` / `Unit`)、同一ファイルに宣言された
  非ジェネリックなバリアント型(各ケースは unit か位置フィールド、フィールドは
  スカラか同条件のバリアント)、スカラ場だけのレコード、それらのタプルと
  `Option`。
- **ワーカ**: 通常のワーカ(木を辿る形を含む)と**直接末尾再帰**のワーカ。
- **呼べるもの**: 他の `scoped fn`、コンストラクタ、`int` / `float` / `math` /
  `bool` のスカラ stdlib 呼び出し、スカラ演算子、`if` / `match` / ブロック /
  ローカル束縛 / 範囲の `for` / `while`。

## What is refused, and why

断片の外は**検査時に**拒否される。検査はターゲット選択より前に走るので、
`almide check` と `almide check --target wasm` は同じ判定を出す(C-364)。
バックエンドが受理を決めることはない — 構造 wasm レグが検査と食い違ったら
E083(コンパイラ欠陥)であって wall ではなく、`scoped` を含むプログラムが
その レグから外れる経路に載ったときは境界なしで出荷せずに拒否する。

| コード | 何を拒否するか | 文書 |
|---|---|---|
| E086 | 領域の値が境界を越える形(ブロックの値、外側への書き戻し) | [E086](../diagnostics/E086.md) |
| E087 | 領域の中で認められない操作(非 `scoped` 呼び出し、グローバル、ホスト資源、未知の間接呼び出し、外側のヒープ捕捉、断片外の型、`!` / `guard` / `?`) | [E087](../diagnostics/E087.md) |
| E088 | 継続を保持する再帰形(単一の非末尾自己再帰、相互再帰) | [E088](../diagnostics/E088.md) |

E086 は `= allocated at <file:line:col>` と `= scope ends at <file:line:col>`
の二行を持ち、E087 は「何を保持しうるか」と「どの宣言が要求したか」を名乗り、
E088 は「呼び出しの後に何が残るか」を名乗る。

木を辿る形(一つのアームに二つ以上の自己呼び出し)は蓄積器に書き換えられない
ため、**通常のワーカとして受理される** — E088 が捕まえるのは蓄積器化で直せる
単一の非末尾再帰と、往復で活性化を保持する相互再帰である。

## stage 1 で未了なこと

- `scoped` はエントリプログラム限定。インポートされたモジュールの `scoped fn` /
  `scoped` ブロックは E087 で拒否される(native の領域パスはルートの関数と型
  宣言を双子化するため)。
- `String` と `T!` は断片の外。設計(#1997)は「detach したバイトから作った
  新しい String は出てよい」「`T!` は `Result[T, String]` のまま」を凍結して
  いるが、構造 wasm レグの境界での detach(巻き戻しの前に領域外へ複製する)は
  stage 1 に含めていない。両者とも E087 / E086 で拒否されるので、黙って通る形は
  ない。
- `scoped effect fn` は文法上受理され、検査で E087 になる。
