> 最終更新：2026-10-25

<a id="effect-system"></a>

# エフェクトシステム

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](effect-system.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](effect-system.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](effect-system.zh-CN.md)

> 日英混在原文の非規範的な固定読者用翻訳。固定原文を正本とし、自動同期、独立した実行検証・安全性監査を意味しない。過去・将来予定の fan 形式を区別したまま保持する。原文の差異も保持する。コールバック例はファイルシステム操作を IO と呼ぶ一方、葉の表では FS.read とする。§1/§2.4 と §8 では経路の検査範囲の説明が異なり、ネイティブ依存の列挙も package-system §6.4 と詳細が異なる。ファイルシステム制限とクロスターゲット一致は原文の主張であり、新たに検証した保証ではない。
> [Source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/effect-system.md)


Almide のエフェクトシステムは、純粋な計算と副作用を持つ操作の間に厳格な境界を設ける。コンパイラは関数単位でエフェクトコンテキストを追跡し、違反をコンパイル時に拒否する。

<a id="1-fn-vs-effect-fn"></a>

## 1. `fn` と `effect fn`

Almide のすべての関数は、**純粋**（`fn`）または**エフェクトを持つ**（`effect fn`）のいずれかである。`effect` 修飾子は関数シグネチャの一部で、呼び出しグラフを通じて伝播する。

```almide
import fs

fn add(a: Int, b: Int) -> Int = a + b

effect fn read_config(path: String) -> Result[String, String] =
  fs.read_text(path)
```

純粋な `fn` は I/O、並行処理、環境へのアクセスを行わないことを保証する。ただし例外が一つあり、§2.1 の六つの組み込みを通じて stdout/stderr に書き込むか、中断することはできる。`effect fn` はこれらすべてを行える。

検査器は `effect fn` の本体に入る際に `can_call_effect = true` を、通常の `fn` では `can_call_effect = false` を設定する。このフラグがエフェクトに関係するすべての操作を制御する。

テスト：`spec/lang/effect_fn_test.almd`

<a id="2-effect-isolation-e006"></a>

## 2. エフェクトの分離（E006）

純粋関数はエフェクト関数を呼べない。コンパイラはすべての呼び出し箇所で、呼び出される側の `is_effect` フラグを呼び出し側の `can_call_effect` コンテキストに照らして検査し、この規則を強制する。

```almide check-fail=E006
import http

effect fn fetch() -> Result[String, String] = http.get("https://example.com")

// Compile error E006: cannot call effect function 'fetch' from a pure function
fn bad() -> String = fetch()!
```

診断はエフェクト関数の宣言箇所を指す副次的な span を含む。

この規則はユーザー定義関数、stdlib のエフェクト関数（例：`fs.read_text`、`http.get`）、モジュールをまたぐエフェクト関数呼び出しに一様に適用される。

テスト：`spec/integration/modules/vis_effect_test.almd`

<a id="21-output-and-abort-builtins-are-admissible-in-a-pure-fn"></a>

### 2.1 出力と中断の組み込みは純粋 fn で許可される

*2026-09-27 追加（[ADR-0022](../../../../adr/0022-output-and-abort-builtins-are-admissible-in-a-pure-fn.md)）。検査器が以前から持つ振る舞いを明文化したもので、変更はない。*

純粋な `fn` から呼べる組み込みは六つ：**`println`、`eprintln`、`panic`、`assert`、`assert_eq`、`assert_ne`**。この集合は閉じている。それ以外の出力経路はすべて stdlib の effect fn であり、純粋 fn からは引き続き E006 となる。`io.print`、`io.eprint`、`io.write`（`io.stdout()` またはほかのハンドル）も含む。

1. **書き込みか中断を行い、読み取りは行わない。** `println` は stdout に、`eprintln` は stderr に追記する。`panic` と失敗した assert はプロセスを終了する（stderr ブロック、終了コード 1。assert 群の形式は C-153）。どれもプログラム外から情報を返さないため、純粋 fn の結果は依然として引数の関数である。
2. **呼び出し側をエフェクト付きにせず、伝播もしない。** `Unit`（`println`、`eprintln`、assert 群）または `Never`（`panic`）を返し、`Result` は返さない。`println(s)!` はすべてのコンテキストで E034 となる。出力する純粋 fn はモジュールインターフェースで純粋なシグネチャを保ち、effect fn も `!` なしの通常値として呼ぶ。
3. **サーブされるハンドラー（ADR-0020 §5.5 / §7.2）。** ハンドラーから呼ばれる純粋ヘルパーの出力にもハンドラー規則が適用される。各呼び出しのバイト列はストリームに連続して届き（一行はアトミック）、同じリクエスト内の行はプログラム順を保つ。同時進行中の異なるリクエスト間の行順は未規定である。
4. **`fan` の本体。** `fan` の腕から呼ばれる純粋 fn の出力は、その腕の一部となる。この例外は §5 の決定性契約を超える順序保証を追加しない。C-004 は `fan.map` のリスト・腕の順序を固定する（`??` チェーンは定義上逐次である）。`fan.settle` の各形式は §5 のとおり両ターゲットで逐次展開される。`fan` 自体には引き続きエフェクトコンテキストが必要（E007）。
5. **参照評価器。** プロセス内インタープリターと judge の参照評価器は、これら六つの名前をエフェクトコンテキストのない prelude 組み込みとして扱う。どこから呼ばれても同じように評価するため、三者オラクルは純粋 fn の出力を effect fn の出力と同じように比較する。純粋・エフェクトの区別を強制するのは検査器だけである。
6. **バッファリング（C-162、および C-367 のストリームに関する記述）。** 純粋 fn の出力は effect fn と同じストリームを使う。native の stdout は一つのバッファで、各行末にフラッシュする（stdout が端末なら各書き込みでもフラッシュ）。stderr は非バッファリング。クロスターゲット保証は**ストリームごと**で、stdout のバイト列、stderr のバイト列、終了コードが native と wasm でバイト一致する。両ストリームを一つのファイルへ向けた場合の交錯順は保証しない。中断時は、それより前に書かれた stdout をフラッシュする。
7. **`scoped` はさらに厳しい。** `scoped fn` 内の `println` は引き続き E087。`@bounded` fn は出力組み込みを許可する（C-316）。

```almide check
fn show(label: String, n: Int) -> Unit = println("${label}=${n}")

fn half(n: Int) -> Int = {
  assert_eq(n % 2, 0)
  eprintln("halving ${n}")
  n / 2
}

fn main() -> Unit = show("half", half(8))
```

同じ場所からの読み取りは引き続き E006：

```almide check-fail=E006
import fs

fn load(p: String) -> String = fs.read_text(p) ?? ""
```

`main` も同じ規則に従う。`main` が実際のエフェクトを行わない場合、通常の `fn main() -> Unit` が正しい。読み取り、ファイル書き込み、ネットワーク、`fan`、または `!` による伝播を行う場合だけ `effect fn main()` と書く。

テスト：`tests/checker_test.rs`（`pure_fn_admits_output_and_abort_builtins`、`pure_fn_still_rejects_a_real_read_with_e006`）、`spec/lang/pure_fn_output_builtins_test.almd`、`spec/wasm_cross/pure_fn_output_and_assert_abort.almd`（C-153）。

<a id="22-stdlib-readers-are-effect-fns"></a>

### 2.2 stdlib の読み取り関数は effect fn

*Dialect epoch 8（#3248）。* `io.read_byte`、`io.read_n_bytes`、`process` の argv リーダーは stdin または argv を読むため、`io.read_line` や `env.args` と同様に `effect fn` となった（argv は現在 `env.args` だけにある）。以前は通常の `fn` で、純粋 fn がそれらを通じて外界を読めた。現在は純粋 fn からの呼び出しは E006 になる。バイトリーダーはその後、`Read` ハンドル（`io.stdin()`、`net.TcpStream`、`unix.Fd`）上の `io.read`、`io.read_exact`、`io.read_to_end` に置き換わり、これらも同様に `effect fn` である。

*Dialect epoch 13。* `args` モジュールのリーダー（`args.has_flag`、`args.option`、`args.positional`）も同じく argv を読むため、`effect fn` である（#848 の決定：argv はエフェクト）。呼び出し側を `effect fn` にするか、`effect fn main` で引数を読み値として渡す。上記のテンプレート経由のリーダーと異なり、これらは Almide 本体を持つので、呼び出しはユーザーの effect fn と同様 `Result[T, String]` 型になり、`!` で伝播する（`args.has_flag("verbose")!`）。

```almide check-fail=E006
import io

fn first_byte(h: io.Stdin) -> Int = match io.read(h, 1) {
  ok(b) => bytes.get(b, 0) ?? -1,
  err(_) => -1,
}
```

```almide check-fail=E006
import args

fn verbose() -> Bool = args.has_flag("verbose")
```

テスト：`tests/diagnostics/e006-io-read-in-pure-fn/`、`tests/diagnostics/e006-io-read-exact-in-pure-fn/`、`tests/diagnostics/e006-process-args-in-pure-fn/`、`tests/diagnostics/e006-args-*-in-pure-fn/`（各 `args` リーダーに一つ）。

<a id="23-pure-the-empty-effect-set-e092"></a>

### 2.3 `@pure`：空のエフェクト集合（E092）

*#3250 で追加。* fn の `@pure` は検査され、その fn とそこから呼ぶすべてに、エフェクトカテゴリ（§8）、出力、宣言された中断がないことを求める。fn が `effect fn` または `@extern` の場合、あるいは到達先が §2.1 の六つの組み込み、カテゴリ付き stdlib 関数、`@extern` fn のいずれかを呼ぶ場合は E092。エラーはそこへ至る呼び出しに位置し、経路を示す。

fn 型の仮引数経由の呼び出しは実引数が記述された場所で判定するため、`@pure fn apply(f: (Int) -> Int, x: Int) -> Int = f(x)` は純粋である。言語が定義するトラップ（ゼロ除算、範囲外インデックス）は宣言された中断ではなく、数えない。

```almide check
@pure
fn area(w: Int, h: Int) -> Int = w * h

fn main() -> Unit = println("${area(2, 3)}")
```

```almide check-fail=E092
@pure
fn area(w: Int, h: Int) -> Int = {
  println("w=${w}")
  w * h
}
```

ADR-0027（草案）は、この意味を持つ `pure fn` 修飾子を導入し、属性をそれに置き換えて廃止することを提案している。`@pure` は同じ集合を意味するため、両者が食い違うことはない。

テスト：`tests/diagnostics/e092-pure-calls-println/`、`tests/diagnostics/e092-pure-reaches-assert-via-helper/`、`tests/diagnostics/e092-pure-calls-categorised-stdlib/`、`tests/diagnostics/e092-pure-on-effect-fn/`。

<a id="24-effect-fns-reached-as-values-e006-e109"></a>

### 2.4 値として到達する effect fn（E006、E109）

*#4151 で追加。* §1 の保証は、値として到達する effect fn（コールバック、レコードフィールド、リスト要素、別の関数が作ったクロージャー）にも適用される。二つの規則がこれを担う（ADR-0009 D2、ADR-0026 D1）：

1. **引数として effect fn を名指すことはエフェクト呼び出しである。** 修飾なしの fn 型仮引数は透過的である。`apply(fs.read_text, p)` は `apply` が実行するものを実行するため、この呼び出しはエフェクト呼び出しとなり、純粋 fn からは E006 になる。単純な名前（`list.map(xs, eff)`）もモジュールパス（`list.map(xs,
   fs.read_text)`）も同様。明示的な `effect (A) -> B` スロットは例外で、effect fn を要求し、呼べるのはエフェクトコンテキストだけである。
2. **エフェクトビットは値に伴う。** 関数値が `effect fn`、またはそれを呼ぶクロージャーの場合、そのビットを持ち、どこに保存されても保持する。純粋 fn の本体がそのビットを持つ値を実行すると E109 となり、純粋 fn から effect fn への経路を示す。ただし、自身の fn 型仮引数を介す場合は除く。それは規則 1 により各呼び出し箇所で計上する。

```almide check-fail=E109
import fs

type Reader = { read: (String) -> Result[String, String] }

// E109: `main` stored fs.read_text in the field; this pure fn runs it.
fn pure_looking(r: Reader) -> Result[String, String] = (r.read)("s.txt")

effect fn main() -> Unit = {
  let r = Reader { read: fs.read_text }
  println(pure_looking(r) ?? "none")
}
```

```almide check-fail=E006
import fs

fn apply(f: (String) -> Result[String, String], p: String) -> Result[String, String] = f(p)

// E006: `apply` is transparent, so this call reads the file.
fn sneaky(p: String) -> Result[String, String] = apply(fs.read_text, p)
```

`effect fn` からなら両方のプログラムを受理する。`effect fn reads(r: Reader)` はフィールドを実行でき、effect fn 内の `apply(fs.read_text, p)` はほかと同じエフェクト呼び出しである。`process.exit` は `Never` を返すが、それでもエフェクト関数であり、純粋な呼び出し側は E006 で拒否する。これは ADR-0022 の例外となる六つの prelude 組み込みではなく、能力台帳はその中断を計上する。

解析は型検査後に全プログラムを対象とし、すべての経路（`check`、`build`、`run`、`test`、両ターゲット）で保守的に行う。追跡対象位置から出た値は、同じシグネチャのほかのすべての値とプールされる（§8、*コールバック*）。そのため、リストから読んだ関数を呼ぶ純粋 fn は、そのシグネチャで effect fn を実行する関数がプログラム内のどこかのリストに入った場合に拒否される。`almide check` はエントリーファイル自身の関数を判定し、モジュール関数は全モジュールを lower するビルド経路で判定する。

テスト：`tests/diagnostics/e109-*`、`tests/diagnostics/e006-effect-fn-path-to-user-hof/`、`tests/diagnostics/e006-effect-fs-remove-as-value/`、`tests/effect_value_laundering_test.rs`。

<a id="3-return-type-wrapping"></a>

## 3. 戻り値型のラッピング

検査済みソースインターフェースは既に、決してエラーにならない intrinsic 宣言を除き、エフェクト呼び出しを失敗しうるものとして扱う。ソースの `!` は明示的で、共有フロントエンドの lowering がその操作を型付き IR に記録する。native 表現では、`effect fn` の戻り値型が既に `Result` でなければコード生成中に `Result[T, String]` へ引き上げる。`ResultPropagationPass` がこの変換を行う：

1. effect fn が `-> T` を宣言し T が Result でなければ、コード生成は `-> Result[T, String]` へ書き換える
2. 本体の末尾式を `ok(...)` で包む
3. 既に Result の戻り値型（例：`-> Result[Int, String]`）は変更しない

```almide
// Source: returns String
effect fn greet(name: String) -> String = "hello ${name}"

// After ResultPropagationPass (Rust codegen): returns Result<String, String>
// Body becomes: Ok("hello ${name}".to_string())
```

型検査器は `constrain_effect_body` を通じてこの柔軟性を扱い、以下を受け付ける：

- `Unit` 本体（guard による制御フローの return）
- ラップされていない `T`（自動で `ok(T)` に包む）
- 完全な `Result[T, E]`（そのまま通す）

テスト：`spec/lang/effect_fn_test.almd` — `safe_div`、`require_positive`

<a id="4-explicit-propagation-the--operator"></a>

## 4. 明示的な伝播：`!` 演算子

Almide は `!` 演算子を、エラー伝播を伴う明示的な Result/Option の取り出しに使う。`effect fn` 内では、`expr!` が値を取り出し、エラーを呼び出し側へ伝播する。

```almide
effect fn add_strings(a: String, b: String) -> Result[Int, String] = {
  let x = int.parse(a)!   // unwrap or propagate err
  let y = int.parse(b)!
  ok(x + y)
}
```

<a id="how-it-works"></a>

### 動作の仕組み

**型検査器（`infer.rs`）：** `Unwrap` 式は内側の型を取り出す。`Result[T, E]` は `T`、`Option[T]` は `T` になる。型注釈のない `let` 束縛の暗黙的な型絞り込みは ADR-0008（v0.55.0）で削除された。失敗しうる呼び出しはすべての位置で Result 値となり、従来暗黙的だった箇所は厳格なエラー E041/E042 になった。`let _ = f()` が認められた破棄方法である（C-217）。docs/specs/result-option-effect.md §3 参照。

**Lowering/コード生成：** フロントエンドは明示的な `!` を IR の `Unwrap` ノードとして記録する（通常値に対して許可される no-op は消去）。ターゲットの lowering/出力がチャネル依存の制御フローを実現する。native の `pass_result_propagation.rs` は Result シグネチャと呼び出し形を Rust に適合させ、構造的 Wasm 経路は独自エミッターを通じて共有の型付き IR を使う。どちらもソースレベルの自動伝播を復活させない。Result 値の match は暗黙的に取り出すことなく `ok`/`err` を調べる。

```almide
fn require_positive(n: Int) -> Result[Int, String] =
  if n > 0 then ok(n) else err("must be positive")

effect fn classify(s: String) -> Result[String, String] = {
  let n = int.parse(s)!           // ! propagates parse error
  let valid = require_positive(n)! // ! propagates validation error
  ok(if valid > 100 then "big" else "small")
}

test "each ! is a propagation point" {
  assert_eq(classify("500"), ok("big"))
  assert_eq(classify("-1"), err("must be positive"))
  assert_eq(classify("x"), err("invalid digit found in string"))
}
```

テスト：`spec/lang/effect_fn_test.almd` — `parse_num`、`add_strings`、`double_parsed`、`classify_str`

<a id="5-fan-blocks"></a>

## 5. `fan` ブロック

`fan` ブロックは式を並行実行する。コンパイル時に強制する制約が二つある：

<a id="e007-fan-requires-effect-context"></a>

### E007：`fan` はエフェクトコンテキストを必要とする

`fan` ブロックは `effect fn` または `test` ブロック内にだけ書ける。純粋な `fn` で `fan` を使うと E007 になる。

```almide check-fail=E007
effect fn compute_a() -> Result[Int, String] = ok(1)
effect fn compute_b() -> Result[Int, String] = ok(2)

// Compile error E007: fan block can only be used inside an effect fn
fn bad() -> (Int, Int) = fan { compute_a(), compute_b() }
```

`static_dispatch.rs` 経由の `fan.map()` / `fan.settle()` / `fan.min()` 呼び出しにも同じ検査を適用する。

<a id="e008-no-mutable-variable-capture"></a>

### E008：可変変数のキャプチャ禁止

`fan` ブロックは外側スコープの `var` 束縛をキャプチャできない。これによってデータ競合を防ぐ。キャプチャできるのは `let` 束縛だけである。

```almide check-fail=E008
effect fn use(n: Int) -> Result[Int, String] = ok(n)

effect fn bad() -> Result[Unit, String] = {
  var count = 0
  // Compile error E008: cannot capture mutable variable 'count' inside fan block
  let (a, b) = fan { use(count), use(count) }
  ok(())
}
```

<a id="type-behavior"></a>

### 型の振る舞い

`fan` ALL ブロック / `fan.settle` はタプルを返す（Result ラッパーはなく、取り出すものもない）。Result を返す形式（mapper、`fan.min`）は ADR-0008 に従う。Result は値であり、束縛では伝播を明示する（`let v = fan.map(xs, f)!`）。テスト：`spec/wasm_cross/fan_any_early_winner.almd` と 0.55 切り替え時に移行した fan fixture。

```almide
effect fn add(a: Int, b: Int) -> Result[Int, String] = ok(a + b)
effect fn mul(a: Int, b: Int) -> Result[Int, String] = ok(a * b)

effect fn example() -> Result[Unit, String] = {
  let (sum, product) = fan {
    add(3, 4)    // Result[Int, String] -> Int
    mul(3, 4)    // Result[Int, String] -> Int
  }
  // sum: Int, product: Int
  assert_eq(sum, 7)
  assert_eq(product, 12)
  ok(())
}

test "the tuple carries the unwrapped values" {
  assert_eq(example(), ok(()))
}
```

<a id="determinism-contract-c-004"></a>

### 決定性契約（C-004）

`fan` の並行 API は**決定的核**を契約として持つ(native ⇄ wasm byte-identical、
契約台帳 C-004、fixture: `spec/wasm_cross/fan_deterministic.almd`,
`spec/wasm_cross/fan_pure_thunks.almd`):

- thunk リストを取る呼び出し形 `fan.race(thunks)` / `fan.any(thunks)` /
  `fan.settle(thunks)` は撤去済み(E027 トンボストーン)。旧 `fan.race(thunks)` は
  決定的モデルの下では `thunks[0]()` そのもので、名前だけが言語にないタイミング意味論を
  約束していた。
- `fan.min(compute.X(n)) { }` / `fan.min(compute.X(n), xs, f)` — 現行の形。時計引数は
  必須(型が「何で測って最小か」を言う)。腕は純粋(E006)で、ソース順に一本ずつ
  実行され、予算内で決定的な計算量が最小の Ok を選ぶ(同量ならソース順で先のもの)。
  (Go の select が仕様で RANDOM を要求するのと対照的な設計選択。)
  `fan.min(duration.X(n)) { }` — 実時間で最初に成功した腕(oracle 層、I/O 可、負けた腕は
  次の区切りで止めて待つ)は決定済み(ADR-0028)で、#3788 で入る。
- 順に試して最初の Ok は fan ではない: `a() ?? b() ?? 既定値`(`??` の右辺は左辺が
  失敗したときだけ評価される)、リストなら `list.find_map(xs, (x) => f(x)?) ?? 既定値`。
  旧 `fan.any { }` / `fan.any(xs, f)` は方言 epoch 14 で撤去(E027 が置換先を示す)。
- `fan.settle { }` / `fan.settle(xs, f)` — 全要素の Result。どちらもターゲットを問わず
  逐次に展開される(ブロックはタプル、マッパーは `list.map`)。
- `fan.timeout(duration.X(n)) { }` — 純粋な本体に壁時計の期限を課す oracle 層の形
  (本体に I/O を許すのは決定済み、#3788)。
  旧呼び出し形 `fan.timeout(ms, f)` は E027(C-006)。

テスト: `tests/fan_acceptance_test.rs`(4 レッグで stdout 一致を固定)。
全形の現状と既知の食い違いは `docs/design/fan-semantics-audit.md`。

<a id="historical-thunk-adapters-not-a-list-of-current-public-forms"></a>

### 歴史的な thunk アダプター（現在の公開形式の一覧ではない）

旧 thunk API の実装では `race` / `any` / `settle` の thunk は純粋(`fn() -> T`)でも effect
(`fn() -> Result[T, String]`)でもよい — 非 Result thunk は FanLowering が
両ターゲット共通で `Ok` アダプタに包む(v0.27.3+、#514)。
**`fan.map` だけは mapper が明示的に Result を返す必要がある**
(`(x) => ok(x * 10)`)— mapper の戻り型推論は thunk と違い困難なため、
純粋 mapper は check 時に拒否される(意図的な非対称、診断改善は #547)。

テスト：`spec/lang/fan_test.almd`、`spec/lang/fan_map_test.almd`、`spec/wasm_cross/fan_pure_thunks.almd`、`spec/wasm_cross/fan_block_err_list_order.almd`（C-199）、`spec/wasm_cross/fan_sibling_trap.almd`（C-200）

<a id="6-test-blocks"></a>

## 6. `test` ブロック

テストブロックは暗黙的にエフェクトコンテキスト内にある。検査器はテスト本体に入る際に `can_call_effect = true` を設定し、lowerer は `is_effect: true` のテスト関数を出力する。

このためテストブロックでは次が可能である：

- エフェクト関数を直接呼ぶ
- `fan` ブロックを使う
- `!` 演算子で値を取り出す

```almide
import fs

effect fn load_config() -> Result[String, String] = ok("port=8080")

test "calls effect functions directly" {
  let content = load_config()!
  assert(string.len(content) > 0)
  assert_eq(fs.exists("/nonexistent/almide-doctest"), false)
}
```

テスト関数は `Result` ではなく `Unit` を返す。テスト内から呼ばれた effect fn は明示的な Result 値を返す。`!` で取り出す（err 時はテストを panic させる）か、`??` / ok/err の match で消費する。docs/specs/result-option-effect.md §5 参照。

テスト：`spec/lang/effect_fn_test.almd` — すべての test ブロックがエフェクト関数を直接呼ぶ

<a id="7-cross-module-effects"></a>

## 7. モジュールをまたぐエフェクト

エフェクト関数のシグネチャはモジュール境界をまたいでも保持する。モジュール A が `effect fn` をエクスポートすると、A を import するモジュールはそれを呼べるが、エフェクトコンテキストからに限る。

```almide project
// file: effectlib/mod.almd
effect fn read_config() -> Result[String, String] = ok("config_value")
fn pure_() -> String = "pure"
// file: main.almd
import effectlib

effect fn main() -> Unit = {
  let config = effectlib.read_config()!  // ok: effect context — the Result is a value, `!` propagates
  assert_eq(config, "config_value")
  assert_eq(effectlib.pure_(), "pure")   // pure fn also callable
}

test "the imported effect fn keeps its signature" {
  assert_eq(effectlib.read_config(), ok("config_value"))
  assert_eq(effectlib.pure_(), "pure")
}
```

`is_effect` フラグは `TypeEnv.functions` に保存する関数シグネチャの一部で、モジュールインターフェースを通じて伝播する。呼び出し箇所の E006 検査はローカル関数と import 関数で同じように働く。

テスト：`spec/integration/modules/vis_effect_test.almd`

<a id="8-permissions"></a>

## 8. 権限

`almide.toml` の `[permissions]` セクションは、パッケージが使えるエフェクトカテゴリを制限する。これは Almide の能力システムのセキュリティ第 2 層である。

<a id="configuration"></a>

### 設定

```toml
[permissions]
allow = ["FS.read", "IO", "Net.fetch"]
```

`[permissions]` がない、または `allow` キーがなければ、能力は検査しない（後方互換）。明示的な **`allow = []` は何も許可しない**（#4101）。能力に到達するすべての関数が能力違反となり、`allow = []` はマニフェストで「このプログラムは何にも触れない」と表す方法である。以前はキーがない場合と同じ解釈で、すべてを許可していた。出力も数える。`println` は `IO.stdout`（下表）なので、通常の `fn` が `println` を呼べても（ADR-0022）、`allow = []` の下では出力するプログラムを拒否する。出力だけを許可するには `IO.stdout` を付与する。

`almide.toml` は TOML として読むため、`allow` は任意の配列表記を使える。一行、複数行、末尾カンマ、項目間のコメントも可（#3253）。以前は複数行配列を空として読み、すべてを許可していた。

<a id="effect-categories"></a>

### エフェクトカテゴリ

カテゴリは ADR-0026 D2 の閉じた二層階層である（#4155）。推論と `[permissions]` は**葉**で扱う。マニフェストは**親**を許可でき、その意味はここでその下に列挙する葉である。後で追加した葉が自動で親に加わるわけではない。

| 親 | 葉 | 該当する操作 |
|--------|--------|-------------------|
| `FS` | `FS.read`, `FS.write` | `fs`：読み取りと一覧は `FS.read`、書き込み・削除・名前変更・作成は `FS.write`（`fs.copy` は両方） |
| `Net` | `Net.fetch`, `Net.listen` | `http` の `effect fn` は `Net.fetch`、`http.serve` は `Net.listen`。`net.tcp_connect` / `unix_connect` は `Net.fetch`、listen/accept 関数は `Net.listen`。開いたストリームまたは共有メモリ領域に対する操作は両方（どちら側が開いたか分からないため） |
| `IO` | `IO.stdin`, `IO.stdout`, `IO.stderr` | `io` のリーダーは `IO.stdin`、`io.print` / `write` / `write_bytes` と `println` は `IO.stdout`、`eprintln` は `IO.stderr` |
| `Env` | `Env.read`, `Env.write` | `env` と `args` は読み取り（`Env.read`）、`env.set` は書き込み（`Env.write`）、`process.pid` は読み取り |
| — | `Proc` | プロセスの起動とシグナル送信：`process.exec` など、`kill`。親からは付与されない。起動したコマンドはほかのすべての能力に到達できる |
| — | `Time` | 時計：`datetime.now` / `now_ms` / `monotonic_ns`、`env.sleep_ms` |
| — | `Rand` | `random` |
| — | `Fan` | `fan` |
| — | `Abort` | `panic`、失敗した `assert*`、`process.exit`（`panic` と失敗した assert は `IO.stderr` にも書く） |

`STDLIB_MODULE_EFFECTS`（`crates/almide-ir/src/effect.rs`）内のモジュール行が既定値になる。操作が異なる関数は、その宣言に `@category("FS.write")` のように独自の葉を宣言する。表の隣の gate は、行がない stdlib モジュール、ランタイムシンボルの分類が宣言元関数と異なる `@intrinsic`（#3246）、存在する葉を指定していない `@category`、分類済みモジュールのカテゴリなし `effect fn` を拒否する。

`path`、`url`、`json`、`regex`、`zlib`、コレクションと数値処理は何も持たない。**`@extern` fn の呼び出しはすべての葉**（⊤、#3245）となる。本体が外部コードなので、`allow` が十四の葉すべてを許可する場合だけ `[permissions]` を通る。**stdlib 外の `@inline_rust` fn もすべての葉**（#4100）となる。native ビルドは Almide 本体の代わりに Rust テンプレートを差し込むため、推論が読む本体は実行されるコードではない。stdlib 自身のテンプレートは上の呼び出し表で分類する。

**Dialect epoch 15 以降**、`IO` はファイルを含まない（`FS` に属する）。`Env` はプロセス起動を許可しない（`Proc` に属する）。出力・中断組み込みには `IO.stdout` / `IO.stderr` / `Abort` が付く。旧名では許可されたはずの葉を拒否する場合、新しい名前を示す `note:` を出力する。

`[permissions].allow` で `Proc` を許可し、`proc = [...]` コマンド一覧（ADR-0025）がない場合、あらゆるコマンドを起動できるため全経路で警告し、`almide check --profile critical` ではエラーにする。

テスト：`tests/effect_permissions_test.rs`、`tests/proc_category_test.rs`。表の gate は `crates/almide-ir/src/effect.rs` の `classification_gate` モジュール。

<a id="unknown-names"></a>

### 未知の名前

`allow` が受け付けるのは厳密に四つの親と十四の葉である。それ以外の名前は、大文字小文字が異なるもの（`io`）も含め、コマンドを実行する前にマニフェストの記載行でエラーにする（#3247）：

```
error: almide.toml:6: unknown capability `Fil` in [permissions].allow — grantable capabilities are FS, Net, IO, Env, FS.read, FS.write, Net.fetch, Net.listen, IO.stdin, IO.stdout, IO.stderr, Env.read, Env.write, Proc, Time, Rand, Fan, Abort
  hint: did you mean `FS`?
```

ヒントは最も近い能力名を示す。`almide check --profile
critical --allow` が未知の名前を報告する方法と同じ（`docs/specs/cli.md`）。

テスト：`tests/manifest_permissions_test.rs`、`tests/manifest_toml_reader_test.rs`、`tests/diagnostics/permissions-unknown-capability/`、`tests/diagnostics/permissions-unknown-capability-multiline/`

<a id="enforcement"></a>

### 強制方法

コード生成後、コンパイラは `EffectInferencePass` を実行し、不動点反復により関数ごとの推移的エフェクト（直接の stdlib 呼び出しと、呼び出す関数のエフェクト）を計算する。いずれかの関数が `allow` にない能力を使うと、能力違反エラーでビルドが失敗する。

```almide project build-fail=capability
// file: almide.toml
[package]
name = "permdemo"
version = "0.1.0"

[permissions]
allow = ["IO"]
// file: main.almd
import http

// This function uses Net.fetch (http.get) -> build error
effect fn fetch() -> Result[String, String] = http.get("https://example.com")
//   error: capability violation in `fetch`
//     Net.fetch is not in [permissions].allow
```

すべての経路が gate を適用する。`almide check`、`build`、`run`、`emit`、`almide test` の両 lane が両ターゲットで対象となる。マニフェストはエントリーファイルのプロジェクト、つまりエントリーファイルのディレクトリまたはその祖先で最も近い `almide.toml`、なければ作業ディレクトリのものを使う。`check` はエントリーファイルだけでなくすべてのユーザーモジュールを判定する。

テスト：`tests/permissions_routes_test.rs`。

<a id="directory-scope-fs"></a>

### ディレクトリスコープ：`fs`

`[permissions] fs` はプログラムが触れてよいディレクトリを列挙する（#4022）：

```toml
[permissions]
fs = ["./data", "/tmp"]
```

相対項目はプロジェクトのディレクトリであり、相対 `path` 依存と同様に `almide.toml` のディレクトリを基準とする。プログラムを開始するディレクトリによってスコープは移動しない。

- **省略**（`fs` キーなし）：従来どおりすべてのパス。これが既定なので、現在動くプログラムが動かなくなることはない。`fs = []` はどのパスも許可しない。
- **表記ではなく解決先。** OS が到達する場所でパスを判定する。作業ディレクトリを基準に絶対化し、存在する最長接頭辞の各シンボリックリンクと `..` を OS が解決し、残りを字句的に適用する。すべての許可ディレクトリの外に到達する、またはリンク先のないシンボリックリンクを通るパスは、触れる前に拒否する。`fs.exists`、`fs.is_dir`、`fs.is_file`、`fs.is_symlink` は `false`、その他すべての `fs` 呼び出しは `err("fs: \"<path>\" is outside [permissions] fs")` を返す。no-follow 呼び出しもパスの到達先で判定するため、許可ディレクトリ内にあって外を指すリンクは問い合わせも削除もできない。`copy` と `rename` は両パスを、temp 呼び出しは作成先 temp ディレクトリを、`glob` はパターンのリテラル基底を検査する。
- **Native**（`almide run`、`almide build`、`almide test`）：ビルドはプログラムから fs ランタイムへの各呼び出しを検査で包む。ルートを埋め込み、最初の fs 呼び出しで解決し、ランタイム関数のスコープ付き twin を呼ぶ（`src/cli/fs_scope_twin.rs`：同じ本体で `std::fs` 呼び出しをスコープ経由にする）。ランタイム自体は変更しない。テストハーネスのビルドには `main` がなく、同じ方式で制限する（#4084）。
- **Wasm、埋め込みホスト**（`almide run --target wasm`、`almide test` の wasm leg）：同じソース（`crates/almide-rt-core/src/fs_scope.rs`）による同じ検査がホストの一つの fs import の前に置かれ、ホストの fs 呼び出しも同じスコープ付き core を経由する。そのため両ターゲットは同じパスを同じバイト列で拒否する。p3 component（full-response http 群）として動くプログラムは許可ディレクトリを WASI preopen として受け取る。キーがなければ作業ディレクトリと temp ディレクトリであり、もはや `/` ではない。この場合の拒否は WASI 自身のエラーになる。
- **Wasm、標準ランタイム**：`almide build --target wasm` の成果物が到達できるのは、運用者が preopen した範囲（`wasmtime --dir`）であり、マニフェストは成果物に付随しない。

検査が判断するのはパスの到達先だけである。native と埋め込みホストでは、その後の操作を、到達先を含む許可ディレクトリの descriptor の下で実行する。descriptor は初回使用時に一度開く（`crates/almide-rt-core/src/fs_scoped_core.rs`、#4084）。確定した各セグメントを、その descriptor からシンボリックリンクを辿らずに走査する。Linux の 64/32 ビットでは openat2 `RESOLVE_BENEATH | RESOLVE_NO_SYMLINKS`、macOS では `O_NOFOLLOW` の openat 走査、Windows では保持したディレクトリハンドルを基準に `FILE_OPEN_REPARSE_POINT` 付き `NtCreateFile` を使う。Windows ではすべての reparse point（シンボリックリンク、junction）をシンボリックリンクと同様に拒否する（#4293）。最後の要素は親 descriptor から開く・作成する・一覧する・削除する・名前変更する。検査と呼び出しの間にツリーが書き換えられた場合（ディレクトリがシンボリックリンクに置き換わった場合）、呼び出しは同じ `err` で拒否し、決して辿らない。許可ディレクトリ自体への操作は保持した descriptor 上で、または親の項目が保持ディレクトリであることを確認した後に親の descriptor から行う。プロセスが開けない項目のメタデータも descriptor で取得する。Linux、macOS、Windows 以外ではすべてのスコープ付き呼び出しを拒否し、ツールチェーンはビルド前にキーを拒否する。p3 component の境界は preopen であり、ランタイム（cap-std 上の wasmtime-wasi）が呼び出し時に preopen 済み descriptor の下で解決する。プログラム内では `fs.open_root`（`docs/stdlib/fs.md`）が、このスコープに加えて、信頼できない入力から作ったパスを一つのディレクトリへ制限する。その呼び出しはすべての leg で descriptor によりシンボリックリンクを拒否する（#4084）ので、書き換えとの競合を防ぐ。

既定を反転して、`fs` キーの省略がプロジェクトディレクトリだけを意味するようにすると、現在プロジェクト外を読むすべてのプログラム（絶対設定パス、`$HOME` のファイル、`/dev/stdin`）を拒否することになる。その変更には dialect epoch 登録、拒否時の `fs = ["/"]` ヒント、`almide init` によるキー出力が必要であり、この変更には含まれない。

テスト：`tests/fs_scope_test.rs`

<a id="callbacks-the-set-rides-the-value-adr-0026-d1"></a>

### コールバック：集合は値に伴う（ADR-0026 D1）

クロージャーのカテゴリが実行されるのは、それが**呼ばれる**場所であり、集合は値とともに移動する。クロージャーの作成は何も実行しない。集合は計算して解き、記述はしない：

- **修飾なしの fn 型仮引数は透過的。** 渡されたクロージャーを呼ぶ関数（`f(x)` または `xs |> list.map(f)`）は、推測した呼び出し先ではなく仮引数として報告する。クロージャーを渡す各呼び出し箇所にその集合を計上する。`apply(paths, (p) => …fs…)` は `IO` を持ち、`apply(names, (s) => string.len(s))` は何も持たない。
- **保存または返却した値も集合を保持する**：`let`/`var` ローカル、レコードフィールド（そこに保存する全クロージャーの和集合）、戻り値、トップレベル `let`。
- それ以外の場所（リスト、option の payload、variant、未知の呼び出し先の引数）に置くクロージャーは、そのシグネチャのプールに加わる（alias は展開し、`effect (A) -> B` スロットは carrier 形式にする）。そこから読んだ値の呼び出しには、そのプールを計上する。型がシグネチャを未確定にする読み取り（型変数）には、別名になりうるすべてのプールを計上する。ジェネリック仮引数に渡した値は呼び出し箇所の具体型でプールする（#4151）。
- 本体がランタイムにある stdlib 関数（`@intrinsic`、`= _`）は、各 fn 型引数を実行し、保持しない。型変数の仮引数で受け取る値はプールし、結果は呼び出しの型におけるプールから読む。#4152 より前はその本体を「何もしない」と読んでいたため、`list.map(paths, fs.read_text)` はファイルシステム読み取り能力を計上しなかった。現在の葉の語彙では `FS.read` を計上しなければならず、stdlib fn を値として扱ってもそのカテゴリを保持する。

```
  call_box  → {FS.read}
  make  → {} (effect fn); returns a closure doing {FS.read}
  use_it  → {} + whatever f (arg 1) does
  main  → {FS.read, IO.stdout} (effect fn)
```

`[permissions].allow` はこれらの集合を検査するため、違反はクロージャーを**実行する**関数名を示し、カテゴリごとに一つの経路を付ける（D4）。コールバックを通る段階は仮引数名、続いて判明していれば具体的なクロージャーを示す。複数の値が流れうる場合（仮引数の別の呼び出し箇所、プールされた値）は、次の段階に `e.g.` を付ける：

```
error: capability violation in `main`
  FS.read is not in [permissions].allow
  path: main → use_it → f (arg 1 of use_it) → closure (line 3:42 in make) → fs.read_text (line 3:49)
```

集合は `Ty::Fn` ではなくエフェクト解析（`crates/almide-codegen/src/effect_flow/`）に存在する。型検査後に計算し、レポートとともに破棄するため、単相化はこれを見ない。表面構文は関わらない。

テスト：`tests/effect_category_flow_test.rs`、`tests/effects_report_callbacks_test.rs`

<a id="native-code-from-dependencies-native-4093"></a>

### 依存先のネイティブコード：`native`（#4093）

上記のカテゴリは Almide コードから推論する。依存先のネイティブコード（`native/*.rs`、`[native-deps]` crate とその `build.rs`、`@extern(rust|c)` fn）は、cargo がプログラムをビルドする間、その推論の前かつ外側で動く。`[permissions] native` はそれを持ち込んでよい依存パッケージを列挙する：

```toml
[permissions]
native = ["nat"]
```

`build`、`run`、`test`、`bench` はすべてのターゲットで許可されていないものを拒否する。ルートパッケージ自身のネイティブコードには許可不要（[package-system.md §6.4](./package-system.ja.md#64-native-code-from-a-dependency-needs-a-grant-permissions-native)）。

<a id="design-layers"></a>

### 設計の層

- **第 1 層**（実装済み）：`effect fn` と `fn` — 型検査器が強制する
- **第 2 層**（実装済み）：almide.toml の `[permissions].allow` — パッケージごとに stdlib 能力を制限する
- **第 3 層**（将来）：利用側が依存先の能力を制限する

テスト：`crates/almide-codegen/src/pass_effect_inference.rs` 内のエフェクト推論単体テスト（`#[cfg(test)]` モジュール）

<a id="9-error-codes"></a>

## 9. エラーコード

| コード | 名前 | 発生条件 | 修正 |
|------|------|---------|-----|
| E006 | エフェクト分離違反 | 純粋な `fn` が `effect fn` を呼ぶ | 呼び出し側を `effect fn` にする。Result を既に消費（`!` / `??` / `?` / `match`）していない呼び出しには `!` も付ける。ヒントは両手順を示し、同じ式の `expected T but got Result[T, String]` は別途報告しない（#3515、テスト：`tests/e006_fallible_cascade_test.rs`） |
| E007 | 純粋関数内の fan ブロック | エフェクトコンテキスト外で `fan { ... }` を使用 | 囲む関数を `effect fn` にする |
| E008 | fan における可変変数キャプチャ | `fan` ブロックが `var` 束縛を参照 | `var` を `let` に変更するか、`fan` の前で値を `let` にコピーする |
| E109 | 値を通じた effect fn の実行 | 純粋な `fn` が、値として到達した `effect fn`（フィールド、リスト要素、保存したクロージャー）を実行 | fn を `effect fn` にするか、I/O を行わない値を渡す |
| E092 | `@pure` fn が純粋でない | `@pure` fn が出力、中断、カテゴリ付き stdlib 呼び出し、または `@extern` に到達 | 呼び出しを呼び出し側へ移すか、`@pure` を外す |
