<a id="closure-codegen-cross-target-gaps"></a>

# クロージャー codegen のクロスターゲット差異

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](closure-codegen-cross-target-gaps.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](closure-codegen-cross-target-gaps.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](closure-codegen-cross-target-gaps.zh-CN.md)

> [英語の正本](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/done/closure-codegen-cross-target-gaps.md)（固定コミット `35297a612fb29e3f78f6587aba253aa91d51f466`）の非規範的な読者用スナップショット。元の文書が正本であり、この写しは自動同期されず、生成器・チェッカーの入力でもない。以下の日付、計画、完了状態、測定値、比較主張は当時の原文の記述を保持したもので、独立した再現や現行版についての検証ではない。コードブロックとコメントは原文のまま保持する。過去の収集用メタデータは、動作しない読者向け注記として表示する。

> 過去のメタデータ：説明 — 敵対的差分調査で発見したクロスターゲット（native 対 wasm）のクロージャー codegen の差異 — 全 8 件修正。完了日 — 2026-06-02。

> 原文の注意点：「8 件すべて修正」とテスト数は当時の調査についての記述であり、末尾には cell 内オブジェクトのリークを含む構造上の未解決事項が残る。「良性」はリークとクラッシュを区別する原文の表現であり、一般的な安全保証ではない。PR 要約は 5 件、最初の表は 4 行に分類されているが、行を捏造して整合させず、そのまま保持する。Lean の証明の記述は記された cell 不変条件に限る。

> [Closure Architecture v2](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/done/closure-architecture-v2.md)（キャプチャの *cell*）に加え、敵対的なクロスターゲット差分調査の 2 回目（保存済み `p6-cross-target-sweep` workflow の再実行：多様なクロージャープログラムを機械生成し、`almide run` と `wasmtime` を比較し、網羅性を批評）が、クロージャーの**型付け／構文解析／保存**方法の差異を一群発見した。これはキャプチャ cell の仕組みとは独立である。方法は次のとおり：同一ソースでも、一方のターゲットで正しくコンパイル・実行できるプログラムが他方でエラー、trap、または静かな差異を起こせばバグである。キャプチャ／codegen を変える際はこの調査を再利用する。

状態：**完了** — 8 件すべてを修正し `develop` に取り込んだ。5 件は PR #334、深い 3 件（map に保存したクロージャー、匿名 record のクロージャーフィールド、クロージャー経由の非 Copy 要素の IndexAssign）は後続で対応した。それぞれの後に spec 240/240、cargo 791/0、調査の再実行がすべて問題なしとなった。

<a id="fixed-pr-334-develop"></a>

## 修正済み（PR #334、`develop`）

| 差異 | 症状 | 修正 |
|-----|---------|-----|
| WASM `string.push` / `string.clear` | WASM dispatch arm がない → emitter ICE（native は正常） | `emit_wasm/calls_string.rs` に arm を追加（push は `string_append` + `emit_mutator_writeback`、clear は len を 0 にする） |
| タプル内のクロージャー → `let (g,_)=…` | native `E0562`（束縛型内の impl-Trait） | let 型内に入れ子になった `Fn` 部分木を `_` に消去する（`erase_fn_types`、walker）。rustc が具体的なクロージャー型を推論する |
| `fs[0]()`（リストから添字で取り出したクロージャーを呼ぶ） | const-generic `fs::<0>()` と解析 → native `E0425` / wasm trap | `parser/helpers.rs peek_type_args_call` が括弧内に `TypeName` token を要求するようにした — `[0]` は添字、`[Int]` は型引数 |
| キャプチャ clone で包んだクロージャーの型付きパラメーター `(k: String)=>` | パラメーター注釈が落ちる（`move |k|`）→ `E0282` | walker Bind が `{ let __cap=…; move|k|… }` ブロックを通して末尾 lambda のパラメーターに注釈を付ける（HOF 引数 lambda は注釈なしのまま — `render_iter_chain` により inline で差し込まれる） |

検証済み：spec 240/240、cargo 784/0、クロスターゲット回帰テスト +5、parser の回帰なし。

<a id="also-fixed-the-3-deep-ones"></a>

## 追加で修正済み（深い 3 件）

<a id="closures-stored-in-a-map--e0308"></a>

### Map に保存したクロージャー → `E0308`

```almide
var m: Map[String, () -> Unit] = map.new()
map.insert(m, "a", () => { list.push(acc, 1) })
let g = map.get_or(m, "a", () => {})   // was: error[E0308] mismatched types
```

修正：`pass_rust_lowering` が、`Map[K, Fn]` に対する `map.insert` / `map.get_or` のクロージャー引数を `RcWrap` するようになった。map の値型は消去された `_` のままで、box 化した引数から単一の共通 `Rc<dyn Fn>` を推論する。これは `List[Fn]` リテラルと同じである（box 化した要素から `_` を推論する技巧）。そのため型の出力を変える必要はなかった。タプルは `_` 消去経路を維持する（各スロットは独立）。

<a id="closure-in-an-anonymous-record--e0277"></a>

### 匿名 record 内のクロージャー → `E0277`

```almide
let r = { run: () => { list.push(acc, 1) } }
r.run()                                  // was: error[E0277] (closure isn't Debug/PartialEq)
```

修正：ジェネリックな匿名 record の struct は `T: Clone + Debug + PartialEq` を要求していたが、クロージャーは後ろの二つを満たさない。クロージャーフィールドを持つ匿名 record は `Clone` だけを derive し、それらのジェネリック境界を外す（derive(Clone) が `T: Clone` を再追加する）。これは `type` 宣言の record にすでにある `has_fn_fields` の緩和と同じである。Fn フィールドのキーは `collect_anon_records` 中に追跡する。（`type` 宣言の record は以前から動作していた。）

<a id="indexassign-of-a-non-copy-element-through-a-closure--wasm-trap"></a>

### クロージャー経由の非 Copy 要素の IndexAssign → WASM trap

```almide
var xs: List[String] = ["a", "b"]
let f = () => { xs[0] = xs[0] + "!" }    // was: wasm trap (native "a!!")
```

根本原因：キャプチャした共有 cell は cell ptr への通常の `rc_inc` で `rc_inc` されるが、対応する `RcDec` は型付きの rc_dec を実行して cell ptr をリストとして走査していた。`cell[0]`（オブジェクト ptr）を要素数として読み、でたらめなアドレスを decref していた。`List[Int]`（Copy 要素で、要素 drop ループなし）は生き残ったが、`List[String]` は trap した。修正：`mutable_capture` の `RcDec` は cell に対する通常の `rc_dec` となり、`rc_inc` と一致するようになった（`inc == dec` — Lean で証明した cell の不変条件）。

<a id="fragility--elimination-targets-surfaced-while-fixing-the-above"></a>

## 脆弱な箇所／解消目標（上記の修正中に判明）

1. `emit_wasm/statements.rs` はリスト要素幅を、リストの宣言された要素型ではなく、値の型（`byte_size(value.ty)`）から導く — ずれを起こしやすい。未解決。
2. cell の deref が、単一の「一度 deref → 通常 ptr」という抽象化ではなく、操作ごと（Var read / IndexAccess / IndexAssign / 各 mutator / RcInc / RcDec）に再実装されている。未解決。
3. ~~parser の `[X]()` の曖昧さ~~ — 解消（#334）。
4. ~~クロージャーの box 化は `List[Fn]` リテラルだけを扱っていた~~ — `Map[K, Fn]`（insert/get_or）と匿名 record のクロージャーフィールドへ拡張した。`Set[Fn]` と一般的なコンテナー要素位置は今も `_` 推論の技巧に依存する。「均一なコンテナーに入るすべてのクロージャーを box 化する」単一の pass があれば、これらすべてを包含できる。未解決に近い。
5. ~~非 Copy 要素の `IndexAssign` が cell の deref と drop を連携させていなかった~~ — 上記の `RcInc`/`RcDec` の cell 一貫性で修正した。より深い問題は残る：cell の RC-drop は cell を解放するが、中のオブジェクトは解放しない（`List[Int]` も含めてすべての cell 型に共通するリーク）。cell には型付きデストラクターが必要である。未解決（良性：クラッシュでなくリーク）。

> 保持したコードコメントの意味：Map の例は以前 E0308 の型不一致、匿名 record の例はクロージャーが Debug/PartialEq でないため E0277、添字代入の例は以前 wasm が trap し native では "a!!" だったことを示す。
