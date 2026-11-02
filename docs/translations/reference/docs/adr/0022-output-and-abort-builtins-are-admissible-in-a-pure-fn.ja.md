<a id="adr-0022-the-output-and-abort-builtins-are-admissible-in-a-pure-fn--writes-and-aborts-never-reads-and-they-do-not-make-the-caller-effectful"></a>

# ADR-0022: 出力と中断の組み込み関数は pure fn 内で許可される — 書き込みと中断であり、読み取りは行わず、呼び出し元を effectful にしない

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](0022-output-and-abort-builtins-are-admissible-in-a-pure-fn.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](0022-output-and-abort-builtins-are-admissible-in-a-pure-fn.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](0022-output-and-abort-builtins-are-admissible-in-a-pure-fn.zh-CN.md)

> [英語の正本](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0022-output-and-abort-builtins-are-admissible-in-a-pure-fn.md)（固定コミット 35297a612fb29e3f78f6587aba253aa91d51f466）の非規範的な履歴参照用スナップショット。正本が優先され、この資料は自動同期されない。以下の Accepted 状態、測定、実装に関する主張は原文の記録であり、現時点で独立して検証した結果ではない。記載のコマンドは実行していない。

- **状態**：Accepted（既存の動作を記録するもの。チェッカーやコード生成の変更はない）
- **日付**：2026-09-27
- **範囲**：`println`、`eprintln`、`panic`、`assert`、`assert_eq`、`assert_ne` を通常の `fn` から呼び出せるか。その呼び出しが、呼び出し元の effect、伝播、`fan`、サーブされるハンドラ、参照評価器、native と wasm の出力バッファリングに対して何を意味するか。
- **関連**：[effect-system.md §2.1](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/effect-system.md)（規範ルール）、[ADR-0002](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0002-fallibility-effect-orthogonal.md)（effect 軸）、[ADR-0008](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0008-explicit-propagation-only.md)（伝播は `!` のみ）、[ADR-0011](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0011-execution-substrate-is-a-free-variable.md) D1（fan アームの出力）、[ADR-0015](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0015-reference-evaluator-is-fresh-source-level-python.md)（参照評価器）、[ADR-0020](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0020-http-serving-and-handler-state.md) §5.5 / §7.2（並行サービングとログ行）、契約 C-004、C-153、C-162、C-367。
- **経緯**：2026-09-27。このルールが明文化されていなかったため、ドキュメントには `main` の両方の書き方（`effect fn main() -> Unit = println(…)` と `fn main() -> Unit = println(…)`）が示され、読者はどちらが必要なのか判断できなかった。ユーザーには、A（pure fn で出力と中断の組み込み関数を許可し、そのルールを記述する）、B（これらに `effect` を必須にする）、C（中断のみ許可する）の 3 案が提示された。ユーザーは **A** を選んだ。

<a id="context"></a>

## 背景

<a id="what-the-checker-does-today-almide-0640-measured"></a>

### 現在のチェッカーの動作（almide 0.64.0、測定済み）

- `check_named_call_with_type_args` はシグネチャを検索する前に組み込み関数を解決する（`crates/almide-frontend/src/check/calls.rs:437-440`）。組み込み関数の分岐（`check_builtin_output`、`crates/almide-frontend/src/check/builtin_calls.rs:62`）は引数の型を制約し、`Unit`（`println`、`eprintln`、assert 系）または `Never`（`panic`）を返す。`can_call_effect` は一切読み取らないので、これらの名前では `calls.rs:514` の E006 判定（`sig.is_effect && !self.env.can_call_effect`）に到達しない。
- したがって、`fn show(p: Int) -> Unit = println("p=${p}")` はチェックに通り実行でき、`fn main() -> Unit = println(…)` も同様。同じ位置の `fs.read_text` は E006 になり、`io.print` も同様。`io.print` は標準ライブラリの *effect fn* であり、組み込み関数ではない。
- 既存コーパスはすでにこれに依存している。`spec/`、`stdlib/`、`research/benchmark/exercises/` には pure な `fn main` を持つファイルが 535 個、`effect fn main` を持つファイルが 386 個ある。`spec/wasm_cross/` では 792 個の fixture 中 486 個が pure な `fn main` を使う。
- `crates/almide-frontend/src/stdlib.rs:305` には今も `["println", "eprintln", "panic"]` を返す `builtin_effect_fns()` 関数がある。そのドキュメントコメントは「組み込み effect 関数」と記している。呼び出し元はなく、この名前は本 ADR と矛盾する。

<a id="survey-how-peers-treat-print-and-abort-in-pure-code"></a>

### 調査：他言語は pure なコードでの出力と中断をどう扱うか

出典は一次資料であり、`almide-references/` 配下のローカルクローン（Koka、Roc、Gleam、Swift、Rust）と公式言語ドキュメント（Go、Haskell）を用いた。

| 言語 | 型で effect を追跡するか | pure なコードからの実出力 | デバッグ出力の例外 | pure なコードからの中断 | 一次資料 |
|---|---|---|---|---|---|
| Rust | いいえ | はい：どこでも `println!` | `dbg!`（stderr、値を返す） | はい：`panic!` | `library/std/src/macros.rs`（`println` :143、`eprintln` :221、`dbg` :358。:296 の `dbg!` のドキュメント例は通常の `fn factorial`） |
| Go | いいえ | はい：どこでも `fmt.Println` または組み込み `println` | 組み込み `print`/`println` はブートストラップ用で、「言語に残り続けることは保証されない」 | はい：`panic` | go.dev/ref/spec §Bootstrapping |
| Haskell | はい（`IO`） | いいえ：`putStrLn :: String -> IO ()` | `Debug.Trace.trace :: String -> a -> a`、pure な型。「本番コードでは使うべきでは*ない*」 | はい：`error :: HasCallStack => [Char] -> a` は pure な bottom | hackage `base` Prelude、`Debug.Trace` |
| Koka | はい（effect row） | いいえ：`println(s) : console ()` | `trace(message) : ()` は total（`unsafe-total` 経由）。`notrace()` で無効になる | `throw : exn a` は追跡される。`assert` と `impossible` は total として型付けされる（`unsafe-abort` 経由） | `lib/std/core/console.kk:78`、`debug.kk:58-60, 81, 84`、`exn.kk:49`、`core.kk:55, 79` |
| Gleam | いいえ | はい：どこでも `io.println(string) -> Nil` | `echo`（1.9）はモジュールと行を添えて stderr に出力する。`gleam publish` は `echo` が残っているパッケージを拒否する | はい：`panic`、`todo`、`assert` | `changelog/v1.9.md:32, 99`、`compiler-cli/src/publish.rs:489-508`、標準ライブラリ `gleam/io.gleam:42` |
| Roc | はい（`->` は pure、`=>` は effectful、推論される） | いいえ：「effectful な関数を呼び出せるのは別の effectful な関数だけ」 | `dbg` と `expect` は pure な関数内で許可される。「プログラムの意味論の一部ではないため、その副作用は effectful な関数の外で許可される」 | はい：`crash`。pure な関数は「*total* である保証はない」 | `docs/langref/functions.md:25-42`、`expressions.md:113-117`、`statements.md:138-151` |
| Swift | いいえ（`throws`/`async` のみ） | はい：どこでも `print(_:separator:terminator:)` | 別枠はない（`print`、`debugPrint`） | はい：`fatalError(…) -> Never`、`precondition`、`assert` | `stdlib/public/core/Print.swift:54`、`Assert.swift:275-278` |

この表から分かること：

1. **すべての言語で pure なコードからの中断が許可される。** effect を追跡する 3 言語でも同様で、Haskell の `error` は pure な bottom、Roc の `crash` は pure なコード内で許可され、Koka は `assert` と `impossible` を total として型付けする。Koka が追跡するのは回復可能な `throw`（`exn`）だけ。中断はプログラムの観測を終了させ、新たな観測を追加しないので、これらの言語はいずれも中断を I/O として扱わない。
2. **effect を追跡するすべての言語がデバッグ出力を例外にする**（`trace`、`dbg`）。理由も同じで、出力はプログラマー向けでありプログラムの意味論ではないため。これらの例外は書き込み専用で、pure なコードが何かを*読み取る*ことは一切許可しない。
3. **effect を追跡しない言語は出力に制限を設けない。**

Almide はこの 2 系統の中間に位置する。`println` はトレースチャネルではなく実際のプログラム出力なので、Almide の例外は Haskell の `trace` や Roc の `dbg` より広い。しかし、読み取り（`fs`、`env`、`io.read_line`、`http`、時計、`random`）は引き続き effect によって制限される。effect を追跡しない言語はこの制限を設けない。

<a id="decision"></a>

## 決定

**`println`、`eprintln`、`panic`、`assert`、`assert_eq`、`assert_ne` は pure な `fn` 内で許可される。これらは stdout や stderr への書き込み、またはプログラムの中断であり、読み取りは一切行わない。呼び出しても呼び出し元は effectful にならず、伝播するものも生じない。** 規範本文は [effect-system.md §2.1](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/effect-system.md)。本 ADR はその理由を記録する。

<a id="d1-the-exempt-set-is-closed-and-names-builtins-not-modules"></a>

### D1. 例外集合は閉じており、モジュールではなく組み込み関数を指定する

上記の 6 つの名前が集合のすべて。`io.print`、`io.write`、`io.write_bytes` と `log` モジュールは標準ライブラリの effect fn であり、pure fn から呼ぶと引き続き E006 になる。この集合に名前を追加するには本 ADR の改訂が必要。候補は、`Unit` または `Never` を返す、書き込み専用か中断用の組み込み関数でなければならない。

<a id="d2-no-effect-no-propagation"></a>

### D2. effect も伝播もない

組み込み関数は `Unit`（`println`、`eprintln`、assert 系）または `Never`（`panic`）を返し、`Result` は返さない。`!` で取り出すものはない。`println(s)!` はどの文脈でも E034 になる。これらを呼ぶ pure fn は、シグネチャ、モジュールインターフェース、すべての呼び出し箇所で pure のまま。effect fn はその関数を、`!` なしで通常の値として呼び出す。

<a id="d3-reads-remain-the-boundary"></a>

### D3. 読み取りが引き続き境界となる

pure fn は引き続き外界を観測できない。例外となる組み込み関数はどれもプログラム外の情報を返さない。そのため pure fn の*結果*は引き続き引数の関数である。例外によって加わるのは*観測*、すなわち stdout や stderr のバイト列と、場合によってはプロセスの終了である。

<a id="d4-interactions-each-is-stated-normatively-in-effect-systemmd-21"></a>

### D4. 相互作用（それぞれ effect-system.md §2.1 に規範として記述）

- **サーブされるハンドラ（ADR-0020）。** pure なヘルパー経由で出力するハンドラにも、直接出力する場合と同じルールが適用される。各呼び出しのバイト列はストリームに連続して到達する（行はアトミック）。1 つのリクエストからの行はプログラム順序を保ち、処理中の異なるリクエスト間の行順序は未規定。
- **`fan` の本体。** `fan` アームから呼ばれた pure fn の出力は、そのアームの一部として行われる。この例外は fan の契約を超える順序保証を追加しない（C-004。native では `fan.settle` のサンク副作用の交錯は契約対象外）。`fan` 自体には依然として effect 文脈が必要（E007）。ADR-0011 D1（アーム順の出力）が実装された時点で、これらのバイト列にも他の出力と同様に適用される。
- **参照評価器。** どちらの評価器にも、これらの名前に対する effect 文脈はない。プロセス内インタープリター（`almide-interp`）は名前でディスパッチする（`crates/almide-interp/src/dispatch.rs:393`）。judge の参照評価器（ADR-0015）はこれらを `PRELUDE` として列挙する（almide/als `ref/src/eval.rs:217-224`）。そのリストは D1 の 6 名と完全に一致する。したがって 3 者間オラクルは、pure fn の出力を effect fn の出力と同じように比較する。pure/effect の区別を強制するのはチェッカーだけ。
- **バッファリング。** pure fn の出力は effect fn と同じストリームを通る。native の stdout は 1 つのバッファで、stdout が端末なら書き込みごとにフラッシュされ、それ以外は 64 KiB のバッファリングを行う。stderr はバッファリングしない（C-162、および C-367 の記述）。契約はストリームごとに適用され、stdout のバイト列、stderr のバイト列、終了コードは native と wasm でバイト単位に一致する。両ストリームを 1 ファイルに流す場合の交錯順序は契約の対象外（測定では、`println; eprintln; println` を `2>&1` で統合すると、native では `o1 e1 o2`、wasm では `e1 o1 o2` となった）。中断は、それ以前に書かれた stdout をフラッシュしてから、自身の stderr ブロックを書き込む（C-153）。
- **`scoped`。** 変更なし。`scoped fn` は pure fn より厳格で、その内部の `println` は引き続き E087。`@bounded` は C-316 の既存の記述どおり、出力の組み込み関数を許可する。

<a id="d5-which-main-the-docs-teach"></a>

### D5. ドキュメントで教える `main`

`main` が実際の effect を実行しない場合は、通常の `fn main() -> Unit` が正しい。`main` が読み取り、ファイル書き込み、ネットワーク利用、`fan` の利用、または `!` による伝播を行う場合にだけ `effect fn main()` と書く。LLM 向けドキュメント（`docs/CHEATSHEET.md`、`llms.txt`）はこの形で教え、1 行で明記する。

<a id="rationale"></a>

## 理由

- **言語の既存の動作だから。** 案 A は動作を一切変えず、コーパスの 535 ファイルがこれに依存する。B はそれらすべてを E006 にし、出力以外の effect を実行しないプログラムに `effect` キーワードを加えることになる。情報を持たないキーワードを加えるとプログラムの変更が難しくなり、modification survival に反する。
- **中断はどの言語でも例外だから（調査の項目 1）。** pure fn で `assert` を拒否すると、Almide は表の中で唯一そうする言語になる。また、「pure なヘルパー内で不変条件をチェックする」という定型的なパターンも妨げる。
- **pure なコードが呼び出し元に与える保証は、出力ではなく入力に関するものだから。** 「pure fn の結果は引数の関数である」は A のもとでも成り立つ（D3）。チェッカーの本質的な価値は読み取りとホストアクセスの制限にあり、ホストに到達するすべてのモジュールは引き続き制限される。
- **pure なコードでのデバッグ出力は普遍的だから（調査の項目 2）。** 表の effect を追跡する言語は、いずれもこのための抜け道を追加する必要があった。Almide に 2 つ目のトレース専用組み込み関数は不要で、`eprintln` がすでに stderr に出力し、stdout と混ざらない。

<a id="alternatives-rejected"></a>

## 却下した代案

- **B：出力に `effect` を必須にする**（Haskell と Koka の `console`）。理論上は厳格になるが、コーパスの 535 ファイルと pure な `main` を持つすべてのドキュメント例を壊す。この方法を選んだ effect 追跡言語も結局トレースの例外を追加したため、結果は案 A に組み込み関数を 1 つ加えたものになる。
- **C：中断のみ許可し、出力には `effect` を必須にする。** 出力について B と同じコストを負い、pure なヘルパーをデバッグする最も一般的な方法を失う。
- **`println` を許可せず、トレース専用組み込み関数（`dbg`/`trace`）を用意する。** LLM が選ぶ出力の書き方が 2 つになり、既存のすべての pure な `main` には依然として `effect` が必要になる。Gleam は公開時にだけデバッグ出力の制約を強制しており、この種のルールは型付けではなくツールの問題だと示している。

<a id="consequences"></a>

## 結果

- ルールを明文化し固定した。チェックテストは `tests/checker_test.rs`（`pure_fn_admits_output_and_abort_builtins`、`pure_fn_still_rejects_a_real_read_with_e006`）。実行 fixture は `spec/lang/pure_fn_output_builtins_test.almd` と `spec/wasm_cross/pure_fn_output_and_assert_abort.almd`。後者は C-153 の証拠であり、native と wasm でバイト単位に一致しなければならない。
- ドキュメントは `main` に関する 1 つのルールを教える（D5）。
- コスト：pure fn の呼び出し回数と呼び出し順序が出力を通して観測可能になる。そのため、出力や中断の可能性がある pure な呼び出しを、最適化器は重複排除、巻き上げ、並べ替えしてはならない。したがって pure fn は自由にメモ化できない。これは、任意の pure fn で `panic` が実行され得たため、もともと成り立っていた。

<a id="falsifier"></a>

## 反証条件

次のいずれかが観測されたら、この決定を撤回する（B またはトレース専用組み込み関数へ移行する）：

1. pure/effect の分離によって可能にするはずの最適化（pure な呼び出しの CSE、巻き上げ、メモ化）が、実際には出力の組み込み関数によって妨げられることが、性能コーパスで測定される。
2. LLM が書くプログラムが、modification survival を損なう形で pure なヘルパーから出力することが Dojo の測定で示される（例：pure な呼び出しが移動すると出力順が変わる）。「より厳格なほうが原則的だ」というだけでは反証条件にならない。

<a id="references"></a>

## 参考資料

- Rust：`library/std/src/macros.rs`（クローン：`almide-references/rust`）
- Go：<https://go.dev/ref/spec#Bootstrapping>
- Haskell：<https://hackage.haskell.org/package/base/docs/Debug-Trace.html>、<https://hackage.haskell.org/package/base/docs/Prelude.html>
- Koka：`lib/std/core/{console,debug,exn,core}.kk`（クローン：`almide-references/koka`）
- Gleam：`changelog/v1.9.md`、`compiler-cli/src/publish.rs`（クローン：`almide-references/gleam`）
- Roc：`docs/langref/{functions,expressions,statements}.md`（クローン：`almide-references/roc`）
- Swift：`stdlib/public/core/{Print,Assert}.swift`（クローン：`almide-references/swift`）

---

歴史的な他言語比較と測定コーパス件数を保持しており、独立に再確認したものではない。表題は観測可能な作用の不在や自由なメモ化を保証せず、原文は出力／中断を明示的に許可し最適化を制約する。参照評価器の説明とアーム順序に関する将来形も原文どおり保持する。
