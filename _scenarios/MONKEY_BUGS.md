# Monkey Test で発見されたバグ (almide 0.11.3 → 0.11.4)

日付: 2026-04-05
範囲: `_scenarios/monkey01..15_*_test.almd` (言語機能・stdlib の広域探索)

現在の状態: **199 passed / 0 failed** (spec/ + _scenarios/ 全域)
monkey テスト単独: **179 passed / 0 failed** (15 ファイル)

以下のバグは本 monkey test で発見され、**すべて修正済み**。各テストファイルには
かつて壊れていたパターンが直接書かれており、回帰として機能する。

---

## 1. 【致命】単一ケース inline variant のパーサ誤解釈 ✅ 修正済

**症状**:
```almide
type PatientId = PatientId(String)   // 同名: 型解決が無限ループ
type Box[T] = Box(T)                 // 同名ジェネリック: 同上
type PatientId = Wrap(String)        // 異名: 未定義型 `Wrap` 参照で codegen 崩壊
```

`almide check` は "No errors" で通るのに `almide test` が CPU 100% でハング。
AST を見ると `type PatientId = PatientId` の **type alias** として解釈されていた。

**原因**: `crates/almide-syntax/src/parser/types.rs` の `parse_type_expr_inner` が
`Name(Args)` を見たとき `args` を捨てて `Ty::Simple { name }` を返していた。

**修正**: `try_parse_inline_variant` に常時フォールバックし、trailing `|` がなくても
単一ケース variant として扱う。既存の leading `|` 書式もそのまま動く。

**回帰**: `_scenarios/monkey01_inline_variant_test.almd`, `scenario6_newtype_test.almd`

---

## 2. 【致命】ジェネリック関数を呼び出すテストが silent drop される ✅ 修正済

**症状**:
```almide
fn wrap_all[T](xs: List[T]) -> List[Box[T]] =
    xs |> list.map((x) => Box(x))

test "wrap_all" {                 // ← エラーなしに codegen から消える
    let boxed = wrap_all([1, 2, 3])
    ...
}
```

**原因**: `lower_test` がテスト関数を `name = "wrap_all"` (テスト名そのまま) として
lower。同名のジェネリック関数 `wrap_all[T]` が monomorphization の filter に
引っかかり、テスト関数も一緒に `retain` で削られていた。さらに `rewrite_calls` の
`fn_param_types` HashMap でもテストとジェネリック関数が同じキーで衝突し、テスト側
(空 params) で上書きされて generic dispatch が崩れていた。

**修正**: 
- `mono/mod.rs` retain に `if f.is_test { return true; }` を追加
- `mono/rewrite.rs`・`mono/discovery.rs` のルックアップに `!f.is_test` フィルタ

**回帰**: `_scenarios/monkey10_generics_test.almd`

---

## 3. 【高】構造的に同一な nominal 型が unify される ✅ 修正済

**症状**:
```almide
type Dog = { name: String }
type Cat = { name: String }
let d: Dog = { name: "rex" }   // codegen: let d: Cat = Cat {...} に化ける
```

**原因**: `canonicalize/resolve.rs` の `resolve_type_expr` が `known_types` から
`Dog` を引くとき、登録された structural form (`Ty::Record { fields }`) をそのまま
返していた。これで型検査器の内部表現から nominal identity が失われ、Dog/Cat が
同一視されていた。

**修正**: user-declared record/variant/open record は `Ty::Named(name, [])` のまま
返し、structural 展開は on-demand (`resolve_named`) に任せる。primitive aliases
(`type Score = Int`) は従来通り透過的に展開する。protocol 実装の signature 比較は
両側で `resolve_named` してから比較するように合わせた。

**回帰**: `_scenarios/monkey15_protocols_test.almd`

---

## 4. 【高】プロトコル境界ジェネリックの dispatch が壊れる ✅ 修正済 (#3 と同根)

**症状**: `fn describe[T: Animal](a: T) -> String = "${a.species()}"` の
monomorphization で `T_species(a)` という未定義関数が codegen される。

**原因**: #3 と同じ。bindings マップの値が `Ty::Record { fields }` になっており、
`ty_to_name` が None を返すため、`T.species` → `Dog.species` の書き換えが起きない。
Dog が nominal Ty::Named として維持されるようになった結果、`ty_to_name(Dog)` が
`"Dog"` を返し、正しく `Dog.species` に書き換わるようになった。

**回帰**: `_scenarios/monkey15_protocols_test.almd`

---

## 5. 【中】レコード部分デストラクチャで `..` が付かない ✅ 修正済

**症状**: `let { name, age } = user` (User は 3 フィールド) で codegen が
`let User { name, age } = user` を吐き、Rust の E0027 で失敗。

**修正**: `codegen/walker/statements.rs` でパターンのフィールド数とレコード型の
総フィールド数を比較し、不足していれば自動で `..` を付与する。付随して
`CodegenAnnotations` に `record_field_counts` を追加。

**回帰**: `_scenarios/monkey13_records_test.almd`

---

## 6. 【中】`list.map((x) => x)` の identity 最適化が Range→Vec の collect を剥がす ✅ 修正済

**症状**: `0..5 |> list.map((x) => x)` が codegen 上で `(0i64..5i64)` になり、
Range<i64> を Vec<i64> に代入する壊れた Rust を吐く。

**修正**: `pass_stream_fusion/fusion_rules.rs` の `try_eliminate_identity_map` で
引数が `IrExprKind::Range` の場合だけ最適化をスキップ。

**回帰**: `_scenarios/monkey04_list_ops_test.almd`

---

## 7. 【中】`Vec<(i64, String)>` の index アクセスで `.clone()` が付かない ✅ 修正済

**症状**: `list.enumerate(xs)[0]` の結果が非 Copy タプルでも `.clone()` なしで
代入され、Rust の move エラーになる (String / Record 単体では正しく付く)。

**修正**: `pass_clone.rs` の `needs_clone` に `Ty::Tuple(elems) => elems.iter().any(needs_clone)`
を追加。numeric タプル `(Int, Int)` は Copy なのでスキップされる。

**回帰**: `_scenarios/monkey04_list_ops_test.almd`

---

## 8. 【中】stdlib/module 関数を first-class 値として渡せない ✅ 修正済

**症状**: `list.map(xs, string.len)` が `E003: undefined variable 'string'`。

**修正**:
- `check/infer.rs`: `ExprKind::Member` で object が module 名かつ field が関数の
  場合、Ty::Fn を返すショートサーキット。
- `lower/expressions.rs`: 同パターンを eta 展開して `(x) => module.func(x)` の
  Lambda にする。結果として `list.map(xs, string.len)` が動く。

**回帰**: `_scenarios/monkey10_generics_test.almd`

---

## 9. 【低】E001 型エラーが無関係な行に出る (診断位置の破壊) ✅ 修正済

**症状**: 制約ベースの型検査器が `Constraint` にスパンを保持しておらず、
`solve_constraints` でエラーを出すとき `self.current_span` (= 最後に訪れた式の span)
を使っていたため、まったく関係ない行に E001 が出ていた。

**修正**: `Constraint` に `span: Option<Span>` を追加し、`constrain()` 時に
current_span を捕捉。`solve_constraints` ではエラー出力前に一時的に current_span を
constraint のスパンに差し替える。

**副次効果**: #10 (診断メッセージに別関数名) も同根だったため同時に解消。

**再現**: `/tmp/cascade.almd`

---

## 既知の残存事項

- 大文字始まり識別子 (`let MyValue = 42`) のパーサエラーメッセージは最初のエラー
  としては正しく報告されるが、以降の識別子使用箇所に副次エラーが出る場合がある。
  これは error recovery の話で、カスケード自体は #9 の修正で緩和された。
- `map.insert` は in-place 変異 (要 `var`) であり、immutable 版は `map.set`。
  ドキュメントどおりで食い違いはなかった (当初の報告は当方の誤解)。

---

## まとめ

この monkey test セッションで発見したバグ **9 件をすべて修正**。各バグには
回帰テストが `_scenarios/` に残してあり、今後の変更で同じ穴が空いた瞬間に検出できる。

修正の大半は「中途半端な抽象化が nominal identity を壊していた」「エラー発生時の
状態が次のコードに漏れていた」という構造的な問題。短期で 199/199 を維持したまま
これだけ直せたのは、各バグを 1 件ずつ分離したテストで切り分けて進めたおかげ。
